//! Week-6 in-game dialogue state machine and pixel-font glyph rasterizer.
//!
//! The behavior is frozen by `docs/M6-CONTRACT.md` §14 (6E): the native
//! `Conversation` state machine (`Start -> Opening -> Speaking -> Idle ->
//! Ending`), the `FlxTypeText` batch typewriter, the `EaseUtil.stepped(8)`
//! outro and the `correctAnimationName` suffix-strip -> idle fallback.
//!
//! This module owns no Bevy pages. Box/speaker/text surfaces are abstracted
//! behind [`ConversationSurface`], so the hermetic tests drive the machine
//! with a recording fake and the later 6E scene slice wires the real sprites.
//! [`raster_glyph_page`] is the pure ab_glyph half of that later load-path
//! baking (`fonts/pixel.otf`, size 32).
//!
//! [D] `music_volume` is a virtual scalar: the play session/audio seam owns
//! playback; this module only records the native fade values.
//! [D] The entry `speed` field is inert on the observed path (`Conversation.hx`
//! never assigns `DialogueBox.speed`), so the pinned 0.05 s/char delay stays
//! constant.

#![allow(dead_code)] // consumed by the 6E scene/play slices in the next tasks

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use oxide_data::dialogue::{ConversationData, DialogueEntryData, OutroData};

use super::RenderError;

/// `FlxTypeText.delay` at the native default 1.0x entry speed.
pub(crate) const TYPE_DELAY_SECONDS: f64 = 0.05;
/// Observed dialogue font size (`DialogueBox.loadText` creates `FunkinTypeText`
/// at 32 px with the embedded pixel font).
pub(crate) const TEXT_SIZE: f64 = 32.0;
/// Native text `letterSpacing` (`FlxText.letterSpacing = -1` in the week-6
/// pixel presentation).
pub(crate) const TEXT_LETTER_SPACING: f64 = -1.0;
/// `shadowWidth` default 2: the shadow pass draws every glyph at (+2, +2).
pub(crate) const SHADOW_OFFSET: f64 = 2.0;
/// The fixed shelf width used when packing baked glyphs into a page.
const GLYPH_PAGE_WIDTH: u32 = 1024;
/// One transparent pixel between packed glyphs (nearest sampling needs no
/// bleed margin, but the gap keeps page rects unambiguous).
const GLYPH_PADDING: u32 = 1;

/// The five native `ConversationState` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConversationPhase {
    Start,
    Opening,
    Speaking,
    Idle,
    Ending,
}

/// One linear tween value. Native music fades use the default linear ease.
#[derive(Debug, Clone, Copy)]
struct LinearFade {
    from: f64,
    to: f64,
    duration: f64,
    elapsed: f64,
}

/// The output/query surface the later 6E load/scene slice implements.
///
/// Default methods keep the hermetic fake small and let the real surface
/// implement only what it draws. `box_animation`/`box_animation_finished`
/// mirror `DialogueBox.getCurrentAnimation()`/`isAnimationFinished()`; the
/// `on_*` callbacks mirror the native state-machine side effects.
pub(crate) trait ConversationSurface {
    /// The box sprite's current animation name (`""` when none).
    fn box_animation(&self) -> &str {
        ""
    }

    /// `DialogueBox.isAnimationFinished()`.
    fn box_animation_finished(&self) -> bool {
        false
    }

    /// Entry open: swap/revive the box when `box_changed`, then play the
    /// entry's `boxAnimation` (with the native suffix-strip -> idle fallback).
    fn on_entry_open(&mut self, _entry: &DialogueEntryData, _box_changed: bool) {}

    /// Opening complete: swap/revive the speaker when `speaker_changed`, then
    /// play the entry's `speakerAnimation`.
    fn on_speaker_show(&mut self, _entry: &DialogueEntryData, _speaker_changed: bool) {}

    /// First line of an entry (`setText`): replace the visible text and start
    /// typing from `typed_len` (always 0 here).
    fn on_line_set(&mut self, _text: &str, _typed_len: usize) {}

    /// A later line of the same entry (`appendText`): keep the already-typed
    /// prefix and type the new text after it.
    fn on_line_append(&mut self, _text: &str, _typed_len: usize) {}

    /// Native `onTypingComplete` (the box script plays `sentenceEnd` here).
    fn on_typing_complete(&mut self) {}

    /// `DIALOGUE_END`: the box script hides the text and plays `exit`.
    fn on_dialogue_end(&mut self) {}

    /// The outro finished (native DESTROY + `completeCallback`).
    fn on_completed(&mut self) {}
}

/// The native conversation machine without any rendering surfaces.
pub(crate) struct ConversationState {
    conversation: ConversationData,
    phase: ConversationPhase,
    entry_index: usize,
    line_index: usize,
    /// Full text of the current line (`_finalText`).
    line_text: String,
    /// Native `_length` over the current line's text.
    typed_len: usize,
    /// Native `_timer` in seconds.
    type_timer: f64,
    typing: bool,
    /// Last box id shown (native `currentDialogueBox.id`).
    current_box: Option<String>,
    /// Last speaker id shown (native `currentSpeaker.id`).
    current_speaker: Option<String>,
    /// Clock of the previous `begin_tick`; `None` until the first tick.
    last_tick_ms: Option<f64>,
    outro_started: bool,
    outro_duration: f64,
    outro_elapsed: f64,
    outro_alpha: f64,
    /// [D] Virtual music volume consumed by the play/audio seam.
    music_volume: f64,
    music_fade: Option<LinearFade>,
    completed: bool,
}

impl ConversationState {
    /// Builds a machine in the native `Start` phase.
    pub(crate) fn new(conversation: ConversationData) -> Self {
        Self {
            conversation,
            phase: ConversationPhase::Start,
            entry_index: 0,
            line_index: 0,
            line_text: String::new(),
            typed_len: 0,
            type_timer: 0.0,
            typing: false,
            current_box: None,
            current_speaker: None,
            last_tick_ms: None,
            outro_started: false,
            outro_duration: 0.0,
            outro_elapsed: 0.0,
            outro_alpha: 1.0,
            music_volume: 0.0,
            music_fade: None,
            completed: false,
        }
    }

