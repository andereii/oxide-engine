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
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oxide-game-slice-test-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).expect("create fixture directory");
        Self { root }
    }

    /// Writes one song's chart/metadata pair under the VFS layout. Both stems
    /// are file stems (`bopeebo-chart` / `bopeebo-metadata`, or the suffixed
    /// variation forms).
    fn song_dir(&self, song: &str) -> PathBuf {
        let songs = self.root.join("preload/data/songs").join(song);
        fs::create_dir_all(&songs).expect("create fixture song directory");
        songs
    }

    fn write_chart(&self, song: &str, chart_stem: &str, chart: &str) {
        fs::write(
            self.song_dir(song).join(format!("{chart_stem}.json")),
            chart,
        )
        .expect("write fixture chart");
    }

    fn write_metadata(&self, song: &str, metadata_stem: &str, metadata: &str) {
        fs::write(
            self.song_dir(song).join(format!("{metadata_stem}.json")),
            metadata,
        )
        .expect("write fixture metadata");
    }

    fn write_song(
        &self,
        song: &str,
        chart_stem: &str,
        chart: &str,
        metadata_stem: &str,
        metadata: &str,
    ) {
        self.write_chart(song, chart_stem, chart);
        self.write_metadata(song, metadata_stem, metadata);
    }

    fn with_notes(notes: &str) -> Self {
        let chart = format!(
            r#"{{"version":"2.0.0","scrollSpeed":{{"easy":1.2,"normal":1.3,"hard":1.6}},"events":[],"notes":{{"easy":[{notes}]}}}}"#
        );
        Self::with_documents(&chart, METADATA)
    }

    fn with_documents(chart: &str, metadata: &str) -> Self {
        let fixture = Self::new();
        fixture.write_song(
            "bopeebo",
            "bopeebo-chart",
            chart,
            "bopeebo-metadata",
            metadata,
        );
        fixture
    }

    fn build(&self) -> GameplaySimState {
        GameplaySimState::build(&self.root, "easy").expect("fixture builds")
    }

    fn build_with(&self, difficulty: &str) -> Result<GameplaySimState, SliceError> {
        GameplaySimState::build(&self.root, difficulty)
    }

    fn build_chart(
        &self,
        chart_id: &str,
        difficulty: &str,
    ) -> Result<GameplaySimState, SliceError> {
        GameplaySimState::build_chart(&self.root, chart_id, difficulty)
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

/// Same canonical tick loop as [`run`], but through the schema-2 chart-id
/// builder.
fn run_chart(
    fixture: &Fixture,
    chart_id: &str,
    difficulty: &str,
    last_ms: u64,
    inputs: &[InputEvent],
) -> (GameplaySimState, Vec<TraceEvent>) {
    let mut sim = fixture
        .build_chart(chart_id, difficulty)
        .expect("fixture chart builds");
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

fn opponent_hits(events: &[TraceEvent]) -> Vec<&TraceEvent> {
    events
        .iter()
        .filter(|event| matches!(event, TraceEvent::OpponentHit { .. }))
        .collect()
}

/// The canonical compact JSONL form used by the M5/gameplay-sim digest.
fn serialize_trace(events: &[TraceEvent]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for event in events {
        serde_json::to_writer(&mut bytes, event).expect("serialize trace event");
        bytes.push(b'\n');
    }
    bytes
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
            ..TallyCounts::default()
        },
        "the real applyScore-on-miss bumps only the missed tally"
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
    // Key stays down. Completion is the first integer T >= end+2 (§6.1 item 8).
    let (sim, events) = run(&fixture, 102, &[press(0, 0)]);
    let ticks = hold_ticks(&events);
    assert_eq!(
        ticks.len(),
        101,
        "the hit tick T0 plus every tick through ceil(end); the silent recompute does not accrue"
    );
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
    assert_eq!(*first_sim, 0, "the hit tick itself accrues");
    assert_eq!(*health_delta, 0.00012);
    assert_eq!(*score_delta, 0.25);
    assert_eq!(*hold_remaining_ms, 100.0);
    assert_eq!(*hold_elapsed_ms, 0.0);
    let TraceEvent::HoldTick {
        sim: second_sim,
        hold_remaining_ms: second_remaining,
        ..
    } = ticks[1]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*second_sim, 1);
    assert_eq!(
        *second_remaining, 100.0,
        "T0+1 still sees the hit capture: mirror P == strumTime does not recompute"
    );
    let TraceEvent::HoldTick {
        hold_remaining_ms: third_remaining,
        ..
    } = ticks[2]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*third_remaining, 99.0, "decay starts at T0+2: end - (T-1)");
    let TraceEvent::HoldTick {
        sim: last_sim,
        hold_remaining_ms,
        hold_elapsed_ms,
        ..
    } = ticks[100]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*last_sim, 100);
    assert_eq!(
        *hold_remaining_ms, 1.0,
        "last accrual at ceil(end) observes end - (ceil(end)-1), never zero or negative"
    );
    assert_eq!(*hold_elapsed_ms, 100.0);
    assert!(hold_drops(&events).is_empty());
    let ends = hold_ends(&events);
    assert_eq!(ends.len(), 1);
    assert!(matches!(ends[0], TraceEvent::HoldEnd { sim: 102, .. }));
    assert!((sim.health - (1.0 + 0.03 + 101.0 * 0.00012)).abs() < 1e-12);
    assert!((sim.song_score - (500.0 + 101.0 * 0.25)).abs() < 1e-12);
    assert_eq!(sim.record().len(), events.len());

    // Emission order at the hit tick: hit, hold_start, then the first tick.
    let hit_index = events
        .iter()
        .position(|event| matches!(event, TraceEvent::Hit { .. }))
        .expect("hit row");
    assert!(matches!(
        &events[hit_index + 1],
        TraceEvent::HoldStart { sim: 0, .. }
    ));
    assert!(matches!(
        &events[hit_index + 2],
        TraceEvent::HoldTick { sim: 0, .. }
    ));
}

