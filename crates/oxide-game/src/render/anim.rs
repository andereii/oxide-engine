//! Character animation bookkeeping for the M5 phase-2b slice renderer.
//!
//! The state machine reproduces `BaseCharacter`/`Bopper` scheduling described
//! in `tools/oracle/RENDER-BEHAVIOR.md` §5: dance gating on step boundaries,
//! sing-triggered animations, the `-hold` progression, `singTimeSteps`
//! (conductor steps, not seconds), the doubled miss timing and bf's
//! continuous-hold stop condition.
//!
//! Data-provided animation names are kept verbatim. A request for a name the
//! character does not list is recorded in [`CharacterScheduler::missing_animations`]
//! instead of being silently dropped; callers decide the fallback (for
//! example bf has no `-end`/`-alt` animations).

use std::collections::{BTreeMap, BTreeSet};

use super::geometry::DANCE_STEPS_PER_BEAT;

/// `Bopper.ignoreExclusionPref` with the `BaseCharacter.new` override
/// (`BaseCharacter.hx:174`): a locked character still plays animations whose
/// names begin with one of these prefixes (Bopper.hx L267-279), covering both
/// `sing{DIR}` and `sing{DIR}miss`.
const IGNORE_EXCLUSION_PREFS: [&str; 1] = ["sing"];

/// `frameRate ?? 24` from the character animation data.
pub(crate) const DEFAULT_ANIMATION_FPS: f64 = 24.0;
/// Observed `CharacterData` default (`CharacterData.hx:451/593-596`
/// `DEFAULT_SINGTIME = 8.0`) used when a character omits `singTime`; dad's
/// explicit 8.0 is unchanged. At BPM 100 one step is 150 ms, so 8 steps is
/// 1.2 s.
pub(crate) const DEFAULT_SING_TIME_STEPS: f64 = 8.0;
/// `danceEvery` default (Bopper).
pub(crate) const DEFAULT_DANCE_EVERY: f64 = 1.0;
/// Note direction names in V-Slice `NoteDirection` order (LEFT/DOWN/UP/RIGHT).
pub(crate) const DIRECTION_NAMES: [&str; 4] = ["LEFT", "DOWN", "UP", "RIGHT"];

/// One bound character/prop animation resolved from its sheet data.
///
/// `frames` are indices into the selected sheet's own frame space: sprite
/// sheets use document-order frame indices, animate sheets use main-timeline
/// frames (or symbol-timeline frames when `symbol` is set). `sheet` selects
/// the loaded sheet; `0` is the character/prop main sheet.
#[derive(Debug, Clone)]
pub(crate) struct AnimationDef {
    /// Frames in playback order (data `frameIndices` reorder them).
    pub(crate) frames: Vec<usize>,
    /// Playback rate; `frameRate ?? 24`.
    pub(crate) frame_rate: f64,
    /// Whether the animation wraps; `looped ?? false`.
    pub(crate) looped: bool,
    /// Per-animation offsets used by the Bopper position shift. `None` when
    /// the data has no animation override (the global offsets then apply, so
    /// the observable shift is zero for characters without per-anim offsets).
    pub(crate) offsets: Option<(f64, f64)>,
    /// Index into the owner's loaded sheets (main sheet is `0`).
    pub(crate) sheet: usize,
    /// `animType: "symbol"` animation: the symbol timeline to resolve.
    pub(crate) symbol: Option<String>,
    /// Per-animation frame flip flags (`flipX/flipY ?? false`), applied to
    /// sprite-sheet frames.
    pub(crate) flip_x: bool,
    pub(crate) flip_y: bool,
}

impl AnimationDef {
    /// Main-sheet animation definition for tests and simple constructors.
    #[cfg(test)]
    pub(crate) fn test(frames: &[usize], frame_rate: f64, looped: bool) -> Self {
        Self {
            frames: frames.to_vec(),
            frame_rate,
            looped,
            offsets: None,
            sheet: 0,
            symbol: None,
            flip_x: false,
            flip_y: false,
        }
    }

