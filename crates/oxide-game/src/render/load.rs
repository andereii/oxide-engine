//! Slice asset loading through `oxide-vfs` and `oxide-data`.
//!
//! The renderer never composes a filesystem path itself: every JSON/PNG read
//! goes through [`oxide_vfs::Vfs::resolve`] (exact relative paths) so mod layer
//! precedence and case-exact lookup stay owned by the VFS crates. The only
//! path conventions here are the V-Slice asset-key prefix (`shared:`, a
//! library prefix, or the plain level key) and the observed level/shared/
//! preload candidate order (M6-CONTRACT §11.1).
//!
//! PNG decoding uses `bevy::image` (`Image::from_buffer`) with the enabled
//! `png` format feature; no extra image dependency is introduced.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use bevy::asset::RenderAssetUsages;
use bevy::image::{CompressedImageFormats, Image, ImageSampler, ImageType};
use oxide_atlas::{AnimateStage, frame_quad, parse_animate, parse_packer_text, parse_sparrow};
use oxide_data::Typed;
use oxide_data::character::AnimationData;
use oxide_data::stage::{StageData, StageProp};
use oxide_vfs::Vfs;

use super::RenderError;
use super::anim::{
    AnimationDef, DEFAULT_ANIMATION_FPS, DEFAULT_DANCE_EVERY, DEFAULT_SING_TIME_STEPS,
};

/// Upper bound for any single slice asset read, matching the M5 slice VFS
/// read contract. The largest shipped page in the slice is a few MiB.
const ASSET_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

const BOPEEBO_CHART_ID: &str = "bopeebo-default";

/// The three slice roles, in the order used by the renderer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    Player = 0,
    Opponent = 1,
    Girlfriend = 2,
}

impl Role {
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

/// One frame of a sparse or packer page (trimmed-frame geometry kept
/// unwrapped). `rect` is the UV-only page rectangle; `dest_offset`/`dest_size`
/// follow the oxide-atlas Sparrow contract.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpriteFrame {
    pub(crate) page: usize,
    /// `[x, y, width, height]` in the page.
    pub(crate) rect: [i32; 4],
    /// `(-frameX, -frameY)` inside the frame box.
    pub(crate) dest_offset: [i32; 2],
    /// `(frameWidth, frameHeight)` of the frame box.
    pub(crate) dest_size: [i32; 2],
    /// Sparrow `rotated` page entry: `rect` stays the stored page rect and the
    /// renderer composes flixel's ANGLE_NEG_90 tile matrix.
    pub(crate) rotated: bool,
}

/// A Sparrow/Packer page in document order.
#[derive(Debug, Clone)]
pub(crate) struct SpriteSheet {
    pub(crate) page: usize,
    pub(crate) page_size: [f64; 2],
    pub(crate) frames: Vec<(String, SpriteFrame)>,
}

impl SpriteSheet {
    pub(crate) fn frame(&self, index: usize) -> Option<SpriteFrame> {
        self.frames.get(index).map(|(_, frame)| *frame)
    }

    pub(crate) fn first_dest(&self) -> Option<[i32; 2]> {
        self.frames.first().map(|(_, frame)| frame.dest_size)
    }

    /// Document-order prefix matches (native `addByPrefix` over frame names).
    fn prefix_indices(&self, prefix: &str) -> Vec<usize> {
        self.frames
            .iter()
            .enumerate()
            .filter(|(_, (name, _))| name.starts_with(prefix))
            .map(|(index, _)| index)
            .collect()
    }
}

/// One animate-atlas collection page (Animation.json + spritemap1.json).
#[derive(Debug, Clone)]
pub(crate) struct AtlasSheet {
    pub(crate) stage: AnimateStage,
    pub(crate) bounds: [f64; 4],
    pub(crate) page: usize,
    pub(crate) page_size: [f64; 2],
}

/// A loaded visual sheet. Characters may mix atlas and Sparrow/Packer sheets
/// because per-animation `assetPath`/`renderType` data can swap collections.
#[derive(Debug, Clone)]
pub(crate) enum Sheet {
    Atlas(Box<AtlasSheet>),
    Sprite(SpriteSheet),
}

impl Sheet {
    pub(crate) fn page(&self) -> usize {
        match self {
            Sheet::Atlas(sheet) => sheet.page,
            Sheet::Sprite(sheet) => sheet.page,
        }
    }

    pub(crate) fn bounds(&self) -> Option<[f64; 4]> {
        match self {
            Sheet::Atlas(sheet) => Some(sheet.bounds),
            Sheet::Sprite(sheet) => sheet
                .first_dest()
                .map(|dest| [0.0, 0.0, dest[0] as f64, dest[1] as f64]),
        }
    }
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

/// A parsed character plus its resolved sheet and animation bindings.
pub(crate) struct LoadedCharacter {
    pub(crate) role: Role,
    pub(crate) id: String,
    pub(crate) sheets: Vec<Sheet>,
    /// Whole-sprite placement bounds at scale 1 (main sheet).
    pub(crate) bounds: [f64; 4],
    /// Setup-time Sparrow/Packer hitbox (`updateHitbox` first document frame).
    /// `None` when every loaded sheet is an animate atlas.
    pub(crate) sprite_hitbox: Option<[i32; 2]>,
    pub(crate) animations: BTreeMap<String, AnimationDef>,
    pub(crate) data_offsets: (f64, f64),
    pub(crate) camera_offsets: (f64, f64),
    pub(crate) sing_time_steps: f64,
    pub(crate) dance_every: f64,
    pub(crate) scale: f64,
    pub(crate) flip_x: bool,
    pub(crate) starting_animation: Option<String>,
    pub(crate) slot: LoadedSlot,
    /// Character `healthIcon` data normalized with the native defaults
    /// (`CharacterData.hx` L531-586).
    pub(crate) health_icon: super::hud::IconConfig,
}

/// One static or animated stage prop.
pub(crate) enum PropVisual {
    /// `#RRGGBB` makeSolidColor equivalent. `size` is the INTEGER
    /// `makeSolidColor(width, height)` dimension pair computed from `scale`
    /// (default 1x1); the draw path applies a unit transform (Stage.hx skips
    /// `scale.set` for solids, M6-CONTRACT §11.6).
    Solid { size: [i32; 2], color: [f32; 4] },
    /// Non-animated texture (`loadTexture`, `active = false`).
    Static { page: usize, image_size: [f64; 2] },
    Sprite {
        sheet: SpriteSheet,
        hitbox_dest: [i32; 2],
    },
    Atlas {
        sheet: Box<AtlasSheet>,
        /// `atlasSettings.applyStageMatrix == true`: loaded for inventory but
        /// not drawn until the native symbol-matrix semantics are frozen
        /// (M6-CONTRACT §11.6; 6C follow-up).
        apply_stage_matrix: bool,
    },
}

/// One loaded stage prop. Boppers own a scheduler; static/solid props do not.
pub(crate) struct LoadedStageProp {
    pub(crate) name: String,
    pub(crate) z_index: f64,
    pub(crate) position: (f64, f64),
    pub(crate) scale: (f64, f64),
    pub(crate) scroll: (f64, f64),
    pub(crate) alpha: f64,
    pub(crate) angle: f64,
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
    pub(crate) visual: PropVisual,
    pub(crate) animations: BTreeMap<String, AnimationDef>,
    pub(crate) dance_every: f64,
    pub(crate) starting_animation: Option<String>,
    pub(crate) bopper: bool,
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
    pub(crate) hold_page_size: [f64; 2],
    pub(crate) hold_scale: f64,
    pub(crate) hold_offsets: (f64, f64),
    /// `(hold page width / 8) * hold style zoom` (416/8*0.7 = 36.4 for funkin,
    /// 72/8*6 = 54 for the pixel style).
    pub(crate) hold_width: f64,
}

impl LoadedNoteStyle {
    /// Sustain body height above the end cap (`graphic.height * zoom * 0.5`).
    pub(crate) fn sustain_bottom_height(&self) -> f64 {
        super::geometry::sustain_bottom_height(self.hold_page_size[1], self.hold_scale)
    }

