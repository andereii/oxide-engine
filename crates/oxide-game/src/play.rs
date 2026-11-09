//! M5 phase-2c interactive bopeebo play session (ADR-0021).
//!
//! Public integration boundary: [`PlaySession::run`] with [`PlayConfig`] and
//! [`PlayReport`]. Everything else (windowed harness, tick planner, HUD,
//! audio bridge) is private.
//!
//! The session reuses the frozen phase-1 sim (`crate::slice`), the phase-2b
//! scene (`crate::render::scene`) and the kira playback API
//! (`oxide_audio::playback`) rather than forking any of them:
//!
//! - The sim advances on the same 1 ms grid as the headless gates.
//! - The scene sees every accepted tick, so camera decay and conductor
//!   step/beat pulses use the same values as `render_diff`.
//! - The window is a real winit window at 1280x720 with scale factor 1 and
//!   vsync; the readback render target is only used by the headless gate.
//! - Audio starts after the Bevy app is built, on the same kira clock as the
//!   playback position that drives the sim, immediately before the event loop
//!   (`app.run()`). Without the `oxide-audio/real-audio` feature,
//!   `SongPlayback::start` fails closed and this module returns
//!   [`PlayError::AudioUnavailable`]; on any host where Bevy app construction
//!   succeeds, the error aborts before the event loop, so no window is shown
//!   and no audio is substituted.
//!
//! Engine decisions recorded in the module:
//!
//! - The HUD geometry is the 6D behavior freeze (M6-CONTRACT §13): the scene
//!   owns the shared health bar, icon and score-text algebra, so the play
//!   window and the deterministic render slice compose the same layers.
//! - Controls are keyboard-only: WASD/arrows for lanes, ESC pause/resume, Q
//!   quit; the window close button also ends the session. The gamepad is not
//!   wired. WASD and arrows are alternate bindings per lane: releasing one
//!   while the other is still held keeps the lane held.
//! - End of song is the last full-chart note boundary on the sim grid (last
//!   note end + hit window + hold-end guard), not a decoded container length.
//! - The session writes no files and prints no metrics; only the CLI consumer
//!   reports the returned [`PlayReport`]. This is a playtest, not a
//!   timing/latency or native-parity measurement.

mod audio;
mod input;
mod session;
mod window;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use thiserror::Error;

use crate::slice::TallyCounts;

/// One of the three bopeebo difficulties.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Difficulty {
    Easy,
    Normal,
    Hard,
}

impl Difficulty {
    /// All slice difficulties in ascending order.
    pub const ALL: [Difficulty; 3] = [Difficulty::Easy, Difficulty::Normal, Difficulty::Hard];

    /// Lowercase wire/stem name (`easy`, `normal`, `hard`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Difficulty::Easy => "easy",
            Difficulty::Normal => "normal",
            Difficulty::Hard => "hard",
        }
    }

    /// Parses the CLI/stem spelling case-insensitively.
    pub fn parse(value: &str) -> Result<Self, PlayError> {
        Self::ALL
            .into_iter()
            .find(|difficulty| difficulty.as_str().eq_ignore_ascii_case(value))
            .ok_or_else(|| PlayError::UnknownDifficulty(value.to_owned()))
    }
}

impl std::fmt::Display for Difficulty {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Difficulty::Easy => "Easy",
            Difficulty::Normal => "Normal",
            Difficulty::Hard => "Hard",
        })
    }
}

impl std::str::FromStr for Difficulty {
    type Err = PlayError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

/// Frozen window settings for the playtest.
///
/// The type intentionally has no public fields: ADR-0021 defines a fixed
/// 1280x720 window at scale factor 1 with vsync and a difficulty-bearing
/// title. Exposing width/height/present-mode knobs would let callers escape
/// that contract. [`WindowConfig::default`] is the only construction path
/// needed by the CLI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowConfig {}

impl WindowConfig {
    /// Frozen game/window width in physical pixels.
    pub const WIDTH: u32 = 1280;
    /// Frozen game/window height in physical pixels.
    pub const HEIGHT: u32 = 720;
    /// Vsync presentation; the observed native target is 60 Hz.
    pub const VSYNC_HZ: u32 = 60;

