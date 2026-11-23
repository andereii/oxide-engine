use std::collections::{BTreeMap, HashMap, VecDeque};

use bevy::prelude::Resource;

use crate::scripting::{HookKind, ScriptCommand, ScriptEvent};
use crate::utils::conductor::Conductor;
use crate::utils::constants::{
    DEFAULT_BOP_INTENSITY, DEFAULT_CAMERA_ANGLE_DEG, DEFAULT_ZOOM_OFFSET, DEFAULT_ZOOM_RATE,
    HEALTH_MIN, HEALTH_STARTING,
};
use crate::utils::easing::EaseKind;

const HUD_FADE_DURATION_S: f64 = 0.5;

use super::hud::judgement::Tallies;
use super::hud::popups::PopupEvent;
use super::hud::subtitles::{
    SubtitleConfig, SubtitleConfigPatch, SubtitleCue, parse_subtitles_auto,
};
use super::notes::{NoteKindManager, Strumline};
use super::playlist::PlayStatePlaylist;
use super::types::{CameraState, LinearTween, PauseMode, PlayPhase, PlayStateParams, PreciseInputEvent, SongChart, SongEventData};

#[derive(Debug, Clone)]
pub struct SongEventRevertSnapshot {
    pub camera: CameraState,
    pub player_scroll: f64,
    pub opponent_scroll: f64,
    pub scroll_tweens: Vec<(bool, LinearTween)>,
    pub player_custom_dist: (bool, f64),
    pub opponent_custom_dist: (bool, f64),
}

