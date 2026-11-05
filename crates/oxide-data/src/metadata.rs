use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Song metadata. This is the same struct for current (2.2.x) metadata and
/// legacy (2.0.0) music metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "songName", skip_serializing_if = "Option::is_none")]
    pub song_name: Option<String>,
    #[serde(rename = "artist", skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(rename = "charter", skip_serializing_if = "Option::is_none")]
    pub charter: Option<String>,
    #[serde(rename = "offsets", skip_serializing_if = "Option::is_none")]
    pub offsets: Option<Value>,
    #[serde(rename = "playData", skip_serializing_if = "Option::is_none")]
    pub play_data: Option<PlayData>,
    #[serde(rename = "generatedBy", skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    #[serde(
        rename = "timeChanges",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub time_changes: Option<Vec<TimeChange>>,
    #[serde(rename = "timeFormat", skip_serializing_if = "Option::is_none")]
    pub time_format: Option<String>,
    #[serde(rename = "looped", skip_serializing_if = "Option::is_none")]
    pub looped: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayData {
    #[serde(
        rename = "songVariations",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub song_variations: Option<Vec<String>>,
    #[serde(rename = "stickerPack", skip_serializing_if = "Option::is_none")]
    pub sticker_pack: Option<String>,
    #[serde(rename = "difficulties", default)]
    pub difficulties: Vec<String>,
    #[serde(rename = "characters", skip_serializing_if = "Option::is_none")]
    pub characters: Option<PlayCharacters>,
    #[serde(rename = "stage", skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(rename = "noteStyle", skip_serializing_if = "Option::is_none")]
    pub note_style: Option<String>,
    #[serde(rename = "ratings", skip_serializing_if = "Option::is_none")]
    pub ratings: Option<Value>,
    #[serde(rename = "album", skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlayCharacters {
    #[serde(rename = "player", skip_serializing_if = "Option::is_none")]
    pub player: Option<String>,
    #[serde(rename = "girlfriend", skip_serializing_if = "Option::is_none")]
    pub girlfriend: Option<String>,
    #[serde(rename = "opponent", skip_serializing_if = "Option::is_none")]
    pub opponent: Option<String>,
    #[serde(rename = "instrumental", skip_serializing_if = "Option::is_none")]
    pub instrumental: Option<String>,
    #[serde(rename = "altInstrumentals", skip_serializing_if = "Option::is_none")]
    pub alt_instrumentals: Option<Vec<String>>,
    #[serde(rename = "opponentVocals", skip_serializing_if = "Option::is_none")]
    pub opponent_vocals: Option<Vec<String>>,
    #[serde(rename = "playerVocals", skip_serializing_if = "Option::is_none")]
    pub player_vocals: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeChange {
    #[serde(rename = "t")]
    pub time: f64,
    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    pub measure: Option<f64>,
    #[serde(rename = "bpm")]
    pub bpm: f64,
    #[serde(rename = "bt", skip_serializing_if = "Option::is_none")]
    pub beat_time: Option<Vec<f64>>,
    #[serde(rename = "n", skip_serializing_if = "Option::is_none")]
    pub time_signature_num: Option<i64>,
    #[serde(rename = "d", skip_serializing_if = "Option::is_none")]
    pub time_signature_den: Option<i64>,
}
