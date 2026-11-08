//! `xtask render-diff` / `xtask perf-budget` CLI regressions.
//!
//! These tests never build a real Bevy scene or require game assets: they
//! exercise usage and prerequisite failure paths only. The pure sample-plan,
//! manifest-drift, digest-mismatch and budget-threshold logic lives in the
//! module unit tests, where a real renderer run is not involved either.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DIR: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../target/render-gate-cli-tests")
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

fn xtask(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(args)
        .output()
        .expect("spawn xtask")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn missing_assets_flag_is_a_usage_error() {
    for gate in ["render-diff", "perf-budget"] {
        let output = xtask(&[gate]);
        assert_eq!(output.status.code(), Some(2), "{gate}: {output:?}");
        assert!(
            stderr(&output).contains("missing --assets"),
            "{gate}: {}",
            stderr(&output)
        );
        assert!(!String::from_utf8_lossy(&output.stdout).contains("PASS"));
    }
}

#[test]
fn unknown_flags_are_usage_errors() {
    for gate in ["render-diff", "perf-budget"] {
        let output = xtask(&[gate, "--assets", "unused", "--bogus"]);
        assert_eq!(output.status.code(), Some(2), "{gate}: {output:?}");
        assert!(
            stderr(&output).contains("unsupported argument"),
            "{gate}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn a_missing_assets_directory_is_a_prerequisite_failure() {
    let scratch = Scratch::new("missing-assets");
    for gate in ["render-diff", "perf-budget"] {
        let output = xtask(&[
            gate,
            "--assets",
            scratch
                .path("missing")
                .to_str()
                .expect("test path is UTF-8"),
        ]);
        assert_eq!(output.status.code(), Some(1), "{gate}: {output:?}");
        assert!(
            stderr(&output).contains("prerequisite: assets directory is missing"),
            "{gate}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn fixtures_absent_is_reported_before_the_renderer_can_run() {
    let manifest =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/render-slice/_manifest.json");
    if manifest.is_file() {
        // Once the orchestrator freezes the fixture this is no longer the
        // absent path; the typed fixture error is covered by unit tests.
        return;
    }
    let scratch = Scratch::new("fixtures-absent");
    let output = xtask(&[
        "render-diff",
        "--assets",
        scratch.0.to_str().expect("test path is UTF-8"),
    ]);
    // The empty assets tree stops the renderer run, but the absent-fixture
    // diagnostic is still printed before that prerequisite failure.
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("fixtures not established"),
        "{output:?}"
    );
    assert!(!manifest.exists(), "the gate must not create fixtures");
}

#[test]
fn perf_budget_without_a_real_chart_fails_closed() {
    let scratch = Scratch::new("perf-empty-assets");
    let output = xtask(&[
        "perf-budget",
        "--assets",
        scratch.0.to_str().expect("test path is UTF-8"),
    ]);
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    if cfg!(debug_assertions) {
        // CLI tests run a debug-built xtask: the release-only guard must fire
        // before any scene construction or asset loading.
        assert!(
            stderr(&output).contains("debug build is not a valid perf measurement"),
            "expected the typed DebugBuild guard: {}",
            stderr(&output)
        );
        assert!(
            stderr(&output).contains("release-built xtask"),
            "expected the release hint: {}",
            stderr(&output)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("DebugBuild"),
            "the typed guard must be recorded on stdout too: {output:?}"
        );
        assert!(!stderr(&output).contains("cannot read asset"));
    } else {
        assert!(
            stderr(&output).contains("asset"),
            "expected a missing-asset diagnostic: {}",
            stderr(&output)
        );
    }
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PASS"));
}
