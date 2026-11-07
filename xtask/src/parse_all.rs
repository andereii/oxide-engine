//! `cargo xtask parse-all` — M1 gate: every known JSON under an assets
//! directory parses, validates and round-trips through `oxide-data`.
//!
//! Output: per-kind table (`kind ok failed total`), the first 10 failures with
//! path and error, and overall `ok/failed/total`. Exit code 0 only if every
//! file parses and round-trips.

use oxide_data::{DataKind, ParsedDocument, Typed, parse_file};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Default assets root: the FNF reference implementation checkout.
const DEFAULT_ASSETS: &str = "~/Descargas/fnf/Funkin/assets";

/// Runs the gate; false for missing/unreadable or JSON-empty input, any parse,
/// validation or round-trip failure, or an incomplete `--corpus` inventory.
pub fn run(rest: &[String]) -> bool {
    let show_diff = rest.iter().any(|a| a == "--diff");
    let show_all = rest.iter().any(|a| a == "--all");
    let corpus = rest.iter().any(|a| a == "--corpus");
    let dir = match rest.iter().position(|a| a == "--dir") {
        Some(i) => match rest.get(i + 1) {
            Some(d) if !d.is_empty() && !d.starts_with("--") => d.clone(),
            _ => {
                eprintln!("parse-all: --dir requires a value");
                return false;
            }
        },
        None => DEFAULT_ASSETS.to_string(),
    };
    let root = expand_tilde(&dir);
    if !root.is_dir() {
        eprintln!("parse-all: directory not found: {}", root.display());
        return false;
    }

    let mut files: Vec<PathBuf> = Vec::new();
    if let Err(error) = collect_json(&root, &mut files) {
        eprintln!("parse-all: {error}");
        return false;
    }
    files.sort();
    if files.is_empty() {
        eprintln!("parse-all: no JSON files found: {}", root.display());
        return false;
    }

    let mut by_kind: BTreeMap<String, KindStats> = BTreeMap::new();
    let mut failure_count = 0usize;

    for path in &files {
        let outcome = check_file(path);
        match outcome {
            Ok(kind) => {
                by_kind.entry(kind).or_default().ok += 1;
            }
            Err((kind, err)) => {
                by_kind.entry(kind).or_default().failed += 1;
                failure_count += 1;
                if failure_count <= 10 || show_all || show_diff {
                    println!("  {}", path.display());
                    println!("    {err}");
                    if show_diff {
                        print_first_diff(path);
                    }
                }
            }
        }
    }

    let total = files.len();
    let total_ok: u64 = by_kind.values().map(|s| s.ok).sum();
    let total_failed: u64 = by_kind.values().map(|s| s.failed).sum();

    println!("{:<12} {:>6} {:>6} {:>6}", "kind", "ok", "failed", "total");
    for (kind, stats) in &by_kind {
        println!(
            "{:<12} {:>6} {:>6} {:>6}",
            kind,
            stats.ok,
            stats.failed,
            stats.ok + stats.failed
        );
    }
    println!(
        "{:<12} {:>6} {:>6} {:>6}",
        "TOTAL", total_ok, total_failed, total
    );

    if failure_count > 0 {
        println!("\n(first 10 failures shown; run with --diff for every failure's first mismatch)");
    }

    let inventory_ok = !corpus || corpus_inventory_matches(total, &by_kind);
    total_failed == 0 && inventory_ok
}

fn corpus_inventory_matches(total: usize, by_kind: &BTreeMap<String, KindStats>) -> bool {
    // Measured with oxide-data's path-first taxonomy, not directory names alone.
    let expected = [
        ("chart", 61),
        ("metadata", 78),
        ("notestyle", 2),
        ("character", 55),
        ("stage", 21),
        ("level", 10),
        ("save", 0),
        ("player", 2),
        ("unknown", 203),
    ];
    let mut ok = total == 432;
    if !ok {
        eprintln!("parse-all: corpus inventory: expected 432 JSON files, found {total}");
    }
    for (kind, expected) in expected {
        let actual = by_kind.get(kind).map_or(0, |stats| stats.ok);
        if actual != expected {
            eprintln!(
                "parse-all: corpus inventory: expected {expected} successful {kind}, found {actual}"
            );
            ok = false;
        }
    }
    ok
}

#[derive(Default)]
struct KindStats {
    ok: u64,
    failed: u64,
}

fn check_file(path: &Path) -> Result<String, (String, String)> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(err) => return Err(("unreadable".into(), format!("read error: {err}"))),
    };
    match parse_file(path, &bytes) {
        Ok(parsed) => {
            if let Err(err) = verify_round_trip(&parsed, &bytes) {
                return Err((kind_name(parsed.kind).to_string(), err));
            }
            Ok(kind_name(parsed.kind).to_string())
        }
        Err(err) => Err((failure_kind_name(&err), err.to_string())),
    }
}

/// The kind to attribute a failure to. Schema errors carry the kind
/// (path-implied or shape-classified); anything earlier is a plain parse.
fn failure_kind_name(err: &oxide_data::DataError) -> String {
    match err {
        oxide_data::DataError::Schema { kind, .. } => (*kind).to_string(),
        _ => "parse".to_string(),
    }
}

fn kind_name(kind: DataKind) -> &'static str {
    match kind {
        DataKind::Chart => "chart",
        DataKind::Metadata => "metadata",
        DataKind::NoteStyle => "notestyle",
        DataKind::Character => "character",
        DataKind::Stage => "stage",
        DataKind::Level => "level",
        DataKind::Save => "save",
        DataKind::Player => "player",
        DataKind::Unknown => "unknown",
    }
}

