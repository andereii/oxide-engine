use bevy::prelude::KeyCode;

use crate::utils::constants::{
    GHOST_TAP_DELAY_S, HIT_WINDOW_MS, NOTE_NUDGE, NOTE_SPACING, PIXELS_PER_MS, SCREEN_HEIGHT,
    STRUMLINE_X_OFFSET, STRUMLINE_Y_OFFSET,
};

use super::direction::NoteDirection;
use super::note::{ActiveHold, NoteSprite};
use super::noteskin::NoteStyle;
use super::receptor::{NoteVibrationStatus, NoteVibrationsState, ReceptorAnimState, ReceptorState};
use super::scroll_velocity::{ScrollLagState, ScrollVelocityTimeline};
pub use super::strumline_window::VwooshTween;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HoldCoverCmd {
    Start(NoteDirection),
    End(NoteDirection),
    Kill(NoteDirection),
}

#[derive(Debug, Clone)]
pub struct Strumline {
    pub is_player: bool,
    pub is_downscroll: bool,
    pub scroll_speed: f64,
    pub base_scroll_speed: f64,
    pub note_spacing_scale: f32,
    pub strumline_scale: f32,
    pub ghost_tap_enabled: bool,
    pub ghost_tap_timer: f64,
    pub show_notesplash: bool,
    pub use_custom_render_distance: bool,
    pub custom_render_distance_ms: f64,
    pub note_style: NoteStyle,
    pub notes: Vec<NoteSprite>,
    pub active_start_index: usize,
    pub next_note_index: usize,
    pub held_keys: [Vec<KeyCode>; 4],
    pub active_holds: [Option<ActiveHold>; 4],
    pub receptors: [ReceptorState; 4],
    pub scroll_velocity: Option<ScrollVelocityTimeline>,
    pub lag_state: ScrollLagState,
    pub visual_song_position: f64,
    pub vibrations: NoteVibrationsState,
    pub pending_splashes: Vec<NoteDirection>,
    pub pending_hold_covers: Vec<HoldCoverCmd>,
    pub vwoosh_tweens: Vec<VwooshTween>,
}

impl Strumline {
    pub fn new(is_player: bool, scroll_speed: f64, note_style: NoteStyle) -> Self {
        Self {
            is_player,
            is_downscroll: false,
            scroll_speed,
            base_scroll_speed: scroll_speed,
            note_spacing_scale: 1.0,
            strumline_scale: 1.0,
            ghost_tap_enabled: true,
            ghost_tap_timer: 0.0,
            show_notesplash: true,
            use_custom_render_distance: false,
            custom_render_distance_ms: 0.0,
            note_style,
            notes: Vec::new(),
            active_start_index: 0,
            next_note_index: 0,
            held_keys: Default::default(),
            active_holds: [None; 4],
            receptors: [
                ReceptorState::new(is_player),
                ReceptorState::new(is_player),
                ReceptorState::new(is_player),
                ReceptorState::new(is_player),
            ],
            scroll_velocity: None,
            lag_state: ScrollLagState::default(),
            visual_song_position: 0.0,
            vibrations: NoteVibrationsState::default(),
            pending_splashes: Vec::new(),
            pending_hold_covers: Vec::new(),
            vwoosh_tweens: Vec::new(),
        }
    }

    pub fn render_distance_ms(&self) -> f64 {
        if self.use_custom_render_distance {
            return self.custom_render_distance_ms;
        }
        let speed = if self.scroll_speed < 1.0 {
            self.scroll_speed.max(0.05)
        } else {
            1.0
        };
        f64::from(SCREEN_HEIGHT) / PIXELS_PER_MS / speed
    }

    pub fn receptor_y(&self) -> f32 {
        let base_from_edge = 360.0 - STRUMLINE_Y_OFFSET - 96.0;
        if self.is_downscroll {
            -base_from_edge
        } else {
            base_from_edge
        }
    }

    pub fn lane_x(&self, lane: usize) -> f32 {
        let spacing = NOTE_SPACING * self.note_spacing_scale * self.strumline_scale;
        let group_w = spacing * 3.0;
        let start_x = if self.is_player {
            640.0 - STRUMLINE_X_OFFSET - group_w - 72.0
        } else {
            -640.0 + STRUMLINE_X_OFFSET + 72.0
        };
        start_x + (lane as f32) * spacing
    }

    pub fn note_x(&self, lane: usize) -> f32 {
        self.lane_x(lane) + self.note_style.notes_offset[0] - NOTE_NUDGE
    }

