use super::direction::NoteDirection;
use super::note::{ActiveHold, NoteSprite};
use super::noteskin::NoteStyle;

#[derive(Debug, Clone)]
pub struct Strumline {
    pub is_player: bool,
    pub scroll_speed: f64,
    pub ghost_tap_enabled: bool,
    pub note_style: NoteStyle,
    pub notes: Vec<NoteSprite>,
    pub held_lanes: [bool; 4],
    pub active_holds: [Option<ActiveHold>; 4],
    pub lane_confirm_timer: [f64; 4],
    pub lane_press_timer: [f64; 4],
}

impl Strumline {
    pub fn new(is_player: bool, scroll_speed: f64, note_style: NoteStyle) -> Self {
        Self {
            is_player,
            scroll_speed,
            ghost_tap_enabled: true,
            note_style,
            notes: Vec::new(),
            held_lanes: [false; 4],
            active_holds: [None; 4],
            lane_confirm_timer: [0.0; 4],
            lane_press_timer: [0.0; 4],
        }
    }

    pub fn clean(&mut self) {
        self.notes.clear();
        self.held_lanes = [false; 4];
        self.active_holds = [None; 4];
        self.lane_confirm_timer = [0.0; 4];
        self.lane_press_timer = [0.0; 4];
    }

    pub fn press_key(&mut self, dir: NoteDirection) {
        self.held_lanes[dir.index()] = true;
    }

    pub fn release_key(&mut self, dir: NoteDirection) {
        self.held_lanes[dir.index()] = false;
    }

    pub fn play_confirm(&mut self, dir: NoteDirection) {
        self.lane_confirm_timer[dir.index()] = self.note_style.confirm_duration_s;
    }

    pub fn play_press(&mut self, dir: NoteDirection) {
        self.lane_press_timer[dir.index()] = self.note_style.press_duration_s;
    }

    pub fn play_static(&mut self, dir: NoteDirection) {
        let i = dir.index();
        self.lane_confirm_timer[i] = 0.0;
        self.lane_press_timer[i] = 0.0;
    }

    pub fn tick_timers(&mut self, elapsed_s: f64) {
        for i in 0..4 {
            self.lane_confirm_timer[i] = (self.lane_confirm_timer[i] - elapsed_s).max(0.0);
            self.lane_press_timer[i] = (self.lane_press_timer[i] - elapsed_s).max(0.0);
        }
    }
}

