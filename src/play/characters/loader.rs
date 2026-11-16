use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use bevy::prelude::*;

use crate::animations::atlas_composite::sample_default_composite_clip;
use crate::animations::base::add_sparrow_animation;
use crate::animations::{
    AnimationController, BaseAnimationData, add_packer_animation, register_default_sparrow_idle,
};
use crate::mods::{DiscoveredScript, ModRegistry, ScriptScope};
use crate::play::stage::StageResource;
use crate::play::stage::schema::StageCharacterSlot;
use crate::play::types::CharactersMeta;

use super::asset_cache::{
    AnimLoadTarget, CharAssetCtx, CharLoadCaches, get_or_load_animate, get_or_load_packer,
    get_or_load_sparrow, load_composite_anim, resolve_idle_size,
};
use super::behavior::{CharacterActor, ClipVisual, SheetSource};
use super::schema::{CharacterData, CharacterRenderType, CharacterRole, is_gameover_only_anim};
use super::spawn::spawn_composite_piece_entity;

#[derive(Debug, Clone)]
pub struct LoadedCharacter {
    pub id: String,
    pub role: CharacterRole,
    pub slot: StageCharacterSlot,
    pub json_path: PathBuf,
    pub data: CharacterData,
    pub controller: AnimationController,
    pub sources: Vec<SheetSource>,
    pub clips: HashMap<String, ClipVisual>,
    pub deferred_gameover_anims: Vec<BaseAnimationData>,
    pub idle_size: [f32; 2],
    pub max_composite_pieces: usize,
    pub facing_flip_x: bool,
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CharactersResource {
    pub characters: Vec<LoadedCharacter>,
}

pub fn load_characters_resource(
    mods: &ModRegistry,
    meta: &CharactersMeta,
    stage: Option<&StageResource>,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> CharactersResource {
    let slots = stage.map(|s| &s.characters);
    let mut characters = Vec::new();

    let entries = [
        (&meta.spectator, CharacterRole::Spectator, slots.map(|s| s.spectator.clone()).unwrap_or_else(StageCharacterSlot::default_for_spectator)),
        (&meta.opponents, CharacterRole::Opponent, slots.map(|s| s.opponent.clone()).unwrap_or_else(StageCharacterSlot::default_for_opponent)),
        (&meta.players, CharacterRole::Player, slots.map(|s| s.player.clone()).unwrap_or_else(StageCharacterSlot::default_for_player)),
    ];

    for (ids, role, slot) in entries {
        for char_id in ids {
            if let Ok(loaded) = load_single_character(mods, char_id, role, slot.clone(), asset_server, layouts) {
                characters.push(loaded);
            }
        }
    }
    CharactersResource { characters }
}

pub(super) fn partition_character_animations(
    animations: &[BaseAnimationData],
    starting_anim: &str,
) -> (Vec<BaseAnimationData>, Vec<BaseAnimationData>) {
    let mut initial = Vec::new();
    let mut deferred = Vec::new();
    for anim in animations {
        if is_gameover_only_anim(&anim.name, starting_anim) {
            deferred.push(anim.clone());
        } else {
            initial.push(anim.clone());
        }
    }
    // Safe fallback for death-only character definitions whose animations are all Game Over clips.
    if initial.is_empty() && !deferred.is_empty() {
        return (deferred, Vec::new());
    }
    (initial, deferred)
}

pub(super) fn max_composite_pieces_in(clips: &HashMap<String, ClipVisual>) -> usize {
    clips
        .values()
        .filter_map(|c| match c {
            ClipVisual::Composite { clip, .. } => clip.frames.iter().map(Vec::len).max(),
            ClipVisual::Sheet { .. } => None,
        })
        .max()
        .unwrap_or(0)
}

fn load_character_animations(
    ctx: &CharAssetCtx<'_>,
    render_type: CharacterRenderType,
    default_asset: &str,
    anims: &[BaseAnimationData],
    layouts: &mut Assets<TextureAtlasLayout>,
    caches: &mut CharLoadCaches,
    target: &mut AnimLoadTarget<'_>,
) {
    match render_type {
        CharacterRenderType::Packer => {
            for anim in anims {
                let sub_path = anim.asset_path.as_deref().unwrap_or(default_asset);
                let resolved = get_or_load_packer(ctx, sub_path, layouts, target.sources, caches)
                    .or_else(|| {
                        (sub_path != default_asset)
                            .then(|| {
                                get_or_load_packer(
                                    ctx,
                                    default_asset,
                                    layouts,
                                    target.sources,
                                    caches,
                                )
                            })
                            .flatten()
                    });
                if let Some((src_i, pk_i)) = resolved
                    && let Some((packer, indices)) = caches.packer_data.get(pk_i)
                    && add_packer_animation(packer, indices, target.controller, anim)
                {
                    target
                        .clips
                        .insert(anim.name.clone(), ClipVisual::Sheet { source_idx: src_i });
                }
            }
        }
        CharacterRenderType::Sparrow | CharacterRenderType::MultiSparrow => {
            for anim in anims {
                let sub_path = anim.asset_path.as_deref().unwrap_or(default_asset);
                if anim.render_type.eq_ignore_ascii_case("animateatlas") {
                    load_composite_anim(ctx, sub_path, anim, layouts, caches, target);
                } else {
                    let resolved =
                        get_or_load_sparrow(ctx, sub_path, layouts, target.sources, caches)
                            .or_else(|| {
                                (sub_path != default_asset)
                                    .then(|| {
                                        get_or_load_sparrow(
                                            ctx,
                                            default_asset,
                                            layouts,
                                            target.sources,
                                            caches,
                                        )
                                    })
                                    .flatten()
                            });
                    if let Some((src_i, sp_i)) = resolved
                        && let Some((atlas, indices)) = caches.sparrow_data.get(sp_i)
                        && add_sparrow_animation(atlas, indices, target.controller, anim)
                    {
                        target
                            .clips
                            .insert(anim.name.clone(), ClipVisual::Sheet { source_idx: src_i });
                    }
                }
            }
        }
        CharacterRenderType::AnimateAtlas | CharacterRenderType::MultiAnimateAtlas => {
            for anim in anims {
                let sub_path = anim.asset_path.as_deref().unwrap_or(default_asset);
                if anim.render_type.eq_ignore_ascii_case("sparrow") {
                    if let Some((src_i, sp_i)) =
                        get_or_load_sparrow(ctx, sub_path, layouts, target.sources, caches)
                        && let Some((atlas, indices)) = caches.sparrow_data.get(sp_i)
                        && add_sparrow_animation(atlas, indices, target.controller, anim)
                    {
                        target
                            .clips
                            .insert(anim.name.clone(), ClipVisual::Sheet { source_idx: src_i });
                    }
                } else {
                    load_composite_anim(ctx, sub_path, anim, layouts, caches, target);
                }
            }
        }
    }
}

pub fn load_single_character(
    mods: &ModRegistry,
    char_id: &str,
    role: CharacterRole,
    slot: StageCharacterSlot,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) -> Result<LoadedCharacter, String> {
    let json_path = mods
        .resolve_file(&format!("gameplay/characters/{char_id}/{char_id}.json"))
        .or_else(|| mods.resolve_file(&format!("characters/{char_id}/{char_id}.json")))
        .unwrap_or_else(|| {
            mods.assets_dir
                .join("gameplay")
                .join("characters")
                .join(char_id)
                .join(format!("{char_id}.json"))
        });
    let raw = fs::read_to_string(&json_path)
        .map_err(|e| format!("failed to read {}: {e}", json_path.display()))?;
    let data: CharacterData = serde_json::from_str(raw.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("failed to parse {}: {e}", json_path.display()))?;

    let default_asset = data
        .asset_path
        .clone()
        .unwrap_or_else(|| format!("gameplay/characters/{char_id}/{char_id}"));
    let (initial_anims, deferred_gameover_anims) =
        partition_character_animations(&data.animations, &data.starting_animation);

    let mut controller = AnimationController::default();
    let mut sources = Vec::new();
    let mut clips = HashMap::new();
    let mut caches = CharLoadCaches::default();
    let ctx = CharAssetCtx {
        mods,
        char_id,
        asset_server,
    };

    {
        let mut target = AnimLoadTarget {
            sources: &mut sources,
            controller: &mut controller,
            clips: &mut clips,
        };
        load_character_animations(
            &ctx,
            data.render_type,
            &default_asset,
            &initial_anims,
            layouts,
            &mut caches,
            &mut target,
        );
    }

    if clips.is_empty() {
        match data.render_type {
            CharacterRenderType::Sparrow | CharacterRenderType::MultiSparrow => {
                if let Some((idx, s_idx)) = get_or_load_sparrow(
                    &ctx,
                    &default_asset,
                    layouts,
                    &mut sources,
                    &mut caches,
                ) && let Some((atlas, indices)) = caches.sparrow_data.get(s_idx)
                    && register_default_sparrow_idle(atlas, indices, &mut controller)
                {
                    clips.insert("idle".to_string(), ClipVisual::Sheet { source_idx: idx });
                }
            }
            CharacterRenderType::AnimateAtlas | CharacterRenderType::MultiAnimateAtlas => {
                if let Some((src_i, atlas, indices)) = get_or_load_animate(
                    &ctx,
                    &default_asset,
                    layouts,
                    &mut sources,
                    &mut caches,
                ) && let Some(clip) = sample_default_composite_clip(&atlas, &indices)
                {
                    let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                    controller.add_anim("idle", frame_ids, clip.fps, true);
                    clips.insert(
                        "idle".to_string(),
                        ClipVisual::Composite {
                            source_idx: src_i,
                            clip,
                            stage_matrix: atlas.stage_matrix,
                        },
                    );
                }
            }
            CharacterRenderType::Packer => {}
        }
    }

    let idle_size = resolve_idle_size(
        &data.starting_animation,
        data.effective_atlas_settings(),
        &controller,
        &sources,
        &clips,
    );
    let max_composite_pieces = max_composite_pieces_in(&clips);
    let facing_flip_x = match role {
        CharacterRole::Player => !data.flip_x,
        CharacterRole::Opponent | CharacterRole::Spectator => data.flip_x,
    };

    Ok(LoadedCharacter {
        id: char_id.to_string(),
        role,
        slot,
        json_path,
        data,
        controller,
        sources,
        clips,
        deferred_gameover_anims,
        idle_size,
        max_composite_pieces,
        facing_flip_x,
    })
}

/// Lazily loads deferred Game Over animations for `actor` and spawns any extra composite
/// piece entities required by the newly registered clips.
pub fn ensure_character_gameover_anims_loaded(
    actor: &mut CharacterActor,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    commands: &mut Commands,
) {
    if actor.deferred_gameover_anims.is_empty() {
        return;
    }
    let deferred: Vec<BaseAnimationData> = actor.deferred_gameover_anims.drain(..).collect();
    let default_asset = actor
        .data
        .asset_path
        .clone()
        .unwrap_or_else(|| format!("gameplay/characters/{}/{}", actor.id, actor.id));
    let prev_sources_len = actor.sources.len();
    let mut caches = CharLoadCaches::default();
    let ctx = CharAssetCtx {
        mods,
        char_id: &actor.id,
        asset_server,
    };
    {
        let mut target = AnimLoadTarget {
            sources: &mut actor.sources,
            controller: &mut actor.controller,
            clips: &mut actor.clips,
        };
        load_character_animations(
            &ctx,
            actor.data.render_type,
            &default_asset,
            &deferred,
            layouts,
            &mut caches,
            &mut target,
        );
    }
    if actor.sources.len() > prev_sources_len {
        actor.sampler_applied = false;
    }
    let needed_pieces = max_composite_pieces_in(&actor.clips);
    if needed_pieces > actor.piece_entities.len() {
        actor
            .piece_entities
            .reserve(needed_pieces - actor.piece_entities.len());
        for i in actor.piece_entities.len()..needed_pieces {
            actor.piece_entities.push(spawn_composite_piece_entity(
                commands,
                &actor.sources,
                actor.slot.layer,
                i,
            ));
        }
    }
}

/// If a character JSON enables `"script": true` (or `"scripts": true`), discover sidecar
/// script files sharing the JSON's stem and directory for any supported script extension.
pub fn append_character_sidecar_scripts(
    mods: &ModRegistry,
    char_id: &str,
    json_path: &Path,
    supported_exts: &[String],
    discovered: &mut Vec<DiscoveredScript>,
) {
    let stem = json_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(char_id);
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
        let mut mod_id = "assets".to_string();
        let mut rel_path = candidate
            .strip_prefix(&mods.assets_dir)
            .unwrap_or(&candidate)
            .to_string_lossy()
            .replace('\\', "/");
        for m in &mods.mods {
            if let Ok(rel) = candidate.strip_prefix(&m.root) {
                mod_id = m.manifest.id.clone();
                rel_path = rel.to_string_lossy().replace('\\', "/");
                break;
            }
        }
        discovered.push(DiscoveredScript {
            id: format!("character:{char_id}:{stem}.{ext}"),
            mod_id,
            name: stem.to_string(),
            extension: ext.to_ascii_lowercase(),
            rel_path,
            path: candidate,
            scope: ScriptScope::Character(char_id.to_string()),
            priority: 0,
        });
    }
}
