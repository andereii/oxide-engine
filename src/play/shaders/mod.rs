pub mod defs;
pub mod material;
pub mod rain_post;
pub mod systems;

use bevy::asset::embedded_asset;
use bevy::core_pipeline::fullscreen_material::FullscreenMaterialPlugin;
use bevy::prelude::*;
use bevy::sprite_render::SpriteMaterialPlugin;

use crate::play::characters::sync_characters;
use crate::play::stage::sync_stage_props;

use material::FunkinSpriteMaterial;
use rain_post::RainCameraEffect;
use systems::{
    PendingShaderCommands, StageShaderRuntime, attach_stage_and_character_shaders,
    update_stage_shaders,
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
    mut state: ResMut<crate::play::scene::PlayState>,
    mut pending_cmds: ResMut<PendingShaderCommands>,
) {
    state.pending_shader_events.clear();
    pending_cmds.0.clear();
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

        app.add_plugins(SpriteMaterialPlugin::<FunkinSpriteMaterial>::default())
            .add_plugins(FullscreenMaterialPlugin::<RainCameraEffect>::default())
            .add_systems(
                Update,
                (attach_stage_and_character_shaders, update_stage_shaders)
                    .chain()
                    .after(sync_stage_props)
                    .after(sync_characters),
            );
    }
}
