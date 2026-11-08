//! `cargo xtask render-diff` — the M5 phase-2b visual-slice engine fixture.
//!
//! Implements the ADR-0020 / `tools/oracle/RENDER-BEHAVIOR.md` §7 gate:
//!
//! 1. The canonical 1 ms input stream is synthesized by the SAME
//!    [`crate::gameplay_sim`] path used by `gameplay_sim_bopeebo` (the stream
//!    recipe is never re-derived here) and replayed through
//!    [`oxide_game::slice`]. The whole trace is replayed twice per difficulty
//!    and both canonical trace sha256 digests must match.
//! 2. The frozen phase-2b [`oxide_game::render::SliceRenderer`] is driven on
//!    the same ticks. A deterministic per-difficulty sample list is computed
//!    from the conductor beat grid and the canonical trace's event kinds;
//!    each sample position renders one frame, writes a PNG and a per-frame
//!    sha256 under `target/render-diff/<difficulty>/`, and the whole render
//!    pipeline runs twice. Per-frame digests and the whole-run digest must be
//!    byte-identical across the two runs.
//! 3. When `tests/fixtures/render-slice/_manifest.json` exists, the frozen
//!    schema (`render-slice-1`), adapter token (`llvmpipe`, software-pinned by
//!    [`SliceRenderer::build`]), sample lists, per-frame digests, whole-run
//!    digests and structural geometry constants are verified fail-closed. A
//!    missing or drifted fixture makes the gate exit nonzero; it never writes
//!    under `tests/` and never regenerates expectations.
//!
//! Sample positions are frozen by the first establishment run (owned by the
//! orchestrator, like ADR-0019), not by this gate:
//!
//! - eight conductor beats at/after the 1000 ms receptor-fade completion
//!   (`RENDER-BEHAVIOR.md` §3 engine decision) and `song_end - 200 ms`;
//! - the first canonical trace event of every distinct scripted-phase class
//!   (`hit/sick|good|bad|shit`, `hit/miss` = the 161 ms boundary hit, window
//!   `miss`, `hold_*`, `ghost_press`, plus the exact 160/161 ms boundary
//!   presses), each with one follow-up at `+250 ms` clamped to song bounds;
//! - the first tick past the last note end.
//!
//! The list is sorted and deduplicated. `song_end` is the grid-rounded last
//! player-note end (`ceil(t + l)`) that the canonical stream covers; the
//! canonical replay then continues for its frozen 2 s tail.
//!
//! Geometry is recomputed here from the frozen spec numbers and the real
//! assets because `oxide-game::render`'s geometry module is private. This is
//! an independent, GLSL-free assertion (not a renderer-output introspection):
//! receptor positions, lane reference line, note top-left crossing y, stage
//! prop world rects and the initial camera scroll. Stage/atlas path candidate
//! order mirrors the renderer's frozen asset-key resolution so the same files
//! are inspected.
//!
//! Honesty: frame digests are OUR renderer's output frozen once (an engine
//! fixture). They are NOT native pixel parity; a native-game rendered-output
//! trace remains a separate pending obligation. The software raster adapter
//! (`llvmpipe`) is pinned by the renderer; GPU-driver and Windows/platform
//! behavior is a separate pending claim. `oxide-game::render` currently exposes
//! no adapter accessor, so the actual adapter identity is echoed by the
//! harness on stderr and this gate prints the pinned policy.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oxide_atlas::AnimateStage;
use oxide_audio::conductor::{Conductor, TimeChange};
use oxide_data::Typed;
use oxide_game::render::SliceRenderer;
use oxide_game::slice::TraceEvent;
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::gameplay_sim::{self, CanonicalReplay, DIFFICULTIES};
use crate::replay::LaneNote;

const SCHEMA: &str = "render-slice-1";
/// Fixture digests are adapter-specific (ADR-0020 addendum 2026-11-07).
/// `SliceRenderer::build` is the software-pinned entry point, so the manifest
/// records this stable policy token; the harness echoes the full
/// `RenderAdapterInfo` on stderr and fails closed if it is not llvmpipe.
const ADAPTER: &str = "llvmpipe";
const FIXTURE_DIR: &str = "tests/fixtures/render-slice";
const MANIFEST_FILE: &str = "_manifest.json";
const TARGET_DIR: &str = "target/render-diff";
const HONESTY: &str = "engine fixture; software adapter pinned; adapter-specific fixture digests \
                       (adapter recorded in the manifest); NOT native pixel parity; native \
                       rendered-output trace remains a separate pending obligation";
/// RENDER-BEHAVIOR §3 engine decision: sampled frames start after the receptor
/// fade completes.
const FADE_COMPLETE_MS: f64 = 1000.0;
const BEAT_SAMPLES: usize = 8;
const FOLLOW_UP_MS: u64 = 250;
const SONG_END_MARGIN_MS: u64 = 200;
const RENDER_WIDTH: usize = 1280;
const RENDER_HEIGHT: usize = 720;
const FRAME_BYTES: usize = RENDER_WIDTH * RENDER_HEIGHT * 4;

// RENDER-BEHAVIOR §2/§3 constants used by the independent geometry check.
const GAME_WIDTH: f64 = 1280.0;
const GAME_HEIGHT: f64 = 720.0;
const STRUMLINE_SIZE: f64 = 104.0;
const NOTE_SPACING: f64 = 112.0;
const INITIAL_OFFSET: f64 = -0.275 * STRUMLINE_SIZE;
const STRUMLINE_Y: f64 = 24.0;
const PLAYER_STRUMLINE_X: f64 = 688.0;
const OPPONENT_STRUMLINE_X: f64 = 48.0;
const PIXELS_PER_MS: f64 = 0.45;
const EASY_SCROLL_SPEED: f64 = 1.2;
/// Nonzero strum offsets used to pin the note TOP-LEFT y formula
/// (RENDER-BEHAVIOR §3); see [`note_top_left_y_at_offsets`].
const NOTE_TOP_LEFT_OFFSETS_MS: [u64; 2] = [1040, 2080];
const GEOMETRY_ABS_TOLERANCE: f64 = 1e-9;

const METADATA_PATH: &str = "preload/data/songs/bopeebo/bopeebo-metadata.json";

/// Typed failure for the fixture-boundary case called out by the gate brief.
#[derive(Debug, thiserror::Error)]
enum RenderDiffError {
    #[error(
        "fixtures not established: {path} is missing; the gate never creates or regenerates \
         fixtures. The computed sample list and frame digests above must be frozen through the \
         reviewed establishment step (target/render-diff/ is not committed evidence)"
    )]
    FixturesNotEstablished { path: String },
    #[error("{0}")]
    Failure(String),
}

impl From<String> for RenderDiffError {
    fn from(message: String) -> Self {
        Self::Failure(message)
    }
}

