//! Source-intake ownership for C_traffic_generator, not traffic spawning/AI.
//! Game SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! ctor RVA0x4d410, Init0x393a0, LoadData0x4d820, dtor0x4d4a0.
//! PS2 class naming and PC vtable623fa0 corroborate the class; member names
//! below are descriptive. Scalars retain source bits; no floating arithmetic.
#![forbid(unsafe_code)]
use crate::script_scheduler::ActorReferenceId;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Explicit allocation seed for constructor-untouched scalar storage. No Default
/// is supplied: a host policy is not an observation of the original heap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrafficGeneratorSeed {
    pub bytes_71_73: [u8; 3],
    pub word_78: u32,
    pub words_80_84: [u32; 2],
    pub word_a4: u32,
    pub words_ac_b4: [u32; 3],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficBackingKind {
    RecordPointers,
    Record,
    Weights,
}
/// Owner/generation scoped native allocation lease, never a host pointer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrafficAllocationId {
    domain: u64,
    serial: u64,
    actor: ActorReferenceId,
    kind: TrafficBackingKind,
    bytes: u32,
}
impl TrafficAllocationId {
    pub fn actor(self) -> ActorReferenceId {
        self.actor
    }
    pub(crate) fn matches(
        self,
        actor: ActorReferenceId,
        kind: TrafficBackingKind,
        bytes: u32,
    ) -> bool {
        self.actor == actor && self.kind == kind && self.bytes == bytes
    }
}
/// Heap boundary owns allocation leases; typed contents live only in the actor.
/// No anonymous byte-object or second copy of record/weight contents lives here.
pub struct TrafficHeap {
    domain: u64,
    next: u64,
    leases: BTreeMap<u64, (ActorReferenceId, TrafficBackingKind, u32)>,
}
impl Default for TrafficHeap {
    fn default() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let domain = NEXT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("traffic heap identity exhausted");
        Self {
            domain,
            next: 0,
            leases: BTreeMap::new(),
        }
    }
}
impl TrafficHeap {
    pub fn allocate(
        &mut self,
        actor: ActorReferenceId,
        kind: TrafficBackingKind,
        bytes: u32,
    ) -> Result<TrafficAllocationId, String> {
        let serial = self.next.checked_add(1).ok_or("traffic heap exhausted")?;
        self.next = serial;
        self.leases.insert(serial, (actor, kind, bytes));
        Ok(TrafficAllocationId {
            domain: self.domain,
            serial,
            actor,
            kind,
            bytes,
        })
    }
    pub fn free(
        &mut self,
        actor: ActorReferenceId,
        kind: TrafficBackingKind,
        allocation: Option<TrafficAllocationId>,
    ) -> Result<(), String> {
        let Some(id) = allocation else { return Ok(()) };
        if id.domain != self.domain
            || id.actor != actor
            || !self
                .leases
                .get(&id.serial)
                .is_some_and(|&(a, k, _)| a == actor && k == kind)
        {
            return Err("foreign or stale traffic allocation".into());
        }
        self.leases.remove(&id.serial);
        Ok(())
    }
    /// Terminal host cleanup removes only this actual heap's still-live leases.
    /// It calls no original free callback and cannot replay an uncertain free.
    pub fn discard_actor_host(&mut self, actor: ActorReferenceId) {
        self.leases.retain(|_, (owner, _, _)| *owner != actor);
    }
    pub fn live_allocations(&self) -> usize {
        self.leases.len()
    }
}
/// Each byte's initialization is explicit. Unknown malloc suffixes are not zero
/// and do not become typed numeric values until a reached read defines them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrafficWord([Option<u8>; 4]);
impl TrafficWord {
    pub(crate) fn unknown() -> Self {
        Self([None; 4])
    }
    pub fn bits(&self) -> Option<u32> {
        Some(u32::from_le_bytes([
            self.0[0]?, self.0[1]?, self.0[2]?, self.0[3]?,
        ]))
    }
    pub(crate) fn known(value: u32) -> Self {
        Self(value.to_le_bytes().map(Some))
    }
    pub(crate) fn destination(&mut self) -> &mut [Option<u8>] {
        &mut self.0
    }
}
#[derive(Debug)]
pub struct TrafficRecord {
    pub(crate) allocation: TrafficAllocationId,
    pub(crate) bytes: [Option<u8>; 17],
}
impl TrafficRecord {
    pub fn bytes(&self) -> &[Option<u8>; 17] {
        &self.bytes
    }
}
#[derive(Debug)]
pub enum TrafficRecordSlot {
    /// malloc pointer slot has not yet been written.
    Unknown,
    /// Native allocation returned NULL and that NULL was published.
    Null,
    Owned(TrafficRecord),
    /// Successful free removes ownership, not the native observed pointer bits.
    Released(TrafficAllocationId),
}
#[derive(Debug)]
pub struct TrafficPointerArray {
    pub(crate) allocation: TrafficAllocationId,
    pub(crate) records: Vec<TrafficRecordSlot>,
}
impl TrafficPointerArray {
    pub fn records(&self) -> &[TrafficRecordSlot] {
        &self.records
    }
}
#[derive(Debug)]
pub struct TrafficWeights {
    pub(crate) allocation: TrafficAllocationId,
    /// Serialized source words, not an ordinary object mirror.
    pub(crate) bytes: Vec<Option<u8>>,
}
impl TrafficWeights {
    pub fn bytes(&self) -> &[Option<u8>] {
        &self.bytes
    }
    pub fn word(&self, index: usize) -> Option<u32> {
        let bytes = self
            .bytes
            .get(index.checked_mul(4)?..index.checked_mul(4)?.checked_add(4)?)?;
        Some(u32::from_le_bytes([
            bytes[0]?, bytes[1]?, bytes[2]?, bytes[3]?,
        ]))
    }
}
#[derive(Debug)]
pub struct TrafficGeneratorState {
    pub(crate) enabled: [Option<u8>; 1],
    pub(crate) scalar_bits: [TrafficWord; 5],
    pub(crate) count: TrafficWord,
    pub(crate) total: TrafficWord,
    pub(crate) records: Option<TrafficPointerArray>,
    pub(crate) weights: Option<TrafficWeights>,
    /// Allocation completed before a failed pointer-slot publication.
    pub(crate) pending_record: Option<TrafficRecord>,
    pub(crate) base_destruction_entered: bool,
    heap_domain: Option<u64>,
    untouched: TrafficGeneratorSeed,
    constructor_zero: u32,
}
impl TrafficGeneratorState {
    pub(crate) fn construct(untouched: TrafficGeneratorSeed) -> Self {
        Self {
            enabled: [Some(1)],
            scalar_bits: [0x42c80000, 0x42c80000, 0x42480000, 0x42b40000, 0x42480000]
                .map(TrafficWord::known),
            count: TrafficWord::known(0),
            total: TrafficWord::known(10),
            records: None,
            weights: None,
            pending_record: None,
            base_destruction_entered: false,
            heap_domain: None,
            untouched,
            constructor_zero: 0,
        }
    }
    pub fn enabled_byte(&self) -> Option<u8> {
        self.enabled[0]
    }
    pub fn scalar_bits(&self) -> &[TrafficWord; 5] {
        &self.scalar_bits
    }
    pub fn count_bits(&self) -> Option<u32> {
        self.count.bits()
    }
    pub fn total_bits(&self) -> Option<u32> {
        self.total.bits()
    }
    pub fn records(&self) -> Option<&TrafficPointerArray> {
        self.records.as_ref()
    }
    pub fn weights(&self) -> Option<&TrafficWeights> {
        self.weights.as_ref()
    }
    pub fn untouched_seed(&self) -> &TrafficGeneratorSeed {
        &self.untouched
    }
    pub fn constructor_zero(&self) -> u32 {
        self.constructor_zero
    }
    /// Bind to the actual heap and reject an alias of another live owned lease.
    pub(crate) fn admit_allocation(&mut self, id: TrafficAllocationId) -> bool {
        if self.heap_domain.is_some_and(|domain| domain != id.domain)
            || self.records.as_ref().is_some_and(|p| {
                p.allocation == id
                    || p.records.iter().any(
                        |slot| matches!(slot, TrafficRecordSlot::Owned(r) if r.allocation == id),
                    )
            })
            || self.weights.as_ref().is_some_and(|p| p.allocation == id)
            || self
                .pending_record
                .as_ref()
                .is_some_and(|p| p.allocation == id)
        {
            return false;
        }
        self.heap_domain = Some(id.domain);
        true
    }
}

