//! `cargo xtask` — the verification and tooling gate for Oxide Engine.
//!
//! Truth source: AGENTS.md §2. A gate that is not implemented yet must exit 2
//! with "not implemented yet", never 0. Gate changes require an ADR in docs/DECISIONS.md.

mod parse_all;

use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (command, rest) = match args.split_first() {
        Some((c, r)) => (c.as_str(), r),
        None => {
            eprintln!("usage: cargo xtask <command> [args]");
            eprintln!(
                "commands: verify | fmt | clippy | test | parse-all | fixtures-conductor | replay | perf-budget | media-check | pack"
            );
            return ExitCode::from(2);
        }
    };

    match command {
        "verify" => {
            // M0: verify = fmt + clippy + test. As gates get implemented
            // (parse-all in M1, fixtures-conductor/replay/perf-budget later,
            // media-check/pack in M3) they are appended here.
            if fmt(rest) && clippy(rest) && test(rest) {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        "fmt" => to_code(fmt(rest)),
        "clippy" => to_code(clippy(rest)),
        "test" => to_code(test(rest)),
        "parse-all" => parse_all::run(rest),
        "fixtures-conductor" | "replay" | "perf-budget" | "media-check" | "pack" => {
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
