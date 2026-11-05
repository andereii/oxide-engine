//! `cargo xtask parse-all` — M1 gate: every known JSON under an assets
//! directory parses and round-trips. Implemented against `oxide-data`.
//!
//! Not implemented yet in M0.
use std::process::ExitCode;

pub fn run(_rest: &[String]) -> ExitCode {
    eprintln!("parse-all: not implemented yet");
    ExitCode::from(2)
}
