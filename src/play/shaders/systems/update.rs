use bevy::prelude::*;

use crate::mods::ModRegistry;
use crate::play::camera::hud::HudCamera;
use crate::play::characters::behavior::CompositeRtCamera;
use crate::play::characters::{CharacterActor, CharacterRole};
use crate::play::scene::PlayState;
use crate::play::stage::schema::StageCoordMode;
use crate::play::stage::{DynamicScriptProp, StagePropVisual};
use crate::play::types::{PlayPhase, SongEventData};
use crate::play::PauseOverlayCamera;
use crate::ui::{LoadingOverlayCamera, SoundTrayOverlayCamera};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

use super::super::defs::{
    CharShaderRole, SHADER_MODE_BUILDING, SHADER_MODE_DROP_SHADOW, SHADER_MODE_RAIN_SPRITE,
    SHADER_MODE_WIGGLE,
};
use super::super::material::FunkinSpriteMaterial;
use super::super::rain_post::RainCameraEffect;
use super::super::stage_blend_post::{
    MAX_STAGE_POST_OVERLAYS, StageBlendPostImages, StageBlendPostUniform, StageOverlaySlot,
    StagePostOverlayProp,
};
use super::commands::{
    DynamicStagePropsQuery, StaticStagePropsQuery, apply_shader_script_command,
};
use super::events::handle_song_shader_event;
use super::{
    ManagedSpriteShader, PendingShaderCommands, ShaderScriptCommand, ShaderTargetKind,
    StageShaderRuntime,
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
    asset_server: Res<AssetServer>,
    mut shaders: Option<ResMut<Assets<Shader>>>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    mut pending_cmds: ResMut<PendingShaderCommands>,
    mut managed: Query<(Entity, &mut ManagedSpriteShader)>,
    mut stage_props: StaticStagePropsQuery,
    mut dynamic_props: DynamicStagePropsQuery,
    dynamic_sprites: Query<&Sprite, With<DynamicScriptProp>>,
    mut actors: Query<&mut CharacterActor>,
    mut cameras: Query<
        (Entity, Option<&mut RainCameraEffect>),
        (
            With<Camera2d>,
            With<IsDefaultUiCamera>,
            Without<HudCamera>,
            Without<PauseOverlayCamera>,
            Without<LoadingOverlayCamera>,
            Without<SoundTrayOverlayCamera>,
            Without<CompositeRtCamera>,
        ),
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

    if state.phase != PlayPhase::Loading {
        let mut pending_mats = std::collections::HashMap::new();
        let commands_to_apply: Vec<ShaderScriptCommand> = pending_cmds.0.drain(..).collect();
        for cmd in commands_to_apply {
            apply_shader_script_command(
                cmd,
                &mut commands,
                &mods,
                &asset_server,
                shaders.as_deref_mut(),
                &mut images,
                &mut materials,
                &mut runtime,
                &mut managed,
                &mut pending_mats,
                &mut stage_props,
                &mut dynamic_props,
                &mut actors,
                &mut cameras,
            );
        }
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

    for (entity, item) in managed.iter() {
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
        if (mat.config.mode_flags & SHADER_MODE_BUILDING) != 0 {
            let alpha_opt = stage_props
                .get(entity)
                .ok()
                .map(|(_, _, sprite, _)| sprite.color.alpha())
                .or_else(|| dynamic_sprites.get(entity).ok().map(|s| s.color.alpha()));
            if let Some(sprite_alpha) = alpha_opt {
                let target_alpha_shit = (1.0 - sprite_alpha).clamp(0.0, 1.0);
                if (mat.config.building.alpha_shit - target_alpha_shit).abs() > f32::EPSILON {
                    mat.config.building.alpha_shit = target_alpha_shit;
                    dirty = true;
                }
            }
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

/// Synchronizes stage post-process overlay layers (`Overlay`, `HardLight`, `SrgbAdd`)
/// onto the gameplay camera so they composite in OpenFL sRGB space after stage props
/// and characters.
#[allow(clippy::type_complexity)]
pub fn sync_stage_blend_post(
    mut commands: Commands,
    state: Res<PlayState>,
    images: Res<Assets<Image>>,
    overlays: Query<(&StagePropVisual, &Sprite, &StagePostOverlayProp)>,
    mut cameras: Query<
        (
            Entity,
            Option<&mut StageBlendPostUniform>,
            Option<&mut StageBlendPostImages>,
        ),
        (
            With<Camera2d>,
            With<IsDefaultUiCamera>,
            Without<HudCamera>,
            Without<PauseOverlayCamera>,
            Without<LoadingOverlayCamera>,
            Without<SoundTrayOverlayCamera>,
            Without<CompositeRtCamera>,
        ),
    >,
    mut warned_overflow: Local<Option<(String, usize)>>,
) {
    let mut active: Vec<(&StagePropVisual, &Sprite, &StagePostOverlayProp)> = if matches!(
        state.phase,
        PlayPhase::Loading
    ) {
        Vec::new()
    } else {
        overlays
            .iter()
            .filter(|(vis, sprite, _)| vis.visible && sprite.color.alpha() > 0.0001)
            .collect()
    };

    if active.is_empty() {
        *warned_overflow = None;
        for (cam_entity, u_opt, img_opt) in &mut cameras {
            if u_opt.is_some() || img_opt.is_some() {
                commands
                    .entity(cam_entity)
                    .remove::<(StageBlendPostUniform, StageBlendPostImages)>();
            }
        }
        return;
    }

    active.sort_by(|a, b| a.0.z_layer.total_cmp(&b.0.z_layer));
    if active.len() > MAX_STAGE_POST_OVERLAYS {
        let key = (state.chart.stage_id.clone(), active.len());
        if warned_overflow.as_ref() != Some(&key) {
            let dropped: Vec<&str> = active[MAX_STAGE_POST_OVERLAYS..]
                .iter()
                .map(|(vis, _, _)| vis.name.as_str())
                .collect();
            warn!(
                "Stage '{}' has {} active post-process blend overlays (max {}); dropping {:?}",
                state.chart.stage_id,
                active.len(),
                MAX_STAGE_POST_OVERLAYS,
                dropped,
            );
            *warned_overflow = Some(key);
        }
    } else {
        *warned_overflow = None;
    }
    let count = active.len().min(MAX_STAGE_POST_OVERLAYS);

    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;
    let cx = state.camera.follow_point[0] as f32;
    let cy = state.camera.follow_point[1] as f32;

    let mut uniform = StageBlendPostUniform {
        camera: Vec4::new(cx, cy, zoom, count as f32),
        screen: Vec4::new(SCREEN_WIDTH, SCREEN_HEIGHT, 0.0, 0.0),
        slots: [StageOverlaySlot {
            rect: Vec4::ZERO,
            params: Vec4::new(0.0, 0.0, 0.0, -1.0),
            tint_and_flip: Vec4::ZERO,
        }; MAX_STAGE_POST_OVERLAYS],
    };
    let mut ids = [None; MAX_STAGE_POST_OVERLAYS];

    for (idx, (vis, sprite, post_ov)) in active.into_iter().take(count).enumerate() {
        let raw_sz = if vis.raw_size[0] > 0.0 && vis.raw_size[1] > 0.0 {
            vis.raw_size
        } else {
            images
                .get(&sprite.image)
                .map_or([0.0, 0.0], |im| [im.width() as f32, im.height() as f32])
        };
        let world_w = raw_sz[0] * vis.base_scale[0];
        let world_h = raw_sz[1] * vis.base_scale[1];
        let top_left = match vis.coord_mode {
            StageCoordMode::TopLeft => vis.base_position,
            StageCoordMode::Center => [
                vis.base_position[0] - world_w * 0.5,
                vis.base_position[1] - world_h * 0.5,
            ],
        };
        let srgba = sprite.color.to_srgba();
        let flip_bits = (u32::from(sprite.flip_x) | (u32::from(sprite.flip_y) << 1)) as f32;
        uniform.slots[idx] = StageOverlaySlot {
            rect: Vec4::new(top_left[0], top_left[1], world_w, world_h),
            params: Vec4::new(
                vis.scroll_factor[0],
                vis.scroll_factor[1],
                srgba.alpha,
                post_ov.kind as u32 as f32,
            ),
            tint_and_flip: Vec4::new(srgba.red, srgba.green, srgba.blue, flip_bits),
        };
        ids[idx] = Some(sprite.image.id());
    }

    let next_images = StageBlendPostImages { ids };
    for (cam_entity, u_opt, img_opt) in &mut cameras {
        match (u_opt, img_opt) {
            (Some(mut cur_u), Some(mut cur_i)) => {
                if *cur_u != uniform {
                    *cur_u = uniform;
                }
                if *cur_i != next_images {
                    *cur_i = next_images.clone();
                }
            }
            _ => {
                commands
                    .entity(cam_entity)
                    .insert((uniform, next_images.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::defs::SpriteShaderConfig;
    use super::*;
    use crate::play::types::PlayStateParams;

    #[test]
    fn building_shader_syncs_sprite_alpha_generically() {
        let mut app = App::new();
        let mut state = PlayState::new(PlayStateParams::default());
        state.phase = PlayPhase::Playing;
        state.chart.stage_id = "customModStage".to_string();

        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Shader>()
            .init_asset::<FunkinSpriteMaterial>()
            .insert_resource(state)
            .insert_resource(ModRegistry::default())
            .init_resource::<StageShaderRuntime>()
            .init_resource::<PendingShaderCommands>()
            .add_systems(Update, update_stage_shaders);

        let handle = {
            let mut mats = app.world_mut().resource_mut::<Assets<FunkinSpriteMaterial>>();
            mats.add(FunkinSpriteMaterial::from_config(
                SpriteShaderConfig::building(1.0),
                None,
            ))
        };

        let prop_ent = app
            .world_mut()
            .spawn((
                StagePropVisual {
                    name: "lights".to_string(),
                    base_position: [0.0, 0.0],
                    base_scale: [1.0, 1.0],
                    scroll_factor: [1.0, 1.0],
                    base_flip: [false, false],
                    base_angle_deg: 0.0,
                    z_layer: 0.0,
                    visible: true,
                    coord_mode: StageCoordMode::Center,
                    raw_size: [100.0, 100.0],
                    velocity: [0.0, 0.0],
                    acceleration: [0.0, 0.0],
                },
                Sprite {
                    color: Color::srgba(1.0, 1.0, 1.0, 0.25),
                    ..default()
                },
                ManagedSpriteShader {
                    handle: handle.clone(),
                    target: ShaderTargetKind::StageProp,
                    char_id: String::new(),
                    prop_name: "lights".to_string(),
                    aux_path: None,
                },
            ))
            .id();

        app.update();
        {
            let mats = app.world().resource::<Assets<FunkinSpriteMaterial>>();
            let mat = mats.get(&handle).expect("material exists");
            assert!((mat.config.building.alpha_shit - 0.75).abs() < 1e-6);
            assert!((mat.uniform.time_and_extra.y - 0.75).abs() < 1e-6);
        }

        app.world_mut()
            .entity_mut(prop_ent)
            .get_mut::<Sprite>()
            .expect("sprite exists")
            .color
            .set_alpha(0.5);
        app.update();
        {
            let mats = app.world().resource::<Assets<FunkinSpriteMaterial>>();
            let mat = mats.get(&handle).expect("material exists");
            assert!((mat.config.building.alpha_shit - 0.5).abs() < 1e-6);
            assert!((mat.uniform.time_and_extra.y - 0.5).abs() < 1e-6);
        }

        app.world_mut()
            .entity_mut(prop_ent)
            .get_mut::<Sprite>()
            .expect("sprite exists")
            .color
            .set_alpha(0.0);
        app.update();
        {
            let mats = app.world().resource::<Assets<FunkinSpriteMaterial>>();
            let mat = mats.get(&handle).expect("material exists");
            assert!((mat.config.building.alpha_shit - 1.0).abs() < 1e-6);
            assert!((mat.uniform.time_and_extra.y - 1.0).abs() < 1e-6);
        }
    }
}
