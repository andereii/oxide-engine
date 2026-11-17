use bevy::prelude::{ButtonInput, KeyCode};

use super::controls::{Action, Controls};
use crate::utils::constants::{TURBO_DEFAULT_DELAY, TURBO_DEFAULT_INTERVAL};

#[derive(Debug, Clone)]
pub struct TurboActionHandler {
    pub action: Action,
    pub delay: f64,
    pub interval: f64,
    pub pressed_time: f64,
    pub activated: bool,
}

impl TurboActionHandler {
    pub fn new(action: Action) -> Self {
        Self {
            action,
            delay: TURBO_DEFAULT_DELAY,
            interval: TURBO_DEFAULT_INTERVAL,
            pressed_time: 0.0,
            activated: false,
        }
    }

    pub fn update(&mut self, elapsed_s: f64, controls: &Controls, keys: &ButtonInput<KeyCode>) -> bool {
        self.activated = false;
        if controls.just_pressed(keys, self.action) {
            self.activated = true;
            self.pressed_time = 0.0;
        } else if controls.pressed(keys, self.action) {
            self.pressed_time += elapsed_s;
            if self.pressed_time >= self.delay {
                self.pressed_time -= self.interval;
                self.activated = true;
            }
        } else {
            self.pressed_time = 0.0;
        }
        self.activated
    }
}

#[derive(Debug, Clone)]
pub struct TurboKeyHandler {
    pub keys: Vec<KeyCode>,
    pub delay: f64,
    pub interval: f64,
    pub all_pressed_time: f64,
    pub activated: bool,
}

impl TurboKeyHandler {
    pub fn new(keys: Vec<KeyCode>) -> Self {
        Self {
            keys,
            delay: TURBO_DEFAULT_DELAY,
            interval: TURBO_DEFAULT_INTERVAL,
            all_pressed_time: 0.0,
            activated: false,
        }
    }

    pub fn update(&mut self, elapsed_s: f64, input: &ButtonInput<KeyCode>) -> bool {
        self.activated = false;
        let any_just_pressed = self.keys.iter().any(|k| input.just_pressed(*k));
        let all_pressed = !self.keys.is_empty() && self.keys.iter().all(|k| input.pressed(*k));

        if all_pressed && any_just_pressed {
            self.activated = true;
            self.all_pressed_time = 0.0;
        } else if all_pressed {
            self.all_pressed_time += elapsed_s;
            if self.all_pressed_time >= self.delay {
                self.all_pressed_time -= self.interval;
                self.activated = true;
            }
        } else {
            self.all_pressed_time = 0.0;
        }
        self.activated
    }
}

#[derive(Debug, Clone)]
pub struct TurboButtonHandler {
    pub action: Action,
    pub delay: f64,
    pub interval: f64,
    pub pressed_time: f64,
    pub activated: bool,
}

impl TurboButtonHandler {
    pub fn new(action: Action) -> Self {
        Self {
            action,
            delay: TURBO_DEFAULT_DELAY,
            interval: TURBO_DEFAULT_INTERVAL,
            pressed_time: 0.0,
            activated: false,
        }
    }
}

