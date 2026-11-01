//! Saved lighting draw data, supplied by the existing CPU visual owner.
use super::*;
/// Cumulative saved-lighting writes, excluding default draw initialization.
/// These counters describe resource traffic, not original-renderer parity.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LightingUploadStatistics {
    pub atlas_uploads: u64,
    pub vertex_uploads: u64,
}
#[derive(Clone, Copy, Debug)]
pub enum LightingMode {
    LightMap {
        base_texture: bool,
        modulate_2x: bool,
    },
    VertexColors {
        modulate_2x: bool,
    },
}
pub struct MeshLighting {
    pub uv2: Vec<[f32; 2]>,
    /// Explicit per-vertex dynamic RGB contribution for LightMap, or saved
    /// vertex colors for VertexColors. No lighting contribution is guessed.
    pub colors: Vec<[f32; 4]>,
    pub atlas: Option<Arc<Texture>>,
    pub mode: LightingMode,
}
pub(super) fn parameters(material: RenderMaterial, lighting: [f32; 3]) -> [f32; 8] {
    let mut parameters = material.parameters();
    parameters[5..8].copy_from_slice(&lighting);
    parameters
}
impl Renderer {
    pub fn lighting_upload_statistics(&self) -> LightingUploadStatistics {
        self.lighting_uploads
    }
    /// Reset a reused draw when its current canonical visual has no saved
    /// lighting. Existing topology/texture reuse must not retain an old atlas.
    pub fn clear_mesh_lighting(&mut self, index: usize) -> Result<(), String> {
        let draw = self
            .draws
            .get_mut(index)
            .ok_or("Lighting mesh index unavailable")?;
        draw.lighting_parameters = [0., 1., 1.];
        draw.lighting_material = Arc::clone(&draw.material);
        self.lighting_materials
            .borrow_mut()
            .retain(|_, entry| entry.source.strong_count() != 0);
        self.queue.write_buffer(
            &draw.parameters_buffer,
            0,
            bytemuck::cast_slice(&parameters(draw.policy, draw.lighting_parameters)),
        );
        Ok(())
    }
    pub fn set_mesh_lighting(
        &mut self,
        index: usize,
        lighting: &MeshLighting,
    ) -> Result<(), String> {
        let draw = self
            .draws
            .get(index)
            .ok_or("Lighting mesh index unavailable")?;
        if matches!(draw.policy, RenderMaterial::Legacy) {
            return Err("Saved lighting requires an original material draw".into());
        }
        if lighting.uv2.len() != draw.vertex_count
            || lighting.colors.len() != draw.vertex_count
            || lighting.uv2.iter().flatten().any(|v| !v.is_finite())
            || lighting
                .colors
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
        {
            return Err("Saved lighting vertex data is incomplete or invalid".into());
        }
        let (mode, factor, base) = match lighting.mode {
            LightingMode::LightMap {
                base_texture,
                modulate_2x,
            } => (
                1.,
                if modulate_2x { 2. } else { 1. },
                if base_texture { 1. } else { 0. },
            ),
            LightingMode::VertexColors { modulate_2x } => {
                (2., if modulate_2x { 2. } else { 1. }, 1.)
            }
        };
        let atlas = match lighting.mode {
            LightingMode::LightMap { .. } => {
                let source = lighting
                    .atlas
                    .as_ref()
                    .ok_or("Saved lightmap atlas is unavailable")?;
                if source.width == 0
                    || source.height == 0
                    || source.width > 8192
                    || source.height > 8192
                    || source.rgba.len() != source.width as usize * source.height as usize * 4
                {
                    return Err("Saved lightmap atlas extent is invalid".into());
                }
                let key = (Arc::as_ptr(source) as usize, false);
                let mut cache = self.lighting_materials.borrow_mut();
                if let Some(entry) = cache.get(&key) {
                    Arc::clone(&entry.material)
                } else {
                    let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                        label: Some("Saved lightmap linear RGB atlas"),
                        size: wgpu::Extent3d {
                            width: source.width,
                            height: source.height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                        view_formats: &[],
                    });
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        &source.rgba,
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(source.width * 4),
                            rows_per_image: Some(source.height),
                        },
                        texture.size(),
                    );
                    self.lighting_uploads.atlas_uploads =
                        self.lighting_uploads.atlas_uploads.saturating_add(1);
                    let sampler = self.device.create_sampler(&wgpu::SamplerDescriptor {
                        mag_filter: wgpu::FilterMode::Linear,
                        min_filter: wgpu::FilterMode::Linear,
                        ..Default::default()
                    });
                    let group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Saved lightmap atlas"),
                        layout: &self.texture_layout,
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
                    let material = Arc::new(GpuMaterial { texture, group });
                    cache.insert(
                        key,
                        CachedMaterial {
                            source: Arc::downgrade(source),
                            material: Arc::clone(&material),
                        },
                    );
                    material
                }
            }
            LightingMode::VertexColors { .. } => Arc::clone(&draw.material),
        };
        let draw = &mut self.draws[index];
        // The draw owns a topology-sized COPY_DST buffer. Compare bytes, so
        // signed zero changes are not hidden by floating-point equality.
        let vertices = || {
            lighting
                .uv2
                .iter()
                .zip(&lighting.colors)
                .map(|(uv, c)| [uv[0], uv[1], c[0], c[1], c[2], c[3]])
        };
        if draw
            .lighting_vertices
            .iter()
            .zip(vertices())
            .any(|(old, new)| bytemuck::bytes_of(old) != bytemuck::bytes_of(&new))
        {
            for (old, new) in draw.lighting_vertices.iter_mut().zip(vertices()) {
                *old = new;
            }
            self.queue.write_buffer(
                &draw.lighting_vertex,
                0,
                bytemuck::cast_slice(&draw.lighting_vertices),
            );
            self.lighting_uploads.vertex_uploads =
                self.lighting_uploads.vertex_uploads.saturating_add(1);
        }
        draw.lighting_material = atlas;
        draw.lighting_parameters = [mode, factor, base];
        self.queue.write_buffer(
            &draw.parameters_buffer,
            0,
            bytemuck::cast_slice(&parameters(draw.policy, draw.lighting_parameters)),
        );
        self.lighting_materials
            .borrow_mut()
            .retain(|_, entry| entry.source.strong_count() != 0);
        Ok(())
    }
}
