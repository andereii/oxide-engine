//! Model-timeline parity only. Native utility evidence and limitations live in
//! tools/oracle/BEHAVIOR.md; this is not an independent full-game oracle or M4 completion.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use oxide_audio::conductor::{Conductor, ConductorOffsets, Dispatch, TimeChange};
use serde_json::Value;

const CORPUS: &str = include_str!("../../tools/oracle/corpus.txt");
const TRUSTED_MANIFEST: &str = include_str!("../../tests/fixtures/conductor/_manifest.json");
const SIMULATION: &str = "countdown-1ms-playback-1ms-v1";
const REFERENCE: &str = "model-only; utility probes documented in tools/oracle/BEHAVIOR.md";
const MAX_UPDATES: u64 = 10_000_000;
const TIME_TOLERANCE_MS: f64 = 1.0;
const LENGTH_RELATIVE_TOLERANCE: f64 = 1e-6;

pub(crate) struct Fixture {
    pub(crate) id: String,
    pub(crate) length: f64,
    offsets: ConductorOffsets,
    changes: Vec<TimeChange>,
    counts: [usize; 3],
    pub(crate) rows: usize,
}

pub(crate) struct Row {
    pub(crate) value: Value,
    pub(crate) kind: usize,
    pub(crate) index: i32,
    pub(crate) sim: u64,
    pub(crate) playing: bool,
}

#[derive(Default)]
pub(crate) struct Stats {
    pub(crate) rows: usize,
    pub(crate) worst_delta: f64,
}

/// Verify frozen fixtures only; never build a reference or generate missing files.
pub fn run(rest: &[String]) -> bool {
    let path = match rest {
        [] => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conductor"),
        [flag, path] if flag == "--fixtures" && !path.is_empty() => PathBuf::from(path),
        _ => {
            eprintln!("fixtures-conductor: usage: fixtures-conductor [--fixtures <dir>]");
            return false;
        }
    };
    println!(
        "reference timelines: model validated only to the extent documented by probes in tools/oracle/BEHAVIOR.md; not an independent full-game oracle"
    );
    println!(
        "coverage: conductor model parity only; M4 input/replay/jitter are wired gates with their own coverage labels"
    );
    match check_corpus(&path) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("fixtures-conductor: {error}");
            false
        }
    }
}

