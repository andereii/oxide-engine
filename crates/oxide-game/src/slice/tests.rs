use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

const METADATA: &str = r#"{
  "version": "2.2.4",
  "songName": "Bopeebo",
  "playData": { "difficulties": ["easy", "normal", "hard"] },
  "timeChanges": [{ "t": 0, "bpm": 100 }]
}"#;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn with_notes(notes: &str) -> Self {
        let chart = format!(
            r#"{{"version":"2.0.0","scrollSpeed":{{"easy":1.2,"normal":1.3,"hard":1.6}},"events":[],"notes":{{"easy":[{notes}]}}}}"#
        );
        Self::with_documents(&chart, METADATA)
    }

    fn with_documents(chart: &str, metadata: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "oxide-game-slice-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let songs = root.join("preload/data/songs/bopeebo");
        fs::create_dir_all(&songs).expect("create fixture directory");
        fs::write(songs.join("bopeebo-chart.json"), chart).expect("write fixture chart");
        fs::write(songs.join("bopeebo-metadata.json"), metadata).expect("write fixture metadata");
        Self { root }
    }

    fn build(&self) -> GameplaySimState {
        GameplaySimState::build(&self.root, "easy").expect("fixture builds")
    }

    fn build_with(&self, difficulty: &str) -> Result<GameplaySimState, SliceError> {
        GameplaySimState::build(&self.root, difficulty)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn press(sim_ms: u64, direction: i64) -> InputEvent {
    InputEvent::Press { sim_ms, direction }
}

fn release(sim_ms: u64, direction: i64) -> InputEvent {
    InputEvent::Release { sim_ms, direction }
}

fn event_ms(event: &InputEvent) -> u64 {
    match event {
        InputEvent::Press { sim_ms, .. } | InputEvent::Release { sim_ms, .. } => *sim_ms,
    }
}

/// Steps every grid millisecond from 0 through `last_ms`, feeding the input
/// events whose own timestamp matches the tick (caller order preserved).
fn run(
    fixture: &Fixture,
    last_ms: u64,
    inputs: &[InputEvent],
) -> (GameplaySimState, Vec<TraceEvent>) {
    let mut sim = fixture.build();
    let mut events = Vec::new();
    for sim_ms in 0..=last_ms {
        let tick: Vec<InputEvent> = inputs
            .iter()
            .filter(|event| event_ms(event) == sim_ms)
            .cloned()
            .collect();
        events.extend(sim.step(sim_ms, &tick));
    }
    (sim, events)
}

fn expected_score(abs_timing: i32) -> i32 {
    if abs_timing > 160 {
        -100
    } else if abs_timing < 5 {
        500
    } else {
        let abs = f64::from(abs_timing);
        (500.0 * (1.0 - 1.0 / (1.0 + (-0.080 * (abs - 54.99)).exp())) + 9.0) as i32
    }
}

fn hits(events: &[TraceEvent]) -> Vec<&TraceEvent> {
    events
        .iter()
        .filter(|event| matches!(event, TraceEvent::Hit { .. }))
        .collect()
}

fn ghosts(events: &[TraceEvent]) -> Vec<&TraceEvent> {
    events
        .iter()
        .filter(|event| matches!(event, TraceEvent::GhostPress { .. }))
        .collect()
}

fn hold_ticks(events: &[TraceEvent]) -> Vec<&TraceEvent> {
    events
        .iter()
        .filter(|event| matches!(event, TraceEvent::HoldTick { .. }))
        .collect()
}

fn hold_drops(events: &[TraceEvent]) -> Vec<&TraceEvent> {
    events
        .iter()
        .filter(|event| matches!(event, TraceEvent::HoldDrop { .. }))
        .collect()
}

fn hold_ends(events: &[TraceEvent]) -> Vec<&TraceEvent> {
    events
        .iter()
        .filter(|event| matches!(event, TraceEvent::HoldEnd { .. }))
        .collect()
}

fn first_hit(events: &[TraceEvent]) -> &TraceEvent {
    let found = hits(events);
    assert_eq!(found.len(), 1, "expected exactly one hit event");
    found[0]
}

#[test]
fn pbot1_judge_and_score_table_matches_the_contract() {
    for (offset, expected_judgement) in [
        (0_i32, "sick"),
        (45, "sick"),
        (46, "good"),
        (90, "good"),
        (91, "bad"),
        (135, "bad"),
        (136, "shit"),
        (160, "shit"),
    ] {
        let fixture = Fixture::with_notes(r#"{"t":1000,"d":0}"#);
        let press_ms = 1000 + u64::try_from(offset).expect("non-negative offset");
        let (_, events) = run(&fixture, press_ms, &[press(press_ms, 0)]);
        let TraceEvent::Hit {
            judgement,
            raw_note_diff,
            note_diff_int,
            score_delta,
            ..
        } = first_hit(&events)
        else {
            unreachable!("first_hit returns a hit");
        };
        assert_eq!(*judgement, expected_judgement, "offset {offset}");
        assert_eq!(*note_diff_int, offset, "offset {offset}");
        assert_eq!(*raw_note_diff, f64::from(offset), "offset {offset}");
        assert_eq!(
            *score_delta,
            expected_score(offset),
            "score at offset {offset}"
        );
    }
}

#[test]
fn full_path_110_ms_late_press_is_bad_with_the_pbot1_sigmoid_score() {
    let fixture = Fixture::with_notes(r#"{"t":1000,"d":0}"#);
    let (_, events) = run(&fixture, 1110, &[press(1110, 0)]);
    let TraceEvent::Hit {
        sim,
        input_ms,
        judgement,
        raw_note_diff,
        note_diff_int,
        score_delta,
        health_after,
        ..
    } = first_hit(&events)
    else {
        unreachable!("first_hit returns a hit");
    };
    assert_eq!(*sim, 1110);
    assert_eq!(*input_ms, 1110.0);
    assert_eq!(*judgement, "bad");
    assert_eq!(*raw_note_diff, 110.0);
    assert_eq!(*note_diff_int, 110);
    assert_eq!(*score_delta, 15, "PBOT1 sigmoid at abs timing 110");
    assert!((*health_after - (HEALTH_STARTING + HEALTH_BAD_BONUS)).abs() < 1e-12);
}

#[test]
fn negative_offsets_truncate_toward_zero_before_pbot1_abs() {
    // Exactly ten ms early: the signed raw offset reaches the trace.
    let fixture = Fixture::with_notes(r#"{"t":1000,"d":0}"#);
    let (_, events) = run(&fixture, 990, &[press(990, 0)]);
    let TraceEvent::Hit {
        sim,
        input_ms,
        judgement,
        raw_note_diff,
        note_diff_int,
        score_delta,
        ..
    } = first_hit(&events)
    else {
        unreachable!("first_hit returns a hit");
    };
    assert_eq!(*sim, 990);
    assert_eq!(*input_ms, 990.0);
    assert_eq!(*judgement, "sick");
    assert_eq!(*raw_note_diff, -10.0);
    assert_eq!(*note_diff_int, -10);
    assert_eq!(*score_delta, 495);

    // -10.5 ms truncates toward zero to -10; a floor conversion would produce
    // -11 and change the PBOT1 score. Both cases stay inside the hit window.
    let fixture = Fixture::with_notes(r#"{"t":1000.5,"d":0}"#);
    let (_, events) = run(&fixture, 990, &[press(990, 0)]);
    let TraceEvent::Hit {
        judgement,
        raw_note_diff,
        note_diff_int,
        score_delta,
        ..
    } = first_hit(&events)
    else {
        unreachable!("first_hit returns a hit");
    };
    assert_eq!(*judgement, "sick");
    assert_eq!(*raw_note_diff, -10.5);
    assert_eq!(*note_diff_int, -10);
    assert_eq!(*score_delta, 495);
}

#[test]
fn exact_160_press_takes_the_hit_path_and_breaks_the_combo() {
    let fixture = Fixture::with_notes(r#"{"t":900,"d":0},{"t":1000,"d":0}"#);
    let (_, events) = run(&fixture, 1300, &[press(900, 0), press(1160, 0)]);
    let found = hits(&events);
    assert_eq!(found.len(), 2);
    let TraceEvent::Hit {
        sim,
        input_ms,
        note_id,
        judgement,
        raw_note_diff,
        note_diff_int,
        score_delta,
        health_before,
        health_after,
        combo_before,
        combo_after,
        combo_max,
        tallies,
        ..
    } = found[1]
    else {
        unreachable!("hits() returns hit variants");
    };
    assert_eq!(*sim, 1160);
    assert_eq!(*input_ms, 1160.0);
    assert_eq!(*note_id, 1);
    assert_eq!(*judgement, "shit");
    assert_eq!(*raw_note_diff, 160.0);
    assert_eq!(*note_diff_int, 160);
    assert_eq!(*score_delta, 9);
    assert_eq!(*health_before, 1.03);
    assert!((*health_after - (health_before - 0.02)).abs() < 1e-12);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0);
    assert_eq!(*combo_max, 1);
    assert_eq!(
        *tallies,
        TallyCounts {
            sick: 1,
            shit: 1,
            total_notes_hit: 2,
            ..TallyCounts::default()
        }
    );
    assert!(
        !events
            .iter()
            .any(|event| { matches!(event, TraceEvent::Miss { note_id: 1, .. }) }),
        "the 160 ms hit removes the note"
    );
}

#[test]
fn exact_161_press_takes_the_hit_path_with_miss_judgement_and_increments_combo() {
    let fixture = Fixture::with_notes(r#"{"t":900,"d":0},{"t":1000,"d":0}"#);
    let (_, events) = run(&fixture, 1300, &[press(900, 0), press(1161, 0)]);
    let found = hits(&events);
    assert_eq!(found.len(), 2);
    let TraceEvent::Hit {
        sim,
        input_ms,
        note_id,
        judgement,
        raw_note_diff,
        note_diff_int,
        score_delta,
        health_before,
        health_after,
        combo_before,
        combo_after,
        combo_max,
        tallies,
        ..
    } = found[1]
    else {
        unreachable!("hits() returns hit variants");
    };
    assert_eq!(*sim, 1161);
    assert_eq!(*input_ms, 1161.0);
    assert_eq!(*note_id, 1);
    assert_eq!(*judgement, "miss");
    assert_eq!(*raw_note_diff, 161.0);
    assert_eq!(*note_diff_int, 161);
    assert_eq!(*score_delta, -100);
    assert_eq!(*health_before, *health_after);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 2);
    assert_eq!(*combo_max, 2);
    assert_eq!(
        *tallies,
        TallyCounts {
            sick: 1,
            missed: 1,
            total_notes_hit: 2,
            ..TallyCounts::default()
        }
    );
    assert!(
        !events
            .iter()
            .any(|event| { matches!(event, TraceEvent::Miss { note_id: 1, .. }) }),
        "the 161 ms hit-path miss removes the note"
    );
}

#[test]
fn window_update_at_160_keeps_may_hit_and_misses_at_161() {
    let fixture = Fixture::with_notes(r#"{"t":1000,"d":0}"#);
    let (_, events) = run(&fixture, 1161, &[]);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, TraceEvent::Miss { sim: 1160, .. })),
        "160 is not past the window end"
    );
    let TraceEvent::Miss {
        sim,
        input_ms,
        note_id,
        note_direction,
        judgement,
        raw_note_diff,
        note_diff_int,
        score_delta,
        health_before,
        health_after,
        combo_before,
        combo_after,
        combo_max,
        tallies,
        ..
    } = events
        .iter()
        .find(|event| matches!(event, TraceEvent::Miss { .. }))
        .expect("window miss dispatched")
    else {
        unreachable!("filtered miss variants");
    };
    assert_eq!(*sim, 1161);
    assert_eq!(*input_ms, 0.0);
    assert_eq!(*note_id, 0);
    assert_eq!(*note_direction, 0);
    assert_eq!(*judgement, "miss");
    assert_eq!(*raw_note_diff, 161.0);
    assert_eq!(*note_diff_int, 161);
    assert_eq!(*score_delta, -100);
    assert_eq!(*health_before, 1.0);
    assert!((*health_after - 0.92).abs() < 1e-12);
    assert_eq!(*combo_before, 0);
    assert_eq!(*combo_after, 0);
    assert_eq!(*combo_max, 0);
    assert_eq!(
        *tallies,
        TallyCounts {
            missed: 1,
            total_notes_hit: 1,
            ..TallyCounts::default()
        }
    );
}

#[test]
fn ghost_press_penalties_are_unconditional_while_another_lane_has_a_hold() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":1000},{"t":10,"d":2}"#);
    let (_, events) = run(&fixture, 20, &[press(0, 0), press(10, 1), release(20, 0)]);
    let found = ghosts(&events);
    assert_eq!(found.len(), 1);
    let TraceEvent::GhostPress {
        sim,
        input_ms,
        note_id,
        note_direction,
        judgement,
        score_delta,
        health_change,
        ..
    } = found[0]
    else {
        unreachable!("ghosts() returns ghost variants");
    };
    assert_eq!(*sim, 10);
    assert_eq!(*input_ms, 10.0);
    assert_eq!(*note_id, 0);
    assert_eq!(*note_direction, 1);
    assert_eq!(*judgement, "none");
    assert_eq!(*score_delta, -10.0);
    assert_eq!(*health_change, -0.08);
    assert_eq!(
        hits(&events).len(),
        1,
        "a different lane's may-hit note does not absorb the press"
    );
}

#[test]
fn hold_ticks_accrue_on_the_grid_and_a_full_hold_ends_without_a_drop() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":100}"#);
    let (sim, events) = run(&fixture, 100, &[press(0, 0)]);
    let ticks = hold_ticks(&events);
    assert_eq!(ticks.len(), 100);
    let TraceEvent::HoldTick {
        sim: first_sim,
        health_delta,
        score_delta,
        hold_remaining_ms,
        hold_elapsed_ms,
        ..
    } = ticks[0]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*first_sim, 1);
    assert_eq!(*health_delta, 0.00012);
    assert_eq!(*score_delta, 0.25);
    assert_eq!(*hold_remaining_ms, 99.0);
    assert_eq!(*hold_elapsed_ms, 1.0);
    let TraceEvent::HoldTick {
        sim: last_sim,
        hold_remaining_ms,
        hold_elapsed_ms,
        ..
    } = ticks[99]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*last_sim, 100);
    assert_eq!(*hold_remaining_ms, 0.0);
    assert_eq!(*hold_elapsed_ms, 100.0);
    assert!(hold_drops(&events).is_empty());
    let ends = hold_ends(&events);
    assert_eq!(ends.len(), 1);
    assert!(matches!(ends[0], TraceEvent::HoldEnd { sim: 100, .. }));
    assert!((sim.health - (1.0 + 0.03 + 0.012)).abs() < 1e-12);
    assert!((sim.song_score - 525.0).abs() < 1e-12);
    assert_eq!(sim.record().len(), events.len());
}

