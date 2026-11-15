use std::fs;
use std::path::Path;

use bevy::prelude::KeyCode;

use crate::audio::player::{should_resync_track, track_resync_target_ms};
use crate::audio::sound::TrackRole;
use crate::input::controls::{Action, Controls, KeyboardScheme};
use crate::play::judgement::{Judgement, judge_note, score_note};
use crate::play::notes::data::{NoteDirection, SongNoteData};
use crate::play::notes::skin::NoteStyle;
use crate::play::notes::strum::{HoldCoverCmd, Strumline};
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::song::load_song_chart;
use crate::play::stage::raw::parse_stage_raw;
use crate::play::stage::schema::StageCoordMode;
use crate::play::types::{PlayStateParams, PreciseInputEvent, SongChart, SongEventData};
use crate::utils::conductor::Conductor;
use crate::utils::constants::{
    DEFAULT_CAMERA_ZOOM, HEALTH_LERP_FACTOR, HEALTH_MAX, HEALTH_STARTING, SCORE_GHOST_MISS_PENALTY,
    SCORE_HOLD_BONUS_PER_SECOND, SUSTAIN_MIN_REMAINING_MS,
};

fn test_note(id: usize, time_ms: f64, direction: usize, sustain_length_ms: f64) -> SongNoteData {
    SongNoteData {
        id,
        time_ms,
        direction,
        sustain_length_ms,
        kind: None,
        low_priority: false,
        scoreable: true,
    }
}

fn make_playing_state(
    bpm: f64,
    player_notes: Vec<SongNoteData>,
    events: Vec<SongEventData>,
    bot_play_mode: bool,
) -> PlayState {
    let mut chart = SongChart::default();
    chart.bpm = bpm;
    chart.scroll_speed = 1.0;
    chart.length_ms = 60_000.0;
    chart.player_notes = player_notes;
    chart.events = events;

    let mut state = PlayState::new(PlayStateParams {
        chart,
        bot_play_mode,
        ..Default::default()
    });
    state.start_song();
    state
}

#[test]
fn pbot1_score_truncates_and_matches_vslice_thresholds() {
    // Reference: V-Slice Scoring.hx (Std.int truncation) & tests/fixtures/note-native/easy.jsonl
    assert_eq!(score_note(0.0), 500);
    assert_eq!(score_note(4.99), 500);
    // At 5.0ms: sigmoid yields ~499.9998 -> Std.int truncates to 499 (rounding gives 500).
    assert_eq!(score_note(5.0), 499);
    // At 45.0ms: sigmoid yields ~353.9016 -> Std.int truncates to 353 (rounding gives 354).
    assert_eq!(score_note(45.0), 353);
    assert_eq!(score_note(-45.0), 353);
    // At 90.0ms: sigmoid yields ~37.64 -> Std.int truncates to 37 (rounding gives 38).
    assert_eq!(score_note(90.0), 37);
    assert_eq!(score_note(160.0), 9);
    assert_eq!(score_note(160.1), -100);

    assert_eq!(judge_note(45.0), Judgement::Sick);
    assert_eq!(judge_note(45.1), Judgement::Good);
    assert_eq!(judge_note(90.0), Judgement::Good);
    assert_eq!(judge_note(90.1), Judgement::Bad);
    assert_eq!(judge_note(135.0), Judgement::Bad);
    assert_eq!(judge_note(135.1), Judgement::Shit);
    assert_eq!(judge_note(160.0), Judgement::Shit);
    assert_eq!(judge_note(160.1), Judgement::Miss);
}

