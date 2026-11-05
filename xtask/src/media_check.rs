//! `cargo xtask media-check` and `cargo xtask pack` — M3 gate wiring.
//!
//! The report block printed by `media-check` is frozen in
//! `docs/M3-CONTRACT.md` §3. Diagnostics go to stderr so the block stays
//! machine-readable.

use std::path::{Path, PathBuf};

use oxide_media::{PackReport, TOTAL_AFTER_LIMIT, check_tools, mib};

const DEFAULT_ASSETS: &str = "~/Descargas/fnf/Funkin/assets";
const DEFAULT_DIST: &str = "assets-dist";
const DEFAULT_JOBS: usize = 4;
const SSIM_THRESHOLD: f64 = 0.93;

#[derive(Debug)]
struct Options {
    input: PathBuf,
    output: PathBuf,
    jobs: usize,
    twice: bool,
}

pub fn media_check(rest: &[String]) -> bool {
    let options = match parse(rest) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("media-check: {message}");
            return false;
        }
    };

    println!(
        "media-check: {} -> {}",
        options.input.display(),
        options.output.display()
    );
    let tools = match check_tools() {
        Ok(tools) => tools,
        Err(error) => {
            eprintln!("media-check: {error}");
            return false;
        }
    };
    println!("tools: {} | ffprobe ok", tools.ffmpeg_version);

    let report = match oxide_media::pack_and_verify(&options.input, &options.output, options.jobs) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("media-check: {error}");
            return false;
        }
    };
    print_report(&report);

    let mut ok = report_passes(&report);

    if options.twice
        && let Err(error) = repeat_run(&options, &report)
    {
        eprintln!("media-check: --twice failed: {error}");
        ok = false;
    }
    ok
}

pub fn pack(rest: &[String]) -> bool {
    let options = match parse(rest) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("pack: {message}");
            return false;
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
            true
        }
        Err(error) => {
            eprintln!("pack: {error}");
            false
        }
    }
}

fn report_passes(report: &PackReport) -> bool {
    let audio_unverified = report
        .audio_converted
        .saturating_sub(report.duration_checked);
    let image_unverified = report
        .images_converted
        .saturating_sub(report.image_converted_checked);
    let video_unverified = report
        .video_converted
        .saturating_sub(report.video_converted_checked);
    let coverage_ok = audio_unverified == 0 && image_unverified == 0 && video_unverified == 0;
    if !coverage_ok {
        eprintln!(
            "media-check: coverage floor failed: audio {audio_unverified} unverified, images {image_unverified} unverified, video {video_unverified} unverified"
        );
    }
    coverage_ok
        && report.files_grew == 0
        && report.align_ok == report.align_checked
        && report.duration_ok == report.duration_checked
        && report.image_ok == report.image_checked
        && report.video_ssim_min >= SSIM_THRESHOLD
        && report.total_after <= TOTAL_AFTER_LIMIT
}

fn print_report(report: &PackReport) {
    println!(
        "audio  before: {:.1} MB  after: {:.1} MB  (converted {}, copied {})",
        mib(report.audio_before),
        mib(report.audio_after),
        report.audio_converted,
        report.audio_copied
    );
    println!(
        "images before: {:.1} MB  after: {:.1} MB  (converted {}, kept png {})",
        mib(report.images_before),
        mib(report.images_after),
        report.images_converted,
        report.images_kept
    );
    println!(
        "video  before: {:.1} MB  after: {:.1} MB  (converted {}, excluded {})",
        mib(report.video_before),
        mib(report.video_after),
        report.video_converted,
        report.video_excluded
    );
    println!(
        "text   before: {:.1} MB  after: {:.1} MB  (deflate in pak, {} files)",
        mib(report.text_before),
        mib(report.text_after),
        report.text_files
    );
    println!(
        "fonts  before: {:.1} MB  after: {:.1} MB  ({} files)",
        mib(report.fonts_before),
        mib(report.fonts_after),
        report.font_files
    );
    println!(
        "skipped (non-runtime files): {} (fla/git metadata)",
        report.skipped
    );
    println!(
        "mp3 dropped: {} files, {:.1} MB",
        report.mp3_dropped,
        mib(report.mp3_bytes)
    );
    println!("files_grew: {}", report.files_grew);
    println!("align_ok: {}/{}", report.align_ok, report.align_checked);
    println!(
        "duration_ok: {}/{}",
        report.duration_ok, report.duration_checked
    );
    println!(
        "video_ssim_min: {:.2} (threshold 0.93)",
        report.video_ssim_min
    );
    println!(
        "image_pixels_identical: {}/{}   (pixels differing only where alpha==0: {})",
        report.image_ok, report.image_checked, report.image_alpha_only
    );
    println!("total_after: {:.1} MB (limit 350)", mib(report.total_after));
    let excluded: Vec<String> = oxide_media::EXCLUDED_VIDEOS
        .iter()
        .map(|stem| format!("{stem}.mp4"))
        .collect();
    println!("excluded_videos: {}", excluded.join(", "));
    println!("pak sha256: {}", report.pak_sha256);
}

fn repeat_run(options: &Options, first: &PackReport) -> Result<(), String> {
    let mut repeated = options.output.clone().into_os_string();
    repeated.push(".twice");
    let repeated = PathBuf::from(repeated);
    let report = oxide_media::pack(&options.input, &repeated, options.jobs)
        .map_err(|error| error.to_string())?;
    if report.pak_sha256 != first.pak_sha256 {
        eprintln!(
            "twice: pak sha256 mismatch (first={}, second={})",
            first.pak_sha256, report.pak_sha256
        );
        return Err("pak bytes changed between runs".to_string());
    }
    if !files_equal(
        &options.output.join("assets.pak"),
        &repeated.join("assets.pak"),
    )? {
        return Err("pak bytes changed between runs".to_string());
    }
    println!(
        "twice: pak bytes identical (first={}, second={})",
        first.pak_sha256, report.pak_sha256
    );
    let _ = std::fs::remove_dir_all(&repeated);
    Ok(())
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, String> {
    let left_meta = std::fs::metadata(left).map_err(|error| error.to_string())?;
    let right_meta = std::fs::metadata(right).map_err(|error| error.to_string())?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }
    let left = std::fs::read(left).map_err(|error| error.to_string())?;
    let right = std::fs::read(right).map_err(|error| error.to_string())?;
    Ok(left == right)
}

fn parse(rest: &[String]) -> Result<Options, String> {
    let mut input = None;
    let mut output = None;
    let mut jobs = DEFAULT_JOBS;
    let mut twice = false;
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
            "--twice" => {
                twice = true;
                index += 1;
            }
            other => return Err(format!("unknown option: {other}")),
        }
    }
    let input = input.unwrap_or_else(|| expand_tilde(DEFAULT_ASSETS));
    let output = output.unwrap_or_else(|| repo_root().join(DEFAULT_DIST));
    Ok(Options {
        input,
        output,
        jobs,
        twice,
    })
}

fn repo_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| crate_dir.to_path_buf())
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}
