//! `cargo xtask conductor-native` — the frozen native Conductor reference gate.
//!
//! Evidence strands (all fail-closed):
//!
//! 1. **Frozen native rows**: `tests/fixtures/conductor-native/*.jsonl` were
//!    produced by running the REAL `funkin.Conductor` (installed game source,
//!    hxcpp) through the frozen replay recipe; `tools/oracle/native_conductor_probe.sh`
//!    is the provenance pipeline and `_manifest_native.json` records input/
//!    source/instrument hashes. This gate validates the row digests against the
//!    committed manifest and refuses drift.
//! 2. **Native == model**: the frozen native rows are compared field-wise with
//!    the committed model fixtures (`tests/fixtures/conductor/*.jsonl`) at the
//!    contract tolerances (<= 1 ms for ms fields, 1e-6 relative for lengths and
//!    BPM); counts, indices, sims and dispatch order must match exactly.
//! 3. **Rust == native**: the same replay mechanic as `fixtures-conductor`
//!    replays the recipe through OUR `oxide_audio::conductor::Conductor`
//!    against the NATIVE rows with the 1 ms budget and exact counts.
//!
//! Honesty labels: the native rows are a native hxcpp Conductor execution on
//! the simulated recipe inputs — no gameplay integration, no audio engine, no
//! hardware. Model agreement is not native-game parity beyond what this probe
//! exercising state; the input_jitter/replay gates stay separate. Output says
//! explicitly: "not a full-game Conductor integration trace".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::conductor_check::{Fixture, Row};

/// Freeze the native reference manifest (digests + provenance).
const TRUSTED_NATIVE_MANIFEST: &str =
    include_str!("../../tests/fixtures/conductor-native/_manifest_native.json");
/// The trusted model fixture manifest (shared with `fixtures-conductor`).
const TRUSTED_MODEL_MANIFEST: &str = include_str!("../../tests/fixtures/conductor/_manifest.json");
const MODEL_MANIFEST_SHA256: &str =
    "279c7a89ca916091218bcb28e8c33876833e441326f148a656c06f1a5f9cc2d7";

const TIME_TOLERANCE_MS: f64 = 1.0;
const LENGTH_RELATIVE_TOLERANCE: f64 = 1e-6;

pub fn run(rest: &[String]) -> ExitCode {
    let dir = match rest {
        [] => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conductor-native"),
        [flag, path] if flag == "--fixtures" && !path.is_empty() => PathBuf::from(path),
        other => {
            eprintln!(
                "conductor-native: unsupported arguments {other:?}; \
                 usage: conductor-native [--fixtures <dir>]"
            );
            return ExitCode::from(2);
        }
    };
    println!(
        "coverage: frozen native hxcpp `funkin.Conductor` rows replayed against our Rust \
         conductor and the model fixtures; not a full-game integration trace"
    );
    match check(&dir) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("conductor-native: {error}");
            ExitCode::FAILURE
        }
    }
}

fn sha_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

