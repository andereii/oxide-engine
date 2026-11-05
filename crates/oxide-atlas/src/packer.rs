//! TexturePacker JSON (hash and array) parser.

use serde_json::{Map, Value};

use crate::AtlasError;

/// One TexturePacker frame entry, mapped 1:1 from the JSON.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackerFrame {
    pub name: String,
    /// `frame.x`, `frame.y`, `frame.w`, `frame.h`.
    pub frame: [i32; 4],
    pub rotated: bool,
    pub trimmed: bool,
    /// `spriteSourceSize.x`, `.y`, `.w`, `.h`.
    pub sprite_source_size: [i32; 4],
    /// `sourceSize.w`, `.h`.
    pub source_size: [i32; 2],
}

/// A parsed TexturePacker atlas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackerAtlas {
    pub image: String,
    pub frames: Vec<PackerFrame>,
}

/// Parses a TexturePacker JSON atlas. `frames` may be an object keyed by frame
/// name or an array of entries carrying a `filename` field.
pub fn parse_packer(json: &str) -> Result<PackerAtlas, AtlasError> {
    let value = parse_json(json)?;
    let root = value
        .as_object()
        .ok_or_else(|| AtlasError::json("root is not a JSON object"))?;

    let image = root
        .get("meta")
        .and_then(Value::as_object)
        .and_then(|meta| meta.get("image"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let frames_value = root
        .get("frames")
        .ok_or_else(|| AtlasError::json("missing `frames`"))?;
    let frames = match frames_value {
        Value::Object(by_name) => by_name
            .iter()
            .map(|(name, entry)| parse_frame(name, entry))
            .collect::<Result<Vec<_>, _>>()?,
        Value::Array(entries) => entries
            .iter()
            .map(|entry| {
                let name = entry
                    .get("filename")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        AtlasError::json("`frames` array entry has no string `filename`")
                    })?;
                parse_frame(name, entry)
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(AtlasError::json("`frames` must be an object or an array"));
        }
    };

    Ok(PackerAtlas { image, frames })
}

fn parse_json(json: &str) -> Result<Value, AtlasError> {
    let json = json.strip_prefix('\u{FEFF}').unwrap_or(json);
    serde_json::from_str(json).map_err(|err| AtlasError::json(err.to_string()))
}

fn parse_frame(name: &str, entry: &Value) -> Result<PackerFrame, AtlasError> {
    let object = entry
        .as_object()
        .ok_or_else(|| AtlasError::json(format!("frame `{name}` is not a JSON object")))?;
    let frame = field_object(object, "frame", name)?;
    let sprite_source_size = field_object(object, "spriteSourceSize", name)?;
    let source_size = field_object(object, "sourceSize", name)?;

    Ok(PackerFrame {
        name: name.to_string(),
        frame: [
            int_field(frame, "x", name)?,
            int_field(frame, "y", name)?,
            int_field(frame, "w", name)?,
            int_field(frame, "h", name)?,
        ],
        rotated: bool_field(object, "rotated", name)?,
        trimmed: bool_field(object, "trimmed", name)?,
        sprite_source_size: [
            int_field(sprite_source_size, "x", name)?,
            int_field(sprite_source_size, "y", name)?,
            int_field(sprite_source_size, "w", name)?,
            int_field(sprite_source_size, "h", name)?,
        ],
        source_size: [
            int_field(source_size, "w", name)?,
            int_field(source_size, "h", name)?,
        ],
    })
}

fn field_object<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    frame: &str,
) -> Result<&'a Map<String, Value>, AtlasError> {
    object
        .get(key)
        .and_then(Value::as_object)
        .ok_or_else(|| AtlasError::json(format!("frame `{frame}` has no object `{key}`")))
}

fn int_field(object: &Map<String, Value>, key: &str, frame: &str) -> Result<i32, AtlasError> {
    object
        .get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| {
            AtlasError::json(format!(
                "frame `{frame}` has no integer `{key}` in its geometry"
            ))
        })
}

fn bool_field(object: &Map<String, Value>, key: &str, frame: &str) -> Result<bool, AtlasError> {
    object
        .get(key)
        .and_then(Value::as_bool)
        .ok_or_else(|| AtlasError::json(format!("frame `{frame}` has no boolean `{key}`")))
}
