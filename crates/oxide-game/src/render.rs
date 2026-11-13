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

pub(crate) mod abot;
mod anim;
// The interactive play module reuses the exact same scene/layer pipeline as the
// headless renderer. These stay crate-private; only `play` consumes them.
pub(crate) mod choreography;
pub(crate) mod conversation;
pub(crate) mod dialogue;
pub(crate) mod geometry;
pub(crate) mod harness;
pub(crate) mod hud;
pub(crate) mod load;
pub(crate) mod scene;
pub(crate) mod shaders;

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
        let loaded = load::load_slice_for_fixture(assets_root, difficulty)?;
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

    /// Dialogue-enabled variant of [`Self::build_for_song`] (M6-CONTRACT
    /// §14.3): loads the conversation payload for the senpai-family chart
    /// ids so the scene can drive the in-game dialogue; other ids load
    /// identically to [`Self::build_for_song`].
    pub fn build_for_song_with_dialogue(
        assets_root: &Path,
        chart_id: &str,
        difficulty: &str,
    ) -> Result<SliceRenderer, RenderError> {
        let loaded = load::load_slice_for_song_with_dialogue(assets_root, chart_id, difficulty)?;
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
        self.harness.render(&layers, self.scene.layer_specs())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn assets_root() -> PathBuf {
        std::env::var_os("OXIDE_ASSETS_ROOT")
            .map(PathBuf::from)
            .expect("OXIDE_ASSETS_ROOT must point at the shipped assets directory")
    }

    /// Engine-level check (NOT a frozen fixture): a shader-active stage must
    /// compile the 6G WGSL material pipeline and render through it. The
    /// schoolErect `senpai-pico` scene gives all three characters DropShadow
    /// layers with alternate masks, exercising the uniform, base-texture and
    /// mask bindings end-to-end on the pinned llvmpipe adapter.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6G shader pipeline"]
    fn shader_active_stage_renders_through_the_material_pipeline() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "senpai-pico", "easy")
            .unwrap_or_else(|error| panic!("build senpai-pico renderer: {error}"));
        for tick in 0..=2u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("shader scene step {tick}: {error}"));
        }
        let frame = renderer.frame().expect("shader material pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(
            non_black > 0,
            "the shader-active stage must draw visible pixels"
        );
    }

    /// Engine-level 6G sub-slice 2 check (NOT a frozen fixture): the
    /// phillyStreetsErect screen-rain post layer must build the offscreen game
    /// target + second camera, sample it through `rain.frag`, and composite a
    /// visible frame on the pinned llvmpipe adapter. The main camera still
    /// renders the unfiltered overlay/HUD above the rain pass.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6G screen-rain pass"]
    fn screen_rain_stage_renders_through_the_offscreen_camera() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "darnell-erect", "erect")
            .unwrap_or_else(|error| panic!("build darnell-erect renderer: {error}"));
        for tick in 0..=2u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("screen-rain scene step {tick}: {error}"));
        }
        let frame = renderer.frame().expect("screen-rain pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(
            non_black > 0,
            "the rain-filtered stage must composite visible pixels"
        );
        // The rain tint biases the darkened phillyStreetsErect stage toward
        // the grey-blue `0xA8ADB5` color; the exact pixels remain an
        // llvmpipe engine fixture obligation, not native parity.
        let bluish = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[2] > pixel[0] && pixel[2] > 8)
            .count();
        assert!(bluish > 0, "the rain color must be visible in the frame");
    }

    /// Engine-level 6G sub-slice 2 check: the schoolEvil wiggle stage must
    /// compile and render through the displaced-UV material path.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6G wiggle shader"]
    fn wiggle_stage_renders_through_the_material_pipeline() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "thorns-default", "normal")
            .unwrap_or_else(|error| panic!("build thorns-default renderer: {error}"));
        for tick in 0..=2u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("wiggle scene step {tick}: {error}"));
        }
        let frame = renderer.frame().expect("wiggle pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(
            non_black > 0,
            "the wiggle-active stage must draw visible pixels"
        );
    }

    /// Engine-level 6G sub-slice 3 check (NOT a frozen fixture): the
    /// sserafim stage must compile the `SserafimShader` material and step
    /// across the Dark / PulseLights / Lights event tween hooks on the pinned
    /// llvmpipe adapter.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6G sserafim shader"]
    fn sserafim_stage_renders_through_the_event_tween_hooks() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "spaghetti-default", "easy")
            .unwrap_or_else(|error| panic!("build spaghetti-default renderer: {error}"));
        let event = |sim: u64, kind: &str, payload: serde_json::Value| TraceEvent::UnknownEvent {
            schema: "3",
            kind: "unknown_event",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            event_kind: kind.to_string(),
            value: Some(oxide_data::chart::SongEvent {
                time: sim as f64,
                event: kind.to_string(),
                value: Some(payload),
            }),
        };
        renderer
            .step(
                0,
                &[event(
                    0,
                    "sserafimDark",
                    serde_json::json!({ "amount": 0.8, "duration": 1.0 }),
                )],
                &[],
            )
            .unwrap_or_else(|error| panic!("sserafimDark step: {error}"));
        renderer
            .step(
                1,
                &[event(
                    1,
                    "sserafimPulseLights",
                    serde_json::json!({
                        "enabled": true,
                        "colors": ["0xFF6D43FF"],
                        "durations": [1.0],
                        "intensities": [1.0],
                    }),
                )],
                &[],
            )
            .unwrap_or_else(|error| panic!("sserafimPulseLights step: {error}"));
        renderer
            .step(
                2,
                &[event(
                    2,
                    "sserafimLights",
                    serde_json::json!({ "amount": 0.5, "duration": 0.5 }),
                )],
                &[],
            )
            .unwrap_or_else(|error| panic!("sserafimLights step: {error}"));
        for tick in 3..=600u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("sserafim scene step {tick}: {error}"));
        }
        let frame = renderer
            .frame()
            .expect("sserafim material pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(non_black > 0, "the sserafim stage must draw visible pixels");
    }

    /// Engine-level 6G sub-slice 2 check: the spookyMansionErect `bgTrees`
    /// sprite-mode rain material must compile and render through the
    /// sprite-space displacement path (its own page, no offscreen camera).
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6G sprite rain shader"]
    fn sprite_rain_stage_renders_through_the_material_pipeline() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "spookeez-erect", "erect")
            .unwrap_or_else(|error| panic!("build spookeez-erect renderer: {error}"));
        for tick in 0..=2u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("sprite-rain scene step {tick}: {error}"));
        }
        let frame = renderer.frame().expect("sprite-rain pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(
            non_black > 0,
            "the sprite-rain stage must draw visible pixels"
        );
    }

    /// Engine-level 6G sub-slice 3 check (NOT a frozen fixture): the
    /// generalized non-bopeebo fixture path enables the Bad/Shit HSV note
    /// overlay, so a Bad hit on a visible player note must feed a non-empty
    /// `LayerShader::Hsv` layer through the kind-2 WGSL branch on the pinned
    /// llvmpipe adapter. The same frame rendered with that layer cleared must
    /// differ, proving the material branch contributes to the output.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6G HSV note shader"]
    fn hsv_desaturated_note_layer_renders_through_the_material_pipeline() {
        let root = assets_root();
        let loaded = load::load_slice_for_song(&root, "senpai", "easy")
            .unwrap_or_else(|error| panic!("load senpai render fixture: {error}"));
        assert!(
            loaded.note_desaturation,
            "the generalized non-bopeebo fixture path must enable the HSV overlay"
        );
        let note = loaded
            .notes
            .iter()
            .filter(|note| !note.opponent && note.note_id == Some(0))
            .min_by(|left, right| left.time.total_cmp(&right.time))
            .expect("senpai chart has a player note");
        let hit_tick = note.time.ceil() as u64;
        let direction = note.direction as i64;

        let (mut scene, pages) = scene::Scene::new(loaded).expect("scene builds");
        let hsv_layer = scene
            .layer_specs()
            .iter()
            .position(|spec| matches!(spec.shader, shaders::LayerShader::Hsv(_)))
            .expect("the scene owns the Hsv note layer");
        for tick in 0..hit_tick {
            scene
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("scene step {tick}: {error}"));
        }
        let hit = TraceEvent::Hit {
            schema: "3",
            kind: "hit",
            sim: hit_tick,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: direction,
            judgement: "bad",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            note_kind: None,
            score_delta: 0,
            health_before: 1.0,
            health_after: 1.0,
            combo_before: 0,
            combo_after: 0,
            combo_max: 0,
            tallies: Default::default(),
        };
        scene
            .step(hit_tick, &[hit], &[])
            .unwrap_or_else(|error| panic!("bad hit step: {error}"));
        let mut harness =
            harness::Harness::new(pages, scene.layer_specs()).expect("llvmpipe harness builds");
        let layers = scene.draw().expect("scene draws");
        assert_eq!(
            layers[hsv_layer].len(),
            1,
            "the visible Bad hit must own one HSV note quad"
        );
        let with_hsv = harness
            .render(&layers, scene.layer_specs())
            .expect("HSV material pipeline renders");
        assert_eq!(with_hsv.len(), harness::FRAME_BYTES, "frame byte length");
        let mut identity_specs = scene.layer_specs().to_vec();
        identity_specs[hsv_layer].shader =
            shaders::LayerShader::Hsv(shaders::Hsv::new(1.0, 1.0, 1.0));
        let with_identity_hsv = harness
            .render(&layers, &identity_specs)
            .expect("identity HSV material pipeline renders");
        assert_ne!(
            with_hsv, with_identity_hsv,
            "the kind-2 Hsv saturation=0.2 WGSL transform must differ from identity saturation=1.0"
        );
        let mut without_hsv = layers;
        without_hsv[hsv_layer].clear();
        let without = harness
            .render(&without_hsv, scene.layer_specs())
            .expect("plain note pipeline renders");
        assert_ne!(
            with_hsv, without,
            "the kind-2 Hsv WGSL branch must change the rendered frame"
        );
    }

    /// Wave 2 engine-level check (NOT a frozen fixture): the tankman
    /// battlefield's animate-atlas DropShadow characters (pico-holding-nene,
    /// otis-speaker, tankman-bloody) must render their limbs into per-character
    /// offscreen targets and draw the rim through the composed output layer.
    /// Clearing the output layers must change the frame, proving the composed
    /// pass (not the old per-page sampling) supplies the character pixels.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the composed DropShadow pass"]
    fn tankman_atlas_drop_shadow_renders_through_the_composed_pass() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "stress-pico", "easy")
            .unwrap_or_else(|error| panic!("build stress-pico renderer: {error}"));
        for tick in 0..=2u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("tankman scene step {tick}: {error}"));
        }
        let groups = harness::composed_group_count(renderer.scene.layer_specs());
        assert_eq!(groups, 3, "three animate-atlas characters compose");
        let outputs: Vec<usize> = renderer
            .scene
            .layer_specs()
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.composed.is_some_and(|composed| composed.output))
            .map(|(index, _)| index)
            .collect();
        assert!(
            outputs.iter().any(|layer| {
                matches!(
                    renderer.scene.layer_specs()[*layer].shader,
                    shaders::LayerShader::DropShadow(shadow) if shadow.mask_page.is_some()
                )
            }),
            "at least the captain mask page is bound on a composed output"
        );

        let layers = renderer.scene.draw().expect("tankman scene draws");
        for layer in &outputs {
            assert_eq!(
                layers[*layer].len(),
                1,
                "each composed group draws one output quad"
            );
        }
        let frame = renderer
            .harness
            .render(&layers, renderer.scene.layer_specs())
            .expect("composed tankman pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(non_black > 0, "the tankman stage draws visible pixels");

        let mut without_composed = layers;
        for layer in &outputs {
            without_composed[*layer].clear();
        }
        let frame_without = renderer
            .harness
            .render(&without_composed, renderer.scene.layer_specs())
            .expect("tankman pipeline renders without the composed characters");
        assert_ne!(
            frame, frame_without,
            "the composed output layers must contribute the character pixels"
        );
    }

    /// Wave 2 engine-level check (NOT a frozen fixture): the sserafim
    /// girlfriend's `Clp` mask must clip her hair through the kind-6/orthogonal
    /// clip uniform path. The clipped layer must own quads and its live
    /// world-space rect; clearing the clipped layer must change the frame.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the sserafim clip path"]
    fn sserafim_clipped_hair_renders_through_the_clip_material() {
        let root = assets_root();
        let mut renderer = SliceRenderer::build_for_song(&root, "spaghetti-default", "easy")
            .unwrap_or_else(|error| panic!("build spaghetti-default renderer: {error}"));
        for tick in 0..=2u64 {
            renderer
                .step(tick, &[], &[])
                .unwrap_or_else(|error| panic!("spaghetti scene step {tick}: {error}"));
        }
        let layers = renderer.scene.draw().expect("spaghetti scene draws");
        let clipped_layers: Vec<usize> = renderer
            .scene
            .layer_specs()
            .iter()
            .enumerate()
            .filter(|(_, spec)| spec.clip.is_some())
            .map(|(index, _)| index)
            .collect();
        assert!(
            !clipped_layers.is_empty(),
            "the sserafim girlfriend hair owns a live clip rect"
        );
        assert!(
            clipped_layers
                .iter()
                .any(|layer| !layers[*layer].is_empty()),
            "the clipped hair layer draws quads"
        );
        let frame = renderer
            .harness
            .render(&layers, renderer.scene.layer_specs())
            .expect("clipped sserafim pipeline renders");
        assert_eq!(frame.len(), harness::FRAME_BYTES, "frame byte length");
        let non_black = frame
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[0] > 8 || pixel[1] > 8 || pixel[2] > 8)
            .count();
        assert!(non_black > 0, "the sserafim stage draws visible pixels");

        let mut without_clipped = layers;
        for layer in &clipped_layers {
            without_clipped[*layer].clear();
        }
        let frame_without = renderer
            .harness
            .render(&without_clipped, renderer.scene.layer_specs())
            .expect("sserafim pipeline renders without the clipped hair");
        assert_ne!(
            frame, frame_without,
            "the clipped layer must contribute visible hair pixels"
        );
    }
}
