//! Bevy integration and game-side deterministic systems.
//!
//! The M5 phase-1 vertical-slice simulation lives in [`slice`]; it is
//! deliberately Bevy-free so the acceptance harness can drive it headlessly.
//! The M5 phase-2b visual slice renderer lives in [`render`].

pub mod render;
pub mod slice;
