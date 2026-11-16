use std::path::PathBuf;

use crate::cli::LaunchFlags;
use crate::mods::{ModRegistry, ModStore};
use crate::play::camera::events::parse_raw_song_event;
use crate::play::notes::data::{NoteDirection, SongNoteData};
use crate::play::notes::skin::NoteStyle;
use crate::play::notes::strum::CharNoteSignal;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::types::{PauseMode, PlayPhase, PlayStateParams, SongChart};
use crate::scripting::{HookKind, ScriptEvent, ScriptHost, ScriptSource};
use crate::utils::constants::SCRIPT_EXT_RHAI;

use super::capture::{ParityCaptureState, capture_shot_path};

fn build_synthetic_state(note_times_ms: &[f64], bpm: f64) -> PlayState {
    let player_notes: Vec<SongNoteData> = note_times_ms
        .iter()
        .enumerate()
        .map(|(i, &t)| SongNoteData {
            id: i + 1,
            time_ms: t,
            direction: i % 4,
            sustain_length_ms: 0.0,
            kind: None,
            low_priority: false,
            scoreable: true,
        })
        .collect();
    let chart = SongChart {
        song_id: "synth_song".into(),
        song_name: "Synth".into(),
        bpm,
        scroll_speed: 2.0,
        length_ms: note_times_ms.last().copied().unwrap_or(10_000.0) + 5_000.0,
        player_notes,
        events: vec![parse_raw_song_event(
            1000.0,
            "SetCameraBop".into(),
            serde_json::json!({ "intensity": 2.0 }),
        )],
        ..Default::default()
    };
    let mut state = PlayState::new(PlayStateParams {
        chart,
        note_style: NoteStyle::default(),
        ..Default::default()
    });
    state.is_in_countdown = false;
    state.phase = PlayPhase::Playing;
    state
}

#[test]
fn cli_capture_flags_sort_dedup_and_filter_invalid_values() {
    let flags = LaunchFlags::parse([
        "--song:tutorial",
        "--capture-ms:30000,10000,-500,nan,10000.0",
        "--capture-ms",
        "20000,inf,0",
        "--capture-out",
        "/tmp/parity_out",
    ]);

    assert_eq!(flags.song_id.as_deref(), Some("tutorial"));
    assert_eq!(flags.capture_timestamps_ms, vec![0.0, 10_000.0, 20_000.0, 30_000.0]);
    assert_eq!(
        flags.capture_out_dir.as_deref(),
        Some(PathBuf::from("/tmp/parity_out").as_path())
    );
}

#[test]
fn capture_state_progresses_and_gates_exit_until_readback_finishes() {
    let empty = LaunchFlags::default();
    assert!(ParityCaptureState::from_flags(&empty).is_none());

    let flags = LaunchFlags::parse(["--capture-ms:15000,5000", "--capture-out:/tmp/cap"]);
    let mut state = ParityCaptureState::from_flags(&flags).expect("capture state should initialize");
    assert_eq!(state.targets_ms, vec![5_000.0, 15_000.0]);
    assert!(state.next_due_target(4_999.0).is_none());

    let (idx, target_ms, path) = state
        .next_due_target(5_000.0)
        .expect("first target should trigger at 5000ms");
    assert_eq!(idx, 0);
    assert_eq!(target_ms, 5_000.0);
    assert_eq!(path, capture_shot_path(&PathBuf::from("/tmp/cap"), 0, 5_000.0));

    state.next_index += 1;
    state.in_flight = true;
    assert!(state.next_due_target(20_000.0).is_none());
    assert!(!state.should_exit_success(PlayPhase::Playing));
    assert!(!state.should_exit_success(PlayPhase::Ended));

    state.in_flight = false;
    assert!(state.next_due_target(15_000.0).is_some());
    assert!(state.should_exit_success(PlayPhase::Ended));

    state.next_index += 1;
    state.saved_count = 2;
    assert!(state.should_exit_success(PlayPhase::Playing));
}

