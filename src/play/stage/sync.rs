use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animations::atlas_composite::{CompositeClip, CompositePiece, upright_size};
use crate::animations::{AnimationController, Bopper};
use crate::play::characters::spawn::CompositeAffine2d;
use crate::play::scene::PlayState;
use crate::play::types::PlayPhase;
use crate::utils::conductor::advance_step_range;
use crate::utils::constants::{SCREEN_HEIGHT, SCREEN_WIDTH};
use crate::utils::ecs::{
    set_atlas_index, set_sprite_atlas, set_sprite_color, set_sprite_flip, set_transform_full,
    set_visibility,
};

use super::loader::{StagePropKind, StageResource};
use super::paths::parse_hex_color;
use super::schema::{StageCoordMode, stage_layer_z};

#[derive(Component)]
pub struct DynamicScriptProp {
    pub id: String,
    pub hud: bool,
}

#[derive(Component)]
pub struct StageCompositePiece;

#[derive(Component)]
pub struct StageCompositeProp {
    pub texture: Handle<Image>,
    pub layout: Handle<TextureAtlasLayout>,
    pub clips: HashMap<String, CompositeClip>,
    pub bounds_min: [f32; 2],
    pub bounds_size: [f32; 2],
    pub piece_entities: Vec<Entity>,
}

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
            StagePropKind::CompositeAtlas {
                texture,
                layout,
                controller,
                clips,
                bounds_min,
                bounds_size,
                max_pieces,
            } => {
                let mut piece_entities = Vec::with_capacity(*max_pieces);
                for i in 0..*max_pieces {
                    let piece_ent = commands
                        .spawn((
                            Sprite::from_atlas_image(
                                texture.clone(),
                                TextureAtlas {
                                    layout: layout.clone(),
                                    index: 0,
                                },
                            ),
                            Transform::from_xyz(0.0, 0.0, z_layer + (i as f32) * 1e-6),
                            Visibility::Hidden,
                            StageCompositePiece,
                            CompositeAffine2d::default(),
                        ))
                        .id();
                    piece_entities.push(piece_ent);
                }

                let mut placeholder = Sprite::default();
                placeholder.color = tint;
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
                    *bounds_size,
                );
                let composite = StageCompositeProp {
                    texture: texture.clone(),
                    layout: layout.clone(),
                    clips: clips.clone(),
                    bounds_min: *bounds_min,
                    bounds_size: *bounds_size,
                    piece_entities,
                };
                if d.dance_every != 0.0 || !d.animations.is_empty() {
                    let mut bopper = Bopper::new(d.dance_every);
                    bopper.name = d.name.clone();
                    bopper.original_position = d.position;
                    commands.spawn((
                        placeholder,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        Visibility::Hidden,
                        controller.clone(),
                        bopper,
                        visual,
                        composite,
                    ));
                } else {
                    commands.spawn((
                        placeholder,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        Visibility::Hidden,
                        controller.clone(),
                        visual,
                        composite,
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
        Option<&DynamicScriptProp>,
        Option<&StageCompositeProp>,
    )>,
    mut pieces: StagePiecesQuery,
) {
    let paused = matches!(
        state.phase,
        PlayPhase::Paused(_) | PlayPhase::GameOver | PlayPhase::Ended
    );
    let current_step = state.conductor.current_step;
    let step_range = advance_step_range(
        &mut sync_state.last_step,
        current_step,
        paused,
        state.needs_reset,
    );

    let cam_scroll_x = state.camera.follow_point[0] as f32 - SCREEN_WIDTH * 0.5;
    let cam_scroll_y = state.camera.follow_point[1] as f32 - SCREEN_HEIGHT * 0.5;
    let zoom = (state.camera.current_zoom * state.camera.bop_multiplier).max(0.05) as f32;

    for (
        vis,
        mut sprite,
        mut transform,
        mut visibility,
        anim_opt,
        bopper_opt,
        dyn_opt,
        composite_opt,
    ) in &mut animated_props
    {
        let is_hud = dyn_opt.is_some_and(|d| d.hud);
        let is_hidden = !vis.visible || (is_hud && !state.hud_visible);
        let prop_zoom = if is_hud { 1.0 } else { zoom };
        let eff_scroll_x = if is_hud { 0.0 } else { cam_scroll_x * vis.scroll_factor[0] };
        let eff_scroll_y = if is_hud { 0.0 } else { cam_scroll_y * vis.scroll_factor[1] };

        if let Some(composite) = composite_opt {
            set_visibility(&mut visibility, Visibility::Hidden);
            let (anim_off, anim_flip, cur_pieces): ([f32; 2], [bool; 2], &[CompositePiece]) =
                match (anim_opt, bopper_opt) {
                    (Some(mut anim), Some(mut bopper)) => {
                        bopper.sync_finished(&anim);
                        if let Some(steps) = step_range.clone() {
                            for step in steps {
                                bopper.on_step_hit(step, &mut anim);
                            }
                        }
                        let frame_pieces = composite_frame_pieces(composite, &anim);
                        (anim.current_offset(), anim.current_flip(), frame_pieces)
                    }
                    (Some(anim), None) => {
                        let frame_pieces = composite_frame_pieces(composite, &anim);
                        (anim.current_offset(), anim.current_flip(), frame_pieces)
                    }
                    _ => ([0.0, 0.0], [false, false], &[]),
                };

            if is_hidden {
                hide_composite_pieces(&composite.piece_entities, &mut pieces);
                continue;
            }

            sync_composite_prop_pieces(
                vis,
                composite,
                cur_pieces,
                anim_off,
                anim_flip,
                sprite.color,
                eff_scroll_x,
                eff_scroll_y,
                prop_zoom,
                &mut pieces,
            );
            continue;
        }

        if is_hidden {
            set_visibility(&mut visibility, Visibility::Hidden);
            continue;
        }
        let (anim_off, frame_off, anim_flip, rotated) = match (anim_opt, bopper_opt) {
            (Some(mut anim), Some(mut bopper)) => {
                bopper.sync_finished(&anim);
                if let Some(steps) = step_range.clone() {
                    for step in steps {
                        bopper.on_step_hit(step, &mut anim);
                    }
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

        let screen_cx = world_cx - eff_scroll_x;
        let screen_cy = world_cy - eff_scroll_y;

        let bx = (screen_cx - SCREEN_WIDTH * 0.5) * prop_zoom
            + frame_off[0] * vis.base_scale[0] * prop_zoom;
        let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * prop_zoom
            + frame_off[1] * vis.base_scale[1] * prop_zoom;

        let base_rot = -vis.base_angle_deg.to_radians();
        let eff_flip_x = vis.base_flip[0] ^ anim_flip[0];
        let eff_flip_y = vis.base_flip[1] ^ anim_flip[1];
        let rot_delta = if rotated {
            if eff_flip_x ^ eff_flip_y { -FRAC_PI_2 } else { FRAC_PI_2 }
        } else {
            0.0
        };
        set_transform_full(
            &mut transform,
            Vec3::new(bx, by, vis.z_layer),
            Vec3::new(vis.base_scale[0] * prop_zoom, vis.base_scale[1] * prop_zoom, 1.0),
            Quat::from_rotation_z(base_rot + rot_delta),
        );
        set_sprite_flip(&mut sprite, eff_flip_x, eff_flip_y);
        set_visibility(&mut visibility, Visibility::Visible);
    }
}

type StagePiecesQuery<'w, 's> = Query<
    'w,
    's,
    (&'static mut Sprite, &'static mut Transform, &'static mut Visibility, &'static mut CompositeAffine2d),
    (With<StageCompositePiece>, Without<StagePropVisual>),
>;

fn composite_frame_pieces<'a>(
    composite: &'a StageCompositeProp,
    anim: &AnimationController,
) -> &'a [CompositePiece] {
    let Some(name) = anim.current.as_deref() else {
        return &[];
    };
    let Some(clip) = composite.clips.get(name) else {
        return &[];
    };
    let frame_idx = anim.current_atlas_index().unwrap_or(0);
    clip.frames.get(frame_idx).map_or(&[], Vec::as_slice)
}

fn hide_composite_pieces(entities: &[Entity], pieces: &mut StagePiecesQuery) {
    for &ent in entities {
        if let Ok((_, _, mut vis, mut aff)) = pieces.get_mut(ent) {
            aff.active = false;
            set_visibility(&mut vis, Visibility::Hidden);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_composite_prop_pieces(
    vis: &StagePropVisual,
    composite: &StageCompositeProp,
    frame_pieces: &[CompositePiece],
    anim_off: [f32; 2],
    anim_flip: [bool; 2],
    tint: Color,
    eff_scroll_x: f32,
    eff_scroll_y: f32,
    prop_zoom: f32,
    pieces_query: &mut StagePiecesQuery,
) {
    let (mut world_cx, mut world_cy) = match vis.coord_mode {
        StageCoordMode::Center => (vis.base_position[0], vis.base_position[1]),
        StageCoordMode::TopLeft => (
            vis.base_position[0] + composite.bounds_size[0] * vis.base_scale[0] * 0.5,
            vis.base_position[1] + composite.bounds_size[1] * vis.base_scale[1] * 0.5,
        ),
    };
    world_cx -= anim_off[0] * vis.base_scale[0];
    world_cy -= anim_off[1] * vis.base_scale[1];

    let screen_cx = world_cx - eff_scroll_x;
    let screen_cy = world_cy - eff_scroll_y;
    let bevy_cx = (screen_cx - SCREEN_WIDTH * 0.5) * prop_zoom;
    let bevy_cy = (SCREEN_HEIGHT * 0.5 - screen_cy) * prop_zoom;

    let anchor_x = composite.bounds_min[0] + composite.bounds_size[0] * 0.5;
    let anchor_y = composite.bounds_min[1] + composite.bounds_size[1] * 0.5;

    let eff_flip_x = vis.base_flip[0] ^ anim_flip[0];
    let eff_flip_y = vis.base_flip[1] ^ anim_flip[1];
    let flip_sx = if eff_flip_x { -1.0 } else { 1.0 };
    let flip_sy = if eff_flip_y { -1.0 } else { 1.0 };
    let sx = flip_sx * vis.base_scale[0] * prop_zoom;
    let sy = flip_sy * vis.base_scale[1] * prop_zoom;
    let (sin_r, cos_r) = (-vis.base_angle_deg.to_radians()).sin_cos();

    for (i, &ent) in composite.piece_entities.iter().enumerate() {
        let Ok((mut sprite, mut tf, mut p_vis, mut aff)) = pieces_query.get_mut(ent) else {
            continue;
        };
        let Some(piece) = frame_pieces.get(i) else {
            aff.active = false;
            set_visibility(&mut p_vis, Visibility::Hidden);
            continue;
        };

        let (w, h) = upright_size(piece);
        let m = piece.matrix;
        let fcx = m[0] * (w * 0.5) + m[2] * (h * 0.5) + m[4];
        let fcy = m[1] * (w * 0.5) + m[3] * (h * 0.5) + m[5];
        let lx = (fcx - anchor_x) * sx;
        let ly = -(fcy - anchor_y) * sy;

        let [mut m00, mut m01, mut m10, mut m11] = if piece.rotated {
            [-m[2], -m[0], m[3], m[1]]
        } else {
            [m[0], -m[2], -m[1], m[3]]
        };
        m00 *= sx;
        m01 *= sx;
        m10 *= sy;
        m11 *= sy;

        let rx = lx * cos_r - ly * sin_r;
        let ry = lx * sin_r + ly * cos_r;
        let c0x = m00 * cos_r - m10 * sin_r;
        let c0y = m00 * sin_r + m10 * cos_r;
        let c1x = m01 * cos_r - m11 * sin_r;
        let c1y = m01 * sin_r + m11 * cos_r;

        let pos = Vec3::new(bevy_cx + rx, bevy_cy + ry, vis.z_layer + (i as f32) * 1e-6);
        set_sprite_atlas(&mut sprite, &composite.texture, &composite.layout, piece.sprite_index);
        set_sprite_color(&mut sprite, tint);
        set_sprite_flip(&mut sprite, false, false);
        set_transform_full(&mut tf, pos, Vec3::ONE, Quat::IDENTITY);
        aff.active = true;
        aff.cols = [c0x, c0y, c1x, c1y];
        aff.translation = pos;
        set_visibility(&mut p_vis, Visibility::Visible);
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
