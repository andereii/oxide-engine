//! Real-asset integration tests for the M5 phase-2b slice renderer.
//!
//! These tests require the reference assets (`OXIDE_ASSETS_ROOT`) and are
//! ignored by default, following the repository's other real-asset tests. Run
//! explicitly for milestone acceptance:
//!
//! ```sh
//! OXIDE_ASSETS_ROOT=/path/to/assets \
//!   cargo test -p oxide-game --test render_slice -- --include-ignored --test-threads=1
//! ```
//!
//! `any_hardware_adapter_renders_frames` additionally exercises the
//! `AdapterPolicy::AnyHardware` path (ADR-0020 addendum). On a machine without
//! a hardware GPU it prints the `NoHardwareAdapter` candidate list and passes.
//!
//! The printed SHA-256 values are diagnostic evidence only; no golden digest
//! is stored or compared here (fixtures are owned by the orchestrator).

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

use oxide_game::render::{AdapterPolicy, RenderError, SliceRenderer};
use oxide_game::slice::{GameplaySimState, InputEvent, TallyCounts, TraceEvent};

const FRAME_BYTES: usize = 1280 * 720 * 4;

/// Serialize the ignored real-asset tests: each builds a Bevy app, and running
/// several adapters concurrently is needlessly heavy.
static TEST_LOCK: Mutex<()> = Mutex::new(());

fn test_lock() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn assets_root() -> PathBuf {
    let root = std::env::var_os("OXIDE_ASSETS_ROOT")
        .expect("set OXIDE_ASSETS_ROOT to the reference assets directory");
    let root = PathBuf::from(root);
    assert!(
        root.is_dir(),
        "OXIDE_ASSETS_ROOT must point to an existing assets directory: {}",
        root.display()
    );
    root
}

/// Player-lane chart notes in sim-note-id order: `(id, direction, sustain_ms)`.
fn player_notes(root: &Path, difficulty: &str) -> Vec<(u32, i64, f64)> {
    let path = root.join("preload/data/songs/bopeebo/bopeebo-chart.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let chart: serde_json::Value =
        serde_json::from_str(&text).expect("bopeebo chart is valid JSON");
    let notes = chart["notes"][difficulty]
        .as_array()
        .unwrap_or_else(|| panic!("chart has no {difficulty:?} note list"));
    let mut player = Vec::new();
    for note in notes {
        let direction = note["d"].as_i64().unwrap_or(-1);
        if (0..=3).contains(&direction) {
            let sustain = note
                .get("l")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.0);
            player.push((player.len() as u32, direction, sustain));
        }
    }
    player
}

fn hit_event(sim: u64, note_id: u32, direction: i64) -> TraceEvent {
    TraceEvent::Hit {
        schema: "1",
        kind: "hit",
        sim,
        input_ms: sim as f64,
        input_time_us: 0.0,
        note_id,
        note_direction: direction,
        judgement: "sick",
        raw_note_diff: 0.0,
        note_diff_int: 0,
        score_delta: 500,
        health_before: 1.0,
        health_after: 1.03,
        combo_before: 0,
        combo_after: 1,
        combo_max: 1,
        tallies: TallyCounts {
            sick: 1,
            ..TallyCounts::default()
        },
    }
}

fn hold_start_event(sim: u64, note_id: u32, direction: i64) -> TraceEvent {
    TraceEvent::HoldStart {
        schema: "1",
        kind: "hold_start",
        sim,
        input_ms: sim as f64,
        input_time_us: 0.0,
        note_id,
        note_direction: direction,
        judgement: "sick",
        raw_note_diff: 0.0,
        note_diff_int: 0,
    }
}

fn miss_event(sim: u64, note_id: u32, direction: i64) -> TraceEvent {
    TraceEvent::Miss {
        schema: "1",
        kind: "miss",
        sim,
        input_ms: 0.0,
        input_time_us: 0.0,
        note_id,
        note_direction: direction,
        judgement: "miss",
        raw_note_diff: 0.0,
        note_diff_int: 0,
        score_delta: -100,
        health_before: 1.0,
        health_after: 0.92,
        combo_before: 0,
        combo_after: 0,
        combo_max: 0,
        tallies: TallyCounts {
            missed: 1,
            ..TallyCounts::default()
        },
    }
}

