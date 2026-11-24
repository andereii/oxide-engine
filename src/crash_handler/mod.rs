mod dialog;
mod report;

use std::backtrace::Backtrace;
use std::fs;
use std::panic::PanicHookInfo;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use dialog::{resolve_or_extract_gif, show_native_crash_dialog};
use report::{CrashContext, generate_session_id};

struct RuntimeCrashMeta {
    started_at: Instant,
    session_id: String,
    exe_path: PathBuf,
    restart_args: Vec<String>,
}

static META: OnceLock<RuntimeCrashMeta> = OnceLock::new();
static HANDLING_PANIC: AtomicBool = AtomicBool::new(false);
static FORCE_DIALOG_IN_DEBUG: AtomicBool = AtomicBool::new(false);

/// Decides whether to show the native GUI crash dialog.
///
/// Never pops up in debug builds by default so errors are not duplicated between the terminal
/// and a modal window; can still be explicitly enabled via `--test-crash` or
/// `OXIDE_CRASH_DIALOG=1`.
pub fn should_show_crash_dialog(
    is_debug_build: bool,
    env_override: Option<&str>,
    forced_flag: bool,
) -> bool {
    if forced_flag {
        return true;
    }
    if let Some(val) = env_override {
        let v = val.trim();
        if v == "1" || v.eq_ignore_ascii_case("true") {
            return true;
        }
        if v == "0" || v.eq_ignore_ascii_case("false") {
            return false;
        }
    }
    !is_debug_build
}

/// Installs the global Oxide Engine panic/crash handler.
pub fn install() {
    let exe_path = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("oxide"));
    let restart_args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--test-crash")
        .collect();
    let _ = META.set(RuntimeCrashMeta {
        started_at: Instant::now(),
        session_id: generate_session_id(),
        exe_path,
        restart_args,
    });

    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let env_val = std::env::var("OXIDE_CRASH_DIALOG").ok();
        let show_gui = should_show_crash_dialog(
            cfg!(debug_assertions),
            env_val.as_deref(),
            FORCE_DIALOG_IN_DEBUG.load(Ordering::SeqCst),
        );

        // In debug builds without explicit override, keep the clean default terminal output only.
        if !show_gui {
            previous_hook(info);
            return;
        }

        // Guard against recursive panics opening multiple crash dialogs.
        if HANDLING_PANIC.swap(true, Ordering::SeqCst) {
            previous_hook(info);
            return;
        }

        handle_fatal_panic(info);
    }));
}

/// Triggers a deliberate test panic with the crash dialog explicitly enabled (works in debug
/// builds too via `--test-crash`).
pub fn trigger_test_crash() -> ! {
    FORCE_DIALOG_IN_DEBUG.store(true, Ordering::SeqCst);
    panic!("Simulated fatal error triggered via --test-crash");
}

fn extract_panic_message(info: &PanicHookInfo<'_>) -> String {
    if let Some(s) = info.payload().downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = info.payload().downcast_ref::<String>() {
        s.clone()
    } else {
        "Box<dyn Any> (non-string panic payload)".to_string()
    }
}

fn handle_fatal_panic(info: &PanicHookInfo<'_>) {
    let meta = META.get();
    let session_id = meta.map_or("00000000-0000-4000-8000-000000000000", |m| {
        m.session_id.as_str()
    });
    let uptime = meta.map_or_else(Default::default, |m| m.started_at.elapsed());
    let exe_path = meta
        .map(|m| m.exe_path.clone())
        .unwrap_or_else(|| PathBuf::from("oxide"));
    let restart_args: Vec<String> = meta.map(|m| m.restart_args.clone()).unwrap_or_default();

    let message = extract_panic_message(info);
    let location = info
        .location()
        .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
        .unwrap_or_else(|| "<unknown location>".to_string());
    let thread_name = std::thread::current()
        .name()
        .unwrap_or("<unnamed>")
        .to_string();
    let backtrace = Backtrace::force_capture();

    let ctx = CrashContext::build(session_id, uptime, message, location, thread_name, &backtrace);
    let report_text = ctx.format_report();

    // Persist crash.log (gitignored) and fall back to temp dir if cwd is read-only.
    let report_path = PathBuf::from("crash.log");
    let effective_report_path = if fs::write(&report_path, &report_text).is_ok() {
        report_path
    } else {
        let tmp = std::env::temp_dir().join("oxide_crash.log");
        let _ = fs::write(&tmp, &report_text);
        tmp
    };

    let gif_path = resolve_or_extract_gif();
    let shown = show_native_crash_dialog(
        &effective_report_path,
        gif_path.as_deref(),
        &exe_path,
        &restart_args,
    );
    if !shown {
        eprintln!("{report_text}");
    }
}

#[cfg(test)]
mod tests {
    use super::dialog::resolve_or_extract_gif;
    use super::report::{CrashContext, format_uptime, format_utc_timestamp, generate_session_id};
    use super::should_show_crash_dialog;
    use std::backtrace::Backtrace;
    use std::time::Duration;

    #[test]
    fn debug_builds_suppress_crash_dialog_unless_forced() {
        assert!(!should_show_crash_dialog(true, None, false));
        assert!(should_show_crash_dialog(true, Some("1"), false));
        assert!(should_show_crash_dialog(true, None, true));

        assert!(should_show_crash_dialog(false, None, false));
        assert!(!should_show_crash_dialog(false, Some("0"), false));
    }

    #[test]
    fn formats_utc_timestamp_uptime_and_crash_report_sections() {
        assert_eq!(format_utc_timestamp(0), "1970-01-01 00:00:00 UTC");
        assert_eq!(format_utc_timestamp(1_795_477_212), "2026-11-23 23:40:12 UTC");
        assert_eq!(format_uptime(Duration::from_secs(14 * 60 + 37)), "00h 14m 37s");

        let sid = generate_session_id();
        assert_eq!(sid.len(), 36);

        let ctx = CrashContext::build(
            &sid,
            Duration::from_secs(877),
            "test boom".to_string(),
            "src/main.rs:42:5".to_string(),
            "main".to_string(),
            &Backtrace::disabled(),
        );
        let report = ctx.format_report();
        assert!(report.contains("APPLICATION CRASH REPORT"));
        assert!(report.contains("SYSTEM INFORMATION"));
        assert!(report.contains("CRASH DETAILS"));
        assert!(report.contains("test boom"));
        assert!(report.contains("src/main.rs:42:5"));
    }

    #[test]
    fn resolves_crash_gif_asset() {
        let gif = resolve_or_extract_gif().expect("crash GIF must resolve or extract");
        assert!(gif.is_file());
    }
}
