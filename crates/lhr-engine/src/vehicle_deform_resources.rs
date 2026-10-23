//! Concrete resource operations consumed by vehicle deformation preparation.
//! These names describe the host bridge. They do not rename unresolved native
//! driver factory slots. All identities remain in the one live visual owner.
use super::*;
use crate::visual_resources::{FaceGroupKey, MeshObjectKey};

impl LoadedVisualResources {
    pub fn deform_create_empty_mesh(&mut self) -> Result<MeshObjectKey> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        self.objects.create_mesh_object().map_err(Into::into)
    }
    pub fn deform_release_mesh(&mut self, mesh: MeshObjectKey) -> Result<u32> {
        self.objects
            .release_mesh_object(mesh, &mut self.backend)
            .map_err(Into::into)
    }
    pub fn deform_mesh_lod_count(&self, mesh: MeshObjectKey, group: u32) -> Result<u32> {
        if group != 0 {
            return Err(LoadedVisualError::Backend("unsupported mesh group"));
        }
        self.objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map(|m| m.num_lods)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
    }
    pub fn deform_mesh_lod(
        &self,
        mesh: MeshObjectKey,
        group: u32,
        index: u32,
    ) -> Result<Option<MeshLevelKey>> {
        if group != 0 || index >= 10 {
            return Err(LoadedVisualError::Backend("unsupported mesh LOD"));
        }
        let mesh = self
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        Ok(if index < mesh.num_lods {
            mesh.levels[index as usize]
        } else {
            None
        })
    }
    pub fn deform_remove_mesh_lod(
        &mut self,
        mesh: MeshObjectKey,
        group: u32,
        index: u32,
    ) -> Result<()> {
        if group != 0 {
            return Err(LoadedVisualError::Backend("unsupported mesh group"));
        }
        self.objects
            .set_lod(mesh, None, index, &mut self.backend)
            .map_err(Into::into)
    }
    pub fn deform_lod_face_group(
        &self,
        level: MeshLevelKey,
        index: u32,
    ) -> Result<Option<FaceGroupKey>> {
        let level = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        if index >= level.num_fgroups {
            return Ok(None);
        }
        let mut next = level.fgroups;
        for _ in 0..index {
            let Some(group) = next else {
                return Err(LoadedVisualError::InvalidGeometry);
            };
            next = self
                .objects
                .managed()
                .resources()
                .face_group(group)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                .next;
        }
        Ok(next)
    }
    pub fn deform_face_group_material(&self, group: FaceGroupKey) -> Result<Option<MaterialKey>> {
        self.objects
            .managed()
            .resources()
            .face_group(group)
            .map(|g| g.material)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
    }
    pub fn deform_retain_material(&mut self, key: MaterialKey) -> Result<()> {
        self.objects.retain_material(key).map_err(Into::into)
    }
    pub fn deform_release_material(&mut self, key: MaterialKey) -> Result<u32> {
        self.objects
            .release_material(key, &mut self.backend)
            .map_err(Into::into)
    }
    pub fn deform_create_material(&mut self) -> Result<MaterialKey> {
        self.objects.create_material().map_err(Into::into)
    }
}

