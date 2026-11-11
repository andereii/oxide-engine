//! Headless Bevy 0.20 render-to-image harness for the slice renderer.
//!
//! The app mirrors the feasibility probe in ADR-0020: `DefaultPlugins` minus
//! `WinitPlugin`, no primary window, and a `Camera2d` whose render target is a
//! 1280x720 `Rgba8UnormSrgb` image with `COPY_SRC`.
//!
//! Frame readback follows the ADR-0020 addendum 2 (2026-11-07): a custom
//! `RenderGraph` node copies the camera target texture into a persistent
//! `MAP_READ | COPY_DST` buffer after the main-pass submission, and a
//! `Render`-schedule system maps that buffer with a bounded poll loop and
//! publishes the row-unpadded RGBA8 bytes. The per-frame `Screenshot` path is
//! not used (it was flaky for repeated captures on the NVIDIA driver).
//!
//! Adapter policy follows the ADR-0020 addendum (2026-11-07):
//! - `Software` requests `llvmpipe` through `WgpuSettings` and fails closed if
//!   the selected adapter is not the llvmpipe software raster.
//! - `AnyHardware` uses the default hardware selection and fails closed with
//!   the typed `NoHardwareAdapter` error (candidate names included) when the
//!   selected adapter name is a software raster. It exists only for the
//!   platform-specific hardware `perf_budget` gate.
//!
//! Both paths print the chosen adapter identity to stderr. No environment
//! adapter override is applied by the harness.
//!
//! The app is driven by explicit `update()` calls from [`Harness::render`];
//! there is no `ScheduleRunnerPlugin`. Scene state never depends on Bevy
//! `Time`, so two calls with the same scene produce the same bytes.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::asset::RenderAssetUsages;
use bevy::camera::{CameraMainTextureUsages, RenderTarget};
use bevy::image::Image;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::pipelined_rendering::PipelinedRenderingPlugin;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{
    Buffer, BufferDescriptor, BufferUsages, CommandEncoderDescriptor, Extent3d, MapMode, PollType,
    PrimitiveTopology, TexelCopyBufferInfo, TexelCopyBufferLayout, TextureDimension, TextureFormat,
    TextureUsages,
};
use bevy::render::renderer::{
    RenderAdapterInfo, RenderContext, RenderDevice, RenderGraph, RenderGraphSystems,
    RenderInstance, RenderQueue,
};
use bevy::render::settings::{Backends, RenderCreation, WgpuSettings};
use bevy::render::texture::GpuImage;
use bevy::render::{Render, RenderApp, RenderPlugin, RenderSystems};
use bevy::sprite_render::AlphaMode2d;
use bevy::window::{ExitCondition, WindowPlugin};
use bevy::winit::WinitPlugin;

use super::load::PageImage;
use super::scene::{LayerSpec, Quad};
use super::{AdapterPolicy, RenderError};

/// Frozen slice render size (`RENDER-BEHAVIOR.md` §2/§7).
pub(crate) const RENDER_WIDTH: u32 = 1280;
pub(crate) const RENDER_HEIGHT: u32 = 720;
/// Bytes in one RGBA8 frame.
pub(crate) const FRAME_BYTES: usize = (RENDER_WIDTH as usize) * (RENDER_HEIGHT as usize) * 4;

/// Software raster adapter name pinned by ADR-0020 (render_diff fixtures).
const SOFTWARE_ADAPTER_NAME: &str = "llvmpipe";

/// Software-raster adapter names rejected by [`AdapterPolicy::AnyHardware`]
/// (ADR-0020 addendum decision 2: reject by the printed adapter name).
fn is_software_adapter_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        SOFTWARE_ADAPTER_NAME,
        "swiftshader",
        "lavapipe",
        "softpipe",
        "software",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn settings_for(policy: AdapterPolicy) -> (WgpuSettings, Backends) {
    let mut settings = WgpuSettings::default();
    match policy {
        AdapterPolicy::Software => {
            settings.adapter_name = Some(SOFTWARE_ADAPTER_NAME.to_string());
            settings.force_fallback_adapter = false;
        }
        AdapterPolicy::AnyHardware => {
            settings.adapter_name = None;
            settings.force_fallback_adapter = false;
        }
    }
    let backends = settings.backends.unwrap_or_else(Backends::all);
    (settings, backends)
}

