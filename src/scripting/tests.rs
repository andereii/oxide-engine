use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::mods::{ModRegistry, ModStore};
use crate::utils::constants::SCRIPT_EXT_RHAI;

use super::api::{ScriptCommand, ScriptStateSnapshot, TweenProp};
use super::backend::ScriptSource;
use super::events::{HookKind, ScriptEvent};
use super::host::ScriptHost;

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "oxide_script_host_{label}_{}_{}",
            std::process::id(),
            seq
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp test dir");
        Self(dir)
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn make_context_and_finalize_preserve_store_across_frames_and_reuse_registry_arc() {
    let mut registry = ModRegistry::discover(PathBuf::new(), PathBuf::new());
    let mut store = ModStore::default();
    let seed = 7_i64;
    store.set("global", "seed", serde_json::json!(seed));

    let mut host = ScriptHost::with_registry(registry.clone());
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "counter_script",
            "test_mod",
            r#"
                fn on_update(e) {
                    let next = if save_has("hits") { save_get("hits") + 1 } else { 1 };
                    save_set("hits", next);
                    global_set("last_hits", next);
                    let prev_seed = global_get("seed");
                    global_set("seed_plus_hits", prev_seed + next);
                }
                "#,
        ),
    )
    .expect("inline script should compile");

    let mut ctx1 = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    let reg_arc_1 = Arc::clone(&ctx1.registry);
    let mut ev1 = ScriptEvent::new(HookKind::Update);
    host.dispatch(&HookKind::Update, &mut ev1, &mut ctx1);
    let _ctx1 = host.finalize_context(ctx1, &mut store);

    let first_hit = 1_i64;
    assert!(host.compile_errors.is_empty());
    assert!(host.runtime_errors.is_empty());
    assert_eq!(
        store.get("test_mod", "hits"),
        Some(&serde_json::json!(first_hit))
    );
    assert_eq!(
        store.get("global", "last_hits"),
        Some(&serde_json::json!(first_hit))
    );
    assert_eq!(store.get("global", "seed"), Some(&serde_json::json!(seed)));
    assert_eq!(
        store.get("global", "seed_plus_hits"),
        Some(&serde_json::json!(seed + first_hit))
    );

    let mut ctx2 = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    assert!(Arc::ptr_eq(&reg_arc_1, &ctx2.registry));
    let mut ev2 = ScriptEvent::new(HookKind::Update);
    host.dispatch(&HookKind::Update, &mut ev2, &mut ctx2);
    let _ctx2 = host.finalize_context(ctx2, &mut store);

    let second_hit = first_hit + 1;
    assert!(host.runtime_errors.is_empty());
    assert_eq!(
        store.get("test_mod", "hits"),
        Some(&serde_json::json!(second_hit))
    );
    assert_eq!(
        store.get("global", "last_hits"),
        Some(&serde_json::json!(second_hit))
    );
    assert_eq!(
        store.get("global", "seed_plus_hits"),
        Some(&serde_json::json!(seed + second_hit))
    );

    let _ = registry.reload();
    let ctx3 = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    assert!(!Arc::ptr_eq(&reg_arc_1, &ctx3.registry));
    let _ctx3 = host.finalize_context(ctx3, &mut store);
}

