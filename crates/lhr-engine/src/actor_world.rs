//! Owned base actors and their distinct Game source collections.
//!
//! Base C_actor, C_detector, C_railway and constructor/source-admitted
//! Human/Player/Entity own their real derived and embedded program lifetimes.
//! This is not derived GameInit,
//! compiled instruction execution, a mission loader or an LS3DF scene constructor.
//! Names C_actor, AddActorToGlobal and C_mission::UnloadActor are corroborated
//! by original PS2 symbols; field names below remain descriptive PC projections.
//! Native allocation is an explicit dependency, separate from Rust storage.
//! Host work/storage bounds allow 65,536 total constructed incarnations (dead
//! tombstones remain inspectable) and 65,536 entries per source. These are host
//! admission limits, not native actor limits. Unsupported native allocator
//! exceptions, allocator callback reentry into collection mutation, other derived virtuals and
//! nonzero manager cleanup flags have no implicit successful implementation.
use crate::actor_shot::{ActorShotSeed, ActorShotState};
use crate::box_new_actor::{BoxLoadFlag, BoxNewSeed, BoxNewState};
use crate::bridge_actor::{BridgeSectors, BridgeSeed, BridgeState, BridgeTable};
use crate::car_actor::{CarActorServices, CarActorState, CarSeed};
use crate::car_ex_actor::{CarExSeed, CarExState};
use crate::clock_actor::{ClockSeed, ClockState};
use crate::cutscene_records::{
    ActorUserData, CutsceneRecordArena, RecordedActorAssociation, RecordedActorId,
};
use crate::door_actor::{DoorLoadVersion, DoorSeed, DoorSoundNames, DoorState};
use crate::entity_actor::{EntityProcessState, EntityResourceSlot, EntitySeed, EntityState};
use crate::frame_lookup::{FrameLookupBackend, SceneRoots};
use crate::human_actor::{
    ActorDataReader, HumanDataError, HumanInventoryItem, HumanResourceSlot, HumanSeed, HumanState,
};
use crate::original_program::{
    CProgram, OwnerFrameName, ProgramBackend, ProgramContext, ProgramError, ProgramReader,
    ProgramReplacement, ProgramSeed,
};
use crate::player_actor::{PlayerSeed, PlayerState};
use crate::pumper_actor::{PumperSeed, PumperState};
use crate::racecam::{RacecamSeed, RacecamState};
use crate::rail_generator::{RailGeneratorSeed, RailGeneratorState};
use crate::railway_actor::{RailwayAllocation, RailwaySeed, RailwayState, RailwayVectorSlot};
use crate::script_lookup::{ActorLifetime, LookupError, LookupWorldBackend, MAX_ACTOR_SOURCES};
use crate::script_scheduler::{ActorReferenceId, FrameReferenceId};
use crate::traffic_car_actor::{
    TrafficCarSeed, TrafficCarState, TrafficLoadVersion, TrafficTuning,
};
use crate::traffic_generator::{
    TrafficAllocationId, TrafficBackingKind, TrafficGeneratorSeed, TrafficGeneratorState,
};

#[path = "actor_world_traffic.rs"]
mod traffic;

/// Bytes/fields left untouched by the base constructor. There is deliberately
/// no default: the allocator/caller must supply the actual storage seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseActorSeed {
    pub word_0e: u16,
    pub actor_id: u32,
    pub state_1c: u32,
    pub bytes_21_5f: [u8; 63],
    pub bytes_64_65: [u8; 2],
    pub byte_67: u8,
    pub release_frame: u8,
    pub bytes_6d_6f: [u8; 3],
}

