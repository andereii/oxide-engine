use std::collections::{BTreeMap, VecDeque};

use bevy::prelude::Resource;

use crate::mods::{ModRegistry, ModStore};
use crate::scripting::{
    HookKind, ScriptCommand, ScriptEvent, ScriptHost, ScriptMutableState, ScriptStateSnapshot,
    TweenProp,
};
use crate::utils::conductor::ConductorUpdate;
use crate::utils::conductor::Conductor;
use crate::utils::constants::{
    AUDIO_CORRECTION_EASE_RATIO, CONDUCTOR_DRIFT_THRESHOLD_MS, DEFAULT_BOP_INTENSITY,
    DEFAULT_CAMERA_ANGLE_DEG, DEFAULT_ZOOM_OFFSET, DEFAULT_ZOOM_RATE, HEALTH_LERP_FACTOR,
    HEALTH_MAX, HEALTH_MIN, HEALTH_STARTING, KEY_COUNT, MAX_RELATIVE_CAM_ZOOM,
    MUSIC_EASE_RATIO, STEPS_PER_BEAT,
};
use crate::utils::easing::EaseKind;

use super::camera::events::parse_raw_song_event;
use super::hud::judgement::Tallies;
use super::hud::popups::PopupEvent;
use super::notes::{NoteKindManager, Strumline};
use super::playlist::PlayStatePlaylist;
use super::types::{CameraState, LinearTween, PauseMode, PlayPhase, PlayStateParams, PreciseInputEvent, SongChart, SongEventData};

#[derive(Resource)]
pub struct PlayState {
    pub chart: SongChart,
    pub current_difficulty: String,
    pub current_variation: String,
    pub current_instrumental: String,
    pub phase: PlayPhase,
    pub needs_reset: bool,
    pub critical_failure: bool,
    pub starting_song: bool,
    pub is_in_countdown: bool,
    pub is_in_cutscene: bool,
    pub is_practice_mode: bool,
    pub is_bot_play_mode: bool,
    pub is_minimal_mode: bool,
    pub is_player_dying: bool,
    pub may_pause_game: bool,
    pub disable_keys: bool,
    pub death_counter: u32,
    pub health: f64,
    pub health_lerp: f64,
    pub song_score: f64,
    pub tallies: Tallies,
    pub start_timestamp_ms: f64,
    pub playback_rate: f64,
    pub instrumental_volume: f64,
    pub player_vocals_volume: f64,
    pub opponent_vocals_volume: f64,
    pub conductor: Conductor,
    pub camera: CameraState,
    pub player_strumline: Strumline,
    pub opponent_strumline: Strumline,
    pub note_kinds: NoteKindManager,
    pub song_events: Vec<SongEventData>,
    pub scroll_speed_tweens: Vec<(bool, LinearTween)>,
    pub input_press_queue: VecDeque<PreciseInputEvent>,
    pub input_release_queue: VecDeque<PreciseInputEvent>,
    pub countdown_ticks_remaining: i32,
    pub countdown_timer_s: f64,
    pub pending_countdown_step: Option<usize>,
    pub pending_miss_sfx: bool,
    pub pending_popups: Vec<PopupEvent>,
    pub score_text: String,
    pub custom_score_text: Option<String>,
    pub hud_visible: bool,
    pub strums_swapped: bool,
    pub pending_hooks: Vec<(HookKind, ScriptEvent)>,
    pub script_char_positions: BTreeMap<String, [f32; 2]>,
    pub script_prop_positions: BTreeMap<String, [f32; 2]>,
    pub script_prop_scales: BTreeMap<String, [f32; 2]>,
    pub script_prop_angles: BTreeMap<String, f32>,
    pub script_prop_alphas: BTreeMap<String, f32>,
    pub status_message: String,
    pub last_hud_key: (i64, u32, u32, i32),
    pub last_raw_audio_ms: Option<f64>,
}