#[test]
fn fractional_sustain_emits_the_remainder_tick_before_hold_end() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":10.5}"#);
    let (sim, events) = run(&fixture, 11, &[press(0, 0)]);
    let ticks = hold_ticks(&events);
    assert_eq!(
        ticks.len(),
        11,
        "ten 1 ms ticks plus the 0.5 ms remainder tick"
    );
    let TraceEvent::HoldTick {
        sim: last_sim,
        health_delta,
        score_delta,
        hold_remaining_ms,
        hold_elapsed_ms,
        ..
    } = ticks[10]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*last_sim, 11);
    assert!((*health_delta - 0.00006).abs() < 1e-12);
    assert!((*score_delta - 0.125).abs() < 1e-12);
    assert_eq!(*hold_remaining_ms, 0.0);
    assert_eq!(*hold_elapsed_ms, 10.5);
    let ends = hold_ends(&events);
    assert_eq!(ends.len(), 1);
    assert!(matches!(ends[0], TraceEvent::HoldEnd { sim: 11, .. }));
    // The order of the raw event stream: the fractional remainder tick must
    // sit exactly one position before the hold end.
    let last_tick_index = events
        .iter()
        .rposition(|event| matches!(event, TraceEvent::HoldTick { .. }));
    let first_end_index = events
        .iter()
        .position(|event| matches!(event, TraceEvent::HoldEnd { .. }));
    let (Some(last_tick_index), Some(first_end_index)) = (last_tick_index, first_end_index) else {
        unreachable!("both hold ticks and a hold end exist");
    };
    assert_eq!(
        last_tick_index + 1,
        first_end_index,
        "remainder tick immediately before hold_end"
    );
    assert!((sim.health - (HEALTH_STARTING + HEALTH_SICK_BONUS + 10.5 * 0.00012)).abs() < 1e-12);
    assert!((sim.song_score - (500.0 + 10.5 * 0.25)).abs() < 1e-12);
}

