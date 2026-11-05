//! `cargo xtask parse-atlas` — M2 gate A.
//!
//! Walks an assets directory and reports Sparrow XML, TexturePacker JSON and
//! Animate Atlas counts. The stdout block is frozen (docs/M2-CONTRACT.md §4);
//! per-file diagnostics go to stderr so the block stays machine-readable.
//! Exit code 0 only when no atlas or animate file failed.

use std::path::{Path, PathBuf};

use oxide_atlas::{AtlasError, parse_animate, parse_packer, parse_sparrow};

/// Default assets root: the FNF reference implementation checkout.
const DEFAULT_ASSETS: &str = "~/Descargas/fnf/Funkin/assets";

/// Runs the gate; `false` when any atlas or animate file failed (or the assets
/// directory is missing / unreadable).
pub fn run(rest: &[String]) -> bool {
    let dir = match rest.iter().position(|arg| arg == "--dir") {
        Some(index) => match rest.get(index + 1) {
            Some(value) => value.clone(),
            None => {
                eprintln!("parse-atlas: --dir requires a value");
                return false;
            }
        },
        None => DEFAULT_ASSETS.to_string(),
    };
    let root = expand_tilde(&dir);
    if !root.is_dir() {
        eprintln!("parse-atlas: directory not found: {}", root.display());
        return false;
    }

    let mut files = Vec::new();
    collect_files(&root, &mut files);
    files.sort();

    let mut xml_visited = 0u64;
    let mut atlas_ok = 0u64;
    let mut atlas_failed = 0u64;
    let mut non_atlas = 0u64;
    let mut subtextures = 0u64;
    let mut trimmed = 0u64;
    let mut negative_frame = 0u64;
    let mut rotated_true = 0u64;
    let mut missing_png_refs = 0u64;
    let mut packer_ok = 0u64;
    let mut packer_failed = 0u64;
    let mut animate_ok = 0u64;
    let mut animate_failed = 0u64;
    let mut animate_sprites = 0u64;

    for path in &files {
        let file_name = path.file_name().and_then(|name| name.to_str());
        let extension = path.extension().and_then(|ext| ext.to_str());

        if extension == Some("xml") {
            xml_visited += 1;
            match read_text(path) {
                Ok(text) => match parse_sparrow(&text) {
                    Ok(atlas) => {
                        atlas_ok += 1;
                        for subtexture in &atlas.subtextures {
                            subtextures += 1;
                            if subtexture.frame_x != 0
                                || subtexture.frame_y != 0
                                || subtexture.frame_width != 0
                                || subtexture.frame_height != 0
                            {
                                trimmed += 1;
                            }
                            if subtexture.frame_x < 0 || subtexture.frame_y < 0 {
                                negative_frame += 1;
                            }
                            if subtexture.rotated {
                                rotated_true += 1;
                            }
                        }
                        if !image_ref_exists(path, &atlas.image_path) {
                            missing_png_refs += 1;
                        }
                    }
                    Err(AtlasError::NotAnAtlas { .. }) => non_atlas += 1,
                    Err(err) => {
                        atlas_failed += 1;
                        eprintln!("atlas failed: {}: {err}", path.display());
                    }
                },
                Err(err) => {
                    atlas_failed += 1;
                    eprintln!("atlas failed: {}: {err}", path.display());
                }
            }
        } else if file_name == Some("spritemap1.json") {
            match read_text(path) {
                Ok(text) => match parse_animate(&text) {
                    Ok(atlas) => {
                        animate_ok += 1;
                        animate_sprites += atlas.sprites.len() as u64;
                    }
                    Err(err) => {
                        animate_failed += 1;
                        eprintln!("animate failed: {}: {err}", path.display());
                    }
                },
                Err(err) => {
                    animate_failed += 1;
                    eprintln!("animate failed: {}: {err}", path.display());
                }
            }
        } else if extension == Some("json") {
            // Only files whose shape has a `frames` key are TexturePacker
            // candidates; every other JSON belongs to oxide-data.
            match read_text(path) {
                Ok(text) if looks_like_packer(&text) => match parse_packer(&text) {
                    Ok(_) => packer_ok += 1,
                    Err(err) => {
                        packer_failed += 1;
                        eprintln!("packer failed: {}: {err}", path.display());
                    }
                },
                Ok(_) | Err(_) => {}
            }
        }
    }

    println!("xml visited: {xml_visited}");
    println!("atlas ok: {atlas_ok}");
    println!("atlas failed: {atlas_failed}");
    println!("non-atlas xml (skipped): {non_atlas}");
    println!("subtextures: {subtextures}");
    println!("trimmed (non-zero frame*): {trimmed}");
    println!("negative frameX/Y: {negative_frame}");
    println!("rotated true: {rotated_true}");
    println!("missing png refs: {missing_png_refs}");
    println!(
        "packer ok {packer_ok} failed {packer_failed} (none in assets; fixtures cover parser)"
    );
    println!("animate ok {animate_ok} failed {animate_failed} sprites {animate_sprites}");

    atlas_failed == 0 && animate_failed == 0
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let text = String::from_utf8(bytes).map_err(|err| err.to_string())?;
    Ok(text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_string())
}

fn looks_like_packer(text: &str) -> bool {
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    serde_json::from_str::<serde_json::Value>(text)
        .ok()
        .and_then(|value| value.get("frames").map(|_| ()))
        .is_some()
}

fn image_ref_exists(xml_path: &Path, image_path: &str) -> bool {
    let parent = xml_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(image_path).exists()
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}
