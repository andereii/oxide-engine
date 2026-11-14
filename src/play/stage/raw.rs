use serde::Deserialize;
use serde::de::{self, Deserializer, SeqAccess, Visitor};

use crate::animations::BaseAnimationData;

use super::schema::{
    StageAnimKind, StageCharacterSlot, StageCharacters, StageCoordMode, StageData, StagePropData,
};

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StageFileRaw {
    name: Option<String>,
    #[serde(rename = "cameraZoom", alias = "camera_zoom")]
    camera_zoom: Option<f32>,
    stage: Vec<serde_json::Value>,
    props: Vec<StagePropRaw>,
    characters: Option<StageCharactersRaw>,
}

#[derive(Debug, Deserialize)]
struct StageCharactersWrapperRaw {
    characters: StageCharactersRaw,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct StageCharactersRaw {
    #[serde(alias = "gf")]
    spectator: Option<StageCharacterSlotRaw>,
    #[serde(alias = "dad")]
    opponent: Option<StageCharacterSlotRaw>,
    #[serde(alias = "bf")]
    player: Option<StageCharacterSlotRaw>,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct StageCharacterSlotRaw {
    #[serde(alias = "zIndex")]
    layer: i32,
    #[serde(deserialize_with = "de_scalar_or_vec2")]
    scale: [f32; 2],
    visible: bool,
    #[serde(alias = "alpha")]
    opacity: f32,
    position: [f32; 2],
    #[serde(rename = "camera_Offset", alias = "cameraOffsets", alias = "cameraOffset")]
    camera_offset: Option<[f32; 2]>,
    flip: [bool; 2],
    #[serde(rename = "scrollFactor", alias = "scroll", deserialize_with = "de_scalar_or_vec2")]
    scroll_factor: [f32; 2],
}

impl Default for StageCharacterSlotRaw {
    fn default() -> Self {
        Self {
            layer: 0,
            scale: [1.0, 1.0],
            visible: true,
            opacity: 1.0,
            position: [0.0, 0.0],
            camera_offset: None,
            flip: [false, false],
            scroll_factor: [1.0, 1.0],
        }
    }
}

impl StageCharacterSlotRaw {
    fn into_slot(self, default_cam_offset: [f32; 2]) -> StageCharacterSlot {
        StageCharacterSlot {
            layer: self.layer,
            scale: self.scale,
            visible: self.visible,
            opacity: self.opacity.clamp(0.0, 1.0),
            position: self.position,
            camera_offset: self.camera_offset.unwrap_or(default_cam_offset),
            flip: self.flip,
            scroll_factor: self.scroll_factor,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct StagePropRaw {
    name: String,
    #[serde(alias = "zIndex")]
    layer: i32,
    #[serde(deserialize_with = "de_scalar_or_vec2")]
    scale: [f32; 2],
    #[serde(alias = "assetPath", alias = "namePath")]
    file: String,
    #[serde(default, deserialize_with = "de_opt_opt_string")]
    animation: Option<Option<String>>,
    #[serde(rename = "animType", default, deserialize_with = "de_opt_opt_string")]
    anim_type: Option<Option<String>>,
    visible: bool,
    #[serde(alias = "alpha")]
    opacity: f32,
    position: [f32; 2],
    flip: [bool; 2],
    #[serde(rename = "scrollFactor", alias = "scroll", deserialize_with = "de_scalar_or_vec2")]
    scroll_factor: [f32; 2],
    angle: f32,
    color: String,
    #[serde(rename = "isPixel")]
    is_pixel: bool,
    #[serde(rename = "danceEvery")]
    dance_every: f32,
    #[serde(rename = "startingAnimation")]
    starting_animation: Option<String>,
    animations: Vec<BaseAnimationData>,
}

impl Default for StagePropRaw {
    fn default() -> Self {
        Self {
            name: String::new(),
            layer: 0,
            scale: [1.0, 1.0],
            file: String::new(),
            animation: None,
            anim_type: None,
            visible: true,
            opacity: 1.0,
            position: [0.0, 0.0],
            flip: [false, false],
            scroll_factor: [1.0, 1.0],
            angle: 0.0,
            color: "#FFFFFF".to_string(),
            is_pixel: false,
            dance_every: 0.0,
            starting_animation: None,
            animations: Vec::new(),
        }
    }
}

impl StagePropRaw {
    fn into_prop(self) -> StagePropData {
        let explicit_anim = self.animation.or(self.anim_type);
        let anim_kind = match explicit_anim {
            Some(None) => None,
            Some(Some(kind)) => parse_anim_kind(&kind),
            None => (!self.animations.is_empty()).then_some(StageAnimKind::Sparrow),
        };
        StagePropData {
            name: self.name,
            layer: self.layer,
            scale: self.scale,
            file: self.file,
            anim_kind,
            visible: self.visible,
            opacity: self.opacity.clamp(0.0, 1.0),
            position: self.position,
            flip: self.flip,
            scroll_factor: self.scroll_factor,
            angle: self.angle,
            color: self.color,
            is_pixel: self.is_pixel,
            dance_every: self.dance_every,
            starting_animation: self.starting_animation,
            animations: self.animations,
        }
    }
}

fn parse_anim_kind(raw: &str) -> Option<StageAnimKind> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "sparrow" => Some(StageAnimKind::Sparrow),
        "atlas" | "animateatlas" => Some(StageAnimKind::Atlas),
        "packer" => Some(StageAnimKind::Packer),
        _ => None,
    }
}

pub(super) fn parse_stage_raw(stage_id: &str, raw_json: &str) -> Result<StageData, String> {
    let raw: StageFileRaw = serde_json::from_str(raw_json)
        .map_err(|e| format!("failed to parse stage '{stage_id}': {e}"))?;

    let mut characters = raw.characters.map(convert_characters).unwrap_or_default();
    let coord_mode = if !raw.stage.is_empty() {
        StageCoordMode::Center
    } else {
        StageCoordMode::TopLeft
    };

    let mut props = Vec::new();
    for entry in raw.stage {
        if entry.get("characters").is_some() {
            if let Ok(wrapper) = serde_json::from_value::<StageCharactersWrapperRaw>(entry) {
                characters = convert_characters(wrapper.characters);
            }
            continue;
        }
        if let Ok(prop) = serde_json::from_value::<StagePropRaw>(entry) {
            props.push(prop.into_prop());
        }
    }
    for prop in raw.props {
        props.push(prop.into_prop());
    }

    Ok(StageData {
        name: raw.name.unwrap_or_else(|| stage_id.to_string()),
        camera_zoom: raw.camera_zoom.unwrap_or(1.0).max(0.05),
        coord_mode,
        characters,
        props,
    })
}

fn convert_characters(raw: StageCharactersRaw) -> StageCharacters {
    StageCharacters {
        spectator: raw
            .spectator
            .map(|c| c.into_slot([0.0, 0.0]))
            .unwrap_or_else(StageCharacterSlot::default_for_spectator),
        opponent: raw
            .opponent
            .map(|c| c.into_slot([100.0, -100.0]))
            .unwrap_or_else(StageCharacterSlot::default_for_opponent),
        player: raw
            .player
            .map(|c| c.into_slot([-100.0, -100.0]))
            .unwrap_or_else(StageCharacterSlot::default_for_player),
    }
}

fn de_opt_opt_string<'de, D>(deserializer: D) -> Result<Option<Option<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Some(Option::<String>::deserialize(deserializer)?))
}

fn de_scalar_or_vec2<'de, D>(deserializer: D) -> Result<[f32; 2], D::Error>
where
    D: Deserializer<'de>,
{
    struct ScalarOrVec2Visitor;
    impl<'de> Visitor<'de> for ScalarOrVec2Visitor {
        type Value = [f32; 2];

        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a number or a 2-element [x, y] array")
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok([v as f32, v as f32])
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok([v as f32, v as f32])
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
            Ok([v as f32, v as f32])
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let x = seq.next_element::<f32>()?.unwrap_or(1.0);
            let y = seq.next_element::<f32>()?.unwrap_or(x);
            while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
            Ok([x, y])
        }
    }
    deserializer.deserialize_any(ScalarOrVec2Visitor)
}

