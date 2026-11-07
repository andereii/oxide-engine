//! Frozen native Conductor reference gate must fail closed on any drift.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_DIR: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oxide-conductor-native-gate-{}-{}",
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

fn committed() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/conductor-native")
}

fn gate(path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["conductor-native", "--fixtures"])
        .arg(path)
        .output()
        .unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_committed_frozen_native_rows_pass() {
    let output = gate(&committed());
    assert!(
        output.status.success(),
        "committed native reference rows must replay green: {}",
        stderr(&output)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("native ok: 62/62 failed: 0"));
    assert!(stdout.contains("not a full-game integration trace"));
}

#[test]
fn drifting_native_rows_fail_closed() {
    let scratch = Scratch::new();
    for entry in std::fs::read_dir(committed()).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), scratch.0.join(entry.file_name())).unwrap();
    }
    let target = scratch.0.join("2hot-default.jsonl");
    let mut text = std::fs::read_to_string(&target).unwrap();
    text = text.replacen("\"", "\"\" ", 1); // byte-level mutation of the frozen rows
    std::fs::write(&target, text).unwrap();
    let output = gate(&scratch.0);
    assert!(!output.status.success(), "row drift must fail");
    let message = stderr(&output);
    assert!(
        message.contains("sha256 differs from the committed manifest"),
        "{message}"
    );
}

#[test]
fn a_staged_partial_set_cannot_pass() {
    let scratch = Scratch::new();
    // Copy the manifest but only one chart: the digest set misses 61 ids.
    let manifest = scratch.0.join("_manifest_native.json");
    std::fs::copy(committed().join("_manifest_native.json"), &manifest).unwrap();
    std::fs::copy(
        committed().join("2hot-default.jsonl"),
        scratch.0.join("2hot-default.jsonl"),
    )
    .unwrap();
    let output = gate(&scratch.0);
    assert!(!output.status.success(), "micro corpus must not pass");
    let message = stderr(&output);
    assert!(
        message.contains("cannot read frozen native rows") || message.contains("sha256 differs"),
        "{message}"
    );
}

#[test]
fn usage_is_failing_with_exit_code_2() {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["conductor-native", "--bogus"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "usage problems exit 2");
    assert!(stderr(&output).contains("unsupported arguments"));
}
