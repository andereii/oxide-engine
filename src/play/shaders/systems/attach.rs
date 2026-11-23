use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use bevy::sprite_render::SpriteMaterial;

use crate::mods::ModRegistry;
use crate::play::camera::hud::{HudCamera, OverlayCamera};
use crate::play::characters::behavior::{
    COMPOSITE_RT_SIZE, CompositeRenderTarget, CompositeRtCamera, compute_composite_rt_dimensions,
};
use crate::play::characters::spawn::CharacterPartVisual;
use crate::play::characters::CharacterActor;
use crate::play::scene::PlayState;
use crate::play::stage::schema::stage_layer_z;
use crate::play::stage::{
    DynamicScriptProp, StageCompositeProp, StageCompositeQuad, StageInitialBlend, StagePropVisual,
};
use crate::play::types::PlayPhase;
use crate::play::PauseOverlayCamera;
use crate::ui::{LoadingOverlayCamera, SoundTrayOverlayCamera};

use super::super::defs::{FunkinBlendMode, SpriteShaderConfig};
use super::super::material::FunkinSpriteMaterial;
use super::super::rain_post::RainCameraEffect;
use super::super::stage_blend_post::{StagePostOverlayKind, StagePostOverlayProp};
use super::{ManagedSpriteShader, ShaderTargetKind, StageShaderRuntime};

fn compute_stage_composite_rt_size(
    composite: &StageCompositeProp,
    base_scale: [f32; 2],
) -> [u32; 2] {
    if !base_scale[0].is_finite()
        || !base_scale[1].is_finite()
        || base_scale[0].abs() <= f32::EPSILON
        || base_scale[1].abs() <= f32::EPSILON
    {
        return [COMPOSITE_RT_SIZE, COMPOSITE_RT_SIZE];
    }
    let scale = [base_scale[0].abs(), base_scale[1].abs()];
    let apply_sm = composite.atlas_settings.apply_stage_matrix;
    let mut max_clip = [0.0_f32, 0.0_f32];
    for clip in composite.clips.values() {
        let mut bw = clip.bounds_size[0];
        let mut bh = clip.bounds_size[1];
        if apply_sm && let Some(sm) = composite.stage_matrix {
            bw *= sm[0].abs().max(0.01);
            bh *= sm[3].abs().max(0.01);
        }
        if bw.is_finite() && bw > 0.0 {
            max_clip[0] = max_clip[0].max(bw * scale[0]);
        }
        if bh.is_finite() && bh > 0.0 {
            max_clip[1] = max_clip[1].max(bh * scale[1]);
        }
    }
    let ref_size = [
        if composite.bounds_size[0].is_finite() && composite.bounds_size[0] > 0.0 {
            composite.bounds_size[0] * scale[0]
        } else {
            0.0
        },
        if composite.bounds_size[1].is_finite() && composite.bounds_size[1] > 0.0 {
            composite.bounds_size[1] * scale[1]
        } else {
            0.0
        },
    ];
    compute_composite_rt_dimensions(ref_size, max_clip)
}

