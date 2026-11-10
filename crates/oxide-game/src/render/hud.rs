//! Play HUD core (docs/M6-CONTRACT.md §13).
//!
//! One private module owns the HUD algebra consumed by BOTH the deterministic
//! scene (`Scene::draw`/`Scene::step`) and the interactive play window (the
//! same `Scene` instance drives the windowed path). Keeping the health-bar
//! fill, icon placement/state and score-text layout here prevents the play and
//! render paths from drifting.
//!
//! Observed sources pinned by the §13 behavior freeze:
//! - `PlayState.hx` L1993-2026: `healthBarBG` at `height*0.9` screen-centred,
//!   zIndex 800; `FlxBar` inset 4 px, zIndex 801; score text
//!   `x = bg.x + bg.width - 190`, `y = bg.y + 30`, RIGHT/OUTLINE,
//!   `letterSpacing = -1`, zIndex 802; icons zIndex 850.
//! - flixel 6.2.1 `FlxBar.hx` L757-830: `scaleInterval = barWidth/100`,
//!   `interval = Math.round(Std.int(fraction*barWidth/scaleInterval) *
//!   scaleInterval)`, `width = Std.int(interval)`, RIGHT_TO_LEFT anchors the
//!   fill at `barWidth - width`.
//! - `HealthIcon.hx`: legacy frame size 150 (32 for pixel icons), state
//!   thresholds 1.6/0.4 (strict comparisons), `POSITION_OFFSET = 26`,
//!   `BOP_SCALE = 0.2`, per-bop tween
//!   `min(stepLengthMs*0.002, 0.175)` s with the default (linear) ease.
//! - `SetHealthIconSongEvent.hx`: configure(id, isPixel, shouldBop, scale,
//!   flipX, offsets); `char` outside {0,1} is a warning no-op.
//! - flixel 6.2.1 `FlxBitmapText.hx` L1239-1340: OUTLINE borderSize 1 draws
//!   the eight ±1 px offsets for every glyph before the white fill pass.
//!
//! [D] `bopAngle` is always 0 on the shipped path; the angle field is kept
//! inert instead of inventing rotation.
//! [D] Mod `SetHealthIcon` ids outside the preloaded 23-icon corpus inventory
//! fail closed with a typed error (the native loader would warn/fall back).

use std::collections::BTreeMap;

use oxide_font::{AngelChar, AngelFont};
use oxide_vfs::Vfs;

use super::RenderError;
use super::geometry::{GAME_HEIGHT, GAME_WIDTH, HALF_HEIGHT, HALF_WIDTH, Vec2, lerp};
use super::load::{LoadedCharacter, PageStore, Role, read_asset};
use super::scene::{LayerSpec, Quad, concat, quad_from_matrix, quad_uv};
use crate::slice::TraceEvent;

/// Shipped health-bar page (601x19 in the reference corpus).
pub(crate) const HEALTH_BAR_PATH: &str = "shared/images/healthBar.png";
/// Shipped Angel-code font page (122x122).
pub(crate) const VCR_FONT_PAGE_PATH: &str = "fonts/vcr-bmp.png";
/// Shipped Angel-code font data.
pub(crate) const VCR_FONT_DATA_PATH: &str = "fonts/vcr-bmp.fnt";
/// The 23 shipped legacy icon PNGs (no `icon-*.xml` ships in the corpus).
pub(crate) const ICON_IDS: [&str; 23] = [
    "bf",
    "bf-old",
    "bf-pixel",
    "chaewon",
    "dad",
    "darnell",
    "eunchae",
    "face",
    "gf",
    "kazuha",
    "mom",
    "monster",
    "parents",
    "pico",
    "pico-pixel",
    "sakura",
    "senpai",
    "senpai-angry",
    "spirit",
    "spooky",
    "tankman",
    "tankman-bloody",
    "yunjin",
];
/// `Constants.HEALTH_MAX`.
pub(crate) const HEALTH_MAX: f64 = 2.0;
/// `Constants.HEALTH_STARTING = HEALTH_MAX / 2`.
pub(crate) const HEALTH_STARTING: f64 = HEALTH_MAX / 2.0;
/// `updateHealthBar` fixed lerp factor (per scene tick, M6-CONTRACT §13).
pub(crate) const HEALTH_LERP: f64 = 0.15;
/// `HealthIcon.HEALTH_ICON_SIZE`.
pub(crate) const HEALTH_ICON_SIZE: f64 = 150.0;
/// `HealthIcon.PIXEL_ICON_SIZE`.
pub(crate) const PIXEL_ICON_SIZE: f64 = 32.0;
/// `HealthIcon.POSITION_OFFSET`.
pub(crate) const ICON_POSITION_OFFSET: f64 = 26.0;
/// `HealthIcon.BOP_SCALE`.
pub(crate) const ICON_BOP_SCALE: f64 = 0.2;
/// `HealthIcon.bopEvery = Constants.STEPS_PER_BEAT`.
pub(crate) const ICON_BOP_EVERY: i32 = 4;
/// `Math.min(stepLengthMs * 0.002, .175)` seconds, in milliseconds.
pub(crate) const ICON_BOP_MAX_MS: f64 = 0.175 * 1000.0;
/// `healthBar.x = healthBarBG.x + 4` / `healthBar.y = healthBarBG.y + 4`.
pub(crate) const BAR_INSET: f64 = 4.0;
/// `scoreText.x = healthBarBG.x + healthBarBG.width - 190`.
pub(crate) const SCORE_RIGHT_OFFSET: f64 = 190.0;
/// `scoreText.y = healthBarBG.y + 30`.
pub(crate) const SCORE_Y_OFFSET: f64 = 30.0;
/// `scoreText.letterSpacing = -1`.
pub(crate) const SCORE_LETTER_SPACING: f64 = -1.0;
/// `Constants.COLOR_HEALTH_BAR_RED = 0xFFFF0000` (empty/background bar).
pub(crate) const COLOR_HEALTH_BAR_RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
/// `Constants.COLOR_HEALTH_BAR_GREEN = 0xFF66FF33` (filled bar).
pub(crate) const COLOR_HEALTH_BAR_GREEN: [f32; 4] = [0.4, 1.0, 0.2, 1.0];
/// `scoreText.borderColor = FlxColor.BLACK`.
pub(crate) const SCORE_BORDER_COLOR: [f32; 4] = [0.0, 0.0, 0.0, 1.0];
/// `FlxBitmapText` white fill pass (`textColor` default white).
pub(crate) const SCORE_FILL_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];
/// `HealthIcon.WINNING_THRESHOLD = 0.8 * 2`.
pub(crate) const WINNING_THRESHOLD: f64 = 0.8 * HEALTH_MAX;
/// `HealthIcon.LOSING_THRESHOLD = 0.2 * 2`.
pub(crate) const LOSING_THRESHOLD: f64 = 0.2 * HEALTH_MAX;

/// `FlxTextBorderStyle.OUTLINE` with `borderSize = 1`, `borderQuality = 0`
/// (`FlxBitmapText.hx` L1278-1293): one iteration, delta 1, eight offsets.
const OUTLINE_OFFSETS: [[f64; 2]; 8] = [
    [-1.0, -1.0],
    [0.0, -1.0],
    [1.0, -1.0],
    [-1.0, 0.0],
    [1.0, 0.0],
    [-1.0, 1.0],
    [0.0, 1.0],
    [1.0, 1.0],
];

/// One loaded icon page pair. Both sampler variants decode the same image; the
/// native `antialiasing = !isPixel` choice selects the page.
pub(crate) struct IconAsset {
    pub(crate) linear_page: usize,
    pub(crate) pixel_page: usize,
    pub(crate) page_size: [f64; 2],
    pub(crate) image_size: [u32; 2],
    /// New-style Sparrow sheet when `icon-<id>.xml` exists next to the PNG
    /// (native `isNewSpritesheet`); legacy slicing otherwise.
    pub(crate) new_style: Option<IconSheet>,
}

/// One legacy (or synthetic Sparrow) icon frame.
#[derive(Debug, Clone, Copy)]
pub(crate) struct IconFrame {
    /// Page rectangle `[x, y, width, height]`.
    pub(crate) rect: [i32; 4],
    /// Frame source size (the first frame also defines the sprite hitbox).
    pub(crate) size: [i32; 2],
}

/// One named icon animation. `fps == 0` is the legacy single-frame form.
#[derive(Debug, Clone)]
pub(crate) struct IconClip {
    pub(crate) frames: Vec<usize>,
    pub(crate) fps: f64,
    pub(crate) looped: bool,
}

/// A loaded icon sheet: page rectangles plus the named animation map.
#[derive(Debug, Clone)]
pub(crate) struct IconSheet {
    pub(crate) page_size: [f64; 2],
    pub(crate) frames: Vec<IconFrame>,
    pub(crate) animations: BTreeMap<String, IconClip>,
}

