use std::collections::HashMap;

use bevy::prelude::*;

use crate::animations::atlas_composite::CompositeClip;
use crate::animations::{AnimationController, Bopper};
use crate::play::notes::CharNoteSignal;
use crate::play::notes::data::{NoteDirection, NoteKindManager};
use crate::play::stage::schema::StageCharacterSlot;

use super::schema::{CharacterData, CharacterRole};

#[derive(Debug, Clone)]
pub struct SheetSource {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub frame_sizes: HashMap<usize, [f32; 2]>,
}

#[derive(Debug, Clone)]
pub enum ClipVisual {
    Sheet { source_idx: usize },
    Composite {
        source_idx: usize,
        clip: CompositeClip,
        feet_anchor: [f32; 2],
    },
}

#[derive(Component)]
#[allow(dead_code)]
pub struct CharacterActor {
    pub id: String,
    pub role: CharacterRole,
    pub slot: StageCharacterSlot,
    pub data: CharacterData,
    pub controller: AnimationController,
    pub bopper: Bopper,
    pub sources: Vec<SheetSource>,
    pub clips: HashMap<String, ClipVisual>,
    pub idle_size: [f32; 2],
    pub facing_flip_x: bool,
    pub camera_focus_point: [f64; 2],
    pub hold_timer: f64,
    pub cur_no_anim: bool,
    pub cur_suffix: String,
    pub combo_counts: Vec<u32>,
    pub drop_counts: Vec<u32>,
    pub sampler_applied: bool,
    pub sheet_entity: Entity,
    pub piece_entities: Vec<Entity>,
}

impl CharacterActor {
    pub fn reset_character(&mut self) {
        self.bopper.dance_every = self.data.dance_every;
        self.hold_timer = 0.0;
        self.cur_no_anim = false;
        self.cur_suffix.clear();

        let start = self.data.starting_animation.clone();
        if start != "idle" && self.controller.has_animation(&start) {
            self.controller.play(&start, true, false, 0);
            if start.starts_with("danceRight") {
                self.bopper.has_danced = true;
            }
        } else {
            self.dance(true);
        }
        self.recompute_camera_focus();
    }

    pub fn total_scale(&self) -> [f32; 2] {
        [
            self.data.scale[0] * self.slot.scale[0],
            self.data.scale[1] * self.slot.scale[1],
        ]
    }

    pub fn effective_flip(&self) -> [bool; 2] {
        [self.facing_flip_x ^ self.slot.flip[0], self.slot.flip[1]]
    }

    pub fn recompute_camera_focus(&mut self) {
        let scale = self.total_scale();
        let cx = self.slot.position[0] + self.data.offsets[0];
        let cy = self.slot.position[1] - (self.idle_size[1] * scale[1]) * 0.5 + self.data.offsets[1];
        self.camera_focus_point = [
            f64::from(cx + self.data.camera_offsets[0] + self.slot.camera_offset[0]),
            f64::from(cy + self.data.camera_offsets[1] + self.slot.camera_offset[1]),
        ];
    }

    pub fn is_singing(&self) -> bool {
        self.controller
            .current_name()
            .is_some_and(|n| n.starts_with("sing") && !n.ends_with("-end"))
    }

    pub fn dance(&mut self, force: bool) {
        if !force {
            if self.is_singing() {
                return;
            }
            if let Some(cur) = self.controller.current_name()
                && !cur.starts_with("dance")
                && !cur.starts_with("idle")
                && !self.controller.is_finished()
            {
                return;
            }
        }
        self.bopper.dance(&mut self.controller, force);
    }

    pub fn on_step_hit(&mut self, step: i32) {
        if self.bopper.dance_every <= 0.0 {
            return;
        }
        let period = (self.bopper.dance_every * 4.0).round() as i32;
        if period > 0 && step.rem_euclid(period) == 0 {
            let restart = self.bopper.should_bop;
            self.dance(restart);
        }
    }

    pub fn on_update(
        &mut self,
        elapsed_s: f64,
        song_pos_ms: Option<f64>,
        step_length_ms: f64,
        player_holding_note: bool,
    ) {
        self.controller.update(elapsed_s as f32, song_pos_ms);
        self.bopper.sync_finished(&self.controller);

        if self.controller.is_finished()
            && let Some(cur) = self.controller.current_name().map(str::to_string)
            && !cur.ends_with("-hold")
        {
            let hold_name = format!("{cur}-hold");
            if self.controller.has_animation(&hold_name) {
                self.controller.play(&hold_name, false, false, 0);
            }
        }

        if self.is_singing() {
            self.hold_timer += elapsed_s;
            let mut sing_time_sec = f64::from(self.data.sing_time) * (step_length_ms / 1000.0);
            if self.controller.current_name().is_some_and(|n| n.ends_with("miss")) {
                sing_time_sec *= 2.0;
            }
            let should_stop = self.role != CharacterRole::Player || !player_holding_note;
            if self.hold_timer > sing_time_sec && should_stop {
                self.hold_timer = 0.0;
                if let Some(cur) = self.controller.current_name() {
                    let base = cur.strip_suffix("-hold").unwrap_or(cur);
                    let end_name = format!("{base}-end");
                    if self.controller.has_animation(&end_name) {
                        self.controller.play(&end_name, true, false, 0);
                    } else {
                        self.dance(true);
                    }
                }
            }
        } else {
            self.hold_timer = 0.0;
        }

        if self.controller.is_finished()
            && self.controller.current_name().is_some_and(|n| {
                (n.ends_with("-end") && !n.starts_with("idle") && !n.starts_with("dance"))
                    || n.starts_with("combo")
                    || n.starts_with("drop")
            })
        {
            self.dance(true);
        }
    }

