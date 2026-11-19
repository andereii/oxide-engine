use std::collections::BTreeMap;

use serde::Serialize;

use crate::utils::constants::{
    DEFAULT_BPM, DEFAULT_CAMERA_ZOOM, DEFAULT_DIFFICULTY, DEFAULT_VARIATION, HEALTH_STARTING,
    MS_PER_SEC, STEPS_PER_BEAT,
};

#[derive(Debug, Clone, Serialize, Default)]
pub struct ScriptNoteSnapshot {
    pub id: usize,
    pub time_ms: f64,
    pub dir: usize,
    pub length_ms: f64,
    pub kind: String,
    pub is_player: bool,
    pub hit: bool,
    pub missed: bool,
    pub holding: bool,
    pub rem_ms: f64,
}

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
    pub song_length_ms: f64,
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
    pub max_combo: u32,
    pub misses: u32,
    pub sick: u32,
    pub good: u32,
    pub bad: u32,
    pub shit: u32,
    pub hit_notes: u32,
    pub total_notes: u32,
    pub note_count: usize,
    pub scroll_speed: f64,
    pub playback_rate: f64,
    pub botplay: bool,
    pub downscroll: bool,
    pub middlescroll: bool,
    pub strums_swapped: bool,
    pub practice_mode: bool,
    pub ghost_tap: bool,
    pub in_countdown: bool,
    pub in_cutscene: bool,
    pub paused: bool,
    pub can_pause: bool,
    pub keys_disabled: bool,
    pub deaths: u32,
    pub inst_vol: f64,
    pub master_volume: f64,
    pub p_vocal_vol: f64,
    pub o_vocal_vol: f64,
    pub cam_x: f64,
    pub cam_y: f64,
    pub cam_zoom: f64,
    pub stage_zoom: f64,
    pub cam_angle_deg: f32,
    pub hud_zoom: f64,
    pub hud_angle_deg: f32,
    pub story_mode: bool,
    pub playlist_songs: Vec<String>,
    pub campaign_score: i32,
    pub char_positions: BTreeMap<String, [f32; 2]>,
    pub char_top_left: BTreeMap<String, [f32; 2]>,
    pub char_z: BTreeMap<String, f32>,
    pub char_scales: BTreeMap<String, [f32; 2]>,
    pub char_alphas: BTreeMap<String, f32>,
    pub char_flips: BTreeMap<String, [bool; 2]>,
    pub char_vis: BTreeMap<String, bool>,
    pub char_sizes: BTreeMap<String, [f32; 2]>,
    pub char_focus: BTreeMap<String, [f64; 2]>,
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
    pub prop_z: BTreeMap<String, f32>,
    pub prop_scrolls: BTreeMap<String, [f32; 2]>,
    pub prop_flips: BTreeMap<String, [bool; 2]>,
    pub prop_vis: BTreeMap<String, bool>,
    pub prop_sizes: BTreeMap<String, [f32; 2]>,
    pub prop_velocities: BTreeMap<String, [f32; 2]>,
    pub prop_anims: BTreeMap<String, String>,
    pub prop_anim_done: BTreeMap<String, bool>,
    pub texts: BTreeMap<String, String>,
    pub text_positions: BTreeMap<String, [f32; 2]>,
    pub text_alphas: BTreeMap<String, f32>,
    pub active_sounds: Vec<String>,
    pub sound_volumes: BTreeMap<String, f32>,
    pub active_notes: Vec<ScriptNoteSnapshot>,
    pub keys_down: Vec<String>,
    pub keys_hit: Vec<String>,
    pub keys_up: Vec<String>,
    pub ctrls_down: Vec<String>,
    pub ctrls_hit: Vec<String>,
    pub ctrls_up: Vec<String>,
}

impl Default for ScriptStateSnapshot {
    fn default() -> Self {
        let beat_ms = (60.0 / DEFAULT_BPM) * MS_PER_SEC;
        let step_ms = beat_ms / f64::from(STEPS_PER_BEAT);
        let mut s = Self::empty_placeholder();
        s.difficulty = DEFAULT_DIFFICULTY.to_string();
        s.variation = DEFAULT_VARIATION.to_string();
        s.bf_id = "bf".to_string();
        s.dad_id = "dad".to_string();
        s.gf_id = "gf".to_string();
        s.bpm = DEFAULT_BPM;
        s.step_length_ms = step_ms;
        s.beat_length_ms = beat_ms;
        s.health = HEALTH_STARTING;
        s.scroll_speed = 1.0;
        s.playback_rate = 1.0;
        s.can_pause = true;
        s.inst_vol = 1.0;
        s.p_vocal_vol = 1.0;
        s.o_vocal_vol = 1.0;
        s.cam_zoom = DEFAULT_CAMERA_ZOOM;
        s.stage_zoom = DEFAULT_CAMERA_ZOOM;
        s.hud_zoom = DEFAULT_CAMERA_ZOOM;
        s
    }
}

