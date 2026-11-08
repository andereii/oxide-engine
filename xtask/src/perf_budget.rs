//! `cargo xtask perf-budget` — the M5 phase-2b visual-slice frame budget.
//!
//! Implements the ADR-0020 / `tools/oracle/RENDER-BEHAVIOR.md` §7 budget under
//! the ADR-0020 addendum (2026-11-07): the easy difficulty scene is driven by
//! the same canonical gameplay replay as `gameplay-sim`/`render-diff`,
//! advanced to the first deterministic sample position after the receptor
//! fade, and rendered sequentially on a HARDWARE adapter. The renderer is
//! built with `SliceRenderer::build_for_adapter(.., AdapterPolicy::AnyHardware)`;
//! a `RenderError::NoHardwareAdapter` is propagated as a typed gate failure,
//! never caught, skipped or substituted with llvmpipe. Each warmup/measured
//! frame steps the next 16 canonical replay ticks (60 Hz discretization) before
//! rendering, and the measured interval includes both the 16 `step()` calls and
//! the single `frame()`. The first 120 frames are excluded warmup; the next 300
//! frame wall times are measured with a monotonic [`Instant`]. If the replay
//! runs out, the final reached state is rendered without looping the song.
//!
//! PASS iff `mean <= 16.667 ms AND max <= 33.333 ms`. These are hard
//! constants: loosening them requires the ADR/review/human path. Failure prints
//! the p50/p95/max distribution. The measurement is platform-specific hardware
//! evidence, NOT llvmpipe, NOT CI-agnostic and NOT a real-game/release-binary
//! latency claim.
//!
//! Debug-built binaries are not a valid perf measurement (ADR-0020 addendum
//! extra decision: debug measured 34.191 ms mean / 39.129 ms max on the same
//! hardware where release measures 3.9–14.4 ms). A debug build fails closed
//! with [`PerfError::DebugBuild`] before any scene construction or stepping.
//!
//! Both adapter policies print the chosen adapter identity through the
//! oxide-game harness (stderr); the gate additionally records that
//! `AnyHardware` selected a hardware class adapter. `oxide_game::render`
//! exposes no adapter accessor at its frozen public boundary, so the harness
//! stderr line remains the observed identity.

use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use oxide_game::render::{AdapterPolicy, RenderError, SliceRenderer};

use crate::gameplay_sim;
use crate::render_diff;

const WARMUP_FRAMES: usize = 120;
const MEASURED_FRAMES: usize = 300;
/// Native 60 Hz target, frozen by ADR-0020.
const MEAN_BUDGET_MS: f64 = 16.667;
const MAX_BUDGET_MS: f64 = 33.333;
const ADAPTER_CLASS: &str = "hardware";
const DEBUG_BUILD_HINT: &str =
    "run a release-built xtask, e.g. cargo run --release --bin xtask -- perf-budget --assets DIR";
const HONESTY: &str = "hardware adapter measurement (platform-specific; llvmpipe never \
                       substitutes; llvmpipe cannot pass the 60 Hz budget and is NOT a valid \
                       measurement); runs only in release-built xtask (debug build fails closed \
                       with DebugBuild); NOT a real-game/release-binary latency claim";

/// Pure debug-build hint used by [`PerfError::DebugBuild`].
pub(crate) fn debug_build_hint() -> &'static str {
    DEBUG_BUILD_HINT
}

/// Typed perf-budget failure. `NoHardwareAdapter` is preserved from the
/// renderer so the gate cannot silently downgrade to software; `DebugBuild`
/// prevents a slow debug measurement from being reported as a perf claim.
#[derive(Debug, thiserror::Error)]
enum PerfError {
    #[error("DebugBuild: debug build is not a valid perf measurement; {hint}")]
    DebugBuild { hint: &'static str },
    #[error(
        "no hardware adapter is available; perf-budget uses AdapterPolicy::AnyHardware and never \
         substitutes llvmpipe/software: {source}"
    )]
    NoHardwareAdapter {
        #[source]
        source: RenderError,
    },
    #[error("{0}")]
    Failure(String),
}

fn debug_build_guard() -> Result<(), PerfError> {
    if cfg!(debug_assertions) {
        Err(PerfError::DebugBuild {
            hint: debug_build_hint(),
        })
    } else {
        Ok(())
    }
}