fn check_corpus(dir: &Path) -> Result<(), String> {
    let expected: BTreeSet<String> = CORPUS.lines().map(str::to_owned).collect();
    if expected.len() != 61 {
        return Err("internal corpus catalog must contain exactly 61 distinct chart IDs".into());
    }
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name();
        let name = name.to_str().ok_or("non-UTF-8 fixture filename")?;
        if !name.ends_with(".jsonl") || name.starts_with('_') {
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
            return Err(format!("fixture is not a regular file: {name}"));
        }
        found.insert(name.trim_end_matches(".jsonl").to_owned());
    }
    if found != expected {
        return Err(format!(
            "expected exactly 61 named chart fixtures (excluding _smoke); found {}; missing {:?}; unexpected {:?}",
            found.len(),
            expected.difference(&found).collect::<Vec<_>>(),
            found.difference(&expected).collect::<Vec<_>>()
        ));
    }
    let manifest = read_json(&dir.join("_manifest.json"))?;
    if integer(&manifest, "schema")? != 1
        || text(&manifest, "simulation")? != SIMULATION
        || text(&manifest, "reference")? != REFERENCE
    {
        return Err("unsupported replay manifest schema/simulation/reference".into());
    }
    // IDs alone cannot distinguish the approved corpus from renamed smoke replays.
    // Embed the approved inputs/counts so --fixtures cannot redefine corpus coverage.
    let trusted: Value = serde_json::from_str(TRUSTED_MANIFEST)
        .map_err(|error| format!("invalid embedded trusted manifest: {error}"))?;
    if manifest != trusted {
        return Err(
            "supplied replay manifest does not match trusted committed baseline (inputs/counts/lengths/offsets)"
                .into(),
        );
    }
    let entries = array(&manifest, "charts")?;
    let mut fixtures = Vec::new();
    for entry in entries {
        fixtures.push(parse_fixture(entry)?);
    }
    let ids: Vec<&str> = fixtures.iter().map(|f| f.id.as_str()).collect();
    if ids != expected.iter().map(String::as_str).collect::<Vec<_>>() {
        return Err("manifest must list the exact 61 chart IDs once, in catalog order".into());
    }
    let smoke = parse_fixture(
        manifest
            .get("smoke")
            .ok_or("missing smoke replay metadata")?,
    )?;
    if smoke.id != "_smoke" {
        return Err("smoke metadata ID must be _smoke".into());
    }

    let mut ok = 0;
    let mut boundaries = 0;
    let mut expected_boundaries = 0;
    let mut worst = 0.0;
    let mut worst_chart = fixtures.first().map_or("", |f| f.id.as_str());
    for fixture in &fixtures {
        expected_boundaries += fixture.rows;
        match check_fixture(dir, fixture) {
            Ok(stats) => {
                ok += 1;
                boundaries += stats.rows;
                if stats.worst_delta > worst {
                    worst = stats.worst_delta;
                    worst_chart = &fixture.id;
                }
            }
            Err(error) => eprintln!("fixtures-conductor: {}: {error}", fixture.id),
        }
    }
    println!("charts: 61");
    println!("ok: {ok}  failed: {}", 61 - ok);
    println!("worst chart: {worst_chart}");
    println!("worst delta ms: {worst:.3}");
    println!("boundary ok: {boundaries}/{expected_boundaries}   (step/beat/measure row counts)");
    // Smoke must pass too, but must never certify a partial corpus as 61 charts.
    let smoke_stats = check_fixture(dir, &smoke).map_err(|e| format!("_smoke: {e}"))?;
    println!(
        "smoke: ok ({} boundaries; excluded from chart count)",
        smoke_stats.rows
    );
    if ok != 61 {
        return Err(format!("{} chart fixture(s) failed", 61 - ok));
    }
    Ok(())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", path.display()))
}

fn number(value: &Value, key: &str) -> Result<f64, String> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| format!("{key}: required finite number"))
}

fn integer(value: &Value, key: &str) -> Result<i64, String> {
    value
        .get(key)
        .and_then(Value::as_i64)
        .ok_or_else(|| format!("{key}: required integer"))
}

fn int32(value: &Value, key: &str) -> Result<i32, String> {
    i32::try_from(integer(value, key)?).map_err(|e| format!("{key}: {e}"))
}

fn count(value: &Value, key: &str) -> Result<usize, String> {
    usize::try_from(integer(value, key)?).map_err(|e| format!("{key}: {e}"))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key}: required string"))
}

fn array<'a>(value: &'a Value, key: &str) -> Result<&'a Vec<Value>, String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key}: required array"))
}

pub(crate) fn parse_fixture(value: &Value) -> Result<Fixture, String> {
    let id = text(value, "id")?.to_owned();
    if id.is_empty()
        || !id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err("invalid fixture ID".into());
    }
    let length = number(value, "lengthMs")?;
    if length <= 0.0 || length > MAX_UPDATES as f64 {
        return Err(format!("{id}: invalid/over-budget music length"));
    }
    let offsets = ConductorOffsets {
        instrumental: number(value, "iOff")?,
        format: number(value, "fOff")?,
        audio_visual: number(value, "avOff")?,
        global: number(value, "gOff")?,
    };
    if !offsets.combined().is_finite() {
        return Err("non-finite combined offset".into());
    }
    let mut changes = Vec::new();
    for change in array(value, "timeChanges")? {
        let time_signature_num = int32(change, "n")?;
        let time_signature_den = int32(change, "d")?;
        let bpm = number(change, "bpm")?;
        if bpm <= 0.0
            || time_signature_num <= 0
            || time_signature_num > i32::MAX / 4
            || time_signature_den <= 0
        {
            return Err(format!("{id}: invalid BPM/time signature"));
        }
        changes.push(TimeChange {
            time_stamp: number(change, "t")?,
            beat_time: number(change, "b")?,
            bpm,
            time_signature_num,
            time_signature_den,
        });
    }
    let counts = [
        count(value, "steps")?,
        count(value, "beats")?,
        count(value, "measures")?,
    ];
    let rows = count(value, "rows")?;
    let total = counts
        .iter()
        .try_fold(0usize, |sum, n| sum.checked_add(*n))
        .ok_or("count overflow")?;
    if rows == 0 || rows != total || counts.contains(&0) {
        return Err(format!("{id}: empty/inconsistent dispatch counts"));
    }
    Ok(Fixture {
        id,
        length,
        offsets,
        changes,
        counts,
        rows,
    })
}

