use crate::utils::constants::{
    HEALTH_GHOST_MISS_PENALTY, HEALTH_HOLD_BONUS_PER_SECOND, HEALTH_HOLD_DROP_PENALTY_PER_SECOND,
    HEALTH_MAX, HEALTH_MIN, HEALTH_MISS_BONUS, HIT_WINDOW_MS, HOLD_DROP_PENALTY_THRESHOLD_MS,
    SCORE_GHOST_MISS_PENALTY, SCORE_HOLD_BONUS_PER_SECOND, SCORE_HOLD_DROP_PENALTY_PER_SECOND,
};

use super::super::judgement::{Judgement, judge_note, miss_score, score_note};
use super::super::scene::PlayState;
use super::super::types::PreciseInputEvent;
use super::direction::NoteDirection;
use super::note::{ActiveHold, NoteSprite};

impl PlayState {
    pub fn generate_song(&mut self) {
        self.song_events = self.chart.events.clone();
        self.song_events.sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
        self.regen_note_data(self.start_timestamp_ms);
    }

    pub fn regen_note_data(&mut self, start_time_ms: f64) {
        self.player_strumline.clean();
        self.opponent_strumline.clean();
        self.tallies.total_notes = 0;

        for note in &self.chart.player_notes {
            if note.time_ms >= start_time_ms
                && let Some(sprite) = NoteSprite::from_data(note.clone())
            {
                if sprite.data.scoreable {
                    self.tallies.total_notes += 1;
                }
                self.player_strumline.notes.push(sprite);
            }
        }
        for note in &self.chart.opponent_notes {
            if note.time_ms >= start_time_ms
                && let Some(sprite) = NoteSprite::from_data(note.clone())
            {
                self.opponent_strumline.notes.push(sprite);
            }
        }
    }

    pub fn on_key_press_precise(&mut self, direction: NoteDirection, song_position_ms: f64, latency_ms: f64) {
        self.input_press_queue.push_back(PreciseInputEvent {
            direction,
            song_position_ms,
            latency_ms,
        });
    }

    pub fn on_key_release_precise(&mut self, direction: NoteDirection, song_position_ms: f64, latency_ms: f64) {
        self.input_release_queue.push_back(PreciseInputEvent {
            direction,
            song_position_ms,
            latency_ms,
        });
    }

    pub fn process_input_queue(&mut self) {
        if self.input_press_queue.is_empty() && self.input_release_queue.is_empty() {
            return;
        }
        if self.is_in_cutscene || self.disable_keys {
            self.input_press_queue.clear();
            self.input_release_queue.clear();
            return;
        }

        let has_any_may_hit = self.player_strumline.notes.iter().any(|n| n.may_hit);

        while let Some(input) = self.input_press_queue.pop_front() {
            self.player_strumline.press_key(input.direction);
            if self.is_bot_play_mode {
                continue;
            }

            let target_idx = self
                .player_strumline
                .notes
                .iter()
                .enumerate()
                .filter(|(_, n)| !n.has_been_hit && !n.has_missed && n.may_hit && n.lane == input.direction)
                .min_by_key(|(idx, n)| (n.data.low_priority, *idx))
                .map(|(idx, _)| idx);

            match target_idx {
                Some(idx) => {
                    self.good_note_hit(idx, input);
                    self.player_strumline.play_confirm(input.direction);
                }
                None => {
                    if !self.player_strumline.ghost_tap_enabled {
                        self.ghost_note_miss(input.direction, has_any_may_hit);
                    }
                    self.player_strumline.play_press(input.direction);
                }
            }
        }

        while let Some(input) = self.input_release_queue.pop_front() {
            self.player_strumline.play_static(input.direction);
            self.player_strumline.release_key(input.direction);
        }
    }

