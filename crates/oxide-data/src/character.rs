use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A V-Slice character definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CharacterData {
    #[serde(rename = "version")]
    pub version: String,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "assetPath")]
    pub asset_path: String,
    #[serde(rename = "renderType", skip_serializing_if = "Option::is_none")]
    pub render_type: Option<String>,
    #[serde(rename = "isPixel", skip_serializing_if = "Option::is_none")]
    pub is_pixel: Option<bool>,
    #[serde(rename = "scale", skip_serializing_if = "Option::is_none")]
    pub scale: Option<f64>,
    #[serde(rename = "offsets", skip_serializing_if = "Option::is_none")]
    pub offsets: Option<(f64, f64)>,
    #[serde(rename = "cameraOffsets", skip_serializing_if = "Option::is_none")]
    pub camera_offsets: Option<(f64, f64)>,
    #[serde(rename = "danceEvery", skip_serializing_if = "Option::is_none")]
    pub dance_every: Option<f64>,
    #[serde(rename = "singTime", skip_serializing_if = "Option::is_none")]
    pub sing_time: Option<f64>,
    #[serde(rename = "flipX", skip_serializing_if = "Option::is_none")]
    pub flip_x: Option<bool>,
    #[serde(rename = "startingAnimation", skip_serializing_if = "Option::is_none")]
    pub starting_animation: Option<String>,
    #[serde(rename = "atlasSettings", skip_serializing_if = "Option::is_none")]
    pub atlas_settings: Option<Value>,
    #[serde(rename = "death", skip_serializing_if = "Option::is_none")]
    pub death: Option<Value>,
    #[serde(rename = "healthIcon", skip_serializing_if = "Option::is_none")]
    pub health_icon: Option<Value>,
    #[serde(rename = "animations", default)]
    pub animations: Vec<AnimationData>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnimationData {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "prefix", skip_serializing_if = "Option::is_none")]
    pub prefix: Option<String>,
    #[serde(rename = "offsets", skip_serializing_if = "Option::is_none")]
    pub offsets: Option<(f64, f64)>,
    #[serde(rename = "looped", skip_serializing_if = "Option::is_none")]
    pub looped: Option<bool>,
    #[serde(rename = "frameRate", skip_serializing_if = "Option::is_none")]
    pub frame_rate: Option<f64>,
    #[serde(rename = "flipX", skip_serializing_if = "Option::is_none")]
    pub flip_x: Option<bool>,
    #[serde(rename = "flipY", skip_serializing_if = "Option::is_none")]
    pub flip_y: Option<bool>,
    #[serde(rename = "animType", skip_serializing_if = "Option::is_none")]
    pub anim_type: Option<String>,
    #[serde(rename = "library", skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    #[serde(rename = "frameIndices", skip_serializing_if = "Option::is_none")]
    pub frame_indices: Option<Vec<i64>>,
    #[serde(rename = "assetPath", skip_serializing_if = "Option::is_none")]
    pub asset_path: Option<String>,
    #[serde(rename = "renderType", skip_serializing_if = "Option::is_none")]
    pub render_type: Option<String>,
}
