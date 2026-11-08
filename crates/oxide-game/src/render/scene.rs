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
use super::geometry::{
    self, BACKGROUND_ALPHA, BACKGROUND_WIDTH, Camera, GAME_HEIGHT, HALF_HEIGHT, HALF_WIDTH,
    OPPONENT_STRUMLINE_X, PLAYER_STRUMLINE_X, STRUMLINE_Y, SUSTAIN_CAP_BOTTOM_CLIP,
    SUSTAIN_CAP_HEIGHT, TICK_SECONDS, Vec2, hold_column_uv, hold_columns, note_top_left,
    receptor_top_left, render_distance_ms, sustain_height, sustain_held_top, sustain_top_left_x,
};
use super::load::{
    LoadedCharacter, LoadedNote, LoadedNoteStyle, LoadedSlice, LoadedStageProp, PageImage, Role,
    SpriteFrame,
};
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

#[derive(Debug, Clone, Copy)]
enum StageItem {
    Prop(usize),
    Character(usize),
}

/// Per-character runtime state: scheduler plus the static atlas placement.
struct CharacterRig {
    role: Role,
    id: String,
    scheduler: CharacterScheduler,
    stage: oxide_atlas::AnimateStage,
    bounds: [f64; 4],
    data_offsets: (f64, f64),
    scale: f64,
    flip_x: bool,
    position: Vec2,
    scroll: Vec2,
    camera_focus: Vec2,
    z_index: f64,
    page: usize,
    page_size: [f64; 2],
}

impl CharacterRig {
    fn from_loaded(loaded: LoadedCharacter) -> Self {
        let mut scheduler = CharacterScheduler::new(
            loaded.animations,
            loaded.dance_every,
            loaded.sing_time_steps,
            loaded.role == Role::Player,
        );
        // Native characters start on idle; girlfriend has no idle animation,
        // so show her first dance pose without consuming the first alternation.
        if scheduler.has("idle") {
            scheduler.play("idle", true);
        } else if scheduler.has("danceLeft") {
            scheduler.play("danceLeft", true);
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
            stage: loaded.stage,
            bounds,
            data_offsets: loaded.data_offsets,
            scale,
            flip_x: loaded.flip_x,
            position,
            scroll: [loaded.slot.scroll.0, loaded.slot.scroll.1],
            camera_focus,
            z_index: loaded.slot.z_index,
            page: loaded.page,
            page_size: loaded.page_size,
        }
    }

