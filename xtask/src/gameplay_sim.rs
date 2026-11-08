//! `cargo xtask gameplay-sim` — deterministic bopeebo slice acceptance trace.
//!
//! Implements the `gameplay_sim_bopeebo` M5 gate from `docs/M5-CONTRACT.md` §2
//! by driving the frozen [`oxide_game::slice`] simulation over a synthesized
//! 1 ms-grid input stream. The gate is acceptance tooling: it never writes into
//! the repository asset tree, only into `target/gameplay-sim/`.
//!
//! Input synthesis mirrors the M4 replay recipe and then applies the contract's
//! scripted negative phase. Engine decisions for details the frozen contract
//! leaves open (all fail closed, never silently skipped):
//!
//! - Presses land on `ceil(note.t)` and releases on `ceil(note.t + note.l)`;
//!   taps release on the press tick. The fractional-sustain ceil rule is the
//!   contract's frozen conversion rule (the corpus contains `.5` sustains) and
//!   is applied once here, not re-rounded by the simulator.
//! - Same-tick order is the M4 rank: time, direction, difficulty, Press before
//!   Release. The harness then converts each event 1:1 with `audio_time_ms.ceil()`
//!   into `oxide_game::slice::InputEvent`.
//! - The scripted targets are the first qualifying notes in chart order. A
//!   target lane/window must contain no other base or scripted event (target
//!   selection fails with a diagnostic rather than skipping a negative case).
//! - The six PBOT1 threshold edges (45/46/90/91/135/136 ms) each claim a
//!   distinct free tap under that same lane/window rule. A difficulty without
//!   six free taps fails closed; the edges are not folded into 110/160/161.
//! - The big sustain drop releases one grid tick after the hit and the
//!   exact-160 boundary release keeps the frozen ADR-0018 formula
//!   `ceil(t + l - 160)`; under §6.1 REVISED its mirror snapshot is stale 161,
//!   so it penalizes (`-20.125`). A second mandatory boundary case releases at
//!   `ceil(t + l - 159)`, whose stale snapshot is bitwise 160.0 (zero penalty,
//!   no combo break). Both drops remove the synthesized release, keep the
//!   press, and must not overlap another same-lane note.
//! - The full hold keeps both synthesized events. That release is `ceil(end)`,
//!   two ticks before the completion tick (first integer `T >= end + 2`), so
//!   the next mirror still sees a positive snapshot in `(0, 1]` and emits a
//!   zero-penalty `hold_drop` rather than `hold_end` (§6.1 REVISED items 8-9).
//!   A key that stays down through that completion tick is the `hold_end` path;
//!   the crate tests cover it. Hold ticks are the fixed per-entry 0.00012/0.25
//!   rows from the hit tick through `ceil(end)`, with the observed mirror
//!   snapshot (two full rows, then the 1 ms decay) and no fractional partial.
//!   The last fractional-end snapshot is `0.5`, never negative.
//! - The ghost press is placed 1 s after the last note end, safely past every
//!   note's 160 ms hit window, in the first lane with no scheduled event.
//! - The sim runs from 0 through `ceil(last note end) + 2000 ms`, so every
//!   scripted insert and the tail ghost press are inside the trace.
//!
//! The digested trace is one compact serde JSON event per line in the §2 field
//! order; the whole synthesis → simulation → serialization → sha256 pipeline
//! runs twice from fresh state and the gate fails unless both runs are byte
//! identical. `input_time_us` is pinned to `0.0` by the frozen crate trace.
//!
//! The same synthesized stream is exported by [`export_stream`] for the
//! separate `native_note_reference` gate (`tools/oracle/NOTE-BEHAVIOR.md` §4):
//! one JSONL header plus one line per 1 ms tick carrying the exact per-tick
//! slice position and input events. The export reuses the same selection and
//! conversion path as the trace gate; it does not re-derive the recipe.
//!
//! Honesty: this gate proves deterministic simulation of the frozen rule set
//! over a synthesized stream, not native-game parity. `native_note_reference`
//! is a separate frozen-reference gate; `render_diff`/`perf_budget` are
//! separate visual-slice gates with their own fixtures.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oxide_data::Typed;
use oxide_game::slice::{GameplaySimState, InputEvent, TallyCounts, TraceEvent};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::replay::{INPUT_SOURCE, Key, LaneNote, PROVENANCE_JSON, StreamEvent};

pub(crate) const DIFFICULTIES: [&str; 3] = ["easy", "normal", "hard"];
pub(crate) const CHART_ID: &str = "bopeebo-default";
const SCHEMA_VERSION: &str = "1";
const CLI_USAGE: &str = "usage: gameplay-sim [--assets <dir>]";
const TRACE_DIR: &str = "target/gameplay-sim";
/// The only stream format the native-note probe and this harness accept.
pub(crate) const STREAM_SCHEMA: &str = "note-stream-1";
/// Provenance label for the exported per-tick position.
///
/// `oxide_game::slice` has no separate conductor object: its frozen `step`
/// argument IS the song position. The export writes that exact value so the
/// Haxe probe feeds its conductor the same per-tick music time.
pub(crate) const SONGPOS_SOURCE: &str =
    "oxide_game::slice step sim_ms (per-tick song position; no separate conductor object)";

/// Observed hit window and hold-drop threshold (docs/M5-CONTRACT.md §1).
const HIT_WINDOW_MS: f64 = 160.0;
const HOLD_DROP_THRESHOLD_MS: f64 = 160.0;
const SCORE_HOLD_DROP_PENALTY_PER_SECOND: f64 = -125.0;
const LATE_110_MS: f64 = 110.0;
const EDGE_160_MS: f64 = 160.0;
const EDGE_161_MS: f64 = 161.0;
/// Fixed per-entry hold accrual under the §6.1 regime. The real branch
/// multiplies the per-second constants by `elapsed = 0.001` s, yielding
/// exactly 0.00012 health and 0.25 score for every 1 ms tick.
const FULL_GRID_HOLD_HEALTH: f64 = 0.00012;
const FULL_GRID_HOLD_SCORE: f64 = 0.25;

/// PBOT1 threshold edges from `docs/M5-CONTRACT.md` §2. Each case needs its
/// own free tap; they are not samples of the 110/160/161 probes.
#[derive(Clone, Copy)]
struct ThresholdCase {
    name: &'static str,
    offset_ms: i32,
    judgement: &'static str,
}

const THRESHOLD_CASES: [ThresholdCase; 6] = [
    ThresholdCase {
        name: "threshold_45",
        offset_ms: 45,
        judgement: "sick",
    },
    ThresholdCase {
        name: "threshold_46",
        offset_ms: 46,
        judgement: "good",
    },
    ThresholdCase {
        name: "threshold_90",
        offset_ms: 90,
        judgement: "good",
    },
    ThresholdCase {
        name: "threshold_91",
        offset_ms: 91,
        judgement: "bad",
    },
    ThresholdCase {
        name: "threshold_135",
        offset_ms: 135,
        judgement: "bad",
    },
    ThresholdCase {
        name: "threshold_136",
        offset_ms: 136,
        judgement: "shit",
    },
];
/// The ghost press sits this far after the last note end; at least one full
/// hit window plus margin, so no note can still be `may_hit`.
const GHOST_TAIL_MS: u64 = 1000;
/// The trace continues this far after the last note end.
const END_TAIL_MS: u64 = 2000;

pub fn run(rest: &[String]) -> ExitCode {
    let cli = match Cli::parse(rest) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("gameplay-sim: {message}");
            return ExitCode::from(2);
        }
    };
    if !cli.assets_root.is_dir() {
        eprintln!(
            "gameplay-sim: prerequisite: assets directory is missing or not a directory: {}",
            cli.assets_root.display()
        );
        return ExitCode::FAILURE;
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let trace_dir = workspace.join(TRACE_DIR);
    if let Err(error) = fs::create_dir_all(&trace_dir) {
        eprintln!(
            "gameplay-sim: cannot create trace directory {}: {error}",
            trace_dir.display()
        );
        return ExitCode::FAILURE;
    }
    // Normalize the `..` in the workspace-relative path for the printed report.
    let trace_dir = trace_dir.canonicalize().unwrap_or(trace_dir);

    println!(
        "gameplay-sim: assets {}; schema {SCHEMA_VERSION:?}; difficulties {}",
        cli.assets_root.display(),
        DIFFICULTIES.join(", ")
    );
    let mut failed = Vec::new();
    for difficulty in DIFFICULTIES {
        match run_difficulty(&cli.assets_root, difficulty, &trace_dir) {
            Ok(report) => print_report(&report),
            Err(error) => {
                eprintln!("gameplay-sim: {difficulty}: {error}");
                failed.push(difficulty);
            }
        }
    }

    println!(
        "gameplay-sim: input source: {INPUT_SOURCE}; scripted negative phase: \
         threshold edges 45/46/90/91/135/136, 110-late, 160/161 full path, ghost, \
         big drop, two-case boundary (stale 161 penalizing at ceil(t+l-160), stale exactly \
         160.0 zero-penalty at ceil(t+l-159)), full-hold tick rates"
    );
    println!(
        "gameplay-sim: trace digest covers compact schema-{SCHEMA_VERSION} JSONL with \
         input_time_us = 0.0; not native-game parity; native_note_reference is the separate \
         frozen native note-row gate; render_diff/perf_budget remain pending"
    );
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "gameplay-sim: gate failed for {} of {} difficulties",
            failed.len(),
            DIFFICULTIES.len()
        );
        ExitCode::FAILURE
    }
}

struct Cli {
    assets_root: PathBuf,
}

impl Cli {
    fn parse(rest: &[String]) -> Result<Self, String> {
        let mut assets = None;
        let mut iter = rest.iter();
        while let Some(flag) = iter.next() {
            match flag.as_str() {
                "--assets" => {
                    let Some(value) = iter.next().filter(|value| !value.is_empty()) else {
                        return Err(format!("--assets requires a value; {CLI_USAGE}"));
                    };
                    assets = Some(value.clone());
                }
                other => {
                    return Err(format!("unsupported argument {other:?}; {CLI_USAGE}"));
                }
            }
        }
        let assets_root = match assets {
            Some(directory) => PathBuf::from(directory),
            None => match std::env::var("OXIDE_ASSETS_ROOT") {
                Ok(root) => PathBuf::from(root),
                Err(_) => {
                    return Err(
                        "missing --assets <dir> or OXIDE_ASSETS_ROOT: real chart assets must \
                         be provided explicitly"
                            .into(),
                    );
                }
            },
        };
        Ok(Self { assets_root })
    }
}

struct DifficultyReport {
    difficulty: &'static str,
    trace_path: PathBuf,
    first_digest: String,
    second_digest: String,
    pass: PassOutput,
}

fn print_report(report: &DifficultyReport) {
    let pass = &report.pass;
    let stats = &pass.stats;
    let counts = &pass.counts;
    println!(
        "gameplay-sim: {}: notes {} (taps {}, sustains {}, non-integer sustains {}; \
         sustain release grid = ceil(t + l)); inputs {}; end {} ms; trace events {}",
        report.difficulty,
        stats.notes,
        stats.taps,
        stats.sustains,
        stats.non_integer_sustains,
        pass.inputs,
        pass.end_ms,
        pass.event_count
    );
    println!(
        "gameplay-sim: {}: kinds hit {} miss {} ghost_press {} hold_start {} hold_tick {} \
         hold_drop {} hold_end {} game_over {}",
        report.difficulty,
        counts.hit,
        counts.miss,
        counts.ghost_press,
        counts.hold_start,
        counts.hold_tick,
        counts.hold_drop,
        counts.hold_end,
        counts.game_over
    );
    for (name, detail) in &pass.checks {
        println!(
            "gameplay-sim: {}: check {name}: PASS ({detail})",
            report.difficulty
        );
    }
    println!(
        "gameplay-sim: {}: trace {} sha256 {}",
        report.difficulty,
        report.trace_path.display(),
        report.first_digest
    );
    println!(
        "gameplay-sim: {}: two-run sha256 match {} == {}",
        report.difficulty, report.first_digest, report.second_digest
    );
}

fn run_difficulty(
    assets_root: &Path,
    difficulty: &'static str,
    trace_dir: &Path,
) -> Result<DifficultyReport, String> {
    let trace_path = trace_dir.join(format!("trace-{difficulty}.jsonl"));
    // The complete pipeline runs twice from fresh state, including the trace
    // write; the file digest is compared per difficulty (the M5 §2 rule).
    let first = simulate_difficulty(assets_root, difficulty)?;
    let first_digest = write_and_hash(&trace_path, &first.trace_bytes)?;
    if first_digest != first.digest {
        return Err(format!(
            "serialized trace digest changed across the write: computed {} but wrote {}",
            first.digest, first_digest
        ));
    }
    let second = simulate_difficulty(assets_root, difficulty)?;
    let second_digest = write_and_hash(&trace_path, &second.trace_bytes)?;
    if second_digest != second.digest {
        return Err(format!(
            "serialized trace digest changed across the second write: computed {} but wrote {}",
            second.digest, second_digest
        ));
    }
    if first.trace_bytes != second.trace_bytes || first_digest != second_digest {
        return Err(format!(
            "two-run determinism failure: run 1 sha256 {first_digest}, run 2 sha256 \
             {second_digest}, byte-identical={}",
            first.trace_bytes == second.trace_bytes
        ));
    }
    Ok(DifficultyReport {
        difficulty,
        trace_path,
        first_digest,
        second_digest,
        pass: first,
    })
}

