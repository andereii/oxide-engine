use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{HookKind, ScriptEvent, ScriptHost};
use crate::utils::constants::{
    CONDUCTOR_DRIFT_THRESHOLD_MS, DEFAULT_BPM, HEALTH_STARTING, HIT_WINDOW_MS,
    MOD_MANIFEST_TOML, MS_PER_SEC, SCRIPT_DIR_SONGS, SCRIPT_DIRS_GLOBAL,
};

use super::notes::SongNoteData;
use super::playlist::PlayStatePlaylist;
use super::scene::PlayState;
use super::types::{PlayPhase, PlayStateParams, SongChart};

const COUNTDOWN_BEATS: usize = 4;
const FRAME_DT_S: f64 = 1.0 / 60.0;

#[test]
fn song_clock_never_drags_backwards_when_audio_lags_or_countdown_ends() {
    let mut state = PlayState::new(PlayStateParams::default());
    let mut playlist = PlayStatePlaylist::default();
    let beat_s = state.conductor.beat_length_ms() / MS_PER_SEC;
    let countdown_step_dt_s = beat_s / 18.0;
    let countdown_ticks = COUNTDOWN_BEATS * 20;

    let mut prev = state.conductor.song_position_ms;
    for _ in 0..countdown_ticks {
        state.update(countdown_step_dt_s, None, &mut playlist);
        let cur = state.conductor.song_position_ms;
        assert!(
            cur >= prev - 1e-9,
            "countdown clock moved backwards: {prev} -> {cur}"
        );
        prev = cur;
    }
    assert!(!state.is_in_countdown);
    assert_eq!(state.phase, PlayPhase::Playing);

    let warmup_frames = ((CONDUCTOR_DRIFT_THRESHOLD_MS / MS_PER_SEC) / FRAME_DT_S).ceil() as usize + 2;
    for _ in 0..warmup_frames {
        state.update(FRAME_DT_S, None, &mut playlist);
        let cur = state.conductor.song_position_ms;
        assert!(cur >= prev, "clock moved backwards while waiting for audio");
        prev = cur;
    }
    assert!(prev > CONDUCTOR_DRIFT_THRESHOLD_MS);

    let lagging_audio_ms = (prev - CONDUCTOR_DRIFT_THRESHOLD_MS * 1.5).max(1.0);
    state.update(FRAME_DT_S, Some(lagging_audio_ms), &mut playlist);
    let after_lag = state.conductor.song_position_ms;
    assert!(
        after_lag >= prev,
        "clock dragged backwards on negative audio drift: {prev} -> {after_lag}"
    );
    assert_eq!(state.player_strumline.visual_song_position, after_lag);

    let forward_audio_ms = after_lag + CONDUCTOR_DRIFT_THRESHOLD_MS * 4.0;
    state.update(FRAME_DT_S, Some(forward_audio_ms), &mut playlist);
    assert!((state.conductor.song_position_ms - forward_audio_ms).abs() < 1e-6);
}

struct TempModRoot {
    root: PathBuf,
    assets: PathBuf,
    mods: PathBuf,
}

impl TempModRoot {
    fn new(tag: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!(
            "oxide_play_script_test_{tag}_{}_{}",
            std::process::id(),
            nanos
        ));
        let assets = root.join("assets");
        let mods = root.join("mods");
        fs::create_dir_all(&assets).unwrap();
        fs::create_dir_all(&mods).unwrap();
        Self { root, assets, mods }
    }
}

impl Drop for TempModRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn mod_scripts_drive_play_state_hooks_modcharts_and_cancellable_misses() {
    let tmp = TempModRoot::new("e2e");
    let song_id = "test_mod_track";
    let mod_id = "community_pack";
    let lane_dx = 24.0_f64;
    let lane_dy = -12.0_f64;
    let cam_deg = 15.0_f64;

    let mod_dir = tmp.mods.join(mod_id);
    let global_dir = mod_dir.join(SCRIPT_DIRS_GLOBAL[0]);
    let song_dir = mod_dir.join(SCRIPT_DIR_SONGS).join(song_id);
    fs::create_dir_all(&global_dir).unwrap();
    fs::create_dir_all(&song_dir).unwrap();

    fs::write(
        mod_dir.join(MOD_MANIFEST_TOML),
        format!(
            "id = \"{mod_id}\"\ntitle = \"Community Test\"\npriority = 10\n[config]\nprevent_miss = true\nlane_dx = {lane_dx}\nlane_dy = {lane_dy}\ncam_deg = {cam_deg}\n"
        ),
    )
    .unwrap();

