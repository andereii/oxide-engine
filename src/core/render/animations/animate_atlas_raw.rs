use serde::Deserialize;

use super::animate_atlas::{
    AnimateBitmapFallback, AnimateElement, AnimateLayerType, AnimateTimelineFrame,
    AnimateTimelineLayer,
};

pub(super) fn convert_layer(raw: LayerRaw) -> AnimateTimelineLayer {
    let layer_type = if raw.clipped_by.is_some() {
        AnimateLayerType::Clipped
    } else {
        match raw.layer_type.as_deref() {
            Some("Clp" | "Clipper") => AnimateLayerType::Clipper,
            Some("Fld" | "Folder") => AnimateLayerType::Folder,
            Some("Guide") => AnimateLayerType::Guide,
            _ => AnimateLayerType::Normal,
        }
    };
    let visible = !matches!(
        layer_type,
        AnimateLayerType::Clipper | AnimateLayerType::Folder | AnimateLayerType::Guide
    );
    let frames = raw
        .frames
        .into_iter()
        .map(|f| AnimateTimelineFrame {
            index: f.index,
            duration: f.duration.max(1),
            name: f.name,
            color: parse_color_transform(f.color.as_ref()),
            elements: f
                .elements
                .into_iter()
                .filter_map(|e| {
                    if let Some(si) = e.symbol_instance {
                        let color = parse_color_transform(si.color.as_ref());
                        let bitmap_fallback = si.bitmap.map(|bm| AnimateBitmapFallback {
                            sprite_name: bm.name,
                            matrix: parse_matrix(bm.mx, bm.m3d, bm.pos),
                        });
                        Some(AnimateElement {
                            symbol_name: Some(si.symbol_name),
                            sprite_name: None,
                            bitmap_fallback,
                            matrix: parse_matrix(si.mx, si.m3d, None),
                            first_frame: si.first_frame,
                            last_frame: si.last_frame,
                            loop_mode: si.loop_mode,
                            color,
                        })
                    } else {
                        e.atlas_sprite_instance.map(|asi| AnimateElement {
                            symbol_name: None,
                            sprite_name: Some(asi.name),
                            bitmap_fallback: None,
                            matrix: parse_matrix(asi.mx, asi.m3d, asi.pos),
                            first_frame: 0,
                            last_frame: -1,
                            loop_mode: "LP".to_string(),
                            color: [1.0; 4],
                        })
                    }
                })
                .collect(),
        })
        .collect();
    AnimateTimelineLayer {
        name: raw.layer_name.unwrap_or_default(),
        visible,
        layer_type,
        clipped_by: raw.clipped_by,
        frames,
    }
}

pub(super) fn parse_stage_matrix(sti: Option<StageInstanceRaw>) -> Option<[f32; 6]> {
    let sti = sti?;
    if let Some(si) = sti.symbol_instance {
        return Some(parse_matrix(si.mx, si.m3d, None));
    }
    if sti.mx.is_some() || sti.m3d.is_some() {
        return Some(parse_matrix(sti.mx, sti.m3d, None));
    }
    None
}

fn parse_color_transform(color: Option<&ColorRaw>) -> [f32; 4] {
    let Some(c) = color else {
        return [1.0; 4];
    };
    let mode = c.mode.as_deref().unwrap_or("");
    match mode {
        "AD" | "Advanced" => [
            c.rm.unwrap_or(1.0),
            c.gm.unwrap_or(1.0),
            c.bm.unwrap_or(1.0),
            c.am.unwrap_or(1.0),
        ],
        "CA" | "Alpha" => [1.0, 1.0, 1.0, c.am.unwrap_or(1.0)],
        "CBRT" | "Brightness" => {
            let brt = c.brt.unwrap_or(0.0);
            if brt < 0.0 {
                let m = (1.0 + brt).clamp(0.0, 1.0);
                [m, m, m, 1.0]
            } else {
                // Positive brightness mixes toward white; preserve alpha and bright multiplier.
                [1.0; 4]
            }
        }
        "T" | "Tint" => {
            let tm = c.tm.unwrap_or(0.0).clamp(0.0, 1.0);
            let inv = 1.0 - tm;
            let [tr, tg, tb] = c
                .tc
                .as_deref()
                .map(parse_hex_rgb)
                .unwrap_or([1.0, 1.0, 1.0]);
            [inv + tr * tm, inv + tg * tm, inv + tb * tm, 1.0]
        }
        _ => [1.0; 4],
    }
}

