//! M5 phase-1 / M6 phase-6A deterministic gameplay simulation.
//!
//! This module is the frozen integration boundary from `docs/M5-CONTRACT.md`
//! §3, extended by the add-only schema "2" rules from `docs/M6-CONTRACT.md`
//! §10. It is Bevy-free: it reads the shipped chart/metadata through
//! `oxide-data` + `oxide-vfs`, advances on an explicit millisecond grid and
//! emits the §2 event trace. The conductor clock stays in `oxide-audio`; this
//! sim consumes the already-derived `sim_ms` position instead of duplicating
//! time math.
//!
//! Engine decisions documented where the frozen contract leaves a private
//! detail open:
//!
//! - `sim_ms` is the song position in milliseconds; one [`GameplaySimState::step`]
//!   is one grid tick. The canonical harness calls it once per millisecond.
//! - [`GameplaySimState::build`] keeps the frozen M5 player-only schema "1"
//!   path (bopeebo default chart). [`GameplaySimState::build_chart`] selects a
//!   corpus-style chart id and emits schema "2" rows for BOTH sides.
//! - Player lanes `0..=3` accept input. Chart directions `4..=7` are the
//!   opponent side: they never accept input (opponent-lane presses are
//!   ignored rather than converted into ghost presses) and, in schema "2",
//!   they auto-hit on the grid. Chart directions outside `0..=7` are invalid
//!   data and fail `build`/`build_chart`.
//! - `game_over` is terminal for the sim. The death check opens the
//!   GameOverSubState boundary (`persistentUpdate = false`), so the detecting
//!   step and every later step process no inputs, notes, opponent hits or
//!   holds; there is no retry/reset input.
//! - `note_id` is the zero-based index inside the selected difficulty's
//!   player-note list (chart order after filtering the opponent lanes). The
//!   schema-2 opponent rows use an INDEPENDENT zero-based index over that
//!   difficulty's opponent-note list (lanes `4..=7` in chart array order);
//!   the two id spaces never shift one another.
//! - Opponent notes auto-hit at the first tick whose song position is
//!   `>= note.time` (the 1 ms grid), dispatch a literal `"perfect"` row, and
//!   never mutate score, health, combo or tallies (no opponent miss rows
//!   exist). Their rows are recorded before the player rows produced by the
//!   same tick's `processNotes` loops; player rows dispatched by
//!   `processInputQueue` precede them, per the frozen frame order.
//! - Chart ids follow the `corpus.txt` spelling (`<song>-default`,
//!   `<song>-<variant>`; a bare `<song>` means the default variation) and
//!   resolve `<song>-chart[<variant>].json` / `<song>-metadata[<variant>].json`
//!   under `preload/data/songs/<song>/` through the VFS.
//! - `total_notes_hit` counts every judged note reached through the hit path,
//!   including the 161 ms hit-path `miss`; window misses bump only the `missed`
//!   tally (the real `applyScore` on a windowed miss never touches
//!   `totalNotesHit`).
//! - A window miss applies the observed PBOT1 miss score (`-100`), the
//!   configured `HEALTH_MISS_BONUS` (`-0.08` absolute) and resets the combo.
//! - Hold rows follow `tools/oracle/NOTE-BEHAVIOR.md` §6.1 REVISED. Each tick
//!   first runs a mirror with `P = T-1`: drop detect, completion when the
//!   previous snapshot is `<= 0`, keep-clipped, then recompute
//!   `snapshot = end - P`. `processNotes` then accrues a fixed 1 ms
//!   `hold_tick` (0.00012 health / 0.25 score) whenever the mirror snapshot is
//!   positive, recording that observed snapshot (two full rows at the hit tick
//!   and the first recompute, then the decay). A drop row is emitted on the
//!   same mirror tick that flags it and uses the stale snapshot; only a stale
//!   value strictly over 160 ms penalizes and resets combo. `hold_end` is the
//!   first integer tick `>= end + 2`; no fractional partial row exists.
//! - Ghost presses, hit-path scores and hold penalties are not rounded: the
//!   trace records the observed f64 arithmetic.

use std::path::Path;

use serde::Serialize;
use thiserror::Error;

use oxide_data::Typed;
use oxide_vfs::Vfs;

mod choreography;
mod events;
use choreography::ChoreographyState;
use events::EventState;

/// The frozen phase-1 player-only wire schema (`docs/M5-CONTRACT.md` §2).
const SCHEMA_V1: &str = "1";
/// The add-only phase-6A wire schema (`docs/M6-CONTRACT.md` §10.2).
const SCHEMA_V2: &str = "2";
/// The add-only phase-6C wire schema (`docs/M6-CONTRACT.md` §12.4.1): chart
/// song-event rows. The event-enabled build path stamps every row "3"; the
/// frozen "1"/"2" paths are unchanged.
const SCHEMA_V3: &str = "3";

/// Upper bound for the two small JSON assets, matching the VFS read contract.
const ASSET_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

/// The default `corpus.txt` chart id. The legacy [`GameplaySimState::build`]
/// path resolves this id explicitly instead of carrying module path constants.
const BOPEEBO_CHART_ID: &str = "bopeebo-default";

