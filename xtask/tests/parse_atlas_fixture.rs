//! Regression: `xtask parse-atlas` must fail on the committed broken Sparrow
//! fixture (non-numeric width) and succeed on the committed good fixture.
//! Fixture path is orchestrator-owned: `tests/fixtures/atlas/**`.

use std::path::Path;
use std::process::Command;

fn repo_root() -> &'static Path {
    // cwd for integration tests is the crate dir; the repo root is its parent.
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    Path::new(crate_dir).parent().expect("crate dir has parent")
}

fn run_parse_atlas(dir: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
        .arg("parse-atlas")
        .arg("--dir")
        .arg(dir)
        .output()
        .expect("spawn xtask")
}

#[test]
fn broken_atlas_fixture_exits_one() {
    let fixture_dir = repo_root().join("tests/fixtures/atlas/broken");
    assert!(
        fixture_dir.is_dir(),
        "fixture missing: {}",
        fixture_dir.display()
    );

    let out = run_parse_atlas(&fixture_dir);
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 1, "parse-atlas must exit 1 on a broken atlas fixture");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("atlas failed: 1"),
        "failure must be counted in the gate block, got:\n{stdout}"
    );
}

#[test]
fn good_atlas_fixture_exits_zero() {
    let fixture_dir = repo_root().join("tests/fixtures/atlas/ok");
    assert!(
        fixture_dir.is_dir(),
        "fixture missing: {}",
        fixture_dir.display()
    );

    let out = run_parse_atlas(&fixture_dir);
    let code = out.status.code().unwrap_or(-1);
    assert_eq!(code, 0, "parse-atlas must accept the good fixture");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("xml visited: 1"), "got:\n{stdout}");
    assert!(stdout.contains("atlas ok: 1"), "got:\n{stdout}");
    assert!(stdout.contains("subtextures: 5"), "got:\n{stdout}");
}
