mod attach;
mod commands;
mod events;
mod update;

use bevy::prelude::*;

use crate::mods::ModRegistry;
use crate::play::characters::CharacterRole;
use crate::play::types::LinearTween;
use crate::utils::easing::EaseKind;

use super::defs::{
    AdjustColorParams, CameraRainSpec, CharShaderRole, SpriteShaderConfig, stage_camera_rain,
};

pub use attach::attach_stage_and_character_shaders;
pub use events::parse_wiggle_effect_type;
pub use update::update_stage_shaders;

/// Dust adjust-color preset set by `sserafim.hxc`'s `resetClear()` before `sserafimKick`.
pub(super) const SSERAFIM_DUST_ADJUST: AdjustColorParams =
    AdjustColorParams::new(6.0, -74.0, -24.0, -26.0);
const SSERAFIM_CLEAR_DURATION_S: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderTargetKind {
    Character(CharShaderRole),
    StageProp,
    DynamicProp,
}

#[derive(Component, Debug, Clone)]
pub struct ManagedSpriteShader {
    pub handle: Handle<super::material::FunkinSpriteMaterial>,
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
        effect_type: super::defs::WiggleEffectType,
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
    SetBlendMode {
        target: String,
        mode: String,
    },
    SetCustomShader {
        target: String,
        path: String,
    },
    SetShaderParam {
        target: String,
        key: String,
        values: [f32; 4],
    },
    SetShaderTex {
        target: String,
        path: String,
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
    pub(super) fn reset(&mut self, active: bool) {
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

    pub(super) fn advance(&mut self, dt_s: f64) {
        step_tween(&mut self.hue_tween, &mut self.adjust.hue, dt_s);
        step_tween(&mut self.sat_tween, &mut self.adjust.saturation, dt_s);
        step_tween(&mut self.bright_tween, &mut self.adjust.brightness, dt_s);
        step_tween(&mut self.contrast_tween, &mut self.adjust.contrast, dt_s);
        step_tween(&mut self.darken_tween, &mut self.darken_amount, dt_s);
        step_tween(&mut self.truck_tween, &mut self.truck_strength, dt_s);
        step_tween(&mut self.truck2_tween, &mut self.truck2_alpha, dt_s);
        step_tween(&mut self.pulse_tween, &mut self.pulse_strength, dt_s);
        step_tween(
            &mut self.white_pulse_tween,
            &mut self.white_pulse_alpha,
            dt_s,
        );
    }

    pub(super) fn start_darken(&mut self, amount: f32, duration_s: f64) {
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

    pub(super) fn flash_truck_lights(&mut self, amount: f32, duration_s: f64) {
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

    pub(super) fn flash_back_light(&mut self, amount: f32, duration_s: f64, color: [f32; 3]) {
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

    pub(super) fn start_clear(&mut self) {
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
    pub next_rt_layer: usize,
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
            next_rt_layer: 10,
            sserafim: SserafimRuntimeState::default(),
        }
    }
}

impl StageShaderRuntime {
    pub(super) fn reset_for_stage(&mut self, stage_id: &str, song_id: &str) {
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
        self.next_rt_layer = 10;
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

pub(super) fn to_char_shader_role(role: CharacterRole) -> CharShaderRole {
    match role {
        CharacterRole::Player => CharShaderRole::Player,
        CharacterRole::Opponent => CharShaderRole::Opponent,
        CharacterRole::Spectator => CharShaderRole::Spectator,
    }
}

pub(super) fn aux_path_for_config(cfg: &SpriteShaderConfig) -> Option<&str> {
    cfg.overlay_path
        .as_deref()
        .or(cfg.drop_shadow.alt_mask_path.as_deref())
}

pub(super) fn load_aux_handle(
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
