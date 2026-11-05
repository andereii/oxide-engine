//! External tool discovery, probing and the frozen ffmpeg recipes.
//!
//! ffmpeg/ffprobe are build-time binaries (ADR-0010): they are spawned here and
//! never linked. Every command runs with a fixed locale/timezone and emits
//! `-fflags +bitexact`, which makes output bytes reproducible (Ogg stream
//! serials and Matroska segment UIDs are otherwise random per run).

use std::path::Path;
use std::process::{Command, Stdio};

use crate::MediaError;

pub(crate) const FIXED_ENV: [(&str, &str); 3] = [("LC_ALL", "C"), ("LANG", "C"), ("TZ", "UTC")];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tools {
    pub ffmpeg_version: String,
    pub ffprobe_version: String,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Probe {
    pub size: u64,
    pub duration: f64,
}

/// Verifies that ffmpeg and ffprobe are present, returning their first version
/// lines. Failure is always `MediaError::Tools` so callers can report it.
pub fn check_tools() -> Result<Tools, MediaError> {
    let ffmpeg_version = tool_version("ffmpeg")?;
    let ffprobe_version = tool_version("ffprobe")?;
    Ok(Tools {
        ffmpeg_version,
        ffprobe_version,
    })
}

fn tool_version(program: &str) -> Result<String, MediaError> {
    let output = tool_command(program)
        .arg("-version")
        .output()
        .map_err(|error| MediaError::Tools(format!("{program}: {error}")))?;
    if !output.status.success() {
        return Err(MediaError::Tools(format!(
            "{program} -version failed ({})",
            output.status
        )));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .next()
        .map(str::to_string)
        .filter(|line| !line.is_empty())
        .ok_or_else(|| MediaError::Tools(format!("{program} -version printed no version")))
}

pub(crate) fn tool_command(program: &str) -> Command {
    let mut command = Command::new(program);
    for (key, value) in FIXED_ENV {
        command.env(key, value);
    }
    command
        .env_remove("FFREPORT")
        .env_remove("FFMPEG_DATADIR")
        .stdin(Stdio::null());
    command
}

fn tool_failure(path: &Path, program: &str, output: std::process::Output) -> MediaError {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr.lines().take(20).collect::<Vec<_>>().join("\n");
    MediaError::Convert {
        path: path.to_path_buf(),
        detail: format!("{program} exited with {}: {detail}", output.status),
    }
}

pub(crate) fn probe(path: &Path) -> Result<Probe, MediaError> {
    let output = tool_command("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=size,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|error| MediaError::Convert {
            path: path.to_path_buf(),
            detail: format!("could not launch ffprobe: {error}"),
        })?;
    if !output.status.success() {
        return Err(tool_failure(path, "ffprobe", output));
    }
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|error| MediaError::Convert {
            path: path.to_path_buf(),
            detail: format!("ffprobe JSON: {error}"),
        })?;
    let format = value.get("format").ok_or_else(|| MediaError::Convert {
        path: path.to_path_buf(),
        detail: "ffprobe JSON has no format section".to_string(),
    })?;
    let size = json_number(format.get("size")).ok_or_else(|| MediaError::Convert {
        path: path.to_path_buf(),
        detail: "ffprobe format has no size".to_string(),
    })?;
    let duration = json_number(format.get("duration")).ok_or_else(|| MediaError::Convert {
        path: path.to_path_buf(),
        detail: "ffprobe format has no duration".to_string(),
    })?;
    Ok(Probe {
        size: size as u64,
        duration,
    })
}

fn json_number(value: Option<&serde_json::Value>) -> Option<f64> {
    match value? {
        serde_json::Value::Number(number) => number.as_f64(),
        serde_json::Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

pub(crate) fn effective_kbps(probe: Probe) -> Result<u64, MediaError> {
    if !probe.duration.is_finite() || probe.duration <= 0.0 {
        return Err(MediaError::Verify(
            "cannot compute source bitrate: non-positive duration".to_string(),
        ));
    }
    Ok(((probe.size as f64 * 8.0) / probe.duration / 1000.0).floor() as u64)
}

pub(crate) fn convert_audio(
    source: &Path,
    destination: &Path,
    target_kbps: u64,
) -> Result<(), MediaError> {
    run_ffmpeg(
        source,
        &[
            "-c:a",
            "libopus",
            "-b:a",
            &format!("{target_kbps}k"),
            "-vbr",
            "on",
        ],
        destination,
    )
}

pub(crate) fn convert_image(source: &Path, destination: &Path) -> Result<(), MediaError> {
    run_ffmpeg(
        source,
        &[
            "-c:v",
            "libwebp",
            "-lossless",
            "1",
            "-compression_level",
            "6",
        ],
        destination,
    )
}

pub(crate) fn convert_video(source: &Path, destination: &Path) -> Result<(), MediaError> {
    run_ffmpeg(
        source,
        &[
            "-c:v",
            "libsvtav1",
            "-preset",
            "6",
            "-crf",
            "44",
            "-svtav1-params",
            "tune=0",
            "-vf",
            "scale='min(1280,iw)':-2",
            "-c:a",
            "libopus",
            "-b:a",
            "48k",
            "-f",
            "webm",
        ],
        destination,
    )
}

fn run_ffmpeg(source: &Path, options: &[&str], destination: &Path) -> Result<(), MediaError> {
    let output = tool_command("ffmpeg")
        .args(["-y", "-v", "error", "-nostdin", "-i"])
        .arg(source)
        .args(options)
        .args(["-fflags", "+bitexact"])
        .arg(destination)
        .output()
        .map_err(|error| MediaError::Convert {
            path: source.to_path_buf(),
            detail: format!("could not launch ffmpeg: {error}"),
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(tool_failure(source, "ffmpeg", output))
    }
}
