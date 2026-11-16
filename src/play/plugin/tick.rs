use std::collections::BTreeMap;

use bevy::app::AppExit;
use bevy::audio::AudioSinkPlayback;
use bevy::prelude::*;
use bevy::window::WindowFocused;

use crate::scripting::{HookKind, ScriptCommand, ScriptEvent};
use crate::ui::LoadingScreenState;

use super::super::characters::{CharacterRole, ensure_character_gameover_anims_loaded};
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::song::SongSelection;
use super::super::types::PlayPhase;
use super::hud_audio::spawn_one_shot;
use super::load_pipeline::{PlayLoadPipeline, step_play_load_pipeline};
use super::reload::{ReloadCleanupQuery, apply_song_seek, execute_song_reload};
use super::script_commands::apply_ecs_script_commands;
use super::{AudioTickParams, ScriptHudAndTextParams, ScriptTickParams};

fn set_map_copy<V: Copy + PartialEq>(map: &mut BTreeMap<String, V>, key: &str, val: V) {
    if let Some(slot) = map.get_mut(key) {
        if *slot != val {
            *slot = val;
        }
    } else {
        map.insert(key.to_string(), val);
    }
}

fn set_map_str(map: &mut BTreeMap<String, String>, key: &str, val: &str) {
    if let Some(slot) = map.get_mut(key) {
        if slot != val {
            slot.clear();
            slot.push_str(val);
        }
    } else {
        map.insert(key.to_string(), val.to_string());
    }
}

