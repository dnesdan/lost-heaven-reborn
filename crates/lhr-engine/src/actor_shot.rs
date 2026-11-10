//! C_actor_shot derived storage for the original PC actor lifecycle (kind 33).
//!
//! Game.exe SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27;
//! constructor STUB VA0x566f40, Init STUB 0x566f90, Update STUB 0x566fd0,
//! GameInit STUB 0x567330, GameDone STUB 0x5673f0, vtable VA0x625458, 0x8c bytes
//! (PS2's 0xa0). Actual PS2 SLUS_206.71
//! SHA256 adf4944c5cdac23fcc4e9af9ef3ec1ba6234497e57cec2af2ff1a7a15c62a46f
//! corroborates the class name only, not the PC member layout. Reference sibling
//! mafia-re @ 113b5b6 game/actors/C_actor_shot.h:25 (vtable) + game/actors/
//! C_actor_shot.cpp:9-12 (constructor), :53-57 (Init), :60-64 (Update), :67-71
//! (GameInit); factory game/mission/Mission.cpp:211 (CreateActor GAME 0x53f7d0,
//! ACTOR_ACTOR_SHOT 0x21). Offsets below are byte offsets into the derived
//! object; the base C_actor occupies 0x70. Member names follow mafia-re; the
//! firing/shoot guesses are mafia-re's. No PS2 member layout is assumed.

/// Actual untouched allocation bytes of one 0x8c-byte shot object. The stub
/// constructor stores only the vtable and kind marker; every other word is
/// admitted untouched allocation, not interpreted native state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorShotSeed {
    pub bytes_00_8b: [u8; 0x8c],
}

/// Owned shot bytes after construction. Words the stub bodies store are
/// observable state, not interpreted native pointers: their only admitted
/// readers are the documented firing-state offsets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActorShotState {
    bytes: [u8; 0x8c],
}

