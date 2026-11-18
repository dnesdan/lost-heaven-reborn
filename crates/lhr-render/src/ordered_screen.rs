use super::{material, upload_meshes, write_capture, AlphaMode, Draw, Mesh, Renderer, Texture};
use glam::Mat4;
use std::path::Path;
use std::sync::Arc;
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
@group(0) @binding(0) var<uniform> projection: mat4x4<f32>;
@group(1) @binding(0) var color_texture: texture_2d<f32>;
@group(1) @binding(1) var color_sampler: sampler;
struct MaterialParameters { color: vec4<f32>, unused: vec4<f32> };
@group(2) @binding(0) var<uniform> material: MaterialParameters;
struct Output {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};
@vertex fn vertex(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Output {
    var output: Output;
    output.position = projection * vec4<f32>(position.xy, 0.0, 1.0);
    output.uv = uv;
    return output;
}
fn decode_srgb(rgb: vec3<f32>) -> vec3<f32> {
    return select(rgb / 12.92, pow((rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4)), rgb > vec3<f32>(0.04045));
}
fn sampled_color(input: Output) -> vec4<f32> {
    return textureSample(color_texture, color_sampler, input.uv);
}
fn target_srgb(color: vec4<f32>) -> vec4<f32> {
    return vec4<f32>(decode_srgb(color.rgb), color.a);
}
@fragment fn fragment(input: Output) -> @location(0) vec4<f32> {
    return sampled_color(input) * material.color;
}
@fragment fn fragment_srgb_target(input: Output) -> @location(0) vec4<f32> {
    return target_srgb(sampled_color(input) * material.color);
}
"#;

pub(super) fn pipeline(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    camera_layout: &wgpu::BindGroupLayout,
    texture_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Ordered screen batches"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Ordered screen batches"),
        bind_group_layouts: &[camera_layout, texture_layout, material_layout],
        push_constant_ranges: &[],
    });
    let fragment = if format.is_srgb() {
        "fragment_srgb_target"
    } else {
        "fragment"
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Ordered screen batches: alpha blend, no depth or cull"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex"),
            buffers: &[wgpu::VertexBufferLayout {
                array_stride: std::mem::size_of::<super::Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
            }],
            compilation_options: Default::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some(fragment),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: material::blend(AlphaMode::Blend),
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
    })
}

pub(super) fn raw_texture_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    texture: &wgpu::Texture,
    linear: bool,
    repeat: [bool; 2],
) -> Option<wgpu::BindGroup> {
    if !texture.format().is_srgb() {
        return None;
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("Ordered screen raw UNORM view"),
        format: Some(texture.format().remove_srgb_suffix()),
        ..Default::default()
    });
    let filter = if linear {
        wgpu::FilterMode::Linear
    } else {
        wgpu::FilterMode::Nearest
    };
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Ordered screen raw texture sampler"),
        mag_filter: filter,
        min_filter: filter,
        address_mode_u: if repeat[0] {
            wgpu::AddressMode::Repeat
        } else {
            wgpu::AddressMode::ClampToEdge
        },
        address_mode_v: if repeat[1] {
            wgpu::AddressMode::Repeat
        } else {
            wgpu::AddressMode::ClampToEdge
        },
        ..Default::default()
    });
    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Ordered screen raw texture binding"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&sampler),
            },
        ],
    }))
}

fn projection(width: u32, height: u32, canvas: [f32; 2]) -> Result<Mat4, String> {
    if width == 0
        || height == 0
        || canvas
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.)
    {
        return Err("Ordered screen viewport or canvas is invalid".into());
    }
    let projection = Mat4::orthographic_rh(0., canvas[0], canvas[1], 0., -1., 1.);
    if !projection.is_finite() {
        return Err("Ordered screen canvas projection is nonfinite".into());
    }
    Ok(projection)
}

/// A temporary ordered draw list on the existing renderer and material cache.
/// Uploads do not replace scene meshes or reset any scene/overlay state.
pub struct OrderedScreenBatch<'renderer> {
    renderer: &'renderer Renderer,
    draws: Vec<Draw>,
    sources: Vec<Arc<Texture>>,
}

