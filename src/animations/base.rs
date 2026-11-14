use std::collections::HashMap;

use serde::Deserialize;

use super::animate_atlas::{AnimateAtlas, AnimateTimelineLayer};
use super::controller::AnimationController;
use super::sparrow::SparrowAtlas;

/// Generic animation clip metadata matching Funkin's `AnimationData`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct BaseAnimationData {
    pub name: String,
    pub prefix: String,
    #[serde(rename = "assetPath", alias = "file")]
    pub asset_path: Option<String>,
    pub offsets: [f32; 2],
    pub looped: bool,
    #[serde(rename = "flipX", alias = "flip_x")]
    pub flip_x: bool,
    #[serde(rename = "flipY", alias = "flip_y")]
    pub flip_y: bool,
    pub flip: Option<[bool; 2]>,
    #[serde(rename = "frameRate", alias = "fps")]
    pub frame_rate: f32,
    #[serde(rename = "frameIndices", alias = "indices")]
    pub frame_indices: Vec<usize>,
    #[serde(rename = "animType")]
    pub anim_type: String,
    #[serde(rename = "renderType")]
    pub render_type: String,
}

impl Default for BaseAnimationData {
    fn default() -> Self {
        Self {
            name: String::new(),
            prefix: String::new(),
            asset_path: None,
            offsets: [0.0, 0.0],
            looped: false,
            flip_x: false,
            flip_y: false,
            flip: None,
            frame_rate: 24.0,
            frame_indices: Vec::new(),
            anim_type: "framelabel".to_string(),
            render_type: String::new(),
        }
    }
}

impl BaseAnimationData {
    pub fn resolved_flip(&self) -> [bool; 2] {
        let f = self.flip.unwrap_or([false, false]);
        [f[0] || self.flip_x, f[1] || self.flip_y]
    }
}

/// Registers a Sparrow animation on `controller` (supports `frameIndices` and prefix matching).
pub fn add_sparrow_animation(
    atlas: &SparrowAtlas,
    indices: &HashMap<String, usize>,
    controller: &mut AnimationController,
    anim: &BaseAnimationData,
) -> bool {
    if anim.name.is_empty() {
        return false;
    }
    let query_prefix = if anim.prefix.is_empty() { &anim.name } else { &anim.prefix };
    let (mut frames, mut offsets, mut rotated) = atlas.find_prefix_frames(indices, query_prefix);
    if frames.is_empty() {
        return false;
    }

    if !anim.frame_indices.is_empty() {
        let (sub_f, sub_o, sub_r) = slice_by_indices(&frames, &offsets, &rotated, &anim.frame_indices);
        frames = sub_f;
        offsets = sub_o;
        rotated = sub_r;
    }
    if frames.is_empty() {
        return false;
    }

    let fps = if anim.frame_rate > 0.0 { anim.frame_rate } else { 24.0 };
    controller.add_anim_frames(&anim.name, frames, offsets, rotated, fps, anim.looped);
    controller.set_offset(&anim.name, anim.offsets);
    controller.set_flip(&anim.name, anim.resolved_flip());
    true
}

pub fn add_sparrow_animations(
    atlas: &SparrowAtlas,
    indices: &HashMap<String, usize>,
    controller: &mut AnimationController,
    animations: &[BaseAnimationData],
) {
    for anim in animations {
        add_sparrow_animation(atlas, indices, controller, anim);
    }
}

/// Registers a default `"idle"` clip across all frames of a Sparrow atlas when no explicit animations exist.
pub fn register_default_sparrow_idle(
    atlas: &SparrowAtlas,
    indices: &HashMap<String, usize>,
    controller: &mut AnimationController,
) -> bool {
    let mut ordered: Vec<_> = atlas
        .frames
        .iter()
        .filter_map(|f| indices.get(&f.name).copied().map(|idx| (f, idx)))
        .collect();
    ordered.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    if ordered.is_empty() {
        return false;
    }
    let frames: Vec<usize> = ordered.iter().map(|(_, idx)| *idx).collect();
    let offsets: Vec<[f32; 2]> = ordered.iter().map(|(f, _)| f.trim_center_offset()).collect();
    let rotated: Vec<bool> = ordered.iter().map(|(f, _)| f.rotated).collect();
    controller.add_anim_frames("idle", frames, offsets, rotated, 24.0, true);
    true
}