#[test]
fn hold_drop_penalty_is_strictly_gated_at_160_ms() {
    // remaining = 200 ms > 160: the score penalty applies.
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":500}"#);
    let (sim, events) = run(&fixture, 301, &[press(0, 0), release(300, 0)]);
    let drops = hold_drops(&events);
    assert_eq!(drops.len(), 1);
    let TraceEvent::HoldDrop {
        sim: drop_sim,
        input_ms,
        score_penalty,
        health_change,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = drops[0]
    else {
        unreachable!("hold_drops() returns drop variants");
    };
    assert_eq!(*drop_sim, 300);
    assert_eq!(*input_ms, 300.0);
    assert_eq!(*score_penalty, -25.0);
    assert_eq!(*health_change, 0.0);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0);
    assert!(*is_combo_break);
    assert_eq!(hold_ticks(&events).len(), 299);
    assert!((sim.health - (1.0 + 0.03 + 299.0 * 0.00012)).abs() < 1e-12);
    assert!((sim.song_score - (500.0 + 299.0 * 0.25 - 25.0)).abs() < 1e-12);

    // remaining == 160 exactly: no score penalty, but the drop event and
    // combo reset still follow the hard-coded shipped hold-drop branch.
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":460}"#);
    let (_, events) = run(&fixture, 301, &[press(0, 0), release(300, 0)]);
    let drops = hold_drops(&events);
    assert_eq!(drops.len(), 1);
    let TraceEvent::HoldDrop {
        score_penalty,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = drops[0]
    else {
        unreachable!("hold_drops() returns drop variants");
    };
    assert_eq!(*score_penalty, 0.0);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0);
    assert!(*is_combo_break);

    // remaining = 100 ms <= 160: no score penalty either.
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":400}"#);
    let (_, events) = run(&fixture, 301, &[press(0, 0), release(300, 0)]);
    let drops = hold_drops(&events);
    assert_eq!(drops.len(), 1);
    let TraceEvent::HoldDrop { score_penalty, .. } = drops[0] else {
        unreachable!("hold_drops() returns drop variants");
    };
    assert_eq!(*score_penalty, 0.0);

    // Release at the sustain end completes the hold instead of dropping it.
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":300}"#);
    let (_, events) = run(&fixture, 301, &[press(0, 0), release(300, 0)]);
    assert!(hold_drops(&events).is_empty());
    assert_eq!(hold_ends(&events).len(), 1);
    assert_eq!(hold_ticks(&events).len(), 300);
}

