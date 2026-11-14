use crate::utils::constants::{
    HEALTH_HOLD_BONUS_PER_SECOND, HEALTH_HOLD_DROP_PENALTY_PER_SECOND, HEALTH_MAX, HEALTH_MIN,
    HOLD_DROP_PENALTY_THRESHOLD_MS, SCORE_HOLD_BONUS_PER_SECOND, SCORE_HOLD_DROP_PENALTY_PER_SECOND,
};

use super::super::scene::PlayState;
use super::note::ActiveHold;

impl PlayState {
    pub(super) fn process_opponent_notes(&mut self, pos_ms: f64) {
        let mut hit_lanes = Vec::new();
        for note in &mut self.opponent_strumline.notes {
            if !note.has_been_hit && pos_ms >= note.data.time_ms {
                note.has_been_hit = true;
                let hold = if note.is_hold_note() {
                    note.hold_hit = true;
                    let end_ms = note.data.time_ms + note.data.sustain_length_ms;
                    Some(ActiveHold {
                        note_id: note.data.id,
                        lane: note.lane,
                        end_ms,
                        remaining_ms: (end_ms - pos_ms).max(0.0),
                        dropped: false,
                    })
                } else {
                    None
                };
                hit_lanes.push((note.lane, hold));
            }
        }

        for (lane, hold) in hit_lanes {
            self.opponent_vocals_volume = 1.0;
            self.opponent_strumline.play_confirm(lane);
            if let Some(h) = hold {
                self.opponent_strumline.active_holds[lane.index()] = Some(h);
            }
        }

        for lane_idx in 0..4 {
            let Some(mut hold) = self.opponent_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = (hold.end_ms - pos_ms).max(0.0);
            if let Some(note) = self
                .opponent_strumline
                .notes
                .iter_mut()
                .find(|n| n.data.id == hold.note_id)
            {
                note.hold_remaining_ms = hold.remaining_ms;
            }
            if hold.remaining_ms <= 10.0 {
                self.opponent_strumline.active_holds[lane_idx] = None;
            } else {
                self.opponent_strumline.hold_confirm(hold.lane);
                self.opponent_strumline.active_holds[lane_idx] = Some(hold);
            }
        }
    }

    pub(super) fn process_player_holds(&mut self, elapsed_s: f64, pos_ms: f64) {
        for lane_idx in 0..4 {
            let Some(mut hold) = self.player_strumline.active_holds[lane_idx] else {
                continue;
            };
            hold.remaining_ms = (hold.end_ms - pos_ms).max(0.0);
            let held = self.is_bot_play_mode || self.player_strumline.held_lanes[lane_idx];

            if held && !hold.dropped {
                if let Some(note) = self
                    .player_strumline
                    .notes
                    .iter_mut()
                    .find(|n| n.data.id == hold.note_id)
                {
                    note.hold_remaining_ms = hold.remaining_ms;
                }
                if hold.remaining_ms <= 10.0 {
                    self.player_strumline.active_holds[lane_idx] = None;
                    continue;
                }
                self.player_vocals_volume = 1.0;
                self.song_score += SCORE_HOLD_BONUS_PER_SECOND * elapsed_s;
                self.health = (self.health + HEALTH_HOLD_BONUS_PER_SECOND * elapsed_s)
                    .clamp(HEALTH_MIN, HEALTH_MAX);
                self.player_strumline.hold_confirm(hold.lane);
                self.player_strumline.active_holds[lane_idx] = Some(hold);
            } else {
                hold.dropped = true;
                if let Some(note) = self
                    .player_strumline
                    .notes
                    .iter_mut()
                    .find(|n| n.data.id == hold.note_id)
                {
                    note.hold_dropped = true;
                    note.hold_remaining_ms = hold.remaining_ms;
                }
                if hold.remaining_ms > HOLD_DROP_PENALTY_THRESHOLD_MS {
                    self.tallies.combo = 0;
                    self.player_vocals_volume = 0.0;
                    self.pending_miss_sfx = true;
                    self.song_score += SCORE_HOLD_DROP_PENALTY_PER_SECOND * elapsed_s;
                    self.health = (self.health + HEALTH_HOLD_DROP_PENALTY_PER_SECOND * elapsed_s)
                        .clamp(HEALTH_MIN, HEALTH_MAX);
                }
                self.player_strumline.active_holds[lane_idx] = None;
            }
        }
    }
}

