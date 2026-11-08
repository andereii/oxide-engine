//! Real winit window for the interactive session.
//!
//! The scene upload path is the same as the headless harness
//! (`LayerSpec` materials + `Quad` meshes); the only differences are the
//! primary window render target and the missing readback buffer. Every frame
//! reads keyboard/window messages, polls the playback clock, steps every
//! complete sim millisecond, then uploads the current scene + HUD layers.

use bevy::input::ButtonState;
use bevy::input::keyboard::KeyboardInput;
use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::sprite_render::AlphaMode2d;
use bevy::window::{PresentMode, Window, WindowCloseRequested, WindowPlugin, WindowResolution};
use bevy::winit::WinitPlugin;

use super::input::direction_for;
use super::session::PlayRuntime;
use super::{PlayConfig, PlayError, QuitReason, WindowConfig};
use crate::render::harness::{build_mesh, placeholder_mesh};
use crate::render::load::PageImage;
use crate::render::scene::{LayerSpec, Quad};

/// One persistent mesh/material entity per draw layer.
#[derive(Resource)]
struct WindowLayers {
    mesh_handles: Vec<Handle<Mesh>>,
    entities: Vec<Entity>,
    /// F6 debug-overlay text entity (top layer, hidden until toggled).
    overlay: Entity,
}

/// Marks the F6 debug-overlay text entity.
#[derive(Component)]
struct DebugOverlayText;

/// Builds the windowed Bevy app. The app (including `DefaultPlugins`, the
/// renderer and the winit event loop) is constructed here but not run;
/// `session::run` starts audio immediately before `app.run()`. If audio fails
/// closed, the typed error returns before the event loop — no window is shown
/// and no audio is substituted, on any host where this construction succeeds.
///
/// `run_on_any_thread` is the private developer-smoke hook: Rust's test
/// harness runs tests on worker threads, so the ignored headed test needs
/// winit's opt-in off-main-thread event loop (Linux/Windows only). Production
/// [`super::PlaySession::run`] always passes `false`.
pub(crate) fn build(
    config: &PlayConfig,
    pages: Vec<PageImage>,
    specs: &[LayerSpec],
    runtime: PlayRuntime,
    run_on_any_thread: bool,
) -> Result<App, PlayError> {
    let title = format!("Oxide Engine - Bopeebo ({})", config.difficulty);
    let window = Window {
        title,
        resolution: WindowResolution::new(WindowConfig::WIDTH, WindowConfig::HEIGHT)
            .with_scale_factor_override(1.0),
        present_mode: PresentMode::AutoVsync,
        resizable: false,
        ..default()
    };

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(window),
                ..default()
            })
            .set(WinitPlugin { run_on_any_thread })
            // Engine decision: keyboard only for this slice. The feature is
            // compiled in by Bevy's default platform set, so disable the
            // plugin explicitly instead of merely not reading it.
            .disable::<bevy::gilrs::GilrsPlugin>(),
    );
    app.insert_resource(ClearColor(Color::BLACK));

    let page_handles: Vec<Handle<Image>> = {
        let mut images = app.world_mut().resource_mut::<Assets<Image>>();
        pages
            .into_iter()
            .map(|page| images.add(page.image))
            .collect()
    };

    let material_handles: Vec<Handle<ColorMaterial>> = {
        let mut materials = app.world_mut().resource_mut::<Assets<ColorMaterial>>();
        specs
            .iter()
            .map(|spec| {
                materials.add(ColorMaterial {
                    color: Color::srgba(spec.color[0], spec.color[1], spec.color[2], spec.color[3]),
                    alpha_mode: AlphaMode2d::Blend,
                    texture: spec.page.map(|index| page_handles[index].clone()),
                    ..default()
                })
            })
            .collect()
    };

    let mesh_handles: Vec<Handle<Mesh>> = {
        let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
        specs
            .iter()
            .map(|_| meshes.add(placeholder_mesh()))
            .collect()
    };

    let mut entities = Vec::with_capacity(specs.len());
    for (index, (mesh, material)) in mesh_handles.iter().zip(material_handles.iter()).enumerate() {
        let entity = app
            .world_mut()
            .spawn((
                Mesh2d(mesh.clone()),
                MeshMaterial2d(material.clone()),
                Transform::from_xyz(0.0, 0.0, index as f32),
                Visibility::Hidden,
            ))
            .id();
        entities.push(entity);
    }

    // Default Camera2d projection at a 1280x720 scale-1 window maps one world
    // unit to one game pixel, matching the headless target.
    app.world_mut().spawn((Camera2d, Msaa::Off));

    // The crate's Bevy feature set (`2d`, no `ui` feature) makes `Text2d` the
    // minimal working top-layer text API (recorded deviation from the UI
    // `Text` suggestion; same default font, no extra feature/manifest change).
    let overlay = app
        .world_mut()
        .spawn((
            Text2d::new(""),
            TextFont {
                font_size: FontSize::Px(18.0),
                ..default()
            },
            TextColor(Color::WHITE),
            Anchor::TOP_LEFT,
            Transform::from_xyz(
                -(WindowConfig::WIDTH as f32) / 2.0 + 8.0,
                WindowConfig::HEIGHT as f32 / 2.0 - 8.0,
                entities.len() as f32 + 1.0,
            ),
            Visibility::Hidden,
            DebugOverlayText,
        ))
        .id();

    app.insert_resource(runtime);
    app.insert_resource(WindowLayers {
        mesh_handles,
        entities,
        overlay,
    });
    app.add_systems(Update, play_frame);

    Ok(app)
}

