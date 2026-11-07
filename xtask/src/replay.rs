//! `cargo xtask replay` — deterministic replay report for one M4 chart fixture.
//!
//! Gate (M4 contract §4): deterministic hits + score from a synthetic input
//! stream. Required M4 evidence is DETERMINISM — two gate runs print the same
//! canonical report sha256 — plus hit/miss counts; not gameplay/note parity,
//! discipline-record boundaries stay explicit (see the gate disclaimer). The
//! purity tripwire (the evaluator reads no clock/RNG or map-order state); the
//! digest and the CLI reruns are the ordering evidence.
//!
//! Observed-original facts behind the synthesis (citations verified against the
//! reference checkout): chart note `t` is song-position ms directly comparable
//! with `Conductor.instance.songPosition` (PlayState.hx L3118 subtracts
//! `note.noteData.time` from the conductor clock). L3118 is the scoring delta
//! computed after the window logic (`GRhythmUtil.processWindow`) has accepted
//! the note, so `Std.int(...)==0` used by this gate is NOT the observed accept
//! window — the real windows are the documented M5 deliverable.
//! `NoteData.getStepTime` (SongData.hx) is an independent ms->steps
//! conversion, not a step that follows L3118. Engine decisions (not
//! observed): use note `t` itself instead of the oracle fixture timeline (a
//! round-trip would only re-add model rounding drift), `inputLatencyMs = 0`,
//! and the 1 ms integer-grid press snap.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;
use sha2::{Digest, Sha256};

use oxide_data::Typed;
use oxide_data::chart::Chart;

/// Freeze the corpus provenance (chart paths + measured input asset hashes);
/// the gate never rebuilds or regenerates reference material.
pub(crate) const PROVENANCE_JSON: &str = include_str!("../../tools/oracle/provenance.json");
/// Per-hit placeholder for `score`; real rating windows are the M5 port's
/// deliverable and are not invented here.
const PLACEHOLDER_SCORE_PER_HIT: i64 = 350;
pub(crate) const INPUT_SOURCE: &str = "synthesized from chart (no human replay yet)";

/// Errors for the gate: usage/prerequisite problems exit 2, real failures exit 1.
#[derive(Debug, PartialEq)]
pub struct ReplayError(pub String);

/// One transition of the synthesized input stream.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamEvent {
    pub audio_time_ms: f64,
    pub direction: i64,
    pub difficulty: String,
    pub state: Key,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Down,
    Up,
}

fn state_rank(state: Key) -> u8 {
    match state {
        Key::Down => 0,
        Key::Up => 1,
    }
}

/// One chart note the stream is matched against.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneNote {
    /// Note strum time in ms, exactly as stored on the wire (observed).
    pub time_ms: f64,
    pub direction: i64,
    pub difficulty: String,
    /// Sustain length in ms (wire `l`; 0 for a tap).
    pub hold_ms: f64,
}

/// Outcome of one evaluation pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassEval {
    pub hits: usize,
    /// Presses matching no lane note plus notes no press reached.
    pub misses: usize,
}

/// Canonical deterministic report; field order defines the digest input.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReplayReport {
    input_source: &'static str,
    chart: String,
    hits: usize,
    misses: usize,
    score: i64,
    notes: usize,
    events: usize,
}

