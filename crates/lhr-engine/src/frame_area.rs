//! Canonical I3D_area geometry owner. PC secondary source RVAs51a0/5880/55d0.
//! PRIVATE DRAFT: primary validation and arena/ring/sound integration pending.
//! Unwritten native array bytes remain unavailable, including after Build.
use crate::frame_transform::{FrameKey, I3dFrameGraph};
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::scene_ownership::SceneKey;
use crate::script_scheduler::FrameReferenceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaKey {
    pub(crate) frame: FrameKey,
    pub(crate) reference: FrameReferenceId,
}
impl AreaKey {
    pub fn frame(self) -> FrameKey {
        self.frame
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AreaError {
    ForeignArena,
    UnknownArea(FrameKey),
    AllocationUnavailable,
    AllocationDiagnosticUnavailable,
    Extent,
    UnwrittenGeometry,
    InvalidFaceIndex,
    NumericUnavailable,
    SceneUnavailable,
    SoundOwnerUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AreaBufferKey {
    identity: u64,
    index: usize,
}
#[derive(Debug)]
struct Buffer {
    bytes: Vec<u8>,
    known: Vec<bool>,
    live: bool,
}
#[derive(Debug)]
pub struct AreaStorage {
    identity: u64,
    buffers: Vec<Buffer>,
    calls: usize,
    fail_on_call: Option<usize>,
}
impl Default for AreaStorage {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
                .expect("area storage identity exhausted"),
            buffers: Vec::new(),
            calls: 0,
            fail_on_call: None,
        }
    }
}
impl AreaStorage {
    pub fn live_allocations(&self) -> usize {
        self.buffers.iter().filter(|b| b.live).count()
    }
    pub fn live_bytes(&self) -> usize {
        self.buffers
            .iter()
            .filter(|b| b.live)
            .map(|b| b.bytes.len())
            .sum()
    }
    pub fn allocation_calls(&self) -> usize {
        self.calls
    }
    pub fn fail_on_allocation(&mut self, call: Option<usize>) {
        self.fail_on_call = call;
    }
    fn allocate(&mut self, bytes: usize) -> Result<Option<AreaBufferKey>, AreaError> {
        self.calls = self
            .calls
            .checked_add(1)
            .ok_or(AreaError::AllocationUnavailable)?;
        if self.fail_on_call == Some(self.calls) {
            return Ok(None);
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(bytes).is_err() {
            return Ok(None);
        }
        data.resize(bytes, 0);
        let mut known = Vec::new();
        known
            .try_reserve_exact(bytes)
            .map_err(|_| AreaError::AllocationUnavailable)?;
        known.resize(bytes, false);
        self.buffers
            .try_reserve(1)
            .map_err(|_| AreaError::AllocationUnavailable)?;
        let key = AreaBufferKey {
            identity: self.identity,
            index: self.buffers.len(),
        };
        self.buffers.push(Buffer {
            bytes: data,
            known,
            live: true,
        });
        Ok(Some(key))
    }
    fn buffer(&self, key: AreaBufferKey) -> Result<&Buffer, AreaError> {
        if key.identity != self.identity {
            return Err(AreaError::ForeignArena);
        }
        self.buffers
            .get(key.index)
            .filter(|b| b.live)
            .ok_or(AreaError::Extent)
    }
    fn buffer_mut(&mut self, key: AreaBufferKey) -> Result<&mut Buffer, AreaError> {
        self.buffer(key)?;
        Ok(&mut self.buffers[key.index])
    }
    fn free(&mut self, key: Option<AreaBufferKey>) -> Result<(), AreaError> {
        if let Some(key) = key {
            let b = self.buffer_mut(key)?;
            b.live = false;
            b.bytes = Vec::new();
            b.known = Vec::new();
        }
        Ok(())
    }
    fn write(&mut self, key: AreaBufferKey, at: usize, bytes: &[u8]) -> Result<(), AreaError> {
        let b = self.buffer_mut(key)?;
        let end = at.checked_add(bytes.len()).ok_or(AreaError::Extent)?;
        b.bytes
            .get_mut(at..end)
            .ok_or(AreaError::Extent)?
            .copy_from_slice(bytes);
        b.known[at..end].fill(true);
        Ok(())
    }
    fn read(&self, key: AreaBufferKey, at: usize, len: usize) -> Result<&[u8], AreaError> {
        let b = self.buffer(key)?;
        let end = at.checked_add(len).ok_or(AreaError::Extent)?;
        let known = b.known.get(at..end).ok_or(AreaError::Extent)?;
        if !known.iter().all(|x| *x) {
            return Err(AreaError::UnwrittenGeometry);
        }
        b.bytes.get(at..end).ok_or(AreaError::Extent)
    }
}
#[derive(Debug)]
pub struct AreaFrame {
    pub key: AreaKey,
    pub scene: Option<SceneKey>,
    pub(crate) previous: Option<AreaKey>,
    pub(crate) next: Option<AreaKey>,
    pub vertex_count: u32,
    pub face_count: u32,
    buffers: [Option<AreaBufferKey>; 4],
    pub(crate) sounds: Vec<FrameKey>,
    pub unknown_164: i32,
    pub switch_fade_time: u32,
}
impl AreaFrame {
    pub(crate) fn new(key: AreaKey) -> Self {
        Self {
            key,
            scene: None,
            previous: None,
            next: None,
            vertex_count: 0,
            face_count: 0,
            buffers: [None; 4],
            sounds: Vec::new(),
            unknown_164: 0,
            switch_fade_time: 500,
        }
    }
    pub fn sounds(&self) -> &[FrameKey] {
        &self.sounds
    }
    pub fn buffer(&self, slot: usize) -> Option<AreaBufferKey> {
        self.buffers.get(slot).copied().flatten()
    }
    pub fn set_switch_fade_time(&mut self, time: u32) {
        self.switch_fade_time = time.max(1)
    }
    /// Exact four-array allocation/copy order of Build5880. Typed input slices
    /// are the caller's bounds-checked copy spans; face indices remain raw.
    pub(crate) fn build(
        &mut self,
        storage: &mut AreaStorage,
        vertices: &[[f32; 3]],
        faces: &[[u16; 3]],
    ) -> Result<i32, AreaError> {
        let nv = u32::try_from(vertices.len()).map_err(|_| AreaError::Extent)?;
        let nf = u32::try_from(faces.len()).map_err(|_| AreaError::Extent)?;
        let vb = vertices.len().checked_mul(12).ok_or(AreaError::Extent)?;
        let fb = faces.len().checked_mul(6).ok_or(AreaError::Extent)?;
        let pb = faces.len().checked_mul(16).ok_or(AreaError::Extent)?;
        // The old native count fields remain until all allocations succeed.
        for buffer in &mut self.buffers {
            storage.free(*buffer)?;
            *buffer = None;
        }
        for (slot, bytes) in [vb, vb, fb, pb].into_iter().enumerate() {
            self.buffers[slot] = storage.allocate(bytes)?;
        }
        if self.buffers.iter().any(Option::is_none) {
            // Original emits the OOM diagnostic and calls fatal ErrorHandler.
            // No successful native diagnostic callback is invented here.
            return Err(AreaError::AllocationDiagnosticUnavailable);
        }
        self.vertex_count = nv;
        for (index, vertex) in vertices.iter().enumerate() {
            for (axis, value) in vertex.iter().enumerate() {
                storage.write(
                    self.buffers[0].unwrap(),
                    index * 12 + axis * 4,
                    &value.to_bits().to_le_bytes(),
                )?;
            }
        }
        self.face_count = nf;
        for (index, face) in faces.iter().enumerate() {
            for (axis, value) in face.iter().enumerate() {
                storage.write(
                    self.buffers[2].unwrap(),
                    index * 6 + axis * 2,
                    &value.to_le_bytes(),
                )?;
            }
        }
        Ok(0)
    }
    pub(crate) fn free_geometry(&mut self, storage: &mut AreaStorage) -> Result<(), AreaError> {
        for buffer in &mut self.buffers {
            storage.free(*buffer)?;
            *buffer = None;
        }
        Ok(())
    }
    pub fn local_vertex(&self, storage: &AreaStorage, index: usize) -> Result<[f32; 3], AreaError> {
        self.vector(storage, 0, index)
    }
    pub fn world_vertex(&self, storage: &AreaStorage, index: usize) -> Result<[f32; 3], AreaError> {
        self.vector(storage, 1, index)
    }
    fn vector(
        &self,
        storage: &AreaStorage,
        slot: usize,
        index: usize,
    ) -> Result<[f32; 3], AreaError> {
        if index >= self.vertex_count as usize {
            return Err(AreaError::InvalidFaceIndex);
        }
        let bytes = storage.read(
            self.buffers[slot].ok_or(AreaError::UnwrittenGeometry)?,
            index.checked_mul(12).ok_or(AreaError::Extent)?,
            12,
        )?;
        Ok(std::array::from_fn(|axis| {
            f32::from_bits(u32::from_le_bytes(
                bytes[axis * 4..axis * 4 + 4].try_into().unwrap(),
            ))
        }))
    }
    pub fn plane(&self, storage: &AreaStorage, index: usize) -> Result<[f32; 4], AreaError> {
        if index >= self.face_count as usize {
            return Err(AreaError::Extent);
        }
        let bytes = storage.read(
            self.buffers[3].ok_or(AreaError::UnwrittenGeometry)?,
            index.checked_mul(16).ok_or(AreaError::Extent)?,
            16,
        )?;
        Ok(std::array::from_fn(|axis| {
            f32::from_bits(u32::from_le_bytes(
                bytes[axis * 4..axis * 4 + 4].try_into().unwrap(),
            ))
        }))
    }
    pub(crate) fn update_v(
        &mut self,
        graph: &mut I3dFrameGraph,
        storage: &mut AreaStorage,
    ) -> Result<(), AreaError> {
        if self.vertex_count == 0 {
            return Ok(());
        }
        let m = graph
            .world_matrix(self.key.frame())
            .map_err(|_| AreaError::NumericUnavailable)?;
        for index in 0..self.vertex_count as usize {
            let v = self.local_vertex(storage, index)?;
            for axis in 0..3 {
                let value = number(add(
                    add(
                        add(mul(v[0], m.elements[axis]), m.elements[12 + axis]),
                        mul(v[1], m.elements[4 + axis]),
                    ),
                    mul(v[2], m.elements[8 + axis]),
                ))?;
                storage.write(
                    self.buffers[1].ok_or(AreaError::UnwrittenGeometry)?,
                    index * 12 + axis * 4,
                    &value.to_bits().to_le_bytes(),
                )?;
            }
        }
        for index in 0..self.face_count as usize {
            let bytes = storage.read(
                self.buffers[2].ok_or(AreaError::UnwrittenGeometry)?,
                index * 6,
                6,
            )?;
            let face: [u16; 3] = std::array::from_fn(|axis| {
                u16::from_le_bytes(bytes[axis * 2..axis * 2 + 2].try_into().unwrap())
            });
            let a = self.world_vertex(storage, usize::from(face[0]))?;
            let b = self.world_vertex(storage, usize::from(face[1]))?;
            let c = self.world_vertex(storage, usize::from(face[2]))?;
            let p = plane(a, b, c)?;
            for (axis, value) in p.iter().enumerate() {
                storage.write(
                    self.buffers[3].ok_or(AreaError::UnwrittenGeometry)?,
                    index * 16 + axis * 4,
                    &value.to_bits().to_le_bytes(),
                )?;
            }
        }
        Ok(())
    }
    pub fn check_point(
        &self,
        storage: &AreaStorage,
        position: [f32; 3],
    ) -> Result<bool, AreaError> {
        for index in 0..self.face_count as usize {
            let p = self.plane(storage, index)?;
            if number(add(
                add(
                    add(mul(position[2], p[2]), mul(position[1], p[1])),
                    mul(position[0], p[0]),
                ),
                p[3],
            ))? > 0.
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
}
fn number(value: f32) -> Result<f32, AreaError> {
    if value.is_finite() && !value.is_subnormal() {
        Ok(value)
    } else {
        Err(AreaError::NumericUnavailable)
    }
}
fn plane(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Result<[f32; 4], AreaError> {
    for value in a.into_iter().chain(b).chain(c) {
        number(value)?;
    }
    let ab = std::array::from_fn::<_, 3, _>(|i| sub(a[i], b[i]));
    let cb = std::array::from_fn::<_, 3, _>(|i| sub(c[i], b[i]));
    let mut n = [
        sub(mul(ab[2], cb[1]), mul(ab[1], cb[2])),
        sub(mul(ab[0], cb[2]), mul(cb[0], ab[2])),
        sub(mul(cb[0], ab[1]), mul(ab[0], cb[1])),
    ];
    let square = number(add(add(mul(n[2], n[2]), mul(n[1], n[1])), mul(n[0], n[0])))?;
    if f64::from(sub(square, 1.).abs()) >= f64::from_bits(0x3e45798ee0000000) {
        if square >= f32::from_bits(0x322bcc77) {
            let inverse = number(div(1., sqrt(square)))?;
            for value in &mut n {
                *value = number(mul(*value, inverse))?;
            }
        } else {
            let axis = if n[0] != 0. {
                0
            } else if n[2] != 0. {
                2
            } else {
                1
            };
            n[axis] = if n[axis] >= 0. { 1. } else { -1. };
        }
    }
    let d = -number(add(add(mul(a[2], n[2]), mul(a[1], n[1])), mul(a[0], n[0])))?;
    Ok([n[0], n[1], n[2], d])
}

impl From<AreaError> for crate::model_loading::ModelError {
    fn from(error: AreaError) -> Self {
        Self::Area(error)
    }
}
impl crate::model_loading::ModelArena {
    pub fn create_area(&mut self) -> Result<AreaKey, crate::model_loading::ModelError> {
        use crate::frame_transform::{FrameKind, I3dFrameState};
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Area;
        state.flags |= 0x40000;
        let frame = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        let key = AreaKey {
            frame,
            reference: self.lookup_reference(frame),
        };
        self.areas.push(AreaFrame::new(key));
        Ok(key)
    }
    pub fn area_key(&self, frame: FrameKey) -> Result<AreaKey, crate::model_loading::ModelError> {
        self.frame(frame)?;
        self.areas
            .iter()
            .find(|a| a.key.frame() == frame)
            .map(|a| a.key)
            .ok_or_else(|| AreaError::UnknownArea(frame).into())
    }
    pub fn area(&self, key: AreaKey) -> Result<&AreaFrame, crate::model_loading::ModelError> {
        if self.lookup_reference(key.frame()) != key.reference {
            return Err(AreaError::ForeignArena.into());
        }
        self.frame(key.frame())?;
        self.areas
            .iter()
            .find(|a| a.key == key)
            .ok_or_else(|| AreaError::UnknownArea(key.frame()).into())
    }
    pub(crate) fn area_mut(
        &mut self,
        key: AreaKey,
    ) -> Result<&mut AreaFrame, crate::model_loading::ModelError> {
        self.area(key)?;
        Ok(self.areas.iter_mut().find(|a| a.key == key).unwrap())
    }
    pub fn area_storage(&self) -> &AreaStorage {
        &self.area_storage
    }
    pub fn area_storage_mut(&mut self) -> &mut AreaStorage {
        &mut self.area_storage
    }
    pub fn area_build(
        &mut self,
        key: AreaKey,
        vertices: &[[f32; 3]],
        faces: &[[u16; 3]],
    ) -> Result<i32, crate::model_loading::ModelError> {
        self.area(key)?;
        let a = self.areas.iter_mut().find(|a| a.key == key).unwrap();
        Ok(a.build(&mut self.area_storage, vertices, faces)?)
    }
    /// Native5440 changes non-owning circular links and Scene count/version.
    /// In particular removal leaves stale prev/next words as the source does.
    pub(crate) fn area_change_scene_ring(
        &mut self,
        key: AreaKey,
        new_scene: Option<SceneKey>,
    ) -> Result<(), crate::model_loading::ModelError> {
        let old = self.area(key)?.scene;
        let previous = self.area(key)?.previous;
        if let (Some(old), Some(previous)) = (old, previous) {
            let next = self.area(key)?.next.ok_or(AreaError::SceneUnavailable)?;
            let si = self.scene_index(old)?;
            if self.scenes[si].area_head == Some(key) {
                self.scenes[si].area_head = Some(next);
            }
            if previous == key {
                self.scenes[si].area_head = None;
            } else {
                self.area_mut(previous)?.next = Some(next);
                self.area_mut(next)?.previous = Some(previous);
            }
            self.scenes[si].area_count = self.scenes[si].area_count.wrapping_sub(1);
            self.scenes[si].areas_changed = true;
            self.area_mut(key)?.scene = None;
        }
        let Some(scene) = new_scene else {
            return Ok(());
        };
        let si = self.scene_index(scene)?;
        self.area_mut(key)?.scene = Some(scene);
        if let Some(head) = self.scenes[si].area_head {
            let previous = self
                .area(head)?
                .previous
                .ok_or(AreaError::SceneUnavailable)?;
            self.area_mut(previous)?.next = Some(key);
            self.area_mut(key)?.previous = Some(previous);
            self.area_mut(key)?.next = Some(head);
            self.area_mut(head)?.previous = Some(key);
        } else {
            self.area_mut(key)?.previous = Some(key);
            self.area_mut(key)?.next = Some(key);
            self.scenes[si].area_head = Some(key);
        }
        self.scenes[si].area_count = self.scenes[si].area_count.wrapping_add(1);
        self.scenes[si].areas_changed = true;
        Ok(())
    }
    pub(crate) fn link_area_frame_derived(
        &mut self,
        key: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, crate::model_loading::ModelError> {
        use crate::frame_transform::FrameKind;
        use crate::scene_ownership::{FrameOwner, SectorKey};
        let area = self.area_key(key)?;
        if self.graph.links(key)?.parent == parent {
            return Ok(0);
        }
        self.area_change_scene_ring(area, None)?;
        if let Some(parent) = parent {
            let scene = if let Some(FrameOwner::Scene(scene)) = self.frame(parent)?.owner {
                scene
            } else {
                let mut current = Some(parent);
                let mut steps = 0usize;
                loop {
                    let frame = current.ok_or(AreaError::SceneUnavailable)?;
                    if self.state(frame)?.kind == FrameKind::Sector {
                        break self
                            .sector(SectorKey(frame))?
                            .scene
                            .ok_or(AreaError::SceneUnavailable)?;
                    }
                    current = self.graph.links(frame)?.parent;
                    steps += 1;
                    if steps > self.live_frame_count() {
                        return Err(AreaError::SceneUnavailable.into());
                    }
                }
            };
            self.area_change_scene_ring(area, Some(scene))?;
        }
        self.link_frame_base(key, parent, flags)
    }
    pub(crate) fn check_area_destruction_domain(
        &self,
        key: AreaKey,
    ) -> Result<(), crate::model_loading::ModelError> {
        let a = self.area(key)?;
        for &sound in &a.sounds {
            self.sound_key(sound)
                .map_err(|_| AreaError::SoundOwnerUnavailable)?;
        }
        if let Some(scene) = a.scene {
            self.scene(scene)?;
        }
        Ok(())
    }
    pub(crate) fn destroy_area_derived(
        &mut self,
        key: AreaKey,
    ) -> Result<(), crate::model_loading::ModelError> {
        self.check_area_destruction_domain(key)?;
        while let Some(&sound) = self.area(key)?.sounds.first() {
            self.delete_sound_from_area(key, self.sound_key(sound)?)?;
        }
        self.areas
            .iter_mut()
            .find(|a| a.key == key)
            .unwrap()
            .free_geometry(&mut self.area_storage)?;
        self.area_change_scene_ring(key, None)?;
        let at = self.areas.iter().position(|a| a.key == key).unwrap();
        self.areas.remove(at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_loading::ModelArena;
    #[test]
    fn build_owns_four_arrays_but_leaves_derived_geometry_unwritten_until_update() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let key = arena.create_area().unwrap();
            arena
                .area_build(
                    key,
                    &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
                    &[[0, 1, 2]],
                )
                .unwrap();
            assert_eq!(arena.area_storage().live_allocations(), 4);
            assert_eq!(
                arena
                    .area(key)
                    .unwrap()
                    .world_vertex(arena.area_storage(), 0),
                Err(AreaError::UnwrittenGeometry)
            );
            assert_eq!(
                arena.area(key).unwrap().plane(arena.area_storage(), 0),
                Err(AreaError::UnwrittenGeometry)
            );
            arena.update_frame(key.frame()).unwrap();
            assert_eq!(
                arena
                    .area(key)
                    .unwrap()
                    .world_vertex(arena.area_storage(), 1)
                    .unwrap(),
                [1., 0., 0.]
            );
            assert!(arena
                .area(key)
                .unwrap()
                .check_point(arena.area_storage(), [0., 0., -1.])
                .unwrap());
            assert!(!arena
                .area(key)
                .unwrap()
                .check_point(arena.area_storage(), [0., 0., 1.])
                .unwrap());
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.area_storage().live_allocations(), 0);
        });
    }
    #[test]
    fn failed_second_build_preserves_native_old_counts_and_new_allocation_prefix() {
        let mut arena = ModelArena::default();
        let key = arena.create_area().unwrap();
        arena.area_build(key, &[[0.; 3]; 3], &[[0, 1, 2]]).unwrap();
        arena.area_storage_mut().fail_on_allocation(Some(6));
        assert!(matches!(
            arena.area_build(key, &[[2.; 3]; 4], &[[0, 1, 2]; 2]),
            Err(crate::model_loading::ModelError::Area(
                AreaError::AllocationDiagnosticUnavailable
            ))
        ));
        assert_eq!(arena.area_storage().allocation_calls(), 8);
        assert_eq!(arena.area_storage().live_allocations(), 3);
        let area = arena.area(key).unwrap();
        assert_eq!(area.vertex_count, 3);
        assert_eq!(area.face_count, 1);
        assert!(area.buffer(0).is_some());
        assert!(area.buffer(1).is_none());
        assert_eq!(
            area.local_vertex(arena.area_storage(), 0),
            Err(AreaError::UnwrittenGeometry)
        );
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.area_storage().live_allocations(), 0);
    }
    #[test]
    fn invalid_face_retains_all_world_vertices_and_prior_plane_writes() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let key = arena.create_area().unwrap();
            arena
                .area_build(
                    key,
                    &[[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
                    &[[0, 1, 2], [0, 1, 99]],
                )
                .unwrap();
            assert!(arena.update_frame(key.frame()).is_err());
            let area = arena.area(key).unwrap();
            assert!(area.world_vertex(arena.area_storage(), 2).is_ok());
            assert!(area.plane(arena.area_storage(), 0).is_ok());
            assert_eq!(
                area.plane(arena.area_storage(), 1),
                Err(AreaError::UnwrittenGeometry)
            );
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.area_storage().live_allocations(), 0);
        });
    }
    #[test]
    fn same_area_ring_has_real_circular_links_and_last_release_removes_membership() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0].frame();
        let a = arena.create_area().unwrap();
        let b = arena.create_area().unwrap();
        arena.link_frame(a.frame(), Some(sector), 0).unwrap();
        arena.link_frame(b.frame(), Some(sector), 0).unwrap();
        assert_eq!(arena.scene(scene).unwrap().area_count, 2);
        assert_eq!(arena.scene(scene).unwrap().area_head, Some(a));
        assert_eq!(arena.area(a).unwrap().next, Some(b));
        assert_eq!(arena.area(b).unwrap().next, Some(a));
        arena.release(a.frame()).unwrap();
        assert_eq!(arena.scene(scene).unwrap().area_head, Some(b));
        assert_eq!(arena.area(b).unwrap().previous, Some(b));
        assert_eq!(arena.area(b).unwrap().next, Some(b));
        arena.release(b.frame()).unwrap();
        assert_eq!(arena.scene(scene).unwrap().area_count, 0);
        assert_eq!(arena.scene(scene).unwrap().area_head, None);
        assert!(arena.scene(scene).unwrap().areas_changed);
    }
}
