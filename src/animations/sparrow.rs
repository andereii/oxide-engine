use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::math::URect;
use bevy::prelude::{TextureAtlasLayout, UVec2};

#[derive(Debug, Clone)]
pub struct SparrowFrame {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub frame_x: i32,
    pub frame_y: i32,
    pub frame_width: u32,
    pub frame_height: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SparrowAtlas {
    pub image_path: String,
    pub frames: Vec<SparrowFrame>,
}

impl SparrowAtlas {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let raw = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("failed to read {}: {e}", path.as_ref().display()))?;
        Ok(Self::parse(&raw))
    }

    pub fn parse(xml: &str) -> Self {
        let mut atlas = Self::default();
        for tag in xml.split('<').skip(1) {
            let trimmed = tag.trim();
            if trimmed.starts_with("TextureAtlas") {
                if let Some(p) = extract_attr(trimmed, "imagePath") {
                    atlas.image_path = p;
                }
            } else if trimmed.starts_with("SubTexture") {
                let Some(name) = extract_attr(trimmed, "name") else {
                    continue;
                };
                let x = extract_attr(trimmed, "x").and_then(|v| v.parse().ok()).unwrap_or(0);
                let y = extract_attr(trimmed, "y").and_then(|v| v.parse().ok()).unwrap_or(0);
                let width = extract_attr(trimmed, "width").and_then(|v| v.parse().ok()).unwrap_or(0);
                let height = extract_attr(trimmed, "height").and_then(|v| v.parse().ok()).unwrap_or(0);
                let frame_x = extract_attr(trimmed, "frameX").and_then(|v| v.parse().ok()).unwrap_or(0);
                let frame_y = extract_attr(trimmed, "frameY").and_then(|v| v.parse().ok()).unwrap_or(0);
                let frame_width = extract_attr(trimmed, "frameWidth")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(width);
                let frame_height = extract_attr(trimmed, "frameHeight")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(height);

                if width > 0 && height > 0 {
                    atlas.frames.push(SparrowFrame {
                        name,
                        x,
                        y,
                        width,
                        height,
                        frame_x,
                        frame_y,
                        frame_width,
                        frame_height,
                    });
                }
            }
        }
        atlas
    }

    pub fn build_layout(&self) -> (TextureAtlasLayout, HashMap<String, usize>) {
        let mut max_x = 1u32;
        let mut max_y = 1u32;
        for f in &self.frames {
            max_x = max_x.max(f.x + f.width);
            max_y = max_y.max(f.y + f.height);
        }
        let mut layout = TextureAtlasLayout::new_empty(UVec2::new(max_x, max_y));
        let mut indices = HashMap::with_capacity(self.frames.len());
        for f in &self.frames {
            let idx = layout.add_texture(URect::new(f.x, f.y, f.x + f.width, f.y + f.height));
            indices.insert(f.name.clone(), idx);
        }
        (layout, indices)
    }

    pub fn find_prefix_indices(
        &self,
        indices: &HashMap<String, usize>,
        prefix: &str,
    ) -> Vec<usize> {
        let mut matched: Vec<(&str, usize)> = self
            .frames
            .iter()
            .filter(|f| f.name.starts_with(prefix))
            .filter_map(|f| indices.get(&f.name).copied().map(|idx| (f.name.as_str(), idx)))
            .collect();
        matched.sort_by(|a, b| a.0.cmp(b.0));
        matched.into_iter().map(|(_, idx)| idx).collect()
    }
}

fn extract_attr(tag: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

