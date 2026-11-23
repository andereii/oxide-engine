mod components;
mod systems;

use bevy::prelude::*;

pub use components::{PauseOverlayCamera, PauseSubSceneState};

pub struct PauseSubScenePlugin;

impl Plugin for PauseSubScenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PauseSubSceneState>()
            .add_systems(Update, systems::sync_pause_subscene)
            .add_systems(
                Update,
                (
                    systems::handle_pause_input,
                    systems::animate_pause_subscene,
                )
                    .chain()
                    .after(systems::sync_pause_subscene)
                    .run_if(crate::ui::in_play_scene),
            );
    }
}
