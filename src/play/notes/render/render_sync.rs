use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::scene::PlayState;
use crate::utils::ecs::{set_atlas_index, set_sprite_color, set_transform_pos_scale, set_visibility};

use crate::play::notes::data::NoteSprite;
use crate::play::notes::skin::NoteStyle;
use crate::play::notes::strum::{ReceptorAnimState, Strumline};
use super::render_sync_sustains::{sync_sustain_body_pool, sync_sustain_end_pool};
use super::visuals::{
    NoteVisual, NoteskinAssets, ReceptorVisual, SustainBodyVisual, SustainEndVisual,
};

#[derive(Default)]
pub struct PoolSyncScratch {
    pub(super) free_entities: Vec<Entity>,
    pub(super) assigned_indices: Vec<usize>,
}

fn strumline(state: &PlayState, is_player: bool) -> &Strumline {
    if is_player {
        &state.player_strumline
    } else {
        &state.opponent_strumline
    }
}

pub fn sync_arrow_visuals(
    mut state: ResMut<PlayState>,
    skin_assets: Option<Res<NoteskinAssets>>,
    mut scratch: Local<PoolSyncScratch>,
    mut receptors: Query<(
        &mut ReceptorVisual,
        &mut Sprite,
        &mut Transform,
        &mut AnimationController,
    )>,
    mut notes: Query<
        (Entity, &mut NoteVisual, &mut Sprite, &mut Transform, &mut Visibility),
        Without<ReceptorVisual>,
    >,
    mut sustain_bodies: Query<
        (
            Entity,
            &mut SustainBodyVisual,
            &mut Sprite,
            &mut Transform,
            &mut Visibility,
        ),
        (Without<ReceptorVisual>, Without<NoteVisual>),
    >,
    mut sustain_ends: Query<
        (
            Entity,
            &mut SustainEndVisual,
            &mut Sprite,
            &mut Transform,
            &mut Visibility,
        ),
        (
            Without<ReceptorVisual>,
            Without<NoteVisual>,
            Without<SustainBodyVisual>,
        ),
    >,
) {
    let fallback_style;
    let style = if let Some(s) = skin_assets.as_ref() {
        &s.style
    } else {
        fallback_style = state.player_strumline.note_style.clone();
        &fallback_style
    };
    let note_indices = skin_assets
        .as_ref()
        .map(|s| s.note_indices)
        .unwrap_or([0; 4]);
    let sustain_indices = skin_assets
        .as_ref()
        .map(|s| s.sustain_indices)
        .unwrap_or([(0, 0, 52.0, 44.0); 4]);

    sync_receptors(&mut state, style, &mut receptors);
    for is_player in [false, true] {
        sync_note_pool(
            strumline(&state, is_player),
            is_player,
            style,
            note_indices,
            &mut scratch,
            &mut notes,
        );
        sync_sustain_body_pool(
            strumline(&state, is_player),
            is_player,
            style,
            sustain_indices,
            &mut scratch,
            &mut sustain_bodies,
        );
        sync_sustain_end_pool(
            strumline(&state, is_player),
            is_player,
            style,
            sustain_indices,
            &mut scratch,
            &mut sustain_ends,
        );
    }
}

fn sync_receptors(
    state: &mut PlayState,
    style: &NoteStyle,
    receptors: &mut Query<(
        &mut ReceptorVisual,
        &mut Sprite,
        &mut Transform,
        &mut AnimationController,
    )>,
) {
    for (mut rec, mut sprite, mut transform, mut anim) in receptors.iter_mut() {
        let strum = if rec.is_player {
            &mut state.player_strumline
        } else {
            &mut state.opponent_strumline
        };
        if anim.finished && anim.current.as_deref() == Some("confirm") {
            strum.receptors[rec.lane].on_confirm_anim_finished();
        }
        let rec_state = &strum.receptors[rec.lane];
        let z = { let t: &Transform = &*transform; t.translation.z };
        set_transform_pos_scale(
            &mut transform,
            Vec3::new(strum.lane_x(rec.lane), strum.receptor_y() + rec_state.y_offset, z),
            Vec3::splat(style.strum_scale * strum.strumline_scale),
        );
        set_sprite_color(&mut sprite, Color::srgba(1.0, 1.0, 1.0, style.strum_alpha * rec_state.alpha));

        if rec.last_anim_gen != rec_state.anim_gen {
            rec.last_anim_gen = rec_state.anim_gen;
            let anim_name = match rec_state.anim_state {
                ReceptorAnimState::Confirm => "confirm",
                ReceptorAnimState::ConfirmHold => "confirm-hold",
                ReceptorAnimState::Press => "press",
                ReceptorAnimState::Static => "static",
            };
            anim.play(anim_name, true, false, 0);
            if let Some(idx) = anim.current_atlas_index() {
                set_atlas_index(&mut sprite, idx);
            }
        }
    }
}

