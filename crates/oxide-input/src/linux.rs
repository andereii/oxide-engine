//! Linux evdev implementation of [`InputStream`].

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, BusType, Device, EventType, InputEvent, InputId, KeyCode};

use crate::{InputError, KeyEvent, KeyState, SECONDARY_CODES, io_error};

/// Timeout for a reader open that is waiting for udev to apply the session ACL.
const VIRTUAL_OPEN_TIMEOUT: Duration = Duration::from_millis(1000);
/// Retry interval while waiting for that ACL. Bounded by [`VIRTUAL_OPEN_TIMEOUT`].
const VIRTUAL_OPEN_RETRY: Duration = Duration::from_millis(5);

/// A non-blocking key input stream backed by a real evdev device or a uinput
/// virtual device.
///
/// The fields are private; use [`InputStream::open`] or
/// [`InputStream::open_virtual`].
pub struct InputStream {
    reader: Device,
    virtual_device: Option<VirtualDevice>,
    label: String,
    last_error: Option<InputError>,
}

impl InputStream {
    /// Opens a real Linux evdev device (for example `/dev/input/event0`) in
    /// non-blocking mode.
    ///
    /// # Permissions fallback
    ///
    /// On this machine group `input` access is missing: the real device nodes
    /// are `root:input 0660`, while only `/dev/uinput` has a session ACL.
    /// Opening such a device returns [`InputError::PermissionDenied`], which is
    /// deliberately a distinct variant. This function does not attempt
    /// privilege escalation, device-number guessing, or a fallback to another
    /// path.
    ///
    /// # Errors
    ///
    /// [`InputError::NotFound`] for a missing path,
    /// [`InputError::PermissionDenied`] for insufficient access, and
    /// [`InputError::Io`] for other open or non-blocking-configuration errors.
    pub fn open(path: &str) -> Result<Self, InputError> {
        let reader = Device::open(path).map_err(|source| io_error(path, source))?;
        reader
            .set_nonblocking(true)
            .map_err(|source| io_error(path, source))?;
        Ok(Self {
            reader,
            virtual_device: None,
            label: path.to_owned(),
            last_error: None,
        })
    }

    /// Creates a uinput virtual device capturing the gameplay keys and opens
    /// its evdev read side.
    ///
    /// This requires write access to `/dev/uinput`; it does **not** require
    /// group `input`. The device name contains `oxide`. Its event node is
    /// discovered through sysfs after registration rather than by guessing the
    /// next `/dev/input/eventN`.
    ///
    /// # Residual constraints
    ///
    /// The read side still needs the kernel/udev-assigned permissions. On
    /// systems using systemd-udevd, the device is classified as a joystick so
    /// that `70-uaccess.rules` applies the active session's ACL. ACL
    /// application is asynchronous, so this function retries a permission-
    /// denied read-side open for up to one second before returning
    /// [`InputError::PermissionDenied`]. On a system without that udev policy,
    /// reading the node requires group `input` or root; the probe test is
    /// consequently `#[ignore]`d and marked as such.
    ///
    /// # Capability shim
    ///
    /// A key-only virtual keyboard would be classified as a normal keyboard and
    /// receive no session ACL. Two unused joystick buttons (`BTN_SOUTH` and
    /// `BTN_EAST`) are therefore advertised so the kernel/udev joystick
    /// classification grants access. They are **not** gameplay bindings, are
    /// not in [`SECONDARY_CODES`], and the probe never emits them. This is a
    /// permission shim, not an extra input contract.
    ///
    /// # Errors
    ///
    /// [`InputError::VirtualDevice`] for uinput setup failures,
    /// [`InputError::VirtualDeviceNoEventNode`] when registration produced no
    /// event node, and [`InputError::PermissionDenied`]/[`InputError::Io`] from
    /// the read side.
    pub fn open_virtual() -> Result<Self, InputError> {
        let mut keys = AttributeSet::<KeyCode>::new();
        for code in SECONDARY_CODES {
            keys.insert(KeyCode::new(code));
        }
        // Permission shim; see the method documentation. udev's input_id only
        // tags a device as a joystick (and hence applies the session uaccess
        // ACL) when it has at least two joystick buttons. These codes are never
        // emitted by this crate and are not part of SECONDARY_CODES.
        keys.insert(KeyCode::BTN_SOUTH);
        keys.insert(KeyCode::BTN_EAST);

        let mut virtual_device = VirtualDevice::builder()
            .map_err(virtual_device_error)?
            .name(b"oxide-input virtual keyboard")
            .input_id(InputId::new(BusType::BUS_VIRTUAL, 0x0d1d, 0xe001, 0x0001))
            .with_keys(&keys)
            .map_err(virtual_device_error)?
            .build()
            .map_err(virtual_device_error)?;

        let node = virtual_event_node(&mut virtual_device)?;
        let label = node.display().to_string();
        let reader = open_virtual_reader(&node)?;

        Ok(Self {
            reader,
            virtual_device: Some(virtual_device),
            label,
            last_error: None,
        })
    }

