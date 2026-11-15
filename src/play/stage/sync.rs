use std::collections::HashMap;
use std::f32::consts::FRAC_PI_2;

use bevy::prelude::*;

use crate::animations::atlas_composite::{
    CompositeClip, CompositePiece, FlxAnimateTransformParams, compose_flx_piece_bevy_affine,
    modulate_color,
};
use crate::animations::{AnimationController, AtlasSettings, Bopper};
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
pub struct StageAtlasFrameSizes(pub HashMap<usize, [f32; 2]>);

#[derive(Component)]
pub struct StageCompositePiece;

#[derive(Component)]
pub struct StageCompositeProp {
    pub textures: Vec<(Handle<Image>, Handle<TextureAtlasLayout>)>,
    pub clips: HashMap<String, CompositeClip>,
    pub bounds_min: [f32; 2],
    pub bounds_size: [f32; 2],
    pub atlas_settings: AtlasSettings,
    pub stage_matrix: Option<[f32; 6]>,
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
                frame_sizes,
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
                let atlas_sizes = StageAtlasFrameSizes(frame_sizes.clone());
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
                        atlas_sizes,
                    ));
                } else {
                    commands.spawn((
                        sprite,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        visibility,
                        controller.clone(),
                        visual,
                        atlas_sizes,
                    ));
                }
            }
            StagePropKind::CompositeAtlas {
                textures,
                controller,
                clips,
                bounds_min,
                bounds_size,
                atlas_settings,
                stage_matrix,
                max_pieces,
            } => {
                let Some((base_tex, base_layout)) = textures.first() else {
                    continue;
                };
                let mut piece_entities = Vec::with_capacity(*max_pieces);
                for i in 0..*max_pieces {
                    let piece_ent = commands
                        .spawn((
                            Sprite::from_atlas_image(
                                base_tex.clone(),
                                TextureAtlas {
                                    layout: base_layout.clone(),
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
                    textures: textures.clone(),
                    clips: clips.clone(),
                    bounds_min: *bounds_min,
                    bounds_size: *bounds_size,
                    atlas_settings: *atlas_settings,
                    stage_matrix: *stage_matrix,
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
        Option<&StageAtlasFrameSizes>,
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
        frame_sizes_opt,
    ) in &mut animated_props
    {
        let is_hud = dyn_opt.is_some_and(|d| d.hud);
        let is_hidden = !vis.visible || (is_hud && !state.hud_visible);
        let prop_zoom = if is_hud { 1.0 } else { zoom };
        let eff_scroll_x = if is_hud { 0.0 } else { cam_scroll_x * vis.scroll_factor[0] };
        let eff_scroll_y = if is_hud { 0.0 } else { cam_scroll_y * vis.scroll_factor[1] };

        if let Some(composite) = composite_opt {
            set_visibility(&mut visibility, Visibility::Hidden);
            let (anim_off, anim_flip, clip_bounds_min, clip_bounds_size, cur_pieces) =
                match (anim_opt, bopper_opt) {
                    (Some(mut anim), Some(mut bopper)) => {
                        bopper.sync_finished(&anim);
                        if let Some(steps) = step_range.clone() {
                            for step in steps {
                                bopper.on_step_hit(step, &mut anim);
                            }
                        }
                        let (bmin, bsize, frame_pieces) = composite_frame_info(composite, &anim);
                        (anim.current_offset(), anim.current_flip(), bmin, bsize, frame_pieces)
                    }
                    (Some(anim), None) => {
                        let (bmin, bsize, frame_pieces) = composite_frame_info(composite, &anim);
                        (anim.current_offset(), anim.current_flip(), bmin, bsize, frame_pieces)
                    }
                    _ => ([0.0, 0.0], [false, false], composite.bounds_min, composite.bounds_size, &[][..]),
                };

            if is_hidden {
                hide_composite_pieces(&composite.piece_entities, &mut pieces);
                continue;
            }

            sync_composite_prop_pieces(
                vis,
                composite,
                cur_pieces,
                clip_bounds_min,
                clip_bounds_size,
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
        let (anim_off, frame_off, anim_flip, rotated, atlas_idx) = match (anim_opt, bopper_opt) {
            (Some(mut anim), Some(mut bopper)) => {
                bopper.sync_finished(&anim);
                if let Some(steps) = step_range.clone() {
                    for step in steps {
                        bopper.on_step_hit(step, &mut anim);
                    }
                }
                let idx = anim.current_atlas_index();
                if let Some(idx) = idx {
                    set_atlas_index(&mut sprite, idx);
                }
                (
                    anim.current_offset(),
                    anim.current_frame_offset(),
                    anim.current_flip(),
                    anim.current_frame_rotated(),
                    idx,
                )
            }
            (Some(anim), None) => (
                anim.current_offset(),
                anim.current_frame_offset(),
                anim.current_flip(),
                anim.current_frame_rotated(),
                anim.current_atlas_index(),
            ),
            _ => ([0.0, 0.0], [0.0, 0.0], [false, false], false, None),
        };

        let cur_raw_size = frame_sizes_opt
            .and_then(|fs| atlas_idx.and_then(|idx| fs.0.get(&idx).copied()))
            .unwrap_or(vis.raw_size);

        let (mut world_cx, mut world_cy) = match vis.coord_mode {
            StageCoordMode::Center => (vis.base_position[0], vis.base_position[1]),
            StageCoordMode::TopLeft => (
                vis.base_position[0] + cur_raw_size[0] * vis.base_scale[0] * 0.5,
                vis.base_position[1] + cur_raw_size[1] * vis.base_scale[1] * 0.5,
            ),
        };
        world_cx -= anim_off[0] * vis.base_scale[0];
        world_cy -= anim_off[1] * vis.base_scale[1];

        let screen_cx = world_cx - eff_scroll_x;
        let screen_cy = world_cy - eff_scroll_y;

        let base_rot = -vis.base_angle_deg.to_radians();
        let eff_flip_x = vis.base_flip[0] ^ anim_flip[0];
        let eff_flip_y = vis.base_flip[1] ^ anim_flip[1];
        let foff_x = if eff_flip_x { -frame_off[0] } else { frame_off[0] };
        let foff_y = if eff_flip_y { -frame_off[1] } else { frame_off[1] };

        let bx = (screen_cx - SCREEN_WIDTH * 0.5) * prop_zoom
            + foff_x * vis.base_scale[0] * prop_zoom;
        let by = (SCREEN_HEIGHT * 0.5 - screen_cy) * prop_zoom
            + foff_y * vis.base_scale[1] * prop_zoom;
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

fn composite_frame_info<'a>(
    composite: &'a StageCompositeProp,
    anim: &AnimationController,
) -> ([f32; 2], [f32; 2], &'a [CompositePiece]) {
    let Some(name) = anim.current.as_deref() else {
        return (composite.bounds_min, composite.bounds_size, &[]);
    };
    let Some(clip) = composite.clips.get(name) else {
        return (composite.bounds_min, composite.bounds_size, &[]);
    };
    let frame_idx = anim.current_atlas_index().unwrap_or(0);
    let pieces = clip.frames.get(frame_idx).map_or(&[][..], Vec::as_slice);
    (clip.bounds_min, clip.bounds_size, pieces)
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
    clip_bounds_min: [f32; 2],
    clip_bounds_size: [f32; 2],
    anim_off: [f32; 2],
    anim_flip: [bool; 2],
    tint: Color,
    eff_scroll_x: f32,
    eff_scroll_y: f32,
    prop_zoom: f32,
    pieces_query: &mut StagePiecesQuery,
) {
    let (mut top_left_x, mut top_left_y) = match vis.coord_mode {
        StageCoordMode::TopLeft => (vis.base_position[0], vis.base_position[1]),
        StageCoordMode::Center => (
            vis.base_position[0] - vis.raw_size[0] * vis.base_scale[0] * 0.5,
            vis.base_position[1] - vis.raw_size[1] * vis.base_scale[1] * 0.5,
        ),
    };
    top_left_x -= anim_off[0] * vis.base_scale[0] + eff_scroll_x;
    top_left_y -= anim_off[1] * vis.base_scale[1] + eff_scroll_y;

    let params = FlxAnimateTransformParams {
        bounds_min: clip_bounds_min,
        bounds_size: clip_bounds_size,
        top_left: [top_left_x, top_left_y],
        scale: vis.base_scale,
        angle_deg: vis.base_angle_deg,
        flip: [vis.base_flip[0] ^ anim_flip[0], vis.base_flip[1] ^ anim_flip[1]],
        stage_matrix: composite.stage_matrix,
        apply_stage_matrix: composite.atlas_settings.apply_stage_matrix,
        post_stage_matrix_apply: composite.atlas_settings.post_stage_matrix_apply,
        screen_size: [SCREEN_WIDTH, SCREEN_HEIGHT],
        zoom: prop_zoom,
    };

    for (i, &ent) in composite.piece_entities.iter().enumerate() {
        let Ok((mut sprite, mut tf, mut p_vis, mut aff)) = pieces_query.get_mut(ent) else {
            continue;
        };
        let Some(piece) = frame_pieces.get(i) else {
            aff.active = false;
            set_visibility(&mut p_vis, Visibility::Hidden);
            continue;
        };
        let Some((tex, layout)) = composite
            .textures
            .get(piece.spritemap_idx)
            .or_else(|| composite.textures.first())
        else {
            aff.active = false;
            set_visibility(&mut p_vis, Visibility::Hidden);
            continue;
        };

        let ([bx, by], cols) = compose_flx_piece_bevy_affine(piece, &params);
        let pos = Vec3::new(bx, by, vis.z_layer + (i as f32) * 1e-6);
        set_sprite_atlas(&mut sprite, tex, layout, piece.sprite_index);
        set_sprite_color(&mut sprite, modulate_color(tint, piece.color));
        set_sprite_flip(&mut sprite, false, false);
        set_transform_full(&mut tf, pos, Vec3::ONE, Quat::IDENTITY);
        aff.active = true;
        aff.cols = cols;
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