#[test]
fn hook_cancellation_and_field_mutation_revert_or_override_play_state() {
    let registry = ModRegistry::discover(PathBuf::new(), PathBuf::new());
    let mut store = ModStore::default();
    let mut host = ScriptHost::with_registry(registry.clone());

    let custom_score = 999.0_f64;
    let custom_health = 0.25_f64;
    host.load_source(
        SCRIPT_EXT_RHAI,
        ScriptSource::inline(
            "hook_mutations",
            "test_mod",
            format!(
                r#"
                fn on_song_loaded(e) {{
                    let notes = e.notes;
                    notes.push(#{{ id: 88, time: 3000.0, dir: 2, len: 0.0, is_player: true, scoreable: true }});
                    e.notes = notes;
                    let evs = e.events;
                    evs.push(#{{ time: 2000.0, kind: "ZoomCamera", value: #{{ zoom: 1.5 }} }});
                    e.events = evs;
                }}
                fn on_pause(e) {{ if global_get("cancel_flow") == true {{ e.cancel(); }} }}
                fn on_resume(e) {{ if global_get("cancel_flow") == true {{ e.cancel(); }} }}
                fn on_game_over(e) {{ if global_get("cancel_flow") == true {{ e.cancel(); }} }}
                fn on_countdown_start(e) {{ if global_get("cancel_flow") == true {{ e.cancel(); }} }}
                fn on_song_end(e) {{ if global_get("cancel_flow") == true {{ e.cancel(); }} }}
                fn on_song_retry(e) {{ if global_get("cancel_flow") == true {{ e.cancel(); }} }}
                fn on_note_hit(e) {{
                    if global_get("cancel_hit") == true {{
                        e.cancel();
                    }} else {{
                        e.score = {custom_score};
                        e.health_change = {custom_health};
                        e.splash = false;
                        e.play_anim = false;
                    }}
                }}
                fn on_note_miss(e) {{ if global_get("cancel_miss") == true {{ e.cancel(); }} }}
                fn on_hold_drop(e) {{ if global_get("cancel_miss") == true {{ e.cancel(); }} }}
                fn on_ghost_miss(e) {{ if global_get("cancel_miss") == true {{ e.cancel(); }} }}
                fn on_song_event(e) {{ if global_get("cancel_event") == true {{ e.cancel(); }} }}
                "#
            ),
        ),
    )
    .expect("hook test script should compile");

    let mut state = build_synthetic_state(&[1500.0, 2000.0], 120.0);
    let mut playlist = PlayStatePlaylist::default();

    // 1. SongLoaded mutation updates chart/strumline notes and song_events
    let orig_loaded = ScriptEvent::new(HookKind::SongLoaded)
        .with_field(
            "notes",
            crate::scripting::ScriptValue::List(vec![]),
        )
        .with_field("events", crate::scripting::ScriptValue::List(vec![]));
    let _ = state.run_script_hooks(
        vec![(HookKind::SongLoaded, orig_loaded)],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert_eq!(state.player_strumline.notes.len(), 1);
    assert_eq!(state.player_strumline.notes[0].data.id, 88);
    assert_eq!(state.song_events.len(), 1);
    assert_eq!(state.song_events[0].kind, "ZoomCamera");

    // 2. Flow hook cancellations revert Pause/Resume/GameOver/CountdownStart/SongEnd/SongRetry
    store.set("global", "cancel_flow", serde_json::json!(true));
    state.pause(PauseMode::Standard);
    let hooks = std::mem::take(&mut state.pending_hooks);
    state.run_script_hooks(
        hooks,
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert_eq!(state.phase, PlayPhase::Playing);

    state.phase = PlayPhase::Paused(PauseMode::Standard);
    state.resume();
    let hooks = std::mem::take(&mut state.pending_hooks);
    state.run_script_hooks(
        hooks,
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert_eq!(state.phase, PlayPhase::Paused(PauseMode::Standard));

    state.phase = PlayPhase::Playing;
    state.move_to_game_over();
    let hooks = std::mem::take(&mut state.pending_hooks);
    state.run_script_hooks(
        hooks,
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert_eq!(state.phase, PlayPhase::Playing);
    assert!(!state.is_player_dying);
    assert_eq!(state.death_counter, 0);

    state.start_countdown();
    let hooks = std::mem::take(&mut state.pending_hooks);
    state.run_script_hooks(
        hooks,
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!(state.countdown_paused);

    state.end_song(&mut playlist);
    let hooks = std::mem::take(&mut state.pending_hooks);
    state.run_script_hooks(
        hooks,
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert_eq!(state.phase, PlayPhase::Playing);

    state.needs_reset = true;
    state.run_script_hooks(
        vec![(HookKind::SongRetry, ScriptEvent::new(HookKind::SongRetry))],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!(!state.needs_reset);

    // 3. NoteHit cancellation vs mutation
    let mut state = build_synthetic_state(&[1500.0, 2000.0], 120.0);
    state.health = 1.0;
    state.player_strumline.notes[0].has_been_hit = true;
    state.song_score = 350.0;
    state.health = 1.05;
    state.tallies.sick = 1;
    state.tallies.combo = 1;
    state.tallies.total_notes_hit = 1;
    state.player_strumline.pending_splashes.push(NoteDirection::Left);
    state.player_strumline.pending_char_events.push(CharNoteSignal::Hit {
        direction: NoteDirection::Left,
        note_kind: None,
        good_judgement: Some(true),
        combo: 1,
    });
    store.set("global", "cancel_hit", serde_json::json!(true));
    let cancel_ev = ScriptEvent::new(HookKind::NoteHit)
        .with_field("id", state.player_strumline.notes[0].data.id as i64)
        .with_field("note_idx", 0_i64)
        .with_field("dir", 0_i64)
        .with_field("scoreable", true)
        .with_field("score", 350.0)
        .with_field("health_change", 0.05)
        .with_field("judgement", "sick")
        .with_field("combo_before", 0_i64)
        .with_field("max_combo_before", 0_i64)
        .with_field("does_notesplash", true);
    state.run_script_hooks(
        vec![(HookKind::NoteHit, cancel_ev)],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!(!state.player_strumline.notes[0].has_been_hit);
    assert_eq!(state.tallies.sick, 0);
    assert!((state.song_score - 0.0).abs() < 1e-6);
    assert!(state.player_strumline.pending_splashes.is_empty());
    assert!(state.player_strumline.pending_char_events.is_empty());

    store.set("global", "cancel_hit", serde_json::json!(false));
    state.health = 1.05;
    state.song_score = 350.0;
    state.player_strumline.pending_splashes.push(NoteDirection::Left);
    state.player_strumline.pending_char_events.push(CharNoteSignal::Hit {
        direction: NoteDirection::Left,
        note_kind: None,
        good_judgement: Some(true),
        combo: 1,
    });
    let mut_ev = ScriptEvent::new(HookKind::NoteHit)
        .with_field("id", state.player_strumline.notes[0].data.id as i64)
        .with_field("note_idx", 0_i64)
        .with_field("dir", 0_i64)
        .with_field("scoreable", true)
        .with_field("score", 350.0)
        .with_field("health_change", 0.05)
        .with_field("does_notesplash", true);
    state.run_script_hooks(
        vec![(HookKind::NoteHit, mut_ev)],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!((state.song_score - custom_score).abs() < 1e-4);
    assert!((state.health - (1.0 + custom_health)).abs() < 1e-4);
    assert!(state.player_strumline.pending_splashes.is_empty());
    assert!(state.player_strumline.pending_char_events.is_empty());

    // 4. NoteMiss, HoldDrop, GhostMiss, SongEvent cancellations
    store.set("global", "cancel_miss", serde_json::json!(true));
    store.set("global", "cancel_event", serde_json::json!(true));
    state.health = 0.8;
    state.song_score = 100.0;
    state.tallies.missed = 1;
    state.pending_miss_sfx = true;
    state.player_strumline.pending_char_events.push(CharNoteSignal::Miss {
        direction: NoteDirection::Down,
        combo_before: 4,
    });
    state.run_script_hooks(
        vec![(
            HookKind::NoteMiss,
            ScriptEvent::new(HookKind::NoteMiss)
                .with_field("dir", 1_i64)
                .with_field("health_change", -0.1)
                .with_field("score_change", -50.0)
                .with_field("combo_before", 4_i64)
                .with_field("max_combo_before", 4_i64),
        )],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!((state.health - 0.9).abs() < 1e-6);
    assert!((state.song_score - 150.0).abs() < 1e-6);
    assert_eq!(state.tallies.combo, 4);
    assert_eq!(state.tallies.missed, 0);
    assert!(!state.pending_miss_sfx);

    state.health = 0.7;
    state.song_score = 200.0;
    state.pending_miss_sfx = true;
    state.run_script_hooks(
        vec![(
            HookKind::HoldDrop,
            ScriptEvent::new(HookKind::HoldDrop)
                .with_field("dir", 2_i64)
                .with_field("health_change", -0.15)
                .with_field("score_change", -75.0)
                .with_field("combo_before", 6_i64)
                .with_field("max_combo_before", 6_i64),
        )],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!((state.health - 0.85).abs() < 1e-6);
    assert!((state.song_score - 275.0).abs() < 1e-6);
    assert_eq!(state.tallies.combo, 6);
    assert!(!state.pending_miss_sfx);

    state.health = 0.6;
    state.song_score = 300.0;
    state.pending_miss_sfx = true;
    state.run_script_hooks(
        vec![(
            HookKind::GhostMiss,
            ScriptEvent::new(HookKind::GhostMiss)
                .with_field("dir", 3_i64)
                .with_field("health_change", -0.1)
                .with_field("score_change", -25.0),
        )],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!((state.health - 0.7).abs() < 1e-6);
    assert!((state.song_score - 325.0).abs() < 1e-6);
    assert!(!state.pending_miss_sfx);

    state.camera.bop_intensity = 9.0;
    state.pending_song_event_reverts.insert(
        7,
        state.capture_song_event_revert(),
    );
    state.camera.bop_intensity = 25.0;
    state.run_script_hooks(
        vec![(
            HookKind::SongEvent,
            ScriptEvent::new(HookKind::SongEvent)
                .with_field("event_idx", 7_i64)
                .with_field("name", "SetCameraBop")
                .with_field("time", 1000.0),
        )],
        false,
        0.0,
        &mut playlist,
        &mut host,
        &registry,
        &mut store,
    );
    assert!((state.camera.bop_intensity - 9.0).abs() < 1e-6);
}

#[test]
fn note_incoming_and_dynamic_note_spawn_hooks_fire_and_seek_resets_window() {
    let mut state = build_synthetic_state(&[500.0, 4000.0], 120.0);
    let mut playlist = PlayStatePlaylist::default();

    state.add_dynamic_note(1200.0, 2, 0.0, Some("custom".into()));
    assert!(
        state
            .pending_hooks
            .iter()
            .any(|(kind, ev)| *kind == HookKind::NoteSpawn && ev.get_str("kind") == Some("custom"))
    );
    state.pending_hooks.clear();

    state.update(0.016, Some(200.0), &mut playlist);
    assert!(
        state
            .pending_hooks
            .iter()
            .any(|(kind, ev)| *kind == HookKind::NoteIncoming && (ev.get_f64("time").unwrap_or(0.0) - 500.0).abs() < 1e-4)
    );

    // Seeking forward skips earlier notes; seeking backward revives notes ahead of target
    state.player_strumline.seek_to(2500.0);
    assert!(state.player_strumline.notes[0].killed);
    assert!(!state.player_strumline.notes.last().expect("last note").killed);

    state.player_strumline.seek_to(100.0);
    assert!(state.player_strumline.notes.iter().all(|n| !n.killed));
    assert_eq!(state.player_strumline.active_start_index, 0);
}

#[test]
fn apply_song_seek_state_exits_countdown_and_replays_skipped_events() {
    let player_notes: Vec<SongNoteData> = [500.0, 2000.0, 35000.0]
        .iter()
        .enumerate()
        .map(|(i, &t)| SongNoteData {
            id: i + 1,
            time_ms: t,
            direction: i % 4,
            sustain_length_ms: 0.0,
            kind: None,
            low_priority: false,
            scoreable: true,
        })
        .collect();
    let chart = SongChart {
        song_id: "synth_seek".into(),
        song_name: "SynthSeek".into(),
        bpm: 120.0,
        scroll_speed: 2.0,
        length_ms: 45_000.0,
        player_notes,
        events: vec![
            parse_raw_song_event(1000.0, "SetCameraBop".into(), serde_json::json!({ "rate": 2 })),
            parse_raw_song_event(40000.0, "SetCameraBop".into(), serde_json::json!({ "rate": 1 })),
        ],
        ..Default::default()
    };
    let mut state = PlayState::new(PlayStateParams {
        chart,
        note_style: NoteStyle::default(),
        ..Default::default()
    });
    assert!(state.is_in_countdown);
    assert_eq!(state.phase, PlayPhase::Countdown);

    super::tick::apply_song_seek_state(&mut state, 30000.0);

    assert!(!state.is_in_countdown);
    assert!(!state.starting_song);
    assert_eq!(state.phase, PlayPhase::Playing);
    assert!((state.conductor.song_position_ms - 30000.0).abs() < 1e-4);
    assert!(state.song_events[0].triggered);
    assert!(!state.song_events[1].triggered);
    assert!((state.camera.zoom_rate - 2.0).abs() < 1e-4);
    assert!(
        state
            .pending_hooks
            .iter()
            .any(|(kind, _)| *kind == HookKind::CountdownEnd)
    );
    assert!(
        state
            .pending_hooks
            .iter()
            .any(|(kind, _)| *kind == HookKind::SongStart)
    );
    assert!(state.player_strumline.notes[0].killed);
    assert!(state.player_strumline.notes[1].killed);
    assert!(!state.player_strumline.notes[2].killed);
}
