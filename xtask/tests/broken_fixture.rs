//! Regression: the broken fixture (chart whose "notes" is an array, on a
//! `songs/...-chart.json` path) must make `xtask parse-all` exit 1, not 0.
//! Fixture path is orchestrator-owned: `tests/fixtures/broken/**`.

use std::process::Command;

#[test]
fn broken_chart_fixture_exits_nonzero() {
    // cwd for integration tests is the crate dir; the repo root is its parent.
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let repo_root = std::path::Path::new(crate_dir).parent().unwrap();
    let fixture_dir = repo_root.join("tests/fixtures/broken");
    assert!(
        fixture_dir.is_dir(),
        "fixture missing: {}",
        fixture_dir.display()
    );

    let exe = env!("CARGO_BIN_EXE_xtask");
    let out = Command::new(exe)
        .arg("parse-all")
        .arg("--dir")
        .arg(&fixture_dir)
        .output()
        .expect("spawn xtask");
    let code = out.status.code().unwrap_or(-1);
    assert_ne!(code, 0, "parse-all must fail on the broken fixture");
    assert_eq!(
        code, 1,
        "parse-all must exit 1 (gate failure), not crash (2/101): {code}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("TOTAL") && stdout.contains("failed"),
        "output must carry the failure table, got:\n{stdout}"
    );
}
