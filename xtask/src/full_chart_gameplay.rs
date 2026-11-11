//! `cargo xtask full-chart-gameplay` — the M6 §6F full-chart acceptance gate.
//!
//! Corpus mode only: every `tools/oracle/corpus.txt` chart id (61) × every
//! difficulty declared by its metadata (166 measured runs). Each run must
//! pass:
//!
//! 1. the two-run schema-2 note-pass replay from
//!    [`gameplay_sim::corpus_run`]: two fresh sim builds, byte-identical
//!    serialized traces, sha256 equality, and zero `miss`/`game_over` rows
//!    for the synthesized exact taps;
//! 2. the two-run schema-3 event-pass replay from the same helper, with the
//!    per-kind row totals equal to the parsed chart-file `events` counts
//!    (`ScrollSpeed` `both` contributes one row per strumline, §12.6);
//! 3. a scene inventory LOAD via
//!    [`oxide_game::render::SliceRenderer::build_for_song`] for every run
//!    (stage, character, note-style and HUD page resolution end-to-end; this
//!    gate never renders a per-tick frame).
//!
//! Fail-closed: every planned run is attempted, all failures are reported,
//! and the command exits 0 only when the measured 61/166 inventory is
//! complete and every run passes both sim passes plus the scene load. A
//! missing/unreadable asset, a declared-difficulty inventory mismatch or a
//! scene-load error is a failure, never a skip.
//!
//! This gate proves deterministic engine simulation and asset loading, NOT
//! native-game parity. The final line names the deferred obligations: native
//! rendered-output traces (the render fixtures remain engine output on
//! llvmpipe, not native pixel parity), Windows/hardware evidence, CI
//! artifacts/dual-platform receipts, native camera/input probes and the M4
//! replay/input-jitter obligations. `render_diff` and `perf_budget` remain
//! separate gates with their own fixtures and measurements.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use oxide_game::render::SliceRenderer;

use crate::gameplay_sim;

const CLI_USAGE: &str = "usage: full-chart-gameplay --corpus --assets <dir>";
const TRACE_DIR: &str = "target/full-chart-gameplay";
/// Measured corpus inventory (docs/M6-CONTRACT.md §12.6): 61 chart ids and
/// 166 declared-difficulty runs.
const EXPECTED_CHART_IDS: usize = 61;
const EXPECTED_RUNS: usize = 166;
const DEFERRED_PARITY: &str = "deferred parity obligations NOT covered by this gate (this gate \
     proves deterministic engine simulation and asset loading, NOT native-game parity): native \
     rendered-output traces (render fixtures remain engine output on llvmpipe, not native pixel \
     parity), Windows/hardware evidence, CI artifacts/dual-platform receipts (dual_platform_ci), \
     native camera/input probes, M4 replay/input-jitter; render_diff and perf_budget remain \
     separate gates";

pub fn run(rest: &[String]) -> ExitCode {
    let cli = match Cli::parse(rest) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("full-chart-gameplay: {error}");
            return ExitCode::from(error.code());
        }
    };
    if !cli.assets_root.is_dir() {
        eprintln!(
            "full-chart-gameplay: prerequisite: assets directory is missing or not a directory: {}",
            cli.assets_root.display()
        );
        return ExitCode::FAILURE;
    }
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let trace_dir = workspace.join(TRACE_DIR);
    if let Err(error) = fs::create_dir_all(&trace_dir) {
        eprintln!(
            "full-chart-gameplay: cannot create trace directory {}: {error}",
            trace_dir.display()
        );
        return ExitCode::FAILURE;
    }
    let trace_dir = trace_dir.canonicalize().unwrap_or(trace_dir);

    println!(
        "full-chart-gameplay: assets {}; corpus mode: two-run schema-2 note pass + two-run \
         schema-3 event pass + scene inventory load (no per-tick rendering)",
        cli.assets_root.display()
    );

    let (plans, planning_failures) = plan_runs(&cli.assets_root);
    let mut report = execute_runs(
        &cli.assets_root,
        &trace_dir,
        &plans,
        gameplay_sim::corpus_run,
        scene_load,
    );
    if !planning_failures.is_empty() {
        let mut failures = planning_failures;
        failures.append(&mut report.failures);
        report.failures = failures;
    }
    print_report(&report, gameplay_sim::corpus_ids().len());
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Debug)]
struct Cli {
    assets_root: PathBuf,
}

