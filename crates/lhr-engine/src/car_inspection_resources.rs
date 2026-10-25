//! Read-only, bounded selected-level views of the live CPU resource owner.
//! inspection_mesh is the explicit legacy LOD0 wrapper; view-aware callers use
//! inspection_mesh_at_lod after ordinary source selection. This is not a second
//! resource owner or a claim of original renderer parity.
use super::*;
use crate::i3d_object::{LodState, ObjectCleanupPhase};
use crate::visual_resources::I3dMaterial;

pub struct CpuInspectionGroup<'a> {
    pub index_bytes: &'a [u8],
    pub material: &'a I3dMaterial,
    pub texture: Option<&'a lhr_assets::Texture>,
    pub texture_alpha: Option<&'a lhr_assets::Texture>,
    pub texture_flags: u32,
    pub inspection_rgba8_color_key: bool,
    pub texture_identity: Option<TextureKey>,
}
pub struct CpuInspectionMesh<'a> {
    /// Admitted plain FVF0x112 vertices: XYZ, normal XYZ, UV, 32-byte stride.
    pub vertex_bytes: &'a [u8],
    pub vertex_count: u32,
    pub uv_offset: [f32; 2],
    pub groups: Vec<CpuInspectionGroup<'a>>,
}
impl LoadedVisualResources {
    fn inspection_graphics(
        &self,
        graphics: GraphicsBuffer,
        offset: usize,
        bytes: usize,
    ) -> Result<&[u8]> {
        let buffer = self
            .backend
            .graphics
            .get(&CpuVisualStorage::graphics_id(graphics))
            .ok_or(LoadedVisualError::Backend(
                "inspection graphics unavailable",
            ))?;
        if !matches!(
            (buffer.kind, graphics),
            (BufferKind::Vertex, GraphicsBuffer::Vertex(_))
                | (BufferKind::Index, GraphicsBuffer::Index(_))
        ) || buffer.lock.is_some()
        {
            return Err(LoadedVisualError::Backend(
                "inspection graphics kind or active lock",
            ));
        }
        if matches!(graphics, GraphicsBuffer::Vertex(_)) && buffer.fvf != Some(0x112) {
            return Err(LoadedVisualError::Backend(
                "inspection UV-only or unknown vertex layout",
            ));
        }
        buffer
            .bytes
            .get(
                offset
                    ..offset
                        .checked_add(bytes)
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
            )
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
    /// Resolves live object/mesh/LOD/material identities each call. A plain
    /// object's private LOD buffer overrides the shared mesh vertex buffer.
    /// Borrowed bytes cannot outlive this owner or overlap mutable teardown.
    pub fn inspection_mesh(
        &self,
        object: ObjectResourceKey,
        maximum_groups: usize,
    ) -> Result<Option<CpuInspectionMesh<'_>>> {
        self.inspection_mesh_at_lod(object, 0, maximum_groups)
    }
    /// Exact selected CPU-owned level; no UV-only or unknown layout is read
    /// as a 32-byte XYZ/normal/UV vertex. A cull sentinel is never admitted here.
    pub fn inspection_mesh_at_lod(
        &self,
        object: ObjectResourceKey,
        index: u32,
        maximum_groups: usize,
    ) -> Result<Option<CpuInspectionMesh<'_>>> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        let object = self.objects.object(object)?;
        if object.phase != ObjectCleanupPhase::Alive {
            return Err(LoadedVisualError::Backend(
                "inspection object is being destroyed",
            ));
        }
        let Some(mesh) = object.mesh else {
            return Ok(None);
        };
        let resources = self.objects.managed().resources();
        let mesh = resources
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        if index >= mesh.num_lods {
            if index == 0 && mesh.num_lods == 0 {
                return Ok(None);
            }
            return Err(LoadedVisualError::Backend(
                "inspection LOD index is not a selected level",
            ));
        }
        let index = index as usize;
        let Some(level_key) = mesh.levels[index] else {
            return Ok(None);
        };
        let level = resources
            .mesh_level(level_key)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        if level.num_fgroups as usize > maximum_groups {
            return Err(LoadedVisualError::Backend("inspection draw quota"));
        }
        let mut vertices = level.vertex_buffer;
        let mut vertex_owner = self
            .objects
            .managed()
            .vertex_bindings()
            .iter()
            .find(|binding| {
                binding.level == level_key
                    && binding.state == crate::managed_visual_resources::BindingState::Installed
            })
            .map(|binding| binding.owner);
        let mut uv_offset = [0.; 2];
        if let Some(lod_key) = object.lods[index] {
            let lod = self.objects.lod(lod_key)?;
            if lod.state != LodState::Constructed {
                return Err(LoadedVisualError::Backend("inspection LOD was destroyed"));
            }
            if lod.vertex_buffer.buffer.is_some() || lod.vertex_buffer.graphics.is_some() {
                vertices = lod.vertex_buffer;
                vertex_owner = self
                    .objects
                    .lod_bindings()
                    .iter()
                    .find(|binding| {
                        binding.lod == lod_key
                            && binding.state
                                == crate::managed_visual_resources::BindingState::Installed
                    })
                    .map(|binding| binding.owner);
            }
            uv_offset = [lod.u_ofs, lod.v_ofs];
        }
        let configuration = self
            .objects
            .managed()
            .vertex_storage_configuration(
                vertex_owner.ok_or(LoadedVisualError::Backend(
                    "inspection vertex ownership unavailable",
                ))?,
                vertices,
                level.num_vertices,
            )
            .map_err(|_| {
                LoadedVisualError::Backend("inspection vertex descriptor/manager mismatch")
            })?;
        if configuration.fvf != 0x112 || configuration.stride != Some(32) {
            return Err(LoadedVisualError::Backend(
                "inspection UV-only or unknown vertex layout",
            ));
        }
        let vertex_bytes = self.inspection_graphics(
            GraphicsBuffer::Vertex(
                vertices
                    .graphics
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            vertices.base as usize * 32,
            (level.num_vertices as usize)
                .checked_mul(32)
                .ok_or(LoadedVisualError::InvalidGeometry)?,
        )?;
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(level.num_fgroups as usize)
            .map_err(|_| LoadedVisualError::Backend("inspection allocation"))?;
        let mut next = level.fgroups;
        for _ in 0..level.num_fgroups {
            let key = next.ok_or(LoadedVisualError::InvalidGeometry)?;
            let group = resources
                .face_group(key)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            let material = resources
                .material(group.material.ok_or(LoadedVisualError::InvalidGeometry)?)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            let index_bytes = self.inspection_graphics(
                GraphicsBuffer::Index(
                    group
                        .index_buffer
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                group.index_buffer.base as usize * 2,
                (group.num_faces as usize)
                    .checked_mul(6)
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
            )?;
            let texture = material
                .texture
                .map(|key| self.cpu_texture(key))
                .transpose()?;
            let owned_texture = material
                .texture
                .and_then(|key| self.backend.textures.get(&key.0));
            groups.push(CpuInspectionGroup {
                index_bytes,
                material,
                texture,
                texture_alpha: owned_texture.and_then(|texture| texture.alpha.as_ref()),
                texture_flags: owned_texture.map_or(0, |texture| texture.flags),
                inspection_rgba8_color_key: owned_texture
                    .is_some_and(|texture| texture.inspection_rgba8_color_key),
                texture_identity: material.texture,
            });
            next = group.next;
        }
        if next.is_some() {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        Ok(Some(CpuInspectionMesh {
            vertex_bytes,
            vertex_count: level.num_vertices,
            uv_offset,
            groups,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_reads_private_lod_and_rejects_retired_resource_identity() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let mesh = owner.objects.create_mesh_object().unwrap();
        let level = owner.objects.create_mesh_level().unwrap();
        let material = owner.objects.create_material().unwrap();
        let group = owner.objects.create_face_group().unwrap();
        owner
            .objects
            .allocate_group_indices(group, 1, 0, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .set_material(group, Some(material), &mut owner.backend)
            .unwrap();
        let request = VertexRequest {
            fvf: 0x112,
            usage: 0x10,
            stride: 32,
            count: 3,
            format: 6,
            exclusive: false,
        };
        owner
            .objects
            .allocate_level_vertices(level, 3, request, Some(group), &mut owner.backend)
            .unwrap();
        let shared = owner
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .unwrap()
            .vertex_buffer;
        owner
            .backend
            .upload_graphics(
                GraphicsBuffer::Vertex(shared.graphics.unwrap()),
                shared.base as usize * 32,
                &[0; 96],
            )
            .unwrap();
        let indices = owner
            .objects
            .managed()
            .resources()
            .face_group(group)
            .unwrap()
            .index_buffer;
        owner
            .backend
            .upload_graphics(
                GraphicsBuffer::Index(indices.graphics.unwrap()),
                indices.base as usize * 2,
                &[0, 0, 1, 0, 2, 0],
            )
            .unwrap();
        owner
            .objects
            .set_lod(mesh, Some(level), 0, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .release_mesh_level(level, &mut owner.backend)
            .unwrap();
        let object = owner.objects.create_object_resources(Some(mesh)).unwrap();
        let private = owner.objects.create_plain_lod().unwrap();
        owner
            .objects
            .allocate_plain_lod_vertices(private, request, &mut owner.backend)
            .unwrap();
        owner.objects.adopt_plain_lod(object, 0, private).unwrap();
        let buffer = owner.objects.lod(private).unwrap().vertex_buffer;
        let mut private_bytes = [0u8; 96];
        private_bytes[..4].copy_from_slice(&42f32.to_bits().to_le_bytes());
        owner
            .backend
            .upload_graphics(
                GraphicsBuffer::Vertex(buffer.graphics.unwrap()),
                buffer.base as usize * 32,
                &private_bytes,
            )
            .unwrap();
        owner
            .objects
            .unlock_all_indices(&mut owner.backend)
            .unwrap();
        owner
            .objects
            .unlock_all_vertices(&mut owner.backend)
            .unwrap();
        assert!(owner.inspection_mesh(object, 0).is_err());
        let view = owner.inspection_mesh(object, 1).unwrap().unwrap();
        assert_eq!(&view.vertex_bytes[..4], &42f32.to_bits().to_le_bytes());
        assert_eq!(view.groups[0].index_bytes, &[0, 0, 1, 0, 2, 0]);
        assert_eq!(view.vertex_count, 3);
        owner
            .objects
            .set_lod(mesh, Some(level), 1, &mut owner.backend)
            .unwrap();
        let uv_only = owner.objects.create_plain_lod().unwrap();
        owner
            .objects
            .allocate_plain_lod_vertices(
                uv_only,
                VertexRequest {
                    fvf: 0x100,
                    stride: 8,
                    ..request
                },
                &mut owner.backend,
            )
            .unwrap();
        owner.objects.adopt_plain_lod(object, 1, uv_only).unwrap();
        owner
            .objects
            .unlock_all_vertices(&mut owner.backend)
            .unwrap();
        assert!(matches!(
            owner.inspection_mesh_at_lod(object, 1, 1),
            Err(LoadedVisualError::Backend(
                "inspection UV-only or unknown vertex layout"
            ))
        ));
        // A later request64 may reuse the original actual32 buffer.
        // Prove that read admission follows retained layout, not requested64.
        owner
            .objects
            .set_lod(mesh, Some(level), 2, &mut owner.backend)
            .unwrap();
        let reused = owner.objects.create_plain_lod().unwrap();
        owner
            .objects
            .allocate_plain_lod_vertices(
                reused,
                VertexRequest {
                    stride: 64,
                    ..request
                },
                &mut owner.backend,
            )
            .unwrap();
        owner.objects.adopt_plain_lod(object, 2, reused).unwrap();
        let descriptor = owner.objects.lod(reused).unwrap().vertex_buffer;
        let binding = owner
            .objects
            .lod_bindings()
            .iter()
            .find(|binding| binding.lod == reused)
            .unwrap();
        assert_eq!(
            owner
                .objects
                .managed()
                .vertex_storage_configuration(binding.owner, descriptor, 3)
                .unwrap()
                .stride,
            Some(32)
        );
        owner
            .backend
            .upload_graphics(
                GraphicsBuffer::Vertex(descriptor.graphics.unwrap()),
                descriptor.base as usize * 32,
                &private_bytes,
            )
            .unwrap();
        owner
            .objects
            .unlock_all_vertices(&mut owner.backend)
            .unwrap();
        let view = owner.inspection_mesh_at_lod(object, 2, 1).unwrap().unwrap();
        assert_eq!(&view.vertex_bytes[..4], &42f32.to_le_bytes());
        // Same FVF does not imply the byte stride. A fresh exclusive64
        // allocation must be rejected using retained manager configuration.
        owner
            .objects
            .set_lod(mesh, Some(level), 3, &mut owner.backend)
            .unwrap();
        let wide = owner.objects.create_plain_lod().unwrap();
        owner
            .objects
            .allocate_plain_lod_vertices(
                wide,
                VertexRequest {
                    stride: 64,
                    exclusive: true,
                    ..request
                },
                &mut owner.backend,
            )
            .unwrap();
        owner.objects.adopt_plain_lod(object, 3, wide).unwrap();
        owner
            .objects
            .unlock_all_vertices(&mut owner.backend)
            .unwrap();
        assert!(matches!(
            owner.inspection_mesh_at_lod(object, 3, 1),
            Err(LoadedVisualError::Backend(
                "inspection UV-only or unknown vertex layout"
            ))
        ));
        owner
            .objects
            .destroy_object_resources(object, &mut owner.backend)
            .unwrap();
        assert!(owner.inspection_mesh(object, 1).is_err());
        owner
            .objects
            .release_mesh_object(mesh, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .release_material(material, &mut owner.backend)
            .unwrap();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_allocations(), 0);
    }
}