#[test]
fn same_tick_press_before_release_starts_then_drops_the_hold() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":500}"#);
    let (_, events) = run(&fixture, 0, &[press(0, 0), release(0, 0)]);
    assert_eq!(events.len(), 3);
    assert!(matches!(&events[0], TraceEvent::Hit { .. }));
    assert!(matches!(&events[1], TraceEvent::HoldStart { .. }));
    let TraceEvent::HoldDrop {
        score_penalty,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = &events[2]
    else {
        panic!("expected hold drop as the third same-tick event: {events:?}");
    };
    assert_eq!(*score_penalty, -62.5);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0);
    assert!(*is_combo_break);
}

#[test]
fn health_is_clamped_and_death_fires_at_the_start_of_the_next_step() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0},{"t":100,"d":1}"#);
    let mut ghosts_at_ten = Vec::new();
    for _ in 0..13 {
        ghosts_at_ten.push(press(10, 2));
    }
    let mut inputs = vec![press(0, 0)];
    inputs.extend(ghosts_at_ten);
    inputs.push(press(12, 1));

    let (sim, events) = run(&fixture, 12, &inputs);

    assert!(
        !events
            .iter()
            .any(|event| matches!(event, TraceEvent::GameOver { sim: 10, .. })),
        "the clamp/death check runs before this tick's events"
    );
    let tick_ten_ghosts: Vec<&TraceEvent> = ghosts(&events)
        .into_iter()
        .filter(|event| matches!(event, TraceEvent::GhostPress { sim: 10, .. }))
        .collect();
    assert_eq!(tick_ten_ghosts.len(), 13);
    let TraceEvent::GhostPress { health_change, .. } = tick_ten_ghosts[12] else {
        unreachable!("ghosts() returns ghost variants");
    };
    assert_eq!(*health_change, -0.08);

    let game_over_index = events
        .iter()
        .position(|event| matches!(event, TraceEvent::GameOver { .. }))
        .expect("game over dispatched at the next step");
    let TraceEvent::GameOver {
        sim: death_sim,
        input_ms,
        note_id,
        note_direction,
        judgement,
        death_count,
        ..
    } = &events[game_over_index]
    else {
        unreachable!("position returns game-over variants");
    };
    assert_eq!(*death_sim, 11);
    assert_eq!(*input_ms, 0.0);
    assert_eq!(*note_id, 0);
    assert_eq!(*note_direction, 0);
    assert_eq!(*judgement, "none");
    assert_eq!(*death_count, 1);
    assert_eq!(sim.death_count, 1);
    assert_eq!(
        game_over_index,
        events.len() - 1,
        "the GameOverSubState boundary stops the play state"
    );

    // The death tick processes no inputs: the press at 12 is not a ghost tap.
    assert!(
        !ghosts(&events)
            .into_iter()
            .any(|event| matches!(event, TraceEvent::GhostPress { sim: 12, .. })),
        "the dead play state stops processing inputs"
    );

    // moveToGameOver: songScore = 0.0, health = HEALTH_STARTING.
    assert!((sim.health - HEALTH_STARTING).abs() < 1e-12);
    assert!(sim.song_score.abs() < 1e-12);
}