    pub fn handle_player_signal(&mut self, sig: &CharNoteSignal, kinds: &NoteKindManager) {
        match sig {
            CharNoteSignal::Hit { direction, note_kind, good_judgement, combo } => {
                let kind = kinds.get(note_kind.as_deref());
                self.cur_no_anim = kind.is_some_and(|k| k.no_anim);
                self.cur_suffix = kind.map(|k| k.suffix.clone()).unwrap_or_default();
                if self.cur_no_anim {
                    return;
                }
                if self.role == CharacterRole::Player {
                    let suffix = self.cur_suffix.clone();
                    self.play_sing(*direction, false, &suffix);
                } else if self.role == CharacterRole::Spectator && let Some(good) = good_judgement {
                    if *good {
                        self.play_combo(*combo);
                    } else {
                        self.play_drop(*combo);
                    }
                }
            }
            CharNoteSignal::HoldTick if self.role == CharacterRole::Player => self.hold_timer = 0.0,
            CharNoteSignal::Miss { direction, combo_before } => match self.role {
                CharacterRole::Player => self.play_sing(*direction, true, ""),
                CharacterRole::Spectator => self.play_drop(*combo_before),
                CharacterRole::Opponent => {}
            },
            CharNoteSignal::HoldDrop { direction, is_combo_break, combo_before } => match self.role {
                CharacterRole::Player => self.play_sing(*direction, true, ""),
                CharacterRole::Spectator if *is_combo_break => self.play_drop(*combo_before),
                _ => {}
            },
            CharNoteSignal::GhostMiss { direction } if self.role == CharacterRole::Player => {
                self.play_sing(*direction, true, "");
            }
            _ => {}
        }
    }

    pub fn handle_opponent_signal(&mut self, sig: &CharNoteSignal, kinds: &NoteKindManager) {
        if self.role != CharacterRole::Opponent {
            return;
        }
        match sig {
            CharNoteSignal::Hit { direction, note_kind, .. } => {
                let kind = kinds.get(note_kind.as_deref());
                self.cur_no_anim = kind.is_some_and(|k| k.no_anim);
                self.cur_suffix = kind.map(|k| k.suffix.clone()).unwrap_or_default();
                if !self.cur_no_anim {
                    let suffix = self.cur_suffix.clone();
                    self.play_sing(*direction, false, &suffix);
                }
            }
            CharNoteSignal::HoldTick => self.hold_timer = 0.0,
            _ => {}
        }
    }

    pub fn play_sing(&mut self, dir: NoteDirection, miss: bool, suffix: &str) {
        let dir_upper = match dir {
            NoteDirection::Left => "LEFT",
            NoteDirection::Down => "DOWN",
            NoteDirection::Up => "UP",
            NoteDirection::Right => "RIGHT",
        };
        let miss_part = if miss { "miss" } else { "" };
        let raw_name = if suffix.is_empty() {
            format!("sing{dir_upper}{miss_part}")
        } else {
            format!("sing{dir_upper}{miss_part}-{suffix}")
        };
        self.hold_timer = 0.0;
        if let Some(resolved) = self.controller.resolve_name(&raw_name) {
            self.controller.play(&resolved, true, false, 0);
        }
    }

    pub fn play_combo(&mut self, combo: u32) {
        let name = format!("combo{combo}");
        if self.controller.has_animation(&name) {
            self.bopper.play_animation(&mut self.controller, &name, true, true);
        }
    }

    pub fn play_drop(&mut self, combo: u32) {
        if let Some(count) = self.drop_counts.iter().copied().filter(|&c| combo >= c).max() {
            let name = format!("drop{count}");
            self.bopper.play_animation(&mut self.controller, &name, true, true);
        }
    }
}

pub fn find_count_animations(controller: &AnimationController, prefix: &str) -> Vec<u32> {
    let mut nums: Vec<u32> = controller
        .animations
        .keys()
        .filter_map(|k| k.strip_prefix(prefix).and_then(|rest| rest.parse::<u32>().ok()))
        .collect();
    nums.sort_unstable();
    nums
}