impl IconSheet {
    /// Legacy `loadGraphic(..., true, isPixel ? 32 : 150, ...)` grid. Frames
    /// are laid out left-to-right/top-to-bottom (`FlxTileFrames.fromGraphic`).
    /// The corpus has no `icon-*.xml`, so every preloaded icon uses this path
    /// unless a sibling XML selects the new-style pipeline below.
    fn legacy(
        page_size: [f64; 2],
        image_size: [u32; 2],
        is_pixel: bool,
    ) -> Result<Self, RenderError> {
        let frame_size = if is_pixel {
            PIXEL_ICON_SIZE as u32
        } else {
            HEALTH_ICON_SIZE as u32
        };
        let cols = image_size[0] / frame_size;
        let rows = image_size[1] / frame_size;
        if cols == 0 || rows == 0 {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "health icon page {}x{} has no {frame_size}x{frame_size} legacy frames \
                     (isPixel={is_pixel})",
                    image_size[0], image_size[1]
                ),
            });
        }
        let mut frames = Vec::with_capacity((cols * rows) as usize);
        for row in 0..rows {
            for col in 0..cols {
                frames.push(IconFrame {
                    rect: [
                        (col * frame_size) as i32,
                        (row * frame_size) as i32,
                        frame_size as i32,
                        frame_size as i32,
                    ],
                    size: [frame_size as i32, frame_size as i32],
                });
            }
        }
        let mut animations = BTreeMap::new();
        // `loadAnimationOld`: idle [0], losing [1], winning [2] iff >=3 frames.
        animations.insert(
            "idle".to_string(),
            IconClip {
                frames: vec![0],
                fps: 0.0,
                looped: false,
            },
        );
        // `loadAnimationOld` + the flixel `add(..., indices, ...)` rule
        // (FlxAnimationController.hx L247-270): indices >= numFrames are
        // DISCARDED, so a 1-frame icon (icon-dad/gf/parents 150x150) never
        // gets a `losing` animation; native `playAnimation(toLosing,
        // losing)` then finds NEITHER clip and the icon stays on `idle`
        // (frame 0). Registering `losing` on such an icon made the icon
        // draw a None frame and disappear.
        if frames.len() >= 2 {
            animations.insert(
                "losing".to_string(),
                IconClip {
                    frames: vec![1],
                    fps: 0.0,
                    looped: false,
                },
            );
        }
        if frames.len() >= 3 {
            animations.insert(
                "winning".to_string(),
                IconClip {
                    frames: vec![2],
                    fps: 0.0,
                    looped: false,
                },
            );
        }
        Ok(Self {
            page_size,
            frames,
            animations,
        })
    }

    fn first_size(&self) -> [f64; 2] {
        self.frames.first().map_or([0.0, 0.0], |frame| {
            [frame.size[0] as f64, frame.size[1] as f64]
        })
    }

    /// New-style `loadAnimationNew` (`HealthIcon.hx` L1004-1020) over a parsed
    /// Sparrow XML sheet. `addByPrefix` collects frame names that START WITH
    /// the prefix in document order (flixel `indexOf(prefix) == 0`); the clip
    /// name is the state key. idle/winning/losing loop at 24 fps; the
    /// to*/from* one-shots do not. Rotated pages fail closed ([D] limit: no
    /// shipped icon uses rotation or in-data trimming).
    fn new_style(page_size: [f64; 2], atlas: &oxide_atlas::Atlas) -> Result<Self, RenderError> {
        const NEW_STYLE_CLIPS: [(&str, bool); 7] = [
            ("idle", true),
            ("winning", true),
            ("losing", true),
            ("toWinning", false),
            ("toLosing", false),
            ("fromWinning", false),
            ("fromLosing", false),
        ];
        let frames: Vec<IconFrame> = atlas
            .subtextures
            .iter()
            .map(|texture| {
                if texture.rotated {
                    return Err(RenderError::Unsupported {
                        detail: format!(
                            "health icon atlas item {:?} is rotated; the icon pipeline \
                             supports unrotated Sparrow frames only ([D])",
                            texture.name
                        ),
                    });
                }
                Ok(IconFrame {
                    rect: [texture.x, texture.y, texture.width, texture.height],
                    size: [texture.width, texture.height],
                })
            })
            .collect::<Result<_, _>>()?;
        let mut animations = BTreeMap::new();
        for (clip_name, looped) in NEW_STYLE_CLIPS {
            let indices: Vec<usize> = atlas
                .subtextures
                .iter()
                .enumerate()
                .filter(|(_, texture)| texture.name.starts_with(clip_name))
                .map(|(index, _)| index)
                .collect();
            animations.insert(
                clip_name.to_string(),
                IconClip {
                    frames: indices,
                    fps: 24.0,
                    looped,
                },
            );
        }
        Ok(Self {
            page_size,
            frames,
            animations,
        })
    }
}

/// Parsed Angel-code metrics plus the `FlxBitmapFont` derived values.
pub(crate) struct HudFont {
    pub(crate) font: AngelFont,
    /// `font.minOffsetX = max(-char.xoffset)`.
    pub(crate) min_offset_x: i32,
    /// `font.spaceWidth = char[32].xadvance`.
    pub(crate) space_width: i32,
}

impl HudFont {
    fn new(font: AngelFont) -> Self {
        let mut min_offset_x = 0;
        let mut space_width = 0;
        for glyph in font.chars.values() {
            min_offset_x = min_offset_x.max(-glyph.xoffset);
            if glyph.id == u32::from(b' ') {
                space_width = glyph.xadvance;
            }
        }
        Self {
            font,
            min_offset_x,
            space_width,
        }
    }

    /// Native `charMap` excludes zero-size frames (space is handled by
    /// `isSpaceChar`/`spaceWidth` instead).
    fn glyph(&self, id: u32) -> Option<&AngelChar> {
        self.font
            .chars
            .get(&id)
            .filter(|glyph| glyph.width > 0 && glyph.height > 0)
    }

    /// `FlxBitmapText.getStringWidth` for a single line: spaces use
    /// `spaceWidth`, the last glyph uses `sourceSize.x` (width + positive
    /// xoffset), other glyphs use `xadvance`, and `letterSpacing` is applied
    /// after every character then subtracted once.
    fn line_width(&self, text: &str) -> f64 {
        let chars: Vec<char> = text.chars().collect();
        let mut width = f64::from(self.min_offset_x);
        for (index, ch) in chars.iter().enumerate() {
            let id = *ch as u32;
            let advance = if id == u32::from(b' ') {
                f64::from(self.space_width)
            } else if id == u32::from(b'\t') {
                4.0 * f64::from(self.space_width)
            } else if let Some(glyph) = self.glyph(id) {
                if index + 1 == chars.len() {
                    f64::from(glyph.width) + f64::from(glyph.xoffset.max(0))
                } else {
                    f64::from(glyph.xadvance)
                }
            } else {
                0.0
            };
            width += advance + SCORE_LETTER_SPACING;
        }
        if !chars.is_empty() {
            width -= SCORE_LETTER_SPACING;
        }
        width.trunc()
    }

    fn kerning(&self, first: u32, second: u32) -> f64 {
        self.font
            .kerning
            .get(&(first, second))
            .copied()
            .map_or(0.0, f64::from)
    }
}

/// Everything the HUD needs from the asset layer. Page indices address
/// `LoadedSlice::pages`.
pub(crate) struct LoadedHud {
    pub(crate) bar_page: usize,
    pub(crate) bar_size: [f64; 2],
    pub(crate) font_page: usize,
    pub(crate) font_page_size: [f64; 2],
    pub(crate) font: HudFont,
    pub(crate) icons: BTreeMap<String, IconAsset>,
}

/// Observed health-bar/score geometry derived from the decoded page size.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BarGeometry {
    pub(crate) left: f64,
    pub(crate) top: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
    pub(crate) fill_width: f64,
    pub(crate) fill_height: f64,
    pub(crate) score_x: f64,
    pub(crate) score_y: f64,
}

impl LoadedHud {
    pub(crate) fn bar_geometry(&self) -> BarGeometry {
        let left = (GAME_WIDTH - self.bar_size[0]) / 2.0;
        let top = GAME_HEIGHT * 0.9;
        BarGeometry {
            left,
            top,
            width: self.bar_size[0],
            height: self.bar_size[1],
            fill_width: self.bar_size[0] - 2.0 * BAR_INSET,
            fill_height: self.bar_size[1] - 2.0 * BAR_INSET,
            score_x: left + self.bar_size[0] - SCORE_RIGHT_OFFSET,
            score_y: top + SCORE_Y_OFFSET,
        }
    }
}

/// Loads the shipped HUD assets. Missing pages and malformed font data fail
/// closed with a typed error (the 6D fixture path must not silently drop HUD
/// pixels).
pub(crate) fn load_hud(vfs: &Vfs, pages: &mut PageStore) -> Result<LoadedHud, RenderError> {
    let (bar_page, bar_size) = pages.load_png_file(vfs, HEALTH_BAR_PATH, false)?;
    if bar_size[0] <= 2.0 * BAR_INSET || bar_size[1] <= 2.0 * BAR_INSET {
        return Err(RenderError::Unsupported {
            detail: format!(
                "health bar page {HEALTH_BAR_PATH:?} is {}x{}, smaller than the 4 px FlxBar inset",
                bar_size[0], bar_size[1]
            ),
        });
    }

    let mut icons = BTreeMap::new();
    for id in ICON_IDS {
        let path = format!("preload/images/icons/icon-{id}.png");
        let (linear_page, page_size) = pages.load_png_file(vfs, &path, false)?;
        let (pixel_page, pixel_size) = pages.load_png_file(vfs, &path, true)?;
        if page_size != pixel_size {
            return Err(RenderError::Unsupported {
                detail: format!("icon page {path:?} decoded to two different sizes"),
            });
        }
        let image_size = [page_size[0].round() as u32, page_size[1].round() as u32];
        // Native `isNewSpritesheet(charId)`: `icon-<id>.xml` next to the PNG
        // selects the new-style Sparrow pipeline (`loadSparrow` +
        // `loadAnimationNew`). The loader cannot list directories (Vfs has no
        // listing API), so sibling XMLs are checked for the measured
        // inventory ids only; mod-only icon ids stay fail-closed ([D]).
        let new_style = match vfs.resolve(&format!("preload/images/icons/icon-{id}.xml")) {
            Some(_source) => {
                let bytes = read_asset(vfs, &format!("preload/images/icons/icon-{id}.xml"))?;
                let text = String::from_utf8(bytes).map_err(|error| RenderError::Decode {
                    path: format!("preload/images/icons/icon-{id}.xml"),
                    detail: error.to_string(),
                })?;
                let atlas = oxide_atlas::parse_sparrow(&text).map_err(|source| {
                    RenderError::Unsupported {
                        detail: format!("health icon XML for {id:?} failed to parse: {source}"),
                    }
                })?;
                Some(IconSheet::new_style(page_size, &atlas)?)
            }
            None => None,
        };
        icons.insert(
            id.to_string(),
            IconAsset {
                linear_page,
                pixel_page,
                page_size,
                image_size,
                new_style,
            },
        );
    }

    let (font_page, font_page_size) = pages.load_png_file(vfs, VCR_FONT_PAGE_PATH, false)?;
    let font_bytes = read_asset(vfs, VCR_FONT_DATA_PATH)?;
    let font_text = String::from_utf8(font_bytes).map_err(|error| RenderError::Decode {
        path: VCR_FONT_DATA_PATH.to_string(),
        detail: error.to_string(),
    })?;
    let font = AngelFont::parse(&font_text).map_err(|error| RenderError::Unsupported {
        detail: format!("angel font {VCR_FONT_DATA_PATH:?} failed to parse: {error}"),
    })?;
    font.validate().map_err(|error| RenderError::Unsupported {
        detail: format!("angel font {VCR_FONT_DATA_PATH:?} failed validation: {error}"),
    })?;
    if f64::from(font.common.scale_w) != font_page_size[0]
        || f64::from(font.common.scale_h) != font_page_size[1]
    {
        return Err(RenderError::Unsupported {
            detail: format!(
                "angel font {VCR_FONT_DATA_PATH:?} declares {}x{} but {VCR_FONT_PAGE_PATH:?} is {}x{}",
                font.common.scale_w, font.common.scale_h, font_page_size[0], font_page_size[1]
            ),
        });
    }

    Ok(LoadedHud {
        bar_page,
        bar_size,
        font_page,
        font_page_size,
        font: HudFont::new(font),
        icons,
    })
}