fn write_and_hash(path: &Path, bytes: &[u8]) -> Result<String, String> {
    fs::write(path, bytes)
        .map_err(|error| format!("cannot write trace {}: {error}", path.display()))?;
    let written = fs::read(path)
        .map_err(|error| format!("cannot re-read trace {}: {error}", path.display()))?;
    if written != bytes {
        return Err(format!(
            "trace {} changed between serialization and the write-back read",
            path.display()
        ));
    }
    Ok(sha_hex(&written))
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// One exported stream in the frozen `note-stream-1` format.
pub(crate) struct ExportedStream {
    pub difficulty: String,
    pub tick_count: u64,
    pub input_count: usize,
    pub digest: String,
    pub bytes: Vec<u8>,
}

/// Line 1 of the frozen stream format (`tools/oracle/NOTE-BEHAVIOR.md` §4).
#[derive(Serialize)]
struct StreamHeader<'a> {
    schema: &'a str,
    difficulty: &'a str,
    chart_id: &'a str,
    tick_count: u32,
    songpos_source: &'a str,
}

#[derive(Serialize)]
struct StreamTick {
    sim: u64,
    music_ms: f64,
    events: Vec<StreamInput>,
}

#[derive(Serialize)]
struct StreamInput {
    kind: &'static str,
    direction: i64,
    input_ms: u64,
}

/// Exports a stream from already-loaded notes.
///
/// This is the shared synthesis seam: the production gate loads provenance-
/// verified notes first, while tests supply synthetic notes over their own
/// small asset tree. The recipe itself is never re-derived here.
pub(crate) fn export_stream_from_notes(
    notes: &[LaneNote],
    difficulty: &str,
) -> Result<ExportedStream, String> {
    if !DIFFICULTIES.contains(&difficulty) {
        return Err(format!(
            "unknown difficulty {difficulty:?}; the native-note stream supports easy, normal and hard"
        ));
    }
    if let Some(note) = notes.iter().find(|note| note.difficulty != difficulty) {
        return Err(format!(
            "cannot export a {difficulty:?} stream from notes labelled {:?}; the header would lie \
             about the replay inputs",
            note.difficulty
        ));
    }
    let PreparedStream { inputs, end_ms, .. } = prepare_stream(notes)?;
    let tick_count = end_ms
        .checked_add(1)
        .ok_or_else(|| "stream tick count overflows u64".to_string())?;
    let header_ticks = u32::try_from(tick_count)
        .map_err(|_| format!("stream tick count {tick_count} does not fit the u32 header field"))?;

    let mut bytes = Vec::new();
    serde_json::to_writer(
        &mut bytes,
        &StreamHeader {
            schema: STREAM_SCHEMA,
            difficulty,
            chart_id: CHART_ID,
            tick_count: header_ticks,
            songpos_source: SONGPOS_SOURCE,
        },
    )
    .map_err(|error| format!("cannot serialize stream header for {difficulty}: {error}"))?;
    bytes.push(b'\n');

    let mut next_input = 0usize;
    for sim in 0..=end_ms {
        let mut events = Vec::new();
        while let Some(input) = inputs.get(next_input) {
            if input_sim(input) != sim {
                break;
            }
            let (kind, direction, input_ms) = match input {
                InputEvent::Press { sim_ms, direction } => ("press", *direction, *sim_ms),
                InputEvent::Release { sim_ms, direction } => ("release", *direction, *sim_ms),
            };
            events.push(StreamInput {
                kind,
                direction,
                input_ms,
            });
            next_input += 1;
        }
        serde_json::to_writer(
            &mut bytes,
            &StreamTick {
                sim,
                // The slice's `step(sim_ms, ...)` argument is the song
                // position; there is no separate conductor state to read.
                music_ms: sim as f64,
                events,
            },
        )
        .map_err(|error| format!("cannot serialize stream tick {sim} for {difficulty}: {error}"))?;
        bytes.push(b'\n');
    }
    if next_input != inputs.len() {
        return Err(format!(
            "{} input event(s) remain after the exported {difficulty} stream end",
            inputs.len() - next_input
        ));
    }
    let digest = sha_hex(&bytes);
    Ok(ExportedStream {
        difficulty: difficulty.to_string(),
        tick_count,
        input_count: inputs.len(),
        digest,
        bytes,
    })
}

/// One fresh pipeline pass over the frozen bopeebo chart.
fn simulate_difficulty(assets_root: &Path, difficulty: &str) -> Result<PassOutput, String> {
    let notes = load_verified_notes(assets_root, difficulty)?;
    simulate_with_notes(assets_root, difficulty, &notes)
}

/// Loads the measured bopeebo chart and this difficulty's player notes in
/// chart order (the order the slice sim uses for `note_id`). The chart and its
/// metadata must match the frozen M4 provenance inventory; arbitrary mods or a
/// drifted checkout are rejected rather than silently accepted as the corpus.
pub(crate) fn load_verified_notes(
    assets_root: &Path,
    difficulty: &str,
) -> Result<Vec<LaneNote>, String> {
    let inventory = provenance_inventory()?;
    let chart_provenance_path =
        crate::replay::resolve_chart_path(CHART_ID, &inventory).map_err(|error| error.0)?;
    let metadata_provenance_path = chart_provenance_path
        .strip_suffix("-chart.json")
        .map(|prefix| format!("{prefix}-metadata.json"))
        .ok_or_else(|| {
            format!(
                "provenance chart path {chart_provenance_path:?} does not end in -chart.json; \
                 cannot derive the metadata path"
            )
        })?;
    let chart_expected = inventory
        .get(chart_provenance_path)
        .ok_or_else(|| format!("provenance lost the hash of {chart_provenance_path:?}"))?;
    let metadata_expected = inventory
        .get(&metadata_provenance_path)
        .ok_or_else(|| format!("provenance lost the hash of {metadata_provenance_path:?}"))?;
    let chart_bytes = read_verified_asset(assets_root, chart_provenance_path, chart_expected)?;
    // The slice sim reads the metadata for difficulty validation; bind its
    // bytes to the measured corpus too even though this harness does not parse
    // those bytes itself.
    let _metadata_bytes =
        read_verified_asset(assets_root, &metadata_provenance_path, metadata_expected)?;
    parse_notes(&chart_bytes, chart_provenance_path, difficulty)
}

fn provenance_inventory() -> Result<BTreeMap<String, String>, String> {
    let raw = serde_json::from_str::<serde_json::Value>(PROVENANCE_JSON)
        .map_err(|error| format!("embedded provenance.json is unparseable: {error}"))?;
    crate::replay::inventory_map(&raw).map_err(|error| error.0)
}

fn read_verified_asset(
    assets_root: &Path,
    provenance_path: &str,
    expected_sha256: &str,
) -> Result<Vec<u8>, String> {
    // Provenance paths are game-checkout relative (`assets/preload/...`); the
    // caller supplies the assets directory itself, matching the M4 convention.
    let relative = provenance_path
        .strip_prefix("assets/")
        .unwrap_or(provenance_path);
    let file = assets_root.join(relative);
    let bytes = fs::read(&file).map_err(|error| {
        format!(
            "cannot read asset {} (real game assets required): {error}",
            file.display()
        )
    })?;
    let actual = sha_hex(&bytes);
    if actual != expected_sha256 {
        return Err(format!(
            "asset {} sha256 {actual} differs from the frozen provenance {expected_sha256}; \
             refusing to simulate outside the measured corpus",
            file.display()
        ));
    }
    Ok(bytes)
}

fn parse_notes(
    chart_bytes: &[u8],
    chart_path: &str,
    difficulty: &str,
) -> Result<Vec<LaneNote>, String> {
    let parsed = oxide_data::parse_file(Path::new(chart_path), chart_bytes)
        .map_err(|error| format!("chart parse error for {chart_path}: {error}"))?;
    let Typed::Chart(chart) = &parsed.typed else {
        return Err(format!(
            "{chart_path} was typed {:?}; a V-Slice chart was required",
            parsed.kind
        ));
    };
    let chart_notes = chart.notes.get(difficulty).ok_or_else(|| {
        format!("chart {chart_path} has no {difficulty:?} difficulty; the M5 slice needs easy, normal and hard")
    })?;
    let mut notes = Vec::new();
    for (index, note) in chart_notes.iter().enumerate() {
        if !(0..=7).contains(&note.direction) {
            return Err(format!(
                "chart {chart_path} has an invalid note at index {index}: direction {} is \
                 outside the V-Slice 0..=7 lane range",
                note.direction
            ));
        }
        if !(0..=3).contains(&note.direction) {
            // Opponent lanes are outside the phase-1 player trace.
            continue;
        }
        if !note.time.is_finite() || note.time < 0.0 {
            return Err(format!(
                "chart {chart_path} has an invalid note at index {index}: time {} must be \
                 finite and non-negative",
                note.time
            ));
        }
        let hold_ms = note.sustain.unwrap_or(0.0);
        if !hold_ms.is_finite() || hold_ms < 0.0 {
            return Err(format!(
                "chart {chart_path} has an invalid note at index {index}: sustain {hold_ms} \
                 must be finite and non-negative"
            ));
        }
        notes.push(LaneNote {
            time_ms: note.time,
            direction: note.direction,
            difficulty: difficulty.to_string(),
            hold_ms,
        });
    }
    if notes.is_empty() {
        return Err(format!(
            "chart {chart_path} {difficulty:?} holds no player notes; the scripted negative \
             phase cannot be constructed"
        ));
    }
    Ok(notes)
}

