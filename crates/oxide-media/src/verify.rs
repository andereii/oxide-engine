//! Headless verification: raw RGBA pixel compares, stem alignment, SSIM.

use std::path::Path;

use crate::MediaError;
use crate::convert::tool_command;

const ANALYSIS_SECONDS: usize = 3;
const SAMPLE_RATE: usize = 48_000;
const DECIMATE: usize = 6;
const COARSE_HALF_WINDOW: usize = 2048;
const COARSE_WINDOW: usize = 4096;
const COARSE_HALF_LAG: i64 = 512;
const REFINE_HALF_WINDOW: i64 = 6144;
const REFINE_HALF_LAG: i64 = 64;
const SILENT_ANCHOR: i32 = 1;
const ONSET_THRESHOLD: i32 = 1024;
const ONSET_WINDOW: usize = 480;

/// Decodes an image to straight RGBA bytes.
pub(crate) fn raw_rgba(path: &Path) -> Result<Vec<u8>, MediaError> {
    let output = tool_command("ffmpeg")
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-f", "rawvideo", "-pix_fmt", "rgba", "-"])
        .output()
        .map_err(|error| MediaError::Convert {
            path: path.to_path_buf(),
            detail: format!("could not launch ffmpeg: {error}"),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MediaError::Verify(format!(
            "decoding {} failed: {stderr}",
            path.display()
        )));
    }
    Ok(output.stdout)
}

/// Outcome of comparing source and dist RGBA buffers for the visible-pixel
/// metric. Only pixels whose *source* alpha is non-zero are visible; those must
/// be byte-identical. Differences under source alpha zero are informational.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PixelCompare {
    pub visible_identical: u64,
    pub visible_total: u64,
    pub alpha_zero_rgb_diff: u64,
    pub failed: bool,
}

pub(crate) fn compare_visible_pixels(source: &[u8], output: &[u8]) -> PixelCompare {
    let mut result = PixelCompare::default();
    if source.len() != output.len() || !source.len().is_multiple_of(4) {
        result.failed = true;
        return result;
    }
    let (source_pixels, _) = source.as_chunks::<4>();
    let (output_pixels, _) = output.as_chunks::<4>();
    for (source_pixel, output_pixel) in source_pixels.iter().zip(output_pixels) {
        if source_pixel[3] > 0 {
            result.visible_total += 1;
            if source_pixel == output_pixel {
                result.visible_identical += 1;
            }
        } else if source_pixel != output_pixel {
            if output_pixel[3] == 0 {
                result.alpha_zero_rgb_diff += 1;
            } else {
                result.failed = true;
            }
        }
    }
    result
}

/// Decodes a media file to mono 48 kHz s16le, optionally capped to `limit`
/// seconds.
fn decode_pcm(path: &Path, limit: Option<u64>) -> Result<Vec<i16>, MediaError> {
    let mut command = tool_command("ffmpeg");
    command.args(["-v", "error", "-nostdin"]);
    if let Some(seconds) = limit {
        command.args(["-t"]).arg(seconds.to_string());
    }
    let output = command
        .arg("-i")
        .arg(path)
        .args([
            "-f",
            "s16le",
            "-acodec",
            "pcm_s16le",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-",
        ])
        .output()
        .map_err(|error| MediaError::Convert {
            path: path.to_path_buf(),
            detail: format!("could not launch ffmpeg: {error}"),
        })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MediaError::Verify(format!(
            "decoding {} failed: {stderr}",
            path.display()
        )));
    }
    let (pairs, _) = output.stdout.as_chunks::<2>();
    let mut samples = Vec::with_capacity(pairs.len());
    for pair in pairs {
        samples.push(i16::from_le_bytes(*pair));
    }
    Ok(samples)
}

/// Delay (in 48 kHz samples) of `destination` relative to `source`, estimated
/// by energy-normalized cross-correlation. A positive result means the
/// destination is delayed by that many samples.
pub(crate) fn estimate_delay(source: &Path, destination: &Path) -> Result<i64, MediaError> {
    let short_source = decode_pcm(source, Some(6))?;
    let short_destination = decode_pcm(destination, Some(6))?;
    if let Some(delay) = try_estimate_delay_from_pcm(&short_source, &short_destination, false)? {
        return Ok(delay);
    }

    // Some Voices stems are silent well past the 6 s probe window (they enter
    // a dozen seconds into the song). Decode those in full and anchor on their
    // first/loudest signal so the correlation still measures a true delay.
    let source_samples = decode_pcm(source, None)?;
    let destination_samples = decode_pcm(destination, None)?;
    try_estimate_delay_from_pcm(&source_samples, &destination_samples, true)?.ok_or_else(|| {
        MediaError::Verify(format!(
            "no usable alignment signal in {}",
            source.display()
        ))
    })
}

