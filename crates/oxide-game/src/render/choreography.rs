//! Weekend-1 (2hot) note-kind choreography for the deterministic scene.
//!
//! This is the engine-internal reimplementation of the shipped content
//! scripts (`assets/preload/scripts/songs/2hot.hxc` and
//! `stages/props/SpraycanAtlasSprite.hxc`), keyed by the chart note kinds that
//! the schema-2/3 sim rows carry (`docs/M6-CONTRACT.md` §14.1/§14.2 [D] 1).
//!
//! Runtime surface:
//! - `weekend-1-kickcan` (an opponent note in the shipped chart) spawns a
//!   `SpraycanAtlasSprite` at `spraycanPile.x - 10`, `spraycanPile.y - 550`
//!   and plays `Can Start`; the prop is sorted at `pile.zIndex - 1` by the
//!   scene's pre-registered can layer.
//! - `weekend-1-firegun[/-hip/-far]` HIT fires the next ARCING can
//!   (`STATE_SHOT`, `Can Shot`) and schedules the native `1/24 s` darken
//!   pass. The sim only emits that row when the 1 s `gunCocked` window was
//!   open, so the scene needs no second window copy.
//! - the same MISS marks the next ARCING can `STATE_IMPACTED`; its animation
//!   keeps playing (native `finishCanAnimation` never consults the state).
//!   The health loss already came through the `Miss` row.
//! - `lightcan`/`kneecan`/`cockgun` rows have no scene effect: the shipped
//!   chart carries no `PlayAnimation` events for them, and the dad's normal
//!   opponent auto-hit sing animations already play from the `OpponentHit`
//!   rows.
//!
//! [D] Can state selection is private: a killed can stays in the spawn list
//! like the native `spawnedCans` array but is never selected again. The
//! `Can Start -> Hit Pico -> spraypaintExplosionEZ` chain and the
//! `Can Shot` frame-3 `SpraypaintExplosion` spawn are the native one-shot
//! timings at the atlas 24 fps.

use super::RenderError;
use super::geometry::{Camera, Vec2};
use super::load::{AtlasSheet, SpriteFrame, SpriteSheet};
use super::scene::{
    Quad, camera_projection_matrix, concat, prop_world_matrix, quad_from_matrix, quad_uv,
    sprite_frame_matrix,
};
/// The spraycan atlas frame rate (`MD.FRT` 24, the native animate default).
const CAN_FRAME_RATE: f64 = 24.0;
/// `new FlxTimer().start(1 / 24, ...)` before `darkenStageProps()`.
const DARKEN_DELAY_MS: f64 = 1000.0 / 24.0;
/// `FlxTween.color(..., 1.4, 0xFF222222, 0xFFFFFFFF)`.
pub(crate) const DARKEN_HOLD_MS: f64 = 1000.0 / 24.0;
pub(crate) const DARKEN_FADE_MS: f64 = 1400.0;
/// `0xFF111111` / `0xFF222222` byte channels.
pub(crate) const DARK_FIRST: f64 = 0x11 as f64 / 255.0;
pub(crate) const DARK_SECOND: f64 = 0x22 as f64 / 255.0;

const KICKCAN_KIND: &str = "weekend-1-kickcan";
const FIREGUN_KINDS: [&str; 3] = [
    "weekend-1-firegun",
    "weekend-1-firegun-hip",
    "weekend-1-firegun-far",
];

/// One animate-atlas clip in main-timeline frame order (`label_frames`).
#[derive(Debug, Clone)]
pub(crate) struct CanClip {
    pub(crate) frames: Vec<usize>,
}

/// The weekend-1 page payload: the spraycan atlas plus the two Sparrow
/// explosions, and the loaded `spraycanPile` anchor.
pub(crate) struct Weekend1Props {
    pub(crate) can_sheet: AtlasSheet,
    pub(crate) can_start: CanClip,
    pub(crate) can_shot: CanClip,
    pub(crate) hit_pico: CanClip,
    pub(crate) big_sheet: SpriteSheet,
    pub(crate) big_frames: Vec<SpriteFrame>,
    pub(crate) ez_sheet: SpriteSheet,
    pub(crate) ez_frames: Vec<SpriteFrame>,
    /// `spraycanPile` `(x, y, zIndex)`; absent on stages that carry no pile
    /// (their charts carry no weekend-1 kinds, so no can can spawn).
    pub(crate) pile: Option<(f64, f64, f64)>,
}

