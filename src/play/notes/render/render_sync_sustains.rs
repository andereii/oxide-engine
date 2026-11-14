use bevy::prelude::*;

use crate::play::notes::data::NoteSprite;
use crate::play::notes::skin::NoteStyle;
use crate::play::notes::strum::Strumline;
use crate::utils::ecs::{
    set_atlas_index, set_sprite_color, set_sprite_custom_size, set_sprite_flip,
    set_transform_pos_scale, set_visibility,
};
use super::render_sync::PoolSyncScratch;
use super::sustain_trail::compute_sustain_geometry;
use super::visuals::{NoteVisual, ReceptorVisual, SustainBodyVisual, SustainEndVisual};

pub(super) fn sync_sustain_body_pool(
    strum: &Strumline,
    is_player: bool,
    style: &NoteStyle,
    sustain_indices: [(usize, usize, f32, f32); 4],
    scratch: &mut PoolSyncScratch,
    bodies: &mut Query<
        (Entity, &mut SustainBodyVisual, &mut Sprite, &mut Transform, &mut Visibility),
        (Without<ReceptorVisual>, Without<NoteVisual>),
    >,
) {
    scratch.free_entities.clear();
    scratch.assigned_indices.clear();
    let start = strum.active_start_index;
    let end = strum.next_note_index.min(strum.notes.len());
    let vis_pos = strum.visual_song_position;

    for (entity, mut vis, mut sprite, mut transform, mut visibility) in bodies.iter_mut() {
        if vis.is_player != is_player {
            continue;
        }
        let valid = vis.assigned_idx.and_then(|idx| {
            (idx >= start && idx < end)
                .then(|| strum.notes.get(idx).map(|n| (idx, n)))
                .flatten()
                .filter(|(_, n)| n.data.id == vis.assigned_id && n.is_hold_alive())
        });
        let Some((idx, note)) = valid else {
            vis.assigned_idx = None;
            set_visibility(&mut visibility, Visibility::Hidden);
            scratch.free_entities.push(entity);
            continue;
        };
        scratch.assigned_indices.push(idx);
        apply_body_geom(strum, style, vis_pos, note, &vis, &mut sprite, &mut transform, &mut visibility);
    }

    for idx in start..end {
        let note = &strum.notes[idx];
        if !note.is_hold_alive() || scratch.assigned_indices.contains(&idx) {
            continue;
        }
        let Some(entity) = scratch.free_entities.pop() else {
            break;
        };
        if let Ok((_, mut vis, mut sprite, mut transform, mut visibility)) = bodies.get_mut(entity) {
            let lane = note.lane.index();
            let (body_idx, _, raw_w, end_h) = sustain_indices[lane];
            vis.assigned_idx = Some(idx);
            vis.assigned_id = note.data.id;
            vis.lane = lane;
            vis.raw_width_px = raw_w;
            vis.end_raw_h = end_h;
            set_atlas_index(&mut sprite, body_idx);
            apply_body_geom(strum, style, vis_pos, note, &vis, &mut sprite, &mut transform, &mut visibility);
        }
    }
}