    let global_script = r#"
let spawned_extra = false;
let step_calls = 0;

fn on_create() {
    save_set("created", true);
    set_cam_angle(mod_cfg("cam_deg"));
}

fn on_step(ev) {
    step_calls += 1;
    let dx = mod_cfg("lane_dx");
    let dy = mod_cfg("lane_dy");
    lane_pos(0, dx, dy);
    if !spawned_extra && step() >= 1 {
        spawned_extra = true;
        add_note(time() + step_ms() * 4.0, 1, 0.0, "");
    }
    set_score_text("MOD[" + song() + "] STEP " + step().to_string());
}

fn on_miss(ev) {
    if mod_cfg("prevent_miss") == true {
        ev.cancel();
        save_set("miss_canceled", true);
    }
}
"#;
    fs::write(global_dir.join("modchart.rhai"), global_script).unwrap();

    let song_script = r#"
fn on_song_loaded(ev) {
    save_set("song_loaded", ev.get("song"));
    set_middlescroll(true);
}
"#;
    fs::write(song_dir.join("song_fx.rhai"), song_script).unwrap();

    let registry = ModRegistry::discover(tmp.assets.clone(), tmp.mods.clone());
    assert!(registry.active_mod_ids().contains(&mod_id));

    let mut store = ModStore::with_root(&registry.mods_dir);
    let mut host = ScriptHost::with_default_backends();
    let discovered = registry.scripts_for_context(song_id, None, &[], &[], &[]);
    host.load_discovered(&discovered);
    assert!(
        host.compile_errors.is_empty(),
        "unexpected script compile errors: {:?}",
        host.compile_errors
    );
    assert_eq!(host.script_count(), 2);

    let miss_note_time_ms = 200.0;
    let mut chart = SongChart {
        song_id: song_id.to_string(),
        song_name: "Test Mod Track".to_string(),
        bpm: DEFAULT_BPM,
        scroll_speed: 2.0,
        length_ms: 60_000.0,
        ..Default::default()
    };
    chart.player_notes.push(SongNoteData {
        id: 1,
        time_ms: miss_note_time_ms,
        direction: 0,
        sustain_length_ms: 0.0,
        kind: None,
        low_priority: false,
        scoreable: true,
    });

    let mut state = PlayState::new(PlayStateParams {
        chart,
        minimal_mode: true,
        ..Default::default()
    });
    let initial_note_count = state.player_strumline.notes.len();
    assert_eq!(initial_note_count, 1);

    let mut playlist = PlayStatePlaylist::default();
    let init_hooks = vec![
        (HookKind::Create, ScriptEvent::new(HookKind::Create)),
        (
            HookKind::SongLoaded,
            ScriptEvent::new(HookKind::SongLoaded).with_field("song", song_id),
        ),
    ];
    let _ = state.run_script_hooks(
        init_hooks,
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );

    assert_eq!(state.camera.angle_deg, cam_deg as f32);
    assert!(state.player_strumline.middlescroll);
    assert_eq!(store.get(mod_id, "created"), Some(&serde_json::json!(true)));
    assert_eq!(
        store.get(mod_id, "song_loaded"),
        Some(&serde_json::json!(song_id))
    );

    let beat_s = state.conductor.beat_length_ms() / MS_PER_SEC;
    let step_dt = beat_s / 16.0;
    for _ in 0..(COUNTDOWN_BEATS * 18) {
        let _ = state.update_with_scripts(
            step_dt,
            None,
            &mut playlist,
            &mut host,
            &registry,
            &mut store,
        );
    }
    assert!(!state.is_in_countdown);

    // Advance past note miss window without pressing keys so NoteMiss fires and gets canceled.
    let miss_cutoff_ms = miss_note_time_ms + HIT_WINDOW_MS * 2.0;
    let mut safety_frames = 0_usize;
    while state.conductor.song_position_ms < miss_cutoff_ms && safety_frames < 500 {
        let _ = state.update_with_scripts(
            FRAME_DT_S,
            None,
            &mut playlist,
            &mut host,
            &registry,
            &mut store,
        );
        safety_frames += 1;
    }

    assert!(
        host.runtime_errors.is_empty(),
        "unexpected script runtime errors: {:?}",
        host.runtime_errors
    );
    assert_eq!(state.tallies.missed, 0, "on_miss cancel should revert miss count");
    assert!(
        (state.health - HEALTH_STARTING).abs() < 1e-6,
        "on_miss cancel should restore health"
    );
    assert_eq!(
        store.get(mod_id, "miss_canceled"),
        Some(&serde_json::json!(true))
    );
    assert_eq!(
        state.player_strumline.receptors[0].mod_offset,
        [lane_dx as f32, lane_dy as f32]
    );
    assert!(
        state.player_strumline.notes.len() > initial_note_count,
        "add_note from on_step should dynamically append note"
    );
    assert!(state.score_text.starts_with(&format!("MOD[{song_id}] STEP ")));
}
