//! Story-entry choreography triggers for the schema-3 sim trace
//! (`docs/M6-CONTRACT.md` §14).
//!
//! The shipped content scripts are data, not engine surface: their observed
//! song-start/ending hooks are reimplemented as a private Rust table keyed by
//! the corpus song id. Only [`GameplaySimState::build_chart_events`] consults
//! this table, so the frozen schema-1/2 traces (including bopeebo) stay
//! byte-identical.
//!
//! Rows emitted:
//! - the five week-6 dialogue songs emit one `dialogue:<conversation id>` row
//!   on the first processed tick (song start);
//! - `darnell` emits `can_cutscene` on the first processed tick;
//! - `2hot` emits `ending_video` on the first tick at or after the last chart
//!   note end.
//!
//! This module also owns the private weekend-1 note-kind state shared by the
//! schema-2/3 paths (`docs/M6-CONTRACT.md` §14.1, `2hot.hxc`): the one-second
//! `gunCocked` window opened by a cockgun HIT and the health overrides applied
//! by cockgun/firegun misses. The rules are keyed by the chart note kind and
//! never run on a kind-free stream.
//!
//! [`GameplaySimState::build_chart_events`]: super::GameplaySimState::build_chart_events

use super::{SCHEMA_V3, TraceEvent};

/// Native `weekend-1-cockgun` hit/miss kind (`2hot.hxc`).
const COCKGUN_KIND: &str = "weekend-1-cockgun";
/// Native `weekend-1-firegun` hit/miss kinds (`2hot.hxc` handles all three).
const FIREGUN_KINDS: [&str; 3] = [
    "weekend-1-firegun",
    "weekend-1-firegun-hip",
    "weekend-1-firegun-far",
];
/// `new FlxTimer().start(1.0, ...)`: the cocked window in milliseconds.
const GUN_COCK_WINDOW_MS: f64 = 1000.0;
/// Native `HEALTH_LOSS = 0.25 * 2` from `takeCanDamage()`.
const CAN_MISS_HEALTH_LOSS: f64 = -0.5;

/// The outcome of applying the weekend-1 hit rules to one player note.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Weekend1Hit {
    /// The normal hit path (scoring, health, combo and the `Hit` row).
    Normal,
    /// Native `event.cancelEvent()`: the note is consumed with no row and no
    /// score/health/combo/tally side effects.
    Cancelled,
}

/// Per-song weekend-1 note-kind state (schema "2"/"3" only).
///
/// `2hot` is the only shipped chart that carries these kinds; a stream with
/// no kinds never mutates the state and stays byte-identical.
#[derive(Debug, Clone, Default)]
pub(super) struct Weekend1State {
    /// `gunCocked` with the native 1 s `FlxTimer` deadline. `None` means the
    /// gun is not cocked (the timer fired or a firegun hit/miss cleared it).
    gun_cocked_until_ms: Option<f64>,
}

impl Weekend1State {
    /// Applies the hit rule for `kind` at `sim_ms`.
    ///
    /// A cockgun hit opens the 1 s window. A firegun hit inside it takes the
    /// normal hit path and closes the window; outside it the native script
    /// cancels the event. `lightcan`/`kickcan`/`kneecan` hits are timing
    /// anchors with no gameplay effect here.
    pub(super) fn on_hit(&mut self, kind: Option<&str>, sim_ms: u64) -> Weekend1Hit {
        let Some(kind) = kind else {
            return Weekend1Hit::Normal;
        };
        if kind == COCKGUN_KIND {
            self.gun_cocked_until_ms = Some(sim_ms as f64 + GUN_COCK_WINDOW_MS);
            return Weekend1Hit::Normal;
        }
        if is_firegun(kind) {
            // The window is half-open: at exactly the timer deadline the
            // native `FlxTimer` has already set `gunCocked = false`.
            let cocked = self
                .gun_cocked_until_ms
                .is_some_and(|until_ms| (sim_ms as f64) < until_ms);
            self.gun_cocked_until_ms = None;
            if !cocked {
                return Weekend1Hit::Cancelled;
            }
        }
        Weekend1Hit::Normal
    }