    fn draw(&self, camera: &Camera) -> Result<Vec<Quad>, RenderError> {
        let Some(timeline_frame) = self.scheduler.current_timeline_frame() else {
            return Ok(Vec::new());
        };
        let quads = self
            .stage
            .resolve_main_frame(timeline_frame)
            .map_err(|source| RenderError::AtlasStage {
                path: format!("character {} ({})", self.id, self.role.label()),
                source,
            })?;
        let bw = self.bounds[2];
        let scale = self.scale;
        let top_left = [
            self.position[0] - bw * scale / 2.0,
            self.position[1] - self.bounds[3] * scale,
        ];
        let animation_offsets = self
            .scheduler
            .current_animation()
            .and_then(|animation| animation.offsets)
            .unwrap_or(self.data_offsets);
        let shift = [
            -(animation_offsets.0 - self.data_offsets.0) * scale,
            -(animation_offsets.1 - self.data_offsets.1) * scale,
        ];

        let projection = camera_projection_matrix(camera, self.scroll);
        let world = character_world_matrix(self.bounds, scale, self.flip_x, top_left, shift);
        let mut out = Vec::with_capacity(quads.len());
        for quad in &quads {
            let [_, _, width, height] = quad.atlas_rect;
            // Frozen composition: gameQuad(p) = spriteTopLeft + (p - boundsTopLeft),
            // with the Bopper shift already folded into `world`, and the
            // atlas quad matrix carrying sprite-local -> atlas coordinates.
            let matrix = concat(projection, concat(world, quad.matrix));
            out.push(quad_from_matrix(
                matrix,
                width as f64,
                height as f64,
                quad_uv(quad.atlas_rect, self.page_size),
            ));
        }
        Ok(out)
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
    direction: usize,
    end_ms: f64,
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
    props: Vec<LoadedStageProp>,
    characters: Vec<CharacterRig>,
    stage_items: Vec<StageItem>,
    layer_specs: Vec<LayerSpec>,
    note_style: LoadedNoteStyle,
    notes: Vec<NoteState>,
    player_note_indices: Vec<usize>,
    scroll_speed: f64,
    sim_ms: f64,
    last_tick: Option<u64>,
    dead: bool,
    player_line: LineState,
    opponent_line: LineState,
    player_holds: [Option<PlayerHold>; 4],
    opponent_holds: [Option<OpponentHold>; 4],
    follow_focus: Vec2,
}

impl Scene {
    /// Builds the scene and returns the decoded pages the harness must insert
    /// into Bevy's `Assets<Image>`.
    pub(crate) fn new(loaded: LoadedSlice) -> Result<(Self, Vec<PageImage>), RenderError> {
        let LoadedSlice {
            pages,
            stage_zoom,
            props,
            characters: loaded_characters,
            note_style,
            notes,
            scroll_speed,
            conductor_changes,
        } = loaded;
        let mut characters = Vec::with_capacity(3);
        for character in loaded_characters {
            characters.push(CharacterRig::from_loaded(character));
        }
        let opponent = Role::Opponent.index();
        let follow_focus = characters
            .get(opponent)
            .ok_or_else(|| RenderError::Unsupported {
                detail: "opponent character was not loaded".to_string(),
            })?
            .camera_focus;
        let mut camera = Camera::new(stage_zoom);
        camera.focus_on(follow_focus);

        let mut stage_items: Vec<(f64, usize, StageItem)> = Vec::new();
        for (index, prop) in props.iter().enumerate() {
            stage_items.push((prop.z_index, stage_items.len(), StageItem::Prop(index)));
        }
        for (index, character) in characters.iter().enumerate() {
            stage_items.push((
                character.z_index,
                stage_items.len(),
                StageItem::Character(index),
            ));
        }
        stage_items.sort_by(|left, right| {
            left.0
                .total_cmp(&right.0)
                .then_with(|| left.1.cmp(&right.1))
        });
        let stage_items: Vec<StageItem> =
            stage_items.into_iter().map(|(_, _, item)| item).collect();

        let mut layer_specs = Vec::with_capacity(stage_items.len() + 4);
        for item in &stage_items {
            match item {
                StageItem::Prop(index) => {
                    let prop = &props[*index];
                    layer_specs.push(LayerSpec {
                        page: Some(prop.page),
                        color: [1.0, 1.0, 1.0, prop.alpha as f32],
                    });
                }
                StageItem::Character(index) => {
                    let character = &characters[*index];
                    layer_specs.push(LayerSpec {
                        page: Some(character.page),
                        color: [1.0, 1.0, 1.0, 1.0],
                    });
                }
            }
        }
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

        let player_note_indices = notes
            .iter()
            .enumerate()
            .filter_map(|(index, note)| note.player_id.map(|_| index))
            .collect();

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
                scroll_speed,
                sim_ms: 0.0,
                last_tick: None,
                dead: false,
                player_line: LineState::default(),
                opponent_line: LineState::default(),
                player_holds: [None, None, None, None],
                opponent_holds: [None, None, None, None],
                follow_focus,
            },
            pages,
        ))
    }

    pub(crate) fn layer_specs(&self) -> &[LayerSpec] {
        &self.layer_specs
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
                InputEvent::Press { sim_ms, .. } | InputEvent::Release { sim_ms, .. } => *sim_ms,
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
        let dispatches = self.conductor.update(Some(self.sim_ms), false, false, 0.0);
        if self.dead {
            self.camera.decay(TICK_SECONDS);
            return Ok(());
        }
        for dispatch in dispatches {
            if let Dispatch::Step(step) = dispatch {
                self.camera.on_step(step);
                for character in &mut self.characters {
                    character.scheduler.on_step(step);
                }
            }
        }

        // Per-frame order from RENDER-BEHAVIOR §6: camera decay right after
        // the conductor update, then strumline/input, then processNotes.
        self.camera.decay(TICK_SECONDS);

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
            }
        }

        for event in events {
            self.process_event(event)?;
        }

        self.trigger_opponent_notes();
        // Strumline.hx:653-669: an opponent hold that reached its end no
        // longer contributes confirm-hold; prune before rendering so the
        // opponent receptor returns to static.
        prune_opponent_holds(&mut self.opponent_holds, self.sim_ms);

        let holding = self.player_holds.iter().any(Option::is_some);
        if let Some(player) = self.characters.get_mut(Role::Player.index()) {
            player.scheduler.set_holding(holding);
        }
        let step_length_ms = self.conductor.step_length_ms();
        for character in &mut self.characters {
            character.scheduler.update(TICK_SECONDS, step_length_ms);
        }
        self.camera.follow(self.follow_focus, TICK_SECONDS);
        Ok(())
    }

    fn process_event(&mut self, event: &TraceEvent) -> Result<(), RenderError> {
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
                    if let Some(player) = self.characters.get_mut(Role::Player.index()) {
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
                    && let Some(player) = self.characters.get_mut(Role::Player.index())
                {
                    player.scheduler.on_sing(direction, true);
                }
            }
            TraceEvent::GhostPress { note_direction, .. } => {
                // Engine decision: the ghost-score path plays the miss form,
                // mirroring the native miss-note reaction.
                if let Ok(direction) = usize::try_from(*note_direction)
                    && direction < 4
                    && let Some(player) = self.characters.get_mut(Role::Player.index())
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
                let note = self.notes[index].note;
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
        }
        Ok(())
    }

    fn trigger_opponent_notes(&mut self) {
        let now = self.sim_ms;
        for index in 0..self.notes.len() {
            let note = self.notes[index].note;
            if !note.opponent || self.notes[index].hit || note.time > now {
                continue;
            }
            self.notes[index].hit = true;
            let direction = note.direction;
            if let Some(opponent) = self.characters.get_mut(Role::Opponent.index()) {
                opponent.scheduler.on_sing(direction, false);
            }
            self.opponent_line.confirm_opponent(direction, now);
            if note.sustain_ms > 0.0 {
                self.opponent_holds[direction] = Some(OpponentHold {
                    direction,
                    end_ms: note.time + note.sustain_ms,
                });
            }
        }
    }

    fn player_note_index(&self, note_id: u32, tick_ms: u64) -> Result<usize, RenderError> {
        self.player_note_indices
            .get(note_id as usize)
            .copied()
            .ok_or(RenderError::UnknownEventNote { tick_ms, note_id })
    }

    /// Composes all draw layers in painter order.
    pub(crate) fn draw(&self) -> Result<Vec<Vec<Quad>>, RenderError> {
        let mut layers: Vec<Vec<Quad>> = Vec::with_capacity(self.layer_specs.len());
        for item in &self.stage_items {
            match item {
                StageItem::Prop(index) => {
                    layers.push(self.draw_prop(&self.props[*index]));
                }
                StageItem::Character(index) => {
                    layers.push(self.characters[*index].draw(&self.camera)?);
                }
            }
        }
        layers.push(self.draw_backgrounds());
        layers.push(self.draw_receptors());
        layers.push(self.draw_holds());
        layers.push(self.draw_notes());
        Ok(layers)
    }

    fn draw_prop(&self, prop: &LoadedStageProp) -> Vec<Quad> {
        let projection = camera_projection_matrix(&self.camera, [prop.scroll.0, prop.scroll.1]);
        let sx = prop.scale.0;
        let sy = prop.scale.1;
        let width = prop.image_size[0];
        let height = prop.image_size[1];
        let mut world = [sx, 0.0, 0.0, sy, prop.position.0, prop.position.1];
        if prop.flip_x {
            world[0] = -sx;
            world[4] = prop.position.0 + width * sx;
        }
        if prop.flip_y {
            world[3] = -sy;
            world[5] = prop.position.1 + height * sy;
        }
        let matrix = concat(projection, world);
        vec![quad_from_matrix(
            matrix,
            width,
            height,
            [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        )]
    }

    fn draw_backgrounds(&self) -> Vec<Quad> {
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
                quads.push(sprite_frame_quad(
                    frame,
                    [base[0] + self.note_style.receptor_offsets.0, base[1]],
                    scale,
                    self.note_style.receptor_page_size,
                ));
            }
        }
        quads
    }

    fn draw_holds(&self) -> Vec<Quad> {
        let mut quads = Vec::new();
        let speed = self.scroll_speed;
        let held_top = sustain_held_top(STRUMLINE_Y);
        for direction in 0..4 {
            if let Some(hold) = &self.player_holds[direction] {
                self.push_hold(
                    &mut quads,
                    PLAYER_STRUMLINE_X,
                    direction,
                    held_top,
                    sustain_height(hold.remaining_ms, speed),
                );
            }
            if let Some(hold) = &self.opponent_holds[direction] {
                let remaining = (hold.end_ms - self.sim_ms).max(0.0);
                self.push_hold(
                    &mut quads,
                    OPPONENT_STRUMLINE_X,
                    hold.direction,
                    held_top,
                    sustain_height(remaining, speed),
                );
            }
        }
        // Dropped player holds are completely hidden (Strumline.hx:634-640);
        // no dropped-trail quads are emitted.
        // Approaching hold tails follow their scrolling note heads. Notes
        // beyond the native spawn window are not built.
        let render_distance = render_distance_ms(speed);
        for note in &self.notes {
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
            let frame = self.note_style.note_frames[note.note.direction];
            let width = frame.dest_size[0] as f64 * self.note_style.note_scale;
            let height = frame.dest_size[1] as f64 * self.note_style.note_scale;
            let top = note_top_left(
                line_x,
                note.note.direction,
                width,
                self.sim_ms,
                note.note.time,
                speed,
            );
            self.push_hold(
                &mut quads,
                line_x,
                note.note.direction,
                top[1] + height,
                sustain_height(note.note.sustain_ms, speed),
            );
        }
        quads
    }

    fn push_hold(&self, out: &mut Vec<Quad>, line_x: f64, direction: usize, top: f64, height: f64) {
        if !visible(top, height) || height <= 0.0 {
            return;
        }
        let (body_column, cap_column) = hold_columns(direction);
        // Observed `graphicWidth = (graphic.width / 8) * zoom =
        // (416/8) * 0.7 = 36.4` Dx (the pre-zoom 52 was a transcription error).
        let width = self.note_style.hold_width;
        let x = sustain_top_left_x(line_x, direction, width);
        let cap_height = SUSTAIN_CAP_HEIGHT.min(height);
        let body_height = (height - cap_height).max(0.0);
        if body_height > 0.0 {
            let (uv_min, uv_max) = hold_column_uv(body_column);
            let uv = [
                [uv_min[0], uv_min[1]],
                [uv_max[0], uv_min[1]],
                [uv_max[0], uv_max[1]],
                [uv_min[0], uv_max[1]],
            ];
            out.push(quad_from_matrix(
                [1.0, 0.0, 0.0, 1.0, x, top],
                width,
                body_height,
                uv,
            ));
        }
        let (uv_min, uv_max) = hold_column_uv(cap_column);
        let clip_fraction = if height >= SUSTAIN_CAP_HEIGHT {
            SUSTAIN_CAP_BOTTOM_CLIP
        } else {
            SUSTAIN_CAP_BOTTOM_CLIP * (cap_height / SUSTAIN_CAP_HEIGHT)
        };
        let cap_v1 = uv_min[1] + (uv_max[1] - uv_min[1]) * clip_fraction;
        let uv = [
            [uv_min[0], uv_min[1]],
            [uv_max[0], uv_min[1]],
            [uv_max[0], cap_v1],
            [uv_min[0], cap_v1],
        ];
        out.push(quad_from_matrix(
            [1.0, 0.0, 0.0, 1.0, x, top + body_height],
            width,
            cap_height,
            uv,
        ));
    }

    fn draw_notes(&self) -> Vec<Quad> {
        let mut quads = Vec::new();
        let scale = self.note_style.note_scale;
        let render_distance = render_distance_ms(self.scroll_speed);
        for note in &self.notes {
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
                self.scroll_speed,
            );
            if !visible(top[1], height) {
                continue;
            }
            quads.push(sprite_frame_quad(
                &frame,
                top,
                scale,
                self.note_style.note_page_size,
            ));
        }
        quads
    }
}

