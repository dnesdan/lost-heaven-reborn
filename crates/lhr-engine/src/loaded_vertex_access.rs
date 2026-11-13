//! Canonical mesh-level vertex access through the existing manager and CPU
//! graphics allocation. A projected pointer never owns a copied vertex array.
use super::*;
use crate::visual_resources::{
    CompleteVertexBuffer, FaceGroupKey, MeshObjectKey, VertexBufferPointer,
};

/// A resolved mesh level vertex window (see `game_level_vertex_window`).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LevelVertexWindow {
    graphics: GraphicsBuffer,
    base: usize,
    stride: usize,
}

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
    /// The live window of a mesh level's vertices: its CPU graphics
    /// allocation, first byte and stride (32, `I3D_vertex`). Resolved once
    /// per caller pass; reads and writes still go through the allocation.
    pub(crate) fn game_level_vertex_window(
        &self,
        level: MeshLevelKey,
    ) -> Result<LevelVertexWindow> {
        let (buffer, stride, _) = self.game_level_buffer(level)?;
        Ok(LevelVertexWindow {
            graphics: GraphicsBuffer::Vertex(
                buffer.graphics.ok_or(LoadedVisualError::InvalidGeometry)?,
            ),
            base: buffer.base as usize * stride,
            stride,
        })
    }
    /// The pointer `I3D_mesh_level::LockVertices` returns, as a live view of
    /// the locked allocation (taken after the lock). The view shares the
    /// allocation's bytes, so later in-place writes are what it reads.
    pub(crate) fn game_level_vertex_view(
        &self,
        level: MeshLevelKey,
    ) -> Result<crate::game_collision::LodVertexBuffer> {
        let window = self.game_level_vertex_window(level)?;
        let count = self.game_level_word(level, 8)? as usize;
        let allocation = self
            .backend
            .graphics
            .get(&CpuVisualStorage::graphics_id(window.graphics))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if allocation.lock.is_none() {
            return Err(LoadedVisualError::Backend("view of unlocked mesh vertices"));
        }
        crate::game_collision::LodVertexBuffer::live(
            allocation.bytes.clone(),
            window.base,
            window.stride,
            count,
        )
        .ok_or(LoadedVisualError::InvalidGeometry)
    }
    /// One locked vertex's position and normal (`I3D_vertex` +0x0, +0xc).
    pub(crate) fn game_vertex_position_normal(
        &self,
        window: LevelVertexWindow,
        index: usize,
    ) -> Result<[[f32; 3]; 2]> {
        let allocation = self
            .backend
            .graphics
            .get(&CpuVisualStorage::graphics_id(window.graphics))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if allocation.lock.is_none() {
            return Err(LoadedVisualError::Backend(
                "read through unlocked mesh vertices",
            ));
        }
        let at = index
            .checked_mul(window.stride)
            .and_then(|n| n.checked_add(window.base))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let bytes = allocation.bytes.borrow();
        let word = |i: usize| -> Result<f32> {
            bytes
                .get(at + 4 * i..at + 4 * i + 4)
                .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
                .ok_or(LoadedVisualError::InvalidGeometry)
        };
        Ok([
            [word(0)?, word(1)?, word(2)?],
            [word(3)?, word(4)?, word(5)?],
        ])
    }
    /// In-place store of a locked vertex's position and normal.
    pub(crate) fn game_write_vertex_position_normal(
        &mut self,
        window: LevelVertexWindow,
        index: usize,
        values: [[f32; 3]; 2],
    ) -> Result<()> {
        let at = index
            .checked_mul(window.stride)
            .and_then(|n| n.checked_add(window.base))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let mut data = [0u8; 24];
        for (slot, value) in data
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(values.into_iter().flatten())
        {
            *slot = value.to_le_bytes();
        }
        self.backend.write_graphics(window.graphics, at, &data)
    }
    /// `I3D_object::DetachMesh` (LS3DF 0x36a70, mafia-re
    /// I3d_object.cpp:89-108) over this CPU owner, in the order of
    /// `ObjectResources::detach_object_mesh_to_unique`: a mesh with fewer
    /// than two references is kept; otherwise `CreateMesh`, `Duplicate`
    /// (0x315e0) run by this owner's `deform_duplicate_mesh`, which copies
    /// the vertex and index bytes into fresh storage as the native level
    /// Duplicate's `SetVertexMesh(LockVertices(0))` does, then the old mesh's
    /// release and the store. A failed Duplicate releases the fresh mesh and
    /// keeps the old one. True when the mesh was replaced.
    pub(crate) fn game_detach_object_mesh(&mut self, object: ObjectResourceKey) -> Result<bool> {
        let Some(mesh) = self.objects.object(object)?.mesh else {
            return Ok(false);
        };
        let references = self
            .objects
            .managed()
            .resources()
            .mesh_object(mesh)
            .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
            .references;
        if references < 2 {
            return Ok(false);
        }
        let fresh = self.objects.create_mesh_object()?;
        if let Err(error) = self.deform_duplicate_mesh(fresh, Some(mesh)) {
            let _ = self.objects.release_mesh_object(fresh, &mut self.backend);
            return Err(error);
        }
        self.objects
            .replace_detached_object_mesh(object, fresh, &mut self.backend)?;
        Ok(true)
    }
    /// The object's virtual `UpdateVertices(index)` (vtable +0x78): the
    /// plain object's (LS3DF 0x36a50) or the lit object's (0x21790), as the
    /// two vtables (0x1009c348, 0x1009bd70) hold them.
    pub(crate) fn game_update_object_lod_vertices(
        &mut self,
        object: ObjectResourceKey,
        index: u32,
    ) -> Result<()> {
        use crate::i3d_object::ObjectVisualType;
        match self.objects.object(object)?.visual_type() {
            ObjectVisualType::Plain => self.objects.update_object_vertices(object, index)?,
            ObjectVisualType::LitUnconstructed => {
                self.objects
                    .update_lit_lod_vertices(object, index as usize)?;
            }
            _ => return Err(ObjectError::UnsupportedVisualSubtype.into()),
        }
        Ok(())
    }
    /// The world-space record word of the object's own `SetSpace` arm.
    pub(crate) fn game_mark_object_world_space(&mut self, object: ObjectResourceKey) -> Result<()> {
        self.objects
            .set_object_space(object, true)
            .map_err(|_| LoadedVisualError::Backend("object world-space record"))?;
        Ok(())
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
            .borrow()
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
            // The level's window is resolved once; each position is still
            // read from the locked allocation in vertex, then axis order.
            let window = self.game_level_vertex_window(level)?;
            for index in 0..count as usize {
                let [position, _] = self.game_vertex_position_normal(window, index)?;
                for axis in [0usize, 1, 2] {
                    let value = position[axis];
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