#[test]
fn hit_tick_accrues_a_fixed_hold_tick_immediately_after_hold_start() {
    let fixture = Fixture::with_notes(r#"{"t":1000,"d":0,"l":50}"#);
    let (_, events) = run(&fixture, 1052, &[press(1000, 0)]);
    let hit_index = events
        .iter()
        .position(|event| matches!(event, TraceEvent::Hit { sim: 1000, .. }))
        .expect("hit row at the press tick");
    assert!(matches!(
        &events[hit_index + 1],
        TraceEvent::HoldStart { sim: 1000, .. }
    ));
    let TraceEvent::HoldTick {
        sim,
        health_delta,
        score_delta,
        hold_remaining_ms,
        hold_elapsed_ms,
        ..
    } = &events[hit_index + 2]
    else {
        panic!("the hit tick must accrue immediately: {events:?}");
    };
    assert_eq!(*sim, 1000);
    assert_eq!(*health_delta, 0.00012);
    assert_eq!(*score_delta, 0.25);
    assert_eq!(*hold_remaining_ms, 50.0);
    assert_eq!(*hold_elapsed_ms, 0.0);
    assert_eq!(
        hold_ticks(&events).len(),
        51,
        "ticks 1000..=1050 accrue the fixed per-entry delta"
    );
    let TraceEvent::HoldTick {
        hold_remaining_ms: second_remaining,
        ..
    } = hold_ticks(&events)[1]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*second_remaining, 50.0, "two consecutive full snapshots");
    let ends = hold_ends(&events);
    assert_eq!(ends.len(), 1);
    assert!(matches!(ends[0], TraceEvent::HoldEnd { sim: 1052, .. }));
}

#[test]
fn window_miss_keeps_missed_without_bumping_total_notes_hit() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0},{"t":100,"d":1}"#);
    let (_, events) = run(&fixture, 300, &[press(0, 0)]);
    let miss = events
        .iter()
        .find(|event| matches!(event, TraceEvent::Miss { note_id: 1, .. }))
        .expect("window miss for the unpressed note");
    let TraceEvent::Miss {
        sim,
        tallies,
        combo_before,
        combo_after,
        ..
    } = miss
    else {
        unreachable!("filtered miss variants");
    };
    assert_eq!(*sim, 261, "position > note.time + 160");
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0, "a window miss breaks combo");
    assert_eq!(
        *tallies,
        TallyCounts {
            sick: 1,
            missed: 1,
            total_notes_hit: 1,
            ..TallyCounts::default()
        },
        "only the earlier hit incremented total_notes_hit"
    );
}

#[test]
fn fractional_sustain_accrues_fixed_ticks_through_ceil_end_without_a_partial() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":10.5}"#);
    // hold_end is the first integer T >= 10.5+2, which is 13.
    let (sim, events) = run(&fixture, 13, &[press(0, 0)]);
    let ticks = hold_ticks(&events);
    assert_eq!(
        ticks.len(),
        12,
        "ticks 0..=ceil(10.5): the fixed per-entry branch has no 0.5 ms partial"
    );
    let TraceEvent::HoldTick {
        hold_remaining_ms: first_remaining,
        ..
    } = ticks[0]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    let TraceEvent::HoldTick {
        hold_remaining_ms: second_remaining,
        ..
    } = ticks[1]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*first_remaining, 10.5);
    assert_eq!(
        *second_remaining, 10.5,
        "two consecutive full snapshots before the decay"
    );
    let TraceEvent::HoldTick {
        sim: last_sim,
        health_delta,
        score_delta,
        hold_remaining_ms,
        hold_elapsed_ms,
        ..
    } = ticks[11]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*last_sim, 11);
    assert_eq!(*health_delta, 0.00012, "always the fixed 1 ms delta");
    assert_eq!(*score_delta, 0.25, "always the fixed 1 ms delta");
    assert_eq!(
        *hold_remaining_ms, 0.5,
        "the last accrual observes the stale 0.5 snapshot, never -0.5"
    );
    assert_eq!(*hold_elapsed_ms, 11.0);
    let ends = hold_ends(&events);
    assert_eq!(ends.len(), 1);
    assert!(matches!(ends[0], TraceEvent::HoldEnd { sim: 13, .. }));
    assert!(
        !events.iter().any(|event| {
            matches!(
                event,
                TraceEvent::HoldTick { sim: 12, .. }
                    | TraceEvent::HoldEnd { sim: 12, .. }
                    | TraceEvent::HoldDrop { sim: 12, .. }
            )
        }),
        "the recompute that stores the non-positive snapshot emits no row"
    );
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
        "the silent tick between last accrual and hold_end emits no row"
    );
    assert!((sim.health - (HEALTH_STARTING + HEALTH_SICK_BONUS + 12.0 * 0.00012)).abs() < 1e-12);
    assert!((sim.song_score - (500.0 + 12.0 * 0.25)).abs() < 1e-12);

    // A whole-millisecond sustain of the same floor length accrues one fewer
    // tick: ceil(10) = 10, so ticks 0..=10. The last snapshot is 1 ms, not 0.
    let whole = Fixture::with_notes(r#"{"t":0,"d":0,"l":10}"#);
    let (_, whole_events) = run(&whole, 12, &[press(0, 0)]);
    let whole_ticks = hold_ticks(&whole_events);
    assert_eq!(whole_ticks.len(), 11);
    let TraceEvent::HoldTick {
        hold_remaining_ms: last_remaining,
        health_delta: last_health,
        score_delta: last_score,
        ..
    } = whole_ticks[10]
    else {
        unreachable!("hold_ticks() returns tick variants");
    };
    assert_eq!(*last_remaining, 1.0);
    assert_eq!(*last_health, 0.00012);
    assert_eq!(*last_score, 0.25);
    assert!(matches!(
        hold_ends(&whole_events)[0],
        TraceEvent::HoldEnd { sim: 12, .. }
    ));
}

