//! Conductor semantics tests: class-doc lengths, update branches and the
//! time-change mapping. Expected numbers are hand-computed from
//! `funkin/Conductor.hx` (citations inline).

use oxide_audio::conductor::{
    Conductor, ConductorOffsets, Dispatch, MS_PER_SEC, STEPS_PER_BEAT, TimeChange,
};

fn time_change(time_stamp: f64, bpm: f64, num: i32, den: i32) -> TimeChange {
    TimeChange {
        time_stamp,
        beat_time: 0.0,
        bpm,
        time_signature_num: num,
        time_signature_den: den,
    }
}

fn conductor_120_4_4() -> Conductor {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[time_change(0.0, 120.0, 4, 4)]);
    conductor
}

fn dispatch_parts(dispatches: &[Dispatch]) -> Vec<(&'static str, i32)> {
    dispatches
        .iter()
        .map(|dispatch| match dispatch {
            Dispatch::Step(index) => ("step", *index),
            Dispatch::Beat(index) => ("beat", *index),
            Dispatch::Measure(index) => ("measure", *index),
        })
        .collect()
}

fn assert_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {expected}, got {actual} (delta {})",
        (actual - expected).abs()
    );
}

/// `Conductor.hx:21-46` class doc checklist, all at 120 BPM.
///
/// `beatLengthMs = (60/bpm)*1000 * (4/den)` (L176-182),
/// `stepLengthMs = beatLengthMs/4` (L187-192),
/// `measureLengthMs = beatLengthMs*num` (L166-171).
#[test]
fn class_doc_lengths_at_120_bpm() {
    // (label, num, den, beat, step, measure)
    let cases = [
        ("4/4", 4, 4, 500.0, 125.0, 2000.0),
        // 7/8: beats are eighth notes -> (60/120)*1000*(4/8) = 250 ms.
        ("7/8", 7, 8, 250.0, 62.5, 1750.0),
        // 15/16: beats are sixteenth notes -> 500*(4/16) = 125 ms.
        ("15/16", 15, 16, 125.0, 31.25, 1875.0),
        // 3/2: beats are half notes -> 500*(4/2) = 1000 ms.
        ("3/2", 3, 2, 1000.0, 250.0, 3000.0),
    ];

    for (label, num, den, beat, step, measure) in cases {
        let mut conductor = Conductor::new();
        conductor.map_time_changes(&[time_change(0.0, 120.0, num, den)]);

        assert_eq!(conductor.bpm(), 120.0, "{label}");
        assert_eq!(conductor.starting_bpm(), 120.0, "{label}");
        assert_eq!(conductor.beat_length_ms(), beat, "{label}");
        assert_eq!(conductor.step_length_ms(), step, "{label}");
        assert_eq!(conductor.measure_length_ms(), measure, "{label}");

        // Class doc rates: 1000/beatLength beats per second, 1000/stepLength
        // steps per second.
        assert_eq!(MS_PER_SEC / conductor.beat_length_ms(), MS_PER_SEC / beat);
        assert_eq!(MS_PER_SEC / conductor.step_length_ms(), MS_PER_SEC / step);
    }

    // Explicit doc-stated rates for the two extremes.
    let seven_eight = {
        let mut conductor = Conductor::new();
        conductor.map_time_changes(&[time_change(0.0, 120.0, 7, 8)]);
        conductor
    };
    assert_eq!(MS_PER_SEC / seven_eight.beat_length_ms(), 4.0);
    assert_eq!(MS_PER_SEC / seven_eight.step_length_ms(), 16.0);

    let three_two = {
        let mut conductor = Conductor::new();
        conductor.map_time_changes(&[time_change(0.0, 120.0, 3, 2)]);
        conductor
    };
    assert_eq!(MS_PER_SEC / three_two.beat_length_ms(), 1.0);
    assert_eq!(MS_PER_SEC / three_two.step_length_ms(), 4.0);
}