#[derive(Debug)]
enum CliError {
    /// Malformed invocation (unknown flag, dangling value, missing mode).
    Usage(String),
    /// Well-formed invocation without a usable prerequisite.
    Prerequisite(String),
}

impl CliError {
    /// Process exit code: usage errors are 2, prerequisite failures are 1.
    fn code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            Self::Prerequisite(_) => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) | Self::Prerequisite(message) => formatter.write_str(message),
        }
    }
}

impl Cli {
    fn parse(rest: &[String]) -> Result<Self, CliError> {
        let mut assets = None;
        let mut corpus = false;
        let mut iter = rest.iter();
        while let Some(flag) = iter.next() {
            match flag.as_str() {
                "--assets" => {
                    let Some(value) = iter.next().filter(|value| !value.is_empty()) else {
                        return Err(CliError::Usage(format!(
                            "--assets requires a value; {CLI_USAGE}"
                        )));
                    };
                    assets = Some(PathBuf::from(value));
                }
                "--corpus" => corpus = true,
                other => {
                    return Err(CliError::Usage(format!(
                        "unsupported argument {other:?}; {CLI_USAGE}"
                    )));
                }
            }
        }
        if !corpus {
            return Err(CliError::Usage(format!(
                "--corpus is required (there is no non-corpus mode); {CLI_USAGE}"
            )));
        }
        let Some(assets_root) = assets else {
            return Err(CliError::Prerequisite(format!(
                "prerequisite: missing required --assets <dir>; {CLI_USAGE}"
            )));
        };
        Ok(Self { assets_root })
    }
}

#[derive(Debug, Clone)]
struct RunPlan {
    chart_id: String,
    difficulty: String,
}

/// Schedules every declared-difficulty run and records inventory problems.
///
/// The measured corpus is 61 chart ids / 166 runs; a different count is a
/// gate failure even if every scheduled run happens to pass.
fn plan_runs(assets_root: &Path) -> (Vec<RunPlan>, Vec<String>) {
    plan_runs_with(gameplay_sim::corpus_ids(), |chart_id| {
        gameplay_sim::declared_corpus_difficulties(assets_root, chart_id)
    })
}

/// [`plan_runs`] with the inventory and difficulty resolver injected (the
/// negative tests drive duplicate inventories through this seam).
fn plan_runs_with(
    ids: Vec<&str>,
    mut difficulties_for: impl FnMut(&str) -> Result<Vec<String>, String>,
) -> (Vec<RunPlan>, Vec<String>) {
    let mut failures = Vec::new();
    if ids.len() != EXPECTED_CHART_IDS {
        failures.push(format!(
            "corpus inventory declares {} chart-id(s); the measured corpus requires \
             {EXPECTED_CHART_IDS}",
            ids.len()
        ));
    }
    // Totals alone can hide drift: a duplicated id or difficulty can keep the
    // 61/166 counts while a measured run is never simulated. Fail closed on
    // non-distinct inventories (review round, 6F).
    let distinct_ids: BTreeSet<&str> = ids.iter().copied().collect();
    if distinct_ids.len() != ids.len() {
        failures.push(format!(
            "corpus inventory carries duplicate chart-id lines ({} distinct of {})",
            distinct_ids.len(),
            ids.len()
        ));
    }
    let mut plans = Vec::new();
    for chart_id in ids {
        match difficulties_for(chart_id) {
            Ok(difficulties) => {
                for difficulty in difficulties {
                    plans.push(RunPlan {
                        chart_id: chart_id.to_owned(),
                        difficulty,
                    });
                }
            }
            Err(error) => failures.push(format!("{chart_id}: {error}")),
        }
    }
    if plans.len() != EXPECTED_RUNS {
        failures.push(format!(
            "planned {} declared-difficulty run(s); the measured corpus requires {EXPECTED_RUNS}",
            plans.len()
        ));
    }
    let distinct_plans: BTreeSet<(&str, &str)> = plans
        .iter()
        .map(|plan| (plan.chart_id.as_str(), plan.difficulty.as_str()))
        .collect();
    if distinct_plans.len() != plans.len() {
        failures.push(format!(
            "planned runs carry duplicate (chart-id, difficulty) pairs ({} distinct of {})",
            distinct_plans.len(),
            plans.len()
        ));
    }
    (plans, failures)
}

