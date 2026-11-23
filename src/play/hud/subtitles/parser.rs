use super::config::{SubtitleConfig, SubtitleHighlightMode};

/// One timed word or syllable span inside a [`SubtitleCue`] (from Enhanced LRC `<mm:ss.xx>` tags).
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleSegment {
    pub start_ms: f64,
    pub end_ms: f64,
    pub text: String,
}

/// One parsed subtitle/lyric cue, with times relative to the start of its track.
#[derive(Debug, Clone, PartialEq)]
pub struct SubtitleCue {
    pub start_ms: f64,
    pub end_ms: f64,
    pub text: String,
    /// Optional word/syllable-level timing spans (Enhanced LRC). Empty for plain SRT/LRC.
    pub segments: Vec<SubtitleSegment>,
}

/// Strips inline HTML/XML tags (`<...>`) and ASS override blocks (`{...}`)
/// emitted when extracting subtitles from MKV/ASS streams or styled SRT tracks.
pub(crate) fn strip_subtitle_markup(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut in_angle = false;
    let mut in_brace = false;
    for ch in raw.chars() {
        match ch {
            '<' if !in_brace => in_angle = true,
            '>' if in_angle => in_angle = false,
            '{' if !in_angle => in_brace = true,
            '}' if in_brace => in_brace = false,
            _ if !in_angle && !in_brace => out.push(ch),
            _ => {}
        }
    }
    out.trim().to_string()
}

/// Parses SRT text into timed cues. Unknown or malformed blocks are skipped rather than
/// rejecting the whole track. Also recognizes optional inline Enhanced LRC `<mm:ss.xx>` tags.
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

        let raw_body = lines[time_idx + 1..].join("\n");
        let (text, segments, _) = parse_inline_enhanced(&raw_body, start_ms, Some(end_ms), 0.0);
        let text = if segments.is_empty() {
            strip_subtitle_markup(&raw_body)
        } else {
            text
        };
        if text.is_empty() {
            continue;
        }

        cues.push(SubtitleCue {
            start_ms,
            end_ms,
            text,
            segments,
        });
    }

    cues.sort_by(|a, b| a.start_ms.total_cmp(&b.start_ms));
    cues
}

/// Parses standard LRC (`[mm:ss.xx] line`) and Enhanced LRC (`[mm:ss.xx] <mm:ss.xx>word ...`).
///
/// Bar termination rules:
/// - A trailing `<mm:ss.xx>` tag at the end of a line explicitly closes that bar at that time.
/// - An empty timestamped line (e.g. `[00:15.50]`) acts as an explicit end marker for the previous
///   bar so it does not linger until the next lyric.
/// - Otherwise a bar ends at `next_bar.start_ms - lrc_end_gap_ms`, optionally capped by
///   `max_line_duration_ms`.
/// - Supports `[offset:+/-ms]` metadata tags and multiple `[mm:ss.xx]` tags per line.
pub fn parse_lrc(raw: &str, config: &SubtitleConfig) -> Vec<SubtitleCue> {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    let normalized = normalized.trim_start_matches('\u{feff}');

    let mut offset_ms = 0.0_f64;
    let mut raw_entries: Vec<(f64, String)> = Vec::new();

    for line in normalized.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Check [offset:...] metadata tag
        if let Some(inner) = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            && let Some((key, val)) = inner.split_once(':')
            && key.trim().eq_ignore_ascii_case("offset")
        {
            if let Ok(ms) = val.trim().parse::<f64>()
                && ms.is_finite()
            {
                offset_ms = ms;
            }
            continue;
        }

        let mut rest = trimmed;
        let mut line_times: Vec<f64> = Vec::new();
        while let Some(after_open) = rest.strip_prefix('[') {
            let Some((tag, after_close)) = after_open.split_once(']') else {
                break;
            };
            if let Some(ms) = parse_timecode(tag) {
                line_times.push(ms);
                rest = after_close;
            } else {
                // Non-timestamp metadata tag like [ti:...], [ar:...], [al:...]
                line_times.clear();
                break;
            }
        }

        if line_times.is_empty() {
            continue;
        }

        let body = rest.trim().to_string();
        for t in line_times {
            raw_entries.push((t, body.clone()));
        }
    }

    if raw_entries.is_empty() {
        return Vec::new();
    }

    // Apply LRC [offset:] (positive offset shifts lyrics earlier in standard LRC)
    for (t, _) in &mut raw_entries {
        *t = (*t - offset_ms).max(0.0);
    }
    raw_entries.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut cues: Vec<SubtitleCue> = Vec::new();
    let default_last_duration_ms = config.max_line_duration_ms.unwrap_or(5_000.0);

    for i in 0..raw_entries.len() {
        let (start_ms, ref body) = raw_entries[i];
        if body.is_empty() {
            // Explicit bar terminator for the previous cue
            continue;
        }

        let next_start_ms = raw_entries.get(i + 1).map(|(t, _)| *t);
        let (text, mut segments, trailing_end_ms) =
            parse_inline_enhanced(body, start_ms, next_start_ms, offset_ms);

        let clean_text = if segments.is_empty() {
            strip_subtitle_markup(body)
        } else {
            text
        };
        if clean_text.is_empty() {
            continue;
        }

        let mut end_ms = if let Some(explicit_end) = trailing_end_ms {
            explicit_end.max(start_ms)
        } else if let Some(next_t) = next_start_ms {
            let gap = if raw_entries[i + 1].1.is_empty() {
                0.0
            } else {
                config.lrc_end_gap_ms.max(0.0)
            };
            (next_t - gap).max(start_ms)
        } else {
            start_ms + default_last_duration_ms
        };

        if let Some(max_dur) = config.max_line_duration_ms
            && max_dur > 0.0
        {
            end_ms = end_ms.min(start_ms + max_dur);
        }

        if let Some(last_seg) = segments.last_mut()
            && last_seg.end_ms > end_ms
        {
            last_seg.end_ms = end_ms.max(last_seg.start_ms);
        }

        cues.push(SubtitleCue {
            start_ms,
            end_ms,
            text: clean_text,
            segments,
        });
    }

    cues
}

