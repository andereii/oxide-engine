use std::fmt;

use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};

use crate::animations::BaseAnimationData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CharacterRole {
    #[default]
    Player,
    Opponent,
    Spectator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CharacterRenderType {
    #[default]
    Sparrow,
    MultiSparrow,
    Packer,
    AnimateAtlas,
    MultiAnimateAtlas,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct HealthIconData {
    pub id: Option<String>,
    pub scale: Option<f32>,
    pub offsets: Option<[f32; 2]>,
    #[serde(rename = "flipX")]
    pub flip_x: Option<bool>,
    #[serde(rename = "isPixel")]
    pub is_pixel: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct DeathData {
    #[serde(rename = "cameraOffsets")]
    pub camera_offsets: Option<[f32; 2]>,
    #[serde(rename = "cameraZoom")]
    pub camera_zoom: Option<f32>,
    #[serde(rename = "preTransitionDelay")]
    pub pre_transition_delay: Option<f32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
#[allow(dead_code)]
pub struct CharacterData {
    pub version: String,
    pub name: String,
    #[serde(rename = "assetPath")]
    pub asset_path: Option<String>,
    #[serde(rename = "renderType")]
    pub render_type: CharacterRenderType,
    #[serde(deserialize_with = "de_scalar_or_vec2")]
    pub scale: [f32; 2],
    #[serde(rename = "healthIcon")]
    pub health_icon: Option<HealthIconData>,
    pub death: Option<DeathData>,
    pub offsets: [f32; 2],
    #[serde(rename = "cameraOffsets")]
    pub camera_offsets: [f32; 2],
    #[serde(rename = "isPixel")]
    pub is_pixel: bool,
    #[serde(rename = "danceEvery")]
    pub dance_every: f32,
    #[serde(rename = "singTime")]
    pub sing_time: f32,
    pub animations: Vec<BaseAnimationData>,
    #[serde(rename = "startingAnimation")]
    pub starting_animation: String,
    #[serde(rename = "flipX")]
    pub flip_x: bool,
}

impl Default for CharacterData {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            name: "Unknown".to_string(),
            asset_path: None,
            render_type: CharacterRenderType::Sparrow,
            scale: [1.0, 1.0],
            health_icon: None,
            death: None,
            offsets: [0.0, 0.0],
            camera_offsets: [0.0, 0.0],
            is_pixel: false,
            dance_every: 1.0,
            sing_time: 8.0,
            animations: Vec::new(),
            starting_animation: "idle".to_string(),
            flip_x: false,
        }
    }
}

fn de_scalar_or_vec2<'de, D>(deserializer: D) -> Result<[f32; 2], D::Error>
where
    D: Deserializer<'de>,
{
    struct ScalarOrVec2;
    impl<'de> Visitor<'de> for ScalarOrVec2 {
        type Value = [f32; 2];

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a number or 2-element array [x, y]")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<[f32; 2], E> {
            Ok([v as f32, v as f32])
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<[f32; 2], E> {
            Ok([v as f32, v as f32])
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<[f32; 2], E> {
            Ok([v as f32, v as f32])
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<[f32; 2], A::Error> {
            let x = seq.next_element::<f32>()?.unwrap_or(1.0);
            let y = seq.next_element::<f32>()?.unwrap_or(x);
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok([x, y])
        }
    }
    deserializer.deserialize_any(ScalarOrVec2)
}

