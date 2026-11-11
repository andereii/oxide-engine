//! Pure-logic regressions for the play session.
//!
//! The window and real audio are not opened here: the tests drive the same
//! `TickPlanner`/`advance_ticks` path the window system uses. The synthetic
//! death test uses the real `GameplaySimState` with temporary chart/metadata
//! files, so it proves the actual terminal `game_over` event rather than a
//! fake event stream.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::input::keyboard::KeyCode;
use oxide_audio::clock::AudioError;

use super::input::{
    InputRouter, TickPlanner, advance_ticks, direction_for, should_queue_lane_event,
    toggle_paused_state,
};
use super::session::{
    DebugOverlay, FPS_WINDOW_FRAMES, FrameStats, ScoreTracker, VoiceMuteState, last_song_tick,
};
use super::{Difficulty, PlayConfig, PlayError, QuitReason, WindowConfig};
use crate::render::load::LoadedNote;
use crate::slice::{GameplaySimState, InputEvent, TraceEvent};

fn press(sim_ms: u64, direction: i64) -> InputEvent {
    InputEvent::Press { sim_ms, direction }
}

fn release(sim_ms: u64, direction: i64) -> InputEvent {
    InputEvent::Release { sim_ms, direction }
}

#[test]
fn frame_stats_average_only_the_last_sixty_frames() {
    let mut stats = FrameStats::new();
    assert_eq!(stats.average_ms(), None);
    assert_eq!(stats.fps(), None);

    for delta in 1..=FPS_WINDOW_FRAMES {
        stats.record(delta as f64);
    }
    let average = stats.average_ms().expect("window filled");
    assert!((average - 30.5).abs() < 1e-9, "mean of 1..=60");
    let fps = stats.fps().expect("fps");
    assert!((fps - 1000.0 / 30.5).abs() < 1e-9);

    // A second full window evicts every value from the first one.
    for delta in 61..=120 {
        stats.record(delta as f64);
    }
    let average = stats.average_ms().expect("window filled");
    assert!((average - 90.5).abs() < 1e-9, "mean of 61..=120");
    assert!((stats.fps().expect("fps") - 1000.0 / 90.5).abs() < 1e-9);
}

#[test]
fn frame_stats_rejects_non_finite_and_negative_deltas() {
    let mut stats = FrameStats::new();
    stats.record(-1.0);
    stats.record(f64::NAN);
    stats.record(f64::INFINITY);
    assert_eq!(stats.average_ms(), None);
    assert_eq!(stats.fps(), None);

    stats.record(16.0);
    assert_eq!(stats.average_ms(), Some(16.0));
    assert_eq!(stats.fps(), Some(62.5));
}

#[test]
fn debug_overlay_toggle_is_pure_state() {
    let mut overlay = DebugOverlay::default();
    assert!(!overlay.active());
    assert!(overlay.toggle());
    assert!(overlay.active());
    assert!(!overlay.toggle());
    assert!(!overlay.active());
}

#[test]
fn keyboard_mapping_matches_adr_0021() {
    assert_eq!(direction_for(KeyCode::KeyA), Some(0));
    assert_eq!(direction_for(KeyCode::ArrowLeft), Some(0));
    assert_eq!(direction_for(KeyCode::KeyS), Some(1));
    assert_eq!(direction_for(KeyCode::ArrowDown), Some(1));
    assert_eq!(direction_for(KeyCode::KeyW), Some(2));
    assert_eq!(direction_for(KeyCode::ArrowUp), Some(2));
    assert_eq!(direction_for(KeyCode::KeyD), Some(3));
    assert_eq!(direction_for(KeyCode::ArrowRight), Some(3));
    assert_eq!(direction_for(KeyCode::KeyE), None);
    assert_eq!(direction_for(KeyCode::Space), None);
}

