//! 6E dialogue load surface (`docs/M6-CONTRACT.md` §14): conversations,
//! boxes and speakers for the shipped week-6 family, parsed through the
//! `oxide_data::dialogue` layer and resolved through the same VFS candidate
//! order as the 6B loader.
//!
//! The shipped conversations all live in the `week6` library, so asset keys
//! without an explicit prefix resolve `week6/images/<key>` first, then
//! `shared`, then `preload` (the observed `Paths.getPath` order).
//!
//! The frozen render-fixture path never calls [`load_dialogue`] (the
//! `load_slice_for_song` entry passes `None`), so the established
//! bopeebo/school frame digests stay byte-identical.
//!
//! The scene-integration slice (M6-CONTRACT §14.2) consumes these types.

use std::collections::BTreeMap;
use std::path::Path;

use oxide_data::character::AnimationData;
use oxide_data::dialogue::{
    BackdropData, ConversationData, DialogueBoxData, DialogueEntryData, SpeakerData,
};
use oxide_vfs::Vfs;

use super::RenderError;
use super::conversation::{
    ConversationState, ConversationSurface, GlyphPage, GlyphRaster, TEXT_LETTER_SPACING, TEXT_SIZE,
    correct_animation_name, raster_glyph_page,
};
use super::geometry::{GAME_HEIGHT, GAME_WIDTH, HALF_HEIGHT, HALF_WIDTH};
use super::load::{ASSET_SIZE_LIMIT, PageImage, PageStore, image_file_bases};
use super::scene::{LayerSpec, Quad, concat, quad_from_matrix, quad_uv};

/// One parsed dialogue sprite sheet: page rectangles plus the named clips
/// built by the native `addAtlasAnimations` prefix rule (frame NAMES that
/// START WITH the clip prefix, document order; `FlxAnimationUtil`).
pub(crate) struct DialogueSheet {
    pub(crate) page_size: [f64; 2],
    pub(crate) frames: Vec<DialogueFrame>,
    pub(crate) clips: BTreeMap<String, DialogueClip>,
}

/// One Sparrow frame with the flixel trim geometry:
/// `rect` is the page sub-rectangle, `source_size` is `frameWidth/Height`
/// (the sprite hitbox used by `screenCenter`) and `offset` is
/// `(-frameX, -frameY)` inside that hitbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DialogueFrame {
    pub(crate) rect: [i32; 4],
    pub(crate) source_size: [i32; 2],
    pub(crate) offset: [i32; 2],
}

/// One named dialogue animation (fps from the data; `looped` from the data's
/// `looped` key when present, otherwise false like the native default).
pub(crate) struct DialogueClip {
    pub(crate) frames: Vec<usize>,
    pub(crate) fps: f64,
    pub(crate) looped: bool,
}

impl DialogueSheet {
    /// Builds the sheet from a parsed Sparrow XML atlas plus the animation
    /// definitions in the dialogue document.
    pub(crate) fn from_atlas(
        page_size: [f64; 2],
        atlas: &oxide_atlas::Atlas,
        animations: &[oxide_data::character::AnimationData],
    ) -> Result<Self, RenderError> {
        let mut frames = Vec::with_capacity(atlas.subtextures.len());
        for texture in &atlas.subtextures {
            if texture.rotated {
                return Err(RenderError::Unsupported {
                    detail: format!(
                        "dialogue sheet item {:?} is rotated; the 6E dialogue pipeline \
                         supports unrotated Sparrow frames only ([D])",
                        texture.name
                    ),
                });
            }
            let quad = oxide_atlas::frame_quad(texture);
            frames.push(DialogueFrame {
                rect: quad.atlas_rect,
                source_size: quad.dest_size,
                offset: quad.dest_offset,
            });
        }
        let mut clips = BTreeMap::new();
        for animation in animations {
            let indices: Vec<usize> = atlas
                .subtextures
                .iter()
                .enumerate()
                .filter(|(_, texture)| {
                    animation
                        .prefix
                        .as_deref()
                        .is_some_and(|prefix| texture.name.starts_with(prefix))
                })
                .map(|(index, _)| index)
                .collect();
            clips.insert(
                animation.name.clone(),
                DialogueClip {
                    frames: indices,
                    fps: animation.frame_rate.unwrap_or(24.0),
                    looped: animation.looped.unwrap_or(false),
                },
            );
        }
        Ok(Self {
            page_size,
            frames,
            clips,
        })
    }
}

/// One loaded dialogue box: data + dual sampler pages + the parsed sheet.
pub(crate) struct BoxAsset {
    pub(crate) data: DialogueBoxData,
    pub(crate) linear_page: usize,
    pub(crate) pixel_page: usize,
    pub(crate) sheet: DialogueSheet,
}

/// One loaded speaker: data + dual sampler pages + the parsed sheet.
pub(crate) struct SpeakerAsset {
    pub(crate) data: SpeakerData,
    pub(crate) linear_page: usize,
    pub(crate) pixel_page: usize,
    pub(crate) sheet: DialogueSheet,
}

/// The loaded dialogue payload for one conversation (`LoadedSlice.dialogue`).
pub(crate) struct LoadedDialogue {
    pub(crate) conversation_id: String,
    pub(crate) conversation: ConversationData,
    pub(crate) boxes: BTreeMap<String, BoxAsset>,
    pub(crate) speakers: BTreeMap<String, SpeakerAsset>,
    /// The `pixel.otf` bytes (family "Pixel Arial 11 Bold"); the per-line
    /// glyph rasters are built at draw time from these bytes (deterministic,
    /// pinned by the conversation-core test).
    pub(crate) font_bytes: Vec<u8>,
}

/// Screen-space projection for the cutscene layer (`camCutscene` has no
/// scroll and the conversation group sits at the viewport origin).
pub(crate) const SCREEN_PROJECTION: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

const FULL_UV: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

/// The two pre-registered sampler variants of one dialogue sprite.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DialogueSpriteLayers {
    pub(crate) linear: usize,
    pub(crate) pixel: usize,
}