pub fn run(rest: &[String]) -> ExitCode {
    let assets_root = match assets_only_cli(rest, "render-diff") {
        Ok(path) => path,
        Err(message) => {
            eprintln!("render-diff: {message}");
            return ExitCode::from(2);
        }
    };
    if !assets_root.is_dir() {
        eprintln!(
            "render-diff: prerequisite: assets directory is missing or not a directory: {}",
            assets_root.display()
        );
        return ExitCode::FAILURE;
    }
    println!("render-diff: honesty: {HONESTY}");
    println!(
        "render-diff: adapter: {ADAPTER} (software pinned by SliceRenderer::build; the actual \
         RenderAdapterInfo is echoed by the oxide-game harness)"
    );

    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let manifest_path = workspace.join(FIXTURE_DIR).join(MANIFEST_FILE);
    let fixtures_present = manifest_path.is_file();
    if !fixtures_present {
        println!(
            "render-diff: fixtures not established: {} is missing; the gate will print the \
             computed sample lists and frame digests, then fail (nothing is written under tests/)",
            manifest_path.display()
        );
    }

    let geometry = match compute_geometry(&assets_root) {
        Ok(geometry) => Some(geometry),
        Err(error) => {
            eprintln!("render-diff: geometry check: {error}");
            None
        }
    };

    let mut evidence: BTreeMap<&'static str, DifficultyEvidence> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    if geometry.is_none() {
        failures
            .push("independent structural geometry could not be computed from the assets".into());
    }
    // Print the recomputed geometry before manifest comparison so drift
    // diagnostics and the establishment log always carry the same values.
    if let Some(geometry) = &geometry {
        println!(
            "render-diff: computed geometry: {}",
            serde_json::to_string(&geometry.to_json())
                .unwrap_or_else(|_| "<unserializable>".to_string())
        );
    }

    for difficulty in DIFFICULTIES {
        let target = workspace.join(TARGET_DIR).join(difficulty);
        match capture_difficulty(&assets_root, difficulty, &target) {
            Ok(captured) => {
                print_evidence(&captured);
                evidence.insert(difficulty, captured);
            }
            Err(error) => {
                eprintln!("render-diff: {difficulty}: {error}");
                failures.push(format!("{difficulty}: {error}"));
            }
        }
    }

    if fixtures_present {
        match load_manifest(&manifest_path) {
            Ok(manifest) => {
                failures.extend(compare_manifest(&manifest, &evidence));
                if let Some(geometry) = &geometry {
                    failures.extend(compare_geometry(geometry, &manifest));
                }
            }
            Err(error) => failures.push(error),
        }
    } else {
        println!(
            "render-diff: manifest must record \"adapter\": {ADAPTER:?} (the software-pinned \
             digests are adapter-specific)"
        );
        // The absent-fixture path is the establishment aid: print everything
        // the orchestrator must freeze, but still fail closed.
        let typed = RenderDiffError::FixturesNotEstablished {
            path: manifest_path.display().to_string(),
        };
        failures.push(typed.to_string());
    }

    if failures.is_empty() {
        println!(
            "render-diff: PASS: two-run slice trace and frame digests are identical and match \
             the frozen render-slice-1 fixture; {HONESTY}"
        );
        ExitCode::SUCCESS
    } else {
        for failure in &failures {
            eprintln!("render-diff: {failure}");
        }
        eprintln!(
            "render-diff: gate failed with {} error(s); fixtures are never auto-repaired",
            failures.len()
        );
        ExitCode::FAILURE
    }
}

/// The shared explicit-assets CLI shape for the two visual-slice gates.
pub(crate) fn assets_only_cli(rest: &[String], gate: &str) -> Result<PathBuf, String> {
    let usage = format!("usage: {gate} --assets <dir>");
    let mut assets = None;
    let mut iter = rest.iter();
    while let Some(flag) = iter.next() {
        match flag.as_str() {
            "--assets" => {
                let Some(value) = iter.next().filter(|value| !value.is_empty()) else {
                    return Err(format!("--assets requires a value; {usage}"));
                };
                assets = Some(PathBuf::from(value));
            }
            other => return Err(format!("unsupported argument {other:?}; {usage}")),
        }
    }
    assets.ok_or_else(|| {
        format!("missing --assets <dir>: real chart assets must be provided explicitly; {usage}")
    })
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

#[derive(Debug, Clone)]
struct FrameDigest {
    sample_ms: u64,
    sha256: String,
}

#[derive(Debug, Clone)]
struct DifficultyEvidence {
    difficulty: &'static str,
    samples: Vec<u64>,
    frames: Vec<FrameDigest>,
    run_sha256: String,
    trace_sha256: String,
}

fn capture_difficulty(
    assets_root: &Path,
    difficulty: &'static str,
    target_dir: &Path,
) -> Result<DifficultyEvidence, String> {
    let notes = gameplay_sim::load_verified_notes(assets_root, difficulty)?;
    // The complete pipeline (sim replay, sample derivation, frame rendering
    // and evidence hashing) runs twice from fresh state. Only the first run
    // writes the never-committed target evidence.
    let first = capture_once(assets_root, difficulty, &notes, Some(target_dir))?;
    let second = capture_once(assets_root, difficulty, &notes, None)?;
    compare_runs(difficulty, &first, &second)?;
    Ok(first)
}

fn capture_once(
    assets_root: &Path,
    difficulty: &'static str,
    notes: &[LaneNote],
    write_dir: Option<&Path>,
) -> Result<DifficultyEvidence, String> {
    let replay = gameplay_sim::canonical_replay(assets_root, difficulty, notes)?;
    let samples = sample_positions(assets_root, notes, &replay)?;
    let last_sample = samples
        .last()
        .copied()
        .ok_or_else(|| format!("{difficulty}: the sample plan is empty"))?;
    let replay_end = replay.ticks.last().map_or(0, |tick| tick.sim_ms);
    if last_sample > replay_end {
        return Err(format!(
            "{difficulty}: sample tick {last_sample} lies after the canonical replay end \
             {replay_end}"
        ));
    }

    let mut renderer = SliceRenderer::build(assets_root, difficulty)
        .map_err(|error| format!("{difficulty}: renderer build failed: {error}"))?;
    let sample_set: BTreeSet<u64> = samples.iter().copied().collect();
    let mut frames = Vec::with_capacity(samples.len());
    let mut frame_metadata = Vec::with_capacity(samples.len());
    let mut run_hasher = Sha256::new();

    if let Some(directory) = write_dir {
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "{difficulty}: cannot create evidence directory {}: {error}",
                directory.display()
            )
        })?;
    }

    for tick in &replay.ticks {
        renderer
            .step(tick.sim_ms, &tick.events, &tick.inputs)
            .map_err(|error| {
                format!(
                    "{difficulty}: renderer rejected canonical tick {}: {error}",
                    tick.sim_ms
                )
            })?;
        if sample_set.contains(&tick.sim_ms) {
            let rgba = renderer.frame().map_err(|error| {
                format!(
                    "{difficulty}: renderer failed at sample {} ms: {error}",
                    tick.sim_ms
                )
            })?;
            if rgba.len() != FRAME_BYTES {
                return Err(format!(
                    "{difficulty}: sample {} produced {} frame bytes, expected {FRAME_BYTES}",
                    tick.sim_ms,
                    rgba.len()
                ));
            }
            let sha256 = sha_hex(&rgba);
            run_hasher.update(&rgba);
            if let Some(directory) = write_dir {
                let png = encode_png_rgba(RENDER_WIDTH as u32, RENDER_HEIGHT as u32, &rgba)?;
                let png_name = format!("frame-{}.png", tick.sim_ms);
                fs::write(directory.join(&png_name), &png).map_err(|error| {
                    format!(
                        "{difficulty}: cannot write frame evidence {}: {error}",
                        directory.join(&png_name).display()
                    )
                })?;
                frame_metadata.push(FrameMetadata {
                    sample_ms: tick.sim_ms,
                    frame_sha256: sha256.clone(),
                    png_sha256: sha_hex(&png),
                    png: png_name,
                });
            }
            frames.push(FrameDigest {
                sample_ms: tick.sim_ms,
                sha256,
            });
        }
        if tick.sim_ms >= last_sample {
            break;
        }
    }

    if frames.len() != samples.len() {
        return Err(format!(
            "{difficulty}: rendered {} sample frame(s) for a {}-position sample plan",
            frames.len(),
            samples.len()
        ));
    }
    if let Some(directory) = write_dir {
        let mut bytes = Vec::new();
        for metadata in &frame_metadata {
            serde_json::to_writer(&mut bytes, metadata).map_err(|error| {
                format!("{difficulty}: cannot serialize frame metadata: {error}")
            })?;
            bytes.push(b'\n');
        }
        fs::write(directory.join("frames.jsonl"), bytes).map_err(|error| {
            format!(
                "{difficulty}: cannot write frame metadata {}: {error}",
                directory.join("frames.jsonl").display()
            )
        })?;
    }

    Ok(DifficultyEvidence {
        difficulty,
        samples,
        frames,
        run_sha256: format!("{:x}", run_hasher.finalize()),
        trace_sha256: replay.digest,
    })
}

fn compare_runs(
    difficulty: &str,
    first: &DifficultyEvidence,
    second: &DifficultyEvidence,
) -> Result<(), String> {
    if first.trace_sha256 != second.trace_sha256 {
        return Err(format!(
            "{difficulty}: two-run slice trace digest mismatch: {} != {}",
            first.trace_sha256, second.trace_sha256
        ));
    }
    if first.samples != second.samples {
        return Err(format!(
            "{difficulty}: two-run sample plan mismatch: {:?} != {:?}",
            first.samples, second.samples
        ));
    }
    if first.run_sha256 != second.run_sha256 {
        return Err(format!(
            "{difficulty}: two-run render digest mismatch: {} != {}",
            first.run_sha256, second.run_sha256
        ));
    }
    if first.frames.len() != second.frames.len() {
        return Err(format!(
            "{difficulty}: two-run frame count mismatch: {} != {}",
            first.frames.len(),
            second.frames.len()
        ));
    }
    for (left, right) in first.frames.iter().zip(&second.frames) {
        if left.sample_ms != right.sample_ms || left.sha256 != right.sha256 {
            return Err(format!(
                "{difficulty}: two-run frame mismatch at sample {} ms: {} != {}",
                left.sample_ms, left.sha256, right.sha256
            ));
        }
    }
    Ok(())
}

