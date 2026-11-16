use bevy::asset::{AssetPath, embedded_path};
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, RenderPipelineDescriptor,
    ShaderType, SpecializedMeshPipelineError,
};
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2dKey, Material2dPipeline, MaterialExtension2d};

use super::defs::{
    FunkinBlendMode, SHADER_FLAG_HAS_AUX_TEX, SHADER_FLAG_IS_CHAR,
    SHADER_FLAG_PREMULTIPLIED_INPUT, SHADER_FLAG_USE_ALT_MASK, SpriteShaderConfig,
};

#[derive(Clone, Copy, ShaderType, Debug, Reflect, PartialEq)]
pub struct FunkinSpriteUniform {
    pub flags: UVec4,
    pub time_and_extra: Vec4,
    pub wiggle_and_rain: Vec4,
    pub hue_col0: Vec4,
    pub hue_col1: Vec4,
    pub hue_col2: Vec4,
    pub sat_col0: Vec4,
    pub sat_col1: Vec4,
    pub sat_col2: Vec4,
    pub drop_color_and_mask: Vec4,
    pub drop_angle_and_rain: Vec4,
    pub rain_b_and_ss_hsbc: Vec4,
    pub ss_params: Vec4,
    pub ss_light: Vec4,
    pub custom_0: Vec4,
    pub custom_1: Vec4,
    pub custom_2: Vec4,
    pub custom_3: Vec4,
}

impl Default for FunkinSpriteUniform {
    fn default() -> Self {
        Self {
            flags: UVec4::ZERO,
            time_and_extra: Vec4::ZERO,
            wiggle_and_rain: Vec4::ZERO,
            hue_col0: Vec4::new(1.0, 0.0, 0.0, 0.0),
            hue_col1: Vec4::new(0.0, 1.0, 0.0, 1.0),
            hue_col2: Vec4::new(0.0, 0.0, 1.0, 0.0),
            sat_col0: Vec4::new(1.0, 0.0, 0.0, 15.0),
            sat_col1: Vec4::new(0.0, 1.0, 0.0, 1.0),
            sat_col2: Vec4::new(0.0, 0.0, 1.0, 0.1),
            drop_color_and_mask: Vec4::new(1.0, 1.0, 1.0, 0.5),
            drop_angle_and_rain: Vec4::new(1.0, 0.0, 0.4, 0.5),
            rain_b_and_ss_hsbc: Vec4::new(0.8, 0.0, 0.0, 0.0),
            ss_params: Vec4::ZERO,
            ss_light: Vec4::ONE,
            custom_0: Vec4::ZERO,
            custom_1: Vec4::ZERO,
            custom_2: Vec4::ZERO,
            custom_3: Vec4::ZERO,
        }
    }
}