#[derive(Resource)]
pub struct PlayState {
    pub chart: SongChart,
    pub current_difficulty: String,
    pub current_variation: String,
    #[allow(dead_code)]
    pub current_instrumental: String,
    pub phase: PlayPhase,
    pub needs_reset: bool,
    pub critical_failure: bool,
    pub starting_song: bool,
    pub is_in_countdown: bool,
    pub is_in_cutscene: bool,
    pub cutscene_dialogue_active: bool,
    pub cutscene_video_active: bool,
    pub cutscene_dialogue_speaker: String,
    pub cutscene_dialogue_box: String,
    pub is_practice_mode: bool,
    pub is_bot_play_mode: bool,
    pub is_minimal_mode: bool,
    pub loop_song: bool,
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
    pub countdown_paused: bool,
    pub pending_countdown_step: Option<usize>,
    pub pending_miss_sfx: bool,
    pub pending_popups: Vec<PopupEvent>,
    pub popups_visible: bool,
    pub popup_offset: [f32; 2],
    pub score_text: String,
    pub custom_score_text: Option<String>,
    pub hud_visible: bool,
    pub hud_alpha: f32,
    pub hud_alpha_tween: Option<LinearTween>,
    pub master_volume: f64,
    pub strums_swapped: bool,
    pub pending_hooks: Vec<(HookKind, ScriptEvent)>,
    pub pending_song_event_reverts: HashMap<usize, SongEventRevertSnapshot>,
    pub next_song_event_seq: usize,
    pub last_song_end_popped: Option<(String, i32)>,
    pub script_char_positions: BTreeMap<String, [f32; 2]>,
    pub script_char_top_left: BTreeMap<String, [f32; 2]>,
    pub script_char_z: BTreeMap<String, f32>,
    pub script_char_scales: BTreeMap<String, [f32; 2]>,
    pub script_char_alphas: BTreeMap<String, f32>,
    pub script_char_flips: BTreeMap<String, [bool; 2]>,
    pub script_char_vis: BTreeMap<String, bool>,
    pub script_char_sizes: BTreeMap<String, [f32; 2]>,
    pub script_char_focus: BTreeMap<String, [f64; 2]>,
    pub script_char_anims: BTreeMap<String, String>,
    pub script_char_frames: BTreeMap<String, usize>,
    pub script_char_anim_finished: BTreeMap<String, bool>,
    pub script_prop_positions: BTreeMap<String, [f32; 2]>,
    pub script_prop_scales: BTreeMap<String, [f32; 2]>,
    pub script_prop_angles: BTreeMap<String, f32>,
    pub script_prop_alphas: BTreeMap<String, f32>,
    pub script_prop_z: BTreeMap<String, f32>,
    pub script_prop_scrolls: BTreeMap<String, [f32; 2]>,
    pub script_prop_flips: BTreeMap<String, [bool; 2]>,
    pub script_prop_vis: BTreeMap<String, bool>,
    pub script_prop_sizes: BTreeMap<String, [f32; 2]>,
    pub script_prop_velocities: BTreeMap<String, [f32; 2]>,
    pub script_prop_anims: BTreeMap<String, String>,
    pub script_prop_anim_done: BTreeMap<String, bool>,
    pub script_texts: BTreeMap<String, String>,
    pub script_text_positions: BTreeMap<String, [f32; 2]>,
    pub script_text_alphas: BTreeMap<String, f32>,
    pub script_active_sounds: Vec<String>,
    pub script_sound_volumes: BTreeMap<String, f32>,
    pub script_keys_down: Vec<String>,
    pub script_keys_hit: Vec<String>,
    pub script_keys_up: Vec<String>,
    pub script_ctrls_down: Vec<String>,
    pub script_ctrls_hit: Vec<String>,
    pub script_ctrls_up: Vec<String>,
    pub status_message: String,
    pub last_hud_key: (i64, u32, u32, i32),
    pub last_raw_audio_ms: Option<f64>,
    pub audio_finished: bool,
    pub song_end_canceled: bool,
    pub subtitle_cues: Vec<SubtitleCue>,
    pub subtitle_config: SubtitleConfig,
    pub subtitle_default_config: SubtitleConfig,
    pub subtitle_elapsed_ms: f64,
    pub subtitle_time_ms: f64,
    pub subtitle_sync_to_song: bool,
    pub subtitle_text: String,
    pub subtitle_active_index: Option<usize>,
    pub subtitle_prev_index: Option<usize>,
    pub subtitle_exiting_index: Option<usize>,
    pub subtitle_next_index: Option<usize>,
    pub subtitle_transition_elapsed_s: f64,
    pub subtitle_bar_elapsed_s: f64,
    pub subtitle_outgoing_text: String,
    pub subtitle_outgoing_elapsed_s: f64,
    pub subtitle_last_ended_ms: Option<f64>,
    pub pending_shader_events: Vec<SongEventData>,
    pub pending_event_commands: Vec<ScriptCommand>,
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
        let mut player_strumline = Strumline::new(true, scroll_speed, params.note_style.clone());
        player_strumline.is_downscroll = params.downscroll;
        let mut opponent_strumline = Strumline::new(false, scroll_speed, params.note_style);
        opponent_strumline.is_downscroll = params.downscroll;
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
            cutscene_dialogue_active: false,
            cutscene_video_active: false,
            cutscene_dialogue_speaker: String::new(),
            cutscene_dialogue_box: String::new(),
            is_practice_mode: params.practice_mode,
            is_bot_play_mode: params.bot_play_mode,
            is_minimal_mode: params.minimal_mode,
            loop_song: params.loop_song,
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
            player_strumline,
            opponent_strumline,
            note_kinds: NoteKindManager::default(),
            song_events: Vec::new(),
            scroll_speed_tweens: Vec::new(),
            input_press_queue: VecDeque::new(),
            input_release_queue: VecDeque::new(),
            countdown_ticks_remaining: 4,
            countdown_timer_s: 0.0,
            countdown_paused: false,
            pending_countdown_step: None,
            pending_miss_sfx: false,
            pending_popups: Vec::new(),
            popups_visible: true,
            popup_offset: [0.0, 0.0],
            score_text: String::new(),
            custom_score_text: None,
            hud_visible: true,
            hud_alpha: 1.0,
            hud_alpha_tween: None,
            master_volume: 1.0,
            strums_swapped: false,
            pending_hooks: Vec::new(),
            pending_song_event_reverts: HashMap::new(),
            next_song_event_seq: 0,
            last_song_end_popped: None,
            script_char_positions: BTreeMap::new(),
            script_char_top_left: BTreeMap::new(),
            script_char_z: BTreeMap::new(),
            script_char_scales: BTreeMap::new(),
            script_char_alphas: BTreeMap::new(),
            script_char_flips: BTreeMap::new(),
            script_char_vis: BTreeMap::new(),
            script_char_sizes: BTreeMap::new(),
            script_char_focus: BTreeMap::new(),
            script_char_anims: BTreeMap::new(),
            script_char_frames: BTreeMap::new(),
            script_char_anim_finished: BTreeMap::new(),
            script_prop_positions: BTreeMap::new(),
            script_prop_scales: BTreeMap::new(),
            script_prop_angles: BTreeMap::new(),
            script_prop_alphas: BTreeMap::new(),
            script_prop_z: BTreeMap::new(),
            script_prop_scrolls: BTreeMap::new(),
            script_prop_flips: BTreeMap::new(),
            script_prop_vis: BTreeMap::new(),
            script_prop_sizes: BTreeMap::new(),
            script_prop_velocities: BTreeMap::new(),
            script_prop_anims: BTreeMap::new(),
            script_prop_anim_done: BTreeMap::new(),
            script_texts: BTreeMap::new(),
            script_text_positions: BTreeMap::new(),
            script_text_alphas: BTreeMap::new(),
            script_active_sounds: Vec::new(),
            script_sound_volumes: BTreeMap::new(),
            script_keys_down: Vec::new(),
            script_keys_hit: Vec::new(),
            script_keys_up: Vec::new(),
            script_ctrls_down: Vec::new(),
            script_ctrls_hit: Vec::new(),
            script_ctrls_up: Vec::new(),
            status_message: String::new(),
            last_hud_key: (i64::MIN, u32::MAX, u32::MAX, i32::MIN),
            last_raw_audio_ms: None,
            audio_finished: false,
            song_end_canceled: false,
            subtitle_cues: Vec::new(),
            subtitle_config: SubtitleConfig::default(),
            subtitle_default_config: SubtitleConfig::default(),
            subtitle_elapsed_ms: 0.0,
            subtitle_time_ms: 0.0,
            subtitle_sync_to_song: false,
            subtitle_text: String::new(),
            subtitle_active_index: None,
            subtitle_prev_index: None,
            subtitle_exiting_index: None,
            subtitle_next_index: None,
            subtitle_transition_elapsed_s: 1.0,
            subtitle_bar_elapsed_s: 0.0,
            subtitle_outgoing_text: String::new(),
            subtitle_outgoing_elapsed_s: 1.0,
            subtitle_last_ended_ms: None,
            pending_shader_events: Vec::new(),
            pending_event_commands: Vec::new(),
        };
        state.create();
        state
    }

    pub fn create(&mut self) {
        self.generate_song();
        self.start_countdown();
        self.update_score_text();
    }

    /// Updates HUD visibility, using a short alpha fade during active play or outro while
    /// keeping explicit cutscene hides (`is_in_cutscene` / `countdown_paused`) immediate.
    pub fn set_hud_visible(&mut self, visible: bool, immediate: bool) {
        let target = if visible { 1.0_f32 } else { 0.0_f32 };
        let cutscene_hide = !visible && (self.is_in_cutscene || self.countdown_paused);
        let duration_s = if immediate || cutscene_hide {
            0.0
        } else {
            HUD_FADE_DURATION_S
        };
        self.fade_hud(target, duration_s, Some(EaseKind::QuadOut));
    }

    /// Fades HUD alpha toward `alpha` over `duration_s` using `ease` (`None` snaps immediately).
    pub fn fade_hud(&mut self, alpha: f32, duration_s: f64, ease: Option<EaseKind>) {
        if !alpha.is_finite() || !duration_s.is_finite() || duration_s < 0.0 {
            return;
        }
        let target = alpha.clamp(0.0, 1.0);
        self.hud_visible = target > 0.001;
        let Some(ease_kind) = ease else {
            self.hud_alpha = target;
            self.hud_alpha_tween = None;
            return;
        };
        if duration_s <= 0.0 || (self.hud_alpha - target).abs() <= 0.001 {
            self.hud_alpha = target;
            self.hud_alpha_tween = None;
            return;
        }
        self.hud_alpha_tween = Some(LinearTween::eased(
            f64::from(self.hud_alpha),
            f64::from(target),
            duration_s,
            ease_kind,
        ));
    }

    pub fn enter_loading_phase(&mut self) {
        self.phase = PlayPhase::Loading;
        self.is_in_countdown = false;
        self.countdown_paused = false;
        self.starting_song = true;
        self.countdown_ticks_remaining = 4;
        self.countdown_timer_s = 0.0;
        self.pending_countdown_step = None;
        self.last_raw_audio_ms = None;
        self.status_message = "Loading assets...".to_string();
    }

    pub fn start_countdown(&mut self) {
        self.is_in_countdown = true;
        self.countdown_paused = false;
        self.starting_song = true;
        self.last_raw_audio_ms = None;
        self.phase = PlayPhase::Countdown;
        self.countdown_ticks_remaining = 4;
        self.countdown_timer_s = 0.0;
        self.pending_countdown_step = Some(0);
        self.player_strumline.fade_in_arrows();
        self.opponent_strumline.fade_in_arrows();
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

    pub fn update(&mut self, elapsed_s: f64, audio_time_ms: Option<f64>, playlist: &mut PlayStatePlaylist) {
        if self.critical_failure
            || matches!(
                self.phase,
                PlayPhase::Loading | PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
            )
        {
            return;
        }

        if self.needs_reset {
            self.reset_song();
            return;
        }

        self.update_health_bar(elapsed_s);
        self.update_score_text();
        self.advance_tweens(elapsed_s);
        self.update_subtitles(elapsed_s);

        let tick = self.sync_song_clock(elapsed_s, audio_time_ms);

        if self.is_in_countdown && !self.countdown_paused {
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
        self.emit_pending_incoming_notes();

        if tick.step_changed {
            self.step_hit();
        }
        if tick.beat_changed {
            self.beat_hit();
        }

        self.process_song_events();
        self.process_input_queue();
        self.process_notes(elapsed_s);

        if !self.is_in_cutscene && self.health <= HEALTH_MIN && !self.is_practice_mode && !self.is_player_dying {
            self.move_to_game_over();
            return;
        }

        if !self.is_in_countdown
            && !self.is_in_cutscene
            && self.conductor.song_position_ms >= self.chart.length_ms
            && self.audio_finished
            && !self.song_end_canceled
        {
            if self.loop_song {
                self.pending_hooks.push((
                    HookKind::SongEnd,
                    ScriptEvent::new(HookKind::SongEnd)
                        .with_field("score", self.song_score)
                        .with_field("misses", i64::from(self.tallies.missed)),
                ));
                self.needs_reset = true;
            } else {
                self.end_song(playlist);
            }
        }
    }

    pub fn reset_song(&mut self) {
        self.needs_reset = false;
        self.is_player_dying = false;
        self.audio_finished = false;
        self.song_end_canceled = false;
        self.clear_subtitles();
        self.countdown_paused = false;
        self.health = HEALTH_STARTING;
        self.health_lerp = HEALTH_STARTING;
        self.song_score = 0.0;
        self.tallies = Tallies::default();
        self.input_press_queue.clear();
        self.input_release_queue.clear();
        self.pending_popups.clear();
        self.pending_song_event_reverts.clear();
        self.custom_score_text = None;
        self.set_hud_visible(true, true);
        self.popups_visible = true;
        self.popup_offset = [0.0, 0.0];
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
        self.script_prop_z.clear();
        self.script_prop_scrolls.clear();
        self.script_prop_flips.clear();
        self.script_prop_vis.clear();
        self.script_prop_sizes.clear();
        self.script_prop_velocities.clear();
        self.script_prop_anims.clear();
        self.script_prop_anim_done.clear();
        self.cancel_all_camera_tweens();
        self.cancel_scroll_speed_tweens();
        self.reset_camera(true, true, true);
        self.camera.angle_deg = DEFAULT_CAMERA_ANGLE_DEG;
        self.camera.hud_angle_deg = DEFAULT_CAMERA_ANGLE_DEG;
        self.camera.bop_intensity = DEFAULT_BOP_INTENSITY;
        self.camera.hud_zoom_intensity = (DEFAULT_BOP_INTENSITY - 1.0) * 2.0;
        self.camera.zoom_rate = DEFAULT_ZOOM_RATE;
        self.camera.zoom_rate_offset = DEFAULT_ZOOM_OFFSET;
        self.camera.flash_timer_s = 0.0;
        self.camera.hud_flash_timer_s = 0.0;
        self.camera.overlay_flash_timer_s = 0.0;
        self.camera.fade_timer_s = 0.0;
        self.camera.fade_hold = false;
        self.camera.hud_fade_timer_s = 0.0;
        self.camera.hud_fade_hold = false;
        self.camera.overlay_fade_timer_s = 0.0;
        self.camera.overlay_fade_hold = false;
        self.player_strumline.clean();
        self.opponent_strumline.clean();
        self.player_strumline.use_custom_render_distance = false;
        self.opponent_strumline.use_custom_render_distance = false;
        self.rebuild_scroll_velocities();
        self.regen_note_data(self.start_timestamp_ms);
        self.pending_shader_events.clear();
        self.pending_event_commands.clear();
        self.pending_hooks
            .push((HookKind::SongRetry, ScriptEvent::new(HookKind::SongRetry)));
        self.start_countdown();
    }

    pub fn pause(&mut self, mode: PauseMode) {
        if !self.may_pause_game
            || matches!(
                self.phase,
                PlayPhase::Loading | PlayPhase::GameOver | PlayPhase::Ended
            )
        {
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

    fn reset_subtitle_playback_state(&mut self) {
        self.subtitle_elapsed_ms = 0.0;
        self.subtitle_time_ms = 0.0;
        self.subtitle_text.clear();
        self.subtitle_active_index = None;
        self.subtitle_prev_index = None;
        self.subtitle_exiting_index = None;
        self.subtitle_next_index = None;
        self.subtitle_transition_elapsed_s = self.subtitle_config.transition_duration_s.max(1.0);
        self.subtitle_bar_elapsed_s = 0.0;
        self.subtitle_outgoing_text.clear();
        self.subtitle_outgoing_elapsed_s = self.subtitle_config.exit_duration_s.max(1.0);
        self.subtitle_last_ended_ms = None;
    }

    pub fn load_subtitles(&mut self, cues: Vec<SubtitleCue>) {
        self.subtitle_cues = cues;
        self.subtitle_sync_to_song = false;
        self.reset_subtitle_playback_state();
    }

    pub fn load_song_subtitles(&mut self, cues: Vec<SubtitleCue>) {
        self.subtitle_cues = cues;
        self.subtitle_sync_to_song = true;
        self.reset_subtitle_playback_state();
    }

    pub fn load_subtitles_from_text(&mut self, raw: &str, path_hint: Option<&str>, sync_to_song: bool) {
        let cues = parse_subtitles_auto(raw, path_hint, &self.subtitle_config);
        self.subtitle_cues = cues;
        self.subtitle_sync_to_song = sync_to_song;
        self.reset_subtitle_playback_state();
    }

    pub fn apply_subtitle_config_patch(&mut self, patch: &SubtitleConfigPatch) {
        self.subtitle_config.apply_patch(patch);
    }

    pub fn reset_subtitle_config(&mut self) {
        self.subtitle_config = self.subtitle_default_config.clone();
    }

    pub fn clear_subtitles(&mut self) {
        self.subtitle_cues.clear();
        self.subtitle_sync_to_song = false;
        self.reset_subtitle_playback_state();
    }

    fn update_subtitles(&mut self, elapsed_s: f64) {
        let dt = elapsed_s.max(0.0);
        if self.subtitle_cues.is_empty() {
            if !self.subtitle_text.is_empty() {
                self.subtitle_text.clear();
            }
            self.subtitle_active_index = None;
            self.subtitle_prev_index = None;
            self.subtitle_exiting_index = None;
            self.subtitle_next_index = None;
            return;
        }

        let cur_ms = if self.subtitle_sync_to_song {
            self.conductor.song_position_ms
        } else {
            self.subtitle_elapsed_ms += dt * 1000.0 * self.playback_rate.max(0.0);
            self.subtitle_elapsed_ms
        };
        self.subtitle_time_ms = cur_ms;

        // Find primary active cue (last cue whose window covers cur_ms)
        let mut active_idx: Option<usize> = None;
        let mut text = String::new();
        for (idx, cue) in self.subtitle_cues.iter().enumerate() {
            if cur_ms >= cue.start_ms && cur_ms <= cue.end_ms {
                active_idx = Some(idx);
                if !text.is_empty() {
                    text.push('\n');
                }
                text.push_str(&cue.text);
            }
        }

        // Track bar changes for smooth ScrollThreeLine & BounceLetters transitions
        if active_idx != self.subtitle_active_index {
            if let Some(old_idx) = self.subtitle_active_index {
                self.subtitle_outgoing_text = self.subtitle_cues[old_idx].text.clone();
                self.subtitle_outgoing_elapsed_s = 0.0;
                self.subtitle_last_ended_ms = Some(self.subtitle_cues[old_idx].end_ms.min(cur_ms));
            }
            if active_idx.is_some() {
                self.subtitle_exiting_index = self.subtitle_prev_index;
                self.subtitle_transition_elapsed_s = 0.0;
                self.subtitle_bar_elapsed_s = 0.0;
            }
            self.subtitle_active_index = active_idx;
        } else {
            self.subtitle_transition_elapsed_s += dt;
            self.subtitle_bar_elapsed_s += dt;
            self.subtitle_outgoing_elapsed_s += dt;
        }

        // Compute previous and upcoming cue indices for multi-line scroll template
        if let Some(cur) = active_idx {
            self.subtitle_prev_index = cur.checked_sub(1);
            self.subtitle_next_index = if cur + 1 < self.subtitle_cues.len() {
                Some(cur + 1)
            } else {
                None
            };
        } else {
            let mut last_past: Option<usize> = None;
            let mut first_future: Option<usize> = None;
            for (idx, cue) in self.subtitle_cues.iter().enumerate() {
                if cue.end_ms < cur_ms {
                    last_past = Some(idx);
                } else if cue.start_ms > cur_ms {
                    first_future = Some(idx);
                    break;
                }
            }
            self.subtitle_prev_index = last_past;
            self.subtitle_next_index = first_future;

            // Optional idle placeholder between bars (e.g. "...", "♪", or "" for nothing)
            if !self.subtitle_config.idle_text.is_empty() && first_future.is_some() {
                let silence_ms = last_past
                    .map(|idx| cur_ms - self.subtitle_cues[idx].end_ms)
                    .or_else(|| self.subtitle_last_ended_ms.map(|t| cur_ms - t))
                    .unwrap_or(cur_ms);
                if silence_ms >= self.subtitle_config.gap_threshold_ms {
                    text = self.subtitle_config.idle_text.clone();
                }
            }
        }

        if self.subtitle_text != text {
            self.subtitle_text = text;
        }
    }

    pub fn end_song(&mut self, playlist: &mut PlayStatePlaylist) {
        self.song_end_canceled = false;
        self.phase = PlayPhase::Ended;
        self.last_song_end_popped = None;
        if playlist.is_story_mode {
            let delta = self.song_score.round() as i32;
            playlist.campaign_score += delta;
            if !playlist.playlist_song_ids.is_empty() {
                let removed = playlist.playlist_song_ids.remove(0);
                self.last_song_end_popped = Some((removed, delta));
            } else {
                self.last_song_end_popped = Some((String::new(), delta));
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

    fn emit_pending_incoming_notes(&mut self) {
        let player_idxs: Vec<usize> = self.player_strumline.pending_incoming_notes.drain(..).collect();
        let opp_idxs: Vec<usize> = self.opponent_strumline.pending_incoming_notes.drain(..).collect();
        for (is_player, idxs) in [(true, player_idxs), (false, opp_idxs)] {
            let notes = if is_player {
                &self.player_strumline.notes
            } else {
                &self.opponent_strumline.notes
            };
            let side = if is_player { "player" } else { "opponent" };
            for idx in idxs {
                let Some(n) = notes.get(idx) else {
                    continue;
                };
                self.pending_hooks.push((
                    HookKind::NoteIncoming,
                    ScriptEvent::new(HookKind::NoteIncoming)
                        .with_field("id", n.data.id as i64)
                        .with_field("dir", n.lane.index() as i64)
                        .with_field("time", n.data.time_ms)
                        .with_field("length", n.data.sustain_length_ms)
                        .with_field("kind", n.data.kind.clone().unwrap_or_default())
                        .with_field("is_player", is_player)
                        .with_field("side", side),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::play::types::PlayStateParams;
    use crate::utils::easing::EaseKind;

    use super::PlayState;

    #[test]
    fn hud_visible_fades_out_during_play_but_snaps_on_initial_cutscene() {
        let mut state = PlayState::new(PlayStateParams::default());
        assert!((state.hud_alpha - 1.0).abs() < 1e-5);

        // Simulated `onCountdownStart` cutscene setup hides immediately via explicit cutscene state.
        state.is_in_cutscene = true;
        state.countdown_paused = true;
        state.set_hud_visible(false, false);
        assert!(!state.hud_visible);
        assert!((state.hud_alpha - 0.0).abs() < 1e-5);
        assert!(state.hud_alpha_tween.is_none());

        // Revealing after cutscene fades in smoothly, then hiding during gameplay fades out smoothly.
        state.is_in_cutscene = false;
        state.countdown_paused = false;
        state.set_hud_visible(true, false);
        assert!(state.hud_visible);
        assert!(state.hud_alpha_tween.is_some());
        state.advance_tweens(1.0);
        assert!((state.hud_alpha - 1.0).abs() < 1e-5);

        state.start_song();
        state.set_hud_visible(false, false);
        assert!(!state.hud_visible);
        assert!(state.hud_alpha_tween.is_some());
        state.advance_tweens(0.25);
        assert!(state.hud_alpha > 0.01 && state.hud_alpha < 0.99);
        state.advance_tweens(0.3);
        assert!((state.hud_alpha - 0.0).abs() < 1e-5);
        assert!(state.hud_alpha_tween.is_none());
    }

    #[test]
    fn hud_visible_does_not_snap_during_normal_countdown_without_cutscene() {
        let mut state = PlayState::new(PlayStateParams::default());
        assert!(
            state.starting_song
                && state.is_in_countdown
                && state.countdown_ticks_remaining == 4
        );
        state.set_hud_visible(false, false);
        assert!(!state.hud_visible);
        assert!(state.hud_alpha_tween.is_some());
        assert!((state.hud_alpha - 1.0).abs() < 1e-5);
    }

    #[test]
    fn fade_hud_supports_partial_alpha_and_instant_snap() {
        let mut state = PlayState::new(PlayStateParams::default());
        state.fade_hud(0.4, 1.0, Some(EaseKind::Linear));
        assert!(state.hud_visible);
        assert!(state.hud_alpha_tween.is_some());
        state.advance_tweens(0.5);
        assert!((state.hud_alpha - 0.7).abs() < 1e-4);
        state.advance_tweens(0.5);
        assert!((state.hud_alpha - 0.4).abs() < 1e-5);
        assert!(state.hud_alpha_tween.is_none());

        state.fade_hud(0.0, 0.5, None);
        assert!(!state.hud_visible);
        assert!((state.hud_alpha - 0.0).abs() < 1e-5);
        assert!(state.hud_alpha_tween.is_none());

        state.fade_hud(1.0, 0.0, Some(EaseKind::QuadOut));
        assert!(state.hud_visible);
        assert!((state.hud_alpha - 1.0).abs() < 1e-5);
        assert!(state.hud_alpha_tween.is_none());
    }

    #[test]
    fn start_countdown_retriggers_strumline_fade_in_after_cutscene() {
        let mut state = PlayState::new(PlayStateParams::default());
        // Simulate a 4s cutscene elapsing while HUD was hidden, finishing initial fade-in tweens.
        state.countdown_paused = true;
        for _ in 0..8 {
            state.player_strumline.update_notes(0.5, 0.0, 0.0);
            state.opponent_strumline.update_notes(0.5, 0.0, 0.0);
        }
        assert!((state.player_strumline.receptors[0].alpha - 1.0).abs() < 1e-5);

        state.start_countdown();
        for lane in 0..state.player_strumline.receptors.len() {
            assert!((state.player_strumline.receptors[lane].alpha - 0.0).abs() < 1e-5);
            assert!((state.player_strumline.receptors[lane].y_offset - (-10.0)).abs() < 1e-5);
            assert!((state.opponent_strumline.receptors[lane].alpha - 0.0).abs() < 1e-5);
        }
    }

    #[test]
    fn song_subtitles_sync_to_conductor_position() {
        let mut state = PlayState::new(PlayStateParams::default());
        state.load_song_subtitles(vec![super::super::hud::subtitles::SubtitleCue {
            start_ms: 18_000.0,
            end_ms: 19_200.0,
            text: "That's how you do it!".to_string(),
            segments: Vec::new(),
        }]);
        state.conductor.update(18_500.0);
        state.update_subtitles(0.016);
        assert_eq!(state.subtitle_text, "That's how you do it!");
        assert_eq!(state.subtitle_active_index, Some(0));

        state.conductor.update(19_500.0);
        state.update_subtitles(0.016);
        assert!(state.subtitle_text.is_empty());
        assert_eq!(state.subtitle_active_index, None);
    }

    #[test]
    fn idle_text_shows_between_bars_after_gap_threshold() {
        let mut state = PlayState::new(PlayStateParams::default());
        state.subtitle_config.idle_text = "...".to_string();
        state.subtitle_config.gap_threshold_ms = 500.0;
        state.load_song_subtitles(vec![
            super::super::hud::subtitles::SubtitleCue {
                start_ms: 1_000.0,
                end_ms: 2_000.0,
                text: "Bar one".to_string(),
                segments: Vec::new(),
            },
            super::super::hud::subtitles::SubtitleCue {
                start_ms: 5_000.0,
                end_ms: 6_000.0,
                text: "Bar two".to_string(),
                segments: Vec::new(),
            },
        ]);

        state.conductor.update(1_500.0);
        state.update_subtitles(0.016);
        assert_eq!(state.subtitle_text, "Bar one");

        // 200ms after bar one ended (< 500ms threshold): nothing shown
        state.conductor.update(2_200.0);
        state.update_subtitles(0.016);
        assert!(state.subtitle_text.is_empty());

        // 600ms after bar one ended (>= 500ms threshold): idle placeholder shown
        state.conductor.update(2_600.0);
        state.update_subtitles(0.016);
        assert_eq!(state.subtitle_text, "...");
        assert_eq!(state.subtitle_prev_index, Some(0));
        assert_eq!(state.subtitle_next_index, Some(1));
    }
}