impl PlayState {
    pub fn new(params: PlayStateParams) -> Self {
        let bpm = params.chart.bpm;
        let scroll_speed = params.chart.scroll_speed;
        let difficulty = params.chart.difficulty.clone();
        let variation = params.chart.variation.clone();
        let mut camera = CameraState::default();
        if let Some(pt) = params.camera_follow_point {
            camera.follow_point = pt;
        }
        let mut state = Self {
            chart: params.chart,
            current_difficulty: difficulty,
            current_variation: variation,
            current_instrumental: params.instrumental,
            phase: PlayPhase::Countdown,
            needs_reset: false,
            critical_failure: false,
            starting_song: true,
            is_in_countdown: true,
            is_in_cutscene: false,
            is_practice_mode: params.practice_mode,
            is_bot_play_mode: params.bot_play_mode,
            is_minimal_mode: params.minimal_mode,
            is_player_dying: false,
            may_pause_game: true,
            disable_keys: false,
            death_counter: 0,
            health: HEALTH_STARTING,
            health_lerp: HEALTH_STARTING,
            song_score: 0.0,
            tallies: Tallies::default(),
            start_timestamp_ms: params.start_timestamp_ms,
            playback_rate: params.playback_rate,
            instrumental_volume: 1.0,
            player_vocals_volume: 1.0,
            opponent_vocals_volume: 1.0,
            conductor: Conductor::new(bpm),
            camera,
            player_strumline: Strumline::new(true, scroll_speed, params.note_style.clone()),
            opponent_strumline: Strumline::new(false, scroll_speed, params.note_style),
            note_kinds: NoteKindManager::default(),
            song_events: Vec::new(),
            scroll_speed_tweens: Vec::new(),
            input_press_queue: VecDeque::new(),
            input_release_queue: VecDeque::new(),
            countdown_ticks_remaining: 4,
            countdown_timer_s: 0.0,
            pending_countdown_step: None,
            pending_miss_sfx: false,
            pending_popups: Vec::new(),
            score_text: String::new(),
            custom_score_text: None,
            hud_visible: true,
            strums_swapped: false,
            pending_hooks: Vec::new(),
            script_char_positions: BTreeMap::new(),
            script_prop_positions: BTreeMap::new(),
            script_prop_scales: BTreeMap::new(),
            script_prop_angles: BTreeMap::new(),
            script_prop_alphas: BTreeMap::new(),
            status_message: String::new(),
            last_hud_key: (i64::MIN, u32::MAX, u32::MAX, i32::MIN),
            last_raw_audio_ms: None,
        };
        state.create();
        state
    }

    pub fn create(&mut self) {
        self.generate_song();
        self.player_strumline.fade_in_arrows();
        self.opponent_strumline.fade_in_arrows();
        self.start_countdown();
        self.update_score_text();
    }

    pub fn start_countdown(&mut self) {
        self.is_in_countdown = true;
        self.starting_song = true;
        self.last_raw_audio_ms = None;
        self.phase = PlayPhase::Countdown;
        self.countdown_ticks_remaining = 4;
        self.countdown_timer_s = 0.0;
        self.pending_countdown_step = Some(0);
        let offset_ms = self.start_timestamp_ms - self.conductor.beat_length_ms() * 4.0;
        self.conductor.update(offset_ms);
        self.status_message = "3... 2... 1... GO!".to_string();
        self.pending_hooks
            .push((HookKind::CountdownStart, ScriptEvent::new(HookKind::CountdownStart)));
    }

    pub fn start_song(&mut self) {
        self.starting_song = false;
        self.is_in_countdown = false;
        self.last_raw_audio_ms = None;
        self.phase = PlayPhase::Playing;
        self.pending_hooks
            .push((HookKind::CountdownEnd, ScriptEvent::new(HookKind::CountdownEnd)));
        let tick = self.conductor.update(self.start_timestamp_ms);
        if tick.step_changed {
            self.step_hit();
        }
        self.pending_hooks.push((
            HookKind::SongStart,
            ScriptEvent::new(HookKind::SongStart)
                .with_field("song", self.chart.song_id.clone())
                .with_field("bpm", self.conductor.bpm()),
        ));
        self.status_message = format!("Playing: {} [{}]", self.chart.song_name, self.current_difficulty);
    }

