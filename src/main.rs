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

use cli::LaunchFlags;
use core::render::animations::AnimationsPlugin;
use oxin_app::{WindowConfig, run_window};
use play::PlayPlugin;
use ui::LoadingScreenPlugin;

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
            ));
        },
    );
}
