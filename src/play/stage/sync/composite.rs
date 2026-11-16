use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::animations::atlas_composite::{
    CompositePiece, FlxAnimateTransformParams, compose_flx_piece_bevy_affine, modulate_color,
};
use crate::play::characters::spawn::CompositeAffine2d;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{set_sprite_atlas, set_sprite_color, set_sprite_flip, set_visibility};

use super::super::schema::{StageCoordMode, composite_piece_z};
use super::{StageCompositePiece, StageCompositeProp, StagePropVisual};

pub(super) type StagePiecesQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Sprite,
        &'static mut Visibility,
        &'static mut CompositeAffine2d,
    ),
    (With<StageCompositePiece>, Without<StagePropVisual>),
>;

pub(super) fn composite_frame_info<'a>(
    composite: &'a StageCompositeProp,
    anim: &AnimationController,
) -> ([f32; 2], [f32; 2], &'a [CompositePiece]) {
    let Some(name) = anim.current.as_deref() else {
        return (composite.bounds_min, composite.bounds_size, &[]);
    };
    let Some(clip) = composite.clips.get(name) else {
        return (composite.bounds_min, composite.bounds_size, &[]);
    };
    let frame_idx = anim.current_atlas_index().unwrap_or(0);
    let pieces = clip.frames.get(frame_idx).map_or(&[][..], Vec::as_slice);
    (clip.bounds_min, clip.bounds_size, pieces)
}

pub(super) fn hide_composite_pieces(entities: &[Entity], pieces: &mut StagePiecesQuery) {
    for &ent in entities {
        if let Ok((_, mut vis, mut aff)) = pieces.get_mut(ent) {
            aff.active = false;
            set_visibility(&mut vis, Visibility::Hidden);
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn sync_composite_prop_pieces(
    vis: &StagePropVisual,
    composite: &StageCompositeProp,
    frame_pieces: &[CompositePiece],
    clip_bounds_min: [f32; 2],
    clip_bounds_size: [f32; 2],
    anim_off: [f32; 2],
    anim_flip: [bool; 2],
    tint: Color,
    eff_scroll_x: f32,
    eff_scroll_y: f32,
    prop_zoom: f32,
    pieces_query: &mut StagePiecesQuery,
) {
    let (mut top_left_x, mut top_left_y) = match vis.coord_mode {
        StageCoordMode::TopLeft => (vis.base_position[0], vis.base_position[1]),
        StageCoordMode::Center => (
            vis.base_position[0] - vis.raw_size[0] * vis.base_scale[0] * 0.5,
            vis.base_position[1] - vis.raw_size[1] * vis.base_scale[1] * 0.5,
        ),
    };
    top_left_x -= anim_off[0] * vis.base_scale[0] + eff_scroll_x;
    top_left_y -= anim_off[1] * vis.base_scale[1] + eff_scroll_y;

    let params = FlxAnimateTransformParams {
        bounds_min: clip_bounds_min,
        bounds_size: clip_bounds_size,
        top_left: [top_left_x, top_left_y],
        scale: vis.base_scale,
        angle_deg: vis.base_angle_deg,
        flip: [
            vis.base_flip[0] ^ anim_flip[0],
            vis.base_flip[1] ^ anim_flip[1],
        ],
        stage_matrix: composite.stage_matrix,
        apply_stage_matrix: composite.atlas_settings.apply_stage_matrix,
        post_stage_matrix_apply: composite.atlas_settings.post_stage_matrix_apply,
        screen_size: [SCREEN_WIDTH, SCREEN_HEIGHT],
        zoom: prop_zoom,
    };

    for (i, &ent) in composite.piece_entities.iter().enumerate() {
        let Ok((mut sprite, mut p_vis, mut aff)) = pieces_query.get_mut(ent) else {
            continue;
        };
        let Some(piece) = frame_pieces.get(i) else {
            aff.active = false;
            set_visibility(&mut p_vis, Visibility::Hidden);
            continue;
        };
        let Some((tex, layout)) = composite
            .textures
            .get(piece.spritemap_idx)
            .or_else(|| composite.textures.first())
        else {
            aff.active = false;
            set_visibility(&mut p_vis, Visibility::Hidden);
            continue;
        };

        let ([bx, by], cols) = compose_flx_piece_bevy_affine(piece, &params);
        let pos = Vec3::new(bx, by, composite_piece_z(vis.z_layer, i));
        set_sprite_atlas(&mut sprite, tex, layout, piece.sprite_index);
        set_sprite_color(&mut sprite, modulate_color(tint, piece.color));
        set_sprite_flip(&mut sprite, false, false);
        aff.active = true;
        aff.cols = cols;
        aff.translation = pos;
        set_visibility(&mut p_vis, Visibility::Visible);
    }
}
