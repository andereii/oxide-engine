use bevy::prelude::*;
use bevy::window::PresentMode;

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Oxide Engine".to_string(),
            width: 1280,
            height: 720,
            vsync: true,
        }
    }
}

pub fn build_app(config: WindowConfig) -> App {
    let mut app = App::new();
    let present_mode = if config.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    app.add_plugins(DefaultPlugins.set(WindowPlugin {
        primary_window: Some(Window {
            title: config.title,
            resolution: (config.width, config.height).into(),
            present_mode,
            ..default()
        }),
        ..default()
    }))
    .add_systems(Startup, spawn_main_camera);
    app
}

pub fn run_window(config: WindowConfig, configure: impl FnOnce(&mut App)) {
    let mut app = build_app(config);
    configure(&mut app);
    app.run();
}

fn spawn_main_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

