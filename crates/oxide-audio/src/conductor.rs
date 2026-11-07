//! Pure port of `funkin/Conductor.hx` timing semantics. Timing values are
//! milliseconds; all arithmetic is `f64` (Haxe `Float` is an IEEE-754 double).
//! `FlxMath.roundDecimal` uses `Math.fround(value * mult) / mult`
//! (`FlxMath.hx:68-76`). On the reference hxcpp runtime, `fround` means
//! `floor(x + 0.5)`, not a binary32 conversion. ADR-0014 corrects the earlier
//! interpretation; real-utility probe evidence is in `tools/oracle/BEHAVIOR.md`.
//! This module has no audio backend and no game dependencies.
//!
//! Line references point at `~/Descargas/fnf/Funkin/source/funkin/Conductor.hx`
//! unless stated otherwise.
//!
//! Deliberately out of scope (see the M4 contract): `bpmOverride` /
//! `forceBPM` (`Conductor.hx:158-161`, `398-410`), the `PlayState.startingSong`
//! special case in `get_bpm` (`Conductor.hx:128-137`), `songPositionDelta` /
//! `getTimeWithDelta`, and the FlxSignal singleton wiring. The port covers the
//! instance update behaviour without those paths.

/// `Constants.STEPS_PER_BEAT` (`funkin/util/Constants.hx:424`): one beat is
/// four steps, and a step is a sixteenth note in 4/4.
pub const STEPS_PER_BEAT: u32 = 4;

/// `Constants.SECS_PER_MIN` (`funkin/util/Constants.hx:368`).
pub const SECS_PER_MIN: f64 = 60.0;

/// `Constants.MS_PER_SEC` (`funkin/util/Constants.hx:373`).
pub const MS_PER_SEC: f64 = 1000.0;

/// `Constants.DEFAULT_BPM` (`funkin/util/Constants.hx:261`).
const DEFAULT_BPM: f64 = 100.0;

/// `Constants.DEFAULT_TIME_SIGNATURE_NUM` (`funkin/util/Constants.hx:326`).
const DEFAULT_TIME_SIGNATURE_NUM: i32 = 4;

/// `Constants.DEFAULT_TIME_SIGNATURE_DEN` (`funkin/util/Constants.hx:331`).
const DEFAULT_TIME_SIGNATURE_DEN: i32 = 4;

/// `FlxMath.roundDecimal` (`FlxMath.hx:68-76`), exactly as the source defines it:
/// `Math.fround(value * mult) / mult`, with `mult` built by repeated `* 10`.
///
/// hxcpp's `include/hxMath.h:28` defines `Math.fround(x)` as `floor(x + 0.5)`:
/// ties go toward positive infinity, including negative ties. Everything remains
/// f64, including the integer-valued intermediate (no i32 or f32 conversion).
fn round_decimal(value: f64, digits: i32) -> f64 {
    let mut multiplier = 1.0;
    for _ in 0..digits {
        multiplier *= 10.0;
    }
    (value * multiplier + 0.5).floor() / multiplier
}

/// Mirrors `SongData.SongTimeChange` (aliased in `oxide-data`).
#[derive(Debug, Clone)]
pub struct TimeChange {
    /// Timestamp in milliseconds. Normalised to `>= 0` by
    /// [`Conductor::map_time_changes`].
    pub time_stamp: f64,
    /// Beats elapsed when this change starts. Normalised/recomputed by
    /// [`Conductor::map_time_changes`].
    pub beat_time: f64,
    pub bpm: f64,
    /// Default 4 (`Conductor.hx:197-204`).
    pub time_signature_num: i32,
    /// Default 4 (`Conductor.hx:209-216`).
    pub time_signature_den: i32,
}

impl TimeChange {
    /// `(((SECS_PER_MIN / bpm) * MS_PER_SEC) * (4 / timeSignatureDen)) /
    /// STEPS_PER_BEAT`, the local formula repeated at `Conductor.hx:632`,
    /// `640`, `688`, `733`, `777` and `815`.
    fn step_length_ms(&self) -> f64 {
        (((SECS_PER_MIN / self.bpm) * MS_PER_SEC) * (4.0 / f64::from(self.time_signature_den)))
            / f64::from(STEPS_PER_BEAT)
    }

