use serde::{Deserialize, Serialize};

use crate::utils::easing::EaseKind;

/// Visual presentation template for HUD subtitles and lyrics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleTemplate {
    /// Single-line subtitle that swaps cleanly on bar changes.
    #[default]
    Simple,
    /// Multi-line scrolling lyrics canvas (default 3 lines: previous, current, next).
    ScrollThreeLine,
    /// Single-line exit fade + per-letter bottom-to-top bounce entrance.
    BounceLetters,
}

impl SubtitleTemplate {
    pub fn from_name(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "simple" | "normal" | "classic" | "single" | "default" => Some(Self::Simple),
            "scroll" | "scroll_three_line" | "three_line" | "3line" | "lyrics" | "spotify" => {
                Some(Self::ScrollThreeLine)
            }
            "bounce" | "bounce_letters" | "letters" | "drop" | "pop" => Some(Self::BounceLetters),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Simple => "SIMPLE",
            Self::ScrollThreeLine => "SCROLL 3-LINE",
            Self::BounceLetters => "BOUNCE LETTERS",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Simple => Self::ScrollThreeLine,
            Self::ScrollThreeLine => Self::BounceLetters,
            Self::BounceLetters => Self::Simple,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Simple => Self::BounceLetters,
            Self::ScrollThreeLine => Self::Simple,
            Self::BounceLetters => Self::ScrollThreeLine,
        }
    }
}

/// Highlight progression mode within the active subtitle/lyric bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubtitleHighlightMode {
    /// Whole active bar is colored uniformly with `active_color`.
    #[default]
    Line,
    /// Word-by-word highlight (uses Enhanced LRC timestamps when present, otherwise distributes
    /// across the bar duration).
    Word,
    /// Smooth letter-by-letter highlight (uses Enhanced LRC word spans when present, otherwise
    /// smoothly sweeps across the bar duration).
    Letter,
}

impl SubtitleHighlightMode {
    pub fn from_name(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "line" | "none" | "off" | "bar" | "normal" => Some(Self::Line),
            "word" | "words" | "word_by_word" | "karaoke_word" => Some(Self::Word),
            "letter" | "letters" | "char" | "chars" | "letter_by_letter" | "karaoke" => {
                Some(Self::Letter)
            }
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Line => "LINE",
            Self::Word => "WORD",
            Self::Letter => "LETTER",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Line => Self::Word,
            Self::Word => Self::Letter,
            Self::Letter => Self::Line,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Line => Self::Letter,
            Self::Word => Self::Line,
            Self::Letter => Self::Word,
        }
    }
}