fn sync_script_entity_maps(
    state: &mut PlayState,
    scripts: &ScriptTickParams,
    hud_texts: &ScriptHudAndTextParams,
    sinks: &Query<&mut AudioSink>,
) {
    let mut seen_aliases = [false; 3];
    for (_, actor) in &scripts.actors {
        if actor.mirror_of.is_some() {
            continue;
        }
        let tl_x = actor.slot.position[0]
            + actor.data.offsets[0] * (1.0 + actor.slot.scale[0])
            - actor.idle_size[0] * actor.slot.scale[0] * 0.5;
        let tl_y = actor.slot.position[1]
            + actor.data.offsets[1] * (1.0 + actor.slot.scale[1])
            - actor.idle_size[1] * actor.slot.scale[1];
        let char_z = actor
            .mod_z
            .unwrap_or_else(|| super::super::stage::schema::stage_layer_z(actor.slot.layer, 0));
        let cur_anim = actor.controller.current.as_deref().unwrap_or("");
        let anim_finished = actor.controller.is_finished();

        let (alias, role_idx) = match actor.role {
            CharacterRole::Player => ("bf", 0),
            CharacterRole::Opponent => ("dad", 1),
            CharacterRole::Spectator => ("gf", 2),
        };
        let write_alias = !seen_aliases[role_idx];
        seen_aliases[role_idx] = true;

        let keys: [&str; 2] = if write_alias {
            [&actor.id, alias]
        } else {
            [&actor.id, ""]
        };
        for key in keys {
            if key.is_empty() {
                continue;
            }
            set_map_copy(&mut state.script_char_positions, key, actor.slot.position);
            set_map_copy(&mut state.script_char_top_left, key, [tl_x, tl_y]);
            set_map_copy(&mut state.script_char_z, key, char_z);
            set_map_copy(&mut state.script_char_scales, key, actor.slot.scale);
            set_map_copy(&mut state.script_char_alphas, key, actor.mod_tint[3]);
            set_map_copy(&mut state.script_char_flips, key, actor.slot.flip);
            set_map_copy(&mut state.script_char_vis, key, actor.mod_visible);
            set_map_copy(&mut state.script_char_sizes, key, actor.idle_size);
            set_map_copy(&mut state.script_char_focus, key, actor.camera_focus_point);
            set_map_str(&mut state.script_char_anims, key, cur_anim);
            set_map_copy(&mut state.script_char_anim_finished, key, anim_finished);
        }
    }
    for (_, vis, sprite, anim_opt, _, _) in &scripts.props {
        set_map_copy(&mut state.script_prop_positions, &vis.name, vis.base_position);
        set_map_copy(&mut state.script_prop_scales, &vis.name, vis.base_scale);
        set_map_copy(&mut state.script_prop_angles, &vis.name, vis.base_angle_deg);
        set_map_copy(&mut state.script_prop_alphas, &vis.name, sprite.color.alpha());
        set_map_copy(&mut state.script_prop_z, &vis.name, vis.z_layer);
        set_map_copy(&mut state.script_prop_scrolls, &vis.name, vis.scroll_factor);
        set_map_copy(&mut state.script_prop_flips, &vis.name, vis.base_flip);
        set_map_copy(&mut state.script_prop_vis, &vis.name, vis.visible);
        set_map_copy(&mut state.script_prop_sizes, &vis.name, vis.raw_size);
        set_map_copy(&mut state.script_prop_velocities, &vis.name, vis.velocity);
        if let Some(ctrl) = anim_opt {
            set_map_str(
                &mut state.script_prop_anims,
                &vis.name,
                ctrl.current.as_deref().unwrap_or(""),
            );
            set_map_copy(&mut state.script_prop_anim_done, &vis.name, ctrl.is_finished());
        }
    }

    state.script_texts.clear();
    state.script_text_positions.clear();
    state.script_text_alphas.clear();
    for (_, txt, t2d, _, tc, _) in &hud_texts.texts {
        state.script_texts.insert(txt.id.clone(), t2d.0.clone());
        state
            .script_text_positions
            .insert(txt.id.clone(), txt.base_position);
        state
            .script_text_alphas
            .insert(txt.id.clone(), tc.0.alpha());
    }

    state.script_active_sounds.clear();
    state.script_sound_volumes.clear();
    for (ent, tagged) in &hud_texts.tagged_sounds {
        state
            .script_sound_volumes
            .insert(tagged.tag.clone(), tagged.base_volume);
        let active = sinks
            .get(ent)
            .ok()
            .is_some_and(|sink| !sink.is_paused() && !sink.empty());
        if active {
            state.script_active_sounds.push(tagged.tag.clone());
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn tick_play_state(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut selection: ResMut<SongSelection>,
    mut audio_res: AudioTickParams,
    mut scripts: ScriptTickParams,
    mut hud_texts: ScriptHudAndTextParams,
    mut focus_events: MessageReader<WindowFocused>,
    mut exit_events: MessageReader<AppExit>,
    reload_cleanup: ReloadCleanupQuery,
    mut loading_screen: ResMut<LoadingScreenState>,
    mut load_pipeline: ResMut<PlayLoadPipeline>,
) {
    for ev in focus_events.read() {
        let kind = if ev.focused {
            HookKind::FocusGained
        } else {
            HookKind::FocusLost
        };
        state.pending_hooks.push((kind.clone(), ScriptEvent::new(kind)));
    }
    if exit_events.read().next().is_some() {
        let exit_cmds = state.run_script_hooks(
            vec![(HookKind::Destroy, ScriptEvent::new(HookKind::Destroy))],
            false,
            0.0,
            &mut playlist,
            &mut scripts.script_host,
            &scripts.mods,
            &mut scripts.mod_store,
        );
        scripts.pending_ecs.0.extend(exit_cmds);
    }

    audio_res.audio.prepare_mix_if_ready(
        &mut commands,
        &audio_res.audio_sources,
        &mut audio_res.synced_songs,
    );

    if let Some((song_id, diff_opt, var_opt)) = hud_texts.pending_reload.request.take() {
        execute_song_reload(
            &mut commands,
            song_id,
            diff_opt,
            var_opt,
            &mut state,
            &mut playlist,
            &mut selection,
            &mut audio_res,
            &mut scripts,
            &mut hud_texts,
            &reload_cleanup,
            &mut loading_screen,
            &mut load_pipeline,
        );
        return;
    }

    if state.phase == PlayPhase::Loading {
        step_play_load_pipeline(
            time.delta_secs(),
            &mut commands,
            &mut state,
            &mut playlist,
            &mut audio_res,
            &mut scripts,
            &mut hud_texts,
            &mut loading_screen,
            &mut load_pipeline,
        );
        return;
    }

    let is_paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    if is_paused && !audio_res.sync.was_paused {
        audio_res.audio.pause(&mut audio_res.sinks);
        for (ent, _) in &hud_texts.tagged_sounds {
            if let Ok(sink) = audio_res.sinks.get_mut(ent) {
                sink.pause();
            }
        }
        audio_res.sync.was_paused = true;
    } else if !is_paused && audio_res.sync.was_paused {
        audio_res.audio.resume(&mut audio_res.sinks);
        for (ent, _) in &hud_texts.tagged_sounds {
            if let Ok(sink) = audio_res.sinks.get_mut(ent) {
                sink.play();
            }
        }
        audio_res.sync.was_paused = false;
    }

    if state.needs_reset {
        audio_res.audio.pause(&mut audio_res.sinks);
        audio_res.sync.was_countdown = true;
    }

    if let Some(seek_ms) = hud_texts.pending_seek.0.take() {
        apply_song_seek(&mut state, &mut audio_res, seek_ms);
    }

    let dt = f64::from(time.delta_secs()).min(0.1);
    let master_ms = audio_res.audio.master_time_ms(&audio_res.sinks);
    state.audio_finished = state.phase == PlayPhase::Playing
        && !state.is_in_countdown
        && audio_res.audio.all_tracks_finished(&audio_res.sinks);

    state.update(dt, master_ms, &mut playlist);

    if state.phase == PlayPhase::GameOver {
        for (_, mut actor) in &mut scripts.actors {
            ensure_character_gameover_anims_loaded(
                &mut actor,
                &scripts.mods,
                &scripts.asset_server,
                &mut scripts.layouts,
                &mut commands,
            );
        }
    }

    let hooks = std::mem::take(&mut state.pending_hooks);
    let mut ecs_cmds: Vec<ScriptCommand> = scripts.pending_ecs.0.drain(..).collect();
    let run_update = !is_paused && scripts.script_host.has_frame_update_work();
    let step_dt = if is_paused { 0.0 } else { dt };
    if !hooks.is_empty() || run_update {
        sync_script_entity_maps(&mut state, &scripts, &hud_texts, &audio_res.sinks);
        let frame_cmds = state.run_script_hooks(
            hooks,
            run_update,
            step_dt,
            &mut playlist,
            &mut scripts.script_host,
            &scripts.mods,
            &mut scripts.mod_store,
        );
        ecs_cmds.extend(frame_cmds);
    }

    if !ecs_cmds.is_empty() {
        apply_ecs_script_commands(
            &mut commands,
            &mut audio_res,
            &mut scripts,
            &mut hud_texts,
            &ecs_cmds,
        );
    }

    if let Some((song_id, diff_opt, var_opt)) = hud_texts.pending_reload.request.take() {
        execute_song_reload(
            &mut commands,
            song_id,
            diff_opt,
            var_opt,
            &mut state,
            &mut playlist,
            &mut selection,
            &mut audio_res,
            &mut scripts,
            &mut hud_texts,
            &reload_cleanup,
            &mut loading_screen,
            &mut load_pipeline,
        );
        return;
    }

    if let Some(seek_ms) = hud_texts.pending_seek.0.take() {
        apply_song_seek(&mut state, &mut audio_res, seek_ms);
    }

    if state.pending_miss_sfx {
        state.pending_miss_sfx = false;
        if !audio_res.sfx.miss_sounds.is_empty() {
            let idx = audio_res.sfx.miss_cursor % audio_res.sfx.miss_sounds.len();
            audio_res.sfx.miss_cursor = audio_res.sfx.miss_cursor.wrapping_add(1);
            spawn_one_shot(
                &mut commands,
                audio_res.sfx.miss_sounds[idx].clone(),
                audio_res.sfx.miss_volume,
                &audio_res.audio_sources,
                &mut audio_res.pcm_sfx,
                &mut audio_res.sfx_cache,
            );
        }
    }

    if audio_res.sync.was_countdown && !state.is_in_countdown && state.phase == PlayPhase::Playing {
        audio_res
            .audio
            .play_from(state.start_timestamp_ms, &mut audio_res.sinks);
        audio_res.sync.was_countdown = false;
    }

    if state.phase == PlayPhase::Playing {
        audio_res.audio.sync_volumes(
            state.instrumental_volume as f32,
            state.player_vocals_volume as f32,
            state.opponent_vocals_volume as f32,
            &mut audio_res.sinks,
        );
        let sync_ms = (state.conductor.song_position_ms - state.conductor.combined_offset()).max(0.0);
        audio_res.audio.tick(sync_ms, dt, &mut audio_res.sinks);
    }
}
