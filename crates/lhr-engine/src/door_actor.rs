//! C_door derived storage for the original PC actor lifecycle (kind 6).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor VA0x439290, Init0x4393a0, LoadData0x43b030,
//! vtable VA0x623b20, 0x1b8 bytes (PS2's 0x200). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Offsets below
//! are byte offsets into the derived object; the base C_actor occupies 0x70.
//! Member names are descriptive PC projections; no PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x1b8-byte door object.
/// Pointer words overwritten by construction are not promoted into owners.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorSeed {
    pub bytes_00_1b7: [u8; 0x1b8],
}

/// Owned door bytes after construction. Words the constructor stores are
/// observable state, not interpreted native pointers: their only admitted
/// readers are the documented Init/LoadData offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DoorState {
    bytes: [u8; 0x1b8],
}

impl DoorState {
    pub(crate) fn allocation(seed: DoorSeed) -> Self {
        Self {
            bytes: seed.bytes_00_1b7,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_byte(&mut self, offset: usize, value: u8) {
        self.bytes[offset] = value;
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("door word"),
        )
    }

    /// Original constructor body in dependency order: base C_actor
    /// construction runs first (owned by the caller through construct_base),
    /// then the zeroed ranges, the stored timer words, the vtable-adjacent
    /// kind marker (on the base owner, see construct_door), the flag bytes
    /// and the float word. The three sound names are written to shared
    /// game-global buffers, retained on the world (see [`DoorSoundNames`]).
    pub(crate) fn complete_constructor(&mut self) {
        for offset in (0x78..0xc0).step_by(4) {
            self.put_word(offset, 0);
        }
        self.put_word(0x190, 0);
        for offset in (0x198..0x1b8).step_by(4) {
            self.put_word(offset, 0);
        }
        self.put_word(0x128, 0x3b83126f);
        self.put_word(0x12c, 0x3b83126f);
        self.put_byte(0x11d, 1);
        self.put_byte(0x11e, 0);
        self.put_word(0x120, 0x3fc90fdb);
        self.put_byte(0x125, 0);
        self.put_byte(0x124, 0);
        self.put_byte(0x11c, 0);
        self.put_byte(0x164, 0);
    }

    pub(crate) fn put_load_byte(&mut self, offset: usize, value: u8) {
        self.put_byte(offset, value);
    }

    pub(crate) fn put_load_word(&mut self, offset: usize, value: u32) {
        self.put_word(offset, value);
    }

    pub(crate) fn put_load_name(&mut self, offset: usize, name: &[u8]) {
        let slot = &mut self.bytes[offset..offset + 16];
        slot.fill(0);
        let length = name.len().min(16);
        slot[..length].copy_from_slice(&name[..length]);
    }

    /// Lock-flag byte at +0x11c selected by the version-7/8 LoadData path.
    pub fn lock_flag(&self) -> u8 {
        self.bytes[0x11c]
    }

    /// Open-sound name retained at +0x130 by every LoadData version.
    pub fn open_sound(&self) -> &[u8] {
        nul_terminated(&self.bytes[0x130..0x140])
    }

    /// Close-sound name retained at +0x140 by the LoadData tail.
    pub fn close_sound(&self) -> &[u8] {
        nul_terminated(&self.bytes[0x140..0x150])
    }

    /// Locked-sound name retained at +0x150 by the LoadData tail.
    pub fn locked_sound(&self) -> &[u8] {
        nul_terminated(&self.bytes[0x150..0x160])
    }

    /// Timer word at +0x128 shared by every LoadData version.
    pub fn timer_word(&self) -> u32 {
        self.word(0x128)
    }
}

fn nul_terminated(slot: &[u8]) -> &[u8] {
    let length = slot
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(slot.len());
    &slot[..length]
}

/// Shared default sound names written by every door construction into
/// game-global buffers. Later constructions overwrite earlier ones, matching
/// the native repeated strcpy into static storage; door effects (unimplemented)
/// read these back.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DoorSoundNames {
    pub open: [u8; 16],
    pub close: [u8; 16],
    pub locked: [u8; 16],
}

impl DoorSoundNames {
    pub(crate) fn construction_defaults() -> Self {
        let mut names = Self {
            open: [0; 16],
            close: [0; 16],
            locked: [0; 16],
        };
        names.open[..12].copy_from_slice(b"d_woodop.wav");
        names.close[..12].copy_from_slice(b"d_woodcl.wav");
        names.locked[..12].copy_from_slice(b"d_woodlo.wav");
        names
    }
}