impl ActorShotState {
    pub(crate) fn allocation(seed: ActorShotSeed) -> Self {
        Self {
            bytes: seed.bytes_00_8b,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    // Test-only storage helpers until evidenced writers land (native stub
    // Init/Update store no derived words).
    #[allow(dead_code)]
    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(
            self.bytes[offset..offset + 4]
                .try_into()
                .expect("shot word"),
        )
    }

    /// Original stub constructor body (mafia-re C_actor_shot.cpp:9-12): only
    /// the base construction plus the vtable store and kind marker, which live
    /// on the base owner (see construct_actor_shot). No derived word is stored;
    /// the -1 particle default has no evidenced store in the stub bodies and
    /// stays allocation bytes until its effect owner exists.
    pub(crate) fn complete_constructor(&mut self) {}

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_shot_time(&mut self, value: i32) {
        self.put_word(0x78, value as u32);
    }

    /// Shot start word at +0x78; -1 lets Update start a shot (mafia-re guess).
    pub fn shot_time(&self) -> i32 {
        self.word(0x78) as i32
    }

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_firing(&mut self, value: bool) {
        self.bytes[0x7c] = u8::from(value);
    }

    /// Firing flag at +0x7c; Update does nothing without it (mafia-re guess).
    pub fn firing(&self) -> bool {
        self.bytes[0x7c] != 0
    }

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_shoots(&mut self, value: bool) {
        self.bytes[0x84] = u8::from(value);
    }

    /// Shoot flag at +0x84; Update fires C_game::NewShoot with it (mafia-re guess).
    pub fn shoots(&self) -> bool {
        self.bytes[0x84] != 0
    }

    // Test-only until evidenced writers land (see put_word note).
    #[allow(dead_code)]
    pub(crate) fn set_particle(&mut self, value: i32) {
        self.put_word(0x88, value as u32);
    }

    /// Record particle word at +0x88; -1 means none (mafia-re name).
    pub fn particle(&self) -> i32 {
        self.word(0x88) as i32
    }
    /// GAME 0x404e20 (RVA 0x4e20) C_actor_shot::AddToCutscene (mafia-re
    /// 113b5b6 game/actors/C_actor.cpp:631-637): the shot actor files one
    /// record kind of `sizeof(S_actor_shot_record)` bytes (0x10 per
    /// game/actors/C_actor_shot.h:5-13) and no other kinds, then reports
    /// true. The recorder call stays with the cutscene owner (DEFERRED);
    /// this leaf stages the three size words plus the constant verdict, in
    /// (sizes, verdict) order, mirroring the base-actor leaf.
    pub fn add_to_cutscene(&self) -> ([u32; 3], bool) {
        ([0x10, 0, 0], true)
    }
}

/// GAME 0x567430 (RVA 0x167430) C_actor_shot::SetWeapon marker (mafia-re
/// 113b5b6 game/actors/C_actor_shot.cpp:87-91 and
/// game/actors/C_actor_shot.h:54-55): the reference body is an empty
/// STUB, so the port stores no word and changes no latch. The item and
/// weapon-effect owners stay DEFERRED.
pub fn set_weapon_marker() {}

/// GAME 0x404e40 (RVA 0x4e40) C_actor_shot::PlayRecord flag-write tail
/// (mafia-re 113b5b6 game/actors/C_actor.cpp:639-668 and
/// game/actors/C_actor_shot.h:15-21): shot_time goes to -1 and the
/// firing/shoots latches follow the staged record flags word
/// (SHOT_REC_FIRING 0x1000, SHOT_REC_SHOOTS 0x8000). The particle
/// start/stop branch (frame plus effects owners) and the SetWeapon handoff
/// stay DEFERRED; the caller writes the returned (shot_time, firing,
/// shoots) words.
pub fn apply_shot_record_flags(flags: u32) -> (i32, bool, bool) {
    const SHOT_REC_FIRING: u32 = 0x1000;
    const SHOT_REC_SHOOTS: u32 = 0x8000;
    (
        -1,
        flags & SHOT_REC_FIRING != 0,
        flags & SHOT_REC_SHOOTS != 0,
    )
}

/// C_ShotSkeleton working-name leaves (no new address credit). They live in
/// this shot module because `lib.rs` is frozen (no new modules) and the
/// human-owner files are off limits; they describe `C_ShotSkeleton`, not
/// `C_actor_shot`. Reference mafia-re `fe7e746`,
/// game/actors/C_ShotSkeleton.h + game/actors/C_ShotSkeleton.cpp. Both
/// native FUNCTION entries are frozen-ledger-occupied (see
/// docs/progress.json), and the joint/bone tables are data offsets rather
/// than code, so every item below is a working name. Frame lookup
/// (`FindChildFrame`) and the `TestCollision` line trace need the model
/// owner and stay caller duties.

/// Working name; number of shot joints (reference mafia-re `fe7e746`,
/// game/actors/C_ShotSkeleton.h joint table, `SHOT_JOINT_COUNT`).
pub const SHOT_JOINT_COUNT: usize = 16;

/// Working name; number of shot bones (reference mafia-re `fe7e746`,
/// game/actors/C_ShotSkeleton.h, `SHOT_BONE_COUNT`).
pub const SHOT_BONE_COUNT: usize = 10;

/// Working name; hit part taken by a bone (reference mafia-re `fe7e746`,
/// game/actors/C_human.h `HIT_PART_*` values and
/// game/actors/C_ShotSkeleton.cpp `vzoroveKosti`). Zero means no hit, as in
/// the native `TestCollision` initializer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShotHitPart {
    None = 0,
    HandR = 1,
    HandL = 2,
    LegR = 3,
    LegL = 4,
    Body = 5,
    Head = 6,
}

/// Working name; one shot-test bone row (reference mafia-re `fe7e746`,
/// game/actors/C_ShotSkeleton.h `S_shot_bone`: joint pair, start/end along
/// the joint axis, radius on either side, hit part).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShotBone {
    pub joint0: usize,
    pub joint1: usize,
    pub start: f32,
    pub end: f32,
    pub radius: f32,
    pub part: ShotHitPart,
}

/// Working name; joint-name lookup leaf (reference mafia-re `fe7e746`,
/// game/actors/C_ShotSkeleton.cpp `jointName` order consumed by
/// `C_ShotSkeleton::Init`). Exact byte match; `None` is unknown. The
/// `FindChildFrame` call for a hit index needs the model owner.
pub fn shot_joint_index(name: &[u8]) -> Option<usize> {
    const JOINTS: [&[u8]; SHOT_JOINT_COUNT] = [
        b"r_hand",
        b"r_elbow",
        b"r_arm",
        b"r_foot",
        b"r_shin",
        b"r_thigh",
        b"l_hand",
        b"l_elbow",
        b"l_arm",
        b"l_foot",
        b"l_shin",
        b"l_thigh",
        b"base",
        b"back1",
        b"back3",
        b"neck",
    ];
    JOINTS.iter().position(|joint| *joint == name)
}

