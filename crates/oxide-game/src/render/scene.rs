//! Private scene model for the M5 phase-2b slice renderer.
//!
//! One [`Scene`] is fed the same 1 ms sim ticks as `oxide_game::slice`. It
//! composes the stage props, bf/dad/gf characters, camera and strumline into
//! ordered draw layers of textured quads. All geometry and scene state stays in
//! `f64`; the harness converts only final vertices to Bevy `f32` (see
//! [`Quad`]).

use oxide_audio::conductor::{Conductor, Dispatch};

use super::RenderError;
use super::anim::CharacterScheduler;
use super::dialogue::{ConversationRuntime, SCREEN_PROJECTION};
use super::geometry::{
    self, BACKGROUND_ALPHA, BACKGROUND_WIDTH, BopConfig, Camera, DEFAULT_HUD_CAMERA_ZOOM,
    GAME_HEIGHT, HALF_HEIGHT, HALF_WIDTH, OPPONENT_STRUMLINE_X, PLAYER_STRUMLINE_X, STRUMLINE_Y,
    SUSTAIN_CAP_BOTTOM_CLIP, SUSTAIN_HELD_HIDE_REMAINING, SUSTAIN_HIDE_CLIP, TICK_SECONDS, Vec2,
    hold_column_uv_for, hold_columns, note_top_left, note_y_offset, receptor_top_left,
    render_distance_ms, sustain_height, sustain_held_top, sustain_top_left_x,
};
use super::hud::{HudLayers, HudState, LoadedHud};
use super::load::{
    LoadedCharacter, LoadedNote, LoadedNoteStyle, LoadedSlice, LoadedStageProp, PageImage,
    PropVisual, Role, Sheet, SpriteFrame,
};
use crate::eases::{EaseFn, resolve_ease};
use crate::slice::{InputEvent, TraceEvent};

/// Default note-style animation rate; the funkin note style has no per-animation
/// frame rate data (engine decision: 24 fps, the atlas/timeline default).
const NOTE_STYLE_FRAME_RATE: f64 = 24.0;

/// One textured quad in final Bevy world space.
///
/// Corners are in texture order top-left, top-right, bottom-right,
/// bottom-left. Positions are still `f64`; the harness casts them to `f32`
/// once when filling mesh vertices (round-to-nearest-even `as f32`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Quad {
    pub(crate) pos: [[f64; 2]; 4],
    pub(crate) uv: [[f64; 2]; 4],
}

/// Material description of one draw layer. Layers are created once at build;
/// `page` is an index into the loaded image pages, `color` is the material
/// tint (the strumline background uses an untextured black tint).
#[derive(Debug, Clone, Copy)]
pub(crate) struct LayerSpec {
    pub(crate) page: Option<usize>,
    pub(crate) color: [f32; 4],
}

/// One sorted stage-group member plus the draw layers it may write.
struct StageItem {
    kind: StageItemKind,
    /// `(page, layer_specs index)`; `None` is an untextured solid color.
    layers: Vec<(Option<usize>, usize)>,
}

#[derive(Debug, Clone, Copy)]
enum StageItemKind {
    Prop(usize),
    Character(usize),
    /// The pre-registered weekend-1 can layer (`spraycanPile.zIndex - 1`);
    /// draws nothing until a kickcan row spawns a can.
    Weekend1Can,
    /// The 7.1 s darnell-cutscene `Can Shot` prop at native zIndex 300.
    Weekend1CanShotProp,
    /// One-shot `SpraypaintExplosion` layer (native stage member appended
    /// after the loaded props, above the stage group).
    Weekend1BigExplosion,
    /// One-shot `spraypaintExplosionEZ` layer (same append position).
    Weekend1EzExplosion,
}

/// Per-character runtime state: scheduler plus the loaded sheet set.
struct CharacterRig {
    role: Role,
    id: String,
    scheduler: CharacterScheduler,
    sheets: Vec<Sheet>,
    sprite_hitbox: Option<[i32; 2]>,
    data_offsets: (f64, f64),
    scale: f64,
    flip_x: bool,
    position: Vec2,
    scroll: Vec2,
    camera_focus: Vec2,
    z_index: f64,
}

impl CharacterRig {
    fn from_loaded(loaded: LoadedCharacter) -> Self {
        let mut scheduler = CharacterScheduler::new(
            loaded.animations,
            loaded.dance_every,
            loaded.sing_time_steps,
            loaded.role == Role::Player,
        );
        // `startingAnimation` is the first animation played on creation
        // (stage props + gf); without one the native characters start on idle
        // (girlfriend has no idle and shows her first dance pose).
        let started = scheduler.start_animation(loaded.starting_animation.as_deref());
        if !started {
            if scheduler.has("idle") {
                scheduler.play("idle", true);
            } else if scheduler.has("danceLeft") {
                scheduler.play("danceLeft", true);
            }
        }
        let scale = loaded.scale;
        let bounds = loaded.bounds;
        let position = [loaded.slot.position.0, loaded.slot.position.1];
        let camera_focus = [
            position[0] + loaded.camera_offsets.0 + loaded.slot.camera_offsets.0,
            position[1] - bounds[3] * scale / 2.0
                + loaded.camera_offsets.1
                + loaded.slot.camera_offsets.1,
        ];
        Self {
            role: loaded.role,
            id: loaded.id,
            scheduler,
            sheets: loaded.sheets,
            sprite_hitbox: loaded.sprite_hitbox,
            data_offsets: loaded.data_offsets,
            scale,
            flip_x: loaded.flip_x,
            position,
            scroll: [loaded.slot.scroll.0, loaded.slot.scroll.1],
            camera_focus,
            z_index: loaded.slot.z_index,
        }
    }

    /// Pages this character may draw on, main sheet first (deterministic
    /// layer order for mixed multi-sheet characters).
    fn pages(&self) -> Vec<Option<usize>> {
        let mut pages = Vec::new();
        for sheet in &self.sheets {
            let page = sheet.page();
            if !pages.contains(&Some(page)) {
                pages.push(Some(page));
            }
        }
        pages
    }

    fn draw(&self, camera: &Camera) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        let (sheet_index, frame_value, animation) = match self.scheduler.current_visual_frame() {
            Some((animation, frame)) => (animation.sheet, frame, Some(animation)),
            None => (0, 0, None),
        };
        let Some(sheet) = self.sheets.get(sheet_index) else {
            // Loaded characters always have a main sheet; an animation index
            // outside the sheet set would be a loader bug.
            return Err(RenderError::Unsupported {
                detail: format!(
                    "character {} ({}) animation selects missing sheet {sheet_index}",
                    self.id,
                    self.role.label()
                ),
            });
        };
        let animation_offsets = animation
            .and_then(|animation| animation.offsets)
            .unwrap_or(self.data_offsets);
        let shift = [
            -(animation_offsets.0 - self.data_offsets.0) * self.scale,
            -(animation_offsets.1 - self.data_offsets.1) * self.scale,
        ];
        let projection = camera_projection_matrix(camera, self.scroll);

        match sheet {
            Sheet::Atlas(sheet) => {
                let quads = match animation.and_then(|animation| animation.symbol.as_deref()) {
                    Some(symbol) => sheet
                        .stage
                        .resolve_symbol_frame(symbol, frame_value)
                        .map_err(|source| RenderError::AtlasStage {
                            path: format!("character {} ({})", self.id, self.role.label()),
                            source,
                        })?,
                    None => sheet
                        .stage
                        .resolve_main_frame(frame_value)
                        .map_err(|source| RenderError::AtlasStage {
                            path: format!("character {} ({})", self.id, self.role.label()),
                            source,
                        })?,
                };
                let bounds = sheet.bounds;
                let top_left = [
                    self.position[0] - bounds[2] * self.scale / 2.0,
                    self.position[1] - bounds[3] * self.scale,
                ];
                let world =
                    character_world_matrix(bounds, self.scale, self.flip_x, top_left, shift);
                let mut out = Vec::with_capacity(quads.len());
                for quad in &quads {
                    let [_, _, width, height] = quad.atlas_rect;
                    let matrix = concat(projection, concat(world, quad.matrix));
                    out.push((
                        Some(sheet.page),
                        quad_from_matrix(
                            matrix,
                            width as f64,
                            height as f64,
                            quad_uv(quad.atlas_rect, sheet.page_size),
                        ),
                    ));
                }
                Ok(out)
            }
            Sheet::Sprite(sheet) => {
                let frame = sheet
                    .frame(frame_value)
                    .ok_or_else(|| RenderError::MissingFrame {
                        path: format!("character {} ({})", self.id, self.role.label()),
                        prefix: format!("frame {frame_value}"),
                    })?;
                let hitbox = self
                    .sprite_hitbox
                    .or_else(|| sheet.first_dest())
                    .ok_or_else(|| RenderError::Unsupported {
                        detail: format!("character {} has a sprite sheet with no frames", self.id),
                    })?;
                let sprite_top_left = [
                    self.position[0] - hitbox[0] as f64 * self.scale / 2.0 + shift[0],
                    self.position[1] - hitbox[1] as f64 * self.scale + shift[1],
                ];
                let flip_x = self.flip_x ^ animation.is_some_and(|animation| animation.flip_x);
                let flip_y = animation.is_some_and(|animation| animation.flip_y);
                let (matrix, uv) = sprite_frame_matrix(
                    sprite_top_left,
                    &frame,
                    [self.scale, self.scale],
                    hitbox,
                    flip_x,
                    flip_y,
                    0.0,
                    sheet.page_size,
                );
                Ok(vec![(
                    Some(sheet.page),
                    quad_from_matrix(matrix, frame.rect[2] as f64, frame.rect[3] as f64, uv),
                )])
            }
        }
    }
}

/// Animated/static prop runtime state.
struct PropRig {
    prop: LoadedStageProp,
    scheduler: Option<CharacterScheduler>,
    /// One-shot structured deviation log for `applyStageMatrix` props that are
    /// loaded but intentionally not drawn (M6-CONTRACT §11.6).
    stage_matrix_log: std::sync::Once,
}

impl PropRig {
    fn from_loaded(prop: LoadedStageProp) -> Self {
        let scheduler = prop.bopper.then(|| {
            let mut scheduler = CharacterScheduler::new(
                prop.animations.clone(),
                prop.dance_every,
                super::anim::DEFAULT_SING_TIME_STEPS,
                false,
            );
            scheduler.start_animation(prop.starting_animation.as_deref());
            scheduler
        });
        Self {
            prop,
            scheduler,
            stage_matrix_log: std::sync::Once::new(),
        }
    }

    fn pages(&self) -> Vec<Option<usize>> {
        match &self.prop.visual {
            PropVisual::Solid { .. } => vec![None],
            PropVisual::Static { page, .. } => vec![Some(*page)],
            PropVisual::Sprite { sheet, .. } => vec![Some(sheet.page)],
            PropVisual::Atlas { sheet, .. } => vec![Some(sheet.page)],
        }
    }
}

/// Per-layer material tint for one stage prop. Solid colors carry their own
/// RGB; `dataProp.alpha` multiplies the material alpha unconditionally for
/// solid and textured props alike (`propSprite.alpha = dataProp.alpha`,
/// Stage.hx; M6-CONTRACT §11.6). A zero-alpha solid therefore contributes an
/// invisible blended quad instead of an opaque overlay.
fn prop_layer_color(prop: &LoadedStageProp) -> [f32; 4] {
    let alpha = prop.alpha as f32;
    match &prop.visual {
        PropVisual::Solid { color, .. } => [color[0], color[1], color[2], color[3] * alpha],
        _ => [1.0, 1.0, 1.0, alpha],
    }
}

/// A note plus its render/removal state.
struct NoteState {
    note: LoadedNote,
    hit: bool,
    missed: bool,
}

struct PlayerHold {
    note_id: u32,
    remaining_ms: f64,
}

struct OpponentHold {
    end_ms: f64,
}

/// Strumline index in the native `playerStrumline`/`opponentStrumline` order.
const LINE_PLAYER: usize = 0;
const LINE_OPPONENT: usize = 1;

#[derive(Debug, Clone, Copy)]
struct FollowTween {
    from: Vec2,
    to: Vec2,
    start_ms: f64,
    duration_ms: f64,
    ease: EaseFn,
}

#[derive(Debug, Clone, Copy)]
struct ZoomTween {
    from: f64,
    to: f64,
    start_ms: f64,
    duration_ms: f64,
    ease: EaseFn,
}

/// One per-strumline scroll-speed channel, matching
/// `slice::events::ScrollState`'s advance/percent convention exactly.
#[derive(Debug, Clone, Copy)]
struct ScrollTween {
    from: f64,
    to: f64,
    start_ms: f64,
    duration_ms: f64,
    ease: EaseFn,
}

#[derive(Debug, Clone, Copy)]
struct ScrollLine {
    value: f64,
    tween: Option<ScrollTween>,
}

/// Native `tweenScrollSpeed`/`cancelScrollSpeedTweens`/`prevScrollTargets`
/// state (`PlayState.hx` L4039-4104) for the scene's own strumlines.
#[derive(Debug, Clone)]
struct ScrollState {
    lines: [ScrollLine; 2],
    /// Targets pushed by the immediately previous native call; a later event
    /// cancels all tweens and snaps these lines back before applying.
    prev_targets: Vec<(usize, f64)>,
}

impl ScrollState {
    fn new(chart_scroll: f64) -> Self {
        let line = ScrollLine {
            value: chart_scroll,
            tween: None,
        };
        Self {
            lines: [line; 2],
            prev_targets: Vec::new(),
        }
    }

    /// Advances in-flight tweens to `now_ms` (native `super.update` runs
    /// before the event dispatch in the same frame).
    fn advance(&mut self, now_ms: f64) {
        for line in &mut self.lines {
            let Some(tween) = line.tween else {
                continue;
            };
            if now_ms - tween.start_ms >= tween.duration_ms {
                line.value = tween.to;
                line.tween = None;
            } else if now_ms > tween.start_ms {
                let t = (now_ms - tween.start_ms) / tween.duration_ms;
                line.value = tween.from + (tween.to - tween.from) * (tween.ease)(t);
            }
        }
    }

    fn value(&self, line: usize) -> f64 {
        self.lines[line].value
    }

    /// `cancelScrollSpeedTweens()` + the `prevScrollTargets` snap loop.
    fn cancel_and_snap(&mut self) {
        for line in &mut self.lines {
            line.tween = None;
        }
        let previous = std::mem::take(&mut self.prev_targets);
        for (line, value) in previous {
            self.lines[line].value = value;
        }
    }

    fn set_value(&mut self, line: usize, value: f64) {
        self.lines[line].value = value;
        self.lines[line].tween = None;
    }

    fn start_tween(
        &mut self,
        line: usize,
        now_ms: f64,
        target: f64,
        duration_ms: f64,
        ease: EaseFn,
    ) {
        let base = self.lines[line].value;
        self.lines[line].tween = Some(ScrollTween {
            from: base,
            to: target,
            start_ms: now_ms,
            duration_ms,
            ease,
        });
    }

    fn push_previous(&mut self, line: usize, target: f64) {
        self.prev_targets.push((line, target));
    }
}

/// One parsed `scroll_speed` row (M6-CONTRACT §12.4.1).
struct ScrollRow {
    strumline: &'static str,
    line: usize,
    target: f64,
    base_scroll: f64,
    duration: f64,
    ease: Option<String>,
}

impl ScrollRow {
    fn from_event(event: &TraceEvent) -> Result<Self, RenderError> {
        let TraceEvent::ScrollSpeed {
            strumline,
            target,
            base_scroll,
            duration,
            ease,
            ..
        } = event
        else {
            return Err(RenderError::Unsupported {
                detail: "scroll row parser received a non-scroll event".to_string(),
            });
        };
        let line = match *strumline {
            "player" => LINE_PLAYER,
            "opponent" => LINE_OPPONENT,
            other => {
                return Err(RenderError::Unsupported {
                    detail: format!("scroll_speed row has unknown strumline {other:?}"),
                });
            }
        };
        if !target.is_finite()
            || !base_scroll.is_finite()
            || !duration.is_finite()
            || *duration < 0.0
        {
            return Err(RenderError::Unsupported {
                detail: format!(
                    "scroll_speed row has invalid target/base/duration ({target}, {base_scroll}, {duration})"
                ),
            });
        }
        Ok(Self {
            strumline,
            line,
            target: *target,
            base_scroll: *base_scroll,
            duration: *duration,
            ease: ease.clone(),
        })
    }
}

enum ScrollEase {
    Instant,
    Named(EaseFn),
}

fn resolve_scroll_ease(ease: Option<&str>) -> Option<ScrollEase> {
    let name = ease?;
    if name == "INSTANT" {
        Some(ScrollEase::Instant)
    } else {
        resolve_ease(name).map(|(_, function)| ScrollEase::Named(function))
    }
}

/// Private camera event state (`docs/M6-CONTRACT.md` §12.7): follow point,
/// follow tween + follow-enabled flag, zoom tween, mutable bop config, the
/// virtual HUD zoom and the per-strumline scroll state.
struct CameraEventState {
    follow_focus: Vec2,
    follow_enabled: bool,
    follow_tween: Option<FollowTween>,
    zoom_tween: Option<ZoomTween>,
    bop: BopConfig,
    hud_zoom: f64,
    scroll: ScrollState,
}

impl CameraEventState {
    fn new(follow_focus: Vec2, chart_scroll: f64) -> Self {
        Self {
            follow_focus,
            follow_enabled: true,
            follow_tween: None,
            zoom_tween: None,
            bop: BopConfig::default(),
            hud_zoom: DEFAULT_HUD_CAMERA_ZOOM,
            scroll: ScrollState::new(chart_scroll),
        }
    }

    /// Native `super.update` position: follow/zoom/scroll tweens advance
    /// BEFORE the conductor update and the event dispatch.
    fn advance_tweens(&mut self, camera: &mut Camera, now_ms: f64) {
        if let Some(tween) = self.follow_tween {
            if now_ms - tween.start_ms >= tween.duration_ms {
                camera.scroll = tween.to;
                self.follow_tween = None;
                // onComplete -> resetCamera(false, false): re-enable follow.
                self.follow_enabled = true;
            } else if now_ms > tween.start_ms {
                let t = (now_ms - tween.start_ms) / tween.duration_ms;
                let eased = (tween.ease)(t);
                camera.scroll = [
                    tween.from[0] + (tween.to[0] - tween.from[0]) * eased,
                    tween.from[1] + (tween.to[1] - tween.from[1]) * eased,
                ];
            }
        }
        if let Some(tween) = self.zoom_tween {
            if now_ms - tween.start_ms >= tween.duration_ms {
                camera.current_zoom = tween.to;
                self.zoom_tween = None;
            } else if now_ms > tween.start_ms {
                let t = (now_ms - tween.start_ms) / tween.duration_ms;
                camera.current_zoom = tween.from + (tween.to - tween.from) * (tween.ease)(t);
            }
        }
        self.scroll.advance(now_ms);
    }

