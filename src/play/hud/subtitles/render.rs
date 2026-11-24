use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::play::camera::hud::hud_render_layer;
use crate::play::scene::PlayState;
use crate::utils::constants::SCREEN_HEIGHT;

use super::config::{SubtitleConfig, SubtitleHighlightMode, SubtitleTemplate};
use super::parser::{
    SubtitleCue, SubtitleSubLine, compute_char_progress, compute_subline_char_progress,
};

// Bevy's default 2D orthographic frustum spans z = -1000..1000, so the native
// FlxSprite zIndex of 10000 cannot be used literally here.
const SUBTITLE_Z: f32 = 100.0;
const SHADOW_OFFSET: Vec2 = Vec2::new(2.0, -2.0);
/// Slots `0..CUE_RING_SLOTS` are persistently bound to `cue_idx % CUE_RING_SLOTS` so a cue never
/// hops between `Text2d` entities while scrolling (`next -> active -> prev -> exiting`).
/// Slot `IDLE_OR_OUTGOING_SLOT` is reserved for `idle_text` and BounceLetters outgoing fade.
/// Slots `SUB_LINE_SLOT_BASE..MAX_LINE_SLOTS` render secondary/duet sub-lines in `Line` mode.
const CUE_RING_SLOTS: usize = 5;
const IDLE_OR_OUTGOING_SLOT: usize = CUE_RING_SLOTS;
const SUB_LINE_SLOT_BASE: usize = IDLE_OR_OUTGOING_SLOT + 1;
const MAX_SUB_LINE_SLOTS: usize = 2;
const MAX_LINE_SLOTS: usize = SUB_LINE_SLOT_BASE + MAX_SUB_LINE_SLOTS;
const INITIAL_GLYPH_POOL: usize = 96;

#[derive(Component)]
pub struct HudSubtitleText;

#[derive(Component)]
pub struct HudSubtitleLineSlot {
    pub slot_idx: usize,
}

#[derive(Component)]
pub struct HudSubtitleGlyph {
    pub glyph_idx: usize,
}

#[derive(Resource, Default)]
pub struct SubtitleVisualPool {
    pub font: Handle<Font>,
    pub glyph_entities: Vec<Entity>,
}

/// Spawns the HUD subtitle visual pool (line slots + expandable per-letter glyphs).
pub fn spawn_subtitle_visuals(commands: &mut Commands, font: Handle<Font>) {
    let bottom = -SCREEN_HEIGHT * 0.5 + SubtitleConfig::default().bottom_margin;

    for slot_idx in 0..MAX_LINE_SLOTS {
        let mut entity_cmds = commands.spawn((
            Text2d::new(""),
            TextFont {
                font: font.clone().into(),
                font_size: FontSize::Px(SubtitleConfig::default().font_size),
                ..default()
            },
            TextColor(Color::WHITE),
            TextLayout::justify(Justify::Center),
            bevy::sprite::Text2dShadow {
                offset: SHADOW_OFFSET,
                color: Color::BLACK,
            },
            Anchor::CENTER,
            Transform::from_xyz(0.0, bottom, SUBTITLE_Z + slot_idx as f32 * 0.01),
            // Text2d entities spawned hidden do not get a usable initial text layout
            // in Bevy 0.20, so keep them visible and clear text when unused.
            Visibility::Visible,
            HudSubtitleLineSlot { slot_idx },
            hud_render_layer(),
        ));
        if slot_idx == 0 {
            entity_cmds.insert(HudSubtitleText);
        }
    }

    let mut pool = SubtitleVisualPool {
        font,
        glyph_entities: Vec::with_capacity(INITIAL_GLYPH_POOL),
    };
    ensure_glyph_pool(
        commands,
        &mut pool,
        INITIAL_GLYPH_POOL,
        SubtitleConfig::default().font_size,
    );
    commands.insert_resource(pool);
}

