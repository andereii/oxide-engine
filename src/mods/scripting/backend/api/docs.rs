use serde::Serialize;

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
    ScriptApiDoc { name: "log", signature: "log(msg) / warn(msg) / err(msg)", category: "debug", summary: "Log runtime message" },
    ScriptApiDoc { name: "song", signature: "song() / song_len() / diff() / stage()", category: "song", summary: "Song metadata and length in ms" },
    ScriptApiDoc { name: "bpm", signature: "bpm() / time() / step() / beat() / beat_f()", category: "song", summary: "Conductor timing queries" },
    ScriptApiDoc { name: "emit", signature: "emit(name) / emit(name, val)", category: "events", summary: "Dispatch HookKind::Custom(name) across scripts" },
    ScriptApiDoc { name: "hp", signature: "hp() / set_hp(v) / add_hp(d)", category: "gameplay", summary: "Health control" },
    ScriptApiDoc { name: "score", signature: "score() / set_score(v) / add_score(d)", category: "gameplay", summary: "Score control" },
    ScriptApiDoc { name: "tallies", signature: "combo() / sicks() / goods() / bads() / shits() / misses() / acc() / rank()", category: "gameplay", summary: "Rating counters and accuracy" },
    ScriptApiDoc { name: "rules", signature: "set_practice(b) / set_ghost_tap(b) / set_cutscene(b) / set_can_pause(b) / lock_keys(b)", category: "gameplay", summary: "Gameplay and pause mode flags" },
    ScriptApiDoc { name: "flow", signature: "end_song() / restart_song() / pause_song() / resume_song() / seek(ms) / countdown(act) / load_song(id)", category: "gameplay", summary: "Song transport, countdown and playlist transitions" },
    ScriptApiDoc { name: "playlist", signature: "story_mode() / set_story_mode(b) / playlist() / set_playlist(arr) / campaign_score()", category: "gameplay", summary: "Story playlist queue inspection and mutation" },
    ScriptApiDoc { name: "cam", signature: "cam_zoom() / set_zoom(z) / zoom(z, s, ease) / pan_cam(x, y, s, ease) / reset_cam()", category: "camera", summary: "Camera zoom, pan and reset" },
    ScriptApiDoc { name: "hud_cam", signature: "hud_zoom() / hud_angle() / set_hud_angle(deg) / fade(hex, s, in, hud) / flash(hex, s)", category: "hud", summary: "HUD camera angle, flash and fades" },
    ScriptApiDoc { name: "icons", signature: "bar_colors(opp, ply) / set_icon(side, id) / icon_scale(side, s) / icon_bop(side, on)", category: "hud", summary: "Health bar colors and icon state" },
    ScriptApiDoc { name: "popups", signature: "popup(judge, combo) / popups_vis(b) / popups_off(x, y)", category: "hud", summary: "Judgement and combo popup control" },
    ScriptApiDoc { name: "char", signature: "anim(t, name) / dance(t, force) / char_pos(t) / char_flip(t, x, y) / swap_char(role, id)", category: "character", summary: "Character animations, transform and runtime swap" },
    ScriptApiDoc { name: "notes", signature: "notes() / add_note(t, dir, len, kind) / kill_note(id) / set_note(id, map)", category: "notes", summary: "Active note inspection and mutation" },
    ScriptApiDoc { name: "strums", signature: "lane_pos(l, x, y) / strum_anim(l, state) / splash(l) / strum_style(side, sc, sp) / vwoosh(side, in)", category: "notes", summary: "Strumline receptor animations, modchart and vwoosh" },
    ScriptApiDoc { name: "props", signature: "spawn_sprite / spawn_sparrow / prop_pos / prop_scroll / prop_flip / prop_z / prop_vel", category: "props", summary: "Dynamic world/HUD props and velocity" },
    ScriptApiDoc { name: "text", signature: "text(id, s, x, y, sz, hex) / hud_text(...) / set_text(id, s) / text_style(id, sz, hex)", category: "text", summary: "World and HUD dynamic text" },
    ScriptApiDoc { name: "subtitles", signature: "subtitle_srt(path) / subtitle_clear()", category: "text", summary: "Timed SRT subtitles on the subtitle HUD" },
    ScriptApiDoc { name: "tween", signature: "tween(target, prop, to, s, ease[, done_fn]) / after(s, fn[, arg]) / every(s, fn, n[, arg])", category: "tween", summary: "Property tweens and scheduled callbacks" },
    ScriptApiDoc { name: "audio", signature: "play_snd(tag, path[, vol, loop, pitch]) / stop_snd(tag) / pause_snd(tag) / snd_vol(tag, v)", category: "audio", summary: "Tagged audio channel playback and control" },
    ScriptApiDoc { name: "input", signature: "key_down(k) / key_hit(k) / ctrl_down(c) / ctrl_hit(c) / screen_size() / screenshot([name])", category: "input", summary: "Keyboard, control, window and capture API" },
    ScriptApiDoc { name: "vfs", signature: "file_exists(p) / dir_exists(p) / ls(p) / read_file(p) / read_merged(p) / from_json(s) / to_json(v)", category: "vfs", summary: "Mod-sandboxed VFS and JSON utilities" },
    ScriptApiDoc { name: "store", signature: "save_get(k) / save_set(k, v) / save_del(k) / save_keys() / save_flush() / global_set(k, v)", category: "store", summary: "Persistent mod-scoped key-value store" },
    ScriptApiDoc { name: "math", signature: "lerp(a, b, t) / clamp(v, lo, hi) / remap(...) / sin_beat(m, a) / cos_beat(m, a) / rand()", category: "math", summary: "Interpolation, beat math and RNG" },
    ScriptApiDoc { name: "shader", signature: "shader(t, path) / shader_val(t, k, v) / shader_vec(...) / blend(t, m) / clear_shader(t)", category: "shader", summary: "Custom WGSL shaders, blend modes and presets" },
];
