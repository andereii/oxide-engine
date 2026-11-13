use crate::utils::constants::DEFAULT_CAMERA_ZOOM;
use super::scene::PlayState;
use super::types::LinearTween;

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
        self.cancel_camera_follow_tween();
        if duration_s <= 0.0 {
            self.camera.follow_point = [x, y];
            return;
        }
        self.camera.follow_x_tween = Some(LinearTween {
            start: self.camera.follow_point[0],
            target: x,
            elapsed_s: 0.0,
            duration_s,
        });
        self.camera.follow_y_tween = Some(LinearTween {
            start: self.camera.follow_point[1],
            target: y,
            elapsed_s: 0.0,
            duration_s,
        });
    }

    pub fn cancel_camera_follow_tween(&mut self) {
        self.camera.follow_x_tween = None;
        self.camera.follow_y_tween = None;
    }

    pub fn tween_camera_zoom(&mut self, zoom: f64, duration_s: f64, direct: bool) {
        self.cancel_camera_zoom_tween();
        let target = zoom * if direct { DEFAULT_CAMERA_ZOOM } else { self.camera.stage_zoom };
        if duration_s <= 0.0 {
            self.camera.current_zoom = target;
            return;
        }
        self.camera.zoom_tween = Some(LinearTween {
            start: self.camera.current_zoom,
            target,
            elapsed_s: 0.0,
            duration_s,
        });
    }

    pub fn cancel_camera_zoom_tween(&mut self) {
        self.camera.zoom_tween = None;
    }

    pub fn cancel_all_camera_tweens(&mut self) {
        self.cancel_camera_follow_tween();
        self.cancel_camera_zoom_tween();
    }

    pub fn tween_scroll_speed(&mut self, speed: f64, duration_s: f64, apply_player: bool, apply_opponent: bool) {
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
                LinearTween {
                    start: self.player_strumline.scroll_speed,
                    target: speed,
                    elapsed_s: 0.0,
                    duration_s,
                },
            ));
        }
        if apply_opponent {
            self.scroll_speed_tweens.push((
                false,
                LinearTween {
                    start: self.opponent_strumline.scroll_speed,
                    target: speed,
                    elapsed_s: 0.0,
                    duration_s,
                },
            ));
        }
    }

    pub fn cancel_scroll_speed_tweens(&mut self) {
        self.scroll_speed_tweens.clear();
    }

    pub fn process_song_events(&mut self) {
        let pos_ms = self.conductor.song_position_ms;
        let mut due = Vec::new();
        for event in &mut self.song_events {
            if !event.triggered && pos_ms >= event.time_ms {
                event.triggered = true;
                due.push(event.clone());
            }
        }
        for event in due {
            match event.kind.as_str() {
                "FocusCamera" => self.tween_camera_to_position(event.target_x, event.target_y, event.duration_s),
                "ZoomCamera" => self.tween_camera_zoom(event.target_zoom, event.duration_s, false),
                "ScrollSpeed" => {
                    if let Some(speed) = event.scroll_speed {
                        self.tween_scroll_speed(speed, event.duration_s, true, true);
                    }
                }
                _ => {}
            }
        }
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
        if let Some(mut tween) = self.camera.zoom_tween {
            tween.elapsed_s += elapsed_s;
            self.camera.current_zoom = tween.value();
            self.camera.zoom_tween = (!tween.finished()).then_some(tween);
        }

        let decay = (elapsed_s * 5.0).min(1.0);
        self.camera.bop_multiplier += (1.0 - self.camera.bop_multiplier) * decay;
        self.camera.hud_zoom += (self.camera.default_hud_zoom - self.camera.hud_zoom) * decay;

        let mut remaining = Vec::new();
        for (is_player, mut tween) in self.scroll_speed_tweens.drain(..) {
            tween.elapsed_s += elapsed_s;
            let value = tween.value();
            if is_player {
                self.player_strumline.scroll_speed = value;
            } else {
                self.opponent_strumline.scroll_speed = value;
            }
            if !tween.finished() {
                remaining.push((is_player, tween));
            }
        }
        self.scroll_speed_tweens = remaining;
    }
}

