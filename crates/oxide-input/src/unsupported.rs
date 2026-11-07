//! Non-Linux placeholder for [`InputStream`].
//!
//! The public capture API is frozen at the crate root so portable consumers can
//! compile on other platforms; every operation reports
//! [`InputError::UnsupportedPlatform`]. No evdev/uinput code is compiled here.

use crate::{InputError, KeyEvent};

/// Non-Linux input stream. Construction always fails with
/// [`InputError::UnsupportedPlatform`].
pub struct InputStream {
    #[allow(dead_code)]
    _private: (),
}

impl InputStream {
    /// Not implemented on this platform.
    pub fn open(_path: &str) -> Result<Self, InputError> {
        Err(InputError::UnsupportedPlatform)
    }

    /// Not implemented on this platform.
    pub fn open_virtual() -> Result<Self, InputError> {
        Err(InputError::UnsupportedPlatform)
    }

    /// Yields no events on this platform.
    pub fn poll(&mut self, _now_audio_ms: f64) -> impl Iterator<Item = KeyEvent> {
        std::iter::empty()
    }

    /// Always `None`; no device read can fail here.
    pub fn take_error(&mut self) -> Option<InputError> {
        None
    }
}