/// All three strands; any error fails the gate without partial success.
fn check(dir: &Path) -> Result<(), String> {
    let trusted: Value = serde_json::from_str(TRUSTED_NATIVE_MANIFEST)
        .map_err(|e| format!("trusted native manifest is unparseable: {e}"))?;
    let supplied_path = dir.join("_manifest_native.json");
    let supplied = read_json(&supplied_path)?;
    if supplied != trusted {
        return Err(
            "supplied native manifest does not match the committed trusted manifest \
             (unexpected fixture drift or instrument change)"
                .into(),
        );
    }
    if schema(&trusted)? != 1 || text(&trusted, "recipe")? != "countdown-1ms-playback-1ms-v1" {
        return Err("unsupported native manifest schema/recipe".into());
    }
    let model_sha = sha_hex(TRUSTED_MODEL_MANIFEST.as_bytes());
    if model_sha != MODEL_MANIFEST_SHA256 {
        return Err(format!(
            "embedded model manifest sha256 {model_sha} differs from the frozen native \
             reference {MODEL_MANIFEST_SHA256}; refusing to replay across fixture sets"
        ));
    }
    let embedded_model_sha = text(&trusted, "model_manifest_sha256")?.to_owned();
    if embedded_model_sha != model_sha {
        return Err(format!(
            "native manifest recorded model manifest sha256 {embedded_model_sha}, but the \
             trusted embedded manifest hashes {model_sha}"
        ));
    }

    // Row digests must match the frozen manifest (no fixture drift).
    let rows_digests = trusted
        .get("rows")
        .and_then(Value::as_object)
        .ok_or_else(|| "native manifest lacks `rows` digests".to_owned())?;
    let mut ids: Vec<&str> = rows_digests.keys().map(String::as_str).collect();
    ids.sort_unstable();
    if ids.len() != 62 || !ids.iter().any(|id| id == &"_smoke.jsonl") {
        return Err(format!(
            "native manifest rows list must cover exactly 62 ids (61 charts + _smoke); got {}",
            ids.len()
        ));
    }

    let model_manifest: Value = serde_json::from_str(TRUSTED_MODEL_MANIFEST)
        .map_err(|e| format!("embedded model manifest is unparseable: {e}"))?;

    // The native manifest does not duplicate the model fixture inputs; use the
    // shared trusted model manifest (weights already equality-checked above)
    // and treat its smoke entry the same way `fixtures-conductor` does.
    let mut charts = Vec::new();
    for entry in model_manifest
        .get("charts")
        .and_then(Value::as_array)
        .ok_or("trusted model manifest lacks `charts`")?
    {
        charts.push(entry.clone());
    }
    charts.push(
        model_manifest
            .get("smoke")
            .cloned()
            .ok_or("trusted model manifest lacks smoke")?,
    );
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut worst_delta = 0.0f64;
    let mut worst_chart = String::new();
    let mut deltas = RowDeltas::EMPTY;
    for entry in &charts {
        let fixture = crate::conductor_check::parse_fixture(entry)?;
        let id = fixture.id.clone();
        let native_path = dir.join(format!("{id}.jsonl"));
        let rows_digest = rows_digests
            .get(&format!("{id}.jsonl"))
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{id}: native manifest lacks row digest"))?;
        let data = fs::read(&native_path).map_err(|e| {
            format!(
                "cannot read frozen native rows {}: {e}",
                native_path.display()
            )
        })?;
        if sha_hex(&data) != rows_digest {
            return Err(format!(
                "{id}: frozen native rows sha256 differs from the committed manifest; \
                 refusing to replay drifted rows"
            ));
        }
        // 1. correct row count against the manifest entry.
        let native_rows = crate::conductor_check::read_rows(&native_path, &fixture)?;
        if native_rows.len() != fixture.rows {
            failed += 1;
            eprintln!(
                "charts {id}: native rows {} != manifest {}",
                native_rows.len(),
                fixture.rows
            );
            continue;
        }
        // 2. native vs model rows field-wise at contract tolerances.
        let model_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conductor");
        let model_rows =
            crate::conductor_check::read_rows(&model_dir.join(format!("{id}.jsonl")), &fixture)?;
        if let Err(error) = rows_equal(&fixture, &native_rows, &model_rows, &mut deltas) {
            failed += 1;
            eprintln!("charts {id}: native vs model rows: {error}");
            continue;
        }
        // 3. OUR Conductor replaying the recipe reach the SAME tracks as the
        //    native rows (1 ms budget).
        let stats = {
            use crate::conductor_check as chk;
            chk::compare(&fixture, &native_rows).map_err(|e| format!("{id}: {e}"))?
        };
        if stats.rows != native_rows.len() {
            failed += 1;
            eprintln!(
                "charts {id}: replayed rows {} != native {}",
                stats.rows,
                native_rows.len()
            );
            continue;
        }
        ok += 1;
        if stats.worst_delta > worst_delta {
            worst_delta = stats.worst_delta;
            worst_chart = id;
        }
    }
    let total = charts.len();
    println!(
        "charts: {} (native manifest {})",
        model_manifest_charts(&model_manifest),
        aggregate_rows(&trusted)
    );
    println!("native ok: {ok}/{total} failed: {failed}");
    println!(
        "native max deltas: input {:.3} ms, songPosition {:.3} ms, stepTime {:.3} ms, \
         beatTime {:.3} ms, measureTime {:.3} ms",
        deltas.input,
        deltas.song_position,
        deltas.step_time_ms,
        deltas.beat_time_ms,
        deltas.measure_time_ms
    );
    println!("native worst chart: {worst_chart}");
    println!(
        "native worst delta ms: {worst_delta:.3}  (budget <= 1 ms; lengths/BPM 1e-6 relative)"
    );
    println!(
        "reference limitation: native rows are a real hxcpp funkin.Conductor execution over \
         the frozen replay recipe; not gameplay/notes/audio parity; the platform-specific \
         and hardware input obligations stay explicit in the acceptance labels"
    );
    if failed != 0 || ok != total {
        return Err(format!(
            "native reference gate failed for {failed}/{total} ids"
        ));
    }
    Ok(())
}

