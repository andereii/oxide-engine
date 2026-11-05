use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A V-Slice song chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chart {
    pub version: String,
    #[serde(rename = "notes")]
    pub notes: BTreeMap<String, Vec<NoteData>>,
    #[serde(rename = "events", default)]
    pub events: Vec<SongEvent>,
    #[serde(rename = "generatedBy", skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    #[serde(rename = "scrollSpeed", skip_serializing_if = "Option::is_none")]
    pub scroll_speed: Option<Value>,
}

/// A single note. Times and sustain lengths are in milliseconds, exactly as
/// V-Slice stores them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteData {
    #[serde(rename = "t")]
    pub time: f64,
    #[serde(rename = "d")]
    pub direction: i64,
    #[serde(rename = "l", skip_serializing_if = "Option::is_none")]
    pub sustain: Option<f64>,
    #[serde(rename = "k", skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

/// A raw `{t, e, v}` song event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SongEvent {
    #[serde(rename = "t")]
    pub time: f64,
    #[serde(rename = "e")]
    pub event: String,
    #[serde(rename = "v", skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}
