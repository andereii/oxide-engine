//! Session state shared by the windowed runner and the pure tests.
//!
//! The Bevy system in [`super::window`] only moves messages and quads; all
//! gameplay/audio/report ordering lives here so it can be tested without a
//! window or an audio device.
//!
//! The F6 overlay's `Frame` value is the rolling mean of the last 60 rendered
//! frame intervals; it is NOT an input or audio latency measurement.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use bevy::app::AppExit;
use bevy::ecs::message::MessageWriter;
use bevy::prelude::Resource;

use oxide_audio::playback::SongPlayback;
use oxide_vfs::Vfs;

use super::audio;
use super::hud::Hud;
use super::input::{
    InputRouter, TickPlanner, advance_ticks, should_queue_lane_event, toggle_paused_state,
};
use super::{PlayConfig, PlayError, PlayReport, QuitReason, VoiceVolumeTick};
use crate::render::load::{self, LoadedNote};
use crate::render::scene::{Quad, Scene};
use crate::slice::{GameplaySimState, TallyCounts, TraceEvent};

/// `Constants.HIT_WINDOW_MS` (already frozen in the M5 slice). Used only to
/// define the full-chart end tick, not to re-judge anything.
const HIT_WINDOW_MS: f64 = 160.0;
/// Hold completion is emitted at `end + 2`; two extra grid ticks keep the
/// final hold event inside the stepped range.
const HOLD_END_GUARD_TICKS: u64 = 2;

/// The F6 overlay averages FPS/frame interval over the last 60 rendered frames
/// (ADR-0020 addendum 4 item 7).
pub(crate) const FPS_WINDOW_FRAMES: usize = 60;

/// Rolling frame-delta window for the F6 debug overlay.
#[derive(Debug, Clone)]
pub(crate) struct FrameStats {
    deltas_ms: [f64; FPS_WINDOW_FRAMES],
    next: usize,
    filled: usize,
}

impl FrameStats {
    pub(crate) fn new() -> Self {
        Self {
            deltas_ms: [0.0; FPS_WINDOW_FRAMES],
            next: 0,
            filled: 0,
        }
    }

    /// Records one rendered-frame delta. Non-finite or negative values are
    /// ignored so the rolling average cannot be poisoned.
    pub(crate) fn record(&mut self, delta_ms: f64) {
        if !delta_ms.is_finite() || delta_ms < 0.0 {
            return;
        }
        self.deltas_ms[self.next] = delta_ms;
        self.next = (self.next + 1) % FPS_WINDOW_FRAMES;
        self.filled = (self.filled + 1).min(FPS_WINDOW_FRAMES);
    }

    /// Mean frame delta over the retained window, or `None` before the first
    /// frame.
    pub(crate) fn average_ms(&self) -> Option<f64> {
        if self.filled == 0 {
            return None;
        }
        let sum: f64 = self.deltas_ms[..self.filled].iter().sum();
        Some(sum / self.filled as f64)
    }

    /// `1000 / average frame delta`; zero for a zero-delta window.
    pub(crate) fn fps(&self) -> Option<f64> {
        self.average_ms()
            .map(|average| if average > 0.0 { 1000.0 / average } else { 0.0 })
    }
}

/// F6 overlay toggle. Toggling only flips this flag; the sim/scene are never
/// touched (ADR-0020 addendum 4 item 7).
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct DebugOverlay {
    active: bool,
}

impl DebugOverlay {
    pub(crate) fn toggle(&mut self) -> bool {
        self.active = !self.active;
        self.active
    }

    pub(crate) fn active(self) -> bool {
        self.active
    }
}