/// Working name; default bone-row getter leaf (reference mafia-re
/// `fe7e746`, game/actors/C_ShotSkeleton.cpp `vzoroveKosti` consumed by
/// `C_ShotSkeleton::Init`). Out-of-range indices yield `None` as a host
/// guard.
pub fn shot_bone_at(index: usize) -> Option<ShotBone> {
    const BONES: [(usize, usize, f32, f32, f32, ShotHitPart); SHOT_BONE_COUNT] = [
        (0, 1, -0.5, 1.2, 0.06, ShotHitPart::HandR),
        (6, 7, -0.5, 1.2, 0.06, ShotHitPart::HandL),
        (1, 2, 0.0, 1.3, 0.08, ShotHitPart::HandR),
        (7, 8, 0.0, 1.3, 0.08, ShotHitPart::HandL),
        (3, 4, -0.3, 1.1, 0.1, ShotHitPart::LegR),
        (9, 10, -0.3, 1.1, 0.1, ShotHitPart::LegL),
        (4, 5, 0.1, 1.2, 0.12, ShotHitPart::LegR),
        (10, 11, 0.1, 1.2, 0.12, ShotHitPart::LegL),
        (13, 14, -0.5, 1.2, 0.17, ShotHitPart::Body),
        (14, 15, 0.5, 3.0, 0.12, ShotHitPart::Head),
    ];
    let (joint0, joint1, start, end, radius, part) = BONES.get(index)?;
    Some(ShotBone {
        joint0: *joint0,
        joint1: *joint1,
        start: *start,
        end: *end,
        radius: *radius,
        part: *part,
    })
}

