pub mod config;
pub mod parser;
pub mod render;

pub use config::{
    SubtitleConfig, SubtitleConfigPatch, SubtitleHighlightMode, SubtitleTemplate,
    parse_subtitle_color_str,
};
pub use parser::{SubtitleCue, parse_srt, parse_subtitles_auto};
pub use render::{spawn_subtitle_visuals, sync_subtitles};

#[cfg(test)]
mod tests {
    use super::parser::{compute_char_progress, parse_lrc};
    use super::*;

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

    #[test]
    fn strips_html_and_ass_tags_and_skips_empty_tag_cues() {
        let raw = "1\n00:00:00,270 --> 00:00:02,660\n<font face=\"VCR OSD Mono\" size=\"30\">{\\an1}*SCREAMS*</font>\n\n2\n00:00:03,000 --> 00:00:04,000\n<font face=\"VCR OSD Mono\" size=\"30\"></font>\n\n3\n00:00:05,000 --> 00:00:06,000\n<font face=\"VCR OSD Mono\" size=\"30\">Let's rock, <b><font color=\"#ff0000\">CUNT</font></b>!</font>\n";
        let cues = parse_srt(raw);

        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "*SCREAMS*");
        assert_eq!(cues[1].text, "Let's rock, CUNT!");
    }

    #[test]
    fn parses_standard_lrc_with_offset_empty_end_marker_and_max_duration() {
        let raw = "[ti:Test]\n[offset:500]\n[00:02.00]First bar\n[00:05.00]\n[00:10.00]Second bar\n[00:25.00]Third bar\n";
        let cfg = SubtitleConfig {
            max_line_duration_ms: Some(4_000.0),
            ..Default::default()
        };
        let cues = parse_lrc(raw, &cfg);

        assert_eq!(cues.len(), 3);
        // 2000 - 500 = 1500; empty line at 5000 - 500 = 4500 ends First bar
        assert!((cues[0].start_ms - 1_500.0).abs() < 0.01);
        assert!((cues[0].end_ms - 4_500.0).abs() < 0.01);
        assert_eq!(cues[0].text, "First bar");
        // Second bar starts at 9500, next is at 24500, capped by max_line_duration_ms (4000) -> 13500
        assert!((cues[1].start_ms - 9_500.0).abs() < 0.01);
        assert!((cues[1].end_ms - 13_500.0).abs() < 0.01);
        assert_eq!(cues[1].text, "Second bar");
    }

    #[test]
    fn parses_enhanced_lrc_word_segments_and_trailing_end_time() {
        let raw = "[00:10.00]<00:10.00>Hello <00:10.50>world<00:11.20>\n[00:15.00]Next line\n";
        let cues = parse_lrc(raw, &SubtitleConfig::default());

        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].text, "Hello world");
        assert!((cues[0].start_ms - 10_000.0).abs() < 0.01);
        assert!((cues[0].end_ms - 11_200.0).abs() < 0.01);
        assert_eq!(cues[0].segments.len(), 2);
        assert_eq!(cues[0].segments[0].text, "Hello ");
        assert!((cues[0].segments[0].end_ms - 10_500.0).abs() < 0.01);
        assert_eq!(cues[0].segments[1].text, "world");
        assert!((cues[0].segments[1].end_ms - 11_200.0).abs() < 0.01);
    }

    #[test]
    fn computes_fake_word_and_smooth_letter_progress() {
        let cue = SubtitleCue {
            start_ms: 1_000.0,
            end_ms: 2_000.0,
            text: "Hi you".to_string(),
            segments: Vec::new(),
            sub_lines: Vec::new(),
        };

        let word_mid = compute_char_progress(&cue, 1_200.0, SubtitleHighlightMode::Word, 80.0);
        assert_eq!(word_mid.len(), 6);
        assert!(word_mid[0] > 0.9, "first word should be active");
        assert!(word_mid[5] < 0.05, "second word should not have started yet");

        let letter_mid = compute_char_progress(&cue, 1_450.0, SubtitleHighlightMode::Letter, 100.0);
        assert!(letter_mid[0] > 0.99);
        assert!(letter_mid[3] > 0.0 && letter_mid[3] < 1.0, "middle letter should blend smoothly");
        assert!(letter_mid[5] < 0.05);
    }

    #[test]
    fn parses_dual_vocalist_and_background_vocal_sublines() {
        let raw = "\
[00:10.00]<00:10.00>Main <00:10.50>line<00:11.50>[bg:<00:10.20>(Echo <00:10.70>vocal)<00:11.40>]
[00:12.00]<00:12.00>First <00:12.50>singer<00:13.50>
[bg:<00:12.30>(Backing <00:12.90>harmony)<00:13.40>]
[00:14.00]v1:Lead vocal
[00:14.00]v2:(Duet partner)
[00:16.00]Early start bar<00:17.60>
[00:17.30]Next sequential bar<00:19.00>
";
        let cues = parse_lrc(raw, &SubtitleConfig::default());

        assert_eq!(cues.len(), 5);
        // 1) Inline [bg:...]
        assert_eq!(cues[0].text, "Main line");
        assert_eq!(cues[0].sub_lines.len(), 1);
        assert_eq!(cues[0].sub_lines[0].text, "(Echo vocal)");
        assert_eq!(cues[0].sub_lines[0].segments.len(), 2);
        assert!((cues[0].sub_lines[0].start_ms - 10_200.0).abs() < 0.01);

        // 2) Standalone [bg:...] on next line
        assert_eq!(cues[1].text, "First singer");
        assert_eq!(cues[1].sub_lines.len(), 1);
        assert_eq!(cues[1].sub_lines[0].text, "(Backing harmony)");

        // 3) Same-timestamp v1:/v2: duet
        assert_eq!(cues[2].text, "Lead vocal");
        assert_eq!(cues[2].sub_lines.len(), 1);
        assert_eq!(cues[2].sub_lines[0].text, "(Duet partner)");

        // 4) Non-contained overlapping sequential bars stay separate main cues
        assert_eq!(cues[3].text, "Early start bar");
        assert!(cues[3].sub_lines.is_empty());
        assert_eq!(cues[4].text, "Next sequential bar");
    }

    #[test]
    fn parses_config_patch_from_json_with_hex_colors_and_aliases() {
        let json: serde_json::Value = serde_json::json!({
            "template": "scroll",
            "highlight": "letter",
            "active_color": "#FFCC00FF",
            "inactive_color": "#44444480",
            "idle_text": "...",
            "gap_threshold_ms": 1200.0,
            "max_line_duration_ms": 4500.0,
            "enter_ease": "bounceOut",
            "letter_stagger_s": 0.03
        });
        let patch = SubtitleConfigPatch::from_json_value(&json);
        let mut cfg = SubtitleConfig::default();
        cfg.apply_patch(&patch);

        assert_eq!(cfg.template, SubtitleTemplate::ScrollThreeLine);
        assert_eq!(cfg.highlight_mode, SubtitleHighlightMode::Letter);
        assert_eq!(cfg.idle_text, "...");
        assert!((cfg.gap_threshold_ms - 1200.0).abs() < 0.01);
        assert_eq!(cfg.max_line_duration_ms, Some(4500.0));
        assert!((cfg.active_color[0] - 1.0).abs() < 0.01);
        assert!((cfg.inactive_color[3] - (128.0 / 255.0)).abs() < 0.02);
    }
}
