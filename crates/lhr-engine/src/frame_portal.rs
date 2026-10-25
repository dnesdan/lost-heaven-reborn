//! Original I3D_portal constructor/LoadImage/rings/destructor.
//! LS3DF SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs41800/458f0/41890/41850; actual PS2 names retained privately.
//! Portals are separate owned objects, never graph FrameKeys.
use crate::frame_transform::FrameKey;
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::{SceneKey, SectorKey};
use crate::script_scheduler::FrameReferenceId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortalKey {
    domain: FrameReferenceId,
    index: u32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PortalError {
    ForeignArena,
    Unknown(PortalKey),
    Extent,
    Allocation,
    NativeAllocationNull,
    ReaderUnavailable,
    UnsafeVertexCount(u32),
    NumericUnavailable,
    MissingSector,
    InvalidRing,
}
impl From<PortalError> for ModelError {
    fn from(e: PortalError) -> Self {
        Self::Portal(e)
    }
}
#[derive(Clone, Debug)]
pub struct PortalFrame {
    pub key: PortalKey,
    pub(crate) bytes: Vec<u8>,
    pub sector: Option<SectorKey>,
    pub connected_sector: Option<SectorKey>,
    pub twin: Option<PortalKey>,
    pub scene: Option<SceneKey>,
    pub previous: Option<PortalKey>,
    pub next: Option<PortalKey>,
    alive: bool,
}
impl PortalFrame {
    pub fn vertex_count(&self) -> u32 {
        self.word(4)
    }
    pub fn flags(&self) -> u32 {
        self.word(8)
    }
    fn word(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().unwrap())
    }
    pub(crate) fn put(&mut self, at: usize, word: u32) {
        self.bytes[at..at + 4].copy_from_slice(&word.to_le_bytes());
    }
    /// Geometry/untouched storage observation only. Typed pointers deliberately
    /// have no invented native addresses; non-null pointer words are unknown.
    pub fn known_word(&self, at: usize) -> Option<u32> {
        if !at.is_multiple_of(4) || at.checked_add(4)? > self.bytes.len() {
            return None;
        }
        let unknown = match at {
            0xf4 => self.sector.is_some(),
            0xf8 => self.connected_sector.is_some(),
            0xfc => self.twin.is_some(),
            0x100 => self.previous.is_some(),
            0x104 => self.next.is_some(),
            0x10c => self.scene.is_some(),
            _ => false,
        };
        (!unknown).then(|| self.word(at))
    }
    pub fn local_vertices(&self) -> Result<Vec<[u32; 3]>, PortalError> {
        let count = self.vertex_count();
        if count > 8 {
            return Err(PortalError::UnsafeVertexCount(count));
        }
        Ok((0..count as usize)
            .map(|i| std::array::from_fn(|j| self.word(0x24 + i * 12 + j * 4)))
            .collect())
    }
}
impl ModelArena {
    /// Controlled original allocation-returning-NULL domain, independent of
    /// Rust metadata/heap errors. The constructor has not run on that branch.
    pub fn fail_portal_allocation(&mut self, call: Option<usize>) {
        self.portal_fail_on_call = call;
    }
    pub fn portal_allocation_calls(&self) -> usize {
        self.portal_allocation_calls
    }
    pub fn global_portal_version(&self) -> u32 {
        self.portal_version
    }
    pub(crate) fn portal_index(&self, key: PortalKey) -> Result<usize, ModelError> {
        if key.domain != self.lookup_reference(FrameKey(0)) {
            return Err(PortalError::ForeignArena.into());
        }
        let index = key.index.checked_sub(1).ok_or(PortalError::Unknown(key))? as usize;
        self.portal_frames
            .get(index)
            .filter(|p| p.alive && p.key == key)
            .map(|_| index)
            .ok_or_else(|| PortalError::Unknown(key).into())
    }
    pub fn portal(&self, key: PortalKey) -> Result<&PortalFrame, ModelError> {
        Ok(&self.portal_frames[self.portal_index(key)?])
    }
    pub fn live_portal_allocations(&self) -> usize {
        self.portal_frames.iter().filter(|p| p.alive).count()
    }
    /// Caller-owned allocation seed preserves every unwritten constructor byte.
    pub fn create_portal(&mut self, seed: [u8; 0x150]) -> Result<PortalKey, ModelError> {
        self.portal_allocation_calls = self
            .portal_allocation_calls
            .checked_add(1)
            .ok_or(PortalError::Extent)?;
        if self.portal_fail_on_call == Some(self.portal_allocation_calls) {
            return Err(PortalError::NativeAllocationNull.into());
        }

        let index = u32::try_from(self.portal_frames.len() + 1).map_err(|_| PortalError::Extent)?;
        let key = PortalKey {
            domain: self.lookup_reference(FrameKey(0)),
            index,
        };
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(seed.len())
            .map_err(|_| PortalError::Allocation)?;
        bytes.extend(seed);
        self.portal_frames
            .try_reserve(1)
            .map_err(|_| PortalError::Allocation)?;
        let mut portal = PortalFrame {
            key,
            bytes,
            sector: None,
            connected_sector: None,
            twin: None,
            scene: None,
            previous: None,
            next: None,
            alive: true,
        };
        for at in [4, 0xf4, 0xf8, 0x10c, 0x100, 0x104, 0xfc, 0xc] {
            portal.put(at, 0);
        }
        portal.bytes[0x108] = 0;
        portal.put(0, 0x1009c738);
        portal.put(8, 4);
        self.portal_version = self.portal_version.wrapping_add(1);
        self.portal_frames.push(portal);
        Ok(key)
    }
    /// Exact bounded read order: zero countDWORD, read countbyte, then bulk
    /// 28+12*count at+8. Short host data preserves the already written prefix.
    /// Counts above8 remain an explicit fixed-array admission boundary.
    pub fn portal_load_image(&mut self, key: PortalKey, bytes: &[u8]) -> Result<usize, ModelError> {
        let i = self.portal_index(key)?;
        self.portal_frames[i].put(4, 0);
        let count = *bytes.first().ok_or(PortalError::ReaderUnavailable)?;
        self.portal_frames[i].bytes[4] = count;
        if count > 8 {
            return Err(PortalError::UnsafeVertexCount(u32::from(count)).into());
        }
        let size = 28 + usize::from(count) * 12;
        let copied = size.min(bytes.len() - 1);
        self.portal_frames[i].bytes[8..8 + copied].copy_from_slice(&bytes[1..1 + copied]);
        if copied != size {
            return Err(PortalError::ReaderUnavailable.into());
        }
        Ok(size + 1)
    }
    pub(crate) fn portal_add_to_scene_ring(
        &mut self,
        key: PortalKey,
        new_scene: Option<SceneKey>,
    ) -> Result<(), ModelError> {
        let i = self.portal_index(key)?;
        let old = self.portal_frames[i].scene;
        if old == new_scene {
            return Ok(());
        }
        if let Some(scene) = new_scene {
            self.scene_index(scene)?;
        }
        if let Some(scene) = old {
            let si = self.scene_index(scene)?;
            let previous = self.portal_frames[i]
                .previous
                .ok_or(PortalError::InvalidRing)?;
            let next = self.portal_frames[i].next.ok_or(PortalError::InvalidRing)?;
            let pi = self.portal_index(previous)?;
            let ni = self.portal_index(next)?;
            self.scenes[si].portal_count = self.scenes[si].portal_count.wrapping_sub(1);
            if self.scenes[si].portal_head == Some(key) {
                self.scenes[si].portal_head = Some(next);
            }
            if previous == key {
                self.scenes[si].portal_head = None;
            } else {
                self.portal_frames[pi].next = Some(next);
                self.portal_frames[ni].previous = Some(previous);
            }
        }
        self.portal_frames[i].scene = new_scene;
        if let Some(scene) = new_scene {
            let si = self.scene_index(scene)?;
            if let Some(head) = self.scenes[si].portal_head {
                let hi = self.portal_index(head)?;
                let previous = self.portal_frames[hi]
                    .previous
                    .ok_or(PortalError::InvalidRing)?;
                let pi = self.portal_index(previous)?;
                self.portal_frames[pi].next = Some(key);
                self.portal_frames[i].previous = Some(previous);
                self.portal_frames[i].next = Some(head);
                self.portal_frames[hi].previous = Some(key);
            } else {
                self.portal_frames[i].previous = Some(key);
                self.portal_frames[i].next = Some(key);
                self.scenes[si].portal_head = Some(key);
            }
            self.scenes[si].portal_count = self.scenes[si].portal_count.wrapping_add(1);
        }
        Ok(())
    }
    /// Preserve the original quirk: destructor calls ring(CURRENT scene), which
    /// does not unlink. Sector teardown must unregister portals beforehand.
    pub fn delete_portal(&mut self, key: PortalKey) -> Result<(), ModelError> {
        let i = self.portal_index(key)?;
        self.portal_version = self.portal_version.wrapping_add(1);
        self.portal_add_to_scene_ring(key, self.portal_frames[i].scene)?;
        if let Some(twin) = self.portal_frames[i].twin {
            let ti = self.portal_index(twin)?;
            self.portal_frames[ti].twin = None;
            self.portal_frames[ti].put(0xfc, 0);
            self.portal_frames[i].twin = None;
            self.portal_frames[i].put(0xfc, 0);
        }
        self.portal_frames[i].alive = false;
        self.portal_frames[i].bytes.clear();
        self.portal_frames[i].bytes.shrink_to_fit();
        Ok(())
    }
}

