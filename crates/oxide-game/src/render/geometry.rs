//! Pure `f64` scene geometry for the M5 phase-2b slice renderer.
//!
//! Every value here comes from `tools/oracle/RENDER-BEHAVIOR.md` §2/§3 (the
//! OBSERVED/DERIVED notes there) or from an engine decision documented next to
//! the constant. The renderer keeps this math in `f64` (Haxe `Float` is a
//! double); only the final Bevy mesh vertices are converted to `f32`.

/// Flixel game width (`Main.hx:33-34`, RENDER-BEHAVIOR §2).
pub(crate) const GAME_WIDTH: f64 = 1280.0;
/// Flixel game height (`Main.hx:33-34`, RENDER-BEHAVIOR §2).
pub(crate) const GAME_HEIGHT: f64 = 720.0;
/// Half the game viewport, the projection pivot.
pub(crate) const HALF_WIDTH: f64 = GAME_WIDTH / 2.0;
/// Half the game viewport, the projection pivot.
pub(crate) const HALF_HEIGHT: f64 = GAME_HEIGHT / 2.0;

/// `Constants.STRUMLINE_SIZE` (RENDER-BEHAVIOR §3).
pub(crate) const STRUMLINE_SIZE: f64 = 104.0;
/// `Constants.NOTE_SPACING = STRUMLINE_SIZE + 8` (RENDER-BEHAVIOR §3).
pub(crate) const NOTE_SPACING: f64 = STRUMLINE_SIZE + 8.0;
/// `INITIAL_OFFSET = -0.275 * STRUMLINE_SIZE` for upscroll (RENDER-BEHAVIOR §3).
pub(crate) const INITIAL_OFFSET: f64 = -0.275 * STRUMLINE_SIZE;
/// `Constants.NUDGE` (RENDER-BEHAVIOR §3).
pub(crate) const NUDGE: f64 = 2.0;
/// `Constants.PIXELS_PER_MS` (RENDER-BEHAVIOR §3).
pub(crate) const PIXELS_PER_MS: f64 = 0.45;
/// `STRUMLINE_Y_OFFSET` (RENDER-BEHAVIOR §3).
pub(crate) const STRUMLINE_Y: f64 = 24.0;
/// `Constants.STRUMLINE_X_OFFSET` for the opponent line (RENDER-BEHAVIOR §3).
pub(crate) const OPPONENT_STRUMLINE_X: f64 = 48.0;
/// Player strumline x (RENDER-BEHAVIOR §3).
pub(crate) const PLAYER_STRUMLINE_X: f64 = 688.0;
/// `BACKGROUND_PAD` (RENDER-BEHAVIOR §3).
pub(crate) const BACKGROUND_PAD: f64 = 16.0;
/// `4 * NOTE_SPACING + 2 * BACKGROUND_PAD` (RENDER-BEHAVIOR §3).
pub(crate) const BACKGROUND_WIDTH: f64 = 4.0 * NOTE_SPACING + 2.0 * BACKGROUND_PAD;
/// Observed default underlay opacity: `Preferences.strumlineBackgroundOpacity
/// ?? 0` (Preferences.hx:462-464) and `background.alpha = opacity / 100`
/// (Strumline.hx:249/300) — 0.0 at first launch with no save file. The
/// phase-2b renderer's 1.0 was an engine invention (ADR-0020 addendum 4).
pub(crate) const BACKGROUND_ALPHA: f64 = 0.0;
/// `NOTE_hold_assets` is a 416x87 sheet with 8 columns (RENDER-BEHAVIOR §3).
pub(crate) const HOLD_TEXTURE_COLUMNS: usize = 8;
/// `NOTE_hold_assets` width (RENDER-BEHAVIOR §3).
pub(crate) const HOLD_TEXTURE_WIDTH: f64 = 416.0;
/// `NOTE_hold_assets` height (RENDER-BEHAVIOR §3).
pub(crate) const HOLD_TEXTURE_HEIGHT: f64 = 87.0;
/// SustainTrail `bottomHeight` (`graphic.height * zoom * endOffset` =
/// `87 * 0.7 * 0.5 = 30.45`): the distance from `graphicHeight` down to the
/// cap-vertex top — NOT the full rendered cap height (SustainTrail.hx:324).
/// The pre-fix name `SUSTAIN_CAP_HEIGHT` blamed the lower value; renamed
/// 2026-11-08 with the user surgical fix round.
pub(crate) const SUSTAIN_BOTTOM_HEIGHT: f64 = HOLD_TEXTURE_HEIGHT * 0.7 * 0.5;
/// SustainTrail `extraCapTail` (`graphic.height * zoom * (bottomClip -
/// endOffset)` = `87 * 0.7 * 0.4 = 24.36`); the cap bottom sits this far
/// BELOW `graphicHeight` (SustainTrail.hx:391-392).
pub(crate) const SUSTAIN_EXTRA_CAP_TAIL: f64 =
    HOLD_TEXTURE_HEIGHT * 0.7 * (SUSTAIN_CAP_BOTTOM_CLIP - 0.5);
