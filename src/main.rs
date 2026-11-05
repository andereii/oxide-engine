//! `oxide` — Oxide Engine command-line entry point.
//!
//! M0: the binary exists and every gameplay/tooling subcommand reports
//! "not implemented yet" with exit code 2, per AGENTS.md.

use std::process::ExitCode;

const VERSION: &str = concat!("Oxide Engine ", env!("CARGO_PKG_VERSION"));

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("{VERSION}");
        eprintln!("usage: oxide <command>");
        eprintln!("commands: pack | new | check | dev");
        return ExitCode::from(2);
    }
    let command = args[0].as_str();
    match command {
        "pack" | "new" | "check" | "dev" => {
            eprintln!("{command}: not implemented yet");
            ExitCode::from(2)
        }
        "--version" | "-V" | "version" => {
            println!("{VERSION}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("unknown command: {other}");
            ExitCode::from(2)
        }
    }
}