/// Layer registry for one HUD composition. Indices are relative to the HUD
/// group start (the scene appends the group after every stage/overlay layer).
pub(crate) struct HudLayers {
    pub(crate) count: usize,
    pub(crate) bar_bg: usize,
    pub(crate) bar_empty: usize,
    pub(crate) bar_fill: usize,
    pub(crate) score_outline: usize,
    pub(crate) score_fill: usize,
    pub(crate) icons: [Vec<IconLayer>; 2],
}

pub(crate) struct IconLayer {
    pub(crate) id: String,
    pub(crate) is_pixel: bool,
    pub(crate) layer: usize,
}

impl HudLayers {
    /// Appends the HUD layer group in native zIndex order: bg 800, bar 801,
    /// score 802, icons 850. Icon layers cover both sampler variants of every
    /// preloaded id so a `SetHealthIcon` row never needs a new material.
    pub(crate) fn build(assets: &LoadedHud, specs: &mut Vec<LayerSpec>) -> Self {
        let base = specs.len();
        let mut push = |page: Option<usize>, color: [f32; 4]| {
            let index = specs.len() - base;
            specs.push(LayerSpec { page, color });
            index
        };
        let bar_bg = push(Some(assets.bar_page), [1.0, 1.0, 1.0, 1.0]);
        let bar_empty = push(None, COLOR_HEALTH_BAR_RED);
        let bar_fill = push(None, COLOR_HEALTH_BAR_GREEN);
        let score_outline = push(Some(assets.font_page), SCORE_BORDER_COLOR);
        let score_fill = push(Some(assets.font_page), SCORE_FILL_COLOR);
        let mut icons: [Vec<IconLayer>; 2] = [Vec::new(), Vec::new()];
        // Native add order: iconP2 (dad, slot 1) then iconP1 (bf, slot 0).
        for slot in [1usize, 0] {
            for (id, asset) in &assets.icons {
                icons[slot].push(IconLayer {
                    id: id.clone(),
                    is_pixel: false,
                    layer: push(Some(asset.linear_page), [1.0, 1.0, 1.0, 1.0]),
                });
                icons[slot].push(IconLayer {
                    id: id.clone(),
                    is_pixel: true,
                    layer: push(Some(asset.pixel_page), [1.0, 1.0, 1.0, 1.0]),
                });
            }
        }
        Self {
            count: specs.len() - base,
            bar_bg,
            bar_empty,
            bar_fill,
            score_outline,
            score_fill,
            icons,
        }
    }
}

/// `SetHealthIcon` / character `healthIcon` configuration.
#[derive(Debug, Clone)]
pub(crate) struct IconConfig {
    pub(crate) id: String,
    pub(crate) is_pixel: bool,
    pub(crate) should_bop: bool,
    pub(crate) scale: f64,
    pub(crate) flip_x: bool,
    pub(crate) offsets: [f64; 2],
}

#[derive(Debug, Clone, Copy)]
struct IconBopTween {
    from: f64,
    to: f64,
    start_ms: f64,
    duration_ms: f64,
}

/// One health icon (`iconP1` slot 0 / `iconP2` slot 1).
pub(crate) struct HealthIconState {
    slot: usize,
    pub(crate) id: String,
    pub(crate) is_pixel: bool,
    pub(crate) should_bop: bool,
    pub(crate) scale: f64,
    pub(crate) flip_x: bool,
    pub(crate) icon_offset: Vec2,
    sheet: IconSheet,
    current_anim: Option<String>,
    anim_elapsed_ms: f64,
    scale_x: f64,
    scale_y: f64,
    width: f64,
    height: f64,
    offset: Vec2,
    x: f64,
    y: f64,
    bop_tween: Option<IconBopTween>,
    /// Bar geometry (`healthBar.x/width/y`) captured at configure time.
    bar: BarGeometry,
    /// `HealthIcon.bopAngle`; always 0 on the shipped path [D], kept so the
    /// native `angle += bopAngle * ±1` expression is explicit and inert.
    bop_angle: f64,
    angle: f64,
}

impl HealthIconState {
    fn new(slot: usize) -> Self {
        Self {
            slot,
            id: String::new(),
            is_pixel: false,
            should_bop: true,
            scale: 1.0,
            flip_x: false,
            icon_offset: [0.0, 0.0],
            sheet: IconSheet {
                page_size: [1.0, 1.0],
                frames: Vec::new(),
                animations: BTreeMap::new(),
            },
            current_anim: None,
            anim_elapsed_ms: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            width: 0.0,
            height: 0.0,
            offset: [0.0, 0.0],
            x: 0.0,
            y: 0.0,
            bop_tween: None,
            bar: BarGeometry {
                left: 0.0,
                top: 0.0,
                width: 0.0,
                height: 0.0,
                fill_width: 0.0,
                fill_height: 0.0,
                score_x: 0.0,
                score_y: 0.0,
            },
            bop_angle: 0.0,
            angle: 0.0,
        }
    }

    fn frame_size(&self) -> [f64; 2] {
        self.sheet.first_size()
    }

