use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

pub const SOUNDTRAY_RENDER_LAYER: usize = 31;
pub const SOUNDTRAY_CAMERA_ORDER: isize = 200;

pub(super) const GRAPHIC_SCALE: f32 = 0.30;
pub(super) const TRAY_SCALE: f32 = 2.0;
pub(super) const BAR_COUNT: usize = 10;
pub(super) const BACKING_BAR_OPACITY: f32 = 0.4;

pub(super) const BOX_RAW_WIDTH: f32 = 260.0;
pub(super) const BOX_RAW_HEIGHT: f32 = 146.0;
pub(super) const BAR_RAW_WIDTH: f32 = 203.0;
pub(super) const BAR_RAW_HEIGHT: f32 = 57.0;
pub(super) const BAR_LOCAL_X: f32 = 9.0;
pub(super) const BAR_LOCAL_Y: f32 = 5.0;

pub(super) const BOX_SCALED_WIDTH: f32 = BOX_RAW_WIDTH * GRAPHIC_SCALE;
pub(super) const BOX_SCALED_HEIGHT: f32 = BOX_RAW_HEIGHT * GRAPHIC_SCALE;
pub(super) const TRAY_WORLD_HEIGHT: f32 = BOX_SCALED_HEIGHT * TRAY_SCALE;

pub(super) const TRAY_SHOWN_Y_FLX: f32 = 10.0;
pub(super) const TRAY_HIDDEN_Y_FLX: f32 = -TRAY_WORLD_HEIGHT - 10.0;
pub(super) const TRAY_HIDE_DELAY_S: f32 = 1.0;

pub(super) const LERP_Y_DURATION_S: f32 = 0.768;
pub(super) const LERP_ALPHA_DURATION_S: f32 = 0.307;
pub(super) const LERP_PRECISION: f32 = 0.01;

pub(super) const BOX_ASSET_PATH: &str = "ui/soundtray/volume-box.png";
pub(super) const VOLUME_UP_SOUND_PATH: &str = "ui/soundtray/volume-up.ogg";
pub(super) const VOLUME_DOWN_SOUND_PATH: &str = "ui/soundtray/volume-down.ogg";
pub(super) const VOLUME_MAX_SOUND_PATH: &str = "ui/soundtray/volume-max.ogg";

#[inline]
pub(super) fn soundtray_render_layer() -> RenderLayers {
    RenderLayers::layer(SOUNDTRAY_RENDER_LAYER)
}

#[inline]
pub(super) fn bar_local_center() -> Vec2 {
    let bar_w = BAR_RAW_WIDTH * GRAPHIC_SCALE;
    let bar_h = BAR_RAW_HEIGHT * GRAPHIC_SCALE;
    let cx = (BAR_LOCAL_X + bar_w * 0.5) - BOX_SCALED_WIDTH * 0.5;
    let cy = BOX_SCALED_HEIGHT * 0.5 - (BAR_LOCAL_Y + bar_h * 0.5);
    Vec2::new(cx, cy)
}

#[derive(Component)]
pub struct SoundTrayOverlayCamera;

#[derive(Component)]
pub(super) struct SoundTrayRoot;

#[derive(Component)]
pub(super) struct SoundTrayBg;

#[derive(Component)]
pub(super) struct SoundTrayBackingBar;

#[derive(Component)]
pub(super) struct SoundTrayBar(pub(super) usize);

#[derive(Resource)]
pub(super) struct SoundTrayAssets {
    pub(super) volume_up_sound: Handle<AudioSource>,
    pub(super) volume_down_sound: Handle<AudioSource>,
    pub(super) volume_max_sound: Handle<AudioSource>,
}
