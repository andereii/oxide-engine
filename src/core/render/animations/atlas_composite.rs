use std::collections::HashMap;

use bevy::prelude::Color;

use super::animate_atlas::{AnimateAtlas, AnimateTimelineLayer, layers_duration};
use super::base::BaseAnimationData;

/// Multiplies a base sprite tint color by a Flash frame/symbol color transform `[r, g, b, a]`.
#[inline]
pub fn modulate_color(base: Color, piece_color: [f32; 4]) -> Color {
    let s = base.to_srgba();
    Color::srgba(
        s.red * piece_color[0],
        s.green * piece_color[1],
        s.blue * piece_color[2],
        (s.alpha * piece_color[3]).clamp(0.0, 1.0),
    )
}

#[derive(Debug, Clone)]
pub struct CompositePiece {
    pub sprite_index: usize,
    pub spritemap_idx: usize,
    pub atlas_size: [u32; 2],
    pub rotated: bool,
    pub matrix: [f32; 6],
    pub color: [f32; 4],
    pub attachment_idx: Option<usize>,
}

/// Captured symbol-instance placement inside a composite frame, matched by symbol keyword.
#[derive(Debug, Clone, Copy)]
pub struct SymbolAnchor {
    pub after_piece_idx: usize,
    pub matrix: [f32; 6],
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct CompositeClip {
    pub frames: Vec<Vec<CompositePiece>>,
    pub fps: f32,
    pub looped: bool,
    pub offset: [f32; 2],
    pub flip: [bool; 2],
    pub bounds_min: [f32; 2],
    pub bounds_size: [f32; 2],
}

/// Inputs needed to place a `CompositePiece` in Bevy world coordinates
/// matching `flixel-animate`'s `FlxAnimate.prepareAnimateDrawMatrix`.
#[derive(Debug, Clone, Copy)]
pub struct FlxAnimateTransformParams {
    pub bounds_min: [f32; 2],
    pub bounds_size: [f32; 2],
    pub top_left: [f32; 2],
    pub scale: [f32; 2],
    pub angle_deg: f32,
    pub flip: [bool; 2],
    pub stage_matrix: Option<[f32; 6]>,
    pub apply_stage_matrix: bool,
    pub post_stage_matrix_apply: bool,
    pub screen_size: [f32; 2],
    pub zoom: f32,
}

pub fn build_composite_clip(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    anim: &BaseAnimationData,
) -> Option<CompositeClip> {
    if anim.name.is_empty() {
        return None;
    }
    let query = if anim.prefix.is_empty() { &anim.name } else { &anim.prefix };
    let (mut frames, bounds_min, bounds_size) =
        sample_composite_timeline(atlas, indices, &anim.anim_type, query);
    if frames.is_empty() {
        return None;
    }
    if !anim.frame_indices.is_empty() {
        frames = anim
            .frame_indices
            .iter()
            .filter_map(|&idx| frames.get(idx).cloned())
            .collect();
    }
    if frames.is_empty() {
        return None;
    }
    let fps = if anim.frame_rate > 0.0 {
        anim.frame_rate
    } else {
        atlas.framerate.max(1.0)
    };
    Some(CompositeClip {
        frames,
        fps,
        looped: anim.looped,
        offset: anim.offsets,
        flip: anim.resolved_flip(),
        bounds_min,
        bounds_size,
    })
}

pub fn sample_default_composite_clip(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
) -> Option<CompositeClip> {
    let frames = sample_layers_composite(atlas, indices, &atlas.main_layers, None);
    if frames.is_empty() {
        return None;
    }
    let (bounds_min, bounds_size) = compute_atlas_whole_bounds(atlas, indices);
    Some(CompositeClip {
        frames,
        fps: atlas.framerate.max(1.0),
        looped: true,
        offset: [0.0, 0.0],
        flip: [false, false],
        bounds_min,
        bounds_size,
    })
}

fn expand_pieces_bounds(pieces: &[CompositePiece], bounds: &mut [f32; 4]) -> bool {
    let mut any = false;
    for piece in pieces {
        any = true;
        let (w, h) = upright_size(piece);
        let m = piece.matrix;
        for (cx, cy) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
            let x = m[0] * cx + m[2] * cy + m[4];
            let y = m[1] * cx + m[3] * cy + m[5];
            bounds[0] = bounds[0].min(x);
            bounds[1] = bounds[1].min(y);
            bounds[2] = bounds[2].max(x);
            bounds[3] = bounds[3].max(y);
        }
    }
    any
}