    pub(crate) fn phase(&self) -> ConversationPhase {
        self.phase
    }

    pub(crate) fn entry_index(&self) -> usize {
        self.entry_index
    }

    pub(crate) fn line_index(&self) -> usize {
        self.line_index
    }

    /// Full text of the current line (even while it is still typing).
    pub(crate) fn line_text(&self) -> &str {
        &self.line_text
    }

    /// Number of visible characters of the current line (`_length`).
    pub(crate) fn typed_len(&self) -> usize {
        self.typed_len
    }

    /// Outro alpha (`1.0` until the fade starts).
    pub(crate) fn outro_alpha(&self) -> f64 {
        self.outro_alpha
    }

    /// [D] Virtual music volume (`0..=1`).
    pub(crate) fn music_volume(&self) -> f64 {
        self.music_volume
    }

    /// Whether the outro finished and the completion callback fired.
    pub(crate) fn completed(&self) -> bool {
        self.completed
    }

    /// One native frame update. `now_ms` is the same monotonic millisecond
    /// clock the scene uses; the first call establishes the clock (elapsed 0).
    pub(crate) fn begin_tick(&mut self, now_ms: f64, surface: &mut impl ConversationSurface) {
        let elapsed = self.elapsed_seconds(now_ms);
        if self.completed {
            return;
        }

        // Native order: the group updates its text display first (the
        // typewriter completion callback sets Idle), then
        // `Conversation.onUpdate` runs the phase switch.
        self.advance_typewriter(elapsed, surface);
        self.advance_music(elapsed);

        match self.phase {
            ConversationPhase::Start => {}
            ConversationPhase::Opening => {
                // Native: `isAnimationFinished()` OR the current animation no
                // longer matches the entry's requested `boxAnimation`.
                let entry_animation = self
                    .conversation
                    .dialogue
                    .get(self.entry_index)
                    .map_or("", |entry| entry.box_animation.as_str());
                if surface.box_animation_finished() || surface.box_animation() != entry_animation {
                    self.open_line(surface);
                }
            }
            ConversationPhase::Speaking | ConversationPhase::Idle => {}
            ConversationPhase::Ending => {
                if !self.outro_started {
                    self.start_outro(surface);
                } else {
                    self.advance_outro(elapsed, surface);
                }
            }
        }
    }

    /// `CUTSCENE_ADVANCE` (`Conversation.advanceConversation`).
    pub(crate) fn advance(&mut self, surface: &mut impl ConversationSurface) {
        if self.completed {
            return;
        }
        match self.phase {
            ConversationPhase::Start => self.start(surface),
            ConversationPhase::Opening | ConversationPhase::Speaking => {
                self.skip_typing(surface);
            }
            ConversationPhase::Idle => self.next_line(surface),
            ConversationPhase::Ending => self.finish(surface),
        }
    }

    /// `skipConversation`: `DIALOGUE_END` immediately, skipping the outro.
    pub(crate) fn skip(&mut self, surface: &mut impl ConversationSurface) {
        if self.completed || self.phase == ConversationPhase::Ending {
            return;
        }
        self.enter_ending(surface);
    }

    fn elapsed_seconds(&mut self, now_ms: f64) -> f64 {
        if !now_ms.is_finite() {
            return 0.0;
        }
        let elapsed = match self.last_tick_ms {
            Some(previous) => (now_ms - previous).max(0.0) / 1000.0,
            None => 0.0,
        };
        self.last_tick_ms = Some(now_ms);
        elapsed
    }

    /// `FlxTypeText.update`: accumulate, batch-add `trunc(timer / delay)`,
    /// keep the remainder, and complete when `_length` reaches the text end.
    fn advance_typewriter(&mut self, elapsed: f64, surface: &mut impl ConversationSurface) {
        let line_len = self.line_text.chars().count();
        if !self.typing || self.typed_len >= line_len {
            return;
        }
        self.type_timer += elapsed;
        if self.type_timer < TYPE_DELAY_SECONDS {
            return;
        }
        let batch = (self.type_timer / TYPE_DELAY_SECONDS).trunc() as usize;
        self.typed_len = (self.typed_len + batch).min(line_len);
        self.type_timer %= TYPE_DELAY_SECONDS;
        if self.typed_len >= line_len {
            self.complete_typing(surface);
        }
    }

    fn complete_typing(&mut self, surface: &mut impl ConversationSurface) {
        self.typing = false;
        self.type_timer = 0.0;
        self.phase = ConversationPhase::Idle;
        surface.on_typing_complete();
    }

    /// `setupMusic` + `onDialogueStart`: fade the music in and open entry 0.
    fn start(&mut self, surface: &mut impl ConversationSurface) {
        if self.conversation.dialogue.is_empty() {
            self.enter_ending(surface);
            return;
        }
        if let Some(music) = &self.conversation.music
            && !music.asset.is_empty()
        {
            if music.fade_time.is_finite() && music.fade_time > 0.0 {
                self.music_fade = Some(LinearFade {
                    from: 0.0,
                    to: 1.0,
                    duration: music.fade_time,
                    elapsed: 0.0,
                });
            } else {
                self.music_volume = 1.0;
            }
        }
        self.entry_index = 0;
        self.open_entry(surface);
    }

    /// `showCurrentDialogueBox` + `playDialogueBoxAnimation`.
    fn open_entry(&mut self, surface: &mut impl ConversationSurface) {
        let Some(entry) = self.conversation.dialogue.get(self.entry_index).cloned() else {
            self.enter_ending(surface);
            return;
        };
        self.line_index = 0;
        self.line_text = entry.text.first().cloned().unwrap_or_default();
        self.typed_len = 0;
        self.type_timer = 0.0;
        self.typing = false;
        let box_changed = self.current_box.as_deref() != Some(entry.box_id.as_str());
        if box_changed {
            self.current_box = Some(entry.box_id.clone());
        }
        self.phase = ConversationPhase::Opening;
        surface.on_entry_open(&entry, box_changed);
    }

