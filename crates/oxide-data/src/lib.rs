//! Serde schemas for V-Slice data formats, `EventRegistry` and validation.
//! This crate performs no file or network I/O.

pub mod character;
pub mod chart;
pub mod error;
pub mod events;
pub mod kind;
pub mod level;
pub mod metadata;
pub mod notestyle;
pub mod save;
pub mod stage;

pub use error::DataError;
pub use kind::{DataKind, classify};

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
    let bytes = strip_bom(bytes);
    let text = std::str::from_utf8(bytes).map_err(|_| DataError::NotUtf8)?;
    let root: Value =
        serde_json::from_str(text).map_err(|error| DataError::Parse(error.to_string()))?;
    let kind = classify(&root);

    let typed = match kind {
        DataKind::Unknown => Typed::Unknown(root),
        DataKind::Chart => Typed::Chart(Box::new(parse_typed(root, kind)?)),
        DataKind::Metadata => Typed::Metadata(Box::new(parse_typed(root, kind)?)),
        DataKind::NoteStyle => Typed::NoteStyle(Box::new(parse_typed(root, kind)?)),
        DataKind::Character => Typed::Character(Box::new(parse_typed(root, kind)?)),
        DataKind::Stage => Typed::Stage(Box::new(parse_typed(root, kind)?)),
        DataKind::Level => Typed::Level(Box::new(parse_typed(root, kind)?)),
        DataKind::Save => Typed::Save(Box::new(parse_typed(root, kind)?)),
    };

    Ok(ParsedDocument { kind, typed })
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