    /// `timeSignatureNum * STEPS_PER_BEAT` (`Conductor.hx:633`, `641`, `681`,
    /// `689`).
    fn steps_per_measure(&self) -> f64 {
        f64::from(self.time_signature_num * STEPS_PER_BEAT as i32)
    }
}

/// Fully-resolved conductor offsets, kept separate from the global save file.
///
/// `audio_visual` mirrors `audioVisualOffset` (`Conductor.hx:282-292`) but is
/// *not* part of `combinedOffset`: the Haxe getter only sums instrumental,
/// format and global (`Conductor.hx:294-299`). The field exists because the
/// contract exposes it; callers decide whether the save option belongs in
/// `global`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ConductorOffsets {
    /// Chart/instrumental offset (`Conductor.hx:248-251`).
    pub instrumental: f64,
    /// File-format delay; 0 on desktop OGG (`docs/M4-CONTRACT.md` §0).
    pub format: f64,
    /// User audio/visual calibration option (`Conductor.hx:282-292`), not
    /// included in [`ConductorOffsets::combined`].
    pub audio_visual: f64,
    /// User input calibration (`Conductor.hx:275-287`).
    pub global: f64,
}

impl ConductorOffsets {
    /// `instrumentalOffset + formatOffset + globalOffset`
    /// (`Conductor.hx:294-299`). `audio_visual` is intentionally excluded.
    #[must_use]
    pub fn combined(&self) -> f64 {
        self.instrumental + self.format + self.global
    }
}

/// One dispatched integer boundary. `Conductor.hx:489-503` fires step, then
/// beat, then measure within a single update; the payload is the new index.
#[derive(Debug, Clone)]
pub enum Dispatch {
    Step(i32),
    Beat(i32),
    Measure(i32),
}

/// Conductor instance state. See module docs for what is intentionally omitted.
#[derive(Debug)]
pub struct Conductor {
    pub offsets: ConductorOffsets,
    /// Normalised time changes: sorted ascending, `time_stamp >= 0`,
    /// `beat_time` recomputed (`Conductor.hx:549-600`).
    time_changes: Vec<TimeChange>,
    /// Index into `time_changes`, or [`usize::MAX`] when the list is empty.
    current_change_index: usize,
    song_position: f64,
    current_step_time: f64,
    current_beat_time: f64,
    current_measure_time: f64,
    current_step: i32,
    current_beat: i32,
    current_measure: i32,
}

impl Conductor {
    #[must_use]
    pub fn new() -> Self {
        Self {
            offsets: ConductorOffsets::default(),
            time_changes: Vec::new(),
            current_change_index: usize::MAX,
            song_position: 0.0,
            current_step_time: 0.0,
            current_beat_time: 0.0,
            current_measure_time: 0.0,
            current_step: 0,
            current_beat: 0,
            current_measure: 0,
        }
    }

    /// `Conductor.mapTimeChanges` (`Conductor.hx:549-600`).
    ///
    /// Sorts a copy ascending by `time_stamp` (stable, mirroring
    /// `SongDataUtils.sortTimeChanges`, `SongDataUtils.hx:267-275`), clamps
    /// negative timestamps to zero with `beat_time = 0` (`Conductor.hx:561-566`)
    /// and recomputes every positive entry from its predecessor
    /// (`Conductor.hx:569-579`):
    ///
    /// ```text
    /// beat_time = round4(prev.beat_time
    ///     + (ts - prev.time_stamp) * prev.bpm / SECS_PER_MIN / MS_PER_SEC
    ///     * (prev.time_signature_den / 4))
    /// ```
    ///
    /// The divider order `/ 60 / 1000` and the `* (den / 4)` factor follow the
    /// Haxe expression exactly. A positive first entry (no predecessor) keeps
    /// `beat_time = 0` (`Conductor.hx:570-572`).
    ///
    /// Haxe calls `update(this.songPosition, false)` at the end
    /// (`Conductor.hx:599`); we refresh the derived values without despatching,
    /// because this API cannot know whether music is playing and the method has
    /// no dispatch return channel.
    pub fn map_time_changes(&mut self, changes: &[TimeChange]) {
        let mut sorted = changes.to_vec();
        sorted.sort_by(|a, b| a.time_stamp.total_cmp(&b.time_stamp));

        let mut mapped: Vec<TimeChange> = Vec::with_capacity(sorted.len());
        for mut change in sorted {
            if change.time_stamp < 0.0 {
                change.time_stamp = 0.0;
            }

            if change.time_stamp <= 0.0 {
                change.beat_time = 0.0;
            } else {
                change.beat_time = 0.0;
                if let Some(previous) = mapped.last() {
                    change.beat_time = round_decimal(
                        previous.beat_time
                            + (change.time_stamp - previous.time_stamp) * previous.bpm
                                / SECS_PER_MIN
                                / MS_PER_SEC
                                * (f64::from(previous.time_signature_den) / 4.0),
                        4,
                    );
                }
            }

            mapped.push(change);
        }

        self.time_changes = mapped;
        self.current_change_index = self.select_current_change_index();
        self.recompute_times();
    }