/// End-cap/un-hit texture bottom clip bound (RENDER-BEHAVIOR §3).
pub(crate) const SUSTAIN_CAP_BOTTOM_CLIP: f64 = 0.9;
/// Un-hit hold visibility threshold (`clipHeight <= 0.1` hides,
/// SustainTrail.hx:310-318).
pub(crate) const SUSTAIN_HIDE_CLIP: f64 = 0.1;
/// Held hold visibility threshold in remaining ms (`sustainLength <= 10`
/// hides, Strumline.hx:729-733).
pub(crate) const SUSTAIN_HELD_HIDE_REMAINING: f64 = 10.0;

/// `Constants.DEFAULT_CAMERA_ZOOM` (RENDER-BEHAVIOR §2); the stage default is
/// `defaultZoom * 1.05`.
pub(crate) const DEFAULT_CAMERA_ZOOM: f64 = 1.0;
/// `cameraZoom ?? FlxCamera.defaultZoom * 1.05` (RENDER-BEHAVIOR §2).
pub(crate) const DEFAULT_STAGE_ZOOM: f64 = DEFAULT_CAMERA_ZOOM * 1.05;
/// `Constants.DEFAULT_BOP_INTENSITY` (RENDER-BEHAVIOR §2).
pub(crate) const CAMERA_BOP_INTENSITY: f64 = 1.015;
/// `Constants.DEFAULT_ZOOM_RATE` (RENDER-BEHAVIOR §2).
pub(crate) const CAMERA_ZOOM_RATE: i32 = 4;
/// `cameraZoomRateOffset` default (RENDER-BEHAVIOR §2).
pub(crate) const CAMERA_ZOOM_RATE_OFFSET: i32 = 1;
/// `Constants.DEFAULT_CAMERA_FOLLOW_RATE` (RENDER-BEHAVIOR §2).
pub(crate) const CAMERA_FOLLOW_LERP: f64 = 0.04;
/// `Constants.STEPS_PER_BEAT`/`STEPS_PER_BEAT` (M4 conductor, RENDER §5).
pub(crate) const DANCE_STEPS_PER_BEAT: f64 = 4.0;
/// Engine decision: the game steps the scene on a fixed 1 ms grid (RENDER §6).
pub(crate) const TICK_SECONDS: f64 = 0.001;
/// Native `StrumlineNote.confirmHoldTimer` opponent reset (RENDER-BEHAVIOR §3).
pub(crate) const RECEPTOR_CONFIRM_MS: f64 = 150.0;

/// A velocity-like pair kept `f64` for the whole scene pipeline.
pub(crate) type Vec2 = [f64; 2];

/// Linear interpolation, kept here so the camera and animation code use one
/// definition.
pub(crate) fn lerp(from: f64, to: f64, factor: f64) -> f64 {
    from + (to - from) * factor
}

/// `(step + cameraZoomRateOffset * 4) % (cameraZoomRate * 4) == 0`.
pub(crate) fn zoom_pulse_step(step: i32) -> bool {
    (step + CAMERA_ZOOM_RATE_OFFSET * 4) % (CAMERA_ZOOM_RATE * 4) == 0
}

/// Camera state and the observed projection/follow formulas from
/// RENDER-BEHAVIOR §2. `scroll` is the Flixel camera scroll (world top-left).
#[derive(Debug, Clone)]
pub(crate) struct Camera {
    pub(crate) scroll: Vec2,
    pub(crate) zoom: f64,
    pub(crate) current_zoom: f64,
    pub(crate) bop_multiplier: f64,
}

impl Camera {
    pub(crate) fn new(current_zoom: f64) -> Self {
        Self {
            scroll: [0.0, 0.0],
            zoom: current_zoom,
            current_zoom,
            bop_multiplier: 1.0,
        }
    }

    /// `focusOn(point)`: `scroll = point - (width/2, height/2)`.
    pub(crate) fn focus_on(&mut self, focus: Vec2) {
        self.scroll = [focus[0] - HALF_WIDTH, focus[1] - HALF_HEIGHT];
    }