impl FunkinSpriteUniform {
    pub fn from_config(
        cfg: &SpriteShaderConfig,
        time_s: f32,
        has_aux_tex: bool,
        premultiplied_input: bool,
    ) -> Self {
        let mut extra_flags = cfg.wiggle.effect_type as u32;
        if cfg.drop_shadow.use_alt_mask {
            extra_flags |= SHADER_FLAG_USE_ALT_MASK;
        }
        if cfg.sserafim.is_char {
            extra_flags |= SHADER_FLAG_IS_CHAR;
        }
        if has_aux_tex {
            extra_flags |= SHADER_FLAG_HAS_AUX_TEX;
        }
        if premultiplied_input {
            extra_flags |= SHADER_FLAG_PREMULTIPLIED_INPUT;
        }

        let hm = cfg.adjust.hue_matrix();
        let sm = cfg.adjust.saturation_matrix();
        let total_ang = (cfg.drop_shadow.angle_deg + cfg.drop_shadow.angle_offset_deg).to_radians();
        let (ang_sin, ang_cos) = total_ang.sin_cos();

        Self {
            flags: UVec4::new(cfg.mode_flags, extra_flags, cfg.blend_mode as u32, 0),
            time_and_extra: Vec4::new(
                time_s,
                cfg.building.alpha_shit,
                cfg.wiggle.speed,
                cfg.wiggle.frequency,
            ),
            wiggle_and_rain: Vec4::new(
                cfg.wiggle.amplitude,
                cfg.rain_sprite.scale,
                cfg.rain_sprite.intensity,
                0.0,
            ),
            hue_col0: Vec4::new(hm[0][0], hm[0][1], hm[0][2], cfg.adjust.brightness_norm()),
            hue_col1: Vec4::new(hm[1][0], hm[1][1], hm[1][2], cfg.adjust.contrast_norm()),
            hue_col2: Vec4::new(hm[2][0], hm[2][1], hm[2][2], 0.0),
            sat_col0: Vec4::new(sm[0][0], sm[0][1], sm[0][2], cfg.drop_shadow.distance),
            sat_col1: Vec4::new(sm[1][0], sm[1][1], sm[1][2], cfg.drop_shadow.strength),
            sat_col2: Vec4::new(sm[2][0], sm[2][1], sm[2][2], cfg.drop_shadow.threshold),
            drop_color_and_mask: Vec4::new(
                cfg.drop_shadow.color[0],
                cfg.drop_shadow.color[1],
                cfg.drop_shadow.color[2],
                cfg.drop_shadow.mask_threshold,
            ),
            drop_angle_and_rain: Vec4::new(
                ang_cos,
                ang_sin,
                cfg.rain_sprite.rain_color[0],
                cfg.rain_sprite.rain_color[1],
            ),
            rain_b_and_ss_hsbc: Vec4::new(
                cfg.rain_sprite.rain_color[2],
                cfg.sserafim.adjust.hue,
                cfg.sserafim.adjust.saturation,
                cfg.sserafim.adjust.brightness,
            ),
            ss_params: Vec4::new(
                cfg.sserafim.adjust.contrast,
                cfg.sserafim.darken_amount,
                cfg.sserafim.pulse_strength,
                cfg.sserafim.truck_strength,
            ),
            ss_light: Vec4::new(
                cfg.sserafim.light_color[0],
                cfg.sserafim.light_color[1],
                cfg.sserafim.light_color[2],
                0.0,
            ),
            custom_0: Vec4::from_array(cfg.custom_params[0]),
            custom_1: Vec4::from_array(cfg.custom_params[1]),
            custom_2: Vec4::from_array(cfg.custom_params[2]),
            custom_3: Vec4::from_array(cfg.custom_params[3]),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug, Reflect)]
pub struct FunkinSpriteMaterialKey {
    pub blend_mode: FunkinBlendMode,
    pub custom_shader_uuid: Option<[u8; 16]>,
}

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
#[bind_group_data(FunkinSpriteMaterialKey)]
pub struct FunkinSpriteMaterial {
    #[uniform(20)]
    pub uniform: FunkinSpriteUniform,
    #[texture(21)]
    #[sampler(22)]
    pub aux_texture: Option<Handle<Image>>,
    #[reflect(ignore)]
    pub config: SpriteShaderConfig,
    #[reflect(ignore)]
    pub custom_shader: Option<Handle<Shader>>,
    pub time_s: f32,
    pub premultiplied_input: bool,
}

impl From<&FunkinSpriteMaterial> for FunkinSpriteMaterialKey {
    fn from(material: &FunkinSpriteMaterial) -> Self {
        let custom_shader_uuid = material.custom_shader.as_ref().and_then(|h| match h.id() {
            bevy::asset::AssetId::Uuid { uuid } => Some(*uuid.as_bytes()),
            bevy::asset::AssetId::Index { .. } => None,
        });
        Self {
            blend_mode: material.config.blend_mode,
            custom_shader_uuid,
        }
    }
}

impl FunkinSpriteMaterial {
    pub fn from_config(config: SpriteShaderConfig, aux_texture: Option<Handle<Image>>) -> Self {
        Self::from_config_ex(config, aux_texture, false)
    }

    pub fn from_config_ex(
        config: SpriteShaderConfig,
        aux_texture: Option<Handle<Image>>,
        premultiplied_input: bool,
    ) -> Self {
        let uniform = FunkinSpriteUniform::from_config(
            &config,
            0.0,
            aux_texture.is_some(),
            premultiplied_input,
        );
        Self {
            uniform,
            aux_texture,
            config,
            custom_shader: None,
            time_s: 0.0,
            premultiplied_input,
        }
    }

    pub fn sync_uniform(&mut self) {
        self.uniform = FunkinSpriteUniform::from_config(
            &self.config,
            self.time_s,
            self.aux_texture.is_some(),
            self.premultiplied_input,
        );
    }
}

impl MaterialExtension2d for FunkinSpriteMaterial {
    fn fragment_shader() -> Option<ShaderRef> {
        Some(ShaderRef::Path(
            AssetPath::from_path_buf(embedded_path!("funkin_sprite.wesl")).with_source("embedded"),
        ))
    }

    fn alpha_mode(&self) -> Option<AlphaMode2d> {
        Some(AlphaMode2d::Blend)
    }

    fn specialize(
        _pipeline: &Material2dPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: Material2dKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        if let Some(fragment) = descriptor.fragment.as_mut() {
            if let Some(uuid_bytes) = key.bind_group_data.custom_shader_uuid {
                fragment.shader = Handle::Uuid(
                    bevy::asset::uuid::Uuid::from_bytes(uuid_bytes),
                    std::marker::PhantomData,
                );
            }
            if let Some(Some(target)) = fragment.targets.first_mut() {
            target.blend = Some(match key.bind_group_data.blend_mode {
                FunkinBlendMode::Normal => BlendState::ALPHA_BLENDING,
                FunkinBlendMode::Add => BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::SrcAlpha,
                        dst_factor: BlendFactor::One,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent::OVER,
                },
                FunkinBlendMode::Multiply => BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::Dst,
                        dst_factor: BlendFactor::OneMinusSrcAlpha,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent::OVER,
                },
                FunkinBlendMode::Screen => BlendState {
                    color: BlendComponent {
                        src_factor: BlendFactor::One,
                        dst_factor: BlendFactor::OneMinusSrc,
                        operation: BlendOperation::Add,
                    },
                    alpha: BlendComponent::OVER,
                },
            });
            }
        }
        Ok(())
    }
}
