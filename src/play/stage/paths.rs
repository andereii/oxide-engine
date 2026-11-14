use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use bevy::prelude::Color;

pub fn resolve_atlas_dir(assets_dir: &Path, stage_dir: &Path, raw: &str) -> Option<PathBuf> {
    let cleaned = raw.trim().trim_start_matches("./").trim_start_matches('/');
    let candidates = if cleaned.starts_with("gameplay/") {
        vec![assets_dir.join(cleaned), stage_dir.join(cleaned)]
    } else {
        vec![stage_dir.join(cleaned)]
    };
    for c in candidates {
        if c.join("Animation.json").is_file() {
            return Some(c);
        }
        let stem = c.with_extension("");
        if stem.join("Animation.json").is_file() {
            return Some(stem);
        }
    }
    None
}

pub fn resolve_stage_image(assets_dir: &Path, stage_dir: &Path, raw: &str) -> Option<PathBuf> {
    let cleaned = raw.trim().trim_start_matches("./").trim_start_matches('/');
    let roots = if cleaned.starts_with("gameplay/") {
        vec![assets_dir.join(cleaned), stage_dir.join(cleaned)]
    } else {
        vec![stage_dir.join(cleaned)]
    };

    for candidate in &roots {
        if candidate.is_file() {
            return Some(candidate.clone());
        }
        if candidate.extension().is_none() {
            for ext in ["png", "jpg", "jpeg", "webp"] {
                let with_ext = candidate.with_extension(ext);
                if with_ext.is_file() {
                    return Some(with_ext);
                }
            }
        }
    }

    for candidate in &roots {
        let Some(parent) = candidate.parent() else {
            continue;
        };
        let Some(target_stem) = candidate.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let wanted = normalize_stem(target_stem);
        let Ok(entries) = fs::read_dir(parent) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_img = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp"));
            if is_img {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if normalize_stem(stem) == wanted {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

fn normalize_stem(s: &str) -> String {
    s.chars()
        .filter(|c| *c != '-' && *c != '_')
        .flat_map(|c| c.to_lowercase())
        .collect()
}

pub fn to_asset_rel(assets_dir: &Path, full_path: &Path) -> String {
    let rel = full_path.strip_prefix(assets_dir).unwrap_or(full_path);
    rel.to_string_lossy().replace('\\', "/")
}

pub fn parse_hex_color(raw: &str) -> Color {
    let hex = raw
        .trim()
        .trim_start_matches('#')
        .trim_start_matches("0x")
        .trim_start_matches("0X");
    if hex.len() == 6 {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            let r = ((v >> 16) & 0xFF) as f32 / 255.0;
            let g = ((v >> 8) & 0xFF) as f32 / 255.0;
            let b = (v & 0xFF) as f32 / 255.0;
            return Color::srgb(r, g, b);
        }
    }
    if hex.len() == 8 {
        if let Ok(v) = u32::from_str_radix(hex, 16) {
            let a = ((v >> 24) & 0xFF) as f32 / 255.0;
            let r = ((v >> 16) & 0xFF) as f32 / 255.0;
            let g = ((v >> 8) & 0xFF) as f32 / 255.0;
            let b = (v & 0xFF) as f32 / 255.0;
            return Color::srgba(r, g, b, a);
        }
    }
    Color::WHITE
}

pub fn read_image_dimensions(path: &Path) -> Option<[f32; 2]> {
    let mut file = fs::File::open(path).ok()?;
    let mut buf = [0u8; 32];
    let n = file.read(&mut buf).ok()?;
    if n >= 24 && &buf[0..8] == b"\x89PNG\r\n\x1a\n" {
        let w = u32::from_be_bytes([buf[16], buf[17], buf[18], buf[19]]);
        let h = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
        return Some([w as f32, h as f32]);
    }
    None
}