/// Runs one interactive session.
///
/// `frame_limit` is a private developer hook for the ignored headed smoke
/// test; the public [`super::PlaySession::run`] passes `None`.
pub(crate) fn run(config: PlayConfig, frame_limit: Option<u64>) -> Result<PlayReport, PlayError> {
    let loaded = load::load_slice(&config.assets_root, config.difficulty.as_str())?;
    let song_end_tick = last_song_tick(&loaded.notes);
    let player_vocals = loaded.player_vocals.clone();
    let opponent_vocals = loaded.opponent_vocals.clone();

    let (scene, mut pages) = Scene::new(loaded)?;
    let vfs = Vfs::new(config.assets_root.clone());
    let (hud, hud_pages) = match Hud::load(&vfs, pages.len()) {
        Some((hud, hud_pages)) => (Some(hud), hud_pages),
        None => (None, Vec::new()),
    };
    pages.extend(hud_pages);

    let mut specs = scene.layer_specs().to_vec();
    if let Some(hud) = &hud {
        specs.extend_from_slice(hud.layer_specs());
    }

    // The play scene consumes the schema-3 chart-event rows (M6-CONTRACT
    // §12.7): the sim walks bopeebo's FocusCamera/ZoomCamera/SetCameraBop/
    // PlayAnimation/ScrollSpeed rows so the scene camera/animations/scroll
    // follow the same stream as the deterministic render path.
    let sim = GameplaySimState::build_chart_events(
        &config.assets_root,
        "bopeebo",
        config.difficulty.as_str(),
    )?;
    let sink = ReportSink::new();
    let runtime = PlayRuntime::new(
        sim,
        scene,
        hud,
        player_vocals.clone(),
        song_end_tick,
        frame_limit,
        sink.clone(),
    );
    runtime.publish_provisional();

    // Build the Bevy app first, then start audio immediately before the event
    // loop (`app.run()`). The app is already constructed at this point; the
    // typed AudioUnavailable path aborts before the event loop, so no window is
    // shown on any host where Bevy app construction itself succeeds. The
    // worker-thread winit opt-in is only for the private frame-limited
    // developer smoke.
    let run_on_any_thread = frame_limit.is_some();
    let mut app = super::window::build(&config, pages, &specs, runtime, run_on_any_thread)?;
    let playback = audio::start(&config.assets_root, &player_vocals, &opponent_vocals)?;
    app.world_mut().resource_mut::<PlayRuntime>().audio = Some(playback);

    eprintln!(
        "oxide play: controls WASD/arrows lanes, ESC pause/resume, Q quit; \
         gamepad not wired; playtest only (not latency/native-parity evidence)"
    );
    let exit = app.run();
    sink.take(exit)
}

/// Final simulated tick for the full chart (player and opponent notes).
pub(crate) fn last_song_tick(notes: &[LoadedNote]) -> u64 {
    let max_end = notes
        .iter()
        .map(|note| note.time + note.sustain_ms)
        .fold(0.0, f64::max);
    if !max_end.is_finite() || max_end <= 0.0 {
        return 1;
    }
    ((max_end + HIT_WINDOW_MS).ceil().max(0.0) as u64).saturating_add(HOLD_END_GUARD_TICKS)
}

/// Read-only-ish gameplay totals reconstructed from the sim's dispatched
/// events (the frozen sim exposes no health/score getter).
pub(crate) struct ScoreTracker {
    pub(crate) tallies: TallyCounts,
    pub(crate) score: f64,
    pub(crate) health: f64,
    pub(crate) terminal: bool,
}

impl ScoreTracker {
    pub(crate) fn new() -> Self {
        Self {
            tallies: TallyCounts::default(),
            score: 0.0,
            health: 1.0,
            terminal: false,
        }
    }

    /// Applies one tick's events in dispatch order.
    ///
    /// The sim clamps health at the START of every tick; mirror that here so
    /// the HUD/report value tracks the internal one (events may leave health
    /// outside the display range until the next clamp, exactly like the sim).
    pub(crate) fn observe(&mut self, events: &[TraceEvent]) {
        if self.terminal {
            return;
        }
        self.health = self.health.clamp(0.0, 2.0);
        for event in events {
            match event {
                TraceEvent::Hit {
                    score_delta,
                    health_after,
                    tallies,
                    ..
                }
                | TraceEvent::Miss {
                    score_delta,
                    health_after,
                    tallies,
                    ..
                } => {
                    self.score += f64::from(*score_delta);
                    self.health = *health_after;
                    self.tallies = *tallies;
                }
                TraceEvent::GhostPress {
                    score_delta,
                    health_change,
                    ..
                } => {
                    self.score += *score_delta;
                    self.health += *health_change;
                }
                TraceEvent::HoldTick {
                    health_delta,
                    score_delta,
                    ..
                } => {
                    self.health += *health_delta;
                    self.score += *score_delta;
                }
                TraceEvent::HoldDrop {
                    score_penalty,
                    health_change,
                    ..
                } => {
                    self.score += *score_penalty;
                    self.health += *health_change;
                }
                TraceEvent::HoldStart { .. } | TraceEvent::HoldEnd { .. } => {}
                TraceEvent::GameOver { .. } => {
                    self.terminal = true;
                    self.health = 0.0;
                }
                // Compile-only arm authorized for the schema-2 variant:
                // opponent rows never mutate player score/health/combo.
                TraceEvent::OpponentHit { .. } => {}
                // Schema-3 chart-event rows are not player scoring state.
                TraceEvent::CameraFocus { .. }
                | TraceEvent::CameraZoom { .. }
                | TraceEvent::CameraBop { .. }
                | TraceEvent::PlayAnimation { .. }
                | TraceEvent::ScrollSpeed { .. }
                | TraceEvent::HealthIcon { .. }
                | TraceEvent::UnknownEvent { .. } => {}
            }
        }
    }
}

