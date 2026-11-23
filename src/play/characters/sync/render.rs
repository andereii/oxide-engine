use std::f32::consts::FRAC_PI_2;

use bevy::image::ImageSampler;
use bevy::prelude::*;

use crate::animations::atlas_composite::{
    CompositePiece, FlxAnimateTransformParams, build_attached_piece,
    compose_flx_piece_bevy_affine, modulate_color,
};
use crate::play::stage::schema::{composite_piece_z, stage_layer_z};
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{
    set_sprite_atlas, set_sprite_color, set_sprite_flip, set_transform_full, set_visibility,
};

use super::super::behavior::{CharacterActor, CharacterTrailSample, ClipVisual};
use super::trail::sync_character_trail;
use super::{CharPartsQuery, CharTrailQuery};

pub(super) fn ensure_pixel_sampler(actor: &mut CharacterActor, images: &mut Assets<Image>) {
    if actor.data.is_pixel && !actor.sampler_applied {
        let mut all_ready = true;
        for src in &actor.sources {
            if let Some(mut img) = images.get_mut(&src.texture) {
                img.sampler = ImageSampler::nearest();
            } else {
                all_ready = false;
            }
        }
        actor.sampler_applied = all_ready;
    }
}

pub(super) fn sync_actor_visuals(
    actor: &mut CharacterActor,
    dt_trail_s: f64,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    parts: &mut CharPartsQuery,
    trails: &mut CharTrailQuery,
) {
    let scale = actor.total_scale();
    let eff_flip = actor.effective_flip();
    let anim_flip = actor.controller.current_flip();
    let flip = [eff_flip[0] ^ anim_flip[0], eff_flip[1] ^ anim_flip[1]];
    let anim_off = actor.controller.current_offset();
    let alpha = (actor.slot.opacity * actor.mod_tint[3]).clamp(0.0, 1.0);
    let tint = Color::srgba(actor.mod_tint[0], actor.mod_tint[1], actor.mod_tint[2], alpha);
    let actor_visible = actor.slot.visible && actor.mod_visible;

    let mut current_sample = None;
    let cur_clip = actor
        .controller
        .current_name()
        .and_then(|n| actor.clips.get(n))
        .cloned();
    match cur_clip {
        Some(ClipVisual::Sheet { source_idx }) => {
            current_sample = compute_sheet_sample(actor, source_idx, scale, flip, anim_off);
            if let Ok((mut sprite, mut tf, mut vis, aff_opt)) = parts.get_mut(actor.sheet_entity) {
                if let Some(mut aff) = aff_opt
                    && aff.active
                {
                    aff.active = false;
                }
                if let Some(ref sample) = current_sample {
                    let z = actor
                        .mod_z
                        .unwrap_or_else(|| stage_layer_z(actor.slot.layer, 0));
                    apply_sheet_sample_to_sprite(
                        actor,
                        sample,
                        tint,
                        z,
                        actor_visible,
                        cam_scroll_x,
                        cam_scroll_y,
                        zoom,
                        &mut sprite,
                        &mut tf,
                        &mut vis,
                    );
                } else {
                    set_visibility(&mut vis, Visibility::Hidden);
                }
            }
            hide_entities(&actor.piece_entities, parts);
            if let Some(ref rt) = actor.composite_rt {
                hide_entities(&[rt.quad_entity], parts);
            }
        }
        Some(ClipVisual::Composite {
            source_idx,
            clip,
            stage_matrix,
            ..
        }) => {
            hide_entities(&[actor.sheet_entity], parts);
            let base_pieces = clip
                .frames
                .get(actor.controller.frame_cursor)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let merged_pieces;
            let pieces: &[CompositePiece] = if !actor.attachments.is_empty()
                && let Some(cur_anim) = actor.controller.current_name()
            {
                merged_pieces = build_merged_composite_pieces(
                    actor,
                    cur_anim,
                    actor.controller.frame_cursor,
                    base_pieces,
                );
                &merged_pieces
            } else {
                base_pieces
            };
            if let Some(ref rt) = actor.composite_rt {
                let rt_origin = [
                    SCREEN_WIDTH * 0.5,
                    SCREEN_HEIGHT * 0.5 + actor.idle_size[1] * scale[1] * 0.5,
                ];
                sync_composite_parts(
                    actor,
                    source_idx,
                    pieces,
                    clip.bounds_min,
                    clip.bounds_size,
                    stage_matrix,
                    scale,
                    flip,
                    anim_off,
                    Color::WHITE,
                    0.0,
                    0.0,
                    1.0,
                    Some((rt_origin, 0.0)),
                    parts,
                );
                if let Ok((mut sprite, mut tf, mut vis, aff_opt)) = parts.get_mut(rt.quad_entity) {
                    if let Some(mut aff) = aff_opt
                        && aff.active
                    {
                        aff.active = false;
                    }
                    let world_cx = actor.slot.position[0]
                        + actor.data.offsets[0] * (1.0 + scale[0])
                        - anim_off[0] * scale[0]
                        - cam_scroll_x * actor.slot.scroll_factor[0];
                    let world_cy = actor.slot.position[1]
                        + actor.data.offsets[1] * (1.0 + scale[1])
                        - anim_off[1] * scale[1]
                        - cam_scroll_y * actor.slot.scroll_factor[1]
                        - actor.idle_size[1] * scale[1] * 0.5;
                    let bx = (world_cx - SCREEN_WIDTH * 0.5) * zoom;
                    let by = (SCREEN_HEIGHT * 0.5 - world_cy) * zoom;
                    let z_base = actor
                        .mod_z
                        .unwrap_or_else(|| stage_layer_z(actor.slot.layer, 0));
                    set_sprite_color(&mut sprite, tint);
                    set_sprite_flip(&mut sprite, false, false);
                    set_transform_full(
                        &mut tf,
                        Vec3::new(bx, by, z_base),
                        Vec3::new(zoom, zoom, 1.0),
                        Quat::IDENTITY,
                    );
                    set_visibility(
                        &mut vis,
                        if actor_visible {
                            Visibility::Visible
                        } else {
                            Visibility::Hidden
                        },
                    );
                }
            } else {
                sync_composite_parts(
                    actor,
                    source_idx,
                    pieces,
                    clip.bounds_min,
                    clip.bounds_size,
                    stage_matrix,
                    scale,
                    flip,
                    anim_off,
                    tint,
                    cam_scroll_x,
                    cam_scroll_y,
                    zoom,
                    None,
                    parts,
                );
            }
        }
        None => {
            hide_entities(&[actor.sheet_entity], parts);
            hide_entities(&actor.piece_entities, parts);
            if let Some(ref rt) = actor.composite_rt {
                hide_entities(&[rt.quad_entity], parts);
            }
        }
    }

    sync_character_trail(
        actor,
        current_sample,
        dt_trail_s,
        alpha,
        actor_visible,
        cam_scroll_x,
        cam_scroll_y,
        zoom,
        trails,
    );
}