impl LoadedVisualResources {
    fn deform_cpu_bytes(
        &mut self,
        graphics: GraphicsBuffer,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let transient = self.backend.get_graphics_mut(graphics)?.lock.is_none();
        if transient {
            self.backend.lock(graphics, 0)?;
        }
        let end = offset
            .checked_add(size)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let result = self
            .backend
            .get_graphics_mut(graphics)?
            .bytes
            .get(offset..end)
            .ok_or(LoadedVisualError::InvalidGeometry)
            .map(<[u8]>::to_vec);
        if transient {
            self.backend.unlock(graphics)?;
        }
        result
    }
    fn deform_duplicate_lod(&mut self, source: MeshLevelKey) -> Result<MeshLevelKey> {
        let (vertices, descriptor, bbox, mut next) = {
            let r = self
                .objects
                .managed()
                .resources()
                .mesh_level(source)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            (r.num_vertices, r.vertex_buffer, r.bbox, r.fgroups)
        };
        let destination = self.objects.create_mesh_level()?;
        self.objects
            .copy_mesh_level_scalar_fields(destination, source)?;
        let vertex_data = if vertices == 0 {
            vec![]
        } else {
            self.deform_cpu_bytes(
                GraphicsBuffer::Vertex(
                    descriptor
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                descriptor.base as usize * 32,
                vertices as usize * 32,
            )?
        };
        if vertices != 0 {
            let request = VertexRequest {
                fvf: 0x112,
                usage: 0x10,
                stride: 32,
                count: vertices,
                format: 6,
                exclusive: false,
            };
            if !self.objects.allocate_level_vertices(
                destination,
                vertices,
                request,
                None,
                &mut self.backend,
            )? {
                return Err(LoadedVisualError::Backend(
                    "CPU duplicate vertex allocation rejected",
                ));
            }
            let output = self
                .objects
                .managed()
                .resources()
                .mesh_level(destination)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                .vertex_buffer;
            self.backend.upload_graphics(
                GraphicsBuffer::Vertex(output.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
                output.base as usize * 32,
                &vertex_data,
            )?;
        }
        while let Some(source_group) = next {
            let (faces, indices, material, following) = {
                let g = self
                    .objects
                    .managed()
                    .resources()
                    .face_group(source_group)
                    .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
                (g.num_faces, g.index_buffer, g.material, g.next)
            };
            let group = self.objects.create_face_group()?;
            if faces != 0 {
                if !self
                    .objects
                    .allocate_group_indices(group, faces, 0, &mut self.backend)?
                {
                    return Err(LoadedVisualError::Backend(
                        "CPU duplicate index allocation rejected",
                    ));
                }
                let data = self.deform_cpu_bytes(
                    GraphicsBuffer::Index(
                        indices.graphics.ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    indices.base as usize * 2,
                    faces as usize * 6,
                )?;
                let output = self
                    .objects
                    .managed()
                    .resources()
                    .face_group(group)
                    .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                    .index_buffer;
                self.backend.upload_graphics(
                    GraphicsBuffer::Index(
                        output.graphics.ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    output.base as usize * 2,
                    &data,
                )?;
            }
            self.objects
                .set_material(group, material, &mut self.backend)?;
            self.objects.append_duplicate_group(destination, group)?;
            next = following;
        }
        self.objects.install_mesh_level_bbox(destination, bbox)?;
        Ok(destination)
    }
    /// Mesh Duplicate315e0 over admitted plain CPU storage. Each source LOD and
    /// face group receives a distinct owned descriptor and copied vertex/index
    /// bytes; materials retain their shared original identities.
    pub fn deform_duplicate_mesh(
        &mut self,
        destination: MeshObjectKey,
        source: Option<MeshObjectKey>,
    ) -> Result<i32> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        self.objects
            .managed()
            .resources()
            .mesh_object(destination)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        if source == Some(destination) {
            return Ok(0);
        }
        if let Some(key) = source {
            self.objects
                .managed()
                .resources()
                .mesh_object(key)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        }
        let result = (|| {
            self.objects
                .clear_duplicate_mesh(destination, &mut self.backend)?;
            if source.is_none() {
                self.objects.reset_empty_mesh_bounds(destination)?;
            }
            if let Some(source) = source {
                for index in 0..10 {
                    let level = self
                        .objects
                        .managed()
                        .resources()
                        .mesh_object(source)
                        .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                        .levels[index];
                    if let Some(level) = level {
                        let copy = self.deform_duplicate_lod(level)?;
                        self.objects.set_lod(
                            destination,
                            Some(copy),
                            index as u32,
                            &mut self.backend,
                        )?;
                        self.objects.release_mesh_level(copy, &mut self.backend)?;
                    }
                }
            }
            self.objects.unlock_all_indices(&mut self.backend)?;
            self.objects.unlock_all_vertices(&mut self.backend)?;
            Ok(0)
        })();
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
}

impl LoadedVisualResources {
    pub fn deform_set_material_copy(
        &mut self,
        destination: MaterialKey,
        source: Option<MaterialKey>,
    ) -> Result<i32> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let result = self
            .objects
            .copy_material(destination, source, &mut self.backend)
            .map_err(Into::into);
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
}

impl LoadedVisualResources {
    pub fn object_mesh(&self, object: ObjectResourceKey) -> Result<Option<MeshObjectKey>> {
        self.objects
            .object(object)
            .map(|o| o.mesh)
            .map_err(Into::into)
    }
    pub fn set_object_mesh(
        &mut self,
        object: ObjectResourceKey,
        mesh: Option<MeshObjectKey>,
        preserve_lods: bool,
    ) -> Result<()> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let result = self
            .objects
            .set_object_mesh(object, mesh, preserve_lods, &mut self.backend)
            .map_err(Into::into);
        if result.is_err() {
            self.incomplete = true;
        }
        result
    }
    pub fn mesh_bounds(&self, mesh: MeshObjectKey) -> Result<MeshBoundVolume> {
        let mesh = self
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        mesh.bound_volume()
            .copied()
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
}

impl LoadedVisualResources {
    pub fn lookup_mesh(&self, slot: u32) -> Result<MeshObjectKey> {
        self.objects
            .managed()
            .resources()
            .lookup_mesh(slot)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
    }
    pub fn lookup_lod(&self, slot: u32) -> Result<MeshLevelKey> {
        self.objects
            .managed()
            .resources()
            .lookup_lod(slot)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
    }
    pub fn lookup_face_group(&self, slot: u32) -> Result<FaceGroupKey> {
        self.objects
            .managed()
            .resources()
            .lookup_face_group(slot)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
    }
    pub fn lookup_material(&self, slot: u32) -> Result<MaterialKey> {
        self.objects
            .managed()
            .resources()
            .lookup_material(slot)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clone_owns_distinct_lods_and_bytes_but_shared_material_then_trims_only_clone() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let source = owner.deform_create_empty_mesh().unwrap();
        let material = owner.deform_create_material().unwrap();
        let texture = owner
            .backend
            .admit_texture_with_flags(
                "synthetic|",
                lhr_assets::Texture {
                    width: 1,
                    height: 1,
                    rgba: vec![1, 2, 3, 255],
                },
                None,
                0x108,
                0x108,
            )
            .unwrap();
        owner
            .objects
            .set_texture(material, Some(texture), &mut owner.backend)
            .unwrap();
        owner.backend.release_texture(texture).unwrap();
        for slot in 0..3 {
            let group = owner.objects.create_face_group().unwrap();
            owner
                .objects
                .allocate_group_indices(group, 1, 0, &mut owner.backend)
                .unwrap();
            let d = owner
                .objects
                .managed()
                .resources()
                .face_group(group)
                .unwrap()
                .index_buffer;
            owner
                .backend
                .upload_graphics(
                    GraphicsBuffer::Index(d.graphics.unwrap()),
                    d.base as usize * 2,
                    &[0, 0, 1, 0, 2, 0],
                )
                .unwrap();
            owner
                .objects
                .set_material(group, Some(material), &mut owner.backend)
                .unwrap();
            let level = owner.objects.create_mesh_level().unwrap();
            owner
                .objects
                .allocate_level_vertices(
                    level,
                    3,
                    VertexRequest {
                        fvf: 0x112,
                        usage: 0x10,
                        stride: 32,
                        count: 3,
                        format: 6,
                        exclusive: false,
                    },
                    Some(group),
                    &mut owner.backend,
                )
                .unwrap();
            let d = owner
                .objects
                .managed()
                .resources()
                .mesh_level(level)
                .unwrap()
                .vertex_buffer;
            owner
                .backend
                .upload_graphics(
                    GraphicsBuffer::Vertex(d.graphics.unwrap()),
                    d.base as usize * 32,
                    &[slot as u8 + 7; 96],
                )
                .unwrap();
            owner
                .objects
                .install_mesh_level_bbox(level, [[-1.; 3], [1.; 3]])
                .unwrap();
            owner
                .objects
                .set_lod(source, Some(level), slot, &mut owner.backend)
                .unwrap();
            owner
                .objects
                .release_mesh_level(level, &mut owner.backend)
                .unwrap();
        }
        owner
            .objects
            .unlock_all_indices(&mut owner.backend)
            .unwrap();
        owner
            .objects
            .unlock_all_vertices(&mut owner.backend)
            .unwrap();
        let clone = owner.deform_create_empty_mesh().unwrap();
        assert_eq!(owner.lookup_mesh(source.slot()).unwrap(), source);
        assert_eq!(owner.deform_duplicate_mesh(clone, Some(source)).unwrap(), 0);
        for slot in 0..3 {
            let a = owner.deform_mesh_lod(source, 0, slot).unwrap().unwrap();
            let b = owner.deform_mesh_lod(clone, 0, slot).unwrap().unwrap();
            assert_ne!(a, b);
            let ga = owner.deform_lod_face_group(a, 0).unwrap().unwrap();
            let gb = owner.deform_lod_face_group(b, 0).unwrap().unwrap();
            assert_ne!(ga, gb);
            assert_eq!(
                owner.deform_face_group_material(gb).unwrap(),
                Some(material)
            );
            let d = owner
                .objects
                .managed()
                .resources()
                .mesh_level(b)
                .unwrap()
                .vertex_buffer;
            assert_eq!(
                owner
                    .deform_cpu_bytes(
                        GraphicsBuffer::Vertex(d.graphics.unwrap()),
                        d.base as usize * 32,
                        96
                    )
                    .unwrap(),
                vec![slot as u8 + 7; 96]
            );
            let d = owner
                .objects
                .managed()
                .resources()
                .face_group(gb)
                .unwrap()
                .index_buffer;
            assert_eq!(
                owner
                    .deform_cpu_bytes(
                        GraphicsBuffer::Index(d.graphics.unwrap()),
                        d.base as usize * 2,
                        6
                    )
                    .unwrap(),
                [0, 0, 1, 0, 2, 0]
            );
        }
        owner.deform_remove_mesh_lod(clone, 0, 2).unwrap();
        assert_eq!(owner.deform_mesh_lod_count(source, 0).unwrap(), 3);
        assert_eq!(owner.deform_mesh_lod_count(clone, 0).unwrap(), 2);
        let copied_material = owner.deform_create_material().unwrap();
        assert_eq!(
            owner
                .deform_set_material_copy(copied_material, Some(material))
                .unwrap(),
            0
        );
        assert_eq!(owner.backend.texture_references(texture), Some(2));
        assert_eq!(
            owner
                .objects
                .managed()
                .resources()
                .material(copied_material)
                .unwrap()
                .flags,
            owner
                .objects
                .managed()
                .resources()
                .material(material)
                .unwrap()
                .flags
        );
        assert_eq!(
            owner
                .deform_set_material_copy(copied_material, None)
                .unwrap(),
            0x80000007u32 as i32
        );
        let object = owner.objects.create_object_resources(Some(source)).unwrap();
        owner.set_object_mesh(object, Some(clone), false).unwrap();
        assert_eq!(owner.object_mesh(object).unwrap(), Some(clone));
        owner.deform_release_mesh(source).unwrap();
        owner.deform_release_mesh(clone).unwrap();
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        assert!(owner.lookup_mesh(source.slot()).is_err());
        assert!(owner.lookup_mesh(clone.slot()).is_err());
        owner.deform_release_material(material).unwrap();
        assert_eq!(owner.backend.live_textures(), 1);
        owner.deform_release_material(copied_material).unwrap();
        assert_eq!(owner.backend.live_textures(), 0);
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.live_graphics(), 0);
    }
}