    fn frame_index(&self, elapsed_seconds: f64) -> usize {
        if self.frames.is_empty() {
            return 0;
        }
        let frame_rate = if self.frame_rate > 0.0 {
            self.frame_rate
        } else {
            DEFAULT_ANIMATION_FPS
        };
        let raw = (elapsed_seconds * frame_rate).floor().max(0.0);
        let raw = if raw >= usize::MAX as f64 {
            usize::MAX
        } else {
            raw as usize
        };
        if self.looped {
            raw % self.frames.len()
        } else {
            raw.min(self.frames.len() - 1)
        }
    }

    fn finished(&self, elapsed_seconds: f64) -> bool {
        if self.looped || self.frames.is_empty() {
            return false;
        }
        let frame_rate = if self.frame_rate > 0.0 {
            self.frame_rate
        } else {
            DEFAULT_ANIMATION_FPS
        };
        elapsed_seconds * frame_rate >= self.frames.len() as f64
    }
}

#[derive(Debug, Clone)]
struct Playing {
    name: String,
    elapsed_seconds: f64,
}

/// Per-character animation scheduler.
#[derive(Debug, Clone)]
pub(crate) struct CharacterScheduler {
    animations: BTreeMap<String, AnimationDef>,
    current: Option<Playing>,
    has_danced: bool,
    dance_every: f64,
    sing_time_steps: f64,
    /// bf's `!isHoldingNote()` requirement in the stop-singing condition.
    requires_hold_release: bool,
    holding: bool,
    hold_timer_seconds: f64,
    /// `Bopper.canPlayOtherAnims`: set false by a forced `playAnimation` and
    /// reopened by the native `onAnimationFinished` when the fired animation
    /// ends (Bopper.hx L250/L261-292, L110-117).
    locked: bool,
    missing_requests: BTreeSet<String>,
}

impl CharacterScheduler {
    pub(crate) fn new(
        animations: BTreeMap<String, AnimationDef>,
        dance_every: f64,
        sing_time_steps: f64,
        requires_hold_release: bool,
    ) -> Self {
        Self {
            animations,
            current: None,
            has_danced: false,
            dance_every,
            sing_time_steps,
            requires_hold_release,
            holding: false,
            hold_timer_seconds: 0.0,
            locked: false,
            missing_requests: BTreeSet::new(),
        }
    }

    pub(crate) fn has(&self, name: &str) -> bool {
        self.animations.contains_key(name)
    }

    pub(crate) fn current_name(&self) -> Option<&str> {
        self.current.as_ref().map(|playing| playing.name.as_str())
    }

    pub(crate) fn current_animation(&self) -> Option<&AnimationDef> {
        self.current
            .as_ref()
            .and_then(|playing| self.animations.get(&playing.name))
    }

    /// All names requested by the state machine that this character does not
    /// list. Recorded for tests and implementation reporting.
    #[cfg(test)]
    pub(crate) fn missing_animations(&self) -> &BTreeSet<String> {
        &self.missing_requests
    }

    /// Starts or continues `name`. Returns `false` (and records the request)
    /// when neither the name nor a suffix-stripped fallback is defined.
    ///
    /// Native `Bopper.correctAnimationName` rule: when an animation name does
    /// not exist, strip one trailing `-<suffix>` and retry recursively (e.g.
    /// `danceLeft-scared` -> `danceLeft`). `restart=false` keeps the current
    /// playhead only when the same animation is already current.
    ///
    /// Also honors `Bopper.canPlayOtherAnims`: while a forced animation is
    /// locked in, only a restart of the CURRENT animation name is allowed.
    pub(crate) fn play(&mut self, name: &str, restart: bool) -> bool {
        self.play_internal(name, restart, false)
    }

