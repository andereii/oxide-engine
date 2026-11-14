use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animations::{AnimationController, Bopper};
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{set_atlas_index, set_sprite_flip, set_transform_full, set_visibility};

use super::loader::{StagePropKind, StageResource};
use super::paths::parse_hex_color;
use super::schema::{StageCoordMode, stage_layer_z};

#[derive(Component)]
#[allow(dead_code)]
pub struct StagePropVisual {
    pub name: String,
    pub base_position: [f32; 2],
    pub base_scale: [f32; 2],
    pub scroll_factor: [f32; 2],
    pub base_flip: [bool; 2],
    pub base_angle_deg: f32,
    pub z_layer: f32,
    pub visible: bool,
    pub coord_mode: StageCoordMode,
    pub raw_size: [f32; 2],
}

#[derive(Resource, Default)]
pub struct StageSyncState {
    pub last_step: Option<i32>,
}

pub fn spawn_stage_props(commands: &mut Commands, stage: &StageResource) {
    for (rank, resolved) in stage.props.iter().enumerate() {
        let d = &resolved.data;
        let z_layer = stage_layer_z(d.layer, rank);
        let tint = apply_opacity(parse_hex_color(&d.color), d.opacity);
        let visibility = if d.visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };

        match &resolved.visual {
            StagePropKind::StaticImage { texture, size } => {
                let mut sprite = Sprite::from_image(texture.clone());
                sprite.color = tint;
                sprite.flip_x = d.flip[0];
                sprite.flip_y = d.flip[1];
                commands.spawn((
                    sprite,
                    Transform::from_xyz(0.0, 0.0, z_layer),
                    visibility,
                    make_visual_comp(d.name.clone(), d.position, d.scale, d.scroll_factor, d.flip, d.angle, z_layer, d.visible, stage.coord_mode, *size),
                ));
            }
            StagePropKind::SolidColor { color } => {
                let solid_tint = apply_opacity(*color, d.opacity);
                let size = [d.scale[0].max(1.0), d.scale[1].max(1.0)];
                let mut sprite = Sprite {
                    color: solid_tint,
                    custom_size: Some(Vec2::new(size[0], size[1])),
                    ..default()
                };
                sprite.flip_x = d.flip[0];
                sprite.flip_y = d.flip[1];
                commands.spawn((
                    sprite,
                    Transform::from_xyz(0.0, 0.0, z_layer),
                    visibility,
                    make_visual_comp(d.name.clone(), d.position, [1.0, 1.0], d.scroll_factor, d.flip, d.angle, z_layer, d.visible, stage.coord_mode, size),
                ));
            }
            StagePropKind::AnimatedAtlas {
                texture,
                layout,
                controller,
                frame_size,
                ..
            } => {
                let initial_idx = controller.current_atlas_index().unwrap_or(0);
                let mut sprite = Sprite::from_atlas_image(
                    texture.clone(),
                    TextureAtlas {
                        layout: layout.clone(),
                        index: initial_idx,
                    },
                );
                sprite.color = tint;
                let anim_flip = controller.current_flip();
                sprite.flip_x = d.flip[0] ^ anim_flip[0];
                sprite.flip_y = d.flip[1] ^ anim_flip[1];
                let visual = make_visual_comp(
                    d.name.clone(),
                    d.position,
                    d.scale,
                    d.scroll_factor,
                    d.flip,
                    d.angle,
                    z_layer,
                    d.visible,
                    stage.coord_mode,
                    *frame_size,
                );
                if d.dance_every != 0.0 || !d.animations.is_empty() {
                    let mut bopper = Bopper::new(d.dance_every);
                    bopper.name = d.name.clone();
                    bopper.original_position = d.position;
                    commands.spawn((
                        sprite,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        visibility,
                        controller.clone(),
                        bopper,
                        visual,
                    ));
                } else {
                    commands.spawn((
                        sprite,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        visibility,
                        controller.clone(),
                        visual,
                    ));
                }
            }
        }
    }
}

