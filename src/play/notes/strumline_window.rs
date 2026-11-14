use crate::utils::constants::{HIT_WINDOW_MS, KEY_COUNT, SCREEN_HEIGHT, VWOOSH_DURATION_S};
use crate::utils::easing::EaseKind;

use super::receptor::NoteVibrationStatus;
use super::strumline::{HoldCoverCmd, Strumline};

#[derive(Debug, Clone, Copy)]
pub struct VwooshTween {
    pub note_idx: usize,
    pub start_y_offset: f32,
    pub end_y_offset: f32,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub ease: EaseKind,
    pub kill_on_finish: bool,
}

impl Strumline {
    pub fn hit_note(&mut self, note_idx: usize, remove_note: bool) {
        let Some(note) = self.notes.get_mut(note_idx) else {
            return;
        };
        let dir = note.lane;
        let is_hold = note.is_hold_note();
        let end_ms = note.data.time_ms + note.data.sustain_length_ms;
        let rem = (end_ms - self.visual_song_position).clamp(0.0, note.data.sustain_length_ms);
        note.has_been_hit = true;
        if is_hold {
            note.hold_hit = true;
            note.hold_dropped = false;
            note.hold_remaining_ms = rem;
        }
        if remove_note {
            note.killed = true;
        } else {
            note.desaturated = true;
            note.alpha = 0.5;
        }
        self.play_confirm(dir);
        if is_hold {
            self.pending_hold_covers.push(HoldCoverCmd::Start(dir));
        }
    }

    pub fn kill_note(&mut self, note_idx: usize) {
        if let Some(note) = self.notes.get_mut(note_idx) {
            note.killed = true;
            if note.is_hold_note() && !note.hold_hit {
                note.hold_dropped = true;
                note.hold_killed = true;
            }
        }
    }

    pub fn fade_in_arrows(&mut self) {
        for (idx, rec) in self.receptors.iter_mut().enumerate() {
            rec.start_fade_in(idx);
        }
    }

    pub fn fade_out_arrows(&mut self) {
        for rec in &mut self.receptors {
            rec.start_fade_out();
        }
    }

    pub fn vwoosh_notes(&mut self) {
        let delta = if self.is_downscroll { -SCREEN_HEIGHT } else { SCREEN_HEIGHT };
        for idx in self.active_start_index..self.next_note_index.min(self.notes.len()) {
            let note = &self.notes[idx];
            if note.killed && !note.is_hold_alive() {
                continue;
            }
            self.vwoosh_tweens.push(VwooshTween {
                note_idx: idx,
                start_y_offset: note.y_offset,
                end_y_offset: note.y_offset + delta,
                elapsed_s: 0.0,
                duration_s: VWOOSH_DURATION_S,
                ease: EaseKind::ExpoIn,
                kill_on_finish: true,
            });
        }
    }

    pub fn vwoosh_in_notes(&mut self) {
        let start_off = if self.is_downscroll { -200.0 } else { 200.0 };
        for idx in self.active_start_index..self.next_note_index.min(self.notes.len()) {
            if let Some(note) = self.notes.get_mut(idx) {
                if note.killed && !note.is_hold_alive() {
                    continue;
                }
                note.y_offset = start_off;
                self.vwoosh_tweens.push(VwooshTween {
                    note_idx: idx,
                    start_y_offset: start_off,
                    end_y_offset: 0.0,
                    elapsed_s: 0.0,
                    duration_s: VWOOSH_DURATION_S,
                    ease: EaseKind::ExpoOut,
                    kill_on_finish: false,
                });
            }
        }
    }

    pub fn update_notes(&mut self, elapsed_s: f64, song_pos_ms: f64, start_timestamp_ms: f64) {
        self.visual_song_position = match &self.scroll_velocity {
            Some(sv) => sv.resolve_visual_song_position(song_pos_ms, self.scroll_speed, &mut self.lag_state),
            None => song_pos_ms,
        };

        if !self.has_notes_on_screen() {
            self.ghost_tap_timer = (self.ghost_tap_timer - elapsed_s).max(0.0);
        }
        for i in 0..KEY_COUNT {
            let held = !self.held_keys[i].is_empty();
            self.receptors[i].tick(elapsed_s, held);
            if !held && self.is_player {
                self.vibrations.statuses[i] = NoteVibrationStatus::Idle;
            }
        }
        self.tick_vwoosh(elapsed_s);
        self.advance_spawn_window(song_pos_ms, start_timestamp_ms);
        self.retire_offscreen_notes(song_pos_ms);
    }

    fn tick_vwoosh(&mut self, elapsed_s: f64) {
        if self.vwoosh_tweens.is_empty() {
            return;
        }
        let mut i = 0;
        while i < self.vwoosh_tweens.len() {
            let mut tw = self.vwoosh_tweens[i];
            tw.elapsed_s += elapsed_s;
            let t = tw.ease.eval((tw.elapsed_s / tw.duration_s).clamp(0.0, 1.0)) as f32;
            if let Some(note) = self.notes.get_mut(tw.note_idx) {
                note.y_offset = tw.start_y_offset + (tw.end_y_offset - tw.start_y_offset) * t;
                if tw.elapsed_s >= tw.duration_s && tw.kill_on_finish {
                    note.killed = true;
                    note.hold_killed = true;
                }
            }
            if tw.elapsed_s >= tw.duration_s {
                self.vwoosh_tweens.swap_remove(i);
            } else {
                self.vwoosh_tweens[i] = tw;
                i += 1;
            }
        }
    }

    fn advance_spawn_window(&mut self, song_pos_ms: f64, start_timestamp_ms: f64) {
        let hit_window_start = song_pos_ms - HIT_WINDOW_MS;
        let render_window_start = song_pos_ms.max(self.visual_song_position) + self.render_distance_ms();
        while self.next_note_index < self.notes.len() {
            let note = &self.notes[self.next_note_index];
            if note.data.time_ms < start_timestamp_ms || note.data.time_ms < hit_window_start {
                self.next_note_index += 1;
                continue;
            }
            if note.data.time_ms > render_window_start {
                break;
            }
            self.next_note_index += 1;
        }
    }

    fn retire_offscreen_notes(&mut self, song_pos_ms: f64) {
        let render_dist = self.render_distance_ms();
        let vis_pos = self.visual_song_position;
        let rec_y = self.receptor_y();
        let rate = self.scroll_rate();
        let downscroll = self.is_downscroll;

        for idx in self.active_start_index..self.next_note_index.min(self.notes.len()) {
            let note = &mut self.notes[idx];
            if !note.killed && (note.handled_miss || note.has_been_hit) {
                let y = rec_y + ((note.data.time_ms - vis_pos) * rate) as f32 + note.y_offset;
                let offscreen = if downscroll { y < -450.0 } else { y > 450.0 };
                if offscreen {
                    note.killed = true;
                }
            }
            if note.is_hold_note() && !note.hold_killed && note.hold_dropped {
                let window_end = note.data.time_ms + note.data.sustain_length_ms + HIT_WINDOW_MS + render_dist / 8.0;
                if song_pos_ms >= window_end {
                    note.hold_killed = true;
                }
            }
        }
        while self.active_start_index < self.next_note_index
            && self.notes[self.active_start_index].killed
            && !self.notes[self.active_start_index].is_hold_alive()
        {
            self.active_start_index += 1;
        }
    }
}