    fn decay_hud(&mut self, elapsed_seconds: f64) {
        self.hud_zoom = geometry::hud_zoom_decay(self.hud_zoom, elapsed_seconds);
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct LineState {
    pressed: [bool; 4],
    press_started_ms: [f64; 4],
    confirm_started_ms: [f64; 4],
    confirm_until_ms: [f64; 4],
}

impl LineState {
    fn press(&mut self, direction: usize, now_ms: f64) {
        if direction < 4 && !self.pressed[direction] {
            self.pressed[direction] = true;
            self.press_started_ms[direction] = now_ms;
        }
    }

    fn release(&mut self, direction: usize) {
        if direction < 4 {
            // StrumlineNote release: the player receptor returns to static
            // immediately (`confirmHoldTimer = -1` / playStatic). The 0.15 s
            // auto-reset applies only to the opponent line.
            self.pressed[direction] = false;
            self.confirm_started_ms[direction] = 0.0;
            self.confirm_until_ms[direction] = 0.0;
        }
    }

    /// Player note-hit confirm: stays active while the key is held (no
    /// opponent-style 0.15 s auto-reset) and is cleared by [`Self::release`].
    fn confirm_player(&mut self, direction: usize, now_ms: f64) {
        if direction < 4 {
            self.confirm_started_ms[direction] = now_ms;
            self.confirm_until_ms[direction] = f64::INFINITY;
        }
    }

    /// Opponent auto-hit confirm, reset by the observed
    /// `StrumlineNote.confirmHoldTimer` after 0.15 s.
    fn confirm_opponent(&mut self, direction: usize, now_ms: f64) {
        if direction < 4 {
            self.confirm_started_ms[direction] = now_ms;
            self.confirm_until_ms[direction] = now_ms + super::geometry::RECEPTOR_CONFIRM_MS;
        }
    }
}

/// Private scene state consumed by the headless harness.
pub(crate) struct Scene {
    camera: Camera,
    conductor: Conductor,
    props: Vec<PropRig>,
    characters: Vec<CharacterRig>,
    stage_items: Vec<StageItem>,
    layer_specs: Vec<LayerSpec>,
    /// First overlay layer (background, receptors, holds, notes).
    overlay_start: usize,
    note_style: LoadedNoteStyle,
    notes: Vec<NoteState>,
    player_note_indices: Vec<usize>,
    /// Opponent note ids (compacted opponent-only, matching the sim's
    /// per-strumline convention) mapped to `notes` indexes.
    opponent_note_indices: Vec<usize>,
    sim_ms: f64,
    last_tick: Option<u64>,
    dead: bool,
    player_line: LineState,
    opponent_line: LineState,
    player_holds: [Option<PlayerHold>; 4],
    opponent_holds: [Option<OpponentHold>; 4],
    /// Follow/zoom/bop/scroll event state (M6-CONTRACT §12.7).
    camera_events: CameraEventState,
    /// Screen-space play HUD (M6-CONTRACT §13), appended after every
    /// stage/overlay layer. `None` only for hermetic synthetic scenes.
    hud: Option<Hud>,
    /// Index of the first HUD layer in `layer_specs`.
    hud_start: usize,
    /// 6E in-game conversation (M6-CONTRACT §14.2). Its layer group is
    /// appended after the HUD group and draws above it. `None` for the
    /// frozen render-fixture path and synthetic scenes without dialogue.
    ///
    /// The native PauseSubState "Restart Dialogue" path
    /// (`Conversation.resetConversation`) is the M7 pause-surface deliverable;
    /// it is intentionally not wired into this scene.
    conversation: Option<ConversationRuntime>,
    /// Index of the first conversation layer in `layer_specs`.
    conversation_start: usize,
    /// Weekend-1 can/explosion choreography (`None` outside the weekend-1
    /// chart ids; M6-CONTRACT §14.2).
    weekend1: Option<super::choreography::Weekend1Runtime>,
    /// The darnell in-game can-cutscene chain (`None` outside the darnell
    /// family; M6-CONTRACT §14.2).
    cutscene: Option<super::choreography::DarnellCutscene>,
    /// The cutscene overlay layer (registered only with a cutscene payload;
    /// its color is the folded black surface alpha).
    cutscene_overlay_layer: Option<usize>,
    /// The native `darkenStageProps`/fade-back tint pass over the eligible
    /// prop layer colors (`None` until a darken timer fires).
    prop_tint: Option<PropTint>,
}

/// One active tint pass over the eligible prop layers
/// (native `darkenStageProps`: `0xFF111111`, then after 1/24 s `0xFF222222`,
/// then a 1.4 s `FlxTween.color` back to white).
struct PropTint {
    /// The layer-spec indices the tint mutates (all pages of every eligible
    /// prop).
    layers: Vec<usize>,
    started_ms: f64,
}

/// One scene-owned HUD: loaded assets + runtime state + layer registry.
struct Hud {
    assets: LoadedHud,
    state: HudState,
    layers: HudLayers,
}

impl Scene {
    /// Builds the scene and returns the decoded pages the harness must insert
    /// into Bevy's `Assets<Image>`.
    pub(crate) fn new(loaded: LoadedSlice) -> Result<(Self, Vec<PageImage>), RenderError> {
        let LoadedSlice {
            mut pages,
            stage_zoom,
            props,
            characters: loaded_characters,
            note_style,
            notes,
            scroll_speed,
            conductor_changes,
            // Play-session-only metadata; the visual scene does not consume it.
            player_vocals: _,
            opponent_vocals: _,
            hud,
            dialogue,
            weekend1_props,
            cutscene_changes,
        } = loaded;
        // The HUD state is built before the character rigs consume the loaded
        // characters (it needs their normalized `healthIcon` data).
        let hud_state = match &hud {
            Some(assets) => Some(HudState::new(assets, &loaded_characters)?),
            None => None,
        };
        let mut characters = Vec::with_capacity(3);
        for character in loaded_characters {
            characters.push(CharacterRig::from_loaded(character));
        }
        let props: Vec<PropRig> = props.into_iter().map(PropRig::from_loaded).collect();
        // M6-CONTRACT §14.2 (weekend-1): the can joins the z-sorted stage
        // surface at `spraycanPile.zIndex - 1`; the explosions draw at the
        // native stage-member default zIndex 0. The pages exist only for
        // the weekend-1 chart ids (bopeebo/school stay byte-stable).
        let weekend1 = weekend1_props.map(super::choreography::Weekend1Runtime::new);
        let follow_focus = characters
            .iter()
            .find(|character| character.role == Role::Opponent)
            .map(|character| character.camera_focus)
            .ok_or_else(|| RenderError::Unsupported {
                detail: "opponent character was not loaded".to_string(),
            })?;
        let mut camera = Camera::new(stage_zoom);
        camera.focus_on(follow_focus);

        let mut sorted: Vec<(f64, usize, StageItemKind)> = Vec::new();
        for (index, prop) in props.iter().enumerate() {
            sorted.push((prop.prop.z_index, sorted.len(), StageItemKind::Prop(index)));
        }
        for (index, character) in characters.iter().enumerate() {
            sorted.push((
                character.z_index,
                sorted.len(),
                StageItemKind::Character(index),
            ));
        }
        if let Some(runtime) = &weekend1 {
            sorted.push((runtime.can_z(), sorted.len(), StageItemKind::Weekend1Can));
            sorted.push((
                runtime.can_shot_prop_z(),
                sorted.len(),
                StageItemKind::Weekend1CanShotProp,
            ));
            sorted.push((0.0, sorted.len(), StageItemKind::Weekend1BigExplosion));
            sorted.push((0.0, sorted.len(), StageItemKind::Weekend1EzExplosion));
        }
        sorted.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });

        let mut layer_specs = Vec::new();
        let mut stage_items = Vec::with_capacity(sorted.len());
        for (_, _, kind) in sorted {
            let (pages, color) = match kind {
                StageItemKind::Prop(index) => {
                    (props[index].pages(), prop_layer_color(&props[index].prop))
                }
                StageItemKind::Character(index) => {
                    (characters[index].pages(), [1.0, 1.0, 1.0, 1.0])
                }
                StageItemKind::Weekend1Can => {
                    let page = weekend1
                        .as_ref()
                        .map_or(usize::MAX, |runtime| runtime.can_page());
                    (vec![Some(page)], [1.0, 1.0, 1.0, 1.0])
                }
                StageItemKind::Weekend1CanShotProp => {
                    let page = weekend1
                        .as_ref()
                        .map_or(usize::MAX, |runtime| runtime.can_page());
                    (vec![Some(page)], [1.0, 1.0, 1.0, 1.0])
                }
                StageItemKind::Weekend1BigExplosion => {
                    let page = weekend1
                        .as_ref()
                        .map_or(usize::MAX, |runtime| runtime.big_page());
                    (vec![Some(page)], [1.0, 1.0, 1.0, 1.0])
                }
                StageItemKind::Weekend1EzExplosion => {
                    let page = weekend1
                        .as_ref()
                        .map_or(usize::MAX, |runtime| runtime.ez_page());
                    (vec![Some(page)], [1.0, 1.0, 1.0, 1.0])
                }
            };
            let mut layers = Vec::with_capacity(pages.len());
            for page in pages {
                layers.push((page, layer_specs.len()));
                layer_specs.push(LayerSpec { page, color });
            }
            stage_items.push(StageItem { kind, layers });
        }

        let overlay_start = layer_specs.len();
        layer_specs.push(LayerSpec {
            page: None,
            color: [0.0, 0.0, 0.0, BACKGROUND_ALPHA as f32],
        });
        layer_specs.push(LayerSpec {
            page: Some(note_style.receptor_page),
            color: [1.0, 1.0, 1.0, 1.0],
        });
        layer_specs.push(LayerSpec {
            page: Some(note_style.hold_page),
            color: [1.0, 1.0, 1.0, 1.0],
        });
        layer_specs.push(LayerSpec {
            page: Some(note_style.note_frames[0].page),
            color: [1.0, 1.0, 1.0, 1.0],
        });

        // M6-CONTRACT §13: HUD layers are appended AFTER every stage/overlay
        // layer (the scrollFactor (0,0) equivalent). zIndex order inside the
        // group: bar bg 800, bar fill 801, score 802, icons 850.
        let hud_start = layer_specs.len();
        let hud = match (hud, hud_state) {
            (Some(assets), Some(state)) => {
                let layers = HudLayers::build(&assets, &mut layer_specs);
                Some(Hud {
                    assets,
                    state,
                    layers,
                })
            }
            _ => None,
        };

        // M6-CONTRACT §14.2: the darnell cutscene overlay layer joins the
        // same top-of-stack region (the camCutscene equivalent; [D] the
        // native overlay removal + camera fadeBlack fold into one surface).
        let cutscene_overlay_layer = cutscene_changes.as_ref().map(|_| {
            let layer = layer_specs.len();
            layer_specs.push(LayerSpec {
                page: None,
                color: [0.0, 0.0, 0.0, 0.0],
            });
            layer
        });

        // M6-CONTRACT §14.2: the camCutscene group is appended after the HUD
        // group, so its quads draw above the gameplay HUD. The runtime bakes
        // every line's glyph pages into the same page store before `pages`
        // leaves this constructor.
        let conversation_start = layer_specs.len();
        let conversation = match dialogue {
            Some(dialogue) => Some(ConversationRuntime::new(
                dialogue,
                &mut layer_specs,
                &mut pages,
            )?),
            None => None,
        };

        let mut player_note_indices = vec![usize::MAX; notes.len()];
        let mut opponent_note_indices = vec![usize::MAX; notes.len()];
        for (index, note) in notes.iter().enumerate() {
            if let Some(note_id) = note.note_id {
                let slot = if note.opponent {
                    &mut opponent_note_indices
                } else {
                    &mut player_note_indices
                };
                let slot_index = note_id as usize; // id spaces are compacted, bounded by len
                if slot_index >= slot.len() || slot[slot_index] != usize::MAX {
                    return Err(RenderError::UnknownEventNote {
                        tick_ms: 0,
                        note_id,
                    });
                }
                slot[slot_index] = index;
            }
        }

        let mut conductor = Conductor::new();
        conductor.map_time_changes(&conductor_changes);

        Ok((
            Self {
                camera,
                conductor,
                props,
                characters,
                stage_items,
                layer_specs,
                overlay_start,
                note_style,
                notes: notes
                    .into_iter()
                    .map(|note| NoteState {
                        note,
                        hit: false,
                        missed: false,
                    })
                    .collect(),
                player_note_indices,
                opponent_note_indices,
                sim_ms: 0.0,
                last_tick: None,
                dead: false,
                player_line: LineState::default(),
                opponent_line: LineState::default(),
                player_holds: [None, None, None, None],
                opponent_holds: [None, None, None, None],
                // Rowless streams keep the frozen M5 initialization; the only
                // sanctioned change is the M6-CONTRACT §12.7 bop default
                // (cameraZoomRateOffset 1 -> 0), which lives in `BopConfig`.
                camera_events: CameraEventState::new(follow_focus, scroll_speed),
                hud,
                hud_start,
                conversation,
                conversation_start,
                weekend1,
                prop_tint: None,
                cutscene: cutscene_changes
                    .as_ref()
                    .map(|changes| super::choreography::DarnellCutscene::new(changes.clone())),
                cutscene_overlay_layer,
            },
            pages,
        ))
    }

    pub(crate) fn layer_specs(&self) -> &[LayerSpec] {
        &self.layer_specs
    }

    /// Native `darkenStageProps` (`2hot.hxc`/`darnell.hxc`): darken every
    /// eligible stage prop to `0xFF111111` (the bf/dad/gf-named props, the
    /// native `picoFade` zIndex term `bf.zIndex - 3` and the spawned cans
    /// are excluded), then finish the pass through [`Self::advance_prop_tint`].
    fn start_prop_darken(&mut self) {
        let bf_z = self
            .characters
            .iter()
            .find(|character| character.role == Role::Player)
            .map(|character| character.z_index);
        let mut layers = Vec::new();
        for item in &self.stage_items {
            let StageItemKind::Prop(index) = item.kind else {
                continue;
            };
            let prop = &self.props[index].prop;
            if !super::choreography::prop_darken_eligible(&prop.name, prop.z_index, bf_z) {
                continue;
            }
            layers.extend(item.layers.iter().map(|(_, layer)| *layer));
        }
        if layers.is_empty() {
            return;
        }
        self.prop_tint = Some(PropTint {
            layers: layers.clone(),
            started_ms: self.sim_ms,
        });
        for &layer in &layers {
            if let Some(spec) = self.layer_specs.get_mut(layer) {
                spec.color = super::choreography::darken_first_color();
            }
        }
    }

    /// Advances the tint pass: `0xFF111111` holds 1/24 s, becomes
    /// `0xFF222222`, then lerps back to white over 1.4 s (native
    /// `FlxTween.color`); the pass clears when it completes.
    fn advance_prop_tint(&mut self) {
        let Some(tint) = &self.prop_tint else {
            return;
        };
        let elapsed = self.sim_ms - tint.started_ms;
        let color = if elapsed <= super::choreography::DARKEN_HOLD_MS {
            super::choreography::darken_second_color()
        } else if elapsed
            >= super::choreography::DARKEN_HOLD_MS + super::choreography::DARKEN_FADE_MS
        {
            self.prop_tint = None;
            return;
        } else {
            super::choreography::darken_fade_color(elapsed - super::choreography::DARKEN_HOLD_MS)
        };
        if let Some(tint) = &self.prop_tint {
            for &layer in &tint.layers {
                if let Some(spec) = self.layer_specs.get_mut(layer) {
                    spec.color = color;
                }
            }
        }
    }

    /// The scene's camera-focus anchors for the cutscene step table
    /// (resolved from the scene's own rigs; the 6C finding-1 rule).
    fn cutscene_focus(&self) -> super::choreography::CutsceneFocus {
        let focus_for = |role: Role| {
            self.characters
                .iter()
                .find(|character| character.role == role)
                .map(|character| character.camera_focus)
        };
        super::choreography::CutsceneFocus {
            player: focus_for(Role::Player),
            opponent: focus_for(Role::Opponent),
            girlfriend: focus_for(Role::Girlfriend),
        }
    }

    /// The animation-finished state the cutscene per-beat dance rule reads.
    fn cutscene_dance_state(&self) -> super::choreography::CutsceneDanceState {
        let state_for = |role: Role| {
            self.characters
                .iter()
                .find(|character| character.role == role)
                .and_then(|character| character.scheduler.current_name())
                .map(str::to_owned)
        };
        let finished_for = |role: Role| {
            self.characters
                .iter()
                .find(|character| character.role == role)
                .is_none_or(|character| character.scheduler.current_finished())
        };
        super::choreography::CutsceneDanceState {
            opponent_finished: finished_for(Role::Opponent),
            opponent_animation: state_for(Role::Opponent),
            girlfriend_finished: finished_for(Role::Girlfriend),
        }
    }

    /// Applies one cutscene step-table action through the 6C camera algebra
    /// and the animation surface.
    fn apply_cutscene_action(
        &mut self,
        action: super::choreography::CutsceneAction,
    ) -> Result<(), RenderError> {
        use super::choreography::CutsceneAction;
        match action {
            CutsceneAction::CameraTo {
                target,
                duration_ms,
                ease,
            } => {
                // Native `tweenCameraToPosition(x, y, duration, ease)`: the
                // follow-tween path (duration 0 snaps and re-follows).
                self.start_follow_tween(
                    target,
                    duration_ms,
                    crate::eases::resolve_ease(ease)
                        .map(|(_, function)| function)
                        .unwrap_or(crate::eases::linear_ease),
                )?;
            }
            CutsceneAction::CameraZoom {
                zoom,
                duration_ms,
                ease,
            } => {
                // Native `tweenCameraZoom(zoom, duration, direct=true, ease)`:
                // direct mode against `FlxCamera.defaultZoom = 1`.
                self.apply_cutscene_zoom(zoom, duration_ms, ease)?;
            }
            CutsceneAction::Character {
                role,
                animation,
                ignore_other,
            } => {
                if let Some(character) = self.character_mut(role) {
                    character
                        .scheduler
                        .play_with_ignore_other(animation, true, ignore_other);
                }
            }
            CutsceneAction::Dance(role) => {
                if let Some(character) = self.character_mut(role) {
                    character.scheduler.dance(true);
                }
            }
            CutsceneAction::CanClip(clip) => {
                if let Some(runtime) = &mut self.weekend1 {
                    runtime.play_cutscene_can(clip);
                }
            }
            CutsceneAction::CanShotProp => {
                if let Some(runtime) = &mut self.weekend1 {
                    runtime.play_can_shot_prop(self.sim_ms);
                }
            }
            CutsceneAction::CutsceneMusic => {
                // [D] Headless seam: the cutscene music start lives in the
                // play session's audio surface; the scene records no state.
            }
            CutsceneAction::End => {
                // [D] The native `startCountdown()` re-entry is the play
                // session's flow; the scene marks the cutscene consumed and
                // gameplay continues.
            }
        }
        Ok(())
    }

    /// The direct-mode zoom tween for the cutscene step table (the row
    /// consumer computes `resolved_target_zoom` from the stage; the step
    /// table carries the direct zoom directly).
    fn apply_cutscene_zoom(
        &mut self,
        target: f64,
        duration_ms: f64,
        ease: &'static str,
    ) -> Result<(), RenderError> {
        if !target.is_finite() || !duration_ms.is_finite() || duration_ms < 0.0 {
            return Err(RenderError::Unsupported {
                detail: format!("cutscene zoom ({target}, {duration_ms}) is invalid"),
            });
        }
        if duration_ms <= 0.0 {
            self.camera_events.zoom_tween = None;
            self.camera.current_zoom = target;
            return Ok(());
        }
        let Some((_, function)) = crate::eases::resolve_ease(ease) else {
            return Ok(());
        };
        self.camera_events.zoom_tween = None;
        self.camera_events.zoom_tween = Some(ZoomTween {
            from: self.camera.current_zoom,
            to: target,
            start_ms: self.sim_ms,
            duration_ms,
            ease: function,
        });
        Ok(())
    }

    /// The cutscene overlay layer color sync (`None` color when absent).
    fn sync_cutscene_overlay(&mut self) {
        let Some(alpha) = self
            .cutscene
            .as_ref()
            .map(super::choreography::DarnellCutscene::overlay_alpha)
        else {
            return;
        };
        if let Some(layer) = self.cutscene_overlay_layer
            && let Some(spec) = self.layer_specs.get_mut(layer)
        {
            spec.color = [0.0, 0.0, 0.0, alpha as f32];
        }
    }

    /// Whether the loaded conversation is currently running (the native
    /// `isInCutscene` gate for the advance key).
    pub(crate) fn conversation_active(&self) -> bool {
        self.conversation
            .as_ref()
            .is_some_and(ConversationRuntime::active)
    }