// Observed gameplay constants (docs/M5-CONTRACT.md §1.2, §1.3).
const HIT_WINDOW_MS: f64 = 160.0;
const HEALTH_MIN: f64 = 0.0;
const HEALTH_MAX: f64 = 2.0;
const HEALTH_STARTING: f64 = 1.0;
const HEALTH_SICK_BONUS: f64 = 1.5 / 100.0 * HEALTH_MAX;
const HEALTH_GOOD_BONUS: f64 = 0.75 / 100.0 * HEALTH_MAX;
const HEALTH_BAD_BONUS: f64 = 0.0;
const HEALTH_SHIT_BONUS: f64 = -1.0 / 100.0 * HEALTH_MAX;
const HEALTH_MISS_BONUS: f64 = -4.0 / 100.0 * HEALTH_MAX;
const HEALTH_GHOST_MISS_PENALTY: f64 = -4.0 / 100.0 * HEALTH_MAX;
const HEALTH_HOLD_BONUS_PER_SECOND: f64 = 6.0 / 100.0 * HEALTH_MAX;
/// Fixed per-tick branch delta for the frozen 1 ms recipe
/// (`tools/oracle/NOTE-BEHAVIOR.md` §6.1 REVISED item 4): one branch entry with
/// `elapsed = 0.001` seconds gives exactly `0.00012` health and `0.25` score.
/// Multiplication (not division) mirrors the real `PER_SECOND * elapsed`.
const HOLD_TICK_ELAPSED_SECONDS: f64 = 0.001;
const HEALTH_HOLD_BONUS_PER_MS: f64 = HEALTH_HOLD_BONUS_PER_SECOND * HOLD_TICK_ELAPSED_SECONDS;
const SCORE_HOLD_BONUS_PER_MS: f64 = SCORE_HOLD_BONUS_PER_SECOND * HOLD_TICK_ELAPSED_SECONDS;
const HEALTH_HOLD_DROP_PENALTY_PER_SECOND: f64 = 0.0;
const SCORE_GHOST_MISS_PENALTY: f64 = -10.0;
const SCORE_HOLD_BONUS_PER_SECOND: f64 = 250.0;
const SCORE_HOLD_DROP_PENALTY_PER_SECOND: f64 = -125.0;
const HOLD_DROP_PENALTY_THRESHOLD_MS: f64 = 160.0;

const PBOT1_MAX_SCORE: i32 = 500;
const PBOT1_MISS_SCORE: i32 = -100;
const PBOT1_SCORING_OFFSET: f64 = 54.99;
const PBOT1_SCORING_SLOPE: f64 = 0.080;
const PBOT1_MIN_SCORE: f64 = 9.0;
const PBOT1_PERFECT_THRESHOLD: f64 = 5.0;
const PBOT1_SICK_THRESHOLD: f64 = 45.0;
const PBOT1_GOOD_THRESHOLD: f64 = 90.0;
const PBOT1_BAD_THRESHOLD: f64 = 135.0;
const PBOT1_SHIT_THRESHOLD: f64 = 160.0;

/// One key-press/release transition consumed by the sim.
///
/// The M4 replay producer converts its own `StreamEvent` values into this
/// type (ceil on the 1 ms grid, same-tick press before release); the game
/// crate never owns the M4 CLI or stream types.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Press {
        sim_ms: u64,
        direction: i64,
    },
    Release {
        sim_ms: u64,
        direction: i64,
    },
    /// M6-CONTRACT §14: the `CUTSCENE_ADVANCE` transition ([Z, ENTER]).
    ///
    /// The gameplay sim has no gameplay effect for it (the dialogue state
    /// machine lives in the scene); accepting the variant keeps one shared
    /// input surface for the play window and the deterministic scene.
    Advance {
        sim_ms: u64,
    },
}

/// Per-judgement tallies in the §2 wire order.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TallyCounts {
    pub sick: u32,
    pub good: u32,
    pub bad: u32,
    pub shit: u32,
    pub missed: u32,
    pub total_notes_hit: u32,
}

/// The §2 event trace, extended add-only with the schema "2" opponent row.
///
/// Every variant carries the shared header first, in wire order:
/// `schema`, `kind`, `sim`, `input_ms`, `input_time_us`, `note_id`,
/// `note_direction`, `judgement`, `raw_note_diff`, `note_diff_int`.
/// `serde(untagged)` makes each struct variant serialize as a flat JSON
/// object; the declaration order is the wire order.
///
/// Schema "1" rows come only from the frozen [`GameplaySimState::build`]
/// path. Schema "2" rows come from [`GameplaySimState::build_chart`];
/// [`TraceEvent::OpponentHit`] is schema "2"-only and carries no
/// score/health/combo/tallies fields.
///
/// No `Eq` derive: several fields are `f64`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum TraceEvent {
    Hit {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        /// Chart note kind (`k`), serialized only when present so kind-free
        /// fixture streams stay byte-identical (M6-CONTRACT §14.2).
        #[serde(skip_serializing_if = "Option::is_none")]
        note_kind: Option<String>,
        score_delta: i32,
        health_before: f64,
        health_after: f64,
        combo_before: u32,
        combo_after: u32,
        combo_max: u32,
        tallies: TallyCounts,
    },
    Miss {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        /// Chart note kind (`k`), present only when the chart declares one.
        #[serde(skip_serializing_if = "Option::is_none")]
        note_kind: Option<String>,
        score_delta: i32,
        health_before: f64,
        health_after: f64,
        combo_before: u32,
        combo_after: u32,
        combo_max: u32,
        tallies: TallyCounts,
    },
    GhostPress {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        score_delta: f64,
        health_change: f64,
    },
    HoldStart {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
    },
    HoldTick {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        health_delta: f64,
        score_delta: f64,
        hold_remaining_ms: f64,
        hold_elapsed_ms: f64,
    },
    HoldDrop {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        score_penalty: f64,
        health_change: f64,
        combo_before: u32,
        combo_after: u32,
        is_combo_break: bool,
    },
    HoldEnd {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
    },
    GameOver {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        death_count: u32,
    },
    /// Opponent-side auto-hit (`docs/M6-CONTRACT.md` §10.2): the shared
    /// header only, no player scoring/health/combo/tallies fields.
    OpponentHit {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        note_id: u32,
        note_direction: i64,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
        /// Chart note kind (`k`), present only when the chart declares one.
        #[serde(skip_serializing_if = "Option::is_none")]
        note_kind: Option<String>,
    },
    /// `FocusCamera` (`docs/M6-CONTRACT.md` §12.4.1).
    CameraFocus {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        target_char: i64,
        target_kind: &'static str,
        x: f64,
        y: f64,
        world_focus: [f64; 2],
        /// Duration in steps (the native duration unit).
        duration: f64,
        ease_kind: &'static str,
        /// Resolved `FlxEase` name for a named ease, else null.
        ease: Option<String>,
        /// `In`/`Out`/`InOut`/`` for a named ease, else null.
        ease_dir: Option<String>,
    },
    /// `ZoomCamera` (`docs/M6-CONTRACT.md` §12.4.1).
    CameraZoom {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        zoom: f64,
        mode: &'static str,
        resolved_target_zoom: f64,
        /// Duration in steps (the native duration unit).
        duration: f64,
        ease: Option<String>,
        ease_dir: Option<String>,
    },
    /// `SetCameraBop` (`docs/M6-CONTRACT.md` §12.4.1).
    CameraBop {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        intensity: f64,
        camera_bop_intensity: f64,
        hud_intensity: f64,
        rate: f64,
        offset: f64,
    },
    /// `PlayAnimation` (`docs/M6-CONTRACT.md` §12.4.1/§12.4.3).
    PlayAnimation {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        target: String,
        anim: String,
        force: bool,
        resolved_kind: String,
        /// The >1000 ms `processOldEvents = false` skip; no state change.
        skipped_old: bool,
    },
    /// One `ScrollSpeed` target strumline (`docs/M6-CONTRACT.md` §12.4.1).
    ScrollSpeed {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        strumline: &'static str,
        target: f64,
        absolute: bool,
        base_scroll: f64,
        /// Duration in steps (the native duration unit).
        duration: f64,
        ease: Option<String>,
        ease_dir: Option<String>,
    },
    /// `SetHealthIcon` (`docs/M6-CONTRACT.md` §12.4.1); record-only for 6C.
    HealthIcon {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        char: i64,
        id: String,
        should_bop: bool,
        scale: f64,
        flip_x: bool,
        is_pixel: bool,
        offsets: [f64; 2],
    },
    /// EnableMask / sserafim / any no-handler kind: record-only
    /// (`docs/M6-CONTRACT.md` §12.2/§12.4.1).
    ///
    /// The raw payload is preserved through `value`; the source event name is
    /// `event_kind` because the shared header already owns `kind`.
    UnknownEvent {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        event_kind: String,
        #[serde(serialize_with = "serialize_event_payload")]
        value: Option<oxide_data::chart::SongEvent>,
    },
    /// Story-entry choreography trigger (`docs/M6-CONTRACT.md` §14.1):
    /// the per-song content table's song-start/ending hooks. `event` is the
    /// trigger name (`dialogue:<conversation id>`, `can_cutscene`,
    /// `ending_video`) and `arg` carries an optional trigger argument.
    Choreography {
        schema: &'static str,
        kind: &'static str,
        sim: u64,
        input_ms: f64,
        input_time_us: f64,
        event: String,
        arg: Option<String>,
    },
}

