use crate::utils::constants::{
    HEALTH_GHOST_MISS_PENALTY, HEALTH_HOLD_BONUS_PER_SECOND, HEALTH_HOLD_DROP_PENALTY_MAX,
    HEALTH_HOLD_DROP_PENALTY_PER_SECOND, HEALTH_MAX, HEALTH_MIN, HEALTH_MISS_BONUS,
    HOLD_DROP_PENALTY_THRESHOLD_MS, MS_PER_SEC, SCORE_GHOST_MISS_PENALTY,
    SCORE_HOLD_BONUS_PER_SECOND, SCORE_HOLD_DROP_PENALTY_PER_SECOND, SUSTAIN_MIN_REMAINING_MS,
};

use crate::play::judgement::{Judgement, miss_score};
use crate::play::notes::data::{ActiveHold, NoteDirection, process_note_window};
use crate::play::notes::strum::{CharNoteSignal, HoldCoverCmd};
use crate::play::scene::PlayState;

impl PlayState {
    pub(super) fn process_opponent_notes(&mut self, pos_ms: f64) {
        let start = self.opponent_strumline.active_start_index;
        let end = self
            .opponent_strumline
            .next_note_index
            .min(self.opponent_strumline.notes.len());

        for idx in start..end {
            let res = {
                let note = &mut self.opponent_strumline.notes[idx];
                process_note_window(note, pos_ms, false)
            };
            if !res.botplay_hit {
                continue;
            }
            let (is_hold, lane, note_id, end_ms, rem, scoreable, note_kind) = {
                let note = &self.opponent_strumline.notes[idx];
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
            self.opponent_vocals_volume = 1.0;
            self.opponent_strumline.hit_note(idx, true);
            self.opponent_strumline.pending_char_events.push(CharNoteSignal::Hit {
                direction: lane,
                note_kind,
                good_judgement: None,
                combo: 0,
            });
            if is_hold {
                self.opponent_strumline.active_holds[lane.index()] = Some(ActiveHold {
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
        }

        let vis_pos = self.opponent_strumline.visual_song_position;
        for lane_idx in 0..4 {
            let Some(mut hold) = self.opponent_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = (hold.end_ms - vis_pos).max(0.0);
            if let Some(note) = self.opponent_strumline.notes.get_mut(hold.note_idx) {
                note.hold_remaining_ms = hold.remaining_ms;
                if hold.remaining_ms <= SUSTAIN_MIN_REMAINING_MS {
                    note.hold_killed = true;
                }
            }
            if hold.remaining_ms <= SUSTAIN_MIN_REMAINING_MS {
                self.opponent_strumline.active_holds[lane_idx] = None;
                self.opponent_strumline.play_static(hold.lane);
                self.opponent_strumline
                    .pending_hold_covers
                    .push(HoldCoverCmd::Kill(hold.lane));
            } else {
                self.opponent_strumline.hold_confirm(hold.lane);
                self.opponent_strumline.pending_char_events.push(CharNoteSignal::HoldTick);
                self.opponent_strumline.active_holds[lane_idx] = Some(hold);
            }
        }
    }

    pub(super) fn process_player_holds(&mut self, elapsed_s: f64, _pos_ms: f64) {
        let vis_pos = self.player_strumline.visual_song_position;

        for lane_idx in 0..4 {
            let Some(mut hold) = self.player_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = (hold.end_ms - vis_pos).max(0.0);
            let held = self.is_bot_play_mode || self.player_strumline.is_key_held(hold.lane);

            if held && !hold.dropped {
                if let Some(note) = self.player_strumline.notes.get_mut(hold.note_idx) {
                    note.hold_remaining_ms = hold.remaining_ms;
                    if hold.remaining_ms <= SUSTAIN_MIN_REMAINING_MS {
                        note.hold_killed = true;
                    }
                }
                if hold.remaining_ms <= SUSTAIN_MIN_REMAINING_MS {
                    self.player_strumline.active_holds[lane_idx] = None;
                    if self.player_strumline.is_key_held(hold.lane) {
                        self.player_strumline.play_press(hold.lane);
                    } else {
                        self.player_strumline.play_static(hold.lane);
                    }
                    self.player_strumline
                        .pending_hold_covers
                        .push(HoldCoverCmd::End(hold.lane));
                    continue;
                }

                self.player_vocals_volume = 1.0;
                if !self.is_bot_play_mode && hold.scoreable {
                    self.song_score += SCORE_HOLD_BONUS_PER_SECOND * elapsed_s;
                    self.health = (self.health + HEALTH_HOLD_BONUS_PER_SECOND * elapsed_s)
                        .clamp(HEALTH_MIN, HEALTH_MAX);
                }
                self.player_strumline.hold_confirm(hold.lane);
                self.player_strumline.pending_char_events.push(CharNoteSignal::HoldTick);
                self.player_strumline.active_holds[lane_idx] = Some(hold);
            } else {
                hold.dropped = true;
                if let Some(note) = self.player_strumline.notes.get_mut(hold.note_idx) {
                    note.hold_dropped = true;
                    note.hold_remaining_ms = hold.remaining_ms;
                    note.hold_killed = true;
                }
                self.player_strumline.play_static(hold.lane);
                self.player_strumline
                    .pending_hold_covers
                    .push(HoldCoverCmd::Kill(hold.lane));

                let mut is_combo_break = false;
                let prev_combo = self.tallies.combo;
                if !hold.handled_miss
                    && !self.is_bot_play_mode
                    && hold.scoreable
                    && hold.remaining_ms > HOLD_DROP_PENALTY_THRESHOLD_MS
                {
                    hold.handled_miss = true;
                    is_combo_break = true;
                    let rem_sec = hold.remaining_ms / MS_PER_SEC;
                    let score_change = SCORE_HOLD_DROP_PENALTY_PER_SECOND * rem_sec;
                    let health_uncapped = rem_sec * HEALTH_HOLD_DROP_PENALTY_PER_SECOND;
                    let health_max =
                        HEALTH_HOLD_DROP_PENALTY_MAX - if hold.hit_base { -HEALTH_MISS_BONUS } else { 0.0 };
                    let health_change = health_uncapped.clamp(health_max, 0.0);

                    self.tallies.combo = 0;
                    self.player_vocals_volume = 0.0;
                    self.pending_miss_sfx = true;
                    self.song_score += score_change;
                    self.health = (self.health + health_change).clamp(HEALTH_MIN, HEALTH_MAX);
                }
                self.player_strumline.pending_char_events.push(CharNoteSignal::HoldDrop {
                    direction: hold.lane,
                    is_combo_break,
                    combo_before: prev_combo,
                });
                self.player_strumline.active_holds[lane_idx] = None;
            }
        }
    }

    pub fn on_note_miss(&mut self, health_change: f64) {
        self.player_vocals_volume = 0.0;
        self.pending_miss_sfx = true;
        self.apply_score(f64::from(miss_score()), Judgement::Miss, health_change, true);
    }

    pub fn ghost_note_miss(&mut self, _direction: NoteDirection, has_possible_notes: bool) {
        self.player_vocals_volume = 0.0;
        self.pending_miss_sfx = true;
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
            if self.tallies.combo >= 10 && matches!(judgement, Judgement::Miss) {
                self.pending_popups.push(crate::play::popups::PopupEvent {
                    judgement: None,
                    combo: Some(0),
                });
            }
            self.tallies.combo = 0;
        } else {
            self.tallies.combo += 1;
            self.tallies.max_combo = self.tallies.max_combo.max(self.tallies.combo);
        }
    }
}
