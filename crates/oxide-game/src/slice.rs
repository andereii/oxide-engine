//! M5 phase-1 deterministic bopeebo gameplay simulation.
//!
//! This module is the frozen integration boundary from `docs/M5-CONTRACT.md`
//! §3. It is Bevy-free: it reads the shipped chart/metadata through
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
//! - Only player lanes `0..=3` participate. Chart directions `4..=7` are the
//!   opponent side, which the contract explicitly excludes; input events
//!   outside `0..=3` are ignored rather than converted into ghost presses.
//!   Chart directions outside `0..=7` are invalid data and fail `build`.
//! - `game_over` is terminal for the phase-1 sim. The death check opens the
//!   GameOverSubState boundary (`persistentUpdate = false`), so the detecting
//!   step and every later step process no inputs, notes or holds; phase 1 has
//!   no retry/reset input.
//! - `note_id` is the zero-based index inside the selected difficulty's
//!   player-note list (chart order after filtering the opponent lanes).
//! - `total_notes_hit` counts every judged note, including the 161 ms
//!   hit-path `miss` and window misses (the FNF tally is the accuracy
//!   denominator, not just the non-miss judgements).
//! - A window miss applies the observed PBOT1 miss score (`-100`), the
//!   configured `HEALTH_MISS_BONUS` (`-0.08` absolute) and resets the combo.
//! - A hold ticks once per elapsed step after the hit tick; the final
//!   fractional/grid remainder up to the sustain end is emitted as a last
//!   `hold_tick` immediately before `hold_end`. A release at or after the
//!   sustain end completes the hold (`hold_end`) instead of dropping it.
//! - Ghost presses, hit-path scores and hold penalties are not rounded: the
//!   trace records the observed f64 arithmetic.

use std::path::Path;

use serde::Serialize;
use thiserror::Error;

use oxide_data::Typed;
use oxide_vfs::Vfs;

/// The phase-1 wire schema version (`docs/M5-CONTRACT.md` §2).
const SCHEMA_VERSION: &str = "1";

/// Bopeebo default chart, relative to the caller's assets directory.
const CHART_PATH: &str = "preload/data/songs/bopeebo/bopeebo-chart.json";
/// Bopeebo metadata, relative to the caller's assets directory.
const METADATA_PATH: &str = "preload/data/songs/bopeebo/bopeebo-metadata.json";
/// Upper bound for the two small JSON assets, matching the VFS read contract.
const ASSET_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

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
    Press { sim_ms: u64, direction: i64 },
    Release { sim_ms: u64, direction: i64 },
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

/// The §2 event trace.
///
/// Every variant carries the shared header first, in wire order:
/// `schema`, `kind`, `sim`, `input_ms`, `input_time_us`, `note_id`,
/// `note_direction`, `judgement`, `raw_note_diff`, `note_diff_int`.
/// `serde(untagged)` makes each struct variant serialize as a flat JSON
/// object; the declaration order is the wire order.
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
}

/// Shared §2 header values used by the private constructors below.
#[derive(Debug, Clone, Copy)]
struct Header {
    sim: u64,
    input_ms: f64,
    note_id: u32,
    note_direction: i64,
    judgement: &'static str,
    raw_note_diff: f64,
    note_diff_int: i32,
}