/// Layer registry for one conversation. Indices are relative to the
/// conversation group start (the scene appends it after the HUD group).
///
/// Registration order is the native `FlxSpriteGroup` sort: backdrop zIndex
/// 10, speaker zIndex 200, box zIndex 300. The box group's own children are
/// added box sprite first, then the text display, so the text layers sit
/// after every box sprite layer.
pub(crate) struct DialogueLayers {
    /// Absolute index of this group's first layer in the scene slice.
    start: usize,
    pub(crate) count: usize,
    pub(crate) backdrop: usize,
    pub(crate) text_shadow: usize,
    pub(crate) text_fill: usize,
    pub(crate) boxes: BTreeMap<String, DialogueSpriteLayers>,
    pub(crate) speakers: BTreeMap<String, DialogueSpriteLayers>,
    /// Base colors before the per-frame conversation alpha is applied.
    base_colors: Vec<[f32; 4]>,
}

impl DialogueLayers {
    /// Appends the conversation layer group after the existing specs.
    ///
    /// Text pages are assigned per line by [`ConversationRuntime`], so the
    /// two text layers start untextured. The sprite variants are all
    /// pre-registered like the HUD icons, so a box/speaker swap never needs
    /// a new material.
    pub(crate) fn build(
        dialogue: &LoadedDialogue,
        specs: &mut Vec<LayerSpec>,
    ) -> Result<Self, RenderError> {
        let base = specs.len();
        let mut base_colors = Vec::new();
        let mut push = |page: Option<usize>, color: [f32; 4]| -> usize {
            let index = specs.len() - base;
            // Inactive conversations contribute no pixels; the scene tick
            // writes the per-frame alpha before the first active draw.
            specs.push(LayerSpec {
                page,
                color: [color[0], color[1], color[2], 0.0],
            });
            base_colors.push(color);
            index
        };

        let backdrop_color = match &dialogue.conversation.backdrop {
            BackdropData::Solid { color, .. } => parse_hex_color(color)?,
        };
        let backdrop = push(None, backdrop_color);

        let mut speakers = BTreeMap::new();
        for (id, asset) in &dialogue.speakers {
            let linear = push(Some(asset.linear_page), [1.0, 1.0, 1.0, 1.0]);
            let pixel = push(Some(asset.pixel_page), [1.0, 1.0, 1.0, 1.0]);
            speakers.insert(id.clone(), DialogueSpriteLayers { linear, pixel });
        }

        let mut boxes = BTreeMap::new();
        for (id, asset) in &dialogue.boxes {
            let linear = push(Some(asset.linear_page), [1.0, 1.0, 1.0, 1.0]);
            let pixel = push(Some(asset.pixel_page), [1.0, 1.0, 1.0, 1.0]);
            boxes.insert(id.clone(), DialogueSpriteLayers { linear, pixel });
        }

        let text_shadow = push(None, [1.0, 1.0, 1.0, 1.0]);
        let text_fill = push(None, [1.0, 1.0, 1.0, 1.0]);
        Ok(Self {
            start: base,
            count: specs.len() - base,
            backdrop,
            text_shadow,
            text_fill,
            boxes,
            speakers,
            base_colors,
        })
    }

    /// Current per-layer colors. The native conversation group alpha (the
    /// outro) multiplies every child; the backdrop additionally carries its
    /// data color alpha and the `EaseUtil.stepped(10)` fade-in.
    fn colors(&self, active: bool, group_alpha: f64, backdrop_alpha: f64) -> Vec<[f32; 4]> {
        let mut colors = self.base_colors.clone();
        if !active {
            for color in &mut colors {
                color[3] = 0.0;
            }
            return colors;
        }
        let group_alpha = group_alpha.clamp(0.0, 1.0) as f32;
        for (index, color) in colors.iter_mut().enumerate() {
            color[3] = if index == self.backdrop {
                let base = f64::from(self.base_colors[self.backdrop][3]);
                (base * backdrop_alpha.clamp(0.0, 1.0)) as f32 * group_alpha
            } else {
                group_alpha
            };
        }
        colors
    }
}

/// One baked line raster plus the page indices it was uploaded to.
struct CachedRaster {
    raster: GlyphRaster,
    fill_page: usize,
    shadow_page: Option<usize>,
}

/// Owns the native conversation machine plus the real box/speaker/text
/// surfaces for one loaded dialogue payload.
///
/// [D] The glyph pages are baked at scene construction instead of lazily in
/// `on_line_set`: the render layers own a fixed page index per material, so
/// every display string the conversation can show (each entry's set line and
/// its append concatenations) is known from the data and uploaded up front.
/// Line selection, append-prefix reveal and the typewriter reveal still run
/// through the surface callbacks and the core's `typed_len`.
pub(crate) struct ConversationRuntime {
    core: ConversationState,
    conversation: ConversationData,
    surface: DialogueSurface,
    layers: DialogueLayers,
    conversation_id: String,
    active: bool,
    last_tick_ms: Option<f64>,
}

impl ConversationRuntime {
    /// Builds the runtime, appends its layer group and uploads every line
    /// glyph page to `pages`.
    pub(crate) fn new(
        dialogue: LoadedDialogue,
        specs: &mut Vec<LayerSpec>,
        pages: &mut Vec<PageImage>,
    ) -> Result<Self, RenderError> {
        let layers = DialogueLayers::build(&dialogue, specs)?;
        let cache = bake_line_rasters(&dialogue, pages)?;
        let backdrop_fade_ms = match &dialogue.conversation.backdrop {
            BackdropData::Solid { fade_time, .. } if fade_time.is_finite() && *fade_time > 0.0 => {
                *fade_time * 1000.0
            }
            _ => 0.0,
        };
        let core = ConversationState::new(dialogue.conversation.clone());
        let LoadedDialogue {
            conversation_id,
            conversation,
            boxes,
            speakers,
            font_bytes: _,
        } = dialogue;
        Ok(Self {
            core,
            conversation,
            surface: DialogueSurface {
                boxes,
                speakers,
                cache,
                active_box: None,
                box_animation: None,
                box_elapsed_ms: 0.0,
                active_speaker: None,
                speaker_animation: None,
                speaker_elapsed_ms: 0.0,
                display_text: String::new(),
                prefix_chars: 0,
                current_key: None,
                raster_error: None,
                backdrop_started: false,
                backdrop_elapsed_ms: 0.0,
                backdrop_fade_ms,
            },
            layers,
            conversation_id,
            active: false,
            last_tick_ms: None,
        })
    }

