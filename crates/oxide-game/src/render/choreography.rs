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
use super::load::{AtlasSheet, Role, SpriteFrame, SpriteSheet};
use super::scene::{
    Quad, camera_projection_matrix, concat, prop_world_matrix, quad_from_matrix, quad_uv,
    sprite_frame_matrix,
};
use oxide_audio::conductor::{Conductor, Dispatch, TimeChange};
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
    /// Darnell cutscene can (`wked1_cutscene_1_can`). Loaded only for the
    /// darnell family because only that chart emits `can_cutscene`; the other
    /// weekend-1 charts keep their page sets unchanged.
    pub(crate) cutscene_can_sheet: Option<SpriteSheet>,
    /// `can kicked up0` frames (the native `up` clip).
    pub(crate) cutscene_can_up: Option<CanClip>,
    /// `can kick quick0` frames (the native `forward` clip).
    pub(crate) cutscene_can_forward: Option<CanClip>,
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

/// The darnell cutscene can's two native clips (`wked1_cutscene_1_can`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CutsceneCanClip {
    /// `can kicked up0` (6.4 s).
    Up,
    /// `can kick quick0` (6.9 s).
    Forward,
}

/// The separate darnell cutscene can instance. It is not one of the 2hot
/// `spawnedCans`: no state machine, no explosion and no darken timer; it only
/// plays the two non-looped Sparrow clips at 24 fps.
struct CutsceneCanRuntime {
    position: Vec2,
    clip: CutsceneCanClip,
    elapsed_ms: f64,
}

