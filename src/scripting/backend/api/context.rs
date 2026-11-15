use crate::mods::{ModRegistry, ModStore};
use crate::scripting::value::ScriptValue;
use crate::utils::easing::EaseKind;

use super::commands::ScriptCommand;
use super::state::{ScriptMutableState, ScriptStateSnapshot};
use super::tweens::{PropertyTween, TweenProp, TweenTarget};

/// Maximum nested recursion depth allowed when scripts trigger custom hooks via `emit(...)`.
pub const MAX_CUSTOM_HOOK_DEPTH: usize = 16;


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