/// Computes `flixel-animate`'s `Timeline._bounds` (`getWholeBounds`) across all frames
/// of a layer list, returning `(bounds_min, bounds_size)`.
pub fn compute_layers_bounds(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    layers: &[AnimateTimelineLayer],
) -> ([f32; 2], [f32; 2]) {
    let total = layers_duration(layers);
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let mut scratch = Vec::new();
    let mut any = false;
    for tick in 0..total {
        scratch.clear();
        collect_tick(
            atlas,
            indices,
            layers,
            tick,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0; 4],
            0,
            &mut scratch,
            None,
        );
        if expand_pieces_bounds(&scratch, &mut bounds) {
            any = true;
        }
    }
    finalize_bounds(any, bounds)
}

/// Computes `flixel-animate`'s `Timeline._bounds` (`getWholeBounds`) across all frames
/// of visible main-timeline layers, returning `(bounds_min, bounds_size)`.
pub fn compute_atlas_whole_bounds(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
) -> ([f32; 2], [f32; 2]) {
    compute_layers_bounds(atlas, indices, &atlas.main_layers)
}

#[allow(dead_code)]
pub fn compute_composite_bounds(pieces: &[CompositePiece]) -> ([f32; 2], [f32; 2]) {
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    if !expand_pieces_bounds(pieces, &mut bounds) {
        return ([0.0, 0.0], [100.0, 100.0]);
    }
    (
        [(bounds[0] + bounds[2]) * 0.5, bounds[3]],
        [(bounds[2] - bounds[0]).max(1.0), (bounds[3] - bounds[1]).max(1.0)],
    )
}

#[allow(dead_code)]
pub fn compute_clips_bounds<'a>(
    clips: impl Iterator<Item = &'a CompositeClip>,
) -> Option<([f32; 2], [f32; 2])> {
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let mut any = false;
    for clip in clips {
        for frame in &clip.frames {
            if expand_pieces_bounds(frame, &mut bounds) {
                any = true;
            }
        }
    }
    any.then(|| finalize_bounds(true, bounds))
}

fn finalize_bounds(any: bool, bounds: [f32; 4]) -> ([f32; 2], [f32; 2]) {
    if !any {
        return ([0.0, 0.0], [100.0, 100.0]);
    }
    (
        [bounds[0], bounds[1]],
        [(bounds[2] - bounds[0]).max(1.0), (bounds[3] - bounds[1]).max(1.0)],
    )
}

#[inline]
pub fn upright_size(piece: &CompositePiece) -> (f32, f32) {
    if piece.rotated {
        (piece.atlas_size[1] as f32, piece.atlas_size[0] as f32)
    } else {
        (piece.atlas_size[0] as f32, piece.atlas_size[1] as f32)
    }
}