/// `Conductor.hx:440-448`: the clamp exists only in the playing branch.
#[test]
fn clamp_applies_only_when_music_playing() {
    // Lower bound with zero offset: Math.min(0, 0) = 0.
    let mut not_playing = conductor_120_4_4();
    not_playing.update(Some(-100.0), true, false, 1000.0);
    assert_eq!(not_playing.song_position_ms(), -100.0);

    let mut playing = conductor_120_4_4();
    playing.update(Some(-100.0), true, true, 1000.0);
    assert_eq!(playing.song_position_ms(), 0.0);

    // Upper bound: currentLength.
    let mut playing = conductor_120_4_4();
    playing.update(Some(5000.0), true, true, 3000.0);
    assert_eq!(playing.song_position_ms(), 3000.0);

    // Positive combined offset: the lower bound is min(offset, 0) = 0.
    let mut positive_offset = conductor_120_4_4();
    positive_offset.offsets.instrumental = 50.0;
    positive_offset.update(Some(-100.0), true, false, 1000.0);
    assert_eq!(positive_offset.song_position_ms(), -50.0);
    positive_offset.update(Some(-100.0), true, true, 1000.0);
    assert_eq!(positive_offset.song_position_ms(), 0.0);

    // Negative combined offset: the lower bound is the (negative) offset.
    let mut negative_offset = conductor_120_4_4();
    negative_offset.offsets.instrumental = -20.0;
    negative_offset.update(Some(-100.0), true, false, 1000.0);
    assert_eq!(negative_offset.song_position_ms(), -120.0);
    negative_offset.update(Some(-100.0), true, true, 1000.0);
    assert_eq!(negative_offset.song_position_ms(), -20.0);
}

/// `Conductor.hx:433` and `294-299`: offsets are only applied when asked, and
/// `audioVisualOffset` is not part of `combinedOffset`.
#[test]
fn apply_offsets_flag_and_combined_components() {
    let mut conductor = conductor_120_4_4();
    conductor.offsets = ConductorOffsets {
        instrumental: 10.0,
        format: 20.0,
        audio_visual: 999.0,
        global: 30.0,
    };
    assert_eq!(conductor.combined_offset(), 60.0);
    assert_eq!(conductor.offsets.combined(), 60.0);

    conductor.update(Some(100.0), true, false, 0.0);
    assert_eq!(conductor.song_position_ms(), 160.0);

    conductor.update(Some(100.0), false, false, 0.0);
    assert_eq!(conductor.song_position_ms(), 100.0);
}

/// `Conductor.hx:478-487` constant-BPM branch: step 0 fires only when the
/// position reaches 0, never before.
#[test]
fn step_zero_dispatches_only_on_crossing() {
    let mut conductor = conductor_120_4_4();

    // Pre-roll: -10 ms / 125 ms per step = -0.08 -> floor -1.
    let dispatches = conductor.update(Some(-10.0), false, false, 0.0);
    assert_eq!(
        dispatch_parts(&dispatches),
        vec![("step", -1), ("beat", -1), ("measure", -1)]
    );
    assert_eq!(conductor.current_step(), -1);
    assert_eq!(conductor.current_beat(), -1);
    assert_eq!(conductor.current_measure(), -1);

    // Still inside step -1: -1 ms / 125 = -0.008.
    let dispatches = conductor.update(Some(-1.0), false, false, 0.0);
    assert!(dispatches.is_empty());

    // Reaching 0 selects the constant branch with stepTime = 0.
    let dispatches = conductor.update(Some(0.0), false, false, 0.0);
    assert_eq!(
        dispatch_parts(&dispatches),
        vec![("step", 0), ("beat", 0), ("measure", 0)]
    );

    // Just after 0 we are still in integer step/beat/measure 0.
    let dispatches = conductor.update(Some(1.0), false, false, 0.0);
    assert!(dispatches.is_empty());
    assert_eq!(conductor.current_step(), 0);
}