/// Serializes only the raw `v` payload of the source event
/// (`docs/M6-CONTRACT.md` §12.4.1 `unknown_event.value`).
fn serialize_event_payload<S>(
    value: &Option<oxide_data::chart::SongEvent>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    match value.as_ref().and_then(|event| event.value.as_ref()) {
        Some(payload) => payload.serialize(serializer),
        None => serializer.serialize_none(),
    }
}

/// Shared §2 header values used by the private constructors below.
#[derive(Debug, Clone, Copy)]
struct Header {
    schema: &'static str,
    sim: u64,
    input_ms: f64,
    note_id: u32,
    note_direction: i64,
    judgement: &'static str,
    raw_note_diff: f64,
    note_diff_int: i32,
}

impl Header {
    fn new(sim: u64, schema: &'static str) -> Self {
        Self {
            schema,
            sim,
            input_ms: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "none",
            raw_note_diff: 0.0,
            note_diff_int: 0,
        }
    }

    fn with_input(mut self, input_ms: u64) -> Self {
        self.input_ms = input_ms as f64;
        self
    }

    fn with_note(mut self, note_id: u32, note_direction: i64) -> Self {
        self.note_id = note_id;
        self.note_direction = note_direction;
        self
    }

    fn with_diff(
        mut self,
        judgement: &'static str,
        raw_note_diff: f64,
        note_diff_int: i32,
    ) -> Self {
        self.judgement = judgement;
        self.raw_note_diff = raw_note_diff;
        self.note_diff_int = note_diff_int;
        self
    }
}

/// Builds a trace event from a header plus the variant's extra fields.
///
/// Keeping the shared header in one place guarantees field order and the
/// constant `input_time_us = 0.0` in the canonical trace.
macro_rules! trace_event {
    ($variant:ident, $kind:literal, $header:expr $(, $field:ident : $value:expr)* $(,)?) => {{
        let header = $header;
        TraceEvent::$variant {
            schema: header.schema,
            kind: $kind,
            sim: header.sim,
            input_ms: header.input_ms,
            input_time_us: 0.0,
            note_id: header.note_id,
            note_direction: header.note_direction,
            judgement: header.judgement,
            raw_note_diff: header.raw_note_diff,
            note_diff_int: header.note_diff_int,
            $( $field: $value, )*
        }
    }};
}