#[test]
fn ghost_tapping_default_disabled_and_ghost_miss_preserves_combo() {
    // V-Slice desktop build disables FEATURE_GHOST_TAPPING (project.hxp).
    let strumline = Strumline::new(true, 1.0, NoteStyle::default());
    assert!(
        !strumline.ghost_tap_enabled,
        "desktop Strumline should default ghost_tap_enabled = false like V-Slice desktop"
    );

    let mut state = make_playing_state(
        100.0,
        vec![test_note(0, 5000.0, NoteDirection::Down.index(), 0.0)],
        Vec::new(),
        false,
    );
    state.tallies.combo = 7;
    state.tallies.missed = 0;
    state.song_score = 1000.0;
    state.health = HEALTH_STARTING;
    state.player_vocals_volume = 1.0;

    // Move into hit window of the Down note so has_notes_may_hit() == true, then press Left (ghost miss).
    state.conductor.update(4900.0);
    state.player_strumline.update_notes(0.0, 4900.0, 0.0);
    state.process_notes(0.0);
    assert!(state.player_strumline.has_notes_may_hit());

    state.input_press_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Left,
        key_code: Some(KeyCode::KeyA),
        song_position_ms: 4900.0,
        latency_ms: 0.0,
    });
    state.process_input_queue();

    // V-Slice PlayState.ghostNoteMiss: applies -10 score, -0.08 health, mutes player vocals,
    // plays miss SFX, and NEVER increments missed tally or resets combo.
    assert_eq!(state.song_score, 1000.0 + SCORE_GHOST_MISS_PENALTY);
    assert!((state.health - 0.92).abs() < 1e-9);
    assert_eq!(state.player_vocals_volume, 0.0);
    assert!(state.pending_miss_sfx);
    assert_eq!(state.tallies.combo, 7, "ghost miss must not break combo");
    assert_eq!(state.tallies.missed, 0, "ghost miss must not add a missed note tally");
}

#[test]
fn controls_solo_scheme_matches_vslice() {
    let controls = Controls::default();
    assert_eq!(controls.scheme, KeyboardScheme::Solo);

    let left = controls.keys_for(Action::NoteLeft);
    let down = controls.keys_for(Action::NoteDown);
    let up = controls.keys_for(Action::NoteUp);
    let right = controls.keys_for(Action::NoteRight);

    assert!(left.contains(&KeyCode::KeyA));
    assert!(left.contains(&KeyCode::ArrowLeft));
    assert!(
        !left.contains(&KeyCode::KeyD),
        "KeyD must not be bound to NoteLeft"
    );

    assert!(down.contains(&KeyCode::KeyS));
    assert!(down.contains(&KeyCode::ArrowDown));

    assert!(up.contains(&KeyCode::KeyW));
    assert!(up.contains(&KeyCode::ArrowUp));

    assert!(right.contains(&KeyCode::KeyD), "KeyD must be bound to NoteRight");
    assert!(right.contains(&KeyCode::ArrowRight));

    assert!(controls.keys_for(Action::Accept).contains(&KeyCode::KeyZ));
    assert!(controls.keys_for(Action::Back).contains(&KeyCode::KeyX));
    assert!(controls.keys_for(Action::Pause).contains(&KeyCode::KeyP));
}

