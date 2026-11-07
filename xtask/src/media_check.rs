//! `cargo xtask media-check` and `cargo xtask pack` — M3 gate wiring.
//!
//! The report block printed by `media-check` is frozen in
//! `docs/M3-CONTRACT.md` §3. Diagnostics go to stderr so the block stays
//! machine-readable.

use std::path::{Path, PathBuf};

use oxide_media::{
    AudioLayer, Disposition, EntryKind, PackReport, Plan, TOTAL_AFTER_LIMIT, check_tools, mib,
    voice_pairings,
};

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
    corpus: bool,
}

pub fn media_check(rest: &[String]) -> bool {
    let options = match parse(rest) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("media-check: {message}");
            return false;
        }
    };

    let plan = match Plan::build(&options.input) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("media-check: {error}");
            return false;
        }
    };
    if options.corpus
        && let Err(error) = check_corpus_inventory(&options.input, &plan)
    {
        eprintln!("media-check: corpus inventory: {error}");
        return false;
    }

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

    let mut ok = report_passes(&report, options.corpus);

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
    if options.corpus {
        eprintln!("pack: --corpus is only supported by media-check");
        return false;
    }
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

fn check_corpus_inventory(root: &Path, plan: &Plan) -> Result<(), String> {
    for required in [
        "songs",
        "preload/images",
        "preload/data",
        "videos/videos",
        "fonts",
    ] {
        if !root.join(required).is_dir() {
            return Err(format!("missing mandatory directory: {required}"));
        }
    }
    let mut counts = std::collections::BTreeMap::new();
    for entry in plan.entries() {
        let category = match (entry.kind, entry.disposition) {
            (
                EntryKind::Audio {
                    layer: AudioLayer::Inst,
                },
                Disposition::ConvertAudio,
            ) => "Inst",
            (
                EntryKind::Audio {
                    layer: AudioLayer::Voices,
                },
                Disposition::ConvertAudio,
            ) => "Voices",
            (
                EntryKind::Audio {
                    layer: AudioLayer::Sfx,
                },
                Disposition::ConvertAudio,
            ) => "SFX",
            (_, Disposition::ConvertImage) => "images",
            (_, Disposition::ConvertVideo) => "runtime videos",
            (_, Disposition::ExcludeVideo) => "excluded videos",
            (_, Disposition::TextOnly) => "text",
            (_, Disposition::CopyFont) => "fonts",
            (_, Disposition::ExcludeHxc) => "HXC",
            (_, Disposition::DropMp3) => "MP3",
            _ => continue,
        };
        *counts.entry(category).or_insert(0usize) += 1;
    }
    for (category, expected) in [
        ("Inst", 62),
        ("Voices", 126),
        ("SFX", 243),
        ("images", 686),
        ("runtime videos", 11),
        ("excluded videos", 3),
        ("text", 747),
        ("fonts", 25),
        ("HXC", 114),
        ("MP3", 413),
    ] {
        let actual = counts.get(category).copied().unwrap_or(0);
        if actual != expected {
            return Err(format!("expected {expected} {category}, found {actual}"));
        }
    }
    let pairings = voice_pairings(plan.entries());
    if let Some(unpaired) = pairings
        .iter()
        .find(|pairing| pairing.instrumental.is_none())
    {
        return Err(format!(
            "unpaired Voices stem: {} (no matching Inst in the same song)",
            unpaired.voices
        ));
    }
    if pairings.len() != 126 {
        return Err(format!(
            "expected 126 Voices/Inst pairs, found {}",
            pairings.len()
        ));
    }
    Ok(())
}