/// Errors returned while building the slice sim.
///
/// `OutOfGridEvent` is part of the frozen error sketch; the frozen `step`
/// signature has no error channel, so conversion-side grid validation belongs
/// to the harness and this variant is currently not produced by the sim.
#[derive(Debug, Error)]
pub enum SliceError {
    #[error("unknown difficulty {0:?}; the selected chart's metadata does not declare it")]
    UnknownDifficulty(String),
    #[error("invalid chart id {0:?}; expected <song>, <song>-default or <song>-<variant>")]
    InvalidChartId(String),
    #[error("metadata {path:?} does not declare playData.difficulties")]
    MissingDifficultyList { path: String },
    #[error("asset {path:?} is missing from the asset layers")]
    MissingAsset { path: String },
    #[error("failed to read asset {path:?}: {source}")]
    Read {
        path: String,
        #[source]
        source: oxide_vfs::VfsError,
    },
    #[error("failed to parse asset {path:?}: {source}")]
    Parse {
        path: String,
        #[source]
        source: oxide_data::DataError,
    },
    #[error("asset {path:?} parsed as {actual:?}, but a {expected} document was required")]
    WrongKind {
        path: String,
        expected: &'static str,
        actual: oxide_data::DataKind,
    },
    #[error("chart {path:?} has an invalid note at index {index}: {detail}")]
    InvalidNote {
        path: String,
        index: usize,
        detail: &'static str,
    },
    #[error("input event at {event_ms} ms does not belong to the active {step_ms} ms sim tick")]
    OutOfGridEvent { event_ms: u64, step_ms: u64 },
    #[error("chart event at index {index} is invalid: {detail}")]
    InvalidEvent { index: usize, detail: &'static str },
    #[error("asset {path:?} cannot be used for event focus resolution: {detail}")]
    InvalidAsset { path: String, detail: String },
}

/// Deterministic simulation state for one chart difficulty.
///
/// `notes` holds the compacted player notes (schema "1" and "2");
/// `opponent_notes` holds the independently compacted opponent notes and is
/// empty on the frozen [`GameplaySimState::build`] path.
pub struct GameplaySimState {
    notes: Vec<SimNote>,
    opponent_notes: Vec<SimNote>,
    schema: &'static str,
    health: f64,
    song_score: f64,
    combo: u32,
    combo_max: u32,
    tallies: TallyCounts,
    death_count: u32,
    /// Set when the death check reaches the GameOverSubState boundary. From
    /// then on the play state stops processing; phase 1 has no retry input.
    dead: bool,
    held: [bool; 4],
    active_holds: [Option<ActiveHold>; 4],
    /// Chart-event walker; present only on the schema-3 event-enabled path.
    events: Option<EventState>,
    /// Story-entry trigger table; present only when the selected song has a
    /// 6E choreography entry (never on bopeebo).
    choreography: Option<ChoreographyState>,
    trace: Vec<TraceEvent>,
}

#[derive(Debug, Clone)]
struct SimNote {
    id: u32,
    /// Strum time in milliseconds, exactly as stored on the wire.
    time: f64,
    direction: i64,
    /// Sustain length in milliseconds (`0.0` for a tap).
    sustain_length: f64,
    /// Chart note kind (`k`); `None` on the frozen schema-1 path.
    kind: Option<String>,
    hit: bool,
    missed: bool,
    may_hit: bool,
}

#[derive(Debug, Clone, Copy)]
struct ActiveHold {
    note_index: usize,
    /// Hit tick T0 in milliseconds; the first accrual row is this same tick.
    start_ms: f64,
    /// Base note strum time (the mirror `P > strumTime` guard).
    strum_time: f64,
    /// Full sustain end `strumTime + fullSustainLength`.
    end_ms: f64,
    /// The hold's `sustainLength` field as last set by the mirror/accrual
    /// chain; `hold_tick` rows record this observed value.
    snapshot_ms: f64,
    /// This tick's mirror set the drop flag; the row is emitted in
    /// `processNotes` of the same tick with the stale snapshot.
    dropped: bool,
    /// This tick's mirror saw the previous snapshot `<= 0`; `hold_end` is
    /// emitted in `processNotes` of the same tick.
    completed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Judgement {
    Sick,
    Good,
    Bad,
    Shit,
    Miss,
}

impl Judgement {
    fn as_str(self) -> &'static str {
        match self {
            Self::Sick => "sick",
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Shit => "shit",
            Self::Miss => "miss",
        }
    }

    fn health_change(self) -> f64 {
        match self {
            Self::Sick => HEALTH_SICK_BONUS,
            Self::Good => HEALTH_GOOD_BONUS,
            Self::Bad => HEALTH_BAD_BONUS,
            Self::Shit => HEALTH_SHIT_BONUS,
            Self::Miss => 0.0,
        }
    }

    fn is_combo_break(self) -> bool {
        matches!(self, Self::Bad | Self::Shit)
    }

    fn bump(self, tallies: &mut TallyCounts) {
        match self {
            Self::Sick => tallies.sick += 1,
            Self::Good => tallies.good += 1,
            Self::Bad => tallies.bad += 1,
            Self::Shit => tallies.shit += 1,
            Self::Miss => tallies.missed += 1,
        }
        tallies.total_notes_hit += 1;
    }
}

impl GameplaySimState {
    /// Builds the frozen M5 player-only bopeebo slice sim for `difficulty`.
    ///
    /// This is the byte-frozen schema "1" entry point used by the M5 gates
    /// and the existing play session. It resolves the `bopeebo-default`
    /// corpus id (the module no longer carries bopeebo path constants) but
    /// keeps the player-only trace and the `easy|normal|hard` guard.
    ///
    /// `assets_root` is the assets directory supplied by the caller (for the
    /// shipped game, `<checkout>/assets`). Chart and metadata are resolved
    /// through [`Vfs`] under `preload/data/songs/...`, then parsed with
    /// `oxide-data` so the M1 schemas stay authoritative.
    pub fn build(assets_root: &Path, difficulty: &str) -> Result<Self, SliceError> {
        if !matches!(difficulty, "easy" | "normal" | "hard") {
            return Err(SliceError::UnknownDifficulty(difficulty.to_owned()));
        }
        let selection = chart_paths(BOPEEBO_CHART_ID)?;
        Self::build_selected(assets_root, &selection, difficulty, false, false)
    }

    /// Builds the schema "2" sim for any `corpus.txt`-style chart id.
    ///
    /// Ids resolve as `<song>-default` -> `<song>-chart.json`, and
    /// `<song>-<variant>` -> `<song>-chart-<variant>.json` (metadata uses the
    /// same suffix). A bare `<song>` is the default variation. The difficulty
    /// keys are the metadata `playData.difficulties`; missing ids, malformed
    /// ids, absent files and undeclared difficulties fail closed with
    /// [`SliceError`].
    ///
    /// Unlike [`Self::build`], this path records the opponent-side auto-hit
    /// rows and tags every row with schema "2".
    pub fn build_chart(
        assets_root: &Path,
        chart_id: &str,
        difficulty: &str,
    ) -> Result<Self, SliceError> {
        let selection = chart_paths(chart_id)?;
        Self::build_selected(assets_root, &selection, difficulty, true, false)
    }

    /// Builds the schema "3" sim with chart song-event consumption.
    ///
    /// Same chart-id/difficulty resolution as [`Self::build_chart`], plus the
    /// `docs/M6-CONTRACT.md` §12 event walker: event rows are emitted in the
    /// native per-tick dispatch position and every row is stamped schema "3".
    /// The frozen [`Self::build`] (schema "1") and [`Self::build_chart`]
    /// (schema "2") paths never consume events, so their fixture streams stay
    /// byte-identical.
    pub fn build_chart_events(
        assets_root: &Path,
        chart_id: &str,
        difficulty: &str,
    ) -> Result<Self, SliceError> {
        let selection = chart_paths(chart_id)?;
        Self::build_selected(assets_root, &selection, difficulty, true, true)
    }

