//! camHUD statics for the playtest (ADR-0021 decision 4, ADR-0020 addendum 4).
//!
//! Placement follows the OBSERVED `PlayState.hx:1993-2030` + `HealthIcon.hx`:
//!
//! - `healthBar.png` is 601x19 at `x = (1280 - 601)/2 = 339.5`,
//!   `y = height*0.9 = 648` (upscroll placement);
//! - the fill area is the 4 px inner inset (`x+4`, `y+4`, 593x11): dad's
//!   (left) fill has width `593 * (1 - health/2)`, bf's fill is the remainder;
//! - the icon split is `x = BAR_LEFT + 4 + 593 * ((2 - health)/2)`; dad's
//!   icon is at `split - (150 - 26)`, bf's at `split - 26`, both at
//!   `y = BAR_TOP + 4 - 75` (`HEALTH_ICON_SIZE/2`, `POSITION_OFFSET = 26`);
//! - health input is the current sim health clamped to `[0, 2]`.
//!
//! The bf icon sheet is a 2x1 150x150 page; only the first frame
//! `(0, 0, 150, 150)` is drawn (engine decision: the steady frame only — the
//! native loss/win frame switching is an unsliced observable). The healthBar
//! and dad pages are drawn whole.
//!
//! Engine decision: the fill textures/colors stay the existing bar-page tint
//! (red dad / green bf); the native `Constants.COLOR...` values are not part
//! of the frozen renderer fixtures. Score text, judgement popups, note
//! splashes and hold covers remain excluded. If any HUD asset is missing or
//! has an unexpected size, the WHOLE layer is skipped with one stderr
//! deviation; the game/audio/input session continues.

use bevy::image::ImageSampler;
use oxide_vfs::Vfs;

use super::super::render::RenderError;
use super::super::render::geometry::{GAME_HEIGHT, GAME_WIDTH};
use super::super::render::load::{PageImage, load_png_page};
use super::super::render::scene::{LayerSpec, Quad, quad_from_matrix};

const HEALTH_BAR_PATH: &str = "shared/images/healthBar.png";
const BF_ICON_PATH: &str = "preload/images/icons/icon-bf.png";
const DAD_ICON_PATH: &str = "preload/images/icons/icon-dad.png";

/// Shipped `healthBar.png` size.
const HEALTH_BAR_SIZE: (u32, u32) = (601, 19);
/// Shipped bf icon sheet size: two 150x150 frames side by side.
const BF_ICON_SHEET_SIZE: (u32, u32) = (300, 150);
/// Shipped dad icon size: one 150x150 frame.
const DAD_ICON_SIZE: (u32, u32) = (150, 150);

/// Observed `PlayState.hx:1993-2030`: `healthBar.y = height * 0.9`.
const BAR_TOP: f64 = GAME_HEIGHT * 0.9;
/// Observed screen-centred background: `(1280 - 601) / 2`.
const BAR_LEFT: f64 = (GAME_WIDTH - HEALTH_BAR_SIZE.0 as f64) / 2.0;
/// `healthBar` inner fill inset (`x + 4`, `y + 4`).
const BAR_INSET: f64 = 4.0;
/// Observed fill width `601 - 8`.
const FILL_WIDTH: f64 = HEALTH_BAR_SIZE.0 as f64 - 2.0 * BAR_INSET;
/// Observed fill height `19 - 8` (the same 4 px inner inset).
const FILL_HEIGHT: f64 = HEALTH_BAR_SIZE.1 as f64 - 2.0 * BAR_INSET;
/// `HealthIcon.HEALTH_ICON_SIZE`.
const ICON_SIZE: f64 = 150.0;
/// `HealthIcon.POSITION_OFFSET`.
const ICON_POSITION_OFFSET: f64 = 26.0;

const BAR_BACKGROUND_COLOR: [f32; 4] = [0.10, 0.10, 0.10, 0.90];
const DAD_FILL_COLOR: [f32; 4] = [0.92, 0.24, 0.38, 1.0];
const BF_FILL_COLOR: [f32; 4] = [0.32, 0.92, 0.58, 1.0];
const ICON_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

