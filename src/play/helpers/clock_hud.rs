use crate::scripting::{HookKind, ScriptEvent};
use crate::utils::conductor::ConductorUpdate;
use crate::utils::constants::{
    AUDIO_CORRECTION_EASE_RATIO, CONDUCTOR_DRIFT_THRESHOLD_MS, HEALTH_LERP_FACTOR, HEALTH_MAX,
    HEALTH_MIN, MAX_RELATIVE_CAM_ZOOM, STEPS_PER_BEAT,
};

const CLOCK_FORWARD_DEADBAND_MS: f64 = 14.0;
const CLOCK_BACKWARD_DEADBAND_MS: f64 = -32.0;

use super::super::scene::PlayState;

impl PlayState {
    pub(crate) fn sync_song_clock(&mut self, elapsed_s: f64, audio_time_ms: Option<f64>) -> ConductorUpdate {
        let delta_ms = elapsed_s * 1000.0 * self.playback_rate;
        let prev_pos = self.conductor.song_position_ms;
        let predicted_ms = prev_pos + delta_ms;

        let (false, Some(raw_audio_ms)) = (self.is_in_countdown, audio_time_ms) else {
            self.last_raw_audio_ms = None;
            let next_pos = if self.is_in_countdown {
                predicted_ms.min(self.start_timestamp_ms).max(prev_pos)
            } else {
                predicted_ms
            };
            return self.conductor.update(next_pos);
        };

        let target_ms = raw_audio_ms + self.conductor.combined_offset();
        let drift_ms = target_ms - predicted_ms;
        // Only hard-snap forward. Snapping backward on audio startup/buffer lag drags notes
        // backward and triggers character reset heuristics.
        if drift_ms.round() > CONDUCTOR_DRIFT_THRESHOLD_MS {
            self.last_raw_audio_ms = Some(raw_audio_ms);
            return self.conductor.update_with_offsets(raw_audio_ms, true);
        }

        // rodio updates `sink.position()` only when its audio mixer buffer refills,
        // so repeated identical readings between refills are stale. Integrate
        // `delta_ms` smoothly on stale frames instead of stalling or pulling back.
        let audio_advanced = self
            .last_raw_audio_ms
            .is_none_or(|prev| (raw_audio_ms - prev).abs() > 0.05);
        self.last_raw_audio_ms = Some(raw_audio_ms);

        // Ignore audio mixer buffer quantization jitter (~20-25ms) so note scrolling runs
        // smoothly at frame rate, converging gently only when drift leaves the deadband.
        let ease_ratio =
            1.0 - (-(AUDIO_CORRECTION_EASE_RATIO * self.playback_rate) * elapsed_s).exp();
        let next_pos = if drift_ms > CLOCK_FORWARD_DEADBAND_MS {
            predicted_ms + (drift_ms - CLOCK_FORWARD_DEADBAND_MS) * ease_ratio
        } else if (audio_advanced || drift_ms < -CONDUCTOR_DRIFT_THRESHOLD_MS)
            && drift_ms < CLOCK_BACKWARD_DEADBAND_MS
        {
            let excess_ms = drift_ms - CLOCK_BACKWARD_DEADBAND_MS;
            (predicted_ms + excess_ms * ease_ratio).max(prev_pos + delta_ms * 0.7)
        } else {
            predicted_ms
        };
        self.conductor.update(next_pos.max(prev_pos))
    }


    pub fn step_hit(&mut self) {
        if self.camera.hud_zoom < MAX_RELATIVE_CAM_ZOOM * self.camera.default_hud_zoom
            && self.camera.zoom_rate > 0.0
        {
            let step_period = (self.camera.zoom_rate * f64::from(STEPS_PER_BEAT)).round() as i32;
            let step_offset = (self.camera.zoom_rate_offset * f64::from(STEPS_PER_BEAT)).round() as i32;
            if step_period > 0 && (self.conductor.current_step + step_offset).rem_euclid(step_period) == 0 {
                self.camera.bop_multiplier += self.camera.bop_intensity - 1.0;
                self.camera.hud_zoom += self.camera.hud_zoom_intensity * self.camera.default_hud_zoom;
            }
        }
        let step = self.conductor.current_step;
        let beat = self.conductor.current_beat;
        let measure = beat.div_euclid(i32::from(STEPS_PER_BEAT));
        self.pending_hooks.push((
            HookKind::StepHit,
            ScriptEvent::new(HookKind::StepHit)
                .with_field("step", i64::from(step))
                .with_field("beat", i64::from(beat))
                .with_field("measure", i64::from(measure)),
        ));
    }

    pub fn beat_hit(&mut self) {
        let step = self.conductor.current_step;
        let beat = self.conductor.current_beat;
        let measure = beat.div_euclid(i32::from(STEPS_PER_BEAT));
        self.pending_hooks.push((
            HookKind::BeatHit,
            ScriptEvent::new(HookKind::BeatHit)
                .with_field("beat", i64::from(beat))
                .with_field("step", i64::from(step))
                .with_field("measure", i64::from(measure)),
        ));
        if beat.rem_euclid(i32::from(STEPS_PER_BEAT)) == 0 {
            self.pending_hooks.push((
                HookKind::MeasureHit,
                ScriptEvent::new(HookKind::MeasureHit)
                    .with_field("measure", i64::from(measure))
                    .with_field("beat", i64::from(beat))
                    .with_field("step", i64::from(step)),
            ));
        }
    }


    pub fn update_health_bar(&mut self, elapsed_s: f64) {
        self.health = self.health.clamp(HEALTH_MIN, HEALTH_MAX);
        if self.is_bot_play_mode {
            self.health_lerp = HEALTH_MAX;
        } else {
            let alpha = 1.0 - (1.0 - HEALTH_LERP_FACTOR).powf(elapsed_s * 60.0);
            self.health_lerp += (self.health - self.health_lerp) * alpha;
        }
    }

    pub fn update_score_text(&mut self) {
        if let Some(custom) = &self.custom_score_text {
            self.last_hud_key = (i64::MIN, u32::MAX, u32::MAX, i32::MIN);
            if self.score_text != *custom {
                self.score_text = custom.clone();
            }
            return;
        }
        if self.is_bot_play_mode {
            self.last_hud_key = (i64::MIN, u32::MAX, u32::MAX, i32::MIN);
            if self.score_text != "Bot Play Enabled" {
                self.score_text = "Bot Play Enabled".to_string();
            }
            return;
        }
        let score = self.song_score.round() as i64;
        let key = (score, 0, 0, 0);
        if self.last_hud_key == key { return; }
        self.last_hud_key = key;
        self.score_text = format!("Score: {score}");
    }
}