/// Native `currentState` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CanState {
    Arcing,
    Shot,
    Impacted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanClipName {
    Start,
    Shot,
    HitPico,
}

pub(crate) struct CanRuntime {
    pub(crate) position: Vec2,
    pub(crate) state: CanState,
    pub(crate) elapsed_ms: f64,
    pub(crate) alive: bool,
    clip: CanClipName,
    big_spawned: bool,
}

impl CanRuntime {
    fn clip<'a>(&self, props: &'a Weekend1Props) -> &'a CanClip {
        match self.clip {
            CanClipName::Start => &props.can_start,
            CanClipName::Shot => &props.can_shot,
            CanClipName::HitPico => &props.hit_pico,
        }
    }

    fn raw_frame_index(&self) -> usize {
        let raw = (self.elapsed_ms / 1000.0 * CAN_FRAME_RATE).floor().max(0.0);
        if raw >= usize::MAX as f64 {
            usize::MAX
        } else {
            raw as usize
        }
    }

    fn frame_index(&self, clip: &CanClip) -> usize {
        if clip.frames.is_empty() {
            return 0;
        }
        self.raw_frame_index().min(clip.frames.len() - 1)
    }

    fn finished(&self, clip: &CanClip) -> bool {
        !clip.frames.is_empty() && self.raw_frame_index() >= clip.frames.len()
    }
}

/// One one-shot explosion sprite (`SpraypaintExplosion`/`spraypaintExplosionEZ`):
/// a screen position, a per-tick elapsed clock and the flixel `finishCallback`
/// kill rule.
pub(crate) struct ExplosionRuntime {
    position: Vec2,
    elapsed_ms: f64,
    alive: bool,
}

impl ExplosionRuntime {
    fn new(position: Vec2) -> Self {
        Self {
            position,
            elapsed_ms: 0.0,
            alive: true,
        }
    }

    /// One 1 ms tick; the explosion dies after the last frame.
    fn advance(&mut self, frame_count: usize) {
        if !self.alive {
            return;
        }
        self.elapsed_ms += 1.0;
        let raw = (self.elapsed_ms / 1000.0 * CAN_FRAME_RATE).floor().max(0.0);
        if raw >= frame_count as f64 {
            self.alive = false;
        }
    }

    fn frame_index(&self, frame_count: usize) -> usize {
        let raw = (self.elapsed_ms / 1000.0 * CAN_FRAME_RATE).floor().max(0.0);
        let index = if raw >= usize::MAX as f64 {
            usize::MAX
        } else {
            raw as usize
        };
        index.min(frame_count.saturating_sub(1))
    }
}

/// Scene-side weekend-1 runtime: spawned cans, one-shot explosions and the
/// pending native darken timer.
pub(crate) struct Weekend1Runtime {
    props: Weekend1Props,
    cans: Vec<CanRuntime>,
    big_explosions: Vec<ExplosionRuntime>,
    ez_explosions: Vec<ExplosionRuntime>,
    pending_darken_ms: Option<f64>,
}

impl Weekend1Runtime {
    pub(crate) fn new(props: Weekend1Props) -> Self {
        Self {
            props,
            cans: Vec::new(),
            big_explosions: Vec::new(),
            ez_explosions: Vec::new(),
            pending_darken_ms: None,
        }
    }

    pub(crate) fn can_page(&self) -> usize {
        self.props.can_sheet.page
    }

    pub(crate) fn big_page(&self) -> usize {
        self.props.big_sheet.page
    }

    pub(crate) fn ez_page(&self) -> usize {
        self.props.ez_sheet.page
    }

    /// `spraycanPile.zIndex - 1`, or 0 when the stage has no pile.
    pub(crate) fn can_z(&self) -> f64 {
        self.props.pile.map_or(0.0, |(_, _, z_index)| z_index - 1.0)
    }

    /// Applies one `Hit`/`OpponentHit` row's chart note kind.
    pub(crate) fn on_hit(&mut self, kind: &str, now_ms: f64) {
        if kind == KICKCAN_KIND {
            self.spawn_can();
        } else if FIREGUN_KINDS.contains(&kind) {
            self.shoot_next_can(now_ms);
        }
        // lightcan/kneecan/cockgun: timing anchors only (the dad's normal
        // opponent sing animation is driven by the same OpponentHit row).
    }