    /// One scene tick, consuming only events/inputs that belong to `sim_ms`.
    pub(crate) fn step(
        &mut self,
        sim_ms: u64,
        events: &[TraceEvent],
        inputs: &[InputEvent],
    ) -> Result<(), RenderError> {
        let expected = self.last_tick.map_or(0, |previous| previous + 1);
        if sim_ms != expected {
            return Err(RenderError::TickSequence {
                previous_ms: self.last_tick,
                tick_ms: sim_ms,
            });
        }
        for event in events {
            let event_ms = trace_sim(event);
            if event_ms != sim_ms {
                return Err(RenderError::OutOfGridEvent {
                    event_ms,
                    tick_ms: sim_ms,
                });
            }
        }
        for input in inputs {
            let event_ms = match input {
                InputEvent::Press { sim_ms, .. }
                | InputEvent::Release { sim_ms, .. }
                | InputEvent::Advance { sim_ms } => *sim_ms,
            };
            if event_ms != sim_ms {
                return Err(RenderError::OutOfGridEvent {
                    event_ms,
                    tick_ms: sim_ms,
                });
            }
        }

        self.last_tick = Some(sim_ms);
        self.sim_ms = sim_ms as f64;

        // Native per-tick order (M6-CONTRACT §12.7): `super.update` advances
        // the follow/zoom/scroll tweens FIRST, then the conductor update runs
        // the bop gate, then the decay/zoom-assign block, then the event rows
        // (processSongEvents), then the follow lerp at end-of-step.
        self.camera_events
            .advance_tweens(&mut self.camera, self.sim_ms);

        let dispatches = self.conductor.update(Some(self.sim_ms), false, false, 0.0);
        if self.dead {
            // Native `subState == null` gate (PlayState.hx L1214): with the
            // GameOverSubState open the decay/zoom-assign block does not run,
            // so both the bop decay and the virtual HUD zoom FREEZE after a
            // GameOver row (review finding: this is the native behavior, not
            // a rate-gated M5 variant).
            return Ok(());
        }
        // HUD per-tick update (M6-CONTRACT §13): health clamp + fixed 0.15
        // lerp, bop-tween advance and icon state/position refresh. It runs
        // before the row pass so a same-tick `SetHealthIcon` configure keeps
        // the native one-frame idle restart.
        if let Some(hud) = &mut self.hud {
            hud.state.begin_tick(self.sim_ms);
        }
        let step_length_ms = self.conductor.step_length_ms();
        for dispatch in dispatches {
            if let Dispatch::Step(step) = dispatch {
                if self
                    .camera
                    .on_step(step, &self.camera_events.bop, self.camera_events.hud_zoom)
                {
                    // `camHUD.zoom += hudCameraZoomIntensity *
                    // defaultHUDCameraZoom` (`PlayState.hx` L1858-1859).
                    self.camera_events.hud_zoom +=
                        self.camera_events.bop.hud_intensity * DEFAULT_HUD_CAMERA_ZOOM;
                }
                if let Some(hud) = &mut self.hud {
                    hud.state.on_step(step, step_length_ms, self.sim_ms);
                }
                for character in &mut self.characters {
                    character.scheduler.on_step(step);
                }
                for prop in &mut self.props {
                    if let Some(scheduler) = &mut prop.scheduler {
                        scheduler.on_step_forced(step);
                    }
                }
            }
        }

        // Decay/zoom-assign block (`PlayState.hx` L1213-1228): gated by
        // `cameraZoomRate > 0.0`; when the rate is zero neither the camera
        // bop nor the virtual HUD zoom decays and the gate never pulses.
        if self.camera_events.bop.rate > 0.0 {
            self.camera.decay(TICK_SECONDS);
            self.camera_events.decay_hud(TICK_SECONDS);
        }

        for input in inputs {
            match input {
                InputEvent::Press {
                    direction, sim_ms, ..
                } => {
                    if let Ok(direction) = usize::try_from(*direction)
                        && direction < 4
                    {
                        self.player_line.press(direction, *sim_ms as f64);
                    }
                }
                InputEvent::Release { direction, .. } => {
                    if let Ok(direction) = usize::try_from(*direction) {
                        self.player_line.release(direction);
                    }
                }
                // M6-CONTRACT §14: the advance key is consumed after the
                // event rows, one coalesced CUTSCENE_ADVANCE per tick.
                InputEvent::Advance { .. } => {}
            }
        }

        self.process_events(events)?;

        // Native order: processSongEvents -> processInputQueue -> cutscene
        // keys. A same-tick `dialogue:` trigger starts first; two advance
        // keys in one frame are one native boolean check, not two advances.
        if inputs
            .iter()
            .any(|input| matches!(input, InputEvent::Advance { .. }))
            && let Some(conversation) = &mut self.conversation
        {
            conversation.advance();
        }

        // The conversation's per-tick update runs after the event rows and
        // the cutscene advance (a same-tick trigger is already active) and
        // before the draw path re-reads `layer_specs`. Completed
        // conversations write zero alpha and no text pages, so their group
        // stops drawing.
        if let Some(conversation) = &mut self.conversation {
            conversation.begin_tick(self.sim_ms);
            conversation.sync_layer_specs(&mut self.layer_specs)?;
        }

        // Weekend-1 choreography per-tick advance (M6-CONTRACT §14.2): the
        // cans/explosions advance on the 1 ms grid and the native 1/24 s
        // darken timer starts the tint pass when due.
        if let Some(runtime) = &mut self.weekend1
            && runtime.advance(self.sim_ms)
        {
            self.start_prop_darken();
        }
        self.advance_prop_tint();

        // Darnell can-cutscene per-tick advance (M6-CONTRACT §14.2): the
        // step table's camera/character/can actions apply in order; the
        // overlay alpha syncs into the cutscene layer color.
        let focus = self.cutscene_focus();
        let dance = self.cutscene_dance_state();
        let actions = self
            .cutscene
            .as_mut()
            .map(|cutscene| cutscene.advance(self.sim_ms, &focus, &dance));
        if let Some(actions) = actions {
            for action in actions {
                self.apply_cutscene_action(action)?;
            }
        }
        self.sync_cutscene_overlay();

        self.trigger_opponent_notes();
        // Strumline.hx:653-669: an opponent hold that reached its end no
        // longer contributes confirm-hold; prune before rendering so the
        // opponent receptor returns to static.
        prune_opponent_holds(&mut self.opponent_holds, self.sim_ms);

        let holding = self.player_holds.iter().any(Option::is_some);
        if let Some(player) = self.character_mut(Role::Player) {
            player.scheduler.set_holding(holding);
        }
        // Native `PlayState.processNotes` (PlayState.hx L2838-2852): while
        // any opponent hold note is active, every frame resets
        // `getDad().holdTimer = 0` so the sing animation survives long
        // sustains (playtest finding on the bopeebo easy 52800/1875 ms hold).
        let opponent_holding = self.opponent_holds.iter().any(Option::is_some);
        if opponent_holding && let Some(dad) = self.character_mut(Role::Opponent) {
            dad.scheduler.reset_hold_timer_under_opponent_hold();
        }
        for character in &mut self.characters {
            character.scheduler.update(TICK_SECONDS, step_length_ms);
        }
        for prop in &mut self.props {
            if let Some(scheduler) = &mut prop.scheduler {
                scheduler.update(TICK_SECONDS, step_length_ms);
            }
        }
        // The follow lerp runs only while the native camera target is set; a
        // follow tween disables it (`camera.target = null`,
        // `tweenCameraToFollowPoint`).
        if self.camera_events.follow_enabled {
            self.camera
                .follow(self.camera_events.follow_focus, TICK_SECONDS);
        }
        Ok(())
    }

    /// Applies one tick's chart-event rows in dispatch order. Consecutive
    /// `scroll_speed` rows belong to one native `tweenScrollSpeed` call (a
    /// 'both' event emits player then opponent), so they are batched.
    fn process_events(&mut self, events: &[TraceEvent]) -> Result<(), RenderError> {
        let mut index = 0;
        while index < events.len() {
            if matches!(events[index], TraceEvent::ScrollSpeed { .. }) {
                let mut rows = Vec::new();
                while let Some(event) = events.get(index) {
                    if !matches!(event, TraceEvent::ScrollSpeed { .. }) {
                        break;
                    }
                    rows.push(ScrollRow::from_event(event)?);
                    index += 1;
                }
                self.apply_scroll_rows(&rows)?;
            } else {
                self.process_event(&events[index])?;
                index += 1;
            }
        }
        Ok(())
    }

    /// `FocusCameraSongEvent.hx` L65-158 with the M6-CONTRACT §12.6 finding-1
    /// rule: the target is resolved from the scene's OWN rigs, never from the
    /// row's `world_focus`; a bf/dad/gf row whose character is absent applies
    /// nothing (native null-character early return).
    fn apply_camera_focus(
        &mut self,
        target_kind: &str,
        x: f64,
        y: f64,
        duration: f64,
        ease_kind: &str,
        ease: Option<&str>,
    ) -> Result<(), RenderError> {
        let focus = match target_kind {
            "bf" => self.character(Role::Player).map(|rig| rig.camera_focus),
            "dad" => self.character(Role::Opponent).map(|rig| rig.camera_focus),
            "gf" => self.character(Role::Girlfriend).map(|rig| rig.camera_focus),
            // Unknown char ids fall through to the literal position in the
            // native switch (the observed quirk).
            "position" | "unknown" => Some([0.0, 0.0]),
            other => {
                return Err(RenderError::Unsupported {
                    detail: format!("camera_focus row has unknown target_kind {other:?}"),
                });
            }
        };
        let Some(focus) = focus else {
            return Ok(());
        };
        let target = [x + focus[0], y + focus[1]];
        if !target[0].is_finite() || !target[1].is_finite() {
            return Err(RenderError::Unsupported {
                detail: format!("camera_focus target {target:?} is not finite"),
            });
        }

        match ease_kind {
            "classic" => {
                // resetCamera(false, false, false) + cancelCameraFollowTween +
                // followPoint.setPosition(target): re-follow, retarget, no snap.
                self.camera_events.follow_tween = None;
                self.camera_events.follow_enabled = true;
                self.camera_events.follow_focus = target;
            }
            "instant" => {
                // tweenCameraToPosition(target, 0) -> resetCamera(false,false):
                // cancel tween, re-follow and snap.
                self.camera_events.follow_tween = None;
                self.camera_events.follow_enabled = true;
                self.camera_events.follow_focus = target;
                self.camera.focus_on(target);
            }
            "named" => {
                let Some(name) = ease else {
                    // Invalid ease: native early return, no state change.
                    return Ok(());
                };
                let Some((_, function)) = resolve_ease(name) else {
                    return Ok(());
                };
                let duration_ms = self.conductor.step_length_ms() * duration;
                self.start_follow_tween(target, duration_ms, function)?;
            }
            other => {
                return Err(RenderError::Unsupported {
                    detail: format!("camera_focus row has unknown ease_kind {other:?}"),
                });
            }
        }
        Ok(())
    }

    fn start_follow_tween(
        &mut self,
        target: Vec2,
        duration_ms: f64,
        ease: EaseFn,
    ) -> Result<(), RenderError> {
        if !duration_ms.is_finite() || duration_ms < 0.0 {
            return Err(RenderError::Unsupported {
                detail: format!("camera_focus duration {duration_ms} ms is invalid"),
            });
        }
        self.camera_events.follow_focus = target;
        self.camera_events.follow_tween = None;
        if duration_ms <= 0.0 {
            // `tweenCameraToFollowPoint(0)` -> resetCamera(false, false).
            self.camera_events.follow_enabled = true;
            self.camera.focus_on(target);
        } else {
            self.camera_events.follow_enabled = false;
            self.camera_events.follow_tween = Some(FollowTween {
                from: self.camera.scroll,
                to: [target[0] - HALF_WIDTH, target[1] - HALF_HEIGHT],
                start_ms: self.sim_ms,
                duration_ms,
                ease,
            });
        }
        Ok(())
    }

    /// `ZoomCameraSongEvent.hx` L37-79 / `tweenCameraZoom`
    /// (`PlayState.hx` L3999-4014).
    fn apply_camera_zoom(
        &mut self,
        target: f64,
        duration: f64,
        ease: Option<&str>,
    ) -> Result<(), RenderError> {
        let Some(name) = ease else {
            // Invalid ease: native early return, no cancel/state change.
            return Ok(());
        };
        if !target.is_finite() || !duration.is_finite() || duration < 0.0 {
            return Err(RenderError::Unsupported {
                detail: format!("camera_zoom target/duration ({target}, {duration}) is invalid"),
            });
        }
        if name == "INSTANT" {
            // tweenCameraZoom(target, 0): cancel the tween, set instantly.
            self.camera_events.zoom_tween = None;
            self.camera.current_zoom = target;
            return Ok(());
        }
        let Some((_, function)) = resolve_ease(name) else {
            return Ok(());
        };
        self.camera_events.zoom_tween = None;
        let duration_ms = self.conductor.step_length_ms() * duration;
        if duration_ms <= 0.0 {
            self.camera.current_zoom = target;
        } else {
            self.camera_events.zoom_tween = Some(ZoomTween {
                from: self.camera.current_zoom,
                to: target,
                start_ms: self.sim_ms,
                duration_ms,
                ease: function,
            });
        }
        Ok(())
    }

    /// `PlayAnimationSongEvent.hx` L25-76: character targets use
    /// `playAnimation(anim, force, force)` (restart = force, ignoreOther =
    /// force); prop targets use the plain `animation.play(anim, force)`.
    fn apply_play_animation(
        &mut self,
        anim: &str,
        force: bool,
        resolved_kind: &str,
        skipped_old: bool,
    ) {
        if skipped_old {
            return;
        }
        match resolved_kind {
            "bf" => {
                if let Some(rig) = self.character_mut(Role::Player) {
                    rig.scheduler.play_with_ignore_other(anim, force, force);
                }
            }
            "dad" => {
                if let Some(rig) = self.character_mut(Role::Opponent) {
                    rig.scheduler.play_with_ignore_other(anim, force, force);
                }
            }
            "gf" => {
                if let Some(rig) = self.character_mut(Role::Girlfriend) {
                    rig.scheduler.play_with_ignore_other(anim, force, force);
                }
            }
            kind => {
                let Some(name) = kind.strip_prefix("prop:") else {
                    // 'none' and any unresolved target apply nothing.
                    return;
                };
                let Some(prop) = self.props.iter_mut().find(|prop| prop.prop.name == name) else {
                    return;
                };
                if let Some(scheduler) = &mut prop.scheduler {
                    // StageProp is not a Bopper: exact name, no suffix
                    // stripping and no animation lock.
                    scheduler.play_exact(anim, force);
                }
            }
        }
    }

    /// Native `tweenScrollSpeed` batches the target strumlines of one event;
    /// the sim emits one row per strumline. A player row immediately followed
    /// by an opponent row is that same native call; any other adjacent pair is
    /// a separate event.
    fn apply_scroll_rows(&mut self, rows: &[ScrollRow]) -> Result<(), RenderError> {
        let mut start = 0;
        while start < rows.len() {
            let end = if start + 1 < rows.len()
                && rows[start].line == LINE_PLAYER
                && rows[start + 1].line == LINE_OPPONENT
            {
                start + 2
            } else {
                start + 1
            };
            self.apply_scroll_event(&rows[start..end])?;
            start = end;
        }
        Ok(())
    }

    fn apply_scroll_event(&mut self, rows: &[ScrollRow]) -> Result<(), RenderError> {
        let resolved: Vec<Option<ScrollEase>> = rows
            .iter()
            .map(|row| resolve_scroll_ease(row.ease.as_deref()))
            .collect();
        // Native `tweenScrollSpeed` cancels + snaps previous targets once for
        // the whole strumline list. An unresolved ease early-returns before
        // that call, so it must not cancel/snap.
        if resolved.iter().any(Option::is_some) {
            self.camera_events.scroll.cancel_and_snap();
        }
        for (row, ease) in rows.iter().zip(resolved) {
            let actual = self.camera_events.scroll.value(row.line);
            if actual != row.base_scroll {
                return Err(RenderError::ScrollBaseMismatch {
                    tick_ms: self.last_tick.unwrap_or(0),
                    strumline: row.strumline,
                    expected: row.base_scroll,
                    actual,
                });
            }
            let Some(ease) = ease else {
                // Unresolved ease: native early return, no state change.
                continue;
            };
            let duration_ms = self.conductor.step_length_ms() * row.duration;
            match ease {
                ScrollEase::Instant => self.camera_events.scroll.set_value(row.line, row.target),
                ScrollEase::Named(function) if duration_ms > 0.0 => self
                    .camera_events
                    .scroll
                    .start_tween(row.line, self.sim_ms, row.target, duration_ms, function),
                ScrollEase::Named(_) => self.camera_events.scroll.set_value(row.line, row.target),
            }
            self.camera_events
                .scroll
                .push_previous(row.line, row.target);
        }
        Ok(())
    }

    fn scroll_speed_for(&self, opponent: bool) -> f64 {
        self.camera_events
            .scroll
            .value(if opponent { LINE_OPPONENT } else { LINE_PLAYER })
    }