    /// End-cap tail below `graphicHeight` (`height * zoom * (0.9 - 0.5)`).
    pub(crate) fn sustain_extra_cap_tail(&self) -> f64 {
        super::geometry::sustain_extra_cap_tail(self.hold_page_size[1], self.hold_scale)
    }

    /// Full hold strip height in game pixels.
    pub(crate) fn hold_strip_height(&self) -> f64 {
        self.hold_page_size[1] * self.hold_scale
    }
}

/// One chart note (both chart sides; `note_id` matches the sim's per-
/// strumline convention: player notes carry the compacted player id, opponent
/// notes the compacted opponent id; exactly one side is `Some`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoadedNote {
    pub(crate) time: f64,
    pub(crate) direction: usize,
    pub(crate) sustain_ms: f64,
    pub(crate) opponent: bool,
    pub(crate) note_id: Option<u32>,
}

/// Everything the scene builder and headless harness need.
pub(crate) struct LoadedSlice {
    pub(crate) pages: Vec<PageImage>,
    pub(crate) stage_zoom: f64,
    pub(crate) props: Vec<LoadedStageProp>,
    /// Player and opponent are always loaded. Girlfriend is omitted when the
    /// metadata carries no id or an empty id (observed `tutorial-default`:
    /// the field is absent, and native leaves `girlfriend == ''` so no
    /// girlfriend character is added to the stage).
    pub(crate) characters: Vec<LoadedCharacter>,
    pub(crate) note_style: LoadedNoteStyle,
    pub(crate) notes: Vec<LoadedNote>,
    pub(crate) scroll_speed: f64,
    pub(crate) conductor_changes: Vec<oxide_audio::conductor::TimeChange>,
    /// Metadata `playData.characters.playerVocals` (voice track ids).
    pub(crate) player_vocals: Vec<String>,
    /// Metadata `playData.characters.opponentVocals` (voice track ids).
    pub(crate) opponent_vocals: Vec<String>,
    /// Loaded HUD core (M6-CONTRACT §13). Synthetic scene tests pass `None`;
    /// the real load path always carries it (missing assets fail closed).
    pub(crate) hud: Option<super::hud::LoadedHud>,
}

/// Observed character renderType dispatch (M6-CONTRACT §11.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CharacterRenderType {
    AnimateAtlas,
    MultiAnimateAtlas,
    MultiSparrow,
    Sparrow,
    Packer,
}

impl CharacterRenderType {
    fn is_animate(self) -> bool {
        matches!(self, Self::AnimateAtlas | Self::MultiAnimateAtlas)
    }
}

/// Native `DEFAULT_RENDERTYPE = Sparrow`: a null/absent renderType loads as
/// Sparrow. Any other string fails closed naming the character file (the
/// native Custom/script branch has no corpus content).
fn resolve_render_type(
    render_type: Option<&str>,
    character_path: &str,
) -> Result<CharacterRenderType, RenderError> {
    match render_type {
        None => Ok(CharacterRenderType::Sparrow),
        Some("animateatlas") => Ok(CharacterRenderType::AnimateAtlas),
        Some("multianimateatlas") => Ok(CharacterRenderType::MultiAnimateAtlas),
        Some("multisparrow") => Ok(CharacterRenderType::MultiSparrow),
        Some("sparrow") => Ok(CharacterRenderType::Sparrow),
        Some("packer") => Ok(CharacterRenderType::Packer),
        Some(other) => Err(RenderError::UnknownRenderType {
            path: character_path.to_string(),
            render_type: other.to_string(),
        }),
    }
}

/// Loads and parses every bopeebo slice asset for `difficulty` (the frozen M5
/// default path).
pub(crate) fn load_slice(assets_root: &Path, difficulty: &str) -> Result<LoadedSlice, RenderError> {
    load_slice_for_song(assets_root, BOPEEBO_CHART_ID, difficulty)
}

/// Builds the same scene surface for any `corpus.txt`-style chart id
/// (`<song>`, `<song>-default`, or a known `<song>-<variation>`).
pub(crate) fn load_slice_for_song(
    assets_root: &Path,
    chart_id: &str,
    difficulty: &str,
) -> Result<LoadedSlice, RenderError> {
    let (chart_path, metadata_path) = song_paths(chart_id)?;
    load_slice_from_paths(assets_root, &chart_path, &metadata_path, difficulty)
}

/// Maps a corpus-style chart id to its chart/metadata paths. Mirrors the 6A
/// `build_chart` iding: only the known corpus variations are split off, so
/// hyphenated song names stay intact.
fn song_paths(chart_id: &str) -> Result<(String, String), RenderError> {
    const VARIATIONS: [&str; 4] = ["default", "erect", "pico", "bf"];
    let (song, variant) = chart_id
        .rsplit_once('-')
        .filter(|(_, variant)| VARIATIONS.contains(variant))
        .unwrap_or((chart_id, "default"));
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    };
    if song.is_empty()
        || song.starts_with('-')
        || song.ends_with('-')
        || !valid_part(song)
        || !valid_part(variant)
    {
        return Err(RenderError::Unsupported {
            detail: format!("invalid chart id {chart_id:?}"),
        });
    }
    let suffix = if variant == "default" {
        String::new()
    } else {
        format!("-{variant}")
    };
    let stem = format!("preload/data/songs/{song}/{song}");
    Ok((
        format!("{stem}-chart{suffix}.json"),
        format!("{stem}-metadata{suffix}.json"),
    ))
}