/// Payload versions admitted by C_door::LoadData. Versions 5-7 populate
/// ascending field prefixes; version 8 populates the full flag/field set.
/// Any other version consumes only its own version word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoorLoadVersion {
    V5,
    V6,
    V7,
    V8,
    Unknown,
}

impl DoorLoadVersion {
    pub fn decode(word: u32) -> Self {
        match word {
            5 => Self::V5,
            6 => Self::V6,
            7 => Self::V7,
            8 => Self::V8,
            _ => Self::Unknown,
        }
    }
}

/// Original `E_state` door positions (mafia-re 113b5b6
/// `game/actors/C_door.h:13-19`). Discriminants preserve the native
/// declaration order; the script's DOOR_OPEN (see `C_door.h:11-12`) opens a
/// closed door and closes an open one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum DoorMotionState {
    Open = 0,
    Closed = 1,
    Opening = 2,
    Closing = 3,
}

/// Marker-only door control latch (GAME 0x439610 `SetState`, 0x43b810
/// `SetOpenAngle`, 0x43bcc0 `ChangeState`, 0x43add0 `EnableUsingObject`;
/// mafia-re 113b5b6 `game/actors/C_door.cpp:21-37,83-137`).
/// Stores the requested state enum, the 0..=1 open amount (`C_door.h:47`),
/// and the using-object flag. The `C_actor*` notify target, the `unknown1/2`
/// effect flags, and all frame articulation, sound, and sector effects run in
/// deferred frame/sound/sector owners; `ChangeState`'s base
/// `C_actor::ChangeState` call is likewise deferred.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoorControlState {
    state: DoorMotionState,
    open_angle: f32,
    using_enabled: bool,
}

impl DoorControlState {
    /// Host-side neutral default (closed, shut, usable). The native default is
    /// unrecovered: the constructor is only evidenced through the `DoorState`
    /// byte stores above.
    pub fn new() -> Self {
        Self {
            state: DoorMotionState::Closed,
            open_angle: 0.0,
            using_enabled: true,
        }
    }

    /// Original `SetState` store (GAME 0x439610; `C_door.cpp:21-25`). The
    /// actor notify target and both unknown effect flags are deferred with
    /// the frame/sound owners, so only the enum transition is retained.
    pub fn set_state(&mut self, state: DoorMotionState, _unknown1: bool, _unknown2: bool) {
        self.state = state;
    }

    /// Original `GetState` read (GAME 0x47b790; `C_door.h:68-71`).
    pub fn state(&self) -> DoorMotionState {
        self.state
    }

    /// Original `SetOpenAngle` store (GAME 0x43b810; `C_door.cpp:33-37`).
    /// Stores the value exactly as passed; the valid range is 0..=1 per
    /// `C_door.h:47`. Native saturation is unrecovered (stub), so no clamp
    /// is invented here. Frame articulation is deferred.
    pub fn set_open_angle(&mut self, open: f32) {
        self.open_angle = open;
    }

    /// Original open-amount read backing `SetOpenAngle`.
    pub fn open_angle(&self) -> f32 {
        self.open_angle
    }

    /// Original `ChangeState` toggle (GAME 0x43bcc0; `C_door.cpp:132-137`).
    /// Working toggle used by the script's DOOR_OPEN: open rests shut and
    /// shut rests open, with the in-between directions mirroring each other.
    /// Intermediate frame motion and the base `C_actor::ChangeState` call are
    /// deferred.
    pub fn change_state(&mut self) {
        self.state = match self.state {
            DoorMotionState::Open => DoorMotionState::Closed,
            DoorMotionState::Closed => DoorMotionState::Open,
            DoorMotionState::Opening => DoorMotionState::Closing,
            DoorMotionState::Closing => DoorMotionState::Opening,
        };
    }

    /// Original `EnableUsingObject` store (GAME 0x43add0; `C_door.cpp:27-31`).
    pub fn enable_using_object(&mut self, enable: bool) {
        self.using_enabled = enable;
    }

    /// Read back of the `EnableUsingObject` flag.
    pub fn using_enabled(&self) -> bool {
        self.using_enabled
    }
}

impl Default for DoorControlState {
    fn default() -> Self {
        Self::new()
    }
}

