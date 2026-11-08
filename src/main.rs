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
        eprintln!("commands: pack | play | new | check | dev");
        return ExitCode::from(2);
    }
    let command = args[0].as_str();
    match command {
        "pack" => run_pack(&args[1..]),
        "play" => run_play(&args[1..]),
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

/// ADR-0021: interactive bopeebo playtest window. Requires the `audio`
/// feature (real kira/cpal backend); a default build fails closed with a
/// typed AudioUnavailable error after loading slice/scene/HUD and building
/// the Bevy app, before `app.run()` — no window is shown on any host where
/// that construction succeeds.
fn run_play(rest: &[String]) -> ExitCode {
    let config = match parse_play_options(rest) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("play: {message}");
            eprintln!("usage: oxide play --assets <dir> [--difficulty easy|normal|hard]");
            return ExitCode::from(2);
        }
    };
    match oxide_game::play::PlaySession::run(config) {
        Ok(report) => {
            println!(
                "play: song position {:.1} ms; quit: {:?}",
                report.song_position_ms, report.quit_reason
            );
            println!(
                "tallies: sick {} good {} bad {} shit {} missed {}",
                report.tallies.sick,
                report.tallies.good,
                report.tallies.bad,
                report.tallies.shit,
                report.tallies.missed,
            );
            println!(
                "score {:.1}; health {:.3}; voice mute ticks {}",
                report.score,
                report.health,
                report.mute_tick_count(),
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("play: {error}");
            ExitCode::FAILURE
        }
    }
}

fn parse_play_options(rest: &[String]) -> Result<oxide_game::play::PlayConfig, String> {
    let mut assets: Option<PathBuf> = None;
    let mut difficulty = oxide_game::play::Difficulty::Easy;
    let mut index = 0;
    while let Some(argument) = rest.get(index) {
        match argument.as_str() {
            "--assets" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| "--assets requires a value".to_string())?;
                assets = Some(expand_tilde(value));
                index += 2;
            }
            "--difficulty" => {
                let value = rest
                    .get(index + 1)
                    .ok_or_else(|| "--difficulty requires a value".to_string())?;
                difficulty = oxide_game::play::Difficulty::parse(value)
                    .map_err(|error| format!("invalid difficulty: {error}"))?;
                index += 2;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let assets_root =
        assets.ok_or_else(|| "--assets <dir> is required for play (fail closed)".to_string())?;
    Ok(oxide_game::play::PlayConfig {
        assets_root,
        difficulty,
        window: oxide_game::play::WindowConfig::default(),
    })
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

#[cfg(test)]
mod play_cli_tests {
    use super::*;

    #[test]
    fn play_requires_explicit_assets() {
        let Some(error) =
            parse_play_options(&["--difficulty".to_string(), "easy".to_string()]).err()
        else {
            panic!("missing --assets must fail");
        };
        assert!(error.contains("--assets"));
    }

    #[test]
    fn play_parses_difficulty_and_unknown_flags_fail() {
        let config = parse_play_options(&[
            "--assets".to_string(),
            "/assets".to_string(),
            "--difficulty".to_string(),
            "hard".to_string(),
        ])
        .expect("valid flags");
        assert!(config.assets_root.ends_with("assets"));
        assert_eq!(config.difficulty, oxide_game::play::Difficulty::Hard);

        let Some(error) = parse_play_options(&[
            "--assets".to_string(),
            "/assets".to_string(),
            "--mystery".to_string(),
        ])
        .err() else {
            panic!("unknown flag must fail");
        };
        assert!(error.contains("--mystery"));

        let Some(error) = parse_play_options(&[
            "--assets".to_string(),
            "/assets".to_string(),
            "--difficulty".to_string(),
            "nightmare".to_string(),
        ])
        .err() else {
            panic!("invalid difficulty must fail");
        };
        assert!(error.contains("invalid difficulty"));
    }
}
