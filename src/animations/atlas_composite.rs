use std::collections::HashMap;

use super::animate_atlas::{AnimateAtlas, AnimateTimelineLayer, layers_duration};
use super::base::BaseAnimationData;

#[derive(Debug, Clone)]
pub struct CompositePiece {
    pub sprite_index: usize,
    pub atlas_size: [u32; 2],
    pub rotated: bool,
    pub matrix: [f32; 6],
}

#[derive(Debug, Clone)]
pub struct CompositeClip {
    pub frames: Vec<Vec<CompositePiece>>,
    pub fps: f32,
    pub looped: bool,
    pub offset: [f32; 2],
    pub flip: [bool; 2],
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
    let mut frames = sample_composite_timeline(atlas, indices, &anim.anim_type, query);
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
        flip: anim.flip,
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
    Some(CompositeClip {
        frames,
        fps: atlas.framerate.max(1.0),
        looped: true,
        offset: [0.0, 0.0],
        flip: [false, false],
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
/// of visible main-timeline layers, returning `(bounds_min, bounds_size)`.
pub fn compute_atlas_whole_bounds(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
) -> ([f32; 2], [f32; 2]) {
    let total = layers_duration(&atlas.main_layers);
    let mut bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let mut scratch = Vec::new();
    let mut any = false;
    for tick in 0..total {
        scratch.clear();
        collect_tick(atlas, indices, &atlas.main_layers, tick, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], 0, &mut scratch);
        if expand_pieces_bounds(&scratch, &mut bounds) {
            any = true;
        }
    }
    if !any {
        return ([0.0, 0.0], [100.0, 100.0]);
    }
    (
        [bounds[0], bounds[1]],
        [(bounds[2] - bounds[0]).max(1.0), (bounds[3] - bounds[1]).max(1.0)],
    )
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
    any.then(|| {
        (
            [bounds[0], bounds[1]],
            [(bounds[2] - bounds[0]).max(1.0), (bounds[3] - bounds[1]).max(1.0)],
        )
    })
}

#[inline]
pub fn upright_size(piece: &CompositePiece) -> (f32, f32) {
    if piece.rotated {
        (piece.atlas_size[1] as f32, piece.atlas_size[0] as f32)
    } else {
        (piece.atlas_size[0] as f32, piece.atlas_size[1] as f32)
    }
}

fn find_symbol_layers<'a>(atlas: &'a AnimateAtlas, name: &str) -> Option<&'a [AnimateTimelineLayer]> {
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
    None
}

fn sample_composite_timeline(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    anim_type: &str,
    prefix: &str,
) -> Vec<Vec<CompositePiece>> {
    if anim_type.eq_ignore_ascii_case("symbol")
        && let Some(layers) = find_symbol_layers(atlas, prefix)
    {
        return sample_layers_composite(atlas, indices, layers, None);
    }
    let ticks = atlas.find_frame_label_indices(prefix);
    if !ticks.is_empty() {
        return sample_layer_ticks(atlas, indices, &atlas.main_layers, &ticks);
    }
    if let Some(layers) = find_symbol_layers(atlas, prefix) {
        return sample_layers_composite(atlas, indices, layers, None);
    }
    Vec::new()
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
            0,
            &mut frame_pieces,
        );
        out.push(frame_pieces);
    }
    out
}

fn collect_tick(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    layers: &[AnimateTimelineLayer],
    tick: usize,
    parent_mx: [f32; 6],
    depth: u8,
    out: &mut Vec<CompositePiece>,
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
        for elem in &frame.elements {
            let combined = mul_affine(parent_mx, elem.matrix);
            if let Some(name) = &elem.sprite_name {
                if let Some(&sprite_index) = indices.get(name) {
                    if let Some(entry) = atlas.sprites.iter().find(|s| s.name == *name) {
                        out.push(CompositePiece {
                            sprite_index,
                            atlas_size: [entry.w, entry.h],
                            rotated: entry.rotated,
                            matrix: combined,
                        });
                    }
                }
            }
            if let Some(sym) = &elem.symbol_name {
                if let Some(sub_layers) = atlas.symbols.get(sym) {
                    let dur = layers_duration(sub_layers).max(1);
                    let child_tick = if elem.loop_mode.eq_ignore_ascii_case("SF")
                        || elem.loop_mode.eq_ignore_ascii_case("singleframe")
                    {
                        elem.first_frame
                    } else if elem.loop_mode.eq_ignore_ascii_case("PO")
                        || elem.loop_mode.eq_ignore_ascii_case("playonce")
                    {
                        (elem.first_frame + rel_tick).min(dur - 1)
                    } else {
                        (elem.first_frame + rel_tick) % dur
                    };
                    collect_tick(atlas, indices, sub_layers, child_tick, combined, depth + 1, out);
                }
            }
        }
    }
}

#[inline]
fn mul_affine(a: [f32; 6], b: [f32; 6]) -> [f32; 6] {
    [
        a[0] * b[0] + a[2] * b[1],
        a[1] * b[0] + a[3] * b[1],
        a[0] * b[2] + a[2] * b[3],
        a[1] * b[2] + a[3] * b[3],
        a[0] * b[4] + a[2] * b[5] + a[4],
        a[1] * b[4] + a[3] * b[5] + a[5],
    ]
}

