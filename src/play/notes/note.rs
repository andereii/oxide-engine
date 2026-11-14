use crate::utils::constants::HIT_WINDOW_MS;

use super::direction::NoteDirection;

#[derive(Debug, Clone)]
pub struct SongNoteData {
    pub id: usize,
    pub time_ms: f64,
    pub direction: usize,
    pub sustain_length_ms: f64,
    pub kind: Option<String>,
    pub low_priority: bool,
    pub scoreable: bool,
}

#[derive(Debug, Clone)]
pub struct NoteSprite {
    pub data: SongNoteData,
    pub lane: NoteDirection,
    pub has_been_hit: bool,
    pub has_missed: bool,
    pub handled_miss: bool,
    pub too_early: bool,
    pub may_hit: bool,
    pub killed: bool,
    pub desaturated: bool,
    pub alpha: f32,
    pub y_offset: f32,
    pub hold_hit: bool,
    pub hold_dropped: bool,
    pub hold_handled_miss: bool,
    pub hold_killed: bool,
    pub hold_remaining_ms: f64,
}

impl NoteSprite {
    pub fn from_data(data: SongNoteData) -> Option<Self> {
        let lane = NoteDirection::from_index(data.direction % 4)?;
        let hold_remaining_ms = data.sustain_length_ms.max(0.0);
        Some(Self {
            data,
            lane,
            has_been_hit: false,
            has_missed: false,
            handled_miss: false,
            too_early: false,
            may_hit: false,
            killed: false,
            desaturated: false,
            alpha: 1.0,
            y_offset: 0.0,
            hold_hit: false,
            hold_dropped: false,
            hold_handled_miss: false,
            hold_killed: false,
            hold_remaining_ms,
        })
    }

    pub fn is_hold_note(&self) -> bool {
        self.data.sustain_length_ms > 0.0
    }

    pub fn is_head_alive(&self) -> bool {
        !self.killed && (!self.has_been_hit || self.desaturated)
    }

    pub fn is_hold_alive(&self) -> bool {
        self.is_hold_note() && !self.hold_killed
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveHold {
    pub note_idx: usize,
    pub note_id: usize,
    pub lane: NoteDirection,
    pub end_ms: f64,
    pub remaining_ms: f64,
    pub dropped: bool,
    pub handled_miss: bool,
    pub scoreable: bool,
    pub hit_base: bool,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HitWindowResult {
    pub botplay_hit: bool,
    pub cont: bool,
}

pub fn process_note_window(
    note: &mut NoteSprite,
    song_pos_ms: f64,
    is_controlled: bool,
) -> HitWindowResult {
    if note.has_missed || note.has_been_hit || note.killed {
        return HitWindowResult {
            botplay_hit: false,
            cont: false,
        };
    }

    let center = note.data.time_ms;
    let start = center - HIT_WINDOW_MS;
    let end = center + HIT_WINDOW_MS;

    if song_pos_ms > end {
        note.too_early = false;
        note.has_missed = true;
        note.may_hit = false;
        if note.is_hold_note() {
            note.hold_dropped = true;
        }
        return HitWindowResult {
            botplay_hit: false,
            cont: true,
        };
    }

    if !is_controlled && song_pos_ms >= center {
        return HitWindowResult {
            botplay_hit: true,
            cont: true,
        };
    }

    if note.is_hold_note() {
        note.hold_dropped = false;
    }

    if song_pos_ms >= start {
        note.too_early = false;
        note.has_missed = false;
        note.may_hit = true;
    } else {
        note.too_early = true;
        note.may_hit = false;
        note.has_missed = false;
    }

    HitWindowResult {
        botplay_hit: false,
        cont: true,
    }
}
