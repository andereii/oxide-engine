use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::build_composite_clip;
use crate::animations::base::BaseAnimationData;
use crate::animations::{AnimationController, AtlasSettings, PackerAtlas, SparrowAtlas};
use crate::mods::ModRegistry;

use super::behavior::{ClipVisual, SheetSource};

pub(super) struct CharAssetCtx<'a> {
    pub mods: &'a ModRegistry,
    pub char_id: &'a str,
    pub asset_server: &'a AssetServer,
}

#[derive(Default)]
pub(super) struct CharLoadCaches {
    pub sheet_cache: HashMap<String, (usize, usize)>,
    pub sparrow_data: Vec<(SparrowAtlas, HashMap<String, usize>)>,
    pub atlas_cache: HashMap<String, CachedAnimateAtlas>,
    pub packer_cache: HashMap<String, (usize, usize)>,
    pub packer_data: Vec<(PackerAtlas, HashMap<String, usize>)>,
}

pub(super) struct AnimLoadTarget<'a> {
    pub sources: &'a mut Vec<SheetSource>,
    pub controller: &'a mut AnimationController,
    pub clips: &'a mut HashMap<String, ClipVisual>,
}

fn compute_sparrow_frame_sizes(
    atlas: &SparrowAtlas,
    indices: &HashMap<String, usize>,
) -> HashMap<usize, [f32; 2]> {
    atlas
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
        .collect()
}

pub(super) fn get_or_load_sparrow(
    ctx: &CharAssetCtx<'_>,
    raw_path: &str,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    caches: &mut CharLoadCaches,
) -> Option<(usize, usize)> {
    if let Some(&hit) = caches.sheet_cache.get(raw_path) {
        return Some(hit);
    }
    let (png, xml) = char_rel_candidates(ctx.char_id, raw_path)
        .into_iter()
        .find_map(|rel_base| {
            let png = ctx.mods.resolve_file(&ensure_ext_str(&rel_base, "png"))?;
            let xml = ctx.mods.resolve_file(&ensure_ext_str(&rel_base, "xml"))?;
            Some((png, xml))
        })?;
    let atlas = SparrowAtlas::from_file(&xml).ok()?;
    let asset_key = format!("sparrow:{}", xml.to_string_lossy());
    if let Some(existing_idx) = sources.iter().position(|s| s.asset_key == asset_key) {
        let (_, indices) = atlas.build_layout();
        let sp_idx = caches.sparrow_data.len();
        caches.sparrow_data.push((atlas, indices));
        let pair = (existing_idx, sp_idx);
        caches.sheet_cache.insert(raw_path.to_string(), pair);
        return Some(pair);
    }
    let (layout, indices) = atlas.build_layout();
    let frame_sizes = compute_sparrow_frame_sizes(&atlas, &indices);
    let texture = ctx
        .asset_server
        .load(ctx.mods.to_asset_server_path(&png));
    let layout_handle = layouts.add(layout);
    let src_idx = sources.len();
    sources.push(SheetSource {
        asset_key,
        texture,
        layout: layout_handle,
        extra_layers: Vec::new(),
        frame_sizes,
    });
    let sp_idx = caches.sparrow_data.len();
    caches.sparrow_data.push((atlas, indices));
    let pair = (src_idx, sp_idx);
    caches.sheet_cache.insert(raw_path.to_string(), pair);
    Some(pair)
}

pub(super) type CachedAnimateAtlas = (usize, Arc<AnimateAtlas>, Arc<HashMap<String, usize>>);

pub(crate) fn load_animate_textures(
    dir: &Path,
    atlas: &AnimateAtlas,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &mut Assets<Image>,
    mut preload: Option<&mut crate::play::preload::PlayPreloadTracker>,
) -> (
    Vec<(Handle<Image>, Handle<TextureAtlasLayout>)>,
    HashMap<String, usize>,
) {
    let (atlas_layouts, indices) = atlas.build_layouts();
    let disk_count = atlas.spritemap_count.max(1);
    let mut baked_iter = atlas.baked_bevy_images().into_iter();
    let textures = atlas_layouts
        .into_iter()
        .enumerate()
        .map(|(i, layout)| {
            let tex = if i < disk_count {
                let img_rel = mods.to_asset_server_path(&dir.join(format!("spritemap{}.png", i + 1)));
                let handle: Handle<Image> = asset_server.load(img_rel);
                if let Some(tr) = preload.as_deref_mut() {
                    tr.register_handle(handle.clone());
                }
                handle
            } else if let Some(baked_img) = baked_iter.next() {
                images.add(baked_img)
            } else {
                Handle::default()
            };
            (tex, layouts.add(layout))
        })
        .collect();
    (textures, indices)
}

