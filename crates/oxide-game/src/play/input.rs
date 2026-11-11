//! Keyboard mapping and the 1 ms input/tick planner.
//!
//! Input is frame-sampled like the native loop: a key transition observed in
//! one Bevy frame is stamped with the conductor tick that the frame will next
//! reach (`floor(song_position_ms)`, raised to the next unstepped tick when the
//! clock has not advanced) and handed to exactly one [`GameplaySimState::step`]
//! call. Press/release message order inside a frame is preserved.
//!
//! WASD and arrows are alternate bindings for the same lane. Physical state is
//! a per-lane count of held bound keys. Every real press is forwarded as a lane
//! press, but a release is queued only when the last bound key goes up
//! (`held -> 0`), so releasing one alias while the other is still held does not
//! release the lane. Pause release synthesis uses the same `count > 0`
//! predicate.
//!
//! [`GameplaySimState::step`]: crate::slice::GameplaySimState::step

use bevy::input::keyboard::KeyCode;

use super::PlayError;
use crate::slice::InputEvent;

/// The frozen ADR-0021 keyboard mapping: WASD/arrows to lanes
/// `{A/Left:0, S/Down:1, W/Up:2, D/Right:3}`.
pub(crate) fn direction_for(key: KeyCode) -> Option<i64> {
    match key {
        KeyCode::KeyA | KeyCode::ArrowLeft => Some(0),
        KeyCode::KeyS | KeyCode::ArrowDown => Some(1),
        KeyCode::KeyW | KeyCode::ArrowUp => Some(2),
        KeyCode::KeyD | KeyCode::ArrowRight => Some(3),
        _ => None,
    }
}

/// Normal lane-event decision: every real press is forwarded, while a release
/// is forwarded only when the per-lane held predicate turns false (the last
/// bound key for that lane went up).
pub(crate) fn should_queue_lane_event(pressed: bool, transition: Option<bool>) -> bool {
    pressed || transition == Some(false)
}

/// Physical keyboard state used by both the normal lane queue and pause
/// release synthesis.
///
/// This deliberately tracks real bound-key transitions, not the sim's held
/// flags. Each lane keeps a count of currently held bound keys (WASD + arrow
/// alternate bindings), so one alias release cannot release the lane while the
/// other is still down.
pub(crate) struct InputRouter {
    held_counts: [u32; 4],
}

impl InputRouter {
    pub(crate) fn new() -> Self {
        Self {
            held_counts: [0; 4],
        }
    }

    /// Records a real key transition, including transitions observed while
    /// paused (the sim queue itself rejects those).
    ///
    /// Returns the physical lane transition: `Some(true)` when the lane becomes
    /// held (`0 -> >0`), `Some(false)` when it becomes unheld (`>0 -> 0`), and
    /// `None` for a redundant alias press/release or an extra release at zero.
    /// Every press is forwarded by the normal path; a release is forwarded only
    /// on `Some(false)`, and pause synthesis uses the held count.
    pub(crate) fn observe(&mut self, direction: i64, pressed: bool) -> Option<bool> {
        let lane = usize::try_from(direction).ok().filter(|lane| *lane < 4)?;
        let before = self.held_counts[lane] > 0;
        if pressed {
            self.held_counts[lane] = self.held_counts[lane].saturating_add(1);
        } else {
            self.held_counts[lane] = self.held_counts[lane].saturating_sub(1);
        }
        let after = self.held_counts[lane] > 0;
        match (before, after) {
            (false, true) => Some(true),
            (true, false) => Some(false),
            _ => None,
        }
    }

    /// Releases for every physically held lane (`count > 0`), in ascending lane
    /// order, stamped on the next unstepped tick.
    pub(crate) fn pause_releases(&self, tick: u64) -> Vec<InputEvent> {
        (0..4)
            .filter(|lane| self.held_counts[*lane] > 0)
            .map(|lane| InputEvent::Release {
                sim_ms: tick,
                direction: lane as i64,
            })
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn is_held(&self, lane: usize) -> bool {
        self.held_counts.get(lane).copied().unwrap_or(0) > 0
    }
}

/// Tick/input state machine shared by the production loop and the pure tests.
pub(crate) struct TickPlanner {
    next_tick: u64,
    pending: Vec<InputEvent>,
    paused: bool,
    terminal: bool,
}

impl TickPlanner {
    pub(crate) fn new() -> Self {
        Self {
            next_tick: 0,
            pending: Vec::new(),
            paused: false,
            terminal: false,
        }
    }

    pub(crate) fn next_tick(&self) -> u64 {
        self.next_tick
    }

    pub(crate) fn is_paused(&self) -> bool {
        self.paused
    }

    #[cfg(test)]
    pub(crate) fn is_terminal(&self) -> bool {
        self.terminal
    }

    /// Toggles pause and returns the new paused state.
    ///
    /// When entering pause, `releases` (synthesized for physically held lanes
    /// and stamped on the next unstepped tick, in lane order) are appended
    /// **first**, then pending input is clamped to release transitions only:
    /// stale presses are dropped while the synthesized and already-queued
    /// releases survive, so the sim un-sticks deterministically at resume.
    pub(crate) fn toggle_pause(&mut self, releases: Vec<InputEvent>) -> bool {
        if self.terminal {
            return self.paused;
        }
        self.paused = !self.paused;
        if self.paused {
            // Queue the synthesized releases before running the pending-input
            // clamp; `retain` must not remove them.
            self.pending.extend(releases);
            self.pending
                .retain(|event| matches!(event, InputEvent::Release { .. }));
        }
        self.paused
    }