    /// Applies one `Miss` row's chart note kind.
    pub(crate) fn on_miss(&mut self, kind: &str) {
        if !FIREGUN_KINDS.contains(&kind) {
            return;
        }
        if let Some(can) = self
            .cans
            .iter_mut()
            .find(|can| can.alive && can.state == CanState::Arcing)
        {
            // [D] Native `missNextCan()` only changes the state; the running
            // animation (and its finish chain) keeps drawing.
            can.state = CanState::Impacted;
        }
    }

    /// Advances every can/explosion by one 1 ms tick and reports whether the
    /// native `1/24 s` darken timer is due.
    pub(crate) fn advance(&mut self, now_ms: f64) -> bool {
        let darken_due = self
            .pending_darken_ms
            .is_some_and(|pending| now_ms >= pending);
        if darken_due {
            self.pending_darken_ms = None;
        }

        let mut new_big = Vec::new();
        let mut new_ez = Vec::new();
        for can in &mut self.cans {
            Self::advance_can(can, &self.props, &mut new_big, &mut new_ez);
        }
        self.big_explosions.append(&mut new_big);
        self.ez_explosions.append(&mut new_ez);
        for explosion in &mut self.big_explosions {
            explosion.advance(self.props.big_frames.len());
        }
        for explosion in &mut self.ez_explosions {
            explosion.advance(self.props.ez_frames.len());
        }
        darken_due
    }

    fn advance_can(
        can: &mut CanRuntime,
        props: &Weekend1Props,
        big: &mut Vec<ExplosionRuntime>,
        ez: &mut Vec<ExplosionRuntime>,
    ) {
        if !can.alive {
            return;
        }
        can.elapsed_ms += 1.0;
        let clip = can.clip(props);
        let frame_count = clip.frames.len();
        if frame_count == 0 {
            can.alive = false;
            return;
        }
        let finished = can.finished(clip);
        match can.clip {
            CanClipName::Start => {
                if finished {
                    // Native `finishCanAnimation('Can Start')` -> playHitPico.
                    can.clip = CanClipName::HitPico;
                    can.elapsed_ms = 0.0;
                }
            }
            CanClipName::HitPico => {
                if finished {
                    // Native `finishCanAnimation('Hit Pico')` ->
                    // playHitExplosion() at +750/-100, then kill.
                    ez.push(ExplosionRuntime::new([
                        can.position[0] + 750.0,
                        can.position[1] - 100.0,
                    ]));
                    can.alive = false;
                }
            }
            CanClipName::Shot => {
                // `onCanFrame`: frame 3 of 'Can Shot' spawns the big
                // explosion at +150/-250; `finishCanAnimation` kills the can.
                if !can.big_spawned && can.frame_index(clip) >= 3 {
                    can.big_spawned = true;
                    big.push(ExplosionRuntime::new([
                        can.position[0] + 150.0,
                        can.position[1] - 250.0,
                    ]));
                }
                if finished {
                    can.alive = false;
                }
            }
        }
    }

    fn spawn_can(&mut self) {
        let Some((pile_x, pile_y, _)) = self.props.pile else {
            // Fail-closed at load time when the chart carries kinds; this is
            // unreachable for the shipped 2hot content.
            return;
        };
        self.cans.push(CanRuntime {
            position: [pile_x - 10.0, pile_y - 550.0],
            state: CanState::Arcing,
            clip: CanClipName::Start,
            elapsed_ms: 0.0,
            alive: true,
            big_spawned: false,
        });
    }

    fn shoot_next_can(&mut self, now_ms: f64) {
        let Some(can) = self
            .cans
            .iter_mut()
            .find(|can| can.alive && can.state == CanState::Arcing)
        else {
            return;
        };
        can.state = CanState::Shot;
        can.clip = CanClipName::Shot;
        can.elapsed_ms = 0.0;
        can.big_spawned = false;
        // The darken timer starts inside the native `if (can != null)`.
        self.pending_darken_ms = Some(now_ms + DARKEN_DELAY_MS);
    }