/// Shared chart/metadata loader. Stage, characters and note style come from
/// the metadata; only the song file pair is selected by the caller.
pub(crate) fn load_slice_from_paths(
    assets_root: &Path,
    chart_path: &str,
    metadata_path: &str,
    difficulty: &str,
) -> Result<LoadedSlice, RenderError> {
    if difficulty.is_empty() {
        return Err(RenderError::UnknownDifficulty(difficulty.to_string()));
    }

    let vfs = Vfs::new(assets_root.to_path_buf());
    let mut pages = PageStore::default();

    let chart_document = read_data_document(&vfs, chart_path)?;
    let Typed::Chart(chart) = chart_document.typed else {
        return Err(RenderError::WrongKind {
            path: chart_path.to_string(),
            expected: "chart",
            actual: chart_document.kind,
        });
    };
    let metadata_document = read_data_document(&vfs, metadata_path)?;
    let Typed::Metadata(metadata) = metadata_document.typed else {
        return Err(RenderError::WrongKind {
            path: metadata_path.to_string(),
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
            path: chart_path.to_string(),
            field: "scrollSpeed",
        })?;
    if !scroll_speed.is_finite() || scroll_speed <= 0.0 {
        return Err(RenderError::Unsupported {
            detail: format!("chart scrollSpeed {scroll_speed} is not a positive number"),
        });
    }

    let mut notes = Vec::with_capacity(chart_notes.len());
    let mut player_id = 0u32;
    let mut opponent_id = 0u32;
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
            let id = opponent_id;
            opponent_id += 1;
            id
        } else {
            let id = player_id;
            player_id += 1;
            id
        };
        notes.push(LoadedNote {
            time: note.time,
            direction,
            sustain_ms,
            opponent,
            note_id: Some(id),
        });
    }

    let play_data = metadata
        .play_data
        .as_ref()
        .ok_or_else(|| RenderError::MissingField {
            path: metadata_path.to_string(),
            field: "playData",
        })?;
    let characters = play_data
        .characters
        .as_ref()
        .ok_or_else(|| RenderError::MissingField {
            path: metadata_path.to_string(),
            field: "playData.characters",
        })?;
    let player_id_name = characters
        .player
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: metadata_path.to_string(),
            field: "playData.characters.player",
        })?;
    let opponent_id_name =
        characters
            .opponent
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: metadata_path.to_string(),
                field: "playData.characters.opponent",
            })?;
    // The observed data model defaults a missing girlfriend to `''` and the
    // native character init loads none in that case (tutorial-default).
    let girlfriend_id_name = characters
        .girlfriend
        .clone()
        .filter(|name| !name.is_empty());
    let player_vocals = characters.player_vocals.clone().unwrap_or_default();
    let opponent_vocals = characters.opponent_vocals.clone().unwrap_or_default();
    let stage_name = play_data
        .stage
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: metadata_path.to_string(),
            field: "playData.stage",
        })?;
    let note_style_name =
        play_data
            .note_style
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: metadata_path.to_string(),
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
    if stage.props.iter().any(|prop| prop.name.is_empty()) {
        return Err(RenderError::Unsupported {
            detail: "stage has a prop with an empty name".to_string(),
        });
    }
    let stage_zoom = stage
        .camera_zoom
        .unwrap_or(super::geometry::DEFAULT_STAGE_ZOOM);
    if stage.characters.is_none() {
        return Err(RenderError::MissingField {
            path: stage_path.clone(),
            field: "characters",
        });
    }
    // Native plain-key resolution starts at the level directory and then
    // falls back to shared/preload (`Paths.getPath`/`getLibraryPath`).
    let level_dir = stage.directory.clone();

    let props = load_stage_props(&vfs, &mut pages, &stage, &stage_path, level_dir.as_deref())?;

    let player_slot = parse_stage_slot(&stage, "bf", &stage_path)?;
    let opponent_slot = parse_stage_slot(&stage, "dad", &stage_path)?;

    let player = load_character(
        &vfs,
        &mut pages,
        Role::Player,
        &player_id_name,
        player_slot,
        level_dir.as_deref(),
    )?;
    let opponent = load_character(
        &vfs,
        &mut pages,
        Role::Opponent,
        &opponent_id_name,
        opponent_slot,
        level_dir.as_deref(),
    )?;
    let mut characters = vec![player, opponent];
    if let Some(girlfriend_id_name) = girlfriend_id_name {
        let girlfriend_slot = parse_stage_slot(&stage, "gf", &stage_path)?;
        characters.push(load_character(
            &vfs,
            &mut pages,
            Role::Girlfriend,
            &girlfriend_id_name,
            girlfriend_slot,
            level_dir.as_deref(),
        )?);
    }

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
    let note_style = load_note_style(
        &vfs,
        &mut pages,
        style_asset,
        &note_style_path,
        level_dir.as_deref(),
    )?;

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

    // M6-CONTRACT §13: the deterministic scene composes the play HUD core, so
    // the HUD pages/font join the same page store. Missing HUD assets fail
    // closed (a partial HUD would silently change frozen frame pixels).
    let hud = super::hud::load_hud(&vfs, &mut pages)?;

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
        hud: Some(hud),
    })
}

fn read_data_document(vfs: &Vfs, path: &str) -> Result<oxide_data::ParsedDocument, RenderError> {
    let bytes = read_asset(vfs, path)?;
    oxide_data::parse_file(Path::new(path), &bytes).map_err(|source| RenderError::Parse {
        path: path.to_string(),
        source,
    })
}