/// Working name; hit-part projection over the default bone table
/// (reference mafia-re `fe7e746`, game/actors/C_ShotSkeleton.cpp
/// `vzoroveKosti` part column). Out-of-range indices yield `None`.
pub fn shot_bone_part(index: usize) -> Option<ShotHitPart> {
    shot_bone_at(index).map(|bone| bone.part)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constructed() -> ActorShotState {
        let mut state = ActorShotState::allocation(ActorShotSeed {
            bytes_00_8b: [0xaa; 0x8c],
        });
        state.complete_constructor();
        state
    }

    #[test]
    fn constructor_stores_no_derived_words() {
        let state = constructed();
        assert_eq!(state.bytes, [0xaa; 0x8c]);
    }

    #[test]
    fn firing_state_round_trips_stored_words() {
        let mut state = constructed();
        assert_eq!(state.shot_time(), 0xaaaaaaaau32 as i32);
        assert!(state.firing());
        assert!(state.shoots());
        assert_eq!(state.particle(), 0xaaaaaaaau32 as i32);
        state.set_shot_time(-1);
        state.set_firing(false);
        state.set_shoots(true);
        state.set_particle(-1);
        assert_eq!(state.shot_time(), -1);
        assert!(!state.firing());
        assert!(state.shoots());
        assert_eq!(state.particle(), -1);
        state.set_shot_time(120);
        state.set_firing(true);
        state.set_shoots(false);
        state.set_particle(7);
        assert_eq!(state.shot_time(), 120);
        assert!(state.firing());
        assert!(!state.shoots());
        assert_eq!(state.particle(), 7);
    }

    #[test]
    // GAME 0x404e20 (RVA 0x4e20) C_actor_shot::AddToCutscene (mafia-re 113b5b6
    // game/actors/C_actor.cpp:631-637): one 0x10-byte shot-record kind, no
    // other kinds, constant true; the recorder call stays DEFERRED.
    fn add_to_cutscene_rva_4e20_files_shot_record_size() {
        let state = constructed();
        assert_eq!(state.add_to_cutscene(), ([0x10, 0, 0], true));
    }

    #[test]
    // GAME 0x567430 (RVA 0x167430) C_actor_shot::SetWeapon (mafia-re 113b5b6
    // game/actors/C_actor_shot.cpp:87-91): the reference body is empty, so
    // the marker stores nothing; latched words survive the call.
    fn set_weapon_rva_167430_marker_stores_nothing() {
        let mut state = constructed();
        state.set_shot_time(120);
        state.set_firing(true);
        state.set_shoots(true);
        state.set_particle(7);
        set_weapon_marker();
        assert_eq!(state.shot_time(), 120);
        assert!(state.firing());
        assert!(state.shoots());
        assert_eq!(state.particle(), 7);
    }

    #[test]
    // GAME 0x404e40 (RVA 0x4e40) C_actor_shot::PlayRecord flag tail
    // (mafia-re 113b5b6 game/actors/C_actor.cpp:639-668): shot_time is -1
    // while firing/shoots follow SHOT_REC_FIRING (0x1000) and
    // SHOT_REC_SHOOTS (0x8000); other flag bits are ignored.
    fn shot_record_flags_rva_4e40_tail_follows_firing_and_shoots_bits() {
        assert_eq!(apply_shot_record_flags(0), (-1, false, false));
        assert_eq!(apply_shot_record_flags(0x1000), (-1, true, false));
        assert_eq!(apply_shot_record_flags(0x8000), (-1, false, true));
        assert_eq!(apply_shot_record_flags(0x9000), (-1, true, true));
        assert_eq!(apply_shot_record_flags(0x6fff), (-1, false, false));
        assert_eq!(apply_shot_record_flags(0xffff), (-1, true, true));
    }

    #[test]
    fn skeleton_joint_index_covers_reference_table() {
        assert_eq!(SHOT_JOINT_COUNT, 16);
        let names: [&[u8]; 16] = [
            b"r_hand",
            b"r_elbow",
            b"r_arm",
            b"r_foot",
            b"r_shin",
            b"r_thigh",
            b"l_hand",
            b"l_elbow",
            b"l_arm",
            b"l_foot",
            b"l_shin",
            b"l_thigh",
            b"base",
            b"back1",
            b"back3",
            b"neck",
        ];
        for (index, name) in names.iter().enumerate() {
            assert_eq!(shot_joint_index(name), Some(index));
        }
        assert_eq!(shot_joint_index(b"unknown"), None);
        assert_eq!(shot_joint_index(b"R_HAND"), None);
        assert_eq!(shot_joint_index(b""), None);
    }

    #[test]
    fn skeleton_bone_rows_match_reference_table() {
        assert_eq!(SHOT_BONE_COUNT, 10);
        let expected = [
            (0, 1, -0.5f32, 1.2f32, 0.06f32, ShotHitPart::HandR),
            (6, 7, -0.5, 1.2, 0.06, ShotHitPart::HandL),
            (1, 2, 0.0, 1.3, 0.08, ShotHitPart::HandR),
            (7, 8, 0.0, 1.3, 0.08, ShotHitPart::HandL),
            (3, 4, -0.3, 1.1, 0.1, ShotHitPart::LegR),
            (9, 10, -0.3, 1.1, 0.1, ShotHitPart::LegL),
            (4, 5, 0.1, 1.2, 0.12, ShotHitPart::LegR),
            (10, 11, 0.1, 1.2, 0.12, ShotHitPart::LegL),
            (13, 14, -0.5, 1.2, 0.17, ShotHitPart::Body),
            (14, 15, 0.5, 3.0, 0.12, ShotHitPart::Head),
        ];
        for (index, row) in expected.iter().enumerate() {
            let bone = shot_bone_at(index).expect("bone row");
            assert_eq!(bone.joint0, row.0);
            assert_eq!(bone.joint1, row.1);
            assert_eq!(bone.start, row.2);
            assert_eq!(bone.end, row.3);
            assert_eq!(bone.radius, row.4);
            assert_eq!(bone.part, row.5);
        }
        assert_eq!(shot_bone_at(10), None);
        assert_eq!(shot_bone_at(usize::MAX), None);
    }

    #[test]
    fn skeleton_bone_part_projects_hit_part() {
        assert_eq!(shot_bone_part(0), Some(ShotHitPart::HandR));
        assert_eq!(shot_bone_part(8), Some(ShotHitPart::Body));
        assert_eq!(shot_bone_part(9), Some(ShotHitPart::Head));
        assert_eq!(shot_bone_part(10), None);
        assert_eq!(ShotHitPart::None as u32, 0);
        assert_eq!(ShotHitPart::Head as u32, 6);
    }
}
