use std::f32::consts::FRAC_PI_2;

use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::scene::PlayState;
use crate::utils::ecs::{
    set_atlas_index, set_sprite_color, set_sprite_flip, set_transform_full, set_visibility,
};

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

#[derive(Default)]
pub struct NoteskinSamplerCache {
    applied_ids: [Option<AssetId<Image>>; 8],
}

fn apply_pixel_noteskin_samplers(
    assets: &NoteskinAssets,
    cache: &mut NoteskinSamplerCache,
    images: &mut Assets<Image>,
) {
    if !assets.style.is_pixel {
        if cache.applied_ids.iter().any(Option::is_some) {
            cache.applied_ids = [None; 8];
        }
        return;
    }
    let handles: [Option<&Handle<Image>>; 8] = [
        Some(&assets.strum_texture),
        Some(&assets.notes_texture),
        assets.sustain_texture.as_ref(),
        assets.splash_texture.as_ref(),
        assets.hold_cover_textures[0].as_ref(),
        assets.hold_cover_textures[1].as_ref(),
        assets.hold_cover_textures[2].as_ref(),
        assets.hold_cover_textures[3].as_ref(),
    ];
    for (slot, handle_opt) in handles.into_iter().enumerate() {
        let Some(handle) = handle_opt else {
            cache.applied_ids[slot] = None;
            continue;
        };
        let id = handle.id();
        if cache.applied_ids[slot] == Some(id) {
            continue;
        }
        if let Some(mut img) = images.get_mut(handle) {
            img.sampler = ImageSampler::nearest();
            cache.applied_ids[slot] = Some(id);
        }
    }
}

fn strumline(state: &PlayState, is_player: bool) -> &Strumline {
    if is_player {
        &state.player_strumline
    } else {
        &state.opponent_strumline
    }
}

#[allow(clippy::too_many_arguments)]
pub fn sync_arrow_visuals(
    mut state: ResMut<PlayState>,
    skin_assets: Option<Res<NoteskinAssets>>,
    mut images: ResMut<Assets<Image>>,
    mut sampler_cache: Local<NoteskinSamplerCache>,
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
    if let Some(assets) = skin_assets.as_deref() {
        apply_pixel_noteskin_samplers(assets, &mut sampler_cache, &mut images);
    }

    // `hud_visible(false)` fades the whole HUD, including the strumline and notes.
    // Receptors keep their Visibility component and scale alpha by `state.hud_alpha`.
    let hud_alpha = state.hud_alpha.clamp(0.0, 1.0);
    if hud_alpha <= 0.001 {
        for (_, mut sprite, _, _) in &mut receptors {
            let color = sprite.color.with_alpha(0.0);
            set_sprite_color(&mut sprite, color);
        }
        for (_, _, _, _, mut visibility) in &mut notes {
            set_visibility(&mut visibility, Visibility::Hidden);
        }
        for (_, _, _, _, mut visibility) in &mut sustain_bodies {
            set_visibility(&mut visibility, Visibility::Hidden);
        }
        for (_, _, _, _, mut visibility) in &mut sustain_ends {
            set_visibility(&mut visibility, Visibility::Hidden);
        }
        return;
    }

    let fallback_style;
    let style = if let Some(s) = skin_assets.as_ref() {
        &s.style
    } else {
        fallback_style = state.player_strumline.note_style.clone();
        &fallback_style
    };
    let note_frames = skin_assets
        .as_ref()
        .map(|s| s.note_frames)
        .unwrap_or([(0, [0.0, 0.0], false); 4]);
    let sustain_indices = skin_assets
        .as_ref()
        .map(|s| s.sustain_indices)
        .unwrap_or([(0, 0, 52.0, 44.0); 4]);

    sync_receptors(&mut state, style, hud_alpha, &mut receptors);
    for is_player in [false, true] {
        sync_note_pool(
            strumline(&state, is_player),
            is_player,
            style,
            hud_alpha,
            note_frames,
            &mut scratch,
            &mut notes,
        );
        sync_sustain_body_pool(
            strumline(&state, is_player),
            is_player,
            style,
            hud_alpha,
            sustain_indices,
            &mut scratch,
            &mut sustain_bodies,
        );
        sync_sustain_end_pool(
            strumline(&state, is_player),
            is_player,
            style,
            hud_alpha,
            sustain_indices,
            &mut scratch,
            &mut sustain_ends,
        );
    }
}

