//! Chart song-event consumption for the deterministic gameplay sim
//! (`docs/M6-CONTRACT.md` §12, schema "3" rows).
//!
//! The walker follows the observed `SongEventRegistry.queryEvents` semantics:
//! a persistent cursor over the time-sorted event array, activate-once, and
//! the >1000 ms age rule (`PlayAnimation` alone has `processOldEvents = false`
//! and records a `skipped_old` row instead of applying). Rows are the frozen
//! §12.4 wire shape; the sim keeps only the state a later row needs
//! (per-strumline scroll speed and the native `prevScrollTargets` snap), while
//! camera/zoom/animation/health state stays in the trace for the scene.
//!
//! Ease formulas are transcribed from flixel 6.1.2 `FlxEase.hx`
//! (`.haxelib/flixel/git/flixel/tweens/FlxEase.hx`): observed source
//! reference, kept as f64 per the M5 policy. Events read chart data with the
//! observed `getFloat`/`getInt`/`getBool`/`getString` coercion (string fields
//! on the corpus include `"char": "0"`, `"x": "0"`, `"y": "0"`).
//!
//! Focus points reuse the frozen M5/M6 stage-character placement algebra
//! (`render::scene::CharacterRig::camera_focus`): slot position, main-sheet
//! placement bounds, character scale, then character + stage-slot camera
//! offsets. Only sheet METADATA is read (no PNG decode, no Bevy), so the sim
//! stays headless; the same VFS asset-key candidates as the render loader are
//! used.

use std::path::Path;

use oxide_atlas::{AnimateStage, frame_quad, parse_packer_text, parse_sparrow};
use oxide_data::Typed;
use oxide_data::chart::{Chart, SongEvent};
use oxide_data::metadata::{Metadata, TimeChange};
use oxide_data::stage::StageData;
use oxide_vfs::Vfs;

use super::{SCHEMA_V3, SliceError, TraceEvent};

/// Upper bound for any single focus-resolution asset read, matching the M5
/// slice VFS contract.
const ASSET_SIZE_LIMIT: u64 = 64 * 1024 * 1024;

/// `SongEvent.DEFAULT_EASE` (`SongEvent.hx:46`); the ZoomCamera/ScrollSpeed
/// ease default ('linear'); FocusCamera defaults to 'CLASSIC' instead and
/// passes its own default into [`ease_input_with_default`].
const DEFAULT_EASE: &str = "linear";
/// `SongEvent.DEFAULT_EASE_DIR` (`SongEvent.hx:51`).
const DEFAULT_EASE_DIR: &str = "In";

/// `Constants.DEFAULT_BOP_INTENSITY` (`Constants.hx:241`).
const DEFAULT_BOP_INTENSITY: f64 = 1.015;
/// `Constants.DEFAULT_ZOOM_RATE` (`Constants.hx:246`).
const DEFAULT_ZOOM_RATE: f64 = 4.0;
/// `Constants.DEFAULT_ZOOM_OFFSET` (`Constants.hx:251`).
const DEFAULT_ZOOM_OFFSET: f64 = 0.0;
/// `Constants.DEFAULT_HEALTH_ICON` (`Constants.hx:210`).
const DEFAULT_HEALTH_ICON: &str = "face";
/// `PlayState.get_stageZoom` fallback with no current stage (`1.05x`).
const DEFAULT_STAGE_ZOOM: f64 = 1.05;
/// Default step length with no time change: `100 bpm`, 4/4, four steps/beat.
const DEFAULT_STEP_LENGTH_MS: f64 = 150.0;

const LINE_PLAYER: usize = 0;
const LINE_OPPONENT: usize = 1;

/// Builds the schema-3 row with the frozen M5 §2 header exactly.
macro_rules! trace_event_v3 {
    ($variant:ident, $kind:literal, $sim:expr $(, $field:ident : $value:expr)* $(,)?) => {
        TraceEvent::$variant {
            schema: SCHEMA_V3,
            kind: $kind,
            sim: $sim,
            input_ms: 0.0,
            input_time_us: 0.0,
            $( $field: $value, )*
        }
    };
}

/// Extracts a two-number JSON array; the value type is never named (the
/// crate does not depend on `serde_json` directly).
macro_rules! json_pair {
    ($value:expr) => {
        $value
            .and_then(|value| value.as_array())
            .and_then(|values| Some((values.first()?.as_f64()?, values.get(1)?.as_f64()?)))
    };
}

macro_rules! coerce_float {
    ($value:expr) => {{
        let value = $value;
        if let Some(number) = value.as_f64() {
            Some(number)
        } else if let Some(text) = value.as_str() {
            parse_float_prefix(text)
        } else {
            None
        }
    }};
}

macro_rules! coerce_int {
    ($value:expr) => {{
        let value = $value;
        if let Some(number) = value.as_i64() {
            Some(number)
        } else if let Some(number) = value.as_f64() {
            Some(number.trunc() as i64)
        } else if let Some(text) = value.as_str() {
            parse_int_prefix(text)
        } else {
            None
        }
    }};
}