/// `Conductor.hx:489-503`: step, beat and measure all changing in one update
/// are returned in that order.
#[test]
fn dispatch_order_is_step_beat_measure() {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[time_change(0.0, 240.0, 4, 4)]);

    // 240 BPM 4/4: beat = 250 ms, step = 62.5 ms.
    // 1000 ms -> stepTime = 16, beatTime = 4, measureTime = 1.
    let dispatches = conductor.update(Some(1000.0), false, false, 0.0);
    assert_eq!(
        dispatch_parts(&dispatches),
        vec![("step", 16), ("beat", 4), ("measure", 1)]
    );
    assert_eq!(conductor.current_step_time(), 16.0);
    assert_eq!(conductor.current_beat_time(), 4.0);
    assert_eq!(conductor.current_measure_time(), 1.0);

    // 1 ms later nothing changes.
    let dispatches = conductor.update(Some(1001.0), false, false, 0.0);
    assert!(dispatches.is_empty());
}

/// Empty timeline: `Conductor.hx:478-487` falls back to
/// `Constants.DEFAULT_BPM = 100` (Constants.hx:261) and 4/4.
#[test]
fn empty_time_changes_uses_default_bpm() {
    let mut conductor = Conductor::new();

    assert!(conductor.current_time_change().is_none());
    assert_eq!(conductor.bpm(), 100.0);
    assert_eq!(conductor.starting_bpm(), 100.0);
    // 100 BPM 4/4: beat 600 ms, step 150 ms, measure 2400 ms.
    assert_eq!(conductor.beat_length_ms(), 600.0);
    assert_eq!(conductor.step_length_ms(), 150.0);
    assert_eq!(conductor.measure_length_ms(), 2400.0);

    let dispatches = conductor.update(Some(150.0), false, false, 0.0);
    // round4(150/150) = 1 -> step 1, beat floor(0.25) = 0, measure 0.
    assert_eq!(dispatch_parts(&dispatches), vec![("step", 1)]);
    assert_eq!(conductor.current_step_time(), 1.0);

    assert_eq!(conductor.get_time_in_steps(150.0), 1.0);
    assert_eq!(conductor.get_beat_time_in_ms(1.0), 600.0);
    assert_eq!(conductor.get_measure_time_in_ms(1.0), 2400.0);
    assert!(conductor.get_time_change_at_ms(0.0).is_none());

    // Mapping an empty slice clears a previously populated timeline.
    let mut populated = conductor_120_4_4();
    populated.map_time_changes(&[]);
    assert!(populated.current_time_change().is_none());
    assert_eq!(populated.bpm(), 100.0);
    assert_eq!(populated.step_length_ms(), 150.0);
}

/// A timeline whose first stamp is positive still selects `timeChanges[0]` at
/// position 0 (`Conductor.hx:452`), while `getTimeChange` only matches from the
/// first actual stamp onwards (`Conductor.hx:833-842`).
#[test]
fn positive_first_stamp_selection() {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[time_change(500.0, 120.0, 4, 4)]);

    assert_eq!(
        conductor
            .current_time_change()
            .map(|change| change.time_stamp),
        Some(500.0)
    );
    assert!(conductor.get_time_change_at_ms(0.0).is_none());
    assert_eq!(
        conductor
            .get_time_change_at_ms(500.0)
            .map(|change| change.bpm),
        Some(120.0)
    );

    // Before the first stamp, getTimeInMeasures reports a negative fractional
    // measure anchored at timeChanges[0] (Conductor.hx:640-643):
    // (0 - 500) / 125 ms per step / 16 steps per measure = -0.25.
    assert_close(conductor.get_time_in_measures(0.0), -0.25, 1e-12);
}

