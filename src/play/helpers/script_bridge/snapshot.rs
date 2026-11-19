use std::collections::BTreeMap;

use crate::play::notes::strum::Strumline;
use crate::scripting::{ScriptMutableState, ScriptNoteSnapshot, ScriptStateSnapshot};
use crate::utils::constants::{HEALTH_MAX, HEALTH_MIN, KEY_COUNT, STEPS_PER_BEAT};

use super::super::super::scene::PlayState;
use super::super::super::types::PlayPhase;

fn push_active_strum_notes(out: &mut Vec<ScriptNoteSnapshot>, strum: &Strumline, is_player: bool) {
    for note in strum.active_slice() {
        if note.killed && !note.is_hold_alive() {
            continue;
        }
        out.push(ScriptNoteSnapshot {
            id: note.data.id,
            time_ms: note.data.time_ms,
            dir: note.lane.index(),
            length_ms: note.data.sustain_length_ms,
            kind: note.data.kind.clone().unwrap_or_default(),
            is_player,
            hit: note.has_been_hit,
            missed: note.has_missed,
            holding: note.hold_hit && !note.hold_dropped && !note.hold_killed,
            rem_ms: note.hold_remaining_ms,
        });
    }
}

impl PlayState {
    /// Captures a read-only gameplay snapshot consumed by script APIs.
    pub fn snapshot_for_scripts(&self, dt_s: f64) -> ScriptStateSnapshot {
        let mut lane_offsets = BTreeMap::new();
        let mut lane_angles = BTreeMap::new();
        let mut lane_scales = BTreeMap::new();
        let mut lane_alphas = BTreeMap::new();
        for idx in 0..KEY_COUNT {
            let p_rec = &self.player_strumline.receptors[idx];
            lane_offsets.insert(idx, p_rec.mod_offset);
            lane_angles.insert(idx, p_rec.mod_angle_deg);
            lane_scales.insert(idx, p_rec.mod_scale);
            lane_alphas.insert(idx, p_rec.mod_alpha);

            let o_idx = idx + KEY_COUNT;
            let o_rec = &self.opponent_strumline.receptors[idx];
            lane_offsets.insert(o_idx, o_rec.mod_offset);
            lane_angles.insert(o_idx, o_rec.mod_angle_deg);
            lane_scales.insert(o_idx, o_rec.mod_scale);
            lane_alphas.insert(o_idx, o_rec.mod_alpha);
        }

        let mut active_notes = Vec::with_capacity(32);
        push_active_strum_notes(&mut active_notes, &self.player_strumline, true);
        push_active_strum_notes(&mut active_notes, &self.opponent_strumline, false);

        let paused = matches!(
            self.phase,
            PlayPhase::Loading | PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
        );
        let beat_ms = self.conductor.beat_length_ms();
        let step_ms = self.conductor.step_length_ms();
        ScriptStateSnapshot {
            song_id: self.chart.song_id.clone(),
            difficulty: self.current_difficulty.clone(),
            variation: self.current_variation.clone(),
            stage_id: self.chart.stage_id.clone(),
            bf_id: self.chart.characters.players.first().cloned().unwrap_or_else(|| "bf".to_string()),
            dad_id: self.chart.characters.opponents.first().cloned().unwrap_or_else(|| "dad".to_string()),
            gf_id: self.chart.characters.spectator.first().cloned().unwrap_or_else(|| "gf".to_string()),
            song_position_ms: self.conductor.song_position_ms,
            song_length_ms: self.chart.length_ms,
            bpm: self.conductor.bpm(),
            step: self.conductor.current_step,
            beat: self.conductor.current_beat,
            measure: self.conductor.current_beat.div_euclid(i32::from(STEPS_PER_BEAT)),
            step_length_ms: step_ms,
            beat_length_ms: beat_ms,
            dt_s,
            health: self.health,
            score: self.song_score,
            combo: self.tallies.combo,
            max_combo: self.tallies.max_combo,
            misses: self.tallies.missed,
            sick: self.tallies.sick,
            good: self.tallies.good,
            bad: self.tallies.bad,
            shit: self.tallies.shit,
            hit_notes: self.tallies.total_notes_hit,
            total_notes: self.tallies.total_notes,
            note_count: self.player_strumline.notes.len() + self.opponent_strumline.notes.len(),
            scroll_speed: self.player_strumline.scroll_speed,
            playback_rate: self.playback_rate,
            botplay: self.is_bot_play_mode,
            downscroll: self.player_strumline.is_downscroll,
            middlescroll: self.player_strumline.middlescroll,
            strums_swapped: self.strums_swapped,
            practice_mode: self.is_practice_mode,
            ghost_tap: self.player_strumline.ghost_tap_enabled,
            in_countdown: self.is_in_countdown,
            in_cutscene: self.is_in_cutscene,
            paused,
            can_pause: self.may_pause_game,
            keys_disabled: self.disable_keys,
            deaths: self.death_counter,
            inst_vol: self.instrumental_volume,
            master_volume: self.master_volume,
            p_vocal_vol: self.player_vocals_volume,
            o_vocal_vol: self.opponent_vocals_volume,
            cam_x: self.camera.follow_point[0],
            cam_y: self.camera.follow_point[1],
            cam_zoom: self.camera.current_zoom,
            stage_zoom: self.camera.stage_zoom,
            cam_angle_deg: self.camera.angle_deg,
            hud_zoom: self.camera.hud_zoom,
            hud_angle_deg: self.camera.hud_angle_deg,
            story_mode: false,
            playlist_songs: Vec::new(),
            campaign_score: 0,
            char_positions: self.script_char_positions.clone(),
            char_top_left: self.script_char_top_left.clone(),
            char_z: self.script_char_z.clone(),
            char_scales: self.script_char_scales.clone(),
            char_alphas: self.script_char_alphas.clone(),
            char_flips: self.script_char_flips.clone(),
            char_vis: self.script_char_vis.clone(),
            char_sizes: self.script_char_sizes.clone(),
            char_focus: self.script_char_focus.clone(),
            char_anims: self.script_char_anims.clone(),
            char_anim_finished: self.script_char_anim_finished.clone(),
            lane_offsets,
            lane_angles,
            lane_scales,
            lane_alphas,
            prop_positions: self.script_prop_positions.clone(),
            prop_scales: self.script_prop_scales.clone(),
            prop_angles: self.script_prop_angles.clone(),
            prop_alphas: self.script_prop_alphas.clone(),
            prop_z: self.script_prop_z.clone(),
            prop_scrolls: self.script_prop_scrolls.clone(),
            prop_flips: self.script_prop_flips.clone(),
            prop_vis: self.script_prop_vis.clone(),
            prop_sizes: self.script_prop_sizes.clone(),
            prop_velocities: self.script_prop_velocities.clone(),
            prop_anims: self.script_prop_anims.clone(),
            prop_anim_done: self.script_prop_anim_done.clone(),
            texts: self.script_texts.clone(),
            text_positions: self.script_text_positions.clone(),
            text_alphas: self.script_text_alphas.clone(),
            active_sounds: self.script_active_sounds.clone(),
            sound_volumes: self.script_sound_volumes.clone(),
            active_notes,
            keys_down: self.script_keys_down.clone(),
            keys_hit: self.script_keys_hit.clone(),
            keys_up: self.script_keys_up.clone(),
            ctrls_down: self.script_ctrls_down.clone(),
            ctrls_hit: self.script_ctrls_hit.clone(),
            ctrls_up: self.script_ctrls_up.clone(),
        }
    }