#[test]
fn hold_note_uses_song_position_caps_early_hit_and_finishes_at_zero() {
    // 0.25x scroll velocity after 500ms decouples visual_song_position from conductor.song_position_ms.
    let mut sv_event = SongEventData::default();
    sv_event.time_ms = 500.0;
    sv_event.kind = "ScrollVelocity".to_string();
    sv_event.scroll_velocity = Some(0.25);
    sv_event.strumline_target = "both".to_string();

    let mut state = make_playing_state(
        100.0,
        vec![test_note(0, 1000.0, NoteDirection::Left.index(), 1000.0)],
        vec![sv_event],
        false,
    );
    let mut playlist = PlayStatePlaylist::default();

    // Hit slightly early at 960ms (-40ms, within Sick window).
    state.conductor.update(960.0);
    state.player_strumline.update_notes(0.0, 960.0, 0.0);
    state.process_notes(0.0);
    state.input_press_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Left,
        key_code: Some(KeyCode::KeyA),
        song_position_ms: 960.0,
        latency_ms: 0.0,
    });
    state.process_input_queue();
    state.process_notes(0.0);

    let initial_hold = state.player_strumline.active_holds[0]
        .expect("hold note should become active on hit");
    assert!(
        (initial_hold.remaining_ms - 1000.0).abs() < 1e-6,
        "early hit must cap remaining_ms to full sustain length (got {})",
        initial_hold.remaining_ms
    );

    // Release key briefly before strum_time (at 980ms <= 1000ms): V-Slice only checks hold drop after strumTime.
    state.input_release_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Left,
        key_code: Some(KeyCode::KeyA),
        song_position_ms: 980.0,
        latency_ms: 0.0,
    });
    state.update(0.020, Some(980.0), &mut playlist);
    let pre_strum_hold = state.player_strumline.active_holds[0]
        .expect("hold must not drop while pos_ms <= strum_time_ms");
    assert!(!pre_strum_hold.dropped);
    assert!(
        pre_strum_hold.remaining_ms <= 1000.0,
        "remaining_ms must stay <= full sustain length before strum_time"
    );

    // Re-press key before crossing strum_time, then advance cleanly to 1200ms (past strum_time).
    state.player_strumline.press_key(NoteDirection::Left, Some(KeyCode::KeyA));
    state.update(0.020, Some(1200.0), &mut playlist);

    // Verify visual_song_position really diverged under ScrollVelocity (should be 675ms vs 1200ms).
    assert!(
        (state.player_strumline.visual_song_position - 1200.0).abs() > 100.0,
        "expected ScrollVelocity to separate visual_song_position from audio pos"
    );
    let mid_hold = state.player_strumline.active_holds[0].expect("hold should remain active at 1200ms");
    assert!(
        (mid_hold.remaining_ms - 800.0).abs() < 1e-6,
        "hold remaining_ms must follow conductor pos_ms (800ms), not visual_song_position (got {})",
        mid_hold.remaining_ms
    );

    // Advance to 1995ms: remaining_ms == 5ms (<= SUSTAIN_MIN_REMAINING_MS 10ms, > 0ms).
    // V-Slice hides visual sprite at <= 10ms, yet keeps hold active & awards bonus until <= 0ms.
    let score_before_tail = state.song_score;
    state.update(0.020, Some(1995.0), &mut playlist);
    assert!(
        SUSTAIN_MIN_REMAINING_MS > 5.0,
        "sanity check that 5ms falls below visual hide cutoff"
    );
    assert!(
        state.player_strumline.notes[0].hold_killed,
        "tail visual should hide at remaining_ms <= 10ms"
    );
    let tail_hold = state.player_strumline.active_holds[0]
        .expect("hold must remain active until remaining_ms <= 0.0, not die early at 10ms");
    assert!((tail_hold.remaining_ms - 5.0).abs() < 1e-6);
    assert!(
        (state.song_score - (score_before_tail + SCORE_HOLD_BONUS_PER_SECOND * 0.020)).abs() < 1e-6,
        "hold bonus must still tick while remaining_ms > 0.0"
    );

    // Advance past 2000ms (remaining_ms <= 0.0): hold completes and emits End cover command.
    state.player_strumline.pending_hold_covers.clear();
    state.update(0.050, Some(2005.0), &mut playlist);
    assert!(
        state.player_strumline.active_holds[0].is_none(),
        "hold must finish when remaining_ms <= 0.0"
    );
    assert!(
        state
            .player_strumline
            .pending_hold_covers
            .contains(&HoldCoverCmd::End(NoteDirection::Left))
    );
}

