use crate::animations::BaseAnimationData;

use super::raw::parse_stage_raw;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StageCoordMode {
    #[default]
    Center,
    TopLeft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageAnimKind {
    Sparrow,
    Atlas,
    Packer,
}

#[derive(Debug, Clone)]
pub struct StageData {
    pub name: String,
    pub camera_zoom: f32,
    pub coord_mode: StageCoordMode,
    pub characters: StageCharacters,
    pub props: Vec<StagePropData>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StageCharacters {
    pub spectator: StageCharacterSlot,
    pub opponent: StageCharacterSlot,
    pub player: StageCharacterSlot,
}

impl Default for StageCharacters {
    fn default() -> Self {
        Self {
            spectator: StageCharacterSlot::default_for_spectator(),
            opponent: StageCharacterSlot::default_for_opponent(),
            player: StageCharacterSlot::default_for_player(),
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StageCharacterSlot {
    pub layer: i32,
    pub scale: [f32; 2],
    pub visible: bool,
    pub opacity: f32,
    pub position: [f32; 2],
    pub camera_offset: [f32; 2],
    pub flip: [bool; 2],
    pub scroll_factor: [f32; 2],
}

impl Default for StageCharacterSlot {
    fn default() -> Self {
        Self {
            layer: 0,
            scale: [1.0, 1.0],
            visible: true,
            opacity: 1.0,
            position: [0.0, 0.0],
            camera_offset: [0.0, 0.0],
            flip: [false, false],
            scroll_factor: [1.0, 1.0],
        }
    }
}

impl StageCharacterSlot {
    pub fn default_for_player() -> Self {
        Self {
            camera_offset: [-100.0, -100.0],
            ..Default::default()
        }
    }

    pub fn default_for_opponent() -> Self {
        Self {
            camera_offset: [100.0, -100.0],
            ..Default::default()
        }
    }

    pub fn default_for_spectator() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone)]
pub struct StagePropData {
    pub name: String,
    pub layer: i32,
    pub scale: [f32; 2],
    pub file: String,
    pub anim_kind: Option<StageAnimKind>,
    pub visible: bool,
    pub opacity: f32,
    pub position: [f32; 2],
    pub flip: [bool; 2],
    pub scroll_factor: [f32; 2],
    pub angle: f32,
    pub color: String,
    pub is_pixel: bool,
    pub dance_every: f32,
    pub starting_animation: Option<String>,
    pub animations: Vec<BaseAnimationData>,
}

impl Default for StagePropData {
    fn default() -> Self {
        Self {
            name: String::new(),
            layer: 0,
            scale: [1.0, 1.0],
            file: String::new(),
            anim_kind: None,
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

pub fn parse_stage_json(stage_id: &str, raw_json: &str) -> Result<StageData, String> {
    parse_stage_raw(stage_id, raw_json)
}