/// Computes Bevy world `[x, y]` and affine column matrix `[c0x, c0y, c1x, c1y]`
/// for one composite piece using `FlxAnimate.prepareAnimateDrawMatrix` order.
pub fn compose_flx_piece_bevy_affine(
    piece: &CompositePiece,
    p: &FlxAnimateTransformParams,
) -> ([f32; 2], [f32; 4]) {
    let [bx, by] = p.bounds_min;
    let [bw, bh] = p.bounds_size;
    let mut t = [1.0_f32, 0.0, 0.0, 1.0, 0.0, 0.0];

    t = mul_affine([1.0, 0.0, 0.0, 1.0, -bx, -by], t);
    if p.flip[0] {
        t = mul_affine([-1.0, 0.0, 0.0, 1.0, bw, 0.0], t);
    }
    if p.flip[1] {
        t = mul_affine([1.0, 0.0, 0.0, -1.0, 0.0, bh], t);
    }

    let mut origin = [bw * 0.5, bh * 0.5];
    if p.apply_stage_matrix
        && let Some(sm) = p.stage_matrix
    {
        if p.post_stage_matrix_apply {
            t = mul_affine([1.0, 0.0, 0.0, 1.0, bx, by], t);
        } else {
            t = mul_affine(sm, t);
            t = mul_affine([1.0, 0.0, 0.0, 1.0, bx * sm[0], by * sm[3]], t);
            origin = [bw * sm[0] * 0.5, bh * sm[3] * 0.5];
        }
    }

    t = mul_affine([1.0, 0.0, 0.0, 1.0, -origin[0], -origin[1]], t);
    t = mul_affine([p.scale[0], 0.0, 0.0, p.scale[1], 0.0, 0.0], t);
    if p.angle_deg.abs() > 1e-6 {
        let (sin_r, cos_r) = p.angle_deg.to_radians().sin_cos();
        t = mul_affine([cos_r, sin_r, -sin_r, cos_r, 0.0, 0.0], t);
    }

    if p.apply_stage_matrix
        && p.post_stage_matrix_apply
        && let Some(sm) = p.stage_matrix
    {
        t = mul_affine(sm, t);
    }

    t = mul_affine(
        [
            1.0,
            0.0,
            0.0,
            1.0,
            p.top_left[0] + origin[0] * p.scale[0],
            p.top_left[1] + origin[1] * p.scale[1],
        ],
        t,
    );

    let full = mul_affine(t, piece.matrix);
    let (w, h) = upright_size(piece);
    let scx = full[0] * (w * 0.5) + full[2] * (h * 0.5) + full[4];
    let scy = full[1] * (w * 0.5) + full[3] * (h * 0.5) + full[5];
    let bevy_x = (scx - p.screen_size[0] * 0.5) * p.zoom;
    let bevy_y = (p.screen_size[1] * 0.5 - scy) * p.zoom;

    let z = p.zoom;
    let cols = if piece.rotated {
        [-full[2] * z, full[3] * z, -full[0] * z, full[1] * z]
    } else {
        [full[0] * z, -full[1] * z, -full[2] * z, full[3] * z]
    };
    ([bevy_x, bevy_y], cols)
}

pub(super) fn find_symbol_layers<'a>(atlas: &'a AnimateAtlas, name: &str) -> Option<&'a [AnimateTimelineLayer]> {
    if let Some(layers) = atlas.symbols.get(name) {
        return Some(layers);
    }
    let trimmed = name.trim();
    if let Some((_, layers)) = atlas
        .symbols
        .iter()
        .find(|(k, _)| k.trim().eq_ignore_ascii_case(trimmed))
    {
        return Some(layers);
    }
    if !atlas.main_symbol.is_empty() && atlas.main_symbol.trim().eq_ignore_ascii_case(trimmed) {
        return Some(&atlas.main_layers);
    }
    // Flash symbol folder fallback (`Folder/SymbolName` <-> `SymbolName`).
    let leaf = trimmed.rsplit('/').next().unwrap_or(trimmed).trim();
    atlas
        .symbols
        .iter()
        .find(|(k, _)| {
            k.trim()
                .rsplit('/')
                .next()
                .is_some_and(|kl| kl.trim().eq_ignore_ascii_case(leaf))
        })
        .map(|(_, layers)| layers.as_slice())
}

fn sample_composite_timeline(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    anim_type: &str,
    prefix: &str,
) -> (Vec<Vec<CompositePiece>>, [f32; 2], [f32; 2]) {
    if anim_type.eq_ignore_ascii_case("symbol")
        && let Some(layers) = find_symbol_layers(atlas, prefix)
    {
        let frames = sample_layers_composite(atlas, indices, layers, None);
        let (bmin, bsize) = compute_layers_bounds(atlas, indices, layers);
        return (frames, bmin, bsize);
    }
    let ticks = atlas.find_frame_label_indices(prefix);
    if !ticks.is_empty() {
        let frames = sample_layer_ticks(atlas, indices, &atlas.main_layers, &ticks);
        let (bmin, bsize) = compute_layers_bounds(atlas, indices, &atlas.main_layers);
        return (frames, bmin, bsize);
    }
    if let Some(layers) = find_symbol_layers(atlas, prefix) {
        let frames = sample_layers_composite(atlas, indices, layers, None);
        let (bmin, bsize) = compute_layers_bounds(atlas, indices, layers);
        return (frames, bmin, bsize);
    }
    if !atlas.main_layers.is_empty() {
        let frames = sample_layers_composite(atlas, indices, &atlas.main_layers, None);
        let (bmin, bsize) = compute_layers_bounds(atlas, indices, &atlas.main_layers);
        return (frames, bmin, bsize);
    }
    (Vec::new(), [0.0, 0.0], [100.0, 100.0])
}

