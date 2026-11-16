// Public input API surface exposed for UI/menu and future gameplay features.
#![allow(dead_code)]

pub mod controls;
pub mod cursor;
pub mod precise;
pub mod turbo;

pub use controls::{Action, Controls, keycode_aliases, primary_keycode_name};
pub use cursor::CursorState;
pub use precise::PreciseInputManager;

