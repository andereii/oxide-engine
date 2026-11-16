use std::collections::BTreeMap;

use crate::mods::{ModRegistry, ModStore};
use crate::play::notes::data::{NoteDirection, SongNoteData};
use crate::play::notes::strum::{CharNoteSignal, Strumline};
use crate::scripting::{
    HookKind, ScriptCommand, ScriptEvent, ScriptHost, ScriptMutableState, ScriptNoteSnapshot,
    ScriptStateSnapshot, ScriptValue,
};
use crate::utils::constants::{HEALTH_MAX, HEALTH_MIN, KEY_COUNT, STEPS_PER_BEAT};

use super::super::camera::events::parse_raw_song_event;
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::types::{PauseMode, PlayPhase, SongEventData};

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

fn decrement_tally(state: &mut PlayState, judge_str: Option<&str>) {
    match judge_str.unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "sick" => state.tallies.sick = state.tallies.sick.saturating_sub(1),
        "good" => state.tallies.good = state.tallies.good.saturating_sub(1),
        "bad" => state.tallies.bad = state.tallies.bad.saturating_sub(1),
        "shit" => state.tallies.shit = state.tallies.shit.saturating_sub(1),
        "miss" => state.tallies.missed = state.tallies.missed.saturating_sub(1),
        _ => {}
    }
}

fn pop_matching_char_event(strum: &mut Strumline, pred: impl Fn(&CharNoteSignal) -> bool) {
    if let Some(pos) = strum.pending_char_events.iter().rposition(pred) {
        strum.pending_char_events.remove(pos);
    }
}