#[test]
fn hold_drop_threshold_and_opponent_hold_timing_match_vslice() {
    let mut sv_event = SongEventData::default();
    sv_event.time_ms = 500.0;
    sv_event.kind = "ScrollVelocity".to_string();
    sv_event.scroll_velocity = Some(0.25);
    sv_event.strumline_target = "both".to_string();

    let mut chart = SongChart::default();
    chart.bpm = 100.0;
    chart.scroll_speed = 1.0;
    chart.length_ms = 60_000.0;
    chart.events = vec![sv_event];
    chart.player_notes = vec![
        test_note(0, 1000.0, NoteDirection::Left.index(), 1000.0),
        test_note(1, 4000.0, NoteDirection::Down.index(), 500.0),
    ];
    chart.opponent_notes = vec![test_note(2, 1000.0, NoteDirection::Right.index(), 1000.0)];

    let mut state = PlayState::new(PlayStateParams {
        chart,
        ..Default::default()
    });
    state.start_song();
    state.tallies.combo = 5;
    let mut playlist = PlayStatePlaylist::default();

    // Hit first player hold cleanly at 1000ms; opponent also hits hold at 1000ms.
    state.conductor.update(1000.0);
    state.player_strumline.update_notes(0.0, 1000.0, 0.0);
    state.opponent_strumline.update_notes(0.0, 1000.0, 0.0);
    state.process_notes(0.0);
    state.input_press_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Left,
        key_code: Some(KeyCode::KeyA),
        song_position_ms: 1000.0,
        latency_ms: 0.0,
    });
    state.process_input_queue();

    // Advance to 1200ms while still holding Left -> both player & opponent remaining_ms must be 800ms.
    state.update(0.020, Some(1200.0), &mut playlist);
    let opp_hold = state.opponent_strumline.active_holds[NoteDirection::Right.index()]
        .expect("opponent hold should remain active at 1200ms");
    assert!((opp_hold.remaining_ms - 800.0).abs() < 1e-6);

    // Release Left early at 1250ms (remaining_ms = 750ms > 160ms): combo breaks & score penalized.
    let score_before_drop = state.song_score;
    state.input_release_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Left,
        key_code: Some(KeyCode::KeyA),
        song_position_ms: 1250.0,
        latency_ms: 0.0,
    });
    state.update(0.020, Some(1250.0), &mut playlist);
    assert!(state.player_strumline.active_holds[NoteDirection::Left.index()].is_none());
    assert_eq!(state.tallies.combo, 0, "early hold drop (>160ms) breaks combo");
    assert!(state.song_score < score_before_drop);

    // Hit second hold at 4000ms (length 500ms -> ends 4500ms) and release near the end (4400ms, remaining=100ms <= 160ms).
    state.tallies.combo = 4;
    state.conductor.update(4000.0);
    state.player_strumline.update_notes(0.0, 4000.0, 0.0);
    state.process_notes(0.0);
    state.input_press_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Down,
        key_code: Some(KeyCode::KeyS),
        song_position_ms: 4000.0,
        latency_ms: 0.0,
    });
    state.process_input_queue();
    state.update(0.020, Some(4300.0), &mut playlist);

    let score_before_late_release = state.song_score;
    let combo_before_late_release = state.tallies.combo;
    state.input_release_queue.push_back(PreciseInputEvent {
        direction: NoteDirection::Down,
        key_code: Some(KeyCode::KeyS),
        song_position_ms: 4400.0,
        latency_ms: 0.0,
    });
    state.update(0.020, Some(4400.0), &mut playlist);
    assert_eq!(
        state.tallies.combo, combo_before_late_release,
        "releasing a hold with <= 160ms remaining must not break combo"
    );
    assert!(
        state.song_score >= score_before_late_release,
        "releasing a hold with <= 160ms remaining must not penalize score"
    );
}

#[test]
fn audio_resync_threshold_matches_vslice() {
    // Pending or unstarted tracks are never drift-resynced.
    assert_eq!(track_resync_target_ms(false, false, 0.0, 2000.0, 0.0), None);
    assert_eq!(track_resync_target_ms(true, true, 0.0, 2000.0, 0.0), None);

    // Within V-Slice RESYNC_THRESHOLD (40ms): no seek needed.
    assert_eq!(track_resync_target_ms(true, false, 1039.5, 1000.0, 0.0), None);
    assert_eq!(track_resync_target_ms(true, false, 960.5, 1000.0, 0.0), None);

    // Drift > 40ms triggers resync target (accounting for track offset_ms and >= 0 clamp).
    assert_eq!(track_resync_target_ms(true, false, 1045.0, 1000.0, 0.0), Some(1000.0));
    assert_eq!(track_resync_target_ms(true, false, 900.0, 1000.0, 50.0), Some(950.0));

    // Only non-instrumental tracks resync on periodic ticks, and only once cooldown settles.
    assert!(!should_resync_track(TrackRole::Instrumental, 0.0));
    assert!(!should_resync_track(TrackRole::PlayerVocal, 0.25));
    assert!(should_resync_track(TrackRole::PlayerVocal, 0.0));
    assert!(should_resync_track(TrackRole::OpponentVocal, 0.0));
}