    /// Consumed by the xtask dialogue fixture family (M6-CONTRACT §14.3
    /// approval item); temporary allow until that lands.
    #[allow(dead_code)]
    pub(crate) fn conversation_id(&self) -> &str {
        &self.conversation_id
    }

    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Starts (or restarts after completion) the loaded conversation.
    ///
    /// A duplicate trigger while the conversation is already active is
    /// ignored: the native choreography hook fires once and must not reset a
    /// running dialogue. An id that does not match the loaded payload fails
    /// closed instead of silently showing the wrong conversation.
    pub(crate) fn start(&mut self, id: &str) -> Result<(), RenderError> {
        if id != self.conversation_id {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "dialogue trigger {id:?} does not match the loaded conversation {:?}",
                    self.conversation_id
                ),
            });
        }
        if self.active {
            return Ok(());
        }
        self.core = ConversationState::new(self.conversation.clone());
        self.surface.reset();
        self.last_tick_ms = None;
        self.active = true;
        // Native `Conversation.onCreate` dispatches DIALOGUE_START immediately
        // (setupMusic/backdrop + the first entry opens); the user's first
        // advance press only skips the typing. The fresh core therefore kicks
        // Start -> Opening here, on the trigger tick.
        self.core.advance(&mut self.surface);
        Ok(())
    }

    /// One scene tick for the active conversation. The surface animation
    /// clocks advance by the scene's 1 ms grid before the core phase switch
    /// observes `box_animation_finished`.
    pub(crate) fn begin_tick(&mut self, now_ms: f64) {
        if !self.active {
            return;
        }
        let elapsed = match self.last_tick_ms {
            Some(previous) => (now_ms - previous).max(0.0),
            None => 0.0,
        };
        self.last_tick_ms = Some(now_ms);
        self.surface.advance(elapsed);
        self.core.begin_tick(now_ms, &mut self.surface);
        if self.core.completed() {
            self.active = false;
        }
    }

    /// `CUTSCENE_ADVANCE`: one `Conversation.advanceConversation` step.
    pub(crate) fn advance(&mut self) {
        if !self.active || self.core.completed() {
            return;
        }
        self.core.advance(&mut self.surface);
        if self.core.completed() {
            self.active = false;
        }
    }

    /// Consumed by the xtask dialogue fixture family (M6-CONTRACT §14.3
    /// approval item); temporary allow until that lands.
    #[allow(dead_code)]
    pub(crate) fn completed(&self) -> bool {
        self.core.completed()
    }

    /// Writes the current colors and text page indices into the scene's
    /// conversation slice. Called once per scene tick; the harness/window
    /// re-reads the specs before uploading a frame.
    pub(crate) fn sync_layer_specs(&self, specs: &mut [LayerSpec]) -> Result<(), RenderError> {
        let end = self
            .layers
            .start
            .checked_add(self.layers.count)
            .ok_or_else(|| RenderError::Unsupported {
                detail: "conversation layer range overflowed".to_string(),
            })?;
        let scene_len = specs.len();
        let specs = specs.get_mut(self.layers.start..end).ok_or_else(|| {
            RenderError::Unsupported {
                detail: format!(
                    "conversation owns {} layers at offset {} but the scene slice has {scene_len}",
                    self.layers.count, self.layers.start,
                ),
            }
        })?;
        let colors = self.layers.colors(
            self.active,
            self.core.outro_alpha(),
            self.surface.backdrop_alpha(),
        );
        for (spec, color) in specs.iter_mut().zip(colors) {
            spec.color = color;
        }
        let (fill, shadow) = if self.active {
            self.surface.current_pages()
        } else {
            (None, None)
        };
        specs[self.layers.text_shadow].page = shadow;
        specs[self.layers.text_fill].page = fill;
        Ok(())
    }

    /// Composes the conversation group's quads (layer-relative order).
    pub(crate) fn draw(&self, projection: [f64; 6]) -> Result<Vec<Vec<Quad>>, RenderError> {
        let mut out = vec![Vec::new(); self.layers.count];
        if !self.active {
            return Ok(out);
        }
        if let Some(detail) = &self.surface.raster_error {
            return Err(RenderError::Unsupported {
                detail: detail.clone(),
            });
        }
        out[self.layers.backdrop].push(quad_from_matrix(
            projection,
            GAME_WIDTH,
            GAME_HEIGHT,
            FULL_UV,
        ));
        if let Some((is_pixel, quad)) = self.surface.speaker_quad(projection)
            && let Some(speaker_id) = self.surface.active_speaker.as_ref()
            && let Some(layers) = self.layers.speakers.get(speaker_id)
        {
            let layer = if is_pixel {
                layers.pixel
            } else {
                layers.linear
            };
            out[layer].push(quad);
        }
        if let Some((is_pixel, quad)) = self.surface.box_quad(projection)
            && let Some(box_id) = self.surface.active_box.as_ref()
            && let Some(layers) = self.layers.boxes.get(box_id)
        {
            let layer = if is_pixel {
                layers.pixel
            } else {
                layers.linear
            };
            out[layer].push(quad);
        }
        let reveal = self
            .surface
            .prefix_chars
            .saturating_add(self.core.typed_len());
        let (shadow, fill) = self.surface.text_quads(projection, reveal)?;
        out[self.layers.text_shadow] = shadow;
        out[self.layers.text_fill] = fill;
        Ok(out)
    }

    #[cfg(test)]
    pub(crate) fn core(&self) -> &ConversationState {
        &self.core
    }

    #[cfg(test)]
    pub(crate) fn layers(&self) -> &DialogueLayers {
        &self.layers
    }
}

/// The real [`ConversationSurface`] implementation (box/speaker sprites and
/// the pixel-font text display).
struct DialogueSurface {
    boxes: BTreeMap<String, BoxAsset>,
    speakers: BTreeMap<String, SpeakerAsset>,
    cache: BTreeMap<(String, String), CachedRaster>,
    active_box: Option<String>,
    box_animation: Option<String>,
    box_elapsed_ms: f64,
    active_speaker: Option<String>,
    speaker_animation: Option<String>,
    speaker_elapsed_ms: f64,
    display_text: String,
    prefix_chars: usize,
    current_key: Option<(String, String)>,
    raster_error: Option<String>,
    backdrop_started: bool,
    backdrop_elapsed_ms: f64,
    backdrop_fade_ms: f64,
}

impl DialogueSurface {
    fn reset(&mut self) {
        self.active_box = None;
        self.box_animation = None;
        self.box_elapsed_ms = 0.0;
        self.active_speaker = None;
        self.speaker_animation = None;
        self.speaker_elapsed_ms = 0.0;
        self.display_text.clear();
        self.prefix_chars = 0;
        self.current_key = None;
        self.raster_error = None;
        self.backdrop_started = false;
        self.backdrop_elapsed_ms = 0.0;
    }

