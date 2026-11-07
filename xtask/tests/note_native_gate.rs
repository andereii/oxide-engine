//! `xtask note-native` CLI regressions: the frozen reference gate must fail
//! closed on missing fixtures, fixture drift, stream drift and malformed
//! manifests before it can ever reach a pass. Synthetic mini fixtures are
//! built in `target/`; they are never repository fixtures or native evidence.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/note-native-cli-tests")
            .join(format!(
                "{tag}-{}-{}",
                std::process::id(),
                NEXT_DIR.fetch_add(1, Ordering::Relaxed)
            ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("create scratch directory");
        Self(path)
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn stream_text(difficulty: &str) -> String {
    format!(
        "{{\"schema\":\"note-stream-1\",\"difficulty\":\"{difficulty}\",\"chart_id\":\
         \"bopeebo-default\",\"tick_count\":1,\"songpos_source\":\"synthetic CLI fixture\"}}\n\
         {{\"sim\":0,\"music_ms\":0.0,\"events\":[]}}\n"
    )
}

fn manifest_file() -> &'static str {
    "_manifest_native.json"
}

fn write_manifest(directory: &Path, manifest: &Value) {
    std::fs::write(
        directory.join(manifest_file()),
        serde_json::to_vec_pretty(manifest).expect("manifest serializes"),
    )
    .expect("write manifest");
}

/// A structurally valid skeleton: one stream tick and one (opaque) row per
/// difficulty. Digests/counts are computed by this test, not by the gate.
fn write_skeleton(directory: &Path) -> Value {
    std::fs::create_dir_all(directory).expect("create fixture directory");
    let mut streams = serde_json::Map::new();
    let mut rows = serde_json::Map::new();
    let mut tick_counts = serde_json::Map::new();
    let mut row_counts = serde_json::Map::new();
    for difficulty in ["easy", "normal", "hard"] {
        let stream_name = format!("{difficulty}.stream.jsonl");
        let stream = stream_text(difficulty);
        std::fs::write(directory.join(&stream_name), &stream).expect("write stream");
        streams.insert(stream_name.clone(), json!(sha_hex(stream.as_bytes())));
        tick_counts.insert(stream_name, json!(1));

        let row_name = format!("{difficulty}.jsonl");
        // Normative §2 hold_start order; the comparator enforces key order,
        // not merely the key set.
        let row_bytes = b"{\"schema\":\"1\",\"kind\":\"hold_start\",\"sim\":0,\"input_ms\":0.0,\"input_time_us\":0.0,\"note_id\":0,\"note_direction\":0,\"judgement\":\"none\",\"raw_note_diff\":0.0,\"note_diff_int\":0}\n";
        std::fs::write(directory.join(&row_name), row_bytes).expect("write rows");
        rows.insert(row_name.clone(), json!(sha_hex(row_bytes)));
        row_counts.insert(row_name, json!(1));
    }
    let manifest = json!({
        "schema": 1,
        "recipe": "synthesized-bopeebo-slice-1ms-v1",
        "reference": "synthetic CLI fixture; not native execution evidence",
        "chart_id": "bopeebo-default",
        "stream_schema": "note-stream-1",
        "difficulties": ["easy", "normal", "hard"],
        "streams": streams,
        "rows": rows,
        "tick_counts": tick_counts,
        "row_counts": row_counts,
        "provenance": {
            "generator": "note_native_gate CLI test",
            "native_source": "synthetic CLI fixture",
            "probe_script_sha256": "0".repeat(64),
            "input_source": "synthesized from chart (no human replay yet)",
            "stream_recipe": "synthetic CLI fixture",
            "evidence_boundary": "test only"
        }
    });
    write_manifest(directory, &manifest);
    manifest
}

fn gate(fixtures: &Path, assets: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("note-native")
        .arg("--assets")
        .arg(assets)
        .arg("--fixtures")
        .arg(fixtures)
        .output()
        .expect("spawn xtask")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_gate_failure(output: &Output, diagnostic: &str) {
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let message = stderr(output);
    assert!(
        message.contains(diagnostic),
        "expected {diagnostic:?}: {message}"
    );
}

#[test]
fn missing_fixtures_directory_fails_closed() {
    let scratch = Scratch::new("missing-fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    let output = gate(&scratch.path("missing"), &assets);
    assert_gate_failure(&output, "fixtures directory is missing");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PASS"));
}

#[test]
fn missing_manifest_fails_closed() {
    let scratch = Scratch::new("missing-manifest");
    let fixtures = scratch.path("fixtures");
    std::fs::create_dir(&fixtures).unwrap();
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "cannot read fixtures manifest");
}

#[test]
fn header_schema_drift_fails_closed() {
    let scratch = Scratch::new("schema");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    let mut manifest = write_skeleton(&fixtures);
    let altered = stream_text("easy").replace("\"note-stream-1\"", "\"note-stream-2\"");
    std::fs::write(fixtures.join("easy.stream.jsonl"), &altered).unwrap();
    manifest["streams"]["easy.stream.jsonl"] = json!(sha_hex(altered.as_bytes()));
    write_manifest(&fixtures, &manifest);
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "expected \"note-stream-1\"");
}

#[test]
fn stream_digest_drift_fails_closed() {
    let scratch = Scratch::new("stream-digest");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    write_skeleton(&fixtures);
    let mut altered = stream_text("easy").into_bytes();
    altered.push(b' ');
    std::fs::write(fixtures.join("easy.stream.jsonl"), altered).unwrap();
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "frozen stream sha256");
}

