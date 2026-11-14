use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::math::URect;
use bevy::prelude::{TextureAtlasLayout, UVec2};

use super::animate_atlas_raw::{AnimationJsonRoot, SpritemapJsonRoot, convert_layer};

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
    pub matrix: [f32; 6],
    pub first_frame: usize,
    pub loop_mode: String,
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
#[allow(dead_code)]
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
        let anim_json: AnimationJsonRoot =
            serde_json::from_str(anim_raw.trim_start_matches('\u{feff}'))
                .map_err(|e| format!("failed to parse {}: {e}", anim_path.display()))?;

        let sprite_raw = fs::read_to_string(&spritemap_path)
            .map_err(|e| format!("failed to read {}: {e}", spritemap_path.display()))?;
        let sprite_json: SpritemapJsonRoot =
            serde_json::from_str(sprite_raw.trim_start_matches('\u{feff}'))
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

pub fn layers_duration(layers: &[AnimateTimelineLayer]) -> usize {
    layers
        .iter()
        .flat_map(|l| l.frames.iter())
        .map(|f| f.index + f.duration)
        .max()
        .unwrap_or(0)
}