pub(crate) fn read_asset(vfs: &Vfs, path: &str) -> Result<Vec<u8>, RenderError> {
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

/// V-Slice image key candidates (without extension), in the observed
/// `Paths.getPath` order (`M6-CONTRACT` §11.1):
///
/// - explicit `<lib>:<key>` -> `<lib>/images/<key>`, then shared, then preload;
/// - plain `<key>` -> `<level directory>/images/<key>`, then shared, then
///   preload (`getLibraryPath` fallbacks).
fn image_file_bases(key: &str, level_dir: Option<&str>) -> Vec<String> {
    if let Some((library, rest)) = key.split_once(':') {
        let mut bases = vec![format!("{library}/images/{rest}")];
        for fallback in ["shared", "preload"] {
            let candidate = format!("{fallback}/images/{rest}");
            if !bases.contains(&candidate) {
                bases.push(candidate);
            }
        }
        return bases;
    }
    let mut bases = Vec::with_capacity(3);
    if let Some(directory) = level_dir {
        bases.push(format!("{directory}/images/{key}"));
    }
    for fallback in ["shared", "preload"] {
        let candidate = format!("{fallback}/images/{key}");
        if !bases.contains(&candidate) {
            bases.push(candidate);
        }
    }
    bases
}

/// V-Slice directory candidates for multi-file atlas assets (Animation.json,
/// spritemap files, Sparrow XML/PNG siblings). Same observed order as
/// [`image_file_bases`].
fn image_dir_candidates(key: &str, level_dir: Option<&str>) -> Vec<String> {
    image_file_bases(key, level_dir)
}

fn sampler_for(is_pixel: bool) -> ImageSampler {
    if is_pixel {
        ImageSampler::nearest()
    } else {
        ImageSampler::linear()
    }
}

#[derive(Default)]
pub(crate) struct PageStore {
    pages: Vec<PageImage>,
    indices: HashMap<String, usize>,
}

impl PageStore {
    fn load_image_key(
        &mut self,
        vfs: &Vfs,
        key: &str,
        level_dir: Option<&str>,
        is_pixel: bool,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        let bases = image_file_bases(key, level_dir);
        for base in &bases {
            let path = format!("{base}.png");
            if let Some(source) = vfs.resolve(&path) {
                let bytes =
                    vfs.read(&source, ASSET_SIZE_LIMIT)
                        .map_err(|source| RenderError::Read {
                            path: path.clone(),
                            source,
                        })?;
                return self.add_png(path, bytes, is_pixel);
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
        is_pixel: bool,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        let (path, bytes) = resolve_in_dirs(vfs, dirs, file)?;
        self.add_png(path, bytes, is_pixel)
    }

    pub(crate) fn load_png_file(
        &mut self,
        vfs: &Vfs,
        path: &str,
        is_pixel: bool,
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
        self.add_png(path.to_string(), bytes, is_pixel)
    }

    fn add_png(
        &mut self,
        path: String,
        bytes: Vec<u8>,
        is_pixel: bool,
    ) -> Result<(usize, [f64; 2]), RenderError> {
        let key = format!("{path}|{is_pixel}");
        if let Some(index) = self.indices.get(&key).copied() {
            let size = self.pages[index].image.size();
            return Ok((index, [size.x as f64, size.y as f64]));
        }
        let page = decode_png_page(path.clone(), bytes, sampler_for(is_pixel))?;
        let size = page.image.size();
        let index = self.pages.len();
        self.pages.push(page);
        self.indices.insert(key, index);
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
    stage: &StageData,
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

/// Observed Stage.hx classification: a prop is a Bopper when it has at least
/// one animation or a non-zero `danceEvery`; otherwise it is a static prop.
fn is_bopper_prop(prop: &StageProp) -> bool {
    is_animated_prop(prop) || prop.dance_every.is_some_and(|every| every != 0.0)
}

/// `isAnimated = dataProp.animations.length > 0` (Stage.hx L175). This is the
/// SHEET-loading decision and is independent of the Bopper class decision: a
/// `danceEvery`-only Bopper loads a static texture and still dances it.
fn is_animated_prop(prop: &StageProp) -> bool {
    prop.animations
        .as_ref()
        .is_some_and(|animations| !animations.is_empty())
}

/// `atlasSettings.applyStageMatrix == true` makes an animate-atlas prop
/// load-only for now (M6-CONTRACT §11.6).
fn applies_stage_matrix(prop: &StageProp) -> bool {
    prop.atlas_settings
        .as_ref()
        .and_then(|settings| settings.get("applyStageMatrix"))
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

/// `makeSolidColor` dimensions from the prop `scale` (`Stage.hx` L224-244):
/// scalar/pair values become `Std.int(...)` dimensions; the caller supplies
/// the native 1x1 default. The data layer normalizes pair/absent only (the
/// shipped corpus has no scalar stage-prop scale), so the already-normalized
/// pair is consumed here.
fn solid_dimensions(scale: (f64, f64)) -> [i32; 2] {
    // Haxe `Std.int` truncates toward zero.
    [scale.0.trunc() as i32, scale.1.trunc() as i32]
}

/// `#RRGGBB` solid-color prop (`makeSolidColor`). Returns `None` when the
/// asset path is not a solid color; malformed six-digit hex is an error at the
/// caller.
fn parse_solid_color(asset_path: &str) -> Option<Result<[f32; 4], String>> {
    let hex = asset_path.strip_prefix('#')?;
    if hex.len() != 6 || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Some(Err(format!(
            "solid-color asset path {asset_path:?} is not #RRGGBB"
        )));
    }
    let channel = |range: std::ops::Range<usize>| -> f32 {
        u8::from_str_radix(&hex[range], 16).map_or(0.0, |value| value as f32 / 255.0)
    };
    Some(Ok([channel(0..2), channel(2..4), channel(4..6), 1.0]))
}

fn load_stage_props(
    vfs: &Vfs,
    pages: &mut PageStore,
    stage: &StageData,
    stage_path: &str,
    level_dir: Option<&str>,
) -> Result<Vec<LoadedStageProp>, RenderError> {
    let mut props = Vec::with_capacity(stage.props.len());
    for prop in &stage.props {
        let asset_path = prop
            .asset_path
            .clone()
            .ok_or_else(|| RenderError::MissingField {
                path: stage_path.to_string(),
                field: "props.assetPath",
            })?;
        let bopper = is_bopper_prop(prop);
        let is_animated = is_animated_prop(prop);
        let is_pixel = prop.is_pixel.unwrap_or(false);
        let nearest = is_pixel;
        let scale = prop.scale.unwrap_or((1.0, 1.0));

        let mut animations = BTreeMap::new();
        // Stage.hx order: animated sheets first, then solid color, then the
        // static texture. The Bopper class decision above is independent: a
        // `danceEvery`-only prop still loads a static texture here but keeps
        // its dance scheduler (M6-CONTRACT §11.6).
        let visual = if is_animated {
            match prop.anim_type.as_deref() {
                Some("packer") => {
                    let sheet = load_packer_sheet(vfs, pages, &asset_path, level_dir, nearest)?;
                    let hitbox_dest =
                        sheet
                            .first_dest()
                            .ok_or_else(|| RenderError::MissingFrame {
                                path: asset_path.clone(),
                                prefix: "<first>".to_string(),
                            })?;
                    if let Some(list) = &prop.animations {
                        for animation in list {
                            animations.insert(
                                animation.name.clone(),
                                sprite_animation_def(
                                    animation,
                                    &sheet,
                                    0,
                                    SpriteAnimationMode::PackerDirect,
                                    &asset_path,
                                )?,
                            );
                        }
                    }
                    PropVisual::Sprite { sheet, hitbox_dest }
                }
                Some("animateatlas") => {
                    let sheet = load_atlas_sheet(vfs, pages, &asset_path, level_dir, nearest)?;
                    if let Some(list) = &prop.animations {
                        for animation in list {
                            animations.insert(
                                animation.name.clone(),
                                atlas_animation_def(animation, &sheet, 0, &asset_path)?,
                            );
                        }
                    }
                    PropVisual::Atlas {
                        sheet: Box::new(sheet),
                        apply_stage_matrix: applies_stage_matrix(prop),
                    }
                }
                _ => {
                    let sheet = load_sparrow_sheet(vfs, pages, &asset_path, level_dir, nearest)?;
                    let hitbox_dest =
                        sheet
                            .first_dest()
                            .ok_or_else(|| RenderError::MissingFrame {
                                path: asset_path.clone(),
                                prefix: "<first>".to_string(),
                            })?;
                    if let Some(list) = &prop.animations {
                        for animation in list {
                            animations.insert(
                                animation.name.clone(),
                                sprite_animation_def(
                                    animation,
                                    &sheet,
                                    0,
                                    SpriteAnimationMode::Prefix,
                                    &asset_path,
                                )?,
                            );
                        }
                    }
                    PropVisual::Sprite { sheet, hitbox_dest }
                }
            }
        } else if let Some(parsed_color) = parse_solid_color(&asset_path) {
            let color = parsed_color.map_err(|detail| RenderError::Unsupported {
                detail: format!("stage prop {:?}: {detail}", prop.name),
            })?;
            PropVisual::Solid {
                // `makeSolidColor(width, height, color)` consumes `scale` as
                // SIZE, not as a second scale factor; the draw path applies a
                // unit transform (Stage.hx L224-244, M6-CONTRACT §11.6).
                size: solid_dimensions(scale),
                color,
            }
        } else {
            let (page, image_size) = pages.load_image_key(vfs, &asset_path, level_dir, nearest)?;
            PropVisual::Static { page, image_size }
        };

        props.push(LoadedStageProp {
            name: prop.name.clone(),
            z_index: prop.z_index.unwrap_or(0.0),
            position: prop.position.unwrap_or((0.0, 0.0)),
            scale,
            scroll: prop.scroll.unwrap_or((1.0, 1.0)),
            alpha: prop.alpha.unwrap_or(1.0),
            angle: prop.angle.unwrap_or(0.0),
            flip_x: prop.flip_x.unwrap_or(false),
            flip_y: prop.flip_y.unwrap_or(false),
            visual,
            animations,
            dance_every: prop.dance_every.unwrap_or(0.0),
            starting_animation: prop.starting_animation.clone(),
            bopper,
        });
    }
    Ok(props)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum SheetKind {
    Atlas,
    Sprite,
    Packer,
}

fn load_sheet(
    vfs: &Vfs,
    pages: &mut PageStore,
    key: &str,
    level_dir: Option<&str>,
    kind: SheetKind,
    is_pixel: bool,
) -> Result<Sheet, RenderError> {
    Ok(match kind {
        SheetKind::Atlas => Sheet::Atlas(Box::new(load_atlas_sheet(
            vfs, pages, key, level_dir, is_pixel,
        )?)),
        SheetKind::Sprite => {
            Sheet::Sprite(load_sparrow_sheet(vfs, pages, key, level_dir, is_pixel)?)
        }
        SheetKind::Packer => {
            Sheet::Sprite(load_packer_sheet(vfs, pages, key, level_dir, is_pixel)?)
        }
    })
}

fn load_character(
    vfs: &Vfs,
    pages: &mut PageStore,
    role: Role,
    id: &str,
    slot: LoadedSlot,
    level_dir: Option<&str>,
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
    let render_type = resolve_render_type(character.render_type.as_deref(), &data_path)?;
    let is_pixel = character.is_pixel.unwrap_or(false);

    let main_kind = if render_type.is_animate() {
        SheetKind::Atlas
    } else if render_type == CharacterRenderType::Packer {
        SheetKind::Packer
    } else {
        SheetKind::Sprite
    };
    let main_sheet = load_sheet(
        vfs,
        pages,
        &character.asset_path,
        level_dir,
        main_kind,
        is_pixel,
    )?;
    let bounds = main_sheet
        .bounds()
        .ok_or_else(|| RenderError::Unsupported {
            detail: format!("character {id:?} main sheet has no frames"),
        })?;
    let main_hitbox = match &main_sheet {
        Sheet::Sprite(sheet) => sheet.first_dest(),
        Sheet::Atlas(_) => None,
    };

    let mut sheets = vec![main_sheet];
    let mut sheet_cache: HashMap<(String, SheetKind), usize> = HashMap::new();
    sheet_cache.insert((character.asset_path.clone(), main_kind), 0);

    let mut sprite_hitbox = main_hitbox;
    let mut animations = BTreeMap::new();
    for animation in &character.animations {
        let (sheet_index, _sheet_kind) = match animation.asset_path.clone() {
            Some(asset_path) => {
                let kind = match animation.render_type.as_deref() {
                    Some("sparrow") => SheetKind::Sprite,
                    Some("packer") => SheetKind::Packer,
                    _ if render_type.is_animate() => SheetKind::Atlas,
                    _ => SheetKind::Sprite,
                };
                let key = (asset_path.clone(), kind);
                let existing = sheet_cache.get(&key).copied();
                match existing {
                    Some(index) => (index, kind),
                    None => match load_sheet(vfs, pages, &asset_path, level_dir, kind, is_pixel) {
                        Ok(sheet) => {
                            if sprite_hitbox.is_none()
                                && let Sheet::Sprite(sheet) = &sheet
                            {
                                sprite_hitbox = sheet.first_dest();
                            }
                            sheets.push(sheet);
                            let index = sheets.len() - 1;
                            sheet_cache.insert(key, index);
                            (index, kind)
                        }
                        Err(error)
                            if render_type.is_animate()
                                && animation.anim_type.as_deref() == Some("symbol") =>
                        {
                            // bf's `fakeoutDeath` points at the newer
                            // SYMBOL_DICTIONARY atlas format, which is outside
                            // the frozen AnimateStage semantics. Keep the
                            // character loadable and record the skipped swap.
                            eprintln!(
                                "oxide render: character {id:?} skips animation {:?} \
                                 assetPath {asset_path:?}: {error}",
                                animation.name
                            );
                            continue;
                        }
                        Err(error) => return Err(error),
                    },
                }
            }
            None => (0, main_kind),
        };

        let def = match &sheets[sheet_index] {
            Sheet::Atlas(sheet) => atlas_animation_def(animation, sheet, sheet_index, id)?,
            Sheet::Sprite(sheet) => sprite_animation_def(
                animation,
                sheet,
                sheet_index,
                SpriteAnimationMode::Prefix,
                id,
            )?,
        };
        animations.insert(animation.name.clone(), def);
    }

    let data_offsets = character.offsets.unwrap_or((0.0, 0.0));
    let camera_offsets = character.camera_offsets.unwrap_or((0.0, 0.0));
    let flip_x = resolved_flip_x(role, character.flip_x);

    // Native `CharacterDataParser` normalization (`CharacterData.hx`
    // L531-586): id defaults to the character id, shouldBop true, scale 1,
    // flipX false, offsets [0,0], isPixel inherits the character flag.
    let health_icon_value = character.health_icon.as_ref();
    let health_icon = super::hud::IconConfig {
        id: health_icon_value
            .and_then(|value| value.get("id"))
            .and_then(|value| value.as_str())
            .unwrap_or(id)
            .to_string(),
        is_pixel: health_icon_value
            .and_then(|value| value.get("isPixel"))
            .and_then(|value| value.as_bool())
            .unwrap_or(is_pixel),
        should_bop: health_icon_value
            .and_then(|value| value.get("shouldBop"))
            .and_then(|value| value.as_bool())
            .unwrap_or(true),
        scale: health_icon_value
            .and_then(|value| value.get("scale"))
            .and_then(|value| value.as_f64())
            .unwrap_or(1.0),
        flip_x: health_icon_value
            .and_then(|value| value.get("flipX"))
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        offsets: health_icon_value
            .and_then(|value| value.get("offsets"))
            .and_then(|value| value.as_array())
            .and_then(|values| {
                let x = values.first()?.as_f64()?;
                let y = values.get(1)?.as_f64()?;
                Some([x, y])
            })
            .unwrap_or([0.0, 0.0]),
    };

    Ok(LoadedCharacter {
        role,
        id: id.to_string(),
        sheets,
        bounds,
        sprite_hitbox,
        animations,
        data_offsets,
        camera_offsets,
        sing_time_steps: character.sing_time.unwrap_or(DEFAULT_SING_TIME_STEPS),
        dance_every: character.dance_every.unwrap_or(DEFAULT_DANCE_EVERY),
        scale: character.scale.unwrap_or(1.0) * slot.scale,
        flip_x,
        starting_animation: character.starting_animation.clone(),
        slot,
        health_icon,
    })
}

#[derive(Debug, Clone, Copy)]
enum SpriteAnimationMode {
    /// `addByIndices(Prefix, Indices)` / `addByPrefix`: data indices address
    /// the prefix-matched frame list.
    Prefix,
    /// Packer stage props register `animation.add(name, frameIndices)`
    /// directly against the whole page frame list (Stage.hx L284-300).
    PackerDirect,
}

fn frame_rate(animation: &AnimationData) -> f64 {
    animation
        .frame_rate
        .filter(|rate| *rate > 0.0)
        .unwrap_or(DEFAULT_ANIMATION_FPS)
}

fn sprite_animation_def(
    animation: &AnimationData,
    sheet: &SpriteSheet,
    sheet_index: usize,
    mode: SpriteAnimationMode,
    owner: &str,
) -> Result<AnimationDef, RenderError> {
    let prefix = animation.prefix.clone();
    let (frames, symbol) = match mode {
        SpriteAnimationMode::PackerDirect
            if animation
                .frame_indices
                .as_ref()
                .is_some_and(|indices| !indices.is_empty()) =>
        {
            let mut frames = Vec::new();
            for index in animation.frame_indices.iter().flatten() {
                if let Ok(index) = usize::try_from(*index)
                    && sheet.frame(index).is_some()
                {
                    frames.push(index);
                }
            }
            (frames, None)
        }
        _ => {
            let prefix = prefix.clone().ok_or_else(|| RenderError::MissingField {
                path: owner.to_string(),
                field: "animations.prefix",
            })?;
            let all = sheet.prefix_indices(&prefix);
            if all.is_empty() {
                return Err(RenderError::MissingFrame {
                    path: owner.to_string(),
                    prefix,
                });
            }
            let frames = match &animation.frame_indices {
                Some(indices) if !indices.is_empty() => indices
                    .iter()
                    .filter_map(|index| usize::try_from(*index).ok())
                    .filter_map(|index| all.get(index).copied())
                    .collect::<Vec<_>>(),
                _ => all,
            };
            (frames, None)
        }
    };
    if frames.is_empty() {
        return Err(RenderError::MissingFrame {
            path: owner.to_string(),
            prefix: prefix.unwrap_or_else(|| animation.name.clone()),
        });
    }
    Ok(AnimationDef {
        frames,
        frame_rate: frame_rate(animation),
        looped: animation.looped.unwrap_or(false),
        offsets: animation.offsets,
        sheet: sheet_index,
        symbol,
        flip_x: animation.flip_x.unwrap_or(false),
        flip_y: animation.flip_y.unwrap_or(false),
    })
}

fn atlas_animation_def(
    animation: &AnimationData,
    sheet: &AtlasSheet,
    sheet_index: usize,
    owner: &str,
) -> Result<AnimationDef, RenderError> {
    let prefix = animation
        .prefix
        .clone()
        .ok_or_else(|| RenderError::MissingField {
            path: owner.to_string(),
            field: "animations.prefix",
        })?;
    let symbol = animation.anim_type.as_deref() == Some("symbol");
    let mut frames = if symbol {
        // `animType: "symbol"` plays the symbol's own timeline. The shipped
        // `AN.SN` main-symbol name (e.g. `BOPPERS_EXPORT`, `rando`) is not in
        // the SD symbol dictionary; its playback is the main stage timeline.
        match sheet.stage.symbol_frame_count(&prefix) {
            Some(count) => (0..count).collect::<Vec<_>>(),
            None => match sheet.stage.label_frames(&prefix) {
                Some(frames) => frames,
                None => (0..sheet.stage.frame_count()).collect(),
            },
        }
    } else {
        sheet
            .stage
            .label_frames(&prefix)
            .ok_or_else(|| RenderError::MissingAnimationLabel {
                character: owner.to_string(),
                animation: animation.name.clone(),
                prefix: prefix.clone(),
            })?
    };
    if let Some(indices) = &animation.frame_indices
        && !indices.is_empty()
    {
        frames = indices
            .iter()
            .filter_map(|index| usize::try_from(*index).ok())
            .filter_map(|index| frames.get(index).copied())
            .collect();
    }
    if frames.is_empty() {
        return Err(RenderError::MissingAnimationLabel {
            character: owner.to_string(),
            animation: animation.name.clone(),
            prefix,
        });
    }
    Ok(AnimationDef {
        frames,
        frame_rate: frame_rate(animation),
        looped: animation.looped.unwrap_or(false),
        offsets: animation.offsets,
        sheet: sheet_index,
        symbol: symbol.then_some(prefix),
        flip_x: animation.flip_x.unwrap_or(false),
        flip_y: animation.flip_y.unwrap_or(false),
    })
}

fn load_sparrow_sheet(
    vfs: &Vfs,
    pages: &mut PageStore,
    asset_path: &str,
    level_dir: Option<&str>,
    is_pixel: bool,
) -> Result<SpriteSheet, RenderError> {
    let bases = image_dir_candidates(asset_path, level_dir);
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
    // Native `Paths.getSparrowAtlas` pairs the key with `<image>.png`; the
    // XML `imagePath` attribute is informational (picoPixel points at a
    // `spritesheet.png` that does not exist in the shipped tree).
    let image_path = format!("{base}.png");
    let (page, page_size) = pages.load_png_file(vfs, &image_path, is_pixel)?;
    let mut frames = Vec::with_capacity(atlas.subtextures.len());
    for subtexture in &atlas.subtextures {
        let quad = frame_quad(subtexture);
        frames.push((
            subtexture.name.clone(),
            SpriteFrame {
                page,
                // `frame_quad.atlas_rect` swaps rotated page dimensions to the
                // sprite footprint; keep the STORED page rect for UV sampling
                // and let the renderer transpose it with the tile matrix.
                rect: if subtexture.rotated {
                    [
                        subtexture.x,
                        subtexture.y,
                        subtexture.width,
                        subtexture.height,
                    ]
                } else {
                    quad.atlas_rect
                },
                dest_offset: quad.dest_offset,
                dest_size: quad.dest_size,
                rotated: subtexture.rotated,
            },
        ));
    }
    Ok(SpriteSheet {
        page,
        page_size,
        frames,
    })
}

fn load_packer_sheet(
    vfs: &Vfs,
    pages: &mut PageStore,
    asset_path: &str,
    level_dir: Option<&str>,
    is_pixel: bool,
) -> Result<SpriteSheet, RenderError> {
    let bases = image_dir_candidates(asset_path, level_dir);
    let mut found = None;
    for base in &bases {
        let text_path = format!("{base}.txt");
        if let Some(source) = vfs.resolve(&text_path) {
            let bytes =
                vfs.read(&source, ASSET_SIZE_LIMIT)
                    .map_err(|source| RenderError::Read {
                        path: text_path.clone(),
                        source,
                    })?;
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
            let text = String::from_utf8(bytes.to_vec()).map_err(|error| RenderError::Decode {
                path: text_path.clone(),
                detail: error.to_string(),
            })?;
            found = Some((base.clone(), text_path, text));
            break;
        }
    }
    let Some((base, text_path, text)) = found else {
        return Err(RenderError::MissingAsset {
            path: format!("{}.txt", bases.first().map_or("<none>", String::as_str)),
        });
    };
    let atlas = parse_packer_text(&text).map_err(|source| RenderError::Packer {
        path: text_path,
        source,
    })?;
    let (page, page_size) = pages.load_png_file(vfs, &format!("{base}.png"), is_pixel)?;
    let mut frames = Vec::with_capacity(atlas.frames.len());
    for frame in &atlas.frames {
        frames.push((
            frame.name.clone(),
            SpriteFrame {
                page,
                rect: frame.frame,
                dest_offset: [0, 0],
                dest_size: frame.source_size,
                rotated: false,
            },
        ));
    }
    Ok(SpriteSheet {
        page,
        page_size,
        frames,
    })
}

fn load_atlas_sheet(
    vfs: &Vfs,
    pages: &mut PageStore,
    asset_path: &str,
    level_dir: Option<&str>,
    is_pixel: bool,
) -> Result<AtlasSheet, RenderError> {
    let dirs = image_dir_candidates(asset_path, level_dir);
    let (spritemap_path, spritemap_text) = read_text_in_dirs(vfs, &dirs, "spritemap1.json")?;
    let spritemap = parse_animate(&spritemap_text).map_err(|source| RenderError::Sparrow {
        path: spritemap_path.clone(),
        source,
    })?;
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
            detail: format!("animate atlas {asset_path:?} has empty/non-finite bounds {bounds:?}"),
        });
    }
    let image_name = if spritemap.image.is_empty() {
        "spritemap1.png".to_string()
    } else {
        spritemap.image
    };
    let (page, page_size) = pages.load_png(vfs, &dirs, &image_name, is_pixel)?;
    Ok(AtlasSheet {
        stage,
        bounds,
        page,
        page_size,
    })
}

struct StyleAsset {
    asset_path: Option<String>,
    scale: Option<f64>,
    offsets: Option<(f64, f64)>,
    is_pixel: bool,
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
            is_pixel: value
                .get("isPixel")
                .and_then(|is_pixel| is_pixel.as_bool())
                .unwrap_or(false),
            prefixes,
        })
    };
    Ok(NoteStyleAssets {
        note: parse("note")?,
        strumline: parse("noteStrumline")?,
        hold: parse("holdNote")?,
    })
}