    fn sync_song_clock(&mut self, elapsed_s: f64, audio_time_ms: Option<f64>) -> ConductorUpdate {
        let delta_ms = elapsed_s * 1000.0 * self.playback_rate;
        let prev_pos = self.conductor.song_position_ms;
        let predicted_ms = prev_pos + delta_ms;

        let (false, Some(raw_audio_ms)) = (self.is_in_countdown, audio_time_ms) else {
            self.last_raw_audio_ms = None;
            let next_pos = if self.is_in_countdown {
                predicted_ms.min(self.start_timestamp_ms).max(prev_pos)
            } else {
                predicted_ms
            };
            return self.conductor.update(next_pos);
        };

        let target_ms = raw_audio_ms + self.conductor.combined_offset();
        let drift_ms = target_ms - predicted_ms;
        // Only hard-snap forward. Snapping backward on audio startup/buffer lag drags notes
        // backward and triggers character reset heuristics.
        if drift_ms.round() > CONDUCTOR_DRIFT_THRESHOLD_MS {
            self.last_raw_audio_ms = Some(raw_audio_ms);
            return self.conductor.update_with_offsets(raw_audio_ms, true);
        }

        // rodio updates `sink.position()` only when its audio mixer buffer refills,
        // so repeated identical readings between refills are stale. Integrate
        // `delta_ms` smoothly on stale frames instead of stalling or pulling back.
        let audio_advanced = self
            .last_raw_audio_ms
            .is_none_or(|prev| (raw_audio_ms - prev).abs() > 0.05);
        self.last_raw_audio_ms = Some(raw_audio_ms);

        let next_pos = if drift_ms > 0.0 {
            let ease_ratio = 1.0 - (-(MUSIC_EASE_RATIO * self.playback_rate) * elapsed_s).exp();
            predicted_ms + drift_ms * ease_ratio
        } else if (audio_advanced || drift_ms < -CONDUCTOR_DRIFT_THRESHOLD_MS)
            && drift_ms < -(delta_ms + 1e-6)
        {
            let excess_ms = drift_ms + delta_ms;
            let ease_ratio = 1.0 - (-(AUDIO_CORRECTION_EASE_RATIO * self.playback_rate) * elapsed_s).exp();
            (predicted_ms + excess_ms * ease_ratio).max(prev_pos + delta_ms * 0.25)
        } else {
            predicted_ms
        };
        self.conductor.update(next_pos.max(prev_pos))
    }

    pub fn update(&mut self, elapsed_s: f64, audio_time_ms: Option<f64>, playlist: &mut PlayStatePlaylist) {
        if self.critical_failure || matches!(self.phase, PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended) {
            return;
        }

        if self.needs_reset {
            self.reset_song();
            return;
        }

        self.update_health_bar(elapsed_s);
        self.update_score_text();
        self.advance_tweens(elapsed_s);

        let tick = self.sync_song_clock(elapsed_s, audio_time_ms);

        if self.is_in_countdown {
            let beat_s = self.conductor.beat_length_ms() / 1000.0;
            self.countdown_timer_s += elapsed_s * self.playback_rate;
            if self.countdown_timer_s >= beat_s {
                self.countdown_timer_s -= beat_s;
                self.countdown_ticks_remaining -= 1;
                if self.countdown_ticks_remaining > 0 {
                    let step = (4 - self.countdown_ticks_remaining) as usize;
                    if step < 4 {
                        self.pending_countdown_step = Some(step);
                        self.pending_hooks.push((
                            HookKind::CountdownStep,
                            ScriptEvent::new(HookKind::CountdownStep).with_field("step", step as i64),
                        ));
                    }
                } else {
                    self.start_song();
                }
            }
        }

        let next_pos = self.conductor.song_position_ms;
        self.player_strumline.update_notes(elapsed_s, next_pos, self.start_timestamp_ms);
        self.opponent_strumline.update_notes(elapsed_s, next_pos, self.start_timestamp_ms);

        if tick.step_changed {
            self.step_hit();
        }
        if tick.beat_changed {
            self.beat_hit();
        }

        self.process_song_events();
        self.process_input_queue();
        self.process_notes(elapsed_s);

        if self.health <= HEALTH_MIN && !self.is_practice_mode && !self.is_player_dying {
            self.move_to_game_over();
            return;
        }

        if !self.is_in_countdown && self.conductor.song_position_ms >= self.chart.length_ms {
            self.end_song(playlist);
        }
    }

