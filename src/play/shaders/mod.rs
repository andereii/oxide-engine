pub mod defs;
pub mod material;
pub mod rain_post;
pub mod stage_blend_post;
pub mod systems;

use bevy::asset::embedded_asset;
use bevy::core_pipeline::fullscreen_material::FullscreenMaterialPlugin;
use bevy::prelude::*;
use bevy::sprite_render::SpriteMaterialPlugin;

use crate::play::characters::sync_characters;
use crate::play::stage::sync_stage_props;

use material::FunkinSpriteMaterial;
use rain_post::RainCameraEffect;
use stage_blend_post::StageBlendPostPlugin;
use systems::{
    PendingShaderCommands, StageShaderRuntime, attach_stage_and_character_shaders,
    ensure_character_composite_rt, sync_stage_blend_post, update_stage_shaders,
};

pub struct FunkinShadersPlugin {
    pub enabled: bool,
}

impl Default for FunkinShadersPlugin {
    fn default() -> Self {
        Self { enabled: true }
    }
}

fn drain_disabled_shader_queues(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut state: ResMut<crate::play::scene::PlayState>,
    mut pending_cmds: ResMut<PendingShaderCommands>,
    mut runtime: ResMut<StageShaderRuntime>,
    mut actors: Query<&mut crate::play::characters::CharacterActor>,
) {
    if state.phase == crate::play::types::PlayPhase::Loading {
        return;
    }
    state.pending_shader_events.clear();
    pending_cmds.0.clear();
    for mut actor in &mut actors {
        if actor.mirror_of.is_some() {
            continue;
        }
        if !actor.piece_entities.is_empty()
            && actor.data.effective_atlas_settings().use_render_texture
            && actor.composite_rt.is_none()
        {
            ensure_character_composite_rt(&mut actor, &mut commands, &mut images, &mut runtime);
        }
    }
}

impl Plugin for FunkinShadersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingShaderCommands>()
            .init_resource::<StageShaderRuntime>();

        if !self.enabled {
            app.add_systems(Update, drain_disabled_shader_queues);
            return;
        }

        embedded_asset!(app, "funkin_sprite.wesl");
        embedded_asset!(app, "funkin_rain_post.wesl");
        embedded_asset!(app, "funkin_stage_blend_post.wesl");

        app.add_plugins((
            SpriteMaterialPlugin::<FunkinSpriteMaterial>::default(),
            FullscreenMaterialPlugin::<RainCameraEffect>::default(),
            StageBlendPostPlugin,
        ))
        .add_systems(
            Update,
            (
                attach_stage_and_character_shaders,
                ApplyDeferred,
                update_stage_shaders,
                sync_stage_blend_post,
            )
                .chain()
                .after(sync_stage_props)
                .after(sync_characters),
        );
    }
}
