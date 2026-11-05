//! `oxide` — Oxide Engine command-line entry point.
//!
//! M3: `oxide pack` packs an asset tree through `oxide-media`. `new`, `check`
//! and `dev` remain M9 stubs (exit 2) per AGENTS.md.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = concat!("Oxide Engine ", env!("CARGO_PKG_VERSION"));
const DEFAULT_ASSETS: &str = "~/Descargas/fnf/Funkin/assets";
const DEFAULT_JOBS: usize = 4;

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
        "pack" => run_pack(&args[1..]),
        "new" | "check" | "dev" => {
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

fn run_pack(rest: &[String]) -> ExitCode {
    if rest.iter().any(|argument| argument == "--check-tools") {
        return match oxide_media::check_tools() {
            Ok(tools) => {
                println!("ffmpeg: {}", tools.ffmpeg_version);
                println!("ffprobe: {}", tools.ffprobe_version);
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("pack: {error}");
                ExitCode::FAILURE
            }
        };
    }

    let options = match parse_pack_options(rest) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("pack: {message}");
            return ExitCode::from(2);
        }
    };
    println!(
        "pack: {} -> {}",
        options.input.display(),
        options.output.display()
    );
    match oxide_media::pack(&options.input, &options.output, options.jobs) {
        Ok(report) => {
            println!("entries: {}", report.pak_entries);
            println!("pak sha256: {}", report.pak_sha256);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("pack: {error}");
            ExitCode::FAILURE
        }
    }
}

struct PackOptions {
    input: PathBuf,
    output: PathBuf,
    jobs: usize,
}

fn parse_pack_options(rest: &[String]) -> Result<PackOptions, String> {
    let mut input = None;
    let mut output = None;
    let mut jobs = DEFAULT_JOBS;
    let mut index = 0;
    while let Some(argument) = rest.get(index) {
        match argument.as_str() {
            "--in" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| "--in requires a value".to_string())?;
                input = Some(expand_tilde(value));
                index += 2;
            }
            "--out" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| "--out requires a value".to_string())?;
                output = Some(expand_tilde(value));
                index += 2;
            }
            "--jobs" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| "--jobs requires a value".to_string())?;
                jobs = value
                    .parse::<usize>()
                    .map_err(|_| format!("invalid --jobs value: {value}"))?;
                if jobs == 0 {
                    return Err("--jobs must be at least 1".to_string());
                }
                index += 2;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let input = input.unwrap_or_else(|| expand_tilde(DEFAULT_ASSETS));
    let output =
        output.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("assets-dist"));
    Ok(PackOptions {
        input,
        output,
        jobs,
    })
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}
