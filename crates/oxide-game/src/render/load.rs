//! Slice asset loading through `oxide-vfs` and `oxide-data`.
//!
//! The renderer never composes a filesystem path itself: every JSON/PNG read
//! goes through [`oxide_vfs::Vfs::resolve`] (exact relative paths) so mod layer
//! precedence and case-exact lookup stay owned by the VFS crates. The only
//! path convention here is the V-Slice asset-key prefix (`shared:` versus the
//! preload default), which is the observed input data, not filesystem search
//! logic.
//!
//! PNG decoding uses `bevy::image` (`Image::from_buffer`) with the enabled
//! `png` format feature; no extra image dependency is introduced.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use oxide_atlas::{AnimateStage, frame_quad, parse_animate, parse_sparrow};
use oxide_data::Typed;
use oxide_vfs::Vfs;

use super::RenderError;
use super::anim::{
    AnimationDef, DEFAULT_ANIMATION_FPS, DEFAULT_DANCE_EVERY, DEFAULT_SING_TIME_STEPS,
};

/// Upper bound for any single slice asset read, matching the M5 slice VFS
/// read contract. The largest shipped page in the slice is a few MiB.
const ASSET_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

const CHART_PATH: &str = "preload/data/songs/bopeebo/bopeebo-chart.json";
const METADATA_PATH: &str = "preload/data/songs/bopeebo/bopeebo-metadata.json";

/// The three slice roles, in the order used by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Player = 0,
    Opponent = 1,
    Girlfriend = 2,
}

impl Role {
    pub(crate) fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Role::Player => "player",
            Role::Opponent => "opponent",
            Role::Girlfriend => "girlfriend",
        }
    }
}

/// A decoded page ready to be inserted into Bevy's `Assets<Image>`.
pub(crate) struct PageImage {
    pub(crate) image: Image,
}

/// One static stage prop (mainStage's props have no animations).
#[derive(Debug, Clone)]
pub(crate) struct LoadedStageProp {
    pub(crate) z_index: f64,
    pub(crate) position: (f64, f64),
    pub(crate) scale: (f64, f64),
    pub(crate) scroll: (f64, f64),
    pub(crate) alpha: f64,
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
    pub(crate) page: usize,
    pub(crate) image_size: [f64; 2],
}

/// The stage slot fields consumed by the renderer.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoadedSlot {
    pub(crate) z_index: f64,
    pub(crate) position: (f64, f64),
    pub(crate) scale: f64,
    pub(crate) camera_offsets: (f64, f64),
    pub(crate) scroll: (f64, f64),
}

/// A parsed character plus its resolved main-atlas animation bindings.
pub(crate) struct LoadedCharacter {
    pub(crate) role: Role,
    pub(crate) id: String,
    pub(crate) stage: AnimateStage,
    pub(crate) bounds: [f64; 4],
    pub(crate) animations: BTreeMap<String, AnimationDef>,
    pub(crate) data_offsets: (f64, f64),
    pub(crate) camera_offsets: (f64, f64),
    pub(crate) sing_time_steps: f64,
    pub(crate) dance_every: f64,
    pub(crate) scale: f64,
    pub(crate) flip_x: bool,
    pub(crate) page: usize,
    pub(crate) page_size: [f64; 2],
    pub(crate) slot: LoadedSlot,
}

/// One frame from a Sparrow page (trimmed-frame geometry kept unwrapped).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpriteFrame {
    pub(crate) page: usize,
    /// `[x, y, width, height]` in the page.
    pub(crate) rect: [i32; 4],
    /// `(-frameX, -frameY)` inside the frame box.
    pub(crate) dest_offset: [i32; 2],
    /// `(frameWidth, frameHeight)` of the frame box.
    pub(crate) dest_size: [i32; 2],
}

/// Receptor animation frames per direction.
#[derive(Debug, Clone)]
pub(crate) struct ReceptorFrames {
    pub(crate) static_frames: Vec<SpriteFrame>,
    pub(crate) press_frames: Vec<SpriteFrame>,
    pub(crate) confirm_frames: Vec<SpriteFrame>,
    pub(crate) confirm_hold_frames: Vec<SpriteFrame>,
}

/// Note-style sprite data used by the strumline renderer.
#[derive(Debug, Clone)]
pub(crate) struct LoadedNoteStyle {
    pub(crate) note_scale: f64,
    pub(crate) note_frames: [SpriteFrame; 4],
    pub(crate) note_page_size: [f64; 2],
    /// The flixel hitbox source size of the frame current at the single
    /// setup-time `updateHitbox()` call: the sheet's first frame in document
    /// order (notes.xml `noteDown0001` -> 157x154). Doubles in game pixels.
    pub(crate) note_hitbox_dest: [f64; 2],
    pub(crate) receptor_scale: f64,
    pub(crate) receptor_offsets: (f64, f64),
    pub(crate) receptors: [ReceptorFrames; 4],
    pub(crate) receptor_page: usize,
    pub(crate) receptor_page_size: [f64; 2],
    /// Same setup-time hitbox rule for the strumline page (first frame in
    /// document order, noteStrumline.xml `confirmDown0001` -> 235x230).
    pub(crate) receptor_hitbox_dest: [f64; 2],
    pub(crate) hold_page: usize,
    /// `(hold page width / 8) * hold style zoom` (416/8*0.7 = 36.4 for funkin).
    pub(crate) hold_width: f64,
}

