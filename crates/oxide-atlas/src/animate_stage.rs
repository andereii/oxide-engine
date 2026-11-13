//! BTA animate-atlas playback resolution.
//!
//! This module reads the two Better TA files as strings — `Animation.json`
//! (`AN`/`SD`/`MD`) and its sibling `spritemap1.json` — and resolves a
//! timeline frame into the ordered draw list of atlas sprites needed by the
//! renderer. It performs no I/O and no rendering; callers supply the file
//! contents.
//!
//! # Coordinate model
//!
//! Matrices are stored as `[a, b, c, d, tx, ty]` and map a local point
//! `(x, y)` to `(a*x + c*y + tx, b*x + d*y + ty)`. Composition follows the
//! Flash/OpenFL concat rule: the local matrix is applied first, then the
//! parent (`parent ∘ local`). A sprite quad's [`SpriteQuad::matrix`] is the
//! accumulated matrix from the sprite's local top-left corner (the atlas
//! entry's unrotated `(width, height)` rectangle) to the stage, excluding the
//! character's whole-sprite position/scale matrix, which the caller applies.
//!
//! A rotated spritemap entry is stored rotated by -90°. Its tile matrix is
//! `[0, -1, 1, 0, 0, width]`, i.e. `(x, y) -> (y, width - x)`: flixel's
//! ANGLE_NEG_90 blit chain `rotateByNegative90()` followed by
//! `translate(0, frame.width)` (FlxFrame.hx:196-199, FlxMatrix.hx:60-65).
//! The local `(width, height)` quad still spans `(height, width)` after the
//! turn. The matrix is composed inside the instance matrix (`MX_asi ∘ tile`)
//! and keeps the sprite's rotation in the returned matrix rather than in the
//! atlas rect. The previous `[0, 1, -1, 0, width, 0]` value was the ANGLE_90
//! chain and rendered rotated dad sprites upside down in the human playtest
//! (ADR-0020 addendum 4). Both quarter-turns produce similar union bounds, so
//! the bounds tests are not the discriminating evidence; the playtest is.
//!
//! # Element matrices
//!
//! `MX` is the authoritative 2D element matrix. For elements without `MX`
//! the resolver falls back, in order, to the element's `M3D` 4x4, an `M3D`
//! nested in the element's `TRP`, the `TRP` x/y translation, and identity.
//! FlxAnimate projects the 4x4 with `[m0, m1, m4, m5, m12, m13]`
//! (`FlxAnimateJson.hx:713-717`; the serialized array is column-major, so
//! indices 12/13 are the translation). A perspective `M3D`, a wrong-length
//! array or a non-finite entry fails closed with [`AnimateStageError::BadMatrix`];
//! no shipped BTA file uses perspective. Elements that do carry `MX` keep
//! their exact previous parse result.
//!
//! # Mask layers
//!
//! A layer with `LT: "Clp"`/`"Clipper"` and no `Clpb` is a mask. Native
//! FlxAnimate marks it `CLIPPER`, sets `visible = false` and never draws it
//! (`Layer.hx:167-178`); draw order is reverse `L` index, so it also never
//! contributes pixels above or below its stack position (`Timeline.hx:308-323`).
//! A layer is clipped only when its `Clpb` names a mask occurring earlier in
//! the same `L` array (native search is backwards, `Layer.hx:138-156`); a
//! `Clpb` that names no such clipper hides the layer just like native
//! `Layer.hx:158-163`. The mask clips the layers *under* it in the Flash
//! stack, i.e. the later `L` entries that draw first.
//!
//! Resolution skips mask layers entirely and gives every quad of a clipped
//! layer an optional [`ResolvedQuad::clip`] rect: the axis-aligned union of
//! the mask layer's own recursively resolved geometry, in the same parent
//! space as the quad matrices. Native clips per pixel through a baked mask;
//! the AABB is the engine's simpler renderer-facing representation for the
//! later renderer wave. An empty mask frame means "no clip" (native renders
//! the layer normally, `FilterRenderer.hx:66-72`); contents fully outside a
//! combined inherited clip keep a zero-size rect. [`AnimateStage::bounds`] and
//! [`AnimateStage::symbol_bounds`] match `flixel-animate`'s
//! `Timeline.getWholeBounds` (excluding invisible clipper layers and narrowing
//! clipped layers with `Timeline.maskBounds`).
//!
//! # Playback semantics
//!
//! - Timeline frame count = the maximum `I + DU` over all layer keyframes.
//! - A keyframe covers timeline frames `I .. I + DU - 1`; the first matching
//!   keyframe in array order wins.
//! - A label lookup inspects layers in array order, stops at the first layer
//!   that has a matching keyframe, and returns the concatenated frame ranges
//!   of every matching keyframe in that layer. Matching follows the observed
//!   reference comparison `name.rtrim() == label` (RENDER-BEHAVIOR §4,
//!   FlxAnimateController.hx L266-290): both sides are right-trimmed,
//!   comparison is case-exact, and no left trim is applied.
//! - Symbol instances (`SI`) recurse with displayed child frame
//!   `FF + (t - keyframe.I)`, then apply their loop mode: `LP` wraps modulo
//!   the child frame count, `PO` clamps to the last frame, `SF` freezes at
//!   `FF`, and unknown modes behave as `LP`.
//! - Layers draw last-to-first; elements within a keyframe draw first-to-last.
//! - Unknown element keys, `ST` and per-element `C` colour transforms are
//!   accepted and ignored for phase 2b; only `SI`/`ASI` produce draws. `TRP`
//!   is consumed only by the matrix fallback documented above.
//!
//! `add_collection` merges another parsed stage for symbol, label and bitmap
//! lookup. The receiver is always searched first, then collections in
//! insertion order. Bounds, frame rate, stage size and frame count stay those
//! of the receiver's main timeline. Label spans found in a merged collection's
//! main timeline are informational for phase 2b: the consumer (`oxide-game`,
//! bf multi-atlas) resolves the main stage first, and only main-timeline labels
//! are rendered in the frozen slice.

use std::collections::HashMap;

use serde_json::{Map, Value};
use thiserror::Error;

use crate::animate::{AnimateAtlas, parse_animate};

/// Maximum nested symbol-instance depth accepted by the resolver.
const MAX_SYMBOL_DEPTH: usize = 64;

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// Errors returned while parsing or resolving a BTA animate stage.
#[derive(Debug, Error)]
pub enum AnimateStageError {
    /// Missing or ill-typed JSON structure in `Animation.json`.
    #[error("invalid animate stage at `{path}`: {message}")]
    Invalid { path: String, message: String },
    /// A matrix (`MX` or `M3D`) was malformed, non-finite, or perspective.
    #[error("bad matrix at `{path}`: {message}")]
    BadMatrix { path: String, message: String },
    /// An `ASI` element referenced a sprite absent from the spritemap.
    #[error("unknown bitmap `{name}` referenced at `{path}`")]
    UnknownBitmap { name: String, path: String },
    /// An `SI` element referenced a symbol absent from every loaded stage.
    #[error("unknown symbol `{name}` referenced at `{path}`")]
    UnknownSymbol { name: String, path: String },
    /// A symbol directly or indirectly instantiates itself, or nesting is
    /// deeper than the module's symbol-depth limit.
    #[error("symbol recursion is not finite at `{path}`: {chain}")]
    SymbolRecursion { path: String, chain: String },
    /// A keyframe has an invalid start/duration range.
    #[error("bad keyframe geometry at `{path}` (start={start}, duration={duration})")]
    BadKeyframe {
        path: String,
        start: usize,
        duration: usize,
    },
    /// A requested timeline frame is outside the timeline.
    #[error("timeline frame {frame} is out of range 0..{frame_count}")]
    FrameOutOfRange { frame: usize, frame_count: usize },
    /// The sibling `spritemap1.json` could not be parsed.
    #[error("spritemap json is invalid: {message}")]
    Spritemap { message: String },
}

/// One atlas sprite instance in a resolved frame, in draw order.
#[derive(Debug, Clone, PartialEq)]
pub struct SpriteQuad {
    /// `ASI.N`: the `ATLAS.SPRITES` entry name (the Animate frame digit name).
    pub bitmap_name: String,
    /// Parsed spritemap entry rect `[x, y, w, h]`. `x`/`y` are UV offsets;
    /// the drawn local quad is `(0, 0, w, h)` transformed by `matrix`.
    pub atlas_rect: [i32; 4],
    /// Accumulated local-to-stage matrix, including the rotated tile matrix.
    pub matrix: [f64; 6],
    /// Whether the spritemap entry is stored rotated.
    pub rotated: bool,
}

