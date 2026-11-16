use bevy::prelude::*;

use crate::mods::ModRegistry;
use crate::play::camera::hud::HudCamera;
use crate::play::characters::{CharacterActor, CharacterRole};
use crate::play::scene::PlayState;
use crate::play::stage::{DynamicScriptProp, StageCompositeProp, StagePropVisual, StageResource};
use crate::play::types::{PlayPhase, SongEventData};
use crate::ui::LoadingOverlayCamera;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::super::defs::{
    CharShaderRole, PHILLY_TRAIN_ERECT_LIGHT_COLORS, PHILLY_TRAIN_LIGHT_COLORS,
    SHADER_MODE_BUILDING, SHADER_MODE_DROP_SHADOW, SHADER_MODE_RAIN_SPRITE, SHADER_MODE_SSERAFIM,
    SHADER_MODE_WIGGLE, argb_to_rgba_f32,
};
use super::super::material::FunkinSpriteMaterial;
use super::super::rain_post::RainCameraEffect;
use super::commands::apply_shader_script_command;
use super::events::handle_song_shader_event;
use super::{
    ManagedSpriteShader, PendingShaderCommands, SSERAFIM_DUST_ADJUST, ShaderScriptCommand,
    ShaderTargetKind, StageShaderRuntime,
};

fn smooth_lerp_precision(base: f32, target: f32, dt_s: f32, duration_s: f32, precision: f32) -> f32 {
    if dt_s <= 0.0 || (base - target).abs() <= f32::EPSILON {
        return base;
    }
    let alpha = precision.powf(dt_s / duration_s);
    target + (base - target) * alpha
}