    fn process_event(&mut self, event: &TraceEvent) -> Result<(), RenderError> {
        // The HUD mirrors the player health/score rows and consumes
        // SetHealthIcon at the native processSongEvents position.
        if let Some(hud) = &mut self.hud {
            hud.state.observe_event(event, &hud.assets)?;
        }
        // Weekend-1 note-kind choreography (M6-CONTRACT §14.2): the rows
        // carry the chart `k` kinds; lightcan/kneecan/cockgun are timing
        // anchors, kickcan spawns the can, firegun fires the ARcing one.
        if let Some(kind) = event.note_kind()
            && let Some(runtime) = &mut self.weekend1
        {
            match event {
                TraceEvent::Miss { .. } => runtime.on_miss(kind),
                TraceEvent::Hit { .. } | TraceEvent::OpponentHit { .. } => {
                    runtime.on_hit(kind, self.sim_ms)
                }
                _ => {}
            }
        }
        match event {
            TraceEvent::Hit {
                note_id,
                note_direction,
                ..
            } => {
                let index = self.player_note_index(*note_id, self.last_tick.unwrap_or(0))?;
                self.notes[index].hit = true;
                let direction = *note_direction;
                if let Ok(direction) = usize::try_from(direction)
                    && direction < 4
                {
                    if let Some(player) = self.character_mut(Role::Player) {
                        player.scheduler.on_sing(direction, false);
                    }
                    // A release later in the same tick must win over the hit
                    // confirm (native release plays static).
                    if self.player_line.pressed[direction] {
                        self.player_line.confirm_player(direction, self.sim_ms);
                    }
                }
            }
            TraceEvent::Miss {
                note_id,
                note_direction,
                ..
            } => {
                let index = self.player_note_index(*note_id, self.last_tick.unwrap_or(0))?;
                self.notes[index].missed = true;
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && let Some(player) = self.character_mut(Role::Player)
                {
                    player.scheduler.on_sing(direction, true);
                }
            }
            TraceEvent::GhostPress { note_direction, .. } => {
                // Engine decision: the ghost-score path plays the miss form,
                // mirroring the native miss-note reaction.
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && let Some(player) = self.character_mut(Role::Player)
                {
                    player.scheduler.on_sing(direction, true);
                }
            }
            TraceEvent::HoldStart {
                note_id,
                note_direction,
                ..
            } => {
                let index = self.player_note_index(*note_id, self.last_tick.unwrap_or(0))?;
                let note = self.notes[index].note.clone();
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && note.sustain_ms > 0.0
                {
                    self.player_holds[direction] = Some(PlayerHold {
                        note_id: *note_id,
                        remaining_ms: note.sustain_ms,
                    });
                    if self.player_line.pressed[direction] {
                        self.player_line.confirm_player(direction, self.sim_ms);
                    }
                }
            }
            TraceEvent::HoldTick {
                note_id,
                note_direction,
                hold_remaining_ms,
                ..
            } => {
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && let Some(hold) = &mut self.player_holds[direction]
                    && hold.note_id == *note_id
                {
                    hold.remaining_ms = hold_remaining_ms.max(0.0);
                }
            }
            TraceEvent::HoldDrop {
                note_id,
                note_direction,
                ..
            } => {
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && self.player_holds[direction]
                        .as_ref()
                        .is_some_and(|hold| hold.note_id == *note_id)
                {
                    self.player_holds[direction] = None;
                    // Strumline.hx:634-640: a dropped hold note is completely
                    // hidden. The observed yOffset = (fullFullLen - currentLen)
                    // * 0.45 only positions that hidden trail; no dropped
                    // trail is drawn in frames.
                }
            }
            TraceEvent::HoldEnd {
                note_id,
                note_direction,
                ..
            } => {
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && self.player_holds[direction]
                        .as_ref()
                        .is_some_and(|hold| hold.note_id == *note_id)
                {
                    self.player_holds[direction] = None;
                }
            }
            TraceEvent::GameOver { .. } => {
                // bf death/fakeout animations are outside phase 2b. Freeze the
                // scene at the substate boundary; the sim stops dispatching.
                self.dead = true;
            }
            TraceEvent::CameraFocus {
                target_kind,
                x,
                y,
                duration,
                ease_kind,
                ease,
                ..
            } => {
                self.apply_camera_focus(
                    target_kind,
                    *x,
                    *y,
                    *duration,
                    ease_kind,
                    ease.as_deref(),
                )?;
            }
            TraceEvent::CameraZoom {
                resolved_target_zoom,
                duration,
                ease,
                ..
            } => {
                self.apply_camera_zoom(*resolved_target_zoom, *duration, ease.as_deref())?;
            }
            TraceEvent::CameraBop {
                camera_bop_intensity,
                hud_intensity,
                rate,
                offset,
                ..
            } => {
                // SetCameraBopSongEvent.hx L40-53 replaces the mutable
                // config; the HUD bump itself happens at the next step gate.
                self.camera_events.bop = BopConfig {
                    intensity: *camera_bop_intensity,
                    hud_intensity: *hud_intensity,
                    rate: *rate,
                    offset: *offset,
                };
            }
            TraceEvent::PlayAnimation {
                anim,
                force,
                resolved_kind,
                skipped_old,
                ..
            } => {
                self.apply_play_animation(anim, *force, resolved_kind, *skipped_old);
            }
            // Batched by `process_events` (one native tweenScrollSpeed call
            // can cover two strumlines).
            TraceEvent::ScrollSpeed { .. } => {}
            // Consumed by the shared HUD observer above (M6-CONTRACT §13).
            TraceEvent::HealthIcon { .. } => {}
            // EnableMask / sserafim / any no-handler kind: record-only
            // (M6-CONTRACT §12.2).
            TraceEvent::UnknownEvent { .. } => {}
            // The 6E conversation slice consumes `dialogue:` trigger rows.
            // Unknown conversation ids and triggers without a loaded payload
            // fail closed instead of silently showing the wrong content.
            TraceEvent::Choreography { event, .. } => {
                if let Some(id) = event.strip_prefix("dialogue:") {
                    let Some(conversation) = &mut self.conversation else {
                        return Err(RenderError::Unsupported {
                            detail: format!(
                                "dialogue trigger {event:?} fired but the scene loaded no \
                                 conversation payload"
                            ),
                        });
                    };
                    conversation.start(id)?;
                } else if event == "can_cutscene" {
                    let Some(cutscene) = &mut self.cutscene else {
                        return Err(RenderError::Unsupported {
                            detail: "can_cutscene trigger fired but the scene loaded no \
                                     cutscene payload"
                                .to_string(),
                        });
                    };
                    cutscene.start(self.sim_ms);
                } else if event == "ending_video" {
                    // The video player is the media phase deliverable ([D]
                    // M6-CONTRACT §14.2); the story-retry path runs a normal
                    // song end.
                }
            }
            TraceEvent::OpponentHit {
                note_id,
                note_direction,
                ..
            } => {
                // Schema-2 sim rows REPLACE the renderer's internal opponent
                // approximation (docs/M6-CONTRACT.md §10.3.3): the native
                // auto-hit decision comes from the sim stream, not from a
                // renderer-side `note.time > now` pass.
                let index = self.opponent_note_index(*note_id, self.last_tick.unwrap_or(0))?;
                if self.notes[index].hit {
                    return Err(RenderError::DuplicateOpponentNote {
                        tick_ms: self.last_tick.unwrap_or(0),
                        note_id: *note_id,
                    });
                }
                let note = self.notes[index].note.clone();
                self.notes[index].hit = true;
                let direction = *note_direction;
                if let Ok(direction) = usize::try_from(direction)
                    && direction < 4
                {
                    if let Some(opponent) = self.character_mut(Role::Opponent) {
                        opponent.scheduler.on_sing(direction, false);
                    }
                    self.opponent_line.confirm_opponent(direction, self.sim_ms);
                    if note.sustain_ms > 0.0 {
                        self.opponent_holds[direction] = Some(OpponentHold {
                            end_ms: note.time + note.sustain_ms,
                        });
                    }
                }
            }
        }
        Ok(())
    }

    /// Legacy bridge for schema-1 trace feeds (the M5 play mode and render
    /// fixtures, which carry no `OpponentHit` rows). Schema-2 streams consume
    /// the same decisions from the sim's rows in `process_event`; the
    /// `notes[index].hit` guard makes this pass a no-op for any note a row
    /// already covered. Removal waits for the play mode/render generation
    /// to flip to `build_chart` (6B/6C).
    fn trigger_opponent_notes(&mut self) {
        let now = self.sim_ms;
        for index in 0..self.notes.len() {
            let note = self.notes[index].note.clone();
            if !note.opponent || self.notes[index].hit || note.time > now {
                continue;
            }
            self.notes[index].hit = true;
            let direction = note.direction;
            if let Some(opponent) = self.character_mut(Role::Opponent) {
                opponent.scheduler.on_sing(direction, false);
            }
            self.opponent_line.confirm_opponent(direction, now);
            if note.sustain_ms > 0.0 {
                self.opponent_holds[direction] = Some(OpponentHold {
                    end_ms: note.time + note.sustain_ms,
                });
            }
        }
    }

    fn player_note_index(&self, note_id: u32, tick_ms: u64) -> Result<usize, RenderError> {
        self.note_index(&self.player_note_indices, note_id, tick_ms)
            .ok_or(RenderError::UnknownEventNote { tick_ms, note_id })
    }

    fn opponent_note_index(&self, note_id: u32, tick_ms: u64) -> Result<usize, RenderError> {
        self.note_index(&self.opponent_note_indices, note_id, tick_ms)
            .ok_or(RenderError::UnknownOpponentNote { tick_ms, note_id })
    }

    fn note_index(&self, indices: &[usize], note_id: u32, _tick_ms: u64) -> Option<usize> {
        indices
            .get(usize::try_from(note_id).ok()?)
            .copied()
            .filter(|index| *index != usize::MAX)
    }

    /// Loaded characters keep their role; the girlfriend role is absent when
    /// the metadata selects none.
    fn character(&self, role: Role) -> Option<&CharacterRig> {
        self.characters
            .iter()
            .find(|character| character.role == role)
    }

    fn character_mut(&mut self, role: Role) -> Option<&mut CharacterRig> {
        self.characters
            .iter_mut()
            .find(|character| character.role == role)
    }

    /// Composes all draw layers in painter order. Stage items may own several
    /// page layers (mixed multi-sheet characters); each quad is routed to the
    /// layer registered for its page.
    pub(crate) fn draw(&self) -> Result<Vec<Vec<Quad>>, RenderError> {
        let mut layers: Vec<Vec<Quad>> = vec![Vec::new(); self.layer_specs.len()];
        for item in &self.stage_items {
            let quads = match item.kind {
                StageItemKind::Prop(index) => Scene::draw_prop(&self.camera, &self.props[index])?,
                StageItemKind::Character(index) => self.characters[index].draw(&self.camera)?,
                StageItemKind::Weekend1Can => match &self.weekend1 {
                    Some(runtime) => runtime.draw_can(&self.camera)?,
                    None => Vec::new(),
                },
                StageItemKind::Weekend1CanShotProp => match &self.weekend1 {
                    Some(runtime) => runtime.draw_can_shot_prop(&self.camera)?,
                    None => Vec::new(),
                },
                StageItemKind::Weekend1BigExplosion => match &self.weekend1 {
                    Some(runtime) => runtime.draw_big_explosion(&self.camera)?,
                    None => Vec::new(),
                },
                StageItemKind::Weekend1EzExplosion => match &self.weekend1 {
                    Some(runtime) => runtime.draw_ez_explosion(&self.camera)?,
                    None => Vec::new(),
                },
            };
            for (page, quad) in quads {
                let Some((_, layer)) = item.layers.iter().find(|(item_page, _)| *item_page == page)
                else {
                    return Err(RenderError::Renderer {
                        detail: format!("stage item drew page {page:?} without a registered layer"),
                    });
                };
                layers[*layer].push(quad);
            }
        }
        layers[self.overlay_start] = self.draw_backgrounds();
        layers[self.overlay_start + 1] = self.draw_receptors();
        layers[self.overlay_start + 2] = self.draw_holds();
        layers[self.overlay_start + 3] = self.draw_notes();
        if let Some(hud) = &self.hud {
            let hud_quads =
                hud.state
                    .compose(&hud.assets, &hud.layers, self.camera_events.hud_zoom);
            for (index, quads) in hud_quads.into_iter().enumerate() {
                layers[self.hud_start + index] = quads;
            }
        }
        if let Some(conversation) = &self.conversation {
            let conversation_quads = conversation.draw(SCREEN_PROJECTION)?;
            for (index, quads) in conversation_quads.into_iter().enumerate() {
                layers[self.conversation_start + index] = quads;
            }
        }
        Ok(layers)
    }

    fn draw_prop(
        camera: &Camera,
        rig: &PropRig,
    ) -> Result<Vec<(Option<usize>, Quad)>, RenderError> {
        let prop = &rig.prop;
        if let PropVisual::Atlas {
            apply_stage_matrix: true,
            ..
        } = &prop.visual
        {
            // M6-CONTRACT §11.6: the native symbol-instance matrix semantics
            // are not frozen yet. Load for inventory, skip the draw, and name
            // the prop in a structured deviation line (never draw invented
            // geometry, never skip silently).
            rig.stage_matrix_log.call_once(|| {
                eprintln!(
                    "oxide render: deviation kind=apply_stage_matrix prop={:?} \
                     action=skip_draw detail=\"atlasSettings.applyStageMatrix=true is a \
                     6C follow-up (M6-CONTRACT 11.6)\"",
                    prop.name
                );
            });
            return Ok(Vec::new());
        }
        let projection = camera_projection_matrix(camera, [prop.scroll.0, prop.scroll.1]);
        let position = [prop.position.0, prop.position.1];
        let scale = [prop.scale.0, prop.scale.1];
        let (sheet_index, frame_value, animation) = match &rig.scheduler {
            Some(scheduler) => match scheduler.current_visual_frame() {
                Some((animation, frame)) => (animation.sheet, frame, Some(animation)),
                None => (0, 0, None),
            },
            None => (0, 0, None),
        };
        let animation_offsets = animation.and_then(|animation| animation.offsets);
        let shift = animation_offsets.map_or([0.0, 0.0], |offsets| {
            [-offsets.0 * scale[0], -offsets.1 * scale[1]]
        });
        match &prop.visual {
            PropVisual::Solid { size, .. } => {
                // `makeSolidColor` consumed `scale` at load time as the
                // INTEGER graphic dimensions; Stage.hx skips `scale.set` for
                // solids, so the draw applies a unit transform. `prop.scale`
                // is NOT used here (M6-CONTRACT §11.6).
                let (width, height) = (size[0] as f64, size[1] as f64);
                let center = [position[0] + width / 2.0, position[1] + height / 2.0];
                let matrix = rotate_about(
                    [1.0, 0.0, 0.0, 1.0, position[0], position[1]],
                    center,
                    prop.angle,
                );
                Ok(vec![(
                    None,
                    quad_from_matrix(
                        concat(projection, matrix),
                        width,
                        height,
                        [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
                    ),
                )])
            }
            PropVisual::Static { page, image_size } => {
                let hitbox = [image_size[0].round() as i32, image_size[1].round() as i32];
                let frame = SpriteFrame {
                    page: *page,
                    rect: [0, 0, hitbox[0], hitbox[1]],
                    dest_offset: [0, 0],
                    dest_size: hitbox,
                    rotated: false,
                };
                let (matrix, uv) = sprite_frame_matrix(
                    position,
                    &frame,
                    [scale[0], scale[1]],
                    hitbox,
                    prop.flip_x,
                    prop.flip_y,
                    prop.angle,
                    *image_size,
                );
                Ok(vec![(
                    Some(*page),
                    quad_from_matrix(
                        concat(projection, matrix),
                        frame.rect[2] as f64,
                        frame.rect[3] as f64,
                        uv,
                    ),
                )])
            }
            PropVisual::Sprite { sheet, hitbox_dest } => {
                let frame = sheet
                    .frame(frame_value)
                    .ok_or_else(|| RenderError::MissingFrame {
                        path: format!("stage prop {:?}", prop.name),
                        prefix: format!("frame {frame_value}"),
                    })?;
                let flip_x = prop.flip_x ^ animation.is_some_and(|animation| animation.flip_x);
                let flip_y = prop.flip_y ^ animation.is_some_and(|animation| animation.flip_y);
                let (matrix, uv) = sprite_frame_matrix(
                    [position[0] + shift[0], position[1] + shift[1]],
                    &frame,
                    [scale[0], scale[1]],
                    *hitbox_dest,
                    flip_x,
                    flip_y,
                    prop.angle,
                    sheet.page_size,
                );
                Ok(vec![(
                    Some(sheet.page),
                    quad_from_matrix(
                        concat(projection, matrix),
                        frame.rect[2] as f64,
                        frame.rect[3] as f64,
                        uv,
                    ),
                )])
            }
            PropVisual::Atlas { sheet, .. } => {
                let quads = match animation.and_then(|animation| animation.symbol.as_deref()) {
                    Some(symbol) => sheet
                        .stage
                        .resolve_symbol_frame(symbol, frame_value)
                        .map_err(|source| RenderError::AtlasStage {
                            path: format!("stage prop {:?} ({sheet_index})", prop.name),
                            source,
                        })?,
                    None => sheet
                        .stage
                        .resolve_main_frame(frame_value)
                        .map_err(|source| RenderError::AtlasStage {
                            path: format!("stage prop {:?} ({sheet_index})", prop.name),
                            source,
                        })?,
                };
                let bounds = sheet.bounds;
                let world = prop_world_matrix(
                    bounds,
                    [scale[0], scale[1]],
                    prop.flip_x,
                    prop.flip_y,
                    position,
                    shift,
                );
                let center = [
                    position[0] + shift[0] + bounds[2] * scale[0] / 2.0,
                    position[1] + shift[1] + bounds[3] * scale[1] / 2.0,
                ];
                let world = rotate_about(world, center, prop.angle);
                let mut out = Vec::with_capacity(quads.len());
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
                Ok(out)
            }
        }
    }

    fn draw_backgrounds(&self) -> Vec<Quad> {
        background_quads(BACKGROUND_ALPHA)
    }

    fn draw_receptors(&self) -> Vec<Quad> {
        let mut quads = Vec::with_capacity(8);
        let scale = self.note_style.receptor_scale;
        for (line_x, line, holds, is_player) in [
            (
                OPPONENT_STRUMLINE_X,
                &self.opponent_line,
                hold_flags(&self.opponent_holds),
                false,
            ),
            (
                PLAYER_STRUMLINE_X,
                &self.player_line,
                hold_flags(&self.player_holds),
                true,
            ),
        ] {
            for (direction, held) in holds.iter().enumerate() {
                let frames = &self.note_style.receptors[direction];
                // Player release clears the hold-confirm immediately (the
                // input release happens before a later-tick HoldDrop event).
                let state = receptor_state(
                    line,
                    direction,
                    *held,
                    is_player,
                    self.sim_ms,
                    frames.confirm_frames.len(),
                );
                let (selected, started_ms, looped) = match state {
                    ReceptorState::ConfirmHold => (
                        &frames.confirm_hold_frames,
                        line.confirm_started_ms[direction],
                        false,
                    ),
                    ReceptorState::Confirm => (
                        &frames.confirm_frames,
                        line.confirm_started_ms[direction],
                        false,
                    ),
                    ReceptorState::Press => (
                        &frames.press_frames,
                        line.press_started_ms[direction],
                        false,
                    ),
                    ReceptorState::Static => (&frames.static_frames, 0.0, true),
                };
                let frame = receptor_frame(selected, self.sim_ms - started_ms, looped);
                let base = receptor_top_left(line_x, direction, self.note_style.receptor_offsets.1);
                let base = [base[0] + self.note_style.receptor_offsets.0, base[1]];
                let content_top_left = native_content_top_left(
                    base,
                    frame,
                    scale,
                    self.note_style.receptor_hitbox_dest,
                );
                quads.push(sprite_frame_quad(
                    frame,
                    content_top_left,
                    scale,
                    self.note_style.receptor_page_size,
                ));
            }
        }
        quads
    }

    fn draw_holds(&self) -> Vec<Quad> {
        let mut quads = Vec::new();
        let player_speed = self.scroll_speed_for(false);
        let opponent_speed = self.scroll_speed_for(true);
        let held_top = sustain_held_top(STRUMLINE_Y);
        // Player: held trails anchor at the fixed line and are consumed from
        // the top; remaining <= 10 ms hides the trail (Strumline.hx:729-733).
        for direction in 0..4 {
            if let Some(hold) = &self.player_holds[direction]
                && hold.remaining_ms > SUSTAIN_HELD_HIDE_REMAINING
            {
                self.push_hold(
                    &mut quads,
                    PLAYER_STRUMLINE_X,
                    direction,
                    held_top,
                    sustain_height(hold.remaining_ms, player_speed),
                );
            }
            if let Some(hold) = &self.opponent_holds[direction] {
                let remaining = (hold.end_ms - self.sim_ms).max(0.0);
                if remaining <= SUSTAIN_HELD_HIDE_REMAINING {
                    continue;
                }
                self.push_hold(
                    &mut quads,
                    OPPONENT_STRUMLINE_X,
                    direction,
                    held_top,
                    sustain_height(remaining, opponent_speed),
                );
            }
        }
        // Approaching (un-hit) trails: the native unhit y is the crossing
        // anchor moved by the note scroll (`Strumline.hx:764-769`: `this.y -
        // INITIAL_OFFSET + getNoteY(...) + STRUMLINE_SIZE / 2`); using the
        // note sprite's bottom instead made the trail ~58 px low and jump
        // into place at the hit. Each line uses its own scrollSpeed value.
        for note in &self.notes {
            let speed = if note.note.opponent {
                opponent_speed
            } else {
                player_speed
            };
            let render_distance = render_distance_ms(speed);
            if note.hit
                || note.note.sustain_ms <= 0.0
                || note.note.time > self.sim_ms + render_distance
            {
                continue;
            }
            let line_x = if note.note.opponent {
                OPPONENT_STRUMLINE_X
            } else {
                PLAYER_STRUMLINE_X
            };
            let anchor = held_top + note_y_offset(self.sim_ms, note.note.time, speed);
            // Pre-hit remaining = full + elapsed; updateClipping clamps it to
            // graphicHeight (SustainTrail.hx:309), so clip == full.
            self.push_hold(
                &mut quads,
                line_x,
                note.note.direction,
                anchor,
                sustain_height(note.note.sustain_ms, speed),
            );
        }
        quads
    }

    fn push_hold(
        &self,
        out: &mut Vec<Quad>,
        line_x: f64,
        direction: usize,
        anchor_y: f64,
        clip_height: f64,
    ) {
        // Native `updateClipping` clamp + `<= 0.1` hide rule
        // (SustainTrail.hx:309-318). NOTE: graphicHeight is RECOMPUTED from
        // the REMAINING length on every frame (`set_sustainLength` ->
        // triggerRedraw, SustainTrail.hx:273-289; Strumline.hx:727 assigns
        // the remaining sustain each frame), so `graphicHeight == clipHeight`
        // and `graphicHeight - clipHeight == 0`: the trail is consumed FROM
        // THE TIP while its body stays pinned at the receptor-anchored top
        // (the human window playtest falsified the fixed `full` reading).
        let clip_height = clip_height.clamp(0.0, f64::INFINITY);
        if clip_height <= SUSTAIN_HIDE_CLIP {
            return;
        }
        let (body_column, cap_column) = hold_columns(direction);
        // Observed `graphicWidth = (graphic.width / 8) * zoom =
        // (416/8) * 0.7 = 36.4` Dx for funkin; the pixel style uses its own
        // 72-px sheet and 6.0 scale through the same frozen formula.
        let width = self.note_style.hold_width;
        let x = sustain_top_left_x(line_x, direction, width) + self.note_style.hold_offsets.0;
        let anchor_y = anchor_y + self.note_style.hold_offsets.1;
        let hold_page_width = self.note_style.hold_page_size[0];
        let strip = self.note_style.hold_strip_height();
        let bottom_height = self.note_style.sustain_bottom_height();
        let extra_cap_tail = self.note_style.sustain_extra_cap_tail();
        let part = clip_height - bottom_height;
        if part > 0.0 {
            // Body spans [anchor, anchor + part]; sampled with the strip's
            // own style proportion (the sanctioned interior-row choice of
            // the native negative-v range).
            let (uv_min, uv_max) = hold_column_uv_for(body_column, hold_page_width);
            let v_span = uv_max[1] - uv_min[1];
            let v_top = uv_max[1] - v_span * (part / strip);
            let uv = [
                [uv_min[0], v_top],
                [uv_max[0], v_top],
                [uv_max[0], uv_max[1]],
                [uv_min[0], uv_max[1]],
            ];
            out.push(quad_from_matrix(
                [1.0, 0.0, 0.0, 1.0, x, anchor_y],
                width,
                part,
                uv,
            ));
        }
        // End cap: top = anchor + max(part, 0); bottom = anchor + clip +
        // extraCapTail — the tip RISES toward the anchor as `clip` shrinks.
        let cap_top = anchor_y + part.max(0.0);
        let cap_bottom = anchor_y + clip_height + extra_cap_tail;
        let cap_height = cap_bottom - cap_top;
        if cap_height <= 0.0 {
            return;
        }
        let (uv_min, uv_max) = hold_column_uv_for(cap_column, hold_page_width);
        let v_span = uv_max[1] - uv_min[1];
        let cap_v0 = if part > 0.0 {
            uv_min[1]
        } else {
            uv_min[1] + v_span * ((bottom_height - clip_height) / strip)
        };
        let cap_v1 = uv_min[1] + v_span * SUSTAIN_CAP_BOTTOM_CLIP;
        let uv = [
            [uv_min[0], cap_v0],
            [uv_max[0], cap_v0],
            [uv_max[0], cap_v1],
            [uv_min[0], cap_v1],
        ];
        out.push(quad_from_matrix(
            [1.0, 0.0, 0.0, 1.0, x, cap_top],
            width,
            cap_height,
            uv,
        ));
    }

    fn draw_notes(&self) -> Vec<Quad> {
        let mut quads = Vec::new();
        let scale = self.note_style.note_scale;
        for note in &self.notes {
            let speed = self.scroll_speed_for(note.note.opponent);
            let render_distance = render_distance_ms(speed);
            if note.hit || note.note.time > self.sim_ms + render_distance {
                continue;
            }
            let line_x = if note.note.opponent {
                OPPONENT_STRUMLINE_X
            } else {
                PLAYER_STRUMLINE_X
            };
            let frame = self.note_style.note_frames[note.note.direction];
            let width = frame.dest_size[0] as f64 * scale;
            let height = frame.dest_size[1] as f64 * scale;
            let top = note_top_left(
                line_x,
                note.note.direction,
                width,
                self.sim_ms,
                note.note.time,
                speed,
            );
            if !visible(top[1], height) {
                continue;
            }
            // Notes follow the same content formula.
            let content_top_left =
                native_content_top_left(top, &frame, scale, self.note_style.note_hitbox_dest);
            quads.push(sprite_frame_quad(
                &frame,
                content_top_left,
                scale,
                self.note_style.note_page_size,
            ));
        }
        quads
    }
}

/// Strumline underlay quads. The observed default opacity is 0
/// (`Preferences.strumlineBackgroundOpacity ?? 0`, Preferences.hx:462-464;
/// `background.alpha = opacity / 100`, Strumline.hx:249/300), so no quads are
/// emitted at alpha 0 and the layer disappears from the output. The geometry
/// functions stay available for the documented placement.
fn background_quads(alpha: f64) -> Vec<Quad> {
    if alpha <= 0.0 {
        return Vec::new();
    }
    let mut quads = Vec::with_capacity(2);
    for line_x in [OPPONENT_STRUMLINE_X, PLAYER_STRUMLINE_X] {
        let top_left = geometry::background_top_left(line_x);
        let matrix = [1.0, 0.0, 0.0, 1.0, top_left[0], top_left[1]];
        quads.push(quad_from_matrix(
            matrix,
            BACKGROUND_WIDTH,
            GAME_HEIGHT,
            [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]],
        ));
    }
    quads
}

