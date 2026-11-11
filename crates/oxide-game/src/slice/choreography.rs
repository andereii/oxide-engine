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
//! [`GameplaySimState::build_chart_events`]: super::GameplaySimState::build_chart_events

use super::{SCHEMA_V3, TraceEvent};

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