/// Exclusively borrows the actual destination storage. A partial read must
/// write the reached prefix even if it returns Err or unwinds afterwards.
/// No reader callback reenters actor/world mutation while that borrow exists.
pub trait TrafficDataReader {
    fn read_bounded(&mut self, destination: &mut [Option<u8>]) -> Result<bool, String>;
    fn read_null_bounded(&mut self, _bytes: usize) -> Result<bool, String> {
        Err("traffic read reached a null allocation".into())
    }
}
/// Narrow C_chunk read boundary: declared chunk end and physical bytes are
/// distinct. Whole-field bound skips do not advance; physical short reads do.
pub struct TrafficChunkReader<'a> {
    bytes: &'a [u8],
    declared_end: usize,
    position: usize,
    readable: bool,
}
impl<'a> TrafficChunkReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self::with_declared_end(bytes, bytes.len())
    }
    pub fn with_declared_end(bytes: &'a [u8], declared_end: usize) -> Self {
        Self {
            bytes,
            declared_end,
            position: 0,
            readable: true,
        }
    }
    pub fn set_readable(&mut self, readable: bool) {
        self.readable = readable;
    }
    pub fn position(&self) -> usize {
        self.position
    }
    #[cfg(test)]
    pub(crate) fn set_position_for_test(&mut self, position: usize) {
        self.position = position;
    }
    fn admits_read(&self, wanted: usize) -> Result<bool, String> {
        if !self.readable {
            return Ok(false);
        }
        let end = u32::try_from(self.declared_end)
            .map_err(|_| "traffic chunk end outside PC32 domain")?;
        let position = u32::try_from(self.position)
            .map_err(|_| "traffic chunk position outside PC32 domain")?;
        let wanted = i32::try_from(wanted)
            .map_err(|_| "traffic chunk read size outside admitted positive PC32 domain")?;
        // SUB lastEnd, Tell; JS chooses zero, then signed CMP/JG at404dc9..d9.
        // Offsets may have the high bit; it is their wrapped difference which
        // supplies the signed remaining length, not host usize subtraction.
        let remaining = (end.wrapping_sub(position) as i32).max(0);
        Ok(wanted <= remaining)
    }
}
impl TrafficDataReader for TrafficChunkReader<'_> {
    fn read_null_bounded(&mut self, bytes: usize) -> Result<bool, String> {
        if !self.admits_read(bytes)? {
            Ok(false)
        } else {
            Err("traffic read reached a null allocation".into())
        }
    }
    fn read_bounded(&mut self, destination: &mut [Option<u8>]) -> Result<bool, String> {
        if !self.admits_read(destination.len())? {
            return Ok(false);
        }
        let available = self
            .bytes
            .len()
            .saturating_sub(self.position)
            .min(destination.len());
        for (dst, src) in destination
            .iter_mut()
            .zip(&self.bytes[self.position.min(self.bytes.len())..][..available])
        {
            *dst = Some(*src);
        }
        self.position += available;
        // C_chunk returns the actual dtaRead result as bool; LoadData ignores it.
        Ok(available == destination.len())
    }
}

/// Game-global traffic flags written by `C_game::Init` (mafia-re
/// game/ai/C_person_traffic.h: `enable_taxi_driver`, `player_in_taxi`,
/// `global_valid_scream`). Native statics start cleared; `Init` stores
/// false/false/true with two conditional edges: disabling an enabled taxi
/// driver first clears all wavers (`ZrusVsechnyMavajici` 0x44fd10,
/// unported), and a non-null car is model-checked before it counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficGameFlags {
    taxi_driver_enabled: bool,
    player_in_taxi: bool,
    valid_scream: bool,
}

/// Waver-clear boundary for disabling an enabled taxi driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficFlagsError {
    WaverClearUnsupported,
}

impl TrafficGameFlags {
    pub fn taxi_driver_enabled(self) -> bool {
        self.taxi_driver_enabled
    }

    pub fn player_in_taxi(self) -> bool {
        self.player_in_taxi
    }

    pub fn valid_scream(self) -> bool {
        self.valid_scream
    }

    /// Original `C_game::Init` stores: taxi driver off, no player taxi,
    /// screams valid. With the boot-cleared state the disable edge never
    /// fires; an enabled driver reports its waver-clear boundary instead
    /// of silently dropping it.
    pub fn apply_game_init(&mut self) -> Result<(), TrafficFlagsError> {
        self.set_taxi_driver_enabled(false)?;
        self.player_in_taxi = false;
        self.valid_scream = true;
        Ok(())
    }

    fn set_taxi_driver_enabled(&mut self, enabled: bool) -> Result<(), TrafficFlagsError> {
        if !enabled && self.taxi_driver_enabled {
            return Err(TrafficFlagsError::WaverClearUnsupported);
        }
        self.taxi_driver_enabled = enabled;
        Ok(())
    }

    /// Original `PlayerInTaxi(NULL)` call from `Init`: clears without any
    /// model check. A non-null car needs the taxi-model check first and
    /// stays an explicit boundary at the call site.
    pub fn clear_player_in_taxi(&mut self) {
        self.player_in_taxi = false;
    }
}

/// Game-global city-traffic visibility for SETCITYTRAFFICVISIBLE (mafia-re
/// 113b5b6 game/mission/C_program.cpp:5677, C_game::SetTrafficVisible GAME
/// 0x5a8470 STUB). The native body only logs; the Rust flag records
/// `p[0] != 0` for the future traffic tick. Starts visible; the Init default
/// is unevidenced and stays open. No overlay entry; rides Process 0x6d3b0.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CityTrafficVisibleState {
    visible: bool,
}
impl Default for CityTrafficVisibleState {
    fn default() -> Self {
        Self { visible: true }
    }
}
impl CityTrafficVisibleState {
    pub fn visible(self) -> bool {
        self.visible
    }
    /// Original `SetTrafficVisible(p[0] != 0)` store.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
}

/// Pedestrian-generator runtime slice as a marker-only contract.
/// GAME RVAs 0x44e960/0x44ef70/0x44f090/0x44f740/0x44fcf0/0x44fd10/0x44fd50/0x450860
/// (mafia-re game/ai/C_person_traffic.cpp:77-138,246-320,
/// game/ai/C_person_traffic.h:67-150).
/// Only visibility, timer, predicate and lifecycle ordering are reconstructed.
/// `TryGoToAssHole` 0x44f0e0 stays unported: it needs the parking, navigation,
/// ground-collision and node owners, and is reported through
/// [`TrafficTickOutcome::needs_try_go_to_ass_hole`] instead.
/// `GeneratePassengerForTaxi` 0x44f7d0 (STUB) is likewise reported, never run.
pub type TrafficCarId = u64;

