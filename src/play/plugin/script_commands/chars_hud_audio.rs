use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use bevy::audio::Volume;
use bevy::prelude::*;
use bevy::render::view::window::screenshot::{Screenshot, ScreenshotCaptured, save_to_disk};

use crate::play::characters::{load_single_character, spawn_single_character};
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

#[allow(clippy::too_many_arguments)]
pub(super) fn handle_char_hud_audio_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    audio_res: &mut AudioTickParams,
    scripts: &mut ScriptTickParams,
    hud_texts: &mut ScriptHudAndTextParams,
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
                for (_, vis, _, anim_opt, _, _) in scripts.props.iter_mut() {
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
            for (_, mut actor) in scripts.actors.iter_mut() {
                if actor_matches_target(&actor, target) {
                    actor.controller.play(anim, *force, false, *frame);
                    actor.hold_timer = 0.0;
                    actor.script_anim_override = true;
                    matched_actor = true;
                }
            }
            if !matched_actor {
                for (_, vis, _, anim_opt, _, _) in scripts.props.iter_mut() {
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
                    ));
                }
            }
            for (ent, role_enum, slot, sheet_ent, pieces) in replacement_specs {
                if let Ok(loaded) = load_single_character(
                    &scripts.mods,
                    char_id,
                    role_enum,
                    slot,
                    &scripts.asset_server,
                    &mut scripts.layouts,
                ) {
                    commands.entity(sheet_ent).despawn();
                    for p_ent in pieces {
                        commands.entity(p_ent).despawn();
                    }
                    commands.entity(ent).despawn();
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