#[cfg(test)]
fn estimate_delay_from_pcm(source: &[i16], destination: &[i16]) -> Result<i64, MediaError> {
    try_estimate_delay_from_pcm(source, destination, true)?.ok_or_else(|| {
        MediaError::Verify("no usable alignment signal for the synthetic pair".to_string())
    })
}

fn try_estimate_delay_from_pcm(
    source: &[i16],
    destination: &[i16],
    search_all: bool,
) -> Result<Option<i64>, MediaError> {
    let source8: Vec<i16> = decimate(source);
    let destination8: Vec<i16> = decimate(destination);
    let analysis_len = ANALYSIS_SECONDS * SAMPLE_RATE / DECIMATE;
    let (source8, destination8) = if search_all {
        (&source8[..], &destination8[..])
    } else {
        (
            &source8[..source8.len().min(analysis_len)],
            &destination8[..destination8.len().min(analysis_len)],
        )
    };

    let Some(anchor) = find_anchor(source8, source, search_all) else {
        return Ok(None);
    };
    let coarse = best_coarse_lag(source8, destination8, anchor);
    Ok(Some(best_refined_lag(source, destination, anchor, coarse)))
}

fn find_anchor(source8: &[i16], source48: &[i16], allow_onset: bool) -> Option<usize> {
    if let Some(peak) = peak_index(source8)
        && (source8[peak] as i32).abs() > SILENT_ANCHOR
    {
        return Some(peak);
    }
    if !allow_onset {
        return None;
    }
    let onset = onset_index(source48)?;
    Some((onset / DECIMATE).min(source8.len().saturating_sub(1)))
}

fn decimate(samples: &[i16]) -> Vec<i16> {
    samples
        .as_chunks::<DECIMATE>()
        .0
        .iter()
        .map(|chunk| {
            let sum: i64 = chunk.iter().map(|&sample| i64::from(sample)).sum();
            (sum / DECIMATE as i64) as i16
        })
        .collect()
}

fn peak_index(samples: &[i16]) -> Option<usize> {
    let mut best = None;
    let mut best_abs = -1i32;
    for (index, &sample) in samples.iter().enumerate() {
        let amplitude = (sample as i32).abs();
        if amplitude > best_abs {
            best_abs = amplitude;
            best = Some(index);
        }
    }
    best
}

fn window_bounds(len: usize, anchor: usize, half: usize, width: usize) -> (usize, usize) {
    if len <= width {
        return (0, len);
    }
    let start = (anchor as i64 - half as i64).clamp(0, (len - width) as i64) as usize;
    (start, start + width)
}

fn best_coarse_lag(source: &[i16], destination: &[i16], anchor: usize) -> i64 {
    let (start, end) = window_bounds(source.len(), anchor, COARSE_HALF_WINDOW, COARSE_WINDOW);
    if start >= end {
        return 0;
    }
    let mut best_lag = 0i64;
    let mut best_score = f64::NEG_INFINITY;
    for lag in -COARSE_HALF_LAG..=COARSE_HALF_LAG {
        let destination_start = start as i64 + lag;
        let destination_end = end as i64 + lag;
        if destination_start < 0 || destination_end > destination.len() as i64 {
            continue;
        }
        let score = normalized_correlation(
            &source[start..end],
            &destination[destination_start as usize..destination_end as usize],
        );
        if better(score, lag, best_score, best_lag) {
            best_score = score;
            best_lag = lag;
        }
    }
    best_lag
}

fn best_refined_lag(source: &[i16], destination: &[i16], anchor8: usize, coarse: i64) -> i64 {
    let anchor = anchor8 as i64 * DECIMATE as i64;
    let width = (REFINE_HALF_WINDOW * 2) as usize;
    let (start, end) = window_bounds(
        source.len(),
        anchor.max(0) as usize,
        REFINE_HALF_WINDOW as usize,
        width,
    );
    if start >= end {
        return coarse * DECIMATE as i64;
    }
    let center = coarse * DECIMATE as i64;
    let mut best_lag = center;
    let mut best_score = f64::NEG_INFINITY;
    for lag in center - REFINE_HALF_LAG..=center + REFINE_HALF_LAG {
        let destination_start = start as i64 + lag;
        let destination_end = end as i64 + lag;
        if destination_start < 0 || destination_end > destination.len() as i64 {
            continue;
        }
        let score = normalized_correlation(
            &source[start..end],
            &destination[destination_start as usize..destination_end as usize],
        );
        if better(score, lag, best_score, best_lag) {
            best_score = score;
            best_lag = lag;
        }
    }
    best_lag
}