    /// The health delta replacing the normal miss bonus for `kind`.
    ///
    /// Cockgun misses zero the health change; firegun misses zero it too and
    /// then apply `takeCanDamage()` (-0.5). The normal miss score/combo rules
    /// are unchanged. `None` means the default miss bonus applies.
    pub(super) fn on_miss(&mut self, kind: Option<&str>) -> Option<f64> {
        let kind = kind?;
        if kind == COCKGUN_KIND {
            return Some(0.0);
        }
        if is_firegun(kind) {
            self.gun_cocked_until_ms = None;
            return Some(CAN_MISS_HEALTH_LOSS);
        }
        None
    }
}

fn is_firegun(kind: &str) -> bool {
    FIREGUN_KINDS.contains(&kind)
}

/// One trigger row queued by the song table.
struct Trigger {
    event: String,
    arg: Option<String>,
}

/// Per-song trigger state. `dispatch` runs before the chart-event walker on
/// every processed tick.
pub(super) struct ChoreographyState {
    start_triggers: Vec<Trigger>,
    ending_tick: Option<u64>,
    emitted_start: bool,
    emitted_ending: bool,
}

impl ChoreographyState {
    /// Builds the table entry for `song`/`variant`, or `None` when the song
    /// has no 6E choreography hook.
    ///
    /// `last_note_end_ms` is the maximum `time + sustain` over the selected
    /// difficulty's chart notes (both sides); the 2hot hook fires on the
    /// first 1 ms grid tick at or after that boundary.
    pub(super) fn build(
        song: &str,
        variant: &str,
        last_note_end_ms: f64,
    ) -> Option<ChoreographyState> {
        let mut start_triggers = Vec::new();
        let mut ending_tick = None;

        match song {
            "senpai" => {
                let conversation = if variant == "pico" {
                    "senpai-pico"
                } else {
                    "senpai"
                };
                start_triggers.push(Trigger {
                    event: format!("dialogue:{conversation}"),
                    arg: None,
                });
            }
            "roses" => {
                let conversation = if variant == "pico" {
                    "roses-pico"
                } else {
                    "roses"
                };
                start_triggers.push(Trigger {
                    event: format!("dialogue:{conversation}"),
                    arg: None,
                });
            }
            "thorns" => start_triggers.push(Trigger {
                event: "dialogue:thorns".to_string(),
                arg: None,
            }),
            "darnell" => start_triggers.push(Trigger {
                event: "can_cutscene".to_string(),
                arg: None,
            }),
            "2hot" => {
                // The ending video follows the song, so its hook is the last
                // note end tick rather than the first dispatch tick.
                let end = last_note_end_ms.max(0.0);
                if end.is_finite() {
                    ending_tick = Some(end.ceil() as u64);
                }
            }
            _ => return None,
        }

        Some(ChoreographyState {
            start_triggers,
            ending_tick,
            emitted_start: false,
            emitted_ending: false,
        })
    }

    /// Emits the pending rows for `sim_ms` in the native song-start/ending
    /// hook order (start first, then the ending hook if both are due).
    pub(super) fn dispatch(&mut self, sim_ms: u64, dispatched: &mut Vec<TraceEvent>) {
        if !self.emitted_start {
            self.emitted_start = true;
            for trigger in &self.start_triggers {
                dispatched.push(choreography_row(sim_ms, trigger));
            }
        }
        if !self.emitted_ending
            && self
                .ending_tick
                .is_some_and(|ending_tick| sim_ms >= ending_tick)
        {
            self.emitted_ending = true;
            dispatched.push(choreography_row(
                sim_ms,
                &Trigger {
                    event: "ending_video".to_string(),
                    arg: None,
                },
            ));
        }
    }
}

fn choreography_row(sim: u64, trigger: &Trigger) -> TraceEvent {
    TraceEvent::Choreography {
        schema: SCHEMA_V3,
        kind: "choreography",
        sim,
        input_ms: 0.0,
        input_time_us: 0.0,
        event: trigger.event.clone(),
        arg: trigger.arg.clone(),
    }
}