/// Allocates an offscreen `Rgba8UnormSrgb` `CompositeRenderTarget`, binds `piece_entities`
/// onto its private `RenderLayers`, and clears any stale per-piece sprite shader components.
#[allow(clippy::too_many_arguments)]
fn allocate_composite_rt(
    rt_slot: &mut Option<CompositeRenderTarget>,
    piece_entities: &[Entity],
    rt_size: [u32; 2],
    z_base: f32,
    is_char: bool,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    runtime: &mut StageShaderRuntime,
) -> Entity {
    if let Some(ref rt) = *rt_slot {
        return rt.quad_entity;
    }
    let render_layer = runtime.next_rt_layer;
    runtime.next_rt_layer = runtime.next_rt_layer.saturating_add(1);
    let rt_image = images.add(Image::new_target_texture(
        rt_size[0],
        rt_size[1],
        TextureFormat::Rgba8UnormSrgb,
        None,
    ));
    let layer = RenderLayers::layer(render_layer);
    for &piece_ent in piece_entities {
        if piece_ent != Entity::PLACEHOLDER {
            commands
                .entity(piece_ent)
                .insert(layer.clone())
                .remove::<(SpriteMaterial<FunkinSpriteMaterial>, ManagedSpriteShader)>();
        }
    }
    let cam_order = -10 - (render_layer as isize - 10);
    let camera_entity = commands
        .spawn((
            Camera2d,
            Camera {
                order: cam_order,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::from(rt_image.clone()),
            Msaa::Off,
            Tonemapping::None,
            CompositeRtCamera,
            layer,
        ))
        .id();
    let mut quad_ec = commands.spawn((
        Sprite::from_image(rt_image.clone()),
        Transform::from_xyz(0.0, 0.0, z_base),
        Visibility::Hidden,
    ));
    if is_char {
        quad_ec.insert(CharacterPartVisual);
    } else {
        quad_ec.insert(StageCompositeQuad);
    }
    let quad_entity = quad_ec.id();
    *rt_slot = Some(CompositeRenderTarget {
        image: rt_image,
        camera_entity,
        quad_entity,
        render_layer,
    });
    quad_entity
}

pub(crate) fn ensure_character_composite_rt(
    actor: &mut CharacterActor,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    runtime: &mut StageShaderRuntime,
) -> Entity {
    let z_base = actor
        .mod_z
        .unwrap_or_else(|| stage_layer_z(actor.slot.layer, 0));
    let rt_size = actor.composite_rt_size();
    let piece_entities = actor.piece_entities.clone();
    allocate_composite_rt(
        &mut actor.composite_rt,
        &piece_entities,
        rt_size,
        z_base,
        true,
        commands,
        images,
        runtime,
    )
}

pub(super) fn ensure_stage_composite_rt_scaled(
    composite: &mut StageCompositeProp,
    z_base: f32,
    base_scale: [f32; 2],
    commands: &mut Commands,
    images: &mut Assets<Image>,
    runtime: &mut StageShaderRuntime,
) -> Entity {
    let rt_size = compute_stage_composite_rt_size(composite, base_scale);
    let piece_entities = composite.piece_entities.clone();
    allocate_composite_rt(
        &mut composite.composite_rt,
        &piece_entities,
        rt_size,
        z_base,
        false,
        commands,
        images,
        runtime,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn attach_stage_and_character_shaders(
    mut commands: Commands,
    mut state: ResMut<PlayState>,
    _mods: Res<ModRegistry>,
    _asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<FunkinSpriteMaterial>>,
    mut runtime: ResMut<StageShaderRuntime>,
    mut props: Query<
        (
            Entity,
            &StagePropVisual,
            Option<&mut StageCompositeProp>,
            Option<&StageInitialBlend>,
        ),
        (Without<ManagedSpriteShader>, Without<DynamicScriptProp>),
    >,
    mut actors: Query<&mut CharacterActor>,
    cameras: Query<
        (Entity, Option<&RainCameraEffect>),
        (
            With<Camera2d>,
            With<IsDefaultUiCamera>,
            Without<HudCamera>,
            Without<OverlayCamera>,
            Without<PauseOverlayCamera>,
            Without<LoadingOverlayCamera>,
            Without<SoundTrayOverlayCamera>,
            Without<CompositeRtCamera>,
        ),
    >,
) {
    if state.phase == PlayPhase::Loading {
        runtime.attached = false;
        return;
    }

    let stage_id = state.chart.stage_id.clone();
    let song_id = state.chart.song_id.clone();
    if runtime.attached && runtime.stage_id == stage_id && runtime.song_id == song_id {
        return;
    }

    runtime.reset_for_stage(&stage_id, &song_id);
    runtime.attached = true;

    for (entity, prop, composite_opt, initial_blend_opt) in &mut props {
        let mut initial_blend: Option<FunkinBlendMode> = None;
        if let Some(StageInitialBlend(mode)) = initial_blend_opt {
            commands.entity(entity).remove::<StageInitialBlend>();
            if let Some(kind) = StagePostOverlayKind::parse(mode) {
                commands.entity(entity).insert(StagePostOverlayProp { kind });
                continue;
            }
            initial_blend = Some(FunkinBlendMode::parse(mode));
        }

        let Some(blend) = initial_blend.filter(|&b| b != FunkinBlendMode::Normal) else {
            continue;
        };
        let cfg = SpriteShaderConfig::blend_only(blend);

        if let Some(composite) = composite_opt {
            let handle = materials.add(FunkinSpriteMaterial::from_config(cfg, None));
            for &piece_ent in &composite.piece_entities {
                if piece_ent != Entity::PLACEHOLDER {
                    commands
                        .entity(piece_ent)
                        .insert(SpriteMaterial(handle.clone()));
                }
            }
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: ShaderTargetKind::StageProp,
                    char_id: String::new(),
                    prop_name: prop.name.clone(),
                    aux_path: None,
                },
            ));
        } else {
            let handle = materials.add(FunkinSpriteMaterial::from_config(cfg, None));
            commands.entity(entity).insert((
                SpriteMaterial(handle.clone()),
                ManagedSpriteShader {
                    handle,
                    target: ShaderTargetKind::StageProp,
                    char_id: String::new(),
                    prop_name: prop.name.clone(),
                    aux_path: None,
                },
            ));
        }
    }

    for mut actor in &mut actors {
        if actor.mirror_of.is_some() {
            continue;
        }
        if !actor.piece_entities.is_empty()
            && actor.data.effective_atlas_settings().use_render_texture
        {
            ensure_character_composite_rt(&mut actor, &mut commands, &mut images, &mut runtime);
        }
    }

    if let Some(spec) = runtime.rain_spec {
        for (cam_entity, existing) in &cameras {
            if existing.is_none() {
                commands
                    .entity(cam_entity)
                    .insert(RainCameraEffect::from_spec(spec));
            }
        }
    }
    state.pending_shader_events.clear();
}

