mod attach;
mod commands;
mod events;
mod update;

use bevy::prelude::*;

use crate::mods::ModRegistry;
use crate::play::characters::CharacterRole;

use super::defs::{CameraRainSpec, CharShaderRole, SpriteShaderConfig};

pub use attach::attach_stage_and_character_shaders;
pub(crate) use attach::ensure_character_composite_rt;
pub use events::parse_wiggle_effect_type;
pub use update::{sync_stage_blend_post, update_stage_shaders};

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
        start_intensity: f32,
        end_intensity: f32,
        color: [f32; 3],
        time_scale_decay: bool,
    },
    SetOverlayBlend {
        target: String,
        aux_path: String,
    },
    SetRainSprite {
        target: String,
        scale: f32,
        intensity: f32,
        color: [f32; 3],
    },
    SetBuilding {
        target: String,
        alpha_shit: f32,
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

#[derive(Resource, Debug)]
pub struct StageShaderRuntime {
    pub stage_id: String,
    pub song_id: String,
    pub attached: bool,
    pub time_s: f32,
    pub rain_spec: Option<CameraRainSpec>,
    pub rain_time_s: f32,
    pub rain_time_scale: f32,
    pub last_total_notes_hit: u32,
    pub tankman_alt_mask: bool,
    pub next_rt_layer: usize,
}

impl Default for StageShaderRuntime {
    fn default() -> Self {
        Self {
            stage_id: String::new(),
            song_id: String::new(),
            attached: false,
            time_s: 0.0,
            rain_spec: None,
            rain_time_s: 1.0,
            rain_time_scale: 1.0,
            last_total_notes_hit: 0,
            tankman_alt_mask: false,
            next_rt_layer: 10,
        }
    }
}

impl StageShaderRuntime {
    pub(super) fn reset_for_stage(&mut self, stage_id: &str, song_id: &str) {
        self.stage_id = stage_id.to_string();
        self.song_id = song_id.to_string();
        self.time_s = 0.0;
        self.rain_spec = None;
        self.rain_time_s = 1.0;
        self.rain_time_scale = 1.0;
        self.last_total_notes_hit = 0;
        self.tankman_alt_mask = false;
        self.next_rt_layer = 10;
    }
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