    /// `Conductor.update` (`Conductor.hx:422-517`) minus the parts that depend
    /// on the concrete sound object (`songPositionDelta`, FlxSignal wiring).
    ///
    /// * `song_position = None` reuses the current position (stand-in for
    ///   `FlxG.sound.music.time`, `Conductor.hx:427-430`).
    /// * `apply_offsets` adds `combinedOffset` before the clamp
    ///   (`Conductor.hx:433`).
    /// * `music_playing` enables the length clamp
    ///   `min(max(songPos, min(combinedOffset, 0)), music_len_ms)`
    ///   (`Conductor.hx:440-444`); when false the position is used verbatim
    ///   (`Conductor.hx:445-448`).
    ///
    /// Returns the integer boundaries that changed in this call, in
    /// step → beat → measure order (`Conductor.hx:489-503`).
    pub fn update(
        &mut self,
        song_position: Option<f64>,
        apply_offsets: bool,
        music_playing: bool,
        music_len_ms: f64,
    ) -> Vec<Dispatch> {
        let mut song_pos = song_position.unwrap_or(self.song_position);
        if apply_offsets {
            song_pos += self.combined_offset();
        }

        let old_step = self.current_step;
        let old_beat = self.current_beat;
        let old_measure = self.current_measure;

        // Conductor.hx:442: Math.min(combinedOffset, 0).clamp(songPos, length).
        if music_playing {
            let lower_bound = self.combined_offset().min(0.0);
            self.song_position = song_pos.max(lower_bound).min(music_len_ms);
        } else {
            self.song_position = song_pos;
        }

        self.current_change_index = self.select_current_change_index();
        self.recompute_times();

        let mut dispatches = Vec::with_capacity(3);
        if self.current_step != old_step {
            dispatches.push(Dispatch::Step(self.current_step));
        }
        if self.current_beat != old_beat {
            dispatches.push(Dispatch::Beat(self.current_beat));
        }
        if self.current_measure != old_measure {
            dispatches.push(Dispatch::Measure(self.current_measure));
        }
        dispatches
    }

    /// Current position in milliseconds (`Conductor.songPosition`).
    #[must_use]
    pub fn song_position_ms(&self) -> f64 {
        self.song_position
    }

    /// `Conductor.get_bpm` (`Conductor.hx:121-140`) without `bpmOverride` and
    /// without the `PlayState.startingSong` re-selection.
    #[must_use]
    pub fn bpm(&self) -> f64 {
        self.current_time_change()
            .map_or(DEFAULT_BPM, |time_change| time_change.bpm)
    }

    /// `Conductor.get_startingBPM` (`Conductor.hx:145-155`).
    #[must_use]
    pub fn starting_bpm(&self) -> f64 {
        self.time_changes
            .first()
            .map_or(DEFAULT_BPM, |time_change| time_change.bpm)
    }

    /// `Conductor.get_measureLengthMs` (`Conductor.hx:166-171`).
    #[must_use]
    pub fn measure_length_ms(&self) -> f64 {
        self.beat_length_ms() * f64::from(self.time_signature_num())
    }

    /// `Conductor.get_beatLengthMs` (`Conductor.hx:176-182`).
    #[must_use]
    pub fn beat_length_ms(&self) -> f64 {
        ((SECS_PER_MIN / self.bpm()) * MS_PER_SEC) * (4.0 / f64::from(self.time_signature_den()))
    }