fn print_evidence(evidence: &DifficultyEvidence) {
    println!(
        "render-diff: {}: sample list ({}): {:?}",
        evidence.difficulty,
        evidence.samples.len(),
        evidence.samples
    );
    println!(
        "render-diff: {}: slice trace sha256 {}; run sha256 {}; frames {}",
        evidence.difficulty,
        evidence.trace_sha256,
        evidence.run_sha256,
        evidence.frames.len()
    );
    for frame in &evidence.frames {
        println!(
            "render-diff: {}: frame {} sha256 {}",
            evidence.difficulty, frame.sample_ms, frame.sha256
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum EventCategory {
    HitSick,
    HitGood,
    HitBad,
    HitShit,
    HitMiss,
    WindowMiss,
    HoldStart,
    HoldTick,
    HoldDrop,
    HoldEnd,
    GhostPress,
    Boundary160,
    Boundary161,
}

fn hit_categories(judgement: &str, note_diff_int: i32) -> Result<Vec<EventCategory>, String> {
    let mut categories = vec![match judgement {
        "sick" => EventCategory::HitSick,
        "good" => EventCategory::HitGood,
        "bad" => EventCategory::HitBad,
        "shit" => EventCategory::HitShit,
        "miss" => EventCategory::HitMiss,
        other => {
            return Err(format!(
                "canonical trace has an unknown hit judgement {other:?}; the render sample plan \
                 refuses to guess"
            ));
        }
    }];
    if note_diff_int == 160 {
        categories.push(EventCategory::Boundary160);
    }
    if note_diff_int == 161 {
        categories.push(EventCategory::Boundary161);
    }
    Ok(categories)
}

fn event_categories(event: &TraceEvent) -> Result<Vec<EventCategory>, String> {
    match event {
        TraceEvent::Hit {
            judgement,
            note_diff_int,
            ..
        } => hit_categories(judgement, *note_diff_int),
        TraceEvent::Miss { .. } => Ok(vec![EventCategory::WindowMiss]),
        TraceEvent::GhostPress { .. } => Ok(vec![EventCategory::GhostPress]),
        TraceEvent::HoldStart { .. } => Ok(vec![EventCategory::HoldStart]),
        TraceEvent::HoldTick { .. } => Ok(vec![EventCategory::HoldTick]),
        TraceEvent::HoldDrop { .. } => Ok(vec![EventCategory::HoldDrop]),
        TraceEvent::HoldEnd { .. } => Ok(vec![EventCategory::HoldEnd]),
        TraceEvent::GameOver { .. } => Err(
            "the canonical render stream reached game_over; the frozen visual slice expects a \
             live trace"
                .to_string(),
        ),
        // Compile-only arm authorized for the schema-2 variant: the frozen
        // schema-1 canonical render stream never carries opponent rows.
        TraceEvent::OpponentHit { .. } => {
            Err("a schema-2 opponent row reached the schema-1 canonical render stream".to_string())
        }
    }
}

fn trace_sim(event: &TraceEvent) -> u64 {
    match event {
        TraceEvent::Hit { sim, .. }
        | TraceEvent::Miss { sim, .. }
        | TraceEvent::GhostPress { sim, .. }
        | TraceEvent::HoldStart { sim, .. }
        | TraceEvent::HoldTick { sim, .. }
        | TraceEvent::HoldDrop { sim, .. }
        | TraceEvent::HoldEnd { sim, .. }
        | TraceEvent::GameOver { sim, .. }
        | TraceEvent::OpponentHit { sim, .. } => *sim,
    }
}

/// Deterministic sample plan for one difficulty.
pub(crate) fn sample_positions(
    assets_root: &Path,
    notes: &[LaneNote],
    replay: &CanonicalReplay,
) -> Result<Vec<u64>, String> {
    let song_end = song_end_ms(notes)?;
    let beats = beat_times(assets_root, song_end)?;
    let mut categories = Vec::new();
    for tick in &replay.ticks {
        for event in &tick.events {
            for category in event_categories(event)? {
                categories.push((trace_sim(event), category));
            }
        }
    }
    sample_positions_from(&beats, song_end, &categories)
}

fn sample_positions_from(
    beat_times: &[u64],
    song_end: u64,
    event_categories: &[(u64, EventCategory)],
) -> Result<Vec<u64>, String> {
    let mut samples = BTreeSet::new();
    for beat in beat_times {
        if *beat <= song_end {
            samples.insert(*beat);
        }
    }
    samples.insert(song_end.saturating_sub(SONG_END_MARGIN_MS));
    let mut seen = BTreeSet::new();
    for (sim_ms, category) in event_categories {
        if seen.insert(*category) {
            samples.insert(*sim_ms);
            samples.insert(
                sim_ms
                    .checked_add(FOLLOW_UP_MS)
                    .ok_or_else(|| "sample follow-up overflows u64 milliseconds".to_string())?
                    .min(song_end),
            );
        }
    }
    samples.insert(
        song_end
            .checked_add(1)
            .ok_or_else(|| "song end overflows u64 milliseconds".to_string())?,
    );
    Ok(samples.into_iter().collect())
}

fn song_end_ms(notes: &[LaneNote]) -> Result<u64, String> {
    let mut end = 0u64;
    for note in notes {
        let note_end = gameplay_sim::grid_ms(note.time_ms + note.hold_ms)?;
        end = end.max(note_end);
    }
    Ok(end)
}

fn beat_times(assets_root: &Path, song_end: u64) -> Result<Vec<u64>, String> {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&conductor_time_changes(assets_root)?);
    let mut first = None;
    for index in 0..1_000_000i32 {
        let beat_ms = conductor.get_beat_time_in_ms(f64::from(index));
        if !beat_ms.is_finite() {
            return Err(format!("conductor beat {index} is not finite: {beat_ms}"));
        }
        if beat_ms >= FADE_COMPLETE_MS {
            first = Some(index);
            break;
        }
        if beat_ms > song_end as f64 {
            break;
        }
    }
    let Some(first) = first else {
        return Err(format!(
            "no conductor beat at/after the {FADE_COMPLETE_MS} ms fade completion lies within the \
             song end {song_end} ms; the render sample plan refuses to invent one"
        ));
    };
    let mut beats = Vec::new();
    for index in first..first.saturating_add(BEAT_SAMPLES as i32) {
        let beat_ms = conductor.get_beat_time_in_ms(f64::from(index));
        if beat_ms > song_end as f64 {
            break;
        }
        beats.push(gameplay_sim::grid_ms(beat_ms)?);
    }
    Ok(beats)
}

fn conductor_time_changes(assets_root: &Path) -> Result<Vec<TimeChange>, String> {
    let document = read_data_document(assets_root, METADATA_PATH)?;
    let Typed::Metadata(metadata) = document.typed else {
        return Err(format!(
            "{METADATA_PATH} parsed as {:?}; metadata was required",
            document.kind
        ));
    };
    let mut changes = Vec::new();
    if let Some(list) = &metadata.time_changes {
        for change in list {
            if !change.time.is_finite() || !change.bpm.is_finite() || change.bpm <= 0.0 {
                return Err(format!(
                    "metadata time change at {} ms has bpm {}",
                    change.time, change.bpm
                ));
            }
            let time_signature_num = i32::try_from(change.time_signature_num.unwrap_or(4))
                .map_err(|_| "metadata time-signature numerator exceeds i32".to_string())?;
            let time_signature_den = i32::try_from(change.time_signature_den.unwrap_or(4))
                .map_err(|_| "metadata time-signature denominator exceeds i32".to_string())?;
            changes.push(TimeChange {
                time_stamp: change.time,
                beat_time: 0.0,
                bpm: change.bpm,
                time_signature_num,
                time_signature_den,
            });
        }
    }
    Ok(changes)
}

#[derive(Debug, Serialize)]
struct FrameMetadata {
    sample_ms: u64,
    frame_sha256: String,
    png_sha256: String,
    png: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct ReceptorGeometry {
    line: &'static str,
    direction: u8,
    x_bits: u64,
    y_bits: u64,
}

impl ReceptorGeometry {
    fn x(self) -> f64 {
        f64::from_bits(self.x_bits)
    }

    fn y(self) -> f64 {
        f64::from_bits(self.y_bits)
    }
}

/// Independent structural geometry constants recomputed from the frozen
/// RENDER-BEHAVIOR spec numbers (the renderer's geometry module is private).
///
/// `lane_center_y` is the strumline reference line (`STRUMLINE_Y + 104/2 =
/// 76.0`), not a note centre. `note_crossing_top_y` is the note TOP-LEFT
/// y-coordinate at the strum-time crossing: `STRUMLINE_Y - INITIAL_OFFSET +
/// getNoteY(0) = 24 + 28.6 = 52.6` (RENDER-BEHAVIOR §3).
/// `character_bounds` pins the OBSERVED atlas content bounds from the
/// oxide-atlas real-asset tests; the renderer's transform module enforces them
/// at render time, so this part is an engine-decision check rather than an
/// independent recomputation.
#[derive(Debug, Clone)]
struct Geometry {
    receptors: Vec<ReceptorGeometry>,
    lane_center_y: f64,
    note_crossing_top_y: f64,
    note_top_left_y_at_offsets: BTreeMap<String, f64>,
    character_bounds: BTreeMap<String, [f64; 4]>,
    props: BTreeMap<String, [f64; 4]>,
    camera_initial_scroll: [f64; 2],
}

impl Geometry {
    fn to_json(&self) -> Value {
        json!({
            "receptors": self.receptors.iter().map(|receptor| json!({
                "line": receptor.line,
                "direction": receptor.direction,
                "x": receptor.x(),
                "y": receptor.y(),
            })).collect::<Vec<_>>(),
            "lane_center_y": self.lane_center_y,
            "note_crossing_top_y": self.note_crossing_top_y,
            "note_top_left_y_at_offsets": self.note_top_left_y_at_offsets,
            "character_bounds": self.character_bounds.iter().map(|(name, rect)| (
                name.clone(),
                json!(rect),
            )).collect::<serde_json::Map<_, _>>(),
            "props": self.props.iter().map(|(name, rect)| (
                name.clone(),
                json!(rect),
            )).collect::<serde_json::Map<_, _>>(),
            "camera_initial_scroll": self.camera_initial_scroll,
        })
    }
}

fn receptor_positions() -> Vec<ReceptorGeometry> {
    let mut receptors = Vec::with_capacity(8);
    for (line, line_x) in [
        ("player", PLAYER_STRUMLINE_X),
        ("opponent", OPPONENT_STRUMLINE_X),
    ] {
        for direction in 0..4u8 {
            let x = line_x + f64::from(direction) * NOTE_SPACING + INITIAL_OFFSET;
            receptors.push(ReceptorGeometry {
                line,
                direction,
                x_bits: x.to_bits(),
                y_bits: STRUMLINE_Y.to_bits(),
            });
        }
    }
    receptors
}

fn lane_center_y() -> f64 {
    STRUMLINE_Y + STRUMLINE_SIZE / 2.0
}

fn note_crossing_top_y() -> f64 {
    STRUMLINE_Y - INITIAL_OFFSET
}

/// Note TOP-LEFT y for notes whose strum time is `offset_ms` in the future
/// (easy scroll speed 1.2). RENDER-BEHAVIOR §3:
/// `y = STRUMLINE_Y - INITIAL_OFFSET + getNoteY`, with
/// `getNoteY = 0.45 * (song_ms - strum_ms) * scrollSpeed * -1`; for a future
/// note `(song_ms - strum_ms) = -offset_ms`, hence
/// `52.6 + 0.45 * offset_ms * 1.2`. Expected: 1040 -> 614.2, 2080 -> 1175.8.
fn note_top_left_y_at_offsets() -> BTreeMap<String, f64> {
    NOTE_TOP_LEFT_OFFSETS_MS
        .iter()
        .map(|offset_ms| {
            (
                offset_ms.to_string(),
                note_crossing_top_y() + PIXELS_PER_MS * (*offset_ms as f64) * EASY_SCROLL_SPEED,
            )
        })
        .collect()
}

/// OBSERVED per-character content bounds at scale 1 from
/// `crates/oxide-atlas`'s real-asset tests (bf/dad/gf). This is a pinned
/// engine-decision check: the renderer's atlas transform module enforces these
/// bounds at render time; it is not an independent symbol-tree recomputation.
fn character_bounds() -> BTreeMap<String, [f64; 4]> {
    BTreeMap::from([
        ("bf".to_string(), [-214.4, -271.3, 496.9, 513.85]),
        (
            "dad".to_string(),
            [-270.15, -563.7707849999999, 566.785472, 841.334785],
        ),
        ("gf".to_string(), [-194.9, -333.65, 748.8, 674.85]),
    ])
}

fn prop_rect(position: [f64; 2], scale: [f64; 2], image_size: [f64; 2]) -> [f64; 4] {
    [
        position[0],
        position[1],
        image_size[0] * scale[0],
        image_size[1] * scale[1],
    ]
}

fn camera_initial_scroll(
    position: [f64; 2],
    slot_scale: f64,
    character_camera: [f64; 2],
    slot_camera: [f64; 2],
    character_scale: f64,
    bounds_height: f64,
) -> [f64; 2] {
    let scale = character_scale * slot_scale;
    let focus = [
        position[0] + character_camera[0] + slot_camera[0],
        position[1] - bounds_height * scale / 2.0 + character_camera[1] + slot_camera[1],
    ];
    [focus[0] - GAME_WIDTH / 2.0, focus[1] - GAME_HEIGHT / 2.0]
}

fn compute_geometry(assets_root: &Path) -> Result<Geometry, String> {
    let metadata_document = read_data_document(assets_root, METADATA_PATH)?;
    let Typed::Metadata(metadata) = metadata_document.typed else {
        return Err(format!(
            "{METADATA_PATH} parsed as {:?}; metadata was required",
            metadata_document.kind
        ));
    };
    let play_data = metadata
        .play_data
        .as_ref()
        .ok_or_else(|| format!("{METADATA_PATH} lacks playData"))?;
    let stage_name = play_data
        .stage
        .as_deref()
        .ok_or_else(|| format!("{METADATA_PATH} lacks playData.stage"))?;
    let opponent_name = play_data
        .characters
        .as_ref()
        .and_then(|characters| characters.opponent.as_deref())
        .ok_or_else(|| format!("{METADATA_PATH} lacks playData.characters.opponent"))?;

    let stage_path = format!("preload/data/stages/{stage_name}.json");
    let stage_document = read_data_document(assets_root, &stage_path)?;
    let Typed::Stage(stage) = stage_document.typed else {
        return Err(format!(
            "{stage_path} parsed as {:?}; a stage was required",
            stage_document.kind
        ));
    };

    let mut props = BTreeMap::new();
    for prop in &stage.props {
        let asset_path = prop
            .asset_path
            .as_deref()
            .ok_or_else(|| format!("{stage_path} prop {:?} lacks assetPath", prop.name))?;
        let image_size = image_size(assets_root, asset_path)?;
        let position = prop.position.unwrap_or((0.0, 0.0));
        let scale = prop.scale.unwrap_or((1.0, 1.0));
        let key = prop.name.to_ascii_lowercase();
        if props
            .insert(
                key.clone(),
                prop_rect([position.0, position.1], [scale.0, scale.1], image_size),
            )
            .is_some()
        {
            return Err(format!("{stage_path} has duplicate prop name {key:?}"));
        }
    }
    for required in ["stageback", "stagefront", "stagecurtains"] {
        if !props.contains_key(required) {
            return Err(format!(
                "{stage_path} does not define the frozen mainStage prop {required:?}"
            ));
        }
    }

    let characters = stage
        .characters
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{stage_path} characters is not an object"))?;
    let opponent_slot = characters
        .get(opponent_name)
        .ok_or_else(|| format!("{stage_path} lacks the {opponent_name:?} character slot"))?;
    let slot_position = json_pair(
        opponent_slot
            .get("position")
            .ok_or_else(|| format!("{stage_path} {opponent_name:?} lacks position"))?,
        &format!("{stage_path} {opponent_name:?}.position"),
    )?;
    let slot_camera = opponent_slot
        .get("cameraOffsets")
        .map(|value| {
            json_pair(
                value,
                &format!("{stage_path} {opponent_name:?}.cameraOffsets"),
            )
        })
        .transpose()?
        .unwrap_or([0.0, 0.0]);
    let slot_scale = opponent_slot
        .get("scale")
        .and_then(Value::as_f64)
        .unwrap_or(1.0);

    let character_path = format!("preload/data/characters/{opponent_name}.json");
    let character_document = read_data_document(assets_root, &character_path)?;
    let Typed::Character(character) = character_document.typed else {
        return Err(format!(
            "{character_path} parsed as {:?}; a character was required",
            character_document.kind
        ));
    };
    let character_camera = character
        .camera_offsets
        .map_or([0.0, 0.0], |offsets| [offsets.0, offsets.1]);
    let character_scale = character.scale.unwrap_or(1.0);
    let bounds = atlas_bounds(assets_root, &character.asset_path)?;

    Ok(Geometry {
        receptors: receptor_positions(),
        lane_center_y: lane_center_y(),
        note_crossing_top_y: note_crossing_top_y(),
        note_top_left_y_at_offsets: note_top_left_y_at_offsets(),
        character_bounds: character_bounds(),
        props,
        camera_initial_scroll: camera_initial_scroll(
            slot_position,
            slot_scale,
            character_camera,
            slot_camera,
            character_scale,
            bounds[3],
        ),
    })
}

fn json_pair(value: &Value, context: &str) -> Result<[f64; 2], String> {
    let array = value
        .as_array()
        .ok_or_else(|| format!("{context} is not an array"))?;
    if array.len() != 2 {
        return Err(format!("{context} has {} entries, expected 2", array.len()));
    }
    let x = array[0]
        .as_f64()
        .ok_or_else(|| format!("{context}[0] is not a number"))?;
    let y = array[1]
        .as_f64()
        .ok_or_else(|| format!("{context}[1] is not a number"))?;
    if !x.is_finite() || !y.is_finite() {
        return Err(format!("{context} is not finite"));
    }
    Ok([x, y])
}

fn read_data_document(
    assets_root: &Path,
    relative: &str,
) -> Result<oxide_data::ParsedDocument, String> {
    let path = assets_root.join(relative);
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read asset {}: {error}", path.display()))?;
    oxide_data::parse_file(Path::new(relative), &bytes)
        .map_err(|error| format!("asset {relative} parse error: {error}"))
}

/// V-Slice image-key candidates from RENDER-BEHAVIOR §1/§3: an explicit
/// `shared:` maps to the shared tree; otherwise preload is tried first. The
/// renderer owns this resolution privately, so the geometry check mirrors the
/// frozen candidate order to read the same files (no VFS mod-layer claim).
/// V-Slice file bases for an image key, mirroring the renderer's observed
/// `image_file_bases` contract (`crates/oxide-game/src/render/load.rs`, read
/// only): an explicit `shared:` searches shared first, otherwise preload first.
/// xtask does not depend on `oxide-vfs`, so this replicates the OBSERVED
/// `ResKind::Image` contract table (one `.png` extension fallback, see
/// `crates/oxide-vfs/src/kinds.rs` tests) instead of copying VFS layer logic;
/// it is only used to read assets for the independent geometry assertion.
fn image_file_bases(key: &str) -> Vec<String> {
    if let Some(rest) = key.strip_prefix("shared:") {
        vec![
            format!("shared/images/{rest}"),
            format!("preload/images/{rest}"),
        ]
    } else {
        vec![
            format!("preload/images/{key}"),
            format!("shared/images/{key}"),
        ]
    }
}

/// Multi-file atlas directory bases (Animation.json, spritemap1.json).
fn image_dir_candidates(key: &str) -> Vec<String> {
    image_file_bases(key)
}

fn resolve_in_dirs(assets_root: &Path, dirs: &[String], file: &str) -> Result<PathBuf, String> {
    for directory in dirs {
        let path = assets_root.join(directory).join(file);
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(format!(
        "asset file {file:?} is missing from candidate directories {dirs:?}"
    ))
}

fn image_size(assets_root: &Path, key: &str) -> Result<[f64; 2], String> {
    let mut tried = Vec::new();
    for base in image_file_bases(key) {
        let relative = format!("{base}.png");
        let path = assets_root.join(&relative);
        if path.is_file() {
            let bytes = fs::read(&path)
                .map_err(|error| format!("cannot read image {}: {error}", path.display()))?;
            let (width, height) = png_size(&bytes)
                .map_err(|error| format!("image {} is not a valid PNG: {error}", path.display()))?;
            return Ok([f64::from(width), f64::from(height)]);
        }
        tried.push(relative);
    }
    Err(format!(
        "asset image candidates {tried:?} are missing for key {key:?}"
    ))
}

fn atlas_bounds(assets_root: &Path, asset_path: &str) -> Result<[f64; 4], String> {
    let dirs = image_dir_candidates(asset_path);
    let (spritemap_path, spritemap) = read_text_in_dirs(assets_root, &dirs, "spritemap1.json")?;
    let (animation_path, animation) = read_text_in_dirs(assets_root, &dirs, "Animation.json")?;
    let stage = AnimateStage::parse(&animation, &spritemap).map_err(|error| {
        format!("animate atlas {animation_path} (spritemap {spritemap_path}) failed: {error}")
    })?;
    let bounds = stage.bounds();
    if !bounds.iter().all(|value| value.is_finite()) || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return Err(format!(
            "animate atlas bounds are empty/non-finite: {bounds:?}"
        ));
    }
    Ok(bounds)
}

fn read_text_in_dirs(
    assets_root: &Path,
    dirs: &[String],
    file: &str,
) -> Result<(String, String), String> {
    let path = resolve_in_dirs(assets_root, dirs, file)?;
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read asset {}: {error}", path.display()))?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|error| format!("asset {} is not UTF-8: {error}", path.display()))?;
    Ok((path.display().to_string(), text))
}

fn png_size(bytes: &[u8]) -> Result<(u32, u32), String> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if bytes.get(..8) != Some(&SIGNATURE[..]) {
        return Err("missing PNG signature".into());
    }
    let length = bytes
        .get(8..12)
        .ok_or_else(|| "truncated PNG chunk header".to_string())?;
    if length != [0, 0, 0, 13] {
        return Err("first PNG chunk is not a 13-byte IHDR".into());
    }
    if bytes.get(12..16) != Some(&b"IHDR"[..]) {
        return Err("first PNG chunk type is not IHDR".into());
    }
    let width: [u8; 4] = bytes
        .get(16..20)
        .ok_or_else(|| "truncated PNG width".to_string())?
        .try_into()
        .map_err(|_| "PNG width is not 4 bytes".to_string())?;
    let height: [u8; 4] = bytes
        .get(20..24)
        .ok_or_else(|| "truncated PNG height".to_string())?
        .try_into()
        .map_err(|_| "PNG height is not 4 bytes".to_string())?;
    Ok((u32::from_be_bytes(width), u32::from_be_bytes(height)))
}

fn load_manifest(path: &Path) -> Result<Value, String> {
    let bytes = fs::read(path)
        .map_err(|error| format!("cannot read fixtures manifest {}: {error}", path.display()))?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "fixtures manifest {} is not valid JSON: {error}",
            path.display()
        )
    })?;
    let schema = manifest
        .get("schema")
        .and_then(Value::as_str)
        .ok_or_else(|| "fixtures manifest lacks the string schema".to_string())?;
    if schema != SCHEMA {
        return Err(format!(
            "fixtures manifest schema is {schema:?}, expected {SCHEMA:?}"
        ));
    }
    Ok(manifest)
}

