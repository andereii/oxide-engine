use std::collections::HashMap;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{build_composite_clip, compute_composite_bounds};
use crate::animations::base::BaseAnimationData;
use crate::animations::{AnimationController, PackerAtlas, SparrowAtlas};
use crate::play::stage::paths::to_asset_rel;

use super::behavior::{ClipVisual, SheetSource};

pub(super) fn get_or_load_sparrow(
    assets_dir: &Path,
    char_id: &str,
    raw_path: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    sheet_cache: &mut HashMap<String, (usize, usize)>,
    sparrow_data: &mut Vec<(SparrowAtlas, HashMap<String, usize>)>,
) -> Option<(usize, usize)> {
    let key = raw_path.to_string();
    if let Some(&hit) = sheet_cache.get(&key) {
        return Some(hit);
    }
    let base = resolve_char_base(assets_dir, char_id, raw_path);
    let png = ensure_ext(&base, "png");
    let xml = ensure_ext(&base, "xml");
    if !png.is_file() || !xml.is_file() {
        return None;
    }
    let atlas = SparrowAtlas::from_file(&xml).ok()?;
    let (layout, indices) = atlas.build_layout();
    let frame_sizes = atlas
        .frames
        .iter()
        .filter_map(|f| {
            indices
                .get(&f.name)
                .copied()
                .map(|idx| (idx, [f.frame_width as f32, f.frame_height as f32]))
        })
        .collect();
    let texture = asset_server.load(to_asset_rel(assets_dir, &png));
    let layout_handle = layouts.add(layout);
    let src_idx = sources.len();
    sources.push(SheetSource {
        texture,
        layout: layout_handle,
        frame_sizes,
    });
    let sp_idx = sparrow_data.len();
    sparrow_data.push((atlas, indices));
    sheet_cache.insert(key, (src_idx, sp_idx));
    Some((src_idx, sp_idx))
}

pub(super) fn get_or_load_animate(
    assets_dir: &Path,
    char_id: &str,
    raw_path: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    atlas_cache: &mut HashMap<String, (usize, AnimateAtlas, HashMap<String, usize>)>,
) -> Option<(usize, AnimateAtlas, HashMap<String, usize>)> {
    if let Some(hit) = atlas_cache.get(raw_path) {
        return Some(hit.clone());
    }
    let dir = resolve_char_base(assets_dir, char_id, raw_path);
    let png = dir.join("spritemap1.png");
    if !dir.join("Animation.json").is_file() || !png.is_file() {
        return None;
    }
    let atlas = AnimateAtlas::from_dir(&dir).ok()?;
    let (layout, indices) = atlas.build_layout();
    let texture = asset_server.load(to_asset_rel(assets_dir, &png));
    let layout_handle = layouts.add(layout);
    let src_idx = sources.len();
    sources.push(SheetSource {
        texture,
        layout: layout_handle,
        frame_sizes: HashMap::new(),
    });
    let val = (src_idx, atlas, indices);
    atlas_cache.insert(raw_path.to_string(), val.clone());
    Some(val)
}

pub(super) fn load_packer_source(
    assets_dir: &Path,
    char_id: &str,
    raw_path: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<(SheetSource, PackerAtlas, HashMap<String, usize>)> {
    let base = resolve_char_base(assets_dir, char_id, raw_path);
    let png = ensure_ext(&base, "png");
    let txt = ensure_ext(&base, "txt");
    if !png.is_file() || !txt.is_file() {
        return None;
    }
    let atlas = PackerAtlas::from_file(&txt).ok()?;
    let (layout, indices) = atlas.build_layout();
    let frame_sizes = atlas
        .frames
        .iter()
        .filter_map(|f| {
            indices
                .get(&f.name)
                .copied()
                .map(|idx| (idx, [f.w as f32, f.h as f32]))
        })
        .collect();
    let texture = asset_server.load(to_asset_rel(assets_dir, &png));
    let layout_handle = layouts.add(layout);
    Some((
        SheetSource {
            texture,
            layout: layout_handle,
            frame_sizes,
        },
        atlas,
        indices,
    ))
}

fn resolve_char_base(assets_dir: &Path, char_id: &str, raw: &str) -> PathBuf {
    let cleaned = raw.trim().trim_start_matches("./").trim_start_matches('/');
    if cleaned.starts_with("gameplay/") {
        assets_dir.join(cleaned)
    } else {
        assets_dir
            .join("gameplay")
            .join("characters")
            .join(char_id)
            .join(cleaned)
    }
}

fn ensure_ext(path: &Path, ext: &str) -> PathBuf {
    let has_known = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "png" | "xml" | "txt" | "jpg" | "webp"
            )
        });
    if has_known {
        path.with_extension(ext)
    } else {
        PathBuf::from(format!("{}.{}", path.display(), ext))
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load_composite_anim(
    assets_dir: &Path,
    char_id: &str,
    rel_path: &str,
    anim: &BaseAnimationData,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    atlas_cache: &mut HashMap<String, (usize, AnimateAtlas, HashMap<String, usize>)>,
    controller: &mut AnimationController,
    clips: &mut HashMap<String, ClipVisual>,
) {
    let Some((src_i, atlas, indices)) =
        get_or_load_animate(assets_dir, char_id, rel_path, asset_server, layouts, sources, atlas_cache)
    else {
        return;
    };
    let Some(clip) = build_composite_clip(&atlas, &indices, anim) else {
        return;
    };
    let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
    controller.add_anim(&anim.name, frame_ids, clip.fps, clip.looped);
    controller.set_offset(&anim.name, clip.offset);
    controller.set_flip(&anim.name, clip.flip);
    let first = clip.frames.first().map(Vec::as_slice).unwrap_or(&[]);
    let (feet_anchor, _) = compute_composite_bounds(first);
    clips.insert(anim.name.clone(), ClipVisual::Composite { source_idx: src_i, clip, feet_anchor });
}

pub(super) fn finalize_composite_feet(clips: &mut HashMap<String, ClipVisual>, starting_anim: &str) {
    let mut ref_by_source: HashMap<usize, [f32; 2]> = HashMap::new();
    for key in [starting_anim, "idle", "danceRight", "danceLeft"] {
        if let Some(ClipVisual::Composite { source_idx, feet_anchor, .. }) = clips.get(key) {
            ref_by_source.entry(*source_idx).or_insert(*feet_anchor);
        }
    }
    for vis in clips.values_mut() {
        if let ClipVisual::Composite { source_idx, feet_anchor, .. } = vis
            && let Some(&shared) = ref_by_source.get(source_idx)
        {
            *feet_anchor = shared;
        }
    }
}

pub(super) fn resolve_idle_size(
    starting_anim: &str,
    controller: &AnimationController,
    sources: &[SheetSource],
    clips: &HashMap<String, ClipVisual>,
) -> [f32; 2] {
    for key in [starting_anim, "idle", "danceRight", "danceLeft"] {
        let Some(vis) = clips.get(key) else { continue };
        match vis {
            ClipVisual::Composite { clip, .. } => {
                if let Some(first) = clip.frames.first() {
                    return compute_composite_bounds(first).1;
                }
            }
            ClipVisual::Sheet { source_idx } => {
                if let Some(def) = controller.animations.get(key)
                    && let Some(&first_id) = def.frames.first()
                    && let Some(&sz) = sources.get(*source_idx).and_then(|s| s.frame_sizes.get(&first_id))
                {
                    return sz;
                }
            }
        }
    }
    [300.0, 400.0]
}