#[derive(Debug)]
struct RunEvidence {
    chart_id: String,
    difficulty: String,
    note_digest: String,
    event_digest: String,
    event_rows: usize,
    event_counts: Vec<(String, usize)>,
}

#[derive(Debug, Default)]
struct GateReport {
    planned_runs: usize,
    evidence: Vec<RunEvidence>,
    failures: Vec<String>,
    aggregate: BTreeMap<String, usize>,
}

impl GateReport {
    fn record_success(&mut self, plan: &RunPlan, run: gameplay_sim::CorpusRun) {
        for (kind, count) in &run.event_counts {
            *self.aggregate.entry(kind.clone()).or_insert(0) += count;
        }
        self.evidence.push(RunEvidence {
            chart_id: plan.chart_id.clone(),
            difficulty: plan.difficulty.clone(),
            note_digest: run.digest,
            event_digest: run.event_digest,
            event_rows: run.event_rows,
            event_counts: run.event_counts,
        });
    }

    /// A missing run without a recorded failure is still not a pass.
    fn passed(&self) -> bool {
        self.failures.is_empty() && self.evidence.len() == self.planned_runs
    }
}

/// Executes every plan, collecting failures instead of stopping at the first.
///
/// The run and scene-load functions are parameters so the fail-closed
/// aggregation can be exercised without a real 166-run corpus.
fn execute_runs(
    assets_root: &Path,
    trace_dir: &Path,
    plans: &[RunPlan],
    mut run: impl FnMut(&Path, &str, &str, &Path) -> Result<gameplay_sim::CorpusRun, String>,
    mut scene_load: impl FnMut(&Path, &str, &str) -> Result<(), String>,
) -> GateReport {
    let mut report = GateReport {
        planned_runs: plans.len(),
        ..GateReport::default()
    };
    for plan in plans {
        let key = format!("{}/{}", plan.chart_id, plan.difficulty);
        let run_result = run(assets_root, &plan.chart_id, &plan.difficulty, trace_dir);
        // The scene load is attempted even when the sim pass failed so one
        // report names every independent failure for the run.
        let scene_result = scene_load(assets_root, &plan.chart_id, &plan.difficulty);
        match (run_result, scene_result) {
            (Ok(run), Ok(())) => report.record_success(plan, run),
            (run_result, scene_result) => {
                if let Err(error) = run_result {
                    report.failures.push(with_run_key(&key, &error));
                }
                if let Err(error) = scene_result {
                    report.failures.push(with_run_key(&key, &error));
                }
            }
        }
    }
    report
}

/// `corpus_run` already prefixes its errors with the run key; avoid repeating
/// it while still naming errors from other helpers.
fn with_run_key(key: &str, error: &str) -> String {
    match error.strip_prefix(key) {
        Some(rest) if rest.starts_with(':') => error.to_owned(),
        _ => format!("{key}: {error}"),
    }
}