    /// Shared chart/metadata loading. `opponent_side` selects the add-only
    /// schema "2" behavior and `event_side` the schema "3" event walker;
    /// both `false` is the frozen M5 player-only path.
    fn build_selected(
        assets_root: &Path,
        selection: &ChartSelection,
        difficulty: &str,
        opponent_side: bool,
        event_side: bool,
    ) -> Result<Self, SliceError> {
        let chart_path = selection.chart_path.as_str();
        let metadata_path = selection.metadata_path.as_str();
        let vfs = Vfs::new(assets_root.to_path_buf());
        let chart_bytes = read_asset(&vfs, chart_path)?;
        let metadata_bytes = read_asset(&vfs, metadata_path)?;

        let chart_document =
            oxide_data::parse_file(Path::new(chart_path), &chart_bytes).map_err(|source| {
                SliceError::Parse {
                    path: chart_path.to_owned(),
                    source,
                }
            })?;
        let Typed::Chart(chart) = chart_document.typed else {
            return Err(SliceError::WrongKind {
                path: chart_path.to_owned(),
                expected: "chart",
                actual: chart_document.kind,
            });
        };

        let metadata_document = oxide_data::parse_file(Path::new(metadata_path), &metadata_bytes)
            .map_err(|source| SliceError::Parse {
            path: metadata_path.to_owned(),
            source,
        })?;
        let Typed::Metadata(metadata) = metadata_document.typed else {
            return Err(SliceError::WrongKind {
                path: metadata_path.to_owned(),
                expected: "metadata",
                actual: metadata_document.kind,
            });
        };

        // The M5 path keeps the frozen permissive rule (an absent/empty
        // difficulty list is tolerated); the schema-2 path requires the
        // metadata difficulty list as its authority.
        let declared_difficulties = metadata
            .play_data
            .as_ref()
            .map_or(&[][..], |play_data| play_data.difficulties.as_slice());
        if opponent_side && declared_difficulties.is_empty() {
            return Err(SliceError::MissingDifficultyList {
                path: metadata_path.to_owned(),
            });
        }
        if !declared_difficulties.is_empty()
            && !declared_difficulties
                .iter()
                .any(|known| known == difficulty)
        {
            return Err(SliceError::UnknownDifficulty(difficulty.to_owned()));
        }

        let chart_notes = chart
            .notes
            .get(difficulty)
            .ok_or_else(|| SliceError::UnknownDifficulty(difficulty.to_owned()))?;

        // The 2hot ending-video hook fires on the last note end tick. Both
        // chart sides count (the video follows the full song).
        let last_note_end_ms = chart_notes.iter().fold(0.0_f64, |last, note| {
            let end = note.time + note.sustain.unwrap_or(0.0);
            if end.is_finite() { last.max(end) } else { last }
        });

        let mut notes = Vec::with_capacity(chart_notes.len());
        let mut opponent_notes = Vec::new();
        for (index, note) in chart_notes.iter().enumerate() {
            // Directions 4..=7 are the opponent side. Anything outside the
            // chart's 0..=7 lane range is invalid data rather than an
            // ignorable side. The M5 path skips the opponent side exactly as
            // the frozen schema "1" trace did (including its validation).
            if !(0..=7).contains(&note.direction) {
                return Err(SliceError::InvalidNote {
                    path: chart_path.to_owned(),
                    index,
                    detail: "note direction must be in 0..=7",
                });
            }
            let opponent = note.direction >= 4;
            if opponent && !opponent_side {
                continue;
            }
            if !note.time.is_finite() || note.time < 0.0 {
                return Err(SliceError::InvalidNote {
                    path: chart_path.to_owned(),
                    index,
                    detail: "note time must be finite and non-negative",
                });
            }
            let sustain_length = note.sustain.unwrap_or(0.0);
            if !sustain_length.is_finite() || sustain_length < 0.0 {
                return Err(SliceError::InvalidNote {
                    path: chart_path.to_owned(),
                    index,
                    detail: "sustain length must be finite and non-negative",
                });
            }
            // Opponent ids are compacted over opponent notes only and the
            // direction is the strumline direction `data % 4`; player ids and
            // directions keep the frozen schema "1" semantics.
            let sim_note = if opponent {
                SimNote {
                    id: opponent_notes.len() as u32,
                    time: note.time,
                    direction: note.direction % 4,
                    sustain_length,
                    kind: if opponent_side {
                        note.kind.clone()
                    } else {
                        None
                    },
                    hit: false,
                    missed: false,
                    may_hit: false,
                }
            } else {
                SimNote {
                    id: notes.len() as u32,
                    time: note.time,
                    direction: note.direction,
                    sustain_length,
                    kind: if opponent_side {
                        note.kind.clone()
                    } else {
                        None
                    },
                    hit: false,
                    missed: false,
                    may_hit: false,
                }
            };
            if opponent {
                opponent_notes.push(sim_note);
            } else {
                notes.push(sim_note);
            }
        }

        // The event walker resolves its focus points/stage zoom from the same
        // VFS data the renderer uses; only the event-enabled path pays for it.
        let events = if event_side {
            Some(EventState::build(&vfs, &metadata, &chart, difficulty)?)
        } else {
            None
        };
        // The story-entry trigger table is schema-3 only: the frozen schema-1
        // and schema-2 paths must stay byte-identical (bopeebo included).
        let choreography = if event_side {
            ChoreographyState::build(&selection.song, &selection.variant, last_note_end_ms)
        } else {
            None
        };

        let mut state = Self {
            notes,
            opponent_notes,
            schema: if event_side {
                SCHEMA_V3
            } else if opponent_side {
                SCHEMA_V2
            } else {
                SCHEMA_V1
            },
            health: HEALTH_STARTING,
            song_score: 0.0,
            combo: 0,
            combo_max: 0,
            tallies: TallyCounts::default(),
            death_count: 0,
            dead: false,
            held: [false; 4],
            active_holds: [None; 4],
            events,
            choreography,
            trace: Vec::new(),
        };
        // The first processed frame always follows a previous window pass in
        // the native loop; prime the state at position 0 so a chart note at
        // t=0 can be hit by the first grid press.
        state.prime_window_states();
        Ok(state)
    }

