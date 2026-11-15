use bevy::prelude::*;
use bevy::sprite_render::SpriteMaterial;

use crate::mods::ModRegistry;
use crate::play::camera::hud::HudCamera;
use crate::play::characters::{CharacterActor, CharacterRole};
use crate::play::scene::PlayState;
use crate::play::stage::{DynamicScriptProp, StagePropVisual};
use crate::play::types::{LinearTween, PlayPhase, SongEventData};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::easing::EaseKind;

use super::defs::{
    AdjustColorParams, CameraRainSpec, CharShaderRole, DropShadowParams,
    PHILLY_TRAIN_ERECT_LIGHT_COLORS, PHILLY_TRAIN_LIGHT_COLORS, SHADER_MODE_BUILDING,
    SHADER_MODE_DROP_SHADOW, SHADER_MODE_RAIN_SPRITE, SHADER_MODE_SSERAFIM, SHADER_MODE_WIGGLE,
    SpriteShaderConfig, WiggleEffectType, argb_to_rgba_f32, stage_camera_rain,
    stage_character_initial_tint, stage_character_shader, stage_prop_shader,
};
use super::material::FunkinSpriteMaterial;
use super::rain_post::RainCameraEffect;

/// Dust adjust-color preset set by `sserafim.hxc`'s `resetClear()` before `sserafimKick`.
const SSERAFIM_DUST_ADJUST: AdjustColorParams = AdjustColorParams::new(6.0, -74.0, -24.0, -26.0);
const SSERAFIM_CLEAR_DURATION_S: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderTargetKind {
    Character(CharShaderRole),
    StageProp,
    DynamicProp,
}

#[derive(Component, Debug, Clone)]
pub struct ManagedSpriteShader {
    pub handle: Handle<FunkinSpriteMaterial>,
    pub target: ShaderTargetKind,
    pub char_id: String,
    pub prop_name: String,
    pub aux_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShaderScriptCommand {
    SetAdjustColor {
        target: String,
        hue: f32,
        saturation: f32,
        brightness: f32,
        contrast: f32,
    },
    SetDropShadow {
        target: String,
        color: [f32; 3],
        angle_deg: f32,
        distance: f32,
        strength: f32,
        threshold: f32,
        hue: f32,
        saturation: f32,
        brightness: f32,
        contrast: f32,
        use_alt_mask: bool,
        alt_mask_path: Option<String>,
        mask_threshold: f32,
    },
    SetWiggle {
        target: String,
        effect_type: WiggleEffectType,
        speed: f32,
        frequency: f32,
        amplitude: f32,
    },
    SetCameraRain {
        scale: f32,
        intensity: f32,
        color: [f32; 3],
        time_scale_decay: bool,
    },
    ClearShader {
        target: String,
    },
}

#[derive(Resource, Default, Debug)]
pub struct PendingShaderCommands(pub Vec<ShaderScriptCommand>);

#[derive(Debug, Clone, Default)]
pub struct SserafimRuntimeState {
    pub adjust: AdjustColorParams,
    pub hue_tween: Option<LinearTween>,
    pub sat_tween: Option<LinearTween>,
    pub bright_tween: Option<LinearTween>,
    pub contrast_tween: Option<LinearTween>,
    pub darken_amount: f32,
    pub darken_tween: Option<LinearTween>,
    pub truck_strength: f32,
    pub truck_tween: Option<LinearTween>,
    pub truck2_alpha: f32,
    pub truck2_tween: Option<LinearTween>,
    pub pulse_strength: f32,
    pub pulse_tween: Option<LinearTween>,
    pub white_pulse_alpha: f32,
    pub white_pulse_tween: Option<LinearTween>,
    pub light_color: [f32; 3],
    pub lights_enabled: bool,
    pub pulse_colors: Vec<[f32; 3]>,
    pub pulse_durations: Vec<f32>,
    pub pulse_intensities: Vec<f32>,
}

impl SserafimRuntimeState {
    fn reset(&mut self, active: bool) {
        self.adjust = if active {
            SSERAFIM_DUST_ADJUST
        } else {
            AdjustColorParams::default()
        };
        self.hue_tween = None;
        self.sat_tween = None;
        self.bright_tween = None;
        self.contrast_tween = None;
        self.darken_amount = 0.0;
        self.darken_tween = None;
        self.truck_strength = 0.0;
        self.truck_tween = None;
        self.truck2_alpha = 0.0;
        self.truck2_tween = None;
        self.pulse_strength = 0.0;
        self.pulse_tween = None;
        self.white_pulse_alpha = 0.0;
        self.white_pulse_tween = None;
        self.light_color = [1.0, 1.0, 1.0];
        self.lights_enabled = false;
        self.pulse_colors.clear();
        self.pulse_durations.clear();
        self.pulse_intensities.clear();
    }