    /// `showCurrentSpeaker` + `playSpeakerAnimation` + `setText`.
    fn open_line(&mut self, surface: &mut impl ConversationSurface) {
        let Some(entry) = self.conversation.dialogue.get(self.entry_index).cloned() else {
            self.enter_ending(surface);
            return;
        };
        self.phase = ConversationPhase::Speaking;
        let speaker_changed = self.current_speaker.as_deref() != Some(entry.speaker.as_str());
        if speaker_changed {
            self.current_speaker = Some(entry.speaker.clone());
        }
        surface.on_speaker_show(&entry, speaker_changed);
        let text = entry.text.get(self.line_index).cloned().unwrap_or_default();
        self.set_line(text, false, surface);
    }

    fn set_line(&mut self, text: String, append: bool, surface: &mut impl ConversationSurface) {
        self.line_text = text;
        self.typed_len = 0;
        self.type_timer = 0.0;
        self.typing = true;
        self.phase = ConversationPhase::Speaking;
        if self.line_text.is_empty() {
            // Native `setText("")` never fires the completion callback because
            // the helper string does not change. Fail closed instead of
            // hanging the conversation on an empty line.
            self.typing = false;
            self.phase = ConversationPhase::Idle;
            surface.on_line_set(&self.line_text, 0);
            surface.on_typing_complete();
        } else if append {
            surface.on_line_append(&self.line_text, 0);
        } else {
            surface.on_line_set(&self.line_text, 0);
        }
    }

    /// `onDialogueLine` in the Idle state.
    fn next_line(&mut self, surface: &mut impl ConversationSurface) {
        let Some(entry) = self.conversation.dialogue.get(self.entry_index).cloned() else {
            self.enter_ending(surface);
            return;
        };
        self.line_index += 1;
        if self.line_index >= entry.text.len() {
            self.line_index = 0;
            self.entry_index += 1;
            if self.entry_index >= self.conversation.dialogue.len() {
                self.enter_ending(surface);
            } else {
                self.open_entry(surface);
            }
        } else {
            let text = entry.text[self.line_index].clone();
            self.set_line(text, true, surface);
        }
    }

    /// `onDialogueCompleteLine` -> `DialogueBox.skip()`.
    fn skip_typing(&mut self, surface: &mut impl ConversationSurface) {
        if self.phase == ConversationPhase::Speaking && self.typing {
            self.typed_len = self.line_text.chars().count();
            self.complete_typing(surface);
        } else {
            // Native `DialogueBox.skip()` on an inactive/empty text display
            // does not fire the completion callback. The frozen advance rule
            // still moves the state to Idle; an Opening entry's first line is
            // skipped without a setText.
            self.typing = false;
            self.type_timer = 0.0;
            self.phase = ConversationPhase::Idle;
        }
    }

    fn enter_ending(&mut self, surface: &mut impl ConversationSurface) {
        if self.phase == ConversationPhase::Ending {
            return;
        }
        self.phase = ConversationPhase::Ending;
        self.typing = false;
        self.type_timer = 0.0;
        surface.on_dialogue_end();
    }

    /// `startOutro`: fade alpha with `EaseUtil.stepped(8)` and fade the music
    /// linearly; `None`/missing/zero-duration outro completes immediately.
    fn start_outro(&mut self, surface: &mut impl ConversationSurface) {
        self.outro_started = true;
        let duration = match self.conversation.outro.as_ref() {
            Some(OutroData::Fade { fade_time, .. })
                if fade_time.is_finite() && *fade_time > 0.0 =>
            {
                Some(*fade_time)
            }
            _ => None,
        };
        let Some(duration) = duration else {
            self.finish(surface);
            return;
        };
        self.outro_duration = duration;
        self.outro_elapsed = 0.0;
        self.outro_alpha = 1.0;
        self.music_fade = Some(LinearFade {
            from: self.music_volume,
            to: 0.0,
            duration,
            elapsed: 0.0,
        });
    }

    fn advance_outro(&mut self, elapsed: f64, surface: &mut impl ConversationSurface) {
        if self.completed {
            return;
        }
        self.outro_elapsed += elapsed;
        let t = (self.outro_elapsed / self.outro_duration).clamp(0.0, 1.0);
        if t >= 1.0 {
            self.finish(surface);
        } else {
            self.outro_alpha = 1.0 - stepped_ease(8, t);
        }
    }

    fn finish(&mut self, surface: &mut impl ConversationSurface) {
        if self.completed {
            return;
        }
        self.completed = true;
        self.outro_alpha = 0.0;
        self.music_volume = 0.0;
        self.music_fade = None;
        self.typing = false;
        self.type_timer = 0.0;
        surface.on_completed();
    }

    fn advance_music(&mut self, elapsed: f64) {
        let Some(fade) = self.music_fade else {
            return;
        };
        let elapsed = fade.elapsed + elapsed;
        let t = (elapsed / fade.duration).clamp(0.0, 1.0);
        self.music_volume = fade.from + (fade.to - fade.from) * t;
        if t >= 1.0 {
            self.music_volume = fade.to;
            self.music_fade = None;
        } else {
            self.music_fade = Some(LinearFade { elapsed, ..fade });
        }
    }
}

/// `EaseUtil.stepped(steps)`: `floor(t * steps) / steps`.
fn stepped_ease(steps: u32, t: f64) -> f64 {
    let steps = f64::from(steps);
    (t * steps).floor() / steps
}

/// Native `correctAnimationName`: exact name, then strip one trailing
/// `-suffix` recursively, then `idle`, then fail.
pub(crate) fn correct_animation_name(
    requested: &str,
    has: impl Fn(&str) -> bool,
) -> Option<String> {
    let mut name = requested;
    loop {
        if has(name) {
            return Some(name.to_string());
        }
        if let Some((base, _suffix)) = name.rsplit_once('-') {
            name = base;
            continue;
        }
        if name != "idle" {
            name = "idle";
            continue;
        }
        return None;
    }
}