pub(super) fn get_or_load_animate(
    ctx: &CharAssetCtx<'_>,
    raw_path: &str,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &mut Assets<Image>,
    sources: &mut Vec<SheetSource>,
    caches: &mut CharLoadCaches,
) -> Option<CachedAnimateAtlas> {
    if let Some(hit) = caches.atlas_cache.get(raw_path) {
        return Some(hit.clone());
    }
    let dir = char_rel_candidates(ctx.char_id, raw_path)
        .into_iter()
        .find_map(|rel_dir| {
            let dir = ctx.mods.resolve_dir(&rel_dir)?;
            (dir.join("Animation.json").is_file() && dir.join("spritemap1.png").is_file())
                .then_some(dir)
        })?;
    let atlas = Arc::new(AnimateAtlas::from_dir(&dir).ok()?);
    let asset_key = format!("animate:{}", dir.to_string_lossy());
    if let Some(existing_idx) = sources.iter().position(|s| s.asset_key == asset_key) {
        let (_, indices) = atlas.build_layouts();
        let val = (existing_idx, atlas, Arc::new(indices));
        caches.atlas_cache.insert(raw_path.to_string(), val.clone());
        return Some(val);
    }
    let (textures, indices) = load_animate_textures(
        &dir,
        &atlas,
        ctx.mods,
        ctx.asset_server,
        layouts,
        images,
        None,
    );
    let mut iter = textures.into_iter();
    let (texture, layout_handle) = iter.next()?;
    let extra_layers = iter.collect();
    let src_idx = sources.len();
    sources.push(SheetSource {
        asset_key,
        texture,
        layout: layout_handle,
        extra_layers,
        frame_sizes: HashMap::new(),
    });
    let val = (src_idx, atlas, Arc::new(indices));
    caches.atlas_cache.insert(raw_path.to_string(), val.clone());
    Some(val)
}

pub(super) fn get_or_load_packer(
    ctx: &CharAssetCtx<'_>,
    raw_path: &str,
    layouts: &mut Assets<TextureAtlasLayout>,
    sources: &mut Vec<SheetSource>,
    caches: &mut CharLoadCaches,
) -> Option<(usize, usize)> {
    if let Some(&hit) = caches.packer_cache.get(raw_path) {
        return Some(hit);
    }
    let (png, txt) = char_rel_candidates(ctx.char_id, raw_path)
        .into_iter()
        .find_map(|rel_base| {
            let png = ctx.mods.resolve_file(&ensure_ext_str(&rel_base, "png"))?;
            let txt = ctx.mods.resolve_file(&ensure_ext_str(&rel_base, "txt"))?;
            Some((png, txt))
        })?;
    let atlas = PackerAtlas::from_file(&txt).ok()?;
    let asset_key = format!("packer:{}", txt.to_string_lossy());
    if let Some(existing_idx) = sources.iter().position(|s| s.asset_key == asset_key) {
        let (_, indices) = atlas.build_layout();
        let pk_idx = caches.packer_data.len();
        caches.packer_data.push((atlas, indices));
        let pair = (existing_idx, pk_idx);
        caches.packer_cache.insert(raw_path.to_string(), pair);
        return Some(pair);
    }
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
    let texture = ctx
        .asset_server
        .load(ctx.mods.to_asset_server_path(&png));
    let layout_handle = layouts.add(layout);
    let src_idx = sources.len();
    sources.push(SheetSource {
        asset_key,
        texture,
        layout: layout_handle,
        extra_layers: Vec::new(),
        frame_sizes,
    });
    let pk_idx = caches.packer_data.len();
    caches.packer_data.push((atlas, indices));
    let pair = (src_idx, pk_idx);
    caches.packer_cache.insert(raw_path.to_string(), pair);
    Some(pair)
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

pub(super) fn load_composite_anim(
    ctx: &CharAssetCtx<'_>,
    rel_path: &str,
    anim: &BaseAnimationData,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &mut Assets<Image>,
    caches: &mut CharLoadCaches,
    target: &mut AnimLoadTarget<'_>,
) {
    let Some((src_i, atlas, indices)) =
        get_or_load_animate(ctx, rel_path, layouts, images, target.sources, caches)
    else {
        return;
    };
    let Some(clip) = build_composite_clip(&atlas, &indices, anim) else {
        return;
    };
    let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
    target
        .controller
        .add_anim(&anim.name, frame_ids, clip.fps, clip.looped);
    target.controller.set_offset(&anim.name, clip.offset);
    target.controller.set_flip(&anim.name, clip.flip);
    target.clips.insert(
        anim.name.clone(),
        ClipVisual::Composite {
            source_idx: src_i,
            clip,
            stage_matrix: atlas.stage_matrix,
            atlas: Some(atlas),
            indices: Some(indices),
            anim_data: Some(anim.clone()),
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
