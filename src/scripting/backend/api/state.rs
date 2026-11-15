use std::collections::BTreeMap;

use serde::Serialize;

use crate::utils::constants::{
    DEFAULT_BPM, DEFAULT_CAMERA_ZOOM, DEFAULT_DIFFICULTY, DEFAULT_VARIATION, HEALTH_STARTING,
    MS_PER_SEC, STEPS_PER_BEAT,
};

#[derive(Debug, Clone, Serialize)]
pub struct ScriptStateSnapshot {
    pub song_id: String,
    pub difficulty: String,
    pub variation: String,
    pub stage_id: String,
    pub bf_id: String,
    pub dad_id: String,
    pub gf_id: String,
    pub song_position_ms: f64,
    pub bpm: f64,
    pub step: i32,
    pub beat: i32,
    pub measure: i32,
    pub step_length_ms: f64,
    pub beat_length_ms: f64,
    pub dt_s: f64,
    pub health: f64,
    pub score: f64,
    pub combo: u32,
    pub misses: u32,
    pub note_count: usize,
    pub scroll_speed: f64,
    pub playback_rate: f64,
    pub botplay: bool,
    pub downscroll: bool,
    pub middlescroll: bool,
    pub strums_swapped: bool,
    pub cam_x: f64,
    pub cam_y: f64,
    pub cam_zoom: f64,
    pub cam_angle_deg: f32,
    pub hud_zoom: f64,
    pub char_positions: BTreeMap<String, [f32; 2]>,
    pub char_anims: BTreeMap<String, String>,
    pub char_anim_finished: BTreeMap<String, bool>,
    pub lane_offsets: BTreeMap<usize, [f32; 2]>,
    pub lane_angles: BTreeMap<usize, f32>,
    pub lane_scales: BTreeMap<usize, [f32; 2]>,
    pub lane_alphas: BTreeMap<usize, f32>,
    pub prop_positions: BTreeMap<String, [f32; 2]>,
    pub prop_scales: BTreeMap<String, [f32; 2]>,
    pub prop_angles: BTreeMap<String, f32>,
    pub prop_alphas: BTreeMap<String, f32>,
}

impl Default for ScriptStateSnapshot {
    fn default() -> Self {
        let beat_ms = (60.0 / DEFAULT_BPM) * MS_PER_SEC;
        let step_ms = beat_ms / f64::from(STEPS_PER_BEAT);
        Self {
            song_id: String::new(),
            difficulty: DEFAULT_DIFFICULTY.to_string(),
            variation: DEFAULT_VARIATION.to_string(),
            stage_id: String::new(),
            bf_id: "bf".to_string(),
            dad_id: "dad".to_string(),
            gf_id: "gf".to_string(),
            song_position_ms: 0.0,
            bpm: DEFAULT_BPM,
            step: 0,
            beat: 0,
            measure: 0,
            step_length_ms: step_ms,
            beat_length_ms: beat_ms,
            dt_s: 0.0,
            health: HEALTH_STARTING,
            score: 0.0,
            combo: 0,
            misses: 0,
            note_count: 0,
            scroll_speed: 1.0,
            playback_rate: 1.0,
            botplay: false,
            downscroll: false,
            middlescroll: false,
            strums_swapped: false,
            cam_x: 0.0,
            cam_y: 0.0,
            cam_zoom: DEFAULT_CAMERA_ZOOM,
            cam_angle_deg: 0.0,
            hud_zoom: DEFAULT_CAMERA_ZOOM,
            char_positions: BTreeMap::new(),
            char_anims: BTreeMap::new(),
            char_anim_finished: BTreeMap::new(),
            lane_offsets: BTreeMap::new(),
            lane_angles: BTreeMap::new(),
            lane_scales: BTreeMap::new(),
            lane_alphas: BTreeMap::new(),
            prop_positions: BTreeMap::new(),
            prop_scales: BTreeMap::new(),
            prop_angles: BTreeMap::new(),
            prop_alphas: BTreeMap::new(),
        }
    }
}

impl ScriptStateSnapshot {
    /// Continuous floating-point beat position (`song_position_ms / beat_length_ms`).
    pub fn beat_f(&self) -> f64 {
        if self.beat_length_ms > 0.0 {
            self.song_position_ms / self.beat_length_ms
        } else {
            0.0
        }
    }

    /// Continuous floating-point step position (`song_position_ms / step_length_ms`).
    pub fn step_f(&self) -> f64 {
        if self.step_length_ms > 0.0 {
            self.song_position_ms / self.step_length_ms
        } else {
            self.beat_f() * f64::from(STEPS_PER_BEAT)
        }
    }

    /// Beat-synced sine wave `amp * sin(beat_f() * mult * TAU)`, where `mult` is cycles per beat.
    pub fn sin_beat(&self, mult: f64, amp: f64) -> f64 {
        (self.beat_f() * mult * std::f64::consts::TAU).sin() * amp
    }

    /// Beat-synced cosine wave `amp * cos(beat_f() * mult * TAU)`, where `mult` is cycles per beat.
    pub fn cos_beat(&self, mult: f64, amp: f64) -> f64 {
        (self.beat_f() * mult * std::f64::consts::TAU).cos() * amp
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptMutableState {
    pub health: f64,
    pub score: f64,
    pub scroll_speed: f64,
    pub playback_rate: f64,
    pub botplay: bool,
    pub middlescroll: bool,
    pub strums_swapped: bool,
    pub cam_x: f64,
    pub cam_y: f64,
    pub cam_zoom: f64,
    pub cam_angle_deg: f32,
    pub hud_zoom: f64,
    pub dirty_health: bool,
    pub dirty_score: bool,
    pub dirty_scroll_speed: bool,
    pub dirty_playback_rate: bool,
    pub dirty_botplay: bool,
    pub dirty_middlescroll: bool,
    pub dirty_strums_swapped: bool,
    pub dirty_cam_pos: bool,
    pub dirty_cam_zoom: bool,
    pub dirty_cam_angle: bool,
    pub dirty_hud_zoom: bool,
}

impl ScriptMutableState {
    pub fn from_snapshot(snap: &ScriptStateSnapshot) -> Self {
        Self {
            health: snap.health,
            score: snap.score,
            scroll_speed: snap.scroll_speed,
            playback_rate: snap.playback_rate,
            botplay: snap.botplay,
            middlescroll: snap.middlescroll,
            strums_swapped: snap.strums_swapped,
            cam_x: snap.cam_x,
            cam_y: snap.cam_y,
            cam_zoom: snap.cam_zoom,
            cam_angle_deg: snap.cam_angle_deg,
            hud_zoom: snap.hud_zoom,
            ..Default::default()
        }
    }
}
