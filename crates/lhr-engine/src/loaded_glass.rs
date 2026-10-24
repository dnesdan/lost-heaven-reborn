//! Glass fragments use the same real mesh/material and CPU graphics owners.
use super::*;
use crate::glass_subdivision::{GlassGeometry, GlassVertex};
use crate::visual_resources::{FaceGroupKey, MeshObjectKey};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GlassResource {
    Group(FaceGroupKey),
    Level(MeshLevelKey),
    Mesh(MeshObjectKey),
}
impl LoadedVisualResources {
    pub(crate) fn glass_new_object(&mut self) -> Result<ObjectResourceKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        self.objects
            .create_object_resources(None)
            .map_err(Into::into)
    }
    fn glass_first_level(
        &self,
        object: ObjectResourceKey,
    ) -> Result<(MeshLevelKey, crate::visual_resources::FaceGroupKey)> {
        let mesh = self
            .object_mesh(object)?
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let level = self
            .deform_mesh_lod(mesh, 0, 0)?
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let group = self
            .deform_lod_face_group(level, 0)?
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        Ok((level, group))
    }
    pub(crate) fn glass_copy_material_to(
        &mut self,
        object: ObjectResourceKey,
        output: MaterialKey,
    ) -> Result<()> {
        let (level, group) = self.glass_first_level(object)?;
        self.game_lock_level_vertices(level, 0)?;
        let source = self
            .deform_face_group_material(group)?
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        self.deform_set_material_copy(output, Some(source))?;
        Ok(())
    }
    pub(crate) fn glass_source_geometry(
        &mut self,
        object: ObjectResourceKey,
    ) -> Result<GlassGeometry> {
        let (level, group) = self.glass_first_level(object)?;
        if !self.game_lock_level_vertices(level, 0)? {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let count = self.game_level_word(level, 8)?;
        let mut vertices = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let mut words = [0.; 8];
            for (j, value) in words.iter_mut().enumerate() {
                let offset = i * 32 + j * 4;
                let mut bytes = [0; 4];
                for (k, byte) in bytes.iter_mut().enumerate() {
                    *byte = self.game_vertex_byte(level, offset + k)?;
                }
                *value = f32::from_le_bytes(bytes);
            }
            vertices.push(GlassVertex {
                position: words[..3].try_into().unwrap(),
                normal: words[3..6].try_into().unwrap(),
                uv: words[6..].try_into().unwrap(),
            });
        }
        self.objects
            .lock_face_group_indices(group, &mut self.backend)?;
        let g = self
            .objects
            .managed()
            .resources()
            .face_group(group)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let descriptor = g.index_buffer;
        let length = g.num_faces as usize * 6;
        let graphics = GraphicsBuffer::Index(
            descriptor
                .graphics
                .ok_or(LoadedVisualError::InvalidGeometry)?,
        );
        let allocation = self
            .backend
            .graphics
            .get(&CpuVisualStorage::graphics_id(graphics))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if allocation.lock.is_none() {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let bytes = allocation
            .bytes
            .get(descriptor.base as usize * 2..descriptor.base as usize * 2 + length)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let faces = bytes
            .as_chunks::<6>()
            .0
            .iter()
            .map(|b| {
                [
                    u16::from_le_bytes([b[0], b[1]]),
                    u16::from_le_bytes([b[2], b[3]]),
                    u16::from_le_bytes([b[4], b[5]]),
                ]
            })
            .collect();
        // CutMesh itself calls only level UnlockVertices; the retained IB lock
        // remains in its real manager until the driver's UnlockAll boundary.
        self.game_unlock_level_vertices(level)?;
        Ok(GlassGeometry { vertices, faces })
    }
    #[cfg(test)]
    pub(crate) fn glass_fragment_mesh(
        &mut self,
        material: MaterialKey,
        vertices: &[GlassVertex; 6],
    ) -> Result<MeshObjectKey> {
        self.glass_geometry_mesh(material, vertices, &[[0, 1, 2], [3, 4, 5]])
    }
    #[cfg(test)]
    pub(crate) fn glass_geometry_mesh(
        &mut self,
        material: MaterialKey,
        vertices: &[GlassVertex],
        faces: &[[u16; 3]],
    ) -> Result<MeshObjectKey> {
        let group = self.glass_begin_group(material, faces)?;
        self.glass_finish_group(group, vertices)
    }
    pub(crate) fn glass_begin_group(
        &mut self,
        material: MaterialKey,
        faces: &[[u16; 3]],
    ) -> Result<crate::visual_resources::FaceGroupKey> {
        let face_count =
            u32::try_from(faces.len()).map_err(|_| LoadedVisualError::InvalidGeometry)?;
        if face_count == 0 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let group = self.objects.create_face_group()?;
        self.glass_pending_resources
            .push(GlassResource::Group(group));
        self.objects
            .set_material(group, Some(material), &mut self.backend)?;
        if !self
            .objects
            .allocate_group_indices(group, face_count, 0x10, &mut self.backend)?
        {
            return Err(LoadedVisualError::Backend(
                "glass index allocation rejected",
            ));
        }
        let indices = self
            .objects
            .managed()
            .resources()
            .face_group(group)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
            .index_buffer;
        let mut data = Vec::with_capacity(faces.len() * 6);
        for face in faces {
            for index in face {
                data.extend_from_slice(&index.to_le_bytes());
            }
        }
        self.backend.upload_graphics(
            GraphicsBuffer::Index(indices.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
            indices.base as usize * 2,
            &data,
        )?;
        Ok(group)
    }
    pub(crate) fn glass_finish_group(
        &mut self,
        group: crate::visual_resources::FaceGroupKey,
        vertices: &[GlassVertex],
    ) -> Result<MeshObjectKey> {
        let vertex_count =
            u32::try_from(vertices.len()).map_err(|_| LoadedVisualError::InvalidGeometry)?;
        if vertex_count == 0 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let level = self.objects.create_mesh_level()?;
        self.glass_pending_resources
            .push(GlassResource::Level(level));
        let request = VertexRequest {
            fvf: 0x112,
            usage: 0x10,
            stride: 32,
            count: vertex_count,
            format: 6,
            exclusive: false,
        };
        if !self.objects.allocate_level_vertices(
            level,
            vertex_count,
            request,
            None,
            &mut self.backend,
        )? {
            return Err(LoadedVisualError::Backend(
                "glass vertex allocation rejected",
            ));
        }
        let descriptor = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
            .vertex_buffer;
        let mut bytes = Vec::with_capacity(vertices.len() * 32);
        for vertex in vertices {
            for value in vertex
                .position
                .into_iter()
                .chain(vertex.normal)
                .chain(vertex.uv)
            {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        self.backend.upload_graphics(
            GraphicsBuffer::Vertex(
                descriptor
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            descriptor.base as usize * 32,
            &bytes,
        )?;
        #[cfg(test)]
        if self.glass_fault_after_vertex_upload {
            return Err(LoadedVisualError::Backend(
                "injected glass host failure after actual vertex upload",
            ));
        }
        self.objects.append_duplicate_group(level, group)?;
        self.glass_transfer_resource(GlassResource::Group(group))?;
        // Original AddFGroup can set environment-map flags; fragment code
        // explicitly clears MESHF_ENVMAP (8) before recomputing the actual box.
        self.objects.clear_mesh_level_environment(level)?;
        self.game_update_level_bbox(level)?;
        let mesh = self.objects.create_mesh_object()?;
        self.glass_pending_resources.push(GlassResource::Mesh(mesh));
        self.objects
            .set_lod(mesh, Some(level), 0, &mut self.backend)?;
        self.objects.release_mesh_level(level, &mut self.backend)?;
        self.glass_transfer_resource(GlassResource::Level(level))?;
        self.objects.update_mesh_bound_volume(mesh)?;
        Ok(mesh)
    }
}

impl LoadedVisualResources {
    pub(crate) fn glass_borrow_material(&self, object: ObjectResourceKey) -> Result<MaterialKey> {
        let (_, group) = self.glass_first_level(object)?;
        self.deform_face_group_material(group)?
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
    pub(crate) fn glass_read_triangle(
        &mut self,
        object: ObjectResourceKey,
    ) -> Result<[GlassVertex; 3]> {
        let (level, _) = self.glass_first_level(object)?;
        if self.game_level_word(level, 8)? < 3 || !self.game_lock_level_vertices(level, 0)? {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let mut vertices = [GlassVertex {
            position: [0.; 3],
            normal: [0.; 3],
            uv: [0.; 2],
        }; 3];
        for (i, vertex) in vertices.iter_mut().enumerate() {
            let mut values = [0.; 8];
            for (j, value) in values.iter_mut().enumerate() {
                let mut bytes = [0; 4];
                for (k, byte) in bytes.iter_mut().enumerate() {
                    *byte = self.game_vertex_byte(level, i * 32 + j * 4 + k)?;
                }
                *value = f32::from_le_bytes(bytes);
            }
            *vertex = GlassVertex {
                position: values[..3].try_into().unwrap(),
                normal: values[3..6].try_into().unwrap(),
                uv: values[6..].try_into().unwrap(),
            };
        }
        self.game_unlock_level_vertices(level)?;
        Ok(vertices)
    }
}

impl LoadedVisualResources {
    fn glass_transfer_resource(&mut self, resource: GlassResource) -> Result<()> {
        let index = self
            .glass_pending_resources
            .iter()
            .rposition(|r| *r == resource)
            .ok_or(LoadedVisualError::Backend(
                "glass factory ownership missing",
            ))?;
        self.glass_pending_resources.remove(index);
        Ok(())
    }
    pub(crate) fn glass_release_mesh(&mut self, mesh: MeshObjectKey) -> Result<()> {
        self.objects.release_mesh_object(mesh, &mut self.backend)?;
        self.glass_transfer_resource(GlassResource::Mesh(mesh))
    }
    pub(crate) fn glass_cleanup_resources(&mut self) -> Result<()> {
        while let Some(resource) = self.glass_pending_resources.last().copied() {
            match resource {
                GlassResource::Group(group) => {
                    self.objects.destroy_face_group(group, &mut self.backend)?
                }
                GlassResource::Level(level) => {
                    self.objects.release_mesh_level(level, &mut self.backend)?;
                }
                GlassResource::Mesh(mesh) => {
                    self.objects.release_mesh_object(mesh, &mut self.backend)?;
                }
            }
            self.glass_pending_resources.pop();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn real_shard_owns_six_vertices_two_faces_and_shared_material_until_last_release() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let material = owner.objects.create_material().unwrap();
        let first = [
            GlassVertex {
                position: [-0.1, -0.1, 0.],
                normal: [0., 0., -1.],
                uv: [0., 0.],
            },
            GlassVertex {
                position: [0., 0.1, 0.],
                normal: [0., 0., -1.],
                uv: [0.5, 1.],
            },
            GlassVertex {
                position: [0.1, -0.1, 0.],
                normal: [0., 0., -1.],
                uv: [1., 0.],
            },
        ];
        let mut vertices = [first[0], first[1], first[2], first[0], first[2], first[1]];
        for vertex in &mut vertices[3..] {
            vertex.normal = vertex.normal.map(|v| -v);
        }
        let mesh = owner.glass_fragment_mesh(material, &vertices).unwrap();
        let object = owner.objects.create_object_resources(Some(mesh)).unwrap();
        owner.glass_release_mesh(mesh).unwrap();
        assert_eq!(
            owner
                .objects
                .managed()
                .resources()
                .material(material)
                .unwrap()
                .references,
            2
        );
        owner.deform_release_material(material).unwrap();
        owner.game_unlock_frame_buffers().unwrap();
        {
            let view = owner.inspection_mesh(object, 1).unwrap().unwrap();
            assert_eq!(view.vertex_count, 6);
            assert_eq!(view.groups.len(), 1);
            assert_eq!(
                view.groups[0].index_bytes,
                [0, 0, 1, 0, 2, 0, 3, 0, 4, 0, 5, 0]
            );
            for (i, vertex) in vertices.iter().enumerate() {
                for (j, value) in vertex
                    .position
                    .into_iter()
                    .chain(vertex.normal)
                    .chain(vertex.uv)
                    .enumerate()
                {
                    assert_eq!(
                        &view.vertex_bytes[i * 32 + j * 4..i * 32 + j * 4 + 4],
                        &value.to_le_bytes()
                    );
                }
            }
        }
        let geometry = owner.glass_source_geometry(object).unwrap();
        assert_eq!(geometry.vertices, vertices);
        assert_eq!(geometry.faces, [[0, 1, 2], [3, 4, 5]]);
        assert_eq!(owner.objects.managed().managers().index_locked().len(), 1);
        assert_eq!(owner.objects.managed().managers().vertex_locked().len(), 0);
        let copy = owner.deform_create_material().unwrap();
        owner.glass_copy_material_to(object, copy).unwrap();
        assert_ne!(copy, material);
        let repeated = owner.glass_source_geometry(object).unwrap();
        assert_eq!(repeated, geometry);
        owner.game_unlock_frame_buffers().unwrap();
        owner.destroy_object(object).unwrap();
        owner.deform_release_material(copy).unwrap();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_graphics(), 0);
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.live_textures(), 0);
    }
}

#[cfg(test)]
mod failure_tests {
    use super::*;
    #[test]
    fn error_after_actual_index_and_vertex_upload_retains_each_prefix_owner_until_close() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let material = owner.deform_create_material().unwrap();
        let vertices = [GlassVertex {
            position: [0., 0., 0.],
            normal: [0., 1., 0.],
            uv: [0.; 2],
        }; 3];
        owner.glass_fault_after_vertex_upload = true;
        assert!(owner
            .glass_geometry_mesh(material, &vertices, &[[0, 1, 2]])
            .is_err());
        assert_eq!(owner.glass_pending_resources.len(), 2);
        assert!(owner.backend.live_graphics() >= 2);
        assert!(owner.backend.live_allocations() > 0);
        assert_eq!(
            owner
                .objects
                .managed()
                .resources()
                .material(material)
                .unwrap()
                .references,
            2
        );
        owner.game_unlock_frame_buffers().unwrap();
        owner.glass_cleanup_resources().unwrap();
        assert!(owner.glass_pending_resources.is_empty());
        assert_eq!(
            owner
                .objects
                .managed()
                .resources()
                .material(material)
                .unwrap()
                .references,
            1
        );
        owner.deform_release_material(material).unwrap();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_graphics(), 0);
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.live_textures(), 0);
    }
}

#[cfg(test)]
impl LoadedVisualResources {
    pub(crate) fn mark_glass_admission_unavailable_for_test(&mut self) {
        self.incomplete = true;
    }
}