/// Every native row accounted for by the frozen manifest digests.
fn aggregate_rows(trusted: &Value) -> usize {
    trusted
        .get("rows")
        .and_then(Value::as_object)
        .map(|m| m.len())
        .unwrap_or(0)
}

fn model_manifest_charts(model: &Value) -> usize {
    model
        .get("charts")
        .and_then(Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0)
}

/// Field-wise row comparison at the frozen tolerances; counts, indices, sim
/// and dispatch order exact. stepTime/beatTime/measureTime are STEP/BEAT/
/// MEASURE counts in the fixture rows, so they are converted to milliseconds
/// through the row's own lengths BEFORE the 1 ms budget — the same mechanics
/// `fixtures-conductor` uses; comparing them raw would hide up to a full
/// step of drift. Per-field maximum deltas are tracked and printed.
struct RowDeltas {
    input: f64,
    song_position: f64,
    step_time_ms: f64,
    beat_time_ms: f64,
    measure_time_ms: f64,
}

impl RowDeltas {
    const EMPTY: Self = Self {
        input: 0.0,
        song_position: 0.0,
        step_time_ms: 0.0,
        beat_time_ms: 0.0,
        measure_time_ms: 0.0,
    };
}

fn as_number(
    value: Option<&Value>,
    label: &str,
    row: usize,
    fixture: &Fixture,
) -> Result<f64, String> {
    match value {
        Some(Value::Number(av)) => av.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
            format!(
                "row {} ({}): {label} not a finite number",
                row + 1,
                fixture.id
            )
        }),
        _ => Err(format!(
            "row {} ({}): {label} missing/non-numeric",
            row + 1,
            fixture.id
        )),
    }
}

fn as_integer(
    value: Option<&Value>,
    label: &str,
    row: usize,
    fixture: &Fixture,
) -> Result<i64, String> {
    match value {
        Some(Value::Number(av)) => av
            .as_i64()
            .ok_or_else(|| format!("row {} ({}): {label} not an integer", row + 1, fixture.id)),
        _ => Err(format!(
            "row {} ({}): {label} missing/non-numeric",
            row + 1,
            fixture.id
        )),
    }
}