fn apply_song_loaded_mutations(state: &mut PlayState, orig: &ScriptEvent, ev: &ScriptEvent) {
    if ev.get("notes") != orig.get("notes")
        && let Some(ScriptValue::List(items)) = ev.get("notes")
    {
            let mut new_player = Vec::new();
            let mut new_opp = Vec::new();
            for (fallback_idx, item) in items.iter().enumerate() {
                let ScriptValue::Map(map) = item else { continue };
                let id = map
                    .get("id")
                    .and_then(ScriptValue::as_i64)
                    .map_or(fallback_idx + 1, |v| v.max(0) as usize);
                let time_ms = map.get("time").and_then(ScriptValue::as_f64).unwrap_or(0.0);
                let raw_dir = map
                    .get("dir")
                    .or_else(|| map.get("lane"))
                    .and_then(ScriptValue::as_i64)
                    .unwrap_or(0)
                    .max(0) as usize;
                let is_player = map
                    .get("is_player")
                    .and_then(ScriptValue::as_bool)
                    .unwrap_or(raw_dir < KEY_COUNT);
                let length_ms = map
                    .get("len")
                    .or_else(|| map.get("length"))
                    .and_then(ScriptValue::as_f64)
                    .unwrap_or(0.0)
                    .max(0.0);
                let kind = map
                    .get("kind")
                    .and_then(ScriptValue::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(ToString::to_string);
                let scoreable = map
                    .get("scoreable")
                    .and_then(ScriptValue::as_bool)
                    .unwrap_or_else(|| is_player && state.note_kinds.is_scoreable(kind.as_deref()));
                let low_priority = state.note_kinds.get(kind.as_deref()).is_some_and(|k| k.low_priority);
                let data = SongNoteData {
                    id,
                    time_ms,
                    direction: raw_dir % KEY_COUNT,
                    sustain_length_ms: length_ms,
                    kind,
                    low_priority,
                    scoreable,
                };
                if is_player {
                    new_player.push(data);
                } else {
                    new_opp.push(data);
                }
            }
            state.chart.player_notes = new_player;
            state.chart.opponent_notes = new_opp;
            state.regen_note_data(state.start_timestamp_ms);
    }

    if ev.get("events") != orig.get("events")
        && let Some(ScriptValue::List(items)) = ev.get("events")
    {
        let mut parsed: Vec<SongEventData> = Vec::with_capacity(items.len());
        for item in items {
            let ScriptValue::Map(map) = item else { continue };
            let time_ms = map.get("time").and_then(ScriptValue::as_f64).unwrap_or(0.0);
            let kind = map
                .get("kind")
                .or_else(|| map.get("name"))
                .and_then(ScriptValue::as_str)
                .unwrap_or("")
                .to_string();
            let raw_val = map.get("value").map(ScriptValue::to_json).unwrap_or(serde_json::Value::Null);
            parsed.push(parse_raw_song_event(time_ms, kind, raw_val));
        }
        parsed.sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
        state.chart.events = parsed.clone();
        state.song_events = parsed;
        state.rebuild_scroll_velocities();
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
        let paused = matches!(
            self.phase,
            PlayPhase::Loading | PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
        );
        let run_update = include_update && !self.critical_failure && host.has_frame_update_work();
        if hooks.is_empty() && !run_update {
            self.pending_song_event_reverts.clear();
            return Vec::new();
        }

        let mut snapshot = self.snapshot_for_scripts(dt_s);
        snapshot.story_mode = playlist.is_story_mode;
        snapshot.playlist_songs = playlist.playlist_song_ids.clone();
        snapshot.campaign_score = playlist.campaign_score;
        let mut ctx = host.make_context(snapshot, registry, store);

        for (hook, ev) in hooks {
            let orig_ev = ev.clone();
            let mut dispatched_ev = ev;
            host.dispatch(&hook, &mut dispatched_ev, &mut ctx);
            self.apply_hook_outcome(&hook, &orig_ev, &dispatched_ev, playlist);
        }
        self.pending_song_event_reverts.clear();

        if run_update {
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

    fn apply_hook_outcome(
        &mut self,
        hook: &HookKind,
        orig: &ScriptEvent,
        ev: &ScriptEvent,
        playlist: &mut PlayStatePlaylist,
    ) {
        match hook {
            HookKind::SongLoaded => {
                apply_song_loaded_mutations(self, orig, ev);
            }
            HookKind::CountdownStart | HookKind::CountdownStep => {
                if ev.canceled {
                    self.countdown_paused = true;
                    self.pending_countdown_step = None;
                }
            }
            HookKind::Pause => {
                if ev.canceled {
                    self.phase = if self.is_in_countdown {
                        PlayPhase::Countdown
                    } else {
                        PlayPhase::Playing
                    };
                    self.status_message =
                        format!("Playing: {} [{}]", self.chart.song_name, self.current_difficulty);
                }
            }
            HookKind::Resume => {
                if ev.canceled {
                    self.phase = PlayPhase::Paused(PauseMode::Standard);
                    self.status_message = "Paused".to_string();
                }
            }
            HookKind::GameOver => {
                if ev.canceled {
                    self.is_player_dying = false;
                    self.death_counter = self.death_counter.saturating_sub(1);
                    self.phase = PlayPhase::Playing;
                    self.health = self.health.max(0.05);
                    self.status_message =
                        format!("Playing: {} [{}]", self.chart.song_name, self.current_difficulty);
                }
            }
            HookKind::SongEnd => {
                if ev.canceled {
                    self.phase = PlayPhase::Playing;
                    if let Some((removed, delta)) = self.last_song_end_popped.take() {
                        playlist.campaign_score -= delta;
                        if !removed.is_empty() {
                            playlist.playlist_song_ids.insert(0, removed);
                        }
                    }
                } else {
                    self.last_song_end_popped = None;
                }
            }
            HookKind::SongRetry => {
                if ev.canceled {
                    self.needs_reset = false;
                }
            }
            HookKind::SongEvent => {
                let seq = orig.get_i64("event_idx").map(|v| v as usize);
                let orig_kind = orig.get_str("name").unwrap_or("");
                let orig_time = orig.get_f64("time").unwrap_or(0.0);
                if ev.canceled {
                    if let Some(id) = seq
                        && let Some(snap) = self.pending_song_event_reverts.remove(&id)
                    {
                        self.restore_song_event_revert(snap);
                    }
                    if let Some(pos) = self
                        .pending_shader_events
                        .iter()
                        .rposition(|e| e.kind == orig_kind && (e.time_ms - orig_time).abs() < 1e-3)
                    {
                        self.pending_shader_events.remove(pos);
                    }
                } else if ev.get("value") != orig.get("value")
                    || ev.get_str("name") != orig.get_str("name")
                {
                    if let Some(id) = seq
                        && let Some(snap) = self.pending_song_event_reverts.remove(&id)
                    {
                        self.restore_song_event_revert(snap);
                    }
                    let new_kind = ev.get_str("name").unwrap_or(orig_kind).to_string();
                    let new_json = ev
                        .get("value")
                        .map(ScriptValue::to_json)
                        .unwrap_or(serde_json::Value::Null);
                    let updated = parse_raw_song_event(orig_time, new_kind, new_json);
                    self.apply_due_song_event(&updated);
                    if let Some(slot) = self
                        .pending_shader_events
                        .iter_mut()
                        .rfind(|e| e.kind == orig_kind && (e.time_ms - orig_time).abs() < 1e-3)
                    {
                        *slot = updated;
                    }
                }
            }
            HookKind::OpponentNoteHit => {
                let suppressed = ev.canceled || ev.get_bool("play_anim") == Some(false);
                if suppressed {
                    let dir_i = orig.get_i64("dir").unwrap_or(-1);
                    pop_matching_char_event(&mut self.opponent_strumline, |sig| {
                        matches!(sig, CharNoteSignal::Hit { direction, .. } if direction.index() as i64 == dir_i)
                    });
                }
            }
            HookKind::NoteHit => {
                let scoreable = orig.get_bool("scoreable").unwrap_or(true);
                let orig_score = orig.get_f64("score").unwrap_or(0.0);
                let orig_hd = orig.get_f64("health_change").unwrap_or(0.0);
                let dir_idx = orig.get_i64("dir").unwrap_or(0).max(0) as usize;
                let note_id = orig.get_i64("id").unwrap_or(0).max(0) as usize;
                let note_idx = orig.get_i64("note_idx").unwrap_or(-1);

                if ev.canceled {
                    if scoreable {
                        self.song_score -= orig_score;
                        self.health = (self.health - orig_hd).clamp(HEALTH_MIN, HEALTH_MAX);
                        self.tallies.total_notes_hit = self.tallies.total_notes_hit.saturating_sub(1);
                        decrement_tally(self, orig.get_str("judgement"));
                        if let Some(cb) = orig.get_i64("combo_before") {
                            self.tallies.combo = cb.max(0) as u32;
                        }
                        if let Some(mcb) = orig.get_i64("max_combo_before") {
                            self.tallies.max_combo = mcb.max(0) as u32;
                        }
                    }
                    if note_idx >= 0
                        && let Some(n) = self.player_strumline.notes.get_mut(note_idx as usize)
                        && n.data.id == note_id
                    {
                        n.has_been_hit = false;
                        n.killed = false;
                        n.desaturated = false;
                        n.alpha = 1.0;
                        n.hold_hit = false;
                        n.hold_killed = false;
                        n.hold_remaining_ms = n.data.sustain_length_ms;
                    }
                    if let Some(slot) = self.player_strumline.active_holds.get_mut(dir_idx)
                        && slot.is_some_and(|h| h.note_id == note_id)
                    {
                        *slot = None;
                    }
                    if let Some(pos) = self
                        .player_strumline
                        .pending_splashes
                        .iter()
                        .rposition(|d| d.index() == dir_idx)
                    {
                        self.player_strumline.pending_splashes.remove(pos);
                    }
                    self.pending_popups.pop();
                    pop_matching_char_event(&mut self.player_strumline, |sig| {
                        matches!(sig, CharNoteSignal::Hit { direction, .. } if direction.index() == dir_idx)
                    });
                } else {
                    if let Some(new_score) = ev.get_f64("score")
                        && (new_score - orig_score).abs() > f64::EPSILON
                    {
                        self.song_score += new_score - orig_score;
                    }
                    if let Some(new_hd) = ev.get_f64("health_change")
                        && (new_hd - orig_hd).abs() > f64::EPSILON
                    {
                        self.health = (self.health + (new_hd - orig_hd)).clamp(HEALTH_MIN, HEALTH_MAX);
                    }
                    let orig_splash = orig.get_bool("does_notesplash").unwrap_or(false);
                    if let Some(want_splash) = ev.get_bool("does_notesplash").or_else(|| ev.get_bool("splash")) {
                        if !want_splash && orig_splash {
                            if let Some(pos) = self
                                .player_strumline
                                .pending_splashes
                                .iter()
                                .rposition(|d| d.index() == dir_idx)
                            {
                                self.player_strumline.pending_splashes.remove(pos);
                            }
                        } else if want_splash && !orig_splash {
                            self.player_strumline
                                .pending_splashes
                                .push(NoteDirection::from_int(dir_idx as i32));
                        }
                    }
                    if ev.get_bool("play_anim") == Some(false) {
                        pop_matching_char_event(&mut self.player_strumline, |sig| {
                            matches!(sig, CharNoteSignal::Hit { direction, .. } if direction.index() == dir_idx)
                        });
                    }
                }
            }
            HookKind::NoteMiss => {
                let orig_hd = orig.get_f64("health_change").unwrap_or(0.0);
                let orig_sd = orig.get_f64("score_change").unwrap_or(0.0);
                let dir_idx = orig.get_i64("dir").unwrap_or(0).max(0) as usize;
                if ev.canceled {
                    self.health = (self.health - orig_hd).clamp(HEALTH_MIN, HEALTH_MAX);
                    self.song_score -= orig_sd;
                    if let Some(cb) = orig.get_i64("combo_before") {
                        self.tallies.combo = cb.max(0) as u32;
                    }
                    if let Some(mcb) = orig.get_i64("max_combo_before") {
                        self.tallies.max_combo = mcb.max(0) as u32;
                    }
                    self.tallies.missed = self.tallies.missed.saturating_sub(1);
                    self.pending_miss_sfx = false;
                    self.player_vocals_volume = 1.0;
                    pop_matching_char_event(&mut self.player_strumline, |sig| {
                        matches!(sig, CharNoteSignal::Miss { direction, .. } if direction.index() == dir_idx)
                    });
                } else {
                    if let Some(new_hd) = ev.get_f64("health_change")
                        && (new_hd - orig_hd).abs() > f64::EPSILON
                    {
                        self.health = (self.health + (new_hd - orig_hd)).clamp(HEALTH_MIN, HEALTH_MAX);
                    }
                    if let Some(new_sd) = ev.get_f64("score_change")
                        && (new_sd - orig_sd).abs() > f64::EPSILON
                    {
                        self.song_score += new_sd - orig_sd;
                    }
                    if ev.get_bool("play_sound").or_else(|| ev.get_bool("play_sfx")) == Some(false) {
                        self.pending_miss_sfx = false;
                    }
                    if ev.get_bool("play_anim") == Some(false) {
                        pop_matching_char_event(&mut self.player_strumline, |sig| {
                            matches!(sig, CharNoteSignal::Miss { direction, .. } if direction.index() == dir_idx)
                        });
                    }
                }
            }
            HookKind::HoldDrop => {
                let orig_hd = orig.get_f64("health_change").unwrap_or(0.0);
                let orig_sd = orig.get_f64("score_change").unwrap_or(0.0);
                let dir_idx = orig.get_i64("dir").unwrap_or(0).max(0) as usize;
                if ev.canceled {
                    self.health = (self.health - orig_hd).clamp(HEALTH_MIN, HEALTH_MAX);
                    self.song_score -= orig_sd;
                    if let Some(cb) = orig.get_i64("combo_before") {
                        self.tallies.combo = cb.max(0) as u32;
                    }
                    if let Some(mcb) = orig.get_i64("max_combo_before") {
                        self.tallies.max_combo = mcb.max(0) as u32;
                    }
                    self.pending_miss_sfx = false;
                    self.player_vocals_volume = 1.0;
                    pop_matching_char_event(&mut self.player_strumline, |sig| {
                        matches!(sig, CharNoteSignal::HoldDrop { direction, .. } if direction.index() == dir_idx)
                    });
                } else {
                    if let Some(new_hd) = ev.get_f64("health_change")
                        && (new_hd - orig_hd).abs() > f64::EPSILON
                    {
                        self.health = (self.health + (new_hd - orig_hd)).clamp(HEALTH_MIN, HEALTH_MAX);
                    }
                    if let Some(new_sd) = ev.get_f64("score_change")
                        && (new_sd - orig_sd).abs() > f64::EPSILON
                    {
                        self.song_score += new_sd - orig_sd;
                    }
                    if ev.get_bool("play_sound").or_else(|| ev.get_bool("play_sfx")) == Some(false) {
                        self.pending_miss_sfx = false;
                    }
                    if ev.get_bool("play_anim") == Some(false) {
                        pop_matching_char_event(&mut self.player_strumline, |sig| {
                            matches!(sig, CharNoteSignal::HoldDrop { direction, .. } if direction.index() == dir_idx)
                        });
                    }
                }
            }
            HookKind::GhostMiss => {
                let orig_hd = orig.get_f64("health_change").unwrap_or(0.0);
                let orig_sd = orig.get_f64("score_change").unwrap_or(0.0);
                let dir_idx = orig.get_i64("dir").unwrap_or(0).max(0) as usize;
                if ev.canceled {
                    self.health = (self.health - orig_hd).clamp(HEALTH_MIN, HEALTH_MAX);
                    self.song_score -= orig_sd;
                    self.pending_miss_sfx = false;
                    self.player_vocals_volume = 1.0;
                    pop_matching_char_event(&mut self.player_strumline, |sig| {
                        matches!(sig, CharNoteSignal::GhostMiss { direction } if direction.index() == dir_idx)
                    });
                } else {
                    if let Some(new_hd) = ev.get_f64("health_change")
                        && (new_hd - orig_hd).abs() > f64::EPSILON
                    {
                        self.health = (self.health + (new_hd - orig_hd)).clamp(HEALTH_MIN, HEALTH_MAX);
                    }
                    if let Some(new_sd) = ev.get_f64("score_change")
                        && (new_sd - orig_sd).abs() > f64::EPSILON
                    {
                        self.song_score += new_sd - orig_sd;
                    }
                    if ev.get_bool("play_sound").or_else(|| ev.get_bool("play_sfx")) == Some(false) {
                        self.pending_miss_sfx = false;
                    }
                    if ev.get_bool("play_anim") == Some(false) {
                        pop_matching_char_event(&mut self.player_strumline, |sig| {
                            matches!(sig, CharNoteSignal::GhostMiss { direction } if direction.index() == dir_idx)
                        });
                    }
                }
            }
            _ => {}
        }
    }

    /// Advances gameplay state for one frame and executes attached mod scripts.
    #[allow(dead_code)]
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