pub fn run(rest: &[String]) -> ExitCode {
    let cli = match Cli::parse(rest) {
        Ok(cli) => cli,
        Err(usage) => {
            eprintln!("replay: {usage}");
            return ExitCode::from(2);
        }
    };
    if cli.corpus {
        return run_corpus(&cli.assets_root);
    }
    let (report, digest) = match replay(&cli.chart_id, &cli.assets_root) {
        Ok(parts) => parts,
        Err(ReplayError(message)) => {
            eprintln!("replay: {message}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "replay: {} hits {} misses {} score {}",
        report.chart, report.hits, report.misses, report.score
    );
    println!("replay: input source: {INPUT_SOURCE}");
    println!(
        "replay: score placeholder = {PLACEHOLDER_SCORE_PER_HIT}/hit; PlayState windows and \
         ratings (epicas) are measured at M5 — not gameplay parity"
    );
    println!(
        "replay: model boundary: 1 ms integer-grid playback and a Std.int \
         songPosition-note.t stand-in (the observed scoring delta shape, not the \
         accept window). Two gate runs print identical sha256. Not native-game \
         note parity; acceptance linkage notes print with each gate invocation"
    );
    println!("replay: determinism sha256 {digest}");
    ExitCode::SUCCESS
}

struct Cli {
    chart_id: String,
    assets_root: PathBuf,
    corpus: bool,
}

impl Cli {
    const USAGE: &str = "usage: replay --fixture <chart-id> | replay --corpus [--assets <dir>]";

    fn parse(rest: &[String]) -> Result<Cli, String> {
        let mut fixture = None;
        let mut assets = None;
        let mut corpus = false;
        let mut iter = rest.iter();
        while let Some(flag) = iter.next() {
            match flag.as_str() {
                "--corpus" => {
                    corpus = true;
                }
                "--fixture" => {
                    let Some(value) = iter.next() else {
                        return Err(Self::USAGE.into());
                    };
                    fixture = Some(value.clone());
                }
                "--assets" => {
                    let Some(value) = iter.next() else {
                        return Err(Self::USAGE.into());
                    };
                    assets = Some(value.clone());
                }
                other => return Err(format!("unsupported argument {other:?}; {}", Self::USAGE)),
            }
        }
        if corpus && fixture.is_some() {
            return Err("replay --corpus does not take --fixture".into());
        }
        let assets_root = match assets {
            Some(dir) => PathBuf::from(dir),
            None => match std::env::var("OXIDE_ASSETS_ROOT") {
                Ok(root) => PathBuf::from(root),
                Err(_) => {
                    return Err(
                        "missing --assets <dir> or OXIDE_ASSETS_ROOT: real chart assets \
                         must be provided explicitly"
                            .into(),
                    );
                }
            },
        };
        if corpus {
            return Ok(Cli {
                chart_id: String::new(),
                assets_root,
                corpus: true,
            });
        }
        let Some(chart_id) = fixture else {
            return Err(Self::USAGE.into());
        };
        Ok(Cli {
            chart_id,
            assets_root,
            corpus: false,
        })
    }
}

/// `replay --corpus`: deterministic reports for the committed 61-chart
/// inventory, in lex icographically stable order, all-or-nothing. The smoke
/// entry has no chart asset and is intentionally excluded. Any chart failing
/// its own determinism or integrity checks fails the whole gate.
fn run_corpus(assets_root: &Path) -> ExitCode {
    let mut ids: Vec<String> = Vec::new();
    {
        let raw = match serde_json::from_str::<serde_json::Value>(PROVENANCE_JSON) {
            Ok(raw) => raw,
            Err(error) => {
                eprintln!("replay corpus: embedded provenance.json is unparseable: {error}");
                return ExitCode::FAILURE;
            }
        };
        let inventory = match inventory_map(&raw) {
            Ok(map) => map,
            Err(ReplayError(message)) => {
                eprintln!("replay corpus: {message}");
                return ExitCode::FAILURE;
            }
        };
        if let Err(error) = collect_inventory_ids(&inventory, &mut ids) {
            eprintln!("replay corpus: {error}");
            return ExitCode::FAILURE;
        }
        ids.sort();
        if ids.len() != 61 {
            eprintln!(
                "replay corpus: provenance inventory holds {} chart ids; the committed \
                 corpus is exactly 61 — refusing to replay a drifted inventory",
                ids.len()
            );
            return ExitCode::FAILURE;
        }
    }
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut digests = Vec::new();
    for id in &ids {
        match replay(id, assets_root) {
            Ok((report, digest)) => {
                println!(
                    "replay: {} hits {} misses {} score {}",
                    report.chart, report.hits, report.misses, report.score
                );
                digests.push((report.chart.clone(), digest));
                ok += 1;
            }
            Err(ReplayError(message)) => {
                eprintln!("replay corpus: {message}");
                failed += 1;
            }
        }
    }
    println!("replay corpus: ok {ok} failed {failed} input source {INPUT_SOURCE}");
    println!(
        "replay corpus: score placeholder = {PLACEHOLDER_SCORE_PER_HIT}/hit; PlayState          windows and ratings (epicas) are measured at M5 — not gameplay parity"
    );
    println!(
        "replay corpus: per-chart determinism digests are stable across evaluation passes;          no human replay stream exists (input source is synthesized from the chart)          and the flow carries no platform/hardware input claim"
    );
    if failed != 0 || ok + failed != ids.len() {
        eprintln!("replay corpus: gate failure");
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Chart ids derived from the committed provenance inventory (exactly one id
/// per chart asset; ambiguity fails instead of picking).
fn collect_inventory_ids(
    inventory: &BTreeMap<String, String>,
    ids: &mut Vec<String>,
) -> Result<(), String> {
    for path in inventory.keys() {
        let Some(id) = fixture_id_of(path) else {
            continue;
        };
        if ids.iter().any(|seen| seen == &id) {
            return Err(format!("provenance has two entries for chart id {id:?}"));
        }
        ids.push(id);
    }
    Ok(())
}

/// Gate entry, reusable by tests: (report, canonical sha256 hex).
pub fn replay(chart_id: &str, assets_root: &Path) -> Result<(ReplayReport, String), ReplayError> {
    let notes = load_notes(chart_id, assets_root)?;
    let events = synthesize_stream(&notes);
    let first = evaluate_pass(&notes, &events);
    let second = evaluate_pass(&notes, &events);
    if first != second {
        return Err(ReplayError(
            "two evaluation passes of an unchanged stream diverged; refusing to report".into(),
        ));
    }
    let report = ReplayReport {
        input_source: INPUT_SOURCE,
        chart: chart_id.to_string(),
        hits: first.hits,
        misses: first.misses,
        score: first.hits as i64 * PLACEHOLDER_SCORE_PER_HIT,
        notes: notes.len(),
        events: events.len(),
    };
    let canonical = serde_json::to_vec(&report).map_err(|e| {
        ReplayError(format!(
            "canonical report serialization is unbuildable: {e}"
        ))
    })?;
    let digest = format!("{:x}", Sha256::digest(&canonical));
    Ok((report, digest))
}

/// Chart → lane notes via the provenance-verified asset path (fail-closed).
fn load_notes(chart_id: &str, assets_root: &Path) -> Result<Vec<LaneNote>, ReplayError> {
    let raw = serde_json::from_str::<serde_json::Value>(PROVENANCE_JSON)
        .map_err(|e| ReplayError(format!("embedded provenance.json is unparseable: {e}")))?;
    let provenance = Provenance {
        input_asset_sha256: inventory_map(&raw)?,
    };
    let path = resolve_chart_path(chart_id, &provenance.input_asset_sha256)?;
    let expected = provenance
        .input_asset_sha256
        .get(path)
        .ok_or_else(|| ReplayError(format!("provenance lost the hash of {path}")))?;
    // Provenance paths are game-checkout relative (`assets/preload/...`); the
    // assets root is the assets directory itself (round_trip.rs convention), so
    // the `assets/` component is consumed explicitly, never silently.
    let relative = path.strip_prefix("assets/").unwrap_or(path);
    let file = assets_root.join(relative);
    let bytes = fs::read(&file).map_err(|e| {
        ReplayError(format!(
            "cannot read chart asset {} (real game assets required): {e}",
            file.display()
        ))
    })?;
    let actual = format!("{:x}", Sha256::digest(&bytes));
    if actual != *expected {
        return Err(ReplayError(format!(
            "chart asset {} sha256 {actual} differs from the frozen provenance {expected}; \
             refusing to replay outside the measured corpus",
            file.display()
        )));
    }
    let parsed = oxide_data::parse_file(&file, &bytes)
        .map_err(|e| ReplayError(format!("chart parse error for {}: {e}", file.display())))?;
    let Typed::Chart(owned) = &parsed.typed else {
        return Err(ReplayError(format!(
            "{} was typed {:?}; a V-Slice chart was required",
            file.display(),
            parsed.kind
        )));
    };
    let chart: &Chart = owned;
    assert_notes_non_negative(chart)?;
    Ok(chart_to_notes(chart))
}

/// The synthesized stream presses on the 1 ms playback grid, so negative
/// note times cannot be honored; fail closed instead of clamping silently.
fn assert_notes_non_negative(chart: &Chart) -> Result<(), ReplayError> {
    for (difficulty, per_difficulty) in &chart.notes {
        if let Some(note) = per_difficulty.iter().find(|note| note.time < 0.0) {
            return Err(ReplayError(format!(
                "difficulty {difficulty:?} holds a negative note time ms ({}); the \
                 synthesized grid stream cannot represent it",
                note.time
            )));
        }
    }
    Ok(())
}

#[derive(Debug)]
struct Provenance {
    input_asset_sha256: BTreeMap<String, String>,
}

/// Latched note order: by (time, direction, difficulty); per-lane subslices
/// stay ascending so the evaluator merge is a straightforward scan.
pub fn chart_to_notes(chart: &Chart) -> Vec<LaneNote> {
    let mut notes = Vec::new();
    for (difficulty, per_difficulty) in &chart.notes {
        for note in per_difficulty {
            notes.push(LaneNote {
                time_ms: note.time,
                direction: note.direction,
                difficulty: difficulty.clone(),
                hold_ms: note.sustain.unwrap_or(0.0),
            });
        }
    }
    notes.sort_by(|a, b| {
        a.time_ms
            .total_cmp(&b.time_ms)
            .then_with(|| a.direction.cmp(&b.direction))
            .then_with(|| a.difficulty.cmp(&b.difficulty))
    });
    notes
}

/// Fixture id → exact chart asset path from the provenance inventory.
///
/// `<song>-default` ↔ `<song>-chart.json`; `<song>-<variant>` ↔
/// `<song>-chart-<variant>.json`. Reference check: `philly-nice-erect` maps to
/// `assets/preload/data/songs/philly-nice/philly-nice-chart-erect.json`.
pub fn resolve_chart_path<'m>(
    chart_id: &str,
    inventory: &'m BTreeMap<String, String>,
) -> Result<&'m str, ReplayError> {
    let mut found: Option<&str> = None;
    for path in inventory.keys() {
        let Some(id) = fixture_id_of(path) else {
            continue;
        };
        if id == chart_id {
            if found.is_some() {
                return Err(ReplayError(format!(
                    "chart id {chart_id:?} matches multiple provenance entries; \
                     refusing to pick one"
                )));
            }
            found = Some(path.as_str());
        }
    }
    found.ok_or_else(|| {
        ReplayError(format!(
            "chart id {chart_id:?} is not in the committed provenance inventory"
        ))
    })
}

/// Extract the chart-count inventory, keeping the exact string values of the
/// frozen provenance; no fields are added or dropped beyond what the gate uses.
pub(crate) fn inventory_map(
    root: &serde_json::Value,
) -> Result<BTreeMap<String, String>, ReplayError> {
    let object = root
        .as_object()
        .ok_or_else(|| ReplayError("embedded provenance.json is not an object".into()))?;
    let raw = object
        .get("input_asset_sha256")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| ReplayError("embedded provenance.json lacks input_asset_sha256".into()))?;
    let mut inventory = BTreeMap::new();
    for (path, hash) in raw {
        let Some(hash) = hash.as_str() else {
            return Err(ReplayError(format!(
                "provenance sha256 value under {path:?} is not a string"
            )));
        };
        inventory.insert(path.clone(), hash.to_string());
    }
    Ok(inventory)
}

/// If the asset is a chart file, yield the fixture id it corresponds to.
fn fixture_id_of(path: &str) -> Option<String> {
    let file = path.rsplit('/').next()?; // bopeebo-chart-erect.json
    let stem = file.strip_suffix(".json")?; // bopeebo-chart-erect
    let song_dir = path.rsplit('/').nth(1)?; // bopeebo
    let prefix = [song_dir, "-chart"].concat();
    // "" or "-erect"
    let variant = stem.strip_prefix(prefix.as_str())?;
    Some(if variant.is_empty() {
        format!("{song_dir}-default")
    } else {
        [song_dir, variant].concat()
    })
}

/// Deterministic perfect-player stream (one pass):
///
/// * one press per note, stamped at the first integer on the 1 ms playback
///   grid that is `>=` the note time (engine decision, documented);
/// * one release after the sustain length when `l > 0`, otherwise released at
///   the press time itself (release-after-press ordering is deterministic);
/// * global order: time, direction, difficulty, state (press before release
///   on ties). Notes arrive pre-sorted, so per-lane subslices are ascending.
pub fn synthesize_stream(notes: &[LaneNote]) -> Vec<StreamEvent> {
    let mut events = Vec::with_capacity(notes.len() * 2);
    for note in notes {
        let press = press_grid_ms(note.time_ms);
        events.push(StreamEvent {
            audio_time_ms: press,
            direction: note.direction,
            difficulty: note.difficulty.clone(),
            state: Key::Down,
        });
        let hold = if note.hold_ms > 0.0 {
            note.hold_ms
        } else {
            0.0
        };
        events.push(StreamEvent {
            audio_time_ms: press + hold,
            direction: note.direction,
            difficulty: note.difficulty.clone(),
            state: Key::Up,
        });
    }
    events.sort_by(|a, b| {
        a.audio_time_ms
            .total_cmp(&b.audio_time_ms)
            .then_with(|| a.direction.cmp(&b.direction))
            .then_with(|| a.difficulty.cmp(&b.difficulty))
            .then_with(|| state_rank(a.state).cmp(&state_rank(b.state)))
    });
    events
}

/// The synthesized grid press stamp (engine decision): notes are observed in
/// the corpus at non-negative times, and the perfect player presses on the
/// first 1 ms grid integer `>=` the note time.
fn press_grid_ms(time_ms: f64) -> f64 {
    time_ms.ceil()
}

/// One deterministic evaluation pass.
///
/// Semantics (engine decisions, documented; real windows measured at M5
/// replace these): a press on lane (difficulty, direction) hits the earliest
/// pending note of that lane with `Std.int(press - note.t) == 0` (the
/// observed `PlayState` comparison with `inputLatencyMs = 0`); notes with
/// `press - t >= 1` can no longer be hit and count as missed; presses that
/// match no pending note count as missed; notes still pending at the end
/// count as missed. Rounds toward zero match Haxe `Std.int`.
pub fn evaluate_pass(notes: &[LaneNote], events: &[StreamEvent]) -> PassEval {
    let mut lane_notes: BTreeMap<(&str, i64), VecDeque<usize>> = BTreeMap::new();
    for (i, note) in notes.iter().enumerate() {
        lane_notes
            .entry((note.difficulty.as_str(), note.direction))
            .or_default()
            .push_back(i);
    }
    let mut hits = 0usize;
    let mut missed_presses = 0usize;
    let mut missed_notes = 0usize;
    for event in events {
        if matches!(event.state, Key::Up) {
            continue; // M4: releases affect ordering only, not hits.
        }
        let Some(queue) = lane_notes.get_mut(&(event.difficulty.as_str(), event.direction)) else {
            missed_presses += 1;
            continue;
        };
        let press = event.audio_time_ms;
        while let Some(i) = queue.front().copied() {
            if press - notes[i].time_ms >= 1.0 {
                queue.pop_front();
                missed_notes += 1;
            } else {
                break;
            }
        }
        let mut hit = false;
        if let Some(i) = queue.front().copied() {
            let within_window = (press - notes[i].time_ms).trunc() == 0.0;
            if within_window {
                queue.pop_front();
                hits += 1;
                hit = true;
            }
        }
        if !hit {
            missed_presses += 1;
        }
    }
    for queue in lane_notes.values() {
        missed_notes += queue.len();
    }
    PassEval {
        hits,
        misses: missed_presses + missed_notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(direction: i64, time_ms: f64, hold_ms: f64) -> LaneNote {
        LaneNote {
            time_ms,
            direction,
            difficulty: "easy".to_string(),
            hold_ms,
        }
    }

    fn press(direction: i64, audio_time_ms: f64) -> StreamEvent {
        StreamEvent {
            audio_time_ms,
            direction,
            difficulty: "easy".to_string(),
            state: Key::Down,
        }
    }

    fn press_at(notes: &[LaneNote], at: f64) -> PassEval {
        let events = [press(0, at)];
        evaluate_pass(notes, &events)
    }

    #[test]
    fn perfect_stream_hits_every_note() {
        let notes = vec![note(0, 0.0, 0.0), note(1, 600.5, 0.0)];
        let events = synthesize_stream(&notes);
        let eval = evaluate_pass(&notes, &events);
        assert_eq!((eval.hits, eval.misses), (notes.len(), 0));
    }

    #[test]
    fn press_far_off_misses_note_and_press() {
        let notes = vec![note(0, 600.0, 0.0)];
        // 110 ms beyond the note: outside the observed int(0) comparison.
        let eval = press_at(&notes, 710.0);
        assert_eq!((eval.hits, eval.misses), (0, 2));
    }

    #[test]
    fn press_within_single_ms_window_hits() {
        let notes = vec![note(0, 600.5, 0.0)];
        // k - t = 0.5, Std.int == 0 under the observed comparison shape.
        let eval = press_at(&notes, 601.0);
        assert_eq!((eval.hits, eval.misses), (1, 0));
    }

    #[test]
    fn note_liberated_after_miss_matters_only_once() {
        let notes = vec![note(0, 600.0, 0.0)];
        // Press at 701 misses the note; later press at 900 is also far —
        // both presses count, the already-missed note counts once.
        let events = [press(0, 701.0), press(0, 900.0)];
        let eval = evaluate_pass(&notes, &events);
        assert_eq!((eval.hits, eval.misses), (0, 3));
    }

    #[test]
    fn stream_order_puts_press_before_release_on_taps() {
        let notes = vec![note(0, 600.0, 0.0)];
        let events = synthesize_stream(&notes);
        assert_eq!(
            (events.len(), events[0].state, events[1].state),
            (2, Key::Down, Key::Up)
        );
        assert_eq!(events[0].audio_time_ms, 600.0);
        assert_eq!(events[1].audio_time_ms, 600.0);
    }

    #[test]
    fn sustain_release_happens_after_hold() {
        let notes = vec![note(0, 500.0, 250.0)];
        let events = synthesize_stream(&notes);
        assert_eq!(events[1].audio_time_ms, 750.0);
    }

    #[test]
    fn fixture_ids_map_from_provenance_style_paths() {
        assert_eq!(
            fixture_id_of("assets/preload/data/songs/philly-nice/philly-nice-chart-erect.json"),
            Some("philly-nice-erect".to_string())
        );
        assert_eq!(
            fixture_id_of("assets/preload/data/songs/2hot/2hot-chart.json"),
            Some("2hot-default".to_string())
        );
        assert_eq!(
            fixture_id_of("assets/preload/data/songs/2hot/2hot-metadata.json"),
            None
        );
        assert_eq!(
            fixture_id_of("assets/preload/data/songs/2hot/2hot-metadata-erect.json"),
            None
        );
    }

    #[test]
    fn resolve_chart_path_rejects_inventory_ambiguity() {
        // A corpus chart and a mod-hosted chart share directory and file name:
        // both resolve to `dup-default`, and the resolver refuses to pick one.
        let duplicated = BTreeMap::from([
            (
                "assets/preload/data/songs/dup/dup-chart.json".to_string(),
                "a1".to_string(),
            ),
            (
                "assets/mods/extra/data/songs/dup/dup-chart.json".to_string(),
                "a2".to_string(),
            ),
        ]);
        assert!(matches!(
            resolve_chart_path("dup-default", &duplicated),
            Err(ReplayError(message)) if message.contains("matches multiple")
        ));
    }

    #[test]
    fn resolve_chart_path_is_unique_and_fail_closed() {
        let inventory = BTreeMap::from([
            (
                "assets/preload/data/songs/2hot/2hot-chart.json".to_string(),
                "h1".to_string(),
            ),
            (
                "assets/preload/data/songs/2hot/2hot-metadata.json".to_string(),
                "h2".to_string(),
            ),
        ]);
        assert_eq!(
            resolve_chart_path("2hot-default", &inventory),
            Ok("assets/preload/data/songs/2hot/2hot-chart.json")
        );
        assert!(resolve_chart_path("unknown-default", &inventory).is_err());
    }

    #[test]
    fn double_evaluation_of_same_stream_is_identical() {
        let notes = vec![
            note(0, 0.0, 0.0),
            note(3, 337.7, 112.5),
            note(1, 600.5, 0.0),
        ];
        let events = synthesize_stream(&notes);
        assert_eq!(
            evaluate_pass(&notes, &events),
            evaluate_pass(&notes, &events)
        );
    }
}