/// Full configuration for subtitle/lyric parsing, idle gaps, colors, and animations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SubtitleConfig {
    pub template: SubtitleTemplate,
    pub highlight_mode: SubtitleHighlightMode,
    /// RGBA `[0..1]` color for the active line / highlighted text.
    pub active_color: [f32; 4],
    /// RGBA `[0..1]` color for inactive / unhighlighted text.
    pub inactive_color: [f32; 4],
    /// Optional RGBA override for the previous (past) line in scroll mode.
    pub past_color: Option<[f32; 4]>,
    /// Optional RGBA override for the next (upcoming) line in scroll mode.
    pub upcoming_color: Option<[f32; 4]>,
    pub font_size: f32,
    pub line_spacing: f32,
    /// Maximum visible lines in `ScrollThreeLine` mode (default 3: prev, current, next).
    pub max_lines: usize,
    pub bottom_margin: f32,
    /// Optional text shown between bars when no lyric cue is active (e.g. `""`, `"..."`, `"♪"`).
    pub idle_text: String,
    /// Minimum silence duration (ms) after a bar ends before showing `idle_text`.
    pub gap_threshold_ms: f64,
    /// Optional maximum duration (ms) for an open-ended LRC bar or overly long cue.
    pub max_line_duration_ms: Option<f64>,
    /// Optional gap (ms) trimmed before the next LRC line starts so bars don't collide.
    pub lrc_end_gap_ms: f64,
    /// Transition duration (s) for `ScrollThreeLine` slide animations.
    pub transition_duration_s: f64,
    /// Easing name for `ScrollThreeLine` slide animations (e.g. `"quadInOut"`, `"cubeInOut"`).
    pub transition_ease: String,
    /// Entrance animation duration (s) for `BounceLetters` (per letter).
    pub enter_duration_s: f64,
    /// Exit fade duration (s) for `BounceLetters` outgoing line.
    pub exit_duration_s: f64,
    /// Easing name for `BounceLetters` entrance (default `"bounceOut"`).
    pub enter_ease: String,
    /// Easing name for `BounceLetters` exit fade (default `"quadOut"`).
    pub exit_ease: String,
    /// Starting vertical offset (px) for entering letters (`< 0` = rises from below).
    pub enter_offset_y: f32,
    /// Vertical drift (px) for exiting line in `BounceLetters`.
    pub exit_offset_y: f32,
    /// Delay (s) between consecutive letters in `BounceLetters`.
    pub letter_stagger_s: f64,
    /// Horizontal glyph advance multiplier (`font_size * letter_spacing`) for per-letter layout.
    pub letter_spacing: f32,
    /// Softness window (ms) for smooth per-letter / per-word color blending.
    pub highlight_softness_ms: f64,
    /// Scale multiplier for the active line.
    pub active_scale: f32,
    /// Scale multiplier for inactive (past/next) lines in `ScrollThreeLine`.
    pub inactive_scale: f32,
    /// Scale multiplier for secondary / duet / background-vocal lines under the active bar.
    pub sub_line_scale: f32,
    /// Vertical offset (px) below the active bar for each secondary / duet sub-line.
    pub sub_line_spacing: f32,
    /// Optional RGBA override for highlighted text on secondary / duet sub-lines.
    pub sub_active_color: Option<[f32; 4]>,
    /// Optional RGBA override for unhighlighted text on secondary / duet sub-lines.
    pub sub_inactive_color: Option<[f32; 4]>,
}

impl Default for SubtitleConfig {
    fn default() -> Self {
        Self {
            template: SubtitleTemplate::Simple,
            highlight_mode: SubtitleHighlightMode::Line,
            active_color: [1.0, 1.0, 1.0, 1.0],
            inactive_color: [0.58, 0.58, 0.58, 0.72],
            past_color: None,
            upcoming_color: None,
            font_size: 30.0,
            line_spacing: 38.0,
            max_lines: 3,
            bottom_margin: 139.0,
            idle_text: String::new(),
            gap_threshold_ms: 650.0,
            max_line_duration_ms: None,
            lrc_end_gap_ms: 0.0,
            transition_duration_s: 0.35,
            transition_ease: "quadInOut".to_string(),
            enter_duration_s: 0.42,
            exit_duration_s: 0.22,
            enter_ease: "bounceOut".to_string(),
            exit_ease: "quadOut".to_string(),
            enter_offset_y: -34.0,
            exit_offset_y: 12.0,
            letter_stagger_s: 0.025,
            letter_spacing: 0.62,
            highlight_softness_ms: 90.0,
            active_scale: 1.0,
            inactive_scale: 0.86,
            sub_line_scale: 0.72,
            sub_line_spacing: 28.0,
            sub_active_color: None,
            sub_inactive_color: None,
        }
    }
}

impl SubtitleConfig {
    pub fn effective_past_color(&self) -> [f32; 4] {
        self.past_color.unwrap_or(self.inactive_color)
    }

    pub fn effective_upcoming_color(&self) -> [f32; 4] {
        self.upcoming_color.unwrap_or(self.inactive_color)
    }

    pub fn effective_sub_active_color(&self) -> [f32; 4] {
        self.sub_active_color.unwrap_or([
            self.active_color[0],
            self.active_color[1],
            self.active_color[2],
            (self.active_color[3] * 0.9).clamp(0.0, 1.0),
        ])
    }

    pub fn effective_sub_inactive_color(&self) -> [f32; 4] {
        self.sub_inactive_color.unwrap_or(self.inactive_color)
    }