#[test]
fn framerate_independence_health_camera_bop_and_audio_lerp() {
    let mut playlist = PlayStatePlaylist::default();

    // 1) Health bar lerp invariance across 60fps vs 240fps over 0.1s (prior to beat 1 at 600ms).
    let mut state_60 = make_playing_state(100.0, Vec::new(), Vec::new(), false);
    let mut state_240 = make_playing_state(100.0, Vec::new(), Vec::new(), false);
    state_60.health = 1.8;
    state_240.health = 1.8;
    state_60.camera.bop_multiplier = 1.05;
    state_240.camera.bop_multiplier = 1.05;
    state_60.camera.hud_zoom = 1.06;
    state_240.camera.hud_zoom = 1.06;

    for _ in 0..6 {
        state_60.update(1.0 / 60.0, None, &mut playlist);
    }
    for _ in 0..24 {
        state_240.update(1.0 / 240.0, None, &mut playlist);
    }

    let expected_health = 1.0 + 0.8 * (1.0 - (1.0 - HEALTH_LERP_FACTOR).powf(6.0));
    assert!(
        (state_60.health_lerp - state_240.health_lerp).abs() < 1e-6,
        "health_lerp drifted between 60fps ({}) and 240fps ({})",
        state_60.health_lerp,
        state_240.health_lerp
    );
    assert!(
        (state_60.health_lerp - expected_health).abs() < 1e-6,
        "health_lerp ({}) != V-Slice 60fps equivalent ({})",
        state_60.health_lerp,
        expected_health
    );

    let expected_bop = 1.0 + 0.05 * 0.95_f64.powf(6.0);
    assert!(
        (state_60.camera.bop_multiplier - state_240.camera.bop_multiplier).abs() < 1e-6,
        "camera bop decay differed between 60fps ({}) and 240fps ({})",
        state_60.camera.bop_multiplier,
        state_240.camera.bop_multiplier
    );
    assert!(
        (state_60.camera.bop_multiplier - expected_bop).abs() < 1e-6,
        "camera bop decay ({}) != V-Slice 0.95^(dt*60) ({})",
        state_60.camera.bop_multiplier,
        expected_bop
    );

    // 2) BotPlay snaps health_lerp to HEALTH_MAX.
    let mut bot_state = make_playing_state(100.0, Vec::new(), Vec::new(), true);
    bot_state.health = 1.0;
    bot_state.health_lerp = 1.0;
    bot_state.update(1.0 / 60.0, None, &mut playlist);
    assert_eq!(bot_state.health_lerp, HEALTH_MAX);

    // 3) Step-modulo camera bop handles fractional zoom_rate (1.5 beats = 6 steps),
    //    adds hud_zoom_intensity * default_hud_zoom, and respects 1.35x cap.
    let mut bop_state = make_playing_state(120.0, Vec::new(), Vec::new(), false); // 125ms per step
    bop_state.camera.zoom_rate = 1.5;
    bop_state.camera.zoom_rate_offset = 0.0;
    bop_state.camera.default_hud_zoom = DEFAULT_CAMERA_ZOOM;
    bop_state.camera.hud_zoom = 1.0;
    bop_state.camera.bop_multiplier = 1.0;

    // Cross into step 6 (pos = 755ms; step 4 at 505ms should NOT bop because zoom_rate=1.5 -> 6 steps).
    bop_state.update(0.016, Some(505.0), &mut playlist);
    assert!(
        (bop_state.camera.bop_multiplier - 1.0).abs() < 1e-3,
        "zoom_rate=1.5 should not bop on step 4 (got {})",
        bop_state.camera.bop_multiplier
    );
    bop_state.camera.hud_zoom = 1.05; // non-default value to prove additive += behavior
    bop_state.update(0.016, Some(755.0), &mut playlist);
    assert!(
        (bop_state.camera.bop_multiplier - bop_state.camera.bop_intensity).abs() < 1e-6,
        "zoom_rate=1.5 must bop on step 6"
    );
    assert!(
        bop_state.camera.hud_zoom > 1.06,
        "step bop must add hud_zoom_intensity * default_hud_zoom rather than overwriting"
    );

    // Cap at 1.35 * default_hud_zoom prevents further camera bop even after 1 frame of decay.
    bop_state.camera.hud_zoom = 1.50 * bop_state.camera.default_hud_zoom;
    bop_state.camera.bop_multiplier = 1.0;
    bop_state.update(0.016, Some(1505.0), &mut playlist); // step 12
    assert!(
        (bop_state.camera.bop_multiplier - 1.0).abs() < 1e-6,
        "camera must not bop when hud_zoom >= 1.35 * default_hud_zoom"
    );

    // 4) Audio clock prediction stays smooth & monotonic across quantized rodio buffer updates,
    //    converges inside 65ms drift threshold, and snaps cleanly outside 65ms.
    let mut quantized_state = make_playing_state(120.0, Vec::new(), Vec::new(), false);
    quantized_state.conductor.update(500.0);
    let dt_240 = 1.0 / 240.0;
    let expected_step_240 = dt_240 * 1000.0;
    let mut prev_pos = quantized_state.conductor.song_position_ms;
    // Six frames where audio cursor stays frozen at 500ms while ~25ms of wall time elapses.
    for _ in 0..6 {
        quantized_state.update(dt_240, Some(500.0), &mut playlist);
        let cur_pos = quantized_state.conductor.song_position_ms;
        assert!(
            (cur_pos - (prev_pos + expected_step_240)).abs() < 1e-6,
            "stagnant audio cursor must integrate delta_ms without stalling ({} -> {})",
            prev_pos,
            cur_pos
        );
        prev_pos = cur_pos;
    }

    // When audio advances to the next mixer chunk (525ms), clock stays continuous without lurching.
    quantized_state.update(dt_240, Some(525.0), &mut playlist);
    assert!(
        quantized_state.conductor.song_position_ms >= prev_pos,
        "fresh mixer update must never move clock backwards"
    );

    // Sustained fast CPU drift (> 1 frame ahead on fresh sample) smoothly trims excess without halting.
    let mut fast_state = make_playing_state(120.0, Vec::new(), Vec::new(), false);
    fast_state.conductor.update(500.0);
    fast_state.update(1.0 / 60.0, Some(490.0), &mut playlist); // seed prev raw = 490
    let before_trim = fast_state.conductor.song_position_ms;
    fast_state.update(1.0 / 60.0, Some(500.0), &mut playlist); // audio advanced, still behind
    assert!(
        fast_state.conductor.song_position_ms > before_trim
            && fast_state.conductor.song_position_ms < before_trim + (1000.0 / 60.0),
        "ahead clock on fresh audio step must trim drift gently while moving forward ({} -> {})",
        before_trim,
        fast_state.conductor.song_position_ms
    );

    // When audio leads by 20ms (< 65ms), clock eases monotonically forward without snapping.
    let mut catchup_state = make_playing_state(120.0, Vec::new(), Vec::new(), false);
    catchup_state.conductor.update(500.0);
    catchup_state.update(1.0 / 60.0, Some(536.666_666_666_7), &mut playlist);
    assert!(
        catchup_state.conductor.song_position_ms > 516.67
            && catchup_state.conductor.song_position_ms < 536.66,
        "ahead audio inside 65ms threshold must ease forward smoothly (got {})",
        catchup_state.conductor.song_position_ms
    );

    // Outside 65ms threshold, conductor snaps to audio time + combined offset.
    catchup_state.update(1.0 / 60.0, Some(700.0), &mut playlist);
    assert!((catchup_state.conductor.song_position_ms - 700.0).abs() < 1e-6);
}

