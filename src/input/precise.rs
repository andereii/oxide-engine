use std::collections::HashMap;
use std::time::Instant;

use bevy::prelude::{ButtonInput, KeyCode, Resource};

use super::controls::{Action, Controls};
use crate::play::notes::NoteDirection;
use crate::utils::constants::NS_PER_MS;

#[derive(Debug, Clone, Copy)]
pub struct PreciseInputEvent {
    pub direction: NoteDirection,
    pub key_code: KeyCode,
    pub timestamp_ns: u128,
    pub song_position_ms: f64,
}

#[derive(Resource, Debug)]
pub struct PreciseInputManager {
    epoch: Instant,
    key_list: Vec<KeyCode>,
    key_dir_map: HashMap<KeyCode, NoteDirection>,
    dir_press_timestamps_ns: [Option<u128>; 4],
    dir_release_timestamps_ns: [Option<u128>; 4],
    pending_presses: Vec<PreciseInputEvent>,
    pending_releases: Vec<PreciseInputEvent>,
}

impl Default for PreciseInputManager {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            key_list: Vec::new(),
            key_dir_map: HashMap::new(),
            dir_press_timestamps_ns: [None; 4],
            dir_release_timestamps_ns: [None; 4],
            pending_presses: Vec::new(),
            pending_releases: Vec::new(),
        }
    }
}

impl PreciseInputManager {
    pub fn current_timestamp_ns(&self) -> u128 {
        self.epoch.elapsed().as_nanos()
    }

    pub fn initialize_keys(&mut self, controls: &Controls) {
        self.key_list.clear();
        self.key_dir_map.clear();

        let mappings = [
            (NoteDirection::Left, Action::NoteLeft),
            (NoteDirection::Down, Action::NoteDown),
            (NoteDirection::Up, Action::NoteUp),
            (NoteDirection::Right, Action::NoteRight),
        ];

        for (dir, action) in mappings {
            for &key in controls.keys_for(action) {
                if !self.key_dir_map.contains_key(&key) {
                    self.key_list.push(key);
                    self.key_dir_map.insert(key, dir);
                }
            }
        }
    }

    pub fn poll_keyboard(&mut self, keys: &ButtonInput<KeyCode>, song_position_ms: f64) {
        let now_ns = self.current_timestamp_ns();
        for &key in &self.key_list {
            let Some(&direction) = self.key_dir_map.get(&key) else {
                continue;
            };
            if keys.just_pressed(key) {
                self.dir_press_timestamps_ns[direction.index()] = Some(now_ns);
                self.pending_presses.push(PreciseInputEvent {
                    direction,
                    key_code: key,
                    timestamp_ns: now_ns,
                    song_position_ms,
                });
            }
            if keys.just_released(key) {
                self.dir_release_timestamps_ns[direction.index()] = Some(now_ns);
                self.pending_releases.push(PreciseInputEvent {
                    direction,
                    key_code: key,
                    timestamp_ns: now_ns,
                    song_position_ms,
                });
            }
        }
    }

    pub fn drain_presses(&mut self) -> Vec<PreciseInputEvent> {
        std::mem::take(&mut self.pending_presses)
    }

    pub fn drain_releases(&mut self) -> Vec<PreciseInputEvent> {
        std::mem::take(&mut self.pending_releases)
    }

    pub fn latency_ms(&self, event: &PreciseInputEvent) -> f64 {
        let delta_ns = self.current_timestamp_ns().saturating_sub(event.timestamp_ns);
        (delta_ns as f64) / NS_PER_MS
    }

    pub fn time_since_pressed_ns(&self, dir: NoteDirection) -> u128 {
        match self.dir_press_timestamps_ns[dir.index()] {
            Some(ts) => self.current_timestamp_ns().saturating_sub(ts),
            None => 0,
        }
    }

    pub fn time_since_released_ns(&self, dir: NoteDirection) -> u128 {
        match self.dir_release_timestamps_ns[dir.index()] {
            Some(ts) => self.current_timestamp_ns().saturating_sub(ts),
            None => 0,
        }
    }
}