#[test]
fn hold_drop_penalty_is_strictly_gated_at_160_ms_and_deferred_one_tick() {
    // Release at 300. The next mirror (sim 301) flags the drop and keeps the
    // snapshot set at sim 300: end - 299 = 201. Penalty is -125 * 201 / 1000.
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
    assert_eq!(
        *drop_sim, 301,
        "the drop row is the flag tick, one after release"
    );
    assert_eq!(*input_ms, 0.0, "a hold drop is a non-input event");
    assert_eq!(*score_penalty, -25.125);
    assert_eq!(*health_change, 0.0);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0);
    assert!(*is_combo_break);
    assert_eq!(hold_ticks(&events).len(), 301, "ticks 0..=release tick");
    assert!((sim.health - (1.0 + 0.03 + 301.0 * 0.00012)).abs() < 1e-12);
    assert!((sim.song_score - (500.0 + 301.0 * 0.25 - 25.125)).abs() < 1e-12);

    // ADR-0018 release ceil(t+l-160)=300 on l=460: stale snapshot
    // end-(R-1)=161, penalizing -20.125, combo break.
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":460}"#);
    let (_, events) = run(&fixture, 301, &[press(0, 0), release(300, 0)]);
    let drops = hold_drops(&events);
    assert_eq!(drops.len(), 1);
    let TraceEvent::HoldDrop {
        sim: drop_sim,
        score_penalty,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = drops[0]
    else {
        unreachable!("hold_drops() returns drop variants");
    };
    assert_eq!(*drop_sim, 301);
    assert_eq!(*score_penalty, -20.125);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 0);
    assert!(*is_combo_break);

    // New boundary ceil(t+l-159)=301: stale snapshot exactly 160.0, zero
    // penalty, combo unchanged.
    let (_, events) = run(&fixture, 302, &[press(0, 0), release(301, 0)]);
    let drops = hold_drops(&events);
    assert_eq!(drops.len(), 1);
    let TraceEvent::HoldDrop {
        sim: drop_sim,
        input_ms,
        score_penalty,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = drops[0]
    else {
        unreachable!("hold_drops() returns drop variants");
    };
    assert_eq!(*drop_sim, 302);
    assert_eq!(*input_ms, 0.0);
    assert_eq!(*score_penalty, 0.0);
    assert_eq!(*combo_before, 1);
    assert_eq!(*combo_after, 1);
    assert!(!*is_combo_break);

    // Stale snapshot end-(R-1)=101 <= 160: zero penalty, no combo break.
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":400}"#);
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
    assert_eq!(*combo_after, 1);
    assert!(!*is_combo_break);

    // Release at the sustain end is not completion. The next mirror still sees
    // snapshot 1.0 and drops with zero penalty (§6.1 items 8-9).
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":300}"#);
    let (_, events) = run(&fixture, 302, &[press(0, 0), release(300, 0)]);
    assert!(hold_ends(&events).is_empty());
    let drops = hold_drops(&events);
    assert_eq!(drops.len(), 1);
    let TraceEvent::HoldDrop {
        sim: drop_sim,
        score_penalty,
        is_combo_break,
        ..
    } = drops[0]
    else {
        unreachable!("hold_drops() returns drop variants");
    };
    assert_eq!(*drop_sim, 301);
    assert_eq!(*score_penalty, 0.0);
    assert!(!*is_combo_break);
    assert_eq!(hold_ticks(&events).len(), 301);

    // Release on the silent tick after the last accrual is seen by the
    // completion mirror, which precedes the penalty (§6.1 item 9).
    let (_, events) = run(&fixture, 302, &[press(0, 0), release(301, 0)]);
    assert!(hold_drops(&events).is_empty());
    assert_eq!(hold_ends(&events).len(), 1);
    assert!(matches!(
        hold_ends(&events)[0],
        TraceEvent::HoldEnd { sim: 302, .. }
    ));
    assert_eq!(hold_ticks(&events).len(), 301);
}