fn parse_hex_rgb(raw: &str) -> [f32; 3] {
    let hex = raw
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if let Ok(v) = u32::from_str_radix(hex, 16) {
        let rgb = if hex.len() == 8 { v & 0x00FF_FFFF } else { v };
        return [
            ((rgb >> 16) & 0xFF) as f32 / 255.0,
            ((rgb >> 8) & 0xFF) as f32 / 255.0,
            (rgb & 0xFF) as f32 / 255.0,
        ];
    }
    [1.0, 1.0, 1.0]
}

fn parse_matrix(mx: Option<[f32; 6]>, m3d: Option<Matrix3dRaw>, pos: Option<PointRaw>) -> [f32; 6] {
    if let Some(m) = mx {
        return m;
    }
    if let Some(m3d) = m3d {
        let m = m3d.into_array();
        let has_persp = m[3] != 0.0 || m[7] != 0.0 || m[11] != 0.0 || m[15] != 1.0;
        if !has_persp {
            return [m[0], m[1], m[4], m[5], m[12], m[13]];
        }
        let p = [(0.0_f32, 0.0_f32), (1.0, 0.0), (0.0, 1.0)].map(|(x, y)| {
            let z = m[3] * x + m[7] * y + m[15];
            let inv_z = if z.abs() > 1e-6 { 1.0 / z } else { 1.0 };
            ((m[0] * x + m[4] * y + m[12]) * inv_z, (m[1] * x + m[5] * y + m[13]) * inv_z)
        });
        return [
            p[1].0 - p[0].0,
            p[1].1 - p[0].1,
            p[2].0 - p[0].0,
            p[2].1 - p[0].1,
            p[0].0,
            p[0].1,
        ];
    }
    if let Some(p) = pos {
        return [1.0, 0.0, 0.0, 1.0, p.x, p.y];
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
    #[serde(rename = "FRT", alias = "framerate", default = "default_fps")]
    pub framerate: f32,
}

fn default_fps() -> f32 {
    24.0
}

#[derive(Deserialize)]
pub(super) struct AnimationHeaderRaw {
    #[serde(rename = "N", alias = "name")]
    pub name: Option<String>,
    #[serde(rename = "SN", alias = "SYMBOL_name")]
    pub symbol_name: Option<String>,
    #[serde(rename = "TL", alias = "TIMELINE")]
    pub timeline: TimelineRaw,
    #[serde(rename = "STI", alias = "StageInstance")]
    pub stage_instance: Option<StageInstanceRaw>,
}

#[derive(Deserialize)]
pub(super) struct StageInstanceRaw {
    #[serde(rename = "SI", alias = "SYMBOL_Instance")]
    pub symbol_instance: Option<SymbolInstanceRaw>,
    #[serde(rename = "MX", alias = "Matrix")]
    pub mx: Option<[f32; 6]>,
    #[serde(rename = "M3D", alias = "Matrix3D")]
    pub m3d: Option<Matrix3dRaw>,
}

#[derive(Deserialize)]
pub(super) struct SymbolDictionaryRaw {
    #[serde(rename = "Symbols", alias = "S", default)]
    pub symbols: Vec<SymbolEntryRaw>,
}

#[derive(Deserialize)]
pub(super) struct SymbolEntryRaw {
    #[serde(rename = "SN", alias = "SYMBOL_name")]
    pub symbol_name: String,
    #[serde(rename = "TL", alias = "TIMELINE")]
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
    #[serde(rename = "LT", alias = "Layer_type")]
    pub layer_type: Option<String>,
    #[serde(rename = "Clpb", alias = "Clipped_by")]
    pub clipped_by: Option<String>,
    #[serde(rename = "FR", alias = "Frames", default)]
    pub frames: Vec<FrameRaw>,
}

#[derive(Deserialize)]
pub(super) struct FrameRaw {
    #[serde(rename = "I", alias = "index", default)]
    pub index: usize,
    #[serde(rename = "DU", alias = "duration", default)]
    pub duration: usize,
    #[serde(rename = "N", alias = "name")]
    pub name: Option<String>,
    #[serde(rename = "C", alias = "color")]
    pub color: Option<ColorRaw>,
    #[serde(rename = "E", alias = "elements", default)]
    pub elements: Vec<ElementRaw>,
}

#[derive(Deserialize)]
pub(super) struct ElementRaw {
    #[serde(rename = "SI", alias = "SYMBOL_Instance")]
    pub symbol_instance: Option<SymbolInstanceRaw>,
    #[serde(rename = "ASI", alias = "ATLAS_SPRITE_instance")]
    pub atlas_sprite_instance: Option<AtlasSpriteInstanceRaw>,
}

#[derive(Deserialize)]
pub(super) struct SymbolInstanceRaw {
    #[serde(rename = "SN", alias = "SYMBOL_name")]
    pub symbol_name: String,
    #[serde(rename = "MX", alias = "Matrix")]
    pub mx: Option<[f32; 6]>,
    #[serde(rename = "M3D", alias = "Matrix3D")]
    pub m3d: Option<Matrix3dRaw>,
    #[serde(rename = "FF", alias = "firstFrame", default)]
    pub first_frame: usize,
    #[serde(rename = "LF", alias = "lastFrame", default = "default_last_frame")]
    pub last_frame: isize,
    #[serde(rename = "LP", alias = "loop", default = "default_loop_mode")]
    pub loop_mode: String,
    #[serde(rename = "C", alias = "color")]
    pub color: Option<ColorRaw>,
    #[serde(rename = "BM", alias = "bitmap")]
    pub bitmap: Option<AtlasSpriteInstanceRaw>,
}

fn default_last_frame() -> isize {
    -1
}

fn default_loop_mode() -> String {
    "LP".to_string()
}

#[derive(Deserialize)]
pub(super) struct AtlasSpriteInstanceRaw {
    #[serde(rename = "N", alias = "name")]
    pub name: String,
    #[serde(rename = "MX", alias = "Matrix")]
    pub mx: Option<[f32; 6]>,
    #[serde(rename = "M3D", alias = "Matrix3D")]
    pub m3d: Option<Matrix3dRaw>,
    #[serde(rename = "POS", alias = "Position")]
    pub pos: Option<PointRaw>,
}

#[derive(Deserialize)]
pub(super) struct PointRaw {
    pub x: f32,
    pub y: f32,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum Matrix3dRaw {
    Array([f32; 16]),
    Object {
        m00: f32, m01: f32, m02: f32, m03: f32,
        m10: f32, m11: f32, m12: f32, m13: f32,
        m20: f32, m21: f32, m22: f32, m23: f32,
        m30: f32, m31: f32, m32: f32, m33: f32,
    },
}

impl Matrix3dRaw {
    pub(super) fn into_array(self) -> [f32; 16] {
        match self {
            Self::Array(arr) => arr,
            Self::Object {
                m00, m01, m02, m03,
                m10, m11, m12, m13,
                m20, m21, m22, m23,
                m30, m31, m32, m33,
            } => [
                m00, m01, m02, m03,
                m10, m11, m12, m13,
                m20, m21, m22, m23,
                m30, m31, m32, m33,
            ],
        }
    }
}

#[derive(Deserialize)]
pub(super) struct ColorRaw {
    #[serde(rename = "M", alias = "mode")]
    pub mode: Option<String>,
    #[serde(rename = "RM", alias = "RedMultiplier")]
    pub rm: Option<f32>,
    #[serde(rename = "GM", alias = "greenMultiplier")]
    pub gm: Option<f32>,
    #[serde(rename = "BM", alias = "blueMultiplier")]
    pub bm: Option<f32>,
    #[serde(rename = "AM", alias = "alphaMultiplier")]
    pub am: Option<f32>,
    #[serde(rename = "TC", alias = "tintColor")]
    pub tc: Option<String>,
    #[serde(rename = "TM", alias = "tintMultiplier")]
    pub tm: Option<f32>,
    #[serde(rename = "BRT", alias = "brightness")]
    pub brt: Option<f32>,
}