    fn advance(&mut self, elapsed_ms: f64) {
        let elapsed_ms = elapsed_ms.max(0.0);
        if self.backdrop_started {
            self.backdrop_elapsed_ms += elapsed_ms;
        }
        if self.box_animation.is_some() {
            self.box_elapsed_ms += elapsed_ms;
        }
        if self.speaker_animation.is_some() {
            self.speaker_elapsed_ms += elapsed_ms;
        }
    }

    fn backdrop_alpha(&self) -> f64 {
        if !self.backdrop_started {
            return 0.0;
        }
        if self.backdrop_fade_ms <= 0.0 {
            return 1.0;
        }
        let t = (self.backdrop_elapsed_ms / self.backdrop_fade_ms).clamp(0.0, 1.0);
        stepped_ease(10, t)
    }

    fn play_box(&mut self, requested: &str) {
        let Some(box_id) = self.active_box.clone() else {
            return;
        };
        let Some(asset) = self.boxes.get(&box_id) else {
            return;
        };
        let target = correct_animation_name(requested, |name| asset.sheet.clips.contains_key(name));
        let Some(target) = target else {
            return;
        };
        if self.box_animation.as_deref() == Some(target.as_str())
            && !box_clip_finished(asset, target.as_str(), self.box_elapsed_ms)
        {
            // FlxAnimation.play on the current, unfinished animation only
            // unpauses it (no restart).
            return;
        }
        self.box_animation = Some(target);
        self.box_elapsed_ms = 0.0;
    }

    fn play_speaker(&mut self, requested: &str) {
        let Some(speaker_id) = self.active_speaker.clone() else {
            return;
        };
        let Some(asset) = self.speakers.get(&speaker_id) else {
            return;
        };
        let target = correct_animation_name(requested, |name| asset.sheet.clips.contains_key(name));
        let Some(target) = target else {
            return;
        };
        if self.speaker_animation.as_deref() == Some(target.as_str())
            && !speaker_clip_finished(asset, target.as_str(), self.speaker_elapsed_ms)
        {
            return;
        }
        self.speaker_animation = Some(target);
        self.speaker_elapsed_ms = 0.0;
    }

    fn set_display(&mut self, text: &str) {
        self.display_text.clear();
        self.display_text.push_str(text);
        self.select_raster();
    }

    fn append_display(&mut self, text: &str) {
        let prefix = self.display_text.clone();
        self.prefix_chars = prefix.chars().count();
        self.display_text = prefix;
        self.display_text.push_str(text);
        self.select_raster();
    }

    fn select_raster(&mut self) {
        self.raster_error = None;
        let Some(box_id) = self.active_box.clone() else {
            self.current_key = None;
            return;
        };
        if self.display_text.is_empty() {
            self.current_key = None;
            return;
        }
        let key = (box_id.clone(), self.display_text.clone());
        if self.cache.contains_key(&key) {
            self.current_key = Some(key);
        } else {
            self.current_key = None;
            self.raster_error = Some(format!(
                "dialogue box {box_id:?} has no pre-baked glyph raster for {:?}",
                self.display_text
            ));
        }
    }

    fn current_pages(&self) -> (Option<usize>, Option<usize>) {
        let Some(key) = &self.current_key else {
            return (None, None);
        };
        let Some(cached) = self.cache.get(key) else {
            return (None, None);
        };
        (Some(cached.fill_page), cached.shadow_page)
    }

    fn box_origin(&self) -> Option<([f64; 2], &BoxAsset)> {
        let box_id = self.active_box.as_ref()?;
        let asset = self.boxes.get(box_id)?;
        let animation = self.box_animation.as_deref().unwrap_or("");
        let offsets = animation_offsets(&asset.data.animations, animation);
        // DialogueBox.hx L142-152: `globalOffsets` calls `screenCenter()`
        // while the group is still empty (640, 360), then stores that as
        // `originalPosition`; the group x/y is
        // `screenCenter + data.offsets + animationOffsets`.
        Some((
            [
                HALF_WIDTH + asset.data.offsets.0 + offsets[0],
                HALF_HEIGHT + asset.data.offsets.1 + offsets[1],
            ],
            asset,
        ))
    }

    fn text_origin(&self) -> Option<[f64; 2]> {
        let (origin, asset) = self.box_origin()?;
        // DialogueBox.hx L285-300: the text display is a child of the group
        // at (0,0) and is offset by `_data.text.offsets`; it is NOT a child
        // of the scaled box sprite.
        Some([
            origin[0] + asset.data.text.offsets.0,
            origin[1] + asset.data.text.offsets.1,
        ])
    }

    /// Returns `(is_pixel, quad)`; the runtime maps the sampler variant to
    /// its registered layer.
    fn box_quad(&self, projection: [f64; 6]) -> Option<(bool, Quad)> {
        let (origin, asset) = self.box_origin()?;
        let frame = box_frame(asset, self.box_animation.as_deref(), self.box_elapsed_ms)?;
        let quad = sprite_quad(
            frame,
            &asset.sheet,
            origin,
            asset.data.scale,
            asset.data.flip_x,
            asset.data.flip_y,
            projection,
        );
        Some((asset.data.is_pixel, quad))
    }

    fn speaker_quad(&self, projection: [f64; 6]) -> Option<(bool, Quad)> {
        let speaker_id = self.active_speaker.as_ref()?;
        let asset = self.speakers.get(speaker_id)?;
        let frame = speaker_frame(
            asset,
            self.speaker_animation.as_deref(),
            self.speaker_elapsed_ms,
        )?;
        let source = asset
            .sheet
            .frames
            .first()
            .map_or([0, 0], |frame| frame.source_size);
        let animation = self.speaker_animation.as_deref().unwrap_or("");
        let offsets = animation_offsets(&asset.data.animations, animation);
        // Speaker.hx L144-153: `screenCenter()` runs at scale 1, then
        // `globalOffsets`/`setScale` are applied. The sprite top-left is
        // `(screen - sourceSize/2) + data.offsets + animationOffsets`.
        let origin = [
            (GAME_WIDTH - f64::from(source[0])) / 2.0 + asset.data.offsets.0 + offsets[0],
            (GAME_HEIGHT - f64::from(source[1])) / 2.0 + asset.data.offsets.1 + offsets[1],
        ];
        let quad = sprite_quad(
            frame,
            &asset.sheet,
            origin,
            asset.data.scale,
            asset.data.flip_x,
            asset.data.flip_y.unwrap_or(false),
            projection,
        );
        Some((asset.data.is_pixel, quad))
    }

