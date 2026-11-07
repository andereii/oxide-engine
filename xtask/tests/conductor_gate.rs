//! Full-corpus certification must fail closed; a micro fixture is not 61 charts.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oxide-conductor-gate-{}-{}",
            std::process::id(),
            NEXT_DIR.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conductor")
}

fn gate(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["fixtures-conductor", "--fixtures"])
        .arg(path)
        .output()
        .unwrap()
}

#[test]
fn full_corpus_reports_model_parity_and_pending_m4_coverage() {
    let output = gate(&fixtures());
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    for required in [
        "charts: 61",
        "ok: 61  failed: 0",
        "not an independent full-game oracle",
        "M4 input/replay/jitter are wired gates with their own coverage labels",
        "smoke: ok",
    ] {
        assert!(stdout.contains(required), "missing {required}: {stdout}");
    }
}

#[test]
fn renamed_smoke_corpus_cannot_claim_full_corpus_coverage() {
    let scratch = Scratch::new();
    let mut manifest: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/conductor/_manifest.json"
    ))
    .unwrap();
    let smoke = manifest["smoke"].clone();
    let charts = manifest["charts"].as_array_mut().unwrap();
    assert_eq!(charts.len(), 61);
    for chart in charts {
        let id = chart["id"].as_str().unwrap().to_owned();
        *chart = smoke.clone();
        chart["id"] = serde_json::json!(id);
        std::fs::copy(
            fixtures().join("_smoke.jsonl"),
            scratch.0.join(format!("{id}.jsonl")),
        )
        .unwrap();
    }
    std::fs::copy(
        fixtures().join("_smoke.jsonl"),
        scratch.0.join("_smoke.jsonl"),
    )
    .unwrap();
    std::fs::write(
        scratch.0.join("_manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();

    let output = gate(&scratch.0);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.contains("trusted committed baseline"), "{stderr}");
    assert!(!stdout.contains("charts: 61"), "{stdout}");
    assert!(!stdout.contains("ok: 61  failed: 0"), "{stdout}");
}

#[test]
fn descriptor_inputs_counts_lengths_and_offsets_are_anchored_before_replay() {
    let scratch = Scratch::new();
    let baseline: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/conductor/_manifest.json"
    ))
    .unwrap();
    for chart in baseline["charts"].as_array().unwrap() {
        let id = chart["id"].as_str().unwrap();
        std::fs::write(scratch.0.join(format!("{id}.jsonl")), "").unwrap();
    }
    for path in [
        "/charts/0/lengthMs",
        "/charts/0/iOff",
        "/charts/0/fOff",
        "/charts/0/avOff",
        "/charts/0/gOff",
        "/charts/0/steps",
        "/charts/0/beats",
        "/charts/0/measures",
        "/charts/0/rows",
        "/charts/0/timeChanges/0/t",
        "/charts/0/timeChanges/0/b",
        "/charts/0/timeChanges/0/bpm",
        "/charts/0/timeChanges/0/n",
        "/charts/0/timeChanges/0/d",
        "/smoke/lengthMs",
    ] {
        let mut manifest = baseline.clone();
        let field = manifest.pointer_mut(path).unwrap();
        *field = if let Some(value) = field.as_i64() {
            serde_json::json!(value + 1)
        } else {
            serde_json::json!(field.as_f64().unwrap() + 1.0)
        };
        std::fs::write(
            scratch.0.join("_manifest.json"),
            serde_json::to_vec_pretty(&manifest).unwrap(),
        )
        .unwrap();
        let output = gate(&scratch.0);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(output.status.code(), Some(1), "accepted {path}: {stdout}");
        assert!(
            stderr.contains("trusted committed baseline"),
            "{path}: {stderr}"
        );
        assert!(!stdout.contains("charts: 61"), "{path}: {stdout}");
    }
}

#[test]
fn missing_empty_and_smoke_only_cannot_certify_the_corpus_or_generate_files() {
    let scratch = Scratch::new();
    let missing = scratch.0.join("missing");
    assert_eq!(gate(&missing).status.code(), Some(1));
    assert!(!missing.exists());
    assert_eq!(gate(&scratch.0).status.code(), Some(1));
    assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 0);

    for name in ["_smoke.jsonl", "_manifest.json"] {
        std::fs::copy(fixtures().join(name), scratch.0.join(name)).unwrap();
    }
    let output = gate(&scratch.0);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("exactly 61"));
    assert_eq!(std::fs::read_dir(&scratch.0).unwrap().count(), 2);
}

#[test]
fn sixty_one_empty_files_and_missing_or_malformed_metadata_fail() {
    let scratch = Scratch::new();
    for entry in std::fs::read_dir(fixtures()).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        if name.ends_with(".jsonl") {
            std::fs::write(scratch.0.join(name), "").unwrap();
        }
    }
    assert_eq!(gate(&scratch.0).status.code(), Some(1));
    std::fs::write(scratch.0.join("_manifest.json"), "{}").unwrap();
    assert_eq!(gate(&scratch.0).status.code(), Some(1));
    std::fs::copy(
        fixtures().join("_manifest.json"),
        scratch.0.join("_manifest.json"),
    )
    .unwrap();
    let output = gate(&scratch.0);
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("empty fixture"));
}
