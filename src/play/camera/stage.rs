use crate::play::scene::PlayState;
use crate::play::types::LinearTween;
use crate::utils::constants::{CAMERA_DECAY_RATE, DEFAULT_CAMERA_ZOOM};
use crate::utils::easing::EaseKind;

impl PlayState {
    pub fn reset_camera(&mut self, reset_zoom: bool, cancel_tweens: bool, snap: bool) {
        if cancel_tweens {
            self.cancel_all_camera_tweens();
        }
        if reset_zoom {
            self.camera.current_zoom = self.camera.stage_zoom;
            self.camera.hud_zoom = self.camera.default_hud_zoom;
        }
        if snap {
            self.camera.bop_multiplier = 1.0;
        }
    }

    pub fn tween_camera_to_position(&mut self, x: f64, y: f64, duration_s: f64) {
        self.tween_camera_to_position_eased(x, y, duration_s, EaseKind::Linear);
    }

    pub fn tween_camera_to_position_eased(&mut self, x: f64, y: f64, duration_s: f64, ease: EaseKind) {
        self.cancel_camera_follow_tween();
        self.camera.follow_target = None;
        if duration_s <= 0.0 {
            self.camera.follow_point = [x, y];
            return;
        }
        self.camera.follow_x_tween = Some(LinearTween::eased(self.camera.follow_point[0], x, duration_s, ease));
        self.camera.follow_y_tween = Some(LinearTween::eased(self.camera.follow_point[1], y, duration_s, ease));
    }

    pub fn cancel_camera_follow_tween(&mut self) {
        self.camera.follow_x_tween = None;
        self.camera.follow_y_tween = None;
    }

    pub fn tween_camera_zoom(&mut self, zoom: f64, duration_s: f64, direct: bool) {
        self.tween_camera_zoom_eased(zoom, duration_s, direct, Some(EaseKind::Linear));
    }

    pub fn tween_camera_zoom_eased(
        &mut self,
        zoom: f64,
        duration_s: f64,
        direct: bool,
        ease: Option<EaseKind>,
    ) {
        let target = zoom * if direct { DEFAULT_CAMERA_ZOOM } else { self.camera.stage_zoom };
        if !target.is_finite() || !duration_s.is_finite() || duration_s < 0.0 {
            return;
        }
        let Some(ease_kind) = ease else {
            self.cancel_camera_zoom_tween();
            self.camera.current_zoom = target;
            return;
        };
        self.cancel_camera_zoom_tween();
        if duration_s <= 0.0 {
            self.camera.current_zoom = target;
            return;
        }
        self.camera.zoom_tween = Some(LinearTween::eased(self.camera.current_zoom, target, duration_s, ease_kind));
    }

    pub fn cancel_camera_zoom_tween(&mut self) {
        self.camera.zoom_tween = None;
    }

    pub fn cancel_all_camera_tweens(&mut self) {
        self.cancel_camera_follow_tween();
        self.cancel_camera_zoom_tween();
    }

    pub fn tween_scroll_speed(&mut self, speed: f64, duration_s: f64, apply_player: bool, apply_opponent: bool) {
        self.tween_scroll_speed_eased(speed, duration_s, Some(EaseKind::Linear), None, apply_player, apply_opponent);
    }

    pub fn tween_scroll_speed_eased(
        &mut self,
        speed: f64,
        duration_s: f64,
        ease: Option<EaseKind>,
        start_scroll: Option<f64>,
        apply_player: bool,
        apply_opponent: bool,
    ) {
        self.scroll_speed_tweens
            .retain(|(is_p, _)| !((*is_p && apply_player) || (!*is_p && apply_opponent)));

        if let Some(start) = start_scroll {
            if apply_player {
                self.player_strumline.scroll_speed = start;
            }
            if apply_opponent {
                self.opponent_strumline.scroll_speed = start;
            }
        }

        let Some(ease_kind) = ease else {
            if apply_player {
                self.player_strumline.scroll_speed = speed;
            }
            if apply_opponent {
                self.opponent_strumline.scroll_speed = speed;
            }
            return;
        };

        if duration_s <= 0.0 {
            if apply_player {
                self.player_strumline.scroll_speed = speed;
            }
            if apply_opponent {
                self.opponent_strumline.scroll_speed = speed;
            }
            return;
        }
        if apply_player {
            self.scroll_speed_tweens.push((
                true,
                LinearTween::eased(self.player_strumline.scroll_speed, speed, duration_s, ease_kind),
            ));
        }
        if apply_opponent {
            self.scroll_speed_tweens.push((
                false,
                LinearTween::eased(self.opponent_strumline.scroll_speed, speed, duration_s, ease_kind),
            ));
        }
    }

    pub fn cancel_scroll_speed_tweens(&mut self) {
        self.scroll_speed_tweens.clear();
    }

    pub fn advance_tweens(&mut self, elapsed_s: f64) {
        if let Some(mut tween) = self.camera.follow_x_tween {
            tween.elapsed_s += elapsed_s;
            self.camera.follow_point[0] = tween.value();
            self.camera.follow_x_tween = (!tween.finished()).then_some(tween);
        }
        if let Some(mut tween) = self.camera.follow_y_tween {
            tween.elapsed_s += elapsed_s;
            self.camera.follow_point[1] = tween.value();
            self.camera.follow_y_tween = (!tween.finished()).then_some(tween);
        }
        if self.camera.follow_x_tween.is_none()
            && self.camera.follow_y_tween.is_none()
            && let Some(target) = self.camera.follow_target
        {
            let lerp = 1.0 - 0.96_f64.powf(elapsed_s * 60.0);
            self.camera.follow_point[0] += (target[0] - self.camera.follow_point[0]) * lerp;
            self.camera.follow_point[1] += (target[1] - self.camera.follow_point[1]) * lerp;
        }

        if let Some(mut tween) = self.camera.zoom_tween {
            tween.elapsed_s += elapsed_s;
            self.camera.current_zoom = tween.value();
            self.camera.zoom_tween = (!tween.finished()).then_some(tween);
        }

        let decay_factor = CAMERA_DECAY_RATE.powf(elapsed_s * 60.0);
        if self.camera.zoom_rate > 0.0 {
            self.camera.bop_multiplier =
                1.0 + (self.camera.bop_multiplier - 1.0) * decay_factor;
        }
        self.camera.hud_zoom = self.camera.default_hud_zoom
            + (self.camera.hud_zoom - self.camera.default_hud_zoom) * decay_factor;

        if !self.scroll_speed_tweens.is_empty() {
            let mut idx = 0;
            while idx < self.scroll_speed_tweens.len() {
                let (is_player, mut tween) = self.scroll_speed_tweens[idx];
                tween.elapsed_s += elapsed_s;
                let value = tween.value();
                if is_player {
                    self.player_strumline.scroll_speed = value;
                } else {
                    self.opponent_strumline.scroll_speed = value;
                }
                if tween.finished() {
                    self.scroll_speed_tweens.swap_remove(idx);
                } else {
                    self.scroll_speed_tweens[idx].1 = tween;
                    idx += 1;
                }
            }
        }
    }
}

