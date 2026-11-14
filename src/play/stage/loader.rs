use std::fs;
use std::path::Path;

use bevy::prelude::*;
use oxin_app::resolve_assets_dir;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::{
    AnimationController, SparrowAtlas, add_animate_atlas_animations, add_sparrow_animations,
    register_default_atlas_idle, register_default_sparrow_idle,
};

use super::paths::{
    parse_hex_color, read_image_dimensions, resolve_atlas_dir, resolve_stage_image, to_asset_rel,
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
    pub name: String,
    pub camera_zoom: f32,
    pub coord_mode: StageCoordMode,
    pub characters: StageCharacters,
    pub props: Vec<ResolvedStageProp>,
}

pub fn load_stage_resource(
    stage_id: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Result<StageResource, String> {
    let assets_dir = resolve_assets_dir();
    let stage_dir = assets_dir.join("gameplay").join("stages").join(stage_id);
    let json_path = stage_dir.join(format!("{stage_id}.json"));

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
        if let Some(visual) = resolve_prop_visual(&assets_dir, &stage_dir, &data, asset_server, layouts) {
            props.push(ResolvedStageProp { data, visual });
        }
    }

    Ok(StageResource {
        stage_id: stage_id.to_string(),
        name: parsed.name,
        camera_zoom: parsed.camera_zoom,
        coord_mode: parsed.coord_mode,
        characters: parsed.characters,
        props,
    })
}

fn resolve_prop_visual(
    assets_dir: &Path,
    stage_dir: &Path,
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
            let dir = resolve_atlas_dir(assets_dir, stage_dir, file)?;
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
            let img_rel = to_asset_rel(assets_dir, &dir.join("spritemap1.png"));
            Some(StagePropKind::AnimatedAtlas {
                texture: asset_server.load(img_rel),
                layout: layouts.add(layout),
                controller,
                frame_size,
            })
        }
        Some(StageAnimKind::Sparrow) => {
            let img_path = resolve_stage_image(assets_dir, stage_dir, file)?;
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
                        texture: asset_server.load(to_asset_rel(assets_dir, &img_path)),
                        layout: layouts.add(layout),
                        controller,
                        frame_size,
                    });
                }
            }
            let size = read_image_dimensions(&img_path).unwrap_or([0.0, 0.0]);
            Some(StagePropKind::StaticImage {
                texture: asset_server.load(to_asset_rel(assets_dir, &img_path)),
                size,
            })
        }
        Some(StageAnimKind::Packer) | None => {
            let img_path = resolve_stage_image(assets_dir, stage_dir, file)?;
            let size = read_image_dimensions(&img_path).unwrap_or([0.0, 0.0]);
            Some(StagePropKind::StaticImage {
                texture: asset_server.load(to_asset_rel(assets_dir, &img_path)),
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
