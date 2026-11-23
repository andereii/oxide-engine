use std::collections::HashMap;

use bevy::prelude::*;
use bevy::sprite::Anchor;
use bevy::text::FontSource;

use crate::mods::ModRegistry;
use crate::play::camera::hud::{hud_render_layer, overlay_render_layer};
use crate::play::stage::sync::DynamicScriptText;
use crate::scripting::ScriptCommand;
use crate::utils::constants::FONT_VCR_OSD_MONO;

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

fn resolve_text_font(raw_font: &str, mods: &ModRegistry, asset_server: &AssetServer) -> FontSource {
    let trimmed = raw_font.trim().trim_start_matches('/');
    let rel = if trimmed.is_empty()
        || trimmed.eq_ignore_ascii_case("vcr.ttf")
        || trimmed.eq_ignore_ascii_case("vcr")
    {
        FONT_VCR_OSD_MONO.to_string()
    } else if let Some(found) = [
        trimmed.to_string(),
        format!("ui/fonts/{trimmed}"),
        format!("fonts/{trimmed}"),
    ]
    .into_iter()
    .find(|cand| mods.file_exists(cand))
    {
        found
    } else {
        FONT_VCR_OSD_MONO.to_string()
    };
    let asset_path = mods.resolve_asset_server_path(&rel).unwrap_or(rel);
    asset_server.load::<Font>(asset_path).into()
}