/// Typed projection of all 0x70 bytes of native base storage. Frame identities
/// and the base vtable tag replace native pointers; no host pointer is exposed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BaseActorState {
    pub vtable: ActorVtable,
    pub flags: u32,
    pub user_data: ActorUserData,
    pub references: u16,
    pub word_0e: u16,
    pub word_10: u32,
    pub word_14: u32,
    pub actor_id: u32,
    pub state_1c: u32,
    pub byte_20: u8,
    pub bytes_21_5f: [u8; 63],
    pub word_60: u32,
    pub bytes_64_65: [u8; 2],
    pub byte_66: u8,
    pub byte_67: u8,
    pub frame: Option<FrameReferenceId>,
    pub release_frame: u8,
    pub bytes_6d_6f: [u8; 3],
    /// Script-visible dead latch for GETACTORSDIST/actor reads (mafia-re
    /// game/actors/C_actor.h:359 dead at 0x5e, GetDead GAME 0x47afc0).
    /// Starts clear; GameInit/Tick producers stay open.
    /// No overlay entry; rides Process 0x6d3b0.
    pub dead: bool,
    /// Script-visible alive latch (mafia-re game/actors/C_actor.h:355-357:
    /// 0x5d bool alive, true from GameInit till the end; GetAlive GAME
    /// 0x47afd0 reads it). Starts at not alive (working deterministic
    /// default for the pre-GameInit seed; the native C_actor ctor leaves it
    /// true, the GameInit writer setting it stays DEFERRED).
    /// No overlay entry of its own; rides the 0x17d2e0 stop entry.
    pub alive: bool,
    /// Script-visible rendered latch (mafia-re game/actors/C_actor.h:359:
    /// 0x5f bool rendered, false from GameInit; GetRendered GAME 0x4a9990
    /// reads it; a copy of the body lives in g_sledovacka.cpp). Starts
    /// clear; the GameInit writer and the scene-last-frame producer stay
    /// DEFERRED.
    /// No overlay entry of its own; rides the 0x17d2e0 stop entry.
    pub rendered: bool,
}
/// Original active actor-state value (mafia-re 113b5b6
/// game/actors/C_actor.h:61 ACTSTATE_ACTIVE 0): the GameInit steady state,
/// also written by Rebirth.
pub const ACTSTATE_ACTIVE: u32 = 0;
/// Original inert actor-state value (mafia-re 113b5b6 game/actors/C_actor.h:63
/// ACTSTATE_NONE 2): SetActState never enters it and Rebirth is the only
/// writer that leaves it.
pub const ACTSTATE_NONE: u32 = 2;
impl BaseActorState {
    fn construct(seed: BaseActorSeed) -> Self {
        Self {
            vtable: ActorVtable::Base,
            flags: 0,
            user_data: ActorUserData::Null,
            references: 1,
            word_0e: seed.word_0e,
            word_10: 1,
            word_14: 1,
            actor_id: seed.actor_id,
            state_1c: seed.state_1c,
            byte_20: 0,
            bytes_21_5f: seed.bytes_21_5f,
            word_60: 0,
            bytes_64_65: seed.bytes_64_65,
            byte_66: 0,
            byte_67: seed.byte_67,
            frame: None,
            release_frame: seed.release_frame,
            bytes_6d_6f: seed.bytes_6d_6f,
            dead: false,
            alive: false,
            rendered: false,
        }
    }
    /// Original GetDead read.
    pub fn get_dead(&self) -> bool {
        self.dead
    }
    /// Test/host setter for the dead latch (Tick producer deferred).
    pub fn set_dead(&mut self, dead: bool) {
        self.dead = dead;
    }
    /// GAME 0x47afd0 (RVA 0x7afd0) C_actor::GetAlive (mafia-re 113b5b6
    /// game/actors/C_actor.h:427-430): read the 0x5d alive latch.
    pub fn alive(&self) -> bool {
        self.alive
    }
    /// Test/host setter for the alive latch (GameInit/death writers deferred).
    pub fn set_alive(&mut self, alive: bool) {
        self.alive = alive;
    }
    /// GAME 0x4a9990 (RVA 0x4a9990) C_actor::GetRendered (mafia-re 113b5b6
    /// game/actors/C_actor.h:421-425; a copy of the body lives in
    /// g_sledovacka.cpp): read the 0x5f rendered latch.
    pub fn rendered(&self) -> bool {
        self.rendered
    }
    /// Test/host setter for the rendered latch (GameInit/scene writers deferred).
    pub fn set_rendered(&mut self, rendered: bool) {
        self.rendered = rendered;
    }
    /// GAME 0x47af20 (RVA 0x7af20) C_actor::GetSubtype (mafia-re 113b5b6
    /// game/actors/C_actor.h:394-397): read the 0x14 subtype word
    /// (ACTORSUB_*, 1 at first). Backed by the typed word_14 seed word.
    pub fn subtype(&self) -> u32 {
        self.word_14
    }
    /// GAME 0x47afb0 (RVA 0x7afb0) C_actor::GetFrame (mafia-re 113b5b6
    /// game/actors/C_actor.h:410-414; FOLDED with the C_mission
    /// GetMissionName copy, so this address names the shared body): read
    /// the 0x68 frame link, None without a frame.
    pub fn frame_id(&self) -> Option<FrameReferenceId> {
        self.frame
    }
    /// GAME 0x47af30 (RVA 0x7af30) C_actor::GetPos (mafia-re 113b5b6
    /// game/actors/C_actor.h:323-327): return the 0x24 position triplet by
    /// value. The words stay inside the staged 0x21 window; the read is a
    /// plain little-endian f32 triplet with no rounding.
    pub fn pos(&self) -> [f32; 3] {
        Self::window_triplet(&self.bytes_21_5f, POS_WINDOW_OFFSET)
    }
    /// GAME 0x47af50 (RVA 0x7af50) C_actor::SetPos (mafia-re 113b5b6
    /// game/actors/C_actor.h:399-403): store the 0x24 position triplet by
    /// value.
    pub fn set_pos(&mut self, pos: [f32; 3]) {
        Self::set_window_triplet(&mut self.bytes_21_5f, POS_WINDOW_OFFSET, pos);
    }
    /// GAME 0x47af70 (RVA 0x7af70) C_actor::GetDir (mafia-re 113b5b6
    /// game/actors/C_actor.h:329-332): return the 0x30 direction triplet by
    /// value, same windowed read as pos.
    pub fn dir(&self) -> [f32; 3] {
        Self::window_triplet(&self.bytes_21_5f, DIR_WINDOW_OFFSET)
    }
    /// GAME 0x47af90 (RVA 0x7af90) C_actor::SetDir (mafia-re 113b5b6
    /// game/actors/C_actor.h:405-408): store the 0x30 direction triplet by
    /// value.
    pub fn set_dir(&mut self, dir: [f32; 3]) {
        Self::set_window_triplet(&mut self.bytes_21_5f, DIR_WINDOW_OFFSET, dir);
    }
    /// GAME 0x47afe0 (RVA 0x7afe0) C_actor::SetOwnFrame (mafia-re 113b5b6
    /// game/actors/C_actor.h:431-435): store the 0x6c own-frame latch. The
    /// destructor/GameDone release reads it back (see destroy_base_inner);
    /// the original exposes no getter, see own_frame.
    pub fn set_own_frame(&mut self, own: bool) {
        self.release_frame = u8::from(own);
    }
    /// Working-name reader for the 0x6c own-frame latch written by
    /// set_own_frame (GAME 0x47afe0, RVA 0x7afe0): the original exposes no
    /// getter, the destructor reads the byte in place instead.
    pub fn own_frame(&self) -> bool {
        self.release_frame != 0
    }
    /// GAME 0x408040 (RVA 0x8040) C_actor::GetWorldDir (mafia-re 113b5b6
    /// game/actors/C_actor.cpp:1691-1697): the frame's world direction where
    /// one is staged, else the staged 0x30 direction. The frame/matrix owner
    /// stays DEFERRED, so the caller stages the frame direction.
    pub fn world_dir(&self, frame_dir: Option<[f32; 3]>) -> [f32; 3] {
        frame_dir.unwrap_or_else(|| self.dir())
    }
    fn window_triplet(window: &[u8; 63], offset: usize) -> [f32; 3] {
        std::array::from_fn(|lane| {
            f32::from_le_bytes(
                window[offset + lane * 4..offset + lane * 4 + 4]
                    .try_into()
                    .expect("window triplet"),
            )
        })
    }
    fn set_window_triplet(window: &mut [u8; 63], offset: usize, value: [f32; 3]) {
        for (lane, word) in value.iter().enumerate() {
            window[offset + lane * 4..offset + lane * 4 + 4]
                .copy_from_slice(&word.to_le_bytes());
        }
    }
    /// GAME 0x47aff0 (RVA 0x7aff0) C_actor::SetUnknown65 (mafia-re 113b5b6
    /// game/actors/C_actor.h:437-440): store the 0x65 byte. Sibling writer
    /// C_detector::SetSignal clears the same byte first; see
    /// ActorWorld::set_detector_signal (GAME 0x47b760).
    pub fn set_unknown65(&mut self, on: bool) {
        self.bytes_64_65[1] = u8::from(on);
    }
    /// GAME 0x4091d0 (RVA 0x91d0) C_actor::Release (mafia-re 113b5b6
    /// game/actors/C_actor.h:196-203): decrement the 0x0c reference count
    /// with 16-bit wrap and return the post-decrement count. The
    /// delete-this half (native `delete this` at zero) stays with the
    /// world owner; see ActorWorld::release.
    pub fn release_one(&mut self) -> u16 {
        self.references = self.references.wrapping_sub(1);
        self.references
    }
    /// GAME 0x547220 (RVA 0x47220) C_actor::SetFromMission (mafia-re
    /// 113b5b6 game/actors/C_actor.h:272-276; field game/actors/C_actor.h:
    /// 369-371: 0x66 from_mission, set for the mission file's actors):
    /// mark the actor as a mission-file actor. No phase gate or GameInit
    /// here; the phase-gated scene-stream marker is the sibling
    /// ActorWorld::mark_scene_loaded, which takes no credit for this RVA.
    pub fn set_from_mission(&mut self) {
        self.byte_66 = 1;
    }
    /// Original C_actor::RecordKey (GAME 0x404f30, mafia-re 113b5b6
    /// game/actors/C_actor.cpp:670-674; contract game/actors/C_actor.h:166):
    /// the record made for the time has no type here, always -1.
    pub fn record_key(&self, _time: i32) -> i32 {
        -1
    }
    /// GAME 0x4091f0 (RVA 0x91f0) C_actor::GetActorSpeed (mafia-re 113b5b6
    /// game/actors/C_actor.h:230-233): the base actor has no speed, so it
    /// zeroes the caller's speed word (`speed.Zero()` inline in the header;
    /// the out-of-line copy the linker kept is the folded SYNTHETIC at this
    /// address). The walking human override publishes its velocity instead;
    /// see HumanState::actor_speed_into (GAME 0x51af20).
    pub fn base_actor_speed_into(&self, speed: &mut [f32; 3]) {
        *speed = [0.0, 0.0, 0.0];
    }
    /// GAME 0x409210 (RVA 0x9210) C_actor::GetMaterialID (mafia-re 113b5b6
    /// game/actors/C_actor.h:250-253): the base reports material 0. The
    /// frame argument the native takes is unused by the base body and is
    /// not staged here; material owners stay DEFERRED.
    pub fn material_id(&self) -> i32 {
        0
    }
    /// GAME 0x407590 (RVA 0x7590) C_actor::IsPossibleColl (mafia-re 113b5b6
    /// game/actors/C_actor.h:254-258): the base never collides, so it
    /// returns false without reading any input. The native probe arguments
    /// (the other actor, both travel vectors, the radius, the distance out
    /// word and the hit-slot out word) are all unused by the base body and
    /// are not staged here; the pedestrian override with real probe math
    /// lives with the collision owner (DEFERRED).
    pub fn is_possible_coll(&self) -> bool {
        false
    }
    /// GAME 0x4aa450 (RVA 0x4a450) C_actor::Collision (mafia-re 113b5b6
    /// game/actors/C_actor.h:238-242): the base never collides, so it
    /// returns false without reading any input. The native arguments (the
    /// other actor, three vectors, the radius and the frame) are all unused
    /// by the base body and are not staged here.
    pub fn collision(&self) -> bool {
        false
    }
    /// GAME 0x404e00 (RVA 0x4e00) C_actor::AddToCutscene (mafia-re 113b5b6
    /// game/actors/C_actor.cpp:623-629): the base files one record kind of
    /// `sizeof(S_actor_record)` bytes (0x5c per game/actors/C_actor.h:102)
    /// and no other kinds, then reports true. The recorder call
    /// (`cutscene.AddActor`) stays with the cutscene owner (DEFERRED); this
    /// leaf only stages the three size words plus the constant verdict, in
    /// (sizes, verdict) order.
    pub fn add_to_cutscene(&self) -> ([u32; 3], bool) {
        ([0x5c, 0, 0], true)
    }
    /// GAME 0x4075a0 (RVA 0x75a0) C_actor::GetBoundPoints2D EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_actor.cpp:1534-1611): the body projects the frame's
    /// high-detail bounding volume through its matrix (four padded ground
    /// corners past a 0.7071 up-facing gate, else eight padded corners with
    /// the furthest +x/-x/+z/-z selection) into the caller's 2D point list.
    /// The frame, matrix and bound-volume owners do not exist here, so no
    /// point is produced and no owner is filed; the stub exists only to pin
    /// the address while those owners stay DEFERRED.
    pub fn bound_points_2d_unsupported() {}
    /// Inline C_actor::GetActState read (mafia-re 113b5b6
    /// game/actors/C_actor.h:281-286; working name, no standalone FUNCTION
    /// and no RVA claimed: C_game::ChangeActState reads the state through
    /// it at the call sites). Reads the 0x1c act-state word (ACTSTATE_*),
    /// backed by the staged state_1c word.
    pub fn act_state(&self) -> u32 {
        self.state_1c
    }
    /// GAME 0x406dd0 (RVA 0x6dd0) C_actor::Rebirth (mafia-re 113b5b6
    /// game/actors/C_actor.cpp:1366-1375): an actor in ACTSTATE_NONE returns
    /// to ACTSTATE_ACTIVE. Returns whether the game-side change flag would
    /// be raised (act_mission->game->act_state_changed = true); the game
    /// owner stays DEFERRED, so the edge is reported, not delivered.
    pub fn rebirth(&mut self) -> bool {
        if self.state_1c == ACTSTATE_NONE {
            self.state_1c = ACTSTATE_ACTIVE;
            return true;
        }
        false
    }
    /// Inline C_actor::GetType read (mafia-re 113b5b6
    /// game/actors/C_actor.h:294-299; working name, no standalone FUNCTION
    /// and no RVA claimed: VC6 left the callers out of line and the copies
    /// folded with C_mission::GetScene and others). Reads the 0x10 type
    /// word (ACTOR_*, ACTOR_ACTOR 1 from the constructor), backed by the
    /// staged word_10.
    pub fn actor_type(&self) -> u32 {
        self.word_10
    }
    /// Inline C_actor::GetActFlags read (mafia-re 113b5b6
    /// game/actors/C_actor.h:446-450; working name, no new address claimed:
    /// the only folded copy (GAME 0x604d80) is retained in the ledger).
    /// Reads the 0x04 act-flags word (ACTF_*), backed by the staged flags
    /// word.
    pub fn act_flags(&self) -> u32 {
        self.flags
    }
    /// Read of the C_actor::SetFromMission word (mafia-re 113b5b6
    /// game/actors/C_actor.h:271-276 and :369-371: 0x66 from_mission; the
    /// setter's address GAME 0x547220 is retained in the ledger and no
    /// reference getter exists, so this is a working name with no address
    /// claimed). Reports whether the actor was marked as a mission-file
    /// actor; the writer is BaseActorState::set_from_mission.
    pub fn from_mission(&self) -> bool {
        self.byte_66 != 0
    }
    /// Base C_actor::GetProgram leaf (mafia-re 113b5b6
    /// game/actors/C_actor.h:260-265; working name, no address claimed: the
    /// NULL-returning bodies were folded by the linker into the group kept
    /// under C_seagull::SaveGameGetSize at GAME 0x409220). The base actor
    /// carries no script, so the leaf reports no program; the detector and
    /// entity overrides live with their program owners.
    pub fn program(&self) -> Option<&CProgram> {
        None
    }
    /// Working-name read of the C_actor 0x20 net-direct latch (mafia-re
    /// 113b5b6 game/actors/C_actor.h:344-346: bool net_direct, false from
    /// the constructor; Tick runs NetDirect instead of AI while set; no
    /// standalone FUNCTION, the byte is read in place, so no RVA claimed).
    /// Reports whether the actor takes the network-direct tick path.
    pub fn is_net_direct(&self) -> bool {
        self.byte_20 != 0
    }
    /// Working-name write of the C_actor 0x20 net-direct latch (mafia-re
    /// 113b5b6 game/actors/C_actor.h:344-346; no standalone FUNCTION, the
    /// byte is written in place by construction/GameInit paths, so no RVA
    /// claimed). Stages the network-direct tick selector for tests; the
    /// mission tick owner stays DEFERRED.
    pub fn set_net_direct(&mut self, direct: bool) {
        self.byte_20 = u8::from(direct);
    }
    /// Working-name read of the C_actor 0x64 frame-actor-set latch
    /// (mafia-re 113b5b6 game/actors/C_actor.h:360-364: bool
    /// frame_actor_set, GameInit gives the frame its actor pointer while
    /// set; no standalone FUNCTION, the byte is read in place by GameDone,
    /// so no RVA claimed). Reports whether the frame currently carries the
    /// actor pointer.
    pub fn frame_actor_set(&self) -> bool {
        self.bytes_64_65[0] != 0
    }
    /// Working-name write of the C_actor 0x64 frame-actor-set latch
    /// (mafia-re 113b5b6 game/actors/C_actor.h:360-364; no standalone
    /// FUNCTION, the byte is written in place, so no RVA claimed). Stages
    /// the frame-pointer latch for tests; the frame owner stays DEFERRED.
    pub fn set_frame_actor_set(&mut self, set: bool) {
        self.bytes_64_65[0] = u8::from(set);
    }
    /// Working-name read of the C_actor 0x65 unknown latch (mafia-re 113b5b6
    /// game/actors/C_actor.h:365: bool unknown_65, set for plain actors by
    /// GameInit; no standalone getter, the byte is read in place by
    /// C_detector::SetSignal and GameDone paths, so no RVA claimed).
    /// Reports the staged byte written by [`Self::set_unknown65`].
    pub fn is_unknown65(&self) -> bool {
        self.bytes_64_65[1] != 0
    }
    /// Working-name read of the C_actor 0x0c reference count (mafia-re
    /// 113b5b6 game/actors/C_actor.h:196-203: ushort ref, 1 at first;
    /// Release decrements with 16-bit wrap; no standalone getter, the word
    /// is read in place, so no RVA claimed). Reports the staged count that
    /// [`Self::release_one`] decrements.
    pub fn reference_count(&self) -> u16 {
        self.references
    }
    /// Working-name read of the C_actor 0x18 actor number (mafia-re 113b5b6
    /// game/actors/C_actor.h:342: int number, the mission count when Init
    /// took the frame; SaveGameLoadGuid reads it back; no standalone
    /// getter, the word is read in place, so no RVA claimed). Reports the
    /// staged mission number.
    pub fn actor_number(&self) -> u32 {
        self.actor_id
    }
}
/// GAME 0x406590 (RVA 0x6590) C_actor::Actor_SaveGameGetSize (mafia-re
/// 113b5b6 game/actors/C_actor.cpp:1146-1155): the saved-actor reference
/// size for a staged frame name: two words without an actor, a nameless
/// frame or an empty name, else two words plus the NUL-terminated name.
/// The frame/car owners stay DEFERRED; the caller stages the frame name.
pub fn actor_savegame_size(frame_name: Option<&str>) -> u32 {
    match frame_name {
        Some(name) if !name.is_empty() => 8 + name.len() as u32 + 1,
        _ => 8,
    }
}
/// GAME 0x547130 (RVA 0x147130) C_actor::SetFrameActor EXPLICIT
/// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_actor.h:507-519): give
/// the frame the actor-pointer slot, freeing the previous user data (and a
/// null-pointed allocation). The frame user-data owner does not exist here,
/// so no pointer is stored and no allocation is freed.
pub fn set_frame_actor_unsupported() {}
/// GAME 0x604d90 (RVA 0x204d90) C_actor::GetFrameActor EXPLICIT
/// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_actor.h:492-505): read
/// the frame user data as the actor-pointer slot. The frame owner does not
/// exist here, so no pointer is returned.
pub fn get_frame_actor_unsupported() {}
/// Staged live-frame placement words read by C_actor::InternalPlacementReset
/// (mafia-re 113b5b6 game/actors/C_actor.h:350-354: 0x3c saved_pos,
/// 0x48 saved_rot, 0x58 saved_parent). The rotation is x/y/z/s like S_quat
/// (vector, then scalar), matching StagedActorRecord. The frame/matrix owner
/// stays DEFERRED; the caller stages the live words.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveFramePlacement {
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub parent: Option<FrameReferenceId>,
}
/// GAME 0x405e60 (RVA 0x5e60) C_actor::InternalPlacementReset snapshot core
/// (mafia-re 113b5b6 game/actors/C_actor.cpp:960-968): without a frame the
/// native returns early and keeps nothing; a staged frame's pos/rot/parent
/// words pass through as the saved words. The in-actor saved_* stores stay
/// DEFERRED with the actor owner; the caller keeps the returned words. The
/// GameInit saved_on writer is a separate producer and is not staged here.
pub fn snapshot_placement_for_reset(frame: Option<LiveFramePlacement>) -> Option<LiveFramePlacement> {
    frame
}
/// GAME 0x405ee0 (RVA 0x5ee0) C_actor::ResetPlacement frame-flag core
/// (mafia-re 113b5b6 game/actors/C_actor.cpp:971-986): clear
/// FRMFLAGS_HR_BOUND_W_VALID (0x100), FRMFLAGS_WMAT_VALID (0x20),
/// FRMFLAGS_MAT_VALID (0x10) and FRMFLAGS_ROT_VALID (0x4), then set
/// FRMFLAGS_UPDATE (0x40000000) and FRMFLAGS_QUAT_VALID (0x8), exactly the
/// native mask surgery (mafia-re 113b5b6 LS3DF/I3D/I3D.h:259-267). The
/// LinkTo/SetPos/SetRot/Update/SetOn frame writes and the saved-rotation
/// Normalize (LS3DF math owner) stay DEFERRED with the frame owner; the
/// caller stages the current flags and applies the returned word with the
/// staged saved pos/rot/parent/on words.
pub fn restored_placement_flags(current_flags: u32) -> u32 {
    const CLEAR: u32 = 0x100 | 0x20 | 0x10 | 0x4;
    const SET: u32 = 0x4000_0000 | 0x8;
    (current_flags & !CLEAR) | SET
}
/// GAME 0x406d80 (RVA 0x6d80) C_actor::SaveGameLoadGuid number core
/// (mafia-re 113b5b6 game/actors/C_actor.cpp:1343-1349): the staged savegame
/// version/number words (S_actor_savegame 0x0 version, 0x2 number per
/// game/actors/C_actor.h:138-148); the actor number is taken only for
/// ACTOR_SAVEGAME_VERSION (3), else left alone. The buffer cursor and the
/// in-actor number store stay DEFERRED; the caller applies the returned word.
pub fn load_guid_number_for_version(version: u8, number: i32) -> Option<i32> {
    const ACTOR_SAVEGAME_VERSION: u8 = 3;
    (version == ACTOR_SAVEGAME_VERSION).then_some(number)
}
/// GAME 0x407cd0 (RVA 0x7cd0) C_actor::DebustizeFrame EXPLICIT
/// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_actor.cpp:1613-1624):
/// clear the projector mode on the frame's visuals, the frame's and its
/// children's, from the last child. The frame/visual owners do not exist
/// here, so no mode is cleared.
pub fn debustize_frame_unsupported() {}
/// GAME 0x407d10 (RVA 0x7d10) C_actor::NoShadowFrame EXPLICIT
/// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_actor.cpp:1626-1637):
/// the same walk for the shadow mode. The frame/visual owners do not exist
/// here, so no mode is cleared.
pub fn no_shadow_frame_unsupported() {}
/// Byte offset of the 0x24 pos triplet inside the staged 0x21 window.
const POS_WINDOW_OFFSET: usize = 0x24 - 0x21;
/// Byte offset of the 0x30 dir triplet inside the staged 0x21 window.
const DIR_WINDOW_OFFSET: usize = 0x30 - 0x21;
/// One staged C_actor spline key for the GetRecordPos/GetRecordRot walk
/// cores (mafia-re 113b5b6 game/actors/C_actor.h:106-122 S_actor_record):
/// the key time, the frame position/rotation, the Hermite tangents and the
/// position/rotation eases, exactly the words the two walks read. The
/// rotation is x/y/z/s like S_quat (vector, then scalar). The 12 unknown
/// bytes, the anim word and the per-type record_sizes byte stepping live
/// with the recorder owner; callers stage the keys in record order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StagedActorRecord {
    pub time: i32,
    pub pos: [f32; 3],
    pub rot: [f32; 4],
    pub tangent_out: [f32; 3],
    pub tangent_in: [f32; 3],
    pub ease_from: f32,
    pub ease_to: f32,
    pub rot_ease_from: f32,
    pub rot_ease_to: f32,
}
/// Shared Ease core of the record walks (mafia-re 113b5b6
/// game/mission/C_recorder.cpp:36-57): the time between two keys eased over
/// `a`, then linear, then over `b` (`a` and `b` scaled down to a sum of 1);
/// the time itself where they add up to nothing. Private helper, no port
/// claimed: the out-of-line Ease (GAME 0x598410) belongs to the recorder
/// owner. The f32 operations replicate the original exactly, including the
/// divide-by-zero NaN over a zero-length ease edge.
fn cutscene_ease(u: f32, a: f32, b: f32) -> f32 {
    let s = a + b;
    if s < 0.001 {
        return u;
    }
    let (a, b) = if s > 1.0 { (a / s, b / s) } else { (a, b) };
    let k = 1.0 / (2.0 - a - b);
    if u < a {
        return k / a * u * u;
    }
    if u < 1.0 - b {
        return k * (2.0 * u - a);
    }
    let u = 1.0 - u;
    1.0 - k / b * u * u
}
/// Bracket walk shared by the GetRecordPos/GetRecordRot cores (mafia-re
/// 113b5b6 game/actors/C_actor.cpp:855-865 and :1871-1881): the last key at
/// or before the query time plus the key after it, or the same key twice
/// past the last key time. The recorder's raw byte stepping by
/// record_sizes[type] is replaced by the staged slice order; an empty
/// slice never reaches this helper (callers return the zero
/// position/rotation first) and a missing successor (malformed input the
/// original trusts) settles on the available key. Returns the (previous,
/// next) indexes.
fn record_bracket(records: &[StagedActorRecord], last_key_time: i32, time: i32) -> (usize, usize) {
    let mut prev = 0;
    for (index, record) in records.iter().enumerate() {
        if record.time > last_key_time {
            break;
        }
        if record.time <= time {
            prev = index;
        }
        if record.time >= last_key_time {
            break;
        }
    }
    let next = if records[prev].time < last_key_time && prev + 1 < records.len() {
        prev + 1
    } else {
        prev
    };
    (prev, next)
}
/// GAME 0x405a00 (RVA 0x5a00) C_actor::GetRecordPos walk-and-blend core
/// (mafia-re 113b5b6 game/actors/C_actor.cpp:849-884): the last record up
/// to the query time, blended towards the next by Hermite's basis over the
/// eased fraction, else the bracketing record's own position; the zero
/// vector without records. The recorder's block walk (first_block,
/// last_key_time) arrives staged as the record slice plus the last key
/// time. The rotation ease the original computes and discards is recomputed
/// and dropped the same way, so the staged rotation-ease words are read
/// exactly like the original reads them.
pub fn record_pos_at(records: &[StagedActorRecord], last_key_time: i32, time: i32) -> [f32; 3] {
    if records.is_empty() {
        return [0.0, 0.0, 0.0];
    }
    let (prev, next) = record_bracket(records, last_key_time, time);
    let prev_record = records[prev];
    let next_record = records[next];
    if time <= prev_record.time {
        return prev_record.pos;
    }
    if next != prev && time <= next_record.time {
        let span = (next_record.time - prev_record.time) as f32;
        let f = (time - prev_record.time) as f32 / span;
        let _rotation_ease = cutscene_ease(f, prev_record.rot_ease_from, next_record.rot_ease_to);
        let s = cutscene_ease(f, prev_record.ease_from, next_record.ease_to);
        let s2 = s * s;
        let s3 = s2 * s;
        let w0 = 2.0 * s3 - 3.0 * s2 + 1.0;
        let w1 = 3.0 * s2 - 2.0 * s3;
        let w2 = s3 - 2.0 * s2 + s;
        let w3 = s3 - s2;
        return std::array::from_fn(|i| {
            prev_record.pos[i] * w0
                + next_record.pos[i] * w1
                + prev_record.tangent_out[i] * w2
                + next_record.tangent_in[i] * w3
        });
    }
    next_record.pos
}
/// Slerp core of the rotation walk below, x/y/z/s order like S_quat
/// (vector, then scalar). Follows LS3DF S_quat::Slerp (mafia-re 113b5b6
/// ls3df/I3D/I3d_math.cpp:1188-1223 with SLERP_DELTA 0.0001): the shorter
/// arc always wins (the native `shorten` flag is read but never changes the
/// negate-on-negative-dot select), near-unit dots fall back to the linear
/// blend, and no normalization occurs. The delta-bound comparison keeps the
/// original float-vs-double promotion. Host f32 acos/sin back the
/// spherical branch: its transcendental rounding is host math, so only
/// branch selection and algebra are pinned here. Private helper, no port
/// claimed.
fn quat_slerp_short(first: [f32; 4], second: [f32; 4], fraction: f32) -> [f32; 4] {
    let mut cosom = first[0] * second[0] + first[1] * second[1] + first[2] * second[2]
        + first[3] * second[3];
    let mut to = second;
    if cosom < 0.0 {
        cosom = -cosom;
        to = [-second[0], -second[1], -second[2], -second[3]];
    }
    let (scale0, scale1) = if f64::from(1.0 - cosom) > 0.0001 {
        let omega = cosom.acos();
        let sinom = omega.sin();
        (
            ((1.0 - fraction) * omega).sin() / sinom,
            (fraction * omega).sin() / sinom,
        )
    } else {
        (1.0 - fraction, fraction)
    };
    [
        scale0 * first[0] + scale1 * to[0],
        scale0 * first[1] + scale1 * to[1],
        scale0 * first[2] + scale1 * to[2],
        scale0 * first[3] + scale1 * to[3],
    ]
}
/// GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot walk-and-slerp core
/// (mafia-re 113b5b6 game/actors/C_actor.cpp:1865-1893): the same bracket
/// walk as GetRecordPos, then the previous rotation, or the slerp towards
/// the next over the plain time fraction (the rotation ease plays no role
/// here); the zero quaternion without records. Same staging contract as
/// [`record_pos_at`].
pub fn record_rot_at(records: &[StagedActorRecord], last_key_time: i32, time: i32) -> [f32; 4] {
    if records.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }
    let (prev, next) = record_bracket(records, last_key_time, time);
    let prev_rot = records[prev].rot;
    let next_rot = records[next].rot;
    if time <= records[prev].time {
        return prev_rot;
    }
    if next != prev && time <= records[next].time {
        let span = (records[next].time - records[prev].time) as f32;
        let f = (time - records[prev].time) as f32 / span;
        return quat_slerp_short(prev_rot, next_rot, f);
    }
    next_rot
}
/// Evidenced primary vtable identities; opaque tags supply no derived behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorVtable {
    Base,
    Detector,
    Railway,
    Bridge,
    Door,
    TrafficCar,
    Pumper,
    BoxNew,
    Human,
    Player,
    Entity,
    Car,
    CarEx,
    Clock,
    ActorShot,
    RailGenerator,
    Racecam,
    TrafficGenerator,
    Opaque(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorPhase {
    Constructing,
    Ready,
    Initializing,
    InitializationInterrupted,
    Loading,
    LoadingInterrupted,
    Destroying,
    DestructionInterrupted,
    Dead,
}
#[derive(Debug)]
struct OwnedActor {
    lifetime: ActorLifetime,
    state: BaseActorState,
    phase: ActorPhase,
    detector: Option<DetectorState>,
    railway: Option<RailwayState>,
    bridge: Option<BridgeState>,
    door: Option<DoorState>,
    traffic_car: Option<TrafficCarState>,
    pumper: Option<PumperState>,
    box_new: Option<BoxNewState>,
    human: Option<HumanState>,
    player: Option<PlayerState>,
    entity: Option<EntityState>,
    car: Option<CarActorState>,
    car_ex: Option<CarExState>,
    clock: Option<ClockState>,
    actor_shot: Option<ActorShotState>,
    rail_generator: Option<RailGeneratorState>,
    racecam: Option<RacecamState>,
    traffic: Option<TrafficGeneratorState>,
}
/// Untouched native detector storage, including the embedded program seed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DetectorSeed {
    pub program: ProgramSeed,
    pub bytes_d8_ef: [u8; 24],
    pub word_f4: u32,
    pub bytes_105_107: [u8; 3],
    pub bytes_118_127: [u8; 16],
}
/// Actual source-only C_detector storage owned by the same actor incarnation.
/// The optional program is taken only during a serialized payload operation;
/// actor phase prevents same-actor reentry while context reads the live owner.
/// Admitted: construct_detector, C_detector::Init via the generic init prefix
/// (base init plus the +0xd8 human_types zero and program owner assignment),
/// C_program::Init/Compile via init_fresh_runtime with the base table, the
/// take/return_detector_program transport, and the C_detector::Process dispatch
/// owners below (human_types read, in-range candidate gate, collision-arm
/// flags, signal). The tick transport itself lives on the mission program
/// world.
/// DEFERRED (no credit, untouched here): the C_detector::GameInit body beyond
/// the latch-nulling slice (C_actor::GameInit call, actor=self assignment,
/// C_program::Init, unknown_65 handling, DebustizeFrame) and the coll-pointer
/// nulling (colls/matrices, num), GameDone, and the CreateColls/EraseColls/
/// CreateUO collision owners.
#[derive(Debug)]
pub struct DetectorState {
    program: Option<CProgram>,
    /// Detector +0xdc signal bit (mafia-re game/actors/C_detector.h:60-71:
    /// GetSignal GAME 0x47b750 reads it, SetSignal GAME 0x47b760 stores it;
    /// natively cleared by GameInit GAME 0x432a00, collapsed to zero here).
    /// Typed owner for the evidenced bit only; the surrounding +0xd8 raw
    /// bytes stay untouched in bytes_d8_ef, and the actor +0x65 clear that
    /// SetSignal performs first lives on the world owner (see
    /// ActorWorld::set_detector_signal), which alone reaches base storage.
    /// Note: bytes_d8_ef[4] positionally images +0xdc but is stale seed that
    /// set_signal never rewrites; all readers use signal, never that byte.
    pub signal: bool,
    /// DETECTOR_WAITFORHIT latch at +0xde (mafia-re
    /// game/actors/C_detector.h:79-81): set while waiting for a hit with
    /// static collisions; CREATEDYNCOLL and ERASEDYNCOLL do nothing then.
    /// Natively nulled by GameInit (GAME 0x432a00, C_detector.cpp:44-58);
    /// collapsed to zero here like signal.
    pub wait_for_hit: bool,
    /// DETECTOR_CREATEDYNCOLL latch at +0xdf (mafia-re
    /// game/actors/C_detector.h:82-84): dynamic collisions following the frame
    /// (Update) until ERASEDYNCOLL. Natively nulled by GameInit; collapsed to
    /// zero here. The collision arrays it gates stay with the future
    /// collision owner.
    pub dyn_colls: bool,
    pub bytes_d8_ef: [u8; 24],
    pub word_f0: u32,
    pub word_f4: u32,
    pub words_f8_100: [u32; 3],
    pub byte_104: u8,
    pub bytes_105_107: [u8; 3],
    pub words_108_114: [u32; 4],
    pub bytes_118_127: [u8; 16],
}
impl DetectorState {
    pub fn program(&self) -> Option<&CProgram> {
        self.program.as_ref()
    }
    /// Detector-side half of C_detector::GetSignal (mafia-re
    /// game/actors/C_detector.h:60-64, GAME 0x47b750): read the +0xdc signal
    /// bit. Full original semantics (live-owner resolution) are on
    /// ActorWorld::detector_signal.
    pub fn get_signal(&self) -> bool {
        self.signal
    }
    /// Detector-side half of C_detector::SetSignal (mafia-re
    /// game/actors/C_detector.h:65-71, GAME 0x47b760): store the +0xdc signal
    /// bit. The actor +0x65 clear that the original performs first is not
    /// reachable from this owner; use ActorWorld::set_detector_signal for the
    /// complete behavior.
    pub fn set_signal(&mut self, on: bool) {
        self.signal = on;
    }
    /// Owned slice of C_detector::GameInit (mafia-re game/actors/
    /// C_detector.cpp:44-62, GAME 0x432a00): null the signal and the
    /// wait_for_hit/dyn_colls latches. The C_actor::GameInit call, the
    /// actor=self assignment, C_program::Init, unknown_65 handling,
    /// DebustizeFrame and the coll-pointer nulling (colls/matrices, num)
    /// stay with their future owners.
    pub fn apply_game_init(&mut self) {
        self.signal = false;
        self.wait_for_hit = false;
        self.dyn_colls = false;
    }
    pub fn data_d8(&self) -> u32 {
        u32::from_le_bytes(self.bytes_d8_ef[..4].try_into().unwrap())
    }
}
/// Read actual current actor attachment and frame full name at the replacement
/// comparison point. Mission path comes from the caller's mission owner; no
/// model root or metadata name is promoted to mission context.
pub struct ActorProgramContext<'a> {
    pub world: &'a ActorWorld,
    pub frames: &'a dyn FrameLookupBackend,
    pub mission_path: Option<&'a [u8]>,
}
impl ProgramContext for ActorProgramContext<'_> {
    fn default_c_locale(&self) -> bool {
        self.frames.default_c_locale()
    }
    fn mission_path(&mut self) -> Result<Option<&[u8]>, String> {
        Ok(self.mission_path)
    }
    fn owner_frame_name(
        &mut self,
        owner: Option<ActorReferenceId>,
    ) -> Result<OwnerFrameName<'_>, String> {
        let Some(owner) = owner else {
            return Ok(OwnerFrameName::NoFrame);
        };
        let state = self.world.state(owner).map_err(|e| format!("{e:?}"))?;
        let Some(frame) = state.frame else {
            return Ok(OwnerFrameName::NoFrame);
        };
        let view = self.frames.frame(frame).map_err(|e| format!("{e:?}"))?;
        Ok(OwnerFrameName::Frame(view.name))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorSource {
    Static,
    Primary,
    Active,
}
/// Opaque native backing allocation retained by the supplied allocator owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorStorageId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceBacking {
    pub storage: Option<ActorStorageId>,
    pub capacity: usize,
}
#[derive(Debug)]
struct ActorCollection {
    values: Vec<Option<ActorReferenceId>>,
    backing: SourceBacking,
    /// On a failed free the new native allocation still belongs to the backend.
    /// The old source/header has not committed. Further insertion is rejected.
    pending: Option<SourceBacking>,
}
impl ActorCollection {
    fn new(backing: SourceBacking) -> Result<Self, ActorWorldError> {
        if backing.capacity > MAX_ACTOR_SOURCES
            || (backing.capacity != 0 && backing.storage.is_none())
        {
            return Err(ActorWorldError::InvalidBacking);
        }
        let mut values = Vec::new();
        values
            .try_reserve(backing.capacity)
            .map_err(|_| ActorWorldError::HostAllocation)?;
        Ok(Self {
            values,
            backing,
            pending: None,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorWorldError {
    NullIdentity,
    DuplicateIdentity,
    InvalidBacking,
    HostAllocation,
    SourceLimit,
    StaleActor(ActorReferenceId),
    Busy(ActorReferenceId),
    IncompleteGrowth(ActorSource),
    UnsupportedKind(u32),
    UnsupportedManagerFlag,
    MissingAttachedFrame,
    UnsupportedVtable,
    NotDetector,
    NotRailway,
    NotBridge,
    NotDoor,
    NotTrafficCar,
    NotPumper,
    NotBoxNew,
    NotClock,
    NotActorShot,
    NotRailGenerator,
    NotRacecam,
    NotCarEx,
    BridgeTableUnavailable,
    BridgeTableTruncated,
    NotHuman,
    NotPlayer,
    NotEntity,
    NotTrafficGenerator,
    HumanData(HumanDataError),
    Program(ProgramError),
    Effect { stage: &'static str, detail: String },
}
#[derive(Debug, PartialEq, Eq)]
pub struct ActorCreationError {
    /// An interrupted constructor has already created this owned identity.
    pub actor: Option<ActorReferenceId>,
    pub source: ActorWorldError,
}
/// Allocate/free the original vector backing. Calls are serialized and must not
/// reenter source mutation. Reads of `world` observe the original header until
/// free completes; context primary commits before active allocation begins.
/// Errors are host availability failures, not recovered native allocator returns.
/// A failed free must retain both allocations for explicit backend recovery.
pub trait ActorCollectionBackend {
    fn allocate_collection(
        &mut self,
        world: &ActorWorld,
        source: ActorSource,
        bytes: u32,
    ) -> Result<ActorStorageId, String>;
    fn free_collection(
        &mut self,
        world: &ActorWorld,
        source: ActorSource,
        old: Option<ActorStorageId>,
    ) -> Result<(), String>;
}
/// Distinguishes a null frame userdata pointer from nonnull userdata whose first
/// actor word is null. Both are accepted by base Init.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameActorData {
    Null,
    Present(Option<ActorReferenceId>),
}
/// Actual frame operations; flags 0x40 are not the script lookup's 0x4ffff.
/// Callbacks may synchronously edit actor state and sources in this live world.
/// Reentering construction/Init/destruction of the same actor is rejected.
/// Backend errors preserve the exact already-applied prefix; they never imply
/// successful frame attachment, virtual release, or gameplay initialization.
pub trait ActorFrameBackend {
    fn init_car_owner(
        &mut self,
        _world: &mut ActorWorld,
        _actor: ActorReferenceId,
        _frame: Option<FrameReferenceId>,
    ) -> Result<bool, String> {
        Err("actual car model Init adapter unavailable".into())
    }
    fn destroy_car_resources(
        &mut self,
        _world: &mut ActorWorld,
        _actor: ActorReferenceId,
    ) -> Result<(), String> {
        Err("actual car model destruction adapter unavailable".into())
    }
    fn frame_is_model(
        &mut self,
        _world: &mut ActorWorld,
        _frame: FrameReferenceId,
    ) -> Result<bool, String> {
        Err("actor frame kind unavailable".into())
    }
    fn stop_model_animation(
        &mut self,
        _world: &mut ActorWorld,
        _frame: FrameReferenceId,
        _index: u32,
    ) -> Result<(), String> {
        Err("model StopAnimation unavailable".into())
    }
    fn actor_data(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<FrameActorData, String>;
    fn clear_actor_data(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<(), String>;
    fn find_child(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
        name: &[u8],
        flags: u32,
    ) -> Result<Option<FrameReferenceId>, String>;
    fn set_child_enabled(
        &mut self,
        world: &mut ActorWorld,
        child: FrameReferenceId,
        enabled: u32,
    ) -> Result<(), String>;
    fn release_frame(
        &mut self,
        world: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<(), String>;
}
/// Allocator for supported actor factory branches. Returning storage and
/// seed does not construct a frame, BODY or mission payload.
pub trait ActorAllocationBackend {
    fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String>;
    fn allocate_traffic_generator(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, TrafficGeneratorSeed)>, String> {
        Err("traffic generator allocation unavailable".into())
    }
    /// Actual heap effect; content storage is published into the typed actor.
    /// Calls do not reenter actor mutation. Err/unwind may retain a heap lease.
    fn allocate_traffic_backing(
        &mut self,
        _world: &ActorWorld,
        _actor: ActorReferenceId,
        _kind: TrafficBackingKind,
        _bytes: u32,
    ) -> Result<Option<TrafficAllocationId>, String> {
        Err("traffic backing allocation unavailable".into())
    }
    fn allocate_car(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, CarSeed)>, String> {
        Err("car allocation unavailable".into())
    }
    fn allocate_player(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, PlayerSeed)>, String> {
        Err("player allocation unavailable".into())
    }
    fn allocate_entity(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, EntitySeed)>, String> {
        Err("entity allocation unavailable".into())
    }
    fn allocate_railway(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, RailwaySeed)>, String> {
        Err("railway allocation unavailable".into())
    }
    /// Kind5 requires actual larger storage and its untouched allocation seed.
    fn allocate_detector(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
        Err("detector allocation unavailable".into())
    }
    /// Kind20 requires actual 0x194-byte storage and its untouched seed.
    fn allocate_bridge(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, BridgeSeed)>, String> {
        Err("bridge allocation unavailable".into())
    }
    /// Kind6 requires actual 0x1b8-byte storage and its untouched seed.
    fn allocate_door(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, DoorSeed)>, String> {
        Err("door allocation unavailable".into())
    }
    /// Kind12 requires actual 0x26c-byte storage and its untouched seed.
    fn allocate_traffic_car(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, TrafficCarSeed)>, String> {
        Err("traffic car allocation unavailable".into())
    }
    /// Kind25 requires actual 0x2ac-byte storage and its untouched seed.
    fn allocate_pumper(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, PumperSeed)>, String> {
        Err("pumper allocation unavailable".into())
    }
    /// Kind35 requires actual 0x3a8-byte storage and its untouched seed.
    fn allocate_box_new(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, BoxNewSeed)>, String> {
        Err("box allocation unavailable".into())
    }
    /// Kind34 requires actual 0x88-byte storage and its untouched seed.
    fn allocate_clock(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, ClockSeed)>, String> {
        Err("clock allocation unavailable".into())
    }
    /// Kind33 requires actual 0x8c-byte storage and its untouched seed.
    fn allocate_actor_shot(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, ActorShotSeed)>, String> {
        Err("actor shot allocation unavailable".into())
    }
    /// Kind38 requires actual 0x84-byte storage and its untouched seed.
    fn allocate_rail_generator(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, RailGeneratorSeed)>, String> {
        Err("rail generator allocation unavailable".into())
    }
    /// Kind36 requires actual 0x227c-byte storage and its untouched seed.
    fn allocate_car_ex(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, CarExSeed)>, String> {
        Err("car_ex allocation unavailable".into())
    }
    /// Kind28 requires actual 0x88-byte storage and its untouched seed.
    fn allocate_racecam(
        &mut self,
        _bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, RacecamSeed)>, String> {
        Err("racecam allocation unavailable".into())
    }
}
/// Native backing/free dependencies. A failed scalar `free_actor` retains its
/// backend lease after object destruction. Traffic's reached resource frees may
/// instead have uncertain post-effect failure, as specified on that method.
pub trait ActorDeallocationBackend {
    /// May Err/unwind after the actual free. The caller latches interruption
    /// before entry and must use explicit host disposal, never replay this free.
    fn free_traffic_backing(
        &mut self,
        _world: &ActorWorld,
        _actor: ActorReferenceId,
        _kind: TrafficBackingKind,
        _allocation: Option<TrafficAllocationId>,
    ) -> Result<(), String> {
        Err("traffic backing free unavailable".into())
    }
    /// Explicit host terminal disposal of still-live actual heap leases. This
    /// must not replay original free callbacks after an uncertain prior effect.
    fn discard_traffic_backings_host(
        &mut self,
        _world: &ActorWorld,
        _actor: ActorReferenceId,
    ) -> Result<(), String> {
        Err("traffic host disposal unavailable".into())
    }
    /// Constructor-domain collections are genuinely empty. The native free
    /// boundary is still called in order, including its null allocation.
    fn free_human_collection(
        &mut self,
        _world: &ActorWorld,
        _actor: ActorReferenceId,
        _slot: HumanResourceSlot,
    ) -> Result<(), String> {
        Err("human empty collection disposal unavailable".into())
    }
    fn free_entity_collection(
        &mut self,
        _world: &ActorWorld,
        _actor: ActorReferenceId,
        _slot: EntityResourceSlot,
    ) -> Result<(), String> {
        Err("entity empty collection disposal unavailable".into())
    }
    /// Scalar free of each actual railway-owned backing, including null. Failed
    /// frees retain the header and backend ownership for explicit recovery.
    fn free_railway_allocation(
        &mut self,
        _world: &ActorWorld,
        _actor: ActorReferenceId,
        _slot: RailwayVectorSlot,
        _allocation: Option<RailwayAllocation>,
    ) -> Result<(), String> {
        Err("railway backing deallocation unavailable".into())
    }
    fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String>;
}
/// One manager+0x90 association record (native stride 0x58). Unknown payload is
/// preserved; the flag-zero destructor branch only reads/clears `actor`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorAssociation {
    OpaqueLegacy {
        prefix: [u8; 80],
        actor: Option<ActorReferenceId>,
        tail: u32,
    },
    Recorded(RecordedActorAssociation),
}
impl ActorAssociation {
    pub fn actor(&self) -> Option<ActorReferenceId> {
        match self {
            Self::OpaqueLegacy { actor, .. } => *actor,
            Self::Recorded(record) => record.actor,
        }
    }
    fn clear_actor(&mut self) {
        match self {
            Self::OpaqueLegacy { actor, .. } => *actor = None,
            Self::Recorded(record) => record.actor = None,
        }
    }
}

#[derive(Debug, Default)]
pub struct ActorManager {
    pub associations: Vec<ActorAssociation>,
    pub records: CutsceneRecordArena,
    pub first: Vec<ActorReferenceId>,
    pub second: Vec<ActorReferenceId>,
}
impl ActorManager {
    /// Bounded flag-zero dependency of the base destructor. Flag-one resource
    /// callbacks and actor userdata cleanup are explicitly unsupported.
    pub fn remove_association(
        &mut self,
        actor: ActorReferenceId,
        flag: bool,
    ) -> Result<(), ActorWorldError> {
        if flag {
            return Err(ActorWorldError::UnsupportedManagerFlag);
        }
        if let Some(index) = self
            .associations
            .iter()
            .position(|r| r.actor() == Some(actor))
        {
            self.associations[index].clear_actor();
            self.associations.remove(index);
        }
        Ok(())
    }
}
/// Opaque lifetime identity of the real mission actor context. Moving the
/// owner preserves it; moving a manager out cannot impersonate this context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorWorldIdentity(u64);
/// Genuine Rust ownership of base actor storage plus separate native-comparable
/// source collections. This does not construct the mission's derived actors.
#[derive(Debug)]
pub struct ActorWorld {
    identity: ActorWorldIdentity,
    actors: Vec<OwnedActor>,
    static_actors: ActorCollection,
    primary: ActorCollection,
    active: ActorCollection,
    pub players: [Option<ActorReferenceId>; 2],
    pub actor_count: u32,
    pub attachment_counter: u32,
    pub manager: ActorManager,
    railway_global_byte: Option<u8>,
    car_services: Option<CarActorServices>,
    bridge_table: Option<BridgeTable>,
    bridge_sectors: BridgeSectors,
    door_sounds: DoorSoundNames,
    traffic_tuning: TrafficTuning,
}
impl ActorWorld {
    pub(crate) fn lookup_actor_frame(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<FrameReferenceId>, LookupError> {
        let i = self
            .index(actor)
            .map_err(|_| LookupError::StaleActor(actor))?;
        if self.actors[i].phase != ActorPhase::Ready {
            return Err(LookupError::Backend("actor lifecycle is incomplete".into()));
        }
        Ok(self.actors[i].state.frame)
    }
    pub fn identity(&self) -> ActorWorldIdentity {
        self.identity
    }
    /// Supply actual admitted initial counter/backing state. Empty sources are
    /// owned here; caller-provided native backing tokens retain their allocator.
    pub fn new(
        actor_count: u32,
        attachment_counter: u32,
        static_backing: SourceBacking,
        primary_backing: SourceBacking,
        active_backing: SourceBacking,
    ) -> Result<Self, ActorWorldError> {
        Ok(Self {
            identity: ActorWorldIdentity(crate::frame_lookup::incarnation()),
            actors: Vec::new(),
            static_actors: ActorCollection::new(static_backing)?,
            primary: ActorCollection::new(primary_backing)?,
            active: ActorCollection::new(active_backing)?,
            players: [None; 2],
            actor_count,
            attachment_counter,
            manager: ActorManager::default(),
            railway_global_byte: None,
            car_services: None,
            bridge_table: None,
            bridge_sectors: BridgeSectors::default(),
            door_sounds: DoorSoundNames::default(),
            traffic_tuning: TrafficTuning::default(),
        })
    }
    fn collection(&self, source: ActorSource) -> &ActorCollection {
        match source {
            ActorSource::Static => &self.static_actors,
            ActorSource::Primary => &self.primary,
            ActorSource::Active => &self.active,
        }
    }
    fn collection_mut(&mut self, source: ActorSource) -> &mut ActorCollection {
        match source {
            ActorSource::Static => &mut self.static_actors,
            ActorSource::Primary => &mut self.primary,
            ActorSource::Active => &mut self.active,
        }
    }
    pub fn source(&self, source: ActorSource) -> &[Option<ActorReferenceId>] {
        &self.collection(source).values
    }
    pub fn backing(&self, source: ActorSource) -> SourceBacking {
        self.collection(source).backing
    }
    pub fn pending_backing(&self, source: ActorSource) -> Option<SourceBacking> {
        self.collection(source).pending
    }
    fn index(&self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        self.actors
            .iter()
            .position(|a| a.lifetime.reference() == actor && a.phase != ActorPhase::Dead)
            .ok_or(ActorWorldError::StaleActor(actor))
    }
    /// Identities of actual live owned incarnations, including stopped prefixes.
    pub fn owned_actor_references(&self) -> impl Iterator<Item = ActorReferenceId> + '_ {
        self.actors
            .iter()
            .filter(|actor| actor.phase != ActorPhase::Dead)
            .map(|actor| actor.lifetime.reference())
    }
    /// Host terminal storage cleanup after every owned actor has died. Context
    /// lists may retain dead references; they carry no additional ownership.
    /// Each successful free commits its empty header before the next call.
    pub fn discard_empty_source_backings<B: ActorCollectionBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        if let Some(actor) = self.owned_actor_references().next() {
            return Err(ActorWorldError::Busy(actor));
        }
        for source in [
            ActorSource::Static,
            ActorSource::Primary,
            ActorSource::Active,
        ] {
            if self.collection(source).pending.is_some() {
                return Err(ActorWorldError::IncompleteGrowth(source));
            }
            let storage = self.backing(source).storage;
            backend
                .free_collection(self, source, storage)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "discard_source_backing",
                    detail,
                })?;
            let collection = self.collection_mut(source);
            collection.values.clear();
            collection.backing = SourceBacking {
                storage: None,
                capacity: 0,
            };
        }
        Ok(())
    }
    /// Read-only storage inspection, including retired/interrupted incarnations.
    /// This grants no live actor resolution and cannot mutate retired storage.
    pub fn inspect(&self, actor: ActorReferenceId) -> Option<(ActorPhase, &BaseActorState)> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)
            .map(|a| (a.phase, &a.state))
    }
    pub fn state(&self, actor: ActorReferenceId) -> Result<&BaseActorState, ActorWorldError> {
        Ok(&self.actors[self.index(actor)?].state)
    }
    pub fn state_mut(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut BaseActorState, ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].detector.is_some()
            || self.actors[i].railway.is_some()
            || self.actors[i].human.is_some()
            || self.actors[i].car.is_some()
            || self.actors[i].traffic.is_some()
        {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        Ok(&mut self.actors[i].state)
    }
    /// Scene-stream load marker: flags one byte of base storage loaded.
    /// Related to Game.exe 0x547220 SetFromMission (working name,
    /// mafia-re game/actors/C_actor.h:272); the native from_mission field
    /// offset is unevidenced, so this helper takes no function credit for
    /// that RVA. This shared byte store does not expose derived storage or
    /// perform GameInit.
    pub fn mark_scene_loaded(&mut self, actor: ActorReferenceId) -> Result<(), ActorWorldError> {
        self.ready(actor)?;
        self.state_mut_inner(actor)?.byte_66 = 1;
        Ok(())
    }
    fn state_mut_inner(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut BaseActorState, ActorWorldError> {
        let i = self.index(actor)?;
        Ok(&mut self.actors[i].state)
    }
    /// Mutable actual attachment for shared base callbacks. Derived vtable/kind
    /// identity remains protected; native Init rereads this frame between calls.
    pub fn actor_frame_mut(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<&mut Option<FrameReferenceId>, ActorWorldError> {
        Ok(&mut self.state_mut_inner(actor)?.frame)
    }
    pub fn phase(&self, actor: ActorReferenceId) -> Result<ActorPhase, ActorWorldError> {
        Ok(self.actors[self.index(actor)?].phase)
    }
    fn ready(&self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].phase != ActorPhase::Ready {
            return Err(ActorWorldError::Busy(actor));
        }
        Ok(i)
    }
    fn append<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        source: ActorSource,
        actor: Option<ActorReferenceId>,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let c = self.collection(source);
        if c.pending.is_some() {
            return Err(ActorWorldError::IncompleteGrowth(source));
        }
        let len = c.values.len();
        if len >= MAX_ACTOR_SOURCES {
            return Err(ActorWorldError::SourceLimit);
        }
        if len == c.backing.capacity {
            let capacity = len
                .checked_add(len.max(1))
                .filter(|n| *n <= MAX_ACTOR_SOURCES)
                .ok_or(ActorWorldError::SourceLimit)?;
            // Host storage is admitted before calling the native allocation dependency.
            self.collection_mut(source)
                .values
                .try_reserve(capacity - len)
                .map_err(|_| ActorWorldError::HostAllocation)?;
            let storage = backend
                .allocate_collection(self, source, (capacity * 4) as u32)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "allocate_collection",
                    detail,
                })?;
            let new_backing = SourceBacking {
                storage: Some(storage),
                capacity,
            };
            self.collection_mut(source).pending = Some(new_backing);
            let old = self.collection(source).backing.storage;
            backend
                .free_collection(self, source, old)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_collection",
                    detail,
                })?;
            let c = self.collection_mut(source);
            c.backing = new_backing;
            c.pending = None;
        }
        self.collection_mut(source).values.push(actor);
        Ok(())
    }
    /// Native static registration skips null and reverse-deduplicates pointers.
    pub fn add_actor_to_global<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        actor: Option<ActorReferenceId>,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let Some(actor) = actor else {
            return Ok(());
        };
        self.index(actor)?;
        if self
            .static_actors
            .values
            .iter()
            .rev()
            .any(|a| *a == Some(actor))
        {
            return Ok(());
        }
        self.append(ActorSource::Static, Some(actor), backend)
    }
    /// Static erasure fills the selected slot with the old final pointer.
    pub fn del_actor_from_global(&mut self, actor: Option<ActorReferenceId>) {
        if let Some(i) = self.static_actors.values.iter().rposition(|a| *a == actor) {
            self.static_actors.values.swap_remove(i);
        }
    }
    /// GAME 0x407fb0 (RVA 0x7fb0) C_actor::IsGlobalActor (mafia-re 113b5b6
    /// game/actors/C_actor.cpp:1667-1679): false for a null actor, else a
    /// reverse scan of the static (global) collection for the identity.
    /// Stale or unknown identities simply miss.
    pub fn is_global_actor(&self, actor: Option<ActorReferenceId>) -> bool {
        let Some(actor) = actor else {
            return false;
        };
        self.static_actors
            .values
            .iter()
            .rev()
            .any(|slot| *slot == Some(actor))
    }
    /// GAME 0x47b000 (RVA 0x7b000) C_actor::GetGlobalActors (mafia-re
    /// 113b5b6 game/actors/C_actor.h:442-445): the native returns the whole
    /// global_actors vector by reference; this is the live-entry count
    /// projection over the static collection. The vector itself stays with
    /// the collection owner; see add_actor_to_global/del_actor_from_global.
    pub fn global_actor_count(&self) -> usize {
        self.static_actors.values.iter().filter(|a| a.is_some()).count()
    }
    /// Context AddActor returns immediately for a primary duplicate, including
    /// an attempted false-to-true active promotion.
    pub fn add_actor<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        actor: Option<ActorReferenceId>,
        active: bool,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        if let Some(a) = actor {
            self.index(a)?;
        }
        if self.primary.values.iter().rev().any(|a| *a == actor) {
            return Ok(());
        }
        self.append(ActorSource::Primary, actor, backend)?;
        if active && !self.active.values.iter().rev().any(|a| *a == actor) {
            self.append(ActorSource::Active, actor, backend)?;
        }
        Ok(())
    }
    /// Membership erasure preserves survivor order and never destroys an actor.
    /// The return predicate reports primary removal, including active-only cases.
    pub fn unload_actor(&mut self, actor: Option<ActorReferenceId>) -> bool {
        let primary = self.primary.values.iter().rposition(|a| *a == actor);
        if let Some(i) = primary {
            self.primary.values.remove(i);
        }
        if let Some(i) = self.active.values.iter().rposition(|a| *a == actor) {
            self.active.values.remove(i);
        }
        primary.is_some()
    }
    /// The constructor stores only evidenced fields, then increments the global
    /// count before static registration. Interrupted registration retains actor
    /// storage and its identity in Constructing phase, never a usable lookup actor.
    pub fn construct_base<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        seed: BaseActorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        self.construct_base_with_railway(identity, seed, None, backend)
    }
    fn construct_base_with_railway<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        seed: BaseActorSeed,
        railway: Option<RailwayState>,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        let fail = |source| ActorCreationError {
            actor: None,
            source,
        };
        if identity == 0 {
            return Err(fail(ActorWorldError::NullIdentity));
        }
        if self
            .actors
            .iter()
            .any(|a| a.phase != ActorPhase::Dead && a.lifetime.reference().identity() == identity)
        {
            return Err(fail(ActorWorldError::DuplicateIdentity));
        }
        if self.actors.len() >= MAX_ACTOR_SOURCES {
            return Err(fail(ActorWorldError::SourceLimit));
        }
        self.actors
            .try_reserve(1)
            .map_err(|_| fail(ActorWorldError::HostAllocation))?;
        let lifetime = ActorLifetime::for_existing(identity);
        let actor = lifetime.reference();
        self.actors.push(OwnedActor {
            lifetime,
            state: BaseActorState::construct(seed),
            phase: ActorPhase::Constructing,
            detector: None,
            railway,
            bridge: None,
            door: None,
            traffic_car: None,
            pumper: None,
            box_new: None,
            human: None,
            player: None,
            entity: None,
            car: None,
            car_ex: None,
            clock: None,
            actor_shot: None,
            rail_generator: None,
            racecam: None,
            traffic: None,
        });
        self.actor_count = self.actor_count.wrapping_add(1);
        self.add_actor_to_global(Some(actor), backend)
            .map_err(|source| ActorCreationError {
                actor: Some(actor),
                source,
            })?;
        let i = self.index(actor).expect("constructor owns actor");
        self.actors[i].phase = ActorPhase::Ready;
        Ok(actor)
    }
    /// Reached cutscene-manager attach, preserving the existing manager lists.
    /// Only the proven base/Car virtual flag operations are admitted; other
    /// classes retain list/userdata/flag effects before their virtual guard.
    pub fn attach_recorded_actor(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<(), ActorWorldError> {
        self.ready(actor)?;
        if self.manager.first.contains(&actor) || self.manager.second.contains(&actor) {
            return Ok(());
        }
        self.manager
            .first
            .try_reserve(1)
            .map_err(|_| ActorWorldError::HostAllocation)?;
        self.manager.first.push(actor);
        self.state_mut_inner(actor)?.user_data = ActorUserData::Null;
        self.state_mut_inner(actor)?.flags |= 0x10;
        if !matches!(
            self.state(actor)?.vtable,
            ActorVtable::Base | ActorVtable::Car
        ) {
            return Err(ActorWorldError::Effect {
                stage: "recorded_actor_virtual_mode",
                detail: "derived recording mode callbacks unavailable".into(),
            });
        }
        // Actual base virtual7(true), then virtual6(false). Reread live flags
        // at each operation; these native leaves invoke no external callbacks.
        let flags = self.state(actor)?.flags;
        if flags & 1 == 0 {
            self.state_mut_inner(actor)?.flags = flags | 4;
        }
        let flags = self.state(actor)?.flags;
        self.state_mut_inner(actor)?.flags = flags & !0x0a;
        Ok(())
    }
    pub fn set_recorded_actor_data(
        &mut self,
        actor: ActorReferenceId,
        record: RecordedActorId,
    ) -> Result<(), ActorWorldError> {
        self.ready(actor)?;
        self.manager
            .records
            .record(&record)
            .map_err(|error| ActorWorldError::Effect {
                stage: "recorded_actor_data",
                detail: format!("{error:?}"),
            })?;
        self.state_mut_inner(actor)?.user_data = ActorUserData::Recorded(record);
        Ok(())
    }
    pub fn append_recorded_association(
        &mut self,
        association: RecordedActorAssociation,
    ) -> Result<(), ActorWorldError> {
        if let Some(record) = &association.record {
            self.manager
                .records
                .record(record)
                .map_err(|error| ActorWorldError::Effect {
                    stage: "recorded_actor_association",
                    detail: format!("{error:?}"),
                })?;
        }
        self.manager
            .associations
            .try_reserve(1)
            .map_err(|_| ActorWorldError::HostAllocation)?;
        self.manager
            .associations
            .push(ActorAssociation::Recorded(association));
        Ok(())
    }
    /// Explicit host source context. It must be supplied before car construction;
    /// it does not assert an original process random state or run GameInit.
    pub fn configure_car_resources(&mut self, random_seed: u32) -> Result<(), ActorWorldError> {
        if self.car_services.is_some() {
            return Err(ActorWorldError::InvalidBacking);
        }
        self.car_services =
            Some(
                CarActorServices::new(random_seed).map_err(|detail| ActorWorldError::Effect {
                    stage: "car_resources",
                    detail,
                })?,
            );
        Ok(())
    }
    pub fn live_car_table_allocation_count(&self) -> usize {
        self.car_services
            .as_ref()
            .map_or(0, CarActorServices::table_allocations)
    }
    /// Inspector only: includes dead/interrupted actual owners without reviving
    /// them, and validates each pointer against its non-reused backend lease.
    pub fn inspect_car_init_temporaries(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Vec<crate::car_actor::CarInitTemporaryDiagnostic>, ActorWorldError> {
        let car = self
            .inspect_car(actor)
            .ok_or(ActorWorldError::UnsupportedVtable)?;
        let Some(services) = self.car_services.as_ref() else {
            return Ok(Vec::new());
        };
        car.init_temporaries(services)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "car_init_temporaries",
                detail,
            })
    }
    pub fn live_car_track_count(&self) -> usize {
        self.car_services
            .as_ref()
            .map_or(0, CarActorServices::cached_tracks)
    }
    /// Explicit terminal host resource disposal, after actual car destruction.
    /// This releases the retained source cache; it executes no original GameDone.
    pub fn discard_car_services_host(&mut self) -> Result<(), ActorWorldError> {
        if let Some(owner) = self
            .actors
            .iter()
            .find(|owner| owner.car.is_some() && owner.phase != ActorPhase::Dead)
        {
            return Err(ActorWorldError::Busy(owner.lifetime.reference()));
        }
        if let Some(services) = self.car_services.as_mut() {
            for owner in &mut self.actors {
                if let Some(car) = &mut owner.car {
                    car.discard_init_temporaries_host(services)
                        .map_err(|detail| ActorWorldError::Effect {
                            stage: "car_init_temporary_host_discard",
                            detail,
                        })?;
                }
            }
            services
                .discard_tables_host()
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "car_table_host_discard",
                    detail,
                })?;
        }
        if self
            .car_services
            .as_ref()
            .is_some_and(|services| services.allocations() != 0)
        {
            return Err(ActorWorldError::Effect {
                stage: "car_service_host_discard",
                detail: "actual vehicle heap allocations remain owned".into(),
            });
        }
        for owner in &mut self.actors {
            if let Some(car) = &mut owner.car {
                car.discard_source_host();
            }
        }
        self.car_services = None;
        Ok(())
    }
    pub fn live_car_allocation_count(&self) -> usize {
        self.car_services
            .as_ref()
            .map_or(0, CarActorServices::allocations)
            + self
                .actors
                .iter()
                .filter_map(|owner| owner.car.as_ref())
                .map(CarActorState::retained_source_allocation_count)
                .sum::<usize>()
    }
    /// Read-only inspection of the retained actual tail, including dead and
    /// interrupted owners. This does not restore semantic actor liveness.
    pub fn inspect_car(&self, actor: ActorReferenceId) -> Option<&CarActorState> {
        self.actors
            .iter()
            .find(|owner| owner.lifetime.reference() == actor)?
            .car
            .as_ref()
    }
    pub fn car(&self, actor: ActorReferenceId) -> Result<&CarActorState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .car
            .as_ref()
            .ok_or(ActorWorldError::UnsupportedVtable)
    }
    pub fn construct_car<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: CarSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        if self.car_services.is_none() {
            return Err(ActorCreationError {
                actor: None,
                source: ActorWorldError::Effect {
                    stage: "car_resources",
                    detail: "Game car resource context unavailable".into(),
                },
            });
        }
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed base actor");
        self.actors[i].phase = ActorPhase::Constructing;
        let result = (|| {
            let state = CarActorState::allocation(actor, seed).map_err(|detail| {
                ActorWorldError::Effect {
                    stage: "car_allocation",
                    detail,
                }
            })?;
            self.actors[i].car = Some(state);
            let owner = &mut self.actors[i];
            let base = &mut owner.state;
            owner
                .car
                .as_mut()
                .unwrap()
                .construct(self.car_services.as_mut().unwrap(), || {
                    base.vtable = ActorVtable::Car;
                    base.word_10 = 4;
                    Ok(())
                })
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "car_constructor",
                    detail,
                })
        })();
        result.map_err(|source| ActorCreationError {
            actor: Some(actor),
            source,
        })?;
        self.actors[i].phase = ActorPhase::Ready;
        Ok(actor)
    }
    /// Actual shared-arena adapter entry. Base attachment remains the same
    /// ActorWorld operation, between the native car prelude and caller suffix.
    pub(crate) fn init_car_on_arena(
        &mut self,
        actor: ActorReferenceId,
        frame: Option<FrameReferenceId>,
        arena: &mut crate::model_loading::ModelArena,
        installation: Option<&lhr_assets::Installation>,
        default_c_locale: bool,
    ) -> Result<bool, ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].phase != ActorPhase::Initializing {
            return Err(ActorWorldError::Busy(actor));
        }
        let model = self.actors[i]
            .car
            .as_mut()
            .ok_or(ActorWorldError::UnsupportedVtable)?
            .prepare_init(
                self.car_services
                    .as_mut()
                    .ok_or(ActorWorldError::InvalidBacking)?,
                arena,
                frame,
            )
            .map_err(|detail| ActorWorldError::Effect {
                stage: "car_init_prelude",
                detail,
            })?;
        let Some(frame) = frame else {
            return Ok(false);
        };
        if !self.init_inner(
            actor,
            frame,
            &mut crate::actor_frames::ModelActorFrames::borrow(arena, default_c_locale),
        )? {
            return Ok(false);
        }
        self.actors[i]
            .car
            .as_mut()
            .unwrap()
            .finish_init(
                self.car_services.as_mut().unwrap(),
                arena,
                installation,
                model,
            )
            .map_err(|detail| ActorWorldError::Effect {
                stage: "car_init",
                detail,
            })
    }
    /// Shared C_car/C_car_ex LoadData owner. Native C_car_ex::LoadData is a
    /// stub (STUB GAME 0x42f110 falling through to the C_car::LoadData stub,
    /// mafia-re game/vehicles/C_car_ex.cpp:60-62 + C_car.cpp:523-526, which
    /// parses nothing): kind 36 takes the zero-consumption skip below while
    /// kind 4 runs the deep source conversion.
    pub fn load_car_data(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
        arena: &mut crate::model_loading::ModelArena,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].car.is_none()
            || !matches!(
                self.actors[i].state.vtable,
                ActorVtable::Car | ActorVtable::CarEx
            )
        {
            // CarEx without its car half reports the kind error; anything
            // else keeps the generic vtable error.
            if self.actors[i].state.vtable == ActorVtable::CarEx {
                return Err(ActorWorldError::NotCarEx);
            }
            return Err(ActorWorldError::UnsupportedVtable);
        }
        // Native C_car_ex does not override LoadData: STUB 0x42f110 falls
        // through to the C_car::LoadData stub (mafia-re C_car.cpp:523-526),
        // which parses nothing and returns base-true. The host mirrors the
        // stub (zero consumption); the deep packed/parameter conversion
        // stays a post-Init path (it needs primed vehicle init).
        if self.actors[i].state.vtable == ActorVtable::CarEx {
            self.actors[i].phase = ActorPhase::Ready;
            return Ok(true);
        }
        self.actors[i].phase = ActorPhase::Loading;
        let result = self.actors[i]
            .car
            .as_mut()
            .unwrap()
            .load_source(
                self.car_services
                    .as_mut()
                    .ok_or(ActorWorldError::InvalidBacking)?,
                arena,
                reader,
            )
            .map_err(|detail| ActorWorldError::Effect {
                stage: "car_load_data",
                detail,
            });
        self.actors[i].phase = if result.is_err() {
            ActorPhase::LoadingInterrupted
        } else {
            ActorPhase::Ready
        };
        result
    }
    pub(crate) fn destroy_car_on_arena(
        &mut self,
        actor: ActorReferenceId,
        arena: &mut crate::model_loading::ModelArena,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].phase != ActorPhase::Destroying {
            return Err(ActorWorldError::Busy(actor));
        }
        if let Some(reference) = self.actors[i].state.frame {
            let key = crate::frame_transform::FrameKey(reference.identity());
            if arena.lookup_reference(key) != reference || arena.frame(key).is_err() {
                return Err(ActorWorldError::Effect {
                    stage: "car_destructor_arena",
                    detail: "foreign or stale actual base frame".into(),
                });
            }
        }
        self.actors[i]
            .car
            .as_mut()
            .ok_or(ActorWorldError::UnsupportedVtable)?
            .destroy_derived(
                self.car_services
                    .as_mut()
                    .ok_or(ActorWorldError::InvalidBacking)?,
                arena,
            )
            .map_err(|detail| ActorWorldError::Effect {
                stage: "car_destructor",
                detail,
            })
    }
    pub fn destroy_car<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::InitializationInterrupted
                | ActorPhase::LoadingInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].car.is_none() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        let vtable = self.actors[i].state.vtable;
        if !matches!(vtable, ActorVtable::Car | ActorVtable::CarEx) {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = vtable;
        let result = frames
            .destroy_car_resources(self, actor)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "car_destructor",
                detail,
            })
            .and_then(|()| self.destroy_base_inner(actor, frames));
        if result.is_err() {
            let i = self.index(actor)?;
            self.actors[i].phase = ActorPhase::DestructionInterrupted;
        }
        result
    }
    pub fn delete_car<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_car(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_car",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Shared C_car/C_car_ex virtual Release: the car_ex tail carries no
    /// native free, so the car deleting wrapper retires both owners.
    pub fn release_car<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Car | ActorVtable::CarEx
        ) {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_car(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    /// Base registration completes before the embedded source program and
    /// detector vtables are installed. No compiler/GameInit is invoked.
    pub fn construct_detector<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        mut seed: DetectorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        let program_address = identity.checked_add(0x70).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        identity.checked_add(0x127).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        seed.program.address_high_byte = (program_address >> 24) as u8;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructor owns actor");
        self.actors[i].detector = Some(DetectorState {
            program: Some(CProgram::construct(seed.program)),
            // Native clears signal in C_detector::GameInit (GAME 0x432a00,
            // C_detector.cpp:44-58), not in the ctor (0x432880,
            // C_detector.cpp:18-22 sets only type); the zero here collapses
            // that deferred-GameInit effect into construction (safe zero-init,
            // consistent with the neighboring zeroed fields). The same
            // collapse covers wait_for_hit (+0xde) and dyn_colls (+0xdf),
            // nulled by the same GameInit.
            signal: false,
            wait_for_hit: false,
            dyn_colls: false,
            bytes_d8_ef: seed.bytes_d8_ef,
            word_f0: 0,
            word_f4: seed.word_f4,
            words_f8_100: [0; 3],
            byte_104: 0,
            bytes_105_107: seed.bytes_105_107,
            words_108_114: [0; 4],
            bytes_118_127: seed.bytes_118_127,
        });
        self.actors[i]
            .detector
            .as_mut()
            .unwrap()
            .program
            .as_mut()
            .unwrap()
            .set_detector_vtable();
        self.actors[i].state.vtable = ActorVtable::Detector;
        self.actors[i].state.word_10 = 5;
        Ok(actor)
    }
    pub fn detector(&self, actor: ActorReferenceId) -> Result<&DetectorState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .detector
            .as_ref()
            .ok_or(ActorWorldError::NotDetector)
    }
    /// Read-only source ownership inspection, including retired/interrupted actors.
    pub fn inspect_detector(&self, actor: ActorReferenceId) -> Option<&DetectorState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .detector
            .as_ref()
    }
    /// Original C_detector::GetSignal (mafia-re game/actors/C_detector.h:60-64,
    /// GAME 0x47b750, read by the C_program ISSIGNAL case
    /// game/mission/C_program.cpp:3351-3365): read the detector +0xdc signal
    /// bit from the live owner. Const read with host liveness only
    /// (Dead actors report StaleActor); no Ready-phase requirement and no
    /// base-byte effect, matching the single-load native body.
    pub fn detector_signal(&self, actor: ActorReferenceId) -> Result<bool, ActorWorldError> {
        Ok(self.detector(actor)?.get_signal())
    }
    /// Original C_detector::SetSignal (mafia-re game/actors/C_detector.h:65-71,
    /// GAME 0x47b760, stored by the C_program SETSIGNAL case
    /// game/mission/C_program.cpp:3341-3350 dispatching C_program::Process
    /// GAME 0x46d3b0):
    /// clear the base C_actor +0x65 byte, then store the detector +0xdc signal
    /// bit. The +0x65 byte is BaseActorState::bytes_64_65[1]; the sibling +0x64
    /// byte is untouched, matching the single-byte native store. Requires a
    /// Ready actor so construction/loading reentry stays rejected; the native
    /// body itself performs no phase check.
    /// DEFERRED (no credit): the GameInit body beyond the C_program::Init
    /// prefix, GameDone, and the CreateColls/EraseColls/CreateUO collision
    /// owners.
    pub fn set_detector_signal(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_none() {
            return Err(ActorWorldError::NotDetector);
        }
        self.actors[i].state.bytes_64_65[1] = 0;
        if let Some(detector) = self.actors[i].detector.as_mut() {
            detector.set_signal(on);
        }
        Ok(())
    }
    /// Original C_detector +0xd8 human-type mask (mafia-re
    /// game/actors/C_detector.h:73-75; LoadData C_detector.cpp:73-82) for the
    /// DETECTOR_INRANGE scan (C_detector.cpp:205-227, GAME RVA 0x33d10): the
    /// bit mask of C_human::human_type values (C_human.h:951-954) the scan
    /// counts. Const read with host liveness only, like detector_signal; a
    /// non-detector reports NotDetector (the owner is implicit here, so there
    /// is no ISSIGNAL-style silent gate). LoadData stores the mask into
    /// bytes_d8_ef[..4], which data_d8 reads back.
    pub fn detector_human_types(&self, actor: ActorReferenceId) -> Result<u32, ActorWorldError> {
        Ok(self.detector(actor)?.data_d8())
    }
    /// Original DETECTOR_INRANGE candidate gate (mafia-re 113b5b6
    /// game/actors/C_detector.cpp:216-217) for one enumerated actor:
    /// ACTOR_ENTITY/ACTOR_PLAYER with a human record. None is the native type
    /// gate (the scan skips the actor). Some carries the live attached frame
    /// (None for a frameless host-only actor, which the scan skips like
    /// GETACTORSDIST instead of dereferencing null) and the live
    /// C_human::human_type mask. Stale actors propagate; no Ready-phase
    /// requirement, matching the scan's unconditional enumeration.
    pub fn detector_inrange_candidate(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<(Option<FrameReferenceId>, u32)>, ActorWorldError> {
        let i = self.index(actor)?;
        let entry = &self.actors[i];
        if !matches!(
            entry.state.vtable,
            ActorVtable::Entity | ActorVtable::Player
        ) {
            return Ok(None);
        }
        let Some(human) = entry.human.as_ref() else {
            return Ok(None);
        };
        Ok(Some((entry.state.frame, human.human_type_mask())))
    }
    /// Original C_detector collision-arm flags at +0xde/+0xdf (mafia-re
    /// game/actors/C_detector.h:79-84) for the WAITFORHIT, CREATEDYNCOLL and
    /// ERASEDYNCOLL Process arms. Const read with host liveness only, like
    /// detector_signal; a non-detector reports NotDetector.
    pub fn detector_collision_flags(
        &self,
        actor: ActorReferenceId,
    ) -> Result<crate::script_lookup::DetectorCollisionFlags, ActorWorldError> {
        let detector = self.detector(actor)?;
        Ok(crate::script_lookup::DetectorCollisionFlags {
            wait_for_hit: detector.wait_for_hit,
            dyn_colls: detector.dyn_colls,
        })
    }
    /// Store for the flags above, after the arm's condition read. Requires a
    /// Ready actor like set_detector_signal so construction/loading reentry
    /// stays rejected.
    pub fn set_detector_collision_flags(
        &mut self,
        actor: ActorReferenceId,
        flags: crate::script_lookup::DetectorCollisionFlags,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        let detector = self.actors[i]
            .detector
            .as_mut()
            .ok_or(ActorWorldError::NotDetector)?;
        detector.wait_for_hit = flags.wait_for_hit;
        detector.dyn_colls = flags.dyn_colls;
        Ok(())
    }
    pub fn railway_global_byte(&self) -> Option<u8> {
        self.railway_global_byte
    }
    /// Genuine base registration precedes construction of the larger derived
    /// allocation. Tail bytes remain separately owned, never aliased as base.
    pub fn construct_railway<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: RailwaySeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x36f).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base_with_railway(
            identity,
            base,
            Some(RailwayState::allocation(seed)),
            backend,
        )?;
        let i = self.index(actor).expect("constructed railway owner");
        self.actors[i]
            .railway
            .as_mut()
            .unwrap()
            .complete_constructor(identity);
        self.actors[i].state.vtable = ActorVtable::Railway;
        self.actors[i].state.word_10 = 8;
        self.railway_global_byte = Some(0);
        Ok(actor)
    }
    pub fn railway(&self, actor: ActorReferenceId) -> Result<&RailwayState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .railway
            .as_ref()
            .ok_or(ActorWorldError::NotRailway)
    }
    pub fn inspect_railway(&self, actor: ActorReferenceId) -> Option<&RailwayState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .railway
            .as_ref()
    }
    /// Genuine C_bridge construction: the caller's base owner plus the exact
    /// derived constructor bytes for kind 20 (0x194 bytes, vtable 0x625588).
    pub fn construct_bridge<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: BridgeSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x193).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed bridge owner");
        let mut state = BridgeState::allocation(seed);
        state.complete_constructor(identity);
        self.actors[i].bridge = Some(state);
        self.actors[i].state.vtable = ActorVtable::Bridge;
        self.actors[i].state.word_10 = 20;
        Ok(actor)
    }
    pub fn bridge(&self, actor: ActorReferenceId) -> Result<&BridgeState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .bridge
            .as_ref()
            .ok_or(ActorWorldError::NotBridge)
    }
    pub fn inspect_bridge(&self, actor: ActorReferenceId) -> Option<&BridgeState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .bridge
            .as_ref()
    }
    /// Shared ferry table retained from the session's tables\parnik.def read.
    /// A missing file stays absent here; only an arriving bridge record turns
    /// that absence into a LoadData error, matching the native per-call load.
    pub fn configure_bridge_table(&mut self, bytes: Option<Vec<u8>>) {
        self.bridge_table = bytes.map(BridgeTable::from_file);
    }
    pub fn bridge_table(&self) -> Option<&BridgeTable> {
        self.bridge_table.as_ref()
    }
    pub fn bridge_sectors(&self) -> &BridgeSectors {
        &self.bridge_sectors
    }
    /// Original C_bridge::LoadData reads no scene2 payload: it opens
    /// tables\parnik.def and retains the header plus 0x2a0 bytes in shared
    /// storage. The table must have been configured through
    /// configure_bridge_table (the session reads it from the installation on
    /// this path); an absent table fails closed here, matching the native
    /// failed open. Zero payload bytes are consumed here; the enclosing chunk
    /// reader skips the record by its declared extent, as for kind 1.
    pub fn load_bridge_data(&mut self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].bridge.is_none() || self.actors[i].state.vtable != ActorVtable::Bridge {
            return Err(ActorWorldError::NotBridge);
        }
        let bytes = self
            .bridge_table
            .as_ref()
            .map(|table| table.bytes().len())
            .ok_or(ActorWorldError::BridgeTableUnavailable)?;
        if bytes < 8 {
            return Err(ActorWorldError::BridgeTableTruncated);
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(0)
    }
    /// Original C_bridge::Init tail after the base init: record the resolved
    /// sibling flap frame, the model-name L-flag and, unless the flag skips
    /// it, the shared sector refresh. A missing sibling fails closed like the
    /// native null FindFrame result. The name derivation itself lives with
    /// the bridge helpers; only resolved identities cross here.
    pub fn init_bridge_linked(
        &mut self,
        actor: ActorReferenceId,
        sibling: Option<FrameReferenceId>,
        model_starts_with_l: bool,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].bridge.is_none() || self.actors[i].state.vtable != ActorVtable::Bridge {
            return Err(ActorWorldError::NotBridge);
        }
        let Some(sibling) = sibling else {
            return Ok(false);
        };
        let bridge = self.actors[i].bridge.as_mut().expect("admitted bridge");
        bridge.set_frame_handle(sibling.identity());
        if model_starts_with_l {
            bridge.set_model_l_flag(0);
        } else {
            bridge.set_model_l_flag(1);
            self.bridge_sectors.refresh();
        }
        Ok(true)
    }
    pub fn release_bridge<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].bridge.is_none() || self.actors[i].state.vtable != ActorVtable::Bridge {
            return Err(ActorWorldError::NotBridge);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_bridge(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_bridge<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].bridge.is_none() {
            return Err(ActorWorldError::NotBridge);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Bridge;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_bridge<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_bridge(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_bridge",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_door construction: the caller's base owner plus the exact
    /// derived constructor bytes for kind 6 (0x1b8 bytes, vtable 0x623b20).
    /// The three sound names are written to shared world storage, matching
    /// the native repeated strcpy into static buffers.
    pub fn construct_door<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: DoorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x1b7).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed door owner");
        let mut state = DoorState::allocation(seed);
        state.complete_constructor();
        self.actors[i].door = Some(state);
        self.actors[i].state.vtable = ActorVtable::Door;
        self.actors[i].state.word_10 = 6;
        self.door_sounds = DoorSoundNames::construction_defaults();
        Ok(actor)
    }
    pub fn door(&self, actor: ActorReferenceId) -> Result<&DoorState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .door
            .as_ref()
            .ok_or(ActorWorldError::NotDoor)
    }
    pub fn inspect_door(&self, actor: ActorReferenceId) -> Option<&DoorState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .door
            .as_ref()
    }
    /// Shared default sound names written by every door construction.
    pub fn door_sounds(&self) -> &DoorSoundNames {
        &self.door_sounds
    }
    /// Original C_door::LoadData version dispatch over the scene2 payload.
    /// Each field read is individually guarded: a truncated tail keeps the
    /// constructor values for the missing fields, and an absent version word
    /// consumes nothing. Returns consumed payload bytes for host inspection,
    /// not a native status; every path reports native success.
    pub fn load_door_data(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].door.is_none() || self.actors[i].state.vtable != ActorVtable::Door {
            return Err(ActorWorldError::NotDoor);
        }
        let start = reader.position();
        let mut version = [0u8; 4];
        if !reader.read_bounded(&mut version) {
            self.actors[i].phase = ActorPhase::Ready;
            return Ok(0);
        }
        let door = self.actors[i].door.as_mut().expect("admitted door");
        let mut byte = [0u8; 1];
        let mut word = [0u8; 4];
        let mut name = [0u8; 16];
        match DoorLoadVersion::decode(u32::from_le_bytes(version)) {
            DoorLoadVersion::V7 => {
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x11c, byte[0]);
                }
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x11d, byte[0]);
                }
                Self::load_door_v5_fields(door, reader, &mut byte, &mut word, &mut name);
                Self::load_door_tail_names(door, reader, &mut name);
            }
            DoorLoadVersion::V6 => {
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x11d, byte[0]);
                }
                Self::load_door_v5_fields(door, reader, &mut byte, &mut word, &mut name);
                Self::load_door_tail_names(door, reader, &mut name);
            }
            DoorLoadVersion::V5 => {
                Self::load_door_v5_fields(door, reader, &mut byte, &mut word, &mut name);
                Self::load_door_tail_names(door, reader, &mut name);
            }
            DoorLoadVersion::V8 => {
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x11c, byte[0]);
                }
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x11d, byte[0]);
                }
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x11e, byte[0]);
                }
                if reader.read_bounded(&mut word) {
                    door.put_load_word(0x120, u32::from_le_bytes(word));
                }
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x124, byte[0]);
                }
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x125, byte[0]);
                }
                if reader.read_bounded(&mut word) {
                    door.put_load_word(0x128, u32::from_le_bytes(word));
                }
                if reader.read_bounded(&mut word) {
                    door.put_load_word(0x12c, u32::from_le_bytes(word));
                }
                if reader.read_bounded(&mut name) {
                    door.put_load_name(0x130, &name);
                }
                if reader.read_bounded(&mut name) {
                    door.put_load_name(0x140, &name);
                }
                if reader.read_bounded(&mut name) {
                    door.put_load_name(0x150, &name);
                }
                if reader.read_bounded(&mut byte) {
                    door.put_load_byte(0x164, byte[0]);
                }
            }
            DoorLoadVersion::Unknown => {}
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(reader.position() - start)
    }
    /// Version-5 field prefix shared by versions 5-7: one flag byte, one
    /// timer word, two flag bytes, two timer words and the 16-byte open-sound
    /// slot. The caller reads newer leading bytes first, matching the native
    /// fallthrough from the newer case into this shared sequence.
    fn load_door_v5_fields(
        door: &mut DoorState,
        reader: &mut ActorDataReader<'_>,
        byte: &mut [u8; 1],
        word: &mut [u8; 4],
        name: &mut [u8; 16],
    ) {
        if reader.read_bounded(&mut *byte) {
            door.put_load_byte(0x11e, byte[0]);
        }
        if reader.read_bounded(&mut *word) {
            door.put_load_word(0x120, u32::from_le_bytes(*word));
        }
        if reader.read_bounded(&mut *byte) {
            door.put_load_byte(0x124, byte[0]);
        }
        if reader.read_bounded(&mut *byte) {
            door.put_load_byte(0x125, byte[0]);
        }
        if reader.read_bounded(&mut *word) {
            door.put_load_word(0x128, u32::from_le_bytes(*word));
        }
        if reader.read_bounded(&mut *word) {
            door.put_load_word(0x12c, u32::from_le_bytes(*word));
        }
        if reader.read_bounded(&mut *name) {
            door.put_load_name(0x130, name);
        }
    }
    /// Tail name slots shared by versions 5-7 after their `break`: the
    /// close- and locked-sound buffers. Version 8 reads its own tail inline.
    fn load_door_tail_names(
        door: &mut DoorState,
        reader: &mut ActorDataReader<'_>,
        name: &mut [u8; 16],
    ) {
        if reader.read_bounded(&mut *name) {
            door.put_load_name(0x140, name);
        }
        if reader.read_bounded(&mut *name) {
            door.put_load_name(0x150, name);
        }
    }
    pub fn release_door<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].door.is_none() || self.actors[i].state.vtable != ActorVtable::Door {
            return Err(ActorWorldError::NotDoor);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_door(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_door<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].door.is_none() {
            return Err(ActorWorldError::NotDoor);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Door;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_door<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_door(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_door",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_traffic_car construction: the caller's base owner plus the
    /// exact derived constructor bytes for kind 12 (0x26c bytes, vtable
    /// 0x624c98). The three unit floats go to shared world tuning storage,
    /// matching the native static writes.
    pub fn construct_traffic_car<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: TrafficCarSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x26b).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed traffic car owner");
        let mut state = TrafficCarState::allocation(seed);
        state.complete_constructor();
        self.actors[i].traffic_car = Some(state);
        self.actors[i].state.vtable = ActorVtable::TrafficCar;
        self.actors[i].state.word_10 = 12;
        self.traffic_tuning = TrafficTuning::construction_defaults();
        Ok(actor)
    }
    pub fn traffic_car(
        &self,
        actor: ActorReferenceId,
    ) -> Result<&TrafficCarState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .traffic_car
            .as_ref()
            .ok_or(ActorWorldError::NotTrafficCar)
    }
    pub fn inspect_traffic_car(&self, actor: ActorReferenceId) -> Option<&TrafficCarState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .traffic_car
            .as_ref()
    }
    /// Shared traffic tuning written by construction and Init.
    pub fn traffic_tuning(&self) -> &TrafficTuning {
        &self.traffic_tuning
    }
    /// Original C_traffic_car::Init global refresh preceding the base init:
    /// one shared word copy. It persists even when the base init refuses the
    /// frame, so intake runs it before the base init call.
    pub fn refresh_traffic_tuning(&mut self) {
        self.traffic_tuning.init_copy();
    }
    /// Original C_traffic_car::LoadData version dispatch over the scene2
    /// payload. Versions 4 and 5 share the four-word header plus the counted
    /// 0x20-byte element array, retained opaquely; other versions consume
    /// only the version word. Each field read is individually guarded.
    /// Returns consumed payload bytes; every path reports native success.
    pub fn load_traffic_car_data(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].traffic_car.is_none()
            || self.actors[i].state.vtable != ActorVtable::TrafficCar
        {
            return Err(ActorWorldError::NotTrafficCar);
        }
        let start = reader.position();
        let mut version = [0u8; 4];
        if !reader.read_bounded(&mut version) {
            self.actors[i].phase = ActorPhase::Ready;
            return Ok(0);
        }
        if matches!(
            TrafficLoadVersion::decode(u32::from_le_bytes(version)),
            TrafficLoadVersion::V4 | TrafficLoadVersion::V5
        ) {
            let car = self.actors[i]
                .traffic_car
                .as_mut()
                .expect("admitted traffic car");
            for offset in [0x224, 0x228, 0x22c, 0x21c] {
                let mut word = [0u8; 4];
                if reader.read_bounded(&mut word) {
                    car.put_load_word(offset, u32::from_le_bytes(word));
                }
            }
            car.replace_elements(Vec::new());
            let mut count = [0u8; 4];
            if reader.read_bounded(&mut count) {
                let count = u32::from_le_bytes(count);
                car.put_load_word(0x238, count);
                let bytes = (count as usize)
                    .checked_mul(0x20)
                    .ok_or(ActorWorldError::HostAllocation)?;
                let mut elements = Vec::new();
                elements
                    .try_reserve_exact(bytes)
                    .map_err(|_| ActorWorldError::HostAllocation)?;
                elements.resize(bytes, 0);
                for element in elements.chunks_mut(0x20) {
                    reader.read_bounded(element);
                }
                car.replace_elements(elements);
            }
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(reader.position() - start)
    }
    pub fn release_traffic_car<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].traffic_car.is_none()
            || self.actors[i].state.vtable != ActorVtable::TrafficCar
        {
            return Err(ActorWorldError::NotTrafficCar);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_traffic_car(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_traffic_car<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].traffic_car.is_none() {
            return Err(ActorWorldError::NotTrafficCar);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::TrafficCar;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_traffic_car<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_traffic_car(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_traffic_car",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_pumper construction: the caller's base owner plus the exact
    /// derived constructor bytes for kind 25 (0x2ac bytes, vtable 0x624990).
    /// The two lookup words stay opaque zeros until their effect owner
    /// exists; no admitted reader exists yet.
    pub fn construct_pumper<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: PumperSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x2ab).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed pumper owner");
        let mut state = PumperState::allocation(seed);
        state.complete_constructor();
        self.actors[i].pumper = Some(state);
        self.actors[i].state.vtable = ActorVtable::Pumper;
        self.actors[i].state.word_10 = 25;
        Ok(actor)
    }
    pub fn pumper(&self, actor: ActorReferenceId) -> Result<&PumperState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .pumper
            .as_ref()
            .ok_or(ActorWorldError::NotPumper)
    }
    pub fn inspect_pumper(&self, actor: ActorReferenceId) -> Option<&PumperState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .pumper
            .as_ref()
    }
    /// Original C_pumper::LoadData over the scene2 payload: one consumed and
    /// ignored flag byte, then the counter/mode/price words at +0x78/+0x7c/
    /// +0x84/+0x80, then the 0x18-byte indexed-slot block. Each read is
    /// individually guarded. Returns consumed payload bytes; every path
    /// reports native success.
    pub fn load_pumper_data(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].pumper.is_none() || self.actors[i].state.vtable != ActorVtable::Pumper {
            return Err(ActorWorldError::NotPumper);
        }
        let start = reader.position();
        let mut flag = [0u8; 1];
        reader.read_bounded(&mut flag);
        let pumper = self.actors[i].pumper.as_mut().expect("admitted pumper");
        let mut word = [0u8; 4];
        for offset in [0x78, 0x7c, 0x84, 0x80] {
            if reader.read_bounded(&mut word) {
                pumper.put_load_word(offset, u32::from_le_bytes(word));
            }
        }
        let mut slots = [0u8; 0x18];
        if reader.read_bounded(&mut slots) {
            pumper.put_load_slots(&slots);
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(reader.position() - start)
    }
    pub fn release_pumper<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].pumper.is_none() || self.actors[i].state.vtable != ActorVtable::Pumper {
            return Err(ActorWorldError::NotPumper);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_pumper(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_pumper<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].pumper.is_none() {
            return Err(ActorWorldError::NotPumper);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Pumper;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_pumper<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_pumper(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_pumper",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_box_new construction: the caller's base owner plus the kind
    /// marker for kind 35 (0x3a8 bytes, vtable 0x6254f0). The constructor
    /// stores no other derived words; Init and LoadData populate the tail.
    pub fn construct_box_new<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: BoxNewSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x3a7).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed box owner");
        let mut state = BoxNewState::allocation(seed);
        state.complete_constructor();
        self.actors[i].box_new = Some(state);
        self.actors[i].state.vtable = ActorVtable::BoxNew;
        self.actors[i].state.word_10 = 35;
        Ok(actor)
    }
    pub fn box_new(&self, actor: ActorReferenceId) -> Result<&BoxNewState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .box_new
            .as_ref()
            .ok_or(ActorWorldError::NotBoxNew)
    }
    pub fn inspect_box_new(&self, actor: ActorReferenceId) -> Option<&BoxNewState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .box_new
            .as_ref()
    }
    /// Original C_box_new::Init tail after the base init: the six words from
    /// the caller-supplied hierarchy volume (see [`BoxNewState::complete_init`]).
    /// The caller refreshes bounds through the arena; only resolved volumes
    /// cross here. Always admits once reached.
    pub fn init_box_volume(
        &mut self,
        actor: ActorReferenceId,
        volume: f32,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].box_new.is_none() || self.actors[i].state.vtable != ActorVtable::BoxNew {
            return Err(ActorWorldError::NotBoxNew);
        }
        self.actors[i]
            .box_new
            .as_mut()
            .expect("admitted box")
            .complete_init(volume);
        Ok(true)
    }
    /// Original C_box_new::LoadData flag dispatch over the scene2 payload:
    /// variant 0 reads the six words plus +0x3a0, variant 1 those plus +0x348,
    /// variant 2 additionally +0x34c; other flags consume only the flag byte.
    /// Every field read is individually guarded, and the trailing density
    /// clamp runs on all paths. Returns consumed payload bytes; every path
    /// reports native success.
    pub fn load_box_new_data(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].box_new.is_none() || self.actors[i].state.vtable != ActorVtable::BoxNew {
            return Err(ActorWorldError::NotBoxNew);
        }
        let start = reader.position();
        let mut flag = [0u8; 1];
        if !reader.read_bounded(&mut flag) {
            self.actors[i].phase = ActorPhase::Ready;
            return Ok(0);
        }
        let bx = self.actors[i].box_new.as_mut().expect("admitted box");
        let mut byte = [0u8; 1];
        let mut word = [0u8; 4];
        match BoxLoadFlag::decode(flag[0]) {
            BoxLoadFlag::V2 => {
                Self::load_box_flag10_fields(bx, reader, &mut byte, &mut word);
                if reader.read_bounded(&mut word) {
                    bx.put_load_word(0x348, u32::from_le_bytes(word));
                }
                if reader.read_bounded(&mut byte) {
                    bx.put_load_byte(0x378, byte[0]);
                }
                if reader.read_bounded(&mut word) {
                    bx.put_load_word(0x34c, u32::from_le_bytes(word));
                }
            }
            BoxLoadFlag::V1 => {
                Self::load_box_flag10_fields(bx, reader, &mut byte, &mut word);
                if reader.read_bounded(&mut word) {
                    bx.put_load_word(0x348, u32::from_le_bytes(word));
                }
            }
            BoxLoadFlag::V0 => {
                Self::load_box_flag10_fields(bx, reader, &mut byte, &mut word);
            }
            BoxLoadFlag::Other => {}
        }
        bx.clamp_density();
        self.actors[i].phase = ActorPhase::Ready;
        Ok(reader.position() - start)
    }
    /// Flag-1/2 shared prefix: the six words at +0x38c..+0x3a0, each read
    /// individually guarded like the native skip-read chain.
    fn load_box_flag10_fields(
        bx: &mut BoxNewState,
        reader: &mut ActorDataReader<'_>,
        byte: &mut [u8; 1],
        word: &mut [u8; 4],
    ) {
        if reader.read_bounded(&mut *byte) {
            bx.put_load_byte(0x38c, byte[0]);
        }
        for offset in [0x390, 0x394, 0x398, 0x39c, 0x3a0] {
            if reader.read_bounded(&mut *word) {
                bx.put_load_word(offset, u32::from_le_bytes(*word));
            }
        }
    }
    pub fn release_box_new<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].box_new.is_none() || self.actors[i].state.vtable != ActorVtable::BoxNew {
            return Err(ActorWorldError::NotBoxNew);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_box_new(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_box_new<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].box_new.is_none() {
            return Err(ActorWorldError::NotBoxNew);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::BoxNew;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_box_new<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_box_new(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_box_new",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_clock construction: the caller's base owner plus the kind
    /// marker for kind 34 (0x88 bytes, vtable 0x625648). The constructor
    /// (mafia-re game/actors/c_clock.cpp:8-12) stores no other derived words;
    /// Init/GameInit/AI populate the hand words and angles.
    pub fn construct_clock<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: ClockSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x87).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed clock owner");
        let mut state = ClockState::allocation(seed);
        state.complete_constructor();
        self.actors[i].clock = Some(state);
        self.actors[i].state.vtable = ActorVtable::Clock;
        self.actors[i].state.word_10 = 34;
        Ok(actor)
    }
    pub fn clock(&self, actor: ActorReferenceId) -> Result<&ClockState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .clock
            .as_ref()
            .ok_or(ActorWorldError::NotClock)
    }
    pub fn inspect_clock(&self, actor: ActorReferenceId) -> Option<&ClockState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .clock
            .as_ref()
    }
    /// Original C_clock::Init tail after the base init (mafia-re game/actors/
    /// c_clock.cpp:14-29): record the resolved Mala/Velka/Sekundova visual
    /// hand frames. Missing hour or minute hands fail closed like the native
    /// refused init; a missing second hand stores zero and stays admitted,
    /// since the clock may lack one. Only resolved identities cross here; the
    /// ENUMF_VISUAL (0x1) child search lives with the loader link.
    pub fn init_clock_linked(
        &mut self,
        actor: ActorReferenceId,
        hour: Option<FrameReferenceId>,
        minute: Option<FrameReferenceId>,
        second: Option<FrameReferenceId>,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].clock.is_none() || self.actors[i].state.vtable != ActorVtable::Clock {
            return Err(ActorWorldError::NotClock);
        }
        let (Some(hour), Some(minute)) = (hour, minute) else {
            return Ok(false);
        };
        self.actors[i]
            .clock
            .as_mut()
            .expect("admitted clock")
            .set_hand_frames(
                hour.identity(),
                minute.identity(),
                second.map_or(0, |frame| frame.identity()),
            );
        Ok(true)
    }
    /// Original C_clock::GameInit derived math (mafia-re game/actors/
    /// c_clock.cpp:31-52): zero the elapsed/angles, then set both hands from
    /// the supplied local-time hour/minute words. The SetDir/Update frame
    /// effects stay with a future frame effect owner. Original GameDone
    /// (mafia-re c_clock.cpp:54-58) runs only the base Done and stores no
    /// derived word, so it has no effect owner here beyond shared destruction.
    pub fn game_init_clock(
        &mut self,
        actor: ActorReferenceId,
        hour: u16,
        minute: u16,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].clock.is_none() || self.actors[i].state.vtable != ActorVtable::Clock {
            return Err(ActorWorldError::NotClock);
        }
        self.actors[i]
            .clock
            .as_mut()
            .expect("admitted clock")
            .game_init(hour, minute);
        Ok(())
    }
    /// Original C_clock::AI (mafia-re game/actors/c_clock.cpp:73-96): add the
    /// tick milliseconds, turn the minute/hour hands each minute past, and
    /// turn the second hand with the time. See ClockState::advance for the
    /// verbatim transition; frame SetDir/Update effects stay with a future
    /// frame effect owner.
    pub fn tick_clock(
        &mut self,
        actor: ActorReferenceId,
        time: u32,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].clock.is_none() || self.actors[i].state.vtable != ActorVtable::Clock {
            return Err(ActorWorldError::NotClock);
        }
        self.actors[i]
            .clock
            .as_mut()
            .expect("admitted clock")
            .advance(time);
        Ok(())
    }
    /// C_clock has no scene2 LoadData override (mafia-re game/actors/
    /// c_clock.h lists none past the folded literal-success SaveGame leaves):
    /// zero payload bytes are consumed and native success is reported. The
    /// enclosing chunk reader skips the record by its declared extent.
    pub fn load_clock_data(&mut self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].clock.is_none() || self.actors[i].state.vtable != ActorVtable::Clock {
            return Err(ActorWorldError::NotClock);
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(0)
    }
    pub fn release_clock<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].clock.is_none() || self.actors[i].state.vtable != ActorVtable::Clock {
            return Err(ActorWorldError::NotClock);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_clock(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_clock<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].clock.is_none() {
            return Err(ActorWorldError::NotClock);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Clock;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_clock<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_clock(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_clock",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_actor_shot construction: the caller's base owner plus the kind
    /// marker for kind 33 (0x8c bytes, vtable 0x625458). The stub constructor
    /// (mafia-re game/actors/C_actor_shot.cpp:9-12) stores no other derived
    /// words; the firing words stay allocation bytes.
    pub fn construct_actor_shot<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: ActorShotSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x8b).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed shot owner");
        let mut state = ActorShotState::allocation(seed);
        state.complete_constructor();
        self.actors[i].actor_shot = Some(state);
        self.actors[i].state.vtable = ActorVtable::ActorShot;
        self.actors[i].state.word_10 = 33;
        Ok(actor)
    }
    pub fn actor_shot(&self, actor: ActorReferenceId) -> Result<&ActorShotState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .actor_shot
            .as_ref()
            .ok_or(ActorWorldError::NotActorShot)
    }
    pub fn inspect_actor_shot(&self, actor: ActorReferenceId) -> Option<&ActorShotState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .actor_shot
            .as_ref()
    }
    /// C_actor_shot has no scene2 LoadData override (mafia-re game/actors/
    /// C_actor_shot.h lists none past the folded literal-success SaveGame
    /// leaves): zero payload bytes are consumed and native success is reported.
    /// The enclosing chunk reader skips the record by its declared extent.
    pub fn load_actor_shot_data(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].actor_shot.is_none()
            || self.actors[i].state.vtable != ActorVtable::ActorShot
        {
            return Err(ActorWorldError::NotActorShot);
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(0)
    }
    pub fn release_actor_shot<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].actor_shot.is_none()
            || self.actors[i].state.vtable != ActorVtable::ActorShot
        {
            return Err(ActorWorldError::NotActorShot);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_actor_shot(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_actor_shot<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].actor_shot.is_none() {
            return Err(ActorWorldError::NotActorShot);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::ActorShot;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_actor_shot<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_actor_shot(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_actor_shot",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_rail_generator construction: the caller's base owner plus the
    /// kind marker for kind 38 (0x84 bytes, vtable 0x6259e8). The stub
    /// constructor (mafia-re game/vehicles/C_rail_generator.cpp:9-12) stores no
    /// other derived words; Init (mafia-re :26-30) stores the frame and
    /// returns true with no base call — the frame store is unobservable on
    /// the host (no slot, no reader; Update folded empty), so init records
    /// initialization only.
    pub fn construct_rail_generator<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: RailGeneratorSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x83).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed rail owner");
        let mut state = RailGeneratorState::allocation(seed);
        state.complete_constructor();
        self.actors[i].rail_generator = Some(state);
        self.actors[i].state.vtable = ActorVtable::RailGenerator;
        self.actors[i].state.word_10 = 38;
        Ok(actor)
    }
    pub fn rail_generator(
        &self,
        actor: ActorReferenceId,
    ) -> Result<&RailGeneratorState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .rail_generator
            .as_ref()
            .ok_or(ActorWorldError::NotRailGenerator)
    }
    pub fn inspect_rail_generator(&self, actor: ActorReferenceId) -> Option<&RailGeneratorState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .rail_generator
            .as_ref()
    }
    /// Original C_rail_generator::LoadData (STUB 0x596710, mafia-re game/
    /// vehicles/C_rail_generator.cpp:40-44) delegates to C_actor::LoadData:
    /// zero payload bytes are consumed and native success is reported. The
    /// enclosing chunk reader skips the record by its declared extent.
    pub fn load_rail_generator_data(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].rail_generator.is_none()
            || self.actors[i].state.vtable != ActorVtable::RailGenerator
        {
            return Err(ActorWorldError::NotRailGenerator);
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(0)
    }
    pub fn release_rail_generator<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].rail_generator.is_none()
            || self.actors[i].state.vtable != ActorVtable::RailGenerator
        {
            return Err(ActorWorldError::NotRailGenerator);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_rail_generator(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_rail_generator<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].rail_generator.is_none() {
            return Err(ActorWorldError::NotRailGenerator);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::RailGenerator;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_rail_generator<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_rail_generator(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_rail_generator",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Genuine C_car_ex construction: the caller's base owner, the shared car
    /// compatibility owner, plus the 0x60-byte truck tail for kind 36 (0x227c
    /// bytes, vtable 0x6237d8). The stub constructor (mafia-re game/vehicles/
    /// C_car_ex.cpp:9-12) stores nothing past the car prefix; the tail stays
    /// allocation bytes. Init/GameInit/GameDone/destruction reuse the car paths
    /// (C_car_ex stubs delegate to C_car); only the factory allocation, the
    /// kind marker and the tail/passenger owners are extended here.
    pub fn construct_car_ex<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: CarExSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x227b).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        if self.car_services.is_none() {
            return Err(ActorCreationError {
                actor: None,
                source: ActorWorldError::Effect {
                    stage: "car_resources",
                    detail: "Game car resource context unavailable".into(),
                },
            });
        }
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed base actor");
        self.actors[i].phase = ActorPhase::Constructing;
        let result = (|| {
            let state = CarActorState::allocation(actor, seed.car).map_err(|detail| {
                ActorWorldError::Effect {
                    stage: "car_allocation",
                    detail,
                }
            })?;
            self.actors[i].car = Some(state);
            let owner = &mut self.actors[i];
            let base = &mut owner.state;
            owner
                .car
                .as_mut()
                .unwrap()
                .construct(self.car_services.as_mut().unwrap(), || {
                    base.vtable = ActorVtable::CarEx;
                    base.word_10 = 36;
                    Ok(())
                })
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "car_constructor",
                    detail,
                })?;
            self.actors[i].car_ex = Some(CarExState::allocation(seed));
            Ok(())
        })();
        result.map_err(|source| ActorCreationError {
            actor: Some(actor),
            source,
        })?;
        self.actors[i].phase = ActorPhase::Ready;
        Ok(actor)
    }
    pub fn car_ex(&self, actor: ActorReferenceId) -> Result<&CarExState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .car_ex
            .as_ref()
            .ok_or(ActorWorldError::NotCarEx)
    }
    pub fn inspect_car_ex(&self, actor: ActorReferenceId) -> Option<&CarExState> {
        self.actors
            .iter()
            .find(|owner| owner.lifetime.reference() == actor)?
            .car_ex
            .as_ref()
    }
    /// Original C_car_ex::CanUseTruck (mafia-re game/vehicles/C_car_ex.h:34-38,
    /// GAME 0x47b6d0) over the retained tail words.
    pub fn can_use_truck(&self, actor: ActorReferenceId) -> Result<bool, ActorWorldError> {
        Ok(self.car_ex(actor)?.can_use_truck())
    }
    /// Live truck passenger owners placed by DCAddPerson.
    pub fn car_ex_passengers(
        &self,
        actor: ActorReferenceId,
    ) -> Result<&[crate::script_scheduler::ActorReferenceId], ActorWorldError> {
        Ok(self.car_ex(actor)?.passengers())
    }
    /// Original C_car_ex::DCAddPerson effect owner (STUB GAME 0x42f160,
    /// mafia-re game/vehicles/C_car_ex.cpp:63-67): retain the passenger owner.
    /// Only owner liveness is validated; the C_human shape gate stays open.
    pub fn dc_add_person(
        &mut self,
        actor: ActorReferenceId,
        human: crate::script_scheduler::ActorReferenceId,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].car_ex.is_none() || self.actors[i].state.vtable != ActorVtable::CarEx {
            return Err(ActorWorldError::NotCarEx);
        }
        self.index(human)?;
        Ok(self.actors[i]
            .car_ex
            .as_mut()
            .expect("admitted car_ex")
            .add_person(human))
    }
    /// Original C_car_ex::DCDelPerson effect owner (STUB GAME 0x42f320,
    /// mafia-re game/vehicles/C_car_ex.cpp:69-73): release the passenger owner.
    pub fn dc_del_person(
        &mut self,
        actor: ActorReferenceId,
        human: crate::script_scheduler::ActorReferenceId,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].car_ex.is_none() || self.actors[i].state.vtable != ActorVtable::CarEx {
            return Err(ActorWorldError::NotCarEx);
        }
        Ok(self.actors[i]
            .car_ex
            .as_mut()
            .expect("admitted car_ex")
            .remove_person(human))
    }
    /// Genuine C_racecam construction: the caller's base owner plus the kind
    /// marker for kind 28 (0x88 bytes, vtable 0x625950). The stub constructor
    /// (mafia-re game/camera/c_racecam.cpp:8-11) stores no other derived words.
    pub fn construct_racecam<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: RacecamSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x87).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed racecam owner");
        let mut state = RacecamState::allocation(seed);
        state.complete_constructor();
        self.actors[i].racecam = Some(state);
        self.actors[i].state.vtable = ActorVtable::Racecam;
        self.actors[i].state.word_10 = 28;
        Ok(actor)
    }
    pub fn racecam(&self, actor: ActorReferenceId) -> Result<&RacecamState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .racecam
            .as_ref()
            .ok_or(ActorWorldError::NotRacecam)
    }
    pub fn inspect_racecam(&self, actor: ActorReferenceId) -> Option<&RacecamState> {
        self.actors
            .iter()
            .find(|a| a.lifetime.reference() == actor)?
            .racecam
            .as_ref()
    }
    /// Original C_racecam::LoadData (STUB 0x5962e0, mafia-re game/camera/
    /// c_racecam.cpp:46-50) delegates to C_actor::LoadData: zero payload bytes
    /// are consumed and native success is reported. The enclosing chunk reader
    /// skips the record by its declared extent.
    pub fn load_racecam_data(&mut self, actor: ActorReferenceId) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].racecam.is_none() || self.actors[i].state.vtable != ActorVtable::Racecam {
            return Err(ActorWorldError::NotRacecam);
        }
        self.actors[i].phase = ActorPhase::Ready;
        Ok(0)
    }
    pub fn release_racecam<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].racecam.is_none() || self.actors[i].state.vtable != ActorVtable::Racecam {
            return Err(ActorWorldError::NotRacecam);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_racecam(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    pub fn destroy_racecam<F: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut F,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].racecam.is_none() {
            return Err(ActorWorldError::NotRacecam);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Racecam;
        self.destroy_base_inner(actor, backend)
    }
    pub fn delete_racecam<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_racecam(actor, frames)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_racecam",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Native LoadData returns AL1 even when bounded read guards skip fields.
    /// Returns consumed payload bytes for host inspection, not a native status.
    pub fn load_railway_data(
        &mut self,
        actor: ActorReferenceId,
        payload: &[u8],
    ) -> Result<usize, ActorWorldError> {
        let i = self.ready(actor)?;
        let railway = self.actors[i]
            .railway
            .as_mut()
            .ok_or(ActorWorldError::NotRailway)?;
        Ok(railway.load_payload(payload))
    }
    /// Actual Human primary-base construction. Factory kind3 remains absent;
    /// this owner is composed by the genuine Player/Entity constructors.
    pub fn construct_human<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: HumanSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        let actor = self.construct_base(identity, base, backend)?;
        let i = self.index(actor).expect("constructed human base");
        self.actors[i].human = Some(HumanState::construct(seed));
        self.actors[i].state.vtable = ActorVtable::Human;
        self.actors[i].state.word_10 = 3;
        self.actors[i].state.word_14 = 2;
        Ok(actor)
    }
    pub fn construct_player<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        seed: PlayerSeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0xaff).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let actor = self.construct_human(identity, base, seed.human, backend)?;
        let i = self.index(actor).expect("constructed player base");
        let player =
            PlayerState::construct(seed.allocator_tag, self.actors[i].human.as_mut().unwrap());
        self.actors[i].player = Some(player);
        self.actors[i].state.vtable = ActorVtable::Player;
        self.actors[i].state.word_10 = 2;
        Ok(actor)
    }
    pub fn construct_entity<B: ActorCollectionBackend + ?Sized>(
        &mut self,
        identity: u32,
        base: BaseActorSeed,
        mut seed: EntitySeed,
        backend: &mut B,
    ) -> Result<ActorReferenceId, ActorCreationError> {
        identity.checked_add(0x128b).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        let program_identity = identity.checked_add(0xa9c).ok_or(ActorCreationError {
            actor: None,
            source: ActorWorldError::InvalidBacking,
        })?;
        seed.program.address_high_byte = (program_identity >> 24) as u8;
        let actor = self.construct_human(identity, base, seed.human.clone(), backend)?;
        let i = self.index(actor).expect("constructed entity base");
        self.actors[i].entity = Some(EntityState::construct(seed));
        self.actors[i].state.vtable = ActorVtable::Entity;
        self.actors[i].state.word_10 = 27;
        Ok(actor)
    }
    pub fn human(&self, actor: ActorReferenceId) -> Result<&HumanState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .human
            .as_ref()
            .ok_or(ActorWorldError::NotHuman)
    }
    pub fn player(&self, actor: ActorReferenceId) -> Result<&PlayerState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .player
            .as_ref()
            .ok_or(ActorWorldError::NotPlayer)
    }
    pub fn entity(&self, actor: ActorReferenceId) -> Result<&EntityState, ActorWorldError> {
        self.actors[self.index(actor)?]
            .entity
            .as_ref()
            .ok_or(ActorWorldError::NotEntity)
    }
    /// Live Process scratch of an entity actor for program dispatch. The
    /// actor stays the authoritative owner; dispatch round-trips every value
    /// back through [`Self::store_entity_process_state`].
    pub fn entity_process_state(
        &self,
        actor: ActorReferenceId,
    ) -> Result<EntityProcessState, ActorWorldError> {
        Ok(self.entity(actor)?.process_state())
    }
    pub fn store_entity_process_state(
        &mut self,
        actor: ActorReferenceId,
        state: &EntityProcessState,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i]
            .entity
            .as_mut()
            .ok_or(ActorWorldError::NotEntity)?
            .store_process_state(state);
        Ok(())
    }
    /// Take an entity actor's embedded program for one tick. The actor keeps
    /// its identity and phase; only the program moves. The caller must return
    /// it through [`Self::return_entity_program`] on every path, including
    /// failed ticks: dropping a live program is host cleanup, never a
    /// sanctioned native free. Callbacks during the tick must not retire the
    /// ticking actor; no such callback exists yet.
    pub fn take_entity_program(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<Option<CProgram>, ActorWorldError> {
        let i = self.ready(actor)?;
        Ok(self.actors[i]
            .entity
            .as_mut()
            .ok_or(ActorWorldError::NotEntity)?
            .program
            .take())
    }
    pub fn return_entity_program(
        &mut self,
        actor: ActorReferenceId,
        program: Option<CProgram>,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i]
            .entity
            .as_mut()
            .ok_or(ActorWorldError::NotEntity)?
            .program = program;
        Ok(())
    }
    /// Take a detector actor's embedded program for one init or tick pass. The
    /// actor keeps its identity and phase; only the program moves. The caller
    /// must return it through [`Self::return_detector_program`] on every path,
    /// including failed passes: dropping a live program is host cleanup, never
    /// a sanctioned native free. This mirrors [`Self::take_entity_program`]
    /// for the C_detector::GameInit (mafia-re 113b5b6
    /// game/actors/C_detector.cpp:44-58, GAME 0x432a00) C_program::Init prefix
    /// (GAME 0x461580) and for the C_detector::Process tick dispatch (GAME
    /// 0x433d10, C_detector.cpp:189-271); the GameInit body beyond that
    /// prefix, GameDone and the CreateColls/EraseColls/CreateUO collision
    /// owners stay DEFERRED.
    pub fn take_detector_program(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<Option<CProgram>, ActorWorldError> {
        let i = self.ready(actor)?;
        Ok(self.actors[i]
            .detector
            .as_mut()
            .ok_or(ActorWorldError::NotDetector)?
            .program
            .take())
    }
    pub fn return_detector_program(
        &mut self,
        actor: ActorReferenceId,
        program: Option<CProgram>,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i]
            .detector
            .as_mut()
            .ok_or(ActorWorldError::NotDetector)?
            .program = program;
        Ok(())
    }
    /// Owned slice of C_detector::GameInit (mafia-re game/actors/
    /// C_detector.cpp:44-62, GAME 0x432a00): null the signal and the
    /// wait_for_hit/dyn_colls latches. The C_actor::GameInit call, the
    /// actor=self assignment, C_program::Init, unknown_65 handling,
    /// DebustizeFrame and the coll-pointer nulling stay with future owners.
    pub fn game_init_detector(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        self.actors[i]
            .detector
            .as_mut()
            .ok_or(ActorWorldError::NotDetector)?
            .apply_game_init();
        Ok(())
    }
    /// Owned slice of C_detector::Init (mafia-re game/actors/C_detector.cpp:24-30,
    /// GAME 0x432990): zero the human_types mask (+0xd8). The C_actor::Init
    /// gate and the actor=self assignment stay with the base owner.
    pub fn init_detector(
        &mut self,
        actor: ActorReferenceId,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        let detector = self.actors[i]
            .detector
            .as_mut()
            .ok_or(ActorWorldError::NotDetector)?;
        detector.bytes_d8_ef[..4].copy_from_slice(&[0; 4]);
        Ok(())
    }
    /// Original C_actor::SetActState: store the ACTSTATE_* word. The +0x1c
    /// identity follows C_actor.h (act_state at 0x1c, 0 from Init, matching
    /// the zeroed real seeds); only this writer treats the word as act state.
    /// The game-side change notification has no Game owner yet and is not
    /// recorded here.
    pub fn set_actor_act_state(
        &mut self,
        actor: ActorReferenceId,
        state: u32,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i].state.state_1c = state;
        Ok(())
    }
    /// Original C_program CMD_HUMAN_ADDWEAPON tail: resolve an ENTITY/PLAYER
    /// human and push the weapon record. Slot-placement and capacity rules,
    /// ChangeWeaponModel and WeaponChoose stay open; no shipped MISE01 line
    /// reads these records back.
    pub fn human_add_weapon(
        &mut self,
        actor: ActorReferenceId,
        id: u16,
        loaded: i32,
        spare: i32,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Entity | ActorVtable::Player
        ) {
            return Err(ActorWorldError::NotHuman);
        }
        self.actors[i]
            .human
            .as_mut()
            .ok_or(ActorWorldError::NotHuman)?
            .inventory
            .push_weapon(HumanInventoryItem::from_weapon_parts(id, loaded, spare));
        Ok(())
    }
    /// Original C_actor::GetDead read (mafia-re game/actors/C_actor.h:359 at
    /// 0x5e, GetDead GAME 0x47afc0) for GETACTORSDIST. Stale actors propagate.
    pub fn actor_dead(&self, actor: ActorReferenceId) -> Result<bool, ActorWorldError> {
        Ok(self.actors[self.index(actor)?].state.get_dead())
    }
    /// Test/host setter for the dead latch (Tick producer deferred).
    pub fn set_actor_dead(
        &mut self,
        actor: ActorReferenceId,
        dead: bool,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i].state.set_dead(dead);
        Ok(())
    }
    /// Original C_human::GetCar read (mafia-re game/actors/C_human.h:1120 GAME
    /// 0x47b7b0) for HUMAN_GETOWNER. Returns None for a resolved non-human
    /// (native type gate, caller advances without dest write); Some carries
    /// the live car latch (None for no car, caller writes null). Stale actors
    /// propagate. The three-conjunct nulling gate (GetCar()->GetType() ==
    /// ACTOR_CAR plus action_state != HUMAN_ACTION_CAR && seat > -1 per
    /// C_program.cpp:4325, HUMAN_ACTION_CAR 9 per C_human.h:145) needs
    /// enter-car AI and stays DEFERRED with the seat/action writers.
    pub fn human_car(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<Option<ActorReferenceId>>, ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Entity | ActorVtable::Player
        ) {
            return Ok(None);
        }
        let human = self.actors[i].human.as_ref().ok_or(ActorWorldError::NotHuman)?;
        Ok(Some(human.car()))
    }
    /// Test/host setter for the human car latch (enter-car AI deferred).
    pub fn set_human_car(
        &mut self,
        actor: ActorReferenceId,
        car: Option<ActorReferenceId>,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Entity | ActorVtable::Player
        ) {
            return Err(ActorWorldError::NotHuman);
        }
        self.actors[i]
            .human
            .as_mut()
            .ok_or(ActorWorldError::NotHuman)?
            .set_car(car);
        Ok(())
    }
    /// Original C_Vehicle::GetSpeed read (mafia-re game/vehicles/C_Vehicle.h:266
    /// GAME 0x47b0f0, speed at 0x59c m/s) for CAR_GETSPEED. Returns None for a
    /// resolved non-car (native type gate, caller leaves the float bank
    /// untouched); Some carries m/s. Stale actors propagate. The dynamics
    /// producer (SetVelocity) stays open; the latch starts at 0.0.
    pub fn car_speed(&self, actor: ActorReferenceId) -> Result<Option<f32>, ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Car | ActorVtable::CarEx
        ) {
            return Ok(None);
        }
        let car = self.actors[i].car.as_ref().ok_or(ActorWorldError::UnsupportedVtable)?;
        Ok(Some(car.script_speed_mps()))
    }
    /// Test/host setter for the car speed latch.
    pub fn set_car_speed(
        &mut self,
        actor: ActorReferenceId,
        speed_mps: f32,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Car | ActorVtable::CarEx
        ) {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        self.actors[i]
            .car
            .as_mut()
            .ok_or(ActorWorldError::UnsupportedVtable)?
            .set_script_speed_mps(speed_mps);
        Ok(())
    }
    /// Original C_car::IsCarUsable read (mafia-re game/vehicles/C_car.h:160,
    /// GAME 0x429850 STUB returning false; C_program.cpp:4954 stores it to a
    /// float). Returns None for a resolved non-car (native type gate);
    /// Some carries the predicate (always false until the stub is replaced).
    /// Partial: the full usable predicate stays DEFERRED. Stale propagates.
    pub fn is_car_usable(&self, actor: ActorReferenceId) -> Result<Option<bool>, ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Car | ActorVtable::CarEx
        ) {
            return Ok(None);
        }
        if self.actors[i].car.is_none() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        Ok(Some(false))
    }
    /// Original C_car::SetBreakMotor store (mafia-re game/vehicles/C_car.h:193
    /// GAME 0x47b3e0, break_motor at 0x210d; C_program.cpp:5002). Returns
    /// false for a resolved non-car (native type gate, no store); true after
    /// storing. Stale actors propagate.
    pub fn set_car_break_motor(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<bool, ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Car | ActorVtable::CarEx
        ) {
            return Ok(false);
        }
        self.actors[i]
            .car
            .as_mut()
            .ok_or(ActorWorldError::UnsupportedVtable)?
            .set_break_motor(on);
        Ok(true)
    }
    /// Original CAR_FORCESTOP tail (mafia-re game/mission/C_program.cpp:5803:
    /// `Deactivate(0)` + `Reset(0.0f, true)`). Returns false for a resolved
    /// non-car (native type gate, no store); true after latching. The
    /// physics/AI effects of those calls stay DEFERRED. Stale propagates.
    pub fn force_stop_car(&mut self, actor: ActorReferenceId) -> Result<bool, ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].state.vtable,
            ActorVtable::Car | ActorVtable::CarEx
        ) {
            return Ok(false);
        }
        self.actors[i]
            .car
            .as_mut()
            .ok_or(ActorWorldError::UnsupportedVtable)?
            .apply_force_stop();
        Ok(true)
    }
    /// Original C_player::LockControls store (mafia-re game/actors/C_player.h:60
    /// GAME 0x595f00 STUB; C_program.cpp:4172) for PLAYER_LOCKCONTROLS.
    /// Locks the game player (players[0]); no player advances without effect.
    /// Do_Breath stays DEFERRED. Stale player propagates as a host defect.
    pub fn set_player_controls_locked(&mut self, locked: bool) -> Result<(), ActorWorldError> {
        let Some(player) = self.players[0] else {
            return Ok(());
        };
        let i = self.index(player)?;
        self.actors[i]
            .player
            .as_mut()
            .ok_or(ActorWorldError::NotPlayer)?
            .set_controls_locked(locked);
        Ok(())
    }
    /// Original C_program::SetEvent for a target actor's embedded program.
    /// Returns true if the caller should advance (no event or event started),
    /// false if the caller should jump to its label (already inside an event).
    /// Only entity and detector programs are considered; other actors return
    /// true (no jump), matching the native type gate. Stale actors propagate.
    pub fn program_set_event(
        &mut self,
        actor: ActorReferenceId,
        event_id: i32,
    ) -> Result<bool, ActorWorldError> {
        let i = self.index(actor)?;
        let is_entity = self.actors[i].entity.is_some();
        let is_detector = self.actors[i].detector.is_some();
        if !is_entity && !is_detector {
            return Ok(true);
        }
        let outcome = {
            let entry = &mut self.actors[i];
            let program = if let Some(entity) = &mut entry.entity {
                match &mut entity.program {
                    Some(p) => p,
                    None => return Ok(true),
                }
            } else if let Some(detector) = &mut entry.detector {
                match &mut detector.program {
                    Some(p) => p,
                    None => return Ok(true),
                }
            } else {
                return Ok(true);
            };
            program.try_set_event(event_id).map_err(|detail| {
                ActorWorldError::Program(crate::original_program::ProgramError::Effect {
                    stage: "set_event",
                    detail,
                })
            })?
        };
        match outcome {
            crate::original_program::SetEventOutcome::NoEvent => Ok(true),
            crate::original_program::SetEventOutcome::Started => {
                let j = self.index(actor)?;
                self.actors[j].state.state_1c = 0;
                Ok(true)
            }
            crate::original_program::SetEventOutcome::Busy => Ok(false),
        }
    }
    pub fn inspect_human(&self, actor: ActorReferenceId) -> Option<&HumanState> {
        self.actors
            .iter()
            .find(|owned| owned.lifetime.reference() == actor)?
            .human
            .as_ref()
    }
    pub fn inspect_entity(&self, actor: ActorReferenceId) -> Option<&EntityState> {
        self.actors
            .iter()
            .find(|owned| owned.lifetime.reference() == actor)?
            .entity
            .as_ref()
    }
    fn init_human_owner<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frame: Option<FrameReferenceId>,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        self.actors[i].phase = ActorPhase::Initializing;
        let result = (|| {
            // Entity stores the secondary C_program owner before Human's
            // null/type/base guards, including every rejected Init prefix.
            if let Some(entity) = self.actors[i].entity.as_mut() {
                entity
                    .program
                    .as_mut()
                    .expect("constructed entity program")
                    .set_owner(Some(actor))
                    .map_err(ActorWorldError::Program)?;
            }
            let Some(frame) = frame else {
                return Ok(false);
            };
            if !backend
                .frame_is_model(self, frame)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "human_frame_kind",
                    detail,
                })?
            {
                return Ok(false);
            }
            if !self.init_inner(actor, frame, backend)? {
                return Ok(false);
            }
            let i = self.index(actor)?;
            self.actors[i].human.as_mut().unwrap().human_type_mask = 0;
            for (name, target_e) in [
                (b"targetE".as_slice(), true),
                (b"targetN".as_slice(), false),
            ] {
                let frame = self
                    .state(actor)?
                    .frame
                    .ok_or(ActorWorldError::MissingAttachedFrame)?;
                let found = backend
                    .find_child(self, frame, name, 0x40)
                    .map_err(|detail| ActorWorldError::Effect {
                        stage: "human_find_target",
                        detail,
                    })?;
                let i = self.index(actor)?;
                let human = self.actors[i].human.as_mut().unwrap();
                if target_e {
                    human.target_e = found;
                } else {
                    human.target_n = found;
                }
            }
            if let Some(child) = self.human(actor)?.target_e {
                backend
                    .set_child_enabled(self, child, 0)
                    .map_err(|detail| ActorWorldError::Effect {
                        stage: "human_disable_target_e",
                        detail,
                    })?;
            }
            if let Some(child) = self.human(actor)?.target_n {
                backend
                    .set_child_enabled(self, child, 0)
                    .map_err(|detail| ActorWorldError::Effect {
                        stage: "human_disable_target_n",
                        detail,
                    })?;
            }
            let frame = self
                .state(actor)?
                .frame
                .ok_or(ActorWorldError::MissingAttachedFrame)?;
            let base = backend
                .find_child(self, frame, b"base", 0xffff)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "human_find_base",
                    detail,
                })?;
            let i = self.index(actor)?;
            self.actors[i].human.as_mut().unwrap().base_frame = base;
            if self.actors[i].player.is_some() {
                self.actors[i].human.as_mut().unwrap().human_type_mask = 1;
            }
            Ok(true)
        })();
        let i = self.index(actor)?;
        self.actors[i].phase = if result.is_err() {
            ActorPhase::InitializationInterrupted
        } else {
            ActorPhase::Ready
        };
        result
    }
    pub fn load_human_data(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].human.is_none() {
            return Err(ActorWorldError::NotHuman);
        }
        self.actors[i].phase = ActorPhase::Loading;
        let result = self.actors[i]
            .human
            .as_mut()
            .unwrap()
            .load_source_data(reader)
            .map_err(ActorWorldError::HumanData);
        self.actors[i].phase = if result.is_err() {
            ActorPhase::LoadingInterrupted
        } else {
            ActorPhase::Ready
        };
        result
    }
    pub fn load_entity_data<P: ProgramBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut ActorDataReader<'_>,
        replacements: &[ProgramReplacement],
        mission_path: Option<&[u8]>,
        frames: &dyn FrameLookupBackend,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].entity.is_none() {
            return Err(ActorWorldError::NotEntity);
        }
        self.actors[i].phase = ActorPhase::Loading;
        let result = self.actors[i]
            .human
            .as_mut()
            .unwrap()
            .load_source_data(reader)
            .map_err(ActorWorldError::HumanData);
        if let Err(error) = result {
            self.actors[i].phase = ActorPhase::LoadingInterrupted;
            return Err(error);
        }
        // The derived version byte is ignored, including an unavailable bounded
        // read. CProgram sees the exact same cursor after this native operation.
        reader.read_bounded(&mut [0]);
        let mut program = self.actors[i]
            .entity
            .as_mut()
            .unwrap()
            .program
            .take()
            .expect("constructed entity program");
        let mut context = ActorProgramContext {
            world: self,
            frames,
            mission_path,
        };
        let result = program.load_source_code(reader, replacements, &mut context, backend);
        self.actors[i].entity.as_mut().unwrap().program = Some(program);
        self.actors[i].phase = if result.is_err() {
            ActorPhase::LoadingInterrupted
        } else {
            ActorPhase::Ready
        };
        result.map_err(ActorWorldError::Program)
    }
    fn begin_human_destruction(
        &mut self,
        actor: ActorReferenceId,
        vtable: ActorVtable,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::InitializationInterrupted
                | ActorPhase::LoadingInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].human.is_none() {
            return Err(ActorWorldError::NotHuman);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = vtable;
        Ok(())
    }
    fn human_destruction_result(
        &mut self,
        actor: ActorReferenceId,
        result: Result<(), ActorWorldError>,
    ) -> Result<(), ActorWorldError> {
        if result.is_err() {
            let i = self.index(actor)?;
            self.actors[i].phase = ActorPhase::DestructionInterrupted;
        }
        result
    }
    fn destroy_human_inner<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.state_mut_inner(actor)?.vtable = ActorVtable::Human;
        // Actual dtor captures this frame once for its inline type check and
        // StopAnimation call. Later fields/base destruction are read live.
        if let Some(frame) = self.state(actor)?.frame {
            if frames
                .frame_is_model(self, frame)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "human_destructor_frame_kind",
                    detail,
                })?
            {
                frames
                    .stop_model_animation(self, frame, 0)
                    .map_err(|detail| ActorWorldError::Effect {
                        stage: "human_stop_animation",
                        detail,
                    })?;
            }
        }
        for slot in [
            HumanResourceSlot::LastAuxiliary,
            HumanResourceSlot::SecondAuxiliary,
            HumanResourceSlot::FirstAuxiliary,
            HumanResourceSlot::Inventory,
        ] {
            backend
                .free_human_collection(self, actor, slot)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_human_collection",
                    detail,
                })?;
            let i = self.index(actor)?;
            self.actors[i].human.as_mut().unwrap().clear_resource(slot);
        }
        let i = self.index(actor)?;
        self.actors[i]
            .human
            .as_mut()
            .unwrap()
            .animation_machine
            .destruct();
        backend
            .free_human_collection(self, actor, HumanResourceSlot::CustomAnimations)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "free_human_collection",
                detail,
            })?;
        let i = self.index(actor)?;
        self.actors[i]
            .human
            .as_mut()
            .unwrap()
            .clear_resource(HumanResourceSlot::CustomAnimations);
        self.destroy_base_inner(actor, frames)
    }
    pub fn destroy_human<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if self.actors[i].player.is_some() || self.actors[i].entity.is_some() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        self.begin_human_destruction(actor, ActorVtable::Human)?;
        let result = self.destroy_human_inner(actor, frames, backend);
        self.human_destruction_result(actor, result)
    }
    pub fn destroy_player<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.player(actor)?;
        self.begin_human_destruction(actor, ActorVtable::Player)?;
        let result = (|| {
            backend
                .free_human_collection(self, actor, HumanResourceSlot::Player)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_player_collection",
                    detail,
                })?;
            let i = self.index(actor)?;
            self.actors[i].player.as_mut().unwrap().collection.clear();
            self.destroy_human_inner(actor, frames, backend)
        })();
        self.human_destruction_result(actor, result)
    }
    pub fn destroy_entity<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ProgramBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.entity(actor)?;
        self.begin_human_destruction(actor, ActorVtable::Entity)?;
        let i = self.index(actor)?;
        self.actors[i]
            .entity
            .as_mut()
            .unwrap()
            .program
            .as_mut()
            .expect("owned entity program")
            .set_entity_vtable();
        let result = (|| {
            for slot in [
                EntityResourceSlot::Last,
                EntityResourceSlot::Auxiliary,
                EntityResourceSlot::ThirdUnsigned,
                EntityResourceSlot::SecondUnsigned,
                EntityResourceSlot::FirstUnsigned,
                EntityResourceSlot::SubstateLocks,
                EntityResourceSlot::CharacterLogs,
            ] {
                backend
                    .free_entity_collection(self, actor, slot)
                    .map_err(|detail| ActorWorldError::Effect {
                        stage: "free_entity_collection",
                        detail,
                    })?;
                let i = self.index(actor)?;
                self.actors[i].entity.as_mut().unwrap().clear_resource(slot);
            }
            let i = self.index(actor)?;
            let mut program = self.actors[i]
                .entity
                .as_mut()
                .unwrap()
                .program
                .take()
                .expect("owned entity program");
            let result = program.destruct(backend);
            self.actors[i].entity.as_mut().unwrap().program = Some(program);
            result.map_err(ActorWorldError::Program)?;
            self.destroy_human_inner(actor, frames, backend)
        })();
        self.human_destruction_result(actor, result)
    }
    pub fn delete_human<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_human(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_human",
                    detail,
                })?;
        }
        Ok(())
    }
    pub fn delete_player<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_player(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_player",
                    detail,
                })?;
        }
        Ok(())
    }
    pub fn delete_entity<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ProgramBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_entity(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_entity",
                    detail,
                })?;
        }
        Ok(())
    }
    pub fn release_player<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].state.vtable != ActorVtable::Player {
            return Err(ActorWorldError::NotPlayer);
        }
        let references = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = references;
        if references == 0 {
            self.delete_player(actor, 1, frames, backend)?;
        }
        Ok(references)
    }
    pub fn release_entity<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ProgramBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].state.vtable != ActorVtable::Entity {
            return Err(ActorWorldError::NotEntity);
        }
        let references = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = references;
        if references == 0 {
            self.delete_entity(actor, 1, frames, backend)?;
        }
        Ok(references)
    }
    /// Supported factory branches allocate their actual derived extents.
    /// Other derived kinds remain explicit dependencies.
    pub fn create_actor<B: ActorAllocationBackend + ActorCollectionBackend + ?Sized>(
        &mut self,
        kind: u32,
        backend: &mut B,
    ) -> Result<Option<ActorReferenceId>, ActorCreationError> {
        // The original switch names this sparse set. Its default returns NULL,
        // including holes inside 1..=38; only named unavailable classes fail.
        if !matches!(kind, 1 | 2 | 4 | 5 | 6 | 8 | 9 | 10 | 12 | 18 | 20..=25 | 27..=36 | 38) {
            return Ok(None);
        }
        if kind == 2 {
            let allocation =
                backend
                    .allocate_player(0xb00)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_player",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_player(identity, base, seed, backend))
                .transpose();
        }
        if kind == 18 {
            let allocation =
                backend
                    .allocate_traffic_generator(0xb8)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_traffic_generator",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| {
                    self.construct_traffic_generator(identity, base, seed, backend)
                })
                .transpose();
        }
        if kind == 4 {
            let allocation = backend
                .allocate_car(0x221c)
                .map_err(|detail| ActorCreationError {
                    actor: None,
                    source: ActorWorldError::Effect {
                        stage: "allocate_car",
                        detail,
                    },
                })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_car(identity, base, seed, backend))
                .transpose();
        }
        if kind == 27 {
            let allocation =
                backend
                    .allocate_entity(0x128c)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_entity",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_entity(identity, base, seed, backend))
                .transpose();
        }
        if kind == 8 {
            let allocation =
                backend
                    .allocate_railway(0x370)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_railway",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_railway(identity, base, seed, backend))
                .transpose();
        }
        if kind == 5 {
            let allocation =
                backend
                    .allocate_detector(0x128)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_detector",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| {
                    self.construct_detector(identity, base, seed, backend)
                })
                .transpose();
        }
        if kind == 20 {
            let allocation =
                backend
                    .allocate_bridge(0x194)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_bridge",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_bridge(identity, base, seed, backend))
                .transpose();
        }
        if kind == 6 {
            let allocation = backend
                .allocate_door(0x1b8)
                .map_err(|detail| ActorCreationError {
                    actor: None,
                    source: ActorWorldError::Effect {
                        stage: "allocate_door",
                        detail,
                    },
                })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_door(identity, base, seed, backend))
                .transpose();
        }
        if kind == 12 {
            let allocation =
                backend
                    .allocate_traffic_car(0x26c)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_traffic_car",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| {
                    self.construct_traffic_car(identity, base, seed, backend)
                })
                .transpose();
        }
        if kind == 25 {
            let allocation =
                backend
                    .allocate_pumper(0x2ac)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_pumper",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_pumper(identity, base, seed, backend))
                .transpose();
        }
        if kind == 35 {
            let allocation =
                backend
                    .allocate_box_new(0x3a8)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_box_new",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_box_new(identity, base, seed, backend))
                .transpose();
        }
        if kind == 28 {
            let allocation =
                backend
                    .allocate_racecam(0x88)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_racecam",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_racecam(identity, base, seed, backend))
                .transpose();
        }
        if kind == 33 {
            let allocation =
                backend
                    .allocate_actor_shot(0x8c)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_actor_shot",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| {
                    self.construct_actor_shot(identity, base, seed, backend)
                })
                .transpose();
        }
        if kind == 34 {
            let allocation = backend
                .allocate_clock(0x88)
                .map_err(|detail| ActorCreationError {
                    actor: None,
                    source: ActorWorldError::Effect {
                        stage: "allocate_clock",
                        detail,
                    },
                })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_clock(identity, base, seed, backend))
                .transpose();
        }
        if kind == 36 {
            let allocation =
                backend
                    .allocate_car_ex(0x227c)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_car_ex",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| self.construct_car_ex(identity, base, seed, backend))
                .transpose();
        }
        if kind == 38 {
            let allocation =
                backend
                    .allocate_rail_generator(0x84)
                    .map_err(|detail| ActorCreationError {
                        actor: None,
                        source: ActorWorldError::Effect {
                            stage: "allocate_rail_generator",
                            detail,
                        },
                    })?;
            return allocation
                .map(|(identity, base, seed)| {
                    self.construct_rail_generator(identity, base, seed, backend)
                })
                .transpose();
        }
        if kind != 1 {
            return Err(ActorCreationError {
                actor: None,
                source: ActorWorldError::UnsupportedKind(kind),
            });
        }
        let allocation = backend
            .allocate_actor(0x70)
            .map_err(|detail| ActorCreationError {
                actor: None,
                source: ActorWorldError::Effect {
                    stage: "allocate_actor",
                    detail,
                },
            })?;
        let Some((identity, seed)) = allocation else {
            return Ok(None);
        };
        self.construct_base(identity, seed, backend).map(Some)
    }
    pub fn init<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frame: Option<FrameReferenceId>,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].car.is_some() {
            self.actors[i].phase = ActorPhase::Initializing;
            let result = backend
                .init_car_owner(self, actor, frame)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "car_init",
                    detail,
                });
            self.actors[i].phase = if result.is_err() {
                ActorPhase::InitializationInterrupted
            } else {
                ActorPhase::Ready
            };
            return result;
        }
        if self.actors[i].human.is_some() {
            return self.init_human_owner(actor, frame, backend);
        }
        if self.actors[i].rail_generator.is_some() {
            // Native Init stores the frame and returns true
            // (C_rail_generator.cpp:26-30, no null check, no base call).
            // The frame has no host slot and no reader (Update is folded
            // empty), so the store is unobservable: initialization only.
            return Ok(true);
        }
        let Some(frame) = frame else {
            return Ok(false);
        };
        if self.actors[i].railway.is_some()
            && !backend
                .frame_is_model(self, frame)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "railway_frame_kind",
                    detail,
                })?
        {
            return Ok(false);
        }
        self.actors[i].phase = if self.actors[i].traffic.is_some() {
            // Latch before base Init callbacks, including caught unwind. The
            // derived Init4393a0 has no effects beyond this actual base call.
            ActorPhase::InitializationInterrupted
        } else {
            ActorPhase::Initializing
        };
        let mut result = self.init_inner(actor, frame, backend);
        let i = self.index(actor)?;
        if matches!(result, Ok(true)) {
            if let Some(detector) = &mut self.actors[i].detector {
                detector.bytes_d8_ef[..4].fill(0);
                if let Err(error) = detector
                    .program
                    .as_mut()
                    .expect("ready detector program")
                    .set_owner(Some(actor))
                {
                    result = Err(ActorWorldError::Program(error));
                }
            }
        }
        self.actors[i].phase = if result.is_err() {
            ActorPhase::InitializationInterrupted
        } else {
            ActorPhase::Ready
        };
        result
    }
    fn init_inner<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frame: FrameReferenceId,
        backend: &mut B,
    ) -> Result<bool, ActorWorldError> {
        let data = backend
            .actor_data(self, frame)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "frame_actor_data",
                detail,
            })?;
        if matches!(data, FrameActorData::Present(Some(owner)) if owner != actor) {
            return Ok(false);
        }
        self.state_mut_inner(actor)?.frame = Some(frame);
        backend
            .clear_actor_data(self, frame)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "clear_frame_actor_data",
                detail,
            })?;
        self.attachment_counter = self.attachment_counter.wrapping_add(1);
        let counter = self.attachment_counter;
        let state = self.state_mut_inner(actor)?;
        state.actor_id = counter;
        state.release_frame = 0;
        // The first null check guards the entire pair. The second native call
        // rereads actor+0x68 without a second null guard after callback return.
        if let Some(frame) = self.state(actor)?.frame {
            self.init_target_child(frame, b"targetE", backend)?;
            let frame = self
                .state(actor)?
                .frame
                .ok_or(ActorWorldError::MissingAttachedFrame)?;
            self.init_target_child(frame, b"targetN", backend)?;
        }
        self.state_mut_inner(actor)?.state_1c = 0;
        Ok(true)
    }
    fn init_target_child<B: ActorFrameBackend + ?Sized>(
        &mut self,
        frame: FrameReferenceId,
        name: &[u8],
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let child = backend
            .find_child(self, frame, name, 0x40)
            .map_err(|detail| ActorWorldError::Effect {
                stage: "find_target_child",
                detail,
            })?;
        if let Some(child) = child {
            backend
                .set_child_enabled(self, child, 0)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "disable_target_child",
                    detail,
                })?;
        }
        Ok(())
    }
    fn remove_from_manager(&mut self, actor: ActorReferenceId) -> Result<bool, ActorWorldError> {
        self.manager.remove_association(actor, false)?;
        if let Some(i) = self.manager.first.iter().position(|a| *a == actor) {
            self.state_mut_inner(actor)?.flags &= !0x10;
            self.manager.first.remove(i);
            return Ok(true);
        }
        if let Some(i) = self.manager.second.iter().position(|a| *a == actor) {
            self.manager.second.remove(i);
            return Ok(true);
        }
        Ok(false)
    }
    /// Actual base destruction: decrement, manager flag-zero removal, conditional
    /// frame virtual0, static erasure. Context/player membership is independent.
    /// A failed frame release retains an explicitly unusable owned actor and
    /// cannot be retried by repeating destruction and decrementing again.
    pub fn destroy_base<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_some()
            || self.actors[i].railway.is_some()
            || self.actors[i].human.is_some()
            || self.actors[i].car.is_some()
            || self.actors[i].traffic.is_some()
        {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        self.destroy_base_inner(actor, backend)
    }
    fn destroy_base_inner<B: ActorFrameBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        self.actors[i].phase = if self.actors[i].traffic.is_some() {
            ActorPhase::DestructionInterrupted
        } else {
            ActorPhase::Destroying
        };
        self.actors[i].state.vtable = ActorVtable::Base;
        self.actor_count = self.actor_count.wrapping_sub(1);
        self.remove_from_manager(actor)?;
        let state = self.state(actor)?;
        if state.release_frame != 0 {
            if let Some(frame) = state.frame {
                if let Err(detail) = backend.release_frame(self, frame) {
                    let i = self.index(actor)?;
                    self.actors[i].phase = ActorPhase::DestructionInterrupted;
                    return Err(ActorWorldError::Effect {
                        stage: "release_actor_frame",
                        detail,
                    });
                }
            }
        }
        self.del_actor_from_global(Some(actor));
        let i = self.index(actor)?;
        self.actors[i].phase = ActorPhase::Dead;
        Ok(())
    }
    /// Scalar deleting wrapper: base destruction precedes optional memory free;
    /// only argument bit0 controls the free. Death does not depend on free success.
    pub fn delete_base<B: ActorFrameBackend + ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_base(actor, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_actor",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Base virtual Release decrements the native 16-bit field with wrapping;
    /// reaching zero calls the deleting wrapper with argument1. No unknown
    /// derived virtual deleting destructor is supplied by this base-only owner.
    pub fn release<B: ActorFrameBackend + ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].state.vtable != ActorVtable::Base || self.actors[i].detector.is_some() {
            return Err(ActorWorldError::UnsupportedVtable);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_base(actor, 1, backend)?;
        }
        Ok(count)
    }
    /// C_railway destructor486760: stop model animation0, clear/free vector
    /// backing in observed reverse order, then actual inherited destruction.
    pub fn destroy_railway<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].railway.is_none() {
            return Err(ActorWorldError::NotRailway);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Railway;
        let result = self.destroy_railway_inner(actor, frames, backend);
        if result.is_err() {
            let i = self.index(actor)?;
            self.actors[i].phase = ActorPhase::DestructionInterrupted;
        }
        result
    }
    fn destroy_railway_inner<
        F: ActorFrameBackend + ?Sized,
        B: ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        if let Some(frame) = self.state(actor)?.frame {
            if frames
                .frame_is_model(self, frame)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "railway_destructor_frame_kind",
                    detail,
                })?
            {
                frames
                    .stop_model_animation(self, frame, 0)
                    .map_err(|detail| ActorWorldError::Effect {
                        stage: "railway_stop_animation",
                        detail,
                    })?;
            }
        }
        let i = self.index(actor)?;
        let railway = self.actors[i].railway.as_mut().unwrap();
        railway.put_word(0x158, 0);
        railway.put_word(0x15c, 0);
        for slot in [
            RailwayVectorSlot::At278,
            RailwayVectorSlot::At260,
            RailwayVectorSlot::At250,
            RailwayVectorSlot::At100,
            RailwayVectorSlot::AtEc,
            RailwayVectorSlot::AtA8,
            RailwayVectorSlot::At7c,
        ] {
            let allocation = self.railway(actor)?.vector(slot).begin;
            // The At7c element helper4d2040 is an actual RET8 with no effects.
            backend
                .free_railway_allocation(self, actor, slot, allocation)
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_railway_backing",
                    detail,
                })?;
            let i = self.index(actor)?;
            self.actors[i].railway.as_mut().unwrap().clear_vector(slot);
        }
        self.destroy_base_inner(actor, frames)
    }
    pub fn delete_railway<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<(), ActorWorldError> {
        self.destroy_railway(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_railway",
                    detail,
                })?;
        }
        Ok(())
    }
    pub fn release_railway<F: ActorFrameBackend + ?Sized, B: ActorDeallocationBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut B,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].railway.is_none() || self.actors[i].state.vtable != ActorVtable::Railway {
            return Err(ActorWorldError::NotRailway);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_railway(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
    /// Detector virtual LoadData: the detector's version and raw d8 field precede
    /// embedded source disposal/reads. Source callbacks borrow only the program;
    /// they cannot mutate this exclusively borrowed world during replacement.
    pub fn load_detector_data<R: ProgramReader + ?Sized, P: ProgramBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        reader: &mut R,
        replacements: &[ProgramReplacement],
        mission_path: Option<&[u8]>,
        frames: &dyn FrameLookupBackend,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_none() {
            return Err(ActorWorldError::NotDetector);
        }
        self.actors[i].phase = ActorPhase::Loading;
        let read = reader.read_exact(&mut [0]).and_then(|()| {
            reader.read_exact(&mut self.actors[i].detector.as_mut().unwrap().bytes_d8_ef[..4])
        });
        if let Err(detail) = read {
            self.actors[i].phase = ActorPhase::LoadingInterrupted;
            return Err(ActorWorldError::Effect {
                stage: "detector_read",
                detail,
            });
        }
        let mut program = self.actors[i]
            .detector
            .as_mut()
            .unwrap()
            .program
            .take()
            .unwrap();
        let mut context = ActorProgramContext {
            world: self,
            frames,
            mission_path,
        };
        let result = program.load_source_code(reader, replacements, &mut context, backend);
        self.actors[i].detector.as_mut().unwrap().program = Some(program);
        self.actors[i].phase = if result.is_ok() {
            ActorPhase::Ready
        } else {
            ActorPhase::LoadingInterrupted
        };
        result.map_err(ActorWorldError::Program)
    }
    /// Source-only detector destruction: embedded program before the genuine
    /// base destructor. A source free failure prevents all base destruction.
    pub fn destroy_detector<F: ActorFrameBackend + ?Sized, P: ProgramBackend + ?Sized>(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        let i = self.index(actor)?;
        if !matches!(
            self.actors[i].phase,
            ActorPhase::Ready
                | ActorPhase::LoadingInterrupted
                | ActorPhase::InitializationInterrupted
        ) {
            return Err(ActorWorldError::Busy(actor));
        }
        if self.actors[i].detector.is_none() {
            return Err(ActorWorldError::NotDetector);
        }
        self.actors[i].phase = ActorPhase::Destroying;
        self.actors[i].state.vtable = ActorVtable::Detector;
        let result = self.actors[i]
            .detector
            .as_mut()
            .unwrap()
            .program
            .as_mut()
            .unwrap()
            .destruct(backend);
        if let Err(error) = result {
            self.actors[i].phase = ActorPhase::DestructionInterrupted;
            return Err(ActorWorldError::Program(error));
        }
        self.destroy_base_inner(actor, frames)
    }
    pub fn delete_detector<
        F: ActorFrameBackend + ?Sized,
        P: ProgramBackend + ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        argument: u32,
        frames: &mut F,
        backend: &mut P,
    ) -> Result<(), ActorWorldError> {
        self.destroy_detector(actor, frames, backend)?;
        if argument & 1 != 0 {
            backend
                .free_actor(self, actor.identity())
                .map_err(|detail| ActorWorldError::Effect {
                    stage: "free_detector",
                    detail,
                })?;
        }
        Ok(())
    }
    /// Shared 16-bit Release dispatched to the detector deleting wrapper.
    pub fn release_detector<
        F: ActorFrameBackend + ?Sized,
        P: ProgramBackend + ActorDeallocationBackend + ?Sized,
    >(
        &mut self,
        actor: ActorReferenceId,
        frames: &mut F,
        backend: &mut P,
    ) -> Result<u16, ActorWorldError> {
        let i = self.ready(actor)?;
        if self.actors[i].detector.is_none() || self.actors[i].state.vtable != ActorVtable::Detector
        {
            return Err(ActorWorldError::NotDetector);
        }
        let count = self.actors[i].state.references.wrapping_sub(1);
        self.actors[i].state.references = count;
        if count == 0 {
            self.delete_detector(actor, 1, frames, backend)?;
        }
        Ok(count)
    }
}
/// Explicit diagnostic effect with access to the same live owner/collections.
/// Subsequent lookup reads observe callback mutations, including a changed owner.
pub trait ActorWorldDiagnosticBackend {
    fn message_beep(
        &mut self,
        world: &mut ActorWorld,
        owner: &mut Option<ActorReferenceId>,
        argument: i32,
    ) -> Result<(), LookupError>;
}
pub struct ActorWorldLookup<'a> {
    pub world: &'a mut ActorWorld,
    pub owner: Option<ActorReferenceId>,
    pub frames: &'a dyn FrameLookupBackend,
    pub scene: Option<SceneRoots>,
    pub diagnostic: &'a mut dyn ActorWorldDiagnosticBackend,
    pub diagnostic_default_c_locale: bool,
}
impl LookupWorldBackend for ActorWorldLookup<'_> {
    fn diagnostic_default_c_locale(&self) -> bool {
        self.diagnostic_default_c_locale
    }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        Ok(self.owner)
    }
    fn actor_frame(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<FrameReferenceId>, LookupError> {
        self.world.lookup_actor_frame(actor)
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.world.source(ActorSource::Primary))
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
        Ok(self.world.players)
    }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.world.source(ActorSource::Static))
    }
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> {
        Ok(self.frames)
    }
    fn scene(&self) -> Result<SceneRoots, LookupError> {
        self.scene.ok_or(LookupError::MissingScene)
    }
    fn message_beep(&mut self, argument: i32) -> Result<(), LookupError> {
        self.diagnostic
            .message_beep(self.world, &mut self.owner, argument)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_lookup::{FrameLifetime, FrameLookupError, FrameView};
    use crate::script_lookup;
    use crate::script_scheduler::ControlState;

    fn control() -> ControlState {
        use crate::named_script::ResetState;
        use crate::script_scheduler::{
            Program, ProgramBackend, SchedulerBackend, ScriptEffects, StartupCommand,
        };
        struct NoEffects;
        impl ScriptEffects for NoEffects {
            type Error = std::convert::Infallible;
            fn execute(
                &mut self,
                _: StartupCommand,
                _: &mut ControlState,
                _: &mut ResetState,
                _: i32,
            ) -> Result<(), Self::Error> {
                panic!("declaration must not execute effects")
            }
        }
        let mut control = ControlState::new(0, 0x3f).unwrap();
        let program = Program::from_source("dim_act 1").unwrap();
        let mut state = ResetState {
            program_counter: 0,
            short_stack_begin: 0,
            short_stack_end: 0,
            field_44: 0,
            continuation: 1,
        };
        ProgramBackend {
            program: &program,
            control: &mut control,
            effects: &mut NoEffects,
        }
        .dispatch(&mut state, 0)
        .unwrap();
        control
    }
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xabcd,
            actor_id: 0x12345678,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 0x6c,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn backing(capacity: usize) -> SourceBacking {
        SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(10)),
            capacity,
        }
    }
    fn world() -> ActorWorld {
        ActorWorld::new(0, 0, backing(8), backing(8), backing(8)).unwrap()
    }
    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Allocate(ActorSource, u32, usize, u32),
        Free(ActorSource, Option<ActorStorageId>, usize),
        AllocateActor(u32),
        Data(FrameReferenceId),
        Clear(FrameReferenceId, Option<FrameReferenceId>),
        Find(FrameReferenceId, Vec<u8>, u32, u32, u32),
        Enable(FrameReferenceId, u32, u32),
        ReleaseFrame(FrameReferenceId, u32, usize),
        FreeActor(u32, u32, usize),
    }
    #[derive(Default)]
    struct Effects {
        events: Vec<Event>,
        fail: Option<&'static str>,
        fail_source: Option<ActorSource>,
        actor: Option<ActorReferenceId>,
        userdata: Option<FrameActorData>,
        child: Option<FrameReferenceId>,
        replace_after_e: Option<Option<FrameReferenceId>>,
        allocated: u32,
    }
    impl ActorCollectionBackend for Effects {
        fn allocate_collection(
            &mut self,
            world: &ActorWorld,
            source: ActorSource,
            bytes: u32,
        ) -> Result<ActorStorageId, String> {
            self.events.push(Event::Allocate(
                source,
                bytes,
                world.source(ActorSource::Primary).len(),
                world.actor_count,
            ));
            if self.fail == Some("allocate") && self.fail_source.is_none_or(|s| s == source) {
                return Err("unavailable".into());
            }
            self.allocated += 1;
            Ok(ActorStorageId(100 + self.allocated))
        }
        fn free_collection(
            &mut self,
            world: &ActorWorld,
            source: ActorSource,
            old: Option<ActorStorageId>,
        ) -> Result<(), String> {
            self.events.push(Event::Free(
                source,
                old,
                world.source(ActorSource::Primary).len(),
            ));
            if self.fail == Some("free_collection") {
                return Err("unavailable".into());
            }
            Ok(())
        }
    }
    impl ActorAllocationBackend for Effects {
        fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            self.events.push(Event::AllocateActor(bytes));
            if self.fail == Some("allocate_actor") {
                return Err("unavailable".into());
            }
            if self.fail == Some("null_actor") {
                return Ok(None);
            }
            Ok(Some((42, seed())))
        }
    }
    impl ActorFrameBackend for Effects {
        fn actor_data(
            &mut self,
            _: &mut ActorWorld,
            frame: FrameReferenceId,
        ) -> Result<FrameActorData, String> {
            self.events.push(Event::Data(frame));
            Ok(self.userdata.unwrap_or(FrameActorData::Null))
        }
        fn clear_actor_data(
            &mut self,
            world: &mut ActorWorld,
            frame: FrameReferenceId,
        ) -> Result<(), String> {
            self.events.push(Event::Clear(
                frame,
                world.state(self.actor.unwrap()).unwrap().frame,
            ));
            self.userdata = Some(FrameActorData::Null);
            Ok(())
        }
        fn find_child(
            &mut self,
            world: &mut ActorWorld,
            frame: FrameReferenceId,
            name: &[u8],
            flags: u32,
        ) -> Result<Option<FrameReferenceId>, String> {
            let actor = self.actor.unwrap();
            self.events.push(Event::Find(
                frame,
                name.to_vec(),
                flags,
                world.state(actor).unwrap().state_1c,
                world.state(actor).unwrap().actor_id,
            ));
            if name == b"targetE" {
                if let Some(replacement) = self.replace_after_e {
                    *world.actor_frame_mut(actor).unwrap() = replacement;
                }
            }
            if self.fail == Some("find") {
                return Err("unavailable".into());
            }
            Ok(self.child)
        }
        fn set_child_enabled(
            &mut self,
            world: &mut ActorWorld,
            child: FrameReferenceId,
            enabled: u32,
        ) -> Result<(), String> {
            self.events.push(Event::Enable(
                child,
                enabled,
                world.state(self.actor.unwrap()).unwrap().state_1c,
            ));
            if self.fail == Some("enable") {
                return Err("unavailable".into());
            }
            Ok(())
        }
        fn release_frame(
            &mut self,
            world: &mut ActorWorld,
            frame: FrameReferenceId,
        ) -> Result<(), String> {
            self.events.push(Event::ReleaseFrame(
                frame,
                world.actor_count,
                world.source(ActorSource::Static).len(),
            ));
            if self.fail == Some("release") {
                return Err("unavailable".into());
            }
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Effects {
        fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
            self.events.push(Event::FreeActor(
                identity,
                world.actor_count,
                world.source(ActorSource::Static).len(),
            ));
            if self.fail == Some("free_actor") {
                return Err("unavailable".into());
            }
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, effects: &mut Effects, id: u32) -> ActorReferenceId {
        world.construct_base(id, seed(), effects).unwrap()
    }
    #[test]
    fn constructor_preserves_every_unwritten_seed_field_and_wraps_count() {
        let mut w = world();
        let mut e = Effects::default();
        w.actor_count = u32::MAX;
        let a = actor(&mut w, &mut e, 1);
        let s = w.state(a).unwrap();
        assert_eq!(s.vtable, ActorVtable::Base);
        assert_eq!(
            (
                s.flags,
                s.user_data.clone(),
                s.references,
                s.word_10,
                s.word_14
            ),
            (0, ActorUserData::Null, 1, 1, 1)
        );
        assert_eq!((s.byte_20, s.word_60, s.byte_66, s.frame), (0, 0, 0, None));
        assert_eq!(
            BaseActorSeed {
                word_0e: s.word_0e,
                actor_id: s.actor_id,
                state_1c: s.state_1c,
                bytes_21_5f: s.bytes_21_5f,
                bytes_64_65: s.bytes_64_65,
                byte_67: s.byte_67,
                release_frame: s.release_frame,
                bytes_6d_6f: s.bytes_6d_6f
            },
            seed()
        );
        assert_eq!(w.actor_count, 0);
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
    }
    #[test]
    fn base_accessors_rva_7af20_7afb0_7aff0_47220_91d0_round_trip() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        // GAME 0x47af20 C_actor::GetSubtype reads the 0x14 word.
        assert_eq!(w.state(a).unwrap().subtype(), 1);
        // GAME 0x47afb0 C_actor::GetFrame reads the 0x68 link, None here.
        assert_eq!(w.state(a).unwrap().frame_id(), None);
        // GAME 0x47aff0 C_actor::SetUnknown65 stores the 0x65 byte.
        w.state_mut(a).unwrap().set_unknown65(true);
        assert_eq!(w.state(a).unwrap().bytes_64_65, [0x65, 1]);
        w.state_mut(a).unwrap().set_unknown65(false);
        assert_eq!(w.state(a).unwrap().bytes_64_65, [0x65, 0]);
        // GAME 0x547220 C_actor::SetFromMission marks the mission-file byte.
        assert_eq!(w.state(a).unwrap().byte_66, 0);
        w.state_mut(a).unwrap().set_from_mission();
        assert_eq!(w.state(a).unwrap().byte_66, 1);
        // GAME 0x4091d0 C_actor::Release decrements with 16-bit wrap and
        // returns the post-decrement count (delete-this stays with release).
        w.state_mut(a).unwrap().references = 3;
        assert_eq!(w.state_mut(a).unwrap().release_one(), 2);
        assert_eq!(w.state_mut(a).unwrap().release_one(), 1);
        assert_eq!(w.state_mut(a).unwrap().release_one(), 0);
        w.state_mut(a).unwrap().references = 0;
        assert_eq!(w.state_mut(a).unwrap().release_one(), u16::MAX);
    }
    #[test]
    fn base_liveness_latches_rva_7afd0_4a9990_start_clear_and_round_trip() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        // GAME 0x47afd0 C_actor::GetAlive / GAME 0x4a9990 C_actor::GetRendered.
        assert!(!w.state(a).unwrap().alive());
        assert!(!w.state(a).unwrap().rendered());
        w.state_mut(a).unwrap().set_alive(true);
        w.state_mut(a).unwrap().set_rendered(true);
        assert!(w.state(a).unwrap().alive());
        assert!(w.state(a).unwrap().rendered());
        w.state_mut(a).unwrap().set_alive(false);
        w.state_mut(a).unwrap().set_rendered(false);
        assert!(!w.state(a).unwrap().alive());
        assert!(!w.state(a).unwrap().rendered());
    }
    #[test]
    fn global_actor_count_rva_7b000_tracks_static_membership() {
        let mut w = world();
        let mut e = Effects::default();
        // GAME 0x47b000 C_actor::GetGlobalActors count projection.
        assert_eq!(w.global_actor_count(), 0);
        let a = actor(&mut w, &mut e, 1);
        assert_eq!(w.global_actor_count(), 1);
        // Null and duplicate registrations are no-ops like the native guards.
        w.add_actor_to_global(None, &mut e).unwrap();
        w.add_actor_to_global(Some(a), &mut e).unwrap();
        assert_eq!(w.global_actor_count(), 1);
        w.del_actor_from_global(Some(a));
        assert_eq!(w.global_actor_count(), 0);
        w.del_actor_from_global(None);
        assert_eq!(w.global_actor_count(), 0);
    }
    #[test]
    fn growth_effects_observe_old_headers_and_primary_commits_before_active() {
        let mut w = ActorWorld::new(0, 0, backing(0), backing(0), backing(0)).unwrap();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        assert_eq!(
            e.events,
            [
                Event::Allocate(ActorSource::Static, 4, 0, 1),
                Event::Free(ActorSource::Static, None, 0)
            ]
        );
        e.events.clear();
        w.add_actor(Some(a), true, &mut e).unwrap();
        assert_eq!(
            e.events,
            [
                Event::Allocate(ActorSource::Primary, 4, 0, 1),
                Event::Free(ActorSource::Primary, None, 0),
                Event::Allocate(ActorSource::Active, 4, 1, 1),
                Event::Free(ActorSource::Active, None, 1)
            ]
        );
        assert_eq!(w.backing(ActorSource::Active).capacity, 1);
        let b = actor(&mut w, &mut e, 2);
        let c = actor(&mut w, &mut e, 3);
        assert_eq!(w.backing(ActorSource::Static).capacity, 4);
        assert_ne!(b, c);
    }
    #[test]
    fn active_allocation_failure_preserves_committed_primary() {
        let mut w = ActorWorld::new(0, 0, backing(1), backing(1), backing(0)).unwrap();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        e.fail = Some("allocate");
        e.fail_source = Some(ActorSource::Active);
        assert!(w.add_actor(Some(a), true, &mut e).is_err());
        assert_eq!(w.source(ActorSource::Primary), &[Some(a)]);
        assert!(w.source(ActorSource::Active).is_empty());
        e.fail = None;
        w.add_actor(Some(a), true, &mut e).unwrap();
        assert!(w.source(ActorSource::Active).is_empty());
    }
    #[test]
    fn failed_free_retains_uncommitted_backing_and_interrupted_constructor() {
        let mut w = ActorWorld::new(0, 0, backing(0), backing(0), backing(0)).unwrap();
        let mut e = Effects {
            fail: Some("free_collection"),
            ..Effects::default()
        };
        let error = w.construct_base(1, seed(), &mut e).unwrap_err();
        let a = error.actor.unwrap();
        assert_eq!(w.phase(a), Ok(ActorPhase::Constructing));
        assert_eq!(w.actor_count, 1);
        assert!(w.source(ActorSource::Static).is_empty());
        assert_eq!(w.backing(ActorSource::Static), backing(0));
        assert_eq!(w.pending_backing(ActorSource::Static).unwrap().capacity, 1);
        assert_eq!(
            w.add_actor_to_global(Some(a), &mut e),
            Err(ActorWorldError::IncompleteGrowth(ActorSource::Static))
        );
        assert!(matches!(
            w.destroy_base(a, &mut e),
            Err(ActorWorldError::Busy(_))
        ));
    }
    #[test]
    fn duplicate_primary_does_not_promote_active_and_sources_erase_differently() {
        let mut w = world();
        let mut e = Effects::default();
        let aa: Vec<_> = (1..=4).map(|id| actor(&mut w, &mut e, id)).collect();
        for a in &aa {
            w.add_actor(Some(*a), false, &mut e).unwrap();
        }
        w.add_actor(Some(aa[0]), true, &mut e).unwrap();
        assert!(w.source(ActorSource::Active).is_empty());
        w.del_actor_from_global(Some(aa[1]));
        assert!(w.unload_actor(Some(aa[1])));
        assert_eq!(
            w.source(ActorSource::Static),
            &[Some(aa[0]), Some(aa[3]), Some(aa[2])]
        );
        assert_eq!(
            w.source(ActorSource::Primary),
            &[Some(aa[0]), Some(aa[2]), Some(aa[3])]
        );
    }
    #[test]
    fn null_static_add_is_skipped_and_active_only_unload_returns_false() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        w.add_actor_to_global(None, &mut e).unwrap();
        w.active.values.extend([Some(a), None, Some(a)]);
        assert!(!w.unload_actor(Some(a)));
        assert_eq!(w.source(ActorSource::Active), &[Some(a), None]);
        w.add_actor(None, false, &mut e).unwrap();
        assert_eq!(w.source(ActorSource::Primary), &[None]);
        assert!(w.unload_actor(None));
        assert_eq!(w.source(ActorSource::Active), &[Some(a)]);
    }
    #[test]
    fn init_rejects_null_or_conflicting_owner_without_state_changes() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        e.actor = Some(a);
        let frame = FrameLifetime::for_existing(1);
        let old = w.state(a).unwrap().clone();
        assert_eq!(w.init(a, None, &mut e), Ok(false));
        assert!(e.events.is_empty());
        e.userdata = Some(FrameActorData::Present(Some(b)));
        assert_eq!(w.init(a, Some(frame.reference()), &mut e), Ok(false));
        assert_eq!(w.state(a).unwrap(), &old);
        assert_eq!(w.attachment_counter, 0);
    }
    #[test]
    fn init_accepts_present_null_owner_and_reloads_frame_between_child_callbacks() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        let replacement = FrameLifetime::for_existing(2);
        let child = FrameLifetime::for_existing(3);
        e.actor = Some(a);
        e.userdata = Some(FrameActorData::Present(None));
        e.child = Some(child.reference());
        e.replace_after_e = Some(Some(replacement.reference()));
        w.attachment_counter = u32::MAX;
        assert_eq!(w.init(a, Some(f.reference()), &mut e), Ok(true));
        assert_eq!(
            e.events,
            [
                Event::Data(f.reference()),
                Event::Clear(f.reference(), Some(f.reference())),
                Event::Find(f.reference(), b"targetE".to_vec(), 0x40, seed().state_1c, 0),
                Event::Enable(child.reference(), 0, seed().state_1c),
                Event::Find(
                    replacement.reference(),
                    b"targetN".to_vec(),
                    0x40,
                    seed().state_1c,
                    0
                ),
                Event::Enable(child.reference(), 0, seed().state_1c)
            ]
        );
        let s = w.state(a).unwrap();
        assert_eq!((s.actor_id, s.state_1c, s.release_frame), (0, 0, 0));
        assert_eq!(s.frame, Some(replacement.reference()));
    }
    #[test]
    fn callback_failure_preserves_attachment_prefix_and_no_completed_lifecycle() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        e.actor = Some(a);
        e.child = Some(f.reference());
        e.fail = Some("enable");
        assert!(w.init(a, Some(f.reference()), &mut e).is_err());
        let s = w.state(a).unwrap();
        assert_eq!(
            (s.frame, s.actor_id, s.release_frame, s.state_1c),
            (Some(f.reference()), 1, 0, seed().state_1c)
        );
        assert_eq!(w.phase(a), Ok(ActorPhase::InitializationInterrupted));
        assert_eq!(e.events.len(), 4);
    }
    #[test]
    fn null_frame_after_first_callback_is_explicit_error_before_second_native_call() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        e.actor = Some(a);
        e.replace_after_e = Some(None);
        assert_eq!(
            w.init(a, Some(f.reference()), &mut e),
            Err(ActorWorldError::MissingAttachedFrame)
        );
        assert_eq!(w.state(a).unwrap().state_1c, seed().state_1c);
        assert_eq!(w.phase(a), Ok(ActorPhase::InitializationInterrupted));
    }
    #[test]
    fn destructor_removes_first_manager_matches_in_order_and_keeps_other_sources() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        w.add_actor(Some(a), true, &mut e).unwrap();
        w.players[0] = Some(a);
        w.state_mut(a).unwrap().flags = 0x35;
        w.manager.first = vec![b, a, b];
        w.manager.second = vec![a, b];
        w.manager.associations = vec![
            ActorAssociation::OpaqueLegacy {
                prefix: [1; 80],
                actor: Some(b),
                tail: 1,
            },
            ActorAssociation::OpaqueLegacy {
                prefix: [2; 80],
                actor: Some(a),
                tail: 2,
            },
            ActorAssociation::OpaqueLegacy {
                prefix: [3; 80],
                actor: Some(a),
                tail: 3,
            },
        ];
        w.destroy_base(a, &mut e).unwrap();
        assert_eq!(w.manager.first, [b, b]);
        assert_eq!(w.manager.second, [a, b]);
        assert_eq!(
            w.manager
                .associations
                .iter()
                .map(|r| match r {
                    ActorAssociation::OpaqueLegacy { tail, .. } => *tail,
                    _ => panic!("opaque fixture"),
                })
                .collect::<Vec<_>>(),
            [1, 3]
        );
        assert!(
            matches!(&w.manager.associations[1], ActorAssociation::OpaqueLegacy { prefix, .. } if *prefix == [3; 80])
        );
        assert_eq!(w.source(ActorSource::Primary), &[Some(a)]);
        assert_eq!(w.players[0], Some(a));
        assert_eq!(w.source(ActorSource::Static), &[Some(b)]);
        assert!(w.state(a).is_err());
        assert!(e.events.is_empty());
        assert_eq!(
            w.manager.remove_association(b, true),
            Err(ActorWorldError::UnsupportedManagerFlag)
        );
    }
    #[test]
    fn failed_frame_release_preserves_destruction_prefix_and_prevents_double_decrement() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        w.state_mut(a).unwrap().frame = Some(f.reference());
        w.state_mut(a).unwrap().flags = 0x31;
        w.state_mut(a).unwrap().vtable = ActorVtable::Opaque(8);
        w.manager.first.push(a);
        w.actor_count = 0;
        e.fail = Some("release");
        assert!(w.destroy_base(a, &mut e).is_err());
        assert_eq!(e.events, [Event::ReleaseFrame(f.reference(), u32::MAX, 1)]);
        assert_eq!(w.state(a).unwrap().flags, 0x21);
        assert_eq!(w.state(a).unwrap().vtable, ActorVtable::Base);
        assert_eq!(w.phase(a), Ok(ActorPhase::DestructionInterrupted));
        assert!(w.manager.first.is_empty());
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
        assert_eq!(w.destroy_base(a, &mut e), Err(ActorWorldError::Busy(a)));
        assert_eq!(w.actor_count, u32::MAX);
    }
    #[test]
    fn base_attachment_clears_conditional_release_and_deleting_wrapper_uses_bit_zero() {
        for flag in [0, 1, 2, 3] {
            let mut w = world();
            let mut e = Effects::default();
            let a = actor(&mut w, &mut e, 1);
            let f = FrameLifetime::for_existing(1);
            e.actor = Some(a);
            w.init(a, Some(f.reference()), &mut e).unwrap();
            e.events.clear();
            w.delete_base(a, flag, &mut e).unwrap();
            assert_eq!(
                e.events,
                if flag & 1 != 0 {
                    vec![Event::FreeActor(1, 0, 0)]
                } else {
                    vec![]
                }
            );
        }
    }
    #[test]
    fn release_word_wraps_and_free_failure_does_not_revive_destroyed_actor() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        w.state_mut(a).unwrap().references = 0;
        w.release(a, &mut e).unwrap();
        assert_eq!(w.state(a).unwrap().references, u16::MAX);
        assert_eq!(w.actor_count, 1);
        w.state_mut(a).unwrap().references = 1;
        e.fail = Some("free_actor");
        assert!(w.release(a, &mut e).is_err());
        assert!(w.state(a).is_err());
        assert_eq!(w.actor_count, 0);
        assert!(w.source(ActorSource::Static).is_empty());
    }
    struct Frames(Vec<(FrameLifetime, Vec<u8>)>);
    impl FrameLookupBackend for Frames {
        fn default_c_locale(&self) -> bool {
            true
        }
        fn frame(&self, f: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
            let (_, name) = self
                .0
                .iter()
                .find(|(lease, _)| lease.reference() == f)
                .ok_or(FrameLookupError::StaleFrame(f))?;
            Ok(FrameView {
                name: Some(name),
                child: None,
                next: None,
            })
        }
    }
    #[derive(Default)]
    struct Diagnostic {
        replacement: Option<ActorReferenceId>,
        remove: Option<ActorReferenceId>,
        fail: bool,
        calls: usize,
    }
    impl ActorWorldDiagnosticBackend for Diagnostic {
        fn message_beep(
            &mut self,
            w: &mut ActorWorld,
            owner: &mut Option<ActorReferenceId>,
            arg: i32,
        ) -> Result<(), LookupError> {
            assert_eq!(arg, -1);
            self.calls += 1;
            *owner = self.replacement;
            if let Some(a) = self.remove {
                w.unload_actor(Some(a));
            }
            if self.fail {
                return Err(LookupError::Backend("beep unavailable".into()));
            }
            Ok(())
        }
    }
    fn lookup<'a>(
        w: &'a mut ActorWorld,
        frames: &'a Frames,
        d: &'a mut Diagnostic,
    ) -> ActorWorldLookup<'a> {
        ActorWorldLookup {
            world: w,
            owner: None,
            frames,
            scene: None,
            diagnostic: d,
            diagnostic_default_c_locale: true,
        }
    }
    #[test]
    fn source_membership_preserves_identity_and_actual_death_rejects_reused_raw_identity() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 7);
        w.add_actor(Some(a), true, &mut e).unwrap();
        w.players[1] = Some(a);
        w.unload_actor(Some(a));
        let frames = Frames(vec![]);
        let mut d = Diagnostic::default();
        assert_eq!(lookup(&mut w, &frames, &mut d).actor_frame(a), Ok(None));
        w.destroy_base(a, &mut e).unwrap();
        let replacement = actor(&mut w, &mut e, 7);
        assert_ne!(a, replacement);
        let l = lookup(&mut w, &frames, &mut d);
        assert_eq!(l.actor_frame(a), Err(LookupError::StaleActor(a)));
        assert_eq!(l.actor_frame(replacement), Ok(None));
        assert_eq!(l.players().unwrap()[1], Some(a));
    }
    #[test]
    fn owned_lookup_observes_diagnostic_owner_and_collection_mutations() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        let frames = Frames(vec![(
            FrameLifetime::for_existing(1),
            b"chcipnuti chlapka".to_vec(),
        )]);
        w.state_mut(a).unwrap().frame = Some(frames.0[0].0.reference());
        w.add_actor(Some(a), true, &mut e).unwrap();
        let mut d = Diagnostic {
            replacement: Some(b),
            remove: Some(a),
            ..Diagnostic::default()
        };
        let mut c = control();
        c.set_actor_reference(0, Some(a)).unwrap();
        {
            let mut l = lookup(&mut w, &frames, &mut d);
            l.owner = Some(a);
            script_lookup::find_actor(&mut c, &mut l, 0, b"").unwrap();
        }
        assert_eq!(c.actor_references()[0], Some(b));
        assert_eq!(d.calls, 1);
        assert!(w.source(ActorSource::Primary).is_empty());
    }
    #[test]
    fn diagnostic_failure_keeps_callback_mutations_but_preserves_script_slot() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        let frames = Frames(vec![(
            FrameLifetime::for_existing(1),
            b"CHCIPNUTI CHLAPKA".to_vec(),
        )]);
        w.state_mut(a).unwrap().frame = Some(frames.0[0].0.reference());
        w.add_actor(Some(a), true, &mut e).unwrap();
        let mut d = Diagnostic {
            replacement: Some(b),
            remove: Some(a),
            fail: true,
            calls: 0,
        };
        let mut c = control();
        c.set_actor_reference(0, Some(a)).unwrap();
        {
            let mut l = lookup(&mut w, &frames, &mut d);
            l.owner = Some(a);
            assert!(script_lookup::find_actor(&mut c, &mut l, 0, b"").is_err());
            assert_eq!(l.owner, Some(b));
        }
        assert_eq!(c.actor_references()[0], Some(a));
        assert!(w.source(ActorSource::Primary).is_empty());
    }
    #[test]
    fn incomplete_actor_cannot_be_presented_as_live_lookup_result() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let f = FrameLifetime::for_existing(1);
        w.state_mut(a).unwrap().frame = Some(f.reference());
        e.fail = Some("release");
        assert!(w.destroy_base(a, &mut e).is_err());
        let mut d = Diagnostic::default();
        let frames = Frames(vec![]);
        assert!(matches!(
            lookup(&mut w, &frames, &mut d).actor_frame(a),
            Err(LookupError::Backend(_))
        ));
    }
    #[test]
    fn factory_kind_one_uses_real_base_constructor_and_rejects_unsupported_kinds() {
        let mut w = world();
        let mut e = Effects::default();
        assert_eq!(
            w.create_actor(9, &mut e).unwrap_err().source,
            ActorWorldError::UnsupportedKind(9)
        );
        assert!(e.events.is_empty());
        let a = w.create_actor(1, &mut e).unwrap().unwrap();
        assert_eq!(a.identity(), 42);
        assert_eq!(e.events, [Event::AllocateActor(0x70)]);
        assert_eq!(w.state(a).unwrap().frame, None);
        assert!(w.source(ActorSource::Primary).is_empty());
    }
    #[test]
    fn factory_native_null_results_are_distinct_from_unsupported_kind_and_host_error() {
        let mut w = world();
        let mut e = Effects::default();
        for kind in [0, 39, u32::MAX] {
            assert_eq!(w.create_actor(kind, &mut e), Ok(None));
        }
        assert!(e.events.is_empty());
        e.fail = Some("null_actor");
        assert_eq!(w.create_actor(1, &mut e), Ok(None));
        assert_eq!(w.actor_count, 0);
        assert!(w.source(ActorSource::Static).is_empty());
        e.fail = Some("allocate_actor");
        assert!(w.create_actor(1, &mut e).is_err());
        assert_eq!(w.actor_count, 0);
    }
    #[test]
    fn factory_classifies_every_switch_value_and_default_without_default_effects() {
        let named = [
            1, 2, 4, 5, 6, 8, 9, 10, 12, 18, 20, 21, 22, 23, 24, 25, 27, 28, 29, 30, 31, 32, 33,
            34, 35, 36, 38,
        ];
        for kind in (0..=39).chain([0x8000_0000, u32::MAX]) {
            let mut w = world();
            let mut effects = Effects::default();
            let result = w.create_actor(kind, &mut effects);
            if !named.contains(&kind) {
                assert_eq!(result, Ok(None), "default kind {kind}");
                assert!(effects.events.is_empty());
                assert_eq!(w.actor_count, 0);
                for source in [
                    ActorSource::Static,
                    ActorSource::Primary,
                    ActorSource::Active,
                ] {
                    assert!(w.source(source).is_empty());
                }
            } else if ![1, 2, 4, 5, 6, 8, 12, 18, 20, 25, 27, 28, 33, 34, 35, 36, 38]
                .contains(&kind)
            {
                assert_eq!(
                    result.unwrap_err().source,
                    ActorWorldError::UnsupportedKind(kind)
                );
                assert!(effects.events.is_empty());
                assert_eq!(w.actor_count, 0);
            } else {
                // This backend admits only the base class; derived allocation
                // dependency errors still demonstrate these are named cases.
                assert_ne!(result, Ok(None));
            }
        }
    }
    fn detector_seed() -> DetectorSeed {
        DetectorSeed {
            program: ProgramSeed {
                address_high_byte: 0xff,
                padding_0d: [1; 3],
                words_1c_2c: [2; 5],
                padding_31: [3; 3],
                word_40: 4,
                bytes_64: [5; 4],
            },
            bytes_d8_ef: [0xa5; 24],
            word_f4: 0x12345678,
            bytes_105_107: [0x63; 3],
            bytes_118_127: [0x72; 16],
        }
    }
    #[derive(Default)]
    struct ProgramEffects {
        next: u64,
        frees: Vec<(crate::original_program::ProgramBuffer, Option<u64>)>,
        actor_frees: Vec<u32>,
        fail_free: bool,
    }
    impl ProgramBackend for ProgramEffects {
        fn allocate(
            &mut self,
            _: &CProgram,
            _: crate::original_program::ProgramBuffer,
            _: u32,
        ) -> Result<crate::original_program::ProgramAllocation, String> {
            self.next += 1;
            Ok(crate::original_program::ProgramAllocation(self.next))
        }
        fn free(
            &mut self,
            _: &CProgram,
            b: crate::original_program::ProgramBuffer,
            a: Option<crate::original_program::ProgramAllocation>,
        ) -> Result<(), String> {
            self.frees.push((b, a.map(|a| a.0)));
            if self.fail_free {
                Err("unavailable free".into())
            } else {
                Ok(())
            }
        }
    }
    impl ActorDeallocationBackend for ProgramEffects {
        fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(world.actor_count, 0);
            self.actor_frees.push(identity);
            Ok(())
        }
    }
    fn detector_payload(source: &[u8]) -> Vec<u8> {
        let mut bytes = vec![0xff, 0x12, 0x34, 0x56, 0x78, 0xfe];
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&(source.len() as u32).to_le_bytes());
        bytes.extend_from_slice(source);
        bytes
    }
    #[test]
    fn detector_owns_real_frame_init_payload_replacement_and_release() {
        use crate::actor_frames::{FrameUserData, ModelActorFrames};
        use crate::frame_lookup::ModelFrameLookup;
        use crate::model_loading::{tests::frame, MaterializedModel};
        use crate::original_program::{ProgramPhase, ProgramSliceReader};
        let mut source = frame(6, None, [0.; 3]);
        source.name_bytes = b"actual.full.frame".to_vec();
        let mut model = MaterializedModel::from_parsed(std::sync::Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![source],
        }))
        .unwrap();
        let key = model.source_frame_keys()[0];
        let frame_ref = ModelFrameLookup::borrow(model.arena(), true)
            .reference(key)
            .unwrap();
        let record = model.arena_mut().create_actor_data_record(None).unwrap();
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::ActorRecord(record))
            .unwrap();
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(0x12ffffc0, seed(), detector_seed(), &mut e)
            .unwrap();
        let d = w.detector(a).unwrap();
        assert_eq!(d.program().unwrap().seed().address_high_byte, 0x13);
        assert_eq!(d.bytes_d8_ef, [0xa5; 24]);
        assert_eq!(
            (d.word_f0, d.words_f8_100, d.byte_104, d.words_108_114),
            (0, [0; 3], 0, [0; 4])
        );
        assert_eq!(d.program().unwrap().owner(), None);
        assert!(w
            .init(
                a,
                Some(frame_ref),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        assert_eq!(w.detector(a).unwrap().program().unwrap().owner(), Some(a));
        assert_eq!(w.detector(a).unwrap().data_d8(), 0);
        assert_eq!(model.arena().user_data(key), Ok(FrameUserData::Null));
        let mut p = ProgramEffects::default();
        let replacement = ProgramReplacement {
            mission: Some(b"mission".to_vec()),
            name: None,
            frame: Some(b"ACTUAL.FULL.FRAME".to_vec()),
            source: Some(b"replacement\0ignored".to_vec()),
        };
        w.load_detector_data(
            a,
            &mut ProgramSliceReader::new(&detector_payload(b"uncompiled source")),
            &[replacement],
            Some(b"real\\mission"),
            &ModelFrameLookup::borrow(model.arena(), true),
            &mut p,
        )
        .unwrap();
        let d = w.detector(a).unwrap();
        assert_eq!(d.data_d8(), 0x78563412);
        assert_eq!(
            d.program().unwrap().source().unwrap().c_bytes(),
            Some(&b"replacement"[..])
        );
        assert_eq!(d.program().unwrap().phase(), ProgramPhase::Source);
        assert_eq!(model.arena().actor_data_record(record), Ok(None));
        assert_eq!(
            w.release_detector(
                a,
                &mut ModelActorFrames::borrow(model.arena_mut(), true),
                &mut p
            ),
            Ok(0)
        );
        assert_eq!(w.inspect(a).unwrap().0, ActorPhase::Dead);
        assert_eq!(
            w.inspect_detector(a).unwrap().program().unwrap().phase(),
            ProgramPhase::Dead
        );
        assert_eq!(p.actor_frees, [a.identity()]);
        assert!(w.source(ActorSource::Static).is_empty());
        assert!(model.arena().frame(key).is_ok());
    }
    #[test]
    fn detector_base_bypass_and_failed_source_free_cannot_retire_actor() {
        use crate::original_program::ProgramSliceReader;
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        assert_eq!(w.state_mut(a), Err(ActorWorldError::UnsupportedVtable));
        // Even a corrupted tag cannot change the actual storage owner.
        let i = w.index(a).unwrap();
        w.actors[i].state.vtable = ActorVtable::Base;
        assert_eq!(
            w.destroy_base(a, &mut e),
            Err(ActorWorldError::UnsupportedVtable)
        );
        assert_eq!(
            w.release(a, &mut e),
            Err(ActorWorldError::UnsupportedVtable)
        );
        assert_eq!(w.state(a).unwrap().references, 1);
        let mut p = ProgramEffects::default();
        w.load_detector_data(
            a,
            &mut ProgramSliceReader::new(&detector_payload(b"source")),
            &[],
            None,
            &Frames(vec![]),
            &mut p,
        )
        .unwrap();
        p.fail_free = true;
        assert!(w.destroy_detector(a, &mut e, &mut p).is_err());
        assert_eq!(w.phase(a), Ok(ActorPhase::DestructionInterrupted));
        assert_eq!(w.actor_count, 1);
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
        assert!(matches!(
            w.destroy_detector(a, &mut e, &mut p),
            Err(ActorWorldError::Busy(_))
        ));
        assert_eq!(p.frees.len(), 1);
    }
    #[test]
    fn interrupted_detector_load_keeps_rawfield_and_can_dispose_owned_prefix() {
        use crate::original_program::ProgramSliceReader;
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let mut p = ProgramEffects::default();
        let bytes = detector_payload(b"code");
        assert!(w
            .load_detector_data(
                a,
                &mut ProgramSliceReader::new(&bytes[..bytes.len() - 1]),
                &[],
                None,
                &Frames(vec![]),
                &mut p
            )
            .is_err());
        assert_eq!(w.phase(a), Ok(ActorPhase::LoadingInterrupted));
        assert_eq!(w.detector(a).unwrap().data_d8(), 0x78563412);
        assert!(
            !w.detector(a)
                .unwrap()
                .program()
                .unwrap()
                .source()
                .unwrap()
                .complete
        );
        w.destroy_detector(a, &mut e, &mut p).unwrap();
        assert_eq!(w.actor_count, 0);
    }
    #[test]
    fn detector_factory_registers_base_before_derived_construction() {
        struct Factory;
        impl ActorAllocationBackend for Factory {
            fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
                panic!("kind5")
            }
            fn allocate_detector(
                &mut self,
                bytes: u32,
            ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
                assert_eq!(bytes, 0x128);
                Ok(Some((0x10203040, seed(), detector_seed())))
            }
        }
        impl ActorCollectionBackend for Factory {
            fn allocate_collection(
                &mut self,
                world: &ActorWorld,
                source: ActorSource,
                bytes: u32,
            ) -> Result<ActorStorageId, String> {
                assert_eq!(
                    (source, bytes, world.actor_count),
                    (ActorSource::Static, 4, 1)
                );
                let actor = &world.actors[0];
                assert_eq!(actor.phase, ActorPhase::Constructing);
                assert_eq!(actor.state.vtable, ActorVtable::Base);
                assert_eq!(actor.state.word_10, 1);
                assert!(actor.detector.is_none());
                Ok(ActorStorageId(33))
            }
            fn free_collection(
                &mut self,
                _: &ActorWorld,
                _: ActorSource,
                old: Option<ActorStorageId>,
            ) -> Result<(), String> {
                assert_eq!(old, None);
                Ok(())
            }
        }
        let mut w = ActorWorld::new(0, 0, backing(0), backing(0), backing(0)).unwrap();
        let a = w.create_actor(5, &mut Factory).unwrap().unwrap();
        assert_eq!(w.state(a).unwrap().vtable, ActorVtable::Detector);
        assert_eq!(w.state(a).unwrap().word_10, 5);
        assert_eq!(w.detector(a).unwrap().program().unwrap().owner(), None);
    }
    #[test]
    fn detector_signal_defaults_false_and_round_trips_with_actor_byte_clear() {
        // Hand-derived vectors from the evidenced bodies via mafia-re
        // game/actors/C_detector.h:60-71: GetSignal (GAME 0x47b750) returns the
        // +0xdc bit; SetSignal (GAME 0x47b760) clears C_actor+0x65, then stores
        // the +0xdc bit. The seed presets both +0x64/+0x65 to 0x65.
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        assert!(!w.detector(a).unwrap().signal);
        assert!(!w.detector(a).unwrap().get_signal());
        assert_eq!(w.detector_signal(a), Ok(false));
        assert_eq!(w.state(a).unwrap().bytes_64_65, [0x65; 2]);
        w.set_detector_signal(a, true).unwrap();
        assert!(w.detector(a).unwrap().signal);
        assert!(w.detector(a).unwrap().get_signal());
        assert_eq!(w.detector_signal(a), Ok(true));
        assert_eq!(w.state(a).unwrap().bytes_64_65, [0x65, 0]);
        w.set_detector_signal(a, false).unwrap();
        assert!(!w.detector(a).unwrap().signal);
        assert!(!w.detector(a).unwrap().get_signal());
        assert_eq!(w.detector_signal(a), Ok(false));
        assert_eq!(w.state(a).unwrap().bytes_64_65, [0x65, 0]);
    }
    #[test]
    fn detector_game_init_nulls_signal_and_collision_latches() {
        // Owned slice of C_detector::GameInit (GAME 0x432a00,
        // C_detector.cpp:44-62): the signal + wait_for_hit/dyn_colls
        // nulling. Base calls and coll-pointer nulling stay deferred.
        use crate::script_lookup::DetectorCollisionFlags;
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        w.set_detector_signal(a, true).unwrap();
        w.set_detector_collision_flags(
            a,
            DetectorCollisionFlags {
                wait_for_hit: true,
                dyn_colls: true,
            },
        )
        .unwrap();
        w.game_init_detector(a).unwrap();
        let latched = w.detector(a).unwrap();
        assert!(!latched.signal && !latched.wait_for_hit && !latched.dyn_colls);
    }
    #[test]
    fn detector_init_zeroes_human_types_mask() {
        // GAME 0x432990 C_detector::Init (C_detector.cpp:24-30): the
        // human_types mask (+0xd8) to zero. The C_actor::Init gate and
        // the actor=self assignment stay with the base owner.
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        assert_eq!(w.detector_human_types(a), Ok(0xa5a5a5a5));
        w.init_detector(a).unwrap();
        assert_eq!(w.detector_human_types(a), Ok(0));
    }
    #[test]
    fn detector_signal_rejects_non_detector_owner() {
        let mut w = world();
        let mut e = Effects::default();
        let a = w.construct_base(7, seed(), &mut e).unwrap();
        assert_eq!(w.detector_signal(a), Err(ActorWorldError::NotDetector));
        assert_eq!(
            w.set_detector_signal(a, true),
            Err(ActorWorldError::NotDetector)
        );
        // The rejected store must not touch base bytes.
        assert_eq!(w.state(a).unwrap().bytes_64_65, [0x65; 2]);
    }
    #[test]
    fn detector_process_owners_default_clear_and_round_trip() {
        use crate::script_lookup::DetectorCollisionFlags;
        // Hand-derived vectors from mafia-re game/actors/C_detector.h:73-84:
        // +0xd8 carries the LoadData human-type mask, +0xde/+0xdf the
        // collision-arm latches. The fixture seed presets bytes_d8_ef to 0xa5.
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        assert_eq!(w.detector_human_types(a), Ok(0xa5a5a5a5));
        assert!(!w.detector(a).unwrap().wait_for_hit);
        assert!(!w.detector(a).unwrap().dyn_colls);
        assert_eq!(
            w.detector_collision_flags(a),
            Ok(DetectorCollisionFlags {
                wait_for_hit: false,
                dyn_colls: false,
            })
        );
        w.set_detector_collision_flags(
            a,
            DetectorCollisionFlags {
                wait_for_hit: true,
                dyn_colls: true,
            },
        )
        .unwrap();
        assert!(w.detector(a).unwrap().wait_for_hit);
        assert!(w.detector(a).unwrap().dyn_colls);
        // The rejected stores must not touch base bytes.
        let b = w.construct_base(7, seed(), &mut e).unwrap();
        assert_eq!(
            w.detector_human_types(b),
            Err(ActorWorldError::NotDetector)
        );
        assert_eq!(
            w.detector_collision_flags(b),
            Err(ActorWorldError::NotDetector)
        );
        assert_eq!(
            w.set_detector_collision_flags(
                b,
                DetectorCollisionFlags {
                    wait_for_hit: true,
                    dyn_colls: false,
                }
            ),
            Err(ActorWorldError::NotDetector)
        );
        assert_eq!(w.state(b).unwrap().bytes_64_65, [0x65; 2]);
    }
    #[test]
    fn detector_inrange_candidate_gates_non_entity_owners() {
        // The native scan only counts ACTOR_ENTITY/ACTOR_PLAYER with a human
        // record (C_detector.cpp:216-217). Base and detector owners carry no
        // human record here, so both gate to None; stale owners propagate.
        let mut w = world();
        let mut e = Effects::default();
        let base = w.construct_base(7, seed(), &mut e).unwrap();
        assert_eq!(w.detector_inrange_candidate(base), Ok(None));
        let detector = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        assert_eq!(w.detector_inrange_candidate(detector), Ok(None));
        assert_eq!(
            w.detector_inrange_candidate(crate::script_scheduler::ActorReferenceId::from_existing(
                0xdead
            )),
            Err(ActorWorldError::StaleActor(
                crate::script_scheduler::ActorReferenceId::from_existing(0xdead)
            ))
        );
    }
    #[test]
    fn detector_program_transport_round_trips_through_the_authoritative_owner() {
        // Take/return mirror the entity transport for the C_detector::GameInit
        // C_program::Init prefix; only the program moves, never the actor.
        let mut w = world();
        let mut e = Effects::default();
        let detector = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let program = w.take_detector_program(detector).unwrap();
        assert!(program.is_some());
        assert!(w.detector(detector).unwrap().program().is_none());
        assert!(w.take_detector_program(detector).unwrap().is_none());
        w.return_detector_program(detector, program).unwrap();
        assert!(w.detector(detector).unwrap().program().is_some());
        // A resolved non-detector reports the type gate on both directions.
        // (matches!, not assert_eq!: the taken program payload has no
        // equality impl, like the stale-actor gates below.)
        let base = w.construct_base(7, seed(), &mut e).unwrap();
        assert!(matches!(
            w.take_detector_program(base),
            Err(ActorWorldError::NotDetector)
        ));
        assert_eq!(
            w.return_detector_program(base, None),
            Err(ActorWorldError::NotDetector)
        );
        // Stale actors propagate on both directions.
        let stale = ActorReferenceId::from_existing(999);
        assert!(matches!(
            w.take_detector_program(stale),
            Err(ActorWorldError::StaleActor(_))
        ));
        assert!(matches!(
            w.return_detector_program(stale, None),
            Err(ActorWorldError::StaleActor(_))
        ));
        // A consumed (None) return is a valid empty store on the same owner.
        w.return_detector_program(detector, None).unwrap();
        assert!(w.detector(detector).unwrap().program().is_none());
    }
    #[test]
    fn detector_init_keeps_derived_identity_while_base_callbacks_change_attachment() {
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let first = FrameLifetime::for_existing(21);
        let second = FrameLifetime::for_existing(22);
        e.actor = Some(a);
        e.replace_after_e = Some(Some(second.reference()));
        assert_eq!(w.init(a, Some(first.reference()), &mut e), Ok(true));
        assert_eq!(w.state(a).unwrap().vtable, ActorVtable::Detector);
        assert_eq!(w.state(a).unwrap().word_10, 5);
        assert_eq!(w.state(a).unwrap().frame, Some(second.reference()));
        assert!(e.events.iter().any(|event| matches!(event, Event::Find(frame, name, ..) if *frame == second.reference() && name == b"targetN")));
        assert_eq!(w.detector(a).unwrap().program().unwrap().owner(), Some(a));
        let b = w
            .construct_detector(10, seed(), detector_seed(), &mut e)
            .unwrap();
        e.actor = Some(b);
        e.replace_after_e = Some(None);
        assert_eq!(
            w.init(b, Some(first.reference()), &mut e),
            Err(ActorWorldError::MissingAttachedFrame)
        );
        assert_eq!(w.phase(b), Ok(ActorPhase::InitializationInterrupted));
        assert_eq!(w.detector(b).unwrap().program().unwrap().owner(), None);
        assert_eq!(w.detector(b).unwrap().data_d8(), 0xa5a5a5a5);
    }
    #[test]
    fn detector_load_uncertain_disposal_is_never_replayed_by_teardown() {
        use crate::original_program::{ProgramPhase, ProgramSliceReader};
        let mut w = world();
        let mut e = Effects::default();
        let a = w
            .construct_detector(9, seed(), detector_seed(), &mut e)
            .unwrap();
        let mut p = ProgramEffects::default();
        w.load_detector_data(
            a,
            &mut ProgramSliceReader::new(&detector_payload(b"old")),
            &[],
            None,
            &Frames(vec![]),
            &mut p,
        )
        .unwrap();
        p.fail_free = true;
        assert!(w
            .load_detector_data(
                a,
                &mut ProgramSliceReader::new(&detector_payload(b"new")),
                &[],
                None,
                &Frames(vec![]),
                &mut p
            )
            .is_err());
        assert_eq!(
            w.detector(a).unwrap().program().unwrap().phase(),
            ProgramPhase::Poisoned
        );
        assert_eq!(w.phase(a), Ok(ActorPhase::LoadingInterrupted));
        assert_eq!(p.frees.len(), 1);
        assert_eq!(
            w.destroy_detector(a, &mut e, &mut p),
            Err(ActorWorldError::Program(ProgramError::Unavailable))
        );
        assert_eq!(w.phase(a), Ok(ActorPhase::DestructionInterrupted));
        assert_eq!(p.frees.len(), 1);
        assert_eq!(w.actor_count, 1);
        assert_eq!(w.source(ActorSource::Static), &[Some(a)]);
        assert!(p.actor_frees.is_empty());
    }
    #[test]
    // GAME 0x47af30/0x47af50 (RVA 0x7af30/0x7af50) C_actor::GetPos/SetPos
    // (mafia-re 113b5b6 game/actors/C_actor.h:323-327, :399-403): the stored
    // triplet reads back exactly and the dir window is untouched.
    fn pos_rva_7af30_7af50_round_trip_keeps_dir_window() {
        let mut state = BaseActorState::construct(seed());
        let dir_before = state.dir();
        state.set_pos([1.0, -2.5, 3.25]);
        assert_eq!(state.pos(), [1.0, -2.5, 3.25]);
        assert_eq!(state.dir(), dir_before);
    }
    #[test]
    // GAME 0x47af70/0x47af90 (RVA 0x7af70/0x7af90) C_actor::GetDir/SetDir
    // (mafia-re 113b5b6 game/actors/C_actor.h:329-332, :405-408): the stored
    // triplet reads back exactly and the pos window is untouched.
    fn dir_rva_7af70_7af90_round_trip_keeps_pos_window() {
        let mut state = BaseActorState::construct(seed());
        let pos_before = state.pos();
        state.set_dir([0.0, 0.0, 1.0]);
        assert_eq!(state.dir(), [0.0, 0.0, 1.0]);
        assert_eq!(state.pos(), pos_before);
    }
    #[test]
    // GAME 0x47afe0 (RVA 0x7afe0) C_actor::SetOwnFrame (mafia-re 113b5b6
    // game/actors/C_actor.h:431-435): the 0x6c latch follows the writer; the
    // working-name reader reports it (the original has no getter).
    fn own_frame_rva_7afe0_latch_follows_writer() {
        let mut state = BaseActorState::construct(seed());
        assert!(state.own_frame());
        state.set_own_frame(false);
        assert!(!state.own_frame());
        state.set_own_frame(true);
        assert!(state.own_frame());
    }
    #[test]
    // GAME 0x406dd0 (RVA 0x6dd0) C_actor::Rebirth (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1366-1375): only ACTSTATE_NONE returns to
    // ACTSTATE_ACTIVE with the game-notify edge; other states are untouched.
    fn rebirth_rva_6dd0_only_none_returns_to_active() {
        let mut state = BaseActorState::construct(seed());
        assert!(!state.rebirth());
        assert_eq!(state.act_state(), seed().state_1c);
        state.state_1c = ACTSTATE_ACTIVE;
        assert!(!state.rebirth());
        assert_eq!(state.act_state(), ACTSTATE_ACTIVE);
        state.state_1c = ACTSTATE_NONE;
        assert!(state.rebirth());
        assert_eq!(state.act_state(), ACTSTATE_ACTIVE);
        assert!(!state.rebirth());
    }
    #[test]
    // GAME 0x408040 (RVA 0x8040) C_actor::GetWorldDir (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1691-1697): a staged frame direction wins,
    // otherwise the staged 0x30 direction is returned.
    fn world_dir_rva_8040_frame_wins_over_staged_dir() {
        let mut state = BaseActorState::construct(seed());
        state.set_dir([0.0, 0.0, 1.0]);
        assert_eq!(state.world_dir(None), [0.0, 0.0, 1.0]);
        assert_eq!(state.world_dir(Some([1.0, 0.0, 0.0])), [1.0, 0.0, 0.0]);
    }
    #[test]
    // GAME 0x407fb0 (RVA 0x7fb0) C_actor::IsGlobalActor (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1667-1679): null misses; construction
    // registers (like the native constructor), erasure (GAME 0x407f50)
    // removes and registration (GAME 0x407d50) re-admits without duplicates.
    fn is_global_actor_rva_7fb0_matches_registration() {
        let mut w = world();
        let mut e = Effects::default();
        let a = actor(&mut w, &mut e, 1);
        let b = actor(&mut w, &mut e, 2);
        assert!(!w.is_global_actor(None));
        assert!(w.is_global_actor(Some(a)));
        assert!(w.is_global_actor(Some(b)));
        w.del_actor_from_global(Some(a));
        assert!(!w.is_global_actor(Some(a)));
        assert!(w.is_global_actor(Some(b)));
        w.add_actor_to_global(Some(a), &mut e).unwrap();
        assert!(w.is_global_actor(Some(a)));
    }
    #[test]
    // GAME 0x406590 (RVA 0x6590) C_actor::Actor_SaveGameGetSize (mafia-re
    // 113b5b6 game/actors/C_actor.cpp:1146-1155): two words without a name,
    // two words plus the NUL-terminated name otherwise.
    fn savegame_size_rva_6590_counts_name_plus_nul() {
        assert_eq!(actor_savegame_size(None), 8);
        assert_eq!(actor_savegame_size(Some("")), 8);
        assert_eq!(actor_savegame_size(Some("a")), 10);
        assert_eq!(actor_savegame_size(Some("frame01")), 16);
    }
    #[test]
    // GAME 0x547130 (RVA 0x147130) C_actor::SetFrameActor and GAME 0x604d90
    // (RVA 0x204d90) C_actor::GetFrameActor (mafia-re 113b5b6
    // game/actors/C_actor.h:492-519): explicit unsupported stubs pin the
    // addresses while the frame user-data owner stays DEFERRED.
    fn frame_actor_rva_147130_204d90_stubs_pin_addresses() {
        set_frame_actor_unsupported();
        get_frame_actor_unsupported();
    }
    #[test]
    // GAME 0x405e60 (RVA 0x5e60) C_actor::InternalPlacementReset (mafia-re
    // 113b5b6 game/actors/C_actor.cpp:960-968): without a frame nothing is
    // kept; a staged frame's pos/rot/parent words pass through as saved.
    fn placement_snapshot_rva_5e60_none_keeps_nothing() {
        assert_eq!(snapshot_placement_for_reset(None), None);
        let frame = LiveFramePlacement {
            pos: [1.0, 2.0, 3.0],
            rot: [0.0, 0.0, 0.0, 1.0],
            parent: Some(FrameReferenceId::from_existing(4)),
        };
        assert_eq!(snapshot_placement_for_reset(Some(frame)), Some(frame));
        let parentless = LiveFramePlacement {
            pos: [-0.5, 0.25, 8.0],
            rot: [0.5, -0.5, 0.5, 0.5],
            parent: None,
        };
        assert_eq!(
            snapshot_placement_for_reset(Some(parentless)),
            Some(parentless)
        );
    }
    #[test]
    // GAME 0x405ee0 (RVA 0x5ee0) C_actor::ResetPlacement flag core (mafia-re
    // 113b5b6 game/actors/C_actor.cpp:971-986 with LS3DF/I3D/I3D.h:259-267):
    // the four validity bits clear while UPDATE and QUAT_VALID set; other
    // bits (incl. ON) survive.
    fn restored_flags_rva_5ee0_surgery_keeps_unrelated_bits() {
        assert_eq!(restored_placement_flags(0), 0x4000_0000 | 0x8);
        assert_eq!(
            restored_placement_flags(0xffff_ffff),
            0xffff_ffff & !(0x100 | 0x20 | 0x10 | 0x4) | (0x4000_0000 | 0x8)
        );
        assert_eq!(
            restored_placement_flags(0x1 | 0x4 | 0x10 | 0x20 | 0x100),
            0x1 | 0x4000_0000 | 0x8
        );
        assert_eq!(
            restored_placement_flags(0x4000_0000 | 0x8),
            0x4000_0000 | 0x8
        );
    }
    #[test]
    // GAME 0x406d80 (RVA 0x6d80) C_actor::SaveGameLoadGuid (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1343-1349, ACTOR_SAVEGAME_VERSION 3 per
    // game/actors/C_actor.h:138-153): only version 3 takes the number.
    fn load_guid_rva_6d80_only_version_3_takes_number() {
        assert_eq!(load_guid_number_for_version(3, 41), Some(41));
        assert_eq!(load_guid_number_for_version(3, -7), Some(-7));
        assert_eq!(load_guid_number_for_version(2, 41), None);
        assert_eq!(load_guid_number_for_version(4, 41), None);
        assert_eq!(load_guid_number_for_version(0, 0), None);
    }
    #[test]
    // GAME 0x407cd0 (RVA 0x7cd0) C_actor::DebustizeFrame and GAME 0x407d10
    // (RVA 0x7d10) C_actor::NoShadowFrame (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1613-1637): explicit unsupported stubs pin the
    // addresses while the frame/visual owners stay DEFERRED.
    fn frame_walk_rva_7cd0_7d10_stubs_pin_addresses() {
        debustize_frame_unsupported();
        no_shadow_frame_unsupported();
    }
}

