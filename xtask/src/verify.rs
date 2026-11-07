//! Explicit verification scope. Quick checks never certify a milestone.
//!
//! Cargo runs directly and sequentially under one verify-owned slot, not through
//! the workflow wrapper again. A caller using workflow.py must hold its outer
//! validation slot for this entire invocation, rather than reacquiring it per gate.

use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const VERSION: u32 = 1;
const CONTROL_TIMEOUT: Duration = Duration::from_secs(1);
const REAP_TIMEOUT: Duration = Duration::from_secs(1);
const TERM_GRACE: Duration = Duration::from_millis(250);
const GROUP_DRAIN_TIMEOUT: Duration = Duration::from_secs(1);
const CAPTURE_LIMIT: u64 = 1024 * 1024;
const PROCESS_CLEANUP_LIMITS: &str = "POSIX: each execute command owns a new process group; timeout cleanup sends TERM then KILL before reaping its leader and checks the original group for live members. Non-child zombies must be reaped by their system parent. Windows: bounded taskkill /PID /T /F plus direct-child fallback is best effort, not Job Object containment. Descendants escaping the original group/session or Windows tree are not searched for or killed globally. Normal exits (including nonzero exits) only reap the leader and assume commands join their descendants; no post-reap group signals are sent. Poll errors leave child ownership unconfirmed, so no further PID/group signals are sent and cleanup is marked uncertain. Cleanup utilities must be available. The outer tools/workflow.py layer must also clean up these child-owned groups/trees; signalling only verify's parent group is insufficient. Forced termination of verify itself can bypass this layer.";
const BASELINE: &[&str] = &["fmt", "clippy", "tests"];
const CI_TRUST_BOUNDARY: &str = "Offline structural consistency only: no network lookup, signature verification or provenance authentication. The human/orchestrator must obtain these artifacts from authentic run URLs and verify their provenance before relying on acceptance.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Pass,
    Fail,
    Blocked,
    Skipped,
    NotImplemented,
}

impl Status {
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Blocked => "blocked",
            Self::Skipped => "skipped",
            Self::NotImplemented => "not_implemented",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Profile {
    Quick,
    Milestone,
    Release,
}

#[derive(Clone, Copy)]
enum Action {
    Cargo(&'static [&'static str]),
    Corpus(&'static str),
    AssetTests(&'static str, &'static str),
    Media,
    Conductor,
    ConductorNative,
    ReplayCorpus,
    InputJitter,
    ExternalCi,
    Pending,
}

struct Gate {
    id: &'static str,
    milestone: u8,
    coverage: &'static str,
    action: Action,
    dependencies: &'static [&'static str],
    timeout_secs: u64,
    ignored_tests: &'static str,
}

const fn pending(id: &'static str, milestone: u8, coverage: &'static str) -> Gate {
    Gate {
        id,
        milestone,
        coverage,
        action: Action::Pending,
        dependencies: &[],
        timeout_secs: 0,
        ignored_tests: "not_applicable",
    }
}