impl ScriptStateSnapshot {
    /// Returns a zero-allocation placeholder snapshot used while swapping `ScriptContext` into TLS.
    pub fn empty_placeholder() -> Self {
        Self {
            song_id: String::new(),
            difficulty: String::new(),
            variation: String::new(),
            stage_id: String::new(),
            bf_id: String::new(),
            dad_id: String::new(),
            gf_id: String::new(),
            song_position_ms: 0.0,
            song_length_ms: 0.0,
            bpm: 0.0,
            step: 0,
            beat: 0,
            measure: 0,
            step_length_ms: 0.0,
            beat_length_ms: 0.0,
            dt_s: 0.0,
            health: 0.0,
            score: 0.0,
            combo: 0,
            max_combo: 0,
            misses: 0,
            sick: 0,
            good: 0,
            bad: 0,
            shit: 0,
            hit_notes: 0,
            total_notes: 0,
            note_count: 0,
            scroll_speed: 0.0,
            playback_rate: 0.0,
            botplay: false,
            downscroll: false,
            middlescroll: false,
            strums_swapped: false,
            practice_mode: false,
            ghost_tap: false,
            in_countdown: false,
            in_cutscene: false,
            paused: false,
            can_pause: false,
            keys_disabled: false,
            deaths: 0,
            inst_vol: 0.0,
            master_volume: 1.0,
            p_vocal_vol: 0.0,
            o_vocal_vol: 0.0,
            cam_x: 0.0,
            cam_y: 0.0,
            cam_zoom: 0.0,
            stage_zoom: 0.0,
            cam_angle_deg: 0.0,
            hud_zoom: 0.0,
            hud_angle_deg: 0.0,
            story_mode: false,
            playlist_songs: Vec::new(),
            campaign_score: 0,
            char_positions: BTreeMap::new(),
            char_top_left: BTreeMap::new(),
            char_z: BTreeMap::new(),
            char_scales: BTreeMap::new(),
            char_alphas: BTreeMap::new(),
            char_flips: BTreeMap::new(),
            char_vis: BTreeMap::new(),
            char_sizes: BTreeMap::new(),
            char_focus: BTreeMap::new(),
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
            prop_z: BTreeMap::new(),
            prop_scrolls: BTreeMap::new(),
            prop_flips: BTreeMap::new(),
            prop_vis: BTreeMap::new(),
            prop_sizes: BTreeMap::new(),
            prop_velocities: BTreeMap::new(),
            prop_anims: BTreeMap::new(),
            prop_anim_done: BTreeMap::new(),
            texts: BTreeMap::new(),
            text_positions: BTreeMap::new(),
            text_alphas: BTreeMap::new(),
            active_sounds: Vec::new(),
            sound_volumes: BTreeMap::new(),
            active_notes: Vec::new(),
            keys_down: Vec::new(),
            keys_hit: Vec::new(),
            keys_up: Vec::new(),
            ctrls_down: Vec::new(),
            ctrls_hit: Vec::new(),
            ctrls_up: Vec::new(),
        }
    }

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

    /// Accuracy percentage (`0.0..=100.0`) from judgement tallies and note count.
    pub fn accuracy(&self) -> f64 {
        let weighted = f64::from(self.sick) * 1.0
            + f64::from(self.good) * 0.8
            + f64::from(self.bad) * 0.5
            + f64::from(self.shit) * 0.25;
        let denom = u32::max(self.sick + self.good + self.bad + self.shit + self.misses, 0);
        let fallback_total = self.total_notes.max(self.hit_notes);
        let base = if denom > 0 { denom } else { fallback_total };
        if base == 0 {
            100.0
        } else {
            ((weighted / f64::from(base)) * 100.0).clamp(0.0, 100.0)
        }
    }

    /// Funkin-style grade letter (`"P"`, `"E"`, `"G"`, `"F"`, `"L"`).
    pub fn rank_name(&self) -> &'static str {
        let acc = self.accuracy();
        if self.misses == 0 && self.bad == 0 && self.shit == 0 && acc >= 99.0 {
            "P"
        } else if acc >= 90.0 {
            "E"
        } else if acc >= 80.0 {
            "G"
        } else if acc >= 60.0 {
            "F"
        } else {
            "L"
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ScriptMutableState {
    pub health: f64,
    pub score: f64,
    pub scroll_speed: f64,
    pub playback_rate: f64,
    pub botplay: bool,
    pub downscroll: bool,
    pub middlescroll: bool,
    pub strums_swapped: bool,
    pub practice_mode: bool,
    pub ghost_tap: bool,
    pub can_pause: bool,
    pub in_cutscene: bool,
    pub keys_disabled: bool,
    pub cam_x: f64,
    pub cam_y: f64,
    pub cam_zoom: f64,
    pub cam_angle_deg: f32,
    pub hud_zoom: f64,
    pub hud_angle_deg: f32,
    pub dirty_health: bool,
    pub dirty_score: bool,
    pub dirty_scroll_speed: bool,
    pub dirty_playback_rate: bool,
    pub dirty_botplay: bool,
    pub dirty_downscroll: bool,
    pub dirty_middlescroll: bool,
    pub dirty_strums_swapped: bool,
    pub dirty_practice: bool,
    pub dirty_ghost_tap: bool,
    pub dirty_can_pause: bool,
    pub dirty_cutscene: bool,
    pub dirty_keys_disabled: bool,
    pub dirty_cam_pos: bool,
    pub dirty_cam_zoom: bool,
    pub dirty_cam_angle: bool,
    pub dirty_hud_zoom: bool,
    pub dirty_hud_angle: bool,
}

impl ScriptMutableState {
    pub fn from_snapshot(snap: &ScriptStateSnapshot) -> Self {
        Self {
            health: snap.health,
            score: snap.score,
            scroll_speed: snap.scroll_speed,
            playback_rate: snap.playback_rate,
            botplay: snap.botplay,
            downscroll: snap.downscroll,
            middlescroll: snap.middlescroll,
            strums_swapped: snap.strums_swapped,
            practice_mode: snap.practice_mode,
            ghost_tap: snap.ghost_tap,
            can_pause: snap.can_pause,
            in_cutscene: snap.in_cutscene,
            keys_disabled: snap.keys_disabled,
            cam_x: snap.cam_x,
            cam_y: snap.cam_y,
            cam_zoom: snap.cam_zoom,
            cam_angle_deg: snap.cam_angle_deg,
            hud_zoom: snap.hud_zoom,
            hud_angle_deg: snap.hud_angle_deg,
            ..Default::default()
        }
    }
}
