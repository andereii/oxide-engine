mod cli;
mod core;
mod crash_handler;
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
use ui::{
    ActiveScene, AppScene, LoadingScreenPlugin, MainMenuPlugin, MenuGameplayOptions,
    MenuSharedPlugin, SoundTrayPlugin, TitlePlugin,
};
use utils::alphabet::AlphabetPlugin;

fn main() {
    crash_handler::install();

    let flags = LaunchFlags::from_env_args();
    if flags.show_help {
        LaunchFlags::print_help();
        return;
    }
    if flags.test_crash {
        crash_handler::trigger_test_crash();
    }

    let started_from_cli = flags.has_play_flags();
    let app_scene = AppScene {
        current: if started_from_cli {
            ActiveScene::Play
        } else {
            ActiveScene::Title
        },
        started_from_cli,
        pending_launch: None,
        exit_to_menu_requested: false,
        play_hud_spawned: false,
        options: MenuGameplayOptions {
            downscroll: flags.downscroll.unwrap_or(false),
            ghost_tap: true,
            botplay: flags.bot_play_mode,
            practice_mode: flags.practice_mode,
            no_shaders: flags.no_shaders,
            ..Default::default()
        },
    };

    let vsync = flags.vsync.unwrap_or_else(|| {
        std::env::var("OXIDE_VSYNC")
            .map(|v| v != "0")
            .unwrap_or(true)
    });
    let render_backend = flags.render_backend;
    run_window(
        WindowConfig {
            title: if started_from_cli {
                "Oxide Engine - PlayState".to_string()
            } else {
                "Oxide Engine - Friday Night Funkin'".to_string()
            },
            vsync,
            render_backend,
            ..Default::default()
        },
        move |app| {
            app.insert_resource(app_scene)
                .add_plugins((
                    AnimationsPlugin,
                    AlphabetPlugin,
                    LoadingScreenPlugin,
                    SoundTrayPlugin,
                    MenuSharedPlugin,
                    TitlePlugin,
                    MainMenuPlugin,
                    PlayPlugin::with_flags(flags),
                ));
        },
    );
}
