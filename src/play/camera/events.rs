use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::play::scene::{PlayState, SongEventRevertSnapshot};
use crate::play::types::SongEventData;
use crate::scripting::{HookKind, ScriptCommand, ScriptEvent, ScriptValue};
use crate::utils::constants::{DEFAULT_BOP_INTENSITY, DEFAULT_ZOOM_OFFSET, DEFAULT_ZOOM_RATE};
use crate::utils::easing::EaseKind;

#[derive(Debug, Deserialize)]
struct RawEventEntry {
    t: f64,
    e: String,
    #[serde(default)]
    v: Value,
}

pub fn load_song_events(events_path: &Path, difficulty: &str) -> Vec<SongEventData> {
    let Ok(raw) = fs::read_to_string(events_path) else {
        return Vec::new();
    };
    let entries: Vec<RawEventEntry> =
        if let Ok(by_diff) = serde_json::from_str::<HashMap<String, Vec<RawEventEntry>>>(&raw) {
            by_diff
                .into_iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(difficulty))
                .map(|(_, v)| v)
                .unwrap_or_default()
        } else if let Ok(flat) = serde_json::from_str::<Vec<RawEventEntry>>(&raw) {
            flat
        } else {
            return Vec::new();
        };

    let mut out: Vec<SongEventData> = entries.into_iter().map(parse_song_event).collect();
    out.sort_by(|a, b| a.time_ms.total_cmp(&b.time_ms));
    out
}

fn parse_song_event(raw: RawEventEntry) -> SongEventData {
    parse_raw_song_event(raw.t, raw.e, raw.v)
}

/// Parses a raw chart/script event entry into a normalized [`SongEventData`].
pub fn parse_raw_song_event(time_ms: f64, kind: String, v: serde_json::Value) -> SongEventData {
    let mut ev = SongEventData {
        time_ms,
        kind,
        raw_value: v.clone(),
        ..Default::default()
    };
    let v = &v;
    match ev.kind.as_str() {
        "FocusCamera" => {
            ev.target_char = get_int(v, "char").or_else(|| scalar_int(v));
            ev.target_x = get_float(v, "x").unwrap_or(0.0);
            ev.target_y = get_float(v, "y").unwrap_or(0.0);
            ev.duration_steps = Some(get_float(v, "duration").unwrap_or(4.0));
            ev.ease = get_string(v, "ease").unwrap_or_else(|| "CLASSIC".to_string());
            ev.ease_dir = get_string(v, "easeDir").unwrap_or_else(|| "In".to_string());
        }
        "ZoomCamera" => {
            ev.target_zoom = get_float(v, "zoom").unwrap_or(1.0);
            ev.duration_steps = Some(get_float(v, "duration").unwrap_or(4.0));
            ev.absolute = get_string(v, "mode").as_deref().unwrap_or("direct") == "direct";
            ev.ease = get_string(v, "ease").unwrap_or_else(|| "linear".to_string());
            ev.ease_dir = get_string(v, "easeDir").unwrap_or_else(|| "In".to_string());
        }
        "SetCameraBop" => {
            ev.bop_intensity = get_float(v, "intensity").unwrap_or(1.0);
            ev.bop_rate = get_float(v, "rate").unwrap_or(DEFAULT_ZOOM_RATE);
            ev.bop_offset = get_float(v, "offset").unwrap_or(DEFAULT_ZOOM_OFFSET);
        }
        "ScrollSpeed" => {
            ev.scroll_speed = get_float(v, "scroll");
            ev.start_scroll = get_float(v, "startScroll");
            ev.use_start = get_bool(v, "useStart").unwrap_or(false);
            ev.absolute = get_bool(v, "absolute").unwrap_or(false);
            ev.duration_steps = get_float(v, "duration");
            ev.ease = get_string(v, "ease").unwrap_or_else(|| "linear".to_string());
            ev.ease_dir = get_string(v, "easeDir").unwrap_or_else(|| "In".to_string());
            ev.strumline_target = get_string(v, "strumline").unwrap_or_default();
        }
        "RenderDistance" => {
            ev.render_distance_ms = get_float(v, "distance");
            ev.use_default_render_distance = get_bool(v, "useDefault").unwrap_or(false);
            ev.strumline_target = get_string(v, "strumline").unwrap_or_default();
        }
        _ => {}
    }
    ev
}

impl PlayState {
    pub fn process_song_events(&mut self) {
        let pos_ms = self.conductor.song_position_ms;
        if !self.song_events.iter().any(|e| !e.triggered && pos_ms >= e.time_ms) {
            return;
        }
        let mut due = Vec::new();
        for event in &mut self.song_events {
            if !event.triggered && pos_ms >= event.time_ms {
                event.triggered = true;
                due.push(event.clone());
            }
        }
        for event in due {
            let seq = self.next_song_event_seq;
            self.next_song_event_seq = self.next_song_event_seq.wrapping_add(1);
            let snapshot = self.capture_song_event_revert();
            self.pending_song_event_reverts.insert(seq, snapshot);

            self.pending_shader_events.push(event.clone());
            self.pending_hooks.push((
                HookKind::SongEvent,
                ScriptEvent::new(HookKind::SongEvent)
                    .with_field("event_idx", seq as i64)
                    .with_field("name", event.kind.clone())
                    .with_field("kind", event.kind.clone())
                    .with_field("time", event.time_ms)
                    .with_field("value", ScriptValue::from_json(&event.raw_value)),
            ));
            self.apply_due_song_event(&event);
        }
    }