fn sample_layers_composite(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    layers: &[AnimateTimelineLayer],
    range: Option<(usize, usize)>,
) -> Vec<Vec<CompositePiece>> {
    let max_end = layers_duration(layers);
    let (start_t, end_t) = range.unwrap_or((0, max_end));
    let ticks: Vec<usize> = (start_t..end_t).collect();
    sample_layer_ticks(atlas, indices, layers, &ticks)
}

fn sample_layer_ticks(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    layers: &[AnimateTimelineLayer],
    ticks: &[usize],
) -> Vec<Vec<CompositePiece>> {
    let mut out = Vec::with_capacity(ticks.len());
    for &t in ticks {
        let mut frame_pieces = Vec::new();
        collect_tick(
            atlas,
            indices,
            layers,
            t,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0; 4],
            0,
            &mut frame_pieces,
            None,
        );
        out.push(frame_pieces);
    }
    out
}

fn push_sprite_piece(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    name: &str,
    matrix: [f32; 6],
    color: [f32; 4],
    out: &mut Vec<CompositePiece>,
) {
    let Some(&sprite_index) = indices.get(name) else {
        return;
    };
    let Some(entry) = atlas
        .sprite_lookup
        .get(name)
        .and_then(|&idx| atlas.sprites.get(idx))
    else {
        return;
    };
    out.push(CompositePiece {
        sprite_index,
        spritemap_idx: entry.spritemap_idx,
        atlas_size: [entry.w, entry.h],
        rotated: entry.rotated,
        matrix,
        color,
        attachment_idx: None,
    });
}

/// Collects per-frame [`SymbolAnchor`] placements for symbols matching `keyword` (case-insensitive
/// substring) in the given animation definition.
pub fn collect_symbol_anchors(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    anim: &BaseAnimationData,
    keyword: &str,
) -> Vec<Vec<SymbolAnchor>> {
    let kw = keyword.trim().to_ascii_lowercase();
    if kw.is_empty() || anim.name.is_empty() {
        return Vec::new();
    }
    let query = if anim.prefix.is_empty() {
        &anim.name
    } else {
        &anim.prefix
    };
    let (layers, ticks): (&[AnimateTimelineLayer], Vec<usize>) =
        if anim.anim_type.eq_ignore_ascii_case("symbol")
            && let Some(layers) = find_symbol_layers(atlas, query)
        {
            (layers, (0..layers_duration(layers)).collect())
        } else {
            let label_ticks = atlas.find_frame_label_indices(query);
            if !label_ticks.is_empty() {
                (&atlas.main_layers, label_ticks)
            } else if let Some(layers) = find_symbol_layers(atlas, query) {
                (layers, (0..layers_duration(layers)).collect())
            } else if !atlas.main_layers.is_empty() {
                (&atlas.main_layers, (0..layers_duration(&atlas.main_layers)).collect())
            } else {
                return Vec::new();
            }
        };

    let selected_ticks: Vec<usize> = if anim.frame_indices.is_empty() {
        ticks
    } else {
        anim.frame_indices
            .iter()
            .filter_map(|&idx| ticks.get(idx).copied())
            .collect()
    };

    let mut out = Vec::with_capacity(selected_ticks.len());
    for t in selected_ticks {
        let mut frame_pieces = Vec::new();
        let mut anchors = Vec::new();
        collect_tick(
            atlas,
            indices,
            layers,
            t,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            [1.0; 4],
            0,
            &mut frame_pieces,
            Some((&kw, &mut anchors)),
        );
        out.push(anchors);
    }
    out
}