    /// Draws every alive can on the pre-registered can layer.
    pub(crate) fn draw_can(
        &self,
        camera: &Camera,
    ) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        let sheet = &self.props.can_sheet;
        let projection = camera_projection_matrix(camera, [1.0, 1.0]);
        let mut out = Vec::new();
        for can in self.cans.iter().filter(|can| can.alive) {
            let clip = can.clip(&self.props);
            let Some(frame) = clip.frames.get(can.frame_index(clip)) else {
                continue;
            };
            let quads = sheet.stage.resolve_main_frame(*frame).map_err(|source| {
                RenderError::AtlasStage {
                    path: "weekend1:spraycanAtlas".to_string(),
                    source,
                }
            })?;
            let world = prop_world_matrix(
                sheet.bounds,
                [1.0, 1.0],
                false,
                false,
                can.position,
                [0.0, 0.0],
            );
            for quad in &quads {
                let [_, _, width, height] = quad.atlas_rect;
                out.push((
                    Some(sheet.page),
                    quad_from_matrix(
                        concat(projection, concat(world, quad.matrix)),
                        width as f64,
                        height as f64,
                        quad_uv(quad.atlas_rect, sheet.page_size),
                    ),
                ));
            }
        }
        Ok(out)
    }

    pub(crate) fn draw_big_explosion(
        &self,
        camera: &Camera,
    ) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        draw_explosions(
            &self.props.big_sheet,
            &self.props.big_frames,
            &self.big_explosions,
            camera,
        )
    }

    pub(crate) fn draw_ez_explosion(
        &self,
        camera: &Camera,
    ) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        draw_explosions(
            &self.props.ez_sheet,
            &self.props.ez_frames,
            &self.ez_explosions,
            camera,
        )
    }
}

/// [D] The darken pass excludes the character-named stage props, the native
/// `picoFade` zIndex term (`bf.zIndex - 3`) and spawned cans. Cans are runtime
/// objects and never appear in the stage prop list, so the identity check is
/// structural here.
pub(crate) fn prop_darken_eligible(name: &str, z_index: f64, bf_z: Option<f64>) -> bool {
    if matches!(name, "bf" | "dad" | "gf") {
        return false;
    }
    if bf_z.is_some_and(|bf_z| z_index == bf_z - 3.0) {
        return false;
    }
    true
}

/// First darken color (`0xFF111111`) as a material multiplier.
pub(crate) fn darken_first_color() -> [f32; 4] {
    [DARK_FIRST as f32, DARK_FIRST as f32, DARK_FIRST as f32, 1.0]
}

/// Second darken color (`0xFF222222`) as a material multiplier.
pub(crate) fn darken_second_color() -> [f32; 4] {
    [
        DARK_SECOND as f32,
        DARK_SECOND as f32,
        DARK_SECOND as f32,
        1.0,
    ]
}

/// The `FlxTween.color` channel at `elapsed_ms` of the 1.4 s fade from
/// `0xFF222222` to `0xFFFFFFFF`.
pub(crate) fn darken_fade_color(elapsed_ms: f64) -> [f32; 4] {
    let t = (elapsed_ms / DARKEN_FADE_MS).clamp(0.0, 1.0);
    let channel = (0x22 as f64 + (0xFF - 0x22) as f64 * t) / 255.0;
    [channel as f32, channel as f32, channel as f32, 1.0]
}

fn draw_explosions(
    sheet: &SpriteSheet,
    frames: &[SpriteFrame],
    explosions: &[ExplosionRuntime],
    camera: &Camera,
) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
    if frames.is_empty() {
        return Ok(Vec::new());
    }
    let hitbox = sheet.first_dest().ok_or_else(|| RenderError::Unsupported {
        detail: "weekend-1 explosion sheet has no frames".to_string(),
    })?;
    let projection = camera_projection_matrix(camera, [1.0, 1.0]);
    let mut out = Vec::new();
    for explosion in explosions.iter().filter(|explosion| explosion.alive) {
        let frame = &frames[explosion.frame_index(frames.len())];
        let (matrix, uv) = sprite_frame_matrix(
            explosion.position,
            frame,
            [1.0, 1.0],
            hitbox,
            false,
            false,
            0.0,
            sheet.page_size,
        );
        out.push((
            Some(frame.page),
            quad_from_matrix(
                concat(projection, matrix),
                frame.rect[2] as f64,
                frame.rect[3] as f64,
                uv,
            ),
        ));
    }
    Ok(out)
}
