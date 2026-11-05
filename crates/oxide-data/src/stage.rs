use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::character::AnimationData;

/// A V-Slice stage definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageData {
    // Optional so stages that predate the version field still parse.
    #[serde(rename = "version", skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "cameraZoom", skip_serializing_if = "Option::is_none")]
    pub camera_zoom: Option<f64>,
    #[serde(rename = "directory", skip_serializing_if = "Option::is_none")]
    pub directory: Option<String>,
    #[serde(rename = "characters", skip_serializing_if = "Option::is_none")]
    pub characters: Option<Value>,
    #[serde(rename = "props", default)]
    pub props: Vec<StageProp>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageProp {
    #[serde(rename = "name")]
    pub name: String,
    #[serde(rename = "assetPath", skip_serializing_if = "Option::is_none")]
    pub asset_path: Option<String>,
    #[serde(rename = "danceEvery", skip_serializing_if = "Option::is_none")]
    pub dance_every: Option<f64>,
    #[serde(rename = "zIndex", skip_serializing_if = "Option::is_none")]
    pub z_index: Option<f64>,
    #[serde(rename = "position", skip_serializing_if = "Option::is_none")]
    pub position: Option<(f64, f64)>,
    #[serde(rename = "scale", skip_serializing_if = "Option::is_none")]
    pub scale: Option<(f64, f64)>,
    #[serde(rename = "alpha", skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
    #[serde(rename = "angle", skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    #[serde(rename = "animType", skip_serializing_if = "Option::is_none")]
    pub anim_type: Option<String>,
    #[serde(rename = "scroll", skip_serializing_if = "Option::is_none")]
    pub scroll: Option<(f64, f64)>,
    #[serde(
        rename = "animations",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub animations: Option<Vec<AnimationData>>,
    #[serde(rename = "isPixel", skip_serializing_if = "Option::is_none")]
    pub is_pixel: Option<bool>,
    #[serde(rename = "flipX", skip_serializing_if = "Option::is_none")]
    pub flip_x: Option<bool>,
    #[serde(rename = "flipY", skip_serializing_if = "Option::is_none")]
    pub flip_y: Option<bool>,
    #[serde(rename = "blend", skip_serializing_if = "Option::is_none")]
    pub blend: Option<String>,
    #[serde(rename = "color", skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(rename = "startingAnimation", skip_serializing_if = "Option::is_none")]
    pub starting_animation: Option<String>,
    #[serde(rename = "atlasSettings", skip_serializing_if = "Option::is_none")]
    pub atlas_settings: Option<Value>,
}