/// A resolved ease is its `FlxEase` field name plus the f64 function.
type EaseFn = fn(f64) -> f64;
type ResolvedEase = (&'static str, EaseFn);

/// Timeline-sorted event plus the native `activated` flag.
#[derive(Debug, Clone)]
struct QueuedEvent {
    source: SongEvent,
    activated: bool,
}

/// One step-length segment derived from the metadata time changes.
#[derive(Debug, Clone, Copy)]
struct StepChange {
    time_ms: f64,
    step_length_ms: f64,
}

/// `Conductor.stepLengthMs` at fire time, without a live conductor.
///
/// `get_stepLengthMs = beatLengthMs / 4` and
/// `beatLengthMs = (60 / bpm) * 1000 * (4 / timeSignatureDen)`; the current
/// change is the last one at or before the song position (the first change
/// before any change starts, matching the Conductor's index 0).
#[derive(Debug, Clone, Default)]
struct StepSchedule {
    changes: Vec<StepChange>,
}

impl StepSchedule {
    fn new(changes: Option<&[TimeChange]>) -> Self {
        let mut mapped = Vec::new();
        for change in changes.unwrap_or_default() {
            if !change.time.is_finite() || !change.bpm.is_finite() || change.bpm <= 0.0 {
                continue;
            }
            let denominator = change
                .time_signature_den
                .filter(|denominator| *denominator > 0)
                .unwrap_or(4);
            let beat_ms = (60.0 / change.bpm) * 1000.0 * (4.0 / denominator as f64);
            let step_length_ms = beat_ms / 4.0;
            if !step_length_ms.is_finite() || step_length_ms <= 0.0 {
                continue;
            }
            mapped.push(StepChange {
                time_ms: change.time.max(0.0),
                step_length_ms,
            });
        }
        mapped.sort_by(|left, right| left.time_ms.total_cmp(&right.time_ms));
        Self { changes: mapped }
    }

    fn at(&self, now_ms: f64) -> f64 {
        let Some(first) = self.changes.first() else {
            return DEFAULT_STEP_LENGTH_MS;
        };
        let mut selected = *first;
        for change in &self.changes {
            if change.time_ms <= now_ms {
                selected = *change;
            } else {
                break;
            }
        }
        selected.step_length_ms
    }
}

/// One monotonic scroll-speed channel (player or opponent strumline).
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

/// Native `tweenScrollSpeed` / `cancelScrollSpeedTweens` /
/// `prevScrollTargets` state (`PlayState.hx:4039-4104`).
#[derive(Debug, Clone)]
struct ScrollState {
    lines: [ScrollLine; 2],
    /// Targets pushed by the immediately previous call; a later event cancels
    /// all tweens and snaps these lines back to those values first.
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

    /// Advances the in-flight tweens to `now_ms` (native `super.update` runs
    /// before the event dispatch in the frame).
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

    /// Runs the native cancel + previous-target snap, then applies the new
    /// targets. Returns each line's value BEFORE its assignment (the row's
    /// `base_scroll`).
    fn apply(
        &mut self,
        now_ms: f64,
        targets: &[usize],
        target: f64,
        duration_ms: f64,
        ease: Option<EaseFn>,
    ) -> Vec<f64> {
        for line in &mut self.lines {
            line.tween = None;
        }
        let previous = std::mem::take(&mut self.prev_targets);
        for (line, value) in previous {
            self.lines[line].value = value;
        }

        let mut bases = Vec::with_capacity(targets.len());
        for &line in targets {
            let base = self.lines[line].value;
            bases.push(base);
            match ease {
                Some(function) if duration_ms > 0.0 => {
                    self.lines[line].tween = Some(ScrollTween {
                        from: base,
                        to: target,
                        start_ms: now_ms,
                        duration_ms,
                        ease: function,
                    });
                }
                _ => self.lines[line].value = target,
            }
            self.prev_targets.push((line, target));
        }
        bases
    }
}

/// Stage-slot placement values consumed by the camera-focus resolution.
#[derive(Debug, Clone, Copy)]
struct StageSlot {
    position: [f64; 2],
    camera_offsets: [f64; 2],
    scale: f64,
}

/// Chart-event walker plus the state later rows depend on.
#[derive(Debug, Clone)]
pub(super) struct EventState {
    events: Vec<QueuedEvent>,
    cursor: usize,
    stage_zoom: f64,
    focus_points: [Option<[f64; 2]>; 3],
    prop_names: Vec<String>,
    chart_scroll: f64,
    schedule: StepSchedule,
    scroll: ScrollState,
}

impl EventState {
    /// Builds the event walker for the event-enabled schema-3 path.
    ///
    /// Missing stage/character files are treated as "no such character"
    /// (native null-character early return) so synthetic charts without the
    /// render assets stay usable; a PRESENT but malformed/resolution-failing
    /// asset fails closed. Non-finite event times fail closed.
    pub(super) fn build(
        vfs: &Vfs,
        metadata: &Metadata,
        chart: &Chart,
        difficulty: &str,
    ) -> Result<Self, SliceError> {
        let mut events = chart.events.clone();
        for (index, event) in events.iter().enumerate() {
            if !event.time.is_finite() {
                return Err(SliceError::InvalidEvent {
                    index,
                    detail: "event time must be finite",
                });
            }
        }
        // Stable sort: same-time events keep chart parse order ([D] engine
        // decision; the corpus is already ascending with 0 unsorted pairs).
        events.sort_by(|left, right| left.time.total_cmp(&right.time));

        let chart_scroll = chart_scroll_speed(chart, difficulty);
        let schedule = StepSchedule::new(metadata.time_changes.as_deref());

        let mut stage_zoom = DEFAULT_STAGE_ZOOM;
        let mut focus_points = [None, None, None];
        let mut prop_names = Vec::new();
        if let Some(play_data) = metadata.play_data.as_ref()
            && let Some(stage_name) = play_data.stage.as_deref()
        {
            let stage_path = format!("preload/data/stages/{stage_name}.json");
            if let Some(bytes) = read_optional_asset(vfs, &stage_path)? {
                let document =
                    oxide_data::parse_file(Path::new(&stage_path), &bytes).map_err(|source| {
                        SliceError::Parse {
                            path: stage_path.clone(),
                            source,
                        }
                    })?;
                let Typed::Stage(stage) = document.typed else {
                    return Err(SliceError::WrongKind {
                        path: stage_path,
                        expected: "stage",
                        actual: document.kind,
                    });
                };
                stage_zoom = stage
                    .camera_zoom
                    .filter(|zoom| zoom.is_finite() && *zoom > 0.0)
                    .unwrap_or(DEFAULT_STAGE_ZOOM);
                prop_names = stage.props.iter().map(|prop| prop.name.clone()).collect();
                focus_points = resolve_focus_points(vfs, play_data, &stage)?;
            }
        }

        Ok(Self {
            events: events
                .into_iter()
                .map(|source| QueuedEvent {
                    source,
                    activated: false,
                })
                .collect(),
            cursor: 0,
            stage_zoom,
            focus_points,
            prop_names,
            chart_scroll,
            schedule,
            scroll: ScrollState::new(chart_scroll),
        })
    }

    /// Consumes every due event in array order and appends its schema-3 row(s)
    /// to the tick dispatch. Called between the hold mirror and the input
    /// queue, the native `processSongEvents` position.
    pub(super) fn dispatch(&mut self, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        let now_ms = sim_ms as f64;
        // Native `super.update` advances scroll-speed tweens before the event
        // dispatch of the same frame.
        self.scroll.advance(now_ms);

        for index in self.take_due(now_ms) {
            let event = self.events[index].source.clone();
            self.events[index].activated = true;
            let age_ms = now_ms - event.time;
            if age_ms > 1000.0 && event.event == "PlayAnimation" {
                // `processOldEvents = false`: record the skip, apply nothing.
                self.play_animation(&event, sim_ms, true, out);
                continue;
            }
            self.dispatch_event(&event, sim_ms, out);
        }
    }

    /// Native `queryEvents`: every non-activated event with `time <= now`
    /// before the first future event; the cursor lands on that future event.
    fn take_due(&mut self, now_ms: f64) -> Vec<usize> {
        while self
            .events
            .get(self.cursor)
            .is_some_and(|event| event.activated)
        {
            self.cursor += 1;
        }
        let mut due = Vec::new();
        let mut index = self.cursor;
        while let Some(event) = self.events.get(index) {
            if event.activated {
                index += 1;
                continue;
            }
            if event.source.time > now_ms {
                self.cursor = index;
                return due;
            }
            due.push(index);
            index += 1;
        }
        due
    }

    fn dispatch_event(&mut self, event: &SongEvent, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        match event.event.as_str() {
            "FocusCamera" => self.focus_camera(event, sim_ms, out),
            "ZoomCamera" => self.camera_zoom(event, sim_ms, out),
            "SetCameraBop" => self.camera_bop(event, sim_ms, out),
            "PlayAnimation" => self.play_animation(event, sim_ms, false, out),
            "ScrollSpeed" => self.scroll_speed(event, sim_ms, out),
            "SetHealthIcon" => self.health_icon(event, sim_ms, out),
            kind => {
                // EnableMask / sserafim / built-in-but-out-of-inventory / any
                // unknown kind: record-only, no engine state ([D] §12.2).
                out.push(trace_event_v3!(
                    UnknownEvent,
                    "unknown_event",
                    sim_ms,
                    event_kind: kind.to_owned(),
                    value: Some(event.clone()),
                ));
            }
        }
    }

    fn focus_camera(&self, event: &SongEvent, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        let x = get_float(event, "x").unwrap_or(0.0);
        let y = get_float(event, "y").unwrap_or(0.0);
        let duration = get_float(event, "duration").unwrap_or(4.0);
        let target_char = get_int(event, "char")
            .or_else(|| scalar_int(event))
            .unwrap_or(0);

        let (target_kind, focus) = match target_char {
            -1 => ("position", None),
            0 => ("bf", self.focus_points[0]),
            1 => ("dad", self.focus_points[1]),
            2 => ("gf", self.focus_points[2]),
            _ => ("unknown", None),
        };
        // A null character traces and returns without a state change; the row
        // keeps the requested target kind and the literal `(x, y)` focus.
        let world_focus = focus.unwrap_or([x, y]);

        let (raw_ease, ease_dir) = ease_input_with_default(event, "CLASSIC");
        let (ease_kind, ease, ease_dir) = match raw_ease.as_str() {
            "CLASSIC" => ("classic", None, None),
            "INSTANT" => ("instant", None, None),
            _ => match resolve_named_ease(&raw_ease, &ease_dir) {
                Some((name, _)) => ("named", Some(name.to_owned()), Some(ease_dir)),
                // Unknown ease: full row, activated, no state change.
                None => ("named", None, None),
            },
        };

        out.push(trace_event_v3!(
            CameraFocus,
            "camera_focus",
            sim_ms,
            target_char: target_char,
            target_kind: target_kind,
            x: x,
            y: y,
            world_focus: world_focus,
            duration: duration,
            ease_kind: ease_kind,
            ease: ease,
            ease_dir: ease_dir,
        ));
    }

    fn camera_zoom(&self, event: &SongEvent, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        let zoom = get_float(event, "zoom").unwrap_or(1.0);
        let duration = get_float(event, "duration").unwrap_or(4.0);
        let mode_raw = get_string(event, "mode").unwrap_or_else(|| "direct".to_owned());
        let direct = mode_raw == "direct";
        let mode = if direct { "direct" } else { "stage" };
        let resolved_target_zoom = zoom * if direct { 1.0 } else { self.stage_zoom };

        // `widescreenScaleX/Y` are accepted and ignored at 16:9
        // (`wideScale = (1, 1)`), so they are not part of the frozen row.

        let (raw_ease, ease_dir) = ease_input_with_default(event, DEFAULT_EASE);
        let (ease, ease_dir) = if raw_ease == "INSTANT" {
            (Some("INSTANT".to_owned()), None)
        } else {
            match resolve_named_ease(&raw_ease, &ease_dir) {
                Some((name, _)) => (Some(name.to_owned()), Some(ease_dir)),
                None => (None, None),
            }
        };

        out.push(trace_event_v3!(
            CameraZoom,
            "camera_zoom",
            sim_ms,
            zoom: zoom,
            mode: mode,
            resolved_target_zoom: resolved_target_zoom,
            duration: duration,
            ease: ease,
            ease_dir: ease_dir,
        ));
    }

    fn camera_bop(&self, event: &SongEvent, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        let intensity = get_float(event, "intensity").unwrap_or(1.0);
        let rate = get_float(event, "rate").unwrap_or(DEFAULT_ZOOM_RATE);
        let offset = get_float(event, "offset").unwrap_or(DEFAULT_ZOOM_OFFSET);
        let camera_bop_intensity = (DEFAULT_BOP_INTENSITY - 1.0) * intensity + 1.0;
        let hud_intensity = (DEFAULT_BOP_INTENSITY - 1.0) * intensity * 2.0;

        out.push(trace_event_v3!(
            CameraBop,
            "camera_bop",
            sim_ms,
            intensity: intensity,
            camera_bop_intensity: camera_bop_intensity,
            hud_intensity: hud_intensity,
            rate: rate,
            offset: offset,
        ));
    }

    fn play_animation(
        &self,
        event: &SongEvent,
        sim_ms: u64,
        skipped_old: bool,
        out: &mut Vec<TraceEvent>,
    ) {
        let target = get_string(event, "target").unwrap_or_else(|| "boyfriend".to_owned());
        let anim = get_string(event, "anim").unwrap_or_else(|| "idle".to_owned());
        let force = get_bool(event, "force").unwrap_or(false);
        let resolved_kind = match target.as_str() {
            "boyfriend" | "bf" | "player" => "bf".to_owned(),
            "dad" | "opponent" => "dad".to_owned(),
            "girlfriend" | "gf" => "gf".to_owned(),
            _ if self.prop_names.iter().any(|name| name == &target) => {
                format!("prop:{target}")
            }
            _ => "none".to_owned(),
        };

        out.push(trace_event_v3!(
            PlayAnimation,
            "play_animation",
            sim_ms,
            target: target,
            anim: anim,
            force: force,
            resolved_kind: resolved_kind,
            skipped_old: skipped_old,
        ));
    }

    fn scroll_speed(&mut self, event: &SongEvent, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        let scroll = get_float(event, "scroll").unwrap_or(1.0);
        let duration = get_float(event, "duration").unwrap_or(4.0);
        let absolute = get_bool(event, "absolute").unwrap_or(false);
        let strumline = get_string(event, "strumline").unwrap_or_else(|| "both".to_owned());
        let target = if absolute {
            scroll
        } else {
            scroll * self.chart_scroll
        };

        let targets: &[usize] = match strumline.as_str() {
            "both" => &[LINE_PLAYER, LINE_OPPONENT],
            "player" => &[LINE_PLAYER],
            "opponent" => &[LINE_OPPONENT],
            // Native builds `<value>Strumline`; no shipped target exists for
            // other values. Outside the corpus, record nothing (no invented
            // strumline names).
            _ => &[],
        };
        if targets.is_empty() {
            return;
        }

        let (raw_ease, ease_dir) = ease_input_with_default(event, DEFAULT_EASE);
        let instant = raw_ease == "INSTANT";
        let (ease_name, ease_dir, ease_fn) = if instant {
            (Some("INSTANT".to_owned()), None, None)
        } else {
            match resolve_named_ease(&raw_ease, &ease_dir) {
                Some((name, function)) => (Some(name.to_owned()), Some(ease_dir), Some(function)),
                None => (None, None, None),
            }
        };

        if instant || ease_fn.is_some() {
            let duration_ms = if instant {
                0.0
            } else {
                self.schedule.at(sim_ms as f64) * duration
            };
            let bases = self
                .scroll
                .apply(sim_ms as f64, targets, target, duration_ms, ease_fn);
            for (&line, base_scroll) in targets.iter().zip(bases) {
                out.push(trace_event_v3!(
                    ScrollSpeed,
                    "scroll_speed",
                    sim_ms,
                    strumline: line_name(line),
                    target: target,
                    absolute: absolute,
                    base_scroll: base_scroll,
                    duration: duration,
                    ease: ease_name.clone(),
                    ease_dir: ease_dir.clone(),
                ));
            }
        } else {
            // Unknown ease: early return, no cancel/snap/state change; the row
            // still records the current value for every listed strumline.
            for &line in targets {
                out.push(trace_event_v3!(
                    ScrollSpeed,
                    "scroll_speed",
                    sim_ms,
                    strumline: line_name(line),
                    target: target,
                    absolute: absolute,
                    base_scroll: self.scroll.value(line),
                    duration: duration,
                    ease: None,
                    ease_dir: None,
                ));
            }
        }
    }

    fn health_icon(&self, event: &SongEvent, sim_ms: u64, out: &mut Vec<TraceEvent>) {
        let char = get_int(event, "char").unwrap_or(0);
        let id = get_string(event, "id").unwrap_or_else(|| DEFAULT_HEALTH_ICON.to_owned());
        let should_bop = get_bool(event, "shouldBop").unwrap_or(true);
        let scale = get_float(event, "scale").unwrap_or(1.0);
        let flip_x = get_bool(event, "flipX").unwrap_or(false);
        let is_pixel = get_bool(event, "isPixel").unwrap_or(false);
        let offset_x = get_float(event, "offsetX").unwrap_or(0.0);
        let offset_y = get_float(event, "offsetY").unwrap_or(0.0);

        // 0/1 apply to iconP1/P2; any other value only traces natively. The
        // row keeps the observed char value either way; no sim state exists
        // (HUD icon rendering is a 6D deliverable).
        out.push(trace_event_v3!(
            HealthIcon,
            "health_icon",
            sim_ms,
            char: char,
            id: id,
            should_bop: should_bop,
            scale: scale,
            flip_x: flip_x,
            is_pixel: is_pixel,
            offsets: [offset_x, offset_y],
        ));
    }
}

fn line_name(line: usize) -> &'static str {
    if line == LINE_PLAYER {
        "player"
    } else {
        "opponent"
    }
}