fn normalized_correlation(left: &[i16], right: &[i16]) -> f64 {
    let mut dot = 0.0;
    let mut left_energy = 0.0;
    let mut right_energy = 0.0;
    for (&left_sample, &right_sample) in left.iter().zip(right.iter()) {
        let left_sample = f64::from(left_sample);
        let right_sample = f64::from(right_sample);
        dot += left_sample * right_sample;
        left_energy += left_sample * left_sample;
        right_energy += right_sample * right_sample;
    }
    if left_energy == 0.0 || right_energy == 0.0 {
        0.0
    } else {
        dot / (left_energy.sqrt() * right_energy.sqrt())
    }
}

fn better(score: f64, lag: i64, best_score: f64, best_lag: i64) -> bool {
    score > best_score
        || (score == best_score
            && (lag.abs() < best_lag.abs() || (lag.abs() == best_lag.abs() && lag < best_lag)))
}

fn onset_index(samples: &[i16]) -> Option<usize> {
    if samples.len() < ONSET_WINDOW {
        return samples
            .iter()
            .any(|&sample| (sample as i32).abs() > ONSET_THRESHOLD)
            .then_some(0);
    }
    for start in 0..=samples.len() - ONSET_WINDOW {
        if samples[start..start + ONSET_WINDOW]
            .iter()
            .any(|&sample| (sample as i32).abs() > ONSET_THRESHOLD)
        {
            return Some(start);
        }
    }
    None
}

/// Parses the `All:` value printed by ffmpeg's ssim filter.
fn parse_ssim(stderr: &str) -> Option<f64> {
    let mut result = None;
    for line in stderr.lines() {
        let Some(index) = line.rfind("All:") else {
            continue;
        };
        let rest = line.get(index + 4..)?;
        let number: String = rest
            .chars()
            .take_while(|character| character.is_ascii_digit() || *character == '.')
            .collect();
        if let Ok(value) = number.parse::<f64>() {
            result = Some(value);
        }
    }
    result
}