    /// `Conductor.get_stepLengthMs` (`Conductor.hx:187-192`).
    #[must_use]
    pub fn step_length_ms(&self) -> f64 {
        self.beat_length_ms() / f64::from(STEPS_PER_BEAT)
    }

    #[must_use]
    pub fn current_step(&self) -> i32 {
        self.current_step
    }

    #[must_use]
    pub fn current_beat(&self) -> i32 {
        self.current_beat
    }

    #[must_use]
    pub fn current_measure(&self) -> i32 {
        self.current_measure
    }

    #[must_use]
    pub fn current_step_time(&self) -> f64 {
        self.current_step_time
    }

    #[must_use]
    pub fn current_beat_time(&self) -> f64 {
        self.current_beat_time
    }

    #[must_use]
    pub fn current_measure_time(&self) -> f64 {
        self.current_measure_time
    }

    /// The selected `SongTimeChange`, if the timeline has any entries.
    #[must_use]
    pub fn current_time_change(&self) -> Option<&TimeChange> {
        if self.current_change_index == usize::MAX {
            None
        } else {
            self.time_changes.get(self.current_change_index)
        }
    }

    /// `Conductor.get_combinedOffset` (`Conductor.hx:294-299`).
    #[must_use]
    pub fn combined_offset(&self) -> f64 {
        self.offsets.combined()
    }

    /// `Conductor.getTimeInSteps` (`Conductor.hx:702-739`).
    pub fn get_time_in_steps(&self, ms: f64) -> f64 {
        let Some(first) = self.time_changes.first() else {
            // Conductor.hx:704-708: constant BPM fallback.
            return (ms / self.step_length_ms()).floor();
        };

        let mut result_step = 0.0;
        let ms = if ms < 0.0 { 0.0 } else { ms };

        let mut last_time_change = first;
        // Conductor.hx:715 starts `i` at -1 and increments at the end of the
        // body, so the `i == length - 1` break at L721 is unreachable. The zip
        // reproduces the source counter exactly while keeping clippy quiet.
        let last_index = self.time_changes.len() as i32 - 1;
        for (i, time_change) in (-1..).zip(&self.time_changes) {
            if ms >= time_change.time_stamp {
                if ms < time_change.time_stamp || i == last_index {
                    last_time_change = time_change;
                    break;
                }
                result_step += (time_change.beat_time - last_time_change.beat_time)
                    * f64::from(STEPS_PER_BEAT);
                last_time_change = time_change;
            }
        }

        let last_step_length_ms = last_time_change.step_length_ms();
        let result_fractional_step = (ms - last_time_change.time_stamp) / last_step_length_ms;
        result_step + result_fractional_step
    }

    /// `Conductor.getStepTimeInMs` (`Conductor.hx:746-782`).
    pub fn get_step_time_in_ms(&self, step_time: f64) -> f64 {
        let Some(first) = self.time_changes.first() else {
            // Conductor.hx:748-752.
            return step_time * self.step_length_ms();
        };

        let mut result_ms = 0.0;
        let step_time = if step_time < 0.0 { 0.0 } else { step_time };

        let mut last_time_change = first;
        // Same counter quirk as getTimeInSteps (L759-775).
        let last_index = self.time_changes.len() as i32 - 1;
        for (i, time_change) in (-1..).zip(&self.time_changes) {
            if step_time >= time_change.beat_time * f64::from(STEPS_PER_BEAT) {
                if step_time < time_change.beat_time * f64::from(STEPS_PER_BEAT) || i == last_index
                {
                    last_time_change = time_change;
                    break;
                }
                result_ms += time_change.time_stamp - last_time_change.time_stamp;
                last_time_change = time_change;
            }
        }

        let last_step_length_ms = last_time_change.step_length_ms();
        result_ms += (step_time - last_time_change.beat_time * f64::from(STEPS_PER_BEAT))
            * last_step_length_ms;
        result_ms
    }

