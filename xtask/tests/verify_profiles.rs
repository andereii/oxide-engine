//! Lightweight CLI checks only: never launch the validation/Cargo gate commands.

use serde_json::Value;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

static NEXT_CASE: AtomicU64 = AtomicU64::new(0);

struct Case(PathBuf);

struct ResultData {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Case {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "oxide-verify-cli-{}-{}",
            std::process::id(),
            NEXT_CASE.fetch_add(1, Ordering::Relaxed)
        ));
        // Do not remove or reuse another invocation's files, including stale ones.
        fs::create_dir(&path).expect("create fresh test directory");
        Self(path)
    }

    fn report_path(&self) -> PathBuf {
        self.0.join("report.json")
    }

    fn run(&self, args: &[&str], report: &Path) -> ResultData {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("workspace parent");
        let stdout_path = self.0.join("stdout");
        let stderr_path = self.0.join("stderr");
        let mut child = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .arg("verify")
            .args(args)
            .arg("--report")
            .arg(report)
            .current_dir(root)
            .stdin(Stdio::null())
            .stdout(File::create(&stdout_path).expect("create stdout"))
            .stderr(File::create(&stderr_path).expect("create stderr"))
            .spawn()
            .expect("spawn xtask verify CLI check");
        let start = Instant::now();
        let status = loop {
            match child.try_wait().expect("poll CLI check") {
                Some(status) => break status,
                None if start.elapsed() < Duration::from_secs(40) => {
                    thread::sleep(Duration::from_millis(25));
                }
                None => {
                    let _ = child.kill();
                    let reap_start = Instant::now();
                    while reap_start.elapsed() < Duration::from_secs(1) {
                        if matches!(child.try_wait(), Ok(Some(_))) {
                            break;
                        }
                        thread::sleep(Duration::from_millis(25));
                    }
                    panic!("CLI-only verification check exceeded its 40s timeout");
                }
            }
        };
        ResultData {
            code: status.code().unwrap_or(-1),
            stdout: fs::read_to_string(stdout_path).expect("read stdout"),
            stderr: fs::read_to_string(stderr_path).expect("read stderr"),
        }
    }

    fn report(&self) -> Value {
        serde_json::from_slice(&fs::read(self.report_path()).expect("report must exist"))
            .expect("report must be valid JSON")
    }
}

impl Drop for Case {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn assert_no_gate_execution(report: &Value) {
    assert_eq!(report["version"], 1);
    assert_eq!(report["milestone_acceptance"], false);
    assert_eq!(report["validation_slot"]["acquired"], false);
    assert_eq!(
        report["validation_slot"]["retained_due_to_cleanup_error"],
        false
    );
    assert_eq!(
        report["process_cleanup"]["cleanup_before_slot_release"],
        true
    );
    assert_eq!(
        report["process_cleanup"]["retain_slot_on_cleanup_error"],
        true
    );
    assert!(
        report["process_cleanup"]["limits"]
            .as_str()
            .expect("cleanup limits")
            .contains("escaping")
    );
    #[cfg(unix)]
    for command in report["metadata"]["commands"]
        .as_array()
        .expect("metadata commands")
    {
        assert_eq!(
            command["outcome"]["owned_process_group"],
            command["outcome"]["process_id"]
        );
        assert!(command["outcome"]["process_id"].is_number());
    }
    for gate in report["gates"].as_array().expect("static gate catalogue") {
        assert_eq!(gate["executed"], false, "unexpected gate execution: {gate}");
        assert!(
            gate["outcome"].is_null(),
            "unexpected gate evidence: {gate}"
        );
        assert_ne!(gate["status"], "pass", "unexecuted gate cannot pass");
    }
}

#[test]
fn help_explains_default_scope_without_running_gates_or_writing_report() {
    let case = Case::new();
    let result = case.run(&["--help"], &case.report_path());
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("quick (default)"));
    assert!(result.stdout.contains("NOT milestone acceptance"));
    assert!(result.stdout.contains("--milestone M0..M9"));
    assert!(result.stdout.contains("Dual-platform CI remains BLOCKED"));
    assert!(result.stdout.contains("--ci-evidence"));
    assert!(result.stdout.contains("NOT authenticity"));
    assert!(!case.report_path().exists());
}

#[test]
fn invalid_profile_still_writes_fresh_failure_report() {
    let case = Case::new();
    fs::write(
        case.report_path(),
        r#"{"status":"pass","milestone_acceptance":true}"#,
    )
    .expect("seed obsolete evidence");
    let result = case.run(&["--profile", "pretend"], &case.report_path());
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stderr.contains("invalid profile: pretend"));
    let report = case.report();
    assert_no_gate_execution(&report);
    assert_eq!(report["profile"], "pretend");
    assert_eq!(report["status"], "blocked");
    assert_eq!(report["exit_code"], 2);
    assert!(report["metadata"]["git"]["head"].is_string());
    assert!(report["metadata"]["git"]["dirty"].is_boolean());
    assert!(report["metadata"]["toolchain"].is_string());
}

