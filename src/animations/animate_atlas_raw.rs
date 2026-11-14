use serde::Deserialize;

use super::animate_atlas::{AnimateElement, AnimateTimelineFrame, AnimateTimelineLayer};

pub(super) fn convert_layer(raw: LayerRaw) -> AnimateTimelineLayer {
    let frames = raw
        .frames
        .into_iter()
        .map(|f| AnimateTimelineFrame {
            index: f.index,
            duration: f.duration.max(1),
            name: f.name,
            elements: f
                .elements
                .into_iter()
                .filter_map(|e| {
                    if let Some(si) = e.symbol_instance {
                        Some(AnimateElement {
                            symbol_name: Some(si.symbol_name),
                            sprite_name: None,
                            matrix: parse_matrix(si.mx, si.m3d),
                            first_frame: si.first_frame,
                            loop_mode: si.loop_mode,
                        })
                    } else {
                        e.atlas_sprite_instance.map(|asi| AnimateElement {
                            symbol_name: None,
                            sprite_name: Some(asi.name),
                            matrix: parse_matrix(asi.mx, asi.m3d),
                            first_frame: 0,
                            loop_mode: "LP".to_string(),
                        })
                    }
                })
                .collect(),
        })
        .collect();
    AnimateTimelineLayer {
        name: raw.layer_name.unwrap_or_default(),
        frames,
    }
}

fn parse_matrix(mx: Option<[f32; 6]>, m3d: Option<[f32; 16]>) -> [f32; 6] {
    if let Some(m) = mx {
        return m;
    }
    if let Some(m) = m3d {
        return [m[0], m[1], m[4], m[5], m[12], m[13]];
    }
    [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
}

#[derive(Deserialize)]
pub(super) struct SpritemapJsonRoot {
    #[serde(rename = "ATLAS")]
    pub atlas: SpritemapAtlasRaw,
}

#[derive(Deserialize)]
pub(super) struct SpritemapAtlasRaw {
    #[serde(rename = "SPRITES")]
    pub sprites: Vec<SpriteWrapperRaw>,
}

#[derive(Deserialize)]
pub(super) struct SpriteWrapperRaw {
    #[serde(rename = "SPRITE")]
    pub sprite: SpriteEntryRaw,
}

#[derive(Deserialize)]
pub(super) struct SpriteEntryRaw {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    #[serde(default)]
    pub rotated: bool,
}

#[derive(Deserialize)]
pub(super) struct AnimationJsonRoot {
    #[serde(rename = "ANIMATION", alias = "AN")]
    pub animation: AnimationHeaderRaw,
    #[serde(rename = "SYMBOL_DICTIONARY", alias = "SD")]
    pub symbol_dictionary: Option<SymbolDictionaryRaw>,
    #[serde(rename = "metadata", alias = "MD")]
    pub metadata: Option<MetadataRaw>,
}

#[derive(Deserialize)]
pub(super) struct MetadataRaw {
    #[serde(rename = "FRT", default = "default_fps")]
    pub framerate: f32,
}

fn default_fps() -> f32 {
    24.0
}

#[derive(Deserialize)]
pub(super) struct AnimationHeaderRaw {
    #[serde(rename = "N")]
    pub name: Option<String>,
    #[serde(rename = "SN")]
    pub symbol_name: Option<String>,
    #[serde(rename = "TL")]
    pub timeline: TimelineRaw,
}

#[derive(Deserialize)]
pub(super) struct SymbolDictionaryRaw {
    #[serde(rename = "Symbols", alias = "S", default)]
    pub symbols: Vec<SymbolEntryRaw>,
}

#[derive(Deserialize)]
pub(super) struct SymbolEntryRaw {
    #[serde(rename = "SN")]
    pub symbol_name: String,
    #[serde(rename = "TL")]
    pub timeline: TimelineRaw,
}

#[derive(Deserialize)]
pub(super) struct TimelineRaw {
    #[serde(rename = "L", alias = "LAYERS", default)]
    pub layers: Vec<LayerRaw>,
}

#[derive(Deserialize)]
pub(super) struct LayerRaw {
    #[serde(rename = "Layer_name", alias = "LN")]
    pub layer_name: Option<String>,
    #[serde(rename = "FR", default)]
    pub frames: Vec<FrameRaw>,
}

#[derive(Deserialize)]
pub(super) struct FrameRaw {
    #[serde(rename = "I", default)]
    pub index: usize,
    #[serde(rename = "DU", default)]
    pub duration: usize,
    #[serde(rename = "N")]
    pub name: Option<String>,
    #[serde(rename = "E", default)]
    pub elements: Vec<ElementRaw>,
}

#[derive(Deserialize)]
pub(super) struct ElementRaw {
    #[serde(rename = "SI")]
    pub symbol_instance: Option<SymbolInstanceRaw>,
    #[serde(rename = "ASI")]
    pub atlas_sprite_instance: Option<AtlasSpriteInstanceRaw>,
}

#[derive(Deserialize)]
pub(super) struct SymbolInstanceRaw {
    #[serde(rename = "SN")]
    pub symbol_name: String,
    #[serde(rename = "MX")]
    pub mx: Option<[f32; 6]>,
    #[serde(rename = "M3D")]
    pub m3d: Option<[f32; 16]>,
    #[serde(rename = "FF", default)]
    pub first_frame: usize,
    #[serde(rename = "LP", default = "default_loop_mode")]
    pub loop_mode: String,
}

fn default_loop_mode() -> String {
    "LP".to_string()
}

#[derive(Deserialize)]
pub(super) struct AtlasSpriteInstanceRaw {
    #[serde(rename = "N")]
    pub name: String,
    #[serde(rename = "MX")]
    pub mx: Option<[f32; 6]>,
    #[serde(rename = "M3D")]
    pub m3d: Option<[f32; 16]>,
}