    pub fn scroll_rate(&self) -> f64 {
        let dir = if self.is_downscroll { 1.0 } else { -1.0 };
        PIXELS_PER_MS * self.scroll_speed * dir
    }

    pub fn note_y_at(&self, note_time_ms: f64, song_pos_ms: f64) -> f32 {
        let distance_ms = note_time_ms - song_pos_ms;
        self.receptor_y() + (distance_ms * self.scroll_rate()) as f32
    }

    pub fn clean(&mut self) {
        self.notes.clear();
        self.active_start_index = 0;
        self.next_note_index = 0;
        self.held_keys = Default::default();
        self.active_holds = [None; 4];
        self.ghost_tap_timer = 0.0;
        self.pending_splashes.clear();
        self.pending_hold_covers.clear();
        self.vwoosh_tweens.clear();
        self.lag_state = ScrollLagState::default();
        self.scroll_speed = self.base_scroll_speed;
        for dir in NoteDirection::ALL {
            self.play_static(dir);
            self.pending_hold_covers.push(HoldCoverCmd::Kill(dir));
        }
    }

    pub fn handle_skipped_notes(&mut self, song_pos_ms: f64) {
        for note in &mut self.notes {
            if !note.has_been_hit && song_pos_ms > note.data.time_ms + HIT_WINDOW_MS {
                note.has_missed = true;
                note.handled_miss = true;
                note.killed = true;
            }
        }
        self.active_start_index = self
            .notes
            .iter()
            .position(|n| !n.killed)
            .unwrap_or(self.notes.len());
        self.next_note_index = self.active_start_index;
        self.active_holds = [None; 4];
        self.lag_state = ScrollLagState::default();
        for dir in NoteDirection::ALL {
            self.pending_hold_covers.push(HoldCoverCmd::Kill(dir));
        }
    }

    pub fn press_key(&mut self, dir: NoteDirection, key: Option<KeyCode>) {
        let lane = &mut self.held_keys[dir.index()];
        match key {
            Some(k) => {
                if !lane.contains(&k) {
                    lane.push(k);
                }
            }
            None => {
                if lane.is_empty() {
                    lane.push(KeyCode::Space);
                }
            }
        }
    }

    pub fn release_key(&mut self, dir: NoteDirection, key: Option<KeyCode>) {
        let lane = &mut self.held_keys[dir.index()];
        match key {
            Some(k) => lane.retain(|&held| held != k),
            None => lane.clear(),
        }
    }

    pub fn is_key_held(&self, dir: NoteDirection) -> bool {
        !self.held_keys[dir.index()].is_empty()
    }

    pub fn play_static(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.receptors[i].play_static();
        if self.is_player {
            self.vibrations.statuses[i] = NoteVibrationStatus::Idle;
        }
    }

    pub fn play_press(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.receptors[i].play_press();
        if self.is_player {
            self.vibrations.statuses[i] = NoteVibrationStatus::Pressed;
        }
    }

    pub fn play_confirm(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.receptors[i].play_confirm();
        if self.is_player {
            self.vibrations.statuses[i] = NoteVibrationStatus::Confirm;
            self.ghost_tap_timer = GHOST_TAP_DELAY_S;
        }
    }

    pub fn hold_confirm(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.receptors[i].hold_confirm();
        if self.is_player {
            self.vibrations.statuses[i] = NoteVibrationStatus::HoldConfirm;
        }
    }

    pub fn anim_state(&self, lane: usize) -> ReceptorAnimState {
        self.receptors[lane].anim_state
    }

    pub fn has_notes_may_hit(&self) -> bool {
        self.active_slice()
            .iter()
            .any(|n| !n.killed && !n.has_been_hit && n.may_hit)
    }

    pub fn has_hold_notes_hit_or_missed(&self) -> bool {
        self.active_holds.iter().any(|h| h.is_some())
            || self
                .active_slice()
                .iter()
                .any(|n| n.is_hold_alive() && (n.hold_hit || n.hold_dropped))
    }

    pub fn has_notes_on_screen(&self) -> bool {
        self.active_slice()
            .iter()
            .any(|n| !n.killed && !n.has_been_hit)
    }

    pub fn may_ghost_tap(&self) -> bool {
        if !self.ghost_tap_enabled {
            return false;
        }
        !self.has_notes_may_hit() && !self.has_hold_notes_hit_or_missed() && self.ghost_tap_timer <= 0.0
    }

    pub fn active_slice(&self) -> &[NoteSprite] {
        let start = self.active_start_index.min(self.notes.len());
        let end = self.next_note_index.min(self.notes.len());
        &self.notes[start..end]
    }
}
