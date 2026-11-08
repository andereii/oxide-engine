//! `cargo xtask note-native` — the frozen native note-judge reference gate.
//!
//! Evidence chain (`tools/oracle/NOTE-BEHAVIOR.md` §7), all fail-closed:
//!
//! 1. **Frozen fixtures**: `tests/fixtures/note-native/_manifest_native.json`
//!    records the sha256 of the three exported streams and the three frozen
//!    native rows plus their tick/row counts and provenance fields. The gate
//!    refuses missing files, digest drift, malformed JSONL and unprovenanced
//!    manifests.
//! 2. **Asset drift**: the gate re-exports the stream from the real corpus
//!    (explicit `--assets`, provenance-checked chart/metadata) to
//!    `target/note-native-export/<difficulty>.stream.jsonl` and compares its
//!    sha256 with the manifest. A changed chart, recipe or export writes a
//!    different digest and fails the gate.
//! 3. **Rust replay**: the frozen stream is replayed through
//!    [`oxide_game::slice::GameplaySimState`] twice from fresh state; the two
//!    serialized traces must be byte-identical. The resulting rows are compared
//!    field-wise with the frozen native rows. The scripted stream carries the
//!    §6.1 REVISED two-case boundary: `ceil(t+l-160)` observes a stale 161 ms
//!    penalizing snapshot, while `ceil(t+l-159)` observes bitwise 160.0 and
//!    stays zero-penalty/no-combo-break.
//!
//! Comparison rules: integers, booleans and strings are exact; event order and
//! row counts are exact; JSON key order is the normative §2 order (a reordered
//! row fails even when its key set matches); f64 fields use an absolute
//! tolerance of `<= 1e-9` (printed on every run). Health fields are strict until the first labelled
//! clamp crossing: the native row's `health_before`/`health_after` exceeds the
//! real `HEALTH_MAX = 2.0` while the Rust value is at/above the clamp regime
//! (`Rust within [2.0, native + 1e-9]`); from that row onward exactly those
//! two fields are excluded and reported per difficulty, while every other
//! field stays strict. No crossing means no exclusion at all. This is the
//! §6.1 item 6 engine-decision divergence (the native probe does not retain
//! `update()`'s next-frame health clamp), not a blanket tolerance. The gate
//! does not re-run the scripted negative checks owned by
//! `gameplay_sim_bopeebo`; it is additive and independent.
//!
//! Manifest schema (frozen together with the fixtures):
//!
//! ```json
//! {
//!   "schema": 1,
//!   "recipe": "synthesized-bopeebo-slice-1ms-v1",
//!   "reference": "native real-funkin PlayState/Scoring/GRhythmUtil hxcpp execution ...",
//!   "chart_id": "bopeebo-default",
//!   "stream_schema": "note-stream-1",
//!   "difficulties": ["easy", "normal", "hard"],
//!   "streams": {"easy.stream.jsonl": "<sha256>", ...},
//!   "rows": {"easy.jsonl": "<sha256>", ...},
//!   "tick_counts": {"easy.stream.jsonl": N, ...},
//!   "row_counts": {"easy.jsonl": N, ...},
//!   "provenance": {
//!     "generator": "...",
//!     "native_source": "...",
//!     "probe_script_sha256": "<64 lowercase hex>",
//!     "input_source": "synthesized from chart (no human replay yet)",
//!     "stream_recipe": "...",
//!     "evidence_boundary": "..."
//!   }
//! }
//! ```
//!
//! Honesty: the stream is synthesized from the chart (no human replay yet);
//! the native rows are frozen reference execution evidence, **not** full-game
//! native parity, and the Rust replay proves only agreement on this covered
//! bopeebo path.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oxide_game::slice::{GameplaySimState, InputEvent, TraceEvent};
use serde::Deserialize;
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::gameplay_sim::{self, CHART_ID, DIFFICULTIES, STREAM_SCHEMA};
use crate::replay::{INPUT_SOURCE, LaneNote};

const MANIFEST_FILE: &str = "_manifest_native.json";
const MANIFEST_SCHEMA: i64 = 1;
/// Recipe label frozen in the manifest; changing the synthesis requires a
/// reviewed fixture replacement, never an automatic regeneration.
const RECIPE: &str = "synthesized-bopeebo-slice-1ms-v1";
const F64_ABS_TOLERANCE: f64 = 1e-9;
/// Real `PlayState.update()` clamps health to `[0, 2]` at the next frame; the
/// native probe does not retain `update()`, so its rows carry the unclamped
/// internal value. §6.1 item 6 freezes this as a labelled engine-decision
/// divergence, not a scoring discrepancy.
const HEALTH_CLAMP_MAX: f64 = 2.0;
const EXPORT_DIR: &str = "target/note-native-export";
const HONESTY: &str = "synthesized stream; not native-game parity; native rows are frozen \
                       reference execution evidence";
const CLI_USAGE: &str =
    "usage: note-native --assets <dir> [--fixtures <dir>] [--export-only] [--opponent]";

pub fn run(rest: &[String]) -> ExitCode {
    let cli = match Cli::parse(rest) {
        Ok(cli) => cli,
        Err(message) => {
            eprintln!("note-native: {message}");
            return ExitCode::from(2);
        }
    };
    println!("note-native: honesty: {HONESTY}");
    if !cli.assets.is_dir() {
        eprintln!(
            "note-native: prerequisite: assets directory is missing or not a directory: {}",
            cli.assets.display()
        );
        return ExitCode::FAILURE;
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let export_dir = workspace.join(EXPORT_DIR);
    if cli.opponent {
        println!(
            "note-native --opponent: round-2 fixtures {}; chart {CHART_ID}; \
             schema-2 opponent_hit rows replayed through the Rust sim per difficulty",
            cli.fixtures.display(),
        );
        return match check_opponent(&cli.fixtures, &cli.assets) {
            Ok(summary) => {
                println!("{summary}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("note-native --opponent: {error}");
                ExitCode::FAILURE
            }
        };
    }
    if cli.export_only {
        println!(
            "note-native: export-only: frozen fixtures and row comparisons are NOT validated in \
             this mode"
        );
        return match export_all(&cli.assets, &export_dir) {
            Ok(streams) => {
                print_exports(&streams, &export_dir);
                println!(
                    "note-native: export-only complete; copy these streams only through the \
                     reviewed freeze step"
                );
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("note-native: {error}");
                ExitCode::FAILURE
            }
        };
    }
    println!(
        "note-native: fixtures {}; chart {CHART_ID}; difficulties {}",
        cli.fixtures.display(),
        DIFFICULTIES.join(", ")
    );
    match check(&cli.fixtures, &cli.assets, &export_dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("note-native: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Cli {
    assets: PathBuf,
    fixtures: PathBuf,
    export_only: bool,
    opponent: bool,
}

impl Cli {
    fn parse(rest: &[String]) -> Result<Self, String> {
        let mut assets = None;
        let mut fixtures = None;
        let mut export_only = false;
        let mut opponent = false;
        let mut iter = rest.iter();
        while let Some(flag) = iter.next() {
            match flag.as_str() {
                "--assets" => {
                    let Some(value) = iter.next().filter(|value| !value.is_empty()) else {
                        return Err(format!("--assets requires a value; {CLI_USAGE}"));
                    };
                    assets = Some(PathBuf::from(value));
                }
                "--fixtures" => {
                    let Some(value) = iter.next().filter(|value| !value.is_empty()) else {
                        return Err(format!("--fixtures requires a value; {CLI_USAGE}"));
                    };
                    fixtures = Some(PathBuf::from(value));
                }
                "--export-only" => export_only = true,
                "--opponent" => opponent = true,
                other => {
                    return Err(format!("unsupported argument {other:?}; {CLI_USAGE}"));
                }
            }
        }
        if export_only && opponent {
            return Err(format!(
                "--export-only and --opponent are mutually exclusive; {CLI_USAGE}"
            ));
        }
        let assets = assets.ok_or_else(|| {
            format!(
                "missing --assets <dir>: real chart assets must be provided explicitly; {CLI_USAGE}"
            )
        })?;
        let fixtures = fixtures.unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join(if opponent {
                "../tests/fixtures/note-native-opponent"
            } else {
                "../tests/fixtures/note-native"
            })
        });
        Ok(Self {
            assets,
            fixtures,
            export_only,
            opponent,
        })
    }
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn stream_name(difficulty: &str) -> String {
    format!("{difficulty}.stream.jsonl")
}

fn row_name(difficulty: &str) -> String {
    format!("{difficulty}.jsonl")
}

fn expected_stream_names() -> Vec<String> {
    DIFFICULTIES.iter().map(|d| stream_name(d)).collect()
}

fn expected_row_names() -> Vec<String> {
    DIFFICULTIES.iter().map(|d| row_name(d)).collect()
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

fn required_text<'a>(value: &'a Value, key: &str, context: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| format!("{context} lacks a non-empty string {key:?}"))
}

fn digest<'a>(manifest: &'a Value, group: &str, file: &str) -> Result<&'a str, String> {
    manifest
        .get(group)
        .and_then(Value::as_object)
        .and_then(|entries| entries.get(file))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("fixtures manifest {group:?} lacks a digest for {file:?}"))
}

fn count(manifest: &Value, group: &str, file: &str) -> Result<u64, String> {
    manifest
        .get(group)
        .and_then(Value::as_object)
        .and_then(|entries| entries.get(file))
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("fixtures manifest {group:?} lacks an integer count for {file:?}"))
}

