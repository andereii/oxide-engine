use std::collections::BTreeMap;

use super::value::ScriptValue;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookKind {
    Create,
    Destroy,
    Update,
    SongLoaded,
    SongStart,
    CountdownStart,
    CountdownStep,
    CountdownEnd,
    StepHit,
    BeatHit,
    MeasureHit,
    NoteIncoming,
    NoteSpawn,
    NoteHit,
    OpponentNoteHit,
    NoteMiss,
    GhostMiss,
    HoldDrop,
    SongEvent,
    KeyPress,
    KeyRelease,
    Pause,
    Resume,
    FocusGained,
    FocusLost,
    SubstateOpen,
    SubstateClose,
    DialogueStart,
    DialogueLine,
    DialogueEnd,
    GameOver,
    SongEnd,
    SongRetry,
    Custom(String),
}

impl HookKind {
    /// Returns candidate function names checked in priority order:
    /// short modder-friendly aliases first, then explicit snake_case, then V-Slice camelCase.
    pub fn candidate_names(&self) -> Vec<&str> {
        match self {
            Self::Create => vec!["create", "on_create", "onCreate"],
            Self::Destroy => vec!["destroy", "on_destroy", "onDestroy"],
            Self::Update => vec!["tick", "update", "on_update", "onUpdate"],
            Self::SongLoaded => vec!["song_loaded", "on_song_loaded", "onSongLoaded"],
            Self::SongStart => vec!["song_start", "on_song_start", "onSongStart"],
            Self::CountdownStart => vec!["countdown_start", "on_countdown_start", "onCountdownStart"],
            Self::CountdownStep => vec!["countdown_step", "on_countdown_step", "onCountdownStep"],
            Self::CountdownEnd => vec!["countdown_end", "on_countdown_end", "onCountdownEnd"],
            Self::StepHit => vec!["step", "on_step", "on_step_hit", "onStepHit"],
            Self::BeatHit => vec!["beat", "on_beat", "on_beat_hit", "onBeatHit"],
            Self::MeasureHit => vec!["measure", "on_measure", "on_measure_hit", "onMeasureHit"],
            Self::NoteIncoming => vec!["note_incoming", "on_note_incoming", "onNoteIncoming"],
            Self::NoteSpawn => vec!["note_spawn", "on_note_spawn", "onNoteSpawn"],
            Self::NoteHit => vec!["hit", "on_hit", "on_note_hit", "onNoteHit"],
            Self::OpponentNoteHit => vec!["opp_hit", "on_opp_hit", "on_opponent_hit", "onOpponentNoteHit"],
            Self::NoteMiss => vec!["miss", "on_miss", "on_note_miss", "onNoteMiss"],
            Self::GhostMiss => vec!["ghost_miss", "on_ghost_miss", "onNoteGhostMiss"],
            Self::HoldDrop => vec!["hold_drop", "on_hold_drop", "onNoteHoldDrop", "onHoldNoteDrop"],
            Self::SongEvent => vec!["event", "on_event", "on_song_event", "onSongEvent"],
            Self::KeyPress => vec!["key_press", "on_key_press", "onKeyPress", "onKeyDown"],
            Self::KeyRelease => vec!["key_release", "on_key_release", "onKeyRelease", "onKeyUp"],
            Self::Pause => vec!["pause", "on_pause", "onPause"],
            Self::Resume => vec!["resume", "on_resume", "onResume"],
            Self::FocusGained => vec!["focus_in", "on_focus_in", "on_focus_gained", "onFocusGained"],
            Self::FocusLost => vec!["focus_out", "on_focus_out", "on_focus_lost", "onFocusLost"],
            Self::SubstateOpen => vec!["substate_open", "on_substate_open", "onSubStateOpenBegin", "onSubStateOpenEnd"],
            Self::SubstateClose => vec!["substate_close", "on_substate_close", "onSubStateCloseBegin", "onSubStateCloseEnd"],
            Self::DialogueStart => vec!["dlg_start", "on_dialogue_start", "onDialogueStart"],
            Self::DialogueLine => vec!["dlg_line", "on_dialogue_line", "onDialogueLine"],
            Self::DialogueEnd => vec!["dlg_end", "on_dialogue_end", "onDialogueEnd"],
            Self::GameOver => vec!["game_over", "on_game_over", "onGameOver"],
            Self::SongEnd => vec!["song_end", "on_song_end", "onSongEnd"],
            Self::SongRetry => vec!["retry", "on_retry", "on_song_retry", "onSongRetry"],
            Self::Custom(name) => vec![name.as_str()],
        }
    }

    pub fn canonical_name(&self) -> &str {
        match self {
            Self::Create => "on_create",
            Self::Destroy => "on_destroy",
            Self::Update => "on_update",
            Self::SongLoaded => "on_song_loaded",
            Self::SongStart => "on_song_start",
            Self::CountdownStart => "on_countdown_start",
            Self::CountdownStep => "on_countdown_step",
            Self::CountdownEnd => "on_countdown_end",
            Self::StepHit => "on_step",
            Self::BeatHit => "on_beat",
            Self::MeasureHit => "on_measure",
            Self::NoteIncoming => "on_note_incoming",
            Self::NoteSpawn => "on_note_spawn",
            Self::NoteHit => "on_hit",
            Self::OpponentNoteHit => "on_opp_hit",
            Self::NoteMiss => "on_miss",
            Self::GhostMiss => "on_ghost_miss",
            Self::HoldDrop => "on_hold_drop",
            Self::SongEvent => "on_event",
            Self::KeyPress => "on_key_press",
            Self::KeyRelease => "on_key_release",
            Self::Pause => "on_pause",
            Self::Resume => "on_resume",
            Self::FocusGained => "on_focus_gained",
            Self::FocusLost => "on_focus_lost",
            Self::SubstateOpen => "on_substate_open",
            Self::SubstateClose => "on_substate_close",
            Self::DialogueStart => "on_dialogue_start",
            Self::DialogueLine => "on_dialogue_line",
            Self::DialogueEnd => "on_dialogue_end",
            Self::GameOver => "on_game_over",
            Self::SongEnd => "on_song_end",
            Self::SongRetry => "on_retry",
            Self::Custom(name) => name.as_str(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScriptEvent {
    pub kind: HookKind,
    pub canceled: bool,
    pub propagation_stopped: bool,
    pub data: BTreeMap<String, ScriptValue>,
}

impl ScriptEvent {
    pub fn new(kind: HookKind) -> Self {
        Self {
            kind,
            canceled: false,
            propagation_stopped: false,
            data: BTreeMap::new(),
        }
    }

    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<ScriptValue>) -> Self {
        self.data.insert(key.into(), value.into());
        self
    }

    pub fn cancel(&mut self) {
        self.canceled = true;
    }

    pub fn stop_propagation(&mut self) {
        self.propagation_stopped = true;
    }

    pub fn get(&self, key: &str) -> Option<&ScriptValue> {
        self.data.get(key)
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<ScriptValue>) {
        self.data.insert(key.into(), value.into());
    }

    pub fn get_i64(&self, key: &str) -> Option<i64> {
        self.data.get(key).and_then(ScriptValue::as_i64)
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.data.get(key).and_then(ScriptValue::as_f64)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.data.get(key).and_then(ScriptValue::as_bool)
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.data.get(key).and_then(ScriptValue::as_str)
    }
}