/// Live-view marker for one `C_traffic_element`. It borrows no heap object:
/// the authoritative pedestrian owner stays outside this contract.
/// `has_frame` mirrors the native `frame` null check
/// (mafia-re game/ai/C_person_traffic.cpp:84); `car` mirrors the `C_car *car`
/// identity compared by `IsCarHasOcas` (cpp:132-134).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrafficElementMarker {
    pub active: bool,
    pub dead: bool,
    pub fleeing: bool,
    pub waving: bool,
    pub has_frame: bool,
    pub car: Option<TrafficCarId>,
    pub pos_x: f32,
    pub pos_z: f32,
    pub dist: f32,
}

/// Marker for one `C_animations_machine_de_luxe` slot. The animation machine
/// owner is deferred, so this carries no behavior; it only counts slots for
/// [`clear_elements`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrafficAnimMarker;

/// Frame/pedestrian effects the generator reaches through backend owners.
/// Every method is a reached effect prefix: an `Err` stops the loop and keeps
/// the already-applied stores, exactly like the other actor backends.
/// Unimplemented owners keep the default, which reports the boundary.
pub trait TrafficElementBackend {
    fn set_frame_on(&mut self, _index: usize, _visible: bool) -> Result<(), String> {
        Err("traffic frame SetOn owner unavailable".into())
    }
    fn tick_element(&mut self, _index: usize, _time: u32) -> Result<(), String> {
        Err("traffic element Tick owner unavailable".into())
    }
    fn end_waving_on_taxi(&mut self, _index: usize) -> Result<(), String> {
        Err("traffic EndMavaniNaTaxi owner unavailable".into())
    }
    fn deactivate_element(&mut self, _index: usize) -> Result<(), String> {
        Err("traffic element Deactivate owner unavailable".into())
    }
}

/// Shared generator tick state: native zero-initialized statics
/// (mafia-re game/ai/C_person_traffic.cpp:42-48: `all_visible`, `t_horn`,
/// `try_do_rici`, `taxi_timer` start cleared).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficTickState {
    all_visible: bool,
    t_horn: i32,
    try_do_rici: i32,
    taxi_timer: i32,
}

impl TrafficTickState {
    pub fn all_visible(self) -> bool {
        self.all_visible
    }
    pub fn t_horn(self) -> i32 {
        self.t_horn
    }
    pub fn try_do_rici(self) -> i32 {
        self.try_do_rici
    }
    pub fn taxi_timer(self) -> i32 {
        self.taxi_timer
    }

    /// `C_traffic_generator::SetVisible`, GAME RVA 0x44e960
    /// (mafia-re game/ai/C_person_traffic.cpp:77-87).
    /// Equal input returns `Ok(false)` with no backend call; otherwise stores
    /// first, then visits elements last-to-first and applies `SetOn(visible)`
    /// to each active element with a frame.
    pub fn set_visible(
        &mut self,
        visible: bool,
        elements: &[TrafficElementMarker],
        backend: &mut impl TrafficElementBackend,
    ) -> Result<bool, String> {
        if self.all_visible == visible {
            return Ok(false);
        }
        self.all_visible = visible;
        for index in (0..elements.len()).rev() {
            if elements[index].active && elements[index].has_frame {
                backend.set_frame_on(index, visible)?;
            }
        }
        Ok(true)
    }

    /// `C_traffic_generator::TickElements` prefix skeleton, GAME RVA 0x44ef70
    /// (mafia-re game/ai/C_person_traffic.cpp:92-127).
    /// Hidden state returns early with timers and distances untouched. The
    /// visible path runs the `t_horn` countdown, the per-element horizontal
    /// distance plus `Tick` callback (last-to-first), the `try_do_rici`
    /// countdown, and the taxi branch. Parking/navigation/collision work
    /// (`TryGoToAssHole`) and passenger spawning (`GeneratePassengerForTaxi`)
    /// are deferred and reported through the returned outcome.
    /// Distance policy: `d.y` is zeroed before `Square` (cpp:109-110), so the
    /// sum is `dx*dx + dz*dz` in IEEE-754 binary32 evaluation order with no
    /// fused operation; any x87 extended-precision divergence stays open
    /// until original-runtime validation.
    pub fn tick_elements_prefix(
        &mut self,
        time: u32,
        cam_x: f32,
        cam_z: f32,
        elements: &mut [TrafficElementMarker],
        enable_taxi_driver: bool,
        player_in_taxi: bool,
        backend: &mut impl TrafficElementBackend,
    ) -> Result<TrafficTickOutcome, String> {
        if !self.all_visible {
            return Ok(TrafficTickOutcome::none());
        }
        if self.t_horn > 0 {
            if u64::from(time) > self.t_horn as u64 {
                self.t_horn = 0;
            } else {
                self.t_horn -= time as i32;
            }
        }
        for index in (0..elements.len()).rev() {
            if elements[index].active {
                let dx = cam_x - elements[index].pos_x;
                let dz = cam_z - elements[index].pos_z;
                elements[index].dist = dx * dx + dz * dz;
                backend.tick_element(index, time)?;
            }
        }
        // Native 32-bit SUB wraps; `time as i32` keeps the low 32 bits.
        self.try_do_rici = self.try_do_rici.wrapping_sub(time as i32);
        let mut outcome = TrafficTickOutcome::none();
        if self.try_do_rici < 0 {
            self.try_do_rici = 2123;
            outcome.needs_try_go_to_ass_hole = true;
        }
        if enable_taxi_driver && player_in_taxi {
            if self.taxi_timer < 0 {
                outcome.needs_taxi_passenger = true;
            } else {
                self.taxi_timer = self.taxi_timer.wrapping_sub(time as i32);
            }
        }
        Ok(outcome)
    }
}

/// Deferred-work markers produced by [`TrafficTickState::tick_elements_prefix`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficTickOutcome {
    /// `TryGoToAssHole` 0x44f0e0 reached: needs parking/navigation/
    /// ground-collision/node owners, so the caller handles it explicitly.
    pub needs_try_go_to_ass_hole: bool,
    /// `GeneratePassengerForTaxi` 0x44f7d0 (STUB) reached: needs the taxi
    /// passenger-spawn owner, so the caller handles it explicitly.
    pub needs_taxi_passenger: bool,
}

impl TrafficTickOutcome {
    pub fn none() -> Self {
        Self::default()
    }
}

/// `C_traffic_generator::IsCarHasOcas`, GAME RVA 0x44f090
/// (mafia-re game/ai/C_person_traffic.cpp:130-138).
/// Pure predicate: reverse scan for an active, live, non-fleeing element
/// walking to `car`.
pub fn is_car_has_ocas(elements: &[TrafficElementMarker], car: TrafficCarId) -> bool {
    for index in (0..elements.len()).rev() {
        let element = &elements[index];
        if element.active && !element.dead && !element.fleeing && element.car == Some(car) {
            return true;
        }
    }
    false
}

/// The PC's nine taxi model names, GAME RVA 0x44f740 string table
/// (mafia-re game/ai/C_person_traffic.cpp:251-270).
pub const TRAFFIC_TAXI_MODELS: [&str; 9] = [
    "taxi",
    "hotrodp4",
    "hotrodp5",
    "tbirdold",
    "fordhot",
    "phantomtaxi",
    "hotrodp2",
    "fordapicktaxi",
    "blackdragon",
];