/// Auto-detects LRC vs SRT from file extension or content and parses accordingly.
pub fn parse_subtitles_auto(raw: &str, path_hint: Option<&str>, config: &SubtitleConfig) -> Vec<SubtitleCue> {
    let is_lrc_ext = path_hint
        .map(|p| p.trim().to_ascii_lowercase().ends_with(".lrc"))
        .unwrap_or(false);
    if is_lrc_ext {
        return parse_lrc(raw, config);
    }
    let is_srt_ext = path_hint
        .map(|p| p.trim().to_ascii_lowercase().ends_with(".srt"))
        .unwrap_or(false);
    if is_srt_ext {
        return apply_config_to_srt_cues(parse_srt(raw), config);
    }
    let srt_cues = parse_srt(raw);
    if !srt_cues.is_empty() {
        return apply_config_to_srt_cues(srt_cues, config);
    }
    parse_lrc(raw, config)
}

pub fn apply_config_to_srt_cues(mut cues: Vec<SubtitleCue>, config: &SubtitleConfig) -> Vec<SubtitleCue> {
    if let Some(max_dur) = config.max_line_duration_ms
        && max_dur > 0.0
    {
        for cue in &mut cues {
            cue.end_ms = cue.end_ms.min(cue.start_ms + max_dur);
        }
    }
    cues
}

/// Parses inline Enhanced LRC `<mm:ss.xx>` timestamps inside a single cue body.
/// Returns `(plain_text, segments, optional_trailing_end_ms)`.
fn parse_inline_enhanced(
    raw_body: &str,
    line_start_ms: f64,
    fallback_end_ms: Option<f64>,
    offset_ms: f64,
) -> (String, Vec<SubtitleSegment>, Option<f64>) {
    let bytes = raw_body.as_bytes();
    let mut i = 0;
    let mut found_any_tag = false;
    let mut raw_spans: Vec<(f64, String)> = Vec::new();
    let mut trailing_end_ms: Option<f64> = None;
    let mut current_time = line_start_ms;
    let mut buf = String::new();

    while i < bytes.len() {
        if bytes[i] == b'<'
            && let Some(rel_end) = raw_body[i + 1..].find('>')
        {
            let tag_inner = &raw_body[i + 1..i + 1 + rel_end];
            if let Some(tag_ms) = parse_timecode(tag_inner) {
                let adj_ms = (tag_ms - offset_ms).max(0.0);
                if !buf.is_empty() {
                    raw_spans.push((current_time, std::mem::take(&mut buf)));
                }
                found_any_tag = true;
                current_time = adj_ms;
                i += rel_end + 2;
                continue;
            }
        }
        if let Some(ch) = raw_body[i..].chars().next() {
            buf.push(ch);
            i += ch.len_utf8();
        } else {
            break;
        }
    }

    if !found_any_tag {
        return (String::new(), Vec::new(), None);
    }

    if !buf.is_empty() {
        raw_spans.push((current_time, buf));
    } else if !raw_spans.is_empty() {
        // Trailing `<mm:ss.xx>` with no text after it marks the explicit end of the bar.
        trailing_end_ms = Some(current_time);
    }

    let mut segments: Vec<SubtitleSegment> = Vec::with_capacity(raw_spans.len());
    for idx in 0..raw_spans.len() {
        let (seg_start, ref seg_text) = raw_spans[idx];
        let seg_end = if let Some((next_start, _)) = raw_spans.get(idx + 1) {
            (*next_start).max(seg_start)
        } else if let Some(t_end) = trailing_end_ms {
            t_end.max(seg_start)
        } else if let Some(fb) = fallback_end_ms {
            fb.max(seg_start)
        } else {
            seg_start + 500.0
        };
        segments.push(SubtitleSegment {
            start_ms: seg_start,
            end_ms: seg_end,
            text: seg_text.clone(),
        });
    }

    if let Some(first) = segments.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = segments.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    segments.retain(|s| !s.text.is_empty());

    let plain: String = segments.iter().map(|s| s.text.as_str()).collect();
    (plain, segments, trailing_end_ms)
}

