mod composite;
mod spawn;
mod systems;

use std::collections::HashMap;

use bevy::prelude::*;

use crate::animations::AtlasSettings;
use crate::animations::atlas_composite::CompositeClip;
use crate::play::characters::behavior::CompositeRenderTarget;

use super::schema::StageCoordMode;

pub use spawn::spawn_stage_props;
pub use systems::{sync_script_texts, sync_stage_props};

#[derive(Component)]
pub struct StageCompositeQuad;

#[derive(Component)]
pub struct DynamicScriptProp {
    pub id: String,
    pub hud: bool,
}

#[derive(Component)]
pub struct DynamicScriptText {
    pub id: String,
    pub hud: bool,
    pub base_position: [f32; 2],
    pub scroll_factor: [f32; 2],
    pub z_layer: f32,
    pub visible: bool,
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
    pub composite_rt: Option<CompositeRenderTarget>,
}

impl StageCompositeProp {
    /// Despawns the backing offscreen composite RT camera and quad if allocated.
    pub fn despawn_composite_rt(&self, commands: &mut Commands) {
        if let Some(ref rt) = self.composite_rt {
            rt.despawn(commands);
        }
    }
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
    pub velocity: [f32; 2],
    pub acceleration: [f32; 2],
}

#[derive(Component, Debug, Clone)]
pub struct StageInitialBlend(pub String);

#[derive(Component)]
pub struct StagePropPixelSampler {
    pub is_pixel: bool,
    pub sampler_applied: bool,
}

#[derive(Resource, Default)]
pub struct StageSyncState {
    pub last_step: Option<i32>,
}