/// Loads and validates the frozen manifest; a missing directory or file is a
/// prerequisite failure, not an empty fixture set.
fn load_manifest(
    directory: &Path,
    manifest_file: &str,
    manifest_schema: i64,
) -> Result<Value, String> {
    if !directory.is_dir() {
        return Err(format!(
            "fixtures directory is missing: {}",
            directory.display()
        ));
    }
    let path = directory.join(manifest_file);
    let bytes = fs::read(&path)
        .map_err(|error| format!("cannot read fixtures manifest {}: {error}", path.display()))?;
    let manifest: Value = serde_json::from_slice(&bytes).map_err(|error| {
        format!(
            "fixtures manifest {} is not valid JSON: {error}",
            path.display()
        )
    })?;
    validate_manifest(
        &manifest,
        manifest_schema,
        manifest_schema == MANIFEST_SCHEMA,
    )?;
    Ok(manifest)
}

fn validate_manifest(
    manifest: &Value,
    manifest_schema: i64,
    strict_input_source: bool,
) -> Result<(), String> {
    let schema = manifest
        .get("schema")
        .and_then(Value::as_i64)
        .ok_or_else(|| "fixtures manifest lacks the integer schema".to_string())?;
    if schema != manifest_schema {
        return Err(format!(
            "fixtures manifest schema is {schema}, expected {manifest_schema}"
        ));
    }
    if required_text(manifest, "recipe", "fixtures manifest")? != RECIPE {
        return Err(format!(
            "fixtures manifest recipe is {:?}, expected {RECIPE:?}; the stream recipe is frozen",
            required_text(manifest, "recipe", "fixtures manifest")?
        ));
    }
    required_text(manifest, "reference", "fixtures manifest")?;
    if required_text(manifest, "chart_id", "fixtures manifest")? != CHART_ID {
        return Err(format!("fixtures manifest chart_id must be {CHART_ID:?}"));
    }
    if required_text(manifest, "stream_schema", "fixtures manifest")? != STREAM_SCHEMA {
        return Err(format!(
            "fixtures manifest stream_schema must be {STREAM_SCHEMA:?}"
        ));
    }
    let difficulties = manifest
        .get("difficulties")
        .and_then(Value::as_array)
        .ok_or_else(|| "fixtures manifest lacks the difficulties array".to_string())?;
    let names: Vec<&str> = difficulties
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .ok_or_else(|| "fixtures manifest difficulties must be strings".to_string())
        })
        .collect::<Result<_, _>>()?;
    if names != DIFFICULTIES {
        return Err(format!(
            "fixtures manifest difficulties must list exactly {}; got {names:?}",
            DIFFICULTIES.join(", ")
        ));
    }

    validate_digest_map(manifest, "streams", &expected_stream_names())?;
    validate_digest_map(manifest, "rows", &expected_row_names())?;
    validate_count_map(manifest, "tick_counts", &expected_stream_names())?;
    validate_count_map(manifest, "row_counts", &expected_row_names())?;

    let provenance = manifest
        .get("provenance")
        .and_then(Value::as_object)
        .ok_or_else(|| "fixtures manifest lacks the provenance object".to_string())?;
    let provenance = Value::Object(provenance.clone());
    for key in [
        "generator",
        "native_source",
        "stream_recipe",
        "evidence_boundary",
    ] {
        required_text(&provenance, key, "fixtures manifest provenance")?;
    }
    if strict_input_source {
        if required_text(&provenance, "input_source", "fixtures manifest provenance")?
            != INPUT_SOURCE
        {
            return Err(format!(
                "fixtures manifest provenance input_source must be {INPUT_SOURCE:?}; the stream \
                 is synthesized, not a human replay"
            ));
        }
    } else if !required_text(&provenance, "input_source", "fixtures manifest provenance")?
        .starts_with("synthesized from chart")
    {
        return Err(
            "fixtures manifest provenance input_source must start with \"synthesized from chart\""
                .to_string(),
        );
    }
    let probe_sha = required_text(
        &provenance,
        "probe_script_sha256",
        "fixtures manifest provenance",
    )?;
    if !is_sha256_hex(probe_sha) {
        return Err(
            "fixtures manifest provenance probe_script_sha256 must be 64 lowercase hex chars"
                .to_string(),
        );
    }
    Ok(())
}

fn validate_digest_map(manifest: &Value, group: &str, expected: &[String]) -> Result<(), String> {
    let entries = manifest
        .get(group)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("fixtures manifest lacks the {group} map"))?;
    validate_map_keys(group, entries, expected)?;
    for file in expected {
        let value = entries
            .get(file)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("fixtures manifest {group:?} entry {file:?} is not a string"))?;
        if !is_sha256_hex(value) {
            return Err(format!(
                "fixtures manifest {group:?} entry {file:?} must be a 64-character lowercase \
                 hex sha256"
            ));
        }
    }
    Ok(())
}

fn validate_count_map(manifest: &Value, group: &str, expected: &[String]) -> Result<(), String> {
    let entries = manifest
        .get(group)
        .and_then(Value::as_object)
        .ok_or_else(|| format!("fixtures manifest lacks the {group} map"))?;
    validate_map_keys(group, entries, expected)?;
    for file in expected {
        if entries.get(file).and_then(Value::as_u64).is_none() {
            return Err(format!(
                "fixtures manifest {group:?} entry {file:?} must be a non-negative integer"
            ));
        }
    }
    Ok(())
}

fn validate_map_keys(
    group: &str,
    entries: &serde_json::Map<String, Value>,
    expected: &[String],
) -> Result<(), String> {
    let missing: Vec<&str> = expected
        .iter()
        .map(String::as_str)
        .filter(|file| !entries.contains_key(*file))
        .collect();
    let extra: Vec<&str> = entries
        .keys()
        .map(String::as_str)
        .filter(|file| !expected.iter().any(|known| known.as_str() == *file))
        .collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(format!(
            "fixtures manifest {group:?} must list exactly {}; missing {missing:?}, extra {extra:?}",
            expected.join(", ")
        ));
    }
    Ok(())
}

#[derive(Debug)]
struct FixtureData {
    stream_sha256: String,
    rows_sha256: String,
    stream: ParsedStream,
    rows: Vec<Value>,
}

#[derive(Debug)]
struct ParsedStream {
    tick_count: u64,
    input_count: usize,
    ticks: Vec<ParsedTick>,
}

#[derive(Debug)]
struct ParsedTick {
    sim: u64,
    events: Vec<InputEvent>,
}

fn read_fixtures(
    directory: &Path,
    manifest: &Value,
) -> Result<BTreeMap<&'static str, FixtureData>, String> {
    let mut fixtures = BTreeMap::new();
    for difficulty in DIFFICULTIES {
        let stream_file = stream_name(difficulty);
        let stream_path = directory.join(&stream_file);
        let stream_bytes = fs::read(&stream_path).map_err(|error| {
            format!(
                "cannot read frozen stream {}: {error}",
                stream_path.display()
            )
        })?;
        let stream_sha256 = sha_hex(&stream_bytes);
        let expected_stream = digest(manifest, "streams", &stream_file)?;
        if stream_sha256 != expected_stream {
            return Err(format!(
                "{difficulty}: frozen stream sha256 {stream_sha256} differs from the committed \
                 manifest {expected_stream}; refusing a drifted stream"
            ));
        }
        let stream = parse_stream(&stream_bytes, difficulty)?;
        let expected_ticks = count(manifest, "tick_counts", &stream_file)?;
        if stream.tick_count != expected_ticks {
            return Err(format!(
                "{difficulty}: stream header tick_count {} != committed manifest {expected_ticks}",
                stream.tick_count
            ));
        }

        let rows_file = row_name(difficulty);
        let rows_path = directory.join(&rows_file);
        let rows_bytes = fs::read(&rows_path).map_err(|error| {
            format!(
                "cannot read frozen native rows {}: {error}",
                rows_path.display()
            )
        })?;
        let rows_sha256 = sha_hex(&rows_bytes);
        let expected_rows = digest(manifest, "rows", &rows_file)?;
        if rows_sha256 != expected_rows {
            return Err(format!(
                "{difficulty}: frozen native rows sha256 {rows_sha256} differs from the committed \
                 manifest {expected_rows}; refusing drifted rows"
            ));
        }
        let rows = parse_jsonl(
            &rows_bytes,
            &format!("frozen native rows {}", rows_path.display()),
        )?;
        let expected_row_count = count(manifest, "row_counts", &rows_file)?;
        if rows.len() as u64 != expected_row_count {
            return Err(format!(
                "{difficulty}: frozen native rows {} != committed manifest {expected_row_count}",
                rows.len()
            ));
        }
        fixtures.insert(
            difficulty,
            FixtureData {
                stream_sha256,
                rows_sha256,
                stream,
                rows,
            },
        );
    }
    Ok(fixtures)
}

