//! Week-6 dialogue documents: conversations, boxes and speakers.
//!
//! The three families live under `data/dialogue/<family>/`. They follow the
//! same M1 contract as the other V-Slice schemas: BOM-aware UTF-8 JSON,
//! path-implied kind, typed validation and a value-preserving round-trip.
//! Unknown object keys are retained in the `extra` maps instead of being
//! rejected or silently dropped.

use std::collections::BTreeMap;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::character::AnimationData;
use crate::error::DataError;

/// The three week-6 dialogue document families.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DialogueKind {
    Conversation,
    Box,
    Speaker,
}

impl DialogueKind {
    /// Stable lowercase name used in schema errors and inventory reports.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Conversation => "dialogue-conversation",
            Self::Box => "dialogue-box",
            Self::Speaker => "dialogue-speaker",
        }
    }
}

/// The dialogue kind implied by the exact `data/dialogue/<family>/` layout.
/// Like [`crate::implied_kind`], a path outside that layout implies nothing.
pub fn implied_dialogue_kind(path: &Path) -> Option<DialogueKind> {
    let family = path.parent()?;
    let dialogue = family.parent()?;
    if dialogue.file_name()?.to_str()? != "dialogue" {
        return None;
    }
    if dialogue.parent()?.file_name()?.to_str()? != "data" {
        return None;
    }
    match family.file_name()?.to_str()? {
        "conversations" => Some(DialogueKind::Conversation),
        "boxes" => Some(DialogueKind::Box),
        "speakers" => Some(DialogueKind::Speaker),
        _ => None,
    }
}

/// Shape-based dialogue classification for callers without a path. Never
/// fails; an unrecognized shape returns `None` so the parse can fail closed.
pub fn classify_dialogue(root: &Value) -> Option<DialogueKind> {
    let object = root.as_object()?;
    if object.get("dialogue").is_some_and(Value::is_array)
        && object.get("backdrop").is_some_and(Value::is_object)
    {
        return Some(DialogueKind::Conversation);
    }
    if object.get("text").is_some_and(Value::is_object) {
        return Some(DialogueKind::Box);
    }
    if object.get("animations").is_some_and(Value::is_array)
        && object.get("assetPath").is_some_and(Value::is_string)
    {
        return Some(DialogueKind::Speaker);
    }
    None
}

/// A parsed dialogue document: its classified kind and typed representation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueDocument {
    pub kind: DialogueKind,
    pub typed: DialogueTyped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogueTyped {
    Conversation(Box<ConversationData>),
    Box(Box<DialogueBoxData>),
    Speaker(Box<SpeakerData>),
}

/// Parses a dialogue document without a path, classifying by JSON shape.
/// A document that matches none of the three shapes fails with
/// `DataError::Schema { kind: "dialogue", .. }`.
pub fn parse_dialogue_any(bytes: &[u8]) -> Result<DialogueDocument, DataError> {
    let root = crate::parse_root(bytes)?;
    let kind = classify_dialogue(&root).ok_or_else(|| DataError::Schema {
        kind: "dialogue",
        message: "document does not match a conversation, box or speaker shape".to_string(),
    })?;
    let typed = parse_dialogue_payload(root, kind)?;
    Ok(DialogueDocument { kind, typed })
}

/// BOM-aware, path-first dialogue parse. The path-implied family wins over the
/// JSON shape, so a box filed under `boxes/` is validated as a box and fails
/// if it is not one.
pub fn parse_dialogue_file(path: &Path, bytes: &[u8]) -> Result<DialogueDocument, DataError> {
    let root = crate::parse_root(bytes)?;
    let kind = implied_dialogue_kind(path)
        .or_else(|| classify_dialogue(&root))
        .ok_or_else(|| DataError::Schema {
            kind: "dialogue",
            message: format!(
                "{}: document does not match a conversation, box or speaker shape",
                path.display()
            ),
        })?;
    let typed = parse_dialogue_payload(root, kind)?;
    Ok(DialogueDocument { kind, typed })
}