const FULL_UV: [[f64; 2]; 4] = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
/// First 150x150 frame of the 2x1 bf icon sheet (`loadGraphic(..., true, 150, 150)`).
const BF_ICON_SOURCE: [f64; 4] = [0.0, 0.0, 150.0, 150.0];

/// One HUD rectangle in game-screen pixels before the Bevy y-flip.
#[derive(Debug, Clone, Copy, PartialEq)]
struct HudRect {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

/// Observed health-bar/icon layout. Pure so the play-test regressions can
/// pin the formulas without loading assets.
#[derive(Debug, Clone, Copy, PartialEq)]
struct HudLayout {
    bar: HudRect,
    dad_fill: Option<HudRect>,
    bf_fill: Option<HudRect>,
    split_x: f64,
    dad_icon: HudRect,
    bf_icon: HudRect,
}

impl HudLayout {
    fn new(health: f64) -> Self {
        // Non-finite input is not a valid health value; treat it as empty.
        let fraction = if health.is_finite() {
            (health / 2.0).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let fill_left = BAR_LEFT + BAR_INSET;
        let fill_top = BAR_TOP + BAR_INSET;
        let split_x = fill_left + FILL_WIDTH * (1.0 - fraction);
        let dad_width = split_x - fill_left;
        let bf_width = fill_left + FILL_WIDTH - split_x;
        let dad_fill = (dad_width > 0.0).then_some(HudRect {
            left: fill_left,
            top: fill_top,
            width: dad_width,
            height: FILL_HEIGHT,
        });
        let bf_fill = (bf_width > 0.0).then_some(HudRect {
            left: split_x,
            top: fill_top,
            width: bf_width,
            height: FILL_HEIGHT,
        });
        let icon_top = fill_top - ICON_SIZE / 2.0;
        Self {
            bar: HudRect {
                left: BAR_LEFT,
                top: BAR_TOP,
                width: HEALTH_BAR_SIZE.0 as f64,
                height: HEALTH_BAR_SIZE.1 as f64,
            },
            dad_fill,
            bf_fill,
            split_x,
            dad_icon: HudRect {
                left: split_x - (ICON_SIZE - ICON_POSITION_OFFSET),
                top: icon_top,
                width: ICON_SIZE,
                height: ICON_SIZE,
            },
            bf_icon: HudRect {
                left: split_x - ICON_POSITION_OFFSET,
                top: icon_top,
                width: ICON_SIZE,
                height: ICON_SIZE,
            },
        }
    }
}

/// Loaded HUD layer specs (the pages live in the windowed harness).
pub(crate) struct Hud {
    specs: Vec<LayerSpec>,
}

impl Hud {
    /// Loads the three HUD images. Any error skips the whole HUD layer and
    /// prints the concrete deviation once; it is not a session failure.
    pub(crate) fn load(vfs: &Vfs, first_page: usize) -> Option<(Hud, Vec<PageImage>)> {
        match Self::load_inner(vfs, first_page) {
            Ok(loaded) => Some(loaded),
            Err(error) => {
                eprintln!("oxide play: HUD layer skipped (asset unavailable): {error}");
                None
            }
        }
    }

    fn load_inner(vfs: &Vfs, first_page: usize) -> Result<(Hud, Vec<PageImage>), RenderError> {
        let bar = load_png_page(vfs, HEALTH_BAR_PATH, ImageSampler::linear())?;
        let bf = load_png_page(vfs, BF_ICON_PATH, ImageSampler::linear())?;
        let dad = load_png_page(vfs, DAD_ICON_PATH, ImageSampler::linear())?;

        check_size(HEALTH_BAR_PATH, &bar, HEALTH_BAR_SIZE)?;
        check_size(BF_ICON_PATH, &bf, BF_ICON_SHEET_SIZE)?;
        check_size(DAD_ICON_PATH, &dad, DAD_ICON_SIZE)?;

        let bar_page = first_page;
        let bf_page = first_page + 1;
        let dad_page = first_page + 2;
        let specs = vec![
            LayerSpec {
                page: Some(bar_page),
                color: BAR_BACKGROUND_COLOR,
            },
            LayerSpec {
                page: Some(bar_page),
                color: DAD_FILL_COLOR,
            },
            LayerSpec {
                page: Some(bar_page),
                color: BF_FILL_COLOR,
            },
            LayerSpec {
                page: Some(dad_page),
                color: ICON_COLOR,
            },
            LayerSpec {
                page: Some(bf_page),
                color: ICON_COLOR,
            },
        ];
        Ok((Hud { specs }, vec![bar, bf, dad]))
    }