/// `chart.scrollSpeed[difficulty] ?? 1.0` (the strumline init value).
fn chart_scroll_speed(chart: &Chart, difficulty: &str) -> f64 {
    chart
        .scroll_speed
        .as_ref()
        .and_then(|speeds| speeds.get(difficulty))
        .and_then(|value| value.as_f64())
        .filter(|value| value.is_finite() && *value > 0.0)
        .unwrap_or(1.0)
}

fn resolve_focus_points(
    vfs: &Vfs,
    play_data: &oxide_data::metadata::PlayData,
    stage: &StageData,
) -> Result<[Option<[f64; 2]>; 3], SliceError> {
    let characters = play_data.characters.as_ref();
    let roster: [(&str, Option<&str>); 3] = [
        ("bf", characters.and_then(|data| data.player.as_deref())),
        ("dad", characters.and_then(|data| data.opponent.as_deref())),
        (
            "gf",
            characters
                .and_then(|data| data.girlfriend.as_deref())
                .filter(|id| !id.is_empty()),
        ),
    ];
    let mut points = [None, None, None];
    for (index, (slot_key, id)) in roster.into_iter().enumerate() {
        let Some(id) = id else {
            continue;
        };
        let Some(slot) = stage_slot(stage, slot_key) else {
            continue;
        };
        points[index] = load_focus_point(vfs, id, slot, stage.directory.as_deref())?;
    }
    Ok(points)
}