/// Player vocal mute/restore state and the tick log.
pub(crate) struct VoiceMuteState {
    muted: bool,
}

impl VoiceMuteState {
    pub(crate) fn new() -> Self {
        Self { muted: false }
    }

    /// Returns the volume calls for this tick. A call is emitted only when an
    /// event changes the player's mute state, in event order (a same-tick
    /// miss then hit records both transitions).
    pub(crate) fn volumes_for(
        &mut self,
        sim_ms: u64,
        events: &[TraceEvent],
        characters: &[String],
    ) -> Vec<VoiceVolumeTick> {
        let mut ticks = Vec::new();
        for event in events {
            let desired = match event {
                TraceEvent::Hit { .. } => Some(false),
                TraceEvent::Miss { .. } | TraceEvent::GhostPress { .. } => Some(true),
                _ => None,
            };
            let Some(desired) = desired else {
                continue;
            };
            if desired == self.muted || characters.is_empty() {
                continue;
            }
            self.muted = desired;
            let volume = if desired { 0.0 } else { 1.0 };
            ticks.extend(characters.iter().map(|character| VoiceVolumeTick {
                sim_ms,
                character: character.clone(),
                volume,
            }));
        }
        ticks
    }
}

/// Bevy resource owning the live session. The window system mutates it; the
/// pure tests use the same methods with the real sim loaded from temporary
/// synthetic assets.
#[derive(Resource)]
pub(crate) struct PlayRuntime {
    scene: Scene,
    hud: Option<Hud>,
    planner: TickPlanner,
    input: InputRouter,
    sim: GameplaySimState,
    tracker: ScoreTracker,
    voice: VoiceMuteState,
    voice_ticks: Vec<VoiceVolumeTick>,
    audio: Option<SongPlayback>,
    player_vocals: Vec<String>,
    song_end_tick: u64,
    last_tick: Option<u64>,
    last_position_ms: f64,
    pending_reason: Option<QuitReason>,
    quit_reason: Option<QuitReason>,
    pub(crate) done: bool,
    frames: u64,
    frame_limit: Option<u64>,
    frame_stats: FrameStats,
    debug_overlay: DebugOverlay,
    last_sim_step_ms: f64,
    sink: ReportSink,
}

impl PlayRuntime {
    fn new(
        sim: GameplaySimState,
        scene: Scene,
        hud: Option<Hud>,
        player_vocals: Vec<String>,
        song_end_tick: u64,
        frame_limit: Option<u64>,
        sink: ReportSink,
    ) -> Self {
        Self {
            scene,
            hud,
            planner: TickPlanner::new(),
            input: InputRouter::new(),
            sim,
            tracker: ScoreTracker::new(),
            voice: VoiceMuteState::new(),
            voice_ticks: Vec::new(),
            audio: None,
            player_vocals,
            song_end_tick,
            last_tick: None,
            last_position_ms: 0.0,
            pending_reason: None,
            quit_reason: None,
            done: false,
            frames: 0,
            frame_limit,
            frame_stats: FrameStats::new(),
            debug_overlay: DebugOverlay::default(),
            last_sim_step_ms: 0.0,
            sink,
        }
    }

    /// Reads and validates the shared playback clock.
    pub(crate) fn poll_audio(&mut self) -> Result<f64, PlayError> {
        let playback = self.audio.as_ref().ok_or_else(|| PlayError::Session {
            detail: "audio playback is not running".to_string(),
        })?;
        let position = playback.position_ms().map_err(audio::map_error)?;
        if !position.is_finite() || position < 0.0 {
            return Err(PlayError::InvalidPosition {
                position_ms: position,
            });
        }
        self.last_position_ms = position;
        Ok(position)
    }

