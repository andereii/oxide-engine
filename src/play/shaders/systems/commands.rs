use bevy::prelude::*;
use bevy::sprite_render::SpriteMaterial;

use crate::mods::ModRegistry;
use crate::play::camera::hud::{HudCamera, OverlayCamera};
use crate::play::characters::CharacterActor;
use crate::play::characters::behavior::CompositeRtCamera;
use crate::play::stage::{DynamicScriptProp, StageCompositeProp, StagePropVisual};
use crate::play::PauseOverlayCamera;
use crate::ui::{LoadingOverlayCamera, SoundTrayOverlayCamera};

use super::super::defs::{
    AdjustColorParams, CameraRainSpec, CharShaderRole, DropShadowParams, FunkinBlendMode,
    SHADER_MODE_ADJUST_COLOR, SHADER_MODE_BUILDING, SHADER_MODE_DROP_SHADOW,
    SHADER_MODE_OVERLAY_BLEND, SHADER_MODE_RAIN_SPRITE, SHADER_MODE_TEXTURE_SWAP,
    SHADER_MODE_WIGGLE, SpriteShaderConfig,
};
use super::super::material::FunkinSpriteMaterial;
use super::super::rain_post::RainCameraEffect;
use super::super::stage_blend_post::{
    StageBlendPostImages, StageBlendPostUniform, StagePostOverlayKind, StagePostOverlayProp,
};
use super::attach::{ensure_character_composite_rt, ensure_stage_composite_rt_scaled};
use super::{
    ManagedSpriteShader, ShaderScriptCommand, ShaderTargetKind, StageShaderRuntime,
    aux_path_for_config, load_aux_handle, to_char_shader_role,
};

pub(super) type StaticStagePropsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static StagePropVisual,
        &'static mut Sprite,
        Option<&'static mut StageCompositeProp>,
    ),
    Without<DynamicScriptProp>,
>;

pub(super) type DynamicStagePropsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static DynamicScriptProp,
        Option<&'static StagePropVisual>,
        Option<&'static mut StageCompositeProp>,
    ),
>;

struct TargetEntry {
    entity: Entity,
    kind: ShaderTargetKind,
    char_id: String,
    prop_name: String,
    premultiplied: bool,
}