    /// Native `configure(data)` (`HealthIcon.hx` L181-225): id/isPixel/
    /// shouldBop, `loadCharacter` (a full reload, so the animation state
    /// restarts), then size/offsets/flip and `updatePosition`.
    fn configure(
        &mut self,
        assets: &LoadedHud,
        config: &IconConfig,
        strict: bool,
        health_lerp: f64,
        health: f64,
    ) -> Result<(), RenderError> {
        if !config.scale.is_finite()
            || config.scale < 0.0
            || !config.offsets[0].is_finite()
            || !config.offsets[1].is_finite()
        {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "health icon {:?} has invalid scale/offsets ({}, {:?})",
                    config.id, config.scale, config.offsets
                ),
            });
        }
        let mut id = config.id.clone();
        let asset = match assets.icons.get(&id) {
            Some(asset) => asset,
            None if strict => {
                return Err(RenderError::Unsupported {
                    detail: format!(
                        "SetHealthIcon id {id:?} is not in the preloaded corpus icon inventory \
                         ([D] mod-surface limitation)"
                    ),
                });
            }
            None => {
                // Native `loadCharacter` warns and falls back to 'face'.
                id = "face".to_string();
                assets
                    .icons
                    .get(&id)
                    .ok_or_else(|| RenderError::MissingAsset {
                        path: format!("preload/images/icons/icon-{id}.png"),
                    })?
            }
        };
        // Native `isLegacyStyle = !Assets.exists('icons/icon-<id>.xml')`
        // (HealthIcon.hx L1037-1044): an XML sheet selects the new-style
        // pipeline, every other id stays on the legacy grid.
        let sheet = match &asset.new_style {
            Some(sheet) => Clone::clone(sheet),
            None => IconSheet::legacy(asset.page_size, asset.image_size, config.is_pixel)?,
        };

        self.id = id;
        self.is_pixel = config.is_pixel;
        self.should_bop = config.should_bop;
        self.scale = config.scale;
        self.flip_x = config.flip_x;
        self.icon_offset = config.offsets;
        self.sheet = sheet;
        self.bar = assets.bar_geometry();
        self.current_anim = None;
        self.anim_elapsed_ms = 0.0;
        self.bop_tween = None;
        let frame = self.frame_size();
        self.width = frame[0];
        self.height = frame[1];
        self.scale_x = 1.0;
        self.scale_y = 1.0;
        self.snap_to_target_size();
        self.update_position(health_lerp, health);
        Ok(())
    }

    /// `snapToTargetSize` (HealthIcon.hx L264-277): cancel the bop tween,
    /// size the longer axis to 150*size, update the hitbox.
    fn snap_to_target_size(&mut self) {
        self.bop_tween = None;
        let target = (HEALTH_ICON_SIZE * self.scale).trunc();
        if self.width > self.height {
            self.set_graphic_size(target, 0.0);
        } else {
            self.set_graphic_size(0.0, target);
        }
        self.update_hitbox();
    }

    /// `FlxSprite.setGraphicSize` (L724-737).
    fn set_graphic_size(&mut self, width: f64, height: f64) {
        if width <= 0.0 && height <= 0.0 {
            return;
        }
        let frame = self.frame_size();
        if frame[0] == 0.0 || frame[1] == 0.0 {
            return;
        }
        let mut scale_x = width / frame[0];
        let mut scale_y = height / frame[1];
        if width <= 0.0 {
            scale_x = scale_y;
        } else if height <= 0.0 {
            scale_y = scale_x;
        }
        self.scale_x = scale_x;
        self.scale_y = scale_y;
    }

    /// `HealthIcon.updateHitbox` (HealthIcon.hx L281-286 + FlxSprite
    /// L884-890): the centering offset is RESET, then `iconOffset` is added
    /// (no accumulation across calls).
    fn update_hitbox(&mut self) {
        let frame = self.frame_size();
        self.width = self.scale_x.abs() * frame[0];
        self.height = self.scale_y.abs() * frame[1];
        self.offset = [
            -0.5 * (self.width - frame[0]) + self.icon_offset[0],
            -0.5 * (self.height - frame[1]) + self.icon_offset[1],
        ];
    }

    /// `HealthIcon.updatePosition` (L896-935): the state machine sees the
    /// health CLAMPED at the native frame head (PlayState.hx L1208-1210 caps
    /// health before any sprite update, so the clamped value IS the native
    /// input; review finding 4), with P2 inverted as `2 - health`, while the
    /// X position uses the clamped `healthBar.value` (= `healthLerp`).
    fn update_position(&mut self, health_lerp: f64, health: f64) {
        let value = health_lerp.clamp(0.0, HEALTH_MAX);
        let split = self.bar.left
            + BAR_INSET
            + (self.bar.width - 2.0 * BAR_INSET) * (1.0 - value / HEALTH_MAX);
        let health_for_state = if self.slot == 0 {
            health
        } else {
            HEALTH_MAX - health
        };
        self.update_icon_state(health_for_state);
        self.x = if self.slot == 0 {
            split - ICON_POSITION_OFFSET
        } else {
            split - (self.width - ICON_POSITION_OFFSET)
        };
        self.y = (self.bar.top + BAR_INSET) - self.height / 2.0;
    }

    /// `updateHealthIcon` state machine (HealthIcon.hx L951-1023) with the
    /// exact-boundary strict comparisons (`< LOSING`, `> WINNING`).
    fn update_icon_state(&mut self, health: f64) {
        match self.current_anim.as_deref() {
            Some("idle") => {
                if health < LOSING_THRESHOLD {
                    self.play("toLosing", Some("losing"), false);
                } else if health > WINNING_THRESHOLD {
                    self.play("toWinning", Some("winning"), false);
                } else {
                    self.play("idle", None, false);
                }
            }
            Some("winning") => {
                if health < WINNING_THRESHOLD {
                    self.play("fromWinning", Some("idle"), false);
                } else {
                    self.play("winning", Some("idle"), false);
                }
            }
            Some("losing") => {
                if health > LOSING_THRESHOLD {
                    self.play("fromLosing", Some("idle"), false);
                } else {
                    self.play("losing", Some("idle"), false);
                }
            }
            Some("toLosing") => {
                if self.animation_finished() {
                    self.play("losing", Some("idle"), false);
                }
            }
            Some("toWinning") => {
                if self.animation_finished() {
                    self.play("winning", Some("idle"), false);
                }
            }
            Some("fromLosing") | Some("fromWinning") => {
                if self.animation_finished() {
                    self.play("idle", None, false);
                }
            }
            // Native `''` (and any unknown current name) -> idle.
            _ => self.play("idle", None, false),
        }
    }

    fn has_animation(&self, name: &str) -> bool {
        self.sheet.animations.contains_key(name)
    }

    /// `HealthIcon.playAnimation(name, fallback, restart = false)`:
    /// `FlxAnimation.play` is a no-op when the requested animation is already
    /// current and not finished; a finished one-shot restarts.
    fn play(&mut self, name: &str, fallback: Option<&str>, restart: bool) {
        let target = if self.has_animation(name) {
            Some(name)
        } else {
            fallback.filter(|fallback| self.has_animation(fallback))
        };
        let Some(target) = target else {
            return;
        };
        if !restart && self.current_anim.as_deref() == Some(target) && !self.animation_finished() {
            return;
        }
        self.current_anim = Some(target.to_string());
        self.anim_elapsed_ms = 0.0;
    }

    fn current_clip(&self) -> Option<&IconClip> {
        self.current_anim
            .as_deref()
            .and_then(|name| self.sheet.animations.get(name))
    }

    /// `FlxAnimation.finished`: looped animations never finish; legacy
    /// `fps == 0` one-shots are finished immediately; non-looped clips finish
    /// strictly after `frames / fps` seconds (`FlxAnimation.update` uses `>`).
    fn animation_finished(&self) -> bool {
        let Some(clip) = self.current_clip() else {
            return true;
        };
        if clip.looped {
            return false;
        }
        if clip.fps <= 0.0 {
            return true;
        }
        let duration_ms = clip.frames.len() as f64 / clip.fps * 1000.0;
        self.anim_elapsed_ms > duration_ms
    }

    fn current_frame_index(&self) -> Option<usize> {
        let clip = self.current_clip()?;
        if clip.frames.is_empty() {
            return None;
        }
        if clip.fps <= 0.0 {
            return clip.frames.first().copied();
        }
        let frame_duration = 1000.0 / clip.fps;
        // `FlxAnimation.update` advances only while the timer is strictly
        // greater than the current frame duration.
        let advances = if self.anim_elapsed_ms <= 0.0 {
            0
        } else {
            ((self.anim_elapsed_ms / frame_duration).ceil() as usize).saturating_sub(1)
        };
        let index = if clip.looped {
            advances % clip.frames.len()
        } else {
            advances.min(clip.frames.len() - 1)
        };
        clip.frames.get(index).copied()
    }

    fn advance_animation(&mut self, elapsed_ms: f64) {
        if self.current_clip().is_some() {
            self.anim_elapsed_ms += elapsed_ms.max(0.0);
        }
    }

    /// `HealthIcon.onStepHit` (L941-970): every `bopEvery` steps (4 = one
    /// beat), cancel the tween, jump the width by `150*size*0.2`, then tween
    /// back to `150*size` over `min(stepLengthMs*0.002, .175)` s (linear).
    fn on_step(
        &mut self,
        step: i32,
        step_length_ms: f64,
        now_ms: f64,
        health_lerp: f64,
        health: f64,
    ) {
        if !self.should_bop || ICON_BOP_EVERY == 0 || step % ICON_BOP_EVERY != 0 {
            return;
        }
        self.bop_tween = None;
        let from = self.width + HEALTH_ICON_SIZE * self.scale * ICON_BOP_SCALE;
        let to = HEALTH_ICON_SIZE * self.scale;
        self.set_graphic_size(from.trunc(), 0.0);
        let duration_ms = (step_length_ms * 2.0).min(ICON_BOP_MAX_MS);
        self.bop_tween = Some(IconBopTween {
            from,
            to,
            start_ms: now_ms,
            duration_ms,
        });
        self.update_hitbox();
        self.update_position(health_lerp, health);
        // Native `angle += bopAngle * (playerId == 0 ? +1 : -1)`; bopAngle is
        // 0 on the shipped path, so the rotation is inert [D].
        self.angle += self.bop_angle * if self.slot == 0 { 1.0 } else { -1.0 };
    }

    /// Advances an in-flight bop tween and applies the truncated width.
    fn advance_bop(&mut self, now_ms: f64) {
        let Some(tween) = self.bop_tween else {
            return;
        };
        if now_ms - tween.start_ms >= tween.duration_ms {
            self.set_graphic_size(tween.to.trunc(), 0.0);
            self.update_hitbox();
            self.bop_tween = None;
        } else if now_ms > tween.start_ms {
            let t = (now_ms - tween.start_ms) / tween.duration_ms;
            let value = tween.from + (tween.to - tween.from) * t;
            self.set_graphic_size(value.trunc(), 0.0);
            self.update_hitbox();
        }
    }

    fn quad(&self, projection: [f64; 6]) -> Option<Quad> {
        // Defensive frame resolution: the native controller never selects an
        // out-of-range index (indices >= numFrames are discarded at add
        // time), but a hypothetical bad clip must degrade to frame 0 rather
        // than making an icon vanish from the screen.
        let index = self.current_frame_index().unwrap_or(0);
        let frame = self
            .sheet
            .frames
            .get(index)
            .or_else(|| self.sheet.frames.first())
            .copied()?;
        let left = self.x - self.offset[0];
        let top = self.y - self.offset[1];
        let matrix = concat(
            projection,
            [self.scale_x, 0.0, 0.0, self.scale_y, left, top],
        );
        let mut uv = quad_uv(frame.rect, self.sheet.page_size);
        if self.flip_x {
            uv.swap(0, 1);
            uv.swap(2, 3);
        }
        Some(quad_from_matrix(
            matrix,
            f64::from(frame.size[0]),
            f64::from(frame.size[1]),
            uv,
        ))
    }
}

/// Runtime HUD state driven once per scene tick.
pub(crate) struct HudState {
    pub(crate) health: f64,
    pub(crate) health_lerp: f64,
    pub(crate) score: f64,
    /// The score string the CURRENT frame shows: native `updateScoreText`
    /// (PlayState.hx L1035-1038) runs at the head of `PlayState.update`
    /// BEFORE `processSongEvents`/`processNotes`, so a same-tick row lags
    /// one frame into the text. `begin_tick` snapshots the accumulated
    /// score there; `compose` formats the snapshot.
    pub(crate) score_snapshot: f64,
    pub(crate) terminal: bool,
    pub(crate) icons: [Option<HealthIconState>; 2],
}

impl HudState {
    /// Builds the two icons from the character `healthIcon` data. The player
    /// icon is XOR-flipped after configure (`initHealthIcon(false)`), the
    /// opponent icon is not.
    pub(crate) fn new(
        assets: &LoadedHud,
        characters: &[LoadedCharacter],
    ) -> Result<Self, RenderError> {
        let mut state = Self {
            health: HEALTH_STARTING,
            health_lerp: HEALTH_STARTING,
            score: 0.0,
            score_snapshot: 0.0,
            terminal: false,
            icons: [None, None],
        };
        if let Some(player) = characters
            .iter()
            .find(|character| character.role == Role::Player)
        {
            let mut icon = HealthIconState::new(0);
            icon.configure(
                assets,
                &player.health_icon,
                false,
                state.health_lerp,
                state.health,
            )?;
            icon.flip_x = !icon.flip_x;
            state.icons[0] = Some(icon);
        }
        if let Some(opponent) = characters
            .iter()
            .find(|character| character.role == Role::Opponent)
        {
            let mut icon = HealthIconState::new(1);
            icon.configure(
                assets,
                &opponent.health_icon,
                false,
                state.health_lerp,
                state.health,
            )?;
            state.icons[1] = Some(icon);
        }
        Ok(state)
    }

    /// Per-tick update (M6-CONTRACT §13, native frame head): snapshot the
    /// score string source FIRST (updateScoreText runs before the row
    /// processing; review finding 2), then clamp health, fixed 0.15 lerp,
    /// advance bop tweens, and refresh both icons' state/position.
    pub(crate) fn begin_tick(&mut self, now_ms: f64) {
        if self.terminal {
            return;
        }
        self.score_snapshot = self.score;
        self.health = self.health.clamp(0.0, HEALTH_MAX);
        self.health_lerp = lerp(self.health_lerp, self.health, HEALTH_LERP);
        if (self.health - self.health_lerp).abs() < 1e-4 {
            self.health_lerp = self.health;
        }
        for icon in self.icons.iter_mut().flatten() {
            // The scene grid advances exactly one millisecond per tick.
            icon.advance_animation(1.0);
            icon.advance_bop(now_ms);
            icon.update_position(self.health_lerp, self.health);
        }
    }

    /// Conductor step gate for the icon bop.
    pub(crate) fn on_step(&mut self, step: i32, step_length_ms: f64, now_ms: f64) {
        if self.terminal {
            return;
        }
        for icon in self.icons.iter_mut().flatten() {
            icon.on_step(step, step_length_ms, now_ms, self.health_lerp, self.health);
        }
    }

