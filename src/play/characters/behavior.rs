use std::collections::{HashMap, VecDeque};

use bevy::prelude::*;

use crate::animations::atlas_composite::CompositeClip;
use crate::animations::{AnimationController, BaseAnimationData, Bopper};
use crate::play::notes::CharNoteSignal;
use crate::play::notes::data::{NoteDirection, NoteKindManager};
use crate::play::stage::schema::StageCharacterSlot;

use super::schema::{CharacterData, CharacterRole};

#[derive(Debug, Clone)]
pub struct SheetSource {
    pub asset_key: String,
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub extra_layers: Vec<(Handle<Image>, Handle<TextureAtlasLayout>)>,
    pub frame_sizes: HashMap<usize, [f32; 2]>,
}

impl SheetSource {
    pub fn atlas_pair(&self, spritemap_idx: usize) -> (&Handle<Image>, &Handle<TextureAtlasLayout>) {
        if spritemap_idx == 0 {
            (&self.texture, &self.layout)
        } else if let Some((tex, layout)) = self.extra_layers.get(spritemap_idx - 1) {
            (tex, layout)
        } else {
            (&self.texture, &self.layout)
        }
    }
}

#[derive(Debug, Clone)]
pub enum ClipVisual {
    Sheet { source_idx: usize },
    Composite {
        source_idx: usize,
        clip: CompositeClip,
        stage_matrix: Option<[f32; 6]>,
    },
}

/// Configuration for a character afterimage trail (`FunkTrail` parity).
#[derive(Debug, Clone)]
pub struct CharacterTrailConfig {
    pub length: usize,
    pub delay_s: f64,
    pub alpha: f32,
    pub diff: f32,
    pub z_layer: f32,
}

/// Captured sheet-space pose for a single character trail ghost.
#[derive(Debug, Clone)]
pub struct CharacterTrailSample {
    pub source_idx: usize,
    pub frame_idx: usize,
    pub world_cx: f32,
    pub world_cy: f32,
    pub foff: [f32; 2],
    pub scale: [f32; 2],
    pub flip: [bool; 2],
    pub rot: f32,
}

/// Ring-buffer state driving character trail capture and fade falloff.
#[derive(Debug, Clone)]
pub struct CharacterTrailState {
    pub config: CharacterTrailConfig,
    pub elapsed_s: f64,
    pub samples: VecDeque<CharacterTrailSample>,
}

impl CharacterTrailState {
    pub fn new(config: CharacterTrailConfig) -> Self {
        Self {
            config,
            elapsed_s: 0.0,
            samples: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.elapsed_s = 0.0;
        self.samples.clear();
    }

    pub fn step_and_capture(&mut self, dt_s: f64, sample: CharacterTrailSample) -> bool {
        if self.config.length == 0 || dt_s <= 0.0 {
            return false;
        }
        self.elapsed_s += dt_s;
        let delay = self.config.delay_s.max(0.0);
        if self.elapsed_s < delay {
            return false;
        }
        self.elapsed_s = if delay > 0.0 {
            (self.elapsed_s - delay).min(delay)
        } else {
            0.0
        };
        self.samples.push_front(sample);
        self.samples.truncate(self.config.length);
        true
    }

    pub fn ghost_alpha(&self, index: usize, parent_alpha: f32) -> f32 {
        ((self.config.alpha - (index as f32) * self.config.diff) * parent_alpha).clamp(0.0, 1.0)
    }
}

/// Mirrors a primary character's animation and pose in a backing Z layer (used by `-dark` overlays).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharacterUnderlayConfig {
    pub parent_role: CharacterRole,
    pub offset: [f32; 2],
}

/// Fallback offscreen render-target side length (px) when composite bounds are unavailable.
pub const COMPOSITE_RT_SIZE: u32 = 1600;
pub const COMPOSITE_RT_MIN_SIZE: u32 = 512;
pub const COMPOSITE_RT_MAX_SIZE: u32 = 2048;
const COMPOSITE_RT_PADDING: f32 = 128.0;

