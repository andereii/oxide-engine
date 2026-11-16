use bevy::prelude::*;

pub(super) const THROBBER_ASSET_PATH: &str = "ui/loading/throbber.png";
pub(super) const THROBBER_SIZE_PX: f32 = 64.0;
pub(super) const THROBBER_MARGIN_PX: f32 = 24.0;
pub(super) const THROBBER_STEPS_PER_SEC: f32 = 8.0;
pub(super) const THROBBER_ANGLE_SNAPS: u32 = 12;
pub(super) const LOADING_OVERLAY_Z_INDEX: i32 = i32::MAX - 64;

#[derive(Component)]
pub(super) struct LoadingScreenRoot;

#[derive(Component)]
pub(super) struct LoadingThrobberVisual;

#[derive(Resource, Default)]
pub(super) struct LoadingScreenRuntime {
    pub(super) generation: u64,
    pub(super) elapsed_secs: f32,
    pub(super) root_entity: Option<Entity>,
    pub(super) throbber_handle: Option<Handle<Image>>,
}