    /// `Bopper.playAnimation(name, restart, ignoreOther)` with
    /// `ignoreOther = true`: the played animation locks other requests until
    /// it finishes (`canPlayOtherAnims = false`).
    pub(crate) fn play_with_ignore_other(
        &mut self,
        name: &str,
        restart: bool,
        ignore_other: bool,
    ) -> bool {
        self.play_internal(name, restart, ignore_other)
    }

    /// Plain `FlxSprite.animation.play(name, restart)` for stage props:
    /// exact-name lookup with NO Bopper suffix stripping and NO lock
    /// (StageProp is not a Bopper).
    pub(crate) fn play_exact(&mut self, name: &str, restart: bool) -> bool {
        let Some(animation) = self.animations.get(name) else {
            self.missing_requests.insert(name.to_string());
            return false;
        };
        let empty_frames = animation.frames.is_empty();
        self.select_current(name, empty_frames, restart);
        true
    }

    fn play_internal(&mut self, name: &str, restart: bool, ignore_other: bool) -> bool {
        self.release_finished_lock();
        if self.locked
            && !self
                .current_name()
                .is_some_and(|current| current == name && restart)
            && !IGNORE_EXCLUSION_PREFS
                .iter()
                .any(|entry| name.starts_with(entry))
        {
            // Native `playAnimation`: a locked Bopper ignores every request
            // that is neither a restart of the current animation nor covered
            // by an `ignoreExclusionPref` entry (Bopper.hx L261-279: a
            // detected entry skips the early return and plays immediately).
            return false;
        }
        let Some(animation) = self.animations.get(name) else {
            if let Some((base, _suffix)) = name.rsplit_once('-') {
                return self.play_internal(base, restart, ignore_other);
            }
            self.missing_requests.insert(name.to_string());
            return false;
        };
        let empty_frames = animation.frames.is_empty();
        self.select_current(name, empty_frames, restart);
        if ignore_other {
            self.locked = true;
        }
        true
    }

    fn select_current(&mut self, name: &str, empty_frames: bool, restart: bool) {
        if empty_frames {
            return;
        }
        let same = self
            .current
            .as_ref()
            .is_some_and(|playing| playing.name == name);
        if !same || restart {
            self.current = Some(Playing {
                name: name.to_string(),
                elapsed_seconds: 0.0,
            });
        }
    }

    /// Native `onAnimationFinished`: reopen the lock once the fired
    /// (non-looped) animation has finished. Checked lazily at the next request
    /// and at the end of [`Self::update`].
    fn release_finished_lock(&mut self) {
        if self.locked && self.current_finished() {
            self.locked = false;
        }
    }

    #[cfg(test)]
    pub(crate) fn locked(&self) -> bool {
        self.locked
    }

    fn record_missing(&mut self, name: &str) {
        self.missing_requests.insert(name.to_string());
    }

    /// Current animation frame as an index into the selected sheet's frame
    /// space (main-timeline frame, symbol frame or sprite frame).
    #[cfg(test)]
    pub(crate) fn current_frame(&self) -> Option<usize> {
        let playing = self.current.as_ref()?;
        let animation = self.animations.get(&playing.name)?;
        animation
            .frames
            .get(animation.frame_index(playing.elapsed_seconds))
            .copied()
    }

    /// The current animation definition plus its selected sheet index.
    pub(crate) fn current_visual_frame(&self) -> Option<(&AnimationDef, usize)> {
        let playing = self.current.as_ref()?;
        let animation = self.animations.get(&playing.name)?;
        let frame = animation
            .frames
            .get(animation.frame_index(playing.elapsed_seconds))
            .copied()?;
        Some((animation, frame))
    }

    fn current_finished(&self) -> bool {
        match (&self.current, self.current_animation()) {
            (Some(playing), Some(animation)) => animation.finished(playing.elapsed_seconds),
            _ => false,
        }
    }

    fn is_singing(&self) -> bool {
        self.current_name()
            .is_some_and(|name| name.starts_with("sing") && !name.ends_with("-end"))
    }