/// World transform for one character frame.
///
/// Frozen composition (RENDER-BEHAVIOR §4):
/// `gameQuad(p) = spriteTopLeft + (p - boundsTopLeft)` at scale 1, with the
/// sprite scale applied to the whole bounds-relative vector and the bf
/// whole-sprite mirror applied inside the bounds. The Bopper animation-offset
/// shift is already folded into `sprite_top_left` by the caller:
/// `-(animOffsets - globalOffsets) * scale`.
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
    frame_top_left: Vec2,
    scale: f64,
    page_size: [f64; 2],
) -> Quad {
    let left = frame_top_left[0] + frame.dest_offset[0] as f64 * scale;
    let top = frame_top_left[1] + frame.dest_offset[1] as f64 * scale;
    let width = frame.rect[2] as f64 * scale;
    let height = frame.rect[3] as f64 * scale;
    quad_from_matrix(
        [scale, 0.0, 0.0, scale, left, top],
        width,
        height,
        quad_uv(frame.rect, page_size),
    )
}

fn quad_uv(rect: [i32; 4], page_size: [f64; 2]) -> [[f64; 2]; 4] {
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
fn concat(parent: [f64; 6], local: [f64; 6]) -> [f64; 6] {
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
fn camera_projection_matrix(camera: &Camera, scroll_factor: Vec2) -> [f64; 6] {
    let tx = HALF_WIDTH - camera.zoom * (camera.scroll[0] * scroll_factor[0] + HALF_WIDTH);
    let ty = HALF_HEIGHT - camera.zoom * (camera.scroll[1] * scroll_factor[1] + HALF_HEIGHT);
    [camera.zoom, 0.0, 0.0, camera.zoom, tx, ty]
}

/// Builds a quad from a local `(0..width, 0..height)` rectangle transformed by
/// `matrix` into game-screen coordinates (Flixel y-down). The final Bevy
/// world position mirrors the y axis around the viewport center.
fn quad_from_matrix(matrix: [f64; 6], width: f64, height: f64, uv: [[f64; 2]; 4]) -> Quad {
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
        | TraceEvent::GameOver { sim, .. } => *sim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        holds[0] = Some(OpponentHold {
            direction: 0,
            end_ms: 1_000.0,
        });
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
}
