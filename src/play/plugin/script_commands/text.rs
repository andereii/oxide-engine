use bevy::prelude::*;
use bevy::sprite::Anchor;

use crate::play::camera::hud::hud_render_layer;
use crate::play::stage::sync::DynamicScriptText;
use crate::scripting::ScriptCommand;

use super::TextsQuery;

fn parse_text_anchor(align: &str) -> Anchor {
    match align.trim().to_ascii_lowercase().as_str() {
        "center" | "middle" => Anchor::CENTER,
        "right" => Anchor::CENTER_RIGHT,
        "top_left" | "tl" => Anchor::TOP_LEFT,
        "top_center" | "tc" => Anchor::TOP_CENTER,
        "top_right" | "tr" => Anchor::TOP_RIGHT,
        "bottom_left" | "bl" => Anchor::BOTTOM_LEFT,
        "bottom_right" | "br" => Anchor::BOTTOM_RIGHT,
        _ => Anchor::CENTER_LEFT,
    }
}

pub(super) fn handle_text_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    texts: &mut TextsQuery,
) {
    match cmd {
        ScriptCommand::SpawnText {
            id,
            text,
            pos,
            z,
            size,
            color,
            font,
            align,
            hud,
        } => {
            for (ent, prev, _, _, _, _) in texts.iter() {
                if prev.id == *id {
                    commands.entity(ent).despawn();
                }
            }
            let _ = font;
            let anchor = parse_text_anchor(align);
            let mut ec = commands.spawn((
                Text2d::new(text.clone()),
                TextFont {
                    font_size: FontSize::Px(*size),
                    font_smoothing: bevy::text::FontSmoothing::None,
                    ..default()
                },
                TextColor(Color::srgba(color[0], color[1], color[2], color[3])),
                anchor,
                Transform::from_xyz(0.0, 0.0, *z),
                Visibility::Visible,
                DynamicScriptText {
                    id: id.clone(),
                    hud: *hud,
                    base_position: *pos,
                    scroll_factor: if *hud { [0.0, 0.0] } else { [1.0, 1.0] },
                    z_layer: *z,
                    visible: true,
                },
            ));
            if *hud {
                ec.insert(hud_render_layer());
            }
        }
        ScriptCommand::SetText { id, text } => {
            for (_, txt, mut t2d, _, _, _) in texts.iter_mut() {
                if txt.id == *id {
                    **t2d = text.clone();
                }
            }
        }
        ScriptCommand::SetTextStyle {
            id,
            size,
            color,
            align,
        } => {
            for (_, txt, _, mut font, mut tc, mut anchor) in texts.iter_mut() {
                if txt.id == *id {
                    if let Some(sz) = size {
                        font.font_size = FontSize::Px(*sz);
                    }
                    if let Some(c) = color {
                        tc.0 = Color::srgba(c[0], c[1], c[2], c[3]);
                    }
                    if let Some(al) = align {
                        *anchor = parse_text_anchor(al);
                    }
                }
            }
        }
        _ => {}
    }
}