#[cfg(test)]
mod railway_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::frame_lookup::ModelFrameLookup;
    use crate::model_loading::{tests::frame, MaterializedModel, ModelArena};
    use std::{collections::BTreeSet, sync::Arc};
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        fail_registration: bool,
        fail_free: Option<usize>,
        frees: Vec<RailwayVectorSlot>,
        live: BTreeSet<u32>,
        actor_freed: bool,
        saw_raw_tail: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_railway(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, RailwaySeed)>, String> {
            assert_eq!(bytes, 0x370);
            Ok(Some((
                self.identity,
                seed(),
                RailwaySeed {
                    bytes_70_36f: [0xa5; 0x300],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            world: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            let actor = world.actors.last().unwrap().lifetime.reference();
            self.saw_raw_tail = world.inspect_railway(actor).unwrap().bytes() == &[0xa5; 0x300];
            if self.fail_registration {
                Err("injected registration failure".into())
            } else {
                Ok(ActorStorageId(2))
            }
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            assert_eq!(world.actor_count, 9);
            self.actor_freed = true;
            Ok(())
        }
        fn free_railway_allocation(
            &mut self,
            world: &ActorWorld,
            actor: ActorReferenceId,
            slot: RailwayVectorSlot,
            allocation: Option<RailwayAllocation>,
        ) -> Result<(), String> {
            assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Railway);
            assert_eq!(world.railway(actor).unwrap().word(0x15c), 0);
            assert_eq!(world.actor_count, 10);
            self.frees.push(slot);
            if self.fail_free == Some(self.frees.len()) {
                return Err("injected backing free failure".into());
            }
            if let Some(allocation) = allocation {
                assert!(self.live.remove(&allocation.0));
            }
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(8, heap).unwrap().unwrap()
    }
    #[test]
    fn constructor_retains_seed_and_actual_address_high_byte() {
        for identity in [0x6d002000, 0x7d002000] {
            let mut world = world(8);
            let mut heap = Heap {
                identity,
                ..Heap::default()
            };
            let actor = actor(&mut world, &mut heap);
            assert_eq!(world.state(actor).unwrap().word_10, 8);
            assert_eq!(world.state(actor).unwrap().word_14, 1);
            assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Railway);
            assert_eq!(world.actor_count, 10);
            assert_eq!(world.railway_global_byte(), Some(0));
            let railway = world.railway(actor).unwrap();
            for slot in [
                RailwayVectorSlot::At7c,
                RailwayVectorSlot::AtA8,
                RailwayVectorSlot::AtEc,
                RailwayVectorSlot::At100,
                RailwayVectorSlot::At250,
                RailwayVectorSlot::At260,
                RailwayVectorSlot::At278,
            ] {
                assert_eq!(railway.vector(slot).control, (identity >> 24) as u8);
                assert_eq!(railway.vector(slot).begin, None);
            }
            assert_eq!(&railway.bytes()[0..12], &[0xa5; 12]);
            assert_eq!(railway.word(0x2f4), 18);
            assert_eq!(railway.word(0x340), 0);
            assert_eq!(railway.word(0x1bc), 0x461c4000);
            assert_eq!(
                world.state_mut(actor),
                Err(ActorWorldError::UnsupportedVtable)
            );
        }
    }
    #[test]
    fn failed_base_registration_retains_unconstructed_tail() {
        let mut world = world(0);
        let mut heap = Heap {
            identity: 0x6d002000,
            fail_registration: true,
            ..Heap::default()
        };
        let error = world.create_actor(8, &mut heap).unwrap_err();
        let actor = error.actor.unwrap();
        assert!(heap.saw_raw_tail);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Constructing);
        assert_eq!(
            world.inspect_railway(actor).unwrap().bytes(),
            &[0xa5; 0x300]
        );
        assert_eq!(world.railway_global_byte(), None);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
    #[test]
    fn init_admits_real_model_and_rejects_dummy_without_base_attachment() {
        let mut world = world(8);
        let mut heap = Heap {
            identity: 0x6d002000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let mut model = MaterializedModel::from_parsed(Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![frame(1, None, [0.; 3])],
        }))
        .unwrap();
        let dummy = ModelFrameLookup::borrow(model.arena(), true)
            .reference(model.source_frame_keys()[0])
            .unwrap();
        assert!(!world
            .init(
                actor,
                Some(dummy),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        assert_eq!(world.state(actor).unwrap().frame, None);
        assert_eq!(world.attachment_counter, 100);
        assert!(!world
            .init(
                actor,
                None,
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        let root = ModelFrameLookup::borrow(model.arena(), true)
            .reference(model.root())
            .unwrap();
        assert!(world
            .init(
                actor,
                Some(root),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        assert_eq!(world.state(actor).unwrap().frame, Some(root));
        assert_eq!(world.attachment_counter, 101);
    }
    #[test]
    fn load_data_preserves_native_bits_and_guard_skipped_fields() {
        let mut world = world(8);
        let mut heap = Heap {
            identity: 0x6d002000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let mut bytes = vec![255];
        for word in [
            0u32, 0x7fc12345, 0xff800000, 0x80000000, 0x3f800000, 0x461c4000,
        ] {
            bytes.extend(word.to_le_bytes());
        }
        assert_eq!(world.load_railway_data(actor, &bytes[..4]).unwrap(), 1);
        assert_eq!(world.railway(actor).unwrap().word(0x1a8), 1);
        assert_eq!(world.load_railway_data(actor, &bytes).unwrap(), 25);
        assert_eq!(world.railway(actor).unwrap().word(0x1b0), 0x7fc12345);
        assert_eq!(world.railway(actor).unwrap().word(0x1ac), 0xff800000);
        assert_eq!(world.railway(actor).unwrap().word(0x1b4), 0x80000000);
        assert_eq!(world.load_railway_data(actor, &[]).unwrap(), 0);
        assert_eq!(world.railway(actor).unwrap().word(0x1bc), 0x461c4000);
    }
    #[test]
    fn deleting_flags_last_reference_and_real_model_stop_null_guard() {
        for flags in [0, 1, 2, 3] {
            let mut world = world(8);
            let mut heap = Heap {
                identity: 0x6d002000,
                ..Heap::default()
            };
            let actor = actor(&mut world, &mut heap);
            let mut arena = ModelArena::default();
            let model = arena.create_model().unwrap();
            let frame = ModelFrameLookup::borrow(&arena, true)
                .reference(model.root())
                .unwrap();
            world
                .init(
                    actor,
                    Some(frame),
                    &mut ModelActorFrames::borrow(&mut arena, true),
                )
                .unwrap();
            world
                .delete_railway(
                    actor,
                    flags,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap,
                )
                .unwrap();
            assert_eq!(heap.actor_freed, flags & 1 != 0);
            assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
            assert_eq!(
                heap.frees,
                [
                    RailwayVectorSlot::At278,
                    RailwayVectorSlot::At260,
                    RailwayVectorSlot::At250,
                    RailwayVectorSlot::At100,
                    RailwayVectorSlot::AtEc,
                    RailwayVectorSlot::AtA8,
                    RailwayVectorSlot::At7c
                ]
            );
            assert_eq!(world.actor_count, 9);
            assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
        }
    }
    #[test]
    fn release_uses_wrapping_sixteen_bit_count_and_only_destroys_last_reference() {
        for initial in [0u16, 2] {
            let mut world = world(8);
            let mut heap = Heap {
                identity: 0x6d002000,
                ..Heap::default()
            };
            let actor = actor(&mut world, &mut heap);
            let i = world.index(actor).unwrap();
            world.actors[i].state.references = initial;
            let mut arena = ModelArena::default();
            assert_eq!(
                world
                    .release_railway(
                        actor,
                        &mut ModelActorFrames::borrow(&mut arena, true),
                        &mut heap
                    )
                    .unwrap(),
                initial.wrapping_sub(1)
            );
            assert!(heap.frees.is_empty());
            assert!(!heap.actor_freed);
            assert_eq!(world.actor_count, 10);
            if initial == 2 {
                assert_eq!(
                    world
                        .release_railway(
                            actor,
                            &mut ModelActorFrames::borrow(&mut arena, true),
                            &mut heap
                        )
                        .unwrap(),
                    0
                );
                assert_eq!(heap.frees.len(), 7);
                assert!(heap.actor_freed);
                assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
            }
        }
    }
    #[test]
    fn interrupted_nonempty_backing_free_retains_failed_owner_and_blocks_retry() {
        let mut world = world(8);
        let mut heap = Heap {
            identity: 0x6d002000,
            fail_free: Some(2),
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        for (slot, allocation) in [
            (RailwayVectorSlot::At278, 11),
            (RailwayVectorSlot::At260, 12),
        ] {
            world.actors[i]
                .railway
                .as_mut()
                .unwrap()
                .put_word(slot.control_offset() + 4, allocation);
            heap.live.insert(allocation);
        }
        let mut arena = ModelArena::default();
        assert!(world
            .release_railway(
                actor,
                &mut ModelActorFrames::borrow(&mut arena, true),
                &mut heap
            )
            .is_err());
        assert_eq!(world.phase(actor), Ok(ActorPhase::DestructionInterrupted));
        assert_eq!(world.state(actor).unwrap().references, 0);
        assert_eq!(world.actor_count, 10);
        assert_eq!(
            world
                .railway(actor)
                .unwrap()
                .vector(RailwayVectorSlot::At278)
                .begin,
            None
        );
        assert_eq!(
            world
                .railway(actor)
                .unwrap()
                .vector(RailwayVectorSlot::At260)
                .begin,
            Some(RailwayAllocation(12))
        );
        assert!(heap.live.contains(&12));
        assert!(!heap.actor_freed);
        assert!(world
            .release_railway(
                actor,
                &mut ModelActorFrames::borrow(&mut arena, true),
                &mut heap
            )
            .is_err());
        assert_eq!(heap.frees.len(), 2);
    }
}

#[cfg(test)]
mod bridge_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::bridge_actor::BridgeSeed;
    use crate::model_loading::ModelArena;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn bridge_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_bridge(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, BridgeSeed)>, String> {
            assert_eq!(bytes, 0x194);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                BridgeSeed {
                    bytes_00_193: [0xa5; 0x194],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(20, heap).unwrap().unwrap()
    }
    #[test]
    fn bridge_factory_constructs_exact_derived_bytes_and_kind() {
        let mut world = bridge_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 20);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Bridge);
        let bridge = world.bridge(actor).unwrap();
        assert_eq!(bridge.bytes()[0x1c], 0x81);
        assert_eq!(bridge.bytes()[0x20], 0x81);
        assert_eq!(
            u32::from_le_bytes(bridge.bytes()[0x178..0x17c].try_into().unwrap()),
            0x3f5f66f3
        );
        assert_eq!(
            u32::from_le_bytes(bridge.bytes()[0x160..0x164].try_into().unwrap()),
            120000
        );
        assert_eq!(bridge.bytes()[0x5d], 0);
        assert_eq!(bridge.bytes()[0x4f], 0);
        assert_eq!(
            u32::from_le_bytes(bridge.bytes()[0x138..0x13c].try_into().unwrap()),
            7
        );
        assert!(world.inspect_bridge(actor).is_some());
        let mut empty = bridge_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(20, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn bridge_load_data_requires_configured_table() {
        let mut world = bridge_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(
            world.load_bridge_data(actor),
            Err(ActorWorldError::BridgeTableUnavailable)
        );
        world.configure_bridge_table(Some(vec![0; 4]));
        assert_eq!(
            world.load_bridge_data(actor),
            Err(ActorWorldError::BridgeTableTruncated)
        );
        world.configure_bridge_table(Some(vec![0; 680]));
        assert_eq!(world.load_bridge_data(actor), Ok(0));
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        let i = world.index(actor).unwrap();
        world.actors[i].bridge = None;
        assert_eq!(
            world.load_bridge_data(actor),
            Err(ActorWorldError::NotBridge)
        );
    }
    #[test]
    fn bridge_linked_init_records_handle_flag_and_sectors() {
        let mut world = bridge_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert!(!world.init_bridge_linked(actor, None, false).unwrap());
        let sibling = FrameReferenceId::from_existing(0x10203040);
        assert!(world
            .init_bridge_linked(actor, Some(sibling), true)
            .unwrap());
        let bridge = world.bridge(actor).unwrap();
        assert_eq!(bridge.frame_handle(), 0x10203040);
        assert_eq!(bridge.model_l_flag(), 0);
        assert_eq!(world.bridge_sectors(), &BridgeSectors::default());
        assert!(world
            .init_bridge_linked(actor, Some(sibling), false)
            .unwrap());
        let bridge = world.bridge(actor).unwrap();
        assert_eq!(bridge.model_l_flag(), 1);
        assert_eq!(world.bridge_sectors().rows, [[0xffff; 9]; 2]);
        assert_eq!(world.bridge_sectors().cleared, [0; 8]);
        let i = world.index(actor).unwrap();
        world.actors[i].bridge = None;
        assert_eq!(
            world.init_bridge_linked(actor, Some(sibling), true),
            Err(ActorWorldError::NotBridge)
        );
    }
    #[test]
    fn bridge_release_decrements_and_deletes_at_zero() {
        let mut world = bridge_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_bridge(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_bridge(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod door_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::door_actor::DoorSeed;
    use crate::human_actor::ActorDataReader;
    use crate::model_loading::ModelArena;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn door_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_door(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, DoorSeed)>, String> {
            assert_eq!(bytes, 0x1b8);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                DoorSeed {
                    bytes_00_1b7: [0xa5; 0x1b8],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(6, heap).unwrap().unwrap()
    }
    fn v8_payload() -> Vec<u8> {
        let mut payload = vec![8u8, 0, 0, 0];
        payload.push(0x11); // 0x11c lock flag
        payload.push(0x22); // 0x11d
        payload.push(0x33); // 0x11e
        payload.extend(0x12345678u32.to_le_bytes()); // 0x120
        payload.push(0x44); // 0x124
        payload.push(0x55); // 0x125
        payload.extend(0x9abcdef0u32.to_le_bytes()); // 0x128
        payload.extend(0x0fedcba9u32.to_le_bytes()); // 0x12c
        let mut open = [0u8; 16];
        open[..9].copy_from_slice(b"open.wav\0");
        payload.extend(open);
        let mut close = [0u8; 16];
        close[..10].copy_from_slice(b"close.wav\0");
        payload.extend(close);
        let mut locked = [0u8; 16];
        locked[..11].copy_from_slice(b"locked.wav\0");
        payload.extend(locked);
        payload.push(0x66); // 0x164
        payload
    }
    #[test]
    fn door_factory_constructs_exact_derived_bytes_and_kind() {
        let mut world = door_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 6);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Door);
        let door = world.door(actor).unwrap();
        assert_eq!(door.timer_word(), 0x3b83126f);
        assert_eq!(door.bytes()[0x11d], 1);
        assert_eq!(door.bytes()[0x120..0x124], [0xdb, 0x0f, 0xc9, 0x3f]);
        assert!(world.inspect_door(actor).is_some());
        assert_eq!(&world.door_sounds().open[..12], b"d_woodop.wav");
        assert_eq!(&world.door_sounds().close[..12], b"d_woodcl.wav");
        assert_eq!(&world.door_sounds().locked[..12], b"d_woodlo.wav");
        let mut empty = door_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(6, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn door_load_data_dispatches_versions_and_counts_bytes() {
        let mut world = door_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let payload = v8_payload();
        assert_eq!(payload.len(), 70);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(70));
        assert_eq!(reader.position(), 70);
        let door = world.door(actor).unwrap();
        assert_eq!(door.lock_flag(), 0x11);
        assert_eq!(door.bytes()[0x11d], 0x22);
        assert_eq!(door.bytes()[0x11e], 0x33);
        assert_eq!(door.timer_word(), 0x9abcdef0);
        assert_eq!(door.open_sound(), b"open.wav");
        assert_eq!(door.close_sound(), b"close.wav");
        assert_eq!(door.locked_sound(), b"locked.wav");
        assert_eq!(door.bytes()[0x164], 0x66);
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        // Unknown versions consume only the version word.
        let mut reader = ActorDataReader::new(&[9u8, 0, 0, 0, 1, 2, 3]);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(4));
        assert_eq!(reader.position(), 4);
        // A missing version word consumes nothing and still succeeds.
        let mut reader = ActorDataReader::new(&[8u8, 0]);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(0));
        // Truncated version-8 tails keep prior values field by field: each
        // guarded read stands alone, so later narrow reads still land after
        // a skipped wide one, exactly like the native guard chain.
        let mut reader = ActorDataReader::new(&payload[..10]);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(10));
        let door = world.door(actor).unwrap();
        assert_eq!(door.bytes()[0x11c], 0x11);
        assert_eq!(door.bytes()[0x120..0x124], [0x78, 0x56, 0x34, 0x12]);
        assert_eq!(door.bytes()[0x124], 0x78);
        assert_eq!(door.bytes()[0x125], 0x56);
        assert_eq!(door.timer_word(), 0x9abcdef0);
        assert_eq!(door.bytes()[0x164], 0x34);
        let i = world.index(actor).unwrap();
        world.actors[i].door = None;
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(
            world.load_door_data(actor, &mut reader),
            Err(ActorWorldError::NotDoor)
        );
    }
    #[test]
    fn door_older_versions_consume_their_prefixes() {
        let mut world = door_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        // Version 5: 4 + 1+4+1+1+4+4+16 + 16+16 tail names.
        let mut payload = vec![5u8, 0, 0, 0, 0x31];
        payload.extend(0x11111111u32.to_le_bytes());
        payload.extend([0x32, 0x33]);
        payload.extend(0x22222222u32.to_le_bytes());
        payload.extend(0x33333333u32.to_le_bytes());
        payload.extend([0x77u8; 16]);
        payload.extend([0x88u8; 16]);
        payload.extend([0x99u8; 16]);
        assert_eq!(payload.len(), 67);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(67));
        let door = world.door(actor).unwrap();
        assert_eq!(door.bytes()[0x11d], 1);
        assert_eq!(door.bytes()[0x11e], 0x31);
        assert_eq!(door.close_sound(), &[0x88u8; 16][..]);
        // Version 6 adds the 0x11d leading byte.
        let mut payload = vec![6u8, 0, 0, 0, 0x41];
        payload.extend([0u8; 63]);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(68));
        assert_eq!(world.door(actor).unwrap().bytes()[0x11d], 0x41);
        // Version 7 adds the 0x11c leading byte.
        let mut payload = vec![7u8, 0, 0, 0, 0x51, 0x52];
        payload.extend([0u8; 63]);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_door_data(actor, &mut reader), Ok(69));
        let door = world.door(actor).unwrap();
        assert_eq!(door.lock_flag(), 0x51);
        assert_eq!(door.bytes()[0x11d], 0x52);
    }
    #[test]
    fn door_release_decrements_and_deletes_at_zero() {
        let mut world = door_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_door(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_door(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod traffic_car_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::human_actor::ActorDataReader;
    use crate::model_loading::ModelArena;
    use crate::traffic_car_actor::TrafficCarSeed;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn traffic_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_traffic_car(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, TrafficCarSeed)>, String> {
            assert_eq!(bytes, 0x26c);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                TrafficCarSeed {
                    bytes_00_26b: [0xa5; 0x26c],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(12, heap).unwrap().unwrap()
    }
    fn element(name: &[u8]) -> Vec<u8> {
        let mut record = vec![0u8; 0x20];
        let length = name.len().min(0x1d);
        record[..length].copy_from_slice(&name[..length]);
        record[0x14..0x18].copy_from_slice(&0x11111111u32.to_le_bytes());
        record[0x18..0x1c].copy_from_slice(&0x22222222u32.to_le_bytes());
        record[0x1c] = 0x33;
        record[0x1d] = 0x44;
        record
    }
    #[test]
    fn traffic_factory_constructs_exact_derived_bytes_and_kind() {
        let mut world = traffic_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 12);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::TrafficCar);
        let car = world.traffic_car(actor).unwrap();
        assert_eq!(car.speed_word(), 0x43340000);
        assert_eq!(car.count_word(), 0);
        assert_eq!(car.element_count(), 0);
        assert!(world.inspect_traffic_car(actor).is_some());
        assert_eq!(world.traffic_tuning().unit_floats, [0x3f800000; 3]);
        world.refresh_traffic_tuning();
        assert_eq!(world.traffic_tuning().copy_target, 0);
        let mut empty = traffic_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(12, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn traffic_load_data_reads_versioned_header_and_elements() {
        let mut world = traffic_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let mut payload = vec![5u8, 0, 0, 0];
        payload.extend(184.0f32.to_le_bytes());
        payload.extend(150.0f32.to_le_bytes());
        payload.extend(170.0f32.to_le_bytes());
        payload.extend(6u32.to_le_bytes());
        payload.extend(2u32.to_le_bytes());
        payload.extend(element(b"fordttud"));
        payload.extend(element(b"fordtru"));
        assert_eq!(payload.len(), 4 + 16 + 4 + 64);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_traffic_car_data(actor, &mut reader), Ok(88));
        assert_eq!(reader.position(), 88);
        let car = world.traffic_car(actor).unwrap();
        assert_eq!(
            f32::from_le_bytes(car.bytes()[0x224..0x228].try_into().unwrap()),
            184.0
        );
        assert_eq!(car.count_word(), 2);
        assert_eq!(car.element_count(), 2);
        assert_eq!(&car.elements()[..9], b"fordttud\0");
        assert_eq!(&car.elements()[0x20..0x28], b"fordtru\0");
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        // Unknown versions consume only the version word.
        let mut reader = ActorDataReader::new(&[6u8, 0, 0, 0, 1, 2]);
        assert_eq!(world.load_traffic_car_data(actor, &mut reader), Ok(4));
        // A missing version word consumes nothing and still succeeds.
        let mut reader = ActorDataReader::new(&[5u8, 0]);
        assert_eq!(world.load_traffic_car_data(actor, &mut reader), Ok(0));
        // Truncated element tails stay fully zeroed: the native guard skips
        // short whole elements without consuming them.
        let mut reader = ActorDataReader::new(&payload[..40]);
        assert_eq!(world.load_traffic_car_data(actor, &mut reader), Ok(24));
        let car = world.traffic_car(actor).unwrap();
        assert_eq!(car.count_word(), 2);
        assert_eq!(car.element_count(), 2);
        assert_eq!(car.elements(), &[0u8; 64]);
        let i = world.index(actor).unwrap();
        world.actors[i].traffic_car = None;
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(
            world.load_traffic_car_data(actor, &mut reader),
            Err(ActorWorldError::NotTrafficCar)
        );
    }
    #[test]
    fn traffic_release_decrements_and_deletes_at_zero() {
        let mut world = traffic_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_traffic_car(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_traffic_car(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod pumper_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::human_actor::ActorDataReader;
    use crate::model_loading::ModelArena;
    use crate::pumper_actor::PumperSeed;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn pumper_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_pumper(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, PumperSeed)>, String> {
            assert_eq!(bytes, 0x2ac);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                PumperSeed {
                    bytes_00_2ab: [0xa5; 0x2ac],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(25, heap).unwrap().unwrap()
    }
    fn full_payload() -> Vec<u8> {
        let mut payload = vec![0x7fu8];
        payload.extend(0x11111111u32.to_le_bytes());
        payload.extend(0x22222222u32.to_le_bytes());
        payload.extend(0x33333333u32.to_le_bytes());
        payload.extend(0x44444444u32.to_le_bytes());
        payload.extend((0x10u8..0x28u8).collect::<Vec<_>>());
        payload
    }
    #[test]
    fn pumper_factory_constructs_exact_derived_bytes_and_kind() {
        let mut world = pumper_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 25);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Pumper);
        let pumper = world.pumper(actor).unwrap();
        assert_eq!(pumper.counter_a(), 0x29a);
        assert_eq!(pumper.counter_b(), 0x29b);
        assert_eq!(pumper.mode_word(), 0);
        assert_eq!(pumper.price_word(), 0x497cc);
        assert_eq!(pumper.bytes()[0x88..0x8c], [0x1e, 0, 0, 0]);
        assert!(world.inspect_pumper(actor).is_some());
        let mut empty = pumper_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(25, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn pumper_load_data_reads_guarded_fields_and_slot_block() {
        let mut world = pumper_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let payload = full_payload();
        assert_eq!(payload.len(), 41);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_pumper_data(actor, &mut reader), Ok(41));
        assert_eq!(reader.position(), 41);
        let pumper = world.pumper(actor).unwrap();
        assert_eq!(pumper.counter_a(), 0x11111111);
        assert_eq!(pumper.counter_b(), 0x22222222);
        assert_eq!(pumper.price_word(), 0x33333333);
        assert_eq!(pumper.mode_word(), 0x44444444);
        assert_eq!(
            pumper.bytes()[0x88..0xa0],
            (0x10u8..0x28u8).collect::<Vec<_>>()[..]
        );
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        // An empty payload consumes nothing and still succeeds.
        let mut reader = ActorDataReader::new(&[]);
        assert_eq!(world.load_pumper_data(actor, &mut reader), Ok(0));
        // A flag-only payload keeps every constructor value.
        let mut reader = ActorDataReader::new(&[0x7fu8]);
        assert_eq!(world.load_pumper_data(actor, &mut reader), Ok(1));
        let pumper = world.pumper(actor).unwrap();
        assert_eq!(pumper.counter_a(), 0x11111111);
        // A short slot block keeps the loaded slots.
        let mut reader = ActorDataReader::new(&payload[..20]);
        assert_eq!(world.load_pumper_data(actor, &mut reader), Ok(17));
        let pumper = world.pumper(actor).unwrap();
        assert_eq!(pumper.mode_word(), 0x44444444);
        assert_eq!(
            pumper.bytes()[0x88..0xa0],
            (0x10u8..0x28u8).collect::<Vec<_>>()[..]
        );
        let i = world.index(actor).unwrap();
        world.actors[i].pumper = None;
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(
            world.load_pumper_data(actor, &mut reader),
            Err(ActorWorldError::NotPumper)
        );
    }
    #[test]
    fn pumper_release_decrements_and_deletes_at_zero() {
        let mut world = pumper_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_pumper(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_pumper(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod box_new_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::box_new_actor::BoxNewSeed;
    use crate::human_actor::ActorDataReader;
    use crate::model_loading::ModelArena;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn box_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_box_new(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, BoxNewSeed)>, String> {
            assert_eq!(bytes, 0x3a8);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                BoxNewSeed {
                    bytes_00_3a7: [0xa5; 0x3a8],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(35, heap).unwrap().unwrap()
    }
    fn flag2_payload() -> Vec<u8> {
        let mut payload = vec![2u8, 0x11];
        payload.extend(0x11111111u32.to_le_bytes());
        payload.extend(0x22222222u32.to_le_bytes());
        payload.extend(40.0f32.to_le_bytes());
        payload.extend(0.5f32.to_le_bytes());
        payload.extend(0x55555555u32.to_le_bytes());
        payload.extend(0x66666666u32.to_le_bytes());
        payload.push(0x77);
        payload.extend(0x07777777u32.to_le_bytes());
        payload
    }
    #[test]
    fn box_factory_constructs_kind_marker_without_derived_writes() {
        let mut world = box_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 35);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::BoxNew);
        assert_eq!(world.box_new(actor).unwrap().bytes(), &[0xa5; 0x3a8]);
        assert!(world.inspect_box_new(actor).is_some());
        let mut empty = box_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(35, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn box_init_volume_selects_density() {
        let mut world = box_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert!(world.init_box_volume(actor, 40.0).unwrap());
        assert_eq!(world.box_new(actor).unwrap().density_word(), 2000.0);
        let i = world.index(actor).unwrap();
        world.actors[i].box_new = None;
        assert_eq!(
            world.init_box_volume(actor, 40.0),
            Err(ActorWorldError::NotBoxNew)
        );
    }
    #[test]
    fn box_load_data_dispatches_flags_and_clamps() {
        let mut world = box_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert!(world.init_box_volume(actor, 40.0).unwrap());
        let payload = flag2_payload();
        assert_eq!(payload.len(), 31);
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_box_new_data(actor, &mut reader), Ok(31));
        assert_eq!(reader.position(), 31);
        let bx = world.box_new(actor).unwrap();
        assert_eq!(bx.bytes()[0x38c], 0x11);
        assert_eq!(bx.bytes()[0x378], 0x77);
        assert_eq!(
            u32::from_le_bytes(bx.bytes()[0x34c..0x350].try_into().unwrap()),
            0x07777777
        );
        assert_eq!(bx.density_word(), 40.0);
        // Variant 0 reads the six words plus +0x3a0.
        let mut payload = vec![0u8, 0x21];
        payload.extend(1.0f32.to_le_bytes());
        payload.extend(2.0f32.to_le_bytes());
        payload.extend(3.0f32.to_le_bytes());
        payload.extend(4.0f32.to_le_bytes());
        payload.extend(5.0f32.to_le_bytes());
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_box_new_data(actor, &mut reader), Ok(22));
        let bx = world.box_new(actor).unwrap();
        assert_eq!(bx.bytes()[0x38c], 0x21);
        assert_eq!(bx.density_word(), 3.0);
        assert_eq!(bx.bytes()[0x3a0..0x3a4], 5.0f32.to_le_bytes());
        // Variant 1 skips the six words and reads +0x348.
        let mut payload = vec![1u8];
        payload.extend([0x31u8; 21]);
        payload.extend(0x09090909u32.to_le_bytes());
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_box_new_data(actor, &mut reader), Ok(26));
        assert_eq!(
            u32::from_le_bytes(
                world.box_new(actor).unwrap().bytes()[0x348..0x34c]
                    .try_into()
                    .unwrap()
            ),
            0x09090909
        );
        // Other flags consume only the flag byte before the clamp.
        let mut reader = ActorDataReader::new(&[9u8, 1, 2, 3]);
        assert_eq!(world.load_box_new_data(actor, &mut reader), Ok(1));
        // The clamp pins densities set by LoadData on both sides.
        let mut payload = vec![0u8, 0x21];
        payload.extend(1.0f32.to_le_bytes());
        payload.extend(2.0f32.to_le_bytes());
        payload.extend(200000.0f32.to_le_bytes());
        payload.extend(4.0f32.to_le_bytes());
        payload.extend(5.0f32.to_le_bytes());
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_box_new_data(actor, &mut reader), Ok(22));
        let bx = world.box_new(actor).unwrap();
        assert_eq!(bx.density_word(), 100000.0);
        let mut payload = vec![0u8, 0x21];
        payload.extend(1.0f32.to_le_bytes());
        payload.extend(2.0f32.to_le_bytes());
        payload.extend(0.001f32.to_le_bytes());
        payload.extend(4.0f32.to_le_bytes());
        payload.extend(5.0f32.to_le_bytes());
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(world.load_box_new_data(actor, &mut reader), Ok(22));
        assert_eq!(world.box_new(actor).unwrap().density_word(), 0.01);
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        let i = world.index(actor).unwrap();
        world.actors[i].box_new = None;
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(
            world.load_box_new_data(actor, &mut reader),
            Err(ActorWorldError::NotBoxNew)
        );
    }
    #[test]
    fn box_release_decrements_and_deletes_at_zero() {
        let mut world = box_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_box_new(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_box_new(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod clock_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::clock_actor::ClockSeed;
    use crate::model_loading::ModelArena;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn clock_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_clock(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, ClockSeed)>, String> {
            assert_eq!(bytes, 0x88);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                ClockSeed {
                    bytes_00_87: [0xa5; 0x88],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(34, heap).unwrap().unwrap()
    }
    #[test]
    fn clock_factory_constructs_kind_marker_without_derived_writes() {
        let mut world = clock_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 34);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Clock);
        assert_eq!(world.clock(actor).unwrap().bytes(), &[0xa5; 0x88]);
        assert!(world.inspect_clock(actor).is_some());
        let mut empty = clock_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(34, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn clock_linked_init_records_hand_frames() {
        let mut world = clock_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert!(!world.init_clock_linked(actor, None, None, None).unwrap());
        let hour = FrameReferenceId::from_existing(0x10203040);
        let minute = FrameReferenceId::from_existing(0x50607080);
        assert!(!world
            .init_clock_linked(actor, Some(hour), None, None)
            .unwrap());
        assert!(world
            .init_clock_linked(actor, Some(hour), Some(minute), None)
            .unwrap());
        let clock = world.clock(actor).unwrap();
        assert_eq!(clock.hand_hour(), 0x10203040);
        assert_eq!(clock.hand_minute(), 0x50607080);
        assert_eq!(clock.hand_second(), 0);
        let second = FrameReferenceId::from_existing(0x90a0b0c0);
        assert!(world
            .init_clock_linked(actor, Some(hour), Some(minute), Some(second))
            .unwrap());
        assert_eq!(world.clock(actor).unwrap().hand_second(), 0x90a0b0c0);
        let i = world.index(actor).unwrap();
        world.actors[i].clock = None;
        assert_eq!(
            world.init_clock_linked(actor, Some(hour), Some(minute), None),
            Err(ActorWorldError::NotClock)
        );
    }
    #[test]
    fn clock_game_init_and_ai_follow_native_math() {
        let mut world = clock_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        world.game_init_clock(actor, 3, 30).unwrap();
        let clock = world.clock(actor).unwrap();
        assert_eq!(clock.elapsed(), 0);
        let epsilon = 1e-6;
        assert!((clock.angle_hour() - std::f32::consts::FRAC_PI_2).abs() < epsilon);
        assert!((clock.angle_minute() - std::f32::consts::PI).abs() < epsilon);
        world.tick_clock(actor, 60001).unwrap();
        let clock = world.clock(actor).unwrap();
        assert_eq!(clock.elapsed(), 1);
        assert!(
            (clock.angle_minute() - (std::f32::consts::PI + 2.0 * std::f32::consts::PI / 60.0))
                .abs()
                < 1e-5
        );
        let i = world.index(actor).unwrap();
        world.actors[i].clock = None;
        assert_eq!(
            world.game_init_clock(actor, 0, 0),
            Err(ActorWorldError::NotClock)
        );
        assert_eq!(world.tick_clock(actor, 1), Err(ActorWorldError::NotClock));
        assert_eq!(world.load_clock_data(actor), Err(ActorWorldError::NotClock));
    }
    #[test]
    fn clock_load_data_consumes_zero_and_reports_success() {
        let mut world = clock_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.load_clock_data(actor), Ok(0));
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
    }
    #[test]
    fn clock_release_decrements_and_deletes_at_zero() {
        let mut world = clock_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_clock(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_clock(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod actor_shot_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::actor_shot::ActorShotSeed;
    use crate::model_loading::ModelArena;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn shot_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_actor_shot(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, ActorShotSeed)>, String> {
            assert_eq!(bytes, 0x8c);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                ActorShotSeed {
                    bytes_00_8b: [0xa5; 0x8c],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(33, heap).unwrap().unwrap()
    }
    #[test]
    fn shot_factory_constructs_kind_marker_without_derived_writes() {
        let mut world = shot_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 33);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::ActorShot);
        assert_eq!(world.actor_shot(actor).unwrap().bytes(), &[0xa5; 0x8c]);
        assert!(world.inspect_actor_shot(actor).is_some());
        let mut empty = shot_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(33, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn shot_load_data_consumes_zero_and_reports_success() {
        let mut world = shot_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.load_actor_shot_data(actor), Ok(0));
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        let i = world.index(actor).unwrap();
        world.actors[i].actor_shot = None;
        assert_eq!(
            world.load_actor_shot_data(actor),
            Err(ActorWorldError::NotActorShot)
        );
    }
    #[test]
    fn shot_release_decrements_and_deletes_at_zero() {
        let mut world = shot_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_actor_shot(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_actor_shot(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod rail_generator_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::model_loading::ModelArena;
    use crate::rail_generator::RailGeneratorSeed;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn rail_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_rail_generator(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, RailGeneratorSeed)>, String> {
            assert_eq!(bytes, 0x84);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                RailGeneratorSeed {
                    bytes_00_83: [0xa5; 0x84],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(38, heap).unwrap().unwrap()
    }
    struct Frames;
    impl ActorFrameBackend for Frames {
        fn actor_data(
            &mut self,
            _: &mut ActorWorld,
            _: FrameReferenceId,
        ) -> Result<FrameActorData, String> {
            Err("rail test has no frame data".into())
        }
        fn clear_actor_data(
            &mut self,
            _: &mut ActorWorld,
            _: FrameReferenceId,
        ) -> Result<(), String> {
            Ok(())
        }
        fn find_child(
            &mut self,
            _: &mut ActorWorld,
            _: FrameReferenceId,
            _: &[u8],
            _: u32,
        ) -> Result<Option<FrameReferenceId>, String> {
            Ok(None)
        }
        fn set_child_enabled(
            &mut self,
            _: &mut ActorWorld,
            _: FrameReferenceId,
            _: u32,
        ) -> Result<(), String> {
            Ok(())
        }
        fn release_frame(&mut self, _: &mut ActorWorld, _: FrameReferenceId) -> Result<(), String> {
            Ok(())
        }
    }
    #[test]
    fn rail_init_returns_true_without_base_call() {
        // Native Init stores the frame and returns true (C_rail_generator
        // .cpp:26-30); the frame store is unobservable on the host.
        let mut world = rail_world(8);
        let mut heap = Heap::default();
        let actor = actor(&mut world, &mut heap);
        let mut frames = Frames;
        assert_eq!(world.init(actor, None, &mut frames), Ok(true));
        assert_eq!(
            world.init(actor, Some(FrameReferenceId::from_existing(7)), &mut frames),
            Ok(true)
        );
    }
    #[test]
    fn rail_factory_constructs_kind_marker_without_derived_writes() {
        let mut world = rail_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 38);
        assert_eq!(
            world.state(actor).unwrap().vtable,
            ActorVtable::RailGenerator
        );
        assert_eq!(world.rail_generator(actor).unwrap().bytes(), &[0xa5; 0x84]);
        assert!(world.inspect_rail_generator(actor).is_some());
        let mut empty = rail_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(38, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn rail_load_data_consumes_zero_and_reports_success() {
        let mut world = rail_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.load_rail_generator_data(actor), Ok(0));
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        let i = world.index(actor).unwrap();
        world.actors[i].rail_generator = None;
        assert_eq!(
            world.load_rail_generator_data(actor),
            Err(ActorWorldError::NotRailGenerator)
        );
    }
    #[test]
    fn rail_release_decrements_and_deletes_at_zero() {
        let mut world = rail_world(8);
        let mut heap = Heap {
            identity: 0x81000000,
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_rail_generator(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_rail_generator(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod car_ex_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::car_actor::CarSeed;
    use crate::car_ex_actor::CarExSeed;
    use crate::human_actor::ActorDataReader;
    use crate::model_loading::ModelArena;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn car_ex_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        let mut world = ActorWorld::new(9, 100, b, b, b).unwrap();
        world.configure_car_resources(1).unwrap();
        world
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_car_ex(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, CarExSeed)>, String> {
            assert_eq!(bytes, 0x227c);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                CarExSeed {
                    car: CarSeed {
                        allocator_fill: 0xa5,
                        constructor_allocator_tag: 0x79,
                        vehicle_allocator_tag: 0xb3,
                    },
                    bytes_221c_227b: [0x3c; 0x60],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(36, heap).unwrap().unwrap()
    }
    fn car_payload() -> Vec<u8> {
        let packed = vec![0; 4385];
        let mut compressed = b"COMP".to_vec();
        compressed.extend(4385u32.to_le_bytes());
        compressed.extend((4385u32 + 13).to_le_bytes());
        compressed.push(0xff);
        compressed.extend(packed);
        let mut payload = 1u32.to_le_bytes().to_vec();
        payload.extend((compressed.len() as u32).to_le_bytes());
        payload.extend(compressed);
        payload
    }
    #[test]
    fn car_ex_factory_constructs_extended_bytes_and_kind() {
        let mut world = car_ex_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 36);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::CarEx);
        assert_eq!(world.car_ex(actor).unwrap().tail_bytes(), &[0x3c; 0x60]);
        assert!(world.inspect_car_ex(actor).is_some());
        assert!(world.car(actor).is_ok());
        assert!(world.can_use_truck(actor).unwrap());
        let mut empty = car_ex_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(36, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn car_ex_load_is_native_stub_skip() {
        let mut world = car_ex_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        // Native C_car_ex::LoadData is a stub chain (0x42f110 -> C_car
        // stub -> base true): zero consumption, Ready, no conversion.
        let payload = car_payload();
        let mut reader = ActorDataReader::new(&payload);
        assert_eq!(
            world.load_car_data(actor, &mut reader, &mut ModelArena::default()),
            Ok(true)
        );
        assert_eq!(reader.position(), 0);
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
    }
    #[test]
    fn car_ex_passengers_add_once_and_remove() {
        let mut world = car_ex_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let truck = actor(&mut world, &mut heap);
        let rider = actor(&mut world, &mut heap);
        assert!(world.dc_add_person(truck, rider).unwrap());
        assert!(!world.dc_add_person(truck, rider).unwrap());
        assert_eq!(world.car_ex_passengers(truck).unwrap(), &[rider]);
        assert!(!world.dc_del_person(truck, truck).unwrap());
        assert!(world.dc_del_person(truck, rider).unwrap());
        assert!(world.car_ex_passengers(truck).unwrap().is_empty());
        let stale = ActorLifetime::for_existing(0xdeadbeef).reference();
        assert_eq!(
            world.dc_add_person(truck, stale),
            Err(ActorWorldError::StaleActor(stale))
        );
        let i = world.index(truck).unwrap();
        world.actors[i].car_ex = None;
        assert_eq!(
            world.dc_add_person(truck, rider),
            Err(ActorWorldError::NotCarEx)
        );
    }
    #[test]
    fn car_ex_release_uses_shared_car_deleting_wrapper() {
        let mut world = car_ex_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_car(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_car(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod racecam_tests {
    use super::*;
    use crate::actor_frames::ModelActorFrames;
    use crate::model_loading::ModelArena;
    use crate::racecam::RacecamSeed;
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0xaabb,
            actor_id: 0x1234,
            state_1c: 0x87654321,
            bytes_21_5f: [0x21; 63],
            bytes_64_65: [0x65; 2],
            byte_67: 0x67,
            release_frame: 1,
            bytes_6d_6f: [0x6f; 3],
        }
    }
    fn racecam_world(capacity: usize) -> ActorWorld {
        let b = SourceBacking {
            storage: (capacity != 0).then_some(ActorStorageId(1)),
            capacity,
        };
        ActorWorld::new(9, 100, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        identity: u32,
        null: bool,
        actor_freed: bool,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_racecam(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, RacecamSeed)>, String> {
            assert_eq!(bytes, 0x88);
            if self.null {
                return Ok(None);
            }
            self.identity += 1;
            Ok(Some((
                self.identity,
                seed(),
                RacecamSeed {
                    bytes_00_87: [0xa5; 0x88],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
            assert_eq!(identity, self.identity);
            self.actor_freed = true;
            Ok(())
        }
    }
    fn actor(world: &mut ActorWorld, heap: &mut Heap) -> ActorReferenceId {
        world.create_actor(28, heap).unwrap().unwrap()
    }
    #[test]
    fn racecam_factory_constructs_kind_marker_without_derived_writes() {
        let mut world = racecam_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.state(actor).unwrap().word_10, 28);
        assert_eq!(world.state(actor).unwrap().vtable, ActorVtable::Racecam);
        assert_eq!(world.racecam(actor).unwrap().bytes(), &[0xa5; 0x88]);
        assert!(world.inspect_racecam(actor).is_some());
        let mut empty = racecam_world(8);
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(28, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 9);
    }
    #[test]
    fn racecam_load_data_consumes_zero_and_reports_success() {
        let mut world = racecam_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        assert_eq!(world.load_racecam_data(actor), Ok(0));
        assert_eq!(world.phase(actor), Ok(ActorPhase::Ready));
        let i = world.index(actor).unwrap();
        world.actors[i].racecam = None;
        assert_eq!(
            world.load_racecam_data(actor),
            Err(ActorWorldError::NotRacecam)
        );
    }
    #[test]
    fn racecam_dtor_runs_vtable_store_then_base_tail_to_dead() {
        // GAME 0x596240 C_racecam dtor (Ghidra: MOV [ECX],0x625950 then JMP
        // base-dtor tail). The base tail completes to terminal disposal, so
        // the observable end state is phase Dead with vtable Base; delete
        // with arg&1 frees, with arg&1 == 0 does not.
        let mut world = racecam_world(8);
        let mut heap = Heap::default();
        let mut arena = ModelArena::default();
        let doomed = actor(&mut world, &mut heap);
        world
            .destroy_racecam(doomed, &mut ModelActorFrames::borrow(&mut arena, true))
            .unwrap();
        let (phase, state) = world.inspect(doomed).unwrap();
        assert_eq!(phase, ActorPhase::Dead);
        assert_eq!(state.vtable, ActorVtable::Base);
        let mut world = racecam_world(8);
        let mut heap = Heap::default();
        let mut arena = ModelArena::default();
        let victim_keep = actor(&mut world, &mut heap);
        world
            .delete_racecam(victim_keep, 0, &mut ModelActorFrames::borrow(&mut arena, true), &mut heap)
            .unwrap();
        assert!(!heap.actor_freed);
        let victim_free = actor(&mut world, &mut heap);
        world
            .delete_racecam(victim_free, 1, &mut ModelActorFrames::borrow(&mut arena, true), &mut heap)
            .unwrap();
        assert!(heap.actor_freed);
    }
    #[test]
    fn racecam_release_decrements_and_deletes_at_zero() {
        let mut world = racecam_world(8);
        let mut heap = Heap {
            // Car-compat domain needs small identities (see CarState::allocation):
            // default Heap starts at 1, unlike the 0x81000000 pattern elsewhere.
            ..Heap::default()
        };
        let actor = actor(&mut world, &mut heap);
        let i = world.index(actor).unwrap();
        world.actors[i].state.references = 2;
        let mut arena = ModelArena::default();
        assert_eq!(
            world
                .release_racecam(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            1
        );
        assert!(!heap.actor_freed);
        assert_eq!(
            world
                .release_racecam(
                    actor,
                    &mut ModelActorFrames::borrow(&mut arena, true),
                    &mut heap
                )
                .unwrap(),
            0
        );
        assert!(heap.actor_freed);
        assert_eq!(world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(world.actor_count, 9);
        assert_eq!(world.inspect(actor).unwrap().1.vtable, ActorVtable::Base);
    }
}

#[cfg(test)]
mod human_owner_tests {
    use super::*;
    use crate::frame_lookup::{FrameLifetime, ModelFrameLookup};
    use crate::human_actor::{HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties};
    use crate::original_program::{ProgramAllocation, ProgramBuffer, ProgramVtable};
    use std::{cell::RefCell, collections::BTreeSet, rc::Rc};
    fn base() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 3,
            actor_id: 4,
            state_1c: 5,
            bytes_21_5f: [6; 63],
            bytes_64_65: [7; 2],
            byte_67: 8,
            release_frame: 0,
            bytes_6d_6f: [9; 3],
        }
    }
    fn human() -> HumanSeed {
        HumanSeed {
            allocator_tag: 0x3c,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xa5a5a5a5,
            },
            previous_animation_code: 0x11111111,
            requested_animation_code: 0x22222222,
            human_type_mask: 0xa5a5a5a5,
            current_properties: HumanProperties::from_record([0xa5; 64]),
            target_e: None,
            target_n: None,
            base_frame: None,
            auxiliary: std::array::from_fn(|_| HumanAuxiliarySeed {
                untouched_word: 0xa5a5a5a5,
                padding: [0xa5; 3],
                untouched_tail: [0xa5; 16],
            }),
        }
    }
    fn program() -> ProgramSeed {
        ProgramSeed {
            address_high_byte: 0xff,
            padding_0d: [0xa5; 3],
            words_1c_2c: [0xa5a5a5a5; 5],
            padding_31: [0xa5; 3],
            word_40: 0xa5a5a5a5,
            bytes_64: [0xa5; 4],
        }
    }
    fn entity() -> EntitySeed {
        EntitySeed {
            human: human(),
            program: program(),
            allocator_tag_a: 0x12,
            allocator_tag_b: 0x9a,
            untouched_word: 0xa5a5a5a5,
            untouched_padding: [0xa5; 3],
        }
    }
    fn car_seed() -> CarSeed {
        CarSeed {
            allocator_fill: 0xa5,
            constructor_allocator_tag: 0x12,
            vehicle_allocator_tag: 0x9a,
        }
    }
    fn world() -> ActorWorld {
        let b = SourceBacking {
            storage: Some(ActorStorageId(1)),
            capacity: 16,
        };
        ActorWorld::new(0, 0, b, b, b).unwrap()
    }
    #[derive(Default)]
    struct Heap {
        next: u32,
        log: Rc<RefCell<Vec<String>>>,
        null: bool,
        fail_source_free: bool,
        fail_human: Option<HumanResourceSlot>,
        allocations: BTreeSet<u64>,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            panic!("derived allocation must not use base extent")
        }
        fn allocate_player(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, PlayerSeed)>, String> {
            assert_eq!(bytes, 0xb00);
            self.log.borrow_mut().push("allocate-player".into());
            if self.null {
                return Ok(None);
            }
            self.next += 1;
            Ok(Some((
                self.next,
                base(),
                PlayerSeed {
                    human: human(),
                    allocator_tag: 0xd7,
                },
            )))
        }
        fn allocate_entity(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, EntitySeed)>, String> {
            assert_eq!(bytes, 0x128c);
            self.log.borrow_mut().push("allocate-entity".into());
            if self.null {
                return Ok(None);
            }
            self.next += 1;
            Ok(Some((self.next, base(), entity())))
        }
        fn allocate_car(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, CarSeed)>, String> {
            assert_eq!(bytes, 0x221c);
            self.log.borrow_mut().push("allocate-car".into());
            if self.null {
                return Ok(None);
            }
            self.next += 1;
            Ok(Some((self.next, base(), car_seed())))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Err("unexpected source growth".into())
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            panic!("preallocated sources")
        }
    }
    impl ActorDeallocationBackend for Heap {
        fn free_human_collection(
            &mut self,
            w: &ActorWorld,
            a: ActorReferenceId,
            slot: HumanResourceSlot,
        ) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("human:{slot:?}:{:?}", w.state(a).unwrap().vtable));
            if self.fail_human == Some(slot) {
                Err("human free unavailable".into())
            } else {
                Ok(())
            }
        }
        fn free_entity_collection(
            &mut self,
            w: &ActorWorld,
            a: ActorReferenceId,
            slot: EntityResourceSlot,
        ) -> Result<(), String> {
            assert_eq!(
                w.entity(a).unwrap().program().unwrap().vtable(),
                ProgramVtable::Entity
            );
            self.log.borrow_mut().push(format!("entity:{slot:?}"));
            Ok(())
        }
        fn free_actor(&mut self, w: &ActorWorld, id: u32) -> Result<(), String> {
            assert_eq!(w.actor_count, 0);
            assert!(w.source(ActorSource::Static).is_empty());
            self.log.borrow_mut().push(format!("actor:{id}"));
            Ok(())
        }
    }
    impl ProgramBackend for Heap {
        fn allocate(
            &mut self,
            _: &CProgram,
            b: ProgramBuffer,
            _: u32,
        ) -> Result<ProgramAllocation, String> {
            self.next += 1;
            let id = self.next as u64;
            self.allocations.insert(id);
            self.log
                .borrow_mut()
                .push(format!("allocate-program:{b:?}"));
            Ok(ProgramAllocation(id))
        }
        fn free(
            &mut self,
            p: &CProgram,
            b: ProgramBuffer,
            a: Option<ProgramAllocation>,
        ) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("program:{b:?}:{:?}", p.vtable()));
            if self.fail_source_free && b == ProgramBuffer::Source && a.is_some() {
                return Err("source free unavailable".into());
            }
            if let Some(a) = a {
                assert!(self.allocations.remove(&a.0));
            }
            Ok(())
        }
    }
    struct Frames {
        leases: Vec<FrameLifetime>,
        actor: Option<ActorReferenceId>,
        log: Rc<RefCell<Vec<String>>>,
        model: bool,
        e_calls: usize,
        n_calls: usize,
        mutate: bool,
    }
    impl Frames {
        fn new(log: Rc<RefCell<Vec<String>>>) -> Self {
            Self {
                leases: (10..20).map(FrameLifetime::for_existing).collect(),
                actor: None,
                log,
                model: true,
                e_calls: 0,
                n_calls: 0,
                mutate: false,
            }
        }
        fn r(&self, index: usize) -> FrameReferenceId {
            self.leases[index].reference()
        }
    }
    impl ActorFrameBackend for Frames {
        fn frame_is_model(
            &mut self,
            _: &mut ActorWorld,
            f: FrameReferenceId,
        ) -> Result<bool, String> {
            self.log.borrow_mut().push(format!("type:{}", f.identity()));
            Ok(self.model)
        }
        fn stop_model_animation(
            &mut self,
            _: &mut ActorWorld,
            f: FrameReferenceId,
            i: u32,
        ) -> Result<(), String> {
            assert_eq!(i, 0);
            self.log.borrow_mut().push(format!("stop:{}", f.identity()));
            Ok(())
        }
        fn actor_data(
            &mut self,
            _: &mut ActorWorld,
            _: FrameReferenceId,
        ) -> Result<FrameActorData, String> {
            self.log.borrow_mut().push("data".into());
            Ok(FrameActorData::Null)
        }
        fn clear_actor_data(
            &mut self,
            _: &mut ActorWorld,
            _: FrameReferenceId,
        ) -> Result<(), String> {
            self.log.borrow_mut().push("clear".into());
            Ok(())
        }
        fn find_child(
            &mut self,
            w: &mut ActorWorld,
            f: FrameReferenceId,
            name: &[u8],
            flags: u32,
        ) -> Result<Option<FrameReferenceId>, String> {
            self.log.borrow_mut().push(format!(
                "find:{}:{}:{flags:x}",
                f.identity(),
                String::from_utf8_lossy(name)
            ));
            let a = self.actor.unwrap();
            if name == b"targetE" {
                self.e_calls += 1;
                if self.mutate {
                    *w.actor_frame_mut(a).unwrap() =
                        Some(self.r(if self.e_calls == 1 { 1 } else { 2 }));
                }
                Ok(Some(self.r(if self.e_calls == 1 { 4 } else { 6 })))
            } else if name == b"targetN" {
                self.n_calls += 1;
                Ok(Some(self.r(if self.n_calls == 1 { 5 } else { 7 })))
            } else {
                assert_eq!(flags, 0xffff);
                Ok(Some(self.r(9)))
            }
        }
        fn set_child_enabled(
            &mut self,
            w: &mut ActorWorld,
            f: FrameReferenceId,
            e: u32,
        ) -> Result<(), String> {
            assert_eq!(e, 0);
            self.log
                .borrow_mut()
                .push(format!("disable:{}", f.identity()));
            if self.mutate && f == self.r(6) {
                let a = self.actor.unwrap();
                let i = w.index(a).unwrap();
                w.actors[i].human.as_mut().unwrap().target_n = Some(self.r(8));
                *w.actor_frame_mut(a).unwrap() = Some(self.r(3));
            }
            Ok(())
        }
        fn release_frame(&mut self, _: &mut ActorWorld, f: FrameReferenceId) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("release-frame:{}", f.identity()));
            Ok(())
        }
    }
    #[test]
    fn actor_dead_round_trips_via_setter() {
        // GAME 0x47afc0 C_human::GetDead read; stale actors propagate.
        let mut w = world();
        let mut heap = Heap::default();
        let actor = w.create_actor(27, &mut heap).unwrap().unwrap();
        assert_eq!(w.actor_dead(actor), Ok(false));
        w.set_actor_dead(actor, true).unwrap();
        assert_eq!(w.actor_dead(actor), Ok(true));
        w.set_actor_dead(actor, false).unwrap();
        assert_eq!(w.actor_dead(actor), Ok(false));
    }
    #[test]
    fn record_key_is_always_minus_one() {
        // GAME 0x404f30 C_actor::RecordKey always returns -1 (mafia-re
        // 113b5b6 game/actors/C_actor.cpp:670-674, C_actor.h:166).
        let state = BaseActorState::construct(base());
        assert_eq!(state.record_key(0), -1);
        assert_eq!(state.record_key(12345), -1);
    }
    #[test]
    fn human_car_latches_seat_owner_and_gates_non_humans() {
        // GAME 0x47b7b0 C_human::GetCar: ENTITY/PLAYER latch read; a
        // non-human resolves to None without a dest write (VM-owned null
        // branch). The three-conjunct nulling gate stays in the VM.
        let mut w = world();
        let mut heap = Heap::default();
        w.configure_car_resources(0x1234).unwrap();
        let car = w.create_actor(4, &mut heap).unwrap().unwrap();
        let pedestrian = w.create_actor(27, &mut heap).unwrap().unwrap();
        assert_eq!(w.human_car(car), Ok(None));
        assert_eq!(w.human_car(pedestrian), Ok(Some(None)));
        w.set_human_car(pedestrian, Some(car)).unwrap();
        assert_eq!(w.human_car(pedestrian), Ok(Some(Some(car))));
        w.set_human_car(pedestrian, None).unwrap();
        assert_eq!(w.human_car(pedestrian), Ok(Some(None)));
    }
    #[test]
    fn car_speed_none_for_non_car_and_zero_latch_for_fresh_car() {
        // GAME 0x47b0f0 C_Vehicle::GetSpeed: non-car resolves to None
        // (caller leaves the float bank untouched); fresh cars latch 0.0.
        let mut w = world();
        let mut heap = Heap::default();
        w.configure_car_resources(0x1234).unwrap();
        let pedestrian = w.create_actor(27, &mut heap).unwrap().unwrap();
        let car = w.create_actor(4, &mut heap).unwrap().unwrap();
        assert_eq!(w.car_speed(pedestrian), Ok(None));
        assert_eq!(w.car_speed(car), Ok(Some(0.0)));
    }
    #[test]
    fn is_car_usable_none_for_non_car_and_stub_false_for_car() {
        // GAME 0x429850 STUB C_car::IsCarUsable always returns false.
        let mut w = world();
        let mut heap = Heap::default();
        w.configure_car_resources(0x1234).unwrap();
        let pedestrian = w.create_actor(27, &mut heap).unwrap().unwrap();
        let car = w.create_actor(4, &mut heap).unwrap().unwrap();
        assert_eq!(w.is_car_usable(pedestrian), Ok(None));
        assert_eq!(w.is_car_usable(car), Ok(Some(false)));
    }
    #[test]
    fn set_car_break_motor_false_for_pedestrian_true_for_car() {
        // GAME 0x47b3e0 C_car::SetBreakMotor: non-car stores nothing.
        let mut w = world();
        let mut heap = Heap::default();
        w.configure_car_resources(0x1234).unwrap();
        let pedestrian = w.create_actor(27, &mut heap).unwrap().unwrap();
        let car = w.create_actor(4, &mut heap).unwrap().unwrap();
        assert_eq!(w.set_car_break_motor(pedestrian, true), Ok(false));
        assert_eq!(w.set_car_break_motor(car, true), Ok(true));
    }
    #[test]
    fn player_controls_lock_accepts_release_cycle() {
        // GAME 0x595f00 STUB C_player::LockControls: game-player latch.
        let mut w = world();
        let mut heap = Heap::default();
        let _player = w.create_actor(2, &mut heap).unwrap().unwrap();
        assert!(w.set_player_controls_locked(true).is_ok());
        assert!(w.set_player_controls_locked(false).is_ok());
    }
    #[test]
    fn human_player_entity_factories_own_real_derived_defaults_and_separate_program() {
        let mut w = world();
        let mut heap = Heap::default();
        let player = w.create_actor(2, &mut heap).unwrap().unwrap();
        let entity = w.create_actor(27, &mut heap).unwrap().unwrap();
        assert_eq!(
            (
                w.state(player).unwrap().vtable,
                w.state(player).unwrap().word_10,
                w.state(player).unwrap().word_14
            ),
            (ActorVtable::Player, 2, 2)
        );
        assert_eq!(w.player(player).unwrap().constructor_words(), &[0, 200]);
        assert_eq!(w.player(player).unwrap().allocator_tag(), 0xd7);
        assert_eq!(w.human(player).unwrap().voice_index(), 3);
        assert_eq!(w.human(player).unwrap().human_type_mask(), 0xa5a5a5a5);
        let bits = w.human(player).unwrap().properties().component_bits();
        assert_eq!(&bits[2..6], &[0x42200000; 4]);
        assert_eq!(bits[1], 0x43480000);
        assert_eq!(bits[10], 0x3f800000);
        assert_eq!(
            w.human(player).unwrap().current_properties().record(),
            &[0xa5; 64]
        );
        assert_eq!(w.state(entity).unwrap().vtable, ActorVtable::Entity);
        let e = w.entity(entity).unwrap();
        assert_eq!(e.program().unwrap().vtable(), ProgramVtable::Entity);
        assert_eq!(e.program().unwrap().owner(), None);
        assert_eq!(
            e.program().unwrap().seed().address_high_byte,
            ((entity.identity() + 0xa9c) >> 24) as u8
        );
        assert_eq!(e.untouched_word(), 0xa5a5a5a5);
        assert_eq!(e.untouched_padding(), &[0xa5; 3]);
        assert_eq!(e.constructor_words(), &[0; 8]);
        assert_eq!(e.allocator_tag(EntityResourceSlot::CharacterLogs), 0x12);
        assert_eq!(e.allocator_tag(EntityResourceSlot::SecondUnsigned), 0x12);
        assert_eq!(e.allocator_tag(EntityResourceSlot::ThirdUnsigned), 0x9a);
        assert_eq!(e.allocator_tag(EntityResourceSlot::Last), 0x9a);
        assert_eq!(w.source(ActorSource::Static), &[Some(player), Some(entity)]);
        assert!(w.source(ActorSource::Primary).is_empty());
        let mut empty = world();
        let mut null = Heap {
            null: true,
            ..Heap::default()
        };
        assert_eq!(empty.create_actor(2, &mut null).unwrap(), None);
        assert_eq!(empty.create_actor(27, &mut null).unwrap(), None);
        assert_eq!(empty.actor_count, 0);
    }
    #[test]
    fn human_init_repeats_lookups_and_rereads_saved_targets_after_callbacks() {
        let mut w = world();
        let mut heap = Heap::default();
        let a = w.create_actor(2, &mut heap).unwrap().unwrap();
        let mut frames = Frames::new(heap.log.clone());
        frames.actor = Some(a);
        frames.mutate = true;
        let root = frames.r(0);
        assert!(w.init(a, Some(root), &mut frames).unwrap());
        assert_eq!(w.human(a).unwrap().human_type_mask(), 1);
        assert_eq!(w.human(a).unwrap().target_e(), Some(frames.r(6)));
        assert_eq!(w.human(a).unwrap().target_n(), Some(frames.r(8)));
        assert_eq!(w.human(a).unwrap().base_frame(), Some(frames.r(9)));
        let log = heap.log.borrow();
        let actual: Vec<_> = log
            .iter()
            .filter(|s| s.starts_with("find:") || s.starts_with("disable:"))
            .cloned()
            .collect();
        assert_eq!(
            actual,
            [
                "find:10:targetE:40",
                "disable:14",
                "find:11:targetN:40",
                "disable:15",
                "find:11:targetE:40",
                "find:12:targetN:40",
                "disable:16",
                "disable:18",
                "find:13:base:ffff"
            ]
        );
    }
    #[test]
    fn entity_process_state_round_trips_through_the_authoritative_owner() {
        let mut w = world();
        let mut heap = Heap::default();
        let entity = w.create_actor(27, &mut heap).unwrap().unwrap();
        assert_eq!(
            w.entity_process_state(entity).unwrap(),
            EntityProcessState::default()
        );
        let state = EntityProcessState {
            process_flags: 0x8100,
            visibility_bits: 3,
            shut_up: 7,
        };
        w.store_entity_process_state(entity, &state).unwrap();
        assert_eq!(w.entity_process_state(entity).unwrap(), state);
        assert_eq!(w.inspect_entity(entity).unwrap().process_state(), state);
        let player = w.create_actor(2, &mut heap).unwrap().unwrap();
        assert_eq!(
            w.entity_process_state(player),
            Err(ActorWorldError::NotEntity)
        );
        assert_eq!(
            w.store_entity_process_state(player, &state),
            Err(ActorWorldError::NotEntity)
        );
        assert_eq!(
            w.entity_process_state(ActorReferenceId::from_existing(999)),
            Err(ActorWorldError::StaleActor(
                ActorReferenceId::from_existing(999)
            ))
        );
    }
    #[test]
    fn act_state_and_weapon_write_through_authoritative_owners() {
        let mut w = world();
        let mut heap = Heap::default();
        let entity = w.create_actor(27, &mut heap).unwrap().unwrap();
        w.set_actor_act_state(entity, 1).unwrap();
        assert_eq!(w.state(entity).unwrap().state_1c, 1);
        w.human_add_weapon(entity, 9, 7, 100).unwrap();
        let items = w.human(entity).unwrap().inventory().items();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].representation_bits(), &[9, 7, 100, 0]);
        w.human_add_weapon(entity, 10, 1, 2).unwrap();
        assert_eq!(w.human(entity).unwrap().inventory().items().len(), 2);
        let player = w.create_actor(2, &mut heap).unwrap().unwrap();
        w.human_add_weapon(player, 5, 0, 0).unwrap();
        assert_eq!(w.human(player).unwrap().inventory().items().len(), 1);
        assert_eq!(
            w.entity_process_state(ActorReferenceId::from_existing(999)),
            Err(ActorWorldError::StaleActor(
                ActorReferenceId::from_existing(999)
            ))
        );
    }
    #[test]
    fn entity_owner_link_precedes_rejected_init_and_player_false_init_releases_derived_storage() {
        let mut w = world();
        let mut heap = Heap::default();
        let a = w.create_actor(27, &mut heap).unwrap().unwrap();
        let mut frames = Frames::new(heap.log.clone());
        frames.actor = Some(a);
        frames.model = false;
        assert!(!w.init(a, Some(frames.r(0)), &mut frames).unwrap());
        assert_eq!(w.entity(a).unwrap().program().unwrap().owner(), Some(a));
        assert_eq!(w.human(a).unwrap().human_type_mask(), 0xa5a5a5a5);
        assert_eq!(w.state(a).unwrap().frame, None);
        w.delete_entity(a, 1, &mut frames, &mut heap).unwrap();
        assert_eq!(w.actor_count, 0);
        let mut w = world();
        let a = w.create_actor(2, &mut heap).unwrap().unwrap();
        frames.actor = Some(a);
        assert!(!w.init(a, None, &mut frames).unwrap());
        assert_eq!(w.release_player(a, &mut frames, &mut heap).unwrap(), 0);
        assert_eq!(w.inspect(a).unwrap().0, ActorPhase::Dead);
        let log = heap.log.borrow();
        assert!(log.iter().any(|e| e == "human:Player:Player"));
    }
    fn payload(code: &[u8], short_human: bool) -> Vec<u8> {
        let mut p = vec![6];
        p.extend(7u32.to_le_bytes());
        p.extend(9u32.to_le_bytes());
        if !short_human {
            p.extend([0xff; 64]);
        }
        p.push(0xfe);
        p.push(1);
        p.extend(0u32.to_le_bytes());
        p.extend((code.len() as u32).to_le_bytes());
        p.extend(code);
        p
    }
    #[test]
    fn entity_same_reader_supports_bounded_human_skips_and_real_program_lifetime() {
        for short in [false, true] {
            let mut w = world();
            let mut heap = Heap::default();
            let a = w.create_actor(27, &mut heap).unwrap().unwrap();
            let mut frames = Frames::new(heap.log.clone());
            frames.actor = Some(a);
            assert!(!w.init(a, None, &mut frames).unwrap());
            let data = payload(b"End;", short);
            let mut reader = ActorDataReader::new(&data);
            let arena = crate::model_loading::ModelArena::default();
            let lookup = ModelFrameLookup::borrow(&arena, true);
            w.load_entity_data(a, &mut reader, &[], None, &lookup, &mut heap)
                .unwrap();
            assert_eq!(reader.position(), data.len());
            assert_eq!(w.human(a).unwrap().human_type_mask(), 7);
            assert_eq!(w.human(a).unwrap().voice_index(), 9);
            assert_eq!(
                w.entity(a)
                    .unwrap()
                    .program()
                    .unwrap()
                    .source()
                    .unwrap()
                    .c_bytes(),
                Some(b"End;".as_slice())
            );
            if !short {
                assert_eq!(w.human(a).unwrap().properties().record(), &[0xff; 64]);
            }
            let log_start = heap.log.borrow().len();
            w.release_entity(a, &mut frames, &mut heap).unwrap();
            assert!(heap.allocations.is_empty());
            let log = heap.log.borrow();
            let tail = &log[log_start..];
            assert_eq!(
                &tail[..7],
                [
                    "entity:Last",
                    "entity:Auxiliary",
                    "entity:ThirdUnsigned",
                    "entity:SecondUnsigned",
                    "entity:FirstUnsigned",
                    "entity:SubstateLocks",
                    "entity:CharacterLogs"
                ]
            );
            let program = tail
                .iter()
                .position(|s| s == "program:Source:Base")
                .unwrap();
            let human = tail
                .iter()
                .position(|s| s == "human:LastAuxiliary:Human")
                .unwrap();
            assert!(program < human);
            assert!(tail.last().unwrap().starts_with("actor:"));
        }
    }
    #[test]
    fn entity_program_free_failure_preserves_prefix_and_prevents_human_stop_or_replay() {
        let mut w = world();
        let mut heap = Heap::default();
        let a = w.create_actor(27, &mut heap).unwrap().unwrap();
        let mut frames = Frames::new(heap.log.clone());
        frames.actor = Some(a);
        assert!(w.init(a, Some(frames.r(0)), &mut frames).unwrap());
        let data = payload(b"End;", false);
        let mut reader = ActorDataReader::new(&data);
        let arena = crate::model_loading::ModelArena::default();
        let lookup = ModelFrameLookup::borrow(&arena, true);
        w.load_entity_data(a, &mut reader, &[], None, &lookup, &mut heap)
            .unwrap();
        heap.fail_source_free = true;
        heap.log.borrow_mut().clear();
        assert!(w.delete_entity(a, 1, &mut frames, &mut heap).is_err());
        assert_eq!(w.phase(a).unwrap(), ActorPhase::DestructionInterrupted);
        assert_eq!(w.actor_count, 1);
        let log = heap.log.borrow().clone();
        assert!(!log
            .iter()
            .any(|s| s.starts_with("stop:") || s.starts_with("human:")));
        assert_eq!(
            w.entity(a).unwrap().program().unwrap().vtable(),
            ProgramVtable::Base
        );
        assert!(
            w.entity(a)
                .unwrap()
                .program()
                .unwrap()
                .source()
                .unwrap()
                .disposal_uncertain
        );
        assert!(w.delete_entity(a, 1, &mut frames, &mut heap).is_err());
        assert_eq!(*heap.log.borrow(), log);
    }
    #[test]
    // GAME 0x4091f0 (RVA 0x91f0) C_actor::GetActorSpeed (mafia-re 113b5b6
    // game/actors/C_actor.h:230-233): the base zeroes the caller's speed.
    fn base_actor_speed_rva_91f0_zeroes_speed() {
        let state = BaseActorState::construct(base());
        let mut speed = [1.0f32, -2.0, 3.5];
        state.base_actor_speed_into(&mut speed);
        assert_eq!(speed, [0.0, 0.0, 0.0]);
    }
    #[test]
    // GAME 0x409210 (RVA 0x9210) C_actor::GetMaterialID (mafia-re 113b5b6
    // game/actors/C_actor.h:250-253): the base reports material 0.
    fn material_id_rva_9210_returns_zero() {
        let state = BaseActorState::construct(base());
        assert_eq!(state.material_id(), 0);
    }
    #[test]
    // GAME 0x407590 (RVA 0x7590) C_actor::IsPossibleColl (mafia-re 113b5b6
    // game/actors/C_actor.h:254-258): the base never collides.
    fn is_possible_coll_rva_7590_returns_false() {
        let state = BaseActorState::construct(base());
        assert!(!state.is_possible_coll());
    }
    #[test]
    // GAME 0x4aa450 (RVA 0x4a450) C_actor::Collision (mafia-re 113b5b6
    // game/actors/C_actor.h:238-242): the base never collides.
    fn collision_rva_4a450_returns_false() {
        let state = BaseActorState::construct(base());
        assert!(!state.collision());
    }
    #[test]
    // GAME 0x404e00 (RVA 0x4e00) C_actor::AddToCutscene (mafia-re 113b5b6
    // game/actors/C_actor.cpp:623-629): one record kind of 0x5c bytes, no
    // other kinds, verdict true.
    fn add_to_cutscene_rva_4e00_stages_single_kind_sizes() {
        let state = BaseActorState::construct(base());
        assert_eq!(state.add_to_cutscene(), ([0x5c, 0, 0], true));
    }
    #[test]
    // GAME 0x4075a0 (RVA 0x75a0) C_actor::GetBoundPoints2D (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1534): explicit unsupported stub, frame owner
    // DEFERRED.
    fn bound_points_2d_rva_75a0_is_unsupported_stub() {
        BaseActorState::bound_points_2d_unsupported();
    }
    #[test]
    // Inline C_actor::GetActState (mafia-re 113b5b6
    // game/actors/C_actor.h:281-286; working name, no RVA claimed): reads
    // the staged 0x1c act-state word.
    fn act_state_working_name_reads_state_word() {
        let state = BaseActorState::construct(base());
        assert_eq!(state.act_state(), 5);
    }
    #[test]
    // Inline C_actor::GetType (mafia-re 113b5b6 game/actors/C_actor.h:294-299
    // with ACTOR_ACTOR 1 at :18; working name, no RVA claimed): reads the
    // staged 0x10 type word.
    fn actor_type_working_name_reads_actor_word() {
        let state = BaseActorState::construct(base());
        assert_eq!(state.actor_type(), 1);
    }
    #[test]
    // Inline C_actor::GetActFlags (mafia-re 113b5b6
    // game/actors/C_actor.h:446-450; working name, folded GAME 0x604d80
    // occupied so no RVA claimed): reads the staged 0x04 flags word.
    fn act_flags_working_name_reads_flags_word() {
        let state = BaseActorState::construct(base());
        assert_eq!(state.act_flags(), 0);
    }
    #[test]
    // C_actor::SetFromMission word read (mafia-re 113b5b6
    // game/actors/C_actor.h:271-276; working name, setter GAME 0x547220
    // occupied so no RVA claimed): false until the setter marks it.
    fn from_mission_working_name_reads_setter_word() {
        let mut state = BaseActorState::construct(base());
        assert!(!state.from_mission());
        state.set_from_mission();
        assert!(state.from_mission());
    }
    #[test]
    // Base C_actor::GetProgram (mafia-re 113b5b6
    // game/actors/C_actor.h:260-265; working name, no address claimed: the
    // NULL-returning bodies folded into the group kept at GAME 0x409220):
    // the base carries no script.
    fn base_program_working_name_returns_none() {
        let state = BaseActorState::construct(base());
        assert!(state.program().is_none());
    }
    #[test]
    // Working-name C_actor 0x20 net-direct latch (mafia-re 113b5b6
    // game/actors/C_actor.h:344-346; no standalone FUNCTION, read in place
    // by Tick, so no RVA claimed): clear at construction, settable.
    fn net_direct_working_name_round_trips_tick_selector() {
        let mut state = BaseActorState::construct(base());
        assert!(!state.is_net_direct());
        state.set_net_direct(true);
        assert!(state.is_net_direct());
        state.set_net_direct(false);
        assert!(!state.is_net_direct());
    }
    #[test]
    // Working-name C_actor 0x64 frame-actor-set latch (mafia-re 113b5b6
    // game/actors/C_actor.h:360-364; no standalone FUNCTION, read in place
    // by GameDone, so no RVA claimed): clear at construction, settable.
    fn frame_actor_set_working_name_round_trips_pointer_latch() {
        let mut state = BaseActorState::construct(base());
        state.set_frame_actor_set(false);
        assert!(!state.frame_actor_set());
        state.set_frame_actor_set(true);
        assert!(state.frame_actor_set());
    }
    #[test]
    // Working-name C_actor 0x65 unknown latch reader (mafia-re 113b5b6
    // game/actors/C_actor.h:365; no standalone getter, read in place, so no
    // RVA claimed): mirrors the set_unknown65 writer.
    fn unknown65_working_name_reads_writer_byte() {
        let mut state = BaseActorState::construct(base());
        state.set_unknown65(false);
        assert!(!state.is_unknown65());
        state.set_unknown65(true);
        assert!(state.is_unknown65());
    }
    #[test]
    // Working-name C_actor 0x0c reference-count reader (mafia-re 113b5b6
    // game/actors/C_actor.h:196-203; no standalone getter, read in place by
    // Release, so no RVA claimed): construction stages 1, release_one
    // decrements with wrap.
    fn reference_count_working_name_tracks_release_word() {
        let mut state = BaseActorState::construct(base());
        assert_eq!(state.reference_count(), 1);
        assert_eq!(state.release_one(), 0);
        assert_eq!(state.reference_count(), 0);
    }
    #[test]
    // Working-name C_actor 0x18 actor-number reader (mafia-re 113b5b6
    // game/actors/C_actor.h:342; no standalone getter, read in place by
    // SaveGameLoadGuid, so no RVA claimed): stages the seed number.
    fn actor_number_working_name_reads_mission_word() {
        let state = BaseActorState::construct(base());
        assert_eq!(state.actor_number(), 4);
    }
    fn record_key_at(time: i32, pos: [f32; 3], rot: [f32; 4]) -> StagedActorRecord {
        StagedActorRecord {
            time,
            pos,
            rot,
            tangent_out: [0.0, 0.0, 0.0],
            tangent_in: [0.0, 0.0, 0.0],
            ease_from: 0.0,
            ease_to: 0.0,
            rot_ease_from: 0.0,
            rot_ease_to: 0.0,
        }
    }
    #[test]
    // GAME 0x405a00 (RVA 0x5a00) C_actor::GetRecordPos (mafia-re 113b5b6
    // game/actors/C_actor.cpp:849-853): the zero vector without records.
    fn record_pos_rva_5a00_empty_is_zero() {
        assert_eq!(record_pos_at(&[], 200, 150), [0.0, 0.0, 0.0]);
    }
    #[test]
    // GAME 0x405a00 (RVA 0x5a00) C_actor::GetRecordPos (mafia-re 113b5b6
    // game/actors/C_actor.cpp:866-867): at or before the previous key the
    // leaf returns that key's own position.
    fn record_pos_rva_5a00_at_or_before_key_returns_key_pos() {
        let records = [
            record_key_at(0, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [9.0, 9.0, 9.0], [0.0, 0.0, 0.0, 1.0]),
        ];
        assert_eq!(record_pos_at(&records, 100, 0), [1.0, 2.0, 3.0]);
        assert_eq!(record_pos_at(&records, 100, -10), [1.0, 2.0, 3.0]);
    }
    #[test]
    // GAME 0x405a00 (RVA 0x5a00) C_actor::GetRecordPos (mafia-re 113b5b6
    // game/actors/C_actor.cpp:868-882): mid-span Hermite blend with zero
    // eases and zero tangents averages the bracketing positions (f = 0.5
    // eased to s = 0.5, weights 0.5/0.5/0.125/-0.125, all exact in f32).
    // The three-key walk settles the 100..200 bracket for time 150.
    fn record_pos_rva_5a00_midpoint_blend_is_exact() {
        let records = [
            record_key_at(0, [-4.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(200, [8.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]),
        ];
        assert_eq!(record_pos_at(&records, 200, 150), [4.0, 0.0, 0.0]);
    }
    #[test]
    // GAME 0x405a00 (RVA 0x5a00) C_actor::GetRecordPos (mafia-re 113b5b6
    // game/actors/C_actor.cpp:868-873): Ease(0.5, 1.0, 0.0) eases the
    // fraction to exactly 0.25 (k = 1, u < a arm), so the 16-unit span
    // blends to exactly 2.5.
    fn record_pos_rva_5a00_eased_fraction_blend_is_exact() {
        let mut prev = record_key_at(0, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]);
        prev.ease_from = 1.0;
        let mut next = record_key_at(100, [16.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0]);
        next.ease_to = 0.0;
        assert_eq!(record_pos_at(&[prev, next], 100, 50), [2.5, 0.0, 0.0]);
    }
    #[test]
    // GAME 0x405a00 (RVA 0x5a00) C_actor::GetRecordPos (mafia-re 113b5b6
    // game/actors/C_actor.cpp:883): past the last key the leaf returns the
    // next (last) key's position.
    fn record_pos_rva_5a00_past_end_returns_next_pos() {
        let records = [
            record_key_at(0, [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [7.0, 8.0, 9.0], [0.0, 0.0, 0.0, 1.0]),
        ];
        assert_eq!(record_pos_at(&records, 100, 250), [7.0, 8.0, 9.0]);
    }
    #[test]
    // GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1865-1869): the zero quaternion without
    // records.
    fn record_rot_rva_2cc20_empty_is_zero_quat() {
        assert_eq!(record_rot_at(&[], 200, 150), [0.0, 0.0, 0.0, 0.0]);
    }
    #[test]
    // GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1882-1883): at or before the previous key the
    // leaf returns that key's own rotation.
    fn record_rot_rva_2cc20_at_or_before_key_returns_key_rot() {
        let prev_rot = [0.0, 0.0, 0.0, 1.0];
        let records = [
            record_key_at(0, [0.0; 3], prev_rot),
            record_key_at(100, [0.0; 3], [0.0, 0.0, 1.0, 0.0]),
        ];
        assert_eq!(record_rot_at(&records, 100, 0), prev_rot);
        assert_eq!(record_rot_at(&records, 100, -10), prev_rot);
    }
    #[test]
    // GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1884-1891): the end fraction reproduces the
    // next key's rotation exactly (scale0 = sin 0 / sinom = 0, scale1 = 1),
    // and a shared rotation slerps to itself exactly through the linear
    // branch (dot 1).
    fn record_rot_rva_2cc20_endpoints_are_exact() {
        let next_rot = [0.0, 0.0, 0.70710677, 0.70710677];
        let records = [
            record_key_at(0, [0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [0.0; 3], next_rot),
        ];
        assert_eq!(record_rot_at(&records, 100, 100), next_rot);
        let shared = [
            record_key_at(0, [0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [0.0; 3], [0.0, 0.0, 0.0, 1.0]),
        ];
        assert_eq!(record_rot_at(&shared, 100, 50), [0.0, 0.0, 0.0, 1.0]);
    }
    #[test]
    // GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot with LS3DF
    // S_quat::Slerp (mafia-re 113b5b6 ls3df/I3D/I3d_math.cpp:1190-1223):
    // opposite quaternions negate to the same rotation first (the native
    // `shorten` flag never changes that select), so the midpoint slerp is
    // exactly the shared rotation through the linear branch.
    fn record_rot_rva_2cc20_opposite_quats_take_shorter_arc() {
        let records = [
            record_key_at(0, [0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [0.0; 3], [0.0, 0.0, 0.0, -1.0]),
        ];
        assert_eq!(record_rot_at(&records, 100, 50), [0.0, 0.0, 0.0, 1.0]);
    }
    #[test]
    // GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot, spherical branch:
    // halfway between the identity and a quarter turn about y the slerp
    // weights are sin(pi/4)/sin(pi/2), i.e. 0.70710678 up to host
    // transcendental rounding (pinned separately from the exact branch
    // tests above), with exact zeros off-axis.
    fn record_rot_rva_2cc20_quarter_turn_midpoint() {
        let records = [
            record_key_at(0, [0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [0.0; 3], [0.0, 1.0, 0.0, 0.0]),
        ];
        let mid = record_rot_at(&records, 100, 50);
        assert_eq!(mid[0], 0.0);
        assert_eq!(mid[2], 0.0);
        assert!((mid[1] - 0.70710678).abs() < 1e-6);
        assert!((mid[3] - 0.70710678).abs() < 1e-6);
        let norm = (mid[0] * mid[0] + mid[1] * mid[1] + mid[2] * mid[2] + mid[3] * mid[3]).sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }
    #[test]
    // GAME 0x42cc20 (RVA 0x2cc20) C_actor::GetRecordRot (mafia-re 113b5b6
    // game/actors/C_actor.cpp:1892): past the last key the leaf returns the
    // next (last) key's rotation.
    fn record_rot_rva_2cc20_past_end_returns_next_rot() {
        let next_rot = [0.0, 0.0, 1.0, 0.0];
        let records = [
            record_key_at(0, [0.0; 3], [0.0, 0.0, 0.0, 1.0]),
            record_key_at(100, [0.0; 3], next_rot),
        ];
        assert_eq!(record_rot_at(&records, 100, 250), next_rot);
    }
}