    /// The only valid window configuration (the defaults above).
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

/// Everything the play session needs to start.
#[derive(Debug, Clone)]
pub struct PlayConfig {
    /// Required explicit assets directory (the shipped `assets/` root).
    pub assets_root: PathBuf,
    /// Selected bopeebo difficulty.
    pub difficulty: Difficulty,
    /// Frozen window configuration.
    pub window: WindowConfig,
}

/// Why the interactive session ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitReason {
    /// The user pressed the quit key (`Q`).
    QuitKey,
    /// The window close button/OS close request ended the session.
    WindowClosed,
    /// The sim advanced past the last full-chart note boundary.
    SongEnd,
    /// The sim's terminal `game_over` event fired (no retry is provided).
    GameOver,
    /// Developer-only auto-quit hook used by the ignored headed smoke test.
    FrameLimit,
}

/// One player-vocal volume change observed on a sim tick.
///
/// These are the phase-2 mute/restore ticks: they are recorded by the play
/// session for the deferred waveform verification. Values are linear
/// amplitudes (`0.0` muted, `1.0` restored).
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceVolumeTick {
    /// Sim grid tick (song position in whole milliseconds).
    pub sim_ms: u64,
    /// Metadata voice character id, e.g. `"bf"`.
    pub character: String,
    /// Effective linear volume passed to the playback API.
    pub volume: f32,
}

/// Result of an interactive session.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayReport {
    /// Judged player-note tallies at the terminal boundary.
    pub tallies: TallyCounts,
    /// Accumulated PBOT1 score at the terminal boundary.
    ///
    /// The sim's internal `moveToGameOver` score reset is intentionally not
    /// reflected: this is the playtest score the user earned.
    pub score: f64,
    /// Player health at the terminal boundary, clamped to the observed
    /// `[0.0, 2.0]` display range.
    pub health: f64,
    /// Mute/restore events in dispatch order.
    pub voice_volume_ticks: Vec<VoiceVolumeTick>,
    /// Last stepped sim tick in milliseconds (song position end).
    pub song_position_ms: f64,
    /// Terminal reason.
    pub quit_reason: QuitReason,
}

impl PlayReport {
    /// Number of recorded ticks that set a voice to silence.
    #[must_use]
    pub fn mute_tick_count(&self) -> usize {
        self.voice_volume_ticks
            .iter()
            .filter(|tick| tick.volume == 0.0)
            .count()
    }
}

/// Typed session failures.
#[derive(Debug, Error)]
pub enum PlayError {
    #[error("unknown difficulty {0:?}; the bopeebo slice supports easy, normal and hard")]
    UnknownDifficulty(String),
    #[error(transparent)]
    Assets(#[from] crate::render::RenderError),
    #[error(transparent)]
    Sim(#[from] crate::slice::SliceError),
    #[error("real audio output is unavailable: {reason}")]
    AudioUnavailable { reason: String },
    #[error(transparent)]
    Audio(#[from] oxide_audio::clock::AudioError),
    #[error("song audio configuration is invalid: {detail}")]
    AudioConfig { detail: String },
    #[error("audio track {path:?} is missing from the asset layers")]
    MissingAudioTrack { path: String },
    #[error("failed to read audio track {path:?}: {source}")]
    AudioRead {
        path: String,
        #[source]
        source: oxide_vfs::VfsError,
    },
    #[error("play clock position {position_ms} is not a finite non-negative millisecond value")]
    InvalidPosition { position_ms: f64 },
    #[error("out-of-order tick: expected {expected} ms, got {got} ms")]
    OutOfOrderTick { expected: u64, got: u64 },
    #[error("window failure: {detail}")]
    Window { detail: String },
    #[error("session failure: {detail}")]
    Session { detail: String },
}

/// Entry point for the interactive bopeebo slice.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlaySession;

impl PlaySession {
    /// Loads the slice, builds the windowed scene, starts Inst + player/
    /// opponent voice tracks, then runs until quit, song end or the terminal
    /// game-over boundary.
    ///
    /// After loading the slice/scene/HUD and building the Bevy app, and before
    /// the event loop starts (`app.run()`), audio is started. A build without
    /// `oxide-audio/real-audio` returns [`PlayError::AudioUnavailable`] (the
    /// underlying `AudioError::AudioUnavailable` reason is preserved), aborting
    /// with no window shown and no audio substituted — on any host where Bevy
    /// app construction succeeds.
    pub fn run(config: PlayConfig) -> Result<PlayReport, PlayError> {
        session::run(config, None)
    }
}
