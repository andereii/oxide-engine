use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::sprite_render::SpriteMaterial;

use crate::mods::ModRegistry;
use crate::play::camera::hud::HudCamera;
use crate::play::characters::behavior::{COMPOSITE_RT_SIZE, CompositeRenderTarget};
use crate::play::characters::spawn::CharacterPartVisual;
use crate::play::characters::CharacterActor;
use crate::play::scene::PlayState;
use crate::play::stage::schema::stage_layer_z;
use crate::play::stage::{DynamicScriptProp, StageCompositeProp, StagePropVisual};
use crate::play::types::PlayPhase;
use crate::ui::LoadingOverlayCamera;

use super::super::defs::{
    CharShaderRole, stage_character_initial_tint, stage_character_shader, stage_prop_shader,
};
use super::super::material::FunkinSpriteMaterial;
use super::super::rain_post::RainCameraEffect;
use super::{
    ManagedSpriteShader, ShaderTargetKind, StageShaderRuntime, aux_path_for_config,
    load_aux_handle, to_char_shader_role,
};

pub(crate) fn ensure_character_composite_rt(
    actor: &mut CharacterActor,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    runtime: &mut StageShaderRuntime,
) -> Entity {
    if let Some(ref rt) = actor.composite_rt {
        return rt.quad_entity;
    }
    let render_layer = runtime.next_rt_layer;
    runtime.next_rt_layer = runtime.next_rt_layer.saturating_add(1);
    let rt_image = images.add(Image::new_target_texture(
        COMPOSITE_RT_SIZE,
        COMPOSITE_RT_SIZE,
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let layer = RenderLayers::layer(render_layer);
    for &piece_ent in &actor.piece_entities {
        if piece_ent != Entity::PLACEHOLDER {
            commands.entity(piece_ent).insert(layer.clone());
        }
    }
    let cam_order = -10 - (render_layer as isize - 10);
    let camera_entity = commands
        .spawn((
            Camera2d,
            Camera {
                order: cam_order,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::from(rt_image.clone()),
            Msaa::Off,
            Tonemapping::None,
            layer,
        ))
        .id();
    let z_base = stage_layer_z(actor.slot.layer, 0);
    let quad_entity = commands
        .spawn((
            Sprite::from_image(rt_image.clone()),
            Transform::from_xyz(0.0, 0.0, z_base),
            Visibility::Hidden,
            CharacterPartVisual,
        ))
        .id();
    actor.composite_rt = Some(CompositeRenderTarget {
        image: rt_image,
        camera_entity,
        quad_entity,
        render_layer,
    });
    quad_entity
}

#[allow(clippy::too_many_arguments)]
pub fn attach_stage_and_character_shaders(
    mut commands: Commands,
    mut state: ResMut<PlayState>,
    mods: Res<ModRegistry>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    props: Query<(Entity, &StagePropVisual, Option<&StageCompositeProp>), Without<ManagedSpriteShader>>,
    dynamic_props: Query<(Entity, &DynamicScriptProp, Option<&StageCompositeProp>), Without<ManagedSpriteShader>>,
    mut actors: Query<&mut CharacterActor>,
    char_shaders: Query<(), With<ManagedSpriteShader>>,
    cameras: Query<
        (Entity, Option<&RainCameraEffect>),
        (With<Camera2d>, Without<HudCamera>, Without<LoadingOverlayCamera>),
    >,
) {
    if state.phase == PlayPhase::Loading {
        runtime.attached = false;
        return;
    }

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
        if !actor.piece_entities.is_empty()
            && actor.data.effective_atlas_settings().use_render_texture
        {
            ensure_character_composite_rt(&mut actor, &mut commands, &mut images, &mut runtime);
        }
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

        let mut target_entities = Vec::with_capacity(2);
        if actor.sheet_entity != Entity::PLACEHOLDER {
            let handle = materials.add(FunkinSpriteMaterial::from_config(
                cfg.clone(),
                aux_tex.clone(),
            ));
            target_entities.push((actor.sheet_entity, handle));
        }
        if !actor.piece_entities.is_empty() {
            let quad_ent =
                ensure_character_composite_rt(&mut actor, &mut commands, &mut images, &mut runtime);
            let rt_handle = materials.add(FunkinSpriteMaterial::from_config_ex(
                cfg.clone(),
                aux_tex.clone(),
                true,
            ));
            target_entities.push((quad_ent, rt_handle));
        }
        if role == CharShaderRole::Spectator
            && actor.id != "nene-dark"
            && actor.id != "nene-pixel"
        {
            let abot_handle = materials.add(FunkinSpriteMaterial::from_config(cfg, aux_tex));
            for (d_ent, dprop, dcomp) in &dynamic_props {
                if is_abot_shader_synced_prop(&dprop.id) {
                    target_entities.push((d_ent, abot_handle.clone()));
                    if let Some(composite) = dcomp {
                        for &piece_ent in &composite.piece_entities {
                            if piece_ent != Entity::PLACEHOLDER {
                                target_entities.push((piece_ent, abot_handle.clone()));
                            }
                        }
                    }
                }
            }
        }

        for (entity, handle) in target_entities {
            if char_shaders.contains(entity) {
                continue;
            }
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
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

fn is_abot_shader_synced_prop(id: &str) -> bool {
    matches!(
        id,
        "abot_body" | "abot_stereo_bg" | "abot_eye_whites" | "abot_pupil"
    ) || id.starts_with("abot_viz_")
}