/// RVA 0x39390, VA 0x439390 (`C_door::~C_door`, mafia-re fe7e746
/// `game/actors/C_door.cpp:15-19`).
/// FUNCTION marker: the declared destructor body is empty apart from the
/// compiler-emitted vtable store handled by construction; no byte is read or
/// written, so the host drop path performs no additional work.
pub fn door_destructor_empty() {}

/// Working-name projection of `C_door::ResetUnknown110` (mafia-re fe7e746
/// `game/actors/C_door.h:49-55`: `unknown_114/118/110 = 0`, invoked by
/// `C_game::Init`). No standalone GAME entry exists for the three stores, so
/// no RVA is claimed here. The stores run in place on the derived bytes.
impl DoorState {
    pub fn reset_unknown110(&mut self) {
        self.put_word(0x110, 0);
        self.put_word(0x114, 0);
        self.put_word(0x118, 0);
    }

    /// Working-name read of the `locked` byte at +0x125 (mafia-re fe7e746
    /// `game/actors/C_door.h:57-65`: `DOOR_LOCK`'s flag). The script path
    /// writes the byte in place and no standalone FUNCTION copy exists, so no
    /// RVA is claimed here.
    pub fn is_locked(&self) -> bool {
        self.bytes[0x125] != 0
    }

    /// Working-name store of the `locked` byte at +0x125. Same ownership note
    /// as [`DoorState::is_locked`].
    pub fn set_locked(&mut self, locked: bool) {
        self.put_byte(0x125, u8::from(locked));
    }
}