#[derive(Debug)]
struct PassOutput {
    trace_bytes: Vec<u8>,
    digest: String,
    counts: KindCounts,
    inputs: usize,
    event_count: usize,
    checks: Vec<(&'static str, String)>,
    stats: NoteStats,
    end_ms: u64,
}

/// The scripted synthesis shared by the trace gate and the stream export.
struct PreparedStream {
    script: Script,
    inputs: Vec<InputEvent>,
    end_ms: u64,
}

/// One canonical 1 ms tick: the inputs dispatched into it and the trace events
/// it emitted. The visual-slice gates replay these ticks into the renderer in
/// the same order the gameplay gate consumed them.
pub(crate) struct CanonicalTick {
    pub(crate) sim_ms: u64,
    pub(crate) inputs: Vec<InputEvent>,
    pub(crate) events: Vec<TraceEvent>,
}

/// The shared canonical replay used by `gameplay-sim` and the phase-2b gates.
///
/// The `script` stays private to this module: `gameplay-sim` uses it for the
/// negative-phase checks while render-diff only needs the tick stream and the
/// canonical trace digest.
pub(crate) struct CanonicalReplay {
    pub(crate) ticks: Vec<CanonicalTick>,
    pub(crate) trace_bytes: Vec<u8>,
    pub(crate) digest: String,
    script: Script,
}

/// Synthesizes the frozen scripted stream and converts it to the 1 ms grid.
///
/// This is the single synthesis path used by both the gameplay trace gate and
/// the native-note stream export; callers must not re-derive the recipe.
fn prepare_stream(notes: &[LaneNote]) -> Result<PreparedStream, String> {
    let script = select_script(notes)?;
    let stream = build_scripted_stream(notes, &script)?;
    let inputs = to_input_events(&stream)?;
    let last_end_ms = grid_ms(
        notes
            .iter()
            .map(|note| note.time_ms + note.hold_ms)
            .fold(0.0_f64, f64::max),
    )?;
    let end_ms = last_end_ms
        .checked_add(END_TAIL_MS)
        .ok_or_else(|| "simulation end overflows u64 milliseconds".to_string())?;
    if script.ghost_sim > end_ms {
        return Err(format!(
            "ghost press at {} ms lies after the {} ms simulation end",
            script.ghost_sim, end_ms
        ));
    }
    Ok(PreparedStream {
        script,
        inputs,
        end_ms,
    })
}

/// Replays the canonical 1 ms input stream through the frozen slice sim.
///
/// This is the single simulation path used by `gameplay-sim`, the
/// `render-diff` two-run trace check and the `perf-budget` scene driver; the
/// stream synthesis is not re-derived by callers.
pub(crate) fn canonical_replay(
    assets_root: &Path,
    difficulty: &str,
    notes: &[LaneNote],
) -> Result<CanonicalReplay, String> {
    let PreparedStream {
        script,
        inputs,
        end_ms,
    } = prepare_stream(notes)?;

    let mut sim = GameplaySimState::build(assets_root, difficulty)
        .map_err(|error| format!("failed to build the bopeebo {difficulty} sim: {error}"))?;
    let mut ticks = Vec::new();
    let mut trace = Vec::new();
    let mut next_input = 0usize;
    for sim_ms in 0..=end_ms {
        let mut tick_inputs = Vec::new();
        while let Some(input) = inputs.get(next_input) {
            if input_sim(input) != sim_ms {
                break;
            }
            tick_inputs.push(input.clone());
            next_input += 1;
        }
        let events = sim.step(sim_ms, &tick_inputs);
        trace.extend(events.iter().cloned());
        ticks.push(CanonicalTick {
            sim_ms,
            inputs: tick_inputs,
            events,
        });
    }
    if next_input != inputs.len() {
        return Err(format!(
            "{} input event(s) remain after the {} ms simulation end",
            inputs.len() - next_input,
            end_ms
        ));
    }
    let recorded = sim.record();
    if recorded != trace {
        return Err(
            "sim.record() differs from the per-tick dispatched trace; refusing a mixed trace"
                .into(),
        );
    }
    let trace_bytes = serialize_trace(&trace)?;
    let digest = sha_hex(&trace_bytes);
    Ok(CanonicalReplay {
        ticks,
        trace_bytes,
        digest,
        script,
    })
}

fn simulate_with_notes(
    assets_root: &Path,
    difficulty: &str,
    notes: &[LaneNote],
) -> Result<PassOutput, String> {
    let replay = canonical_replay(assets_root, difficulty, notes)?;
    let trace: Vec<TraceEvent> = replay
        .ticks
        .iter()
        .flat_map(|tick| tick.events.iter().cloned())
        .collect();
    let checks = validate_expectations(&trace, &replay.script)?;
    let counts = count_kinds(&trace);
    let event_count = trace.len();
    let inputs = replay.ticks.iter().map(|tick| tick.inputs.len()).sum();
    let end_ms = replay.ticks.last().map_or(0, |tick| tick.sim_ms);
    Ok(PassOutput {
        trace_bytes: replay.trace_bytes,
        digest: replay.digest,
        counts,
        inputs,
        event_count,
        checks,
        stats: note_stats(notes),
        end_ms,
    })
}

#[derive(Debug, Clone, Copy)]
struct NoteStats {
    notes: usize,
    taps: usize,
    sustains: usize,
    non_integer_sustains: usize,
}

fn note_stats(notes: &[LaneNote]) -> NoteStats {
    let taps = notes.iter().filter(|note| note.hold_ms == 0.0).count();
    NoteStats {
        notes: notes.len(),
        taps,
        sustains: notes.len() - taps,
        non_integer_sustains: notes
            .iter()
            .filter(|note| note.hold_ms.fract() != 0.0)
            .count(),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct KindCounts {
    hit: usize,
    miss: usize,
    ghost_press: usize,
    hold_start: usize,
    hold_tick: usize,
    hold_drop: usize,
    hold_end: usize,
    game_over: usize,
}

fn count_kinds(trace: &[TraceEvent]) -> KindCounts {
    let mut counts = KindCounts::default();
    for event in trace {
        match event {
            TraceEvent::Hit { .. } => counts.hit += 1,
            TraceEvent::Miss { .. } => counts.miss += 1,
            TraceEvent::GhostPress { .. } => counts.ghost_press += 1,
            TraceEvent::HoldStart { .. } => counts.hold_start += 1,
            TraceEvent::HoldTick { .. } => counts.hold_tick += 1,
            TraceEvent::HoldDrop { .. } => counts.hold_drop += 1,
            TraceEvent::HoldEnd { .. } => counts.hold_end += 1,
            TraceEvent::GameOver { .. } => counts.game_over += 1,
        }
    }
    counts
}

fn serialize_trace(trace: &[TraceEvent]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    for (index, event) in trace.iter().enumerate() {
        if input_time_us(event) != 0.0 {
            return Err(format!(
                "trace event {index} carries input_time_us {}; the canonical digest requires 0.0",
                input_time_us(event)
            ));
        }
        serde_json::to_writer(&mut bytes, event)
            .map_err(|error| format!("cannot serialize trace event {index}: {error}"))?;
        bytes.push(b'\n');
    }
    Ok(bytes)
}

fn input_time_us(event: &TraceEvent) -> f64 {
    match event {
        TraceEvent::Hit { input_time_us, .. }
        | TraceEvent::Miss { input_time_us, .. }
        | TraceEvent::GhostPress { input_time_us, .. }
        | TraceEvent::HoldStart { input_time_us, .. }
        | TraceEvent::HoldTick { input_time_us, .. }
        | TraceEvent::HoldDrop { input_time_us, .. }
        | TraceEvent::HoldEnd { input_time_us, .. }
        | TraceEvent::GameOver { input_time_us, .. } => *input_time_us,
    }
}

fn input_sim(input: &InputEvent) -> u64 {
    match input {
        InputEvent::Press { sim_ms, .. } | InputEvent::Release { sim_ms, .. } => *sim_ms,
    }
}

/// The frozen M4 grid conversion: every audio timestamp rounds up to the next
/// integer millisecond. Fractional sustain releases therefore become
/// `ceil(note.t + note.l)`, not `ceil(note.t) + note.l`.
pub(crate) fn grid_ms(value_ms: f64) -> Result<u64, String> {
    if !value_ms.is_finite() {
        return Err(format!("non-finite timestamp {value_ms}"));
    }
    if value_ms < 0.0 {
        return Err(format!(
            "negative timestamp {value_ms} cannot sit on the 1 ms grid"
        ));
    }
    let rounded = value_ms.ceil();
    // `u64::MAX as f64` is 2^64; values at or past it cannot be represented as
    // a u64 millisecond without saturation, so fail instead of clamping.
    if rounded >= u64::MAX as f64 {
        return Err(format!(
            "timestamp {value_ms} exceeds the u64 millisecond grid"
        ));
    }
    Ok(rounded as u64)
}

fn press_audio(note: &LaneNote) -> f64 {
    note.time_ms.ceil()
}

fn release_audio(note: &LaneNote) -> f64 {
    if note.hold_ms > 0.0 {
        note.time_ms + note.hold_ms
    } else {
        press_audio(note)
    }
}

fn to_input_events(stream: &[StreamEvent]) -> Result<Vec<InputEvent>, String> {
    stream
        .iter()
        .map(|event| {
            let sim_ms = grid_ms(event.audio_time_ms).map_err(|error| {
                format!(
                    "stream event at {} ms (direction {}): {error}",
                    event.audio_time_ms, event.direction
                )
            })?;
            Ok(match event.state {
                Key::Down => InputEvent::Press {
                    sim_ms,
                    direction: event.direction,
                },
                Key::Up => InputEvent::Release {
                    sim_ms,
                    direction: event.direction,
                },
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct ScriptedTarget {
    note_index: usize,
    note_id: u32,
    lane: i64,
    press_sim: u64,
}

#[derive(Debug, Clone, Copy)]
struct HoldTarget {
    note_index: usize,
    note_id: u32,
    lane: i64,
    press_sim: u64,
    /// Release/flag tick (the grid release for a scripted drop; `ceil(end)`
    /// for a full hold). Drop rows are emitted on this tick + 1 under the
    /// §6.1 REVISED mirror (`release+1`), while hold_end is two ticks after
    /// the last accrual.
    release_sim: u64,
    /// Base-note strum time, used by the mirror's `P > strumTime` guard and
    /// the stale-snapshot formula.
    strum_time_ms: f64,
    /// Chart sustain end, `note.t + note.l`, not the grid release.
    end_ms: f64,
}

#[derive(Debug, Clone, Copy)]
struct Script {
    thresholds: [ScriptedTarget; 6],
    late_110: ScriptedTarget,
    edge_160: ScriptedTarget,
    edge_161: ScriptedTarget,
    big_drop: HoldTarget,
    /// ADR-0018 release `ceil(t+l-160)`: its mirror snapshot is 161 ms, so
    /// the real branch penalizes.
    boundary_160: HoldTarget,
    /// New no-penalty boundary release `ceil(t+l-159)`: its mirror snapshot
    /// is exactly 160.0.
    boundary_exact_160: HoldTarget,
    full_hold: HoldTarget,
    ghost_sim: u64,
    ghost_direction: i64,
}

#[derive(Debug, Clone, Copy)]
struct LaneInterval {
    lane: i64,
    from: u64,
    to: u64,
}

fn select_script(notes: &[LaneNote]) -> Result<Script, String> {
    let mut used = BTreeSet::new();
    let mut scheduled = Vec::new();

    let mut thresholds = Vec::with_capacity(THRESHOLD_CASES.len());
    for case in THRESHOLD_CASES {
        let label = format!("{} ms threshold edge ({})", case.offset_ms, case.judgement);
        let target = select_tap(notes, &used, &scheduled, &label, f64::from(case.offset_ms))
            .map_err(|error| {
                format!(
                    "threshold edges require six distinct free tap notes (45 sick, 46 good, \
                 90 good, 91 bad, 135 bad, 136 shit); claimed {} of 6 before the {} ms \
                 edge failed: {error}",
                    thresholds.len(),
                    case.offset_ms
                )
            })?;
        claim_tap(&target, &mut used, &mut scheduled);
        thresholds.push(target);
    }
    let thresholds = <[ScriptedTarget; 6]>::try_from(thresholds)
        .map_err(|_| "internal: threshold selection did not return six targets".to_string())?;

    let late_110 = select_tap(notes, &used, &scheduled, "110 ms-late", LATE_110_MS)?;
    claim_tap(&late_110, &mut used, &mut scheduled);

    let edge_160 = select_tap(notes, &used, &scheduled, "160 ms-edge", EDGE_160_MS)?;
    claim_tap(&edge_160, &mut used, &mut scheduled);

    let edge_161 = select_tap(notes, &used, &scheduled, "161 ms-edge", EDGE_161_MS)?;
    claim_tap(&edge_161, &mut used, &mut scheduled);

    let big_drop = select_big_drop(notes, &used, &scheduled)?;
    used.insert(big_drop.note_index);
    scheduled.push(LaneInterval {
        lane: big_drop.lane,
        from: big_drop.press_sim,
        to: big_drop.release_sim,
    });

    let boundary_160 = select_boundary_drop(
        notes,
        &used,
        &scheduled,
        160.0,
        161.0,
        "stale 161 penalizing",
    )?;
    used.insert(boundary_160.note_index);
    scheduled.push(LaneInterval {
        lane: boundary_160.lane,
        from: boundary_160.press_sim,
        to: boundary_160.release_sim,
    });

    let boundary_exact_160 = select_boundary_drop(
        notes,
        &used,
        &scheduled,
        159.0,
        160.0,
        "stale exactly 160.0 zero-penalty",
    )?;
    used.insert(boundary_exact_160.note_index);
    scheduled.push(LaneInterval {
        lane: boundary_exact_160.lane,
        from: boundary_exact_160.press_sim,
        to: boundary_exact_160.release_sim,
    });

    let full_hold = select_full_hold(notes, &used, &scheduled)?;
    scheduled.push(LaneInterval {
        lane: full_hold.lane,
        from: full_hold.press_sim,
        to: full_hold.release_sim,
    });

    let last_end_ms = grid_ms(
        notes
            .iter()
            .map(|note| note.time_ms + note.hold_ms)
            .fold(0.0_f64, f64::max),
    )?;
    let ghost_sim = last_end_ms
        .checked_add(GHOST_TAIL_MS)
        .ok_or_else(|| "ghost press timestamp overflows u64 milliseconds".to_string())?;
    let max_note_time = notes
        .iter()
        .map(|note| note.time_ms)
        .fold(0.0_f64, f64::max);
    if (ghost_sim as f64) <= max_note_time + HIT_WINDOW_MS {
        return Err(format!(
            "ghost press at {ghost_sim} ms is not safely past the last hit window \
             (last note {max_note_time} ms)"
        ));
    }
    let ghost_direction = (0..=3)
        .find(|direction| {
            lane_interval_free(
                notes,
                *direction,
                ghost_sim,
                ghost_sim,
                usize::MAX,
                &scheduled,
            )
        })
        .ok_or_else(|| {
            format!("no player lane is free of scheduled events at the ghost tick {ghost_sim} ms")
        })?;

    Ok(Script {
        thresholds,
        late_110,
        edge_160,
        edge_161,
        big_drop,
        boundary_160,
        boundary_exact_160,
        full_hold,
        ghost_sim,
        ghost_direction,
    })
}

fn claim_tap(
    target: &ScriptedTarget,
    used: &mut BTreeSet<usize>,
    scheduled: &mut Vec<LaneInterval>,
) {
    used.insert(target.note_index);
    scheduled.push(LaneInterval {
        lane: target.lane,
        from: target.press_sim,
        to: target.press_sim,
    });
}

fn select_tap(
    notes: &[LaneNote],
    used: &BTreeSet<usize>,
    scheduled: &[LaneInterval],
    label: &str,
    offset_ms: f64,
) -> Result<ScriptedTarget, String> {
    let mut unused_taps = 0usize;
    let mut lane_blocked = 0usize;
    for (index, note) in notes.iter().enumerate() {
        if note.hold_ms != 0.0 {
            continue;
        }
        if used.contains(&index) {
            continue;
        }
        unused_taps += 1;
        let press_sim = grid_ms(note.time_ms + offset_ms)?;
        let from = grid_ms((note.time_ms - HIT_WINDOW_MS).max(0.0))?;
        if !lane_interval_free(notes, note.direction, from, press_sim, index, scheduled) {
            lane_blocked += 1;
            continue;
        }
        return Ok(ScriptedTarget {
            note_index: index,
            note_id: index as u32,
            lane: note.direction,
            press_sim,
        });
    }
    Err(format!(
        "no {label} tap-note target (press at note.t + {offset_ms} ms) satisfies the \
         lane/window preconditions; unused taps={unused_taps}, lane/window blocked=\
         {lane_blocked}; the negative acceptance phase refuses to skip"
    ))
}

fn select_big_drop(
    notes: &[LaneNote],
    used: &BTreeSet<usize>,
    scheduled: &[LaneInterval],
) -> Result<HoldTarget, String> {
    for (index, note) in notes.iter().enumerate() {
        if used.contains(&index) || note.hold_ms <= 0.0 {
            continue;
        }
        let press_sim = grid_ms(note.time_ms)?;
        let release_sim = press_sim
            .checked_add(1)
            .ok_or_else(|| "early sustain release overflows u64 milliseconds".to_string())?;
        let end_ms = note.time_ms + note.hold_ms;
        let stale_ms = stale_snapshot(end_ms, note.time_ms, release_sim);
        if stale_ms <= HOLD_DROP_THRESHOLD_MS || (release_sim as f64) >= end_ms {
            continue;
        }
        if !lane_interval_free(
            notes,
            note.direction,
            press_sim,
            release_sim,
            index,
            scheduled,
        ) {
            continue;
        }
        return Ok(HoldTarget {
            note_index: index,
            note_id: index as u32,
            lane: note.direction,
            press_sim,
            release_sim,
            strum_time_ms: note.time_ms,
            end_ms,
        });
    }
    Err(
        "no sustain note can be released with a stale mirror snapshot > 160 while its lane \
         stays free; the negative acceptance phase refuses to skip"
            .into(),
    )
}

/// Selects a sustain for one of the two §6.1 REVISED boundary cases.
///
/// `offset_ms` is the frozen release formula `ceil(t + l - offset_ms)`
/// (`160` for the ADR-0018 penalizing case, `159` for the new zero-penalty
/// case). The mirror at `release_sim + 1` sees the snapshot last set at
/// `release_sim`; when `release_sim - 1 > strumTime` that snapshot is
/// `end - (release_sim - 1)`, otherwise it is still the hit capture `full`.
/// The selected note must produce `expected_stale_ms` bitwise.
fn select_boundary_drop(
    notes: &[LaneNote],
    used: &BTreeSet<usize>,
    scheduled: &[LaneInterval],
    offset_ms: f64,
    expected_stale_ms: f64,
    label: &str,
) -> Result<HoldTarget, String> {
    let mut stale_matches = 0usize;
    let mut lane_blocked = 0usize;
    for (index, note) in notes.iter().enumerate() {
        if used.contains(&index) || note.hold_ms <= 0.0 {
            continue;
        }
        let press_sim = grid_ms(note.time_ms)?;
        let end_ms = note.time_ms + note.hold_ms;
        let release_target = end_ms - offset_ms;
        if release_target < 0.0 {
            continue;
        }
        let release_sim = grid_ms(release_target)?;
        if release_sim <= press_sim || (release_sim as f64) >= end_ms {
            continue;
        }
        let stale_ms = stale_snapshot(end_ms, note.time_ms, release_sim);
        if !same_f64(stale_ms, expected_stale_ms) {
            continue;
        }
        stale_matches += 1;
        if !lane_interval_free(
            notes,
            note.direction,
            press_sim,
            release_sim,
            index,
            scheduled,
        ) {
            lane_blocked += 1;
            continue;
        }
        return Ok(HoldTarget {
            note_index: index,
            note_id: index as u32,
            lane: note.direction,
            press_sim,
            release_sim,
            strum_time_ms: note.time_ms,
            end_ms,
        });
    }
    Err(format!(
        "no lane-free sustain produces the {label} boundary case (release at \
         ceil(t + l - {offset_ms}), stale mirror snapshot {expected_stale_ms}); \
         stale-matching candidates: {stale_matches}, of which {lane_blocked} were blocked by a \
         same-lane note or an earlier scripted interval; both §6.1 REVISED boundary cases are \
         mandatory and the negative acceptance phase refuses to skip"
    ))
}

/// Snapshot observed by the mirror at `release_sim + 1`.
///
/// The snapshot is last set at frame `release_sim`: the recompute there uses
/// `P = release_sim - 1` when that is strictly past the strum time; otherwise
/// the hold still carries the hit capture (`full`).
fn stale_snapshot(end_ms: f64, strum_time_ms: f64, release_sim: u64) -> f64 {
    let mirror_position = release_sim as f64 - 1.0;
    if mirror_position > strum_time_ms {
        end_ms - mirror_position
    } else {
        end_ms - strum_time_ms
    }
}

fn same_f64(left: f64, right: f64) -> bool {
    left.to_bits() == right.to_bits()
}

fn select_full_hold(
    notes: &[LaneNote],
    used: &BTreeSet<usize>,
    scheduled: &[LaneInterval],
) -> Result<HoldTarget, String> {
    for (index, note) in notes.iter().enumerate() {
        if used.contains(&index) || note.hold_ms <= 0.0 {
            continue;
        }
        let press_sim = grid_ms(note.time_ms)?;
        let end_ms = note.time_ms + note.hold_ms;
        let release_sim = grid_ms(end_ms)?;
        if release_sim <= press_sim {
            continue;
        }
        if !lane_interval_free(
            notes,
            note.direction,
            press_sim,
            release_sim,
            index,
            scheduled,
        ) {
            continue;
        }
        return Ok(HoldTarget {
            note_index: index,
            note_id: index as u32,
            lane: note.direction,
            press_sim,
            release_sim,
            strum_time_ms: note.time_ms,
            end_ms,
        });
    }
    Err(
        "no sustain note can be held to its full length while its lane stays free; the \
         negative acceptance phase refuses to skip"
            .into(),
    )
}

fn lane_interval_free(
    notes: &[LaneNote],
    lane: i64,
    from: u64,
    to: u64,
    exclude_index: usize,
    scheduled: &[LaneInterval],
) -> bool {
    for (index, note) in notes.iter().enumerate() {
        if index == exclude_index || note.direction != lane {
            continue;
        }
        let Ok(press) = grid_ms(note.time_ms) else {
            return false;
        };
        let Ok(release) = grid_ms(release_audio(note)) else {
            return false;
        };
        if intervals_overlap(from, to, press, release) {
            return false;
        }
    }
    !scheduled
        .iter()
        .filter(|interval| interval.lane == lane)
        .any(|interval| intervals_overlap(from, to, interval.from, interval.to))
}

fn intervals_overlap(a_from: u64, a_to: u64, b_from: u64, b_to: u64) -> bool {
    a_from <= b_to && b_from <= a_to
}

fn scripted_taps(script: &Script) -> Vec<&ScriptedTarget> {
    script
        .thresholds
        .iter()
        .chain([&script.late_110, &script.edge_160, &script.edge_161])
        .collect()
}

fn build_scripted_stream(notes: &[LaneNote], script: &Script) -> Result<Vec<StreamEvent>, String> {
    let taps = scripted_taps(script);
    let mut omitted = BTreeSet::new();
    for target in &taps {
        omitted.insert(target.note_index);
    }
    let difficulty = notes
        .first()
        .map(|note| note.difficulty.clone())
        .ok_or_else(|| "cannot build a stream for an empty note list".to_string())?;
    let mut events = Vec::new();
    for (index, note) in notes.iter().enumerate() {
        if omitted.contains(&index) {
            continue;
        }
        let press = press_audio(note);
        events.push(StreamEvent {
            audio_time_ms: press,
            direction: note.direction,
            difficulty: difficulty.clone(),
            state: Key::Down,
        });
        let release = if index == script.big_drop.note_index {
            script.big_drop.release_sim as f64
        } else if index == script.boundary_160.note_index {
            script.boundary_160.release_sim as f64
        } else if index == script.boundary_exact_160.note_index {
            script.boundary_exact_160.release_sim as f64
        } else {
            release_audio(note)
        };
        events.push(StreamEvent {
            audio_time_ms: release,
            direction: note.direction,
            difficulty: difficulty.clone(),
            state: Key::Up,
        });
    }
    for target in taps {
        let time = target.press_sim as f64;
        events.push(StreamEvent {
            audio_time_ms: time,
            direction: target.lane,
            difficulty: difficulty.clone(),
            state: Key::Down,
        });
        events.push(StreamEvent {
            audio_time_ms: time,
            direction: target.lane,
            difficulty: difficulty.clone(),
            state: Key::Up,
        });
    }
    events.push(StreamEvent {
        audio_time_ms: script.ghost_sim as f64,
        direction: script.ghost_direction,
        difficulty: difficulty.clone(),
        state: Key::Down,
    });
    events.push(StreamEvent {
        audio_time_ms: script.ghost_sim as f64,
        direction: script.ghost_direction,
        difficulty,
        state: Key::Up,
    });
    sort_stream(&mut events);
    Ok(events)
}

fn sort_stream(events: &mut [StreamEvent]) {
    events.sort_by(|a, b| {
        a.audio_time_ms
            .total_cmp(&b.audio_time_ms)
            .then_with(|| a.direction.cmp(&b.direction))
            .then_with(|| a.difficulty.cmp(&b.difficulty))
            .then_with(|| state_rank(a.state).cmp(&state_rank(b.state)))
    });
}

fn state_rank(state: Key) -> u8 {
    match state {
        Key::Down => 0,
        Key::Up => 1,
    }
}

fn validate_expectations(
    trace: &[TraceEvent],
    script: &Script,
) -> Result<Vec<(&'static str, String)>, String> {
    let mut checks = Vec::with_capacity(THRESHOLD_CASES.len() + 8);
    for (case, target) in THRESHOLD_CASES.iter().zip(script.thresholds.iter()) {
        let detail = check_threshold(trace, target, case)
            .map_err(|error| format!("{} check: {error}", case.name))?;
        checks.push((case.name, detail));
    }
    let late = check_late_110(trace, &script.late_110)
        .map_err(|error| format!("late_110 check: {error}"))?;
    let edge_160 = check_edge_160(trace, &script.edge_160)
        .map_err(|error| format!("edge_160 check: {error}"))?;
    let edge_161 = check_edge_161(trace, &script.edge_161)
        .map_err(|error| format!("edge_161 check: {error}"))?;
    let ghost =
        check_ghost(trace, script).map_err(|error| format!("ghost_press check: {error}"))?;
    check_hold_inventory(trace, script).map_err(|error| format!("holds check: {error}"))?;
    let big_drop =
        check_big_drop(trace, script).map_err(|error| format!("hold_big_drop check: {error}"))?;
    let boundary = check_boundary_drop(
        trace,
        &script.boundary_160,
        "hold_boundary_160",
        161.0,
        true,
    )
    .map_err(|error| format!("hold_boundary_160 check: {error}"))?;
    let boundary_exact = check_boundary_drop(
        trace,
        &script.boundary_exact_160,
        "hold_boundary_exact_160",
        160.0,
        false,
    )
    .map_err(|error| format!("hold_boundary_exact_160 check: {error}"))?;
    let full_hold = check_full_hold(trace, script)
        .map_err(|error| format!("hold_full_ticks check: {error}"))?;
    if let Some(event) = trace
        .iter()
        .find(|event| matches!(event, TraceEvent::Miss { .. }))
    {
        let TraceEvent::Miss { note_id, sim, .. } = event else {
            unreachable!("the filter only keeps Miss variants");
        };
        return Err(format!(
            "unexpected window miss for note {note_id} at sim {sim}; the synthesized stream \
             must hit every non-scripted note"
        ));
    }
    if trace
        .iter()
        .any(|event| matches!(event, TraceEvent::GameOver { .. }))
    {
        return Err(
            "the scripted acceptance run reached game_over; the slice trace must stay alive".into(),
        );
    }
    checks.push(("late_110", late));
    checks.push(("edge_160", edge_160));
    checks.push(("edge_161", edge_161));
    checks.push(("ghost_press", ghost));
    checks.push(("hold_big_drop", big_drop));
    checks.push(("hold_boundary_160", boundary));
    checks.push(("hold_boundary_exact_160", boundary_exact));
    checks.push(("hold_full_ticks", full_hold));
    Ok(checks)
}

fn check_threshold(
    trace: &[TraceEvent],
    target: &ScriptedTarget,
    case: &ThresholdCase,
) -> Result<String, String> {
    let event = find_hit(trace, target.note_id, target.press_sim)?;
    let TraceEvent::Hit {
        kind,
        judgement,
        note_diff_int,
        ..
    } = event
    else {
        return Err("matched a non-hit trace event".into());
    };
    if *kind != "hit" {
        return Err(format!(
            "note {} at sim {} has kind {kind:?}; expected \"hit\"",
            target.note_id, target.press_sim
        ));
    }
    if *judgement != case.judgement {
        return Err(format!(
            "note {} at sim {} judged {judgement:?}; the {} ms edge must be {:?}",
            target.note_id, target.press_sim, case.offset_ms, case.judgement
        ));
    }
    if *note_diff_int != case.offset_ms {
        return Err(format!(
            "note {} at sim {} has note_diff_int {note_diff_int}; expected {}",
            target.note_id, target.press_sim, case.offset_ms
        ));
    }
    Ok(format!(
        "note {} sim {} -> hit/{} diff {}",
        target.note_id, target.press_sim, judgement, note_diff_int
    ))
}

fn check_late_110(trace: &[TraceEvent], target: &ScriptedTarget) -> Result<String, String> {
    let event = find_hit(trace, target.note_id, target.press_sim)?;
    let TraceEvent::Hit {
        kind,
        judgement,
        note_diff_int,
        score_delta,
        ..
    } = event
    else {
        return Err("matched a non-hit trace event".into());
    };
    if *kind != "hit" {
        return Err(format!(
            "note {} at sim {} has kind {kind:?}; expected \"hit\"",
            target.note_id, target.press_sim
        ));
    }
    if *judgement != "bad" {
        return Err(format!(
            "note {} at sim {} judged {judgement:?}; expected \"bad\"",
            target.note_id, target.press_sim
        ));
    }
    if *note_diff_int != 110 {
        return Err(format!(
            "note {} at sim {} has note_diff_int {note_diff_int}; expected 110",
            target.note_id, target.press_sim
        ));
    }
    Ok(format!(
        "note {} sim {} -> hit/bad diff {} score {}",
        target.note_id, target.press_sim, note_diff_int, score_delta
    ))
}

fn check_edge_160(trace: &[TraceEvent], target: &ScriptedTarget) -> Result<String, String> {
    let event = find_hit(trace, target.note_id, target.press_sim)?;
    let TraceEvent::Hit {
        kind,
        judgement,
        note_diff_int,
        score_delta,
        health_before,
        health_after,
        combo_after,
        tallies,
        ..
    } = event
    else {
        return Err("matched a non-hit trace event".into());
    };
    if *kind != "hit" {
        return Err(format!(
            "note {} at sim {} has kind {kind:?}; expected \"hit\"",
            target.note_id, target.press_sim
        ));
    }
    if *judgement != "shit" {
        return Err(format!(
            "note {} at sim {} judged {judgement:?}; expected \"shit\"",
            target.note_id, target.press_sim
        ));
    }
    if *note_diff_int != 160 {
        return Err(format!(
            "note {} at sim {} has note_diff_int {}; expected 160",
            target.note_id, target.press_sim, note_diff_int
        ));
    }
    if *score_delta != 9 {
        return Err(format!(
            "note {} at sim {} has score_delta {}; expected 9",
            target.note_id, target.press_sim, score_delta
        ));
    }
    if (health_after - health_before + 0.02).abs() > 1e-12 {
        return Err(format!(
            "note {} at sim {} has health delta {}; expected -0.02",
            target.note_id,
            target.press_sim,
            health_after - health_before
        ));
    }
    if *combo_after != 0 {
        return Err(format!(
            "note {} at sim {} has combo_after {}; the shipped shit judgement must break the combo",
            target.note_id, target.press_sim, combo_after
        ));
    }
    let prior = prior_judgement(trace, event)?;
    let expected_shit = prior.tallies.shit.checked_add(1).ok_or_else(|| {
        format!(
            "note {} shit tally overflowed past {}",
            target.note_id, prior.tallies.shit
        )
    })?;
    if tallies.shit != expected_shit {
        return Err(format!(
            "note {} at sim {} has tallies.shit {}; expected {} (pre-insert {} + 1)",
            target.note_id, target.press_sim, tallies.shit, expected_shit, prior.tallies.shit
        ));
    }
    Ok(format!(
        "note {} sim {} -> hit/shit diff {} score {} health -0.02 combo reset tallies.shit {} (pre-insert {} + 1)",
        target.note_id,
        target.press_sim,
        note_diff_int,
        score_delta,
        tallies.shit,
        prior.tallies.shit
    ))
}

fn check_edge_161(trace: &[TraceEvent], target: &ScriptedTarget) -> Result<String, String> {
    let event = find_hit(trace, target.note_id, target.press_sim)?;
    let TraceEvent::Hit {
        kind,
        judgement,
        note_diff_int,
        score_delta,
        health_before,
        health_after,
        combo_before,
        combo_after,
        combo_max,
        tallies,
        ..
    } = event
    else {
        return Err("matched a non-hit trace event".into());
    };
    if *kind != "hit" {
        return Err(format!(
            "note {} at sim {} has kind {kind:?}; expected \"hit\"",
            target.note_id, target.press_sim
        ));
    }
    if *judgement != "miss" {
        return Err(format!(
            "note {} at sim {} judged {judgement:?}; expected the hit-path \"miss\"",
            target.note_id, target.press_sim
        ));
    }
    if *note_diff_int != 161 {
        return Err(format!(
            "note {} at sim {} has note_diff_int {}; expected 161",
            target.note_id, target.press_sim, note_diff_int
        ));
    }
    if *score_delta != -100 {
        return Err(format!(
            "note {} at sim {} has score_delta {}; expected -100",
            target.note_id, target.press_sim, score_delta
        ));
    }
    if (*health_after - *health_before).abs() > 0.0 {
        return Err(format!(
            "note {} at sim {} changed health by {}; the hit-path miss applies no health change",
            target.note_id,
            target.press_sim,
            health_after - health_before
        ));
    }
    let expected_combo = combo_before
        .checked_add(1)
        .ok_or_else(|| "combo counter overflow on the 161 ms hit-path miss".to_string())?;
    if *combo_after != expected_combo {
        return Err(format!(
            "note {} at sim {} combo {combo_before} -> {combo_after}; expected increment to \
             {expected_combo} (the shipped applyScore('miss') path)",
            target.note_id, target.press_sim
        ));
    }
    if trace.iter().any(
        |event| matches!(event, TraceEvent::Miss { note_id, .. } if *note_id == target.note_id),
    ) {
        return Err(format!(
            "note {} produced a later window-miss event after the 161 ms hit-path miss",
            target.note_id
        ));
    }
    let prior = prior_judgement(trace, event)?;
    let expected_missed = prior.tallies.missed.checked_add(1).ok_or_else(|| {
        format!(
            "note {} missed tally overflowed past {}",
            target.note_id, prior.tallies.missed
        )
    })?;
    if tallies.missed != expected_missed {
        return Err(format!(
            "note {} at sim {} has tallies.missed {}; expected {} (pre-insert {} + 1)",
            target.note_id, target.press_sim, tallies.missed, expected_missed, prior.tallies.missed
        ));
    }
    // combo_max is recorded after the hit. It moves only when the incremented
    // combo exceeds the pre-insert maximum; a later chain that is still below
    // the old maximum must leave combo_max untouched.
    let combo_max_before = prior.combo_max;
    let combo_max_updated = *combo_after > combo_max_before;
    let expected_combo_max = if combo_max_updated {
        *combo_after
    } else {
        combo_max_before
    };
    if *combo_max != expected_combo_max {
        return Err(format!(
            "note {} at sim {} has combo_max {combo_max}; expected {expected_combo_max} \
             (updated iff combo_after {combo_after} > combo_max_before {combo_max_before})",
            target.note_id, target.press_sim
        ));
    }
    Ok(format!(
        "note {} sim {} -> hit/miss diff {} score {} combo {} -> {} (increment) \
         tallies.missed {} (pre-insert {} + 1) combo_max {} (before {}, updated={combo_max_updated})",
        target.note_id,
        target.press_sim,
        note_diff_int,
        score_delta,
        combo_before,
        combo_after,
        tallies.missed,
        prior.tallies.missed,
        combo_max,
        combo_max_before
    ))
}

fn check_ghost(trace: &[TraceEvent], script: &Script) -> Result<String, String> {
    let matches: Vec<&TraceEvent> = trace
        .iter()
        .filter(|event| {
            matches!(
                event,
                TraceEvent::GhostPress {
                    sim,
                    note_direction,
                    ..
                } if *sim == script.ghost_sim && *note_direction == script.ghost_direction
            )
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one ghost_press at sim {} direction {}, found {}",
            script.ghost_sim,
            script.ghost_direction,
            matches.len()
        ));
    }
    let total_ghosts = trace
        .iter()
        .filter(|event| matches!(event, TraceEvent::GhostPress { .. }))
        .count();
    if total_ghosts != 1 {
        return Err(format!(
            "expected exactly the one scripted ghost press, found {total_ghosts}; the perfect \
             synthesized stream must not ghost"
        ));
    }
    let TraceEvent::GhostPress {
        score_delta,
        health_change,
        ..
    } = matches[0]
    else {
        unreachable!("the filter only keeps GhostPress variants");
    };
    if *score_delta != -10.0 {
        return Err(format!(
            "ghost press at sim {} has score_delta {score_delta}; expected -10.0",
            script.ghost_sim
        ));
    }
    if *health_change != -0.08 {
        return Err(format!(
            "ghost press at sim {} has health_change {health_change}; expected -0.08",
            script.ghost_sim
        ));
    }
    Ok(format!(
        "sim {} direction {} -> ghost press score -10.0 health -0.08",
        script.ghost_sim, script.ghost_direction
    ))
}

/// Tallies and `combo_max` immediately before `event`.
///
/// Hit and window-miss events record those counters after their own update,
/// and nothing else in the frozen trace mutates them, so the latest earlier
/// hit/miss is the pre-insert state (zeros when this is the first).
fn prior_judgement(trace: &[TraceEvent], event: &TraceEvent) -> Result<PriorJudgement, String> {
    let index = trace
        .iter()
        .position(|candidate| std::ptr::eq(candidate, event))
        .ok_or_else(|| "checked event is not in the trace".to_string())?;
    let mut prior = PriorJudgement {
        tallies: TallyCounts::default(),
        combo_max: 0,
    };
    for earlier in &trace[..index] {
        match earlier {
            TraceEvent::Hit {
                tallies, combo_max, ..
            }
            | TraceEvent::Miss {
                tallies, combo_max, ..
            } => {
                prior.tallies = *tallies;
                prior.combo_max = *combo_max;
            }
            _ => {}
        }
    }
    Ok(prior)
}

struct PriorJudgement {
    tallies: TallyCounts,
    combo_max: u32,
}

fn check_hold_inventory(trace: &[TraceEvent], script: &Script) -> Result<(), String> {
    // Every synthesized sustain releases at or before `ceil(end)`. That is two
    // ticks before the completion tick, so the next mirror still sees a
    // positive snapshot and drops. `hold_end` requires the key to still be
    // down at the first integer `T >= end + 2` (§6.1 REVISED items 8-9).
    let ends: Vec<u32> = trace
        .iter()
        .filter_map(|event| match event {
            TraceEvent::HoldEnd { note_id, .. } => Some(*note_id),
            _ => None,
        })
        .collect();
    if !ends.is_empty() {
        return Err(format!(
            "synthesized releases land at ceil(end), before the §6.1 REVISED completion tick; \
             expected no hold_end, found {ends:?}"
        ));
    }
    let starts: BTreeSet<u32> = trace
        .iter()
        .filter_map(|event| match event {
            TraceEvent::HoldStart { note_id, .. } => Some(*note_id),
            _ => None,
        })
        .collect();
    let early = [
        script.big_drop.note_id,
        script.boundary_160.note_id,
        script.boundary_exact_160.note_id,
    ];
    let mut drop_notes = Vec::new();
    for event in trace {
        let TraceEvent::HoldDrop {
            note_id,
            input_ms,
            score_penalty,
            combo_before,
            combo_after,
            is_combo_break,
            ..
        } = event
        else {
            continue;
        };
        drop_notes.push(*note_id);
        if early.contains(note_id) {
            continue;
        }
        if *input_ms != 0.0
            || *score_penalty != 0.0
            || *is_combo_break
            || combo_after != combo_before
        {
            return Err(format!(
                "end-release drop note {note_id} has input_ms {input_ms} score_penalty \
                 {score_penalty} is_combo_break={is_combo_break} combo {combo_before} -> \
                 {combo_after}; the stale snapshot is in (0, 1] and must not penalize \
                 (§6.1 REVISED items 8-9)"
            ));
        }
    }
    let drop_set: BTreeSet<u32> = drop_notes.iter().copied().collect();
    if drop_notes.len() != drop_set.len() || drop_set != starts {
        return Err(format!(
            "every hold_start must drop exactly once under the synthesized ceil(end) release; \
             starts {starts:?}, drops {drop_notes:?}"
        ));
    }
    for note_id in early.into_iter().chain([script.full_hold.note_id]) {
        if !drop_set.contains(&note_id) {
            return Err(format!("scripted sustain {note_id} did not drop"));
        }
    }
    Ok(())
}

fn check_big_drop(trace: &[TraceEvent], script: &Script) -> Result<String, String> {
    let stale_ms = hold_stale_snapshot(&script.big_drop);
    if stale_ms <= HOLD_DROP_THRESHOLD_MS {
        return Err(format!(
            "big sustain drop note {} stale snapshot {stale_ms} ms is not > 160",
            script.big_drop.note_id
        ));
    }
    let big = find_hold_drop(trace, &script.big_drop)?;
    let TraceEvent::HoldDrop {
        score_penalty,
        health_change,
        is_combo_break,
        combo_before,
        combo_after,
        ..
    } = big
    else {
        return Err("matched a non-hold_drop trace event".into());
    };
    let expected_penalty = SCORE_HOLD_DROP_PENALTY_PER_SECOND * stale_ms / 1000.0;
    if (*score_penalty - expected_penalty).abs() > 1e-12 {
        return Err(format!(
            "big sustain drop note {} has score_penalty {score_penalty}; expected \
             stale_snapshot/1000 * -125.0 = {expected_penalty}",
            script.big_drop.note_id
        ));
    }
    if *health_change != 0.0 {
        return Err(format!(
            "big sustain drop note {} has health_change {health_change}; the shipped constants \
             are zero",
            script.big_drop.note_id
        ));
    }
    if !*is_combo_break || *combo_after != 0 {
        return Err(format!(
            "big sustain drop note {} combo {combo_before} -> {combo_after} \
             is_combo_break={is_combo_break}; expected a combo break to 0",
            script.big_drop.note_id
        ));
    }
    Ok(format!(
        "note {} release {} stale snapshot {stale_ms} > 160 penalty {expected_penalty} \
         health 0.0 combo break; hold_drop at sim {}",
        script.big_drop.note_id,
        script.big_drop.release_sim,
        hold_drop_sim(&script.big_drop)?
    ))
}

/// Checks one of the two §6.1 REVISED boundary cases.
///
/// `expected_stale_ms` is the mirror snapshot observed on the penalty tick,
/// not the projected `end - release` value. The ADR-0018 `ceil(t+l-160)`
/// release sees stale 161 and penalizes; the new `ceil(t+l-159)` release sees
/// bitwise 160.0 and must not penalize or break combo.
fn check_boundary_drop(
    trace: &[TraceEvent],
    target: &HoldTarget,
    name: &str,
    expected_stale_ms: f64,
    penalizing: bool,
) -> Result<String, String> {
    let stale_ms = hold_stale_snapshot(target);
    if !same_f64(stale_ms, expected_stale_ms) {
        return Err(format!(
            "{name}: sustain drop note {} release {} observes stale snapshot {stale_ms}; \
             expected bitwise {expected_stale_ms}",
            target.note_id, target.release_sim
        ));
    }
    let boundary = find_hold_drop(trace, target)?;
    let TraceEvent::HoldDrop {
        sim,
        input_ms,
        score_penalty,
        health_change,
        combo_before,
        combo_after,
        is_combo_break,
        ..
    } = boundary
    else {
        return Err("matched a non-hold_drop trace event".into());
    };
    let drop_sim = hold_drop_sim(target)?;
    if *sim != drop_sim {
        return Err(format!(
            "{name}: sustain drop note {} emitted at sim {sim}; expected the mirror penalty \
             tick {drop_sim}",
            target.note_id
        ));
    }
    if *input_ms != 0.0 {
        return Err(format!(
            "{name}: sustain drop note {} has input_ms {input_ms}; a hold drop is a non-input \
             event with input_ms 0.0",
            target.note_id
        ));
    }
    let expected_penalty = if penalizing {
        SCORE_HOLD_DROP_PENALTY_PER_SECOND * stale_ms / 1000.0
    } else {
        0.0
    };
    if (*score_penalty - expected_penalty).abs() > 1e-12 {
        return Err(format!(
            "{name}: sustain drop note {} has score_penalty {score_penalty}; expected \
             {expected_penalty} from stale snapshot {stale_ms}",
            target.note_id
        ));
    }
    if *health_change != 0.0 {
        return Err(format!(
            "{name}: sustain drop note {} has health_change {health_change}; expected 0.0",
            target.note_id
        ));
    }
    if penalizing {
        if !*is_combo_break || *combo_after != 0 {
            return Err(format!(
                "{name}: sustain drop note {} combo {combo_before} -> {combo_after} \
                 is_combo_break={is_combo_break}; expected a penalizing combo break to 0",
                target.note_id
            ));
        }
    } else if *is_combo_break || *combo_after != *combo_before {
        return Err(format!(
            "{name}: sustain drop note {} combo {combo_before} -> {combo_after} \
             is_combo_break={is_combo_break}; expected no combo change on the zero-penalty branch",
            target.note_id
        ));
    }
    let branch = if penalizing {
        "penalizing"
    } else {
        "zero penalty / no combo break"
    };
    Ok(format!(
        "note {} release {} stale snapshot {stale_ms} -> {branch}; score_penalty \
         {expected_penalty} health 0.0; hold_drop at sim {drop_sim}",
        target.note_id, target.release_sim
    ))
}

fn check_full_hold(trace: &[TraceEvent], script: &Script) -> Result<String, String> {
    // The synthesized release is `ceil(end)`, two ticks before the completion
    // tick. The drop mirror therefore still sees `end - (ceil(end)-1)`, which
    // is in `(0, 1]`, and emits a zero-penalty drop. `hold_end` is only the
    // key-still-down path (§6.1 REVISED items 8-9); crate tests cover it.
    if trace.iter().any(|event| {
        matches!(event, TraceEvent::HoldEnd { note_id, .. } if *note_id == script.full_hold.note_id)
    }) {
        return Err(format!(
            "full hold note {} emitted hold_end; the synthesized ceil(end) release drops \
             while the snapshot is still positive (§6.1 REVISED items 8-9)",
            script.full_hold.note_id
        ));
    }
    let ceil_end = grid_ms(script.full_hold.end_ms)?;
    if script.full_hold.release_sim != ceil_end {
        return Err(format!(
            "full hold note {} release {} is not ceil(end) {ceil_end}",
            script.full_hold.note_id, script.full_hold.release_sim
        ));
    }
    let expected_stale = script.full_hold.end_ms - (ceil_end as f64 - 1.0);
    if !(expected_stale > 0.0 && expected_stale <= 1.0) {
        return Err(format!(
            "full hold note {} end-release remainder {expected_stale} is not in (0, 1]",
            script.full_hold.note_id
        ));
    }
    let drop = check_boundary_drop(
        trace,
        &script.full_hold,
        "hold_full_release",
        expected_stale,
        false,
    )?;
    let ticks = check_full_hold_ticks(trace, &script.full_hold)?;
    Ok(format!(
        "note {} end-release zero-penalty drop ({drop}); {ticks}",
        script.full_hold.note_id
    ))
}

fn hold_stale_snapshot(target: &HoldTarget) -> f64 {
    stale_snapshot(target.end_ms, target.strum_time_ms, target.release_sim)
}

/// The snapshot observed by the accrual branch at tick `sim`.
///
/// T0 uses the hit capture (`full`); T0+1 still sees `full` for an on-time
/// hit because the mirror's `P = T0` does not exceed `strumTime`. From T0+2
/// onward the mirror recomputes `end - (T-1)`, so the last accrual at
/// `ceil(end)` carries the positive stale value (1 ms for integer ends,
/// `0.5` for a `.5` end).
fn observed_snapshot(target: &HoldTarget, sim: u64) -> f64 {
    let full = target.end_ms - target.strum_time_ms;
    if sim == target.press_sim {
        full
    } else {
        let mirror_position = sim as f64 - 1.0;
        if mirror_position > target.strum_time_ms {
            target.end_ms - mirror_position
        } else {
            full
        }
    }
}

/// The `hold_drop` row is emitted on the same mirror tick that flags the
/// hold, one tick after the release (`release+1`) under §6.1 REVISED.
fn hold_drop_sim(target: &HoldTarget) -> Result<u64, String> {
    target.release_sim.checked_add(1).ok_or_else(|| {
        format!(
            "mirror hold-drop tick overflows u64 for note {}",
            target.note_id
        )
    })
}

/// Hold ticks emitted by the frozen §6.1 regime for a key held through
/// `ceil(end_ms)`: the hit tick T0 and every tick through `ceil(end)`, each
/// carrying the fixed per-entry 1 ms delta. There is no fractional remainder
/// row.
fn expected_hold_ticks(press_sim: u64, end_ms: f64) -> Result<Vec<u64>, String> {
    if !end_ms.is_finite() || end_ms < 0.0 {
        return Err(format!("full hold end {end_ms} is not a usable timestamp"));
    }
    let last = grid_ms(end_ms).map_err(|error| format!("full hold end {end_ms}: {error}"))?;
    if last < press_sim {
        return Err(format!(
            "full hold end {end_ms} (ceil {last}) precedes the hit tick {press_sim}"
        ));
    }
    Ok((press_sim..=last).collect())
}

fn check_full_hold_ticks(trace: &[TraceEvent], target: &HoldTarget) -> Result<String, String> {
    let expected = expected_hold_ticks(target.press_sim, target.end_ms)?;
    let actual: Vec<&TraceEvent> = trace
        .iter()
        .filter(|event| {
            matches!(event, TraceEvent::HoldTick { note_id, .. } if *note_id == target.note_id)
        })
        .collect();
    if actual.len() != expected.len() {
        return Err(format!(
            "full hold note {} emitted {} hold_tick(s); expected {} fixed per-entry ticks \
             from hit tick {} through ceil(end) {} (no fractional partial)",
            target.note_id,
            actual.len(),
            expected.len(),
            target.press_sim,
            target.end_ms
        ));
    }
    for (event, expected_sim) in actual.iter().zip(expected.iter().copied()) {
        let TraceEvent::HoldTick {
            sim: event_sim,
            health_delta,
            score_delta,
            hold_remaining_ms,
            hold_elapsed_ms,
            ..
        } = event
        else {
            return Err("matched a non-hold_tick trace event".into());
        };
        if *event_sim != expected_sim {
            return Err(format!(
                "full hold note {} tick at sim {event_sim}; expected sim {expected_sim}",
                target.note_id
            ));
        }
        if !same_f64(*health_delta, FULL_GRID_HOLD_HEALTH)
            || !same_f64(*score_delta, FULL_GRID_HOLD_SCORE)
        {
            return Err(format!(
                "full hold note {} tick sim {event_sim} has health_delta {health_delta} \
                 score_delta {score_delta}; expected the fixed per-entry 0.00012 and 0.25",
                target.note_id
            ));
        }
        let expected_remaining = observed_snapshot(target, *event_sim);
        if !same_f64(*hold_remaining_ms, expected_remaining) {
            return Err(format!(
                "full hold note {} tick sim {event_sim} has hold_remaining_ms \
                 {hold_remaining_ms}; expected the observed mirror snapshot {expected_remaining}",
                target.note_id
            ));
        }
        let expected_elapsed = *event_sim as f64 - target.press_sim as f64;
        if !same_f64(*hold_elapsed_ms, expected_elapsed) {
            return Err(format!(
                "full hold note {} tick sim {event_sim} has hold_elapsed_ms \
                 {hold_elapsed_ms}; expected sim - hit tick = {expected_elapsed}",
                target.note_id
            ));
        }
    }
    let detail_last = expected.last().copied().unwrap_or(target.press_sim);
    Ok(format!(
        "{} fixed per-entry ticks at health_delta 0.00012 score_delta 0.25 from hit tick {} \
         through ceil(end) {}; no fractional partial",
        expected.len(),
        target.press_sim,
        detail_last
    ))
}

fn find_hit(trace: &[TraceEvent], note_id: u32, sim: u64) -> Result<&TraceEvent, String> {
    let matches: Vec<&TraceEvent> = trace
        .iter()
        .filter(|event| {
            matches!(
                event,
                TraceEvent::Hit {
                    note_id: id,
                    sim: event_sim,
                    ..
                } if *id == note_id && *event_sim == sim
            )
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one hit for note {note_id} at sim {sim}, found {}; trace events \
             for that note: {:?}",
            matches.len(),
            note_events(trace, note_id)
        ));
    }
    Ok(matches[0])
}

fn find_hold_drop<'a>(
    trace: &'a [TraceEvent],
    target: &HoldTarget,
) -> Result<&'a TraceEvent, String> {
    let drop_sim = hold_drop_sim(target)?;
    let matches: Vec<&TraceEvent> = trace
        .iter()
        .filter(|event| {
            matches!(
                event,
                TraceEvent::HoldDrop {
                    note_id,
                    sim,
                    ..
                } if *note_id == target.note_id && *sim == drop_sim
            )
        })
        .collect();
    if matches.len() != 1 {
        return Err(format!(
            "expected exactly one hold_drop for note {} at the deferred sim {drop_sim}, found \
             {}; trace events for that note: {:?}",
            target.note_id,
            matches.len(),
            note_events(trace, target.note_id)
        ));
    }
    Ok(matches[0])
}

/// Compact trace summary for a failed expectation (kind + sim + judgement).
fn note_events(trace: &[TraceEvent], note_id: u32) -> Vec<String> {
    trace
        .iter()
        .filter_map(|event| {
            let (kind, sim, judgement, event_note_id) = match event {
                TraceEvent::Hit {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::Miss {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::GhostPress {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::HoldStart {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::HoldTick {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::HoldDrop {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::HoldEnd {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                }
                | TraceEvent::GameOver {
                    kind,
                    sim,
                    judgement,
                    note_id,
                    ..
                } => (kind, sim, judgement, note_id),
            };
            (*event_note_id == note_id).then(|| format!("{kind} sim={sim} judgement={judgement}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    const METADATA: &str = r#"{
      "version": "2.2.4",
      "songName": "Bopeebo",
      "playData": { "difficulties": ["easy", "normal", "hard"] },
      "timeChanges": [{ "t": 0, "bpm": 100 }]
    }"#;

    struct Fixture {
        root: PathBuf,
    }

    impl Fixture {
        fn with_notes(notes: &str) -> Self {
            let chart = format!(
                r#"{{"version":"2.0.0","scrollSpeed":{{"easy":1.2,"normal":1.3,"hard":1.6}},"events":[],"notes":{{"easy":[{notes}]}}}}"#
            );
            let root = std::env::temp_dir().join(format!(
                "oxide-gameplay-sim-test-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            let songs = root.join("preload/data/songs/bopeebo");
            fs::create_dir_all(&songs).expect("create fixture directory");
            fs::write(songs.join("bopeebo-chart.json"), chart).expect("write fixture chart");
            fs::write(songs.join("bopeebo-metadata.json"), METADATA)
                .expect("write fixture metadata");
            Self { root }
        }

        fn chart_path(&self) -> PathBuf {
            self.root
                .join("preload/data/songs/bopeebo/bopeebo-chart.json")
        }

        fn notes(&self, difficulty: &str) -> Vec<LaneNote> {
            let path = self.chart_path();
            let bytes = fs::read(&path).expect("read fixture chart");
            parse_notes(&bytes, &path.to_string_lossy(), difficulty).expect("parse fixture notes")
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn note(direction: i64, time_ms: f64, hold_ms: f64) -> LaneNote {
        LaneNote {
            time_ms,
            direction,
            difficulty: "easy".to_string(),
            hold_ms,
        }
    }

    /// Nine taps (six threshold edges plus 110/160/161) and four sustains,
    /// spaced so no lane/window collides. The sustain set supplies the big
    /// drop, both §6.1 REVISED boundary cases and the fractional full hold.
    /// The full-hold last accrual at `ceil(end)` carries the positive stale
    /// snapshot `0.5`; nothing sits between the 160 and 161 presses, so that
    /// hit does not extend `combo_max`.
    const SCRIPTED_NOTES: &str = r#"
        {"t":1000,"d":0},
        {"t":2000,"d":1},
        {"t":3000,"d":2},
        {"t":4000,"d":3},
        {"t":5000,"d":0},
        {"t":6000,"d":1},
        {"t":7000,"d":2},
        {"t":8000,"d":3},
        {"t":9000,"d":0},
        {"t":11000,"d":1,"l":500},
        {"t":13000,"d":2,"l":400},
        {"t":15000,"d":3,"l":300.5},
        {"t":17000,"d":3,"l":400}
    "#;

    fn check_detail<'a>(checks: &'a [(&'static str, String)], name: &str) -> &'a str {
        checks
            .iter()
            .find(|(check, _)| *check == name)
            .map(|(_, detail)| detail.as_str())
            .unwrap_or_else(|| panic!("missing check {name}"))
    }

    #[test]
    fn grid_and_release_arithmetic_use_ceil_on_the_one_ms_grid() {
        assert_eq!(grid_ms(1000.0).unwrap(), 1000);
        assert_eq!(grid_ms(1000.1).unwrap(), 1001);
        assert_eq!(grid_ms(1000.5).unwrap(), 1001);
        assert_eq!(grid_ms(0.0).unwrap(), 0);
        assert!(grid_ms(-0.5).is_err());
        assert!(grid_ms(f64::NAN).is_err());
        assert!(grid_ms(f64::INFINITY).is_err());

        let tap = note(0, 1000.5, 0.0);
        assert_eq!(press_audio(&tap), 1001.0);
        assert_eq!(release_audio(&tap), 1001.0);
        let hold = note(0, 1000.5, 10.5);
        assert_eq!(release_audio(&hold), 1011.0);
        // ceil(t + l) and ceil(t) + l differ here; the frozen rule is ceil(t+l).
        assert_eq!(grid_ms(release_audio(&hold)).unwrap(), 1011);
    }

    #[test]
    fn conversion_keeps_press_before_release_on_the_same_tick() {
        let mut events = vec![
            StreamEvent {
                audio_time_ms: 1000.2,
                direction: 0,
                difficulty: "easy".to_string(),
                state: Key::Up,
            },
            StreamEvent {
                audio_time_ms: 1000.2,
                direction: 0,
                difficulty: "easy".to_string(),
                state: Key::Down,
            },
        ];
        sort_stream(&mut events);
        let inputs = to_input_events(&events).unwrap();
        assert_eq!(
            inputs,
            vec![
                InputEvent::Press {
                    sim_ms: 1001,
                    direction: 0
                },
                InputEvent::Release {
                    sim_ms: 1001,
                    direction: 0
                }
            ]
        );
    }

    #[test]
    fn scripted_negative_cases_pass_on_a_synthetic_chart() {
        let fixture = Fixture::with_notes(SCRIPTED_NOTES);
        let notes = fixture.notes("easy");
        let first = simulate_with_notes(&fixture.root, "easy", &notes).expect("first pass");
        let second = simulate_with_notes(&fixture.root, "easy", &notes).expect("second pass");

        assert_eq!(first.digest, second.digest);
        assert_eq!(first.trace_bytes, second.trace_bytes);
        assert_eq!(
            first
                .checks
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            vec![
                "threshold_45",
                "threshold_46",
                "threshold_90",
                "threshold_91",
                "threshold_135",
                "threshold_136",
                "late_110",
                "edge_160",
                "edge_161",
                "ghost_press",
                "hold_big_drop",
                "hold_boundary_160",
                "hold_boundary_exact_160",
                "hold_full_ticks",
            ]
        );
        assert_eq!(
            first.counts,
            KindCounts {
                hit: 13,
                miss: 0,
                ghost_press: 1,
                hold_start: 4,
                // §6.1 REVISED fixed accrual from each hit tick through
                // ceil(end): big 11000..=11001 (2), ADR-0018 boundary
                // 13000..=13240 (241), new exact-160 boundary 17000..=17241
                // (242), full hold 15000..=15301 (302). Total 787.
                // The ceil(end) release drops; it does not reach hold_end.
                hold_tick: 787,
                hold_drop: 4,
                hold_end: 0,
                game_over: 0,
            }
        );
        assert!(check_detail(&first.checks, "threshold_45").contains("hit/sick diff 45"));
        assert!(check_detail(&first.checks, "threshold_46").contains("hit/good diff 46"));
        assert!(check_detail(&first.checks, "threshold_90").contains("hit/good diff 90"));
        assert!(check_detail(&first.checks, "threshold_91").contains("hit/bad diff 91"));
        assert!(check_detail(&first.checks, "threshold_135").contains("hit/bad diff 135"));
        assert!(check_detail(&first.checks, "threshold_136").contains("hit/shit diff 136"));
        assert!(check_detail(&first.checks, "late_110").contains("diff 110"));
        assert!(check_detail(&first.checks, "edge_160").contains("tallies.shit"));
        assert!(check_detail(&first.checks, "edge_161").contains("tallies.missed"));
        assert!(check_detail(&first.checks, "edge_161").contains("updated=false"));
        assert!(check_detail(&first.checks, "hold_big_drop").contains("stale snapshot 500"));
        assert!(check_detail(&first.checks, "hold_big_drop").contains("-62.5"));
        let boundary_160 = check_detail(&first.checks, "hold_boundary_160");
        assert!(
            boundary_160.contains("stale snapshot 161"),
            "{boundary_160}"
        );
        assert!(boundary_160.contains("penalizing"), "{boundary_160}");
        assert!(boundary_160.contains("-20.125"), "{boundary_160}");
        let boundary_exact = check_detail(&first.checks, "hold_boundary_exact_160");
        assert!(
            boundary_exact.contains("stale snapshot 160"),
            "{boundary_exact}"
        );
        assert!(
            boundary_exact.contains("zero penalty / no combo break"),
            "{boundary_exact}"
        );
        let ticks = check_detail(&first.checks, "hold_full_ticks");
        assert!(ticks.contains("302 fixed per-entry ticks"), "{ticks}");
        assert!(ticks.contains("health_delta 0.00012"), "{ticks}");
        assert!(ticks.contains("score_delta 0.25"), "{ticks}");
        assert!(ticks.contains("no fractional partial"), "{ticks}");
        let text = std::str::from_utf8(&first.trace_bytes).expect("utf-8 trace");
        assert!(
            text.starts_with(
                r#"{"schema":"1","kind":"hit","sim":1045,"input_ms":1045.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"sick","#
            ),
            "{}",
            text.lines().next().unwrap_or("")
        );
        assert!(text.lines().all(|line| !line.is_empty()));
    }

    #[test]
    fn synthetic_two_run_determinism_is_byte_and_digest_stable() {
        let fixture = Fixture::with_notes(SCRIPTED_NOTES);
        let notes = fixture.notes("easy");
        let first = simulate_with_notes(&fixture.root, "easy", &notes).unwrap();
        let second = simulate_with_notes(&fixture.root, "easy", &notes).unwrap();
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.trace_bytes, second.trace_bytes);
        assert_eq!(first.event_count, second.event_count);
    }

    #[test]
    fn exported_stream_matches_the_gates_own_grid_inputs_and_is_deterministic() {
        let fixture = Fixture::with_notes(SCRIPTED_NOTES);
        let notes = fixture.notes("easy");
        let exported = export_stream_from_notes(&notes, "easy").expect("first export");
        let again = export_stream_from_notes(&notes, "easy").expect("second export");
        assert_eq!(exported.bytes, again.bytes);
        assert_eq!(exported.digest, again.digest);

        let script = select_script(&notes).unwrap();
        let stream = build_scripted_stream(&notes, &script).unwrap();
        let inputs = to_input_events(&stream).unwrap();
        let text = std::str::from_utf8(&exported.bytes).expect("stream is UTF-8");
        let mut lines = text.lines();
        let header: serde_json::Value =
            serde_json::from_str(lines.next().expect("header line")).expect("header JSON");
        assert_eq!(header["schema"], STREAM_SCHEMA);
        assert_eq!(header["difficulty"], "easy");
        assert_eq!(header["chart_id"], CHART_ID);
        assert_eq!(header["tick_count"].as_u64(), Some(exported.tick_count));
        assert_eq!(header["songpos_source"], SONGPOS_SOURCE);

        let mut seen = 0usize;
        let mut ticks = 0u64;
        for (index, line) in lines.enumerate() {
            let tick: serde_json::Value = serde_json::from_str(line).expect("tick JSON");
            assert_eq!(tick["sim"].as_u64(), Some(index as u64));
            assert_eq!(tick["music_ms"].as_f64(), Some(index as f64));
            for event in tick["events"].as_array().expect("events array") {
                let input = inputs.get(seen).expect("input not in the gate stream");
                let (kind, direction, input_ms) = match input {
                    InputEvent::Press { sim_ms, direction } => ("press", *direction, *sim_ms),
                    InputEvent::Release { sim_ms, direction } => ("release", *direction, *sim_ms),
                };
                assert_eq!(event["kind"], kind);
                assert_eq!(event["direction"].as_i64(), Some(direction));
                assert_eq!(event["input_ms"].as_u64(), Some(input_ms));
                assert_eq!(input_ms, index as u64);
                seen += 1;
            }
            ticks += 1;
        }
        assert_eq!(ticks, exported.tick_count);
        assert_eq!(seen, inputs.len());
    }

    #[test]
    fn provenance_mismatch_and_missing_assets_fail_closed() {
        let fixture = Fixture::with_notes(SCRIPTED_NOTES);
        let mismatch = simulate_difficulty(&fixture.root, "easy")
            .expect_err("synthetic chart bytes cannot match the frozen bopeebo provenance");
        assert!(
            mismatch.contains("differs from the frozen provenance"),
            "{mismatch}"
        );

        let missing = fixture.root.join("missing-assets");
        let error =
            simulate_difficulty(&missing, "easy").expect_err("a missing asset root must fail");
        assert!(error.contains("cannot read asset"), "{error}");
    }

    #[test]
    fn unknown_difficulty_and_impossible_negative_cases_fail_closed() {
        let fixture = Fixture::with_notes(SCRIPTED_NOTES);
        let notes = fixture.notes("easy");
        let error = simulate_with_notes(&fixture.root, "expert", &notes)
            .expect_err("the slice supports only easy, normal and hard");
        assert!(error.contains("unknown difficulty"), "{error}");

        let only_one_tap = vec![note(0, 1000.0, 0.0)];
        let error =
            select_script(&only_one_tap).expect_err("one tap cannot supply six threshold edges");
        assert!(error.contains("six distinct free"), "{error}");
        assert!(error.contains("45"), "{error}");

        let no_holds: Vec<_> = (0..9)
            .map(|index| note(i64::from(index % 4), 1000.0 + (index as f64) * 2000.0, 0.0))
            .collect();
        let error = select_script(&no_holds).expect_err("the sustain drops require hold notes");
        assert!(error.contains("stale mirror snapshot > 160"), "{error}");
    }

    #[test]
    fn too_few_free_taps_fail_closed() {
        let five: Vec<_> = (0..5)
            .map(|index| note(i64::from(index % 4), 1000.0 + (index as f64) * 2000.0, 0.0))
            .collect();
        let error = select_script(&five).expect_err("five taps cannot supply six threshold edges");
        assert!(error.contains("six distinct free"), "{error}");
        assert!(error.contains("136"), "{error}");
        assert!(error.contains("claimed 5 of 6"), "{error}");
    }

    #[test]
    fn both_revised_boundary_cases_are_mandatory() {
        let fractional_only = vec![note(0, 1000.0, 412.5)];
        let error = select_boundary_drop(
            &fractional_only,
            &BTreeSet::new(),
            &[],
            160.0,
            161.0,
            "stale 161 penalizing",
        )
        .expect_err("a non-integer end cannot observe stale 161");
        assert!(error.contains("stale 161"), "{error}");
        assert!(error.contains("refuses to skip"), "{error}");

        let mut notes: Vec<_> = (0..9)
            .map(|index| note(i64::from(index % 4), 1000.0 + (index as f64) * 2000.0, 0.0))
            .collect();
        notes.push(note(1, 20000.0, 412.5));
        notes.push(note(2, 22000.0, 100.0));
        let error = select_boundary_drop(
            &notes,
            &BTreeSet::new(),
            &[],
            160.0,
            161.0,
            "stale 161 penalizing",
        )
        .expect_err("no penalizing stale-161 candidate");
        assert!(error.contains("stale 161"), "{error}");

        // ADR-0018 release ceil(t+l-160): stale 161 => -20.125 penalty.
        let holds = vec![note(0, 1000.0, 412.5), note(1, 3000.0, 400.0)];
        let target = select_boundary_drop(
            &holds,
            &BTreeSet::new(),
            &[],
            160.0,
            161.0,
            "stale 161 penalizing",
        )
        .expect("penalizing boundary target");
        assert_eq!(target.note_index, 1);
        assert_eq!(target.release_sim, 3240);
        assert!(same_f64(hold_stale_snapshot(&target), 161.0));

        // New release ceil(t+l-159): stale bitwise 160.0 => no penalty.
        let target = select_boundary_drop(
            &holds,
            &BTreeSet::new(),
            &[],
            159.0,
            160.0,
            "stale exactly 160.0 zero-penalty",
        )
        .expect("zero-penalty boundary target");
        assert_eq!(target.note_index, 1);
        assert_eq!(target.release_sim, 3241);
        assert!(same_f64(hold_stale_snapshot(&target), 160.0));

        // A 160 ms sustain releases on t+1 for the -159 formula and observes
        // the hit capture full = 160.0 (not a recompute).
        let same_tick = vec![note(0, 1000.0, 160.0)];
        let target = select_boundary_drop(
            &same_tick,
            &BTreeSet::new(),
            &[],
            159.0,
            160.0,
            "stale exactly 160.0 zero-penalty",
        )
        .expect("160 ms hold is the zero-penalty boundary");
        assert_eq!(target.press_sim, 1000);
        assert_eq!(target.release_sim, 1001);
        assert!(same_f64(hold_stale_snapshot(&target), 160.0));
        assert!(
            select_boundary_drop(
                &same_tick,
                &BTreeSet::new(),
                &[],
                160.0,
                161.0,
                "stale 161 penalizing",
            )
            .is_err()
        );
    }

    fn scripted_tap(note_id: u32, press_sim: u64) -> ScriptedTarget {
        ScriptedTarget {
            note_index: note_id as usize,
            note_id,
            lane: 0,
            press_sim,
        }
    }

    // The frozen hit record has this many fields; a test-only positional
    // builder keeps the tally/combo cases readable without a second type.
    #[allow(clippy::too_many_arguments)]
    fn hit_event(
        sim: u64,
        note_id: u32,
        judgement: &'static str,
        note_diff_int: i32,
        score_delta: i32,
        health_before: f64,
        health_after: f64,
        combo_before: u32,
        combo_after: u32,
        combo_max: u32,
        tallies: TallyCounts,
    ) -> TraceEvent {
        TraceEvent::Hit {
            schema: "1",
            kind: "hit",
            sim,
            input_ms: sim as f64,
            input_time_us: 0.0,
            note_id,
            note_direction: 0,
            judgement,
            raw_note_diff: f64::from(note_diff_int),
            note_diff_int,
            score_delta,
            health_before,
            health_after,
            combo_before,
            combo_after,
            combo_max,
            tallies,
        }
    }

    fn hold_tick(
        note_id: u32,
        sim: u64,
        health_delta: f64,
        score_delta: f64,
        target: &HoldTarget,
    ) -> TraceEvent {
        TraceEvent::HoldTick {
            schema: "1",
            kind: "hold_tick",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id,
            note_direction: 0,
            judgement: "none",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            health_delta,
            score_delta,
            hold_remaining_ms: observed_snapshot(target, sim),
            hold_elapsed_ms: sim as f64 - target.press_sim as f64,
        }
    }

    #[test]
    fn edge_160_requires_shit_tally_increment_and_161_updates_combo_max_iff() {
        let prior = hit_event(
            0,
            0,
            "shit",
            136,
            9,
            1.0,
            0.98,
            1,
            0,
            3,
            TallyCounts {
                shit: 1,
                total_notes_hit: 1,
                ..TallyCounts::default()
            },
        );
        let incremented = hit_event(
            100,
            1,
            "shit",
            160,
            9,
            0.98,
            0.96,
            0,
            0,
            3,
            TallyCounts {
                shit: 2,
                total_notes_hit: 2,
                ..TallyCounts::default()
            },
        );
        let target = scripted_tap(1, 100);
        let detail = check_edge_160(&[prior.clone(), incremented], &target).expect("shit + 1");
        assert!(detail.contains("tallies.shit 2"), "{detail}");

        let stale = hit_event(
            100,
            1,
            "shit",
            160,
            9,
            0.98,
            0.96,
            0,
            0,
            3,
            TallyCounts {
                shit: 1,
                total_notes_hit: 2,
                ..TallyCounts::default()
            },
        );
        let error = check_edge_160(&[prior, stale], &target).expect_err("stale shit tally");
        assert!(error.contains("tallies.shit"), "{error}");

        let updated = hit_event(
            161,
            0,
            "miss",
            161,
            -100,
            1.0,
            1.0,
            0,
            1,
            1,
            TallyCounts {
                missed: 1,
                total_notes_hit: 1,
                ..TallyCounts::default()
            },
        );
        let detail = check_edge_161(&[updated], &scripted_tap(0, 161)).expect("combo_max grows");
        assert!(detail.contains("tallies.missed 1"), "{detail}");
        assert!(detail.contains("updated=true"), "{detail}");

        let prior_max = hit_event(
            0,
            0,
            "sick",
            0,
            500,
            1.0,
            1.03,
            0,
            1,
            4,
            TallyCounts {
                sick: 1,
                total_notes_hit: 1,
                ..TallyCounts::default()
            },
        );
        let kept = hit_event(
            161,
            1,
            "miss",
            161,
            -100,
            1.03,
            1.03,
            0,
            1,
            4,
            TallyCounts {
                sick: 1,
                missed: 1,
                total_notes_hit: 2,
                ..TallyCounts::default()
            },
        );
        let detail =
            check_edge_161(&[prior_max.clone(), kept], &scripted_tap(1, 161)).expect("kept max");
        assert!(detail.contains("updated=false"), "{detail}");

        let moved = hit_event(
            161,
            1,
            "miss",
            161,
            -100,
            1.03,
            1.03,
            0,
            1,
            1,
            TallyCounts {
                sick: 1,
                missed: 1,
                total_notes_hit: 2,
                ..TallyCounts::default()
            },
        );
        let error = check_edge_161(&[prior_max.clone(), moved], &scripted_tap(1, 161))
            .expect_err("combo_max must stay at the pre-insert maximum");
        assert!(error.contains("combo_max"), "{error}");

        let missed_stale = hit_event(
            161,
            1,
            "miss",
            161,
            -100,
            1.03,
            1.03,
            0,
            1,
            4,
            TallyCounts {
                sick: 1,
                total_notes_hit: 2,
                ..TallyCounts::default()
            },
        );
        let error = check_edge_161(&[prior_max, missed_stale], &scripted_tap(1, 161))
            .expect_err("missed tally must increment");
        assert!(error.contains("tallies.missed"), "{error}");
    }

    #[test]
    fn full_hold_ticks_require_the_fixed_per_entry_rate_through_ceil_end() {
        let fractional = HoldTarget {
            note_index: 0,
            note_id: 7,
            lane: 0,
            press_sim: 0,
            release_sim: 3,
            strum_time_ms: 0.0,
            end_ms: 2.5,
        };
        let good = vec![
            hold_tick(7, 0, 0.00012, 0.25, &fractional),
            hold_tick(7, 1, 0.00012, 0.25, &fractional),
            hold_tick(7, 2, 0.00012, 0.25, &fractional),
            hold_tick(7, 3, 0.00012, 0.25, &fractional),
        ];
        let detail = check_full_hold_ticks(&good, &fractional).expect("fixed ticks accepted");
        assert!(detail.contains("4 fixed per-entry ticks"), "{detail}");
        assert!(detail.contains("health_delta 0.00012"), "{detail}");
        assert!(detail.contains("score_delta 0.25"), "{detail}");
        assert!(detail.contains("no fractional partial"), "{detail}");

        let mut deviant = good.clone();
        deviant[2] = hold_tick(7, 2, 0.00024, 0.25, &fractional);
        let error = check_full_hold_ticks(&deviant, &fractional).expect_err("fixed tick rate");
        assert!(error.contains("0.00012"), "{error}");
        assert!(error.contains("0.25"), "{error}");

        let short = good[..3].to_vec();
        let error = check_full_hold_ticks(&short, &fractional).expect_err("missing ceil-end tick");
        assert!(error.contains("expected 4"), "{error}");
        assert!(error.contains("no fractional partial"), "{error}");

        let mut wrong_elapsed = good.clone();
        wrong_elapsed[2] = TraceEvent::HoldTick {
            schema: "1",
            kind: "hold_tick",
            sim: 2,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 7,
            note_direction: 0,
            judgement: "none",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            health_delta: 0.00012,
            score_delta: 0.25,
            hold_remaining_ms: 1.5,
            hold_elapsed_ms: 0.0,
        };
        let error =
            check_full_hold_ticks(&wrong_elapsed, &fractional).expect_err("elapsed mismatch");
        assert!(error.contains("hold_elapsed_ms"), "{error}");

        let integer = HoldTarget {
            note_index: 0,
            note_id: 7,
            lane: 0,
            press_sim: 0,
            release_sim: 2,
            strum_time_ms: 0.0,
            end_ms: 2.0,
        };
        let ticks = vec![
            hold_tick(7, 0, 0.00012, 0.25, &integer),
            hold_tick(7, 1, 0.00012, 0.25, &integer),
            hold_tick(7, 2, 0.00012, 0.25, &integer),
        ];
        let detail = check_full_hold_ticks(&ticks, &integer).expect("integer end");
        assert!(detail.contains("3 fixed per-entry ticks"), "{detail}");
        assert!(detail.contains("no fractional partial"), "{detail}");
    }

    #[test]
    fn cli_parse_rejects_unknown_flags() {
        let error = match Cli::parse(&["--unknown".into()]) {
            Ok(_) => panic!("unknown flags must not parse"),
            Err(error) => error,
        };
        assert!(error.contains("unsupported argument"), "{error}");
    }
}