/// Enumerates every adapter visible to the already-initialized renderer.
/// Used only on the `AnyHardware` failure path to list candidates.
fn adapter_candidates(app: &App, backends: Backends) -> Vec<String> {
    let Some(render_app) = app.get_sub_app(RenderApp) else {
        return Vec::new();
    };
    let Some(instance) = render_app.world().get_resource::<RenderInstance>() else {
        return Vec::new();
    };
    let adapters = bevy::tasks::block_on(instance.enumerate_adapters(backends));
    let mut names: Vec<String> = adapters
        .iter()
        .map(|adapter| adapter.get_info().name)
        .collect();
    names.sort();
    names.dedup();
    names
}

/// Upper bound for one GPU buffer-map readback. This is a fail-closed timeout,
/// not a slow retry: a driver that cannot map within this window makes the
/// renderer return a typed error (ADR-0020 addendum 2).
const READBACK_TIMEOUT_MS: u64 = 500;

/// Failure modes of the custom render-graph readback. The render world stores
/// this in the shared result slot; the main world converts it to
/// [`RenderError`] without retrying.
#[derive(Debug)]
enum ReadbackFailure {
    MissingTexture,
    UnsupportedFormat(String),
    WrongSize { width: u32, height: u32 },
    Map(String),
    Poll(String),
    Timeout { timeout_ms: u64, detail: String },
}

impl ReadbackFailure {
    fn detail(&self) -> String {
        format!("{self:?}")
    }

    fn into_render_error(self) -> RenderError {
        match self {
            ReadbackFailure::UnsupportedFormat(format) => RenderError::RenderFormat { format },
            ReadbackFailure::WrongSize { width, height } => {
                RenderError::RenderWindow { width, height }
            }
            ReadbackFailure::MissingTexture => RenderError::Readback {
                detail: "target image has no GPU texture in the render world".to_string(),
            },
            ReadbackFailure::Map(detail) | ReadbackFailure::Poll(detail) => {
                RenderError::Readback { detail }
            }
            ReadbackFailure::Timeout { timeout_ms, detail } => {
                RenderError::ReadbackTimeout { timeout_ms, detail }
            }
        }
    }
}

type ReadbackSlot = Arc<Mutex<Option<(u64, Result<Vec<u8>, ReadbackFailure>)>>>;

/// Render-world state for the persistent target -> buffer readback.
#[derive(Resource)]
struct ReadbackState {
    src_image: Handle<Image>,
    buffer: Buffer,
    padded_bytes_per_row: u32,
    requested: Arc<AtomicU64>,
    copied: Arc<AtomicU64>,
    reported: Arc<AtomicU64>,
    result: ReadbackSlot,
    adapter_name: String,
}

/// One persistent headless app plus the layer entities it renders.
pub(crate) struct Harness {
    app: App,
    mesh_handles: Vec<Handle<Mesh>>,
    material_handles: Vec<Handle<ColorMaterial>>,
    page_handles: Vec<Handle<Image>>,
    entities: Vec<Entity>,
    requested: Arc<AtomicU64>,
    result: ReadbackSlot,
    adapter_name: String,
    failed: Option<String>,
}

impl Harness {
    /// Builds the app, inserts decoded pages/materials/meshes and warms the
    /// render pipelines. `pages` and `specs` are consumed once.
    ///
    /// This is the deterministic llvmpipe-pinned entry point
    /// ([`AdapterPolicy::Software`]); [`Harness::with_policy`] exposes the
    /// explicit `AnyHardware` path for `perf_budget`.
    pub(crate) fn new(pages: Vec<PageImage>, specs: &[LayerSpec]) -> Result<Self, RenderError> {
        Self::with_policy(pages, specs, AdapterPolicy::Software)
    }

