use bevy::prelude::*;

use crate::animations::Bopper;
use crate::play::characters::spawn::CompositeAffine2d;

use super::super::loader::{StagePropKind, StageResource};
use super::super::paths::parse_hex_color;
use super::super::schema::{StageCoordMode, composite_piece_z, stage_layer_z};
use super::{
    StageAtlasFrameSizes, StageCompositePiece, StageCompositeProp, StageInitialBlend,
    StagePropPixelSampler, StagePropVisual,
};

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

        let mut prop_entity = match &resolved.visual {
            StagePropKind::StaticImage { texture, size } => {
                let mut sprite = Sprite::from_image(texture.clone());
                sprite.color = tint;
                sprite.flip_x = d.flip[0];
                sprite.flip_y = d.flip[1];
                commands.spawn((
                    sprite,
                    Transform::from_xyz(0.0, 0.0, z_layer),
                    visibility,
                    make_visual_comp(
                        d.name.clone(),
                        d.position,
                        d.scale,
                        d.scroll_factor,
                        d.flip,
                        d.angle,
                        z_layer,
                        d.visible,
                        stage.coord_mode,
                        *size,
                    ),
                    StagePropPixelSampler {
                        is_pixel: d.is_pixel,
                        sampler_applied: false,
                    },
                ))
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
                    make_visual_comp(
                        d.name.clone(),
                        d.position,
                        [1.0, 1.0],
                        d.scroll_factor,
                        d.flip,
                        d.angle,
                        z_layer,
                        d.visible,
                        stage.coord_mode,
                        size,
                    ),
                ))
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
                let pixel_sampler = StagePropPixelSampler {
                    is_pixel: d.is_pixel,
                    sampler_applied: false,
                };
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
                        pixel_sampler,
                    ))
                } else {
                    commands.spawn((
                        sprite,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        visibility,
                        controller.clone(),
                        visual,
                        atlas_sizes,
                        pixel_sampler,
                    ))
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
                            Transform::from_xyz(0.0, 0.0, composite_piece_z(z_layer, i)),
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
                    composite_rt: None,
                };
                let pixel_sampler = StagePropPixelSampler {
                    is_pixel: d.is_pixel,
                    sampler_applied: false,
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
                        pixel_sampler,
                    ))
                } else {
                    commands.spawn((
                        placeholder,
                        Transform::from_xyz(0.0, 0.0, z_layer),
                        Visibility::Hidden,
                        controller.clone(),
                        visual,
                        composite,
                        pixel_sampler,
                    ))
                }
            }
        };
        if let Some(blend) = d.blend.as_ref().filter(|b| !b.trim().is_empty()) {
            prop_entity.insert(StageInitialBlend(blend.clone()));
        }
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
        velocity: [0.0, 0.0],
        acceleration: [0.0, 0.0],
    }
}

fn apply_opacity(color: Color, opacity: f32) -> Color {
    let s = color.to_srgba();
    Color::srgba(s.red, s.green, s.blue, s.alpha * opacity.clamp(0.0, 1.0))
}