    /// Applies direct mutable property writes made by scripts during a dispatch cycle.
    pub fn apply_script_mutable(&mut self, m: &ScriptMutableState) {
        if m.dirty_health {
            self.health = m.health.clamp(HEALTH_MIN, HEALTH_MAX);
        }
        if m.dirty_score {
            self.song_score = m.score;
        }
        if m.dirty_scroll_speed {
            let speed = m.scroll_speed.max(0.05);
            self.player_strumline.scroll_speed = speed;
            self.opponent_strumline.scroll_speed = speed;
        }
        if m.dirty_playback_rate {
            self.playback_rate = m.playback_rate.clamp(0.1, 4.0);
        }
        if m.dirty_botplay {
            self.is_bot_play_mode = m.botplay;
            self.player_strumline.is_player = !m.botplay;
        }
        if m.dirty_downscroll {
            self.player_strumline.is_downscroll = m.downscroll;
            self.opponent_strumline.is_downscroll = m.downscroll;
        }
        if m.dirty_middlescroll {
            self.player_strumline.middlescroll = m.middlescroll;
            self.opponent_strumline.middlescroll = m.middlescroll;
        }
        if m.dirty_strums_swapped {
            self.strums_swapped = m.strums_swapped;
            self.player_strumline.strums_swapped = m.strums_swapped;
            self.opponent_strumline.strums_swapped = m.strums_swapped;
        }
        if m.dirty_practice {
            self.is_practice_mode = m.practice_mode;
        }
        if m.dirty_ghost_tap {
            self.player_strumline.ghost_tap_enabled = m.ghost_tap;
        }
        if m.dirty_can_pause {
            self.may_pause_game = m.can_pause;
        }
        if m.dirty_cutscene {
            self.is_in_cutscene = m.in_cutscene;
        }
        if m.dirty_keys_disabled {
            self.disable_keys = m.keys_disabled;
        }
        if m.dirty_cam_pos {
            self.cancel_camera_follow_tween();
            self.camera.follow_target = None;
            self.camera.follow_point = [m.cam_x, m.cam_y];
        }
        if m.dirty_cam_zoom {
            self.cancel_camera_zoom_tween();
            self.camera.current_zoom = m.cam_zoom.max(0.05);
        }
        if m.dirty_cam_angle {
            self.camera.angle_deg = m.cam_angle_deg;
        }
        if m.dirty_hud_zoom {
            self.camera.hud_zoom = m.hud_zoom.max(0.05);
        }
        if m.dirty_hud_angle {
            self.camera.hud_angle_deg = m.hud_angle_deg;
        }
    }

}