#[test]
fn alias_keys_hold_the_lane_until_every_bound_key_is_released() {
    let mut input = InputRouter::new();

    // A and Left both map to lane 0. The first press changes the lane-held
    // predicate; the second alias press does not, but every real press is still
    // forwarded to the sim.
    let transition = input.observe(0, true);
    assert_eq!(transition, Some(true));
    assert!(should_queue_lane_event(true, transition));
    let transition = input.observe(0, true);
    assert_eq!(transition, None);
    assert!(should_queue_lane_event(true, transition));
    assert!(input.is_held(0));
    assert_eq!(input.pause_releases(7), vec![release(7, 0)]);

    // Releasing one alias leaves the lane held: no lane Release is queued.
    let transition = input.observe(0, false);
    assert_eq!(transition, None);
    assert!(!should_queue_lane_event(false, transition));
    assert!(input.is_held(0));
    assert_eq!(input.pause_releases(7), vec![release(7, 0)]);

    // Releasing the last bound key releases the lane.
    let transition = input.observe(0, false);
    assert_eq!(transition, Some(false));
    assert!(should_queue_lane_event(false, transition));
    assert!(!input.is_held(0));
    assert!(input.pause_releases(7).is_empty());

    // Extra releases clamp at zero and produce no lane transition/event.
    let transition = input.observe(0, false);
    assert_eq!(transition, None);
    assert!(!should_queue_lane_event(false, transition));
    assert!(!input.is_held(0));
}

#[test]
fn frame_input_is_stamped_on_the_next_tick_in_message_order() {
    let mut planner = TickPlanner::new();
    assert_eq!(planner.stamp_tick(0.0).expect("valid position"), 0);
    assert_eq!(planner.stamp_tick(0.9).expect("valid position"), 0);

    planner.queue(0, 2, true);
    planner.queue(0, 2, false);

    let mut seen = Vec::new();
    let terminal = advance_ticks(&mut planner, 0.9, u64::MAX, |tick, inputs| {
        seen.push((tick, inputs.to_vec()));
        Ok(false)
    })
    .expect("advance tick 0");
    assert!(!terminal);
    assert_eq!(seen, vec![(0, vec![press(0, 2), release(0, 2)])]);

    // A second render frame inside the same millisecond cannot step tick 0
    // again; the transition is stamped on the next unstepped tick.
    assert_eq!(planner.stamp_tick(0.9).expect("valid position"), 1);
    planner.queue(1, 2, true);
    advance_ticks(&mut planner, 0.9, u64::MAX, |_, _| {
        panic!("no new complete millisecond")
    })
    .expect("repeated sub-ms frame");
    assert_eq!(planner.pending_len(), 1);

    let mut seen = Vec::new();
    advance_ticks(&mut planner, 1.0, u64::MAX, |tick, inputs| {
        seen.push((tick, inputs.to_vec()));
        Ok(false)
    })
    .expect("advance tick 1");
    assert_eq!(seen, vec![(1, vec![press(1, 2)])]);
    assert_eq!(planner.pending_len(), 0);
}

#[test]
fn out_of_order_pending_event_is_a_typed_error() {
    let mut planner = TickPlanner::new();
    planner.queue(0, 0, true);
    let error = planner
        .take_inputs(1)
        .expect_err("a past event must not be delivered");
    assert!(matches!(
        error,
        PlayError::OutOfOrderTick {
            expected: 1,
            got: 0
        }
    ));
}

#[test]
fn clock_rewind_is_a_typed_error_but_same_tick_frame_is_not() {
    let mut planner = TickPlanner::new();
    advance_ticks(&mut planner, 2.0, u64::MAX, |_, _| Ok(false)).expect("advance");
    assert_eq!(planner.next_tick(), 3);

    let error = planner.target_tick(0.0).expect_err("rewind");
    assert!(matches!(
        error,
        PlayError::OutOfOrderTick {
            expected: 3,
            got: 0
        }
    ));
    assert!(
        planner.target_tick(2.0).expect("same tick").is_none(),
        "a frame inside the last stepped millisecond is not a rewind"
    );
}