/// World transform for one character frame.
///
/// Frozen composition (RENDER-BEHAVIOR §4):
/// `gameQuad(p) = spriteTopLeft + (p - boundsTopLeft)` at scale 1, with the
/// sprite scale applied to the whole bounds-relative vector and the
/// data-driven whole-sprite mirror (`Stage.hx:425`) applied inside the
/// bounds. The Bopper animation-offset shift is already folded into
/// `sprite_top_left` by the caller: `-(animOffsets - globalOffsets) * scale`.
fn character_world_matrix(
    bounds: [f64; 4],
    scale: f64,
    flip_x: bool,
    sprite_top_left: Vec2,
    shift: Vec2,
) -> [f64; 6] {
    let [bx, by, bw, _] = bounds;
    if flip_x {
        [
            -scale,
            0.0,
            0.0,
            scale,
            sprite_top_left[0] + shift[0] + scale * (bx + bw),
            sprite_top_left[1] + shift[1] - scale * by,
        ]
    } else {
        [
            scale,
            0.0,
            0.0,
            scale,
            sprite_top_left[0] + shift[0] - scale * bx,
            sprite_top_left[1] + shift[1] - scale * by,
        ]
    }
}

/// Stage-prop variant of [`character_world_matrix`]: independent x/y scale,
/// optional `flipY`, and the sprite top-left (not a feet anchor). Props use
/// `x/y = position` directly (Stage.hx L262-282).
pub(crate) fn prop_world_matrix(
    bounds: [f64; 4],
    scale: Vec2,
    flip_x: bool,
    flip_y: bool,
    sprite_top_left: Vec2,
    shift: Vec2,
) -> [f64; 6] {
    let [bx, by, bw, bh] = bounds;
    let a = if flip_x { -scale[0] } else { scale[0] };
    let d = if flip_y { -scale[1] } else { scale[1] };
    let tx = if flip_x {
        sprite_top_left[0] + shift[0] + scale[0] * (bx + bw)
    } else {
        sprite_top_left[0] + shift[0] - scale[0] * bx
    };
    let ty = if flip_y {
        sprite_top_left[1] + shift[1] + scale[1] * (by + bh)
    } else {
        sprite_top_left[1] + shift[1] - scale[1] * by
    };
    [a, 0.0, 0.0, d, tx, ty]
}

/// Rotates `matrix` around `center` by `degrees` (Flixel y-down convention).
/// `0.0` returns `matrix` untouched so the frozen angle-free bopeebo frames
/// keep bit-identical matrices.
fn rotate_about(matrix: [f64; 6], center: Vec2, degrees: f64) -> [f64; 6] {
    if degrees == 0.0 {
        return matrix;
    }
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let rotation = [cos, sin, -sin, cos, 0.0, 0.0];
    let to_center = [1.0, 0.0, 0.0, 1.0, center[0], center[1]];
    let from_center = [1.0, 0.0, 0.0, 1.0, -center[0], -center[1]];
    concat(to_center, concat(rotation, concat(from_center, matrix)))
}

/// Flixel ANGLE_NEG_90 tile chain for a `rotated` Sparrow page entry:
/// `(x, y) -> (y, width - x)` (FlxFrame.hx:196-199, FlxMatrix.hx:60-65).
fn rotated_tile_matrix(width: i32) -> [f64; 6] {
    [0.0, -1.0, 1.0, 0.0, 0.0, width as f64]
}

/// Position of a sprite-sheet frame's content inside the sprite box, with the
/// frozen setup-hitbox term:
/// `spriteTopLeft + scale * ((hitbox - dest_size) * 0.5 + dest_offset)`.
/// A whole-sprite flip mirrors the content rect inside the hitbox and swaps
/// the frame UVs in [`sprite_frame_matrix`].
fn sprite_content_top_left(
    sprite_top_left: Vec2,
    frame: &SpriteFrame,
    scale: Vec2,
    hitbox: [i32; 2],
    flip_x: bool,
    flip_y: bool,
) -> Vec2 {
    let term = |hitbox: i32, dest: i32, offset: i32| -> f64 {
        0.5 * (hitbox as f64 - dest as f64) + offset as f64
    };
    let unflipped_x = term(hitbox[0], frame.dest_size[0], frame.dest_offset[0]);
    let unflipped_y = term(hitbox[1], frame.dest_size[1], frame.dest_offset[1]);
    let mirrored_x = term(hitbox[0], frame.dest_size[0], -frame.dest_offset[0]);
    let mirrored_y = term(hitbox[1], frame.dest_size[1], -frame.dest_offset[1]);
    [
        sprite_top_left[0] + scale[0] * if flip_x { mirrored_x } else { unflipped_x },
        sprite_top_left[1] + scale[1] * if flip_y { mirrored_y } else { unflipped_y },
    ]
}

/// Composes one sprite-sheet frame into game-screen space and returns the
/// quad UVs (mirrored when the whole sprite flips). `sprite_top_left` is the
/// sprite's unrotated top-left (a feet-adjusted point for characters,
/// `position` for stage props).
#[allow(clippy::too_many_arguments)]
pub(crate) fn sprite_frame_matrix(
    sprite_top_left: Vec2,
    frame: &SpriteFrame,
    scale: Vec2,
    hitbox: [i32; 2],
    flip_x: bool,
    flip_y: bool,
    degrees: f64,
    page_size: [f64; 2],
) -> ([f64; 6], [[f64; 2]; 4]) {
    let content = sprite_content_top_left(sprite_top_left, frame, scale, hitbox, flip_x, flip_y);
    let center = [
        sprite_top_left[0] + hitbox[0] as f64 * scale[0] / 2.0,
        sprite_top_left[1] + hitbox[1] as f64 * scale[1] / 2.0,
    ];
    let mut matrix = rotate_about(
        [scale[0], 0.0, 0.0, scale[1], content[0], content[1]],
        center,
        degrees,
    );
    if frame.rotated {
        matrix = concat(matrix, rotated_tile_matrix(frame.rect[2]));
    }
    let mut uv = quad_uv(frame.rect, page_size);
    if flip_x {
        uv.swap(0, 1);
        uv.swap(2, 3);
    }
    if flip_y {
        uv.swap(0, 3);
        uv.swap(1, 2);
    }
    (matrix, uv)
}

fn hold_flags<T>(holds: &[Option<T>; 4]) -> [bool; 4] {
    [
        holds[0].is_some(),
        holds[1].is_some(),
        holds[2].is_some(),
        holds[3].is_some(),
    ]
}