/// Transforms a piece from an attached child `AnimateAtlas` into parent symbol space,
/// matching `flixel-animate`'s `FlxSpriteElement` positioning, rotation, `flipX`, and `offset`.
#[allow(clippy::too_many_arguments)]
pub fn build_attached_piece(
    src_piece: &CompositePiece,
    attachment_idx: usize,
    attach_bounds_min: [f32; 2],
    attach_bounds_size: [f32; 2],
    anchor_mx: [f32; 6],
    anchor_color: [f32; 4],
    offset: [f32; 2],
    angle_deg: f32,
    flip_x: bool,
    alpha: f32,
) -> CompositePiece {
    let [lbx, lby] = attach_bounds_min;
    let [lw, lh] = attach_bounds_size;
    let mut m = mul_affine([1.0, 0.0, 0.0, 1.0, -lbx, -lby], src_piece.matrix);
    if flip_x {
        m = mul_affine([-1.0, 0.0, 0.0, 1.0, lw, 0.0], m);
    }
    m = mul_affine([1.0, 0.0, 0.0, 1.0, -lw * 0.5, -lh * 0.5], m);
    let total_rad = angle_deg.to_radians() + anchor_mx[1].atan2(anchor_mx[0]);
    if total_rad.abs() > 1e-6 {
        let (sin_r, cos_r) = total_rad.sin_cos();
        m = mul_affine([cos_r, sin_r, -sin_r, cos_r, 0.0, 0.0], m);
    }
    m = mul_affine(
        [
            1.0,
            0.0,
            0.0,
            1.0,
            anchor_mx[4] + lw * 0.5 - offset[0],
            anchor_mx[5] + lh * 0.5 - offset[1],
        ],
        m,
    );
    let color = mul_color(
        anchor_color,
        [
            src_piece.color[0],
            src_piece.color[1],
            src_piece.color[2],
            src_piece.color[3] * alpha.clamp(0.0, 1.0),
        ],
    );
    CompositePiece {
        sprite_index: src_piece.sprite_index,
        spritemap_idx: src_piece.spritemap_idx,
        atlas_size: src_piece.atlas_size,
        rotated: src_piece.rotated,
        matrix: m,
        color,
        attachment_idx: Some(attachment_idx),
    }
}