    /// Reads currently available events without blocking.
    ///
    /// Only `EV_KEY` transitions are yielded: value `1` is [`KeyState::Down`]
    /// and value `0` is [`KeyState::Up`]. Value `2` (kernel autorepeat) is
    /// deliberately skipped because repeat is not a key-state transition;
    /// mapping it to `Down` would double-count a held key. `SYN_REPORT` and all
    /// other event types are discarded.
    ///
    /// Every returned event carries the same `audio_time_ms = now_audio_ms`
    /// value supplied to this call, and the event's kernel timestamp in
    /// microseconds. Events are returned in device read order.
    ///
    /// Device read failures are swallowed (the contract returns events, not
    /// `Result`s); use [`Self::take_error`] to inspect them.
    pub fn poll(&mut self, now_audio_ms: f64) -> impl Iterator<Item = KeyEvent> {
        let read = self
            .reader
            .fetch_events()
            .map(|events| events.collect::<Vec<_>>());

        let events = match read {
            Ok(events) => events,
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Vec::new()
            }
            Err(error) => {
                self.last_error = Some(io_error(&self.label, error));
                Vec::new()
            }
        };

        let mut output = Vec::new();
        for event in events {
            match map_event(&event, now_audio_ms) {
                Ok(Some(event)) => output.push(event),
                Ok(None) => {}
                Err(error) => self.last_error = Some(error),
            }
        }
        output.into_iter()
    }

    /// Takes the last error swallowed by [`Self::poll`].
    ///
    /// Non-blocking `WouldBlock` is normal and is never stored. A stored error
    /// means a device read, timestamp conversion, or similar operation failed
    /// and the affected event(s) were not yielded.
    pub fn take_error(&mut self) -> Option<InputError> {
        self.last_error.take()
    }

    /// Emits synthetic events through the uinput device. Private to the crate;
    /// the public probe lives in [`crate::probe`].
    pub(crate) fn emit_virtual(&mut self, events: &[InputEvent]) -> Result<(), InputError> {
        let Some(device) = self.virtual_device.as_mut() else {
            return Err(virtual_device_error(io::Error::other(
                "input stream is not a virtual device",
            )));
        };
        device.emit(events).map_err(virtual_device_error)
    }
}

fn virtual_device_error(source: io::Error) -> InputError {
    InputError::VirtualDevice { source }
}

fn virtual_event_node(device: &mut VirtualDevice) -> Result<PathBuf, InputError> {
    let mut nodes = device
        .enumerate_dev_nodes_blocking()
        .map_err(virtual_device_error)?;
    match nodes.next() {
        Some(Ok(node)) => Ok(node),
        Some(Err(source)) => Err(virtual_device_error(source)),
        None => Err(InputError::VirtualDeviceNoEventNode),
    }
}

fn open_virtual_reader(node: &Path) -> Result<Device, InputError> {
    let label = node.display().to_string();
    let deadline = Instant::now() + VIRTUAL_OPEN_TIMEOUT;
    loop {
        match Device::open(node) {
            Ok(reader) => {
                reader
                    .set_nonblocking(true)
                    .map_err(|source| io_error(&label, source))?;
                return Ok(reader);
            }
            Err(source)
                if source.kind() == io::ErrorKind::PermissionDenied
                    && Instant::now() < deadline =>
            {
                // udev applies the uaccess ACL after UI_DEV_CREATE returns.
                std::thread::sleep(VIRTUAL_OPEN_RETRY);
            }
            Err(source) => return Err(io_error(&label, source)),
        }
    }
}