/// Normative §2 row key order. The Rust serializer and the native probe must
/// both emit these exact sequences; key sets alone are not sufficient.
const ROW_HEADER_KEYS: [&str; 10] = [
    "schema",
    "kind",
    "sim",
    "input_ms",
    "input_time_us",
    "note_id",
    "note_direction",
    "judgement",
    "raw_note_diff",
    "note_diff_int",
];
const ROW_HIT_KEYS: [&str; 17] = [
    "schema",
    "kind",
    "sim",
    "input_ms",
    "input_time_us",
    "note_id",
    "note_direction",
    "judgement",
    "raw_note_diff",
    "note_diff_int",
    "score_delta",
    "health_before",
    "health_after",
    "combo_before",
    "combo_after",
    "combo_max",
    "tallies",
];
const ROW_GHOST_KEYS: [&str; 12] = [
    "schema",
    "kind",
    "sim",
    "input_ms",
    "input_time_us",
    "note_id",
    "note_direction",
    "judgement",
    "raw_note_diff",
    "note_diff_int",
    "score_delta",
    "health_change",
];
const ROW_HOLD_TICK_KEYS: [&str; 14] = [
    "schema",
    "kind",
    "sim",
    "input_ms",
    "input_time_us",
    "note_id",
    "note_direction",
    "judgement",
    "raw_note_diff",
    "note_diff_int",
    "health_delta",
    "score_delta",
    "hold_remaining_ms",
    "hold_elapsed_ms",
];
const ROW_HOLD_DROP_KEYS: [&str; 15] = [
    "schema",
    "kind",
    "sim",
    "input_ms",
    "input_time_us",
    "note_id",
    "note_direction",
    "judgement",
    "raw_note_diff",
    "note_diff_int",
    "score_penalty",
    "health_change",
    "combo_before",
    "combo_after",
    "is_combo_break",
];
const ROW_GAME_OVER_KEYS: [&str; 11] = [
    "schema",
    "kind",
    "sim",
    "input_ms",
    "input_time_us",
    "note_id",
    "note_direction",
    "judgement",
    "raw_note_diff",
    "note_diff_int",
    "death_count",
];
const TALLY_KEYS: [&str; 6] = ["sick", "good", "bad", "shit", "missed", "total_notes_hit"];

fn expected_row_keys(kind: &str) -> Option<&'static [&'static str]> {
    match kind {
        "hit" | "miss" => Some(&ROW_HIT_KEYS),
        "ghost_press" => Some(&ROW_GHOST_KEYS),
        "hold_start" | "hold_end" | "opponent_hit" => Some(&ROW_HEADER_KEYS),
        "hold_tick" => Some(&ROW_HOLD_TICK_KEYS),
        "hold_drop" => Some(&ROW_HOLD_DROP_KEYS),
        "game_over" => Some(&ROW_GAME_OVER_KEYS),
        _ => None,
    }
}

/// Source-order key sequence for one JSON row (top level and `tallies`).
struct RowKeyOrder {
    top: Vec<String>,
    tallies: Vec<String>,
}

struct TalliesKeyOrder(Vec<String>);

impl<'de> Deserialize<'de> for RowKeyOrder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct OrderVisitor;
        impl<'de> Visitor<'de> for OrderVisitor {
            type Value = RowKeyOrder;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a JSON row object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut top = Vec::new();
                let mut tallies = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    top.push(key.clone());
                    if key == "tallies" {
                        tallies = map.next_value::<TalliesKeyOrder>()?.0;
                    } else {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
                Ok(RowKeyOrder { top, tallies })
            }
        }
        deserializer.deserialize_map(OrderVisitor)
    }
}

impl<'de> Deserialize<'de> for TalliesKeyOrder {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct TalliesVisitor;
        impl<'de> Visitor<'de> for TalliesVisitor {
            type Value = TalliesKeyOrder;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a tallies object")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut keys = Vec::new();
                while let Some(key) = map.next_key::<String>()? {
                    keys.push(key);
                    map.next_value::<IgnoredAny>()?;
                }
                Ok(TalliesKeyOrder(keys))
            }
        }
        deserializer.deserialize_map(TalliesVisitor)
    }
}

fn parse_jsonl(bytes: &[u8], label: &str) -> Result<Vec<Value>, String> {
    let text =
        std::str::from_utf8(bytes).map_err(|error| format!("{label} is not UTF-8: {error}"))?;
    let mut rows = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            return Err(format!("{label} line {} is blank; fail-closed", index + 1));
        }
        let value: Value = serde_json::from_str(line)
            .map_err(|error| format!("{label} line {} is not valid JSON: {error}", index + 1))?;
        if !value.is_object() {
            return Err(format!("{label} line {} is not a JSON object", index + 1));
        }
        let order: RowKeyOrder = serde_json::from_str(line).map_err(|error| {
            format!(
                "{label} line {} key order could not be read: {error}",
                index + 1
            )
        })?;
        let kind = value
            .get("kind")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{label} line {} lacks a string kind", index + 1))?;
        let expected = expected_row_keys(kind).ok_or_else(|| {
            format!(
                "{label} line {}: unknown row kind {kind:?}; schema drift",
                index + 1
            )
        })?;
        let actual: Vec<&str> = order.top.iter().map(String::as_str).collect();
        if actual != expected {
            return Err(format!(
                "{label} line {}: {kind} key order differs from the normative §2 schema; \
                 expected {expected:?}, got {actual:?}",
                index + 1
            ));
        }
        if kind == "hit" || kind == "miss" {
            let actual: Vec<&str> = order.tallies.iter().map(String::as_str).collect();
            if actual != TALLY_KEYS {
                return Err(format!(
                    "{label} line {}: tallies key order differs from the normative §2 schema; \
                     expected {TALLY_KEYS:?}, got {actual:?}",
                    index + 1
                ));
            }
        }
        rows.push(value);
    }
    if rows.is_empty() {
        return Err(format!(
            "{label} is empty; a reference trace cannot be empty"
        ));
    }
    Ok(rows)
}

fn parse_stream(bytes: &[u8], difficulty: &str) -> Result<ParsedStream, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| format!("{difficulty}: stream is not UTF-8: {error}"))?;
    let mut lines = text.lines().enumerate();
    let Some((_, header_line)) = lines.next() else {
        return Err(format!("{difficulty}: stream is empty"));
    };
    if header_line.trim().is_empty() {
        return Err(format!("{difficulty}: stream header line is blank"));
    }
    let header: Value = serde_json::from_str(header_line)
        .map_err(|error| format!("{difficulty}: stream header is not valid JSON: {error}"))?;
    match header.get("schema") {
        Some(Value::String(schema)) if schema == STREAM_SCHEMA => {}
        Some(other) => {
            return Err(format!(
                "{difficulty}: stream header schema is {other}, expected {STREAM_SCHEMA:?}"
            ));
        }
        None => {
            return Err(format!(
                "{difficulty}: stream header lacks the {STREAM_SCHEMA:?} schema"
            ));
        }
    }
    match header.get("difficulty").and_then(Value::as_str) {
        Some(actual) if actual == difficulty => {}
        Some(other) => {
            return Err(format!(
                "{difficulty}: stream header difficulty is {other:?}; expected {difficulty:?}"
            ));
        }
        None => {
            return Err(format!(
                "{difficulty}: stream header lacks a difficulty string"
            ));
        }
    }
    match header.get("chart_id").and_then(Value::as_str) {
        Some(CHART_ID) => {}
        Some(other) => {
            return Err(format!(
                "{difficulty}: stream header chart_id is {other:?}; expected {CHART_ID:?}"
            ));
        }
        None => {
            return Err(format!(
                "{difficulty}: stream header lacks a chart_id string"
            ));
        }
    }
    let declared_ticks = header
        .get("tick_count")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{difficulty}: stream header lacks an integer tick_count"))?;
    if declared_ticks > u64::from(u32::MAX) {
        return Err(format!(
            "{difficulty}: stream header tick_count {declared_ticks} exceeds the u32 format field"
        ));
    }
    if header
        .get("songpos_source")
        .and_then(Value::as_str)
        .is_none_or(|source| source.trim().is_empty())
    {
        return Err(format!(
            "{difficulty}: stream header lacks a non-empty songpos_source"
        ));
    }

    let mut ticks = Vec::new();
    let mut input_count = 0usize;
    for (line_index, line) in lines {
        if line.trim().is_empty() {
            return Err(format!(
                "{difficulty}: stream tick line {} is blank; fail-closed",
                line_index + 1
            ));
        }
        let value: Value = serde_json::from_str(line).map_err(|error| {
            format!(
                "{difficulty}: stream tick line {} is not valid JSON: {error}",
                line_index + 1
            )
        })?;
        let sim = value.get("sim").and_then(Value::as_u64).ok_or_else(|| {
            format!(
                "{difficulty}: stream tick line {} lacks sim",
                line_index + 1
            )
        })?;
        let expected_sim = ticks.len() as u64;
        if sim != expected_sim {
            return Err(format!(
                "{difficulty}: stream tick {expected_sim} carries sim {sim}; ticks must be \
                 contiguous from 0 (absent/short/extra ticks fail closed)"
            ));
        }
        let music_ms = value
            .get("music_ms")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite())
            .ok_or_else(|| format!("{difficulty}: stream tick {sim} lacks a finite music_ms"))?;
        if music_ms.to_bits() != (sim as f64).to_bits() {
            return Err(format!(
                "{difficulty}: tick {sim} music_ms {music_ms} is not the exact slice step \
                 position {sim}; the exported conductor music time must be the value the Rust \
                 sim consumed"
            ));
        }
        let events = value
            .get("events")
            .and_then(Value::as_array)
            .ok_or_else(|| format!("{difficulty}: stream tick {sim} lacks an events array"))?;
        let mut parsed_events = Vec::with_capacity(events.len());
        for event in events {
            let kind = event
                .get("kind")
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{difficulty}: tick {sim} event lacks kind"))?;
            let direction = event
                .get("direction")
                .and_then(Value::as_i64)
                .ok_or_else(|| format!("{difficulty}: tick {sim} event lacks direction"))?;
            let input_ms = event
                .get("input_ms")
                .and_then(Value::as_u64)
                .ok_or_else(|| format!("{difficulty}: tick {sim} event lacks input_ms"))?;
            if input_ms != sim {
                return Err(format!(
                    "{difficulty}: tick {sim} event input_ms {input_ms} does not equal the tick \
                     position; the exported event is not on its declared tick"
                ));
            }
            parsed_events.push(match kind {
                "press" => InputEvent::Press {
                    sim_ms: input_ms,
                    direction,
                },
                "release" => InputEvent::Release {
                    sim_ms: input_ms,
                    direction,
                },
                other => {
                    return Err(format!(
                        "{difficulty}: tick {sim} event kind {other:?} is not press/release"
                    ));
                }
            });
        }
        input_count += parsed_events.len();
        ticks.push(ParsedTick {
            sim,
            events: parsed_events,
        });
    }
    if ticks.len() as u64 != declared_ticks {
        return Err(format!(
            "{difficulty}: stream has {} tick lines but the header declares {declared_ticks}; \
             absent or short streams fail closed",
            ticks.len()
        ));
    }
    Ok(ParsedStream {
        tick_count: declared_ticks,
        input_count,
        ticks,
    })
}

