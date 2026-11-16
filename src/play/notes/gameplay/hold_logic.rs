use crate::scripting::{HookKind, ScriptEvent};
use crate::utils::constants::{
    COMBO_POPUP_THRESHOLD, HEALTH_GHOST_MISS_PENALTY, HEALTH_HOLD_BONUS_PER_SECOND,
    HEALTH_HOLD_DROP_PENALTY_MAX, HEALTH_HOLD_DROP_PENALTY_PER_SECOND, HEALTH_MAX, HEALTH_MIN,
    HEALTH_MISS_BONUS, HOLD_DROP_PENALTY_THRESHOLD_MS, KEY_COUNT, MS_PER_SEC,
    SCORE_GHOST_MISS_PENALTY, SCORE_HOLD_BONUS_PER_SECOND, SCORE_HOLD_DROP_PENALTY_PER_SECOND,
    SUSTAIN_MIN_REMAINING_MS,
};

use crate::play::hud::judgement::{Judgement, miss_score};
use crate::play::notes::data::{ActiveHold, NoteDirection, process_note_window};
use crate::play::notes::strum::{CharNoteSignal, HoldCoverCmd};
use crate::play::scene::PlayState;

fn compute_hold_remaining_ms(hold: &ActiveHold, pos_ms: f64) -> f64 {
    if pos_ms > hold.strum_time_ms {
        (hold.end_ms - pos_ms).clamp(0.0, hold.full_length_ms)
    } else {
        hold.full_length_ms
    }
}

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
            let (is_hold, lane, note_id, strum_time_ms, full_length_ms, end_ms, rem, scoreable, note_kind) = {
                let note = &self.opponent_strumline.notes[idx];
                (
                    note.is_hold_note(),
                    note.lane,
                    note.data.id,
                    note.data.time_ms,
                    note.data.sustain_length_ms,
                    note.data.time_ms + note.data.sustain_length_ms,
                    (note.data.time_ms + note.data.sustain_length_ms - pos_ms).clamp(0.0, note.data.sustain_length_ms),
                    note.data.scoreable,
                    note.data.kind.clone(),
                )
            };
            self.opponent_vocals_volume = 1.0;
            self.opponent_strumline.hit_note(idx, true);
            self.pending_hooks.push((
                HookKind::OpponentNoteHit,
                ScriptEvent::new(HookKind::OpponentNoteHit)
                    .with_field("id", note_id as i64)
                    .with_field("note_idx", idx as i64)
                    .with_field("dir", lane.index() as i64)
                    .with_field("time", strum_time_ms)
                    .with_field("length", full_length_ms)
                    .with_field("kind", note_kind.clone().unwrap_or_default())
                    .with_field("is_hold", is_hold)
                    .with_field("is_player", false)
                    .with_field("play_anim", true),
            ));
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
                    strum_time_ms,
                    full_length_ms,
                    end_ms,
                    remaining_ms: rem,
                    dropped: false,
                    handled_miss: false,
                    scoreable,
                    hit_base: true,
                });
            }
        }

        for lane_idx in 0..KEY_COUNT {
            let Some(mut hold) = self.opponent_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = compute_hold_remaining_ms(&hold, pos_ms);
            if let Some(note) = self.opponent_strumline.notes.get_mut(hold.note_idx) {
                note.hold_remaining_ms = hold.remaining_ms;
                if hold.remaining_ms <= 0.0 {
                    note.hold_killed = true;
                }
            }
            if hold.remaining_ms <= 0.0 {
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

    pub(super) fn process_player_holds(&mut self, elapsed_s: f64, pos_ms: f64) {
        for lane_idx in 0..KEY_COUNT {
            let Some(mut hold) = self.player_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = compute_hold_remaining_ms(&hold, pos_ms);
            let held = self.is_bot_play_mode || self.player_strumline.is_key_held(hold.lane);
            // V-Slice only flags hold drops after the note's strum_time has passed.
            let dropped_now = !hold.dropped && pos_ms > hold.strum_time_ms && !held;

            if !hold.dropped && !dropped_now {
                if let Some(note) = self.player_strumline.notes.get_mut(hold.note_idx) {
                    note.hold_remaining_ms = hold.remaining_ms;
                    if hold.remaining_ms <= 0.0 {
                        note.hold_killed = true;
                    }
                }
                if hold.remaining_ms <= 0.0 {
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
            } else if hold.remaining_ms <= SUSTAIN_MIN_REMAINING_MS {
                // Once the sustain trail has reached its visual minimum cutoff, releasing
                // finishes the hold cleanly instead of killing the cover or flagging a drop.
                if let Some(note) = self.player_strumline.notes.get_mut(hold.note_idx) {
                    note.hold_remaining_ms = 0.0;
                    note.hold_killed = true;
                }
                self.player_strumline.play_static(hold.lane);
                self.player_strumline
                    .pending_hold_covers
                    .push(HoldCoverCmd::End(hold.lane));
                self.player_strumline.active_holds[lane_idx] = None;
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

                // V-Slice (PlayState.hx) skips penalties and NOTE_HOLD_DROP events
                // when the remaining hold length is within HOLD_DROP_PENALTY_THRESHOLD_MS.
                if !hold.handled_miss
                    && !self.is_bot_play_mode
                    && hold.scoreable
                    && hold.remaining_ms > HOLD_DROP_PENALTY_THRESHOLD_MS
                {
                    hold.handled_miss = true;
                    let prev_combo = self.tallies.combo;
                    let prev_max_combo = self.tallies.max_combo;
                    let prev_health = self.health;
                    let rem_sec = hold.remaining_ms / MS_PER_SEC;
                    let score_change = SCORE_HOLD_DROP_PENALTY_PER_SECOND * rem_sec;
                    let health_uncapped = rem_sec * HEALTH_HOLD_DROP_PENALTY_PER_SECOND;
                    let health_max =
                        HEALTH_HOLD_DROP_PENALTY_MAX - if hold.hit_base { -HEALTH_MISS_BONUS } else { 0.0 };
                    let health_change = health_uncapped.clamp(health_max, 0.0);

                    self.break_combo();
                    self.player_vocals_volume = 0.0;
                    self.pending_miss_sfx = true;
                    self.song_score += score_change;
                    self.health = (self.health + health_change).clamp(HEALTH_MIN, HEALTH_MAX);
                    let applied_health_delta = self.health - prev_health;
                    self.pending_hooks.push((
                        HookKind::HoldDrop,
                        ScriptEvent::new(HookKind::HoldDrop)
                            .with_field("id", hold.note_id as i64)
                            .with_field("dir", hold.lane.index() as i64)
                            .with_field("remaining_ms", hold.remaining_ms)
                            .with_field("health_change", applied_health_delta)
                            .with_field("score_change", score_change)
                            .with_field("combo_before", prev_combo as i64)
                            .with_field("max_combo_before", prev_max_combo as i64)
                            .with_field("play_sound", true)
                            .with_field("play_anim", true),
                    ));
                    self.player_strumline.pending_char_events.push(CharNoteSignal::HoldDrop {
                        direction: hold.lane,
                        is_combo_break: true,
                        combo_before: prev_combo,
                    });
                }
                self.player_strumline.active_holds[lane_idx] = None;
            }
        }
    }

    pub(super) fn apply_missed_hold_head_penalty(&mut self, sustain_length_ms: f64) {
        if sustain_length_ms <= HOLD_DROP_PENALTY_THRESHOLD_MS {
            return;
        }
        // Matches V-Slice PlayState.hx: when the base note was already missed,
        // apply the remaining sustain score penalty without extra health loss.
        let rem_sec = sustain_length_ms / MS_PER_SEC;
        self.song_score += SCORE_HOLD_DROP_PENALTY_PER_SECOND * rem_sec;
    }

    fn break_combo(&mut self) {
        if self.popups_visible && self.tallies.combo >= COMBO_POPUP_THRESHOLD {
            self.pending_popups.push(crate::play::hud::popups::PopupEvent {
                judgement: None,
                combo: Some(0),
            });
        }
        self.tallies.combo = 0;
    }

    pub fn on_note_miss(&mut self, health_change: f64) {
        self.player_vocals_volume = 0.0;
        self.pending_miss_sfx = true;
        self.apply_score(f64::from(miss_score()), Judgement::Miss, health_change, true);
    }

    pub fn ghost_note_miss(&mut self, direction: NoteDirection, has_possible_notes: bool) {
        let prev_health = self.health;
        self.player_vocals_volume = 0.0;
        self.pending_miss_sfx = true;
        self.song_score += SCORE_GHOST_MISS_PENALTY;
        self.health = (self.health + HEALTH_GHOST_MISS_PENALTY).clamp(HEALTH_MIN, HEALTH_MAX);
        let applied_health_delta = self.health - prev_health;
        self.pending_hooks.push((
            HookKind::GhostMiss,
            ScriptEvent::new(HookKind::GhostMiss)
                .with_field("dir", direction.index() as i64)
                .with_field("has_possible_notes", has_possible_notes)
                .with_field("health_change", applied_health_delta)
                .with_field("score_change", SCORE_GHOST_MISS_PENALTY)
                .with_field("play_sound", true)
                .with_field("play_anim", true),
        ));
    }

    pub fn apply_score(&mut self, score: f64, judgement: Judgement, health_change: f64, is_combo_break: bool) {
        judgement.record(&mut self.tallies);
        self.song_score += score;
        self.health = (self.health + health_change).clamp(HEALTH_MIN, HEALTH_MAX);
        if is_combo_break {
            self.break_combo();
        } else {
            self.tallies.combo += 1;
            self.tallies.max_combo = self.tallies.max_combo.max(self.tallies.combo);
        }
    }
}