#[test]
fn dead_state_is_terminal_and_appends_nothing_on_later_steps() {
    // The first note is a sustain: its hold is active when death arrives, so
    // the detecting tick must not emit any hold bookkeeping (tick/drop/end).
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":10000},{"t":100,"d":1}"#);
    let mut inputs = vec![press(0, 0)];
    for _ in 0..13 {
        inputs.push(press(10, 2));
    }
    // A press on the detecting tick itself: the pre-death hold tick still
    // fires, but the post-death step must not ghost it.
    inputs.push(press(11, 3));

    let (mut sim, events) = run(&fixture, 12, &inputs);
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, TraceEvent::GameOver { .. }))
            .count(),
        1,
        "death is detected once"
    );
    assert!(
        matches!(
            events.last(),
            Some(TraceEvent::GameOver {
                sim: 11,
                death_count: 1,
                ..
            })
        ),
        "the run ends with the only game_over event: {:?}",
        events.last()
    );

    // The active hold must actually be ticking before death: otherwise the
    // late-hold assertion above could pass vacuously.
    let pre_death_ticks = hold_ticks(&events)
        .into_iter()
        .filter(|event| {
            let TraceEvent::HoldTick { sim, .. } = event else {
                unreachable!("hold_ticks() returns tick variants");
            };
            *sim < 11
        })
        .count();
    assert!(
        pre_death_ticks > 0,
        "the sustain ticks before the detecting tick"
    );
    // The active hold only ticks while the play state is alive (steps 1..10).
    let late_hold_events = events.iter().any(|event| {
        let hold_sim = match event {
            TraceEvent::HoldTick { sim, .. }
            | TraceEvent::HoldDrop { sim, .. }
            | TraceEvent::HoldStart { sim, .. }
            | TraceEvent::HoldEnd { sim, .. } => *sim,
            _ => 0,
        };
        matches!(
            event,
            TraceEvent::HoldTick { .. } | TraceEvent::HoldDrop { .. } | TraceEvent::HoldEnd { .. }
        ) && hold_sim >= 11
    });
    assert!(
        !late_hold_events,
        "holds emit nothing on or after the detecting tick"
    );

    // Note 1 would be missed at 261, the press on the detecting tick and the
    // press at 13 would ghost without the substate boundary: none may appear.
    assert!(sim.step(13, &[press(13, 1)]).is_empty());
    assert!(sim.step(261, &[press(261, 1)]).is_empty());
    assert!(
        !ghosts(&events).into_iter().any(|event| matches!(
            event,
            TraceEvent::GhostPress { sim: 11, .. } | TraceEvent::GhostPress { sim: 13, .. }
        )),
        "the detecting tick ignores inputs and dead steps process none"
    );
    assert_eq!(sim.record(), events, "terminal steps append no events");
    assert_eq!(sim.death_count, 1, "death_count does not increment again");
}