    fn advance(&mut self, dt_s: f64) {
        step_tween(&mut self.hue_tween, &mut self.adjust.hue, dt_s);
        step_tween(&mut self.sat_tween, &mut self.adjust.saturation, dt_s);
        step_tween(&mut self.bright_tween, &mut self.adjust.brightness, dt_s);
        step_tween(&mut self.contrast_tween, &mut self.adjust.contrast, dt_s);
        step_tween(&mut self.darken_tween, &mut self.darken_amount, dt_s);
        step_tween(&mut self.truck_tween, &mut self.truck_strength, dt_s);
        step_tween(&mut self.truck2_tween, &mut self.truck2_alpha, dt_s);
        step_tween(&mut self.pulse_tween, &mut self.pulse_strength, dt_s);
        step_tween(&mut self.white_pulse_tween, &mut self.white_pulse_alpha, dt_s);
    }

    fn start_darken(&mut self, amount: f32, duration_s: f64) {
        if duration_s <= 0.0 {
            self.darken_amount = amount;
            self.darken_tween = None;
        } else {
            self.darken_tween = Some(LinearTween::eased(
                f64::from(self.darken_amount),
                f64::from(amount),
                duration_s,
                EaseKind::SineInOut,
            ));
        }
    }

    fn flash_truck_lights(&mut self, amount: f32, duration_s: f64) {
        self.truck_strength = amount;
        self.truck2_alpha = amount;
        if duration_s <= 0.0 {
            self.truck_tween = None;
            self.truck2_tween = None;
        } else {
            self.truck_tween = Some(LinearTween::eased(
                f64::from(amount),
                0.0,
                duration_s,
                EaseKind::CubeInOut,
            ));
            self.truck2_tween = Some(LinearTween::eased(
                f64::from(amount),
                0.0,
                duration_s,
                EaseKind::CubeInOut,
            ));
        }
    }

    fn flash_back_light(&mut self, amount: f32, duration_s: f64, color: [f32; 3]) {
        self.light_color = color;
        let pulse_alpha = amount * 0.8;
        let white_alpha = amount * 0.7;
        self.pulse_strength = pulse_alpha;
        self.white_pulse_alpha = white_alpha;
        if duration_s <= 0.0 {
            self.pulse_tween = None;
            self.white_pulse_tween = None;
        } else {
            self.pulse_tween = Some(LinearTween::eased(
                f64::from(pulse_alpha),
                0.0,
                duration_s,
                EaseKind::CubeInOut,
            ));
            self.white_pulse_tween = Some(LinearTween::eased(
                f64::from(white_alpha),
                0.0,
                duration_s,
                EaseKind::CubeInOut,
            ));
        }
    }

