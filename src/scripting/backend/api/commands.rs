use crate::scripting::value::ScriptValue;
use crate::utils::easing::EaseKind;

use super::tweens::{TweenProp, TweenTarget};

pub enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub enum ScriptCommand {
    Log {
        level: LogLevel,
        mod_id: String,
        message: String,
    },
    SetStatus(String),
    SetScoreText(Option<String>),
    SetHudVisible(bool),
    TweenCameraZoom {
        target: f64,
        duration_s: f64,
        ease: String,
    },
    TweenCameraPos {
        x: f64,
        y: f64,
        duration_s: f64,
        ease: String,
    },
    TweenScrollSpeed {
        target: f64,
        duration_s: f64,
        ease: String,
    },
    FocusCameraTarget(String),
    CameraBop {
        game_delta: f64,
        hud_delta: f64,
    },
    SetCameraBopConfig {
        intensity: f64,
        rate: f64,
    },
    CameraShake {
        intensity: f32,
        duration_s: f64,
        hud: bool,
    },
    CameraFlash {
        color: [f32; 4],
        duration_s: f64,
        hud: bool,
    },
    PlayAnim {
        target: String,
        anim: String,
        force: bool,
    },
    SetCharacterPos {
        target: String,
        x: f32,
        y: f32,
    },
    SetCharacterScale {
        target: String,
        sx: f32,
        sy: f32,
    },
    SetCharacterTint {
        target: String,
        color: [f32; 4],
    },
    SetCharacterVisible {
        target: String,
        visible: bool,
    },
    SetCharacterDanceEvery {
        target: String,
        beats: f32,
    },
    SetLaneOffset {
        lane: usize,
        x: f32,
        y: f32,
    },
    SetLaneAngle {
        lane: usize,
        deg: f32,
    },
    SetLaneScale {
        lane: usize,
        sx: f32,
        sy: f32,
    },
    SetLaneAlpha {
        lane: usize,
        alpha: f32,
    },
    ResetLanes,
    SpawnProp {
        id: String,
        image_path: Option<String>,
        color: [f32; 4],
        size: [f32; 2],
        pos: [f32; 2],
        z: f32,
        hud: bool,
    },
    DespawnProp(String),
    SetPropNumber {
        id: String,
        prop: TweenProp,
        value: f32,
    },
    SetPropTint {
        id: String,
        color: [f32; 4],
    },
    SetPropVisible {
        id: String,
        visible: bool,
    },
    SetAudioVolumes {
        inst: Option<f32>,
        vocals: Option<f32>,
        player_vocal: Option<f32>,
        opponent_vocal: Option<f32>,
    },
    AddNote {
        time_ms: f64,
        dir: usize,
        length_ms: f64,
        kind: String,
    },
    TriggerSongEvent {
        name: String,
        value: serde_json::Value,
    },
    PlaySfx {
        path: String,
        volume: f32,
    },
    SetShaderAdjustColor {
        target: String,
        hue: f32,
        saturation: f32,
        brightness: f32,
        contrast: f32,
    },
    SetShaderDropShadow {
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
    SetShaderWiggle {
        target: String,
        effect: String,
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
    EndSong,
    RestartSong,
    PauseSong,
}

#[derive(Debug, Clone)]