fn ensure_glyph_pool(
    commands: &mut Commands,
    pool: &mut SubtitleVisualPool,
    needed: usize,
    font_size: f32,
) {
    let base_y = -SCREEN_HEIGHT * 0.5 + SubtitleConfig::default().bottom_margin;
    while pool.glyph_entities.len() < needed {
        let glyph_idx = pool.glyph_entities.len();
        let ent = commands
            .spawn((
                Text2d::new(""),
                TextFont {
                    font: pool.font.clone().into(),
                    font_size: FontSize::Px(font_size),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::justify(Justify::Center),
                bevy::sprite::Text2dShadow {
                    offset: SHADOW_OFFSET,
                    color: Color::BLACK,
                },
                Anchor::CENTER,
                Transform::from_xyz(0.0, base_y, SUBTITLE_Z + 0.5 + glyph_idx as f32 * 0.0005),
                Visibility::Visible,
                HudSubtitleGlyph { glyph_idx },
                hud_render_layer(),
            ))
            .id();
        pool.glyph_entities.push(ent);
    }
}

#[inline]
fn lerp_rgba(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

#[inline]
fn rgba_to_color(c: [f32; 4]) -> Color {
    Color::srgba(
        c[0].clamp(0.0, 1.0),
        c[1].clamp(0.0, 1.0),
        c[2].clamp(0.0, 1.0),
        c[3].clamp(0.0, 1.0),
    )
}

#[inline]
fn scale_alpha(mut c: [f32; 4], factor: f32) -> [f32; 4] {
    c[3] = (c[3] * factor).clamp(0.0, 1.0);
    c
}

#[derive(Clone)]
struct LineRenderCmd {
    text: String,
    y: f32,
    scale: f32,
    color: [f32; 4],
}

#[derive(Clone)]
struct GlyphRenderCmd {
    ch: String,
    x: f32,
    y: f32,
    scale: f32,
    color: [f32; 4],
}

/// Layouts a string (supporting `\n`) into centered per-character `(x, y)` coordinates.
fn layout_text_glyphs(
    text: &str,
    center_y: f32,
    font_size: f32,
    letter_spacing: f32,
    scale: f32,
) -> Vec<(char, f32, f32)> {
    let lines: Vec<&str> = text.split('\n').collect();
    let line_count = lines.len().max(1);
    let line_h = font_size * 1.15 * scale;
    let char_adv = font_size * letter_spacing * scale;
    let total_h = (line_count.saturating_sub(1) as f32) * line_h;
    let mut out = Vec::with_capacity(text.chars().count());

    for (li, line) in lines.iter().enumerate() {
        let chars: Vec<char> = line.chars().collect();
        let line_w = (chars.len().saturating_sub(1) as f32) * char_adv;
        let y = center_y + total_h * 0.5 - (li as f32) * line_h;
        for (ci, ch) in chars.into_iter().enumerate() {
            let x = -line_w * 0.5 + (ci as f32) * char_adv;
            out.push((ch, x, y));
        }
    }
    out
}

fn build_highlighted_glyphs_raw(
    text: &str,
    progress: &[f32],
    center_y: f32,
    scale: f32,
    alpha_mul: f32,
    active_color: [f32; 4],
    inactive_color: [f32; 4],
    cfg: &SubtitleConfig,
    bar_elapsed_s: Option<f64>,
) -> Vec<GlyphRenderCmd> {
    let layout = layout_text_glyphs(
        text,
        center_y,
        cfg.font_size,
        cfg.letter_spacing,
        scale,
    );
    let enter_ease = cfg.enter_ease_kind();
    let enter_dur = cfg.enter_duration_s.max(0.001);

    let mut out = Vec::with_capacity(layout.len());
    let mut prog_idx = 0usize;
    let mut visible_idx = 0usize;

    for (ch, x, base_y) in layout {
        // Advance past newlines in `progress` (which indexes `text.chars()`)
        while prog_idx < progress.len() && text.chars().nth(prog_idx) == Some('\n') {
            prog_idx += 1;
        }
        let p = progress.get(prog_idx).copied().unwrap_or(1.0);
        prog_idx += 1;

        if ch.is_whitespace() {
            continue;
        }

        let mut col = lerp_rgba(inactive_color, active_color, p);
        let mut y = base_y;

        if let Some(elapsed_s) = bar_elapsed_s {
            let delay = (visible_idx as f64) * cfg.letter_stagger_s;
            let raw_t = ((elapsed_s - delay) / enter_dur).clamp(0.0, 1.0);
            let eased_y = enter_ease.eval(raw_t) as f32;
            y = base_y + cfg.enter_offset_y * (1.0 - eased_y);
            // Smooth alpha fade-in slightly ahead of bounce landing
            let alpha_t =
                ((elapsed_s - delay) / (enter_dur * 0.45).max(0.02)).clamp(0.0, 1.0) as f32;
            col = scale_alpha(col, alpha_t);
        }

        col = scale_alpha(col, alpha_mul);
        out.push(GlyphRenderCmd {
            ch: ch.to_string(),
            x,
            y,
            scale,
            color: col,
        });
        visible_idx += 1;
    }

    out
}

fn build_highlighted_glyphs(
    cue: &SubtitleCue,
    cur_ms: f64,
    center_y: f32,
    scale: f32,
    alpha_mul: f32,
    cfg: &SubtitleConfig,
    bar_elapsed_s: Option<f64>,
) -> Vec<GlyphRenderCmd> {
    let progress = compute_char_progress(
        cue,
        cur_ms,
        cfg.highlight_mode,
        cfg.highlight_softness_ms,
    );
    build_highlighted_glyphs_raw(
        &cue.text,
        &progress,
        center_y,
        scale,
        alpha_mul,
        cfg.active_color,
        cfg.inactive_color,
        cfg,
        bar_elapsed_s,
    )
}

#[allow(clippy::too_many_arguments)]
fn append_active_sublines(
    sub_lines: &[SubtitleSubLine],
    cur_ms: f64,
    active_y: f32,
    active_scale: f32,
    cfg: &SubtitleConfig,
    bounce_mode: bool,
    slot_cmds: &mut [Option<LineRenderCmd>; MAX_LINE_SLOTS],
    glyph_cmds: &mut Vec<GlyphRenderCmd>,
) -> usize {
    const SUB_FADE_MS: f64 = 120.0;
    const SUB_END_HOLD_MS: f64 = 160.0;

    let sub_active_col = cfg.effective_sub_active_color();
    let sub_inactive_col = cfg.effective_sub_inactive_color();
    let sub_scale = active_scale * cfg.sub_line_scale;
    let mut visible_count = 0usize;

    for sub in sub_lines {
        if visible_count >= MAX_SUB_LINE_SLOTS {
            break;
        }
        // Show while active (or briefly fading out after end_ms while parent bar is still active)
        if cur_ms + SUB_FADE_MS < sub.start_ms || cur_ms > sub.end_ms + SUB_END_HOLD_MS {
            continue;
        }
        let fade_in = ((cur_ms - (sub.start_ms - SUB_FADE_MS * 0.5)) / SUB_FADE_MS).clamp(0.0, 1.0);
        let fade_out = if cur_ms > sub.end_ms {
            1.0 - ((cur_ms - sub.end_ms) / SUB_END_HOLD_MS).clamp(0.0, 1.0)
        } else {
            1.0
        };
        let alpha_mul = (fade_in * fade_out) as f32;
        if alpha_mul <= 0.001 {
            continue;
        }

        let sub_y = active_y - cfg.sub_line_spacing * ((visible_count + 1) as f32);
        let use_glyphs = bounce_mode || cfg.highlight_mode != SubtitleHighlightMode::Line;

        if use_glyphs {
            let progress = compute_subline_char_progress(
                sub,
                cur_ms,
                cfg.highlight_mode,
                cfg.highlight_softness_ms,
            );
            let sub_elapsed = if bounce_mode {
                Some(((cur_ms - sub.start_ms) / 1000.0).max(0.0))
            } else {
                None
            };
            glyph_cmds.extend(build_highlighted_glyphs_raw(
                &sub.text,
                &progress,
                sub_y,
                sub_scale,
                alpha_mul,
                sub_active_col,
                sub_inactive_col,
                cfg,
                sub_elapsed,
            ));
        } else {
            slot_cmds[SUB_LINE_SLOT_BASE + visible_count] = Some(LineRenderCmd {
                text: sub.text.clone(),
                y: sub_y,
                scale: sub_scale,
                color: scale_alpha(sub_active_col, alpha_mul),
            });
        }
        visible_count += 1;
    }

    visible_count
}

/// Applies the current subtitle/lyric state, template animations, and word/letter highlights
/// to the HUD subtitle entity pool.
#[allow(clippy::too_many_arguments)]
pub fn sync_subtitles(
    mut commands: Commands,
    state: Res<PlayState>,
    mut pool: Option<ResMut<SubtitleVisualPool>>,
    mut lines_q: Query<
        (
            &HudSubtitleLineSlot,
            &mut Text2d,
            &mut TextFont,
            &mut TextColor,
            &mut bevy::sprite::Text2dShadow,
            &mut Transform,
            &mut Visibility,
        ),
        Without<HudSubtitleGlyph>,
    >,
    mut glyphs_q: Query<
        (
            &HudSubtitleGlyph,
            &mut Text2d,
            &mut TextFont,
            &mut TextColor,
            &mut bevy::sprite::Text2dShadow,
            &mut Transform,
            &mut Visibility,
        ),
        Without<HudSubtitleLineSlot>,
    >,
) {
    let Some(ref mut pool) = pool else {
        return;
    };

    let cfg = &state.subtitle_config;
    let downscroll = state.player_strumline.is_downscroll;
    let anchor_y = if downscroll {
        SCREEN_HEIGHT * 0.5 - cfg.bottom_margin
    } else {
        -SCREEN_HEIGHT * 0.5 + cfg.bottom_margin
    };

    let mut slot_cmds: [Option<LineRenderCmd>; MAX_LINE_SLOTS] = Default::default();
    let mut glyph_cmds: Vec<GlyphRenderCmd> = Vec::new();

    let active_idx = state.subtitle_active_index;
    let active_cue = active_idx.and_then(|i| state.subtitle_cues.get(i));
    let idle_fade = (state.subtitle_idle_elapsed_s / 0.22).clamp(0.0, 1.0) as f32;

    match cfg.template {
        SubtitleTemplate::Simple => {
            if let (Some(idx), Some(cue)) = (active_idx, active_cue) {
                if cfg.highlight_mode == SubtitleHighlightMode::Line {
                    slot_cmds[idx % CUE_RING_SLOTS] = Some(LineRenderCmd {
                        text: cue.text.clone(),
                        y: anchor_y,
                        scale: cfg.active_scale,
                        color: cfg.active_color,
                    });
                } else {
                    glyph_cmds = build_highlighted_glyphs(
                        cue,
                        state.subtitle_time_ms,
                        anchor_y,
                        cfg.active_scale,
                        1.0,
                        cfg,
                        None,
                    );
                }
                append_active_sublines(
                    &cue.sub_lines,
                    state.subtitle_time_ms,
                    anchor_y,
                    cfg.active_scale,
                    cfg,
                    false,
                    &mut slot_cmds,
                    &mut glyph_cmds,
                );
            } else if !state.subtitle_text.is_empty() {
                slot_cmds[IDLE_OR_OUTGOING_SLOT] = Some(LineRenderCmd {
                    text: state.subtitle_text.clone(),
                    y: anchor_y,
                    scale: cfg.inactive_scale,
                    color: scale_alpha(cfg.inactive_color, idle_fade),
                });
            }
        }
        SubtitleTemplate::ScrollThreeLine => {
            let ease = cfg.transition_ease_kind();
            let raw_t = if cfg.transition_duration_s > 0.0 {
                (state.subtitle_transition_elapsed_s / cfg.transition_duration_s).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let t = ease.eval(raw_t) as f32;
            let slide = 1.0 - t;
            let spacing = cfg.line_spacing;
            let past_col = cfg.effective_past_color();
            let next_col = cfg.effective_upcoming_color();
            let max_lines = cfg.max_lines.max(1);

            // 1) Exiting oldest line (only while actively sliding out of the top of the canvas)
            if max_lines >= 3
                && slide > 0.001
                && let Some(exiting_idx) = state.subtitle_exiting_index
                && Some(exiting_idx) != state.subtitle_prev_index
                && Some(exiting_idx) != active_idx
                && let Some(cue) = state.subtitle_cues.get(exiting_idx)
            {
                let y = anchor_y + spacing * (1.0 + t);
                let col = scale_alpha(past_col, 1.0 - t);
                slot_cmds[exiting_idx % CUE_RING_SLOTS] = Some(LineRenderCmd {
                    text: cue.text.clone(),
                    y,
                    scale: cfg.inactive_scale,
                    color: col,
                });
            }

            // 2) Previous line (above current). Only animates from center->top if it just left active;
            // if it was already resting in the previous slot during a gap, keep it completely stable.
            if max_lines >= 2
                && let Some(prev_idx) = state.subtitle_prev_index
                && Some(prev_idx) != active_idx
                && let Some(cue) = state.subtitle_cues.get(prev_idx)
            {
                let (y, col, sc) = if state.subtitle_prev_was_active {
                    (
                        anchor_y + spacing * t,
                        lerp_rgba(cfg.active_color, past_col, t),
                        cfg.active_scale + (cfg.inactive_scale - cfg.active_scale) * t,
                    )
                } else {
                    (anchor_y + spacing, past_col, cfg.inactive_scale)
                };
                slot_cmds[prev_idx % CUE_RING_SLOTS] = Some(LineRenderCmd {
                    text: cue.text.clone(),
                    y,
                    scale: sc,
                    color: col,
                });
            }

            // 3) Current active line (center) + optional duet/backing-vocal sub-lines underneath
            let mut sub_push_down = 0.0_f32;
            if let (Some(idx), Some(cue)) = (active_idx, active_cue) {
                let (y, sc, line_col) = if state.subtitle_active_was_next {
                    (
                        anchor_y - spacing * slide,
                        cfg.inactive_scale + (cfg.active_scale - cfg.inactive_scale) * t,
                        lerp_rgba(next_col, cfg.active_color, t),
                    )
                } else {
                    (anchor_y, cfg.active_scale, cfg.active_color)
                };
                if cfg.highlight_mode == SubtitleHighlightMode::Line {
                    slot_cmds[idx % CUE_RING_SLOTS] = Some(LineRenderCmd {
                        text: cue.text.clone(),
                        y,
                        scale: sc,
                        color: line_col,
                    });
                } else {
                    // Keep the cue's ring slot primed with invisible text so moving active -> prev
                    // never triggers a 1-frame Bevy Text2d layout hitch.
                    slot_cmds[idx % CUE_RING_SLOTS] = Some(LineRenderCmd {
                        text: cue.text.clone(),
                        y,
                        scale: sc,
                        color: scale_alpha(cfg.active_color, 0.0),
                    });
                    glyph_cmds = build_highlighted_glyphs(
                        cue,
                        state.subtitle_time_ms,
                        y,
                        sc,
                        1.0,
                        cfg,
                        None,
                    );
                }
                let sub_count = append_active_sublines(
                    &cue.sub_lines,
                    state.subtitle_time_ms,
                    y,
                    sc,
                    cfg,
                    false,
                    &mut slot_cmds,
                    &mut glyph_cmds,
                );
                if sub_count > 0 {
                    sub_push_down = cfg.sub_line_spacing * (sub_count as f32);
                }
            } else if !state.subtitle_text.is_empty() {
                slot_cmds[IDLE_OR_OUTGOING_SLOT] = Some(LineRenderCmd {
                    text: state.subtitle_text.clone(),
                    y: anchor_y,
                    scale: cfg.inactive_scale,
                    color: scale_alpha(cfg.inactive_color, idle_fade),
                });
            }

            // 4) Upcoming next line (below current). Slides in from below only when a new active bar
            // just took the center slot; stays stable when entering/waiting in a gap.
            if max_lines >= 3
                && let Some(next_idx) = state.subtitle_next_index
                && Some(next_idx) != active_idx
                && let Some(cue) = state.subtitle_cues.get(next_idx)
            {
                let (y, col) = if state.subtitle_active_was_next {
                    (
                        anchor_y - spacing * (1.0 + slide) - sub_push_down,
                        scale_alpha(next_col, t),
                    )
                } else {
                    (anchor_y - spacing - sub_push_down, next_col)
                };
                slot_cmds[next_idx % CUE_RING_SLOTS] = Some(LineRenderCmd {
                    text: cue.text.clone(),
                    y,
                    scale: cfg.inactive_scale,
                    color: col,
                });
            }
        }
        SubtitleTemplate::BounceLetters => {
            // Outgoing line fading out smoothly
            if !state.subtitle_outgoing_text.is_empty() && cfg.exit_duration_s > 0.0 {
                let raw_exit = (state.subtitle_outgoing_elapsed_s / cfg.exit_duration_s).clamp(0.0, 1.0);
                if raw_exit < 1.0 {
                    let exit_t = cfg.exit_ease_kind().eval(raw_exit) as f32;
                    let col = scale_alpha(cfg.active_color, 1.0 - exit_t);
                    slot_cmds[IDLE_OR_OUTGOING_SLOT] = Some(LineRenderCmd {
                        text: state.subtitle_outgoing_text.clone(),
                        y: anchor_y + cfg.exit_offset_y * exit_t,
                        scale: cfg.active_scale,
                        color: col,
                    });
                }
            }

            // Incoming active bar: independent per-letter bottom-to-top bounce + optional highlight
            if let Some(cue) = active_cue {
                glyph_cmds = build_highlighted_glyphs(
                    cue,
                    state.subtitle_time_ms,
                    anchor_y,
                    cfg.active_scale,
                    1.0,
                    cfg,
                    Some(state.subtitle_bar_elapsed_s),
                );
                append_active_sublines(
                    &cue.sub_lines,
                    state.subtitle_time_ms,
                    anchor_y,
                    cfg.active_scale,
                    cfg,
                    true,
                    &mut slot_cmds,
                    &mut glyph_cmds,
                );
            } else if !state.subtitle_text.is_empty() && slot_cmds[IDLE_OR_OUTGOING_SLOT].is_none() {
                slot_cmds[IDLE_OR_OUTGOING_SLOT] = Some(LineRenderCmd {
                    text: state.subtitle_text.clone(),
                    y: anchor_y,
                    scale: cfg.inactive_scale,
                    color: scale_alpha(cfg.inactive_color, idle_fade),
                });
            }
        }
    }

    if glyph_cmds.len() > pool.glyph_entities.len() {
        ensure_glyph_pool(&mut commands, pool, glyph_cmds.len(), cfg.font_size);
    }

    // Apply persistent line slots
    for (slot, mut text2d, mut font, mut color, mut shadow, mut tf, mut vis) in &mut lines_q {
        if let Some(cmd) = slot_cmds.get(slot.slot_idx).and_then(Option::as_ref) {
            if text2d.0 != cmd.text {
                **text2d = cmd.text.clone();
            }
            font.font_size = FontSize::Px(cfg.font_size);
            color.0 = rgba_to_color(cmd.color);
            shadow.color = Color::srgba(0.0, 0.0, 0.0, cmd.color[3].clamp(0.0, 1.0));
            tf.translation.x = 0.0;
            tf.translation.y = cmd.y;
            tf.scale = Vec3::splat(cmd.scale);
        } else {
            if !text2d.0.is_empty() {
                text2d.0.clear();
            }
            color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
            shadow.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
        *vis = Visibility::Visible;
    }

    // Apply per-letter glyphs
    for (glyph, mut text2d, mut font, mut color, mut shadow, mut tf, mut vis) in &mut glyphs_q {
        if let Some(cmd) = glyph_cmds.get(glyph.glyph_idx) {
            if text2d.0 != cmd.ch {
                **text2d = cmd.ch.clone();
            }
            font.font_size = FontSize::Px(cfg.font_size);
            color.0 = rgba_to_color(cmd.color);
            shadow.color = Color::srgba(0.0, 0.0, 0.0, cmd.color[3].clamp(0.0, 1.0));
            tf.translation.x = cmd.x;
            tf.translation.y = cmd.y;
            tf.scale = Vec3::splat(cmd.scale);
        } else {
            if !text2d.0.is_empty() {
                text2d.0.clear();
            }
            color.0 = Color::srgba(1.0, 1.0, 1.0, 0.0);
            shadow.color = Color::srgba(0.0, 0.0, 0.0, 0.0);
        }
        *vis = Visibility::Visible;
    }
}
