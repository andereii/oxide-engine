//! M5 phase-2b visual slice renderer.
//!
//! This module consumes the frozen phase-1 gameplay trace (`crate::slice`)
//! together with the frozen M2 asset APIs (`oxide-data`, `oxide-atlas`,
//! `oxide-vfs`) and the M4 conductor (`oxide-audio`), reproducing the scene
//! semantics frozen in `tools/oracle/RENDER-BEHAVIOR.md` §1-§6 on a headless
//! Bevy 0.20 render-to-image harness.
//!
//! Public integration boundary (frozen): [`SliceRenderer`] with `build`,
//! `build_for_song`, `step` and `frame`, plus [`RenderError`]. Everything else
//! is private.
//!
//! Numeric conventions:
//! - Scene geometry, camera, follow and animation deadlines are `f64`.
//! - Bevy mesh vertices and UVs are `f32`; the single conversion is an `as
//!   f32` cast at mesh assembly (round-to-nearest-even).
//! - `sim_ms` is the song position in milliseconds; `step` takes exactly the
//!   next 1 ms grid tick and only events/inputs stamped with that tick.
//! - Output frames are 720*1280*4 RGBA8 bytes (row-major, top-left origin,
//!   straight alpha), rendered into an sRGB target.

mod anim;
// The interactive play module reuses the exact same scene/layer pipeline as the
// headless renderer. These stay crate-private; only `play` consumes them.
pub(crate) mod geometry;
pub(crate) mod harness;
pub(crate) mod load;
pub(crate) mod scene;

use std::path::Path;

use thiserror::Error;

use crate::slice::{InputEvent, TraceEvent};

/// Errors returned while building or driving the slice renderer.
#[derive(Debug, Error)]
pub enum RenderError {
    #[error("unknown difficulty {0:?}; the bopeebo slice supports easy, normal and hard")]
    UnknownDifficulty(String),
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
    #[error("animate atlas {path:?} failed: {source}")]
    AtlasStage {
        path: String,
        #[source]
        source: oxide_atlas::AnimateStageError,
    },
    #[error("sparrow atlas {path:?} failed: {source}")]
    Sparrow {
        path: String,
        #[source]
        source: oxide_atlas::AtlasError,
    },
    #[error("packer atlas {path:?} failed: {source}")]
    Packer {
        path: String,
        #[source]
        source: oxide_atlas::AtlasError,
    },
    #[error("character file {path:?} requests unknown renderType {render_type:?}")]
    UnknownRenderType { path: String, render_type: String },
    #[error("asset text {path:?} is not valid UTF-8: {detail}")]
    Decode { path: String, detail: String },
    #[error("PNG {path:?} could not be decoded: {detail}")]
    Image { path: String, detail: String },
    #[error("asset {path:?} is missing required field {field}")]
    MissingField { path: String, field: &'static str },
    #[error("sprite page {path:?} has no frame with prefix {prefix:?}")]
    MissingFrame { path: String, prefix: String },
    #[error("character {character:?} animation {animation:?} has no main-atlas label {prefix:?}")]
    MissingAnimationLabel {
        character: String,
        animation: String,
        prefix: String,
    },
    #[error("unsupported slice data: {detail}")]
    Unsupported { detail: String },
    #[error("input/event at {event_ms} ms does not belong to the active {tick_ms} ms render tick")]
    OutOfGridEvent { event_ms: u64, tick_ms: u64 },
    #[error("render tick {tick_ms} ms is not the next 1 ms tick after {previous_ms:?}")]
    TickSequence {
        previous_ms: Option<u64>,
        tick_ms: u64,
    },
    #[error("event at {tick_ms} ms references unknown player note id {note_id}")]
    UnknownEventNote { tick_ms: u64, note_id: u32 },
    #[error("sim event at {tick_ms} ms references unknown opponent note id {note_id}")]
    UnknownOpponentNote { tick_ms: u64, note_id: u32 },
    #[error("duplicate opponent hit for note id {note_id} at tick {tick_ms} ms")]
    DuplicateOpponentNote { tick_ms: u64, note_id: u32 },
    #[error(
        "scroll_speed row at {tick_ms} ms for {strumline} expects base_scroll {expected}, scene has {actual}"
    )]
    ScrollBaseMismatch {
        tick_ms: u64,
        strumline: &'static str,
        expected: f64,
        actual: f64,
    },
    #[error("render adapter is {name:?}, not the pinned llvmpipe software raster")]
    AdapterNotPinned { name: String },
    #[error(
        "no hardware adapter is available for AdapterPolicy::AnyHardware; candidate adapters: {candidates:?}"
    )]
    NoHardwareAdapter { candidates: Vec<String> },
    #[error("renderer failure: {detail}")]
    Renderer { detail: String },
    #[error("render window is {width}x{height}, expected 1280x720")]
    RenderWindow { width: u32, height: u32 },
    #[error("readback image format {format:?} is not the expected RGBA8 sRGB target")]
    RenderFormat { format: String },
    #[error("GPU readback failed: {detail}")]
    Readback { detail: String },
    #[error("GPU readback did not complete within {timeout_ms} ms: {detail}")]
    ReadbackTimeout { timeout_ms: u64, detail: String },
}