/// Computes a smooth highlight factor `t ∈ [0.0, 1.0]` for every `char` in `cue.text` at `cur_ms`.
///
/// - `Line`: all chars are `1.0`.
/// - `Word`: uses Enhanced LRC `cue.segments` when present; otherwise distributes `[start_ms, end_ms]`
///   proportionally across words (fake word-by-word) and blends smoothly over `softness_ms`.
/// - `Letter`: uses Enhanced LRC `cue.segments` when present (sweeping letter-by-letter inside each
///   timed segment); otherwise sweeps smoothly across the full bar (fake letter-by-letter).
pub fn compute_char_progress(
    cue: &SubtitleCue,
    cur_ms: f64,
    mode: SubtitleHighlightMode,
    softness_ms: f64,
) -> Vec<f32> {
    let chars: Vec<char> = cue.text.chars().collect();
    let n = chars.len();
    if n == 0 {
        return Vec::new();
    }
    if mode == SubtitleHighlightMode::Line {
        return vec![1.0; n];
    }

    let soft = softness_ms.max(1.0);
    let mut out = vec![0.0_f32; n];

    // Enhanced LRC segments path
    if !cue.segments.is_empty() {
        let seg_char_total: usize = cue.segments.iter().map(|s| s.text.chars().count()).sum();
        if seg_char_total == n {
            let mut cursor = 0;
            for seg in &cue.segments {
                let len = seg.text.chars().count();
                let dur = (seg.end_ms - seg.start_ms).max(1.0);
                match mode {
                    SubtitleHighlightMode::Word => {
                        let w_t = smooth_step_01((cur_ms - seg.start_ms) / soft.min(dur));
                        for slot in &mut out[cursor..cursor + len] {
                            *slot = w_t;
                        }
                    }
                    SubtitleHighlightMode::Letter => {
                        let step = dur / (len.max(1) as f64);
                        for j in 0..len {
                            let c_start = seg.start_ms + step * (j as f64);
                            out[cursor + j] = smooth_step_01((cur_ms - c_start) / soft);
                        }
                    }
                    SubtitleHighlightMode::Line => {}
                }
                cursor += len;
            }
            return out;
        }
    }

    // Fake word-by-word or letter-by-letter path (plain SRT / plain LRC)
    let bar_dur = (cue.end_ms - cue.start_ms).max(1.0);
    match mode {
        SubtitleHighlightMode::Word => {
            // Split into word/whitespace runs so timing is weighted by character count
            let mut runs: Vec<(usize, usize, bool)> = Vec::new();
            let mut i = 0;
            while i < n {
                let is_ws = chars[i].is_whitespace();
                let start = i;
                while i < n && chars[i].is_whitespace() == is_ws {
                    i += 1;
                }
                runs.push((start, i, is_ws));
            }
            let total_word_chars: usize = runs
                .iter()
                .filter(|(_, _, ws)| !*ws)
                .map(|(a, b, _)| b - a)
                .sum::<usize>()
                .max(1);
            let mut acc_chars = 0usize;
            for (a, b, is_ws) in runs {
                let run_start_ms =
                    cue.start_ms + bar_dur * (acc_chars as f64 / total_word_chars as f64);
                let run_len = b - a;
                let run_dur = bar_dur * (run_len as f64 / total_word_chars as f64);
                let t = smooth_step_01((cur_ms - run_start_ms) / soft.min(run_dur.max(1.0)));
                for slot in &mut out[a..b] {
                    *slot = t;
                }
                if !is_ws {
                    acc_chars += run_len;
                }
            }
        }
        SubtitleHighlightMode::Letter => {
            let visible_count = chars.iter().filter(|c| !c.is_whitespace()).count().max(1);
            let step = bar_dur / (visible_count as f64);
            let mut vis_idx = 0usize;
            for (idx, ch) in chars.iter().enumerate() {
                let c_start = cue.start_ms + step * (vis_idx as f64);
                out[idx] = smooth_step_01((cur_ms - c_start) / soft);
                if !ch.is_whitespace() {
                    vis_idx += 1;
                }
            }
        }
        SubtitleHighlightMode::Line => {}
    }

    out
}

#[inline]
fn smooth_step_01(x: f64) -> f32 {
    let t = x.clamp(0.0, 1.0);
    (t * t * (3.0 - 2.0 * t)) as f32
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

pub(crate) fn parse_timecode(raw: &str) -> Option<f64> {
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
