use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteskinFileRaw {
    #[serde(default)]
    pub is_pixel: bool,
    pub gameplay: GameplaySectionRaw,
    pub misses: Option<MissesSectionRaw>,
    pub ui: Option<UiSectionRaw>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameplaySectionRaw {
    pub strumline: StrumlineSectionRaw,
    pub notes: NotesSectionRaw,
    pub sustains: SustainsSectionRaw,
    pub note_splashes: Option<NoteSplashesSectionRaw>,
    pub hold_covers: Option<HoldCoversSectionRaw>,
}

#[derive(Debug, Deserialize)]
pub struct StrumlineSectionRaw {
    pub path: String,
    pub scale: f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(default)]
    pub offsets: Option<StrumOffsetsRaw>,
    pub animations: HashMap<String, StrumAnimRaw>,
}

#[derive(Debug, Default, Deserialize)]
pub struct StrumOffsetsRaw {
    #[serde(rename = "static", default)]
    pub static_offset: [f32; 2],
    #[serde(default)]
    pub press: [f32; 2],
    #[serde(default)]
    pub confirm: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct StrumAnimRaw {
    #[serde(rename = "static")]
    pub static_anim: String,
    pub press: String,
    pub confirm: String,
    #[serde(rename = "confirmHold", default)]
    pub confirm_hold: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NotesSectionRaw {
    pub path: String,
    pub scale: f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(rename = "Offset", default)]
    pub offset: [f32; 2],
    pub animations: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SustainsSectionRaw {
    pub path: String,
    pub scale: f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(rename = "Offset", default)]
    pub offset: [f32; 2],
    #[serde(rename = "OffsetHit", default)]
    pub offset_hit: [f32; 2],
    #[serde(default)]
    pub end_offset: [f32; 2],
    pub animations: HashMap<String, SustainAnimRaw>,
}

#[derive(Debug, Deserialize)]
pub struct SustainAnimRaw {
    pub body: String,
    pub end: String,
}

#[derive(Debug, Deserialize)]
pub struct NoteSplashesSectionRaw {
    pub path: String,
    #[serde(default = "default_one")]
    pub scale: f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(rename = "Offset", default)]
    pub offset: [f32; 2],
    #[serde(default)]
    pub animations: HashMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize)]
pub struct HoldCoversSectionRaw {
    #[serde(default = "default_one")]
    pub scale: f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(default = "default_hold_cover_fps")]
    pub frames: f32,
    #[serde(rename = "Offset", default)]
    pub offset: [f32; 2],
    #[serde(default)]
    pub directions: HashMap<String, HoldCoverDirRaw>,
}

#[derive(Debug, Deserialize)]
pub struct HoldCoverDirRaw {
    pub path: String,
    pub animations: HoldCoverAnimRaw,
}

#[derive(Debug, Deserialize)]
pub struct HoldCoverAnimRaw {
    pub start: String,
    pub hold: String,
    pub end: String,
}

#[derive(Debug, Deserialize)]
pub struct MissesSectionRaw {
    pub sounds: MissSoundsRaw,
}

#[derive(Debug, Deserialize)]
pub struct MissSoundsRaw {
    #[serde(default = "default_miss_volume")]
    pub volume: f32,
    #[serde(default)]
    pub path: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSectionRaw {
    pub countdown: Option<CountdownUiRaw>,
    pub judgments: Option<HashMap<String, UiSpriteAssetRaw>>,
    pub combo_numbers: Option<ComboNumbersRaw>,
}

#[derive(Debug, Default, Deserialize)]
pub struct UiSpriteAssetRaw {
    pub path: Option<String>,
    #[serde(default = "default_one")]
    pub scale: f32,
    #[serde(default = "default_one")]
    pub alpha: f32,
    #[serde(rename = "Offset", alias = "offsets", default)]
    pub offset: [f32; 2],
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComboNumbersRaw {
    #[serde(default = "default_half")]
    pub global_scale: f32,
    #[serde(default = "default_one")]
    pub global_alpha: f32,
    #[serde(rename = "Offset", alias = "offsets", default)]
    pub offset: [f32; 2],
    #[serde(default)]
    pub assets: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct UiSpriteStyle {
    pub path: Option<String>,
    pub scale: f32,
    pub alpha: f32,
    pub offset: [f32; 2],
}

impl Default for UiSpriteStyle {
    fn default() -> Self {
        Self { path: None, scale: 1.0, alpha: 1.0, offset: [0.0, 0.0] }
    }
}

#[derive(Debug, Clone)]
pub struct PopupSkinConfig {
    pub judgments: [UiSpriteStyle; 4],
    pub combo_scale: f32,
    pub combo_alpha: f32,
    pub combo_offset: [f32; 2],
    pub combo_digits: [Option<String>; 10],
}

impl PopupSkinConfig {
    pub fn fallback_funkin() -> Self {
        let names = ["sick", "good", "bad", "shit"];
        let judgments = names.map(|n| UiSpriteStyle {
            path: Some(format!("popup/{n}.png")),
            scale: 0.65,
            alpha: 1.0,
            offset: [0.0, 0.0],
        });
        let combo_digits = std::array::from_fn(|d| Some(format!("popup/digit-{d}.png")));
        Self { judgments, combo_scale: 0.5, combo_alpha: 1.0, combo_offset: [0.0, 0.0], combo_digits }
    }

    pub fn apply_ui(&mut self, ui: &UiSectionRaw) {
        if let Some(map) = &ui.judgments {
            for (idx, key) in ["sick", "good", "bad", "shit"].iter().enumerate() {
                if let Some(entry) = map.get(*key) {
                    self.judgments[idx] = UiSpriteStyle {
                        path: entry.path.clone(),
                        scale: entry.scale,
                        alpha: entry.alpha,
                        offset: entry.offset,
                    };
                }
            }
        }
        if let Some(cb) = &ui.combo_numbers {
            self.combo_scale = cb.global_scale;
            self.combo_alpha = cb.global_alpha;
            self.combo_offset = cb.offset;
            for d in 0..10 {
                if let Some(path) = cb.assets.get(&d.to_string()) {
                    self.combo_digits[d] = Some(path.clone());
                }
            }
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CountdownUiRaw {
    pub three: Option<CountdownStepRaw>,
    pub two: Option<CountdownStepRaw>,
    pub one: Option<CountdownStepRaw>,
    pub go: Option<CountdownStepRaw>,
}

#[derive(Debug, Default, Deserialize)]
pub struct CountdownStepRaw {
    pub audio: Option<CountdownAudioRaw>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountdownAudioRaw {
    pub path: Option<String>,
    pub asset_path: Option<String>,
    #[serde(default = "default_one")]
    pub volume: f32,
}

fn default_one() -> f32 {
    1.0
}

fn default_half() -> f32 {
    0.5
}

fn default_hold_cover_fps() -> f32 {
    24.0
}

fn default_miss_volume() -> f32 {
    0.8
}
