use std::collections::HashMap;

use bevy::audio::{AudioPlayer, AudioSink, AudioSinkPlayback, PlaybackMode, PlaybackSettings, Volume};
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::input::{Action, Controls};
use crate::mods::ModRegistry;
use crate::utils::constants::SCREEN_HEIGHT;

use super::SoundTrayState;
use super::components::{
    BACKING_BAR_OPACITY, BAR_COUNT, BOX_ASSET_PATH, GRAPHIC_SCALE, LERP_ALPHA_DURATION_S,
    LERP_PRECISION, LERP_Y_DURATION_S, SOUNDTRAY_CAMERA_ORDER, SoundTrayAssets,
    SoundTrayBackingBar, SoundTrayBar, SoundTrayBg, SoundTrayOverlayCamera, SoundTrayRoot,
    TRAY_HIDDEN_Y_FLX, TRAY_SCALE, TRAY_WORLD_HEIGHT, VOLUME_DOWN_SOUND_PATH,
    VOLUME_MAX_SOUND_PATH, VOLUME_UP_SOUND_PATH, bar_local_center, soundtray_render_layer,
};
use super::persistence::save_sound_preferences;

const FOCUS_LOST_VOLUME: f32 = 0.2;
const FOCUS_FADE_SPEED: f32 = 6.0;
const VOLUME_STEP: f32 = 0.1;

fn resolve_ui_asset(mods: Option<&ModRegistry>, rel: &str) -> String {
    mods.and_then(|m| m.resolve_asset_server_path(rel))
        .unwrap_or_else(|| rel.to_string())
}

fn smooth_lerp_precision(
    base: f32,
    target: f32,
    dt_s: f32,
    duration_s: f32,
    precision: f32,
) -> f32 {
    if dt_s <= 0.0 || (base - target).abs() <= f32::EPSILON {
        return base;
    }
    let alpha = precision.powf(dt_s / duration_s);
    target + (base - target) * alpha
}

#[inline]
fn flx_y_to_world_y(flx_top_y: f32) -> f32 {
    SCREEN_HEIGHT * 0.5 - (flx_top_y + TRAY_WORLD_HEIGHT * 0.5)
}

