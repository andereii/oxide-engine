//! Sparrow atlas font metrics, replicating Haxe `funkin.ui.AtlasText`.
//!
//! Glyph frames are named `<prefix><4 digits>` (`A0000`, `-dash-0000`,
//! `&0000`). The glyph map is keyed by that prefix and stores the metrics of
//! the frame with the lowest numeric suffix (flixel `FlxFrame.sortFrames`
//! ordering); unsuffixed names fall back to first-seen-wins.

use std::collections::BTreeMap;

use oxide_atlas::Atlas;

use crate::{Case, FontError, INLINE_URI};

/// Metrics of one glyph, in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparrowGlyph {
    pub width: u32,
    pub height: u32,
}

/// A Sparrow atlas interpreted as a bitmap font.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparrowFont {
    pub atlas: Atlas,
    pub glyphs: BTreeMap<String, SparrowGlyph>,
    pub case_allowed: Case,
    pub max_height: u32,
}

impl SparrowFont {
    /// Derives font metrics from an already-parsed Sparrow atlas.
    ///
    /// Every frame contributes to `max_height` and the case flags. A glyph
    /// with 4-digit-suffixed frames takes the metrics of the lowest numeric
    /// suffix; a name without that suffix keeps the first-seen-wins fallback.
    pub fn from_atlas(atlas: Atlas) -> Result<SparrowFont, FontError> {
        let mut glyphs = BTreeMap::new();
        let mut lowest_suffix: BTreeMap<String, u32> = BTreeMap::new();
        let mut max_height = 0u32;
        let mut contains_upper = false;
        let mut contains_lower = false;

        for frame in &atlas.subtextures {
            let width = u32::try_from(frame.width)
                .map_err(|_| malformed_frame(&frame.name, "width", frame.width))?;
            let height = u32::try_from(frame.height)
                .map_err(|_| malformed_frame(&frame.name, "height", frame.height))?;

            max_height = max_height.max(height);
            contains_upper |= is_upper_frame_name(&frame.name);
            contains_lower |= is_lower_frame_name(&frame.name);

            let prefix = frame_prefix(&frame.name);
            let metrics = SparrowGlyph { width, height };
            match four_digit_suffix(&frame.name) {
                Some(suffix) => {
                    if lowest_suffix
                        .get(&prefix)
                        .is_none_or(|lowest| suffix < *lowest)
                    {
                        lowest_suffix.insert(prefix.clone(), suffix);
                        glyphs.insert(prefix, metrics);
                    }
                }
                None => {
                    glyphs.entry(prefix).or_insert(metrics);
                }
            }
        }

        let case_allowed = if contains_upper == contains_lower {
            Case::Both
        } else if contains_upper {
            Case::Upper
        } else {
            Case::Lower
        };

        Ok(SparrowFont {
            atlas,
            glyphs,
            case_allowed,
            max_height,
        })
    }

    /// Glyph frame-name prefix for a character: the verbatim Haxe
    /// `AtlasText.getAnimPrefix` table, including its quirks.
    pub fn glyph_prefix(&self, ch: char) -> String {
        let prefix = match ch {
            '&' => "-andpersand-",
            '\u{1F620}' => "-angry faic-",
            '\'' => "-apostraphie-",
            '\\' => "-back slash-",
            ',' => "-comma-",
            '-' => "-dash-",
            '\u{2193}' => "-down arrow-",
            '\u{201D}' => "-end quote-",
            '!' => "-exclamation point-",
            '/' => "-forward slash-",
            '>' => "-greater than-",
            '\u{2665}' | '\u{2661}' => "-heart-",
            '\u{2190}' => "-left arrow-",
            '<' => "-less than-",
            '*' => "-multiply x-",
            '.' => "-period-",
            '?' => "-question mark-",
            '\u{2192}' => "-right arrow-",
            '\u{201C}' => "-start quote-",
            '\u{2191}' => "-up arrow-",
            other => return other.to_string(),
        };
        prefix.to_string()
    }

    /// Measures `text` with the AtlasText rules: `' '` advances 40, `'\n'`
    /// starts a new line at horizontal position 0 and advances one
    /// `max_height` row, a glyph advances by its first-frame width, and an
    /// unknown glyph advances nothing. The width is the widest line's width;
    /// the height is `lines * max_height`.
    pub fn measure(&self, text: &str) -> (u32, u32) {
        if text.is_empty() {
            return (0, 0);
        }

        let mut line: u64 = 0;
        let mut max_width: u64 = 0;
        for ch in text.chars() {
            match ch {
                ' ' => line += 40,
                '\n' => {
                    max_width = max_width.max(line);
                    line = 0;
                }
                _ => {
                    if let Some(glyph) = self.glyphs.get(&self.glyph_prefix(ch)) {
                        line += u64::from(glyph.width);
                    }
                }
            }
        }
        max_width = max_width.max(line);

        let lines = text.split('\n').count() as u64;
        let width = max_width.min(u64::from(u32::MAX)) as u32;
        let height = (u64::from(self.max_height) * lines).min(u64::from(u32::MAX)) as u32;
        (width, height)
    }
}

/// The glyph prefix of a frame name: the name without its trailing four
/// digits (`A0000` -> `A`, `-dash-0000` -> `-dash-`, `00000` -> `0`).
fn frame_prefix(name: &str) -> String {
    match four_digit_suffix(name) {
        Some(_) => name[..name.len() - 4].to_string(),
        None => name.to_string(),
    }
}

/// The numeric value of a frame name's trailing four digits, when present.
fn four_digit_suffix(name: &str) -> Option<u32> {
    let bytes = name.as_bytes();
    if bytes.len() < 4 || !bytes[bytes.len() - 4..].iter().all(u8::is_ascii_digit) {
        return None;
    }
    name[name.len() - 4..].parse().ok()
}

/// Haxe `AtlasText.AtlasFontData.upperChar`: `^[A-Z][0-9]+$`.
fn is_upper_frame_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_uppercase() && bytes[1..].iter().all(u8::is_ascii_digit)
}

/// Haxe `AtlasText.AtlasFontData.lowerChar`: `^[a-z][0-9]+$`.
fn is_lower_frame_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_lowercase() && bytes[1..].iter().all(u8::is_ascii_digit)
}

fn malformed_frame(name: &str, attribute: &str, value: i32) -> FontError {
    FontError::Malformed {
        uri: INLINE_URI.to_string(),
        message: format!("frame {name:?} has negative {attribute} ({value})"),
    }
}