fn rows_equal(
    fixture: &Fixture,
    native: &[Row],
    model: &[Row],
    deltas: &mut RowDeltas,
) -> Result<(), String> {
    if native.len() != model.len() {
        return Err(format!(
            "row count {} (native) != {} (model) for {}",
            native.len(),
            model.len(),
            fixture.id
        ));
    }
    for (i, (a, b)) in native.iter().zip(model.iter()).enumerate() {
        if a.kind != b.kind || a.index != b.index || a.sim != b.sim || a.playing != b.playing {
            return Err(format!(
                "row {} ({}): kind/index/sim/phase mismatch",
                i + 1,
                fixture.id
            ));
        }
        let check = |key: &str, label: &str, budget: f64, mode: &str| -> Result<f64, String> {
            let av = as_number(a.value.get(key), label, i, fixture)?;
            let bv = as_number(b.value.get(key), label, i, fixture)?;
            match mode {
                "abs" => {
                    if (av - bv).abs() > budget {
                        return Err(format!(
                            "row {} ({}): {label} native={av} model={bv} exceeds {budget}",
                            i + 1,
                            fixture.id
                        ));
                    }
                }
                _ => {
                    let equal = av.total_cmp(&bv) == std::cmp::Ordering::Equal
                        || ((av / bv) - 1.0).abs() <= budget
                        || (bv.abs() < 1e-9 && av.abs() < budget);
                    if !equal {
                        return Err(format!(
                            "row {} ({}): {label} native={av} model={bv} outside {budget} relative",
                            i + 1,
                            fixture.id
                        ));
                    }
                }
            }
            Ok(av)
        };
        // step/beat/measure converted through the row's own lengths to ms.
        let step_len = as_number(b.value.get("stepLen"), "stepLen", i, fixture)?;
        let beat_len = as_number(b.value.get("beatLen"), "beatLen", i, fixture)?;
        let measure_len = as_number(b.value.get("measureLen"), "measureLen", i, fixture)?;
        check("stepLen", "stepLen", LENGTH_RELATIVE_TOLERANCE, "rel")?;
        check("beatLen", "beatLen", LENGTH_RELATIVE_TOLERANCE, "rel")?;
        check("measureLen", "measureLen", LENGTH_RELATIVE_TOLERANCE, "rel")?;
        check("bpm", "bpm", LENGTH_RELATIVE_TOLERANCE, "rel")?;
        check("iOff", "iOff", 0.0, "abs")?;
        check("fOff", "fOff", 0.0, "abs")?;
        check("combined", "combined", 0.0, "abs")?;
        let step_delta = as_number(a.value.get("stepTime"), "stepTime", i, fixture)?
            - as_number(b.value.get("stepTime"), "stepTime", i, fixture)?;
        if step_delta.abs() * (step_len / 1.0) > TIME_TOLERANCE_MS {
            return Err(format!(
                "row {} ({}): stepTime native={} model={} exceeds {} ms of the step grid",
                i + 1,
                fixture.id,
                step_delta,
                step_len,
                step_delta * step_len
            ));
        }
        deltas.step_time_ms = deltas.step_time_ms.max(step_delta.abs() * step_len);
        let beat_delta = as_number(a.value.get("beatTime"), "beatTime", i, fixture)?
            - as_number(b.value.get("beatTime"), "beatTime", i, fixture)?;
        if beat_delta.abs() * beat_len > TIME_TOLERANCE_MS {
            return Err(format!(
                "row {} ({}): beatTime native delta is {} ms of the beat grid",
                i + 1,
                fixture.id,
                beat_delta * beat_len
            ));
        }
        deltas.beat_time_ms = deltas.beat_time_ms.max(beat_delta.abs() * beat_len);
        let measure_delta = as_number(a.value.get("measureTime"), "measureTime", i, fixture)?
            - as_number(b.value.get("measureTime"), "measureTime", i, fixture)?;
        if measure_delta.abs() * measure_len > TIME_TOLERANCE_MS {
            return Err(format!(
                "row {} ({}): measureTime native delta is {} ms of the measure grid",
                i + 1,
                fixture.id,
                measure_delta * measure_len
            ));
        }
        deltas.measure_time_ms = deltas
            .measure_time_ms
            .max(measure_delta.abs() * measure_len);
        let input_delta = as_number(a.value.get("input"), "input", i, fixture)?
            - as_number(b.value.get("input"), "input", i, fixture)?;
        let position_delta = as_number(a.value.get("songPosition"), "songPosition", i, fixture)?
            - as_number(b.value.get("songPosition"), "songPosition", i, fixture)?;
        if input_delta.abs() > TIME_TOLERANCE_MS || position_delta.abs() > TIME_TOLERANCE_MS {
            return Err(format!(
                "row {} ({}): input/songPosition delta ({input_delta}, {position_delta})                  exceeds {TIME_TOLERANCE_MS} ms",
                i + 1,
                fixture.id
            ));
        }
        deltas.input = deltas.input.max(input_delta.abs());
        deltas.song_position = deltas.song_position.max(position_delta.abs());
        // State integers (step/beat/measure, tc, ts, offsets) exactly.
        for (key, label) in [
            ("step", "step"),
            ("beat", "beat"),
            ("measure", "measure"),
            ("tc", "tc"),
            ("tsNum", "tsNum"),
            ("tsDen", "tsDen"),
        ] {
            let av = as_integer(a.value.get(key), label, i, fixture)?;
            let bv = as_integer(b.value.get(key), label, i, fixture)?;
            if av != bv {
                return Err(format!(
                    "row {} ({}): {label} native={av} model={bv} exact mismatch",
                    i + 1,
                    fixture.id
                ));
            }
        }
        for (key, label) in [("avOff", "avOff"), ("gOff", "gOff")] {
            let av = as_integer(a.value.get(key), label, i, fixture)?;
            let bv = as_integer(b.value.get(key), label, i, fixture)?;
            if av != bv {
                return Err(format!(
                    "row {} ({}): {label} native={av} model={bv} exact mismatch",
                    i + 1,
                    fixture.id
                ));
            }
        }
        let a_clamp = a.value.get("clampActive").and_then(Value::as_bool);
        let b_clamp = b.value.get("clampActive").and_then(Value::as_bool);
        if a_clamp != b_clamp {
            return Err(format!(
                "row {} ({}): clampActive native={a_clamp:?} model={b_clamp:?}",
                i + 1,
                fixture.id
            ));
        }
    }
    Ok(())
}

fn schema(value: &Value) -> Result<i64, String> {
    value
        .get("schema")
        .and_then(Value::as_i64)
        .ok_or_else(|| "required integer schema".into())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_slice(&data).map_err(|e| format!("{}: {e}", path.display()))
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("{key}: required string"))
}