/// wgpu adapter selection policy for [`SliceRenderer::build_for_adapter`].
///
/// ADR-0020 addendum (2026-11-07) decision 2: the deterministic renderer and
/// its frozen frame fixtures use [`AdapterPolicy::Software`] (llvmpipe pinned);
/// [`AdapterPolicy::AnyHardware`] exists only for the hardware `perf_budget`
/// gate and fails closed when the environment has no hardware adapter. Frame
/// bytes are adapter-specific, so digests are never compared across policies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdapterPolicy {
    /// Pin the llvmpipe software raster explicitly (deterministic fixture path).
    Software,
    /// Select the default hardware adapter and reject software adapters.
    /// Used only for platform-specific hardware performance measurements.
    AnyHardware,
}

/// Deterministic headless renderer for the bopeebo visual slice.
pub struct SliceRenderer {
    scene: scene::Scene,
    harness: harness::Harness,
}

impl SliceRenderer {
    /// Loads the frozen slice scene for `difficulty` and builds the headless
    /// Bevy render target pinned to the llvmpipe software adapter.
    ///
    /// `assets_root` is the real `assets/` directory. This is the frozen
    /// fixture/deterministic path; [`SliceRenderer::build_for_adapter`] exposes
    /// the explicit hardware policy for `perf_budget`.
    pub fn build(assets_root: &Path, difficulty: &str) -> Result<SliceRenderer, RenderError> {
        Self::build_for_adapter(assets_root, difficulty, AdapterPolicy::Software)
    }

    /// Loads the slice scene and builds the renderer for an explicit adapter
    /// policy (ADR-0020 addendum decision 2).
    ///
    /// `Software` is identical to [`SliceRenderer::build`]. `AnyHardware`
    /// selects the default hardware adapter and returns
    /// [`RenderError::NoHardwareAdapter`] when only software adapters exist.
    /// Both policies print the chosen adapter identity to stderr.
    pub fn build_for_adapter(
        assets_root: &Path,
        difficulty: &str,
        policy: AdapterPolicy,
    ) -> Result<SliceRenderer, RenderError> {
        let loaded = load::load_slice(assets_root, difficulty)?;
        Self::from_loaded(loaded, policy)
    }

    /// Loads any `corpus.txt`-style chart id (`<song>`, `<song>-default`, or
    /// `<song>-<variation>`), mirroring [`crate::slice::GameplaySimState::build_chart`]
    /// id selection (docs/M6-CONTRACT §11.2.5).
    ///
    /// This is the generalized scene front end needed to build non-bopeebo
    /// stages; it always uses the deterministic [`AdapterPolicy::Software`]
    /// path. The M5 bopeebo fixture path remains [`SliceRenderer::build`].
    pub fn build_for_song(
        assets_root: &Path,
        chart_id: &str,
        difficulty: &str,
    ) -> Result<SliceRenderer, RenderError> {
        let loaded = load::load_slice_for_song(assets_root, chart_id, difficulty)?;
        Self::from_loaded(loaded, AdapterPolicy::Software)
    }

    fn from_loaded(
        loaded: load::LoadedSlice,
        policy: AdapterPolicy,
    ) -> Result<SliceRenderer, RenderError> {
        let (scene, pages) = scene::Scene::new(loaded)?;
        let harness = match policy {
            AdapterPolicy::Software => harness::Harness::new(pages, scene.layer_specs())?,
            AdapterPolicy::AnyHardware => {
                harness::Harness::with_policy(pages, scene.layer_specs(), policy)?
            }
        };
        Ok(SliceRenderer { scene, harness })
    }

    /// One scene tick on the same 1 ms grid as the gameplay sim.
    ///
    /// Every event/input must be stamped with `sim_ms` (this tick) and the
    /// call must be the next consecutive millisecond; otherwise a typed error
    /// is returned and scene state is unchanged except for the accepted tick
    /// bookkeeping above.
    pub fn step(
        &mut self,
        sim_ms: u64,
        events: &[TraceEvent],
        inputs: &[InputEvent],
    ) -> Result<(), RenderError> {
        self.scene.step(sim_ms, events, inputs)
    }

    /// Renders the current scene state to 720*1280*4 RGBA8 bytes.
    ///
    /// The call does not advance sim state: two calls with an unchanged scene
    /// return identical bytes.
    pub fn frame(&mut self) -> Result<Vec<u8>, RenderError> {
        let layers = self.scene.draw()?;
        self.harness.render(&layers)
    }
}
