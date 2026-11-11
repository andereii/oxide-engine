//! 6E dialogue load surface (`docs/M6-CONTRACT.md` §14): conversations,
//! boxes and speakers for the shipped week-6 family, parsed through the
//! `oxide_data::dialogue` layer and resolved through the same VFS candidate
//! order as the 6B loader.
//!
//! The shipped conversations all live in the `week6` library, so asset keys
//! without an explicit prefix resolve `week6/images/<key>` first, then
//! `shared`, then `preload` (the observed `Paths.getPath` order).
//!
//! The frozen render-fixture path never calls [`load_dialogue`] (the
//! `load_slice_for_song` entry passes `None`), so the established
//! bopeebo/school frame digests stay byte-identical.
//!
//! The scene-integration slice (M6-CONTRACT §14.2) consumes these types;
//! temporary module-level dead-code allow until it lands.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::Path;

use oxide_data::dialogue::{ConversationData, DialogueBoxData, SpeakerData};
use oxide_vfs::Vfs;

use super::RenderError;
use super::hud::IconFrame;
use super::load::{ASSET_SIZE_LIMIT, PageStore, image_file_bases};

/// One parsed dialogue sprite sheet: page rectangles plus the named clips
/// built by the native `addAtlasAnimations` prefix rule (frame NAMES that
/// START WITH the clip prefix, document order; `FlxAnimationUtil`).
/// Consumed by the scene-integration slice (M6-CONTRACT §14.2); temporary
/// dead-code allows until that lands.
pub(crate) struct DialogueSheet {
    pub(crate) page_size: [f64; 2],
    pub(crate) frames: Vec<IconFrame>,
    pub(crate) clips: BTreeMap<String, DialogueClip>,
}

/// One named dialogue animation (fps from the data; `looped` from the data's
/// `looped` key when present, otherwise false like the native default).
pub(crate) struct DialogueClip {
    pub(crate) frames: Vec<usize>,
    pub(crate) fps: f64,
    pub(crate) looped: bool,
}

impl DialogueSheet {
    /// Builds the sheet from a parsed Sparrow XML atlas plus the animation
    /// definitions in the dialogue document.
    pub(crate) fn from_atlas(
        page_size: [f64; 2],
        atlas: &oxide_atlas::Atlas,
        animations: &[oxide_data::character::AnimationData],
    ) -> Result<Self, RenderError> {
        let mut frames = Vec::with_capacity(atlas.subtextures.len());
        for texture in &atlas.subtextures {
            if texture.rotated {
                return Err(RenderError::Unsupported {
                    detail: format!(
                        "dialogue sheet item {:?} is rotated; the 6E dialogue pipeline \
                         supports unrotated Sparrow frames only ([D])",
                        texture.name
                    ),
                });
            }
            frames.push(IconFrame {
                rect: [texture.x, texture.y, texture.width, texture.height],
                size: [texture.width, texture.height],
            });
        }
        let mut clips = BTreeMap::new();
        for animation in animations {
            let indices: Vec<usize> = atlas
                .subtextures
                .iter()
                .enumerate()
                .filter(|(_, texture)| {
                    animation
                        .prefix
                        .as_deref()
                        .is_some_and(|prefix| texture.name.starts_with(prefix))
                })
                .map(|(index, _)| index)
                .collect();
            clips.insert(
                animation.name.clone(),
                DialogueClip {
                    frames: indices,
                    fps: animation.frame_rate.unwrap_or(24.0),
                    looped: animation.looped.unwrap_or(false),
                },
            );
        }
        Ok(Self {
            page_size,
            frames,
            clips,
        })
    }
}

/// One loaded dialogue box: data + dual sampler pages + the parsed sheet.
pub(crate) struct BoxAsset {
    pub(crate) data: DialogueBoxData,
    pub(crate) linear_page: usize,
    pub(crate) pixel_page: usize,
    pub(crate) page_size: [f64; 2],
    pub(crate) sheet: DialogueSheet,
}

/// One loaded speaker: data + dual sampler pages + the parsed sheet.
pub(crate) struct SpeakerAsset {
    pub(crate) data: SpeakerData,
    pub(crate) linear_page: usize,
    pub(crate) pixel_page: usize,
    pub(crate) page_size: [f64; 2],
    pub(crate) sheet: DialogueSheet,
}

