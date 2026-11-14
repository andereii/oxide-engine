use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use oxin_app::resolve_assets_dir;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{compute_composite_bounds, sample_default_composite_clip};
use crate::animations::base::add_sparrow_animation;
use crate::animations::{AnimationController, SparrowAtlas, add_packer_animation, register_default_sparrow_idle};
use crate::play::stage::StageResource;
use crate::play::stage::schema::StageCharacterSlot;
use crate::play::types::CharactersMeta;

use super::asset_cache::{
    finalize_composite_feet, get_or_load_animate, get_or_load_sparrow, load_composite_anim,
    load_packer_source, resolve_idle_size,
};
use super::behavior::{ClipVisual, SheetSource};
use super::schema::{CharacterData, CharacterRenderType, CharacterRole};

#[derive(Debug, Clone)]
pub struct LoadedCharacter {
    pub id: String,
    pub role: CharacterRole,
    pub slot: StageCharacterSlot,
    pub data: CharacterData,
    pub controller: AnimationController,
    pub sources: Vec<SheetSource>,
    pub clips: HashMap<String, ClipVisual>,
    pub idle_size: [f32; 2],
    pub max_composite_pieces: usize,
    pub facing_flip_x: bool,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CharactersResource {
    pub characters: Vec<LoadedCharacter>,
}

pub fn load_characters_resource(
    meta: &CharactersMeta,
    stage: Option<&StageResource>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> CharactersResource {
    let assets_dir = resolve_assets_dir();
    let slots = stage.map(|s| &s.characters);
    let mut characters = Vec::new();

    let entries = [
        (&meta.spectator, CharacterRole::Spectator, slots.map(|s| s.spectator.clone()).unwrap_or_else(StageCharacterSlot::default_for_spectator)),
        (&meta.opponents, CharacterRole::Opponent, slots.map(|s| s.opponent.clone()).unwrap_or_else(StageCharacterSlot::default_for_opponent)),
        (&meta.players, CharacterRole::Player, slots.map(|s| s.player.clone()).unwrap_or_else(StageCharacterSlot::default_for_player)),
    ];

    for (ids, role, slot) in entries {
        for char_id in ids {
            if let Ok(loaded) = load_single_character(&assets_dir, char_id, role, slot.clone(), asset_server, layouts) {
                characters.push(loaded);
            }
        }
    }
    CharactersResource { characters }
}

fn load_single_character(
    assets_dir: &Path,
    char_id: &str,
    role: CharacterRole,
    slot: StageCharacterSlot,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Result<LoadedCharacter, String> {
    let json_path = assets_dir.join("gameplay").join("characters").join(char_id).join(format!("{char_id}.json"));
    let raw = fs::read_to_string(&json_path).map_err(|e| format!("failed to read {}: {e}", json_path.display()))?;
    let data: CharacterData = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("failed to parse {}: {e}", json_path.display()))?;

    let default_asset = data.asset_path.clone().unwrap_or_else(|| format!("gameplay/characters/{char_id}/{char_id}"));
    let mut controller = AnimationController::default();
    let mut sources = Vec::new();
    let mut clips = HashMap::new();
    let mut sheet_cache: HashMap<String, (usize, usize)> = HashMap::new();
    let mut sparrow_data: Vec<(SparrowAtlas, HashMap<String, usize>)> = Vec::new();
    let mut atlas_cache: HashMap<String, (usize, AnimateAtlas, HashMap<String, usize>)> = HashMap::new();

    match data.render_type {
        CharacterRenderType::Packer => {
            if let Some((src, packer, indices)) = load_packer_source(assets_dir, char_id, &default_asset, asset_server, layouts) {
                sources.push(src);
                for anim in &data.animations {
                    if add_packer_animation(&packer, &indices, &mut controller, anim) {
                        clips.insert(anim.name.clone(), ClipVisual::Sheet { source_idx: 0 });
                    }
                }
            }
        }
        CharacterRenderType::Sparrow | CharacterRenderType::MultiSparrow => {
            if let Some((idx, s_idx)) = get_or_load_sparrow(assets_dir, char_id, &default_asset, asset_server, layouts, &mut sources, &mut sheet_cache, &mut sparrow_data) {
                for anim in &data.animations {
                    let sub_path = anim.asset_path.as_deref().unwrap_or(&default_asset);
                    if anim.render_type.eq_ignore_ascii_case("animateatlas") {
                        load_composite_anim(assets_dir, char_id, sub_path, anim, asset_server, layouts, &mut sources, &mut atlas_cache, &mut controller, &mut clips);
                    } else {
                        let (src_i, sp_i) = if anim.asset_path.is_some() {
                            get_or_load_sparrow(assets_dir, char_id, sub_path, asset_server, layouts, &mut sources, &mut sheet_cache, &mut sparrow_data).unwrap_or((idx, s_idx))
                        } else {
                            (idx, s_idx)
                        };
                        let (ref atlas, ref indices) = sparrow_data[sp_i];
                        if add_sparrow_animation(atlas, indices, &mut controller, anim) {
                            clips.insert(anim.name.clone(), ClipVisual::Sheet { source_idx: src_i });
                        }
                    }
                }
                if clips.is_empty() {
                    let (ref atlas, ref indices) = sparrow_data[s_idx];
                    if register_default_sparrow_idle(atlas, indices, &mut controller) {
                        clips.insert("idle".to_string(), ClipVisual::Sheet { source_idx: idx });
                    }
                }
            }
        }
        CharacterRenderType::AnimateAtlas | CharacterRenderType::MultiAnimateAtlas => {
            for anim in &data.animations {
                let sub_path = anim.asset_path.as_deref().unwrap_or(&default_asset);
                if anim.render_type.eq_ignore_ascii_case("sparrow") {
                    if let Some((src_i, sp_i)) = get_or_load_sparrow(assets_dir, char_id, sub_path, asset_server, layouts, &mut sources, &mut sheet_cache, &mut sparrow_data) {
                        let (ref atlas, ref indices) = sparrow_data[sp_i];
                        if add_sparrow_animation(atlas, indices, &mut controller, anim) {
                            clips.insert(anim.name.clone(), ClipVisual::Sheet { source_idx: src_i });
                        }
                    }
                } else {
                    load_composite_anim(assets_dir, char_id, sub_path, anim, asset_server, layouts, &mut sources, &mut atlas_cache, &mut controller, &mut clips);
                }
            }
            if clips.is_empty()
                && let Some((src_i, atlas, indices)) = get_or_load_animate(assets_dir, char_id, &default_asset, asset_server, layouts, &mut sources, &mut atlas_cache)
                && let Some(clip) = sample_default_composite_clip(&atlas, &indices)
            {
                let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                let first = clip.frames.first().map(Vec::as_slice).unwrap_or(&[]);
                let (feet_anchor, _) = compute_composite_bounds(first);
                controller.add_anim("idle", frame_ids, clip.fps, true);
                clips.insert("idle".to_string(), ClipVisual::Composite { source_idx: src_i, clip, feet_anchor });
            }
        }
    }

    finalize_composite_feet(&mut clips, &data.starting_animation);
    let idle_size = resolve_idle_size(&data.starting_animation, &controller, &sources, &clips);
    let max_composite_pieces = clips
        .values()
        .filter_map(|c| match c {
            ClipVisual::Composite { clip, .. } => clip.frames.iter().map(Vec::len).max(),
            ClipVisual::Sheet { .. } => None,
        })
        .max()
        .unwrap_or(0);
    let facing_flip_x = match role {
        CharacterRole::Player => !data.flip_x,
        CharacterRole::Opponent | CharacterRole::Spectator => data.flip_x,
    };

    Ok(LoadedCharacter {
        id: char_id.to_string(),
        role,
        slot,
        data,
        controller,
        sources,
        clips,
        idle_size,
        max_composite_pieces,
        facing_flip_x,
    })
}