/// RVA 0x3bab0, VA 0x43bab0 (`C_door::GetBoundPoints2D`, mafia-re fe7e746
/// `game/actors/C_door.cpp:125-131`).
/// STUB: explicit Unsupported. The native body logs and delegates to
/// `C_actor::GetBoundPoints2D`; the frame/sector owners that resolve the
/// hierarchy points do not exist here, so no points are produced.
pub fn door_bound_points_2d_unsupported() {}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> DoorState {
        let mut state = DoorState::allocation(DoorSeed {
            bytes_00_1b7: [0xaa; 0x1b8],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_exact_zero_ranges_words_and_flags() {
        let state = constructed();
        for offset in (0x78..0xc0).step_by(4) {
            assert_eq!(
                u32::from_le_bytes(state.bytes[offset..offset + 4].try_into().unwrap()),
                0,
                "{offset:#x}"
            );
        }
        assert_eq!(state.word(0x190), 0);
        for offset in (0x198..0x1b8).step_by(4) {
            assert_eq!(
                u32::from_le_bytes(state.bytes[offset..offset + 4].try_into().unwrap()),
                0,
                "{offset:#x}"
            );
        }
        assert_eq!(state.word(0x128), 0x3b83126f);
        assert_eq!(state.word(0x12c), 0x3b83126f);
        assert_eq!(state.bytes[0x11d], 1);
        assert_eq!(state.bytes[0x11e], 0);
        assert_eq!(state.word(0x120), 0x3fc90fdb);
        assert_eq!(state.bytes[0x125], 0);
        assert_eq!(state.bytes[0x124], 0);
        assert_eq!(state.bytes[0x11c], 0);
        assert_eq!(state.bytes[0x164], 0);
        // Untouched allocation survives outside the constructor writes.
        assert_eq!(state.bytes[0x00], 0xaa);
        assert_eq!(state.word(0x130), 0xaaaaaaaa);
    }

    #[test]
    fn construction_sound_defaults_match_original_strings() {
        let names = DoorSoundNames::construction_defaults();
        assert_eq!(&names.open[..12], b"d_woodop.wav");
        assert_eq!(&names.close[..12], b"d_woodcl.wav");
        assert_eq!(&names.locked[..12], b"d_woodlo.wav");
    }

    #[test]
    fn load_version_decodes_known_and_unknown_words() {
        assert_eq!(DoorLoadVersion::decode(5), DoorLoadVersion::V5);
        assert_eq!(DoorLoadVersion::decode(6), DoorLoadVersion::V6);
        assert_eq!(DoorLoadVersion::decode(7), DoorLoadVersion::V7);
        assert_eq!(DoorLoadVersion::decode(8), DoorLoadVersion::V8);
        assert_eq!(DoorLoadVersion::decode(0), DoorLoadVersion::Unknown);
        assert_eq!(DoorLoadVersion::decode(9), DoorLoadVersion::Unknown);
        assert_eq!(DoorLoadVersion::decode(u32::MAX), DoorLoadVersion::Unknown);
    }

    #[test]
    fn name_slots_terminate_and_truncate_like_native_buffers() {
        let mut state = constructed();
        state.put_load_name(0x130, b"d_woodop.wav");
        assert_eq!(state.open_sound(), b"d_woodop.wav");
        state.put_load_name(0x140, b"0123456789abcdefSURPLUS");
        assert_eq!(state.close_sound(), b"0123456789abcdef");
        // Untouched slots carry allocation bytes with no terminator, exactly
        // like the native buffers before any LoadData write.
        assert_eq!(state.locked_sound(), &[0xaa; 16]);
    }

    #[test]
    fn control_state_machine_stores_enum_and_angle() {
        // Hand-derived: SetState (GAME 0x439610) keeps the E_state enum,
        // SetOpenAngle (GAME 0x43b810) keeps the 0..=1 amount (C_door.h:47).
        let mut control = DoorControlState::new();
        assert_eq!(control.state(), DoorMotionState::Closed);
        assert_eq!(control.open_angle(), 0.0);
        assert!(control.using_enabled());
        control.set_state(DoorMotionState::Opening, false, false);
        assert_eq!(control.state(), DoorMotionState::Opening);
        control.set_open_angle(0.25);
        assert_eq!(control.open_angle(), 0.25);
        control.set_state(DoorMotionState::Open, true, false);
        assert_eq!(control.state(), DoorMotionState::Open);
    }

    #[test]
    fn control_change_state_toggles_round_trip() {
        // Hand-derived: ChangeState (GAME 0x43bcc0) backs the script DOOR_OPEN
        // (C_door.h:11-12), which opens a closed door and closes an open one.
        let mut control = DoorControlState::new();
        control.change_state();
        assert_eq!(control.state(), DoorMotionState::Open);
        control.change_state();
        assert_eq!(control.state(), DoorMotionState::Closed);
        control.set_state(DoorMotionState::Opening, false, false);
        control.change_state();
        assert_eq!(control.state(), DoorMotionState::Closing);
        control.change_state();
        assert_eq!(control.state(), DoorMotionState::Opening);
    }

    #[test]
    fn control_using_flag_stores_enable() {
        // Hand-derived: EnableUsingObject (GAME 0x43add0) latch round-trip.
        let mut control = DoorControlState::new();
        control.enable_using_object(false);
        assert!(!control.using_enabled());
        control.enable_using_object(true);
        assert!(control.using_enabled());
    }

    #[test]
    fn door_destructor_empty_rva_39390_is_noop() {
        // FUNCTION: GAME 0x439390 (VA 0x439390), mafia-re fe7e746
        // game/actors/C_door.cpp:15-19: empty destructor body, no stores.
        let before = constructed();
        door_destructor_empty();
        assert_eq!(constructed(), before);
    }

    #[test]
    fn reset_unknown110_zeroes_three_words() {
        // Working name: C_door::ResetUnknown110 (mafia-re fe7e746
        // game/actors/C_door.h:49-55) has no standalone GAME entry.
        let mut state = constructed();
        state.put_load_word(0x110, 0x11111111);
        state.put_load_word(0x114, 0x22222222);
        state.put_load_word(0x118, 0x33333333);
        state.reset_unknown110();
        assert_eq!(state.word(0x110), 0);
        assert_eq!(state.word(0x114), 0);
        assert_eq!(state.word(0x118), 0);
        // Neighbouring constructor words survive the reset.
        assert_eq!(state.word(0x128), 0x3b83126f);
    }

    #[test]
    fn locked_flag_round_trips() {
        // Working name: +0x125 locked byte (mafia-re fe7e746
        // game/actors/C_door.h:57-65), written in place by the script path.
        let mut state = constructed();
        assert!(!state.is_locked());
        state.set_locked(true);
        assert!(state.is_locked());
        assert_eq!(state.bytes[0x125], 1);
        state.set_locked(false);
        assert!(!state.is_locked());
    }

    #[test]
    fn door_bound_points_rva_3bab0_is_unsupported_stub() {
        // STUB: GAME 0x43bab0 (VA 0x43bab0), mafia-re fe7e746
        // game/actors/C_door.cpp:125-131: frame owner deferred.
        door_bound_points_2d_unsupported();
    }
}
