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
    asset_server: &AssetServer,
    mods: &crate::mods::ModRegistry,
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
            let mut text_font = TextFont {
                font_size: FontSize::Px(*size),
                ..default()
            };
            if let Some(font_name) = font.as_deref()
                && !font_name.trim().is_empty()
            {
                let rel = if font_name.contains('/') {
                    font_name.to_string()
                } else {
                    format!("ui/fonts/{font_name}")
                };
                if let Some(srv) = mods.resolve_asset_server_path(&rel) {
                    let h: Handle<Font> = asset_server.load(srv);
                    text_font.font = h.into();
                }
            }
            let anchor = parse_text_anchor(align);
            let mut ec = commands.spawn((
                Text2d::new(text.clone()),
                text_font,
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
        ScriptCommand::SetTextFont { id, font } => {
            let rel = if font.contains('/') {
                font.clone()
            } else {
                format!("ui/fonts/{font}")
            };
            if let Some(srv) = mods.resolve_asset_server_path(&rel) {
                let handle: Handle<Font> = asset_server.load(srv);
                for (_, txt, _, mut tf, _, _) in texts.iter_mut() {
                    if txt.id == *id {
                        tf.font = handle.clone().into();
                    }
                }
            }
        }
        _ => {}
    }
}