pub(super) fn setup_soundtray(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mods: Option<Res<ModRegistry>>,
    mut state: ResMut<SoundTrayState>,
) {
    let layer = soundtray_render_layer();
    let mods_ref = mods.as_deref();

    commands.spawn((
        Camera2d,
        Camera {
            order: SOUNDTRAY_CAMERA_ORDER,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Msaa::Off,
        layer.clone(),
        SoundTrayOverlayCamera,
    ));

    let box_handle: Handle<Image> = asset_server.load(resolve_ui_asset(mods_ref, BOX_ASSET_PATH));
    let bar_handles: Vec<Handle<Image>> = (1..=BAR_COUNT)
        .map(|i| {
            let rel = format!("ui/soundtray/bars-{i:02}.png");
            asset_server.load(resolve_ui_asset(mods_ref, &rel))
        })
        .collect();
    let backing_handle = bar_handles[BAR_COUNT - 1].clone();

    let bar_center = bar_local_center();
    let initial_world_y = flx_y_to_world_y(state.current_y);
    let initial_vis = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    commands
        .spawn((
            Transform::from_xyz(0.0, initial_world_y, 0.0).with_scale(Vec3::splat(TRAY_SCALE)),
            initial_vis,
            layer.clone(),
            SoundTrayRoot,
        ))
        .with_children(|parent| {
            let mut bg_sprite = Sprite::from_image(box_handle);
            bg_sprite.color = Color::srgba(1.0, 1.0, 1.0, state.current_alpha);
            parent.spawn((
                bg_sprite,
                Transform::from_xyz(0.0, 0.0, 0.0).with_scale(Vec3::splat(GRAPHIC_SCALE)),
                layer.clone(),
                SoundTrayBg,
            ));

            let mut backing_sprite = Sprite::from_image(backing_handle);
            backing_sprite.color =
                Color::srgba(1.0, 1.0, 1.0, BACKING_BAR_OPACITY * state.current_alpha);
            parent.spawn((
                backing_sprite,
                Transform::from_xyz(bar_center.x, bar_center.y, 1.0)
                    .with_scale(Vec3::splat(GRAPHIC_SCALE)),
                layer.clone(),
                SoundTrayBackingBar,
            ));

            let active_bars = state.active_bars();
            for (idx, handle) in bar_handles.into_iter().enumerate() {
                let mut bar_sprite = Sprite::from_image(handle);
                bar_sprite.color = Color::srgba(1.0, 1.0, 1.0, state.current_alpha);
                let bar_vis = if idx < active_bars {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                parent.spawn((
                    bar_sprite,
                    Transform::from_xyz(bar_center.x, bar_center.y, 2.0 + idx as f32 * 0.01)
                        .with_scale(Vec3::splat(GRAPHIC_SCALE)),
                    bar_vis,
                    layer.clone(),
                    SoundTrayBar(idx),
                ));
            }
        });

    commands.insert_resource(SoundTrayAssets {
        volume_up_sound: asset_server.load(resolve_ui_asset(mods_ref, VOLUME_UP_SOUND_PATH)),
        volume_down_sound: asset_server.load(resolve_ui_asset(mods_ref, VOLUME_DOWN_SOUND_PATH)),
        volume_max_sound: asset_server.load(resolve_ui_asset(mods_ref, VOLUME_MAX_SOUND_PATH)),
    });

    if !state.has_audible_volume() {
        state.show_tray();
    }
}

pub(super) fn handle_soundtray_input(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    mut key_events: MessageReader<KeyboardInput>,
    controls: Res<Controls>,
    mut state: ResMut<SoundTrayState>,
    assets: Option<Res<SoundTrayAssets>>,
) {
    let mut char_up = false;
    let mut char_down = false;
    for ev in key_events.read() {
        if ev.state == ButtonState::Pressed
            && !ev.repeat
            && let Key::Character(ref ch) = ev.logical_key
        {
            match ch.as_str() {
                "+" => char_up = true,
                "-" => char_down = true,
                _ => {}
            }
        }
    }

    let mut changed = false;
    let mut direction: Option<bool> = None;

    if controls.just_pressed(&keys, Action::VolumeMute) {
        state.toggle_muted();
        state.show_tray();
        changed = true;
    } else if controls.just_pressed(&keys, Action::VolumeUp) || char_up {
        state.change_volume(VOLUME_STEP);
        state.show_tray();
        changed = true;
        direction = Some(true);
    } else if controls.just_pressed(&keys, Action::VolumeDown) || char_down {
        state.change_volume(-VOLUME_STEP);
        state.show_tray();
        changed = true;
        direction = Some(false);
    }

    if !changed {
        return;
    }

    if let (Some(up), Some(assets)) = (direction, assets.as_deref()) {
        let sound_handle = if state.volume >= 1.0 - f32::EPSILON {
            assets.volume_max_sound.clone()
        } else if up {
            assets.volume_up_sound.clone()
        } else {
            assets.volume_down_sound.clone()
        };
        commands.spawn((
            AudioPlayer(sound_handle),
            PlaybackSettings {
                mode: PlaybackMode::Despawn,
                ..default()
            },
        ));
    }

    save_sound_preferences(state.volume, state.muted);
}

#[allow(clippy::type_complexity)]
pub(super) fn update_soundtray(
    time: Res<Time>,
    mut state: ResMut<SoundTrayState>,
    mut root_query: Query<(&mut Transform, &mut Visibility), With<SoundTrayRoot>>,
    mut bg_query: Query<&mut Sprite, With<SoundTrayBg>>,
    mut backing_query: Query<&mut Sprite, (With<SoundTrayBackingBar>, Without<SoundTrayBg>)>,
    mut bar_query: Query<
        (&SoundTrayBar, &mut Sprite, &mut Visibility),
        (
            Without<SoundTrayRoot>,
            Without<SoundTrayBg>,
            Without<SoundTrayBackingBar>,
        ),
    >,
) {
    let elapsed = time.delta_secs();
    let has_volume = state.has_audible_volume();

    if has_volume {
        if state.timer_s > 0.0 {
            state.timer_s -= elapsed;
            if state.timer_s <= 0.0 {
                state.timer_s = 0.0;
                state.lerp_y_pos = TRAY_HIDDEN_Y_FLX;
                state.alpha_target = 0.0;
            }
        } else if state.current_y <= -TRAY_WORLD_HEIGHT {
            state.visible = false;
        }
    } else if !state.visible {
        state.show_tray();
    }

    state.current_y = smooth_lerp_precision(
        state.current_y,
        state.lerp_y_pos,
        elapsed,
        LERP_Y_DURATION_S,
        LERP_PRECISION,
    );
    state.current_alpha = smooth_lerp_precision(
        state.current_alpha,
        state.alpha_target,
        elapsed,
        LERP_ALPHA_DURATION_S,
        LERP_PRECISION,
    );

    let world_y = flx_y_to_world_y(state.current_y);
    let root_vis = if state.visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for (mut tf, mut vis) in &mut root_query {
        tf.translation.y = world_y;
        if *vis != root_vis {
            *vis = root_vis;
        }
    }

    let alpha = state.current_alpha.clamp(0.0, 1.0);
    for mut sprite in &mut bg_query {
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
    }
    for mut sprite in &mut backing_query {
        sprite.color = Color::srgba(1.0, 1.0, 1.0, BACKING_BAR_OPACITY * alpha);
    }

    let active_bars = state.active_bars();
    for (bar, mut sprite, mut vis) in &mut bar_query {
        sprite.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        let target_vis = if bar.0 < active_bars {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *vis != target_vis {
            *vis = target_vis;
        }
    }
}

#[derive(Default)]
pub(super) struct MasterVolumeCache {
    focus_multiplier: Option<f32>,
    sink_volumes: HashMap<Entity, (f32, f32)>,
}

pub(super) fn sync_master_volume(
    time: Res<Time>,
    state: Res<SoundTrayState>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut sinks: Query<(Entity, &mut AudioSink)>,
    mut cache: Local<MasterVolumeCache>,
) {
    let is_focused = windows.iter().next().map(|w| w.focused).unwrap_or(true);
    let focus_target = if is_focused { 1.0 } else { FOCUS_LOST_VOLUME };
    let focus_cur = cache.focus_multiplier.get_or_insert(1.0);

    if (*focus_cur - focus_target).abs() > 1e-4 {
        let step = time.delta_secs() * FOCUS_FADE_SPEED;
        *focus_cur = if *focus_cur > focus_target {
            (*focus_cur - step).max(focus_target)
        } else {
            (*focus_cur + step).min(focus_target)
        };
    }

    let master = state.effective_volume() * *focus_cur;

    cache
        .sink_volumes
        .retain(|entity, _| sinks.contains(*entity));

    for (entity, mut sink) in &mut sinks {
        let actual = sink.volume().to_linear();
        let entry = cache.sink_volumes.entry(entity).or_insert((actual, actual));
        if (actual - entry.1).abs() > 1e-4 {
            entry.0 = actual;
        }
        let desired = (entry.0 * master).max(0.0);
        if (actual - desired).abs() > 1e-4 {
            sink.set_volume(Volume::Linear(desired));
        }
        entry.1 = desired;
    }
}
