use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use bevy::app::AppExit;
use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::cli::{LaunchFlags, normalize_capture_ms};

use super::super::scene::PlayState;
use super::super::types::PlayPhase;

#[derive(Resource, Debug, Clone)]
pub(super) struct ParityCaptureState {
    pub(super) targets_ms: Vec<f64>,
    pub(super) output_dir: PathBuf,
    pub(super) next_index: usize,
    pub(super) saved_count: usize,
    pub(super) in_flight: bool,
    dir_ready: bool,
    exit_requested: bool,
}

impl ParityCaptureState {
    pub(super) fn from_flags(flags: &LaunchFlags) -> Option<Self> {
        let mut targets_ms = flags.capture_timestamps_ms.clone();
        normalize_capture_ms(&mut targets_ms);
        if targets_ms.is_empty() {
            return None;
        }
        let output_dir = flags
            .capture_out_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("captures"));
        Some(Self {
            targets_ms,
            output_dir,
            next_index: 0,
            saved_count: 0,
            in_flight: false,
            dir_ready: false,
            exit_requested: false,
        })
    }

    pub(super) fn next_due_target(&self, current_ms: f64) -> Option<(usize, f64, PathBuf)> {
        if self.in_flight || self.exit_requested {
            return None;
        }
        let target_ms = *self.targets_ms.get(self.next_index)?;
        if current_ms + 0.001 < target_ms {
            return None;
        }
        let path = capture_shot_path(&self.output_dir, self.next_index, target_ms);
        Some((self.next_index, target_ms, path))
    }

    pub(super) fn should_exit_success(&self, phase: PlayPhase) -> bool {
        if self.exit_requested || self.in_flight {
            return false;
        }
        self.next_index >= self.targets_ms.len() || phase == PlayPhase::Ended
    }
}

pub(super) fn capture_shot_path(output_dir: &Path, index: usize, target_ms: f64) -> PathBuf {
    let rounded_ms = target_ms.max(0.0).round() as u64;
    output_dir.join(format!("shot_{index:02}_{rounded_ms}ms.png"))
}

/// Captures screenshots of the primary window at configured song timestamps and exits when done.
pub(super) fn drive_parity_capture(
    mut commands: Commands,
    state: Res<PlayState>,
    mut capture: ResMut<ParityCaptureState>,
    mut exit: MessageWriter<AppExit>,
) {
    if capture.exit_requested {
        return;
    }

    if state.critical_failure {
        eprintln!("[PARITY] ERROR {}", state.status_message);
        let _ = std::io::stderr().flush();
        capture.exit_requested = true;
        exit.write(AppExit::from_code(2));
        return;
    }

    if capture.in_flight {
        return;
    }

    if capture.should_exit_success(state.phase) {
        if state.phase == PlayPhase::Ended && capture.next_index < capture.targets_ms.len() {
            println!(
                "[PARITY] ENDED_EARLY captured={}/{} song_ms={:.3}",
                capture.saved_count,
                capture.targets_ms.len(),
                state.conductor.song_position_ms
            );
        } else {
            println!(
                "[PARITY] DONE saved={}/{}",
                capture.saved_count,
                capture.targets_ms.len()
            );
        }
        let _ = std::io::stdout().flush();
        capture.exit_requested = true;
        exit.write(AppExit::Success);
        return;
    }

    if state.is_in_countdown || state.phase != PlayPhase::Playing {
        return;
    }

    let current_ms = state.conductor.song_position_ms;
    let Some((idx, target_ms, output_path)) = capture.next_due_target(current_ms) else {
        return;
    };

    if !capture.dir_ready {
        if let Err(err) = fs::create_dir_all(&capture.output_dir) {
            eprintln!(
                "[PARITY] ERROR failed to create capture directory {}: {err}",
                capture.output_dir.display()
            );
            let _ = std::io::stderr().flush();
            capture.exit_requested = true;
            exit.write(AppExit::from_code(2));
            return;
        }
        capture.dir_ready = true;
    }

    capture.next_index += 1;
    capture.in_flight = true;

    let mut saver = save_to_disk(output_path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |event: On<ScreenshotCaptured>, mut state: ResMut<ParityCaptureState>| {
            saver(event);
            state.saved_count += 1;
            state.in_flight = false;
            println!(
                "[PARITY] SHOT idx={idx} target_ms={target_ms:.3} actual_ms={current_ms:.3} path={}",
                output_path.display()
            );
            let _ = std::io::stdout().flush();
        },
    );
}
