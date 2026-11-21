use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::play::camera::hud::hud_render_layer;
use crate::play::scene::PlayState;
use crate::utils::constants::SCREEN_HEIGHT;

/// One parsed SRT cue, with times relative to the start of its subtitle track.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleCue {
    pub start_ms: f64,
    pub end_ms: f64,
    pub text: String,
}

#[derive(Component)]
pub struct HudSubtitleText;

const SUBTITLE_BOTTOM_MARGIN: f32 = 139.0;
const SUBTITLE_FONT_SIZE: f32 = 30.0;
// Bevy's default 2D orthographic frustum spans z = -1000..1000, so the native
// FlxSprite zIndex of 10000 cannot be used literally here.
const SUBTITLE_Z: f32 = 100.0;

/// Creates the bottom-centered subtitle text on the HUD layer.
pub fn spawn_subtitle_visuals(commands: &mut Commands, font: Handle<Font>) {
    let bottom = -SCREEN_HEIGHT * 0.5 + SUBTITLE_BOTTOM_MARGIN;
    commands.spawn((
        Text2d::new(""),
        TextFont {
            font: font.into(),
            font_size: FontSize::Px(SUBTITLE_FONT_SIZE),
            ..default()
        },
        TextColor(Color::WHITE),
        TextLayout::justify(Justify::Center),
        bevy::sprite::Text2dShadow {
            offset: Vec2::new(2.0, -2.0),
            color: Color::BLACK,
        },
        Anchor::BOTTOM_CENTER,
        Transform::from_xyz(0.0, bottom, SUBTITLE_Z),
        // Text2d entities spawned hidden do not get a usable initial text layout
        // in Bevy 0.20, so keep the (empty) text visible; no cue means empty text.
        Visibility::Visible,
        HudSubtitleText,
        hud_render_layer(),
    ));
}

/// Applies the currently active subtitle text to the HUD entities.
pub fn sync_subtitles(
    state: Res<PlayState>,
    mut texts: Query<(&mut Text2d, &mut Visibility), With<HudSubtitleText>>,
) {
    let text = state.subtitle_text.as_str();

    for (mut text2d, mut visibility) in &mut texts {
        if text2d.0 != text {
            **text2d = text.to_string();
        }
        // Keep the text entity visible even when there is no cue. Empty Text2d
        // renders nothing, and toggling Visibility across long gaps can leave
        // Bevy's text layout without a fresh render.
        *visibility = Visibility::Visible;
    }
}

/// Parses SRT text into timed cues. Unknown or malformed blocks are skipped rather than
/// rejecting the whole track.
pub fn parse_srt(raw: &str) -> Vec<SubtitleCue> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.trim_start_matches('\u{feff}');
    let mut cues = Vec::new();

    for block in normalized.split("\n\n") {
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        if lines.is_empty() {
            continue;
        }

        let Some(time_idx) = lines.iter().position(|line| is_time_line(line)) else {
            continue;
        };
        let Some((start_ms, end_ms)) = parse_time_line(lines[time_idx]) else {
            continue;
        };
        if !start_ms.is_finite() || !end_ms.is_finite() || end_ms < start_ms {
            continue;
        }

        let text = lines[time_idx + 1..].join("\n");
        let text = text.trim();
        if text.is_empty() {
            continue;
        }

        cues.push(SubtitleCue {
            start_ms,
            end_ms,
            text: text.to_string(),
        });
    }

    cues.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
    cues
}

fn is_time_line(line: &str) -> bool {
    line.contains("-->")
        || line.contains("—>")
        || line.contains("–>")
        || line.contains("->")
}

fn parse_time_line(line: &str) -> Option<(f64, f64)> {
    for arrow in ["-->", "—>", "–>", "->"] {
        if let Some((left, right)) = line.split_once(arrow) {
            let start = parse_timecode(left)?;
            let end = parse_timecode(right)?;
            return Some((start, end));
        }
    }
    None
}

fn parse_timecode(raw: &str) -> Option<f64> {
    let clean = raw.trim().replace(',', ".");
    let parts: Vec<&str> = clean.split(':').collect();
    let (hours, minutes, seconds) = match parts.as_slice() {
        [hours, minutes, seconds] => (
            hours.parse::<f64>().ok()?,
            minutes.parse::<f64>().ok()?,
            seconds.parse::<f64>().ok()?,
        ),
        [minutes, seconds] => (0.0, minutes.parse::<f64>().ok()?, seconds.parse::<f64>().ok()?),
        _ => return None,
    };
    if !hours.is_finite()
        || !minutes.is_finite()
        || !seconds.is_finite()
        || hours < 0.0
        || minutes < 0.0
        || seconds < 0.0
    {
        return None;
    }
    Some((hours * 3600.0 + minutes * 60.0 + seconds) * 1000.0)
}

#[cfg(test)]
mod tests {
    use super::parse_srt;

    #[test]
    fn parses_basic_srt_with_bom_and_multiline_text() {
        let raw = "\u{feff}1\r\n00:00:00,024 --> 00:00:01,700\r\nAlright, cutie...\r\n\r\n2\r\n00:00:01,700 --> 00:00:03,150\r\nThink you can\r\nkeep up?\r\n";
        let cues = parse_srt(raw);

        assert_eq!(cues.len(), 2);
        assert!((cues[0].start_ms - 24.0).abs() < 0.001);
        assert!((cues[0].end_ms - 1700.0).abs() < 0.001);
        assert_eq!(cues[0].text, "Alright, cutie...");
        assert_eq!(cues[1].text, "Think you can\nkeep up?");
        assert!((cues[1].end_ms - 3150.0).abs() < 0.001);
    }

    #[test]
    fn skips_malformed_blocks_and_supports_short_seconds() {
        let raw = "1\n00:00:03,720 --> 00:00:06,240\nYou're kind of a show-off, huh?\n\nbroken\n\n2\n00:00:7,150 --> 00:00:8,000\nShort seconds\n";
        let cues = parse_srt(raw);

        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "You're kind of a show-off, huh?");
        assert!((cues[1].start_ms - 7150.0).abs() < 0.001);
    }

    #[test]
    fn sorts_out_of_order_cues() {
        let raw = "1\n00:00:05,000 --> 00:00:06,000\nSecond\n\n2\n00:00:01,000 --> 00:00:02,000\nFirst\n";
        let cues = parse_srt(raw);

        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "First");
        assert_eq!(cues[1].text, "Second");
    }
}