    pub(crate) fn layer_specs(&self) -> &[LayerSpec] {
        &self.specs
    }

    /// Draws the statics above the game scene. One empty vector per layer is
    /// returned when a fill has zero width, keeping the layer count stable.
    pub(crate) fn draw(&self, health: f64) -> Vec<Vec<Quad>> {
        let layout = HudLayout::new(health);
        vec![
            vec![hud_quad(layout.bar, FULL_UV)],
            layout
                .dad_fill
                .map(|rect| vec![hud_quad(rect, FULL_UV)])
                .unwrap_or_default(),
            layout
                .bf_fill
                .map(|rect| vec![hud_quad(rect, FULL_UV)])
                .unwrap_or_default(),
            vec![hud_quad(layout.dad_icon, FULL_UV)],
            vec![hud_quad(
                layout.bf_icon,
                subrect_uv_mirrored(
                    source_rect_uv(BF_ICON_SHEET_SIZE, BF_ICON_SOURCE)[0],
                    source_rect_uv(BF_ICON_SHEET_SIZE, BF_ICON_SOURCE)[2],
                    true,
                ),
            )],
        ]
    }
}

fn check_size(path: &str, page: &PageImage, expected: (u32, u32)) -> Result<(), RenderError> {
    let size = page.image.size();
    if (size.x, size.y) != expected {
        return Err(RenderError::Unsupported {
            detail: format!(
                "HUD asset {path:?} is {}x{}, expected {}x{}",
                size.x, size.y, expected.0, expected.1
            ),
        });
    }
    Ok(())
}

fn hud_quad(rect: HudRect, uv: [[f64; 2]; 4]) -> Quad {
    quad_from_matrix(
        [1.0, 0.0, 0.0, 1.0, rect.left, rect.top],
        rect.width,
        rect.height,
        uv,
    )
}

/// UV rect for a source rectangle inside a page (`(x, y, width, height)`).
fn source_rect_uv(page_size: (u32, u32), source: [f64; 4]) -> [[f64; 2]; 4] {
    let x0 = source[0] / page_size.0 as f64;
    let y0 = source[1] / page_size.1 as f64;
    let x1 = (source[0] + source[2]) / page_size.0 as f64;
    let y1 = (source[1] + source[3]) / page_size.1 as f64;
    subrect_uv([x0, y0], [x1, y1])
}

/// Horizontal UVs for a source sub-rect; `mirror` flips the u-axis within the
/// sub-rect (the bf health icon is drawn facing RIGHT in the raw sheet and
/// Funkin flips it for player 1: `iconP1.flipX = !iconP1.flipX`,
/// HealthIcon.hx:200 + PlayState/BaseCharacter.hx `initHealthIcon(false)`).
fn subrect_uv(start: [f64; 2], end: [f64; 2]) -> [[f64; 2]; 4] {
    subrect_uv_mirrored(start, end, false)
}

fn subrect_uv_mirrored(start: [f64; 2], end: [f64; 2], mirror: bool) -> [[f64; 2]; 4] {
    let (u0, u1) = if mirror {
        (end[0], start[0])
    } else {
        (start[0], end[0])
    };
    [[u0, start[1]], [u1, start[1]], [u1, end[1]], [u0, end[1]]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "expected {expected}, got {actual}"
        );
    }