    pub fn transition_ease_kind(&self) -> EaseKind {
        EaseKind::from_name(&self.transition_ease, "").unwrap_or(EaseKind::QuadInOut)
    }

    pub fn enter_ease_kind(&self) -> EaseKind {
        EaseKind::from_name(&self.enter_ease, "").unwrap_or(EaseKind::BounceOut)
    }

    pub fn exit_ease_kind(&self) -> EaseKind {
        EaseKind::from_name(&self.exit_ease, "").unwrap_or(EaseKind::QuadOut)
    }

    pub fn apply_patch(&mut self, patch: &SubtitleConfigPatch) {
        if let Some(v) = patch.template {
            self.template = v;
        }
        if let Some(v) = patch.highlight_mode {
            self.highlight_mode = v;
        }
        if let Some(v) = patch.active_color {
            self.active_color = v;
        }
        if let Some(v) = patch.inactive_color {
            self.inactive_color = v;
        }
        if let Some(v) = patch.past_color {
            self.past_color = v;
        }
        if let Some(v) = patch.upcoming_color {
            self.upcoming_color = v;
        }
        if let Some(v) = patch.font_size {
            self.font_size = v.max(6.0);
        }
        if let Some(v) = patch.line_spacing {
            self.line_spacing = v.max(8.0);
        }
        if let Some(v) = patch.max_lines {
            self.max_lines = v.clamp(1, 5);
        }
        if let Some(v) = patch.bottom_margin {
            self.bottom_margin = v;
        }
        if let Some(v) = &patch.idle_text {
            self.idle_text = v.clone();
        }
        if let Some(v) = patch.gap_threshold_ms {
            self.gap_threshold_ms = v.max(0.0);
        }
        if let Some(v) = patch.max_line_duration_ms {
            self.max_line_duration_ms = v.filter(|d| *d > 0.0);
        }
        if let Some(v) = patch.lrc_end_gap_ms {
            self.lrc_end_gap_ms = v.max(0.0);
        }
        if let Some(v) = patch.transition_duration_s {
            self.transition_duration_s = v.max(0.0);
        }
        if let Some(v) = &patch.transition_ease {
            self.transition_ease = v.clone();
        }
        if let Some(v) = patch.enter_duration_s {
            self.enter_duration_s = v.max(0.0);
        }
        if let Some(v) = patch.exit_duration_s {
            self.exit_duration_s = v.max(0.0);
        }
        if let Some(v) = &patch.enter_ease {
            self.enter_ease = v.clone();
        }
        if let Some(v) = &patch.exit_ease {
            self.exit_ease = v.clone();
        }
        if let Some(v) = patch.enter_offset_y {
            self.enter_offset_y = v;
        }
        if let Some(v) = patch.exit_offset_y {
            self.exit_offset_y = v;
        }
        if let Some(v) = patch.letter_stagger_s {
            self.letter_stagger_s = v.max(0.0);
        }
        if let Some(v) = patch.letter_spacing {
            self.letter_spacing = v.clamp(0.25, 2.0);
        }
        if let Some(v) = patch.highlight_softness_ms {
            self.highlight_softness_ms = v.max(0.0);
        }
        if let Some(v) = patch.active_scale {
            self.active_scale = v.clamp(0.25, 4.0);
        }
        if let Some(v) = patch.inactive_scale {
            self.inactive_scale = v.clamp(0.25, 4.0);
        }
        if let Some(v) = patch.sub_line_scale {
            self.sub_line_scale = v.clamp(0.25, 2.0);
        }
        if let Some(v) = patch.sub_line_spacing {
            self.sub_line_spacing = v.max(8.0);
        }
        if let Some(v) = patch.sub_active_color {
            self.sub_active_color = v;
        }
        if let Some(v) = patch.sub_inactive_color {
            self.sub_inactive_color = v;
        }
    }
}