    /// A conductor step boundary: apply the zoom pulse when the observed gate
    /// matches (RENDER-BEHAVIOR §2).
    pub(crate) fn on_step(&mut self, step: i32) {
        if zoom_pulse_step(step) {
            self.bop_multiplier = CAMERA_BOP_INTENSITY;
        }
    }

    /// Per-tick decay: `bop = lerp(1.0, bop, pow(0.95, elapsed*60))`, then
    /// `zoom = currentZoom * bop`.
    pub(crate) fn decay(&mut self, elapsed_seconds: f64) {
        let factor = 0.95_f64.powf(elapsed_seconds * 60.0);
        self.bop_multiplier = lerp(1.0, self.bop_multiplier, factor);
        self.zoom = self.current_zoom * self.bop_multiplier;
    }

    /// Follow lerp with the same per-tick edge as the sim:
    /// `scroll += (target - scroll) * (1 - (1 - followLerp)^(elapsed*60))`.
    pub(crate) fn follow(&mut self, midpoint: Vec2, elapsed_seconds: f64) {
        let target = [midpoint[0] - HALF_WIDTH, midpoint[1] - HALF_HEIGHT];
        let factor = 1.0 - (1.0 - CAMERA_FOLLOW_LERP).powf(elapsed_seconds * 60.0);
        self.scroll[0] += (target[0] - self.scroll[0]) * factor;
        self.scroll[1] += (target[1] - self.scroll[1]) * factor;
    }

    /// Flixel camera projection (RENDER-BEHAVIOR §2):
    /// `screen = 640 + zoom * (world - scroll*scrollFactor - 640)`.
    #[cfg(test)]
    pub(crate) fn project(&self, world: Vec2, scroll_factor: Vec2) -> Vec2 {
        [
            HALF_WIDTH + self.zoom * (world[0] - self.scroll[0] * scroll_factor[0] - HALF_WIDTH),
            HALF_HEIGHT + self.zoom * (world[1] - self.scroll[1] * scroll_factor[1] - HALF_HEIGHT),
        ]
    }
}

/// `getNoteY` for the locked upscroll configuration (RENDER-BEHAVIOR §3).
/// The returned value is the offset from the strumline-relative base.
pub(crate) fn note_y_offset(song_ms: f64, strum_ms: f64, scroll_speed: f64) -> f64 {
    -(PIXELS_PER_MS * (song_ms - strum_ms) * scroll_speed)
}

/// Note head top-left in game-screen coordinates (RENDER-BEHAVIOR §3):
/// `y = strumline.y - INITIAL_OFFSET + getNoteY`,
/// `x = strumline.x + getXPos(dir) - (sprite.width - 104)/2 - NUDGE`.
pub(crate) fn note_top_left(
    line_x: f64,
    direction: usize,
    note_width: f64,
    song_ms: f64,
    strum_ms: f64,
    scroll_speed: f64,
) -> Vec2 {
    [
        line_x + direction as f64 * NOTE_SPACING - (note_width - STRUMLINE_SIZE) / 2.0 - NUDGE,
        STRUMLINE_Y - INITIAL_OFFSET + note_y_offset(song_ms, strum_ms, scroll_speed),
    ]
}

/// Receptor frame top-left (RENDER-BEHAVIOR §3):
/// `x = line.x + dir*112 + INITIAL_OFFSET`, `y = line.y + style y offset`.
pub(crate) fn receptor_top_left(line_x: f64, direction: usize, style_y_offset: f64) -> Vec2 {
    [
        line_x + direction as f64 * NOTE_SPACING + INITIAL_OFFSET,
        STRUMLINE_Y + style_y_offset,
    ]
}

/// Strumline background top-left (RENDER-BEHAVIOR §3).
pub(crate) fn background_top_left(line_x: f64) -> Vec2 {
    [line_x - BACKGROUND_PAD, STRUMLINE_Y]
}

/// `graphicWidth = (graphic.width / 8) * zoom`; for `NOTE_hold_assets` with
/// the funkin hold style: `(416/8) * 0.7 = 36.4` Dx. The 52 figure was the
/// pre-zoom column width and is a corrected transcription error
/// (RENDER-BEHAVIOR §3, corrected 2026-11-07).
pub(crate) fn sustain_width(page_width: f64, zoom: f64) -> f64 {
    (page_width / HOLD_TEXTURE_COLUMNS as f64) * zoom
}

/// Sustain strip top-left x:
/// `this.x + getXPos(dir) + STRUMLINE_SIZE/2 - width/2`; with width 36.4 the
/// lane-center offset is `-18.2` (`x = line + dir*112 + 33.8`).
pub(crate) fn sustain_top_left_x(line_x: f64, direction: usize, width: f64) -> f64 {
    line_x + direction as f64 * NOTE_SPACING + STRUMLINE_SIZE / 2.0 - width / 2.0
}

