use std::path::PathBuf;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::window::PresentMode;

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub assets_dir: Option<PathBuf>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Oxide Engine".to_string(),
            width: 1280,
            height: 720,
            vsync: true,
            assets_dir: None,
        }
    }
}

pub fn resolve_assets_dir() -> PathBuf {
    if let Some(env_dir) = std::env::var_os("OXIDE_ASSETS_DIR") {
        let path = PathBuf::from(env_dir);
        if path.is_dir() {
            return path.canonicalize().unwrap_or(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("assets"));
        if let Some(p) = cwd.parent() {
            candidates.push(p.join("assets"));
            if let Some(gp) = p.parent() {
                candidates.push(gp.join("assets"));
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        for _ in 0..4 {
            if let Some(d) = dir {
                candidates.push(d.join("assets"));
                dir = d.parent();
            }
        }
    }

    for candidate in candidates {
        if candidate.is_dir() {
            return candidate.canonicalize().unwrap_or(candidate);
        }
    }

    PathBuf::from("assets")
}

pub fn build_app(config: WindowConfig) -> App {
    let mut app = App::new();
    let present_mode = if config.vsync {
        PresentMode::AutoVsync
    } else {
        PresentMode::AutoNoVsync
    };
    let assets_path = config
        .assets_dir
        .unwrap_or_else(resolve_assets_dir)
        .to_string_lossy()
        .into_owned();

    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: assets_path,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: config.title,
                    resolution: (config.width, config.height).into(),
                    present_mode,
                    ..default()
                }),
                ..default()
            }),
    )
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
