//! Oxide Engine audio crate.
//!
//! * [`conductor`] — pure `f64` port of `funkin/Conductor.hx` timing semantics.
//!   No audio backend and no game dependencies.
//! * [`clock`] — thin wrapper around Kira's clock. The default build uses Kira's
//!   simulation backend (no native audio dependency); the optional `real-audio`
//!   feature enables the cpal backend.

pub mod clock;
pub mod conductor;
