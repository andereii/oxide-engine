use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::play::characters::{
    CharacterTrailConfig, CharacterTrailState, CharacterTrailVisual, CharacterUnderlayConfig,
    load_single_character, spawn_single_character, spawn_single_character_with_underlay,
};
use crate::play::stage::schema::stage_layer_z;
use crate::play::hud::health_icon::apply_icon_char_id;
use crate::play::notes::data::NoteDirection;
use crate::scripting::ScriptCommand;

use super::super::hud_audio::{spawn_one_shot, spawn_tagged_sound};
use super::super::{AudioTickParams, ScriptHudAndTextParams, ScriptTickParams, actor_matches_target};

static SCREENSHOT_SEQ: AtomicU64 = AtomicU64::new(1);

fn side_matches_icon(player_id: u8, side: &str) -> bool {
    match side.trim().to_ascii_lowercase().as_str() {
        "p1" | "bf" | "boyfriend" | "player" | "0" => player_id == 0,
        "p2" | "dad" | "opponent" | "1" => player_id == 1,
        "both" | "all" => true,
        _ => false,
    }
}

fn sanitize_screenshot_path(mods_dir: &std::path::Path, name_opt: Option<&str>) -> PathBuf {
    let dir = mods_dir.join(".store").join("screenshots");
    let _ = fs::create_dir_all(&dir);
    let raw = name_opt.unwrap_or("").trim();
    let stem: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let filename = if stem.is_empty() {
        let seq = SCREENSHOT_SEQ.fetch_add(1, Ordering::Relaxed);
        format!("shot_{seq:04}.png")
    } else if stem.to_ascii_lowercase().ends_with(".png") {
        stem
    } else {
        format!("{stem}.png")
    };
    dir.join(filename)
}

pub(super) fn trigger_sandboxed_screenshot(
    commands: &mut Commands,
    mods_dir: &std::path::Path,
    name_opt: Option<&str>,
) {
    let out_path = sanitize_screenshot_path(mods_dir, name_opt);
    let mut saver = save_to_disk(out_path.clone());
    commands.spawn(Screenshot::primary_window()).observe(
        move |event: On<ScreenshotCaptured>| {
            saver(event);
            info!("Saved screenshot to {}", out_path.display());
        },
    );
}