fn parse_dialogue_payload(root: Value, kind: DialogueKind) -> Result<DialogueTyped, DataError> {
    Ok(match kind {
        DialogueKind::Conversation => {
            DialogueTyped::Conversation(Box::new(parse_dialogue_typed(root, kind)?))
        }
        DialogueKind::Box => DialogueTyped::Box(Box::new(parse_dialogue_typed(root, kind)?)),
        DialogueKind::Speaker => {
            DialogueTyped::Speaker(Box::new(parse_dialogue_typed(root, kind)?))
        }
    })
}

fn parse_dialogue_typed<T: DeserializeOwned>(
    root: Value,
    kind: DialogueKind,
) -> Result<T, DataError> {
    serde_json::from_value(root).map_err(|error| DataError::Schema {
        kind: kind.name(),
        message: error.to_string(),
    })
}

/// A dialogue box definition (`data/dialogue/boxes/*.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueBoxData {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "assetPath")]
    pub asset_path: String,
    #[serde(rename = "isPixel")]
    pub is_pixel: bool,
    #[serde(rename = "flipX")]
    pub flip_x: bool,
    #[serde(rename = "flipY")]
    pub flip_y: bool,
    /// Sprite offsets in pixels.
    #[serde(rename = "offsets")]
    pub offsets: (f64, f64),
    /// Sprite scale multiplier.
    #[serde(rename = "scale")]
    pub scale: f64,
    #[serde(rename = "text")]
    pub text: DialogueBoxTextData,
    #[serde(rename = "animations")]
    pub animations: Vec<AnimationData>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// Text layout and style for a dialogue box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueBoxTextData {
    /// Text offsets in pixels.
    #[serde(rename = "offsets")]
    pub offsets: (f64, f64),
    /// Maximum text width in pixels.
    #[serde(rename = "width")]
    pub width: i64,
    #[serde(rename = "size", skip_serializing_if = "Option::is_none")]
    pub size: Option<i64>,
    #[serde(rename = "color", deserialize_with = "deserialize_hex_color")]
    pub color: String,
    #[serde(rename = "fontFamily")]
    pub font_family: String,
    #[serde(
        rename = "shadowColor",
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "deserialize_optional_hex_color"
    )]
    pub shadow_color: Option<String>,
    #[serde(rename = "shadowWidth", skip_serializing_if = "Option::is_none")]
    pub shadow_width: Option<i64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// A dialogue speaker definition (`data/dialogue/speakers/*.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeakerData {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "assetPath")]
    pub asset_path: String,
    #[serde(rename = "flipX")]
    pub flip_x: bool,
    // Optional in the native schema; the shipped week-6 speakers omit it.
    #[serde(rename = "flipY", skip_serializing_if = "Option::is_none")]
    pub flip_y: Option<bool>,
    #[serde(rename = "isPixel")]
    pub is_pixel: bool,
    /// Sprite scale multiplier.
    #[serde(rename = "scale")]
    pub scale: f64,
    /// Sprite offsets in pixels.
    #[serde(rename = "offsets")]
    pub offsets: (f64, f64),
    #[serde(rename = "animations")]
    pub animations: Vec<AnimationData>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// A dialogue conversation (`data/dialogue/conversations/*.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversationData {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "backdrop")]
    pub backdrop: BackdropData,
    #[serde(rename = "music", skip_serializing_if = "Option::is_none")]
    pub music: Option<MusicData>,
    #[serde(rename = "outro", skip_serializing_if = "Option::is_none")]
    pub outro: Option<OutroData>,
    #[serde(rename = "dialogue")]
    pub dialogue: Vec<DialogueEntryData>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// The backdrop behind a conversation. Only the observed `solid` variant is
/// modeled; any other `type` fails closed during deserialization.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackdropData {
    Solid {
        /// Fade-in duration in seconds.
        #[serde(rename = "fadeTime")]
        fade_time: f64,
        #[serde(rename = "color", deserialize_with = "deserialize_hex_color")]
        color: String,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
}

/// How a conversation ends. Unknown `type` values fail closed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutroData {
    None {
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
    Fade {
        /// Fade-out duration in seconds.
        #[serde(rename = "fadeTime")]
        fade_time: f64,
        #[serde(flatten)]
        extra: BTreeMap<String, Value>,
    },
}