/// One resolved draw plus the mask region that applies to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedQuad {
    /// The sprite quad to draw.
    pub quad: SpriteQuad,
    /// Optional mask clip in the same parent space as [`SpriteQuad::matrix`]:
    /// `[x, y, width, height]`. `None` when no mask governs this quad; a
    /// zero-size rect means the inherited clip removed all of it.
    ///
    /// This is the axis-aligned union of the mask layer's resolved geometry,
    /// not the exact per-pixel mask native FlxAnimate bakes.
    pub clip: Option<[f64; 4]>,
}

/// A parsed BTA stage, optionally merged with additional atlas collections.
#[derive(Debug, Clone)]
pub struct AnimateStage {
    data: StageData,
    collections: Vec<StageData>,
    frame_rate: u32,
    stage_size: (u32, u32),
    bounds: [f64; 4],
}

#[derive(Debug, Clone)]
struct StageData {
    main: Timeline,
    symbols: HashMap<String, Timeline>,
    sprites: HashMap<String, SpriteEntry>,
}

#[derive(Debug, Clone, Copy)]
struct SpriteEntry {
    rect: [i32; 4],
    rotated: bool,
}

#[derive(Debug, Clone)]
struct Timeline {
    layers: Vec<Layer>,
    frame_count: usize,
}

#[derive(Debug, Clone)]
struct Layer {
    name: Option<String>,
    is_clipper: bool,
    clipped_by: Option<String>,
    frames: Vec<Keyframe>,
}

/// Outcome of resolving a clipped layer's `Clpb` mask reference.
#[derive(Debug, Clone, Copy)]
enum MaskRect {
    /// `Clpb` names no preceding clipper layer; native hides the layer.
    Missing,
    /// The clipper exists but has no geometry at this frame; native draws
    /// the layer normally.
    Empty,
    /// Mask AABB in the current parent space: `[x, y, width, height]`.
    Rect([f64; 4]),
}

#[derive(Debug, Clone)]
struct Keyframe {
    label: Option<String>,
    start: usize,
    duration: usize,
    elements: Vec<Element>,
}

#[derive(Debug, Clone)]
enum Element {
    Symbol(SymbolInstance),
    Atlas(AtlasSprite),
}

#[derive(Debug, Clone)]
struct SymbolInstance {
    symbol: String,
    first_frame: usize,
    loop_mode: LoopMode,
    matrix: [f64; 6],
    path: String,
}

#[derive(Debug, Clone)]
struct AtlasSprite {
    name: String,
    matrix: [f64; 6],
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    Loop,
    PlayOnce,
    SingleFrame,
}

impl AnimateStage {
    /// Parses `Animation.json` and `spritemap1.json` file contents from
    /// memory. The whole main timeline is resolved once to validate symbol
    /// and bitmap references and to cache [`AnimateStage::bounds`].
    pub fn parse(animation_json: &str, spritemap_json: &str) -> Result<Self, AnimateStageError> {
        let animation = parse_json(animation_json)?;
        let spritemap =
            parse_animate(spritemap_json).map_err(|err| AnimateStageError::Spritemap {
                message: err.to_string(),
            })?;

        let (data, frame_rate, stage_size) = parse_stage(&animation, &spritemap)?;
        let resolver = Resolver {
            stages: vec![&data],
        };
        let bounds = resolver.compute_bounds(&data.main)?;

        Ok(Self {
            data,
            collections: Vec::new(),
            frame_rate,
            stage_size,
            bounds,
        })
    }

    /// `MD.FRT`, the timeline frame rate in frames per second.
    pub fn frame_rate(&self) -> u32 {
        self.frame_rate
    }

    /// `MD.W`/`MD.H`, the Animate stage size in pixels.
    pub fn stage_size(&self) -> (u32, u32) {
        self.stage_size
    }

    /// Whole main-timeline content bounds at scale 1: `[x, y, width, height]`.
    /// Only the receiver's main timeline contributes, even after
    /// [`AnimateStage::add_collection`].
    pub fn bounds(&self) -> [f64; 4] {
        self.bounds
    }

    /// Main timeline frame count: the maximum `I + DU` over all main layers.
    pub fn frame_count(&self) -> usize {
        self.data.main.frame_count
    }

    /// Frame count of a named symbol timeline, or `None` when no loaded
    /// collection defines it.
    ///
    /// Native rule implemented: animation data with `animType: "symbol"`
    /// plays the symbol's OWN timeline, whose length is the maximum `I + DU`
    /// over that symbol's layer keyframes (the same keyframe expansion as the
    /// main timeline, RENDER-BEHAVIOR §4). Symbols are looked up in the
    /// receiver first and then in merged collections in insertion order.
    pub fn symbol_frame_count(&self, symbol: &str) -> Option<usize> {
        self.resolver()
            .find_symbol(symbol)
            .map(|timeline| timeline.frame_count)
    }

    /// Frame list for a main-timeline label. The receiver's main timeline is
    /// searched first, then each added collection in insertion order. Within a
    /// timeline the first layer containing any match wins. Matching follows the
    /// observed reference comparison `name.rtrim() == label` (RENDER-BEHAVIOR
    /// §4 / FlxAnimateController.hx L266-290): both sides are right-trimmed,
    /// comparison is case-exact, and no left trim is applied.
    ///
    /// Spans found in a merged collection's main timeline are informational for
    /// phase 2b: the consumer (`oxide-game`, bf multi-atlas) resolves the main
    /// stage first, and only main-timeline labels are rendered in the frozen
    /// slice. This does not change the lookup or return type.
    pub fn label_frames(&self, label: &str) -> Option<Vec<usize>> {
        if let Some(frames) = label_frames_in(&self.data.main, label) {
            return Some(frames);
        }
        for collection in &self.collections {
            if let Some(frames) = label_frames_in(&collection.main, label) {
                return Some(frames);
            }
        }
        None
    }

    /// Resolves one frame of the main timeline into draw-ordered sprites.
    /// Mask layers produce no draws; use
    /// [`AnimateStage::resolve_main_frame_with_clips`] when the mask regions
    /// are needed.
    pub fn resolve_main_frame(
        &self,
        timeline_frame: usize,
    ) -> Result<Vec<SpriteQuad>, AnimateStageError> {
        Ok(self
            .resolve_main_frame_with_clips(timeline_frame)?
            .into_iter()
            .map(|resolved| resolved.quad)
            .collect())
    }

    /// Resolves one frame of the main timeline into draw-ordered sprites with
    /// each quad's optional mask clip region.
    pub fn resolve_main_frame_with_clips(
        &self,
        timeline_frame: usize,
    ) -> Result<Vec<ResolvedQuad>, AnimateStageError> {
        let frame_count = self.data.main.frame_count;
        if timeline_frame >= frame_count {
            return Err(AnimateStageError::FrameOutOfRange {
                frame: timeline_frame,
                frame_count,
            });
        }

        let resolver = self.resolver();
        let mut quads = Vec::new();
        let mut stack = Vec::new();
        resolver.resolve_timeline(
            &self.data.main,
            timeline_frame,
            IDENTITY,
            0,
            &mut stack,
            None,
            &mut quads,
        )?;
        Ok(quads)
    }

    /// Resolves one frame of a named symbol timeline into draw-ordered
    /// sprites, in the symbol's local coordinates. The symbol may live in the
    /// receiver or in any added collection; nested references use the same
    /// main-first lookup order. Mask layers produce no draws.
    pub fn resolve_symbol_frame(
        &self,
        symbol: &str,
        timeline_frame: usize,
    ) -> Result<Vec<SpriteQuad>, AnimateStageError> {
        Ok(self
            .resolve_symbol_frame_with_clips(symbol, timeline_frame)?
            .into_iter()
            .map(|resolved| resolved.quad)
            .collect())
    }