    /// Stamps a real key transition and queues the resulting lane event.
    ///
    /// Every press is forwarded; a release is forwarded only when the last
    /// bound key for the lane goes up (alternate WASD/arrow bindings share one
    /// lane count). The physical lane state is updated even while paused; the
    /// tick queue itself ignores paused/terminal events.
    pub(crate) fn queue_key(&mut self, direction: i64, pressed: bool) -> Result<(), PlayError> {
        let transition = self.input.observe(direction, pressed);
        if should_queue_lane_event(pressed, transition) {
            let tick = self.planner.stamp_tick(self.last_position_ms)?;
            self.planner.queue(tick, direction, pressed);
        }
        Ok(())
    }

    /// ESC pause/resume: the playback clock and the sim stop together.
    ///
    /// Entering pause synthesizes one release per physically held lane on the
    /// next unstepped tick, in lane order. The planner queues those releases
    /// before clamping pending input, so the sim's held lanes are released
    /// deterministically when the frozen clock resumes.
    pub(crate) fn toggle_pause(&mut self) -> Result<(), PlayError> {
        let paused = toggle_paused_state(&mut self.planner, &self.input);
        if let Some(playback) = self.audio.as_ref() {
            if paused {
                playback.pause().map_err(audio::map_error)?;
                eprintln!("oxide play: paused (ESC resumes, Q quits)");
            } else {
                playback.resume().map_err(audio::map_error)?;
                eprintln!("oxide play: resumed");
            }
        }
        Ok(())
    }

    /// Advances every complete millisecond up to the current clock position.
    pub(crate) fn advance(&mut self, position_ms: f64) -> Result<bool, PlayError> {
        let song_end_tick = self.song_end_tick;
        let terminal = {
            let PlayRuntime {
                scene,
                planner,
                sim,
                tracker,
                voice,
                voice_ticks,
                audio,
                player_vocals,
                last_tick,
                pending_reason,
                ..
            } = self;
            advance_ticks(planner, position_ms, song_end_tick, |tick, inputs| {
                let events = sim.step(tick, inputs);
                scene.step(tick, &events, inputs)?;
                *last_tick = Some(tick);
                tracker.observe(&events);
                let volumes = voice.volumes_for(tick, &events, player_vocals);
                if let Some(playback) = audio.as_ref() {
                    for volume in &volumes {
                        playback
                            .set_voice_volume(&volume.character, volume.volume)
                            .map_err(audio::map_error)?;
                    }
                }
                voice_ticks.extend(volumes);
                if tracker.terminal {
                    *pending_reason = Some(QuitReason::GameOver);
                    Ok(true)
                } else {
                    Ok(false)
                }
            })?
        };

        if !terminal && self.planner.next_tick() > self.song_end_tick {
            self.pending_reason = Some(QuitReason::SongEnd);
        }
        Ok(terminal)
    }

    pub(crate) fn song_finished(&self) -> bool {
        self.planner.next_tick() > self.song_end_tick
    }

    pub(crate) fn has_pending_reason(&self) -> bool {
        self.pending_reason.is_some()
    }

    pub(crate) fn take_pending_reason(&mut self) -> Option<QuitReason> {
        self.pending_reason.take()
    }

    pub(crate) fn set_pending_reason(&mut self, reason: QuitReason) {
        if self.pending_reason.is_none() {
            self.pending_reason = Some(reason);
        }
    }

    /// Increments the rendered-frame counter and reports whether the private
    /// developer frame limit has been reached.
    pub(crate) fn frame_limit_reached(&mut self) -> bool {
        self.frames += 1;
        self.frame_limit.is_some_and(|limit| self.frames >= limit)
    }

    /// Records one rendered-frame delta for the F6 overlay.
    pub(crate) fn record_frame_delta(&mut self, delta_ms: f64) {
        self.frame_stats.record(delta_ms);
    }

    /// Records the wall time spent stepping the sim this frame.
    pub(crate) fn record_sim_step_ms(&mut self, elapsed_ms: f64) {
        if elapsed_ms.is_finite() && elapsed_ms >= 0.0 {
            self.last_sim_step_ms = elapsed_ms;
        }
    }

    /// F6: pure UI toggle; never steps or mutates the sim/scene.
    pub(crate) fn toggle_debug_overlay(&mut self) -> bool {
        self.debug_overlay.toggle()
    }

    pub(crate) fn debug_overlay_active(&self) -> bool {
        self.debug_overlay.active()
    }