/// Registers Adobe Animate Texture Atlas animations (`framelabel` or `symbol`).
pub fn add_animate_atlas_animations(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    controller: &mut AnimationController,
    animations: &[BaseAnimationData],
) {
    let sprite_map: HashMap<&str, bool> = atlas
        .sprites
        .iter()
        .map(|s| (s.name.as_str(), s.rotated))
        .collect();

    for anim in animations {
        if anim.name.is_empty() {
            continue;
        }
        let query = if anim.prefix.is_empty() { &anim.name } else { &anim.prefix };
        let timeline = collect_atlas_timeline(atlas, indices, &sprite_map, &anim.anim_type, query);
        if timeline.0.is_empty() {
            continue;
        }
        let (frames, offsets, rotated) = if anim.frame_indices.is_empty() {
            timeline
        } else {
            slice_by_indices(&timeline.0, &timeline.1, &timeline.2, &anim.frame_indices)
        };
        if frames.is_empty() {
            continue;
        }
        let fps = if anim.frame_rate > 0.0 {
            anim.frame_rate
        } else {
            atlas.framerate.max(1.0)
        };
        controller.add_anim_frames(&anim.name, frames, offsets, rotated, fps, anim.looped);
        controller.set_offset(&anim.name, anim.offsets);
        controller.set_flip(&anim.name, anim.resolved_flip());
    }
}

pub fn register_default_atlas_idle(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    controller: &mut AnimationController,
) -> bool {
    let sprite_map: HashMap<&str, bool> = atlas
        .sprites
        .iter()
        .map(|s| (s.name.as_str(), s.rotated))
        .collect();
    let (frames, offsets, rotated) = sample_layers(atlas, indices, &sprite_map, &atlas.main_layers, None);
    if frames.is_empty() {
        return false;
    }
    controller.add_anim_frames("idle", frames, offsets, rotated, atlas.framerate.max(1.0), true);
    true
}

fn slice_by_indices(
    frames: &[usize],
    offsets: &[[f32; 2]],
    rotated: &[bool],
    wanted: &[usize],
) -> (Vec<usize>, Vec<[f32; 2]>, Vec<bool>) {
    let mut out_f = Vec::with_capacity(wanted.len());
    let mut out_o = Vec::with_capacity(wanted.len());
    let mut out_r = Vec::with_capacity(wanted.len());
    for &idx in wanted {
        if let Some(&f) = frames.get(idx) {
            out_f.push(f);
            out_o.push(offsets.get(idx).copied().unwrap_or([0.0, 0.0]));
            out_r.push(rotated.get(idx).copied().unwrap_or(false));
        }
    }
    (out_f, out_o, out_r)
}

fn collect_atlas_timeline(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    sprite_map: &HashMap<&str, bool>,
    anim_type: &str,
    prefix: &str,
) -> (Vec<usize>, Vec<[f32; 2]>, Vec<bool>) {
    if anim_type.eq_ignore_ascii_case("symbol") {
        if let Some(layers) = atlas.symbols.get(prefix) {
            return sample_layers(atlas, indices, sprite_map, layers, None);
        }
    }
    let labels = atlas.frame_labels();
    if let Some(&(start, duration)) = labels.get(prefix) {
        return sample_layers(atlas, indices, sprite_map, &atlas.main_layers, Some((start, start + duration)));
    }
    if let Some(layers) = atlas.symbols.get(prefix) {
        return sample_layers(atlas, indices, sprite_map, layers, None);
    }
    (Vec::new(), Vec::new(), Vec::new())
}

fn sample_layers(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    sprite_map: &HashMap<&str, bool>,
    layers: &[AnimateTimelineLayer],
    range: Option<(usize, usize)>,
) -> (Vec<usize>, Vec<[f32; 2]>, Vec<bool>) {
    let max_end = layers
        .iter()
        .flat_map(|l| l.frames.iter())
        .map(|f| f.index + f.duration)
        .max()
        .unwrap_or(0);
    let (start_t, end_t) = range.unwrap_or((0, max_end));
    let mut out_f = Vec::new();
    let mut out_o = Vec::new();
    let mut out_r = Vec::new();

    for t in start_t..end_t {
        if let Some((idx, offset, rot)) = resolve_tick(atlas, indices, sprite_map, layers, t, 0) {
            out_f.push(idx);
            out_o.push(offset);
            out_r.push(rot);
        }
    }
    (out_f, out_o, out_r)
}

fn resolve_tick(
    atlas: &AnimateAtlas,
    indices: &HashMap<String, usize>,
    sprite_map: &HashMap<&str, bool>,
    layers: &[AnimateTimelineLayer],
    tick: usize,
    depth: u8,
) -> Option<(usize, [f32; 2], bool)> {
    if depth > 8 {
        return None;
    }
    for layer in layers {
        let Some(frame) = layer.frames.iter().find(|f| tick >= f.index && tick < f.index + f.duration) else {
            continue;
        };
        let local_tick = tick.saturating_sub(frame.index);
        for elem in &frame.elements {
            if let Some(name) = &elem.sprite_name {
                if let Some(&idx) = indices.get(name) {
                    let rot = sprite_map.get(name.as_str()).copied().unwrap_or(false);
                    return Some((idx, [0.0, 0.0], rot));
                }
            }
            if let Some(sym) = &elem.symbol_name {
                if let Some(sub_layers) = atlas.symbols.get(sym) {
                    if let Some(hit) = resolve_tick(atlas, indices, sprite_map, sub_layers, local_tick, depth + 1) {
                        return Some(hit);
                    }
                }
            }
        }
    }
    None
}