#[test]
fn same_tick_press_before_release_deferrs_the_drop_one_tick() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":500}"#);
    let (_, events) = run(&fixture, 2, &[press(0, 0), release(0, 0)]);
    assert_eq!(events.len(), 5);
    assert!(matches!(&events[0], TraceEvent::Hit { .. }));
    assert!(matches!(&events[1], TraceEvent::HoldStart { sim: 0, .. }));
    assert!(matches!(&events[2], TraceEvent::HoldTick { sim: 0, .. }));
    assert!(matches!(&events[3], TraceEvent::HoldTick { sim: 1, .. }));
    let TraceEvent::HoldDrop {
        sim,
        input_ms,
        score_penalty,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = &events[4]
    else {
        panic!("expected the deferred hold drop as the fifth event: {events:?}");
    };
    // The hit-tick mirror has not run yet, and the T0+1 mirror sees P == strumTime,
    // so the drop flag and row both land on tick 2 with the hit-capture snapshot.
    assert_eq!(*sim, 2);
    assert_eq!(*input_ms, 0.0);
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
    let (_, events) = run(&fixture, 301, &[press(0, 0), release(300, 0)]);

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
        .find(|event| matches!(event, TraceEvent::HoldTick { sim: 0, .. }))
        .expect("hit-tick hold tick");
    let tick_json = serde_json::to_string(tick).expect("serialize hold tick");
    assert_eq!(
        tick_json,
        r#"{"schema":"1","kind":"hold_tick","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0,"health_delta":0.00012,"score_delta":0.25,"hold_remaining_ms":500.0,"hold_elapsed_ms":0.0}"#
    );

    let drop = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldDrop { .. }))
        .expect("hold drop");
    let drop_json = serde_json::to_string(drop).expect("serialize hold drop");
    assert_eq!(
        drop_json,
        r#"{"schema":"1","kind":"hold_drop","sim":301,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0,"score_penalty":-25.125,"health_change":0.0,"combo_before":1,"combo_after":0,"is_combo_break":true}"#
    );

    let fixture = Fixture::with_notes(r#"{"t":0,"d":0,"l":10}"#);
    let (_, events) = run(&fixture, 12, &[press(0, 0)]);
    let end = events
        .iter()
        .find(|event| matches!(event, TraceEvent::HoldEnd { .. }))
        .expect("hold end");
    let end_json = serde_json::to_string(end).expect("serialize hold end");
    assert_eq!(
        end_json,
        r#"{"schema":"1","kind":"hold_end","sim":12,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0}"#
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

fn trace_sim(event: &TraceEvent) -> u64 {
    match event {
        TraceEvent::Hit { sim, .. }
        | TraceEvent::Miss { sim, .. }
        | TraceEvent::GhostPress { sim, .. }
        | TraceEvent::HoldStart { sim, .. }
        | TraceEvent::HoldTick { sim, .. }
        | TraceEvent::HoldDrop { sim, .. }
        | TraceEvent::HoldEnd { sim, .. }
        | TraceEvent::GameOver { sim, .. }
        | TraceEvent::OpponentHit { sim, .. } => *sim,
    }
}

fn trace_kind(event: &TraceEvent) -> &'static str {
    match event {
        TraceEvent::Hit { kind, .. }
        | TraceEvent::Miss { kind, .. }
        | TraceEvent::GhostPress { kind, .. }
        | TraceEvent::HoldStart { kind, .. }
        | TraceEvent::HoldTick { kind, .. }
        | TraceEvent::HoldDrop { kind, .. }
        | TraceEvent::HoldEnd { kind, .. }
        | TraceEvent::GameOver { kind, .. }
        | TraceEvent::OpponentHit { kind, .. } => kind,
    }
}

fn tick_kinds(events: &[TraceEvent], sim: u64) -> Vec<&'static str> {
    events
        .iter()
        .filter(|event| trace_sim(event) == sim)
        .map(trace_kind)
        .collect()
}

const ERECT_METADATA: &str = r#"{
  "version": "2.2.4",
  "songName": "Bopeebo Erect",
  "playData": { "difficulties": ["erect"] },
  "timeChanges": [{ "t": 0, "bpm": 100 }]
}"#;

const ERECT_CHART: &str = r#"{
  "version": "2.0.0",
  "scrollSpeed": { "erect": 1.2 },
  "events": [],
  "notes": { "erect": [{ "t": 0, "d": 0 }, { "t": 10, "d": 4 }] }
}"#;