/// One chart note (both chart sides; player ids match the sim's note ids).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoadedNote {
    pub(crate) time: f64,
    pub(crate) direction: usize,
    pub(crate) sustain_ms: f64,
    pub(crate) opponent: bool,
    pub(crate) player_id: Option<u32>,
}

/// Everything the scene builder and headless harness need.
pub(crate) struct LoadedSlice {
    pub(crate) pages: Vec<PageImage>,
    pub(crate) stage_zoom: f64,
    pub(crate) props: Vec<LoadedStageProp>,
    pub(crate) characters: [LoadedCharacter; 3],
    pub(crate) note_style: LoadedNoteStyle,
    pub(crate) notes: Vec<LoadedNote>,
    pub(crate) scroll_speed: f64,
    pub(crate) conductor_changes: Vec<oxide_audio::conductor::TimeChange>,
    /// Metadata `playData.characters.playerVocals` (voice track ids).
    pub(crate) player_vocals: Vec<String>,
    /// Metadata `playData.characters.opponentVocals` (voice track ids).
    pub(crate) opponent_vocals: Vec<String>,
}

/// Loads and parses every slice asset for `difficulty`.
pub(crate) fn load_slice(assets_root: &Path, difficulty: &str) -> Result<LoadedSlice, RenderError> {
    if !matches!(difficulty, "easy" | "normal" | "hard") {
        return Err(RenderError::UnknownDifficulty(difficulty.to_string()));
    }

    let vfs = Vfs::new(assets_root.to_path_buf());
    let mut pages = PageStore::default();

    let chart_document = read_data_document(&vfs, CHART_PATH)?;
    let Typed::Chart(chart) = chart_document.typed else {
        return Err(RenderError::WrongKind {
            path: CHART_PATH.to_string(),
            expected: "chart",
            actual: chart_document.kind,
        });
    };
    let metadata_document = read_data_document(&vfs, METADATA_PATH)?;
    let Typed::Metadata(metadata) = metadata_document.typed else {
        return Err(RenderError::WrongKind {
            path: METADATA_PATH.to_string(),
            expected: "metadata",
            actual: metadata_document.kind,
        });
    };

    if let Some(play_data) = &metadata.play_data
        && !play_data.difficulties.is_empty()
        && !play_data
            .difficulties
            .iter()
            .any(|known| known == difficulty)
    {
        return Err(RenderError::UnknownDifficulty(difficulty.to_string()));
    }
    let chart_notes = chart
        .notes
        .get(difficulty)
        .ok_or_else(|| RenderError::UnknownDifficulty(difficulty.to_string()))?;

    let scroll_speed = chart
        .scroll_speed
        .as_ref()
        .and_then(|speeds| speeds.get(difficulty))
        .and_then(|value| value.as_f64())
        .ok_or_else(|| RenderError::MissingField {
            path: CHART_PATH.to_string(),
            field: "scrollSpeed",
        })?;
    if !scroll_speed.is_finite() || scroll_speed <= 0.0 {
        return Err(RenderError::Unsupported {
            detail: format!("chart scrollSpeed {scroll_speed} is not a positive number"),
        });
    }

    let mut notes = Vec::with_capacity(chart_notes.len());
    let mut player_id = 0u32;
    for (index, note) in chart_notes.iter().enumerate() {
        if !(0..=7).contains(&note.direction) {
            return Err(RenderError::Unsupported {
                detail: format!("chart note {index} has direction {}", note.direction),
            });
        }
        if note.time < 0.0 || !note.time.is_finite() {
            return Err(RenderError::Unsupported {
                detail: format!("chart note {index} has invalid time {}", note.time),
            });
        }
        let sustain_ms = note.sustain.unwrap_or(0.0);
        if sustain_ms < 0.0 || !sustain_ms.is_finite() {
            return Err(RenderError::Unsupported {
                detail: format!("chart note {index} has invalid sustain {sustain_ms}"),
            });
        }
        let opponent = note.direction >= 4;
        let direction = (note.direction % 4) as usize;
        let id = if opponent {
            None
        } else {
            let id = player_id;
            player_id += 1;
            Some(id)
        };
        notes.push(LoadedNote {
            time: note.time,
            direction,
            sustain_ms,
            opponent,
            player_id: id,
        });
    }

    let play_data = metadata
        .play_data
        .as_ref()
        .ok_or_else(|| RenderError::MissingField {
            path: METADATA_PATH.to_string(),
            field: "playData",
        })?;
    let characters = play_data
        .characters
        .as_ref()
        .ok_or_else(|| RenderError::MissingField {
            path: METADATA_PATH.to_string(),
            field: "playData.characters",
        })?;
    let player_id_name = characters
        .player
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: METADATA_PATH.to_string(),
            field: "playData.characters.player",
        })?;
    let opponent_id_name =
        characters
            .opponent
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: METADATA_PATH.to_string(),
                field: "playData.characters.opponent",
            })?;
    let girlfriend_id_name =
        characters
            .girlfriend
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: METADATA_PATH.to_string(),
                field: "playData.characters.girlfriend",
            })?;
    let player_vocals = characters.player_vocals.clone().unwrap_or_default();
    let opponent_vocals = characters.opponent_vocals.clone().unwrap_or_default();
    let stage_name = play_data
        .stage
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: METADATA_PATH.to_string(),
            field: "playData.stage",
        })?;
    let note_style_name =
        play_data
            .note_style
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: METADATA_PATH.to_string(),
                field: "playData.noteStyle",
            })?;

    let stage_path = format!("preload/data/stages/{stage_name}.json");
    let stage_document = read_data_document(&vfs, &stage_path)?;
    let Typed::Stage(stage) = stage_document.typed else {
        return Err(RenderError::WrongKind {
            path: stage_path,
            expected: "stage",
            actual: stage_document.kind,
        });
    };
    let stage_zoom = stage
        .camera_zoom
        .unwrap_or(super::geometry::DEFAULT_STAGE_ZOOM);
    if stage.characters.is_none() {
        return Err(RenderError::MissingField {
            path: stage_path.clone(),
            field: "characters",
        });
    }

    let mut props = Vec::with_capacity(stage.props.len());
    for prop in &stage.props {
        let name = prop.name.clone();
        let animated = prop
            .animations
            .as_ref()
            .is_some_and(|list| !list.is_empty());
        if animated || prop.dance_every.is_some_and(|every| every != 0.0) {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "stage prop {name:?} is a Bopper/animated prop; phase 2b renders the static mainStage props only"
                ),
            });
        }
        let asset_path = prop
            .asset_path
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: stage_path.clone(),
                field: "props.assetPath",
            })?;
        // Stage.hx:250-251 sets `antialiasing = !isPixel`; mainStage's three
        // props have `isPixel: false`, so their page uses linear filtering.
        let (page, image_size) = pages.load_image_key(&vfs, &asset_path, ImageSampler::linear())?;
        props.push(LoadedStageProp {
            z_index: prop.z_index.unwrap_or(0.0),
            position: prop.position.unwrap_or((0.0, 0.0)),
            scale: prop.scale.unwrap_or((1.0, 1.0)),
            scroll: prop.scroll.unwrap_or((1.0, 1.0)),
            alpha: prop.alpha.unwrap_or(1.0),
            flip_x: prop.flip_x.unwrap_or(false),
            flip_y: prop.flip_y.unwrap_or(false),
            page,
            image_size,
        });
    }

    let player_slot = parse_stage_slot(&stage, &player_id_name, &stage_path)?;
    let opponent_slot = parse_stage_slot(&stage, &opponent_id_name, &stage_path)?;
    let girlfriend_slot = parse_stage_slot(&stage, &girlfriend_id_name, &stage_path)?;

    let player = load_character(&vfs, &mut pages, Role::Player, &player_id_name, player_slot)?;
    let opponent = load_character(
        &vfs,
        &mut pages,
        Role::Opponent,
        &opponent_id_name,
        opponent_slot,
    )?;
    let girlfriend = load_character(
        &vfs,
        &mut pages,
        Role::Girlfriend,
        &girlfriend_id_name,
        girlfriend_slot,
    )?;
    let characters = [player, opponent, girlfriend];

    let note_style_path = format!("preload/data/notestyles/{note_style_name}.json");
    let style_document = read_data_document(&vfs, &note_style_path)?;
    let Typed::NoteStyle(style) = style_document.typed else {
        return Err(RenderError::WrongKind {
            path: note_style_path,
            expected: "notestyle",
            actual: style_document.kind,
        });
    };
    let style_asset = note_style_assets(&style, &note_style_path)?;
    let note_style = load_note_style(&vfs, &mut pages, style_asset, &note_style_path)?;

    let conductor_changes = metadata
        .time_changes
        .as_ref()
        .map(|changes| {
            changes
                .iter()
                .map(|change| {
                    if !change.time.is_finite() || !change.bpm.is_finite() || change.bpm <= 0.0 {
                        return Err(RenderError::Unsupported {
                            detail: format!(
                                "metadata time change at {} ms has bpm {}",
                                change.time, change.bpm
                            ),
                        });
                    }
                    Ok(oxide_audio::conductor::TimeChange {
                        time_stamp: change.time,
                        beat_time: 0.0,
                        bpm: change.bpm,
                        time_signature_num: change.time_signature_num.unwrap_or(4) as i32,
                        time_signature_den: change.time_signature_den.unwrap_or(4) as i32,
                    })
                })
                .collect::<Result<Vec<_>, RenderError>>()
        })
        .transpose()?
        .unwrap_or_default();

    Ok(LoadedSlice {
        pages: pages.pages,
        stage_zoom,
        props,
        characters,
        note_style,
        notes,
        scroll_speed,
        conductor_changes,
        player_vocals,
        opponent_vocals,
    })
}