impl CutsceneCanRuntime {
    fn frame_index(&self, frames: usize) -> usize {
        if frames == 0 {
            return 0;
        }
        let raw = (self.elapsed_ms / 1000.0 * CAN_FRAME_RATE).floor().max(0.0);
        let index = if raw >= usize::MAX as f64 {
            usize::MAX
        } else {
            raw as usize
        };
        index.min(frames - 1)
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
/// pending native darken timer. It also owns the darnell cutscene can and the
/// 7.1 s `Can Shot` prop, which draw through the same can sheet/draw path.
pub(crate) struct Weekend1Runtime {
    props: Weekend1Props,
    cans: Vec<CanRuntime>,
    /// Darnell cutscene can (`pile.x + 30`, `pile.y - 320`, `zIndex pile-1`).
    cutscene_can: Option<CutsceneCanRuntime>,
    /// The 7.1 s `SpraycanAtlasSprite` at `pile.x - 10`, `pile.y - 550`,
    /// native zIndex 300. A separate instance from the 2hot spawned cans.
    can_shot_prop: Option<CanRuntime>,
    big_explosions: Vec<ExplosionRuntime>,
    ez_explosions: Vec<ExplosionRuntime>,
    pending_darken_ms: Option<f64>,
}

impl Weekend1Runtime {
    pub(crate) fn new(props: Weekend1Props) -> Self {
        Self {
            props,
            cans: Vec::new(),
            cutscene_can: None,
            can_shot_prop: None,
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

    /// Native zIndex of the 7.1 s `SpraycanAtlasSprite` (`300`), above the
    /// stage's mid-ground props and below the spraycan pile.
    /// Consumed by the cutscene layer routing slice (the 7.1 s prop draws
    /// on its own z slot); temporary allow until the draw path uses it.
    #[allow(dead_code)]
    pub(crate) fn can_shot_prop_z(&self) -> f64 {
        CUTSCENE_CAN_SHOT_PROP_Z
    }

    /// Creates/restarts the darnell cutscene can and plays one of its two
    /// clips. The can is a separate instance from the 2hot `spawnedCans`.
    pub(crate) fn play_cutscene_can(&mut self, clip: CutsceneCanClip) {
        let Some((pile_x, pile_y, _)) = self.props.pile else {
            return;
        };
        let can = self.cutscene_can.get_or_insert(CutsceneCanRuntime {
            position: [pile_x + 30.0, pile_y - 320.0],
            clip,
            elapsed_ms: 0.0,
        });
        can.clip = clip;
        can.elapsed_ms = 0.0;
    }

    /// 7.1 s `SpraycanAtlasSprite`: a SHOT-state can at `pile.x - 10`,
    /// `pile.y - 550` whose frame 3 spawns the big explosion and whose
    /// `1/24 s` darken timer starts immediately.
    pub(crate) fn play_can_shot_prop(&mut self, now_ms: f64) {
        let Some((pile_x, pile_y, _)) = self.props.pile else {
            return;
        };
        self.can_shot_prop = Some(CanRuntime {
            position: [pile_x - 10.0, pile_y - 550.0],
            state: CanState::Shot,
            elapsed_ms: 0.0,
            alive: true,
            clip: CanClipName::Shot,
            big_spawned: false,
        });
        self.pending_darken_ms = Some(now_ms + DARKEN_DELAY_MS);
    }

    #[cfg(test)]
    /// Consumed by the cutscene walkthrough test; temporary allow.
    #[allow(dead_code)]
    pub(crate) fn cutscene_can_clip(&self) -> Option<CutsceneCanClip> {
        self.cutscene_can.as_ref().map(|can| can.clip)
    }

    #[cfg(test)]
    /// Consumed by the cutscene walkthrough test; temporary allow.
    #[allow(dead_code)]
    pub(crate) fn can_shot_prop_active(&self) -> bool {
        self.can_shot_prop.as_ref().is_some_and(|can| can.alive)
    }

    #[cfg(test)]
    /// Consumed by the cutscene walkthrough test; temporary allow.
    #[allow(dead_code)]
    pub(crate) fn pending_darken_ms(&self) -> Option<f64> {
        self.pending_darken_ms
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
        if let Some(can) = &mut self.can_shot_prop {
            Self::advance_can(can, &self.props, &mut new_big, &mut new_ez);
        }
        if let Some(can) = &mut self.cutscene_can {
            can.elapsed_ms += 1.0;
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

    /// Draws every alive can on the pre-registered can layer, including the
    /// separate darnell cutscene can.
    pub(crate) fn draw_can(
        &self,
        camera: &Camera,
    ) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        let sheet = &self.props.can_sheet;
        let projection = camera_projection_matrix(camera, [1.0, 1.0]);
        let mut out = Vec::new();
        for can in self.cans.iter().filter(|can| can.alive) {
            push_atlas_can_quads(&mut out, sheet, &self.props, can, projection)?;
        }
        if let (Some(can), Some(sheet)) = (&self.cutscene_can, &self.props.cutscene_can_sheet) {
            let clip = match can.clip {
                CutsceneCanClip::Up => self.props.cutscene_can_up.as_ref(),
                CutsceneCanClip::Forward => self.props.cutscene_can_forward.as_ref(),
            };
            if let Some(clip) = clip {
                let frame_index = can.frame_index(clip.frames.len());
                if let Some(frame) = clip
                    .frames
                    .get(frame_index)
                    .and_then(|frame| sheet.frame(*frame))
                {
                    let (matrix, uv) = sprite_frame_matrix(
                        can.position,
                        &frame,
                        [1.0, 1.0],
                        frame.dest_size,
                        false,
                        false,
                        0.0,
                        sheet.page_size,
                    );
                    out.push((
                        Some(sheet.page),
                        quad_from_matrix(
                            concat(projection, matrix),
                            frame.rect[2] as f64,
                            frame.rect[3] as f64,
                            uv,
                        ),
                    ));
                }
            }
        }
        Ok(out)
    }

    /// Draws the 7.1 s `Can Shot` prop on its native zIndex-300 layer.
    /// Consumed by the cutscene layer routing slice; temporary allow.
    #[allow(dead_code)]
    pub(crate) fn draw_can_shot_prop(
        &self,
        camera: &Camera,
    ) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        let Some(can) = self.can_shot_prop.as_ref().filter(|can| can.alive) else {
            return Ok(Vec::new());
        };
        let sheet = &self.props.can_sheet;
        let projection = camera_projection_matrix(camera, [1.0, 1.0]);
        let mut out = Vec::new();
        push_atlas_can_quads(&mut out, sheet, &self.props, can, projection)?;
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

/// Appends one alive can's current atlas frame quads using the native
/// `prop_world_matrix` can path (shared by the 2hot spawned cans and the 7.1 s
/// `Can Shot` prop).
fn push_atlas_can_quads(
    out: &mut Vec<(Option<usize>, Quad)>,
    sheet: &AtlasSheet,
    props: &Weekend1Props,
    can: &CanRuntime,
    projection: [f64; 6],
) -> Result<(), RenderError> {
    let clip = can.clip(props);
    let Some(frame) = clip.frames.get(can.frame_index(clip)) else {
        return Ok(());
    };
    let quads =
        sheet
            .stage
            .resolve_main_frame(*frame)
            .map_err(|source| RenderError::AtlasStage {
                path: "weekend1:spraycanAtlas".to_string(),
                source,
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
    Ok(())
}

// ---------------------------------------------------------------------------
// Darnell second-story-entry in-game cutscene (`darnell.hxc introCutscene`,
// M6-CONTRACT §14.1)
// ---------------------------------------------------------------------------

/// One step of the native timer chain. Offsets are milliseconds from the
/// `can_cutscene` trigger tick and are the frozen §14.1 values.
struct CutsceneStep {
    offset_ms: f64,
    actions: &'static [CutsceneStepAction],
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CutsceneStepAction {
    /// `tweenCameraToPosition(focus + x, focus.y, duration)`.
    CameraTo {
        role: Role,
        offset_x: f64,
        duration_ms: f64,
        ease: &'static str,
    },
    /// `tweenCameraZoom(zoom, duration, direct = true, ease)`.
    CameraZoom {
        zoom: f64,
        duration_ms: f64,
        ease: &'static str,
    },
    Character {
        role: Role,
        animation: &'static str,
        ignore_other: bool,
    },
    CanClip(CutsceneCanClip),
    CanShotProp,
    /// [D] Headless seam: the play window owns the cutscene music start.
    CutsceneMusic,
    End,
}

/// Native zIndex of the 7.1 s `SpraycanAtlasSprite`.
#[allow(dead_code)]
pub(crate) const CUTSCENE_CAN_SHOT_PROP_Z: f64 = 300.0;
/// Native `cutsceneDelay = 2.0 s`: the camera move that establishes the
/// darnell shot.
const CUTSCENE_DELAY_MS: f64 = 2000.0;
/// Black cutscene overlay removal (`0.25 s`).
const OVERLAY_REMOVE_MS: f64 = 250.0;
/// Native fade tween `startDelay 0.3`.
const OVERLAY_FADE_DELAY_MS: f64 = 300.0;
/// Native `2 s` alpha fade of the folded black surface.
const OVERLAY_FADE_MS: f64 = 2000.0;
/// Folded fade start: overlay removal (0.25 s) + startDelay (0.3 s).
const OVERLAY_FADE_START_MS: f64 = OVERLAY_REMOVE_MS + OVERLAY_FADE_DELAY_MS;
/// `isInCutscene = false` at 10.0 s (the native countdown/start flow is the
/// play-session seam, not a scene action).
const CUTSCENE_END_MS: f64 = 10_000.0;

/// The 11-step chain from M6-CONTRACT §14.1. The 0.7 s step is the music
/// start seam; the folded black-surface fade is scheduled by the overlay
/// timeline (0.25 s removal + 0.3 s startDelay = 0.55 s), matching the
/// native removal-then-camera-fade sequence.
const DARNELL_STEPS: [CutsceneStep; 11] = [
    CutsceneStep {
        offset_ms: 100.0,
        actions: &[
            CutsceneStepAction::CameraTo {
                role: Role::Player,
                offset_x: 250.0,
                duration_ms: 0.0,
                ease: "quadInOut",
            },
            CutsceneStepAction::CameraZoom {
                zoom: 1.3,
                duration_ms: 0.0,
                ease: "quadInOut",
            },
        ],
    },
    CutsceneStep {
        offset_ms: 700.0,
        actions: &[CutsceneStepAction::CutsceneMusic],
    },
    CutsceneStep {
        offset_ms: CUTSCENE_DELAY_MS,
        actions: &[
            CutsceneStepAction::CameraTo {
                role: Role::Opponent,
                offset_x: 100.0,
                duration_ms: 2500.0,
                ease: "quadInOut",
            },
            CutsceneStepAction::CameraZoom {
                zoom: 0.66,
                duration_ms: 2500.0,
                ease: "quadInOut",
            },
        ],
    },
    CutsceneStep {
        offset_ms: 5000.0,
        actions: &[CutsceneStepAction::Character {
            role: Role::Opponent,
            animation: "lightCan",
            ignore_other: false,
        }],
    },
    CutsceneStep {
        offset_ms: 6000.0,
        actions: &[
            CutsceneStepAction::Character {
                role: Role::Player,
                animation: "cock",
                ignore_other: false,
            },
            CutsceneStepAction::CameraTo {
                role: Role::Opponent,
                offset_x: 180.0,
                duration_ms: 400.0,
                ease: "backOut",
            },
        ],
    },
    CutsceneStep {
        offset_ms: 6400.0,
        actions: &[
            CutsceneStepAction::Character {
                role: Role::Opponent,
                animation: "kickCan",
                ignore_other: false,
            },
            CutsceneStepAction::CanClip(CutsceneCanClip::Up),
        ],
    },
    CutsceneStep {
        offset_ms: 6900.0,
        actions: &[
            CutsceneStepAction::Character {
                role: Role::Opponent,
                animation: "kneeCan",
                ignore_other: false,
            },
            CutsceneStepAction::CanClip(CutsceneCanClip::Forward),
        ],
    },
    CutsceneStep {
        offset_ms: 7100.0,
        actions: &[
            CutsceneStepAction::Character {
                role: Role::Player,
                animation: "intro2",
                ignore_other: true,
            },
            CutsceneStepAction::CameraTo {
                role: Role::Opponent,
                offset_x: 100.0,
                duration_ms: 1000.0,
                ease: "quadInOut",
            },
            CutsceneStepAction::CanShotProp,
        ],
    },
    CutsceneStep {
        offset_ms: 7900.0,
        actions: &[CutsceneStepAction::Character {
            role: Role::Opponent,
            animation: "laughCutscene",
            ignore_other: false,
        }],
    },
    CutsceneStep {
        offset_ms: 8200.0,
        actions: &[CutsceneStepAction::Character {
            role: Role::Girlfriend,
            animation: "laughCutscene",
            ignore_other: false,
        }],
    },
    CutsceneStep {
        offset_ms: CUTSCENE_END_MS,
        actions: &[
            CutsceneStepAction::CameraZoom {
                zoom: 0.77,
                duration_ms: 2000.0,
                ease: "sineInOut",
            },
            CutsceneStepAction::CameraTo {
                role: Role::Opponent,
                offset_x: 180.0,
                duration_ms: 2000.0,
                ease: "sineInOut",
            },
            CutsceneStepAction::End,
        ],
    },
];

/// One camera/character/can action emitted by the cutscene runtime. The scene
/// applies them through the existing 6C camera algebra and scheduler surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum CutsceneAction {
    CameraTo {
        target: Vec2,
        duration_ms: f64,
        ease: &'static str,
    },
    CameraZoom {
        zoom: f64,
        duration_ms: f64,
        ease: &'static str,
    },
    Character {
        role: Role,
        animation: &'static str,
        ignore_other: bool,
    },
    /// `dance(true)` from the cutscene-local per-beat rule.
    Dance(Role),
    CanClip(CutsceneCanClip),
    CanShotProp,
    CutsceneMusic,
    End,
}

/// The scene's camera-focus anchors, resolved by the scene from its own rigs
/// (the 6C finding-1 rule; never from row coordinates).
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct CutsceneFocus {
    pub(crate) player: Option<Vec2>,
    pub(crate) opponent: Option<Vec2>,
    pub(crate) girlfriend: Option<Vec2>,
}

impl CutsceneFocus {
    fn get(self, role: Role) -> Option<Vec2> {
        match role {
            Role::Player => self.player,
            Role::Opponent => self.opponent,
            Role::Girlfriend => self.girlfriend,
        }
    }
}

/// Character scheduler state consulted by the per-beat dance rule.
#[derive(Debug, Clone, Default)]
pub(crate) struct CutsceneDanceState {
    pub(crate) opponent_finished: bool,
    pub(crate) opponent_animation: Option<String>,
    pub(crate) girlfriend_finished: bool,
}

#[derive(Debug, Clone, Copy)]
struct CutsceneOverlay {
    alpha: f64,
    removed: bool,
}

impl Default for CutsceneOverlay {
    fn default() -> Self {
        Self {
            alpha: 0.0,
            removed: false,
        }
    }
}

/// Scene-side darnell second-entry cutscene runtime.
///
/// The trigger is consumed at most once per scene session: a second
/// `can_cutscene` row while active or after the 10 s end is ignored.
pub(crate) struct DarnellCutscene {
    started_ms: f64,
    step_index: usize,
    conductor: Conductor,
    active: bool,
    consumed: bool,
    overlay: CutsceneOverlay,
}

impl DarnellCutscene {
    /// Builds the runtime with the `darnellCanCutscene` metadata time changes.
    pub(crate) fn new(changes: Vec<TimeChange>) -> Self {
        let mut conductor = Conductor::new();
        conductor.map_time_changes(&changes);
        Self {
            started_ms: 0.0,
            step_index: 0,
            conductor,
            active: false,
            consumed: false,
            overlay: CutsceneOverlay::default(),
        }
    }

    /// Starts the chain on the `can_cutscene` trigger tick. Returns whether
    /// this call started it (false while active or after it was consumed).
    pub(crate) fn start(&mut self, now_ms: f64) -> bool {
        if self.active || self.consumed {
            return false;
        }
        self.started_ms = now_ms;
        self.step_index = 0;
        self.active = true;
        self.overlay = CutsceneOverlay {
            alpha: 1.0,
            removed: false,
        };
        true
    }

    /// Consumed by the play-window gate + the fixture family; temporary
    /// allow until those land.
    #[allow(dead_code)]
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    #[allow(dead_code)]
    pub(crate) fn consumed(&self) -> bool {
        self.consumed
    }

    /// The folded cutscene-layer black surface alpha (`1.0` from the start
    /// through the native overlay removal, then the 2 s fade).
    pub(crate) fn overlay_alpha(&self) -> f64 {
        self.overlay.alpha
    }

    #[cfg(test)]
    /// Consumed by the cutscene overlay timeline test; temporary allow.
    #[allow(dead_code)]
    pub(crate) fn overlay_removed(&self) -> bool {
        self.overlay.removed
    }

    /// Advances the cutscene on the scene's 1 ms grid. Returns the actions due
    /// this tick; the scene applies them in order.
    pub(crate) fn advance(
        &mut self,
        now_ms: f64,
        focus: &CutsceneFocus,
        dance: &CutsceneDanceState,
    ) -> Vec<CutsceneAction> {
        let mut actions = Vec::new();
        if !self.active {
            return actions;
        }
        let local_ms = (now_ms - self.started_ms).max(0.0);
        self.advance_overlay(local_ms);

        // The cutscene-local conductor drives the per-beat dance rule. Dance
        // actions are emitted before same-tick step actions so an explicit
        // `lightCan`/`kickCan`/... step wins over the beat rule.
        if local_ms < CUTSCENE_END_MS {
            for dispatch in self.conductor.update(Some(local_ms), false, false, 0.0) {
                if matches!(dispatch, Dispatch::Beat(_)) {
                    if dance.opponent_finished
                        && dance.opponent_animation.as_deref() != Some("lightCan")
                    {
                        actions.push(CutsceneAction::Dance(Role::Opponent));
                    }
                    if dance.girlfriend_finished {
                        actions.push(CutsceneAction::Dance(Role::Girlfriend));
                    }
                }
            }
        }

        while let Some(step) = DARNELL_STEPS.get(self.step_index) {
            if local_ms < step.offset_ms {
                break;
            }
            self.step_index += 1;
            for action in step.actions {
                match action {
                    CutsceneStepAction::CameraTo {
                        role,
                        offset_x,
                        duration_ms,
                        ease,
                    } => {
                        if let Some(focus) = focus.get(*role) {
                            actions.push(CutsceneAction::CameraTo {
                                target: [focus[0] + offset_x, focus[1]],
                                duration_ms: *duration_ms,
                                ease,
                            });
                        }
                    }
                    CutsceneStepAction::CameraZoom {
                        zoom,
                        duration_ms,
                        ease,
                    } => actions.push(CutsceneAction::CameraZoom {
                        zoom: *zoom,
                        duration_ms: *duration_ms,
                        ease,
                    }),
                    CutsceneStepAction::Character {
                        role,
                        animation,
                        ignore_other,
                    } => actions.push(CutsceneAction::Character {
                        role: *role,
                        animation,
                        ignore_other: *ignore_other,
                    }),
                    CutsceneStepAction::CanClip(clip) => {
                        actions.push(CutsceneAction::CanClip(*clip));
                    }
                    CutsceneStepAction::CanShotProp => {
                        actions.push(CutsceneAction::CanShotProp);
                    }
                    CutsceneStepAction::CutsceneMusic => {
                        actions.push(CutsceneAction::CutsceneMusic);
                    }
                    CutsceneStepAction::End => {
                        actions.push(CutsceneAction::End);
                        self.active = false;
                        self.consumed = true;
                    }
                }
            }
        }
        actions
    }

    fn advance_overlay(&mut self, local_ms: f64) {
        if local_ms >= OVERLAY_REMOVE_MS {
            self.overlay.removed = true;
        }
        if local_ms >= OVERLAY_FADE_START_MS {
            let t = ((local_ms - OVERLAY_FADE_START_MS) / OVERLAY_FADE_MS).clamp(0.0, 1.0);
            self.overlay.alpha = 1.0 - t;
        } else {
            self.overlay.alpha = 1.0;
        }
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
