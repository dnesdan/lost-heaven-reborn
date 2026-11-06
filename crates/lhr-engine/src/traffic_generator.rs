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
}