fn ghost_event(sim: u64, direction: i64) -> TraceEvent {
    TraceEvent::GhostPress {
        schema: "1",
        kind: "ghost_press",
        sim,
        input_ms: sim as f64,
        input_time_us: 0.0,
        note_id: 0,
        note_direction: direction,
        judgement: "none",
        raw_note_diff: 0.0,
        note_diff_int: 0,
        score_delta: -10.0,
        health_change: -0.08,
    }
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn headless_smoke_40ms_hit_miss_hold_ghost() {
    let _lock = test_lock();
    let root = assets_root();
    let notes = player_notes(&root, "normal");
    let (hit_id, hit_direction, _) = *notes
        .iter()
        .find(|(_, _, sustain)| *sustain > 0.0)
        .expect("bopeebo normal has a sustained player note");
    let (miss_id, miss_direction, _) = notes
        .iter()
        .find(|(id, _, _)| *id != hit_id)
        .copied()
        .expect("bopeebo normal has at least two player notes");

    let mut sim = GameplaySimState::build(&root, "normal").expect("build gameplay sim");
    let mut renderer = SliceRenderer::build(&root, "normal").expect("build slice renderer");

    for tick in 0..=40u64 {
        let inputs = match tick {
            10 => vec![InputEvent::Press {
                sim_ms: tick,
                direction: 0,
            }],
            20 => vec![InputEvent::Release {
                sim_ms: tick,
                direction: 0,
            }],
            _ => Vec::new(),
        };
        let mut events = sim.step(tick, &inputs);
        if tick == 30 {
            events.push(hit_event(tick, hit_id, hit_direction));
            events.push(hold_start_event(tick, hit_id, hit_direction));
            events.push(miss_event(tick, miss_id, miss_direction));
            events.push(ghost_event(tick, (hit_direction + 1) % 4));
        }
        renderer
            .step(tick, &events, &inputs)
            .unwrap_or_else(|error| panic!("renderer step {tick}: {error}"));

        if matches!(tick, 10 | 30 | 40) {
            let started = Instant::now();
            let first = renderer.frame().expect("render frame");
            let elapsed = started.elapsed();
            assert_eq!(first.len(), FRAME_BYTES, "frame byte length");
            let second = renderer.frame().expect("render repeated frame");
            assert_frames_equal(&first, &second, "smoke repeated frame");
            println!(
                "smoke normal tick {tick:>3}: sha256={} render={:?}",
                sha256_hex(&first),
                elapsed
            );
        }
    }
}

#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn real_assets_all_difficulties_fixed_positions() {
    let _lock = test_lock();
    let root = assets_root();
    let sample_ticks = [600u64, 1200, 2000];
    for difficulty in ["easy", "normal", "hard"] {
        let mut sim = GameplaySimState::build(&root, difficulty).expect("build gameplay sim");
        let mut renderer = SliceRenderer::build(&root, difficulty).expect("build slice renderer");
        for tick in 0..=*sample_ticks.last().expect("sample ticks") {
            let events = sim.step(tick, &[]);
            renderer
                .step(tick, &events, &[])
                .unwrap_or_else(|error| panic!("{difficulty} step {tick}: {error}"));
            if sample_ticks.contains(&tick) {
                let started = Instant::now();
                let first = renderer.frame().expect("render frame");
                let elapsed = started.elapsed();
                assert_eq!(first.len(), FRAME_BYTES, "frame byte length");
                let second = renderer.frame().expect("render repeated frame");
                assert_frames_equal(
                    &first,
                    &second,
                    &format!("{difficulty} tick {tick} repeated frame"),
                );
                println!(
                    "{difficulty} tick {tick:>4}: sha256={} render={:?}",
                    sha256_hex(&first),
                    elapsed
                );
            }
        }
    }
}

/// Opponent-lane note count (chart lanes 4..=7) within `horizon_ms` for the
/// schema-2 coverage assertion of the equivalence test below (review finding
/// 3): the test simulates to `horizon_ms`, so only notes whose auto-hit fires
/// inside it can carry rows.
fn opponent_notes_through(root: &Path, difficulty: &str, horizon_ms: u64) -> usize {
    let path = root.join("preload/data/songs/bopeebo/bopeebo-chart.json");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let chart: serde_json::Value =
        serde_json::from_str(&text).expect("bopeebo chart is valid JSON");
    chart["notes"][difficulty]
        .as_array()
        .unwrap_or_else(|| panic!("chart has no {difficulty:?} note list"))
        .iter()
        .filter(|note| {
            let direction = note["d"].as_i64().unwrap_or(-1);
            let time = note["t"].as_f64().unwrap_or(f64::INFINITY);
            direction >= 4 && time <= horizon_ms as f64
        })
        .count()
}

