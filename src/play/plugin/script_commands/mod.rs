mod chars;
mod hud_audio;
mod props;
mod text;

use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::stage::{DynamicScriptProp, DynamicScriptText, StageCompositeProp, StagePropVisual};
use crate::scripting::{LogLevel, ScriptCommand};

use super::super::shaders::systems::{ShaderScriptCommand, parse_wiggle_effect_type};
use super::{AudioTickParams, ScriptHudAndTextParams, ScriptTickParams};
use chars::handle_char_cmd;
use hud_audio::handle_hud_audio_cmd;
pub(crate) use props::ScriptSparrowData;
use props::{build_prop_index_ci, handle_prop_cmd};
use text::handle_text_cmd;

pub(crate) fn apply_ecs_script_commands(
    commands: &mut Commands,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
    cmds: &[ScriptCommand],
) {
    // Build an indexed lookup when batches address props repeatedly (e.g. per-note visualizers)
    // and invalidate if a spawn/despawn mutates prop entities mid-batch.
    let mut prop_index: Option<HashMap<String, Vec<Entity>>> =
        (cmds.len() >= 2).then(|| build_prop_index_ci(&scripts.props));
    let mut pending_spawned: HashMap<String, Entity> = HashMap::new();
    let mut pending_chars: HashMap<String, Entity> = HashMap::new();
    let mut pending_texts: HashMap<String, Entity> = HashMap::new();
    let mut queue: VecDeque<ScriptCommand> = cmds.iter().cloned().collect();
    let mut follow_up: Vec<ScriptCommand> = Vec::new();

    while let Some(cmd_owned) = queue.pop_front() {
        let cmd = &cmd_owned;
        match cmd {
            ScriptCommand::Log { level, mod_id, message } => match level {
                LogLevel::Info => info!("[mod:{mod_id}] {message}"),
                LogLevel::Warn => warn!("[mod:{mod_id}] {message}"),
                LogLevel::Error => error!("[mod:{mod_id}] {message}"),
            },
            ScriptCommand::SpawnProp { .. }
            | ScriptCommand::SpawnAnimatedProp { .. }
            | ScriptCommand::DespawnProp(_)
            | ScriptCommand::SetPropNumber { .. }
            | ScriptCommand::SetPropTint { .. }
            | ScriptCommand::SetPropVisible { .. }
            | ScriptCommand::SetPropFlip { .. }
            | ScriptCommand::SetPropScroll { .. }
            | ScriptCommand::SetPropZ { .. }
            | ScriptCommand::SetPropSize { .. }
            | ScriptCommand::SetPropVelocity { .. }
            | ScriptCommand::SetPropRepeat { .. }
            | ScriptCommand::SetPropAffine { .. }
            | ScriptCommand::SetPropTexture { .. }
            | ScriptCommand::AddPropAnim { .. } => {
                handle_prop_cmd(
                    cmd,
                    commands,
                    &scripts.asset_server,
                    &mut scripts.layouts,
                    &mut scripts.images,
                    &scripts.mods,
                    scripts.preload.as_deref_mut(),
                    &mut scripts.props,
                    &scripts.sparrows,
                    &mut hud_texts.texts,
                    &mut prop_index,
                    &mut pending_spawned,
                );
            }
            ScriptCommand::SetPropOverlay { .. } => {
                handle_prop_cmd(
                    cmd,
                    commands,
                    &scripts.asset_server,
                    &mut scripts.layouts,
                    &mut scripts.images,
                    &scripts.mods,
                    scripts.preload.as_deref_mut(),
                    &mut scripts.props,
                    &scripts.sparrows,
                    &mut hud_texts.texts,
                    &mut prop_index,
                    &mut pending_spawned,
                );
                handle_text_cmd(
                    cmd,
                    commands,
                    &scripts.asset_server,
                    &scripts.mods,
                    &mut hud_texts.texts,
                    &mut pending_texts,
                );
            }
            ScriptCommand::SpawnText { .. }
            | ScriptCommand::SetText { .. }
            | ScriptCommand::SetTextStyle { .. } => {
                handle_text_cmd(
                    cmd,
                    commands,
                    &scripts.asset_server,
                    &scripts.mods,
                    &mut hud_texts.texts,
                    &mut pending_texts,
                );
            }
            ScriptCommand::SetShaderAdjustColor {
                target,
                hue,
                saturation,
                brightness,
                contrast,
            } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetAdjustColor {
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
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetDropShadow {
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
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetWiggle {
                    target: target.clone(),
                    effect_type: parse_wiggle_effect_type(effect),
                    speed: *speed,
                    frequency: *frequency,
                    amplitude: *amplitude,
                });
            }
            ScriptCommand::SetCameraRain {
                scale,
                start_intensity,
                end_intensity,
                color,
                time_scale_decay,
            } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetCameraRain {
                    scale: *scale,
                    start_intensity: *start_intensity,
                    end_intensity: *end_intensity,
                    color: *color,
                    time_scale_decay: *time_scale_decay,
                });
            }
            ScriptCommand::SetShaderOverlayBlend { target, aux_path } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetOverlayBlend {
                    target: target.clone(),
                    aux_path: aux_path.clone(),
                });
            }
            ScriptCommand::SetShaderRainSprite {
                target,
                scale,
                intensity,
                color,
            } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetRainSprite {
                    target: target.clone(),
                    scale: *scale,
                    intensity: *intensity,
                    color: *color,
                });
            }
            ScriptCommand::SetShaderBuilding { target, alpha_shit } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetBuilding {
                    target: target.clone(),
                    alpha_shit: *alpha_shit,
                });
            }
            ScriptCommand::SetBlendMode { target, mode } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetBlendMode {
                    target: target.clone(),
                    mode: mode.clone(),
                });
            }
            ScriptCommand::SetCustomShader { target, path } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetCustomShader {
                    target: target.clone(),
                    path: path.clone(),
                });
            }
            ScriptCommand::SetShaderParam { target, key, values } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetShaderParam {
                    target: target.clone(),
                    key: key.clone(),
                    values: *values,
                });
            }
            ScriptCommand::SetShaderTex { target, path } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::SetShaderTex {
                    target: target.clone(),
                    path: path.clone(),
                });
            }
            ScriptCommand::ClearShader { target } => {
                scripts.pending_shaders.0.push(ShaderScriptCommand::ClearShader {
                    target: target.clone(),
                });
            }
            ScriptCommand::SetBarColors { .. }
            | ScriptCommand::SetIcon { .. }
            | ScriptCommand::SetIconFlip { .. }
            | ScriptCommand::SetIconVisible { .. }
            | ScriptCommand::SetIconScale { .. }
            | ScriptCommand::SetIconBop { .. }
            | ScriptCommand::PlaySfx { .. }
            | ScriptCommand::PreloadSound { .. }
            | ScriptCommand::PlaySound { .. }
            | ScriptCommand::StopSound(_)
            | ScriptCommand::PauseSound { .. }
            | ScriptCommand::SetSoundParam { .. }
            | ScriptCommand::SeekSong(_)
            | ScriptCommand::TakeScreenshot(_)
            | ScriptCommand::LoadSong { .. } => {
                handle_hud_audio_cmd(cmd, commands, audio_res, scripts, hud_texts);
            }
            _ => {
                handle_char_cmd(
                    cmd,
                    commands,
                    state,
                    playlist,
                    scripts,
                    &pending_spawned,
                    &mut pending_chars,
                    &mut follow_up,
                );
                if !follow_up.is_empty() {
                    for f in follow_up.drain(..).rev() {
                        queue.push_front(f);
                    }
                }
            }
        }
    }
}

pub(crate) type PropsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut StagePropVisual,
        &'static mut Sprite,
        Option<&'static mut AnimationController>,
        Option<&'static DynamicScriptProp>,
        Option<&'static mut StageCompositeProp>,
    ),
>;

pub(crate) type TextsQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut DynamicScriptText,
        &'static mut Text2d,
        &'static mut TextFont,
        &'static mut TextColor,
        &'static mut bevy::sprite::Anchor,
        Option<&'static mut bevy::sprite::Text2dShadow>,
    ),
>;
