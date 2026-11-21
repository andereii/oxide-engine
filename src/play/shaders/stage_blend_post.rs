use bevy::asset::{AssetId, AssetPath, embedded_path};
use bevy::core_pipeline::{
    Core2d, Core2dSystems, FullscreenShader,
    fullscreen_material::fullscreen_material_system,
    tonemapping::tonemapping,
};
use bevy::prelude::*;
use bevy::render::{
    Render, RenderApp, RenderStartup, RenderSystems,
    camera::ExtractedCamera,
    diagnostic::RecordDiagnostics,
    extract_component::{
        ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin,
        UniformComponentPlugin,
    },
    render_asset::RenderAssets,
    render_resource::{
        AddressMode, BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, Buffer,
        BufferId, CachedRenderPipelineId, Canonical, ColorTargetState, ColorWrites, FilterMode,
        FragmentState, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor,
        RenderPipeline, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
        ShaderStages, ShaderType, Specializer, SpecializerKey, TextureFormat, TextureSampleType,
        TextureView, Variants,
        binding_types::{sampler, texture_2d, uniform_buffer},
    },
    renderer::{RenderContext, RenderDevice, ViewQuery},
    texture::{FallbackImage, GpuImage},
    view::{
        ExtractedView, PostProcessBindGroupCache, PostProcessBindGroupCacheBuilder, ViewTarget,
    },
};

use super::rain_post::RainCameraEffect;

/// Maximum number of active `StagePostOverlayProp` layers composited in a single post-process pass.
/// Exceeding this limit logs a warning in `sync_stage_blend_post` and drops higher-Z overlays.
pub const MAX_STAGE_POST_OVERLAYS: usize = 4;

/// Blend mode handled by the stage camera post-process pass in sRGB space.
///
/// OpenFL / Flash (`BlendMode.OVERLAY`, `BlendMode.HARDLIGHT`, `BlendMode.ADD`) evaluates these
/// blend formulas directly on gamma-encoded sRGB `[0, 1]` channels rather than linear light.
/// Consequently, the post-process shader converts the scene and overlay samples to sRGB, blends
/// in sRGB, and converts back to linear light for output into a `TextureFormat::Rgba8UnormSrgb`
/// render target (which performs hardware linear-to-sRGB encoding on write). Using a linear
/// `Unorm` target or skipping the sRGB conversion would mis-encode gamma and break OpenFL parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum StagePostOverlayKind {
    Overlay = 0,
    HardLight = 1,
    SrgbAdd = 2,
}

impl StagePostOverlayKind {
    /// Parses a stage blend string that requires framebuffer / sRGB stage compositing.
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "overlay" => Some(Self::Overlay),
            "hardlight" | "hard_light" => Some(Self::HardLight),
            "srgb_add" | "srgb-add" => Some(Self::SrgbAdd),
            _ => None,
        }
    }
}

/// Marker placed on a `StagePropVisual` so its sprite renders via `StageBlendPostPlugin`
/// instead of normal forward sprite draw.
#[derive(Component, Debug, Clone, Copy)]
pub struct StagePostOverlayProp {
    pub kind: StagePostOverlayKind,
}

#[derive(Clone, Copy, ShaderType, Default, Debug, PartialEq)]
pub struct StageOverlaySlot {
    pub rect: Vec4,
    pub params: Vec4,
    pub tint_and_flip: Vec4,
}

#[derive(Component, ExtractComponent, Clone, Copy, ShaderType, Default, Debug, PartialEq)]
#[extract_app(RenderApp)]
pub struct StageBlendPostUniform {
    pub camera: Vec4,
    pub screen: Vec4,
    pub slots: [StageOverlaySlot; MAX_STAGE_POST_OVERLAYS],
}

#[derive(Component, ExtractComponent, Clone, Default, Debug, PartialEq, Eq)]
#[extract_app(RenderApp)]
pub struct StageBlendPostImages {
    pub ids: [Option<AssetId<Image>>; MAX_STAGE_POST_OVERLAYS],
}

#[derive(Resource)]
struct StageBlendPostPipeline {
    layout: BindGroupLayoutDescriptor,
    screen_sampler: Sampler,
    ov_sampler: Sampler,
    variants: Variants<RenderPipeline, StageBlendPostSpecializer>,
}

struct StageBlendPostSpecializer;

#[derive(PartialEq, Eq, Hash, Clone, Copy, SpecializerKey)]
struct StageBlendPostKey {
    target_format: TextureFormat,
}

impl Specializer<RenderPipeline> for StageBlendPostSpecializer {
    type Key = StageBlendPostKey;

    fn specialize(
        &self,
        key: Self::Key,
        descriptor: &mut RenderPipelineDescriptor,
    ) -> Result<Canonical<Self::Key>, BevyError> {
        let fragment = descriptor.fragment_mut()?;
        fragment.set_target(
            0,
            ColorTargetState {
                format: key.target_format,
                blend: None,
                write_mask: ColorWrites::ALL,
            },
        );
        Ok(key)
    }
}

