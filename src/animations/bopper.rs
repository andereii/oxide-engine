use bevy::prelude::Component;

use super::controller::AnimationController;

/// Stage prop or character that dances on beat intervals (ported from Funkin `Bopper.hx`).
#[derive(Component, Debug, Clone)]
#[allow(dead_code)]
pub struct Bopper {
    pub name: String,
    pub dance_every: f32,
    pub should_alternate: Option<bool>,
    pub idle_suffix: String,
    pub should_bop: bool,
    pub has_danced: bool,
    pub can_play_other_anims: bool,
    pub original_position: [f32; 2],
    pub global_offsets: [f32; 2],
}

impl Default for Bopper {
    fn default() -> Self {
        Self {
            name: String::new(),
            dance_every: 0.0,
            should_alternate: None,
            idle_suffix: String::new(),
            should_bop: true,
            has_danced: false,
            can_play_other_anims: true,
            original_position: [0.0, 0.0],
            global_offsets: [0.0, 0.0],
        }
    }
}

impl Bopper {
    pub fn new(dance_every: f32) -> Self {
        Self {
            dance_every,
            ..Default::default()
        }
    }

    pub fn on_step_hit(&mut self, step: i32, controller: &mut AnimationController) {
        if self.dance_every <= 0.0 {
            return;
        }
        let period = (self.dance_every * 4.0).round() as i32;
        if period > 0 && step.rem_euclid(period) == 0 {
            let restart = self.should_bop;
            self.dance(controller, restart);
        }
    }

    pub fn dance(&mut self, controller: &mut AnimationController, force_restart: bool) {
        if self.should_alternate.is_none() {
            self.should_alternate = Some(controller.has_animation("danceLeft"));
        }

        if self.should_alternate == Some(true) {
            let target = if self.has_danced {
                format!("danceRight{}", self.idle_suffix)
            } else {
                format!("danceLeft{}", self.idle_suffix)
            };
            self.play_animation(controller, &target, force_restart, false);
            self.has_danced = !self.has_danced;
        } else {
            let target = format!("idle{}", self.idle_suffix);
            self.play_animation(controller, &target, force_restart, false);
        }
    }

    pub fn play_animation(
        &mut self,
        controller: &mut AnimationController,
        name: &str,
        restart: bool,
        ignore_other: bool,
    ) {
        if !self.can_play_other_anims {
            let same_anim = controller.current_name() == Some(name);
            if !(same_anim && restart) {
                return;
            }
        }

        let Some(resolved) = controller.resolve_name(name) else {
            return;
        };
        controller.play(&resolved, restart, false, 0);
        if ignore_other {
            self.can_play_other_anims = false;
        }
    }

    pub fn sync_finished(&mut self, controller: &AnimationController) {
        if !self.can_play_other_anims && controller.is_finished() {
            self.can_play_other_anims = true;
        }
    }
}
