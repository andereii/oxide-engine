use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use serde_json::Value;

use crate::play::scene::PlayState;
use crate::play::types::SongEventData;
use crate::scripting::{HookKind, ScriptEvent, ScriptValue};
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
            self.pending_shader_events.push(event.clone());
            self.pending_hooks.push((
                HookKind::SongEvent,
                ScriptEvent::new(HookKind::SongEvent)
                    .with_field("name", event.kind.clone())
                    .with_field("kind", event.kind.clone())
                    .with_field("time", event.time_ms)
                    .with_field("value", ScriptValue::from_json(&event.raw_value)),
            ));
            let target_lower = event.strumline_target.to_ascii_lowercase();
            let apply_player = target_lower != "opponent";
            let apply_opponent = target_lower != "player";
            match event.kind.as_str() {
                "FocusCamera" => self.apply_focus_camera_event(&event),
                "ZoomCamera" => self.apply_zoom_camera_event(&event),
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
                _ => {}
            }
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
