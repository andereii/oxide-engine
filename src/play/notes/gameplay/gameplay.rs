use bevy::prelude::KeyCode;

use crate::utils::constants::HEALTH_MISS_BONUS;

use crate::play::judgement::{Judgement, judge_note, score_note};
use crate::play::notes::data::{
    ActiveHold, NoteDirection, NoteSprite, ScrollVelocityTimeline, process_note_window,
};
use crate::play::notes::strum::CharNoteSignal;
use crate::play::popups::PopupEvent;
use crate::play::scene::PlayState;
use crate::play::types::PreciseInputEvent;

impl PlayState {
    pub fn generate_song(&mut self) {
        self.song_events = self.chart.events.clone();
        self.song_events
            .sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
        self.rebuild_scroll_velocities();
        self.regen_note_data(self.start_timestamp_ms);
    }

    pub fn rebuild_scroll_velocities(&mut self) {
        self.player_strumline.scroll_velocity =
            ScrollVelocityTimeline::from_events(&self.song_events, "player", &self.conductor);
        self.opponent_strumline.scroll_velocity =
            ScrollVelocityTimeline::from_events(&self.song_events, "opponent", &self.conductor);
    }

    pub fn regen_note_data(&mut self, start_time_ms: f64) {
        self.player_strumline.clean();
        self.opponent_strumline.clean();
        self.tallies.total_notes = 0;

        for note in &self.chart.player_notes {
            if note.time_ms >= start_time_ms
                && let Some(mut sprite) = NoteSprite::from_data(note.clone())
            {
                let kind_scoreable = self.note_kinds.is_scoreable(sprite.data.kind.as_deref());
                sprite.data.scoreable = sprite.data.scoreable && kind_scoreable;
                if let Some(k) = self.note_kinds.get(sprite.data.kind.as_deref()) {
                    sprite.data.low_priority = sprite.data.low_priority || k.low_priority;
                }
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

        self.player_strumline
            .notes
            .sort_by(|a, b| a.data.time_ms.total_cmp(&b.data.time_ms));
        self.opponent_strumline
            .notes
            .sort_by(|a, b| a.data.time_ms.total_cmp(&b.data.time_ms));
    }

    pub fn handle_skipped_notes(&mut self) {
        let pos_ms = self.conductor.song_position_ms;
        self.player_strumline.handle_skipped_notes(pos_ms);
        self.opponent_strumline.handle_skipped_notes(pos_ms);
    }

    pub fn on_key_press_precise(
        &mut self,
        direction: NoteDirection,
        key_code: Option<KeyCode>,
        song_position_ms: f64,
        latency_ms: f64,
    ) {
        self.input_press_queue.push_back(PreciseInputEvent {
            direction,
            key_code,
            song_position_ms,
            latency_ms,
        });
    }

    pub fn on_key_release_precise(
        &mut self,
        direction: NoteDirection,
        key_code: Option<KeyCode>,
        song_position_ms: f64,
        latency_ms: f64,
    ) {
        self.input_release_queue.push_back(PreciseInputEvent {
            direction,
            key_code,
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

        let has_any_may_hit = self.player_strumline.has_notes_may_hit();
        let start = self.player_strumline.active_start_index;
        let end = self
            .player_strumline
            .next_note_index
            .min(self.player_strumline.notes.len());

        while let Some(input) = self.input_press_queue.pop_front() {
            self.player_strumline.press_key(input.direction, input.key_code);
            if self.is_bot_play_mode {
                continue;
            }

            let target_idx = self.player_strumline.notes[start..end]
                .iter()
                .enumerate()
                .filter(|(_, n)| {
                    !n.killed
                        && !n.has_been_hit
                        && !n.has_missed
                        && n.may_hit
                        && n.lane == input.direction
                })
                .min_by_key(|(rel_idx, n)| (n.data.low_priority, *rel_idx))
                .map(|(rel_idx, _)| start + rel_idx);

            match target_idx {
                Some(idx) => {
                    self.good_note_hit(idx, input);
                }
                None => {
                    if !self.player_strumline.may_ghost_tap() {
                        self.ghost_note_miss(input.direction, has_any_may_hit);
                        self.player_strumline.pending_char_events.push(CharNoteSignal::GhostMiss {
                            direction: input.direction,
                        });
                    }
                    self.player_strumline.play_press(input.direction);
                }
            }
        }

        while let Some(input) = self.input_release_queue.pop_front() {
            self.player_strumline.release_key(input.direction, input.key_code);
            if !self.player_strumline.is_key_held(input.direction) {
                self.player_strumline.play_static(input.direction);
            }
        }
    }

    pub fn process_notes(&mut self, elapsed_s: f64) {
        let pos_ms = self.conductor.song_position_ms;
        self.process_opponent_notes(pos_ms);

        let start = self.player_strumline.active_start_index;
        let end = self
            .player_strumline
            .next_note_index
            .min(self.player_strumline.notes.len());
        let is_controlled = !self.is_bot_play_mode;

        for idx in start..end {
            let res = {
                let note = &mut self.player_strumline.notes[idx];
                process_note_window(note, pos_ms, is_controlled)
            };

            if res.botplay_hit {
                let (is_hold, lane, note_id, end_ms, rem, scoreable, note_kind) = {
                    let note = &self.player_strumline.notes[idx];
                    let end_ms = note.data.time_ms + note.data.sustain_length_ms;
                    let rem = (end_ms - pos_ms).clamp(0.0, note.data.sustain_length_ms);
                    (
                        note.is_hold_note(),
                        note.lane,
                        note.data.id,
                        end_ms,
                        rem,
                        note.data.scoreable,
                        note.data.kind.clone(),
                    )
                };
                self.player_strumline.hit_note(idx, true);
                self.player_strumline.pending_char_events.push(CharNoteSignal::Hit {
                    direction: lane,
                    note_kind,
                    good_judgement: Some(true),
                    combo: self.tallies.combo,
                });
                if is_hold {
                    self.player_strumline.active_holds[lane.index()] = Some(ActiveHold {
                        note_idx: idx,
                        note_id,
                        lane,
                        end_ms,
                        remaining_ms: rem,
                        dropped: false,
                        handled_miss: false,
                        scoreable,
                        hit_base: true,
                    });
                }
                continue;
            }

            if !res.cont {
                continue;
            }

            let should_miss = {
                let note = &mut self.player_strumline.notes[idx];
                if note.has_missed && !note.handled_miss {
                    note.handled_miss = true;
                    !self.is_bot_play_mode && note.data.scoreable
                } else {
                    false
                }
            };
            if should_miss {
                let miss_dir = self.player_strumline.notes[idx].lane;
                let prev_combo = self.tallies.combo;
                self.on_note_miss(HEALTH_MISS_BONUS);
                self.player_strumline.pending_char_events.push(CharNoteSignal::Miss {
                    direction: miss_dir,
                    combo_before: prev_combo,
                });
            }
        }

        self.process_player_holds(elapsed_s, pos_ms);
    }

    pub fn good_note_hit(&mut self, note_idx: usize, input: PreciseInputEvent) {
        let note = &self.player_strumline.notes[note_idx];
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
        let note_kind = note.data.kind.clone();
        let rem = (hold_end_ms - self.conductor.song_position_ms).clamp(0.0, note.data.sustain_length_ms);
        let is_combo_break = judgement.is_combo_break();
        let prev_combo = self.tallies.combo;

        self.player_strumline.hit_note(note_idx, !is_combo_break);
        if matches!(judgement, Judgement::Sick) && self.player_strumline.show_notesplash {
            self.player_strumline.pending_splashes.push(lane);
        }

        if is_hold {
            self.player_strumline.active_holds[lane.index()] = Some(ActiveHold {
                note_idx,
                note_id,
                lane,
                end_ms: hold_end_ms,
                remaining_ms: rem,
                dropped: false,
                handled_miss: false,
                scoreable,
                hit_base: true,
            });
        }

        self.player_vocals_volume = 1.0;
        if scoreable {
            self.tallies.total_notes_hit += 1;
            self.apply_score(score, judgement, judgement.health_change(), is_combo_break);
            let combo = (self.tallies.combo >= 10).then_some(self.tallies.combo);
            self.pending_popups.push(PopupEvent { judgement: Some(judgement), combo });
        }
        self.player_strumline.pending_char_events.push(CharNoteSignal::Hit {
            direction: lane,
            note_kind,
            good_judgement: scoreable.then_some(!is_combo_break),
            combo: if is_combo_break { prev_combo } else { self.tallies.combo },
        });
    }
}
