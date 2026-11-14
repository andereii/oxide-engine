use crate::utils::constants::DEFAULT_CAMERA_ZOOM;
use crate::utils::easing::EaseKind;
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
        self.camera.follow_x_tween = Some(LinearTween::linear(self.camera.follow_point[0], x, duration_s));
        self.camera.follow_y_tween = Some(LinearTween::linear(self.camera.follow_point[1], y, duration_s));
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
        self.camera.zoom_tween = Some(LinearTween::linear(self.camera.current_zoom, target, duration_s));
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
            let target_lower = event.strumline_target.to_ascii_lowercase();
            let apply_player = target_lower != "opponent";
            let apply_opponent = target_lower != "player";
            match event.kind.as_str() {
                "FocusCamera" => self.tween_camera_to_position(event.target_x, event.target_y, event.duration_s),
                "ZoomCamera" => self.tween_camera_zoom(event.target_zoom, event.duration_s, false),
                "ScrollSpeed" => {
                    if let Some(raw_speed) = event.scroll_speed {
                        let base = if event.absolute { 1.0 } else { self.chart.scroll_speed };
                        let target_speed = raw_speed * base;
                        let start_scroll = event
                            .use_start
                            .then_some(event.start_scroll)
                            .flatten()
                            .map(|s| s * base);
                        let duration_s = event
                            .duration_steps
                            .map(|steps| steps * self.conductor.step_length_ms() / 1000.0)
                            .unwrap_or(event.duration_s);
                        let ease = EaseKind::from_name(&event.ease, &event.ease_dir);
                        self.tween_scroll_speed_eased(
                            target_speed,
                            duration_s,
                            ease,
                            start_scroll,
                            apply_player,
                            apply_opponent,
                        );
                    }
                }
                "RenderDistance" => {
                    for (apply, strum) in [
                        (apply_player, &mut self.player_strumline),
                        (apply_opponent, &mut self.opponent_strumline),
                    ] {
                        if !apply {
                            continue;
                        }
                        if event.use_default_render_distance {
                            strum.use_custom_render_distance = false;
                        } else if let Some(ms) = event.render_distance_ms {
                            strum.use_custom_render_distance = true;
                            strum.custom_render_distance_ms = ms;
                        }
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

