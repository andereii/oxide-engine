use bevy::math::{Affine3A, Vec3A};
use bevy::prelude::*;

use crate::animations::Bopper;
use crate::play::stage::schema::stage_layer_z;

use super::behavior::{
    CharacterActor, CharacterUnderlayConfig, SheetSource, find_count_animations,
};
use super::loader::{CharactersResource, LoadedCharacter};

#[derive(Component)]
pub struct CharacterPartVisual;

#[derive(Component)]
pub struct CharacterTrailVisual;

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
        spawn_single_character(commands, loaded);
    }
}

fn make_character_sprite(sources: &[SheetSource]) -> Sprite {
    match sources.first() {
        Some(first_src) => Sprite::from_atlas_image(
            first_src.texture.clone(),
            TextureAtlas {
                layout: first_src.layout.clone(),
                index: 0,
            },
        ),
        None => Sprite::default(),
    }
}

fn composite_piece_bundle(
    sources: &[SheetSource],
    layer: i32,
    piece_idx: usize,
) -> impl Bundle {
    (
        make_character_sprite(sources),
        Transform::from_xyz(0.0, 0.0, stage_layer_z(layer, piece_idx)),
        Visibility::Hidden,
        CharacterPartVisual,
        CompositeAffine2d::default(),
    )
}

pub(super) fn spawn_composite_piece_entity(
    commands: &mut Commands,
    sources: &[SheetSource],
    layer: i32,
    piece_idx: usize,
) -> Entity {
    commands
        .spawn(composite_piece_bundle(sources, layer, piece_idx))
        .id()
}

pub fn spawn_single_character(commands: &mut Commands, loaded: &LoadedCharacter) -> Entity {
    spawn_single_character_with_underlay(commands, loaded, None)
}

fn push_piece_entity(actor: &mut CharacterActor, ent: Entity, commands: &mut Commands) {
    if let Some(ref rt) = actor.composite_rt {
        commands
            .entity(ent)
            .insert(bevy::camera::visibility::RenderLayers::layer(rt.render_layer));
    }
    actor.piece_entities.push(ent);
}

/// Ensures `actor.piece_entities` has at least `needed` entities, inheriting `CompositeRenderTarget` layer if active.
pub(crate) fn ensure_actor_piece_capacity(
    actor: &mut CharacterActor,
    needed: usize,
    commands: &mut Commands,
) {
    if needed <= actor.piece_entities.len() {
        return;
    }
    actor
        .piece_entities
        .reserve(needed - actor.piece_entities.len());
    for i in actor.piece_entities.len()..needed {
        let ent = spawn_composite_piece_entity(commands, &actor.sources, actor.slot.layer, i);
        push_piece_entity(actor, ent, commands);
    }
}

/// World-level counterpart to [`ensure_actor_piece_capacity`] for deferred entity patches.
pub(crate) fn ensure_actor_piece_capacity_world(
    world: &mut World,
    actor_ent: Entity,
    needed: usize,
) {
    let Some((sources, layer, layer_override, start)) = world
        .get::<CharacterActor>(actor_ent)
        .map(|a| {
            (
                a.sources.clone(),
                a.slot.layer,
                a.composite_rt
                    .as_ref()
                    .map(|rt| bevy::camera::visibility::RenderLayers::layer(rt.render_layer)),
                a.piece_entities.len(),
            )
        })
    else {
        return;
    };
    if needed <= start {
        return;
    }
    let mut added = Vec::with_capacity(needed - start);
    for i in start..needed {
        let mut ec = world.spawn(composite_piece_bundle(&sources, layer, i));
        if let Some(ref rl) = layer_override {
            ec.insert(rl.clone());
        }
        added.push(ec.id());
    }
    if let Some(mut actor) = world.get_mut::<CharacterActor>(actor_ent) {
        actor.piece_entities.extend(added);
    }
}

pub fn spawn_single_character_with_underlay(
    commands: &mut Commands,
    loaded: &LoadedCharacter,
    mirror_of: Option<CharacterUnderlayConfig>,
) -> Entity {
    let z_base = stage_layer_z(loaded.slot.layer, 0);
    let sheet_entity = commands
        .spawn((
            make_character_sprite(&loaded.sources),
            Transform::from_xyz(0.0, 0.0, z_base),
            Visibility::Hidden,
            CharacterPartVisual,
        ))
        .id();

    let mut piece_entities = Vec::with_capacity(loaded.max_composite_pieces);
    for i in 0..loaded.max_composite_pieces {
        piece_entities.push(spawn_composite_piece_entity(
            commands,
            &loaded.sources,
            loaded.slot.layer,
            i,
        ));
    }

    let combo_counts = find_count_animations(&loaded.controller, "combo");
    let drop_counts = find_count_animations(&loaded.controller, "drop");
    let mut bopper = Bopper::new(loaded.data.dance_every);
    bopper.name = loaded.data.name.clone();
    bopper.should_bop = false;

    let mut actor = CharacterActor {
        id: loaded.id.clone(),
        role: loaded.role,
        sing_role: loaded.role,
        default_sing_suffix: String::new(),
        slot: loaded.slot.clone(),
        data: loaded.data.clone(),
        controller: loaded.controller.clone(),
        bopper,
        sources: loaded.sources.clone(),
        clips: loaded.clips.clone(),
        deferred_gameover_anims: loaded.deferred_gameover_anims.clone(),
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
        mod_visible: if mirror_of.is_some() { false } else { true },
        mod_z: None,
        script_anim_override: false,
        sheet_entity,
        piece_entities,
        composite_rt: None,
        trail: None,
        trail_entities: Vec::new(),
        mirror_of,
        attachments: Vec::new(),
    };
    if mirror_of.is_some() {
        actor.mod_tint[3] = 0.0;
    }
    actor.reset_character();
    commands.spawn(actor).id()
}