    /// Resolves one frame of a named symbol timeline into draw-ordered
    /// sprites with each quad's optional mask clip region, in the symbol's
    /// local coordinates.
    pub fn resolve_symbol_frame_with_clips(
        &self,
        symbol: &str,
        timeline_frame: usize,
    ) -> Result<Vec<ResolvedQuad>, AnimateStageError> {
        let resolver = self.resolver();
        let timeline =
            resolver
                .find_symbol(symbol)
                .ok_or_else(|| AnimateStageError::UnknownSymbol {
                    name: symbol.to_string(),
                    path: "resolve_symbol_frame".to_string(),
                })?;
        if timeline_frame >= timeline.frame_count {
            return Err(AnimateStageError::FrameOutOfRange {
                frame: timeline_frame,
                frame_count: timeline.frame_count,
            });
        }

        let mut quads = Vec::new();
        let mut stack = vec![symbol.to_string()];
        resolver.resolve_timeline(
            timeline,
            timeline_frame,
            IDENTITY,
            1,
            &mut stack,
            None,
            &mut quads,
        )?;
        Ok(quads)
    }

    /// Whole bounds of a named symbol timeline at scale 1: `[x, y, width, height]`,
    /// in the symbol's local coordinates (`Timeline.getWholeBounds`). Searches
    /// the receiver first, then any collections added via [`AnimateStage::add_collection`].
    pub fn symbol_bounds(&self, symbol: &str) -> Result<[f64; 4], AnimateStageError> {
        let resolver = self.resolver();
        let timeline =
            resolver
                .find_symbol(symbol)
                .ok_or_else(|| AnimateStageError::UnknownSymbol {
                    name: symbol.to_string(),
                    path: "symbol_bounds".to_string(),
                })?;
        resolver.compute_bounds(timeline)
    }

    /// Merges `other` for symbol, label and bitmap lookup after this stage's
    /// own data. Existing bounds and frame metadata are unchanged.
    pub fn add_collection(&mut self, other: AnimateStage) {
        self.collections.push(other.data);
        self.collections.extend(other.collections);
    }

    fn resolver(&self) -> Resolver<'_> {
        let mut stages = Vec::with_capacity(1 + self.collections.len());
        stages.push(&self.data);
        stages.extend(self.collections.iter());
        Resolver { stages }
    }
}

struct Resolver<'a> {
    /// Main stage first, then merged collections in insertion order.
    stages: Vec<&'a StageData>,
}

struct BoundsState {
    stack: Vec<String>,
    cache: HashMap<(*const Timeline, usize), [f64; 4]>,
}

impl<'a> Resolver<'a> {
    fn find_symbol(&self, name: &str) -> Option<&'a Timeline> {
        self.stages.iter().find_map(|stage| stage.symbols.get(name))
    }

    fn find_sprite(&self, name: &str) -> Option<&'a SpriteEntry> {
        self.stages.iter().find_map(|stage| stage.sprites.get(name))
    }

    /// Ports `flixel-animate`'s `Timeline.getWholeBounds` + `Timeline.getBounds`:
    /// unions non-empty frame bounds across `0..timeline.frame_count`, memoizing
    /// untransformed local frame bounds by timeline pointer and frame index.
    fn compute_bounds(&self, timeline: &Timeline) -> Result<[f64; 4], AnimateStageError> {
        let mut state = BoundsState {
            stack: Vec::new(),
            cache: HashMap::new(),
        };
        let mut rect = [0.0; 4];
        let mut first = true;

        for frame in 0..timeline.frame_count {
            let frame_bounds = self.timeline_bounds(timeline, frame, None, 0, &mut state)?;
            if is_empty_rect(frame_bounds) {
                continue;
            }
            if first {
                rect = frame_bounds;
                first = false;
            } else {
                rect = expand_bounds(rect, frame_bounds);
            }
        }

        Ok(rect)
    }

    fn timeline_bounds(
        &self,
        timeline: &Timeline,
        frame: usize,
        matrix: Option<[f64; 6]>,
        depth: usize,
        state: &mut BoundsState,
    ) -> Result<[f64; 4], AnimateStageError> {
        let key = (timeline as *const Timeline, frame);
        if let Some(&cached) = state.cache.get(&key) {
            return Ok(apply_matrix_to_rect(cached, matrix));
        }

        let mut rect = [0.0; 4];
        let mut first = true;

        for (layer_index, layer) in timeline.layers.iter().enumerate() {
            if layer.is_clipper {
                continue;
            }
            if let Some(name) = layer.clipped_by.as_deref()
                && find_clipper_index(timeline, layer_index, name).is_none()
            {
                continue;
            }
            let Some(keyframe) = layer.frame_at(frame) else {
                continue;
            };
            if keyframe.elements.is_empty() {
                continue;
            }

            let frame_bounds =
                self.layer_bounds(timeline, layer_index, frame, None, depth, state)?;
            if is_empty_rect(frame_bounds) {
                continue;
            }
            if first {
                rect = frame_bounds;
                first = false;
            } else {
                rect = expand_bounds(rect, frame_bounds);
            }
        }

        state.cache.insert(key, rect);
        Ok(apply_matrix_to_rect(rect, matrix))
    }

    fn layer_bounds(
        &self,
        timeline: &Timeline,
        layer_index: usize,
        frame: usize,
        matrix: Option<[f64; 6]>,
        depth: usize,
        state: &mut BoundsState,
    ) -> Result<[f64; 4], AnimateStageError> {
        let layer = &timeline.layers[layer_index];
        let Some(keyframe) = layer.frame_at(frame) else {
            return Ok(apply_matrix_to_rect([0.0; 4], matrix));
        };
        if keyframe.elements.is_empty() {
            return Ok(apply_matrix_to_rect([0.0; 4], matrix));
        }

        let rel_frame = frame - keyframe.start;
        let mut rect =
            self.element_bounds(&keyframe.elements[0], rel_frame, matrix, depth, state)?;
        for element in &keyframe.elements[1..] {
            let elem_rect = self.element_bounds(element, rel_frame, matrix, depth, state)?;
            rect = expand_bounds(rect, elem_rect);
        }

        if let Some(name) = layer.clipped_by.as_deref()
            && let Some(clipper_index) = find_clipper_index(timeline, layer_index, name)
        {
            let masker = self.layer_bounds(timeline, clipper_index, frame, matrix, depth, state)?;
            rect = mask_bounds(rect, masker);
        }

        Ok(rect)
    }

    fn element_bounds(
        &self,
        element: &Element,
        rel_frame: usize,
        matrix: Option<[f64; 6]>,
        depth: usize,
        state: &mut BoundsState,
    ) -> Result<[f64; 4], AnimateStageError> {
        match element {
            Element::Atlas(sprite) => {
                let entry = self.find_sprite(&sprite.name).ok_or_else(|| {
                    AnimateStageError::UnknownBitmap {
                        name: sprite.name.clone(),
                        path: sprite.path.clone(),
                    }
                })?;
                let mut rect = [0.0, 0.0, entry.rect[2] as f64, entry.rect[3] as f64];
                if entry.rotated {
                    rect = apply_matrix_to_rect(rect, Some(rotated_tile_matrix(entry.rect[2])));
                }
                rect = apply_matrix_to_rect(rect, Some(sprite.matrix));
                Ok(apply_matrix_to_rect(rect, matrix))
            }
            Element::Symbol(instance) => {
                if depth >= MAX_SYMBOL_DEPTH || state.stack.iter().any(|s| s == &instance.symbol) {
                    return Err(AnimateStageError::SymbolRecursion {
                        path: instance.path.clone(),
                        chain: format_recursion_chain(&state.stack, &instance.symbol),
                    });
                }
                let child_timeline = self.find_symbol(&instance.symbol).ok_or_else(|| {
                    AnimateStageError::UnknownSymbol {
                        name: instance.symbol.clone(),
                        path: instance.path.clone(),
                    }
                })?;
                let child_frame = instance.child_frame(0, rel_frame, child_timeline.frame_count)?;
                let target_matrix = Some(match matrix {
                    Some(parent) => concat(parent, instance.matrix),
                    None => instance.matrix,
                });
                state.stack.push(instance.symbol.clone());
                let result = self.timeline_bounds(
                    child_timeline,
                    child_frame,
                    target_matrix,
                    depth + 1,
                    state,
                );
                state.stack.pop();
                result
            }
        }
    }

    // Resolution threads the parent matrix, recursion depth, symbol stack and
    // inherited clip through every nested call; bundling them into a context
    // struct would only move the same state without a second consumer.
    #[allow(clippy::too_many_arguments)]
    fn resolve_timeline(
        &self,
        timeline: &Timeline,
        frame: usize,
        parent: [f64; 6],
        depth: usize,
        stack: &mut Vec<String>,
        inherited_clip: Option<[f64; 4]>,
        out: &mut Vec<ResolvedQuad>,
    ) -> Result<(), AnimateStageError> {
        for layer_index in (0..timeline.layers.len()).rev() {
            let layer = &timeline.layers[layer_index];
            // Native clipper layers are invisible and never contribute
            // geometry; their only role is the clip rect of layers naming
            // them in `Clpb` (Layer.hx:167-178).
            if layer.is_clipper {
                continue;
            }
            let Some(keyframe) = layer.frame_at(frame) else {
                continue;
            };
            let clip = match layer.clipped_by.as_deref() {
                None => inherited_clip,
                Some(name) => match self.mask_rect(
                    timeline,
                    layer_index,
                    name,
                    frame,
                    parent,
                    depth,
                    stack,
                )? {
                    // Native hides a clipped layer whose clipper is absent
                    // (Layer.hx:158-163).
                    MaskRect::Missing => continue,
                    // An existing mask with no geometry leaves the layer
                    // unclipped (FilterRenderer.hx:66-72).
                    MaskRect::Empty => inherited_clip,
                    MaskRect::Rect(rect) => intersect_clip(inherited_clip, Some(rect)),
                },
            };
            self.resolve_elements(
                &keyframe.elements,
                keyframe.start,
                frame,
                parent,
                depth,
                stack,
                clip,
                out,
            )?;
        }
        Ok(())
    }

    /// Resolves one mask layer's geometry at `frame` and returns its AABB in
    /// the current parent space. Mask elements are resolved recursively into a
    /// scratch list, so mask-nested symbols contribute to the rect but never
    /// to the draw list.
    // Mirrors resolve_timeline's threaded resolution state for a single mask
    // layer; see the comment above resolve_timeline.
    #[allow(clippy::too_many_arguments)]
    fn mask_rect(
        &self,
        timeline: &Timeline,
        clipped_index: usize,
        name: &str,
        frame: usize,
        parent: [f64; 6],
        depth: usize,
        stack: &mut Vec<String>,
    ) -> Result<MaskRect, AnimateStageError> {
        let Some(clipper_index) = find_clipper_index(timeline, clipped_index, name) else {
            return Ok(MaskRect::Missing);
        };
        let Some(keyframe) = timeline.layers[clipper_index].frame_at(frame) else {
            return Ok(MaskRect::Empty);
        };
        let mut quads = Vec::new();
        self.resolve_elements(
            &keyframe.elements,
            keyframe.start,
            frame,
            parent,
            depth,
            stack,
            None,
            &mut quads,
        )?;

        let mut bounds: Option<[f64; 4]> = None;
        for resolved in &quads {
            let rect = quad_corners(&resolved.quad);
            bounds = Some(match bounds {
                None => rect,
                Some(current) => [
                    current[0].min(rect[0]),
                    current[1].min(rect[1]),
                    current[2].max(rect[2]),
                    current[3].max(rect[3]),
                ],
            });
        }
        Ok(match bounds {
            None => MaskRect::Empty,
            Some([min_x, min_y, max_x, max_y]) => {
                MaskRect::Rect([min_x, min_y, max_x - min_x, max_y - min_y])
            }
        })
    }

    // Mirrors resolve_timeline's threaded resolution state for one keyframe's
    // elements; see the comment above resolve_timeline.
    #[allow(clippy::too_many_arguments)]
    fn resolve_elements(
        &self,
        elements: &[Element],
        keyframe_start: usize,
        frame: usize,
        parent: [f64; 6],
        depth: usize,
        stack: &mut Vec<String>,
        clip: Option<[f64; 4]>,
        out: &mut Vec<ResolvedQuad>,
    ) -> Result<(), AnimateStageError> {
        for element in elements {
            match element {
                Element::Atlas(sprite) => {
                    let entry = self.find_sprite(&sprite.name).ok_or_else(|| {
                        AnimateStageError::UnknownBitmap {
                            name: sprite.name.clone(),
                            path: sprite.path.clone(),
                        }
                    })?;
                    let matrix = concat(parent, sprite.matrix);
                    let matrix = if entry.rotated {
                        concat(matrix, rotated_tile_matrix(entry.rect[2]))
                    } else {
                        matrix
                    };
                    out.push(ResolvedQuad {
                        quad: SpriteQuad {
                            bitmap_name: sprite.name.clone(),
                            atlas_rect: entry.rect,
                            matrix,
                            rotated: entry.rotated,
                        },
                        clip,
                    });
                }
                Element::Symbol(instance) => {
                    if depth >= MAX_SYMBOL_DEPTH
                        || stack.iter().any(|active| active == &instance.symbol)
                    {
                        return Err(AnimateStageError::SymbolRecursion {
                            path: instance.path.clone(),
                            chain: format_recursion_chain(stack, &instance.symbol),
                        });
                    }

                    let child_timeline = self.find_symbol(&instance.symbol).ok_or_else(|| {
                        AnimateStageError::UnknownSymbol {
                            name: instance.symbol.clone(),
                            path: instance.path.clone(),
                        }
                    })?;
                    let child_frame =
                        instance.child_frame(keyframe_start, frame, child_timeline.frame_count)?;
                    stack.push(instance.symbol.clone());
                    let result = self.resolve_timeline(
                        child_timeline,
                        child_frame,
                        concat(parent, instance.matrix),
                        depth + 1,
                        stack,
                        // A layer clip covers the whole symbol the layer
                        // draws; inner masks intersect with it.
                        clip,
                        out,
                    );
                    stack.pop();
                    result?;
                }
            }
        }
        Ok(())
    }
}

