use std::collections::HashMap;

use bevy::prelude::KeyCode;

use crate::utils::constants::{DEFAULT_CAMERA_ZOOM, DEFAULT_DIFFICULTY, DEFAULT_VARIATION};
use crate::utils::easing::EaseKind;

use super::notes::{NoteDirection, NoteStyle, SongNoteData};

#[derive(Debug, Clone)]
pub struct AudioTrackSpec {
    pub file: String,
    pub offset_ms: f64,
    pub volume: f32,
}

impl Default for AudioTrackSpec {
    fn default() -> Self {
        Self {
            file: "Inst.ogg".to_string(),
            offset_ms: 0.0,
            volume: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedAudioConfig {
    pub bpm: f64,
    pub needs_voices: bool,
    pub multi_vocal: bool,
    pub instrumental: HashMap<String, AudioTrackSpec>,
    pub player_vocal: Option<AudioTrackSpec>,
    pub opponent_vocal: Option<AudioTrackSpec>,
    pub spectator_vocal: Option<AudioTrackSpec>,
    pub shared_vocal_file: String,
}

impl Default for ResolvedAudioConfig {
    fn default() -> Self {
        let mut instrumental = HashMap::new();
        instrumental.insert("inst".to_string(), AudioTrackSpec::default());
        Self {
            bpm: 100.0,
            needs_voices: true,
            multi_vocal: false,
            instrumental,
            player_vocal: None,
            opponent_vocal: None,
            spectator_vocal: None,
            shared_vocal_file: "Voices.ogg".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CharactersMeta {
    pub players: Vec<String>,
    pub opponents: Vec<String>,
    pub spectator: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SkinsMeta {
    pub hud: String,
    pub ui: String,
}

impl Default for SkinsMeta {
    fn default() -> Self {
        Self {
            hud: "funkin".to_string(),
            ui: "funkin".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PreciseInputEvent {
    pub direction: NoteDirection,
    pub key_code: Option<KeyCode>,
    pub song_position_ms: f64,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseMode {
    Standard,
    Charting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayPhase {
    Countdown,
    Playing,
    Paused(PauseMode),
    GameOver,
    Ended,
}

#[derive(Debug, Clone)]
pub struct SongEventData {
    pub time_ms: f64,
    pub kind: String,
    pub triggered: bool,
    pub target_x: f64,
    pub target_y: f64,
    pub target_zoom: f64,
    pub duration_s: f64,
    pub duration_steps: Option<f64>,
    pub scroll_speed: Option<f64>,
    pub start_scroll: Option<f64>,
    pub scroll_velocity: Option<f64>,
    pub start_velocity: Option<f64>,
    pub use_start: bool,
    pub absolute: bool,
    pub ease: String,
    pub ease_dir: String,
    pub strumline_target: String,
    pub render_distance_ms: Option<f64>,
    pub use_default_render_distance: bool,
}

impl Default for SongEventData {
    fn default() -> Self {
        Self {
            time_ms: 0.0,
            kind: String::new(),
            triggered: false,
            target_x: 0.0,
            target_y: 0.0,
            target_zoom: 1.0,
            duration_s: 0.0,
            duration_steps: None,
            scroll_speed: None,
            start_scroll: None,
            scroll_velocity: None,
            start_velocity: None,
            use_start: false,
            absolute: false,
            ease: "linear".to_string(),
            ease_dir: "In".to_string(),
            strumline_target: "both".to_string(),
            render_distance_ms: None,
            use_default_render_distance: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SongChart {
    pub song_id: String,
    pub song_name: String,
    pub difficulty: String,
    pub variation: String,
    pub bpm: f64,
    pub scroll_speed: f64,
    pub lanes: usize,
    pub stage_id: String,
    pub characters: CharactersMeta,
    pub credits: Vec<String>,
    pub skins: SkinsMeta,
    pub audio: ResolvedAudioConfig,
    pub player_notes: Vec<SongNoteData>,
    pub opponent_notes: Vec<SongNoteData>,
    pub events: Vec<SongEventData>,
    pub length_ms: f64,
}

impl Default for SongChart {
    fn default() -> Self {
        Self {
            song_id: "bopeebo".to_string(),
            song_name: "Bopeebo".to_string(),
            difficulty: DEFAULT_DIFFICULTY.to_string(),
            variation: DEFAULT_VARIATION.to_string(),
            bpm: 100.0,
            scroll_speed: 1.0,
            lanes: 4,
            stage_id: "mainStage".to_string(),
            characters: CharactersMeta::default(),
            credits: Vec::new(),
            skins: SkinsMeta::default(),
            audio: ResolvedAudioConfig::default(),
            player_notes: Vec::new(),
            opponent_notes: Vec::new(),
            events: Vec::new(),
            length_ms: 60_000.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PlayStateParams {
    pub chart: SongChart,
    pub instrumental: String,
    pub note_style: NoteStyle,
    pub practice_mode: bool,
    pub bot_play_mode: bool,
    pub minimal_mode: bool,
    pub start_timestamp_ms: f64,
    pub playback_rate: f64,
    pub camera_follow_point: Option<[f64; 2]>,
}

impl Default for PlayStateParams {
    fn default() -> Self {
        Self {
            chart: SongChart::default(),
            instrumental: String::new(),
            note_style: NoteStyle::default(),
            practice_mode: false,
            bot_play_mode: false,
            minimal_mode: false,
            start_timestamp_ms: 0.0,
            playback_rate: 1.0,
            camera_follow_point: None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct LinearTween {
    pub start: f64,
    pub target: f64,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub ease: EaseKind,
}

impl LinearTween {
    pub fn linear(start: f64, target: f64, duration_s: f64) -> Self {
        Self::eased(start, target, duration_s, EaseKind::Linear)
    }

    pub fn eased(start: f64, target: f64, duration_s: f64, ease: EaseKind) -> Self {
        Self { start, target, elapsed_s: 0.0, duration_s, ease }
    }

    pub fn value(&self) -> f64 {
        if self.duration_s <= 0.0 {
            return self.target;
        }
        let raw_t = (self.elapsed_s / self.duration_s).clamp(0.0, 1.0);
        let t = self.ease.eval(raw_t);
        self.start + (self.target - self.start) * t
    }

    pub fn finished(&self) -> bool {
        self.duration_s <= 0.0 || self.elapsed_s >= self.duration_s
    }
}

#[derive(Debug, Clone)]
pub struct CameraState {
    pub follow_point: [f64; 2],
    pub follow_x_tween: Option<LinearTween>,
    pub follow_y_tween: Option<LinearTween>,
    pub current_zoom: f64,
    pub zoom_tween: Option<LinearTween>,
    pub stage_zoom: f64,
    pub bop_multiplier: f64,
    pub default_hud_zoom: f64,
    pub hud_zoom: f64,
    pub bop_intensity: f64,
    pub hud_zoom_intensity: f64,
    pub zoom_rate: f64,
    pub zoom_rate_offset: f64,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            follow_point: [0.0, 0.0],
            follow_x_tween: None,
            follow_y_tween: None,
            current_zoom: DEFAULT_CAMERA_ZOOM,
            zoom_tween: None,
            stage_zoom: DEFAULT_CAMERA_ZOOM,
            bop_multiplier: 1.0,
            default_hud_zoom: DEFAULT_CAMERA_ZOOM,
            hud_zoom: DEFAULT_CAMERA_ZOOM,
            bop_intensity: 1.015,
            hud_zoom_intensity: 0.03,
            zoom_rate: 4.0,
            zoom_rate_offset: 0.0,
        }
    }
}
