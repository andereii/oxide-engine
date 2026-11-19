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

use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;
use bevy::window::WindowFocused;
use cli::LaunchFlags;
use core::render::animations::AnimationsPlugin;
use oxin_app::{WindowConfig, run_window};
use play::PlayPlugin;
use ui::LoadingScreenPlugin;

const FOCUS_LOST_VOLUME: f32 = 0.2;
const FOCUS_FADE_SPEED: f32 = 6.0;

fn sync_focus_volume(
    time: Res<Time>,
    mut focus_events: MessageReader<WindowFocused>,
    mut global_volume: ResMut<GlobalVolume>,
    mut focused: Local<Option<bool>>,
    mut current_linear: Local<Option<f32>>,
) {
    let is_focused = focused.get_or_insert(true);
    for ev in focus_events.read() {
        *is_focused = ev.focused;
    }
    let target = if *is_focused { 1.0 } else { FOCUS_LOST_VOLUME };
    let cur = current_linear.get_or_insert(1.0);
    if (*cur - target).abs() > 1e-4 {
        let step = time.delta_secs() * FOCUS_FADE_SPEED;
        if *cur > target {
            *cur = (*cur - step).max(target);
        } else {
            *cur = (*cur + step).min(target);
        }
        global_volume.volume = Volume::Linear(*cur);
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
