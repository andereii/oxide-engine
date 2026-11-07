//! Timestamped Linux evdev key capture.
//!
//! [`InputStream::poll`] reads currently available events without blocking and
//! yields only key transitions. Non-key events, `SYN_REPORT`, and kernel key
//! autorepeat are discarded.
//!
//! # Units
//!
//! * [`KeyEvent::kernel_time_us`] is the kernel event timestamp in
//!   **microseconds** since the Unix epoch. It is taken verbatim from the evdev
//!   event (the kernel timeval has microsecond resolution).
//! * [`KeyEvent::audio_time_ms`] is the caller's audio-clock value in
//!   **milliseconds**, copied unchanged from the `now_audio_ms` argument of the
//!   [`InputStream::poll`] call that produced the event. It is identical for
//!   every event returned by one call and is never derived from frame timing.
//!
//! The crate never converts between the two clocks. The difference is the
//! observable used to measure delivery jitter.
//!
//! # Permissions and fallback
//!
//! On this machine group `input` access is missing: real `/dev/input/event*`
//! nodes are `root:input 0660`, while only `/dev/uinput` has a session ACL.
//! [`InputStream::open`] therefore returns the distinct
//! [`InputError::PermissionDenied`] variant for an existing real device it is
//! not allowed to read; it does not silently pick another device, escalate
//! privileges, or turn the failure into an empty stream. Callers should surface
//! that error (or grant group/ACL access) rather than treating it as a missing
//! device.
//!
//! [`InputStream::open_virtual`] creates a uinput device and needs write access
//! to `/dev/uinput`, not group `input`. Its read-side event node is discovered
//! through sysfs instead of guessing an event number. Reading that node still
//! depends on the system's udev `uaccess` policy; see the residual constraint on
//! [`InputStream::open_virtual`].
//!
//! # Errors from a stream
//!
//! The contract's stream yields [`KeyEvent`] items, not `Result`s, so a device
//! read failure cannot be an item. After polling, call
//! [`InputStream::take_error`] to take the last swallowed error. Silent failure
//! is therefore avoidable. Non-blocking `WouldBlock` is normal and is never
//! stored.
//!
//! # Jitter probe
//!
//! [`probe::run`] is an **engine-decision measurement**, not an audio-clock
//! parity proof. It emits synthetic uinput toggle events and measures the
//! realtime delta between each event's kernel timestamp and the userspace clock
//! read immediately after the poll that returned it. That measures
//! uinput-to-userspace delivery jitter only. It does not correlate with a real
//! `AudioClock` or audio device; that evidence remains pending M5/hardware.

/// Whether a captured key transition is a press or a release.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Down,
    Up,
}

/// One timestamped key transition.
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// Audio-clock time in milliseconds, copied from the `poll` call's
    /// `now_audio_ms` argument.
    pub audio_time_ms: f64,
    /// Kernel event timestamp in microseconds since the Unix epoch.
    pub kernel_time_us: u64,
    /// Linux evdev key code.
    pub code: u16,
    /// Press or release.
    pub state: KeyState,
}

/// Linux evdev key codes for the eight default Solo-scheme gameplay bindings.
///
/// Element order is `W`, `A`, `S`, `D`, `UP`, `LEFT`, `RIGHT`, `DOWN`.
/// This is a **derived mapping** (ADR-0015) of the observed default
/// `funkin.input.Controls.getDefaultKeybinds` Solo scheme:
///
/// * `NOTE_UP = [W, UP]` — `KEY_W = 17`, `KEY_UP = 103`
/// * `NOTE_LEFT = [A, LEFT]` — `KEY_A = 30`, `KEY_LEFT = 105`
/// * `NOTE_DOWN = [S, DOWN]` — `KEY_S = 31`, `KEY_DOWN = 108`
/// * `NOTE_RIGHT = [D, RIGHT]` — `KEY_D = 32`, `KEY_RIGHT = 106`
///
/// `PreciseInputManager` binds only `NOTE_*` actions, so each code here maps
/// 1:1 to a default gameplay binding and no `UI_*`-only key is present.
pub const SECONDARY_CODES: [u16; 8] = [17, 30, 31, 32, 103, 105, 106, 108];

/// Result of [`probe::run`].
pub struct JitterReport {
    /// 50th nearest-rank percentile of `wall_read_ms - kernel_timestamp_ms`.
    pub p50_ms: f64,
    /// 99th nearest-rank percentile of `wall_read_ms - kernel_timestamp_ms`.
    pub p99_ms: f64,
    /// Number of events actually observed and included in the percentiles.
    pub samples: usize,
}

/// Errors surfaced by input capture.
///
/// `PermissionDenied` is deliberately distinct from generic [`InputError::Io`]
/// because an existing real device without group `input`/ACL access is a
/// documented condition on this machine.
#[derive(Debug, thiserror::Error)]
pub enum InputError {
    /// The requested device path does not exist.
    #[error("input device not found: {path}")]
    NotFound { path: String },
    /// The device exists but this process may not open it.
    #[error(
        "permission denied opening input device {path} (is the process in group `input` or granted a device ACL?): {source}"
    )]
    PermissionDenied {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// Another I/O error while opening or reading a device.
    #[error("input device I/O error on {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    /// uinput creation or configuration failed.
    #[error("uinput virtual device error: {source}")]
    VirtualDevice {
        #[source]
        source: std::io::Error,
    },
    /// The uinput device was created but no event node was found in sysfs.
    #[error("uinput virtual device has no /dev/input event node")]
    VirtualDeviceNoEventNode,
    /// The jitter probe requires at least one sample.
    #[error("jitter probe requires at least one sample")]
    InvalidSampleCount,
    /// A kernel timestamp could not be represented as microseconds since epoch.
    #[error("kernel event timestamp cannot be represented as microseconds since the Unix epoch")]
    TimestampOutOfRange,
    /// `CLOCK_REALTIME` could not be read.
    #[error("CLOCK_REALTIME is unavailable")]
    ClockUnavailable,
    /// The jitter probe emitted events but observed none.
    #[error("jitter probe observed no key events")]
    ProbeNoEvents,
    /// Input capture is only implemented for Linux evdev.
    #[error("input capture is only implemented for Linux evdev")]
    #[cfg(not(target_os = "linux"))]
    UnsupportedPlatform,
}

#[cfg(target_os = "linux")]
fn io_error(path: &str, source: std::io::Error) -> InputError {
    match source.kind() {
        std::io::ErrorKind::NotFound => InputError::NotFound {
            path: path.to_owned(),
        },
        std::io::ErrorKind::PermissionDenied => InputError::PermissionDenied {
            path: path.to_owned(),
            source,
        },
        _ => InputError::Io {
            path: path.to_owned(),
            source,
        },
    }
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod unsupported;

#[cfg(target_os = "linux")]
pub use linux::InputStream;
#[cfg(not(target_os = "linux"))]
pub use unsupported::InputStream;

pub mod probe;
