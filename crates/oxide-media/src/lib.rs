//! Offline media packer for Oxide Engine (M3).
//!
//! This crate is build-time tooling: it classifies the asset tree, converts
//! audio/images/video with external ffmpeg, copies never-grow fallbacks, writes
//! a deterministic `assets.pak` and verifies the result. Nothing here is linked
//! into the game runtime (ADR-0010/ADR-0011).

mod convert;
mod pak;
mod plan;
mod verify;

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

pub use convert::{Tools, check_tools};
pub use pak::DIST_MARKER;
use pak::{PAK_NAME, PakInput, PakMethod};
pub use plan::{VoicePairing, voice_pairings};
use thiserror::Error;

/// Attract trailers that are never packed (exact stems, not globs).
pub const EXCLUDED_VIDEOS: [&str; 3] =
    ["mobileRelease", "riftCollabTrailer", "boyfriendEverywhere"];

/// Byte limit for `total_after` (350 MiB, matching the plan's units).
pub const TOTAL_AFTER_LIMIT: u64 = 350 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum MediaError {
    #[error("ffmpeg/ffprobe is missing or old: {0}")]
    Tools(String),
    #[error("ffmpeg failed on {path:?}: {detail}")]
    Convert { path: PathBuf, detail: String },
    #[error("unexpected asset file {path:?} (extension .{ext})")]
    Unknown { path: PathBuf, ext: String },
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip: {0}")]
    Zip(String),
    #[error("verification failed: {0}")]
    Verify(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Audio {
        layer: AudioLayer,
    },
    Image,
    Video,
    Text,
    Font,
    /// Contract §1 has `Disposition::Skip` but no matching `EntryKind`; skip
    /// entries still carry their disposition in the plan.
    Skip,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioLayer {
    Inst,
    Voices,
    Sfx,
}

impl AudioLayer {
    pub(crate) fn target_kbps(self) -> u64 {
        match self {
            AudioLayer::Inst => 112,
            AudioLayer::Voices => 64,
            AudioLayer::Sfx => 48,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    ConvertAudio,
    ConvertImage,
    ConvertVideo,
    CopyAudio,
    CopyImage,
    CopyFont,
    DropMp3,
    ExcludeVideo,
    ExcludeHxc,
    TextOnly,
    Skip,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanEntry {
    pub rel: String,
    pub kind: EntryKind,
    pub disposition: Disposition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    entries: Vec<PlanEntry>,
}

impl Plan {
    pub fn entries(&self) -> &[PlanEntry] {
        &self.entries
    }
}

/// Counts and byte totals for one pack run. `pack_and_verify` additionally
/// fills the verification counters.
#[derive(Debug, Clone)]
pub struct PackReport {
    pub tools: Tools,
    pub plan_entries: u64,
    pub pak_entries: u64,
    pub pak_bytes: u64,
    pub pak_sha256: String,
    pub audio_before: u64,
    pub audio_after: u64,
    pub audio_converted: u64,
    pub audio_copied: u64,
    pub images_before: u64,
    pub images_after: u64,
    pub images_converted: u64,
    pub images_kept: u64,
    pub video_before: u64,
    pub video_after: u64,
    pub video_converted: u64,
    pub video_copied: u64,
    pub video_excluded: u64,
    pub text_before: u64,
    pub text_after: u64,
    pub text_files: u64,
    pub fonts_before: u64,
    pub fonts_after: u64,
    pub font_files: u64,
    pub skipped: u64,
    pub hxc_excluded: u64,
    pub hxc_bytes: u64,
    pub mp3_dropped: u64,
    pub mp3_bytes: u64,
    pub files_grew: u64,
    pub align_checked: u64,
    pub align_ok: u64,
    pub duration_checked: u64,
    pub duration_ok: u64,
    pub video_ssim_min: f64,
    pub video_converted_checked: u64,
    pub image_converted_checked: u64,
    pub visible_pixels_identical: u64,
    pub visible_pixels_total: u64,
    pub alpha_zero_rgb_diff: u64,
    pub visible_check_failed: bool,
    pub total_after: u64,
}

#[derive(Debug, Clone)]
struct Artifact {
    source_rel: String,
    key_rel: String,
    kind: EntryKind,
    disposition: Disposition,
    source_len: u64,
    output_len: u64,
    copied: bool,
    duration_delta: Option<f64>,
}

struct Packed {
    report: PackReport,
    plan: Plan,
    artifacts: BTreeMap<String, Artifact>,
}

/// Packs `in_dir` into `out_dir` without running verification. Used by
/// `xtask pack`; the M3 gate uses [`pack_and_verify`].
pub fn pack(in_dir: &Path, out_dir: &Path, jobs: usize) -> Result<PackReport, MediaError> {
    Ok(pack_internal(in_dir, out_dir, jobs)?.report)
}

/// Full M3 gate: pack, then verify pixels, alignment, durations and SSIM.
pub fn pack_and_verify(
    in_dir: &Path,
    out_dir: &Path,
    jobs: usize,
) -> Result<PackReport, MediaError> {
    let mut packed = pack_internal(in_dir, out_dir, jobs)?;
    let report = &mut packed.report;

    verify_durations(&mut packed.artifacts, report);
    verify_images(&packed.plan, &packed.artifacts, in_dir, out_dir, report)?;
    verify_alignment(&packed.plan, &packed.artifacts, in_dir, out_dir, report)?;
    verify_videos(&packed.artifacts, in_dir, out_dir, report)?;

    Ok(packed.report)
}

fn pack_internal(in_dir: &Path, out_dir: &Path, jobs: usize) -> Result<Packed, MediaError> {
    let plan = Plan::build(in_dir)?;
    if !plan.entries().iter().any(|entry| {
        matches!(
            entry.disposition,
            Disposition::ConvertAudio
                | Disposition::ConvertImage
                | Disposition::ConvertVideo
                | Disposition::CopyFont
                | Disposition::TextOnly
        )
    }) {
        return Err(MediaError::Verify(
            "source corpus has no runtime assets".into(),
        ));
    }
    preflight_output_collisions(&plan)?;
    let tools = check_tools()?;
    prepare_output(in_dir, out_dir)?;

    let mut audio_before = 0u64;
    let mut images_before = 0u64;
    let mut video_before = 0u64;
    let mut text_before = 0u64;
    let mut fonts_before = 0u64;
    let mut skipped = 0u64;
    let mut hxc_excluded = 0u64;
    let mut hxc_bytes = 0u64;
    let mut mp3_dropped = 0u64;
    let mut mp3_bytes = 0u64;
    let mut video_excluded = 0u64;

    let mut tasks = Vec::new();
    for entry in plan.entries() {
        match entry.kind {
            EntryKind::Audio { .. }
                if matches!(
                    entry.disposition,
                    Disposition::ConvertAudio | Disposition::CopyAudio
                ) =>
            {
                audio_before += file_len(in_dir, &entry.rel)?;
            }
            EntryKind::Image => images_before += file_len(in_dir, &entry.rel)?,
            EntryKind::Video => video_before += file_len(in_dir, &entry.rel)?,
            EntryKind::Text => {
                if entry.disposition == Disposition::TextOnly {
                    text_before += file_len(in_dir, &entry.rel)?;
                }
            }
            EntryKind::Font => fonts_before += file_len(in_dir, &entry.rel)?,
            EntryKind::Audio { .. } | EntryKind::Skip => {}
        }
        match entry.disposition {
            Disposition::ConvertAudio
            | Disposition::ConvertImage
            | Disposition::ConvertVideo
            | Disposition::CopyFont => tasks.push(entry.clone()),
            Disposition::ExcludeVideo => video_excluded += 1,
            Disposition::DropMp3 => {
                mp3_dropped += 1;
                mp3_bytes += file_len(in_dir, &entry.rel)?;
            }
            Disposition::ExcludeHxc => {
                hxc_excluded += 1;
                hxc_bytes += file_len(in_dir, &entry.rel)?;
            }
            Disposition::Skip => skipped += 1,
            Disposition::CopyAudio | Disposition::CopyImage | Disposition::TextOnly => {}
        }
    }

    let artifacts = run_conversions(&tasks, in_dir, out_dir, jobs.max(1))?;
    let mut audio_after = 0u64;
    let mut audio_converted = 0u64;
    let mut audio_copied = 0u64;
    let mut images_after = 0u64;
    let mut images_converted = 0u64;
    let mut images_kept = 0u64;
    let mut video_after = 0u64;
    let mut video_converted = 0u64;
    let mut video_copied = 0u64;
    let mut fonts_after = 0u64;
    let mut font_files = 0u64;
    let mut files_grew = 0u64;

    for artifact in artifacts.values() {
        if artifact.output_len > artifact.source_len {
            files_grew += 1;
        }
        match artifact.kind {
            EntryKind::Audio { .. } => {
                audio_after += artifact.output_len;
                if artifact.disposition == Disposition::CopyAudio {
                    audio_copied += 1;
                } else {
                    audio_converted += 1;
                }
            }
            EntryKind::Image => {
                images_after += artifact.output_len;
                if artifact.disposition == Disposition::CopyImage {
                    images_kept += 1;
                } else {
                    images_converted += 1;
                }
            }
            EntryKind::Video => {
                video_after += artifact.output_len;
                if artifact.copied {
                    video_copied += 1;
                } else {
                    video_converted += 1;
                }
            }
            EntryKind::Font => {
                fonts_after += artifact.output_len;
                font_files += 1;
            }
            EntryKind::Text | EntryKind::Skip => {}
        }
    }

    let mut pak_inputs = Vec::new();
    let mut text_files = 0u64;
    for entry in plan.entries() {
        if entry.disposition == Disposition::TextOnly {
            text_files += 1;
            pak_inputs.push(PakInput {
                name: entry.rel.clone(),
                source: in_dir.join(&entry.rel),
                method: PakMethod::Deflated,
            });
        }
    }
    for artifact in artifacts.values() {
        pak_inputs.push(PakInput {
            name: artifact.key_rel.clone(),
            source: out_dir.join(&artifact.key_rel),
            method: PakMethod::Stored,
        });
    }

    let pak_path = out_dir.join(PAK_NAME);
    let (pak, pak_sha256) = pak::write_pak(&pak_path, pak_inputs)?;
    let text_after = pak.bytes.saturating_sub(pak.stored_bytes);

    let dist_bytes = tree_bytes_excluding(out_dir, &pak_path)?;
    let total_after = dist_bytes + pak.bytes - pak.stored_bytes;
    let mut owned: Vec<String> = artifacts
        .values()
        .map(|artifact| artifact.key_rel.clone())
        .collect();
    owned.push(PAK_NAME.to_string());
    pak::write_marker(out_dir, &owned, &pak_sha256)?;

    let report = PackReport {
        tools,
        plan_entries: plan.entries().len() as u64,
        pak_entries: pak.entries,
        pak_bytes: pak.bytes,
        pak_sha256,
        audio_before,
        audio_after,
        audio_converted,
        audio_copied,
        images_before,
        images_after,
        images_converted,
        images_kept,
        video_before,
        video_after,
        video_converted,
        video_copied,
        video_excluded,
        text_before,
        text_after,
        text_files,
        fonts_before,
        fonts_after,
        font_files,
        skipped,
        hxc_excluded,
        hxc_bytes,
        mp3_dropped,
        mp3_bytes,
        files_grew,
        align_checked: 0,
        align_ok: 0,
        duration_checked: 0,
        duration_ok: 0,
        video_ssim_min: 1.0,
        video_converted_checked: 0,
        image_converted_checked: 0,
        visible_pixels_identical: 0,
        visible_pixels_total: 0,
        alpha_zero_rgb_diff: 0,
        visible_check_failed: false,
        total_after,
    };

    Ok(Packed {
        report,
        plan,
        artifacts,
    })
}

fn prepare_output(in_dir: &Path, out_dir: &Path) -> Result<(), MediaError> {
    let input = std::fs::canonicalize(in_dir)?;
    let output = if out_dir.exists() {
        std::fs::canonicalize(out_dir)?
    } else {
        std::path::absolute(out_dir)?
    };
    if output == input || output.starts_with(&input) || input.starts_with(&output) {
        return Err(MediaError::Verify(format!(
            "output directory must not overlap the assets root: {} / {}",
            in_dir.display(),
            out_dir.display()
        )));
    }
    if out_dir.exists() {
        let refusal = || {
            MediaError::Verify(format!(
                "refusing to wipe {}: not an assets-dist output (delete it or point --out elsewhere)",
                out_dir.display()
            ))
        };
        if !out_dir.is_dir() {
            return Err(refusal());
        }
        let marker = out_dir.join(DIST_MARKER);
        if marker.is_file() {
            let owned = pak::read_marker_owned(out_dir).ok_or_else(refusal)?;
            pak::remove_owned(out_dir, &owned)?;
        } else {
            let mut entries = std::fs::read_dir(out_dir)?;
            if entries.next().transpose()?.is_some() {
                return Err(refusal());
            }
        }
    }
    std::fs::create_dir_all(out_dir)?;
    Ok(())
}

fn run_conversions(
    tasks: &[PlanEntry],
    in_dir: &Path,
    out_dir: &Path,
    jobs: usize,
) -> Result<BTreeMap<String, Artifact>, MediaError> {
    if tasks.is_empty() {
        return Ok(BTreeMap::new());
    }
    let queue = AtomicUsize::new(0);
    let (sender, receiver) = mpsc::channel();
    let workers = jobs.min(tasks.len());

    std::thread::scope(|scope| {
        for _ in 0..workers {
            let sender = sender.clone();
            let queue = &queue;
            scope.spawn(move || {
                loop {
                    let index = queue.fetch_add(1, Ordering::Relaxed);
                    let Some(entry) = tasks.get(index) else {
                        break;
                    };
                    let result = convert_entry(entry, in_dir, out_dir);
                    if sender.send((index, result)).is_err() {
                        break;
                    }
                }
            });
        }
    });
    drop(sender);

    let mut slots: Vec<Option<Result<Artifact, MediaError>>> =
        (0..tasks.len()).map(|_| None).collect();
    for (index, result) in receiver {
        if let Some(slot) = slots.get_mut(index) {
            *slot = Some(result);
        }
    }

    let mut artifacts = BTreeMap::new();
    for (index, slot) in slots.into_iter().enumerate() {
        let result =
            slot.ok_or_else(|| MediaError::Verify(format!("conversion worker lost task {index}")))?;
        let artifact = result?;
        artifacts.insert(artifact.source_rel.clone(), artifact);
    }
    Ok(artifacts)
}

fn convert_entry(entry: &PlanEntry, in_dir: &Path, out_dir: &Path) -> Result<Artifact, MediaError> {
    let source = in_dir.join(&entry.rel);
    let source_len = std::fs::metadata(&source)?.len();

    match (entry.kind, entry.disposition) {
        (EntryKind::Audio { layer }, Disposition::ConvertAudio) => {
            let target = layer.target_kbps();
            let probe = convert::probe(&source)?;
            let kbps = convert::effective_kbps(probe).map_err(|error| MediaError::Convert {
                path: source.clone(),
                detail: error.to_string(),
            })?;
            if kbps <= target {
                let key_rel = entry.rel.clone();
                copy_verified(&source, &out_dir.join(&key_rel))?;
                return Ok(Artifact {
                    source_rel: entry.rel.clone(),
                    key_rel,
                    kind: entry.kind,
                    disposition: Disposition::CopyAudio,
                    source_len,
                    output_len: source_len,
                    copied: true,
                    duration_delta: None,
                });
            }
            let key_rel = with_extension(&entry.rel, "opus");
            let destination = out_dir.join(&key_rel);
            create_parent(&destination)?;
            convert::convert_audio(&source, &destination, target)?;
            let output_len = std::fs::metadata(&destination)?.len();
            if output_len >= source_len {
                let _ = std::fs::remove_file(&destination);
                let key_rel = entry.rel.clone();
                copy_verified(&source, &out_dir.join(&key_rel))?;
                return Ok(Artifact {
                    source_rel: entry.rel.clone(),
                    key_rel,
                    kind: entry.kind,
                    disposition: Disposition::CopyAudio,
                    source_len,
                    output_len: source_len,
                    copied: true,
                    duration_delta: None,
                });
            }
            let delta = (convert::probe(&destination)?.duration - probe.duration).abs();
            Ok(Artifact {
                source_rel: entry.rel.clone(),
                key_rel,
                kind: entry.kind,
                disposition: Disposition::ConvertAudio,
                source_len,
                output_len,
                copied: false,
                duration_delta: Some(delta),
            })
        }
        (EntryKind::Image, Disposition::ConvertImage) => {
            let key_rel = with_extension(&entry.rel, "webp");
            let destination = out_dir.join(&key_rel);
            create_parent(&destination)?;
            convert::convert_image(&source, &destination)?;
            let output_len = std::fs::metadata(&destination)?.len();
            if output_len >= source_len {
                let _ = std::fs::remove_file(&destination);
                let key_rel = entry.rel.clone();
                copy_verified(&source, &out_dir.join(&key_rel))?;
                return Ok(Artifact {
                    source_rel: entry.rel.clone(),
                    key_rel,
                    kind: entry.kind,
                    disposition: Disposition::CopyImage,
                    source_len,
                    output_len: source_len,
                    copied: true,
                    duration_delta: None,
                });
            }
            Ok(Artifact {
                source_rel: entry.rel.clone(),
                key_rel,
                kind: entry.kind,
                disposition: Disposition::ConvertImage,
                source_len,
                output_len,
                copied: false,
                duration_delta: None,
            })
        }
        (EntryKind::Video, Disposition::ConvertVideo) => {
            let key_rel = with_extension(&entry.rel, "webm");
            let destination = out_dir.join(&key_rel);
            create_parent(&destination)?;
            convert::convert_video(&source, &destination)?;
            let output_len = std::fs::metadata(&destination)?.len();
            if output_len >= source_len {
                let _ = std::fs::remove_file(&destination);
                let key_rel = entry.rel.clone();
                copy_verified(&source, &out_dir.join(&key_rel))?;
                return Ok(Artifact {
                    source_rel: entry.rel.clone(),
                    key_rel,
                    kind: entry.kind,
                    disposition: Disposition::ConvertVideo,
                    source_len,
                    output_len: source_len,
                    copied: true,
                    duration_delta: None,
                });
            }
            Ok(Artifact {
                source_rel: entry.rel.clone(),
                key_rel,
                kind: entry.kind,
                disposition: Disposition::ConvertVideo,
                source_len,
                output_len,
                copied: false,
                duration_delta: None,
            })
        }
        (EntryKind::Font, Disposition::CopyFont) => {
            let key_rel = entry.rel.clone();
            copy_verified(&source, &out_dir.join(&key_rel))?;
            Ok(Artifact {
                source_rel: entry.rel.clone(),
                key_rel,
                kind: entry.kind,
                disposition: Disposition::CopyFont,
                source_len,
                output_len: source_len,
                copied: true,
                duration_delta: None,
            })
        }
        _ => Err(MediaError::Verify(format!(
            "internal: entry queued without a conversion: {}",
            entry.rel
        ))),
    }
}

fn preflight_output_collisions(plan: &Plan) -> Result<(), MediaError> {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for entry in plan.entries() {
        let Some(key) = derived_key(entry) else {
            continue;
        };
        if let Some(previous) = seen.insert(key.clone(), entry.rel.clone()) {
            return Err(MediaError::Verify(format!(
                "derived output collision: {previous} and {} both map to {key}",
                entry.rel
            )));
        }
    }
    Ok(())
}

fn derived_key(entry: &PlanEntry) -> Option<String> {
    match entry.disposition {
        Disposition::ConvertAudio => Some(with_extension(&entry.rel, "opus")),
        Disposition::ConvertImage => Some(with_extension(&entry.rel, "webp")),
        Disposition::ConvertVideo => Some(with_extension(&entry.rel, "webm")),
        Disposition::CopyAudio | Disposition::CopyImage | Disposition::CopyFont => {
            Some(entry.rel.clone())
        }
        Disposition::DropMp3
        | Disposition::ExcludeVideo
        | Disposition::ExcludeHxc
        | Disposition::TextOnly
        | Disposition::Skip => None,
    }
}

fn with_extension(rel: &str, extension: &str) -> String {
    match rel.rsplit_once('/') {
        Some((directory, name)) => {
            let stem = name.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(name);
            format!("{directory}/{stem}.{extension}")
        }
        None => {
            let stem = rel.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(rel);
            format!("{stem}.{extension}")
        }
    }
}

fn create_parent(path: &Path) -> Result<(), MediaError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn copy_verified(source: &Path, destination: &Path) -> Result<(), MediaError> {
    create_parent(destination)?;
    std::fs::copy(source, destination)?;
    if files_equal(source, destination)? {
        Ok(())
    } else {
        Err(MediaError::Verify(format!(
            "copy mismatch: {} -> {}",
            source.display(),
            destination.display()
        )))
    }
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, MediaError> {
    let left_len = std::fs::metadata(left)?.len();
    let right_len = std::fs::metadata(right)?.len();
    if left_len != right_len {
        return Ok(false);
    }
    let mut left_file = File::open(left)?;
    let mut right_file = File::open(right)?;
    let mut left_buffer = [0u8; 1 << 16];
    let mut right_buffer = [0u8; 1 << 16];
    loop {
        let left_read = read_chunk(&mut left_file, &mut left_buffer)?;
        let right_read = read_chunk(&mut right_file, &mut right_buffer)?;
        if left_read != right_read {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
        if left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
    }
}

fn read_chunk(file: &mut File, buffer: &mut [u8]) -> Result<usize, MediaError> {
    let mut filled = 0;
    while filled < buffer.len() {
        let read = file.read(&mut buffer[filled..])?;
        if read == 0 {
            break;
        }
        filled += read;
    }
    Ok(filled)
}

fn file_len(root: &Path, rel: &str) -> Result<u64, MediaError> {
    Ok(std::fs::metadata(root.join(rel))?.len())
}

fn tree_bytes_excluding(root: &Path, excluded: &Path) -> Result<u64, MediaError> {
    let marker = root.join(DIST_MARKER);
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() && path != excluded && path != marker {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

fn verify_durations(artifacts: &mut BTreeMap<String, Artifact>, report: &mut PackReport) {
    for artifact in artifacts.values_mut() {
        let Some(delta) = artifact.duration_delta.take() else {
            continue;
        };
        report.duration_checked += 1;
        if delta <= 0.05 {
            report.duration_ok += 1;
        }
    }
}

fn verify_images(
    plan: &Plan,
    artifacts: &BTreeMap<String, Artifact>,
    in_dir: &Path,
    out_dir: &Path,
    report: &mut PackReport,
) -> Result<(), MediaError> {
    for entry in plan.entries() {
        if entry.kind != EntryKind::Image {
            continue;
        }
        let artifact = artifacts
            .get(&entry.rel)
            .ok_or_else(|| MediaError::Verify(format!("no image artifact for {}", entry.rel)))?;
        let source = verify::raw_rgba(&in_dir.join(&entry.rel))?;
        let output = verify::raw_rgba(&out_dir.join(&artifact.key_rel))?;
        if !artifact.copied {
            report.image_converted_checked += 1;
        }
        let comparison = verify::compare_visible_pixels(&source, &output);
        report.visible_pixels_identical += comparison.visible_identical;
        report.visible_pixels_total += comparison.visible_total;
        report.alpha_zero_rgb_diff += comparison.alpha_zero_rgb_diff;
        if comparison.failed {
            report.visible_check_failed = true;
        }
    }
    Ok(())
}

fn verify_alignment(
    plan: &Plan,
    artifacts: &BTreeMap<String, Artifact>,
    in_dir: &Path,
    out_dir: &Path,
    report: &mut PackReport,
) -> Result<(), MediaError> {
    let available = plan
        .entries()
        .iter()
        .filter(|entry| artifacts.contains_key(&entry.rel));
    let mut delays: BTreeMap<String, i64> = BTreeMap::new();
    for pairing in voice_pairings(available) {
        let Some(inst_rel) = pairing.instrumental else {
            continue;
        };
        let delay_inst = stem_delay(
            &mut delays,
            in_dir,
            out_dir,
            inst_rel,
            &artifacts[inst_rel].key_rel,
        )?;
        let delay_voices = stem_delay(
            &mut delays,
            in_dir,
            out_dir,
            pairing.voices,
            &artifacts[pairing.voices].key_rel,
        )?;
        report.align_checked += 1;
        if delay_inst.abs_diff(delay_voices) <= 128 {
            report.align_ok += 1;
        }
    }
    Ok(())
}

fn stem_delay(
    cache: &mut BTreeMap<String, i64>,
    in_dir: &Path,
    out_dir: &Path,
    source_rel: &str,
    artifact_rel: &str,
) -> Result<i64, MediaError> {
    if let Some(delay) = cache.get(source_rel) {
        return Ok(*delay);
    }
    let delay = verify::estimate_delay(&in_dir.join(source_rel), &out_dir.join(artifact_rel))?;
    cache.insert(source_rel.to_string(), delay);
    Ok(delay)
}

fn verify_videos(
    artifacts: &BTreeMap<String, Artifact>,
    in_dir: &Path,
    out_dir: &Path,
    report: &mut PackReport,
) -> Result<(), MediaError> {
    let mut minimum = f64::INFINITY;
    let mut any = false;
    for artifact in artifacts.values() {
        if artifact.kind != EntryKind::Video || artifact.copied {
            continue;
        }
        let score = verify::video_ssim(
            &in_dir.join(&artifact.source_rel),
            &out_dir.join(&artifact.key_rel),
        )?;
        report.video_converted_checked += 1;
        any = true;
        if score < minimum {
            minimum = score;
        }
    }
    report.video_ssim_min = if any { minimum } else { 1.0 };
    Ok(())
}

/// Formats a byte count as MiB with one decimal, matching the plan's units.
pub fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