fn sync_note_pool(
    strum: &Strumline,
    is_player: bool,
    style: &NoteStyle,
    note_indices: [usize; 4],
    scratch: &mut PoolSyncScratch,
    notes: &mut Query<
        (Entity, &mut NoteVisual, &mut Sprite, &mut Transform, &mut Visibility),
        Without<ReceptorVisual>,
    >,
) {
    scratch.free_entities.clear();
    scratch.assigned_indices.clear();
    let start = strum.active_start_index;
    let end = strum.next_note_index.min(strum.notes.len());
    let vis_pos = strum.visual_song_position;

    for (entity, mut vis, mut sprite, mut transform, mut visibility) in notes.iter_mut() {
        if vis.is_player != is_player {
            continue;
        }
        let valid = vis.assigned_idx.and_then(|idx| {
            (idx >= start && idx < end)
                .then(|| strum.notes.get(idx).map(|n| (idx, n)))
                .flatten()
                .filter(|(_, n)| n.data.id == vis.assigned_id && n.is_head_alive())
        });
        let Some((idx, note)) = valid else {
            vis.assigned_idx = None;
            set_visibility(&mut visibility, Visibility::Hidden);
            scratch.free_entities.push(entity);
            continue;
        };
        scratch.assigned_indices.push(idx);
        apply_note_transform(strum, style, vis_pos, note, &mut sprite, &mut transform, &mut visibility);
    }

    for idx in start..end {
        let note = &strum.notes[idx];
        if !note.is_head_alive() || scratch.assigned_indices.contains(&idx) {
            continue;
        }
        let Some(entity) = scratch.free_entities.pop() else {
            break;
        };
        if let Ok((_, mut vis, mut sprite, mut transform, mut visibility)) = notes.get_mut(entity) {
            vis.assigned_idx = Some(idx);
            vis.assigned_id = note.data.id;
            vis.lane = note.lane.index();
            set_atlas_index(&mut sprite, note_indices[vis.lane]);
            apply_note_transform(strum, style, vis_pos, note, &mut sprite, &mut transform, &mut visibility);
        }
    }
}

fn apply_note_transform(
    strum: &Strumline,
    style: &NoteStyle,
    vis_pos: f64,
    note: &NoteSprite,
    sprite: &mut Mut<'_, Sprite>,
    transform: &mut Mut<'_, Transform>,
    visibility: &mut Mut<'_, Visibility>,
) {
    let y = strum.note_y_at(note.data.time_ms, vis_pos) + style.notes_offset[1] + note.y_offset;
    if y < -520.0 || y > 520.0 {
        set_visibility(visibility, Visibility::Hidden);
        return;
    }
    set_visibility(visibility, Visibility::Visible);
    let z = { let t: &Mut<'_, Transform> = &*transform; t.translation.z };
    set_transform_pos_scale(
        transform,
        Vec3::new(strum.lane_x(note.lane.index()) + style.notes_offset[0], y, z),
        Vec3::splat(style.notes_scale * strum.strumline_scale),
    );
    let tint = if note.desaturated { 0.65 } else { 1.0 };
    set_sprite_color(sprite, Color::srgba(tint, tint, tint, style.notes_alpha * note.alpha));
}