    pub fn capture_song_event_revert(&self) -> SongEventRevertSnapshot {
        SongEventRevertSnapshot {
            camera: self.camera.clone(),
            player_scroll: self.player_strumline.scroll_speed,
            opponent_scroll: self.opponent_strumline.scroll_speed,
            scroll_tweens: self.scroll_speed_tweens.clone(),
            player_custom_dist: (
                self.player_strumline.use_custom_render_distance,
                self.player_strumline.custom_render_distance_ms,
            ),
            opponent_custom_dist: (
                self.opponent_strumline.use_custom_render_distance,
                self.opponent_strumline.custom_render_distance_ms,
            ),
        }
    }

    pub fn restore_song_event_revert(&mut self, snap: SongEventRevertSnapshot) {
        self.camera = snap.camera;
        self.player_strumline.scroll_speed = snap.player_scroll;
        self.opponent_strumline.scroll_speed = snap.opponent_scroll;
        self.scroll_speed_tweens = snap.scroll_tweens;
        self.player_strumline.use_custom_render_distance = snap.player_custom_dist.0;
        self.player_strumline.custom_render_distance_ms = snap.player_custom_dist.1;
        self.opponent_strumline.use_custom_render_distance = snap.opponent_custom_dist.0;
        self.opponent_strumline.custom_render_distance_ms = snap.opponent_custom_dist.1;
    }