/// Partial update to [`SubtitleConfig`] coming from scripts or JSON sidecars.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SubtitleConfigPatch {
    pub template: Option<SubtitleTemplate>,
    pub highlight_mode: Option<SubtitleHighlightMode>,
    pub active_color: Option<[f32; 4]>,
    pub inactive_color: Option<[f32; 4]>,
    pub past_color: Option<Option<[f32; 4]>>,
    pub upcoming_color: Option<Option<[f32; 4]>>,
    pub font_size: Option<f32>,
    pub line_spacing: Option<f32>,
    pub max_lines: Option<usize>,
    pub bottom_margin: Option<f32>,
    pub idle_text: Option<String>,
    pub gap_threshold_ms: Option<f64>,
    pub max_line_duration_ms: Option<Option<f64>>,
    pub lrc_end_gap_ms: Option<f64>,
    pub transition_duration_s: Option<f64>,
    pub transition_ease: Option<String>,
    pub enter_duration_s: Option<f64>,
    pub exit_duration_s: Option<f64>,
    pub enter_ease: Option<String>,
    pub exit_ease: Option<String>,
    pub enter_offset_y: Option<f32>,
    pub exit_offset_y: Option<f32>,
    pub letter_stagger_s: Option<f64>,
    pub letter_spacing: Option<f32>,
    pub highlight_softness_ms: Option<f64>,
    pub active_scale: Option<f32>,
    pub inactive_scale: Option<f32>,
    pub sub_line_scale: Option<f32>,
    pub sub_line_spacing: Option<f32>,
    pub sub_active_color: Option<Option<[f32; 4]>>,
    pub sub_inactive_color: Option<Option<[f32; 4]>>,
}

/// Parses a `#RRGGBB`, `#RRGGBBAA`, `0xRRGGBB`, or `0xAARRGGBB` color string into `[r, g, b, a]`.
pub fn parse_subtitle_color_str(raw: &str) -> Option<[f32; 4]> {
    let s = raw.trim();
    let hex = s
        .strip_prefix('#')
        .or_else(|| s.strip_prefix("0x"))
        .or_else(|| s.strip_prefix("0X"))?;
    let bytes = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok().map(|b| f32::from(b) / 255.0);
    match hex.len() {
        6 => Some([bytes(0)?, bytes(2)?, bytes(4)?, 1.0]),
        8 => {
            if s.starts_with('#') {
                // CSS-style #RRGGBBAA
                Some([bytes(0)?, bytes(2)?, bytes(4)?, bytes(6)?])
            } else {
                // Haxe/OpenFL-style 0xAARRGGBB
                Some([bytes(2)?, bytes(4)?, bytes(6)?, bytes(0)?])
            }
        }
        _ => None,
    }
}

fn parse_color_json(val: &serde_json::Value) -> Option<[f32; 4]> {
    if let Some(s) = val.as_str() {
        return parse_subtitle_color_str(s);
    }
    if let Some(arr) = val.as_array() {
        let f = |i: usize| arr.get(i).and_then(serde_json::Value::as_f64).map(|v| v as f32);
        return match arr.len() {
            3 => Some([f(0)?, f(1)?, f(2)?, 1.0]),
            4 => Some([f(0)?, f(1)?, f(2)?, f(3)?]),
            _ => None,
        };
    }
    None
}

