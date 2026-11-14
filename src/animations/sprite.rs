use bevy::prelude::*;

use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;

use super::controller::AnimationController;

pub struct AnimationsPlugin;

impl Plugin for AnimationsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, step_animations);
    }
}

pub fn step_animations(
    time: Res<Time>,
    play_state: Option<Res<PlayState>>,
    mut query: Query<(&mut AnimationController, Option<&mut Sprite>)>,
) {
    if play_state
        .as_ref()
        .is_some_and(|s| matches!(s.phase, PlayPhase::Paused(_)))
    {
        return;
    }
    let elapsed_s = time.delta_secs();
    let song_pos_ms = play_state.map(|s| s.conductor.song_position_ms);

    for (mut controller, sprite_opt) in &mut query {
        controller.update(elapsed_s, song_pos_ms);
        if let (Some(idx), Some(mut sprite)) = (
            controller.current_atlas_index(),
            sprite_opt,
        ) {
            if let Some(atlas) = sprite.texture_atlas.as_mut() {
                atlas.index = idx;
            }
        }
    }
}

