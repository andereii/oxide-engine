use bevy::prelude::*;
use bevy::sprite_render::SpriteMaterial;

use crate::mods::ModRegistry;
use crate::play::camera::hud::HudCamera;
use crate::play::characters::CharacterActor;
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
use super::{
    ManagedSpriteShader, ShaderScriptCommand, ShaderTargetKind, StageShaderRuntime,
    aux_path_for_config, load_aux_handle, to_char_shader_role,
};

fn collect_target_entities(
    target: &str,
    managed: &Query<(Entity, &mut ManagedSpriteShader)>,
    stage_props: &Query<
        (Entity, &StagePropVisual, &mut Sprite, Option<&StageCompositeProp>),
        Without<DynamicScriptProp>,
    >,
    dynamic_props: &Query<(Entity, &DynamicScriptProp, Option<&StageCompositeProp>)>,
    actors: &Query<&CharacterActor>,
) -> Vec<(Entity, ShaderTargetKind, String, String)> {
    let mut out = Vec::new();
    for actor in actors {
        if crate::play::plugin::actor_matches_target(actor, target) {
            let role = to_char_shader_role(actor.role);
            if actor.sheet_entity != Entity::PLACEHOLDER {
                out.push((
                    actor.sheet_entity,
                    ShaderTargetKind::Character(role),
                    actor.id.clone(),
                    String::new(),
                ));
            }
            if let Some(ref rt) = actor.composite_rt {
                if rt.quad_entity != Entity::PLACEHOLDER {
                    out.push((
                        rt.quad_entity,
                        ShaderTargetKind::Character(role),
                        actor.id.clone(),
                        String::new(),
                    ));
                }
            } else {
                for &piece in &actor.piece_entities {
                    if piece != Entity::PLACEHOLDER {
                        out.push((
                            piece,
                            ShaderTargetKind::Character(role),
                            actor.id.clone(),
                            String::new(),
                        ));
                    }
                }
            }
            if role == CharShaderRole::Spectator
                && actor.id != "nene-dark"
                && actor.id != "nene-pixel"
                && actor.id != "nene-tankmen"
            {
                for (d_ent, dprop, dcomp) in dynamic_props {
                    if matches!(
                        dprop.id.as_str(),
                        "abot_body" | "abot_stereo_bg" | "abot_eye_whites"
                    ) || dprop.id.starts_with("abot_viz_")
                    {
                        out.push((
                            d_ent,
                            ShaderTargetKind::DynamicProp,
                            String::new(),
                            dprop.id.clone(),
                        ));
                        if let Some(comp) = dcomp {
                            for &piece in &comp.piece_entities {
                                if piece != Entity::PLACEHOLDER {
                                    out.push((
                                        piece,
                                        ShaderTargetKind::DynamicProp,
                                        String::new(),
                                        dprop.id.clone(),
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for (entity, prop, _, composite) in stage_props.iter() {
        if prop.name == target || prop.name.eq_ignore_ascii_case(target) {
            out.push((
                entity,
                ShaderTargetKind::StageProp,
                String::new(),
                prop.name.clone(),
            ));
            if let Some(composite) = composite {
                for &piece in &composite.piece_entities {
                    if piece != Entity::PLACEHOLDER {
                        out.push((
                            piece,
                            ShaderTargetKind::StageProp,
                            String::new(),
                            prop.name.clone(),
                        ));
                    }
                }
            }
        }
    }
    for (entity, dprop, composite) in dynamic_props {
        if dprop.id == target {
            out.push((
                entity,
                ShaderTargetKind::DynamicProp,
                String::new(),
                dprop.id.clone(),
            ));
            if let Some(composite) = composite {
                for &piece in &composite.piece_entities {
                    if piece != Entity::PLACEHOLDER {
                        out.push((
                            piece,
                            ShaderTargetKind::DynamicProp,
                            String::new(),
                            dprop.id.clone(),
                        ));
                    }
                }
            }
        }
    }
    if out.is_empty() {
        for (entity, item) in managed.iter() {
            if (!item.prop_name.is_empty() && item.prop_name == target)
                || (!item.char_id.is_empty() && item.char_id.eq_ignore_ascii_case(target))
            {
                out.push((
                    entity,
                    item.target,
                    item.char_id.clone(),
                    item.prop_name.clone(),
                ));
            }
        }
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_shader_script_command(
    cmd: ShaderScriptCommand,
    commands: &mut Commands,
    mods: &ModRegistry,
    asset_server: &AssetServer,
    shaders: Option<&mut Assets<Shader>>,
    materials: &mut Assets<FunkinSpriteMaterial>,
    runtime: &mut StageShaderRuntime,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    pending_mats: &mut std::collections::HashMap<Entity, Handle<FunkinSpriteMaterial>>,
    stage_props: &Query<
        (Entity, &StagePropVisual, &mut Sprite, Option<&StageCompositeProp>),
        Without<DynamicScriptProp>,
    >,
    dynamic_props: &Query<(Entity, &DynamicScriptProp, Option<&StageCompositeProp>)>,
    actors: &Query<&CharacterActor>,
    cameras: &mut Query<
        (Entity, Option<&mut RainCameraEffect>),
        (
            With<Camera2d>,
            Without<HudCamera>,
            Without<PauseOverlayCamera>,
            Without<LoadingOverlayCamera>,
            Without<SoundTrayOverlayCamera>,
        ),
    >,
) {
    match cmd {
        ShaderScriptCommand::SetCameraRain {
            scale,
            intensity,
            color,
            time_scale_decay,
        } => {
            let spec = CameraRainSpec {
                scale,
                start_intensity: intensity,
                end_intensity: intensity,
                rain_color: color,
                time_scale_decay,
            };
            runtime.rain_spec = Some(spec);
            for (cam_entity, existing) in cameras.iter_mut() {
                if let Some(mut fx) = existing {
                    fx.screen_and_scale.z = scale;
                    fx.screen_and_scale.w = intensity;
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
        ShaderScriptCommand::ClearShader { target } => {
            let lower = target.trim().to_ascii_lowercase();
            if matches!(lower.as_str(), "all" | "reload" | "__reload__") {
                runtime.attached = false;
                runtime.rain_spec = None;
                for (cam_entity, _) in cameras.iter() {
                    commands.entity(cam_entity).remove::<RainCameraEffect>();
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
            for (entity, _, _, _) in
                collect_target_entities(&target, managed, stage_props, dynamic_props, actors)
            {
                pending_mats.remove(&entity);
                commands
                    .entity(entity)
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
                materials,
                runtime.time_s,
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
                materials,
                runtime.time_s,
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
                materials,
                runtime.time_s,
                managed,
                pending_mats,
                stage_props,
                dynamic_props,
                actors,
            );
        }
        ShaderScriptCommand::SetBlendMode { target, mode } => {
            let blend = parse_blend_mode(&mode);
            mutate_target_materials(
                &target,
                commands,
                materials,
                runtime.time_s,
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
                    commands,
                    materials,
                    runtime.time_s,
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
                    commands,
                    materials,
                    runtime.time_s,
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
                commands,
                materials,
                runtime.time_s,
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
                    "color" | "rain_color" => spec.rain_color = [values[0], values[1], values[2]],
                    _ => {}
                }
                return;
            }
            let key_lower = key.trim().to_ascii_lowercase();
            mutate_target_materials(
                &target,
                commands,
                materials,
                runtime.time_s,
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
            let entities =
                collect_target_entities(&target, managed, stage_props, dynamic_props, actors);
            for (entity, kind, char_id, prop_name) in entities {
                if let Ok((_, mut existing)) = managed.get_mut(entity) {
                    if let Some(mut mat) = materials.get_mut(&existing.handle) {
                        mat.aux_texture = aux_tex.clone();
                        mat.sync_uniform();
                    }
                    existing.aux_path = aux_rel.clone();
                } else if let Some(pending_handle) = pending_mats.get(&entity) {
                    if let Some(mut mat) = materials.get_mut(pending_handle) {
                        mat.aux_texture = aux_tex.clone();
                        mat.time_s = runtime.time_s;
                        mat.sync_uniform();
                    }
                    commands.entity(entity).insert(ManagedSpriteShader {
                        handle: pending_handle.clone(),
                        target: kind,
                        char_id,
                        prop_name,
                        aux_path: aux_rel.clone(),
                    });
                } else {
                    let mut mat = FunkinSpriteMaterial::from_config(
                        SpriteShaderConfig::default(),
                        aux_tex.clone(),
                    );
                    mat.time_s = runtime.time_s;
                    mat.sync_uniform();
                    let handle = materials.add(mat);
                    pending_mats.insert(entity, handle.clone());
                    commands.entity(entity).insert((
                        SpriteMaterial(handle.clone()),
                        ManagedSpriteShader {
                            handle,
                            target: kind,
                            char_id,
                            prop_name,
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
    commands: &mut Commands,
    materials: &mut Assets<FunkinSpriteMaterial>,
    time_s: f32,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    pending_mats: &mut std::collections::HashMap<Entity, Handle<FunkinSpriteMaterial>>,
    stage_props: &Query<
        (Entity, &StagePropVisual, &mut Sprite, Option<&StageCompositeProp>),
        Without<DynamicScriptProp>,
    >,
    dynamic_props: &Query<(Entity, &DynamicScriptProp, Option<&StageCompositeProp>)>,
    actors: &Query<&CharacterActor>,
    mut mutator: impl FnMut(&mut FunkinSpriteMaterial),
) {
    let entities = collect_target_entities(target, managed, stage_props, dynamic_props, actors);
    for (entity, kind, char_id, prop_name) in entities {
        if let Ok((_, existing)) = managed.get(entity) {
            if let Some(mut mat) = materials.get_mut(&existing.handle) {
                mutator(&mut mat);
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else if let Some(pending_handle) = pending_mats.get(&entity) {
            if let Some(mut mat) = materials.get_mut(pending_handle) {
                mutator(&mut mat);
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else {
            let mut mat = FunkinSpriteMaterial::from_config(SpriteShaderConfig::default(), None);
            mat.time_s = time_s;
            mutator(&mut mat);
            mat.sync_uniform();
            let handle = materials.add(mat);
            pending_mats.insert(entity, handle.clone());
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: kind,
                    char_id,
                    prop_name,
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
    materials: &mut Assets<FunkinSpriteMaterial>,
    time_s: f32,
    managed: &mut Query<(Entity, &mut ManagedSpriteShader)>,
    pending_mats: &mut std::collections::HashMap<Entity, Handle<FunkinSpriteMaterial>>,
    stage_props: &Query<
        (Entity, &StagePropVisual, &mut Sprite, Option<&StageCompositeProp>),
        Without<DynamicScriptProp>,
    >,
    dynamic_props: &Query<(Entity, &DynamicScriptProp, Option<&StageCompositeProp>)>,
    actors: &Query<&CharacterActor>,
) {
    let aux_rel = aux_path_for_config(&cfg).map(str::to_string);
    let aux_tex = load_aux_handle(mods, asset_server, aux_rel.as_deref());
    let entities = collect_target_entities(target, managed, stage_props, dynamic_props, actors);

    for (entity, kind, char_id, prop_name) in entities {
        if let Ok((_, mut existing)) = managed.get_mut(entity) {
            if let Some(mut mat) = materials.get_mut(&existing.handle) {
                mat.config = cfg.clone();
                if existing.aux_path != aux_rel {
                    mat.aux_texture = aux_tex.clone();
                    existing.aux_path = aux_rel.clone();
                }
                mat.time_s = time_s;
                mat.sync_uniform();
            }
        } else if let Some(pending_handle) = pending_mats.get(&entity) {
            if let Some(mut mat) = materials.get_mut(pending_handle) {
                mat.config = cfg.clone();
                mat.aux_texture = aux_tex.clone();
                mat.time_s = time_s;
                mat.sync_uniform();
            }
            commands.entity(entity).insert(ManagedSpriteShader {
                handle: pending_handle.clone(),
                target: kind,
                char_id,
                prop_name,
                aux_path: aux_rel.clone(),
            });
        } else {
            let mut mat = FunkinSpriteMaterial::from_config(cfg.clone(), aux_tex.clone());
            mat.time_s = time_s;
            mat.sync_uniform();
            let handle = materials.add(mat);
            pending_mats.insert(entity, handle.clone());
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: kind,
                    char_id,
                    prop_name,
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