    pub fn reset_song(&mut self) {
        self.needs_reset = false;
        self.is_player_dying = false;
        self.health = HEALTH_STARTING;
        self.health_lerp = HEALTH_STARTING;
        self.song_score = 0.0;
        self.tallies = Tallies::default();
        self.input_press_queue.clear();
        self.input_release_queue.clear();
        self.pending_popups.clear();
        self.custom_score_text = None;
        self.hud_visible = true;
        self.strums_swapped = false;
        self.player_strumline.middlescroll = false;
        self.opponent_strumline.middlescroll = false;
        self.player_strumline.strums_swapped = false;
        self.opponent_strumline.strums_swapped = false;
        for rec in &mut self.player_strumline.receptors {
            rec.reset_modchart();
        }
        for rec in &mut self.opponent_strumline.receptors {
            rec.reset_modchart();
        }
        self.script_prop_positions.clear();
        self.script_prop_scales.clear();
        self.script_prop_angles.clear();
        self.script_prop_alphas.clear();
        self.cancel_all_camera_tweens();
        self.cancel_scroll_speed_tweens();
        self.reset_camera(true, true, true);
        self.camera.angle_deg = DEFAULT_CAMERA_ANGLE_DEG;
        self.camera.hud_angle_deg = DEFAULT_CAMERA_ANGLE_DEG;
        self.camera.bop_intensity = DEFAULT_BOP_INTENSITY;
        self.camera.hud_zoom_intensity = (DEFAULT_BOP_INTENSITY - 1.0) * 2.0;
        self.camera.zoom_rate = DEFAULT_ZOOM_RATE;
        self.camera.zoom_rate_offset = DEFAULT_ZOOM_OFFSET;
        self.player_strumline.clean();
        self.opponent_strumline.clean();
        self.player_strumline.use_custom_render_distance = false;
        self.opponent_strumline.use_custom_render_distance = false;
        self.rebuild_scroll_velocities();
        self.regen_note_data(self.start_timestamp_ms);
        self.player_strumline.fade_in_arrows();
        self.opponent_strumline.fade_in_arrows();
        self.pending_hooks
            .push((HookKind::SongRetry, ScriptEvent::new(HookKind::SongRetry)));
        self.start_countdown();
    }

    pub fn step_hit(&mut self) {
        if self.camera.hud_zoom < MAX_RELATIVE_CAM_ZOOM * self.camera.default_hud_zoom
            && self.camera.zoom_rate > 0.0
        {
            let step_period = (self.camera.zoom_rate * f64::from(STEPS_PER_BEAT)).round() as i32;
            let step_offset = (self.camera.zoom_rate_offset * f64::from(STEPS_PER_BEAT)).round() as i32;
            if step_period > 0 && (self.conductor.current_step + step_offset).rem_euclid(step_period) == 0 {
                self.camera.bop_multiplier += self.camera.bop_intensity - 1.0;
                self.camera.hud_zoom += self.camera.hud_zoom_intensity * self.camera.default_hud_zoom;
            }
        }
        let step = self.conductor.current_step;
        let beat = self.conductor.current_beat;
        let measure = beat.div_euclid(i32::from(STEPS_PER_BEAT));
        self.pending_hooks.push((
            HookKind::StepHit,
            ScriptEvent::new(HookKind::StepHit)
                .with_field("step", i64::from(step))
                .with_field("beat", i64::from(beat))
                .with_field("measure", i64::from(measure)),
        ));
    }

    pub fn beat_hit(&mut self) {
        let step = self.conductor.current_step;
        let beat = self.conductor.current_beat;
        let measure = beat.div_euclid(i32::from(STEPS_PER_BEAT));
        self.pending_hooks.push((
            HookKind::BeatHit,
            ScriptEvent::new(HookKind::BeatHit)
                .with_field("beat", i64::from(beat))
                .with_field("step", i64::from(step))
                .with_field("measure", i64::from(measure)),
        ));
        if beat.rem_euclid(i32::from(STEPS_PER_BEAT)) == 0 {
            self.pending_hooks.push((
                HookKind::MeasureHit,
                ScriptEvent::new(HookKind::MeasureHit)
                    .with_field("measure", i64::from(measure))
                    .with_field("beat", i64::from(beat))
                    .with_field("step", i64::from(step)),
            ));
        }
    }