fn compare_manifest(
    manifest: &Value,
    evidence: &BTreeMap<&'static str, DifficultyEvidence>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if manifest.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        errors.push(format!("fixtures manifest schema must be {SCHEMA:?}"));
    }
    errors.extend(compare_adapter(manifest));
    for difficulty in DIFFICULTIES {
        let Some(captured) = evidence.get(difficulty) else {
            errors.push(format!(
                "{difficulty}: no captured evidence; the manifest cannot be verified"
            ));
            continue;
        };
        errors.extend(compare_difficulty_manifest(difficulty, captured, manifest));
    }
    errors
}

fn compare_adapter(manifest: &Value) -> Vec<String> {
    match manifest.get("adapter").and_then(Value::as_str) {
        Some(adapter) if adapter == ADAPTER => Vec::new(),
        Some(adapter) => vec![format!(
            "fixtures manifest adapter is {adapter:?}, expected the software-pinned \
             {ADAPTER:?} token; frame digests are adapter-specific"
        )],
        None => vec![format!(
            "fixtures manifest lacks the adapter token {ADAPTER:?}; frame digests are \
             adapter-specific"
        )],
    }
}

fn compare_difficulty_manifest(
    difficulty: &str,
    evidence: &DifficultyEvidence,
    manifest: &Value,
) -> Vec<String> {
    let mut errors = Vec::new();
    let samples = manifest
        .get("samples")
        .and_then(Value::as_object)
        .and_then(|samples| samples.get(difficulty))
        .and_then(Value::as_array);
    let mut frozen_samples = Vec::new();
    if let Some(samples) = samples {
        for (index, value) in samples.iter().enumerate() {
            match value.as_u64() {
                Some(sample) => frozen_samples.push(sample),
                None => errors.push(format!(
                    "{difficulty}: manifest sample {index} is not a non-negative integer"
                )),
            }
        }
    } else {
        errors.push(format!("{difficulty}: manifest lacks the samples array"));
    }
    if frozen_samples != evidence.samples {
        errors.push(format!(
            "{difficulty}: frozen sample list drift: manifest {frozen_samples:?} != computed \
             {:?}; no fixture regeneration during verification",
            evidence.samples
        ));
    }

    let frozen_frames = manifest
        .get("frame_sha256")
        .and_then(Value::as_object)
        .and_then(|frames| frames.get(difficulty))
        .and_then(Value::as_object);
    match frozen_frames {
        Some(frames) => {
            for frame in &evidence.frames {
                let key = frame.sample_ms.to_string();
                match frames.get(&key).and_then(Value::as_str) {
                    Some(frozen) if is_sha256_hex(frozen) && frozen == frame.sha256 => {}
                    Some(frozen) if is_sha256_hex(frozen) => errors.push(format!(
                        "{difficulty}: frame {} sha256 {frozen} != computed {}",
                        frame.sample_ms, frame.sha256
                    )),
                    Some(frozen) => errors.push(format!(
                        "{difficulty}: frame {key} manifest digest {frozen:?} is not a \
                         64-character lowercase hex sha256"
                    )),
                    None => errors.push(format!(
                        "{difficulty}: manifest lacks the frame digest for {key} ms"
                    )),
                }
            }
            for key in frames.keys() {
                if !evidence
                    .frames
                    .iter()
                    .any(|frame| frame.sample_ms.to_string() == *key)
                {
                    errors.push(format!(
                        "{difficulty}: manifest has an extra frame digest {key:?} outside the \
                         computed sample list"
                    ));
                }
            }
        }
        None => errors.push(format!("{difficulty}: manifest lacks the frame_sha256 map")),
    }

    match manifest
        .get("run_sha256")
        .and_then(Value::as_object)
        .and_then(|runs| runs.get(difficulty))
        .and_then(Value::as_str)
    {
        Some(frozen) if is_sha256_hex(frozen) && frozen == evidence.run_sha256 => {}
        Some(frozen) if is_sha256_hex(frozen) => errors.push(format!(
            "{difficulty}: run sha256 {frozen} != computed {}",
            evidence.run_sha256
        )),
        Some(frozen) => errors.push(format!(
            "{difficulty}: run sha256 {frozen:?} is not a 64-character lowercase hex sha256"
        )),
        None => errors.push(format!("{difficulty}: manifest lacks the run_sha256 entry")),
    }

    match manifest
        .get("trace_sha256")
        .and_then(Value::as_object)
        .and_then(|traces| traces.get(difficulty))
        .and_then(Value::as_str)
    {
        Some(frozen) if is_sha256_hex(frozen) && frozen == evidence.trace_sha256 => {}
        Some(frozen) if is_sha256_hex(frozen) => errors.push(format!(
            "{difficulty}: trace sha256 {frozen} != computed {}; canonical slice trace drift",
            evidence.trace_sha256
        )),
        Some(frozen) => errors.push(format!(
            "{difficulty}: trace sha256 {frozen:?} is not a 64-character lowercase hex sha256"
        )),
        None => errors.push(format!(
            "{difficulty}: manifest lacks the trace_sha256 entry"
        )),
    }
    errors
}