    /// `Bopper.onStepHit` dance gate and the `BaseCharacter.dance` checks.
    pub(crate) fn on_step(&mut self, step: i32) {
        if self.dance_every > 0.0
            && (step as f64 % (self.dance_every * DANCE_STEPS_PER_BEAT)) == 0.0
        {
            self.dance(false);
        }
    }

    /// `Bopper.onStepHit` dance gate. Stage props pass `shouldBop = true`, so
    /// a qualifying step calls `dance(true)` (forceRestart) instead of the
    /// character `dance(false)` path.
    pub(crate) fn on_step_forced(&mut self, step: i32) {
        if self.dance_every > 0.0
            && (step as f64 % (self.dance_every * DANCE_STEPS_PER_BEAT)) == 0.0
        {
            self.dance(true);
        }
    }

    /// Plays `startingAnimation` first when the data defines one. Returns
    /// whether a starting animation was started; absence is not an error (the
    /// native constructor simply leaves the first frame showing).
    pub(crate) fn start_animation(&mut self, starting: Option<&str>) -> bool {
        starting.is_some_and(|name| self.play(name, true))
    }

    /// `BaseCharacter.dance(force)`.
    pub(crate) fn dance(&mut self, force: bool) -> bool {
        if self.is_singing() {
            return false;
        }
        if !force
            && let Some(name) = self.current_name()
            && !(name.starts_with("dance") || name.starts_with("idle"))
            && !self.current_finished()
        {
            return false;
        }
        self.select_dance(force)
    }

    /// The stop-singing path calls `dance(true)` after the current animation
    /// has already been identified as a sing animation; the singing guard is
    /// bypassed there, otherwise the character would be stuck on the last
    /// sing frame forever.
    fn dance_forced(&mut self) -> bool {
        self.select_dance(true)
    }

    fn select_dance(&mut self, force: bool) -> bool {
        if self.has("danceLeft") {
            let next = if self.has_danced {
                "danceRight"
            } else {
                "danceLeft"
            };
            if !self.has(next) {
                self.record_missing(next);
                return false;
            }
            self.has_danced = !self.has_danced;
            return self.play(next, true);
        }
        let restart = force || self.current_finished();
        self.play("idle", restart)
    }

    /// Plays `sing{DIR}` or `sing{DIR}miss` with restart, as the note events
    /// do (RENDER-BEHAVIOR §5).
    pub(crate) fn on_sing(&mut self, direction: usize, miss: bool) -> bool {
        let Some(direction_name) = DIRECTION_NAMES.get(direction) else {
            return false;
        };
        let name = if miss {
            format!("sing{direction_name}miss")
        } else {
            format!("sing{direction_name}")
        };
        self.hold_timer_seconds = 0.0;
        self.play(&name, true)
    }

    /// Native `PlayState.processNotes` (PlayState.hx L2838-2852): while an
    /// opponent hold note is active (`hitNote && !missedNote &&
    /// sustainLength > 0`), every frame resets `getDad().holdTimer = 0` (when
    /// the opponent is singing), which keeps the sing animation alive through
    /// long sustains. Folded into one call from the scene step; a no-op when
    /// the character is not singing (the native `if (...isSinging())` guard).
    pub(crate) fn reset_hold_timer_under_opponent_hold(&mut self) {
        if self.is_singing() {
            self.hold_timer_seconds = 0.0;
        }
    }

    pub(crate) fn set_holding(&mut self, holding: bool) {
        self.holding = holding;
    }