fn read_data_document(vfs: &Vfs, path: &str) -> Result<oxide_data::ParsedDocument, RenderError> {
    let bytes = read_asset(vfs, path)?;
    oxide_data::parse_file(Path::new(path), &bytes).map_err(|source| RenderError::Parse {
        path: path.to_string(),
        source,
    })
}

fn read_asset(vfs: &Vfs, path: &str) -> Result<Vec<u8>, RenderError> {
    let source = vfs.resolve(path).ok_or_else(|| RenderError::MissingAsset {
        path: path.to_string(),
    })?;
    vfs.read(&source, ASSET_SIZE_LIMIT)
        .map_err(|source| RenderError::Read {
            path: path.to_string(),
            source,
        })
}

/// Resolves `file` inside the first existing candidate directory.
fn resolve_in_dirs(
    vfs: &Vfs,
    dirs: &[String],
    file: &str,
) -> Result<(String, Vec<u8>), RenderError> {
    for dir in dirs {
        let path = format!("{dir}/{file}");
        if let Some(source) = vfs.resolve(&path) {
            let bytes =
                vfs.read(&source, ASSET_SIZE_LIMIT)
                    .map_err(|source| RenderError::Read {
                        path: path.clone(),
                        source,
                    })?;
            return Ok((path, bytes));
        }
    }
    Err(RenderError::MissingAsset {
        path: format!("{}/{}", dirs.first().map_or("<none>", String::as_str), file),
    })
}

