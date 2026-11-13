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
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use oxide_atlas::{AnimateStage, frame_quad, parse_animate, parse_packer_text, parse_sparrow};
use oxide_data::Typed;
use oxide_data::character::AnimationData;
use oxide_data::stage::{StageData, StageProp};
use oxide_vfs::Vfs;

use super::RenderError;
use super::anim::{
    AnimationDef, DEFAULT_ANIMATION_FPS, DEFAULT_DANCE_EVERY, DEFAULT_SING_TIME_STEPS,
};
use super::choreography::{CanClip, Weekend1Props};
use super::shaders::ShaderMaskPages;

/// Upper bound for any single slice asset read, matching the M5 slice VFS
/// read contract. The largest shipped page in the slice is a few MiB.
pub(crate) const ASSET_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

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

impl PageImage {
    /// Builds a page from straight-alpha RGBA8 bytes.
    ///
    /// Used for the runtime-baked dialogue glyph pages: the loader page store
    /// only decodes files, while `raster_glyph_page` returns raw coverage
    /// pages. `FunkinTypeText` uses flixel's default sampling, so these pages
    /// are always LINEAR; the dialogue box's `isPixel` applies to its sprite
    /// page only.
    pub(crate) fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Result<Self, RenderError> {
        let path = "<runtime glyph page>".to_string();
        if width == 0 || height == 0 {
            return Err(RenderError::Image {
                path,
                detail: "runtime glyph page has a zero dimension".to_string(),
            });
        }
        let expected = (width as usize)
            .saturating_mul(height as usize)
            .saturating_mul(4);
        if rgba.len() != expected {
            return Err(RenderError::Image {
                path,
                detail: format!(
                    "runtime glyph page is {width}x{height} but carries {} bytes (expected {expected})",
                    rgba.len()
                ),
            });
        }
        let mut image = Image::new(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            rgba,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = sampler_for(false);
        Ok(Self { image })
    }
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
    pub(crate) fn prefix_indices(&self, prefix: &str) -> Vec<usize> {
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
    /// Whole-timeline bounds (`Timeline.getWholeBounds`) for each `animType:
    /// "symbol"` symbol referenced in the character or prop's animations.
    pub(crate) symbol_bounds: BTreeMap<String, [f64; 4]>,
    pub(crate) page: usize,
    pub(crate) page_size: [f64; 2],
}

impl AtlasSheet {
    /// Whole-timeline bounds for the active animation (`symbol` when defined in
    /// the sheet's symbol dictionary, else the main timeline). Matches
    /// `FlxAnimate`'s `timeline._bounds`.
    pub(crate) fn timeline_bounds(&self, symbol: Option<&str>) -> [f64; 4] {
        symbol
            .and_then(|name| self.symbol_bounds.get(name).copied())
            .unwrap_or(self.bounds)
    }
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
    /// Setup-time `updateHitbox` frame dimensions at scale 1, in the idle
    /// animation's timeline/frame space (`BaseCharacter.hx` L258-268, L300-309):
    /// Sparrow/Packer frames contribute their frame `dest_size`; Animate atlases
    /// contribute their active timeline's whole bounds (`timeline._bounds`,
    /// `FlxAnimateController.updateTimelineBounds`). This is the
    /// `characterOrigin` basis (`width/2`, `height` before scaling) and the
    /// vertical camera-focus basis (`height * scale / 2`).
    pub(crate) idle_size: [f64; 2],
    /// Setup-time Sparrow/Packer hitbox (`updateHitbox` frame box): the idle
    /// animation's first frame when it lives on a Sparrow/Packer sheet.
    /// `None` when the idle animation is an animate atlas; the draw path then
    /// keeps the historical first-document-frame fallback.
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
#[derive(Debug, Clone)]
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
    /// Base song id (`<song>` before a known `-variant` suffix). The 6G
    /// rain stages select their per-song intensity range from it.
    pub(crate) song_id: String,
    /// `playData.stage`: selects the frozen per-sprite shader attachments.
    pub(crate) stage_id: String,
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
    /// Loaded 6E dialogue payload. The frozen render-fixture path leaves this
    /// `None` (bopeebo/school frame digests must not change); the explicit
    /// dialogue-enabled loader fills it for the senpai family.
    pub(crate) dialogue: Option<super::dialogue::LoadedDialogue>,
    /// Loaded weekend-1 (2hot) can/explosion pages. Only the weekend-1 family
    /// ids fill this (`weekend1_prop_pages`); every other chart leaves it
    /// `None`, so the frozen bopeebo/school page sets are untouched.
    pub(crate) weekend1_props: Option<Weekend1Props>,
    /// `darnellCanCutscene` conductor time changes (M6-CONTRACT §14.1).
    /// Only the darnell family fills this; the scene's cutscene-local
    /// conductor consumes it.
    pub(crate) cutscene_changes: Option<Vec<oxide_audio::conductor::TimeChange>>,
    /// Alternate-mask pages for the two DropShadow stages (M6-CONTRACT
    /// §15.1). Empty for every other stage, so their page sets stay frozen.
    pub(crate) shader_masks: ShaderMaskPages,
    /// Loaded `ABotSpeaker` assembly when a matching speaker character is
    /// present (`nene*` or `otis-speaker`, `docs/M6-CONTRACT.md` §18).
    pub(crate) abot: Option<super::abot::LoadedABot>,
    /// Whether Bad/Shit player hits render the scrolling HSV-desaturated note
    /// overlay. Enabled by the interactive play loader and by the generalized
    /// non-bopeebo fixture path; disabled only by the frozen bopeebo
    /// render-diff loader whose scripted negative phase deliberately contains
    /// Bad/Shit threshold hits.
    pub(crate) note_desaturation: bool,
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
/// default path) with the interactive note-desaturation overlay enabled.
pub(crate) fn load_slice(assets_root: &Path, difficulty: &str) -> Result<LoadedSlice, RenderError> {
    load_slice_for_song_mode(assets_root, BOPEEBO_CHART_ID, difficulty, true)
}

/// The headless [`crate::render::SliceRenderer`] bopeebo path: same assets as
/// [`load_slice`], but with the note-desaturation overlay disabled so the
/// frozen frame fixtures stay byte-identical.
pub(crate) fn load_slice_for_fixture(
    assets_root: &Path,
    difficulty: &str,
) -> Result<LoadedSlice, RenderError> {
    load_slice_for_song_mode(assets_root, BOPEEBO_CHART_ID, difficulty, false)
}

/// Builds the same scene surface for any `corpus.txt`-style chart id
/// (`<song>`, `<song>-default`, or a known `<song>-<variation>`).
///
/// This is the frozen render-fixture path: dialogue stays unloaded so the
/// established bopeebo/school/dialogue frame digests are byte-identical.
/// `bopeebo` keeps the interactive note-desaturation overlay disabled because
/// its scripted negative phase deliberately contains Bad/Shit threshold hits;
/// every other render-diff family uses chart taps with only Sick hits, so the
/// overlay stays empty there and their digests are unchanged. The play path
/// ([`load_slice`]) also enables it.
pub(crate) fn load_slice_for_song(
    assets_root: &Path,
    chart_id: &str,
    difficulty: &str,
) -> Result<LoadedSlice, RenderError> {
    load_slice_for_song_mode(
        assets_root,
        chart_id,
        difficulty,
        fixture_note_desaturation(chart_id),
    )
}

/// The generalized fixture overlay rule: only the bare `bopeebo` chart id keeps
/// the Bad/Shit HSV note overlay disabled. Other families never emit Bad/Shit
/// hits in the frozen replay, so enabling it leaves their digests untouched.
fn fixture_note_desaturation(chart_id: &str) -> bool {
    chart_id != "bopeebo"
}

fn load_slice_for_song_mode(
    assets_root: &Path,
    chart_id: &str,
    difficulty: &str,
    note_desaturation: bool,
) -> Result<LoadedSlice, RenderError> {
    let (chart_path, metadata_path) = song_paths(chart_id)?;
    let (song_id, _) = split_song_variant(chart_id);
    load_slice_from_paths(
        assets_root,
        &chart_path,
        &metadata_path,
        difficulty,
        None,
        weekend1_prop_pages(chart_id),
        darnell_cutscene_chart(chart_id),
        note_desaturation,
        song_id,
    )
}

/// Dialogue-enabled variant of [`load_slice_for_song`] (6E). The chart id is
/// mapped through the shipped senpai-family table; ids outside that table
/// simply load without a dialogue payload. The frozen fixture entry point
/// above deliberately never calls this; the dialogue render family does.
pub(crate) fn load_slice_for_song_with_dialogue(
    assets_root: &Path,
    chart_id: &str,
    difficulty: &str,
) -> Result<LoadedSlice, RenderError> {
    let (chart_path, metadata_path) = song_paths(chart_id)?;
    let dialogue_id = dialogue_load_id(chart_id, true);
    let (song_id, _) = split_song_variant(chart_id);
    load_slice_from_paths(
        assets_root,
        &chart_path,
        &metadata_path,
        difficulty,
        dialogue_id,
        weekend1_prop_pages(chart_id),
        darnell_cutscene_chart(chart_id),
        false,
        song_id,
    )
}

/// Maps a corpus-style chart id to its chart/metadata paths. Mirrors the 6A
/// `build_chart` iding: only the known corpus variations are split off, so
/// hyphenated song names stay intact.
fn song_paths(chart_id: &str) -> Result<(String, String), RenderError> {
    let (song, variant) = split_song_variant(chart_id);
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

/// Splits a chart id into `(song, variation)`. Only the known corpus
/// variations are recognised, so hyphenated song names stay intact.
fn split_song_variant(chart_id: &str) -> (&str, &str) {
    const VARIATIONS: [&str; 4] = ["default", "erect", "pico", "bf"];
    chart_id
        .rsplit_once('-')
        .filter(|(_, variant)| VARIATIONS.contains(variant))
        .unwrap_or((chart_id, "default"))
}

/// Shipped week-6 dialogue family (uncensored only): the same song/variant
/// table the choreography triggers use (`slice/choreography.rs`).
fn dialogue_conversation_id(chart_id: &str) -> Option<&'static str> {
    let (song, variant) = split_song_variant(chart_id);
    match song {
        "senpai" if variant == "pico" => Some("senpai-pico"),
        "senpai" => Some("senpai"),
        "roses" if variant == "pico" => Some("roses-pico"),
        "roses" => Some("roses"),
        "thorns" => Some("thorns"),
        _ => None,
    }
}

/// Dialogue id for a loader call. `enabled == false` is the frozen render
/// fixture path and always yields `None`.
fn dialogue_load_id(chart_id: &str, enabled: bool) -> Option<&'static str> {
    if enabled {
        dialogue_conversation_id(chart_id)
    } else {
        None
    }
}

/// Weekend-1 family chart ids that carry the 2hot can/explosion pages.
///
/// The shipped pages live under `weekend1/images/` for the whole family
/// (`2hot`, `darnell`, `lit-up`), so the loader fails closed when they are
/// missing for those ids only. Charts outside the family never load them and
/// keep their frozen page sets. `lit-up`/`darnell` ship no note kinds; their
/// payload is loaded but inert.
fn weekend1_prop_pages(chart_id: &str) -> bool {
    let (song, _) = split_song_variant(chart_id);
    matches!(song, "2hot" | "darnell" | "lit-up")
}

/// The darnell second-story-entry in-game cutscene (`can_cutscene` trigger).
/// Only the darnell family loads the `wked1_cutscene_1_can` sheet and the
/// `darnellCanCutscene` conductor metadata; 2hot/lit-up keep their page sets.
fn darnell_cutscene_chart(chart_id: &str) -> bool {
    let (song, _) = split_song_variant(chart_id);
    song == "darnell"
}

/// Shared chart/metadata loader. Stage, characters and note style come from
/// the metadata; only the song file pair is selected by the caller.
///
/// `dialogue_id` is the explicit 6E conversation to load, or `None` for the
/// frozen render-fixture path. `weekend1` selects the weekend-1 can/explosion
/// pages (see [`weekend1_prop_pages`]); `darnell_cutscene` additionally loads
/// the cutscene can sheet and the `darnellCanCutscene` conductor metadata.
/// `note_desaturation` enables the interactive Bad/Shit note overlay; the
/// frozen bopeebo fixture path passes `false` because its scripted negative
/// phase contains Bad/Shit threshold hits. Every other generalized fixture
/// family passes `true` (its chart taps emit only Sick hits, so the overlay
/// stays empty and the frozen digests are unchanged).
#[allow(clippy::too_many_arguments)]
pub(crate) fn load_slice_from_paths(
    assets_root: &Path,
    chart_path: &str,
    metadata_path: &str,
    difficulty: &str,
    dialogue_id: Option<&str>,
    weekend1: bool,
    darnell_cutscene: bool,
    note_desaturation: bool,
    song_id: &str,
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
    let has_girlfriend = girlfriend_id_name.is_some();
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

    let conductor_changes = metadata_conductor_changes(&metadata)?;

    // M6-CONTRACT §13: the deterministic scene composes the play HUD core, so
    // the HUD pages/font join the same page store. Missing HUD assets fail
    // closed (a partial HUD would silently change frozen frame pixels).
    let hud = super::hud::load_hud(&vfs, &mut pages)?;
    // 6E: the dialogue payload joins the same store when the caller enables
    // it (the frozen render-fixture path passes None and stays byte-stable).
    let dialogue = dialogue_id
        .map(|id| super::dialogue::load_dialogue(&vfs, &mut pages, id))
        .transpose()?;
    // Weekend-1 (2hot) can/explosion pages. Loaded after every existing page
    // so non-family page indices stay exactly where they were; the family ids
    // fail closed when any page is missing. The darnell cutscene can sheet
    // loads only for the darnell family (only that chart emits the trigger).
    let weekend1_props = if weekend1 {
        Some(load_weekend1_props(
            &vfs,
            &mut pages,
            &stage_path,
            level_dir.as_deref(),
            &props,
            chart_notes,
            darnell_cutscene,
        )?)
    } else {
        None
    };
    // The cutscene-local conductor's metadata is the shipped music metadata
    // (`weekend1/music/darnellCanCutscene/darnellCanCutscene-metadata.json`),
    // not a `preload/data` document. Loaded only for the darnell family.
    let cutscene_changes = if darnell_cutscene {
        Some(load_cutscene_changes(&vfs)?)
    } else {
        None
    };
    // DropShadow alternate-mask pages (M6-CONTRACT §15.1) are appended after
    // every existing page, so the frozen bopeebo/school/dialogue page indices
    // stay byte-identical. Only the two shipped DropShadow stages load pages.
    let shader_masks = load_shader_masks(&vfs, &mut pages, &stage_name)?;
    // Loaded after existing stage/mask pages so non-speaker stages keep
    // byte-identical page indices (`docs/M6-CONTRACT.md` §18).
    let abot = super::abot::load_abot_speaker(&vfs, &mut pages, &characters, level_dir.as_deref())?;

    // `tankmanBattlefieldErect.hxc`: when the girlfriend is added, the
    // `tankBricks` prop is repositioned to (445, 774).
    let mut props = props;
    if stage_name == "tankmanBattlefieldErect"
        && has_girlfriend
        && let Some(prop) = props.iter_mut().find(|prop| prop.name == "tankBricks")
    {
        prop.position = (445.0, 774.0);
    }

    Ok(LoadedSlice {
        pages: pages.pages,
        song_id: song_id.to_string(),
        stage_id: stage_name,
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
        dialogue,
        weekend1_props,
        cutscene_changes,
        shader_masks,
        abot,
        note_desaturation,
    })
}

/// Loads the alternate-mask pages of the two frozen DropShadow stages.
///
/// These are plain linear-filtered PNGs (the native mask sheets are not
/// pixel-style). Every other stage returns the empty set and appends no page.
fn load_shader_masks(
    vfs: &Vfs,
    pages: &mut PageStore,
    stage_id: &str,
) -> Result<ShaderMaskPages, RenderError> {
    let mut masks = ShaderMaskPages::default();
    match stage_id {
        "tankmanBattlefieldErect" => {
            masks.tankman_gf = Some(load_mask_page(
                vfs,
                pages,
                "week7/images/erect/masks/gfTankmen_mask.png",
            )?);
            masks.tankman_nene = Some(load_mask_page(
                vfs,
                pages,
                "week7/images/erect/masks/neneTankmen_mask.png",
            )?);
            masks.tankman_captain_bloody = Some(load_mask_page(
                vfs,
                pages,
                "week7/images/erect/masks/tankmanCaptainBloody_mask.png",
            )?);
        }
        "schoolErect" => {
            masks.school_bf = Some(load_mask_page(
                vfs,
                pages,
                "week6/images/weeb/erect/masks/bfPixel_mask.png",
            )?);
            masks.school_pico = Some(load_mask_page(
                vfs,
                pages,
                "week6/images/weeb/erect/masks/picoPixel_mask.png",
            )?);
            masks.school_gf = Some(load_mask_page(
                vfs,
                pages,
                "week6/images/weeb/erect/masks/gfPixel_mask.png",
            )?);
            masks.school_nene = Some(load_mask_page(
                vfs,
                pages,
                "week6/images/weeb/erect/masks/nenePixel_mask.png",
            )?);
            masks.school_senpai = Some(load_mask_page(
                vfs,
                pages,
                "week6/images/weeb/erect/masks/senpai_mask.png",
            )?);
        }
        _ => {}
    }
    Ok(masks)
}

/// One linear-filtered mask page through the shared page store.
fn load_mask_page(vfs: &Vfs, pages: &mut PageStore, path: &str) -> Result<usize, RenderError> {
    pages.load_png_file(vfs, path, false).map(|(page, _)| page)
}

fn read_data_document(vfs: &Vfs, path: &str) -> Result<oxide_data::ParsedDocument, RenderError> {
    let bytes = read_asset(vfs, path)?;
    oxide_data::parse_file(Path::new(path), &bytes).map_err(|source| RenderError::Parse {
        path: path.to_string(),
        source,
    })
}

/// Loads the weekend-1 (2hot) can/explosion payload through the same VFS and
/// page store as every other slice asset.
///
/// Native precache (`2hot.hxc onCreate`): `spraycanAtlas/spritemap1`,
/// `SpraypaintExplosion`, `spraypaintExplosionEZ`. The atlas key uses the
/// script's `weekend1` library; the Sparrow keys resolve through the stage
/// directory (`weekend1/images/...`).
fn load_weekend1_props(
    vfs: &Vfs,
    pages: &mut PageStore,
    stage_path: &str,
    stage_dir: Option<&str>,
    props: &[LoadedStageProp],
    chart_notes: &[oxide_data::chart::NoteData],
    darnell_cutscene: bool,
) -> Result<Weekend1Props, RenderError> {
    let can_sheet = load_atlas_sheet(vfs, pages, "weekend1:spraycanAtlas", stage_dir, false)?;
    let can_start = weekend1_clip(&can_sheet, "Can Start")?;
    let can_shot = weekend1_clip(&can_sheet, "Can Shot")?;
    let hit_pico = weekend1_clip(&can_sheet, "Hit Pico")?;
    // The darnell in-game cutscene can (`wked1_cutscene_1_can` Sparrow with
    // the kick clips); loaded only for the darnell family (only that chart
    // emits the `can_cutscene` trigger).
    let cutscene = if darnell_cutscene {
        Some(load_cutscene_can(vfs, pages)?)
    } else {
        None
    };

    let big_sheet = load_sparrow_sheet(vfs, pages, "SpraypaintExplosion", stage_dir, false)?;
    let big_frames = require_frames(
        &big_sheet,
        "Explosion 1 movie0",
        "weekend1/images/SpraypaintExplosion",
    )?;
    let ez_sheet = load_sparrow_sheet(vfs, pages, "spraypaintExplosionEZ", stage_dir, false)?;
    let ez_frames = require_frames(
        &ez_sheet,
        "explosion round 1 short0",
        "weekend1/images/spraypaintExplosionEZ",
    )?;

    let pile = props
        .iter()
        .find(|prop| prop.name == "spraycanPile")
        .map(|prop| (prop.position.0, prop.position.1, prop.z_index));
    let chart_carries_kinds = chart_notes.iter().any(|note| {
        note.kind
            .as_deref()
            .is_some_and(|kind| kind.starts_with("weekend-1-"))
    });
    if chart_carries_kinds && pile.is_none() {
        // The native script dereferences `getNamedProp('spraycanPile')` on a
        // kickcan hit; a chart with kinds on a stage without the pile would
        // crash there, so fail closed at load instead.
        return Err(RenderError::MissingField {
            path: stage_path.to_string(),
            field: "props.spraycanPile",
        });
    }

    Ok(Weekend1Props {
        can_sheet,
        can_start,
        can_shot,
        hit_pico,
        big_sheet,
        big_frames,
        ez_sheet,
        ez_frames,
        cutscene_can_sheet: cutscene.as_ref().map(|cutscene| cutscene.sheet.clone()),
        cutscene_can_up: cutscene.as_ref().and_then(|cutscene| cutscene.up.clone()),
        cutscene_can_forward: cutscene
            .as_ref()
            .and_then(|cutscene| cutscene.forward.clone()),
        pile,
    })
}

/// The darnell in-game cutscene can: the `wked1_cutscene_1_can` Sparrow sheet
/// with the kick clips (`can kicked up0` / `can kick quick0`).
struct CutsceneCan {
    sheet: SpriteSheet,
    up: Option<CanClip>,
    forward: Option<CanClip>,
}

fn load_cutscene_can(vfs: &Vfs, pages: &mut PageStore) -> Result<CutsceneCan, RenderError> {
    let sheet = load_sparrow_sheet(vfs, pages, "weekend1:wked1_cutscene_1_can", None, false)?;
    let up = CanClip {
        frames: sheet.prefix_indices("can kicked up0"),
    };
    let forward = CanClip {
        frames: sheet.prefix_indices("can kick quick0"),
    };
    Ok(CutsceneCan {
        sheet,
        up: Some(up),
        forward: Some(forward),
    })
}

/// The metadata time changes as conductor `TimeChange`s (the M4 units).
fn metadata_conductor_changes(
    metadata: &oxide_data::metadata::Metadata,
) -> Result<Vec<oxide_audio::conductor::TimeChange>, RenderError> {
    metadata
        .time_changes
        .as_deref()
        .unwrap_or_default()
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
        .collect()
}

/// The shipped cutscene-local music metadata
/// (`weekend1/music/darnellCanCutscene/darnellCanCutscene-metadata.json`).
fn load_cutscene_changes(
    vfs: &Vfs,
) -> Result<Vec<oxide_audio::conductor::TimeChange>, RenderError> {
    let path = "weekend1/music/darnellCanCutscene/darnellCanCutscene-metadata.json";
    let _source = vfs.resolve(path).ok_or_else(|| RenderError::MissingAsset {
        path: path.to_string(),
    })?;
    let document = read_data_document(vfs, path)?;
    let oxide_data::Typed::Metadata(metadata) = document.typed else {
        return Err(RenderError::WrongKind {
            path: path.to_string(),
            expected: "metadata",
            actual: document.kind,
        });
    };
    metadata_conductor_changes(&metadata)
}

/// One named main-timeline label as a can clip (the native `animation.play`
/// names). The BTA labels are `Can Start` / `Hit Pico` / `Can Shot`.
fn weekend1_clip(sheet: &AtlasSheet, label: &str) -> Result<CanClip, RenderError> {
    let frames =
        sheet
            .stage
            .label_frames(label)
            .ok_or_else(|| RenderError::MissingAnimationLabel {
                character: "weekend1:spraycanAtlas".to_string(),
                animation: label.to_string(),
                prefix: label.to_string(),
            })?;
    Ok(CanClip { frames })
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
pub(crate) fn image_file_bases(key: &str, level_dir: Option<&str>) -> Vec<String> {
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
    /// The decoded pages in registration order (test-only; the production
    /// loader consumes `pages.pages` directly in the LoadedSlice literal).
    #[cfg(test)]
    pub(crate) fn into_pages(self) -> Vec<PageImage> {
        self.pages
    }

    pub(crate) fn add_solid_white(&mut self) -> Result<usize, RenderError> {
        let key = "<solid-white-2x2>".to_string();
        if let Some(index) = self.indices.get(&key).copied() {
            return Ok(index);
        }
        let page = PageImage::from_rgba(2, 2, vec![255; 16])?;
        let index = self.pages.len();
        self.pages.push(page);
        self.indices.insert(key, index);
        Ok(index)
    }

    pub(crate) fn load_image_key(
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
                    let mut sheet = load_atlas_sheet(vfs, pages, &asset_path, level_dir, nearest)?;
                    if let Some(list) = &prop.animations {
                        for animation in list {
                            animations.insert(
                                animation.name.clone(),
                                atlas_animation_def(animation, &mut sheet, 0, &asset_path)?,
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

/// Resolves the animation displayed when the character is created, mirroring
/// [`super::anim::CharacterScheduler`]'s startup: an explicit
/// `startingAnimation` (with the native `-suffix` stripping fallback), else
/// `idle`, else `danceLeft`. Native `resetCharacter` re-plays the idle/dance
/// animation before measuring the hitbox (`BaseCharacter.hx` L258-268,
/// L300-309).
fn resolve_startup_animation<'a>(
    animations: &'a BTreeMap<String, AnimationDef>,
    starting_animation: Option<&str>,
) -> Option<&'a AnimationDef> {
    fn lookup<'a>(
        animations: &'a BTreeMap<String, AnimationDef>,
        name: &str,
    ) -> Option<&'a AnimationDef> {
        if let Some(animation) = animations.get(name) {
            return Some(animation);
        }
        name.rsplit_once('-')
            .and_then(|(base, _suffix)| lookup(animations, base))
    }
    starting_animation
        .and_then(|name| lookup(animations, name))
        .or_else(|| animations.get("idle"))
        .or_else(|| animations.get("danceLeft"))
}

/// Frame/timeline dimensions of the startup animation at scale 1 (`width`, `height`).
/// `None` when the animation has no frames or the sheet/frame index is
/// unavailable; the caller falls back to the main-sheet bounds. For
/// `Sheet::Atlas`, native `FlxAnimateController.updateTimelineBounds` sets
/// `frameWidth`/`frameHeight` to the active timeline's whole bounds
/// (`timeline._bounds`), not a single resolved frame.
fn startup_frame_size(sheets: &[Sheet], animation: &AnimationDef) -> Option<[f64; 2]> {
    let frame = animation.frames.first().copied()?;
    let sheet = sheets.get(animation.sheet)?;
    match sheet {
        Sheet::Sprite(sheet) => sheet
            .frame(frame)
            .map(|frame| [frame.dest_size[0] as f64, frame.dest_size[1] as f64]),
        Sheet::Atlas(sheet) => {
            let bounds = sheet.timeline_bounds(animation.symbol.as_deref());
            Some([bounds[2], bounds[3]])
        }
    }
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

        let def = match &mut sheets[sheet_index] {
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

    // `BaseCharacter.resetCharacter`/`onCreate` play the idle animation and
    // only then call `updateHitbox`, so `characterOrigin` (and the position
    // derived from it) uses the startup animation's active timeline/frame
    // dimensions, not the sheet's first document frame (`BaseCharacter.hx`
    // L258-268, L300-309).
    let startup_animation =
        resolve_startup_animation(&animations, character.starting_animation.as_deref());
    let idle_size = startup_animation
        .and_then(|animation| startup_frame_size(&sheets, animation))
        .unwrap_or([bounds[2], bounds[3]]);
    if let Some(animation) = startup_animation
        && let Some(Sheet::Sprite(sheet)) = sheets.get(animation.sheet)
        && let Some(frame) = animation
            .frames
            .first()
            .and_then(|index| sheet.frame(*index))
    {
        // Setup-time `frameWidth`/`frameHeight` for the Sprite draw content
        // path: the idle frame's box, even when it is not the sheet's first
        // document frame.
        sprite_hitbox = Some(frame.dest_size);
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
        idle_size,
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
    sheet: &mut AtlasSheet,
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
            Some(count) => {
                if !sheet.symbol_bounds.contains_key(&prefix) {
                    let symbol_bounds = sheet.stage.symbol_bounds(&prefix).map_err(|source| {
                        RenderError::AtlasStage {
                            path: owner.to_string(),
                            source,
                        }
                    })?;
                    sheet.symbol_bounds.insert(prefix.clone(), symbol_bounds);
                }
                (0..count).collect::<Vec<_>>()
            }
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

pub(crate) fn load_sparrow_sheet(
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

pub(crate) fn load_atlas_sheet(
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
        symbol_bounds: BTreeMap::new(),
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
    fn startup_animation_resolution_matches_the_scheduler_order() {
        let frame = |dest: [i32; 2]| SpriteFrame {
            page: 0,
            rect: [0, 0, dest[0], dest[1]],
            dest_offset: [0, 0],
            dest_size: dest,
            rotated: false,
        };
        let sheets = vec![Sheet::Sprite(SpriteSheet {
            page: 0,
            page_size: [100.0, 100.0],
            frames: vec![
                ("first".to_string(), frame([10, 10])),
                ("idle".to_string(), frame([30, 40])),
                ("danceLeft".to_string(), frame([20, 25])),
                ("hey".to_string(), frame([50, 60])),
            ],
        })];
        let mut animations = BTreeMap::new();
        for (name, index) in [("idle", 1usize), ("danceLeft", 2), ("hey", 3)] {
            let mut animation = AnimationDef::test(&[index], 24.0, true);
            animation.sheet = 0;
            animations.insert(name.to_string(), animation);
        }
        let startup = resolve_startup_animation(&animations, None).expect("idle is the fallback");
        assert_eq!(startup.frames, vec![1]);
        assert_eq!(startup_frame_size(&sheets, startup), Some([30.0, 40.0]));

        let startup = resolve_startup_animation(&animations, Some("hey-alt"))
            .expect("explicit starting animation wins");
        assert_eq!(startup.frames, vec![3]);
        // Missing explicit name falls through to idle.
        let startup =
            resolve_startup_animation(&animations, Some("missing")).expect("idle fallback");
        assert_eq!(startup.frames, vec![1]);

        // No idle: danceLeft is the native dance fallback.
        let mut dance_only = BTreeMap::new();
        dance_only.insert("danceLeft".to_string(), animations["danceLeft"].clone());
        dance_only.insert("danceRight".to_string(), animations["danceLeft"].clone());
        let startup = resolve_startup_animation(&dance_only, None).expect("danceLeft fallback");
        assert_eq!(startup.frames, vec![2]);
    }

    /// The startup frame may be a trimmed/offset idle frame: the origin uses
    /// its frame box, not the first document frame (BaseCharacter.hx L258-268,
    /// L300-309).
    #[test]
    fn startup_frame_size_uses_the_idle_animation_frame() {
        let frame = |rect: [i32; 4], offset: [i32; 2], dest: [i32; 2]| SpriteFrame {
            page: 0,
            rect,
            dest_offset: offset,
            dest_size: dest,
            rotated: false,
        };
        let sheets = vec![Sheet::Sprite(SpriteSheet {
            page: 0,
            page_size: [100.0, 100.0],
            frames: vec![
                ("first".to_string(), frame([0, 0, 10, 10], [0, 0], [10, 10])),
                (
                    "idle0000".to_string(),
                    frame([10, 0, 30, 40], [-2, -3], [30, 40]),
                ),
            ],
        })];
        let mut idle = AnimationDef::test(&[1], 24.0, true);
        idle.sheet = 0;
        assert_eq!(startup_frame_size(&sheets, &idle), Some([30.0, 40.0]));
    }

    #[test]
    fn startup_frame_size_for_atlas_uses_whole_timeline_and_symbol_bounds() {
        let animation_json = r#"{
            "AN": {"TL": {"L": [{"FR": [
                {"I": 0, "DU": 1, "N": "Idle", "E": [{"ASI": {"N": "small", "MX": [1, 0, 0, 1, 0, 0]}}]},
                {"I": 1, "DU": 1, "E": [{"ASI": {"N": "wide", "MX": [1, 0, 0, 1, 100, 200]}}]}
            ]}]}},
            "SD": {"S": [{"SN": "DanceSym", "TL": {"L": [{"FR": [
                {"I": 0, "DU": 1, "E": [{"ASI": {"N": "small", "MX": [1, 0, 0, 1, 5, 6]}}]},
                {"I": 1, "DU": 1, "E": [{"ASI": {"N": "tall", "MX": [1, 0, 0, 1, 25, 36]}}]}
            ]}]}}]},
            "MD": {"W": 300, "H": 400, "FRT": 24}
        }"#;
        let spritemap = r#"{
            "ATLAS": {"SPRITES": [
                {"SPRITE": {"name": "small", "x": 0, "y": 0, "w": 10, "h": 12}},
                {"SPRITE": {"name": "wide", "x": 0, "y": 0, "w": 50, "h": 30}},
                {"SPRITE": {"name": "tall", "x": 0, "y": 0, "w": 20, "h": 40}}
            ]},
            "meta": {"image": "spritemap1.png", "size": {"w": 128, "h": 128}}
        }"#;
        let stage = oxide_atlas::AnimateStage::parse(animation_json, spritemap)
            .expect("synthetic stage parses");
        let bounds = stage.bounds();
        // Main timeline covers [0, 0] to [150, 230].
        assert_eq!(bounds, [0.0, 0.0, 150.0, 230.0]);
        let mut sheet = AtlasSheet {
            stage,
            bounds,
            symbol_bounds: BTreeMap::new(),
            page: 0,
            page_size: [128.0, 128.0],
        };
        let label_def = atlas_animation_def(
            &AnimationData {
                name: "idle".to_string(),
                prefix: Some("Idle".to_string()),
                asset_path: None,
                render_type: None,
                anim_type: None,
                library: None,
                frame_indices: None,
                offsets: None,
                frame_rate: None,
                looped: None,
                flip_x: None,
                flip_y: None,
            },
            &mut sheet,
            0,
            "test",
        )
        .expect("label def");
        let symbol_def = atlas_animation_def(
            &AnimationData {
                name: "danceLeft".to_string(),
                prefix: Some("DanceSym".to_string()),
                asset_path: None,
                render_type: None,
                anim_type: Some("symbol".to_string()),
                library: None,
                frame_indices: None,
                offsets: None,
                frame_rate: None,
                looped: None,
                flip_x: None,
                flip_y: None,
            },
            &mut sheet,
            0,
            "test",
        )
        .expect("symbol def");
        let sheets = vec![Sheet::Atlas(Box::new(sheet))];

        // Main-timeline animations use whole timeline._bounds, NOT frame 0 (10x12).
        assert_eq!(
            startup_frame_size(&sheets, &label_def),
            Some([150.0, 230.0])
        );
        // `animType: "symbol"` uses that symbol's own whole timeline bounds ([5,6]..[45,76]).
        assert_eq!(startup_frame_size(&sheets, &symbol_def), Some([40.0, 70.0]));
    }

    #[test]
    fn generalized_fixture_note_desaturation_excludes_only_bopeebo() {
        assert!(!fixture_note_desaturation("bopeebo"));
        assert!(fixture_note_desaturation("senpai"));
        assert!(fixture_note_desaturation("spaghetti-default"));
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