    pub(crate) fn finish(&mut self) {
        self.terminal = true;
        self.pending.clear();
    }

    /// Tick that a newly observed key transition belongs to.
    ///
    /// `floor(current_ms)` is the frame's conductor position; when the clock
    /// has not advanced past the last stepped tick (multiple render frames in
    /// one millisecond, or the first frame), the next unstepped tick is used
    /// so the event is never dropped into the past.
    pub(crate) fn stamp_tick(&self, position_ms: f64) -> Result<u64, PlayError> {
        position_tick(position_ms).map(|tick| tick.max(self.next_tick))
    }

    /// Appends one transition. Events are ignored while paused/terminal.
    pub(crate) fn queue(&mut self, tick: u64, direction: i64, pressed: bool) {
        if self.paused || self.terminal {
            return;
        }
        let event = if pressed {
            InputEvent::Press {
                sim_ms: tick,
                direction,
            }
        } else {
            InputEvent::Release {
                sim_ms: tick,
                direction,
            }
        };
        self.pending.push(event);
    }

    /// Appends the `CUTSCENE_ADVANCE` transition ([Z, ENTER]) on `tick`.
    /// Ignored while paused/terminal like lane transitions; no autorepeat is
    /// synthesized by this planner (the window forwards real keydowns only).
    /// Consumed by the 6E conversation slice's play-window wiring
    /// (M6-CONTRACT §14.2); temporary allow until that lands.
    #[allow(dead_code)]
    pub(crate) fn queue_advance(&mut self, tick: u64) {
        if self.paused || self.terminal {
            return;
        }
        self.pending.push(InputEvent::Advance { sim_ms: tick });
    }

    /// Inclusive last tick to step for the current song position.
    ///
    /// `None` means there is no new complete millisecond (paused, terminal, or
    /// a repeated render frame inside the last stepped tick).
    pub(crate) fn target_tick(&self, position_ms: f64) -> Result<Option<u64>, PlayError> {
        if self.paused || self.terminal {
            return Ok(None);
        }
        let target = position_tick(position_ms)?;
        let expected = self.next_tick;
        // A clock that rewinds by more than the current fractional millisecond
        // is an ordering error; `target == expected-1` is the same tick frame.
        if target.saturating_add(1) < expected {
            return Err(PlayError::OutOfOrderTick {
                expected,
                got: target,
            });
        }
        if target < expected {
            return Ok(None);
        }
        Ok(Some(target))
    }

    /// Takes the queued events for `tick`, preserving insertion order.
    ///
    /// Any queued event stamped in the past is a typed ordering error; future
    /// events stay queued for their own tick.
    pub(crate) fn take_inputs(&mut self, tick: u64) -> Result<Vec<InputEvent>, PlayError> {
        let mut current = Vec::new();
        let mut future = Vec::new();
        for event in self.pending.drain(..) {
            let event_ms = match &event {
                InputEvent::Press { sim_ms, .. }
                | InputEvent::Release { sim_ms, .. }
                | InputEvent::Advance { sim_ms } => *sim_ms,
            };
            if event_ms == tick {
                current.push(event);
            } else if event_ms < tick {
                return Err(PlayError::OutOfOrderTick {
                    expected: tick,
                    got: event_ms,
                });
            } else {
                future.push(event);
            }
        }
        self.pending = future;
        Ok(current)
    }

    /// Returns the next unstepped tick when it is within `target`.
    fn next_step_tick(&mut self, target: u64) -> Option<u64> {
        if self.terminal || self.next_tick > target {
            None
        } else {
            let tick = self.next_tick;
            self.next_tick += 1;
            Some(tick)
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_len(&self) -> usize {
        self.pending.len()
    }

    #[cfg(test)]
    pub(crate) fn pending_events(&self) -> &[InputEvent] {
        &self.pending
    }
}

/// Shared ESC pause/resume transition used by the runtime and the tests.
///
/// Entering pause synthesizes one release per physically held lane on the next
/// unstepped tick (lane order) and hands them to the planner, which queues them
/// before clamping stale pending input.
pub(crate) fn toggle_paused_state(planner: &mut TickPlanner, input: &InputRouter) -> bool {
    let entering_pause = !planner.is_paused();
    let releases = if entering_pause {
        input.pause_releases(planner.next_tick())
    } else {
        Vec::new()
    };
    planner.toggle_pause(releases)
}

/// Advances every complete millisecond up to `max_tick`, stopping at the first
/// terminal step.
///
/// `step` receives the tick and its ordered input batch. Returning `true`
/// marks the planner terminal; no later tick is delivered. This is the same
/// path the window system uses, so death/pause/song-end ordering is exercised
/// without opening a window in tests.
pub(crate) fn advance_ticks<F>(
    planner: &mut TickPlanner,
    position_ms: f64,
    max_tick: u64,
    mut step: F,
) -> Result<bool, PlayError>
where
    F: FnMut(u64, &[InputEvent]) -> Result<bool, PlayError>,
{
    let Some(target) = planner.target_tick(position_ms)? else {
        return Ok(false);
    };
    let target = target.min(max_tick);
    while let Some(tick) = planner.next_step_tick(target) {
        let inputs = planner.take_inputs(tick)?;
        if step(tick, &inputs)? {
            planner.finish();
            return Ok(true);
        }
    }
    Ok(false)
}

/// `floor` of a validated song position on the 1 ms grid.
fn position_tick(position_ms: f64) -> Result<u64, PlayError> {
    if !position_ms.is_finite() || position_ms < 0.0 {
        return Err(PlayError::InvalidPosition { position_ms });
    }
    Ok(position_ms.floor() as u64)
}
