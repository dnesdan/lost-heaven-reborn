//! Original loaded I3D_sector CPU ownership, LS3DF e211c850...178a6.
//! RVAs4ed30/505c0/4f260/4ee80; preparation/math methods follow separately.
use crate::frame_portal::{PortalError, PortalKey};
use crate::frame_transform::{FrameKind, I3dFrameState};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::{SceneKey, SectorFrame, SectorKey};
#[path = "frame_sector_math.rs"]
pub(crate) mod math;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectorError {
    Extent,
    Allocation,
    Buffer,
    AlreadyLoaded,
    MissingRecord,
    GeneratedOccluderUnavailable,
    NumericUnavailable,
    InvalidRing,
    MissingScene,
    Interrupted,
}
impl From<SectorError> for ModelError {
    fn from(e: SectorError) -> Self {
        Self::Sector(e)
    }
}
/// SCTF_VISIBLE 0x1 (mafia-re ls3df/I3D/I3D.h:902): the portal walk found the
/// sector in the view depth range this frame.
pub const SCTF_VISIBLE: u32 = 0x01;
/// SCTF_PREPARED 0x4 (mafia-re ls3df/I3D/I3D.h:905): MakeReady ran, so the
/// sector owns its hull planes.
pub const SCTF_PREPARED: u32 = 0x04;
/// Upstream working name, not an evidenced original symbol: direct read of
/// the SCTF_VISIBLE bit. The reference records no LS3DF FUNCTION address for
/// a standalone reader (I3D.h:902; set by the portal walk). Reports whether
/// the sector was found in view this frame; no state moves.
pub fn sector_is_visible(sector_flags: u32) -> bool {
    sector_flags & SCTF_VISIBLE != 0
}
/// Upstream working name, not an evidenced original symbol: direct read of
/// the SCTF_PREPARED bit. The reference records no LS3DF FUNCTION address
/// for a standalone reader (I3D.h:905; set by `MakeReady`, mafia-re
/// ls3df/I3D/frames/I3d_sector.cpp:333-348). Reports whether the sector hull
/// is prepared; no state moves.
pub fn sector_is_prepared(sector_flags: u32) -> bool {
    sector_flags & SCTF_PREPARED != 0
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SectorBufferKey {
    domain: u64,
    index: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SectorAllocationKind {
    Vertices,
    Faces,
    Planes,
    OutgoingPortals,
    IncomingPortals,
    TemporaryPlanes,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SectorStorageEvent {
    Allocate {
        kind: SectorAllocationKind,
        bytes: usize,
        key: Option<SectorBufferKey>,
    },
    Free(Option<SectorBufferKey>),
}
#[derive(Debug)]
struct Buffer {
    key: SectorBufferKey,
    bytes: Vec<u8>,
    alive: bool,
}
#[derive(Debug)]
pub struct SectorStorage {
    domain: u64,
    buffers: Vec<Buffer>,
    calls: usize,
    fail_on: Option<usize>,
    events: Vec<SectorStorageEvent>,
}
impl Default for SectorStorage {
    fn default() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            domain: NEXT.fetch_add(1, Ordering::Relaxed),
            buffers: Vec::new(),
            calls: 0,
            fail_on: None,
            events: Vec::new(),
        }
    }
}
impl SectorStorage {
    pub fn fail_on_allocation(&mut self, call: Option<usize>) {
        self.fail_on = call;
    }
    pub fn allocation_calls(&self) -> usize {
        self.calls
    }
    pub fn live_allocations(&self) -> usize {
        self.buffers.iter().filter(|b| b.alive).count()
    }
    pub fn live_bytes(&self) -> usize {
        self.buffers
            .iter()
            .filter(|b| b.alive)
            .map(|b| b.bytes.len())
            .sum()
    }
    pub fn events(&self) -> &[SectorStorageEvent] {
        &self.events
    }
    pub(crate) fn allocate(
        &mut self,
        kind: SectorAllocationKind,
        bytes: usize,
    ) -> Result<Option<SectorBufferKey>, ModelError> {
        self.calls = self.calls.checked_add(1).ok_or(SectorError::Extent)?;
        if self.fail_on == Some(self.calls) {
            self.events.push(SectorStorageEvent::Allocate {
                kind,
                bytes,
                key: None,
            });
            return Ok(None);
        }
        let mut data = Vec::new();
        data.try_reserve_exact(bytes)
            .map_err(|_| SectorError::Allocation)?;
        data.resize(bytes, 0);
        let key = SectorBufferKey {
            domain: self.domain,
            index: self.buffers.len(),
        };
        self.buffers
            .try_reserve(1)
            .map_err(|_| SectorError::Allocation)?;
        self.buffers.push(Buffer {
            key,
            bytes: data,
            alive: true,
        });
        self.events.push(SectorStorageEvent::Allocate {
            kind,
            bytes,
            key: Some(key),
        });
        Ok(Some(key))
    }
    pub(crate) fn bytes(&self, key: SectorBufferKey) -> Result<&[u8], ModelError> {
        self.buffers
            .get(key.index)
            .filter(|b| b.key == key && b.alive)
            .map(|b| b.bytes.as_slice())
            .ok_or_else(|| SectorError::Buffer.into())
    }
    pub(crate) fn write(
        &mut self,
        key: SectorBufferKey,
        offset: usize,
        data: &[u8],
    ) -> Result<(), ModelError> {
        let end = offset.checked_add(data.len()).ok_or(SectorError::Extent)?;
        let buffer = self
            .buffers
            .get_mut(key.index)
            .filter(|b| b.key == key && b.alive)
            .ok_or(SectorError::Buffer)?;
        buffer
            .bytes
            .get_mut(offset..end)
            .ok_or(SectorError::Extent)?
            .copy_from_slice(data);
        Ok(())
    }
    pub(crate) fn free(&mut self, key: Option<SectorBufferKey>) -> Result<(), ModelError> {
        self.events.push(SectorStorageEvent::Free(key));
        if let Some(key) = key {
            let b = self
                .buffers
                .get_mut(key.index)
                .filter(|b| b.key == key && b.alive)
                .ok_or(SectorError::Buffer)?;
            b.alive = false;
            b.bytes.clear();
            b.bytes.shrink_to_fit();
        }
        Ok(())
    }
}
/// Geometry buffers have one central owner. Clone is an inspection snapshot of
/// keys, not an extra native allocation or resource reference.
#[derive(Clone, Debug, Default)]
pub struct SectorGeometry {
    pub group_bits: u32,
    pub vertex_count: u32,
    pub face_count: u32,
    pub plane_count: u32,
    pub(crate) vertices: Option<SectorBufferKey>,
    pub(crate) faces: Option<SectorBufferKey>,
    pub(crate) planes: Option<SectorBufferKey>,
    pub portals: Vec<PortalKey>,
    pub incoming_portals: Vec<PortalKey>,
    pub(crate) outgoing_buffer: Option<SectorBufferKey>,
    pub(crate) outgoing_capacity: usize,
    pub(crate) incoming_buffer: Option<SectorBufferKey>,
    pub(crate) incoming_capacity: usize,
    pub previous: Option<SectorKey>,
    pub next: Option<SectorKey>,
    pub(crate) update_interrupted: bool,
    pub(crate) destructor_entered: bool,
}
fn admit_sector_record(record: &lhr_assets::SectorRecord) -> Result<(), ModelError> {
    if record.portals.len() > u8::MAX as usize {
        return Err(SectorError::Extent.into());
    }
    for portal in &record.portals {
        if portal.vertices.len() > 8 {
            return Err(PortalError::UnsafeVertexCount(
                u32::try_from(portal.vertices.len()).unwrap_or(u32::MAX),
            )
            .into());
        }
    }
    Ok(())
}
impl ModelArena {
    pub fn sector_storage(&self) -> &SectorStorage {
        &self.sector_storage
    }
    pub fn sector_storage_mut(&mut self) -> &mut SectorStorage {
        &mut self.sector_storage
    }
    pub(crate) fn sector_index(&self, key: SectorKey) -> Result<usize, ModelError> {
        self.sector(key)?;
        self.sectors
            .iter()
            .position(|s| s.key == key)
            .ok_or_else(|| SectorError::Buffer.into())
    }
    pub fn create_sector_frame(&mut self) -> Result<SectorKey, ModelError> {
        self.sectors
            .try_reserve(1)
            .map_err(|_| SectorError::Allocation)?;
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Sector;
        state.flags |= 0x08080000;
        let key = SectorKey(self.insert(state, None, Vec::new(), Vec::new(), None)?);
        Ok(self.finish_sector_constructor(key))
    }
    pub(crate) fn finish_sector_constructor(&mut self, key: SectorKey) -> SectorKey {
        let mut sound_properties = [0; crate::scene_weather::SECTOR_SOUND_PROPERTIES_WORDS];
        crate::scene_weather::setup_sector_sound_properties_defaults(&mut sound_properties);
        self.sectors.push(SectorFrame {
            key,
            scene: None,
            sector_flags: 0x801,
            bounds: None,
            bounding_sphere: None,
            next_visual: None,
            lights: Vec::new(),
            num_fog_lights: 0,
            fog_light: None,
            pending_light_add: None,
            sounds: Vec::new(),
            hull_occluder: None,
            occluder_list: None,
            sound_properties,
            sound_words_1e8_1ec: [0; 2],
            geometry: SectorGeometry::default(),
        });
        self.sector_version = self.sector_version.wrapping_add(1);
        key
    }
    /// Descriptive bbox/sphere setter, PC RVA1ce80; no standalone original PS2
    /// name is claimed. Invalid/unordered X preserves the old center; admitted
    /// PC24 arithmetic follows the original half/center/radius store order.
    pub fn set_frame_bound_volume(
        &mut self,
        key: crate::frame_transform::FrameKey,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        if bbox[0][0] <= bbox[1][0] {
            self.frames[fi].hr_bounds.bbox = bbox;
            let delta = [
                math::sub(bbox[1][0], bbox[0][0])?,
                math::sub(bbox[1][1], bbox[0][1])?,
                math::sub(bbox[1][2], bbox[0][2])?,
            ];
            let half = [
                math::mul(delta[0], 0.5)?,
                math::mul(delta[1], 0.5)?,
                math::mul(delta[2], 0.5)?,
            ];
            for axis in 0..3 {
                self.frames[fi].hr_bounds.center[axis] = math::add(bbox[0][axis], half[axis])?;
            }
            self.frames[fi].hr_bounds.radius = math::sqrt(math::squared(half)?)?;
        } else {
            self.frames[fi].hr_bounds.bbox = crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR.bbox;
            self.frames[fi].hr_bounds.radius = 0.;
        }
        self.graph.projected_state_mut(key)?.flags = (self.state(key)?.flags & !0x100) | 0x40;
        Ok(())
    }
    /// Parsed data has already been fully retained/validated for source extents.
    /// This does not emulate native partial DTA reads. Allocation ordering and
    /// owned successful prefixes follow505c0; ImageErr is returned separately.
    pub fn load_sector_record(
        &mut self,
        key: SectorKey,
        record: &lhr_assets::SectorRecord,
    ) -> Result<Option<u32>, ModelError> {
        admit_sector_record(record)?;
        let i = self.sector_index(key)?;
        if self.sectors[i].geometry.vertices.is_some()
            || self.sectors[i].geometry.faces.is_some()
            || !self.sectors[i].geometry.portals.is_empty()
        {
            return Err(SectorError::AlreadyLoaded.into());
        }
        let nv = u32::try_from(record.vertices.len()).map_err(|_| SectorError::Extent)?;
        let nf = u32::try_from(record.faces.len()).map_err(|_| SectorError::Extent)?;
        self.sectors[i].sector_flags = record.flags;
        self.sectors[i].geometry.group_bits = record.group;
        self.sectors[i].geometry.vertex_count = nv;
        self.sectors[i].geometry.face_count = nf;
        let vertices = self.sector_storage.allocate(
            SectorAllocationKind::Vertices,
            record
                .vertices
                .len()
                .checked_mul(12)
                .ok_or(SectorError::Extent)?,
        )?;
        self.sectors[i].geometry.vertices = vertices;
        let faces = self.sector_storage.allocate(
            SectorAllocationKind::Faces,
            record
                .faces
                .len()
                .checked_mul(6)
                .ok_or(SectorError::Extent)?,
        )?;
        self.sectors[i].geometry.faces = faces;
        let (Some(vertices), Some(faces)) = (vertices, faces) else {
            self.sector_storage.free(vertices)?;
            self.sector_storage.free(faces)?;
            self.sectors[i].geometry.vertices = None;
            self.sectors[i].geometry.faces = None;
            return Ok(Some(0x80000002));
        };
        for (at, vector) in record.vertices.iter().enumerate() {
            for (axis, value) in vector.iter().enumerate() {
                self.sector_storage.write(
                    vertices,
                    at * 12 + axis * 4,
                    &value.to_bits().to_le_bytes(),
                )?;
            }
        }
        for (at, triangle) in record.faces.iter().enumerate() {
            for (axis, value) in triangle.iter().enumerate() {
                self.sector_storage
                    .write(faces, at * 6 + axis * 2, &value.to_le_bytes())?;
            }
        }
        self.set_frame_bound_volume(key.frame(), record.bounds)?;
        let bounds = self.frame(key.frame())?.hr_bounds;
        self.sectors[i].bounds = Some(bounds.bbox);
        self.sectors[i].bounding_sphere = Some([
            bounds.center[0],
            bounds.center[1],
            bounds.center[2],
            bounds.radius,
        ]);
        let count = record
            .portals
            .len()
            .max(self.sectors[i].geometry.portals.len());
        if count > self.sectors[i].geometry.outgoing_capacity {
            self.sectors[i].geometry.outgoing_capacity = count;
            let allocated = self
                .sector_storage
                .allocate(
                    SectorAllocationKind::OutgoingPortals,
                    count.checked_mul(4).ok_or(SectorError::Extent)?,
                )?
                .ok_or(SectorError::Allocation)?;
            self.sector_storage
                .free(self.sectors[i].geometry.outgoing_buffer)?;
            self.sectors[i].geometry.outgoing_buffer = Some(allocated);
        }
        for record in &record.portals {
            let portal = match self.create_portal([0; 0x150]) {
                Ok(p) => p,
                Err(ModelError::Portal(PortalError::NativeAllocationNull)) => {
                    return Ok(Some(0x80000002))
                }
                Err(e) => return Err(e),
            };
            let pi = self.portal_index(portal)?;
            self.portal_frames[pi].sector = Some(key);
            let count = u8::try_from(record.vertices.len()).map_err(|_| SectorError::Extent)?;
            let mut data = vec![count];
            data.extend(record.prefix);
            for vector in &record.vertices {
                for value in vector {
                    data.extend(value.to_bits().to_le_bytes());
                }
            }
            self.portal_load_image(portal, &data)?;
            // Reserve above guarantees append has capacity for every source
            // record. Reentrant mutation is outside this parsed intake domain.
            self.sectors[i].geometry.portals.push(portal);
        }
        Ok(None)
    }
}

impl ModelArena {
    /// Original4f260 borrowed scene ring. Constructor primary/backdrop/omni/near
    /// sectors have a scene but no links, so changing those skips old removal.
    pub(crate) fn sector_add_to_scene_ring(
        &mut self,
        key: SectorKey,
        new: Option<SceneKey>,
    ) -> Result<(), ModelError> {
        let i = self.sector_index(key)?;
        let old = self.sectors[i].scene;
        if old == new {
            return Ok(());
        }
        if let Some(scene) = new {
            self.scene_index(scene)?;
        }
        if new.is_some() && self.sectors[i].hull_occluder.is_some() {
            return Err(SectorError::GeneratedOccluderUnavailable.into());
        }
        if let Some(scene) = old {
            if let Some(previous) = self.sectors[i].geometry.previous {
                let next = self.sectors[i]
                    .geometry
                    .next
                    .ok_or(SectorError::InvalidRing)?;
                let pi = self.sector_index(previous)?;
                let ni = self.sector_index(next)?;
                let si = self.scene_index(scene)?;
                self.scenes[si].sector_count = self.scenes[si].sector_count.wrapping_sub(1);
                if self.scenes[si].sector_head == Some(key) {
                    self.scenes[si].sector_head = Some(next);
                }
                if previous == key {
                    self.scenes[si].sector_head = None;
                } else {
                    self.sectors[pi].geometry.next = Some(next);
                    self.sectors[ni].geometry.previous = Some(previous);
                }
                self.scenes[si].portal_version = 0;
                self.sectors[i].scene = None;
            }
        }
        self.sectors[i].scene = new;
        if let Some(scene) = new {
            let si = self.scene_index(scene)?;
            if let Some(head) = self.scenes[si].sector_head {
                let hi = self.sector_index(head)?;
                let previous = self.sectors[hi]
                    .geometry
                    .previous
                    .ok_or(SectorError::InvalidRing)?;
                let pi = self.sector_index(previous)?;
                self.sectors[pi].geometry.next = Some(key);
                self.sectors[i].geometry.previous = Some(previous);
                self.sectors[i].geometry.next = Some(head);
                self.sectors[hi].geometry.previous = Some(key);
            } else {
                self.sectors[i].geometry.previous = Some(key);
                self.sectors[i].geometry.next = Some(key);
                self.scenes[si].sector_head = Some(key);
            }
            self.scenes[si].sector_count = self.scenes[si].sector_count.wrapping_add(1);
            self.scenes[si].portal_version = 0;
        }
        self.sector_version = self.sector_version.wrapping_add(1);
        for portal in self.sectors[i].geometry.portals.clone() {
            self.portal_add_to_scene_ring(portal, new)?;
        }
        Ok(())
    }
}

fn vector(bytes: &[u8], index: usize) -> Result<[f32; 3], ModelError> {
    let at = index.checked_mul(12).ok_or(SectorError::Extent)?;
    let data = bytes.get(at..at + 12).ok_or(SectorError::Extent)?;
    Ok(std::array::from_fn(|axis| {
        f32::from_bits(u32::from_le_bytes(
            data[axis * 4..axis * 4 + 4].try_into().unwrap(),
        ))
    }))
}
fn plane_bytes(plane: [f32; 4]) -> [u8; 16] {
    let mut bytes = [0; 16];
    for (axis, value) in plane.into_iter().enumerate() {
        bytes[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_bits().to_le_bytes());
    }
    bytes
}
impl ModelArena {
    pub(crate) fn sector_update_with_graph(
        &mut self,
        graph: &mut crate::frame_transform::I3dFrameGraph,
        key: SectorKey,
    ) -> Result<(), ModelError> {
        let i = self.sector_index(key)?;
        if self.sectors[i].geometry.update_interrupted {
            return Err(SectorError::Interrupted.into());
        }
        let result = (|| {
            self.sector_update_planes_with_graph(graph, key)?;
            for portal in self.sectors[i].geometry.portals.clone() {
                self.portal_update_with_graph(graph, portal)?;
            }
            Ok(())
        })();
        if result.is_err() {
            self.sectors[i].geometry.update_interrupted = true;
        }
        result
    }
    fn sector_update_planes_with_graph(
        &mut self,
        graph: &mut crate::frame_transform::I3dFrameGraph,
        key: SectorKey,
    ) -> Result<(), ModelError> {
        let i = self.sector_index(key)?;
        if self.sectors[i].sector_flags & 4 == 0 {
            return Ok(());
        }
        if let Some(old) = self.sectors[i].geometry.planes {
            self.sector_storage.free(Some(old))?;
        }
        let matrix = graph.world_matrix(key.frame())?;
        let geometry = self.sectors[i].geometry.clone();
        let mut planes: Vec<[f32; 4]> = Vec::new();
        let mut temporary = None;
        let mut capacity = 0usize;
        for index in 0..geometry.face_count as usize {
            let faces = self
                .sector_storage
                .bytes(geometry.faces.ok_or(SectorError::Buffer)?)?;
            let face = faces
                .get(index * 6..index * 6 + 6)
                .ok_or(SectorError::Extent)?;
            let indices: [usize; 3] = std::array::from_fn(|n| {
                u16::from_le_bytes(face[n * 2..n * 2 + 2].try_into().unwrap()) as usize
            });
            let vertices = self
                .sector_storage
                .bytes(geometry.vertices.ok_or(SectorError::Buffer)?)?;
            let a = math::transformed(&matrix, vector(vertices, indices[0])?)?;
            let b = math::transformed(&matrix, vector(vertices, indices[1])?)?;
            let c = math::transformed(&matrix, vector(vertices, indices[2])?)?;
            let plane = math::plane(a, b, c)?;
            let mut duplicate = false;
            for old in &planes {
                let mut same = true;
                for axis in [3, 0, 1, 2] {
                    if f64::from(math::sub(old[axis], plane[axis])?.abs())
                        >= f64::from_bits(0x3f50624dd2f1a9fc)
                    {
                        same = false;
                        break;
                    }
                }
                if same {
                    duplicate = true;
                    break;
                }
            }
            if duplicate {
                continue;
            }
            if planes.len() == capacity {
                capacity = capacity.checked_add(8).ok_or(SectorError::Extent)?;
                let buffer = self
                    .sector_storage
                    .allocate(SectorAllocationKind::TemporaryPlanes, capacity * 16)?
                    .ok_or(SectorError::Allocation)?;
                for (at, value) in planes.iter().enumerate() {
                    self.sector_storage
                        .write(buffer, at * 16, &plane_bytes(*value))?;
                }
                self.sector_storage.free(temporary)?;
                temporary = Some(buffer);
            }
            self.sector_storage.write(
                temporary.ok_or(SectorError::Buffer)?,
                planes.len() * 16,
                &plane_bytes(plane),
            )?;
            planes.push(plane);
        }
        self.sectors[i].geometry.plane_count =
            u32::try_from(planes.len()).map_err(|_| SectorError::Extent)?;
        let allocated = self
            .sector_storage
            .allocate(SectorAllocationKind::Planes, planes.len() * 16)?;
        self.sectors[i].geometry.planes = allocated;
        if let Some(buffer) = allocated {
            for (at, plane) in planes.into_iter().enumerate() {
                self.sector_storage
                    .write(buffer, at * 16, &plane_bytes(plane))?;
            }
        }
        self.sector_storage.free(temporary)?;
        Ok(())
    }
    pub fn sector_make_ready(&mut self, key: SectorKey) -> Result<(), ModelError> {
        let i = self.sector_index(key)?;
        // The native prepared bit is written before work begins. It does not
        // turn an incomplete host capability/error prefix into a completed call.
        if self.sectors[i].geometry.update_interrupted {
            return Err(SectorError::Interrupted.into());
        }
        if self.sectors[i].sector_flags & 4 != 0 {
            return Ok(());
        }
        self.sectors[i].sector_flags |= 4;
        if self.sectors[i].geometry.vertex_count == 0 {
            return Ok(());
        }
        if self.sectors[i].sector_flags & 0x40 != 0 {
            self.sectors[i].geometry.update_interrupted = true;
            return Err(SectorError::GeneratedOccluderUnavailable.into());
        }
        let mut graph = std::mem::take(&mut self.graph);
        let result = self.sector_update_with_graph(&mut graph, key);
        self.graph = graph;
        result
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x4a8c0, VA 0x1004a8c0.
    /// `I3D_scene::MakeReadySectors`' static `I3D_EnumProc` (mafia-re
    /// ls3df/I3D/I3d_scene.cpp:650-657), evidenced original symbol. Runs
    /// the sector `MakeReady` and returns `I3DENUMRET_OK` (0) so the
    /// enumeration continues; the native `context` word is unused.
    pub fn make_ready_sectors_enum_proc(&mut self, key: SectorKey) -> Result<u32, ModelError> {
        self.sector_make_ready(key)?;
        Ok(0)
    }
    pub fn sector_planes(&self, key: SectorKey) -> Result<Vec<[u32; 4]>, ModelError> {
        let s = self.sector(key)?;
        let Some(buffer) = s.geometry.planes else {
            return if s.geometry.plane_count == 0 {
                Ok(Vec::new())
            } else {
                Err(SectorError::Buffer.into())
            };
        };
        let bytes = self.sector_storage.bytes(buffer)?;
        Ok(bytes
            .as_chunks::<16>()
            .0
            .iter()
            .map(|raw| {
                std::array::from_fn(|i| {
                    u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap())
                })
            })
            .collect())
    }
}

impl ModelArena {
    pub(crate) fn check_sector_destruction_domain(&self, key: SectorKey) -> Result<(), ModelError> {
        let s = self.sector(key)?;
        if let Some(pending) = s.pending_light_add {
            return Err(
                crate::scene_ownership::SceneOwnerError::LightAddIncomplete {
                    sector: key,
                    light: pending.light,
                }
                .into(),
            );
        }
        let g = &s.geometry;
        if g.destructor_entered || g.update_interrupted {
            return Err(SectorError::Interrupted.into());
        }
        for buffer in [
            g.vertices,
            g.faces,
            g.planes,
            g.outgoing_buffer,
            g.incoming_buffer,
        ]
        .into_iter()
        .flatten()
        {
            self.sector_storage.bytes(buffer)?;
        }
        // Native unlink clears the scene while retaining stale ring links.
        // Only a still-registered sector needs a live borrowed scene here.
        if g.previous.is_some() {
            if let Some(scene) = s.scene {
                self.scene_index(scene)?;
            }
        }
        for &portal in &g.portals {
            let p = self.portal(portal)?;
            if p.sector != Some(key) {
                return Err(PortalError::MissingSector.into());
            }
            if let Some(twin) = p.twin {
                self.portal(twin)?;
            }
            if let Some(connected) = p.connected_sector {
                self.sector(connected)?;
            }
        }
        for &portal in &g.incoming_portals {
            self.portal(portal)?;
        }
        for &light in &s.lights {
            self.graph.light_state(light)?;
        }
        for &sound in &s.sounds {
            self.sound_key(sound)?;
        }
        Ok(())
    }
    pub(crate) fn destroy_sector_derived(&mut self, key: SectorKey) -> Result<(), ModelError> {
        self.check_sector_destruction_domain(key)?;
        let i = self.sector_index(key)?;
        if self.sectors[i].geometry.destructor_entered
            || self.sectors[i].geometry.update_interrupted
        {
            return Err(SectorError::Interrupted.into());
        }
        self.sectors[i].geometry.destructor_entered = true;
        self.sector_version = self.sector_version.wrapping_add(1);
        self.sector_add_to_scene_ring(key, None)?;
        if let Some(occluder) = self.sectors[i].hull_occluder {
            self.clear_occluder_sector_backpointer(occluder)?;
            self.sectors[i].hull_occluder = None;
        }
        for light in self.sectors[i].lights.clone() {
            let list = &mut self.graph.light_state_mut(light)?.sectors;
            if let Some(index) = list.iter().position(|&s| s == key.frame()) {
                list.swap_remove(index);
            }
        }
        self.sectors[i].lights = Vec::new();
        self.sectors[i].num_fog_lights = 0;
        self.sectors[i].fog_light = None;
        while let Some(&sound) = self.sectors[i].sounds.first() {
            self.delete_sound_from_sector(key, self.sound_key(sound)?)?;
        }
        self.sector_storage.free(self.sectors[i].geometry.planes)?;
        for portal in self.sectors[i].geometry.portals.clone() {
            if let Some(connected) = self.portal(portal)?.connected_sector {
                let ci = self.sector_index(connected)?;
                let list = &mut self.sectors[ci].geometry.incoming_portals;
                if let Some(index) = list.iter().position(|&p| p == portal) {
                    list.swap_remove(index);
                }
            }
            self.delete_portal(portal)?;
        }
        self.sector_storage
            .free(self.sectors[i].geometry.outgoing_buffer)?;
        self.sectors[i].geometry.portals = Vec::new();
        self.sectors[i].geometry.outgoing_buffer = None;
        self.sectors[i].geometry.outgoing_capacity = 0;
        for portal in self.sectors[i].geometry.incoming_portals.clone() {
            let pi = self.portal_index(portal)?;
            self.portal_frames[pi].connected_sector = None;
            self.portal_frames[pi].put(0xf8, 0);
        }
        self.sector_storage
            .free(self.sectors[i].geometry.incoming_buffer)?;
        self.sectors[i].geometry.incoming_portals = Vec::new();
        self.sectors[i].geometry.incoming_buffer = None;
        self.sectors[i].geometry.incoming_capacity = 0;
        self.sector_storage
            .free(self.sectors[i].geometry.vertices)?;
        self.sector_storage.free(self.sectors[i].geometry.faces)?;
        // The native member-vector destructors see already-null outgoing and
        // incoming arrays, then the empty sound/light arrays before base tail.
        self.sector_storage.free(None)?;
        self.sector_storage.free(None)?;
        Ok(())
    }
}

impl ModelArena {
    /// Prevalidated positive FrameLoadImage/SectorLoadImage CPU intake into a
    /// fresh real factory sector. Hierarchy linkage and scene membership remain
    /// explicit caller operations. Native short common-field reads and original
    /// naming allocator failures are not emulated by this parsed-data adapter.
    pub fn load_sector_frame_image(
        &mut self,
        key: SectorKey,
        source: &lhr_assets::Frame,
    ) -> Result<Option<u32>, ModelError> {
        if source.kind != 5 || source.visual.is_some() {
            return Err(SectorError::MissingRecord.into());
        }
        let record = source.sector.as_ref().ok_or(SectorError::MissingRecord)?;
        admit_sector_record(record)?;
        let sector = self.sector(key)?;
        if sector.geometry.vertices.is_some()
            || sector.geometry.faces.is_some()
            || !sector.geometry.portals.is_empty()
        {
            return Err(SectorError::AlreadyLoaded.into());
        }
        let fi = self.frame_index(key.frame())?;
        if !self.frames[fi].name_is_null || !self.frames[fi].properties.is_empty() {
            return Err(SectorError::AlreadyLoaded.into());
        }
        self.graph.apply_common_load_image_fields(
            key.frame(),
            source.position,
            source.scale,
            crate::native_transform::SQuat {
                wxyz: source.rotation_wxyz,
            },
            source.culling,
        )?;
        if !source.name_bytes.is_empty() {
            self.set_frame_name(key.frame(), Some(&source.name_bytes))?;
        }
        self.frames[fi]
            .properties
            .try_reserve_exact(source.properties.len())
            .map_err(|_| SectorError::Allocation)?;
        self.frames[fi]
            .properties
            .extend_from_slice(&source.properties);
        self.load_sector_record(key, record)
    }
}
impl ModelArena {
    /// Read-only observations of the actual borrowed scene rings.
    pub fn scene_loaded_sector_count(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.sector_count)
    }
    pub fn scene_portal_count(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.portal_count)
    }
}