/// Semantic round-trip check: the typed payload must re-serialize to a value
/// equivalent to re-parsing the raw bytes (numeric equivalence for numbers,
/// exact otherwise). Unknown kinds must round-trip strictly.
fn verify_round_trip(parsed: &ParsedDocument, raw: &[u8]) -> Result<(), String> {
    let reparsed: Value = parse_raw(raw)?;
    let typed_value: Value = typed_payload_value(parsed)?;
    if matches!(parsed.typed, Typed::Unknown(_)) {
        if typed_value == reparsed {
            Ok(())
        } else {
            let detail = first_mismatch(&typed_value, &reparsed, String::new())
                .unwrap_or_else(|| "no nested detail".into());
            Err(format!(
                "unknown-value round-trip is not strict-equal ({detail})"
            ))
        }
    } else if values_equal(&typed_value, &reparsed) {
        Ok(())
    } else {
        let detail = first_mismatch(&typed_value, &reparsed, String::new())
            .unwrap_or_else(|| "no nested detail".into());
        Err(format!(
            "typed round-trip differs from the original value ({detail})"
        ))
    }
}

/// Re-parses raw bytes the same way `oxide_data::parse_any` does
/// (UTF-8 validity + BOM strip) so both sides see identical input.
fn parse_raw(raw: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(raw).map_err(|e| e.to_string())?;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    serde_json::from_str(text).map_err(|e| e.to_string())
}

/// Serializes the payload inside `Typed`, without the enum variant wrapper.
fn typed_payload_value(parsed: &ParsedDocument) -> Result<Value, String> {
    match &parsed.typed {
        Typed::Unknown(v) => Ok(v.clone()),
        Typed::Chart(x) => serde_json::to_value(x.as_ref()),
        Typed::Metadata(x) => serde_json::to_value(x.as_ref()),
        Typed::NoteStyle(x) => serde_json::to_value(x.as_ref()),
        Typed::Character(x) => serde_json::to_value(x.as_ref()),
        Typed::Stage(x) => serde_json::to_value(x.as_ref()),
        Typed::Level(x) => serde_json::to_value(x.as_ref()),
        Typed::Save(x) => serde_json::to_value(x.as_ref()),
        Typed::Player(x) => serde_json::to_value(x.as_ref()),
    }
    .map_err(|e| e.to_string())
}

fn values_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(xs), Value::Array(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(x, y)| values_equal(x, y))
        }
        (Value::Object(xs), Value::Object(ys)) => {
            xs.len() == ys.len()
                && xs
                    .iter()
                    .all(|(k, x)| ys.get(k).is_some_and(|y| values_equal(x, y)))
        }
        _ => a == b,
    }
}

fn collect_json(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read directory {}: {error}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("cannot read entry in {}: {error}", dir.display()))?;
        let path = entry.path();
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.is_dir() {
            collect_json(&path, out)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            out.push(path);
        }
    }
    Ok(())
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}

/// Diagnostic aid for the `--diff` flag: prints the first mismatching path
/// between the original value and the typed round-trip of one file.
fn print_first_diff(path: &Path) {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(err) => {
            println!("  [diff] read error: {err}");
            return;
        }
    };
    match parse_file(path, &bytes) {
        Ok(parsed) => {
            let reparsed: Value = match parse_raw(&bytes) {
                Ok(v) => v,
                Err(err) => {
                    println!("  [diff] reparse error: {err}");
                    return;
                }
            };
            match typed_payload_value(&parsed) {
                Ok(typed) => {
                    if let Some(d) = first_mismatch(&typed, &reparsed, String::new()) {
                        println!("  [diff] first mismatch at `{d}`");
                    } else {
                        println!("  [diff] no mismatch (error elsewhere)");
                    }
                }
                Err(err) => println!("  [diff] to_value error: {err}"),
            }
        }
        Err(err) => println!("  [diff] parse error: {err}"),
    }
}

fn first_mismatch(a: &Value, b: &Value, path: String) -> Option<String> {
    let diff = |detail: String| {
        Some(if path.is_empty() {
            detail
        } else {
            format!("{path}: {detail}")
        })
    };
    match (a, b) {
        (Value::Object(xs), Value::Object(ys)) => {
            for k in xs.keys() {
                if ys.get(k).is_none() {
                    return diff(format!("typed lost `{k}`"));
                }
            }
            for k in ys.keys() {
                if xs.get(k).is_none() {
                    return diff(format!("original extra `{k}` = {}", ys[k]));
                }
            }
            for (k, x) in xs.iter() {
                let y = ys.get(k).unwrap_or(x);
                if let Some(p) = first_mismatch(x, y, format!("{path}.{k}")) {
                    return Some(p);
                }
            }
            None
        }
        (Value::Array(xs), Value::Array(ys)) => {
            if xs.len() != ys.len() {
                return diff(format!("length {} vs {}", xs.len(), ys.len()));
            }
            for (i, (x, y)) in xs.iter().zip(ys).enumerate() {
                if let Some(p) = first_mismatch(x, y, format!("{path}[{i}]")) {
                    return Some(p);
                }
            }
            None
        }
        _ => {
            if values_equal(a, b) {
                None
            } else if path.is_empty() {
                diff("whole value".into())
            } else {
                Some(path)
            }
        }
    }
}