pub(crate) fn read_rows(path: &Path, fixture: &Fixture) -> Result<Vec<Row>, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if data.is_empty() {
        return Err("empty fixture (verification never regenerates it)".into());
    }
    let mut rows: Vec<Row> = Vec::new();
    let mut counts = [0; 3];
    for (line, contents) in data.lines().enumerate() {
        let result = parse_row(contents).and_then(|row| {
            if let Some(previous) = rows.last() {
                if row.sim < previous.sim || (previous.playing && !row.playing) {
                    return Err("simulation time/phase moved backwards".into());
                }
                let inconsistent = row
                    .value
                    .as_object()
                    .ok_or("row must be an object")?
                    .iter()
                    .any(|(key, value)| {
                        key != "t" && key != "i" && previous.value.get(key) != Some(value)
                    });
                if row.sim == previous.sim && (row.kind <= previous.kind || inconsistent) {
                    return Err("same-update rows must share state and dispatch step -> beat -> measure once".into());
                }
            } else if row.sim != 0 || row.playing {
                return Err("first update must be the stopped pre-roll at sim=0".into());
            }
            Ok(row)
        });
        let row = result.map_err(|e| format!("line {}: {e}", line + 1))?;
        counts[row.kind] += 1;
        rows.push(row);
    }
    if rows.len() != fixture.rows || counts != fixture.counts {
        return Err(format!(
            "count mismatch: rows={} kinds={counts:?}; metadata rows={} kinds={:?}",
            rows.len(),
            fixture.rows,
            fixture.counts
        ));
    }
    Ok(rows)
}

fn parse_row(contents: &str) -> Result<Row, String> {
    let value: Value = serde_json::from_str(contents).map_err(|e| e.to_string())?;
    let kind = match text(&value, "t")? {
        "step" => 0,
        "beat" => 1,
        "measure" => 2,
        _ => return Err("unknown dispatch kind".into()),
    };
    let index = int32(&value, "i")?;
    let sim = u64::try_from(integer(&value, "sim")?).map_err(|e| e.to_string())?;
    let playing = value
        .get("clampActive")
        .and_then(Value::as_bool)
        .ok_or("clampActive: required bool")?;
    for key in [
        "input",
        "songPosition",
        "stepTime",
        "beatTime",
        "measureTime",
        "beatLen",
        "stepLen",
        "measureLen",
        "bpm",
        "iOff",
        "fOff",
        "avOff",
        "gOff",
        "combined",
    ] {
        number(&value, key)?;
    }
    for key in ["step", "beat", "measure", "tc", "tsNum", "tsDen"] {
        int32(&value, key)?;
    }
    if index != int32(&value, ["step", "beat", "measure"][kind])? {
        return Err("dispatch index does not match row state".into());
    }
    Ok(Row {
        value,
        kind,
        index,
        sim,
        playing,
    })
}

fn check_fixture(dir: &Path, fixture: &Fixture) -> Result<Stats, String> {
    let rows = read_rows(&dir.join(format!("{}.jsonl", fixture.id)), fixture)?;
    compare(fixture, &rows)
}

