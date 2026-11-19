mod cli;
mod core;
mod mods;
mod play;
mod ui;
mod utils;

pub use crate::core::audio;
pub use crate::core::input;
pub use crate::core::render;
pub use crate::core::render::animations;
pub use crate::mods::scripting;

use bevy::audio::{AudioSink, AudioSinkPlayback, Volume};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use cli::LaunchFlags;
use core::render::animations::AnimationsPlugin;
use oxin_app::{WindowConfig, run_window};
use play::PlayPlugin;
use ui::LoadingScreenPlugin;

const FOCUS_LOST_VOLUME: f32 = 0.2;
const FOCUS_FADE_SPEED: f32 = 6.0;

fn sync_focus_volume(
    time: Res<Time>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sinks: Query<&mut AudioSink>,
    added_sinks: Query<Entity, Added<AudioSink>>,
    mut current_linear: Local<Option<f32>>,
) {
    let is_focused = windows.iter().next().map(|w| w.focused).unwrap_or(true);
    let target = if is_focused { 1.0 } else { FOCUS_LOST_VOLUME };
    let cur = current_linear.get_or_insert(1.0);
    let prev = *cur;

    if (prev - target).abs() > 1e-4 {
        let step = time.delta_secs() * FOCUS_FADE_SPEED;
        *cur = if prev > target {
            (prev - step).max(target)
        } else {
            (prev + step).min(target)
        };
    }

    let ratio = if prev > 1e-4 { *cur / prev } else { 1.0 };
    let changed = (*cur - prev).abs() > 1e-5;

    if changed {
        for mut sink in &mut sinks {
            let scaled = (sink.volume().to_linear() * ratio).max(0.0);
            sink.set_volume(Volume::Linear(scaled));
        }
    } else if *cur < 0.9999 {
        for entity in &added_sinks {
            if let Ok(mut sink) = sinks.get_mut(entity) {
                let scaled = (sink.volume().to_linear() * *cur).max(0.0);
                sink.set_volume(Volume::Linear(scaled));
            }
        }
    }
}

fn main() {
    let flags = LaunchFlags::from_env_args();
    if flags.show_help {
        LaunchFlags::print_help();
        return;
    }

    let vsync = flags.vsync.unwrap_or_else(|| {
        std::env::var("OXIDE_VSYNC")
            .map(|v| v != "0")
            .unwrap_or(true)
    });
    let render_backend = flags.render_backend;
    run_window(
        WindowConfig {
            title: "Oxide Engine - PlayState".to_string(),
            vsync,
            render_backend,
            ..Default::default()
        },
        move |app| {
            app.add_plugins((
                AnimationsPlugin,
                LoadingScreenPlugin,
                PlayPlugin::with_flags(flags),
            ))
            .add_systems(Update, sync_focus_volume);
        },
    );
}