#[test]
fn chart_ids_map_to_the_corpus_file_spelling() {
    assert_eq!(
        chart_paths("bopeebo-default").expect("default id"),
        (
            "preload/data/songs/bopeebo/bopeebo-chart.json".to_owned(),
            "preload/data/songs/bopeebo/bopeebo-metadata.json".to_owned(),
        )
    );
    assert_eq!(
        chart_paths("bopeebo").expect("bare default id"),
        chart_paths("bopeebo-default").expect("default id")
    );
    assert_eq!(
        chart_paths("bopeebo-erect").expect("erect id"),
        (
            "preload/data/songs/bopeebo/bopeebo-chart-erect.json".to_owned(),
            "preload/data/songs/bopeebo/bopeebo-metadata-erect.json".to_owned(),
        )
    );
    assert_eq!(
        chart_paths("darnell-bf").expect("bf swap id"),
        (
            "preload/data/songs/darnell/darnell-chart-bf.json".to_owned(),
            "preload/data/songs/darnell/darnell-metadata-bf.json".to_owned(),
        )
    );
    assert_eq!(
        chart_paths("philly-nice-erect").expect("hyphenated song id"),
        (
            "preload/data/songs/philly-nice/philly-nice-chart-erect.json".to_owned(),
            "preload/data/songs/philly-nice/philly-nice-metadata-erect.json".to_owned(),
        )
    );
    assert_eq!(
        chart_paths("lit-up-bf").expect("hyphenated bf id"),
        (
            "preload/data/songs/lit-up/lit-up-chart-bf.json".to_owned(),
            "preload/data/songs/lit-up/lit-up-metadata-bf.json".to_owned(),
        )
    );
    assert_eq!(
        chart_paths("winter-horrorland-default").expect("hyphenated default id"),
        (
            "preload/data/songs/winter-horrorland/winter-horrorland-chart.json".to_owned(),
            "preload/data/songs/winter-horrorland/winter-horrorland-metadata.json".to_owned(),
        )
    );

    for invalid in [
        "",
        "-default",
        "bopeebo-",
        "bopeebo--",
        "bopeebo-default-",
        "bopeebo default",
        "songs/bopeebo-default",
        "bopeebo-default/../x",
        "bopeebo-é",
    ] {
        assert!(
            matches!(chart_paths(invalid), Err(SliceError::InvalidChartId(id)) if id == invalid),
            "chart id {invalid:?} must fail closed"
        );
    }
}

#[test]
fn build_chart_resolves_default_and_variant_ids_and_fails_closed() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":0}"#);
    fixture.write_song(
        "bopeebo",
        "bopeebo-chart-erect",
        ERECT_CHART,
        "bopeebo-metadata-erect",
        ERECT_METADATA,
    );

    let default = fixture
        .build_chart("bopeebo-default", "easy")
        .expect("default corpus id builds");
    assert_eq!(default.schema, SCHEMA_V2);
    let bare = fixture
        .build_chart("bopeebo", "easy")
        .expect("bare default id builds");
    assert_eq!(
        serde_json::to_vec(&default.record()).expect("serialize default trace"),
        serde_json::to_vec(&bare.record()).expect("serialize bare trace"),
        "the bare song id is the default variation"
    );

    let erect = fixture
        .build_chart("bopeebo-erect", "erect")
        .expect("variant id builds");
    assert_eq!(erect.schema, SCHEMA_V2);
    assert_eq!(erect.notes.len(), 1, "player notes stay player-compacted");
    assert_eq!(
        erect.opponent_notes.len(),
        1,
        "opponent notes are parsed on the schema-2 path"
    );

    assert!(matches!(
        fixture.build_chart("bopeebo-erect", "easy"),
        Err(SliceError::UnknownDifficulty(difficulty)) if difficulty == "easy"
    ));
    assert!(matches!(
        fixture.build_chart("bopeebo-pico", "easy"),
        Err(SliceError::MissingAsset { path })
            if path.ends_with("bopeebo-chart-pico.json")
    ));
    assert!(matches!(
        fixture.build_chart("nope-default", "easy"),
        Err(SliceError::MissingAsset { path })
            if path.ends_with("nope-chart.json")
    ));

    // Missing metadata: the chart parses, then the metadata lookup fails with
    // the exact expected path.
    let metadata_missing = Fixture::new();
    metadata_missing.write_chart(
        "bopeebo",
        "bopeebo-chart",
        r#"{"version":"2.0.0","notes":{"easy":[]},"events":[]}"#,
    );
    assert!(matches!(
        metadata_missing.build_chart("bopeebo", "easy"),
        Err(SliceError::MissingAsset { path })
            if path.ends_with("bopeebo-metadata.json")
    ));

    // Missing chart: fail before reading metadata.
    let chart_missing = Fixture::new();
    chart_missing.write_metadata("bopeebo", "bopeebo-metadata", METADATA);
    assert!(matches!(
        chart_missing.build_chart("bopeebo", "easy"),
        Err(SliceError::MissingAsset { path })
            if path.ends_with("bopeebo-chart.json")
    ));

    // The schema-2 path requires the metadata difficulty list; the frozen M5
    // path keeps its permissive behavior.
    let no_difficulties = Fixture::new();
    no_difficulties.write_chart(
        "bopeebo",
        "bopeebo-chart",
        r#"{"version":"2.0.0","notes":{"easy":[]},"events":[]}"#,
    );
    no_difficulties.write_metadata(
        "bopeebo",
        "bopeebo-metadata",
        r#"{"version":"2.2.4","songName":"No Play Data"}"#,
    );
    assert!(matches!(
        no_difficulties.build_chart("bopeebo", "easy"),
        Err(SliceError::MissingDifficultyList { path })
            if path.ends_with("bopeebo-metadata.json")
    ));
    assert!(no_difficulties.build_with("easy").is_ok());
}

