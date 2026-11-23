use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bevy::prelude::*;

use crate::animations::animate_atlas::AnimateAtlas;
use crate::animations::atlas_composite::{collect_symbol_anchors, sample_default_composite_clip};
use crate::mods::ModRegistry;
use crate::play::characters::{
    CharacterActor, CharacterRole, CharacterTrailConfig, CharacterTrailState, CharacterTrailVisual,
    CharacterUnderlayConfig, SymbolAtlasAttachment, append_character_sidecar_scripts,
    ensure_actor_piece_capacity, ensure_actor_piece_capacity_world, load_single_character,
    spawn_single_character, spawn_single_character_with_underlay,
};
use crate::play::notes::data::NoteDirection;
use crate::play::playlist::PlayStatePlaylist;
use crate::play::scene::PlayState;
use crate::play::stage::schema::{StageCharacterSlot, stage_layer_z};
use crate::scripting::ScriptCommand;

use super::super::{ScriptTickParams, actor_matches_target};

fn parse_sing_role(role: &str) -> CharacterRole {
    match role.trim().to_ascii_lowercase().as_str() {
        "bf" | "boyfriend" | "player" | "0" => CharacterRole::Player,
        "dad" | "opponent" | "1" => CharacterRole::Opponent,
        "gf" | "girlfriend" | "spectator" | "2" => CharacterRole::Spectator,
        _ => CharacterRole::Other,
    }
}

fn resolve_atlas_dir(mods: &ModRegistry, atlas_path: &str) -> Option<PathBuf> {
    let trimmed = atlas_path.trim().trim_start_matches('/');
    let mut candidates = vec![trimmed.to_string()];
    if !trimmed.starts_with("gameplay/") {
        candidates.push(format!("gameplay/{trimmed}"));
        if !trimmed.starts_with("stages/") {
            candidates.push(format!("gameplay/stages/{trimmed}"));
        }
    }
    candidates.into_iter().find_map(|rel| {
        let dir = mods.resolve_dir(&rel)?;
        dir.join("Animation.json").is_file().then_some(dir)
    })
}

fn apply_actor_patch<F>(
    target: &str,
    commands: &mut Commands,
    scripts: &mut ScriptTickParams,
    pending_chars: &HashMap<String, Entity>,
    mut patch: F,
) -> bool
where
    F: FnMut(&mut CharacterActor) + Send + Sync + Clone + 'static,
{
    let mut matched = false;
    for (_, mut actor) in scripts.actors.iter_mut() {
        if actor_matches_target(&actor, target) {
            patch(&mut actor);
            matched = true;
        }
    }
    if !matched && let Some(&ent) = pending_chars.get(&target.trim().to_ascii_lowercase()) {
        commands.queue(move |world: &mut World| {
            if let Some(mut actor) = world.get_mut::<CharacterActor>(ent) {
                patch(&mut actor);
            }
        });
        return true;
    }
    matched
}

fn required_piece_capacity(actor: &CharacterActor) -> usize {
    let base_max = actor
        .clips
        .values()
        .filter_map(|v| match v {
            crate::play::characters::behavior::ClipVisual::Composite { clip, .. } => Some(clip),
            _ => None,
        })
        .flat_map(|c| c.frames.iter().map(Vec::len))
        .max()
        .unwrap_or(0);
    let attach_extra: usize = actor
        .attachments
        .iter()
        .map(|att| {
            let max_src = att.frames.iter().map(Vec::len).max().unwrap_or(0);
            let max_anchors = att
                .per_clip_anchors
                .values()
                .flat_map(|frames| frames.iter().map(Vec::len))
                .max()
                .unwrap_or(0);
            max_src * max_anchors
        })
        .sum();
    base_max + attach_extra
}

fn apply_attachment_patch<F>(
    target: &str,
    attach_id: &str,
    commands: &mut Commands,
    scripts: &mut ScriptTickParams,
    pending_chars: &HashMap<String, Entity>,
    patch: F,
) where
    F: Fn(&mut SymbolAtlasAttachment) + Send + Sync + Clone + 'static,
{
    let aid = attach_id.to_string();
    apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
        if let Some(att) = actor
            .attachments
            .iter_mut()
            .find(|a| a.id.eq_ignore_ascii_case(&aid))
        {
            patch(att);
        }
    });
}