    /// One sim tick: clamp + death check, then the hold mirror with
    /// `P = T-1`, then the input events, then the window/process-notes
    /// updates (opponent auto-hit rows first inside `processNotes`). Returns
    /// only the events dispatched during this tick, in dispatch order.
    ///
    /// The death check is terminal: the tick that reaches it dispatches the
    /// single `game_over` event after the `moveToGameOver` score/health reset
    /// and processes nothing else, and every later tick returns no events.
    pub fn step(&mut self, sim_ms: u64, events: &[InputEvent]) -> Vec<TraceEvent> {
        // `persistentUpdate = false`: the play state stopped processing when
        // GameOverSubState opened. Phase 1 has no retry/reset input.
        if self.dead {
            return Vec::new();
        }

        // Observed per-frame order: health clamp and death check run before
        // the strumline mirror; the conductor update and processInputQueue
        // follow, then processNotes.
        self.health = self.health.clamp(HEALTH_MIN, HEALTH_MAX);
        if self.health <= HEALTH_MIN {
            self.dead = true;
            self.death_count += 1;
            let game_over = trace_event!(
                GameOver,
                "game_over",
                Header::new(sim_ms, self.schema),
                death_count: self.death_count,
            );
            // moveToGameOver resets the score/health track before the substate
            // opens. This tick's inputs, notes and holds are not processed.
            self.song_score = 0.0;
            self.health = HEALTH_STARTING;
            self.active_holds = [None; 4];
            self.trace.push(game_over.clone());
            return vec![game_over];
        }

        self.mirror_holds(sim_ms);
        let mut dispatched = Vec::new();

        // Story-entry hooks are dispatched on the first processed tick (the
        // song-start boundary) and at the 2hot last-note-end boundary, before
        // the chart song-event rows of the same tick.
        if let Some(choreography) = &mut self.choreography {
            choreography.dispatch(sim_ms, &mut dispatched);
        }

        // Native `processSongEvents` runs after the camera/bop decay block and
        // before `processInputQueue`/`processNotes`; its rows therefore
        // precede this tick's input and note rows.
        if let Some(chart_events) = &mut self.events {
            chart_events.dispatch(sim_ms, &mut dispatched);
        }

        for event in events {
            match event {
                InputEvent::Press {
                    sim_ms: event_ms,
                    direction,
                } => self.press(*event_ms, *direction, sim_ms, &mut dispatched),
                InputEvent::Release {
                    sim_ms: event_ms,
                    direction,
                } => self.release(*event_ms, *direction),
                // M6-CONTRACT §14: the scene owns the dialogue state machine;
                // the deterministic gameplay sim has no advance effect.
                InputEvent::Advance { .. } => {}
            }
        }

        self.process_notes(sim_ms, &mut dispatched);
        self.trace.extend(dispatched.iter().cloned());
        dispatched
    }

    /// The full trace dispatched since construction.
    #[must_use]
    pub fn record(&self) -> Vec<TraceEvent> {
        self.trace.clone()
    }

    fn press(
        &mut self,
        event_ms: u64,
        direction: i64,
        step_ms: u64,
        dispatched: &mut Vec<TraceEvent>,
    ) {
        let Some(lane) = player_lane(direction) else {
            // Opponent-side (4..=7) or invalid directions are outside the
            // phase-1 player trace, not ghost taps.
            return;
        };
        self.held[lane] = true;

        if let Some(note_index) = self.find_may_hit(direction) {
            self.hit_note(note_index, event_ms, step_ms, dispatched);
        } else {
            self.health += HEALTH_GHOST_MISS_PENALTY;
            self.song_score += SCORE_GHOST_MISS_PENALTY;
            let header = Header::new(step_ms, self.schema)
                .with_input(event_ms)
                .with_note(0, direction);
            dispatched.push(trace_event!(
                GhostPress,
                "ghost_press",
                header,
                score_delta: SCORE_GHOST_MISS_PENALTY,
                health_change: HEALTH_GHOST_MISS_PENALTY,
            ));
        }
    }

    fn release(&mut self, _event_ms: u64, direction: i64) {
        let Some(lane) = player_lane(direction) else {
            return;
        };
        self.held[lane] = false;
    }

    fn find_may_hit(&self, direction: i64) -> Option<usize> {
        self.notes
            .iter()
            .enumerate()
            .filter(|(_, note)| {
                !note.hit && !note.missed && note.may_hit && note.direction == direction
            })
            .min_by(|(left_index, left), (right_index, right)| {
                left.time
                    .total_cmp(&right.time)
                    .then_with(|| left_index.cmp(right_index))
            })
            .map(|(index, _)| index)
    }