// Bevy systems take their dependencies as parameters; keeping them explicit
// is simpler than introducing a custom `SystemParam` for one system.
#[allow(clippy::too_many_arguments)]
fn play_frame(
    mut runtime: ResMut<PlayRuntime>,
    layers: Res<WindowLayers>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut visibility: Query<&mut Visibility>,
    mut overlay_text: Query<&mut Text2d, With<DebugOverlayText>>,
    time: Res<Time>,
    mut keyboard: MessageReader<KeyboardInput>,
    mut close: MessageReader<WindowCloseRequested>,
    mut exit: MessageWriter<AppExit>,
) {
    if runtime.done {
        return;
    }
    runtime.record_frame_delta(time.delta_secs_f64() * 1000.0);

    if close.read().next().is_some() {
        runtime.request_terminal(QuitReason::WindowClosed, &mut exit);
        return;
    }

    // Poll before stamping input so every transition uses this frame's
    // conductor position.
    let position = match runtime.poll_audio() {
        Ok(position) => position,
        Err(error) => {
            runtime.request_error(error, &mut exit);
            return;
        }
    };

    for event in keyboard.read() {
        // Auto-repeat is not a real key transition.
        if event.repeat {
            continue;
        }
        match event.state {
            ButtonState::Pressed => match event.key_code {
                KeyCode::Escape => {
                    if let Err(error) = runtime.toggle_pause() {
                        runtime.request_error(error, &mut exit);
                        return;
                    }
                }
                KeyCode::KeyQ => runtime.set_pending_reason(QuitReason::QuitKey),
                // F6 is the playtest debug overlay; toggling does not touch
                // the sim or scene (ADR-0020 addendum 4 item 7).
                KeyCode::F6 => {
                    runtime.toggle_debug_overlay();
                }
                key => {
                    if let Some(direction) = direction_for(key)
                        && let Err(error) = runtime.queue_key(direction, true)
                    {
                        runtime.request_error(error, &mut exit);
                        return;
                    }
                }
            },
            ButtonState::Released => {
                if let Some(direction) = direction_for(event.key_code)
                    && let Err(error) = runtime.queue_key(direction, false)
                {
                    runtime.request_error(error, &mut exit);
                    return;
                }
            }
        }
    }

    if !runtime.has_pending_reason() {
        let started = std::time::Instant::now();
        let advanced = runtime.advance(position);
        runtime.record_sim_step_ms(started.elapsed().as_secs_f64() * 1000.0);
        if let Err(error) = advanced {
            runtime.request_error(error, &mut exit);
            return;
        }
    }
    if !runtime.has_pending_reason() && runtime.song_finished() {
        runtime.set_pending_reason(QuitReason::SongEnd);
    }
    if runtime.frame_limit_reached() {
        runtime.set_pending_reason(QuitReason::FrameLimit);
    }

    let quads = match runtime.draw_layers() {
        Ok(quads) => quads,
        Err(error) => {
            runtime.request_error(error, &mut exit);
            return;
        }
    };
    if let Err(error) = upload_layers(&layers, &mut meshes, &mut visibility, &quads) {
        runtime.request_error(error, &mut exit);
        return;
    }
    update_overlay(&runtime, layers.overlay, &mut overlay_text, &mut visibility);
    runtime.publish_provisional();

    if let Some(reason) = runtime.take_pending_reason() {
        runtime.request_terminal(reason, &mut exit);
    }
}

/// Shows/hides the F6 overlay and refreshes its three lines while active.
/// Reads runtime counters only; no sim/scene mutation.
fn update_overlay(
    runtime: &PlayRuntime,
    entity: Entity,
    text: &mut Query<&mut Text2d, With<DebugOverlayText>>,
    visibility: &mut Query<&mut Visibility>,
) {
    let active = runtime.debug_overlay_active();
    if let Ok(mut visibility) = visibility.get_mut(entity) {
        *visibility = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if !active {
        return;
    }
    if let Ok(mut text) = text.get_mut(entity) {
        let value = runtime.overlay_text();
        if text.0 != value {
            text.0 = value;
        }
    }
}

fn upload_layers(
    layers: &WindowLayers,
    meshes: &mut Assets<Mesh>,
    visibility: &mut Query<&mut Visibility>,
    quads: &[Vec<Quad>],
) -> Result<(), PlayError> {
    if quads.len() != layers.mesh_handles.len() {
        return Err(PlayError::Session {
            detail: format!(
                "session produced {} layers but the window owns {}",
                quads.len(),
                layers.mesh_handles.len()
            ),
        });
    }

    for (handle, layer) in layers.mesh_handles.iter().zip(quads.iter()) {
        if layer.is_empty() {
            continue;
        }
        meshes
            .insert(handle, build_mesh(layer))
            .map_err(|error| PlayError::Session {
                detail: format!("could not update layer mesh: {error}"),
            })?;
    }

    for (entity, layer) in layers.entities.iter().zip(quads.iter()) {
        let mut visibility = visibility
            .get_mut(*entity)
            .map_err(|error| PlayError::Session {
                detail: format!("layer entity lost its Visibility component: {error:?}"),
            })?;
        *visibility = if layer.is_empty() {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    Ok(())
}