impl SymbolInstance {
    fn child_frame(
        &self,
        keyframe_start: usize,
        timeline_frame: usize,
        frame_count: usize,
    ) -> Result<usize, AnimateStageError> {
        let offset = timeline_frame.saturating_sub(keyframe_start);
        let raw = self.first_frame.saturating_add(offset);
        match self.loop_mode {
            LoopMode::Loop => Ok(raw % frame_count),
            LoopMode::PlayOnce => Ok(raw.min(frame_count - 1)),
            LoopMode::SingleFrame => {
                if self.first_frame >= frame_count {
                    return Err(AnimateStageError::FrameOutOfRange {
                        frame: self.first_frame,
                        frame_count,
                    });
                }
                Ok(self.first_frame)
            }
        }
    }
}

impl Layer {
    fn frame_at(&self, frame: usize) -> Option<&Keyframe> {
        self.frames
            .iter()
            .find(|keyframe| frame >= keyframe.start && frame < keyframe.start + keyframe.duration)
    }
}

fn label_frames_in(timeline: &Timeline, label: &str) -> Option<Vec<usize>> {
    let wanted = label.trim_end();
    for layer in &timeline.layers {
        let mut frames = Vec::new();
        for keyframe in &layer.frames {
            if let Some(candidate) = &keyframe.label
                && candidate.trim_end() == wanted
            {
                frames.extend(keyframe.start..keyframe.start + keyframe.duration);
            }
        }
        if !frames.is_empty() {
            return Some(frames);
        }
    }
    None
}

fn concat(parent: [f64; 6], local: [f64; 6]) -> [f64; 6] {
    [
        parent[0] * local[0] + parent[2] * local[1],
        parent[1] * local[0] + parent[3] * local[1],
        parent[0] * local[2] + parent[2] * local[3],
        parent[1] * local[2] + parent[3] * local[3],
        parent[0] * local[4] + parent[2] * local[5] + parent[4],
        parent[1] * local[4] + parent[3] * local[5] + parent[5],
    ]
}

/// Intersects two `[x, y, width, height]` clips. An empty intersection is
/// preserved as a zero-size rect at the clipped origin so callers can still
/// tell that a clip applied.
fn intersect_clip(outer: Option<[f64; 4]>, inner: Option<[f64; 4]>) -> Option<[f64; 4]> {
    match (outer, inner) {
        (None, clip) | (clip, None) => clip,
        (Some(a), Some(b)) => {
            let min_x = a[0].max(b[0]);
            let min_y = a[1].max(b[1]);
            let max_x = (a[0] + a[2]).min(b[0] + b[2]);
            let max_y = (a[1] + a[3]).min(b[1] + b[3]);
            if max_x <= min_x || max_y <= min_y {
                Some([min_x, min_y, 0.0, 0.0])
            } else {
                Some([min_x, min_y, max_x - min_x, max_y - min_y])
            }
        }
    }
}