    /// One 1 ms scene tick (RENDER-BEHAVIOR §6 item 4).
    pub(crate) fn update(&mut self, elapsed_seconds: f64, step_length_ms: f64) {
        if let Some(playing) = &mut self.current {
            playing.elapsed_seconds += elapsed_seconds;
        }
        // Native `onAnimationFinished` reopens `canPlayOtherAnims` when the
        // forced animation finishes.
        self.release_finished_lock();

        // 1. finished sing/idle animations continue through their `-hold`
        //    form when the data provides one.
        if self.current_finished()
            && let Some(name) = self.current_name()
            && !name.ends_with("-hold")
        {
            let hold = format!("{name}-hold");
            if self.has(&hold) {
                self.play(&hold, true);
            }
        }

        // 2. sing hold timer and the stop condition.
        if self.is_singing() {
            self.hold_timer_seconds += elapsed_seconds;
            let mut sing_time_seconds = self.sing_time_steps * step_length_ms / 1000.0;
            if self
                .current_name()
                .is_some_and(|name| name.ends_with("miss"))
            {
                sing_time_seconds *= 2.0;
            }
            let should_stop = !self.requires_hold_release || !self.holding;
            if self.hold_timer_seconds > sing_time_seconds && should_stop {
                let current = self.current_name().unwrap_or_default().to_string();
                let base = current
                    .strip_suffix("-hold")
                    .unwrap_or(current.as_str())
                    .to_string();
                let end = format!("{base}-end");
                if self.has(&end) {
                    self.play(&end, true);
                } else {
                    // bf has no `-end`; the fallback is a forced dance.
                    self.dance_forced();
                }
                self.hold_timer_seconds = 0.0;
            }
        } else {
            // 3. not singing: the timer is inert.
            self.hold_timer_seconds = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn animation(frames: &[usize], looped: bool) -> AnimationDef {
        AnimationDef::test(frames, 24.0, looped)
    }

    fn scheduler(defs: &[(&str, AnimationDef)], sing_time_steps: f64) -> CharacterScheduler {
        let mut animations = BTreeMap::new();
        for (name, def) in defs {
            animations.insert((*name).to_string(), def.clone());
        }
        CharacterScheduler::new(animations, 1.0, sing_time_steps, false)
    }

    #[test]
    fn bopper_forced_step_gate_restarts_alternation_and_starts_left() {
        let mut sched = scheduler(
            &[
                ("idle", animation(&[0, 1], false)),
                ("danceLeft", animation(&[10, 11], false)),
                ("danceRight", animation(&[20, 21], false)),
            ],
            4.0,
        );
        // startingAnimation plays first and does not consume the alternation.
        assert!(sched.start_animation(Some("danceRight")));
        assert_eq!(sched.current_name(), Some("danceRight"));
        // DanceEvery 1 -> every 4 steps; first forced dance still goes LEFT.
        sched.on_step(3);
        assert_eq!(sched.current_name(), Some("danceRight"));
        sched.on_step(4);
        assert_eq!(sched.current_name(), Some("danceLeft"));
        sched.on_step(8);
        assert_eq!(sched.current_name(), Some("danceRight"));
        // Non-zero danceEvery 2 -> every 8 steps.
        let mut animations = BTreeMap::new();
        animations.insert("danceLeft".to_string(), animation(&[10], false));
        let mut slow = CharacterScheduler::new(animations, 2.0, 4.0, false);
        slow.on_step(4);
        assert_eq!(slow.current_name(), None);
        slow.on_step(8);
        assert_eq!(slow.current_name(), Some("danceLeft"));
    }

    #[test]
    fn missing_suffixed_animation_falls_back_to_the_base_name() {
        let mut sched = scheduler(
            &[
                ("danceLeft", animation(&[10], false)),
                ("idle", animation(&[0], false)),
            ],
            4.0,
        );
        assert!(sched.play("danceLeft-scared", true));
        assert_eq!(sched.current_name(), Some("danceLeft"));
        assert!(sched.play("idle-tired-scared", true));
        assert_eq!(sched.current_name(), Some("idle"));
    }

    #[test]
    fn existing_suffixed_animation_resolves_to_itself() {
        // `correctAnimationName` only strips a suffix when the requested
        // animation does NOT exist; an exact `danceLeft-scared` entry wins.
        let mut sched = scheduler(
            &[
                ("danceLeft", animation(&[10], false)),
                ("danceLeft-scared", animation(&[7, 8], false)),
            ],
            4.0,
        );
        assert!(sched.play("danceLeft-scared", true));
        assert_eq!(sched.current_name(), Some("danceLeft-scared"));
        assert_eq!(sched.current_frame(), Some(7));
    }

    #[test]
    fn dance_alternates_starting_left() {
        let mut sched = scheduler(
            &[
                ("idle", animation(&[0, 1], false)),
                ("danceLeft", animation(&[10, 11], false)),
                ("danceRight", animation(&[20, 21], false)),
            ],
            4.0,
        );
        sched.play("idle", true);
        sched.on_step(4);
        assert_eq!(sched.current_name(), Some("danceLeft"));
        sched.on_step(8);
        assert_eq!(sched.current_name(), Some("danceRight"));
        sched.on_step(12);
        assert_eq!(sched.current_name(), Some("danceLeft"));
    }

    #[test]
    fn dance_gate_requires_beat_boundaries_and_not_singing() {
        let mut sched = scheduler(
            &[
                ("danceLeft", animation(&[10], false)),
                ("danceRight", animation(&[20], false)),
                ("singLEFT", animation(&[1], false)),
            ],
            4.0,
        );
        sched.on_step(3);
        assert_eq!(sched.current_name(), None);
        sched.on_sing(0, false);
        assert_eq!(sched.current_name(), Some("singLEFT"));
        sched.on_step(4);
        assert_eq!(sched.current_name(), Some("singLEFT"));
    }

    #[test]
    fn finished_animation_progresses_into_hold() {
        let mut sched = scheduler(
            &[
                ("singLEFT", AnimationDef::test(&[1, 2], 24.0, false)),
                ("singLEFT-hold", animation(&[3, 4, 5], true)),
            ],
            8.0,
        );
        sched.on_sing(0, false);
        sched.update(0.05, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT"));
        sched.update(0.05, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT-hold"));
        assert_eq!(sched.current_frame(), Some(3));
    }

    #[test]
    fn default_sing_time_is_eight_steps_at_bpm_100() {
        assert_eq!(DEFAULT_SING_TIME_STEPS, 8.0);
        let mut sched = scheduler(
            &[
                ("singLEFT", animation(&[1], false)),
                ("danceLeft", animation(&[10], false)),
            ],
            DEFAULT_SING_TIME_STEPS,
        );
        sched.on_sing(0, false);
        // 8 steps * 150 ms = 1.2 s; the comparison is strict (>).
        sched.update(1.199, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT"));
        sched.update(0.002, 150.0);
        assert_eq!(sched.current_name(), Some("danceLeft"));
    }

    #[test]
    fn stop_singing_after_sing_time_steps() {
        let mut sched = scheduler(
            &[
                ("singLEFT", animation(&[1], false)),
                ("danceLeft", animation(&[10], false)),
                ("danceRight", animation(&[20], false)),
            ],
            4.0,
        );
        sched.on_sing(0, false);
        // 4 steps of 150 ms = 0.6 s; the comparison is strict (>).
        sched.update(0.600, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT"));
        sched.update(0.001, 150.0);
        assert_eq!(sched.current_name(), Some("danceLeft"));
    }

    #[test]
    fn miss_timing_is_doubled() {
        let mut sched = scheduler(
            &[
                ("singLEFTmiss", animation(&[1], false)),
                ("danceLeft", animation(&[10], false)),
            ],
            4.0,
        );
        sched.on_sing(0, true);
        sched.update(0.600, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFTmiss"));
        // Doubled timing: 4 steps * 150 ms * 2 = 1.2 s, strict `>`.
        sched.update(0.601, 150.0);
        assert_eq!(sched.current_name(), Some("danceLeft"));
    }

    #[test]
    fn bf_hold_requires_release_before_stop() {
        let mut animations = BTreeMap::new();
        animations.insert("singLEFT".to_string(), animation(&[1], false));
        animations.insert("danceLeft".to_string(), animation(&[10], false));
        let mut sched = CharacterScheduler::new(animations, 1.0, 4.0, true);
        sched.set_holding(true);
        sched.on_sing(0, false);
        sched.update(1.0, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT"));
        sched.set_holding(false);
        sched.update(0.001, 150.0);
        assert_eq!(sched.current_name(), Some("danceLeft"));
    }

    #[test]
    fn held_animation_strips_hold_before_end_or_dance() {
        let mut sched = scheduler(
            &[
                ("singLEFT", animation(&[1], false)),
                ("singLEFT-hold", animation(&[2], true)),
                ("singLEFT-end", animation(&[3], false)),
            ],
            4.0,
        );
        sched.on_sing(0, false);
        sched.update(0.05, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT-hold"));
        sched.update(0.600, 150.0);
        assert_eq!(sched.current_name(), Some("singLEFT-end"));
    }

    #[test]
    fn idle_restarts_when_finished_but_continues_otherwise() {
        let mut sched = scheduler(&[("idle", animation(&[0, 1, 2, 3], false))], 4.0);
        assert!(sched.play("idle", true));
        sched.update(0.010, 150.0);
        sched.on_step(4);
        assert_eq!(sched.current_frame(), Some(0));
        sched.update(0.500, 150.0);
        sched.on_step(8);
        assert_eq!(sched.current_frame(), Some(0));
    }

    #[test]
    fn missing_animation_requests_are_recorded() {
        let mut sched = scheduler(&[("idle", animation(&[0], false))], 4.0);
        assert!(!sched.on_sing(3, true));
        assert!(sched.missing_animations().contains("singRIGHTmiss"));
        assert_eq!(sched.current_name(), None);
    }

    /// `Bopper.playAnimation(anim, force, force)` (`PlayAnimationSongEvent`
    /// L59-66): the forced animation locks other requests until it finishes;
    /// a restart of the current name is still allowed.
    #[test]
    fn forced_animation_locks_other_requests_until_it_finishes() {
        let mut sched = scheduler(
            &[
                ("idle", animation(&[0, 1], true)),
                ("hey", animation(&[5, 6], false)),
                ("singLEFT", animation(&[1], false)),
            ],
            4.0,
        );
        sched.play("idle", true);
        assert!(sched.play_with_ignore_other("hey", true, true));
        assert_eq!(sched.current_name(), Some("hey"));
        assert!(sched.locked());

        // A dance animation is ignored while the forced animation runs, but
        // sing animations bypass via `ignoreExclusionPref = ['sing']`
        // (BaseCharacter.hx:174) and play immediately; the lock persists
        // (the sing call passes `ignoreOther = false`).
        assert!(!sched.play("idle", true));
        assert!(sched.on_sing(0, false));
        assert_eq!(sched.current_name(), Some("singLEFT"));
        assert!(sched.locked());
        // Restarting a NON-current non-sing animation ('hey' is no longer
        // current after the bypasssing sing) is dropped per the native gate.
        assert!(!sched.play_with_ignore_other("hey", true, true));
        assert_eq!(sched.current_name(), Some("singLEFT"));
        // Restarting the CURRENT animation (the sing) stays allowed.
        assert!(sched.on_sing(0, false));
        assert_eq!(sched.current_name(), Some("singLEFT"));
        assert!(sched.locked());

        // `singLEFT` is one frame at 24 fps (41.7 ms); the lock reopens once
        // the sing finishes and the dance caller works again.
        sched.update(0.05, 150.0);
        assert!(!sched.locked());
        assert!(sched.play("idle", true));
        assert_eq!(sched.current_name(), Some("idle"));
    }

    /// `ignoreExclusionPref = ['sing']` (`BaseCharacter.hx:174`): both
    /// `sing{DIR}` and `sing{DIR}miss` play immediately while locked, and
    /// the lock only reopens after the fired animation finishes.
    #[test]
    fn sing_prefix_bypasses_the_forced_animation_lock() {
        let mut sched = scheduler(
            &[
                ("idle", animation(&[0, 1], true)),
                ("hey", animation(&[5, 6], false)),
                ("singLEFT", animation(&[1], false)),
                ("singRIGHTmiss", animation(&[1], false)),
                ("hey-alt", animation(&[5, 6], false)),
            ],
            8.0,
        );
        sched.play("idle", true);
        assert!(sched.play_with_ignore_other("hey", true, true));
        assert!(sched.locked());
        // sing{DIR} plays through the lock...
        assert!(sched.on_sing(0, false));
        assert_eq!(sched.current_name(), Some("singLEFT"));
        // ...as does sing{DIR}miss (same 'sing' prefix), which also exercises
        // the restart of a DIFFERENT animation under the lock.
        assert!(sched.on_sing(3, true));
        assert_eq!(sched.current_name(), Some("singRIGHTmiss"));
        assert!(sched.locked());
        // Non-sing names are still dropped.
        assert!(!sched.play("hey-alt", true));
        assert_eq!(sched.current_name(), Some("singRIGHTmiss"));
        // The lock reopens when the currently-playing animation finishes.
        sched.update(0.05, 150.0);
        assert!(!sched.locked());
        assert!(sched.play("hey-alt", true));
        assert_eq!(sched.current_name(), Some("hey-alt"));
    }

    /// Native `PlayState.processNotes` reset (PlayState.hx L2838-2852): the
    /// opponent hold timer restarts while the hold is active, so the sing
    /// animation survives long sustains and only stops `singTimeSeconds`
    /// after the hold ends.
    #[test]
    fn opponent_hold_reset_keeps_long_sustains_singing() {
        let mut sched = scheduler(
            &[
                ("idle", animation(&[0, 1], true)),
                ("singLEFT", animation(&[1], false)),
            ],
            8.0,
        );
        // Default sing time = 8 steps * 150 ms = 1.2 s; the hold resets the
        // timer every frame, so 2 s of accrual never stops the sing.
        assert!(sched.on_sing(0, false));
        sched.update(0.001, 150.0);
        for _ in 0..2000 {
            sched.reset_hold_timer_under_opponent_hold();
            sched.update(0.001, 150.0);
        }
        assert_eq!(sched.current_name(), Some("singLEFT"));
        // Once the hold ends (no more resets), the timer completes its
        // `singTimeSeconds` and the character stops singing.
        for _ in 0..1400 {
            sched.update(0.001, 150.0);
        }
        assert_eq!(sched.current_name(), Some("idle"));
    }

    /// StageProp uses `animation.play(anim, force)` directly: no suffix
    /// stripping and no Bopper lock.
    #[test]
    fn prop_exact_play_does_not_strip_suffixes() {
        let mut sched = scheduler(&[("wave", animation(&[0, 1], true))], 4.0);
        assert!(sched.play_exact("wave", true));
        assert_eq!(sched.current_name(), Some("wave"));
        // The character path would strip `-alt` to `wave`; the prop path must
        // keep the exact name and fail softly.
        assert!(!sched.play_exact("wave-alt", true));
        assert_eq!(sched.current_name(), Some("wave"));
        assert!(sched.missing_animations().contains("wave-alt"));
    }

    #[test]
    fn hit_direction_names_are_verbatim() {
        for (direction, expected) in DIRECTION_NAMES.iter().enumerate() {
            let mut sched = scheduler(&[], 4.0);
            sched.on_sing(direction, false);
            assert!(
                sched
                    .missing_animations()
                    .contains(&format!("sing{expected}"))
            );
            sched.on_sing(direction, true);
            assert!(
                sched
                    .missing_animations()
                    .contains(&format!("sing{expected}miss"))
            );
        }
    }
}
