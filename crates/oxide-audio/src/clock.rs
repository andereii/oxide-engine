//! Kira-backed audio clock.
//!
//! Kira's tick rate is pinned to [`CLOCK_HZ`] = 1000 ticks per second at rate
//! 1.0, so one tick is exactly one millisecond of song position. The clock is
//! the single source of truth for `song_position_ms`; it is never derived from
//! frame deltas.
//!
//! By default this crate builds Kira *without* its `cpal` backend
//! (`default-features = false`), which makes [`kira::backend::DefaultBackend`]
//! the simulation (mock) backend. That backend advances only when its
//! `AudioClock::advance_simulation_ms` driver is pumped, which keeps tests and
//! headless replays free of native audio dependencies. Enabling the optional
//! `real-audio` feature turns on `kira/cpal` for actual playback.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use kira::clock::ClockHandle as KiraClockHandle;
use kira::clock::{ClockId, ClockSpeed, ClockTime};
use kira::{AudioManager, AudioManagerSettings, Tween};
use thiserror::Error;

#[cfg(not(feature = "real-audio"))]
use kira::backend::mock::MockBackendSettings;

/// Backend selection: simulation by default, cpal behind `real-audio`.
#[cfg(not(feature = "real-audio"))]
type Backend = kira::backend::mock::MockBackend;
#[cfg(feature = "real-audio")]
type Backend = kira::backend::cpal::CpalBackend;

/// Kira ticks per second at rate 1.0; one tick is one millisecond of song time.
pub const CLOCK_HZ: f64 = 1000.0;

/// Sample rate of the simulation backend, chosen so one [`AudioManager`] buffer
/// equals one millisecond (buffer size 1 sample).
#[cfg(not(feature = "real-audio"))]
const SIMULATION_SAMPLE_RATE: u32 = 1000;
/// Internal buffer size for the simulation backend, in samples.
#[cfg(not(feature = "real-audio"))]
const SIMULATION_BUFFER_FRAMES: usize = 1;

/// Errors surfaced by the clock wrapper.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("audio backend error: {0}")]
    Backend(String),
    #[error("audio clock error: {0}")]
    Clock(String),
    #[error("audio decode error: {0}")]
    Decode(String),
}

/// A cloneable handle to the shared Kira clock.
///
/// Kira's own `ClockHandle` is not `Clone` (dropping it removes the clock), so
/// this wrapper shares the handle through an [`Arc`]. Two clones observe the
/// exact same tick timeline, which is what M5 needs to schedule Inst and Voices
/// on one clock.
#[derive(Debug, Clone)]
pub struct ClockHandle {
    clock: Arc<Mutex<KiraClockHandle>>,
}

impl ClockHandle {
    /// The current clock time. The tick unit is one millisecond of song
    /// position at rate 1.0.
    #[must_use]
    pub fn time(&self) -> ClockTime {
        lock(&self.clock).time()
    }

    /// The underlying Kira clock id, shared by every clone of this handle.
    #[must_use]
    pub fn id(&self) -> ClockId {
        lock(&self.clock).id()
    }
}

/// Kira clock wrapper. `song_position_ms = start_offset + elapsed clock time`.
pub struct AudioClock {
    manager: Mutex<AudioManager<Backend>>,
    clock: Arc<Mutex<KiraClockHandle>>,
    /// `start_offset_ms` is set through `&self`, so it lives in an atomic.
    start_offset_bits: AtomicU64,
    /// Playback rate; `set_rate` takes `&mut self`.
    rate: f64,
}

impl AudioClock {
    /// Creates a running clock at position 0.
    pub fn new() -> Result<Self, AudioError> {
        let mut manager = AudioManager::<Backend>::new(manager_settings())
            .map_err(|error| AudioError::Backend(format!("{error:?}")))?;
        let mut clock = manager
            .add_clock(clock_speed(1.0))
            .map_err(|error| AudioError::Clock(error.to_string()))?;
        clock.start();

        Ok(Self {
            manager: Mutex::new(manager),
            clock: Arc::new(Mutex::new(clock)),
            start_offset_bits: AtomicU64::new(0.0f64.to_bits()),
            rate: 1.0,
        })
    }

    /// Current song position in milliseconds, read from the Kira clock ticks
    /// plus the configured start offset.
    pub fn song_position_ms(&self) -> Result<f64, AudioError> {
        let elapsed_ticks = {
            let clock = lock(&self.clock);
            let time = clock.time();
            time.ticks as f64 + time.fraction
        };
        Ok(self.start_offset_ms() + elapsed_ticks)
    }