    fn text_quads(
        &self,
        projection: [f64; 6],
        reveal: usize,
    ) -> Result<(Vec<Quad>, Vec<Quad>), RenderError> {
        let Some(key) = &self.current_key else {
            return Ok((Vec::new(), Vec::new()));
        };
        let Some(cached) = self.cache.get(key) else {
            return Err(RenderError::Unsupported {
                detail: format!("missing glyph raster for {:?}", key.1),
            });
        };
        let Some(origin) = self.text_origin() else {
            return Ok((Vec::new(), Vec::new()));
        };
        let page_size = [
            f64::from(cached.raster.fill.width),
            f64::from(cached.raster.fill.height),
        ];
        let mut shadow = Vec::new();
        let mut fill = Vec::new();
        for glyph in &cached.raster.glyphs {
            if glyph.char_index >= reveal {
                continue;
            }
            if cached.shadow_page.is_some()
                && let Some(position) = glyph.shadow
            {
                shadow.push(glyph_quad(
                    position,
                    glyph.page_rect,
                    origin,
                    page_size,
                    projection,
                ));
            }
            fill.push(glyph_quad(
                glyph.screen,
                glyph.page_rect,
                origin,
                page_size,
                projection,
            ));
        }
        Ok((shadow, fill))
    }
}

impl ConversationSurface for DialogueSurface {
    fn box_animation(&self) -> &str {
        self.box_animation.as_deref().unwrap_or("")
    }

    fn box_animation_finished(&self) -> bool {
        let Some(box_id) = self.active_box.as_ref() else {
            return true;
        };
        let Some(asset) = self.boxes.get(box_id) else {
            return true;
        };
        self.box_animation
            .as_deref()
            .is_none_or(|name| box_clip_finished(asset, name, self.box_elapsed_ms))
    }

    fn on_entry_open(&mut self, entry: &DialogueEntryData, box_changed: bool) {
        if box_changed || self.active_box.as_deref() != Some(entry.box_id.as_str()) {
            self.active_box = Some(entry.box_id.clone());
            // A fresh/revived box has an empty text display until the
            // Opening animation completes and calls setText.
            self.display_text.clear();
            self.prefix_chars = 0;
            self.current_key = None;
            self.raster_error = None;
        }
        if !self.backdrop_started {
            self.backdrop_started = true;
            self.backdrop_elapsed_ms = 0.0;
        }
        self.play_box(&entry.box_animation);
    }

    fn on_speaker_show(&mut self, entry: &DialogueEntryData, speaker_changed: bool) {
        if speaker_changed || self.active_speaker.as_deref() != Some(entry.speaker.as_str()) {
            self.active_speaker = Some(entry.speaker.clone());
            self.speaker_animation = None;
            self.speaker_elapsed_ms = 0.0;
        }
        self.play_speaker(&entry.speaker_animation);
    }

    fn on_line_set(&mut self, text: &str, _typed_len: usize) {
        self.prefix_chars = 0;
        self.set_display(text);
    }

    fn on_line_append(&mut self, text: &str, _typed_len: usize) {
        self.append_display(text);
    }
}

fn box_frame(asset: &BoxAsset, animation: Option<&str>, elapsed_ms: f64) -> Option<DialogueFrame> {
    if let Some(animation) = animation
        && let Some(clip) = asset.sheet.clips.get(animation)
    {
        return clip_frame(&asset.sheet, clip, elapsed_ms)
            .or_else(|| asset.sheet.frames.first().copied());
    }
    asset.sheet.frames.first().copied()
}

fn speaker_frame(
    asset: &SpeakerAsset,
    animation: Option<&str>,
    elapsed_ms: f64,
) -> Option<DialogueFrame> {
    if let Some(animation) = animation
        && let Some(clip) = asset.sheet.clips.get(animation)
    {
        return clip_frame(&asset.sheet, clip, elapsed_ms)
            .or_else(|| asset.sheet.frames.first().copied());
    }
    asset.sheet.frames.first().copied()
}

fn box_clip_finished(asset: &BoxAsset, animation: &str, elapsed_ms: f64) -> bool {
    asset
        .sheet
        .clips
        .get(animation)
        .is_none_or(|clip| clip_finished(clip, elapsed_ms))
}

fn speaker_clip_finished(asset: &SpeakerAsset, animation: &str, elapsed_ms: f64) -> bool {
    asset
        .sheet
        .clips
        .get(animation)
        .is_none_or(|clip| clip_finished(clip, elapsed_ms))
}

/// `FlxAnimation.update`: strict `>` frame timer, looped clips wrap, one-shot
/// clips finish after the last frame is displayed. `fps <= 0` is the loader's
/// static-clip form and is finished immediately.
fn clip_finished(clip: &DialogueClip, elapsed_ms: f64) -> bool {
    if clip.looped {
        return false;
    }
    if clip.fps <= 0.0 || clip.frames.is_empty() {
        return true;
    }
    let duration_ms = clip.frames.len() as f64 / clip.fps * 1000.0;
    elapsed_ms > duration_ms
}

fn clip_frame(
    sheet: &DialogueSheet,
    clip: &DialogueClip,
    elapsed_ms: f64,
) -> Option<DialogueFrame> {
    let first = clip.frames.first().copied()?;
    if clip.fps <= 0.0 {
        return sheet.frames.get(first).copied();
    }
    let frame_duration = 1000.0 / clip.fps;
    // FlxAnimation advances only while `_frameTimer > frameDuration`; the
    // ceil-minus-one form matches the HUD icon implementation at an exact
    // frame boundary.
    let advances = if elapsed_ms <= 0.0 {
        0
    } else {
        ((elapsed_ms / frame_duration).ceil() as usize).saturating_sub(1)
    };
    let index = if clip.looped {
        advances % clip.frames.len()
    } else {
        advances.min(clip.frames.len() - 1)
    };
    clip.frames
        .get(index)
        .and_then(|frame| sheet.frames.get(*frame))
        .copied()
}

fn animation_offsets(animations: &[AnimationData], name: &str) -> [f64; 2] {
    animations
        .iter()
        .find(|animation| animation.name == name)
        .and_then(|animation| animation.offsets)
        .map(|(x, y)| {
            if x.is_finite() && y.is_finite() {
                [x, y]
            } else {
                [0.0, 0.0]
            }
        })
        .unwrap_or([0.0, 0.0])
}