#[test]
fn pause_clears_pending_and_resume_continues() {
    let mut planner = TickPlanner::new();
    let mut stepped = Vec::new();
    advance_ticks(&mut planner, 0.0, u64::MAX, |tick, _| {
        stepped.push(tick);
        Ok(false)
    })
    .expect("advance tick 0");

    planner.queue(1, 0, true);
    assert!(planner.toggle_pause(Vec::new()));
    assert!(planner.is_paused());
    assert_eq!(planner.pending_len(), 0, "pause drops stale queued input");

    advance_ticks(&mut planner, 50.0, u64::MAX, |_, _| {
        panic!("the sim must not advance while paused")
    })
    .expect("paused advance");
    assert_eq!(planner.next_tick(), 1);

    assert!(!planner.toggle_pause(Vec::new()));
    advance_ticks(&mut planner, 2.0, u64::MAX, |tick, inputs| {
        stepped.push(tick);
        assert!(inputs.is_empty(), "cleared press must not be delivered");
        Ok(false)
    })
    .expect("resume advance");
    assert_eq!(stepped, vec![0, 1, 2]);
}

#[test]
fn pause_clamp_retains_an_already_queued_release() {
    let mut planner = TickPlanner::new();
    planner.queue(1, 2, false);

    assert!(planner.toggle_pause(Vec::new()), "enter pause");
    assert_eq!(
        planner.pending_events(),
        &[release(1, 2)][..],
        "the pre-existing release must survive the pause clamp"
    );
    assert_eq!(planner.pending_len(), 1);

    assert!(!planner.toggle_pause(Vec::new()), "resume");
    assert_eq!(
        planner.pending_events(),
        &[release(1, 2)][..],
        "resume must not consume or reorder the retained release"
    );
}

