use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::mods::{ModRegistry, ModStore};
use crate::utils::constants::{DEFAULT_BPM, DEFAULT_CAMERA_ZOOM, DEFAULT_DIFFICULTY, DEFAULT_VARIATION, HEALTH_STARTING, MS_PER_SEC, STEPS_PER_BEAT};
use crate::utils::easing::EaseKind;

use super::value::ScriptValue;

/// Maximum nested recursion depth allowed when scripts trigger custom hooks via `emit(...)`.
pub const MAX_CUSTOM_HOOK_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TweenProp {
    X,
    Y,
    Alpha,
    Angle,
    ScaleX,
    ScaleY,
    Zoom,
    HudZoom,
    Scroll,
}

impl TweenProp {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "x" | "pos_x" => Some(Self::X),
            "y" | "pos_y" => Some(Self::Y),
            "alpha" | "opacity" | "a" => Some(Self::Alpha),
            "angle" | "rot" | "rotation" => Some(Self::Angle),
            "scale_x" | "scalex" | "sx" => Some(Self::ScaleX),
            "scale_y" | "scaley" | "sy" => Some(Self::ScaleY),
            "zoom" => Some(Self::Zoom),
            "hud_zoom" | "hudzoom" => Some(Self::HudZoom),
            "scroll" | "scroll_speed" => Some(Self::Scroll),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TweenTarget {
    Camera,
    Scroll,
    Lane(usize),
    Prop(String),
}

impl TweenTarget {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let lower = trimmed.to_ascii_lowercase();
        if matches!(lower.as_str(), "cam" | "camera" | "cam_game") {
            return Self::Camera;
        }
        if matches!(lower.as_str(), "scroll" | "speed") {
            return Self::Scroll;
        }
        if let Some(idx_str) = lower.strip_prefix("lane:").or_else(|| lower.strip_prefix("strum:"))
            && let Ok(lane) = idx_str.parse::<usize>()
        {
            return Self::Lane(lane);
        }
        let prop_id = trimmed.strip_prefix("prop:").unwrap_or(trimmed);
        Self::Prop(prop_id.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct PropertyTween {
    pub target: TweenTarget,
    pub prop: TweenProp,
    pub start: f64,
    pub end: f64,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub ease: EaseKind,
}

impl PropertyTween {
    pub fn sample(&self) -> f64 {
        if self.duration_s <= 0.0 {
            return self.end;
        }
        let t = (self.elapsed_s / self.duration_s).clamp(0.0, 1.0);
        let eased = self.ease.eval(t);
        self.start + (self.end - self.start) * eased
    }

    pub fn is_finished(&self) -> bool {
        self.duration_s <= 0.0 || self.elapsed_s >= self.duration_s
    }
}

#[derive(Debug, Clone)]
pub struct ScriptTimer {
    pub id: u64,
    pub script_id: String,
    pub fn_name: String,
    pub interval_s: f64,
    pub remaining_s: f64,
    pub repeats_left: Option<u32>,
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

#[derive(Debug, Clone)]
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
pub struct NewTimerRequest {
    pub script_id: String,
    pub fn_name: String,
    pub interval_s: f64,
    pub repeats_left: Option<u32>,
}

/// Cross-script custom hook request queued by a script calling `emit(name[, val])`.
#[derive(Debug, Clone, PartialEq)]
pub struct CustomHookRequest {
    pub name: String,
    pub data: ScriptValue,
    pub sender_script_id: String,
    pub sender_mod_id: String,
}

/// Self-contained execution context handed to every scripting backend during hook execution.
/// Future backends (Lua, Wasm) bind directly against this struct without needing Rhai types.
pub struct ScriptContext {
    pub current_script_id: String,
    pub current_mod_id: String,
    pub snapshot: ScriptStateSnapshot,
    pub mutable: ScriptMutableState,
    pub commands: Vec<ScriptCommand>,
    pub new_tweens: Vec<PropertyTween>,
    pub cancel_tween_targets: Vec<TweenTarget>,
    pub new_timers: Vec<NewTimerRequest>,
    pub cancel_timers_for_fn: Vec<(String, String)>,
    pub pending_custom_hooks: Vec<CustomHookRequest>,
    pub registry: ModRegistry,
    pub store: ModStore,
    pub rng_state: u64,
}

impl ScriptContext {
    pub fn new(
        snapshot: ScriptStateSnapshot,
        registry: ModRegistry,
        store: ModStore,
        rng_state: u64,
    ) -> Self {
        let mutable = ScriptMutableState::from_snapshot(&snapshot);
        Self {
            current_script_id: String::new(),
            current_mod_id: String::new(),
            snapshot,
            mutable,
            commands: Vec::new(),
            new_tweens: Vec::new(),
            cancel_tween_targets: Vec::new(),
            new_timers: Vec::new(),
            cancel_timers_for_fn: Vec::new(),
            pending_custom_hooks: Vec::new(),
            registry,
            store,
            rng_state: if rng_state == 0 { 0x9E37_79B9_7F4A_7C15 } else { rng_state },
        }
    }

    /// Queues a `HookKind::Custom(name)` dispatch to run after the current hook completes.
    pub fn emit_custom_hook(&mut self, name: impl Into<String>, data: ScriptValue) {
        self.pending_custom_hooks.push(CustomHookRequest {
            name: name.into(),
            data,
            sender_script_id: self.current_script_id.clone(),
            sender_mod_id: self.current_mod_id.clone(),
        });
    }

    pub fn next_rand_u64(&mut self) -> u64 {
        let mut x = self.rng_state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.rng_state = x;
        x
    }

    pub fn next_rand_f64(&mut self) -> f64 {
        (self.next_rand_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }

    pub fn current_prop_value(&self, target: &TweenTarget, prop: TweenProp) -> f64 {
        match (target, prop) {
            (TweenTarget::Camera, TweenProp::X) => self.mutable.cam_x,
            (TweenTarget::Camera, TweenProp::Y) => self.mutable.cam_y,
            (TweenTarget::Camera, TweenProp::Zoom) => self.mutable.cam_zoom,
            (TweenTarget::Camera, TweenProp::HudZoom) => self.mutable.hud_zoom,
            (TweenTarget::Camera, TweenProp::Angle) => f64::from(self.mutable.cam_angle_deg),
            (TweenTarget::Scroll, TweenProp::Scroll) => self.mutable.scroll_speed,
            (TweenTarget::Lane(lane), TweenProp::X) => self
                .snapshot
                .lane_offsets
                .get(lane)
                .map_or(0.0, |p| f64::from(p[0])),
            (TweenTarget::Lane(lane), TweenProp::Y) => self
                .snapshot
                .lane_offsets
                .get(lane)
                .map_or(0.0, |p| f64::from(p[1])),
            (TweenTarget::Lane(lane), TweenProp::Angle) => self
                .snapshot
                .lane_angles
                .get(lane)
                .copied()
                .map_or(0.0, f64::from),
            (TweenTarget::Lane(lane), TweenProp::ScaleX) => self
                .snapshot
                .lane_scales
                .get(lane)
                .map_or(1.0, |s| f64::from(s[0])),
            (TweenTarget::Lane(lane), TweenProp::ScaleY) => self
                .snapshot
                .lane_scales
                .get(lane)
                .map_or(1.0, |s| f64::from(s[1])),
            (TweenTarget::Lane(lane), TweenProp::Alpha) => self
                .snapshot
                .lane_alphas
                .get(lane)
                .copied()
                .map_or(1.0, f64::from),
            (TweenTarget::Prop(id), TweenProp::X) => self
                .snapshot
                .prop_positions
                .get(id)
                .map_or(0.0, |p| f64::from(p[0])),
            (TweenTarget::Prop(id), TweenProp::Y) => self
                .snapshot
                .prop_positions
                .get(id)
                .map_or(0.0, |p| f64::from(p[1])),
            (TweenTarget::Prop(id), TweenProp::ScaleX) => self
                .snapshot
                .prop_scales
                .get(id)
                .map_or(1.0, |s| f64::from(s[0])),
            (TweenTarget::Prop(id), TweenProp::ScaleY) => self
                .snapshot
                .prop_scales
                .get(id)
                .map_or(1.0, |s| f64::from(s[1])),
            (TweenTarget::Prop(id), TweenProp::Angle) => self
                .snapshot
                .prop_angles
                .get(id)
                .copied()
                .map_or(0.0, f64::from),
            (TweenTarget::Prop(id), TweenProp::Alpha) => self
                .snapshot
                .prop_alphas
                .get(id)
                .copied()
                .map_or(1.0, f64::from),
            _ => 0.0,
        }
    }

    pub fn push_tween(&mut self, target: TweenTarget, prop: TweenProp, end: f64, duration_s: f64, ease: &str) {
        let start = self.current_prop_value(&target, prop);
        let parsed_ease = EaseKind::from_name(ease, "");
        let dur = if parsed_ease.is_some() { duration_s.max(0.0) } else { 0.0 };
        self.new_tweens.push(PropertyTween {
            target,
            prop,
            start,
            end,
            elapsed_s: 0.0,
            duration_s: dur,
            ease: parsed_ease.unwrap_or_default(),
        });
    }
}

pub fn parse_hex_rgba(raw: &str) -> [f32; 4] {
    let cleaned = raw.trim().trim_start_matches('#').trim_start_matches("0x");
    let parse_byte = |slice: &str| -> f32 {
        u8::from_str_radix(slice, 16)
            .map(|b| f32::from(b) / 255.0)
            .unwrap_or(1.0)
    };
    match cleaned.len() {
        6 => [
            parse_byte(&cleaned[0..2]),
            parse_byte(&cleaned[2..4]),
            parse_byte(&cleaned[4..6]),
            1.0,
        ],
        8 => [
            parse_byte(&cleaned[2..4]),
            parse_byte(&cleaned[4..6]),
            parse_byte(&cleaned[6..8]),
            parse_byte(&cleaned[0..2]),
        ],
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

/// Linear interpolation `a + (b - a) * t` shared across scripting backends.
pub fn script_lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Clamps `v` into `[min(lo, hi), max(lo, hi)]` without panicking on swapped bounds or `NaN`.
pub fn script_clamp(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() || lo.is_nan() || hi.is_nan() {
        return v;
    }
    v.clamp(lo.min(hi), lo.max(hi))
}

/// Remaps `v` from `[in_lo, in_hi]` onto `[out_lo, out_hi]`, returning `out_lo` on zero-width spans.
pub fn script_remap(v: f64, in_lo: f64, in_hi: f64, out_lo: f64, out_hi: f64) -> f64 {
    let span = in_hi - in_lo;
    if span.abs() < f64::EPSILON {
        return out_lo;
    }
    let t = (v - in_lo) / span;
    script_lerp(out_lo, out_hi, t)
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct ScriptApiDoc {
    pub name: &'static str,
    pub signature: &'static str,
    pub category: &'static str,
    pub summary: &'static str,
}

/// Cross-backend catalog entry type alias used for ABI documentation and verification.
pub type ScriptApiCatalogEntry = ScriptApiDoc;

/// Authoritative API catalog shared across backends (Rhai today; Lua/Wasm in the future).
pub const SCRIPT_API_CATALOG: &[ScriptApiDoc] = &[
    ScriptApiDoc { name: "log", signature: "log(msg)", category: "debug", summary: "Log info message" },
    ScriptApiDoc { name: "warn", signature: "warn(msg)", category: "debug", summary: "Log warning message" },
    ScriptApiDoc { name: "song", signature: "song() -> String", category: "song", summary: "Current song ID" },
    ScriptApiDoc { name: "bpm", signature: "bpm() -> f64", category: "song", summary: "Current BPM" },
    ScriptApiDoc { name: "time", signature: "time() -> f64", category: "song", summary: "Song position in ms" },
    ScriptApiDoc { name: "step", signature: "step() -> i64", category: "song", summary: "Current step" },
    ScriptApiDoc { name: "beat", signature: "beat() -> i64", category: "song", summary: "Current beat" },
    ScriptApiDoc { name: "beat_f", signature: "beat_f() -> f64", category: "song", summary: "Continuous floating-point beat (song_position_ms / beat_length_ms)" },
    ScriptApiDoc { name: "step_f", signature: "step_f() -> f64", category: "song", summary: "Continuous floating-point step (song_position_ms / step_length_ms)" },
    ScriptApiDoc { name: "emit", signature: "emit(name) / emit(name, val)", category: "events", summary: "Dispatch HookKind::Custom(name) across loaded scripts" },
    ScriptApiDoc { name: "hp", signature: "hp() / set_hp(v) / add_hp(d)", category: "gameplay", summary: "Health control" },
    ScriptApiDoc { name: "score", signature: "score() / set_score(v) / add_score(d)", category: "gameplay", summary: "Score control" },
    ScriptApiDoc { name: "cam_zoom", signature: "cam_zoom() / set_zoom(z) / zoom(z, s, ease)", category: "camera", summary: "Camera zoom and tweens" },
    ScriptApiDoc { name: "cam_turn", signature: "set_cam_angle(deg) / cam_turn(deg, s, ease)", category: "camera", summary: "Camera rotation in degrees" },
    ScriptApiDoc { name: "shake", signature: "shake(intensity, secs)", category: "camera", summary: "Camera shake effect" },
    ScriptApiDoc { name: "focus", signature: "focus(target) / focus_at(x, y)", category: "camera", summary: "Focus camera" },
    ScriptApiDoc { name: "bf_anim", signature: "bf_anim(name) / dad_anim(name) / gf_anim(name) / anim(target, name)", category: "character", summary: "Trigger character or prop animation" },
    ScriptApiDoc { name: "lane_pos", signature: "lane_pos(lane, dx, dy) / lane_angle(lane, deg) / tween_lane(lane, prop, to, secs, ease)", category: "modchart", summary: "Per-lane receptor & note offsets/rotations/tweens" },
    ScriptApiDoc { name: "spawn_sprite", signature: "spawn_sprite(id, path, x, y, z) / spawn_rect(id, w, h, hex, x, y, z)", category: "props", summary: "Spawn dynamic world/HUD sprites & rectangles" },
    ScriptApiDoc { name: "tween", signature: "tween(target, prop, to, secs, ease)", category: "tween", summary: "Unified property tween" },
    ScriptApiDoc { name: "after", signature: "after(secs, fn_name) / every(secs, fn_name, count)", category: "timer", summary: "Scheduled script callbacks" },
    ScriptApiDoc { name: "save_get", signature: "save_get(k) / save_set(k, v) / save_flush()", category: "store", summary: "Mod-scoped persistent storage" },
    ScriptApiDoc { name: "lerp", signature: "lerp(a, b, t) -> f64", category: "math", summary: "Linear interpolation a + (b - a) * t" },
    ScriptApiDoc { name: "clamp", signature: "clamp(v, lo, hi) -> f64", category: "math", summary: "Clamp value v between lo and hi" },
    ScriptApiDoc { name: "remap", signature: "remap(v, in_lo, in_hi, out_lo, out_hi) -> f64", category: "math", summary: "Remap v from [in_lo, in_hi] to [out_lo, out_hi]" },
    ScriptApiDoc { name: "sin_beat", signature: "sin_beat(mult, amp) -> f64", category: "math", summary: "Beat-synced sine wave amp * sin(beat_f() * mult * TAU)" },
    ScriptApiDoc { name: "cos_beat", signature: "cos_beat(mult, amp) -> f64", category: "math", summary: "Beat-synced cosine wave amp * cos(beat_f() * mult * TAU)" },
    ScriptApiDoc { name: "set_adjust_color", signature: "set_adjust_color(target, hue, sat, bright, contrast)", category: "shader", summary: "Apply AdjustColor HSB/contrast shader to a character or prop" },
    ScriptApiDoc { name: "set_drop_shadow", signature: "set_drop_shadow(target, hex, angle, dist, str, thr)", category: "shader", summary: "Apply Funkin rim DropShadow shader to a character or prop" },
    ScriptApiDoc { name: "set_wiggle", signature: "set_wiggle(target, effect, speed, freq, amp)", category: "shader", summary: "Apply Wiggle distortion shader (dreamy/wavy/heat_h/heat_v/flag)" },
    ScriptApiDoc { name: "set_rain", signature: "set_rain(scale, intensity, hex)", category: "shader", summary: "Configure fullscreen camera rain post-effect" },
    ScriptApiDoc { name: "clear_shader", signature: "clear_shader(target)", category: "shader", summary: "Remove active shader from a character, prop, or camera" },
];