impl SubtitleConfigPatch {
    /// Parses a flexible JSON object into a [`SubtitleConfigPatch`], accepting friendly aliases
    /// (`template`/`style`, `highlight`/`highlight_mode`, hex colors or `[r,g,b,a]` arrays, etc.).
    pub fn from_json_value(val: &serde_json::Value) -> Self {
        let Some(obj) = val.as_object() else {
            return Self::default();
        };
        let mut p = Self::default();
        let str_key = |keys: &[&str]| -> Option<String> {
            keys.iter()
                .find_map(|k| obj.get(*k).and_then(serde_json::Value::as_str))
                .map(ToString::to_string)
        };
        let f64_key = |keys: &[&str]| -> Option<f64> {
            keys.iter()
                .find_map(|k| obj.get(*k).and_then(serde_json::Value::as_f64))
        };
        let f32_key = |keys: &[&str]| -> Option<f32> { f64_key(keys).map(|v| v as f32) };

        if let Some(s) = str_key(&["template", "style", "animation", "mode"]) {
            p.template = SubtitleTemplate::from_name(&s);
        }
        if let Some(s) = str_key(&["highlight_mode", "highlight", "karaoke", "fill_mode"]) {
            p.highlight_mode = SubtitleHighlightMode::from_name(&s);
        }
        for key in ["active_color", "color", "highlight_color"] {
            if let Some(v) = obj.get(key)
                && let Some(c) = parse_color_json(v)
            {
                p.active_color = Some(c);
                break;
            }
        }
        for key in ["inactive_color", "dim_color", "base_color"] {
            if let Some(v) = obj.get(key)
                && let Some(c) = parse_color_json(v)
            {
                p.inactive_color = Some(c);
                break;
            }
        }
        if let Some(v) = obj.get("past_color") {
            p.past_color = Some(if v.is_null() { None } else { parse_color_json(v) });
        }
        if let Some(v) = obj.get("upcoming_color").or_else(|| obj.get("next_color")) {
            p.upcoming_color = Some(if v.is_null() { None } else { parse_color_json(v) });
        }
        p.font_size = f32_key(&["font_size", "size"]);
        p.line_spacing = f32_key(&["line_spacing", "spacing"]);
        if let Some(v) = f64_key(&["max_lines", "lines"]) {
            p.max_lines = Some(v.max(1.0) as usize);
        }
        p.bottom_margin = f32_key(&["bottom_margin", "margin_y", "y_margin"]);
        p.idle_text = str_key(&["idle_text", "gap_text", "empty_text", "placeholder"]);
        p.gap_threshold_ms = f64_key(&["gap_threshold_ms", "idle_delay_ms", "gap_delay_ms"]);
        if let Some(v) = obj
            .get("max_line_duration_ms")
            .or_else(|| obj.get("max_bar_ms"))
        {
            p.max_line_duration_ms = Some(if v.is_null() {
                None
            } else {
                v.as_f64().filter(|d| *d > 0.0)
            });
        }
        p.lrc_end_gap_ms = f64_key(&["lrc_end_gap_ms", "end_gap_ms", "bar_gap_ms"]);
        p.transition_duration_s = f64_key(&["transition_duration_s", "scroll_duration_s", "transition_s"]);
        p.transition_ease = str_key(&["transition_ease", "scroll_ease"]);
        p.enter_duration_s = f64_key(&["enter_duration_s", "enter_s", "bounce_duration_s"]);
        p.exit_duration_s = f64_key(&["exit_duration_s", "exit_s", "fade_out_s"]);
        p.enter_ease = str_key(&["enter_ease", "bounce_ease"]);
        p.exit_ease = str_key(&["exit_ease", "fade_out_ease"]);
        p.enter_offset_y = f32_key(&["enter_offset_y", "rise_offset_y"]);
        p.exit_offset_y = f32_key(&["exit_offset_y"]);
        p.letter_stagger_s = f64_key(&["letter_stagger_s", "stagger_s"]);
        p.letter_spacing = f32_key(&["letter_spacing", "char_spacing"]);
        p.highlight_softness_ms = f64_key(&["highlight_softness_ms", "letter_fade_ms", "softness_ms"]);
        p.active_scale = f32_key(&["active_scale"]);
        p.inactive_scale = f32_key(&["inactive_scale"]);
        p.sub_line_scale = f32_key(&["sub_line_scale", "sub_scale", "bg_scale"]);
        p.sub_line_spacing = f32_key(&["sub_line_spacing", "sub_spacing", "bg_spacing"]);
        if let Some(v) = obj
            .get("sub_active_color")
            .or_else(|| obj.get("bg_active_color"))
            .or_else(|| obj.get("sub_color"))
        {
            p.sub_active_color = Some(if v.is_null() { None } else { parse_color_json(v) });
        }
        if let Some(v) = obj
            .get("sub_inactive_color")
            .or_else(|| obj.get("bg_inactive_color"))
        {
            p.sub_inactive_color = Some(if v.is_null() { None } else { parse_color_json(v) });
        }
        p
    }
}