/// The loaded dialogue payload for one conversation (`LoadedSlice.dialogue`).
pub(crate) struct LoadedDialogue {
    pub(crate) conversation_id: String,
    pub(crate) conversation: ConversationData,
    pub(crate) boxes: BTreeMap<String, BoxAsset>,
    pub(crate) speakers: BTreeMap<String, SpeakerAsset>,
    /// The `pixel.otf` bytes (family "Pixel Arial 11 Bold"); the per-line
    /// glyph rasters are built at draw time from these bytes (deterministic,
    /// pinned by the conversation-core test).
    pub(crate) font_bytes: Vec<u8>,
}

/// Resolves the sparrow XML + PNG page pair for one dialogue asset key.
///
/// The XML is the animation source; the PNG is the same key's page. Both use
/// the `image_file_bases` candidate order with the fixed `week6` level
/// directory (every shipped dialogue asset lives there).
fn resolve_sheet_sources(
    vfs: &Vfs,
    key: &str,
) -> Result<(String, Vec<u8>, String, Vec<u8>), RenderError> {
    let bases = image_file_bases(key, Some("week6"));
    let xml_candidates: Vec<String> = bases.iter().map(|base| format!("{base}.xml")).collect();
    let png_candidates: Vec<String> = bases.iter().map(|base| format!("{base}.png")).collect();
    let read_first = |paths: &[String]| -> Result<Option<(String, Vec<u8>)>, RenderError> {
        for path in paths {
            if let Some(source) = vfs.resolve(path) {
                let bytes =
                    vfs.read(&source, ASSET_SIZE_LIMIT)
                        .map_err(|source| RenderError::Read {
                            path: path.clone(),
                            source,
                        })?;
                return Ok(Some((path.clone(), bytes)));
            }
        }
        Ok(None)
    };
    let xml = read_first(&xml_candidates)?.ok_or_else(|| RenderError::MissingAsset {
        path: xml_candidates[0].clone(),
    })?;
    let png = read_first(&png_candidates)?.ok_or_else(|| RenderError::MissingAsset {
        path: png_candidates[0].clone(),
    })?;
    Ok((xml.0, xml.1, png.0, png.1))
}

fn parse_sparrow_sheet(path: &str, bytes: &[u8]) -> Result<oxide_atlas::Atlas, RenderError> {
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| RenderError::Decode {
        path: path.to_string(),
        detail: error.to_string(),
    })?;
    oxide_atlas::parse_sparrow(&text).map_err(|source| RenderError::Unsupported {
        detail: format!("dialogue sheet {path:?} failed to parse: {source}"),
    })
}

fn parse_dialogue_document(
    path: &str,
    bytes: &[u8],
) -> Result<oxide_data::dialogue::DialogueDocument, RenderError> {
    oxide_data::dialogue::parse_dialogue_file(Path::new(path), bytes).map_err(|source| {
        RenderError::Unsupported {
            detail: format!("dialogue document {path:?} failed to parse: {source}"),
        }
    })
}

