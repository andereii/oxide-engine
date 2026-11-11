//! Serde schemas for V-Slice data formats, `EventRegistry`, dialogue
//! documents and validation. This crate performs no file or network I/O.

pub mod character;
pub mod chart;
pub mod dialogue;
pub mod error;
pub mod events;
pub mod kind;
pub mod level;
pub mod metadata;
pub mod notestyle;
pub mod player;
pub mod save;
pub mod stage;

pub use error::DataError;
pub use kind::{DataKind, classify, implied_kind};

use serde::de::DeserializeOwned;
use serde_json::Value;

/// A parsed document: its classified kind and its typed representation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParsedDocument {
    pub kind: DataKind,
    pub typed: Typed,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum Typed {
    Unknown(Value),
    Chart(Box<chart::Chart>),
    Metadata(Box<metadata::Metadata>),
    NoteStyle(Box<notestyle::NoteStyle>),
    Character(Box<character::CharacterData>),
    Stage(Box<stage::StageData>),
    Level(Box<level::LevelData>),
    Save(Box<save::SaveData>),
    Player(Box<player::PlayerData>),
}

// `ParsedDocument` serializes its kind, so `DataKind` needs serde too. The
// enum lives in `kind.rs`; its representation is the unit variant name, which
// is what `#[derive(Serialize, Deserialize)]` would produce.
impl serde::Serialize for DataKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            Self::Chart => "Chart",
            Self::Metadata => "Metadata",
            Self::NoteStyle => "NoteStyle",
            Self::Character => "Character",
            Self::Stage => "Stage",
            Self::Level => "Level",
            Self::Save => "Save",
            Self::Player => "Player",
            Self::Unknown => "Unknown",
        })
    }
}

impl<'de> serde::Deserialize<'de> for DataKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        const VARIANTS: &[&str] = &[
            "Chart",
            "Metadata",
            "NoteStyle",
            "Character",
            "Stage",
            "Level",
            "Save",
            "Player",
            "Unknown",
        ];
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        match value.as_str() {
            "Chart" => Ok(Self::Chart),
            "Metadata" => Ok(Self::Metadata),
            "NoteStyle" => Ok(Self::NoteStyle),
            "Character" => Ok(Self::Character),
            "Stage" => Ok(Self::Stage),
            "Level" => Ok(Self::Level),
            "Save" => Ok(Self::Save),
            "Player" => Ok(Self::Player),
            "Unknown" => Ok(Self::Unknown),
            other => Err(<D::Error as serde::de::Error>::unknown_variant(
                other, VARIANTS,
            )),
        }
    }
}

/// Reads raw bytes, strips one leading UTF-8 BOM, parses JSON
/// (`DataError::Parse` on parse failure, `DataError::NotUtf8` if not UTF-8),
/// classifies, validates (`DataError::Schema` on failure).
pub fn parse_any(bytes: &[u8]) -> Result<ParsedDocument, DataError> {
    let root = parse_root(bytes)?;
    let kind = classify(&root);
    let typed = parse_payload(root, kind)?;
    Ok(ParsedDocument { kind, typed })
}

/// BOM-aware, path-first parse. Classification: `implied_kind(path)` wins;
/// otherwise shape-based `classify(root)`. Validation: the implied kind's
/// typed parse IS the check, so a chart whose `notes` is an array fails with
/// `DataError::Schema { kind: "chart", .. }`.
pub fn parse_file(path: &std::path::Path, bytes: &[u8]) -> Result<ParsedDocument, DataError> {
    let root = parse_root(bytes)?;
    let kind = implied_kind(path).unwrap_or_else(|| classify(&root));
    let typed = parse_payload(root, kind)?;
    Ok(ParsedDocument { kind, typed })
}

pub(crate) fn parse_root(bytes: &[u8]) -> Result<Value, DataError> {
    let bytes = strip_bom(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| DataError::NotUtf8)?;
    serde_json::from_str(text).map_err(|error| DataError::Parse(error.to_string()))
}

fn parse_payload(root: Value, kind: DataKind) -> Result<Typed, DataError> {
    Ok(match kind {
        DataKind::Unknown => Typed::Unknown(root),
        DataKind::Chart => Typed::Chart(Box::new(parse_typed(root, kind)?)),
        DataKind::Metadata => Typed::Metadata(Box::new(parse_typed(root, kind)?)),
        DataKind::NoteStyle => Typed::NoteStyle(Box::new(parse_typed(root, kind)?)),
        DataKind::Character => Typed::Character(Box::new(parse_typed(root, kind)?)),
        DataKind::Stage => Typed::Stage(Box::new(parse_typed(root, kind)?)),
        DataKind::Level => Typed::Level(Box::new(parse_typed(root, kind)?)),
        DataKind::Save => Typed::Save(Box::new(parse_typed(root, kind)?)),
        DataKind::Player => Typed::Player(Box::new(parse_typed(root, kind)?)),
    })
}

fn strip_bom(bytes: &[u8]) -> &[u8] {
    const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
    if bytes.starts_with(&BOM) {
        &bytes[BOM.len()..]
    } else {
        bytes
    }
}

fn parse_typed<T: DeserializeOwned>(root: Value, kind: DataKind) -> Result<T, DataError> {
    serde_json::from_value(root).map_err(|error| DataError::Schema {
        kind: kind.name(),
        message: error.to_string(),
    })
}
