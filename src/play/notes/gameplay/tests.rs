use bevy::prelude::KeyCode;

use crate::input::controls::{Action, Controls};
use crate::play::hud::judgement::{Judgement, miss_score};
use crate::play::notes::SongNoteData;
use crate::play::notes::data::NoteDirection;
use crate::play::notes::strum::{CharNoteSignal, HoldCoverCmd};
use crate::play::scene::PlayState;
use crate::play::types::{PlayStateParams, SongChart};
use crate::utils::constants::{
    COMBO_POPUP_THRESHOLD, HEALTH_MISS_BONUS, HEALTH_STARTING, HIT_WINDOW_MS,
    HOLD_DROP_PENALTY_THRESHOLD_MS, MS_PER_SEC, PBOT1_BAD_THRESHOLD, PBOT1_GOOD_THRESHOLD,
    SCORE_HOLD_DROP_PENALTY_PER_SECOND, SUSTAIN_MIN_REMAINING_MS,
};

const FRAME_DT_S: f64 = 1.0 / 60.0;
const BASE_NOTE_TIME_MS: f64 = 1000.0;

fn key_for_direction(controls: &Controls, dir: NoteDirection) -> KeyCode {
    let action = match dir {
        NoteDirection::Left => Action::NoteLeft,
        NoteDirection::Down => Action::NoteDown,
        NoteDirection::Up => Action::NoteUp,
        NoteDirection::Right => Action::NoteRight,
    };
    controls
        .keys_for(action)
        .first()
        .copied()
        .expect("default controls bind every note direction")
}

fn state_with_note(dir: NoteDirection, time_ms: f64, sustain_length_ms: f64) -> PlayState {
    let mut chart = SongChart::default();
    chart.player_notes.push(SongNoteData {
        id: 0,
        time_ms,
        direction: dir.index(),
        sustain_length_ms,
        kind: None,
        low_priority: false,
        scoreable: true,
    });
    let mut state = PlayState::new(PlayStateParams {
        chart,
        ..Default::default()
    });
    state.start_song();
    state.player_strumline.update_notes(FRAME_DT_S, time_ms, 0.0);
    state
}

fn hit_note_at(
    state: &mut PlayState,
    dir: NoteDirection,
    key: KeyCode,
    press_time_ms: f64,
) {
    state.conductor.update(press_time_ms);
    state.process_notes(FRAME_DT_S);
    state.on_key_press_precise(dir, Some(key), press_time_ms, 0.0);
    state.process_input_queue();
    state.process_notes(FRAME_DT_S);
}

fn release_hold_at(
    state: &mut PlayState,
    dir: NoteDirection,
    key: KeyCode,
    release_time_ms: f64,
) {
    state.on_key_release_precise(dir, Some(key), release_time_ms, 0.0);
    state.conductor.update(release_time_ms);
    state.process_input_queue();
    state.process_notes(FRAME_DT_S);
}

#[test]
fn releasing_hold_within_threshold_does_not_penalize_or_miss_sing() {
    let controls = Controls::default();
    let sustain_len_ms = HOLD_DROP_PENALTY_THRESHOLD_MS * 3.0;
    let grace_remaining_ms =
        (SUSTAIN_MIN_REMAINING_MS + HOLD_DROP_PENALTY_THRESHOLD_MS) * 0.5;
    let initial_combo = COMBO_POPUP_THRESHOLD + 5;

    for dir in NoteDirection::ALL {
        let key = key_for_direction(&controls, dir);
        let mut state = state_with_note(dir, BASE_NOTE_TIME_MS, sustain_len_ms);
        state.tallies.combo = initial_combo;
        hit_note_at(&mut state, dir, key, BASE_NOTE_TIME_MS);
        state.player_strumline.pending_char_events.clear();
        state.player_strumline.pending_hold_covers.clear();

        let combo_after_head = state.tallies.combo;
        let score_before = state.song_score;
        let health_before = state.health;
        let release_ms = BASE_NOTE_TIME_MS + sustain_len_ms - grace_remaining_ms;
        release_hold_at(&mut state, dir, key, release_ms);

        assert_eq!(combo_after_head, initial_combo + 1);
        assert_eq!(state.tallies.combo, combo_after_head, "combo should stay intact");
        assert_eq!(state.song_score, score_before, "score should not be penalized");
        assert_eq!(state.health, health_before, "health should not be penalized");
        assert!(!state.pending_miss_sfx, "miss sfx should not play");
        assert_eq!(state.player_vocals_volume, 1.0, "vocals should stay unmuted");
        assert!(
            !state
                .player_strumline
                .pending_char_events
                .iter()
                .any(|sig| matches!(sig, CharNoteSignal::HoldDrop { .. })),
            "character should not play miss animation inside grace threshold"
        );
        assert!(state.player_strumline.active_holds[dir.index()].is_none());
    }
}