pub(crate) fn compare(fixture: &Fixture, rows: &[Row]) -> Result<Stats, String> {
    let mut conductor = Conductor::new();
    conductor.offsets = fixture.offsets.clone();
    conductor.map_time_changes(&fixture.changes);
    let pre = conductor.beat_length_ms() * -5.0;
    if !pre.is_finite()
        || pre.abs() + fixture.offsets.combined().abs() + fixture.length > MAX_UPDATES as f64
    {
        return Err("simulation exceeds finite update budget".into());
    }
    let mut mapped = fixture.changes.clone();
    mapped.sort_by(|a, b| a.time_stamp.total_cmp(&b.time_stamp));
    for change in &mut mapped {
        change.time_stamp = change.time_stamp.max(0.0);
    }
    let mut replay = Replay {
        fixture,
        rows,
        mapped,
        cursor: 0,
        stats: Stats::default(),
    };
    replay.update(&mut conductor, pre, 0, false)?;
    let mut sim = 0;
    // Do not skip silent updates: they can change integer state before playback.
    while conductor.song_position_ms() < conductor.combined_offset() {
        sim += 1;
        if sim > MAX_UPDATES {
            return Err("countdown update budget exceeded".into());
        }
        let input = conductor.song_position_ms() + 1.0;
        replay.update(&mut conductor, input, sim, false)?;
    }
    for music_time in 0..=fixture.length.floor() as u64 {
        sim += 1;
        let input = music_time as f64 + conductor.combined_offset();
        replay.update(&mut conductor, input, sim, true)?;
    }
    if replay.cursor != rows.len() {
        return Err(format!(
            "{} unconsumed reference rows",
            rows.len() - replay.cursor
        ));
    }
    Ok(replay.stats)
}

struct Replay<'a> {
    fixture: &'a Fixture,
    rows: &'a [Row],
    mapped: Vec<TimeChange>,
    cursor: usize,
    stats: Stats,
}

impl Replay<'_> {
    fn update(
        &mut self,
        conductor: &mut Conductor,
        input: f64,
        sim: u64,
        playing: bool,
    ) -> Result<(), String> {
        let fired = conductor.update(Some(input), false, playing, self.fixture.length);
        if fired.is_empty() {
            return Ok(());
        }
        let start = self.cursor;
        let first = self
            .rows
            .get(start)
            .ok_or_else(|| format!("unexpected dispatch at sim={sim}"))?;
        let end = start
            + self.rows[start..]
                .iter()
                .take_while(|r| r.sim == first.sim)
                .count();
        if fired.len() != end - start {
            return Err(format!(
                "update group mismatch at sim={sim}: Rust {} dispatches vs reference {}",
                fired.len(),
                end - start
            ));
        }
        let rows = self.rows;
        for (dispatch, row) in fired.iter().zip(&rows[start..end]) {
            let (kind, index) = match dispatch {
                Dispatch::Step(i) => (0, *i),
                Dispatch::Beat(i) => (1, *i),
                Dispatch::Measure(i) => (2, *i),
            };
            if kind != row.kind || index != row.index || playing != row.playing {
                return Err(format!(
                    "order/index/phase mismatch at sim={sim}: Rust ({kind},{index},{playing}), reference ({},{},{})",
                    row.kind, row.index, row.playing
                ));
            }
            self.compare_state(conductor, row, input, sim)
                .map_err(|e| format!("row {} (sim={sim}): {e}", self.cursor + 1))?;
            self.cursor += 1;
        }
        Ok(())
    }

    fn compare_state(
        &mut self,
        c: &Conductor,
        row: &Row,
        input: f64,
        sim: u64,
    ) -> Result<(), String> {
        let r = &row.value;
        let tc = if self.mapped.is_empty() {
            -1
        } else {
            let mut index = 0;
            if c.song_position_ms() > 0.0 {
                for (i, change) in self.mapped.iter().enumerate() {
                    if c.song_position_ms() < change.time_stamp {
                        break;
                    }
                    index = i;
                }
            }
            i32::try_from(index).map_err(|e| e.to_string())?
        };
        let (num, den) = c
            .current_time_change()
            .map_or((4, 4), |t| (t.time_signature_num, t.time_signature_den));
        for (key, actual) in [
            ("step", c.current_step()),
            ("beat", c.current_beat()),
            ("measure", c.current_measure()),
            ("tc", tc),
            ("tsNum", num),
            ("tsDen", den),
        ] {
            if int32(r, key)? != actual {
                return Err(format!("{key}: expected {}, got {actual}", int32(r, key)?));
            }
        }
        for (key, actual) in [
            ("beatLen", c.beat_length_ms()),
            ("stepLen", c.step_length_ms()),
            ("measureLen", c.measure_length_ms()),
            ("bpm", c.bpm()),
        ] {
            relative(actual, number(r, key)?, key)?;
        }
        for (key, actual) in [
            ("iOff", c.offsets.instrumental),
            ("fOff", c.offsets.format),
            ("avOff", c.offsets.audio_visual),
            ("gOff", c.offsets.global),
            ("combined", c.combined_offset()),
        ] {
            if number(r, key)? != actual {
                return Err(format!("{key}: offset metadata mismatch"));
            }
        }
        for (key, actual, expected) in [
            ("sim", sim as f64, row.sim as f64),
            ("input", input, number(r, "input")?),
            (
                "songPosition",
                c.song_position_ms(),
                number(r, "songPosition")?,
            ),
            (
                "stepTime (ms)",
                c.current_step_time() * c.step_length_ms(),
                number(r, "stepTime")? * c.step_length_ms(),
            ),
            (
                "beatTime (ms)",
                c.current_beat_time() * c.beat_length_ms(),
                number(r, "beatTime")? * c.beat_length_ms(),
            ),
            (
                "measureTime (ms)",
                c.current_measure_time() * c.measure_length_ms(),
                number(r, "measureTime")? * c.measure_length_ms(),
            ),
        ] {
            let delta = (actual - expected).abs();
            if !delta.is_finite() || delta > TIME_TOLERANCE_MS {
                return Err(format!(
                    "{key}: delta {delta} exceeds 1 ms (expected {expected}, got {actual})"
                ));
            }
            if key == "songPosition" {
                self.stats.worst_delta = self.stats.worst_delta.max(delta);
            }
        }
        self.stats.rows += 1;
        Ok(())
    }
}

