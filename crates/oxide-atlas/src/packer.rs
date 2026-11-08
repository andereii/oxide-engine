//! TexturePacker JSON (hash and array) and shipped Packer-text parsers.

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

/// Parses the shipped Packer-text sibling format observed through
/// `Paths.getPackerAtlas` (`Paths.hx` L224-228: `<key>.png` + `<key>.txt`).
///
/// The observed format is one `frameName = x y width height` record per line
/// in document order (spaces are allowed inside the frame name before the
/// `" = "` separator). The native Packer sprite builds its animation frame
/// list from the collection order, so line order is the frame order.
/// `PackerFrame::source_size` equals the page rect because the shipped records
/// carry no trim/rotation data.
pub fn parse_packer_text(text: &str) -> Result<PackerAtlas, AtlasError> {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut frames = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, geometry) = line.split_once(" = ").ok_or_else(|| {
            AtlasError::json(format!(
                "packer text line {} has no ` = ` separator: {line:?}",
                line_number + 1
            ))
        })?;
        let name = name.trim();
        if name.is_empty() {
            return Err(AtlasError::json(format!(
                "packer text line {} has an empty frame name",
                line_number + 1
            )));
        }
        let values: Vec<&str> = geometry.split_whitespace().collect();
        if values.len() != 4 {
            return Err(AtlasError::json(format!(
                "packer text line {} must carry x y width height, got {geometry:?}",
                line_number + 1
            )));
        }
        let mut parsed = [0i32; 4];
        for (slot, value) in parsed.iter_mut().zip(values) {
            *slot = value.parse().map_err(|_| {
                AtlasError::json(format!(
                    "packer text line {} has a non-integer geometry value {value:?}",
                    line_number + 1
                ))
            })?;
        }
        frames.push(PackerFrame {
            name: name.to_string(),
            frame: parsed,
            rotated: false,
            trimmed: false,
            sprite_source_size: [0, 0, parsed[2], parsed[3]],
            source_size: [parsed[2], parsed[3]],
        });
    }
    if frames.is_empty() {
        return Err(AtlasError::json("packer text has no frame records"));
    }
    Ok(PackerAtlas {
        image: String::new(),
        frames,
    })
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
