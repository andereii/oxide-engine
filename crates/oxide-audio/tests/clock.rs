//! Clock tests. These run against Kira's simulation backend, which is the
//! default build; the `real-audio` feature removes the manual driver.

#![cfg(not(feature = "real-audio"))]

use oxide_audio::clock::{AudioClock, CLOCK_HZ};

fn position(clock: &AudioClock) -> f64 {
    clock.song_position_ms().expect("song position")
}

fn assert_close(actual: f64, expected: f64, epsilon: f64) {
    assert!(
        (actual - expected).abs() <= epsilon,
        "expected {expected}, got {actual} (delta {})",
        (actual - expected).abs()
    );
}

/// `song_position_ms` is read from Kira ticks, starts at 0 and grows
/// monotonically under rate 1.0.
#[test]
fn song_position_grows_monotonically_at_rate_one() {
    let clock = AudioClock::new().expect("create audio clock");
    assert_eq!(clock.clock_hz(), CLOCK_HZ);
    assert_eq!(CLOCK_HZ, 1000.0);
    assert_eq!(position(&clock), 0.0);

    clock.advance_simulation_ms(100.0);
    let first = position(&clock);
    assert_close(first, 100.0, 1e-6);

    clock.advance_simulation_ms(50.0);
    let second = position(&clock);
    assert_close(second, 150.0, 1e-6);
    assert!(second > first);

    // A sub-millisecond request does not advance the simulation grid, but the
    // position must never go backwards.
    clock.advance_simulation_ms(0.49);
    assert_eq!(position(&clock), second);

    clock.advance_simulation_ms(1.0);
    assert!(position(&clock) > second);
}

/// `set_start_offset_ms` destroys and recreates the clock, so it is a seek:
/// the position jumps to the offset and then keeps ticking.
#[test]
fn set_start_offset_seeks() {
    let clock = AudioClock::new().expect("create audio clock");
    clock.advance_simulation_ms(30.0);
    assert_close(position(&clock), 30.0, 1e-6);

    clock.set_start_offset_ms(5000.0).expect("seek to 5000 ms");
    // The fresh clock has not received a buffer yet, so the position is exact.
    assert_eq!(position(&clock), 5000.0);

    clock.advance_simulation_ms(25.0);
    assert_close(position(&clock), 5025.0, 1e-6);

    // Seeking backwards also works.
    clock.set_start_offset_ms(0.0).expect("seek to 0 ms");
    assert_eq!(position(&clock), 0.0);
    clock.advance_simulation_ms(10.0);
    assert_close(position(&clock), 10.0, 1e-6);
}

/// `set_rate(0.0)` pauses the clock, a positive rate scales it, and the rate
/// survives a seek (destroy + recreate).
#[test]
fn rate_pause_resume_and_seek() {
    let mut clock = AudioClock::new().expect("create audio clock");
    clock.advance_simulation_ms(20.0);
    assert_close(position(&clock), 20.0, 1e-6);

    clock.set_rate(0.0).expect("pause");
    clock.advance_simulation_ms(100.0);
    assert_close(position(&clock), 20.0, 1e-9);

    // Seek while paused: the recreated clock inherits rate 0.
    clock
        .set_start_offset_ms(1000.0)
        .expect("seek while paused");
    clock.advance_simulation_ms(50.0);
    assert_close(position(&clock), 1000.0, 1e-9);

    // 2x rate: 100 ms of wall time move 200 ms of song position.
    clock.set_rate(2.0).expect("set rate 2.0");
    clock.advance_simulation_ms(100.0);
    assert_close(position(&clock), 1200.0, 1e-6);

    clock.set_rate(1.0).expect("set rate 1.0");
    clock.advance_simulation_ms(50.0);
    assert_close(position(&clock), 1250.0, 1e-6);

    assert!(clock.set_rate(-1.0).is_err());
    assert!(clock.set_rate(f64::NAN).is_err());
    assert!(clock.set_start_offset_ms(f64::INFINITY).is_err());
}

/// Handles are clones over one shared Kira clock: both observe identical ticks,
/// which is how Inst and Voices stay synchronised.
#[test]
fn handles_share_one_clock() {
    let clock = AudioClock::new().expect("create audio clock");
    let first = clock.handle();
    let second = clock.handle();

    assert_eq!(first.id(), second.id());
    assert_eq!(first.time(), second.time());
    assert_eq!(first.time().ticks, 0);

    clock.advance_simulation_ms(10.0);

    assert_eq!(first.id(), second.id());
    assert_eq!(first.time(), second.time());
    let time = first.time();
    assert_eq!(time.ticks, 10);
    assert_close(time.ticks as f64 + time.fraction, 10.0, 1e-9);
    assert_close(position(&clock), 10.0, 1e-6);

    // The handles keep working after a seek: they point at the replacement
    // clock inside the same shared slot.
    clock.set_start_offset_ms(250.0).expect("seek");
    assert_eq!(first.time().ticks, 0);
    assert_eq!(first.id(), second.id());
}