fn apply_body_geom(
    strum: &Strumline,
    style: &NoteStyle,
    vis_pos: f64,
    note: &NoteSprite,
    vis: &SustainBodyVisual,
    sprite: &mut Mut<'_, Sprite>,
    transform: &mut Mut<'_, Transform>,
    visibility: &mut Mut<'_, Visibility>,
) {
    let geom = compute_sustain_geometry(note, strum, style, vis_pos, vis.end_raw_h);
    if !geom.visible || geom.body_height_px <= 0.5 {
        set_visibility(visibility, Visibility::Hidden);
        return;
    }
    let x_off = if note.hold_hit { style.sustains_offset_hit[0] } else { style.sustains_offset[0] };
    set_visibility(visibility, Visibility::Visible);
    let (z, scale) = { let t: &Mut<'_, Transform> = &*transform; (t.translation.z, t.scale) };
    set_transform_pos_scale(
        transform,
        Vec3::new(strum.lane_x(vis.lane) + x_off, geom.body_y, z),
        scale,
    );
    let flip_x = { let s: &Mut<'_, Sprite> = &*sprite; s.flip_x };
    set_sprite_flip(sprite, flip_x, geom.flip_y);
    set_sprite_color(sprite, Color::srgba(1.0, 1.0, 1.0, geom.alpha));
    set_sprite_custom_size(
        sprite,
        Some(Vec2::new(
            vis.raw_width_px,
            (geom.body_height_px / style.sustains_scale.max(0.01)).max(1.0),
        )),
    );
}

pub(super) fn sync_sustain_end_pool(
    strum: &Strumline,
    is_player: bool,
    style: &NoteStyle,
    sustain_indices: [(usize, usize, f32, f32); 4],
    scratch: &mut PoolSyncScratch,
    ends: &mut Query<
        (Entity, &mut SustainEndVisual, &mut Sprite, &mut Transform, &mut Visibility),
        (Without<ReceptorVisual>, Without<NoteVisual>, Without<SustainBodyVisual>),
    >,
) {
    scratch.free_entities.clear();
    scratch.assigned_indices.clear();
    let start = strum.active_start_index;
    let end = strum.next_note_index.min(strum.notes.len());
    let vis_pos = strum.visual_song_position;

    for (entity, mut vis, mut sprite, mut transform, mut visibility) in ends.iter_mut() {
        if vis.is_player != is_player {
            continue;
        }
        let valid = vis.assigned_idx.and_then(|idx| {
            (idx >= start && idx < end)
                .then(|| strum.notes.get(idx).map(|n| (idx, n)))
                .flatten()
                .filter(|(_, n)| n.data.id == vis.assigned_id && n.is_hold_alive())
        });
        let Some((idx, note)) = valid else {
            vis.assigned_idx = None;
            set_visibility(&mut visibility, Visibility::Hidden);
            scratch.free_entities.push(entity);
            continue;
        };
        scratch.assigned_indices.push(idx);
        apply_end_geom(strum, style, vis_pos, note, &vis, &mut sprite, &mut transform, &mut visibility);
    }

    for idx in start..end {
        let note = &strum.notes[idx];
        if !note.is_hold_alive() || scratch.assigned_indices.contains(&idx) {
            continue;
        }
        let Some(entity) = scratch.free_entities.pop() else {
            break;
        };
        if let Ok((_, mut vis, mut sprite, mut transform, mut visibility)) = ends.get_mut(entity) {
            let lane = note.lane.index();
            let (_, end_idx, raw_w, end_h) = sustain_indices[lane];
            vis.assigned_idx = Some(idx);
            vis.assigned_id = note.data.id;
            vis.lane = lane;
            vis.end_raw_w = raw_w;
            vis.end_raw_h = end_h;
            set_atlas_index(&mut sprite, end_idx);
            apply_end_geom(strum, style, vis_pos, note, &vis, &mut sprite, &mut transform, &mut visibility);
        }
    }
}

fn apply_end_geom(
    strum: &Strumline,
    style: &NoteStyle,
    vis_pos: f64,
    note: &NoteSprite,
    vis: &SustainEndVisual,
    sprite: &mut Mut<'_, Sprite>,
    transform: &mut Mut<'_, Transform>,
    visibility: &mut Mut<'_, Visibility>,
) {
    let geom = compute_sustain_geometry(note, strum, style, vis_pos, vis.end_raw_h);
    if !geom.visible || geom.body_height_px <= 0.5 {
        set_visibility(visibility, Visibility::Hidden);
        return;
    }
    let x_off = if note.hold_hit { style.sustains_offset_hit[0] } else { style.sustains_offset[0] };
    set_visibility(visibility, Visibility::Visible);
    let (z, scale) = { let t: &Mut<'_, Transform> = &*transform; (t.translation.z, t.scale) };
    set_transform_pos_scale(
        transform,
        Vec3::new(strum.lane_x(vis.lane) + x_off + geom.end_x_offset, geom.end_y, z),
        scale,
    );
    let flip_x = { let s: &Mut<'_, Sprite> = &*sprite; s.flip_x };
    set_sprite_flip(sprite, flip_x, geom.flip_y);
    set_sprite_color(sprite, Color::srgba(1.0, 1.0, 1.0, geom.alpha));
    set_sprite_custom_size(
        sprite,
        if geom.end_height_px + 0.5 < vis.end_raw_h * style.sustains_scale {
            Some(Vec2::new(
                vis.end_raw_w,
                (geom.end_height_px / style.sustains_scale.max(0.01)).max(1.0),
            ))
        } else {
            None
        },
    );
}