fn compute_sheet_sample(
    actor: &CharacterActor,
    source_idx: usize,
    scale: [f32; 2],
    flip: [bool; 2],
    anim_off: [f32; 2],
) -> Option<CharacterTrailSample> {
    let src = actor.sources.get(source_idx)?;
    let frame_idx = actor.controller.current_atlas_index().unwrap_or(0);
    let frame_sz = src
        .frame_sizes
        .get(&frame_idx)
        .copied()
        .unwrap_or(actor.idle_size);
    let mut world_cx = actor.slot.position[0]
        + (frame_sz[0] - actor.idle_size[0]) * 0.5 * scale[0]
        + actor.data.offsets[0] * (1.0 + scale[0]);
    let mut world_cy = actor.slot.position[1]
        - (actor.idle_size[1] - frame_sz[1] * 0.5) * scale[1]
        + actor.data.offsets[1] * (1.0 + scale[1]);
    world_cx -= anim_off[0] * scale[0];
    world_cy -= anim_off[1] * scale[1];

    let frame_off = actor.controller.current_frame_offset();
    let foff = [
        if flip[0] { -frame_off[0] } else { frame_off[0] },
        if flip[1] { -frame_off[1] } else { frame_off[1] },
    ];
    let rot = if actor.controller.current_frame_rotated() {
        if flip[0] ^ flip[1] {
            -FRAC_PI_2
        } else {
            FRAC_PI_2
        }
    } else {
        0.0
    };
    Some(CharacterTrailSample {
        source_idx,
        frame_idx,
        world_cx,
        world_cy,
        foff,
        scale,
        flip,
        rot,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_sheet_sample_to_sprite(
    actor: &CharacterActor,
    sample: &CharacterTrailSample,
    tint: Color,
    z: f32,
    visible: bool,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    sprite: &mut Mut<'_, Sprite>,
    tf: &mut Mut<'_, Transform>,
    vis: &mut Mut<'_, Visibility>,
) {
    let Some(src) = actor.sources.get(sample.source_idx) else {
        set_visibility(vis, Visibility::Hidden);
        return;
    };
    set_sprite_atlas(sprite, &src.texture, &src.layout, sample.frame_idx);
    set_sprite_color(sprite, tint);
    set_sprite_flip(sprite, sample.flip[0], sample.flip[1]);

    let screen_cx = sample.world_cx - cam_scroll_x * actor.slot.scroll_factor[0];
    let screen_cy = sample.world_cy - cam_scroll_y * actor.slot.scroll_factor[1];
    let bx = (screen_cx - SCREEN_WIDTH * 0.5) * zoom + sample.foff[0] * sample.scale[0] * zoom;
    let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * zoom + sample.foff[1] * sample.scale[1] * zoom;
    set_transform_full(
        tf,
        Vec3::new(bx, by, z),
        Vec3::new(sample.scale[0] * zoom, sample.scale[1] * zoom, 1.0),
        Quat::from_rotation_z(sample.rot),
    );
    set_visibility(
        vis,
        if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        },
    );
}

#[allow(clippy::too_many_arguments)]
fn sync_composite_parts(
    actor: &CharacterActor,
    source_idx: usize,
    pieces: &[CompositePiece],
    bounds_min: [f32; 2],
    bounds_size: [f32; 2],
    stage_matrix: Option<[f32; 6]>,
    scale: [f32; 2],
    flip: [bool; 2],
    anim_off: [f32; 2],
    tint: Color,
    cam_scroll_x: f32,
    cam_scroll_y: f32,
    zoom: f32,
    rt_override: Option<([f32; 2], f32)>,
    parts: &mut CharPartsQuery,
) {
    let Some(src) = actor.sources.get(source_idx) else {
        hide_entities(&actor.piece_entities, parts);
        return;
    };
    let (screen_fx, screen_fy) = if let Some((rt_origin, _)) = rt_override {
        (rt_origin[0], rt_origin[1])
    } else {
        (
            actor.slot.position[0]
                + actor.data.offsets[0] * (1.0 + scale[0])
                - anim_off[0] * scale[0]
                - cam_scroll_x * actor.slot.scroll_factor[0],
            actor.slot.position[1]
                + actor.data.offsets[1] * (1.0 + scale[1])
                - anim_off[1] * scale[1]
                - cam_scroll_y * actor.slot.scroll_factor[1],
        )
    };
    let top_left = [
        screen_fx - actor.idle_size[0] * scale[0] * 0.5,
        screen_fy - actor.idle_size[1] * scale[1],
    ];
    let atlas_settings = actor.data.effective_atlas_settings();
    let params = FlxAnimateTransformParams {
        bounds_min,
        bounds_size,
        top_left,
        scale,
        angle_deg: 0.0,
        flip,
        stage_matrix,
        apply_stage_matrix: atlas_settings.apply_stage_matrix,
        post_stage_matrix_apply: atlas_settings.post_stage_matrix_apply,
        screen_size: [SCREEN_WIDTH, SCREEN_HEIGHT],
        zoom,
    };
    let visible = if actor.slot.visible && actor.mod_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    for (i, &ent) in actor.piece_entities.iter().enumerate() {
        let Ok((mut sprite, _, mut vis, aff_opt)) = parts.get_mut(ent) else {
            continue;
        };
        let Some(mut aff) = aff_opt else {
            continue;
        };
        let Some(piece) = pieces.get(i) else {
            if aff.active {
                aff.active = false;
            }
            set_visibility(&mut vis, Visibility::Hidden);
            continue;
        };
        let ([bx, by], cols) = compose_flx_piece_bevy_affine(piece, &params);
        let base_z = if let Some((_, rt_z)) = rt_override {
            rt_z
        } else {
            actor
                .mod_z
                .unwrap_or_else(|| stage_layer_z(actor.slot.layer, 0))
        };
        let pos = Vec3::new(bx, by, composite_piece_z(base_z, i));
        let (tex, layout) = piece
            .attachment_idx
            .and_then(|att_idx| actor.attachments.get(att_idx))
            .and_then(|att| {
                att.textures
                    .get(piece.spritemap_idx)
                    .or_else(|| att.textures.first())
                    .map(|(t, l)| (t, l))
            })
            .unwrap_or_else(|| src.atlas_pair(piece.spritemap_idx));
        set_sprite_atlas(&mut sprite, tex, layout, piece.sprite_index);
        set_sprite_color(&mut sprite, modulate_color(tint, piece.color));
        set_sprite_flip(&mut sprite, false, false);
        aff.active = true;
        aff.cols = cols;
        aff.translation = pos;
        set_visibility(&mut vis, visible);
    }
}

fn build_merged_composite_pieces(
    actor: &CharacterActor,
    anim_name: &str,
    frame_idx: usize,
    base_pieces: &[CompositePiece],
) -> Vec<CompositePiece> {
    let mut insertions: Vec<(usize, CompositePiece)> = Vec::new();
    for (att_idx, att) in actor.attachments.iter().enumerate() {
        if !att.visible || att.alpha <= 0.0001 || att.frames.is_empty() {
            continue;
        }
        let Some(anchors) = att
            .per_clip_anchors
            .get(anim_name)
            .and_then(|frames| frames.get(frame_idx))
        else {
            continue;
        };
        if anchors.is_empty() {
            continue;
        }
        let att_frame_idx = att.cur_frame.min(att.frames.len().saturating_sub(1));
        let Some(src_frame) = att.frames.get(att_frame_idx) else {
            continue;
        };
        for anchor in anchors {
            for src_piece in src_frame {
                insertions.push((
                    anchor.after_piece_idx,
                    build_attached_piece(
                        src_piece,
                        att_idx,
                        att.bounds_min,
                        att.bounds_size,
                        anchor.matrix,
                        anchor.color,
                        att.offset,
                        att.angle_deg,
                        att.flip_x,
                        att.alpha,
                    ),
                ));
            }
        }
    }
    if insertions.is_empty() {
        return base_pieces.to_vec();
    }
    let mut merged = Vec::with_capacity(base_pieces.len() + insertions.len());
    for (idx, base_piece) in base_pieces.iter().enumerate() {
        for (ins_idx, piece) in &insertions {
            if *ins_idx == idx {
                merged.push(piece.clone());
            }
        }
        merged.push(base_piece.clone());
    }
    for (ins_idx, piece) in insertions {
        if ins_idx >= base_pieces.len() {
            merged.push(piece);
        }
    }
    merged
}

fn hide_entities(entities: &[Entity], parts: &mut CharPartsQuery) {
    for &ent in entities {
        if let Ok((_, _, mut vis, aff_opt)) = parts.get_mut(ent) {
            if let Some(mut aff) = aff_opt
                && aff.active
            {
                aff.active = false;
            }
            set_visibility(&mut vis, Visibility::Hidden);
        }
    }
}