fn sync_receptors(
    state: &mut PlayState,
    style: &NoteStyle,
    hud_alpha: f32,
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
        let base_scale = style.strum_scale * strum.strumline_scale;
        let sx = base_scale * rec_state.mod_scale[0];
        let sy = base_scale * rec_state.mod_scale[1];
        let f_off = anim.current_frame_offset();
        let anim_off = anim.current_offset();
        let flip = anim.current_flip();
        let frame_rot = if anim.current_frame_rotated() {
            if flip[0] ^ flip[1] { -FRAC_PI_2 } else { FRAC_PI_2 }
        } else {
            0.0
        };
        let rot = Quat::from_rotation_z(-rec_state.mod_angle_deg.to_radians() + frame_rot);
        let middle_alpha = if strum.middlescroll && !strum.is_player { 0.0 } else { 1.0 };
        set_sprite_flip(&mut sprite, flip[0], flip[1]);
        set_transform_full(
            &mut transform,
            Vec3::new(
                strum.lane_x(rec.lane) + (f_off[0] - anim_off[0]) * sx,
                strum.lane_y(rec.lane) + rec_state.y_offset + (f_off[1] + anim_off[1]) * sy,
                z,
            ),
            Vec3::new(sx, sy, 1.0),
            rot,
        );
        set_sprite_color(
            &mut sprite,
            Color::srgba(
                1.0,
                1.0,
                1.0,
                style.strum_alpha * rec_state.alpha * rec_state.mod_alpha * middle_alpha * hud_alpha,
            ),
        );

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
    hud_alpha: f32,
    note_frames: [(usize, [f32; 2], bool); 4],
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
        let (_, f_off, rotated) = note_frames[vis.lane];
        apply_note_transform(strum, style, vis_pos, note, f_off, rotated, hud_alpha, &mut sprite, &mut transform, &mut visibility);
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
            let (tex_idx, f_off, rotated) = note_frames[vis.lane];
            set_atlas_index(&mut sprite, tex_idx);
            apply_note_transform(strum, style, vis_pos, note, f_off, rotated, hud_alpha, &mut sprite, &mut transform, &mut visibility);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_note_transform(
    strum: &Strumline,
    style: &NoteStyle,
    vis_pos: f64,
    note: &NoteSprite,
    frame_offset: [f32; 2],
    rotated: bool,
    hud_alpha: f32,
    sprite: &mut Mut<'_, Sprite>,
    transform: &mut Mut<'_, Transform>,
    visibility: &mut Mut<'_, Visibility>,
) {
    let lane_idx = note.lane.index();
    if strum.middlescroll && !strum.is_player {
        set_visibility(visibility, Visibility::Hidden);
        return;
    }
    let rec_state = strum.receptors.get(lane_idx);
    let mod_scale = rec_state.map_or([1.0, 1.0], |r| r.mod_scale);
    let mod_alpha = rec_state.map_or(1.0, |r| r.mod_alpha);
    let mod_angle = rec_state.map_or(0.0, |r| r.mod_angle_deg);

    let y = strum.note_y_at(lane_idx, note.data.time_ms, vis_pos) + style.notes_offset[1] + note.y_offset;
    if y < -520.0 || y > 520.0 {
        set_visibility(visibility, Visibility::Hidden);
        return;
    }
    set_visibility(visibility, Visibility::Visible);
    let z = { let t: &Mut<'_, Transform> = &*transform; t.translation.z };
    let base_scale = style.notes_scale * strum.strumline_scale;
    let sx = base_scale * mod_scale[0];
    let sy = base_scale * mod_scale[1];
    let rot_z = -mod_angle.to_radians() + if rotated { FRAC_PI_2 } else { 0.0 };
    let rot = Quat::from_rotation_z(rot_z);
    set_transform_full(
        transform,
        Vec3::new(
            strum.lane_x(lane_idx) + style.notes_offset[0] + frame_offset[0] * sx,
            y + frame_offset[1] * sy,
            z,
        ),
        Vec3::new(sx, sy, 1.0),
        rot,
    );
    let tint = if note.desaturated { 0.65 } else { 1.0 };
    set_sprite_color(
        sprite,
        Color::srgba(tint, tint, tint, style.notes_alpha * note.alpha * mod_alpha * hud_alpha),
    );
}