fn write_verified(path: &Path, bytes: &[u8], expected_digest: &str) -> Result<(), String> {
    fs::write(path, bytes)
        .map_err(|error| format!("cannot write exported stream {}: {error}", path.display()))?;
    let read_back = fs::read(path)
        .map_err(|error| format!("cannot re-read exported stream {}: {error}", path.display()))?;
    if read_back != bytes {
        return Err(format!(
            "exported stream {} changed between serialization and the write-back read",
            path.display()
        ));
    }
    let actual = sha_hex(&read_back);
    if actual != expected_digest {
        return Err(format!(
            "exported stream {} digest changed across the write: computed {expected_digest} but \
             wrote {actual}",
            path.display()
        ));
    }
    Ok(())
}

fn export_all_from_notes(
    notes: &BTreeMap<&'static str, Vec<LaneNote>>,
    export_dir: &Path,
) -> Result<Vec<gameplay_sim::ExportedStream>, String> {
    fs::create_dir_all(export_dir).map_err(|error| {
        format!(
            "cannot create stream export directory {}: {error}",
            export_dir.display()
        )
    })?;
    let mut streams = Vec::with_capacity(DIFFICULTIES.len());
    for difficulty in DIFFICULTIES {
        let difficulty_notes = notes
            .get(difficulty)
            .ok_or_else(|| format!("internal: no loaded notes for the {difficulty} export"))?;
        let exported = gameplay_sim::export_stream_from_notes(difficulty_notes, difficulty)?;
        write_verified(
            &export_dir.join(stream_name(difficulty)),
            &exported.bytes,
            &exported.digest,
        )?;
        streams.push(exported);
    }
    Ok(streams)
}

fn export_all(
    assets_root: &Path,
    export_dir: &Path,
) -> Result<Vec<gameplay_sim::ExportedStream>, String> {
    let mut notes = BTreeMap::new();
    for difficulty in DIFFICULTIES {
        notes.insert(
            difficulty,
            gameplay_sim::load_verified_notes(assets_root, difficulty)?,
        );
    }
    export_all_from_notes(&notes, export_dir)
}

fn print_exports(streams: &[gameplay_sim::ExportedStream], export_dir: &Path) {
    for exported in streams {
        println!(
            "note-native: {}: exported {} sha256 {} ticks {} inputs {}",
            exported.difficulty,
            export_dir.join(stream_name(&exported.difficulty)).display(),
            exported.digest,
            exported.tick_count,
            exported.input_count
        );
    }
}

fn verify_exports(
    manifest: &Value,
    streams: &[gameplay_sim::ExportedStream],
) -> Result<(), String> {
    if streams.len() != DIFFICULTIES.len() {
        return Err(format!(
            "internal: expected {} exported streams, got {}",
            DIFFICULTIES.len(),
            streams.len()
        ));
    }
    for exported in streams {
        let file = stream_name(&exported.difficulty);
        let expected = digest(manifest, "streams", &file)?;
        if exported.digest != expected {
            return Err(format!(
                "{}: re-exported stream sha256 {} differs from the frozen manifest {expected}; \
                 asset drift or a changed synthesis recipe",
                exported.difficulty, exported.digest
            ));
        }
        let expected_ticks = count(manifest, "tick_counts", &file)?;
        if exported.tick_count != expected_ticks {
            return Err(format!(
                "{}: re-exported tick_count {} != frozen manifest {expected_ticks}",
                exported.difficulty, exported.tick_count
            ));
        }
    }
    Ok(())
}

struct Replay {
    bytes: Vec<u8>,
    event_count: usize,
}

fn replay(assets_root: &Path, difficulty: &str, stream: &ParsedStream) -> Result<Replay, String> {
    let mut sim = GameplaySimState::build(assets_root, difficulty)
        .map_err(|error| format!("failed to build the bopeebo {difficulty} sim: {error}"))?;
    let mut trace = Vec::new();
    for tick in &stream.ticks {
        trace.extend(sim.step(tick.sim, &tick.events));
    }
    if sim.record() != trace {
        return Err(format!(
            "{difficulty}: sim.record() differs from the per-tick dispatched trace; refusing a \
             mixed replay"
        ));
    }
    let mut bytes = Vec::new();
    for (index, event) in trace.iter().enumerate() {
        serde_json::to_writer(&mut bytes, event).map_err(|error| {
            format!("{difficulty}: cannot serialize replay row {index}: {error}")
        })?;
        bytes.push(b'\n');
    }
    Ok(Replay {
        bytes,
        event_count: trace.len(),
    })
}

fn replay_and_compare(
    assets_root: &Path,
    manifest: &Value,
    fixtures: &BTreeMap<&'static str, FixtureData>,
) -> Result<(), String> {
    for difficulty in DIFFICULTIES {
        let fixture = fixtures
            .get(difficulty)
            .ok_or_else(|| format!("internal: no frozen fixture data for {difficulty}"))?;
        let first = replay(assets_root, difficulty, &fixture.stream)?;
        let second = replay(assets_root, difficulty, &fixture.stream)?;
        if first.bytes != second.bytes {
            return Err(format!(
                "{difficulty}: two-run replay over the frozen stream diverged; the Rust trace is \
                 not deterministic"
            ));
        }
        let rust_rows = parse_jsonl(&first.bytes, &format!("{difficulty}: Rust replay rows"))?;
        let expected_rows = count(manifest, "row_counts", &row_name(difficulty))?;
        if rust_rows.len() as u64 != expected_rows {
            return Err(format!(
                "{difficulty}: Rust replay produced {} rows but the frozen manifest row_count is \
                 {expected_rows}",
                rust_rows.len()
            ));
        }
        if rust_rows.len() != fixture.rows.len() {
            return Err(format!(
                "{difficulty}: Rust replay rows {} != frozen native rows {}",
                rust_rows.len(),
                fixture.rows.len()
            ));
        }
        let stats = compare_rows(difficulty, &fixture.rows, &rust_rows)?;
        println!(
            "note-native: {difficulty}: frozen stream sha256 {} ({} ticks, {} inputs); frozen \
             native rows sha256 {} ({} rows)",
            fixture.stream_sha256,
            fixture.stream.tick_count,
            fixture.stream.input_count,
            fixture.rows_sha256,
            fixture.rows.len()
        );
        if stats.clamp_excluded_fields > 0 {
            println!(
                "note-native: {difficulty}: clamp exclusion policy: {} health field \
                 comparison(s) excluded on {} row(s); first sim {} (row {}), last sim {} (row \
                 {}); every other field compared strictly",
                stats.clamp_excluded_fields,
                stats.clamp_excluded_rows,
                stats.clamp_first_sim.unwrap_or(0),
                stats.clamp_first_index.unwrap_or(0),
                stats.clamp_last_sim.unwrap_or(0),
                stats.clamp_last_index.unwrap_or(0)
            );
        } else {
            println!(
                "note-native: {difficulty}: clamp exclusion policy: no crossing; every field \
                 compared strictly"
            );
        }
        println!(
            "note-native: {difficulty}: replayed {} rows twice identically; frozen native rows \
             match under the labelled §6.1 clamp policy; {} compared f64 fields; worst abs delta \
             {:.3e} (gate tolerance <= {F64_ABS_TOLERANCE:e})",
            first.event_count, stats.f64_fields, stats.worst_abs_delta
        );
    }
    Ok(())
}