#[allow(clippy::too_many_arguments)]
fn push_composite_prop_targets(
    entity: Entity,
    kind: ShaderTargetKind,
    prop_name: &str,
    z_layer: f32,
    base_scale: [f32; 2],
    composite_opt: Option<Mut<'_, StageCompositeProp>>,
    promote_rt: bool,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    runtime: &mut StageShaderRuntime,
    out: &mut Vec<TargetEntry>,
) {
    let Some(mut composite) = composite_opt else {
        out.push(TargetEntry {
            entity,
            kind,
            char_id: String::new(),
            prop_name: prop_name.to_string(),
            premultiplied: false,
        });
        return;
    };
    if promote_rt && composite.composite_rt.is_none() && !composite.piece_entities.is_empty() {
        ensure_stage_composite_rt_scaled(
            &mut composite,
            z_layer,
            base_scale,
            commands,
            images,
            runtime,
        );
        commands
            .entity(entity)
            .remove::<(SpriteMaterial<FunkinSpriteMaterial>, ManagedSpriteShader)>();
    }
    if let Some(ref rt) = composite.composite_rt {
        if rt.quad_entity != Entity::PLACEHOLDER {
            out.push(TargetEntry {
                entity: rt.quad_entity,
                kind,
                char_id: String::new(),
                prop_name: prop_name.to_string(),
                premultiplied: true,
            });
        }
        return;
    }
    out.push(TargetEntry {
        entity,
        kind,
        char_id: String::new(),
        prop_name: prop_name.to_string(),
        premultiplied: false,
    });
    for &piece in &composite.piece_entities {
        if piece != Entity::PLACEHOLDER {
            out.push(TargetEntry {
                entity: piece,
                kind,
                char_id: String::new(),
                prop_name: prop_name.to_string(),
                premultiplied: false,
            });
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn collect_target_entities(
    target: &str,
    promote_rt: bool,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    runtime: &mut StageShaderRuntime,
    managed: &Query<(Entity, &mut ManagedSpriteShader)>,
    stage_props: &mut StaticStagePropsQuery,
    dynamic_props: &mut DynamicStagePropsQuery,
    actors: &mut Query<&mut CharacterActor>,
) -> Vec<TargetEntry> {
    let mut out = Vec::new();
    for mut actor in actors.iter_mut() {
        if crate::play::plugin::actor_matches_target(&actor, target) {
            let role = to_char_shader_role(actor.role);
            if actor.sheet_entity != Entity::PLACEHOLDER {
                out.push(TargetEntry {
                    entity: actor.sheet_entity,
                    kind: ShaderTargetKind::Character(role),
                    char_id: actor.id.clone(),
                    prop_name: String::new(),
                    premultiplied: false,
                });
            }
            if promote_rt && actor.composite_rt.is_none() && !actor.piece_entities.is_empty() {
                ensure_character_composite_rt(&mut actor, commands, images, runtime);
            }
            if let Some(ref rt) = actor.composite_rt {
                if rt.quad_entity != Entity::PLACEHOLDER {
                    out.push(TargetEntry {
                        entity: rt.quad_entity,
                        kind: ShaderTargetKind::Character(role),
                        char_id: actor.id.clone(),
                        prop_name: String::new(),
                        premultiplied: true,
                    });
                }
            } else {
                for &piece in &actor.piece_entities {
                    if piece != Entity::PLACEHOLDER {
                        out.push(TargetEntry {
                            entity: piece,
                            kind: ShaderTargetKind::Character(role),
                            char_id: actor.id.clone(),
                            prop_name: String::new(),
                            premultiplied: false,
                        });
                    }
                }
            }
            if role == CharShaderRole::Spectator
                && actor.id != "nene-dark"
                && actor.id != "nene-pixel"
                && actor.id != "nene-tankmen"
            {
                for (d_ent, dprop, _, dcomp) in dynamic_props.iter() {
                    if matches!(
                        dprop.id.as_str(),
                        "abot_body" | "abot_stereo_bg" | "abot_eye_whites"
                    ) || dprop.id.starts_with("abot_viz_")
                    {
                        out.push(TargetEntry {
                            entity: d_ent,
                            kind: ShaderTargetKind::DynamicProp,
                            char_id: String::new(),
                            prop_name: dprop.id.clone(),
                            premultiplied: false,
                        });
                        if let Some(comp) = dcomp {
                            for &piece in &comp.piece_entities {
                                if piece != Entity::PLACEHOLDER {
                                    out.push(TargetEntry {
                                        entity: piece,
                                        kind: ShaderTargetKind::DynamicProp,
                                        char_id: String::new(),
                                        prop_name: dprop.id.clone(),
                                        premultiplied: false,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for (entity, prop, _, composite) in stage_props.iter_mut() {
        if prop.name == target || prop.name.eq_ignore_ascii_case(target) {
            push_composite_prop_targets(
                entity,
                ShaderTargetKind::StageProp,
                &prop.name,
                prop.z_layer,
                prop.base_scale,
                composite,
                promote_rt,
                commands,
                images,
                runtime,
                &mut out,
            );
        }
    }
    for (entity, dprop, vis_opt, composite) in dynamic_props.iter_mut() {
        if dprop.id == target || dprop.id.eq_ignore_ascii_case(target) {
            let z_layer = vis_opt.map_or(0.0, |v| v.z_layer);
            let base_scale = vis_opt.map_or([1.0, 1.0], |v| v.base_scale);
            push_composite_prop_targets(
                entity,
                ShaderTargetKind::DynamicProp,
                &dprop.id,
                z_layer,
                base_scale,
                composite,
                promote_rt,
                commands,
                images,
                runtime,
                &mut out,
            );
        }
    }
    if out.is_empty() {
        for (entity, item) in managed.iter() {
            if (!item.prop_name.is_empty() && item.prop_name == target)
                || (!item.char_id.is_empty() && item.char_id.eq_ignore_ascii_case(target))
            {
                out.push(TargetEntry {
                    entity,
                    kind: item.target,
                    char_id: item.char_id.clone(),
                    prop_name: item.prop_name.clone(),
                    premultiplied: false,
                });
            }
        }
    }
    out
}

fn collect_prop_root_entities(
    target: &str,
    stage_props: &StaticStagePropsQuery,
    dynamic_props: &DynamicStagePropsQuery,
) -> Vec<Entity> {
    let mut entities = Vec::new();
    for (entity, prop, _, _) in stage_props.iter() {
        if prop.name == target || prop.name.eq_ignore_ascii_case(target) {
            entities.push(entity);
        }
    }
    for (entity, dprop, _, _) in dynamic_props.iter() {
        if dprop.id == target || dprop.id.eq_ignore_ascii_case(target) {
            entities.push(entity);
        }
    }
    entities
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_shader_script_command(
    cmd: ShaderScriptCommand,
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    shaders: Option<&mut Assets<Shader>>,
    images: &mut Assets<Image>,
    materials: &mut Assets<FunkinSpriteMaterial>,
    runtime: &mut StageShaderRuntime,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    pending_mats: &mut std::collections::HashMap<Entity, Handle<FunkinSpriteMaterial>>,
    stage_props: &mut StaticStagePropsQuery,
    dynamic_props: &mut DynamicStagePropsQuery,
    actors: &mut Query<&mut CharacterActor>,
    cameras: &mut Query<
        (Entity, Option<&mut RainCameraEffect>),
        (
            With<Camera2d>,
            With<IsDefaultUiCamera>,
            Without<HudCamera>,
            Without<OverlayCamera>,
            Without<PauseOverlayCamera>,
            Without<LoadingOverlayCamera>,
            Without<SoundTrayOverlayCamera>,
            Without<CompositeRtCamera>,
        ),
    >,
) {
    match cmd {
        ShaderScriptCommand::SetCameraRain {
            scale,
            start_intensity,
            end_intensity,
            color,
            time_scale_decay,
        } => {
            let spec = CameraRainSpec {
                scale,
                start_intensity,
                end_intensity,
                rain_color: color,
                time_scale_decay,
            };
            runtime.rain_spec = Some(spec);
            for (cam_entity, existing) in cameras.iter_mut() {
                if let Some(mut fx) = existing {
                    fx.screen_and_scale.z = scale;
                    fx.screen_and_scale.w = start_intensity;
                    fx.time_and_color.y = color[0];
                    fx.time_and_color.z = color[1];
                    fx.time_and_color.w = color[2];
                } else {
                    commands
                        .entity(cam_entity)
                        .insert(RainCameraEffect::from_spec(spec));
                }
            }
        }
        ShaderScriptCommand::SetOverlayBlend { target, aux_path } => {
            let trimmed = aux_path.trim();
            let mut cfg = SpriteShaderConfig::overlay_blend(trimmed);
            if trimmed.is_empty() {
                cfg.overlay_path = None;
            }
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetRainSprite {
            target,
            scale,
            intensity,
            color,
        } => {
            let cfg = SpriteShaderConfig::rain_sprite(scale, intensity, color);
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetBuilding { target, alpha_shit } => {
            let cfg = SpriteShaderConfig::building(alpha_shit);
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::ClearShader { target } => {
            let lower = target.trim().to_ascii_lowercase();
            if matches!(lower.as_str(), "all" | "reload" | "__reload__") {
                runtime.attached = false;
                runtime.rain_spec = None;
                for (cam_entity, _) in cameras.iter() {
                    commands.entity(cam_entity).remove::<(
                        RainCameraEffect,
                        StageBlendPostUniform,
                        StageBlendPostImages,
                    )>();
                }
                return;
            }
            if matches!(lower.as_str(), "cam" | "camera" | "camgame" | "rain") {
                runtime.rain_spec = None;
                for (cam_entity, _) in cameras.iter() {
                    commands.entity(cam_entity).remove::<RainCameraEffect>();
                }
                return;
            }
            for prop_ent in collect_prop_root_entities(&target, stage_props, dynamic_props) {
                commands.entity(prop_ent).remove::<StagePostOverlayProp>();
            }
            for item in collect_target_entities(
                &target,
                false,
                commands,
                images,
                runtime,
                managed,
                stage_props,
                dynamic_props,
                actors,
            ) {
                pending_mats.remove(&item.entity);
                commands
                    .entity(item.entity)
                    .remove::<(SpriteMaterial<FunkinSpriteMaterial>, ManagedSpriteShader)>();
            }
        }
        ShaderScriptCommand::SetAdjustColor {
            target,
            hue,
            saturation,
            brightness,
            contrast,
        } => {
            let cfg = SpriteShaderConfig::adjust_color(hue, saturation, brightness, contrast);
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetDropShadow {
            target,
            color,
            angle_deg,
            distance,
            strength,
            threshold,
            hue,
            saturation,
            brightness,
            contrast,
            use_alt_mask,
            alt_mask_path,
            mask_threshold,
        } => {
            let cfg = SpriteShaderConfig::drop_shadow(DropShadowParams {
                adjust: AdjustColorParams::new(hue, saturation, brightness, contrast),
                color,
                angle_deg,
                distance,
                strength,
                threshold,
                use_alt_mask,
                alt_mask_path,
                mask_threshold,
                ..Default::default()
            });
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetWiggle {
            target,
            effect_type,
            speed,
            frequency,
            amplitude,
        } => {
            let cfg = SpriteShaderConfig::wiggle(effect_type, speed, frequency, amplitude);
            apply_config_to_target(
                &target,
                cfg,
                commands,
                mods,
                asset_server,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetBlendMode { target, mode } => {
            if let Some(kind) = StagePostOverlayKind::parse(&mode) {
                for prop_ent in collect_prop_root_entities(&target, stage_props, dynamic_props) {
                    commands
                        .entity(prop_ent)
                        .insert(StagePostOverlayProp { kind })
                        .remove::<(SpriteMaterial<FunkinSpriteMaterial>, ManagedSpriteShader)>();
                }
                return;
            }
            for prop_ent in collect_prop_root_entities(&target, stage_props, dynamic_props) {
                commands.entity(prop_ent).remove::<StagePostOverlayProp>();
            }
            let blend = parse_blend_mode(&mode);
            mutate_target_materials(
                &target,
                false,
                commands,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
                |mat| {
                    mat.config.blend_mode = blend;
                },
            );
        }
        ShaderScriptCommand::SetCustomShader { target, path } => {
            let trimmed = path.trim();
            if trimmed.is_empty()
                || trimmed.eq_ignore_ascii_case("none")
                || trimmed.eq_ignore_ascii_case("clear")
            {
                mutate_target_materials(
                    &target,
                    false,
                    commands,
                    images,
                    materials,
                    runtime,
                    managed,
                    pending_mats,
                    stage_props,
                    dynamic_props,
                    actors,
                    |mat| {
                        mat.custom_shader = None;
                    },
                );
                return;
            }
            if let Some(mode_bit) = builtin_mode_flag(trimmed) {
                mutate_target_materials(
                    &target,
                    false,
                    commands,
                    images,
                    materials,
                    runtime,
                    managed,
                    pending_mats,
                    stage_props,
                    dynamic_props,
                    actors,
                    |mat| {
                        mat.custom_shader = None;
                        mat.config.mode_flags |= mode_bit;
                    },
                );
                return;
            }
            let Some(shader_file) = mods.resolve_with_extensions(trimmed, &["wgsl", "wesl"]) else {
                warn!("shader({target}): could not find '{trimmed}'");
                return;
            };
            let Ok(src) = std::fs::read_to_string(&shader_file) else {
                warn!("shader({target}): failed to read '{}'", shader_file.display());
                return;
            };
            let srv_path = mods.to_asset_server_path(&shader_file);
            let is_wesl = shader_file
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("wesl"));
            let shader = if is_wesl {
                Shader::from_wesl(src, srv_path.clone())
            } else {
                Shader::from_wgsl(src, srv_path.clone())
            };
            let uuid = deterministic_shader_uuid(&srv_path);
            if let Some(shader_assets) = shaders {
                let _ = shader_assets.insert(bevy::asset::AssetId::Uuid { uuid }, shader);
            }
            let shader_handle: Handle<Shader> = Handle::Uuid(uuid, std::marker::PhantomData);
            mutate_target_materials(
                &target,
                true,
                commands,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
                |mat| {
                    mat.custom_shader = Some(shader_handle.clone());
                },
            );
        }
        ShaderScriptCommand::SetShaderParam {
            target,
            key,
            values,
        } => {
            if matches!(
                target.trim().to_ascii_lowercase().as_str(),
                "cam" | "camera" | "rain"
            ) && let Some(ref mut spec) = runtime.rain_spec
            {
                match key.trim().to_ascii_lowercase().as_str() {
                    "scale" => spec.scale = values[0],
                    "intensity" | "int" => {
                        spec.start_intensity = values[0];
                        spec.end_intensity = values[0];
                    }
                    "start_intensity" => spec.start_intensity = values[0],
                    "end_intensity" => spec.end_intensity = values[0],
                    "color" | "rain_color" => spec.rain_color = [values[0], values[1], values[2]],
                    _ => {}
                }
                return;
            }
            let key_lower = key.trim().to_ascii_lowercase();
            mutate_target_materials(
                &target,
                false,
                commands,
                images,
                materials,
                runtime,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
                |mat| apply_param_to_config(&mut mat.config, &key_lower, values),
            );
        }
        ShaderScriptCommand::SetShaderTex { target, path } => {
            let trimmed = path.trim();
            let aux_tex = (!trimmed.is_empty())
                .then(|| load_aux_handle(mods, asset_server, Some(trimmed)))
                .flatten();
            let aux_rel = (!trimmed.is_empty()).then(|| trimmed.to_string());
            let entities = collect_target_entities(
                &target,
                false,
                commands,
                images,
                runtime,
                managed,
                stage_props,
                dynamic_props,
                actors,
            );
            for item in entities {
                if let Ok((_, mut existing)) = managed.get_mut(item.entity) {
                    if let Some(mut mat) = materials.get_mut(&existing.handle) {
                        mat.aux_texture = aux_tex.clone();
                        mat.sync_uniform();
                    }
                    existing.aux_path = aux_rel.clone();
                } else if let Some(pending_handle) = pending_mats.get(&item.entity) {
                    if let Some(mut mat) = materials.get_mut(pending_handle) {
                        mat.aux_texture = aux_tex.clone();
                        mat.time_s = runtime.time_s;
                        mat.sync_uniform();
                    }
                    commands.entity(item.entity).insert(ManagedSpriteShader {
                        handle: pending_handle.clone(),
                        target: item.kind,
                        char_id: item.char_id,
                        prop_name: item.prop_name,
                        aux_path: aux_rel.clone(),
                    });
                } else {
                    let mut mat = FunkinSpriteMaterial::from_config_ex(
                        SpriteShaderConfig::default(),
                        aux_tex.clone(),
                        item.premultiplied,
                    );
                    mat.time_s = runtime.time_s;
                    mat.sync_uniform();
                    let handle = materials.add(mat);
                    pending_mats.insert(item.entity, handle.clone());
                    commands.entity(item.entity).insert((
                        SpriteMaterial(handle.clone()),
                        ManagedSpriteShader {
                            handle,
                            target: item.kind,
                            char_id: item.char_id,
                            prop_name: item.prop_name,
                            aux_path: aux_rel.clone(),
                        },
                    ));
                }
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn mutate_target_materials(
    target: &str,
    promote_rt: bool,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    materials: &mut Assets<FunkinSpriteMaterial>,
    runtime: &mut StageShaderRuntime,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    pending_mats: &mut std::collections::HashMap<Entity, Handle<FunkinSpriteMaterial>>,
    stage_props: &mut StaticStagePropsQuery,
    dynamic_props: &mut DynamicStagePropsQuery,
    actors: &mut Query<&mut CharacterActor>,
    mut mutator: impl FnMut(&mut FunkinSpriteMaterial),
) {
    let time_s = runtime.time_s;
    let entities = collect_target_entities(
        target,
        promote_rt,
        commands,
        images,
        runtime,
        managed,
        stage_props,
        dynamic_props,
        actors,
    );
    for item in entities {
        if let Ok((_, existing)) = managed.get(item.entity) {
            if let Some(mut mat) = materials.get_mut(&existing.handle) {
                mutator(&mut mat);
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else if let Some(pending_handle) = pending_mats.get(&item.entity) {
            if let Some(mut mat) = materials.get_mut(pending_handle) {
                mutator(&mut mat);
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else {
            let mut mat = FunkinSpriteMaterial::from_config_ex(
                SpriteShaderConfig::default(),
                None,
                item.premultiplied,
            );
            mat.time_s = time_s;
            mutator(&mut mat);
            mat.sync_uniform();
            let handle = materials.add(mat);
            pending_mats.insert(item.entity, handle.clone());
            commands.entity(item.entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: item.kind,
                    char_id: item.char_id,
                    prop_name: item.prop_name,
                    aux_path: None,
                },
            ));
        }
    }
}

fn builtin_mode_flag(name: &str) -> Option<u32> {
    match name.trim().to_ascii_lowercase().as_str() {
        "adjust_color" | "adjustcolor" | "hsbc" => Some(SHADER_MODE_ADJUST_COLOR),
        "drop_shadow" | "dropshadow" => Some(SHADER_MODE_DROP_SHADOW),
        "wiggle" => Some(SHADER_MODE_WIGGLE),
        "building" => Some(SHADER_MODE_BUILDING),
        "overlay" | "overlay_blend" => Some(SHADER_MODE_OVERLAY_BLEND),
        "rain" | "rain_sprite" => Some(SHADER_MODE_RAIN_SPRITE),
        "texture_swap" | "textureswap" => Some(SHADER_MODE_TEXTURE_SWAP),
        _ => None,
    }
}

fn parse_blend_mode(raw: &str) -> FunkinBlendMode {
    FunkinBlendMode::parse(raw)
}

fn apply_param_to_config(cfg: &mut SpriteShaderConfig, key: &str, values: [f32; 4]) {
    if let Some(slot_str) = key.strip_prefix('u')
        && let Ok(idx) = slot_str.parse::<usize>()
        && idx < 16
    {
        cfg.custom_params[idx / 4][idx % 4] = values[0];
        return;
    }
    if let Some(vec_str) = key.strip_prefix("vec").or_else(|| key.strip_prefix('c'))
        && let Ok(row) = vec_str.parse::<usize>()
        && row < 4
    {
        cfg.custom_params[row] = values;
        return;
    }
    match key {
        "hue" => cfg.adjust.hue = values[0],
        "sat" | "saturation" => cfg.adjust.saturation = values[0],
        "bright" | "brightness" => cfg.adjust.brightness = values[0],
        "contrast" => cfg.adjust.contrast = values[0],
        "angle" => cfg.drop_shadow.angle_deg = values[0],
        "dist" | "distance" => cfg.drop_shadow.distance = values[0],
        "str" | "strength" => cfg.drop_shadow.strength = values[0],
        "thr" | "threshold" => cfg.drop_shadow.threshold = values[0],
        "mask_thr" | "mask_threshold" => cfg.drop_shadow.mask_threshold = values[0],
        "drop_color" | "color" => cfg.drop_shadow.color = [values[0], values[1], values[2]],
        "speed" => cfg.wiggle.speed = values[0],
        "freq" | "frequency" => cfg.wiggle.frequency = values[0],
        "amp" | "amplitude" => cfg.wiggle.amplitude = values[0],
        "alpha_shit" | "amount" | "fade_amount" => cfg.building.alpha_shit = values[0],
        "scale" => cfg.rain_sprite.scale = values[0],
        "intensity" | "int" => cfg.rain_sprite.intensity = values[0],
        "rain_color" => cfg.rain_sprite.rain_color = [values[0], values[1], values[2]],
        _ => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn apply_config_to_target(
    target: &str,
    cfg: SpriteShaderConfig,
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    images: &mut Assets<Image>,
    materials: &mut Assets<FunkinSpriteMaterial>,
    runtime: &mut StageShaderRuntime,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    pending_mats: &mut std::collections::HashMap<Entity, Handle<FunkinSpriteMaterial>>,
    stage_props: &mut StaticStagePropsQuery,
    dynamic_props: &mut DynamicStagePropsQuery,
    actors: &mut Query<&mut CharacterActor>,
) {
    let promote_rt = (cfg.mode_flags & SHADER_MODE_DROP_SHADOW) != 0;
    let time_s = runtime.time_s;
    let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
    let aux_tex = load_aux_handle(mods, asset_server, aux_rel.as_deref());
    let entities = collect_target_entities(
        target,
        promote_rt,
        commands,
        images,
        runtime,
        managed,
        stage_props,
        dynamic_props,
        actors,
    );

    for item in entities {
        if let Ok((_, mut existing)) = managed.get_mut(item.entity) {
            if let Some(mut mat) = materials.get_mut(&existing.handle) {
                mat.config = cfg.clone();
                mat.premultiplied_input = item.premultiplied;
                if existing.aux_path != aux_rel {
                    mat.aux_texture = aux_tex.clone();
                    existing.aux_path = aux_rel.clone();
                }
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else if let Some(pending_handle) = pending_mats.get(&item.entity) {
            if let Some(mut mat) = materials.get_mut(pending_handle) {
                mat.config = cfg.clone();
                mat.premultiplied_input = item.premultiplied;
                mat.aux_texture = aux_tex.clone();
                mat.time_s = time_s;
                mat.sync_uniform();
            }
            commands.entity(item.entity).insert(ManagedSpriteShader {
                handle: pending_handle.clone(),
                target: item.kind,
                char_id: item.char_id,
                prop_name: item.prop_name,
                aux_path: aux_rel.clone(),
            });
        } else {
            let mut mat = FunkinSpriteMaterial::from_config_ex(
                cfg.clone(),
                aux_tex.clone(),
                item.premultiplied,
            );
            mat.time_s = time_s;
            mat.sync_uniform();
            let handle = materials.add(mat);
            pending_mats.insert(item.entity, handle.clone());
            commands.entity(item.entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: item.kind,
                    char_id: item.char_id,
                    prop_name: item.prop_name,
                    aux_path: aux_rel.clone(),
                },
            ));
        }
    }
}

fn deterministic_shader_uuid(key: &str) -> bevy::asset::uuid::Uuid {
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x517c_c1b7_2722_0a95;
    for &b in key.as_bytes() {
        h1 ^= u64::from(b);
        h1 = h1.wrapping_mul(0x0000_0100_0000_01b3);
        h2 ^= u64::from(b).wrapping_add(17);
        h2 = h2.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&h1.to_le_bytes());
    bytes[8..].copy_from_slice(&h2.to_le_bytes());
    bevy::asset::uuid::Uuid::from_bytes(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::super::defs::DropShadowParams;
    use super::super::PendingShaderCommands;
    use super::super::attach::attach_stage_and_character_shaders;
    use super::super::update::update_stage_shaders;
    use super::*;
    use crate::animations::AtlasSettings;
    use crate::play::scene::PlayState;
    use crate::play::stage::StageInitialBlend;
    use crate::play::stage::schema::StageCoordMode;
    use crate::play::types::{PlayPhase, PlayStateParams};
    use std::collections::HashMap;

    #[test]
    fn promoting_scaled_stage_composite_prop_uses_base_scale_and_clears_placeholder_shader() {
        let mut app = App::new();
        let mut state = PlayState::new(PlayStateParams::default());
        state.phase = PlayPhase::Playing;
        state.chart.stage_id = "scaled_composite_stage".to_string();

        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Shader>()
            .init_asset::<FunkinSpriteMaterial>()
            .insert_resource(state)
            .insert_resource(ModRegistry::default())
            .init_resource::<StageShaderRuntime>()
            .init_resource::<PendingShaderCommands>()
            .add_systems(
                Update,
                (
                    attach_stage_and_character_shaders,
                    ApplyDeferred,
                    update_stage_shaders,
                )
                    .chain(),
            );

        let piece_ent = app.world_mut().spawn(Sprite::default()).id();
        let prop_ent = app
            .world_mut()
            .spawn((
                Sprite::default(),
                StagePropVisual {
                    name: "scaledProp".to_string(),
                    base_position: [0.0, 0.0],
                    base_scale: [2.0, 2.0],
                    scroll_factor: [1.0, 1.0],
                    base_flip: [false, false],
                    base_angle_deg: 0.0,
                    z_layer: 10.0,
                    visible: true,
                    coord_mode: StageCoordMode::Center,
                    raw_size: [400.0, 500.0],
                    velocity: [0.0, 0.0],
                    acceleration: [0.0, 0.0],
                    repeat: [false, false],
                    custom_affine: None,
                    overlay: false,
                },
                StageCompositeProp {
                    atlas_dir: None,
                    textures: Vec::new(),
                    clips: HashMap::new(),
                    bounds_min: [0.0, 0.0],
                    bounds_size: [400.0, 500.0],
                    atlas_settings: AtlasSettings::default(),
                    stage_matrix: None,
                    piece_entities: vec![piece_ent],
                    composite_rt: None,
                },
                StageInitialBlend("add".to_string()),
            ))
            .id();

        app.update();
        assert!(app.world().entity(prop_ent).contains::<ManagedSpriteShader>());
        assert!(
            app.world()
                .entity(piece_ent)
                .contains::<SpriteMaterial<FunkinSpriteMaterial>>()
        );

        app.world_mut()
            .resource_mut::<PendingShaderCommands>()
            .0
            .push(ShaderScriptCommand::SetDropShadow {
                target: "scaledProp".to_string(),
                color: DropShadowParams::default().color,
                angle_deg: 90.0,
                distance: 15.0,
                strength: 1.0,
                threshold: 0.1,
                hue: 0.0,
                saturation: 0.0,
                brightness: 0.0,
                contrast: 0.0,
                use_alt_mask: false,
                alt_mask_path: None,
                mask_threshold: 0.5,
            });
        app.update();

        let world = app.world();
        let composite = world
            .entity(prop_ent)
            .get::<StageCompositeProp>()
            .expect("composite prop exists");
        let rt = composite
            .composite_rt
            .as_ref()
            .expect("composite RT promoted");
        let images = world.resource::<Assets<Image>>();
        let rt_img = images.get(&rt.image).expect("RT image exists");
        // [400, 500] * [2.0, 2.0] + 128 padding = [928, 1128] (vs [528, 628] unscaled)
        assert_eq!(rt_img.width(), 928);
        assert_eq!(rt_img.height(), 1128);

        assert!(!world.entity(prop_ent).contains::<ManagedSpriteShader>());
        assert!(
            !world
                .entity(prop_ent)
                .contains::<SpriteMaterial<FunkinSpriteMaterial>>()
        );
        assert!(
            !world
                .entity(piece_ent)
                .contains::<SpriteMaterial<FunkinSpriteMaterial>>()
        );
        let quad_managed = world
            .entity(rt.quad_entity)
            .get::<ManagedSpriteShader>()
            .expect("RT quad has ManagedSpriteShader");
        let mats = world.resource::<Assets<FunkinSpriteMaterial>>();
        let mat = mats.get(&quad_managed.handle).expect("RT quad material exists");
        assert!(mat.premultiplied_input);
        assert_ne!(mat.config.mode_flags & SHADER_MODE_DROP_SHADOW, 0);
    }
}
