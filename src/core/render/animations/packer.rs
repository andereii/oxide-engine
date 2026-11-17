use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::math::URect;
use bevy::prelude::{TextureAtlasLayout, UVec2};

use super::base::BaseAnimationData;
use super::controller::AnimationController;

#[derive(Debug, Clone)]
pub struct PackerFrame {
    pub name: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone, Default)]
pub struct PackerAtlas {
    pub frames: Vec<PackerFrame>,
}

impl PackerAtlas {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, String> {
        let raw = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("failed to read {}: {e}", path.as_ref().display()))?;
        Ok(Self::parse(&raw))
    }

    pub fn parse(raw: &str) -> Self {
        let mut frames = Vec::new();
        for line in raw.trim_start_matches('\u{feff}').lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Some((left, right)) = line.split_once('=') else {
                continue;
            };
            let name = left.trim();
            let nums: Vec<u32> = right
                .split_whitespace()
                .filter_map(|t| t.parse::<u32>().ok())
                .collect();
            if name.is_empty() || nums.len() < 4 || nums[2] == 0 || nums[3] == 0 {
                continue;
            }
            frames.push(PackerFrame {
                name: name.to_string(),
                x: nums[0],
                y: nums[1],
                w: nums[2],
                h: nums[3],
            });
        }
        Self { frames }
    }

    pub fn build_layout(&self) -> (TextureAtlasLayout, HashMap<String, usize>) {
        let mut max_x = 1u32;
        let mut max_y = 1u32;
        for f in &self.frames {
            max_x = max_x.max(f.x + f.w);
            max_y = max_y.max(f.y + f.h);
        }
        let mut layout = TextureAtlasLayout::new_empty(UVec2::new(max_x, max_y));
        let mut indices = HashMap::with_capacity(self.frames.len());
        for f in &self.frames {
            let idx = layout.add_texture(URect::new(f.x, f.y, f.x + f.w, f.y + f.h));
            indices.insert(f.name.clone(), idx);
        }
        (layout, indices)
    }

    pub fn find_prefix_indices(&self, indices: &HashMap<String, usize>, prefix: &str) -> Vec<usize> {
        let mut matched: Vec<(&PackerFrame, usize)> = self
            .frames
            .iter()
            .filter(|f| f.name.starts_with(prefix))
            .filter_map(|f| indices.get(&f.name).copied().map(|idx| (f, idx)))
            .collect();
        matched.sort_by(|a, b| compare_packer_names(&a.0.name, &b.0.name));
        matched.into_iter().map(|(_, idx)| idx).collect()
    }
}

pub fn add_packer_animation(
    atlas: &PackerAtlas,
    indices: &HashMap<String, usize>,
    controller: &mut AnimationController,
    anim: &BaseAnimationData,
) -> bool {
    if anim.name.is_empty() {
        return false;
    }
    let query = if anim.prefix.is_empty() { &anim.name } else { &anim.prefix };
    let mut frames = atlas.find_prefix_indices(indices, query);
    if frames.is_empty() {
        return false;
    }
    if !anim.frame_indices.is_empty() {
        frames = anim
            .frame_indices
            .iter()
            .filter_map(|&i| frames.get(i).copied())
            .collect();
    }
    if frames.is_empty() {
        return false;
    }
    let fps = if anim.frame_rate > 0.0 { anim.frame_rate } else { 24.0 };
    controller.add_anim(&anim.name, frames, fps, anim.looped);
    controller.set_offset(&anim.name, anim.offsets);
    controller.set_flip(&anim.name, anim.flip);
    true
}

fn compare_packer_names(a: &str, b: &str) -> std::cmp::Ordering {
    match (trailing_num(a), trailing_num(b)) {
        (Some(na), Some(nb)) => na.cmp(&nb).then_with(|| a.cmp(b)),
        _ => a.cmp(b),
    }
}

fn trailing_num(s: &str) -> Option<u32> {
    let idx = s.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    (idx < s.len()).then(|| s[idx..].parse::<u32>().ok()).flatten()
}