fn check(fixtures: &Path, assets_root: &Path, export_dir: &Path) -> Result<(), String> {
    let manifest = load_manifest(fixtures, MANIFEST_FILE, MANIFEST_SCHEMA)?;
    let fixture_data = read_fixtures(fixtures, &manifest)?;
    println!(
        "note-native: tolerance: f64 absolute <= {F64_ABS_TOLERANCE:e}; integers/bools/strings \
         exact; row order exact; counts exact"
    );
    let streams = export_all(assets_root, export_dir)?;
    print_exports(&streams, export_dir);
    verify_exports(&manifest, &streams)?;
    replay_and_compare(assets_root, &manifest, &fixture_data)?;
    println!(
        "note-native: PASS: re-exported stream digests match the frozen manifest and the Rust \
         slice replay matches the frozen native rows under the labelled §6.1 health-clamp policy \
         (every non-health field strict)"
    );
    Ok(())
}

/// Round-2 opponent-side check (`note-native --opponent`).
///
/// Validates the frozen `tests/fixtures/note-native-opponent/` fixtures by
/// identity/digests (reusing the round-1 reader), replays the frozen streams
/// through the schema-2 sim [`GameplaySimState::build_chart`], serializes the
/// Rust [`TraceEvent::OpponentHit`] rows, and compares them field-wise with
/// the frozen native `opponent_hit` rows: integers/bools/strings exact, f64
/// absolute <= tolerance, row order and counts exact, key order normative.
/// Fail-closed: manifest identity, sha256 drift, unknown kinds, empty files.
const OPPONENT_MANIFEST_FILE: &str = "_manifest_native_opponent.json";
const OPPONENT_CHART_ID: &str = "bopeebo";
const OPPONENT_DRIVER: &str = "native-note-probe-2";
const OPPONENT_MANIFEST_SCHEMA: i64 = 2;

fn check_opponent(fixtures: &Path, assets_root: &Path) -> Result<String, String> {
    let manifest = load_manifest(fixtures, OPPONENT_MANIFEST_FILE, OPPONENT_MANIFEST_SCHEMA)?;
    if manifest.get("schema").and_then(Value::as_i64) != Some(2) {
        return Err(format!(
            "opponent fixtures manifest schema is {:?}, expected 2",
            manifest.get("schema")
        ));
    }
    if manifest.get("round").and_then(Value::as_i64) != Some(2) {
        return Err(format!(
            "opponent fixtures manifest round is {:?}, expected 2",
            manifest.get("round")
        ));
    }
    if manifest.get("trace_schema").and_then(Value::as_i64) != Some(2) {
        return Err(format!(
            "opponent fixtures manifest trace_schema is {:?}, expected 2",
            manifest.get("trace_schema")
        ));
    }
    if required_text(&manifest, "driver", "opponent fixtures manifest")? != OPPONENT_DRIVER {
        return Err(
            "opponent fixtures manifest driver is not the round-2 probe driver".to_string(),
        );
    }
    if required_text(&manifest, "chart_id", "opponent fixtures manifest")? != CHART_ID {
        return Err(format!(
            "opponent fixtures manifest chart_id is {:?}, expected {CHART_ID:?}",
            manifest.get("chart_id")
        ));
    }
    if required_text(&manifest, "stream_schema", "opponent fixtures manifest")? != STREAM_SCHEMA {
        return Err(format!(
            "opponent fixtures manifest stream_schema is {:?}, expected {STREAM_SCHEMA:?}",
            manifest.get("stream_schema")
        ));
    }
    let fixture_data = read_fixtures(fixtures, &manifest)?;
    // Round-2 binds the frozen streams to the explicit assets the same way
    // round-1 does: re-export the bopeebo streams from the passed root and
    // require the digests to equal the round-2 manifest (byte-identical to
    // the round-1 manifest stream digests). A drifted checkout fails closed.
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let export_dir = workspace.join(EXPORT_DIR);
    let streams = export_all(assets_root, &export_dir)?;
    verify_exports(&manifest, &streams)?;
    println!(
        "note-native --opponent: tolerance: f64 absolute <= {F64_ABS_TOLERANCE:e}; \
         integers/bools/strings exact; row order exact; counts exact"
    );
    let mut lines = Vec::new();
    for difficulty in DIFFICULTIES {
        let data = fixture_data
            .get(difficulty)
            .ok_or_else(|| format!("opponent fixtures lack the {difficulty:?} fixture data"))?;
        let native_count = manifest
            .get("opponent_note_counts")
            .and_then(Value::as_object)
            .and_then(|counts| counts.get(difficulty))
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("opponent manifest lacks opponent_note_counts[{difficulty}]"))?;
        if data.rows.len() as u64 != native_count {
            return Err(format!(
                "{difficulty}: native rows {} != manifest opponent_note_counts {native_count}",
                data.rows.len()
            ));
        }
        let mut sim = GameplaySimState::build_chart(assets_root, OPPONENT_CHART_ID, difficulty)
            .map_err(|error| format!("{difficulty}: Rust sim build failed: {error}"))?;
        let mut serialized = Vec::new();
        for tick in &data.stream.ticks {
            let trace = sim.step(tick.sim, &tick.events);
            for event in trace {
                if matches!(event, TraceEvent::OpponentHit { .. }) {
                    let row = serde_json::to_string(&event)
                        .map_err(|error| format!("{difficulty}: row serialization: {error}"))?;
                    serialized.push(row);
                }
            }
        }
        if serialized.is_empty() {
            return Err(format!(
                "{difficulty}: the Rust replay produced zero opponent rows; stream/failure drift"
            ));
        }
        let rust_bytes = serialized.join("\n").into_bytes();
        let rust_rows = parse_jsonl(
            &rust_bytes,
            &format!("Rust schema-2 opponent rows ({difficulty})"),
        )?;
        let mut stats = CompareStats::new();
        if rust_rows.len() != data.rows.len() {
            return Err(format!(
                "{difficulty}: Rust replay rows {} != frozen native rows {}",
                rust_rows.len(),
                data.rows.len()
            ));
        }
        for (index, (native_row, rust_row)) in data.rows.iter().zip(rust_rows.iter()).enumerate() {
            let path = format!("{difficulty}: row {}", index + 1);
            compare_value(native_row, rust_row, &path, &mut stats)?;
        }
        lines.push(format!(
            "{difficulty}: {} opponent_hit rows match the frozen native round-2 rows \
             (schema-2 field order verified)",
            rust_rows.len()
        ));
    }
    let summary = format!(
        "note-native --opponent: PASS: {}\ntolerance note: f64 absolute <= \
         {F64_ABS_TOLERANCE:e}; not native-game parity: the rows are the real retained \
         PlayState processNotes opponent loops over the synthesized round-1 stream; opponent \
         hold lifecycle and sing scheduling rows remain excluded",
        lines.join("; ")
    );
    Ok(summary)
}

#[derive(Debug)]
struct CompareStats {
    f64_fields: usize,
    worst_abs_delta: f64,
    /// `health_before`/`health_after` comparisons skipped after the first
    /// labelled clamp crossing.
    clamp_excluded_fields: usize,
    clamp_excluded_rows: usize,
    clamp_first_index: Option<usize>,
    clamp_last_index: Option<usize>,
    clamp_first_sim: Option<u64>,
    clamp_last_sim: Option<u64>,
}

impl CompareStats {
    fn new() -> Self {
        Self {
            f64_fields: 0,
            worst_abs_delta: 0.0,
            clamp_excluded_fields: 0,
            clamp_excluded_rows: 0,
            clamp_first_index: None,
            clamp_last_index: None,
            clamp_first_sim: None,
            clamp_last_sim: None,
        }
    }

    fn merge(&mut self, other: Self) {
        self.f64_fields += other.f64_fields;
        if other.worst_abs_delta > self.worst_abs_delta {
            self.worst_abs_delta = other.worst_abs_delta;
        }
    }
}

#[derive(Debug)]
struct ClampCrossing {
    sim: u64,
    fields: Vec<(String, f64, f64)>,
}