#[test]
fn has_hook_listeners_and_frame_update_work_track_hooks_timers_and_tweens() {
    let registry = ModRegistry::discover(PathBuf::new(), PathBuf::new());
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(registry.clone());

    assert!(!host.has_hook_listeners(&HookKind::Update));
    assert!(!host.has_hook_listeners(&HookKind::BeatHit));
    assert!(!host.has_frame_update_work());

    let delay_s = 0.5_f64;
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "timer_tween_script",
            "test_mod",
            format!(
                r#"
                fn on_create(e) {{
                    after({delay_s}, "on_timer");
                    tween("cam", "zoom", 1.25, {delay_s});
                }}
                fn on_timer() {{
                    global_set("timer_fired", true);
                }}
                fn on_beat(e) {{}}
                "#
            ),
        ),
    )
    .expect("script should compile");

    assert!(host.has_hook_listeners(&HookKind::BeatHit));
    assert!(host.has_hook_listeners(&HookKind::Create));
    assert!(!host.has_hook_listeners(&HookKind::Update));
    assert!(!host.has_frame_update_work());

    let mut ctx = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    let mut create_ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut create_ev, &mut ctx);
    let _ = host.finalize_context(ctx, &mut store);

    assert!(host.runtime_errors.is_empty());
    assert!(host.has_frame_update_work());

    let mut tick_ctx = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    host.tick_timers_and_tweens(delay_s + 0.1, &mut tick_ctx);
    let _ = host.finalize_context(tick_ctx, &mut store);

    assert!(host.runtime_errors.is_empty());
    assert_eq!(
        store.get("global", "timer_fired"),
        Some(&serde_json::json!(true))
    );
    assert!(!host.has_frame_update_work());

    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline("update_script", "test_mod", "fn on_update(e) {}"),
    )
    .expect("update script should compile");

    assert!(host.has_hook_listeners(&HookKind::Update));
    assert!(host.has_frame_update_work());

    host.clear_scripts();
    assert!(!host.has_hook_listeners(&HookKind::Update));
    assert!(!host.has_hook_listeners(&HookKind::BeatHit));
    assert!(!host.has_frame_update_work());
}

#[test]
fn clear_scripts_invalidates_backend_module_caches() {
    let temp = TempDirGuard::new("clear_caches");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");
    let modules_dir = assets_dir.join("modules");
    fs::create_dir_all(&modules_dir).expect("create modules dir");

    let initial_val = 10_i64;
    let updated_val = 25_i64;
    let module_path = modules_dir.join("shared_val.rhai");
    fs::write(&module_path, format!("fn num() {{ {initial_val} }}\n")).expect("write module");

    let registry = ModRegistry::discover(assets_dir, mods_dir);
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(registry.clone());

    let code = r#"fn on_update(e) { import "shared_val" as m; global_set("num", m::num()); }"#;
    host.load_source(SCRIPT_EXT_RHAI, ScriptSource::inline("main", "mod_a", code))
        .expect("compile main script");

    let mut ctx1 = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    let mut ev1 = ScriptEvent::new(HookKind::Update);
    host.dispatch(&HookKind::Update, &mut ev1, &mut ctx1);
    let _ = host.finalize_context(ctx1, &mut store);
    assert_eq!(
        store.get("global", "num"),
        Some(&serde_json::json!(initial_val))
    );

    fs::write(&module_path, format!("fn num() {{ {updated_val} }}\n"))
        .expect("update module on disk");

    // Cached module still yields initial_val prior to clear_scripts().
    let mut ctx2 = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    let mut ev2 = ScriptEvent::new(HookKind::Update);
    host.dispatch(&HookKind::Update, &mut ev2, &mut ctx2);
    let _ = host.finalize_context(ctx2, &mut store);
    assert_eq!(
        store.get("global", "num"),
        Some(&serde_json::json!(initial_val))
    );

    host.clear_scripts();
    host.load_source(SCRIPT_EXT_RHAI, ScriptSource::inline("main", "mod_a", code))
        .expect("recompile main script");

    let mut ctx3 = host.make_context(ScriptStateSnapshot::default(), &registry, &mut store);
    let mut ev3 = ScriptEvent::new(HookKind::Update);
    host.dispatch(&HookKind::Update, &mut ev3, &mut ctx3);
    let _ = host.finalize_context(ctx3, &mut store);
    assert_eq!(
        store.get("global", "num"),
        Some(&serde_json::json!(updated_val))
    );
}

