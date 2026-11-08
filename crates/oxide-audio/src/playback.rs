//! Real song playback: the instrumental plus the metadata's voice tracks on the
//! same Kira clock that [`AudioClock`] measures.
//!
//! The playable slice (ADR-0021) needs one shared timeline for `Inst` and the
//! `playerVocals`/`opponentVocals` tracks. [`SongPlayback::start`] decodes the
//! supplied OGG/Vorbis bytes with Kira/Symphonia (no runtime transcoding), resets
//! the clock to tick 0, schedules every track at that tick and only then starts
//! the clock, so all tracks begin on the same buffer. `position_ms` reads that
//! clock; it is never derived from frame deltas.
//!
//! Building without the `real-audio` feature still compiles, but
//! [`SongPlayback::start`] fails closed with
//! [`AudioError::AudioUnavailable`]: the default (simulation) backend is used by
//! the clock/conductor tests, never silently substituted for missing device
//! output here.
//!
//! Tracks are decoded into memory up front with Kira's [`StaticSoundData`]; the
//! phase-2c playtest plays one song at a time, so a streaming-source lifecycle
//! was not introduced. Decoding happens before the shared clock is reset, so it
//! never shifts the playback start.
//!
//! Volume steps are linear amplitudes in `[0.0, 1.0]` (the `FlxG.sound` volume
//! range used by the play session). They are converted explicitly to Kira's
//! decibel parameter: `0.0` is [`Decibels::SILENCE`](kira::Decibels::SILENCE)
//! and `1.0` is `0 dB` (identity).

use std::io::Cursor;
use std::time::Duration;

use kira::clock::ClockTime;
use kira::sound::static_sound::{StaticSoundData, StaticSoundHandle};
use kira::{Decibels, Easing, StartTime, Tween};

use crate::clock::{AudioClock, ClockHandle, lock};

pub use crate::clock::AudioError;

/// Immediate (zero-duration) tween: mute/restore, pause and stop take effect on
/// the next processed audio buffer instead of fading over Kira's 10 ms default.
const IMMEDIATE: Tween = Tween {
    start_time: StartTime::Immediate,
    duration: Duration::ZERO,
    easing: Easing::Linear,
};

/// Validated audio bytes for one song track.
///
/// The only validation is non-emptiness; the container is kept opaque here and
/// decoded by [`SongPlayback::start`]. The play session reads the bytes through
/// the asset/VFS layer, so this type does no path handling.
#[derive(Clone)]
pub struct AudioBytes {
    bytes: Vec<u8>,
}

impl AudioBytes {
    /// Wraps track bytes, rejecting an empty buffer with
    /// [`AudioError::EmptyAudio`]. The OGG container itself is not parsed here.
    pub fn new(bytes: Vec<u8>) -> Result<Self, AudioError> {
        if bytes.is_empty() {
            return Err(AudioError::EmptyAudio);
        }
        Ok(Self { bytes })
    }
}

impl std::fmt::Debug for AudioBytes {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never dump a whole song into a log line.
        formatter
            .debug_struct("AudioBytes")
            .field("len", &self.bytes.len())
            .finish()
    }
}

/// One voice track from the song metadata (`playerVocals`/`opponentVocals`).
#[derive(Debug)]
pub struct VoiceTrack {
    /// Character id as used by the metadata, e.g. `"bf"` or `"dad"`.
    pub character: String,
    /// Encoded (OGG/Vorbis) voice audio.
    pub data: AudioBytes,
}

/// A running song: one shared Kira clock plus the instrumental and voice tracks.
///
/// Dropping the playback drops the audio manager, which stops all output.
pub struct SongPlayback {
    clock: AudioClock,
    clock_handle: ClockHandle,
    instrumental: std::sync::Mutex<StaticSoundHandle>,
    voices: std::sync::Mutex<VoiceState>,
}

struct Voice {
    character: String,
    handle: StaticSoundHandle,
}

struct VoiceState {
    voices: Vec<Voice>,
    volume_ticks: Vec<(String, f32)>,
}