/// Parses one stage character slot (`position`, `cameraOffsets`, `scale`).
fn stage_slot(stage: &StageData, key: &str) -> Option<StageSlot> {
    let characters = stage.characters.as_ref()?;
    let value = characters.as_object()?.get(key)?;
    let position = json_pair!(value.get("position"))?;
    let camera_offsets = json_pair!(value.get("cameraOffsets")).unwrap_or((0.0, 0.0));
    let scale = value
        .get("scale")
        .and_then(|scale| scale.as_f64())
        .unwrap_or(1.0);
    Some(StageSlot {
        position: [position.0, position.1],
        camera_offsets: [camera_offsets.0, camera_offsets.1],
        scale,
    })
}

fn load_focus_point(
    vfs: &Vfs,
    id: &str,
    slot: StageSlot,
    level_dir: Option<&str>,
) -> Result<Option<[f64; 2]>, SliceError> {
    let data_path = format!("preload/data/characters/{id}.json");
    let Some(bytes) = read_optional_asset(vfs, &data_path)? else {
        return Ok(None);
    };
    let document = oxide_data::parse_file(Path::new(&data_path), &bytes).map_err(|source| {
        SliceError::Parse {
            path: data_path.clone(),
            source,
        }
    })?;
    let Typed::Character(character) = document.typed else {
        return Err(SliceError::WrongKind {
            path: data_path,
            expected: "character",
            actual: document.kind,
        });
    };

    let kind = sheet_kind(character.render_type.as_deref(), id)?;
    let height = main_sheet_height(vfs, &character.asset_path, level_dir, kind, id)?;
    let scale = character.scale.unwrap_or(1.0) * slot.scale;
    let offsets = character.camera_offsets.unwrap_or((0.0, 0.0));
    let focus = [
        slot.position[0] + offsets.0 + slot.camera_offsets[0],
        slot.position[1] - height * scale / 2.0 + offsets.1 + slot.camera_offsets[1],
    ];
    if !focus[0].is_finite() || !focus[1].is_finite() {
        return Err(SliceError::InvalidAsset {
            path: format!("preload/data/characters/{id}.json"),
            detail: "character camera focus point is not finite".to_owned(),
        });
    }
    Ok(Some(focus))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SheetKind {
    Atlas,
    Sprite,
    Packer,
}

/// Character renderType normalization from the 6B freeze: null defaults to
/// sparrow; any unsupported string fails closed.
fn sheet_kind(render_type: Option<&str>, id: &str) -> Result<SheetKind, SliceError> {
    match render_type {
        None | Some("sparrow" | "multisparrow") => Ok(SheetKind::Sprite),
        Some("packer") => Ok(SheetKind::Packer),
        Some("animateatlas" | "multianimateatlas") => Ok(SheetKind::Atlas),
        Some(other) => Err(SliceError::InvalidAsset {
            path: format!("preload/data/characters/{id}.json"),
            detail: format!("unsupported renderType {other:?}"),
        }),
    }
}

fn main_sheet_height(
    vfs: &Vfs,
    asset_path: &str,
    level_dir: Option<&str>,
    kind: SheetKind,
    id: &str,
) -> Result<f64, SliceError> {
    let bases = image_file_bases(asset_path, level_dir);
    match kind {
        SheetKind::Sprite => {
            let Some(text) = read_text_in_dirs(vfs, &bases, ".xml")? else {
                return Err(SliceError::InvalidAsset {
                    path: format!("{}.xml", bases.first().map_or("<none>", String::as_str)),
                    detail: format!("character {id:?} main sparrow sheet metadata is missing"),
                });
            };
            let atlas = parse_sparrow(&text).map_err(|source| SliceError::InvalidAsset {
                path: format!("{}.xml", bases.first().map_or("<none>", String::as_str)),
                detail: format!("character {id:?} sparrow sheet: {source}"),
            })?;
            let first = atlas
                .subtextures
                .first()
                .ok_or_else(|| SliceError::InvalidAsset {
                    path: format!("{}.xml", bases.first().map_or("<none>", String::as_str)),
                    detail: format!("character {id:?} sparrow sheet has no frames"),
                })?;
            Ok(frame_quad(first).dest_size[1] as f64)
        }
        SheetKind::Packer => {
            let Some(text) = read_text_in_dirs(vfs, &bases, ".txt")? else {
                return Err(SliceError::InvalidAsset {
                    path: format!("{}.txt", bases.first().map_or("<none>", String::as_str)),
                    detail: format!("character {id:?} main packer sheet metadata is missing"),
                });
            };
            let atlas = parse_packer_text(&text).map_err(|source| SliceError::InvalidAsset {
                path: format!("{}.txt", bases.first().map_or("<none>", String::as_str)),
                detail: format!("character {id:?} packer sheet: {source}"),
            })?;
            let first = atlas
                .frames
                .first()
                .ok_or_else(|| SliceError::InvalidAsset {
                    path: format!("{}.txt", bases.first().map_or("<none>", String::as_str)),
                    detail: format!("character {id:?} packer sheet has no frames"),
                })?;
            Ok(f64::from(first.source_size[1]))
        }
        SheetKind::Atlas => {
            let Some(spritemap) = read_text_in_dirs(vfs, &bases, "/spritemap1.json")? else {
                return Err(SliceError::InvalidAsset {
                    path: format!(
                        "{}/spritemap1.json",
                        bases.first().map_or("<none>", String::as_str)
                    ),
                    detail: format!("character {id:?} animate-atlas spritemap is missing"),
                });
            };
            let Some(animation) = read_text_in_dirs(vfs, &bases, "/Animation.json")? else {
                return Err(SliceError::InvalidAsset {
                    path: format!(
                        "{}/Animation.json",
                        bases.first().map_or("<none>", String::as_str)
                    ),
                    detail: format!("character {id:?} animate-atlas Animation.json is missing"),
                });
            };
            let stage = AnimateStage::parse(&animation, &spritemap).map_err(|source| {
                SliceError::InvalidAsset {
                    path: format!(
                        "{}/Animation.json",
                        bases.first().map_or("<none>", String::as_str)
                    ),
                    detail: format!("character {id:?} animate atlas: {source}"),
                }
            })?;
            let bounds = stage.bounds();
            if !bounds.iter().all(|value| value.is_finite()) || bounds[2] <= 0.0 || bounds[3] <= 0.0
            {
                return Err(SliceError::InvalidAsset {
                    path: format!(
                        "{}/Animation.json",
                        bases.first().map_or("<none>", String::as_str)
                    ),
                    detail: format!("character {id:?} animate atlas has empty bounds {bounds:?}"),
                });
            }
            Ok(bounds[3])
        }
    }
}

/// V-Slice image key candidates, in the observed `Paths.getPath` order.
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

/// `file` carries its own separator: `.xml`/`.txt` for sheet metadata files
/// (assetPath is a file base) or `/Animation.json` for animate-atlas
/// directories (assetPath is a directory).
fn read_text_in_dirs(vfs: &Vfs, dirs: &[String], file: &str) -> Result<Option<String>, SliceError> {
    for dir in dirs {
        let path = format!("{dir}{file}");
        let Some(bytes) = read_optional_asset(vfs, &path)? else {
            continue;
        };
        let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
        let text = String::from_utf8(bytes.to_vec()).map_err(|error| SliceError::InvalidAsset {
            path: path.clone(),
            detail: format!("not UTF-8: {error}"),
        })?;
        return Ok(Some(text));
    }
    Ok(None)
}

fn read_optional_asset(vfs: &Vfs, path: &str) -> Result<Option<Vec<u8>>, SliceError> {
    let Some(source) = vfs.resolve(path) else {
        return Ok(None);
    };
    vfs.read(&source, ASSET_SIZE_LIMIT)
        .map(Some)
        .map_err(|source| SliceError::Read {
            path: path.to_owned(),
            source,
        })
}

// --- observed SongEventData field coercion ---------------------------------

fn get_float(event: &SongEvent, key: &str) -> Option<f64> {
    let value = event.value.as_ref()?;
    let field = value.get(key)?;
    coerce_float!(field)
}

fn get_int(event: &SongEvent, key: &str) -> Option<i64> {
    let value = event.value.as_ref()?;
    let field = value.get(key)?;
    coerce_int!(field)
}

fn get_bool(event: &SongEvent, key: &str) -> Option<bool> {
    let value = event.value.as_ref()?;
    value.get(key).and_then(|field| field.as_bool())
}

fn get_string(event: &SongEvent, key: &str) -> Option<String> {
    let value = event.value.as_ref()?;
    value
        .get(key)
        .and_then(|field| field.as_str())
        .map(str::to_owned)
}

/// `cast data.value` fallback for a scalar event payload (`v: 0|1`).
fn scalar_int(event: &SongEvent) -> Option<i64> {
    let value = event.value.as_ref()?;
    coerce_int!(value)
}

/// `(ease, easeDir)` after the observed `(In|Out|InOut)$/i`/`linear`
/// suppression.
/// (`ease`, resolved `easeDir`) with the per-event default ease: 'CLASSIC'
/// for FocusCamera (FocusCameraSongEvent.hx L62/L86), 'linear'
/// (`SongEvent.DEFAULT_EASE`, SongEvent.hx L46) for ZoomCamera/ScrollSpeed.
fn ease_input_with_default(event: &SongEvent, default_ease: &str) -> (String, String) {
    let ease = get_string(event, "ease").unwrap_or_else(|| default_ease.to_owned());
    let mut ease_dir = get_string(event, "easeDir").unwrap_or_else(|| DEFAULT_EASE_DIR.to_owned());
    if ease_has_direction_suffix(&ease) || ease == "linear" {
        ease_dir.clear();
    }
    (ease, ease_dir)
}

fn ease_has_direction_suffix(ease: &str) -> bool {
    let lower = ease.to_ascii_lowercase();
    lower.ends_with("in") || lower.ends_with("out") || lower.ends_with("inout")
}

fn resolve_named_ease(ease: &str, ease_dir: &str) -> Option<ResolvedEase> {
    let name = format!("{ease}{ease_dir}");
    resolve_ease(&name)
}

/// `Std.parseInt`-style leading integer prefix.
fn parse_int_prefix(text: &str) -> Option<i64> {
    let text = text.trim_start();
    let mut end = 0usize;
    for (index, ch) in text.char_indices() {
        if index == 0 && (ch == '+' || ch == '-') {
            end = ch.len_utf8();
            continue;
        }
        if ch.is_ascii_digit() {
            end = index + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    text.get(..end)?.parse().ok()
}

/// `Std.parseFloat`-style leading decimal prefix (decimal/exponent forms).
fn parse_float_prefix(text: &str) -> Option<f64> {
    let text = text.trim_start();
    let bytes = text.as_bytes();
    let mut index = 0usize;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }
    let mut digits = false;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
        digits = true;
    }
    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            digits = true;
        }
    }
    if !digits {
        return None;
    }
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        let exponent_start = index;
        index += 1;
        if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
            index += 1;
        }
        let mut exponent_digits = false;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
            exponent_digits = true;
        }
        if !exponent_digits {
            index = exponent_start;
        }
    }
    text.get(..index)?.parse().ok()
}

