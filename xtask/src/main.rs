//! `cargo xtask` — the verification and tooling gate for Oxide Engine.
//!
//! Truth source: AGENTS.md §2. A gate that is not implemented yet must exit 2
//! with "not implemented yet", never 0. Gate changes require an ADR in docs/DECISIONS.md.

mod media_check;
mod parse_all;
mod parse_atlas;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprintln!("usage: cargo xtask <command> [args]");
            eprintln!(
                "commands: verify | fmt | clippy | test | parse-all | parse-atlas | fixtures-conductor | replay | perf-budget | media-check | pack"
            );
            return ExitCode::from(2);
        }
    };

    match command {
        "verify" => {
            // As of M1: verify = fmt + clippy + test + parse-all.
            // Pending gates (appended as they get implemented): fixtures-conductor
            // and replay (M4), perf-budget (M5), media-check/pack (M3).
            if fmt(rest) && clippy(rest) && test(rest) && parse_all::run(&[]) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        "fmt" => to_code(fmt(rest)),
        "clippy" => to_code(clippy(rest)),
        "test" => to_code(test(rest)),
        "parse-all" => to_code(parse_all::run(rest)),
        "parse-atlas" => to_code(parse_atlas::run(rest)),
        "media-check" => to_code(media_check::media_check(rest)),
        "pack" => to_code(media_check::pack(rest)),
        "fixtures-conductor" | "replay" | "perf-budget" => {
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
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )
}

fn test(_rest: &[String]) -> bool {
    run_cargo("cargo test", &["test", "--workspace"])
}
