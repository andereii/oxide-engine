use std::path::PathBuf;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::time::Real;
use bevy::window::PresentMode;
use bevy::winit::WinitSettings;

const FPS_REFRESH_INTERVAL_S: f64 = 0.25;
const FPS_OVERLAY_Z_INDEX: i32 = i32::MAX - 32;

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
            vsync: false,
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
            })
            .disable::<bevy::gilrs::GilrsPlugin>(),
    )
    .add_systems(Startup, (spawn_main_camera, spawn_fps_counter))
    .add_systems(Update, update_fps_counter);

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

#[derive(Component)]
struct FpsCounterText;

#[derive(Default)]
struct FpsCounterState {
    frames: u32,
    elapsed_s: f64,
}

fn spawn_main_camera(mut commands: Commands) {
    commands.spawn((Camera2d, Msaa::Off));
}

/// Spawns a top-left screen-space FPS overlay above gameplay and HUD layers.
fn spawn_fps_counter(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(FPS_OVERLAY_Z_INDEX),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("FPS: --"),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextShadow {
                    offset: Vec2::splat(1.5),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.9),
                },
                FpsCounterText,
                Pickable::IGNORE,
            ));
        });
}

fn format_fps(fps: f64) -> String {
    if !fps.is_finite() {
        return "FPS: --".to_string();
    }
    format!("FPS: {:.0}", fps.round().max(0.0))
}

/// Updates the on-screen FPS counter over fixed wall-clock windows to avoid jitter and excess UI relayout.
fn update_fps_counter(
    time: Res<Time<Real>>,
    mut state: Local<FpsCounterState>,
    mut query: Query<&mut Text, With<FpsCounterText>>,
) {
    let delta_s = time.delta_secs_f64();
    if delta_s <= 0.0 {
        return;
    }

    state.frames = state.frames.saturating_add(1);
    state.elapsed_s += delta_s;
    if state.elapsed_s < FPS_REFRESH_INTERVAL_S {
        return;
    }

    let fps = f64::from(state.frames) / state.elapsed_s;
    let label = format_fps(fps);
    for mut text in &mut query {
        **text = label.clone();
    }
    state.frames = 0;
    state.elapsed_s = 0.0;
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use super::*;

    #[test]
    fn window_config_defaults_to_uncapped_vsync() {
        assert!(!WindowConfig::default().vsync);
    }

    #[test]
    fn format_fps_rounds_and_clamps_invalid_values() {
        assert_eq!(format_fps(143.6), "FPS: 144");
        assert_eq!(format_fps(0.0), "FPS: 0");
        assert_eq!(format_fps(-10.0), "FPS: 0");
        assert_eq!(format_fps(f64::NAN), "FPS: --");
        assert_eq!(format_fps(f64::INFINITY), "FPS: --");
    }

    #[test]
    fn update_fps_counter_updates_text_after_refresh_interval() {
        let mut app = App::new();
        app.insert_resource(Time::<Real>::default());
        app.add_systems(Update, update_fps_counter);

        let entity = app
            .world_mut()
            .spawn((Text::new("FPS: --"), FpsCounterText))
            .id();

        // First real-time update establishes baseline (delta == 0).
        app.world_mut()
            .resource_mut::<Time<Real>>()
            .update_with_duration(Duration::ZERO);
        app.update();
        assert_eq!(
            app.world().entity(entity).get::<Text>().unwrap().0,
            "FPS: --"
        );

        // Two 125ms frames reach the 250ms window: 2 / 0.25s = 8 FPS.
        for _ in 0..2 {
            app.world_mut()
                .resource_mut::<Time<Real>>()
                .update_with_duration(Duration::from_millis(125));
            app.update();
        }
        assert_eq!(
            app.world().entity(entity).get::<Text>().unwrap().0,
            "FPS: 8"
        );
    }
}