    fn rect(left: f64, top: f64, width: f64, height: f64) -> HudRect {
        HudRect {
            left,
            top,
            width,
            height,
        }
    }

    #[test]
    fn bar_matches_the_observed_playstate_placement() {
        let layout = HudLayout::new(1.0);
        close(BAR_TOP, 648.0);
        close(BAR_LEFT, 339.5);
        assert_eq!(layout.bar, rect(339.5, 648.0, 601.0, 19.0));
    }

    #[test]
    fn neutral_health_splits_the_fill_and_icons_at_the_center() {
        let layout = HudLayout::new(1.0);
        close(layout.split_x, 640.0);
        assert_eq!(layout.dad_fill, Some(rect(343.5, 652.0, 296.5, 11.0)));
        assert_eq!(layout.bf_fill, Some(rect(640.0, 652.0, 296.5, 11.0)));
        // dad x = split - (150 - 26); bf x = split - 26; y = 652 - 75.
        assert_eq!(layout.dad_icon, rect(516.0, 577.0, 150.0, 150.0));
        assert_eq!(layout.bf_icon, rect(614.0, 577.0, 150.0, 150.0));
    }

    #[test]
    fn full_and_empty_health_use_the_observed_split_extremes() {
        let full = HudLayout::new(2.0);
        close(full.split_x, 343.5);
        assert_eq!(full.dad_fill, None);
        assert_eq!(full.bf_fill, Some(rect(343.5, 652.0, 593.0, 11.0)));
        assert_eq!(full.dad_icon, rect(219.5, 577.0, 150.0, 150.0));
        assert_eq!(full.bf_icon, rect(317.5, 577.0, 150.0, 150.0));

        let empty = HudLayout::new(0.0);
        close(empty.split_x, 936.5);
        assert_eq!(empty.dad_fill, Some(rect(343.5, 652.0, 593.0, 11.0)));
        assert_eq!(empty.bf_fill, None);
        assert_eq!(empty.dad_icon, rect(812.5, 577.0, 150.0, 150.0));
        assert_eq!(empty.bf_icon, rect(910.5, 577.0, 150.0, 150.0));
    }

    #[test]
    fn health_input_is_clamped_and_non_finite_is_empty() {
        assert_eq!(HudLayout::new(5.0), HudLayout::new(2.0));
        assert_eq!(HudLayout::new(-1.0), HudLayout::new(0.0));
        assert_eq!(HudLayout::new(f64::NAN), HudLayout::new(0.0));
        assert_eq!(HudLayout::new(f64::INFINITY), HudLayout::new(0.0));
    }

    #[test]
    fn bf_icon_draws_the_first_150x150_frame_mirrored() {
        // Funkin mirrors the player health icon (HealthIcon.hx:200 +
        // `initHealthIcon(false)`): flip the u-axis inside the sub-rect.
        assert_eq!(
            subrect_uv_mirrored(
                source_rect_uv(BF_ICON_SHEET_SIZE, BF_ICON_SOURCE)[0],
                source_rect_uv(BF_ICON_SHEET_SIZE, BF_ICON_SOURCE)[2],
                true
            ),
            [[0.5, 0.0], [0.0, 0.0], [0.0, 1.0], [0.5, 1.0]]
        );
    }

    #[test]
    fn draw_keeps_the_layer_count_stable_for_every_health_value() {
        let hud = Hud {
            specs: (0..5)
                .map(|_| LayerSpec {
                    page: None,
                    color: [0.0; 4],
                })
                .collect(),
        };
        for health in [0.0, 0.5, 1.0, 1.5, 2.0] {
            let layers = hud.draw(health);
            assert_eq!(layers.len(), hud.layer_specs().len());
            assert_eq!(layers[0].len(), 1);
            assert_eq!(layers[3].len(), 1);
            assert_eq!(layers[4].len(), 1);
        }
        assert!(hud.draw(0.0)[2].is_empty(), "no bf fill at zero health");
        assert!(hud.draw(2.0)[1].is_empty(), "no dad fill at full health");
    }
}