/// A schema-2 trace (with the sim's `OpponentHit` rows) must produce frames
/// byte-identical to the schema-1 bridge path (`trigger_opponent_notes`) over
/// the same ticks, difficulty and adapter: docs/M6-CONTRACT.md §10.3/§10.4.
/// Coverage guard (review finding 3): the schema-2 stream must carry EXACTLY
/// one `opponent_hit` row per opponent chart note, so the row path (not the
/// legacy bridge) drives the schema-2 scene.
#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for milestone acceptance"]
fn schema_two_rows_match_the_schema_one_bridge_frames() {
    let _lock = test_lock();
    let root = assets_root();
    for difficulty in ["easy", "normal", "hard"] {
        let mut legacy = SliceRenderer::build(&root, difficulty)
            .unwrap_or_else(|error| panic!("schema-1 renderer build {difficulty}: {error}"));
        let mut schema2 = SliceRenderer::build(&root, difficulty)
            .unwrap_or_else(|error| panic!("schema-2 renderer build {difficulty}: {error}"));
        let mut player_sim =
            GameplaySimState::build(&root, difficulty).expect("schema-1 gameplay sim");
        let mut pub_sim = GameplaySimState::build_chart(&root, "bopeebo", difficulty)
            .expect("schema-2 gameplay sim");
        const HORIZON_MS: u64 = 2000;
        let edge = opponent_notes_through(&root, difficulty, HORIZON_MS);
        let mut opponent_rows = 0usize;
        for tick in 0..=HORIZON_MS {
            let legacy_events = player_sim.step(tick, &[]);
            let schema2_events = pub_sim.step(tick, &[]);
            let kind = |event: &TraceEvent| -> &'static str {
                match event {
                    TraceEvent::Hit { .. } => "hit",
                    TraceEvent::Miss { .. } => "miss",
                    TraceEvent::GhostPress { .. } => "ghost_press",
                    TraceEvent::HoldStart { .. } => "hold_start",
                    TraceEvent::HoldTick { .. } => "hold_tick",
                    TraceEvent::HoldDrop { .. } => "hold_drop",
                    TraceEvent::HoldEnd { .. } => "hold_end",
                    TraceEvent::GameOver { .. } => "game_over",
                    TraceEvent::OpponentHit { .. } => "opponent_hit",
                }
            };
            let player_kinds: Vec<_> = legacy_events.iter().map(kind).collect();
            let schema2_kinds: Vec<_> = schema2_events
                .iter()
                .filter(|event| kind(event) != "opponent_hit")
                .map(kind)
                .collect();
            assert_eq!(
                player_kinds, schema2_kinds,
                "{difficulty} tick {tick}: player-path event kinds must match (schema-2-only opponent rows excluded)"
            );
            opponent_rows += schema2_events
                .iter()
                .filter(|event| matches!(event, TraceEvent::OpponentHit { .. }))
                .count();
            legacy
                .step(tick, &legacy_events, &[])
                .unwrap_or_else(|error| panic!("legacy step {difficulty} tick {tick}: {error}"));
            schema2
                .step(tick, &schema2_events, &[])
                .unwrap_or_else(|error| panic!("schema-2 step {difficulty} tick {tick}: {error}"));
            if tick % 500 == 0 {
                let legacy_frame = legacy.frame().expect("schema-1 frame");
                let schema2_frame = schema2.frame().expect("schema-2 frame");
                assert_frames_equal(
                    &legacy_frame,
                    &schema2_frame,
                    &format!("{difficulty} schema-1/schema-2 frame at tick {tick}"),
                );
            }
        }
        let legacy_final = legacy.frame().expect("final schema-1 frame");
        let schema2_final = schema2.frame().expect("final schema-2 frame");
        assert_frames_equal(
            &legacy_final,
            &schema2_final,
            &format!("{difficulty} final frame"),
        );
        assert_eq!(
            opponent_rows, edge,
            "{difficulty}: schema-2 trace must carry exactly one opponent_hit row per opponent \
             chart note (the row path, not the bridge, drives the scene)"
        );
        println!(
            "{difficulty}: schema-2 rows match the schema-1 bridge through tick 2000; final sha256={}",
            sha256_hex(&schema2_final)
        );
    }
}