impl SongPlayback {
    /// Starts `inst` and every voice track at the shared clock's tick 0.
    ///
    /// Every track is scheduled at the same [`ClockTime`] before the clock is
    /// started, so Inst and Voices begin on one audio buffer. Returns
    /// [`AudioError::NoVoices`] when no voice track is supplied,
    /// [`AudioError::Decode`] for undecodable OGG/Vorbis bytes,
    /// [`AudioError::Backend`] when the device cannot be opened and
    /// [`AudioError::AudioUnavailable`] when built without `real-audio`.
    pub fn start(inst: AudioBytes, voices: Vec<VoiceTrack>) -> Result<Self, AudioError> {
        if voices.is_empty() {
            return Err(AudioError::NoVoices);
        }

        // Fail closed on a build without device output before doing any work.
        let clock = open_clock()?;
        let instrumental = decode_audio(inst)?;
        let mut decoded_voices = Vec::with_capacity(voices.len());
        for voice in voices {
            decoded_voices.push((voice.character, decode_audio(voice.data)?));
        }

        Self::from_decoded(clock, instrumental, decoded_voices)
    }

    /// Schedules already-decoded tracks. Kept separate so the simulation-clock
    /// tests can exercise scheduling/volume/pause without OGG fixtures.
    fn from_decoded(
        clock: AudioClock,
        instrumental: StaticSoundData,
        voices: Vec<(String, StaticSoundData)>,
    ) -> Result<Self, AudioError> {
        let clock_handle = clock.handle();
        // Reset and freeze the shared clock before scheduling. The first sound
        // tick is therefore exactly tick 0, independent of decode time.
        clock_handle.stop();
        let start_time = ClockTime {
            clock: clock_handle.id(),
            ticks: 0,
            fraction: 0.0,
        };

        let instrumental = clock.play_static(instrumental.start_time(start_time))?;
        let mut voice_state = VoiceState {
            voices: Vec::with_capacity(voices.len()),
            volume_ticks: Vec::new(),
        };
        for (character, data) in voices {
            let handle = clock.play_static(data.start_time(start_time))?;
            voice_state.voices.push(Voice { character, handle });
        }

        clock_handle.start();
        Ok(Self {
            clock,
            clock_handle,
            instrumental: std::sync::Mutex::new(instrumental),
            voices: std::sync::Mutex::new(voice_state),
        })
    }

    /// Song position in milliseconds, read from the shared Kira clock.
    ///
    /// Kira publishes a clock time as whole ticks plus a fractional tick, and
    /// this crate configures the clock at
    /// [`CLOCK_HZ`](crate::clock::CLOCK_HZ) = 1000 ticks per second at rate
    /// 1.0. One tick is therefore exactly one millisecond and the existing
    /// [`AudioClock::song_position_ms`] conversion (ticks + fraction, plus the
    /// zero start offset) already returns milliseconds; no seconds or
    /// microsecond round-trip is performed here. Song playback never changes
    /// the clock rate.
    pub fn position_ms(&self) -> Result<f64, AudioError> {
        self.clock.song_position_ms()
    }

    /// Sets one character's voice volume (linear amplitude, clamped to
    /// `[0.0, 1.0]`) and appends the effective value to the volume-tick log.
    ///
    /// `NaN` is rejected with [`AudioError::InvalidVolume`]; infinities clamp
    /// like any other out-of-range value. Unknown characters return
    /// [`AudioError::UnknownVoice`] without recording a tick.
    pub fn set_voice_volume(&self, character: &str, volume: f32) -> Result<(), AudioError> {
        if volume.is_nan() {
            return Err(AudioError::InvalidVolume(volume));
        }
        let volume = volume.clamp(0.0, 1.0);
        let decibels = amplitude_to_decibels(volume);

        let mut state = lock(&self.voices);
        let mut matched = false;
        for voice in state
            .voices
            .iter_mut()
            .filter(|voice| voice.character == character)
        {
            voice.handle.set_volume(decibels, IMMEDIATE);
            matched = true;
        }
        if !matched {
            return Err(AudioError::UnknownVoice(character.to_owned()));
        }
        state.volume_ticks.push((character.to_owned(), volume));
        Ok(())
    }

