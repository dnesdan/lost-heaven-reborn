//! Canonical mesh-level vertex access through the existing manager and CPU
//! graphics allocation. A projected pointer never owns a copied vertex array.
use super::*;
use crate::visual_resources::{
    CompleteVertexBuffer, FaceGroupKey, MeshObjectKey, VertexBufferPointer,
};

impl LoadedVisualResources {
    pub fn game_unlock_frame_buffers(&mut self) -> Result<()> {
        self.objects.unlock_all_vertices(&mut self.backend)?;
        self.objects.unlock_all_indices(&mut self.backend)?;
        Ok(())
    }
    fn game_level_buffer(
        &self,
        level: MeshLevelKey,
    ) -> Result<(CompleteVertexBuffer, usize, usize)> {
        let managed = self.objects.managed();
        let state = managed
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        let Some(VertexBufferPointer::Managed(token)) = state.vertex_buffer.buffer else {
            return Err(LoadedVisualError::InvalidGeometry);
        };
        let key = managed
            .vertex_tokens()
            .iter()
            .find(|entry| entry.visual == token)
            .map(|entry| entry.manager)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let buffer = managed
            .managers()
            .vertex_buffer(key)
            .map_err(|_| LoadedVisualError::Backend("mesh vertex manager unavailable"))?;
        let stride = buffer
            .vertex
            .as_ref()
            .and_then(|v| v.stride)
            .ok_or(LoadedVisualError::InvalidGeometry)? as usize;
        if stride != 32 {
            return Err(LoadedVisualError::Backend(
                "unsupported canonical vertex stride",
            ));
        }
        Ok((
            state.vertex_buffer,
            stride,
            state.num_vertices as usize * stride,
        ))
    }
    pub(crate) fn game_lock_level_vertices(
        &mut self,
        level: MeshLevelKey,
        flags: u32,
    ) -> Result<bool> {
        self.objects
            .lock_mesh_level_vertices(level, flags, &mut self.backend)
            .map_err(Into::into)
    }
    pub(crate) fn game_unlock_level_vertices(&mut self, level: MeshLevelKey) -> Result<()> {
        self.objects
            .unlock_mesh_level_vertices(level, &mut self.backend)
            .map_err(Into::into)
    }
    pub(crate) fn game_vertex_byte(&self, level: MeshLevelKey, offset: usize) -> Result<u8> {
        let (buffer, stride, length) = self.game_level_buffer(level)?;
        if offset >= length {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let graphics =
            GraphicsBuffer::Vertex(buffer.graphics.ok_or(LoadedVisualError::InvalidGeometry)?);
        let allocation = self
            .backend
            .graphics
            .get(&CpuVisualStorage::graphics_id(graphics))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if allocation.lock.is_none() {
            return Err(LoadedVisualError::Backend(
                "read through unlocked mesh vertices",
            ));
        }
        allocation
            .bytes
            .get(buffer.base as usize * stride + offset)
            .copied()
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
    pub(crate) fn game_write_vertex_byte(
        &mut self,
        level: MeshLevelKey,
        offset: usize,
        value: u8,
    ) -> Result<()> {
        let (buffer, stride, length) = self.game_level_buffer(level)?;
        if offset >= length {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        self.backend.write_graphics(
            GraphicsBuffer::Vertex(buffer.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
            buffer.base as usize * stride + offset,
            &[value],
        )
    }
    pub(crate) fn game_level_word(&self, level: MeshLevelKey, offset: u32) -> Result<u32> {
        let level = self
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        Ok(match offset {
            4 => level.references,
            8 => level.num_vertices,
            0x18 => level.num_fgroups,
            0x20..=0x34 if offset.is_multiple_of(4) => level.bbox[((offset - 0x20) / 12) as usize]
                [(((offset - 0x20) / 4) % 3) as usize]
                .to_bits(),
            0x38 => level.lod_dist.to_bits(),
            0x3c => level.mesh_flags,
            0x40 => level.env_scale.to_bits(),
            _ => return Err(LoadedVisualError::Backend("unsupported mesh level word")),
        })
    }
    pub(crate) fn game_mesh_word(&self, mesh: MeshObjectKey, offset: u32) -> Result<u32> {
        let mesh = self
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?;
        Ok(match offset {
            4 => mesh.references,
            8 => mesh.num_lods,
            0x5c => mesh.u_ofs.to_bits(),
            0x60 => mesh.v_ofs.to_bits(),
            _ => return Err(LoadedVisualError::Backend("unsupported mesh word")),
        })
    }
    pub(crate) fn game_set_group_material(
        &mut self,
        group: FaceGroupKey,
        material: Option<MaterialKey>,
    ) -> Result<()> {
        self.objects
            .set_material(group, material, &mut self.backend)
            .map_err(Into::into)
    }
    /// UpdateBBox deliberately keeps the manager's real vertex lock active.
    pub(crate) fn game_update_level_bbox(&mut self, level: MeshLevelKey) -> Result<()> {
        let locked = self.game_lock_level_vertices(level, 0)?;
        let count = self.game_level_word(level, 8)?;
        let mut bbox = MeshBoundVolume::CONSTRUCTOR.bbox;
        if locked && count == 0 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        if locked && count != 0 {
            for index in 0..count as usize {
                for axis in [0usize, 1, 2] {
                    let offset = index * 32 + axis * 4;
                    let value = f32::from_le_bytes([
                        self.game_vertex_byte(level, offset)?,
                        self.game_vertex_byte(level, offset + 1)?,
                        self.game_vertex_byte(level, offset + 2)?,
                        self.game_vertex_byte(level, offset + 3)?,
                    ]);
                    if index == 0 {
                        bbox[0][axis] = value;
                        bbox[1][axis] = value;
                    } else {
                        if value < bbox[0][axis] {
                            bbox[0][axis] = value
                        }
                        if value > bbox[1][axis] {
                            bbox[1][axis] = value
                        }
                    }
                }
            }
        }
        self.objects
            .install_mesh_level_bbox(level, bbox)
            .map_err(Into::into)
    }
    pub(crate) fn game_update_mesh_bounds(&mut self, mesh: MeshObjectKey) -> Result<()> {
        self.objects
            .update_mesh_bound_volume(mesh)
            .map_err(Into::into)
    }
    pub(crate) fn game_update_object_vertices(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<()> {
        self.objects
            .update_object_vertices(object, index)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_vertex_edits_survive_unlock_and_shared_instance_reads_then_release() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let mesh = owner.objects.create_mesh_object().unwrap();
        let level = owner.objects.create_mesh_level().unwrap();
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
            .allocate_level_vertices(level, 3, request, None, &mut owner.backend)
            .unwrap();
        let descriptor = owner
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .unwrap()
            .vertex_buffer;
        let graphics = GraphicsBuffer::Vertex(descriptor.graphics.unwrap());
        let mut original = [0; 96];
        for (i, position) in [[1f32, 2., 3.], [-1., 0., 0.], [0., 1., 2.]]
            .into_iter()
            .enumerate()
        {
            for (axis, value) in position.into_iter().enumerate() {
                original[i * 32 + axis * 4..i * 32 + axis * 4 + 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        owner
            .backend
            .upload_graphics(graphics, descriptor.base as usize * 32, &original)
            .unwrap();
        owner
            .objects
            .set_lod(mesh, Some(level), 0, &mut owner.backend)
            .unwrap();
        let first = owner.objects.create_object_resources(Some(mesh)).unwrap();
        let second = owner.objects.create_object_resources(Some(mesh)).unwrap();
        owner.game_unlock_frame_buffers().unwrap();
        assert!(owner.game_vertex_byte(level, 0).is_err());
        assert!(owner.game_lock_level_vertices(level, 0).unwrap());
        let lock = owner.backend.get_graphics_mut(graphics).unwrap().lock;
        assert!(owner.game_lock_level_vertices(level, 0).unwrap());
        assert_eq!(owner.backend.get_graphics_mut(graphics).unwrap().lock, lock);
        for (offset, byte) in 8f32.to_le_bytes().into_iter().enumerate() {
            owner.game_write_vertex_byte(level, offset, byte).unwrap();
        }
        owner.game_update_level_bbox(level).unwrap();
        assert_eq!(owner.game_level_word(level, 0x2c).unwrap(), 8f32.to_bits());
        assert!(owner
            .backend
            .get_graphics_mut(graphics)
            .unwrap()
            .lock
            .is_some());
        owner.game_update_mesh_bounds(mesh).unwrap();
        owner.game_unlock_level_vertices(level).unwrap();
        assert!(owner.game_vertex_byte(level, 0).is_err());
        for object in [first, second] {
            let view = owner.inspection_mesh(object, 0).unwrap().unwrap();
            assert_eq!(&view.vertex_bytes[..4], &8f32.to_le_bytes());
        }
        owner
            .objects
            .destroy_object_resources(first, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .destroy_object_resources(second, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .release_mesh_level(level, &mut owner.backend)
            .unwrap();
        owner
            .objects
            .release_mesh_object(mesh, &mut owner.backend)
            .unwrap();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_allocations(), 0);
        assert_eq!(owner.backend.live_graphics(), 0);
        assert!(owner.game_lock_level_vertices(level, 0).is_err());
    }
    #[test]
    fn readonly_lock_upgrade_uses_same_manager_and_global_unlock_clears_cached_lock() {
        let mut owner = LoadedVisualResources::new().unwrap();
        let level = owner.objects.create_mesh_level().unwrap();
        owner
            .objects
            .allocate_level_vertices(
                level,
                1,
                VertexRequest {
                    fvf: 0x112,
                    usage: 0x10,
                    stride: 32,
                    count: 1,
                    format: 6,
                    exclusive: false,
                },
                None,
                &mut owner.backend,
            )
            .unwrap();
        owner.game_unlock_frame_buffers().unwrap();
        assert!(owner.game_lock_level_vertices(level, 1).unwrap());
        let descriptor = owner
            .objects
            .managed()
            .resources()
            .mesh_level(level)
            .unwrap()
            .vertex_buffer;
        let graphics = GraphicsBuffer::Vertex(descriptor.graphics.unwrap());
        let read_only = owner.backend.get_graphics_mut(graphics).unwrap().lock;
        owner.game_lock_level_vertices(level, 0).unwrap();
        assert_ne!(
            owner.backend.get_graphics_mut(graphics).unwrap().lock,
            read_only
        );
        owner.game_unlock_frame_buffers().unwrap();
        assert!(owner
            .backend
            .get_graphics_mut(graphics)
            .unwrap()
            .lock
            .is_none());
        owner
            .objects
            .release_mesh_level(level, &mut owner.backend)
            .unwrap();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(owner.backend.live_allocations(), 0);
    }
}
