//! `cargo xtask` — the verification and tooling gate for Oxide Engine.
//!
//! Truth source: AGENTS.md §2. A gate that is not implemented yet must exit 2
//! with "not implemented yet", never 0. Gate changes require an ADR in docs/DECISIONS.md.

mod conductor_check;
mod conductor_native_check;
mod gameplay_sim;
mod input_jitter;
mod media_check;
mod note_native_check;
mod parse_all;
mod parse_atlas;
mod replay;
mod verify;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprintln!("usage: cargo xtask <command> [args]");
            eprintln!(
                "commands: verify [--help] | fmt | clippy | test | parse-all | parse-atlas | fixtures-conductor | conductor-native | note-native | replay | gameplay-sim | input-jitter | render-diff | perf-budget | media-check | pack"
            );
            return ExitCode::from(2);
        }
    };

    match command {
        "verify" => verify::run(rest),
        "fmt" => to_code(fmt(rest)),
        "clippy" => to_code(clippy(rest)),
        "test" => to_code(test(rest)),
        "parse-all" => to_code(parse_all::run(rest)),
        "parse-atlas" => to_code(parse_atlas::run(rest)),
        "media-check" => to_code(media_check::media_check(rest)),
        "input-jitter" => input_jitter::run(rest),
        "conductor-native" => conductor_native_check::run(rest),
        "note-native" => note_native_check::run(rest),
        "pack" => to_code(media_check::pack(rest)),
        "fixtures-conductor" => to_code(conductor_check::run(rest)),
        "replay" => replay::run(rest),
        "gameplay-sim" => gameplay_sim::run(rest),
        "perf-budget" | "render-diff" => {
            eprintln!("{command}: not implemented yet");
            ExitCode::from(2)
        }
        other => {
            eprintln!("unknown xtask command: {other}");
            ExitCode::from(2)
        }
    }
}

fn to_code(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn run_cargo(description: &str, cargo_args: &[&str]) -> bool {
    let mut cmd = Command::new(env!("CARGO"));
    cmd.args(cargo_args);
    match cmd.status() {
        Ok(status) if status.success() => true,
        Ok(status) => {
            eprintln!("{description}: failed (exit status {status})");
            false
        }
        Err(err) => {
            eprintln!("{description}: could not launch cargo: {err}");
            false
        }
    }
}

fn fmt(_rest: &[String]) -> bool {
    run_cargo("cargo fmt", &["fmt", "--all", "--", "--check"])
}

fn clippy(_rest: &[String]) -> bool {
    run_cargo(
        "cargo clippy",
        &[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
}

fn test(_rest: &[String]) -> bool {
    run_cargo("cargo test", &["test", "--locked", "--workspace"])
}