#[derive(Component)]
struct StageBlendPostPipelineId(CachedRenderPipelineId);

#[derive(Component)]
struct StageBlendPostBindGroups {
    buffer_id: Option<BufferId>,
    ids: [Option<AssetId<Image>>; MAX_STAGE_POST_OVERLAYS],
    cache: PostProcessBindGroupCache,
}

pub struct StageBlendPostPlugin;

impl Plugin for StageBlendPostPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            ExtractComponentPlugin::<StageBlendPostUniform>::default(),
            UniformComponentPlugin::<StageBlendPostUniform>::default(),
            ExtractComponentPlugin::<StageBlendPostImages>::default(),
        ));

        let Some(render_app) = app.get_sub_app_mut(RenderApp) else {
            return;
        };

        render_app
            .add_systems(RenderStartup, init_stage_blend_post_pipeline)
            .add_systems(
                Render,
                (
                    prepare_stage_blend_post_pipelines.in_set(RenderSystems::Prepare),
                    prepare_stage_blend_post_bind_groups.in_set(RenderSystems::PrepareBindGroups),
                ),
            )
            .add_systems(
                Core2d,
                stage_blend_post_system
                    .in_set(Core2dSystems::PostProcess)
                    .before(tonemapping)
                    .before(fullscreen_material_system::<RainCameraEffect>),
            );
    }
}

fn init_stage_blend_post_pipeline(
    mut commands: Commands,
    render_device: Res<RenderDevice>,
    asset_server: Res<AssetServer>,
    fullscreen_shader: Res<FullscreenShader>,
) {
    let tex_entry = texture_2d(TextureSampleType::Float { filterable: true });
    let layout = BindGroupLayoutDescriptor::new(
        "stage_blend_post_bind_group_layout",
        &BindGroupLayoutEntries::sequential(
            ShaderStages::FRAGMENT,
            (
                tex_entry,
                sampler(SamplerBindingType::Filtering),
                uniform_buffer::<StageBlendPostUniform>(true),
                tex_entry,
                tex_entry,
                tex_entry,
                tex_entry,
                sampler(SamplerBindingType::Filtering),
            ),
        ),
    );
    let screen_sampler = render_device.create_sampler(&SamplerDescriptor::default());
    let ov_sampler = render_device.create_sampler(&SamplerDescriptor {
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        ..default()
    });
    let shader: Handle<Shader> = asset_server.load(
        AssetPath::from_path_buf(embedded_path!("funkin_stage_blend_post.wesl"))
            .with_source("embedded"),
    );
    let desc = RenderPipelineDescriptor {
        label: Some("stage_blend_post_pipeline".into()),
        layout: vec![layout.clone()],
        vertex: fullscreen_shader.to_vertex_state(),
        fragment: Some(FragmentState {
            shader,
            // Base pipeline target uses `Rgba8UnormSrgb` (before `StageBlendPostSpecializer`
            // specializes to `view.target_format`) so hardware sRGB write-encoding matches
            // Bevy's 2D sRGB view target and `funkin_stage_blend_post.wesl`'s `srgb_to_linear`
            // output, preserving OpenFL gamma-space overlay/hardlight/srgb_add parity.
            targets: vec![Some(ColorTargetState {
                format: TextureFormat::Rgba8UnormSrgb,
                blend: None,
                write_mask: ColorWrites::ALL,
            })],
            ..default()
        }),
        ..default()
    };

    commands.insert_resource(StageBlendPostPipeline {
        layout,
        screen_sampler,
        ov_sampler,
        variants: Variants::new(StageBlendPostSpecializer, desc),
    });
}