fn compare_rows(
    difficulty: &str,
    native: &[Value],
    rust: &[Value],
) -> Result<CompareStats, String> {
    if native.len() != rust.len() {
        return Err(format!(
            "{difficulty}: native row count {} != Rust replay {}",
            native.len(),
            rust.len()
        ));
    }
    let mut stats = CompareStats::new();
    let mut clamping = false;
    for (index, (native_row, rust_row)) in native.iter().zip(rust.iter()).enumerate() {
        let path = format!("{difficulty}: row {}", index + 1);
        if clamping {
            compare_row_without_health(native_row, rust_row, &path, index + 1, &mut stats)?;
            continue;
        }
        let mut row_stats = CompareStats::new();
        match compare_value(native_row, rust_row, &path, &mut row_stats) {
            Ok(()) => stats.merge(row_stats),
            Err(strict_error) => match detect_clamp_crossing(native_row, rust_row, &path)? {
                Some(crossing) => {
                    println!(
                        "note-native: {difficulty}: health clamp decision divergence excluded from \
                         sim {} (row {}; native > {HEALTH_CLAMP_MAX}, Rust clamped in \
                         [{HEALTH_CLAMP_MAX}, native+{F64_ABS_TOLERANCE:e}]; fields {})",
                        crossing.sim,
                        index + 1,
                        crossing
                            .fields
                            .iter()
                            .map(|(field, native, rust)| format!(
                                "{field} native={native} Rust={rust}"
                            ))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    clamping = true;
                    // The crossing row's health deltas are excluded, so its
                    // scratch stats (which contain them) are discarded.
                    compare_row_without_health(native_row, rust_row, &path, index + 1, &mut stats)?;
                }
                None => return Err(strict_error),
            },
        }
    }
    Ok(stats)
}

/// Detects the first clamp-crossing row and validates its exact shape.
///
/// The row is accepted only when every differing field is one of
/// `health_before`/`health_after`, each native value is `> HEALTH_MAX`, and
/// each Rust value lies in `[HEALTH_MAX, native + 1e-9]`. Any non-health
/// divergence in the same row fails immediately; a health mismatch below the
/// clamp is left to the normal 1e-9 tolerance error.
fn detect_clamp_crossing(
    native: &Value,
    rust: &Value,
    path: &str,
) -> Result<Option<ClampCrossing>, String> {
    let (Value::Object(native_map), Value::Object(rust_map)) = (native, rust) else {
        return Ok(None);
    };
    if native_map.len() != rust_map.len() {
        return Ok(None);
    }
    let mut fields = Vec::new();
    for (key, rust_value) in rust_map {
        let Some(native_value) = native_map.get(key) else {
            return Ok(None);
        };
        if key == "health_before" || key == "health_after" {
            if native_value == rust_value {
                continue;
            }
            let native_number = native_value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("{path}.{key}: native health is not a finite number"))?;
            let rust_number = rust_value
                .as_f64()
                .filter(|value| value.is_finite())
                .ok_or_else(|| format!("{path}.{key}: Rust health is not a finite number"))?;
            if native_number <= HEALTH_CLAMP_MAX {
                return Ok(None);
            }
            if rust_number < HEALTH_CLAMP_MAX || rust_number > native_number + F64_ABS_TOLERANCE {
                return Err(format!(
                    "{path}.{key}: health clamp crossing shape violated: native={native_number} \
                     must exceed {HEALTH_CLAMP_MAX} and Rust={rust_number} must be within \
                     [{HEALTH_CLAMP_MAX}, native+{F64_ABS_TOLERANCE:e}]"
                ));
            }
            fields.push((key.clone(), native_number, rust_number));
        } else {
            let mut scratch = CompareStats::new();
            compare_value(
                native_value,
                rust_value,
                &format!("{path}.{key}"),
                &mut scratch,
            )?;
        }
    }
    if fields.is_empty() {
        return Ok(None);
    }
    let sim = rust_map
        .get("sim")
        .and_then(Value::as_u64)
        .ok_or_else(|| format!("{path}: clamp-crossing row lacks an integer sim"))?;
    Ok(Some(ClampCrossing { sim, fields }))
}

/// Compares a row after the clamp crossing: exactly `health_before` and
/// `health_after` are skipped (and counted), every other field stays strict.
fn compare_row_without_health(
    native: &Value,
    rust: &Value,
    path: &str,
    row_index: usize,
    stats: &mut CompareStats,
) -> Result<(), String> {
    let (Value::Object(native_map), Value::Object(rust_map)) = (native, rust) else {
        return compare_value(native, rust, path, stats);
    };
    if native_map.len() != rust_map.len() {
        let missing: Vec<&str> = rust_map
            .keys()
            .map(String::as_str)
            .filter(|key| !native_map.contains_key(*key))
            .collect();
        let extra: Vec<&str> = native_map
            .keys()
            .map(String::as_str)
            .filter(|key| !rust_map.contains_key(*key))
            .collect();
        return Err(format!(
            "{path}: field set differs; missing {missing:?}, extra {extra:?}"
        ));
    }
    let mut excluded_this_row = false;
    for (key, rust_value) in rust_map {
        let native_value = native_map
            .get(key)
            .ok_or_else(|| format!("{path}: missing field {key:?}"))?;
        if key == "health_before" || key == "health_after" {
            excluded_this_row = true;
            stats.clamp_excluded_fields += 1;
            continue;
        }
        compare_value(native_value, rust_value, &format!("{path}.{key}"), stats)?;
    }
    if excluded_this_row {
        let sim = rust_map
            .get("sim")
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("{path}: health-excluded row lacks an integer sim"))?;
        stats.clamp_excluded_rows += 1;
        if stats.clamp_first_index.is_none() {
            stats.clamp_first_index = Some(row_index);
        }
        stats.clamp_last_index = Some(row_index);
        if stats.clamp_first_sim.is_none() {
            stats.clamp_first_sim = Some(sim);
        }
        stats.clamp_last_sim = Some(sim);
    }
    Ok(())
}

fn compare_value(
    native: &Value,
    rust: &Value,
    path: &str,
    stats: &mut CompareStats,
) -> Result<(), String> {
    match (native, rust) {
        (Value::Object(native_map), Value::Object(rust_map)) => {
            if native_map.len() != rust_map.len() {
                let missing: Vec<&str> = rust_map
                    .keys()
                    .map(String::as_str)
                    .filter(|key| !native_map.contains_key(*key))
                    .collect();
                let extra: Vec<&str> = native_map
                    .keys()
                    .map(String::as_str)
                    .filter(|key| !rust_map.contains_key(*key))
                    .collect();
                return Err(format!(
                    "{path}: field set differs; missing {missing:?}, extra {extra:?}"
                ));
            }
            for (key, rust_value) in rust_map {
                let native_value = native_map
                    .get(key)
                    .ok_or_else(|| format!("{path}: missing field {key:?}"))?;
                compare_value(native_value, rust_value, &format!("{path}.{key}"), stats)?;
            }
            Ok(())
        }
        (Value::Array(native_rows), Value::Array(rust_rows)) => {
            if native_rows.len() != rust_rows.len() {
                return Err(format!(
                    "{path}: array length {} != {}",
                    native_rows.len(),
                    rust_rows.len()
                ));
            }
            for (index, (native_item, rust_item)) in
                native_rows.iter().zip(rust_rows.iter()).enumerate()
            {
                compare_value(native_item, rust_item, &format!("{path}[{index}]"), stats)?;
            }
            Ok(())
        }
        (Value::String(native), Value::String(rust)) if native == rust => Ok(()),
        (Value::Bool(native), Value::Bool(rust)) if native == rust => Ok(()),
        (Value::Number(native), Value::Number(rust)) => compare_number(native, rust, path, stats),
        (Value::Null, Value::Null) => Ok(()),
        _ => Err(format!(
            "{path}: native {native} and Rust {rust} differ in type or exact value"
        )),
    }
}

fn compare_number(
    native: &serde_json::Number,
    rust: &serde_json::Number,
    path: &str,
    stats: &mut CompareStats,
) -> Result<(), String> {
    if !rust.is_f64() {
        if native.is_f64() {
            return Err(format!(
                "{path}: integer field native={native} Rust={rust}; integers/bools/strings are exact"
            ));
        }
        if !numbers_equal(native, rust) {
            return Err(format!(
                "{path}: integer field native={native} Rust={rust}; integers/bools/strings are exact"
            ));
        }
        return Ok(());
    }
    let rust_value = rust
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{path}: Rust f64 value is not finite"))?;
    let native_value = native
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("{path}: native value {native} is not a finite number"))?;
    let delta = (native_value - rust_value).abs();
    stats.f64_fields += 1;
    if delta > stats.worst_abs_delta {
        stats.worst_abs_delta = delta;
    }
    if delta > F64_ABS_TOLERANCE {
        return Err(format!(
            "{path}: native={native_value} Rust={rust_value} abs delta {delta:e} exceeds the \
             gate tolerance {F64_ABS_TOLERANCE:e}"
        ));
    }
    Ok(())
}

