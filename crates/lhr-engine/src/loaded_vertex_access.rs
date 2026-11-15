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
    /// One `fstp dword`/`mov dword` store into a locked vertex: `I3D_vertex`
    /// word `word` (0..2 position, 3..5 normal), through the allocation's
    /// owner, which refuses an unlocked or borrowed allocation and advances
    /// its revision.
    pub(crate) fn game_write_vertex_word(
        &mut self,
        window: LevelVertexWindow,
        index: usize,
        word: usize,
        value: f32,
    ) -> Result<()> {
        if word >= window.stride / 4 {
            return Err(LoadedVisualError::InvalidGeometry);
        }
        let at = index
            .checked_mul(window.stride)
            .and_then(|n| n.checked_add(window.base))
            .and_then(|n| n.checked_add(4 * word))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        self.backend
            .write_graphics(window.graphics, at, &value.to_le_bytes())
    }
    #[cfg(test)]
    pub(crate) fn game_window_graphics(window: LevelVertexWindow) -> GraphicsBuffer {
        window.graphics
    }
    #[cfg(test)]
    pub(crate) fn game_lock_group_indices_for_test(&mut self, group: FaceGroupKey) -> Result<bool> {
        self.objects
            .lock_face_group_indices(group, &mut self.backend)
            .map_err(Into::into)
    }
    /// A copy of `size` bytes at `offset` of a locked allocation, read the
    /// way a `memcpy` from the native lock pointer reads them.
    fn game_locked_bytes(
        &self,
        graphics: GraphicsBuffer,
        offset: usize,
        size: usize,
    ) -> Result<Vec<u8>> {
        let allocation = self
            .backend
            .graphics
            .get(&CpuVisualStorage::graphics_id(graphics))
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        if allocation.lock.is_none() {
            return Err(LoadedVisualError::Backend("copy from unlocked graphics"));
        }
        let end = offset
            .checked_add(size)
            .ok_or(LoadedVisualError::InvalidGeometry)?;
        let bytes = allocation
            .bytes
            .try_borrow()
            .map_err(|_| LoadedVisualError::Backend("CPU graphics bytes being written"))?;
        bytes
            .get(offset..end)
            .map(<[u8]>::to_vec)
            .ok_or(LoadedVisualError::InvalidGeometry)
    }
    /// `I3D_face_group::Duplicate`, LS3DF 0x30800 (mafia-re
    /// I3d_mesh.cpp:86-93): `CreateFaces(n)` (0x30610: allocate, then
    /// `GetFaces` 0x30690 locks the fresh block), the source's `GetFaces`
    /// lock, the byte copy, `num_faces` and `SetMaterial`. Both locks stay.
    /// An empty source group (`CreateFaces(0)`, an `m_Alloc` of zero
    /// indices) has no owner here and stops.
    fn game_duplicate_face_group(
        &mut self,
        destination: FaceGroupKey,
        source: FaceGroupKey,
    ) -> Result<()> {
        let object = |e: crate::visual_resources::VisualResourceError| {
            LoadedVisualError::Object(ObjectError::Managed(e.into()))
        };
        let (faces, indices, material) = {
            let g = self
                .objects
                .managed()
                .resources()
                .face_group(source)
                .map_err(object)?;
            (g.num_faces, g.index_buffer, g.material)
        };
        if faces == 0 {
            return Err(LoadedVisualError::Backend(
                "face group Duplicate of an empty group: CreateFaces(0) has no owner",
            ));
        }
        if !self
            .objects
            .allocate_group_indices(destination, faces, 0, &mut self.backend)?
        {
            return Err(LoadedVisualError::Backend(
                "face group Duplicate index allocation rejected",
            ));
        }
        let output = self
            .objects
            .managed()
            .resources()
            .face_group(destination)
            .map_err(object)?
            .index_buffer;
        if !self
            .objects
            .lock_face_group_indices(destination, &mut self.backend)?
            || !self
                .objects
                .lock_face_group_indices(source, &mut self.backend)?
        {
            return Err(LoadedVisualError::Backend(
                "face group Duplicate without index storage",
            ));
        }
        let data = self.game_locked_bytes(
            GraphicsBuffer::Index(indices.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
            indices.base as usize * 2,
            faces as usize * 6,
        )?;
        self.backend.write_graphics(
            GraphicsBuffer::Index(output.graphics.ok_or(LoadedVisualError::InvalidGeometry)?),
            output.base as usize * 2,
            &data,
        )?;
        self.objects
            .set_material(destination, material, &mut self.backend)?;
        Ok(())
    }
    /// `I3D_mesh_level::Duplicate`, LS3DF 0x31000 (mafia-re
    /// I3d_mesh.cpp:264-300), into a fresh level: the scalars;
    /// `SetVertexMesh(source->LockVertices(0), num)` (0x30b40: allocate,
    /// `Lock` the fresh block with flags 0, copy, `UpdateBBox` 0x30a50 with
    /// its own `LockVertices(0)`); each face group duplicated and appended
    /// last; then the source's box. Every lock taken stays, as natively.
    fn game_duplicate_mesh_level(
        &mut self,
        destination: MeshLevelKey,
        source: MeshLevelKey,
    ) -> Result<()> {
        let object = |e: crate::visual_resources::VisualResourceError| {
            LoadedVisualError::Object(ObjectError::Managed(e.into()))
        };
        self.objects
            .copy_mesh_level_scalar_fields(destination, source)?;
        let source_locked = self.game_lock_level_vertices(source, 0)?;
        let vertices = self.game_level_word(source, 8)?;
        if source_locked {
            if vertices == 0 {
                return Err(LoadedVisualError::Backend(
                    "SetVertexMesh of a locked level with no vertices has no owner",
                ));
            }
            let window = self.game_level_vertex_window(source)?;
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
                    "SetVertexMesh vertex allocation rejected",
                ));
            }
            if !self.game_lock_level_vertices(destination, 0)? {
                return Err(LoadedVisualError::Backend(
                    "SetVertexMesh without vertex storage",
                ));
            }
            let data = self.game_locked_bytes(
                window.graphics,
                window.base,
                vertices as usize * window.stride,
            )?;
            let output = self.game_level_vertex_window(destination)?;
            self.backend
                .write_graphics(output.graphics, output.base, &data)?;
            self.game_update_level_bbox(destination)?;
        }
        let (bbox, mut next) = {
            let level = self
                .objects
                .managed()
                .resources()
                .mesh_level(source)
                .map_err(object)?;
            (level.bbox, level.fgroups)
        };
        while let Some(group) = next {
            let following = self
                .objects
                .managed()
                .resources()
                .face_group(group)
                .map_err(object)?
                .next;
            let fresh = self.objects.create_face_group()?;
            self.game_duplicate_face_group(fresh, group)?;
            self.objects.append_duplicate_group(destination, fresh)?;
            next = following;
        }
        self.objects.install_mesh_level_bbox(destination, bbox)?;
        Ok(())
    }
    /// `I3D_mesh_object::Duplicate`, LS3DF 0x315e0 (mafia-re
    /// I3d_mesh.cpp:471-505), from `source` into the fresh `destination`:
    /// per source slot `CreateMeshLevel`, level Duplicate, `SetLOD`, then the
    /// level's `Release`. Unlike the vehicle-deform copy it leaves every
    /// buffer lock as the native level and face-group Duplicates leave them
    /// and never unlocks other buffers.
    fn game_duplicate_mesh(
        &mut self,
        destination: MeshObjectKey,
        source: MeshObjectKey,
    ) -> Result<()> {
        if self.incomplete {
            return Err(LoadedVisualError::IncompleteAdmission);
        }
        self.objects
            .clear_duplicate_mesh(destination, &mut self.backend)?;
        for index in 0..10 {
            let level = self
                .objects
                .managed()
                .resources()
                .mesh_object(source)
                .map_err(|e| LoadedVisualError::Object(ObjectError::Managed(e.into())))?
                .levels[index];
            if let Some(level) = level {
                let copy = self.objects.create_mesh_level()?;
                self.game_duplicate_mesh_level(copy, level)?;
                self.objects
                    .set_lod(destination, Some(copy), index as u32, &mut self.backend)?;
                self.objects.release_mesh_level(copy, &mut self.backend)?;
            }
        }
        Ok(())
    }
    /// `I3D_object::DetachMesh` (LS3DF 0x36a70, mafia-re
    /// I3d_object.cpp:89-108) over this CPU owner, in the order of
    /// `ObjectResources::detach_object_mesh_to_unique`: a mesh with fewer
    /// than two references is kept; otherwise `CreateMesh`, mesh `Duplicate`
    /// (0x315e0, [`Self::game_duplicate_mesh`]), then the old mesh's
    /// release and the store. The copy takes the native locks and releases
    /// none; locks on other buffers are untouched. A failed Duplicate
    /// releases the fresh mesh and keeps the old one. True when the mesh was
    /// replaced.
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
        if let Err(error) = self.game_duplicate_mesh(fresh, mesh) {
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