    /// Drains the phase-2 mute/restore tick record in call order.
    ///
    /// Each [`SongPlayback::set_voice_volume`] call appends the effective
    /// `(character, volume)` once; the returned vector preserves that ordering
    /// and the internal log is cleared, so repeated calls observe disjoint
    /// batches.
    #[must_use = "reading the tick log drains it"]
    pub fn voice_volume_ticks(&self) -> Vec<(String, f32)> {
        std::mem::take(&mut lock(&self.voices).volume_ticks)
    }

    /// Pauses the shared clock and every track. The song position freezes.
    pub fn pause(&self) -> Result<(), AudioError> {
        self.clock_handle.pause();
        lock(&self.instrumental).pause(IMMEDIATE);
        for voice in lock(&self.voices).voices.iter_mut() {
            voice.handle.pause(IMMEDIATE);
        }
        Ok(())
    }

    /// Resumes every track and then the shared clock, continuing from the
    /// paused position.
    pub fn resume(&self) -> Result<(), AudioError> {
        lock(&self.instrumental).resume(IMMEDIATE);
        for voice in lock(&self.voices).voices.iter_mut() {
            voice.handle.resume(IMMEDIATE);
        }
        self.clock_handle.start();
        Ok(())
    }

    /// Stops all tracks and the clock. The consumed playback is not reusable.
    pub fn stop(self) -> Result<(), AudioError> {
        self.clock_handle.pause();
        self.instrumental
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .stop(IMMEDIATE);
        let mut state = self
            .voices
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for voice in &mut state.voices {
            voice.handle.stop(IMMEDIATE);
        }
        self.clock_handle.stop();
        Ok(())
    }
}

fn decode_audio(bytes: AudioBytes) -> Result<StaticSoundData, AudioError> {
    StaticSoundData::from_cursor(Cursor::new(bytes.bytes))
        .map_err(|error| AudioError::Decode(error.to_string()))
}

/// Opens the real device backend when `real-audio` is enabled; the default
/// build fails closed instead of silently playing on the simulation backend.
#[cfg(feature = "real-audio")]
fn open_clock() -> Result<AudioClock, AudioError> {
    AudioClock::new()
}

#[cfg(not(feature = "real-audio"))]
fn open_clock() -> Result<AudioClock, AudioError> {
    Err(AudioError::AudioUnavailable(
        "oxide-audio was built without the `real-audio` feature".to_owned(),
    ))
}

