use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::math::URect;
use bevy::prelude::{TextureAtlasLayout, UVec2};

use super::animate_atlas_raw::{
    AnimationJsonRoot, SpritemapJsonRoot, convert_layer, parse_stage_matrix,
};

#[derive(Debug, Clone)]
pub struct AnimateSpriteEntry {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub rotated: bool,
    pub spritemap_idx: usize,
}

#[derive(Debug, Clone)]
pub struct AnimateBitmapFallback {
    pub sprite_name: String,
    pub matrix: [f32; 6],
}

#[derive(Debug, Clone)]
pub struct AnimateElement {
    pub symbol_name: Option<String>,
    pub sprite_name: Option<String>,
    pub bitmap_fallback: Option<AnimateBitmapFallback>,
    pub matrix: [f32; 6],
    pub first_frame: usize,
    pub last_frame: isize,
    pub loop_mode: String,
    pub color: [f32; 4],
}

#[derive(Debug, Clone)]
pub struct AnimateTimelineFrame {
    pub index: usize,
    pub duration: usize,
    pub name: Option<String>,
    pub color: [f32; 4],
    pub elements: Vec<AnimateElement>,
}

#[derive(Debug, Clone)]
pub struct AnimateTimelineLayer {
    pub name: String,
    pub visible: bool,
    pub frames: Vec<AnimateTimelineFrame>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct AnimateAtlas {
    pub name: String,
    pub framerate: f32,
    pub main_symbol: String,
    pub stage_matrix: Option<[f32; 6]>,
    pub spritemap_count: usize,
    pub sprites: Vec<AnimateSpriteEntry>,
    pub sprite_lookup: HashMap<String, usize>,
    pub main_layers: Vec<AnimateTimelineLayer>,
    pub symbols: HashMap<String, Vec<AnimateTimelineLayer>>,
}

impl AnimateAtlas {
    pub fn from_dir(dir: impl AsRef<Path>) -> Result<Self, String> {
        let dir = dir.as_ref();
        let anim_path = dir.join("Animation.json");

        let anim_raw = fs::read_to_string(&anim_path)
            .map_err(|e| format!("failed to read {}: {e}", anim_path.display()))?;
        let anim_json: AnimationJsonRoot =
            serde_json::from_str(anim_raw.trim_start_matches('\u{feff}'))
                .map_err(|e| format!("failed to parse {}: {e}", anim_path.display()))?;

        let mut sprites = Vec::new();
        let mut sprite_lookup = HashMap::new();
        let mut spritemap_count = 0usize;
        let mut idx = 1usize;
        loop {
            let spritemap_path = dir.join(format!("spritemap{idx}.json"));
            if !spritemap_path.is_file() {
                if idx == 1 {
                    return Err(format!("failed to find {}", spritemap_path.display()));
                }
                break;
            }
            let sprite_raw = fs::read_to_string(&spritemap_path)
                .map_err(|e| format!("failed to read {}: {e}", spritemap_path.display()))?;
            let sprite_json: SpritemapJsonRoot =
                serde_json::from_str(sprite_raw.trim_start_matches('\u{feff}'))
                    .map_err(|e| format!("failed to parse {}: {e}", spritemap_path.display()))?;

            let map_idx = spritemap_count;
            spritemap_count += 1;
            for entry in sprite_json.atlas.sprites {
                let entry_idx = sprites.len();
                sprite_lookup
                    .entry(entry.sprite.name.clone())
                    .or_insert(entry_idx);
                sprites.push(AnimateSpriteEntry {
                    name: entry.sprite.name,
                    x: entry.sprite.x,
                    y: entry.sprite.y,
                    w: entry.sprite.w,
                    h: entry.sprite.h,
                    rotated: entry.sprite.rotated,
                    spritemap_idx: map_idx,
                });
            }
            idx += 1;
        }

        let stage_matrix = parse_stage_matrix(anim_json.animation.stage_instance);
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
            stage_matrix,
            spritemap_count: spritemap_count.max(1),
            sprites,
            sprite_lookup,
            main_layers,
            symbols,
        })
    }

    pub fn build_layouts(&self) -> (Vec<TextureAtlasLayout>, HashMap<String, usize>) {
        let count = self.spritemap_count.max(1);
        let mut bounds = vec![(1u32, 1u32); count];
        for s in &self.sprites {
            let b = &mut bounds[s.spritemap_idx.min(count - 1)];
            b.0 = b.0.max(s.x + s.w);
            b.1 = b.1.max(s.y + s.h);
        }
        let mut layouts: Vec<TextureAtlasLayout> = bounds
            .into_iter()
            .map(|(w, h)| TextureAtlasLayout::new_empty(UVec2::new(w, h)))
            .collect();
        let mut indices = HashMap::with_capacity(self.sprites.len());
        for s in &self.sprites {
            let map_idx = s.spritemap_idx.min(layouts.len() - 1);
            let idx = layouts[map_idx].add_texture(URect::new(s.x, s.y, s.x + s.w, s.y + s.h));
            indices.insert(s.name.clone(), idx);
        }
        (layouts, indices)
    }

    #[allow(dead_code)]
    pub fn build_layout(&self) -> (TextureAtlasLayout, HashMap<String, usize>) {
        let (mut layouts, indices) = self.build_layouts();
        let first = if layouts.is_empty() {
            TextureAtlasLayout::new_empty(UVec2::new(1, 1))
        } else {
            layouts.remove(0)
        };
        (first, indices)
    }

    /// Matches `flixel-animate`'s `Timeline.findFrameLabelIndices`: trims trailing
    /// whitespace on frame labels and returns all tick indices from the first matching layer.
    pub fn find_frame_label_indices(&self, label: &str) -> Vec<usize> {
        let target = label.trim_end();
        let mut found = Vec::new();
        for layer in &self.main_layers {
            let mut matched = false;
            for frame in &layer.frames {
                if frame.name.as_deref().is_some_and(|n| n.trim_end() == target) {
                    matched = true;
                    found.extend(frame.index..frame.index + frame.duration);
                }
            }
            if matched {
                break;
            }
        }
        found
    }

    #[allow(dead_code)]
    pub fn frame_labels(&self) -> HashMap<String, (usize, usize)> {
        let mut labels = HashMap::new();
        for layer in &self.main_layers {
            for frame in &layer.frames {
                if let Some(name) = &frame.name {
                    labels.entry(name.trim_end().to_string()).or_insert((frame.index, frame.duration));
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