fn scene_load(assets_root: &Path, chart_id: &str, difficulty: &str) -> Result<(), String> {
    SliceRenderer::build_for_song(assets_root, chart_id, difficulty)
        .map(|_renderer| ())
        .map_err(|error| format!("scene inventory load failed: {error}"))
}

fn format_counts(counts: &[(String, usize)]) -> String {
    if counts.is_empty() {
        return "none".to_string();
    }
    counts
        .iter()
        .map(|(kind, count)| format!("{kind}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_aggregate(aggregate: &BTreeMap<String, usize>) -> String {
    if aggregate.is_empty() {
        return "none".to_string();
    }
    aggregate
        .iter()
        .map(|(kind, count)| format!("{kind}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn print_report(report: &GateReport, chart_ids: usize) {
    for evidence in &report.evidence {
        println!(
            "full-chart-gameplay: {}/{}: note sha256 {}; event sha256 {}; event rows {} ({}); \
             scene load ok",
            evidence.chart_id,
            evidence.difficulty,
            evidence.note_digest,
            evidence.event_digest,
            evidence.event_rows,
            format_counts(&evidence.event_counts),
        );
    }
    println!(
        "full-chart-gameplay: aggregate schema-3 chart-event rows across {} passing run(s): {}",
        report.evidence.len(),
        format_aggregate(&report.aggregate),
    );
    for failure in &report.failures {
        eprintln!("full-chart-gameplay: failure: {failure}");
    }
    if report.passed() {
        println!(
            "full-chart-gameplay: PASS: {chart_ids}/{EXPECTED_CHART_IDS} chart-ids, {}/\
             {EXPECTED_RUNS} runs passed the two-run schema-2 note pass (zero miss/game-over), \
             the two-run schema-3 event pass and the scene inventory load; {DEFERRED_PARITY}",
            report.evidence.len(),
        );
    } else {
        println!(
            "full-chart-gameplay: FAIL: {} failure(s); {}/{} runs passed; {DEFERRED_PARITY}",
            report.failures.len(),
            report.evidence.len(),
            EXPECTED_RUNS,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(chart_id: &str, difficulty: &str) -> RunPlan {
        RunPlan {
            chart_id: chart_id.to_string(),
            difficulty: difficulty.to_string(),
        }
    }

    fn corpus_run_ok(chart_id: &str, difficulty: &str) -> gameplay_sim::CorpusRun {
        gameplay_sim::CorpusRun {
            event_count: 7,
            opponent_count: 1,
            tap_count: 6,
            digest: format!("note-{chart_id}-{difficulty}"),
            event_digest: format!("event-{chart_id}-{difficulty}"),
            event_rows: 2,
            event_counts: vec![("ScrollSpeed".to_string(), 2)],
        }
    }

    #[test]
    fn cli_requires_corpus_and_assets_with_the_documented_exit_codes() {
        let cli = Cli::parse(&["--corpus".into(), "--assets".into(), "/tmp/assets".into()])
            .expect("valid invocation");
        assert_eq!(cli.assets_root, PathBuf::from("/tmp/assets"));

        let error = Cli::parse(&["--assets".into(), "/tmp/assets".into()])
            .expect_err("--corpus is required");
        assert!(matches!(error, CliError::Usage(_)));
        assert!(error.to_string().contains("--corpus"), "{error}");
        assert_eq!(error.code(), 2);

        let error = Cli::parse(&["--corpus".into()]).expect_err("--assets is required");
        assert!(matches!(error, CliError::Prerequisite(_)));
        assert!(error.to_string().contains("--assets"), "{error}");
        assert_eq!(error.code(), 1);

        let error = Cli::parse(&["--corpus".into(), "--assets".into()])
            .expect_err("a dangling --assets is a usage error");
        assert!(matches!(error, CliError::Usage(_)));
        assert!(error.to_string().contains("requires a value"), "{error}");
        assert_eq!(error.code(), 2);

        let error = Cli::parse(&[
            "--corpus".into(),
            "--assets".into(),
            "/tmp/assets".into(),
            "--bogus".into(),
        ])
        .expect_err("unknown flags must not parse");
        assert!(matches!(error, CliError::Usage(_)));
        assert!(
            error.to_string().contains("unsupported argument"),
            "{error}"
        );
        assert_eq!(error.code(), 2);
    }

    #[test]
    fn gate_collects_failures_and_keeps_running() {
        let plans = vec![
            plan("ok", "easy"),
            plan("sim-fail", "easy"),
            plan("scene-fail", "easy"),
            plan("ok", "hard"),
        ];
        let run_calls = std::cell::Cell::new(0usize);
        let scene_calls = std::cell::Cell::new(0usize);
        let report = execute_runs(
            Path::new("/synthetic/assets"),
            Path::new("/synthetic/traces"),
            &plans,
            |_, chart_id, difficulty, _| {
                run_calls.set(run_calls.get() + 1);
                if chart_id == "sim-fail" {
                    Err(format!("{chart_id}/{difficulty}: synthetic sim failure"))
                } else {
                    Ok(corpus_run_ok(chart_id, difficulty))
                }
            },
            |_, chart_id, _| {
                scene_calls.set(scene_calls.get() + 1);
                if chart_id == "scene-fail" {
                    Err("synthetic scene load failure".to_string())
                } else {
                    Ok(())
                }
            },
        );

        // A failing run never stops the remaining plans (or its own scene
        // load): both failure paths are collected in one pass.
        assert_eq!(run_calls.get(), plans.len());
        assert_eq!(scene_calls.get(), plans.len());
        assert_eq!(report.planned_runs, 4);
        assert_eq!(report.evidence.len(), 2);
        assert!(!report.passed());
        assert_eq!(report.failures.len(), 2);
        assert!(
            report.failures.iter().any(|failure| {
                failure.contains("sim-fail/easy")
                    && failure.contains("synthetic sim failure")
                    // `corpus_run` already prefixes the key; the gate must not
                    // repeat it.
                    && failure.matches("sim-fail/easy").count() == 1
            }),
            "{:?}",
            report.failures
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("scene-fail/easy")
                    && failure.contains("synthetic scene load failure")),
            "{:?}",
            report.failures
        );
        assert_eq!(report.aggregate.get("ScrollSpeed"), Some(&4));

        let all_ok = execute_runs(
            Path::new("/synthetic/assets"),
            Path::new("/synthetic/traces"),
            &plans[..1],
            |_, chart_id, difficulty, _| Ok(corpus_run_ok(chart_id, difficulty)),
            |_, _, _| Ok(()),
        );
        assert!(all_ok.passed());
        assert_eq!(all_ok.evidence.len(), 1);
    }
}

#[cfg(test)]
mod inventory_tests {
    use super::*;

    fn difficulty(_chart_id: &str) -> Result<Vec<String>, String> {
        Ok(vec!["easy".to_string(), "normal".to_string()])
    }

    /// Count-preserving drift must fail closed: a duplicated chart-id line
    /// keeps the totals identical while a measured id is never simulated.
    #[test]
    fn duplicate_chart_ids_fail_closed() {
        let mut ids = vec!["a"; EXPECTED_CHART_IDS];
        ids[1] = "a";
        let (_plans, failures) = plan_runs_with(ids, difficulty);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("duplicate chart-id")),
            "the duplicate inventory must be rejected: {failures:?}"
        );
    }

    /// A duplicate declared difficulty keeps the run count while a measured
    /// (id, difficulty) pair is never simulated.
    #[test]
    fn duplicate_declared_difficulties_fail_closed() {
        let ids = vec!["a"; EXPECTED_CHART_IDS];
        let (_plans, failures) =
            plan_runs_with(ids, |_| Ok(vec!["easy".to_string(), "easy".to_string()]));
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("duplicate (chart-id, difficulty)")),
            "the duplicate plans must be rejected: {failures:?}"
        );
    }
}