/// Original PC CheckPoint4fcc0 and GetSector4a1d0, using the actual owned
/// HR-world bbox and stored planes. Finite normal-or-zero PC24 arithmetic is
/// admitted; unsupported numeric values are host errors, never native false.
impl ModelArena {
    pub fn sector_check_point(
        &mut self,
        key: SectorKey,
        position: [f32; 3],
    ) -> Result<bool, ModelError> {
        self.sector(key)?;
        // The original updates bounds before testing either the box or point.
        let bounds = self.hr_bound_volume_w(key.frame())?;
        if position.iter().any(|v| !v.is_finite() || v.is_subnormal()) {
            return Err(SectorError::NumericUnavailable.into());
        }
        let [min, max] = bounds.bbox;
        if min[0].partial_cmp(&max[0]).is_none_or(|v| v.is_gt()) {
            return Ok(false);
        }
        if (0..3).any(|i| min[i] > position[i] || max[i] < position[i]) {
            return Ok(false);
        }
        for p in self.sector_planes(key)? {
            let p = p.map(f32::from_bits);
            // No intermediate native FST32. In this guarded PC24 exponent
            // domain each operation has the same value as these f32 results.
            let distance = math::add(
                math::add(
                    math::add(math::mul(p[2], position[2])?, math::mul(p[1], position[1])?)?,
                    math::mul(p[0], position[0])?,
                )?,
                p[3],
            )?;
            if distance < f32::from_bits(0xb22bcc77) {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub fn scene_get_sector(
        &mut self,
        scene: SceneKey,
        position: [f32; 3],
        hint: Option<SectorKey>,
    ) -> Result<SectorKey, ModelError> {
        let si = self.scene_index(scene)?;
        if let Some(hint) = hint {
            if self.sector(hint)?.sector_flags & 0x200 == 0
                && self.sector_check_point(hint, position)?
            {
                return Ok(hint);
            }
        }
        let mut current = self.scenes[si].sector_head;
        for _ in 0..self.scenes[si].sector_count {
            let key = current.ok_or(SectorError::InvalidRing)?;
            if self.sector_check_point(key, position)? {
                return Ok(key);
            }
            current = self.sector(key)?.geometry.next;
        }
        Ok(self.scenes[si].sectors[0])
    }
    /// Original SetName, PC RVA 0x4f230 (contract: Ghidra formed no body,
    /// LAB_1004f230/not_in_procedure; entry/mid probes + _strncpy and
    /// FUN_1001b6f0 callees + "sector " static bytes; mafia-re
    /// I3d_sector.cpp:166-176). Upstream working name, not an evidenced
    /// original symbol. Order-exact: null forwards to the base setter; else
    /// "sector " + first 54 NUL-truncated bytes through the base setter
    /// (strncpy tail-JMP order).
    pub fn sector_set_name(
        &mut self,
        key: SectorKey,
        name: Option<&[u8]>,
    ) -> Result<(), ModelError> {
        self.sector(key)?;
        let Some(raw) = name else {
            return self.set_frame_name(key.frame(), None);
        };
        let len = raw
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(raw.len())
            .min(54);
        let mut full = Vec::with_capacity(7 + len);
        full.extend_from_slice(b"sector ");
        full.extend_from_slice(&raw[..len]);
        self.set_frame_name(key.frame(), Some(&full))
    }
    /// Original Duplicate, PC RVA 0x50430 (differential: Ghidra body 399 B,
    /// 120 insn/17 BB; mafia-re I3d_sector.cpp:511-561). Upstream working
    /// name, not an evidenced original symbol. Order-exact: base
    /// duplicate_frame first; prepared source keeps the hull (message has no
    /// host log analog); else free dst hull, copy counts, verts new (n*12,
    /// 0x80000002 word on null with num_verts=0), 3-word copy, faces new
    /// (n*6, 0x80000002 with num_faces=0, verts retained), halfword copy;
    /// flags=(src&0x140)|0x800, group copy. PC copies no portals and touches
    /// no planes: both stay as admitted (disclosed). Null-frame limb has no
    /// projection (typed keys cannot spell null). Further edges (disclosed
    /// here): self-duplicate frees the hull before rereading it
    /// (surfaces Buffer, never a use-after-free — storage is freed first);
    /// a 0-vert/nonzero-face source skips the faces copy (unreachable via
    /// the loader, which keeps counts consistent). Word form mirrors
    /// load_sector_record: None=I3D_OK.
    pub fn duplicate_sector_hull(
        &mut self,
        dst: SectorKey,
        src: SectorKey,
    ) -> Result<Option<u32>, ModelError> {
        self.sector(src)?;
        self.sector(dst)?;
        self.duplicate_frame(dst.frame(), src.frame())?;
        let si = self.sector_index(src)?;
        let (src_flags, src_group, src_nv, src_nf, src_verts, src_faces) = {
            let s = &self.sectors[si];
            (
                s.sector_flags,
                s.geometry.group_bits,
                s.geometry.vertex_count,
                s.geometry.face_count,
                s.geometry.vertices,
                s.geometry.faces,
            )
        };
        if src_flags & 4 != 0 {
            let di = self.sector_index(dst)?;
            self.sectors[di].sector_flags = (src_flags & 0x140) | 0x800;
            self.sectors[di].geometry.group_bits = src_group;
            return Ok(None);
        }
        let di = self.sector_index(dst)?;
        self.sector_storage
            .free(self.sectors[di].geometry.vertices)?;
        self.sector_storage.free(self.sectors[di].geometry.faces)?;
        self.sectors[di].geometry.vertices = None;
        self.sectors[di].geometry.faces = None;
        self.sectors[di].geometry.vertex_count = 0;
        self.sectors[di].geometry.face_count = 0;
        self.sectors[di].geometry.vertex_count = src_nv;
        if src_nv != 0 {
            let bytes = (src_nv as usize)
                .checked_mul(12)
                .ok_or(SectorError::Extent)?;
            let alloc = self
                .sector_storage
                .allocate(SectorAllocationKind::Vertices, bytes)?;
            self.sectors[di].geometry.vertices = alloc;
            let Some(buffer) = alloc else {
                self.sectors[di].geometry.vertex_count = 0;
                let dj = self.sector_index(dst)?;
                self.sectors[dj].sector_flags = (src_flags & 0x140) | 0x800;
                self.sectors[dj].geometry.group_bits = src_group;
                return Ok(Some(0x80000002));
            };
            let src_key = src_verts.ok_or(SectorError::Buffer)?;
            let data = self.sector_storage.bytes(src_key)?.to_vec();
            if data.len() != bytes {
                return Err(SectorError::Buffer.into());
            }
            self.sector_storage.write(buffer, 0, &data)?;
            self.sectors[di].geometry.face_count = src_nf;
            if src_nf != 0 {
                let fbytes = (src_nf as usize)
                    .checked_mul(6)
                    .ok_or(SectorError::Extent)?;
                let falloc = self
                    .sector_storage
                    .allocate(SectorAllocationKind::Faces, fbytes)?;
                self.sectors[di].geometry.faces = falloc;
                let Some(fbuffer) = falloc else {
                    self.sectors[di].geometry.face_count = 0;
                    let dj = self.sector_index(dst)?;
                    self.sectors[dj].sector_flags = (src_flags & 0x140) | 0x800;
                    self.sectors[dj].geometry.group_bits = src_group;
                    return Ok(Some(0x80000002));
                };
                let src_fkey = src_faces.ok_or(SectorError::Buffer)?;
                let fdata = self.sector_storage.bytes(src_fkey)?.to_vec();
                if fdata.len() != fbytes {
                    return Err(SectorError::Buffer.into());
                }
                self.sector_storage.write(fbuffer, 0, &fdata)?;
            }
        }
        let dj = self.sector_index(dst)?;
        self.sectors[dj].sector_flags = (src_flags & 0x140) | 0x800;
        self.sectors[dj].geometry.group_bits = src_group;
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn image() -> lhr_assets::SectorRecord {
        let mut prefix = [0; 28];
        prefix[..4].copy_from_slice(&4u32.to_le_bytes());
        lhr_assets::SectorRecord {
            flags: 0x801,
            group: 0x11223344,
            vertices: vec![[0., 0., 0.], [1., 0., 0.], [0., 1., 0.]],
            faces: vec![[0, 1, 2], [0, 1, 2]],
            bounds: [[0., 0., 0.], [1., 1., 0.]],
            portals: vec![lhr_assets::SectorPortalRecord {
                prefix,
                vertices: vec![[0., 0., 0.], [1., 0., 0.], [1., 1., 0.], [0., 1., 0.]],
            }],
        }
    }
    #[test]
    fn check_point_uses_inclusive_world_bounds_and_strict_stored_plane_threshold() {
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        arena
            .set_frame_bound_volume(key.frame(), [[-1.; 3], [1.; 3]])
            .unwrap();
        let planes = arena
            .sector_storage
            .allocate(SectorAllocationKind::Planes, 16)
            .unwrap()
            .unwrap();
        let raw: Vec<u8> = [1f32, 0., 0., 0.]
            .into_iter()
            .flat_map(|v| v.to_bits().to_le_bytes())
            .collect();
        arena.sector_storage.write(planes, 0, &raw).unwrap();
        let si = arena.sector_index(key).unwrap();
        arena.sectors[si].geometry.planes = Some(planes);
        arena.sectors[si].geometry.plane_count = 1;
        let threshold = f32::from_bits(0xb22bcc77);
        assert!(arena.sector_check_point(key, [threshold, 0., 0.]).unwrap());
        assert!(!arena
            .sector_check_point(key, [f32::from_bits(threshold.to_bits() + 1), 0., 0.])
            .unwrap());
        assert!(arena.sector_check_point(key, [1., 1., 1.]).unwrap());
        assert!(!arena.sector_check_point(key, [2., 0., 0.]).unwrap());
        assert!(matches!(
            arena.sector_check_point(key, [f32::NAN, 0., 0.]),
            Err(ModelError::Sector(SectorError::NumericUnavailable))
        ));
        assert_ne!(arena.state(key.frame()).unwrap().flags & 0x100, 0);
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.sector_storage.live_allocations(), 0);
    }
    #[test]
    fn sector_visible_reports_only_bit_zero() {
        assert_eq!(SCTF_VISIBLE, 0x01);
        assert!(sector_is_visible(SCTF_VISIBLE));
        assert!(sector_is_visible(u32::MAX));
        assert!(!sector_is_visible(0));
        assert!(!sector_is_visible(SCTF_PREPARED));
        assert!(!sector_is_visible(u32::MAX & !SCTF_VISIBLE));
    }
    #[test]
    fn sector_prepared_reports_only_bit_two() {
        assert_eq!(SCTF_PREPARED, 0x04);
        assert!(sector_is_prepared(SCTF_PREPARED));
        assert!(sector_is_prepared(u32::MAX));
        assert!(!sector_is_prepared(0));
        assert!(!sector_is_prepared(SCTF_VISIBLE));
        assert!(!sector_is_prepared(u32::MAX & !SCTF_PREPARED));
    }
    #[test]
    fn get_sector_honors_hint_before_ring_but_does_not_filter_ring_primary_flag() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let a = arena.create_sector_frame().unwrap();
        let b = arena.create_sector_frame().unwrap();
        for key in [a, b] {
            arena
                .set_frame_bound_volume(key.frame(), [[-1.; 3], [1.; 3]])
                .unwrap();
            arena.sector_add_to_scene_ring(key, Some(scene)).unwrap();
        }
        assert_eq!(arena.scene_get_sector(scene, [0.; 3], Some(b)).unwrap(), b);
        let ai = arena.sector_index(a).unwrap();
        let bi = arena.sector_index(b).unwrap();
        arena.sectors[ai].sector_flags |= 0x200;
        arena.sectors[bi].sector_flags |= 0x200;
        assert_eq!(arena.scene_get_sector(scene, [0.; 3], Some(b)).unwrap(), a);
        assert_eq!(
            arena.scene_get_sector(scene, [2.; 3], None).unwrap(),
            arena.scene(scene).unwrap().sectors[0]
        );
        arena.release(a.frame()).unwrap();
        arena.release(b.frame()).unwrap();
        for key in arena.scene(scene).unwrap().sectors {
            arena.release(key.frame()).unwrap();
        }
        // Destroy-tail slot hygiene on the scene-base path (the release_inner
        // tail is covered by the refcount test in model_loading).
        arena
            .set_frame_callback(
                scene.root(),
                Some(crate::frame_callback::FrameCallbackToken(3)),
                0x1e00,
            )
            .unwrap();
        assert!(arena.callbacks.has_slot(scene.root()));
        arena.destroy_empty_scene_base(scene.root()).unwrap();
        assert!(!arena.callbacks.has_slot(scene.root()));
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn make_ready_sectors_enum_proc_4a8c0_reports_ok_and_marks_prepared() {
        // LS3DF RVA 0x4a8c0, VA 0x1004a8c0: MakeReadySectors' static
        // I3D_EnumProc (mafia-re ls3df/I3D/I3d_scene.cpp:650-657). Runs the
        // sector MakeReady and returns I3DENUMRET_OK (0).
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        assert_eq!(arena.make_ready_sectors_enum_proc(key), Ok(0));
        assert_ne!(arena.sector(key).unwrap().sector_flags & 4, 0);
        assert_eq!(arena.make_ready_sectors_enum_proc(key), Ok(0));
        assert!(arena
            .make_ready_sectors_enum_proc(SectorKey(crate::frame_transform::FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn loaded_hull_portal_planes_and_last_release_have_real_single_owners() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let key = arena.create_sector_frame().unwrap();
            assert_eq!(arena.sector(key).unwrap().scene, None);
            assert_eq!(arena.load_sector_record(key, &image()).unwrap(), None);
            assert_eq!(arena.sector(key).unwrap().geometry.group_bits, 0x11223344);
            let p = arena.sector(key).unwrap().geometry.portals[0];
            arena.sector_make_ready(key).unwrap();
            assert_eq!(arena.sector(key).unwrap().sector_flags, 0x805);
            let planes = arena.sector_planes(key).unwrap();
            assert_eq!(planes.len(), 1);
            assert_eq!(
                planes[0],
                [
                    0f32.to_bits(),
                    0f32.to_bits(),
                    1f32.to_bits(),
                    (-0f32).to_bits()
                ]
            );
            assert_eq!(
                arena.portal(p).unwrap().known_word(0x110),
                Some(0.5f32.to_bits())
            );
            assert_eq!(
                arena.portal(p).unwrap().known_word(0x114),
                Some(0.5f32.to_bits())
            );
            assert_eq!(arena.portal(p).unwrap().known_word(0x11c), Some(0x3f3504f3));
            let calls = arena.sector_storage().allocation_calls();
            arena.sector_make_ready(key).unwrap();
            assert_eq!(arena.sector_storage().allocation_calls(), calls);
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.live_frame_count(), 0);
            assert_eq!(arena.live_portal_allocations(), 0);
            assert_eq!(arena.sector_storage().live_allocations(), 0);
            assert_eq!(arena.sector_storage().live_bytes(), 0);
            assert_eq!(arena.global_sector_version(), 2);
            assert_eq!(arena.global_portal_version(), 2);
        });
    }
    #[test]
    fn hull_null_allocations_free_both_arrays_and_portal_null_keeps_prior_prefix() {
        for fail in [1, 2] {
            let mut arena = ModelArena::default();
            let key = arena.create_sector_frame().unwrap();
            arena.sector_storage_mut().fail_on_allocation(Some(fail));
            assert_eq!(
                arena.load_sector_record(key, &image()).unwrap(),
                Some(0x80000002)
            );
            assert_eq!(arena.sector_storage().allocation_calls(), 2);
            assert_eq!(arena.sector_storage().live_allocations(), 0);
            assert!(arena.sector(key).unwrap().bounds.is_none());
            assert!(arena.sector(key).unwrap().geometry.portals.is_empty());
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.live_frame_count(), 0);
        }
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        let mut source = image();
        source.portals.push(source.portals[0].clone());
        arena.fail_portal_allocation(Some(2));
        assert_eq!(
            arena.load_sector_record(key, &source).unwrap(),
            Some(0x80000002)
        );
        assert_eq!(arena.sector(key).unwrap().geometry.portals.len(), 1);
        assert_eq!(arena.global_portal_version(), 1);
        assert_eq!(arena.live_portal_allocations(), 1);
        assert!(arena.sector(key).unwrap().bounds.is_some());
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.live_portal_allocations(), 0);
        assert_eq!(arena.sector_storage().live_allocations(), 0);
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn malformed_portal_admission_precedes_all_payload_and_allocation_effects() {
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        let mut record = image();
        record.flags = 0x1234;
        record.group = 99;
        record.portals[0].vertices = vec![[0.; 3]; 9];
        assert!(matches!(
            arena.load_sector_record(key, &record),
            Err(ModelError::Portal(PortalError::UnsafeVertexCount(9)))
        ));
        assert_eq!(arena.sector_storage().allocation_calls(), 0);
        assert_eq!(arena.portal_allocation_calls(), 0);
        let s = arena.sector(key).unwrap();
        assert_eq!(s.sector_flags, 0x801);
        assert_eq!(s.geometry.group_bits, 0);
        assert!(s.bounds.is_none());
        assert!(s.geometry.portals.is_empty());
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn make_ready_host_failures_keep_prepared_prefix_but_never_replay_as_success() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            for mode in 0..3 {
                let mut arena = ModelArena::default();
                let key = arena.create_sector_frame().unwrap();
                let mut record = image();
                let expected = match mode {
                    0 => SectorError::Allocation,
                    1 => {
                        record.vertices[0][0] = f32::NAN;
                        SectorError::NumericUnavailable
                    }
                    _ => {
                        record.flags |= 0x40;
                        SectorError::GeneratedOccluderUnavailable
                    }
                };
                assert_eq!(arena.load_sector_record(key, &record).unwrap(), None);
                if mode == 0 {
                    let next = arena.sector_storage().allocation_calls() + 1;
                    arena.sector_storage_mut().fail_on_allocation(Some(next));
                }
                assert_eq!(
                    arena.sector_make_ready(key),
                    Err(ModelError::Sector(expected))
                );
                let sector = arena.sector(key).unwrap();
                assert_ne!(sector.sector_flags & 4, 0);
                assert!(sector.geometry.update_interrupted);
                assert!(sector.geometry.planes.is_none());
                assert_eq!(sector.geometry.plane_count, 0);
                if mode == 0 {
                    assert!(matches!(
                        arena.sector_storage().events().last(),
                        Some(SectorStorageEvent::Allocate {
                            kind: SectorAllocationKind::TemporaryPlanes,
                            key: None,
                            ..
                        })
                    ));
                }
                let events = arena.sector_storage().events().to_vec();
                let calls = arena.sector_storage().allocation_calls();
                let bytes = arena.sector_storage().live_bytes();
                let flags = arena.state(key.frame()).unwrap().flags;
                let updates = arena.state(key.frame()).unwrap().update_count;
                let versions = (arena.sector_version, arena.portal_version);
                arena.sector_storage_mut().fail_on_allocation(None);
                assert_eq!(
                    arena.sector_make_ready(key),
                    Err(SectorError::Interrupted.into())
                );
                assert_eq!(arena.sector_storage().events(), events);
                assert_eq!(arena.sector_storage().allocation_calls(), calls);
                assert_eq!(arena.sector_storage().live_bytes(), bytes);
                assert_eq!(arena.state(key.frame()).unwrap().flags, flags);
                assert_eq!(arena.state(key.frame()).unwrap().update_count, updates);
                assert_eq!((arena.sector_version, arena.portal_version), versions);
                assert_eq!(
                    arena.release(key.frame()),
                    Err(SectorError::Interrupted.into())
                );
            }
        });
    }
    #[test]
    fn final_plane_null_returns_normally_and_frees_the_temporary_list() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let key = arena.create_sector_frame().unwrap();
            arena.load_sector_record(key, &image()).unwrap();
            let final_call = arena.sector_storage().allocation_calls() + 2;
            arena
                .sector_storage_mut()
                .fail_on_allocation(Some(final_call));
            arena.sector_make_ready(key).unwrap();
            let s = arena.sector(key).unwrap();
            assert_eq!(s.geometry.plane_count, 1);
            assert!(s.geometry.planes.is_none());
            assert!(!s.geometry.update_interrupted);
            let events = arena.sector_storage().events().to_vec();
            arena.sector_make_ready(key).unwrap();
            assert_eq!(arena.sector_storage().events(), events);
            assert!(matches!(
                arena.sector_planes(key),
                Err(ModelError::Sector(SectorError::Buffer))
            ));
            assert_eq!(arena.sector_storage().live_allocations(), 3);
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.sector_storage().live_allocations(), 0);
            assert_eq!(arena.live_portal_allocations(), 0);
            assert_eq!(arena.live_frame_count(), 0);
        });
    }
    struct Screen;
    impl crate::scene_resources::SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> Result<i32, ()> {
            Ok(800)
        }
        fn screen_sy(&mut self) -> Result<i32, ()> {
            Ok(600)
        }
    }
    impl crate::driver_scene::DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            Ok([0, 0, 800, 600])
        }
    }
    struct Images;
    impl crate::itexture::TextureImageSource for Images {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, crate::itexture::TextureImageError> {
            let mut bytes = vec![0; 18];
            bytes[2] = 2;
            bytes[12] = 1;
            bytes[14] = 1;
            bytes[16] = 24;
            bytes.extend([255, 255, 255]);
            Ok(bytes)
        }
    }
    #[test]
    fn two_driver_scenes_keep_loaded_borrowed_rings_and_shared_snow_until_valid_last_release() {
        use crate::driver_scene::{DriverSceneContext, DriverSceneOwner, NativeSceneSoundDriver};
        use crate::itexture::{CpuTextureStorage, TextureResources};
        use crate::scene_resources::SceneResources;
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            for explicit_detach in [false, true] {
                let mut arena = ModelArena::default();
                let mut resources = SceneResources::default();
                let mut textures = TextureResources::new().unwrap();
                let mut storage = CpuTextureStorage::default();
                let mut driver = DriverSceneOwner::default();
                let mut screen = Screen;
                let a = driver
                    .create_scene(
                        &mut DriverSceneContext {
                            arena: &mut arena,
                            resources: &mut resources,
                            textures: &mut textures,
                            storage: &mut storage,
                        },
                        &mut screen,
                        &mut Images,
                        NativeSceneSoundDriver::Absent,
                    )
                    .unwrap();
                let b = driver
                    .create_scene(
                        &mut DriverSceneContext {
                            arena: &mut arena,
                            resources: &mut resources,
                            textures: &mut textures,
                            storage: &mut storage,
                        },
                        &mut screen,
                        &mut Images,
                        NativeSceneSoundDriver::Absent,
                    )
                    .unwrap();
                assert_eq!(resources.ws_ref_count(), 2);
                let snow = resources.snow_texture().unwrap();
                let key = driver
                    .create_sector_frame(&mut arena, &resources, a)
                    .unwrap();
                arena.load_sector_record(key, &image()).unwrap();
                arena.scene_add_frame(a, key.frame()).unwrap();
                arena.release(key.frame()).unwrap();
                let parent = arena.scene(b).unwrap().sectors[0];
                arena
                    .link_frame(key.frame(), Some(parent.frame()), 0)
                    .unwrap();
                arena.sector_make_ready(key).unwrap();
                let portal = arena.sector(key).unwrap().geometry.portals[0];
                assert_eq!(arena.frame(key.frame()).unwrap().owner, Some(a.into()));
                assert_eq!(arena.sector(key).unwrap().scene, Some(b));
                assert_eq!(arena.portal(portal).unwrap().scene, Some(b));
                assert_eq!(arena.scene_loaded_sector_count(b).unwrap(), 1);
                assert_eq!(arena.scene_portal_count(b).unwrap(), 1);
                let (first, last) = if explicit_detach {
                    arena.link_frame(key.frame(), None, 0).unwrap();
                    assert_eq!(arena.sector(key).unwrap().scene, None);
                    assert_eq!(arena.portal(portal).unwrap().scene, None);
                    (b, a)
                } else {
                    (a, b)
                };
                resources
                    .close_constructed_scene(&mut arena, first, &mut textures, &mut storage)
                    .unwrap();
                assert_eq!(arena.scene_loaded_sector_count(b).unwrap(), 0);
                assert_eq!(arena.scene_portal_count(b).unwrap(), 0);
                driver
                    .release_scene(
                        &mut DriverSceneContext {
                            arena: &mut arena,
                            resources: &mut resources,
                            textures: &mut textures,
                            storage: &mut storage,
                        },
                        first,
                    )
                    .unwrap();
                assert_eq!(resources.ws_ref_count(), 1);
                assert_eq!(resources.snow_texture(), Some(snow));
                if explicit_detach {
                    assert_eq!(arena.frame(key.frame()).unwrap().owner, Some(a.into()));
                    assert_eq!(arena.frame(key.frame()).unwrap().references, 1);
                    assert_eq!(arena.live_portal_allocations(), 1);
                } else {
                    assert_eq!(arena.live_portal_allocations(), 0);
                }
                resources
                    .close_constructed_scene(&mut arena, last, &mut textures, &mut storage)
                    .unwrap();
                driver
                    .release_scene(
                        &mut DriverSceneContext {
                            arena: &mut arena,
                            resources: &mut resources,
                            textures: &mut textures,
                            storage: &mut storage,
                        },
                        last,
                    )
                    .unwrap();
                driver.release_empty_registry_storage().unwrap();
                assert_eq!(arena.live_frame_count(), 0);
                assert_eq!(arena.live_portal_allocations(), 0);
                assert_eq!(arena.sector_storage().live_allocations(), 0);
                assert_eq!(arena.sector_storage().live_bytes(), 0);
                assert_eq!(resources.ws_ref_count(), 0);
                assert_eq!(resources.snow_texture(), None);
                assert_eq!(driver.capacity(), 0);
            }
        });
    }
    #[test]
    fn set_bound_volume_rejects_unordered_x_and_preserves_center() {
        // RVA 0x1ce80: ordered-X gate (NaN fails); invalid path restores
        // sentinels, zeroes radius, preserves the old center.
        use crate::mesh_bounds::MeshBoundVolume;
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        arena
            .set_frame_bound_volume(key.frame(), [[-1.; 3], [1.; 3]])
            .unwrap();
        let valid = arena.frame(key.frame()).unwrap().hr_bounds;
        assert_eq!(valid.center, [0.; 3]);
        assert!(valid.radius > 0.);
        // A NaN minimum X fails the ordered-X validity gate.
        arena
            .set_frame_bound_volume(key.frame(), [[f32::NAN, -2., -3.], [1., 2., 3.]])
            .unwrap();
        let invalid = arena.frame(key.frame()).unwrap().hr_bounds;
        assert_eq!(invalid.bbox, MeshBoundVolume::CONSTRUCTOR.bbox);
        assert_eq!(invalid.radius, 0.);
        assert_eq!(invalid.center, valid.center);
        assert_ne!(arena.state(key.frame()).unwrap().flags & 0x40, 0);
        // An unordered X extent takes the same path.
        arena
            .set_frame_bound_volume(key.frame(), [[2., -2., -3.], [1., 2., 3.]])
            .unwrap();
        let unordered = arena.frame(key.frame()).unwrap().hr_bounds;
        assert_eq!(unordered.bbox, MeshBoundVolume::CONSTRUCTOR.bbox);
        assert_eq!(unordered.radius, 0.);
        assert_eq!(unordered.center, valid.center);
        arena.release(key.frame()).unwrap();
    }
    #[test]
    fn sector_set_name_prefix_and_truncation() {
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        arena.sector_set_name(key, None).unwrap();
        assert!(arena.frame(key.frame()).unwrap().name_is_null);
        arena.sector_set_name(key, Some(b"abc")).unwrap();
        assert_eq!(arena.frame(key.frame()).unwrap().name_bytes, b"sector abc");
        let long = [b'x'; 60];
        arena.sector_set_name(key, Some(&long)).unwrap();
        let name = arena.frame(key.frame()).unwrap().name_bytes.clone();
        assert_eq!(name.len(), 7 + 54);
        assert!(name.starts_with(b"sector "));
        arena.sector_set_name(key, Some(b"ab\x00cd")).unwrap();
        assert_eq!(arena.frame(key.frame()).unwrap().name_bytes, b"sector ab");
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn sector_ring_add_remove_roundtrip() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let v0 = arena.global_sector_version();
        let a = arena.create_sector_frame().unwrap();
        let b = arena.create_sector_frame().unwrap();
        arena.sector_add_to_scene_ring(a, Some(scene)).unwrap();
        arena.sector_add_to_scene_ring(b, Some(scene)).unwrap();
        assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 2);
        assert_eq!(arena.scene(scene).unwrap().sector_head, Some(a));
        assert_eq!(arena.sector(a).unwrap().geometry.next, Some(b));
        assert_eq!(arena.sector(b).unwrap().geometry.next, Some(a));
        // Same-scene re-add is a no-op (no version bump, no count change).
        let v1 = arena.global_sector_version();
        arena.sector_add_to_scene_ring(a, Some(scene)).unwrap();
        assert_eq!(arena.global_sector_version(), v1);
        assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 2);
        // Remove head: count--, head advances, ring re-spliced.
        arena.sector_add_to_scene_ring(a, None).unwrap();
        assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 1);
        assert_eq!(arena.scene(scene).unwrap().sector_head, Some(b));
        assert_eq!(arena.sector(b).unwrap().geometry.previous, Some(b));
        assert_eq!(arena.sector(b).unwrap().geometry.next, Some(b));
        // Re-add lands at the tail.
        arena.sector_add_to_scene_ring(a, Some(scene)).unwrap();
        assert_eq!(arena.scene(scene).unwrap().sector_head, Some(b));
        assert_eq!(arena.sector(b).unwrap().geometry.next, Some(a));
        assert!(arena.global_sector_version() > v0);
        arena.sector_add_to_scene_ring(a, None).unwrap();
        arena.sector_add_to_scene_ring(b, None).unwrap();
        assert_eq!(arena.scene(scene).unwrap().sector_head, None);
        assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 0);
        arena.release(a.frame()).unwrap();
        arena.release(b.frame()).unwrap();
        assert_eq!(arena.live_frame_count(), 5); // scene root + 4 retained sectors
    }
    #[test]
    fn sector_dtor_clears_ring_and_buffers() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let key = arena.create_sector_frame().unwrap();
            assert_eq!(arena.load_sector_record(key, &image()).unwrap(), None);
            arena.sector_add_to_scene_ring(key, Some(scene)).unwrap();
            assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 1);
            assert!(arena.sector_storage().live_allocations() >= 2);
            arena.sector_add_to_scene_ring(key, None).unwrap();
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.sector_storage().live_allocations(), 0);
            assert_eq!(arena.sector_storage().live_bytes(), 0);
            assert_eq!(arena.scene(scene).unwrap().sector_head, None);
        });
    }
    #[test]
    fn sector_duplicate_hull_field_equality() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let src = arena.create_sector_frame().unwrap();
            assert_eq!(arena.load_sector_record(src, &image()).unwrap(), None);
            let dst = arena.create_sector_frame().unwrap();
            assert_eq!(arena.duplicate_sector_hull(dst, src).unwrap(), None);
            let (s, d) = (arena.sector(src).unwrap(), arena.sector(dst).unwrap());
            assert_eq!(d.geometry.vertex_count, s.geometry.vertex_count);
            assert_eq!(d.geometry.face_count, s.geometry.face_count);
            assert_eq!(d.sector_flags, (s.sector_flags & 0x140) | 0x800);
            assert_eq!(d.geometry.group_bits, s.geometry.group_bits);
            let sb = arena
                .sector_storage()
                .bytes(s.geometry.vertices.unwrap())
                .unwrap()
                .to_vec();
            let db = arena
                .sector_storage()
                .bytes(d.geometry.vertices.unwrap())
                .unwrap()
                .to_vec();
            assert_eq!(sb, db);
            let sf = arena
                .sector_storage()
                .bytes(s.geometry.faces.unwrap())
                .unwrap()
                .to_vec();
            let df = arena
                .sector_storage()
                .bytes(d.geometry.faces.unwrap())
                .unwrap()
                .to_vec();
            assert_eq!(sf, df);
            // Prepared source keeps the destination hull, still copies flags/group.
            let si = arena.sector_index(src).unwrap();
            arena.sectors[si].sector_flags |= 4;
            let dst2 = arena.create_sector_frame().unwrap();
            assert_eq!(arena.duplicate_sector_hull(dst2, src).unwrap(), None);
            let d2 = arena.sector(dst2).unwrap();
            assert!(d2.geometry.vertices.is_none() && d2.geometry.faces.is_none());
            assert_eq!(
                d2.sector_flags,
                (arena.sector(src).unwrap().sector_flags & 0x140) | 0x800
            );
            // OOM on verts: word + zeroed counts + no live residue.
            let src2 = arena.create_sector_frame().unwrap();
            assert_eq!(arena.load_sector_record(src2, &image()).unwrap(), None);
            let dst3 = arena.create_sector_frame().unwrap();
            let next = arena.sector_storage().allocation_calls() + 1;
            arena.sector_storage_mut().fail_on_allocation(Some(next));
            assert_eq!(
                arena.duplicate_sector_hull(dst3, src2).unwrap(),
                Some(0x80000002)
            );
            let d3 = arena.sector(dst3).unwrap();
            assert_eq!(d3.geometry.vertex_count, 0);
            assert!(d3.geometry.vertices.is_none() && d3.geometry.faces.is_none());
            arena.sector_storage_mut().fail_on_allocation(None);
            // OOM on faces: word + verts retained + faces zeroed.
            let dst4 = arena.create_sector_frame().unwrap();
            let calls = arena.sector_storage().allocation_calls();
            arena
                .sector_storage_mut()
                .fail_on_allocation(Some(calls + 2));
            assert_eq!(
                arena.duplicate_sector_hull(dst4, src2).unwrap(),
                Some(0x80000002)
            );
            let d4 = arena.sector(dst4).unwrap();
            assert!(d4.geometry.vertices.is_some());
            assert_eq!(d4.geometry.face_count, 0);
            assert!(d4.geometry.faces.is_none());
            arena.sector_storage_mut().fail_on_allocation(None);
            for k in [src, dst, dst2, src2, dst3, dst4] {
                arena.release(k.frame()).unwrap();
            }
            assert_eq!(arena.sector_storage().live_allocations(), 0);
        });
    }
    #[test]
    fn sector_load_failure_prefix_allows_retry() {
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        arena.sector_storage_mut().fail_on_allocation(Some(1));
        assert_eq!(
            arena.load_sector_record(key, &image()).unwrap(),
            Some(0x80000002)
        );
        assert!(arena.sector(key).unwrap().bounds.is_none());
        assert!(arena.sector(key).unwrap().geometry.portals.is_empty());
        arena.sector_storage_mut().fail_on_allocation(None);
        assert_eq!(arena.load_sector_record(key, &image()).unwrap(), None);
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.sector_storage().live_allocations(), 0);
    }
    #[test]
    fn check_point_strict_plane_threshold_rejects_just_below_minus_eps() {
        let mut arena = ModelArena::default();
        let key = arena.create_sector_frame().unwrap();
        arena
            .set_frame_bound_volume(key.frame(), [[-1.; 3], [1.; 3]])
            .unwrap();
        let planes = arena
            .sector_storage
            .allocate(SectorAllocationKind::Planes, 16)
            .unwrap()
            .unwrap();
        let raw: Vec<u8> = [1f32, 0., 0., 0.]
            .into_iter()
            .flat_map(|v| v.to_bits().to_le_bytes())
            .collect();
        arena.sector_storage.write(planes, 0, &raw).unwrap();
        let si = arena.sector_index(key).unwrap();
        arena.sectors[si].geometry.planes = Some(planes);
        arena.sectors[si].geometry.plane_count = 1;
        // distance == -1e-08 is inside (strict `<` fails only below).
        let eps = f32::from_bits(0xb22bcc77);
        assert!(arena.sector_check_point(key, [eps, 0., 0.]).unwrap());
        // Negative floats grow more negative with larger bits.
        let below = f32::from_bits(eps.to_bits() + 1);
        assert!(below < eps);
        assert!(!arena.sector_check_point(key, [below, 0., 0.]).unwrap());
        arena.release(key.frame()).unwrap();
        assert_eq!(arena.sector_storage.live_allocations(), 0);
    }
    #[test]
    fn make_ready_empty_hull_sets_prepared_bit_with_no_allocations() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let key = arena.create_sector_frame().unwrap();
            assert_eq!(arena.sector_storage().allocation_calls(), 0);
            arena.sector_make_ready(key).unwrap();
            let s = arena.sector(key).unwrap();
            assert_ne!(s.sector_flags & 4, 0);
            assert_eq!(s.geometry.plane_count, 0);
            assert!(s.geometry.planes.is_none());
            assert!(!s.geometry.update_interrupted);
            assert_eq!(arena.sector_storage().allocation_calls(), 0);
            let calls = arena.sector_storage().allocation_calls();
            arena.sector_make_ready(key).unwrap(); // idempotent early-out
            assert_eq!(arena.sector_storage().allocation_calls(), calls);
            arena.release(key.frame()).unwrap();
            assert_eq!(arena.live_frame_count(), 0);
        });
    }
}