fn compare_geometry(geometry: &Geometry, manifest: &Value) -> Vec<String> {
    let mut errors = Vec::new();
    let Some(frozen) = manifest.get("geometry") else {
        return vec!["manifest lacks the geometry object".into()];
    };

    match frozen.get("receptors").and_then(Value::as_array) {
        Some(receptors) => {
            if receptors.len() != geometry.receptors.len() {
                errors.push(format!(
                    "geometry receptors: manifest has {} entries, expected {}",
                    receptors.len(),
                    geometry.receptors.len()
                ));
            }
            for (index, expected) in geometry.receptors.iter().enumerate() {
                let Some(actual) = receptors.get(index) else {
                    continue;
                };
                let path = format!("geometry.receptors[{index}]");
                match actual.get("line").and_then(Value::as_str) {
                    Some(line) if line == expected.line => {}
                    Some(line) => errors.push(format!(
                        "{path}.line: manifest {line:?} != expected {:?}",
                        expected.line
                    )),
                    None => errors.push(format!("{path}.line is missing")),
                }
                match actual.get("direction").and_then(Value::as_u64) {
                    Some(direction) if direction == u64::from(expected.direction) => {}
                    Some(direction) => errors.push(format!(
                        "{path}.direction: manifest {direction} != expected {}",
                        expected.direction
                    )),
                    None => errors.push(format!("{path}.direction is missing")),
                }
                compare_geometry_number(actual, "x", expected.x(), &path, &mut errors);
                compare_geometry_number(actual, "y", expected.y(), &path, &mut errors);
            }
        }
        None => errors.push("geometry.receptors must be an array".into()),
    }

    match frozen.get("lane_center_y").and_then(Value::as_f64) {
        Some(value) if geometry_close(value, geometry.lane_center_y) => {}
        Some(value) => errors.push(format!(
            "geometry.lane_center_y: manifest {value} != expected {}",
            geometry.lane_center_y
        )),
        None => errors.push("geometry.lane_center_y is missing".into()),
    }

    match frozen.get("note_crossing_top_y").and_then(Value::as_f64) {
        Some(value) if geometry_close(value, geometry.note_crossing_top_y) => {}
        Some(value) => errors.push(format!(
            "geometry.note_crossing_top_y: manifest {value} != expected {}",
            geometry.note_crossing_top_y
        )),
        None => errors.push("geometry.note_crossing_top_y is missing".into()),
    }

    match frozen
        .get("note_top_left_y_at_offsets")
        .and_then(Value::as_object)
    {
        Some(offsets) => compare_geometry_number_map(
            offsets,
            &geometry.note_top_left_y_at_offsets,
            "geometry.note_top_left_y_at_offsets",
            &mut errors,
        ),
        None => errors.push("geometry.note_top_left_y_at_offsets must be an object".into()),
    }

    match frozen.get("character_bounds").and_then(Value::as_object) {
        Some(bounds) => compare_geometry_rect_map(
            bounds,
            &geometry.character_bounds,
            "geometry.character_bounds",
            &mut errors,
        ),
        None => errors.push("geometry.character_bounds must be an object".into()),
    }

    match frozen.get("props").and_then(Value::as_object) {
        Some(props) => {
            for (name, expected) in &geometry.props {
                let path = format!("geometry.props.{name}");
                let Some(rect) = props.get(name).and_then(Value::as_array) else {
                    errors.push(format!("{path} is missing"));
                    continue;
                };
                if rect.len() != 4 {
                    errors.push(format!(
                        "{path} has {} entries, expected [x, y, width, height]",
                        rect.len()
                    ));
                    continue;
                }
                for (index, expected_value) in expected.iter().enumerate() {
                    match rect[index].as_f64() {
                        Some(value) if geometry_close(value, *expected_value) => {}
                        Some(value) => errors.push(format!(
                            "{path}[{index}]: manifest {value} != expected {expected_value}"
                        )),
                        None => errors.push(format!("{path}[{index}] is not a number")),
                    }
                }
            }
            for name in props.keys() {
                if !geometry.props.contains_key(name) {
                    errors.push(format!(
                        "geometry.props.{name}: manifest has an extra prop outside the frozen \
                         mainStage geometry"
                    ));
                }
            }
        }
        None => errors.push("geometry.props must be an object".into()),
    }

    match frozen
        .get("camera_initial_scroll")
        .and_then(Value::as_array)
    {
        Some(scroll) if scroll.len() == 2 => {
            for (index, expected_value) in geometry.camera_initial_scroll.iter().enumerate() {
                match scroll[index].as_f64() {
                    Some(value) if geometry_close(value, *expected_value) => {}
                    Some(value) => errors.push(format!(
                        "geometry.camera_initial_scroll[{index}]: manifest {value} != expected \
                         {expected_value}"
                    )),
                    None => errors.push(format!(
                        "geometry.camera_initial_scroll[{index}] is not a number"
                    )),
                }
            }
        }
        Some(scroll) => errors.push(format!(
            "geometry.camera_initial_scroll has {} entries, expected 2",
            scroll.len()
        )),
        None => errors.push("geometry.camera_initial_scroll must be an array".into()),
    }
    errors
}