fn despawn_actor_with_parts(
    commands: &mut Commands,
    ent: Entity,
    sheet_ent: Entity,
    pieces: &[Entity],
    trails: &[Entity],
) {
    if sheet_ent != Entity::PLACEHOLDER {
        commands.entity(sheet_ent).despawn();
    }
    for &p_ent in pieces {
        if p_ent != Entity::PLACEHOLDER {
            commands.entity(p_ent).despawn();
        }
    }
    for &t_ent in trails {
        if t_ent != Entity::PLACEHOLDER {
            commands.entity(t_ent).despawn();
        }
    }
    commands.entity(ent).despawn();
}

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_char_hud_audio_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
    pending_spawned: &std::collections::HashMap<String, Entity>,
) {
    match cmd {
        ScriptCommand::PlayAnim { target, anim, force } => {
            let mut matched_actor = false;
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.controller.play(anim, *force, false, 0);
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
                        despawn_actor_with_parts(
                            commands,
                            ent,
                            actor.sheet_entity,
                            &actor.piece_entities,
                            &actor.trail_entities,
                        );
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
                        despawn_actor_with_parts(
                            commands,
                            ent,
                            actor.sheet_entity,
                            &actor.piece_entities,
                            &actor.trail_entities,
                        );
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
                        actor.sheet_entity,
                        actor.piece_entities.clone(),
                        actor.trail_entities.clone(),
                    ));
                }
            }
            for (ent, role_enum, slot, sheet_ent, pieces, trails) in replacement_specs {
                for (u_ent, u_actor) in scripts.actors.iter() {
                    if let Some(cfg) = u_actor.mirror_of
                        && cfg.parent_role == role_enum
                    {
                        despawn_actor_with_parts(
                            commands,
                            u_ent,
                            u_actor.sheet_entity,
                            &u_actor.piece_entities,
                            &u_actor.trail_entities,
                        );
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
                    despawn_actor_with_parts(commands, ent, sheet_ent, &pieces, &trails);
                    spawn_single_character(commands, &loaded);
                }
            }
        }
        ScriptCommand::SetBarColors { opp, ply } => {
            for (fill, mut sprite) in &mut hud_texts.bar_fills {
                let c = if fill.is_player { ply } else { opp };
                sprite.color = Color::srgba(c[0], c[1], c[2], c[3]);
            }
        }
        ScriptCommand::SetIcon { side, char_id } => {
            for (mut icon, mut anim, mut sprite) in &mut hud_texts.icons {
                if side_matches_icon(icon.player_id, side) {
                    apply_icon_char_id(
                        &mut icon,
                        &mut anim,
                        &mut sprite,
                        char_id,
                        &scripts.mods,
                        &scripts.asset_server,
                        &mut scripts.layouts,
                    );
                }
            }
        }
        ScriptCommand::SetIconScale { side, scale } => {
            let s = (*scale).max(0.05);
            for (mut icon, _, _) in &mut hud_texts.icons {
                if side_matches_icon(icon.player_id, side) {
                    icon.base_scale = [s, s];
                    icon.snap_to_target_size();
                }
            }
        }
        ScriptCommand::SetIconBop { side, enabled, every, angle_deg } => {
            for (mut icon, _, _) in &mut hud_texts.icons {
                if side_matches_icon(icon.player_id, side) {
                    icon.should_bop = *enabled;
                    icon.bop_every = *every;
                    icon.bop_angle_deg = *angle_deg;
                }
            }
        }
        ScriptCommand::PlaySfx { path, volume } => {
            let resolved = scripts
                .mods
                .resolve_asset_server_path(path)
                .unwrap_or_else(|| path.clone());
            let handle: Handle<AudioSource> = scripts.asset_server.load(resolved);
            spawn_one_shot(
                commands,
                handle,
                *volume,
                &audio_res.audio_sources,
                &mut audio_res.pcm_sfx,
                &mut audio_res.sfx_cache,
            );
        }
        ScriptCommand::PlaySound { tag, path, volume, looped, pitch } => {
            for (ent, tagged) in &hud_texts.tagged_sounds {
                if tagged.tag == *tag {
                    commands.entity(ent).despawn();
                }
            }
            let resolved = scripts
                .mods
                .resolve_asset_server_path(path)
                .unwrap_or_else(|| path.clone());
            let handle: Handle<AudioSource> = scripts.asset_server.load(resolved);
            spawn_tagged_sound(
                commands,
                tag,
                handle,
                *volume,
                *looped,
                *pitch,
                &audio_res.audio_sources,
                &mut audio_res.pcm_sfx,
                &mut audio_res.sfx_cache,
            );
        }
        ScriptCommand::StopSound(tag) => {
            for (ent, tagged) in &hud_texts.tagged_sounds {
                if tagged.tag == *tag {
                    commands.entity(ent).despawn();
                }
            }
        }
        ScriptCommand::PauseSound { tag, paused } => {
            for (ent, tagged) in &hud_texts.tagged_sounds {
                if tagged.tag == *tag
                    && let Ok(sink) = audio_res.sinks.get(ent)
                {
                    if *paused {
                        sink.pause();
                    } else {
                        sink.play();
                    }
                }
            }
        }
        ScriptCommand::SetSoundParam { tag, volume, pitch } => {
            for (ent, mut tagged) in &mut hud_texts.tagged_sounds {
                if tagged.tag == *tag
                    && let Ok(mut sink) = audio_res.sinks.get_mut(ent)
                {
                    if let Some(v) = volume {
                        let clamped = v.clamp(0.0, 2.0);
                        tagged.base_volume = clamped;
                        sink.set_volume(Volume::Linear(clamped));
                    }
                    if let Some(p) = pitch {
                        sink.set_speed(p.max(0.05));
                    }
                }
            }
        }
        ScriptCommand::SeekSong(ms) => {
            audio_res.audio.play_from(*ms, &mut audio_res.sinks);
            audio_res.sync.was_countdown = false;
            hud_texts.pending_seek.0 = Some(*ms);
        }
        ScriptCommand::TakeScreenshot(name_opt) => {
            trigger_sandboxed_screenshot(commands, &scripts.mods.mods_dir, name_opt.as_deref());
        }
        ScriptCommand::LoadSong { song_id, difficulty, variation } => {
            hud_texts.pending_reload.request = Some((
                song_id.clone(),
                difficulty.clone(),
                variation.clone(),
            ));
        }
        _ => {}
    }
}
