use bevy::prelude::*;
use bevy::ui::UiTransform;

use super::LoadingScreenState;
use super::components::{
    LOADING_OVERLAY_Z_INDEX, LoadingScreenRoot, LoadingScreenRuntime, LoadingThrobberVisual,
    THROBBER_ANGLE_SNAPS, THROBBER_ASSET_PATH, THROBBER_MARGIN_PX, THROBBER_SIZE_PX,
    THROBBER_STEPS_PER_SEC,
};

pub(super) fn sync_loading_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    state: Res<LoadingScreenState>,
    mut runtime: ResMut<LoadingScreenRuntime>,
) {
    if !state.is_active() {
        if let Some(root) = runtime.root_entity.take() {
            commands.entity(root).despawn();
        }
        return;
    }

    if runtime.root_entity.is_some() && runtime.generation == state.generation() {
        return;
    }

    if let Some(prev) = runtime.root_entity.take() {
        commands.entity(prev).despawn();
    }

    let bg_color = if state.config().transparent {
        Color::NONE
    } else {
        Color::BLACK
    };

    let throbber_handle = if state.config().show_throbber {
        Some(
            runtime
                .throbber_handle
                .get_or_insert_with(|| asset_server.load(THROBBER_ASSET_PATH))
                .clone(),
        )
    } else {
        None
    };

    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(bg_color),
            GlobalZIndex(LOADING_OVERLAY_Z_INDEX),
            Pickable::IGNORE,
            LoadingScreenRoot,
        ))
        .with_children(|parent| {
            if let Some(handle) = throbber_handle {
                parent.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        right: Val::Px(THROBBER_MARGIN_PX),
                        bottom: Val::Px(THROBBER_MARGIN_PX),
                        width: Val::Px(THROBBER_SIZE_PX),
                        height: Val::Px(THROBBER_SIZE_PX),
                        ..default()
                    },
                    ImageNode::new(handle).with_color(Color::srgba(1.0, 1.0, 1.0, 0.0)),
                    UiTransform::default(),
                    Pickable::IGNORE,
                    LoadingThrobberVisual,
                ));
            }
        })
        .id();

    runtime.root_entity = Some(root);
    runtime.generation = state.generation();
    runtime.elapsed_secs = 0.0;
}

pub(super) fn animate_loading_throbber(
    time: Res<Time>,
    state: Res<LoadingScreenState>,
    mut runtime: ResMut<LoadingScreenRuntime>,
    mut throbbers: Query<(&mut ImageNode, &mut UiTransform), With<LoadingThrobberVisual>>,
) {
    if !state.is_active() || !state.config().show_throbber {
        return;
    }

    runtime.elapsed_secs += time.delta_secs().min(0.1);
    let config = state.config();
    let alpha = if runtime.elapsed_secs < config.delay_secs {
        0.0
    } else if config.fade_in_secs <= 0.0 {
        1.0
    } else {
        ((runtime.elapsed_secs - config.delay_secs) / config.fade_in_secs).clamp(0.0, 1.0)
    };

    let step =
        (runtime.elapsed_secs * THROBBER_STEPS_PER_SEC).floor() as u32 % THROBBER_ANGLE_SNAPS;
    let angle_deg = (360.0 / THROBBER_ANGLE_SNAPS as f32) * (step as f32);
    let rotation = Rot2::radians(-angle_deg.to_radians());

    for (mut image, mut tf) in &mut throbbers {
        image.color = Color::srgba(1.0, 1.0, 1.0, alpha);
        tf.rotation = rotation;
    }
}
