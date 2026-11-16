use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::mods::ModRegistry;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::set_atlas_index;

use super::LoadingScreenState;
use super::character::pick_and_load_character;
use super::components::{
    BAR_ALPHA, BAR_LERP_SPEED, BAR_PAD, BAR_Z, BG_Z, CHARACTER_Z, LOADING_CAMERA_ORDER,
    LoadingBgVisual, LoadingCharacterVisual, LoadingMicStandVisual, LoadingOverlayCamera,
    LoadingProgressBarVisual, LoadingScreenRuntime, MIC_STAND_ASSET_PATH, MIC_STAND_MARGIN_BOTTOM,
    MIC_STAND_MARGIN_RIGHT, MIC_STAND_SCALE, MIC_STAND_Z, loading_render_layer,
};

pub(super) fn sync_loading_screen(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mods: Option<Res<ModRegistry>>,
    state: Res<LoadingScreenState>,
    mut runtime: ResMut<LoadingScreenRuntime>,
) {
    if !state.is_active() {
        despawn_loading_entities(&mut commands, &mut runtime);
        return;
    }

    if runtime.camera_entity.is_some() && runtime.generation == state.generation() {
        return;
    }

    despawn_loading_entities(&mut commands, &mut runtime);

    let layer = loading_render_layer();
    let cam = commands
        .spawn((
            Camera2d,
            Camera {
                order: LOADING_CAMERA_ORDER,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Msaa::Off,
            layer.clone(),
            LoadingOverlayCamera,
        ))
        .id();
    runtime.camera_entity = Some(cam);

    if !state.config().transparent {
        let bg = commands
            .spawn((
                Sprite::from_color(Color::BLACK, Vec2::new(SCREEN_WIDTH * 4.0, SCREEN_HEIGHT * 4.0)),
                Transform::from_xyz(0.0, 0.0, BG_Z),
                layer.clone(),
                LoadingBgVisual,
            ))
            .id();
        runtime.bg_entity = Some(bg);
    }

    if state.config().show_progress {
        let mic_handle = runtime
            .mic_handle
            .get_or_insert_with(|| asset_server.load(MIC_STAND_ASSET_PATH))
            .clone();
        let mic = commands
            .spawn((
                Sprite::from_image(mic_handle),
                Transform::from_xyz(0.0, 0.0, MIC_STAND_Z).with_scale(Vec3::splat(MIC_STAND_SCALE)),
                Visibility::Hidden,
                layer.clone(),
                LoadingMicStandVisual,
            ))
            .id();
        let bar = commands
            .spawn((
                Sprite::from_color(Color::srgba(0.0, 0.0, 0.0, BAR_ALPHA), Vec2::ONE),
                Transform::from_xyz(0.0, 0.0, BAR_Z),
                Visibility::Hidden,
                layer.clone(),
                LoadingProgressBarVisual,
            ))
            .id();
        runtime.mic_entity = Some(mic);
        runtime.bar_entity = Some(bar);
        runtime.mic_layout_done = false;
    }

    if state.config().show_character {
        let char_data = pick_and_load_character(mods.as_deref(), &asset_server, &mut layouts);
        if let Some(loaded) = char_data {
            let initial_idx = loaded.controller.current_atlas_index().unwrap_or(0);
            let mut sprite = Sprite::from_atlas_image(
                loaded.texture.clone(),
                TextureAtlas {
                    layout: loaded.layout.clone(),
                    index: initial_idx,
                },
            );
            sprite.color = Color::WHITE;
            let ent = commands
                .spawn((
                    sprite,
                    Transform::from_xyz(loaded.base_center_x, loaded.base_center_y, CHARACTER_Z)
                        .with_scale(Vec3::splat(loaded.scale)),
                    loaded.controller.clone(),
                    layer,
                    LoadingCharacterVisual {
                        scale: loaded.scale,
                        base_center_x: loaded.base_center_x,
                        base_center_y: loaded.base_center_y,
                    },
                ))
                .id();
            runtime.char_entity = Some(ent);
            runtime.cached_char = Some(loaded);
        }
    }

    runtime.generation = state.generation();
}

pub(super) fn sync_loading_mic_and_bar(
    time: Res<Time>,
    images: Res<Assets<Image>>,
    state: Res<LoadingScreenState>,
    mut runtime: ResMut<LoadingScreenRuntime>,
    mut mic_query: Query<(&mut Transform, &mut Visibility), With<LoadingMicStandVisual>>,
    mut bar_query: Query<
        (&mut Sprite, &mut Transform, &mut Visibility),
        (With<LoadingProgressBarVisual>, Without<LoadingMicStandVisual>),
    >,
) {
    if !state.is_active() || !state.config().show_progress {
        return;
    }

    if !runtime.mic_layout_done {
        let Some(ref handle) = runtime.mic_handle else {
            return;
        };
        let Some(img) = images.get(handle) else {
            return;
        };
        let raw_size = img.size().as_vec2();
        let mic_w = raw_size.x * MIC_STAND_SCALE;
        let mic_h = raw_size.y * MIC_STAND_SCALE;

        let mic_cx = SCREEN_WIDTH * 0.5 - MIC_STAND_MARGIN_RIGHT - mic_w * 0.5;
        let mic_cy = -SCREEN_HEIGHT * 0.5 + MIC_STAND_MARGIN_BOTTOM + mic_h * 0.5;

        let bar_w = mic_w + BAR_PAD;
        let bar_h = mic_h + BAR_PAD;
        let bar_base_x = (SCREEN_WIDTH * 0.5 - MIC_STAND_MARGIN_RIGHT - mic_w) + bar_w * 0.5;
        let bar_cy = (-SCREEN_HEIGHT * 0.5 + MIC_STAND_MARGIN_BOTTOM) + bar_h * 0.5;

        for (mut tf, mut vis) in &mut mic_query {
            tf.translation = Vec3::new(mic_cx, mic_cy, MIC_STAND_Z);
            *vis = Visibility::Visible;
        }
        for (mut sprite, mut tf, mut vis) in &mut bar_query {
            sprite.custom_size = Some(Vec2::new(bar_w, bar_h));
            tf.translation = Vec3::new(bar_base_x, bar_cy, BAR_Z);
            *vis = Visibility::Visible;
        }

        runtime.mic_width = mic_w;
        runtime.bar_base_x = bar_base_x;
        runtime.bar_current_x = bar_base_x;
        runtime.mic_layout_done = true;
    }

    let target_x = runtime.bar_base_x + runtime.mic_width * state.progress();
    let dt = time.delta_secs().min(0.1);
    let t = 1.0 - (-dt * BAR_LERP_SPEED).exp();
    runtime.bar_current_x += (target_x - runtime.bar_current_x) * t;

    for (_, mut tf, _) in &mut bar_query {
        tf.translation.x = runtime.bar_current_x;
    }
}

pub(super) fn sync_loading_character(
    state: Res<LoadingScreenState>,
    mut query: Query<(
        &LoadingCharacterVisual,
        &AnimationController,
        &mut Sprite,
        &mut Transform,
    )>,
) {
    if !state.is_active() || !state.config().show_character {
        return;
    }

    for (vis, anim, mut sprite, mut tf) in &mut query {
        if let Some(idx) = anim.current_atlas_index() {
            set_atlas_index(&mut sprite, idx);
        }
        let frame_off = anim.current_frame_offset();
        let rotated = anim.current_frame_rotated();
        tf.translation.x = vis.base_center_x + frame_off[0] * vis.scale;
        tf.translation.y = vis.base_center_y + frame_off[1] * vis.scale;
        tf.rotation = if rotated {
            Quat::from_rotation_z(FRAC_PI_2)
        } else {
            Quat::IDENTITY
        };
    }
}

fn despawn_loading_entities(commands: &mut Commands, runtime: &mut LoadingScreenRuntime) {
    for slot in [
        &mut runtime.camera_entity,
        &mut runtime.bg_entity,
        &mut runtime.mic_entity,
        &mut runtime.bar_entity,
        &mut runtime.char_entity,
    ] {
        if let Some(ent) = slot.take() {
            commands.entity(ent).despawn();
        }
    }
    runtime.mic_layout_done = false;
    runtime.cached_char = None;
}