    pub fn process_notes(&mut self, elapsed_s: f64) {
        let pos_ms = self.conductor.song_position_ms;

        let mut confirm_lanes = [false; 4];
        for note in &mut self.opponent_strumline.notes {
            if !note.has_been_hit && pos_ms >= note.data.time_ms {
                note.has_been_hit = true;
                confirm_lanes[note.lane.index()] = true;
            }
        }
        for (i, triggered) in confirm_lanes.into_iter().enumerate() {
            if triggered {
                self.opponent_strumline.play_confirm(super::direction::NoteDirection::from_index(i));
            }
        }

        let mut missed_indices = Vec::new();
        let mut bot_hits = Vec::new();

        for (idx, note) in self.player_strumline.notes.iter_mut().enumerate() {
            if note.has_been_hit || note.has_missed {
                continue;
            }
            let diff_ms = pos_ms - note.data.time_ms;
            note.may_hit = diff_ms.abs() <= HIT_WINDOW_MS;

            if self.is_bot_play_mode && pos_ms >= note.data.time_ms {
                bot_hits.push((idx, note.lane, note.data.time_ms));
                continue;
            }
            if diff_ms > HIT_WINDOW_MS {
                note.has_missed = true;
                note.handled_miss = true;
                missed_indices.push(idx);
            }
        }

        for (idx, lane, time_ms) in bot_hits {
            self.good_note_hit(
                idx,
                PreciseInputEvent {
                    direction: lane,
                    song_position_ms: time_ms,
                    latency_ms: 0.0,
                },
            );
            self.player_strumline.play_confirm(lane);
        }

        for _ in missed_indices {
            self.on_note_miss(HEALTH_MISS_BONUS);
        }

        self.process_active_holds(elapsed_s, pos_ms);
    }

    fn process_active_holds(&mut self, elapsed_s: f64, pos_ms: f64) {
        for lane_idx in 0..4 {
            let Some(mut hold) = self.player_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = (hold.end_ms - pos_ms).max(0.0);
            if hold.remaining_ms <= 0.0 {
                self.player_strumline.active_holds[lane_idx] = None;
                continue;
            }
            let held = self.is_bot_play_mode || self.player_strumline.held_lanes[lane_idx];
            if held {
                self.song_score += SCORE_HOLD_BONUS_PER_SECOND * elapsed_s;
                self.health = (self.health + HEALTH_HOLD_BONUS_PER_SECOND * elapsed_s).clamp(HEALTH_MIN, HEALTH_MAX);
                self.player_strumline.play_confirm(hold.lane);
            } else if !hold.dropped {
                hold.dropped = true;
                if hold.remaining_ms > HOLD_DROP_PENALTY_THRESHOLD_MS {
                    self.tallies.combo = 0;
                    self.song_score += SCORE_HOLD_DROP_PENALTY_PER_SECOND * elapsed_s;
                    self.health = (self.health + HEALTH_HOLD_DROP_PENALTY_PER_SECOND * elapsed_s).clamp(HEALTH_MIN, HEALTH_MAX);
                }
            }
            self.player_strumline.active_holds[lane_idx] = Some(hold);
        }
    }

    pub fn good_note_hit(&mut self, note_idx: usize, input: PreciseInputEvent) {
        let note = &mut self.player_strumline.notes[note_idx];
        note.has_been_hit = true;
        let note_diff = (input.song_position_ms - note.data.time_ms - input.latency_ms).trunc();
        let score = f64::from(score_note(note_diff));
        let judgement = judge_note(note_diff);
        if matches!(judgement, Judgement::Miss) {
            return;
        }
        let is_hold = note.is_hold_note();
        let hold_end_ms = note.data.time_ms + note.data.sustain_length_ms;
        let lane = note.lane;
        let note_id = note.data.id;
        let scoreable = note.data.scoreable;

        if is_hold {
            self.player_strumline.active_holds[lane.index()] = Some(ActiveHold {
                note_id,
                lane,
                end_ms: hold_end_ms,
                remaining_ms: (hold_end_ms - input.song_position_ms).max(0.0),
                dropped: false,
            });
        }

        if scoreable {
            self.tallies.total_notes_hit += 1;
            self.apply_score(score, judgement, judgement.health_change(), judgement.is_combo_break());
        }
    }

    pub fn on_note_miss(&mut self, health_change: f64) {
        self.apply_score(f64::from(miss_score()), Judgement::Miss, health_change, true);
    }

    pub fn ghost_note_miss(&mut self, _direction: NoteDirection, has_possible_notes: bool) {
        if has_possible_notes {
            self.apply_score(SCORE_GHOST_MISS_PENALTY, Judgement::Miss, HEALTH_GHOST_MISS_PENALTY, true);
        } else {
            self.song_score += SCORE_GHOST_MISS_PENALTY;
            self.health = (self.health + HEALTH_GHOST_MISS_PENALTY).clamp(HEALTH_MIN, HEALTH_MAX);
        }
    }

    pub fn apply_score(&mut self, score: f64, judgement: Judgement, health_change: f64, is_combo_break: bool) {
        judgement.record(&mut self.tallies);
        self.song_score += score;
        self.health = (self.health + health_change).clamp(HEALTH_MIN, HEALTH_MAX);
        if is_combo_break {
            self.tallies.combo = 0;
        } else {
            self.tallies.combo += 1;
            self.tallies.max_combo = self.tallies.max_combo.max(self.tallies.combo);
        }
    }
}