    pub fn apply_due_song_event(&mut self, event: &SongEventData) {
        let target_lower = event.strumline_target.to_ascii_lowercase();
        let apply_player = target_lower != "opponent";
        let apply_opponent = target_lower != "player";
        match event.kind.as_str() {
            "FocusCamera" => self.apply_focus_camera_event(event),
            "ZoomCamera" => self.apply_zoom_camera_event(event),
            "SetCameraBop" => {
                self.camera.bop_intensity = (DEFAULT_BOP_INTENSITY - 1.0) * event.bop_intensity + 1.0;
                self.camera.hud_zoom_intensity = (DEFAULT_BOP_INTENSITY - 1.0) * event.bop_intensity * 2.0;
                self.camera.zoom_rate = event.bop_rate;
                self.camera.zoom_rate_offset = event.bop_offset;
            }
            "ScrollSpeed" => {
                if let Some(raw_speed) = event.scroll_speed {
                    let base = if event.absolute { 1.0 } else { self.chart.scroll_speed };
                    let target_speed = raw_speed * base;
                    let start_scroll = event.use_start.then_some(event.start_scroll).flatten().map(|s| s * base);
                    let duration_s = event
                        .duration_steps
                        .map(|steps| steps * self.conductor.step_length_ms() / 1000.0)
                        .unwrap_or(event.duration_s);
                    let ease = EaseKind::from_name(&event.ease, &event.ease_dir);
                    self.tween_scroll_speed_eased(target_speed, duration_s, ease, start_scroll, apply_player, apply_opponent);
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
            "PlayAnimation" => {
                let anim = event
                    .raw_value
                    .get("anim")
                    .and_then(|v| v.as_str())
                    .unwrap_or("idle")
                    .to_string();
                let force = event
                    .raw_value
                    .get("force")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let target = event
                    .raw_value
                    .get("target")
                    .and_then(|v| v.as_str())
                    .unwrap_or("boyfriend")
                    .to_string();
                self.pending_event_commands.push(ScriptCommand::PlayAnim {
                    target,
                    anim,
                    force,
                });
            }
            "SetHealthIcon" => {
                let side = match event.raw_value.get("char") {
                    Some(v) => {
                        if let Some(idx) = scalar_int(v) {
                            match idx {
                                0 => "bf".to_string(),
                                1 => "dad".to_string(),
                                2 => "gf".to_string(),
                                _ => idx.to_string(),
                            }
                        } else if let Some(s) =
                            v.as_str().map(str::trim).filter(|s| !s.is_empty())
                        {
                            s.to_string()
                        } else {
                            "bf".to_string()
                        }
                    }
                    None => "bf".to_string(),
                };
                if let Some(char_id) = get_string(&event.raw_value, "id")
                    .or_else(|| get_string(&event.raw_value, "charId"))
                    .or_else(|| get_string(&event.raw_value, "icon"))
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                {
                    self.pending_event_commands.push(ScriptCommand::SetIcon {
                        side: side.clone(),
                        char_id,
                    });
                }
                if let Some(scale) =
                    get_float(&event.raw_value, "scale").filter(|s| s.is_finite())
                {
                    self.pending_event_commands.push(ScriptCommand::SetIconScale {
                        side: side.clone(),
                        scale: scale as f32,
                    });
                }
                if let Some(flip_x) = get_bool(&event.raw_value, "flipX") {
                    self.pending_event_commands.push(ScriptCommand::SetIconFlip {
                        side,
                        flip_x,
                    });
                }
            }
            _ => {}
        }
    }

    fn apply_focus_camera_event(&mut self, event: &SongEventData) {
        let focus = match event.target_char.unwrap_or(0) {
            0 => self.camera.focus_points[0],
            1 => self.camera.focus_points[1],
            2 => self.camera.focus_points[2],
            _ => Some([0.0, 0.0]),
        };
        let Some(focus) = focus else {
            return;
        };
        let target = [event.target_x + focus[0], event.target_y + focus[1]];
        if !target[0].is_finite() || !target[1].is_finite() {
            return;
        }
        if event.ease.eq_ignore_ascii_case("CLASSIC") {
            self.cancel_camera_follow_tween();
            self.camera.follow_target = Some(target);
        } else if event.ease.eq_ignore_ascii_case("INSTANT") {
            self.tween_camera_to_position(target[0], target[1], 0.0);
        } else if let Some(ease) = EaseKind::from_name(&event.ease, &event.ease_dir) {
            let duration_s = event
                .duration_steps
                .map(|steps| steps * self.conductor.step_length_ms() / 1000.0)
                .unwrap_or(event.duration_s);
            self.tween_camera_to_position_eased(target[0], target[1], duration_s, ease);
        }
    }

    fn apply_zoom_camera_event(&mut self, event: &SongEventData) {
        if event.ease.eq_ignore_ascii_case("INSTANT") {
            self.tween_camera_zoom_eased(event.target_zoom, 0.0, event.absolute, None);
        } else if let Some(ease) = EaseKind::from_name(&event.ease, &event.ease_dir) {
            let duration_s = event
                .duration_steps
                .map(|steps| steps * self.conductor.step_length_ms() / 1000.0)
                .unwrap_or(event.duration_s);
            self.tween_camera_zoom_eased(event.target_zoom, duration_s, event.absolute, Some(ease));
        }
    }
}

fn scalar_int(v: &Value) -> Option<i64> {
    v.as_i64()
        .or_else(|| v.as_f64().map(|n| n.trunc() as i64))
        .or_else(|| v.as_str().and_then(|s| s.trim().parse::<i64>().ok()))
}

fn get_int(v: &Value, key: &str) -> Option<i64> {
    v.get(key).and_then(scalar_int)
}

fn get_float(v: &Value, key: &str) -> Option<f64> {
    let val = v.get(key)?;
    val.as_f64().or_else(|| val.as_str().and_then(|s| s.trim().parse::<f64>().ok()))
}

fn get_bool(v: &Value, key: &str) -> Option<bool> {
    let val = v.get(key)?;
    val.as_bool().or_else(|| match val.as_str()?.trim().to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    })
}

fn get_string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::play::types::PlayStateParams;

    use super::*;

    #[test]
    fn set_health_icon_maps_native_char_indices_and_scale() {
        let mut state = PlayState::new(PlayStateParams::default());
        let ev_dad = parse_raw_song_event(
            1000.0,
            "SetHealthIcon".to_string(),
            json!({ "char": 1, "id": "tankman-bloody", "scale": 1.25 }),
        );
        state.apply_due_song_event(&ev_dad);

        assert_eq!(state.pending_event_commands.len(), 2);
        assert!(matches!(
            &state.pending_event_commands[0],
            ScriptCommand::SetIcon { side, char_id } if side == "dad" && char_id == "tankman-bloody"
        ));
        assert!(matches!(
            &state.pending_event_commands[1],
            ScriptCommand::SetIconScale { side, scale }
                if side == "dad" && (*scale - 1.25).abs() < 1e-5
        ));

        state.pending_event_commands.clear();
        let ev_bf = parse_raw_song_event(
            2000.0,
            "SetHealthIcon".to_string(),
            json!({ "char": 0, "charId": "pico" }),
        );
        state.apply_due_song_event(&ev_bf);
        assert!(matches!(
            state.pending_event_commands.as_slice(),
            [ScriptCommand::SetIcon { side, char_id }] if side == "bf" && char_id == "pico"
        ));

        state.pending_event_commands.clear();
        let ev_str = parse_raw_song_event(
            3000.0,
            "SetHealthIcon".to_string(),
            json!({ "char": "gf", "icon": "nene" }),
        );
        state.apply_due_song_event(&ev_str);
        assert!(matches!(
            state.pending_event_commands.as_slice(),
            [ScriptCommand::SetIcon { side, char_id }] if side == "gf" && char_id == "nene"
        ));
    }
}