fn read_text_in_dirs(
    vfs: &Vfs,
    dirs: &[String],
    file: &str,
) -> Result<(String, String), RenderError> {
    let (path, bytes) = resolve_in_dirs(vfs, dirs, file)?;
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
    let text = String::from_utf8(bytes.to_vec()).map_err(|error| RenderError::Decode {
        path: path.clone(),
        detail: error.to_string(),
    })?;
    Ok((path, text))
}

/// V-Slice single-image key candidates (without extension): an explicit
/// `shared:` maps to the shared asset tree; otherwise the preload tree is
/// tried before shared (the shipped NOTE_hold_assets page exists identically
/// in both).
fn image_file_bases(key: &str) -> Vec<String> {
    if let Some(rest) = key.strip_prefix("shared:") {
        vec![
            format!("shared/images/{rest}"),
            format!("preload/images/{rest}"),
        ]
    } else {
        vec![
            format!("preload/images/{key}"),
            format!("shared/images/{key}"),
        ]
    }
}

/// V-Slice directory candidates for multi-file atlas assets (Animation.json,
/// spritemap files, Sparrow XML/PNG siblings).
fn image_dir_candidates(key: &str) -> Vec<String> {
    image_file_bases(key)
}

fn parent_dir(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(parent, _)| parent)
}

#[derive(Default)]
struct PageStore {
    pages: Vec<PageImage>,
    indices: HashMap<String, usize>,
}

impl PageStore {
    fn load_image_key(
        &mut self,
        vfs: &Vfs,
        key: &str,
        sampler: ImageSampler,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        let bases = image_file_bases(key);
        for base in &bases {
            let path = format!("{base}.png");
            if let Some(source) = vfs.resolve(&path) {
                let bytes =
                    vfs.read(&source, ASSET_SIZE_LIMIT)
                        .map_err(|source| RenderError::Read {
                            path: path.clone(),
                            source,
                        })?;
                return self.add_png(path, bytes, sampler);
            }
        }
        Err(RenderError::MissingAsset {
            path: format!("{}.png", bases.first().map_or("<none>", String::as_str)),
        })
    }

    fn load_png(
        &mut self,
        vfs: &Vfs,
        dirs: &[String],
        file: &str,
        sampler: ImageSampler,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        let (path, bytes) = resolve_in_dirs(vfs, dirs, file)?;
        self.add_png(path, bytes, sampler)
    }

    fn load_png_file(
        &mut self,
        vfs: &Vfs,
        path: &str,
        sampler: ImageSampler,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        let source = vfs.resolve(path).ok_or_else(|| RenderError::MissingAsset {
            path: path.to_string(),
        })?;
        let bytes = vfs
            .read(&source, ASSET_SIZE_LIMIT)
            .map_err(|source| RenderError::Read {
                path: path.to_string(),
                source,
            })?;
        self.add_png(path.to_string(), bytes, sampler)
    }