/// One sprite quad using the flixel trim geometry: the subtexture is drawn at
/// `origin + frame.offset * scale` at its natural trimmed size; `source_size`
/// is only the hitbox (`screenCenter`) dimension.
fn sprite_quad(
    frame: DialogueFrame,
    sheet: &DialogueSheet,
    origin: [f64; 2],
    scale: f64,
    flip_x: bool,
    flip_y: bool,
    projection: [f64; 6],
) -> Quad {
    let left = origin[0] + f64::from(frame.offset[0]) * scale;
    let top = origin[1] + f64::from(frame.offset[1]) * scale;
    let matrix = concat(projection, [scale, 0.0, 0.0, scale, left, top]);
    let mut uv = quad_uv(frame.rect, sheet.page_size);
    if flip_x {
        uv.swap(0, 1);
        uv.swap(2, 3);
    }
    if flip_y {
        uv.swap(0, 3);
        uv.swap(1, 2);
    }
    quad_from_matrix(
        matrix,
        f64::from(frame.rect[2]),
        f64::from(frame.rect[3]),
        uv,
    )
}

fn glyph_quad(
    position: [f64; 2],
    rect: [u32; 4],
    origin: [f64; 2],
    page_size: [f64; 2],
    projection: [f64; 6],
) -> Quad {
    let matrix = concat(
        projection,
        [
            1.0,
            0.0,
            0.0,
            1.0,
            origin[0] + position[0],
            origin[1] + position[1],
        ],
    );
    let x0 = f64::from(rect[0]) / page_size[0];
    let y0 = f64::from(rect[1]) / page_size[1];
    let x1 = f64::from(rect[0] + rect[2]) / page_size[0];
    let y1 = f64::from(rect[1] + rect[3]) / page_size[1];
    quad_from_matrix(
        matrix,
        f64::from(rect[2]),
        f64::from(rect[3]),
        [[x0, y0], [x1, y0], [x1, y1], [x0, y1]],
    )
}

/// `EaseUtil.stepped(steps)`: `floor(t * steps) / steps`.
fn stepped_ease(steps: u32, t: f64) -> f64 {
    let steps = f64::from(steps);
    (t * steps).floor() / steps
}

fn text_colors(data: &DialogueBoxData) -> Result<([f32; 4], Option<[f32; 4]>), RenderError> {
    let fill = parse_hex_color(&data.text.color)?;
    // `borderSize = shadowWidth ?? 2`; width 0 disables the shadow pass.
    // [D] `raster_glyph_page` has the observed 2 px shadow offset baked in;
    // the shipped boxes use 0 or 2 only.
    let shadow = if data.text.shadow_width.unwrap_or(2) <= 0 {
        None
    } else {
        data.text
            .shadow_color
            .as_deref()
            .map(parse_hex_color)
            .transpose()?
            .filter(|color| color[3] > 0.0)
    };
    Ok((fill, shadow))
}

fn text_size(data: &DialogueBoxData) -> f64 {
    data.text
        .size
        .map(|size| size as f64)
        .filter(|size| size.is_finite() && *size > 0.0)
        .unwrap_or(TEXT_SIZE)
}

fn bake_line_rasters(
    dialogue: &LoadedDialogue,
    pages: &mut Vec<PageImage>,
) -> Result<BTreeMap<(String, String), CachedRaster>, RenderError> {
    let mut cache = BTreeMap::new();
    for entry in &dialogue.conversation.dialogue {
        let Some(box_asset) = dialogue.boxes.get(&entry.box_id) else {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "dialogue entry references box {:?}, which the loader did not load",
                    entry.box_id
                ),
            });
        };
        let (fill, shadow) = text_colors(&box_asset.data)?;
        let size = text_size(&box_asset.data);
        let field_width = box_asset.data.text.width as f64;
        let mut display = String::new();
        for line in &entry.text {
            display.push_str(line);
            if display.is_empty() {
                continue;
            }
            let key = (entry.box_id.clone(), display.clone());
            if cache.contains_key(&key) {
                continue;
            }
            let raster = raster_glyph_page(
                &dialogue.font_bytes,
                size,
                fill,
                shadow,
                &display,
                field_width,
                TEXT_LETTER_SPACING,
            )?;
            let fill_page = push_glyph_page(pages, &raster.fill, box_asset.data.is_pixel)?;
            let shadow_page = match &raster.shadow {
                Some(page) => Some(push_glyph_page(pages, page, box_asset.data.is_pixel)?),
                None => None,
            };
            cache.insert(
                key,
                CachedRaster {
                    raster,
                    fill_page,
                    shadow_page,
                },
            );
        }
    }
    Ok(cache)
}

fn push_glyph_page(
    pages: &mut Vec<PageImage>,
    page: &GlyphPage,
    is_pixel: bool,
) -> Result<usize, RenderError> {
    let index = pages.len();
    pages.push(PageImage::from_rgba(
        page.width,
        page.height,
        page.rgba.clone(),
        is_pixel,
    )?);
    Ok(index)
}

fn parse_hex_color(value: &str) -> Result<[f32; 4], RenderError> {
    let Some(hex) = value.strip_prefix('#') else {
        return Err(RenderError::Unsupported {
            detail: format!("dialogue color {value:?} is not #RRGGBB or #RRGGBBAA"),
        });
    };
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RenderError::Unsupported {
            detail: format!("dialogue color {value:?} is not #RRGGBB or #RRGGBBAA"),
        });
    }
    let channel = |index: usize| -> Result<f32, RenderError> {
        u8::from_str_radix(&hex[index..index + 2], 16)
            .map(|value| f32::from(value) / 255.0)
            .map_err(|error| RenderError::Unsupported {
                detail: format!("dialogue color {value:?} has an invalid channel: {error}"),
            })
    };
    Ok([
        channel(0)?,
        channel(2)?,
        channel(4)?,
        if hex.len() == 8 { channel(6)? } else { 1.0 },
    ])
}

