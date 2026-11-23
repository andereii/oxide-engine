use bevy::prelude::*;

use super::super::behavior::CharacterActor;
use super::super::schema::CharacterRole;
use super::render::{ensure_pixel_sampler, sync_actor_visuals};
use super::{CharPartsQuery, CharTrailQuery};

#[derive(Clone)]
pub(super) struct ParentMirrorState {
    pub(super) anim_name: Option<String>,
    pub(super) frame_cursor: usize,
    pub(super) reversed: bool,
    pub(super) position: [f32; 2],
    pub(super) scale: [f32; 2],
    pub(super) flip: [bool; 2],
    pub(super) scroll_factor: [f32; 2],
    pub(super) visible: bool,
    pub(super) alpha: f32,
    pub(super) data_offsets: [f32; 2],
    pub(super) idle_size: [f32; 2],
    pub(super) facing_flip_x: bool,
}

#[inline]
pub(super) fn role_index(role: CharacterRole) -> Option<usize> {
    role.primary_slot_index()
}

#[inline]
pub(super) fn compute_underlay_alpha(parent_alpha: f32) -> f32 {
    if parent_alpha < 0.999 { 1.0 } else { 0.0 }
}

pub(super) fn capture_mirror_state(actor: &CharacterActor) -> ParentMirrorState {
    ParentMirrorState {
        anim_name: actor.controller.current_name().map(str::to_string),
        frame_cursor: actor.controller.frame_cursor,
        reversed: actor.controller.reversed,
        position: actor.slot.position,
        scale: actor.slot.scale,
        flip: actor.slot.flip,
        scroll_factor: actor.slot.scroll_factor,
        visible: actor.slot.visible && actor.mod_visible,
        alpha: (actor.slot.opacity * actor.mod_tint[3]).clamp(0.0, 1.0),
        data_offsets: actor.data.offsets,
        idle_size: actor.idle_size,
        facing_flip_x: actor.facing_flip_x,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_underlay_actors(
    mirror_states: &[Option<ParentMirrorState>; 3],
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    images: &mut Assets<Image>,
    actors: &mut Query<&mut CharacterActor>,
    parts: &mut CharPartsQuery,
    trails: &mut CharTrailQuery,
) {
    for mut actor in actors.iter_mut() {
        let Some(cfg) = actor.mirror_of else {
            continue;
        };
        let Some(ref parent) = role_index(cfg.parent_role).and_then(|idx| mirror_states[idx].as_ref()) else {
            actor.mod_visible = false;
            sync_actor_visuals(
                &mut actor,
                0.0,
                cam_scroll_x,
                cam_scroll_y,
                zoom,
                parts,
                trails,
            );
            continue;
        };
        ensure_pixel_sampler(&mut actor, images);

        actor.slot.position = [
            parent.position[0] + cfg.offset[0],
            parent.position[1] + cfg.offset[1],
        ];
        actor.slot.scale = parent.scale;
        actor.slot.flip = parent.flip;
        actor.slot.scroll_factor = parent.scroll_factor;
        actor.slot.visible = parent.visible;
        actor.slot.opacity = 1.0;
        actor.data.offsets = parent.data_offsets;
        actor.idle_size = parent.idle_size;
        actor.facing_flip_x = parent.facing_flip_x;

        let underlay_alpha = compute_underlay_alpha(parent.alpha);
        actor.mod_tint[3] = underlay_alpha;
        actor.mod_visible = parent.visible && underlay_alpha > 0.0005;

        if let Some(ref target_anim) = parent.anim_name {
            let resolved = actor.controller.resolve_name(target_anim).or_else(|| {
                if target_anim == "idle" {
                    actor
                        .controller
                        .resolve_name("danceRight")
                        .or_else(|| actor.controller.resolve_name("danceLeft"))
                } else if target_anim.starts_with("dance") {
                    actor.controller.resolve_name("idle")
                } else {
                    None
                }
            });
            if let Some(anim_key) = resolved {
                let needs_switch = actor.controller.current_name() != Some(anim_key.as_str())
                    || actor.controller.reversed != parent.reversed;
                if needs_switch {
                    actor
                        .controller
                        .play(&anim_key, true, parent.reversed, parent.frame_cursor);
                } else if let Some(anim_def) = actor.controller.animations.get(&anim_key) {
                    let max_f = anim_def.frames.len().saturating_sub(1);
                    actor.controller.frame_cursor = parent.frame_cursor.min(max_f);
                }
            }
        }

        sync_actor_visuals(
            &mut actor,
            0.0,
            cam_scroll_x,
            cam_scroll_y,
            zoom,
            parts,
            trails,
        );
    }
}