    /// Mirrors the sim rows like `play::session::ScoreTracker::observe`
    /// (health clamp happens at the START of the next tick, so rows may leave
    /// health outside the display range until then). DOCUMENTED DIVERGENCE at
    /// `GameOver` (review finding 3): the HUD freezes at the last DISPLAY
    /// value (the dead scene keeps composing a frozen HUD) while the play
    /// report sets health 0.0; on the true native death path health is 0.0
    /// when death fires, so the two agree there. No shared accumulator: the
    /// report tracking and the display freeze are separate consumers; keep
    /// both rule sets aligned when editing (a differential test pins them).
    pub(crate) fn observe_event(
        &mut self,
        event: &TraceEvent,
        assets: &LoadedHud,
    ) -> Result<(), RenderError> {
        if self.terminal {
            return Ok(());
        }
        match event {
            TraceEvent::Hit {
                score_delta,
                health_after,
                ..
            }
            | TraceEvent::Miss {
                score_delta,
                health_after,
                ..
            } => {
                self.score += f64::from(*score_delta);
                self.health = *health_after;
            }
            TraceEvent::GhostPress {
                score_delta,
                health_change,
                ..
            } => {
                self.score += *score_delta;
                self.health += *health_change;
            }
            TraceEvent::HoldTick {
                health_delta,
                score_delta,
                ..
            } => {
                self.health += *health_delta;
                self.score += *score_delta;
            }
            TraceEvent::HoldDrop {
                score_penalty,
                health_change,
                ..
            } => {
                self.score += *score_penalty;
                self.health += *health_change;
            }
            TraceEvent::GameOver { .. } => {
                self.terminal = true;
            }
            TraceEvent::HealthIcon {
                char,
                id,
                should_bop,
                scale,
                flip_x,
                is_pixel,
                offsets,
                ..
            } => {
                let slot = match char {
                    0 => 0,
                    1 => 1,
                    // Native switch default: warning/no-op (the corpus only
                    // carries int-typed 0/1; §12.6 finding 2).
                    _ => return Ok(()),
                };
                let Some(icon) = self.icons[slot].as_mut() else {
                    return Ok(());
                };
                let config = IconConfig {
                    id: id.clone(),
                    is_pixel: *is_pixel,
                    should_bop: *should_bop,
                    scale: *scale,
                    flip_x: *flip_x,
                    offsets: *offsets,
                };
                icon.configure(assets, &config, true, self.health_lerp, self.health)?;
            }
            // Note visuals/camera rows do not change HUD state.
            TraceEvent::HoldStart { .. }
            | TraceEvent::HoldEnd { .. }
            | TraceEvent::OpponentHit { .. }
            | TraceEvent::CameraFocus { .. }
            | TraceEvent::CameraZoom { .. }
            | TraceEvent::CameraBop { .. }
            | TraceEvent::PlayAnimation { .. }
            | TraceEvent::ScrollSpeed { .. }
            | TraceEvent::UnknownEvent { .. } => {}
        }
        Ok(())
    }

    /// Composes the HUD quads into one vector per HUD layer (relative order:
    /// bar bg, red empty bar, green fill, score outline, score fill, icons).
    pub(crate) fn compose(
        &self,
        assets: &LoadedHud,
        layers: &HudLayers,
        cam_hud_zoom: f64,
    ) -> Vec<Vec<Quad>> {
        let mut out = vec![Vec::new(); layers.count];
        let geometry = assets.bar_geometry();
        let projection = hud_projection(cam_hud_zoom);

        out[layers.bar_bg].push(hud_rect(
            projection,
            geometry.left,
            geometry.top,
            geometry.width,
            geometry.height,
            FULL_UV,
        ));
        out[layers.bar_empty].push(hud_rect(
            projection,
            geometry.left + BAR_INSET,
            geometry.top + BAR_INSET,
            geometry.fill_width,
            geometry.fill_height,
            FULL_UV,
        ));
        let fill = flx_bar_fill_width(self.health_lerp, geometry.fill_width);
        if fill > 0.0 {
            out[layers.bar_fill].push(hud_rect(
                projection,
                geometry.left + BAR_INSET + (geometry.fill_width - fill),
                geometry.top + BAR_INSET,
                fill,
                geometry.fill_height,
                FULL_UV,
            ));
        }

        let text = format!("Score: {}", format_money(self.score_snapshot));
        let (outline, fill_quads) = score_quads(assets, geometry, &text, projection);
        out[layers.score_outline] = outline;
        out[layers.score_fill] = fill_quads;

        // Native add order: dad (slot 1) then bf (slot 0).
        for slot in [1usize, 0] {
            let Some(icon) = &self.icons[slot] else {
                continue;
            };
            let Some(quad) = icon.quad(projection) else {
                continue;
            };
            if let Some(layer) = layers.icons[slot]
                .iter()
                .find(|layer| layer.id == icon.id && layer.is_pixel == icon.is_pixel)
            {
                out[layer.layer].push(quad);
            }
        }
        out
    }
}

/// Flixel camera projection for `camHUD` with `scroll = 0` and HUD
/// `scrollFactor = (0, 0)`: `screen = center + zoom * (world - center)`.
pub(crate) fn hud_projection(zoom: f64) -> [f64; 6] {
    [
        zoom,
        0.0,
        0.0,
        zoom,
        HALF_WIDTH - zoom * HALF_WIDTH,
        HALF_HEIGHT - zoom * HALF_HEIGHT,
    ]
}

/// Native `FlxBar.updateFilledBar` width chain (flixel 6.2.1 L757-830):
/// `interval = Math.round(Std.int(fraction*barWidth/(barWidth/100)) *
/// (barWidth/100))`, `width = Std.int(interval)`. Haxe `Std.int` truncates
/// toward zero and `Math.round` is `floor(v + 0.5)` for non-negative values.
pub(crate) fn flx_bar_fill_width(value: f64, bar_width: f64) -> f64 {
    let value = value.clamp(0.0, HEALTH_MAX);
    // Floating-point precision guard: if value is within float rounding error
    // of HEALTH_MAX (such as asymptotic lerp 1.9999999999999993), fraction must
    // be 1.0 so truncation does not drop 99.99999999999997 down to 99 slices.
    let fraction = if (HEALTH_MAX - value).abs() < 1e-7 {
        1.0
    } else {
        value / HEALTH_MAX
    };
    let scale_interval = bar_width / 100.0;
    let stepped = (fraction * bar_width / scale_interval).trunc();
    let interval = (stepped * scale_interval + 0.5).trunc();
    interval.trunc()
}

/// `FlxStringUtil.formatMoney(amount, showDecimals = false,
/// commaSeparated = true)` (flixel 6.2.1 L117-158). Negative amounts keep the
/// native `"-0"` result for `-0.5`.
pub(crate) fn format_money(amount: f64) -> String {
    let is_negative = amount < 0.0;
    let mut amount = amount.abs().floor();
    let mut chunks = Vec::new();
    while amount > 0.0 {
        let helper = amount - (amount / 1000.0).floor() * 1000.0;
        amount = (amount / 1000.0).floor();
        chunks.push((helper as u64, amount > 0.0));
    }
    let mut string = String::new();
    if chunks.is_empty() {
        string.push('0');
    } else {
        for (chunk, pad) in chunks.iter().rev() {
            if !string.is_empty() {
                string.push(',');
            }
            if *pad {
                string.push_str(&format!("{chunk:03}"));
            } else {
                string.push_str(&format!("{chunk}"));
            }
        }
    }
    if is_negative {
        string.insert(0, '-');
    }
    string
}

fn score_quads(
    assets: &LoadedHud,
    geometry: BarGeometry,
    text: &str,
    projection: [f64; 6],
) -> (Vec<Quad>, Vec<Quad>) {
    let font = &assets.font;
    let chars: Vec<char> = text.chars().collect();
    let line_width = font.line_width(text);
    // autoSize=true, padding=0: frameWidth == textWidth, so the RIGHT
    // alignment term `(frameWidth - lineWidth)` cancels and the text's left
    // edge stays at scoreText.x + minOffsetX. Kept explicit for the frozen
    // algebra (`FlxBitmapText.updateTextBitmap` L1046-1060).
    let frame_width = line_width;
    let start_x = f64::from(font.min_offset_x) + (frame_width - line_width);
    let mut pen = start_x;
    let mut outline = Vec::new();
    let mut fill = Vec::new();
    for (index, ch) in chars.iter().enumerate() {
        let id = *ch as u32;
        if let Some(glyph) = font.glyph(id) {
            let glyph_x = geometry.score_x + pen + f64::from(glyph.xoffset);
            let glyph_y = geometry.score_y + f64::from(glyph.yoffset);
            let width = f64::from(glyph.width);
            let height = f64::from(glyph.height);
            let uv = quad_uv(
                [glyph.x, glyph.y, glyph.width as i32, glyph.height as i32],
                assets.font_page_size,
            );
            // `FlxBitmapText.draw` emits every border quad first, then every
            // white fill quad (L434-435); letterSpacing -1 makes glyphs
            // overlap by 1 px, so the global pass order is observable.
            for offset in OUTLINE_OFFSETS {
                outline.push(hud_rect(
                    projection,
                    glyph_x + offset[0],
                    glyph_y + offset[1],
                    width,
                    height,
                    uv,
                ));
            }
            fill.push(hud_rect(projection, glyph_x, glyph_y, width, height, uv));
        }
        let base_advance = if id == u32::from(b' ') {
            f64::from(font.space_width)
        } else if id == u32::from(b'\t') {
            4.0 * f64::from(font.space_width)
        } else {
            font.glyph(id)
                .map_or(0.0, |glyph| f64::from(glyph.xadvance))
        };
        let pair_advance = chars
            .get(index + 1)
            .map_or(0.0, |next| font.kerning(id, *next as u32));
        pen += base_advance + pair_advance + SCORE_LETTER_SPACING;
    }
    (outline, fill)
}

const FULL_UV: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

