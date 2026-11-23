pub mod dialogue;
pub mod video;

use bevy::prelude::*;

pub use dialogue::{DialogueRuntime, DialogueRuntimeCmd};
pub use video::VideoCutsceneState;

pub struct CutscenePlugin;

impl Plugin for CutscenePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DialogueRuntime>()
            .init_resource::<VideoCutsceneState>()
            .add_systems(
                Update,
                (
                    dialogue::tick_dialogue_system,
                    video::tick_video_cutscene_system,
                )
                    .run_if(crate::ui::in_play_scene),
            );
    }
}
