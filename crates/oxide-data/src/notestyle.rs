use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A V-Slice note style. `assets` is kept verbatim in M1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteStyle {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "author", skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    // Deliberately not skipped: real note styles spell an explicit JSON `null`
    // for this field, and that null must survive a round-trip.
    #[serde(rename = "fallback")]
    pub fallback: Option<String>,
    #[serde(rename = "assets")]
    pub assets: Value,
}