fn hud_rect(
    projection: [f64; 6],
    left: f64,
    top: f64,
    width: f64,
    height: f64,
    uv: [[f64; 2]; 4],
) -> Quad {
    quad_from_matrix(
        concat(projection, [1.0, 0.0, 0.0, 1.0, left, top]),
        width,
        height,
        uv,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slice::TallyCounts;

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn test_font_xml() -> String {
        let mut xml = String::from(
            "<font><info face=\"test\" size=\"16\"/>\
             <common lineHeight=\"18\" base=\"14\" scaleW=\"300\" scaleH=\"100\"/>\
             <pages><page id=\"0\" file=\"test.png\"/></pages><chars>\
             <char id=\"32\" x=\"0\" y=\"0\" width=\"0\" height=\"0\" xoffset=\"0\" \
             yoffset=\"0\" xadvance=\"10\" page=\"0\"/>",
        );
        for (index, ch) in [
            45u32, 44, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 58, 83, 99, 101, 111, 114,
        ]
        .iter()
        .enumerate()
        {
            xml.push_str(&format!(
                "<char id=\"{ch}\" x=\"{}\" y=\"0\" width=\"11\" height=\"14\" \
                 xoffset=\"-1\" yoffset=\"2\" xadvance=\"10\" page=\"0\"/>",
                index * 11
            ));
        }
        xml.push_str("</chars></font>");
        xml
    }

    fn test_hud(ids: &[&str], image_size: [u32; 2]) -> LoadedHud {
        let mut icons = BTreeMap::new();
        for (index, id) in ids.iter().enumerate() {
            icons.insert(
                id.to_string(),
                IconAsset {
                    linear_page: index * 2,
                    pixel_page: index * 2 + 1,
                    page_size: [image_size[0] as f64, image_size[1] as f64],
                    image_size,
                    new_style: None,
                },
            );
        }
        let font = AngelFont::parse(&test_font_xml()).expect("test font parses");
        LoadedHud {
            bar_page: 100,
            bar_size: [601.0, 19.0],
            font_page: 101,
            font_page_size: [300.0, 100.0],
            font: HudFont::new(font),
            icons,
        }
    }

    fn test_config(id: &str, is_pixel: bool) -> IconConfig {
        IconConfig {
            id: id.to_string(),
            is_pixel,
            should_bop: true,
            scale: 1.0,
            flip_x: false,
            offsets: [0.0, 0.0],
        }
    }

    fn test_state(assets: &LoadedHud) -> HudState {
        let mut state = HudState {
            health: HEALTH_STARTING,
            health_lerp: HEALTH_STARTING,
            score: 0.0,
            score_snapshot: 0.0,
            terminal: false,
            icons: [None, None],
        };
        for (slot, id) in [(0usize, "bf"), (1, "dad")] {
            let mut icon = HealthIconState::new(slot);
            icon.configure(assets, &test_config(id, false), false, 1.0, 1.0)
                .expect("test icon configures");
            if slot == 0 {
                icon.flip_x = !icon.flip_x;
            }
            state.icons[slot] = Some(icon);
        }
        state
    }

    fn hit_row(score_delta: i32, health_after: f64) -> TraceEvent {
        TraceEvent::Hit {
            schema: "3",
            kind: "hit",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "sick",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            score_delta,
            health_before: health_after,
            health_after,
            combo_before: 0,
            combo_after: 0,
            combo_max: 0,
            tallies: TallyCounts::default(),
        }
    }

    fn game_over_row() -> TraceEvent {
        TraceEvent::GameOver {
            schema: "3",
            kind: "game_over",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "miss",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            death_count: 1,
        }
    }

    fn health_icon_row(
        char: i64,
        id: &str,
        is_pixel: bool,
        scale: f64,
        flip_x: bool,
        offsets: [f64; 2],
    ) -> TraceEvent {
        TraceEvent::HealthIcon {
            schema: "3",
            kind: "health_icon",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            char,
            id: id.to_string(),
            should_bop: true,
            scale,
            flip_x,
            is_pixel,
            offsets,
        }
    }

    fn sparrow_sheet() -> IconSheet {
        let names = [
            "idle",
            "winning",
            "losing",
            "toWinning",
            "toLosing",
            "fromWinning",
            "fromLosing",
        ];
        let frames = (0..names.len())
            .map(|index| IconFrame {
                rect: [index as i32 * 10, 0, 10, 10],
                size: [10, 10],
            })
            .collect();
        let mut animations = BTreeMap::new();
        for (index, name) in names.iter().enumerate() {
            animations.insert(
                name.to_string(),
                IconClip {
                    frames: vec![index],
                    fps: 24.0,
                    looped: matches!(*name, "idle" | "winning" | "losing"),
                },
            );
        }
        IconSheet {
            page_size: [70.0, 10.0],
            frames,
            animations,
        }
    }

    fn sparrow_icon() -> HealthIconState {
        let mut icon = HealthIconState::new(0);
        icon.id = "sparrow".to_string();
        icon.sheet = sparrow_sheet();
        let size = icon.frame_size();
        icon.width = size[0];
        icon.height = size[1];
        icon.update_hitbox();
        icon.current_anim = Some("idle".to_string());
        icon
    }

    #[test]
    fn flx_bar_fill_width_uses_the_native_round_half_up_chain() {
        // 593/100 = 5.93 per division. `Std.int` truncates, `Math.round`
        // rounds half up (296.5 -> 297).
        assert_eq!(flx_bar_fill_width(0.0, 593.0), 0.0);
        assert_eq!(flx_bar_fill_width(2.0, 593.0), 593.0);
        assert_eq!(flx_bar_fill_width(1.0, 593.0), 297.0);
        assert_eq!(flx_bar_fill_width(0.02, 593.0), 6.0);
        assert_eq!(flx_bar_fill_width(0.04, 593.0), 12.0);
        assert_eq!(flx_bar_fill_width(0.2, 593.0), 59.0);
        assert_eq!(flx_bar_fill_width(1.98, 593.0), 587.0);
        // Truncation boundary: 0.9995 health is still 99 slices.
        assert_eq!(flx_bar_fill_width(1.999, 593.0), 587.0);
        // Floating point asymptotic lerp limit (1.9999999999999993) reaches 100 slices.
        assert_eq!(flx_bar_fill_width(1.9999999999999993, 593.0), 593.0);
        // Clamping happens in FlxBar.set_value.
        assert_eq!(flx_bar_fill_width(-1.0, 593.0), 0.0);
        assert_eq!(flx_bar_fill_width(3.0, 593.0), 593.0);
    }

    #[test]
    fn icon_x_uses_the_clamped_health_lerp_split_and_position_offset() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        for (value, player_x, dad_x) in [
            (0.0, 910.5, 812.5),
            (1.0, 614.0, 516.0),
            (2.0, 317.5, 219.5),
        ] {
            state.health_lerp = value;
            let player = state.icons[0].as_mut().expect("player icon");
            player.update_position(value, value);
            close(player.x, player_x);
            close(player.y, 652.0 - player.height / 2.0);
            let dad = state.icons[1].as_mut().expect("dad icon");
            dad.update_position(value, value);
            close(dad.x, dad_x);
        }
        // healthBar.value is clamped to [0, 2] before the split.
        state.health_lerp = -1.0;
        let player = state.icons[0].as_mut().expect("player icon");
        player.update_position(-1.0, 0.0);
        close(player.x, 910.5);
    }

    #[test]
    fn icon_state_machine_uses_strict_threshold_boundaries() {
        let mut icon = sparrow_icon();
        icon.update_icon_state(LOSING_THRESHOLD);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
        icon.update_icon_state(WINNING_THRESHOLD);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));

        icon.update_icon_state(LOSING_THRESHOLD - 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("toLosing"));
        icon.anim_elapsed_ms = 100.0; // > 1/24 s
        icon.update_icon_state(LOSING_THRESHOLD - 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("losing"));
        icon.update_icon_state(LOSING_THRESHOLD);
        assert_eq!(icon.current_anim.as_deref(), Some("losing"));
        icon.update_icon_state(LOSING_THRESHOLD + 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("fromLosing"));
        icon.anim_elapsed_ms = 100.0;
        icon.update_icon_state(LOSING_THRESHOLD + 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));

        icon.update_icon_state(WINNING_THRESHOLD + 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("toWinning"));
        icon.anim_elapsed_ms = 100.0;
        icon.update_icon_state(WINNING_THRESHOLD + 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("winning"));
        icon.update_icon_state(WINNING_THRESHOLD);
        assert_eq!(icon.current_anim.as_deref(), Some("winning"));
        icon.update_icon_state(WINNING_THRESHOLD - 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("fromWinning"));
        icon.anim_elapsed_ms = 100.0;
        icon.update_icon_state(WINNING_THRESHOLD - 0.001);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));

        icon.current_anim = None;
        icon.update_icon_state(0.0);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
    }

    #[test]
    fn legacy_two_frame_icon_has_no_winning_fallback() {
        let assets = test_hud(&["bf"], [300, 150]);
        let mut icon = HealthIconState::new(0);
        icon.configure(&assets, &test_config("bf", false), false, 1.0, 1.0)
            .expect("configure");
        assert!(!icon.has_animation("winning"));
        assert!(!icon.has_animation("toWinning"));
        icon.update_icon_state(WINNING_THRESHOLD + 0.1);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
        icon.update_icon_state(LOSING_THRESHOLD - 0.1);
        assert_eq!(icon.current_anim.as_deref(), Some("losing"));
    }

    #[test]
    fn legacy_three_frame_icon_selects_the_winning_frame() {
        let sheet = IconSheet::legacy([450.0, 150.0], [450, 150], false).expect("legacy sheet");
        assert_eq!(sheet.frames.len(), 3);
        assert!(sheet.animations.contains_key("winning"));
        assert_eq!(sheet.animations["winning"].frames, vec![2]);
        let assets = test_hud(&["wide"], [450, 150]);
        let mut icon = HealthIconState::new(0);
        icon.configure(&assets, &test_config("wide", false), false, 1.0, 1.0)
            .expect("configure");
        icon.update_icon_state(WINNING_THRESHOLD + 0.1);
        // toWinning is missing; the fallback winning exists.
        assert_eq!(icon.current_anim.as_deref(), Some("winning"));
        assert_eq!(icon.current_frame_index(), Some(2));
    }

    #[test]
    fn configure_reload_restarts_at_idle_and_resets_size_and_flip() {
        let assets = test_hud(&["bf", "face"], [300, 150]);
        let mut icon = HealthIconState::new(0);
        icon.configure(&assets, &test_config("bf", false), false, 0.0, 0.0)
            .expect("configure bf");
        icon.update_icon_state(0.0);
        assert_eq!(icon.current_anim.as_deref(), Some("losing"));

        let mut face = test_config("face", false);
        face.scale = 2.0;
        face.flip_x = true;
        face.offsets = [5.0, 7.0];
        icon.configure(&assets, &face, true, 0.0, 0.0)
            .expect("configure face");
        assert_eq!(icon.id, "face");
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
        assert_eq!(icon.width, 300.0);
        assert_eq!(icon.height, 300.0);
        assert!(icon.flip_x);
        assert_eq!(icon.icon_offset, [5.0, 7.0]);
        // The next tick re-applies the state machine from the new health.
        icon.update_icon_state(0.0);
        assert_eq!(icon.current_anim.as_deref(), Some("losing"));
    }

    #[test]
    fn legacy_pixel_icons_display_at_150_after_snap_to_target_size() {
        let pixel = test_hud(&["bf-pixel"], [64, 32]);
        let mut icon = HealthIconState::new(0);
        icon.configure(&pixel, &test_config("bf-pixel", true), false, 1.0, 1.0)
            .expect("pixel configure");
        assert_eq!(icon.sheet.frames.len(), 2);
        assert_eq!(icon.frame_size(), [32.0, 32.0]);
        assert_eq!(icon.width, 150.0);
        assert_eq!(icon.height, 150.0);
        assert_eq!(icon.scale_x, 150.0 / 32.0);

        let wide = test_hud(&["bf"], [300, 150]);
        let mut icon = HealthIconState::new(0);
        icon.configure(&wide, &test_config("bf", false), false, 1.0, 1.0)
            .expect("wide configure");
        assert_eq!(icon.sheet.frames.len(), 2);
        assert_eq!(icon.width, 150.0);
        assert_eq!(icon.height, 150.0);

        let single = test_hud(&["dad"], [150, 150]);
        let mut icon = HealthIconState::new(1);
        icon.configure(&single, &test_config("dad", false), false, 1.0, 1.0)
            .expect("single configure");
        assert_eq!(icon.sheet.frames.len(), 1);
        assert_eq!(icon.width, 150.0);
        assert_eq!(icon.height, 150.0);
    }

    #[test]
    fn bop_tween_jumps_and_returns_to_the_base_width() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        let player = state.icons[0].as_mut().expect("player icon");
        assert_eq!(player.width, 150.0);
        player.on_step(4, 150.0, 600.0, 1.0, 1.0);
        assert_eq!(player.width, 180.0);
        let tween = player.bop_tween.expect("bop tween");
        assert_eq!(tween.from, 180.0);
        assert_eq!(tween.to, 150.0);
        assert_eq!(tween.duration_ms, 175.0);

        player.advance_bop(600.0);
        assert_eq!(player.width, 180.0);
        player.advance_bop(687.5);
        assert_eq!(player.width, 165.0);
        player.advance_bop(775.0);
        assert_eq!(player.width, 150.0);
        assert!(player.bop_tween.is_none());

        // Off-beat steps and shouldBop=false are no-ops.
        player.on_step(5, 150.0, 800.0, 1.0, 1.0);
        assert!(player.bop_tween.is_none());
        assert_eq!(player.width, 150.0);
        player.should_bop = false;
        player.on_step(8, 150.0, 900.0, 1.0, 1.0);
        assert!(player.bop_tween.is_none());

        // Fast song: stepLengthMs * 0.002 = 100 ms wins over the 175 ms cap.
        player.should_bop = true;
        player.on_step(0, 50.0, 0.0, 1.0, 1.0);
        assert_eq!(player.bop_tween.expect("bop tween").duration_ms, 100.0);
    }

    #[test]
    fn health_lerp_uses_the_fixed_015_factor_and_clamps_at_tick_start() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        assert_eq!(state.health, 1.0);
        assert_eq!(state.health_lerp, 1.0);
        state
            .observe_event(&hit_row(100, 0.5), &assets)
            .expect("hit row");
        assert_eq!(state.score, 100.0);
        state.begin_tick(0.0);
        close(state.health, 0.5);
        close(state.health_lerp, 1.0 + (0.5 - 1.0) * HEALTH_LERP);

        // A row may leave health outside [0,2]; the next tick clamps first.
        state.health = -0.25;
        state.begin_tick(1.0);
        close(state.health, 0.0);
        close(
            state.health_lerp,
            (1.0 + (0.5 - 1.0) * HEALTH_LERP) * (1.0 - HEALTH_LERP),
        );

        // Health lerp to max health settles cleanly at 2.0 and fills 100% of the bar.
        state.health = 2.0;
        for tick in 2..100 {
            state.begin_tick(tick as f64);
        }
        assert_eq!(state.health_lerp, 2.0);
        assert_eq!(flx_bar_fill_width(state.health_lerp, 593.0), 593.0);
    }

    #[test]
    fn rowless_state_keeps_health_one_and_score_zero() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        for tick in 0..120 {
            state.begin_tick(tick as f64);
            state.on_step(tick, 150.0, f64::from(tick));
        }
        assert_eq!(state.health, 1.0);
        assert_eq!(state.health_lerp, 1.0);
        assert_eq!(state.score, 0.0);
        assert!(!state.terminal);
    }

    #[test]
    fn game_over_freezes_the_hud_at_the_last_value() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        state
            .observe_event(&hit_row(50, 0.75), &assets)
            .expect("hit row");
        state.begin_tick(0.0);
        let frozen = (state.health, state.health_lerp, state.score);
        state
            .observe_event(&game_over_row(), &assets)
            .expect("game over");
        assert!(state.terminal);
        assert_eq!((state.health, state.health_lerp, state.score), frozen);
        for tick in 1..120 {
            state.begin_tick(tick as f64);
            state.on_step(tick, 150.0, f64::from(tick));
        }
        assert_eq!((state.health, state.health_lerp, state.score), frozen);
    }

    #[test]
    fn set_health_icon_rows_configure_the_right_slot_and_fail_closed() {
        let assets = test_hud(&["bf", "dad", "face"], [300, 150]);
        let mut state = test_state(&assets);
        state
            .observe_event(
                &health_icon_row(1, "face", false, 2.0, true, [3.0, 4.0]),
                &assets,
            )
            .expect("opponent icon row");
        let dad = state.icons[1].as_ref().expect("dad icon");
        assert_eq!(dad.id, "face");
        assert_eq!(dad.width, 300.0);
        assert!(dad.flip_x);
        assert_eq!(dad.icon_offset, [3.0, 4.0]);

        // Unknown char is the native warning/no-op.
        state
            .observe_event(
                &health_icon_row(2, "face", false, 1.0, false, [0.0, 0.0]),
                &assets,
            )
            .expect("unknown char is a no-op");
        // Unknown id fails closed ([D] mod-surface limitation).
        let error = state
            .observe_event(
                &health_icon_row(0, "mod-only", false, 1.0, false, [0.0, 0.0]),
                &assets,
            )
            .expect_err("unknown id must fail closed");
        assert!(matches!(error, RenderError::Unsupported { .. }));
        assert_eq!(state.icons[0].as_ref().expect("player icon").id, "bf");
    }

    #[test]
    fn score_accumulates_the_native_rows_and_formats_money() {
        assert_eq!(format_money(0.0), "0");
        assert_eq!(format_money(999.0), "999");
        assert_eq!(format_money(1000.0), "1,000");
        assert_eq!(format_money(1_234_567.0), "1,234,567");
        assert_eq!(format_money(-1234.0), "-1,234");
        // Native formatMoney(-0.5) floors the absolute value and keeps "-0".
        assert_eq!(format_money(-0.5), "-0");

        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        state
            .observe_event(&hit_row(100, 1.0), &assets)
            .expect("hit");
        state
            .observe_event(
                &TraceEvent::Miss {
                    schema: "3",
                    kind: "miss",
                    sim: 0,
                    input_ms: 0.0,
                    input_time_us: 0.0,
                    note_id: 0,
                    note_direction: 0,
                    judgement: "miss",
                    raw_note_diff: 0.0,
                    note_diff_int: 0,
                    score_delta: -10,
                    health_before: 1.0,
                    health_after: 0.9,
                    combo_before: 0,
                    combo_after: 0,
                    combo_max: 0,
                    tallies: TallyCounts::default(),
                },
                &assets,
            )
            .expect("miss");
        state
            .observe_event(
                &TraceEvent::GhostPress {
                    schema: "3",
                    kind: "ghost_press",
                    sim: 0,
                    input_ms: 0.0,
                    input_time_us: 0.0,
                    note_id: 0,
                    note_direction: 0,
                    judgement: "miss",
                    raw_note_diff: 0.0,
                    note_diff_int: 0,
                    score_delta: 5.5,
                    health_change: -0.08,
                },
                &assets,
            )
            .expect("ghost");
        state
            .observe_event(
                &TraceEvent::HoldTick {
                    schema: "3",
                    kind: "hold_tick",
                    sim: 0,
                    input_ms: 0.0,
                    input_time_us: 0.0,
                    note_id: 0,
                    note_direction: 0,
                    judgement: "sick",
                    raw_note_diff: 0.0,
                    note_diff_int: 0,
                    health_delta: 0.1,
                    score_delta: 1.25,
                    hold_remaining_ms: 0.0,
                    hold_elapsed_ms: 0.0,
                },
                &assets,
            )
            .expect("hold tick");
        state
            .observe_event(
                &TraceEvent::HoldDrop {
                    schema: "3",
                    kind: "hold_drop",
                    sim: 0,
                    input_ms: 0.0,
                    input_time_us: 0.0,
                    note_id: 0,
                    note_direction: 0,
                    judgement: "miss",
                    raw_note_diff: 0.0,
                    note_diff_int: 0,
                    score_penalty: -2.5,
                    health_change: -0.1,
                    combo_before: 0,
                    combo_after: 0,
                    is_combo_break: true,
                },
                &assets,
            )
            .expect("hold drop");
        close(state.score, 100.0 - 10.0 + 5.5 + 1.25 - 2.5);
        // Miss sets health_after 0.9; ghost -0.08, hold tick +0.1, drop -0.1.
        close(state.health, 0.9 - 0.08 + 0.1 - 0.1);
    }

    fn quad_game_corner(quad: &Quad) -> Vec2 {
        [quad.pos[0][0] + HALF_WIDTH, HALF_HEIGHT - quad.pos[0][1]]
    }

    #[test]
    fn score_text_is_auto_sized_right_aligned_and_uses_the_outline_pass_first() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let state = test_state(&assets);
        let layers = HudLayers::build(&assets, &mut Vec::new());
        let quads = state.compose(&assets, &layers, 1.0);
        // "Score: 0" has seven glyphs (space contributes no frame):
        // 8 OUTLINE offsets per glyph and one white fill per glyph.
        assert_eq!(quads[layers.score_outline].len(), 7 * 8);
        assert_eq!(quads[layers.score_fill].len(), 7);
        let first = quad_game_corner(&quads[layers.score_fill][0]);
        close(first[0], 750.5);
        close(first[1], 680.0);
        let last = quad_game_corner(&quads[layers.score_fill][6]);
        close(last[0], 813.5);
        // The auto-sized field's right edge is x + textWidth = 750.5 + 75.
        let last_width =
            quads[layers.score_fill][6].pos[1][0] - quads[layers.score_fill][6].pos[0][0];
        close(last[0] + last_width, 824.5);
        // A longer number shifts no glyph left (autoSize right alignment is a
        // no-op; the field grows to the right).
        let mut longer = test_state(&assets);
        longer.score = 1_234_567.0;
        // The compose string comes from the pre-row snapshot (native frame
        // head ordering); the test pins the composed text source.
        longer.score_snapshot = longer.score;
        let longer_quads = longer.compose(&assets, &layers, 1.0);
        close(
            quad_game_corner(&longer_quads[layers.score_fill][0])[0],
            750.5,
        );
    }

    /// Native frame-head ordering (review finding 2): the score text shows
    /// the PREVIOUS tick's accumulated value; a same-tick row becomes
    /// visible only at the next begin_tick.
    #[test]
    fn score_text_uses_the_pre_row_snapshot() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        let hit = TraceEvent::Hit {
            schema: "3",
            kind: "hit",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "sick",
            raw_note_diff: 0.0,
            note_diff_int: 350,
            score_delta: 350,
            health_before: 1.0,
            health_after: 1.0,
            combo_before: 0,
            combo_after: 1,
            combo_max: 1,
            tallies: Default::default(),
        };
        state.observe_event(&hit, &assets).expect("hit row");
        assert_eq!(state.score, 350.0);
        assert_eq!(state.score_snapshot, 0.0, "the frame text lags the row");
        state.begin_tick(1.0);
        assert_eq!(
            state.score_snapshot, 350.0,
            "the next frame head sees the row"
        );
    }

    /// Review finding 3: the HUD mirrors the play ScoreTracker rule set
    /// exactly until GameOver, where the HUD FREEZES the last display value
    /// while the report records 0.0 (on the true native death path health
    /// is already 0.0, so the two agree there). The differential test pins
    /// both rule sets so a future edit cannot drift one side silently.
    #[test]
    fn hud_state_mirrors_score_tracker_and_documents_the_game_over_divergence() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let mut state = test_state(&assets);
        let mut tracker = crate::play::ScoreTracker::new();
        let events = [
            TraceEvent::HoldTick {
                schema: "3",
                kind: "hold_tick",
                sim: 0,
                input_ms: 0.0,
                input_time_us: 0.0,
                note_id: 0,
                note_direction: 0,
                judgement: "sick",
                raw_note_diff: 0.0,
                note_diff_int: 0,
                health_delta: 0.02,
                score_delta: 3.0,
                hold_remaining_ms: 100.0,
                hold_elapsed_ms: 40.0,
            },
            TraceEvent::HoldDrop {
                schema: "3",
                kind: "hold_drop",
                sim: 0,
                input_ms: 0.0,
                input_time_us: 0.0,
                note_id: 0,
                note_direction: 0,
                judgement: "miss",
                raw_note_diff: 0.0,
                note_diff_int: 0,
                score_penalty: -10.0,
                health_change: -0.09,
                combo_before: 0,
                combo_after: 0,
                is_combo_break: true,
            },
        ];
        for event in &events {
            state.observe_event(event, &assets).expect("hud observe");
            tracker.observe(std::slice::from_ref(event));
        }
        close(state.health, tracker.health);
        close(state.score, tracker.score);
        let game_over = TraceEvent::GameOver {
            schema: "3",
            kind: "game_over",
            sim: 1,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "miss",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            death_count: 1,
        };
        state.observe_event(&game_over, &assets).expect("game over");
        tracker.observe(std::slice::from_ref(&game_over));
        assert!(tracker.terminal);
        close(tracker.health, 0.0);
        // The documented divergence: the HUD froze at the last display value
        // (0.93 = 1.0 + 0.02 - 0.09) while the report carries 0.0. On the
        // true native death path health is already exactly 0.0.
        close(state.health, 0.93);
    }

    /// Native `add` discards indices >= numFrames
    /// (FlxAnimationController.hx L247-270): a 1-frame legacy icon
    /// (icon-dad/gf/parents, 150x150) never registers `losing`, so a
    /// health drop below LOSING_THRESHOLD keeps the icon on `idle`
    /// (frame 0) WITH a rendered quad instead of making it vanish.
    #[test]
    fn single_frame_icon_stays_idle_and_is_drawn_under_losing_health() {
        let assets = test_hud(&["dad"], [150, 150]);
        let mut icon = HealthIconState::new(1);
        icon.configure(&assets, &test_config("dad", false), false, 1.0, 1.0)
            .expect("dad configures");
        assert_eq!(icon.sheet.frames.len(), 1, "icon-dad is one 150x150 frame");
        assert!(
            !icon.sheet.animations.contains_key("losing"),
            "the flixel add() rule drops the out-of-range losing index"
        );
        // The losing path: player health high (2 - health < 0.4 for the dad).
        icon.update_position(1.8, 2.0);
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
        assert!(
            icon.quad(hud_projection(1.0)).is_some(),
            "a 1-frame icon must never disappear into thin air"
        );
        // The same guarantee at the exact threshold and beyond.
        icon.update_position(1.6, 2.0);
        assert!(icon.quad(hud_projection(1.0)).is_some());
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
    }

    /// The new-style Sparrow pipeline (review finding 1): the IconSheet built
    /// from a parsed XML sheet must carry the seven native clips with
    /// addByPrefix startswith semantics, and configure must select it over the
    /// legacy grid when the loader found a sibling XML.
    #[test]
    fn new_style_sheet_selection_uses_the_sparrow_clips() {
        let atlas = r#"<TextureAtlas imagePath="icon-myth.png">
<SubTexture name="idle0000" x="0" y="0" width="10" height="10"/>
<SubTexture name="idle0001" x="10" y="0" width="10" height="10"/>
<SubTexture name="toWinning0000" x="0" y="10" width="10" height="10"/>
<SubTexture name="fromWinning0000" x="10" y="10" width="10" height="10"/>
</TextureAtlas>"#;
        let parsed = oxide_atlas::parse_sparrow(atlas).expect("synthetic atlas parses");
        let sheet = IconSheet::new_style([300.0, 150.0], &parsed).expect("new style builds");
        let idle = &sheet.animations["idle"];
        assert_eq!(idle.frames, vec![0, 1]);
        assert!(idle.looped);
        close(idle.fps, 24.0);
        assert!(!sheet.animations["toWinning"].looped);
        assert_eq!(sheet.animations["fromWinning"].frames, vec![3]);
        assert_eq!(sheet.animations["losing"].frames, Vec::<usize>::new());

        // configure selects the new-style sheet when the loader found an XML.
        let mut icons = BTreeMap::new();
        icons.insert(
            "myth".to_string(),
            IconAsset {
                linear_page: 0,
                pixel_page: 1,
                page_size: [300.0, 150.0],
                image_size: [300, 150],
                new_style: Some(sheet),
            },
        );
        let font = AngelFont::parse(&test_font_xml()).expect("test font parses");
        let assets = LoadedHud {
            bar_page: 100,
            bar_size: [601.0, 19.0],
            font_page: 101,
            font_page_size: [300.0, 100.0],
            font: HudFont::new(font),
            icons,
        };
        let mut icon = HealthIconState::new(0);
        icon.configure(&assets, &test_config("myth", false), false, 1.0, 1.0)
            .expect("new-style configure");
        assert_eq!(icon.current_anim.as_deref(), Some("idle"));
    }

    #[test]
    fn compose_orders_bar_score_and_icons_and_applies_cam_hud_zoom() {
        let assets = test_hud(&["bf", "dad"], [300, 150]);
        let state = test_state(&assets);
        let mut specs = Vec::new();
        let layers = HudLayers::build(&assets, &mut specs);
        assert_eq!(specs.len(), layers.count);
        assert_eq!(layers.count, 5 + 2 * 2 * 2);
        let quads = state.compose(&assets, &layers, 1.0);
        assert_eq!(quads.len(), layers.count);
        assert_eq!(quads[layers.bar_bg].len(), 1);
        assert_eq!(quads[layers.bar_empty].len(), 1);
        assert_eq!(quads[layers.bar_fill].len(), 1);
        // Health 1.0 -> 297 px green fill anchored at the right edge
        // (bar right edge 936.5 - 297 = 639.5; the raw 296.5 split was the
        // pre-quantization M5 value).
        close(quad_game_corner(&quads[layers.bar_fill][0])[0], 639.5);
        let fill_width = quads[layers.bar_fill][0].pos[1][0] - quads[layers.bar_fill][0].pos[0][0];
        close(fill_width, 297.0);
        // Dad then bf: each active icon occupies exactly one registered layer.
        let dad_layer = layers.icons[1]
            .iter()
            .find(|layer| layer.id == "dad" && !layer.is_pixel)
            .expect("dad layer");
        let bf_layer = layers.icons[0]
            .iter()
            .find(|layer| layer.id == "bf" && !layer.is_pixel)
            .expect("bf layer");
        assert_eq!(quads[dad_layer.layer].len(), 1);
        assert_eq!(quads[bf_layer.layer].len(), 1);
        close(quad_game_corner(&quads[dad_layer.layer][0])[0], 516.0);
        close(quad_game_corner(&quads[bf_layer.layer][0])[0], 614.0);

        // camHUD zoom scales every HUD rect around the 640x360 screen centre.
        let zoomed = state.compose(&assets, &layers, 2.0);
        let bar = &zoomed[layers.bar_bg][0];
        close(quad_game_corner(bar)[0], 640.0 + 2.0 * (339.5 - 640.0));
        close(quad_game_corner(bar)[1], 360.0 + 2.0 * (648.0 - 360.0));
        let bar_width = bar.pos[1][0] - bar.pos[0][0];
        close(bar_width, 601.0 * 2.0);
    }

    #[test]
    fn hud_projection_matches_the_scroll_factor_zero_camera() {
        let identity = hud_projection(1.0);
        assert_eq!(identity, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
        let zoomed = hud_projection(1.5);
        let apply = |x: f64, y: f64| {
            [
                zoomed[0] * x + zoomed[2] * y + zoomed[4],
                zoomed[1] * x + zoomed[3] * y + zoomed[5],
            ]
        };
        assert_eq!(apply(HALF_WIDTH, HALF_HEIGHT), [HALF_WIDTH, HALF_HEIGHT]);
        close(apply(0.0, 0.0)[0], HALF_WIDTH - 1.5 * HALF_WIDTH);
        close(apply(0.0, 0.0)[1], HALF_HEIGHT - 1.5 * HALF_HEIGHT);
    }

    /// Asset-gated: the shipped inventory must expose the measured page sizes
    /// and the vcr-bmp Angel font glyph surface.
    #[test]
    fn shipped_hud_inventory_matches_the_measured_corpus() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let loaded = crate::render::load::load_slice_for_song(
            std::path::Path::new(&root),
            "bopeebo",
            "normal",
        )
        .expect("bopeebo loads");
        let hud = loaded.hud.as_ref().expect("HUD payload");
        assert_eq!(hud.bar_size, [601.0, 19.0]);
        assert_eq!(hud.font_page_size, [122.0, 122.0]);
        assert_eq!(hud.icons.len(), ICON_IDS.len());
        let expected: &[(&str, [u32; 2])] = &[
            ("bf", [300, 150]),
            ("bf-old", [300, 150]),
            ("bf-pixel", [64, 32]),
            ("chaewon", [300, 150]),
            ("dad", [150, 150]),
            ("darnell", [300, 150]),
            ("eunchae", [300, 150]),
            ("face", [300, 150]),
            ("gf", [150, 150]),
            ("kazuha", [300, 150]),
            ("mom", [300, 150]),
            ("monster", [300, 150]),
            ("parents", [150, 150]),
            ("pico", [300, 150]),
            ("pico-pixel", [64, 32]),
            ("sakura", [300, 150]),
            ("senpai", [64, 32]),
            ("senpai-angry", [64, 32]),
            ("spirit", [64, 32]),
            ("spooky", [300, 150]),
            ("tankman", [300, 150]),
            ("tankman-bloody", [300, 150]),
            ("yunjin", [300, 150]),
        ];
        for (id, size) in expected {
            assert_eq!(hud.icons[*id].image_size, *size, "icon {id}");
        }
        assert_eq!(hud.font.min_offset_x, 1);
        assert_eq!(hud.font.space_width, 10);
        assert_eq!(hud.font.font.common.line_height, 18);
        assert_eq!(hud.font.font.chars[&u32::from(b'S')].xadvance, 10);
        assert_eq!(hud.font.line_width("Score: 0"), 75.0);
    }
}