    fn start_clear(&mut self) {
        self.hue_tween = Some(LinearTween::eased(
            f64::from(self.adjust.hue),
            0.0,
            SSERAFIM_CLEAR_DURATION_S,
            EaseKind::SineOut,
        ));
        self.sat_tween = Some(LinearTween::eased(
            f64::from(self.adjust.saturation),
            0.0,
            SSERAFIM_CLEAR_DURATION_S,
            EaseKind::SineOut,
        ));
        self.bright_tween = Some(LinearTween::eased(
            f64::from(self.adjust.brightness),
            0.0,
            SSERAFIM_CLEAR_DURATION_S,
            EaseKind::SineOut,
        ));
        self.contrast_tween = Some(LinearTween::eased(
            f64::from(self.adjust.contrast),
            0.0,
            SSERAFIM_CLEAR_DURATION_S,
            EaseKind::SineOut,
        ));
    }
}

#[derive(Resource, Debug)]
pub struct StageShaderRuntime {
    pub stage_id: String,
    pub song_id: String,
    pub attached: bool,
    pub time_s: f32,
    pub last_beat: i32,
    pub philly_light_index: usize,
    pub philly_alpha_shit: f32,
    pub rain_spec: Option<CameraRainSpec>,
    pub rain_time_s: f32,
    pub rain_time_scale: f32,
    pub last_total_notes_hit: u32,
    pub tankman_alt_mask: bool,
    pub sserafim: SserafimRuntimeState,
}

impl Default for StageShaderRuntime {
    fn default() -> Self {
        Self {
            stage_id: String::new(),
            song_id: String::new(),
            attached: false,
            time_s: 0.0,
            last_beat: i32::MIN,
            philly_light_index: 0,
            philly_alpha_shit: 1.0,
            rain_spec: None,
            rain_time_s: 1.0,
            rain_time_scale: 1.0,
            last_total_notes_hit: 0,
            tankman_alt_mask: false,
            sserafim: SserafimRuntimeState::default(),
        }
    }
}

impl StageShaderRuntime {
    fn reset_for_stage(&mut self, stage_id: &str, song_id: &str) {
        self.stage_id = stage_id.to_string();
        self.song_id = song_id.to_string();
        self.time_s = 0.0;
        self.last_beat = i32::MIN;
        self.philly_light_index = 0;
        self.philly_alpha_shit = 1.0;
        self.rain_spec = stage_camera_rain(stage_id, song_id);
        self.rain_time_s = 1.0;
        self.rain_time_scale = 1.0;
        self.last_total_notes_hit = 0;
        self.tankman_alt_mask = false;
        self.sserafim.reset(stage_id == "sserafim");
    }
}

fn step_tween(slot: &mut Option<LinearTween>, dst: &mut f32, dt_s: f64) {
    let Some(mut tween) = *slot else {
        return;
    };
    tween.elapsed_s += dt_s;
    *dst = tween.value() as f32;
    *slot = (!tween.finished()).then_some(tween);
}

fn to_char_shader_role(role: CharacterRole) -> CharShaderRole {
    match role {
        CharacterRole::Player => CharShaderRole::Player,
        CharacterRole::Opponent => CharShaderRole::Opponent,
        CharacterRole::Spectator => CharShaderRole::Spectator,
    }
}

fn aux_path_for_config(cfg: &SpriteShaderConfig) -> Option<&str> {
    cfg.overlay_path
        .as_deref()
        .or(cfg.drop_shadow.alt_mask_path.as_deref())
}

fn load_aux_handle(
    mods: &ModRegistry,
    asset_server: &AssetServer,
    rel_path: Option<&str>,
) -> Option<Handle<Image>> {
    let rel = rel_path?;
    let resolved = mods
        .resolve_asset_server_path(rel)
        .unwrap_or_else(|| rel.to_string());
    Some(asset_server.load(resolved))
}

fn smooth_lerp_precision(base: f32, target: f32, dt_s: f32, duration_s: f32, precision: f32) -> f32 {
    if dt_s <= 0.0 || (base - target).abs() <= f32::EPSILON {
        return base;
    }
    let alpha = precision.powf(dt_s / duration_s);
    target + (base - target) * alpha
}

pub fn attach_stage_and_character_shaders(
    mut commands: Commands,
    mut state: ResMut<PlayState>,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    props: Query<(Entity, &StagePropVisual), Without<ManagedSpriteShader>>,
    mut actors: Query<&mut CharacterActor>,
    char_shaders: Query<(), With<ManagedSpriteShader>>,
    cameras: Query<(Entity, Option<&RainCameraEffect>), (With<Camera2d>, Without<HudCamera>)>,
) {
    let stage_id = state.chart.stage_id.clone();
    let song_id = state.chart.song_id.clone();
    if runtime.attached && runtime.stage_id == stage_id && runtime.song_id == song_id {
        return;
    }

    runtime.reset_for_stage(&stage_id, &song_id);
    runtime.attached = true;

    for (entity, prop) in &props {
        if let Some(mut cfg) = stage_prop_shader(&stage_id, &prop.name) {
            if stage_id == "sserafim" {
                cfg.sserafim.adjust = runtime.sserafim.adjust;
            }
            let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
            let aux_tex = load_aux_handle(&mods, &asset_server, aux_rel.as_deref());
            let handle = materials.add(FunkinSpriteMaterial::from_config(cfg, aux_tex));
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: ShaderTargetKind::StageProp,
                    char_id: String::new(),
                    prop_name: prop.name.clone(),
                    aux_path: aux_rel,
                },
            ));
        }
    }

    for mut actor in &mut actors {
        let role = to_char_shader_role(actor.role);
        if let Some(tint) = stage_character_initial_tint(&stage_id, role) {
            actor.mod_tint = tint;
        }
        let Some(mut cfg) = stage_character_shader(&stage_id, role, &actor.id) else {
            continue;
        };
        if stage_id == "sserafim" {
            cfg.sserafim.adjust = runtime.sserafim.adjust;
        }
        let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
        let aux_tex = load_aux_handle(&mods, &asset_server, aux_rel.as_deref());
        let handle = materials.add(FunkinSpriteMaterial::from_config(cfg, aux_tex));

        let mut target_entities = Vec::with_capacity(1 + actor.piece_entities.len());
        if actor.sheet_entity != Entity::PLACEHOLDER {
            target_entities.push(actor.sheet_entity);
        }
        target_entities.extend(
            actor
                .piece_entities
                .iter()
                .copied()
                .filter(|&e| e != Entity::PLACEHOLDER),
        );

        for entity in target_entities {
            if char_shaders.contains(entity) {
                continue;
            }
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle: handle.clone(),
                    target: ShaderTargetKind::Character(role),
                    char_id: actor.id.clone(),
                    prop_name: String::new(),
                    aux_path: aux_rel.clone(),
                },
            ));
        }
    }

    if let Some(spec) = runtime.rain_spec {
        for (cam_entity, existing) in &cameras {
            if existing.is_none() {
                commands
                    .entity(cam_entity)
                    .insert(RainCameraEffect::from_spec(spec));
            }
        }
    }
    state.pending_shader_events.clear();
}

