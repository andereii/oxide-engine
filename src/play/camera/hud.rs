use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::play::scene::PlayState;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

pub const HUD_RENDER_LAYER: usize = 1;

#[inline]
pub fn hud_render_layer() -> RenderLayers {
    RenderLayers::layer(HUD_RENDER_LAYER)
}

#[derive(Component)]
pub struct HudCamera;

#[derive(Component)]
pub struct CameraFlashOverlay {
    pub hud: bool,
}

pub fn spawn_hud_camera(commands: &mut Commands) {
    commands.spawn((
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Msaa::Off,
        hud_render_layer(),
        HudCamera,
    ));

    let overlay_size = Vec2::new(SCREEN_WIDTH * 4.0, SCREEN_HEIGHT * 4.0);
    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.0), overlay_size),
        Transform::from_xyz(0.0, 0.0, 900.0),
        Visibility::Hidden,
        CameraFlashOverlay { hud: false },
    ));
    commands.spawn((
        Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.0), overlay_size),
        Transform::from_xyz(0.0, 0.0, 900.0),
        Visibility::Hidden,
        hud_render_layer(),
        CameraFlashOverlay { hud: true },
    ));
}

pub fn sync_hud_camera(
    state: Res<PlayState>,
    mut hud_cam: Query<(&mut Projection, &mut Transform), With<HudCamera>>,
    mut world_cam: Query<&mut Transform, (With<Camera2d>, Without<HudCamera>)>,
    mut flashes: Query<(&CameraFlashOverlay, &mut Sprite, &mut Visibility)>,
) {
    let target_scale = 1.0 / (state.camera.hud_zoom.max(0.05) as f32);
    let hud_rot = Quat::from_rotation_z(state.camera.hud_angle_deg.to_radians());
    let hud_pos = Vec3::new(
        state.camera.hud_shake_offset[0],
        state.camera.hud_shake_offset[1],
        0.0,
    );
    for (mut projection, mut tf) in &mut hud_cam {
        if let Projection::Orthographic(ref mut ortho) = *projection
            && (ortho.scale - target_scale).abs() > 1e-5
        {
            ortho.scale = target_scale;
        }
        tf.translation = hud_pos;
        tf.rotation = hud_rot;
    }

    let world_rot = Quat::from_rotation_z(state.camera.angle_deg.to_radians());
    let world_pos = Vec3::new(
        state.camera.shake_offset[0],
        state.camera.shake_offset[1],
        0.0,
    );
    for mut tf in &mut world_cam {
        tf.translation = world_pos;
        tf.rotation = world_rot;
    }

    for (overlay, mut sprite, mut vis) in &mut flashes {
        let (timer, dur, color) = if overlay.hud {
            (
                state.camera.hud_flash_timer_s,
                state.camera.hud_flash_duration_s,
                state.camera.hud_flash_color,
            )
        } else {
            (
                state.camera.flash_timer_s,
                state.camera.flash_duration_s,
                state.camera.flash_color,
            )
        };
        if timer > 0.0 && dur > 0.0 {
            let alpha = ((timer / dur) as f32 * color[3]).clamp(0.0, 1.0);
            sprite.color = Color::srgba(color[0], color[1], color[2], alpha);
            if *vis != Visibility::Visible {
                *vis = Visibility::Visible;
            }
        } else if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
}
