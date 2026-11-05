//! Sparrow `TextureAtlas` XML parser.

use crate::xml::Scanner;
use crate::{AtlasError, INLINE_URI};

/// One page rectangle and its optional trimmed-frame geometry, in pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubTexture {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub frame_x: i32,
    pub frame_y: i32,
    pub frame_width: i32,
    pub frame_height: i32,
    pub rotated: bool,
}

/// A parsed Sparrow atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Atlas {
    pub image_path: String,
    pub subtextures: Vec<SubTexture>,
}

/// Trimming geometry for one subtexture. Pure data; the renderer applies it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameQuad {
    /// Page rect in sprite orientation: for `rotated=true` the parsed
    /// width/height are swapped (the renderer transposes the page rect).
    pub atlas_rect: [i32; 4],
    /// `(-frameX, -frameY)`.
    pub dest_offset: [i32; 2],
    /// `(frameWidth or width, frameHeight or height)`; for the never-in-data
    /// corner case `rotated && frameWidth == 0 && frameHeight == 0` flixel
    /// swaps the page dims: `(height, width)`.
    pub dest_size: [i32; 2],
}

/// Computes flixel's `FlxAtlasFrames.fromSparrow` trimming geometry for one
/// subtexture. Pure arithmetic; no I/O and no rendering.
pub fn frame_quad(subtexture: &SubTexture) -> FrameQuad {
    let atlas_rect = if subtexture.rotated {
        [
            subtexture.x,
            subtexture.y,
            subtexture.height,
            subtexture.width,
        ]
    } else {
        [
            subtexture.x,
            subtexture.y,
            subtexture.width,
            subtexture.height,
        ]
    };

    let dest_offset = [-subtexture.frame_x, -subtexture.frame_y];

    let dest_size =
        if subtexture.rotated && subtexture.frame_width == 0 && subtexture.frame_height == 0 {
            [subtexture.height, subtexture.width]
        } else {
            [
                if subtexture.frame_width != 0 {
                    subtexture.frame_width
                } else {
                    subtexture.width
                },
                if subtexture.frame_height != 0 {
                    subtexture.frame_height
                } else {
                    subtexture.height
                },
            ]
        };

    FrameQuad {
        atlas_rect,
        dest_offset,
        dest_size,
    }
}

/// Parses a Sparrow `TextureAtlas` XML document.
///
/// Missing geometry attributes default to `0`; a present non-integer geometry
/// attribute is an error. A root element other than `TextureAtlas` yields
/// [`AtlasError::NotAnAtlas`].
pub fn parse_sparrow(xml: &str) -> Result<Atlas, AtlasError> {
    let mut scanner = Scanner::new(xml);
    let root = scanner
        .next_start_tag()?
        .ok_or_else(|| AtlasError::malformed("no root element"))?;

    if root.name != "TextureAtlas" {
        return Err(AtlasError::NotAnAtlas {
            uri: INLINE_URI.to_string(),
            root_tag: root.name.to_string(),
        });
    }

    let image_path = root.attribute("imagePath").unwrap_or("").to_string();
    let mut subtextures = Vec::new();

    while let Some(tag) = scanner.next_start_tag()? {
        if tag.name != "SubTexture" {
            continue;
        }
        subtextures.push(SubTexture {
            name: tag.required_attribute("name")?.to_string(),
            x: tag.optional_i32("x")?.unwrap_or(0),
            y: tag.optional_i32("y")?.unwrap_or(0),
            width: tag.optional_i32("width")?.unwrap_or(0),
            height: tag.optional_i32("height")?.unwrap_or(0),
            frame_x: tag.optional_i32("frameX")?.unwrap_or(0),
            frame_y: tag.optional_i32("frameY")?.unwrap_or(0),
            frame_width: tag.optional_i32("frameWidth")?.unwrap_or(0),
            frame_height: tag.optional_i32("frameHeight")?.unwrap_or(0),
            rotated: tag.is_true("rotated"),
        });
    }

    Ok(Atlas {
        image_path,
        subtextures,
    })
}