impl TrafficGameFlags {
    /// `C_traffic_generator::PlayerInTaxi`, GAME RVA 0x44f740
    /// (mafia-re game/ai/C_person_traffic.cpp:246-280).
    /// Always clears first; a non-null model sets the flag only on a
    /// case-insensitive match against the nine PC taxi names.
    /// The `None` path is the original `PlayerInTaxi(NULL)` call from `Init`.
    pub fn apply_player_in_taxi(&mut self, model: Option<&str>) {
        self.player_in_taxi = false;
        let Some(model) = model else { return };
        for index in (0..TRAFFIC_TAXI_MODELS.len()).rev() {
            if model.eq_ignore_ascii_case(TRAFFIC_TAXI_MODELS[index]) {
                self.player_in_taxi = true;
                return;
            }
        }
    }

    /// `C_traffic_generator::SetGlobalValidScream`, GAME VA 0x47b780 (RVA
    /// 0x7b780; mafia-re game/ai/C_person_traffic.h:116-121 at pin 113b5b6):
    /// unconditional store of the pedestrians-may-scream flag. The
    /// `NPC_SHUTUP` nonzero-parameter edge is caller-owned.
    pub fn set_global_valid_scream(&mut self, valid: bool) {
        self.valid_scream = valid;
    }

    /// `C_traffic_generator::EnableTaxiDriver`, GAME RVA 0x44fcf0
    /// (mafia-re game/ai/C_person_traffic.cpp:282-288).
    /// Disabling an enabled driver first ends every waver through the backend
    /// (`ZrusVsechnyMavajici` edge), then stores unconditionally.
    pub fn enable_taxi_driver(
        &mut self,
        enable: bool,
        elements: &[TrafficElementMarker],
        backend: &mut impl TrafficElementBackend,
    ) -> Result<(), String> {
        if !enable && self.taxi_driver_enabled {
            zrus_vsechny_mavajici(elements, backend)?;
        }
        self.taxi_driver_enabled = enable;
        Ok(())
    }
}

/// `C_traffic_generator::ZrusVsechnyMavajici`, GAME RVA 0x44fd10
/// (mafia-re game/ai/C_person_traffic.cpp:291-299).
/// Ends the waving at a taxi of every waving pedestrian, last-to-first,
/// through the `EndMavaniNaTaxi` backend.
pub fn zrus_vsechny_mavajici(
    elements: &[TrafficElementMarker],
    backend: &mut impl TrafficElementBackend,
) -> Result<(), String> {
    for index in (0..elements.len()).rev() {
        if elements[index].waving {
            backend.end_waving_on_taxi(index)?;
        }
    }
    Ok(())
}

/// Dropped-slot counts for [`clear_elements`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TrafficClearOutcome {
    pub dropped_elements: usize,
    pub dropped_anims: usize,
}

/// `C_traffic_generator::ClearElements`, GAME RVA 0x44fd50
/// (mafia-re game/ai/C_person_traffic.cpp:301-309).
/// Forgets the pedestrians, then frees the animations last-to-first through
/// their owner. This marker contract drops both vectors and reports the
/// counts so the host animation owner can free its machines in native order.
pub fn clear_elements(
    elements: &mut Vec<TrafficElementMarker>,
    anims: &mut Vec<TrafficAnimMarker>,
) -> TrafficClearOutcome {
    let outcome = TrafficClearOutcome {
        dropped_elements: elements.len(),
        dropped_anims: anims.len(),
    };
    elements.clear();
    anims.clear();
    outcome
}

/// `C_traffic_generator::FreeDynCollElements`, GAME RVA 0x450860
/// (mafia-re game/ai/C_person_traffic.cpp:311-320).
/// Takes every active pedestrian out of the game, last-to-first, through the
/// `Deactivate` backend.
pub fn free_dyn_coll_elements(
    elements: &[TrafficElementMarker],
    backend: &mut impl TrafficElementBackend,
) -> Result<(), String> {
    for index in (0..elements.len()).rev() {
        if elements[index].active {
            backend.deactivate_element(index)?;
        }
    }
    Ok(())
}

/// Still-unimplemented traffic-generator calls. Entries name the original PC
/// addresses; the parking, navigation, ground-collision, node, passenger and
/// pedestrian-spawn owners stay outside this contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrafficGeneratorError {
    Unsupported { entry: u32 },
}

/// `C_traffic_generator::PlayerHoMaVytaseneho` (GAME VA 0x44e890, RVA
/// 0x4e890).
pub const PLAYER_HO_MA_VYTASENEHO_ENTRY: u32 = 0x44e890;
/// `C_traffic_generator::TryGoToAssHole` (GAME VA 0x44f0e0, RVA 0x4f0e0).
pub const TRY_GO_TO_ASS_HOLE_ENTRY: u32 = 0x44f0e0;
/// `C_traffic_generator::GeneratePassengerForTaxi` (GAME VA 0x44f7d0, RVA
/// 0x4f7d0).
pub const GENERATE_PASSENGER_FOR_TAXI_ENTRY: u32 = 0x44f7d0;
/// `C_traffic_generator::AddFreePerson` (GAME VA 0x450f00, RVA 0x50f00).
pub const ADD_FREE_PERSON_ENTRY: u32 = 0x450f00;
/// `C_traffic_generator::VyfunOdTeMasiny` (GAME VA 0x4512a0, RVA 0x512a0).
pub const VYFUN_OD_TE_MASINY_ENTRY: u32 = 0x4512a0;

/// Original `C_traffic_generator::PlayerHoMaVytaseneho` (mafia-re
/// game/ai/C_person_traffic.cpp:68-72 at pin 113b5b6; `STUB: GAME` VA
/// 0x44e890, RVA 0x4e890; "the player has it drawn", which `C_shvestky::Tick`
/// calls for a human on foot with a weapon in the hand): the original only
/// logs. The weapon/pedestrian owners stay outside this contract, so the
/// boundary is explicit.
pub fn player_ho_ma_vytaseneho() -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: PLAYER_HO_MA_VYTASENEHO_ENTRY,
    })
}

/// Original `C_traffic_generator::TryGoToAssHole` (mafia-re
/// game/ai/C_person_traffic.cpp:163-236 at pin 113b5b6; `FUNCTION: GAME` VA
/// 0x44f0e0, RVA 0x4f0e0): sends one free pedestrian to the nearest parked
/// car. The parking, navigation, ground-collision and node owners stay
/// outside this contract, so the boundary is explicit; the reached call is
/// reported through [`TrafficTickOutcome::needs_try_go_to_ass_hole`].
pub fn try_go_to_ass_hole() -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: TRY_GO_TO_ASS_HOLE_ENTRY,
    })
}

/// Original `C_traffic_generator::GeneratePassengerForTaxi` (mafia-re
/// game/ai/C_person_traffic.cpp:238-242 at pin 113b5b6; `STUB: GAME` VA
/// 0x44f7d0, RVA 0x4f7d0): the original only logs. The taxi-passenger-spawn
/// owner stays outside this contract, so the boundary is explicit; the
/// reached call is reported through
/// [`TrafficTickOutcome::needs_taxi_passenger`].
pub fn generate_passenger_for_taxi() -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: GENERATE_PASSENGER_FOR_TAXI_ENTRY,
    })
}

