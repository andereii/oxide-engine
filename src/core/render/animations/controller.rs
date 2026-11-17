use std::collections::HashMap;

use bevy::prelude::Component;

#[derive(Debug, Clone)]
pub struct AnimationDef {
    pub name: String,
    pub frames: Vec<usize>,
    pub frame_offsets: Vec<[f32; 2]>,
    pub frame_rotated: Vec<bool>,
    pub fps: f32,
    pub looped: bool,
    pub offset: [f32; 2],
    pub flip: [bool; 2],
}

#[derive(Component, Debug, Clone)]
pub struct AnimationController {
    pub animations: HashMap<String, AnimationDef>,
    pub current: Option<String>,
    pub frame_cursor: usize,
    pub elapsed_s: f32,
    pub finished: bool,
    pub reversed: bool,
    pub time_scale: f32,
    pub should_use_conductor_sync: bool,
    last_song_position_ms: Option<f64>,
}

impl Default for AnimationController {
    fn default() -> Self {
        Self {
            animations: HashMap::new(),
            current: None,
            frame_cursor: 0,
            elapsed_s: 0.0,
            finished: false,
            reversed: false,
            time_scale: 1.0,
            should_use_conductor_sync: false,
            last_song_position_ms: None,
        }
    }
}

impl AnimationController {
    pub fn add_anim(
        &mut self,
        name: impl Into<String>,
        frames: Vec<usize>,
        fps: f32,
        looped: bool,
    ) {
        self.add_anim_frames(name, frames, Vec::new(), Vec::new(), fps, looped);
    }

    pub fn add_anim_frames(
        &mut self,
        name: impl Into<String>,
        frames: Vec<usize>,
        frame_offsets: Vec<[f32; 2]>,
        frame_rotated: Vec<bool>,
        fps: f32,
        looped: bool,
    ) {
        let name = name.into();
        self.animations.insert(
            name.clone(),
            AnimationDef {
                name,
                frames,
                frame_offsets,
                frame_rotated,
                fps: fps.max(1.0),
                looped,
                offset: [0.0, 0.0],
                flip: [false, false],
            },
        );
    }

    pub fn set_offset(&mut self, name: &str, offset: [f32; 2]) {
        if let Some(anim) = self.animations.get_mut(name) {
            anim.offset = offset;
        }
    }

    pub fn set_flip(&mut self, name: &str, flip: [bool; 2]) {
        if let Some(anim) = self.animations.get_mut(name) {
            anim.flip = flip;
        }
    }

    pub fn has_animation(&self, name: &str) -> bool {
        self.animations.contains_key(name)
    }

    pub fn is_finished(&self) -> bool {
        self.finished
    }

    pub fn current_name(&self) -> Option<&str> {
        self.current.as_deref()
    }

    /// Resolves `name` by stripping trailing `-suffix` segments if missing (mirrors Funkin `Bopper.correctAnimationName`).
    pub fn resolve_name(&self, name: &str) -> Option<String> {
        let mut candidate = name;
        loop {
            if self.has_animation(candidate) {
                return Some(candidate.to_string());
            }
            let pos = candidate.rfind('-')?;
            candidate = &candidate[..pos];
        }
    }

    pub fn play(&mut self, name: &str, force: bool, reversed: bool, start_frame: usize) {
        let Some(anim) = self.animations.get(name) else {
            return;
        };
        if !force && self.current.as_deref() == Some(name) {
            return;
        }
        let len = anim.frames.len();
        if len == 0 {
            return;
        }
        self.current = Some(name.to_string());
        self.reversed = reversed;
        self.finished = false;
        self.elapsed_s = 0.0;
        self.frame_cursor = if reversed {
            len.saturating_sub(1).saturating_sub(start_frame.min(len - 1))
        } else {
            start_frame.min(len - 1)
        };
        if self.should_use_conductor_sync {
            self.last_song_position_ms = None;
        }
    }

    pub fn update(&mut self, elapsed_s: f32, song_position_ms: Option<f64>) {
        let dt = if self.should_use_conductor_sync {
            let Some(pos_ms) = song_position_ms else {
                return;
            };
            let Some(prev) = self.last_song_position_ms else {
                self.last_song_position_ms = Some(pos_ms);
                return;
            };
            self.last_song_position_ms = Some(pos_ms);
            let delta_ms = pos_ms - prev;
            if delta_ms <= 0.0 {
                return;
            }
            (delta_ms as f32 / 1000.0) * self.time_scale
        } else {
            self.last_song_position_ms = None;
            elapsed_s * self.time_scale
        };

        let Some(name) = self.current.as_deref() else {
            return;
        };
        let Some(anim) = self.animations.get(name) else {
            return;
        };
        if self.finished || dt <= 0.0 || anim.frames.is_empty() || (anim.frames.len() == 1 && anim.looped) {
            return;
        }

        let frame_duration = 1.0 / anim.fps.max(1.0);
        self.elapsed_s += dt;

        while self.elapsed_s >= frame_duration {
            self.elapsed_s -= frame_duration;
            if self.reversed {
                if self.frame_cursor == 0 {
                    if anim.looped {
                        self.frame_cursor = anim.frames.len() - 1;
                    } else {
                        self.finished = true;
                        break;
                    }
                } else {
                    self.frame_cursor -= 1;
                }
            } else if self.frame_cursor + 1 >= anim.frames.len() {
                if anim.looped {
                    self.frame_cursor = 0;
                } else {
                    self.finished = true;
                    break;
                }
            } else {
                self.frame_cursor += 1;
            }
        }
    }

    pub fn current_atlas_index(&self) -> Option<usize> {
        let name = self.current.as_deref()?;
        let anim = self.animations.get(name)?;
        anim.frames.get(self.frame_cursor).copied()
    }

    pub fn current_offset(&self) -> [f32; 2] {
        let Some(name) = self.current.as_deref() else {
            return [0.0, 0.0];
        };
        self.animations
            .get(name)
            .map(|a| a.offset)
            .unwrap_or([0.0, 0.0])
    }

    pub fn current_frame_offset(&self) -> [f32; 2] {
        let Some(name) = self.current.as_deref() else {
            return [0.0, 0.0];
        };
        self.animations
            .get(name)
            .and_then(|a| a.frame_offsets.get(self.frame_cursor).copied())
            .unwrap_or([0.0, 0.0])
    }

    pub fn current_flip(&self) -> [bool; 2] {
        let Some(name) = self.current.as_deref() else {
            return [false, false];
        };
        self.animations
            .get(name)
            .map(|a| a.flip)
            .unwrap_or([false, false])
    }

    pub fn current_frame_rotated(&self) -> bool {
        let Some(name) = self.current.as_deref() else {
            return false;
        };
        self.animations
            .get(name)
            .and_then(|a| a.frame_rotated.get(self.frame_cursor).copied())
            .unwrap_or(false)
    }
}