#[test]
fn assets_charts_and_conductor_fixtures_parse_cleanly() {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));

    // Modified stage config (custom "stage" schema vs V-Slice) parses in Center mode.
    let stage_path = manifest_dir.join("assets/gameplay/stages/mainStage/mainStage.json");
    let stage_raw = fs::read_to_string(&stage_path).expect("read mainStage.json");
    let stage = parse_stage_raw("mainStage", &stage_raw).expect("parse mainStage.json");
    assert_eq!(stage.coord_mode, StageCoordMode::Center);
    assert!(!stage.props.is_empty());

    // Modified noteskins load cleanly.
    let funkin_skin = NoteStyle::load("funkin").expect("load funkin noteskin");
    let pixel_skin = NoteStyle::load("pixel").expect("load pixel noteskin");
    assert!(funkin_skin.notes_scale > 0.0 && !funkin_skin.is_pixel);
    assert!(pixel_skin.notes_scale > 0.0 && pixel_skin.is_pixel);

    // Bopeebo chart & conductor step/beat match bopeebo-default fixture baseline.
    let chart = load_song_chart("bopeebo", "hard", "default").expect("load bopeebo hard");
    assert_eq!(chart.bpm, 100.0);
    assert!(!chart.player_notes.is_empty());
    let mut conductor = Conductor::new(chart.bpm);
    assert!((conductor.step_length_ms() - 150.0).abs() < 1e-6);
    assert!((conductor.beat_length_ms() - 600.0).abs() < 1e-6);
    conductor.update(600.0);
    assert_eq!(conductor.current_beat, 1);
    assert_eq!(conductor.current_step, 4);
}