// --- flixel 6.1.2 FlxEase (f64 transcription) ------------------------------

fn resolve_ease(name: &str) -> Option<ResolvedEase> {
    type Ease = fn(f64) -> f64;
    let resolved: (&'static str, Ease) = match name {
        "linear" => ("linear", linear),
        "quadIn" => ("quadIn", quad_in),
        "quadOut" => ("quadOut", quad_out),
        "quadInOut" => ("quadInOut", quad_in_out),
        "cubeIn" => ("cubeIn", cube_in),
        "cubeOut" => ("cubeOut", cube_out),
        "cubeInOut" => ("cubeInOut", cube_in_out),
        "quartIn" => ("quartIn", quart_in),
        "quartOut" => ("quartOut", quart_out),
        "quartInOut" => ("quartInOut", quart_in_out),
        "quintIn" => ("quintIn", quint_in),
        "quintOut" => ("quintOut", quint_out),
        "quintInOut" => ("quintInOut", quint_in_out),
        "sineIn" => ("sineIn", sine_in),
        "sineOut" => ("sineOut", sine_out),
        "sineInOut" => ("sineInOut", sine_in_out),
        "expoIn" => ("expoIn", expo_in),
        "expoOut" => ("expoOut", expo_out),
        "expoInOut" => ("expoInOut", expo_in_out),
        "circIn" => ("circIn", circ_in),
        "circOut" => ("circOut", circ_out),
        "circInOut" => ("circInOut", circ_in_out),
        "backIn" => ("backIn", back_in),
        "backOut" => ("backOut", back_out),
        "backInOut" => ("backInOut", back_in_out),
        "bounceIn" => ("bounceIn", bounce_in),
        "bounceOut" => ("bounceOut", bounce_out),
        "bounceInOut" => ("bounceInOut", bounce_in_out),
        "smoothStepIn" => ("smoothStepIn", smooth_step_in),
        "smoothStepOut" => ("smoothStepOut", smooth_step_out),
        "smoothStepInOut" => ("smoothStepInOut", smooth_step_in_out),
        "smootherStepIn" => ("smootherStepIn", smoother_step_in),
        "smootherStepOut" => ("smootherStepOut", smoother_step_out),
        "smootherStepInOut" => ("smootherStepInOut", smoother_step_in_out),
        "elasticIn" => ("elasticIn", elastic_in),
        "elasticOut" => ("elasticOut", elastic_out),
        "elasticInOut" => ("elasticInOut", elastic_in_out),
        _ => return None,
    };
    Some(resolved)
}

