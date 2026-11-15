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