fn prepare_stage_blend_post_pipelines(
    mut commands: Commands,
    pipeline_cache: Res<PipelineCache>,
    mut pipeline: ResMut<StageBlendPostPipeline>,
    views: Query<(Entity, &ExtractedView, Option<&StageBlendPostUniform>), With<ExtractedCamera>>,
) -> Result<(), BevyError> {
    for (entity, view, uniform) in &views {
        if uniform.is_none_or(|u| u.camera.w < 0.5) {
            commands.entity(entity).remove::<StageBlendPostPipelineId>();
            continue;
        }
        let id = pipeline.variants.specialize(
            &pipeline_cache,
            StageBlendPostKey {
                target_format: view.target_format,
            },
        )?;
        commands.entity(entity).insert(StageBlendPostPipelineId(id));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn prepare_stage_blend_post_bind_groups(
    mut commands: Commands,
    mut views: Query<(
        Entity,
        &ViewTarget,
        Option<&mut StageBlendPostBindGroups>,
        Option<&StageBlendPostUniform>,
        Option<&StageBlendPostImages>,
    )>,
    pipeline: Option<Res<StageBlendPostPipeline>>,
    pipeline_cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<StageBlendPostUniform>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    fallback: Res<FallbackImage>,
    render_device: Res<RenderDevice>,
) {
    let Some(pipeline) = pipeline else {
        return;
    };
    let Some(settings_binding) = uniforms.uniforms().binding() else {
        return;
    };
    let cur_buffer_id = uniforms.uniforms().buffer().map(Buffer::id);

    let resolve_view = |id_opt: Option<AssetId<Image>>| -> &TextureView {
        id_opt
            .and_then(|id| gpu_images.get(id))
            .map_or(&fallback.d2.texture_view, |img| &img.texture_view)
    };

    for (entity, view_target, mut existing, uniform, images) in &mut views {
        let Some(u) = uniform.filter(|u| u.camera.w >= 0.5) else {
            commands.entity(entity).remove::<StageBlendPostBindGroups>();
            continue;
        };
        let ids = images.map_or([None; MAX_STAGE_POST_OVERLAYS], |i| i.ids);
        let ready_ids = [
            if u.slots[0].params.w >= -0.5 { ids[0].filter(|id| gpu_images.get(*id).is_some()) } else { None },
            if u.slots[1].params.w >= -0.5 { ids[1].filter(|id| gpu_images.get(*id).is_some()) } else { None },
            if u.slots[2].params.w >= -0.5 { ids[2].filter(|id| gpu_images.get(*id).is_some()) } else { None },
            if u.slots[3].params.w >= -0.5 { ids[3].filter(|id| gpu_images.get(*id).is_some()) } else { None },
        ];

        let ov0 = resolve_view(ready_ids[0]);
        let ov1 = resolve_view(ready_ids[1]);
        let ov2 = resolve_view(ready_ids[2]);
        let ov3 = resolve_view(ready_ids[3]);

        let builder = PostProcessBindGroupCacheBuilder::new(|screen_tex: &TextureView| {
            (
                screen_tex.id(),
                render_device.create_bind_group(
                    "stage_blend_post_bind_group",
                    &pipeline_cache.get_bind_group_layout(&pipeline.layout),
                    &BindGroupEntries::sequential((
                        screen_tex,
                        &pipeline.screen_sampler,
                        settings_binding.clone(),
                        ov0,
                        ov1,
                        ov2,
                        ov3,
                        &pipeline.ov_sampler,
                    )),
                ),
            )
        });

        if let Some(bg) = &mut existing {
            if bg.buffer_id != cur_buffer_id || bg.ids != ready_ids {
                bg.buffer_id = cur_buffer_id;
                bg.ids = ready_ids;
                bg.cache = builder.generate_bind_groups(view_target);
            } else if bg.cache.should_update(view_target) {
                bg.cache.update(view_target, builder);
            }
        } else {
            commands.entity(entity).insert(StageBlendPostBindGroups {
                buffer_id: cur_buffer_id,
                ids: ready_ids,
                cache: builder.generate_bind_groups(view_target),
            });
        }
    }
}

fn stage_blend_post_system(
    view: ViewQuery<(
        &ViewTarget,
        &DynamicUniformIndex<StageBlendPostUniform>,
        &StageBlendPostBindGroups,
        &StageBlendPostPipelineId,
    )>,
    pipeline_cache: Res<PipelineCache>,
    mut ctx: RenderContext,
) {
    let (view_target, settings_index, bind_groups, pipeline_id) = view.into_inner();
    let Some(pipeline) = pipeline_cache.get_render_pipeline(pipeline_id.0) else {
        return;
    };

    let post_process = view_target.post_process_write();
    let bind_group = bind_groups.cache.get_current_bind_group(post_process.source);

    let pass_descriptor = RenderPassDescriptor {
        label: Some("stage_blend_post_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: post_process.destination,
            depth_slice: None,
            resolve_target: None,
            ops: Operations::default(),
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    };

    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let mut render_pass = ctx.command_encoder().begin_render_pass(&pass_descriptor);
    let pass_span = diagnostics.pass_span(&mut render_pass, "stage_blend_post_pass");
    render_pass.set_pipeline(pipeline);
    render_pass.set_bind_group(0, bind_group, &[settings_index.index()]);
    render_pass.draw(0..3, 0..1);
    pass_span.end(&mut render_pass);
}

#[cfg(test)]
mod tests {
    use super::StagePostOverlayKind;

    #[test]
    fn parses_post_blend_modes_only() {
        assert_eq!(
            StagePostOverlayKind::parse("overlay"),
            Some(StagePostOverlayKind::Overlay)
        );
        assert_eq!(
            StagePostOverlayKind::parse("HardLight"),
            Some(StagePostOverlayKind::HardLight)
        );
        assert_eq!(
            StagePostOverlayKind::parse("hard_light"),
            Some(StagePostOverlayKind::HardLight)
        );
        assert_eq!(
            StagePostOverlayKind::parse("srgb_add"),
            Some(StagePostOverlayKind::SrgbAdd)
        );
        assert_eq!(
            StagePostOverlayKind::parse("srgb-add"),
            Some(StagePostOverlayKind::SrgbAdd)
        );
        assert_eq!(StagePostOverlayKind::parse("add"), None);
        assert_eq!(StagePostOverlayKind::parse("multiply"), None);
        assert_eq!(StagePostOverlayKind::parse("screen"), None);
        assert_eq!(StagePostOverlayKind::parse("normal"), None);
    }
}