const ELASTIC_AMPLITUDE: f64 = 1.0;
const ELASTIC_PERIOD: f64 = 0.4;
const B1: f64 = 1.0 / 2.75;
const B2: f64 = 2.0 / 2.75;
const B3: f64 = 1.5 / 2.75;
const B4: f64 = 2.5 / 2.75;
const B5: f64 = 2.25 / 2.75;
const B6: f64 = 2.625 / 2.75;

const PI2: f64 = std::f64::consts::PI / 2.0;

fn linear(t: f64) -> f64 {
    t
}

fn quad_in(t: f64) -> f64 {
    t * t
}

fn quad_out(t: f64) -> f64 {
    -t * (t - 2.0)
}

fn quad_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        t * t * 2.0
    } else {
        let t = t - 1.0;
        1.0 - t * t * 2.0
    }
}

fn cube_in(t: f64) -> f64 {
    t * t * t
}

fn cube_out(t: f64) -> f64 {
    let t = t - 1.0;
    1.0 + t * t * t
}

fn cube_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        t * t * t * 4.0
    } else {
        let t = t - 1.0;
        1.0 + t * t * t * 4.0
    }
}

fn quart_in(t: f64) -> f64 {
    t * t * t * t
}

fn quart_out(t: f64) -> f64 {
    let t = t - 1.0;
    1.0 - t * t * t * t
}