    pub fn pause(&mut self, mode: PauseMode) {
        if !self.may_pause_game || matches!(self.phase, PlayPhase::GameOver | PlayPhase::Ended) {
            return;
        }
        self.phase = PlayPhase::Paused(mode);
        self.status_message = "Paused".to_string();
        self.pending_hooks
            .push((HookKind::Pause, ScriptEvent::new(HookKind::Pause)));
    }

    pub fn resume(&mut self) {
        if matches!(self.phase, PlayPhase::Paused(_)) {
            self.phase = if self.is_in_countdown { PlayPhase::Countdown } else { PlayPhase::Playing };
            self.status_message = format!("Playing: {} [{}]", self.chart.song_name, self.current_difficulty);
            self.pending_hooks
                .push((HookKind::Resume, ScriptEvent::new(HookKind::Resume)));
        }
    }

    pub fn move_to_game_over(&mut self) {
        self.is_player_dying = true;
        self.death_counter += 1;
        self.phase = PlayPhase::GameOver;
        self.status_message = format!("Game Over (deaths: {}) - Press R to Retry", self.death_counter);
        self.pending_hooks.push((
            HookKind::GameOver,
            ScriptEvent::new(HookKind::GameOver).with_field("deaths", i64::from(self.death_counter)),
        ));
    }

    pub fn end_song(&mut self, playlist: &mut PlayStatePlaylist) {
        self.phase = PlayPhase::Ended;
        if playlist.is_story_mode {
            playlist.campaign_score += self.song_score.round() as i32;
            if !playlist.playlist_song_ids.is_empty() {
                playlist.playlist_song_ids.remove(0);
            }
        }
        self.status_message = format!(
            "Song Complete! Score: {} | Sick: {} Good: {} Bad: {} Shit: {} Miss: {}",
            self.song_score.round() as i64, self.tallies.sick, self.tallies.good,
            self.tallies.bad, self.tallies.shit, self.tallies.missed
        );
        self.pending_hooks.push((
            HookKind::SongEnd,
            ScriptEvent::new(HookKind::SongEnd)
                .with_field("score", self.song_score)
                .with_field("misses", i64::from(self.tallies.missed)),
        ));
    }

    pub fn update_health_bar(&mut self, elapsed_s: f64) {
        self.health = self.health.clamp(HEALTH_MIN, HEALTH_MAX);
        if self.is_bot_play_mode {
            self.health_lerp = HEALTH_MAX;
        } else {
            let alpha = 1.0 - (1.0 - HEALTH_LERP_FACTOR).powf(elapsed_s * 60.0);
            self.health_lerp += (self.health - self.health_lerp) * alpha;
        }
    }

    pub fn update_score_text(&mut self) {
        if let Some(custom) = &self.custom_score_text {
            if self.score_text != *custom {
                self.score_text = custom.clone();
            }
            return;
        }
        let key = (self.song_score.round() as i64, self.tallies.combo, self.tallies.missed, ((self.health / HEALTH_MAX) * 100.0).round() as i32);
        if self.last_hud_key == key { return; }
        self.last_hud_key = key;
        self.score_text = format!("Score: {} | Combo: {} | Misses: {} | Health: {}%", key.0, key.1, key.2, key.3);
    }

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

    fn receptor_mut(&mut self, lane: usize) -> Option<&mut super::notes::strum::ReceptorState> {
        if lane < KEY_COUNT {
            self.player_strumline.receptors.get_mut(lane)
        } else {
            self.opponent_strumline.receptors.get_mut(lane - KEY_COUNT)
        }
    }