pub fn run(rest: &[String]) -> ExitCode {
    let assets_root = match render_diff::assets_only_cli(rest, "perf-budget") {
        Ok(path) => path,
        Err(message) => {
            eprintln!("perf-budget: {message}");
            return ExitCode::from(2);
        }
    };
    if !assets_root.is_dir() {
        eprintln!(
            "perf-budget: prerequisite: assets directory is missing or not a directory: {}",
            assets_root.display()
        );
        return ExitCode::FAILURE;
    }
    if let Err(error) = debug_build_guard() {
        println!("perf-budget: {error}");
        eprintln!("perf-budget: {error}");
        return ExitCode::FAILURE;
    }
    println!("perf-budget: honesty: {HONESTY}");
    match measure(&assets_root) {
        Ok(report) => {
            print_report(&report);
            if report.evaluation.pass {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("perf-budget: {error}");
            ExitCode::FAILURE
        }
    }
}

struct PerfReport {
    scene_ms: u64,
    warmup: usize,
    measured: usize,
    steps_per_frame: usize,
    ticks_consumed: usize,
    ticks_total: usize,
    adapter_class: &'static str,
    evaluation: BudgetEvaluation,
}

/// 60 Hz frame discretization: 16.667 ms of song per rendered frame is stepped
/// as 16 canonical 1 ms replay ticks before each `frame()` call (ADR-0020
/// addendum; frozen by this gate brief).
const STEPS_PER_FRAME: usize = 16;

fn step_batch(
    renderer: &mut SliceRenderer,
    replay: &gameplay_sim::CanonicalReplay,
    next_tick: &mut usize,
) -> Result<usize, String> {
    let mut stepped = 0usize;
    while stepped < STEPS_PER_FRAME {
        let Some(tick) = replay.ticks.get(*next_tick) else {
            break;
        };
        renderer
            .step(tick.sim_ms, &tick.events, &tick.inputs)
            .map_err(|error| {
                format!("renderer rejected canonical tick {}: {error}", tick.sim_ms)
            })?;
        *next_tick += 1;
        stepped += 1;
    }
    Ok(stepped)
}

fn measure(assets_root: &Path) -> Result<PerfReport, PerfError> {
    let renderer =
        match SliceRenderer::build_for_adapter(assets_root, "easy", AdapterPolicy::AnyHardware) {
            Ok(renderer) => renderer,
            Err(error @ RenderError::NoHardwareAdapter { .. }) => {
                return Err(PerfError::NoHardwareAdapter { source: error });
            }
            Err(error) => {
                return Err(PerfError::Failure(format!(
                    "renderer build failed for AdapterPolicy::AnyHardware: {error}"
                )));
            }
        };
    measure_with_renderer(assets_root, renderer).map_err(PerfError::Failure)
}

fn measure_with_renderer(
    assets_root: &Path,
    mut renderer: SliceRenderer,
) -> Result<PerfReport, String> {
    let notes = gameplay_sim::load_verified_notes(assets_root, "easy")?;
    let replay = gameplay_sim::canonical_replay(assets_root, "easy", &notes)?;
    let samples = render_diff::sample_positions(assets_root, &notes, &replay)?;
    let scene_ms = samples
        .first()
        .copied()
        .ok_or_else(|| "the deterministic sample plan is empty".to_string())?;
    let replay_end = replay.ticks.last().map_or(0, |tick| tick.sim_ms);
    if scene_ms > replay_end {
        return Err(format!(
            "perf scene tick {scene_ms} lies after the canonical replay end {replay_end}"
        ));
    }

    // Align the continuous replay pointer to the first post-fade sample; the
    // warmup/measurement loops then step 16 ticks per rendered frame.
    let mut next_tick = 0usize;
    while let Some(tick) = replay.ticks.get(next_tick) {
        if tick.sim_ms >= scene_ms {
            break;
        }
        renderer
            .step(tick.sim_ms, &tick.events, &tick.inputs)
            .map_err(|error| {
                format!("renderer rejected canonical tick {}: {error}", tick.sim_ms)
            })?;
        next_tick += 1;
    }
    if !replay
        .ticks
        .get(next_tick)
        .is_some_and(|tick| tick.sim_ms >= scene_ms)
    {
        return Err(format!(
            "canonical replay never reached perf scene tick {scene_ms}"
        ));
    }

    for _ in 0..WARMUP_FRAMES {
        step_batch(&mut renderer, &replay, &mut next_tick)?;
        let frame = renderer
            .frame()
            .map_err(|error| format!("warmup frame failed: {error}"))?;
        std::hint::black_box(&frame);
    }
    let mut durations_ms = Vec::with_capacity(MEASURED_FRAMES);
    for index in 0..MEASURED_FRAMES {
        // The measured interval is the frozen "16 ticks of step() + one
        // frame()" unit, not a static-frame render.
        let start = Instant::now();
        step_batch(&mut renderer, &replay, &mut next_tick)?;
        let frame = renderer
            .frame()
            .map_err(|error| format!("measured frame {index} failed: {error}"))?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        std::hint::black_box(&frame);
        durations_ms.push(elapsed_ms);
    }
    let evaluation = evaluate_budget(&durations_ms)?;
    Ok(PerfReport {
        scene_ms,
        warmup: WARMUP_FRAMES,
        measured: MEASURED_FRAMES,
        steps_per_frame: STEPS_PER_FRAME,
        ticks_consumed: next_tick,
        ticks_total: replay.ticks.len(),
        adapter_class: ADAPTER_CLASS,
        evaluation,
    })
}

fn print_report(report: &PerfReport) {
    let evaluation = &report.evaluation;
    println!(
        "perf-budget: adapter policy: AnyHardware (hardware required; llvmpipe never substitutes)"
    );
    println!(
        "perf-budget: adapter class: {} (selected by SliceRenderer::build_for_adapter; the actual \
         RenderAdapterInfo is echoed by the oxide-game harness on stderr)",
        report.adapter_class
    );
    println!(
        "perf-budget: difficulty easy; scene start tick {} ms; frames: warmup {} excluded, \
         measured {} sequential; steps-per-frame {} (60 Hz discretization); total replay ticks \
         consumed {} / {}",
        report.scene_ms,
        report.warmup,
        report.measured,
        report.steps_per_frame,
        report.ticks_consumed,
        report.ticks_total
    );
    println!(
        "perf-budget: frame wall time mean {:.3} ms; max {:.3} ms",
        evaluation.mean_ms, evaluation.max_ms
    );
    println!(
        "perf-budget: budget: mean <= {MEAN_BUDGET_MS:.3} ms AND max <= {MAX_BUDGET_MS:.3} ms"
    );
    if evaluation.pass {
        println!("perf-budget: PASS: both frame-wall-time bounds hold; {HONESTY}");
    } else {
        println!(
            "perf-budget: distribution: p50 {:.3} ms; p95 {:.3} ms; max {:.3} ms",
            evaluation.p50_ms, evaluation.p95_ms, evaluation.max_ms
        );
        if !evaluation.mean_ok {
            println!(
                "perf-budget: FAIL: mean {:.3} ms exceeds {MEAN_BUDGET_MS:.3} ms",
                evaluation.mean_ms
            );
        }
        if !evaluation.max_ok {
            println!(
                "perf-budget: FAIL: max {:.3} ms exceeds {MAX_BUDGET_MS:.3} ms",
                evaluation.max_ms
            );
        }
        println!("perf-budget: FAIL: hard ADR-0020 budget; no tolerance loosening");
    }
}

#[derive(Debug)]
struct BudgetEvaluation {
    mean_ms: f64,
    max_ms: f64,
    p50_ms: f64,
    p95_ms: f64,
    mean_ok: bool,
    max_ok: bool,
    pass: bool,
}

fn evaluate_budget(durations_ms: &[f64]) -> Result<BudgetEvaluation, String> {
    if durations_ms.is_empty() {
        return Err("no measured frame durations".into());
    }
    if durations_ms
        .iter()
        .any(|duration| !duration.is_finite() || *duration < 0.0)
    {
        return Err("measured frame durations must be finite and non-negative".into());
    }
    let mut sorted = durations_ms.to_vec();
    sorted.sort_by(f64::total_cmp);
    let sum: f64 = sorted.iter().sum();
    let mean_ms = sum / sorted.len() as f64;
    let max_ms = *sorted.last().unwrap_or(&0.0);
    let p50_ms = percentile(&sorted, 50);
    let p95_ms = percentile(&sorted, 95);
    let mean_ok = mean_ms <= MEAN_BUDGET_MS;
    let max_ok = max_ms <= MAX_BUDGET_MS;
    Ok(BudgetEvaluation {
        mean_ms,
        max_ms,
        p50_ms,
        p95_ms,
        mean_ok,
        max_ok,
        pass: mean_ok && max_ok,
    })
}

/// Nearest-rank percentile over an ascending slice.
fn percentile(sorted: &[f64], percent: usize) -> f64 {
    let rank = (percent * sorted.len()).div_ceil(100).saturating_sub(1);
    sorted
        .get(rank)
        .copied()
        .or_else(|| sorted.last().copied())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passing_budget_requires_both_bounds() {
        let evaluation = evaluate_budget(&[10.0; 300]).unwrap();
        assert!(evaluation.pass, "{evaluation:?}");
        assert!(evaluation.mean_ok);
        assert!(evaluation.max_ok);
        assert_eq!(evaluation.mean_ms, 10.0);
        assert_eq!(evaluation.max_ms, 10.0);
    }

    #[test]
    fn mean_over_budget_fails_even_when_max_is_below() {
        let durations = [17.0; 300];
        let evaluation = evaluate_budget(&durations).unwrap();
        assert!(!evaluation.mean_ok);
        assert!(evaluation.max_ok);
        assert!(!evaluation.pass);
    }

    #[test]
    fn max_over_budget_fails_even_when_mean_is_below() {
        let mut durations = vec![1.0; 299];
        durations.push(40.0);
        let evaluation = evaluate_budget(&durations).unwrap();
        assert!(evaluation.mean_ok);
        assert!(!evaluation.max_ok);
        assert!(!evaluation.pass);
        assert_eq!(evaluation.max_ms, 40.0);
    }

    #[test]
    fn budget_boundary_values_pass_inclusively() {
        let evaluation = evaluate_budget(&[MEAN_BUDGET_MS]).unwrap();
        assert!(evaluation.mean_ok);
        assert!(evaluation.max_ok);
        assert!(evaluation.pass);
        let mut durations = vec![1.0; 299];
        durations.push(MAX_BUDGET_MS);
        let evaluation = evaluate_budget(&durations).unwrap();
        assert!(evaluation.max_ok);
        assert!(evaluation.pass, "{evaluation:?}");
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let values: Vec<f64> = (1..=300).map(f64::from).collect();
        assert_eq!(percentile(&values, 50), 150.0);
        assert_eq!(percentile(&values, 95), 285.0);
        assert_eq!(percentile(&[], 95), 0.0);
    }

    #[test]
    fn empty_or_invalid_durations_fail_closed() {
        assert!(evaluate_budget(&[]).is_err());
        assert!(evaluate_budget(&[f64::NAN]).is_err());
        assert!(evaluate_budget(&[-1.0]).is_err());
    }

    #[test]
    fn no_hardware_adapter_is_a_typed_failure_not_a_skip() {
        let error = PerfError::NoHardwareAdapter {
            source: RenderError::NoHardwareAdapter {
                candidates: vec!["llvmpipe".to_string()],
            },
        };
        let message = error.to_string();
        assert!(message.contains("no hardware adapter"), "{message}");
        assert!(message.contains("never substitutes llvmpipe"), "{message}");
        assert!(message.contains("llvmpipe"), "{message}");
    }

    #[test]
    fn frame_step_batch_is_frozen_at_sixteen_ticks() {
        assert_eq!(STEPS_PER_FRAME, 16);
    }

    #[test]
    fn debug_build_guard_is_typed_and_points_at_release() {
        let hint = debug_build_hint();
        assert!(
            hint.contains("cargo run --release --bin xtask -- perf-budget --assets DIR"),
            "{hint}"
        );
        let message = PerfError::DebugBuild { hint }.to_string();
        assert!(
            message.contains("debug build is not a valid perf measurement"),
            "{message}"
        );
        assert!(message.contains("release-built xtask"), "{message}");
        if cfg!(debug_assertions) {
            assert!(matches!(
                debug_build_guard(),
                Err(PerfError::DebugBuild { .. })
            ));
        } else {
            assert!(debug_build_guard().is_ok());
        }
    }
}