#[test]
fn pause_synthesises_release_for_physically_held_lane_before_pending_clear() {
    // One up-lane hold at t=0. Pressing W starts it; without a release the
    // sustain would keep ticking. The hold drop after resume is the observable
    // proof that the sim's held[2] flag was cleared by the synthesized release.
    let assets = TempAssets::with_easy_notes(r#"[{ "t": 0, "d": 2, "l": 500 }]"#);
    let mut sim = GameplaySimState::build(&assets.root, "easy").expect("build real sim");
    let mut planner = TickPlanner::new();
    let mut input = InputRouter::new();

    assert_eq!(input.observe(2, true), Some(true), "W presses lane 2");
    assert!(input.is_held(2), "W is physically held");
    planner.queue(0, 2, true);

    let mut first_events = Vec::new();
    advance_ticks(&mut planner, 0.0, u64::MAX, |tick, inputs| {
        first_events.extend(sim.step(tick, inputs));
        Ok(false)
    })
    .expect("press tick 0");
    assert!(first_events.iter().any(|event| matches!(
        event,
        TraceEvent::HoldStart {
            note_direction: 2,
            ..
        }
    )));

    // A stale press for the next tick must be dropped by the pause clamp; the
    // physically held lane contributes exactly one synthesized release.
    planner.queue(1, 1, true);
    let next_tick = planner.next_tick();
    assert_eq!(next_tick, 1);
    assert_eq!(
        input.pause_releases(next_tick),
        vec![release(1, 2)],
        "only the physically held lane is synthesized"
    );
    assert!(toggle_paused_state(&mut planner, &input), "pause is active");
    assert_eq!(
        planner.pending_len(),
        1,
        "only the synthesized release survives the pause clamp"
    );

    assert!(
        !toggle_paused_state(&mut planner, &input),
        "resume clears pause"
    );
    let mut resumed_inputs = Vec::new();
    let mut resumed_events = Vec::new();
    advance_ticks(&mut planner, 3.0, u64::MAX, |tick, inputs| {
        resumed_inputs.push((tick, inputs.to_vec()));
        resumed_events.extend(sim.step(tick, inputs));
        Ok(false)
    })
    .expect("resume ticks");

    assert_eq!(
        resumed_inputs[0],
        (1, vec![release(1, 2)]),
        "the synthesized release is applied on the next unstepped tick"
    );
    assert!(
        resumed_events.iter().any(|event| matches!(
            event,
            TraceEvent::HoldDrop {
                sim: 2,
                note_direction: 2,
                ..
            }
        )),
        "the hold drops at tick 2 after resume, proving the sim's held[2] is false"
    );
}

#[test]
fn terminal_step_stops_later_ticks() {
    let mut planner = TickPlanner::new();
    let mut stepped = Vec::new();
    let terminal = advance_ticks(&mut planner, 10.0, u64::MAX, |tick, _| {
        stepped.push(tick);
        Ok(tick == 5)
    })
    .expect("advance");
    assert!(terminal);
    assert_eq!(stepped, vec![0, 1, 2, 3, 4, 5]);
    assert!(planner.is_terminal());

    let mut called = false;
    let terminal = advance_ticks(&mut planner, 20.0, u64::MAX, |_, _| {
        called = true;
        Ok(false)
    })
    .expect("post-terminal advance");
    assert!(!terminal);
    assert!(!called, "no tick may run after the terminal boundary");
}

#[test]
fn audio_unavailable_reason_is_preserved() {
    let mapped = super::audio::map_error(AudioError::AudioUnavailable(
        "oxide-audio was built without the `real-audio` feature".to_string(),
    ));
    match mapped {
        PlayError::AudioUnavailable { reason } => {
            assert!(reason.contains("real-audio"));
        }
        other => panic!("unexpected mapping: {other:?}"),
    }
}

#[test]
fn difficulty_parser_is_validating() {
    assert_eq!(
        Difficulty::parse("EASY").expect("case fold"),
        Difficulty::Easy
    );
    assert_eq!(
        Difficulty::parse("normal").expect("exact"),
        Difficulty::Normal
    );
    assert!(matches!(
        Difficulty::parse("nightmare"),
        Err(PlayError::UnknownDifficulty(value)) if value == "nightmare"
    ));
}

#[test]
fn song_end_is_the_last_full_chart_tick_plus_hit_window_and_hold_guard() {
    let note = LoadedNote {
        time: 1_000.0,
        direction: 0,
        sustain_ms: 500.0,
        opponent: false,
        note_id: Some(0),
        kind: None,
    };
    // ceil(1500 + 160) + 2
    assert_eq!(last_song_tick(&[note]), 1_662);
    assert_eq!(last_song_tick(&[]), 1);
}

#[test]
fn real_sim_game_over_is_terminal_and_mutes_player_once() {
    let assets = TempAssets::new();
    let mut sim = GameplaySimState::build(&assets.root, "easy").expect("build real sim");
    let mut planner = TickPlanner::new();
    for tick in 0..13 {
        // No notes in the synthetic chart: every press is a ghost press
        // (-0.08 health), so the 13th press leaves health at -0.04 and the
        // next sim tick clamps to zero and dispatches the real game_over.
        planner.queue(tick, 0, true);
    }

    let players = vec!["bf".to_string()];
    let mut tracker = ScoreTracker::new();
    let mut voice = VoiceMuteState::new();
    let mut voice_ticks = Vec::new();
    let mut stepped = Vec::new();
    let terminal = advance_ticks(&mut planner, 13.0, u64::MAX, |tick, inputs| {
        stepped.push(tick);
        let events = sim.step(tick, inputs);
        tracker.observe(&events);
        voice_ticks.extend(voice.volumes_for(tick, &events, &players));
        Ok(tracker.terminal)
    })
    .expect("drive real sim");

    assert!(terminal);
    assert_eq!(stepped, (0..=13).collect::<Vec<_>>());
    assert_eq!(planner.next_tick(), 14);
    assert!(tracker.terminal);
    assert_eq!(tracker.health, 0.0);
    assert_eq!(tracker.score, -130.0, "13 ghost presses at -10 each");
    assert!(matches!(
        sim.record().last(),
        Some(TraceEvent::GameOver { .. })
    ));
    assert_eq!(voice_ticks.len(), 1, "only the mute transition is recorded");
    assert_eq!(
        voice_ticks[0],
        super::VoiceVolumeTick {
            sim_ms: 0,
            character: "bf".to_string(),
            volume: 0.0,
        }
    );

    let mut called = false;
    advance_ticks(&mut planner, 1_000.0, u64::MAX, |_, _| {
        called = true;
        Ok(false)
    })
    .expect("post-game-over advance");
    assert!(!called, "game_over is terminal; no later sim step runs");
}

/// Developer-only window/audio smoke. Ignored by default; when
/// `OXIDE_PLAY_HEADED=1` and `OXIDE_PLAY_ASSETS` are set it builds the window
/// and starts the real loop for one frame (private off-main-thread winit
/// opt-in: Linux/Windows only). Without the `oxide-audio/real-audio` build it
/// stops before `app.run()`, reports the typed unavailable reason, and skips
/// instead of pretending to play. This exercises the window branch only; it is
/// not a timing/latency or acceptance check.
#[test]
#[ignore = "developer-only headed smoke; needs OXIDE_PLAY_HEADED=1, OXIDE_PLAY_ASSETS and oxide-audio/real-audio"]
fn headed_window_one_frame_smoke() {
    if std::env::var("OXIDE_PLAY_HEADED").ok().as_deref() != Some("1") {
        return;
    }
    let Ok(assets_root) = std::env::var("OXIDE_PLAY_ASSETS") else {
        eprintln!("headed smoke skipped: OXIDE_PLAY_ASSETS is not set");
        return;
    };
    let config = PlayConfig {
        assets_root: PathBuf::from(assets_root),
        difficulty: Difficulty::Easy,
        window: WindowConfig::default(),
    };
    match super::session::run(config, Some(1)) {
        Ok(report) => assert_eq!(report.quit_reason, QuitReason::FrameLimit),
        Err(PlayError::AudioUnavailable { reason }) => {
            eprintln!("headed smoke skipped (no real audio): {reason}");
        }
        Err(other) => panic!("headed session failed: {other}"),
    }
}

/// Minimal real-assets shape for `GameplaySimState::build` only (chart +
/// metadata). It never feeds the renderer, so no images/stage/character data
/// are needed. The directory is unique per process/test and removed on drop.
struct TempAssets {
    root: PathBuf,
}

impl TempAssets {
    fn new() -> Self {
        Self::with_easy_notes("[]")
    }

    /// Writes a minimal chart whose `easy` note array is `notes` (raw JSON).
    fn with_easy_notes(notes: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "oxide-play-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let song_dir = root.join("preload/data/songs/bopeebo");
        std::fs::create_dir_all(&song_dir).expect("create temporary asset directories");
        let chart = format!(r#"{{ "version": "2.2.4", "notes": {{ "easy": {notes} }} }}"#);
        std::fs::write(song_dir.join("bopeebo-chart.json"), chart).expect("write temporary chart");
        std::fs::write(
            song_dir.join("bopeebo-metadata.json"),
            r#"{
                "version": "2.2.4",
                "songName": "Bopeebo",
                "playData": {
                    "difficulties": ["easy"],
                    "characters": {
                        "player": "bf",
                        "girlfriend": "gf",
                        "opponent": "dad",
                        "playerVocals": ["bf"],
                        "opponentVocals": ["dad"]
                    },
                    "stage": "mainStage",
                    "noteStyle": "funkin"
                },
                "timeChanges": [{ "t": 0, "bpm": 100 }]
            }"#,
        )
        .expect("write temporary metadata");
        Self { root }
    }
}

impl Drop for TempAssets {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