/// `Conductor.hx:549-600` + `SongDataUtils.hx:267-275`: stable ascending sort,
/// negative stamps clamped to 0, beat times recomputed from the predecessor.
#[test]
fn map_time_changes_sorts_and_recomputes() {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[
        TimeChange {
            time_stamp: 3000.0,
            beat_time: 999.0,
            bpm: 150.0,
            time_signature_num: 3,
            time_signature_den: 2,
        },
        TimeChange {
            time_stamp: -100.0,
            beat_time: 999.0,
            bpm: 120.0,
            time_signature_num: 4,
            time_signature_den: 4,
        },
        TimeChange {
            time_stamp: 1000.0,
            beat_time: 999.0,
            bpm: 90.0,
            time_signature_num: 7,
            time_signature_den: 8,
        },
    ]);

    // At song position 0 the current change is timeChanges[0].
    let first = conductor.current_time_change().expect("first time change");
    assert_eq!(first.time_stamp, 0.0);
    assert_eq!(first.beat_time, 0.0);
    assert_eq!(first.bpm, 120.0);

    // Entry 1: prev BPM 120, prev den 4, delta 1000 ms:
    // 1000 * 120 / 60 / 1000 * (4/4) = 2.0 beats.
    let second = conductor
        .get_time_change_at_ms(2000.0)
        .expect("second time change");
    assert_eq!(second.time_stamp, 1000.0);
    assert_eq!(second.beat_time, 2.0);
    assert_eq!(second.bpm, 90.0);

    // Entry 2: prev BPM 90, prev den 8, delta 2000 ms:
    // 2.0 + 2000 * 90 / 60 / 1000 * (8/4) = 2.0 + 3*2 = 8.0 beats.
    let third = conductor
        .get_time_change_at_ms(4000.0)
        .expect("third time change");
    assert_eq!(third.time_stamp, 3000.0);
    assert_eq!(third.beat_time, 8.0);
    assert_eq!(third.bpm, 150.0);

    // Boundary selection uses `>=` (Conductor.hx:457).
    conductor.update(Some(999.0), false, false, 0.0);
    assert_eq!(conductor.bpm(), 120.0);
    conductor.update(Some(1000.0), false, false, 0.0);
    assert_eq!(conductor.bpm(), 90.0);
    assert_eq!(conductor.starting_bpm(), 120.0);

    // Updating into the second segment selects its BPM/signature.
    conductor.update(Some(2000.0), false, false, 0.0);
    assert_eq!(conductor.bpm(), 90.0);
    // stepLength = (60/90)*1000*(4/8)/4 = 83.333... ms
    // stepTime = round6(2.0*4 + (2000-1000)/83.333...) = 8 + 12 = 20.
    assert_eq!(conductor.current_step_time(), 20.0);
    assert_eq!(conductor.current_step(), 20);
    assert_eq!(conductor.current_beat(), 5);
}

/// `Conductor.hx:569-579`: `roundDecimal(..., 4)` is applied to the recomputed
/// beat time and a provided `beat_time` is overwritten. HaxeFlixel's
/// `roundDecimal` is `Math.fround(value * 1e4) / 1e4` (`FlxMath.hx:68-76`).
///
/// Change 0: stamp -50 -> 0, beatTime 0.
/// Change 1: stamp 1000, prev BPM 137, prev den 4:
///   raw   = 1000*137/60/1000*(4/4) = 2.2833333333333337
///   x1e4  = 22833.333333333336; native Haxe fround gives 22833.0
///   beat  = 22833.0 / 1e4 = 2.2833
/// Change 2: stamp 1500, prev BPM 137, prev den 4:
///   raw   = 2.2833 + 500*137/60/1000*(4/4) = 3.424966666666667
///   x1e4  = 34249.66666666667; native Haxe fround gives 34250.0
///   beat  = 34250.0 / 1e4 = 3.425 (ADR-0014 native utility evidence).
#[test]
fn map_time_changes_uses_fround_for_beat_time() {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[
        time_change(-50.0, 137.0, 4, 4),
        time_change(1000.0, 137.0, 4, 4),
        time_change(1500.0, 200.0, 4, 8),
    ]);

    let first = conductor
        .get_time_change_at_ms(0.0)
        .expect("first time change");
    assert_eq!(first.time_stamp, 0.0);
    assert_eq!(first.beat_time, 0.0);

    let second = conductor
        .get_time_change_at_ms(1000.0)
        .expect("second time change");
    assert_eq!(second.beat_time, 2.2833);

    let third = conductor
        .get_time_change_at_ms(1500.0)
        .expect("third time change");
    assert_eq!(third.beat_time, 3.425);
}

