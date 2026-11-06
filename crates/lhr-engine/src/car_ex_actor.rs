//! C_car_ex extended storage for the original PC actor lifecycle (kind 36).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x42e240, vtable VA0x6237d8 (C_actor), 0x227c bytes
//! (PS2's 0x24f0): a C_car (0x221c bytes) plus the 0x60-byte truck tail.
//! Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference sibling
//! mafia-re @ 113b5b6 game/vehicles/C_car_ex.h:9 (vtable), :41-50 (tail layout),
//! :30-31 (DCAddPerson/DCDelPerson) + game/vehicles/C_car_ex.cpp:9-12
//! (constructor), :63-67 (DCAddPerson STUB 0x42f160), :69-73 (DCDelPerson STUB
//! 0x42f320); CanUseTruck GAME 0x47b6d0; factory game/mission/Mission.cpp:211
//! (CreateActor GAME 0x53f7d0, ACTOR_CAR_EX 0x24). The car prefix reuses the
//! established Vehicle compatibility backend (see construct_car_ex); only the
//! tail bytes and the truck passenger owners live here. Tail offsets below are
//! absolute byte offsets into the derived object; the tail starts at 0x221c.
//! No PS2 member layout is assumed.

use crate::car_actor::CarSeed;
use crate::script_scheduler::ActorReferenceId;

/// Actual untouched allocation tail of one 0x227c-byte car_ex object: the car
/// prefix seed plus the 0x60 bytes past the 0x221c-byte C_car. The stub
/// constructor stores nothing past the car prefix; every tail word is admitted
/// untouched allocation, not interpreted native state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CarExSeed {
    pub car: CarSeed,
    pub bytes_221c_227b: [u8; 0x60],
}

/// Owned car_ex tail after construction. The native dynamic-collision vector
/// (+0x2258) stays an opaque retained word, never a host pointer; the typed
/// passenger owners stand in for the DCAddPerson/DCDelPerson human effects.
#[derive(Debug)]
pub struct CarExState {
    tail: [u8; 0x60],
    passengers: Vec<ActorReferenceId>,
}

impl CarExState {
    pub(crate) fn allocation(seed: CarExSeed) -> Self {
        Self {
            tail: seed.bytes_221c_227b,
            passengers: Vec::new(),
        }
    }

    pub fn tail_bytes(&self) -> &[u8; 0x60] {
        &self.tail
    }

    fn tail_word(&self, offset: usize) -> u32 {
        let relative = offset - 0x221c;
        u32::from_le_bytes(
            self.tail[relative..relative + 4]
                .try_into()
                .expect("car_ex word"),
        )
    }

    fn tail_floats(&self, offset: usize) -> [f32; 3] {
        [0, 4, 8].map(|delta| {
            f32::from_le_bytes(
                self.tail[offset - 0x221c + delta..offset - 0x221c + delta + 4]
                    .try_into()
                    .expect("car_ex vector"),
            )
        })
    }

    /// Truck-use flag at +0x223c (mafia-re C_car_ex.h:42).
    pub fn truck_usable_flag(&self) -> bool {
        self.tail[0x223c - 0x221c] != 0
    }

    /// Opaque dynamic-collision vector word at +0x2258 (mafia-re C_car_ex.h:47-49).
    /// Retained verbatim; never dereferenced on the host.
    pub fn dyn_colls_word(&self) -> u32 {
        self.tail_word(0x2258)
    }

    /// Original C_car_ex::CanUseTruck (mafia-re C_car_ex.h:35-38, GAME 0x47b6d0):
    /// the flag word and the collision vector word are both set.
    pub fn can_use_truck(&self) -> bool {
        self.truck_usable_flag() && self.dyn_colls_word() != 0
    }

    /// Truck bed position in the truck frame at +0x2240 (mafia-re C_car_ex.h:45).
    pub fn truck_pos(&self) -> [f32; 3] {
        self.tail_floats(0x2240)
    }

    /// Truck bed direction in the truck frame at +0x224c (mafia-re C_car_ex.h:46).
    pub fn truck_dir(&self) -> [f32; 3] {
        self.tail_floats(0x224c)
    }

