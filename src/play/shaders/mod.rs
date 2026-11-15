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

pub struct FunkinShadersPlugin;

impl Plugin for FunkinShadersPlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "funkin_sprite.wesl");
        embedded_asset!(app, "funkin_rain_post.wesl");

        app.init_resource::<PendingShaderCommands>()
            .init_resource::<StageShaderRuntime>()
            .add_plugins((
                SpriteMaterialPlugin::<FunkinSpriteMaterial>::default(),
                FullscreenMaterialPlugin::<RainCameraEffect>::default(),
            ))
            .add_systems(
                Update,
                (attach_stage_and_character_shaders, update_stage_shaders)
                    .chain()
                    .after(sync_stage_props)
                    .after(sync_characters),
            );
    }
}
