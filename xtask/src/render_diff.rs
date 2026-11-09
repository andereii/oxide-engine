//! `cargo xtask render-diff` — the visual-slice engine fixture.
//!
//! Implements the ADR-0020 / `tools/oracle/RENDER-BEHAVIOR.md` §7 gate:
//!
//! 1. Two chart-id families are driven through the schema-3 event-enabled
//!    canonical replay (`docs/M6-CONTRACT.md` §12.7): `bopeebo` keeps the
//!    frozen scripted negative-phase input stream, `school` uses the exact
//!    `senpai` chart taps (that chart cannot satisfy the scripted-phase
//!    boundary preconditions). The shared 1 ms tick loop, `record()`
//!    cross-check and trace sha256 live in [`crate::gameplay_sim`].
//! 2. The canonical ticks are replayed through
//!    [`oxide_game::render::SliceRenderer::build_for_song`] on the software
//!    adapter. A deterministic per-difficulty sample list is computed from the
//!    family metadata beat grid and the canonical trace's scripted-phase event
//!    kinds; schema-3 chart rows and opponent rows add no sample classes.
//!    Each sample position renders one frame, writes a PNG and a per-frame
//!    sha256 under `target/render-diff/<family>/<difficulty>/`, and the whole
//!    pipeline runs twice. Per-frame digests and the whole-run digest must be
//!    byte-identical across the two runs.
//! 3. When `tests/fixtures/render-slice/_manifest.json` exists, the frozen
//!    `render-slice-2` schema, adapter token (`llvmpipe`), per-family
//!    chart-id/difficulty lists, sample lists, per-frame digests, whole-run
//!    digests, canonical trace digests and structural geometry are verified
//!    fail-closed. A missing/unknown family section, a difficulty subset
//!    mismatch or any drift makes the gate exit nonzero; it never writes under
//!    `tests/` and never regenerates expectations.
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
//! Schema-3 chart song-event rows (FocusCamera/ZoomCamera/SetCameraBop/
//! PlayAnimation/ScrollSpeed/SetHealthIcon/unknown) and schema-2 opponent rows
//! are consumed by the scene but carry no scripted-phase sample category. The
//! list is sorted and deduplicated. `song_end` is the grid-rounded last
//! player-note end (`ceil(t + l)`) that the canonical stream covers; the
//! canonical replay then continues for its frozen 2 s tail.
//!
//! Geometry is recomputed here from the frozen spec numbers and the real
//! assets because `oxide-game::render`'s geometry module is private. This is
//! an independent, GLSL-free assertion (not a renderer-output introspection):
//! receptor positions, lane reference line, note top-left crossing y, stage
//! prop world rects and the initial camera scroll. Stage/atlas path candidate
//! order mirrors the renderer's frozen asset-key resolution so the same files
//! are inspected. The `bopeebo` family keeps the frozen mainStage checks and
//! character bounds; `school` checks the `senpai` Sparrow first-frame bounds.
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

use crate::gameplay_sim::{self, CanonicalTick, EventInputPlan};
use crate::replay::LaneNote;

const SCHEMA: &str = "render-slice-2";
/// Fixture digests are adapter-specific (ADR-0020 addendum 2026-11-07).
/// `SliceRenderer::build` is the software-pinned entry point, so the manifest
/// records this stable policy token; the harness echoes the full
/// `RenderAdapterInfo` on stderr and fails closed if it is not llvmpipe.
const ADAPTER: &str = "llvmpipe";
const FIXTURE_DIR: &str = "tests/fixtures/render-slice";
const MANIFEST_FILE: &str = "_manifest.json";
const TARGET_DIR: &str = "target/render-diff";
const HONESTY: &str = "engine fixture; schema-3 event-driven scene consumption; software adapter \
                       pinned; adapter-specific fixture digests (adapter recorded in the \
                       manifest); NOT native pixel parity; native rendered-output trace remains a \
                       separate pending obligation";
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
/// Nonzero strum offsets used to pin the note TOP-LEFT y formula
/// (RENDER-BEHAVIOR §3); see [`note_top_left_y_at_offsets`].
const NOTE_TOP_LEFT_OFFSETS_MS: [u64; 2] = [1040, 2080];
const GEOMETRY_ABS_TOLERANCE: f64 = 1e-9;

/// The input recipe a family replays through the schema-3 event sim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FamilyStream {
    /// Frozen bopeebo scripted negative phase (`prepare_stream`).
    Scripted,
    /// Exact chart taps/releases (no scripted-phase preconditions).
    ChartTaps,
}

/// The independent geometry check that is family-specific.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CharacterBoundsCheck {
    /// Frozen bopeebo bf/dad/gf content bounds.
    PinnedBopeebo,
    /// The family opponent's Sparrow first-frame content bounds.
    OpponentContent,
}

struct Family {
    name: &'static str,
    chart_id: &'static str,
    difficulties: &'static [&'static str],
    stream: FamilyStream,
    required_props: &'static [&'static str],
    character_bounds: CharacterBoundsCheck,
}

const BOPEEBO_FAMILY: Family = Family {
    name: "bopeebo",
    chart_id: "bopeebo",
    difficulties: &["easy", "normal", "hard"],
    stream: FamilyStream::Scripted,
    required_props: &["stageback", "stagefront", "stagecurtains"],
    character_bounds: CharacterBoundsCheck::PinnedBopeebo,
};