/// One glyph placed in the baked pages and on the text field.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlacedGlyph {
    pub(crate) ch: char,
    /// Character index in the source text (Unicode scalar values).
    pub(crate) char_index: usize,
    /// `[x, y, width, height]` in both the fill and shadow pages.
    pub(crate) page_rect: [u32; 4],
    /// Top-left position relative to the text field's top-left corner.
    pub(crate) screen: [f64; 2],
    /// Shadow-pass top-left position, or `None` for a transparent shadow.
    pub(crate) shadow: Option<[f64; 2]>,
}

/// A straight-alpha RGBA8 glyph page, row-major with a top-left origin.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GlyphPage {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

/// The baked fill/shadow pages plus their shared per-glyph placements.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GlyphRaster {
    pub(crate) fill: GlyphPage,
    pub(crate) shadow: Option<GlyphPage>,
    pub(crate) glyphs: Vec<PlacedGlyph>,
    pub(crate) line_height: f64,
}

struct BakedGlyph {
    ch: char,
    rect: [u32; 4],
    width: u32,
    height: u32,
    coverage: Vec<f32>,
}

/// Bakes the fill (and optional shadow) glyph page for `text` and returns the
/// per-glyph placements in text order.
///
/// The layout is deterministic: glyphs are packed in first-occurrence order,
/// advances are `h_advance + letter_spacing` with no kerning, and wrapping is
/// a greedy word-boundary wrap at `field_width` (newlines always break). The
/// shadow pass reuses the same page rects at `(+2, +2)`; a transparent shadow
/// color emits no shadow page.
pub(crate) fn raster_glyph_page(
    font_bytes: &[u8],
    size: f64,
    fill: [f32; 4],
    shadow: Option<[f32; 4]>,
    text: &str,
    field_width: f64,
    letter_spacing: f64,
) -> Result<GlyphRaster, RenderError> {
    if !size.is_finite() || size <= 0.0 {
        return Err(RenderError::Unsupported {
            detail: format!("glyph size {size} must be finite and positive"),
        });
    }
    if !letter_spacing.is_finite() {
        return Err(RenderError::Unsupported {
            detail: format!("letter spacing {letter_spacing} must be finite"),
        });
    }
    let font = FontRef::try_from_slice(font_bytes).map_err(|error| RenderError::Unsupported {
        detail: format!("font could not be parsed: {error}"),
    })?;
    let scaled = font.as_scaled(PxScale::from(size as f32));

    let chars: Vec<char> = text.chars().collect();
    let advances: Vec<f64> = chars
        .iter()
        .map(|&ch| f64::from(scaled.h_advance(scaled.glyph_id(ch))) + letter_spacing)
        .collect();
    let lines = wrap_ranges(&chars, &advances, field_width);

    let line_height = {
        let height = f64::from(scaled.ascent() - scaled.descent() + scaled.line_gap());
        if height.is_finite() && height > 0.0 {
            height
        } else {
            size
        }
    };
    let shadow = shadow.filter(|color| color[3].is_finite() && color[3] > 0.0);

    let mut baked: Vec<BakedGlyph> = Vec::new();
    let mut glyphs = Vec::new();
    let mut cursor_x = 0_u32;
    let mut cursor_y = 0_u32;
    let mut shelf_height = 0_u32;
    let mut used_width = 0_u32;
    let mut used_height = 0_u32;

    for (line_index, &(start, end)) in lines.iter().enumerate() {
        let mut pen = 0.0_f64;
        for index in start..end {
            let ch = chars[index];
            let advance = advances[index];
            if let Some(outlined) = font.outline_glyph(scaled.scaled_glyph(ch)) {
                let bounds = outlined.px_bounds();
                let width = bounds.width() as u32;
                let height = bounds.height() as u32;
                if width > 0 && height > 0 {
                    let rect = match baked.iter().find(|glyph| glyph.ch == ch) {
                        Some(glyph) => glyph.rect,
                        None => {
                            if cursor_x > 0 && cursor_x + width > GLYPH_PAGE_WIDTH {
                                cursor_y = cursor_y
                                    .saturating_add(shelf_height)
                                    .saturating_add(GLYPH_PADDING);
                                cursor_x = 0;
                                shelf_height = 0;
                            }
                            let rect = [cursor_x, cursor_y, width, height];
                            let mut coverage = vec![0.0_f32; (width * height) as usize];
                            outlined.draw(|x, y, coverage_value| {
                                if x < width
                                    && y < height
                                    && let Some(slot) = coverage.get_mut((y * width + x) as usize)
                                {
                                    *slot = coverage_value.clamp(0.0, 1.0);
                                }
                            });
                            baked.push(BakedGlyph {
                                ch,
                                rect,
                                width,
                                height,
                                coverage,
                            });
                            cursor_x = cursor_x.saturating_add(width).saturating_add(GLYPH_PADDING);
                            shelf_height = shelf_height.max(height);
                            used_width = used_width.max(rect[0] + width);
                            used_height = used_height.max(rect[1] + height);
                            rect
                        }
                    };
                    let screen = [
                        pen + f64::from(bounds.min.x),
                        f64::from(scaled.ascent())
                            + line_index as f64 * line_height
                            + f64::from(bounds.min.y),
                    ];
                    glyphs.push(PlacedGlyph {
                        ch,
                        char_index: index,
                        page_rect: rect,
                        screen,
                        shadow: shadow
                            .map(|_| [screen[0] + SHADOW_OFFSET, screen[1] + SHADOW_OFFSET]),
                    });
                }
            }
            pen += advance;
        }
    }

    let page_width = used_width.max(1);
    let page_height = used_height.max(1);
    let mut fill_page = GlyphPage {
        width: page_width,
        height: page_height,
        rgba: vec![0; (page_width * page_height * 4) as usize],
    };
    for glyph in &baked {
        paint_glyph(&mut fill_page, glyph, fill);
    }
    let shadow_page = shadow.map(|color| {
        let mut page = GlyphPage {
            width: page_width,
            height: page_height,
            rgba: vec![0; (page_width * page_height * 4) as usize],
        };
        for glyph in &baked {
            paint_glyph(&mut page, glyph, color);
        }
        page
    });

    Ok(GlyphRaster {
        fill: fill_page,
        shadow: shadow_page,
        glyphs,
        line_height,
    })
}