/// ADR-0020 addendum explicit-adapter path: exercise `AnyHardware` when a
/// hardware GPU is available. On machines with only a software adapter the
/// harness returns the typed `NoHardwareAdapter` error and the test prints the
/// candidate list and passes (documented environment guard). Hardware frame
/// bytes/digests are adapter-specific and are never compared with llvmpipe.
#[test]
#[ignore = "requires OXIDE_ASSETS_ROOT; needs a hardware GPU, otherwise prints NoHardwareAdapter and passes"]
fn any_hardware_adapter_renders_frames() {
    let _lock = test_lock();
    let root = assets_root();
    let mut sim = GameplaySimState::build(&root, "easy").expect("build gameplay sim");
    let mut renderer =
        match SliceRenderer::build_for_adapter(&root, "easy", AdapterPolicy::AnyHardware) {
            Ok(renderer) => renderer,
            Err(RenderError::NoHardwareAdapter { candidates }) => {
                println!(
                    "anyhardware: skipped, no hardware adapter available; candidates={candidates:?}"
                );
                return;
            }
            Err(error) => panic!("AnyHardware build failed: {error}"),
        };
    for tick in 0..=600u64 {
        let events = sim.step(tick, &[]);
        renderer
            .step(tick, &events, &[])
            .unwrap_or_else(|error| panic!("anyhardware step {tick}: {error}"));
    }
    // Three consecutive readbacks (plus a repeat of each) exercise the
    // repeated-capture path that was flaky with the old Screenshot design.
    let mut tick = 600u64;
    for index in 0..3 {
        let frame = renderer.frame().expect("render hardware frame");
        assert_eq!(frame.len(), FRAME_BYTES, "hardware frame byte length");
        let repeated = renderer.frame().expect("render repeated hardware frame");
        assert_frames_equal(&frame, &repeated, "anyhardware repeated frame");
        println!(
            "anyhardware easy tick {tick}: sha256={} (adapter-specific; not comparable with llvmpipe)",
            sha256_hex(&frame)
        );
        if index < 2 {
            tick += 1;
            let events = sim.step(tick, &[]);
            renderer
                .step(tick, &events, &[])
                .unwrap_or_else(|error| panic!("anyhardware step {tick}: {error}"));
        }
    }
}

/// Compact frame-diff diagnostics: the full 3.7 MB vectors are not printed.
fn assert_frames_equal(first: &[u8], second: &[u8], label: &str) {
    assert_eq!(first.len(), second.len(), "{label}: length");
    let mut differing = 0usize;
    let mut first_diff = None;
    for (index, (left, right)) in first.iter().zip(second.iter()).enumerate() {
        if left != right {
            differing += 1;
            if first_diff.is_none() {
                first_diff = Some((index, *left, *right));
            }
        }
    }
    assert_eq!(
        differing, 0,
        "{label}: {differing} differing bytes, first diff {first_diff:?}"
    );
}

#[test]
fn sha256_helper_matches_known_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

/// Self-contained SHA-256 for diagnostic frame digests (no extra dependency).
fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_len.to_be_bytes());

    for chunk in message.as_chunks::<64>().0 {
        let mut schedule = [0u32; 64];
        for (index, word) in chunk.as_chunks::<4>().0.iter().enumerate() {
            schedule[index] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for index in 16..64 {
            let s0 = schedule[index - 15].rotate_right(7)
                ^ schedule[index - 15].rotate_right(18)
                ^ (schedule[index - 15] >> 3);
            let s1 = schedule[index - 2].rotate_right(17)
                ^ schedule[index - 2].rotate_right(19)
                ^ (schedule[index - 2] >> 10);
            schedule[index] = schedule[index - 16]
                .wrapping_add(s0)
                .wrapping_add(schedule[index - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for index in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ (!e & g);
            let temp1 = h
                .wrapping_add(s1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(schedule[index]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(majority);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }
        for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *slot = slot.wrapping_add(value);
        }
    }

    state
        .iter()
        .map(|word| format!("{word:08x}"))
        .collect::<String>()
}
