use std::collections::BTreeMap;

use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{
    HookKind, ScriptCommand, ScriptEvent, ScriptHost, ScriptMutableState, ScriptStateSnapshot,
};
use crate::utils::constants::{HEALTH_MAX, HEALTH_MIN, KEY_COUNT, STEPS_PER_BEAT};

use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::types::PlayPhase;

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
            misses: self.tallies.missed,
            note_count: self.player_strumline.notes.len() + self.opponent_strumline.notes.len(),
            scroll_speed: self.player_strumline.scroll_speed,
            playback_rate: self.playback_rate,
            botplay: self.is_bot_play_mode,
            downscroll: self.player_strumline.is_downscroll,
            middlescroll: self.player_strumline.middlescroll,
            strums_swapped: self.strums_swapped,
            cam_x: self.camera.follow_point[0],
            cam_y: self.camera.follow_point[1],
            cam_zoom: self.camera.current_zoom,
            cam_angle_deg: self.camera.angle_deg,
            hud_zoom: self.camera.hud_zoom,
            char_positions: self.script_char_positions.clone(),
            lane_offsets,
            lane_angles,
            lane_scales,
            lane_alphas,
            prop_positions: self.script_prop_positions.clone(),
            prop_scales: self.script_prop_scales.clone(),
            prop_angles: self.script_prop_angles.clone(),
            prop_alphas: self.script_prop_alphas.clone(),
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
        if m.dirty_middlescroll {
            self.player_strumline.middlescroll = m.middlescroll;
            self.opponent_strumline.middlescroll = m.middlescroll;
        }
        if m.dirty_strums_swapped {
            self.strums_swapped = m.strums_swapped;
            self.player_strumline.strums_swapped = m.strums_swapped;
            self.opponent_strumline.strums_swapped = m.strums_swapped;
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
    }


    /// Dispatches queued lifecycle/gameplay hooks, optionally runs `Update` + timers/tweens,
    /// applies state mutations, and returns commands requiring Bevy ECS execution.
    #[allow(clippy::too_many_arguments)]
    pub fn run_script_hooks(
        &mut self,
        hooks: Vec<(HookKind, ScriptEvent)>,
        include_update: bool,
        dt_s: f64,
        playlist: &mut PlayStatePlaylist,
        host: &mut ScriptHost,
        registry: &ModRegistry,
        store: &mut ModStore,
    ) -> Vec<ScriptCommand> {
        let snapshot = self.snapshot_for_scripts(dt_s);
        let mut ctx = host.make_context(snapshot, registry, store);

        for (hook, mut ev) in hooks {
            host.dispatch(&hook, &mut ev, &mut ctx);
            if matches!(hook, HookKind::NoteMiss) && ev.canceled {
                if let Some(hd) = ev.get_f64("health_change") {
                    self.health = (self.health - hd).clamp(HEALTH_MIN, HEALTH_MAX);
                }
                if let Some(sd) = ev.get_f64("score_change") {
                    self.song_score -= sd;
                }
                if let Some(cb) = ev.get_i64("combo_before") {
                    self.tallies.combo = cb.max(0) as u32;
                }
                self.tallies.missed = self.tallies.missed.saturating_sub(1);
                self.pending_miss_sfx = false;
                self.player_vocals_volume = 1.0;
            }
        }

        if include_update && !self.critical_failure {
            let paused = matches!(
                self.phase,
                PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
            );
            let mut update_ev = ScriptEvent::new(HookKind::Update)
                .with_field("dt", dt_s)
                .with_field("paused", paused);
            host.dispatch(&HookKind::Update, &mut update_ev, &mut ctx);
            if !paused {
                host.tick_timers_and_tweens(dt_s, &mut ctx);
            }
        }

        let mut ctx = host.finalize_context(ctx, store);
        self.apply_script_mutable(&ctx.mutable);

        let mut remaining = Vec::new();
        for cmd in ctx.commands.drain(..) {
            if !self.apply_state_script_command(&cmd, playlist) {
                remaining.push(cmd);
            }
        }
        remaining
    }

    /// Advances gameplay state for one frame and executes attached mod scripts.
    pub fn update_with_scripts(
        &mut self,
        elapsed_s: f64,
        audio_time_ms: Option<f64>,
        playlist: &mut PlayStatePlaylist,
        host: &mut ScriptHost,
        registry: &ModRegistry,
        store: &mut ModStore,
    ) -> Vec<ScriptCommand> {
        self.update(elapsed_s, audio_time_ms, playlist);
        let hooks = std::mem::take(&mut self.pending_hooks);
        self.run_script_hooks(hooks, true, elapsed_s, playlist, host, registry, store)
    }
}