fn numbers_equal(native: &serde_json::Number, rust: &serde_json::Number) -> bool {
    match (native.as_u64(), rust.as_u64()) {
        (Some(native), Some(rust)) => native == rust,
        _ => native.as_i64() == rust.as_i64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    /// Same shape as the gameplay_sim scripted fixture: nine taps (six
    /// threshold edges plus 110/160/161) and four sustains covering the big
    /// drop, both §6.1 REVISED boundary cases and the fractional full hold,
    /// spaced so the lane/window preconditions hold.
    const SYNTHETIC_NOTES: [(i64, f64, f64); 13] = [
        (0, 1000.0, 0.0),
        (1, 2000.0, 0.0),
        (2, 3000.0, 0.0),
        (3, 4000.0, 0.0),
        (0, 5000.0, 0.0),
        (1, 6000.0, 0.0),
        (2, 7000.0, 0.0),
        (3, 8000.0, 0.0),
        (0, 9000.0, 0.0),
        (1, 11000.0, 500.0),
        (2, 13000.0, 400.0),
        (3, 15000.0, 300.5),
        (3, 17000.0, 400.0),
    ];

    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let root = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../target/note-native-tests")
                .join(format!(
                    "{tag}-{}-{}",
                    std::process::id(),
                    NEXT_TEST.fetch_add(1, Ordering::Relaxed)
                ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create test scratch");
            Self(root)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn synthetic_chart() -> String {
        let notes: Vec<String> = SYNTHETIC_NOTES
            .iter()
            .map(|(direction, time, hold)| {
                if *hold > 0.0 {
                    format!(r#"{{"t":{time},"d":{direction},"l":{hold}}}"#)
                } else {
                    format!(r#"{{"t":{time},"d":{direction}}}"#)
                }
            })
            .collect();
        let notes = notes.join(",");
        format!(
            r#"{{"version":"2.0.0","scrollSpeed":{{"easy":1.2,"normal":1.3,"hard":1.6}},"events":[],"notes":{{"easy":[{notes}],"normal":[{notes}],"hard":[{notes}]}}}}"#
        )
    }

    const SYNTHETIC_METADATA: &str = r#"{
      "version": "2.2.4",
      "songName": "Bopeebo",
      "playData": { "difficulties": ["easy", "normal", "hard"] },
      "timeChanges": [{ "t": 0, "bpm": 100 }]
    }"#;

    fn write_assets(root: &Path) {
        let songs = root.join("preload/data/songs/bopeebo");
        fs::create_dir_all(&songs).expect("create asset tree");
        fs::write(songs.join("bopeebo-chart.json"), synthetic_chart()).expect("write chart");
        fs::write(songs.join("bopeebo-metadata.json"), SYNTHETIC_METADATA).expect("write metadata");
    }

    fn synthetic_notes() -> BTreeMap<&'static str, Vec<LaneNote>> {
        let mut notes = BTreeMap::new();
        for difficulty in DIFFICULTIES {
            notes.insert(
                difficulty,
                SYNTHETIC_NOTES
                    .iter()
                    .map(|(direction, time, hold)| LaneNote {
                        time_ms: *time,
                        direction: *direction,
                        difficulty: difficulty.to_string(),
                        hold_ms: *hold,
                    })
                    .collect(),
            );
        }
        notes
    }

    /// Freezes the synthetic Rust replay as the "native" reference. This is a
    /// unit-test fixture, never execution evidence.
    fn build_skeleton(
        fixtures: &Path,
        assets: &Path,
        export_dir: &Path,
    ) -> (Value, Vec<gameplay_sim::ExportedStream>) {
        fs::create_dir_all(fixtures).expect("create fixture dir");
        let notes = synthetic_notes();
        let streams = export_all_from_notes(&notes, export_dir).expect("synthetic export");
        let mut stream_digests = serde_json::Map::new();
        let mut tick_counts = serde_json::Map::new();
        let mut row_digests = serde_json::Map::new();
        let mut row_counts = serde_json::Map::new();
        for exported in &streams {
            let difficulty = exported.difficulty.as_str();
            let stream_file = stream_name(difficulty);
            fs::write(fixtures.join(&stream_file), &exported.bytes).expect("write stream fixture");
            stream_digests.insert(stream_file.clone(), json!(exported.digest));
            tick_counts.insert(stream_file, json!(exported.tick_count));

            let parsed = parse_stream(&exported.bytes, difficulty).expect("parse synthetic stream");
            let replay = replay(assets, difficulty, &parsed).expect("synthetic replay");
            let row_file = row_name(difficulty);
            fs::write(fixtures.join(&row_file), &replay.bytes).expect("write row fixture");
            row_digests.insert(row_file.clone(), json!(sha_hex(&replay.bytes)));
            row_counts.insert(row_file, json!(replay.event_count));
        }
        let manifest = json!({
            "schema": MANIFEST_SCHEMA,
            "recipe": RECIPE,
            "reference": "synthetic unit-test rows generated from the Rust replay",
            "chart_id": CHART_ID,
            "stream_schema": STREAM_SCHEMA,
            "difficulties": DIFFICULTIES,
            "streams": stream_digests,
            "rows": row_digests,
            "tick_counts": tick_counts,
            "row_counts": row_counts,
            "provenance": {
                "generator": "note_native_check synthetic unit test",
                "native_source": "synthetic Rust replay rows; NOT native execution evidence",
                "probe_script_sha256": "0".repeat(64),
                "input_source": INPUT_SOURCE,
                "stream_recipe": "synthetic scripted fixture",
                "evidence_boundary": "unit test only; never acceptance evidence"
            }
        });
        fs::write(
            fixtures.join(MANIFEST_FILE),
            serde_json::to_vec_pretty(&manifest).expect("manifest serializes"),
        )
        .expect("write manifest");
        (manifest, streams)
    }

    #[test]
    fn synthetic_fixture_skeleton_passes_the_full_comparison_pipeline() {
        let scratch = Scratch::new("pass");
        let assets = scratch.path("assets");
        write_assets(&assets);
        let fixtures = scratch.path("fixtures");
        let export_dir = scratch.path("export");
        let (manifest, streams) = build_skeleton(&fixtures, &assets, &export_dir);

        let loaded = load_manifest(&fixtures, MANIFEST_FILE, MANIFEST_SCHEMA)
            .expect("synthetic manifest validates");
        let fixture_data = read_fixtures(&fixtures, &loaded).expect("synthetic fixtures load");
        verify_exports(&manifest, &streams).expect("synthetic export digests match");
        replay_and_compare(&assets, &manifest, &fixture_data)
            .expect("synthetic rows compare equal");
    }

    #[test]
    fn synthetic_rows_are_non_empty_and_replay_twice_identically() {
        let scratch = Scratch::new("replay");
        let assets = scratch.path("assets");
        write_assets(&assets);
        let fixtures = scratch.path("fixtures");
        let export_dir = scratch.path("export");
        let (manifest, _) = build_skeleton(&fixtures, &assets, &export_dir);
        let loaded = load_manifest(&fixtures, MANIFEST_FILE, MANIFEST_SCHEMA).unwrap();
        let data = read_fixtures(&fixtures, &loaded).unwrap();
        for difficulty in DIFFICULTIES {
            let fixture = data.get(difficulty).unwrap();
            assert!(!fixture.rows.is_empty());
            let first = replay(&assets, difficulty, &fixture.stream).unwrap();
            let second = replay(&assets, difficulty, &fixture.stream).unwrap();
            assert_eq!(first.bytes, second.bytes);
            assert_eq!(first.event_count, fixture.rows.len());
            assert_eq!(
                first.event_count as u64,
                count(&manifest, "row_counts", &row_name(difficulty)).unwrap()
            );
        }
    }

    #[test]
    fn header_schema_drift_fails_closed() {
        let scratch = Scratch::new("schema");
        let assets = scratch.path("assets");
        write_assets(&assets);
        let fixtures = scratch.path("fixtures");
        let export_dir = scratch.path("export");
        let (_, streams) = build_skeleton(&fixtures, &assets, &export_dir);
        let text = std::str::from_utf8(&streams[0].bytes).unwrap().replacen(
            r#""schema":"note-stream-1""#,
            r#""schema":"note-stream-2""#,
            1,
        );
        let error = parse_stream(text.as_bytes(), "easy").expect_err("schema drift must fail");
        assert!(error.contains("expected \"note-stream-1\""), "{error}");
    }

    #[test]
    fn stream_digest_drift_fails_closed() {
        let scratch = Scratch::new("stream-digest");
        let assets = scratch.path("assets");
        write_assets(&assets);
        let fixtures = scratch.path("fixtures");
        let export_dir = scratch.path("export");
        let (manifest, mut streams) = build_skeleton(&fixtures, &assets, &export_dir);
        streams[0].bytes.push(b' ');
        streams[0].digest = sha_hex(&streams[0].bytes);
        let error = verify_exports(&manifest, &streams)
            .expect_err("an altered exported stream must not match the frozen manifest");
        assert!(error.contains("re-exported stream sha256"), "{error}");
    }

    #[test]
    fn row_digest_drift_fails_closed() {
        let scratch = Scratch::new("row-digest");
        let assets = scratch.path("assets");
        write_assets(&assets);
        let fixtures = scratch.path("fixtures");
        let export_dir = scratch.path("export");
        build_skeleton(&fixtures, &assets, &export_dir);
        let manifest = load_manifest(&fixtures, MANIFEST_FILE, MANIFEST_SCHEMA).unwrap();
        fs::write(
            fixtures.join(row_name("easy")),
            br#"{"schema":"1","kind":"hit","sim":0}
"#,
        )
        .unwrap();
        let error =
            read_fixtures(&fixtures, &manifest).expect_err("altered native rows must fail closed");
        assert!(error.contains("frozen native rows sha256"), "{error}");
    }

    #[test]
    fn missing_fixture_files_fail_closed() {
        let scratch = Scratch::new("missing");
        let assets = scratch.path("assets");
        write_assets(&assets);
        let fixtures = scratch.path("fixtures");
        let export_dir = scratch.path("export");
        build_skeleton(&fixtures, &assets, &export_dir);
        let manifest = load_manifest(&fixtures, MANIFEST_FILE, MANIFEST_SCHEMA).unwrap();
        fs::remove_file(fixtures.join(row_name("hard"))).unwrap();
        let error = read_fixtures(&fixtures, &manifest)
            .expect_err("a missing frozen row file must fail closed");
        assert!(error.contains("cannot read frozen native rows"), "{error}");
    }

    #[test]
    fn difficulty_count_mismatch_fails_closed() {
        let mut manifest = json!({
            "schema": MANIFEST_SCHEMA,
            "recipe": RECIPE,
            "reference": "synthetic",
            "chart_id": CHART_ID,
            "stream_schema": STREAM_SCHEMA,
            "difficulties": ["easy", "normal"],
            "streams": {},
            "rows": {},
            "tick_counts": {},
            "row_counts": {},
            "provenance": {
                "generator": "synthetic",
                "native_source": "synthetic",
                "probe_script_sha256": "0".repeat(64),
                "input_source": INPUT_SOURCE,
                "stream_recipe": "synthetic",
                "evidence_boundary": "synthetic"
            }
        });
        let error = validate_manifest(&manifest, MANIFEST_SCHEMA, true)
            .expect_err("two difficulties must fail");
        assert!(error.contains("must list exactly"), "{error}");

        manifest["difficulties"] = json!(["easy", "normal", "hard"]);
        manifest["streams"] = json!({"easy.stream.jsonl": "0".repeat(64)});
        let error = validate_manifest(&manifest, MANIFEST_SCHEMA, true)
            .expect_err("missing stream entries must fail");
        assert!(error.contains("must list exactly"), "{error}");
    }

    #[test]
    fn row_comparison_uses_exact_integers_and_absolute_1e_9_floats() {
        let rust = json!({
            "schema": "1",
            "kind": "hit",
            "sim": 10,
            "raw_note_diff": 0.0,
            "health_after": 1.0,
            "combo_after": 2,
            "ok": true,
            "tallies": {"sick": 2, "total_notes_hit": 2}
        });
        let mut native = rust.clone();
        native["raw_note_diff"] = json!(1e-9);
        compare_rows("easy", &[native.clone()], std::slice::from_ref(&rust))
            .expect("exactly 1e-9 is within the gate tolerance");

        native["raw_note_diff"] = json!(1.0000001e-9);
        let error = compare_rows("easy", &[native.clone()], std::slice::from_ref(&rust))
            .expect_err("more than 1e-9 must fail");
        assert!(error.contains("gate tolerance"), "{error}");

        native["raw_note_diff"] = json!(0.0);
        native["combo_after"] = json!(3);
        let error = compare_rows("easy", &[native.clone()], std::slice::from_ref(&rust))
            .expect_err("integers must be exact");
        assert!(
            error.contains("integers/bools/strings are exact"),
            "{error}"
        );

        native["combo_after"] = json!(2);
        native["extra"] = json!(0);
        let error = compare_rows("easy", &[native], std::slice::from_ref(&rust))
            .expect_err("extra fields must fail");
        assert!(error.contains("field set differs"), "{error}");
    }

    #[test]
    fn parse_jsonl_rejects_reordered_keys_with_an_explicit_message() {
        let reordered = br#"{"kind":"hold_start","schema":"1","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"none","raw_note_diff":0.0,"note_diff_int":0}"#;
        let error = parse_jsonl(reordered, "test rows").expect_err("reordered keys must fail");
        assert!(
            error.contains("key order differs from the normative §2 schema"),
            "{error}"
        );

        let bad_tallies = br#"{"schema":"1","kind":"hit","sim":0,"input_ms":0.0,"input_time_us":0.0,"note_id":0,"note_direction":0,"judgement":"sick","raw_note_diff":0.0,"note_diff_int":0,"score_delta":500,"health_before":1.0,"health_after":1.03,"combo_before":0,"combo_after":1,"combo_max":1,"tallies":{"good":0,"sick":1,"bad":0,"shit":0,"missed":0,"total_notes_hit":1}}"#;
        let error = parse_jsonl(bad_tallies, "test rows").expect_err("reordered tallies must fail");
        assert!(
            error.contains("tallies key order differs from the normative §2 schema"),
            "{error}"
        );
    }

    #[test]
    fn stream_parser_rejects_music_time_and_event_tick_drift() {
        let header = format!(
            r#"{{"schema":"{STREAM_SCHEMA}","difficulty":"easy","chart_id":"{CHART_ID}","tick_count":2,"songpos_source":"test"}}"#
        );
        let valid = format!(
            "{header}\n{}\n{}\n",
            r#"{"sim":0,"music_ms":0.0,"events":[{"kind":"press","direction":0,"input_ms":0}]}"#,
            r#"{"sim":1,"music_ms":1.0,"events":[]}"#
        );
        parse_stream(valid.as_bytes(), "easy").expect("valid stream");
        let music_drift = valid.replacen(r#""music_ms":1.0"#, r#""music_ms":1.5"#, 1);
        let error = parse_stream(music_drift.as_bytes(), "easy")
            .expect_err("a non-slice music time must fail");
        assert!(error.contains("exact slice step position"), "{error}");
        let event_drift = valid.replacen(r#""input_ms":0"#, r#""input_ms":1"#, 1);
        let error =
            parse_stream(event_drift.as_bytes(), "easy").expect_err("event/tick drift must fail");
        assert!(
            error.contains("does not equal the tick position"),
            "{error}"
        );
    }

    fn hit_row(sim: u64, health_before: f64, health_after: f64) -> Value {
        json!({
            "schema": "1",
            "kind": "hit",
            "sim": sim,
            "input_ms": sim as f64,
            "input_time_us": 0.0,
            "note_id": 0,
            "note_direction": 0,
            "judgement": "sick",
            "raw_note_diff": 0.0,
            "note_diff_int": 0,
            "score_delta": 500,
            "health_before": health_before,
            "health_after": health_after,
            "combo_before": 0,
            "combo_after": 1,
            "combo_max": 1,
            "tallies": {
                "sick": 1, "good": 0, "bad": 0, "shit": 0, "missed": 0,
                "total_notes_hit": 1
            }
        })
    }

    #[test]
    fn clamp_policy_stays_strict_without_a_crossing() {
        let rows = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 1.03, 1.06012),
            hit_row(300, 1.06012, 1.09024),
        ];
        let stats = compare_rows("easy", &rows, &rows).expect("identical rows are strict");
        assert_eq!(stats.clamp_excluded_fields, 0);
        assert_eq!(stats.clamp_excluded_rows, 0);
        assert_eq!(stats.clamp_first_index, None);
        assert_eq!(stats.clamp_last_index, None);
        assert_eq!(stats.clamp_first_sim, None);
        assert_eq!(stats.clamp_last_sim, None);

        // Below HEALTH_MAX the policy never applies: ordinary f64 drift is
        // still a strict failure.
        let mut drifted = rows.clone();
        drifted[1]["health_before"] = json!(1.03 + 2e-9);
        let error = compare_rows("easy", &rows, &drifted).expect_err("sub-clamp drift fails");
        assert!(error.contains("gate tolerance"), "{error}");
        assert!(!error.contains("clamp"), "{error}");
    }

    #[test]
    fn clamp_policy_detects_crossing_and_excludes_following_health_fields() {
        let native = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 2.12704, 2.15704),
            hit_row(300, 2.2, 2.23),
        ];
        let rust = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 2.0, 2.03),
            hit_row(300, 2.0, 2.03),
        ];
        let stats = compare_rows("easy", &native, &rust).expect("crossing shape accepted");
        assert_eq!(
            stats.clamp_excluded_fields, 4,
            "both health fields on the crossing and following hit rows"
        );
        assert_eq!(stats.clamp_excluded_rows, 2);
        assert_eq!(stats.clamp_first_index, Some(2));
        assert_eq!(stats.clamp_last_index, Some(3));
        assert_eq!(stats.clamp_first_sim, Some(200));
        assert_eq!(stats.clamp_last_sim, Some(300));
        assert!(
            stats.worst_abs_delta <= F64_ABS_TOLERANCE,
            "the excluded health delta must not pollute the strict worst-delta stat: {}",
            stats.worst_abs_delta
        );
    }

    #[test]
    fn clamp_policy_rejects_a_bad_crossing_shape() {
        let native = vec![hit_row(100, 1.0, 1.03), hit_row(200, 2.12704, 2.15704)];
        let below_floor = vec![hit_row(100, 1.0, 1.03), hit_row(200, 1.9, 1.93)];
        let error = compare_rows("easy", &native, &below_floor)
            .expect_err("Rust below HEALTH_MAX is not a clamp crossing");
        assert!(error.contains("crossing shape violated"), "{error}");

        let above_native = vec![hit_row(100, 1.0, 1.03), hit_row(200, 2.5, 2.53)];
        let error = compare_rows("easy", &native, &above_native)
            .expect_err("Rust above native is not a clamp crossing");
        assert!(error.contains("crossing shape violated"), "{error}");

        // A health mismatch below the clamp is ordinary drift, not a crossing;
        // the later row is the first eligible crossing.
        let native_below = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 1.9, 1.93),
            hit_row(300, 2.2, 2.23),
        ];
        let rust = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 1.9, 1.93),
            hit_row(300, 2.0, 2.03),
        ];
        let stats = compare_rows("easy", &native_below, &rust).expect("late crossing accepted");
        assert_eq!(stats.clamp_first_sim, Some(300));
    }

    #[test]
    fn post_crossing_non_health_divergence_still_fails() {
        let native = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 2.12704, 2.15704),
            hit_row(300, 2.2, 2.23),
        ];
        let mut rust = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 2.0, 2.03),
            hit_row(300, 2.0, 2.03),
        ];
        rust[2]["combo_after"] = json!(7);
        let error = compare_rows("easy", &native, &rust)
            .expect_err("non-health divergence after the crossing must fail");
        assert!(error.contains("combo_after"), "{error}");
        assert!(
            error.contains("integers/bools/strings are exact"),
            "{error}"
        );

        // A non-health divergence on the crossing row itself is not eligible
        // for the exclusion policy either.
        let mut crossing_rust = vec![
            hit_row(100, 1.0, 1.03),
            hit_row(200, 2.0, 2.03),
            hit_row(300, 2.0, 2.03),
        ];
        crossing_rust[1]["combo_after"] = json!(9);
        let error = compare_rows("easy", &native, &crossing_rust)
            .expect_err("only health fields may differ at the crossing row");
        assert!(error.contains("combo_after"), "{error}");
    }
}
