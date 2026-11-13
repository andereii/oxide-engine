use super::constants::{DEFAULT_BPM, MS_PER_SEC, SECS_PER_MIN, STEPS_PER_BEAT};

#[derive(Debug, Clone)]
pub struct SongTimeChange {
    pub time_stamp_ms: f64,
    pub beat_time: f64,
    pub bpm: f64,
    pub time_signature_num: i32,
    pub time_signature_den: i32,
}

impl SongTimeChange {
    pub fn new(time_stamp_ms: f64, bpm: f64) -> Self {
        Self {
            time_stamp_ms,
            beat_time: 0.0,
            bpm: bpm.max(1.0),
            time_signature_num: 4,
            time_signature_den: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConductorUpdate {
    pub step_changed: bool,
    pub beat_changed: bool,
    pub measure_changed: bool,
}

#[derive(Debug, Clone)]
pub struct Conductor {
    pub time_changes: Vec<SongTimeChange>,
    pub current_time_change: Option<SongTimeChange>,
    pub song_position_ms: f64,
    pub bpm_override: Option<f64>,
    pub current_measure: i32,
    pub current_beat: i32,
    pub current_step: i32,
    pub current_measure_time: f64,
    pub current_beat_time: f64,
    pub current_step_time: f64,
    pub instrumental_offset_ms: f64,
    pub format_offset_ms: f64,
    pub global_offset_ms: f64,
}

impl Default for Conductor {
    fn default() -> Self {
        Self::new(DEFAULT_BPM)
    }
}

impl Conductor {
    pub fn new(bpm: f64) -> Self {
        let mut conductor = Self {
            time_changes: Vec::new(),
            current_time_change: None,
            song_position_ms: 0.0,
            bpm_override: None,
            current_measure: 0,
            current_beat: 0,
            current_step: 0,
            current_measure_time: 0.0,
            current_beat_time: 0.0,
            current_step_time: 0.0,
            instrumental_offset_ms: 0.0,
            format_offset_ms: 0.0,
            global_offset_ms: 0.0,
        };
        conductor.map_time_changes(vec![SongTimeChange::new(0.0, bpm)]);
        conductor
    }

    pub fn bpm(&self) -> f64 {
        if let Some(bpm) = self.bpm_override {
            return bpm;
        }
        self.current_time_change
            .as_ref()
            .map(|tc| tc.bpm)
            .unwrap_or(DEFAULT_BPM)
    }

    pub fn starting_bpm(&self) -> f64 {
        if let Some(bpm) = self.bpm_override {
            return bpm;
        }
        self.time_changes
            .first()
            .map(|tc| tc.bpm)
            .unwrap_or(DEFAULT_BPM)
    }

    pub fn time_signature_numerator(&self) -> i32 {
        self.current_time_change
            .as_ref()
            .map(|tc| tc.time_signature_num)
            .unwrap_or(4)
    }

    pub fn time_signature_denominator(&self) -> i32 {
        self.current_time_change
            .as_ref()
            .map(|tc| tc.time_signature_den)
            .unwrap_or(4)
    }

    pub fn beats_per_measure(&self) -> f64 {
        f64::from(self.time_signature_numerator()) * (4.0 / f64::from(self.time_signature_denominator()))
    }

    pub fn steps_per_measure(&self) -> i32 {
        (self.beats_per_measure() * f64::from(STEPS_PER_BEAT)).round() as i32
    }

    pub fn beat_length_ms(&self) -> f64 {
        (SECS_PER_MIN / self.bpm()) * MS_PER_SEC
    }

    pub fn step_length_ms(&self) -> f64 {
        self.beat_length_ms() / f64::from(STEPS_PER_BEAT)
    }

    pub fn measure_length_ms(&self) -> f64 {
        self.beat_length_ms() * self.beats_per_measure()
    }

    pub fn combined_offset(&self) -> f64 {
        self.instrumental_offset_ms + self.format_offset_ms + self.global_offset_ms
    }

    pub fn force_bpm(&mut self, bpm: Option<f64>) {
        self.bpm_override = bpm;
    }

    pub fn map_time_changes(&mut self, mut changes: Vec<SongTimeChange>) {
        changes.sort_by(|a, b| a.time_stamp_ms.total_cmp(&b.time_stamp_ms));
        self.time_changes.clear();

        for mut tc in changes {
            if tc.time_stamp_ms < 0.0 {
                tc.time_stamp_ms = 0.0;
            }
            if tc.time_stamp_ms <= 0.0 || self.time_changes.is_empty() {
                tc.beat_time = 0.0;
            } else if let Some(prev) = self.time_changes.last() {
                let dt_beats = (tc.time_stamp_ms - prev.time_stamp_ms)
                    * prev.bpm
                    / SECS_PER_MIN
                    / MS_PER_SEC
                    * (f64::from(prev.time_signature_den) / 4.0);
                tc.beat_time = round_decimal(prev.beat_time + dt_beats, 4);
            }
            self.time_changes.push(tc);
        }

        let pos = self.song_position_ms;
        self.update_with_offsets(pos, false);
    }

    pub fn update(&mut self, song_pos_ms: f64) -> ConductorUpdate {
        self.update_with_offsets(song_pos_ms, false)
    }

    pub fn update_with_offsets(&mut self, mut song_pos_ms: f64, apply_offsets: bool) -> ConductorUpdate {
        if apply_offsets {
            song_pos_ms += self.combined_offset();
        }
        let old_measure = self.current_measure;
        let old_beat = self.current_beat;
        let old_step = self.current_step;

        self.song_position_ms = song_pos_ms;
        self.current_time_change = self.time_changes.first().cloned();
        if song_pos_ms > 0.0 {
            for tc in &self.time_changes {
                if song_pos_ms >= tc.time_stamp_ms {
                    self.current_time_change = Some(tc.clone());
                } else {
                    break;
                }
            }
        }

        let step_ms = self.step_length_ms();
        if let Some(tc) = &self.current_time_change
            && song_pos_ms > 0.0
        {
            self.current_step_time = round_decimal(
                (tc.beat_time * f64::from(STEPS_PER_BEAT)) + (song_pos_ms - tc.time_stamp_ms) / step_ms,
                6,
            );
            self.current_beat_time = self.current_step_time / f64::from(STEPS_PER_BEAT);
            self.current_measure_time = self.get_time_in_measures(song_pos_ms);
        } else {
            self.current_step_time = round_decimal(song_pos_ms / step_ms, 4);
            self.current_beat_time = self.current_step_time / f64::from(STEPS_PER_BEAT);
            self.current_measure_time = self.current_step_time / f64::from(self.steps_per_measure().max(1));
        }

        self.current_step = self.current_step_time.floor() as i32;
        self.current_beat = self.current_beat_time.floor() as i32;
        self.current_measure = self.current_measure_time.floor() as i32;

        ConductorUpdate {
            step_changed: self.current_step != old_step,
            beat_changed: self.current_beat != old_beat,
            measure_changed: self.current_measure != old_measure,
        }
    }

    pub fn get_time_change(&self, ms: f64) -> Option<&SongTimeChange> {
        let mut selected = self.time_changes.first();
        for tc in &self.time_changes {
            if ms >= tc.time_stamp_ms {
                selected = Some(tc);
            } else {
                break;
            }
        }
        selected
    }

    pub fn get_time_in_steps(&self, ms: f64) -> f64 {
        let Some(tc) = self.get_time_change(ms) else {
            return ms / self.step_length_ms();
        };
        let step_ms = ((SECS_PER_MIN / tc.bpm) * MS_PER_SEC) / f64::from(STEPS_PER_BEAT);
        (tc.beat_time * f64::from(STEPS_PER_BEAT)) + (ms - tc.time_stamp_ms) / step_ms
    }

    pub fn get_step_time_in_ms(&self, step_time: f64) -> f64 {
        let target_beat = step_time / f64::from(STEPS_PER_BEAT);
        self.get_beat_time_in_ms(target_beat)
    }

    pub fn get_beat_time_in_ms(&self, beat_time: f64) -> f64 {
        let mut selected = self.time_changes.first();
        for tc in &self.time_changes {
            if beat_time >= tc.beat_time {
                selected = Some(tc);
            } else {
                break;
            }
        }
        let Some(tc) = selected else {
            return beat_time * self.beat_length_ms();
        };
        let beat_ms = (SECS_PER_MIN / tc.bpm) * MS_PER_SEC;
        tc.time_stamp_ms + (beat_time - tc.beat_time) * beat_ms
    }

    pub fn get_time_in_measures(&self, ms: f64) -> f64 {
        self.get_time_in_steps(ms) / f64::from(self.steps_per_measure().max(1))
    }
}

fn round_decimal(value: f64, precision: i32) -> f64 {
    let factor = 10_f64.powi(precision);
    (value * factor).round() / factor
}

