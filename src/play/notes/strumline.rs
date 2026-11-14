use crate::utils::constants::{
    GHOST_TAP_DELAY_S, NOTE_SPACING, PIXELS_PER_MS, STRUMLINE_X_OFFSET, STRUMLINE_Y_OFFSET,
};

use super::direction::NoteDirection;
use super::note::{ActiveHold, NoteSprite};
use super::noteskin::NoteStyle;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReceptorAnimState {
    Static,
    Press,
    Confirm,
}

#[derive(Debug, Clone)]
pub struct Strumline {
    pub is_player: bool,
    pub is_downscroll: bool,
    pub scroll_speed: f64,
    pub note_spacing_scale: f32,
    pub ghost_tap_enabled: bool,
    pub ghost_tap_timer: f64,
    pub note_style: NoteStyle,
    pub notes: Vec<NoteSprite>,
    pub held_lanes: [bool; 4],
    pub active_holds: [Option<ActiveHold>; 4],
    pub lane_confirm_timer: [f64; 4],
    pub lane_press_timer: [f64; 4],
    pub anim_state: [ReceptorAnimState; 4],
}

impl Strumline {
    pub fn new(is_player: bool, scroll_speed: f64, note_style: NoteStyle) -> Self {
        Self {
            is_player,
            is_downscroll: false,
            scroll_speed,
            note_spacing_scale: 1.0,
            ghost_tap_enabled: true,
            ghost_tap_timer: 0.0,
            note_style,
            notes: Vec::new(),
            held_lanes: [false; 4],
            active_holds: [None; 4],
            lane_confirm_timer: [0.0; 4],
            lane_press_timer: [0.0; 4],
            anim_state: [ReceptorAnimState::Static; 4],
        }
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
        let spacing = NOTE_SPACING * self.note_spacing_scale;
        let group_w = spacing * 3.0;
        let start_x = if self.is_player {
            640.0 - STRUMLINE_X_OFFSET - group_w - 72.0
        } else {
            -640.0 + STRUMLINE_X_OFFSET + 72.0
        };
        start_x + (lane as f32) * spacing
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
        self.held_lanes = [false; 4];
        self.active_holds = [None; 4];
        self.lane_confirm_timer = [0.0; 4];
        self.lane_press_timer = [0.0; 4];
        self.anim_state = [ReceptorAnimState::Static; 4];
        self.ghost_tap_timer = 0.0;
    }

    pub fn press_key(&mut self, dir: NoteDirection) {
        self.held_lanes[dir.index()] = true;
    }

    pub fn release_key(&mut self, dir: NoteDirection) {
        self.held_lanes[dir.index()] = false;
        self.play_static(dir);
    }

    pub fn play_confirm(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.lane_confirm_timer[i] = self.note_style.confirm_duration_s;
        self.anim_state[i] = ReceptorAnimState::Confirm;
        if self.is_player {
            self.ghost_tap_timer = GHOST_TAP_DELAY_S;
        }
    }

    pub fn hold_confirm(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.lane_confirm_timer[i] = self.note_style.confirm_duration_s;
        self.anim_state[i] = ReceptorAnimState::Confirm;
    }

    pub fn play_press(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.lane_press_timer[i] = self.note_style.press_duration_s;
        self.anim_state[i] = ReceptorAnimState::Press;
    }

    pub fn play_static(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.lane_confirm_timer[i] = 0.0;
        self.lane_press_timer[i] = 0.0;
        self.anim_state[i] = ReceptorAnimState::Static;
    }

    pub fn has_notes_on_screen(&self) -> bool {
        self.notes
            .iter()
            .any(|n| !n.has_been_hit && !n.has_missed && n.may_hit)
    }

    pub fn tick_timers(&mut self, elapsed_s: f64) {
        if !self.has_notes_on_screen() {
            self.ghost_tap_timer = (self.ghost_tap_timer - elapsed_s).max(0.0);
        }
        for i in 0..4 {
            self.lane_confirm_timer[i] = (self.lane_confirm_timer[i] - elapsed_s).max(0.0);
            self.lane_press_timer[i] = (self.lane_press_timer[i] - elapsed_s).max(0.0);

            if self.lane_confirm_timer[i] > 0.0 {
                self.anim_state[i] = ReceptorAnimState::Confirm;
            } else if self.held_lanes[i] || self.lane_press_timer[i] > 0.0 {
                self.anim_state[i] = ReceptorAnimState::Press;
            } else {
                self.anim_state[i] = ReceptorAnimState::Static;
            }
        }
    }
}