fn insert_attachment_on_actor(
    actor: &mut CharacterActor,
    attach_id: &str,
    symbol_keyword: &str,
    textures: &[(Handle<Image>, Handle<TextureAtlasLayout>)],
    frames: &[crate::animations::atlas_composite::CompositePiece],
    all_frames: &[Vec<crate::animations::atlas_composite::CompositePiece>],
    bounds_min: [f32; 2],
    bounds_size: [f32; 2],
) -> usize {
    let _ = frames;
    let mut per_clip_anchors = HashMap::new();
    for (clip_name, clip_vis) in &actor.clips {
        if let crate::play::characters::behavior::ClipVisual::Composite {
            atlas: Some(atlas),
            indices: Some(indices),
            anim_data: Some(anim_data),
            ..
        } = clip_vis
        {
            let anchors = collect_symbol_anchors(atlas, indices, anim_data, symbol_keyword);
            if anchors.iter().any(|f| !f.is_empty()) {
                per_clip_anchors.insert(clip_name.clone(), anchors);
            }
        }
    }
    let att = SymbolAtlasAttachment {
        id: attach_id.to_string(),
        textures: textures.to_vec(),
        frames: all_frames.to_vec(),
        bounds_min,
        bounds_size,
        cur_frame: 0,
        offset: [0.0, 0.0],
        angle_deg: 0.0,
        flip_x: false,
        alpha: 1.0,
        visible: true,
        per_clip_anchors,
    };
    if let Some(existing) = actor
        .attachments
        .iter_mut()
        .find(|a| a.id.eq_ignore_ascii_case(attach_id))
    {
        *existing = att;
    } else {
        actor.attachments.push(att);
    }
    required_piece_capacity(actor)
}

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

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_char_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    state: &mut PlayState,
    playlist: &mut PlayStatePlaylist,
    scripts: &mut ScriptTickParams,
    pending_spawned: &HashMap<String, Entity>,
    pending_chars: &mut HashMap<String, Entity>,
    follow_up_cmds: &mut Vec<ScriptCommand>,
) {
    match cmd {
        ScriptCommand::PlayAnim { target, anim, force } => {
            let mut matched_actor = false;
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    let char_actor = &mut *actor;
                    if *force {
                        char_actor.bopper.can_play_other_anims = true;
                    }
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
                } else if !matched_prop
                    && let Some(&ent) = pending_chars.get(&target.trim().to_ascii_lowercase())
                {
                    let anim_name = anim.clone();
                    let force_flag = *force;
                    commands.queue(move |world: &mut World| {
                        if let Some(mut actor) = world.get_mut::<CharacterActor>(ent) {
                            let char_actor = &mut *actor;
                            if force_flag {
                                char_actor.bopper.can_play_other_anims = true;
                            }
                            char_actor.bopper.play_animation(
                                &mut char_actor.controller,
                                &anim_name,
                                force_flag,
                                force_flag,
                            );
                            char_actor.hold_timer = 0.0;
                            char_actor.script_anim_override = true;
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
                } else if !matched_prop
                    && let Some(&ent) = pending_chars.get(&target.trim().to_ascii_lowercase())
                {
                    let anim_name = anim.clone();
                    let force_flag = *force;
                    let frame_idx = *frame;
                    commands.queue(move |world: &mut World| {
                        if let Some(mut actor) = world.get_mut::<CharacterActor>(ent) {
                            actor.controller.play(&anim_name, force_flag, false, frame_idx);
                            actor.hold_timer = 0.0;
                            actor.script_anim_override = true;
                        }
                    });
                }
            }
        }
        ScriptCommand::PlaySing { target, dir, miss, suffix } => {
            let note_dir = NoteDirection::from_int(*dir as i32);
            let miss_flag = *miss;
            let suffix_owned = suffix.clone();
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.play_sing(note_dir, miss_flag, &suffix_owned);
                actor.hold_timer = 0.0;
                actor.script_anim_override = true;
            });
        }
        ScriptCommand::CharDance { target, force } => {
            let force_flag = *force;
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.dance(force_flag);
                actor.script_anim_override = true;
            });
        }
        ScriptCommand::SetCharacterPos { target, x, y } => {
            let pos = [*x, *y];
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.slot.position = pos;
                actor.recompute_camera_focus();
            });
        }
        ScriptCommand::SetCharacterScale { target, sx, sy } => {
            let scale = [*sx, *sy];
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.slot.scale = scale;
            });
        }
        ScriptCommand::SetCharacterTint { target, color } => {
            let tint = *color;
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.mod_tint = tint;
            });
        }
        ScriptCommand::SetCharacterAlpha { target, alpha } => {
            let alpha = alpha.clamp(0.0, 1.0);
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.mod_tint[3] = alpha;
            });
        }
        ScriptCommand::SetCharacterVisible { target, visible } => {
            let vis = *visible;
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.mod_visible = vis;
            });
        }
        ScriptCommand::SetCharacterDanceEvery { target, beats } => {
            let beats_val = *beats;
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.bopper.dance_every = beats_val;
            });
        }
        ScriptCommand::SetCharFlip { target, flip_x, flip_y } => {
            let flip = [*flip_x, *flip_y];
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.slot.flip = flip;
            });
        }
        ScriptCommand::SetCharZ { target, z } => {
            let z_val = *z;
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.mod_z = Some(z_val);
            });
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
                        &mut scripts.images,
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
                    &mut scripts.images,
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
        ScriptCommand::SpawnChar {
            id,
            char_id,
            pos,
            z_layer,
            scroll,
            visible,
        } => {
            let mut removed_entities = HashSet::new();
            let mut old_ids = Vec::new();
            for (ent, actor) in scripts.actors.iter() {
                if actor.mirror_of.is_none() && actor.id.eq_ignore_ascii_case(id) {
                    despawn_actor_with_parts(commands, ent, &actor);
                    removed_entities.insert(ent);
                    old_ids.push(actor.id.clone());
                }
            }
            if !removed_entities.is_empty() {
                for (u_ent, u_actor) in scripts.actors.iter() {
                    if !removed_entities.contains(&u_ent)
                        && u_actor.mirror_of.is_some()
                        && old_ids.iter().any(|old| u_actor.id.eq_ignore_ascii_case(old))
                    {
                        despawn_actor_with_parts(commands, u_ent, &u_actor);
                    }
                }
            }
            let slot = StageCharacterSlot {
                layer: *z_layer,
                position: *pos,
                scroll_factor: *scroll,
                visible: *visible,
                ..Default::default()
            };
            match load_single_character(
                &scripts.mods,
                char_id,
                CharacterRole::Other,
                slot,
                &scripts.asset_server,
                &mut scripts.layouts,
                &mut scripts.images,
            ) {
                Ok(mut loaded) => {
                    let should_load_script = loaded.data.script;
                    let json_path = loaded.json_path.clone();
                    if !id.is_empty() {
                        loaded.id = id.clone();
                    }
                    if let Some(tr) = scripts.preload.as_deref_mut() {
                        tr.register_character(&loaded);
                    }
                    let ent = spawn_single_character(commands, &loaded);
                    pending_chars.insert(loaded.id.to_ascii_lowercase(), ent);
                    pending_chars.insert(char_id.to_ascii_lowercase(), ent);

                    if should_load_script {
                        let supported = scripts.script_host.supported_extensions();
                        let mut discovered = Vec::new();
                        append_character_sidecar_scripts(
                            &scripts.mods,
                            char_id,
                            &json_path,
                            &supported,
                            &mut discovered,
                        );
                        for disc in &discovered {
                            if let Some(script_id) = scripts.script_host.load_discovered_single(disc) {
                                let cmds = state.run_instance_create(
                                    &script_id,
                                    playlist,
                                    &mut scripts.script_host,
                                    &scripts.mods,
                                    &mut scripts.mod_store,
                                );
                                follow_up_cmds.extend(cmds);
                            }
                        }
                        for err in &scripts.script_host.compile_errors {
                            warn!("Dynamic character script error ({char_id}): {err}");
                        }
                    }
                }
                Err(err) => warn!("char_spawn({id}, {char_id}) failed: {err}"),
            }
        }
        ScriptCommand::SetCharSingType { target, role } => {
            let parsed = parse_sing_role(role);
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.sing_role = parsed;
            });
        }
        ScriptCommand::SetCharAnimSuffix { target, suffix } => {
            let next_suffix = suffix.clone();
            apply_actor_patch(target, commands, scripts, pending_chars, move |actor| {
                actor.default_sing_suffix = next_suffix.clone();
                actor.bopper.idle_suffix = next_suffix.clone();
            });
        }
        ScriptCommand::AttachCharAtlas {
            target,
            attach_id,
            symbol_keyword,
            atlas_path,
        } => {
            let Some(dir) = resolve_atlas_dir(&scripts.mods, atlas_path) else {
                warn!("char_attach_atlas({target}, {attach_id}): missing atlas '{atlas_path}'");
                return;
            };
            let Ok(atlas) = AnimateAtlas::from_dir(&dir) else {
                warn!("char_attach_atlas({target}, {attach_id}): failed to parse '{}'", dir.display());
                return;
            };
            let (textures, indices) = crate::play::characters::load_animate_textures(
                &dir,
                &atlas,
                &scripts.mods,
                &scripts.asset_server,
                &mut scripts.layouts,
                &mut scripts.images,
                scripts.preload.as_deref_mut(),
            );
            let Some(clip) = sample_default_composite_clip(&atlas, &indices) else {
                warn!("char_attach_atlas({target}, {attach_id}): no frames in '{atlas_path}'");
                return;
            };
            let all_frames = clip.frames;
            let bounds_min = clip.bounds_min;
            let bounds_size = clip.bounds_size;
            let mut matched = false;
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    let needed = insert_attachment_on_actor(
                        &mut actor,
                        attach_id,
                        symbol_keyword,
                        &textures,
                        &[],
                        &all_frames,
                        bounds_min,
                        bounds_size,
                    );
                    ensure_actor_piece_capacity(&mut actor, needed, commands);
                    matched = true;
                }
            }
            if !matched && let Some(&ent) = pending_chars.get(&target.trim().to_ascii_lowercase()) {
                let aid = attach_id.clone();
                let kw = symbol_keyword.clone();
                commands.queue(move |world: &mut World| {
                    let needed = {
                        let Some(mut actor) = world.get_mut::<CharacterActor>(ent) else {
                            return;
                        };
                        insert_attachment_on_actor(
                            &mut actor,
                            &aid,
                            &kw,
                            &textures,
                            &[],
                            &all_frames,
                            bounds_min,
                            bounds_size,
                        )
                    };
                    ensure_actor_piece_capacity_world(world, ent, needed);
                });
            }
        }
        ScriptCommand::SetCharAttachFrame {
            target,
            attach_id,
            frame,
        } => {
            let frame_idx = *frame;
            apply_attachment_patch(target, attach_id, commands, scripts, pending_chars, move |att| {
                att.cur_frame = frame_idx;
            });
        }
        ScriptCommand::SetCharAttachPose {
            target,
            attach_id,
            offset,
            angle_deg,
        } => {
            let off = *offset;
            let ang = *angle_deg;
            apply_attachment_patch(target, attach_id, commands, scripts, pending_chars, move |att| {
                att.offset = off;
                att.angle_deg = ang;
            });
        }
        ScriptCommand::SetCharAttachStyle {
            target,
            attach_id,
            flip_x,
            alpha,
        } => {
            let fx = *flip_x;
            let a_val = alpha.clamp(0.0, 1.0);
            apply_attachment_patch(target, attach_id, commands, scripts, pending_chars, move |att| {
                att.flip_x = fx;
                att.alpha = a_val;
            });
        }
        ScriptCommand::SetCharAttachVis {
            target,
            attach_id,
            visible,
        } => {
            let vis = *visible;
            apply_attachment_patch(target, attach_id, commands, scripts, pending_chars, move |att| {
                att.visible = vis;
            });
        }
        _ => {}
    }
}