// This is an acceptance catalogue, not a discovery mechanism. A workflow file,
// a successful local command, or a skipped/ignored test is not external evidence.
const GATES: &[Gate] = &[
    Gate {
        id: "fmt",
        milestone: 0,
        coverage: "Workspace formatting only; cargo fmt has no --locked option and does not build dependencies.",
        action: Action::Cargo(&["fmt", "--all", "--", "--check"]),
        dependencies: &[],
        timeout_secs: 120,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "clippy",
        milestone: 0,
        coverage: "Workspace/all-targets linting with a locked dependency graph; not runtime acceptance.",
        action: Action::Cargo(&[
            "clippy",
            "--locked",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
        dependencies: &["fmt"],
        timeout_secs: 1200,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "tests",
        milestone: 0,
        coverage: "Portable workspace unit/integration/doc tests, including fixtures and VFS; ignored real-data/atlas/font and external-tool/media tests are NOT executed or counted as passed.",
        action: Action::Cargo(&["test", "--locked", "--workspace"]),
        dependencies: &["fmt", "clippy"],
        timeout_secs: 1200,
        ignored_tests: "excluded_not_counted_as_pass",
    },
    Gate {
        id: "dual_platform_ci",
        milestone: 0,
        coverage: "Linux AND Windows quick-report artifacts consistent with this clean revision/toolchain. Offline structural consistency only; artifact provenance must be verified by the human/orchestrator.",
        action: Action::ExternalCi,
        dependencies: BASELINE,
        timeout_secs: 0,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "parse_all_corpus",
        milestone: 1,
        coverage: "Official data corpus parsing/validation/round-trip, not merely portable fixtures.",
        action: Action::Corpus("parse-all"),
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "real_data",
        milestone: 1,
        coverage: "Real-data round-trip integration tests, explicitly including ignored tests with OXIDE_ASSETS_ROOT from --assets.",
        action: Action::AssetTests("oxide-data", "round_trip"),
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "included_strictly",
    },
    Gate {
        id: "parse_atlas_corpus",
        milestone: 2,
        coverage: "Official atlas corpus; fixture-only atlas success is insufficient.",
        action: Action::Corpus("parse-atlas"),
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "real_atlas_geometry",
        milestone: 2,
        coverage: "Real-atlas frame geometry integration tests, explicitly including ignored tests with OXIDE_ASSETS_ROOT from --assets.",
        action: Action::AssetTests("oxide-atlas", "frame_quad"),
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "included_strictly",
    },
    Gate {
        id: "real_fonts",
        milestone: 2,
        coverage: "Real-font integration tests, explicitly including ignored tests with OXIDE_ASSETS_ROOT from --assets.",
        action: Action::AssetTests("oxide-font", "fonts"),
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "included_strictly",
    },
    Gate {
        id: "media_mini_library",
        milestone: 3,
        coverage: "Strict oxide-media mini media integration tests; missing external tools must fail, not silently skip.",
        action: Action::Cargo(&[
            "test",
            "--locked",
            "-p",
            "oxide-media",
            "--test",
            "media_mini",
            "--",
            "--include-ignored",
        ]),
        dependencies: BASELINE,
        timeout_secs: 900,
        ignored_tests: "included_strictly",
    },
    Gate {
        id: "media_mini_xtask",
        milestone: 3,
        coverage: "Strict xtask mini media integration tests, explicitly including ignored external-tool tests.",
        action: Action::Cargo(&[
            "test",
            "--locked",
            "-p",
            "xtask",
            "--test",
            "media_mini",
            "--",
            "--include-ignored",
        ]),
        dependencies: BASELINE,
        timeout_secs: 900,
        ignored_tests: "included_strictly",
    },
    Gate {
        id: "media_synthetic_delay",
        milestone: 3,
        coverage: "Strict synthetic-delay cross-correlation unit test, explicitly including the ignored external-tool test.",
        action: Action::Cargo(&[
            "test",
            "--locked",
            "-p",
            "oxide-media",
            "--lib",
            "cross_correlation_recovers_synthetic_delay",
            "--",
            "--include-ignored",
        ]),
        dependencies: BASELINE,
        timeout_secs: 900,
        ignored_tests: "included_strictly",
    },
    Gate {
        id: "media_corpus",
        milestone: 3,
        coverage: "Official corpus media alignment, fidelity and size gates; not mini-fixture acceptance.",
        action: Action::Media,
        dependencies: &[
            "fmt",
            "clippy",
            "tests",
            "media_mini_library",
            "media_mini_xtask",
            "media_synthetic_delay",
        ],
        timeout_secs: 1800,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "conductor",
        milestone: 4,
        coverage: "Committed conductor fixture comparison only; fixture/model agreement is not independent real-game parity or replay/input-jitter acceptance.",
        action: Action::Conductor,
        dependencies: BASELINE,
        timeout_secs: 120,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "conductor_native_reference",
        milestone: 4,
        coverage: "Frozen native hxcpp `funkin.Conductor` rows (real execution of the installed game source over the frozen recipe, provenance/digests fail-closed; see tests/fixtures/conductor-native/_manifest_native.json) replayed against the Rust conductor and the model fixtures; NOT a full-game integration trace and not a substitute for input/replay/jitter platform evidence.",
        action: Action::ConductorNative,
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "replay",
        milestone: 4,
        coverage: "Deterministic replay for each committed chart: report sha256 equality across two evaluation passes with `input source: synthesized from chart (no human replay yet)`; score/windows are placeholders measured at M5; no gameplay/note parity or human replay claim.",
        action: Action::ReplayCorpus,
        dependencies: BASELINE,
        timeout_secs: 600,
        ignored_tests: "not_applicable",
    },
    Gate {
        id: "input_jitter",
        milestone: 4,
        coverage: "Linux uinput delivery-jitter harness p50/p99 against the 2 ms guideline with the documented kernel-timestamp mitigation; NOT real audio-clock correlation, NOT Windows Raw Input or hardware measurement, so platform-specific evidence remains pending.",
        action: Action::InputJitter,
        dependencies: BASELINE,
        timeout_secs: 300,
        ignored_tests: "not_applicable",
    },
    pending("render_diff", 5, "Vertical-slice render-diff acceptance."),
    pending(
        "perf_budget",
        5,
        "Vertical-slice performance-budget acceptance.",
    ),
    pending(
        "gameplay_sim_bopeebo",
        5,
        "Deterministic bopeebo-slice gameplay trace per docs/M5-CONTRACT.md (schema '1', sha256 two-run stability); not a native-game parity claim.",
    ),
    pending(
        "native_note_reference",
        5,
        "Native PlayState note-judge trace (human ADR-0014-style approval required before any implementation); M5 acceptance stays blocked while pending.",
    ),
    pending("full_chart_gameplay", 6, "Full-chart gameplay acceptance."),
    pending("ui_save_e2e", 7, "UI and save end-to-end acceptance."),
    pending(
        "ruby_mods",
        8,
        "Ruby scripting and mod integration acceptance.",
    ),
    pending(
        "release_binaries",
        9,
        "Release binaries for every supported platform.",
    ),
    pending(
        "release_size",
        9,
        "Total release binary plus assets size budget.",
    ),
    pending(
        "reproducibility",
        9,
        "Independent release reproducibility evidence.",
    ),
    pending(
        "fuzz_parsers",
        9,
        "Parser fuzz acceptance before final release.",
    ),
];

struct Options {
    profile_name: String,
    profile: Option<Profile>,
    milestone: Option<u8>,
    assets: Option<PathBuf>,
    ci_evidence: Option<PathBuf>,
    out: PathBuf,
    fixtures: PathBuf,
    report: PathBuf,
    help: bool,
    errors: Vec<String>,
}

impl Options {
    fn parse(args: &[String], root: &Path) -> Self {
        let mut options = Self {
            profile_name: "quick".into(),
            profile: Some(Profile::Quick),
            milestone: None,
            assets: None,
            ci_evidence: None,
            out: root.join("target/verify-media"),
            fixtures: root.join("tests/fixtures/conductor"),
            report: root.join("target/verify.json"),
            help: false,
            errors: Vec::new(),
        };
        let mut seen = Vec::new();
        let mut index = 0;
        while let Some(argument) = args.get(index) {
            index += 1;
            if argument == "--help" || argument == "-h" {
                options.help = true;
                continue;
            }
            if !matches!(
                argument.as_str(),
                "--profile"
                    | "--milestone"
                    | "--assets"
                    | "--ci-evidence"
                    | "--out"
                    | "--fixtures"
                    | "--report"
            ) {
                options.errors.push(format!("unknown option: {argument}"));
                continue;
            }
            if seen.contains(&argument.as_str()) {
                options.errors.push(format!("duplicate option: {argument}"));
            }
            seen.push(argument.as_str());
            let Some(value) = args.get(index).filter(|value| !value.starts_with("--")) else {
                options.errors.push(format!("{argument} requires a value"));
                continue;
            };
            index += 1;
            if value.is_empty() {
                options
                    .errors
                    .push(format!("{argument} requires a nonempty value"));
                continue;
            }
            match argument.as_str() {
                "--profile" => {
                    options.profile_name = value.clone();
                    options.profile = match value.as_str() {
                        "quick" => Some(Profile::Quick),
                        "milestone" => Some(Profile::Milestone),
                        "release" => Some(Profile::Release),
                        _ => {
                            options.errors.push(format!("invalid profile: {value}"));
                            None
                        }
                    };
                }
                "--milestone" => {
                    options.milestone = match value.as_bytes() {
                        [b'M', digit @ b'0'..=b'9'] => Some(*digit - b'0'),
                        _ => {
                            options
                                .errors
                                .push(format!("invalid milestone: {value}; expected M0..M9"));
                            None
                        }
                    };
                }
                "--assets" => options.assets = Some(PathBuf::from(value)),
                "--ci-evidence" => options.ci_evidence = Some(PathBuf::from(value)),
                "--out" => options.out = PathBuf::from(value),
                "--fixtures" => options.fixtures = PathBuf::from(value),
                "--report" => options.report = PathBuf::from(value),
                _ => unreachable!(),
            }
        }
        if options.profile == Some(Profile::Milestone) && options.milestone.is_none() {
            options
                .errors
                .push("--profile milestone requires --milestone M0..M9".into());
        }
        if seen.contains(&"--milestone") && options.profile != Some(Profile::Milestone) {
            options.errors.push(
                "--milestone is only valid with --profile milestone; release targets M9".into(),
            );
        }
        options
    }

    fn target(&self) -> Option<u8> {
        match self.profile {
            Some(Profile::Milestone) => self.milestone,
            Some(Profile::Release) => Some(9),
            _ => None,
        }
    }

    fn required(&self, gate: &Gate) -> bool {
        match self.profile {
            Some(Profile::Quick) => BASELINE.contains(&gate.id),
            Some(Profile::Milestone | Profile::Release) => {
                self.target().is_some_and(|target| gate.milestone <= target)
            }
            None => false,
        }
    }
}

fn help() {
    println!(
        "usage: cargo xtask verify [--profile quick|milestone|release] [options]\n\
         quick (default): fmt, clippy and portable tests; NOT milestone acceptance.\n\
         milestone: cumulative acceptance; requires --milestone M0..M9.\n\
         release: all obligations through M9; pending gates stay nonzero.\n\
         --assets DIR    explicit official corpus; no implicit home-directory assets\n\
         --ci-evidence DIR Linux/Windows quick artifacts: linux.json and windows.json\n\
         --out DIR       media output (default: target/verify-media)\n\
         --fixtures DIR conductor fixtures (default: tests/fixtures/conductor)\n\
         --report PATH   JSON report (default: target/verify.json)\n\
         --help, -h      show help without running gates or writing a report\n\
         User paths are relative to the caller's working directory.\n\
         Cargo clippy/tests use --locked; fmt has no --locked option.\n\
         Dual-platform CI remains BLOCKED without consistent --ci-evidence reports.\n\
         CI evidence checks offline structural consistency only, NOT authenticity.\n\
         Obtain artifacts from authentic run URLs and verify provenance externally.\n\
         Exit codes: 0 = selected checks passed (quick is never acceptance),\n\
         1 = command/metadata/report failure, 2 = blocked/not implemented/invalid options."
    );
}

fn now_ms() -> Option<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|time| time.as_millis())
}

struct Outcome {
    started_unix_ms: Option<u128>,
    duration_ms: u128,
    exit_code: Option<i32>,
    success: bool,
    timed_out: bool,
    error: Option<String>,
    process_id: Option<u32>,
    cleanup: Option<Value>,
    cleanup_uncertain: bool,
}

impl Outcome {
    fn json(&self) -> Value {
        json!({
            "started_unix_ms": self.started_unix_ms,
            "duration_ms": self.duration_ms,
            "exit_code": self.exit_code,
            "success": self.success,
            "timed_out": self.timed_out,
            "error": self.error,
            "process_id": self.process_id,
            "owned_process_group": if cfg!(unix) { self.process_id } else { None },
            "cleanup": self.cleanup,
            "cleanup_uncertain": self.cleanup_uncertain,
        })
    }
}

// Polling reaps an exited child. Do not poll the group leader during TERM grace:
// finish group signalling first, while the unreaped leader pins its PID/PGID.
fn poll(child: &mut Child, timeout: Duration) -> Result<Option<std::process::ExitStatus>, String> {
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(25)),
            Ok(None) => return Ok(None),
            Err(error) => return Err(format!("could not poll child: {error}")),
        }
    }
}

// Cleanup utilities do not call execute/capture, avoiding recursive cleanup.
// Their regular-file output and fallback reaping are independently bounded.
fn control(command: &mut Command) -> Value {
    let start = Instant::now();
    let specification = command_json(command);
    let result = (|| -> Result<Value, String> {
        let scratch = Scratch::create()?;
        let stdout_path = scratch.0.join("stdout");
        let stderr_path = scratch.0.join("stderr");
        command
            .stdin(Stdio::null())
            .stdout(File::create(&stdout_path).map_err(|error| error.to_string())?)
            .stderr(File::create(&stderr_path).map_err(|error| error.to_string())?);
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        let (status, error) = match poll(&mut child, CONTROL_TIMEOUT) {
            Ok(Some(status)) => (Some(status), None),
            Ok(None) => {
                let killed = child.kill();
                let reaped = poll(&mut child, REAP_TIMEOUT);
                let error = format!("cleanup utility timeout; kill={killed:?}; reap={reaped:?}");
                (reaped.ok().flatten(), Some(error))
            }
            Err(error) => (
                None,
                Some(format!(
                    "cleanup utility ownership unconfirmed after poll error; no PID signal sent: {error}"
                )),
            ),
        };
        Ok(json!({
            "success": error.is_none() && status.as_ref().is_some_and(|status| status.success()),
            "exit_code": status.and_then(|status| status.code()),
            "process_id": child.id(),
            "leader_reaped": status.is_some(),
            "error": error,
            "stdout": read_capture(&stdout_path)?,
            "stderr": read_capture(&stderr_path)?,
        }))
    })();
    let mut record = result.unwrap_or_else(|error| json!({"success": false, "error": error}));
    record["command"] = specification;
    record["duration_ms"] = json!(start.elapsed().as_millis());
    record
}