#[allow(clippy::too_many_arguments)]
pub fn update_stage_shaders(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mods: Res<ModRegistry>,
    stage_res: Option<Res<StageResource>>,
    asset_server: Res<AssetServer>,
    mut shaders: Option<ResMut<Assets<Shader>>>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    mut pending_cmds: ResMut<PendingShaderCommands>,
    mut managed: Query<(Entity, &mut ManagedSpriteShader)>,
    mut stage_props: Query<
        (Entity, &StagePropVisual, &mut Sprite, Option<&StageCompositeProp>),
        Without<DynamicScriptProp>,
    >,
    dynamic_props: Query<(Entity, &DynamicScriptProp)>,
    actors: Query<&CharacterActor>,
    mut cameras: Query<
        (Entity, Option<&mut RainCameraEffect>),
        (With<Camera2d>, Without<HudCamera>, Without<LoadingOverlayCamera>),
    >,
) {
    if state.needs_reset {
        let stage_id = state.chart.stage_id.clone();
        let song_id = state.chart.song_id.clone();
        runtime.reset_for_stage(&stage_id, &song_id);
        state.pending_shader_events.clear();
        for (_, item) in managed.iter() {
            if let Some(mut mat) = materials.get_mut(&item.handle) {
                if stage_id == "tankmanBattlefieldErect"
                    && item.target == ShaderTargetKind::Character(CharShaderRole::Opponent)
                {
                    mat.config.drop_shadow.use_alt_mask = false;
                }
                if stage_id == "sserafim" && (mat.config.mode_flags & SHADER_MODE_SSERAFIM) != 0 {
                    mat.config.sserafim.adjust = SSERAFIM_DUST_ADJUST;
                    mat.config.sserafim.darken_amount = 0.0;
                    mat.config.sserafim.pulse_strength = 0.0;
                    mat.config.sserafim.truck_strength = 0.0;
                }
                if (mat.config.mode_flags & SHADER_MODE_BUILDING) != 0 {
                    mat.config.building.alpha_shit = 1.0;
                }
                mat.time_s = 0.0;
                mat.sync_uniform();
            }
        }
        return;
    }

    let paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let dt = if paused { 0.0 } else { time.delta_secs() };

    if !paused {
        runtime.time_s += dt;
    }

    let events: Vec<SongEventData> = state.pending_shader_events.drain(..).collect();
    for event in &events {
        handle_song_shader_event(event, &mut runtime);
    }

    let has_stage_sidecar = stage_res.as_deref().is_some_and(|s| s.scripts);
    if !paused && (runtime.stage_id == "phillyTrain" || runtime.stage_id == "phillyTrainErect") {
        if has_stage_sidecar && runtime.stage_id == "phillyTrain" {
            for (_, prop, mut sprite, _) in &mut stage_props {
                if prop.name == "lights" {
                    runtime.philly_alpha_shit = (1.0 - sprite.color.alpha()).clamp(0.0, 1.0);
                    sprite.color.set_alpha(1.0);
                }
            }
        } else {
            let beat_s = (state.conductor.beat_length_ms() / 1000.0) as f32;
            runtime.philly_alpha_shit =
                (runtime.philly_alpha_shit + beat_s * dt * 1.5).min(1.0);
            let beat = state.conductor.current_beat;
            if beat != runtime.last_beat {
                runtime.last_beat = beat;
                if beat >= 0 && beat % 4 == 0 {
                    let palette = if runtime.stage_id == "phillyTrainErect" {
                        &PHILLY_TRAIN_ERECT_LIGHT_COLORS
                    } else {
                        &PHILLY_TRAIN_LIGHT_COLORS
                    };
                    runtime.philly_light_index = (runtime.philly_light_index + 1) % palette.len();
                    let [r, g, b, a] = argb_to_rgba_f32(palette[runtime.philly_light_index]);
                    for (_, prop, mut sprite, _) in &mut stage_props {
                        if prop.name == "lights" {
                            sprite.color = Color::srgba(r, g, b, a);
                        }
                    }
                    runtime.philly_alpha_shit = 0.0;
                }
            }
        }
    }

    if !paused && runtime.stage_id == "sserafim" {
        let beat = state.conductor.current_beat;
        if beat != runtime.last_beat {
            runtime.last_beat = beat;
            if runtime.sserafim.lights_enabled
                && beat >= 0
                && !runtime.sserafim.pulse_colors.is_empty()
                && !runtime.sserafim.pulse_durations.is_empty()
                && !runtime.sserafim.pulse_intensities.is_empty()
            {
                let b = beat as usize;
                let color =
                    runtime.sserafim.pulse_colors[b % runtime.sserafim.pulse_colors.len()];
                let dur =
                    runtime.sserafim.pulse_durations[b % runtime.sserafim.pulse_durations.len()];
                let int = runtime.sserafim.pulse_intensities
                    [b % runtime.sserafim.pulse_intensities.len()];
                runtime.sserafim.flash_back_light(int, f64::from(dur), color);
            }
        }
        runtime.sserafim.advance(f64::from(dt));
        for (_, prop, mut sprite, _) in &mut stage_props {
            match prop.name.as_str() {
                "truckLight1" => {
                    sprite.color = Color::srgba(1.0, 1.0, 1.0, runtime.sserafim.truck_strength);
                }
                "truckLight2" => {
                    sprite.color = Color::srgba(1.0, 1.0, 1.0, runtime.sserafim.truck2_alpha);
                }
                "backLightColor" => {
                    let [r, g, b] = runtime.sserafim.light_color;
                    sprite.color = Color::srgba(r, g, b, runtime.sserafim.pulse_strength);
                }
                "backLightWhite" => {
                    sprite.color =
                        Color::srgba(1.0, 1.0, 1.0, runtime.sserafim.white_pulse_alpha);
                }
                _ => {}
            }
        }
    }

    let commands_to_apply: Vec<ShaderScriptCommand> = pending_cmds.0.drain(..).collect();
    for cmd in commands_to_apply {
        apply_shader_script_command(
            cmd,
            &mut commands,
            &mods,
            &asset_server,
            shaders.as_deref_mut(),
            &mut materials,
            &mut runtime,
            &mut managed,
            &stage_props,
            &dynamic_props,
            &actors,
            &mut cameras,
        );
    }

    let mut actor_rot_flags = [false; 3];
    for actor in &actors {
        let idx = match actor.role {
            CharacterRole::Player => 0,
            CharacterRole::Spectator => 1,
            CharacterRole::Opponent => 2,
        };
        actor_rot_flags[idx] = actor.controller.current_frame_rotated();
    }

    for (_, item) in managed.iter() {
        let Some(mut mat) = materials.get_mut(&item.handle) else {
            continue;
        };
        let mut dirty = false;

        if (mat.custom_shader.is_some()
            || (mat.config.mode_flags & (SHADER_MODE_WIGGLE | SHADER_MODE_RAIN_SPRITE)) != 0)
            && (mat.time_s - runtime.time_s).abs() > f32::EPSILON
        {
            mat.time_s = runtime.time_s;
            dirty = true;
        }
        if (mat.config.mode_flags & SHADER_MODE_BUILDING) != 0
            && (mat.config.building.alpha_shit - runtime.philly_alpha_shit).abs() > f32::EPSILON
        {
            mat.config.building.alpha_shit = runtime.philly_alpha_shit;
            dirty = true;
        }
        if let ShaderTargetKind::Character(role) = item.target
            && (mat.config.mode_flags & SHADER_MODE_DROP_SHADOW) != 0
        {
            let ridx = match role {
                CharShaderRole::Player => 0,
                CharShaderRole::Spectator => 1,
                CharShaderRole::Opponent => 2,
            };
            let target_off = if actor_rot_flags[ridx] { -90.0 } else { 0.0 };
            if (mat.config.drop_shadow.angle_offset_deg - target_off).abs() > f32::EPSILON {
                mat.config.drop_shadow.angle_offset_deg = target_off;
                dirty = true;
            }
            if role == CharShaderRole::Opponent
                && item.char_id == "tankman-bloody"
                && mat.config.drop_shadow.use_alt_mask != runtime.tankman_alt_mask
            {
                mat.config.drop_shadow.use_alt_mask = runtime.tankman_alt_mask;
                dirty = true;
            }
        }
        if (mat.config.mode_flags & SHADER_MODE_SSERAFIM) != 0 {
            let ss = &mut mat.config.sserafim;
            if ss.adjust != runtime.sserafim.adjust
                || (ss.darken_amount - runtime.sserafim.darken_amount).abs() > 1e-5
                || (ss.pulse_strength - runtime.sserafim.pulse_strength).abs() > 1e-5
                || (ss.truck_strength - runtime.sserafim.truck_strength).abs() > 1e-5
                || ss.light_color != runtime.sserafim.light_color
            {
                ss.adjust = runtime.sserafim.adjust;
                ss.darken_amount = runtime.sserafim.darken_amount;
                ss.pulse_strength = runtime.sserafim.pulse_strength;
                ss.truck_strength = runtime.sserafim.truck_strength;
                ss.light_color = runtime.sserafim.light_color;
                dirty = true;
            }
        }
        if dirty {
            mat.sync_uniform();
        }
    }

    if let Some(spec) = runtime.rain_spec {
        if !paused {
            if spec.time_scale_decay {
                let hits = state.tallies.total_notes_hit;
                if hits > runtime.last_total_notes_hit {
                    let delta_hits = (hits - runtime.last_total_notes_hit) as f32;
                    runtime.rain_time_scale += 0.7 * delta_hits;
                }
                runtime.last_total_notes_hit = hits;
                runtime.rain_time_s += dt * runtime.rain_time_scale;
                runtime.rain_time_scale =
                    smooth_lerp_precision(runtime.rain_time_scale, 0.02, dt, 1.535, 0.01);
            } else {
                runtime.rain_time_s = runtime.time_s;
            }
        }

        let length_ms = state.chart.length_ms.max(1.0);
        let progress =
            (state.conductor.song_position_ms.max(0.0) / length_ms).clamp(0.0, 1.0) as f32;
        let intensity = spec.start_intensity + (spec.end_intensity - spec.start_intensity) * progress;

        let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;
        let half_w = (SCREEN_WIDTH * 0.5) / zoom;
        let half_h = (SCREEN_HEIGHT * 0.5) / zoom;
        let cx = state.camera.follow_point[0] as f32;
        let cy = state.camera.follow_point[1] as f32;
        let bounds = Vec4::new(cx - half_w, cy - half_h, cx + half_w, cy + half_h);
        let screen_and_scale = Vec4::new(SCREEN_WIDTH, SCREEN_HEIGHT, spec.scale, intensity);
        let time_and_color = Vec4::new(
            runtime.rain_time_s,
            spec.rain_color[0],
            spec.rain_color[1],
            spec.rain_color[2],
        );

        for (_, cam_fx) in &mut cameras {
            if let Some(mut fx) = cam_fx {
                if fx.bounds != bounds {
                    fx.bounds = bounds;
                }
                if fx.screen_and_scale != screen_and_scale {
                    fx.screen_and_scale = screen_and_scale;
                }
                if fx.time_and_color != time_and_color {
                    fx.time_and_color = time_and_color;
                }
            }
        }
    }
}
