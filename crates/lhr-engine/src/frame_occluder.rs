//! Original LS3DF I3D_occluder, PC RVAs0x3a430..0x3c9b0.
//! Binary SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! Geometry buffers belong to the same ModelArena as the frame and hierarchy.
//! Unknown constructor/allocation bytes are not exposed as recovered zeroes.
use crate::frame_transform::{FrameKey, FrameKind, I3dFrameGraph, I3dFrameState};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::{FrameOwner, SceneKey, SectorKey};
use crate::script_scheduler::FrameReferenceId;

pub const OCCLUDER_WORDS: usize = 0x5ec / 4;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OccluderKey {
    frame: FrameKey,
    reference: FrameReferenceId,
}
impl OccluderKey {
    pub fn frame(self) -> FrameKey {
        self.frame
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OccluderError {
    ForeignArena,
    UnknownOccluder(FrameKey),
    BufferUnavailable,
    CopySpanUnavailable,
    NumericUnavailable(&'static str),
    GeometryUnavailable,
    ImageError(u32),
    SceneUnavailable,
    SectorTransitionUnavailable,
    AllocationUnavailable,
}
impl From<OccluderError> for ModelError {
    fn from(value: OccluderError) -> Self {
        Self::Occluder(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OccluderBufferKey {
    identity: u64,
    index: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OccluderDiagnosticKind {
    BuildCopyOutOfMemory,
    BuildAdjacencyOutOfMemory,
    LoadImageOutOfMemory,
    DestructedWithoutBuild,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OccluderDiagnostic {
    pub kind: OccluderDiagnosticKind,
    pub frame: FrameReferenceId,
    pub name: Vec<u8>,
    /// None denotes an unavailable original ImageFileName producer, not NULL.
    pub image_filename: Option<Vec<u8>>,
    pub native_diagnostic_backend_executed: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OccluderStorageEvent {
    Allocate {
        call: usize,
        bytes: usize,
        buffer: Option<OccluderBufferKey>,
    },
    Free {
        buffer: Option<OccluderBufferKey>,
    },
    Diagnostic {
        request: usize,
    },
}
#[derive(Debug)]
struct Buffer {
    bytes: Vec<u8>,
    known: Vec<bool>,
    live: bool,
}
/// Actual CPU storage. Replaced native auxiliary pointers are not implicitly
/// freed: their still-live allocations remain observable as external survivors.
#[derive(Debug)]
pub struct OccluderStorage {
    identity: u64,
    buffers: Vec<Buffer>,
    allocation_calls: usize,
    fail_on_call: Option<usize>,
    diagnostics: Vec<OccluderDiagnostic>,
    events: Vec<OccluderStorageEvent>,
}
impl Default for OccluderStorage {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT
                .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .expect("occluder storage identity exhausted"),
            buffers: Vec::new(),
            allocation_calls: 0,
            fail_on_call: None,
            diagnostics: Vec::new(),
            events: Vec::new(),
        }
    }
}
impl OccluderStorage {
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
    pub fn diagnostics(&self) -> &[OccluderDiagnostic] {
        &self.diagnostics
    }
    pub fn events(&self) -> &[OccluderStorageEvent] {
        &self.events
    }
    pub fn allocation_calls(&self) -> usize {
        self.allocation_calls
    }
    /// Controlled allocator failure at an absolute allocation call. No buffer
    /// is allocated on that call; all preceding actual allocations survive.
    pub fn fail_on_allocation(&mut self, call: Option<usize>) {
        self.fail_on_call = call;
    }
    fn allocate(&mut self, bytes: usize) -> Result<Option<OccluderBufferKey>, OccluderError> {
        self.allocation_calls = self
            .allocation_calls
            .checked_add(1)
            .ok_or(OccluderError::AllocationUnavailable)?;
        if self.fail_on_call == Some(self.allocation_calls) {
            self.events.push(OccluderStorageEvent::Allocate {
                call: self.allocation_calls,
                bytes,
                buffer: None,
            });
            return Ok(None);
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(bytes).is_err() {
            self.events.push(OccluderStorageEvent::Allocate {
                call: self.allocation_calls,
                bytes,
                buffer: None,
            });
            return Ok(None);
        }
        data.resize(bytes, 0);
        let mut known = Vec::new();
        known
            .try_reserve_exact(bytes)
            .map_err(|_| OccluderError::AllocationUnavailable)?;
        known.resize(bytes, false);
        self.buffers
            .try_reserve(1)
            .map_err(|_| OccluderError::AllocationUnavailable)?;
        let key = OccluderBufferKey {
            identity: self.identity,
            index: self.buffers.len(),
        };
        self.buffers.push(Buffer {
            bytes: data,
            known,
            live: true,
        });
        self.events.push(OccluderStorageEvent::Allocate {
            call: self.allocation_calls,
            bytes,
            buffer: Some(key),
        });
        Ok(Some(key))
    }
    fn free(&mut self, key: Option<OccluderBufferKey>) -> Result<(), OccluderError> {
        if let Some(key) = key {
            if key.identity != self.identity {
                return Err(OccluderError::ForeignArena);
            }
            let b = self
                .buffers
                .get_mut(key.index)
                .filter(|b| b.live)
                .ok_or(OccluderError::BufferUnavailable)?;
            b.live = false;
            b.bytes = Vec::new();
            b.known = Vec::new();
        }
        self.events.push(OccluderStorageEvent::Free { buffer: key });
        Ok(())
    }
    pub(crate) fn write(
        &mut self,
        key: OccluderBufferKey,
        offset: usize,
        bytes: &[u8],
    ) -> Result<(), OccluderError> {
        if key.identity != self.identity {
            return Err(OccluderError::ForeignArena);
        }
        let b = self
            .buffers
            .get_mut(key.index)
            .filter(|b| b.live)
            .ok_or(OccluderError::BufferUnavailable)?;
        let end = offset
            .checked_add(bytes.len())
            .filter(|end| *end <= b.bytes.len())
            .ok_or(OccluderError::CopySpanUnavailable)?;
        b.bytes[offset..end].copy_from_slice(bytes);
        b.known[offset..end].fill(true);
        Ok(())
    }
    pub fn bytes(&self, key: OccluderBufferKey) -> Result<&[u8], OccluderError> {
        if key.identity != self.identity {
            return Err(OccluderError::ForeignArena);
        }
        let b = self
            .buffers
            .get(key.index)
            .filter(|b| b.live)
            .ok_or(OccluderError::BufferUnavailable)?;
        if b.known.iter().any(|known| !*known) {
            return Err(OccluderError::BufferUnavailable);
        }
        Ok(&b.bytes)
    }
}
#[derive(Debug)]
pub struct OccluderFrame {
    pub key: OccluderKey,
    pub scene: Option<SceneKey>,
    pub sector: Option<SectorKey>,
    pub previous: Option<OccluderKey>,
    pub next: Option<OccluderKey>,
    pub(crate) next_in_sector: Option<OccluderKey>,
    words: [Option<u32>; OCCLUDER_WORDS],
    buffers: [Option<OccluderBufferKey>; 7],
}
impl OccluderFrame {
    pub fn word(&self, offset: usize) -> Option<u32> {
        if offset == 0x5e0 && self.next_in_sector.is_some() {
            return None;
        }
        if offset.is_multiple_of(4) {
            self.words.get(offset / 4).copied().flatten()
        } else {
            None
        }
    }
    pub fn buffer(&self, offset: usize) -> Option<OccluderBufferKey> {
        Self::buffer_index(offset).and_then(|index| self.buffers[index])
    }
    fn buffer_index(offset: usize) -> Option<usize> {
        [0x148, 0x14c, 0x150, 0x15c, 0x160, 0x164, 0x168]
            .iter()
            .position(|&v| v == offset)
    }
    fn set_buffer(&mut self, offset: usize, key: Option<OccluderBufferKey>) {
        let i = Self::buffer_index(offset).unwrap();
        self.buffers[i] = key;
        self.words[offset / 4] = key.is_none().then_some(0);
    }
    pub(crate) fn put(&mut self, offset: usize, value: u32) {
        self.words[offset / 4] = Some(value);
    }
}
impl ModelArena {
    /// Original I3D_occluder ctor0x3a430 on the canonical common-frame owner.
    pub fn create_occluder(&mut self) -> Result<OccluderKey, ModelError> {
        self.occluders
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Occluder;
        state.flags |= 0x08100000;
        let frame = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        Ok(self.finish_occluder_constructor(frame))
    }
    pub(crate) fn finish_occluder_constructor(&mut self, frame: FrameKey) -> OccluderKey {
        let key = OccluderKey {
            frame,
            reference: self.lookup_reference(frame),
        };
        let mut payload = OccluderFrame {
            key,
            scene: None,
            sector: None,
            previous: None,
            next: None,
            next_in_sector: None,
            words: [None; OCCLUDER_WORDS],
            buffers: [None; 7],
        };
        for offset in [
            0x39c, 0x5b0, 0x5c4, 0x158, 0x154, 0x140, 0x144, 0x5e0, 0x5e4, 0x168, 0x14c, 0x148,
            0x150, 0x160, 0x15c, 0x164, 0x5c8, 0x5cc, 0x5d0, 0x5d4, 0x5d8, 0x5dc, 0x5e8,
        ] {
            payload.put(offset, 0);
        }
        self.occluders.push(payload);
        key
    }
    pub fn occluder_key(&self, frame: FrameKey) -> Result<OccluderKey, ModelError> {
        self.frame(frame)?;
        self.occluders
            .iter()
            .find(|v| v.key.frame == frame)
            .map(|v| v.key)
            .ok_or_else(|| OccluderError::UnknownOccluder(frame).into())
    }
    pub(crate) fn occluder_index(&self, key: OccluderKey) -> Result<usize, ModelError> {
        if key.reference != self.lookup_reference(key.frame) {
            return Err(OccluderError::ForeignArena.into());
        }
        self.frame(key.frame)?;
        self.occluders
            .iter()
            .position(|v| v.key == key)
            .ok_or_else(|| OccluderError::UnknownOccluder(key.frame).into())
    }
    pub fn occluder(&self, key: OccluderKey) -> Result<&OccluderFrame, ModelError> {
        Ok(&self.occluders[self.occluder_index(key)?])
    }
    pub fn occluder_update_v(&mut self, key: OccluderKey) -> Result<(), ModelError> {
        let index = self.occluder_index(key)?;
        update_occluder_v(
            &mut self.graph,
            &mut self.occluders[index],
            &mut self.occluder_storage,
        )?;
        Ok(())
    }
    pub fn sector_occluder_links(
        &self,
        sector: SectorKey,
    ) -> Result<(Option<OccluderKey>, Option<OccluderKey>), ModelError> {
        let sector = self.sector(sector)?;
        Ok((sector.occluder_list, sector.hull_occluder))
    }
    fn occluder_diagnostic(
        &mut self,
        key: OccluderKey,
        kind: OccluderDiagnosticKind,
    ) -> Result<(), ModelError> {
        let name = self.frame(key.frame)?.name_bytes.clone();
        let index = self.occluder_storage.diagnostics.len();
        self.occluder_storage.diagnostics.push(OccluderDiagnostic {
            kind,
            frame: key.reference,
            name,
            image_filename: None,
            native_diagnostic_backend_executed: false,
        });
        self.occluder_storage
            .events
            .push(OccluderStorageEvent::Diagnostic { request: index });
        Ok(())
    }
    pub fn occluder_storage(&self) -> &OccluderStorage {
        &self.occluder_storage
    }
    pub fn occluder_storage_mut(&mut self) -> &mut OccluderStorage {
        &mut self.occluder_storage
    }
}
#[derive(Clone, Copy)]
struct EdgeNode {
    key: u32,
    face: u16,
    side: u16,
    left: Option<usize>,
    right: Option<usize>,
    next: usize,
}
fn tree_add(
    nodes: &mut Vec<EdgeNode>,
    capacity: usize,
    key: u32,
    face: u16,
    side: u16,
) -> Result<(), OccluderError> {
    let index = nodes.len();
    if index >= capacity {
        return Err(OccluderError::GeometryUnavailable);
    }
    if nodes.is_empty() {
        nodes.push(EdgeNode {
            key,
            face,
            side,
            left: None,
            right: None,
            next: 0,
        });
        return Ok(());
    }
    let mut at = 0;
    loop {
        if nodes[at].key == key {
            let mut tail = at;
            while nodes[tail].next != at {
                tail = nodes[tail].next;
            }
            nodes[tail].next = index;
            nodes.push(EdgeNode {
                key,
                face,
                side,
                left: Some(0),
                right: None,
                next: at,
            });
            return Ok(());
        }
        let left = key < nodes[at].key;
        let child = if left {
            nodes[at].left
        } else {
            nodes[at].right
        };
        if let Some(child) = child {
            at = child;
        } else {
            if left {
                nodes[at].left = Some(index);
            } else {
                nodes[at].right = Some(index);
            }
            nodes.push(EdgeNode {
                key,
                face,
                side,
                left: None,
                right: None,
                next: index,
            });
            return Ok(());
        }
    }
}
impl ModelArena {
    pub fn occluder_build(
        &mut self,
        key: OccluderKey,
        vertices: &[[f32; 3]],
        faces: &[[u16; 3]],
    ) -> Result<u32, ModelError> {
        self.occluder_index(key)?;
        let mut v = Vec::new();
        v.try_reserve_exact(
            vertices
                .len()
                .checked_mul(12)
                .ok_or(ModelError::AllocationOverflow)?,
        )
        .map_err(|_| ModelError::AllocationOverflow)?;
        for vertex in vertices {
            for value in vertex {
                v.extend_from_slice(&value.to_bits().to_le_bytes());
            }
        }
        let mut f = Vec::new();
        f.try_reserve_exact(
            faces
                .len()
                .checked_mul(6)
                .ok_or(ModelError::AllocationOverflow)?,
        )
        .map_err(|_| ModelError::AllocationOverflow)?;
        for face in faces {
            for value in face {
                f.extend_from_slice(&value.to_le_bytes());
            }
        }
        self.occluder_build_strided(
            key,
            &v,
            u32::try_from(vertices.len()).map_err(|_| ModelError::AllocationOverflow)?,
            &f,
            u32::try_from(faces.len()).map_err(|_| ModelError::AllocationOverflow)?,
            12,
        )
    }
    /// Native Build(v,n,f,n,stride), RVA0x3ac60. Copies raw words, including
    /// nonfinite vertices and unchecked indices; unsafe dereference is bounded
    /// only when a later UpdateV actually uses the face indices.
    pub fn occluder_build_strided(
        &mut self,
        key: OccluderKey,
        vertices: &[u8],
        vertex_count: u32,
        faces: &[u8],
        face_count: u32,
        stride: u32,
    ) -> Result<u32, ModelError> {
        let index = self.occluder_index(key)?;
        for offset in [0x148, 0x14c, 0x150] {
            let old = self.occluders[index].buffer(offset);
            if old.is_some() {
                self.occluder_storage.free(old)?;
            }
        }
        for (offset, size) in [
            (0x148, vertex_count.wrapping_mul(12)),
            (0x14c, vertex_count.wrapping_mul(12)),
            (0x150, face_count.wrapping_mul(6)),
        ] {
            let buffer = self.occluder_storage.allocate(size as usize)?;
            self.occluders[index].set_buffer(offset, buffer);
        }
        if [0x148, 0x14c, 0x150]
            .into_iter()
            .any(|offset| self.occluders[index].buffer(offset).is_none())
        {
            self.occluder_diagnostic(key, OccluderDiagnosticKind::BuildCopyOutOfMemory)?;
            return Ok(0x80000002);
        }
        self.occluders[index].put(0x140, vertex_count);
        let destination = self.occluders[index].buffer(0x148).unwrap();
        for vertex in 0..vertex_count {
            let source_offset = vertex.wrapping_mul(stride) as usize;
            let bytes = vertices
                .get(
                    source_offset
                        ..source_offset
                            .checked_add(12)
                            .ok_or(OccluderError::CopySpanUnavailable)?,
                )
                .ok_or(OccluderError::CopySpanUnavailable)?;
            self.occluder_storage
                .write(destination, vertex as usize * 12, bytes)?;
        }
        self.occluders[index].put(0x144, face_count);
        let size = face_count.wrapping_mul(6) as usize;
        let bytes = faces
            .get(..size)
            .ok_or(OccluderError::CopySpanUnavailable)?;
        self.occluder_storage
            .write(self.occluders[index].buffer(0x150).unwrap(), 0, bytes)?;
        self.occluder_build_adjacency(key)
    }
    /// Native internal Build(), RVA0x3ad90, including the original XSBTree
    /// duplicate-key circular insertion (0x3caf0), allocation and free order.
    pub fn occluder_build_adjacency(&mut self, key: OccluderKey) -> Result<u32, ModelError> {
        let index = self.occluder_index(key)?;
        let count = self.occluders[index].word(0x144).unwrap();
        let tree_capacity = (count as u16).wrapping_mul(3) as usize;
        let tree = self.occluder_storage.allocate(tree_capacity * 20)?;
        for (offset, size) in [
            (0x15c, count.wrapping_mul(6)),
            (0x160, count.wrapping_mul(3)),
            (0x164, count.wrapping_mul(16)),
            (0x168, count.wrapping_mul(2)),
        ] {
            let buffer = self.occluder_storage.allocate(size as usize)?;
            self.occluders[index].set_buffer(offset, buffer);
        }
        if tree.is_none()
            || [0x15c, 0x160, 0x164, 0x168]
                .into_iter()
                .any(|offset| self.occluders[index].buffer(offset).is_none())
        {
            self.occluder_diagnostic(key, OccluderDiagnosticKind::BuildAdjacencyOutOfMemory)?;
            if tree.is_some() {
                self.occluder_storage.free(tree)?;
            }
            return Ok(0x80000002);
        }
        let faces = self.occluders[index]
            .buffer(0x150)
            .ok_or(OccluderError::BufferUnavailable)?;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(tree_capacity)
            .map_err(|_| OccluderError::AllocationUnavailable)?;
        for face in 0..count {
            let raw = self.occluder_storage.bytes(faces)?;
            let offset = face as usize * 6;
            let f = raw
                .get(offset..offset + 6)
                .ok_or(OccluderError::CopySpanUnavailable)?;
            let indices =
                std::array::from_fn::<_, 3, _>(|i| u16::from_le_bytes([f[i * 2], f[i * 2 + 1]]));
            for (side, (a, b)) in [
                (indices[0], indices[1]),
                (indices[1], indices[2]),
                (indices[2], indices[0]),
            ]
            .into_iter()
            .enumerate()
            {
                let edge = (u32::from(a.min(b)) << 16) | u32::from(a.max(b));
                tree_add(&mut nodes, tree_capacity, edge, face as u16, side as u16)?;
            }
        }
        for node in &nodes {
            let neighbor = nodes[node.next];
            let offset = usize::from(node.face) * 3 + usize::from(node.side);
            self.occluder_storage.write(
                self.occluders[index].buffer(0x15c).unwrap(),
                offset * 2,
                &neighbor.face.to_le_bytes(),
            )?;
            self.occluder_storage.write(
                self.occluders[index].buffer(0x160).unwrap(),
                offset,
                &[neighbor.side as u8],
            )?;
        }
        let flags = self.occluders[index].buffer(0x168).unwrap();
        for face in 0..count {
            self.occluder_storage
                .write(flags, face as usize * 2, &[0, 0])?;
        }
        self.occluders[index].put(0x5d8, 1.0f32.to_bits());
        if tree.is_some() {
            self.occluder_storage.free(tree)?;
        }
        Ok(0)
    }
}
impl ModelArena {
    pub fn scene_occluder_ring(
        &self,
        scene: SceneKey,
    ) -> Result<(Option<OccluderKey>, u32, bool, bool), ModelError> {
        let s = self.scene(scene)?;
        Ok((
            s.occluder_head,
            s.occluder_count,
            s.occluder_flag_274,
            s.occluders_changed,
        ))
    }
    /// Original nonowning scene-ring transition0x3a650. Previous/next members
    /// deliberately retain their stale pointer values after removal.
    pub(crate) fn occluder_set_scene_ring(
        &mut self,
        key: OccluderKey,
        new_scene: Option<SceneKey>,
    ) -> Result<(), ModelError> {
        let index = self.occluder_index(key)?;
        if let (Some(old), Some(previous)) =
            (self.occluders[index].scene, self.occluders[index].previous)
        {
            let si = self.scene_index(old)?;
            if self.scenes[si].occluder_head == Some(key) {
                self.scenes[si].occluder_head = self.occluders[index].next;
            }
            if previous == key {
                self.scenes[si].occluder_head = None;
            } else {
                let next = self.occluders[index]
                    .next
                    .ok_or(OccluderError::SceneUnavailable)?;
                let p = self.occluder_index(previous)?;
                self.occluders[p].next = Some(next);
                let n = self.occluder_index(next)?;
                self.occluders[n].previous = Some(previous);
            }
            self.scenes[si].occluder_count = self.scenes[si].occluder_count.wrapping_sub(1);
            self.scenes[si].occluders_changed = true;
            self.occluders[index].scene = None;
            self.occluders[index].put(0x5c8, 0);
        }
        if let Some(scene) = new_scene {
            self.occluders[index].scene = Some(scene);
            self.occluders[index].words[0x5c8 / 4] = None;
            let si = self.scene_index(scene)?;
            if let Some(head) = self.scenes[si].occluder_head {
                let hi = self.occluder_index(head)?;
                let previous = self.occluders[hi]
                    .previous
                    .ok_or(OccluderError::SceneUnavailable)?;
                let pi = self.occluder_index(previous)?;
                self.occluders[pi].next = Some(key);
                self.occluders[index].previous = Some(previous);
                self.occluders[index].next = Some(head);
                self.occluders[hi].previous = Some(key);
            } else {
                self.occluders[index].previous = Some(key);
                self.occluders[index].next = Some(key);
                self.scenes[si].occluder_head = Some(key);
            }
            self.occluders[index].words[0x154 / 4] = None;
            self.occluders[index].words[0x158 / 4] = None;
            self.scenes[si].occluder_count = self.scenes[si].occluder_count.wrapping_add(1);
            self.scenes[si].occluder_flag_274 = true;
            self.scenes[si].occluders_changed = true;
        }
        Ok(())
    }
    /// Derived Occluder LinkTo0x3a770 precedes the common-frame LinkTo. Its
    /// ring changes are retained if the later base operation is unavailable.
    pub(crate) fn link_occluder_frame_derived(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, ModelError> {
        self.frame(child)?;
        if self.state(child)?.kind == FrameKind::Occluder {
            let key = self.occluder_key(child)?;
            if self.links(child)?.parent == parent {
                return Ok(0);
            }
            self.occluder_set_scene_ring(key, None)?;
            if let Some(parent) = parent {
                self.frame(parent)?;
                let scene = match self.frame(parent)?.owner {
                    Some(FrameOwner::Scene(scene)) => Some(scene),
                    _ => {
                        let mut at = Some(parent);
                        let mut scene = None;
                        let mut remaining = self.frames.len();
                        while let Some(frame) = at {
                            if remaining == 0 {
                                return Err(OccluderError::SceneUnavailable.into());
                            }
                            remaining -= 1;
                            if self.state(frame)?.kind == FrameKind::Sector {
                                scene = self.sector(SectorKey(frame))?.scene;
                                break;
                            }
                            at = self.links(frame)?.parent;
                        }
                        scene
                    }
                };
                if let Some(scene) = scene {
                    self.occluder_set_scene_ring(key, Some(scene))?;
                }
            }
        }
        self.link_frame_base(child, parent, flags)
    }
    pub(crate) fn link_frame_base(
        &mut self,
        child: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, ModelError> {
        let mut graph = std::mem::take(&mut self.graph);
        let mut callback_error = None;
        let result = graph.link_to_with_scene_context(
            child,
            parent,
            flags,
            |graph, key, old, new| match self.link_scene_transition(graph, key, old, new) {
                Ok(()) => Ok(()),
                Err(error) => {
                    callback_error = Some(error);
                    Err(crate::frame_transform::FrameError::UnsupportedLinkState)
                }
            },
        );
        self.graph = graph;
        if let Some(error) = callback_error {
            return Err(error);
        }
        Ok(result?)
    }
    fn link_scene_transition(
        &mut self,
        graph: &mut I3dFrameGraph,
        key: FrameKey,
        old: Option<FrameKey>,
        new: Option<FrameKey>,
    ) -> Result<(), ModelError> {
        let old = graph
            .closest_sector(old)?
            .map(|frame| self.sector(SectorKey(frame)).map(|sector| sector.scene))
            .transpose()?
            .flatten();
        let new = graph
            .closest_sector(new)?
            .map(|frame| self.sector(SectorKey(frame)).map(|sector| sector.scene))
            .transpose()?
            .flatten();
        if old == new {
            return Ok(());
        }
        if graph.state(key)?.kind == FrameKind::Sector {
            self.sector_ring_same_scene(SectorKey(key), new)?;
        }
        self.link_enum_transition(graph, key, true, new)?;
        if graph.state(key)?.kind == FrameKind::Occluder {
            self.occluder_set_scene_ring(self.occluder_key(key)?, new)?;
        }
        // Actual partial mask0x800 never calls back for type12. The mask0x1000
        // also admits none of the currently represented frame kinds. Both
        // traversals still read links and reset LastDestructedFrame.
        self.link_enum_transition(graph, key, false, new)?;
        self.link_enum_transition(graph, key, false, new)?;
        Ok(())
    }
    fn sector_ring_same_scene(
        &mut self,
        sector: SectorKey,
        scene: Option<SceneKey>,
    ) -> Result<(), ModelError> {
        self.sector_add_to_scene_ring(sector, scene)
    }
    fn link_enum_transition(
        &mut self,
        graph: &I3dFrameGraph,
        root: FrameKey,
        sectors: bool,
        scene: Option<SceneKey>,
    ) -> Result<(), ModelError> {
        let mut pending = Vec::new();
        if let Some(child) = graph.links(root)?.child {
            self.last_destroyed_frame = None;
            pending.push(child);
        }
        let mut remaining = self.frames.len();
        while let Some(frame) = pending.pop() {
            if remaining == 0 {
                return Err(OccluderError::SceneUnavailable.into());
            }
            remaining -= 1;
            let links = graph.links(frame)?;
            if let Some(next) = links.next {
                pending.push(next);
            }
            if sectors && graph.state(frame)?.kind == FrameKind::Sector {
                self.sector_ring_same_scene(SectorKey(frame), scene)?;
            }
            if let Some(child) = links.child {
                pending.push(child);
            }
        }
        Ok(())
    }
    pub(crate) fn clear_occluder_sector_backpointer(
        &mut self,
        key: OccluderKey,
    ) -> Result<(), ModelError> {
        let index = self.occluder_index(key)?;
        self.occluders[index].sector = None;
        self.occluders[index].put(0x5e4, 0);
        Ok(())
    }
    pub(crate) fn destroy_occluder_derived(&mut self, key: OccluderKey) -> Result<(), ModelError> {
        let index = self.occluder_index(key)?;
        for offset in [0x168, 0x14c, 0x148, 0x150, 0x160, 0x15c, 0x164] {
            let buffer = self.occluders[index].buffer(offset);
            if buffer.is_some() {
                self.occluder_storage.free(buffer)?;
            }
        }
        if let Some(sector) = self.occluders[index].sector {
            let si = self
                .sectors
                .iter()
                .position(|value| value.key == sector)
                .ok_or(OccluderError::SceneUnavailable)?;
            self.sectors[si].hull_occluder = None;
            self.occluders[index].sector = None;
            self.occluders[index].put(0x5e4, 0);
        }
        self.occluder_set_scene_ring(key, None)?;
        if f32::from_bits(self.occluders[index].word(0x5d8).unwrap()) == 0. {
            self.occluder_diagnostic(key, OccluderDiagnosticKind::DestructedWithoutBuild)?;
        }
        // The CPU diagnostic request is explicit; original console I/O remains
        // unexecuted/uncredited. Common base destruction follows this body.
        Ok(())
    }
}
fn number(value: f32) -> Result<f32, OccluderError> {
    if !value.is_finite() || value.is_subnormal() {
        Err(OccluderError::NumericUnavailable(
            "occluder PC24 numeric domain",
        ))
    } else {
        Ok(value)
    }
}
fn plus(a: f32, b: f32) -> Result<f32, OccluderError> {
    number(a)?;
    number(b)?;
    number(crate::native_precision::add(a, b))
}
fn minus(a: f32, b: f32) -> Result<f32, OccluderError> {
    number(a)?;
    number(b)?;
    number(crate::native_precision::sub(a, b))
}
fn times(a: f32, b: f32) -> Result<f32, OccluderError> {
    number(a)?;
    number(b)?;
    let value = number(crate::native_precision::mul(a, b))?;
    if value == 0. && a != 0. && b != 0. {
        return Err(OccluderError::NumericUnavailable(
            "occluder extended exponent product",
        ));
    }
    Ok(value)
}
fn vector(bytes: &[u8], index: usize) -> Result<[f32; 3], OccluderError> {
    let offset = index
        .checked_mul(12)
        .ok_or(OccluderError::CopySpanUnavailable)?;
    let raw = bytes
        .get(offset..offset + 12)
        .ok_or(OccluderError::CopySpanUnavailable)?;
    Ok(std::array::from_fn(|i| {
        f32::from_bits(u32::from_le_bytes(
            raw[i * 4..i * 4 + 4].try_into().unwrap(),
        ))
    }))
}
fn vector_bytes(values: [f32; 3]) -> [u8; 12] {
    let mut out = [0; 12];
    for (i, value) in values.into_iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    out
}
fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Result<[f32; 3], OccluderError> {
    let mut ab = [0.; 3];
    let mut cb = [0.; 3];
    for i in [2, 1, 0] {
        cb[i] = minus(c[i], b[i])?;
        ab[i] = minus(a[i], b[i])?;
    }
    let mut n = [
        minus(times(ab[2], cb[1])?, times(ab[1], cb[2])?)?,
        minus(times(ab[0], cb[2])?, times(cb[0], ab[2])?)?,
        minus(times(cb[0], ab[1])?, times(ab[0], cb[1])?)?,
    ];
    let squared = plus(
        plus(times(n[0], n[0])?, times(n[1], n[1])?)?,
        times(n[2], n[2])?,
    )?;
    if f64::from(minus(squared, 1.)?.abs()) >= f64::from(f32::from_bits(0x322bcc77)) {
        if squared >= f32::from_bits(0x322bcc77) {
            let length = number(crate::native_precision::sqrt(squared))?;
            let inverse = number(crate::native_precision::div(1., length))?;
            for value in &mut n {
                *value = times(*value, inverse)?;
            }
        } else {
            let i = if n[0] != 0. {
                0
            } else if n[2] != 0. {
                2
            } else {
                1
            };
            n[i] = if n[i] >= 0. { 1. } else { -1. };
        }
    }
    Ok(n)
}
pub(crate) fn update_occluder_v(
    graph: &mut I3dFrameGraph,
    frame: &mut OccluderFrame,
    storage: &mut OccluderStorage,
) -> Result<(), OccluderError> {
    let nv = frame.word(0x140).unwrap();
    if nv == 0 {
        return Ok(());
    }
    let matrix = graph
        .world_matrix(frame.key.frame)
        .map_err(|_| OccluderError::NumericUnavailable("occluder world matrix"))?;
    let source = frame
        .buffer(0x148)
        .ok_or(OccluderError::BufferUnavailable)?;
    let world = frame
        .buffer(0x14c)
        .ok_or(OccluderError::BufferUnavailable)?;
    for index in 0..nv as usize {
        let v = vector(storage.bytes(source)?, index)?;
        let mut transformed = [0.; 3];
        for (col, destination) in transformed.iter_mut().enumerate() {
            *destination = plus(
                plus(
                    plus(
                        times(v[0], matrix.elements[col])?,
                        matrix.elements[12 + col],
                    )?,
                    times(v[1], matrix.elements[4 + col])?,
                )?,
                times(v[2], matrix.elements[8 + col])?,
            )?;
        }
        storage.write(world, index * 12, &vector_bytes(transformed))?;
    }
    let nf = frame.word(0x144).unwrap();
    for face in 0..nf as usize {
        let faces = frame
            .buffer(0x150)
            .ok_or(OccluderError::BufferUnavailable)?;
        let raw = storage.bytes(faces)?;
        let face_indices = raw
            .get(face * 6..face * 6 + 6)
            .ok_or(OccluderError::CopySpanUnavailable)?;
        let indices = std::array::from_fn::<_, 3, _>(|i| {
            u16::from_le_bytes([face_indices[i * 2], face_indices[i * 2 + 1]]) as usize
        });
        let vertices = storage.bytes(world)?;
        let a = vector(vertices, indices[0])?;
        let b = vector(vertices, indices[1])?;
        let c = vector(vertices, indices[2])?;
        let n = normal(a, b, c)?;
        let yz = plus(times(n[1], a[1])?, times(n[2], a[2])?)?;
        let x = times(n[0], a[0])?;
        let planes = frame
            .buffer(0x164)
            .ok_or(OccluderError::BufferUnavailable)?;
        storage.write(planes, face * 16, &n[0].to_bits().to_le_bytes())?;
        storage.write(planes, face * 16 + 4, &n[1].to_bits().to_le_bytes())?;
        let dot = plus(yz, x)?;
        storage.write(planes, face * 16 + 8, &n[2].to_bits().to_le_bytes())?;
        storage.write(planes, face * 16 + 12, &(-dot).to_bits().to_le_bytes())?;
    }
    let raw = storage.bytes(world)?;
    let mut lo = vector(raw, 0)?;
    let mut hi = lo;
    for index in 1..nv as usize {
        let v = vector(raw, index)?;
        for i in 0..3 {
            if v[i] < lo[i] {
                lo[i] = v[i];
            }
            if v[i] > hi[i] {
                hi[i] = v[i];
            }
        }
    }
    let sums = [
        plus(hi[0], lo[0])?,
        plus(hi[1], lo[1])?,
        plus(hi[2], lo[2])?,
    ];
    for (i, sum) in sums.into_iter().enumerate() {
        frame.put(0x5cc + i * 4, times(sum, 0.5)?.to_bits());
    }
    let differences = [
        minus(hi[0], lo[0])?,
        minus(hi[1], lo[1])?,
        minus(hi[2], lo[2])?,
    ];
    let half = [
        times(differences[0], 0.5)?,
        times(differences[1], 0.5)?,
        times(differences[2], 0.5)?,
    ];
    let squared = plus(
        plus(times(half[2], half[2])?, times(half[1], half[1])?)?,
        times(half[0], half[0])?,
    )?;
    let radius = number(crate::native_precision::sqrt(squared))?;
    frame.put(0x5d8, radius.to_bits());
    frame.put(0x5dc, times(radius, radius)?.to_bits());
    Ok(())
}
impl ModelArena {
    /// Derived LoadImage0x3c9b0 geometry suffix, after common FrameLoadImage.
    /// Original read/stream ownership is supplied by the parsed-record caller;
    /// no unexecuted native dtaRead is credited by this adapter.
    pub fn occluder_load_geometry(
        &mut self,
        key: OccluderKey,
        vertices: &[[f32; 3]],
        faces: &[[u16; 3]],
        image_error: &mut u32,
    ) -> Result<(), ModelError> {
        let index = self.occluder_index(key)?;
        let nv = u32::try_from(vertices.len()).map_err(|_| ModelError::AllocationOverflow)?;
        let nf = u32::try_from(faces.len()).map_err(|_| ModelError::AllocationOverflow)?;
        self.occluders[index].put(0x140, nv);
        self.occluders[index].put(0x144, nf);
        for (offset, size) in [
            (0x148, nv.wrapping_mul(12)),
            (0x14c, nv.wrapping_mul(12)),
            (0x150, nf.wrapping_mul(6)),
        ] {
            let buffer = self.occluder_storage.allocate(size as usize)?;
            self.occluders[index].set_buffer(offset, buffer);
        }
        if [0x148, 0x150, 0x14c]
            .into_iter()
            .any(|offset| self.occluders[index].buffer(offset).is_none())
        {
            self.occluder_storage
                .free(self.occluders[index].buffer(0x148))?;
            self.occluders[index].set_buffer(0x148, None);
            self.occluders[index].put(0x140, 0);
            self.occluder_storage
                .free(self.occluders[index].buffer(0x14c))?;
            self.occluders[index].set_buffer(0x14c, None);
            self.occluder_storage
                .free(self.occluders[index].buffer(0x150))?;
            self.occluders[index].set_buffer(0x150, None);
            self.occluders[index].put(0x144, 0);
            self.occluder_diagnostic(key, OccluderDiagnosticKind::LoadImageOutOfMemory)?;
            *image_error = 0x80000002;
            return Ok(());
        }
        let v = self.occluders[index].buffer(0x148).unwrap();
        for (i, vertex) in vertices.iter().enumerate() {
            self.occluder_storage
                .write(v, i * 12, &vector_bytes(*vertex))?;
        }
        let f = self.occluders[index].buffer(0x150).unwrap();
        for (i, face) in faces.iter().enumerate() {
            for (j, value) in face.iter().enumerate() {
                self.occluder_storage
                    .write(f, i * 6 + j * 2, &value.to_le_bytes())?;
            }
        }
        let result = self.occluder_build_adjacency(key)?;
        if result != 0 {
            *image_error = result;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver_scene::{
        DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
    };
    use crate::itexture::{
        CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources,
    };
    use crate::scene_resources::{SceneResources, SceneScreen};
    struct Screen;
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> Result<i32, ()> {
            Ok(800)
        }
        fn screen_sy(&mut self) -> Result<i32, ()> {
            Ok(600)
        }
    }
    impl DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            Ok([0, 0, 800, 600])
        }
    }
    struct Images;
    impl TextureImageSource for Images {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            Err(TextureImageError::Missing)
        }
    }
    struct World {
        arena: ModelArena,
        resources: SceneResources,
        textures: TextureResources,
        storage: CpuTextureStorage,
        driver: DriverSceneOwner,
        scene: SceneKey,
    }
    impl World {
        fn new() -> Self {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut Screen,
                    &mut Images,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            Self {
                arena,
                resources,
                textures,
                storage,
                driver,
                scene,
            }
        }
        fn occluder(&mut self) -> OccluderKey {
            self.driver
                .create_occluder_frame(&mut self.arena, &self.resources, self.scene)
                .unwrap()
        }
        fn close(&mut self) {
            self.resources
                .close_constructed_scene(
                    &mut self.arena,
                    self.scene,
                    &mut self.textures,
                    &mut self.storage,
                )
                .unwrap();
        }
        fn destroy(&mut self) {
            self.driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut self.arena,
                        resources: &mut self.resources,
                        textures: &mut self.textures,
                        storage: &mut self.storage,
                    },
                    self.scene,
                )
                .unwrap();
            self.driver.release_empty_registry_storage().unwrap();
        }
    }
    const V: [[f32; 3]; 3] = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]];
    const F: [[u16; 3]; 1] = [[0, 1, 2]];
    fn words(arena: &ModelArena, key: OccluderKey, offset: usize) -> Vec<u32> {
        let bytes = arena
            .occluder_storage()
            .bytes(arena.occluder(key).unwrap().buffer(offset).unwrap())
            .unwrap();
        bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|v| u32::from_le_bytes(*v))
            .collect()
    }
    #[test]
    fn genuine_driver_scene_ownership_close_and_last_release_return_cpu_baseline() {
        let mut world = World::new();
        let key = world.occluder();
        let primary = world.arena.scene(world.scene).unwrap().sectors[0].frame();
        world.arena.occluder_build(key, &V, &F).unwrap();
        world
            .arena
            .scene_add_frame(world.scene, key.frame())
            .unwrap();
        world.arena.release(key.frame()).unwrap();
        world
            .arena
            .link_frame(key.frame(), Some(primary), 0)
            .unwrap();
        world.arena.update_frame(key.frame()).unwrap();
        assert_eq!(world.arena.state(key.frame()).unwrap().update_count, 1);
        assert_eq!(
            world.arena.scene_occluder_ring(world.scene).unwrap(),
            (Some(key), 1, true, true)
        );
        world.close();
        assert!(world.arena.frame(key.frame()).is_err());
        assert_eq!(
            world.arena.scene_occluder_ring(world.scene).unwrap(),
            (None, 0, true, true)
        );
        assert_eq!(world.arena.occluder_storage().live_allocations(), 0);
        world.destroy();
        assert_eq!(world.arena.live_frame_count(), 0);
        assert!(world.driver.scenes().is_empty());
    }
    #[test]
    fn external_creator_reference_survives_close_and_releases_ring_while_scene_alive() {
        let mut world = World::new();
        let key = world.occluder();
        let primary = world.arena.scene(world.scene).unwrap().sectors[0].frame();
        world.arena.occluder_build(key, &V, &F).unwrap();
        world
            .arena
            .scene_add_frame(world.scene, key.frame())
            .unwrap();
        world
            .arena
            .link_frame(key.frame(), Some(primary), 0)
            .unwrap();
        world.close();
        assert_eq!(world.arena.frame(key.frame()).unwrap().references, 1);
        assert_eq!(world.arena.occluder(key).unwrap().scene, Some(world.scene));
        assert_eq!(world.arena.scene_occluder_ring(world.scene).unwrap().1, 1);
        world.arena.release(key.frame()).unwrap();
        assert_eq!(world.arena.scene_occluder_ring(world.scene).unwrap().1, 0);
        world.destroy();
        assert_eq!(world.arena.live_frame_count(), 0);
    }
    #[test]
    fn scene_ring_removal_retains_stale_members_and_reinsertion_appends() {
        let mut world = World::new();
        let a = world.occluder();
        let b = world.occluder();
        let c = world.occluder();
        let primary = world.arena.scene(world.scene).unwrap().sectors[0].frame();
        for key in [a, b, c] {
            world
                .arena
                .link_frame(key.frame(), Some(primary), 0)
                .unwrap();
        }
        assert_eq!(world.arena.occluder(a).unwrap().previous, Some(c));
        assert_eq!(world.arena.occluder(a).unwrap().next, Some(b));
        world.arena.link_frame(b.frame(), None, 0).unwrap();
        assert_eq!(world.arena.occluder(b).unwrap().scene, None);
        assert_eq!(world.arena.occluder(b).unwrap().previous, Some(a));
        assert_eq!(world.arena.occluder(b).unwrap().next, Some(c));
        world.arena.link_frame(b.frame(), Some(primary), 0).unwrap();
        assert_eq!(world.arena.occluder(a).unwrap().next, Some(c));
        assert_eq!(world.arena.occluder(c).unwrap().next, Some(b));
        assert_eq!(world.arena.occluder(b).unwrap().next, Some(a));
        assert_eq!(
            world
                .arena
                .link_frame(b.frame(), Some(primary), u32::MAX)
                .unwrap(),
            0
        );
        for key in [a, b, c] {
            world.arena.release(key.frame()).unwrap();
        }
        assert_eq!(world.arena.scene_occluder_ring(world.scene).unwrap().1, 0);
        world.destroy();
        assert_eq!(world.arena.live_frame_count(), 0);
    }
    #[test]
    fn base_link_propagates_hr_then_old_ancestor_bug_clears_it_despite_remaining_child() {
        let mut world = World::new();
        let a = world.occluder();
        let b = world.occluder();
        let primary = world.arena.scene(world.scene).unwrap().sectors[0].frame();
        world.arena.link_frame(a.frame(), Some(primary), 0).unwrap();
        world.arena.link_frame(b.frame(), Some(primary), 0).unwrap();
        assert_ne!(world.arena.state(primary).unwrap().flags & 0x100000, 0);
        world.arena.link_frame(a.frame(), None, 0).unwrap();
        assert_eq!(world.arena.state(primary).unwrap().flags & 0x100000, 0);
        assert_eq!(world.arena.links(primary).unwrap().child, Some(b.frame()));
        assert_eq!(world.arena.scene_occluder_ring(world.scene).unwrap().1, 1);
        world.arena.release(a.frame()).unwrap();
        world.arena.release(b.frame()).unwrap();
        world.destroy();
    }
    #[test]
    fn occluder_world_preserving_link_executes_real_hr_scene_tail() {
        let mut world = World::new();
        let key = world.occluder();
        let primary = world.arena.scene(world.scene).unwrap().sectors[0].frame();
        world.arena.occluder_build(key, &V, &F).unwrap();
        world
            .arena
            .graph
            .assign_position(key.frame(), [7., 8., 9.])
            .unwrap();
        world
            .arena
            .graph
            .assign_scale(key.frame(), [2., 2., 2.])
            .unwrap();
        world.arena.update_frame(key.frame()).unwrap();
        let before = world.arena.graph.world_matrix(key.frame()).unwrap();
        world
            .arena
            .link_frame(key.frame(), Some(primary), 1)
            .unwrap();
        assert_eq!(world.arena.graph.world_matrix(key.frame()).unwrap(), before);
        assert_eq!(world.arena.scene_occluder_ring(world.scene).unwrap().1, 1);
        world.arena.release(key.frame()).unwrap();
        world.destroy();
    }
    #[test]
    fn scene_owned_lights_unlink_on_real_close_and_external_light_survives_sector_teardown() {
        let mut world = World::new();
        let primary = world.arena.scene(world.scene).unwrap().sectors[0];
        let owned = world
            .driver
            .create_light_frame(&mut world.arena, &world.resources, world.scene)
            .unwrap();
        let external = world
            .driver
            .create_light_frame(&mut world.arena, &world.resources, world.scene)
            .unwrap();
        for light in [owned, external] {
            world.arena.graph.set_light_type(light, 1).unwrap();
            world.arena.scene_add_frame(world.scene, light).unwrap();
            world.arena.sector_add_light(primary, light).unwrap();
            world
                .arena
                .link_frame(light, Some(primary.frame()), 0)
                .unwrap();
        }
        world.arena.release(owned).unwrap();
        world.close();
        assert!(world.arena.frame(owned).is_err());
        assert_eq!(world.arena.frame(external).unwrap().references, 1);
        assert_eq!(world.arena.sector(primary).unwrap().lights, [external]);
        let changes = world.arena.sector_lights_changed(primary).unwrap();
        assert_eq!(changes, 3);
        world.destroy();
        assert_eq!(world.arena.live_frame_count(), 1);
        assert!(world.arena.light(external).unwrap().sectors.is_empty());
        assert_eq!(world.arena.links(external).unwrap().parent, None);
        world.arena.release(external).unwrap();
        assert_eq!(world.arena.live_frame_count(), 0);
    }
    #[test]
    fn parsed_occluder_suffix_uses_load_image_and_canonical_model_child_update() {
        let mut source = crate::model_loading::tests::frame(12, None, [2., 3., 4.]);
        source.occluder = Some(lhr_assets::OccluderRecord {
            vertices: V.to_vec(),
            faces: F.to_vec(),
        });
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let content = arena
            .load_parsed_into_model(
                model,
                std::sync::Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames: vec![source],
                }),
            )
            .unwrap();
        let frame = content.frames()[0];
        let key = arena.occluder_key(frame).unwrap();
        assert_eq!(arena.frame(frame).unwrap().references, 1);
        arena.update_frame(model.root()).unwrap();
        assert_eq!(
            arena.occluder(key).unwrap().word(0x5cc),
            Some(2.5f32.to_bits())
        );
        arena.release(model.root()).unwrap();
        assert_eq!(arena.occluder_storage().live_allocations(), 0);
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn native_ctor_unknown_padding_triangle_adjacency_and_exact_update_radius() {
        let mut arena = ModelArena::default();
        let key = arena.create_occluder().unwrap();
        assert_eq!(arena.frame(key.frame()).unwrap().references, 1);
        assert_eq!(arena.state(key.frame()).unwrap().flags, 0x4810003d);
        assert_eq!(arena.occluder(key).unwrap().word(0x39c), Some(0));
        assert_eq!(arena.occluder(key).unwrap().word(0x16c), None);
        assert_eq!(arena.occluder_build(key, &V, &F).unwrap(), 0);
        let frame = arena.occluder(key).unwrap();
        assert!(arena
            .occluder_storage()
            .bytes(frame.buffer(0x14c).unwrap())
            .is_err());
        assert!(arena
            .occluder_storage()
            .bytes(frame.buffer(0x164).unwrap())
            .is_err());
        assert_eq!(
            arena
                .occluder_storage()
                .bytes(frame.buffer(0x15c).unwrap())
                .unwrap(),
            [0, 0, 0, 0, 0, 0]
        );
        assert_eq!(
            arena
                .occluder_storage()
                .bytes(frame.buffer(0x160).unwrap())
                .unwrap(),
            [0, 1, 2]
        );
        assert_eq!(arena.occluder_storage().live_allocations(), 7);
        arena.occluder_update_v(key).unwrap();
        assert_eq!(arena.occluder(key).unwrap().word(0x5d8), Some(0x3f3504f3));
        assert_eq!(arena.occluder(key).unwrap().word(0x5dc), Some(0x3effffff));
        assert_eq!(
            words(&arena, key, 0x164),
            vec![0, 0, 0x3f800000, 0x80000000]
        );
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
        assert_eq!(arena.occluder_storage().live_allocations(), 0);
    }
    #[test]
    fn original_nonmanifold_edge_cycle_and_self_edges_are_not_pair_saturated() {
        let mut arena = ModelArena::default();
        let key = arena.create_occluder().unwrap();
        assert_eq!(
            arena
                .occluder_build(key, &V, &[[0, 1, 2], [1, 0, 2], [0, 1, 2]])
                .unwrap(),
            0
        );
        let faces = arena
            .occluder_storage()
            .bytes(arena.occluder(key).unwrap().buffer(0x15c).unwrap())
            .unwrap();
        assert_eq!([faces[0], faces[6], faces[12]], [1, 2, 0]);
        arena.release(key.frame()).unwrap();
        let key = arena.create_occluder().unwrap();
        assert_eq!(arena.occluder_build(key, &V, &[[0, 0, 1]]).unwrap(), 0);
        assert_eq!(
            arena
                .occluder_storage()
                .bytes(arena.occluder(key).unwrap().buffer(0x160).unwrap())
                .unwrap(),
            [0, 2, 1]
        );
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.occluder_storage().live_allocations(), 0);
    }
    #[test]
    fn eight_native_allocation_failure_prefixes_keep_members_and_last_release_frees() {
        for failure in 1..=8 {
            let mut arena = ModelArena::default();
            let key = arena.create_occluder().unwrap();
            arena
                .occluder_storage_mut()
                .fail_on_allocation(Some(failure));
            assert_eq!(arena.occluder_build(key, &V, &F).unwrap(), 0x80000002);
            assert_eq!(
                arena.occluder(key).unwrap().word(0x140),
                Some(if failure <= 3 { 0 } else { 3 })
            );
            assert_eq!(
                arena.occluder(key).unwrap().word(0x144),
                Some(if failure <= 3 { 0 } else { 1 })
            );
            assert_eq!(
                arena.occluder_storage().allocation_calls(),
                if failure <= 3 { 3 } else { 8 }
            );
            assert_eq!(arena.occluder_storage().diagnostics().len(), 1);
            assert!(!arena.occluder_storage().diagnostics()[0].native_diagnostic_backend_executed);
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.occluder_storage().live_allocations(), 0);
            assert_eq!(arena.live_frame_count(), 0);
        }
    }
    #[test]
    fn repeated_build_preserves_native_unreleased_auxiliary_allocations() {
        let mut arena = ModelArena::default();
        let key = arena.create_occluder().unwrap();
        arena.occluder_build(key, &V, &F).unwrap();
        arena.occluder_build(key, &V, &F).unwrap();
        assert_eq!(arena.occluder_storage().live_allocations(), 11);
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.occluder_storage().live_allocations(), 4);
        assert_eq!(arena.occluder_storage().live_bytes(), 27);
    }
    #[test]
    fn foreign_arena_keys_reject_before_effect_and_copy_accepts_invalid_update_indices() {
        let mut a = ModelArena::default();
        let key = a.create_occluder().unwrap();
        let mut b = ModelArena::default();
        let other = b.create_occluder().unwrap();
        assert!(matches!(
            b.occluder_build(key, &V, &F),
            Err(ModelError::Occluder(OccluderError::ForeignArena))
        ));
        assert_eq!(b.occluder_storage().allocation_calls(), 0);
        assert_eq!(a.occluder_build(key, &V, &[[0, 1, 65535]]).unwrap(), 0);
        assert!(a.occluder_update_v(key).is_err());
        assert!(a
            .occluder_storage()
            .bytes(a.occluder(key).unwrap().buffer(0x14c).unwrap())
            .is_ok());
        a.release(key.frame()).unwrap();
        b.release(other.frame()).unwrap();
        assert_eq!(a.occluder_storage().live_allocations(), 0);
    }
    #[test]
    fn load_image_failure_cleans_copy_arrays_and_sets_sticky_error_distinct_from_build() {
        let mut arena = ModelArena::default();
        let key = arena.create_occluder().unwrap();
        let mut image_error = 0x12345678;
        arena.occluder_storage_mut().fail_on_allocation(Some(2));
        arena
            .occluder_load_geometry(key, &V, &F, &mut image_error)
            .unwrap();
        assert_eq!(image_error, 0x80000002);
        assert_eq!(arena.occluder(key).unwrap().word(0x140), Some(0));
        assert_eq!(arena.occluder_storage().live_allocations(), 0);
        arena.release(key.frame()).unwrap();
    }
}
