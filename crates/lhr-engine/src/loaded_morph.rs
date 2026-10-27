//! MRPH owns a render-position LOD separately from its shared canonical mesh.
//! UpdateMorphLod's strided-output contract retains each visual's positions
//! across aliased mesh writes and unchanged update counts. The existing manager
//! owns a bounded LVERTEX-layout CPU buffer; complete native lighting/driver
//! buffers and partial allocation/failure behavior remain unclaimed.
use super::*;
use crate::i3d_object::PlainLodKey;
use crate::singlemorph_state::SinglemorphState;

/// Borrowed positions from the actual per-visual render LOD allocation. Shared
/// canonical normals and UVs remain separate, live source inputs for lighting.
pub(crate) struct MorphRenderPositions<'a> {
    vertices: &'a [u8],
}
impl MorphRenderPositions<'_> {
    pub(crate) fn len(&self) -> usize {
        self.vertices.len() / 24
    }
    pub(crate) fn position(&self, index: usize) -> Result<[f32; 3]> {
        let start = index
            .checked_mul(24)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let vertex = self
            .vertices
            .get(start..start + 12)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let position = std::array::from_fn(|i| {
            f32::from_le_bytes(vertex[i * 4..i * 4 + 4].try_into().unwrap())
        });
        if !position.into_iter().all(f32::is_finite) {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        Ok(position)
    }
}
impl LoadedVisualResources {
    pub(crate) fn morph_render_positions(
        &self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<MorphRenderPositions<'_>> {
        let object = self.objects.object(object)?;
        if object.visual_type() != crate::i3d_object::ObjectVisualType::Morph {
            return Err(LoadedVisualError::UnsupportedVisual(Some(5)));
        }
        let key = object
            .lods
            .get(index as usize)
            .copied()
            .flatten()
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let mesh = object.mesh.ok_or(LoadedVisualError::InvalidGeometry)?;
        let level = self
            .deform_mesh_lod(mesh, 0, index)?
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let count = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
            .num_vertices;
        let descriptor = self.objects.lod(key)?.vertex_buffer;
        let owner = self
            .objects
            .lod_bindings()
            .iter()
            .find(|b| {
                b.lod == key && b.state == crate::managed_visual_resources::BindingState::Installed
            })
            .ok_or(LoadedVisualError::InvalidGeometry)?
            .owner;
        let configuration = self
            .objects
            .managed()
            .vertex_storage_configuration(owner, descriptor, count)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e)))?;
        if configuration.fvf != 0x142
            || configuration.stride != Some(24)
            || configuration.format != 0
        {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        Ok(MorphRenderPositions {
            vertices: self.inspection_graphics(
                GraphicsBuffer::Vertex(
                    descriptor
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                descriptor.base as usize * 24,
                count as usize * 24,
                Some(0x142),
            )?,
        })
    }
    fn ensure_morph_render_lod(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
        canonical: &[lhr_assets::Vertex],
    ) -> Result<PlainLodKey> {
        if let Some(key) = self
            .objects
            .object(object)?
            .lods
            .get(index as usize)
            .copied()
            .flatten()
        {
            return Ok(key);
        }
        let result = (|| {
            let key = self.objects.create_plain_lod()?;
            self.objects.adopt_plain_lod(object, index, key)?;
            let request = VertexRequest {
                fvf: 0x142,
                usage: 0x8,
                stride: 24,
                count: u32::try_from(canonical.len())
                    .map_err(|_| LoadedVisualError::InvalidGeometry)?,
                format: 0,
                exclusive: false,
            };
            if !self
                .objects
                .allocate_plain_lod_vertices(key, request, &mut self.backend)?
            {
                return Err(LoadedVisualError::Backend(
                    "MRPH render LOD allocation rejected",
                ));
            }
            // Native lod::Create initializes from the currently locked canonical
            // positions and UVs, not immutable file data. Only represented
            // position/UV fields are used; color remains owned by lighting.
            let mut data = vec![0; canonical.len() * 24];
            for (vertex, bytes) in canonical
                .iter()
                .zip(data.as_chunks_mut::<24>().0.iter_mut())
            {
                for (word, value) in bytes[..12]
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(vertex.position)
                {
                    word.copy_from_slice(&value.to_le_bytes());
                }
                for (word, value) in bytes[16..24]
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(vertex.uv)
                {
                    word.copy_from_slice(&value.to_le_bytes());
                }
            }
            let descriptor = self.objects.lod(key)?.vertex_buffer;
            self.backend.upload_graphics(
                GraphicsBuffer::Vertex(
                    descriptor
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                descriptor.base as usize * 24,
                &data,
            )?;
            self.objects.unlock_all_vertices(&mut self.backend)?;
            Ok(key)
        })();
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
    pub(crate) fn update_selected_morph_lod(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
        update_count: u32,
        record: &lhr_assets::MorphRecord,
        state: &mut SinglemorphState,
    ) -> Result<()> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let resource = self.objects.object(object)?;
        if resource.visual_type() != crate::i3d_object::ObjectVisualType::Morph {
            return Err(LoadedVisualError::UnsupportedVisual(Some(5)));
        }
        let mesh = resource.mesh.ok_or(LoadedVisualError::InvalidGeometry)?;
        let level = self
            .deform_mesh_lod(mesh, 0, index)?
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let level = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let descriptor = level.vertex_buffer;
        let graphics = GraphicsBuffer::Vertex(
            descriptor
                .graphics
                .ok_or(LoadedVisualError::InvalidGeometry)?,
        );
        let offset = descriptor.base as usize * 32;
        let data = self.inspection_graphics(
            graphics,
            offset,
            level.num_vertices as usize * 32,
            Some(0x112),
        )?;
        let mut vertices = data
            .as_chunks::<32>()
            .0
            .iter()
            .map(|v| {
                let word = |i: usize| f32::from_le_bytes(v[i..i + 4].try_into().unwrap());
                lhr_assets::Vertex {
                    position: [word(0), word(4), word(8)],
                    normal: [word(12), word(16), word(20)],
                    uv: [word(24), word(28)],
                }
            })
            .collect::<Vec<_>>();
        let key = self.ensure_morph_render_lod(object, index, &vertices)?;
        if self.objects.lod(key)?.update_count == update_count {
            return Ok(());
        }
        // Stage arithmetic first. A supported update writes canonical positions
        // and normals plus indexed positions in this visual's retained render
        // buffer. Untouched render vertices must survive other mesh aliases.
        if state
            .update_morph_lod(record, index as usize, &mut vertices)
            .map_err(LoadedVisualError::Morph)?
        {
            let mut render_data = self
                .morph_render_positions(object, index)?
                .vertices
                .to_vec();
            let channels = &record
                .definition
                .as_ref()
                .ok_or(LoadedVisualError::InvalidGeometry)?
                .lods[index as usize]
                .channels;
            for channel in channels {
                for &vertex in &channel.vertex_indices {
                    let vertex = vertex as usize;
                    for axis in 0..3 {
                        render_data[vertex * 24 + axis * 4..vertex * 24 + axis * 4 + 4]
                            .copy_from_slice(&vertices[vertex].position[axis].to_le_bytes());
                    }
                }
            }
            let mut output = Vec::with_capacity(vertices.len() * 32);
            for vertex in vertices {
                for word in vertex
                    .position
                    .into_iter()
                    .chain(vertex.normal)
                    .chain(vertex.uv)
                {
                    output.extend_from_slice(&word.to_le_bytes());
                }
            }
            self.backend.upload_graphics(graphics, offset, &output)?;
            let render = self.objects.lod(key)?.vertex_buffer;
            self.backend.upload_graphics(
                GraphicsBuffer::Vertex(render.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
                render.base as usize * 24,
                &render_data,
            )?;
        }
        self.objects
            .commit_morph_lod_update_count(key, update_count)?;
        Ok(())
    }
    /// LoadMeshImage's file-instance prefix shares its preceding mutable mesh;
    /// derived cache Duplicate below instead creates independent mesh storage.
    pub fn load_morph_mesh_instance(
        &mut self,
        original: ObjectResourceKey,
    ) -> Result<ObjectResourceKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let mesh = self.objects.object(original)?.mesh;
        let materials = self.source_materials(original)?.to_vec();
        let object = self.objects.create_morph_resources(mesh)?;
        self.sources
            .insert((object.arena(), object.slot()), materials);
        Ok(object)
    }
    pub fn load_morph_instance(
        &mut self,
        original: ObjectResourceKey,
    ) -> Result<ObjectResourceKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let source = self.objects.object(original)?;
        if source.visual_type() != crate::i3d_object::ObjectVisualType::Morph {
            return Err(LoadedVisualError::UnsupportedVisual(Some(5)));
        }
        let mesh = source.mesh.ok_or(LoadedVisualError::InvalidGeometry)?;
        let materials = self.source_materials(original)?.to_vec();
        let result = (|| {
            let duplicate = self.deform_create_empty_mesh()?;
            if let Err(error) = self.deform_duplicate_mesh(duplicate, Some(mesh)) {
                let _ = self.deform_release_mesh(duplicate);
                return Err(error);
            }
            let object = self.objects.create_morph_resources(Some(duplicate))?;
            self.deform_release_mesh(duplicate)?;
            self.sources
                .insert((object.arena(), object.slot()), materials);
            Ok(object)
        })();
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
    pub fn load_morph_source(
        &mut self,
        source: &lhr_assets::Model,
        index: usize,
        installation: &lhr_assets::Installation,
    ) -> Result<ObjectResourceKey> {
        let frame = source
            .frames
            .get(index)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if frame.kind != 1
            || frame.visual != Some(5)
            || frame.skin.is_some()
            || frame.morph.is_none()
        {
            return Err(LoadedVisualError::UnsupportedVisual(frame.visual));
        }
        self.load_base_object(source, index, installation)
    }
}