fn map_event(event: &InputEvent, audio_time_ms: f64) -> Result<Option<KeyEvent>, InputError> {
    if event.event_type() != EventType::KEY {
        return Ok(None);
    }

    let state = match event.value() {
        1 => KeyState::Down,
        0 => KeyState::Up,
        // ENGINE DECISION: EV_KEY value 2 is autorepeat, not a Down/Up
        // transition. Ignore it (and any other unexpected value) rather than
        // inventing a state.
        2 => return Ok(None),
        _ => return Ok(None),
    };

    let kernel_time_us =
        timestamp_to_us(event.timestamp()).ok_or(InputError::TimestampOutOfRange)?;

    Ok(Some(KeyEvent {
        audio_time_ms,
        kernel_time_us,
        code: event.code(),
        state,
    }))
}

/// Converts the evdev kernel timestamp to microseconds since the Unix epoch.
///
/// evdev exposes the kernel `timeval` as [`SystemTime`]; it has microsecond
/// resolution, so converting back with checked arithmetic is exact. Returning
/// `None` (rather than panicking or saturating) keeps malformed/pre-epoch
/// timestamps from silently becoming a plausible event.
fn timestamp_to_us(timestamp: SystemTime) -> Option<u64> {
    let since_epoch = timestamp.duration_since(UNIX_EPOCH).ok()?;
    since_epoch
        .as_secs()
        .checked_mul(1_000_000)?
        .checked_add(u64::from(since_epoch.subsec_micros()))
}

#[cfg(test)]
mod tests {
    use super::{map_event, timestamp_to_us};
    use crate::KeyState;
    use evdev::{EventType, InputEvent};
    use std::time::{Duration, UNIX_EPOCH};

    fn mapped_value(value: i32) -> Option<KeyState> {
        let event = InputEvent::new_now(EventType::KEY.0, 30, value);
        match map_event(&event, 12.5) {
            Ok(Some(event)) => Some(event.state),
            Ok(None) => None,
            Err(error) => panic!("unexpected mapping error: {error:?}"),
        }
    }

    #[test]
    fn key_values_map_to_down_and_up() {
        assert_eq!(mapped_value(1), Some(KeyState::Down));
        assert_eq!(mapped_value(0), Some(KeyState::Up));
    }

    #[test]
    fn repeat_and_unknown_values_are_not_key_states() {
        assert_eq!(mapped_value(2), None);
        assert_eq!(mapped_value(7), None);
    }

    #[test]
    fn non_key_events_are_discarded() {
        let event = InputEvent::new_now(EventType::SYNCHRONIZATION.0, 0, 0);
        match map_event(&event, 1.0) {
            Ok(None) => {}
            other => panic!("non-key event was not discarded: {other:?}"),
        }
    }

    #[test]
    fn mapped_event_keeps_audio_time_code_and_state() {
        let event = InputEvent::new_now(EventType::KEY.0, 30, 1);
        let mapped = match map_event(&event, 42.5) {
            Ok(Some(mapped)) => mapped,
            other => panic!("expected one key event, got {other:?}"),
        };
        assert_eq!(mapped.audio_time_ms, 42.5);
        assert_eq!(mapped.code, 30);
        assert_eq!(mapped.state, KeyState::Down);
        assert!(mapped.kernel_time_us > 0);
    }

    #[test]
    fn kernel_timestamp_converts_to_exact_microseconds() {
        assert_eq!(timestamp_to_us(UNIX_EPOCH), Some(0));
        let timestamp = UNIX_EPOCH + Duration::new(1, 234_000_000);
        assert_eq!(timestamp_to_us(timestamp), Some(1_234_000));
    }

    #[test]
    fn kernel_timestamp_overflow_is_checked() {
        let seconds = u64::MAX / 1_000_000 + 1;
        let timestamp = UNIX_EPOCH + Duration::from_secs(seconds);
        assert_eq!(timestamp_to_us(timestamp), None);
    }

    #[test]
    fn pre_epoch_timestamp_is_rejected() {
        // A pre-epoch SystemTime is not representable as microseconds since
        // epoch, so conversion reports None instead of wrapping.
        let before_epoch = UNIX_EPOCH - Duration::from_secs(1);
        assert_eq!(timestamp_to_us(before_epoch), None);
    }
}
