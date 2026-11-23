use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{
    CompositeClip, build_composite_clip, compute_atlas_whole_bounds,
    sample_default_composite_clip,
};
use crate::animations::{
    AnimationController, AtlasSettings, SparrowAtlas, add_sparrow_animations,
    register_default_sparrow_idle,
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
        frame_sizes: HashMap<usize, [f32; 2]>,
    },
    CompositeAtlas {
        textures: Vec<(Handle<Image>, Handle<TextureAtlasLayout>)>,
        controller: AnimationController,
        clips: HashMap<String, CompositeClip>,
        bounds_min: [f32; 2],
        bounds_size: [f32; 2],
        atlas_settings: AtlasSettings,
        stage_matrix: Option<[f32; 6]>,
        max_pieces: usize,
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

pub fn begin_stage_load(
    mods: &ModRegistry,
    stage_id: &str,
) -> Result<(StageResource, Vec<StagePropData>), String> {
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
    let ordered_props: Vec<StagePropData> = indexed.into_iter().map(|(_, _, data)| data).collect();

    let stage = StageResource {
        stage_id: stage_id.to_string(),
        json_path,
        scripts: parsed.scripts,
        name: parsed.name,
        camera_zoom: parsed.camera_zoom,
        coord_mode: parsed.coord_mode,
        characters: parsed.characters,
        props: Vec::with_capacity(ordered_props.len()),
    };
    Ok((stage, ordered_props))
}

pub fn resolve_stage_prop(
    mods: &ModRegistry,
    stage_id: &str,
    data: StagePropData,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &mut Assets<Image>,
) -> Option<ResolvedStageProp> {
    resolve_prop_visual(mods, stage_id, &data, asset_server, layouts, images)
        .map(|visual| ResolvedStageProp { data, visual })
}

#[allow(dead_code)]
pub fn load_stage_resource(
    mods: &ModRegistry,
    stage_id: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &mut Assets<Image>,
) -> Result<StageResource, String> {
    let (mut stage, ordered_props) = begin_stage_load(mods, stage_id)?;
    for data in ordered_props {
        if let Some(prop) = resolve_stage_prop(mods, stage_id, data, asset_server, layouts, images) {
            stage.props.push(prop);
        }
    }
    Ok(stage)
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
    images: &mut Assets<Image>,
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
            load_composite_stage_prop(mods, stage_id, prop, file, asset_server, layouts, images)
        }
        Some(StageAnimKind::Sparrow) => {
            if let Some(img_path) = resolve_stage_image_in(mods, stage_id, file) {
                let xml_path = img_path.with_extension("xml");
                if xml_path.is_file()
                    && let Ok(atlas) = SparrowAtlas::from_file(&xml_path)
                {
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
                    let frame_sizes: HashMap<usize, [f32; 2]> = atlas
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
                    let frame_size = controller
                        .current_atlas_index()
                        .and_then(|idx| frame_sizes.get(&idx).copied())
                        .or_else(|| {
                            atlas.frames.first().map(|f| {
                                let (unrot_w, unrot_h) = if f.rotated {
                                    (f.height, f.width)
                                } else {
                                    (f.width, f.height)
                                };
                                [
                                    f.frame_width.max(unrot_w) as f32,
                                    f.frame_height.max(unrot_h) as f32,
                                ]
                            })
                        })
                        .unwrap_or([0.0, 0.0]);
                    return Some(StagePropKind::AnimatedAtlas {
                        texture: asset_server.load(mods.to_asset_server_path(&img_path)),
                        layout: layouts.add(layout),
                        controller,
                        frame_size,
                        frame_sizes,
                    });
                }
                if !xml_path.is_file()
                    && let Some(comp) = load_composite_stage_prop(
                        mods,
                        stage_id,
                        prop,
                        file,
                        asset_server,
                        layouts,
                        images,
                    )
                {
                    return Some(comp);
                }
                let size = read_image_dimensions(&img_path).unwrap_or([0.0, 0.0]);
                return Some(StagePropKind::StaticImage {
                    texture: asset_server.load(mods.to_asset_server_path(&img_path)),
                    size,
                });
            }
            load_composite_stage_prop(mods, stage_id, prop, file, asset_server, layouts, images)
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

fn load_composite_stage_prop(
    mods: &ModRegistry,
    stage_id: &str,
    prop: &StagePropData,
    file: &str,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    images: &mut Assets<Image>,
) -> Option<StagePropKind> {
    let dir = resolve_atlas_dir_in(mods, stage_id, file)?;
    let atlas = AnimateAtlas::from_dir(&dir).ok()?;
    let (textures, indices) = crate::play::characters::load_animate_textures(
        &dir,
        &atlas,
        mods,
        asset_server,
        layouts,
        images,
        None,
    );
    if textures.is_empty() {
        return None;
    }

    let mut controller = AnimationController::default();
    let mut clips = HashMap::new();
    if prop.animations.is_empty() {
        if let Some(clip) = sample_default_composite_clip(&atlas, &indices) {
            let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
            controller.add_anim("idle", frame_ids, clip.fps, clip.looped);
            clips.insert("idle".to_string(), clip);
        }
    } else {
        for anim in &prop.animations {
            if let Some(clip) = build_composite_clip(&atlas, &indices, anim) {
                let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                controller.add_anim(&anim.name, frame_ids, clip.fps, clip.looped);
                controller.set_offset(&anim.name, clip.offset);
                controller.set_flip(&anim.name, clip.flip);
                clips.insert(anim.name.clone(), clip);
            }
        }
    }
    let start_anim = pick_starting_anim(prop, &controller);
    if let Some(anim) = &start_anim {
        controller.play(anim, true, false, 0);
    }
    let max_pieces = clips
        .values()
        .flat_map(|c| c.frames.iter())
        .map(Vec::len)
        .max()
        .unwrap_or(1)
        .max(1);
    let ref_clip = start_anim
        .as_deref()
        .and_then(|n| clips.get(n))
        .or_else(|| clips.get("idle"))
        .or_else(|| clips.values().next());
    let (bounds_min, mut bounds_size) = ref_clip
        .map(|c| (c.bounds_min, c.bounds_size))
        .unwrap_or_else(|| compute_atlas_whole_bounds(&atlas, &indices));
    if prop.atlas_settings.apply_stage_matrix
        && !prop.atlas_settings.post_stage_matrix_apply
        && let Some(sm) = atlas.stage_matrix
    {
        bounds_size = [
            bounds_size[0] * sm[0].abs().max(0.01),
            bounds_size[1] * sm[3].abs().max(0.01),
        ];
    }

    Some(StagePropKind::CompositeAtlas {
        textures,
        controller,
        clips,
        bounds_min,
        bounds_size,
        atlas_settings: prop.atlas_settings,
        stage_matrix: atlas.stage_matrix,
        max_pieces,
    })
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