fn report_passes(report: &PackReport, corpus: bool) -> bool {
    let coverage_ok = report.duration_checked == report.audio_converted
        && report.image_converted_checked == report.images_converted
        && report.video_converted_checked == report.video_converted;
    if !coverage_ok {
        eprintln!(
            "media-check: coverage floor failed: checked/converted audio {}/{}, images {}/{}, video {}/{}",
            report.duration_checked,
            report.audio_converted,
            report.image_converted_checked,
            report.images_converted,
            report.video_converted_checked,
            report.video_converted,
        );
    }
    let corpus_coverage_ok = !corpus
        || (report.audio_converted > 0
            && report.images_converted > 0
            && report.video_converted > 0
            && report.align_checked == 126
            && report.visible_pixels_total > 0);
    if !corpus_coverage_ok {
        eprintln!(
            "media-check: corpus coverage requires audio/image/video conversions, 126 stem pairs and visible pixels"
        );
    }
    coverage_ok
        && corpus_coverage_ok
        && report.pak_entries > 0
        && report.files_grew == 0
        && report.align_ok == report.align_checked
        && report.duration_ok == report.duration_checked
        && !report.visible_check_failed
        && report.visible_pixels_identical == report.visible_pixels_total
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
        "hxc excluded: {} files, {:.1} MB",
        report.hxc_excluded,
        mib(report.hxc_bytes)
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
        "visible_pixels_identical: {}/{}   (pixels differing only where alpha==0: {})",
        report.visible_pixels_identical, report.visible_pixels_total, report.alpha_zero_rgb_diff
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
    let mut corpus = false;
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
            "--corpus" => {
                corpus = true;
                index += 1;
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
        corpus,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn covered_report() -> PackReport {
        PackReport {
            tools: oxide_media::Tools {
                ffmpeg_version: String::new(),
                ffprobe_version: String::new(),
            },
            plan_entries: 1,
            pak_entries: 1,
            pak_bytes: 0,
            pak_sha256: String::new(),
            audio_before: 0,
            audio_after: 0,
            audio_converted: 1,
            audio_copied: 0,
            images_before: 0,
            images_after: 0,
            images_converted: 1,
            images_kept: 0,
            video_before: 0,
            video_after: 0,
            video_converted: 1,
            video_copied: 0,
            video_excluded: 0,
            text_before: 0,
            text_after: 0,
            text_files: 0,
            fonts_before: 0,
            fonts_after: 0,
            font_files: 0,
            skipped: 0,
            hxc_excluded: 0,
            hxc_bytes: 0,
            mp3_dropped: 0,
            mp3_bytes: 0,
            files_grew: 0,
            align_checked: 126,
            align_ok: 126,
            duration_checked: 1,
            duration_ok: 1,
            video_ssim_min: 1.0,
            video_converted_checked: 1,
            image_converted_checked: 1,
            visible_pixels_identical: 1,
            visible_pixels_total: 1,
            alpha_zero_rgb_diff: 0,
            visible_check_failed: false,
            total_after: 0,
        }
    }

    #[test]
    fn corpus_requires_positive_coverage_for_each_category() {
        let covered = covered_report();
        assert!(report_passes(&covered, true));
        for category in ["audio", "images", "video", "alignment", "pixels"] {
            let mut report = covered.clone();
            match category {
                "audio" => {
                    report.audio_converted = 0;
                    report.duration_checked = 0;
                    report.duration_ok = 0;
                }
                "images" => {
                    report.images_converted = 0;
                    report.image_converted_checked = 0;
                }
                "video" => {
                    report.video_converted = 0;
                    report.video_converted_checked = 0;
                }
                "alignment" => {
                    report.align_checked = 0;
                    report.align_ok = 0;
                }
                "pixels" => {
                    report.visible_pixels_total = 0;
                    report.visible_pixels_identical = 0;
                }
                _ => unreachable!(),
            }
            assert!(
                report_passes(&report, false),
                "small mods may lack {category}"
            );
            assert!(!report_passes(&report, true), "missing corpus {category}");
        }
    }

    #[test]
    fn every_conversion_requires_its_check() {
        for category in ["audio", "images", "video"] {
            let mut report = covered_report();
            match category {
                "audio" => report.duration_checked = 0,
                "images" => report.image_converted_checked = 0,
                "video" => report.video_converted_checked = 0,
                _ => unreachable!(),
            }
            assert!(!report_passes(&report, false), "unchecked {category}");
        }
        let mut report = covered_report();
        report.pak_entries = 0;
        assert!(!report_passes(&report, false), "empty report must not pass");
    }
}
