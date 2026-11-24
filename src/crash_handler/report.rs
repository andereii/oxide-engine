use std::backtrace::Backtrace;
use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub const ENGINE_NAME: &str = "Oxide Engine";
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const BUG_REPORT_URL: &str = "https://github.com/andereii/oxide-engine/issues/new";

#[derive(Debug, Clone)]
pub struct CrashContext {
    pub timestamp_utc: String,
    pub app_name: String,
    pub version_line: String,
    pub session_id: String,
    pub uptime: String,
    pub panic_message: String,
    pub location: String,
    pub thread_name: String,
    pub backtrace: String,
    pub os_info: String,
    pub cpu_info: String,
    pub ram_info: String,
}

impl CrashContext {
    pub fn build(
        session_id: &str,
        uptime: Duration,
        panic_message: String,
        location: String,
        thread_name: String,
        backtrace: &Backtrace,
    ) -> Self {
        let now_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let app_name = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "oxide".to_string());

        Self {
            timestamp_utc: format_utc_timestamp(now_secs),
            app_name,
            version_line: format!("{ENGINE_VERSION}-{profile}"),
            session_id: session_id.to_string(),
            uptime: format_uptime(uptime),
            panic_message,
            location,
            thread_name,
            backtrace: format!("{backtrace}"),
            os_info: collect_os_info(),
            cpu_info: collect_cpu_info(),
            ram_info: collect_ram_info(),
        }
    }

    pub fn format_report(&self) -> String {
        let mut out = String::with_capacity(2048);
        out.push_str("========================================================================\n");
        out.push_str("APPLICATION CRASH REPORT\n");
        out.push_str("========================================================================\n");
        out.push_str(&format!("Timestamp:    {}\n", self.timestamp_utc));
        out.push_str(&format!("Application:  {}\n", self.app_name));
        out.push_str(&format!("Version:      {}\n", self.version_line));
        out.push_str(&format!("Session ID:   {}\n", self.session_id));
        out.push_str(&format!("Uptime:       {}\n", self.uptime));
        out.push_str("\n----------------------------------------\n");
        out.push_str("SYSTEM INFORMATION\n");
        out.push_str("----------------------------------------\n");
        out.push_str(&format!("OS:           {}\n", self.os_info));
        out.push_str(&format!("CPU:          {}\n", self.cpu_info));
        out.push_str(&format!("RAM:          {}\n", self.ram_info));
        out.push_str("\n----------------------------------------\n");
        out.push_str("CRASH DETAILS\n");
        out.push_str("----------------------------------------\n");
        out.push_str(&format!("Thread:       {}\n", self.thread_name));
        out.push_str(&format!("Location:     {}\n", self.location));
        out.push_str(&format!("Message:      {}\n", self.panic_message));
        out.push_str("\nBacktrace:\n");
        out.push_str(&self.backtrace);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// Generates a compact UUID-like session identifier without extra crates.
pub fn generate_session_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let pid = u128::from(std::process::id());
    let mut x = nanos ^ (pid.wrapping_mul(0x9E37_79B9_7F4A_7C15_u128));
    // SplitMix-style avalanche
    x ^= x >> 33;
    x = x.wrapping_mul(0xFF51_AFD7_ED55_8CCD_u128);
    x ^= x >> 33;
    let hi = (x >> 64) as u64;
    let lo = x as u64;
    format!(
        "{:08x}-{:04x}-4{:03x}-{:04x}-{:012x}",
        (hi >> 32) as u32,
        ((hi >> 16) & 0xFFFF) as u16,
        (hi & 0x0FFF) as u16,
        (((lo >> 48) & 0x3FFF) | 0x8000) as u16,
        lo & 0xFFFF_FFFF_FFFF
    )
}

/// Converts UNIX seconds into `YYYY-MM-DD HH:MM:SS UTC` using Howard Hinnant's civil calendar math.
pub fn format_utc_timestamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let rem = unix_secs % 86_400;
    let hour = rem / 3_600;
    let minute = (rem % 3_600) / 60;
    let second = rem % 60;

    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = y + i64::from(m <= 2);

    format!("{year:04}-{m:02}-{d:02} {hour:02}:{minute:02}:{second:02} UTC")
}

pub fn format_uptime(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3_600;
    let m = (total % 3_600) / 60;
    let s = total % 60;
    format!("{h:02}h {m:02}m {s:02}s")
}

fn collect_os_info() -> String {
    #[cfg(target_os = "linux")]
    {
        let pretty = fs::read_to_string("/etc/os-release").ok().and_then(|txt| {
            txt.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                .map(|v| v.trim().trim_matches('"').to_string())
        });
        let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
            .ok()
            .map(|s| s.trim().to_string());
        match (pretty, kernel) {
            (Some(p), Some(k)) => format!("{p} ({}, kernel {k})", std::env::consts::ARCH),
            (Some(p), None) => format!("{p} ({})", std::env::consts::ARCH),
            (None, Some(k)) => format!("Linux ({}, kernel {k})", std::env::consts::ARCH),
            (None, None) => format!("Linux ({})", std::env::consts::ARCH),
        }
    }
    #[cfg(target_os = "windows")]
    {
        let ver = std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        match ver {
            Some(v) => format!("{v} ({})", std::env::consts::ARCH),
            None => format!("Windows ({})", std::env::consts::ARCH),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        format!("{} ({})", std::env::consts::OS, std::env::consts::ARCH)
    }
}

fn collect_cpu_info() -> String {
    let cores = std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1);

    #[cfg(target_os = "linux")]
    {
        if let Ok(cpuinfo) = fs::read_to_string("/proc/cpuinfo")
            && let Some(model) = cpuinfo
                .lines()
                .find_map(|l| l.strip_prefix("model name"))
                .and_then(|rest| rest.split_once(':'))
                .map(|(_, v)| v.trim().to_string())
        {
            return format!("{model} ({cores} Cores)");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(ident) = std::env::var("PROCESSOR_IDENTIFIER")
            && !ident.trim().is_empty()
        {
            return format!("{} ({cores} Cores)", ident.trim());
        }
    }

    format!("{} ({cores} Cores)", std::env::consts::ARCH)
}

fn collect_ram_info() -> String {
    #[cfg(target_os = "linux")]
    {
        if let Ok(meminfo) = fs::read_to_string("/proc/meminfo") {
            let mut total_kb = None;
            let mut avail_kb = None;
            for line in meminfo.lines() {
                if let Some(rest) = line.strip_prefix("MemTotal:") {
                    total_kb = rest.split_whitespace().next().and_then(|s| s.parse::<u64>().ok());
                } else if let Some(rest) = line.strip_prefix("MemAvailable:") {
                    avail_kb = rest.split_whitespace().next().and_then(|s| s.parse::<u64>().ok());
                }
            }
            if let (Some(tot), Some(avail)) = (total_kb, avail_kb) {
                return format!("{} MB Total / {} MB Available", tot / 1024, avail / 1024);
            }
        }
    }
    #[cfg(target_os = "windows")]
    {
        let out = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$os=Get-CimInstance Win32_OperatingSystem; \"{0} MB Total / {1} MB Available\" -f [int]($os.TotalVisibleMemorySize/1024), [int]($os.FreePhysicalMemory/1024)",
            ])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        if let Some(ram) = out {
            return ram;
        }
    }

    "Unavailable".to_string()
}