/// Conversions from `Conductor.hx:607-844` on a two-segment timeline.
///
/// Segment 0: stamp 0, 120 BPM 4/4 (step 125 ms, beat 500 ms, measure 2000 ms).
/// Segment 1: stamp 1000, 240 BPM 4/4 (step 62.5 ms, beat 250 ms, measure
/// 1000 ms), beatTime = 2.0.
#[test]
fn time_conversions_round_trip() {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[
        time_change(0.0, 120.0, 4, 4),
        time_change(1000.0, 240.0, 4, 4),
    ]);

    assert_close(conductor.get_time_in_steps(250.0), 2.0, 1e-12);
    assert_close(conductor.get_time_in_steps(1000.0), 8.0, 1e-12);
    assert_close(conductor.get_time_in_steps(1500.0), 16.0, 1e-12);

    assert_close(conductor.get_step_time_in_ms(16.0), 1500.0, 1e-9);
    assert_close(conductor.get_beat_time_in_ms(2.0), 1000.0, 1e-9);
    assert_close(conductor.get_beat_time_in_ms(4.0), 1500.0, 1e-9);

    assert_close(conductor.get_time_in_measures(1000.0), 0.5, 1e-12);
    assert_close(conductor.get_time_in_measures(2000.0), 1.5, 1e-12);
    assert_close(conductor.get_measure_time_in_ms(0.5), 1000.0, 1e-9);
    assert_close(conductor.get_measure_time_in_ms(1.5), 2000.0, 1e-9);

    // Round trips across the tempo boundary.
    for ms in [0.0, 125.0, 999.0, 1000.0, 1500.0, 2345.0, 5000.0] {
        let steps = conductor.get_time_in_steps(ms);
        assert_close(conductor.get_step_time_in_ms(steps), ms, 1e-6);
        let beats = steps / f64::from(STEPS_PER_BEAT);
        assert_close(conductor.get_beat_time_in_ms(beats), ms, 1e-6);
        let measures = conductor.get_time_in_measures(ms);
        assert_close(conductor.get_measure_time_in_ms(measures), ms, 1e-6);
    }

    // getTimeChange (Conductor.hx:827-844) segment selection.
    assert_eq!(
        conductor
            .get_time_change_at_ms(-5.0)
            .map(|change| change.bpm),
        Some(120.0)
    );
    assert_eq!(
        conductor
            .get_time_change_at_ms(999.0)
            .map(|change| change.bpm),
        Some(120.0)
    );
    assert_eq!(
        conductor
            .get_time_change_at_ms(1000.0)
            .map(|change| change.bpm),
        Some(240.0)
    );
    assert_eq!(
        conductor
            .get_time_change_at_ms(9999.0)
            .map(|change| change.bpm),
        Some(240.0)
    );

    // Empty branch of getTimeChange -> None (Haxe returns a synthetic change).
    let empty = Conductor::new();
    assert!(empty.get_time_change_at_ms(0.0).is_none());
}

/// `map_time_changes` preserves input order for equal timestamps (stable sort).
#[test]
fn map_time_changes_is_stable_for_equal_timestamps() {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&[
        time_change(1000.0, 111.0, 4, 4),
        time_change(0.0, 100.0, 4, 4),
        time_change(1000.0, 222.0, 4, 4),
    ]);

    // Both 1000 ms entries collapse to the same beat time; the later one is
    // the last entry with that stamp, so it is selected at ms >= 1000.
    let selected = conductor
        .get_time_change_at_ms(1000.0)
        .expect("time change at 1000");
    assert_eq!(selected.bpm, 222.0);
    // FlxMath.roundDecimal(raw, 4) with raw = 1000*100/60/1000*(4/4):
    //   raw * 1e4 = 16666.666666666668
    //   native Haxe fround = floor(16666.666666666668 + 0.5) = 16667
    //   beat = 16667 / 1e4 = 1.6667 (ADR-0014).
    // The third entry has a zero delta, so it preserves that decimal result.
    assert_eq!(selected.beat_time, 1.6667);
}
