use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::math::URect;
use bevy::prelude::{TextureAtlasLayout, UVec2};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct AnimateSpriteEntry {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub rotated: bool,
}

#[derive(Debug, Clone)]
pub struct AnimateElement {
    pub symbol_name: Option<String>,
    pub sprite_name: Option<String>,
    pub matrix: [f32; 16],
}

#[derive(Debug, Clone)]
pub struct AnimateTimelineFrame {
    pub index: usize,
    pub duration: usize,
    pub name: Option<String>,
    pub elements: Vec<AnimateElement>,
}

#[derive(Debug, Clone)]
pub struct AnimateTimelineLayer {
    pub name: String,
    pub frames: Vec<AnimateTimelineFrame>,
}

#[derive(Debug, Clone, Default)]
pub struct AnimateAtlas {
    pub name: String,
    pub framerate: f32,
    pub main_symbol: String,
    pub sprites: Vec<AnimateSpriteEntry>,
    pub main_layers: Vec<AnimateTimelineLayer>,
    pub symbols: HashMap<String, Vec<AnimateTimelineLayer>>,
}

impl AnimateAtlas {
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = dir.as_ref();
        let anim_path = dir.join("Animation.json");
        let spritemap_path = dir.join("spritemap1.json");

        let anim_raw = fs::read_to_string(&anim_path)
            .map_err(|e| format!("failed to read {}: {e}", anim_path.display()))?;
        let anim_json: AnimationJsonRoot = serde_json::from_str(&anim_raw)
            .map_err(|e| format!("failed to parse {}: {e}", anim_path.display()))?;

        let sprite_raw = fs::read_to_string(&spritemap_path)
            .map_err(|e| format!("failed to read {}: {e}", spritemap_path.display()))?;
        let sprite_json: SpritemapJsonRoot = serde_json::from_str(&sprite_raw)
            .map_err(|e| format!("failed to parse {}: {e}", spritemap_path.display()))?;

        let sprites = sprite_json
            .atlas
            .sprites
            .into_iter()
            .map(|entry| AnimateSpriteEntry {
                name: entry.sprite.name,
                x: entry.sprite.x,
                y: entry.sprite.y,
                w: entry.sprite.w,
                h: entry.sprite.h,
                rotated: entry.sprite.rotated,
            })
            .collect();

        let main_layers = anim_json
            .animation
            .timeline
            .layers
            .into_iter()
            .map(convert_layer)
            .collect();

        let mut symbols = HashMap::new();
        if let Some(sd) = anim_json.symbol_dictionary {
            for sym in sd.symbols {
                let layers = sym.timeline.layers.into_iter().map(convert_layer).collect();
                symbols.insert(sym.symbol_name, layers);
            }
        }

        Ok(Self {
            name: anim_json.animation.name.unwrap_or_default(),
            framerate: anim_json.metadata.map(|m| m.framerate).unwrap_or(24.0),
            main_symbol: anim_json.animation.symbol_name.unwrap_or_default(),
            sprites,
            main_layers,
            symbols,
        })
    }

    pub fn build_layout(&self) -> (TextureAtlasLayout, HashMap<String, usize>) {
        let mut max_x = 1u32;
        let mut max_y = 1u32;
        for s in &self.sprites {
            max_x = max_x.max(s.x + s.w);
            max_y = max_y.max(s.y + s.h);
        }
        let mut layout = TextureAtlasLayout::new_empty(UVec2::new(max_x, max_y));
        let mut indices = HashMap::with_capacity(self.sprites.len());
        for s in &self.sprites {
            let idx = layout.add_texture(URect::new(s.x, s.y, s.x + s.w, s.y + s.h));
            indices.insert(s.name.clone(), idx);
        }
        (layout, indices)
    }

    pub fn frame_labels(&self) -> HashMap<String, (usize, usize)> {
        let mut labels = HashMap::new();
        for layer in &self.main_layers {
            for frame in &layer.frames {
                if let Some(name) = &frame.name {
                    labels.insert(name.clone(), (frame.index, frame.duration));
                }
            }
        }
        labels
    }
}