/// Strumline.hx:653-669/721-727 with the ADR-0019 previous-frame snapshot:
/// the opponent hold remains active through its end tick and leaves
/// confirm-hold on the first grid tick strictly after `end_ms`.
fn prune_opponent_holds(holds: &mut [Option<OpponentHold>; 4], sim_ms: f64) {
    for hold in holds.iter_mut() {
        if hold.as_ref().is_some_and(|hold| sim_ms > hold.end_ms) {
            *hold = None;
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReceptorState {
    ConfirmHold,
    Confirm,
    Press,
    Static,
}

/// Confirm animation span in milliseconds at the note-style 24 fps.
fn confirm_duration_ms(frame_count: usize) -> f64 {
    frame_count as f64 / NOTE_STYLE_FRAME_RATE * 1000.0
}

/// Receptor selection, including the observed player post-finish fallback:
/// while the key stays held and the confirm animation has finished, use press
/// frames (Strumline.hx:774-779 `onAnimationFinished` -> playStatic/press
/// fallback). A release already clears the confirm state in [`LineState`].
fn receptor_state(
    line: &LineState,
    direction: usize,
    held: bool,
    is_player: bool,
    now_ms: f64,
    confirm_frame_count: usize,
) -> ReceptorState {
    if held && (!is_player || line.pressed[direction]) {
        return ReceptorState::ConfirmHold;
    }
    let confirm_elapsed_ms = now_ms - line.confirm_started_ms[direction];
    let confirm_finished = confirm_elapsed_ms >= confirm_duration_ms(confirm_frame_count);
    if now_ms < line.confirm_until_ms[direction] && (!is_player || !confirm_finished) {
        return ReceptorState::Confirm;
    }
    if line.pressed[direction] {
        ReceptorState::Press
    } else {
        ReceptorState::Static
    }
}

/// Native frame-content top-left from the frozen matrix chain
/// (RENDER-BEHAVIOR §3 CONTENT placement, corrected 2026-11-08 after the
/// reachability verification of `StrumlineNote.fixOffsets`):
///
/// ```text
/// content_top_left = sprite_top_left + s*dest_offset
/// ```
///
/// `dest_offset` is the Sparrow `(-frameX, -frameY)` exactly as stored by the
/// oxide-atlas contract (positive 37 for the static Left frame). The algebra
/// remains: `centerOrigin` uses the UNSCALED frame centre
/// (`FlxSprite.hx:995-998`), OpenFL's `Matrix.scale` scales the accumulated
/// translation (`tx *= sx; ty *= sy`, `openfl/geom/Matrix.hx`), and the
/// FlxSprite draw subtracts the sprite offset (`drawFrameComplex`,
/// FlxSprite.hx:876-905) — the origin and `centerOffsets` terms cancel.
/// `StrumlineNote.fixOffsets()` (the −13 confirm compensation) is DEAD CODE
/// in this revision: it has no call site (grep over the source shows only the
/// definition), while `playAnimation`/`setup`/`update` call
/// `centerOffsets(); centerOrigin()` for every receptor state
/// (`StrumlineNote.hx:121-127`). No glow term exists for receptors or notes.
/// EXACT native receptor content top-left per the human-playtest finding
/// (RENDER-BEHAVIOR §3; the "origin and offset cancel" drift is WRONG because
/// flixel `set_frame` -> `resetFrameSize()` tracks the CURRENT frame's
/// sourceSize while the hitbox `width/height` is frozen at the single
/// setup-time `updateHitbox()` (StrumlineNote.hx:113-115); `centerOrigin`
/// therefore uses the current frame and `centerOffsets` the hitbox baseline):
///
/// `content[i] = sprite[i] + scale * ((hitbox_dest[i] - frame.dest_size[i]) *
/// 0.5 + frame.dest_offset[i])`
///
/// The `hitbox_dest` term shifts every frame whose `dest_size` differs from
/// the setup frame (press frames move ~30 px down/right; delta = native).
fn native_content_top_left(
    sprite_top_left: Vec2,
    frame: &SpriteFrame,
    scale: f64,
    hitbox_dest: [f64; 2],
) -> Vec2 {
    [
        sprite_top_left[0]
            + scale
                * (0.5 * (hitbox_dest[0] - frame.dest_size[0] as f64)
                    + frame.dest_offset[0] as f64),
        sprite_top_left[1]
            + scale
                * (0.5 * (hitbox_dest[1] - frame.dest_size[1] as f64)
                    + frame.dest_offset[1] as f64),
    ]
}

fn receptor_frame(frames: &[SpriteFrame], elapsed_ms: f64, looped: bool) -> &SpriteFrame {
    let raw = (elapsed_ms.max(0.0) / 1000.0 * NOTE_STYLE_FRAME_RATE).floor() as usize;
    if looped {
        &frames[raw % frames.len()]
    } else {
        &frames[raw.min(frames.len() - 1)]
    }
}

fn sprite_frame_quad(
    frame: &SpriteFrame,
    content_top_left: Vec2,
    scale: f64,
    page_size: [f64; 2],
) -> Quad {
    // The caller passes the native content top-left from
    // [`native_content_top_left`]; the matrix translate is that point and the
    // locals are the raw source rect, so the scale is applied exactly once
    // (ADR-0020 addendum 4 item 4).
    let mut matrix = [
        scale,
        0.0,
        0.0,
        scale,
        content_top_left[0],
        content_top_left[1],
    ];
    if frame.rotated {
        matrix = concat(matrix, rotated_tile_matrix(frame.rect[2]));
    }
    quad_from_matrix(
        matrix,
        frame.rect[2] as f64,
        frame.rect[3] as f64,
        quad_uv(frame.rect, page_size),
    )
}

pub(crate) fn quad_uv(rect: [i32; 4], page_size: [f64; 2]) -> [[f64; 2]; 4] {
    let x0 = rect[0] as f64 / page_size[0];
    let y0 = rect[1] as f64 / page_size[1];
    let x1 = (rect[0] + rect[2]) as f64 / page_size[0];
    let y1 = (rect[1] + rect[3]) as f64 / page_size[1];
    [[x0, y0], [x1, y0], [x1, y1], [x0, y1]]
}

fn visible(top: f64, height: f64) -> bool {
    top < GAME_HEIGHT && top + height > 0.0
}

/// `parent ∘ local` in the atlas matrix convention:
/// `[a,b,c,d,tx,ty]` maps `(x,y) -> (a*x + c*y + tx, b*x + d*y + ty)`.
pub(crate) fn concat(parent: [f64; 6], local: [f64; 6]) -> [f64; 6] {
    [
        parent[0] * local[0] + parent[2] * local[1],
        parent[1] * local[0] + parent[3] * local[1],
        parent[0] * local[2] + parent[2] * local[3],
        parent[1] * local[2] + parent[3] * local[3],
        parent[0] * local[4] + parent[2] * local[5] + parent[4],
        parent[1] * local[4] + parent[3] * local[5] + parent[5],
    ]
}

fn apply(matrix: [f64; 6], x: f64, y: f64) -> [f64; 2] {
    [
        matrix[0] * x + matrix[2] * y + matrix[4],
        matrix[1] * x + matrix[3] * y + matrix[5],
    ]
}

/// Projects a world point through the camera into game-screen coordinates,
/// expressed as a composed matrix (used before applying a quad matrix).
pub(crate) fn camera_projection_matrix(camera: &Camera, scroll_factor: Vec2) -> [f64; 6] {
    let tx = HALF_WIDTH - camera.zoom * (camera.scroll[0] * scroll_factor[0] + HALF_WIDTH);
    let ty = HALF_HEIGHT - camera.zoom * (camera.scroll[1] * scroll_factor[1] + HALF_HEIGHT);
    [camera.zoom, 0.0, 0.0, camera.zoom, tx, ty]
}

/// Builds a quad from a local `(0..width, 0..height)` rectangle transformed by
/// `matrix` into game-screen coordinates (Flixel y-down). The final Bevy
/// world position mirrors the y axis around the viewport center.
///
/// Shared with the interactive HUD so both use the exact same screen-to-Bevy
/// conversion as the scene layers.
pub(crate) fn quad_from_matrix(
    matrix: [f64; 6],
    width: f64,
    height: f64,
    uv: [[f64; 2]; 4],
) -> Quad {
    let local = [[0.0, 0.0], [width, 0.0], [width, height], [0.0, height]];
    let mut pos = [[0.0; 2]; 4];
    for (index, point) in local.iter().enumerate() {
        let screen = apply(matrix, point[0], point[1]);
        pos[index] = [screen[0] - HALF_WIDTH, HALF_HEIGHT - screen[1]];
    }
    Quad { pos, uv }
}

fn trace_sim(event: &TraceEvent) -> u64 {
    match event {
        TraceEvent::Hit { sim, .. }
        | TraceEvent::Miss { sim, .. }
        | TraceEvent::GhostPress { sim, .. }
        | TraceEvent::HoldStart { sim, .. }
        | TraceEvent::HoldTick { sim, .. }
        | TraceEvent::HoldDrop { sim, .. }
        | TraceEvent::HoldEnd { sim, .. }
        | TraceEvent::GameOver { sim, .. }
        | TraceEvent::OpponentHit { sim, .. }
        | TraceEvent::CameraFocus { sim, .. }
        | TraceEvent::CameraZoom { sim, .. }
        | TraceEvent::CameraBop { sim, .. }
        | TraceEvent::PlayAnimation { sim, .. }
        | TraceEvent::ScrollSpeed { sim, .. }
        | TraceEvent::HealthIcon { sim, .. }
        | TraceEvent::Choreography { sim, .. }
        | TraceEvent::UnknownEvent { sim, .. } => *sim,
    }
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

    fn sprite_frame(rect: [i32; 4], dest_offset: [i32; 2], dest_size: [i32; 2]) -> SpriteFrame {
        SpriteFrame {
            page: 0,
            rect,
            dest_offset,
            dest_size,
            rotated: false,
        }
    }

    /// A loaded solid prop with the loader's integer makeSolidColor size and
    /// the original data `scale` pair kept beside it.
    fn solid_prop(size: [i32; 2], scale: (f64, f64), alpha: f64) -> LoadedStageProp {
        LoadedStageProp {
            name: "solidCover".to_string(),
            z_index: 9999.0,
            position: (100.0, 200.0),
            scale,
            scroll: (1.0, 1.0),
            alpha,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
            visual: PropVisual::Solid {
                size,
                color: [0.0, 0.0, 0.0, 1.0],
            },
            animations: std::collections::BTreeMap::new(),
            dance_every: 0.0,
            starting_animation: None,
            bopper: false,
        }
    }

    #[test]
    fn solid_prop_quad_consumes_scale_once_as_make_solid_color_dimensions() {
        // scale [2400, 2000] is already consumed at load time; the draw path
        // must emit ONE 2400x2000 rect with a unit transform (a second scale
        // application would produce 5,760,000 x 4,000,000).
        let prop = solid_prop([2400, 2000], (2400.0, 2000.0), 1.0);
        let rig = PropRig::from_loaded(prop);
        let quads = Scene::draw_prop(&Camera::new(1.0), &rig).expect("solid prop draws");
        assert_eq!(quads.len(), 1, "one solid rect");
        let (page, quad) = &quads[0];
        assert_eq!(*page, None, "solid props are untextured");
        close(quad.pos[1][0] - quad.pos[0][0], 2400.0);
        close(quad.pos[0][1] - quad.pos[3][1], 2000.0);
        // Camera::new(1.0) has scroll 0/zoom 1 and the prop scroll factor is
        // 1, so the screen-space top-left is the prop position.
        close(quad.pos[0][0] + HALF_WIDTH, 100.0);
        close(HALF_HEIGHT - quad.pos[0][1], 200.0);
        assert!(quad.uv.iter().all(|uv| *uv == [0.0, 0.0]));
    }

    #[test]
    fn zero_alpha_solid_layer_contributes_no_visible_color() {
        // sserafim solidCover: alpha 0.0 over the whole screen. The material
        // tint must carry that alpha; the quad itself stays geometrically
        // present and blends invisibly (harness AlphaMode2d::Blend).
        let cover = solid_prop([10000, 10000], (10000.0, 10000.0), 0.0);
        assert_eq!(prop_layer_color(&cover), [0.0, 0.0, 0.0, 0.0]);
        let half = solid_prop([2400, 2000], (2400.0, 2000.0), 0.5);
        assert_eq!(prop_layer_color(&half), [0.0, 0.0, 0.0, 0.5]);
        // Non-solid props keep white RGB and apply the data alpha too.
        let mut textured = solid_prop([1, 1], (1.0, 1.0), 0.25);
        textured.visual = PropVisual::Static {
            page: 0,
            image_size: [1.0, 1.0],
        };
        assert_eq!(prop_layer_color(&textured), [1.0, 1.0, 1.0, 0.25]);
    }

    #[test]
    fn background_underlay_is_omitted_at_the_default_zero_opacity() {
        assert_eq!(BACKGROUND_ALPHA, 0.0);
        assert!(
            background_quads(BACKGROUND_ALPHA).is_empty(),
            "alpha 0 emits no background quads"
        );
        // The documented geometry helper and widths remain available.
        assert_eq!(
            geometry::background_top_left(OPPONENT_STRUMLINE_X),
            [32.0, 24.0]
        );
        assert_eq!(
            geometry::background_top_left(PLAYER_STRUMLINE_X),
            [672.0, 24.0]
        );
        assert_eq!(background_quads(1.0).len(), 2);
    }

    /// Real Sparrow data transcribed from
    /// `assets/preload/images/noteStrumline.xml` (static/press/confirm Left
    /// frames):
    ///
    /// ```text
    /// staticLeft0001:  x=1474 y=0   width=154 height=157
    ///                  frameX=-37  frameY=-38  frameWidth=232 frameHeight=236
    /// pressLeft0001:   x=142  y=0   width=139 height=141
    ///                  frameX=-4   frameY=-3   frameWidth=146 frameHeight=148
    /// confirmLeft0001: x=1564 y=424 width=226 height=228
    ///                  frameX=-2   frameY=-2   frameWidth=232 frameHeight=236
    /// ```
    ///
    /// The atlas contract exposes `dest_offset = (-frameX, -frameY)` and
    /// `dest_size = (frameWidth, frameHeight)`.
    fn real_left_receptor_frames() -> [SpriteFrame; 3] {
        [
            sprite_frame([1474, 0, 154, 157], [37, 38], [232, 236]),
            sprite_frame([142, 0, 139, 141], [4, 3], [146, 148]),
            sprite_frame([1564, 424, 226, 228], [2, 2], [232, 236]),
        ]
    }

    /// `Quad::pos` is Bevy world space (origin at the viewport centre, y-up).
    /// The frozen game-screen coordinate is recovered with the documented
    /// inverse conversion `[x + HALF_WIDTH, HALF_HEIGHT - y]`.
    fn quad_content_corner(quad: &Quad) -> Vec2 {
        [quad.pos[0][0] + HALF_WIDTH, HALF_HEIGHT - quad.pos[0][1]]
    }

    #[test]
    fn native_content_placement_matches_the_frozen_matrix_chain() {
        let [static_frame, press_frame, confirm_frame] = real_left_receptor_frames();
        let scale = 0.7;
        let relative =
            |frame: &SpriteFrame| native_content_top_left([0.0, 0.0], frame, scale, [235.0, 230.0]);

        // static: 0.7*((235-232)/2 + 37, (230-236)/2 + 38) = (26.95, 24.5).
        let static_rel = relative(&static_frame);
        close(static_rel[0], 26.95);
        close(static_rel[1], 24.5);

        // press: 0.7*((235-146)/2 + 4, (230-148)/2 + 3) = (33.95, 30.8) —
        // the hitbox term swings press back into lane alignment (~30 px
        // versus the raw-offset reading).
        let press_rel = relative(&press_frame);
        close(press_rel[0], 33.95);
        close(press_rel[1], 30.8);

        // confirm: 0.7*((235-232)/2 + 2, (230-236)/2 + 2) = (2.45, -0.7).
        // Confirm-hold shares the confirm dest/offset frame pair.
        let confirm_rel = relative(&confirm_frame);
        close(confirm_rel[0], 2.45);
        close(confirm_rel[1], -0.7);
    }

    #[test]
    fn sprite_frame_quad_pins_the_real_sparrow_content_corners() {
        let [static_frame, press_frame, confirm_frame] = real_left_receptor_frames();
        let scale = 0.7;
        // Player line dir 0: receptor_top_left(688, 0, 0) = (659.4, 24.0).
        let base = [659.4, 24.0];
        let page = [1920.0, 1080.0];

        // static: 659.4 + 26.95 = 686.35; 24 + 24.5 = 48.5.
        let static_quad = sprite_frame_quad(
            &static_frame,
            native_content_top_left(base, &static_frame, scale, [235.0, 230.0]),
            scale,
            page,
        );
        let static_corner = quad_content_corner(&static_quad);
        close(static_corner[0], 686.35);
        close(static_corner[1], 48.5);
        // The same corner as stored in `pos[0]` (Bevy world space).
        close(static_quad.pos[0][0], 686.35 - HALF_WIDTH);
        close(static_quad.pos[0][1], HALF_HEIGHT - 48.5);
        // The 0.7 scale is applied exactly once to the raw source rect.
        close(static_quad.pos[1][0] - static_quad.pos[0][0], 154.0 * 0.7);
        close(static_quad.pos[0][1] - static_quad.pos[3][1], 157.0 * 0.7);
        assert_eq!(static_quad.uv, quad_uv(static_frame.rect, page));

        // press: 659.4 + 33.95 = 693.35; 24 + 30.8 = 54.8.
        let press_quad = sprite_frame_quad(
            &press_frame,
            native_content_top_left(base, &press_frame, scale, [235.0, 230.0]),
            scale,
            page,
        );
        let press_corner = quad_content_corner(&press_quad);
        close(press_corner[0], 693.35);
        close(press_corner[1], 54.8);
        close(press_quad.pos[0][0], 693.35 - HALF_WIDTH);
        close(press_quad.pos[0][1], HALF_HEIGHT - 54.8);
        close(press_quad.pos[1][0] - press_quad.pos[0][0], 139.0 * 0.7);
        close(press_quad.pos[0][1] - press_quad.pos[3][1], 141.0 * 0.7);

        // confirm: 659.4 + 2.45 = 661.85; 24 - 0.7 = 23.3. Confirm-hold
        // shares the confirm frame/prefix, so it has the same content corner.
        let confirm_quad = sprite_frame_quad(
            &confirm_frame,
            native_content_top_left(base, &confirm_frame, scale, [235.0, 230.0]),
            scale,
            page,
        );
        let confirm_corner = quad_content_corner(&confirm_quad);
        close(confirm_corner[0], 661.85);
        close(confirm_corner[1], 23.3);
        close(confirm_quad.pos[0][0], 661.85 - HALF_WIDTH);
        close(confirm_quad.pos[0][1], HALF_HEIGHT - 23.3);
        close(confirm_quad.pos[1][0] - confirm_quad.pos[0][0], 226.0 * 0.7);
        close(confirm_quad.pos[0][1] - confirm_quad.pos[3][1], 228.0 * 0.7);
    }

    #[test]
    fn note_content_placement_uses_the_same_formula() {
        // notes.xml noteLeft0001: x=157 y=0, 154x157, frameX/Y=-0,
        // frameWidth/Height=154x157 (no trim offset).
        let frame = sprite_frame([157, 0, 154, 157], [0, 0], [154, 157]);
        // §3 note x uses the SCALED native sprite width (154*0.7 = 107.8):
        // x = 688 - (107.8 - 104)/2 - 2 = 684.1; y = 24 + 28.6 = 52.6.
        let base = note_top_left(
            PLAYER_STRUMLINE_X,
            0,
            frame.dest_size[0] as f64 * 0.7,
            800.0,
            800.0,
            1.2,
        );
        close(base[0], 684.1);
        close(base[1], 52.6);

        // untrimmed per-direction, but the setup hitbox is the SHEET's first
        // frame noteDown0001 = (157,154): rel = 0.7*((157-154)/2, (154-157)/2)
        // = (1.05, -1.05).
        let content = native_content_top_left(base, &frame, 0.7, [157.0, 154.0]);
        close(content[0], 685.15);
        close(content[1], 51.55);

        let quad = sprite_frame_quad(&frame, content, 0.7, [1920.0, 1080.0]);
        close(quad.pos[0][0], content[0] - HALF_WIDTH);
        close(quad.pos[0][1], HALF_HEIGHT - content[1]);
        close(quad.pos[1][0] - quad.pos[0][0], 154.0 * 0.7);
        close(quad.pos[0][1] - quad.pos[3][1], 157.0 * 0.7);
    }

    #[test]
    fn character_world_matrix_matches_frozen_composition() {
        let bounds = [0.0, 0.0, 100.0, 200.0];
        let top_left = [10.0, 20.0];
        let p = [10.0, 20.0];

        let matrix = character_world_matrix(bounds, 1.0, false, top_left, [0.0, 0.0]);
        assert_eq!(apply(matrix, p[0], p[1]), [20.0, 40.0]);

        let mirrored = character_world_matrix(bounds, 1.0, true, top_left, [0.0, 0.0]);
        assert_eq!(apply(mirrored, p[0], p[1]), [100.0, 40.0]);

        let scaled = character_world_matrix(bounds, 2.0, false, top_left, [-4.0, 6.0]);
        assert_eq!(apply(scaled, p[0], p[1]), [26.0, 66.0]);
    }

    #[test]
    fn character_quad_matrix_keeps_the_atlas_tile_rotation() {
        // A rotated spritemap entry arrives with its tile matrix already
        // folded into SpriteQuad::matrix by oxide-atlas. The renderer applies
        // that matrix unchanged; it does not inspect `rotated` again.
        let bounds = [0.0, 0.0, 100.0, 200.0];
        let world = character_world_matrix(bounds, 1.0, false, [10.0, 20.0], [0.0, 0.0]);
        let quad = oxide_atlas::SpriteQuad {
            bitmap_name: "7".to_string(),
            atlas_rect: [0, 0, 40, 30],
            matrix: [0.0, 1.0, -1.0, 0.0, 40.0, 0.0],
            rotated: true,
        };
        let composed = concat(world, quad.matrix);
        // local (0,0) -> atlas tile (40,0) -> world (50,20)
        assert_eq!(apply(composed, 0.0, 0.0), [50.0, 20.0]);
        // local (40,30) -> atlas tile (10,40) -> world (20,60)
        assert_eq!(apply(composed, 40.0, 30.0), [20.0, 60.0]);
    }

    #[test]
    fn camera_projection_matrix_matches_the_scalar_formula() {
        let mut camera = Camera::new(1.1);
        camera.focus_on([496.0, 361.0]);
        let matrix = camera_projection_matrix(&camera, [1.0, 1.0]);
        assert_eq!(
            apply(matrix, 0.0, 0.0),
            camera.project([0.0, 0.0], [1.0, 1.0])
        );
    }

    #[test]
    fn opponent_hold_stays_confirm_hold_through_end_and_static_after() {
        // Strumline.hx:653-669/721-727 with the previous-frame snapshot:
        // confirm-hold at end_ms, static at end_ms + 1.
        let mut holds = [None, None, None, None];
        holds[0] = Some(OpponentHold { end_ms: 1_000.0 });
        let mut line = LineState::default();
        line.confirm_opponent(0, 100.0);
        let confirm_frames = 4;

        prune_opponent_holds(&mut holds, 999.0);
        assert!(hold_flags(&holds)[0], "hold active during its sustain");
        assert_eq!(
            receptor_state(
                &line,
                0,
                hold_flags(&holds)[0],
                false,
                999.0,
                confirm_frames
            ),
            ReceptorState::ConfirmHold
        );

        prune_opponent_holds(&mut holds, 1_000.0);
        assert!(hold_flags(&holds)[0], "hold still active at end_ms");
        assert_eq!(
            receptor_state(
                &line,
                0,
                hold_flags(&holds)[0],
                false,
                1_000.0,
                confirm_frames
            ),
            ReceptorState::ConfirmHold
        );

        prune_opponent_holds(&mut holds, 1_001.0);
        assert!(!hold_flags(&holds)[0], "hold pruned at end_ms + 1");
        assert_eq!(
            receptor_state(
                &line,
                0,
                hold_flags(&holds)[0],
                false,
                1_001.0,
                confirm_frames
            ),
            ReceptorState::Static
        );
    }

    #[test]
    fn player_confirm_falls_back_to_press_after_finish_then_release_static() {
        let mut player = LineState::default();
        player.press(2, 500.0);
        player.confirm_player(2, 500.0);
        assert!(player.pressed[2]);
        assert_eq!(
            player.confirm_until_ms[2],
            f64::INFINITY,
            "player confirm has no 0.15 s auto-reset while held"
        );

        // funkin confirm prefixes resolve to four frames at 24 fps. The scene
        // grid is 1 ms, so the first tick after the 166.67 ms span is 167 ms.
        let confirm_frames = 4;
        let duration = confirm_duration_ms(confirm_frames);
        let finish_ms = 500.0 + duration.ceil();
        assert_eq!(
            receptor_state(&player, 2, false, true, 500.0, confirm_frames),
            ReceptorState::Confirm
        );
        assert_eq!(
            receptor_state(&player, 2, false, true, finish_ms - 1.0, confirm_frames),
            ReceptorState::Confirm
        );
        // Held key + finished confirm animation -> press frames.
        assert_eq!(
            receptor_state(&player, 2, false, true, finish_ms, confirm_frames),
            ReceptorState::Press
        );

        // Release clears the confirm and falls to static immediately.
        player.release(2);
        assert!(!player.pressed[2]);
        assert_eq!(player.confirm_until_ms[2], 0.0);
        assert_eq!(player.confirm_started_ms[2], 0.0);
        assert_eq!(
            receptor_state(&player, 2, false, true, finish_ms + 1.0, confirm_frames),
            ReceptorState::Static
        );

        let mut opponent = LineState::default();
        opponent.confirm_opponent(0, 100.0);
        assert_eq!(
            opponent.confirm_until_ms[0],
            100.0 + super::super::geometry::RECEPTOR_CONFIRM_MS
        );
    }

    // --- M6-CONTRACT §12.7 scene event consumption -------------------------

    use std::collections::BTreeMap;

    use crate::render::anim::AnimationDef;
    use crate::render::conversation::ConversationPhase;
    use crate::render::dialogue::{BoxAsset, DialogueSheet, LoadedDialogue};
    use crate::render::load::ReceptorFrames;
    use oxide_audio::conductor::TimeChange;
    use oxide_data::dialogue::{
        BackdropData, ConversationData, DialogueBoxData, DialogueBoxTextData, DialogueEntryData,
        OutroData,
    };

    fn animation(frames: &[usize], looped: bool) -> AnimationDef {
        AnimationDef::test(frames, 24.0, looped)
    }

    fn test_character(
        role: Role,
        position: (f64, f64),
        animations: BTreeMap<String, AnimationDef>,
        starting_animation: Option<&str>,
    ) -> LoadedCharacter {
        LoadedCharacter {
            role,
            id: role.label().to_string(),
            sheets: Vec::new(),
            bounds: [0.0, 0.0, 100.0, 200.0],
            sprite_hitbox: None,
            animations,
            data_offsets: (0.0, 0.0),
            camera_offsets: (0.0, 0.0),
            sing_time_steps: 8.0,
            dance_every: 1.0,
            scale: 1.0,
            flip_x: false,
            starting_animation: starting_animation.map(str::to_owned),
            slot: super::super::load::LoadedSlot {
                z_index: 0.0,
                position,
                scale: 1.0,
                camera_offsets: (0.0, 0.0),
                scroll: (1.0, 1.0),
            },
            health_icon: crate::render::hud::IconConfig {
                id: role.label().to_string(),
                is_pixel: false,
                should_bop: true,
                scale: 1.0,
                flip_x: false,
                offsets: [0.0, 0.0],
            },
        }
    }

    fn test_note_style() -> LoadedNoteStyle {
        let frame = SpriteFrame {
            page: 0,
            rect: [0, 0, 10, 10],
            dest_offset: [0, 0],
            dest_size: [10, 10],
            rotated: false,
        };
        let receptors = ReceptorFrames {
            static_frames: vec![frame],
            press_frames: vec![frame],
            confirm_frames: vec![frame],
            confirm_hold_frames: vec![frame],
        };
        LoadedNoteStyle {
            note_scale: 1.0,
            note_frames: [frame; 4],
            note_page_size: [100.0, 100.0],
            note_hitbox_dest: [10.0, 10.0],
            receptor_scale: 1.0,
            receptor_offsets: (0.0, 0.0),
            receptors: [
                receptors.clone(),
                receptors.clone(),
                receptors.clone(),
                receptors,
            ],
            receptor_page: 0,
            receptor_page_size: [100.0, 100.0],
            receptor_hitbox_dest: [10.0, 10.0],
            hold_page: 0,
            hold_page_size: [80.0, 10.0],
            hold_scale: 1.0,
            hold_offsets: (0.0, 0.0),
            hold_width: 10.0,
        }
    }

    fn test_scene_with(
        characters: Vec<LoadedCharacter>,
        props: Vec<LoadedStageProp>,
        chart_scroll: f64,
    ) -> Scene {
        test_scene_with_notes(characters, props, chart_scroll, Vec::new())
    }

    fn test_scene_with_notes(
        characters: Vec<LoadedCharacter>,
        props: Vec<LoadedStageProp>,
        chart_scroll: f64,
        notes: Vec<LoadedNote>,
    ) -> Scene {
        let loaded = LoadedSlice {
            pages: Vec::new(),
            stage_zoom: 1.1,
            props,
            characters,
            note_style: test_note_style(),
            notes,
            scroll_speed: chart_scroll,
            conductor_changes: vec![TimeChange {
                time_stamp: 0.0,
                beat_time: 0.0,
                bpm: 100.0,
                time_signature_num: 4,
                time_signature_den: 4,
            }],
            player_vocals: Vec::new(),
            opponent_vocals: Vec::new(),
            hud: None,
            dialogue: None,
            weekend1_props: None,
            cutscene_changes: None,
        };
        Scene::new(loaded).expect("synthetic scene builds").0
    }

    fn empty_dialogue_entry(box_id: &str, lines: usize) -> DialogueEntryData {
        DialogueEntryData {
            speaker: String::new(),
            speaker_animation: String::new(),
            box_id: box_id.to_string(),
            box_animation: "enter".to_string(),
            text: (0..lines).map(|_| String::new()).collect(),
            speed: None,
            extra: BTreeMap::new(),
        }
    }

    /// A raster-free conversation: empty lines never bake glyph pages, so the
    /// scene-level advance/order tests run without reference assets.
    fn synthetic_dialogue() -> LoadedDialogue {
        let conversation = ConversationData {
            version: "1.0.0".to_string(),
            backdrop: BackdropData::Solid {
                fade_time: 0.0,
                color: "#000000".to_string(),
                extra: BTreeMap::new(),
            },
            music: None,
            outro: Some(OutroData::None {
                extra: BTreeMap::new(),
            }),
            dialogue: vec![
                empty_dialogue_entry("synthetic", 2),
                empty_dialogue_entry("synthetic", 1),
            ],
            extra: BTreeMap::new(),
        };
        let box_data = DialogueBoxData {
            version: "1.1.0".to_string(),
            name: "Synthetic".to_string(),
            asset_path: "synthetic".to_string(),
            is_pixel: false,
            flip_x: false,
            flip_y: false,
            offsets: (0.0, 0.0),
            scale: 1.0,
            text: DialogueBoxTextData {
                offsets: (0.0, 0.0),
                width: 100,
                size: None,
                color: "#000000".to_string(),
                font_family: "Pixel Arial 11 Bold".to_string(),
                shadow_color: None,
                shadow_width: Some(0),
                extra: BTreeMap::new(),
            },
            animations: Vec::new(),
            extra: BTreeMap::new(),
        };
        LoadedDialogue {
            conversation_id: "test".to_string(),
            conversation,
            boxes: BTreeMap::from([(
                "synthetic".to_string(),
                BoxAsset {
                    data: box_data,
                    linear_page: 0,
                    pixel_page: 0,
                    sheet: DialogueSheet {
                        page_size: [1.0, 1.0],
                        frames: Vec::new(),
                        clips: BTreeMap::new(),
                    },
                },
            )]),
            speakers: BTreeMap::new(),
            font_bytes: Vec::new(),
        }
    }

    fn test_scene_with_dialogue(dialogue: LoadedDialogue) -> Scene {
        let loaded = LoadedSlice {
            pages: Vec::new(),
            stage_zoom: 1.1,
            props: Vec::new(),
            characters: vec![
                test_character(Role::Player, (100.0, 200.0), bf_animations(), Some("idle")),
                test_character(Role::Opponent, (300.0, 400.0), BTreeMap::new(), None),
                test_character(Role::Girlfriend, (500.0, 600.0), BTreeMap::new(), None),
            ],
            note_style: test_note_style(),
            notes: Vec::new(),
            scroll_speed: 1.5,
            conductor_changes: vec![TimeChange {
                time_stamp: 0.0,
                beat_time: 0.0,
                bpm: 100.0,
                time_signature_num: 4,
                time_signature_den: 4,
            }],
            player_vocals: Vec::new(),
            opponent_vocals: Vec::new(),
            hud: None,
            dialogue: Some(dialogue),
            weekend1_props: None,
            cutscene_changes: None,
        };
        Scene::new(loaded)
            .expect("synthetic dialogue scene builds")
            .0
    }

    fn bf_animations() -> BTreeMap<String, AnimationDef> {
        let mut animations = BTreeMap::new();
        animations.insert("idle".to_string(), animation(&[0, 1], true));
        animations.insert("hey".to_string(), animation(&[5, 6], false));
        animations.insert("singLEFT".to_string(), animation(&[1], false));
        animations
    }

    fn default_scene() -> Scene {
        test_scene_with(
            vec![
                test_character(Role::Player, (100.0, 200.0), bf_animations(), Some("idle")),
                test_character(Role::Opponent, (300.0, 400.0), BTreeMap::new(), None),
                test_character(Role::Girlfriend, (500.0, 600.0), BTreeMap::new(), None),
            ],
            Vec::new(),
            1.5,
        )
    }

    fn step_empty(scene: &mut Scene, from: u64, to: u64) {
        for tick in from..=to {
            scene.step(tick, &[], &[]).expect("scene step");
        }
    }

    fn camera_focus_row(
        sim: u64,
        target_kind: &'static str,
        x: f64,
        y: f64,
        duration: f64,
        ease_kind: &'static str,
        ease: Option<&str>,
    ) -> TraceEvent {
        TraceEvent::CameraFocus {
            schema: "3",
            kind: "camera_focus",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            target_char: 0,
            target_kind,
            x,
            y,
            world_focus: [0.0, 0.0],
            duration,
            ease_kind,
            ease: ease.map(str::to_owned),
            ease_dir: None,
        }
    }

    fn camera_zoom_row(
        sim: u64,
        resolved_target_zoom: f64,
        duration: f64,
        ease: Option<&str>,
    ) -> TraceEvent {
        TraceEvent::CameraZoom {
            schema: "3",
            kind: "camera_zoom",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            zoom: resolved_target_zoom,
            mode: "direct",
            resolved_target_zoom,
            duration,
            ease: ease.map(str::to_owned),
            ease_dir: None,
        }
    }

    fn camera_bop_row(
        sim: u64,
        intensity: f64,
        hud_intensity: f64,
        rate: f64,
        offset: f64,
    ) -> TraceEvent {
        TraceEvent::CameraBop {
            schema: "3",
            kind: "camera_bop",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            intensity: 1.0,
            camera_bop_intensity: intensity,
            hud_intensity,
            rate,
            offset,
        }
    }

    fn scroll_row(
        sim: u64,
        strumline: &'static str,
        target: f64,
        base_scroll: f64,
        duration: f64,
        ease: Option<&str>,
    ) -> TraceEvent {
        TraceEvent::ScrollSpeed {
            schema: "3",
            kind: "scroll_speed",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            strumline,
            target,
            absolute: true,
            base_scroll,
            duration,
            ease: ease.map(str::to_owned),
            ease_dir: None,
        }
    }

    fn play_animation_row(
        sim: u64,
        anim: &str,
        force: bool,
        resolved_kind: &str,
        skipped_old: bool,
    ) -> TraceEvent {
        TraceEvent::PlayAnimation {
            schema: "3",
            kind: "play_animation",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            target: "bf".to_string(),
            anim: anim.to_string(),
            force,
            resolved_kind: resolved_kind.to_string(),
            skipped_old,
        }
    }

    /// Named-ease FocusCamera: the follow tween advances from the CURRENT
    /// scroll, disables following while active, then on completion re-enables
    /// following and snaps to `target - (640, 360)`.
    #[test]
    fn camera_focus_named_tween_advances_and_snaps_on_completion() {
        let mut scene = default_scene();
        scene
            .step(
                0,
                &[camera_focus_row(
                    0,
                    "bf",
                    0.0,
                    0.0,
                    4.0,
                    "named",
                    Some("linear"),
                )],
                &[],
            )
            .expect("focus row applies");
        // 4 steps at 100 bpm = 600 ms. Follow is disabled during the tween.
        assert!(!scene.camera_events.follow_enabled);
        assert_eq!(scene.camera_events.follow_focus, [100.0, 100.0]);
        close(scene.camera.scroll[0], -340.0);
        close(scene.camera.scroll[1], -60.0);

        step_empty(&mut scene, 1, 300);
        close(scene.camera.scroll[0], -440.0);
        close(scene.camera.scroll[1], -160.0);
        assert!(!scene.camera_events.follow_enabled);

        step_empty(&mut scene, 301, 600);
        close(scene.camera.scroll[0], -540.0);
        close(scene.camera.scroll[1], -260.0);
        assert!(scene.camera_events.follow_enabled);
        assert!(scene.camera_events.follow_tween.is_none());
    }

    #[test]
    fn camera_focus_instant_snaps_and_classic_retargets_without_snap() {
        let mut scene = default_scene();
        scene
            .step(
                0,
                &[camera_focus_row(0, "dad", 10.0, 20.0, 4.0, "instant", None)],
                &[],
            )
            .expect("instant focus row applies");
        assert!(scene.camera_events.follow_enabled);
        assert_eq!(scene.camera_events.follow_focus, [310.0, 320.0]);
        close(scene.camera.scroll[0], -330.0);
        close(scene.camera.scroll[1], -40.0);

        // CLASSIC cancels an active tween, re-follows and retargets without a
        // snap; the end-of-step follow lerp starts converging.
        let mut scene = default_scene();
        scene
            .step(
                0,
                &[camera_focus_row(
                    0,
                    "bf",
                    0.0,
                    0.0,
                    4.0,
                    "named",
                    Some("linear"),
                )],
                &[],
            )
            .expect("named focus row applies");
        step_empty(&mut scene, 1, 101);
        scene
            .step(
                102,
                &[camera_focus_row(102, "gf", 0.0, 0.0, 4.0, "classic", None)],
                &[],
            )
            .expect("classic focus row applies");
        assert!(scene.camera_events.follow_enabled);
        assert!(scene.camera_events.follow_tween.is_none());
        assert_eq!(scene.camera_events.follow_focus, [500.0, 500.0]);
        let factor = 1.0 - (1.0 - super::super::geometry::CAMERA_FOLLOW_LERP).powf(0.06);
        let target = [500.0 - HALF_WIDTH, 500.0 - HALF_HEIGHT];
        // The classic row runs AFTER the tick's tween advance: at 102 ms the
        // linear tween from [-340, -60] to [-540, -260] is at t = 0.17.
        let t = 102.0 / 600.0;
        let before = [-340.0 - 200.0 * t, -60.0 - 200.0 * t];
        close(
            scene.camera.scroll[0],
            before[0] + (target[0] - before[0]) * factor,
        );
        close(
            scene.camera.scroll[1],
            before[1] + (target[1] - before[1]) * factor,
        );
    }

    /// §12.6 finding 1: the scene consults its own rigs; a bf/dad/gf row for
    /// an absent role applies nothing, while 'position'/'unknown' target the
    /// literal pair.
    #[test]
    fn camera_focus_missing_character_is_a_noop_and_position_uses_the_literal() {
        let mut scene = test_scene_with(
            vec![
                test_character(Role::Player, (100.0, 200.0), BTreeMap::new(), None),
                test_character(Role::Opponent, (300.0, 400.0), BTreeMap::new(), None),
            ],
            Vec::new(),
            1.5,
        );
        let scroll = scene.camera.scroll;
        scene
            .step(
                0,
                &[camera_focus_row(0, "gf", 7.0, 8.0, 4.0, "instant", None)],
                &[],
            )
            .expect("missing-gf row is a no-op");
        assert_eq!(scene.camera_events.follow_focus, [300.0, 300.0]);
        assert_eq!(scene.camera.scroll, scroll);

        scene
            .step(
                1,
                &[camera_focus_row(
                    1, "position", 100.0, 200.0, 4.0, "instant", None,
                )],
                &[],
            )
            .expect("position row applies");
        assert_eq!(scene.camera_events.follow_focus, [100.0, 200.0]);
        close(scene.camera.scroll[0], -540.0);
        close(scene.camera.scroll[1], -160.0);

        scene
            .step(
                2,
                &[camera_focus_row(
                    2, "unknown", 30.0, 40.0, 4.0, "instant", None,
                )],
                &[],
            )
            .expect("unknown row applies the literal target");
        assert_eq!(scene.camera_events.follow_focus, [30.0, 40.0]);
        close(scene.camera.scroll[0], -610.0);
        close(scene.camera.scroll[1], -320.0);
    }

    #[test]
    fn camera_focus_unresolved_ease_applies_nothing() {
        let mut scene = default_scene();
        let scroll = scene.camera.scroll;
        scene
            .step(
                0,
                &[camera_focus_row(0, "bf", 10.0, 20.0, 4.0, "named", None)],
                &[],
            )
            .expect("unresolved ease row is a no-op");
        assert_eq!(scene.camera_events.follow_focus, [300.0, 300.0]);
        assert_eq!(scene.camera.scroll, scroll);
        assert!(scene.camera_events.follow_enabled);
    }

    #[test]
    fn camera_zoom_instant_tween_and_unresolved_ease() {
        // INSTANT sets currentCameraZoom; the rendered zoom is assigned in
        // the NEXT tick's decay block.
        let mut scene = default_scene();
        scene
            .step(0, &[camera_zoom_row(0, 1.5, 4.0, Some("INSTANT"))], &[])
            .expect("instant zoom applies");
        close(scene.camera.current_zoom, 1.5);
        close(scene.camera.zoom, 1.1);
        step_empty(&mut scene, 1, 1);
        close(scene.camera.zoom, 1.5);

        // Named linear tween over 4 steps (600 ms).
        let mut scene = default_scene();
        scene
            .step(0, &[camera_zoom_row(0, 2.0, 4.0, Some("linear"))], &[])
            .expect("zoom tween starts");
        step_empty(&mut scene, 1, 300);
        close(scene.camera.current_zoom, 1.55);
        step_empty(&mut scene, 301, 600);
        close(scene.camera.current_zoom, 2.0);
        step_empty(&mut scene, 601, 601);
        close(scene.camera.zoom, 2.0);

        // Unresolved ease (null) is a native early return: no cancel/state.
        let mut scene = default_scene();
        scene
            .step(0, &[camera_zoom_row(0, 3.0, 4.0, None)], &[])
            .expect("unresolved zoom row is a no-op");
        close(scene.camera.current_zoom, 1.1);
        assert!(scene.camera_events.zoom_tween.is_none());
    }

    /// Mutable `SetCameraBop`: the gate pulses on the configured rate/offset,
    /// the HUD zoom bump feeds the 1.35 gate, and rate <= 0 freezes both the
    /// bop decay and the HUD zoom.
    #[test]
    fn camera_bop_gate_hud_decay_and_rate_zero_freeze() {
        let mut scene = default_scene();
        scene
            .step(0, &[camera_bop_row(0, 1.03, 0.06, 1.0, 0.0)], &[])
            .expect("bop row applies");
        // rate 1 / offset 0 pulses at steps 4, 8, ... (600 ms, 1200 ms, ...).
        step_empty(&mut scene, 1, 599);
        close(scene.camera.bop_multiplier, 1.0);
        step_empty(&mut scene, 600, 600);
        let factor = 0.95_f64.powf(0.06);
        close(scene.camera.bop_multiplier, 1.0 + 0.03 * factor);
        close(scene.camera_events.hud_zoom, 1.0 + 0.06 * factor);

        // rate 0 after the pulse: nothing decays, the HUD zoom freezes and the
        // gate never pulses again.
        scene
            .step(601, &[camera_bop_row(601, 1.0, 0.0, 0.0, 0.0)], &[])
            .expect("rate-zero row applies");
        let frozen_bop = scene.camera.bop_multiplier;
        let frozen_hud = scene.camera_events.hud_zoom;
        let frozen_zoom = scene.camera.zoom;
        step_empty(&mut scene, 602, 1200);
        assert_eq!(scene.camera.bop_multiplier, frozen_bop);
        assert_eq!(scene.camera_events.hud_zoom, frozen_hud);
        assert_eq!(scene.camera.zoom, frozen_zoom);
    }

    #[test]
    fn scroll_rows_tween_per_line_and_snap_to_previous_targets() {
        let mut scene = default_scene();
        scene
            .step(
                0,
                &[
                    scroll_row(0, "player", 2.0, 1.5, 8.0, Some("linear")),
                    scroll_row(0, "opponent", 2.0, 1.5, 8.0, Some("linear")),
                ],
                &[],
            )
            .expect("scroll pair applies");
        close(scene.scroll_speed_for(false), 1.5);
        close(scene.scroll_speed_for(true), 1.5);
        step_empty(&mut scene, 1, 300);
        close(scene.scroll_speed_for(false), 1.625);
        close(scene.scroll_speed_for(true), 1.625);
        step_empty(&mut scene, 301, 599);

        // The next event cancels the in-flight tween, snaps every line in
        // prevScrollTargets to 2.0, then applies the instant target.
        scene
            .step(
                600,
                &[
                    scroll_row(600, "player", 3.0, 2.0, 4.0, Some("INSTANT")),
                    scroll_row(600, "opponent", 3.0, 2.0, 4.0, Some("INSTANT")),
                ],
                &[],
            )
            .expect("snap + instant pair applies");
        close(scene.scroll_speed_for(false), 3.0);
        close(scene.scroll_speed_for(true), 3.0);
    }

    #[test]
    fn scroll_row_base_mismatch_fails_closed() {
        let mut scene = default_scene();
        let error = scene
            .step(
                0,
                &[scroll_row(0, "player", 2.0, 9.9, 4.0, Some("linear"))],
                &[],
            )
            .expect_err("base_scroll drift must fail closed");
        assert!(matches!(
            error,
            RenderError::ScrollBaseMismatch {
                tick_ms: 0,
                strumline: "player",
                expected,
                actual,
            } if (expected - 9.9).abs() < 1e-12 && (actual - 1.5).abs() < 1e-12
        ));
    }

    #[test]
    fn scroll_row_unresolved_ease_applies_nothing() {
        let mut scene = default_scene();
        scene
            .step(0, &[scroll_row(0, "player", 2.0, 1.5, 4.0, None)], &[])
            .expect("unresolved scroll row is a no-op");
        step_empty(&mut scene, 1, 100);
        close(scene.scroll_speed_for(false), 1.5);
        close(scene.scroll_speed_for(true), 1.5);
    }

    #[test]
    fn scroll_rows_are_per_line() {
        let mut scene = default_scene();
        scene
            .step(
                0,
                &[scroll_row(0, "player", 2.5, 1.5, 4.0, Some("INSTANT"))],
                &[],
            )
            .expect("player-only row applies");
        close(scene.scroll_speed_for(false), 2.5);
        close(scene.scroll_speed_for(true), 1.5);
    }

    /// `playAnimation(anim, force, force)`: force locks the character against
    /// derived callers (sing/dance) until the animation finishes; a
    /// `skipped_old` row applies nothing.
    #[test]
    fn play_animation_force_lock_suppresses_sing_and_dance() {
        let mut scene = default_scene();
        scene
            .step(0, &[play_animation_row(0, "hey", true, "bf", false)], &[])
            .expect("play_animation row applies");
        let player = scene.character_mut(Role::Player).expect("bf rig");
        assert_eq!(player.scheduler.current_name(), Some("hey"));
        assert!(player.scheduler.locked());
        assert!(!player.scheduler.play("idle", true));
        // The note-hit sing BYPASSES the lock (`ignoreExclusionPref = ['sing']`,
        // BaseCharacter.hx:174) while the dance stays locked.
        assert!(player.scheduler.on_sing(0, false));
        assert_eq!(player.scheduler.current_name(), Some("singLEFT"));
        assert!(player.scheduler.locked());
        assert!(!player.scheduler.dance(true));

        // `hey` is two frames at 24 fps; the bypassing sing (one frame,
        // 41.7 ms) also finished inside the window, so the lock reopens
        // lazily and a later unforced row plays.
        step_empty(&mut scene, 1, 84);
        assert!(
            !scene
                .character_mut(Role::Player)
                .expect("bf rig")
                .scheduler
                .locked()
        );
        scene
            .step(
                85,
                &[play_animation_row(85, "idle", false, "bf", false)],
                &[],
            )
            .expect("post-finish row applies");
        assert_eq!(
            scene
                .character_mut(Role::Player)
                .expect("bf rig")
                .scheduler
                .current_name(),
            Some("idle")
        );
    }

    #[test]
    fn play_animation_skipped_old_applies_nothing() {
        let mut scene = default_scene();
        scene
            .step(0, &[play_animation_row(0, "hey", true, "bf", true)], &[])
            .expect("skipped_old row is a no-op");
        let player = scene.character_mut(Role::Player).expect("bf rig");
        assert_eq!(player.scheduler.current_name(), Some("idle"));
        assert!(!player.scheduler.locked());
    }

    #[test]
    fn play_animation_prop_target_uses_the_exact_animation_name() {
        let mut animations = BTreeMap::new();
        animations.insert("wave".to_string(), animation(&[0, 1], true));
        let prop = LoadedStageProp {
            name: "speaker".to_string(),
            z_index: 0.0,
            position: (0.0, 0.0),
            scale: (1.0, 1.0),
            scroll: (1.0, 1.0),
            alpha: 1.0,
            angle: 0.0,
            flip_x: false,
            flip_y: false,
            visual: PropVisual::Solid {
                size: [1, 1],
                color: [0.0, 0.0, 0.0, 1.0],
            },
            animations,
            dance_every: 1.0,
            starting_animation: None,
            bopper: true,
        };
        let mut scene = test_scene_with(
            vec![
                test_character(Role::Player, (100.0, 200.0), bf_animations(), Some("idle")),
                test_character(Role::Opponent, (300.0, 400.0), BTreeMap::new(), None),
            ],
            vec![prop],
            1.5,
        );
        scene
            .step(
                0,
                &[play_animation_row(0, "wave", true, "prop:speaker", false)],
                &[],
            )
            .expect("prop row applies");
        assert_eq!(
            scene.props[0]
                .scheduler
                .as_ref()
                .expect("bopper scheduler")
                .current_name(),
            Some("wave")
        );
        // No suffix stripping on the StageProp path: `wave-alt` fails softly.
        scene
            .step(
                1,
                &[play_animation_row(
                    1,
                    "wave-alt",
                    true,
                    "prop:speaker",
                    false,
                )],
                &[],
            )
            .expect("missing prop animation is soft");
        let scheduler = scene.props[0].scheduler.as_ref().expect("bopper scheduler");
        assert_eq!(scheduler.current_name(), Some("wave"));
        assert!(scheduler.missing_animations().contains("wave-alt"));
    }

    #[test]
    fn health_icon_and_unknown_rows_do_not_touch_camera_state_without_hud() {
        // Synthetic scenes pass `hud: None`; the real load path consumes the
        // health_icon row through the shared HUD observer (M6-CONTRACT §13).
        let mut scene = default_scene();
        let before = (
            scene.camera.scroll,
            scene.camera.current_zoom,
            scene.camera_events.follow_focus,
            scene.camera_events.bop,
        );
        let health = TraceEvent::HealthIcon {
            schema: "3",
            kind: "health_icon",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            char: 1,
            id: "dad".to_string(),
            should_bop: true,
            scale: 1.0,
            flip_x: false,
            is_pixel: false,
            offsets: [0.0, 0.0],
        };
        let unknown = TraceEvent::UnknownEvent {
            schema: "3",
            kind: "unknown_event",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            event_kind: "EnableMask".to_string(),
            value: None,
        };
        scene
            .step(0, &[health, unknown], &[])
            .expect("record-only rows are no-ops");
        assert_eq!(scene.camera.scroll, before.0);
        assert_eq!(scene.camera.current_zoom, before.1);
        assert_eq!(scene.camera_events.follow_focus, before.2);
        assert_eq!(scene.camera_events.bop.rate, before.3.rate);
    }

    #[test]
    fn game_over_row_freezes_the_decay_and_hud_zoom_block() {
        // Native `subState == null` gate (PlayState.hx L1214): once the
        // GameOverSubState opens neither the bop decay nor the virtual HUD
        // zoom may keep moving (review round resolution, 2026-11-09).
        let mut scene = default_scene();
        scene
            .step(0, &[camera_bop_row(0, 1.045, 0.06, 1.0, 0.0)], &[])
            .expect("bop row applies");
        // rate 1 / offset 0 pulses at steps 4, 8, ... (600 ms, 1200 ms, ...).
        step_empty(&mut scene, 1, 599);
        assert_eq!(scene.camera.bop_multiplier, 1.0);
        step_empty(&mut scene, 600, 600);
        assert!(
            scene.camera.bop_multiplier > 1.0,
            "the pulse must have fired at step 4"
        );
        assert!(scene.camera_events.hud_zoom > DEFAULT_HUD_CAMERA_ZOOM);
        let game_over = TraceEvent::GameOver {
            schema: "3",
            kind: "game_over",
            sim: 601,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "miss",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            death_count: 1,
        };
        scene.step(601, &[game_over], &[]).expect("game over row");
        let frozen = (
            scene.camera.zoom,
            scene.camera.bop_multiplier,
            scene.camera_events.hud_zoom,
        );
        step_empty(&mut scene, 602, 1200);
        let still_frozen = (
            scene.camera.zoom,
            scene.camera.bop_multiplier,
            scene.camera_events.hud_zoom,
        );
        assert_eq!(
            frozen, still_frozen,
            "post-GameOver ticks must freeze the block"
        );
        // Tweens keep advancing in `super.update` even while a substate is
        // open (native), so the follow state is NOT asserted frozen here.
    }

    /// Native processNotes hard-reset while the opponent hold is active
    /// (PlayState.hx L2838-2852; playtest finding on the bopeebo easy
    /// 52800 d=7 l=1875 hold): the dad keeps `singLEFT` through the sustain
    /// and returns to the dance only `singTimeSeconds` after the hold ends.
    #[test]
    fn opponent_hold_sustain_keeps_the_dad_singing() {
        let animations = BTreeMap::from([
            ("idle".to_string(), animation(&[0, 1], true)),
            ("singLEFT".to_string(), animation(&[1], false)),
        ]);
        let mut scene = test_scene_with_notes(
            vec![
                test_character(Role::Player, (100.0, 200.0), bf_animations(), Some("idle")),
                test_character(Role::Opponent, (300.0, 400.0), animations, None),
            ],
            Vec::new(),
            1.5,
            vec![LoadedNote {
                time: 0.0,
                direction: 0,
                sustain_ms: 3000.0,
                opponent: true,
                note_id: Some(0),
            }],
        );
        let opponent_hit = TraceEvent::OpponentHit {
            schema: "3",
            kind: "opponent_hit",
            sim: 0,
            input_ms: 0.0,
            input_time_us: 0.0,
            note_id: 0,
            note_direction: 0,
            judgement: "sick",
            raw_note_diff: 0.0,
            note_diff_int: 0,
            note_kind: None,
        };
        scene
            .step(0, &[opponent_hit], &[])
            .expect("opponent hit row");
        assert_eq!(
            scene
                .characters
                .iter()
                .find(|character| character.role == Role::Opponent)
                .and_then(|dad| dad.scheduler.current_name()),
            Some("singLEFT")
        );
        // Through the 3000 ms sustain (default sing time 1200 ms) the sing
        // must still be current at 4000 ms...
        step_empty(&mut scene, 1, 4000);
        assert_eq!(
            scene
                .characters
                .iter()
                .find(|character| character.role == Role::Opponent)
                .and_then(|dad| dad.scheduler.current_name()),
            Some("singLEFT")
        );
        // ...and by 5000 ms the >1.2 s post-hold accrual has stopped it.
        step_empty(&mut scene, 4001, 5000);
        assert_ne!(
            scene
                .characters
                .iter()
                .find(|character| character.role == Role::Opponent)
                .and_then(|dad| dad.scheduler.current_name()),
            Some("singLEFT")
        );
    }

    #[test]
    fn sim_and_scene_agree_on_the_sampled_focus_points() {
        // OXIDE_ASSETS_ROOT guard: sim `world_focus` rows must equal the
        // scene's own rig targets for the two established families
        // (review finding 2: the §12.6 finding-3 helper factoring is
        // deferred to a later slice; this equivalence gate catches algebra
        // drift between the headless sim and the render loader).
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let root = std::path::PathBuf::from(root);
        for chart_id in ["bopeebo", "senpai"] {
            let loaded = super::super::load::load_slice_for_song(&root, chart_id, "normal")
                .unwrap_or_else(|error| panic!("{chart_id}: load failed: {error}"));
            let scene = Scene::new(loaded).expect("scene builds").0;
            let rig_for = |kind: &str| -> Option<[f64; 2]> {
                let role = match kind {
                    "bf" => Role::Player,
                    "dad" => Role::Opponent,
                    _ => return None,
                };
                scene
                    .characters
                    .iter()
                    .find(|character| character.role == role)
                    .map(|character| character.camera_focus)
            };
            let mut sim =
                crate::slice::GameplaySimState::build_chart_events(&root, chart_id, "normal")
                    .unwrap_or_else(|error| panic!("{chart_id}: sim build failed: {error}"));
            let mut emitted = false;
            for tick in 0..60_000u64 {
                let rows = sim.step(tick, &[]);
                for row in &rows {
                    let TraceEvent::CameraFocus {
                        target_kind,
                        x,
                        y,
                        world_focus,
                        ..
                    } = row
                    else {
                        continue;
                    };
                    if *target_kind != "bf" && *target_kind != "dad" {
                        continue;
                    }
                    let Some(rig) = rig_for(target_kind) else {
                        continue;
                    };
                    emitted = true;
                    // The handler ADDS the literal x/y on top of the focus
                    // point (FocusCameraSongEvent.hx L113-134).
                    let want = [x + rig[0], y + rig[1]];
                    assert_eq!(
                        *world_focus, want,
                        "{chart_id}: world_focus row drifts from the scene rig target"
                    );
                }
            }
            assert!(
                emitted,
                "{chart_id}: expected at least one resolvable camera_focus row"
            );
        }
    }
    /// Asset-gated 6D integration: the real bopeebo scene carries the HUD
    /// after every stage/overlay layer, the character healthIcon data drives
    /// both icons, and `Scene::draw` emits the HUD quads.
    /// Review requested check: a synthetic SetHealthIcon row through the
    /// REAL loaded Scene re-configures the icon in place (car 1 id reload
    /// with the corpus icon inventory), unknown chars are no-ops, and an
    /// unknown corpus id fails closed at the row.
    #[test]
    fn set_health_icon_row_reconfigures_the_loaded_scene_icon() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let loaded = super::super::load::load_slice_for_song(
            std::path::Path::new(&root),
            "bopeebo",
            "normal",
        )
        .expect("bopeebo loads");
        let (mut scene, _pages) = Scene::new(loaded).expect("scene builds");
        assert!(scene.hud.is_some(), "the real load path carries the HUD");
        let row = |tick: u64, char: i64, id: &str| TraceEvent::HealthIcon {
            schema: "3",
            kind: "health_icon",
            sim: tick,
            input_ms: 0.0,
            input_time_us: 0.0,
            char,
            id: id.to_string(),
            should_bop: true,
            scale: 1.0,
            flip_x: true,
            is_pixel: false,
            offsets: [0.0, 0.0],
        };
        scene
            .step(0, &[row(0, 1, "tankman-bloody")], &[])
            .expect("row applies");
        let icon = scene
            .hud
            .as_ref()
            .and_then(|hud| hud.state.icons[1].as_ref())
            .expect("opponent icon");
        assert_eq!(icon.id, "tankman-bloody");
        assert!(icon.flip_x, "the row's flipX applies directly (no XOR)");
        // Unknown char: native warning branch, no state change.
        let before = scene
            .hud
            .as_ref()
            .and_then(|hud| hud.state.icons[1].as_ref())
            .map(|icon| (icon.id.clone(), icon.flip_x));
        scene
            .step(1, &[row(1, 5, "bf")], &[])
            .expect("unknown char is a no-op row");
        let after = scene
            .hud
            .as_ref()
            .and_then(|hud| hud.state.icons[1].as_ref())
            .map(|icon| (icon.id.clone(), icon.flip_x));
        assert_eq!(before, after);
        // Unknown id outside the icon inventory fails closed ([D] mod surface).
        let error = scene
            .step(2, &[row(2, 1, "mod-only-icon")], &[])
            .unwrap_err();
        assert!(error.to_string().contains("mod-only-icon"));
    }

    #[test]
    fn shipped_hud_composes_after_the_scene_layers() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let loaded = super::super::load::load_slice_for_song(
            std::path::Path::new(&root),
            "bopeebo",
            "normal",
        )
        .expect("bopeebo loads");
        let (mut scene, _pages) = Scene::new(loaded).expect("scene builds");
        let hud_start = scene.hud_start;
        let specs_len = scene.layer_specs().len();
        assert!(specs_len > hud_start + 5, "HUD layer group appended");
        let hud = scene.hud.as_ref().expect("HUD payload");
        let player = hud.state.icons[0].as_ref().expect("player icon");
        let opponent = hud.state.icons[1].as_ref().expect("opponent icon");
        assert_eq!(player.id, "bf");
        assert_eq!(opponent.id, "dad");
        // `initHealthIcon(false)` XOR-flips the player icon.
        assert!(player.flip_x);
        assert!(!opponent.flip_x);

        for tick in 0..10 {
            scene.step(tick, &[], &[]).expect("scene tick");
        }
        let layers = scene.draw().expect("scene draws");
        assert_eq!(layers.len(), specs_len);
        let hud = scene.hud.as_ref().expect("HUD payload");
        assert_eq!(layers[hud_start + hud.layers.bar_bg].len(), 1);
        assert_eq!(layers[hud_start + hud.layers.bar_empty].len(), 1);
        assert_eq!(layers[hud_start + hud.layers.bar_fill].len(), 1);
        // "Score: 0" is seven vcr-bmp glyphs: one white fill each and the
        // eight-outline border pass (7 * 8) in the layer before it.
        assert_eq!(layers[hud_start + hud.layers.score_fill].len(), 7);
        assert_eq!(layers[hud_start + hud.layers.score_outline].len(), 56);
        let non_empty = layers[hud_start..]
            .iter()
            .filter(|layer| !layer.is_empty())
            .count();
        assert!(non_empty >= 7, "bar/score/icons contribute quads");
    }

    fn dialogue_trigger_row(sim: u64) -> TraceEvent {
        TraceEvent::Choreography {
            schema: "3",
            kind: "choreography",
            sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            event: "dialogue:test".to_string(),
            arg: None,
        }
    }

    /// F4: two advance keys in one frame are one native boolean check, and
    /// the advance runs after `processSongEvents` (a same-tick dialogue
    /// trigger starts first).
    #[test]
    fn dialogue_advance_is_coalesced_and_applied_after_event_rows() {
        // Two Advance inputs in one step: one advance moves from the first
        // empty line to the second; an uncoalesced second advance would open
        // entry 1 instead.
        let mut scene = test_scene_with_dialogue(synthetic_dialogue());
        scene
            .step(0, &[dialogue_trigger_row(0)], &[])
            .expect("trigger tick");
        assert!(scene.conversation_active());
        let advance = InputEvent::Advance { sim_ms: 1 };
        scene
            .step(1, &[], &[advance.clone(), advance])
            .expect("advance tick");
        let conversation = scene.conversation.as_ref().expect("conversation");
        assert_eq!(
            conversation.advance_count(),
            1,
            "two Advance events are one advance"
        );
        assert_eq!(conversation.core().entry_index(), 0);
        assert_eq!(
            conversation.core().line_index(),
            1,
            "one advance reached the second empty line"
        );

        // Same-tick trigger + advance: the event row starts the conversation
        // first, so the advance is counted (an advance processed before the
        // rows would hit an inactive runtime and not be applied).
        let mut scene = test_scene_with_dialogue(synthetic_dialogue());
        scene
            .step(
                0,
                &[dialogue_trigger_row(0)],
                &[InputEvent::Advance { sim_ms: 0 }],
            )
            .expect("trigger + advance tick");
        let conversation = scene.conversation.as_ref().expect("conversation");
        assert!(conversation.active(), "the same-tick trigger started");
        assert_eq!(
            conversation.advance_count(),
            1,
            "the advance applies after the trigger starts"
        );
        assert_eq!(conversation.core().phase(), ConversationPhase::Idle);
    }

    /// F8: the 7.1 s `Can Shot` prop has its own z-300 stage item/layer and
    /// receives a quad after a kickcan + the cutscene `CanShotProp` action.
    #[test]
    #[ignore = "requires OXIDE_ASSETS_ROOT; run explicitly for the weekend-1 prop draw"]
    fn weekend1_can_shot_prop_draws_on_its_z300_layer() {
        let Some(root) = std::env::var_os("OXIDE_ASSETS_ROOT") else {
            return;
        };
        let loaded =
            super::super::load::load_slice_for_song(std::path::Path::new(&root), "2hot", "easy")
                .expect("2hot loads");
        let (mut scene, _pages) = Scene::new(loaded).expect("2hot scene builds");

        let can_shot_position = scene
            .stage_items
            .iter()
            .position(|item| matches!(item.kind, StageItemKind::Weekend1CanShotProp))
            .expect("the Can Shot stage item is registered");
        let can_shot_layer = scene.stage_items[can_shot_position].layers[0].1;
        assert_eq!(
            scene.layer_specs[can_shot_layer].color,
            [1.0, 1.0, 1.0, 1.0],
            "the prop layer is untinted white"
        );
        assert!(
            scene.layer_specs[can_shot_layer].page.is_some(),
            "the prop layer is bound to the spraycan atlas page"
        );

        // z-sort: every stage prop below 300 draws before the prop, every
        // prop above 300 draws after it.
        for (index, item) in scene.stage_items.iter().enumerate() {
            let StageItemKind::Prop(prop_index) = item.kind else {
                continue;
            };
            let z = scene.props[prop_index].prop.z_index;
            if z < 300.0 {
                assert!(
                    index < can_shot_position,
                    "prop z {z} sorts below the Can Shot"
                );
            } else if z > 300.0 {
                assert!(
                    index > can_shot_position,
                    "prop z {z} sorts above the Can Shot"
                );
            }
        }

        scene
            .weekend1
            .as_mut()
            .expect("weekend-1 payload")
            .on_hit("weekend-1-kickcan", 0.0);
        // The 7.1 s cutscene step is the native `Can Shot` trigger.
        scene
            .apply_cutscene_action(super::super::choreography::CutsceneAction::CanShotProp)
            .expect("CanShotProp action applies");
        assert!(
            scene
                .weekend1
                .as_ref()
                .expect("weekend-1 payload")
                .can_shot_prop_active(),
            "the 7.1 s prop is alive"
        );

        let layers = scene.draw().expect("scene draws");
        assert!(
            !layers[can_shot_layer].is_empty(),
            "the Can Shot prop layer receives its atlas quad"
        );
    }
}
