use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::play::scene::PlayState;

pub const HUD_RENDER_LAYER: usize = 1;

#[inline]
pub fn hud_render_layer() -> RenderLayers {
    RenderLayers::layer(HUD_RENDER_LAYER)
}

#[derive(Component)]
pub struct HudCamera;

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
}

pub fn sync_hud_camera(
    state: Res<PlayState>,
    mut hud_cam: Query<&mut Projection, With<HudCamera>>,
) {
    let target_scale = 1.0 / (state.camera.hud_zoom.max(0.05) as f32);
    for mut projection in &mut hud_cam {
        if let Projection::Orthographic(ref mut ortho) = *projection
            && (ortho.scale - target_scale).abs() > 1e-5
        {
            ortho.scale = target_scale;
        }
    }
}