/// Matrix restoring an upright sprite from a spritemap entry stored rotated
/// by -90° using flixel's ANGLE_NEG_90 blit chain: `rotateByNegative90()`
/// then `translate(0, frame.width)` (`FlxFrame.hx:196-199`,
/// `FlxMatrix.hx:60-65`). `width` is the stored entry width; the local
/// `(width, height)` quad maps to `(y, width - x)` and spans `(height, width)`.
/// The previous `[0, 1, -1, 0, width, 0]` value was the ANGLE_90 chain
/// (rendered dad's head upside down in the human playtest).
fn rotated_tile_matrix(width: i32) -> [f64; 6] {
    [0.0, -1.0, 1.0, 0.0, 0.0, width as f64]
}

fn quad_corners(quad: &SpriteQuad) -> [f64; 4] {
    let [_, _, width, height] = quad.atlas_rect;
    let width = width as f64;
    let height = height as f64;
    let points = [
        apply(quad.matrix, 0.0, 0.0),
        apply(quad.matrix, width, 0.0),
        apply(quad.matrix, width, height),
        apply(quad.matrix, 0.0, height),
    ];
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for (x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    [min_x, min_y, max_x, max_y]
}

fn apply(matrix: [f64; 6], x: f64, y: f64) -> (f64, f64) {
    (
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    )
}

fn find_clipper_index(timeline: &Timeline, clipped_index: usize, name: &str) -> Option<usize> {
    (0..clipped_index).rev().find(|&index| {
        let layer = &timeline.layers[index];
        layer.is_clipper && layer.name.as_deref() == Some(name)
    })
}

fn format_recursion_chain(stack: &[String], symbol: &str) -> String {
    let mut chain = stack.join(" -> ");
    if !chain.is_empty() {
        chain.push_str(" -> ");
    }
    chain.push_str(symbol);
    chain
}

fn is_empty_rect(rect: [f64; 4]) -> bool {
    rect[2] <= 0.0 || rect[3] <= 0.0
}

fn expand_bounds(base: [f64; 4], expanded: [f64; 4]) -> [f64; 4] {
    let x = base[0].min(expanded[0]);
    let y = base[1].min(expanded[1]);
    let w = (base[0] + base[2]).max(expanded[0] + expanded[2]) - x;
    let h = (base[1] + base[3]).max(expanded[1] + expanded[3]) - y;
    [x, y, w, h]
}

fn mask_bounds(masked: [f64; 4], masker: [f64; 4]) -> [f64; 4] {
    if is_empty_rect(masker) {
        return masked;
    }
    let x1 = masked[0].max(masker[0]);
    let y1 = masked[1].max(masker[1]);
    let x2 = (masked[0] + masked[2]).min(masker[0] + masker[2]);
    let y2 = (masked[1] + masked[3]).min(masker[1] + masker[3]);
    if x2 <= x1 || y2 <= y1 {
        return [0.0, 0.0, 0.0, 0.0];
    }
    [x1, y1, x2 - x1, y2 - y1]
}

fn apply_matrix_to_rect(rect: [f64; 4], matrix: Option<[f64; 6]>) -> [f64; 4] {
    let Some(m) = matrix else {
        return rect;
    };
    if is_empty_rect(rect) {
        return [m[4], m[5], 0.0, 0.0];
    }
    let left = rect[0];
    let top = rect[1];
    let right = rect[0] + rect[2];
    let bottom = rect[1] + rect[3];
    let corners = [
        (m[0] * left + m[2] * top, m[1] * left + m[3] * top),
        (m[0] * right + m[2] * top, m[1] * right + m[3] * top),
        (m[0] * right + m[2] * bottom, m[1] * right + m[3] * bottom),
        (m[0] * left + m[2] * bottom, m[1] * left + m[3] * bottom),
    ];
    let mut tx0 = corners[0].0;
    let mut tx1 = tx0;
    let mut ty0 = corners[0].1;
    let mut ty1 = ty0;
    for (tx, ty) in corners.into_iter().skip(1) {
        tx0 = tx0.min(tx);
        ty0 = ty0.min(ty);
        tx1 = tx1.max(tx);
        ty1 = ty1.max(ty);
    }
    [tx0 + m[4], ty0 + m[5], tx1 - tx0, ty1 - ty0]
}

/// Resolves the animate stage size from `MD.W`/`MD.H`, falling back to the
/// spritemap's `meta.size`.
///
/// Observed shipped shape (M6-CONTRACT §11.5): `shared/images/characters/
/// sserafim/sakura/Animation.json` has `MD: {"FRT":24}` without `W`/`H`; its
/// sibling `spritemap1.json` declares `meta.size = {w: 2043, h: 1950}`. The
/// fallback is data-equivalent on every file that carries both sources, and
/// present `MD.W`/`MD.H` win, so existing parse results are unchanged.
/// Missing sizes from both sources fail closed and name both paths.
fn stage_size(
    metadata: &Map<String, Value>,
    spritemap: &AnimateAtlas,
) -> Result<(u32, u32), AnimateStageError> {
    if let (Some(width), Some(height)) = (metadata.get("W"), metadata.get("H")) {
        return Ok((
            as_u32(width, "Animation.json.MD.W")?,
            as_u32(height, "Animation.json.MD.H")?,
        ));
    }
    if let Some([width, height]) = spritemap.page_size {
        return Ok((width, height));
    }
    Err(AnimateStageError::Invalid {
        path: "Animation.json.MD".to_string(),
        message: "stage size missing: `MD.W`/`MD.H` are absent and the sibling \
                  `spritemap1.json.meta.size` has no integer `w`/`h`"
            .to_string(),
    })
}

struct AnimateKeys {
    metadata: &'static str,
    framerate: &'static str,
    animation: &'static str,
    timeline: &'static str,
    symbol_dictionary: &'static str,
    symbols: &'static str,
    symbol_name: &'static str,
    layers: &'static str,
    layer_name: &'static str,
    layer_type: &'static str,
    clipped_by: &'static str,
    frames: &'static str,
    frame_index: &'static str,
    frame_duration: &'static str,
    frame_name: &'static str,
    elements: &'static str,
    symbol_instance: &'static str,
    atlas_sprite_instance: &'static str,
    si_symbol_name: &'static str,
    si_first_frame: &'static str,
    si_loop: &'static str,
    asi_name: &'static str,
}

const MINIFIED_KEYS: AnimateKeys = AnimateKeys {
    metadata: "MD",
    framerate: "FRT",
    animation: "AN",
    timeline: "TL",
    symbol_dictionary: "SD",
    symbols: "S",
    symbol_name: "SN",
    layers: "L",
    layer_name: "LN",
    layer_type: "LT",
    clipped_by: "Clpb",
    frames: "FR",
    frame_index: "I",
    frame_duration: "DU",
    frame_name: "N",
    elements: "E",
    symbol_instance: "SI",
    atlas_sprite_instance: "ASI",
    si_symbol_name: "SN",
    si_first_frame: "FF",
    si_loop: "LP",
    asi_name: "N",
};

const VERBOSE_KEYS: AnimateKeys = AnimateKeys {
    metadata: "metadata",
    framerate: "framerate",
    animation: "ANIMATION",
    timeline: "TIMELINE",
    symbol_dictionary: "SYMBOL_DICTIONARY",
    symbols: "Symbols",
    symbol_name: "SYMBOL_name",
    layers: "LAYERS",
    layer_name: "Layer_name",
    layer_type: "Layer_type",
    clipped_by: "Clipped_by",
    frames: "Frames",
    frame_index: "index",
    frame_duration: "duration",
    frame_name: "name",
    elements: "elements",
    symbol_instance: "SYMBOL_Instance",
    atlas_sprite_instance: "ATLAS_SPRITE_instance",
    si_symbol_name: "SYMBOL_name",
    si_first_frame: "firstFrame",
    si_loop: "loop",
    asi_name: "name",
};

const M3D_OBJECT_KEYS: [&str; 16] = [
    "m00", "m01", "m02", "m03", "m10", "m11", "m12", "m13", "m20", "m21", "m22", "m23", "m30",
    "m31", "m32", "m33",
];

fn parse_stage(
    animation: &Value,
    spritemap: &AnimateAtlas,
) -> Result<(StageData, u32, (u32, u32)), AnimateStageError> {
    let root = as_object(animation, "Animation.json")?;
    let keys = if root.contains_key("MD") || root.contains_key("AN") {
        &MINIFIED_KEYS
    } else if root.contains_key("metadata") || root.contains_key("ANIMATION") {
        &VERBOSE_KEYS
    } else {
        &MINIFIED_KEYS
    };

    let meta_path = format!("Animation.json.{}", keys.metadata);
    let metadata = as_object(field(root, keys.metadata, "Animation.json")?, &meta_path)?;
    let frame_rate = as_u32(
        field(metadata, keys.framerate, &meta_path)?,
        &format!("{}.{}", keys.metadata, keys.framerate),
    )?;
    let (width, height) = stage_size(metadata, spritemap)?;

    let anim_path = format!("Animation.json.{}", keys.animation);
    let animate = as_object(field(root, keys.animation, "Animation.json")?, &anim_path)?;
    let timeline_path = format!("{}.{}", keys.animation, keys.timeline);
    let timeline_value = field(animate, keys.timeline, keys.animation)?;
    let main = parse_timeline(timeline_value, &timeline_path, keys)?;

    let mut symbols = HashMap::new();
    if let Some(dictionary) = root.get(keys.symbol_dictionary) {
        let dict_path = format!("Animation.json.{}", keys.symbol_dictionary);
        let dictionary = as_object(dictionary, &dict_path)?;
        if let Some(symbol_array) = dictionary.get(keys.symbols) {
            let symbols_path = format!("{}.{}", keys.symbol_dictionary, keys.symbols);
            for (index, symbol_value) in as_array(symbol_array, &symbols_path)?.iter().enumerate() {
                let path = format!("{symbols_path}[{index}]");
                let symbol = as_object(symbol_value, &path)?;
                let name = as_string(
                    field(symbol, keys.symbol_name, &path)?,
                    &format!("{path}.{}", keys.symbol_name),
                )?;
                let timeline = parse_timeline(
                    field(symbol, keys.timeline, &path)?,
                    &format!("{path}.{}", keys.timeline),
                    keys,
                )?;
                symbols.insert(name, timeline);
            }
        }
    }

    let mut sprites = HashMap::with_capacity(spritemap.sprites.len());
    for sprite in &spritemap.sprites {
        if sprite.w < 0 || sprite.h < 0 {
            return Err(AnimateStageError::Invalid {
                path: format!("ATLAS.SPRITES.{}", sprite.name),
                message: "negative sprite width or height".to_string(),
            });
        }
        sprites.insert(
            sprite.name.clone(),
            SpriteEntry {
                rect: [sprite.x, sprite.y, sprite.w, sprite.h],
                rotated: sprite.rotated,
            },
        );
    }

    Ok((
        StageData {
            main,
            symbols,
            sprites,
        },
        frame_rate,
        (width, height),
    ))
}

fn parse_timeline(
    value: &Value,
    path: &str,
    keys: &AnimateKeys,
) -> Result<Timeline, AnimateStageError> {
    let object = as_object(value, path)?;
    let layers_path = format!("{path}.{}", keys.layers);
    let layers = as_array(field(object, keys.layers, path)?, &layers_path)?;
    let mut parsed_layers = Vec::with_capacity(layers.len());
    let mut frame_count = 0usize;

    for (layer_index, layer_value) in layers.iter().enumerate() {
        let layer_path = format!("{layers_path}[{layer_index}]");
        let layer = as_object(layer_value, &layer_path)?;
        let name = layer
            .get(keys.layer_name)
            .and_then(Value::as_str)
            .map(str::to_string);
        let clipped_by = layer
            .get(keys.clipped_by)
            .or_else(|| layer.get("Clpb"))
            .and_then(Value::as_str)
            .map(str::to_string);
        // Native `Clpb` takes priority over `LT`: a layer that is itself
        // clipped is not also a clipper (Layer.hx:138-174).
        let is_clipper = clipped_by.is_none()
            && matches!(
                layer
                    .get(keys.layer_type)
                    .or_else(|| layer.get("LT"))
                    .and_then(Value::as_str),
                Some("Clp" | "Clipper")
            );
        let frames_path = format!("{layer_path}.{}", keys.frames);
        let frames = match layer.get(keys.frames) {
            Some(frames) => as_array(frames, &frames_path)?,
            None => &[],
        };
        let mut parsed_frames = Vec::with_capacity(frames.len());

        for (frame_index, frame_value) in frames.iter().enumerate() {
            let frame_path = format!("{frames_path}[{frame_index}]");
            let frame = as_object(frame_value, &frame_path)?;
            let start = as_usize(
                field(frame, keys.frame_index, &frame_path)?,
                &format!("{frame_path}.{}", keys.frame_index),
            )?;
            let duration = as_usize(
                field(frame, keys.frame_duration, &frame_path)?,
                &format!("{frame_path}.{}", keys.frame_duration),
            )?;
            let end =
                start
                    .checked_add(duration)
                    .ok_or_else(|| AnimateStageError::BadKeyframe {
                        path: frame_path.clone(),
                        start,
                        duration,
                    })?;
            if duration == 0 {
                return Err(AnimateStageError::BadKeyframe {
                    path: frame_path,
                    start,
                    duration: 0,
                });
            }
            frame_count = frame_count.max(end);

            let label = match frame.get(keys.frame_name) {
                Some(value) => Some(as_string(
                    value,
                    &format!("{frame_path}.{}", keys.frame_name),
                )?),
                None => None,
            };

            let mut elements = Vec::new();
            if let Some(element_array) = frame.get(keys.elements) {
                let elements_path = format!("{frame_path}.{}", keys.elements);
                for (element_index, element_value) in
                    as_array(element_array, &elements_path)?.iter().enumerate()
                {
                    let element_path = format!("{elements_path}[{element_index}]");
                    let element = as_object(element_value, &element_path)?;
                    if let Some(symbol_value) = element.get(keys.symbol_instance) {
                        elements.push(Element::Symbol(parse_symbol_instance(
                            symbol_value,
                            &format!("{element_path}.{}", keys.symbol_instance),
                            keys,
                        )?));
                    } else if let Some(atlas_value) = element.get(keys.atlas_sprite_instance) {
                        elements.push(Element::Atlas(parse_atlas_sprite(
                            atlas_value,
                            &format!("{element_path}.{}", keys.atlas_sprite_instance),
                            keys,
                        )?));
                    }
                    // Unknown element kinds (and colour transforms carried in
                    // `C`) are preserved in the source but ignored for phase 2b.
                }
            }

            parsed_frames.push(Keyframe {
                label,
                start,
                duration,
                elements,
            });
        }
        parsed_layers.push(Layer {
            name,
            is_clipper,
            clipped_by,
            frames: parsed_frames,
        });
    }

    if frame_count == 0 {
        return Err(AnimateStageError::BadKeyframe {
            path: format!("{path} (empty timeline)"),
            start: 0,
            duration: 0,
        });
    }

    Ok(Timeline {
        layers: parsed_layers,
        frame_count,
    })
}

fn parse_symbol_instance(
    value: &Value,
    path: &str,
    keys: &AnimateKeys,
) -> Result<SymbolInstance, AnimateStageError> {
    let object = as_object(value, path)?;
    let symbol = as_string(
        field(object, keys.si_symbol_name, path)?,
        &format!("{path}.{}", keys.si_symbol_name),
    )?;
    let first_frame = match object.get(keys.si_first_frame) {
        Some(value) => as_usize(value, &format!("{path}.{}", keys.si_first_frame))?,
        None => 0,
    };
    let loop_mode = match object.get(keys.si_loop) {
        Some(value) => {
            let value = value.as_str().ok_or_else(|| AnimateStageError::Invalid {
                path: format!("{path}.{}", keys.si_loop),
                message: "expected a string".to_string(),
            })?;
            match value {
                "PO" | "playonce" => LoopMode::PlayOnce,
                "SF" | "singleframe" => LoopMode::SingleFrame,
                _ => LoopMode::Loop,
            }
        }
        None => LoopMode::Loop,
    };
    let matrix = parse_matrix(object, path)?;

    Ok(SymbolInstance {
        symbol,
        first_frame,
        loop_mode,
        matrix,
        path: path.to_string(),
    })
}

fn parse_atlas_sprite(
    value: &Value,
    path: &str,
    keys: &AnimateKeys,
) -> Result<AtlasSprite, AnimateStageError> {
    let object = as_object(value, path)?;
    let name = as_string(
        field(object, keys.asi_name, path)?,
        &format!("{path}.{}", keys.asi_name),
    )?;
    let matrix = parse_matrix(object, path)?;

    Ok(AtlasSprite {
        name,
        matrix,
        path: path.to_string(),
    })
}

/// Parses an element's 2D matrix.
///
/// Order: `MX` (authoritative, unchanged previous behavior), element-level
/// `M3D` / `Matrix3D`, an `M3D` nested in `TRP`, the `TRP` x/y translation,
/// identity. Verbose Adobe Animate `transformationPoint` is intentionally
/// NOT used as translation because it is the Flash rotation pivot.
fn parse_matrix(object: &Map<String, Value>, path: &str) -> Result<[f64; 6], AnimateStageError> {
    if let Some(value) = object.get("MX") {
        return parse_mx(value, &format!("{path}.MX"));
    }
    if let Some(value) = object.get("M3D") {
        return parse_m3d_value(value, &format!("{path}.M3D"));
    }
    if let Some(value) = object.get("Matrix3D") {
        return parse_m3d_value(value, &format!("{path}.Matrix3D"));
    }
    // A non-object `TRP` is tolerated as absent, matching the previous
    // ignored-TRP behavior and native's null-safe `trp?.x ?? 0.0`.
    if let Some(trp) = object.get("TRP").filter(|value| value.is_object()) {
        if let Some(value) = trp.get("M3D") {
            return parse_m3d_value(value, &format!("{path}.TRP.M3D"));
        }
        return parse_trp_translation(trp, &format!("{path}.TRP"));
    }
    Ok(IDENTITY)
}

fn parse_mx(value: &Value, path: &str) -> Result<[f64; 6], AnimateStageError> {
    let values = value
        .as_array()
        .ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: "expected an array".to_string(),
        })?;
    if values.len() != 6 {
        return Err(AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: format!("expected six numbers, got {}", values.len()),
        });
    }

    let mut matrix = IDENTITY;
    for (index, value) in values.iter().enumerate() {
        let number = value.as_f64().ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: format!("entry {index} is not a number"),
        })?;
        if !number.is_finite() {
            return Err(AnimateStageError::BadMatrix {
                path: path.to_string(),
                message: format!("entry {index} is not finite"),
            });
        }
        matrix[index] = number;
    }
    Ok(matrix)
}

