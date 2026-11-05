//! Bitmap font metrics for the two formats used by the game.
//!
//! - [`SparrowFont`]: a Sparrow `TextureAtlas` interpreted as a font, exactly
//!   like Haxe `funkin.ui.AtlasText` (space advances 40, a newline advances one
//!   `maxHeight` row, a glyph advances by the width of its first frame).
//! - [`AngelFont`]: an angel-code (BMFont) XML font as loaded by
//!   `FlxBitmapFont.fromAngelCode`.
//!
//! The crate is headless and performs no I/O: every entry point takes the file
//! contents as a string. Numeric advances stay in stored pixels; nothing is
//! scaled or rounded here.

mod angel;
mod sparrow;

pub use angel::{AngelChar, AngelCommon, AngelFont, AngelInfo, AngelPage};
pub use sparrow::{SparrowFont, SparrowGlyph};

use thiserror::Error;

/// Placeholder URI used by the in-memory parser entry points.
pub(crate) const INLINE_URI: &str = "<inline>";

/// Errors returned by the font parsers and validators.
#[derive(Debug, Error)]
pub enum FontError {
    #[error("font parse error in {uri}: {message}")]
    Malformed { uri: String, message: String },
    #[error("glyph {glyph:?} rect (x={x}, y={y}, w={w}, h={h}) overflows page {sw}x{sh}")]
    GlyphOverflowsPage {
        glyph: String,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        sw: i32,
        sh: i32,
    },
}

impl From<oxide_atlas::AtlasError> for FontError {
    fn from(err: oxide_atlas::AtlasError) -> Self {
        let uri = match &err {
            oxide_atlas::AtlasError::Malformed { uri, .. }
            | oxide_atlas::AtlasError::NotAnAtlas { uri, .. }
            | oxide_atlas::AtlasError::Json { uri, .. } => uri.clone(),
        };
        FontError::Malformed {
            uri,
            message: err.to_string(),
        }
    }
}

/// Case restrictions declared by the glyph frames of a Sparrow font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Case {
    Both,
    Upper,
    Lower,
}

/// Builds a [`FontError::Malformed`] for the in-memory parsers.
pub(crate) fn malformed(message: impl Into<String>) -> FontError {
    FontError::Malformed {
        uri: INLINE_URI.to_string(),
        message: message.into(),
    }
}