/// Resolves the sparrow XML + PNG page pair for one dialogue asset key.
///
/// The XML is the animation source; the PNG is the same key's page. Both use
/// the `image_file_bases` candidate order with the fixed `week6` level
/// directory (every shipped dialogue asset lives there).
fn resolve_sheet_sources(
    vfs: &Vfs,
    key: &str,
) -> Result<(String, Vec<u8>, String, Vec<u8>), RenderError> {
    let bases = image_file_bases(key, Some("week6"));
    let xml_candidates: Vec<String> = bases.iter().map(|base| format!("{base}.xml")).collect();
    let png_candidates: Vec<String> = bases.iter().map(|base| format!("{base}.png")).collect();
    let read_first = |paths: &[String]| -> Result<Option<(String, Vec<u8>)>, RenderError> {
        for path in paths {
            if let Some(source) = vfs.resolve(path) {
                let bytes =
                    vfs.read(&source, ASSET_SIZE_LIMIT)
                        .map_err(|source| RenderError::Read {
                            path: path.clone(),
                            source,
                        })?;
                return Ok(Some((path.clone(), bytes)));
            }
        }
        Ok(None)
    };
    let xml = read_first(&xml_candidates)?.ok_or_else(|| RenderError::MissingAsset {
        path: xml_candidates[0].clone(),
    })?;
    let png = read_first(&png_candidates)?.ok_or_else(|| RenderError::MissingAsset {
        path: png_candidates[0].clone(),
    })?;
    Ok((xml.0, xml.1, png.0, png.1))
}

fn parse_sparrow_sheet(path: &str, bytes: &[u8]) -> Result<oxide_atlas::Atlas, RenderError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| RenderError::Decode {
        path: path.to_string(),
        detail: error.to_string(),
    })?;
    oxide_atlas::parse_sparrow(&text).map_err(|source| RenderError::Unsupported {
        detail: format!("dialogue sheet {path:?} failed to parse: {source}"),
    })
}

fn parse_dialogue_document(
    path: &str,
    bytes: &[u8],
) -> Result<oxide_data::dialogue::DialogueDocument, RenderError> {
    oxide_data::dialogue::parse_dialogue_file(Path::new(path), bytes).map_err(|source| {
        RenderError::Unsupported {
            detail: format!("dialogue document {path:?} failed to parse: {source}"),
        }
    })
}