fn parse_m3d_value(value: &Value, path: &str) -> Result<[f64; 6], AnimateStageError> {
    if let Some(object) = value.as_object() {
        return parse_m3d_object(object, path);
    }
    let values = value
        .as_array()
        .ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: "expected a 16-entry array or Matrix3D object".to_string(),
        })?;
    if values.len() != 16 {
        return Err(AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: format!("expected sixteen numbers, got {}", values.len()),
        });
    }

    let mut matrix = [0.0; 16];
    for (index, value) in values.iter().enumerate() {
        matrix[index] = value.as_f64().ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: format!("entry {index} is not a number"),
        })?;
    }
    m3d_to_affine(&matrix, path)
}

fn parse_m3d_object(
    object: &Map<String, Value>,
    path: &str,
) -> Result<[f64; 6], AnimateStageError> {
    let mut matrix = [0.0; 16];
    for (index, key) in M3D_OBJECT_KEYS.iter().enumerate() {
        let value = object
            .get(*key)
            .ok_or_else(|| AnimateStageError::BadMatrix {
                path: path.to_string(),
                message: format!("missing required matrix field `{key}`"),
            })?;
        matrix[index] = value.as_f64().ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: format!("field `{key}` is not a number"),
        })?;
    }
    m3d_to_affine(&matrix, path)
}

