use serde::{Deserialize, Serialize};

use crate::scripting::value::ScriptValue;
use crate::utils::easing::EaseKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TweenProp {
    X,
    Y,
    Z,
    VelX,
    VelY,
    Alpha,
    Angle,
    HudAngle,
    ScaleX,
    ScaleY,
    Zoom,
    HudZoom,
    Scroll,
    Volume,
}

impl TweenProp {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "x" | "pos_x" => Some(Self::X),
            "y" | "pos_y" => Some(Self::Y),
            "z" | "z_index" => Some(Self::Z),
            "vx" | "vel_x" => Some(Self::VelX),
            "vy" | "vel_y" => Some(Self::VelY),
            "alpha" | "opacity" | "a" => Some(Self::Alpha),
            "angle" | "rot" | "rotation" => Some(Self::Angle),
            "hud_angle" | "hudangle" => Some(Self::HudAngle),
            "scale_x" | "scalex" | "sx" => Some(Self::ScaleX),
            "scale_y" | "scaley" | "sy" => Some(Self::ScaleY),
            "zoom" => Some(Self::Zoom),
            "hud_zoom" | "hudzoom" => Some(Self::HudZoom),
            "scroll" | "scroll_speed" => Some(Self::Scroll),
            "vol" | "volume" => Some(Self::Volume),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TweenTarget {
    Camera,
    Scroll,
    Lane(usize),
    Prop(String),
    Char(String),
    Sound(String),
    Text(String),
}

impl TweenTarget {
    pub fn parse(raw: &str) -> Self {
        let trimmed = raw.trim();
        let lower = trimmed.to_ascii_lowercase();
        if matches!(lower.as_str(), "cam" | "camera" | "cam_game") {
            return Self::Camera;
        }
        if matches!(lower.as_str(), "scroll" | "speed") {
            return Self::Scroll;
        }
        if let Some(idx_str) = lower.strip_prefix("lane:").or_else(|| lower.strip_prefix("strum:"))
            && let Ok(lane) = idx_str.parse::<usize>()
        {
            return Self::Lane(lane);
        }
        if let Some(idx) = trimmed
            .strip_prefix("char:")
            .or_else(|| trimmed.strip_prefix("CHAR:"))
        {
            return Self::Char(idx.trim().to_string());
        }
        if matches!(
            lower.as_str(),
            "bf" | "dad" | "gf" | "player" | "opponent" | "spectator"
        ) {
            return Self::Char(lower);
        }
        if let Some(tag) = trimmed
            .strip_prefix("snd:")
            .or_else(|| trimmed.strip_prefix("sound:"))
        {
            return Self::Sound(tag.trim().to_string());
        }
        if let Some(tid) = trimmed
            .strip_prefix("txt:")
            .or_else(|| trimmed.strip_prefix("text:"))
        {
            return Self::Text(tid.trim().to_string());
        }
        let prop_id = trimmed.strip_prefix("prop:").unwrap_or(trimmed);
        Self::Prop(prop_id.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct PropertyTween {
    pub target: TweenTarget,
    pub prop: TweenProp,
    pub start: f64,
    pub end: f64,
    pub elapsed_s: f64,
    pub duration_s: f64,
    pub ease: EaseKind,
    pub on_complete: Option<(String, String)>,
}

impl PropertyTween {
    pub fn sample(&self) -> f64 {
        if self.duration_s <= 0.0 {
            return self.end;
        }
        let t = (self.elapsed_s / self.duration_s).clamp(0.0, 1.0);
        let eased = self.ease.eval(t);
        self.start + (self.end - self.start) * eased
    }

    pub fn is_finished(&self) -> bool {
        self.duration_s <= 0.0 || self.elapsed_s >= self.duration_s
    }
}

#[derive(Debug, Clone)]
pub struct ScriptTimer {
    pub id: u64,
    pub script_id: String,
    pub fn_name: String,
    pub interval_s: f64,
    pub remaining_s: f64,
    pub repeats_left: Option<u32>,
    pub args: Vec<ScriptValue>,
}