#[test]
fn build_chart_resolves_bare_hyphenated_song_ids() {
    const NICE_METADATA: &str = r#"{
      "version": "2.2.4",
      "songName": "Philly Nice",
      "playData": { "difficulties": ["easy"] },
      "timeChanges": [{ "t": 0, "bpm": 50 }]
    }"#;
    const NICE_CHART: &str = r#"{"version":"2.0.0","scrollSpeed":{"easy":1.5},"events":[],"notes":{"easy":[
          {"t":500,"d":0},{"t":700,"d":4}
        ]}}"#;
    let fixture = Fixture::new();
    fixture.write_song(
        "philly-nice",
        "philly-nice-chart",
        NICE_CHART,
        "philly-nice-metadata",
        NICE_METADATA,
    );
    let bare = fixture
        .build_chart("philly-nice", "easy")
        .expect("bare hyphenated id builds");
    assert_eq!(bare.schema, SCHEMA_V2);
    assert_eq!(bare.notes.len(), 1, "player notes parse for the bare id");
    assert_eq!(
        bare.opponent_notes.len(),
        1,
        "opponent notes parse for the bare id"
    );
}

#[test]
fn build_chart_resolves_the_bf_swap_song_ids() {
    const BF_METADATA: &str = r#"{
      "version": "2.2.4",
      "songName": "BF Mix",
      "playData": { "difficulties": ["easy"] },
      "timeChanges": [{ "t": 0, "bpm": 100 }]
    }"#;
    const BF_CHART: &str = r#"{
      "version": "2.0.0",
      "scrollSpeed": { "easy": 1.2 },
      "events": [],
      "notes": { "easy": [{ "t": 0, "d": 0 }, { "t": 10, "d": 5 }] }
    }"#;

    let fixture = Fixture::new();
    fixture.write_song(
        "darnell",
        "darnell-chart-bf",
        BF_CHART,
        "darnell-metadata-bf",
        BF_METADATA,
    );
    fixture.write_song(
        "lit-up",
        "lit-up-chart-bf",
        BF_CHART,
        "lit-up-metadata-bf",
        BF_METADATA,
    );

    for chart_id in ["darnell-bf", "lit-up-bf"] {
        let sim = fixture
            .build_chart(chart_id, "easy")
            .unwrap_or_else(|error| panic!("{chart_id} must build: {error}"));
        assert_eq!(sim.notes.len(), 1, "{chart_id} player id space");
        assert_eq!(sim.opponent_notes.len(), 1, "{chart_id} opponent id space");
    }
}

#[test]
fn m5_build_stays_schema_one_and_player_only_with_opponent_notes_present() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":4},{"t":0,"d":0}"#);
    let (sim, events) = run(&fixture, 0, &[press(0, 0)]);

    assert_eq!(sim.schema, SCHEMA_V1);
    assert!(sim.opponent_notes.is_empty());
    assert_eq!(hits(&events).len(), 1);
    assert!(opponent_hits(&events).is_empty());
    assert!(events.iter().all(|event| match event {
        TraceEvent::Hit { schema, .. }
        | TraceEvent::Miss { schema, .. }
        | TraceEvent::GhostPress { schema, .. }
        | TraceEvent::HoldStart { schema, .. }
        | TraceEvent::HoldTick { schema, .. }
        | TraceEvent::HoldDrop { schema, .. }
        | TraceEvent::HoldEnd { schema, .. }
        | TraceEvent::GameOver { schema, .. } => *schema == SCHEMA_V1,
        TraceEvent::OpponentHit { .. } => false,
    }));

    // The same fixture on the chart-id path takes the schema-2 extension and
    // records the opponent lane.
    let (schema_two, schema_two_events) = run_chart(&fixture, "bopeebo", "easy", 0, &[press(0, 0)]);
    assert_eq!(schema_two.schema, SCHEMA_V2);
    assert_eq!(opponent_hits(&schema_two_events).len(), 1);
    assert!(matches!(
        schema_two_events
            .iter()
            .find(|event| matches!(event, TraceEvent::Hit { .. }))
            .expect("player hit"),
        TraceEvent::Hit {
            schema: SCHEMA_V2,
            ..
        }
    ));
}

