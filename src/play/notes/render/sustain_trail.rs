use crate::utils::constants::{PIXELS_PER_MS, SUSTAIN_MIN_REMAINING_MS, SUSTAIN_MISSED_ALPHA};

use crate::play::notes::data::NoteSprite;
use crate::play::notes::skin::NoteStyle;
use crate::play::notes::strum::Strumline;

#[derive(Debug, Clone, Copy)]
pub struct SustainGeometry {
    pub visible: bool,
    pub body_y: f32,
    pub body_height_px: f32,
    pub end_x_offset: f32,
    pub end_y: f32,
    pub end_height_px: f32,
    pub alpha: f32,
    pub flip_y: bool,
}

pub fn sustain_height_px(length_ms: f64, scroll_speed: f64) -> f32 {
    (length_ms.max(0.0) * PIXELS_PER_MS * scroll_speed) as f32
}

pub fn compute_sustain_geometry(
    note: &NoteSprite,
    strum: &Strumline,
    style: &NoteStyle,
    vis_pos_ms: f64,
    end_raw_h: f32,
) -> SustainGeometry {
    let hidden = SustainGeometry {
        visible: false,
        body_y: -900.0,
        body_height_px: 0.0,
        end_x_offset: style.sustains_end_offset[0],
        end_y: -900.0,
        end_height_px: 0.0,
        alpha: style.sustains_alpha,
        flip_y: strum.is_downscroll,
    };

    if !note.is_hold_alive() {
        return hidden;
    }

    // In Funkin Strumline.hx: dropping an already-hit hold note hides it immediately (alpha = 0).
    if note.hold_hit && note.hold_dropped {
        return hidden;
    }

    let remaining_ms = if note.hold_hit && !note.hold_dropped {
        note.hold_remaining_ms
    } else if note.hold_dropped && note.hold_remaining_ms < note.data.sustain_length_ms {
        note.hold_remaining_ms
    } else {
        note.data.sustain_length_ms
    };

    if remaining_ms <= SUSTAIN_MIN_REMAINING_MS {
        return hidden;
    }

    let total_px = sustain_height_px(remaining_ms, strum.scroll_speed);
    if total_px <= 1.0 {
        return hidden;
    }

    let rendered_end_px = end_raw_h * style.sustains_scale;
    let body_px = (total_px - rendered_end_px).max(0.0);
    let end_height_px = if body_px > 0.5 { rendered_end_px } else { total_px.min(rendered_end_px).max(1.0) };
    let sign = if strum.is_downscroll { 1.0 } else { -1.0 };

    let start_y = if note.hold_hit && !note.hold_dropped {
        strum.receptor_y() + note.y_offset
    } else if note.hold_dropped && note.hold_remaining_ms < note.data.sustain_length_ms {
        let elapsed_held_px = sustain_height_px(
            note.data.sustain_length_ms - note.hold_remaining_ms,
            strum.scroll_speed,
        );
        strum.note_y_at(note.data.time_ms, vis_pos_ms) + note.y_offset + sign * elapsed_held_px
    } else {
        strum.note_y_at(note.data.time_ms, vis_pos_ms) + note.y_offset
    };

    let body_y = start_y + sign * (body_px * 0.5);
    let body_end_edge = start_y + sign * body_px;
    let anti_seam = if !style.is_pixel && body_px > 0.5 { -sign * 0.5 } else { 0.0 };
    let end_y = body_end_edge + sign * (end_height_px * 0.5) + anti_seam + sign * style.sustains_end_offset[1];

    let min_vis = body_y.min(end_y);
    let max_vis = body_y.max(end_y);
    if max_vis < -520.0 || min_vis > 520.0 {
        return hidden;
    }

    let alpha = if note.has_missed || note.hold_dropped {
        style.sustains_alpha * SUSTAIN_MISSED_ALPHA
    } else {
        style.sustains_alpha
    };

    SustainGeometry {
        visible: true,
        body_y,
        body_height_px: body_px,
        end_x_offset: style.sustains_end_offset[0],
        end_y,
        end_height_px,
        alpha,
        flip_y: strum.is_downscroll,
    }
}