#[test]
fn concise_rhai_api_emits_expected_commands_and_snapshot_mutations() {
    let temp = TempDirGuard::new("concise_api");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");
    let mod_root = mods_dir.join("sample_mod");
    fs::create_dir_all(mod_root.join("data")).expect("create mod data dir");
    fs::write(mod_root.join("data").join("alpha.txt"), "ok").expect("write alpha");
    fs::write(mod_root.join("data").join("beta.json"), r#"{"x":7}"#).expect("write beta");
    fs::write(
        mod_root.join("_polymod_meta.json"),
        r#"{"title":"Sample"}"#,
    )
    .expect("write meta");

    let registry = ModRegistry::discover(assets_dir, mods_dir);
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(registry.clone());

    let mut snap = ScriptStateSnapshot::default();
    snap.song_length_ms = 92_500.0;
    snap.keys_down = vec!["Space".into()];
    snap.keys_hit = vec!["A".into()];
    snap.ctrls_down = vec!["ACCEPT".into()];
    snap.ctrls_hit = vec!["NOTE_LEFT".into()];

    let target_x = 120.0_f64;
    let target_y = -40.0_f64;
    let note_id = 42_i64;
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "parity_api_smoke",
            "sample_mod",
            format!(
                r##"
                fn on_create(e) {{
                    let dir_ok = dir_exists("data");
                    let files = ls("data");
                    let parsed = from_json("{{\"k\":9}}");
                    let serialized = to_json(parsed);
                    global_set("vfs_ok", dir_ok && files.len() >= 2 && parsed.k == 9 && serialized.contains("9"));
                    global_set("input_ok", key_down("space") && key_hit("a") && ctrl_down("accept") && ctrl_hit("note_left") && song_len() > 90000.0);

                    anim("bf", "hey");
                    dance("dad", true);
                    prop_pos("stage_bg", {target_x}, {target_y});
                    prop_flip("stage_bg", true, false);
                    prop_z("stage_bg", 7.5);
                    prop_scroll("stage_bg", 0.5, 0.25);
                    prop_vel("stage_bg", 10.0, -20.0, 1.0, 2.0);
                    blend("bf", "add");
                    shader("bf", "shaders/test.wgsl");
                    shader_val("bf", "u0", 0.75);
                    text("t1", "Hello", 10.0, 20.0, 24.0, "#FF0000");
                    hud_text("h1", "Score", 30.0, 40.0, 18.0, "#00FF00");
                    play_snd("bgm", "sounds/menu.ogg", 0.8, true);
                    stop_snd("bgm");
                    seek(1500.0);
                    kill_note({note_id});
                    strum_anim(1, "confirm");
                    splash(2);
                    bar_colors("#AA0000", "#00AA00");
                    set_icon("p1", "pico");
                    popup("sick", 25);
                    screenshot("cap_01");
                    load_song("bopeebo", "hard", "erect");
                }}
                "##
            ),
        ),
    )
    .expect("concise API test script should compile");

    let mut ctx = host.make_context(snap, &registry, &mut store);
    let mut ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut ev, &mut ctx);
    let out = host.finalize_context(ctx, &mut store);

    assert!(host.runtime_errors.is_empty(), "errors: {:?}", host.runtime_errors);
    assert_eq!(store.get("global", "vfs_ok"), Some(&serde_json::json!(true)));
    assert_eq!(store.get("global", "input_ok"), Some(&serde_json::json!(true)));

    let cmds = &out.commands;
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::PlayAnim { target, anim, .. } if target == "bf" && anim == "hey")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::CharDance { target, force } if target == "dad" && *force)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetPropNumber { id, prop: TweenProp::X, value } if id == "stage_bg" && (*value - target_x as f32).abs() < 1e-4)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetPropFlip { id, flip_x: true, flip_y: false } if id == "stage_bg")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetPropZ { id, z } if id == "stage_bg" && (*z - 7.5).abs() < 1e-4)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetPropScroll { id, sx, sy } if id == "stage_bg" && (*sx - 0.5).abs() < 1e-4 && (*sy - 0.25).abs() < 1e-4)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetPropVelocity { id, vx, vy, ax, ay } if id == "stage_bg" && (*vx - 10.0).abs() < 1e-4 && (*vy + 20.0).abs() < 1e-4 && (*ax - 1.0).abs() < 1e-4 && (*ay - 2.0).abs() < 1e-4)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetBlendMode { target, mode } if target == "bf" && mode == "add")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetCustomShader { target, path } if target == "bf" && path == "shaders/test.wgsl")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetShaderParam { target, key, values } if target == "bf" && key == "u0" && (values[0] - 0.75).abs() < 1e-4)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SpawnText { id, hud: false, .. } if id == "t1")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SpawnText { id, hud: true, .. } if id == "h1")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::PlaySound { tag, looped: true, .. } if tag == "bgm")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::StopSound(tag) if tag == "bgm")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SeekSong(ms) if (*ms - 1500.0).abs() < 1e-4)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::KillNote { id } if *id == note_id as usize)));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::StrumAnim { lane: 1, state } if state == "confirm")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SpawnSplash { lane: 2 })));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetBarColors { .. })));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SetIcon { side, char_id } if side == "p1" && char_id == "pico")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::SpawnPopup { judgement, combo: Some(25) } if judgement == "sick")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::TakeScreenshot(Some(name)) if name == "cap_01")));
    assert!(cmds.iter().any(|c| matches!(c, ScriptCommand::LoadSong { song_id, difficulty, variation } if song_id == "bopeebo" && difficulty.as_deref() == Some("hard") && variation.as_deref() == Some("erect"))));
}