pub(super) fn handle_text_cmd(
    cmd: &ScriptCommand,
    commands: &mut Commands,
    asset_server: &AssetServer,
    mods: &ModRegistry,
    texts: &mut TextsQuery,
    pending_texts: &mut HashMap<String, Entity>,
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
            if let Some(prev_ent) = pending_texts.remove(id) {
                commands.entity(prev_ent).despawn();
            }
            for (ent, prev, _, _, _, _, _) in texts.iter() {
                if prev.id == *id {
                    commands.entity(ent).despawn();
                }
            }
            let anchor = parse_text_anchor(align);
            let font_source = resolve_text_font(font.as_deref().unwrap_or(""), mods, asset_server);
            let mut ec = commands.spawn((
                Text2d::new(text.clone()),
                TextFont {
                    font: font_source,
                    font_size: FontSize::Px(*size),
                    ..default()
                },
                TextColor(Color::srgba(color[0], color[1], color[2], color[3])),
                bevy::sprite::Text2dShadow {
                    offset: Vec2::new(2.0, -2.0),
                    color: Color::srgba(0.0, 0.0, 0.0, color[3]),
                },
                anchor,
                Transform::from_xyz(0.0, 0.0, *z),
                Visibility::Visible,
                DynamicScriptText {
                    id: id.clone(),
                    hud: *hud,
                    overlay: false,
                    base_position: *pos,
                    scroll_factor: if *hud { [0.0, 0.0] } else { [1.0, 1.0] },
                    z_layer: *z,
                    visible: true,
                },
            ));
            if *hud {
                ec.insert(hud_render_layer());
            }
            pending_texts.insert(id.clone(), ec.id());
        }
        ScriptCommand::SetText { id, text } => {
            let mut matched = false;
            for (_, txt, mut t2d, _, _, _, _) in texts.iter_mut() {
                if txt.id == *id {
                    **t2d = text.clone();
                    matched = true;
                }
            }
            if !matched && let Some(&ent) = pending_texts.get(id) {
                let next_text = text.clone();
                commands.queue(move |world: &mut World| {
                    if let Some(mut t2d) = world.get_mut::<Text2d>(ent) {
                        **t2d = next_text;
                    }
                });
            }
        }
        ScriptCommand::SetTextStyle {
            id,
            size,
            color,
            align,
        } => {
            let next_anchor = align.as_deref().map(parse_text_anchor);
            let next_size = *size;
            let next_color = *color;
            let mut matched = false;
            for (_, txt, _, mut font, mut tc, mut anchor, shadow_opt) in texts.iter_mut() {
                if txt.id == *id {
                    if let Some(sz) = next_size {
                        font.font_size = FontSize::Px(sz);
                    }
                    if let Some(c) = next_color {
                        tc.0 = Color::srgba(c[0], c[1], c[2], c[3]);
                        if let Some(mut shadow) = shadow_opt {
                            shadow.color = Color::srgba(0.0, 0.0, 0.0, c[3]);
                        }
                    }
                    if let Some(a) = next_anchor {
                        *anchor = a;
                    }
                    matched = true;
                }
            }
            if !matched && let Some(&ent) = pending_texts.get(id) {
                commands.queue(move |world: &mut World| {
                    if let Some(sz) = next_size
                        && let Some(mut font) = world.get_mut::<TextFont>(ent)
                    {
                        font.font_size = FontSize::Px(sz);
                    }
                    if let Some(c) = next_color {
                        if let Some(mut tc) = world.get_mut::<TextColor>(ent) {
                            tc.0 = Color::srgba(c[0], c[1], c[2], c[3]);
                        }
                        if let Some(mut shadow) = world.get_mut::<bevy::sprite::Text2dShadow>(ent) {
                            shadow.color = Color::srgba(0.0, 0.0, 0.0, c[3]);
                        }
                    }
                    if let Some(a) = next_anchor
                        && let Some(mut anchor) = world.get_mut::<Anchor>(ent)
                    {
                        *anchor = a;
                    }
                });
            }
        }
        ScriptCommand::SetPropOverlay { id, overlay } => {
            let ov = *overlay;
            let mut matched = false;
            for (ent, mut txt, _, _, _, _, _) in texts.iter_mut() {
                if txt.id == *id {
                    txt.overlay = ov;
                    let layer = if ov {
                        overlay_render_layer()
                    } else if txt.hud {
                        hud_render_layer()
                    } else {
                        bevy::camera::visibility::RenderLayers::layer(0)
                    };
                    commands.entity(ent).insert(layer);
                    matched = true;
                }
            }
            if !matched && let Some(&ent) = pending_texts.get(id) {
                commands.queue(move |world: &mut World| {
                    let Some(is_hud) = world.get::<DynamicScriptText>(ent).map(|t| t.hud) else {
                        return;
                    };
                    if let Some(mut txt) = world.get_mut::<DynamicScriptText>(ent) {
                        txt.overlay = ov;
                    }
                    let layer = if ov {
                        overlay_render_layer()
                    } else if is_hud {
                        hud_render_layer()
                    } else {
                        bevy::camera::visibility::RenderLayers::layer(0)
                    };
                    world.entity_mut(ent).insert(layer);
                });
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::play::camera::hud::OVERLAY_RENDER_LAYER;
    use bevy::camera::visibility::RenderLayers;

    #[test]
    fn same_batch_overlay_and_style_apply_to_newly_spawned_text() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Font>();
        let cmds = vec![
            ScriptCommand::SpawnText {
                id: "skipText".to_string(),
                text: "Skip [ Z ]".to_string(),
                pos: [936.0, 618.0],
                z: 950.0,
                size: 40.0,
                color: [1.0, 1.0, 1.0, 0.0],
                font: Some("vcr.ttf".to_string()),
                align: "top_left".to_string(),
                hud: true,
            },
            ScriptCommand::SetPropOverlay {
                id: "skipText".to_string(),
                overlay: true,
            },
            ScriptCommand::SetTextStyle {
                id: "skipText".to_string(),
                size: Some(40.0),
                color: Some([1.0, 1.0, 1.0, 1.0]),
                align: Some("top_left".to_string()),
            },
        ];

        app.world_mut().insert_resource(ModRegistry::default());
        let asset_server = app.world().resource::<AssetServer>().clone();
        app.add_systems(
            Update,
            move |mut commands: Commands, mut texts: TextsQuery, mods: Res<ModRegistry>| {
                let mut pending = HashMap::new();
                for cmd in &cmds {
                    handle_text_cmd(
                        cmd,
                        &mut commands,
                        &asset_server,
                        &mods,
                        &mut texts,
                        &mut pending,
                    );
                }
            },
        );
        app.update();

        let mut q = app.world_mut().query::<(
            &DynamicScriptText,
            &Text2d,
            &TextColor,
            &bevy::sprite::Text2dShadow,
            &RenderLayers,
        )>();
        let results: Vec<_> = q.iter(app.world()).collect();
        assert_eq!(results.len(), 1);
        let (txt, t2d, tc, shadow, layers) = results[0];
        assert_eq!(txt.id, "skipText");
        assert!(txt.overlay);
        assert_eq!(**t2d, "Skip [ Z ]");
        assert!((tc.0.alpha() - 1.0).abs() < 1e-5);
        assert!((shadow.color.alpha() - 1.0).abs() < 1e-5);
        assert_eq!(*layers, RenderLayers::layer(OVERLAY_RENDER_LAYER));
    }
}