/// Resolves the active child frame index for a symbol instance, returning `None`
/// when `SingleFrame` (`SF`) points past the end of the symbol (`first_frame >= dur`)
/// or when the target symbol has no frames (`dur == 0`), matching `flixel-animate`.
pub(super) fn resolve_child_tick(
    loop_mode: &str,
    first_frame: usize,
    last_frame: isize,
    rel_tick: usize,
    dur: usize,
) -> Option<usize> {
    if dur == 0 {
        return None;
    }
    if loop_mode.eq_ignore_ascii_case("SF") || loop_mode.eq_ignore_ascii_case("singleframe") {
        return (first_frame < dur).then_some(first_frame);
    }
    let max_idx = dur - 1;
    let start = first_frame.min(max_idx);
    let end = if last_frame >= 0 && (last_frame as usize) >= start {
        (last_frame as usize).min(max_idx)
    } else {
        max_idx
    };
    let span = end.saturating_sub(start).saturating_add(1).max(1);
    if loop_mode.eq_ignore_ascii_case("PO") || loop_mode.eq_ignore_ascii_case("playonce") {
        Some((start + rel_tick).min(end))
    } else {
        Some(start + (rel_tick % span))
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_single_layer_frame_pieces(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    frame: &super::animate_atlas::AnimateTimelineFrame,
    rel_tick: usize,
    parent_mx: [f32; 6],
    parent_color: [f32; 4],
    depth: u8,
    out: &mut Vec<CompositePiece>,
) {
    let frame_color = mul_color(parent_color, frame.color);
    for elem in &frame.elements {
        let combined = mul_affine(parent_mx, elem.matrix);
        let elem_color = mul_color(frame_color, elem.color);
        if let Some(name) = &elem.sprite_name {
            push_sprite_piece(atlas, indices, name, combined, elem_color, out);
        }
        if let Some(sym) = &elem.symbol_name {
            if let Some(sub_layers) = find_symbol_layers(atlas, sym) {
                let dur = layers_duration(sub_layers);
                if let Some(child_tick) = resolve_child_tick(
                    &elem.loop_mode,
                    elem.first_frame,
                    elem.last_frame,
                    rel_tick,
                    dur,
                ) {
                    collect_tick(
                        atlas,
                        indices,
                        sub_layers,
                        child_tick,
                        combined,
                        elem_color,
                        depth + 1,
                        out,
                        None,
                    );
                }
            } else if let Some(bm) = &elem.bitmap_fallback {
                push_sprite_piece(
                    atlas,
                    indices,
                    &bm.sprite_name,
                    mul_affine(combined, bm.matrix),
                    elem_color,
                    out,
                );
            }
        } else if let Some(bm) = &elem.bitmap_fallback {
            push_sprite_piece(
                atlas,
                indices,
                &bm.sprite_name,
                mul_affine(combined, bm.matrix),
                elem_color,
                out,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn collect_single_layer_tick_pieces(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    layer: &AnimateTimelineLayer,
    tick: usize,
    parent_mx: [f32; 6],
    parent_color: [f32; 4],
    depth: u8,
    out: &mut Vec<CompositePiece>,
) {
    let Some(frame) = layer
        .frames
        .iter()
        .find(|f| tick >= f.index && tick < f.index + f.duration)
    else {
        return;
    };
    collect_single_layer_frame_pieces(
        atlas,
        indices,
        frame,
        tick.saturating_sub(frame.index),
        parent_mx,
        parent_color,
        depth,
        out,
    );
}

#[allow(clippy::too_many_arguments)]
fn collect_tick(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    layers: &[AnimateTimelineLayer],
    tick: usize,
    parent_mx: [f32; 6],
    parent_color: [f32; 4],
    depth: u8,
    out: &mut Vec<CompositePiece>,
    mut anchor_filter: Option<(&str, &mut Vec<SymbolAnchor>)>,
) {
    if depth > 12 {
        return;
    }
    // Flash layers are ordered front-to-back (`L[0]` on top), elements within a frame back-to-front.
    for layer in layers.iter().rev().filter(|l| l.visible) {
        let Some(frame) = layer
            .frames
            .iter()
            .find(|f| tick >= f.index && tick < f.index + f.duration)
        else {
            continue;
        };
        let rel_tick = tick.saturating_sub(frame.index);
        let frame_color = mul_color(parent_color, frame.color);
        for elem in &frame.elements {
            let combined = mul_affine(parent_mx, elem.matrix);
            let elem_color = mul_color(frame_color, elem.color);
            if let Some(name) = &elem.sprite_name {
                push_sprite_piece(atlas, indices, name, combined, elem_color, out);
            }
            if let Some(sym) = &elem.symbol_name {
                if let Some(sub_layers) = find_symbol_layers(atlas, sym) {
                    let dur = layers_duration(sub_layers);
                    if let Some(child_tick) = resolve_child_tick(
                        &elem.loop_mode,
                        elem.first_frame,
                        elem.last_frame,
                        rel_tick,
                        dur,
                    ) {
                        collect_tick(
                            atlas,
                            indices,
                            sub_layers,
                            child_tick,
                            combined,
                            elem_color,
                            depth + 1,
                            out,
                            anchor_filter
                                .as_mut()
                                .map(|(kw, vec)| (*kw, &mut **vec)),
                        );
                    }
                } else if let Some(bm) = &elem.bitmap_fallback {
                    push_sprite_piece(
                        atlas,
                        indices,
                        &bm.sprite_name,
                        mul_affine(combined, bm.matrix),
                        elem_color,
                        out,
                    );
                }
                if let Some((kw, ref mut anchors)) = anchor_filter
                    && sym.to_ascii_lowercase().contains(kw)
                {
                    anchors.push(SymbolAnchor {
                        after_piece_idx: out.len(),
                        matrix: combined,
                        color: elem_color,
                    });
                }
            } else if let Some(bm) = &elem.bitmap_fallback {
                push_sprite_piece(
                    atlas,
                    indices,
                    &bm.sprite_name,
                    mul_affine(combined, bm.matrix),
                    elem_color,
                    out,
                );
            }
        }
    }
}

#[inline]
pub(super) fn mul_color(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [a[0] * b[0], a[1] * b[1], a[2] * b[2], a[3] * b[3]]
}

#[inline]
pub(super) fn mul_affine(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

#[cfg(test)]
mod tests {
    use super::resolve_child_tick;

    #[test]
    fn single_frame_out_of_range_returns_none() {
        assert_eq!(resolve_child_tick("SF", 8, -1, 0, 2), None);
        assert_eq!(resolve_child_tick("singleframe", 2, -1, 5, 2), None);
        assert_eq!(resolve_child_tick("SF", 1, -1, 5, 2), Some(1));
        assert_eq!(resolve_child_tick("LP", 0, -1, 0, 0), None);
    }

    #[test]
    fn loop_and_play_once_ticks_clamp_within_span() {
        assert_eq!(resolve_child_tick("PO", 1, -1, 10, 4), Some(3));
        assert_eq!(resolve_child_tick("LP", 1, 2, 3, 4), Some(2));
    }
}