#[allow(clippy::too_many_arguments)]
pub fn update_stage_shaders(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    mut pending_cmds: ResMut<PendingShaderCommands>,
    mut managed: Query<(Entity, &mut ManagedSpriteShader)>,
    mut stage_props: Query<(Entity, &StagePropVisual, &mut Sprite), Without<DynamicScriptProp>>,
    dynamic_props: Query<(Entity, &DynamicScriptProp), Without<StagePropVisual>>,
    actors: Query<&CharacterActor>,
    mut cameras: Query<(Entity, Option<&mut RainCameraEffect>), (With<Camera2d>, Without<HudCamera>)>,
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

    if !paused && (runtime.stage_id == "phillyTrain" || runtime.stage_id == "phillyTrainErect") {
        let beat_s = (state.conductor.beat_length_ms() / 1000.0) as f32;
        runtime.philly_alpha_shit = (runtime.philly_alpha_shit + beat_s * dt * 1.5).min(1.0);
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
                for (_, prop, mut sprite) in &mut stage_props {
                    if prop.name == "lights" {
                        sprite.color = Color::srgba(r, g, b, a);
                    }
                }
                runtime.philly_alpha_shit = 0.0;
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
                let color = runtime.sserafim.pulse_colors[b % runtime.sserafim.pulse_colors.len()];
                let dur = runtime.sserafim.pulse_durations
                    [b % runtime.sserafim.pulse_durations.len()];
                let int = runtime.sserafim.pulse_intensities
                    [b % runtime.sserafim.pulse_intensities.len()];
                runtime.sserafim.flash_back_light(int, f64::from(dur), color);
            }
        }
        runtime.sserafim.advance(f64::from(dt));
        for (_, prop, mut sprite) in &mut stage_props {
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
                    sprite.color = Color::srgba(1.0, 1.0, 1.0, runtime.sserafim.white_pulse_alpha);
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

        if (mat.config.mode_flags & (SHADER_MODE_WIGGLE | SHADER_MODE_RAIN_SPRITE)) != 0
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
        if let ShaderTargetKind::Character(role) = item.target {
            if (mat.config.mode_flags & SHADER_MODE_DROP_SHADOW) != 0 {
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
        let progress = (state.conductor.song_position_ms.max(0.0) / length_ms).clamp(0.0, 1.0) as f32;
        let intensity = spec.start_intensity + (spec.end_intensity - spec.start_intensity) * progress;

        let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;
        let half_w = (SCREEN_WIDTH * 0.5) / zoom;
        let half_h = (SCREEN_HEIGHT * 0.5) / zoom;
        let cx = state.camera.follow_point[0] as f32;
        let cy = state.camera.follow_point[1] as f32;
        let bounds = Vec4::new(cx - half_w, cy - half_h, cx + half_w, cy + half_h);

        for (_, cam_fx) in &mut cameras {
            if let Some(mut fx) = cam_fx {
                fx.bounds = bounds;
                fx.screen_and_scale = Vec4::new(SCREEN_WIDTH, SCREEN_HEIGHT, spec.scale, intensity);
                fx.time_and_color = Vec4::new(
                    runtime.rain_time_s,
                    spec.rain_color[0],
                    spec.rain_color[1],
                    spec.rain_color[2],
                );
            }
        }
    }
}

fn handle_song_shader_event(event: &SongEventData, runtime: &mut StageShaderRuntime) {
    let (name, params_root, params_list) = unwrap_custom_event(event);
    match name {
        "EnableMask" | "enableMask" => {
            runtime.tankman_alt_mask = true;
        }
        "sserafimDark" => {
            let amount = read_param_f32(params_root, params_list, "amount").unwrap_or(0.0);
            let duration = read_param_f64(params_root, params_list, "duration").unwrap_or(0.0);
            runtime.sserafim.start_darken(amount, duration);
        }
        "sserafimLights" => {
            let amount = read_param_f32(params_root, params_list, "amount").unwrap_or(0.0);
            let duration = read_param_f64(params_root, params_list, "duration").unwrap_or(0.0);
            runtime.sserafim.flash_truck_lights(amount, duration);
        }
        "sserafimPulseLights" => {
            runtime.sserafim.lights_enabled =
                read_param_bool(params_root, params_list, "enabled").unwrap_or(false);
            if let Some(colors) = read_param_string_array(params_root, params_list, "colors") {
                runtime.sserafim.pulse_colors =
                    colors.iter().map(|s| parse_shader_color_rgb(s)).collect();
            }
            if let Some(durs) = read_param_f32_array(params_root, params_list, "durations") {
                runtime.sserafim.pulse_durations = durs;
            }
            if let Some(ints) = read_param_f32_array(params_root, params_list, "intensities") {
                runtime.sserafim.pulse_intensities = ints;
            }
        }
        "sserafimKick" => {
            if read_param_bool(params_root, params_list, "final").unwrap_or(false) {
                runtime.sserafim.start_clear();
            }
        }
        _ => {}
    }
}

fn unwrap_custom_event(
    event: &SongEventData,
) -> (&str, &serde_json::Value, Option<&Vec<serde_json::Value>>) {
    if event.kind.eq_ignore_ascii_case("CustomEvent")
        && let Some(inner_name) = event.raw_value.get("name").and_then(|v| v.as_str())
    {
        let params = event.raw_value.get("params").and_then(|v| v.as_array());
        return (inner_name, &event.raw_value, params);
    }
    (&event.kind, &event.raw_value, None)
}

fn find_param<'a>(
    root: &'a serde_json::Value,
    params: Option<&'a Vec<serde_json::Value>>,
    key: &str,
) -> Option<&'a serde_json::Value> {
    if let Some(list) = params {
        for entry in list {
            if entry.get("name").and_then(|v| v.as_str()) == Some(key)
                && let Some(val) = entry.get("value")
            {
                return Some(val);
            }
        }
    }
    root.get(key)
}

