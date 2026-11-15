use bevy::prelude::*;
use bevy::time::Real;

const FPS_REFRESH_INTERVAL_S: f64 = 0.25;
const FPS_OVERLAY_Z_INDEX: i32 = i32::MAX - 32;

#[derive(Component)]
struct FpsCounterText;

#[derive(Default)]
struct FpsCounterState {
    frames: u32,
    elapsed_s: f64,
}

/// Spawns a top-left screen-space FPS overlay above gameplay and HUD layers.
pub(crate) fn spawn_fps_counter(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                top: Val::Px(12.0),
                padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            GlobalZIndex(FPS_OVERLAY_Z_INDEX),
            Pickable::IGNORE,
        ))
        .with_children(|parent| {
            parent.spawn((
                Text::new("FPS: --"),
                TextFont {
                    font_size: FontSize::Px(18.0),
                    ..default()
                },
                TextColor(Color::WHITE),
                TextShadow {
                    offset: Vec2::splat(1.5),
                    color: Color::srgba(0.0, 0.0, 0.0, 0.9),
                },
                FpsCounterText,
                Pickable::IGNORE,
            ));
        });
}

fn format_fps(fps: f64) -> String {
    if !fps.is_finite() {
        return "FPS: --".to_string();
    }
    format!("FPS: {:.0}", fps.round().max(0.0))
}

/// Updates the on-screen FPS counter over fixed wall-clock windows to avoid jitter and excess UI relayout.
pub(crate) fn update_fps_counter(
    time: Res<Time<Real>>,
    mut state: Local<FpsCounterState>,
    mut query: Query<&mut Text, With<FpsCounterText>>,
) {
    let delta_s = time.delta_secs_f64();
    if delta_s <= 0.0 {
        return;
    }

    state.frames = state.frames.saturating_add(1);
    state.elapsed_s += delta_s;
    if state.elapsed_s < FPS_REFRESH_INTERVAL_S {
        return;
    }

    let fps = f64::from(state.frames) / state.elapsed_s;
    let label = format_fps(fps);
    for mut text in &mut query {
        **text = label.clone();
    }
    state.frames = 0;
    state.elapsed_s = 0.0;
}

