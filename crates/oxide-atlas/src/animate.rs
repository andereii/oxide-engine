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
    /// `meta.size` as `[width, height]` when the spritemap declares both.
    ///
    /// Observed shipped shape: `shared/images/characters/sserafim/sakura/
    /// spritemap1.json` carries `meta.size = {w: 2043, h: 1950}` while the
    /// sibling `Animation.json` has `MD: {"FRT":24}` without `W`/`H`
    /// (M6-CONTRACT §11.5). On files that do carry `MD.W`/`MD.H`, the two
    /// sources are data-equivalent; [`crate::AnimateStage`] only consults this
    /// value as the documented fallback, so existing parse results keep using
    /// the metadata fields.
    pub page_size: Option<[u32; 2]>,
}

/// Parses the structural contents of an Animate Atlas `spritemap1.json`:
/// `ATLAS.SPRITES[].SPRITE` plus `meta.image`. No playback or frame
/// resolution happens here.
pub fn parse_animate(json: &str) -> Result<AnimateAtlas, AtlasError> {
    let value = parse_json(json)?;

    let meta = value.get("meta").and_then(Value::as_object);
    let image = meta
        .and_then(|meta| meta.get("image"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let page_size = meta
        .and_then(|meta| meta.get("size"))
        .and_then(Value::as_object)
        .and_then(|size| {
            let width = u32::try_from(size.get("w").and_then(Value::as_u64)?).ok()?;
            let height = u32::try_from(size.get("h").and_then(Value::as_u64)?).ok()?;
            Some([width, height])
        });

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

    Ok(AnimateAtlas {
        image,
        sprites,
        page_size,
    })
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
