mod components;
mod systems;

use bevy::prelude::*;

pub use components::{PAUSE_CAMERA_ORDER, PAUSE_RENDER_LAYER, PauseOverlayCamera, PauseSubSceneState};

pub struct PauseSubScenePlugin;

impl Plugin for PauseSubScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseSubSceneState>().add_systems(
            Update,
            (
                systems::sync_pause_subscene,
                systems::handle_pause_input,
                systems::animate_pause_subscene,
            )
                .chain(),
        );
    }
}
