//! Public-boundary regression for the M5 phase-1 trace: two fresh sims over
//! the same synthesized stream must serialize byte-identically, and the
//! canonical bytes are frozen as a golden. Hashing stays in the xtask gate;
//! this crate test only compares bytes and avoids a hashing dependency. The
//! fixture is embedded read-only via `include_str!`; no test writes or
//! regenerates it.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use oxide_game::slice::{GameplaySimState, InputEvent};

static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

const CHART: &str = r#"{
  "version": "2.0.0",
  "scrollSpeed": { "easy": 1.2, "normal": 1.3, "hard": 1.6 },
  "events": [],
  "notes": {
    "easy": [
      { "t": 0, "d": 0 },
      { "t": 100, "d": 1 }
    ]
  }
}"#;

const METADATA: &str = r#"{
  "version": "2.2.4",
  "songName": "Bopeebo",
  "playData": { "difficulties": ["easy", "normal", "hard"] },
  "timeChanges": [{ "t": 0, "bpm": 100 }]
}"#;

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "oxide-game-slice-integration-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let songs = root.join("preload/data/songs/bopeebo");
        fs::create_dir_all(&songs).expect("create fixture directory");
        fs::write(songs.join("bopeebo-chart.json"), CHART).expect("write fixture chart");
        fs::write(songs.join("bopeebo-metadata.json"), METADATA).expect("write fixture metadata");
        Self { root }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn press(sim_ms: u64, direction: i64) -> InputEvent {
    InputEvent::Press { sim_ms, direction }
}

fn event_ms(event: &InputEvent) -> u64 {
    match event {
        InputEvent::Press { sim_ms, .. } | InputEvent::Release { sim_ms, .. } => *sim_ms,
    }
}

fn canonical_trace(fixture: &Fixture) -> Vec<u8> {
    let mut inputs = vec![press(0, 0)];
    for ghost_ms in (10..=130).step_by(10) {
        inputs.push(press(ghost_ms, 3));
    }

    let mut sim = GameplaySimState::build(&fixture.root, "easy").expect("fixture builds");
    for sim_ms in 0..=300 {
        let tick: Vec<InputEvent> = inputs
            .iter()
            .filter(|event| event_ms(event) == sim_ms)
            .cloned()
            .collect();
        sim.step(sim_ms, &tick);
    }
    serde_json::to_vec(&sim.record()).expect("trace serializes")
}

#[test]
fn two_runs_over_the_same_stream_serialize_identically_and_match_the_golden() {
    let fixture = Fixture::new();
    let first = canonical_trace(&fixture);
    let second = canonical_trace(&fixture);
    assert_eq!(first, second, "two fresh sims diverged");
    let text = String::from_utf8(first).expect("trace is UTF-8");
    assert_eq!(text, normalized_golden(), "canonical trace bytes changed");

    let trace: Vec<serde_json::Value> = serde_json::from_str(&text).expect("trace is JSON");
    assert_eq!(
        trace
            .last()
            .and_then(|event| event.get("kind"))
            .and_then(serde_json::Value::as_str),
        Some("game_over"),
        "the dead play state must end the trace at game_over"
    );
    assert_eq!(
        trace
            .last()
            .and_then(|event| event.get("sim"))
            .and_then(serde_json::Value::as_u64),
        Some(131)
    );
    assert_eq!(
        trace
            .iter()
            .filter(
                |event| event.get("kind").and_then(serde_json::Value::as_str) == Some("game_over")
            )
            .count(),
        1,
        "death is detected exactly once"
    );
}

/// Checkout line endings are not part of the frozen trace. Normalize CRLF
/// before comparing so a Windows checkout cannot spuriously fail CI.
fn normalized_golden() -> String {
    GOLDEN
        .replace("\r\n", "\n")
        .trim_end_matches('\n')
        .to_owned()
}

/// Frozen canonical trace for the synthetic stream above. Hashing remains the
/// xtask gate's job; this fixture freezes the exact bytes and is never written
/// by a test.
const GOLDEN: &str = include_str!("fixtures/bopeebo-synthetic-trace.json");