fn relative(actual: f64, expected: f64, key: &str) -> Result<(), String> {
    if actual.is_finite()
        && expected > 0.0
        && (actual - expected).abs() <= expected.abs() * LENGTH_RELATIVE_TOLERANCE
    {
        Ok(())
    } else {
        Err(format!(
            "{key}: expected {expected}, got {actual}; relative tolerance 1e-6"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn smoke() -> (Fixture, Vec<Row>) {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conductor");
        let manifest = read_json(&dir.join("_manifest.json")).unwrap();
        let fixture = parse_fixture(&manifest["smoke"]).unwrap();
        let rows = read_rows(&dir.join("_smoke.jsonl"), &fixture).unwrap();
        (fixture, rows)
    }

    #[test]
    fn smoke_replays_silent_updates_and_phase_boundary() {
        let (fixture, rows) = smoke();
        assert!(rows.windows(2).any(|r| r[1].sim > r[0].sim + 1));
        assert!(rows.iter().any(|r| r.playing));
        assert_eq!(compare(&fixture, &rows).unwrap().rows, 111);
    }

    #[test]
    fn comparator_rejects_bad_index_order_signature_time_and_length() {
        for (key, replacement) in [
            ("i", serde_json::json!(-21)),
            ("tsNum", serde_json::json!(8)),
            ("songPosition", serde_json::json!(-1251.01)),
            ("beatLen", serde_json::json!(250.001)),
        ] {
            let (fixture, mut rows) = smoke();
            rows[0].value[key] = replacement;
            if key == "i" {
                rows[0].index = -21;
            }
            assert!(compare(&fixture, &rows).is_err(), "accepted corrupt {key}");
        }
        let (fixture, mut rows) = smoke();
        rows.swap(0, 1);
        assert!(compare(&fixture, &rows).is_err());
        let (fixture, mut rows) = smoke();
        rows.remove(0);
        assert!(compare(&fixture, &rows).is_err());
    }

    #[test]
    fn tolerances_are_not_relaxed() {
        assert!(relative(250.00025, 250.0, "beatLen").is_ok());
        assert!(relative(250.000251, 250.0, "beatLen").is_err());
        assert!(parse_row("{}").is_err());
        assert!(parse_row("").is_err());
    }
}