fn quart_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        t * t * t * t * 8.0
    } else {
        let t = t * 2.0 - 2.0;
        (1.0 - t * t * t * t) / 2.0 + 0.5
    }
}

fn quint_in(t: f64) -> f64 {
    t * t * t * t * t
}

fn quint_out(t: f64) -> f64 {
    let t = t - 1.0;
    t * t * t * t * t + 1.0
}

fn quint_in_out(t: f64) -> f64 {
    let t = t * 2.0;
    if t < 1.0 {
        (t * t * t * t * t) / 2.0
    } else {
        let t = t - 2.0;
        (t * t * t * t * t + 2.0) / 2.0
    }
}

fn smooth_step_in(t: f64) -> f64 {
    2.0 * smooth_step_in_out(t / 2.0)
}

fn smooth_step_out(t: f64) -> f64 {
    2.0 * smooth_step_in_out(t / 2.0 + 0.5) - 1.0
}

fn smooth_step_in_out(t: f64) -> f64 {
    t * t * (t * -2.0 + 3.0)
}

fn smoother_step_in(t: f64) -> f64 {
    2.0 * smoother_step_in_out(t / 2.0)
}

fn smoother_step_out(t: f64) -> f64 {
    2.0 * smoother_step_in_out(t / 2.0 + 0.5) - 1.0
}