fn compare_geometry_number(
    value: &Value,
    key: &str,
    expected: f64,
    path: &str,
    errors: &mut Vec<String>,
) {
    match value.get(key).and_then(Value::as_f64) {
        Some(actual) if geometry_close(actual, expected) => {}
        Some(actual) => errors.push(format!(
            "{path}.{key}: manifest {actual} != expected {expected}"
        )),
        None => errors.push(format!("{path}.{key} is missing")),
    }
}

fn compare_geometry_number_map(
    frozen: &serde_json::Map<String, Value>,
    expected: &BTreeMap<String, f64>,
    path: &str,
    errors: &mut Vec<String>,
) {
    for (key, expected_value) in expected {
        match frozen.get(key).and_then(Value::as_f64) {
            Some(actual) if geometry_close(actual, *expected_value) => {}
            Some(actual) => errors.push(format!(
                "{path}.{key}: manifest {actual} != expected {expected_value}"
            )),
            None => errors.push(format!("{path}.{key} is missing")),
        }
    }
    for key in frozen.keys() {
        if !expected.contains_key(key) {
            errors.push(format!(
                "{path}.{key}: manifest has an extra constant outside the frozen geometry"
            ));
        }
    }
}

fn compare_geometry_rect_map(
    frozen: &serde_json::Map<String, Value>,
    expected: &BTreeMap<String, [f64; 4]>,
    path: &str,
    errors: &mut Vec<String>,
) {
    for (name, expected_rect) in expected {
        let Some(rect) = frozen.get(name).and_then(Value::as_array) else {
            errors.push(format!("{path}.{name} is missing"));
            continue;
        };
        if rect.len() != 4 {
            errors.push(format!(
                "{path}.{name} has {} entries, expected [x, y, width, height]",
                rect.len()
            ));
            continue;
        }
        for (index, expected_value) in expected_rect.iter().enumerate() {
            match rect[index].as_f64() {
                Some(actual) if geometry_close(actual, *expected_value) => {}
                Some(actual) => errors.push(format!(
                    "{path}.{name}[{index}]: manifest {actual} != expected {expected_value}"
                )),
                None => errors.push(format!("{path}.{name}[{index}] is not a number")),
            }
        }
    }
    for name in frozen.keys() {
        if !expected.contains_key(name) {
            errors.push(format!(
                "{path}.{name}: manifest has an extra entry outside the frozen geometry"
            ));
        }
    }
}