/// Original `C_traffic_generator::AddFreePerson` (mafia-re
/// game/ai/C_person_traffic.cpp:322-326 at pin 113b5b6; `STUB: GAME` VA
/// 0x450f00, RVA 0x50f00; a script command's): gives a pedestrian of that
/// model, or the actor, to the generators. The original only logs; the model
/// and pedestrian owners stay outside this contract, so the boundary is
/// explicit. Identities are opaque and never dereferenced.
pub fn add_free_person(
    _model: u32,
    _actor: u32,
    _unknown: bool,
) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: ADD_FREE_PERSON_ENTRY,
    })
}

/// Original `C_traffic_generator::VyfunOdTeMasiny` (mafia-re
/// game/ai/C_person_traffic.cpp:482-485 at pin 113b5b6; `STUB: GAME` VA
/// 0x4512a0, RVA 0x512a0; "blow off from that machine": the pedestrians
/// walking to the car sent away): the original only logs. The pedestrian
/// owner stays outside this contract, so the boundary is explicit.
pub fn vyfun_od_te_masiny(_car: TrafficCarId) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: VYFUN_OD_TE_MASINY_ENTRY,
    })
}

/// `C_traffic_generator::Explosion` entry, GAME RVA 0x50450 (VA 0x450450;
/// mafia-re game/ai/C_person_traffic.cpp:311-315 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): hits the
/// pedestrians in range of an explosion at `pos`.
pub const EXPLOSION_ENTRY: u32 = 0x450450;
/// `C_traffic_generator::NewPanic` entry, GAME RVA 0x509d0 (VA 0x4509d0;
/// mafia-re game/ai/C_person_traffic.cpp:328-332 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): panics the
/// pedestrians at a shot or an explosion at `pos`.
pub const NEW_PANIC_ENTRY: u32 = 0x4509d0;
/// `C_traffic_generator::PlayerPromluvilNaTehoChuja` entry, GAME RVA 0x4e870
/// (VA 0x44e870; mafia-re game/ai/C_person_traffic.cpp:500-502 at pin fe7e746;
/// `STUB: GAME` code in Game.exe, not a data offset and not another binary):
/// the player talks to the pedestrian of `frame` (opaque identity, never
/// dereferenced).
pub const PLAYER_PROMLUVIL_NA_TEHO_CHUJA_ENTRY: u32 = 0x44e870;
/// `C_traffic_element::ActivateFree` entry, GAME RVA 0x56fa0 (VA 0x456fa0;
/// mafia-re game/ai/C_person_traffic.cpp:488-492 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): activates the
/// pedestrian a human was made of.
pub const ACTIVATE_FREE_ENTRY: u32 = 0x456fa0;
/// `C_traffic_element::SetNewNode` entry, GAME RVA 0x51860 (VA 0x451860;
/// mafia-re game/ai/C_person_traffic.cpp:340-344 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): arms the next
/// walk node once `TryGoToAssHole` has set the way.
pub const SET_NEW_NODE_ENTRY: u32 = 0x451860;
/// `C_traffic_element::EndMavaniNaTaxi` entry, GAME RVA 0x52ca0 (VA 0x452ca0;
/// mafia-re game/ai/C_person_traffic.cpp:346-350 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): ends the
/// waving at a taxi.
pub const END_MAVANI_NA_TAXI_ENTRY: u32 = 0x452ca0;
/// `C_traffic_element::Deactivate` entry, GAME RVA 0x587c0 (VA 0x4587c0;
/// mafia-re game/ai/C_person_traffic.cpp:358-362 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): takes the
/// pedestrian out of the game.
pub const DEACTIVATE_TRAFFIC_ELEMENT_ENTRY: u32 = 0x4587c0;

/// Original `C_traffic_generator::Explosion` stub (mafia-re
/// game/ai/C_person_traffic.cpp:311-315 at pin fe7e746; `STUB: GAME` VA
/// 0x450450, RVA 0x50450): the original only logs. The pedestrian,
/// collision and scene owners stay outside this contract, so the boundary is
/// explicit. Positions are staged `[f32; 3]` triples, never native pointers.
pub fn traffic_explosion(
    _pos: [f32; 3],
    _range: f32,
    _power: f32,
) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: EXPLOSION_ENTRY,
    })
}

/// Original `C_traffic_generator::NewPanic` stub (mafia-re
/// game/ai/C_person_traffic.cpp:328-332 at pin fe7e746; `STUB: GAME` VA
/// 0x4509d0, RVA 0x509d0): the original only logs. The pedestrian owner
/// stays outside this contract, so the boundary is explicit.
pub fn traffic_new_panic(_pos: [f32; 3]) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: NEW_PANIC_ENTRY,
    })
}

/// Original `C_traffic_generator::PlayerPromluvilNaTehoChuja` stub (mafia-re
/// game/ai/C_person_traffic.cpp:500-502 at pin fe7e746; `STUB: GAME` VA
/// 0x44e870, RVA 0x4e870; "the player spoke to that dude"): the original only
/// logs. The frame/human owners stay outside this contract, so the boundary
/// is explicit. The frame identity is opaque and never dereferenced.
pub fn player_promluvil_na_teho_chuja(_frame: u32) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: PLAYER_PROMLUVIL_NA_TEHO_CHUJA_ENTRY,
    })
}

/// Original `C_traffic_element::ActivateFree` stub (mafia-re
/// game/ai/C_person_traffic.cpp:488-492 at pin fe7e746; `STUB: GAME` VA
/// 0x456fa0, RVA 0x56fa0): the original only logs. The human/pedestrian
/// owners stay outside this contract, so the boundary is explicit.
pub fn activate_free_traffic_element(
    _first: bool,
    _second: bool,
) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: ACTIVATE_FREE_ENTRY,
    })
}

/// Original `C_traffic_element::SetNewNode` stub (mafia-re
/// game/ai/C_person_traffic.cpp:340-344 at pin fe7e746; `STUB: GAME` VA
/// 0x451860, RVA 0x51860): the original only logs. The navigation and
/// ground-collision owners stay outside this contract, so the boundary is
/// explicit.
pub fn set_new_node_traffic_element() -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: SET_NEW_NODE_ENTRY,
    })
}

/// Original `C_traffic_element::EndMavaniNaTaxi` stub (mafia-re
/// game/ai/C_person_traffic.cpp:346-350 at pin fe7e746; `STUB: GAME` VA
/// 0x452ca0, RVA 0x52ca0): the original only logs. The animation owner stays
/// outside this contract, so the boundary is explicit.
pub fn end_mavani_na_taxi_element() -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: END_MAVANI_NA_TAXI_ENTRY,
    })
}

/// Original `C_traffic_element::Deactivate` stub (mafia-re
/// game/ai/C_person_traffic.cpp:358-362 at pin fe7e746; `STUB: GAME` VA
/// 0x4587c0, RVA 0x587c0): the original only logs. The game/world owners stay
/// outside this contract, so the boundary is explicit.
pub fn deactivate_traffic_element() -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: DEACTIVATE_TRAFFIC_ELEMENT_ENTRY,
    })
}

/// `C_don_ocas::Hit` entry, GAME RVA 0x4ccf0 (VA 0x44ccf0; mafia-re
/// game/ai/C_person_traffic.cpp:473-479 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): forwards to
/// `C_actor::Hit`.
pub const DON_OCAS_HIT_ENTRY: u32 = 0x44ccf0;
/// `C_don_ocas::GetBoundPoints2D` entry, GAME RVA 0x4d1c0 (VA 0x44d1c0;
/// mafia-re game/ai/C_person_traffic.cpp:481-486 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): forwards to
/// `C_actor::GetBoundPoints2D`.
pub const DON_OCAS_GET_BOUND_POINTS_2D_ENTRY: u32 = 0x44d1c0;