fn convert_layer(raw: LayerRaw) -> AnimateTimelineLayer {
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
                            matrix: si.matrix.unwrap_or(IDENTITY_M3D),
                        })
                    } else {
                        e.atlas_sprite_instance.map(|asi| AnimateElement {
                            symbol_name: None,
                            sprite_name: Some(asi.name),
                            matrix: asi.matrix.unwrap_or(IDENTITY_M3D),
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

const IDENTITY_M3D: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

#[derive(Deserialize)]
struct SpritemapJsonRoot {
    #[serde(rename = "ATLAS")]
    atlas: SpritemapAtlasRaw,
}

#[derive(Deserialize)]
struct SpritemapAtlasRaw {
    #[serde(rename = "SPRITES")]
    sprites: Vec<SpriteWrapperRaw>,
}

#[derive(Deserialize)]
struct SpriteWrapperRaw {
    #[serde(rename = "SPRITE")]
    sprite: SpriteEntryRaw,
}

#[derive(Deserialize)]
struct SpriteEntryRaw {
    name: String,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    #[serde(default)]
    rotated: bool,
}

#[derive(Deserialize)]
struct AnimationJsonRoot {
    #[serde(rename = "ANIMATION")]
    animation: AnimationHeaderRaw,
    #[serde(rename = "SYMBOL_DICTIONARY")]
    symbol_dictionary: Option<SymbolDictionaryRaw>,
    #[serde(rename = "metadata")]
    metadata: Option<MetadataRaw>,
}

#[derive(Deserialize)]
struct MetadataRaw {
    #[serde(rename = "FRT", default = "default_fps")]
    framerate: f32,
}

fn default_fps() -> f32 {
    24.0
}

#[derive(Deserialize)]
struct AnimationHeaderRaw {
    #[serde(rename = "N")]
    name: Option<String>,
    #[serde(rename = "SN")]
    symbol_name: Option<String>,
    #[serde(rename = "TL")]
    timeline: TimelineRaw,
}

#[derive(Deserialize)]
struct SymbolDictionaryRaw {
    #[serde(rename = "Symbols", default)]
    symbols: Vec<SymbolEntryRaw>,
}

#[derive(Deserialize)]
struct SymbolEntryRaw {
    #[serde(rename = "SN")]
    symbol_name: String,
    #[serde(rename = "TL")]
    timeline: TimelineRaw,
}

#[derive(Deserialize)]
struct TimelineRaw {
    #[serde(rename = "L", default)]
    layers: Vec<LayerRaw>,
}

#[derive(Deserialize)]
struct LayerRaw {
    #[serde(rename = "Layer_name")]
    layer_name: Option<String>,
    #[serde(rename = "FR", default)]
    frames: Vec<FrameRaw>,
}

#[derive(Deserialize)]
struct FrameRaw {
    #[serde(rename = "I", default)]
    index: usize,
    #[serde(rename = "DU", default)]
    duration: usize,
    #[serde(rename = "N")]
    name: Option<String>,
    #[serde(rename = "E", default)]
    elements: Vec<ElementRaw>,
}

#[derive(Deserialize)]
struct ElementRaw {
    #[serde(rename = "SI")]
    symbol_instance: Option<SymbolInstanceRaw>,
    #[serde(rename = "ASI")]
    atlas_sprite_instance: Option<AtlasSpriteInstanceRaw>,
}

#[derive(Deserialize)]
struct SymbolInstanceRaw {
    #[serde(rename = "SN")]
    symbol_name: String,
    #[serde(rename = "M3D")]
    matrix: Option<[f32; 16]>,
}

#[derive(Deserialize)]
struct AtlasSpriteInstanceRaw {
    #[serde(rename = "N")]
    name: String,
    #[serde(rename = "M3D")]
    matrix: Option<[f32; 16]>,
}