    /// Applies state-level script commands and updates prop caches. Returns `true` if fully handled
    /// without needing downstream ECS execution in [`PlayPlugin`](super::PlayPlugin).
    pub fn apply_state_script_command(
        &mut self,
        cmd: &ScriptCommand,
        playlist: &mut PlayStatePlaylist,
    ) -> bool {
        match cmd {
            ScriptCommand::SetStatus(msg) => {
                self.status_message = msg.clone();
                true
            }
            ScriptCommand::SetScoreText(txt) => {
                self.custom_score_text = txt.clone();
                self.last_hud_key = (i64::MIN, u32::MAX, u32::MAX, i32::MIN);
                self.update_score_text();
                true
            }
            ScriptCommand::SetHudVisible(vis) => {
                self.hud_visible = *vis;
                true
            }
            ScriptCommand::TweenCameraZoom { target, duration_s, ease } => {
                let parsed = EaseKind::from_name(ease, "");
                self.tween_camera_zoom_eased(*target, *duration_s, true, parsed);
                true
            }
            ScriptCommand::TweenCameraPos { x, y, duration_s, ease } => {
                let parsed = EaseKind::from_name(ease, "").unwrap_or_default();
                self.tween_camera_to_position_eased(*x, *y, *duration_s, parsed);
                true
            }
            ScriptCommand::TweenScrollSpeed { target, duration_s, ease } => {
                let parsed = EaseKind::from_name(ease, "");
                self.tween_scroll_speed_eased(*target, *duration_s, parsed, None, true, true);
                true
            }
            ScriptCommand::FocusCameraTarget(target) => {
                let idx = match target.trim().to_ascii_lowercase().as_str() {
                    "bf" | "player" | "0" => Some(0),
                    "dad" | "opponent" | "1" => Some(1),
                    "gf" | "spectator" | "2" => Some(2),
                    _ => None,
                };
                if let Some(i) = idx
                    && let Some(pt) = self.camera.focus_points[i]
                {
                    self.cancel_camera_follow_tween();
                    self.camera.follow_target = Some(pt);
                }
                true
            }
            ScriptCommand::CameraBop { game_delta, hud_delta } => {
                self.camera.bop_multiplier += *game_delta;
                self.camera.hud_zoom += *hud_delta * self.camera.default_hud_zoom;
                true
            }
            ScriptCommand::SetCameraBopConfig { intensity, rate } => {
                self.camera.bop_intensity = *intensity;
                self.camera.zoom_rate = *rate;
                true
            }
            ScriptCommand::CameraShake { intensity, duration_s, hud } => {
                if *hud {
                    self.camera.hud_shake_intensity = *intensity;
                    self.camera.hud_shake_timer_s = (*duration_s).max(0.0);
                    if *duration_s <= 0.0 {
                        self.camera.hud_shake_offset = [0.0, 0.0];
                    }
                } else {
                    self.camera.shake_intensity = *intensity;
                    self.camera.shake_timer_s = (*duration_s).max(0.0);
                    if *duration_s <= 0.0 {
                        self.camera.shake_offset = [0.0, 0.0];
                    }
                }
                true
            }
            ScriptCommand::CameraFlash { color, duration_s, hud } => {
                let dur = (*duration_s).max(0.0);
                if *hud {
                    self.camera.hud_flash_color = *color;
                    self.camera.hud_flash_duration_s = dur;
                    self.camera.hud_flash_timer_s = dur;
                } else {
                    self.camera.flash_color = *color;
                    self.camera.flash_duration_s = dur;
                    self.camera.flash_timer_s = dur;
                }
                true
            }
            ScriptCommand::SetLaneOffset { lane, x, y } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_offset = [*x, *y];
                }
                true
            }
            ScriptCommand::SetLaneAngle { lane, deg } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_angle_deg = *deg;
                }
                true
            }
            ScriptCommand::SetLaneScale { lane, sx, sy } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_scale = [*sx, *sy];
                }
                true
            }
            ScriptCommand::SetLaneAlpha { lane, alpha } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_alpha = alpha.clamp(0.0, 1.0);
                }
                true
            }
            ScriptCommand::ResetLanes => {
                for rec in &mut self.player_strumline.receptors {
                    rec.reset_modchart();
                }
                for rec in &mut self.opponent_strumline.receptors {
                    rec.reset_modchart();
                }
                true
            }
            ScriptCommand::SetAudioVolumes { inst, vocals, player_vocal, opponent_vocal } => {
                if let Some(v) = inst {
                    self.instrumental_volume = f64::from(*v).max(0.0);
                }
                if let Some(v) = vocals {
                    let f = f64::from(*v).max(0.0);
                    self.player_vocals_volume = f;
                    self.opponent_vocals_volume = f;
                }
                if let Some(v) = player_vocal {
                    self.player_vocals_volume = f64::from(*v).max(0.0);
                }
                if let Some(v) = opponent_vocal {
                    self.opponent_vocals_volume = f64::from(*v).max(0.0);
                }
                true
            }
            ScriptCommand::AddNote { time_ms, dir, length_ms, kind } => {
                let kind_opt = (!kind.is_empty()).then(|| kind.clone());
                self.add_dynamic_note(*time_ms, *dir, *length_ms, kind_opt);
                true
            }
            ScriptCommand::TriggerSongEvent { name, value } => {
                let ev = parse_raw_song_event(
                    self.conductor.song_position_ms,
                    name.clone(),
                    value.clone(),
                );
                self.song_events.push(ev);
                true
            }
            ScriptCommand::EndSong => {
                self.end_song(playlist);
                true
            }
            ScriptCommand::RestartSong => {
                self.needs_reset = true;
                true
            }
            ScriptCommand::PauseSong => {
                self.pause(PauseMode::Standard);
                true
            }
            ScriptCommand::SpawnProp { id, pos, color, .. } => {
                self.script_prop_positions.insert(id.clone(), *pos);
                self.script_prop_scales.insert(id.clone(), [1.0, 1.0]);
                self.script_prop_angles.insert(id.clone(), 0.0);
                self.script_prop_alphas.insert(id.clone(), color[3]);
                false
            }
            ScriptCommand::DespawnProp(id) => {
                self.script_prop_positions.remove(id);
                self.script_prop_scales.remove(id);
                self.script_prop_angles.remove(id);
                self.script_prop_alphas.remove(id);
                false
            }
            ScriptCommand::SetPropNumber { id, prop, value } => {
                match prop {
                    TweenProp::X => {
                        let y = self.script_prop_positions.get(id).map_or(0.0, |p| p[1]);
                        self.script_prop_positions.insert(id.clone(), [*value, y]);
                    }
                    TweenProp::Y => {
                        let x = self.script_prop_positions.get(id).map_or(0.0, |p| p[0]);
                        self.script_prop_positions.insert(id.clone(), [x, *value]);
                    }
                    TweenProp::ScaleX => {
                        let sy = self.script_prop_scales.get(id).map_or(1.0, |s| s[1]);
                        self.script_prop_scales.insert(id.clone(), [*value, sy]);
                    }
                    TweenProp::ScaleY => {
                        let sx = self.script_prop_scales.get(id).map_or(1.0, |s| s[0]);
                        self.script_prop_scales.insert(id.clone(), [sx, *value]);
                    }
                    TweenProp::Angle => {
                        self.script_prop_angles.insert(id.clone(), *value);
                    }
                    TweenProp::Alpha => {
                        self.script_prop_alphas.insert(id.clone(), value.clamp(0.0, 1.0));
                    }
                    _ => {}
                }
                false
            }
            ScriptCommand::SetPropTint { id, color } => {
                self.script_prop_alphas.insert(id.clone(), color[3].clamp(0.0, 1.0));
                false
            }
            ScriptCommand::SetCharacterPos { target, x, y } => {
                self.script_char_positions.insert(target.clone(), [*x, *y]);
                false
            }
            ScriptCommand::Log { .. }
            | ScriptCommand::PlayAnim { .. }
            | ScriptCommand::SetCharacterScale { .. }
            | ScriptCommand::SetCharacterTint { .. }
            | ScriptCommand::SetCharacterVisible { .. }
            | ScriptCommand::SetCharacterDanceEvery { .. }
            | ScriptCommand::SetPropVisible { .. }
            | ScriptCommand::PlaySfx { .. } => false,
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

