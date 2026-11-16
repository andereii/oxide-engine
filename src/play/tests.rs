use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::mods::{ModRegistry, ModStore, ScriptScope};
use crate::play::events::notekind::register_scripted_notekind;
use crate::play::events::{append_notekind_sidecar_scripts, apply_notekind_to_play_state};
use crate::play::notes::data::{NoteKind, NoteKindManager, SongNoteData};
use crate::play::notes::skin::NoteStyle;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::song::{append_song_sidecar_scripts, load_song_chart_in};
use crate::play::stage::schema::{StageCoordMode, composite_piece_z, parse_stage_json, stage_layer_z};
use crate::play::types::{PlayStateParams, SongChart};
use crate::scripting::{HookKind, ScriptEvent, ScriptHost, ScriptSource};
use crate::utils::constants::SCRIPT_EXT_RHAI;

use super::preload::{PlayPreloadTracker, PreloadProgress};

struct TempDirGuard(PathBuf);

impl TempDirGuard {
    fn new(label: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "oxide_play_tests_{label}_{}_{}",
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

fn write_test_song_files(
    root: &std::path::Path,
    song_id: &str,
    meta_json: &str,
) {
    let chart_dir = root.join("gameplay").join("songs").join(song_id).join("chart");
    fs::create_dir_all(&chart_dir).expect("create chart dir");
    fs::write(chart_dir.join("meta.json"), meta_json).expect("write meta.json");
    fs::write(
        chart_dir.join("notes.json"),
        r#"{"easy":[{"t":500,"d":0,"l":0,"p":"pl"}],"hard":[{"t":500,"d":0,"l":0,"p":"pl"},{"t":1000,"d":1,"l":0,"p":"pl","k":"mine"}]}"#,
    )
    .expect("write notes.json");
}

#[test]
fn tracker_exposes_default_progress() {
    let tracker = PlayPreloadTracker::default();
    assert!(!tracker.progress().is_complete());
    assert_eq!(tracker.progress().fraction(), 0.0);
}

#[test]
fn preload_progress_fraction_and_completion_require_both_assets_and_audio() {
    let empty_waiting = PreloadProgress {
        loaded_assets: 0,
        total_assets: 0,
        audio_ready: false,
    };
    assert!(!empty_waiting.is_complete());
    assert_eq!(empty_waiting.fraction(), 0.0);

    let total_assets = 3_usize;
    let loaded_assets = total_assets;
    let partial = PreloadProgress {
        loaded_assets,
        total_assets,
        audio_ready: false,
    };
    let expected_partial = loaded_assets as f32 / (total_assets + 1) as f32;
    assert!(!partial.is_complete());
    assert!((partial.fraction() - expected_partial).abs() < 1e-6);

    let done = PreloadProgress {
        loaded_assets,
        total_assets,
        audio_ready: true,
    };
    assert!(done.is_complete());
    assert_eq!(done.fraction(), 1.0);
}

#[test]
fn composite_piece_z_stays_strictly_monotonic_within_f32_layer_band() {
    for layer in (-2..=8).chain([100, 1000, 2000, 3000, 4500, 5000]) {
        let base_z = stage_layer_z(layer, 0);
        let next_layer_z = stage_layer_z(layer + 1, 0);
        assert!(
            base_z > -900.0 && base_z < 900.0,
            "layer={layer} base_z={base_z} exceeded Camera2d safe bounds"
        );
        let mut prev = base_z;
        for piece_idx in 1..=256 {
            let cur = composite_piece_z(base_z, piece_idx);
            assert!(
                cur > prev && cur < next_layer_z && cur > -900.0 && cur < 900.0,
                "z collapsed or overflowed layer={layer} piece={piece_idx}: prev={prev} cur={cur} next={next_layer_z}"
            );
            prev = cur;
        }
    }
}

#[test]
fn song_metadata_parses_scripts_and_script_path_across_layers() {
    let temp = TempDirGuard::new("song_meta_scripts");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");

    write_test_song_files(
        &assets_dir,
        "base_enabled",
        r#"{
            "songName": "Base Enabled",
            "base": {
                "scripts": true,
                "audio": { "bpm": 120 }
            },
            "difficulties": {
                "easy": { "scripts": false },
                "hard": { "scriptPath": "custom/entry.rhai" }
            }
        }"#,
    );
    write_test_song_files(
        &assets_dir,
        "top_enabled",
        r#"{
            "songName": "Top Enabled",
            "script": true,
            "base": { "audio": { "bpm": 140 } }
        }"#,
    );
    write_test_song_files(
        &assets_dir,
        "default_disabled",
        r#"{
            "songName": "Default Disabled",
            "base": { "audio": { "bpm": 110 } }
        }"#,
    );

    let mods = ModRegistry::discover(assets_dir, mods_dir);

    let hard = load_song_chart_in(&mods, "base_enabled", "hard", "default").expect("load hard");
    assert!(hard.scripts);
    assert_eq!(hard.script_path.as_deref(), Some("custom/entry.rhai"));

    let easy = load_song_chart_in(&mods, "base_enabled", "easy", "default").expect("load easy");
    assert!(!easy.scripts);

    let top = load_song_chart_in(&mods, "top_enabled", "hard", "default").expect("load top");
    assert!(top.scripts);
    assert_eq!(top.script_path, None);

    let disabled =
        load_song_chart_in(&mods, "default_disabled", "hard", "default").expect("load disabled");
    assert!(!disabled.scripts);
}

