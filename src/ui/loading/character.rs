use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::prelude::*;
use oxin_app::resolve_assets_dir;
use serde::Deserialize;

use crate::animations::base::add_sparrow_animation;
use crate::animations::{AnimationController, BaseAnimationData, SparrowAtlas};
use crate::mods::ModRegistry;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};

const CHARACTERS_REL_DIR: &str = "ui/loading/characters";
const DEFAULT_CHAR_REL: &str = "ui/loading/characters/bf/bf";
const DEFAULT_ANIM_NAME: &str = "idle";
const DEFAULT_ANIM_PREFIX: &str = "run";
const DEFAULT_ANIM_FPS: f32 = 24.0;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub(super) struct LoadingCharacterData {
    pub(super) offsets: [f32; 2],
    pub(super) scale: f32,
    pub(super) weight: f32,
}

impl Default for LoadingCharacterData {
    fn default() -> Self {
        Self {
            offsets: [40.0, 105.0],
            scale: 0.38,
            weight: 400.0,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct LoadingCharacterEntry {
    pub(super) stem_path: PathBuf,
    pub(super) data: LoadingCharacterData,
}

pub(super) struct LoadedCharacterVisual {
    pub(super) texture: Handle<Image>,
    pub(super) layout: Handle<TextureAtlasLayout>,
    pub(super) controller: AnimationController,
    pub(super) scale: f32,
    pub(super) base_center_x: f32,
    pub(super) base_center_y: f32,
}

pub(super) fn pick_and_load_character(
    mods: Option<&ModRegistry>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<LoadedCharacterVisual> {
    let entries = discover_character_entries(mods);
    let chosen = pick_weighted_entry(&entries);

    if let Some(entry) = chosen
        && let Some(loaded) = try_load_entry(entry, mods, asset_server, layouts)
    {
        return Some(loaded);
    }

    let assets_dir = mods
        .map(|m| m.assets_dir.clone())
        .unwrap_or_else(resolve_assets_dir);
    let fallback = LoadingCharacterEntry {
        stem_path: assets_dir.join(DEFAULT_CHAR_REL),
        data: LoadingCharacterData::default(),
    };
    try_load_entry(&fallback, mods, asset_server, layouts)
}

fn discover_character_entries(mods: Option<&ModRegistry>) -> Vec<LoadingCharacterEntry> {
    let mut entries = Vec::new();
    let assets_dir = mods
        .map(|m| m.assets_dir.clone())
        .unwrap_or_else(resolve_assets_dir);

    collect_entries_in_dir(&assets_dir.join(CHARACTERS_REL_DIR), &mut entries);
    if let Some(reg) = mods {
        for loaded_mod in &reg.mods {
            collect_entries_in_dir(&loaded_mod.root.join(CHARACTERS_REL_DIR), &mut entries);
        }
    }
    entries
}

fn collect_entries_in_dir(dir: &Path, out: &mut Vec<LoadingCharacterEntry>) {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_entries_in_dir(&path, out);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(raw) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(data) = serde_json::from_str::<LoadingCharacterData>(&raw) else {
            continue;
        };
        let stem_path = path.with_extension("");
        if stem_path.with_extension("xml").is_file() && stem_path.with_extension("png").is_file() {
            out.push(LoadingCharacterEntry { stem_path, data });
        }
    }
}

fn pick_weighted_entry(entries: &[LoadingCharacterEntry]) -> Option<&LoadingCharacterEntry> {
    if entries.is_empty() {
        return None;
    }
    let total_weight: f32 = entries.iter().map(|e| e.data.weight.max(0.0)).sum();
    if total_weight <= 0.0 {
        return entries.first();
    }

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let roll = ((nanos % 10_000) as f32 / 10_000.0) * total_weight;

    let mut acc = 0.0;
    for entry in entries {
        acc += entry.data.weight.max(0.0);
        if roll <= acc {
            return Some(entry);
        }
    }
    entries.last()
}

fn try_load_entry(
    entry: &LoadingCharacterEntry,
    mods: Option<&ModRegistry>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<LoadedCharacterVisual> {
    let xml_path = entry.stem_path.with_extension("xml");
    let png_path = entry.stem_path.with_extension("png");
    let atlas = SparrowAtlas::from_file(&xml_path).ok()?;
    let (layout, indices) = atlas.build_layout();
    let layout_handle = layouts.add(layout);

    let mut controller = AnimationController::default();
    let anim = BaseAnimationData {
        name: DEFAULT_ANIM_NAME.to_string(),
        prefix: DEFAULT_ANIM_PREFIX.to_string(),
        looped: true,
        frame_rate: DEFAULT_ANIM_FPS,
        ..default()
    };
    if !add_sparrow_animation(&atlas, &indices, &mut controller, &anim) {
        return None;
    }
    controller.play(DEFAULT_ANIM_NAME, true);

    let (frame_w, frame_h) = atlas
        .frames
        .first()
        .map(|f| (f.frame_width.max(1) as f32, f.frame_height.max(1) as f32))
        .unwrap_or((600.0, 600.0));
    let scale = entry.data.scale.max(0.01);
    let char_w = frame_w * scale;
    let char_h = frame_h * scale;
    let base_center_x = SCREEN_WIDTH * 0.5 - entry.data.offsets[0] - char_w * 0.5;
    let base_center_y = -SCREEN_HEIGHT * 0.5 + entry.data.offsets[1] + char_h * 0.5;

    let asset_rel = to_asset_path(&png_path, mods);
    let texture = asset_server.load(asset_rel);

    Some(LoadedCharacterVisual {
        texture,
        layout: layout_handle,
        controller,
        scale,
        base_center_x,
        base_center_y,
    })
}

fn to_asset_path(disk_path: &Path, mods: Option<&ModRegistry>) -> String {
    if let Some(reg) = mods {
        return reg.to_asset_server_path(disk_path);
    }
    let assets_dir = resolve_assets_dir();
    if let Ok(rel) = disk_path.strip_prefix(&assets_dir) {
        return rel.to_string_lossy().replace('\\', "/");
    }
    disk_path.to_string_lossy().replace('\\', "/")
}