    /// `Conductor.getBeatTimeInMs` (`Conductor.hx:789-820`).
    ///
    /// Unlike the step/measure variants this one does not clamp negative input.
    pub fn get_beat_time_in_ms(&self, beat_time: f64) -> f64 {
        let Some(first) = self.time_changes.first() else {
            // Conductor.hx:791-795.
            return beat_time * self.step_length_ms() * f64::from(STEPS_PER_BEAT);
        };

        let mut result_ms = 0.0;
        let mut last_time_change = first;
        for time_change in &self.time_changes {
            if beat_time >= time_change.beat_time {
                last_time_change = time_change;
                result_ms = last_time_change.time_stamp;
            } else {
                // This time change is after the requested time (L808-812).
                break;
            }
        }

        let last_step_length_ms = last_time_change.step_length_ms();
        result_ms += (beat_time - last_time_change.beat_time)
            * last_step_length_ms
            * f64::from(STEPS_PER_BEAT);
        result_ms
    }

    /// `Conductor.getTimeInMeasures` (`Conductor.hx:607-647`).
    pub fn get_time_in_measures(&self, ms: f64) -> f64 {
        let Some(first) = self.time_changes.first() else {
            // Conductor.hx:609-613.
            return ms / self.step_length_ms() / self.steps_per_measure();
        };

        let mut result_measure_time = 0.0;
        let ms = if ms < 0.0 { 0.0 } else { ms };

        let mut last_time_change = first;
        // Conductor.hx:620 starts `i` at -1 and increments at the end of the
        // body: the `i == length - 1` break at L626 is unreachable, exactly
        // like the two loops above. The trailing fractional measure below is
        // what makes the result correct for the last segment.
        let last_index = self.time_changes.len() as i32 - 1;
        for (i, time_change) in (-1..).zip(&self.time_changes) {
            if ms >= time_change.time_stamp {
                if ms < time_change.time_stamp || i == last_index {
                    last_time_change = time_change;
                    break;
                }
                let current_step_length_ms = last_time_change.step_length_ms();
                let current_steps_per_measure = last_time_change.steps_per_measure();
                result_measure_time += (time_change.time_stamp - last_time_change.time_stamp)
                    / current_step_length_ms
                    / current_steps_per_measure;
                last_time_change = time_change;
            }
        }

        let remaining_step_length_ms = last_time_change.step_length_ms();
        let remaining_steps_per_measure = last_time_change.steps_per_measure();
        result_measure_time
            + (ms - last_time_change.time_stamp)
                / remaining_step_length_ms
                / remaining_steps_per_measure
    }

    /// `Conductor.getMeasureTimeInMs` (`Conductor.hx:654-695`).
    pub fn get_measure_time_in_ms(&self, measure_time: f64) -> f64 {
        let Some(first) = self.time_changes.first() else {
            // Conductor.hx:656-660.
            return measure_time * self.step_length_ms() * self.steps_per_measure();
        };

        let mut result_ms = 0.0;
        let measure_time = if measure_time < 0.0 {
            0.0
        } else {
            measure_time
        };

        let mut last_time_change = first;
        // Conductor.hx:667-686, same unreachable last-index break as above.
        let last_index = self.time_changes.len() as i32 - 1;
        for (i, time_change) in (-1..).zip(&self.time_changes) {
            let current_time_change_measure_time =
                self.get_time_in_measures(time_change.time_stamp);
            if measure_time >= current_time_change_measure_time {
                if measure_time < current_time_change_measure_time || i == last_index {
                    last_time_change = time_change;
                    break;
                }
                let current_step_length_ms = last_time_change.step_length_ms();
                let current_steps_per_measure = last_time_change.steps_per_measure();
                result_ms += (current_time_change_measure_time
                    - self.get_time_in_measures(last_time_change.time_stamp))
                    * current_step_length_ms
                    * current_steps_per_measure;
                last_time_change = time_change;
            }
        }

        let remaining_step_length_ms = last_time_change.step_length_ms();
        let remaining_steps_per_measure = last_time_change.steps_per_measure();
        result_ms += (measure_time - self.get_time_in_measures(last_time_change.time_stamp))
            * remaining_step_length_ms
            * remaining_steps_per_measure;
        result_ms
    }

