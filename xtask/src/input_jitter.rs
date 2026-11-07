//! `cargo xtask input-jitter` — uinput delivery-latency report on this machine.
//!
//! Gate report (M4 contract §4): p50/p99 of the delta between each event's
//! kernel timestamp and the userspace clock read right after the poll that
//! returned it, from the uinput probe loop. Target guideline is 2 ms; above it
//! the documented mitigation is kernel-timestamp→audio-clock interpolation
//! (never "ignore the input").
//!
//! Honesty bounds, stated in the output: this measures uinput→userspace
//! delivery jitter only. It is a Linux harness datum — NOT a real audio-clock
//! correlation, NOT a Windows Raw Input measurement — and it does not cover
//! the platform-specific hardware-evidence obligations that remain.

use std::process::ExitCode;

const TARGET_P99_MS: f64 = 2.0;

pub fn run(rest: &[String]) -> ExitCode {
    let samples = match parse_samples(rest) {
        Ok(samples) => samples,
        Err(message) => {
            eprintln!("input-jitter: {message}");
            return ExitCode::from(2);
        }
    };

    let report = match oxide_input::probe::run(samples) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("input-jitter: probe failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    println!(
        "input-jitter: samples {} p50_ms {:.3} p99_ms {:.3}",
        report.samples, report.p50_ms, report.p99_ms
    );
    if report.p99_ms <= TARGET_P99_MS {
        println!("input-jitter: p99 <= {TARGET_P99_MS} ms target; proceeding.");
    } else {
        println!(
            "input-jitter: p99 > {TARGET_P99_MS} ms target; documented mitigation is the \
             kernel-timestamp→audio-clock interpolation, not ignoring the input."
        );
    }
    println!(
        "input-jitter: uinput delivery jitter only; no real AudioClock correlation; \
         Linux-only harness datum (Windows Raw Input is a separate obligation); \
         M5/hardware audio-clock tracing is a separate milestone deliverable."
    );
    ExitCode::SUCCESS
}

fn parse_samples(rest: &[String]) -> Result<usize, String> {
    const DEFAULT_SAMPLES: usize = 2000;
    match rest {
        [] => Ok(DEFAULT_SAMPLES),
        [flag, value] if flag == "--samples" => value
            .parse::<usize>()
            .map_err(|_| format!("invalid --samples value {value:?}: expected a positive integer")),
        other => Err(format!(
            "unsupported arguments {other:?}: usage: cargo xtask input-jitter [--samples N]"
        )),
    }
}
