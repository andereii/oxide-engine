use bevy::prelude::*;

use crate::animations::AnimationController;
use crate::mods::ModRegistry;
use crate::play::characters::loader::CharactersResource;
use crate::play::notes::NoteStyle;
use crate::play::scene::PlayState;
use crate::utils::constants::{
    COLOR_HEALTH_BAR_GREEN, COLOR_HEALTH_BAR_RED, HEALTH_BAR_BORDER_PX,
    HEALTH_BAR_Y_RATIO_DOWNSCROLL, HEALTH_BAR_Y_RATIO_UPSCROLL, HEALTH_MAX, SCREEN_HEIGHT,
    SCREEN_WIDTH,
};
use crate::utils::ecs::{set_sprite_custom_size, set_transform_pos_scale};

use crate::play::camera::hud::hud_render_layer;
use super::health_icon::{HealthIcon, spawn_health_icons, update_icon_state_and_transform};

#[derive(Component)]
pub struct HealthBarBgVisual {
    pub bg_image: Handle<Image>,
    pub size: [f32; 2],
    pub scale: f32,
}

#[derive(Component)]
pub struct HealthBarFillVisual {
    pub is_player: bool,
}

#[derive(Default)]
pub struct HealthBarStepTracker {
    last_step: Option<i32>,
}

pub fn spawn_health_bar(
    mods: &ModRegistry,
    commands: &mut Commands,
    chars: Option<&CharactersResource>,
    style: &NoteStyle,
    asset_server: &AssetServer,
    layouts: &mut Assets<TextureAtlasLayout>,
) {
    let bg_image: Handle<Image> = asset_server.load(style.health_bar_asset_in(mods));
    let bg_scale = style.health_bar.scale.max(0.01);
    let default_w = 601.0_f32 * bg_scale;
    let default_h = 19.0_f32 * bg_scale;

    let mut bg_sprite = Sprite::from_image(bg_image.clone());
    bg_sprite.color = Color::srgba(1.0, 1.0, 1.0, style.health_bar.alpha.clamp(0.0, 1.0));
    commands.spawn((
        bg_sprite,
        Transform::from_xyz(0.0, 0.0, 10.0).with_scale(Vec3::splat(bg_scale)),
        hud_render_layer(),
        HealthBarBgVisual {
            bg_image,
            size: [default_w, default_h],
            scale: bg_scale,
        },
    ));

    let opp_color = Color::srgb_u8(
        COLOR_HEALTH_BAR_RED[0],
        COLOR_HEALTH_BAR_RED[1],
        COLOR_HEALTH_BAR_RED[2],
    );
    let ply_color = Color::srgb_u8(
        COLOR_HEALTH_BAR_GREEN[0],
        COLOR_HEALTH_BAR_GREEN[1],
        COLOR_HEALTH_BAR_GREEN[2],
    );

    let inner_w = default_w - HEALTH_BAR_BORDER_PX * 2.0;
    let inner_h = default_h - HEALTH_BAR_BORDER_PX * 2.0;

    let mut opp_sprite = Sprite::from_color(opp_color, Vec2::new(inner_w * 0.5, inner_h));
    opp_sprite.custom_size = Some(Vec2::new(inner_w * 0.5, inner_h));
    commands.spawn((
        opp_sprite,
        Transform::from_xyz(-inner_w * 0.25, 0.0, 10.1),
        hud_render_layer(),
        HealthBarFillVisual { is_player: false },
    ));

    let mut ply_sprite = Sprite::from_color(ply_color, Vec2::new(inner_w * 0.5, inner_h));
    ply_sprite.custom_size = Some(Vec2::new(inner_w * 0.5, inner_h));
    commands.spawn((
        ply_sprite,
        Transform::from_xyz(inner_w * 0.25, 0.0, 10.1),
        hud_render_layer(),
        HealthBarFillVisual { is_player: true },
    ));

    spawn_health_icons(mods, commands, chars, asset_server, layouts);
}

