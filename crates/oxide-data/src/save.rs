use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A V-Slice save document. The nested maps are kept verbatim in M1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SaveData {
    #[serde(rename = "options", skip_serializing_if = "Option::is_none")]
    pub options: Option<Value>,
    #[serde(rename = "songScores", skip_serializing_if = "Option::is_none")]
    pub song_scores: Option<Value>,
    #[serde(rename = "songRankings", skip_serializing_if = "Option::is_none")]
    pub song_rankings: Option<Value>,
    #[serde(rename = "unlocks", skip_serializing_if = "Option::is_none")]
    pub unlocks: Option<Value>,
    #[serde(rename = "newCharacter", skip_serializing_if = "Option::is_none")]
    pub new_character: Option<String>,
    #[serde(rename = "vars", skip_serializing_if = "Option::is_none")]
    pub vars: Option<Value>,
}
