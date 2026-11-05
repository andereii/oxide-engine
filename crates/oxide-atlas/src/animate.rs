//! Animate Atlas (`spritemap1.json`) structural parser.

use serde_json::Value;

use crate::AtlasError;

/// One Animate Atlas `SPRITE` entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimateSprite {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub rotated: bool,
}

/// The structural contents of an Animate Atlas `spritemap1.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimateAtlas {
    pub image: String,
    pub sprites: Vec<AnimateSprite>,
}

/// Parses the structural contents of an Animate Atlas `spritemap1.json`:
/// `ATLAS.SPRITES[].SPRITE` plus `meta.image`. No playback or frame
/// resolution happens here.
pub fn parse_animate(json: &str) -> Result<AnimateAtlas, AtlasError> {
    let value = parse_json(json)?;

    let image = value
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get("image"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let entries = value
        .get("ATLAS")
        .and_then(|atlas| atlas.get("SPRITES"))
        .and_then(Value::as_array)
        .ok_or_else(|| AtlasError::json("missing `ATLAS.SPRITES` array"))?;

    let mut sprites = Vec::with_capacity(entries.len());
    for entry in entries {
        let sprite = entry
            .get("SPRITE")
            .and_then(Value::as_object)
            .ok_or_else(|| AtlasError::json("`ATLAS.SPRITES` entry has no `SPRITE` object"))?;
        let name = sprite
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| AtlasError::json("`SPRITE` has no string `name`"))?
            .to_string();
        let rotated = match sprite.get("rotated") {
            None => false,
            Some(flag) => flag.as_bool().ok_or_else(|| {
                AtlasError::json(format!("`SPRITE` `{name}` has a non-boolean `rotated`"))
            })?,
        };
        sprites.push(AnimateSprite {
            name,
            x: int_field(sprite, "x")?,
            y: int_field(sprite, "y")?,
            w: int_field(sprite, "w")?,
            h: int_field(sprite, "h")?,
            rotated,
        });
    }

    Ok(AnimateAtlas { image, sprites })
}

fn parse_json(json: &str) -> Result<Value, AtlasError> {
    let json = json.strip_prefix('\u{FEFF}').unwrap_or(json);
    serde_json::from_str(json).map_err(|err| AtlasError::json(err.to_string()))
}

fn int_field(object: &serde_json::Map<String, Value>, key: &str) -> Result<i32, AtlasError> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| AtlasError::json(format!("`SPRITE` has no integer `{key}`")))
}