/// Computes clamped `[width, height]` for an offscreen composite `RenderTarget`,
/// falling back to `COMPOSITE_RT_SIZE` on any axis whose bounds are `<= 0.0` or non-finite.
pub(crate) fn compute_composite_rt_dimensions(
    ref_size: [f32; 2],
    max_clip_size: [f32; 2],
) -> [u32; 2] {
    let axis = |r: f32, c: f32| -> u32 {
        let r_ok = r.is_finite() && r > 0.0;
        let c_ok = c.is_finite() && c > 0.0;
        let extent = match (r_ok, c_ok) {
            (true, true) => r.max(2.0 * c - r),
            (true, false) => r,
            (false, true) => c,
            (false, false) => return COMPOSITE_RT_SIZE,
        };
        let padded = extent + COMPOSITE_RT_PADDING;
        if !padded.is_finite() || padded <= 0.0 {
            return COMPOSITE_RT_SIZE;
        }
        (padded.ceil() as u32).clamp(COMPOSITE_RT_MIN_SIZE, COMPOSITE_RT_MAX_SIZE)
    };
    [
        axis(ref_size[0], max_clip_size[0]),
        axis(ref_size[1], max_clip_size[1]),
    ]
}

#[derive(Component, Debug, Clone, Copy)]
pub struct CompositeRtCamera;

#[derive(Clone, Debug)]
pub struct CompositeRenderTarget {
    #[allow(dead_code)] // Retains a strong Handle<Image> for the offscreen RT independently of camera/quad entities.
    pub image: Handle<Image>,
    pub camera_entity: Entity,
    pub quad_entity: Entity,
    pub render_layer: usize,
}

impl CompositeRenderTarget {
    /// Despawns the offscreen RT camera and display quad entities.
    pub fn despawn(&self, commands: &mut Commands) {
        if self.camera_entity != Entity::PLACEHOLDER {
            commands.entity(self.camera_entity).despawn();
        }
        if self.quad_entity != Entity::PLACEHOLDER {
            commands.entity(self.quad_entity).despawn();
        }
    }
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
    pub deferred_gameover_anims: Vec<BaseAnimationData>,
    pub idle_size: [f32; 2],
    pub facing_flip_x: bool,
    pub camera_focus_point: [f64; 2],
    pub hold_timer: f64,
    pub cur_no_anim: bool,
    pub cur_suffix: String,
    pub combo_counts: Vec<u32>,
    pub drop_counts: Vec<u32>,
    pub sampler_applied: bool,
    pub mod_tint: [f32; 4],
    pub mod_visible: bool,
    pub mod_z: Option<f32>,
    pub script_anim_override: bool,
    pub sheet_entity: Entity,
    pub piece_entities: Vec<Entity>,
    pub composite_rt: Option<CompositeRenderTarget>,
    pub trail: Option<CharacterTrailState>,
    pub trail_entities: Vec<Entity>,
    pub mirror_of: Option<CharacterUnderlayConfig>,
}

