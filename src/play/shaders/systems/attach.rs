use bevy::prelude::*;
use bevy::sprite_render::SpriteMaterial;

use crate::mods::ModRegistry;
use crate::play::camera::hud::HudCamera;
use crate::play::characters::CharacterActor;
use crate::play::scene::PlayState;
use crate::play::stage::{StageCompositeProp, StagePropVisual};
use crate::ui::LoadingOverlayCamera;

use super::super::defs::{stage_character_initial_tint, stage_character_shader, stage_prop_shader};
use super::super::material::FunkinSpriteMaterial;
use super::super::rain_post::RainCameraEffect;
use super::{
    ManagedSpriteShader, ShaderTargetKind, StageShaderRuntime, aux_path_for_config,
    load_aux_handle, to_char_shader_role,
};

#[allow(clippy::too_many_arguments)]
pub fn attach_stage_and_character_shaders(
    mut commands: Commands,
    mut state: ResMut<PlayState>,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    props: Query<(Entity, &StagePropVisual, Option<&StageCompositeProp>), Without<ManagedSpriteShader>>,
    mut actors: Query<&mut CharacterActor>,
    char_shaders: Query<(), With<ManagedSpriteShader>>,
    cameras: Query<
        (Entity, Option<&RainCameraEffect>),
        (With<Camera2d>, Without<HudCamera>, Without<LoadingOverlayCamera>),
    >,
) {
    let stage_id = state.chart.stage_id.clone();
    let song_id = state.chart.song_id.clone();
    if runtime.attached && runtime.stage_id == stage_id && runtime.song_id == song_id {
        return;
    }

    runtime.reset_for_stage(&stage_id, &song_id);
    runtime.attached = true;

    for (entity, prop, composite_opt) in &props {
        if let Some(mut cfg) = stage_prop_shader(&stage_id, &prop.name) {
            if stage_id == "sserafim" {
                cfg.sserafim.adjust = runtime.sserafim.adjust;
            }
            let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
            let aux_tex = load_aux_handle(&mods, &asset_server, aux_rel.as_deref());
            let handle = materials.add(FunkinSpriteMaterial::from_config(cfg, aux_tex));
            if let Some(composite) = composite_opt {
                for &piece_ent in &composite.piece_entities {
                    if piece_ent != Entity::PLACEHOLDER {
                        commands
                            .entity(piece_ent)
                            .insert(SpriteMaterial(handle.clone()));
                    }
                }
            }
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: ShaderTargetKind::StageProp,
                    char_id: String::new(),
                    prop_name: prop.name.clone(),
                    aux_path: aux_rel,
                },
            ));
        }
    }

    for mut actor in &mut actors {
        if actor.mirror_of.is_some() {
            continue;
        }
        let role = to_char_shader_role(actor.role);
        if let Some(tint) = stage_character_initial_tint(&stage_id, role) {
            actor.mod_tint = tint;
        }
        let Some(mut cfg) = stage_character_shader(&stage_id, role, &actor.id) else {
            continue;
        };
        if stage_id == "sserafim" {
            cfg.sserafim.adjust = runtime.sserafim.adjust;
        }
        let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
        let aux_tex = load_aux_handle(&mods, &asset_server, aux_rel.as_deref());
        let handle = materials.add(FunkinSpriteMaterial::from_config(cfg, aux_tex));

        let mut target_entities = Vec::with_capacity(1 + actor.piece_entities.len());
        if actor.sheet_entity != Entity::PLACEHOLDER {
            target_entities.push(actor.sheet_entity);
        }
        target_entities.extend(
            actor
                .piece_entities
                .iter()
                .copied()
                .filter(|&e| e != Entity::PLACEHOLDER),
        );

        for entity in target_entities {
            if char_shaders.contains(entity) {
                continue;
            }
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle: handle.clone(),
                    target: ShaderTargetKind::Character(role),
                    char_id: actor.id.clone(),
                    prop_name: String::new(),
                    aux_path: aux_rel.clone(),
                },
            ));
        }
    }

    if let Some(spec) = runtime.rain_spec {
        for (cam_entity, existing) in &cameras {
            if existing.is_none() {
                commands
                    .entity(cam_entity)
                    .insert(RainCameraEffect::from_spec(spec));
            }
        }
    }
    state.pending_shader_events.clear();
}
