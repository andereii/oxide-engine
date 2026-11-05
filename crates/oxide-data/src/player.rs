use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A V-Slice playable character definition (`data/players/*.json`).
///
/// Nested menu/result stamps are kept verbatim until the milestone that
/// consumes them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayerData {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "ownedChars", default)]
    pub owned_chars: Vec<String>,
    #[serde(rename = "showUnownedChars", default)]
    pub show_unowned_chars: bool,
    #[serde(rename = "unlocked", default = "default_unlocked")]
    pub unlocked: bool,
    #[serde(rename = "freeplayStyle", skip_serializing_if = "Option::is_none")]
    pub freeplay_style: Option<String>,
    #[serde(rename = "stickerPack", skip_serializing_if = "Option::is_none")]
    pub sticker_pack: Option<String>,
    #[serde(rename = "freeplayDJ", skip_serializing_if = "Option::is_none")]
    pub freeplay_dj: Option<Value>,
    #[serde(rename = "charSelect", skip_serializing_if = "Option::is_none")]
    pub char_select: Option<Value>,
    #[serde(rename = "results", skip_serializing_if = "Option::is_none")]
    pub results: Option<Value>,
}

fn default_unlocked() -> bool {
    true
}
