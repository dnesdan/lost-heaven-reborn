//! Small indexed textured renderer. Source coordinate conversion belongs to asset intake.
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::{
    cell::RefCell,
    collections::HashMap,
    ops::Range,
    path::Path,
    sync::{mpsc, Arc, Weak},
};
use wgpu::util::DeviceExt;
mod material;
use material::PipelineKey;
pub use material::{AlphaMode, FrontFace, MaterialQueue, OriginalMaterial, RenderMaterial};

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
}
#[derive(Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub repeat: [bool; 2],
}
pub struct Mesh {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
    pub texture: Option<Arc<Texture>>,
    pub material: RenderMaterial,
}
pub struct Scene {
    pub meshes: Vec<Mesh>,
}
pub struct Camera {
    pub eye: Vec3,
    pub target: Vec3,
    pub up: Vec3,
    pub near: f32,
    pub far: f32,
}
impl Camera {
    pub fn validate(&self) -> Result<(), String> {
        let direction = self.target - self.eye;
        if !self.eye.is_finite()
            || !self.target.is_finite()
            || !self.up.is_finite()
            || self.eye.abs().max_element() > 1.0e8
            || self.target.abs().max_element() > 1.0e8
            || direction.length_squared() < 1.0e-8
            || direction
                .normalize()
                .cross(self.up.normalize_or_zero())
                .length_squared()
                < 1.0e-6
            || !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
            || !self.matrix(1.0).is_finite()
        {
            return Err("Camera position, basis or clipping range is invalid".into());
        }
        Ok(())
    }
    pub fn fit(scene: &Scene) -> Result<Self, String> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in scene.meshes.iter().flat_map(|m| &m.vertices) {
            let p = Vec3::from_array(vertex.position);
            if !p.is_finite() {
                return Err("Scene contains nonfinite coordinates".into());
            }
            min = min.min(p);
            max = max.max(p);
        }
        if !min.is_finite() {
            return Err("Scene has no vertices".into());
        }
        let center = (min + max) * 0.5;
        let radius = (max - min).length().max(0.1) * 0.5;
        if !center.is_finite()
            || center.abs().max_element() > 1.0e8
            || !radius.is_finite()
            || radius > 1.0e8
        {
            return Err("Scene bounds exceed supported range".into());
        }
        let result = Self {
            eye: center + Vec3::new(1.0, 0.65, 1.0).normalize() * radius * 2.8,
            target: center,
            up: Vec3::Y,
            near: (radius / 1000.0).max(0.001),
            far: radius * 20.0,
        };
        result.validate()?;
        Ok(result)
    }
    fn matrix(&self, aspect: f32) -> Mat4 {
        Mat4::perspective_rh(50f32.to_radians(), aspect, self.near, self.far)
            * Mat4::look_at_rh(self.eye, self.target, self.up)
    }
}
struct Draw {
    vertex: wgpu::Buffer,
    vertex_count: usize,
    index: wgpu::Buffer,
    count: u32,
    material: Arc<GpuMaterial>,
    source: Weak<Texture>,
    parameters: wgpu::BindGroup,
    parameters_buffer: wgpu::Buffer,
    policy: RenderMaterial,
}
struct GpuMaterial {
    group: wgpu::BindGroup,
    texture: wgpu::Texture,
}
struct CachedMaterial {
    source: Weak<Texture>,
    material: Arc<GpuMaterial>,
}
type MaterialCache = HashMap<usize, CachedMaterial>;
struct DepthTarget {
    size: [u32; 2],
    view: wgpu::TextureView,
}
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    format: wgpu::TextureFormat,
    pipeline: wgpu::RenderPipeline,
    inspection_cutout_pipeline: wgpu::RenderPipeline,
    inspection_alpha_cutout: bool,
    overlay_pipeline: wgpu::RenderPipeline,
    overlay_buffer: wgpu::Buffer,
    overlay_group: wgpu::BindGroup,
    camera_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    draws: Vec<Draw>,
    texture_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    original_pipelines: HashMap<PipelineKey, wgpu::RenderPipeline>,
    linear: bool,
    materials: RefCell<MaterialCache>,
    depth: RefCell<Option<DepthTarget>>,
}
impl Renderer {
    /// Opt-in host inspection policy: discard sampled alpha below 0.5.
    /// The default scene renderer and overlay retain their previous behavior.
    pub fn set_inspection_alpha_cutout(&mut self, enabled: bool) {
        self.inspection_alpha_cutout = enabled;
    }
    pub async fn new(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
        scene: &Scene,
        format: wgpu::TextureFormat,
        linear: bool,
    ) -> Result<Self, String> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface,
                force_fallback_adapter: false,
            })
            .await
            .ok_or("No compatible graphics adapter")?;
        Self::with_adapter(&adapter, scene, format, linear).await
    }
    pub async fn with_adapter(
        adapter: &wgpu::Adapter,
        scene: &Scene,
        format: wgpu::TextureFormat,
        linear: bool,
    ) -> Result<Self, String> {
        let (device, queue) = adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    label: Some("LHR renderer"),
                    required_features: wgpu::Features::empty(),
                    required_limits: wgpu::Limits::default(),
                    memory_hints: wgpu::MemoryHints::MemoryUsage,
                },
                None,
            )
            .await
            .map_err(|_| "Graphics device initialization failed")?;
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Original material parameters"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: None,
            source: wgpu::ShaderSource::Wgsl(include_str!("scene.wgsl").into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&camera_layout, &texture_layout],
            push_constant_ranges: &[],
        });
        let scene_pipeline = |fragment_entry| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: None,
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[wgpu::VertexBufferLayout {
                        array_stride: 20,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
                    }],
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment_entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: true,
                    depth_compare: wgpu::CompareFunction::Less,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: Default::default(),
                multiview: None,
                cache: None,
            })
        };
        let pipeline = scene_pipeline("fragment");
        let inspection_cutout_pipeline = scene_pipeline("fragment_inspection_cutout");
        let original_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Original material pipeline layout"),
            bind_group_layouts: &[&camera_layout, &texture_layout, &material_layout],
            push_constant_ranges: &[],
        });
        let mut original_pipelines = HashMap::new();
        for alpha in [
            AlphaMode::Opaque,
            AlphaMode::ColorKey,
            AlphaMode::Blend,
            AlphaMode::Additive,
        ] {
            for two_sided in [false, true] {
                for front_face in [FrontFace::CounterClockwise, FrontFace::Clockwise] {
                    let key = PipelineKey {
                        alpha,
                        two_sided,
                        front_face,
                    };
                    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                        label: Some("Original material draw"),
                        layout: Some(&original_layout),
                        vertex: wgpu::VertexState {
                            module: &shader,
                            entry_point: Some("vertex"),
                            buffers: &[wgpu::VertexBufferLayout {
                                array_stride: 20,
                                step_mode: wgpu::VertexStepMode::Vertex,
                                attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
                            }],
                            compilation_options: Default::default(),
                        },
                        fragment: Some(wgpu::FragmentState {
                            module: &shader,
                            entry_point: Some("fragment_original"),
                            targets: &[Some(wgpu::ColorTargetState {
                                format,
                                blend: material::blend(alpha),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                            compilation_options: Default::default(),
                        }),
                        primitive: wgpu::PrimitiveState {
                            cull_mode: (!two_sided).then_some(wgpu::Face::Back),
                            front_face: match front_face {
                                FrontFace::CounterClockwise => wgpu::FrontFace::Ccw,
                                FrontFace::Clockwise => wgpu::FrontFace::Cw,
                            },
                            ..Default::default()
                        },
                        depth_stencil: Some(wgpu::DepthStencilState {
                            format: wgpu::TextureFormat::Depth32Float,
                            depth_write_enabled: true,
                            depth_compare: wgpu::CompareFunction::LessEqual,
                            stencil: Default::default(),
                            bias: Default::default(),
                        }),
                        multisample: Default::default(),
                        multiview: None,
                        cache: None,
                    });
                    original_pipelines.insert(key, pipeline);
                }
            }
        }
        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 20,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x2],
                }],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let overlay_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen projection"),
            contents: bytemuck::cast_slice(&Mat4::IDENTITY.to_cols_array()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let overlay_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: overlay_buffer.as_entire_binding(),
            }],
        });
        let mut materials = MaterialCache::new();
        let draws = upload_meshes(
            &device,
            &queue,
            &texture_layout,
            &material_layout,
            linear,
            &scene.meshes,
            &mut materials,
        )?;
        Ok(Self {
            device,
            queue,
            format,
            pipeline,
            inspection_cutout_pipeline,
            inspection_alpha_cutout: false,
            overlay_pipeline,
            overlay_buffer,
            overlay_group,
            camera_buffer,
            camera_group,
            draws,
            texture_layout,
            material_layout,
            original_pipelines,
            linear,
            materials: RefCell::new(materials),
            depth: RefCell::new(None),
        })
    }
    /// Replace a contiguous set of draws on the existing device. Unaffected
    /// draws, camera state and pipelines remain owned by this renderer.
    /// Empty ranges insert draws; an empty slice removes the selected draws.
    /// Invalid input leaves the existing draw list intact.
    pub fn replace_meshes(&mut self, range: Range<usize>, meshes: &[Mesh]) -> Result<(), String> {
        if range.start > range.end || range.end > self.draws.len() {
            return Err("Replacement mesh range is unavailable".into());
        }
        if range.is_empty() && meshes.is_empty() {
            return Ok(());
        }
        let next = upload_meshes(
            &self.device,
            &self.queue,
            &self.texture_layout,
            &self.material_layout,
            self.linear,
            meshes,
            &mut self.materials.borrow_mut(),
        )?;
        self.draws.splice(range, next);
        self.materials
            .borrow_mut()
            .retain(|_, cached| cached.source.strong_count() != 0);
        Ok(())
    }
    pub fn render(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
    ) -> wgpu::CommandEncoder {
        self.render_matrix(
            view,
            width,
            height,
            camera.matrix(width as f32 / height as f32),
        )
    }
    /// Draw screen-space pixels on a fixed design canvas, preserving its aspect ratio.
    pub fn render_screen(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        design: [f32; 2],
    ) -> wgpu::CommandEncoder {
        self.render_matrix(view, width, height, screen_matrix(width, height, design))
    }
    /// Replace pixels of an existing material without recreating the device or geometry.
    pub fn update_texture(&self, index: usize, pixels: &Texture) -> Result<(), String> {
        let draw = self.draws.get(index).ok_or("Texture index unavailable")?;
        if pixels.width != draw.material.texture.width()
            || pixels.height != draw.material.texture.height()
            || pixels.rgba.len() != pixels.width as usize * pixels.height as usize * 4
        {
            return Err("Replacement texture dimensions differ".into());
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &draw.material.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pixels.width * 4),
                rows_per_image: Some(pixels.height),
            },
            draw.material.texture.size(),
        );
        // This mutable upload no longer represents the immutable CPU source.
        // Existing draws keep their shared GPU allocation; a future upload of
        // the old source must not observe the replacement pixels.
        let mut cache = self.materials.borrow_mut();
        let key = draw.source.as_ptr() as usize;
        if cache
            .get(&key)
            .is_some_and(|cached| Arc::ptr_eq(&cached.material, &draw.material))
        {
            cache.remove(&key);
        }
        Ok(())
    }
    /// Update per-draw scalar/queue/cull decisions without duplicating its GPU
    /// texture or replacing geometry. The source owner supplies a fresh center.
    pub fn update_render_material(
        &mut self,
        index: usize,
        material: RenderMaterial,
    ) -> Result<(), String> {
        if !material.valid() {
            return Err("Invalid render material".into());
        }
        let draw = self.draws.get_mut(index).ok_or("Mesh index unavailable")?;
        self.queue.write_buffer(
            &draw.parameters_buffer,
            0,
            bytemuck::cast_slice(&material.parameters()),
        );
        draw.policy = material;
        Ok(())
    }
    /// Queue replacement vertices for an existing draw, preserving its topology and material.
    ///
    /// Updates take effect on the next queue submission, like `update_texture`.
    /// The caller owns pose evaluation and coordinate conversion. This upload
    /// interface does not implement or approximate original animation behavior.
    pub fn update_mesh_vertices(&self, index: usize, vertices: &[Vertex]) -> Result<(), String> {
        let draw = self.draws.get(index).ok_or("Mesh index unavailable")?;
        validate_vertex_replacement(draw.vertex_count, vertices)?;
        self.queue
            .write_buffer(&draw.vertex, 0, bytemuck::cast_slice(vertices));
        Ok(())
    }
    fn depth_view(&self, width: u32, height: u32) -> wgpu::TextureView {
        let mut cached = self.depth.borrow_mut();
        if let Some(target) = cached
            .as_ref()
            .filter(|target| target.size == [width, height])
        {
            return target.view.clone();
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Scene depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        *cached = Some(DepthTarget {
            size: [width, height],
            view: view.clone(),
        });
        view
    }
    fn render_matrix(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        matrix: Mat4,
    ) -> wgpu::CommandEncoder {
        self.render_layers(view, width, height, matrix, None)
    }
    /// Render a scene followed by the final mesh as an alpha-composited design-canvas overlay.
    pub fn render_with_overlay(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        camera: &Camera,
        design: [f32; 2],
    ) -> wgpu::CommandEncoder {
        self.render_layers(
            view,
            width,
            height,
            camera.matrix(width as f32 / height as f32),
            Some(design),
        )
    }
    fn render_layers(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        matrix: Mat4,
        overlay: Option<[f32; 2]>,
    ) -> wgpu::CommandEncoder {
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::cast_slice(&matrix.to_cols_array()),
        );
        let depth_view = self.depth_view(width, height);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.025,
                            g: 0.035,
                            b: 0.055,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(if self.inspection_alpha_cutout {
                &self.inspection_cutout_pipeline
            } else {
                &self.pipeline
            });
            pass.set_bind_group(0, &self.camera_group, &[]);
            let count = self
                .draws
                .len()
                .saturating_sub(usize::from(overlay.is_some()));
            for index in self.draw_order(count, matrix) {
                let draw = &self.draws[index];
                match draw.policy {
                    RenderMaterial::Legacy => pass.set_pipeline(if self.inspection_alpha_cutout {
                        &self.inspection_cutout_pipeline
                    } else {
                        &self.pipeline
                    }),
                    RenderMaterial::Original(m) => {
                        pass.set_pipeline(&self.original_pipelines[&m.into()]);
                        pass.set_bind_group(2, &draw.parameters, &[]);
                    }
                }
                pass.set_bind_group(1, &draw.material.group, &[]);
                pass.set_vertex_buffer(0, draw.vertex.slice(..));
                pass.set_index_buffer(draw.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.count, 0, 0..1);
            }
        }
        if let Some(design) = overlay {
            self.queue.write_buffer(
                &self.overlay_buffer,
                0,
                bytemuck::cast_slice(&screen_matrix(width, height, design).to_cols_array()),
            );
            if let Some(draw) = self.draws.last() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Menu overlay"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, &self.overlay_group, &[]);
                pass.set_bind_group(1, &draw.material.group, &[]);
                pass.set_vertex_buffer(0, draw.vertex.slice(..));
                pass.set_index_buffer(draw.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.count, 0, 0..1);
            }
        }
        encoder
    }
    /// Source-inspired host ordering: opaque, color-keyed opaque, then blended
    /// groups back-to-front by projected view depth of their supplied bounds.
    /// Native texture UID batching and quantized radix keys are not fabricated.
    fn draw_order(&self, count: usize, matrix: Mat4) -> Vec<usize> {
        let queue = |index: usize| match self.draws[index].policy {
            RenderMaterial::Legacy => MaterialQueue::Opaque,
            RenderMaterial::Original(m) => m.queue,
        };
        let depth = |index: usize| match self.draws[index].policy {
            RenderMaterial::Legacy => 0.,
            RenderMaterial::Original(m) => (matrix * Vec3::from_array(m.sort_center).extend(1.)).z,
        };
        let mut order: Vec<_> = (0..count).collect();
        order.sort_by(|&a, &b| {
            queue(a).cmp(&queue(b)).then_with(|| {
                if queue(a) == MaterialQueue::Transparent {
                    depth(b).total_cmp(&depth(a)).then_with(|| b.cmp(&a))
                } else {
                    a.cmp(&b)
                }
            })
        });
        order
    }
    pub fn capture(
        &self,
        width: u32,
        height: u32,
        camera: &Camera,
        path: &Path,
    ) -> Result<(), String> {
        self.capture_matrix(
            width,
            height,
            camera.matrix(width as f32 / height as f32),
            None,
            path,
        )
    }
    pub fn capture_screen(
        &self,
        width: u32,
        height: u32,
        design: [f32; 2],
        path: &Path,
    ) -> Result<(), String> {
        self.capture_matrix(
            width,
            height,
            screen_matrix(width, height, design),
            None,
            path,
        )
    }
    pub fn capture_with_overlay(
        &self,
        width: u32,
        height: u32,
        camera: &Camera,
        design: [f32; 2],
        path: &Path,
    ) -> Result<(), String> {
        self.capture_matrix(
            width,
            height,
            camera.matrix(width as f32 / height as f32),
            Some(design),
            path,
        )
    }
    fn readback_matrix(
        &self,
        width: u32,
        height: u32,
        matrix: Mat4,
        overlay: Option<[f32; 2]>,
    ) -> Result<Vec<u8>, String> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err("Capture size outside supported range".into());
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let row = (width * 4 + 255) & !255;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.render_layers(
            &texture.create_view(&Default::default()),
            width,
            height,
            matrix,
            overlay,
        );
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(height),
                },
            },
            texture.size(),
        );
        self.queue.submit(Some(encoder.finish()));
        let slice = buffer.slice(..);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::Maintain::Wait);
        rx.recv()
            .map_err(|_| "Capture readback disconnected")?
            .map_err(|_| "Capture readback failed")?;
        let mapped = slice.get_mapped_range();
        let mut rgba = Vec::with_capacity(width as usize * height as usize * 4);
        for bytes in mapped.chunks_exact(row as usize) {
            rgba.extend_from_slice(&bytes[..width as usize * 4]);
        }
        drop(mapped);
        buffer.unmap();
        if matches!(
            self.format,
            wgpu::TextureFormat::Bgra8UnormSrgb | wgpu::TextureFormat::Bgra8Unorm
        ) {
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        Ok(rgba)
    }
    /// Offscreen RGBA8 readback of the same scene/pipeline used for presentation.
    pub fn readback(&self, width: u32, height: u32, camera: &Camera) -> Result<Vec<u8>, String> {
        self.readback_matrix(
            width,
            height,
            camera.matrix(width as f32 / height as f32),
            None,
        )
    }
    pub fn readback_screen(
        &self,
        width: u32,
        height: u32,
        design: [f32; 2],
    ) -> Result<Vec<u8>, String> {
        self.readback_matrix(width, height, screen_matrix(width, height, design), None)
    }
    fn capture_matrix(
        &self,
        width: u32,
        height: u32,
        matrix: Mat4,
        overlay: Option<[f32; 2]>,
        path: &Path,
    ) -> Result<(), String> {
        let rgba = self.readback_matrix(width, height, matrix, overlay)?;
        let file = std::fs::File::create(path).map_err(|_| "Cannot create capture output")?;
        let mut encoder = png::Encoder::new(file, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .map_err(|_| "Cannot write capture header")?
            .write_image_data(&rgba)
            .map_err(|_| "Cannot write capture image")?;
        Ok(())
    }
}
fn upload_meshes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    linear: bool,
    meshes: &[Mesh],
    materials: &mut MaterialCache,
) -> Result<Vec<Draw>, String> {
    let mut draws = Vec::new();
    // Validate the whole replacement before allocating or changing the cache.
    for mesh in meshes {
        if !mesh.material.valid() {
            return Err("Invalid render material".into());
        }
        if mesh.indices.is_empty()
            || mesh.indices.len() % 3 != 0
            || mesh
                .indices
                .iter()
                .any(|&i| i as usize >= mesh.vertices.len())
        {
            return Err("Invalid mesh triangle indices".into());
        }
        if mesh
            .vertices
            .iter()
            .any(|v| v.position.iter().chain(&v.uv).any(|x| !x.is_finite()))
        {
            return Err("Nonfinite mesh vertex".into());
        }
        let tex = mesh
            .texture
            .as_ref()
            .ok_or("Mesh material has no texture or diffuse color")?;
        if tex.width == 0
            || tex.height == 0
            || tex.width > 8192
            || tex.height > 8192
            || tex.rgba.len() != tex.width as usize * tex.height as usize * 4
        {
            return Err("Invalid texture dimensions or data".into());
        }
        u32::try_from(mesh.indices.len()).map_err(|_| "Too many indices")?;
    }
    materials.retain(|_, cached| cached.source.strong_count() != 0);
    for mesh in meshes {
        let tex = mesh.texture.as_ref().expect("validated texture");
        let key = Arc::as_ptr(tex) as usize;
        let material = if let Some(cached) = materials.get(&key) {
            Arc::clone(&cached.material)
        } else {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d {
                    width: tex.width,
                    height: tex.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &tex.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(tex.width * 4),
                    rows_per_image: Some(tex.height),
                },
                texture.size(),
            );
            let filter = if linear {
                wgpu::FilterMode::Linear
            } else {
                wgpu::FilterMode::Nearest
            };
            let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: filter,
                min_filter: filter,
                address_mode_u: if tex.repeat[0] {
                    wgpu::AddressMode::Repeat
                } else {
                    wgpu::AddressMode::ClampToEdge
                },
                address_mode_v: if tex.repeat[1] {
                    wgpu::AddressMode::Repeat
                } else {
                    wgpu::AddressMode::ClampToEdge
                },
                ..Default::default()
            });
            let material = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None,
                layout: texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });
            let material = Arc::new(GpuMaterial {
                texture,
                group: material,
            });
            materials.insert(
                key,
                CachedMaterial {
                    source: Arc::downgrade(tex),
                    material: Arc::clone(&material),
                },
            );
            material
        };
        let parameters_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Per-draw material parameters"),
            contents: bytemuck::cast_slice(&mesh.material.parameters()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let parameters = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Per-draw material parameters"),
            layout: material_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: parameters_buffer.as_entire_binding(),
            }],
        });
        draws.push(Draw {
            vertex: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&mesh.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            }),
            vertex_count: mesh.vertices.len(),
            index: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: None,
                contents: bytemuck::cast_slice(&mesh.indices),
                usage: wgpu::BufferUsages::INDEX,
            }),
            count: mesh
                .indices
                .len()
                .try_into()
                .map_err(|_| "Too many indices")?,
            material,
            source: Arc::downgrade(tex),
            parameters,
            parameters_buffer,
            policy: mesh.material,
        });
    }
    Ok(draws)
}
fn validate_vertex_replacement(expected: usize, vertices: &[Vertex]) -> Result<(), String> {
    if vertices.len() != expected {
        return Err("Replacement vertex count differs".into());
    }
    if vertices
        .iter()
        .any(|v| v.position.iter().chain(&v.uv).any(|x| !x.is_finite()))
    {
        return Err("Replacement vertices contain nonfinite values".into());
    }
    Ok(())
}
fn screen_matrix(width: u32, height: u32, design: [f32; 2]) -> Mat4 {
    let scale = (width as f32 / design[0]).min(height as f32 / design[1]);
    let x = design[0] * scale / width as f32;
    let y = design[1] * scale / height as f32;
    Mat4::from_scale(Vec3::new(x, y, 1.0))
        * Mat4::orthographic_rh(0.0, design[0], design[1], 0.0, -1.0, 1.0)
}
pub fn instance() -> wgpu::Instance {
    wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN | wgpu::Backends::METAL,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicitly opt in: the workspace logic suite must also run without a GPU.
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn gpu_resources_survive_replacement_and_invalidate_mutable_uploads() {
        let instance = instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
            .expect("GPU resource test requires a graphics adapter");
        let source = Arc::new(Texture {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            repeat: [true; 2],
        });
        let mesh = || Mesh {
            material: Default::default(),
            vertices: vec![
                Vertex {
                    position: [0., 0., 0.],
                    uv: [0.; 2],
                },
                Vertex {
                    position: [1., 0., 0.],
                    uv: [0.; 2],
                },
                Vertex {
                    position: [0., 1., 0.],
                    uv: [0.; 2],
                },
            ],
            indices: vec![0, 1, 2],
            texture: Some(Arc::clone(&source)),
        };
        let scene = Scene {
            meshes: vec![mesh(), mesh()],
        };
        let mut renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let first = Arc::clone(&renderer.draws[0].material);
        assert!(Arc::ptr_eq(&first, &renderer.draws[1].material));
        renderer.replace_meshes(0..2, &[mesh()]).unwrap();
        assert!(Arc::ptr_eq(&first, &renderer.draws[0].material));
        assert_eq!(renderer.materials.borrow().len(), 1);

        let mut invalid = mesh();
        invalid.indices[0] = 99;
        assert!(renderer.replace_meshes(0..1, &[invalid]).is_err());
        assert!(Arc::ptr_eq(&first, &renderer.draws[0].material));

        renderer
            .update_texture(
                0,
                &Texture {
                    width: 1,
                    height: 1,
                    rgba: vec![0, 0, 0, 255],
                    repeat: [true; 2],
                },
            )
            .unwrap();
        assert!(renderer.materials.borrow().is_empty());
        renderer.replace_meshes(1..1, &[mesh()]).unwrap();
        assert!(!Arc::ptr_eq(&first, &renderer.draws[1].material));
        let pristine = Arc::clone(&renderer.draws[1].material);
        // A further edit of the old generation must not evict the original
        // source's newer, untouched allocation from the cache.
        renderer
            .update_texture(
                0,
                &Texture {
                    width: 1,
                    height: 1,
                    rgba: vec![128, 0, 0, 255],
                    repeat: [true; 2],
                },
            )
            .unwrap();
        renderer.replace_meshes(2..2, &[mesh()]).unwrap();
        assert!(Arc::ptr_eq(&pristine, &renderer.draws[2].material));

        let depth = renderer.depth_view(32, 16);
        assert_eq!(depth, renderer.depth_view(32, 16));
        assert_ne!(depth, renderer.depth_view(16, 32));
        drop(scene);
        drop(source);
        renderer.replace_meshes(0..3, &[]).unwrap();
        assert!(renderer.materials.borrow().is_empty());
        renderer.device.poll(wgpu::Maintain::Wait);
    }

    #[test]
    fn replacement_geometry_preserves_topology_and_requires_finite_attributes() {
        let valid = Vertex {
            position: [1.0, -2.0, 3.0],
            uv: [0.0, 1.0],
        };
        assert!(validate_vertex_replacement(3, &[valid; 3]).is_ok());
        assert!(validate_vertex_replacement(3, &[valid; 2]).is_err());
        assert!(validate_vertex_replacement(3, &[valid; 4]).is_err());
        let mut invalid = [valid; 3];
        invalid[2].position[0] = f32::NAN;
        assert!(validate_vertex_replacement(3, &invalid).is_err());
        invalid[2] = valid;
        invalid[2].uv[1] = f32::INFINITY;
        assert!(validate_vertex_replacement(3, &invalid).is_err());
    }

    #[test]
    fn authored_camera_rejects_collapsed_and_vertical_bases() {
        let mut camera = Camera {
            eye: Vec3::splat(1.0e30),
            target: Vec3::splat(1.0e30),
            up: Vec3::Y,
            near: 0.1,
            far: 100.0,
        };
        assert!(camera.validate().is_err());
        camera.eye = Vec3::ZERO;
        camera.target = Vec3::Y;
        assert!(camera.validate().is_err());
        camera.target = Vec3::Z;
        assert!(camera.validate().is_ok());
    }
    #[test]
    fn fitted_camera_contains_all_fixture_vertices_in_depth_and_view() {
        let scene = Scene {
            meshes: vec![Mesh {
                material: Default::default(),
                vertices: [-1.0, 1.0]
                    .into_iter()
                    .flat_map(|x| {
                        [-2.0, 2.0].into_iter().flat_map(move |y| {
                            [-3.0, 3.0].into_iter().map(move |z| Vertex {
                                position: [x, y, z],
                                uv: [0.0; 2],
                            })
                        })
                    })
                    .collect(),
                indices: vec![],
                texture: None,
            }],
        };
        let camera = Camera::fit(&scene).unwrap();
        for vertex in &scene.meshes[0].vertices {
            let clip = camera.matrix(1.5) * Vec3::from_array(vertex.position).extend(1.0);
            let ndc = clip.truncate() / clip.w;
            assert!(ndc.x.abs() < 1.0 && ndc.y.abs() < 1.0);
            assert!((0.0..1.0).contains(&ndc.z));
        }
    }

    #[test]
    fn huge_finite_constant_coordinates_cannot_collapse_eye_to_target() {
        let scene = Scene {
            meshes: vec![Mesh {
                material: Default::default(),
                vertices: vec![Vertex {
                    position: [1.0e30; 3],
                    uv: [0.0; 2],
                }],
                indices: vec![],
                texture: None,
            }],
        };
        assert!(Camera::fit(&scene).is_err());
    }

    #[test]
    fn empty_or_nonfinite_geometry_cannot_produce_a_camera() {
        assert!(Camera::fit(&Scene { meshes: vec![] }).is_err());
        let invalid = Scene {
            meshes: vec![Mesh {
                material: Default::default(),
                vertices: vec![Vertex {
                    position: [f32::NAN, 0.0, 0.0],
                    uv: [0.0; 2],
                }],
                indices: vec![],
                texture: None,
            }],
        };
        assert!(Camera::fit(&invalid).is_err());
    }
}

#[cfg(test)]
mod material_gpu_tests;