fn paint_glyph(page: &mut GlyphPage, glyph: &BakedGlyph, color: [f32; 4]) {
    let red = color_channel(color[0]);
    let green = color_channel(color[1]);
    let blue = color_channel(color[2]);
    let alpha = color[3].clamp(0.0, 1.0);
    for (index, &coverage) in glyph.coverage.iter().enumerate() {
        if coverage <= 0.0 {
            continue;
        }
        let x = glyph.rect[0] + (index as u32 % glyph.width);
        let y = glyph.rect[1] + (index as u32 / glyph.width);
        if x >= page.width || y >= page.height {
            continue;
        }
        let offset = ((y * page.width + x) * 4) as usize;
        if let Some(pixel) = page.rgba.get_mut(offset..offset + 4) {
            pixel[0] = red;
            pixel[1] = green;
            pixel[2] = blue;
            pixel[3] = color_channel(coverage * alpha);
        }
    }
}

fn color_channel(value: f32) -> u8 {
    if value.is_finite() {
        (value.clamp(0.0, 1.0) * 255.0).round() as u8
    } else {
        0
    }
}

/// Greedy word-boundary wrap over measured character advances.
///
/// Returns half-open `(start, end)` char ranges. A space chosen as the break
/// point is excluded from both lines (native `insertBreakLines` turns it into
/// a line break). A word wider than the field is split at the field boundary.
fn wrap_ranges(chars: &[char], advances: &[f64], field_width: f64) -> Vec<(usize, usize)> {
    let wrap = field_width.is_finite() && field_width > 0.0;
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut width = 0.0;
    let mut last_space: Option<usize> = None;
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\n' {
            ranges.push((start, index));
            index += 1;
            start = index;
            width = 0.0;
            last_space = None;
            continue;
        }
        let advance = advances[index];
        if wrap && width + advance > field_width && index > start {
            if let Some(space) = last_space {
                ranges.push((start, space));
                index = space + 1;
            } else {
                ranges.push((start, index));
            }
            start = index;
            width = 0.0;
            last_space = None;
            continue;
        }
        if chars[index] == ' ' {
            last_space = Some(index);
        }
        width += advance;
        index += 1;
    }
    ranges.push((start, chars.len()));
    ranges
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use oxide_data::dialogue::{BackdropData, DialogueTyped, MusicData, parse_dialogue_any};

    use super::*;

    fn entry(
        speaker: &str,
        speaker_animation: &str,
        box_id: &str,
        box_animation: &str,
        text: &[&str],
    ) -> DialogueEntryData {
        DialogueEntryData {
            speaker: speaker.to_string(),
            speaker_animation: speaker_animation.to_string(),
            box_id: box_id.to_string(),
            box_animation: box_animation.to_string(),
            text: text.iter().map(|line| (*line).to_string()).collect(),
            speed: None,
            extra: BTreeMap::new(),
        }
    }

    fn conversation(entries: Vec<DialogueEntryData>, outro: Option<OutroData>) -> ConversationData {
        ConversationData {
            version: "1.0.0".to_string(),
            backdrop: BackdropData::Solid {
                fade_time: 0.0,
                color: "#000000".to_string(),
                extra: BTreeMap::new(),
            },
            music: None,
            outro,
            dialogue: entries,
            extra: BTreeMap::new(),
        }
    }

    fn walkthrough_conversation() -> ConversationData {
        conversation(
            vec![
                entry("bf-pixel", "talkEnter", "roses", "enter", &["hello", "two"]),
                entry("bf-pixel", "talk", "roses", "speaking", &["one", "two"]),
                entry("senpai", "talkEnter", "default", "enter", &["bye"]),
            ],
            Some(OutroData::Fade {
                fade_time: 1.0,
                extra: BTreeMap::new(),
            }),
        )
    }

    #[derive(Default)]
    struct RecordingSurface {
        animation: String,
        finished: bool,
        events: Vec<String>,
    }

    impl RecordingSurface {
        fn event_count(&self, prefix: &str) -> usize {
            self.events
                .iter()
                .filter(|event| event.starts_with(prefix))
                .count()
        }
    }

    impl ConversationSurface for RecordingSurface {
        fn box_animation(&self) -> &str {
            &self.animation
        }

        fn box_animation_finished(&self) -> bool {
            self.finished
        }

        fn on_entry_open(&mut self, entry: &DialogueEntryData, box_changed: bool) {
            self.events.push(format!(
                "entry:{}:{}:{box_changed}",
                entry.box_id, entry.box_animation
            ));
        }

        fn on_speaker_show(&mut self, entry: &DialogueEntryData, speaker_changed: bool) {
            self.events.push(format!(
                "speaker:{}:{}:{speaker_changed}",
                entry.speaker, entry.speaker_animation
            ));
        }

        fn on_line_set(&mut self, text: &str, typed_len: usize) {
            self.events.push(format!("set:{text}:{typed_len}"));
        }

        fn on_line_append(&mut self, text: &str, typed_len: usize) {
            self.events.push(format!("append:{text}:{typed_len}"));
        }

        fn on_typing_complete(&mut self) {
            self.events.push("typing-complete".to_string());
        }

        fn on_dialogue_end(&mut self) {
            self.events.push("dialogue-end".to_string());
        }

        fn on_completed(&mut self) {
            self.events.push("completed".to_string());
        }
    }

    #[test]
    fn state_machine_walkthrough_set_append_entry_exhaustion_and_outro() {
        let mut state = ConversationState::new(walkthrough_conversation());
        let mut surface = RecordingSurface::default();

        assert_eq!(state.phase(), ConversationPhase::Start);
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Opening);
        assert_eq!(state.entry_index(), 0);
        assert_eq!(last_event(&surface), "entry:roses:enter:true");

        // The box animation has not started yet (`""` differs from `enter`),
        // so the native Opening check completes on the first frame.
        state.begin_tick(0.0, &mut surface);
        assert_eq!(state.phase(), ConversationPhase::Speaking);
        assert_eq!(state.line_text(), "hello");
        assert_eq!(state.typed_len(), 0);
        assert!(
            surface
                .events
                .iter()
                .any(|e| e == "speaker:bf-pixel:talkEnter:true")
        );
        assert!(surface.events.iter().any(|e| e == "set:hello:0"));

        // Speaking advance skips the remaining typing and reaches Idle.
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Idle);
        assert_eq!(state.typed_len(), 5);
        assert_eq!(surface.event_count("typing-complete"), 1);

        // More lines in the SAME entry append instead of setText.
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Speaking);
        assert_eq!(state.line_index(), 1);
        assert_eq!(state.line_text(), "two");
        assert!(surface.events.iter().any(|e| e == "append:two:0"));
        assert!(!surface.events.iter().any(|e| e == "set:two:0"));

        // Type the appended line to completion, then move to entry 1.
        state.begin_tick(150.0, &mut surface);
        state.begin_tick(200.0, &mut surface);
        assert_eq!(state.phase(), ConversationPhase::Idle);
        assert_eq!(state.typed_len(), 3);

        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Opening);
        assert_eq!(state.entry_index(), 1);
        assert_eq!(state.line_index(), 0);
        assert_eq!(last_event(&surface), "entry:roses:speaking:false");

        // Same box id: the Opening check still waits while the current box
        // animation equals the entry's requested `speaking`.
        surface.animation = "speaking".to_string();
        surface.finished = false;
        state.begin_tick(250.0, &mut surface);
        assert_eq!(state.phase(), ConversationPhase::Opening);

        // A finished box animation completes Opening; the speaker id is
        // unchanged, so the swap is skipped but the animation still plays.
        surface.finished = true;
        state.begin_tick(300.0, &mut surface);
        assert_eq!(state.phase(), ConversationPhase::Speaking);
        assert_eq!(state.line_text(), "one");
        assert!(
            surface
                .events
                .iter()
                .any(|e| e == "speaker:bf-pixel:talk:false")
        );

        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Idle);
        state.advance(&mut surface);
        assert_eq!(state.line_index(), 1);
        assert!(surface.events.iter().any(|e| e == "append:two:0"));
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Idle);

        // Entry exhaustion opens the next entry; a new box id swaps.
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Opening);
        assert_eq!(state.entry_index(), 2);
        assert_eq!(last_event(&surface), "entry:default:enter:true");

        surface.animation.clear();
        surface.finished = false;
        state.begin_tick(350.0, &mut surface);
        assert_eq!(state.phase(), ConversationPhase::Speaking);
        assert_eq!(state.line_text(), "bye");
        assert!(
            surface
                .events
                .iter()
                .any(|e| e == "speaker:senpai:talkEnter:true")
        );

        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Idle);
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Ending);
        assert_eq!(surface.event_count("dialogue-end"), 1);

        // Ending starts the outro on the first frame, then steps the fade.
        state.begin_tick(400.0, &mut surface);
        assert_eq!(state.outro_alpha(), 1.0);
        assert!(!state.completed());

        state.begin_tick(525.0, &mut surface);
        assert!((state.outro_alpha() - 0.875).abs() < 1e-12);
        state.begin_tick(650.0, &mut surface);
        assert!((state.outro_alpha() - 0.75).abs() < 1e-12);
        state.begin_tick(900.0, &mut surface);
        assert!((state.outro_alpha() - 0.5).abs() < 1e-12);
        state.begin_tick(1275.0, &mut surface);
        assert!((state.outro_alpha() - 0.125).abs() < 1e-12);
        state.begin_tick(1400.0, &mut surface);
        assert!(state.completed());
        assert_eq!(state.outro_alpha(), 0.0);
        assert_eq!(surface.event_count("completed"), 1);
    }

    #[test]
    fn typewriter_batches_by_truncated_timer_and_keeps_the_remainder() {
        let mut state = ConversationState::new(conversation(
            vec![entry("bf-pixel", "talk", "roses", "enter", &["abcdef"])],
            None,
        ));
        let mut surface = RecordingSurface::default();
        state.advance(&mut surface);
        state.begin_tick(0.0, &mut surface);
        assert_eq!(state.phase(), ConversationPhase::Speaking);

        // 0.12 s / 0.05 s = 2.4 -> two chars, 0.02 s remainder.
        state.begin_tick(120.0, &mut surface);
        assert_eq!(state.typed_len(), 2);
        assert!((state.type_timer - 0.02).abs() < 1e-12);

        // 0.04 s more crosses one delay -> one more char, 0.01 s remainder
        // (0.12 % 0.05 is not exactly 0.02 in binary floating point).
        state.begin_tick(160.0, &mut surface);
        assert_eq!(state.typed_len(), 3);
        assert!((state.type_timer - 0.01).abs() < 1e-12);

        // A large frame batch clamps to the line length and completes once.
        state.begin_tick(500.0, &mut surface);
        assert_eq!(state.typed_len(), 6);
        assert_eq!(state.phase(), ConversationPhase::Idle);
        assert_eq!(surface.event_count("typing-complete"), 1);
        assert!(state.type_timer.abs() < 1e-12);
    }

    #[test]
    fn advance_and_skip_follow_the_native_phase_table() {
        // Advance during Opening moves to Idle without a setText (the line is
        // skipped, matching the frozen Opening/Speaking skip rule).
        let mut state = ConversationState::new(conversation(
            vec![
                entry("bf-pixel", "talk", "roses", "enter", &["one", "two"]),
                entry("bf-pixel", "talk", "roses", "enter", &["three"]),
            ],
            Some(OutroData::None {
                extra: BTreeMap::new(),
            }),
        ));
        let mut surface = RecordingSurface::default();
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Opening);
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Idle);
        assert_eq!(surface.event_count("set:"), 0);
        assert_eq!(surface.event_count("typing-complete"), 0);
        state.advance(&mut surface);
        assert_eq!(state.phase(), ConversationPhase::Speaking);
        assert_eq!(state.line_index(), 1);
        assert_eq!(state.line_text(), "two");

        // skipConversation from any live phase reaches Ending and fires the
        // dialogue-end hook exactly once.
        let mut skipped = ConversationState::new(conversation(
            vec![entry("bf-pixel", "talk", "roses", "enter", &["one"])],
            Some(OutroData::None {
                extra: BTreeMap::new(),
            }),
        ));
        let mut skipped_surface = RecordingSurface::default();
        skipped.advance(&mut skipped_surface);
        skipped.skip(&mut skipped_surface);
        assert_eq!(skipped.phase(), ConversationPhase::Ending);
        assert_eq!(skipped_surface.event_count("dialogue-end"), 1);
        skipped.skip(&mut skipped_surface);
        assert_eq!(skipped_surface.event_count("dialogue-end"), 1);

        // Ending advance completes immediately; `OutroData::None` would also
        // complete on the next begin_tick.
        skipped.advance(&mut skipped_surface);
        assert!(skipped.completed());
        assert_eq!(skipped_surface.event_count("completed"), 1);

        // A missing/None outro completes on the first Ending frame.
        let mut none_outro = ConversationState::new(conversation(
            vec![entry("bf-pixel", "talk", "roses", "enter", &["one"])],
            None,
        ));
        let mut none_surface = RecordingSurface::default();
        none_outro.advance(&mut none_surface);
        none_outro.begin_tick(0.0, &mut none_surface);
        none_outro.advance(&mut none_surface); // Speaking -> Idle
        none_outro.advance(&mut none_surface); // Idle -> Ending
        none_outro.begin_tick(1.0, &mut none_surface);
        assert!(none_outro.completed());
        assert_eq!(none_surface.event_count("completed"), 1);
    }

    #[test]
    fn outro_steps_eight_and_fades_music_linearly() {
        let mut data = conversation(
            vec![entry("bf-pixel", "talk", "roses", "enter", &["one"])],
            Some(OutroData::Fade {
                fade_time: 1.0,
                extra: BTreeMap::new(),
            }),
        );
        data.music = Some(MusicData {
            asset: "Lunchbox".to_string(),
            fade_time: 2.0,
            looped: true,
            extra: BTreeMap::new(),
        });
        let mut state = ConversationState::new(data);
        let mut surface = RecordingSurface::default();
        state.advance(&mut surface);
        state.begin_tick(0.0, &mut surface);
        assert!((state.music_volume()).abs() < 1e-12);

        // One second of the two-second linear fade-in.
        state.begin_tick(1000.0, &mut surface);
        assert!((state.music_volume() - 0.5).abs() < 1e-12);
        assert_eq!(state.phase(), ConversationPhase::Idle);

        state.advance(&mut surface); // Idle -> Ending
        // The music fade-in is at 1.5/2.0 = 0.75 when the outro starts; the
        // outro fades that current value linearly to zero.
        state.begin_tick(1500.0, &mut surface); // start outro (no step yet)
        assert_eq!(state.outro_alpha(), 1.0);
        assert!((state.music_volume() - 0.75).abs() < 1e-12);

        state.begin_tick(1750.0, &mut surface);
        assert!((state.outro_alpha() - 0.75).abs() < 1e-12);
        assert!((state.music_volume() - 0.5625).abs() < 1e-12);

        state.begin_tick(2000.0, &mut surface);
        assert!((state.outro_alpha() - 0.5).abs() < 1e-12);
        assert!((state.music_volume() - 0.375).abs() < 1e-12);

        state.begin_tick(2375.0, &mut surface);
        assert!((state.outro_alpha() - 0.125).abs() < 1e-12);
        assert!((state.music_volume() - 0.09375).abs() < 1e-12);

        state.begin_tick(2500.0, &mut surface);
        assert!(state.completed());
        assert_eq!(state.outro_alpha(), 0.0);
        assert_eq!(state.music_volume(), 0.0);
    }

    #[test]
    fn same_id_box_and_speaker_swaps_are_recorded_as_skipped() {
        let mut state = ConversationState::new(conversation(
            vec![
                entry("bf-pixel", "talkEnter", "roses", "enter", &["a"]),
                entry("bf-pixel", "talk", "roses", "speaking", &["b"]),
                entry("senpai", "talkEnter", "default", "enter", &["c"]),
            ],
            None,
        ));
        let mut surface = RecordingSurface::default();
        state.advance(&mut surface);
        state.begin_tick(0.0, &mut surface);
        state.advance(&mut surface); // Speaking -> Idle
        state.advance(&mut surface); // Idle -> entry 1 (Opening)
        assert_eq!(last_event(&surface), "entry:roses:speaking:false");

        surface.animation = "speaking".to_string();
        surface.finished = true;
        state.begin_tick(1.0, &mut surface);
        assert!(
            surface
                .events
                .iter()
                .any(|e| e == "speaker:bf-pixel:talk:false")
        );
        state.advance(&mut surface); // Idle
        state.advance(&mut surface); // entry 2 (Opening)
        assert_eq!(last_event(&surface), "entry:default:enter:true");
    }

    #[test]
    fn animation_names_strip_suffixes_then_fall_back_to_idle() {
        let available: BTreeSet<&str> = ["talkEnter", "talk", "idle"].into_iter().collect();
        let has = |name: &str| available.contains(name);
        assert_eq!(
            correct_animation_name("talkEnter", has),
            Some("talkEnter".to_string())
        );
        assert_eq!(
            correct_animation_name("talkEnter-angry", has),
            Some("talkEnter".to_string())
        );
        assert_eq!(
            correct_animation_name("dance-alt", has),
            Some("idle".to_string())
        );
        assert_eq!(
            correct_animation_name("idle", has),
            Some("idle".to_string())
        );

        let no_idle: BTreeSet<&str> = ["talkEnter"].into_iter().collect();
        let has_no_idle = |name: &str| no_idle.contains(name);
        assert_eq!(correct_animation_name("missing", has_no_idle), None);
    }

    #[test]
    fn wrap_ranges_break_at_word_boundaries_and_newlines() {
        let chars: Vec<char> = "aaa bbb ccc".chars().collect();
        let advances: Vec<f64> = chars
            .iter()
            .map(|&ch| if ch == ' ' { 5.0 } else { 10.0 })
            .collect();
        // "aaa bbb" is 65 px; adding the second space and "ccc" would be 100.
        assert_eq!(wrap_ranges(&chars, &advances, 75.0), vec![(0, 7), (8, 11)]);
        assert_eq!(wrap_ranges(&chars, &advances, 1000.0), vec![(0, 11)]);

        let lines: Vec<char> = "aaa\nbb".chars().collect();
        let line_advances: Vec<f64> = lines
            .iter()
            .map(|&ch| if ch == ' ' { 5.0 } else { 10.0 })
            .collect();
        assert_eq!(
            wrap_ranges(&lines, &line_advances, 1000.0),
            vec![(0, 3), (4, 6)]
        );
    }

    #[test]
    fn senpai_shipped_entries_are_pinned() {
        // Shipped `data/dialogue/conversations/senpai.json` values
        // (uncensored family; whitespace compacted). Kept hermetic so the
        // data pin runs without the reference asset tree.
        let json = r##"{
  "version": "1.0.0",
  "backdrop": {"type": "solid", "fadeTime": 2.0, "color": "#BFB3DFD8"},
  "music": {"asset": "Lunchbox", "fadeTime": 2.0, "looped": true},
  "outro": {"type": "fade", "fadeTime": 1.0},
  "dialogue": [
    {
      "speaker": "senpai",
      "speakerAnimation": "talkEnter",
      "box": "roses",
      "boxAnimation": "enter",
      "text": ["Ah, a new fair maiden has come in search of true love!"]
    },
    {
      "speaker": "senpai",
      "speakerAnimation": "talk",
      "box": "roses",
      "boxAnimation": "speaking",
      "text": ["A serenade between gentlemen shall decide where her beautiful heart shall reside."]
    },
    {
      "speaker": "bf-pixel",
      "speakerAnimation": "talkEnter",
      "box": "roses",
      "boxAnimation": "speaking",
      "text": ["Beep bo bop"]
    }
  ]
}"##;
        let document = parse_dialogue_any(json.as_bytes()).expect("senpai parses");
        let DialogueTyped::Conversation(data) = document.typed else {
            panic!("senpai must parse as a conversation");
        };
        assert_eq!(data.dialogue.len(), 3);
        let entries: Vec<(&str, &str, &str, &str, usize)> = data
            .dialogue
            .iter()
            .map(|entry| {
                (
                    entry.speaker.as_str(),
                    entry.speaker_animation.as_str(),
                    entry.box_id.as_str(),
                    entry.box_animation.as_str(),
                    entry.text.len(),
                )
            })
            .collect();
        assert_eq!(
            entries,
            vec![
                ("senpai", "talkEnter", "roses", "enter", 1),
                ("senpai", "talk", "roses", "speaking", 1),
                ("bf-pixel", "talkEnter", "roses", "speaking", 1),
            ]
        );
    }

    #[test]
    fn raster_rejects_invalid_font_bytes_and_bad_size() {
        let error = raster_glyph_page(b"not a font", 32.0, [1.0; 4], None, "hello", 100.0, -1.0)
            .expect_err("invalid font bytes must fail");
        assert!(matches!(error, RenderError::Unsupported { .. }));

        let error = raster_glyph_page(b"not a font", 0.0, [1.0; 4], None, "hello", 100.0, -1.0)
            .expect_err("zero size must fail before parsing");
        assert!(matches!(error, RenderError::Unsupported { .. }));
    }

    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for 6E glyph-raster determinism"]
    fn pixel_font_raster_is_deterministic() {
        let root = std::env::var_os("OXIDE_ASSETS_ROOT")
            .expect("set OXIDE_ASSETS_ROOT to the reference assets directory");
        let vfs = oxide_vfs::Vfs::new(root.into());
        let bytes = crate::render::load::read_asset(&vfs, "fonts/pixel.otf")
            .expect("fonts/pixel.otf resolves through the VFS");
        let text = "Ah, a new fair maiden has come in search of true love!";
        let fill = [0.247, 0.125, 0.129, 1.0];
        let shadow = [0.847, 0.58, 0.58, 1.0];
        let first = raster_glyph_page(&bytes, 32.0, fill, Some(shadow), text, 800.0, -1.0)
            .expect("pixel.otf rasterizes");
        let second = raster_glyph_page(&bytes, 32.0, fill, Some(shadow), text, 800.0, -1.0)
            .expect("pixel.otf rasterizes");
        assert_eq!(first, second, "same input must bake the same page");
        assert!(!first.glyphs.is_empty());
        assert_eq!(
            first.fill.rgba.len(),
            (first.fill.width * first.fill.height * 4) as usize
        );
        assert!(first.shadow.is_some());
        let glyph = first
            .glyphs
            .iter()
            .find(|glyph| glyph.ch == 'A')
            .expect("A");
        assert_eq!(
            glyph.shadow,
            Some([
                glyph.screen[0] + SHADOW_OFFSET,
                glyph.screen[1] + SHADOW_OFFSET
            ])
        );

        // A narrow field must wrap onto more than one baseline.
        let wrapped = raster_glyph_page(
            &bytes,
            32.0,
            [1.0; 4],
            None,
            "word word word word word word",
            100.0,
            -1.0,
        )
        .expect("pixel.otf rasterizes");
        let min_y = wrapped
            .glyphs
            .iter()
            .map(|glyph| glyph.screen[1])
            .fold(f64::INFINITY, f64::min);
        let max_y = wrapped
            .glyphs
            .iter()
            .map(|glyph| glyph.screen[1])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(max_y > min_y + 10.0, "narrow field must wrap");
        assert!(wrapped.shadow.is_none());
    }

    fn last_event(surface: &RecordingSurface) -> &str {
        surface.events.last().map_or("<none>", String::as_str)
    }
}
