use bevy::math::{Affine3A, Vec3A};
use bevy::prelude::*;

use crate::animations::Bopper;
use crate::play::stage::schema::stage_layer_z;

use super::behavior::{CharacterActor, find_count_animations};
use super::loader::CharactersResource;

#[derive(Component)]
pub struct CharacterPartVisual;

#[derive(Component, Default, Clone, Copy)]
pub struct CompositeAffine2d {
    pub active: bool,
    pub cols: [f32; 4],
    pub translation: Vec3,
}

pub fn apply_composite_affine_transforms(
    mut query: Query<(&CompositeAffine2d, &mut GlobalTransform)>,
) {
    for (aff, mut global_tf) in &mut query {
        if !aff.active {
            continue;
        }
        let affine = Affine3A::from_cols(
            Vec3A::new(aff.cols[0], aff.cols[1], 0.0),
            Vec3A::new(aff.cols[2], aff.cols[3], 0.0),
            Vec3A::Z,
            Vec3A::from(aff.translation),
        );
        *global_tf = GlobalTransform::from(affine);
    }
}

pub fn spawn_characters(commands: &mut Commands, res: &CharactersResource) {
    for loaded in &res.characters {
        let Some(first_src) = loaded.sources.first() else {
            continue;
        };
        let z_base = stage_layer_z(loaded.slot.layer, 0);
        let sheet_entity = commands
            .spawn((
                Sprite::from_atlas_image(
                    first_src.texture.clone(),
                    TextureAtlas {
                        layout: first_src.layout.clone(),
                        index: 0,
                    },
                ),
                Transform::from_xyz(0.0, 0.0, z_base),
                Visibility::Hidden,
                CharacterPartVisual,
                CompositeAffine2d::default(),
            ))
            .id();

        let mut piece_entities = Vec::with_capacity(loaded.max_composite_pieces);
        for i in 0..loaded.max_composite_pieces {
            let piece_entity = commands
                .spawn((
                    Sprite::from_atlas_image(
                        first_src.texture.clone(),
                        TextureAtlas {
                            layout: first_src.layout.clone(),
                            index: 0,
                        },
                    ),
                    Transform::from_xyz(0.0, 0.0, stage_layer_z(loaded.slot.layer, i)),
                    Visibility::Hidden,
                    CharacterPartVisual,
                    CompositeAffine2d::default(),
                ))
                .id();
            piece_entities.push(piece_entity);
        }

        let combo_counts = find_count_animations(&loaded.controller, "combo");
        let drop_counts = find_count_animations(&loaded.controller, "drop");
        let mut bopper = Bopper::new(loaded.data.dance_every);
        bopper.name = loaded.data.name.clone();
        bopper.should_bop = false;

        let mut actor = CharacterActor {
            id: loaded.id.clone(),
            role: loaded.role,
            slot: loaded.slot.clone(),
            data: loaded.data.clone(),
            controller: loaded.controller.clone(),
            bopper,
            sources: loaded.sources.clone(),
            clips: loaded.clips.clone(),
            idle_size: loaded.idle_size,
            facing_flip_x: loaded.facing_flip_x,
            camera_focus_point: [0.0, 0.0],
            hold_timer: 0.0,
            cur_no_anim: false,
            cur_suffix: String::new(),
            combo_counts,
            drop_counts,
            sampler_applied: false,
            mod_tint: [1.0; 4],
            mod_visible: true,
            sheet_entity,
            piece_entities,
        };
        actor.reset_character();
        commands.spawn(actor);
    }
}