fn read_param_f32(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<f32> {
    let val = find_param(root, params, key)?;
    val.as_f64()
        .map(|n| n as f32)
        .or_else(|| val.as_str().and_then(|s| s.trim().parse::<f32>().ok()))
}

fn read_param_f64(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<f64> {
    let val = find_param(root, params, key)?;
    val.as_f64()
        .or_else(|| val.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

fn read_param_bool(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<bool> {
    let val = find_param(root, params, key)?;
    val.as_bool().or_else(|| {
        val.as_str().and_then(|s| match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" => Some(true),
            "false" | "0" => Some(false),
            _ => None,
        })
    })
}

fn read_param_string_array(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<Vec<String>> {
    let val = find_param(root, params, key)?;
    if let Some(arr) = val.as_array() {
        return Some(
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
        );
    }
    if let Some(s) = val.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<String>>(s) {
            return Some(parsed);
        }
        return Some(
            s.split(',')
                .map(|part| part.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect(),
        );
    }
    None
}

fn read_param_f32_array(
    root: &serde_json::Value,
    params: Option<&Vec<serde_json::Value>>,
    key: &str,
) -> Option<Vec<f32>> {
    let val = find_param(root, params, key)?;
    if let Some(arr) = val.as_array() {
        return Some(arr.iter().filter_map(|v| v.as_f64().map(|n| n as f32)).collect());
    }
    if let Some(s) = val.as_str() {
        if let Ok(parsed) = serde_json::from_str::<Vec<f32>>(s) {
            return Some(parsed);
        }
        return Some(
            s.split(',')
                .filter_map(|part| part.trim().parse::<f32>().ok())
                .collect(),
        );
    }
    None
}

fn collect_target_entities(
    target: &str,
    managed: &Query<(Entity, &mut ManagedSpriteShader)>,
    stage_props: &Query<(Entity, &StagePropVisual, &mut Sprite), Without<DynamicScriptProp>>,
    dynamic_props: &Query<(Entity, &DynamicScriptProp), Without<StagePropVisual>>,
    actors: &Query<&CharacterActor>,
) -> Vec<(Entity, ShaderTargetKind, String, String)> {
    let mut out = Vec::new();
    for actor in actors {
        if crate::play::plugin::actor_matches_target(actor, target) {
            let role = to_char_shader_role(actor.role);
            if actor.sheet_entity != Entity::PLACEHOLDER {
                out.push((
                    actor.sheet_entity,
                    ShaderTargetKind::Character(role),
                    actor.id.clone(),
                    String::new(),
                ));
            }
            for &piece in &actor.piece_entities {
                if piece != Entity::PLACEHOLDER {
                    out.push((
                        piece,
                        ShaderTargetKind::Character(role),
                        actor.id.clone(),
                        String::new(),
                    ));
                }
            }
        }
    }
    for (entity, prop, _) in stage_props.iter() {
        if prop.name == target || prop.name.eq_ignore_ascii_case(target) {
            out.push((entity, ShaderTargetKind::StageProp, String::new(), prop.name.clone()));
        }
    }
    for (entity, dprop) in dynamic_props {
        if dprop.id == target {
            out.push((entity, ShaderTargetKind::DynamicProp, String::new(), dprop.id.clone()));
        }
    }
    if out.is_empty() {
        for (entity, item) in managed.iter() {
            if (!item.prop_name.is_empty() && item.prop_name == target)
                || (!item.char_id.is_empty() && item.char_id.eq_ignore_ascii_case(target))
            {
                out.push((
                    entity,
                    item.target,
                    item.char_id.clone(),
                    item.prop_name.clone(),
                ));
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn apply_shader_script_command(
    cmd: ShaderScriptCommand,
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    materials: &mut Assets<FunkinSpriteMaterial>,
    runtime: &mut StageShaderRuntime,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    stage_props: &Query<(Entity, &StagePropVisual, &mut Sprite), Without<DynamicScriptProp>>,
    dynamic_props: &Query<(Entity, &DynamicScriptProp), Without<StagePropVisual>>,
    actors: &Query<&CharacterActor>,
    cameras: &mut Query<(Entity, Option<&mut RainCameraEffect>), (With<Camera2d>, Without<HudCamera>)>,
) {
    match cmd {
        ShaderScriptCommand::SetCameraRain {
            scale,
            intensity,
            color,
            time_scale_decay,
        } => {
            let spec = CameraRainSpec {
                scale,
                start_intensity: intensity,
                end_intensity: intensity,
                rain_color: color,
                time_scale_decay,
            };
            runtime.rain_spec = Some(spec);
            for (cam_entity, existing) in cameras.iter_mut() {
                if let Some(mut fx) = existing {
                    fx.screen_and_scale.z = scale;
                    fx.screen_and_scale.w = intensity;
                    fx.time_and_color.y = color[0];
                    fx.time_and_color.z = color[1];
                    fx.time_and_color.w = color[2];
                } else {
                    commands
                        .entity(cam_entity)
                        .insert(RainCameraEffect::from_spec(spec));
                }
            }
        }
        ShaderScriptCommand::ClearShader { target } => {
            let lower = target.trim().to_ascii_lowercase();
            if matches!(lower.as_str(), "cam" | "camera" | "camgame" | "rain") {
                runtime.rain_spec = None;
                for (cam_entity, _) in cameras.iter() {
                    commands.entity(cam_entity).remove::<RainCameraEffect>();
                }
                return;
            }
            for (entity, _, _, _) in
                collect_target_entities(&target, managed, stage_props, dynamic_props, actors)
            {
                commands
                    .entity(entity)
                    .remove::<(SpriteMaterial<FunkinSpriteMaterial>, ManagedSpriteShader)>();
            }
        }
        ShaderScriptCommand::SetAdjustColor {
            target,
            hue,
            saturation,
            brightness,
            contrast,
        } => {
            let cfg = SpriteShaderConfig::adjust_color(hue, saturation, brightness, contrast);
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                materials,
                runtime.time_s,
                managed,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetDropShadow {
            target,
            color,
            angle_deg,
            distance,
            strength,
            threshold,
            hue,
            saturation,
            brightness,
            contrast,
            use_alt_mask,
            alt_mask_path,
            mask_threshold,
        } => {
            let cfg = SpriteShaderConfig::drop_shadow(DropShadowParams {
                adjust: AdjustColorParams::new(hue, saturation, brightness, contrast),
                color,
                angle_deg,
                distance,
                strength,
                threshold,
                use_alt_mask,
                alt_mask_path,
                mask_threshold,
                ..Default::default()
            });
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                materials,
                runtime.time_s,
                managed,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetWiggle {
            target,
            effect_type,
            speed,
            frequency,
            amplitude,
        } => {
            let cfg = SpriteShaderConfig::wiggle(effect_type, speed, frequency, amplitude);
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                materials,
                runtime.time_s,
                managed,
                stage_props,
                dynamic_props,
                actors,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_config_to_target(
    target: &str,
    cfg: SpriteShaderConfig,
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    materials: &mut Assets<FunkinSpriteMaterial>,
    time_s: f32,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    stage_props: &Query<(Entity, &StagePropVisual, &mut Sprite), Without<DynamicScriptProp>>,
    dynamic_props: &Query<(Entity, &DynamicScriptProp), Without<StagePropVisual>>,
    actors: &Query<&CharacterActor>,
) {
    let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
    let aux_tex = load_aux_handle(mods, asset_server, aux_rel.as_deref());
    let entities = collect_target_entities(target, managed, stage_props, dynamic_props, actors);

    for (entity, kind, char_id, prop_name) in entities {
        if let Ok((_, mut existing)) = managed.get_mut(entity) {
            if let Some(mut mat) = materials.get_mut(&existing.handle) {
                mat.config = cfg.clone();
                if existing.aux_path != aux_rel {
                    mat.aux_texture = aux_tex.clone();
                    existing.aux_path = aux_rel.clone();
                }
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else {
            let mut mat = FunkinSpriteMaterial::from_config(cfg.clone(), aux_tex.clone());
            mat.time_s = time_s;
            mat.sync_uniform();
            let handle = materials.add(mat);
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: kind,
                    char_id,
                    prop_name,
                    aux_path: aux_rel.clone(),
                },
            ));
        }
    }
}

pub fn parse_wiggle_effect_type(name: &str) -> WiggleEffectType {
    match name.trim().to_ascii_lowercase().as_str() {
        "wavy" | "1" => WiggleEffectType::Wavy,
        "heat_wave_horizontal" | "heatwavehorizontal" | "heat_h" | "2" => {
            WiggleEffectType::HeatWaveHorizontal
        }
        "heat_wave_vertical" | "heatwavevertical" | "heat_v" | "3" => {
            WiggleEffectType::HeatWaveVertical
        }
        "flag" | "4" => WiggleEffectType::Flag,
        _ => WiggleEffectType::Dreamy,
    }
}

pub fn parse_shader_color_rgb(hex: &str) -> [f32; 3] {
    let [r, g, b, _] = crate::scripting::api::parse_hex_rgba(hex);
    [r, g, b]
}
