use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;

use super::character::LoadedCharacterVisual;

pub(super) const LOADING_RENDER_LAYER: usize = 30;
pub(super) const LOADING_CAMERA_ORDER: isize = 100;

pub(super) const MIC_STAND_ASSET_PATH: &str = "ui/loading/mic-stand.png";
pub(super) const MIC_STAND_SCALE: f32 = 0.34;
pub(super) const MIC_STAND_MARGIN_RIGHT: f32 = 45.0;
pub(super) const MIC_STAND_MARGIN_BOTTOM: f32 = 30.0;
pub(super) const BAR_PAD: f32 = 16.0;
pub(super) const BAR_ALPHA: f32 = 0.9;
pub(super) const BAR_LERP_SPEED: f32 = 12.0;

pub(super) const BG_Z: f32 = 0.0;
pub(super) const MIC_STAND_Z: f32 = 100.0;
pub(super) const BAR_Z: f32 = 200.0;
pub(super) const CHARACTER_Z: f32 = 300.0;

#[inline]
pub(super) fn loading_render_layer() -> RenderLayers {
    RenderLayers::layer(LOADING_RENDER_LAYER)
}

#[derive(Component)]
pub struct LoadingOverlayCamera;

#[derive(Component)]
pub(super) struct LoadingBgVisual;

#[derive(Component)]
pub(super) struct LoadingMicStandVisual;

#[derive(Component)]
pub(super) struct LoadingProgressBarVisual;

#[derive(Component)]
pub(super) struct LoadingCharacterVisual {
    pub(super) scale: f32,
    pub(super) base_center_x: f32,
    pub(super) base_center_y: f32,
}

#[derive(Resource, Default)]
pub(super) struct LoadingScreenRuntime {
    pub(super) generation: u64,
    pub(super) camera_entity: Option<Entity>,
    pub(super) bg_entity: Option<Entity>,
    pub(super) mic_entity: Option<Entity>,
    pub(super) bar_entity: Option<Entity>,
    pub(super) char_entity: Option<Entity>,
    pub(super) mic_handle: Option<Handle<Image>>,
    pub(super) mic_layout_done: bool,
    pub(super) mic_width: f32,
    pub(super) bar_base_x: f32,
    pub(super) bar_current_x: f32,
    pub(super) cached_char: Option<LoadedCharacterVisual>,
}
