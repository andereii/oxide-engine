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
    pub may_hit: bool,
    pub hold_hit: bool,
    pub hold_dropped: bool,
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
            may_hit: false,
            hold_hit: false,
            hold_dropped: false,
            hold_remaining_ms,
        })
    }

    pub fn is_hold_note(&self) -> bool {
        self.data.sustain_length_ms > 0.0
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ActiveHold {
    pub note_id: usize,
    pub lane: NoteDirection,
    pub end_ms: f64,
    pub remaining_ms: f64,
    pub dropped: bool,
}
