mod animations;
mod audio;
mod input;
mod play;
mod utils;

use animations::AnimationsPlugin;
use oxin_app::{WindowConfig, run_window};
use play::PlayPlugin;

fn main() {
    run_window(
        WindowConfig {
            title: "Oxide Engine - PlayState".to_string(),
            ..Default::default()
        },
        |app| {
            app.add_plugins((AnimationsPlugin, PlayPlugin));
        },
    );
}
