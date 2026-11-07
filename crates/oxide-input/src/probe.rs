//! Engine-decision uinput jitter probe.
//!
//! [`run`] measures uinput-to-userspace delivery jitter, **not** audio-device
//! accuracy. It emits synthetic `Down`/`Up` pairs across [`SECONDARY_CODES`]
//! through a virtual device. For every event returned by
//! [`InputStream::poll`], the delta is:
//!
//! ```text
//! delta_ms = wall_read_ms (CLOCK_REALTIME, read immediately after poll)
//!          - kernel_timestamp_ms (event's realtime kernel timestamp)
//! ```
//!
//! evdev kernel timestamps are `CLOCK_REALTIME`, which is why the same clock is
//! used for the read. This measures only the uinput-to-userspace path. It does
//! **not** establish a correlation with a real audio clock or audio device;
//! that parity evidence is pending M5/hardware and this report must not be
//! presented as audio-device accuracy.
//!
//! Percentiles are nearest-rank on a sorted copy: for `N` samples, rank
//! `ceil(p/100 * N)` (1-based) selects the reported value. If a poll yields
//! fewer events than were emitted before the delivery deadline, only the
//! matched events contribute to [`JitterReport::samples`].
//!
//! The probe is a bounded measurement, not a gate: it never waits indefinitely.

use crate::{InputError, JitterReport};

#[cfg(target_os = "linux")]
use crate::{InputStream, KeyEvent, SECONDARY_CODES};

#[cfg(target_os = "linux")]
use evdev::{EventType, InputEvent};

#[cfg(target_os = "linux")]
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Runs the engine-decision delivery-jitter measurement for `samples` synthetic
/// key events.
///
/// `samples` must be at least 1; zero is rejected with
/// [`InputError::InvalidSampleCount`] rather than panicking.
///
/// The synthetic stream cycles the eight [`SECONDARY_CODES`] as Down/Up pairs,
/// so every emitted event is a real state transition (the kernel suppresses
/// duplicate key states). Events are emitted in small batches and polled
/// without blocking. The result counts only events actually observed; if none
/// are observed, [`InputError::ProbeNoEvents`] is returned.
///
/// # Parity boundary
///
/// This is an **engine-decision** measurement of uinput delivery jitter. It is
/// not a real device trace and not an AudioClock correlation. The real audio
/// clock does not exist until M5/hardware evidence is available.
#[cfg(target_os = "linux")]
pub fn run(samples: usize) -> Result<JitterReport, InputError> {
    if samples == 0 {
        return Err(InputError::InvalidSampleCount);
    }

    const BATCH_EVENTS: usize = 16;
    const DELIVERY_TIMEOUT: Duration = Duration::from_millis(250);

    let mut stream = InputStream::open_virtual()?;
    let mut deltas = Vec::with_capacity(samples.min(4096));
    let mut next = 0;

    'chunks: while next < samples {
        let end = next.saturating_add(BATCH_EVENTS).min(samples);
        let batch = synthetic_batch(next..end);
        let expected = batch.len();
        stream.emit_virtual(&batch)?;

        let deadline = Instant::now() + DELIVERY_TIMEOUT;
        let mut observed = 0;
        while observed < expected {
            let events = stream.poll(0.0);
            // Read the realtime clock immediately after the poll syscall, before
            // consuming the already-collected iterator. One value applies to all
            // events returned by this poll.
            let wall_read_ms = clock_realtime_ms()?;
            let events: Vec<KeyEvent> = events.collect();

            if let Some(error) = stream.take_error() {
                return Err(error);
            }
            if events.is_empty() {
                if Instant::now() >= deadline {
                    break 'chunks;
                }
                std::thread::yield_now();
                continue;
            }

            observed += events.len();
            for event in events {
                let kernel_ms = event.kernel_time_us as f64 / 1000.0;
                deltas.push(wall_read_ms - kernel_ms);
            }
        }

        if observed < expected {
            break;
        }
        next = end;
    }

    let p50_ms = percentile_nearest_rank(&deltas, 50.0).ok_or(InputError::ProbeNoEvents)?;
    let p99_ms = percentile_nearest_rank(&deltas, 99.0).ok_or(InputError::ProbeNoEvents)?;
    Ok(JitterReport {
        p50_ms,
        p99_ms,
        samples: deltas.len(),
    })
}

