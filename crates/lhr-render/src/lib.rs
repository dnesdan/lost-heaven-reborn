//! Small indexed textured renderer. Source coordinate conversion belongs to asset intake.
use bytemuck::{Pod, Zeroable};
use glam::{Mat4, Vec3};
use std::{
    collections::HashMap,
    path::Path,
    sync::{mpsc, Arc},
};
use wgpu::util::DeviceExt;

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
    material: wgpu::BindGroup,
    texture: wgpu::Texture,
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
        let mut draws = Vec::new();
        let mut materials = HashMap::<(usize, [bool; 2]), (wgpu::Texture, wgpu::BindGroup)>::new();
        for mesh in &scene.meshes {
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
            let key = (Arc::as_ptr(tex) as usize, tex.repeat);
            let (texture, material) = if let Some(cached) = materials.get(&key) {
                cached.clone()
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
                    layout: &texture_layout,
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
                materials.insert(key, (texture.clone(), material.clone()));
                (texture, material)
            };
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
                texture,
            });
        }
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
        })
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
        if pixels.width != draw.texture.width()
            || pixels.height != draw.texture.height()
            || pixels.rgba.len() != pixels.width as usize * pixels.height as usize * 4
        {
            return Err("Replacement texture dimensions differ".into());
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &draw.texture,
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
            draw.texture.size(),
        );
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
        let depth = self.device.create_texture(&wgpu::TextureDescriptor {
            label: None,
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
        let depth_view = depth.create_view(&Default::default());
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
            for draw in &self.draws[..count] {
                pass.set_bind_group(1, &draw.material, &[]);
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
                pass.set_bind_group(1, &draw.material, &[]);
                pass.set_vertex_buffer(0, draw.vertex.slice(..));
                pass.set_index_buffer(draw.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.count, 0, 0..1);
            }
        }
        encoder
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
    fn capture_matrix(
        &self,
        width: u32,
        height: u32,
        matrix: Mat4,
        overlay: Option<[f32; 2]>,
        path: &Path,
    ) -> Result<(), String> {
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
