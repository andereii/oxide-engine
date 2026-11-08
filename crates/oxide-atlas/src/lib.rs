//! Parsers for the atlas formats used by the engine: Sparrow XML,
//! TexturePacker JSON (hash and array) and Animate Atlas JSON.
//!
//! The crate is headless and performs no I/O. Every parser takes the file
//! contents as a string; the caller strips a leading UTF-8 BOM (the parsers
//! also tolerate one for convenience).
//!
//! The XML scanner in [`xml`] is deliberately lenient: shipped Sparrow data is
//! not well-formed XML by strict standards (raw `<` / `&` inside attribute
//! values, declarations preceded by blank lines).

pub mod animate;
pub mod animate_stage;
pub mod packer;
pub mod sparrow;
pub mod xml;

pub use animate::{AnimateAtlas, AnimateSprite, parse_animate};
pub use animate_stage::{AnimateStage, AnimateStageError, SpriteQuad};
pub use packer::{PackerAtlas, PackerFrame, parse_packer};
pub use sparrow::{Atlas, FrameQuad, SubTexture, frame_quad, parse_sparrow};

use thiserror::Error;

/// Placeholder URI used by the in-memory `&str` parser entry points.
pub(crate) const INLINE_URI: &str = "<inline>";

/// Errors returned by the atlas parsers.
#[derive(Debug, Error)]
pub enum AtlasError {
    #[error("malformed atlas at {uri}: {message}")]
    Malformed { uri: String, message: String },
    #[error("not a sparrow atlas (root element <{root_tag}>): {uri}")]
    NotAnAtlas { uri: String, root_tag: String },
    #[error("bad atlas json at {uri}: {message}")]
    Json { uri: String, message: String },
}

impl AtlasError {
    pub(crate) fn malformed(message: impl Into<String>) -> Self {
        Self::Malformed {
            uri: INLINE_URI.to_string(),
            message: message.into(),
        }
    }

    pub(crate) fn json(message: impl Into<String>) -> Self {
        Self::Json {
            uri: INLINE_URI.to_string(),
            message: message.into(),
        }
    }
}

impl From<xml::XmlError> for AtlasError {
    fn from(err: xml::XmlError) -> Self {
        Self::malformed(err.message)
    }
}