#[test]
fn opponent_directions_are_excluded_and_note_ids_are_player_indices() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":4},{"t":10,"d":0}"#);
    let (_, events) = run(&fixture, 10, &[press(0, 4), press(10, 0)]);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, TraceEvent::GhostPress { .. })),
        "opponent input is outside the player trace"
    );
    let TraceEvent::Hit {
        note_id,
        note_direction,
        sim,
        ..
    } = first_hit(&events)
    else {
        unreachable!("first_hit returns a hit");
    };
    assert_eq!(*sim, 10);
    assert_eq!(*note_id, 0);
    assert_eq!(*note_direction, 0);
}

#[test]
fn build_rejects_chart_directions_outside_zero_to_seven() {
    let high = Fixture::with_notes(r#"{"t":0,"d":0},{"t":10,"d":8}"#);
    assert!(matches!(
        high.build_with("easy"),
        Err(SliceError::InvalidNote {
            index: 1,
            detail,
            ..
        }) if detail.contains("direction")
    ));
    let negative = Fixture::with_notes(r#"{"t":0,"d":-1}"#);
    assert!(matches!(
        negative.build_with("easy"),
        Err(SliceError::InvalidNote {
            index: 0,
            detail,
            ..
        }) if detail.contains("direction")
    ));

    // 4..=7 are the opponent side: valid chart lanes, excluded from the
    // player trace rather than rejected as invalid data.
    let opponent = Fixture::with_notes(r#"{"t":0,"d":4},{"t":0,"d":7}"#);
    assert!(opponent.build_with("easy").is_ok());
}

#[test]
fn hold_trace_variants_serialize_in_contract_order() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":500}"#);
    let (_, events) = run(&fixture, 300, &[press(0, 0), release(300, 0)]);

    let start = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldStart { .. }))
        .expect("hold start");
    let start_json = serde_json::to_string(start).expect("serialize hold start");
    assert_eq!(
        start_json,
        r#"{"schema":"1","kind":"hold_start","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0}"#
    );

    let tick = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldTick { sim: 1, .. }))
        .expect("first hold tick");
    let tick_json = serde_json::to_string(tick).expect("serialize hold tick");
    assert_eq!(
        tick_json,
        r#"{"schema":"1","kind":"hold_tick","sim":1,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0,"health_delta":0.00012,"score_delta":0.25,"hold_remaining_ms":499.0,"hold_elapsed_ms":1.0}"#
    );

    let drop = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldDrop { .. }))
        .expect("hold drop");
    let drop_json = serde_json::to_string(drop).expect("serialize hold drop");
    assert_eq!(
        drop_json,
        r#"{"schema":"1","kind":"hold_drop","sim":300,"input_ms":300.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0,"score_penalty":-25.0,"health_change":0.0,"combo_before":1,"combo_after":0,"is_combo_break":true}"#
    );

    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":10}"#);
    let (_, events) = run(&fixture, 10, &[press(0, 0)]);
    let end = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldEnd { .. }))
        .expect("hold end");
    let end_json = serde_json::to_string(end).expect("serialize hold end");
    assert_eq!(
        end_json,
        r#"{"schema":"1","kind":"hold_end","sim":10,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0}"#
    );
}

