mod animations;
mod audio;
mod input;
mod play;
mod utils;

use animations::AnimationsPlugin;
use oxin_app::{WindowConfig, run_window};
use play::PlayPlugin;

fn main() {
    let vsync = std::env::var("OXIDE_VSYNC")
        .map(|v| v != "0")
        .unwrap_or(true);
    run_window(
        WindowConfig {
            title: "Oxide Engine - PlayState".to_string(),
            vsync,
            ..Default::default()
        },
        |app| {
            app.add_plugins((AnimationsPlugin, PlayPlugin));
        },
    );
}
