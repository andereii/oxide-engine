use crate::utils::constants::CONFIRM_HOLD_TIME_S;
use crate::utils::easing::EaseKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReceptorAnimState {
    #[default]
    Static,
    Press,
    Confirm,
    ConfirmHold,
}

impl ReceptorAnimState {
    pub fn as_anim_name(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Press => "press",
            Self::Confirm => "confirm",
            Self::ConfirmHold => "confirm-hold",
        }
    }

    pub fn is_confirm(self) -> bool {
        matches!(self, Self::Confirm | Self::ConfirmHold)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NoteVibrationStatus {
    #[default]
    Idle,
    Pressed,
    Confirm,
    HoldConfirm,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoteVibrationsState {
    pub statuses: [NoteVibrationStatus; 4],
}

#[derive(Debug, Clone, Copy)]
pub struct ArrowFadeTween {
    pub delay_s: f64,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub start_y: f32,
    pub end_y: f32,
    pub start_alpha: f32,
    pub end_alpha: f32,
    pub ease: EaseKind,
}

#[derive(Debug, Clone)]
pub struct ReceptorState {
    pub is_player: bool,
    pub anim_state: ReceptorAnimState,
    pub confirm_hold_timer_s: f64,
    pub confirm_finished: bool,
    pub anim_gen: u64,
    pub y_offset: f32,
    pub alpha: f32,
    pub fade_tween: Option<ArrowFadeTween>,
}

impl ReceptorState {
    pub fn new(is_player: bool) -> Self {
        Self {
            is_player,
            anim_state: ReceptorAnimState::Static,
            confirm_hold_timer_s: -1.0,
            confirm_finished: false,
            anim_gen: 1,
            y_offset: 0.0,
            alpha: 1.0,
            fade_tween: None,
        }
    }

    pub fn play_static(&mut self) {
        if self.anim_state != ReceptorAnimState::Static {
            self.anim_gen = self.anim_gen.wrapping_add(1);
        }
        self.anim_state = ReceptorAnimState::Static;
        self.confirm_hold_timer_s = -1.0;
        self.confirm_finished = false;
    }

    pub fn play_press(&mut self) {
        if self.anim_state != ReceptorAnimState::Press {
            self.anim_gen = self.anim_gen.wrapping_add(1);
        }
        self.anim_state = ReceptorAnimState::Press;
        self.confirm_hold_timer_s = -1.0;
        self.confirm_finished = false;
    }

    pub fn play_confirm(&mut self) {
        self.anim_state = ReceptorAnimState::Confirm;
        self.confirm_finished = false;
        self.confirm_hold_timer_s = if self.is_player { -1.0 } else { 0.0 };
        self.anim_gen = self.anim_gen.wrapping_add(1);
    }

    pub fn hold_confirm(&mut self) {
        if self.anim_state == ReceptorAnimState::ConfirmHold {
            return;
        }
        if self.anim_state == ReceptorAnimState::Confirm {
            if self.confirm_finished {
                self.confirm_hold_timer_s = -1.0;
                self.anim_state = ReceptorAnimState::ConfirmHold;
                self.anim_gen = self.anim_gen.wrapping_add(1);
            }
        } else {
            self.anim_state = ReceptorAnimState::Confirm;
            self.confirm_finished = false;
            self.anim_gen = self.anim_gen.wrapping_add(1);
        }
    }

    pub fn on_confirm_anim_finished(&mut self) {
        self.confirm_finished = true;
        if self.is_player && self.anim_state == ReceptorAnimState::Confirm && self.confirm_hold_timer_s < 0.0 {
            self.confirm_hold_timer_s = 0.0;
        }
    }

    pub fn start_fade_in(&mut self, index: usize) {
        self.y_offset = -10.0;
        self.alpha = 0.0;
        self.fade_tween = Some(ArrowFadeTween {
            delay_s: 0.5 + 0.2 * index as f64,
            elapsed_s: 0.0,
            duration_s: 1.0,
            start_y: -10.0,
            end_y: 0.0,
            start_alpha: 0.0,
            end_alpha: 1.0,
            ease: EaseKind::CircOut,
        });
    }

    pub fn start_fade_out(&mut self) {
        self.fade_tween = Some(ArrowFadeTween {
            delay_s: 0.0,
            elapsed_s: 0.0,
            duration_s: 0.5,
            start_y: self.y_offset,
            end_y: self.y_offset - 10.0,
            start_alpha: self.alpha,
            end_alpha: 0.0,
            ease: EaseKind::CircIn,
        });
    }

    pub fn tick(&mut self, elapsed_s: f64, is_held: bool) {
        if self.confirm_hold_timer_s >= 0.0 {
            self.confirm_hold_timer_s += elapsed_s;
            if self.confirm_hold_timer_s >= CONFIRM_HOLD_TIME_S {
                self.confirm_hold_timer_s = -1.0;
                if is_held {
                    self.play_press();
                } else {
                    self.play_static();
                }
            }
        } else if is_held && self.anim_state == ReceptorAnimState::Static {
            self.play_press();
        }

        if let Some(mut tw) = self.fade_tween {
            if tw.delay_s > 0.0 {
                tw.delay_s = (tw.delay_s - elapsed_s).max(0.0);
                self.fade_tween = Some(tw);
            } else {
                tw.elapsed_s += elapsed_s;
                let t = if tw.duration_s <= 0.0 {
                    1.0
                } else {
                    tw.ease.eval((tw.elapsed_s / tw.duration_s).clamp(0.0, 1.0))
                };
                self.y_offset = tw.start_y + (tw.end_y - tw.start_y) * t as f32;
                self.alpha = tw.start_alpha + (tw.end_alpha - tw.start_alpha) * t as f32;
                self.fade_tween = if tw.elapsed_s >= tw.duration_s {
                    None
                } else {
                    Some(tw)
                };
            }
        }
    }
}