#[cfg(test)]
mod tests {
    use super::super::update::{sync_stage_blend_post, update_stage_shaders};
    use super::super::{PendingShaderCommands, StageShaderRuntime};
    use super::*;
    use crate::animations::AtlasSettings;
    use crate::play::stage::schema::StageCoordMode;
    use crate::play::types::PlayStateParams;
    use std::collections::HashMap;

    fn test_prop_visual(name: &str) -> StagePropVisual {
        StagePropVisual {
            name: name.to_string(),
            base_position: [0.0, 0.0],
            base_scale: [1.0, 1.0],
            scroll_factor: [1.0, 1.0],
            base_flip: [false, false],
            base_angle_deg: 0.0,
            z_layer: 0.0,
            visible: true,
            coord_mode: StageCoordMode::Center,
            raw_size: [100.0, 100.0],
            velocity: [0.0, 0.0],
            acceleration: [0.0, 0.0],
            repeat: [false, false],
            custom_affine: None,
            overlay: false,
        }
    }

    #[test]
    fn shader_systems_have_disjoint_ecs_queries() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Shader>()
            .init_asset::<FunkinSpriteMaterial>()
            .insert_resource(PlayState::new(PlayStateParams::default()))
            .insert_resource(ModRegistry::default())
            .init_resource::<StageShaderRuntime>()
            .init_resource::<PendingShaderCommands>()
            .add_systems(
                Update,
                (
                    attach_stage_and_character_shaders,
                    update_stage_shaders,
                    sync_stage_blend_post,
                ),
            );
        app.update();
    }

    #[test]
    fn stage_initial_blend_applies_post_and_forward_modes_before_removal() {
        let mut app = App::new();
        let mut state = PlayState::new(PlayStateParams::default());
        state.phase = PlayPhase::Playing;
        state.chart.stage_id = "test_stage".to_string();

        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Shader>()
            .init_asset::<FunkinSpriteMaterial>()
            .insert_resource(state)
            .insert_resource(ModRegistry::default())
            .init_resource::<StageShaderRuntime>()
            .init_resource::<PendingShaderCommands>()
            .add_systems(Update, attach_stage_and_character_shaders);

        let ov = app
            .world_mut()
            .spawn((test_prop_visual("ov"), StageInitialBlend("overlay".into())))
            .id();
        let hl = app
            .world_mut()
            .spawn((test_prop_visual("hl"), StageInitialBlend("hard_light".into())))
            .id();
        let sa = app
            .world_mut()
            .spawn((test_prop_visual("sa"), StageInitialBlend("srgb_add".into())))
            .id();
        let add = app
            .world_mut()
            .spawn((test_prop_visual("add"), StageInitialBlend("add".into())))
            .id();
        let mult = app
            .world_mut()
            .spawn((test_prop_visual("mult"), StageInitialBlend("multiply".into())))
            .id();
        let scr = app
            .world_mut()
            .spawn((test_prop_visual("scr"), StageInitialBlend("screen".into())))
            .id();
        let norm = app
            .world_mut()
            .spawn((test_prop_visual("norm"), StageInitialBlend("normal".into())))
            .id();

        app.update();

        let world = app.world();
        for ent in [ov, hl, sa, add, mult, scr, norm] {
            assert!(!world.entity(ent).contains::<StageInitialBlend>());
        }
        assert_eq!(
            world.entity(ov).get::<StagePostOverlayProp>().map(|p| p.kind),
            Some(StagePostOverlayKind::Overlay)
        );
        assert_eq!(
            world.entity(hl).get::<StagePostOverlayProp>().map(|p| p.kind),
            Some(StagePostOverlayKind::HardLight)
        );
        assert_eq!(
            world.entity(sa).get::<StagePostOverlayProp>().map(|p| p.kind),
            Some(StagePostOverlayKind::SrgbAdd)
        );
        assert!(!world.entity(ov).contains::<ManagedSpriteShader>());

        let mats = world.resource::<Assets<FunkinSpriteMaterial>>();
        for (ent, expected) in [
            (add, FunkinBlendMode::Add),
            (mult, FunkinBlendMode::Multiply),
            (scr, FunkinBlendMode::Screen),
        ] {
            assert!(!world.entity(ent).contains::<StagePostOverlayProp>());
            let managed = world
                .entity(ent)
                .get::<ManagedSpriteShader>()
                .expect("forward blend prop should attach ManagedSpriteShader");
            assert_eq!(
                mats.get(&managed.handle).map(|m| m.config.blend_mode),
                Some(expected)
            );
        }
        assert!(!world.entity(norm).contains::<StagePostOverlayProp>());
        assert!(!world.entity(norm).contains::<ManagedSpriteShader>());
    }

    #[test]
    fn stage_composite_rt_size_uses_scaled_bounds_and_fallback() {
        let mut comp = StageCompositeProp {
            atlas_dir: None,
            textures: Vec::new(),
            clips: HashMap::new(),
            bounds_min: [0.0, 0.0],
            bounds_size: [400.0, 500.0],
            atlas_settings: AtlasSettings::default(),
            stage_matrix: None,
            piece_entities: Vec::new(),
            composite_rt: None,
        };
        assert_eq!(
            compute_stage_composite_rt_size(&comp, [1.5, 2.0]),
            [728, 1128]
        );
        assert_eq!(
            compute_stage_composite_rt_size(&comp, [0.0, 1.0]),
            [COMPOSITE_RT_SIZE, COMPOSITE_RT_SIZE]
        );
        comp.bounds_size = [-10.0, f32::NAN];
        assert_eq!(
            compute_stage_composite_rt_size(&comp, [1.0, 1.0]),
            [COMPOSITE_RT_SIZE, COMPOSITE_RT_SIZE]
        );
    }
}