    /// Kira tick rate in Hz at rate 1.0. One tick is one millisecond of song
    /// position; [`AudioClock::set_rate`] changes how fast ticks accumulate.
    #[must_use]
    pub fn clock_hz(&self) -> f64 {
        CLOCK_HZ
    }

    /// Seeks by destroying the current Kira clock and creating a fresh one
    /// started at `ms` (the contract's "pause/seek = destroy + recreate").
    ///
    /// The new clock keeps the current [`AudioClock::set_rate`] speed. Because
    /// clock commands are applied on the next audio buffer, the position reads
    /// exactly `ms` immediately after this call and advances only once the
    /// backend has processed the start command.
    pub fn set_start_offset_ms(&self, ms: f64) -> Result<(), AudioError> {
        if !ms.is_finite() {
            return Err(AudioError::Clock(
                "start offset must be a finite number of milliseconds".to_string(),
            ));
        }

        let mut manager = lock(&self.manager);
        let mut clock = manager
            .add_clock(clock_speed(self.rate))
            .map_err(|error| AudioError::Clock(error.to_string()))?;
        clock.start();

        // Dropping the previous handle marks its clock for removal; the backend
        // reaps it on the next processed buffer.
        *lock(&self.clock) = clock;
        self.start_offset_bits.store(ms.to_bits(), Ordering::SeqCst);
        Ok(())
    }

    /// Sets the playback rate. `0.0` pauses the clock (the song position stops
    /// advancing); a positive rate scales how fast the position moves.
    pub fn set_rate(&mut self, rate: f64) -> Result<(), AudioError> {
        if !rate.is_finite() || rate < 0.0 {
            return Err(AudioError::Clock(
                "playback rate must be a finite, non-negative number".to_string(),
            ));
        }

        self.rate = rate;
        lock(&self.clock).set_speed(
            clock_speed(rate),
            Tween {
                duration: std::time::Duration::ZERO,
                ..Tween::default()
            },
        );
        Ok(())
    }

    /// A cloneable handle to this clock, for sharing the timeline with M5's
    /// instrumental and voice playback.
    #[must_use]
    pub fn handle(&self) -> ClockHandle {
        ClockHandle {
            clock: Arc::clone(&self.clock),
        }
    }

    /// Advances the simulation backend by `ms` milliseconds.
    ///
    /// Only available without the `real-audio` feature: the mock backend has no
    /// audio device and must be pumped manually. Each processed buffer covers
    /// exactly one millisecond (sample rate 1000 Hz, buffer size 1 sample),
    /// applying queued clock commands first.
    #[cfg(not(feature = "real-audio"))]
    pub fn advance_simulation_ms(&self, ms: f64) {
        if !ms.is_finite() || ms <= 0.0 {
            return;
        }

        let buffers = ms.round() as u64;
        let mut manager = lock(&self.manager);
        for _ in 0..buffers {
            manager.backend_mut().on_start_processing();
            manager.backend_mut().process();
        }
        // Kira publishes `ClockShared` (what `ClockHandle::time` reads) during
        // `on_start_processing`, i.e. at the start of a buffer. Publish the last
        // processed buffer so the handle reflects the simulated time we just
        // advanced through, matching what the next audio callback would see.
        manager.backend_mut().on_start_processing();
    }

    fn start_offset_ms(&self) -> f64 {
        f64::from_bits(self.start_offset_bits.load(Ordering::SeqCst))
    }
}

/// `ClockSpeed` that makes `rate` milliseconds of song time elapse per
/// millisecond of clock time.
fn clock_speed(rate: f64) -> ClockSpeed {
    ClockSpeed::TicksPerSecond(CLOCK_HZ * rate)
}

/// Lock helper: a poisoned mutex is recoverable here because the protected data
/// is plain clock state, not an invariant that a panic could have broken.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(not(feature = "real-audio"))]
fn manager_settings() -> AudioManagerSettings<Backend> {
    AudioManagerSettings {
        internal_buffer_size: SIMULATION_BUFFER_FRAMES,
        backend_settings: MockBackendSettings {
            sample_rate: SIMULATION_SAMPLE_RATE,
        },
        ..AudioManagerSettings::default()
    }
}

#[cfg(feature = "real-audio")]
fn manager_settings() -> AudioManagerSettings<Backend> {
    AudioManagerSettings::default()
}
