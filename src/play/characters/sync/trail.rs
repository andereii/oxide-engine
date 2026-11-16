use bevy::prelude::*;

use crate::utils::ecs::set_visibility;

use super::super::behavior::{CharacterActor, CharacterTrailSample};
use super::CharTrailQuery;
use super::render::apply_sheet_sample_to_sprite;

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_character_trail(
    actor: &mut CharacterActor,
    current_sample: Option<CharacterTrailSample>,
    dt_trail_s: f64,
    alpha: f32,
    actor_visible: bool,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    trails: &mut CharTrailQuery,
) {
    if let (Some(sample), Some(trail)) = (current_sample, actor.trail.as_mut())
        && actor_visible
    {
        trail.step_and_capture(dt_trail_s, sample);
    }

    let Some(ref trail) = actor.trail else {
        return;
    };
    for (idx, &trail_ent) in actor.trail_entities.iter().enumerate() {
        let Ok((mut sprite, mut tf, mut vis)) = trails.get_mut(trail_ent) else {
            continue;
        };
        let Some(sample) = trail.samples.get(idx) else {
            set_visibility(&mut vis, Visibility::Hidden);
            continue;
        };
        let ghost_alpha = trail.ghost_alpha(idx, alpha);
        if ghost_alpha <= 0.001 || !actor_visible {
            set_visibility(&mut vis, Visibility::Hidden);
            continue;
        }
        let ghost_tint = Color::srgba(
            actor.mod_tint[0],
            actor.mod_tint[1],
            actor.mod_tint[2],
            ghost_alpha,
        );
        let z = trail.config.z_layer - (idx as f32) * 0.0001;
        apply_sheet_sample_to_sprite(
            actor,
            sample,
            ghost_tint,
            z,
            true,
            cam_scroll_x,
            cam_scroll_y,
            zoom,
            &mut sprite,
            &mut tf,
            &mut vis,
        );
    }
}