/// FlxAnimate's 4x4-to-2D projection (`FlxAnimateJson.hx:713-717`): the
/// serialized matrix is column-major, giving `[a, b, c, d, tx, ty] =
/// [m0, m1, m4, m5, m12, m13]`. A perspective row has no 2D affine
/// equivalent, so it fails closed instead of silently dropping the
/// projection; no shipped BTA file uses one.
fn m3d_to_affine(matrix: &[f64; 16], path: &str) -> Result<[f64; 6], AnimateStageError> {
    for (index, number) in matrix.iter().enumerate() {
        if !number.is_finite() {
            return Err(AnimateStageError::BadMatrix {
                path: path.to_string(),
                message: format!("entry {index} is not finite"),
            });
        }
    }
    if matrix[3] != 0.0 || matrix[7] != 0.0 || matrix[11] != 0.0 || matrix[15] != 1.0 {
        return Err(AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: "perspective M3D has no 2D affine equivalent".to_string(),
        });
    }
    Ok([
        matrix[0], matrix[1], matrix[4], matrix[5], matrix[12], matrix[13],
    ])
}

fn parse_trp_translation(value: &Value, path: &str) -> Result<[f64; 6], AnimateStageError> {
    let object = as_object(value, path)?;
    let x = trp_axis(object, "x", path)?;
    let y = trp_axis(object, "y", path)?;
    Ok([1.0, 0.0, 0.0, 1.0, x, y])
}

fn trp_axis(object: &Map<String, Value>, key: &str, path: &str) -> Result<f64, AnimateStageError> {
    let Some(value) = object.get(key) else {
        return Ok(0.0);
    };
    let number = value.as_f64().ok_or_else(|| AnimateStageError::BadMatrix {
        path: path.to_string(),
        message: format!("`{key}` is not a number"),
    })?;
    if !number.is_finite() {
        return Err(AnimateStageError::BadMatrix {
            path: path.to_string(),
            message: format!("`{key}` is not finite"),
        });
    }
    Ok(number)
}

fn parse_json(json: &str) -> Result<Value, AnimateStageError> {
    let json = json.strip_prefix('\u{FEFF}').unwrap_or(json);
    serde_json::from_str(json).map_err(|error| AnimateStageError::Invalid {
        path: "Animation.json".to_string(),
        message: error.to_string(),
    })
}

fn as_object<'a>(
    value: &'a Value,
    path: &str,
) -> Result<&'a Map<String, Value>, AnimateStageError> {
    value
        .as_object()
        .ok_or_else(|| invalid(path, "expected a JSON object"))
}

fn as_array<'a>(value: &'a Value, path: &str) -> Result<&'a [Value], AnimateStageError> {
    value
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| invalid(path, "expected a JSON array"))
}

fn as_string(value: &Value, path: &str) -> Result<String, AnimateStageError> {
    value
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| invalid(path, "expected a string"))
}

fn as_u32(value: &Value, path: &str) -> Result<u32, AnimateStageError> {
    integral_u64(value)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| invalid(path, "expected a non-negative 32-bit integer"))
}

fn as_usize(value: &Value, path: &str) -> Result<usize, AnimateStageError> {
    integral_u64(value)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| invalid(path, "expected a non-negative integer"))
}

/// Accepts JSON integers and integral floats (`24` and `24.0`). Haxe's JSON
/// parser exposes every number as a float, so both spellings are valid input.
fn integral_u64(value: &Value) -> Option<u64> {
    if let Some(value) = value.as_u64() {
        return Some(value);
    }
    let number = value.as_f64()?;
    if number.is_finite() && number >= 0.0 && number.fract() == 0.0 && number <= u64::MAX as f64 {
        Some(number as u64)
    } else {
        None
    }
}

fn field<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    path: &str,
) -> Result<&'a Value, AnimateStageError> {
    object
        .get(key)
        .ok_or_else(|| invalid(&format!("{path}.{key}"), "missing required key"))
}

