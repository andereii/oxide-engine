use std::path::PathBuf;

use bevy::asset::io::AssetSourceBuilder;
use bevy::asset::{AssetApp, AssetPlugin};
use bevy::prelude::*;
use bevy::render::RenderPlugin;
use bevy::render::settings::{Backends, WgpuSettings};
use bevy::window::{MonitorSelection, PresentMode, WindowPosition};
use bevy::winit::WinitSettings;

mod fps_counter;
use fps_counter::FpsCounterPlugin;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RenderBackend {
    #[default]
    Auto,
    Vulkan,
    Dx12,
    Gl,
    Metal,
}

#[derive(Debug, Clone)]
pub struct WindowConfig {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
    pub render_backend: RenderBackend,
    pub assets_dir: Option<PathBuf>,
    pub mods_dir: Option<PathBuf>,
}

impl Default for WindowConfig {
    fn default() -> Self {
        Self {
            title: "Oxide Engine".to_string(),
            width: 1280,
            height: 720,
            vsync: false,
            render_backend: RenderBackend::Auto,
            assets_dir: None,
            mods_dir: None,
        }
    }
}

fn resolve_project_subdir(env_var: &str, subdir: &str, require_existing_dir: bool) -> PathBuf {
    if let Some(env_dir) = std::env::var_os(env_var) {
        let path = PathBuf::from(env_dir);
        if path.is_dir() || !require_existing_dir {
            return path.canonicalize().unwrap_or(path);
        }
    }

    let mut candidates = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(subdir));
        if let Some(p) = cwd.parent() {
            candidates.push(p.join(subdir));
            if let Some(gp) = p.parent() {
                candidates.push(gp.join(subdir));
            }
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent();
        for _ in 0..4 {
            if let Some(d) = dir {
                candidates.push(d.join(subdir));
                dir = d.parent();
            }
        }
    }

    for candidate in candidates {
        if candidate.is_dir() {
            return candidate.canonicalize().unwrap_or(candidate);
        }
    }

    PathBuf::from(subdir)
}

pub fn resolve_assets_dir() -> PathBuf {
    resolve_project_subdir("OXIDE_ASSETS_DIR", "assets", true)
}

pub fn resolve_mods_dir() -> PathBuf {
    if let Some(env_dir) = std::env::var_os("OXIDE_MODS_DIR") {
        return PathBuf::from(env_dir);
    }
    if std::env::var_os("OXIDE_ASSETS_DIR").is_some() {
        let assets = resolve_assets_dir();
        if let Some(parent) = assets.parent() {
            let sibling = parent.join("mods");
            if sibling.is_dir() {
                return sibling;
            }
        }
    }
    resolve_project_subdir("OXIDE_MODS_DIR", "mods", false)
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
    let mods_dir = config.mods_dir.unwrap_or_else(resolve_mods_dir);
    let _ = std::fs::create_dir_all(&mods_dir);
    let mods_path = mods_dir.to_string_lossy().into_owned();

    let mut wgpu_settings = WgpuSettings::default();
    match config.render_backend {
        RenderBackend::Auto => {}
        RenderBackend::Vulkan => wgpu_settings.backends = Some(Backends::VULKAN),
        RenderBackend::Dx12 => wgpu_settings.backends = Some(Backends::DX12),
        RenderBackend::Gl => wgpu_settings.backends = Some(Backends::GL),
        RenderBackend::Metal => wgpu_settings.backends = Some(Backends::METAL),
    }

    app.register_asset_source("mods", AssetSourceBuilder::platform_default(&mods_path, None))
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: assets_path,
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: wgpu_settings.into(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: config.title,
                        resolution: (config.width, config.height).into(),
                        position: WindowPosition::Centered(MonitorSelection::Primary),
                        present_mode,
                        ..default()
                    }),
                    ..default()
                })
                .disable::<bevy::gilrs::GilrsPlugin>(),
        )
        .add_plugins(FpsCounterPlugin)
        .add_systems(Startup, spawn_main_camera);

    if !config.vsync {
        // Keep the event loop running continuously even when unfocused so FPS stays uncapped.
        app.insert_resource(WinitSettings::continuous());
    }

    app
}

pub fn run_window(config: WindowConfig, configure: impl FnOnce(&mut App)) {
    let mut app = build_app(config);
    configure(&mut app);
    app.run();
}

fn spawn_main_camera(mut commands: Commands) {
    commands.spawn((Camera2d, Msaa::Off, IsDefaultUiCamera));
}