#[test]
fn hold_start_serializes_the_nonzero_press_timestamp() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":10}"#);
    let (_, events) = run(&fixture, 10, &[press(3, 0)]);
    let start = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldStart { .. }))
        .expect("hold start");
    let TraceEvent::HoldStart { sim, input_ms, .. } = start else {
        unreachable!("filtered hold-start variants");
    };
    assert_eq!(*sim, 3);
    assert_eq!(*input_ms, 3.0);
    assert_eq!(
        serde_json::to_string(start).expect("serialize hold start"),
        r#"{"schema":"1","kind":"hold_start","sim":3,"input_ms":3.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0}"#
    );
}

#[test]
fn serialization_emits_the_shared_header_in_contract_order() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0}"#);
    let (_, events) = run(&fixture, 0, &[press(0, 0)]);
    let json = serde_json::to_string(&events[0]).expect("serialize trace event");
    assert!(
        json.starts_with(
            r#"{"schema":"1","kind":"hit","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"sick","raw_note_diff":0.0,"note_diff_int":0,"score_delta":500,"#
        ),
        "{json}"
    );
}

#[test]
fn build_fails_closed_on_unknown_difficulty_missing_data_and_parse_errors() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0}"#);
    assert!(matches!(
        fixture.build_with("expert"),
        Err(SliceError::UnknownDifficulty(difficulty)) if difficulty == "expert"
    ));
    assert!(matches!(
        fixture.build_with("nightmare"),
        Err(SliceError::UnknownDifficulty(_))
    ));

    let empty_root = std::env::temp_dir().join(format!(
        "oxide-game-slice-missing-{}-{}",
        std::process::id(),
        NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
    ));
    assert!(matches!(
        GameplaySimState::build(&empty_root, "easy"),
        Err(SliceError::MissingAsset { .. })
    ));

    let broken = Fixture::with_documents("{this is not json", METADATA);
    assert!(matches!(
        broken.build_with("easy"),
        Err(SliceError::Parse { .. })
    ));

    let mismatched_metadata = Fixture::with_documents(
        r#"{"version":"2.0.0","notes":{"easy":[]},"events":[]}"#,
        r#"{"version":"2.2.4","songName":"Bopeebo","playData":{"difficulties":["hard"]}}"#,
    );
    assert!(matches!(
        mismatched_metadata.build_with("easy"),
        Err(SliceError::UnknownDifficulty(_))
    ));
}
