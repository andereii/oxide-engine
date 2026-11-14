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
    pub rotated: bool,
}

impl SparrowFrame {
    /// Returns the `[cx, cy]` offset (in Bevy Y-up unscaled pixels) from the
    /// virtual `frame_width x frame_height` canvas center to the trimmed rect center.
    pub fn trim_center_offset(&self) -> [f32; 2] {
        let (unrot_w, unrot_h) = if self.rotated {
            (self.height as f32, self.width as f32)
        } else {
            (self.width as f32, self.height as f32)
        };
        let fw = (self.frame_width as f32).max(unrot_w);
        let fh = (self.frame_height as f32).max(unrot_h);
        let cx = (-self.frame_x as f32 + unrot_w * 0.5) - fw * 0.5;
        let cy = fh * 0.5 - (-self.frame_y as f32 + unrot_h * 0.5);
        [cx, cy]
    }
}

#[derive(Debug, Clone, Default)]
pub struct SparrowAtlas {
    pub image_path: String,
    pub width: u32,
    pub height: u32,
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
                atlas.width = extract_attr(trimmed, "width").and_then(|v| v.parse().ok()).unwrap_or(0);
                atlas.height = extract_attr(trimmed, "height").and_then(|v| v.parse().ok()).unwrap_or(0);
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
                let rotated = extract_attr(trimmed, "rotated").is_some_and(|v| v == "true");

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
                        rotated,
                    });
                }
            }
        }
        atlas
    }

    pub fn build_layout(&self) -> (TextureAtlasLayout, HashMap<String, usize>) {
        let mut max_x = self.width.max(1);
        let mut max_y = self.height.max(1);
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

    fn find_prefix_matched<'a>(
        &'a self,
        indices: &HashMap<String, usize>,
        prefix: &str,
    ) -> Vec<(&'a SparrowFrame, usize)> {
        let mut matched: Vec<(&SparrowFrame, usize)> = self
            .frames
            .iter()
            .filter(|f| f.name.starts_with(prefix))
            .filter_map(|f| indices.get(&f.name).copied().map(|idx| (f, idx)))
            .collect();
        if matched.is_empty() {
            let norm_prefix = collapse_spaces(prefix);
            matched = self
                .frames
                .iter()
                .filter(|f| collapse_spaces(&f.name).starts_with(&norm_prefix))
                .filter_map(|f| indices.get(&f.name).copied().map(|idx| (f, idx)))
                .collect();
        }
        matched.sort_by(|a, b| a.0.name.cmp(&b.0.name));
        matched
    }

    pub fn find_prefix_indices(
        &self,
        indices: &HashMap<String, usize>,
        prefix: &str,
    ) -> Vec<usize> {
        self.find_prefix_matched(indices, prefix)
            .into_iter()
            .map(|(_, idx)| idx)
            .collect()
    }

    pub fn find_prefix_frames(
        &self,
        indices: &HashMap<String, usize>,
        prefix: &str,
    ) -> (Vec<usize>, Vec<[f32; 2]>, Vec<bool>) {
        let matched = self.find_prefix_matched(indices, prefix);
        let mut frame_indices = Vec::with_capacity(matched.len());
        let mut offsets = Vec::with_capacity(matched.len());
        let mut rotated = Vec::with_capacity(matched.len());
        for (frame, idx) in matched {
            frame_indices.push(idx);
            offsets.push(frame.trim_center_offset());
            rotated.push(frame.rotated);
        }
        (frame_indices, offsets, rotated)
    }
}

fn collapse_spaces(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_space = false;
    for ch in input.chars() {
        if ch == ' ' {
            if !prev_space {
                out.push(ch);
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out
}

fn extract_attr(tag: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=\"");
    let start = tag.find(&pattern)? + pattern.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

