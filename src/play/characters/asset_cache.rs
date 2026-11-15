use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{build_composite_clip, compute_atlas_whole_bounds};
use crate::animations::base::BaseAnimationData;
use crate::animations::{AnimationController, PackerAtlas, SparrowAtlas};
use crate::mods::ModRegistry;

use super::behavior::{ClipVisual, SheetSource};

pub(super) fn get_or_load_sparrow(
    mods: &ModRegistry,
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
    let rel_base = resolve_char_rel(char_id, raw_path);
    let png = mods.resolve_file(&ensure_ext_str(&rel_base, "png"))?;
    let xml = mods.resolve_file(&ensure_ext_str(&rel_base, "xml"))?;
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
    let texture = asset_server.load(mods.to_asset_server_path(&png));
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

pub(super) type CachedAnimateAtlas = (usize, AnimateAtlas, HashMap<String, usize>, [f32; 2], [f32; 2]);

pub(super) fn get_or_load_animate(
    mods: &ModRegistry,
    char_id: &str,
    raw_path: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    atlas_cache: &mut HashMap<String, CachedAnimateAtlas>,
) -> Option<CachedAnimateAtlas> {
    if let Some(hit) = atlas_cache.get(raw_path) {
        return Some(hit.clone());
    }
    let rel_dir = resolve_char_rel(char_id, raw_path);
    let dir = mods.resolve_dir(&rel_dir)?;
    let png = dir.join("spritemap1.png");
    if !dir.join("Animation.json").is_file() || !png.is_file() {
        return None;
    }
    let atlas = AnimateAtlas::from_dir(&dir).ok()?;
    let (layout, indices) = atlas.build_layout();
    let (bounds_min, bounds_size) = compute_atlas_whole_bounds(&atlas, &indices);
    let texture = asset_server.load(mods.to_asset_server_path(&png));
    let layout_handle = layouts.add(layout);
    let src_idx = sources.len();
    sources.push(SheetSource {
        texture,
        layout: layout_handle,
        frame_sizes: HashMap::new(),
    });
    let val = (src_idx, atlas, indices, bounds_min, bounds_size);
    atlas_cache.insert(raw_path.to_string(), val.clone());
    Some(val)
}

pub(super) fn load_packer_source(
    mods: &ModRegistry,
    char_id: &str,
    raw_path: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<(SheetSource, PackerAtlas, HashMap<String, usize>)> {
    let rel_base = resolve_char_rel(char_id, raw_path);
    let png = mods.resolve_file(&ensure_ext_str(&rel_base, "png"))?;
    let txt = mods.resolve_file(&ensure_ext_str(&rel_base, "txt"))?;
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
    let texture = asset_server.load(mods.to_asset_server_path(&png));
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

fn resolve_char_rel(char_id: &str, raw: &str) -> String {
    let cleaned = raw.trim().trim_start_matches("./").trim_start_matches('/');
    if cleaned.starts_with("gameplay/") || cleaned.starts_with("characters/") {
        cleaned.to_string()
    } else {
        format!("gameplay/characters/{char_id}/{cleaned}")
    }
}

fn ensure_ext_str(rel: &str, ext: &str) -> String {
    let path = Path::new(rel);
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
        path.with_extension(ext).to_string_lossy().replace('\\', "/")
    } else {
        format!("{rel}.{ext}")
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load_composite_anim(
    mods: &ModRegistry,
    char_id: &str,
    rel_path: &str,
    anim: &BaseAnimationData,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    atlas_cache: &mut HashMap<String, CachedAnimateAtlas>,
    controller: &mut AnimationController,
    clips: &mut HashMap<String, ClipVisual>,
) {
    let Some((src_i, atlas, indices, bounds_min, bounds_size)) =
        get_or_load_animate(mods, char_id, rel_path, asset_server, layouts, sources, atlas_cache)
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
    clips.insert(anim.name.clone(), ClipVisual::Composite { source_idx: src_i, clip, bounds_min, bounds_size });
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
            ClipVisual::Composite { bounds_size, .. } => return *bounds_size,
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