/// Sustains pinned to the receptor while held (RENDER-BEHAVIOR §3):
/// `y = strumline.y - INITIAL_OFFSET + 104/2` (fixed).
pub(crate) fn sustain_held_top(line_y: f64) -> f64 {
    line_y - INITIAL_OFFSET + STRUMLINE_SIZE / 2.0
}

/// `sustainHeight = 0.45 * sustainLengthMs * speed`.
pub(crate) fn sustain_height(remaining_ms: f64, scroll_speed: f64) -> f64 {
    PIXELS_PER_MS * remaining_ms * scroll_speed
}

/// Dropped hold offset: `(fullFullLen - currentLen) * 0.45`.
///
/// The renderer keeps this formula for the record, but a dropped hold is
/// completely hidden (`Strumline.hx:634-640`), so it is test-only.
#[cfg(test)]
pub(crate) fn sustain_drop_offset(full_ms: f64, remaining_ms: f64) -> f64 {
    (full_ms - remaining_ms) * PIXELS_PER_MS
}

/// `renderDistanceMs = 720 / 0.45 / (scrollSpeed < 1 ? scrollSpeed : 1)`
/// (RENDER-BEHAVIOR §3): only speeds below 1 reduce the window.
pub(crate) fn render_distance_ms(scroll_speed: f64) -> f64 {
    let divisor = if scroll_speed < 1.0 {
        scroll_speed
    } else {
        1.0
    };
    GAME_HEIGHT / PIXELS_PER_MS / divisor
}

/// Column pair for a direction: `(body, end cap)` in the 8-column sheet.
/// Columns are ordered by direction left/down/up/right, body then cap
/// (RENDER-BEHAVIOR §3 `NOTE_hold_assets` 416x87, 8 columns).
pub(crate) fn hold_columns(direction: usize) -> (usize, usize) {
    let body = direction * 2;
    (body, body + 1)
}

