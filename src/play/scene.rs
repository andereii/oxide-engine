use std::collections::VecDeque;

use bevy::prelude::Resource;

use crate::utils::conductor::Conductor;
use crate::utils::constants::{
    DEFAULT_BOP_INTENSITY, DEFAULT_ZOOM_OFFSET, DEFAULT_ZOOM_RATE, HEALTH_LERP_FACTOR, HEALTH_MAX,
    HEALTH_MIN, HEALTH_STARTING,
};

use super::judgement::Tallies;
use super::notes::{NoteKindManager, Strumline};
use super::playlist::PlayStatePlaylist;
use super::popups::PopupEvent;
use super::types::{
    CameraState, LinearTween, PauseMode, PlayPhase, PlayStateParams, PreciseInputEvent,
    SongChart, SongEventData,
};

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
    pub status_message: String,
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
            player_strumline: Strumline::new(!params.bot_play_mode, scroll_speed, params.note_style.clone()),
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
            status_message: String::new(),
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
        self.phase = PlayPhase::Countdown;
        self.countdown_ticks_remaining = 4;
        self.countdown_timer_s = 0.0;
        self.pending_countdown_step = Some(0);
        let offset_ms = self.start_timestamp_ms - self.conductor.beat_length_ms() * 4.0;
        self.conductor.update(offset_ms);
        self.status_message = "3... 2... 1... GO!".to_string();
    }

    pub fn start_song(&mut self) {
        self.starting_song = false;
        self.is_in_countdown = false;
        self.phase = PlayPhase::Playing;
        self.conductor.update(self.start_timestamp_ms);
        self.status_message = format!("Playing: {} [{}]", self.chart.song_name, self.current_difficulty);
    }

    pub fn update(&mut self, elapsed_s: f64, audio_time_ms: Option<f64>, playlist: &mut PlayStatePlaylist) {
        if self.critical_failure || matches!(self.phase, PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended) {
            return;
        }

        if self.needs_reset {
            self.reset_song();
            return;
        }

        self.update_health_bar();
        self.update_score_text();
        self.advance_tweens(elapsed_s);

        let delta_ms = elapsed_s * 1000.0 * self.playback_rate;
        let next_pos = match (self.is_in_countdown, audio_time_ms) {
            (false, Some(ms)) if (ms - self.conductor.song_position_ms).abs() > 25.0 => ms,
            (false, Some(ms)) if ms > self.conductor.song_position_ms => ms,
            _ => self.conductor.song_position_ms + delta_ms,
        };
        let tick = self.conductor.update(next_pos);
        self.player_strumline.update_notes(elapsed_s, next_pos, self.start_timestamp_ms);
        self.opponent_strumline.update_notes(elapsed_s, next_pos, self.start_timestamp_ms);

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
                    }
                } else {
                    self.start_song();
                }
            }
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
        self.cancel_all_camera_tweens();
        self.cancel_scroll_speed_tweens();
        self.reset_camera(true, true, true);
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
        self.start_countdown();
    }

    pub fn beat_hit(&mut self) {
        if self.camera.zoom_rate > 0.0 {
            let rate = self.camera.zoom_rate.max(1.0) as i32;
            let offset = self.camera.zoom_rate_offset as i32;
            if (self.conductor.current_beat - offset).rem_euclid(rate) == 0 {
                self.camera.bop_multiplier = self.camera.bop_intensity;
                self.camera.hud_zoom = self.camera.default_hud_zoom + self.camera.hud_zoom_intensity;
            }
        }
    }

    pub fn pause(&mut self, mode: PauseMode) {
        if !self.may_pause_game || matches!(self.phase, PlayPhase::GameOver | PlayPhase::Ended) {
            return;
        }
        self.phase = PlayPhase::Paused(mode);
        self.status_message = "Paused".to_string();
    }

    pub fn resume(&mut self) {
        if matches!(self.phase, PlayPhase::Paused(_)) {
            self.phase = if self.is_in_countdown { PlayPhase::Countdown } else { PlayPhase::Playing };
            self.status_message = format!("Playing: {} [{}]", self.chart.song_name, self.current_difficulty);
        }
    }

    pub fn move_to_game_over(&mut self) {
        self.is_player_dying = true;
        self.death_counter += 1;
        self.phase = PlayPhase::GameOver;
        self.status_message = format!("Game Over (deaths: {}) - Press R to Retry", self.death_counter);
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
    }

    pub fn update_health_bar(&mut self) {
        self.health = self.health.clamp(HEALTH_MIN, HEALTH_MAX);
        self.health_lerp += (self.health - self.health_lerp) * HEALTH_LERP_FACTOR;
    }

    pub fn update_score_text(&mut self) {
        self.score_text = format!(
            "Score: {} | Combo: {} | Misses: {} | Health: {:.0}%",
            self.song_score.round() as i64, self.tallies.combo, self.tallies.missed, (self.health / HEALTH_MAX) * 100.0
        );
    }
}

