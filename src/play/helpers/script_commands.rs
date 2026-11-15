use crate::scripting::{ScriptCommand, TweenProp};
use crate::utils::constants::KEY_COUNT;
use crate::utils::easing::EaseKind;

use super::super::camera::events::parse_raw_song_event;
use super::super::notes::strum::ReceptorState;
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::types::PauseMode;

impl PlayState {
    fn receptor_mut(&mut self, lane: usize) -> Option<&mut ReceptorState> {
        if lane < KEY_COUNT {
            self.player_strumline.receptors.get_mut(lane)
        } else {
            self.opponent_strumline.receptors.get_mut(lane - KEY_COUNT)
        }
    }

    /// Applies state-level script commands and updates prop caches. Returns `true` if fully handled
    /// without needing downstream ECS execution in [`PlayPlugin`](super::PlayPlugin).
    pub fn apply_state_script_command(
        &mut self,
        cmd: &ScriptCommand,
        playlist: &mut PlayStatePlaylist,
    ) -> bool {
        match cmd {
            ScriptCommand::SetStatus(msg) => {
                self.status_message = msg.clone();
                true
            }
            ScriptCommand::SetScoreText(txt) => {
                self.custom_score_text = txt.clone();
                self.last_hud_key = (i64::MIN, u32::MAX, u32::MAX, i32::MIN);
                self.update_score_text();
                true
            }
            ScriptCommand::SetHudVisible(vis) => {
                self.hud_visible = *vis;
                true
            }
            ScriptCommand::TweenCameraZoom { target, duration_s, ease } => {
                let parsed = EaseKind::from_name(ease, "");
                self.tween_camera_zoom_eased(*target, *duration_s, true, parsed);
                true
            }
            ScriptCommand::TweenCameraPos { x, y, duration_s, ease } => {
                let parsed = EaseKind::from_name(ease, "").unwrap_or_default();
                self.tween_camera_to_position_eased(*x, *y, *duration_s, parsed);
                true
            }
            ScriptCommand::TweenScrollSpeed { target, duration_s, ease } => {
                let parsed = EaseKind::from_name(ease, "");
                self.tween_scroll_speed_eased(*target, *duration_s, parsed, None, true, true);
                true
            }
            ScriptCommand::FocusCameraTarget(target) => {
                let idx = match target.trim().to_ascii_lowercase().as_str() {
                    "bf" | "player" | "0" => Some(0),
                    "dad" | "opponent" | "1" => Some(1),
                    "gf" | "spectator" | "2" => Some(2),
                    _ => None,
                };
                if let Some(i) = idx
                    && let Some(pt) = self.camera.focus_points[i]
                {
                    self.cancel_camera_follow_tween();
                    self.camera.follow_target = Some(pt);
                }
                true
            }
            ScriptCommand::CameraBop { game_delta, hud_delta } => {
                self.camera.bop_multiplier += *game_delta;
                self.camera.hud_zoom += *hud_delta * self.camera.default_hud_zoom;
                true
            }
            ScriptCommand::SetCameraBopConfig { intensity, rate } => {
                self.camera.bop_intensity = *intensity;
                self.camera.zoom_rate = *rate;
                true
            }
            ScriptCommand::CameraShake { intensity, duration_s, hud } => {
                if *hud {
                    self.camera.hud_shake_intensity = *intensity;
                    self.camera.hud_shake_timer_s = (*duration_s).max(0.0);
                    if *duration_s <= 0.0 {
                        self.camera.hud_shake_offset = [0.0, 0.0];
                    }
                } else {
                    self.camera.shake_intensity = *intensity;
                    self.camera.shake_timer_s = (*duration_s).max(0.0);
                    if *duration_s <= 0.0 {
                        self.camera.shake_offset = [0.0, 0.0];
                    }
                }
                true
            }
            ScriptCommand::CameraFlash { color, duration_s, hud } => {
                let dur = (*duration_s).max(0.0);
                if *hud {
                    self.camera.hud_flash_color = *color;
                    self.camera.hud_flash_duration_s = dur;
                    self.camera.hud_flash_timer_s = dur;
                } else {
                    self.camera.flash_color = *color;
                    self.camera.flash_duration_s = dur;
                    self.camera.flash_timer_s = dur;
                }
                true
            }
            ScriptCommand::SetLaneOffset { lane, x, y } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_offset = [*x, *y];
                }
                true
            }
            ScriptCommand::SetLaneAngle { lane, deg } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_angle_deg = *deg;
                }
                true
            }
            ScriptCommand::SetLaneScale { lane, sx, sy } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_scale = [*sx, *sy];
                }
                true
            }
            ScriptCommand::SetLaneAlpha { lane, alpha } => {
                if let Some(rec) = self.receptor_mut(*lane) {
                    rec.mod_alpha = alpha.clamp(0.0, 1.0);
                }
                true
            }
            ScriptCommand::ResetLanes => {
                for rec in &mut self.player_strumline.receptors {
                    rec.reset_modchart();
                }
                for rec in &mut self.opponent_strumline.receptors {
                    rec.reset_modchart();
                }
                true
            }
            ScriptCommand::SetAudioVolumes { inst, vocals, player_vocal, opponent_vocal } => {
                if let Some(v) = inst {
                    self.instrumental_volume = f64::from(*v).max(0.0);
                }
                if let Some(v) = vocals {
                    let f = f64::from(*v).max(0.0);
                    self.player_vocals_volume = f;
                    self.opponent_vocals_volume = f;
                }
                if let Some(v) = player_vocal {
                    self.player_vocals_volume = f64::from(*v).max(0.0);
                }
                if let Some(v) = opponent_vocal {
                    self.opponent_vocals_volume = f64::from(*v).max(0.0);
                }
                true
            }
            ScriptCommand::AddNote { time_ms, dir, length_ms, kind } => {
                let kind_opt = (!kind.is_empty()).then(|| kind.clone());
                self.add_dynamic_note(*time_ms, *dir, *length_ms, kind_opt);
                true
            }
            ScriptCommand::TriggerSongEvent { name, value } => {
                let ev = parse_raw_song_event(
                    self.conductor.song_position_ms,
                    name.clone(),
                    value.clone(),
                );
                self.song_events.push(ev);
                true
            }
            ScriptCommand::EndSong => {
                self.end_song(playlist);
                true
            }
            ScriptCommand::RestartSong => {
                self.needs_reset = true;
                true
            }
            ScriptCommand::PauseSong => {
                self.pause(PauseMode::Standard);
                true
            }
            ScriptCommand::SpawnProp { id, pos, color, .. } => {
                self.script_prop_positions.insert(id.clone(), *pos);
                self.script_prop_scales.insert(id.clone(), [1.0, 1.0]);
                self.script_prop_angles.insert(id.clone(), 0.0);
                self.script_prop_alphas.insert(id.clone(), color[3]);
                false
            }
            ScriptCommand::DespawnProp(id) => {
                self.script_prop_positions.remove(id);
                self.script_prop_scales.remove(id);
                self.script_prop_angles.remove(id);
                self.script_prop_alphas.remove(id);
                false
            }
            ScriptCommand::SetPropNumber { id, prop, value } => {
                match prop {
                    TweenProp::X => {
                        let y = self.script_prop_positions.get(id).map_or(0.0, |p| p[1]);
                        self.script_prop_positions.insert(id.clone(), [*value, y]);
                    }
                    TweenProp::Y => {
                        let x = self.script_prop_positions.get(id).map_or(0.0, |p| p[0]);
                        self.script_prop_positions.insert(id.clone(), [x, *value]);
                    }
                    TweenProp::ScaleX => {
                        let sy = self.script_prop_scales.get(id).map_or(1.0, |s| s[1]);
                        self.script_prop_scales.insert(id.clone(), [*value, sy]);
                    }
                    TweenProp::ScaleY => {
                        let sx = self.script_prop_scales.get(id).map_or(1.0, |s| s[0]);
                        self.script_prop_scales.insert(id.clone(), [sx, *value]);
                    }
                    TweenProp::Angle => {
                        self.script_prop_angles.insert(id.clone(), *value);
                    }
                    TweenProp::Alpha => {
                        self.script_prop_alphas.insert(id.clone(), value.clamp(0.0, 1.0));
                    }
                    _ => {}
                }
                false
            }
            ScriptCommand::SetPropTint { id, color } => {
                self.script_prop_alphas.insert(id.clone(), color[3].clamp(0.0, 1.0));
                false
            }
            ScriptCommand::SetCharacterPos { target, x, y } => {
                self.script_char_positions.insert(target.clone(), [*x, *y]);
                false
            }
            ScriptCommand::Log { .. }
            | ScriptCommand::PlayAnim { .. }
            | ScriptCommand::SetCharacterScale { .. }
            | ScriptCommand::SetCharacterTint { .. }
            | ScriptCommand::SetCharacterVisible { .. }
            | ScriptCommand::SetCharacterDanceEvery { .. }
            | ScriptCommand::SetPropVisible { .. }
            | ScriptCommand::PlaySfx { .. }
            | ScriptCommand::SetShaderAdjustColor { .. }
            | ScriptCommand::SetShaderDropShadow { .. }
            | ScriptCommand::SetShaderWiggle { .. }
            | ScriptCommand::SetCameraRain { .. }
            | ScriptCommand::ClearShader { .. } => false,
        }
    }
}
