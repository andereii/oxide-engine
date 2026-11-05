use serde_json::Value;

/// The kind of V-Slice JSON document contained in a payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DataKind {
    Chart,
    Metadata,
    NoteStyle,
    Character,
    Stage,
    Level,
    Save,
    Unknown,
}

impl DataKind {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Chart => "chart",
            Self::Metadata => "metadata",
            Self::NoteStyle => "notestyle",
            Self::Character => "character",
            Self::Stage => "stage",
            Self::Level => "level",
            Self::Save => "save",
            Self::Unknown => "unknown",
        }
    }
}

/// Never fails; anything not matching a kind's shape is Unknown.
pub fn classify(root: &Value) -> DataKind {
    let Some(object) = root.as_object() else {
        return DataKind::Unknown;
    };

    let version = object.get("version").and_then(Value::as_str);
    let version_is_two = version.is_some_and(|value| value.starts_with("2."));
    let has_notes_dict = object.get("notes").is_some_and(Value::is_object);

    if version_is_two && has_notes_dict {
        return DataKind::Chart;
    }

    if version_is_two
        && !has_notes_dict
        && (object.contains_key("songName") || object.contains_key("timeChanges"))
    {
        return DataKind::Metadata;
    }

    if version == Some("1.1.0") && object.contains_key("assets") {
        return DataKind::NoteStyle;
    }

    if version == Some("1.1.0") && object.contains_key("text") {
        return DataKind::Unknown;
    }

    let has_animations = object.get("animations").is_some_and(Value::is_array);
    let has_asset_path = object.get("assetPath").is_some_and(Value::is_string);
    let version_is_one = version.is_some_and(|value| value.starts_with("1."));

    if has_animations && has_asset_path && (version.is_none() || version_is_one) {
        return DataKind::Character;
    }

    if object.get("props").is_some_and(Value::is_array) {
        return if object.contains_key("songs") {
            DataKind::Level
        } else {
            DataKind::Stage
        };
    }

    if object.contains_key("songScores")
        || object.contains_key("songRankings")
        || (object.contains_key("options") && object.contains_key("vars"))
    {
        return DataKind::Save;
    }

    DataKind::Unknown
}