#[test]
fn milestone_profile_requires_an_explicit_target() {
    let case = Case::new();
    let result = case.run(&["--profile", "milestone"], &case.report_path());
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stderr.contains("requires --milestone M0..M9"));
    let report = case.report();
    assert_no_gate_execution(&report);
    assert_eq!(report["profile"], "milestone");
    assert!(report["target_milestone"].is_null());
}

#[test]
fn target_cannot_silently_turn_quick_into_acceptance() {
    let case = Case::new();
    let result = case.run(&["--milestone", "M0"], &case.report_path());
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(result.stdout.contains("NOT milestone acceptance"));
    assert!(
        result
            .stderr
            .contains("only valid with --profile milestone")
    );
    let report = case.report();
    assert_no_gate_execution(&report);
    assert_eq!(report["profile"], "quick");
    let gates = report["gates"].as_array().expect("gates");
    for gate in gates.iter().filter(|gate| gate["required"] == false) {
        assert_eq!(gate["status"], "skipped");
    }
}

#[test]
fn invalid_release_options_still_list_all_pending_obligations() {
    let case = Case::new();
    // The deliberately invalid option prevents heavy validation from starting.
    let result = case.run(&["--profile", "release", "--invalid"], &case.report_path());
    assert_eq!(result.code, 2, "{}", result.stderr);
    let report = case.report();
    assert_no_gate_execution(&report);
    assert_eq!(report["target_milestone"], "M9");
    let gates = report["gates"].as_array().expect("gates");
    assert!(gates.iter().all(|gate| gate["required"] == true));
    let mut not_implemented = [
        "render_diff",
        "perf_budget",
        "native_note_reference",
        "full_chart_gameplay",
        "ui_save_e2e",
        "ruby_mods",
        "release_binaries",
        "release_size",
        "reproducibility",
        "fuzz_parsers",
    ];
    not_implemented.sort_unstable();
    for id in not_implemented {
        let gate = gates
            .iter()
            .find(|gate| gate["id"] == id)
            .expect("required pending gate");
        assert_eq!(gate["status"], "not_implemented", "{id}");
    }
    // The wired M4/M5 obligations must not be `not_implemented` anymore: they
    // are executable, fail-closed gates. With unmet invocation-level
    // preconditions they report blocked (never a pass) with diagnostics.
    for id in [
        "conductor_native_reference",
        "replay",
        "input_jitter",
        "gameplay_sim_bopeebo",
    ] {
        let gate = gates
            .iter()
            .find(|gate| gate["id"] == id)
            .expect("wired obligation gate");
        assert_eq!(gate["status"], "blocked", "{id}");
        let errors = gate["prerequisite_errors"].as_array().expect("errors");
        assert!(
            !errors.is_empty(),
            "{id} must carry a fail-closed diagnostic"
        );
    }
    let ci = gates
        .iter()
        .find(|gate| gate["id"] == "dual_platform_ci")
        .expect("CI gate");
    assert_eq!(ci["status"], "blocked");
    for id in [
        "parse_all_corpus",
        "real_data",
        "parse_atlas_corpus",
        "real_atlas_geometry",
        "real_fonts",
        "media_corpus",
    ] {
        let gate = gates
            .iter()
            .find(|gate| gate["id"] == id)
            .expect("corpus gate");
        assert_eq!(gate["status"], "blocked");
        assert!(
            gate["prerequisite_errors"]
                .as_array()
                .expect("errors")
                .iter()
                .any(|error| {
                    error
                        .as_str()
                        .is_some_and(|error| error.contains("explicit --assets"))
                })
        );
    }
}

#[test]
fn explicit_ci_directory_is_consumed_and_missing_artifacts_stay_blocked() {
    let case = Case::new();
    // Invalid options keep this a CLI-only test: no validation gates can start.
    let result = case.run(
        &[
            "--profile",
            "milestone",
            "--milestone",
            "M0",
            "--invalid",
            "--ci-evidence",
            case.0.to_str().expect("temporary directory"),
        ],
        &case.report_path(),
    );
    assert_eq!(result.code, 2, "{}", result.stderr);
    assert!(!result.stderr.contains("unknown option: --ci-evidence"));
    let report = case.report();
    assert_no_gate_execution(&report);
    let ci = report["gates"]
        .as_array()
        .expect("gates")
        .iter()
        .find(|gate| gate["id"] == "dual_platform_ci")
        .expect("CI gate");
    assert_eq!(ci["status"], "blocked");
    assert_eq!(ci["evidence"]["structurally_consistent"], false);
    assert_eq!(ci["evidence"]["provenance_verified"], false);
    let files = ci["evidence"]["files"].as_array().expect("CI file records");
    assert_eq!(files.len(), 2);
    for (file, os) in files.iter().zip(["linux", "windows"]) {
        assert_eq!(
            Path::new(file["path"].as_str().expect("file path")),
            case.0.join(format!("{os}.json"))
        );
        assert!(file["error"].is_string());
    }
}

#[test]
fn report_write_failure_is_not_success_or_a_skip() {
    let case = Case::new();
    let result = case.run(&["--profile", "invalid"], &case.0);
    assert_eq!(result.code, 1);
    assert!(result.stderr.contains("cannot write report"));
}
