use std::collections::HashMap;
use std::path::Path;

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::build_composite_clip;
use crate::animations::base::BaseAnimationData;
use crate::animations::{AnimationController, AtlasSettings, PackerAtlas, SparrowAtlas};
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
    let (png, xml) = char_rel_candidates(char_id, raw_path)
        .into_iter()
        .find_map(|rel_base| {
            let png = mods.resolve_file(&ensure_ext_str(&rel_base, "png"))?;
            let xml = mods.resolve_file(&ensure_ext_str(&rel_base, "xml"))?;
            Some((png, xml))
        })?;
    let atlas = SparrowAtlas::from_file(&xml).ok()?;
    let (layout, indices) = atlas.build_layout();
    let frame_sizes = atlas
        .frames
        .iter()
        .filter_map(|f| {
            let (unrot_w, unrot_h) = if f.rotated {
                (f.height, f.width)
            } else {
                (f.width, f.height)
            };
            indices.get(&f.name).copied().map(|idx| {
                (
                    idx,
                    [
                        f.frame_width.max(unrot_w) as f32,
                        f.frame_height.max(unrot_h) as f32,
                    ],
                )
            })
        })
        .collect();
    let texture = asset_server.load(mods.to_asset_server_path(&png));
    let layout_handle = layouts.add(layout);
    let src_idx = sources.len();
    sources.push(SheetSource {
        texture,
        layout: layout_handle,
        extra_layers: Vec::new(),
        frame_sizes,
    });
    let sp_idx = sparrow_data.len();
    sparrow_data.push((atlas, indices));
    sheet_cache.insert(key, (src_idx, sp_idx));
    Some((src_idx, sp_idx))
}

pub(super) type CachedAnimateAtlas = (usize, AnimateAtlas, HashMap<String, usize>);

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
    let dir = char_rel_candidates(char_id, raw_path)
        .into_iter()
        .find_map(|rel_dir| {
            let dir = mods.resolve_dir(&rel_dir)?;
            (dir.join("Animation.json").is_file() && dir.join("spritemap1.png").is_file())
                .then_some(dir)
        })?;
    let atlas = AnimateAtlas::from_dir(&dir).ok()?;
    let (atlas_layouts, indices) = atlas.build_layouts();
    let mut iter = atlas_layouts.into_iter().enumerate();
    let (_, first_layout) = iter.next()?;
    let texture = asset_server.load(mods.to_asset_server_path(&dir.join("spritemap1.png")));
    let layout_handle = layouts.add(first_layout);
    let extra_layers = iter
        .map(|(i, layout)| {
            let img_rel = mods.to_asset_server_path(&dir.join(format!("spritemap{}.png", i + 1)));
            (asset_server.load(img_rel), layouts.add(layout))
        })
        .collect();
    let src_idx = sources.len();
    sources.push(SheetSource {
        texture,
        layout: layout_handle,
        extra_layers,
        frame_sizes: HashMap::new(),
    });
    let val = (src_idx, atlas, indices);
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
    let (png, txt) = char_rel_candidates(char_id, raw_path)
        .into_iter()
        .find_map(|rel_base| {
            let png = mods.resolve_file(&ensure_ext_str(&rel_base, "png"))?;
            let txt = mods.resolve_file(&ensure_ext_str(&rel_base, "txt"))?;
            Some((png, txt))
        })?;
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
            extra_layers: Vec::new(),
            frame_sizes,
        },
        atlas,
        indices,
    ))
}

fn push_unique_char_norm(out: &mut Vec<String>, raw_rel: &str) {
    if let Ok(norm) = ModRegistry::normalize_rel_path(raw_rel) {
        let norm_str = norm.to_string_lossy().replace('\\', "/");
        if !norm_str.is_empty() && !out.iter().any(|existing| existing == &norm_str) {
            out.push(norm_str);
        }
    }
}

fn char_rel_candidates(char_id: &str, raw: &str) -> Vec<String> {
    let normalized = raw.trim().replace('\\', "/");
    let mut cleaned = normalized.as_str();
    loop {
        let next = cleaned.trim_start_matches("./").trim_start_matches('/');
        if next.len() == cleaned.len() {
            break;
        }
        cleaned = next;
    }
    if let Some(rest) = cleaned.strip_prefix("assets/") {
        cleaned = rest;
    }
    if cleaned.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if cleaned.starts_with("gameplay/") || cleaned.starts_with("characters/") {
        push_unique_char_norm(&mut out, cleaned);
        push_unique_char_norm(&mut out, &format!("gameplay/characters/{char_id}/{cleaned}"));
    } else {
        push_unique_char_norm(&mut out, &format!("gameplay/characters/{char_id}/{cleaned}"));
        push_unique_char_norm(&mut out, &format!("gameplay/characters/{cleaned}"));
        push_unique_char_norm(&mut out, &format!("characters/{char_id}/{cleaned}"));
        push_unique_char_norm(&mut out, &format!("characters/{cleaned}"));
        push_unique_char_norm(&mut out, cleaned);
    }
    out
}

pub fn char_rel_candidates_for_watch(char_id: &str, raw: &str) -> Vec<String> {
    char_rel_candidates(char_id, raw)
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
    let Some((src_i, atlas, indices)) =
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
    clips.insert(
        anim.name.clone(),
        ClipVisual::Composite {
            source_idx: src_i,
            clip,
            stage_matrix: atlas.stage_matrix,
        },
    );
}

pub(super) fn resolve_idle_size(
    starting_anim: &str,
    atlas_settings: AtlasSettings,
    controller: &AnimationController,
    sources: &[SheetSource],
    clips: &HashMap<String, ClipVisual>,
) -> [f32; 2] {
    for key in [starting_anim, "idle", "danceRight", "danceLeft"] {
        let Some(vis) = clips.get(key) else { continue };
        match vis {
            ClipVisual::Composite { clip, stage_matrix, .. } => {
                let mut sz = clip.bounds_size;
                if atlas_settings.apply_stage_matrix
                    && !atlas_settings.post_stage_matrix_apply
                    && let Some(sm) = stage_matrix
                {
                    sz = [
                        sz[0] * sm[0].abs().max(0.01),
                        sz[1] * sm[3].abs().max(0.01),
                    ];
                }
                return sz;
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
