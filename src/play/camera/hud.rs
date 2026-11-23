use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use crate::play::scene::PlayState;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

pub const HUD_RENDER_LAYER: usize = 1;
pub const OVERLAY_RENDER_LAYER: usize = 2;

#[inline]
pub fn hud_render_layer() -> RenderLayers {
    RenderLayers::layer(HUD_RENDER_LAYER)
}

#[inline]
pub fn overlay_render_layer() -> RenderLayers {
    RenderLayers::layer(OVERLAY_RENDER_LAYER)
}

#[derive(Component)]
pub struct HudCamera;

#[derive(Component)]
pub struct OverlayCamera;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlashOverlayLayer {
    Game,
    Hud,
    Overlay,
}

#[derive(Component)]
pub struct CameraFlashOverlay {
    pub layer: FlashOverlayLayer,
    pub is_fade: bool,
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
    commands.spawn((
        Camera2d,
        Camera {
            order: 2,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Msaa::Off,
        overlay_render_layer(),
        OverlayCamera,
    ));

    // Render all camera flash/fade overlays:
    // - World (`Game`) sits on `hud_render_layer()` at negative Z below HUD elements yet after
    //   `StageCamera` (`order = 0`) post-processing, so stage fades cover stage overlay shaders.
    // - HUD (`Hud`) sits at Z = 900..901 above HUD elements.
    // - Overlay/cutscene (`Overlay`) renders on `overlay_render_layer()` (`order = 2`), always
    //   unzoomed and independent of `hud_visible(false)`.
    let overlay_size = Vec2::new(SCREEN_WIDTH * 6.0, SCREEN_HEIGHT * 6.0);
    for (layer, render_layer, is_fade, z) in [
        (FlashOverlayLayer::Game, hud_render_layer(), false, -500.0),
        (FlashOverlayLayer::Game, hud_render_layer(), true, -499.0),
        (FlashOverlayLayer::Hud, hud_render_layer(), false, 900.0),
        (FlashOverlayLayer::Hud, hud_render_layer(), true, 901.0),
        (FlashOverlayLayer::Overlay, overlay_render_layer(), false, 900.0),
        (FlashOverlayLayer::Overlay, overlay_render_layer(), true, 901.0),
    ] {
        commands.spawn((
            Sprite::from_color(Color::srgba(1.0, 1.0, 1.0, 0.0), overlay_size),
            Transform::from_xyz(0.0, 0.0, z),
            Visibility::Hidden,
            render_layer,
            CameraFlashOverlay { layer, is_fade },
        ));
    }
}

/// Evaluates the current overlay alpha for a flash or fade effect.
///
/// Flashes start at `color[3]` and decay to `0.0`.
/// Fades match Flixel semantics: `fade_in = true` reveals from `color[3]` -> `0.0` then hides,
/// while `fade_in = false` fades `0.0` -> `color[3]` and holds opaque when `hold` is true.
fn evaluate_overlay_color(
    is_fade: bool,
    timer: f64,
    dur: f64,
    color: [f32; 4],
    fade_in: bool,
    hold: bool,
) -> Option<Color> {
    let t = if !is_fade {
        (timer > 0.0 && dur > 0.0).then(|| (timer / dur) as f32)?
    } else if timer > 0.0 && dur > 0.0 {
        let progress = (1.0 - (timer / dur)) as f32;
        if fade_in { 1.0 - progress } else { progress }
    } else if hold {
        1.0
    } else {
        return None;
    };
    let alpha = (t * color[3]).clamp(0.0, 1.0);
    Some(Color::srgba(color[0], color[1], color[2], alpha))
}

pub fn sync_hud_camera(
    state: Res<PlayState>,
    mut hud_cam: Query<(&mut Projection, &mut Transform), With<HudCamera>>,
    mut world_cam: Query<
        &mut Transform,
        (
            With<Camera2d>,
            With<IsDefaultUiCamera>,
            Without<HudCamera>,
            Without<OverlayCamera>,
        ),
    >,
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
        let color_opt = match (overlay.layer, overlay.is_fade) {
            (FlashOverlayLayer::Hud, false) => evaluate_overlay_color(
                false,
                state.camera.hud_flash_timer_s,
                state.camera.hud_flash_duration_s,
                state.camera.hud_flash_color,
                false,
                false,
            ),
            (FlashOverlayLayer::Game, false) => evaluate_overlay_color(
                false,
                state.camera.flash_timer_s,
                state.camera.flash_duration_s,
                state.camera.flash_color,
                false,
                false,
            ),
            (FlashOverlayLayer::Overlay, false) => evaluate_overlay_color(
                false,
                state.camera.overlay_flash_timer_s,
                state.camera.overlay_flash_duration_s,
                state.camera.overlay_flash_color,
                false,
                false,
            ),
            (FlashOverlayLayer::Hud, true) => evaluate_overlay_color(
                true,
                state.camera.hud_fade_timer_s,
                state.camera.hud_fade_duration_s,
                state.camera.hud_fade_color,
                state.camera.hud_fade_in,
                state.camera.hud_fade_hold,
            ),
            (FlashOverlayLayer::Game, true) => evaluate_overlay_color(
                true,
                state.camera.fade_timer_s,
                state.camera.fade_duration_s,
                state.camera.fade_color,
                state.camera.fade_in,
                state.camera.fade_hold,
            ),
            (FlashOverlayLayer::Overlay, true) => evaluate_overlay_color(
                true,
                state.camera.overlay_fade_timer_s,
                state.camera.overlay_fade_duration_s,
                state.camera.overlay_fade_color,
                state.camera.overlay_fade_in,
                state.camera.overlay_fade_hold,
            ),
        };
        if let Some(color) = color_opt {
            sprite.color = color;
            if *vis != Visibility::Visible {
                *vis = Visibility::Visible;
            }
        } else if *vis != Visibility::Hidden {
            *vis = Visibility::Hidden;
        }
    }
}