impl Header {
    fn new(sim: u64) -> Self {
        Self {
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
            schema: SCHEMA_VERSION,
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
    #[error("unknown difficulty {0:?}; the bopeebo slice supports easy, normal and hard")]
    UnknownDifficulty(String),
    #[error("asset {path:?} is missing from the asset layers")]
    MissingAsset { path: &'static str },
    #[error("failed to read asset {path:?}: {source}")]
    Read {
        path: &'static str,
        #[source]
        source: oxide_vfs::VfsError,
    },
    #[error("failed to parse asset {path:?}: {source}")]
    Parse {
        path: &'static str,
        #[source]
        source: oxide_data::DataError,
    },
    #[error("asset {path:?} parsed as {actual:?}, but a {expected} document was required")]
    WrongKind {
        path: &'static str,
        expected: &'static str,
        actual: oxide_data::DataKind,
    },
    #[error("chart {path:?} has an invalid note at index {index}: {detail}")]
    InvalidNote {
        path: &'static str,
        index: usize,
        detail: &'static str,
    },
    #[error("input event at {event_ms} ms does not belong to the active {step_ms} ms sim tick")]
    OutOfGridEvent { event_ms: u64, step_ms: u64 },
}

/// Deterministic player-side simulation state for one bopeebo difficulty.
pub struct GameplaySimState {
    notes: Vec<SimNote>,
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
    last_release_ms: [Option<u64>; 4],
    active_holds: [Option<ActiveHold>; 4],
    trace: Vec<TraceEvent>,
}

#[derive(Debug, Clone, Copy)]
struct SimNote {
    id: u32,
    /// Strum time in milliseconds, exactly as stored on the wire.
    time: f64,
    direction: i64,
    /// Sustain length in milliseconds (`0.0` for a tap).
    sustain_length: f64,
    hit: bool,
    missed: bool,
    may_hit: bool,
}

#[derive(Debug, Clone, Copy)]
struct ActiveHold {
    note_index: usize,
    start_ms: f64,
    end_ms: f64,
    last_tick_ms: f64,
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
    /// Builds the player-land bopeebo slice sim for `difficulty`.
    ///
    /// `assets_root` is the assets directory supplied by the caller (for the
    /// shipped game, `<checkout>/assets`). Chart and metadata are resolved
    /// through [`Vfs`] at `preload/data/songs/bopeebo/...`, then parsed with
    /// `oxide-data` so the M1 schemas stay authoritative.
    pub fn build(assets_root: &Path, difficulty: &str) -> Result<Self, SliceError> {
        if !matches!(difficulty, "easy" | "normal" | "hard") {
            return Err(SliceError::UnknownDifficulty(difficulty.to_owned()));
        }

        let vfs = Vfs::new(assets_root.to_path_buf());
        let chart_bytes = read_asset(&vfs, CHART_PATH)?;
        let metadata_bytes = read_asset(&vfs, METADATA_PATH)?;

        let chart_document =
            oxide_data::parse_file(Path::new(CHART_PATH), &chart_bytes).map_err(|source| {
                SliceError::Parse {
                    path: CHART_PATH,
                    source,
                }
            })?;
        let Typed::Chart(chart) = chart_document.typed else {
            return Err(SliceError::WrongKind {
                path: CHART_PATH,
                expected: "chart",
                actual: chart_document.kind,
            });
        };

        let metadata_document = oxide_data::parse_file(Path::new(METADATA_PATH), &metadata_bytes)
            .map_err(|source| SliceError::Parse {
            path: METADATA_PATH,
            source,
        })?;
        let Typed::Metadata(metadata) = metadata_document.typed else {
            return Err(SliceError::WrongKind {
                path: METADATA_PATH,
                expected: "metadata",
                actual: metadata_document.kind,
            });
        };

        // Metadata is required input, not decoration: when it declares the
        // difficulty list, an unknown selection fails closed.
        if let Some(play_data) = &metadata.play_data
            && !play_data.difficulties.is_empty()
            && !play_data
                .difficulties
                .iter()
                .any(|known| known == difficulty)
        {
            return Err(SliceError::UnknownDifficulty(difficulty.to_owned()));
        }

        let chart_notes = chart
            .notes
            .get(difficulty)
            .ok_or_else(|| SliceError::UnknownDifficulty(difficulty.to_owned()))?;

        let mut notes = Vec::with_capacity(chart_notes.len());
        for (index, note) in chart_notes.iter().enumerate() {
            // Directions 4..=7 are the opponent side and are excluded from
            // the phase-1 player trace. Anything outside the chart's 0..=7
            // lane range is invalid data rather than an ignorable side.
            if !(0..=7).contains(&note.direction) {
                return Err(SliceError::InvalidNote {
                    path: CHART_PATH,
                    index,
                    detail: "note direction must be in 0..=7",
                });
            }
            if !(0..=3).contains(&note.direction) {
                continue;
            }
            if !note.time.is_finite() || note.time < 0.0 {
                return Err(SliceError::InvalidNote {
                    path: CHART_PATH,
                    index,
                    detail: "note time must be finite and non-negative",
                });
            }
            let sustain_length = note.sustain.unwrap_or(0.0);
            if !sustain_length.is_finite() || sustain_length < 0.0 {
                return Err(SliceError::InvalidNote {
                    path: CHART_PATH,
                    index,
                    detail: "sustain length must be finite and non-negative",
                });
            }
            notes.push(SimNote {
                id: notes.len() as u32,
                time: note.time,
                direction: note.direction,
                sustain_length,
                hit: false,
                missed: false,
                may_hit: false,
            });
        }

        let mut state = Self {
            notes,
            health: HEALTH_STARTING,
            song_score: 0.0,
            combo: 0,
            combo_max: 0,
            tallies: TallyCounts::default(),
            death_count: 0,
            dead: false,
            held: [false; 4],
            last_release_ms: [None; 4],
            active_holds: [None; 4],
            trace: Vec::new(),
        };
        // The first processed frame always follows a previous window pass in
        // the native loop; prime the state at position 0 so a chart note at
        // t=0 can be hit by the first grid press.
        state.prime_window_states();
        Ok(state)
    }

    /// One sim tick: clamp + death check, then the input events, then the
    /// window/process-notes updates. Returns only the events dispatched during
    /// this tick, in dispatch order.
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
        // processInputQueue/processNotes.
        self.health = self.health.clamp(HEALTH_MIN, HEALTH_MAX);
        if self.health <= HEALTH_MIN {
            self.dead = true;
            self.death_count += 1;
            let game_over = trace_event!(
                GameOver,
                "game_over",
                Header::new(sim_ms),
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

        let mut dispatched = Vec::new();

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
            let header = Header::new(step_ms)
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

    fn release(&mut self, event_ms: u64, direction: i64) {
        let Some(lane) = player_lane(direction) else {
            return;
        };
        self.held[lane] = false;
        self.last_release_ms[lane] = Some(event_ms);
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
        let note = self.notes[index];
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

        let header = Header::new(step_ms)
            .with_input(event_ms)
            .with_note(note.id, note.direction)
            .with_diff(judgement.as_str(), raw_note_diff, note_diff_int);
        dispatched.push(trace_event!(
            Hit,
            "hit",
            header,
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
            self.active_holds[lane] = Some(ActiveHold {
                note_index: index,
                start_ms: step_ms as f64,
                end_ms: note.time + note.sustain_length,
                last_tick_ms: step_ms as f64,
            });
            dispatched.push(trace_event!(
                HoldStart,
                "hold_start",
                Header::new(step_ms)
                    .with_input(event_ms)
                    .with_note(note.id, note.direction),
            ));
        }
    }

    fn process_notes(&mut self, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let position = sim_ms as f64;
        for index in 0..self.notes.len() {
            let note = self.notes[index];
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

    fn miss_window_note(&mut self, index: usize, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let note = self.notes[index];
        let raw_note_diff = sim_ms as f64 - note.time;
        let note_diff_int = raw_note_diff.trunc() as i32;
        let health_before = self.health;
        let combo_before = self.combo;

        self.notes[index].missed = true;
        self.notes[index].may_hit = false;
        self.health += HEALTH_MISS_BONUS;
        self.song_score += f64::from(PBOT1_MISS_SCORE);
        self.tallies.missed += 1;
        self.tallies.total_notes_hit += 1;
        self.combo = 0;

        let header = Header::new(sim_ms)
            .with_note(note.id, note.direction)
            .with_diff("miss", raw_note_diff, note_diff_int);
        dispatched.push(trace_event!(
            Miss,
            "miss",
            header,
            score_delta: PBOT1_MISS_SCORE,
            health_before: health_before,
            health_after: self.health,
            combo_before: combo_before,
            combo_after: self.combo,
            combo_max: self.combo_max,
            tallies: self.tallies,
        ));
    }

    fn process_hold_lane(&mut self, lane: usize, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        let Some(mut hold) = self.active_holds[lane] else {
            return;
        };
        let position = sim_ms as f64;

        if !self.held[lane] {
            let remaining_ms = hold.end_ms - position;
            if remaining_ms <= 0.0 {
                self.finish_hold(hold, sim_ms, dispatched);
            } else {
                let note = self.notes[hold.note_index];
                let score_penalty = if remaining_ms > HOLD_DROP_PENALTY_THRESHOLD_MS {
                    remaining_ms / 1000.0 * SCORE_HOLD_DROP_PENALTY_PER_SECOND
                } else {
                    0.0
                };
                let health_change = remaining_ms / 1000.0 * HEALTH_HOLD_DROP_PENALTY_PER_SECOND;
                self.song_score += score_penalty;
                self.health += health_change;
                let combo_before = self.combo;
                self.combo = 0;
                self.active_holds[lane] = None;

                let input_ms = self.last_release_ms[lane].unwrap_or(sim_ms);
                let header = Header::new(sim_ms)
                    .with_input(input_ms)
                    .with_note(note.id, note.direction);
                dispatched.push(trace_event!(
                    HoldDrop,
                    "hold_drop",
                    header,
                    score_penalty: score_penalty,
                    health_change: health_change,
                    combo_before: combo_before,
                    combo_after: self.combo,
                    is_combo_break: true,
                ));
            }
            return;
        }

        if position >= hold.end_ms {
            self.finish_hold(hold, sim_ms, dispatched);
            return;
        }

        if position > hold.last_tick_ms {
            let delta_ms = position - hold.last_tick_ms;
            hold.last_tick_ms = position;
            self.active_holds[lane] = Some(hold);
            self.emit_hold_tick(hold, sim_ms, delta_ms, position, dispatched);
        }
    }

    fn finish_hold(&mut self, mut hold: ActiveHold, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        if hold.last_tick_ms < hold.end_ms {
            let delta_ms = hold.end_ms - hold.last_tick_ms;
            hold.last_tick_ms = hold.end_ms;
            self.emit_hold_tick(hold, sim_ms, delta_ms, hold.end_ms, dispatched);
        }
        for lane in 0..4 {
            if self.active_holds[lane].is_some_and(|active| {
                active.note_index == hold.note_index && active.start_ms == hold.start_ms
            }) {
                self.active_holds[lane] = None;
            }
        }
        let note = self.notes[hold.note_index];
        dispatched.push(trace_event!(
            HoldEnd,
            "hold_end",
            Header::new(sim_ms).with_note(note.id, note.direction),
        ));
    }

    fn emit_hold_tick(
        &mut self,
        hold: ActiveHold,
        sim_ms: u64,
        delta_ms: f64,
        position_ms: f64,
        dispatched: &mut Vec<TraceEvent>,
    ) {
        let elapsed_seconds = delta_ms / 1000.0;
        let health_delta = HEALTH_HOLD_BONUS_PER_SECOND * elapsed_seconds;
        let score_delta = SCORE_HOLD_BONUS_PER_SECOND * elapsed_seconds;
        self.health += health_delta;
        self.song_score += score_delta;
        let note = self.notes[hold.note_index];
        let header = Header::new(sim_ms).with_note(note.id, note.direction);
        dispatched.push(trace_event!(
            HoldTick,
            "hold_tick",
            header,
            health_delta: health_delta,
            score_delta: score_delta,
            hold_remaining_ms: (hold.end_ms - position_ms).max(0.0),
            hold_elapsed_ms: position_ms - hold.start_ms,
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

fn read_asset(vfs: &Vfs, path: &'static str) -> Result<Vec<u8>, SliceError> {
    let source = vfs.resolve(path).ok_or(SliceError::MissingAsset { path })?;
    vfs.read(&source, ASSET_SIZE_LIMIT)
        .map_err(|source| SliceError::Read { path, source })
}

#[cfg(test)]
mod tests;