fn smoother_step_in_out(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn sine_in(t: f64) -> f64 {
    -f64::cos(PI2 * t) + 1.0
}

fn sine_out(t: f64) -> f64 {
    f64::sin(PI2 * t)
}

fn sine_in_out(t: f64) -> f64 {
    -f64::cos(std::f64::consts::PI * t) / 2.0 + 0.5
}

fn bounce_in(t: f64) -> f64 {
    1.0 - bounce_out(1.0 - t)
}

fn bounce_out(t: f64) -> f64 {
    if t < B1 {
        7.5625 * t * t
    } else if t < B2 {
        7.5625 * (t - B3) * (t - B3) + 0.75
    } else if t < B4 {
        7.5625 * (t - B5) * (t - B5) + 0.9375
    } else {
        7.5625 * (t - B6) * (t - B6) + 0.984_375
    }
}

fn bounce_in_out(t: f64) -> f64 {
    if t < 0.5 {
        (1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
    } else {
        (1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
    }
}

fn circ_in(t: f64) -> f64 {
    -((1.0 - t * t).sqrt() - 1.0)
}

fn circ_out(t: f64) -> f64 {
    (1.0 - (t - 1.0) * (t - 1.0)).sqrt()
}

fn circ_in_out(t: f64) -> f64 {
    if t <= 0.5 {
        ((1.0 - t * t * 4.0).sqrt() - 1.0) / -2.0
    } else {
        let t = t * 2.0 - 2.0;
        ((1.0 - t * t).sqrt() + 1.0) / 2.0
    }
}

fn expo_in(t: f64) -> f64 {
    2.0_f64.powf(10.0 * (t - 1.0))
}

fn expo_out(t: f64) -> f64 {
    -2.0_f64.powf(-10.0 * t) + 1.0
}

fn expo_in_out(t: f64) -> f64 {
    if t < 0.5 {
        2.0_f64.powf(10.0 * (t * 2.0 - 1.0)) / 2.0
    } else {
        (-2.0_f64.powf(-10.0 * (t * 2.0 - 1.0)) + 2.0) / 2.0
    }
}

fn back_in(t: f64) -> f64 {
    t * t * (2.701_58 * t - 1.701_58)
}

fn back_out(t: f64) -> f64 {
    let t = t - 1.0;
    1.0 - t * t * (-2.701_58 * t - 1.701_58)
}

fn back_in_out(t: f64) -> f64 {
    let mut t = t * 2.0;
    if t < 1.0 {
        t * t * (2.701_58 * t - 1.701_58) / 2.0
    } else {
        t -= 1.0;
        let t = t - 1.0;
        (1.0 - t * t * (-2.701_58 * t - 1.701_58)) / 2.0 + 0.5
    }
}

fn elastic_in(t: f64) -> f64 {
    let t = t - 1.0;
    -(ELASTIC_AMPLITUDE
        * 2.0_f64.powf(10.0 * t)
        * f64::sin(
            (t - (ELASTIC_PERIOD / (2.0 * std::f64::consts::PI)
                * (1.0 / ELASTIC_AMPLITUDE).asin()))
                * (2.0 * std::f64::consts::PI)
                / ELASTIC_PERIOD,
        ))
}

fn elastic_out(t: f64) -> f64 {
    ELASTIC_AMPLITUDE
        * 2.0_f64.powf(-10.0 * t)
        * f64::sin(
            (t - (ELASTIC_PERIOD / (2.0 * std::f64::consts::PI)
                * (1.0 / ELASTIC_AMPLITUDE).asin()))
                * (2.0 * std::f64::consts::PI)
                / ELASTIC_PERIOD,
        )
        + 1.0
}

fn elastic_in_out(t: f64) -> f64 {
    if t < 0.5 {
        let t = t - 0.5;
        -0.5 * (2.0_f64.powf(10.0 * t)
            * f64::sin((t - ELASTIC_PERIOD / 4.0) * (2.0 * std::f64::consts::PI) / ELASTIC_PERIOD))
    } else {
        let t = t - 0.5;
        2.0_f64.powf(-10.0 * t)
            * f64::sin((t - ELASTIC_PERIOD / 4.0) * (2.0 * std::f64::consts::PI) / ELASTIC_PERIOD)
            * 0.5
            + 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EASE_NAMES: [&str; 37] = [
        "linear",
        "quadIn",
        "quadOut",
        "quadInOut",
        "cubeIn",
        "cubeOut",
        "cubeInOut",
        "quartIn",
        "quartOut",
        "quartInOut",
        "quintIn",
        "quintOut",
        "quintInOut",
        "sineIn",
        "sineOut",
        "sineInOut",
        "expoIn",
        "expoOut",
        "expoInOut",
        "circIn",
        "circOut",
        "circInOut",
        "backIn",
        "backOut",
        "backInOut",
        "bounceIn",
        "bounceOut",
        "bounceInOut",
        "smoothStepIn",
        "smoothStepOut",
        "smoothStepInOut",
        "smootherStepIn",
        "smootherStepOut",
        "smootherStepInOut",
        "elasticIn",
        "elasticOut",
        "elasticInOut",
    ];

    fn close(context: &str, actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-12,
            "{context}: expected {expected}, got {actual}"
        );
    }

    /// Endpoint values matching the transcribed flixel 6.1.2 formulas
    /// (FlxEase.hx L190-203, L224-244): the expo family starts/ends at
    /// 2^-10 offsets and two elastic variants overshoot — elasticIn(0) =
    /// -2^-10 (the `t -= 1` phase lands on a 5.5π sine) and
    /// elasticOut(1) = 1 + 2^-10 (a 4.5π sine) — unlike the exact 0/1 of
    /// the polynomial and elasticInOut families. Pinning the observed
    /// native values instead of an idealized endpoint invariant.
    fn endpoint(name: &str) -> (f64, f64) {
        const EPS: f64 = 9.765_625e-4; // 2^-10
        match name {
            "expoIn" => (EPS, 1.0),
            "expoOut" => (0.0, 1.0 - EPS),
            "expoInOut" => (EPS / 2.0, 1.0 - EPS / 2.0),
            "elasticIn" => (-EPS, 1.0),
            "elasticOut" => (0.0, 1.0 + EPS),
            _ => (0.0, 1.0),
        }
    }

    #[test]
    fn full_flx_ease_family_resolves_and_hits_endpoints() {
        for name in EASE_NAMES {
            let (resolved, function) =
                resolve_ease(name).unwrap_or_else(|| panic!("{name} must resolve"));
            assert_eq!(resolved, name);
            let (low, high) = endpoint(name);
            close(name, function(0.0), low);
            close(name, function(1.0), high);
        }
        assert!(resolve_ease("quadin").is_none(), "lookup is case-sensitive");
        assert!(resolve_ease("notAnEase").is_none());
    }

    #[test]
    fn representative_ease_formulas_match_the_transcription() {
        close("quadIn", quad_in(0.5), 0.25);
        close("quadOut", quad_out(0.5), 0.75);
        close("quadInOut", quad_in_out(0.25), 0.125);
        close("cubeIn", cube_in(0.5), 0.125);
        close("quartOut", quart_out(0.5), 1.0 - 0.5_f64.powi(4));
        close("quintIn", quint_in(0.5), 0.5_f64.powi(5));
        close("sineIn", sine_in(1.0), 1.0);
        close("sineOut", sine_out(0.0), 0.0);
        close("sineInOut", sine_in_out(0.5), 0.5);
        close("expoIn", expo_in(1.0), 1.0);
        close("expoOut", expo_out(0.0), 0.0);
        close("circIn", circ_in(1.0), 1.0);
        close("circOut", circ_out(0.0), 0.0);
        close("backIn", back_in(1.0), 1.0);
        close("backOut", back_out(0.0), 0.0);
        close("bounceOut", bounce_out(1.0), 1.0);
        close("smoothStepInOut", smooth_step_in_out(0.5), 0.5);
        close("smootherStepInOut", smoother_step_in_out(0.5), 0.5);
        close("elasticOut", elastic_out(0.0), 0.0);
        close("elasticIn", elastic_in(1.0), 1.0);
        close("elasticInOut", elastic_in_out(1.0), 1.0);
    }

    #[test]
    fn step_schedule_uses_the_change_at_the_fire_time() {
        let metadata = r#"{
            "version": "2.2.4",
            "songName": "Schedule",
            "timeChanges": [
                { "t": 0, "bpm": 100 },
                { "t": 1000, "bpm": 200 },
                { "t": 2000, "bpm": 50 }
            ]
        }"#;
        let parsed =
            oxide_data::parse_file(std::path::Path::new("metadata.json"), metadata.as_bytes())
                .expect("metadata parses");
        let oxide_data::Typed::Metadata(metadata) = parsed.typed else {
            panic!("metadata kind");
        };
        let schedule = StepSchedule::new(metadata.time_changes.as_deref());
        close("at0", schedule.at(0.0), 150.0);
        close("at999", schedule.at(999.0), 150.0);
        close("at1000", schedule.at(1000.0), 75.0);
        close("at2500", schedule.at(2500.0), 300.0);
        assert!(
            StepSchedule::default().at(0.0) == DEFAULT_STEP_LENGTH_MS,
            "the no-change fallback is the 100 bpm default"
        );
    }
}