#[cfg(test)]
mod tests {
    use bevy::color::Alpha;

    use super::evaluate_overlay_color;

    #[test]
    fn fade_in_reveals_from_opaque_and_hides_when_complete() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let start = evaluate_overlay_color(true, 2.0, 2.0, black, true, false)
            .expect("fade_in start should be visible");
        assert!((start.alpha() - 1.0).abs() < 1e-5);

        let mid = evaluate_overlay_color(true, 0.5, 2.0, black, true, false)
            .expect("fade_in midpoint should be visible");
        assert!((mid.alpha() - 0.25).abs() < 1e-5);

        assert!(evaluate_overlay_color(true, 0.0, 2.0, black, true, false).is_none());
    }

    #[test]
    fn fade_out_builds_to_opaque_and_holds_color() {
        let black = [0.0, 0.0, 0.0, 1.0];
        let start = evaluate_overlay_color(true, 2.0, 2.0, black, false, true)
            .expect("fade_out start should be visible");
        assert!(start.alpha().abs() < 1e-5);

        let mid = evaluate_overlay_color(true, 0.5, 2.0, black, false, true)
            .expect("fade_out midpoint should be visible");
        assert!((mid.alpha() - 0.75).abs() < 1e-5);

        let held = evaluate_overlay_color(true, 0.0, 2.0, black, false, true)
            .expect("completed fade_out with hold=true should remain opaque");
        assert!((held.alpha() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn flash_decays_to_zero_and_hides() {
        let white = [1.0, 1.0, 1.0, 0.8];
        let start = evaluate_overlay_color(false, 1.0, 1.0, white, false, false)
            .expect("flash start should be visible");
        assert!((start.alpha() - 0.8).abs() < 1e-5);
        assert!(evaluate_overlay_color(false, 0.0, 1.0, white, false, false).is_none());
    }
}