    /// Three-line F6 overlay text (song position/total, health/score).
    pub(crate) fn overlay_text(&self) -> String {
        let fps = self.frame_stats.fps().unwrap_or(0.0);
        let frame_ms = self.frame_stats.average_ms().unwrap_or(0.0);
        format!(
            "FPS: {fps:.0} | Frame: {frame_ms:.1} ms | Sim: {:.2} ms\n\
             Song: {:.0} / {}\n\
             Health: {:.2} | Score: {:.1}",
            self.last_sim_step_ms,
            self.last_position_ms,
            self.song_end_tick,
            self.tracker.health.clamp(0.0, 2.0),
            self.tracker.score,
        )
    }

    /// Draws the frozen scene plus the optional HUD layer.
    pub(crate) fn draw_layers(&self) -> Result<Vec<Vec<Quad>>, PlayError> {
        let mut layers = self.scene.draw()?;
        if let Some(hud) = &self.hud {
            layers.extend(hud.draw(self.tracker.health));
        }
        Ok(layers)
    }

    pub(crate) fn publish_provisional(&self) {
        let reason = self.quit_reason.unwrap_or(QuitReason::WindowClosed);
        self.sink.publish(self.make_report(reason));
    }

    /// Terminal success path: stop audio, publish the report, request exit.
    pub(crate) fn request_terminal(
        &mut self,
        reason: QuitReason,
        exit: &mut MessageWriter<AppExit>,
    ) {
        if self.done {
            return;
        }
        self.done = true;
        self.quit_reason = Some(reason);
        let report = self.make_report(reason);
        self.sink.publish(report.clone());
        if let Err(error) = self.stop_audio() {
            self.sink.fail(error);
            exit.write(AppExit::error());
            return;
        }
        self.sink.finish(report);
        exit.write(AppExit::Success);
    }

    /// Error path: publish the typed error and request an error exit.
    pub(crate) fn request_error(&mut self, error: PlayError, exit: &mut MessageWriter<AppExit>) {
        if self.done {
            return;
        }
        self.done = true;
        self.sink.fail(error);
        let _ = self.stop_audio();
        exit.write(AppExit::error());
    }

    fn make_report(&self, reason: QuitReason) -> PlayReport {
        PlayReport {
            tallies: self.tracker.tallies,
            score: self.tracker.score,
            health: self.tracker.health.clamp(0.0, 2.0),
            voice_volume_ticks: self.voice_ticks.clone(),
            song_position_ms: self.last_tick.map_or(0.0, |tick| tick as f64),
            quit_reason: reason,
        }
    }

    fn stop_audio(&mut self) -> Result<(), PlayError> {
        if let Some(playback) = self.audio.take() {
            playback.stop().map_err(audio::map_error)?;
        }
        Ok(())
    }
}

/// Shared report/error slot written by the Bevy system and read after the
/// event loop returns. `App::run` consumes the app, so an `Arc` is the only
/// way to surface the report.
#[derive(Clone)]
pub(crate) struct ReportSink {
    terminal: Arc<Mutex<Option<Result<PlayReport, PlayError>>>>,
    provisional: Arc<Mutex<PlayReport>>,
}

impl ReportSink {
    fn new() -> Self {
        Self {
            terminal: Arc::new(Mutex::new(None)),
            provisional: Arc::new(Mutex::new(PlayReport {
                tallies: TallyCounts::default(),
                score: 0.0,
                health: 1.0,
                voice_volume_ticks: Vec::new(),
                song_position_ms: 0.0,
                quit_reason: QuitReason::WindowClosed,
            })),
        }
    }

    fn publish(&self, report: PlayReport) {
        *lock(&self.provisional) = report;
    }

    fn finish(&self, report: PlayReport) {
        self.publish(report.clone());
        *lock(&self.terminal) = Some(Ok(report));
    }

    fn fail(&self, error: PlayError) {
        *lock(&self.terminal) = Some(Err(error));
    }

    fn take(&self, exit: AppExit) -> Result<PlayReport, PlayError> {
        if let Some(outcome) = lock(&self.terminal).take() {
            return outcome;
        }
        if !exit.is_success() {
            return Err(PlayError::Window {
                detail: format!("windowed application exited with {exit:?}"),
            });
        }
        // The window may close through the OS path before the session system
        // observes it; the provisional report is the last coherent state.
        Ok(lock(&self.provisional).clone())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
