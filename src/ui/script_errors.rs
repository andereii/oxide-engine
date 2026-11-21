//! On-screen script failures for modders: orange monospace text, file and line.

use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::text::{LineBreak, TextLayout};

use crate::mods::ModRegistry;
use crate::scripting::ScriptHost;
use crate::scripting::diagnostics::{
    SCRIPT_ERROR_HINT, ScriptErrorAction, ScriptErrorView, format_script_error_banner,
    format_script_error_log, step_script_error_view,
};

pub(crate) const SCRIPT_ERROR_FONT_PATH: &str = "ui/fonts/Inconsolata Regular.ttf";
pub(crate) const SCRIPT_ERROR_FONT_SIZE: f32 = 11.0;
pub(crate) const SCRIPT_ERROR_RGB: [f32; 3] = [1.0, 0.533, 0.0];

const SCRIPT_ERROR_LAYER: usize = 31;
const SCRIPT_ERROR_CAMERA_ORDER: isize = 110;

#[derive(Component)]
pub(crate) struct ScriptErrorCamera;

#[derive(Component)]
pub(crate) struct ScriptErrorPanel;

#[derive(Component)]
pub(crate) struct ScriptErrorBody;

#[derive(Resource)]
pub(crate) struct ScriptErrorOverlay {
    pub(crate) view: ScriptErrorView,
    pub(crate) swallow_note_s: bool,
    pub(crate) swallow_backspace: bool,
}

impl Default for ScriptErrorOverlay {
    fn default() -> Self {
        Self {
            view: ScriptErrorView::default(),
            swallow_note_s: false,
            swallow_backspace: false,
        }
    }
}

pub(crate) fn spawn_script_error_overlay(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mods: Res<ModRegistry>,
) {
    let layer = RenderLayers::layer(SCRIPT_ERROR_LAYER);
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: SCRIPT_ERROR_CAMERA_ORDER,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Msaa::Off,
            layer.clone(),
            ScriptErrorCamera,
        ))
        .id();

    let font_path = mods
        .resolve_asset_server_path(SCRIPT_ERROR_FONT_PATH)
        .unwrap_or_else(|| SCRIPT_ERROR_FONT_PATH.to_string());
    let font = asset_server.load(font_path);
    let color = Color::srgb(SCRIPT_ERROR_RGB[0], SCRIPT_ERROR_RGB[1], SCRIPT_ERROR_RGB[2]);

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(16.0),
                width: Val::Px(640.0),
                max_width: Val::Percent(92.0),
                max_height: Val::Percent(70.0),
                padding: UiRect::all(Val::Px(8.0)),
                border: UiRect::all(Val::Px(1.0)),
                overflow: Overflow::scroll_y(),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.03, 0.0, 0.88)),
            BorderColor::all(color),
            UiTargetCamera(camera),
            layer,
            ScriptErrorPanel,
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Percent(100.0),
                    ..default()
                },
                Text::new(""),
                TextFont {
                    font: font.into(),
                    font_size: FontSize::Px(SCRIPT_ERROR_FONT_SIZE),
                    ..default()
                },
                TextColor(color),
                TextLayout {
                    linebreak: LineBreak::AnyCharacter,
                    ..default()
                },
                ScriptErrorBody,
            ));
        });
}

pub(crate) fn update_script_error_overlay(
    keys: Res<ButtonInput<KeyCode>>,
    host: Res<ScriptHost>,
    mut overlay: ResMut<ScriptErrorOverlay>,
    mut panels: Query<&mut Node, With<ScriptErrorPanel>>,
    mut bodies: Query<&mut Text, With<ScriptErrorBody>>,
    mut shown: Local<String>,
) {
    overlay.swallow_backspace = false;
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let toggle = shift && keys.just_pressed(KeyCode::KeyS);
    if toggle {
        overlay.swallow_note_s = true;
    }
    let dismiss = keys.just_pressed(KeyCode::Backspace) && overlay.view.panel_open();
    if dismiss {
        overlay.swallow_backspace = true;
    }
    let action = if dismiss {
        ScriptErrorAction::Dismiss
    } else if toggle {
        ScriptErrorAction::ToggleLog
    } else {
        ScriptErrorAction::None
    };
    let latest = host.diagnostics.last().map(|diag| diag.seq).unwrap_or(0);
    step_script_error_view(&mut overlay.view, host.log_epoch, latest, action);

    let open = overlay.view.panel_open();
    for mut node in &mut panels {
        node.display = if open { Display::Flex } else { Display::None };
    }
    if !open {
        return;
    }
    let body = if overlay.view.log_open {
        format_script_error_log(&host.diagnostics)
    } else {
        format_script_error_banner(&host.diagnostics)
    };
    let text = format!("{SCRIPT_ERROR_HINT}\n\n{body}");
    if *shown == text {
        return;
    }
    *shown = text.clone();
    for mut label in &mut bodies {
        **label = text.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_style_is_orange_mono_11() {
        assert_eq!(SCRIPT_ERROR_FONT_SIZE, 11.0);
        assert!(SCRIPT_ERROR_FONT_PATH.ends_with("Inconsolata Regular.ttf"));
        assert!(SCRIPT_ERROR_RGB[0] > 0.9);
        assert!(SCRIPT_ERROR_RGB[1] > 0.4 && SCRIPT_ERROR_RGB[1] < 0.7);
        assert!(SCRIPT_ERROR_RGB[2] < 0.05);
        assert!(SCRIPT_ERROR_HINT.contains("Backspace"));
        assert!(SCRIPT_ERROR_HINT.contains("Shift+S"));
    }
}