impl ModelArena {
    pub(crate) fn portal_update_with_graph(
        &mut self,
        graph: &mut crate::frame_transform::I3dFrameGraph,
        key: PortalKey,
    ) -> Result<(), ModelError> {
        use crate::frame_sector::math;
        let i = self.portal_index(key)?;
        let sector = self.portal_frames[i]
            .sector
            .ok_or(PortalError::MissingSector)?;
        let count = self.portal_frames[i].vertex_count();
        if !(3..=8).contains(&count) {
            return Err(PortalError::UnsafeVertexCount(count).into());
        }
        let matrix = graph.world_matrix(sector.frame())?;
        let mut min = [1_000_000.; 3];
        let mut max = [-1_000_000.; 3];
        let mut world = Vec::new();
        for index in 0..count as usize {
            let vector = std::array::from_fn(|axis| {
                f32::from_bits(self.portal_frames[i].word(0x24 + index * 12 + axis * 4))
            });
            let transformed = math::transformed(&matrix, vector)?;
            for axis in 0..3 {
                self.portal_frames[i]
                    .put(0x84 + index * 12 + axis * 4, transformed[axis].to_bits());
                if min[axis] > transformed[axis] {
                    min[axis] = transformed[axis];
                }
                if max[axis] < transformed[axis] {
                    max[axis] = transformed[axis];
                }
            }
            world.push(transformed);
        }
        for axis in 0..3 {
            let center = math::mul(math::add(max[axis], min[axis])?, 0.5)?;
            self.portal_frames[i].put(0x110 + axis * 4, center.to_bits());
        }
        let difference = [
            math::sub(max[0], min[0])?,
            math::sub(max[1], min[1])?,
            math::sub(max[2], min[2])?,
        ];
        let radius = math::mul(math::magnitude(difference)?, 0.5)?;
        self.portal_frames[i].put(0x11c, radius.to_bits());
        let plane = math::plane(world[world.len() - 1], world[0], world[1])?;
        for (axis, value) in plane.into_iter().enumerate() {
            self.portal_frames[i].put(0xe4 + axis * 4, value.to_bits());
        }
        self.portal_add_to_scene_ring(key, self.sector(sector)?.scene)?;
        Ok(())
    }
    pub fn update_portal(&mut self, key: PortalKey) -> Result<(), ModelError> {
        let mut graph = std::mem::take(&mut self.graph);
        let result = self.portal_update_with_graph(&mut graph, key);
        self.graph = graph;
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constructor_preserves_unwritten_bytes_and_load_retains_short_prefix() {
        let mut arena = ModelArena::default();
        let p = arena.create_portal([0xa5; 0x150]).unwrap();
        assert_eq!(arena.global_portal_version(), 1);
        let portal = arena.portal(p).unwrap();
        assert_eq!(portal.known_word(0), Some(0x1009c738));
        assert_eq!(portal.vertex_count(), 0);
        assert_eq!(portal.flags(), 4);
        assert_eq!(portal.known_word(0x110), Some(0xa5a5a5a5));
        assert_eq!(portal.bytes[0x108], 0);
        assert_eq!(&portal.bytes[0x109..0x10c], &[0xa5; 3]);
        let error = arena.portal_load_image(p, &[4, 1, 2, 3]).unwrap_err();
        assert_eq!(error, ModelError::Portal(PortalError::ReaderUnavailable));
        assert_eq!(arena.portal(p).unwrap().vertex_count(), 4);
        assert_eq!(&arena.portal(p).unwrap().bytes[8..12], &[1, 2, 3, 0]);
        let mut bytes = vec![4];
        bytes.extend((0..76).map(|n| n as u8));
        assert_eq!(arena.portal_load_image(p, &bytes).unwrap(), 77);
        assert_eq!(arena.portal(p).unwrap().local_vertices().unwrap().len(), 4);
        arena.delete_portal(p).unwrap();
        assert_eq!(arena.global_portal_version(), 2);
        assert_eq!(arena.live_portal_allocations(), 0);
    }
    #[test]
    fn portal_keys_are_not_frame_keys_or_cross_arena_aliases() {
        let mut first = ModelArena::default();
        let mut second = ModelArena::default();
        let p = first.create_portal([0; 0x150]).unwrap();
        let other = second.create_portal([0; 0x150]).unwrap();
        assert_eq!(first.live_frame_count(), 0);
        assert_eq!(second.live_frame_count(), 0);
        assert_eq!(
            first.portal(other).unwrap_err(),
            ModelError::Portal(PortalError::ForeignArena)
        );
        first.delete_portal(p).unwrap();
        second.delete_portal(other).unwrap();
    }
    #[test]
    fn destructor_keeps_current_scene_ring_and_clears_only_twin_reciprocity() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let a = arena.create_portal([0; 0x150]).unwrap();
        let b = arena.create_portal([0; 0x150]).unwrap();
        let ai = arena.portal_index(a).unwrap();
        let bi = arena.portal_index(b).unwrap();
        arena.portal_frames[ai].twin = Some(b);
        arena.portal_frames[bi].twin = Some(a);
        arena.portal_add_to_scene_ring(a, Some(scene)).unwrap();
        let si = arena.scene_index(scene).unwrap();
        assert_eq!(arena.scenes[si].portal_head, Some(a));
        assert_eq!(arena.scenes[si].portal_count, 1);
        arena.delete_portal(a).unwrap();
        assert_eq!(arena.scenes[si].portal_head, Some(a));
        assert_eq!(arena.scenes[si].portal_count, 1);
        assert_eq!(arena.portal(b).unwrap().twin, None);
        assert_eq!(arena.portal(b).unwrap().known_word(0xfc), Some(0));
        assert_eq!(arena.global_portal_version(), 3);
        arena.delete_portal(b).unwrap();
        assert_eq!(arena.live_portal_allocations(), 0); /* The original destructor intentionally leaves this dangling borrowed scene ring; no further ring traversal or fullScene destructor is claimed. */
    }
}