/// Loads the conversation `id` with every referenced box and speaker.
pub(crate) fn load_dialogue(
    vfs: &Vfs,
    pages: &mut PageStore,
    id: &str,
) -> Result<LoadedDialogue, RenderError> {
    let conversation_path = format!("preload/data/dialogue/conversations/{id}.json");
    let source = vfs
        .resolve(&conversation_path)
        .ok_or_else(|| RenderError::MissingAsset {
            path: conversation_path.clone(),
        })?;
    let bytes = vfs
        .read(&source, ASSET_SIZE_LIMIT)
        .map_err(|source| RenderError::Read {
            path: conversation_path.clone(),
            source,
        })?;
    let document = parse_dialogue_document(&conversation_path, &bytes)?;
    let oxide_data::dialogue::DialogueTyped::Conversation(conversation) = document.typed else {
        return Err(RenderError::Unsupported {
            detail: format!("dialogue document {conversation_path:?} is not a conversation"),
        });
    };
    let conversation = *conversation;

    let mut box_ids: Vec<String> = Vec::new();
    let mut speaker_ids: Vec<String> = Vec::new();
    for entry in &conversation.dialogue {
        if !box_ids.iter().any(|existing| existing == &entry.box_id) {
            box_ids.push(entry.box_id.clone());
        }
        if !speaker_ids
            .iter()
            .any(|existing| existing == &entry.speaker)
        {
            speaker_ids.push(entry.speaker.clone());
        }
    }

    let mut boxes = BTreeMap::new();
    for box_id in &box_ids {
        let data_path = format!("preload/data/dialogue/boxes/{box_id}.json");
        let source = vfs
            .resolve(&data_path)
            .ok_or_else(|| RenderError::MissingAsset {
                path: data_path.clone(),
            })?;
        let bytes = vfs
            .read(&source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: data_path.clone(),
                source,
            })?;
        let document = parse_dialogue_document(&data_path, &bytes)?;
        let oxide_data::dialogue::DialogueTyped::Box(box_data) = document.typed else {
            return Err(RenderError::Unsupported {
                detail: format!("dialogue document {data_path:?} is not a dialogue box"),
            });
        };
        let data = *box_data;
        let (xml_path, xml_bytes, png_path, _png_bytes) =
            resolve_sheet_sources(vfs, &data.asset_path)?;
        let atlas = parse_sparrow_sheet(&xml_path, &xml_bytes)?;
        let (linear_page, page_size) = pages.load_png_file(vfs, &png_path, false)?;
        let (pixel_page, pixel_size) = pages.load_png_file(vfs, &png_path, true)?;
        if page_size != pixel_size {
            return Err(RenderError::Unsupported {
                detail: format!("dialogue box page {png_path:?} decoded to two sizes"),
            });
        }
        let sheet = DialogueSheet::from_atlas(page_size, &atlas, &data.animations)?;
        boxes.insert(
            box_id.clone(),
            BoxAsset {
                data,
                linear_page,
                pixel_page,
                sheet,
            },
        );
    }

    let mut speakers = BTreeMap::new();
    for speaker_id in &speaker_ids {
        let data_path = format!("preload/data/dialogue/speakers/{speaker_id}.json");
        let source = vfs
            .resolve(&data_path)
            .ok_or_else(|| RenderError::MissingAsset {
                path: data_path.clone(),
            })?;
        let bytes = vfs
            .read(&source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: data_path.clone(),
                source,
            })?;
        let document = parse_dialogue_document(&data_path, &bytes)?;
        let oxide_data::dialogue::DialogueTyped::Speaker(speaker_data) = document.typed else {
            return Err(RenderError::Unsupported {
                detail: format!("dialogue document {data_path:?} is not a speaker"),
            });
        };
        let data = *speaker_data;
        let (xml_path, xml_bytes, png_path, _png_bytes) =
            resolve_sheet_sources(vfs, &data.asset_path)?;
        let atlas = parse_sparrow_sheet(&xml_path, &xml_bytes)?;
        let (linear_page, page_size) = pages.load_png_file(vfs, &png_path, false)?;
        let (pixel_page, pixel_size) = pages.load_png_file(vfs, &png_path, true)?;
        if page_size != pixel_size {
            return Err(RenderError::Unsupported {
                detail: format!("speaker page {png_path:?} decoded to two sizes"),
            });
        }
        let sheet = DialogueSheet::from_atlas(page_size, &atlas, &data.animations)?;
        speakers.insert(
            speaker_id.clone(),
            SpeakerAsset {
                data,
                linear_page,
                pixel_page,
                sheet,
            },
        );
    }

    let font_path = "fonts/pixel.otf";
    let font_source = vfs
        .resolve(font_path)
        .ok_or_else(|| RenderError::MissingAsset {
            path: font_path.to_string(),
        })?;
    let font_bytes =
        vfs.read(&font_source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: font_path.to_string(),
                source,
            })?;

    Ok(LoadedDialogue {
        conversation_id: id.to_string(),
        conversation,
        boxes,
        speakers,
        font_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_senpai_walkthrough() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let mut runtime = senpai_runtime(&root);
        runtime.start("senpai").expect("start");
        let mut open_tick = None;
        for tick in 0..20_000u64 {
            runtime.begin_tick(tick as f64);
            if runtime.core().typed_len() > 0 {
                open_tick = Some(tick);
                break;
            }
        }
        println!("open_tick = {:?}", open_tick);
        runtime.begin_tick(open_tick.unwrap() as f64 + 120.0);
        println!("after +120: typed={}", runtime.core().typed_len());
        runtime.advance();
        println!(
            "after adv1: line={:?} typed={}",
            runtime
                .core()
                .line_text()
                .chars()
                .take(24)
                .collect::<String>(),
            runtime.core().typed_len()
        );
        runtime.advance();
        println!(
            "after adv2: line={:?} typed={}",
            runtime
                .core()
                .line_text()
                .chars()
                .take(24)
                .collect::<String>(),
            runtime.core().typed_len()
        );
    }

    /// Asset-gated: the real senpai conversation load (data + box + speaker
    /// sheets + font) through the shipped tree.
    fn senpai_runtime(root: &std::ffi::OsStr) -> ConversationRuntime {
        let vfs = Vfs::new(std::path::PathBuf::from(root));
        let mut pages = PageStore::default();
        let loaded = load_dialogue(&vfs, &mut pages, "senpai").expect("senpai dialogue loads");
        let mut specs = Vec::new();
        let mut page_images = pages.into_pages();
        ConversationRuntime::new(loaded, &mut specs, &mut page_images).expect("runtime builds")
    }

    /// The runtime drives the frozen machine rules over the real payload:
    /// the trigger starts exactly once, the typewriter advances on the 1 ms
    /// clock, the advance input moves through the entries, and a completed
    /// conversation restarts on a later trigger (native kill/revive).
    #[test]
    fn senpai_runtime_start_typewriter_advance_and_restart() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let mut runtime = senpai_runtime(&root);
        assert!(!runtime.active());
        // The trigger row starts it; a second row while active is a no-op.
        runtime.start("senpai").expect("trigger starts");
        runtime
            .start("senpai")
            .expect("an active trigger is idempotent");
        // The text appears after the box 'enter' animation finishes
        // (ConversationState Opening -> Speaking), then the typewriter
        // reveals 20 chars/s on the 1 ms grid.
        let mut open_tick = None;
        for tick in 0..20_000u64 {
            runtime.begin_tick(tick as f64);
            if runtime.core().typed_len() > 0 {
                open_tick = Some(tick);
                break;
            }
        }
        let open_tick = open_tick.expect("the first entry types");
        assert!(open_tick > 0, "the box animation plays before the text");
        runtime.begin_tick(open_tick as f64 + 120.0);
        assert!(
            runtime.core().typed_len() > 1,
            "the typewriter reveals on the 1 ms grid (20 chars/s)"
        );
        // Skip the typing, then advance to the next entry.
        runtime.advance();
        let next_line = runtime.core().line_text().to_string();
        runtime.advance();
        assert_ne!(
            runtime.core().line_text(),
            next_line,
            "the next entry opens with its own text"
        );
        // Exhaust the remaining entries; the completion deactivates the
        // runtime and a later trigger restarts it (kill/revive).
        for _ in 0..8 {
            runtime.advance();
        }
        runtime.begin_tick(100_000.0);
        assert!(runtime.completed());
        assert!(!runtime.active());
        runtime.start("senpai").expect("restart after completion");
        assert!(runtime.active());
    }

    /// The composed draw path fills the cutscene layers in z order
    /// (backdrop < speaker < box < text shadow < text fill) with the
    /// per-frame colors synced into the scene's layer specs.
    #[test]
    fn senpai_runtime_draw_fills_layers_after_hud() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let mut runtime = senpai_runtime(&root);
        runtime.start("senpai").expect("trigger starts");
        // Drive forward until the text is typing (the box enter animation
        // precedes it), then compose.
        let mut typed = false;
        for tick in 0..20_000u64 {
            runtime.begin_tick(tick as f64);
            if runtime.core().typed_len() > 2 {
                typed = true;
                break;
            }
        }
        assert!(typed, "the runtime reaches the typing state");
        let quads = runtime.draw(SCREEN_PROJECTION).expect("draw composes");
        assert!(quads[0].len() == 1, "backdrop quad");
        assert!(quads.len() >= 5);
        let total: usize = quads.iter().map(|layer| layer.len()).sum();
        assert!(total >= 3, "speaker/box/text quads compose: {total}");

        // The synced specs carry the per-frame alpha into the layer colors.
        let mut specs = Vec::new();
        let count = runtime.layers().count;
        specs.resize(
            count,
            LayerSpec {
                page: None,
                color: [0.0, 0.0, 0.0, 0.0],
            },
        );
        runtime.sync_layer_specs(&mut specs).expect("specs sync");
        assert!(
            specs.iter().any(|spec| spec.color[3] > 0.0),
            "the backdrop/outro alphas reach the material colors"
        );
    }

    #[test]
    fn real_senpai_dialogue_loads() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let vfs = Vfs::new(std::path::PathBuf::from(&root));
        let mut pages = PageStore::default();
        let loaded = load_dialogue(&vfs, &mut pages, "senpai").expect("senpai dialogue loads");
        assert_eq!(loaded.conversation_id, "senpai");
        assert_eq!(loaded.conversation.dialogue.len(), 3);
        assert!(loaded.boxes.contains_key("roses"), "the roses box loads");
        assert!(loaded.speakers.contains_key("senpai"));
        assert!(loaded.speakers.contains_key("bf-pixel"));
        assert!(!loaded.font_bytes.is_empty());
        // The week6 library resolution found the shipped sheets.
        let roses = &loaded.boxes["roses"];
        assert!(roses.sheet.clips.contains_key("enter"));
        let senpai = &loaded.speakers["senpai"];
        assert!(senpai.sheet.clips.contains_key("talkEnter"));
    }
}