/// Original `C_don_ocas::Hit` stub (mafia-re
/// game/ai/C_person_traffic.cpp:473-479 at pin fe7e746; `STUB: GAME` VA
/// 0x44ccf0, RVA 0x4ccf0): the original only logs and the `C_actor` base
/// owner stays outside this contract, so the boundary is explicit. Positions
/// are staged `[f32; 3]` triples and actor/frame identities are opaque `u32`
/// values, never dereferenced.
pub fn don_ocas_hit(
    _kind: i32,
    _v1: [f32; 3],
    _v2: [f32; 3],
    _v3: [f32; 3],
    _power: f32,
    _actor: u32,
    _unknown: u32,
    _frame: u32,
) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: DON_OCAS_HIT_ENTRY,
    })
}

/// Original `C_don_ocas::GetBoundPoints2D` stub (mafia-re
/// game/ai/C_person_traffic.cpp:481-486 at pin fe7e746; `STUB: GAME` VA
/// 0x44d1c0, RVA 0x4d1c0): the original only logs and the `C_actor` base
/// owner stays outside this contract, so the boundary is explicit. The frame
/// identity is opaque and never dereferenced; the output point list has no
/// typed owner here.
pub fn don_ocas_get_bound_points_2d(_frame: u32) -> Result<(), TrafficGeneratorError> {
    Err(TrafficGeneratorError::Unsupported {
        entry: DON_OCAS_GET_BOUND_POINTS_2D_ENTRY,
    })
}

/// `C_don_ocas` actor-kind marker, GAME RVA 0x44c640
/// (mafia-re game/ai/C_person_traffic.cpp:50-54,
/// game/actors/C_actor.h:33).
/// The constructor only stores `type = ACTOR_DON_OCAS` (0x13); the `C_actor`
/// base owner stays outside this contract, so the kind is carried as a plain
/// value with no heap state.
///
/// No `Drop` impl is filed for `C_don_ocas::~C_don_ocas` (GAME 0x44c680,
/// cpp:56-60): the native body is empty (it only restores the vtable), holds
/// no state and performs no effect, so a no-op function would claim credit
/// for an unobservable helper. It stays documented here instead of ported.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DonOcasKind {
    pub actor_type: u32,
}

impl DonOcasKind {
    /// Actor-type tag stored by the constructor (`ACTOR_DON_OCAS`,
    /// game/actors/C_actor.h:33).
    pub const ACTOR_TYPE: u32 = 0x13;

    /// `C_don_ocas::C_don_ocas`, GAME RVA 0x44c640
    /// (mafia-re game/ai/C_person_traffic.cpp:50-54): stores the kind tag.
    pub fn new() -> Self {
        Self {
            actor_type: Self::ACTOR_TYPE,
        }
    }
}

impl Default for DonOcasKind {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_init_stores_false_false_true() {
        let mut flags = TrafficGameFlags::default();
        flags.apply_game_init().unwrap();
        assert!(!flags.taxi_driver_enabled());
        assert!(!flags.player_in_taxi());
        assert!(flags.valid_scream());
        // Repeating init over boot state stays a pure no-op success.
        flags.apply_game_init().unwrap();
    }

    #[test]
    fn disabling_enabled_driver_reports_waver_boundary() {
        let mut flags = TrafficGameFlags {
            taxi_driver_enabled: true,
            player_in_taxi: false,
            valid_scream: false,
        };
        assert_eq!(
            flags.apply_game_init(),
            Err(TrafficFlagsError::WaverClearUnsupported)
        );
        // The refused transition leaves prior state untouched.
        assert!(flags.taxi_driver_enabled());
    }

    #[test]
    fn clearing_taxi_needs_no_model_check() {
        let mut flags = TrafficGameFlags {
            taxi_driver_enabled: false,
            player_in_taxi: true,
            valid_scream: true,
        };
        flags.clear_player_in_taxi();
        assert!(!flags.player_in_taxi());
    }

    #[test]
    fn city_traffic_visible_starts_visible_and_stores_bool() {
        // Hand-derived: default visible, SetTrafficVisible(p[0] != 0).
        let mut state = CityTrafficVisibleState::default();
        assert!(state.visible());
        state.set_visible(false);
        assert!(!state.visible());
        state.set_visible(true);
        assert!(state.visible());
    }