pub fn sync_stage_props(
    state: Res<PlayState>,
    mut sync_state: ResMut<StageSyncState>,
    mut animated_props: Query<(
        &StagePropVisual,
        &mut Sprite,
        &mut Transform,
        &mut Visibility,
        Option<&mut AnimationController>,
        Option<&mut Bopper>,
    )>,
) {
    let paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let current_step = state.conductor.current_step;
    let step_changed = if state.needs_reset {
        sync_state.last_step = None;
        false
    } else if !paused && sync_state.last_step != Some(current_step) {
        sync_state.last_step = Some(current_step);
        true
    } else {
        false
    };

    let cam_scroll_x = state.camera.follow_point[0] as f32 - SCREEN_WIDTH * 0.5;
    let cam_scroll_y = state.camera.follow_point[1] as f32 - SCREEN_HEIGHT * 0.5;
    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;

    for (vis, mut sprite, mut transform, mut visibility, anim_opt, bopper_opt) in &mut animated_props {
        if !vis.visible {
            set_visibility(&mut visibility, Visibility::Hidden);
            continue;
        }
        let (anim_off, frame_off, anim_flip, rotated) = match (anim_opt, bopper_opt) {
            (Some(mut anim), Some(mut bopper)) => {
                bopper.sync_finished(&anim);
                if step_changed {
                    bopper.on_step_hit(current_step, &mut anim);
                }
                if let Some(idx) = anim.current_atlas_index() {
                    set_atlas_index(&mut sprite, idx);
                }
                (
                    anim.current_offset(),
                    anim.current_frame_offset(),
                    anim.current_flip(),
                    anim.current_frame_rotated(),
                )
            }
            (Some(anim), None) => (
                anim.current_offset(),
                anim.current_frame_offset(),
                anim.current_flip(),
                anim.current_frame_rotated(),
            ),
            _ => ([0.0, 0.0], [0.0, 0.0], [false, false], false),
        };

        let (mut world_cx, mut world_cy) = match vis.coord_mode {
            StageCoordMode::Center => (vis.base_position[0], vis.base_position[1]),
            StageCoordMode::TopLeft => (
                vis.base_position[0] + vis.raw_size[0] * vis.base_scale[0] * 0.5,
                vis.base_position[1] + vis.raw_size[1] * vis.base_scale[1] * 0.5,
            ),
        };
        world_cx -= anim_off[0] * vis.base_scale[0];
        world_cy -= anim_off[1] * vis.base_scale[1];

        let screen_cx = world_cx - cam_scroll_x * vis.scroll_factor[0];
        let screen_cy = world_cy - cam_scroll_y * vis.scroll_factor[1];

        let bx = (screen_cx - SCREEN_WIDTH * 0.5) * zoom + frame_off[0] * vis.base_scale[0] * zoom;
        let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * zoom + frame_off[1] * vis.base_scale[1] * zoom;

        let base_rot = -vis.base_angle_deg.to_radians();
        set_transform_full(
            &mut transform,
            Vec3::new(bx, by, vis.z_layer),
            Vec3::new(vis.base_scale[0] * zoom, vis.base_scale[1] * zoom, 1.0),
            Quat::from_rotation_z(if rotated { base_rot - FRAC_PI_2 } else { base_rot }),
        );
        set_sprite_flip(&mut sprite, vis.base_flip[0] ^ anim_flip[0], vis.base_flip[1] ^ anim_flip[1]);
        set_visibility(&mut visibility, Visibility::Visible);
    }
}

#[allow(clippy::too_many_arguments)]
fn make_visual_comp(
    name: String,
    base_position: [f32; 2],
    base_scale: [f32; 2],
    scroll_factor: [f32; 2],
    base_flip: [bool; 2],
    base_angle_deg: f32,
    z_layer: f32,
    visible: bool,
    coord_mode: StageCoordMode,
    raw_size: [f32; 2],
) -> StagePropVisual {
    StagePropVisual {
        name,
        base_position,
        base_scale,
        scroll_factor,
        base_flip,
        base_angle_deg,
        z_layer,
        visible,
        coord_mode,
        raw_size,
    }
}

fn apply_opacity(color: Color, opacity: f32) -> Color {
    let s = color.to_srgba();
    Color::srgba(s.red, s.green, s.blue, s.alpha * opacity.clamp(0.0, 1.0))
}