    /// Builds the headless app under an explicit adapter policy.
    pub(crate) fn with_policy(
        pages: Vec<PageImage>,
        specs: &[LayerSpec],
        policy: AdapterPolicy,
    ) -> Result<Self, RenderError> {
        let (settings, backends) = settings_for(policy);
        let mut app = App::new();
        app.add_plugins(
            DefaultPlugins
                .set(WindowPlugin {
                    primary_window: None,
                    exit_condition: ExitCondition::DontExit,
                    ..default()
                })
                .set(RenderPlugin {
                    render_creation: RenderCreation::Automatic(Box::new(settings)),
                    // Compile pipelines synchronously: the first rendered
                    // frame after a scene change must contain the scene, not
                    // an async-compilation placeholder.
                    synchronous_pipeline_compilation: true,
                    ..default()
                })
                // Engine decision: run the render sub-app synchronously inside
                // `App::update`. Pipelined rendering is initialized by the
                // app runner's plugin cleanup, which this manually driven
                // harness does not use; synchronous rendering also keeps
                // frame() wall-time accounting direct and deterministic.
                .disable::<PipelinedRenderingPlugin>()
                .disable::<WinitPlugin>(),
        );
        app.insert_resource(ClearColor(Color::BLACK));
        app.finish();

        let adapter_name = app
            .world()
            .get_resource::<RenderAdapterInfo>()
            .map(|info| info.name.clone())
            .unwrap_or_default();
        eprintln!("oxide-game slice renderer adapter: {adapter_name}");
        match policy {
            AdapterPolicy::Software => {
                if !adapter_name
                    .to_ascii_lowercase()
                    .contains(SOFTWARE_ADAPTER_NAME)
                {
                    return Err(RenderError::AdapterNotPinned { name: adapter_name });
                }
            }
            AdapterPolicy::AnyHardware => {
                if is_software_adapter_name(&adapter_name) {
                    let mut candidates = adapter_candidates(&app, backends);
                    if candidates.is_empty() {
                        candidates.push(adapter_name.clone());
                    }
                    return Err(RenderError::NoHardwareAdapter { candidates });
                }
            }
        }

        let target = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            let mut image = Image::new_fill(
                Extent3d {
                    width: RENDER_WIDTH,
                    height: RENDER_HEIGHT,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                &[0, 0, 0, 255],
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            // `Image::new_uninit` currently omits RENDER_ATTACHMENT; a camera
            // render target requires it explicitly. COPY_SRC is what the
            // custom render-graph node copies from; keep it guaranteed even
            // if the constructor defaults change.
            image.texture_descriptor.usage |=
                TextureUsages::RENDER_ATTACHMENT | TextureUsages::COPY_SRC;
            images.add(image)
        };

        let page_handles: Vec<Handle<Image>> = {
            let mut images = app.world_mut().resource_mut::<Assets<Image>>();
            pages
                .into_iter()
                .map(|page| images.add(page.image))
                .collect()
        };

        let material_handles: Vec<Handle<ColorMaterial>> = {
            let mut materials = app.world_mut().resource_mut::<Assets<ColorMaterial>>();
            specs
                .iter()
                .map(|spec| {
                    materials.add(ColorMaterial {
                        color: Color::srgba(
                            spec.color[0],
                            spec.color[1],
                            spec.color[2],
                            spec.color[3],
                        ),
                        alpha_mode: AlphaMode2d::Blend,
                        texture: spec.page.map(|index| page_handles[index].clone()),
                        ..default()
                    })
                })
                .collect()
        };

        let mesh_handles: Vec<Handle<Mesh>> = {
            let mut meshes = app.world_mut().resource_mut::<Assets<Mesh>>();
            specs
                .iter()
                .map(|_| meshes.add(placeholder_mesh()))
                .collect()
        };

        let mut entities = Vec::with_capacity(specs.len());
        for (index, (mesh, material)) in
            mesh_handles.iter().zip(material_handles.iter()).enumerate()
        {
            let entity = app
                .world_mut()
                .spawn((
                    Mesh2d(mesh.clone()),
                    MeshMaterial2d(material.clone()),
                    Transform::from_xyz(0.0, 0.0, index as f32),
                    Visibility::Hidden,
                ))
                .id();
            entities.push(entity);
        }

        app.world_mut().spawn((
            Camera2d,
            RenderTarget::from(target.clone()),
            CameraMainTextureUsages::default().with(TextureUsages::COPY_SRC),
            Msaa::Off,
        ));

        // Persistent GPU -> host readback state (ADR-0020 addendum 2). The
        // buffer is written by a custom RenderGraph node after the main-pass
        // submission and mapped by a Render-schedule system after rendering.
        let render_device = app.world().resource::<RenderDevice>().clone();
        let padded_bytes_per_row =
            RenderDevice::align_copy_bytes_per_row(RENDER_WIDTH as usize * 4);
        let buffer = render_device.create_buffer(&BufferDescriptor {
            label: Some("oxide-slice-readback"),
            size: padded_bytes_per_row as u64 * RENDER_HEIGHT as u64,
            usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let requested = Arc::new(AtomicU64::new(0));
        let copied = Arc::new(AtomicU64::new(0));
        let reported = Arc::new(AtomicU64::new(0));
        let result: ReadbackSlot = Arc::new(Mutex::new(None));
        {
            let render_app = app.sub_app_mut(RenderApp);
            render_app.insert_resource(ReadbackState {
                src_image: target.clone(),
                buffer,
                padded_bytes_per_row: padded_bytes_per_row as u32,
                requested: requested.clone(),
                copied: copied.clone(),
                reported: reported.clone(),
                result: result.clone(),
                adapter_name: adapter_name.clone(),
            });
            render_app.add_systems(
                RenderGraph,
                copy_target_to_buffer.after(RenderGraphSystems::Submit),
            );
            render_app.add_systems(
                Render,
                receive_image_from_buffer.after(RenderSystems::Render),
            );
        }

        // Warm up asset uploads and pipeline compilation once so the first
        // measured/rendered frame follows the same path as later frames.
        app.update();
        app.update();

        Ok(Self {
            app,
            mesh_handles,
            material_handles,
            page_handles,
            entities,
            requested,
            result,
            adapter_name,
            failed: None,
        })
    }

    /// Updates the layer meshes and returns the current frame as RGBA8 bytes
    /// (1280x720, row-major, top-left origin, straight alpha).
    ///
    /// [D] Materials are built once, but the scene re-reads its `LayerSpec`s
    /// on every tick. Refreshing the whole material list here (instead of
    /// tracking a mutable conversation subrange) is the smallest honest
    /// update: only the conversation layers ever change values, the refresh
    /// is O(layer count) with no allocation, and the frozen fixture digests
    /// are unchanged because their specs are constant.
    pub(crate) fn render(
        &mut self,
        layers: &[Vec<Quad>],
        specs: &[LayerSpec],
    ) -> Result<Vec<u8>, RenderError> {
        if let Some(detail) = &self.failed {
            return Err(RenderError::Readback {
                detail: detail.clone(),
            });
        }
        if layers.len() != self.mesh_handles.len() {
            return Err(RenderError::Renderer {
                detail: format!(
                    "scene produced {} layers but the harness owns {}",
                    layers.len(),
                    self.mesh_handles.len()
                ),
            });
        }
        if specs.len() != self.material_handles.len() {
            return Err(RenderError::Renderer {
                detail: format!(
                    "scene produced {} layer specs but the harness owns {} materials",
                    specs.len(),
                    self.material_handles.len()
                ),
            });
        }

        {
            let mut materials = self.app.world_mut().resource_mut::<Assets<ColorMaterial>>();
            sync_material_specs(
                &mut materials,
                &self.material_handles,
                &self.page_handles,
                specs,
            )?;
        }

        {
            let mut meshes = self.app.world_mut().resource_mut::<Assets<Mesh>>();
            for (handle, quads) in self.mesh_handles.iter().zip(layers.iter()) {
                if quads.is_empty() {
                    // Keep the last/placeholder mesh and hide the entity; an
                    // empty mesh would only trigger allocator warnings.
                    continue;
                }
                meshes.insert(handle, build_mesh(quads)).map_err(|error| {
                    RenderError::Renderer {
                        detail: format!("could not update layer mesh: {error}"),
                    }
                })?;
            }
        }
        {
            let world = self.app.world_mut();
            for (entity, quads) in self.entities.iter().zip(layers.iter()) {
                let mut entity = world.entity_mut(*entity);
                if let Some(mut visibility) = entity.get_mut::<Visibility>() {
                    *visibility = if quads.is_empty() {
                        Visibility::Hidden
                    } else {
                        Visibility::Inherited
                    };
                } else {
                    return Err(RenderError::Renderer {
                        detail: "layer entity lost its Visibility component".to_string(),
                    });
                }
            }
        }

        let generation = self
            .requested
            .fetch_add(1, Ordering::AcqRel)
            .checked_add(1)
            .ok_or_else(|| {
                let detail = "readback generation counter overflowed".to_string();
                self.failed = Some(detail.clone());
                RenderError::Readback { detail }
            })?;
        self.app.update();
        let result = self
            .result
            .lock()
            .map_err(|_| RenderError::Renderer {
                detail: "readback result slot is poisoned".to_string(),
            })?
            .take();
        match result {
            Some((got, Ok(bytes))) if got == generation => {
                if bytes.len() != FRAME_BYTES {
                    let detail = format!(
                        "readback produced {} bytes, expected {FRAME_BYTES}",
                        bytes.len()
                    );
                    self.failed = Some(detail.clone());
                    return Err(RenderError::Readback { detail });
                }
                Ok(bytes)
            }
            Some((got, Err(failure))) if got == generation => {
                let detail = failure.detail();
                self.failed = Some(detail);
                Err(failure.into_render_error())
            }
            Some((got, _)) => {
                let detail =
                    format!("readback generation mismatch (got {got}, expected {generation})");
                self.failed = Some(detail.clone());
                Err(RenderError::Readback { detail })
            }
            None => {
                let detail = format!(
                    "adapter {}: no readback result was produced for generation {generation} after one update",
                    self.adapter_name
                );
                self.failed = Some(detail.clone());
                Err(RenderError::Readback { detail })
            }
        }
    }
}

/// RenderGraph node: copy the current camera target texture into the
/// persistent MAP_READ buffer after the main-pass submission.
///
/// Runs after [`RenderGraphSystems::Submit`] so the copy is ordered behind the
/// camera render passes by the wgpu queue.
fn copy_target_to_buffer(
    render_context: RenderContext,
    state: Res<ReadbackState>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    render_queue: Res<RenderQueue>,
) {
    let generation = state.requested.load(Ordering::Acquire);
    if generation == 0 || generation == state.copied.load(Ordering::Acquire) {
        return;
    }
    let Some(src_image) = gpu_images.get(&state.src_image) else {
        store_readback_failure(&state, generation, ReadbackFailure::MissingTexture);
        return;
    };
    let format = src_image.texture_descriptor.format;
    if format != TextureFormat::Rgba8UnormSrgb {
        store_readback_failure(
            &state,
            generation,
            ReadbackFailure::UnsupportedFormat(format!("{format:?}")),
        );
        return;
    }
    let size = src_image.texture_descriptor.size;
    if size.width != RENDER_WIDTH || size.height != RENDER_HEIGHT {
        store_readback_failure(
            &state,
            generation,
            ReadbackFailure::WrongSize {
                width: size.width,
                height: size.height,
            },
        );
        return;
    }
    let mut encoder =
        render_context
            .render_device()
            .create_command_encoder(&CommandEncoderDescriptor {
                label: Some("oxide-slice-readback-copy"),
            });
    encoder.copy_texture_to_buffer(
        src_image.texture.as_image_copy(),
        TexelCopyBufferInfo {
            buffer: &state.buffer,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(state.padded_bytes_per_row),
                rows_per_image: None,
            },
        },
        size,
    );
    render_queue.submit(std::iter::once(encoder.finish()));
    state.copied.store(generation, Ordering::Release);
}

/// Render-schedule system: map the buffer filled by
/// [`copy_target_to_buffer`], strip wgpu's row padding, and publish the
/// RGBA8 rows for the main world.
///
/// The map wait is a bounded loop of short `PollType::Wait` calls ending at
/// [`READBACK_TIMEOUT_MS`]; failures are reported as typed errors, never
/// retried silently.
fn receive_image_from_buffer(state: Res<ReadbackState>, render_device: Res<RenderDevice>) {
    let copied = state.copied.load(Ordering::Acquire);
    let reported = state.reported.load(Ordering::Acquire);
    if copied == 0 || copied <= reported {
        return;
    }

    let buffer_slice = state.buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    buffer_slice.map_async(MapMode::Read, move |result| {
        let _ = sender.send(result);
    });

    let deadline = Instant::now() + Duration::from_millis(READBACK_TIMEOUT_MS);
    loop {
        match receiver.try_recv() {
            Ok(Ok(())) => break,
            Ok(Err(error)) => {
                state.reported.store(copied, Ordering::Release);
                store_readback_failure(
                    &state,
                    copied,
                    ReadbackFailure::Map(format!(
                        "adapter {}: buffer map failed: {error}",
                        state.adapter_name
                    )),
                );
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                state.reported.store(copied, Ordering::Release);
                store_readback_failure(
                    &state,
                    copied,
                    ReadbackFailure::Map(format!(
                        "adapter {}: buffer map callback disconnected",
                        state.adapter_name
                    )),
                );
                return;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {
                if Instant::now() >= deadline {
                    state.reported.store(copied, Ordering::Release);
                    store_readback_failure(
                        &state,
                        copied,
                        ReadbackFailure::Timeout {
                            timeout_ms: READBACK_TIMEOUT_MS,
                            detail: format!(
                                "adapter {}: buffer map did not complete within {READBACK_TIMEOUT_MS} ms while polling",
                                state.adapter_name
                            ),
                        },
                    );
                    return;
                }
                if let Err(error) = render_device.poll(PollType::Poll) {
                    state.reported.store(copied, Ordering::Release);
                    store_readback_failure(
                        &state,
                        copied,
                        ReadbackFailure::Poll(format!(
                            "adapter {}: device poll failed: {error}",
                            state.adapter_name
                        )),
                    );
                    return;
                }
                std::thread::yield_now();
            }
        }
    }

    let row_bytes = RENDER_WIDTH as usize * 4;
    let padded = state.padded_bytes_per_row as usize;
    let mut bytes = Vec::with_capacity(FRAME_BYTES);
    {
        let mapped = match buffer_slice.get_mapped_range() {
            Ok(mapped) => mapped,
            Err(error) => {
                state.reported.store(copied, Ordering::Release);
                store_readback_failure(
                    &state,
                    copied,
                    ReadbackFailure::Map(format!(
                        "adapter {}: mapped buffer range unavailable: {error}",
                        state.adapter_name
                    )),
                );
                state.buffer.unmap();
                return;
            }
        };
        for row in 0..RENDER_HEIGHT as usize {
            let start = row * padded;
            let end = start + row_bytes;
            if end > mapped.len() {
                state.reported.store(copied, Ordering::Release);
                store_readback_failure(
                    &state,
                    copied,
                    ReadbackFailure::WrongSize {
                        width: RENDER_WIDTH,
                        height: RENDER_HEIGHT,
                    },
                );
                drop(mapped);
                state.buffer.unmap();
                return;
            }
            bytes.extend_from_slice(&mapped[start..end]);
        }
    }
    state.buffer.unmap();
    if bytes.len() != FRAME_BYTES {
        state.reported.store(copied, Ordering::Release);
        store_readback_failure(
            &state,
            copied,
            ReadbackFailure::WrongSize {
                width: RENDER_WIDTH,
                height: RENDER_HEIGHT,
            },
        );
        return;
    }
    if let Ok(mut slot) = state.result.lock() {
        *slot = Some((copied, Ok(bytes)));
    }
    state.reported.store(copied, Ordering::Release);
}

fn store_readback_failure(state: &ReadbackState, generation: u64, failure: ReadbackFailure) {
    if let Ok(mut slot) = state.result.lock() {
        *slot = Some((generation, Err(failure)));
    }
    state.reported.store(generation, Ordering::Release);
}

/// Rewrites every material from the current scene layer specs.
///
/// [D] The windowed and headless harnesses both build materials once at app
/// construction. The 6E conversation changes a layer's color alpha and its
/// text page index at runtime, so each frame upload re-reads the specs. Only
/// conversation layers actually change; the loop is bounded by the scene's
/// fixed layer count and allocates nothing.
pub(crate) fn sync_material_specs(
    materials: &mut Assets<ColorMaterial>,
    handles: &[Handle<ColorMaterial>],
    page_handles: &[Handle<Image>],
    specs: &[LayerSpec],
) -> Result<(), RenderError> {
    if handles.len() != specs.len() {
        return Err(RenderError::Renderer {
            detail: format!(
                "scene produced {} layer specs but the renderer owns {} materials",
                specs.len(),
                handles.len()
            ),
        });
    }
    for (handle, spec) in handles.iter().zip(specs) {
        let texture = match spec.page {
            Some(index) => Some(
                page_handles
                    .get(index)
                    .ok_or_else(|| RenderError::Renderer {
                        detail: format!(
                            "layer spec references page {index} but only {} pages were uploaded",
                            page_handles.len()
                        ),
                    })?
                    .clone(),
            ),
            None => None,
        };
        let Some(mut material) = materials.get_mut(handle) else {
            return Err(RenderError::Renderer {
                detail: "layer material handle is missing from Assets<ColorMaterial>".to_string(),
            });
        };
        material.color = Color::srgba(spec.color[0], spec.color[1], spec.color[2], spec.color[3]);
        material.texture = texture;
    }
    Ok(())
}

/// Degenerate but non-empty placeholder so hidden layers never create an
/// empty mesh asset (the allocator warns on those).
///
/// Shared with the interactive windowed harness.
pub(crate) fn placeholder_mesh() -> Mesh {
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0f32, 0.0, 0.0]; 4]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 0.0]; 4]);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

/// Builds the Bevy mesh for one draw layer. Shared with the interactive
/// windowed harness (same vertex/UV conversion as the headless path).
pub(crate) fn build_mesh(quads: &[Quad]) -> Mesh {
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(quads.len() * 4);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(quads.len() * 4);
    let mut indices: Vec<u32> = Vec::with_capacity(quads.len() * 6);
    for quad in quads {
        let base = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        for corner in 0..4 {
            positions.push([quad.pos[corner][0] as f32, quad.pos[corner][1] as f32, 0.0]);
            uvs.push([quad.uv[corner][0] as f32, quad.uv[corner][1] as f32]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
