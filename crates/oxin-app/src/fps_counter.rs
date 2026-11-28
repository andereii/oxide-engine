use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::time::Real;

const FPS_REFRESH_INTERVAL_S: f64 = 0.25;
const FPS_OVERLAY_CAMERA_ORDER: isize = 1000;
const FPS_OVERLAY_Z_INDEX: i32 = i32::MAX - 32;
const BYTES_PER_MIB: f64 = 1024.0 * 1024.0;
const BYTES_PER_GIB: f64 = 1024.0 * 1024.0 * 1024.0;

pub(crate) struct FpsCounterPlugin;

impl Plugin for FpsCounterPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_fps_counter)
            .add_systems(Update, update_fps_counter);
    }
}

#[derive(Component)]
struct FpsOverlayCamera;

#[derive(Component, Clone, Copy)]
enum PerfOverlayLine {
    Fps,
    FrameMs,
    Ram,
}

#[derive(Default)]
struct FpsCounterState {
    frames: u32,
    elapsed_s: f64,
}

/// Spawns a dedicated topmost camera and top-left screen-space performance overlay
/// (FPS, frame time in ms, and app RAM) above all gameplay, HUD, pause, loading, and overlay layers.
fn spawn_fps_counter(mut commands: Commands) {
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: FPS_OVERLAY_CAMERA_ORDER,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Msaa::Off,
            RenderLayers::none(),
            FpsOverlayCamera,
        ))
        .id();

    let shadow = TextShadow {
        offset: Vec2::splat(1.5),
        color: Color::srgba(0.0, 0.0, 0.0, 0.9),
    };
    let sub_color = Color::srgb(0.88, 0.88, 0.88);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(2.0),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            UiTargetCamera(camera),
            GlobalZIndex(FPS_OVERLAY_Z_INDEX),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("FPS: --"),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                shadow,
                PerfOverlayLine::Fps,
                Pickable::IGNORE,
            ));
            parent.spawn((
                Text::new("MS: --"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(sub_color),
                shadow,
                PerfOverlayLine::FrameMs,
                Pickable::IGNORE,
            ));
            parent.spawn((
                Text::new("RAM: --"),
                TextFont {
                    font_size: FontSize::Px(14.0),
                    ..default()
                },
                TextColor(sub_color),
                shadow,
                PerfOverlayLine::Ram,
                Pickable::IGNORE,
            ));
        });
}

fn format_fps(fps: f64) -> String {
    if !fps.is_finite() {
        return "FPS: --".to_string();
    }
    format!("FPS: {:.0}", fps.round().max(0.0))
}

fn format_frame_ms(frame_ms: f64) -> String {
    if !frame_ms.is_finite() || frame_ms < 0.0 {
        return "MS: --".to_string();
    }
    format!("MS: {frame_ms:.2}")
}

fn format_ram_bytes(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "RAM: --".to_string();
    };
    let bytes_f = bytes as f64;
    if bytes_f >= BYTES_PER_GIB {
        format!("RAM: {:.2} GB", bytes_f / BYTES_PER_GIB)
    } else {
        format!("RAM: {:.1} MB", bytes_f / BYTES_PER_MIB)
    }
}

#[cfg(target_os = "linux")]
fn parse_vm_rss_bytes(status: &str) -> Option<u64> {
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            return kb.checked_mul(1024);
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn current_process_ram_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_rss_bytes(&status)
}

#[cfg(target_os = "windows")]
fn current_process_ram_bytes() -> Option<u64> {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            ppsmem_counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };

    // SAFETY: GetCurrentProcess returns a valid pseudo-handle for this process, and `counters`
    // is a properly aligned, stack-allocated `PROCESS_MEMORY_COUNTERS` with `cb` set to its size.
    let ok = unsafe {
        let handle = GetCurrentProcess();
        K32GetProcessMemoryInfo(handle, &mut counters, counters.cb)
    };
    if ok != 0 {
        Some(counters.working_set_size as u64)
    } else {
        None
    }
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn current_process_ram_bytes() -> Option<u64> {
    None
}

/// Updates the on-screen FPS, frame time (ms), and RAM indicators over fixed wall-clock windows
/// to avoid jitter and excess UI relayout.
fn update_fps_counter(
    time: Res<Time<Real>>,
    mut state: Local<FpsCounterState>,
    mut query: Query<(&PerfOverlayLine, &mut Text)>,
) {
    let delta_s = time.delta_secs_f64();
    if delta_s <= 0.0 {
        return;
    }

    state.frames = state.frames.saturating_add(1);
    state.elapsed_s += delta_s;
    if state.elapsed_s < FPS_REFRESH_INTERVAL_S {
        return;
    }

    let frames_f = f64::from(state.frames);
    let fps = frames_f / state.elapsed_s;
    let frame_ms = (state.elapsed_s / frames_f) * 1000.0;
    let fps_label = format_fps(fps);
    let ms_label = format_frame_ms(frame_ms);
    let ram_label = format_ram_bytes(current_process_ram_bytes());

    for (line, mut text) in &mut query {
        **text = match line {
            PerfOverlayLine::Fps => fps_label.clone(),
            PerfOverlayLine::FrameMs => ms_label.clone(),
            PerfOverlayLine::Ram => ram_label.clone(),
        };
    }
    state.frames = 0;
    state.elapsed_s = 0.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_perf_labels() {
        assert_eq!(format_fps(59.6), "FPS: 60");
        assert_eq!(format_fps(f64::NAN), "FPS: --");
        assert_eq!(format_frame_ms(16.666), "MS: 16.67");
        assert_eq!(format_frame_ms(f64::INFINITY), "MS: --");
        assert_eq!(format_ram_bytes(None), "RAM: --");
        assert_eq!(format_ram_bytes(Some(128 * 1024 * 1024)), "RAM: 128.0 MB");
        assert_eq!(
            format_ram_bytes(Some(2 * 1024 * 1024 * 1024)),
            "RAM: 2.00 GB"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn parses_linux_vm_rss() {
        let sample = "Name:\toxide\nVmPeak:\t  500000 kB\nVmRSS:\t  123456 kB\n";
        assert_eq!(parse_vm_rss_bytes(sample), Some(123_456 * 1024));
        assert!(current_process_ram_bytes().is_some_and(|b| b > 0));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn reads_windows_working_set() {
        assert!(current_process_ram_bytes().is_some_and(|b| b > 0));
    }
}

