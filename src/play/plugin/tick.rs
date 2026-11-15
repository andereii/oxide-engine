use bevy::audio::AudioSink;
use bevy::prelude::*;

use crate::audio::SongAudioPlayer;
use crate::scripting::ScriptCommand;

use super::super::characters::CharacterRole;
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::types::PlayPhase;
use super::hud_audio::spawn_one_shot;
use super::script_commands::apply_ecs_script_commands;
use super::{AudioSyncState, PlaySfxAssets, ScriptTickParams};

#[allow(clippy::too_many_arguments)]
pub(super) fn tick_play_state(
    mut commands: Commands,
    time: Res<Time>,
    mut state: ResMut<PlayState>,
    mut playlist: ResMut<PlayStatePlaylist>,
    mut audio: ResMut<SongAudioPlayer>,
    mut sync: ResMut<AudioSyncState>,
    mut sfx: ResMut<PlaySfxAssets>,
    mut sinks: Query<&mut AudioSink>,
    mut scripts: ScriptTickParams,
) {
    let is_paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    if is_paused && !sync.was_paused {
        audio.pause(&mut sinks);
        sync.was_paused = true;
    } else if !is_paused && sync.was_paused {
        audio.resume(&mut sinks);
        sync.was_paused = false;
    }

    if state.needs_reset {
        audio.pause(&mut sinks);
        sync.was_countdown = true;
    }

    let mut seen_aliases = [false; 3];
    for actor in &scripts.actors {
        let tl_x = actor.slot.position[0]
            + actor.data.offsets[0] * (1.0 + actor.slot.scale[0])
            - actor.idle_size[0] * actor.slot.scale[0] * 0.5;
        let tl_y = actor.slot.position[1]
            + actor.data.offsets[1] * (1.0 + actor.slot.scale[1])
            - actor.idle_size[1] * actor.slot.scale[1];
        let char_z = super::super::stage::schema::stage_layer_z(actor.slot.layer, 0);
        state
            .script_char_positions
            .insert(actor.id.clone(), actor.slot.position);
        state
            .script_char_top_left
            .insert(actor.id.clone(), [tl_x, tl_y]);
        state.script_char_z.insert(actor.id.clone(), char_z);
        let cur_anim = actor.controller.current.clone().unwrap_or_default();
        let anim_finished = actor.controller.is_finished();
        state
            .script_char_anims
            .insert(actor.id.clone(), cur_anim.clone());
        state
            .script_char_anim_finished
            .insert(actor.id.clone(), anim_finished);
        let (alias, role_idx) = match actor.role {
            CharacterRole::Player => ("bf", 0),
            CharacterRole::Opponent => ("dad", 1),
            CharacterRole::Spectator => ("gf", 2),
        };
        state
            .script_char_positions
            .entry(alias.to_string())
            .or_insert(actor.slot.position);
        state
            .script_char_top_left
            .entry(alias.to_string())
            .or_insert([tl_x, tl_y]);
        state
            .script_char_z
            .entry(alias.to_string())
            .or_insert(char_z);
        if !seen_aliases[role_idx] {
            seen_aliases[role_idx] = true;
            state.script_char_anims.insert(alias.to_string(), cur_anim);
            state
                .script_char_anim_finished
                .insert(alias.to_string(), anim_finished);
        }
    }
    for (_, vis, sprite, _, _, _) in &scripts.props {
        state
            .script_prop_positions
            .insert(vis.name.clone(), vis.base_position);
        state
            .script_prop_scales
            .insert(vis.name.clone(), vis.base_scale);
        state
            .script_prop_angles
            .insert(vis.name.clone(), vis.base_angle_deg);
        state
            .script_prop_alphas
            .insert(vis.name.clone(), sprite.color.alpha());
    }

    let dt = f64::from(time.delta_secs()).min(0.1);
    let master_ms = audio.master_time_ms(&sinks);
    state.audio_finished = state.phase == PlayPhase::Playing
        && !state.is_in_countdown
        && audio.all_tracks_finished(&sinks);
    let mut ecs_cmds: Vec<ScriptCommand> = scripts.pending_ecs.0.drain(..).collect();
    let frame_cmds = state.update_with_scripts(
        dt,
        master_ms,
        &mut playlist,
        &mut scripts.script_host,
        &scripts.mods,
        &mut scripts.mod_store,
    );
    ecs_cmds.extend(frame_cmds);
    apply_ecs_script_commands(
        &mut commands,
        &scripts.asset_server,
        &mut scripts.layouts,
        &scripts.mods,
        &mut scripts.actors,
        &mut scripts.props,
        &mut scripts.pending_shaders,
        &ecs_cmds,
    );

    if state.pending_miss_sfx {
        state.pending_miss_sfx = false;
        if !sfx.miss_sounds.is_empty() {
            let idx = sfx.miss_cursor % sfx.miss_sounds.len();
            sfx.miss_cursor = sfx.miss_cursor.wrapping_add(1);
            spawn_one_shot(&mut commands, sfx.miss_sounds[idx].clone(), sfx.miss_volume);
        }
    }

    if sync.was_countdown && !state.is_in_countdown && state.phase == PlayPhase::Playing {
        audio.play_from(state.start_timestamp_ms, &mut sinks);
        sync.was_countdown = false;
    }

    if state.phase == PlayPhase::Playing {
        audio.sync_volumes(
            state.instrumental_volume as f32,
            state.player_vocals_volume as f32,
            state.opponent_vocals_volume as f32,
            &mut sinks,
        );
        let sync_ms = (state.conductor.song_position_ms - state.conductor.combined_offset()).max(0.0);
        audio.tick(sync_ms, dt, &mut sinks);
    }
}