    /// Live truck passenger owners placed by DCAddPerson.
    pub fn passengers(&self) -> &[ActorReferenceId] {
        &self.passengers
    }

    /// Host-only passenger manifest (native C_car_ex::DCAddPerson/DCDelPerson,
    /// mafia-re C_car_ex.cpp:63-73, are void log-only STUBs with no state and
    /// take no function credit): retain one more passenger owner. Duplicate
    /// adds report false and keep a single owner, since Rust owners are unique.
    pub(crate) fn add_person(&mut self, human: ActorReferenceId) -> bool {
        if self.passengers.contains(&human) {
            return false;
        }
        self.passengers.push(human);
        true
    }

    /// Host-only manifest release (native DCDelPerson is a void log-only STUB,
    /// no function credit): release the passenger owner. Absent owners report
    /// false and change nothing.
    pub(crate) fn remove_person(&mut self, human: ActorReferenceId) -> bool {
        if let Some(index) = self.passengers.iter().position(|owner| *owner == human) {
            self.passengers.remove(index);
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn human(identity: u32) -> ActorReferenceId {
        // Stable identity (from_existing pins incarnation 0): the
        // ActorLifetime helper mints a fresh global incarnation per call,
        // which would defeat duplicate detection under test.
        ActorReferenceId::from_existing(identity)
    }

    fn seed_with(tail: [u8; 0x60]) -> CarExSeed {
        CarExSeed {
            car: CarSeed {
                allocator_fill: 0xa5,
                constructor_allocator_tag: 0x79,
                vehicle_allocator_tag: 0xb3,
            },
            bytes_221c_227b: tail,
        }
    }

    #[test]
    fn constructor_retains_tail_and_starts_empty() {
        let state = CarExState::allocation(seed_with([0x3c; 0x60]));
        assert_eq!(state.tail_bytes(), &[0x3c; 0x60]);
        assert!(state.passengers().is_empty());
        assert!(state.truck_usable_flag());
        assert_eq!(state.dyn_colls_word(), 0x3c3c3c3c);
        assert!(state.can_use_truck());
    }

    #[test]
    fn can_use_truck_needs_flag_and_collisions() {
        for (flag, collisions, expected) in [
            (0u8, 0u32, false),
            (1u8, 0u32, false),
            (0u8, 1u32, false),
            (1u8, 1u32, true),
        ] {
            let mut tail = [0u8; 0x60];
            tail[0x223c - 0x221c] = flag;
            tail[0x2258 - 0x221c..0x225c - 0x221c].copy_from_slice(&collisions.to_le_bytes());
            let state = CarExState::allocation(seed_with(tail));
            assert_eq!(state.truck_usable_flag(), flag != 0);
            assert_eq!(state.dyn_colls_word(), collisions);
            assert_eq!(state.can_use_truck(), expected);
        }
    }

    #[test]
    fn truck_vectors_decode_absolute_tail_offsets() {
        let mut tail = [0u8; 0x60];
        for (offset, value) in [
            (0x2240usize, 1.0f32),
            (0x2244, 2.0),
            (0x2248, 3.0),
            (0x224c, 4.0),
            (0x2250, 5.0),
            (0x2254, 6.0),
        ] {
            tail[offset - 0x221c..offset - 0x221c + 4].copy_from_slice(&value.to_le_bytes());
        }
        let state = CarExState::allocation(seed_with(tail));
        assert_eq!(state.truck_pos(), [1.0, 2.0, 3.0]);
        assert_eq!(state.truck_dir(), [4.0, 5.0, 6.0]);
    }

    #[test]
    fn passenger_owners_add_once_and_remove() {
        let mut state = CarExState::allocation(seed_with([0; 0x60]));
        assert!(state.add_person(human(7)));
        assert!(!state.add_person(human(7)));
        assert!(state.add_person(human(9)));
        assert_eq!(state.passengers(), &[human(7), human(9)]);
        assert!(!state.remove_person(human(11)));
        assert!(state.remove_person(human(7)));
        assert_eq!(state.passengers(), &[human(9)]);
        assert!(!state.remove_person(human(7)));
    }
}