    fn add_png(
        &mut self,
        path: String,
        bytes: Vec<u8>,
        sampler: ImageSampler,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        if let Some(index) = self.indices.get(&path).copied() {
            let size = self.pages[index].image.size();
            return Ok((index, [size.x as f64, size.y as f64]));
        }
        let page = decode_png_page(path.clone(), bytes, sampler)?;
        let size = page.image.size();
        let index = self.pages.len();
        self.pages.push(page);
        self.indices.insert(path, index);
        Ok((index, [size.x as f64, size.y as f64]))
    }
}

/// Decodes one PNG page through Bevy's image decoder.
///
/// Shared with the interactive HUD loader so both render paths use the same
/// format/sampler handling. `path` is display-only.
pub(crate) fn decode_png_page(
    path: String,
    bytes: Vec<u8>,
    sampler: ImageSampler,
) -> Result<PageImage, RenderError> {
    let mut image = Image::from_buffer(
        &bytes,
        ImageType::Extension("png"),
        CompressedImageFormats::NONE,
        true,
        ImageSampler::Default,
        RenderAssetUsages::default(),
    )
    .map_err(|error| RenderError::Image {
        path: path.clone(),
        detail: error.to_string(),
    })?;
    image.sampler = sampler;
    let size = image.size();
    if size.x == 0 || size.y == 0 {
        return Err(RenderError::Image {
            path,
            detail: "decoded PNG has a zero dimension".to_string(),
        });
    }
    Ok(PageImage { image })
}

/// Reads and decodes one PNG asset from the VFS. Used by the interactive HUD.
pub(crate) fn load_png_page(
    vfs: &Vfs,
    path: &str,
    sampler: ImageSampler,
) -> Result<PageImage, RenderError> {
    let bytes = read_asset(vfs, path)?;
    decode_png_page(path.to_string(), bytes, sampler)
}

/// Extracts a two-number JSON array without naming `serde_json::Value`
/// (the renderer does not depend on `serde_json` directly; the macro expands
/// where the inferred value type is already known).
macro_rules! json_pair {
    ($value:expr) => {
        $value
            .and_then(|value| value.as_array())
            .and_then(|values| Some((values.first()?.as_f64()?, values.get(1)?.as_f64()?)))
    };
}

fn parse_stage_slot(
    stage: &oxide_data::stage::StageData,
    id: &str,
    stage_path: &str,
) -> Result<LoadedSlot, RenderError> {
    let object = stage
        .characters
        .as_ref()
        .and_then(|characters| characters.as_object())
        .ok_or_else(|| RenderError::Unsupported {
            detail: format!("stage {stage_path:?} characters is not an object"),
        })?;
    let value = object.get(id).ok_or_else(|| RenderError::MissingField {
        path: stage_path.to_string(),
        field: "characters.<id>",
    })?;
    let position = json_pair!(value.get("position")).ok_or_else(|| RenderError::MissingField {
        path: stage_path.to_string(),
        field: "characters.<id>.position",
    })?;
    Ok(LoadedSlot {
        z_index: value
            .get("zIndex")
            .and_then(|number| number.as_f64())
            .unwrap_or(0.0),
        position,
        scale: value
            .get("scale")
            .and_then(|number| number.as_f64())
            .unwrap_or(1.0),
        camera_offsets: json_pair!(value.get("cameraOffsets")).unwrap_or((0.0, 0.0)),
        scroll: json_pair!(value.get("scroll")).unwrap_or((1.0, 1.0)),
    })
}

/// Observed flip rule (`Stage.hx:425`, `BaseCharacter.hx:221-226`,
/// ADR-0020 addendum 4): the player gets `flipX = !getDataFlipX()`; dad/gf
/// keep the data value (absent ⇒ false). `bf.json` has `flipX: true`, so the
/// player is unmirrored in this checkout.
fn resolved_flip_x(role: Role, data_flip_x: Option<bool>) -> bool {
    let data_flip_x = data_flip_x.unwrap_or(false);
    if role == Role::Player {
        !data_flip_x
    } else {
        data_flip_x
    }
}