impl CharacterActor {
    pub fn despawn_composite_rt(&self, commands: &mut Commands) {
        if let Some(ref rt) = self.composite_rt {
            rt.despawn(commands);
        }
    }
    pub fn reset_character(&mut self) {
        self.bopper.dance_every = self.data.dance_every;
        self.bopper.can_play_other_anims = true;
        self.hold_timer = 0.0;
        self.cur_no_anim = false;
        self.cur_suffix.clear();
        if let Some(trail) = &mut self.trail {
            trail.reset();
        }

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

    /// Computes dynamic offscreen `RenderTarget` dimensions `[width, height]` from
    /// composite clip bounds and `idle_size` scaled by `total_scale`, clamped to
    /// `[COMPOSITE_RT_MIN_SIZE, COMPOSITE_RT_MAX_SIZE]` with `COMPOSITE_RT_SIZE` fallback.
    pub fn composite_rt_size(&self) -> [u32; 2] {
        let raw_scale = self.total_scale();
        if !raw_scale[0].is_finite()
            || !raw_scale[1].is_finite()
            || raw_scale[0].abs() <= f32::EPSILON
            || raw_scale[1].abs() <= f32::EPSILON
        {
            return [COMPOSITE_RT_SIZE, COMPOSITE_RT_SIZE];
        }
        let scale = [raw_scale[0].abs(), raw_scale[1].abs()];
        let apply_sm = self.data.effective_atlas_settings().apply_stage_matrix;
        let mut max_clip = [0.0_f32, 0.0_f32];
        for vis in self.clips.values() {
            if let ClipVisual::Composite {
                clip,
                stage_matrix,
                ..
            } = vis
            {
                let mut bw = clip.bounds_size[0];
                let mut bh = clip.bounds_size[1];
                if apply_sm && let Some(sm) = stage_matrix {
                    bw *= sm[0].abs().max(0.01);
                    bh *= sm[3].abs().max(0.01);
                }
                if bw.is_finite() && bw > 0.0 {
                    max_clip[0] = max_clip[0].max(bw * scale[0]);
                }
                if bh.is_finite() && bh > 0.0 {
                    max_clip[1] = max_clip[1].max(bh * scale[1]);
                }
            }
        }
        let ref_size = [
            if self.idle_size[0].is_finite() && self.idle_size[0] > 0.0 {
                self.idle_size[0] * scale[0]
            } else {
                0.0
            },
            if self.idle_size[1].is_finite() && self.idle_size[1] > 0.0 {
                self.idle_size[1] * scale[1]
            } else {
                0.0
            },
        ];
        compute_composite_rt_dimensions(ref_size, max_clip)
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

    pub fn is_playing_special_anim(&self) -> bool {
        if !self.bopper.can_play_other_anims {
            return !self.controller.is_finished();
        }
        self.controller.current_name().is_some_and(|cur| {
            !cur.starts_with("dance")
                && !cur.starts_with("idle")
                && !self.is_singing()
                && !self.controller.is_finished()
        })
    }

    pub fn dance(&mut self, force: bool) {
        if self.is_playing_special_anim() {
            return;
        }
        if !force && self.is_singing() {
            return;
        }
        self.bopper.dance(&mut self.controller, force);
    }

    pub fn on_step_hit(&mut self, step: i32) {
        if self.bopper.dance_every <= 0.0
            || self.is_singing()
            || self.is_playing_special_anim()
        {
            return;
        }
        let period = (self.bopper.dance_every * 4.0).round() as i32;
        if period > 0 && step.rem_euclid(period) == 0 {
            let can_bop = match self.controller.current_name() {
                None => true,
                Some(cur) if cur.starts_with("dance") => true,
                Some(cur) if cur.starts_with("idle") => {
                    cur.ends_with("-hold") || self.controller.is_finished()
                }
                Some(_) => self.controller.is_finished(),
            };
            if can_bop {
                self.bopper.dance(&mut self.controller, true);
            }
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

#[cfg(test)]
mod tests {
    use super::{
        COMPOSITE_RT_MAX_SIZE, COMPOSITE_RT_MIN_SIZE, COMPOSITE_RT_SIZE,
        compute_composite_rt_dimensions,
    };

    #[test]
    fn composite_rt_dimensions_fall_back_on_invalid_bounds() {
        assert_eq!(
            compute_composite_rt_dimensions([0.0, -5.0], [f32::NAN, f32::INFINITY]),
            [COMPOSITE_RT_SIZE, COMPOSITE_RT_SIZE]
        );
    }

    #[test]
    fn composite_rt_dimensions_clamp_between_min_and_max() {
        assert_eq!(
            compute_composite_rt_dimensions([120.0, 200.0], [150.0, 220.0]),
            [COMPOSITE_RT_MIN_SIZE, COMPOSITE_RT_MIN_SIZE]
        );
        assert_eq!(
            compute_composite_rt_dimensions([2200.0, 1900.0], [2400.0, 2100.0]),
            [COMPOSITE_RT_MAX_SIZE, COMPOSITE_RT_MAX_SIZE]
        );
    }

    #[test]
    fn composite_rt_dimensions_account_for_larger_clip_bounds_and_padding() {
        // ref = 500, max_clip = 700 -> symmetric extent = 2*700 - 500 = 900 (+ 128 pad = 1028)
        assert_eq!(
            compute_composite_rt_dimensions([500.0, 600.0], [700.0, 600.0]),
            [1028, 728]
        );
    }
}