    fn hit_note(
        &mut self,
        index: usize,
        event_ms: u64,
        step_ms: u64,
        dispatched: &mut Vec<TraceEvent>,
    ) {
        let note = self.notes[index].clone();
        let raw_note_diff = step_ms as f64 - note.time;
        let note_diff_int = raw_note_diff.trunc() as i32;
        let abs_timing = f64::from(note_diff_int).abs();
        let judgement = judge_pbot1(abs_timing);
        let score_delta = score_pbot1(abs_timing);

        let health_before = self.health;
        let combo_before = self.combo;
        judgement.bump(&mut self.tallies);
        self.song_score += f64::from(score_delta);
        self.health += judgement.health_change();
        if judgement.is_combo_break() {
            self.combo = 0;
        } else {
            self.combo += 1;
            self.combo_max = self.combo_max.max(self.combo);
        }
        let combo_after = self.combo;
        let health_after = self.health;

        self.notes[index].hit = true;
        self.notes[index].may_hit = false;

        let header = Header::new(step_ms, self.schema)
            .with_input(event_ms)
            .with_note(note.id, note.direction)
            .with_diff(judgement.as_str(), raw_note_diff, note_diff_int);
        dispatched.push(trace_event!(
            Hit,
            "hit",
            header,
            note_kind: note.kind.clone(),
            score_delta: score_delta,
            health_before: health_before,
            health_after: health_after,
            combo_before: combo_before,
            combo_after: combo_after,
            combo_max: self.combo_max,
            tallies: self.tallies,
        ));

        if note.sustain_length > 0.0 {
            let Some(lane) = player_lane(note.direction) else {
                return;
            };
            // A new sustain in an occupied lane (not present in bopeebo)
            // completes the previous one instead of silently dropping state.
            if let Some(previous) = self.active_holds[lane].take() {
                self.finish_hold(previous, step_ms, dispatched);
            }
            let end_ms = note.time + note.sustain_length;
            // Hit capture: `min(full, end - songPosition_at_hit)`. Recipe hits
            // land on strumTime, so this is `full` (§6.1 REVISED item 4).
            let snapshot_ms = note.sustain_length.min(end_ms - step_ms as f64);
            self.active_holds[lane] = Some(ActiveHold {
                note_index: index,
                start_ms: step_ms as f64,
                strum_time: note.time,
                end_ms,
                snapshot_ms,
                dropped: false,
                completed: false,
            });
            dispatched.push(trace_event!(
                HoldStart,
                "hold_start",
                Header::new(step_ms, self.schema)
                    .with_input(event_ms)
                    .with_note(note.id, note.direction),
            ));
        }
    }

    fn process_notes(&mut self, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        // Native processNotes runs the opponent note loop first. The M5
        // schema "1" path has no opponent notes, so this call is inert there.
        self.process_opponent_notes(sim_ms, dispatched);

        let position = sim_ms as f64;
        for index in 0..self.notes.len() {
            let note = self.notes[index].clone();
            if note.hit || note.missed {
                continue;
            }
            if position > note.time + HIT_WINDOW_MS {
                self.miss_window_note(index, sim_ms, dispatched);
            } else {
                self.notes[index].may_hit = position >= note.time - HIT_WINDOW_MS;
            }
        }

        for lane in 0..4 {
            self.process_hold_lane(lane, sim_ms, dispatched);
        }
    }

    /// Opponent auto-hit: the first tick at or after the note time dispatches
    /// one schema "2" row in chart array order. Opponent rows never mutate
    /// player score/health/combo/tallies and there is no opponent miss path.
    fn process_opponent_notes(&mut self, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let position = sim_ms as f64;
        for index in 0..self.opponent_notes.len() {
            let note = self.opponent_notes[index].clone();
            if note.hit || position < note.time {
                continue;
            }
            self.opponent_notes[index].hit = true;
            dispatched.push(trace_event!(
                OpponentHit,
                "opponent_hit",
                Header::new(sim_ms, self.schema)
                    .with_note(note.id, note.direction)
                    .with_diff("perfect", 0.0, 0),
                note_kind: note.kind.clone(),
            ));
        }
    }

    fn miss_window_note(&mut self, index: usize, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let note = self.notes[index].clone();
        let raw_note_diff = sim_ms as f64 - note.time;
        let note_diff_int = raw_note_diff.trunc() as i32;
        let health_before = self.health;
        let combo_before = self.combo;

        self.notes[index].missed = true;
        self.notes[index].may_hit = false;
        self.health += HEALTH_MISS_BONUS;
        self.song_score += f64::from(PBOT1_MISS_SCORE);
        self.tallies.missed += 1;
        self.combo = 0;

        let header = Header::new(sim_ms, self.schema)
            .with_note(note.id, note.direction)
            .with_diff("miss", raw_note_diff, note_diff_int);
        dispatched.push(trace_event!(
            Miss,
            "miss",
            header,
            note_kind: note.kind.clone(),
            score_delta: PBOT1_MISS_SCORE,
            health_before: health_before,
            health_after: self.health,
            combo_before: combo_before,
            combo_after: self.combo,
            combo_max: self.combo_max,
            tallies: self.tallies,
        ));
    }

    /// Strumline mirror (super-equivalent) using the previous frame's
    /// conductor position `P = T-1`.
    ///
    /// The real chain order is drop detect, completion (using the snapshot
    /// set at T-1), keep-clipped, then recompute. A flagged drop preserves
    /// the stale snapshot; completion kills the hold with no recompute.
    fn mirror_holds(&mut self, sim_ms: u64) {
        if sim_ms == 0 {
            return;
        }
        let mirror_position = sim_ms as f64 - 1.0;
        for lane in 0..4 {
            let Some(mut hold) = self.active_holds[lane] else {
                continue;
            };
            // a. drop detect: snapshot is NOT touched (keep-clipped later).
            if mirror_position > hold.strum_time && !self.held[lane] {
                hold.dropped = true;
            }
            // b. completion sees the snapshot set at frame T-1.
            if hold.snapshot_ms <= 0.0 {
                hold.completed = true;
                self.active_holds[lane] = Some(hold);
                continue;
            }
            // c. keep-clipped branch preserves the snapshot.
            if hold.dropped {
                self.active_holds[lane] = Some(hold);
                continue;
            }
            // d. recompute for an active non-dropped hold.
            if mirror_position > hold.strum_time {
                hold.snapshot_ms = hold.end_ms - mirror_position;
            }
            self.active_holds[lane] = Some(hold);
        }
    }

    fn process_hold_lane(&mut self, lane: usize, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let Some(hold) = self.active_holds[lane] else {
            return;
        };
        if hold.completed {
            self.finish_hold(hold, sim_ms, dispatched);
            return;
        }
        if hold.dropped {
            let note = self.notes[hold.note_index].clone();
            let stale_ms = hold.snapshot_ms;
            let combo_before = self.combo;
            let is_combo_break = stale_ms > HOLD_DROP_PENALTY_THRESHOLD_MS;
            // Spec formula is `-125.0 * snapshot / 1000` (multiply, then
            // divide) so the stale-161 boundary is bitwise `-20.125`.
            let score_penalty = if is_combo_break {
                SCORE_HOLD_DROP_PENALTY_PER_SECOND * stale_ms / 1000.0
            } else {
                0.0
            };
            let health_change = stale_ms / 1000.0 * HEALTH_HOLD_DROP_PENALTY_PER_SECOND;
            self.song_score += score_penalty;
            self.health += health_change;
            if is_combo_break {
                self.combo = 0;
            }
            let combo_after = self.combo;
            self.active_holds[lane] = None;
            dispatched.push(trace_event!(
                HoldDrop,
                "hold_drop",
                // A hold drop is a non-input event: the §2 schema pins 0.0.
                Header::new(sim_ms, self.schema).with_note(note.id, note.direction),
                score_penalty: score_penalty,
                health_change: health_change,
                combo_before: combo_before,
                combo_after: combo_after,
                is_combo_break: is_combo_break,
            ));
            return;
        }
        // The accrual branch observes this tick's mirror snapshot.
        if hold.snapshot_ms > 0.0 {
            self.emit_hold_tick(hold, sim_ms, dispatched);
        }
    }