#[test]
fn releasing_hold_after_visual_cutoff_finishes_cleanly_with_end_cover() {
    let controls = Controls::default();
    let sustain_len_ms = HOLD_DROP_PENALTY_THRESHOLD_MS * 3.0;
    let sub_cutoff_remaining_ms = SUSTAIN_MIN_REMAINING_MS * 0.5;

    for dir in NoteDirection::ALL {
        let key = key_for_direction(&controls, dir);
        let mut state = state_with_note(dir, BASE_NOTE_TIME_MS, sustain_len_ms);
        hit_note_at(&mut state, dir, key, BASE_NOTE_TIME_MS);
        state.player_strumline.pending_hold_covers.clear();

        let release_ms = BASE_NOTE_TIME_MS + sustain_len_ms - sub_cutoff_remaining_ms;
        release_hold_at(&mut state, dir, key, release_ms);

        let note = &state.player_strumline.notes[0];
        assert!(!note.hold_dropped);
        assert!(note.hold_killed);
        assert!(
            state
                .player_strumline
                .pending_hold_covers
                .contains(&HoldCoverCmd::End(dir))
        );
    }
}

#[test]
fn releasing_hold_early_above_threshold_breaks_combo_and_penalizes() {
    let controls = Controls::default();
    let sustain_len_ms = HOLD_DROP_PENALTY_THRESHOLD_MS * 3.0;
    let early_remaining_ms = HOLD_DROP_PENALTY_THRESHOLD_MS * 1.5;
    let initial_combo = COMBO_POPUP_THRESHOLD + 2;

    for dir in NoteDirection::ALL {
        let key = key_for_direction(&controls, dir);
        let mut state = state_with_note(dir, BASE_NOTE_TIME_MS, sustain_len_ms);
        state.tallies.combo = initial_combo;
        hit_note_at(&mut state, dir, key, BASE_NOTE_TIME_MS);
        let expected_combo_before = state.tallies.combo;
        state.player_strumline.pending_char_events.clear();
        state.pending_popups.clear();

        let score_before = state.song_score;
        let release_ms = BASE_NOTE_TIME_MS + sustain_len_ms - early_remaining_ms;
        release_hold_at(&mut state, dir, key, release_ms);

        assert_eq!(state.tallies.combo, 0);
        assert!(state.song_score < score_before);
        assert!(state.pending_miss_sfx);
        assert_eq!(state.player_vocals_volume, 0.0);
        assert!(
            state
                .pending_popups
                .iter()
                .any(|p| p.judgement.is_none() && p.combo == Some(0))
        );
        assert!(
            state
                .player_strumline
                .pending_char_events
                .iter()
                .any(|sig| matches!(
                    sig,
                    CharNoteSignal::HoldDrop {
                        direction,
                        is_combo_break: true,
                        combo_before,
                    } if *direction == dir && *combo_before == expected_combo_before
                ))
        );
    }
}

#[test]
fn missing_long_hold_head_also_applies_sustain_score_penalty() {
    let sustain_len_ms = HOLD_DROP_PENALTY_THRESHOLD_MS * 4.0;
    let miss_pos_ms = BASE_NOTE_TIME_MS + HIT_WINDOW_MS + 10.0;

    let mut state = state_with_note(NoteDirection::ALL[0], BASE_NOTE_TIME_MS, sustain_len_ms);
    state.conductor.update(miss_pos_ms);
    state.process_notes(FRAME_DT_S);

    assert_eq!(state.tallies.missed, 1);
    let expected_score =
        f64::from(miss_score()) + SCORE_HOLD_DROP_PENALTY_PER_SECOND * (sustain_len_ms / MS_PER_SEC);
    assert!((state.song_score - expected_score).abs() < 1e-6);
    assert!((state.health - (HEALTH_STARTING + HEALTH_MISS_BONUS)).abs() < 1e-6);
}

#[test]
fn bad_or_shit_hit_triggers_combo_drop_popup_when_combo_ten_or_more() {
    let controls = Controls::default();
    let bad_offset_ms = (PBOT1_GOOD_THRESHOLD + PBOT1_BAD_THRESHOLD) * 0.5;
    let dir = NoteDirection::ALL[0];
    let key = key_for_direction(&controls, dir);

    let mut state = state_with_note(dir, BASE_NOTE_TIME_MS, 0.0);
    state.tallies.combo = COMBO_POPUP_THRESHOLD;
    state.conductor.update(BASE_NOTE_TIME_MS);
    state.process_notes(FRAME_DT_S);
    state.on_key_press_precise(dir, Some(key), BASE_NOTE_TIME_MS + bad_offset_ms, 0.0);
    state.process_input_queue();

    assert_eq!(state.tallies.combo, 0);
    assert!(
        state
            .pending_popups
            .iter()
            .any(|p| p.judgement.is_none() && p.combo == Some(0))
    );
    assert!(
        state
            .pending_popups
            .iter()
            .any(|p| p.judgement == Some(Judgement::Bad) && p.combo.is_none())
    );
}