/// Non-Linux stub: the evdev/uinput measurement is Linux-only.
#[cfg(not(target_os = "linux"))]
pub fn run(samples: usize) -> Result<JitterReport, InputError> {
    if samples == 0 {
        return Err(InputError::InvalidSampleCount);
    }
    Err(InputError::UnsupportedPlatform)
}

#[cfg(target_os = "linux")]
fn synthetic_batch(range: std::ops::Range<usize>) -> Vec<InputEvent> {
    range
        .map(|index| {
            let code = SECONDARY_CODES[(index / 2) % SECONDARY_CODES.len()];
            let value = if index % 2 == 0 { 1 } else { 0 };
            InputEvent::new(EventType::KEY.0, code, value)
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn clock_realtime_ms() -> Result<f64, InputError> {
    // On Unix, `SystemTime::now()` reads CLOCK_REALTIME. evdev kernel
    // timestamps use the same clock, so the subtraction has a defined parity
    // meaning for the uinput->userspace path.
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| InputError::ClockUnavailable)?;
    Ok(since_epoch.as_secs_f64() * 1000.0)
}

/// Nearest-rank percentile on an unsorted slice: sorts a copy, then selects
/// rank `ceil(percentile/100 * N)` (1-based). Returns `None` for an empty
/// slice.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn percentile_nearest_rank(values: &[f64], percentile: f64) -> Option<f64> {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    percentile_nearest_rank_sorted(&sorted, percentile)
}

/// Nearest-rank selection on an already-sorted slice.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn percentile_nearest_rank_sorted(sorted: &[f64], percentile: f64) -> Option<f64> {
    let len = sorted.len();
    if len == 0 {
        return None;
    }
    let rank = (percentile / 100.0 * len as f64).ceil();
    // `max(1.0)` also turns NaN/infinity inputs into a safe 1-based rank.
    let index = ((rank.max(1.0) as usize) - 1).min(len - 1);
    sorted.get(index).copied()
}

#[cfg(test)]
mod tests {
    use super::{percentile_nearest_rank, percentile_nearest_rank_sorted};

    #[test]
    fn nearest_rank_odd_length() {
        let values = [1.0, 2.0, 3.0, 4.0, 5.0];
        assert_eq!(percentile_nearest_rank_sorted(&values, 50.0), Some(3.0));
        assert_eq!(percentile_nearest_rank_sorted(&values, 99.0), Some(5.0));
    }

    #[test]
    fn nearest_rank_even_length() {
        let values = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(percentile_nearest_rank_sorted(&values, 50.0), Some(2.0));
        assert_eq!(percentile_nearest_rank_sorted(&values, 99.0), Some(4.0));
    }

    #[test]
    fn nearest_rank_single_sample() {
        let values = [7.5];
        assert_eq!(percentile_nearest_rank_sorted(&values, 50.0), Some(7.5));
        assert_eq!(percentile_nearest_rank_sorted(&values, 99.0), Some(7.5));
    }

    #[test]
    fn percentile_sorts_a_copy_without_mutating_input() {
        let values = [4.0, 1.0, 3.0, 2.0];
        assert_eq!(percentile_nearest_rank(&values, 50.0), Some(2.0));
        assert_eq!(values, [4.0, 1.0, 3.0, 2.0]);
    }

    #[test]
    fn nearest_rank_empty_is_none() {
        assert_eq!(percentile_nearest_rank_sorted(&[], 50.0), None);
        assert_eq!(percentile_nearest_rank(&[], 99.0), None);
    }
}