#[test]
fn append_song_sidecar_scripts_discovers_base_variation_and_mod_overrides() {
    let temp = TempDirGuard::new("song_sidecars");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");

    let song_dir = assets_dir.join("gameplay").join("songs").join("duet");
    fs::create_dir_all(&song_dir).expect("create song dir");
    let base_script = song_dir.join("duet.rhai");
    let var_script = song_dir.join("duet-erect.rhai");
    fs::write(&base_script, "fn on_create() {}").expect("write base");
    fs::write(&var_script, "fn on_create() {}").expect("write var");

    let mod_root = mods_dir.join("override_mod");
    let mod_song_dir = mod_root.join("songs").join("duet");
    fs::create_dir_all(&mod_song_dir).expect("create mod song dir");
    fs::write(mod_root.join("_polymod_meta.json"), r#"{"title":"Override"}"#)
        .expect("write manifest");
    let mod_base_script = mod_song_dir.join("duet.rhai");
    fs::write(&mod_base_script, "fn on_create() {}").expect("write mod base");

    let mods = ModRegistry::discover(assets_dir, mods_dir);
    let exts = vec![SCRIPT_EXT_RHAI.to_string()];

    // Disabled chart does not load sidecars
    let disabled_chart = SongChart {
        song_id: "duet".into(),
        scripts: false,
        ..Default::default()
    };
    let mut disabled_out = Vec::new();
    append_song_sidecar_scripts(&mods, &disabled_chart, &exts, &mut disabled_out);
    assert!(disabled_out.is_empty());

    // Enabled chart with variation preserves mod override (no duplicate base) + adds variation sidecar
    let enabled_chart = SongChart {
        song_id: "duet".into(),
        variation: "erect".into(),
        scripts: true,
        ..Default::default()
    };
    let mut discovered = mods.scripts_for_context("duet", None, &[], &[], &[]);
    append_song_sidecar_scripts(&mods, &enabled_chart, &exts, &mut discovered);

    assert_eq!(discovered.len(), 2);
    assert_eq!(discovered[0].mod_id, "override_mod");
    assert_eq!(discovered[1].mod_id, "assets");
    assert_eq!(discovered[1].name, "duet-erect");
    assert!(matches!(discovered[1].scope, ScriptScope::Song(ref s) if s == "duet"));
}

#[test]
fn scripted_notekind_discovery_and_rhai_registration_update_notes_and_tallies() {
    let temp = TempDirGuard::new("notekind_scripts");
    let assets_dir = temp.0.join("assets");
    let mods_dir = temp.0.join("mods");

    let nk_dir = assets_dir.join("gameplay").join("notekinds");
    fs::create_dir_all(&nk_dir).expect("create notekinds dir");
    let mine_script = nk_dir.join("mine.rhai");
    fs::write(
        &mine_script,
        r#"
        fn on_create() {
            register_notekind("mine", #{
                description: "Hurt mine",
                style: "pixel",
                no_anim: true,
                suffix: "-hurt",
                scoreable: false,
                low_priority: true
            });
            notekind("bonus", #{
                description: "Bonus",
                scoreable: true,
                low_priority: false
            });
        }
        "#,
    )
    .expect("write mine.rhai");
    let unused_script = nk_dir.join("unused.rhai");
    fs::write(&unused_script, "fn on_create() {}").expect("write unused.rhai");

    let mods = ModRegistry::discover(assets_dir, mods_dir);
    let exts = vec![SCRIPT_EXT_RHAI.to_string()];
    let mut discovered = mods.scripts_for_context("test", None, &[], &["mine"], &[]);
    append_notekind_sidecar_scripts(&mods, &["mine"], &exts, &mut discovered);
    // Calling twice must remain deduplicated by canonical path
    append_notekind_sidecar_scripts(&mods, &["mine"], &exts, &mut discovered);

    assert_eq!(discovered.len(), 1);
    assert_eq!(discovered[0].name, "mine");
    assert!(matches!(discovered[0].scope, ScriptScope::NoteKind(ref k) if k == "mine"));

    let mut mgr = NoteKindManager::default();
    assert!(!register_scripted_notekind(
        &mut mgr,
        NoteKind {
            id: "   ".into(),
            ..Default::default()
        }
    ));

    let chart = SongChart {
        song_id: "nk_test".into(),
        bpm: 120.0,
        scroll_speed: 1.5,
        player_notes: vec![
            SongNoteData {
                id: 1,
                time_ms: 500.0,
                direction: 0,
                sustain_length_ms: 0.0,
                kind: None,
                low_priority: false,
                scoreable: true,
            },
            SongNoteData {
                id: 2,
                time_ms: 1000.0,
                direction: 1,
                sustain_length_ms: 0.0,
                kind: Some("mine".into()),
                low_priority: false,
                scoreable: true,
            },
        ],
        opponent_notes: vec![SongNoteData {
            id: 3,
            time_ms: 1000.0,
            direction: 2,
            sustain_length_ms: 0.0,
            kind: Some("mine".into()),
            low_priority: false,
            scoreable: false,
        }],
        ..Default::default()
    };
    let mut state = PlayState::new(PlayStateParams {
        chart,
        note_style: NoteStyle::default(),
        ..Default::default()
    });
    assert_eq!(state.tallies.total_notes, 2);

    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(mods.clone());
    host.load_discovered(&discovered);
    assert!(host.compile_errors.is_empty(), "compile errors: {:?}", host.compile_errors);

    let mut playlist = PlayStatePlaylist::default();
    let remaining = state.run_script_hooks(
        vec![(HookKind::Create, ScriptEvent::new(HookKind::Create))],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &mods,
        &mut store,
    );
    assert!(host.runtime_errors.is_empty(), "runtime errors: {:?}", host.runtime_errors);
    assert!(remaining.is_empty());

    let mine_kind = state.note_kinds.get(Some("mine")).expect("mine registered");
    assert!(mine_kind.no_anim);
    assert_eq!(mine_kind.suffix, "-hurt");
    assert_eq!(mine_kind.note_style_id.as_deref(), Some("pixel"));
    assert!(!mine_kind.scoreable);
    assert!(mine_kind.low_priority);
    assert!(state.note_kinds.get(Some("bonus")).is_some());

    assert_eq!(state.tallies.total_notes, 1);
    assert!(state.player_strumline.notes[0].data.scoreable);
    assert!(!state.player_strumline.notes[1].data.scoreable);
    assert!(state.player_strumline.notes[1].data.low_priority);
    assert!(state.opponent_strumline.notes[0].data.low_priority);

    // Direct helper invocation updates existing state cleanly as well
    assert!(apply_notekind_to_play_state(
        &mut state,
        NoteKind {
            id: "mine".into(),
            scoreable: true,
            low_priority: false,
            ..Default::default()
        }
    ));
    assert_eq!(state.tallies.total_notes, 2);
}

