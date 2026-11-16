use crate::play::hud::judgement::Judgement;
use crate::play::hud::popups::PopupEvent;
use crate::scripting::{ScriptCommand, TweenProp};
use crate::utils::constants::KEY_COUNT;
use crate::utils::easing::EaseKind;

use super::super::camera::events::parse_raw_song_event;
use super::super::notes::data::NoteDirection;
use super::super::notes::strum::{ReceptorState, Strumline};
use super::super::playlist::PlayStatePlaylist;
use super::super::scene::PlayState;
use super::super::types::PauseMode;

fn parse_judgement(raw: &str) -> Option<Judgement> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "sick" => Some(Judgement::Sick),
        "good" => Some(Judgement::Good),
        "bad" => Some(Judgement::Bad),
        "shit" => Some(Judgement::Shit),
        "miss" => Some(Judgement::Miss),
        _ => None,
    }
}

fn mutate_strumline_note(
    strum: &mut Strumline,
    id: usize,
    time_ms: Option<f64>,
    dir: Option<usize>,
    length_ms: Option<f64>,
    kind: Option<&str>,
    scoreable: Option<bool>,
) -> bool {
    let Some(note) = strum.notes.iter_mut().find(|n| n.data.id == id) else {
        return false;
    };
    if let Some(t) = time_ms {
        note.data.time_ms = t;
    }
    if let Some(d) = dir
        && let Some(nd) = NoteDirection::from_index(d % KEY_COUNT)
    {
        note.data.direction = d % KEY_COUNT;
        note.lane = nd;
    }
    if let Some(len) = length_ms {
        let clamped = len.max(0.0);
        note.data.sustain_length_ms = clamped;
        note.hold_remaining_ms = note.hold_remaining_ms.min(clamped);
    }
    if let Some(k) = kind {
        note.data.kind = (!k.trim().is_empty()).then(|| k.to_string());
    }
    if let Some(sc) = scoreable {
        note.data.scoreable = sc;
    }
    if time_ms.is_some() {
        strum.notes.sort_by(|a, b| a.data.time_ms.total_cmp(&b.data.time_ms));
        strum.active_start_index = 0;
        strum.next_note_index = 0;
    }
    true
}

impl PlayState {
    fn receptor_mut(&mut self, lane: usize) -> Option<&mut ReceptorState> {
        if lane < KEY_COUNT {
            self.player_strumline.receptors.get_mut(lane)
        } else {
            self.opponent_strumline.receptors.get_mut(lane - KEY_COUNT)
        }
    }