fn load_character(
    vfs: &Vfs,
    pages: &mut PageStore,
    role: Role,
    id: &str,
    slot: LoadedSlot,
) -> Result<LoadedCharacter, RenderError> {
    let data_path = format!("preload/data/characters/{id}.json");
    let document = read_data_document(vfs, &data_path)?;
    let Typed::Character(character) = document.typed else {
        return Err(RenderError::WrongKind {
            path: data_path,
            expected: "character",
            actual: document.kind,
        });
    };

    let dirs = image_dir_candidates(&character.asset_path);
    let (spritemap_path, spritemap_text) = read_text_in_dirs(vfs, &dirs, "spritemap1.json")?;
    let spritemap = parse_animate(&spritemap_text).map_err(|source| RenderError::Sparrow {
        path: spritemap_path.clone(),
        source,
    })?;
    let image_name = if spritemap.image.is_empty() {
        "spritemap1.png".to_string()
    } else {
        spritemap.image
    };
    // Character atlas content: observed antialiasing is ON, so linear
    // filtering is used for character pages.
    let (page, page_size) = pages.load_png(vfs, &dirs, &image_name, ImageSampler::linear())?;
    let (animation_path, animation_text) = read_text_in_dirs(vfs, &dirs, "Animation.json")?;
    let stage = AnimateStage::parse(&animation_text, &spritemap_text).map_err(|source| {
        RenderError::AtlasStage {
            path: animation_path,
            source,
        }
    })?;
    let bounds = stage.bounds();
    if !bounds.iter().all(|value| value.is_finite()) || bounds[2] <= 0.0 || bounds[3] <= 0.0 {
        return Err(RenderError::Unsupported {
            detail: format!("character {id:?} has empty/non-finite atlas bounds {bounds:?}"),
        });
    }
    let mut animations = BTreeMap::new();
    for animation in &character.animations {
        // Multi-atlas collections (bf death/fakeout) and symbol-rendered
        // animations are outside phase 2b; their collection pages are not
        // merged because that would change bounds and symbol lookup for the
        // rendered slice frames.
        if animation.asset_path.is_some() || animation.anim_type.as_deref() == Some("symbol") {
            continue;
        }
        let prefix = animation
            .prefix
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: format!("preload/data/characters/{id}.json"),
                field: "animations.prefix",
            })?;
        let label_frames =
            stage
                .label_frames(&prefix)
                .ok_or_else(|| RenderError::MissingAnimationLabel {
                    character: id.to_string(),
                    animation: animation.name.clone(),
                    prefix: prefix.clone(),
                })?;
        let frames = match &animation.frame_indices {
            Some(indices) => indices
                .iter()
                .filter_map(|index| usize::try_from(*index).ok())
                .filter_map(|index| label_frames.get(index).copied())
                .collect::<Vec<_>>(),
            None => label_frames,
        };
        if frames.is_empty() {
            return Err(RenderError::MissingAnimationLabel {
                character: id.to_string(),
                animation: animation.name.clone(),
                prefix,
            });
        }
        animations.insert(
            animation.name.clone(),
            AnimationDef {
                frames,
                frame_rate: animation
                    .frame_rate
                    .filter(|rate| *rate > 0.0)
                    .unwrap_or(DEFAULT_ANIMATION_FPS),
                looped: animation.looped.unwrap_or(false),
                offsets: animation.offsets,
            },
        );
    }

    let data_offsets = character.offsets.unwrap_or((0.0, 0.0));
    let camera_offsets = character.camera_offsets.unwrap_or((0.0, 0.0));
    let flip_x = resolved_flip_x(role, character.flip_x);

    Ok(LoadedCharacter {
        role,
        id: id.to_string(),
        stage,
        bounds,
        animations,
        data_offsets,
        camera_offsets,
        sing_time_steps: character.sing_time.unwrap_or(DEFAULT_SING_TIME_STEPS),
        dance_every: character.dance_every.unwrap_or(DEFAULT_DANCE_EVERY),
        scale: character.scale.unwrap_or(1.0) * slot.scale,
        flip_x,
        page,
        page_size,
        slot,
    })
}

struct StyleAsset {
    asset_path: Option<String>,
    scale: Option<f64>,
    offsets: Option<(f64, f64)>,
    /// `data[key].prefix` extracted while the JSON value type is inferred.
    prefixes: BTreeMap<String, String>,
}

struct NoteStyleAssets {
    note: StyleAsset,
    strumline: StyleAsset,
    hold: StyleAsset,
}

