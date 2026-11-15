use std::collections::HashMap;

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{
    build_composite_clip, compute_atlas_whole_bounds, sample_default_composite_clip,
};
use crate::animations::{
    AnimationController, AtlasSettings, BaseAnimationData, SparrowAtlas, add_sparrow_animations,
    register_default_sparrow_idle,
};
use crate::mods::ModRegistry;
use crate::scripting::{LogLevel, ScriptCommand, ScriptPropAnimKind, TweenProp};

use super::super::camera::hud::hud_render_layer;
use super::super::characters::CharacterActor;
use super::super::characters::spawn::CompositeAffine2d;
use super::super::notes::data::NoteDirection;
use super::super::shaders::systems::{ShaderScriptCommand, parse_wiggle_effect_type};
use super::super::stage::schema::StageCoordMode;
use super::super::stage::sync::StageAtlasFrameSizes;
use super::super::stage::{DynamicScriptProp, StageCompositePiece, StageCompositeProp, StagePropVisual};
use super::actor_matches_target;
use super::hud_audio::spawn_one_shot;

#[allow(clippy::too_many_arguments)]
pub(super) fn apply_ecs_script_commands(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    mods: &ModRegistry,
    actors: &mut Query<&mut CharacterActor>,
    props: &mut Query<(
        Entity,
        &mut StagePropVisual,
        &mut Sprite,
        Option<&mut AnimationController>,
        Option<&DynamicScriptProp>,
        Option<&StageCompositeProp>,
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
                    for (_, vis, _, anim_opt, _, _) in props.iter_mut() {
                        if vis.name.eq_ignore_ascii_case(target)
                            && let Some(mut ctrl) = anim_opt
                        {
                            ctrl.play(anim, *force, false, 0);
                        }
                    }
                }
            }
            ScriptCommand::PlayAnimFrame { target, anim, force, frame } => {
                let mut matched_actor = false;
                for mut actor in actors.iter_mut() {
                    if actor_matches_target(&actor, target) {
                        actor.controller.play(anim, *force, false, *frame);
                        actor.hold_timer = 0.0;
                        actor.script_anim_override = true;
                        matched_actor = true;
                    }
                }
                if !matched_actor {
                    for (_, vis, _, anim_opt, _, _) in props.iter_mut() {
                        if vis.name.eq_ignore_ascii_case(target)
                            && let Some(mut ctrl) = anim_opt
                        {
                            ctrl.play(anim, *force, false, *frame);
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
                despawn_prop_by_id(commands, props, id, true);
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
            ScriptCommand::SpawnAnimatedProp {
                id,
                path,
                kind,
                anims,
                color,
                size,
                pos,
                z,
                hud,
                top_left,
            } => {
                despawn_prop_by_id(commands, props, id, true);
                let coord_mode = if *top_left {
                    StageCoordMode::TopLeft
                } else {
                    StageCoordMode::Center
                };
                spawn_animated_script_prop(
                    commands,
                    asset_server,
                    layouts,
                    mods,
                    id,
                    path,
                    *kind,
                    anims,
                    *color,
                    *size,
                    *pos,
                    *z,
                    *hud,
                    coord_mode,
                );
            }
            ScriptCommand::DespawnProp(id) => {
                despawn_prop_by_id(commands, props, id, false);
            }
            ScriptCommand::SetPropNumber { id, prop, value } => {
                for (_, mut vis, mut sprite, _, _, _) in props.iter_mut() {
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
                for (_, vis, mut sprite, _, _, _) in props.iter_mut() {
                    if vis.name == *id {
                        sprite.color = Color::srgba(color[0], color[1], color[2], color[3]);
                    }
                }
            }
            ScriptCommand::SetPropVisible { id, visible } => {
                for (_, mut vis, _, _, _, _) in props.iter_mut() {
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

fn despawn_prop_by_id(
    commands: &mut Commands,
    props: &Query<(
        Entity,
        &mut StagePropVisual,
        &mut Sprite,
        Option<&mut AnimationController>,
        Option<&DynamicScriptProp>,
        Option<&StageCompositeProp>,
    )>,
    id: &str,
    only_dynamic: bool,
) {
    for (entity, vis, _, _, dyn_opt, comp_opt) in props.iter() {
        let matched = if only_dynamic {
            dyn_opt.is_some_and(|d| d.id == id)
        } else {
            dyn_opt.is_some_and(|d| d.id == id) || vis.name == id
        };
        if matched {
            if let Some(comp) = comp_opt {
                for &piece in &comp.piece_entities {
                    commands.entity(piece).despawn();
                }
            }
            commands.entity(entity).despawn();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_animated_script_prop(
    commands: &mut Commands,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
    mods: &ModRegistry,
    id: &str,
    path: &str,
    kind: ScriptPropAnimKind,
    anims: &[BaseAnimationData],
    color: [f32; 4],
    size: [f32; 2],
    pos: [f32; 2],
    z: f32,
    hud: bool,
    coord_mode: StageCoordMode,
) {
    let tint = Color::srgba(color[0], color[1], color[2], color[3]);
    let scroll_factor = if hud { [0.0, 0.0] } else { [1.0, 1.0] };
    let make_vis = |raw_size: [f32; 2]| StagePropVisual {
        name: id.to_string(),
        base_position: pos,
        base_scale: [1.0, 1.0],
        scroll_factor,
        base_flip: [false, false],
        base_angle_deg: 0.0,
        z_layer: z,
        visible: true,
        coord_mode,
        raw_size,
    };

    match kind {
        ScriptPropAnimKind::Static => {
            let resolved_file = mods
                .resolve_file(path)
                .or_else(|| mods.resolve_file(&format!("{path}.png")));
            let raw_size = resolved_file
                .as_deref()
                .and_then(super::super::stage::paths::read_image_dimensions)
                .unwrap_or(size);
            let srv = resolved_file
                .as_deref()
                .map(|p| mods.to_asset_server_path(p))
                .unwrap_or_else(|| path.to_string());
            let mut spr = Sprite::from_image(asset_server.load(srv));
            spr.color = tint;
            let mut ec = commands.spawn((
                spr,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Visible,
                make_vis(raw_size),
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
        }
        ScriptPropAnimKind::Rect => {
            let spr = Sprite::from_color(tint, Vec2::new(size[0].max(1.0), size[1].max(1.0)));
            let mut ec = commands.spawn((
                spr,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Visible,
                make_vis(size),
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
        }
        ScriptPropAnimKind::Sparrow => {
            let Some(img_path) = mods
                .resolve_file(path)
                .or_else(|| mods.resolve_file(&format!("{path}.png")))
            else {
                warn!("spawn_sparrow({id}): missing image for '{path}'");
                return;
            };
            let xml_path = img_path.with_extension("xml");
            let Ok(atlas) = SparrowAtlas::from_file(&xml_path) else {
                warn!("spawn_sparrow({id}): failed to parse '{}'", xml_path.display());
                return;
            };
            let (layout, indices) = atlas.build_layout();
            let mut controller = AnimationController::default();
            if anims.is_empty() {
                register_default_sparrow_idle(&atlas, &indices, &mut controller);
            } else {
                add_sparrow_animations(&atlas, &indices, &mut controller, anims);
            }
            if let Some(first_name) = anims.first().map(|a| a.name.as_str()).or(Some("idle"))
                && controller.has_animation(first_name)
            {
                controller.play(first_name, true, false, 0);
            }
            let frame_sizes: HashMap<usize, [f32; 2]> = atlas
                .frames
                .iter()
                .filter_map(|f| {
                    let (uw, uh) = if f.rotated {
                        (f.height, f.width)
                    } else {
                        (f.width, f.height)
                    };
                    indices
                        .get(&f.name)
                        .copied()
                        .map(|idx| (idx, [f.frame_width.max(uw) as f32, f.frame_height.max(uh) as f32]))
                })
                .collect();
            let frame_size = controller
                .current_atlas_index()
                .and_then(|idx| frame_sizes.get(&idx).copied())
                .unwrap_or([0.0, 0.0]);
            let initial_idx = controller.current_atlas_index().unwrap_or(0);
            let mut spr = Sprite::from_atlas_image(
                asset_server.load(mods.to_asset_server_path(&img_path)),
                TextureAtlas {
                    layout: layouts.add(layout),
                    index: initial_idx,
                },
            );
            spr.color = tint;
            let mut ec = commands.spawn((
                spr,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Visible,
                controller,
                make_vis(frame_size),
                StageAtlasFrameSizes(frame_sizes),
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
        }
        ScriptPropAnimKind::Atlas => {
            let Some(dir) = mods.resolve_dir(path) else {
                warn!("spawn_atlas({id}): missing atlas directory '{path}'");
                return;
            };
            let Ok(atlas) = AnimateAtlas::from_dir(&dir) else {
                warn!("spawn_atlas({id}): failed to parse '{}'", dir.display());
                return;
            };
            let (atlas_layouts, indices) = atlas.build_layouts();
            let textures: Vec<(Handle<Image>, Handle<TextureAtlasLayout>)> = atlas_layouts
                .into_iter()
                .enumerate()
                .map(|(i, layout)| {
                    let img_rel = mods.to_asset_server_path(&dir.join(format!("spritemap{}.png", i + 1)));
                    (asset_server.load(img_rel), layouts.add(layout))
                })
                .collect();
            let Some((base_tex, base_layout)) = textures.first() else {
                return;
            };
            let mut controller = AnimationController::default();
            let mut clips = HashMap::new();
            if anims.is_empty() {
                if let Some(clip) = sample_default_composite_clip(&atlas, &indices) {
                    let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                    controller.add_anim("idle", frame_ids, clip.fps, clip.looped);
                    clips.insert("idle".to_string(), clip);
                }
            } else {
                for anim in anims {
                    if let Some(clip) = build_composite_clip(&atlas, &indices, anim) {
                        let frame_ids: Vec<usize> = (0..clip.frames.len()).collect();
                        controller.add_anim(&anim.name, frame_ids, clip.fps, clip.looped);
                        controller.set_offset(&anim.name, clip.offset);
                        controller.set_flip(&anim.name, clip.flip);
                        clips.insert(anim.name.clone(), clip);
                    }
                }
            }
            if let Some(first_name) = anims.first().map(|a| a.name.as_str()).or(Some("idle"))
                && controller.has_animation(first_name)
            {
                controller.play(first_name, true, false, 0);
            }
            let max_pieces = clips
                .values()
                .flat_map(|c| c.frames.iter())
                .map(Vec::len)
                .max()
                .unwrap_or(1)
                .max(1);
            let (bounds_min, bounds_size) = clips
                .values()
                .next()
                .map(|c| (c.bounds_min, c.bounds_size))
                .unwrap_or_else(|| compute_atlas_whole_bounds(&atlas, &indices));
            let mut piece_entities = Vec::with_capacity(max_pieces);
            for i in 0..max_pieces {
                let mut pe = commands.spawn((
                    Sprite::from_atlas_image(
                        base_tex.clone(),
                        TextureAtlas {
                            layout: base_layout.clone(),
                            index: 0,
                        },
                    ),
                    Transform::from_xyz(0.0, 0.0, z + (i as f32) * 1e-6),
                    Visibility::Hidden,
                    StageCompositePiece,
                    CompositeAffine2d::default(),
                ));
                if hud {
                    pe.insert(hud_render_layer());
                }
                piece_entities.push(pe.id());
            }
            let mut placeholder = Sprite::default();
            placeholder.color = tint;
            let composite = StageCompositeProp {
                textures,
                clips,
                bounds_min,
                bounds_size,
                atlas_settings: AtlasSettings::default(),
                stage_matrix: atlas.stage_matrix,
                piece_entities,
            };
            let mut ec = commands.spawn((
                placeholder,
                Transform::from_xyz(0.0, 0.0, z),
                Visibility::Hidden,
                controller,
                make_vis(bounds_size),
                composite,
                DynamicScriptProp {
                    id: id.to_string(),
                    hud,
                },
            ));
            if hud {
                ec.insert(hud_render_layer());
            }
        }
    }
}