fn geometry_close(left: f64, right: f64) -> bool {
    (left - right).abs() <= GEOMETRY_ABS_TOLERANCE
}

fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let width_usize = usize::try_from(width).map_err(|_| "PNG width exceeds usize".to_string())?;
    let height_usize =
        usize::try_from(height).map_err(|_| "PNG height exceeds usize".to_string())?;
    let expected = width_usize
        .checked_mul(height_usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| "PNG dimensions overflow usize".to_string())?;
    if rgba.len() != expected {
        return Err(format!(
            "PNG input has {} bytes, expected {expected} for {width}x{height} RGBA",
            rgba.len()
        ));
    }

    let mut filtered = Vec::with_capacity(expected + height_usize);
    for row in rgba.chunks_exact(width_usize * 4) {
        filtered.push(0);
        filtered.extend_from_slice(row);
    }
    if filtered.len() != expected + height_usize {
        return Err("PNG row filtering produced an unexpected length".into());
    }

    let mut zlib = vec![0x78, 0x01];
    let chunk_count = filtered.len().div_ceil(65_535);
    for (index, block) in filtered.chunks(65_535).enumerate() {
        let final_block = index + 1 == chunk_count;
        zlib.push(u8::from(final_block));
        let length = u16::try_from(block.len())
            .map_err(|_| "deflate stored block exceeds u16".to_string())?;
        zlib.extend_from_slice(&length.to_le_bytes());
        zlib.extend_from_slice(&(!length).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&filtered).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    png_chunk(&mut png, b"IHDR", &ihdr)?;
    png_chunk(&mut png, b"IDAT", &zlib)?;
    png_chunk(&mut png, b"IEND", &[])?;
    Ok(png)
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) -> Result<(), String> {
    let length =
        u32::try_from(data.len()).map_err(|_| "PNG chunk exceeds u32 bytes".to_string())?;
    out.extend_from_slice(&length.to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
    Ok(())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    const MOD: u32 = 65_521;
    let mut a = 1u32;
    let mut b = 0u32;
    for chunk in bytes.chunks(5_552) {
        for byte in chunk {
            a += u32::from(*byte);
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!(
            geometry_close(actual, expected),
            "expected {expected}, got {actual}"
        );
    }

    fn evidence() -> DifficultyEvidence {
        DifficultyEvidence {
            difficulty: "easy",
            samples: vec![1200, 1800, 4800, 5001],
            frames: vec![
                FrameDigest {
                    sample_ms: 1200,
                    sha256: "a".repeat(64),
                },
                FrameDigest {
                    sample_ms: 1800,
                    sha256: "b".repeat(64),
                },
                FrameDigest {
                    sample_ms: 4800,
                    sha256: "c".repeat(64),
                },
                FrameDigest {
                    sample_ms: 5001,
                    sha256: "d".repeat(64),
                },
            ],
            run_sha256: "e".repeat(64),
            trace_sha256: "f".repeat(64),
        }
    }

    fn manifest_for(evidence: &DifficultyEvidence) -> Value {
        let frames = evidence
            .frames
            .iter()
            .map(|frame| (frame.sample_ms.to_string(), json!(frame.sha256)))
            .collect::<serde_json::Map<_, _>>();
        json!({
            "schema": SCHEMA,
            "adapter": ADAPTER,
            "samples": {"easy": evidence.samples},
            "frame_sha256": {"easy": frames},
            "run_sha256": {"easy": evidence.run_sha256},
            "trace_sha256": {"easy": evidence.trace_sha256},
        })
    }

    fn geometry() -> Geometry {
        Geometry {
            receptors: receptor_positions(),
            lane_center_y: lane_center_y(),
            note_crossing_top_y: note_crossing_top_y(),
            note_top_left_y_at_offsets: note_top_left_y_at_offsets(),
            character_bounds: character_bounds(),
            props: BTreeMap::from([
                ("stageback".to_string(), [-600.0, -200.0, 100.0, 50.0]),
                ("stagefront".to_string(), [-650.0, 600.0, 110.0, 55.0]),
                ("stagecurtains".to_string(), [-500.0, -300.0, 90.0, 45.0]),
            ]),
            camera_initial_scroll: [-144.0, 1.0],
        }
    }

    fn geometry_manifest(geometry: &Geometry) -> Value {
        json!({"geometry": geometry.to_json()})
    }

    #[test]
    fn image_candidates_mirror_the_renderer_file_shapes() {
        assert_eq!(
            image_file_bases("stageback"),
            [
                "preload/images/stageback".to_string(),
                "shared/images/stageback".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("shared:characters/dad"),
            [
                "shared/images/characters/dad".to_string(),
                "preload/images/characters/dad".to_string()
            ]
        );
        // The PNG join is a file base plus the one contract extension, not a
        // directory plus the last path segment (the establishment-run bug).
        assert_eq!(
            format!("{}.png", image_file_bases("stageback")[1]),
            "shared/images/stageback.png"
        );
    }

    #[test]
    fn receptor_geometry_matches_the_frozen_spec() {
        let receptors = receptor_positions();
        assert_eq!(receptors.len(), 8);
        let find = |line: &str, direction: u8| {
            receptors
                .iter()
                .find(|receptor| receptor.line == line && receptor.direction == direction)
                .copied()
                .unwrap()
        };
        close(find("player", 0).x(), 688.0 - 28.6);
        close(find("player", 3).x(), 688.0 + 3.0 * 112.0 - 28.6);
        close(find("opponent", 0).x(), 48.0 - 28.6);
        close(find("opponent", 3).x(), 48.0 + 3.0 * 112.0 - 28.6);
        for receptor in receptors {
            close(receptor.y(), 24.0);
        }
        close(lane_center_y(), 76.0);
        close(note_crossing_top_y(), 52.6);
        close(STRUMLINE_Y - INITIAL_OFFSET, 52.6);
        let offsets = note_top_left_y_at_offsets();
        close(offsets["1040"], 614.2);
        close(offsets["2080"], 1175.8);
        let bounds = character_bounds();
        assert_eq!(bounds["bf"], [-214.4, -271.3, 496.9, 513.85]);
        assert_eq!(
            bounds["dad"],
            [-270.15, -563.7707849999999, 566.785472, 841.334785]
        );
        assert_eq!(bounds["gf"], [-194.9, -333.65, 748.8, 674.85]);
    }

    #[test]
    fn lane_center_and_note_crossing_are_distinct_constants() {
        close(lane_center_y(), 76.0);
        close(note_crossing_top_y(), 52.6);
        assert!((lane_center_y() - note_crossing_top_y() - 23.4).abs() < 1e-9);
    }

    #[test]
    fn prop_and_camera_geometry_match_the_frozen_formulas() {
        close(
            prop_rect([-600.0, -200.0], [1.0, 1.0], [1920.0, 1080.0])[2],
            1920.0,
        );
        close(
            prop_rect([-650.0, 600.0], [1.1, 1.1], [100.0, 200.0])[3],
            220.0,
        );
        let scroll = camera_initial_scroll(
            [335.0, 885.0],
            1.0,
            [11.0, 6.0],
            [150.0, -100.0],
            1.0,
            860.0,
        );
        close(scroll[0], -144.0);
        close(scroll[1], 1.0);
    }

    #[test]
    fn sample_plan_is_sorted_deduplicated_and_bounded() {
        let categories = [
            (2000, EventCategory::HitSick),
            (2100, EventCategory::HitSick),
            (2600, EventCategory::HoldDrop),
            (4900, EventCategory::HitGood),
            (5000, EventCategory::Boundary161),
        ];
        let samples = sample_positions_from(&[1200, 1800, 2400], 5000, &categories).unwrap();
        assert_eq!(
            samples,
            vec![
                1200, 1800, 2000, 2250, 2400, 2600, 2850, 4800, 4900, 5000, 5001
            ]
        );
        assert!(samples.windows(2).all(|window| window[0] < window[1]));
    }

    #[test]
    fn only_the_first_event_of_each_class_is_sampled() {
        let categories = [
            (2000, EventCategory::HoldTick),
            (2001, EventCategory::HoldTick),
            (2002, EventCategory::HoldTick),
        ];
        let samples = sample_positions_from(&[], 10_000, &categories).unwrap();
        assert_eq!(samples, vec![2000, 2250, 9800, 10_001]);
    }

    #[test]
    fn hit_categories_include_the_160_161_boundary_classes() {
        assert_eq!(
            hit_categories("shit", 160).unwrap(),
            vec![EventCategory::HitShit, EventCategory::Boundary160]
        );
        assert_eq!(
            hit_categories("miss", 161).unwrap(),
            vec![EventCategory::HitMiss, EventCategory::Boundary161]
        );
        assert_eq!(
            hit_categories("sick", 45).unwrap(),
            vec![EventCategory::HitSick]
        );
        assert!(hit_categories("killer", 0).is_err());
    }

    #[test]
    fn identical_two_runs_pass() {
        let first = evidence();
        let second = first.clone();
        assert!(compare_runs("easy", &first, &second).is_ok());
    }

    #[test]
    fn nondeterministic_frame_digest_fails_closed() {
        let first = evidence();
        let mut second = first.clone();
        second.frames[2].sha256 = "0".repeat(64);
        let error = compare_runs("easy", &first, &second).unwrap_err();
        assert!(error.contains("two-run frame mismatch"), "{error}");
    }

    #[test]
    fn nondeterministic_run_digest_fails_closed() {
        let first = evidence();
        let mut second = first.clone();
        second.run_sha256 = "0".repeat(64);
        let error = compare_runs("easy", &first, &second).unwrap_err();
        assert!(error.contains("two-run render digest mismatch"), "{error}");
    }

    #[test]
    fn matching_manifest_passes() {
        let captured = evidence();
        let errors = compare_difficulty_manifest("easy", &captured, &manifest_for(&captured));
        assert!(errors.is_empty(), "{errors:?}");
        assert!(compare_adapter(&manifest_for(&captured)).is_empty());
    }

    #[test]
    fn adapter_drift_fails_closed() {
        let captured = evidence();
        let mut manifest = manifest_for(&captured);
        manifest["adapter"] = json!("GeForce GTX 1050 Ti");
        let errors = compare_adapter(&manifest);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("adapter-specific")),
            "{errors:?}"
        );
        manifest.as_object_mut().unwrap().remove("adapter");
        let errors = compare_adapter(&manifest);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("lacks the adapter")),
            "{errors:?}"
        );
    }

    #[test]
    fn sample_list_drift_fails_closed() {
        let captured = evidence();
        let mut manifest = manifest_for(&captured);
        manifest["samples"]["easy"] = json!([1200, 1800, 4800]);
        let errors = compare_difficulty_manifest("easy", &captured, &manifest);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("frozen sample list drift")),
            "{errors:?}"
        );
    }

    #[test]
    fn frame_digest_mismatch_fails_closed() {
        let captured = evidence();
        let mut manifest = manifest_for(&captured);
        manifest["frame_sha256"]["easy"]["1800"] = json!("0".repeat(64));
        let errors = compare_difficulty_manifest("easy", &captured, &manifest);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("frame 1800 sha256")),
            "{errors:?}"
        );
    }

    #[test]
    fn trace_digest_mismatch_fails_closed() {
        let captured = evidence();
        let mut manifest = manifest_for(&captured);
        manifest["trace_sha256"]["easy"] = json!("0".repeat(64));
        let errors = compare_difficulty_manifest("easy", &captured, &manifest);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("canonical slice trace drift")),
            "{errors:?}"
        );
    }

    #[test]
    fn geometry_mismatch_fails_closed() {
        let geometry = geometry();
        let mut manifest = geometry_manifest(&geometry);
        manifest["geometry"]["receptors"][0]["x"] = json!(0.0);
        manifest["geometry"]["camera_initial_scroll"][1] = json!(2.0);
        manifest["geometry"]["lane_center_y"] = json!(52.6);
        manifest["geometry"]["note_crossing_top_y"] = json!(76.0);
        manifest["geometry"]["note_top_left_y_at_offsets"]["1040"] = json!(0.0);
        manifest["geometry"]["character_bounds"]["bf"][0] = json!(0.0);
        let errors = compare_geometry(&geometry, &manifest);
        assert!(
            errors
                .iter()
                .any(|error| error.contains("geometry.receptors[0].x")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("camera_initial_scroll[1]")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("geometry.lane_center_y")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("geometry.note_crossing_top_y")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("note_top_left_y_at_offsets.1040")),
            "{errors:?}"
        );
        assert!(
            errors
                .iter()
                .any(|error| error.contains("character_bounds.bf[0]")),
            "{errors:?}"
        );
    }

    #[test]
    fn matching_geometry_passes() {
        let geometry = geometry();
        let errors = compare_geometry(&geometry, &geometry_manifest(&geometry));
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn png_encoder_emits_a_parseable_rgba_header() {
        let rgba = [255u8, 0, 0, 255, 0, 255, 0, 255];
        let png = encode_png_rgba(2, 1, &rgba).unwrap();
        assert_eq!(png_size(&png).unwrap(), (2, 1));
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(adler32(b"Wikipedia"), 0x11e60398);
        assert_eq!(crc32(b"123456789"), 0xcbf43926);
    }
}
