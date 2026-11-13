//! `oxide` — Oxide Engine command-line entry point.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

const VERSION: &str = concat!("Oxide Engine ", env!("CARGO_PKG_VERSION"));

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return run_play(&[]);
    }
    let command = args[0].as_str();
    match command {
        "play" => run_play(&args[1..]),
        "new" | "check" | "dev" => {
            eprintln!("{command}: not implemented yet");
            ExitCode::from(2)
        }
        "--help" | "-h" | "help" => {
            eprintln!("{VERSION}");
            eprintln!("usage: oxide [command]");
            eprintln!("commands: play | new | check | dev");
            ExitCode::SUCCESS
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
    let assets_root = assets
        .or_else(default_assets_dir)
        .ok_or_else(|| "--assets <dir> is required for play (no 'assets' directory found)".to_string())?;
    Ok(oxide_game::play::PlayConfig {
        assets_root,
        difficulty,
        window: oxide_game::play::WindowConfig::default(),
    })
}

fn default_assets_dir() -> Option<PathBuf> {
    let local = PathBuf::from("assets");
    if local.is_dir() {
        return Some(local);
    }
    let manifest_assets = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    if manifest_assets.is_dir() {
        return Some(manifest_assets);
    }
    None
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
    fn play_uses_default_assets_when_omitted() {
        let config = parse_play_options(&["--difficulty".to_string(), "easy".to_string()])
            .expect("root assets directory should be used as default");
        assert!(config.assets_root.ends_with("assets"));
        assert_eq!(config.difficulty, oxide_game::play::Difficulty::Easy);
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
