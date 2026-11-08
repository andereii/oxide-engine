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
//! `[0, 1, -1, 0, width, 0]`, i.e. `(x, y) -> (width - y, x)`: a clockwise
//! quarter turn in engine screen space that restores the upright bitmap. The
//! matrix is composed inside the instance matrix (`MX_asi ∘ tile`). This
//! reproduces the observed dad bounds (see the real-asset regression test),
//! and keeps the sprite's rotation in the returned matrix rather than in the
//! atlas rect.
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
//! - Unknown element keys, `ST`, `TRP` and per-element `C` colour transforms
//!   are accepted and ignored for phase 2b; only `SI`/`ASI` produce draws.
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
    /// A matrix was not six finite numbers.
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
    frames: Vec<Keyframe>,
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
    pub fn resolve_main_frame(
        &self,
        timeline_frame: usize,
    ) -> Result<Vec<SpriteQuad>, AnimateStageError> {
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
            &mut quads,
        )?;
        Ok(quads)
    }

    /// Resolves one frame of a named symbol timeline into draw-ordered
    /// sprites, in the symbol's local coordinates. The symbol may live in the
    /// receiver or in any added collection; nested references use the same
    /// main-first lookup order.
    pub fn resolve_symbol_frame(
        &self,
        symbol: &str,
        timeline_frame: usize,
    ) -> Result<Vec<SpriteQuad>, AnimateStageError> {
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
            &mut quads,
        )?;
        Ok(quads)
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

