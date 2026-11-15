use bevy::asset::{AssetPath, embedded_path};
use bevy::core_pipeline::{
    Core2d, Core2dSystems, fullscreen_material::FullscreenMaterial, tonemapping::tonemapping,
};
use bevy::ecs::schedule::{IntoScheduleConfigs, ScheduleConfigs, ScheduleLabel};
use bevy::ecs::system::BoxedSystem;
use bevy::prelude::*;
use bevy::render::RenderApp;
use bevy::render::extract_component::ExtractComponent;
use bevy::render::render_resource::ShaderType;
use bevy::shader::ShaderRef;

use super::defs::{CameraRainSpec, FUNKIN_SCREEN_HEIGHT};

#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Reflect, Debug)]
#[extract_app(RenderApp)]
#[reflect(Component)]
pub struct RainCameraEffect {
    pub bounds: Vec4,
    pub screen_and_scale: Vec4,
    pub time_and_color: Vec4,
}

impl Default for RainCameraEffect {
    fn default() -> Self {
        Self {
            bounds: Vec4::new(0.0, 0.0, 1280.0, FUNKIN_SCREEN_HEIGHT),
            screen_and_scale: Vec4::new(1280.0, FUNKIN_SCREEN_HEIGHT, FUNKIN_SCREEN_HEIGHT / 200.0, 0.0),
            time_and_color: Vec4::new(1.0, 0.4, 0.5019608, 0.8),
        }
    }
}

impl RainCameraEffect {
    pub fn from_spec(spec: CameraRainSpec) -> Self {
        Self {
            bounds: Vec4::new(0.0, 0.0, 1280.0, FUNKIN_SCREEN_HEIGHT),
            screen_and_scale: Vec4::new(
                1280.0,
                FUNKIN_SCREEN_HEIGHT,
                spec.scale,
                spec.start_intensity,
            ),
            time_and_color: Vec4::new(
                1.0,
                spec.rain_color[0],
                spec.rain_color[1],
                spec.rain_color[2],
            ),
        }
    }
}

impl FullscreenMaterial for RainCameraEffect {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("funkin_rain_post.wesl"))
                .with_source("embedded"),
        )
    }

    fn schedule() -> impl ScheduleLabel + Clone {
        Core2d
    }

    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system
            .in_set(Core2dSystems::PostProcess)
            .before(tonemapping)
    }
}
