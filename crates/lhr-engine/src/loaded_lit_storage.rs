//! Selected saved-lighting presentation preparation in the same live owner.
//! The HWTL layouts follow pinned mafia-re29b65b0 litLod::Create. This host
//! adapter prepares buffers after Load; it does not claim native lazy render or
//! serialized IB-allocation timing. Load/HRESULT/cursor semantics stay separate.
use super::*;
use crate::i3d_object::{saved_lightmaps::SavedLitLod, LitStoragePhase};

impl LoadedVisualResources {
    /// Build only the selected actual lit LOD. Its inspection view borrows
    /// these manager allocations, not another geometry or atlas registry.
    pub fn prepare_saved_lit_storage(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<()> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let record = self.objects.object(object)?;
        let Some(lit) = record.lit_object() else {
            return Ok(());
        };
        let level = lit
            .levels
            .get(lit.active_level)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if !level.constructed {
            return Ok(());
        }
        let key = level
            .lods
            .get(index as usize)
            .copied()
            .flatten()
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        match self.objects.lit_lod_storage(key)?.phase {
            LitStoragePhase::Ready | LitStoragePhase::Unprepared => (),
            _ => return Err(LoadedVisualError::IncompleteAdmission),
        }
        let payload = level
            .saved_lods
            .get(index as usize)
            .and_then(Option::as_ref)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let mesh = self
            .objects
            .managed()
            .resources()
            .mesh_object(record.mesh.ok_or(LoadedVisualError::InvalidGeometry)?)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        if mesh.u_ofs != 0. || mesh.v_ofs != 0. {
            return Err(LoadedVisualError::Backend(
                "constructed lightmap animated UV cache unavailable",
            ));
        }
        let source_key = mesh
            .levels
            .get(index as usize)
            .copied()
            .flatten()
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let source = self
            .objects
            .managed()
            .resources()
            .mesh_level(source_key)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let source_owner = self
            .objects
            .managed()
            .vertex_bindings()
            .iter()
            .find(|b| {
                b.level == source_key
                    && b.state == crate::managed_visual_resources::BindingState::Installed
            })
            .ok_or(LoadedVisualError::InvalidGeometry)?
            .owner;
        let configuration = self
            .objects
            .managed()
            .vertex_storage_configuration(source_owner, source.vertex_buffer, source.num_vertices)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e)))?;
        if configuration.fvf != 0x112
            || configuration.stride != Some(32)
            || configuration.format != 6
        {
            return Err(LoadedVisualError::Backend(
                "saved lighting requires original source vertices",
            ));
        }
        let graphics = GraphicsBuffer::Vertex(
            source
                .vertex_buffer
                .graphics
                .ok_or(LoadedVisualError::InvalidGeometry)?,
        );
        let source_binding = crate::i3d_object::LitSourceBinding {
            level: source_key,
            vertices: source.vertex_buffer,
            vertex_count: source.num_vertices,
            revision: self
                .backend
                .graphics
                .get(&CpuVisualStorage::graphics_id(graphics))
                .ok_or(LoadedVisualError::InvalidGeometry)?
                .revision,
        };
        let previous = self.objects.lit_lod_storage(key)?;
        if previous.phase == LitStoragePhase::Ready
            && previous.source_binding == Some(source_binding)
            && !self.objects.lod(key)?.dirty
        {
            return Ok(());
        }
        let replace_vertices = previous.phase == LitStoragePhase::Ready;
        let original = self.inspection_graphics(
            GraphicsBuffer::Vertex(
                source
                    .vertex_buffer
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            source.vertex_buffer.base as usize * 32,
            source.num_vertices as usize * 32,
            Some(0x112),
        )?;
        let (request, vertices, indices) = match payload {
            SavedLitLod::VertexColors { colors, .. } => {
                if colors.len() != source.num_vertices as usize {
                    return Err(LoadedVisualError::InvalidGeometry);
                }
                let mut vertices = bytes(colors.len() * 24)?;
                for ((source, target), color) in original
                    .as_chunks::<32>()
                    .0
                    .iter()
                    .zip(vertices.as_chunks_mut::<24>().0.iter_mut())
                    .zip(colors)
                {
                    target[..12].copy_from_slice(&source[..12]);
                    target[12..16].copy_from_slice(&color.to_le_bytes());
                    target[16..24].copy_from_slice(&source[24..32]);
                }
                (
                    VertexRequest {
                        fvf: 0x142,
                        usage: 0x8,
                        stride: 24,
                        count: source.num_vertices,
                        format: 0,
                        exclusive: false,
                    },
                    vertices,
                    None,
                )
            }
            SavedLitLod::Lightmap(lod) => {
                let count = u32::try_from(lod.vertices.len())
                    .map_err(|_| LoadedVisualError::InvalidGeometry)?;
                let mut vertices = bytes(lod.vertices.len() * 32)?;
                for (vertex, target) in lod
                    .vertices
                    .iter()
                    .zip(vertices.as_chunks_mut::<32>().0.iter_mut())
                {
                    let at = usize::from(vertex.source_vertex) * 32;
                    let source = original
                        .get(at..at + 32)
                        .ok_or(LoadedVisualError::InvalidGeometry)?;
                    target[..12].copy_from_slice(&source[..12]);
                    // Existing canonical admitted LM presentation has zero
                    // dynamic diffuse, with alpha one. Its real sector/light
                    // checks still run before any draw; no empty-list premise.
                    target[12..16].copy_from_slice(&0xff000000u32.to_le_bytes());
                    target[16..20].copy_from_slice(&vertex.lightmap_uv[0].to_le_bytes());
                    target[20..24].copy_from_slice(&vertex.lightmap_uv[1].to_le_bytes());
                    target[24..32].copy_from_slice(&source[24..32]);
                }
                let mut indices = bytes(lod.indices.len() * 2)?;
                for (target, index) in indices.as_chunks_mut::<2>().0.iter_mut().zip(&lod.indices) {
                    if u32::from(*index) >= count {
                        return Err(LoadedVisualError::InvalidGeometry);
                    }
                    *target = index.to_le_bytes();
                }
                (
                    VertexRequest {
                        fvf: 0x242,
                        usage: 0x8,
                        stride: 32,
                        count,
                        format: 1,
                        exclusive: false,
                    },
                    vertices,
                    Some(indices),
                )
            }
        };
        if replace_vertices {
            if let Err(error) = self
                .objects
                .release_lit_render_vertices(key, &mut self.backend)
            {
                self.incomplete = true;
                return Err(error.into());
            }
        }
        self.objects.begin_lit_preparation(key)?;
        let mut provider_interruption = false;
        let result = (|| -> Result<()> {
            if let Some(indices) = &indices {
                let count = u32::try_from(indices.len() / 2)
                    .map_err(|_| LoadedVisualError::InvalidGeometry)?;
                if !self
                    .objects
                    .allocate_saved_lit_indices(key, count, &mut self.backend)?
                {
                    return Err(LoadedVisualError::Backend(
                        "saved lighting index allocation rejected",
                    ));
                }
                let descriptor = self.objects.saved_lit_index_descriptor(key, count)?;
                if let Err(error) = self.backend.upload_graphics(
                    GraphicsBuffer::Index(
                        descriptor
                            .graphics
                            .ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    descriptor.base as usize * 2,
                    indices,
                ) {
                    provider_interruption = true;
                    return Err(error);
                }
            }
            if !self
                .objects
                .allocate_plain_lod_vertices(key, request, &mut self.backend)?
            {
                return Err(LoadedVisualError::Backend(
                    "saved lighting vertex allocation rejected",
                ));
            }
            let descriptor = self.objects.lod(key)?.vertex_buffer;
            let owner = self
                .objects
                .lod_bindings()
                .iter()
                .find(|b| {
                    b.lod == key
                        && b.state == crate::managed_visual_resources::BindingState::Installed
                })
                .ok_or(LoadedVisualError::InvalidGeometry)?
                .owner;
            let actual = self
                .objects
                .managed()
                .vertex_storage_configuration(owner, descriptor, request.count)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e)))?;
            if actual.fvf != request.fvf
                || actual.stride != Some(request.stride)
                || actual.format != request.format
            {
                return Err(LoadedVisualError::Backend(
                    "saved lighting manager returned an incompatible layout",
                ));
            }
            if let Err(error) = self.backend.upload_graphics(
                GraphicsBuffer::Vertex(
                    descriptor
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                descriptor.base as usize * request.stride as usize,
                &vertices,
            ) {
                provider_interruption = true;
                return Err(error);
            }
            self.objects.unlock_all_indices(&mut self.backend)?;
            self.objects.unlock_all_vertices(&mut self.backend)?;
            self.objects.finish_lit_preparation(key, source_binding)?;
            Ok(())
        })();
        if result.is_err() {
            self.objects
                .interrupt_lit_preparation(key, provider_interruption);
            self.incomplete = true;
        }
        result
    }
}