impl<'a> Resolver<'a> {
    fn find_symbol(&self, name: &str) -> Option<&'a Timeline> {
        self.stages.iter().find_map(|stage| stage.symbols.get(name))
    }

    fn find_sprite(&self, name: &str) -> Option<&'a SpriteEntry> {
        self.stages.iter().find_map(|stage| stage.sprites.get(name))
    }

    fn compute_bounds(&self, timeline: &Timeline) -> Result<[f64; 4], AnimateStageError> {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        let mut quads = Vec::new();
        let mut stack = Vec::new();

        for frame in 0..timeline.frame_count {
            quads.clear();
            self.resolve_timeline(timeline, frame, IDENTITY, 0, &mut stack, &mut quads)?;
            for quad in &quads {
                let rect = quad_corners(quad);
                min_x = min_x.min(rect[0]);
                min_y = min_y.min(rect[1]);
                max_x = max_x.max(rect[2]);
                max_y = max_y.max(rect[3]);
            }
        }

        if !min_x.is_finite() {
            return Ok([0.0, 0.0, 0.0, 0.0]);
        }
        Ok([min_x, min_y, max_x - min_x, max_y - min_y])
    }

    fn resolve_timeline(
        &self,
        timeline: &Timeline,
        frame: usize,
        parent: [f64; 6],
        depth: usize,
        stack: &mut Vec<String>,
        out: &mut Vec<SpriteQuad>,
    ) -> Result<(), AnimateStageError> {
        for layer in timeline.layers.iter().rev() {
            let Some(keyframe) = layer.frame_at(frame) else {
                continue;
            };
            for element in &keyframe.elements {
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
                        out.push(SpriteQuad {
                            bitmap_name: sprite.name.clone(),
                            atlas_rect: entry.rect,
                            matrix,
                            rotated: entry.rotated,
                        });
                    }
                    Element::Symbol(instance) => {
                        if depth >= MAX_SYMBOL_DEPTH {
                            let mut chain = stack.join(" -> ");
                            if !chain.is_empty() {
                                chain.push_str(" -> ");
                            }
                            chain.push_str(&instance.symbol);
                            return Err(AnimateStageError::SymbolRecursion {
                                path: instance.path.clone(),
                                chain,
                            });
                        }
                        if stack.iter().any(|active| active == &instance.symbol) {
                            let mut chain = stack.join(" -> ");
                            if !chain.is_empty() {
                                chain.push_str(" -> ");
                            }
                            chain.push_str(&instance.symbol);
                            return Err(AnimateStageError::SymbolRecursion {
                                path: instance.path.clone(),
                                chain,
                            });
                        }

                        let child_timeline =
                            self.find_symbol(&instance.symbol).ok_or_else(|| {
                                AnimateStageError::UnknownSymbol {
                                    name: instance.symbol.clone(),
                                    path: instance.path.clone(),
                                }
                            })?;
                        let child_frame = instance.child_frame(
                            keyframe.start,
                            frame,
                            child_timeline.frame_count,
                        )?;
                        stack.push(instance.symbol.clone());
                        let result = self.resolve_timeline(
                            child_timeline,
                            child_frame,
                            concat(parent, instance.matrix),
                            depth + 1,
                            stack,
                            out,
                        );
                        stack.pop();
                        result?;
                    }
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

/// Matrix restoring an upright sprite from a spritemap entry stored rotated
/// by -90°. `w` is the stored entry width.
fn rotated_tile_matrix(width: i32) -> [f64; 6] {
    [0.0, 1.0, -1.0, 0.0, width as f64, 0.0]
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

fn parse_stage(
    animation: &Value,
    spritemap: &AnimateAtlas,
) -> Result<(StageData, u32, (u32, u32)), AnimateStageError> {
    let root = as_object(animation, "Animation.json")?;

    let metadata = as_object(field(root, "MD", "Animation.json")?, "Animation.json.MD")?;
    let frame_rate = as_u32(field(metadata, "FRT", "Animation.json.MD")?, "MD.FRT")?;
    let width = as_u32(field(metadata, "W", "Animation.json.MD")?, "MD.W")?;
    let height = as_u32(field(metadata, "H", "Animation.json.MD")?, "MD.H")?;

    let animate = as_object(field(root, "AN", "Animation.json")?, "Animation.json.AN")?;
    let timeline_value = field(animate, "TL", "AN")?;
    let main = parse_timeline(timeline_value, "AN.TL")?;

    let mut symbols = HashMap::new();
    if let Some(dictionary) = root.get("SD") {
        let dictionary = as_object(dictionary, "Animation.json.SD")?;
        if let Some(symbol_array) = dictionary.get("S") {
            for (index, symbol_value) in as_array(symbol_array, "SD.S")?.iter().enumerate() {
                let path = format!("SD.S[{index}]");
                let symbol = as_object(symbol_value, &path)?;
                let name = as_string(field(symbol, "SN", &path)?, &format!("{path}.SN"))?;
                let timeline = parse_timeline(field(symbol, "TL", &path)?, &format!("{path}.TL"))?;
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

fn parse_timeline(value: &Value, path: &str) -> Result<Timeline, AnimateStageError> {
    let object = as_object(value, path)?;
    let layers = as_array(field(object, "L", path)?, &format!("{path}.L"))?;
    let mut parsed_layers = Vec::with_capacity(layers.len());
    let mut frame_count = 0usize;

    for (layer_index, layer_value) in layers.iter().enumerate() {
        let layer_path = format!("{path}.L[{layer_index}]");
        let layer = as_object(layer_value, &layer_path)?;
        let frames = match layer.get("FR") {
            Some(frames) => as_array(frames, &format!("{layer_path}.FR"))?,
            None => &[],
        };
        let mut parsed_frames = Vec::with_capacity(frames.len());

        for (frame_index, frame_value) in frames.iter().enumerate() {
            let frame_path = format!("{layer_path}.FR[{frame_index}]");
            let frame = as_object(frame_value, &frame_path)?;
            let start = as_usize(field(frame, "I", &frame_path)?, &format!("{frame_path}.I"))?;
            let duration = as_usize(
                field(frame, "DU", &frame_path)?,
                &format!("{frame_path}.DU"),
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

            let label = match frame.get("N") {
                Some(value) => Some(as_string(value, &format!("{frame_path}.N"))?),
                None => None,
            };

            let mut elements = Vec::new();
            if let Some(element_array) = frame.get("E") {
                for (element_index, element_value) in
                    as_array(element_array, &format!("{frame_path}.E"))?
                        .iter()
                        .enumerate()
                {
                    let element_path = format!("{frame_path}.E[{element_index}]");
                    let element = as_object(element_value, &element_path)?;
                    if let Some(symbol_value) = element.get("SI") {
                        elements.push(Element::Symbol(parse_symbol_instance(
                            symbol_value,
                            &format!("{element_path}.SI"),
                        )?));
                    } else if let Some(atlas_value) = element.get("ASI") {
                        elements.push(Element::Atlas(parse_atlas_sprite(
                            atlas_value,
                            &format!("{element_path}.ASI"),
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

fn parse_symbol_instance(value: &Value, path: &str) -> Result<SymbolInstance, AnimateStageError> {
    let object = as_object(value, path)?;
    let symbol = as_string(field(object, "SN", path)?, &format!("{path}.SN"))?;
    let first_frame = match object.get("FF") {
        Some(value) => as_usize(value, &format!("{path}.FF"))?,
        None => 0,
    };
    let loop_mode = match object.get("LP") {
        Some(value) => {
            let value = value.as_str().ok_or_else(|| AnimateStageError::Invalid {
                path: format!("{path}.LP"),
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

fn parse_atlas_sprite(value: &Value, path: &str) -> Result<AtlasSprite, AnimateStageError> {
    let object = as_object(value, path)?;
    let name = as_string(field(object, "N", path)?, &format!("{path}.N"))?;
    let matrix = parse_matrix(object, path)?;

    Ok(AtlasSprite {
        name,
        matrix,
        path: path.to_string(),
    })
}

fn parse_matrix(object: &Map<String, Value>, path: &str) -> Result<[f64; 6], AnimateStageError> {
    let Some(value) = object.get("MX") else {
        return Ok(IDENTITY);
    };
    let path = format!("{path}.MX");
    let values = value
        .as_array()
        .ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.clone(),
            message: "expected an array".to_string(),
        })?;
    if values.len() != 6 {
        return Err(AnimateStageError::BadMatrix {
            path,
            message: format!("expected six numbers, got {}", values.len()),
        });
    }

    let mut matrix = IDENTITY;
    for (index, value) in values.iter().enumerate() {
        let number = value.as_f64().ok_or_else(|| AnimateStageError::BadMatrix {
            path: path.clone(),
            message: format!("entry {index} is not a number"),
        })?;
        if !number.is_finite() {
            return Err(AnimateStageError::BadMatrix {
                path: path.clone(),
                message: format!("entry {index} is not finite"),
            });
        }
        matrix[index] = number;
    }
    Ok(matrix)
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