#[allow(clippy::too_many_arguments)]
pub fn sync_health_bar(
    time: Res<Time>,
    state: Res<PlayState>,
    images: Res<Assets<Image>>,
    mut layouts: ResMut<Assets<TextureAtlasLayout>>,
    mut step_tracker: Local<HealthBarStepTracker>,
    mut bg_query: Query<(&mut HealthBarBgVisual, &mut Transform, &mut Visibility)>,
    mut fill_query: Query<
        (&HealthBarFillVisual, &mut Sprite, &mut Transform, &mut Visibility),
        Without<HealthBarBgVisual>,
    >,
    mut icons: Query<
        (
            &mut HealthIcon,
            &mut AnimationController,
            &mut Sprite,
            &mut Transform,
            &mut Visibility,
        ),
        (Without<HealthBarBgVisual>, Without<HealthBarFillVisual>),
    >,
) {
    if state.is_minimal_mode || state.critical_failure {
        return;
    }

    let hud_vis = if state.hud_visible {
        Visibility::Visible
    } else {
        Visibility::Hidden
    };

    let dt = time.delta_secs().min(0.1);
    let cur_step = state.conductor.current_step;
    let step_hit = step_tracker.last_step != Some(cur_step);
    step_tracker.last_step = Some(cur_step);

    let downscroll = state.player_strumline.is_downscroll;
    let bg_y_flx = if downscroll {
        SCREEN_HEIGHT * HEALTH_BAR_Y_RATIO_DOWNSCROLL
    } else {
        SCREEN_HEIGHT * HEALTH_BAR_Y_RATIO_UPSCROLL
    };

    let mut bg_size = [601.0_f32, 19.0_f32];
    for (mut bg, mut tf, mut vis) in &mut bg_query {
        if *vis != hud_vis {
            *vis = hud_vis;
        }
        if let Some(img) = images.get(&bg.bg_image) {
            bg.size = [img.width() as f32 * bg.scale, img.height() as f32 * bg.scale];
        }
        bg_size = bg.size;
        let bg_by = SCREEN_HEIGHT * 0.5 - (bg_y_flx + bg_size[1] * 0.5);
        set_transform_pos_scale(&mut tf, Vec3::new(0.0, bg_by, 10.0), Vec3::splat(bg.scale));
    }

    let inner_w = (bg_size[0] - HEALTH_BAR_BORDER_PX * 2.0).max(1.0);
    let inner_h = (bg_size[1] - HEALTH_BAR_BORDER_PX * 2.0).max(1.0);
    let bar_x_flx = (SCREEN_WIDTH - bg_size[0]) * 0.5 + HEALTH_BAR_BORDER_PX;
    let bar_y_flx = bg_y_flx + HEALTH_BAR_BORDER_PX;
    let bar_by = SCREEN_HEIGHT * 0.5 - (bar_y_flx + inner_h * 0.5);

    let pct = (state.health_lerp / HEALTH_MAX).clamp(0.0, 1.0) as f32;
    let ply_w = inner_w * pct;
    let opp_w = inner_w * (1.0 - pct);

    for (fill, mut sprite, mut tf, mut vis) in &mut fill_query {
        if *vis != hud_vis {
            *vis = hud_vis;
        }
        let (w, bx) = if fill.is_player {
            (ply_w, inner_w * 0.5 - ply_w * 0.5)
        } else {
            (opp_w, -inner_w * 0.5 + opp_w * 0.5)
        };
        set_sprite_custom_size(&mut sprite, Some(Vec2::new(w.max(0.0), inner_h)));
        set_transform_pos_scale(&mut tf, Vec3::new(bx, bar_by, 10.1), Vec3::ONE);
    }

    let split_x_flx = bar_x_flx + inner_w * (1.0 - pct);
    let step_len_ms = state.conductor.step_length_ms();
    for (mut icon, mut anim, mut sprite, mut tf, mut vis) in &mut icons {
        if *vis != hud_vis {
            *vis = hud_vis;
        }
        if step_hit {
            icon.on_step_hit(cur_step, step_len_ms);
        }
        icon.update_bop(dt);
        update_icon_state_and_transform(
            &mut icon,
            &mut anim,
            &mut sprite,
            &mut tf,
            &images,
            &mut layouts,
            state.health,
            split_x_flx,
            bar_y_flx,
        );
    }
}