    /// `Conductor.getTimeChange` (`Conductor.hx:827-844`).
    ///
    /// Haxe returns a synthetic `SongTimeChange(0, 100)` when nothing matches;
    /// the Rust API surfaces that as [`Option::None`].
    ///
    /// The source loop increments `i` *before* testing, so it compares against
    /// `time_changes[i]` (the following entry) and uses `i == length` for the
    /// last entry. Both halves are reproduced, including the short-circuit that
    /// keeps the index in bounds.
    #[must_use]
    pub fn get_time_change_at_ms(&self, ms: f64) -> Option<&TimeChange> {
        if self.time_changes.is_empty() {
            return None;
        }

        let ms = if ms < 0.0 { 0.0 } else { ms };
        let length = self.time_changes.len();
        let mut i: usize = 0;
        for time_change in &self.time_changes {
            i += 1;
            if (i == length && ms >= time_change.time_stamp)
                || (ms >= time_change.time_stamp && ms < self.time_changes[i].time_stamp)
            {
                return Some(time_change);
            }
        }
        None
    }

    fn time_signature_num(&self) -> i32 {
        self.current_time_change()
            .map_or(DEFAULT_TIME_SIGNATURE_NUM, |time_change| {
                time_change.time_signature_num
            })
    }

    fn time_signature_den(&self) -> i32 {
        self.current_time_change()
            .map_or(DEFAULT_TIME_SIGNATURE_DEN, |time_change| {
                time_change.time_signature_den
            })
    }

    fn steps_per_measure(&self) -> f64 {
        f64::from(self.time_signature_num() * STEPS_PER_BEAT as i32)
    }

    /// `Conductor.hx:452-461`: `timeChanges[0]` for `songPosition <= 0`,
    /// otherwise the last change whose timestamp is `<= songPosition`.
    fn select_current_change_index(&self) -> usize {
        if self.time_changes.is_empty() {
            return usize::MAX;
        }

        let mut index = 0;
        if self.song_position > 0.0 {
            for (i, time_change) in self.time_changes.iter().enumerate() {
                if self.song_position >= time_change.time_stamp {
                    index = i;
                }
                if self.song_position < time_change.time_stamp {
                    break;
                }
            }
        }
        index
    }

    /// `Conductor.hx:463-487`: choose between the time-change formula (round 6)
    /// and the constant-BPM fallback (round 4), then floor to integers.
    fn recompute_times(&mut self) {
        let selected = self
            .current_time_change()
            .map(|time_change| (time_change.beat_time, time_change.time_stamp));

        match selected {
            Some((beat_time, time_stamp)) if self.song_position > 0.0 => {
                // Conductor.hx:470-471.
                self.current_step_time = round_decimal(
                    beat_time * f64::from(STEPS_PER_BEAT)
                        + (self.song_position - time_stamp) / self.step_length_ms(),
                    6,
                );
                self.current_beat_time = self.current_step_time / f64::from(STEPS_PER_BEAT);
                self.current_measure_time = self.get_time_in_measures(self.song_position);
            }
            _ => {
                // Conductor.hx:478-487: constant BPM (also used when the
                // timeline is empty; `stepLengthMs` then uses DEFAULT_BPM).
                self.current_step_time =
                    round_decimal(self.song_position / self.step_length_ms(), 4);
                self.current_beat_time = self.current_step_time / f64::from(STEPS_PER_BEAT);
                self.current_measure_time = self.current_step_time / self.steps_per_measure();
            }
        }

        self.current_step = self.current_step_time.floor() as i32;
        self.current_beat = self.current_beat_time.floor() as i32;
        self.current_measure = self.current_measure_time.floor() as i32;
    }
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::round_decimal;

    #[test]
    fn round_decimal_matches_observed_native_flxmath() {
        // Same discriminating cases as tools/oracle/src/RoundCases.hx.
        for (value, digits, expected) in [
            (0.5, 0, 1.0),
            (-0.5, 0, 0.0),
            (2.5, 0, 3.0),
            (-1.5, 0, -1.0),
            (-1.6, 0, -2.0),
            (1.23456789, 6, 1.234568),
            (-1.23456789, 6, -1.234568),
            (1.23445, 4, 1.2345),
            (-1.23445, 4, -1.2344),
            (2147483649.25, 0, 2147483649.0),
        ] {
            assert_eq!(round_decimal(value, digits), expected);
        }
    }
}
