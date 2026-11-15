use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::{
    AnimationController, SparrowAtlas, add_animate_atlas_animations, add_sparrow_animations,
    register_default_atlas_idle, register_default_sparrow_idle,
};
use crate::mods::{DiscoveredScript, ModRegistry, ScriptScope};
use crate::utils::constants::STAGES_ASSET_PATH;

use super::paths::{
    parse_hex_color, read_image_dimensions, resolve_atlas_dir_in, resolve_stage_image_in,
};
use super::schema::{StageAnimKind, StageCharacters, StageCoordMode, StagePropData, parse_stage_json};

#[derive(Debug, Clone)]
pub enum StagePropKind {
    StaticImage {
        texture: Handle<Image>,
        size: [f32; 2],
    },
    SolidColor {
        color: Color,
    },
    AnimatedAtlas {
        texture: Handle<Image>,
        layout: Handle<TextureAtlasLayout>,
        controller: AnimationController,
        frame_size: [f32; 2],
    },
}

#[derive(Debug, Clone)]
pub struct ResolvedStageProp {
    pub data: StagePropData,
    pub visual: StagePropKind,
}

#[derive(Resource, Debug, Clone)]
#[allow(dead_code)]
pub struct StageResource {
    pub stage_id: String,
    pub json_path: PathBuf,
    pub scripts: bool,
    pub name: String,
    pub camera_zoom: f32,
    pub coord_mode: StageCoordMode,
    pub characters: StageCharacters,
    pub props: Vec<ResolvedStageProp>,
}