    #[test]
    fn traffic_slice_backends_and_markers_cover_setvisible_tick_predicate_taxi_and_lifecycle() {
        #[derive(Debug, PartialEq)]
        enum Call {
            SetOn(usize, bool),
            Tick(usize, u32),
            EndWaving(usize),
            Deactivate(usize),
        }
        #[derive(Default)]
        struct Log {
            calls: Vec<Call>,
            fail_at: Option<usize>,
        }
        impl TrafficElementBackend for Log {
            fn set_frame_on(&mut self, index: usize, visible: bool) -> Result<(), String> {
                if self.fail_at == Some(self.calls.len()) {
                    return Err("frame SetOn unavailable".into());
                }
                self.calls.push(Call::SetOn(index, visible));
                Ok(())
            }
            fn tick_element(&mut self, index: usize, time: u32) -> Result<(), String> {
                if self.fail_at == Some(self.calls.len()) {
                    return Err("element Tick unavailable".into());
                }
                self.calls.push(Call::Tick(index, time));
                Ok(())
            }
            fn end_waving_on_taxi(&mut self, index: usize) -> Result<(), String> {
                if self.fail_at == Some(self.calls.len()) {
                    return Err("EndMavaniNaTaxi unavailable".into());
                }
                self.calls.push(Call::EndWaving(index));
                Ok(())
            }
            fn deactivate_element(&mut self, index: usize) -> Result<(), String> {
                if self.fail_at == Some(self.calls.len()) {
                    return Err("Deactivate unavailable".into());
                }
                self.calls.push(Call::Deactivate(index));
                Ok(())
            }
        }
        fn element(
            active: bool,
            dead: bool,
            fleeing: bool,
            waving: bool,
            has_frame: bool,
            car: Option<TrafficCarId>,
            x: f32,
            z: f32,
        ) -> TrafficElementMarker {
            TrafficElementMarker {
                active,
                dead,
                fleeing,
                waving,
                has_frame,
                car,
                pos_x: x,
                pos_z: z,
                dist: -1.0,
            }
        }

        // SetVisible 0x44e960: equal input is a no-op with no backend call.
        let mut state = TrafficTickState::default();
        assert!(!state.all_visible());
        let elements = [element(true, false, false, false, true, None, 0.0, 0.0)];
        let mut log = Log::default();
        assert_eq!(state.set_visible(false, &elements, &mut log), Ok(false));
        assert!(log.calls.is_empty());

        // Only active elements with a frame are visited, last-to-first.
        let elements = [
            element(true, false, false, false, true, None, 0.0, 0.0),
            element(true, false, false, false, false, None, 0.0, 0.0),
            element(false, false, false, false, true, None, 0.0, 0.0),
            element(true, false, false, false, true, None, 0.0, 0.0),
        ];
        assert_eq!(state.set_visible(true, &elements, &mut log), Ok(true));
        assert!(state.all_visible());
        assert_eq!(log.calls, [Call::SetOn(3, true), Call::SetOn(0, true)]);
        // A failed SetOn keeps the store and the already-applied prefix.
        log.calls.clear();
        log.fail_at = Some(0);
        assert!(state.set_visible(false, &elements, &mut log).is_err());
        assert!(!state.all_visible());
        assert!(log.calls.is_empty());

        // TickElements 0x44ef70: hidden state returns early, timers/dist untouched.
        let mut hidden = TrafficTickState::default();
        hidden.t_horn = 500;
        hidden.try_do_rici = 7;
        let mut elems = [element(true, false, false, false, true, None, 1.0, 2.0)];
        let mut log = Log::default();
        let outcome = hidden
            .tick_elements_prefix(100, 0.0, 0.0, &mut elems, false, false, &mut log)
            .unwrap();
        assert_eq!(outcome, TrafficTickOutcome::none());
        assert_eq!(hidden.t_horn(), 500);
        assert_eq!(hidden.try_do_rici(), 7);
        assert_eq!(elems[0].dist, -1.0);
        assert!(log.calls.is_empty());

        // Visible prefix: t_horn clamp arm, dist write before Tick, try_do_rici
        // wrap to 2123 with the TryGoToAssHole marker, taxi countdown arm.
        let mut ticking = TrafficTickState {
            all_visible: true,
            t_horn: 50,
            try_do_rici: 30,
            taxi_timer: 400,
        };
        let mut elems = [
            element(true, false, false, false, true, None, 1.0, 0.0),
            element(false, false, false, false, true, None, 9.0, 9.0),
        ];
        let mut log = Log::default();
        let outcome = ticking
            .tick_elements_prefix(100, 4.0, 0.0, &mut elems, true, true, &mut log)
            .unwrap();
        assert_eq!(ticking.t_horn(), 0);
        assert_eq!(elems[0].dist, 9.0); // (4-1)^2 + 0^2, y zeroed per cpp:109-110.
        assert_eq!(elems[1].dist, -1.0); // inactive elements keep their distance.
        assert_eq!(log.calls, [Call::Tick(0, 100)]);
        assert_eq!(ticking.try_do_rici(), 2123);
        assert!(outcome.needs_try_go_to_ass_hole);
        assert!(!outcome.needs_taxi_passenger);
        assert_eq!(ticking.taxi_timer(), 300);
        // Taxi branch: negative timer reports the deferred passenger spawn.
        ticking.taxi_timer = -1;
        let outcome = ticking
            .tick_elements_prefix(10, 4.0, 0.0, &mut elems, true, true, &mut log)
            .unwrap();
        assert!(outcome.needs_taxi_passenger);
        assert_eq!(ticking.taxi_timer(), -1);
        // A failed Tick keeps the already-written distance.
        log.fail_at = Some(log.calls.len());
        let before = ticking.t_horn();
        assert!(
            ticking
                .tick_elements_prefix(10, 4.0, 0.0, &mut elems, false, false, &mut log)
                .is_err()
        );
        assert_eq!(elems[0].dist, 9.0);
        assert_eq!(ticking.t_horn(), before);

        // IsCarHasOcas 0x44f090: active && !dead && !fleeing && car match.
        let walkers = [
            element(true, false, false, false, true, Some(7), 0.0, 0.0),
            element(true, true, false, false, true, Some(7), 0.0, 0.0),
            element(true, false, true, false, true, Some(7), 0.0, 0.0),
            element(false, false, false, false, true, Some(7), 0.0, 0.0),
            element(true, false, false, false, true, Some(9), 0.0, 0.0),
        ];
        assert!(is_car_has_ocas(&walkers, 7));
        assert!(is_car_has_ocas(&walkers, 9));
        assert!(!is_car_has_ocas(&walkers, 11));
        assert!(!is_car_has_ocas(&[], 7));

        // PlayerInTaxi 0x44f740: None clears; all nine PC names match
        // case-insensitively; other models clear.
        let mut flags = TrafficGameFlags::default();
        for model in TRAFFIC_TAXI_MODELS {
            flags.apply_player_in_taxi(Some(&model.to_ascii_uppercase()));
            assert!(flags.player_in_taxi(), "{model}");
        }
        flags.apply_player_in_taxi(Some("sedan"));
        assert!(!flags.player_in_taxi());
        flags.apply_player_in_taxi(Some("Taxi"));
        assert!(flags.player_in_taxi());
        flags.apply_player_in_taxi(None);
        assert!(!flags.player_in_taxi());

        // EnableTaxiDriver 0x44fcf0: enabling stores without waver calls.
        let wavers = [
            element(true, false, false, true, true, None, 0.0, 0.0),
            element(true, false, false, false, true, None, 0.0, 0.0),
            element(true, false, false, true, true, None, 0.0, 0.0),
        ];
        let mut log = Log::default();
        flags.enable_taxi_driver(true, &wavers, &mut log).unwrap();
        assert!(flags.taxi_driver_enabled());
        assert!(log.calls.is_empty());
        // Disabling an enabled driver ends wavers last-to-first, then stores.
        flags.enable_taxi_driver(false, &wavers, &mut log).unwrap();
        assert!(!flags.taxi_driver_enabled());
        assert_eq!(log.calls, [Call::EndWaving(2), Call::EndWaving(0)]);
        // Disabling an already-disabled driver stores without backend calls.
        log.calls.clear();
        flags.enable_taxi_driver(false, &wavers, &mut log).unwrap();
        assert!(log.calls.is_empty());
        // A failed waver-clear keeps the driver enabled.
        flags.enable_taxi_driver(true, &wavers, &mut log).unwrap();
        log.fail_at = Some(0);
        assert!(flags.enable_taxi_driver(false, &wavers, &mut log).is_err());
        assert!(flags.taxi_driver_enabled());

        // ZrusVsechnyMavajici 0x44fd10 standalone: wavers only, reversed.
        let mut log = Log::default();
        zrus_vsechny_mavajici(&wavers, &mut log).unwrap();
        assert_eq!(log.calls, [Call::EndWaving(2), Call::EndWaving(0)]);

        // ClearElements 0x44fd50: drops both vectors and reports counts.
        let mut elems = vec![element(true, false, false, false, true, None, 0.0, 0.0)];
        let mut anims = vec![TrafficAnimMarker, TrafficAnimMarker, TrafficAnimMarker];
        let outcome = clear_elements(&mut elems, &mut anims);
        assert_eq!(
            outcome,
            TrafficClearOutcome {
                dropped_elements: 1,
                dropped_anims: 3
            }
        );
        assert!(elems.is_empty() && anims.is_empty());

        // FreeDynCollElements 0x450860: active elements only, last-to-first.
        let mixed = [
            element(true, false, false, false, true, None, 0.0, 0.0),
            element(false, false, false, false, true, None, 0.0, 0.0),
            element(true, false, false, false, true, None, 0.0, 0.0),
        ];
        let mut log = Log::default();
        free_dyn_coll_elements(&mixed, &mut log).unwrap();
        assert_eq!(log.calls, [Call::Deactivate(2), Call::Deactivate(0)]);
    }

    #[test]
    fn don_ocas_ctor_stores_don_ocas_type() {
        // C_don_ocas::C_don_ocas 0x44c640 stores ACTOR_DON_OCAS (0x13).
        assert_eq!(DonOcasKind::new().actor_type, DonOcasKind::ACTOR_TYPE);
        assert_eq!(DonOcasKind::ACTOR_TYPE, 0x13);
        assert_eq!(DonOcasKind::default().actor_type, 0x13);
    }