fn invalid(path: &str, message: &str) -> AnimateStageError {
    AnimateStageError::Invalid {
        path: path.to_string(),
        message: message.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{AnimateStage, AnimateStageError, m3d_to_affine};

    const SAMPLE_SPRITEMAP: &str = r#"{
        "ATLAS": {
            "SPRITES": [
                {"SPRITE": {"name": "0000", "x": 0, "y": 0, "w": 106, "h": 38, "rotated": false}},
                {"SPRITE": {"name": "0001", "x": 0, "y": 43, "w": 79, "h": 22, "rotated": false}}
            ]
        },
        "meta": {
            "image": "spritemap1.png",
            "size": {"w": 106, "h": 65}
        }
    }"#;

    #[test]
    fn m3d_to_affine_uses_flxanimate_column_major_layout() {
        let mut matrix = [0.0; 16];
        matrix[0] = 2.0;
        matrix[1] = 0.25;
        matrix[4] = -0.5;
        matrix[5] = 3.0;
        matrix[10] = 1.0;
        matrix[12] = 10.0;
        matrix[13] = 20.0;
        matrix[15] = 1.0;
        assert_eq!(
            m3d_to_affine(&matrix, "test.M3D").expect("affine projection"),
            [2.0, 0.25, -0.5, 3.0, 10.0, 20.0]
        );
    }

    #[test]
    fn m3d_to_affine_rejects_non_finite_and_perspective() {
        let mut matrix = [0.0; 16];
        matrix[0] = 1.0;
        matrix[5] = 1.0;
        matrix[10] = 1.0;
        matrix[15] = 1.0;
        matrix[12] = f64::INFINITY;
        let error = m3d_to_affine(&matrix, "test.M3D").expect_err("infinite translation");
        assert!(matches!(
            error,
            AnimateStageError::BadMatrix { ref path, ref message }
                if path == "test.M3D" && message.contains("not finite")
        ));

        matrix[12] = 0.0;
        matrix[7] = 1.0;
        let error = m3d_to_affine(&matrix, "test.M3D").expect_err("perspective row");
        assert!(matches!(
            error,
            AnimateStageError::BadMatrix { ref path, ref message }
                if path == "test.M3D" && message.contains("perspective")
        ));
    }

    #[test]
    fn parses_verbose_adobe_animate_json_with_matrix3d_and_symbols() {
        let animation = r#"{
            "ANIMATION": {
                "name": "Nene_assets(1)",
                "StageInstance": {
                    "SYMBOL_Instance": {
                        "SYMBOL_name": "a bot eyes lookin",
                        "Matrix3D": {
                            "m00": 1.0, "m01": 0.0, "m02": 0.0, "m03": 0.0,
                            "m10": 0.0, "m11": 1.0, "m12": 0.0, "m13": 0.0,
                            "m20": 0.0, "m21": 0.0, "m22": 1.0, "m23": 0.0,
                            "m30": 562.9, "m31": 731.2, "m32": 0.0, "m33": 1.0
                        }
                    }
                },
                "TIMELINE": {
                    "LAYERS": [
                        {
                            "Layer_name": "Layer_1",
                            "Frames": [
                                {
                                    "index": 0,
                                    "duration": 2,
                                    "elements": [
                                        {
                                            "SYMBOL_Instance": {
                                                "SYMBOL_name": "abot eyes",
                                                "firstFrame": 0,
                                                "loop": "loop",
                                                "transformationPoint": {"x": 53.2, "y": 20.25},
                                                "Matrix3D": {
                                                    "m00": 1.0, "m01": 0.0, "m02": 0.0, "m03": 0.0,
                                                    "m10": 0.0, "m11": 1.0, "m12": 0.0, "m13": 0.0,
                                                    "m20": 0.0, "m21": 0.0, "m22": 1.0, "m23": 0.0,
                                                    "m30": 0.0, "m31": -0.95, "m32": 0.0, "m33": 1.0
                                                }
                                            }
                                        }
                                    ]
                                },
                                {
                                    "index": 2,
                                    "duration": 1,
                                    "elements": [
                                        {
                                            "SYMBOL_Instance": {
                                                "SYMBOL_name": "abot eyes 2",
                                                "firstFrame": 0,
                                                "loop": "playonce",
                                                "Matrix3D": {
                                                    "m00": -1.0, "m01": 0.0, "m02": 0.0, "m03": 0.0,
                                                    "m10": 0.0, "m11": 1.0, "m12": 0.0, "m13": 0.0,
                                                    "m20": 0.0, "m21": 0.0, "m22": 1.0, "m23": 0.0,
                                                    "m30": -9.0, "m31": 17.35, "m32": 0.0, "m33": 1.0
                                                }
                                            }
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            },
            "SYMBOL_DICTIONARY": {
                "Symbols": [
                    {
                        "SYMBOL_name": "abot eyes",
                        "TIMELINE": {
                            "LAYERS": [
                                {
                                    "Layer_name": "Layer_2",
                                    "Frames": [
                                        {
                                            "index": 0,
                                            "duration": 1,
                                            "elements": [
                                                {
                                                    "ATLAS_SPRITE_instance": {
                                                        "name": "0000",
                                                        "Matrix3D": {
                                                            "m00": 1.0, "m01": 0.0, "m02": 0.0, "m03": 0.0,
                                                            "m10": 0.0, "m11": 1.0, "m12": 0.0, "m13": 0.0,
                                                            "m20": 0.0, "m21": 0.0, "m22": 1.0, "m23": 0.0,
                                                            "m30": 0.0, "m31": 1.0, "m32": 0.0, "m33": 1.0
                                                        }
                                                    }
                                                }
                                            ]
                                        }
                                    ]
                                }
                            ]
                        }
                    },
                    {
                        "SYMBOL_name": "abot eyes 2",
                        "TIMELINE": {
                            "LAYERS": [
                                {
                                    "Layer_name": "Layer_2",
                                    "Frames": [
                                        {
                                            "index": 0,
                                            "duration": 1,
                                            "elements": [
                                                {
                                                    "ATLAS_SPRITE_instance": {
                                                        "name": "0001",
                                                        "Matrix3D": {
                                                            "m00": 1.0, "m01": 0.0, "m02": 0.0, "m03": 0.0,
                                                            "m10": 0.0, "m11": 1.0, "m12": 0.0, "m13": 0.0,
                                                            "m20": 0.0, "m21": 0.0, "m22": 1.0, "m23": 0.0,
                                                            "m30": 0.0, "m31": 0.0, "m32": 0.0, "m33": 1.0
                                                        }
                                                    }
                                                }
                                            ]
                                        }
                                    ]
                                }
                            ]
                        }
                    }
                ]
            },
            "metadata": {"framerate": 24.0}
        }"#;

        let stage = AnimateStage::parse(animation, SAMPLE_SPRITEMAP).expect("parse verbose JSON");
        assert_eq!(stage.frame_rate(), 24);
        assert_eq!(stage.stage_size(), (106, 65));
        assert_eq!(stage.frame_count(), 3);

        let f0 = stage.resolve_main_frame(0).expect("frame 0");
        assert_eq!(f0.len(), 1);
        assert_eq!(f0[0].atlas_rect, [0, 0, 106, 38]);
        // Concat([1,0,0,1,0,-0.95], [1,0,0,1,0,1.0]) => ty = 0.05; StageInstance ignored.
        assert!((f0[0].matrix[4] - 0.0).abs() < 1e-12);
        assert!((f0[0].matrix[5] - 0.05).abs() < 1e-12);

        let f2 = stage.resolve_main_frame(2).expect("frame 2");
        assert_eq!(f2.len(), 1);
        assert_eq!(f2[0].atlas_rect, [0, 43, 79, 22]);
        assert_eq!(f2[0].matrix, [-1.0, 0.0, 0.0, 1.0, -9.0, 17.35]);
    }

    #[test]
    fn verbose_parser_fails_closed_on_fractional_framerate_or_missing_matrix3d_field() {
        let bad_fps = r#"{
            "ANIMATION": {"TIMELINE": {"LAYERS": [{"Layer_name": "L", "Frames": [{"index": 0, "duration": 1, "elements": []}]}]}},
            "metadata": {"framerate": 24.5}
        }"#;
        let error = AnimateStage::parse(bad_fps, SAMPLE_SPRITEMAP).expect_err("fractional fps");
        assert!(matches!(
            error,
            AnimateStageError::Invalid { ref path, .. } if path == "metadata.framerate"
        ));

        let bad_m3d = r#"{
            "ANIMATION": {
                "TIMELINE": {
                    "LAYERS": [
                        {
                            "Layer_name": "L",
                            "Frames": [
                                {
                                    "index": 0,
                                    "duration": 1,
                                    "elements": [
                                        {
                                            "ATLAS_SPRITE_instance": {
                                                "name": "0000",
                                                "Matrix3D": {"m00": 1.0}
                                            }
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                }
            },
            "metadata": {"framerate": 24.0}
        }"#;
        let error =
            AnimateStage::parse(bad_m3d, SAMPLE_SPRITEMAP).expect_err("incomplete Matrix3D");
        assert!(matches!(
            error,
            AnimateStageError::BadMatrix { ref message, .. } if message.contains("missing required matrix field")
        ));
    }
}