impl OrderedScreenBatch<'_> {
    /// Append one mesh, validating it at the point the caller emits it.
    pub fn push(&mut self, mesh: Mesh) -> Result<(), String> {
        let renderer = self.renderer;
        self.draws.extend(upload_meshes(
            &renderer.device,
            &renderer.queue,
            &renderer.texture_layout,
            &renderer.material_layout,
            renderer.linear,
            std::slice::from_ref(&mesh),
            &mut renderer.materials.borrow_mut(),
        )?);
        self.sources.extend(mesh.texture);
        Ok(())
    }

    /// Encode the full target in pixel coordinates without submitting it.
    /// The target must belong to this renderer's device.
    pub fn encode(&self, target: &wgpu::Texture) -> Result<wgpu::CommandEncoder, String> {
        if target.format() != self.renderer.format
            || !target
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            || target.dimension() != wgpu::TextureDimension::D2
            || target.depth_or_array_layers() != 1
            || target.sample_count() != 1
            || target.mip_level_count() != 1
        {
            return Err("Ordered screen target is incompatible with the renderer".into());
        }
        let width = target.width();
        let height = target.height();
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer.encode_ordered_draws(
            &view,
            width,
            height,
            [width as f32, height as f32],
            &self.draws,
        )
    }
}

impl Drop for OrderedScreenBatch<'_> {
    fn drop(&mut self) {
        self.sources.clear();
        self.renderer
            .materials
            .borrow_mut()
            .retain(|_, cached| cached.source.strong_count() != 0);
    }
}

impl Renderer {
    pub fn ordered_screen_batch(&self) -> OrderedScreenBatch<'_> {
        OrderedScreenBatch {
            renderer: self,
            draws: Vec::new(),
            sources: Vec::new(),
        }
    }

    /// Draw every uploaded mesh in supplied order, alpha blended over opaque black.
    /// Position XY are top-left canvas pixels; Z, scene lighting, alpha tests,
    /// material queues, culling and depth are ignored. UVs are unchanged and
    /// sampled RGBA multiplies the material RGBA (white for Legacy materials).
    /// A viewport-sized canvas uses actual pixels; other canvas sizes scale X/Y
    /// independently, without aspect fitting or an added half-pixel offset.
    pub fn render_ordered_screen(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        canvas: [f32; 2],
    ) -> Result<wgpu::CommandEncoder, String> {
        self.encode_ordered_draws(view, width, height, canvas, &self.draws)
    }

    fn encode_ordered_draws(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        canvas: [f32; 2],
        draws: &[Draw],
    ) -> Result<wgpu::CommandEncoder, String> {
        let projection = projection(width, height, canvas)?;
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Ordered screen frame projection"),
                contents: bytemuck::cast_slice(&projection.to_cols_array()),
                usage: wgpu::BufferUsages::UNIFORM,
            });
        let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Ordered screen frame projection"),
            layout: &self.camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.lab_buffer.as_entire_binding(),
                },
            ],
        });
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Ordered screen frame"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Ordered screen batches"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(0., 0., width as f32, height as f32, 0., 1.);
            pass.set_bind_group(0, &group, &[]);
            pass.set_pipeline(&self.ordered_screen_pipeline);
            for draw in draws {
                pass.set_bind_group(
                    1,
                    draw.ordered_raw_group
                        .as_ref()
                        .unwrap_or(&draw.material.group),
                    &[],
                );
                pass.set_bind_group(2, &draw.parameters, &[]);
                pass.set_vertex_buffer(0, draw.vertex.slice(..));
                pass.set_index_buffer(draw.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.count, 0, 0..1);
            }
        }
        Ok(encoder)
    }

    /// Read RGBA8 pixels from the same GPU path used by render_ordered_screen.
    pub fn readback_ordered_screen(
        &self,
        width: u32,
        height: u32,
        canvas: [f32; 2],
    ) -> Result<Vec<u8>, String> {
        projection(width, height, canvas)?;
        self.readback_using(width, height, |view| {
            self.render_ordered_screen(view, width, height, canvas)
        })
    }

    /// Write a PNG from the ordered screen GPU readback, never a CPU rasterizer.
    pub fn capture_ordered_screen(
        &self,
        width: u32,
        height: u32,
        canvas: [f32; 2],
        path: &Path,
    ) -> Result<(), String> {
        let rgba = self.readback_ordered_screen(width, height, canvas)?;
        write_capture(width, height, &rgba, path)
    }
}

#[cfg(test)]
#[path = "ordered_screen_tests.rs"]
mod tests;
