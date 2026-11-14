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

pub fn compute_composite_bounds(pieces: &[CompositePiece]) -> ([f32; 2], [f32; 2]) {
    if pieces.is_empty() {
        return ([0.0, 0.0], [100.0, 100.0]);
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for piece in pieces {
        let (w, h) = upright_size(piece);
        let m = piece.matrix;
        for (cx, cy) in [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)] {
            let x = m[0] * cx + m[2] * cy + m[4];
            let y = m[1] * cx + m[3] * cy + m[5];
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
    }
    let feet = [(min_x + max_x) * 0.5, max_y];
    let size = [(max_x - min_x).max(1.0), (max_y - min_y).max(1.0)];
    (feet, size)
}

#[inline]
pub fn upright_size(piece: &CompositePiece) -> (f32, f32) {
    if piece.rotated {
        (piece.atlas_size[1] as f32, piece.atlas_size[0] as f32)
    } else {
        (piece.atlas_size[0] as f32, piece.atlas_size[1] as f32)
    }
}

fn sample_composite_timeline(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    anim_type: &str,
    prefix: &str,
) -> Vec<Vec<CompositePiece>> {
    if anim_type.eq_ignore_ascii_case("symbol") {
        if let Some(layers) = atlas.symbols.get(prefix) {
            return sample_layers_composite(atlas, indices, layers, None);
        }
    }
    let labels = atlas.frame_labels();
    if let Some(&(start, duration)) = labels.get(prefix) {
        return sample_layers_composite(atlas, indices, &atlas.main_layers, Some((start, start + duration)));
    }
    if let Some(layers) = atlas.symbols.get(prefix) {
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
    let mut out = Vec::with_capacity(end_t.saturating_sub(start_t));
    for t in start_t..end_t {
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
    for layer in layers.iter().rev() {
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
                    let child_tick = match elem.loop_mode.as_str() {
                        "SF" => elem.first_frame,
                        "PO" => (elem.first_frame + rel_tick).min(dur - 1),
                        _ => (elem.first_frame + rel_tick) % dur,
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

