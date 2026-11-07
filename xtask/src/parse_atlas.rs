//! `cargo xtask parse-atlas` — M2 gate A.
//!
//! Walks an assets directory and reports Sparrow XML, TexturePacker JSON and
//! Animate Atlas counts. The stdout block is frozen (docs/M2-CONTRACT.md §4);
//! per-file diagnostics go to stderr so the block stays machine-readable.
//! Exit code 0 requires candidates, no Sparrow/Packer/Animate failures, and
//! the measured reference inventory when `--corpus` is requested.

use std::path::{Path, PathBuf};

use oxide_atlas::{AtlasError, parse_animate, parse_packer, parse_sparrow};

/// Default assets root: the FNF reference implementation checkout.
const DEFAULT_ASSETS: &str = "~/Descargas/fnf/Funkin/assets";

/// Runs the gate; `false` for missing/unreadable or candidate-empty input, any
/// atlas failure, or an incomplete `--corpus` inventory.
pub fn run(rest: &[String]) -> bool {
    let corpus = rest.iter().any(|arg| arg == "--corpus");
    let dir = match rest.iter().position(|arg| arg == "--dir") {
        Some(index) => match rest.get(index + 1) {
            Some(value) if !value.is_empty() && !value.starts_with("--") => value.clone(),
            _ => {
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
    if let Err(error) = collect_files(&root, &mut files) {
        eprintln!("parse-atlas: {error}");
        return false;
    }
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
            // Valid JSON without `frames` belongs to oxide-data. A failed read
            // or malformed JSON cannot safely be ruled out as a candidate.
            match read_text(path) {
                Ok(text) => match looks_like_packer(&text) {
                    Ok(true) => match parse_packer(&text) {
                        Ok(_) => packer_ok += 1,
                        Err(err) => {
                            packer_failed += 1;
                            eprintln!("packer failed: {}: {err}", path.display());
                        }
                    },
                    Ok(false) => {}
                    Err(err) => {
                        packer_failed += 1;
                        eprintln!("packer candidate failed: {}: {err}", path.display());
                    }
                },
                Err(err) => {
                    packer_failed += 1;
                    eprintln!("packer candidate failed: {}: {err}", path.display());
                }
            }
        }
    }

    println!("xml visited: {xml_visited}");
    println!("atlas ok: {atlas_ok}");
    println!("atlas failed: {atlas_failed}");
    println!("non-atlas xml (skipped): {non_atlas}");
    println!("subtextures: {subtextures}");
    println!("with_frame_attrs: {trimmed}");
    println!("negative frameX/Y: {negative_frame}");
    println!("rotated true: {rotated_true}");
    println!("missing png refs: {missing_png_refs}");
    println!(
        "packer ok {packer_ok} failed {packer_failed} (none in assets; fixtures cover parser)"
    );
    println!("animate ok {animate_ok} failed {animate_failed} sprites {animate_sprites}");

    let nonempty = xml_visited + packer_ok + packer_failed + animate_ok + animate_failed > 0;
    if !nonempty {
        eprintln!("parse-atlas: no atlas candidates found: {}", root.display());
    }
    let mut inventory_ok = true;
    if corpus {
        for (category, actual, expected) in [
            ("XML visited", xml_visited, 266),
            ("Sparrow", atlas_ok, 205),
            ("non-atlas XML", non_atlas, 61),
            ("Animate", animate_ok, 88),
        ] {
            if actual != expected {
                eprintln!(
                    "parse-atlas: corpus inventory: expected {expected} {category}, found {actual}"
                );
                inventory_ok = false;
            }
        }
    }
    nonempty && inventory_ok && atlas_failed == 0 && packer_failed == 0 && animate_failed == 0
}

fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let text = String::from_utf8(bytes).map_err(|err| err.to_string())?;
    Ok(text.strip_prefix('\u{FEFF}').unwrap_or(&text).to_string())
}

fn looks_like_packer(text: &str) -> Result<bool, serde_json::Error> {
    let value: serde_json::Value = serde_json::from_str(text)?;
    Ok(value.get("frames").is_some())
}

fn image_ref_exists(xml_path: &Path, image_path: &str) -> bool {
    let parent = xml_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(image_path).exists()
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read directory {}: {error}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("cannot read entry in {}: {error}", dir.display()))?;
        let path = entry.path();
        let metadata = std::fs::metadata(&path)
            .map_err(|error| format!("cannot inspect {}: {error}", path.display()))?;
        if metadata.is_dir() {
            collect_files(&path, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    PathBuf::from(path)
}