/// Linear amplitude (`0.0..=1.0`) to Kira's decibel volume parameter.
fn amplitude_to_decibels(amplitude: f32) -> Decibels {
    if amplitude <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * amplitude.log10())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(tag: u8) -> AudioBytes {
        AudioBytes::new(vec![b'O', b'g', b'g', b'S', tag]).expect("non-empty bytes")
    }

    #[cfg(not(feature = "real-audio"))]
    fn voice(character: &str, tag: u8) -> VoiceTrack {
        VoiceTrack {
            character: character.to_owned(),
            data: bytes(tag),
        }
    }

    #[test]
    fn audio_bytes_reject_empty_input() {
        let error = AudioBytes::new(Vec::new()).expect_err("empty bytes must fail");
        assert!(matches!(error, AudioError::EmptyAudio));
    }

    #[test]
    fn audio_bytes_accept_opaque_non_empty_data() {
        // Validation is non-emptiness only; a non-OGG payload still constructs.
        let data = AudioBytes::new(vec![0, 1, 2, 3]).expect("non-empty bytes");
        assert_eq!(data.bytes.len(), 4);
    }

    /// The play session owns the playback from a Bevy resource, which requires
    /// `Send + Sync`; the handles and manager are behind mutexes/shared state.
    #[test]
    fn song_playback_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<SongPlayback>();
    }

    #[test]
    fn decoding_non_audio_bytes_is_a_typed_error() {
        let data = AudioBytes::new(vec![0_u8; 128]).expect("non-empty bytes");
        match decode_audio(data) {
            Err(AudioError::Decode(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("garbage bytes must not decode"),
        }
    }

    #[test]
    fn start_requires_at_least_one_voice() {
        match SongPlayback::start(bytes(1), Vec::new()) {
            Err(AudioError::NoVoices) => {}
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("empty voice list must fail"),
        }
    }

    #[cfg(not(feature = "real-audio"))]
    #[test]
    fn start_fails_closed_without_real_audio() {
        match SongPlayback::start(bytes(1), vec![voice("bf", 2)]) {
            Err(AudioError::AudioUnavailable(_)) => {}
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("the simulation backend must not be substituted for playback"),
        }
    }

    #[cfg(not(feature = "real-audio"))]
    fn silent_sound() -> StaticSoundData {
        StaticSoundData {
            sample_rate: 1000,
            frames: vec![kira::Frame::ZERO; 60_000].into(),
            settings: kira::sound::static_sound::StaticSoundSettings::new(),
            slice: None,
        }
    }

    #[cfg(not(feature = "real-audio"))]
    fn simulated_playback(characters: &[&str]) -> SongPlayback {
        let clock = AudioClock::new().expect("create the Kira simulation manager");
        let voices = characters
            .iter()
            .map(|character| ((*character).to_owned(), silent_sound()))
            .collect();
        SongPlayback::from_decoded(clock, silent_sound(), voices).expect("schedule silent tracks")
    }

    #[cfg(not(feature = "real-audio"))]
    #[test]
    fn voice_volume_ticks_are_clamped_ordered_and_drained() {
        let playback = simulated_playback(&["bf", "dad"]);

        playback.set_voice_volume("bf", -1.0).expect("clamp up");
        playback.set_voice_volume("dad", 2.0).expect("clamp down");
        playback.set_voice_volume("bf", 0.25).expect("mid volume");

        assert_eq!(
            playback.voice_volume_ticks(),
            vec![
                ("bf".to_owned(), 0.0),
                ("dad".to_owned(), 1.0),
                ("bf".to_owned(), 0.25),
            ]
        );
        assert!(playback.voice_volume_ticks().is_empty(), "log drains");
    }

    #[cfg(not(feature = "real-audio"))]
    #[test]
    fn unknown_voice_and_nan_volume_are_typed_and_not_recorded() {
        let playback = simulated_playback(&["bf"]);

        let error = playback
            .set_voice_volume("gf", 0.5)
            .expect_err("unknown character");
        assert!(matches!(error, AudioError::UnknownVoice(character) if character == "gf"));
        assert!(playback.voice_volume_ticks().is_empty());

        let error = playback
            .set_voice_volume("bf", f32::NAN)
            .expect_err("NaN volume");
        assert!(matches!(error, AudioError::InvalidVolume(value) if value.is_nan()));
        assert!(playback.voice_volume_ticks().is_empty());
    }

    #[cfg(not(feature = "real-audio"))]
    #[test]
    fn pause_resume_and_stop_drive_the_simulation_clock() {
        let playback = simulated_playback(&["bf"]);
        assert_eq!(playback.position_ms().expect("position"), 0.0);

        // Test-only driver access: the simulation backend only advances when
        // pumped (same pattern as the existing clock tests).
        playback.clock.advance_simulation_ms(100.0);
        let played = playback.position_ms().expect("position");
        assert!((played - 100.0).abs() <= 1e-6, "played {played}");

        playback.pause().expect("pause");
        playback.clock.advance_simulation_ms(50.0);
        let paused = playback.position_ms().expect("position");
        assert!(
            (paused - played).abs() <= 1e-9,
            "position froze at {paused}"
        );

        playback.resume().expect("resume");
        playback.clock.advance_simulation_ms(25.0);
        let resumed = playback.position_ms().expect("position");
        assert!(
            (resumed - (played + 25.0)).abs() <= 1e-6,
            "resumed at {resumed}"
        );

        playback.stop().expect("stop");
    }
}