/// Conversation music.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MusicData {
    #[serde(rename = "asset")]
    pub asset: String,
    /// Fade-in duration in seconds.
    #[serde(rename = "fadeTime")]
    pub fade_time: f64,
    #[serde(rename = "looped")]
    pub looped: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// One conversation entry. Later strings in `text` are appended to the
/// already-typed text of the same entry (native `appendText` semantics).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueEntryData {
    #[serde(rename = "speaker")]
    pub speaker: String,
    #[serde(rename = "speakerAnimation")]
    pub speaker_animation: String,
    /// The dialogue box id (JSON key `box`).
    #[serde(rename = "box")]
    pub box_id: String,
    #[serde(rename = "boxAnimation")]
    pub box_animation: String,
    #[serde(rename = "text")]
    pub text: Vec<String>,
    /// Relative typing speed; native default is 1.0.
    #[serde(rename = "speed", skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

/// In-memory registry for the three dialogue families. There is no global
/// singleton: callers own one registry per loaded asset set.
#[derive(Debug, Clone, Default)]
pub struct DialogueRegistry {
    conversations: BTreeMap<String, ConversationData>,
    boxes: BTreeMap<String, DialogueBoxData>,
    speakers: BTreeMap<String, SpeakerData>,
}

impl DialogueRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts a parsed document under `id`, routing by its kind.
    /// Returns false when that family already contains `id`.
    pub fn insert(&mut self, id: impl Into<String>, document: DialogueDocument) -> bool {
        let id = id.into();
        match document.typed {
            DialogueTyped::Conversation(data) => self.insert_conversation(id, *data),
            DialogueTyped::Box(data) => self.insert_box(id, *data),
            DialogueTyped::Speaker(data) => self.insert_speaker(id, *data),
        }
    }

    /// Returns false when the conversation id is already present.
    pub fn insert_conversation(&mut self, id: impl Into<String>, data: ConversationData) -> bool {
        self.conversations.insert(id.into(), data).is_none()
    }

    /// Returns false when the box id is already present.
    pub fn insert_box(&mut self, id: impl Into<String>, data: DialogueBoxData) -> bool {
        self.boxes.insert(id.into(), data).is_none()
    }

    /// Returns false when the speaker id is already present.
    pub fn insert_speaker(&mut self, id: impl Into<String>, data: SpeakerData) -> bool {
        self.speakers.insert(id.into(), data).is_none()
    }

    pub fn conversation(&self, id: &str) -> Option<&ConversationData> {
        self.conversations.get(id)
    }

    pub fn box_data(&self, id: &str) -> Option<&DialogueBoxData> {
        self.boxes.get(id)
    }

    pub fn speaker(&self, id: &str) -> Option<&SpeakerData> {
        self.speakers.get(id)
    }

    /// Conversation ids in lexical order.
    pub fn conversation_ids(&self) -> Vec<String> {
        self.conversations.keys().cloned().collect()
    }

    /// Box ids in lexical order.
    pub fn box_ids(&self) -> Vec<String> {
        self.boxes.keys().cloned().collect()
    }

    /// Speaker ids in lexical order.
    pub fn speaker_ids(&self) -> Vec<String> {
        self.speakers.keys().cloned().collect()
    }
}

fn deserialize_hex_color<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if is_hex_color(&value) {
        Ok(value)
    } else {
        Err(<D::Error as serde::de::Error>::custom(format!(
            "expected a #RRGGBB or #RRGGBBAA color, got {value:?}"
        )))
    }
}

fn deserialize_optional_hex_color<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    if let Some(value) = &value
        && !is_hex_color(value)
    {
        return Err(<D::Error as serde::de::Error>::custom(format!(
            "expected a #RRGGBB or #RRGGBBAA color, got {value:?}"
        )));
    }
    Ok(value)
}

fn is_hex_color(value: &str) -> bool {
    let Some(hex) = value.strip_prefix('#') else {
        return false;
    };
    matches!(hex.len(), 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}