#[test]
fn opponent_notes_auto_hit_at_the_first_tick_at_or_after_their_time() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":6},{"t":100,"d":7},{"t":100.5,"d":4}"#);
    let (sim, events) = run_chart(&fixture, "bopeebo", "easy", 300, &[]);

    let found = opponent_hits(&events);
    assert_eq!(found.len(), 3, "one row per opponent note");
    let mut observed = Vec::new();
    for event in found {
        let TraceEvent::OpponentHit {
            schema,
            kind,
            sim,
            input_ms,
            input_time_us,
            note_id,
            note_direction,
            judgement,
            raw_note_diff,
            note_diff_int,
        } = event
        else {
            unreachable!("opponent_hits() returns opponent variants");
        };
        observed.push((*sim, *note_id, *note_direction));
        assert_eq!(*schema, SCHEMA_V2);
        assert_eq!(*kind, "opponent_hit");
        assert_eq!(*input_ms, 0.0);
        assert_eq!(*input_time_us, 0.0);
        assert_eq!(*judgement, "perfect");
        assert_eq!(*raw_note_diff, 0.0);
        assert_eq!(*note_diff_int, 0);
    }
    assert_eq!(observed, [(0, 0, 2), (100, 1, 3), (101, 2, 0)]);

    assert!(
        events
            .iter()
            .all(|event| matches!(event, TraceEvent::OpponentHit { .. })),
        "opponent-only charts produce no player rows and no opponent misses"
    );
    assert_eq!(sim.record(), events);
    assert_eq!(sim.health, HEALTH_STARTING);
    assert_eq!(sim.song_score, 0.0);
    assert_eq!(sim.combo, 0);
    assert_eq!(sim.combo_max, 0);
    assert_eq!(sim.tallies, TallyCounts::default());
    assert_eq!(sim.death_count, 0);
}

#[test]
fn opponent_lane_presses_are_ignored_on_the_schema_two_path() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":4}"#);
    let (_, events) = run_chart(
        &fixture,
        "bopeebo",
        "easy",
        1,
        &[press(0, 4), release(1, 4)],
    );
    assert_eq!(opponent_hits(&events).len(), 1);
    assert!(
        events
            .iter()
            .all(|event| matches!(event, TraceEvent::OpponentHit { .. })),
        "an opponent-lane press is not a player ghost/hit: {events:?}"
    );
}

#[test]
fn opponent_note_ids_are_compacted_independently_from_player_ids() {
    let fixture = Fixture::with_notes(
        r#"{"t":0,"d":0},{"t":0,"d":4},{"t":0,"d":1},{"t":0,"d":5},{"t":0,"d":2},{"t":0,"d":6},{"t":0,"d":7}"#,
    );
    let mut sim = fixture
        .build_chart("bopeebo", "easy")
        .expect("fixture builds");
    let events = sim.step(0, &[press(0, 0), press(0, 1), press(0, 2)]);

    let player_ids: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            TraceEvent::Hit { note_id, .. } => Some(*note_id),
            _ => None,
        })
        .collect();
    let player_directions: Vec<i64> = events
        .iter()
        .filter_map(|event| match event {
            TraceEvent::Hit { note_direction, .. } => Some(*note_direction),
            _ => None,
        })
        .collect();
    let opponent_rows: Vec<(u32, i64)> = events
        .iter()
        .filter_map(|event| match event {
            TraceEvent::OpponentHit {
                note_id,
                note_direction,
                ..
            } => Some((*note_id, *note_direction)),
            _ => None,
        })
        .collect();

    assert_eq!(player_ids, [0, 1, 2]);
    assert_eq!(player_directions, [0, 1, 2]);
    assert_eq!(opponent_rows, [(0, 0), (1, 1), (2, 2), (3, 3)]);
    assert_eq!(sim.notes.len(), 3);
    assert_eq!(sim.opponent_notes.len(), 4);
}