    #[test]
    fn rva_7b780_set_global_valid_scream_stores_flag() {
        // C_traffic_generator::SetGlobalValidScream GAME VA 0x47b780 (RVA
        // 0x7b780).
        let mut flags = TrafficGameFlags::default();
        assert!(!flags.valid_scream());
        flags.set_global_valid_scream(true);
        assert!(flags.valid_scream());
        flags.set_global_valid_scream(false);
        assert!(!flags.valid_scream());
        // Repeating the store is idempotent.
        flags.set_global_valid_scream(false);
        assert!(!flags.valid_scream());
    }

    #[test]
    fn rva_4e890_player_ho_ma_vytaseneho_reports_unsupported() {
        // C_traffic_generator::PlayerHoMaVytaseneho GAME VA 0x44e890 (RVA
        // 0x4e890).
        assert_eq!(PLAYER_HO_MA_VYTASENEHO_ENTRY, 0x44e890);
        assert_eq!(
            player_ho_ma_vytaseneho(),
            Err(TrafficGeneratorError::Unsupported { entry: 0x44e890 })
        );
    }

    #[test]
    fn rva_4f0e0_try_go_to_ass_hole_reports_unsupported() {
        // C_traffic_generator::TryGoToAssHole GAME VA 0x44f0e0 (RVA 0x4f0e0).
        assert_eq!(TRY_GO_TO_ASS_HOLE_ENTRY, 0x44f0e0);
        assert_eq!(
            try_go_to_ass_hole(),
            Err(TrafficGeneratorError::Unsupported { entry: 0x44f0e0 })
        );
    }

    #[test]
    fn rva_4f7d0_generate_passenger_for_taxi_reports_unsupported() {
        // C_traffic_generator::GeneratePassengerForTaxi GAME VA 0x44f7d0
        // (RVA 0x4f7d0).
        assert_eq!(GENERATE_PASSENGER_FOR_TAXI_ENTRY, 0x44f7d0);
        assert_eq!(
            generate_passenger_for_taxi(),
            Err(TrafficGeneratorError::Unsupported { entry: 0x44f7d0 })
        );
    }

    #[test]
    fn rva_50f00_add_free_person_reports_unsupported() {
        // C_traffic_generator::AddFreePerson GAME VA 0x450f00 (RVA 0x50f00).
        assert_eq!(ADD_FREE_PERSON_ENTRY, 0x450f00);
        assert_eq!(
            add_free_person(1, 2, false),
            Err(TrafficGeneratorError::Unsupported { entry: 0x450f00 })
        );
    }

    #[test]
    fn rva_512a0_vyfun_od_te_masiny_reports_unsupported() {
        // C_traffic_generator::VyfunOdTeMasiny GAME VA 0x4512a0 (RVA 0x512a0).
        assert_eq!(VYFUN_OD_TE_MASINY_ENTRY, 0x4512a0);
        assert_eq!(
            vyfun_od_te_masiny(7),
            Err(TrafficGeneratorError::Unsupported { entry: 0x4512a0 })
        );
    }

    #[test]
    fn rva_50450_explosion_reports_unsupported() {
        // C_traffic_generator::Explosion GAME RVA 0x50450 (VA 0x450450).
        assert_eq!(EXPLOSION_ENTRY, 0x450450);
        assert_eq!(
            traffic_explosion([0.0, 0.0, 0.0], 10.0, 5.0),
            Err(TrafficGeneratorError::Unsupported { entry: 0x450450 })
        );
        assert_eq!(
            traffic_explosion([1.0, -2.0, 3.0], 0.0, 0.0),
            Err(TrafficGeneratorError::Unsupported { entry: 0x450450 })
        );
    }

    #[test]
    fn rva_509d0_new_panic_reports_unsupported() {
        // C_traffic_generator::NewPanic GAME RVA 0x509d0 (VA 0x4509d0).
        assert_eq!(NEW_PANIC_ENTRY, 0x4509d0);
        assert_eq!(
            traffic_new_panic([4.0, 5.0, 6.0]),
            Err(TrafficGeneratorError::Unsupported { entry: 0x4509d0 })
        );
    }

    #[test]
    fn rva_4e870_player_promluvil_reports_unsupported() {
        // C_traffic_generator::PlayerPromluvilNaTehoChuja GAME RVA 0x4e870
        // (VA 0x44e870).
        assert_eq!(PLAYER_PROMLUVIL_NA_TEHO_CHUJA_ENTRY, 0x44e870);
        assert_eq!(
            player_promluvil_na_teho_chuja(0x1234),
            Err(TrafficGeneratorError::Unsupported { entry: 0x44e870 })
        );
    }

    #[test]
    fn rva_56fa0_activate_free_reports_unsupported() {
        // C_traffic_element::ActivateFree GAME RVA 0x56fa0 (VA 0x456fa0).
        assert_eq!(ACTIVATE_FREE_ENTRY, 0x456fa0);
        assert_eq!(
            activate_free_traffic_element(true, false),
            Err(TrafficGeneratorError::Unsupported { entry: 0x456fa0 })
        );
        assert_eq!(
            activate_free_traffic_element(false, false),
            Err(TrafficGeneratorError::Unsupported { entry: 0x456fa0 })
        );
    }

    #[test]
    fn rva_51860_set_new_node_reports_unsupported() {
        // C_traffic_element::SetNewNode GAME RVA 0x51860 (VA 0x451860).
        assert_eq!(SET_NEW_NODE_ENTRY, 0x451860);
        assert_eq!(
            set_new_node_traffic_element(),
            Err(TrafficGeneratorError::Unsupported { entry: 0x451860 })
        );
    }

    #[test]
    fn rva_52ca0_end_mavani_reports_unsupported() {
        // C_traffic_element::EndMavaniNaTaxi GAME RVA 0x52ca0 (VA 0x452ca0).
        assert_eq!(END_MAVANI_NA_TAXI_ENTRY, 0x452ca0);
        assert_eq!(
            end_mavani_na_taxi_element(),
            Err(TrafficGeneratorError::Unsupported { entry: 0x452ca0 })
        );
    }

    #[test]
    fn rva_587c0_deactivate_element_reports_unsupported() {
        // C_traffic_element::Deactivate GAME RVA 0x587c0 (VA 0x4587c0).
        assert_eq!(DEACTIVATE_TRAFFIC_ELEMENT_ENTRY, 0x4587c0);
        assert_eq!(
            deactivate_traffic_element(),
            Err(TrafficGeneratorError::Unsupported { entry: 0x4587c0 })
        );
    }

    #[test]
    fn rva_4ccf0_don_ocas_hit_reports_unsupported() {
        // C_don_ocas::Hit GAME RVA 0x4ccf0 (VA 0x44ccf0).
        assert_eq!(DON_OCAS_HIT_ENTRY, 0x44ccf0);
        assert_eq!(
            don_ocas_hit(
                1,
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                5.0,
                7,
                0,
                9
            ),
            Err(TrafficGeneratorError::Unsupported { entry: 0x44ccf0 })
        );
    }

    #[test]
    fn rva_4d1c0_don_ocas_get_bound_points_reports_unsupported() {
        // C_don_ocas::GetBoundPoints2D GAME RVA 0x4d1c0 (VA 0x44d1c0).
        assert_eq!(DON_OCAS_GET_BOUND_POINTS_2D_ENTRY, 0x44d1c0);
        assert_eq!(
            don_ocas_get_bound_points_2d(3),
            Err(TrafficGeneratorError::Unsupported { entry: 0x44d1c0 })
        );
    }
}