fn note_style_assets(
    style: &oxide_data::notestyle::NoteStyle,
    path: &str,
) -> Result<NoteStyleAssets, RenderError> {
    let object = style
        .assets
        .as_object()
        .ok_or_else(|| RenderError::Unsupported {
            detail: format!("note style {path:?} assets is not an object"),
        })?;
    let parse = |key: &str| -> Result<StyleAsset, RenderError> {
        let value = object.get(key).ok_or_else(|| RenderError::MissingField {
            path: path.to_string(),
            field: "assets.<key>",
        })?;
        let prefixes = value
            .get("data")
            .and_then(|data| data.as_object())
            .map(|data| {
                data.iter()
                    .filter_map(|(key, value)| {
                        value
                            .get("prefix")
                            .and_then(|prefix| prefix.as_str())
                            .map(|prefix| (key.clone(), prefix.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(StyleAsset {
            asset_path: value
                .get("assetPath")
                .and_then(|asset_path| asset_path.as_str())
                .map(str::to_string),
            scale: value.get("scale").and_then(|scale| scale.as_f64()),
            offsets: json_pair!(value.get("offsets")),
            prefixes,
        })
    };
    Ok(NoteStyleAssets {
        note: parse("note")?,
        strumline: parse("noteStrumline")?,
        hold: parse("holdNote")?,
    })
}

struct SparrowPage {
    page: usize,
    page_size: [f64; 2],
    frames: Vec<(String, SpriteFrame)>,
}

impl SparrowPage {
    fn frames_for(&self, prefix: &str) -> Vec<SpriteFrame> {
        let mut matches: Vec<(u64, usize, SpriteFrame)> = self
            .frames
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name.starts_with(prefix))
            .map(|(order, (name, frame))| (numeric_suffix(name), order, *frame))
            .collect();
        matches.sort_by_key(|(suffix, order, _)| (*suffix, *order));
        matches.into_iter().map(|(_, _, frame)| frame).collect()
    }
}

fn numeric_suffix(name: &str) -> u64 {
    let digits = name.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return u64::MAX;
    }
    name[name.len() - digits..].parse().unwrap_or(u64::MAX)
}

fn load_sparrow_page(
    vfs: &Vfs,
    pages: &mut PageStore,
    asset_path: &str,
    sampler: ImageSampler,
) -> Result<SparrowPage, RenderError> {
    let bases = image_file_bases(asset_path);
    let mut found = None;
    for base in &bases {
        let xml_path = format!("{base}.xml");
        if let Some(source) = vfs.resolve(&xml_path) {
            let bytes =
                vfs.read(&source, ASSET_SIZE_LIMIT)
                    .map_err(|source| RenderError::Read {
                        path: xml_path.clone(),
                        source,
                    })?;
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
            let xml = String::from_utf8(bytes.to_vec()).map_err(|error| RenderError::Decode {
                path: xml_path.clone(),
                detail: error.to_string(),
            })?;
            found = Some((base.clone(), xml_path, xml));
            break;
        }
    }
    let Some((base, xml_path, xml)) = found else {
        return Err(RenderError::MissingAsset {
            path: format!("{}.xml", bases.first().map_or("<none>", String::as_str)),
        });
    };
    let atlas = parse_sparrow(&xml).map_err(|source| RenderError::Sparrow {
        path: xml_path,
        source,
    })?;
    let image_path = if atlas.image_path.is_empty() {
        format!("{base}.png")
    } else {
        format!("{}/{}", parent_dir(&base), atlas.image_path)
    };
    let (page, page_size) = pages.load_png_file(vfs, &image_path, sampler)?;
    let mut frames = Vec::with_capacity(atlas.subtextures.len());
    for subtexture in &atlas.subtextures {
        if subtexture.rotated {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "sparrow frame {:?} is rotated; the phase-2b slice pages are unrotated",
                    subtexture.name
                ),
            });
        }
        let quad = frame_quad(subtexture);
        frames.push((
            subtexture.name.clone(),
            SpriteFrame {
                page,
                rect: quad.atlas_rect,
                dest_offset: quad.dest_offset,
                dest_size: quad.dest_size,
            },
        ));
    }
    Ok(SparrowPage {
        page,
        page_size,
        frames,
    })
}

/// First frame of the sheet in DOCUMENT order (the frame current at the
/// spriting setup call that runs `updateHitbox()`; FlxSprite `set_frame` ->
/// `resetFrameSize()` tracks the CURRENT frame while the hitbox `width`
/// stays frozen at this call's value).
fn first_frame_dest(page: &SparrowPage, style_path: &str) -> Result<[f64; 2], RenderError> {
    let (_, frame) = page
        .frames
        .first()
        .ok_or_else(|| RenderError::MissingAsset {
            path: format!("{}.xml", style_path),
        })?;
    Ok([frame.dest_size[0] as f64, frame.dest_size[1] as f64])
}

fn require_frames(
    page: &SparrowPage,
    prefix: &str,
    style_path: &str,
) -> Result<Vec<SpriteFrame>, RenderError> {
    let frames = page.frames_for(prefix);
    if frames.is_empty() {
        return Err(RenderError::MissingFrame {
            path: style_path.to_string(),
            prefix: prefix.to_string(),
        });
    }
    Ok(frames)
}

fn require_prefix(
    map: &BTreeMap<String, String>,
    key: &str,
    style_path: &str,
) -> Result<String, RenderError> {
    map.get(key)
        .cloned()
        .ok_or_else(|| RenderError::MissingField {
            path: style_path.to_string(),
            field: "assets.data.<key>.prefix",
        })
}

fn load_note_style(
    vfs: &Vfs,
    pages: &mut PageStore,
    assets: NoteStyleAssets,
    style_path: &str,
) -> Result<LoadedNoteStyle, RenderError> {
    let note_asset = assets
        .note
        .asset_path
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: style_path.to_string(),
            field: "assets.note.assetPath",
        })?;
    // Observed: `target.antialiasing = !(isPixel ?? false)` (NoteStyle.hx:87/
    // 225/253); the funkin style has no `isPixel`, so note content smooths
    // (LINEAR sampling). The earlier nearest engine decision is superseded
    // (human playtest: jagged pixels).
    let note_page = load_sparrow_page(vfs, pages, &note_asset, ImageSampler::linear())?;
    let note_data = &assets.note.prefixes;
    let direction_keys = ["left", "down", "up", "right"];
    let mut note_frames = Vec::with_capacity(4);
    for key in direction_keys {
        let prefix = require_prefix(note_data, key, style_path)?;
        let frames = require_frames(&note_page, &prefix, style_path)?;
        let frame = frames
            .first()
            .copied()
            .ok_or_else(|| RenderError::MissingFrame {
                path: style_path.to_string(),
                prefix: prefix.clone(),
            })?;
        note_frames.push(frame);
    }
    let note_hitbox_dest = first_frame_dest(&note_page, style_path)?;

    let strumline_asset =
        assets
            .strumline
            .asset_path
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: style_path.to_string(),
                field: "assets.noteStrumline.assetPath",
            })?;
    // Observed: same `antialiasing = !(isPixel ?? false)` rule for the
    // strumline page (NoteStyle.hx:225/253).
    let strumline_page = load_sparrow_page(vfs, pages, &strumline_asset, ImageSampler::linear())?;
    let strumline_data = &assets.strumline.prefixes;
    let receptor_hitbox_dest = first_frame_dest(&strumline_page, style_path)?;
    let mut receptors = Vec::with_capacity(4);
    for direction in direction_keys {
        let state_key = |suffix: &str| format!("{direction}{suffix}");
        let static_prefix = require_prefix(strumline_data, &state_key("Static"), style_path)?;
        let press_prefix = require_prefix(strumline_data, &state_key("Press"), style_path)?;
        let confirm_prefix = require_prefix(strumline_data, &state_key("Confirm"), style_path)?;
        let confirm_hold_prefix =
            require_prefix(strumline_data, &state_key("ConfirmHold"), style_path)?;
        receptors.push(ReceptorFrames {
            static_frames: require_frames(&strumline_page, &static_prefix, style_path)?,
            press_frames: require_frames(&strumline_page, &press_prefix, style_path)?,
            confirm_frames: require_frames(&strumline_page, &confirm_prefix, style_path)?,
            confirm_hold_frames: require_frames(&strumline_page, &confirm_hold_prefix, style_path)?,
        });
    }

    let hold_asset = assets
        .hold
        .asset_path
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: style_path.to_string(),
            field: "assets.holdNote.assetPath",
        })?;
    // Observed: the hold sheet follows the same style rule (NoteStyle.hx:225).
    let (hold_page, hold_size) = pages.load_image_key(vfs, &hold_asset, ImageSampler::linear())?;
    if hold_size
        != [
            super::geometry::HOLD_TEXTURE_WIDTH,
            super::geometry::HOLD_TEXTURE_HEIGHT,
        ]
    {
        return Err(RenderError::Unsupported {
            detail: format!("hold sheet is {hold_size:?}, expected 416x87"),
        });
    }

    let note_frames: [SpriteFrame; 4] =
        note_frames
            .try_into()
            .map_err(|_| RenderError::Unsupported {
                detail: "note style does not define four note directions".to_string(),
            })?;
    let receptors: [ReceptorFrames; 4] =
        receptors.try_into().map_err(|_| RenderError::Unsupported {
            detail: "note style does not define four receptor directions".to_string(),
        })?;
    if note_frames.iter().any(|frame| frame.page != note_page.page)
        || receptors.iter().any(|receptor| {
            receptor
                .static_frames
                .iter()
                .any(|frame| frame.page != strumline_page.page)
        })
    {
        return Err(RenderError::Unsupported {
            detail: "note style frames span more than one Sprite page".to_string(),
        });
    }
    let hold_width = super::geometry::sustain_width(hold_size[0], assets.hold.scale.unwrap_or(1.0));
    Ok(LoadedNoteStyle {
        note_scale: assets.note.scale.unwrap_or(1.0),
        note_frames,
        note_page_size: note_page.page_size,
        note_hitbox_dest,
        receptor_scale: assets.strumline.scale.unwrap_or(1.0),
        receptor_offsets: assets.strumline.offsets.unwrap_or((0.0, 0.0)),
        receptors,
        receptor_page: strumline_page.page,
        receptor_page_size: strumline_page.page_size,
        receptor_hitbox_dest,
        hold_page,
        hold_width,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_flip_follows_the_observed_data_rule() {
        // bf.json flipX=true => the player does NOT mirror.
        assert!(!resolved_flip_x(Role::Player, Some(true)));
        // A player checkout without flipX would mirror (native negation).
        assert!(resolved_flip_x(Role::Player, None));
        // dad/gf keep the data value; both files omit flipX.
        assert!(!resolved_flip_x(Role::Opponent, None));
        assert!(!resolved_flip_x(Role::Girlfriend, None));
        assert!(resolved_flip_x(Role::Opponent, Some(true)));
        assert!(resolved_flip_x(Role::Girlfriend, Some(true)));
    }
}