pub fn load_stage_resource(
    mods: &ModRegistry,
    stage_id: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Result<StageResource, String> {
    let json_path = mods
        .resolve_file(&format!("{STAGES_ASSET_PATH}/{stage_id}/{stage_id}.json"))
        .or_else(|| mods.resolve_file(&format!("stages/{stage_id}/{stage_id}.json")))
        .unwrap_or_else(|| {
            mods.assets_dir
                .join(STAGES_ASSET_PATH)
                .join(stage_id)
                .join(format!("{stage_id}.json"))
        });

    let raw_json = fs::read_to_string(&json_path)
        .map_err(|e| format!("failed to read {}: {e}", json_path.display()))?;
    let mut parsed = parse_stage_json(stage_id, &raw_json)?;

    let mut indexed: Vec<(i32, usize, StagePropData)> = parsed
        .props
        .drain(..)
        .enumerate()
        .map(|(i, p)| (p.layer, i, p))
        .collect();
    indexed.sort_by_key(|(layer, idx, _)| (*layer, *idx));

    let mut props = Vec::with_capacity(indexed.len());
    for (_, _, data) in indexed {
        if let Some(visual) = resolve_prop_visual(mods, stage_id, &data, asset_server, layouts) {
            props.push(ResolvedStageProp { data, visual });
        }
    }

    Ok(StageResource {
        stage_id: stage_id.to_string(),
        json_path,
        scripts: parsed.scripts,
        name: parsed.name,
        camera_zoom: parsed.camera_zoom,
        coord_mode: parsed.coord_mode,
        characters: parsed.characters,
        props,
    })
}

/// If a stage JSON enables `"scripts": true`, discover sidecar script files
/// sharing the JSON's stem and directory for any supported script extension.
pub fn append_stage_sidecar_scripts(
    mods: &ModRegistry,
    stage_id: &str,
    json_path: &Path,
    supported_exts: &[String],
    discovered: &mut Vec<DiscoveredScript>,
) {
    let stem = json_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(stage_id);
    for ext in supported_exts {
        let candidate = json_path.with_extension(ext);
        if !candidate.is_file() {
            continue;
        }
        let canon = fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        let already_loaded = discovered.iter().any(|d| {
            fs::canonicalize(&d.path).unwrap_or_else(|_| d.path.clone()) == canon
        });
        if already_loaded {
            continue;
        }
        let rel_path = candidate
            .strip_prefix(&mods.assets_dir)
            .unwrap_or(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        discovered.push(DiscoveredScript {
            id: format!("stage:{stage_id}:{stem}.{ext}"),
            mod_id: "assets".to_string(),
            name: stem.to_string(),
            extension: ext.to_ascii_lowercase(),
            rel_path,
            path: candidate,
            scope: ScriptScope::Stage(stage_id.to_string()),
            priority: 0,
        });
    }
}

fn resolve_prop_visual(
    mods: &ModRegistry,
    stage_id: &str,
    prop: &StagePropData,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Option<StagePropKind> {
    let file = prop.file.trim();
    if file.is_empty() {
        return None;
    }
    if file.starts_with('#') || file.starts_with("0x") {
        return Some(StagePropKind::SolidColor {
            color: parse_hex_color(file),
        });
    }

    match prop.anim_kind {
        Some(StageAnimKind::Atlas) => {
            let dir = resolve_atlas_dir_in(mods, stage_id, file)?;
            let atlas = AnimateAtlas::from_dir(&dir).ok()?;
            let (layout, indices) = atlas.build_layout();
            let mut controller = AnimationController::default();
            if prop.animations.is_empty() {
                register_default_atlas_idle(&atlas, &indices, &mut controller);
            } else {
                add_animate_atlas_animations(&atlas, &indices, &mut controller, &prop.animations);
            }
            if let Some(anim) = pick_starting_anim(prop, &controller) {
                controller.play(&anim, true, false, 0);
            }
            let frame_size = atlas
                .sprites
                .first()
                .map(|s| [s.w as f32, s.h as f32])
                .unwrap_or([0.0, 0.0]);
            let img_rel = mods.to_asset_server_path(&dir.join("spritemap1.png"));
            Some(StagePropKind::AnimatedAtlas {
                texture: asset_server.load(img_rel),
                layout: layouts.add(layout),
                controller,
                frame_size,
            })
        }
        Some(StageAnimKind::Sparrow) => {
            let img_path = resolve_stage_image_in(mods, stage_id, file)?;
            let xml_path = img_path.with_extension("xml");
            if xml_path.is_file() {
                if let Ok(atlas) = SparrowAtlas::from_file(&xml_path) {
                    let (layout, indices) = atlas.build_layout();
                    let mut controller = AnimationController::default();
                    if prop.animations.is_empty() {
                        register_default_sparrow_idle(&atlas, &indices, &mut controller);
                    } else {
                        add_sparrow_animations(&atlas, &indices, &mut controller, &prop.animations);
                    }
                    if let Some(anim) = pick_starting_anim(prop, &controller) {
                        controller.play(&anim, true, false, 0);
                    }
                    let frame_size = atlas
                        .frames
                        .first()
                        .map(|f| [f.frame_width.max(f.width) as f32, f.frame_height.max(f.height) as f32])
                        .unwrap_or([0.0, 0.0]);
                    return Some(StagePropKind::AnimatedAtlas {
                        texture: asset_server.load(mods.to_asset_server_path(&img_path)),
                        layout: layouts.add(layout),
                        controller,
                        frame_size,
                    });
                }
            }
            let size = read_image_dimensions(&img_path).unwrap_or([0.0, 0.0]);
            Some(StagePropKind::StaticImage {
                texture: asset_server.load(mods.to_asset_server_path(&img_path)),
                size,
            })
        }
        Some(StageAnimKind::Packer) | None => {
            let img_path = resolve_stage_image_in(mods, stage_id, file)?;
            let size = read_image_dimensions(&img_path).unwrap_or([0.0, 0.0]);
            Some(StagePropKind::StaticImage {
                texture: asset_server.load(mods.to_asset_server_path(&img_path)),
                size,
            })
        }
    }
}

fn pick_starting_anim(prop: &StagePropData, controller: &AnimationController) -> Option<String> {
    if let Some(start) = &prop.starting_animation {
        if controller.has_animation(start) {
            return Some(start.clone());
        }
    }
    if controller.has_animation("idle") {
        return Some("idle".to_string());
    }
    prop.animations
        .first()
        .map(|a| a.name.clone())
        .filter(|n| controller.has_animation(n))
}