#[test]
fn tweens_support_char_sound_text_targets_on_complete_and_timers_pass_args() {
    let registry = ModRegistry::discover(PathBuf::new(), PathBuf::new());
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(registry.clone());

    let start_x = 100.0_f32;
    let end_x = 500.0_f64;
    let payload = "payload_val";
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "tween_timer_callbacks",
            "test_mod",
            format!(
                r#"
                fn on_create(e) {{
                    tween("bf", "x", {end_x}, 0.2, "linear", "on_bf_done");
                    tween("snd:bgm", "vol", 0.25, 0.2, "linear");
                    tween("txt:t1", "alpha", 0.5, 0.2, "linear");
                    after(0.1, "on_timer_arg", "{payload}");
                }}
                fn on_bf_done() {{
                    global_set("tween_done_cb", true);
                }}
                fn on_timer_arg(v) {{
                    global_set("timer_arg_val", v);
                }}
                "#
            ),
        ),
    )
    .expect("tween and timer script should compile");

    let mut snap = ScriptStateSnapshot::default();
    snap.char_positions.insert("bf".into(), [start_x, 200.0]);
    snap.sound_volumes.insert("bgm".into(), 1.0);
    snap.text_alphas.insert("t1".into(), 1.0);

    let mut ctx1 = host.make_context(snap.clone(), &registry, &mut store);
    let mut create_ev = ScriptEvent::new(HookKind::Create);
    host.dispatch(&HookKind::Create, &mut create_ev, &mut ctx1);
    let _ = host.finalize_context(ctx1, &mut store);

    let mut tick_ctx = host.make_context(snap, &registry, &mut store);
    host.tick_timers_and_tweens(0.25, &mut tick_ctx);
    let out = host.finalize_context(tick_ctx, &mut store);

    assert!(host.runtime_errors.is_empty(), "errors: {:?}", host.runtime_errors);
    assert!(out.commands.iter().any(|c| matches!(c, ScriptCommand::SetCharacterPos { target, x, .. } if target == "bf" && (*x - end_x as f32).abs() < 1e-3)));
    assert!(out.commands.iter().any(|c| matches!(c, ScriptCommand::SetSoundParam { tag, volume: Some(v), .. } if tag == "bgm" && (*v - 0.25).abs() < 1e-3)));
    assert!(out.commands.iter().any(|c| matches!(c, ScriptCommand::SetPropNumber { id, prop: TweenProp::Alpha, value } if id == "t1" && (*value - 0.5).abs() < 1e-3)));
    assert_eq!(store.get("global", "tween_done_cb"), Some(&serde_json::json!(true)));
    assert_eq!(store.get("global", "timer_arg_val"), Some(&serde_json::json!(payload)));
}