/// Loads the conversation `id` with every referenced box and speaker.
pub(crate) fn load_dialogue(
    vfs: &Vfs,
    pages: &mut PageStore,
    id: &str,
) -> Result<LoadedDialogue, RenderError> {
    let conversation_path = format!("preload/data/dialogue/conversations/{id}.json");
    let source = vfs
        .resolve(&conversation_path)
        .ok_or_else(|| RenderError::MissingAsset {
            path: conversation_path.clone(),
        })?;
    let bytes = vfs
        .read(&source, ASSET_SIZE_LIMIT)
        .map_err(|source| RenderError::Read {
            path: conversation_path.clone(),
            source,
        })?;
    let document = parse_dialogue_document(&conversation_path, &bytes)?;
    let oxide_data::dialogue::DialogueTyped::Conversation(conversation) = document.typed else {
        return Err(RenderError::Unsupported {
            detail: format!("dialogue document {conversation_path:?} is not a conversation"),
        });
    };
    let conversation = *conversation;

    let mut box_ids: Vec<String> = Vec::new();
    let mut speaker_ids: Vec<String> = Vec::new();
    for entry in &conversation.dialogue {
        if !box_ids.iter().any(|existing| existing == &entry.box_id) {
            box_ids.push(entry.box_id.clone());
        }
        if !speaker_ids
            .iter()
            .any(|existing| existing == &entry.speaker)
        {
            speaker_ids.push(entry.speaker.clone());
        }
    }

    let mut boxes = BTreeMap::new();
    for box_id in &box_ids {
        let data_path = format!("preload/data/dialogue/boxes/{box_id}.json");
        let source = vfs
            .resolve(&data_path)
            .ok_or_else(|| RenderError::MissingAsset {
                path: data_path.clone(),
            })?;
        let bytes = vfs
            .read(&source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: data_path.clone(),
                source,
            })?;
        let document = parse_dialogue_document(&data_path, &bytes)?;
        let oxide_data::dialogue::DialogueTyped::Box(box_data) = document.typed else {
            return Err(RenderError::Unsupported {
                detail: format!("dialogue document {data_path:?} is not a dialogue box"),
            });
        };
        let data = *box_data;
        let (xml_path, xml_bytes, png_path, _png_bytes) =
            resolve_sheet_sources(vfs, &data.asset_path)?;
        let atlas = parse_sparrow_sheet(&xml_path, &xml_bytes)?;
        let (linear_page, page_size) = pages.load_png_file(vfs, &png_path, false)?;
        let (pixel_page, pixel_size) = pages.load_png_file(vfs, &png_path, true)?;
        if page_size != pixel_size {
            return Err(RenderError::Unsupported {
                detail: format!("dialogue box page {png_path:?} decoded to two sizes"),
            });
        }
        let sheet = DialogueSheet::from_atlas(page_size, &atlas, &data.animations)?;
        boxes.insert(
            box_id.clone(),
            BoxAsset {
                data,
                linear_page,
                pixel_page,
                page_size,
                sheet,
            },
        );
    }

    let mut speakers = BTreeMap::new();
    for speaker_id in &speaker_ids {
        let data_path = format!("preload/data/dialogue/speakers/{speaker_id}.json");
        let source = vfs
            .resolve(&data_path)
            .ok_or_else(|| RenderError::MissingAsset {
                path: data_path.clone(),
            })?;
        let bytes = vfs
            .read(&source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: data_path.clone(),
                source,
            })?;
        let document = parse_dialogue_document(&data_path, &bytes)?;
        let oxide_data::dialogue::DialogueTyped::Speaker(speaker_data) = document.typed else {
            return Err(RenderError::Unsupported {
                detail: format!("dialogue document {data_path:?} is not a speaker"),
            });
        };
        let data = *speaker_data;
        let (xml_path, xml_bytes, png_path, _png_bytes) =
            resolve_sheet_sources(vfs, &data.asset_path)?;
        let atlas = parse_sparrow_sheet(&xml_path, &xml_bytes)?;
        let (linear_page, page_size) = pages.load_png_file(vfs, &png_path, false)?;
        let (pixel_page, pixel_size) = pages.load_png_file(vfs, &png_path, true)?;
        if page_size != pixel_size {
            return Err(RenderError::Unsupported {
                detail: format!("speaker page {png_path:?} decoded to two sizes"),
            });
        }
        let sheet = DialogueSheet::from_atlas(page_size, &atlas, &data.animations)?;
        speakers.insert(
            speaker_id.clone(),
            SpeakerAsset {
                data,
                linear_page,
                pixel_page,
                page_size,
                sheet,
            },
        );
    }

    let font_path = "fonts/pixel.otf";
    let font_source = vfs
        .resolve(font_path)
        .ok_or_else(|| RenderError::MissingAsset {
            path: font_path.to_string(),
        })?;
    let font_bytes =
        vfs.read(&font_source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: font_path.to_string(),
                source,
            })?;

    Ok(LoadedDialogue {
        conversation_id: id.to_string(),
        conversation,
        boxes,
        speakers,
        font_bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Asset-gated: the real senpai conversation load (data + box + speaker
    /// sheets + font) through the shipped tree.
    #[test]
    fn real_senpai_dialogue_loads() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let vfs = Vfs::new(std::path::PathBuf::from(&root));
        let mut pages = PageStore::default();
        let loaded = load_dialogue(&vfs, &mut pages, "senpai").expect("senpai dialogue loads");
        assert_eq!(loaded.conversation_id, "senpai");
        assert_eq!(loaded.conversation.dialogue.len(), 3);
        assert!(loaded.boxes.contains_key("roses"), "the roses box loads");
        assert!(loaded.speakers.contains_key("senpai"));
        assert!(loaded.speakers.contains_key("bf-pixel"));
        assert!(!loaded.font_bytes.is_empty());
        // The week6 library resolution found the shipped sheets.
        let roses = &loaded.boxes["roses"];
        assert!(roses.sheet.clips.contains_key("enter"));
        let senpai = &loaded.speakers["senpai"];
        assert!(senpai.sheet.clips.contains_key("talkEnter"));
    }
}
