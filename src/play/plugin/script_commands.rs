use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::mods::ModRegistry;
use crate::scripting::{LogLevel, ScriptCommand, TweenProp};

use super::super::camera::hud::hud_render_layer;
use super::super::characters::CharacterActor;
use super::super::notes::data::NoteDirection;
use super::super::shaders::systems::{ShaderScriptCommand, parse_wiggle_effect_type};
use super::super::stage::schema::StageCoordMode;
use super::super::stage::{DynamicScriptProp, StagePropVisual};
use super::actor_matches_target;
use super::hud_audio::spawn_one_shot;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_ecs_script_commands(
    commands: &mut Commands,
    asset_server: &AssetServer,
    mods: &ModRegistry,
    actors: &mut Query<&mut CharacterActor>,
    props: &mut Query<(
        Entity,
        &mut StagePropVisual,
        &mut Sprite,
        Option<&mut AnimationController>,
        Option<&DynamicScriptProp>,
    )>,
    pending_shaders: &mut super::super::shaders::systems::PendingShaderCommands,
    cmds: &[ScriptCommand],
) {
    for cmd in cmds {
        match cmd {
            ScriptCommand::Log { level, mod_id, message } => match level {
                LogLevel::Info => info!("[mod:{mod_id}] {message}"),
                LogLevel::Warn => warn!("[mod:{mod_id}] {message}"),
                LogLevel::Error => error!("[mod:{mod_id}] {message}"),
            },
            ScriptCommand::PlayAnim { target, anim, force } => {
                let mut matched_actor = false;
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.controller.play(anim, *force, false, 0);
                        actor.hold_timer = 0.0;
                        actor.script_anim_override = true;
                        matched_actor = true;
                    }
                }
                if !matched_actor {
                    for (_, vis, _, anim_opt, _) in props.iter_mut() {
                        if vis.name.eq_ignore_ascii_case(target)
                            && let Some(mut ctrl) = anim_opt
                        {
                            ctrl.play(anim, *force, false, 0);
                        }
                    }
                }
            }
            ScriptCommand::PlaySing { target, dir, miss, suffix } => {
                let note_dir = NoteDirection::from_int(*dir as i32);
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.play_sing(note_dir, *miss, suffix);
                        actor.hold_timer = 0.0;
                        actor.script_anim_override = true;
                    }
                }
            }
            ScriptCommand::SetCharacterPos { target, x, y } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.slot.position = [*x, *y];
                        actor.recompute_camera_focus();
                    }
                }
            }
            ScriptCommand::SetCharacterScale { target, sx, sy } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.slot.scale = [*sx, *sy];
                    }
                }
            }
            ScriptCommand::SetCharacterTint { target, color } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.mod_tint = *color;
                    }
                }
            }
            ScriptCommand::SetCharacterAlpha { target, alpha } => {
                let alpha = alpha.clamp(0.0, 1.0);
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.mod_tint[3] = alpha;
                    }
                }
            }
            ScriptCommand::SetCharacterVisible { target, visible } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.mod_visible = *visible;
                    }
                }
            }
            ScriptCommand::SetCharacterDanceEvery { target, beats } => {
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.bopper.dance_every = *beats;
                    }
                }
            }
            ScriptCommand::SpawnProp { id, image_path, color, size, pos, z, hud } => {
                for (entity, _, _, _, dyn_opt) in props.iter() {
                    if dyn_opt.is_some_and(|d| d.id == *id) {
                        commands.entity(entity).despawn();
                    }
                }
                let mut sprite = match image_path {
                    Some(rel) => {
                        let resolved = mods
                            .resolve_asset_server_path(rel)
                            .unwrap_or_else(|| rel.clone());
                        let mut spr = Sprite::from_image(asset_server.load(resolved));
                        spr.color = Color::srgba(color[0], color[1], color[2], color[3]);
                        if size[0] > 0.0 && size[1] > 0.0 {
                            spr.custom_size = Some(Vec2::new(size[0], size[1]));
                        }
                        spr
                    }
                    None => Sprite::from_color(
                        Color::srgba(color[0], color[1], color[2], color[3]),
                        Vec2::new(size[0].max(1.0), size[1].max(1.0)),
                    ),
                };
                sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
                let vis = StagePropVisual {
                    name: id.clone(),
                    base_position: *pos,
                    base_scale: [1.0, 1.0],
                    scroll_factor: if *hud { [0.0, 0.0] } else { [1.0, 1.0] },
                    base_flip: [false, false],
                    base_angle_deg: 0.0,
                    z_layer: *z,
                    visible: true,
                    coord_mode: StageCoordMode::Center,
                    raw_size: *size,
                };
                let mut ec = commands.spawn((
                    sprite,
                    Transform::from_xyz(0.0, 0.0, *z),
                    Visibility::Visible,
                    vis,
                    DynamicScriptProp {
                        id: id.clone(),
                        hud: *hud,
                    },
                ));
                if *hud {
                    ec.insert(hud_render_layer());
                }
            }
            ScriptCommand::DespawnProp(id) => {
                for (entity, vis, _, _, dyn_opt) in props.iter() {
                    if dyn_opt.is_some_and(|d| d.id == *id) || vis.name == *id {
                        commands.entity(entity).despawn();
                    }
                }
            }
            ScriptCommand::SetPropNumber { id, prop, value } => {
                for (_, mut vis, mut sprite, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        match prop {
                            TweenProp::X => vis.base_position[0] = *value,
                            TweenProp::Y => vis.base_position[1] = *value,
                            TweenProp::ScaleX => vis.base_scale[0] = *value,
                            TweenProp::ScaleY => vis.base_scale[1] = *value,
                            TweenProp::Angle => vis.base_angle_deg = *value,
                            TweenProp::Alpha => {
                                sprite.color = sprite.color.with_alpha(value.clamp(0.0, 1.0));
                            }
                            _ => {}
                        }
                    }
                }
            }
            ScriptCommand::SetPropTint { id, color } => {
                for (_, vis, mut sprite, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
                    }
                }
            }
            ScriptCommand::SetPropVisible { id, visible } => {
                for (_, mut vis, _, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        vis.visible = *visible;
                    }
                }
            }
            ScriptCommand::PlaySfx { path, volume } => {
                let resolved = mods
                    .resolve_asset_server_path(path)
                    .unwrap_or_else(|| path.clone());
                let handle: Handle<AudioSource> = asset_server.load(resolved);
                spawn_one_shot(commands, handle, *volume);
            }
            ScriptCommand::SetShaderAdjustColor {
                target,
                hue,
                saturation,
                brightness,
                contrast,
            } => {
                pending_shaders.0.push(ShaderScriptCommand::SetAdjustColor {
                    target: target.clone(),
                    hue: *hue,
                    saturation: *saturation,
                    brightness: *brightness,
                    contrast: *contrast,
                });
            }
            ScriptCommand::SetShaderDropShadow {
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
                pending_shaders.0.push(ShaderScriptCommand::SetDropShadow {
                    target: target.clone(),
                    color: *color,
                    angle_deg: *angle_deg,
                    distance: *distance,
                    strength: *strength,
                    threshold: *threshold,
                    hue: *hue,
                    saturation: *saturation,
                    brightness: *brightness,
                    contrast: *contrast,
                    use_alt_mask: *use_alt_mask,
                    alt_mask_path: alt_mask_path.clone(),
                    mask_threshold: *mask_threshold,
                });
            }
            ScriptCommand::SetShaderWiggle {
                target,
                effect,
                speed,
                frequency,
                amplitude,
            } => {
                pending_shaders.0.push(ShaderScriptCommand::SetWiggle {
                    target: target.clone(),
                    effect_type: parse_wiggle_effect_type(effect),
                    speed: *speed,
                    frequency: *frequency,
                    amplitude: *amplitude,
                });
            }
            ScriptCommand::SetCameraRain {
                scale,
                intensity,
                color,
                time_scale_decay,
            } => {
                pending_shaders.0.push(ShaderScriptCommand::SetCameraRain {
                    scale: *scale,
                    intensity: *intensity,
                    color: *color,
                    time_scale_decay: *time_scale_decay,
                });
            }
            ScriptCommand::ClearShader { target } => {
                pending_shaders.0.push(ShaderScriptCommand::ClearShader {
                    target: target.clone(),
                });
            }
            _ => {}
        }
    }
}

