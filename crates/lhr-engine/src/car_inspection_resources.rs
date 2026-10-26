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
pub struct CpuLightmapView<'a> {
    pub atlas: crate::lightmap_atlas::LightmapAtlasKey,
    pub texture: &'a lhr_assets::Texture,
    pub uv_byte_offset: usize,
}
pub struct CpuInspectionMesh<'a> {
    /// Actual checked source/private vertex buffer, with its retained stride.
    pub vertex_bytes: &'a [u8],
    pub vertex_stride: usize,
    /// Source ARGB32 words for constructed saved vertex lighting. These are
    /// borrowed from the same selected litLod whose checked buffer is exposed.
    pub saved_vertex_colors: Option<&'a [u32]>,
    /// Expanded geometry's atlas from the same litLod allocation. No light
    /// evaluation is implied: the caller must validate its actual context.
    pub lightmap: Option<CpuLightmapView<'a>>,
    pub vertex_count: u32,
    pub visual_type: crate::i3d_object::ObjectVisualType,
    pub uv_byte_offset: usize,
    pub uv_offset: [f32; 2],
    pub groups: Vec<CpuInspectionGroup<'a>>,
}
impl LoadedVisualResources {
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
        let visual_type = object.visual_type();
        let mut lightmap_data = None;
        let (private_lod, uv_byte_offset, saved_vertex_colors) =
            if let Some(lit) = object.lit_object() {
                let lighting = lit
                    .levels
                    .get(lit.active_level)
                    .ok_or(LoadedVisualError::InvalidGeometry)?;
                if lighting.constructed && lighting.mode == 2 {
                    if mesh.u_ofs != 0. || mesh.v_ofs != 0. {
                        return Err(LoadedVisualError::Backend(
                            "saved lightmap nonzero mesh UV offsets unavailable",
                        ));
                    }
                    let key = lighting.lods[index].ok_or(LoadedVisualError::InvalidGeometry)?;
                    let data = self
                        .objects
                        .lightmap_lod(key)?
                        .ok_or(LoadedVisualError::InvalidGeometry)?;
                    if !data.ready
                        || data.vertex_count == 0
                        || data.vertex_info.len() != data.vertex_count as usize
                        || data.base_uv.len() != data.vertex_count as usize
                        || data.lightmap_uv.len() != data.vertex_count as usize
                        || data.face_rectangles.len() != level.num_fgroups as usize
                    {
                        return Err(LoadedVisualError::InvalidGeometry);
                    }
                    lightmap_data = Some(data);
                    (Some(key), 24, None)
                } else {
                    let colors = if lighting.constructed {
                        if lighting.mode != 1 && lighting.mode != 5 {
                            return Err(LoadedVisualError::Backend(
                                "constructed lighting mode unavailable",
                            ));
                        }
                        let colors = lighting.saved_vertex_colors[index].as_deref().ok_or(
                            LoadedVisualError::Backend("saved vertex colors unavailable"),
                        )?;
                        if colors.len() != level.num_vertices as usize {
                            return Err(LoadedVisualError::InvalidGeometry);
                        }
                        Some(colors)
                    } else {
                        None
                    };
                    (lighting.lods[index], 16, colors)
                }
            } else {
                (object.lods[index], 24, None)
            };
        if let Some(lod_key) = private_lod {
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
        let vertex_count = lightmap_data.map_or(level.num_vertices, |data| data.vertex_count);
        let lightmap = lightmap_data
            .map(|data| -> Result<CpuLightmapView<'_>> {
                let view = self
                    .objects
                    .lightmap_atlases()
                    .allocation(data.allocation.ok_or(LoadedVisualError::InvalidGeometry)?)
                    .map_err(ObjectError::Lightmap)?;
                Ok(CpuLightmapView {
                    atlas: view.atlas,
                    texture: view.texture,
                    uv_byte_offset: 16,
                })
            })
            .transpose()?;
        let lightmap_indices = lightmap_data
            .map(|data| {
                let key = private_lod.ok_or(LoadedVisualError::InvalidGeometry)?;
                let binding = self
                    .objects
                    .lit_index_bindings()
                    .iter()
                    .find(|binding| {
                        binding.lod == key
                            && binding.state
                                == crate::managed_visual_resources::BindingState::Installed
                    })
                    .ok_or(LoadedVisualError::Backend(
                        "saved lightmap index owner unavailable",
                    ))?;
                let usage = self
                    .objects
                    .managed()
                    .index_storage_configuration(binding.owner, data.index_buffer, data.index_count)
                    .map_err(|error| LoadedVisualError::Object(ObjectError::Managed(error)))?;
                if usage != 0x10 {
                    return Err(LoadedVisualError::Backend(
                        "saved lightmap index storage usage differs",
                    ));
                }
                self.inspection_graphics(
                    GraphicsBuffer::Index(
                        data.index_buffer
                            .graphics
                            .ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    data.index_buffer.base as usize * 2,
                    data.index_count as usize * 2,
                    None,
                )
            })
            .transpose()?;
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
        let expected = if saved_vertex_colors.is_some() {
            (0x142, 0, 24)
        } else if visual_type == crate::i3d_object::ObjectVisualType::Lit {
            (0x242, 1, 32)
        } else {
            (0x112, 6, 32)
        };
        if configuration.fvf != expected.0
            || configuration.stride != Some(expected.2)
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
            vertices.base as usize * expected.2 as usize,
            (vertex_count as usize)
                .checked_mul(expected.2 as usize)
                .ok_or(LoadedVisualError::InvalidGeometry)?,
            Some(expected.0),
        )?;
        if let Some(colors) = saved_vertex_colors {
            if vertex_bytes
                .as_chunks::<24>()
                .0
                .iter()
                .zip(colors)
                .any(|(vertex, color)| {
                    u32::from_le_bytes(vertex[12..16].try_into().unwrap()) != *color
                })
            {
                return Err(LoadedVisualError::Backend(
                    "saved vertex color buffer differs from its owner",
                ));
            }
        }
        if let Some(data) = lightmap_data {
            for ((vertex, base), light) in vertex_bytes
                .as_chunks::<32>()
                .0
                .iter()
                .zip(&data.base_uv)
                .zip(&data.lightmap_uv)
            {
                let word =
                    |offset| u32::from_le_bytes(vertex[offset..offset + 4].try_into().unwrap());
                if word(16) != light[0].to_bits()
                    || word(20) != light[1].to_bits()
                    || word(24) != base[0].to_bits()
                    || word(28) != base[1].to_bits()
                {
                    return Err(LoadedVisualError::Backend(
                        "saved lightmap UV buffer differs from owner",
                    ));
                }
            }
        }
        let mut groups = Vec::new();
        groups
            .try_reserve_exact(level.num_fgroups as usize)
            .map_err(|_| LoadedVisualError::Backend("inspection allocation"))?;
        let mut next = level.fgroups;
        let mut first_index_byte = 0usize;
        for _ in 0..level.num_fgroups {
            let key = next.ok_or(LoadedVisualError::InvalidGeometry)?;
            let group = resources
                .face_group(key)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            let material = resources
                .material(group.material.ok_or(LoadedVisualError::InvalidGeometry)?)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
            let group_bytes = (group.num_faces as usize)
                .checked_mul(6)
                .ok_or(LoadedVisualError::InvalidGeometry)?;
            let index_bytes = if let Some(indices) = lightmap_indices {
                let end = first_index_byte
                    .checked_add(group_bytes)
                    .ok_or(LoadedVisualError::InvalidGeometry)?;
                let slice = indices
                    .get(first_index_byte..end)
                    .ok_or(LoadedVisualError::InvalidGeometry)?;
                first_index_byte = end;
                slice
            } else {
                self.inspection_graphics(
                    GraphicsBuffer::Index(
                        group
                            .index_buffer
                            .graphics
                            .ok_or(LoadedVisualError::InvalidGeometry)?,
                    ),
                    group.index_buffer.base as usize * 2,
                    group_bytes,
                    None,
                )?
            };
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
        if lightmap_indices.is_some_and(|indices| first_index_byte != indices.len()) {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        Ok(Some(CpuInspectionMesh {
            vertex_bytes,
            vertex_stride: expected.2 as usize,
            saved_vertex_colors,
            lightmap,
            vertex_count,
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