/// First frame of the sheet in DOCUMENT order (the frame current at the
/// spriting setup call that runs `updateHitbox()`; FlxSprite `set_frame` ->
/// `resetFrameSize()` tracks the CURRENT frame while the hitbox `width`
/// stays frozen at this call's value).
fn first_frame_dest(sheet: &SpriteSheet, style_path: &str) -> Result<[f64; 2], RenderError> {
    let dest = sheet
        .first_dest()
        .ok_or_else(|| RenderError::MissingAsset {
            path: format!("{style_path}.xml"),
        })?;
    Ok([dest[0] as f64, dest[1] as f64])
}

fn require_frames(
    sheet: &SpriteSheet,
    prefix: &str,
    style_path: &str,
) -> Result<Vec<SpriteFrame>, RenderError> {
    let frames = sheet
        .prefix_indices(prefix)
        .into_iter()
        .filter_map(|index| sheet.frame(index))
        .collect::<Vec<_>>();
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
    level_dir: Option<&str>,
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
    // (LINEAR sampling). The pixel style flags all three pages as nearest.
    let note_page = load_sparrow_page(vfs, pages, &note_asset, level_dir, assets.note.is_pixel)?;
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
    let strumline_page = load_sparrow_page(
        vfs,
        pages,
        &strumline_asset,
        level_dir,
        assets.strumline.is_pixel,
    )?;
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
    let (hold_page, hold_page_size) =
        pages.load_image_key(vfs, &hold_asset, level_dir, assets.hold.is_pixel)?;
    if hold_page_size[0] <= 0.0
        || hold_page_size[1] <= 0.0
        || !(hold_page_size[0] as usize).is_multiple_of(super::geometry::HOLD_TEXTURE_COLUMNS)
    {
        return Err(RenderError::Unsupported {
            detail: format!(
                "hold sheet is {hold_page_size:?}, expected a positive width divisible by {}",
                super::geometry::HOLD_TEXTURE_COLUMNS
            ),
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
    let hold_scale = assets.hold.scale.unwrap_or(1.0);
    let hold_width = super::geometry::sustain_width(hold_page_size[0], hold_scale);
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
        hold_page_size,
        hold_scale,
        hold_offsets: assets.hold.offsets.unwrap_or((0.0, 0.0)),
        hold_width,
    })
}

/// Loads one Sparrow page through the shared sprite-sheet representation.
fn load_sparrow_page(
    vfs: &Vfs,
    pages: &mut PageStore,
    asset_path: &str,
    level_dir: Option<&str>,
    is_pixel: bool,
) -> Result<SpriteSheet, RenderError> {
    load_sparrow_sheet(vfs, pages, asset_path, level_dir, is_pixel)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    /// Minimal temp asset root for synthetic loader fixtures (no real assets).
    struct TempAssets {
        root: PathBuf,
    }

    impl TempAssets {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "oxide-game-render-load-test-{}-{}",
                std::process::id(),
                NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&root).expect("create temp asset root");
            Self { root }
        }

        fn write(&self, path: &str, bytes: &[u8]) {
            let path = self.root.join(path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create temp asset directory");
            }
            fs::write(path, bytes).expect("write temp asset");
        }
    }

    impl Drop for TempAssets {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// 1x1 RGBA PNG used by the static-texture loader path.
    const PNG_1X1: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn test_prop(
        name: &str,
        asset_path: &str,
        dance_every: Option<f64>,
        animations: Option<Vec<AnimationData>>,
    ) -> StageProp {
        StageProp {
            name: name.to_string(),
            asset_path: Some(asset_path.to_string()),
            dance_every,
            z_index: Some(0.0),
            position: Some((0.0, 0.0)),
            scale: Some((1.0, 1.0)),
            alpha: None,
            angle: None,
            anim_type: None,
            scroll: Some((1.0, 1.0)),
            animations,
            is_pixel: None,
            flip_x: None,
            flip_y: None,
            blend: None,
            color: None,
            starting_animation: None,
            atlas_settings: None,
        }
    }

    fn stage_with_props(props: Vec<StageProp>) -> StageData {
        StageData {
            version: None,
            name: "test stage".to_string(),
            camera_zoom: None,
            directory: Some("week1".to_string()),
            characters: None,
            props,
        }
    }

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

    #[test]
    fn render_type_dispatch_covers_the_inventory_and_fails_closed() {
        let dispatch =
            |value: Option<&str>| resolve_render_type(value, "preload/data/characters/x.json");
        assert_eq!(dispatch(None).ok(), Some(CharacterRenderType::Sparrow));
        assert_eq!(
            dispatch(Some("animateatlas")).ok(),
            Some(CharacterRenderType::AnimateAtlas)
        );
        assert_eq!(
            dispatch(Some("multianimateatlas")).ok(),
            Some(CharacterRenderType::MultiAnimateAtlas)
        );
        assert_eq!(
            dispatch(Some("multisparrow")).ok(),
            Some(CharacterRenderType::MultiSparrow)
        );
        assert_eq!(
            dispatch(Some("sparrow")).ok(),
            Some(CharacterRenderType::Sparrow)
        );
        assert_eq!(
            dispatch(Some("packer")).ok(),
            Some(CharacterRenderType::Packer)
        );
        let error = dispatch(Some("custom")).expect_err("unknown renderType fails closed");
        assert!(
            matches!(
                &error,
                RenderError::UnknownRenderType { path, render_type }
                    if path == "preload/data/characters/x.json" && render_type == "custom"
            ),
            "{error}"
        );
        assert!(error.to_string().contains("preload/data/characters/x.json"));
    }

    #[test]
    fn prop_classification_follows_animation_or_dance_every() {
        let base = |dance_every: Option<f64>, animations: Option<Vec<AnimationData>>| StageProp {
            name: "prop".to_string(),
            asset_path: Some("key".to_string()),
            dance_every,
            z_index: None,
            position: None,
            scale: None,
            alpha: None,
            angle: None,
            anim_type: None,
            scroll: None,
            animations,
            is_pixel: None,
            flip_x: None,
            flip_y: None,
            blend: None,
            color: None,
            starting_animation: None,
            atlas_settings: None,
        };
        assert!(!is_bopper_prop(&base(None, None)));
        assert!(!is_bopper_prop(&base(Some(0.0), Some(Vec::new()))));
        assert!(is_bopper_prop(&base(Some(1.0), None)));
        assert!(is_bopper_prop(&base(
            Some(0.0),
            Some(vec![AnimationData {
                name: "idle".to_string(),
                prefix: Some("idle".to_string()),
                offsets: None,
                looped: None,
                frame_rate: None,
                flip_x: None,
                flip_y: None,
                anim_type: None,
                library: None,
                frame_indices: None,
                asset_path: None,
                render_type: None,
            }])
        )));
    }

    #[test]
    fn solid_props_store_integer_make_solid_color_dimensions() {
        let assets = TempAssets::new();
        let cases = [
            (Some((2400.0, 2000.0)), [2400, 2000]),
            // Haxe Std.int truncates toward zero.
            (Some((2400.9, 2000.9)), [2400, 2000]),
            (None, [1, 1]),
        ];
        for (scale, expected) in cases {
            let mut prop = test_prop("solid", "#FF8000", None, None);
            prop.scale = scale;
            let stage = stage_with_props(vec![prop]);
            let mut pages = PageStore::default();
            let props = load_stage_props(
                &Vfs::new(assets.root.clone()),
                &mut pages,
                &stage,
                "test-stage",
                Some("week1"),
            )
            .expect("solid prop loads without a page");
            assert_eq!(props.len(), 1);
            assert!(!props[0].bopper, "a plain solid prop is not a Bopper");
            match &props[0].visual {
                PropVisual::Solid { size, color } => {
                    assert_eq!(*size, expected, "integer makeSolidColor dimensions");
                    assert_eq!(*color, [1.0, 128.0 / 255.0, 0.0, 1.0]);
                }
                _ => panic!("solid assetPath must load as PropVisual::Solid"),
            }
            assert!(
                pages.pages.is_empty(),
                "a solid prop consumes no texture page"
            );
        }
    }

    #[test]
    fn bopper_without_animations_loads_a_static_texture() {
        let assets = TempAssets::new();
        assets.write("week1/images/bg.png", PNG_1X1);
        // Native Stage.hx: the Bopper class decision (danceEvery != 0 ||
        // animations non-empty) is separate from the sheet-loading decision
        // (animations non-empty). A danceEvery-only Bopper is static.
        let stage = stage_with_props(vec![
            test_prop("danceOnly", "bg", Some(1.0), None),
            test_prop("emptyAnimations", "bg", Some(0.0), Some(Vec::new())),
        ]);
        let mut pages = PageStore::default();
        let props = load_stage_props(
            &Vfs::new(assets.root.clone()),
            &mut pages,
            &stage,
            "test-stage",
            Some("week1"),
        )
        .expect("danceEvery-only Bopper loads its static texture");
        assert_eq!(props.len(), 2);
        assert!(props[0].bopper, "danceEvery-only prop keeps its scheduler");
        assert!(
            matches!(&props[0].visual, PropVisual::Static { .. }),
            "empty animations must load a static texture, not a sheet"
        );
        assert!(!props[1].bopper);
        assert!(matches!(&props[1].visual, PropVisual::Static { .. }));
        assert_eq!(pages.pages.len(), 1, "the shared key loads one page");
    }

    #[test]
    fn atlas_settings_apply_stage_matrix_is_read_from_the_prop() {
        let mut prop = test_prop("sniper", "sniper", None, None);
        assert!(!applies_stage_matrix(&prop));
        prop.atlas_settings = Some(serde_json::json!({ "applyStageMatrix": true }));
        assert!(applies_stage_matrix(&prop));
        prop.atlas_settings = Some(serde_json::json!({ "applyStageMatrix": false }));
        assert!(!applies_stage_matrix(&prop));
        prop.atlas_settings = Some(serde_json::json!({ "applyStageMatrix": "yes" }));
        assert!(!applies_stage_matrix(&prop));
        prop.atlas_settings = Some(serde_json::json!({}));
        assert!(!applies_stage_matrix(&prop));
    }

    #[test]
    fn solid_color_props_parse_rrggbb_and_reject_malformed_hex() {
        assert_eq!(
            parse_solid_color("#FF8000")
                .expect("valid")
                .expect("parsed"),
            [1.0, 128.0 / 255.0, 0.0, 1.0]
        );
        assert!(parse_solid_color("stageback").is_none());
        assert!(parse_solid_color("#12345").is_some_and(|result| result.is_err()));
        assert!(parse_solid_color("#GGGGGG").is_some_and(|result| result.is_err()));
    }

    #[test]
    fn plain_and_library_keys_follow_the_observed_candidate_order() {
        assert_eq!(
            image_file_bases("shared:characters/bf", Some("week1")),
            [
                "shared/images/characters/bf".to_string(),
                "preload/images/characters/bf".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("week6:weeb/pixelUI/arrows-pixels", Some("week6")),
            [
                "week6/images/weeb/pixelUI/arrows-pixels".to_string(),
                "shared/images/weeb/pixelUI/arrows-pixels".to_string(),
                "preload/images/weeb/pixelUI/arrows-pixels".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("stageback", Some("week1")),
            [
                "week1/images/stageback".to_string(),
                "shared/images/stageback".to_string(),
                "preload/images/stageback".to_string()
            ]
        );
        assert_eq!(
            image_file_bases("stageback", None),
            [
                "shared/images/stageback".to_string(),
                "preload/images/stageback".to_string()
            ]
        );
    }

    #[test]
    fn sprite_prefix_selection_is_document_order_and_add_by_indices_reorders() {
        let page = |index: usize| SpriteFrame {
            page: 0,
            rect: [index as i32 * 10, 0, 10, 10],
            dest_offset: [0, 0],
            dest_size: [10, 10],
            rotated: false,
        };
        let sheet = SpriteSheet {
            page: 0,
            page_size: [100.0, 100.0],
            frames: vec![
                ("walk1".to_string(), page(0)),
                ("idle1".to_string(), page(1)),
                ("walk2".to_string(), page(2)),
                ("walk0".to_string(), page(3)),
            ],
        };
        assert_eq!(sheet.prefix_indices("walk"), vec![0, 2, 3]);
        let animation = |indices: Option<Vec<i64>>| AnimationData {
            name: "walk".to_string(),
            prefix: Some("walk".to_string()),
            offsets: None,
            looped: None,
            frame_rate: None,
            flip_x: None,
            flip_y: None,
            anim_type: None,
            library: None,
            frame_indices: indices,
            asset_path: None,
            render_type: None,
        };
        let def = sprite_animation_def(
            &animation(None),
            &sheet,
            0,
            SpriteAnimationMode::Prefix,
            "test",
        )
        .expect("prefix animation");
        assert_eq!(def.frames, [0, 2, 3]);
        let def = sprite_animation_def(
            &animation(Some(vec![2, 0])),
            &sheet,
            0,
            SpriteAnimationMode::Prefix,
            "test",
        )
        .expect("addByIndices animation");
        assert_eq!(def.frames, [3, 0], "data index order is preserved");
        let def = sprite_animation_def(
            &AnimationData {
                prefix: None,
                ..animation(Some(vec![3, 1]))
            },
            &sheet,
            0,
            SpriteAnimationMode::PackerDirect,
            "test",
        )
        .expect("packer direct animation");
        assert_eq!(def.frames, [3, 1]);
    }

    #[test]
    fn song_ids_resolve_default_variants_and_reject_invalid_parts() {
        assert_eq!(
            song_paths("bopeebo-default").expect("default"),
            (
                "preload/data/songs/bopeebo/bopeebo-chart.json".to_string(),
                "preload/data/songs/bopeebo/bopeebo-metadata.json".to_string()
            )
        );
        assert_eq!(
            song_paths("bopeebo").expect("bare"),
            song_paths("bopeebo-default").expect("default")
        );
        assert_eq!(
            song_paths("bopeebo-erect").expect("variant"),
            (
                "preload/data/songs/bopeebo/bopeebo-chart-erect.json".to_string(),
                "preload/data/songs/bopeebo/bopeebo-metadata-erect.json".to_string()
            )
        );
        assert_eq!(
            song_paths("philly-nice").expect("hyphenated song"),
            (
                "preload/data/songs/philly-nice/philly-nice-chart.json".to_string(),
                "preload/data/songs/philly-nice/philly-nice-metadata.json".to_string()
            )
        );
        assert!(song_paths("../bopeebo").is_err());
    }

    /// Explicit 6B inventory check: every corpus chart id loads its stage,
    /// characters and note style without an error path. This is a load-only
    /// check (no GPU/Bevy app); run with `--ignored` and an explicit assets
    /// root.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the 6B load inventory"]
    fn load_all_corpus_songs() {
        let root = std::env::var_os("OXIDE_ASSETS_ROOT")
            .map(std::path::PathBuf::from)
            .expect("set OXIDE_ASSETS_ROOT to the reference assets directory");
        let corpus = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tools/oracle/corpus.txt"
        ))
        .expect("read corpus.txt");
        let mut stage_ids = std::collections::BTreeSet::new();
        let mut stage_prop_counts: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut stage_matrix_props = std::collections::BTreeSet::new();
        let mut loaded = 0usize;
        for chart_id in corpus
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
        {
            let (_, metadata_path) = song_paths(chart_id).expect("chart id resolves");
            let metadata: serde_json::Value = serde_json::from_slice(
                &read_asset(&Vfs::new(root.clone()), &metadata_path).expect("read metadata"),
            )
            .expect("metadata json");
            let difficulty = metadata["playData"]["difficulties"]
                .as_array()
                .and_then(|difficulties| difficulties.first())
                .and_then(|difficulty| difficulty.as_str())
                .unwrap_or("easy")
                .to_string();
            let slice = load_slice_for_song(&root, chart_id, &difficulty).unwrap_or_else(|error| {
                panic!("{chart_id} ({difficulty}) failed to load: {error}")
            });
            // The stage name comes from the same metadata the loader consumed.
            let stage = metadata["playData"]["stage"]
                .as_str()
                .expect("metadata stage")
                .to_string();
            stage_ids.insert(stage.clone());
            stage_prop_counts
                .entry(stage.clone())
                .or_insert(slice.props.len());
            for prop in &slice.props {
                if matches!(
                    &prop.visual,
                    PropVisual::Atlas {
                        apply_stage_matrix: true,
                        ..
                    }
                ) {
                    stage_matrix_props.insert(format!("{stage}/{}", prop.name));
                }
            }
            if stage == "sserafim" {
                // Observed sserafim.json: solidCover is alpha 0.0 at zIndex
                // 9999; it must load (inventory) with the integer
                // makeSolidColor dimensions, never as an opaque overlay.
                let cover = slice
                    .props
                    .iter()
                    .find(|prop| prop.name == "solidCover")
                    .expect("sserafim solidCover prop");
                assert_eq!(cover.alpha, 0.0);
                assert_eq!(cover.z_index, 9999.0);
                assert!(matches!(
                    &cover.visual,
                    PropVisual::Solid {
                        size: [10000, 10000],
                        ..
                    }
                ));
            }
            loaded += 1;
            // Observed `tutorial-default` metadata has no girlfriend id; the
            // native default `''` loads no girlfriend character. All other
            // corpus songs load player+opponent+girlfriend.
            let expected_girlfriend = metadata["playData"]["characters"]["girlfriend"]
                .as_str()
                .is_some_and(|id| !id.is_empty());
            let roles: Vec<Role> = slice.characters.iter().map(|c| c.role).collect();
            assert!(roles.contains(&Role::Player), "{chart_id} player");
            assert!(roles.contains(&Role::Opponent), "{chart_id} opponent");
            assert_eq!(
                roles.contains(&Role::Girlfriend),
                expected_girlfriend,
                "{chart_id} girlfriend presence"
            );
            assert_eq!(
                slice.characters.len(),
                2 + usize::from(expected_girlfriend),
                "{chart_id} characters"
            );
        }
        assert_eq!(loaded, 61);
        assert_eq!(stage_ids.len(), 21, "stage ids: {stage_ids:?}");
        assert_eq!(
            stage_prop_counts.values().sum::<usize>(),
            169,
            "per-stage prop counts: {stage_prop_counts:?}"
        );
        assert_eq!(
            stage_matrix_props.len(),
            3,
            "applyStageMatrix props: {stage_matrix_props:?}"
        );
    }
}