#[cfg(unix)]
fn owned_group_target(pid: u32) -> Option<String> {
    // Never accept kill's special targets 0/-1, our PID, or an invalid pid_t.
    (pid > 1 && pid != std::process::id() && pid <= i32::MAX as u32).then(|| format!("-{pid}"))
}

#[cfg(unix)]
fn group_members(stdout: &str, group: u32) -> Result<(usize, usize), String> {
    if stdout.trim().is_empty() {
        return Err("empty ps snapshot cannot confirm group cleanup".into());
    }
    let (mut live, mut zombies) = (0, 0);
    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let mut fields = line.split_whitespace();
        let id = fields
            .next()
            .and_then(|id| id.parse::<u32>().ok())
            .ok_or_else(|| "cannot parse ps process-group column".to_string())?;
        let state = fields
            .next()
            .ok_or_else(|| "cannot parse ps state column".to_string())?;
        if id == group {
            if state.starts_with('Z') {
                zombies += 1;
            } else {
                live += 1;
            }
        }
    }
    Ok((live, zombies))
}

#[cfg(unix)]
fn drain_group(group: u32) -> (bool, Value) {
    let start = Instant::now();
    loop {
        let mut command = Command::new("ps");
        command.args(["-e", "-o", "pgid=", "-o", "stat="]);
        let mut observation = control(&mut command);
        let members = if observation["success"] == true {
            group_members(observation["stdout"].as_str().unwrap_or(""), group)
        } else {
            Err("cannot inspect original process group after KILL".into())
        };
        // Do not include unrelated process groups in the verification report.
        observation["stdout"] = Value::Null;
        match members {
            Ok((live, zombies)) => {
                observation["live_members"] = json!(live);
                observation["non_child_zombies"] = json!(zombies);
                if live == 0 {
                    return (true, observation);
                }
            }
            Err(error) => {
                observation["error"] = json!(error);
                return (false, observation);
            }
        }
        if start.elapsed() >= GROUP_DRAIN_TIMEOUT {
            return (false, observation);
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn cleanup(child: &mut Child) -> (Option<i32>, bool, Value) {
    let start = Instant::now();
    let pid = child.id();
    let mut actions: Vec<Value> = Vec::new();
    #[cfg(unix)]
    let terminated = if let Some(target) = owned_group_target(pid) {
        for signal in ["TERM", "KILL"] {
            let mut command = Command::new("kill");
            command.args(["-s", signal, "--", &target]);
            actions.push(control(&mut command));
            if signal == "TERM" {
                thread::sleep(TERM_GRACE);
            }
        }
        actions
            .last()
            .is_some_and(|action| action["success"] == true)
    } else {
        actions.push(json!({"success": false, "error": "refusing unsafe process-group target"}));
        false
    };
    #[cfg(windows)]
    let terminated = {
        let mut command = Command::new("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        let action = control(&mut command);
        let success = action["success"] == true;
        actions.push(action);
        success
    };
    #[cfg(not(any(unix, windows)))]
    let terminated = false;

    // No try_wait/wait before the final group signal: even if TERM exits the
    // leader, an unreaped leader pins the PID/PGID while stubborn children live.
    let killed = child.kill();
    let reaped = poll(child, REAP_TIMEOUT);
    let leader_reaped = matches!(reaped, Ok(Some(_)));
    let exit_code = reaped
        .as_ref()
        .ok()
        .and_then(|status| status.as_ref())
        .and_then(|status| status.code());
    #[cfg(unix)]
    let (quiescent, observation) = if terminated && leader_reaped {
        drain_group(pid)
    } else {
        (false, Value::Null)
    };
    #[cfg(unix)]
    if !observation.is_null() {
        actions.push(observation);
    }
    #[cfg(not(unix))]
    let quiescent = terminated && leader_reaped;
    let utility_errors = actions
        .iter()
        .any(|action| action.get("error").is_some_and(|error| !error.is_null()));
    let uncertain = !terminated || !leader_reaped || !quiescent || utility_errors;
    (
        exit_code,
        uncertain,
        json!({
            "process_id": pid,
            "method": if cfg!(unix) { "owned_process_group_term_kill" } else if cfg!(windows) { "taskkill_tree_best_effort" } else { "direct_child_only_unsupported_platform" },
            "actions": actions,
            "direct_child_kill": format!("{killed:?}"),
            "leader_reaped": leader_reaped,
            "reap_error": reaped.err(),
            "original_group_quiescent": if cfg!(unix) { Some(quiescent) } else { None },
            "uncertain": uncertain,
            "duration_ms": start.elapsed().as_millis(),
            "limits": PROCESS_CLEANUP_LIMITS,
        }),
    )
}

fn execute(command: &mut Command, timeout_secs: u64) -> Outcome {
    execute_with_timeout(command, Duration::from_secs(timeout_secs))
}

fn execute_with_timeout(command: &mut Command, timeout: Duration) -> Outcome {
    let start = Instant::now();
    let mut outcome = Outcome {
        started_unix_ms: now_ms(),
        duration_ms: 0,
        exit_code: None,
        success: false,
        timed_out: false,
        error: None,
        process_id: None,
        cleanup: None,
        cleanup_uncertain: false,
    };
    command.stdin(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    match command.spawn() {
        Ok(mut child) => {
            outcome.process_id = Some(child.id());
            match poll(&mut child, timeout) {
                Ok(Some(status)) => {
                    outcome.exit_code = status.code();
                    outcome.success = status.success();
                }
                Ok(None) => {
                    outcome.timed_out = true;
                    outcome.error = Some(format!("timeout after {}ms", timeout.as_millis()));
                    let (exit_code, uncertain, record) = cleanup(&mut child);
                    outcome.exit_code = exit_code;
                    outcome.cleanup_uncertain = uncertain;
                    outcome.cleanup = Some(record);
                }
                Err(error) => {
                    // A poll error may mean another reaper took the child. Do
                    // not signal a PID/PGID whose ownership is now unconfirmed.
                    outcome.error = Some(format!(
                        "child polling failed; no PID/group signals sent: {error}"
                    ));
                    outcome.cleanup_uncertain = true;
                    outcome.cleanup = Some(json!({
                        "process_id": child.id(),
                        "method": "none_unconfirmed_child_ownership",
                        "uncertain": true,
                        "error": error,
                        "limits": PROCESS_CLEANUP_LIMITS,
                    }));
                }
            }
        }
        Err(error) => outcome.error = Some(format!("could not launch command: {error}")),
    }
    outcome.duration_ms = start.elapsed().as_millis();
    outcome
}

fn command_json(command: &Command) -> Value {
    json!({
        "program": command.get_program().to_string_lossy(),
        "args": command.get_args().map(|arg| arg.to_string_lossy()).collect::<Vec<_>>(),
        "cwd": command.get_current_dir(),
        "env": command.get_envs().map(|(key, value)| (
            key.to_string_lossy().into_owned(),
            value.map(|value| value.to_string_lossy().into_owned()),
        )).collect::<std::collections::BTreeMap<_, _>>(),
    })
}

fn read_capture(path: &Path) -> Result<String, String> {
    let mut output = String::new();
    File::open(path)
        .map_err(|error| error.to_string())?
        .take(CAPTURE_LIMIT + 1)
        .read_to_string(&mut output)
        .map_err(|error| error.to_string())?;
    if output.len() as u64 > CAPTURE_LIMIT {
        return Err("metadata/control output exceeds capture limit".into());
    }
    Ok(output)
}

fn cleanup_policy() -> Value {
    json!({
        "limits": PROCESS_CLEANUP_LIMITS,
        "utility_timeout_ms": CONTROL_TIMEOUT.as_millis(),
        "reap_timeout_ms": REAP_TIMEOUT.as_millis(),
        "posix_term_grace_ms": TERM_GRACE.as_millis(),
        "posix_group_drain_timeout_ms": GROUP_DRAIN_TIMEOUT.as_millis(),
        "cleanup_before_slot_release": true,
        "retain_slot_on_cleanup_error": true,
        "normal_exit": "leader reaped only; command must join its own descendants",
    })
}

// Files, rather than pipes, prevent a full metadata pipe or an inherited pipe in
// a grandchild from defeating the timeout. Exclusive creation never reuses logs.
struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        for index in 0..100 {
            let path =
                std::env::temp_dir().join(format!("oxide-verify-{}-{index}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => {
                    return Err(format!("cannot create metadata scratch directory: {error}"));
                }
            }
        }
        Err("cannot allocate fresh metadata scratch directory".into())
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn capture(command: &mut Command) -> Result<(String, Value), String> {
    capture_with_timeout(command, Duration::from_secs(10))
}

fn capture_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<(String, Value), String> {
    let scratch = Scratch::create()?;
    let stdout_path = scratch.0.join("stdout");
    let stderr_path = scratch.0.join("stderr");
    let stdout = File::create(&stdout_path).map_err(|error| error.to_string())?;
    let stderr = File::create(&stderr_path).map_err(|error| error.to_string())?;
    let specification = command_json(command);
    command.stdout(stdout).stderr(stderr);
    let outcome = execute_with_timeout(command, timeout);
    let stdout = read_capture(&stdout_path);
    let stderr = read_capture(&stderr_path);
    let capture_errors: Vec<_> = [stdout.as_ref().err(), stderr.as_ref().err()]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    let record = json!({
        "command": specification,
        "status": if outcome.success && capture_errors.is_empty() { "pass" } else { "fail" },
        "outcome": outcome.json(),
        "stdout": stdout.as_ref().ok(),
        "stderr": stderr.as_ref().ok(),
        "capture_errors": capture_errors,
    });
    Ok((stdout.unwrap_or_default(), record))
}

fn ci_metadata(env: impl Fn(&str) -> Option<String>) -> Value {
    if env("GITHUB_ACTIONS").as_deref() != Some("true") {
        return Value::Null;
    }
    json!({
        "provider": "github_actions",
        "repository": env("GITHUB_REPOSITORY"),
        "run_id": env("GITHUB_RUN_ID"),
        "run_attempt": env("GITHUB_RUN_ATTEMPT"),
        "job": env("GITHUB_JOB"),
        "server_url": env("GITHUB_SERVER_URL"),
        "sha": env("GITHUB_SHA"),
    })
}

fn valid_head(head: &str) -> bool {
    matches!(head.len(), 40 | 64) && head.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn ci_identity_errors(ci: &Value, head: &str) -> Vec<String> {
    let mut errors = Vec::new();
    if ci["provider"] != "github_actions" {
        errors.push("CI provider must be github_actions".into());
    }
    for field in [
        "repository",
        "run_id",
        "run_attempt",
        "job",
        "server_url",
        "sha",
    ] {
        if !ci[field].as_str().is_some_and(|value| {
            !value.is_empty() && value.trim() == value && !value.chars().any(char::is_control)
        }) {
            errors.push(format!("CI identity requires nonempty {field}"));
        }
    }
    for field in ["run_id", "run_attempt"] {
        if !ci[field]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
            .is_some_and(|value| value > 0)
        {
            errors.push(format!("CI {field} must be a positive decimal string"));
        }
    }
    if !ci["repository"].as_str().is_some_and(|repository| {
        repository.split_once('/').is_some_and(|(owner, name)| {
            [owner, name].iter().all(|part| {
                !part.is_empty()
                    && !matches!(*part, "." | "..")
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            })
        })
    }) {
        errors.push("CI repository must identify owner/repository".into());
    }
    if !ci["server_url"]
        .as_str()
        .and_then(|url| url.strip_prefix("https://"))
        .is_some_and(|authority| {
            let (host, port) = authority
                .split_once(':')
                .map_or((authority, None), |(host, port)| (host, Some(port)));
            !host.is_empty()
                && host
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"-.".contains(&byte))
                && port.is_none_or(|port| port.parse::<u16>().is_ok_and(|port| port > 0))
        })
    {
        errors.push("CI server_url must be an HTTPS server origin".into());
    }
    if ci["sha"].as_str() != Some(head) {
        errors.push("CI sha must equal local Git HEAD".into());
    }
    errors
}

// This deliberately cannot distinguish a fabricated, structurally consistent
// receipt from an authentic artifact. Provenance is an explicit caller obligation.
fn validate_ci_reports(local: &Value, reports: &[Option<Value>; 2]) -> Vec<String> {
    let mut errors = Vec::new();
    let head = local["git"]["head"].as_str().unwrap_or("");
    let toolchain = local["toolchain"].as_str().unwrap_or("");
    if !valid_head(head) || !toolchain.starts_with("rustc ") {
        errors.push("local HEAD and exact rustc version are required for CI evidence".into());
    }
    if local["git"]["dirty"] != false || local["git"]["status_porcelain"] != "" {
        errors.push("milestone/release acceptance requires a clean local Git tree".into());
    }
    let mut identity = local.get("ci").filter(|ci| !ci.is_null()).cloned();
    if let Some(ci) = &identity {
        errors.extend(
            ci_identity_errors(ci, head)
                .into_iter()
                .map(|error| format!("local: {error}")),
        );
    }
    for (os, report) in ["linux", "windows"].into_iter().zip(reports) {
        let Some(report) = report else {
            errors.push(format!("{os}.json quick report is missing or unreadable"));
            continue;
        };
        if report["version"] != VERSION
            || report["profile"] != "quick"
            || report["status"] != "pass"
            || report["exit_code"] != 0
            || report["milestone_acceptance"] != false
            || report.get("target_milestone") != Some(&Value::Null)
            || !report["prerequisite_errors"]
                .as_array()
                .is_some_and(Vec::is_empty)
        {
            errors.push(format!("{os}.json must be a version {VERSION}, successful quick report, not milestone acceptance"));
        }
        let metadata = &report["metadata"];
        if metadata["os"] != os {
            errors.push(format!("{os}.json OS must be {os}"));
        }
        if metadata["git"]["head"].as_str() != Some(head) {
            errors.push(format!("{os}.json HEAD does not match local HEAD"));
        }
        if metadata["git"]["dirty"] != false || metadata["git"]["status_porcelain"] != "" {
            errors.push(format!("{os}.json requires a clean Git tree"));
        }
        // Compare the captured rustc version, never a moving channel name.
        if metadata["toolchain"].as_str() != Some(toolchain) {
            errors.push(format!(
                "{os}.json must use the identical pinned rustc version as local validation"
            ));
        }
        let ci = &metadata["ci"];
        errors.extend(
            ci_identity_errors(ci, head)
                .into_iter()
                .map(|error| format!("{os}.json: {error}")),
        );
        if let Some(expected) = &identity {
            for field in ["repository", "server_url"] {
                if ci[field] != expected[field] {
                    errors.push(format!("{os}.json CI {field} must be consistent across receipts and local CI identity"));
                }
            }
        } else {
            identity = Some(ci.clone());
        }
        for id in BASELINE {
            let expected_args = GATES.iter().find_map(|gate| match gate.action {
                Action::Cargo(args) if gate.id == *id => Some(args),
                _ => None,
            });
            let gates: Vec<_> = report["gates"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|gate| gate["id"] == *id)
                .collect();
            if gates.len() != 1
                || !gates.first().is_some_and(|gate| {
                    gate["required"] == true
                        && gate["executed"] == true
                        && gate["status"] == "pass"
                        && gate["outcome"]["success"] == true
                        && gate["outcome"]["exit_code"] == 0
                        && gate["outcome"]["timed_out"] == false
                        && gate["outcome"]["error"].is_null()
                        && gate["command"]["program"].as_str().is_some_and(|program| {
                            matches!(
                                program.rsplit(['/', '\\']).next(),
                                Some("cargo" | "cargo.exe")
                            )
                        })
                        && expected_args.is_some()
                        && gate["command"]["args"] == json!(expected_args)
                })
            {
                errors.push(format!("{os}.json {id} must be exactly one executed, passing required gate with successful exit 0"));
            }
        }
    }
    errors
}

fn ci_evidence(directory: Option<&Path>, local: &Value) -> (Vec<String>, Value) {
    let mut reports = [None, None];
    let mut files = Vec::new();
    let mut errors = Vec::new();
    if let Some(directory) = directory {
        for (index, os) in ["linux", "windows"].into_iter().enumerate() {
            let path = directory.join(format!("{os}.json"));
            let read = fs::read(&path)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    serde_json::from_slice::<Value>(&bytes).map_err(|error| error.to_string())
                });
            match read {
                Ok(report) => {
                    files.push(json!({"path": path, "report": report}));
                    reports[index] = Some(report);
                }
                Err(error) => {
                    errors.push(format!(
                        "cannot read CI evidence {}: {error}",
                        path.display()
                    ));
                    files.push(json!({"path": path, "error": error}));
                }
            }
        }
    } else {
        errors.push("--ci-evidence DIR is required for dual-platform acceptance".into());
    }
    errors.extend(validate_ci_reports(local, &reports));
    let structurally_consistent = errors.is_empty();
    (
        errors,
        json!({
            "validation": "offline_structural_consistency_only",
            "structurally_consistent": structurally_consistent,
            "provenance_verified": false,
            "trust_boundary": CI_TRUST_BOUNDARY,
            "files": files,
        }),
    )
}

fn metadata(root: &Path) -> (Value, Vec<String>) {
    let mut errors = Vec::new();
    let mut records = Vec::new();
    let mut outputs = Vec::new();
    for (program, args) in [
        (
            "git",
            &[
                "--no-pager",
                "--no-optional-locks",
                "rev-parse",
                "--verify",
                "HEAD",
            ][..],
        ),
        (
            "git",
            &[
                "--no-pager",
                "--no-optional-locks",
                "status",
                "--porcelain=v1",
                "--untracked-files=normal",
            ][..],
        ),
        ("rustc", &["--version"][..]),
    ] {
        let mut command = Command::new(program);
        command.args(args).current_dir(root);
        match capture(&mut command) {
            Ok((stdout, record)) => {
                if record["status"] != "pass" {
                    errors.push(format!(
                        "essential metadata command failed: {program} {args:?}"
                    ));
                }
                outputs.push(Some(stdout));
                records.push(record);
            }
            Err(error) => {
                errors.push(format!(
                    "cannot capture essential metadata ({program} {args:?}): {error}"
                ));
                outputs.push(None);
                records.push(
                    json!({"command": command_json(&command), "status": "fail", "error": error}),
                );
            }
        }
    }
    let head = outputs[0].as_deref().map(str::trim);
    if !head.is_some_and(valid_head) {
        errors.push("essential git HEAD is missing or invalid".into());
    }
    let toolchain = outputs[2].as_deref().map(str::trim);
    if !toolchain.is_some_and(|version| version.starts_with("rustc ")) {
        errors.push("essential rustc version is missing or invalid".into());
    }
    (
        json!({
            "git": {
                "head": head,
                "dirty": outputs[1].as_ref().map(|status| !status.is_empty()),
                "status_porcelain": outputs[1],
                "scope": "fresh working-tree snapshot, not a claim of a clean tree or CI execution",
            },
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "toolchain": toolchain,
            "ci": ci_metadata(|key| std::env::var(key).ok()),
            "commands": records,
        }),
        errors,
    )
}

struct ValidationSlot {
    path: PathBuf,
    retain: bool,
}

impl ValidationSlot {
    fn acquire(root: &Path) -> Result<Self, (bool, String)> {
        let path = root.join("target/verify-validation.lock");
        fs::create_dir_all(root.join("target")).map_err(|error| (true, error.to_string()))?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&path).map_err(|error| {
            let failed = error.kind() != std::io::ErrorKind::AlreadyExists;
            (failed, format!("cannot acquire verify validation slot {}: {error}; an existing slot may be active or stale and is never removed automatically", path.display()))
        })?;
        let slot = Self {
            path,
            retain: false,
        };
        writeln!(file, "{}", std::process::id()).map_err(|error| (true, error.to_string()))?;
        Ok(slot)
    }
}

impl Drop for ValidationSlot {
    fn drop(&mut self) {
        if !self.retain {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn gate_command(gate: &Gate, options: &Options, root: &Path, executable: &Path) -> Option<Command> {
    let mut command = match gate.action {
        Action::Cargo(args) => {
            let mut command = Command::new(env!("CARGO"));
            command.args(args);
            command
        }
        Action::Corpus(subcommand) => {
            let mut command = Command::new(executable);
            command.args([subcommand, "--corpus", "--dir"]);
            if let Some(assets) = &options.assets {
                command.arg(assets);
            }
            command
        }
        Action::AssetTests(package, test) => {
            let mut command = Command::new(env!("CARGO"));
            command.args([
                "test",
                "--locked",
                "-p",
                package,
                "--test",
                test,
                "--",
                "--include-ignored",
            ]);
            if let Some(assets) = &options.assets {
                command.env("OXIDE_ASSETS_ROOT", assets);
            }
            command
        }
        Action::Media => {
            let mut command = Command::new(executable);
            command.args(["media-check", "--corpus", "--in"]);
            if let Some(assets) = &options.assets {
                command.arg(assets);
            }
            command.arg("--out").arg(&options.out);
            command
        }
        Action::Conductor => {
            let mut command = Command::new(executable);
            command
                .arg("fixtures-conductor")
                .arg("--fixtures")
                .arg(&options.fixtures);
            command
        }
        Action::ConductorNative => {
            let mut command = Command::new(executable);
            command.arg("conductor-native");
            command
        }
        Action::ReplayCorpus => {
            let mut command = Command::new(executable);
            command.args(["replay", "--corpus"]);
            if let Some(assets) = &options.assets {
                command.arg("--assets").arg(assets);
            }
            command
        }
        Action::InputJitter => {
            let mut command = Command::new(executable);
            command.arg("input-jitter");
            command
        }
        Action::ExternalCi | Action::Pending => return None,
    };
    command.current_dir(root);
    Some(command)
}

struct GateRecord {
    gate: &'static Gate,
    required: bool,
    status: Status,
    errors: Vec<String>,
    command: Option<Value>,
    outcome: Option<Outcome>,
    evidence: Option<Value>,
}

impl GateRecord {
    fn json(&self) -> Value {
        json!({
            "id": self.gate.id,
            "milestone": format!("M{}", self.gate.milestone),
            "required": self.required,
            "applicability": if self.required { "required" } else { "optional_not_selected" },
            "status": self.status.label(),
            "coverage": self.gate.coverage,
            "ignored_test_applicability": self.gate.ignored_tests,
            "dependencies": self.gate.dependencies,
            "prerequisite_errors": self.errors,
            "command": self.command,
            "timeout_secs": self.gate.timeout_secs,
            "executed": self.outcome.is_some(),
            "outcome": self.outcome.as_ref().map(Outcome::json),
            "evidence": self.evidence,
        })
    }
}

fn overall(
    statuses: impl Iterator<Item = (bool, Status)>,
    failed: bool,
    blocked: bool,
) -> (Status, u8) {
    let statuses: Vec<_> = statuses
        .filter_map(|(required, status)| required.then_some(status))
        .collect();
    if failed || statuses.contains(&Status::Fail) {
        (Status::Fail, 1)
    } else if blocked
        || statuses.is_empty()
        || statuses.iter().any(|status| *status != Status::Pass)
    {
        (Status::Blocked, 2)
    } else {
        (Status::Pass, 0)
    }
}

fn write_report(path: &Path, report: &Value) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let mut file = File::create(path).map_err(|error| error.to_string())?;
    serde_json::to_writer_pretty(&mut file, report).map_err(|error| error.to_string())?;
    file.write_all(b"\n")
        .and_then(|()| file.sync_all())
        .map_err(|error| error.to_string())
}

fn is_acceptance(profile: Option<Profile>, status: Status) -> bool {
    status == Status::Pass && matches!(profile, Some(Profile::Milestone | Profile::Release))
}

/// Parent dispatch: `"verify" => verify::run(rest)`; this function owns exit codes.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let start = Instant::now();
    let started_unix_ms = now_ms();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut options = Options::parse(args, &root);
    if options.help && options.errors.is_empty() {
        help();
        return ExitCode::SUCCESS;
    }
    // Resolve explicit paths before child commands change their working directory.
    match std::env::current_dir() {
        Ok(cwd) => {
            options.assets = options.assets.map(|path| cwd.join(path));
            options.ci_evidence = options.ci_evidence.map(|path| cwd.join(path));
            options.out = cwd.join(options.out);
            options.fixtures = cwd.join(options.fixtures);
            options.report = cwd.join(options.report);
        }
        Err(error) => options
            .errors
            .push(format!("cannot resolve caller working directory: {error}")),
    }
    println!("verify: profile {}", options.profile_name);
    if options.profile == Some(Profile::Quick) {
        println!("QUICK CHECK ONLY — NOT milestone acceptance (even when exit code is 0).");
    }
    // Invalidate any previous report before metadata or expensive work begins.
    let initial = json!({
        "version": VERSION,
        "profile": options.profile_name,
        "target_milestone": options.target().map(|target| format!("M{target}")),
        "status": "blocked",
        "milestone_acceptance": false,
        "started_unix_ms": started_unix_ms,
        "prerequisite_errors": ["verification is incomplete; this report is not acceptance evidence"],
        "gates": [],
    });
    if let Err(error) = write_report(&options.report, &initial) {
        eprintln!(
            "verify: cannot write report {}: {error}",
            options.report.display()
        );
        return ExitCode::FAILURE;
    }
    let (metadata, metadata_errors) = metadata(&root);
    let mut failed = !metadata_errors.is_empty();
    let mut errors = options.errors.clone();
    errors.extend(metadata_errors);
    let executable = match std::env::current_exe() {
        Ok(path) => Some(path),
        Err(error) => {
            failed = true;
            errors.push(format!("cannot locate current xtask executable: {error}"));
            None
        }
    };
    let mut slot = if errors.is_empty() {
        match ValidationSlot::acquire(&root) {
            Ok(slot) => Some(slot),
            Err((failure, error)) => {
                failed |= failure;
                errors.push(error);
                None
            }
        }
    } else {
        None
    };
    for error in &errors {
        eprintln!("verify: {error}");
    }
    let mut records: Vec<GateRecord> = Vec::new();
    for gate in GATES {
        let required = options.required(gate);
        let mut prerequisite_errors = if required { errors.clone() } else { Vec::new() };
        if required {
            for dependency in gate.dependencies {
                if !records
                    .iter()
                    .any(|record| record.gate.id == *dependency && record.status == Status::Pass)
                {
                    prerequisite_errors.push(format!(
                        "required prerequisite {dependency} did not pass in this invocation"
                    ));
                }
            }
            if matches!(
                gate.action,
                Action::Corpus(_) | Action::AssetTests(_, _) | Action::Media | Action::ReplayCorpus
            ) {
                match &options.assets {
                    None => prerequisite_errors.push("explicit --assets DIR is required; no implicit home-specific corpus is accepted".into()),
                    Some(path) if !path.is_dir() => prerequisite_errors.push(format!("--assets is not a directory: {}", path.display())),
                    Some(_) => {}
                }
            }
            if matches!(gate.action, Action::Conductor) && !options.fixtures.is_dir() {
                prerequisite_errors.push(format!(
                    "conductor fixtures directory is missing: {}",
                    options.fixtures.display()
                ));
            }
            if matches!(gate.action, Action::ConductorNative) {
                let native_fixtures =
                    root.join("tests/fixtures/conductor-native/_manifest_native.json");
                if !native_fixtures.is_file() {
                    prerequisite_errors.push(format!(
                        "committed native reference fixtures are missing: {}",
                        native_fixtures.display()
                    ));
                }
            }
        }
        let mut command = executable
            .as_deref()
            .and_then(|executable| gate_command(gate, &options, &root, executable));
        let specification = command.as_ref().map(command_json);
        let mut evidence = None;
        let (status, outcome) = if !required {
            (Status::Skipped, None)
        } else if matches!(gate.action, Action::Pending) {
            prerequisite_errors.push(format!("not implemented yet: {}", gate.coverage));
            (Status::NotImplemented, None)
        } else if matches!(gate.action, Action::ExternalCi) {
            let (ci_errors, checked) = ci_evidence(options.ci_evidence.as_deref(), &metadata);
            prerequisite_errors.extend(ci_errors);
            evidence = Some(checked);
            println!("verify: {CI_TRUST_BOUNDARY}");
            (
                if prerequisite_errors.is_empty() {
                    Status::Pass
                } else {
                    Status::Blocked
                },
                None,
            )
        } else if !prerequisite_errors.is_empty() {
            (Status::Blocked, None)
        } else if let Some(command) = command.as_mut() {
            println!(
                "\nverify: running {} (timeout {}s): {command:?}",
                gate.id, gate.timeout_secs
            );
            let outcome = execute(command, gate.timeout_secs);
            if outcome.cleanup_uncertain {
                if let Some(slot) = slot.as_mut() {
                    slot.retain = true;
                }
                let error = format!(
                    "{}: process cleanup could not be confirmed; remaining gates are blocked and the validation slot is retained for manual/outer workflow cleanup",
                    gate.id
                );
                eprintln!("verify: {error}");
                errors.push(error);
            }
            let status = if outcome.success {
                Status::Pass
            } else {
                Status::Fail
            };
            (status, Some(outcome))
        } else {
            prerequisite_errors.push("cannot construct required command".into());
            (Status::Blocked, None)
        };
        records.push(GateRecord {
            gate,
            required,
            status,
            errors: prerequisite_errors,
            command: specification,
            outcome,
            evidence,
        });
    }
    let (status, exit_code) = overall(
        records
            .iter()
            .map(|record| (record.required, record.status)),
        failed,
        !errors.is_empty(),
    );
    let acceptance = is_acceptance(options.profile, status);
    println!(
        "\n{:<24} {:<4} {:<16} {:<28} scope",
        "gate", "need", "status", "ignored-test policy"
    );
    for record in &records {
        println!(
            "{:<24} {:<4} {:<16} {:<28} {}",
            record.gate.id,
            if record.required { "yes" } else { "no" },
            record.status.label(),
            record.gate.ignored_tests,
            record.gate.coverage
        );
        for error in &record.errors {
            println!("  {error}");
        }
        if let Some(outcome) = &record.outcome {
            println!(
                "  exit={:?}, timed_out={}, duration={}ms",
                outcome.exit_code, outcome.timed_out, outcome.duration_ms
            );
            if let Some(error) = &outcome.error {
                println!("  {error}");
            }
        }
    }
    for record in &records {
        for error in &record.errors {
            let error = format!("{}: {error}", record.gate.id);
            if !errors.contains(&error) {
                errors.push(error);
            }
        }
    }
    let report = json!({
        "version": VERSION,
        "profile": options.profile_name,
        "target_milestone": options.target().map(|target| format!("M{target}")),
        "status": status.label(),
        "exit_code": exit_code,
        "milestone_acceptance": acceptance,
        "coverage_meaning": if options.profile == Some(Profile::Quick) {
            "Quick portable checks only. NOT milestone acceptance. Ignored/external obligations are skipped, not passed."
        } else {
            "Cumulative acceptance requires every target-required gate to pass with fresh evidence; blocked, skipped and not implemented never count as pass."
        },
        "started_unix_ms": started_unix_ms,
        "duration_ms": start.elapsed().as_millis(),
        "metadata": metadata,
        "process_cleanup": cleanup_policy(),
        "inputs": { "assets": options.assets, "ci_evidence": options.ci_evidence, "out": options.out, "fixtures": options.fixtures },
        "validation_slot": {
            "owner": "verify",
            "path": root.join("target/verify-validation.lock"),
            "acquired": slot.is_some(),
            "retained_due_to_cleanup_error": slot.as_ref().is_some_and(|slot| slot.retain),
            "commands_parallel": false,
            "nested_workflow_wrapper": false,
            "scope": "Serializes verify invocations only. The caller must serialize other heavy jobs with its outer workflow validation slot; gates invoke cargo/current xtask directly to avoid nested-lock deadlock.",
        },
        "prerequisite_errors": errors,
        "gates": records.iter().map(GateRecord::json).collect::<Vec<_>>(),
    });
    if let Err(error) = write_report(&options.report, &report) {
        eprintln!(
            "verify: cannot write final report {}: {error}",
            options.report.display()
        );
        return ExitCode::FAILURE;
    }
    println!(
        "verify: {}; milestone acceptance: {}; report: {}",
        status.label(),
        acceptance,
        options.report.display()
    );
    ExitCode::from(exit_code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn finite_descendant_writer(heartbeat: &Path) -> Command {
        let mut command = Command::new("sh");
        command
            .args([
                "-c",
                r#"
            printf 'metadata stdout\n'
            printf 'metadata stderr\n' >&2
            trap 'exit 0' TERM
            (
                trap '' TERM
                for outer in 1 2 3 4 5; do
                    for inner in 1 2 3 4 5 6 7 8 9 10; do
                        printf x >> "$1"
                        sleep 0.1
                    done
                done
            ) &
            wait
        "#,
                "verify-cleanup-regression",
            ])
            .arg(heartbeat);
        // Finite even if the implementation regresses and leaks the descendant.
        command.stdout(Stdio::null()).stderr(Stdio::null());
        command
    }

    #[cfg(unix)]
    #[test]
    fn timeout_kills_stubborn_descendants_before_releasing_validation_slot() {
        let scratch = Scratch::create().unwrap();
        let heartbeat = scratch.0.join("heartbeat");
        let slot = ValidationSlot::acquire(&scratch.0).unwrap();
        let lock = slot.path.clone();
        let start = Instant::now();
        let outcome = execute_with_timeout(
            &mut finite_descendant_writer(&heartbeat),
            Duration::from_secs(1),
        );
        assert!(outcome.timed_out);
        assert!(!outcome.success);
        assert!(!outcome.cleanup_uncertain, "{:?}", outcome.cleanup);
        let cleanup = outcome.cleanup.as_ref().unwrap();
        assert_eq!(cleanup["method"], "owned_process_group_term_kill");
        assert_eq!(cleanup["leader_reaped"], true);
        assert_eq!(cleanup["original_group_quiescent"], true);
        assert!(cleanup["actions"].as_array().unwrap().len() >= 2);
        for (action, signal) in cleanup["actions"]
            .as_array()
            .unwrap()
            .iter()
            .take(2)
            .zip(["TERM", "KILL"])
        {
            assert_eq!(action["success"], true);
            assert_eq!(action["command"]["args"][1], signal);
            assert_eq!(
                action["command"]["args"][3],
                format!("-{}", outcome.process_id.unwrap())
            );
        }
        assert!(
            lock.exists(),
            "slot must still be held after synchronous cleanup"
        );
        let stopped = fs::read(&heartbeat).expect("descendant must have started writing");
        assert!(!stopped.is_empty());
        thread::sleep(Duration::from_millis(250));
        assert_eq!(
            fs::read(&heartbeat).unwrap(),
            stopped,
            "descendant kept writing after timeout cleanup"
        );
        drop(slot);
        assert!(!lock.exists());
        assert!(start.elapsed() < Duration::from_secs(12));
    }

    #[cfg(unix)]
    #[test]
    fn captured_metadata_timeout_cleans_group_before_reading_temp_files() {
        let scratch = Scratch::create().unwrap();
        let heartbeat = scratch.0.join("heartbeat");
        let start = Instant::now();
        let (stdout, record) = capture_with_timeout(
            &mut finite_descendant_writer(&heartbeat),
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(stdout.contains("metadata stdout"));
        assert!(
            record["stderr"]
                .as_str()
                .unwrap()
                .contains("metadata stderr")
        );
        assert_eq!(record["status"], "fail");
        assert_eq!(record["outcome"]["timed_out"], true);
        assert_eq!(record["outcome"]["cleanup_uncertain"], false);
        assert_eq!(record["outcome"]["cleanup"]["leader_reaped"], true);
        assert_eq!(
            record["outcome"]["cleanup"]["original_group_quiescent"],
            true
        );
        let stopped = fs::read(&heartbeat).unwrap();
        thread::sleep(Duration::from_millis(250));
        assert_eq!(fs::read(&heartbeat).unwrap(), stopped);
        assert!(start.elapsed() < Duration::from_secs(12));
    }

    #[cfg(unix)]
    #[test]
    fn normal_metadata_command_owns_group_and_does_not_signal_after_exit() {
        let mut command = Command::new("sh");
        command.args(["-c", "ps -o pgid= -p \"$$\""]);
        let (stdout, record) = capture(&mut command).unwrap();
        let group: u64 = stdout.trim().parse().unwrap();
        assert_eq!(record["status"], "pass");
        assert_eq!(record["outcome"]["process_id"], group);
        assert_eq!(record["outcome"]["owned_process_group"], group);
        assert_ne!(group, u64::from(std::process::id()));
        assert!(record["outcome"]["cleanup"].is_null());
    }

    #[cfg(unix)]
    #[test]
    fn group_targets_and_snapshots_are_fail_closed() {
        for pid in [0, 1, std::process::id(), u32::MAX] {
            assert_eq!(owned_group_target(pid), None);
        }
        let candidate = if std::process::id() == 42 { 43 } else { 42 };
        assert_eq!(owned_group_target(candidate), Some(format!("-{candidate}")));
        assert_eq!(group_members("11 S\n42 Z\n42 S+\n", 42).unwrap(), (1, 1));
        assert_eq!(group_members("11 S\n42 Z\n", 42).unwrap(), (0, 1));
        assert!(group_members("", 42).is_err());
        assert!(group_members("not a ps snapshot", 42).is_err());
    }

    #[test]
    fn unconfirmed_cleanup_retains_validation_slot_for_manual_recovery() {
        let scratch = Scratch::create().unwrap();
        let mut slot = ValidationSlot::acquire(&scratch.0).unwrap();
        let lock = slot.path.clone();
        slot.retain = true;
        drop(slot);
        assert!(lock.exists());
        assert!(matches!(
            ValidationSlot::acquire(&scratch.0),
            Err((false, _))
        ));
        assert_eq!(cleanup_policy()["retain_slot_on_cleanup_error"], true);
    }

    #[test]
    fn quick_only_requires_the_portable_baseline() {
        let options = Options::parse(&[], Path::new("/workspace"));
        let required: Vec<_> = GATES
            .iter()
            .filter(|gate| options.required(gate))
            .map(|gate| gate.id)
            .collect();
        assert_eq!(required, BASELINE);
        assert_eq!(options.target(), None);
        assert!(!is_acceptance(options.profile, Status::Pass));
    }

    #[test]
    fn cumulative_catalogue_keeps_ci_and_pending_obligations_required() {
        for milestone in 0..=9 {
            let args = [
                "--profile".into(),
                "milestone".into(),
                "--milestone".into(),
                format!("M{milestone}"),
            ];
            let options = Options::parse(&args, Path::new("/workspace"));
            assert!(options.errors.is_empty());
            for gate in GATES {
                assert_eq!(
                    options.required(gate),
                    gate.milestone <= milestone,
                    "{}",
                    gate.id
                );
            }
            assert!(GATES.iter().any(|gate| options.required(gate) && matches!(gate.action, Action::ExternalCi)));
        }
        let release = Options::parse(
            &["--profile".into(), "release".into()],
            Path::new("/workspace"),
        );
        assert_eq!(release.target(), Some(9));
        assert!(GATES.iter().all(|gate| release.required(gate)));
        assert!(
            GATES
                .iter()
                .any(|gate| matches!(gate.action, Action::Pending))
        );
    }

    // These are fabricated data for pure tests, never real execution evidence.
    const SYNTHETIC_HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn synthetic_local() -> Value {
        json!({
            "synthetic_test_fixture": true,
            "git": {"head": SYNTHETIC_HEAD, "dirty": false, "status_porcelain": ""},
            "toolchain": "rustc 1.99.0 (synthetic-test-fixture 2000-01-01)",
            "ci": null,
        })
    }

    fn synthetic_report(os: &str) -> Value {
        let mut metadata = synthetic_local();
        metadata["os"] = json!(os);
        metadata["ci"] = ci_metadata(|key| {
            match key {
                "GITHUB_ACTIONS" => Some("true"),
                "GITHUB_REPOSITORY" => Some("synthetic-owner/synthetic-repo"),
                "GITHUB_RUN_ID" => Some("123"),
                "GITHUB_RUN_ATTEMPT" => Some("1"),
                "GITHUB_JOB" => Some("synthetic_quick"),
                "GITHUB_SERVER_URL" => Some("https://github.com"),
                "GITHUB_SHA" => Some(SYNTHETIC_HEAD),
                _ => None,
            }
            .map(str::to_owned)
        });
        let gates: Vec<_> = GATES.iter().filter_map(|gate| match gate.action {
            Action::Cargo(args) if BASELINE.contains(&gate.id) => Some(json!({
                "id": gate.id, "required": true, "executed": true, "status": "pass",
                "command": {"program": if os == "windows" { r"C:\synthetic\cargo.exe" } else { "/synthetic/cargo" }, "args": args},
                "outcome": {"success": true, "exit_code": 0, "timed_out": false, "error": null},
            })),
            _ => None,
        }).collect();
        json!({
            "synthetic_test_fixture": "fabricated receipt, not actual CI evidence",
            "version": VERSION, "profile": "quick", "status": "pass", "exit_code": 0,
            "milestone_acceptance": false, "target_milestone": null,
            "prerequisite_errors": [], "metadata": metadata, "gates": gates,
        })
    }

    fn synthetic_reports() -> [Option<Value>; 2] {
        [
            Some(synthetic_report("linux")),
            Some(synthetic_report("windows")),
        ]
    }

    #[test]
    fn ci_identity_is_recorded_only_for_explicit_github_actions() {
        assert!(ci_metadata(|_| None).is_null());
        assert!(ci_metadata(|_| Some("false".into())).is_null());
        let report = synthetic_report("linux");
        assert_eq!(report["metadata"]["ci"]["provider"], "github_actions");
        assert_eq!(report["metadata"]["ci"]["sha"], SYNTHETIC_HEAD);
    }

    #[test]
    fn synthetic_consistent_receipts_prove_structure_not_authenticity() {
        // Deliberately accepts these fabricated fields: an offline validator
        // cannot authenticate provenance, which must remain a caller obligation.
        assert!(validate_ci_reports(&synthetic_local(), &synthetic_reports()).is_empty());
        let (errors, evidence) = ci_evidence(None, &synthetic_local());
        assert!(!errors.is_empty());
        assert_eq!(evidence["provenance_verified"], false);
        assert_eq!(
            evidence["validation"],
            "offline_structural_consistency_only"
        );
        assert!(CI_TRUST_BOUNDARY.contains("authentic run URLs"));
    }

    #[test]
    fn missing_ci_reports_never_satisfy_acceptance() {
        let errors = validate_ci_reports(&synthetic_local(), &[None, None]);
        assert!(errors.iter().any(|error| error.contains("linux.json")));
        assert!(errors.iter().any(|error| error.contains("windows.json")));
        assert!(
            !validate_ci_reports(&synthetic_local(), &[Some(synthetic_report("linux")), None])
                .is_empty()
        );
    }

    #[test]
    fn invalid_stale_dirty_or_unexecuted_ci_receipts_are_rejected() {
        for (pointer, replacement) in [
            ("/version", json!(2)),
            ("/profile", json!("milestone")),
            ("/status", json!("fail")),
            ("/exit_code", json!(1)),
            ("/milestone_acceptance", json!(true)),
            ("/target_milestone", json!("M0")),
            ("/prerequisite_errors", json!(["blocked"])),
            ("/metadata/os", json!("linux")),
            (
                "/metadata/git/head",
                json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            ),
            ("/metadata/git/dirty", json!(true)),
            ("/metadata/git/status_porcelain", json!(" M synthetic.rs\n")),
            (
                "/metadata/toolchain",
                json!("rustc 1.98.0 (different synthetic version)"),
            ),
            ("/metadata/ci/provider", json!("local")),
            ("/metadata/ci/repository", json!("other-owner/other-repo")),
            ("/metadata/ci/server_url", json!("https://other.example")),
            ("/metadata/ci/run_id", json!("0")),
            ("/metadata/ci/run_attempt", json!("not-a-number")),
            ("/metadata/ci/job", json!("")),
            (
                "/metadata/ci/sha",
                json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            ),
            ("/gates/0/executed", json!(false)),
            ("/gates/0/required", json!(false)),
            ("/gates/1/status", json!("skipped")),
            ("/gates/2/outcome/success", json!(false)),
            ("/gates/2/outcome/exit_code", json!(2)),
            ("/gates/2/outcome/timed_out", json!(true)),
            ("/gates/2/command/args", json!(["test", "--help"])),
        ] {
            let mut reports = synthetic_reports();
            *reports[1].as_mut().unwrap().pointer_mut(pointer).unwrap() = replacement;
            assert!(
                !validate_ci_reports(&synthetic_local(), &reports).is_empty(),
                "{pointer}"
            );
        }
        let mut reports = synthetic_reports();
        let gates = reports[0].as_mut().unwrap()["gates"]
            .as_array_mut()
            .unwrap();
        gates.push(gates[0].clone());
        assert!(!validate_ci_reports(&synthetic_local(), &reports).is_empty());
        assert!(
            !validate_ci_reports(&synthetic_local(), &[Some(json!("invalid report")), None])
                .is_empty()
        );
    }

    #[test]
    fn complete_ci_identity_and_clean_current_local_revision_are_required() {
        for field in [
            "provider",
            "repository",
            "run_id",
            "run_attempt",
            "job",
            "server_url",
            "sha",
        ] {
            let mut reports = synthetic_reports();
            reports[0].as_mut().unwrap()["metadata"]["ci"]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                !validate_ci_reports(&synthetic_local(), &reports).is_empty(),
                "{field}"
            );
        }
        for (pointer, replacement) in [
            ("/git/dirty", json!(true)),
            ("/git/status_porcelain", json!("?? synthetic.rs\n")),
            (
                "/git/head",
                json!("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
            ),
            ("/toolchain", json!("rustc different synthetic version")),
        ] {
            let mut local = synthetic_local();
            *local.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                !validate_ci_reports(&local, &synthetic_reports()).is_empty(),
                "{pointer}"
            );
        }
        let mut local = synthetic_local();
        local["ci"] = synthetic_report("linux")["metadata"]["ci"].clone();
        assert!(validate_ci_reports(&local, &synthetic_reports()).is_empty());
        local["ci"]["repository"] = json!("other-owner/other-repo");
        assert!(!validate_ci_reports(&local, &synthetic_reports()).is_empty());
    }

    #[test]
    fn acceptance_obligations_do_not_create_false_execution_dependencies() {
        for id in [
            "parse_all_corpus",
            "real_data",
            "parse_atlas_corpus",
            "real_atlas_geometry",
            "real_fonts",
            "media_mini_library",
            "media_mini_xtask",
            "media_synthetic_delay",
            "conductor",
        ] {
            let gate = GATES.iter().find(|gate| gate.id == id).unwrap();
            assert_eq!(gate.dependencies, BASELINE, "{id}");
        }
        let options = Options::parse(
            &[
                "--profile".into(),
                "milestone".into(),
                "--milestone".into(),
                "M0".into(),
                "--ci-evidence".into(),
                "receipts".into(),
            ],
            Path::new("/workspace"),
        );
        assert!(options.errors.is_empty());
        assert_eq!(options.ci_evidence, Some(PathBuf::from("receipts")));
        assert_eq!(
            overall(
                GATES.iter().map(|gate| (
                    options.required(gate),
                    if gate.id == "dual_platform_ci" {
                        Status::Blocked
                    } else {
                        Status::Pass
                    }
                )),
                false,
                false
            ),
            (Status::Blocked, 2)
        );
        let gate = GATES
            .iter()
            .find(|gate| gate.id == "media_synthetic_delay")
            .unwrap();
        let Action::Cargo(args) = gate.action else {
            panic!("expected cargo unit test");
        };
        assert_eq!(
            args,
            &[
                "test",
                "--locked",
                "-p",
                "oxide-media",
                "--lib",
                "cross_correlation_recovers_synthetic_delay",
                "--",
                "--include-ignored"
            ]
        );
    }

    #[test]
    fn strict_real_asset_and_pending_reference_obligations_are_catalogued() {
        let options = Options::parse(
            &["--assets".into(), "corpus".into()],
            Path::new("/workspace"),
        );
        for (id, milestone, package, test) in [
            ("real_data", 1, "oxide-data", "round_trip"),
            ("real_atlas_geometry", 2, "oxide-atlas", "frame_quad"),
            ("real_fonts", 2, "oxide-font", "fonts"),
        ] {
            let gate = GATES.iter().find(|gate| gate.id == id).unwrap();
            assert_eq!(gate.milestone, milestone, "{id}");
            assert_eq!(gate.dependencies, BASELINE, "{id}");
            assert_eq!(gate.ignored_tests, "included_strictly", "{id}");
            let command =
                gate_command(gate, &options, Path::new("/workspace"), Path::new("xtask")).unwrap();
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_str().unwrap())
                .collect();
            assert_eq!(
                args,
                [
                    "test",
                    "--locked",
                    "-p",
                    package,
                    "--test",
                    test,
                    "--",
                    "--include-ignored"
                ],
                "{id}"
            );
            assert!(
                command.get_envs().any(|(key, value)| {
                    key == std::ffi::OsStr::new("OXIDE_ASSETS_ROOT")
                        && value == Some(std::ffi::OsStr::new("corpus"))
                }),
                "{id}"
            );
        }
        for (id, milestone, expected_action) in [
            ("conductor_native_reference", 4, "executable"),
            ("fuzz_parsers", 9, "pending"),
        ] {
            let gate = GATES.iter().find(|gate| gate.id == id).unwrap();
            assert_eq!(gate.milestone, milestone, "{id}");
            match expected_action {
                "executable" => assert!(
                    !matches!(gate.action, Action::Pending | Action::ExternalCi),
                    "{id} must be a fail-closed executable gate"
                ),
                _ => assert!(matches!(gate.action, Action::Pending), "{id}"),
            }
        }
    }

    #[test]
    fn commands_are_locked_strict_and_never_recurse_into_verify() {
        let options = Options::parse(
            &["--assets".into(), "corpus".into()],
            Path::new("/workspace"),
        );
        for gate in GATES {
            let Some(command) =
                gate_command(gate, &options, Path::new("/workspace"), Path::new("xtask"))
            else {
                assert!(matches!(gate.action, Action::Pending | Action::ExternalCi));
                continue;
            };
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect();
            assert_ne!(args[0], "verify");
            if matches!(gate.action, Action::Cargo(_) | Action::AssetTests(_, _))
                && gate.id != "fmt"
            {
                assert!(args.iter().any(|arg| arg == "--locked"), "{}", gate.id);
            }
            if gate.ignored_tests == "included_strictly" {
                assert!(
                    args.iter().any(|arg| arg == "--include-ignored"),
                    "{}",
                    gate.id
                );
            }
            if matches!(gate.action, Action::Corpus(_) | Action::Media) {
                assert!(args.iter().any(|arg| arg == "--corpus"), "{}", gate.id);
            }
        }
    }

    #[test]
    fn only_all_required_passes_can_succeed() {
        for status in [Status::Blocked, Status::NotImplemented, Status::Skipped] {
            assert_eq!(
                overall(
                    [(true, Status::Pass), (true, status)].into_iter(),
                    false,
                    false
                ),
                (Status::Blocked, 2)
            );
        }
        assert_eq!(
            overall(
                [(true, Status::Pass), (false, Status::Skipped)].into_iter(),
                false,
                false
            ),
            (Status::Pass, 0)
        );
        assert_eq!(
            overall(
                [(true, Status::Fail), (true, Status::NotImplemented)].into_iter(),
                false,
                false
            ),
            (Status::Fail, 1)
        );
        assert_eq!(
            overall([(true, Status::Pass)].into_iter(), true, false),
            (Status::Fail, 1)
        );
        assert_eq!(
            overall(std::iter::empty(), false, false),
            (Status::Blocked, 2)
        );
    }
}