    /// Applies state-level script commands and updates caches. Returns `true` if fully handled
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
            ScriptCommand::ResetCamera { zoom, tweens, snap } => {
                self.reset_camera(*zoom, *tweens, *snap);
                true
            }
            ScriptCommand::CancelCameraTweens => {
                self.cancel_all_camera_tweens();
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
                    self.camera.hud_flash_fade_in = false;
                } else {
                    self.camera.flash_color = *color;
                    self.camera.flash_duration_s = dur;
                    self.camera.flash_timer_s = dur;
                    self.camera.flash_fade_in = false;
                }
                true
            }
            ScriptCommand::CameraFade { color, duration_s, fade_in, hud } => {
                let dur = (*duration_s).max(0.0);
                if *hud {
                    self.camera.hud_flash_color = *color;
                    self.camera.hud_flash_duration_s = dur;
                    self.camera.hud_flash_timer_s = dur;
                    self.camera.hud_flash_fade_in = *fade_in;
                } else {
                    self.camera.flash_color = *color;
                    self.camera.flash_duration_s = dur;
                    self.camera.flash_timer_s = dur;
                    self.camera.flash_fade_in = *fade_in;
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
            ScriptCommand::KillNote { id } => {
                if let Some(idx) = self.player_strumline.notes.iter().position(|n| n.data.id == *id) {
                    self.player_strumline.kill_note(idx);
                } else if let Some(idx) = self.opponent_strumline.notes.iter().position(|n| n.data.id == *id) {
                    self.opponent_strumline.kill_note(idx);
                }
                true
            }
            ScriptCommand::ModifyNote { id, time_ms, dir, length_ms, kind, scoreable } => {
                if !mutate_strumline_note(
                    &mut self.player_strumline,
                    *id,
                    *time_ms,
                    *dir,
                    *length_ms,
                    kind.as_deref(),
                    *scoreable,
                ) {
                    mutate_strumline_note(
                        &mut self.opponent_strumline,
                        *id,
                        *time_ms,
                        *dir,
                        *length_ms,
                        kind.as_deref(),
                        *scoreable,
                    );
                }
                true
            }
            ScriptCommand::StrumAnim { lane, state } => {
                let is_player = *lane < KEY_COUNT;
                let dir = NoteDirection::from_int((*lane % KEY_COUNT) as i32);
                let strum = if is_player {
                    &mut self.player_strumline
                } else {
                    &mut self.opponent_strumline
                };
                match state.trim().to_ascii_lowercase().as_str() {
                    "press" | "pressed" => strum.play_press(dir),
                    "confirm" => strum.play_confirm(dir),
                    "hold" | "confirm-hold" | "confirm_hold" => strum.hold_confirm(dir),
                    _ => strum.play_static(dir),
                }
                true
            }
            ScriptCommand::SpawnSplash { lane } => {
                self.player_strumline
                    .pending_splashes
                    .push(NoteDirection::from_int((*lane % KEY_COUNT) as i32));
                true
            }
            ScriptCommand::SetNotesplashVisible(vis) => {
                self.player_strumline.show_notesplash = *vis;
                self.opponent_strumline.show_notesplash = *vis;
                true
            }
            ScriptCommand::SetStrumStyle { side, scale, spacing } => {
                let lower = side.trim().to_ascii_lowercase();
                if lower != "opponent" && lower != "dad" {
                    self.player_strumline.strumline_scale = (*scale).max(0.05);
                    self.player_strumline.note_spacing_scale = (*spacing).max(0.05);
                }
                if lower != "player" && lower != "bf" {
                    self.opponent_strumline.strumline_scale = (*scale).max(0.05);
                    self.opponent_strumline.note_spacing_scale = (*spacing).max(0.05);
                }
                true
            }
            ScriptCommand::VwooshNotes { side, fade_in } => {
                let lower = side.trim().to_ascii_lowercase();
                if lower != "opponent" && lower != "dad" {
                    if *fade_in {
                        self.player_strumline.vwoosh_in_notes();
                    } else {
                        self.player_strumline.vwoosh_notes();
                    }
                }
                if lower != "player" && lower != "bf" {
                    if *fade_in {
                        self.opponent_strumline.vwoosh_in_notes();
                    } else {
                        self.opponent_strumline.vwoosh_notes();
                    }
                }
                true
            }
            ScriptCommand::SpawnPopup { judgement, combo } => {
                self.pending_popups.push(PopupEvent {
                    judgement: parse_judgement(judgement),
                    combo: *combo,
                });
                true
            }
            ScriptCommand::SetPopupsConfig { visible, offset } => {
                if let Some(vis) = visible {
                    self.popups_visible = *vis;
                }
                if let Some(off) = offset {
                    self.popup_offset = *off;
                }
                true
            }
            ScriptCommand::ControlCountdown { action } => {
                match action.trim().to_ascii_lowercase().as_str() {
                    "start" | "restart" => self.start_countdown(),
                    "pause" => self.countdown_paused = true,
                    "resume" => self.countdown_paused = false,
                    "skip" | "end" => {
                        self.countdown_paused = false;
                        self.start_song();
                    }
                    _ => {}
                }
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
            ScriptCommand::ResumeSong => {
                self.resume();
                true
            }
            ScriptCommand::SetPlaylist { story_mode, songs, score } => {
                if let Some(sm) = story_mode {
                    playlist.is_story_mode = *sm;
                }
                if let Some(list) = songs {
                    playlist.playlist_song_ids = list.clone();
                }
                if let Some(sc) = score {
                    playlist.campaign_score = *sc;
                }
                true
            }
            ScriptCommand::SpawnProp { id, pos, color, z, hud, .. }
            | ScriptCommand::SpawnAnimatedProp { id, pos, color, z, hud, .. } => {
                self.script_prop_positions.insert(id.clone(), *pos);
                self.script_prop_scales.insert(id.clone(), [1.0, 1.0]);
                self.script_prop_angles.insert(id.clone(), 0.0);
                self.script_prop_alphas.insert(id.clone(), color[3]);
                self.script_prop_z.insert(id.clone(), *z);
                self.script_prop_scrolls.insert(id.clone(), if *hud { [0.0, 0.0] } else { [1.0, 1.0] });
                self.script_prop_flips.insert(id.clone(), [false, false]);
                self.script_prop_vis.insert(id.clone(), true);
                false
            }
            ScriptCommand::DespawnProp(id) => {
                self.script_prop_positions.remove(id);
                self.script_prop_scales.remove(id);
                self.script_prop_angles.remove(id);
                self.script_prop_alphas.remove(id);
                self.script_prop_z.remove(id);
                self.script_prop_scrolls.remove(id);
                self.script_prop_flips.remove(id);
                self.script_prop_vis.remove(id);
                self.script_prop_sizes.remove(id);
                self.script_prop_velocities.remove(id);
                self.script_prop_anims.remove(id);
                self.script_prop_anim_done.remove(id);
                false
            }
            ScriptCommand::SetPropNumber { id, prop, value } => {
                match prop {
                    TweenProp::X => {
                        let y = self.script_prop_positions.get(id).map_or(0.0, |p| p[1]);
                        self.script_prop_positions.insert(id.clone(), [*value, y]);
                        if self.script_texts.contains_key(id) {
                            let ty = self.script_text_positions.get(id).map_or(0.0, |p| p[1]);
                            self.script_text_positions.insert(id.clone(), [*value, ty]);
                        }
                    }
                    TweenProp::Y => {
                        let x = self.script_prop_positions.get(id).map_or(0.0, |p| p[0]);
                        self.script_prop_positions.insert(id.clone(), [x, *value]);
                        if self.script_texts.contains_key(id) {
                            let tx = self.script_text_positions.get(id).map_or(0.0, |p| p[0]);
                            self.script_text_positions.insert(id.clone(), [tx, *value]);
                        }
                    }
                    TweenProp::Z => {
                        self.script_prop_z.insert(id.clone(), *value);
                    }
                    TweenProp::VelX => {
                        let vy = self.script_prop_velocities.get(id).map_or(0.0, |v| v[1]);
                        self.script_prop_velocities.insert(id.clone(), [*value, vy]);
                    }
                    TweenProp::VelY => {
                        let vx = self.script_prop_velocities.get(id).map_or(0.0, |v| v[0]);
                        self.script_prop_velocities.insert(id.clone(), [vx, *value]);
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
                        let a = value.clamp(0.0, 1.0);
                        self.script_prop_alphas.insert(id.clone(), a);
                        if self.script_texts.contains_key(id) {
                            self.script_text_alphas.insert(id.clone(), a);
                        }
                    }
                    _ => {}
                }
                false
            }
            ScriptCommand::SetPropTint { id, color } => {
                self.script_prop_alphas.insert(id.clone(), color[3].clamp(0.0, 1.0));
                false
            }
            ScriptCommand::SetPropFlip { id, flip_x, flip_y } => {
                self.script_prop_flips.insert(id.clone(), [*flip_x, *flip_y]);
                false
            }
            ScriptCommand::SetPropScroll { id, sx, sy } => {
                self.script_prop_scrolls.insert(id.clone(), [*sx, *sy]);
                false
            }
            ScriptCommand::SetPropZ { id, z } => {
                self.script_prop_z.insert(id.clone(), *z);
                false
            }
            ScriptCommand::SetPropSize { id, w, h } => {
                self.script_prop_sizes.insert(id.clone(), [*w, *h]);
                false
            }
            ScriptCommand::SetPropVelocity { id, vx, vy, .. } => {
                self.script_prop_velocities.insert(id.clone(), [*vx, *vy]);
                false
            }
            ScriptCommand::SetCharacterPos { target, x, y } => {
                self.script_char_positions.insert(target.clone(), [*x, *y]);
                false
            }
            ScriptCommand::SetCharZ { target, z } => {
                self.script_char_z.insert(target.clone(), *z);
                false
            }
            ScriptCommand::SpawnText { id, text, pos, color, .. } => {
                self.script_texts.insert(id.clone(), text.clone());
                self.script_text_positions.insert(id.clone(), *pos);
                self.script_text_alphas.insert(id.clone(), color[3].clamp(0.0, 1.0));
                false
            }
            ScriptCommand::SetText { id, text } => {
                self.script_texts.insert(id.clone(), text.clone());
                false
            }
            _ => false,
        }
    }
}
