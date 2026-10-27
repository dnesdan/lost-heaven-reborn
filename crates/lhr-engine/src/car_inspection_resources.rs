//! Read-only, bounded selected-level views of the live CPU resource owner.
//! inspection_mesh is the explicit legacy LOD0 wrapper; view-aware callers use
//! inspection_mesh_at_lod after ordinary source selection. This is not a second
//! resource owner or a claim of original renderer parity.
use super::*;
use crate::i3d_object::{LodState, ObjectCleanupPhase};
use crate::visual_resources::I3dMaterial;

pub struct CpuInspectionGroup<'a> {
    pub index_bytes: &'a [u8],
    pub material: Option<&'a I3dMaterial>,
    pub material_key: Option<crate::visual_resources::MaterialKey>,
    pub texture: Option<&'a lhr_assets::Texture>,
    pub texture_alpha: Option<&'a lhr_assets::Texture>,
    pub texture_flags: u32,
    pub inspection_rgba8_color_key: bool,
    pub texture_identity: Option<TextureKey>,
}
pub struct CpuInspectionMesh<'a> {
    /// Borrowed validated source/private manager bytes. The selected FVF
    /// determines stride and UV/diffuse offsets; callers never reinterpret a
    /// source normal as constructed diffuse or use source indices for lit IB.
    pub vertex_bytes: &'a [u8],
    pub vertex_stride: usize,
    pub color_byte_offset: Option<usize>,
    pub lightmap_uv_byte_offset: Option<usize>,
    pub vertex_count: u32,
    pub visual_type: crate::i3d_object::ObjectVisualType,
    pub uv_byte_offset: usize,
    pub uv_offset: [f32; 2],
    pub groups: Vec<CpuInspectionGroup<'a>>,
}
impl LoadedVisualResources {
    pub(crate) fn set_material_driver_time(&mut self, time: i32) -> Result<()> {
        self.objects.set_material_driver_time(time)?;
        Ok(())
    }
    pub(crate) fn access_material_texture(
        &mut self,
        key: crate::visual_resources::MaterialKey,
        random: &mut crate::frame_light::LightCrtRandom,
    ) -> Result<Option<TextureKey>> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        Ok(self.objects.get_material_texture(key, random)?)
    }
    pub(super) fn inspection_graphics(
        &self,
        graphics: GraphicsBuffer,
        offset: usize,
        bytes: usize,
        expected_fvf: Option<u32>,
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
        if matches!(graphics, GraphicsBuffer::Vertex(_)) && buffer.fvf != expected_fvf {
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
        self.inspection_mesh_at_lod_inner(object, index, maximum_groups, None)
    }
    /// Geometry preparation never accesses animated texture channels. The
    /// canonical draw adapter resolves only the reached group after its lazy
    /// GetTexture call, preserving access order and random consumption.
    pub(crate) fn inspection_mesh_at_lod_for_group(
        &self,
        object: ObjectResourceKey,
        index: u32,
        maximum_groups: usize,
        texture_group: usize,
    ) -> Result<Option<CpuInspectionMesh<'_>>> {
        self.inspection_mesh_at_lod_inner(object, index, maximum_groups, Some(texture_group))
    }
    fn inspection_mesh_at_lod_inner(
        &self,
        object: ObjectResourceKey,
        index: u32,
        maximum_groups: usize,
        texture_group: Option<usize>,
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
            .find(|b| {
                b.level == level_key
                    && b.state == crate::managed_visual_resources::BindingState::Installed
            })
            .map(|b| b.owner);
        let mut vertex_count = level.num_vertices;
        let mut vertex_stride = 32;
        let mut expected = (0x112, 6);
        let mut uv_byte_offset = 24;
        let mut uv_offset = [0.; 2];
        let mut color_byte_offset = None;
        let mut lightmap_uv_byte_offset = None;
        let mut lit_indices = None;
        let visual_type = object.visual_type();
        let private_lod = if let Some(lit) = object.lit_object() {
            let lighting = lit
                .levels
                .get(lit.active_level)
                .ok_or(LoadedVisualError::InvalidGeometry)?;
            if lighting.constructed {
                if mesh.u_ofs != 0. || mesh.v_ofs != 0. {
                    return Err(LoadedVisualError::Backend(
                        "constructed lightmap animated UV cache unavailable",
                    ));
                }
                let key = lighting.lods[index].ok_or(LoadedVisualError::InvalidGeometry)?;
                if self.objects.lit_lod_storage(key)?.phase
                    != crate::i3d_object::LitStoragePhase::Ready
                {
                    return Err(LoadedVisualError::Backend(
                        "selected saved lighting buffers are not prepared",
                    ));
                }
                let payload = lighting.saved_lods[index]
                    .as_ref()
                    .ok_or(LoadedVisualError::InvalidGeometry)?;
                color_byte_offset = Some(12);
                match payload {
                    crate::i3d_object::saved_lightmaps::SavedLitLod::VertexColors {
                        colors,
                        ..
                    } => {
                        vertex_count = u32::try_from(colors.len())
                            .map_err(|_| LoadedVisualError::InvalidGeometry)?;
                        vertex_stride = 24;
                        expected = (0x142, 0);
                        uv_byte_offset = 16;
                    }
                    crate::i3d_object::saved_lightmaps::SavedLitLod::Lightmap(lod) => {
                        vertex_count = u32::try_from(lod.vertices.len())
                            .map_err(|_| LoadedVisualError::InvalidGeometry)?;
                        expected = (0x242, 1);
                        lightmap_uv_byte_offset = Some(16);
                        let count = u32::try_from(lod.indices.len())
                            .map_err(|_| LoadedVisualError::InvalidGeometry)?;
                        lit_indices =
                            Some((self.objects.saved_lit_index_descriptor(key, count)?, count));
                    }
                }
                Some(key)
            } else {
                expected = (0x242, 1);
                uv_byte_offset = 16;
                lighting.lods[index]
            }
        } else if visual_type == crate::i3d_object::ObjectVisualType::Morph {
            // MRPH lighting/UV/group reads use the shared canonical mesh. Its
            // distinct render-position owner is borrowed separately for draws;
            // substituting that buffer here would change source normal reads.
            None
        } else {
            object.lods[index]
        };
        if let Some(key) = private_lod {
            let lod = self.objects.lod(key)?;
            if lod.state != LodState::Constructed {
                return Err(LoadedVisualError::Backend("inspection LOD was destroyed"));
            }
            if lod.vertex_buffer.buffer.is_some() || lod.vertex_buffer.graphics.is_some() {
                vertices = lod.vertex_buffer;
                vertex_owner = self
                    .objects
                    .lod_bindings()
                    .iter()
                    .find(|b| {
                        b.lod == key
                            && b.state == crate::managed_visual_resources::BindingState::Installed
                    })
                    .map(|b| b.owner);
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
                vertex_count,
            )
            .map_err(|_| {
                LoadedVisualError::Backend("inspection vertex descriptor/manager mismatch")
            })?;
        if configuration.fvf != expected.0
            || configuration.stride != Some(vertex_stride as u32)
            || configuration.format != expected.1
        {
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
            vertices.base as usize * vertex_stride,
            (vertex_count as usize)
                .checked_mul(vertex_stride)
                .ok_or(LoadedVisualError::InvalidGeometry)?,
            Some(expected.0),
        )?;
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(level.num_fgroups as usize)
            .map_err(|_| LoadedVisualError::Backend("inspection allocation"))?;
        let mut next = level.fgroups;
        let mut lit_first = 0u32;
        for group_index in 0..level.num_fgroups {
            let key = next.ok_or(LoadedVisualError::InvalidGeometry)?;
            let group = resources
                .face_group(key)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            let material = group
                .material
                .map(|key| {
                    resources
                        .material(key)
                        .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))
                })
                .transpose()?;
            let index_count = group
                .num_faces
                .checked_mul(3)
                .ok_or(LoadedVisualError::InvalidGeometry)?;
            let (descriptor, first) = if let Some((descriptor, count)) = lit_indices {
                let first = lit_first;
                lit_first = lit_first
                    .checked_add(index_count)
                    .ok_or(LoadedVisualError::InvalidGeometry)?;
                if lit_first > count {
                    return Err(LoadedVisualError::InvalidGeometry);
                }
                (descriptor, first)
            } else {
                (group.index_buffer, 0)
            };
            let index_bytes = self.inspection_graphics(
                GraphicsBuffer::Index(
                    descriptor
                        .graphics
                        .ok_or(LoadedVisualError::InvalidGeometry)?,
                ),
                (descriptor.base as usize + first as usize) * 2,
                (index_count as usize)
                    .checked_mul(2)
                    .ok_or(LoadedVisualError::InvalidGeometry)?,
                None,
            )?;
            let texture_key = if texture_group.is_none_or(|index| index == group_index as usize) {
                material.and_then(|material| material.texture)
            } else {
                None
            };
            let texture = texture_key.map(|key| self.cpu_texture(key)).transpose()?;
            let owned_texture = texture_key.and_then(|key| self.backend.textures.get(&key.0));
            groups.push(CpuInspectionGroup {
                index_bytes,
                material,
                material_key: group.material,
                texture,
                texture_alpha: owned_texture.and_then(|texture| texture.alpha.as_ref()),
                texture_flags: owned_texture.map_or(0, |texture| texture.flags),
                inspection_rgba8_color_key: owned_texture
                    .is_some_and(|texture| texture.inspection_rgba8_color_key),
                texture_identity: material.and_then(|material| material.texture),
            });
            next = group.next;
        }
        if next.is_some() || lit_indices.is_some_and(|(_, count)| lit_first != count) {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        Ok(Some(CpuInspectionMesh {
            vertex_bytes,
            vertex_count,
            vertex_stride,
            color_byte_offset,
            lightmap_uv_byte_offset,
            visual_type,
            uv_byte_offset,
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
    #[test]
    fn lit_private_buffer_requires_actual_lvertex2_stride_and_format() {
        for (stride, format, accepted) in [(64, 1, false), (32, 6, false), (32, 1, true)] {
            let mut owner = LoadedVisualResources::new().unwrap();
            let mesh = owner.objects.create_mesh_object().unwrap();
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
                        exclusive: true,
                    },
                    None,
                    &mut owner.backend,
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
            let object = owner
                .objects
                .create_unconstructed_lit_object_resources(Some(mesh))
                .unwrap();
            owner
                .objects
                .release_mesh_object(mesh, &mut owner.backend)
                .unwrap();
            let lod = owner
                .objects
                .create_unconstructed_lit_lod(object, 0)
                .unwrap();
            owner
                .objects
                .allocate_plain_lod_vertices(
                    lod,
                    VertexRequest {
                        fvf: 0x242,
                        usage: 0x10,
                        stride,
                        count: 3,
                        format,
                        exclusive: true,
                    },
                    &mut owner.backend,
                )
                .unwrap();
            owner
                .objects
                .unlock_all_vertices(&mut owner.backend)
                .unwrap();
            let result = owner.inspection_mesh(object, 0);
            if accepted {
                let view = result.unwrap().unwrap();
                assert_eq!(view.vertex_bytes.len(), 96);
                assert_eq!(view.uv_byte_offset, 16);
            } else {
                assert!(matches!(
                    result,
                    Err(LoadedVisualError::Backend(
                        "inspection UV-only or unknown vertex layout"
                    ))
                ));
            }
            owner.destroy_object(object).unwrap();
            assert!(owner.objects.lod(lod).is_err());
        }
    }
}