/// Minimum SSIM between the (scaled) source video and the converted output.
pub(crate) fn video_ssim(source: &Path, output: &Path) -> Result<f64, MediaError> {
    let command_output = tool_command("ffmpeg")
        .args(["-hide_banner", "-nostdin", "-i"])
        .arg(source)
        .arg("-i")
        .arg(output)
        .args([
            "-filter_complex",
            "[0:v]scale='min(1280,iw)':-2[scaled];[scaled][1:v]ssim",
            "-f",
            "null",
            "-",
        ])
        .output()
        .map_err(|error| MediaError::Convert {
            path: source.to_path_buf(),
            detail: format!("could not launch ffmpeg: {error}"),
        })?;
    if !command_output.status.success() {
        let stderr = String::from_utf8_lossy(&command_output.stderr);
        return Err(MediaError::Verify(format!(
            "ssim failed for {}: {stderr}",
            source.display()
        )));
    }
    let stderr = String::from_utf8_lossy(&command_output.stderr);
    parse_ssim(&stderr).ok_or_else(|| {
        MediaError::Verify(format!("ssim printed no score for {}", source.display()))
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::{
        ONSET_THRESHOLD, ONSET_WINDOW, compare_visible_pixels, estimate_delay,
        estimate_delay_from_pcm, parse_ssim,
    };

    fn write_wav(path: &std::path::Path, samples: &[i16]) {
        let data_len = (samples.len() * 2) as u32;
        let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
        bytes.extend_from_slice(b"RIFF");
        bytes.extend_from_slice(&(36 + data_len).to_le_bytes());
        bytes.extend_from_slice(b"WAVEfmt ");
        bytes.extend_from_slice(&16u32.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&48_000u32.to_le_bytes());
        bytes.extend_from_slice(&96_000u32.to_le_bytes());
        bytes.extend_from_slice(&2u16.to_le_bytes());
        bytes.extend_from_slice(&16u16.to_le_bytes());
        bytes.extend_from_slice(b"data");
        bytes.extend_from_slice(&data_len.to_le_bytes());
        for sample in samples {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        let mut file = std::fs::File::create(path).expect("create wav");
        file.write_all(&bytes).expect("write wav");
    }

    fn synthetic_burst(len: usize) -> Vec<i16> {
        (0..len)
            .map(|index| {
                let time = index as f64 / 48_000.0;
                let envelope = if time < 0.95 {
                    0.0
                } else if time < 0.96 {
                    (time - 0.95) / 0.01
                } else if time < 1.3 {
                    (1.3 - time) / 0.34
                } else {
                    0.0
                };
                (envelope * 12_000.0 * (std::f64::consts::TAU * 440.0 * time).sin()).round() as i16
            })
            .collect()
    }

    fn crescendo(decoration: f64) -> Vec<i16> {
        let len = 3 * 48_000;
        let crossing = 0.5 + 0.8 * 1024.0 / 4000.0;
        (0..len)
            .map(|index| {
                let time = index as f64 / 48_000.0;
                let envelope = if time < 0.5 {
                    0.0
                } else if time < 1.3 {
                    (time - 0.5) / 0.8 * 4000.0
                } else {
                    4000.0
                };
                let bump = 500.0 * (-((time - crossing) / 0.02).powi(2)).exp();
                let signal =
                    (envelope + decoration * bump) * (std::f64::consts::TAU * 440.0 * time).sin();
                signal.round().clamp(-32768.0, 32767.0) as i16
            })
            .collect()
    }

    fn threshold_crossing(samples: &[i16]) -> usize {
        for start in 0..samples.len() {
            let end = (start + ONSET_WINDOW).min(samples.len());
            if samples[start..end]
                .iter()
                .any(|&sample| (sample as i32).abs() > ONSET_THRESHOLD)
            {
                return start;
            }
        }
        samples.len()
    }

    #[test]
    fn identical_buffers_are_identical() {
        let result = compare_visible_pixels(&[1, 2, 3, 4], &[1, 2, 3, 4]);
        assert_eq!(result.visible_total, 1);
        assert_eq!(result.visible_identical, 1);
        assert_eq!(result.alpha_zero_rgb_diff, 0);
        assert!(!result.failed);
    }

    #[test]
    fn hidden_rgb_differences_are_counted_but_not_visible() {
        let result = compare_visible_pixels(&[0, 0, 0, 0], &[255, 128, 64, 0]);
        assert_eq!(result.visible_total, 0);
        assert_eq!(result.visible_identical, 0);
        assert_eq!(result.alpha_zero_rgb_diff, 1);
        assert!(!result.failed);
    }

    #[test]
    fn visible_differences_are_rejected() {
        for output in [[1, 2, 3, 5], [9, 2, 3, 4]] {
            let result = compare_visible_pixels(&[1, 2, 3, 4], &output);
            assert_eq!(result.visible_total, 1);
            assert_eq!(result.visible_identical, 0);
            assert!(!result.failed);
        }
        let appearance = compare_visible_pixels(&[1, 2, 3, 0], &[1, 2, 3, 4]);
        assert!(appearance.failed);
        let size_mismatch = compare_visible_pixels(&[1, 2, 3, 4], &[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(size_mismatch.failed);
    }

    #[test]
    fn ssim_parser_reads_the_last_all_score() {
        let text = "SSIM Y:0.9 All:0.941 (12.3)\nSSIM Y:0.8 All:0.936 (11.1)\n";
        assert_eq!(parse_ssim(text), Some(0.936));
    }

    #[test]
    #[ignore = "requires ffmpeg/ffprobe; executed by milestone acceptance"]
    fn cross_correlation_recovers_synthetic_delay() {
        crate::check_tools().expect("milestone acceptance requires ffmpeg/ffprobe");
        let source = synthetic_burst(2 * 48_000);
        let mut delayed = vec![0i16; source.len()];
        delayed[37..].copy_from_slice(&source[..source.len() - 37]);

        let directory = std::env::temp_dir();
        let source_path = directory.join(format!(
            "oxide-media-delay-source-{}.wav",
            std::process::id()
        ));
        let delayed_path = directory.join(format!(
            "oxide-media-delay-destination-{}.wav",
            std::process::id()
        ));
        write_wav(&source_path, &source);
        write_wav(&delayed_path, &delayed);

        let delay = estimate_delay(&source_path, &delayed_path).expect("delay estimate");
        assert!(
            (delay - 37).abs() <= 2,
            "expected delay 37 +/- 2 samples, got {delay}"
        );
        let _ = std::fs::remove_file(source_path);
        let _ = std::fs::remove_file(delayed_path);
    }

    #[test]
    fn crescendo_threshold_crossing_is_not_a_delay() {
        let source = crescendo(-1.0);
        let destination = crescendo(1.0);
        let source_crossing = threshold_crossing(&source);
        let destination_crossing = threshold_crossing(&destination);
        assert!(
            source_crossing.abs_diff(destination_crossing) >= 2000,
            "the synthetic threshold crossings must differ: {source_crossing} vs {destination_crossing}"
        );
        let delay = estimate_delay_from_pcm(&source, &destination).expect("delay estimate");
        assert!(
            delay.abs() <= 2,
            "correlation must report no drift, got {delay}"
        );
    }
}
