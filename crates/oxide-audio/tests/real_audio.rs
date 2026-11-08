//! Real-device integration check.
//!
//! Only compiled with the `real-audio` feature and ignored by default, because
//! it needs a working cpal device (ALSA/PulseAudio on Linux) that portable CI
//! does not provide. Run explicitly with:
//!
//! ```text
//! cargo test -p oxide-audio --features real-audio --test real_audio -- --ignored
//! ```

#![cfg(feature = "real-audio")]

use oxide_audio::clock::{AudioClock, AudioError};

#[test]
#[ignore = "requires a real cpal audio device; run with --features real-audio -- --ignored"]
fn real_backend_opens_a_device_or_skips_when_unavailable() {
    match AudioClock::new() {
        Ok(clock) => {
            // The real clock must report a song position through the same
            // ticks-to-milliseconds conversion playback uses.
            let position = clock.song_position_ms().expect("song position");
            assert!(position.is_finite(), "position {position} must be finite");
        }
        Err(AudioError::Backend(message)) => {
            eprintln!("skipping: audio device unavailable: {message}");
        }
        Err(other) => panic!("unexpected audio error: {other}"),
    }
}
