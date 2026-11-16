use bevy::prelude::*;

use crate::play::characters::{
    CharacterActor, CharacterTrailConfig, CharacterTrailState, CharacterTrailVisual,
    CharacterUnderlayConfig, load_single_character, spawn_single_character,
    spawn_single_character_with_underlay,
};
use crate::play::notes::data::NoteDirection;
use crate::play::stage::schema::stage_layer_z;
use crate::scripting::ScriptCommand;

use super::super::{ScriptTickParams, actor_matches_target};

fn despawn_actor_with_parts(
    commands: &mut Commands,
    ent: Entity,
    actor: &CharacterActor,
) {
    if actor.sheet_entity != Entity::PLACEHOLDER {
        commands.entity(actor.sheet_entity).despawn();
    }
    for &p_ent in &actor.piece_entities {
        if p_ent != Entity::PLACEHOLDER {
            commands.entity(p_ent).despawn();
        }
    }
    actor.despawn_composite_rt(commands);
    for &t_ent in &actor.trail_entities {
        if t_ent != Entity::PLACEHOLDER {
            commands.entity(t_ent).despawn();
        }
    }
    commands.entity(ent).despawn();
}

pub(super) fn handle_char_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    scripts: &mut ScriptTickParams,
    pending_spawned: &std::collections::HashMap<String, Entity>,
) {
    match cmd {
        ScriptCommand::PlayAnim { target, anim, force } => {
            let mut matched_actor = false;
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    let char_actor = &mut *actor;
                    char_actor
                        .bopper
                        .play_animation(&mut char_actor.controller, anim, *force, *force);
                    char_actor.hold_timer = 0.0;
                    char_actor.script_anim_override = true;
                    matched_actor = true;
                }
            }
            if !matched_actor {
                let mut matched_prop = false;
                for (_, vis, _, anim_opt, _, _) in scripts.props.iter_mut() {
                    if vis.name.eq_ignore_ascii_case(target)
                        && let Some(mut ctrl) = anim_opt
                    {
                        ctrl.play(anim, *force, false, 0);
                        matched_prop = true;
                    }
                }
                if !matched_prop && let Some(&ent) = pending_spawned.get(target) {
                    let anim_name = anim.clone();
                    let force_flag = *force;
                    commands.queue(move |world: &mut World| {
                        if let Some(mut ctrl) = world.get_mut::<crate::animations::AnimationController>(ent) {
                            ctrl.play(&anim_name, force_flag, false, 0);
                        }
                    });
                }
            }
        }
        ScriptCommand::PlayAnimFrame { target, anim, force, frame } => {
            let mut matched_actor = false;
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.controller.play(anim, *force, false, *frame);
                    actor.hold_timer = 0.0;
                    actor.script_anim_override = true;
                    matched_actor = true;
                }
            }
            if !matched_actor {
                let mut matched_prop = false;
                for (_, vis, _, anim_opt, _, _) in scripts.props.iter_mut() {
                    if vis.name.eq_ignore_ascii_case(target)
                        && let Some(mut ctrl) = anim_opt
                    {
                        ctrl.play(anim, *force, false, *frame);
                        matched_prop = true;
                    }
                }
                if !matched_prop && let Some(&ent) = pending_spawned.get(target) {
                    let anim_name = anim.clone();
                    let force_flag = *force;
                    let frame_idx = *frame;
                    commands.queue(move |world: &mut World| {
                        if let Some(mut ctrl) = world.get_mut::<crate::animations::AnimationController>(ent) {
                            ctrl.play(&anim_name, force_flag, false, frame_idx);
                        }
                    });
                }
            }
        }
        ScriptCommand::PlaySing { target, dir, miss, suffix } => {
            let note_dir = NoteDirection::from_int(*dir as i32);
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.play_sing(note_dir, *miss, suffix);
                    actor.hold_timer = 0.0;
                    actor.script_anim_override = true;
                }
            }
        }
        ScriptCommand::CharDance { target, force } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.dance(*force);
                    actor.script_anim_override = true;
                }
            }
        }
        ScriptCommand::SetCharacterPos { target, x, y } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.slot.position = [*x, *y];
                    actor.recompute_camera_focus();
                }
            }
        }
        ScriptCommand::SetCharacterScale { target, sx, sy } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.slot.scale = [*sx, *sy];
                }
            }
        }
        ScriptCommand::SetCharacterTint { target, color } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.mod_tint = *color;
                }
            }
        }
        ScriptCommand::SetCharacterAlpha { target, alpha } => {
            let alpha = alpha.clamp(0.0, 1.0);
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.mod_tint[3] = alpha;
                }
            }
        }
        ScriptCommand::SetCharacterVisible { target, visible } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.mod_visible = *visible;
                }
            }
        }
        ScriptCommand::SetCharacterDanceEvery { target, beats } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.bopper.dance_every = *beats;
                }
            }
        }
        ScriptCommand::SetCharFlip { target, flip_x, flip_y } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.slot.flip = [*flip_x, *flip_y];
                }
            }
        }
        ScriptCommand::SetCharZ { target, z } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.mod_z = Some(*z);
                }
            }
        }
        ScriptCommand::SetCharTrail {
            target,
            length,
            delay_s,
            alpha,
            diff,
            z_layer,
        } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if !actor_matches_target(&actor, target) {
                    continue;
                }
                for ent in actor.trail_entities.drain(..) {
                    commands.entity(ent).despawn();
                }
                let layer = z_layer.unwrap_or(actor.slot.layer.saturating_sub(1));
                let z_f32 = stage_layer_z(layer, 0);
                let mut entities = Vec::with_capacity(*length);
                for _ in 0..*length {
                    let sprite = match actor.sources.first() {
                        Some(src) => Sprite::from_atlas_image(
                            src.texture.clone(),
                            TextureAtlas {
                                layout: src.layout.clone(),
                                index: 0,
                            },
                        ),
                        None => Sprite::default(),
                    };
                    entities.push(
                        commands
                            .spawn((
                                sprite,
                                Transform::from_xyz(0.0, 0.0, z_f32),
                                Visibility::Hidden,
                                CharacterTrailVisual,
                            ))
                            .id(),
                    );
                }
                actor.trail_entities = entities;
                actor.trail = Some(CharacterTrailState::new(CharacterTrailConfig {
                    length: *length,
                    delay_s: *delay_s,
                    alpha: *alpha,
                    diff: *diff,
                    z_layer: z_f32,
                }));
            }
        }
        ScriptCommand::ClearCharTrail { target } => {
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    for ent in actor.trail_entities.drain(..) {
                        commands.entity(ent).despawn();
                    }
                    actor.trail = None;
                }
            }
        }
        ScriptCommand::SetCharUnderlay {
            target,
            char_id,
            offset,
            z_offset,
        } => {
            let mut parent_specs = Vec::new();
            for (_, actor) in scripts.actors.iter() {
                if actor_matches_target(&actor, target) {
                    let mut underlay_slot = actor.slot.clone();
                    underlay_slot.layer = underlay_slot.layer.saturating_add(*z_offset);
                    underlay_slot.opacity = 1.0;
                    parent_specs.push((actor.role, underlay_slot, actor.facing_flip_x));
                }
            }
            if !parent_specs.is_empty() {
                for (ent, actor) in scripts.actors.iter() {
                    if let Some(cfg) = actor.mirror_of
                        && parent_specs.iter().any(|(r, _, _)| *r == cfg.parent_role)
                    {
                        despawn_actor_with_parts(commands, ent, &actor);
                    }
                }
                for (parent_role, underlay_slot, parent_flip_x) in parent_specs {
                    if let Ok(mut loaded) = load_single_character(
                        &scripts.mods,
                        char_id,
                        parent_role,
                        underlay_slot,
                        &scripts.asset_server,
                        &mut scripts.layouts,
                    ) {
                        loaded.facing_flip_x = parent_flip_x;
                        if let Some(tr) = scripts.preload.as_deref_mut() {
                            tr.register_character(&loaded);
                        }
                        spawn_single_character_with_underlay(
                            commands,
                            &loaded,
                            Some(CharacterUnderlayConfig {
                                parent_role,
                                offset: *offset,
                            }),
                        );
                    }
                }
            }
        }
        ScriptCommand::ClearCharUnderlay { target } => {
            let mut roles = Vec::new();
            for (_, actor) in scripts.actors.iter() {
                if actor_matches_target(&actor, target) {
                    roles.push(actor.role);
                }
            }
            if !roles.is_empty() {
                for (ent, actor) in scripts.actors.iter() {
                    if let Some(cfg) = actor.mirror_of
                        && roles.contains(&cfg.parent_role)
                    {
                        despawn_actor_with_parts(commands, ent, &actor);
                    }
                }
            }
        }
        ScriptCommand::SwapChar { role, char_id } => {
            let mut replacement_specs = Vec::new();
            for (ent, actor) in scripts.actors.iter() {
                if actor_matches_target(&actor, role) {
                    replacement_specs.push((
                        ent,
                        actor.role,
                        actor.slot.clone(),
                    ));
                }
            }
            for (ent, role_enum, slot) in replacement_specs {
                for (u_ent, u_actor) in scripts.actors.iter() {
                    if let Some(cfg) = u_actor.mirror_of
                        && cfg.parent_role == role_enum
                    {
                        despawn_actor_with_parts(commands, u_ent, &u_actor);
                    }
                }
                if let Ok(loaded) = load_single_character(
                    &scripts.mods,
                    char_id,
                    role_enum,
                    slot,
                    &scripts.asset_server,
                    &mut scripts.layouts,
                ) {
                    if let Some(tr) = scripts.preload.as_deref_mut() {
                        tr.register_character(&loaded);
                    }
                    if let Ok((_, actor)) = scripts.actors.get(ent) {
                        despawn_actor_with_parts(commands, ent, &actor);
                    }
                    spawn_single_character(commands, &loaded);
                }
            }
        }
        _ => {}
    }
}