#[test]
fn rhai_middlescroll_and_strum_visible_apply_to_strumlines() {
    let registry = ModRegistry::discover(PathBuf::new(), PathBuf::new());
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(registry.clone());
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "strum_visibility_test",
            "test_mod",
            r#"
            fn on_create() {
                middlescroll(true);
                strum_visible("opponent", false);
                strum_visible("player", true);
            }
            "#,
        ),
    )
    .expect("compile strum visibility script");

    let mut state = PlayState::new(PlayStateParams::default());
    let mut playlist = PlayStatePlaylist::default();
    let remaining = state.run_script_hooks(
        vec![(HookKind::Create, ScriptEvent::new(HookKind::Create))],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!(host.runtime_errors.is_empty(), "errors: {:?}", host.runtime_errors);
    assert!(remaining.is_empty());
    assert!(state.player_strumline.middlescroll);
    assert!(state.opponent_strumline.middlescroll);
    assert!(state.opponent_strumline.receptors.iter().all(|r| r.mod_alpha == 0.0));
    assert!(state.player_strumline.receptors.iter().all(|r| r.mod_alpha == 1.0));
}

#[test]
fn main_stage_json_matches_vslice_layout() {
    let raw = include_str!("../../assets/gameplay/stages/mainStage/mainStage.json");
    let stage = parse_stage_json("mainStage", raw).expect("parse mainStage.json");
    assert_eq!(stage.coord_mode, StageCoordMode::TopLeft);
    assert!((stage.camera_zoom - 1.1).abs() < 1e-6);
    assert_eq!(stage.characters.spectator.layer, 100);
    assert_eq!(stage.characters.spectator.position, [751.5, 787.0]);
    assert_eq!(stage.characters.spectator.camera_offset, [0.0, 0.0]);
    assert_eq!(stage.characters.opponent.layer, 200);
    assert_eq!(stage.characters.opponent.position, [335.0, 885.0]);
    assert_eq!(stage.characters.opponent.camera_offset, [150.0, -100.0]);
    assert_eq!(stage.characters.player.layer, 300);
    assert_eq!(stage.characters.player.position, [989.5, 885.0]);
    assert_eq!(stage.characters.player.camera_offset, [-100.0, -100.0]);
    assert_eq!(stage.props.len(), 3);
    assert_eq!(stage.props[0].name, "stageBack");
    assert_eq!(stage.props[1].name, "stageFront");
    assert_eq!(stage.props[2].name, "stageCurtains");
}

#[test]
fn stress_chart_loads_with_picospeaker_tracks_preserved() {
    let registry = ModRegistry::discover(PathBuf::from("assets"), PathBuf::from("mods"));
    let chart = load_song_chart_in(&registry, "stress", "hard", "default")
        .expect("stress hard chart should load with picospeaker tracks");
    assert!(!chart.player_notes.is_empty());
    assert!(!chart.opponent_notes.is_empty());

    let raw = include_str!("../../assets/gameplay/songs/stress/chart/notes.json");
    let parsed: serde_json::Value = serde_json::from_str(raw).expect("stress notes.json should parse");
    assert_eq!(parsed["picospeaker"].as_array().map(Vec::len), Some(546));
    assert_eq!(parsed["pico-picospeaker"].as_array().map(Vec::len), Some(584));
}