const SCHOOL_FAMILY: Family = Family {
    name: "school",
    chart_id: "senpai",
    // Observed `senpai-metadata.json` playData.difficulties (2026-11-08);
    // capture re-reads the metadata and fails closed on any mismatch.
    difficulties: &["easy", "normal", "hard"],
    // A read-only scan of `senpai-chart.json` found no sustain satisfying
    // either mandatory scripted boundary case (see `EventInputPlan::ChartTaps`),
    // so the school family uses exact chart taps.
    stream: FamilyStream::ChartTaps,
    required_props: &[],
    character_bounds: CharacterBoundsCheck::OpponentContent,
};

const FAMILIES: [&Family; 2] = [&BOPEEBO_FAMILY, &SCHOOL_FAMILY];

/// Typed failure for the fixture-boundary case called out by the gate brief.
#[derive(Debug, thiserror::Error)]
enum RenderDiffError {
    #[error(
        "fixtures not established: {path} is missing; the gate never creates or regenerates \
         fixtures. The per-family sample lists and frame digests above must be frozen through \
         the reviewed establishment step (target/render-diff/ is not committed evidence)"
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

    let mut families: BTreeMap<&'static str, FamilyEvidence> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    for family in FAMILIES {
        let mut evidence = FamilyEvidence {
            entries: BTreeMap::new(),
            geometry: None,
        };
        match declared_difficulties(&assets_root, family.chart_id) {
            Ok(declared) => {
                let matches = declared.len() == family.difficulties.len()
                    && declared
                        .iter()
                        .zip(family.difficulties)
                        .all(|(actual, expected)| actual.as_str() == *expected);
                if !matches {
                    failures.push(format!(
                        "{}: metadata declares difficulties {:?}, but the gate family requires \
                         {:?}; refusing a difficulty subset mismatch",
                        family.name, declared, family.difficulties
                    ));
                }
            }
            Err(error) => failures.push(format!("{}: {error}", family.name)),
        }
        // Print the recomputed geometry before manifest comparison so drift
        // diagnostics and the establishment log always carry the same values.
        match compute_geometry(&assets_root, family) {
            Ok(geometry) => {
                println!(
                    "render-diff: {}: computed geometry: {}",
                    family.name,
                    serde_json::to_string(&geometry.to_json())
                        .unwrap_or_else(|_| "<unserializable>".to_string())
                );
                evidence.geometry = Some(geometry);
            }
            Err(error) => {
                eprintln!("render-diff: {}: geometry check: {error}", family.name);
                failures.push(format!(
                    "{}: independent structural geometry could not be computed: {error}",
                    family.name
                ));
            }
        }
        for difficulty in family.difficulties {
            let target = workspace
                .join(TARGET_DIR)
                .join(family.name)
                .join(difficulty);
            match capture_difficulty(&assets_root, family, difficulty, &target) {
                Ok(captured) => {
                    print_evidence(family.name, &captured);
                    evidence.entries.insert(difficulty, captured);
                }
                Err(error) => {
                    eprintln!("render-diff: {}/{}: {error}", family.name, difficulty);
                    failures.push(format!("{}/{}: {error}", family.name, difficulty));
                }
            }
        }
        families.insert(family.name, evidence);
    }

    if fixtures_present {
        match load_manifest(&manifest_path) {
            Ok(manifest) => failures.extend(compare_manifest(&manifest, &families)),
            Err(error) => failures.push(error),
        }
    } else {
        println!(
            "render-diff: manifest must record \"schema\": {SCHEMA:?}, \"adapter\": {ADAPTER:?} \
             and a \"families\" object with sections {:?}; each section needs chart_id, \
             difficulties, samples, frame_sha256, run_sha256, trace_sha256 and geometry",
            FAMILIES
                .iter()
                .map(|family| family.name)
                .collect::<Vec<_>>()
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
             the frozen {SCHEMA} fixture for every family; {HONESTY}"
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

struct FamilyEvidence {
    entries: BTreeMap<&'static str, DifficultyEvidence>,
    geometry: Option<Geometry>,
}

fn declared_difficulties(assets_root: &Path, chart_id: &str) -> Result<Vec<String>, String> {
    let (_, metadata_path) = gameplay_sim::chart_asset_paths(chart_id)?;
    let document = read_data_document(assets_root, &metadata_path)?;
    let Typed::Metadata(metadata) = document.typed else {
        return Err(format!(
            "{metadata_path} parsed as {:?}; metadata was required",
            document.kind
        ));
    };
    let play_data = metadata
        .play_data
        .as_ref()
        .ok_or_else(|| format!("{metadata_path} lacks playData"))?;
    if play_data.difficulties.is_empty() {
        return Err(format!("{metadata_path} declares an empty difficulty list"));
    }
    Ok(play_data.difficulties.clone())
}

fn capture_difficulty(
    assets_root: &Path,
    family: &'static Family,
    difficulty: &'static str,
    target_dir: &Path,
) -> Result<DifficultyEvidence, String> {
    let notes =
        gameplay_sim::load_verified_notes_for_chart(assets_root, family.chart_id, difficulty)?;
    // The complete pipeline (sim replay, sample derivation, frame rendering
    // and evidence hashing) runs twice from fresh state. Only the first run
    // writes the never-committed target evidence.
    let first = capture_once(assets_root, family, difficulty, &notes, Some(target_dir))?;
    let second = capture_once(assets_root, family, difficulty, &notes, None)?;
    compare_runs(&format!("{}/{}", family.name, difficulty), &first, &second)?;
    Ok(first)
}

fn capture_once(
    assets_root: &Path,
    family: &Family,
    difficulty: &'static str,
    notes: &[LaneNote],
    write_dir: Option<&Path>,
) -> Result<DifficultyEvidence, String> {
    let plan = match family.stream {
        FamilyStream::Scripted => EventInputPlan::Scripted(notes),
        FamilyStream::ChartTaps => EventInputPlan::ChartTaps(notes),
    };
    let replay =
        gameplay_sim::canonical_event_replay(assets_root, family.chart_id, difficulty, plan)?;
    let samples = sample_positions(assets_root, family.chart_id, notes, &replay.ticks)?;
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

    let mut renderer = SliceRenderer::build_for_song(assets_root, family.chart_id, difficulty)
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

fn print_evidence(family: &str, evidence: &DifficultyEvidence) {
    println!(
        "render-diff: {family}/{}: sample list ({}): {:?}",
        evidence.difficulty,
        evidence.samples.len(),
        evidence.samples
    );
    println!(
        "render-diff: {family}/{}: slice trace sha256 {}; run sha256 {}; frames {}",
        evidence.difficulty,
        evidence.trace_sha256,
        evidence.run_sha256,
        evidence.frames.len()
    );
    for frame in &evidence.frames {
        println!(
            "render-diff: {family}/{}: frame {} sha256 {}",
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
        // Schema-2 opponent rows and schema-3 chart-event rows are consumed by
        // the scene but are not scripted-phase sample classes: the frozen
        // sample plan keeps its eight beats, end margin, scripted categories
        // and last-note tick.
        TraceEvent::OpponentHit { .. }
        | TraceEvent::CameraFocus { .. }
        | TraceEvent::CameraZoom { .. }
        | TraceEvent::CameraBop { .. }
        | TraceEvent::PlayAnimation { .. }
        | TraceEvent::ScrollSpeed { .. }
        | TraceEvent::HealthIcon { .. }
        | TraceEvent::UnknownEvent { .. } => Ok(Vec::new()),
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
        | TraceEvent::OpponentHit { sim, .. }
        | TraceEvent::CameraFocus { sim, .. }
        | TraceEvent::CameraZoom { sim, .. }
        | TraceEvent::CameraBop { sim, .. }
        | TraceEvent::PlayAnimation { sim, .. }
        | TraceEvent::ScrollSpeed { sim, .. }
        | TraceEvent::HealthIcon { sim, .. }
        | TraceEvent::UnknownEvent { sim, .. } => *sim,
    }
}

/// Deterministic sample plan for one family difficulty.
pub(crate) fn sample_positions(
    assets_root: &Path,
    chart_id: &str,
    notes: &[LaneNote],
    ticks: &[CanonicalTick],
) -> Result<Vec<u64>, String> {
    let (_, metadata_path) = gameplay_sim::chart_asset_paths(chart_id)?;
    let song_end = song_end_ms(notes)?;
    let beats = beat_times(assets_root, &metadata_path, song_end)?;
    let mut categories = Vec::new();
    for tick in ticks {
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

fn beat_times(assets_root: &Path, metadata_path: &str, song_end: u64) -> Result<Vec<u64>, String> {
    let mut conductor = Conductor::new();
    conductor.map_time_changes(&conductor_time_changes(assets_root, metadata_path)?);
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

fn conductor_time_changes(
    assets_root: &Path,
    metadata_path: &str,
) -> Result<Vec<TimeChange>, String> {
    let document = read_data_document(assets_root, metadata_path)?;
    let Typed::Metadata(metadata) = document.typed else {
        return Err(format!(
            "{metadata_path} parsed as {:?}; metadata was required",
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
/// at the family's easy scroll speed. RENDER-BEHAVIOR §3:
/// `y = STRUMLINE_Y - INITIAL_OFFSET + getNoteY`, with
/// `getNoteY = 0.45 * (song_ms - strum_ms) * scrollSpeed * -1`; for a future
/// note `(song_ms - strum_ms) = -offset_ms`, hence
/// `52.6 + 0.45 * offset_ms * scrollSpeed`. The frozen bopeebo values
/// (scrollSpeed 1.2): 1040 -> 614.2, 2080 -> 1175.8.
fn note_top_left_y_at_offsets(scroll_speed: f64) -> BTreeMap<String, f64> {
    NOTE_TOP_LEFT_OFFSETS_MS
        .iter()
        .map(|offset_ms| {
            (
                offset_ms.to_string(),
                note_crossing_top_y() + PIXELS_PER_MS * (*offset_ms as f64) * scroll_speed,
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

fn compute_geometry(assets_root: &Path, family: &Family) -> Result<Geometry, String> {
    let (chart_path, metadata_path) = gameplay_sim::chart_asset_paths(family.chart_id)?;
    let metadata_document = read_data_document(assets_root, &metadata_path)?;
    let Typed::Metadata(metadata) = metadata_document.typed else {
        return Err(format!(
            "{metadata_path} parsed as {:?}; metadata was required",
            metadata_document.kind
        ));
    };
    let play_data = metadata
        .play_data
        .as_ref()
        .ok_or_else(|| format!("{metadata_path} lacks playData"))?;
    let stage_name = play_data
        .stage
        .as_deref()
        .ok_or_else(|| format!("{metadata_path} lacks playData.stage"))?;
    let opponent_name = play_data
        .characters
        .as_ref()
        .and_then(|characters| characters.opponent.as_deref())
        .ok_or_else(|| format!("{metadata_path} lacks playData.characters.opponent"))?;

    let stage_path = format!("preload/data/stages/{stage_name}.json");
    let stage_document = read_data_document(assets_root, &stage_path)?;
    let Typed::Stage(stage) = stage_document.typed else {
        return Err(format!(
            "{stage_path} parsed as {:?}; a stage was required",
            stage_document.kind
        ));
    };
    let level_dir = stage.directory.as_deref();

    let mut props = BTreeMap::new();
    for prop in &stage.props {
        let asset_path = prop
            .asset_path
            .as_deref()
            .ok_or_else(|| format!("{stage_path} prop {:?} lacks assetPath", prop.name))?;
        let image_size = image_size(assets_root, asset_path, level_dir)?;
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
    for required in family.required_props {
        if !props.contains_key(*required) {
            return Err(format!(
                "{stage_path} does not define the frozen {required:?} prop"
            ));
        }
    }

    let characters = stage
        .characters
        .as_ref()
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{stage_path} characters is not an object"))?;
    // Stage character slots are role-keyed (`bf`/`dad`/`gf`); the metadata
    // opponent id selects the character data, not the slot key.
    let opponent_slot = characters
        .get("dad")
        .ok_or_else(|| format!("{stage_path} lacks the \"dad\" character slot"))?;
    let slot_position = json_pair(
        opponent_slot
            .get("position")
            .ok_or_else(|| format!("{stage_path} \"dad\" lacks position"))?,
        &format!("{stage_path} \"dad\".position"),
    )?;
    let slot_camera = opponent_slot
        .get("cameraOffsets")
        .map(|value| json_pair(value, &format!("{stage_path} \"dad\".cameraOffsets")))
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
    let bounds = character_content_bounds(assets_root, &character, level_dir)?;
    let character_bounds = match family.character_bounds {
        CharacterBoundsCheck::PinnedBopeebo => character_bounds(),
        CharacterBoundsCheck::OpponentContent => {
            BTreeMap::from([(opponent_name.to_string(), bounds)])
        }
    };

    let chart_document = read_data_document(assets_root, &chart_path)?;
    let Typed::Chart(chart) = chart_document.typed else {
        return Err(format!(
            "{chart_path} parsed as {:?}; a chart was required",
            chart_document.kind
        ));
    };
    let scroll_speed = chart
        .scroll_speed
        .as_ref()
        .and_then(|scroll| scroll.get("easy"))
        .and_then(Value::as_f64)
        .unwrap_or(1.0);
    if !scroll_speed.is_finite() || scroll_speed <= 0.0 {
        return Err(format!(
            "{chart_path} easy scrollSpeed {scroll_speed} must be finite and positive"
        ));
    }

    Ok(Geometry {
        receptors: receptor_positions(),
        lane_center_y: lane_center_y(),
        note_crossing_top_y: note_crossing_top_y(),
        note_top_left_y_at_offsets: note_top_left_y_at_offsets(scroll_speed),
        character_bounds,
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

/// The opponent character's whole-sprite bounds at scale 1, mirroring the
/// renderer's `Sheet::bounds` rule: animate atlases use the parsed stage
/// bounds; Sparrow characters use the first document frame's destination size
/// as `[0, 0, width, height]`.
fn character_content_bounds(
    assets_root: &Path,
    character: &oxide_data::character::CharacterData,
    level_dir: Option<&str>,
) -> Result<[f64; 4], String> {
    match character.render_type.as_deref() {
        Some("animateatlas") | Some("multianimateatlas") => {
            atlas_bounds(assets_root, &character.asset_path, level_dir)
        }
        None | Some("sparrow") | Some("multisparrow") => {
            sparrow_bounds(assets_root, &character.asset_path, level_dir)
        }
        Some(other) => Err(format!(
            "character {:?} renderType {other:?} is outside the render-diff geometry families",
            character.name
        )),
    }
}

fn sparrow_bounds(
    assets_root: &Path,
    asset_path: &str,
    level_dir: Option<&str>,
) -> Result<[f64; 4], String> {
    let bases = image_dir_candidates(asset_path, level_dir);
    for base in &bases {
        let path = assets_root.join(format!("{base}.xml"));
        if !path.is_file() {
            continue;
        }
        let bytes = fs::read(&path)
            .map_err(|error| format!("cannot read sparrow atlas {}: {error}", path.display()))?;
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|error| format!("sparrow atlas {} is not UTF-8: {error}", path.display()))?;
        let atlas = oxide_atlas::parse_sparrow(&text)
            .map_err(|error| format!("sparrow atlas {} failed: {error}", path.display()))?;
        let first = atlas
            .subtextures
            .first()
            .ok_or_else(|| format!("sparrow atlas {} has no subtextures", path.display()))?;
        let quad = oxide_atlas::frame_quad(first);
        let [width, height] = quad.dest_size;
        if width <= 0 || height <= 0 {
            return Err(format!(
                "sparrow atlas {} first frame has empty content {width}x{height}",
                path.display()
            ));
        }
        return Ok([0.0, 0.0, f64::from(width), f64::from(height)]);
    }
    Err(format!(
        "sparrow atlas {asset_path:?} is missing from candidate directories {bases:?}"
    ))
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
    // Provenance paths are game-checkout relative (`assets/preload/...`); the
    // caller supplies the assets directory itself.
    let relative = relative.strip_prefix("assets/").unwrap_or(relative);
    let path = assets_root.join(relative);
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read asset {}: {error}", path.display()))?;
    oxide_data::parse_file(Path::new(relative), &bytes)
        .map_err(|error| format!("asset {relative} parse error: {error}"))
}

/// V-Slice file bases for an image key, mirroring the renderer's observed
/// `image_file_bases` contract (`crates/oxide-game/src/render/load.rs`, read
/// only): an explicit `<lib>:` searches `<lib>/images` first, then shared and
/// preload; a plain key searches the stage's level directory first, then
/// shared and preload.
/// xtask does not depend on `oxide-vfs`, so this replicates the OBSERVED
/// `ResKind::Image` contract table (one `.png` extension fallback, see
/// `crates/oxide-vfs/src/kinds.rs` tests) instead of copying VFS layer logic;
/// it is only used to read assets for the independent geometry assertion.
fn image_file_bases(key: &str, level_dir: Option<&str>) -> Vec<String> {
    if let Some((library, rest)) = key.split_once(':') {
        let mut bases = vec![format!("{library}/images/{rest}")];
        for fallback in ["shared", "preload"] {
            let candidate = format!("{fallback}/images/{rest}");
            if !bases.contains(&candidate) {
                bases.push(candidate);
            }
        }
        return bases;
    }
    let mut bases = Vec::with_capacity(3);
    if let Some(directory) = level_dir {
        bases.push(format!("{directory}/images/{key}"));
    }
    for fallback in ["shared", "preload"] {
        let candidate = format!("{fallback}/images/{key}");
        if !bases.contains(&candidate) {
            bases.push(candidate);
        }
    }
    bases
}

/// Multi-file atlas directory bases (Animation.json, spritemap1.json).
fn image_dir_candidates(key: &str, level_dir: Option<&str>) -> Vec<String> {
    image_file_bases(key, level_dir)
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

fn image_size(assets_root: &Path, key: &str, level_dir: Option<&str>) -> Result<[f64; 2], String> {
    let mut tried = Vec::new();
    for base in image_file_bases(key, level_dir) {
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

fn atlas_bounds(
    assets_root: &Path,
    asset_path: &str,
    level_dir: Option<&str>,
) -> Result<[f64; 4], String> {
    let dirs = image_dir_candidates(asset_path, level_dir);
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
    validate_manifest(&manifest)?;
    Ok(manifest)
}

fn validate_manifest(manifest: &Value) -> Result<(), String> {
    let schema = manifest
        .get("schema")
        .and_then(Value::as_str)
        .ok_or_else(|| "fixtures manifest lacks the string schema".to_string())?;
    if schema != SCHEMA {
        return Err(format!(
            "fixtures manifest schema is {schema:?}, expected {SCHEMA:?}"
        ));
    }
    let families = manifest
        .get("families")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!("fixtures manifest lacks the families object required by {SCHEMA}")
        })?;
    for name in families.keys() {
        if !FAMILIES.iter().any(|family| family.name == name) {
            return Err(format!(
                "fixtures manifest has unknown family section {name:?}; expected {:?}",
                FAMILIES
                    .iter()
                    .map(|family| family.name)
                    .collect::<Vec<_>>()
            ));
        }
    }
    Ok(())
}

fn compare_manifest(
    manifest: &Value,
    evidence: &BTreeMap<&'static str, FamilyEvidence>,
) -> Vec<String> {
    let mut errors = Vec::new();
    if manifest.get("schema").and_then(Value::as_str) != Some(SCHEMA) {
        errors.push(format!("fixtures manifest schema must be {SCHEMA:?}"));
    }
    errors.extend(compare_adapter(manifest));
    let Some(families) = manifest.get("families").and_then(Value::as_object) else {
        errors.push("fixtures manifest lacks the families object".into());
        return errors;
    };
    for name in families.keys() {
        if !FAMILIES.iter().any(|family| family.name == name) {
            errors.push(format!(
                "fixtures manifest has an unknown family section {name:?}"
            ));
        }
    }
    for family in FAMILIES {
        let Some(section) = families.get(family.name) else {
            errors.push(format!(
                "{}: manifest lacks the family section (expected chart_id {:?}, difficulties \
                 {:?})",
                family.name, family.chart_id, family.difficulties
            ));
            continue;
        };
        match section.get("chart_id").and_then(Value::as_str) {
            Some(chart_id) if chart_id == family.chart_id => {}
            Some(chart_id) => errors.push(format!(
                "{}: manifest chart_id {chart_id:?} != expected {:?}",
                family.name, family.chart_id
            )),
            None => errors.push(format!(
                "{}: manifest lacks the string chart_id",
                family.name
            )),
        }
        match section.get("difficulties").and_then(Value::as_array) {
            Some(values) => {
                let frozen: Vec<&str> = values.iter().filter_map(Value::as_str).collect();
                if frozen.len() != values.len() || frozen.as_slice() != family.difficulties {
                    errors.push(format!(
                        "{}: manifest difficulties {values:?} != expected {:?}",
                        family.name, family.difficulties
                    ));
                }
            }
            None => errors.push(format!(
                "{}: manifest lacks the difficulties array",
                family.name
            )),
        }
        for map_name in ["samples", "frame_sha256", "run_sha256", "trace_sha256"] {
            if let Some(map) = section.get(map_name).and_then(Value::as_object) {
                for key in map.keys() {
                    if !family.difficulties.contains(&key.as_str()) {
                        errors.push(format!(
                            "{}: manifest {map_name} has an extra difficulty {key:?} outside {:?}",
                            family.name, family.difficulties
                        ));
                    }
                }
            }
        }
        let Some(captured_family) = evidence.get(family.name) else {
            errors.push(format!("{}: no captured family evidence", family.name));
            continue;
        };
        match &captured_family.geometry {
            Some(geometry) => errors.extend(compare_geometry(geometry, section)),
            None => errors.push(format!("{}: no computed geometry", family.name)),
        }
        for difficulty in family.difficulties {
            match captured_family.entries.get(difficulty) {
                Some(captured) => errors.extend(compare_difficulty_manifest(
                    family.name,
                    difficulty,
                    captured,
                    section,
                )),
                None => errors.push(format!(
                    "{}/{}: no captured evidence; the manifest cannot be verified",
                    family.name, difficulty
                )),
            }
        }
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
    family: &str,
    difficulty: &str,
    evidence: &DifficultyEvidence,
    section: &Value,
) -> Vec<String> {
    let mut errors = Vec::new();
    let label = format!("{family}/{difficulty}");
    let samples = section
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
                    "{label}: manifest sample {index} is not a non-negative integer"
                )),
            }
        }
    } else {
        errors.push(format!("{label}: manifest lacks the samples array"));
    }
    if frozen_samples != evidence.samples {
        errors.push(format!(
            "{label}: frozen sample list drift: manifest {frozen_samples:?} != computed \
             {:?}; no fixture regeneration during verification",
            evidence.samples
        ));
    }

    let frozen_frames = section
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
                        "{label}: frame {} sha256 {frozen} != computed {}",
                        frame.sample_ms, frame.sha256
                    )),
                    Some(frozen) => errors.push(format!(
                        "{label}: frame {key} manifest digest {frozen:?} is not a \
                         64-character lowercase hex sha256"
                    )),
                    None => errors.push(format!(
                        "{label}: manifest lacks the frame digest for {key} ms"
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
                        "{label}: manifest has an extra frame digest {key:?} outside the \
                         computed sample list"
                    ));
                }
            }
        }
        None => errors.push(format!("{label}: manifest lacks the frame_sha256 map")),
    }

    match section
        .get("frame_counts")
        .and_then(Value::as_object)
        .and_then(|counts| counts.get(difficulty))
        .and_then(Value::as_u64)
    {
        Some(count) if count as usize == evidence.frames.len() => {}
        Some(count) => errors.push(format!(
            "{label}: frame count {count} != computed {}",
            evidence.frames.len()
        )),
        None => errors.push(format!("{label}: manifest lacks the frame_counts entry")),
    }

    match section
        .get("run_sha256")
        .and_then(Value::as_object)
        .and_then(|runs| runs.get(difficulty))
        .and_then(Value::as_str)
    {
        Some(frozen) if is_sha256_hex(frozen) && frozen == evidence.run_sha256 => {}
        Some(frozen) if is_sha256_hex(frozen) => errors.push(format!(
            "{label}: run sha256 {frozen} != computed {}",
            evidence.run_sha256
        )),
        Some(frozen) => errors.push(format!(
            "{label}: run sha256 {frozen:?} is not a 64-character lowercase hex sha256"
        )),
        None => errors.push(format!("{label}: manifest lacks the run_sha256 entry")),
    }

    match section
        .get("trace_sha256")
        .and_then(Value::as_object)
        .and_then(|traces| traces.get(difficulty))
        .and_then(Value::as_str)
    {
        Some(frozen) if is_sha256_hex(frozen) && frozen == evidence.trace_sha256 => {}
        Some(frozen) if is_sha256_hex(frozen) => errors.push(format!(
            "{label}: trace sha256 {frozen} != computed {}; canonical slice trace drift",
            evidence.trace_sha256
        )),
        Some(frozen) => errors.push(format!(
            "{label}: trace sha256 {frozen:?} is not a 64-character lowercase hex sha256"
        )),
        None => errors.push(format!("{label}: manifest lacks the trace_sha256 entry")),
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
    use std::sync::atomic::{AtomicU64, Ordering};

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

    fn evidence_for(difficulty: &'static str) -> DifficultyEvidence {
        let mut evidence = evidence();
        evidence.difficulty = difficulty;
        evidence
    }

    fn family_section(family: &Family) -> Value {
        let mut samples = serde_json::Map::new();
        let mut frame_sha256 = serde_json::Map::new();
        let mut run_sha256 = serde_json::Map::new();
        let mut trace_sha256 = serde_json::Map::new();
        let mut frame_counts = serde_json::Map::new();
        for difficulty in family.difficulties {
            let evidence = evidence_for(difficulty);
            let frames = evidence
                .frames
                .iter()
                .map(|frame| (frame.sample_ms.to_string(), json!(frame.sha256)))
                .collect::<serde_json::Map<_, _>>();
            samples.insert((*difficulty).to_string(), json!(evidence.samples));
            frame_sha256.insert((*difficulty).to_string(), Value::Object(frames));
            run_sha256.insert((*difficulty).to_string(), json!(evidence.run_sha256));
            trace_sha256.insert((*difficulty).to_string(), json!(evidence.trace_sha256));
            frame_counts.insert((*difficulty).to_string(), json!(evidence.frames.len()));
        }
        json!({
            "chart_id": family.chart_id,
            "difficulties": family.difficulties,
            "samples": samples,
            "frame_sha256": frame_sha256,
            "run_sha256": run_sha256,
            "trace_sha256": trace_sha256,
            "frame_counts": frame_counts,
            "geometry": geometry().to_json(),
        })
    }

    fn family_evidence(family: &'static Family) -> FamilyEvidence {
        FamilyEvidence {
            entries: family
                .difficulties
                .iter()
                .map(|difficulty| (*difficulty, evidence_for(difficulty)))
                .collect(),
            geometry: Some(geometry()),
        }
    }

    fn full_manifest() -> Value {
        json!({
            "schema": SCHEMA,
            "adapter": ADAPTER,
            "families": {
                "bopeebo": family_section(&BOPEEBO_FAMILY),
                "school": family_section(&SCHOOL_FAMILY),
            }
        })
    }

    fn full_evidence() -> BTreeMap<&'static str, FamilyEvidence> {
        FAMILIES
            .iter()
            .map(|family| (family.name, family_evidence(family)))
            .collect()
    }

    fn geometry() -> Geometry {
        Geometry {
            receptors: receptor_positions(),
            lane_center_y: lane_center_y(),
            note_crossing_top_y: note_crossing_top_y(),
            note_top_left_y_at_offsets: note_top_left_y_at_offsets(1.2),
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
            image_file_bases("stageback", None),
            [
                "shared/images/stageback".to_string(),
                "preload/images/stageback".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("stageback", Some("week1")),
            [
                "week1/images/stageback".to_string(),
                "shared/images/stageback".to_string(),
                "preload/images/stageback".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("shared:characters/dad", None),
            [
                "shared/images/characters/dad".to_string(),
                "preload/images/characters/dad".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("week6:weeb/pixelUI/arrows-pixels", None),
            [
                "week6/images/weeb/pixelUI/arrows-pixels".to_string(),
                "shared/images/weeb/pixelUI/arrows-pixels".to_string(),
                "preload/images/weeb/pixelUI/arrows-pixels".to_string()
            ]
        );
        // The PNG join is a file base plus the one contract extension, not a
        // directory plus the last path segment (the establishment-run bug).
        assert_eq!(
            format!("{}.png", image_file_bases("stageback", None)[0]),
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
        let offsets = note_top_left_y_at_offsets(1.2);
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

    fn hit_sick(sim: u64) -> TraceEvent {
        TraceEvent::Hit {
            schema: "3",
            kind: "hit",
            sim,
            input_ms: sim as f64,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "sick",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            score_delta: 350,
            health_before: 1.0,
            health_after: 1.0,
            combo_before: 0,
            combo_after: 1,
            combo_max: 1,
            tallies: oxide_game::slice::TallyCounts {
                sick: 1,
                total_notes_hit: 1,
                ..oxide_game::slice::TallyCounts::default()
            },
        }
    }

    fn opponent_hit_row() -> TraceEvent {
        TraceEvent::OpponentHit {
            schema: "3",
            kind: "opponent_hit",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 4,
            judgement: "sick",
            raw_note_diff: 0.0,
            note_diff_int: 0,
        }
    }

    fn camera_focus_row() -> TraceEvent {
        TraceEvent::CameraFocus {
            schema: "3",
            kind: "camera_focus",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            target_char: 1,
            target_kind: "dad",
            x: 0.0,
            y: 0.0,
            world_focus: [0.0, 0.0],
            duration: 4.0,
            ease_kind: "classic",
            ease: None,
            ease_dir: None,
        }
    }

    fn camera_zoom_row() -> TraceEvent {
        TraceEvent::CameraZoom {
            schema: "3",
            kind: "camera_zoom",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            zoom: 1.0,
            mode: "stage",
            resolved_target_zoom: 1.0,
            duration: 4.0,
            ease: Some("linear".into()),
            ease_dir: Some(String::new()),
        }
    }

    fn camera_bop_row() -> TraceEvent {
        TraceEvent::CameraBop {
            schema: "3",
            kind: "camera_bop",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            intensity: 1.0,
            camera_bop_intensity: 1.015,
            hud_intensity: 0.03,
            rate: 4.0,
            offset: 0.0,
        }
    }

    fn play_animation_row() -> TraceEvent {
        TraceEvent::PlayAnimation {
            schema: "3",
            kind: "play_animation",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            target: "dad".into(),
            anim: "singLEFT".into(),
            force: false,
            resolved_kind: "dad".into(),
            skipped_old: false,
        }
    }

    fn scroll_speed_row() -> TraceEvent {
        TraceEvent::ScrollSpeed {
            schema: "3",
            kind: "scroll_speed",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            strumline: "player",
            target: 1.2,
            absolute: true,
            base_scroll: 1.2,
            duration: 4.0,
            ease: Some("linear".into()),
            ease_dir: Some(String::new()),
        }
    }

    fn health_icon_row() -> TraceEvent {
        TraceEvent::HealthIcon {
            schema: "3",
            kind: "health_icon",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            char: 0,
            id: "face".into(),
            should_bop: true,
            scale: 1.0,
            flip_x: false,
            is_pixel: false,
            offsets: [0.0, 0.0],
        }
    }

    fn unknown_event_row() -> TraceEvent {
        TraceEvent::UnknownEvent {
            schema: "3",
            kind: "unknown_event",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            event_kind: "EnableMask".into(),
            value: None,
        }
    }

    #[test]
    fn event_categories_ignore_opponent_and_schema3_rows() {
        for event in [
            opponent_hit_row(),
            camera_focus_row(),
            camera_zoom_row(),
            camera_bop_row(),
            play_animation_row(),
            scroll_speed_row(),
            health_icon_row(),
            unknown_event_row(),
        ] {
            assert_eq!(
                event_categories(&event).unwrap(),
                Vec::<EventCategory>::new()
            );
        }
        let game_over = TraceEvent::GameOver {
            schema: "3",
            kind: "game_over",
            sim: 1000,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "miss",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            death_count: 1,
        };
        assert!(event_categories(&game_over).is_err());
    }

    #[test]
    fn schema3_rows_do_not_change_the_sample_plan() {
        let hit = hit_sick(2000);
        let mut categories = Vec::new();
        for event in [&hit, &camera_focus_row(), &opponent_hit_row()] {
            for category in event_categories(event).unwrap() {
                categories.push((trace_sim(event), category));
            }
        }
        let samples = sample_positions_from(&[1200, 1800], 5000, &categories).unwrap();
        assert_eq!(samples, vec![1200, 1800, 2000, 2250, 4800, 5001]);
    }

    #[test]
    fn chart_asset_paths_are_family_parameterized() {
        assert_eq!(
            gameplay_sim::chart_asset_paths("bopeebo").unwrap(),
            (
                String::from("assets/preload/data/songs/bopeebo/bopeebo-chart.json"),
                String::from("assets/preload/data/songs/bopeebo/bopeebo-metadata.json"),
            )
        );
        assert_eq!(
            gameplay_sim::chart_asset_paths("senpai").unwrap(),
            (
                String::from("assets/preload/data/songs/senpai/senpai-chart.json"),
                String::from("assets/preload/data/songs/senpai/senpai-metadata.json"),
            )
        );
        assert_eq!(
            gameplay_sim::chart_asset_paths("philly-nice").unwrap().0,
            "assets/preload/data/songs/philly-nice/philly-nice-chart.json"
        );
    }

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct TempAssets {
        root: PathBuf,
    }

    impl TempAssets {
        fn new() -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../target/render-diff-tests")
                .join(format!(
                    "{}-{}",
                    std::process::id(),
                    NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
                ));
            fs::create_dir_all(&root).expect("create fixture root");
            Self { root }
        }
    }

    impl Drop for TempAssets {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn beat_derivation_reads_the_family_metadata_path() {
        let assets = TempAssets::new();
        let metadata_path = "preload/data/songs/senpai/senpai-metadata.json";
        let file = assets.root.join(metadata_path);
        fs::create_dir_all(file.parent().expect("metadata parent")).expect("create metadata dir");
        fs::write(
            &file,
            r#"{"version":"2.2.4","songName":"Senpai","playData":{"difficulties":["easy","normal","hard"],"stage":"school"},"timeChanges":[{"t":0,"bpm":144}]}"#,
        )
        .expect("write metadata");
        let changes = conductor_time_changes(&assets.root, metadata_path).expect("time changes");
        assert_eq!(changes.len(), 1);
        assert!((changes[0].bpm - 144.0).abs() < 1e-12);
        let beats = beat_times(&assets.root, metadata_path, 5000).expect("beat grid");
        assert_eq!(beats, vec![1250, 1667, 2084, 2500, 2917, 3334, 3750, 4167]);
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
        let manifest = full_manifest();
        assert!(validate_manifest(&manifest).is_ok());
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(errors.is_empty(), "{errors:?}");
        assert!(compare_adapter(&manifest).is_empty());
    }

    #[test]
    fn manifest_schema_token_must_be_render_slice_2() {
        let mut manifest = full_manifest();
        manifest["schema"] = json!("render-slice-1");
        let error = validate_manifest(&manifest).unwrap_err();
        assert!(error.contains("render-slice-2"), "{error}");
    }

    #[test]
    fn unknown_family_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["weekend1"] = json!({});
        let error = validate_manifest(&manifest).unwrap_err();
        assert!(error.contains("unknown family"), "{error}");
    }

    #[test]
    fn missing_family_section_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]
            .as_object_mut()
            .unwrap()
            .remove("school");
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("school") && error.contains("lacks the family")),
            "{errors:?}"
        );
    }

    #[test]
    fn difficulty_subset_mismatch_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["school"]["difficulties"] = json!(["easy", "normal"]);
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("school") && error.contains("difficulties")),
            "{errors:?}"
        );
    }

    #[test]
    fn extra_difficulty_map_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["bopeebo"]["samples"]["pico"] = json!([1200]);
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("extra difficulty") && error.contains("pico")),
            "{errors:?}"
        );
    }

    #[test]
    fn adapter_drift_fails_closed() {
        let mut manifest = full_manifest();
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
        let mut manifest = full_manifest();
        manifest["families"]["bopeebo"]["samples"]["easy"] = json!([1200, 1800, 4800]);
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("frozen sample list drift")),
            "{errors:?}"
        );
    }

    #[test]
    fn frame_digest_mismatch_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["bopeebo"]["frame_sha256"]["easy"]["1800"] = json!("0".repeat(64));
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors
                .iter()
                .any(|error| error.contains("frame 1800 sha256")),
            "{errors:?}"
        );
    }

    #[test]
    fn frame_count_mismatch_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["bopeebo"]["frame_counts"]["easy"] = json!(3);
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors.iter().any(|error| error.contains("frame count 3")),
            "{errors:?}"
        );
    }

    #[test]
    fn run_digest_mismatch_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["bopeebo"]["run_sha256"]["easy"] = json!("0".repeat(64));
        let errors = compare_manifest(&manifest, &full_evidence());
        assert!(
            errors.iter().any(|error| error.contains("run sha256")),
            "{errors:?}"
        );
    }

    #[test]
    fn trace_digest_mismatch_fails_closed() {
        let mut manifest = full_manifest();
        manifest["families"]["bopeebo"]["trace_sha256"]["easy"] = json!("0".repeat(64));
        let errors = compare_manifest(&manifest, &full_evidence());
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
