use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use bevy::prelude::Color;

use crate::mods::ModRegistry;
use crate::utils::constants::STAGES_ASSET_PATH;

fn clean_rel_input(raw: &str) -> String {
    let normalized = raw.trim().replace('\\', "/");
    let mut cleaned = normalized.as_str();
    loop {
        let next = cleaned.trim_start_matches("./").trim_start_matches('/');
        if next.len() == cleaned.len() {
            break;
        }
        cleaned = next;
    }
    if let Some(rest) = cleaned.strip_prefix("assets/") {
        cleaned = rest;
    }
    cleaned.to_string()
}

fn push_unique_norm(out: &mut Vec<String>, raw_rel: &str) {
    if let Ok(norm) = ModRegistry::normalize_rel_path(raw_rel)
        && !norm.is_empty()
        && !out.iter().any(|existing| existing == &norm)
    {
        out.push(norm);
    }
}

fn stage_rel_candidates(stage_id: &str, raw: &str) -> Vec<String> {
    let cleaned = clean_rel_input(raw);
    if cleaned.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    if cleaned.starts_with("gameplay/") || cleaned.starts_with("characters/") {
        push_unique_norm(&mut out, &cleaned);
        push_unique_norm(&mut out, &format!("{STAGES_ASSET_PATH}/{stage_id}/{cleaned}"));
    } else {
        push_unique_norm(&mut out, &format!("{STAGES_ASSET_PATH}/{stage_id}/{cleaned}"));
        push_unique_norm(&mut out, &format!("{STAGES_ASSET_PATH}/{cleaned}"));
        push_unique_norm(&mut out, &format!("stages/{stage_id}/{cleaned}"));
        push_unique_norm(&mut out, &format!("stages/{cleaned}"));
        push_unique_norm(&mut out, &cleaned);
    }
    out
}

pub fn stage_rel_candidates_for_watch(stage_id: &str, raw: &str) -> Vec<String> {
    stage_rel_candidates(stage_id, raw)
}

pub fn resolve_atlas_dir_in(mods: &ModRegistry, stage_id: &str, raw: &str) -> Option<PathBuf> {
    for rel in stage_rel_candidates(stage_id, raw) {
        if let Some(dir) = mods.resolve_dir(&rel)
            && dir.join("Animation.json").is_file()
        {
            return Some(dir);
        }
        let stem = Path::new(&rel).with_extension("").to_string_lossy().replace('\\', "/");
        if let Some(dir) = mods.resolve_dir(&stem)
            && dir.join("Animation.json").is_file()
        {
            return Some(dir);
        }
    }
    None
}

pub fn resolve_stage_image_in(mods: &ModRegistry, stage_id: &str, raw: &str) -> Option<PathBuf> {
    for rel in &stage_rel_candidates(stage_id, raw) {
        if let Some(found) = mods.resolve_with_extensions(rel, &["png", "jpg", "jpeg", "webp"]) {
            return Some(found);
        }
    }

    let stage_dirs = [mods.assets_dir.join(STAGES_ASSET_PATH).join(stage_id)];
    for stage_dir in stage_dirs {
        if let Some(found) = resolve_stage_image(&mods.assets_dir, &stage_dir, raw) {
            return Some(found);
        }
    }
    None
}

fn stage_path_candidates(assets_dir: &Path, stage_dir: &Path, raw: &str) -> Vec<PathBuf> {
    let cleaned = clean_rel_input(raw);
    if cleaned.is_empty() {
        return Vec::new();
    }
    let mut raw_rels = Vec::new();
    if let Ok(stage_rel) = stage_dir.strip_prefix(assets_dir) {
        let stage_rel_str = stage_rel.to_string_lossy().replace('\\', "/");
        if !stage_rel_str.is_empty() {
            raw_rels.push(format!("{stage_rel_str}/{cleaned}"));
        }
    }
    if cleaned.starts_with("gameplay/") || cleaned.starts_with("characters/") {
        raw_rels.push(cleaned.clone());
    } else {
        raw_rels.push(format!("{STAGES_ASSET_PATH}/{cleaned}"));
        raw_rels.push(cleaned);
    }

    let mut out = Vec::new();
    for rel in raw_rels {
        if let Ok(safe) = ModRegistry::normalize_rel_path(&rel) {
            let p = assets_dir.join(safe);
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

pub fn resolve_atlas_dir(assets_dir: &Path, stage_dir: &Path, raw: &str) -> Option<PathBuf> {
    let candidates = stage_path_candidates(assets_dir, stage_dir, raw);
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
    let roots = stage_path_candidates(assets_dir, stage_dir, raw);

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