/// UV rect for one hold column.
pub(crate) fn hold_column_uv(column: usize) -> ([f64; 2], [f64; 2]) {
    let column = column.min(HOLD_TEXTURE_COLUMNS - 1);
    let width = HOLD_TEXTURE_WIDTH / HOLD_TEXTURE_COLUMNS as f64;
    let x0 = column as f64 * width / HOLD_TEXTURE_WIDTH;
    let x1 = (column as f64 + 1.0) * width / HOLD_TEXTURE_WIDTH;
    ([x0, 0.0], [x1, 1.0])
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

    #[test]
    fn frozen_strum_constants() {
        close(STRUMLINE_SIZE, 104.0);
        close(NOTE_SPACING, 112.0);
        close(INITIAL_OFFSET, -28.6);
        close(NUDGE, 2.0);
        close(PIXELS_PER_MS, 0.45);
        close(render_distance_ms(1.0), 1600.0);
        close(render_distance_ms(1.2), 1600.0);
        close(render_distance_ms(0.8), 2000.0);
        close(BACKGROUND_WIDTH, 480.0);
        close(BACKGROUND_ALPHA, 0.0);
        close(sustain_width(HOLD_TEXTURE_WIDTH, 0.7), 36.4);
        close(SUSTAIN_BOTTOM_HEIGHT, 30.45);
        close(SUSTAIN_EXTRA_CAP_TAIL, 24.36);
        close(SUSTAIN_HIDE_CLIP, 0.1);
        close(SUSTAIN_HELD_HIDE_REMAINING, 10.0);
        close(SUSTAIN_CAP_BOTTOM_CLIP, 0.9);
    }

    #[test]
    fn receptors_match_the_frozen_line_positions() {
        close(receptor_top_left(PLAYER_STRUMLINE_X, 0, 0.0)[0], 659.4);
        close(receptor_top_left(PLAYER_STRUMLINE_X, 3, 0.0)[0], 995.4);
        close(receptor_top_left(OPPONENT_STRUMLINE_X, 0, 0.0)[0], 19.4);
        close(receptor_top_left(OPPONENT_STRUMLINE_X, 3, 0.0)[0], 355.4);
        close(receptor_top_left(PLAYER_STRUMLINE_X, 0, 0.0)[1], 24.0);
        close(background_top_left(PLAYER_STRUMLINE_X)[0], 672.0);
        close(background_top_left(OPPONENT_STRUMLINE_X)[0], 32.0);
    }

    #[test]
    fn note_geometry_matches_upscroll_formulas() {
        let note_width = 157.0 * 0.7;
        let tl = note_top_left(PLAYER_STRUMLINE_X, 0, note_width, 800.0, 800.0, 1.2);
        close(tl[0], 688.0 - (note_width - 104.0) / 2.0 - 2.0);
        close(tl[1], 52.6);
        let future = note_top_left(PLAYER_STRUMLINE_X, 0, note_width, 0.0, 800.0, 1.2);
        close(future[1], 52.6 + 432.0);
        let late = note_top_left(PLAYER_STRUMLINE_X, 1, note_width, 1000.0, 800.0, 1.6);
        close(late[1], 52.6 - 144.0);
        close(note_y_offset(0.0, 800.0, 1.2), 432.0);
        close(note_y_offset(1000.0, 800.0, 1.6), -144.0);
    }

    #[test]
    fn sustain_geometry_matches_the_frozen_formulas() {
        let width = sustain_width(HOLD_TEXTURE_WIDTH, 0.7);
        close(width, 36.4);
        close(sustain_top_left_x(PLAYER_STRUMLINE_X, 0, width), 721.8);
        close(sustain_top_left_x(PLAYER_STRUMLINE_X, 1, width), 833.8);
        close(sustain_top_left_x(OPPONENT_STRUMLINE_X, 0, width), 81.8);
        // The x formula's `- width/2` term is -18.2; the line-relative x
        // offset is 104/2 - 18.2 = +33.8.
        close(width / 2.0, 18.2);
        close(
            sustain_top_left_x(PLAYER_STRUMLINE_X, 0, width) - PLAYER_STRUMLINE_X,
            33.8,
        );
        close(sustain_held_top(STRUMLINE_Y), 104.6);
        close(sustain_height(800.0, 1.2), 432.0);
        close(sustain_drop_offset(1000.0, 400.0), 270.0);
        assert_eq!(hold_columns(0), (0, 1));
        assert_eq!(hold_columns(3), (6, 7));
        let (start, end) = hold_column_uv(2);
        close(start[0], 0.25);
        close(end[0], 0.375);
    }

    #[test]
    fn camera_focus_and_projection_match_section_2() {
        let mut camera = Camera::new(1.1);
        // mainStage dad: slot (335, 885), whole bounds height 860 at scale 1,
        // character camera offsets [11, 6], stage-slot offsets [150, -100].
        let height = 860.0;
        let focus = [335.0 + 11.0 + 150.0, 885.0 - height / 2.0 + 6.0 - 100.0];
        close(focus[0], 496.0);
        close(focus[1], 361.0);
        camera.focus_on(focus);
        close(camera.scroll[0], -144.0);
        close(camera.scroll[1], 1.0);
        let screen = camera.project([0.0, 0.0], [1.0, 1.0]);
        close(screen[0], 94.4);
        close(screen[1], -37.1);
        let fixed = camera.project([0.0, 0.0], [0.0, 0.0]);
        close(fixed[0], -64.0);
        close(fixed[1], -36.0);
    }

    #[test]
    fn camera_pulse_gate_and_decay() {
        assert!(zoom_pulse_step(12));
        assert!(zoom_pulse_step(28));
        assert!(zoom_pulse_step(-4));
        assert!(!zoom_pulse_step(0));
        assert!(!zoom_pulse_step(11));
        assert!(!zoom_pulse_step(13));
        assert!(!zoom_pulse_step(16));

        let mut camera = Camera::new(1.1);
        camera.on_step(0);
        close(camera.bop_multiplier, 1.0);
        camera.on_step(12);
        close(camera.bop_multiplier, 1.015);
        camera.decay(TICK_SECONDS);
        let expected = 1.0 + (1.015 - 1.0) * 0.95_f64.powf(0.06);
        close(camera.bop_multiplier, expected);
        close(camera.zoom, 1.1 * expected);
        // 16.666 ticks of the 1 ms grid approximate one native 60 Hz frame.
        for _ in 0..15 {
            camera.decay(TICK_SECONDS);
        }
        close(
            camera.bop_multiplier,
            1.0 + (1.015 - 1.0) * 0.95_f64.powf(0.96),
        );
        close(camera.zoom, 1.1 * camera.bop_multiplier);
    }

    #[test]
    fn follow_lerp_uses_the_per_frame_ramp() {
        let mut camera = Camera::new(1.1);
        camera.scroll = [0.0, 0.0];
        camera.follow([640.0 + 100.0, 360.0], TICK_SECONDS);
        let factor = 1.0 - (1.0 - CAMERA_FOLLOW_LERP).powf(0.06);
        close(camera.scroll[0], 100.0 * factor);
        close(camera.scroll[1], 0.0);
    }
}