#[test]
fn same_tick_record_order_follows_the_native_frame_phases() {
    // Native order (NOTE-BEHAVIOR §6.1 item 5 / M6 §10.1): processInputQueue
    // rows first, then the opponent loop at the start of processNotes, then
    // the player-note rows and the hold rows.
    let fixture =
        Fixture::with_notes(r#"{"t":0,"d":0,"l":500},{"t":0,"d":1},{"t":0,"d":4},{"t":161,"d":5}"#);
    let (_, events) = run_chart(&fixture, "bopeebo", "easy", 161, &[press(0, 0)]);
    assert_eq!(
        tick_kinds(&events, 0),
        ["hit", "hold_start", "opponent_hit", "hold_tick"],
        "input rows, then the opponent loop, then the hold accrual"
    );
    assert_eq!(
        tick_kinds(&events, 161),
        ["opponent_hit", "miss", "hold_tick"],
        "the opponent loop precedes the player window miss in processNotes"
    );
}

#[test]
fn schema_two_opponent_hit_serializes_in_contract_order_and_round_trips() {
    let fixture = Fixture::with_notes(r#"{"t":0,"d":4},{"t":0,"d":0}"#);
    let (_, events) = run_chart(&fixture, "bopeebo", "easy", 0, &[press(0, 0)]);
    let row = events
        .iter()
        .find(|event| matches!(event, TraceEvent::OpponentHit { .. }))
        .expect("opponent row");

    let json = serde_json::to_string(row).expect("serialize opponent row");
    assert_eq!(
        json,
        r#"{"schema":"2","kind":"opponent_hit","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"perfect","raw_note_diff":0.0,"note_diff_int":0}"#
    );
    let value: serde_json::Value = serde_json::from_str(&json).expect("row is JSON");
    let object = value.as_object().expect("row is a JSON object");
    assert_eq!(object.len(), 10, "the shared header only");
    for absent in [
        "score_delta",
        "health_before",
        "health_after",
        "combo",
        "tallies",
    ] {
        assert!(!object.contains_key(absent), "{absent} must not be present");
    }

    #[derive(Debug, serde::Deserialize, serde::Serialize)]
    struct OpponentRow {
        schema: String,
        kind: String,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: String,
        raw_note_diff: f64,
        note_diff_int: i32,
    }
    let decoded: OpponentRow = serde_json::from_str(&json).expect("schema-2 row decodes");
    assert_eq!(decoded.kind, "opponent_hit");
    assert_eq!(decoded.judgement, "perfect");
    assert_eq!(
        serde_json::to_string(&decoded).expect("re-serialize schema-2 row"),
        json,
        "the schema-2 wire order round-trips"
    );
}

#[test]
fn two_run_sha256_is_stable_for_a_scripted_schema_two_stream() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "the test-local SHA-256 implementation matches the FIPS vector"
    );

    let fixture = Fixture::with_notes(
        r#"{"t":0,"d":0},{"t":100,"d":1},{"t":200,"d":2},{"t":300,"d":3},
           {"t":0,"d":4},{"t":150,"d":5},{"t":300,"d":6},{"t":450,"d":7}"#,
    );
    let inputs = [
        press(0, 0),
        press(50, 3),
        press(100, 1),
        press(200, 2),
        press(300, 3),
    ];

    let (first_sim, first_events) = run_chart(&fixture, "bopeebo", "easy", 600, &inputs);
    let (second_sim, second_events) = run_chart(&fixture, "bopeebo", "easy", 600, &inputs);
    assert_eq!(first_sim.record(), first_events);
    assert_eq!(second_sim.record(), second_events);
    assert_eq!(
        first_sim.record(),
        second_sim.record(),
        "two fresh sims diverge"
    );

    let first_bytes = serialize_trace(&first_events);
    let second_bytes = serialize_trace(&second_events);
    assert_eq!(first_bytes, second_bytes);
    assert_eq!(sha256_hex(&first_bytes), sha256_hex(&second_bytes));
    assert_eq!(opponent_hits(&first_events).len(), 4);
    assert!(
        first_events
            .iter()
            .filter(|event| matches!(event, TraceEvent::OpponentHit { .. }))
            .all(|event| matches!(
                event,
                TraceEvent::OpponentHit {
                    schema: SCHEMA_V2,
                    ..
                }
            ))
    );
}

#[test]
fn build_chart_rejects_invalid_opponent_note_data() {
    let negative_time = Fixture::with_notes(r#"{"t":0,"d":4},{"t":-1,"d":5}"#);
    assert!(matches!(
        negative_time.build_chart("bopeebo", "easy"),
        Err(SliceError::InvalidNote {
            index: 1,
            detail,
            ..
        }) if detail.contains("time")
    ));
    // The frozen M5 path keeps skipping opponent notes before validation.
    assert!(negative_time.build_with("easy").is_ok());

    let negative_sustain = Fixture::with_notes(r#"{"t":0,"d":4,"l":-1}"#);
    assert!(matches!(
        negative_sustain.build_chart("bopeebo", "easy"),
        Err(SliceError::InvalidNote {
            index: 0,
            detail,
            ..
        }) if detail.contains("sustain")
    ));

    let out_of_range = Fixture::with_notes(r#"{"t":0,"d":8}"#);
    assert!(matches!(
        out_of_range.build_chart("bopeebo", "easy"),
        Err(SliceError::InvalidNote {
            index: 0,
            detail,
            ..
        }) if detail.contains("direction")
    ));
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

/// Minimal FIPS 180-4 SHA-256 used only by the two-run trace digest test.
///
/// The crate has no hashing dependency, and the acceptance harness (xtask)
/// owns the real digest gate; this local implementation keeps the regression
/// test self-contained and is pinned by the `abc` FIPS vector above.
fn sha256_hex(input: &[u8]) -> String {
    const ROUND_CONSTANTS: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];

    let mut state: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let bit_len = (input.len() as u64).wrapping_mul(8);
    let mut message = input.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.as_chunks::<64>().0 {
        let mut schedule = [0u32; 64];
        for (index, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            schedule[index] = u32::from_be_bytes(*word);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let mut work = state;
        for index in 0..64 {
            let s1 = work[4].rotate_right(6) ^ work[4].rotate_right(11) ^ work[4].rotate_right(25);
            let choose = (work[4] & work[5]) ^ (!work[4] & work[6]);
            let temp1 = work[7]
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(ROUND_CONSTANTS[index])
                .wrapping_add(schedule[index]);
            let s0 = work[0].rotate_right(2) ^ work[0].rotate_right(13) ^ work[0].rotate_right(22);
            let majority = (work[0] & work[1]) ^ (work[0] & work[2]) ^ (work[1] & work[2]);
            let temp2 = s0.wrapping_add(majority);
            work[7] = work[6];
            work[6] = work[5];
            work[5] = work[4];
            work[4] = work[3].wrapping_add(temp1);
            work[3] = work[2];
            work[2] = work[1];
            work[1] = work[0];
            work[0] = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip(work) {
            *slot = slot.wrapping_add(value);
        }
    }

    state.iter().map(|word| format!("{word:08x}")).collect()
}