#[test]
fn row_digest_drift_fails_closed() {
    let scratch = Scratch::new("row-digest");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    write_skeleton(&fixtures);
    std::fs::write(fixtures.join("normal.jsonl"), b"{\"changed\":true}\n").unwrap();
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "frozen native rows sha256");
}

#[test]
fn reordered_row_keys_fail_closed() {
    let scratch = Scratch::new("row-key-order");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    let mut manifest = write_skeleton(&fixtures);
    let valid = std::fs::read_to_string(fixtures.join("easy.jsonl")).unwrap();
    let reordered = valid.replacen(
        "{\"schema\":\"1\",\"kind\":\"hold_start\"",
        "{\"kind\":\"hold_start\",\"schema\":\"1\"",
        1,
    );
    assert_ne!(reordered, valid, "the fixture row was not reordered");
    std::fs::write(fixtures.join("easy.jsonl"), &reordered).unwrap();
    manifest["rows"]["easy.jsonl"] = json!(sha_hex(reordered.as_bytes()));
    write_manifest(&fixtures, &manifest);
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "key order differs from the normative §2 schema");
}

#[test]
fn missing_fixture_row_file_fails_closed() {
    let scratch = Scratch::new("missing-row");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    write_skeleton(&fixtures);
    std::fs::remove_file(fixtures.join("hard.jsonl")).unwrap();
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "cannot read frozen native rows");
}

#[test]
fn difficulty_count_mismatch_fails_closed() {
    let scratch = Scratch::new("difficulty-count");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("assets");
    std::fs::create_dir(&assets).unwrap();
    let mut manifest = write_skeleton(&fixtures);
    manifest["difficulties"] = json!(["easy", "normal"]);
    write_manifest(&fixtures, &manifest);
    let output = gate(&fixtures, &assets);
    assert_gate_failure(&output, "must list exactly");
}

#[test]
fn a_structurally_valid_skeleton_reaches_the_real_asset_requirement() {
    let scratch = Scratch::new("skeleton");
    let fixtures = scratch.path("fixtures");
    let assets = scratch.path("empty-assets");
    std::fs::create_dir(&assets).unwrap();
    write_skeleton(&fixtures);
    let output = gate(&fixtures, &assets);
    // Fixture validation passed; the gate must still refuse synthetic assets
    // instead of pretending to re-export the real corpus.
    assert_gate_failure(&output, "cannot read asset");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PASS"));
}

#[test]
fn missing_assets_flag_is_a_usage_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["note-native", "--fixtures", "unused"])
        .output()
        .expect("spawn xtask");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr(&output).contains("missing --assets"));
}

#[test]
fn unknown_flags_are_usage_errors() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args([
            "note-native",
            "--assets",
            "unused",
            "--fixtures",
            "unused",
            "--bogus",
        ])
        .output()
        .expect("spawn xtask");
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    assert!(stderr(&output).contains("unsupported argument"));
}

#[test]
fn a_missing_assets_directory_is_a_prerequisite_failure() {
    let scratch = Scratch::new("missing-assets");
    let output = gate(&scratch.path("fixtures"), &scratch.path("missing-assets"));
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(stderr(&output).contains("prerequisite: assets directory is missing"));
}
