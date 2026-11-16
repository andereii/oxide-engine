//! Gameplay events and softcoded `NoteKind` script integration.

pub mod notekind;

pub use notekind::{append_notekind_sidecar_scripts, apply_notekind_to_play_state};