    fn finish_hold(&mut self, hold: ActiveHold, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        for lane in 0..4 {
            if self.active_holds[lane].is_some_and(|active| {
                active.note_index == hold.note_index && active.start_ms == hold.start_ms
            }) {
                self.active_holds[lane] = None;
            }
        }
        let note = self.notes[hold.note_index].clone();
        dispatched.push(trace_event!(
            HoldEnd,
            "hold_end",
            Header::new(sim_ms, self.schema).with_note(note.id, note.direction),
        ));
    }

    fn emit_hold_tick(&mut self, hold: ActiveHold, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let health_delta = HEALTH_HOLD_BONUS_PER_MS;
        let score_delta = SCORE_HOLD_BONUS_PER_MS;
        self.health += health_delta;
        self.song_score += score_delta;
        let note = self.notes[hold.note_index].clone();
        let header = Header::new(sim_ms, self.schema).with_note(note.id, note.direction);
        dispatched.push(trace_event!(
            HoldTick,
            "hold_tick",
            header,
            health_delta: health_delta,
            score_delta: score_delta,
            hold_remaining_ms: hold.snapshot_ms,
            hold_elapsed_ms: sim_ms as f64 - hold.start_ms,
        ));
    }

    fn prime_window_states(&mut self) {
        for note in &mut self.notes {
            if note.hit || note.missed {
                continue;
            }
            // Equivalent to one processWindow pass at song position 0: the
            // window start of a note at t=0 is -160 ms.
            note.may_hit = 0.0 >= note.time - HIT_WINDOW_MS;
        }
    }
}

/// `Std.int` equivalent: truncation toward zero.
fn judge_pbot1(abs_timing: f64) -> Judgement {
    if abs_timing <= PBOT1_SICK_THRESHOLD {
        Judgement::Sick
    } else if abs_timing <= PBOT1_GOOD_THRESHOLD {
        Judgement::Good
    } else if abs_timing <= PBOT1_BAD_THRESHOLD {
        Judgement::Bad
    } else if abs_timing <= PBOT1_SHIT_THRESHOLD {
        Judgement::Shit
    } else {
        Judgement::Miss
    }
}

fn score_pbot1(abs_timing: f64) -> i32 {
    if abs_timing > PBOT1_SHIT_THRESHOLD {
        PBOT1_MISS_SCORE
    } else if abs_timing < PBOT1_PERFECT_THRESHOLD {
        PBOT1_MAX_SCORE
    } else {
        let curve =
            1.0 - 1.0 / (1.0 + (-PBOT1_SCORING_SLOPE * (abs_timing - PBOT1_SCORING_OFFSET)).exp());
        (f64::from(PBOT1_MAX_SCORE) * curve + PBOT1_MIN_SCORE) as i32
    }
}

fn player_lane(direction: i64) -> Option<usize> {
    usize::try_from(direction).ok().filter(|lane| *lane < 4)
}

/// A `corpus.txt`-style chart id resolved to its file paths and song parts.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ChartSelection {
    song: String,
    variant: String,
    chart_path: String,
    metadata_path: String,
}

/// Maps a `corpus.txt`-style chart id to the chart/metadata paths.
///
/// `<song>-default` and the bare `<song>` shorthand use the unsuffixed file
/// names; `<song>-<variant>` appends `-<variant>` to both. The variant is the
/// trailing `-<variant>` ONLY when `<variant>` is a known corpus variation
/// (`default`/`erect`/`pico`/`bf`); otherwise the whole id is the song name
/// (`philly-nice`, `winter-horrorland`). This keeps hyphenated song names
/// intact for the bare-id shorthand.
fn chart_paths(chart_id: &str) -> Result<ChartSelection, SliceError> {
    // A bare `<song>` (the DEFAULT variant) may itself contain hyphens
    // (`philly-nice`, `winter-horrorland`), so the variant suffix must be one
    // of the KNOWN corpus variations; an unknown/absent suffix is the default.
    const VARIATIONS: [&str; 4] = ["default", "erect", "pico", "bf"];
    let (song, variant) = chart_id
        .rsplit_once('-')
        .filter(|(_, variant)| VARIATIONS.contains(variant))
        .unwrap_or((chart_id, "default"));
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    };
    let song_valid =
        !song.is_empty() && !song.starts_with('-') && !song.ends_with('-') && valid_part(song);
    let variant_valid = valid_part(variant);
    if !song_valid || !variant_valid {
        return Err(SliceError::InvalidChartId(chart_id.to_owned()));
    }

    let suffix = if variant == "default" {
        String::new()
    } else {
        format!("-{variant}")
    };
    let stem = format!("preload/data/songs/{song}/{song}");
    Ok(ChartSelection {
        song: song.to_owned(),
        variant: variant.to_owned(),
        chart_path: format!("{stem}-chart{suffix}.json"),
        metadata_path: format!("{stem}-metadata{suffix}.json"),
    })
}

fn read_asset(vfs: &Vfs, path: &str) -> Result<Vec<u8>, SliceError> {
    let source = vfs.resolve(path).ok_or_else(|| SliceError::MissingAsset {
        path: path.to_owned(),
    })?;
    vfs.read(&source, ASSET_SIZE_LIMIT)
        .map_err(|source| SliceError::Read {
            path: path.to_owned(),
            source,
        })
}

#[cfg(test)]
mod tests;
