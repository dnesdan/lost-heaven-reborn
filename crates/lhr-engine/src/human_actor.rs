//! Typed owners for the C_human constructor plus copy/persist
//! (DuplicateFrom/SaveData) source-admission domain.
//! Game.exe SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! PC40e010 corresponds to the actual PS2 C_animations_machine_de_luxe
//! constructor through its Reset/StopAnims call pattern, not shared layout.
//! GAME 0x47aed0/0x47b020 (RVA 0x7aed0/0x7b020) construct/reset the embedded
//! G_Inventory aggregate. Rust
//! member names below are descriptive; unknown item/parameter bits have no
//! invented gameplay meaning. Only the Human GameInit prefix slice
//! (car/using_car clear, STAND, max-to-current bank copy), the
//! Intern_UseCar/Intern_FromCar seat-placement core (seat latch plus the
//! car/using_car/action writers of this slice only), the Can_EnterToCar
//! owned-car guard and the Do_StopAnim stop core (STAND plus the alive-gated
//! anim-id reset and PersonAnim handoff) are owned here; the full GameInit
//! body plus animation selection, AI and Update stay DEFERRED, as do every
//! collision, animation-machine, frame, camera, indicator, inventory, weapon
//! and police effect of the three car-function bodies. TestMoveColl's capsule
//! descent (TestPanak2) has no owner; its collision-independent leaves live
//! as a guarded prefix in human_move_coll.
//! The dtor animation-stop device effect (GAME 0x56f210) and the Init frame
//! effects (GAME 0x56f350, owned by ActorWorld::init_human_owner) stay with the
//! frame owner; detector/police owners do not exist yet and are DEFERRED. AI
//! ghost staging only is owned here (ghost_colls @0x698 push/empty plus the
//! next_coll_owners @0x6a8 merge leaf); the C_entity ai_ghost @0x1268 refresh
//! and the pedestrian AI Tick (GAME 0x508150) stay DEFERRED.
#![forbid(unsafe_code)]

use crate::mission_options::OPTION_HIGH;
use crate::original_program::ProgramReader;
use crate::script_scheduler::{ActorReferenceId, FrameReferenceId};

/// The machine constructor leaves its middle DWORD untouched. This explicit
/// allocation seed does not supply a model, owner, Init or Reset defaults.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanAnimationMachineSeed {
    pub untouched_word: u32,
}
#[derive(Debug)]
pub struct HumanAnimationMachine {
    owner: Option<ActorReferenceId>,
    model: Option<FrameReferenceId>,
    untouched_word: u32,
}
impl HumanAnimationMachine {
    pub(crate) fn construct(seed: HumanAnimationMachineSeed) -> Self {
        Self {
            owner: None,
            model: None,
            untouched_word: seed.untouched_word,
        }
    }
    pub fn owner(&self) -> Option<ActorReferenceId> {
        self.owner
    }
    pub fn model(&self) -> Option<FrameReferenceId> {
        self.model
    }
    pub fn untouched_constructor_word(&self) -> u32 {
        self.untouched_word
    }
    /// Actual PC4aabe0 is RET. It does not implicitly execute StopAnims,
    /// GameDone, Reset, reference release or model destruction.
    pub(crate) fn destruct(&mut self) {}
}

/// Actual PS2 S_GameItem correspondence; the PC constructor clears four DWORDs.
/// These are uninterpreted representation bits, not guessed item properties.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HumanInventoryItem {
    bits: [u32; 4],
}
impl HumanInventoryItem {
    pub(crate) fn construct() -> Self {
        Self { bits: [0; 4] }
    }

    /// Provisional weapon record from an ADDWEAPON command, following the
    /// evidenced upstream field order (id, loaded, spare) with a zeroed tail:
    /// S_GameItem carries ushort id plus pad at +0x0, ints at +0x4/+0x8 and a
    /// pointer at +0xc from a memset-0 constructor. The u16 id mirrors the
    /// native `(ushort)` conversion at the command site (padding stays zero).
    /// Slot-placement and capacity rules, ChangeWeaponModel and WeaponChoose
    /// stay open; no shipped MISE01 line reads these records back.
    pub(crate) fn from_weapon_parts(id: u16, loaded: i32, spare: i32) -> Self {
        Self {
            bits: [u32::from(id), loaded as u32, spare as u32, 0],
        }
    }
    pub fn representation_bits(&self) -> &[u32; 4] {
        &self.bits
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanInventorySeed {
    /// An untouched native temporary is copied into the allocator tag.
    pub allocator_tag: u8,
}
/// One genuine empty inventory/vector owner. No API populates these collections
/// before the inventory's gameplay operations have their own closed contracts.
#[derive(Debug)]
pub struct HumanInventory {
    allocator_tag: u8,
    items: Vec<HumanInventoryItem>,
    cycle_state: u32,
    cycle_words: [u32; 2],
    cycle_mask: u32,
    first_parameters: [u32; 4],
    first_slots: [HumanInventoryItem; 5],
    second_parameters: [u32; 4],
    second_slots: [HumanInventoryItem; 4],
    last_parameters: [u32; 4],
    cycle_flag: u8,
}
impl HumanInventory {
    pub(crate) fn construct(seed: HumanInventorySeed) -> Self {
        let mut inventory = Self {
            allocator_tag: seed.allocator_tag,
            items: Vec::new(),
            cycle_state: 0,
            cycle_words: [0; 2],
            cycle_mask: 0,
            first_parameters: [0; 4],
            first_slots: [HumanInventoryItem::construct(); 5],
            second_parameters: [0; 4],
            second_slots: [HumanInventoryItem::construct(); 4],
            last_parameters: [0; 4],
            cycle_flag: 0,
        };
        inventory.reset();
        inventory
    }
    /// PC47b020: reset the aggregate and vector end, preserving allocation and
    /// allocator tag. Construction reaches this with a genuinely empty vector.
    fn reset(&mut self) {
        self.cycle_state = 1;
        self.cycle_words = [0; 2];
        self.cycle_mask = 0x00ff_ffff;
        self.first_parameters = [0; 4];
        self.first_slots = [HumanInventoryItem::construct(); 5];
        self.second_parameters = [0; 4];
        self.second_slots = [HumanInventoryItem::construct(); 4];
        self.last_parameters = [0; 4];
        self.items.clear();
        self.cycle_flag = 0;
    }
    pub fn allocator_tag(&self) -> u8 {
        self.allocator_tag
    }
    pub fn items(&self) -> &[HumanInventoryItem] {
        &self.items
    }
    /// Push a weapon record without slot-placement rules: capacity checks,
    /// slot assignment, ChangeWeaponModel and WeaponChoose stay open.
    pub(crate) fn push_weapon(&mut self, item: HumanInventoryItem) {
        self.items.push(item);
    }
    pub fn cycle_state(&self) -> u32 {
        self.cycle_state
    }
    pub fn cycle_words(&self) -> &[u32; 2] {
        &self.cycle_words
    }
    pub fn cycle_mask(&self) -> u32 {
        self.cycle_mask
    }
    pub fn cycle_flag(&self) -> u8 {
        self.cycle_flag
    }
    pub fn slots(&self) -> (&[HumanInventoryItem; 5], &[HumanInventoryItem; 4]) {
        (&self.first_slots, &self.second_slots)
    }
    pub fn parameter_records(&self) -> [&[u32; 4]; 3] {
        [
            &self.first_parameters,
            &self.second_parameters,
            &self.last_parameters,
        ]
    }
}

/// Original property-name table supplies the first fifteen labels. The
/// sixteenth value remains unnamed; no gameplay meaning is assigned to it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanProperties {
    /// The exact64-byte serialized property record, separate from actor layout.
    bytes: [u8; 64],
}
impl HumanProperties {
    pub fn from_record(bytes: [u8; 64]) -> Self {
        Self { bytes }
    }
    pub fn record(&self) -> &[u8; 64] {
        &self.bytes
    }
    pub fn component_bits(&self) -> [u32; 16] {
        std::array::from_fn(|index| {
            u32::from_le_bytes(self.bytes[index * 4..index * 4 + 4].try_into().unwrap())
        })
    }
    fn constructor() -> Self {
        let bits = [
            0x3f333333u32,
            0x42c80000,
            0x41a00000,
            0x41a00000,
            0x41a00000,
            0x41a00000,
            0x3f333333,
            0x3f800000,
            0x3f19999a,
            0x3f4ccccd,
            0x3f333333,
            0x3f800000,
            0x3f800000,
            0x3f4ccccd,
            0x42a00000,
            0x3f000000,
        ];
        Self {
            bytes: std::array::from_fn(|index| bits[index / 4].to_le_bytes()[index % 4]),
        }
    }
    pub(crate) fn set_component_bits(&mut self, component: usize, bits: u32) {
        self.bytes[component * 4..component * 4 + 4].copy_from_slice(&bits.to_le_bytes());
    }
}

/// An empty constructor-domain resource owner. Its unresolved element type
/// cannot be populated through this API; destruction still reaches free(NULL).
#[derive(Debug)]
pub(crate) struct EmptyHumanCollection {
    pub(crate) allocator_tag: u8,
    elements: Vec<std::convert::Infallible>,
}
impl EmptyHumanCollection {
    pub(crate) fn construct(allocator_tag: u8) -> Self {
        Self {
            allocator_tag,
            elements: Vec::new(),
        }
    }
    pub(crate) fn clear(&mut self) {
        self.elements.clear();
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HumanResourceSlot {
    LastAuxiliary,
    SecondAuxiliary,
    FirstAuxiliary,
    Inventory,
    CustomAnimations,
    Player,
}

/// Only untouched parts of each sparse constructor record are supplied here.
/// No weapon, movement or other gameplay interpretation is invented.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanAuxiliarySeed {
    pub untouched_word: u32,
    pub padding: [u8; 3],
    pub untouched_tail: [u8; 16],
}
#[derive(Debug)]
pub struct HumanAuxiliaryRecord {
    pub(crate) zero_words: [u32; 8],
    pub(crate) zero_byte: u8,
    pub(crate) untouched: HumanAuxiliarySeed,
}
impl HumanAuxiliaryRecord {
    fn construct(seed: HumanAuxiliarySeed) -> Self {
        Self {
            zero_words: [0; 8],
            zero_byte: 0,
            untouched: seed,
        }
    }
    pub fn constructor_words(&self) -> &[u32; 8] {
        &self.zero_words
    }
    pub fn constructor_byte(&self) -> u8 {
        self.zero_byte
    }
    pub fn untouched_seed(&self) -> &HumanAuxiliarySeed {
        &self.untouched
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HumanSeed {
    pub allocator_tag: u8,
    pub animation_machine: HumanAnimationMachineSeed,
    pub previous_animation_code: u32,
    pub requested_animation_code: u32,
    pub human_type_mask: u32,
    pub current_properties: HumanProperties,
    pub target_e: Option<FrameReferenceId>,
    pub target_n: Option<FrameReferenceId>,
    pub base_frame: Option<FrameReferenceId>,
    pub auxiliary: [HumanAuxiliarySeed; 2],
}
/// Original on-foot idle action value (mafia-re 113b5b6
/// game/actors/C_human.h:137 HUMAN_ACTION_STAND; the live word is `int`
/// action_state per C_human.h:824-826, save-packed as ushort per the
/// S_human_savegame 0x6d field, so the Rust owner keeps the PC 32-bit signed
/// width as i32). WALK, FALL and JUMP are owned below by the Movement slice;
/// the remaining HUMAN_ACTION_* values are written by the DEFERRED Update and
/// car AI, so they take no constants here.
pub const HUMAN_ACTION_STAND: i32 = 1;
/// Original walking action value (mafia-re 113b5b6
/// game/actors/C_human.h:138 HUMAN_ACTION_WALK; same PC 32-bit signed word
/// as HUMAN_ACTION_STAND). Read by the Movement dispatch writer of STAND/FALL
/// transitions; the Go_*/Update writers stay DEFERRED.
pub const HUMAN_ACTION_WALK: i32 = 2;
/// Original falling action value (mafia-re 113b5b6 game/actors/C_human.h:139
/// HUMAN_ACTION_FALL; same PC 32-bit signed word). Written by MovementIntern
/// (landing clears it to STAND) and by the Movement fall entry; the landing
/// Hit/PersonAnim/particle/sound effects stay DEFERRED.
pub const HUMAN_ACTION_FALL: i32 = 3;
/// Original jump action value (mafia-re 113b5b6 game/actors/C_human.h:147
/// HUMAN_ACTION_JUMP; same PC 32-bit signed word). Read by MovementIntern
/// (slope projection skipped, landing converts it to STAND); the Do_Jump
/// writer stays DEFERRED.
pub const HUMAN_ACTION_JUMP: i32 = 11;
/// Original seated-in-car action value (mafia-re 113b5b6
/// game/actors/C_human.h:145 HUMAN_ACTION_CAR; same PC 32-bit signed word
/// as HUMAN_ACTION_STAND). Written here only by the Intern_UseCar seat core;
/// the Movement and car-function writers of the remaining HUMAN_ACTION_*
/// values stay DEFERRED.
pub const HUMAN_ACTION_CAR: i32 = 9;
/// No-seat sentinel for the seat latch (descriptive working name: the
/// upstream field name `seat` is itself a guess per mafia-re 113b5b6
/// game/actors/C_human.h:718-719). Evidenced by the Intern_FromCar guard
/// `seat == -1` (C_human.cpp:5658), the truck-back `seat = -1`
/// (C_human.cpp:2242) and the `seat != -1` in-car tests throughout
/// C_human.cpp; the GameInit body (:3162-3407) writes no seat word.
pub const HUMAN_SEAT_NONE: i32 = -1;
/// The idle breath animation id (mafia-re 113b5b6 game/actors/C_actor.h:548:
/// 0x1, "breath01a.i3d"). The numeric id is original; the symbolic name
/// follows upstream, derived from the animation file name. Written by the
/// Do_StopAnim core of this slice; the PersonAnim stance/anim-set remap of
/// the id stays DEFERRED to the PersonAnim owner.
pub const ANIM_BREATH01A: i32 = 1;
/// Bloody-footprint track texture index (mafia-re 113b5b6
/// game/mission/game.h:92-94: TRACK_TEXTURE_BFOOTPRINT 6, the
/// C_game::track_textures index of "2bfootprint.bmp", given to the steps of
/// a human through blood by cbCollTester).
pub const TRACK_TEXTURE_BFOOTPRINT: u8 = 6;

/// Opaque identity of one dynamic collision object (`tDynamicCollObject *`).
/// Working name and representation (NOT original): the native value is a host
/// pointer, which never crosses this boundary. Only opaque identity flows here;
/// the collision manager owns exclusion. Zero is a valid test identity, never
/// a null pointer on this path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanDynCollId(pub u32);

impl HumanDynCollId {
    /// Wrap an opaque test/capture identity without host-address meaning.
    pub const fn from_raw(raw: u32) -> Self {
        Self(raw)
    }

    /// The opaque identity back for capture comparison only.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

/// GAME 0x52f920 (RVA 0x12f920): `C_entity` publishes one AI-ghost collision
/// into `C_human::ghost_colls` (0x698) whenever the ghosts change (mafia-re
/// 113b5b6 game/actors/C_human.h:970-973). Body DEFERRED to the entity owner;
/// [`HumanState::push_ghost_coll`] is the typed staging leaf only.
pub const GHOST_COLLS_PUSH_VA: u32 = 0x52f920;
/// GAME 0x507b50 (RVA 0x107b50): helper emptying `C_human::ghost_colls`
/// (0x698), called from `C_entity::GameInit` (mafia-re 113b5b6
/// game/actors/C_human.h:970-973). [`HumanState::empty_ghost_colls`] owns the
/// emptying leaf; the entity `GameInit` body stays DEFERRED.
pub const GHOST_COLLS_EMPTY_VA: u32 = 0x507b50;
/// GAME 0x57cfe0 (RVA 0x17cfe0): `C_human::DynSetNextCollOwnerMult` inline
/// (mafia-re 113b5b6 game/actors/C_human.cpp:5308-5322). The merge itself is
/// owned here via [`HumanState::rebuild_next_coll_owners`]; the
/// `g_collision::DynSetNextCollOwnerMult` call stays with the collision
/// manager (DEFERRED).
pub const DYN_SET_NEXT_COLL_OWNER_MULT_RVA: u32 = 0x17cfe0;
/// Pending PersonAnim invocation recorded by the Do_StopAnim core (descriptive
/// working name and field names; the original call is synchronous with no
/// latch). The future PersonAnim owner consumes it via
/// take_person_anim_request; no frame or animation-machine effect happens here.
/// Parameter meanings per mafia-re 113b5b6 game/actors/C_human.h:185 (first
/// bool: stance, in the weapon's and the crouch's stance) and :172-174
/// (second bool: once, the animation plays once, not looped, on to
/// C_animations_machine_de_luxe::SetAnim); the float is the blend passed on
/// to SetAnim (C_human.cpp:2965-2971).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HumanPersonAnimRequest {
    pub blend: f32,
    pub stance: bool,
    pub play_once: bool,
}

#[derive(Debug)]
pub struct HumanState {
    pub(crate) previous_animation_code: u32,
    pub(crate) requested_animation_code: u32,
    pub(crate) human_type_mask: u32,
    pub(crate) voice_index: u32,
    pub(crate) properties: HumanProperties,
    pub(crate) current_properties: HumanProperties,
    pub(crate) target_e: Option<FrameReferenceId>,
    pub(crate) target_n: Option<FrameReferenceId>,
    pub(crate) base_frame: Option<FrameReferenceId>,
    pub(crate) animation_machine: HumanAnimationMachine,
    pub(crate) inventory: HumanInventory,
    pub(crate) custom_animations: EmptyHumanCollection,
    pub(crate) first_auxiliary: EmptyHumanCollection,
    pub(crate) second_auxiliary: EmptyHumanCollection,
    pub(crate) last_auxiliary: EmptyHumanCollection,
    pub(crate) auxiliary: [HumanAuxiliaryRecord; 2],
    /// Script-visible car latch for HUMAN_GETOWNER (mafia-re 113b5b6
    /// game/actors/C_human.h:1120 GetCar GAME 0x47b7b0). Populated by the
    /// Intern_UseCar seat core and cleared by the Intern_FromCar placement
    /// core of this slice; the Use_Actor/Movement enter-car AI (seat/
    /// action_state gating per game/mission/C_program.cpp:4325) stays
    /// DEFERRED. Starts at no car.
    /// No overlay entry; rides Process 0x6d3b0.
    car: Option<ActorReferenceId>,
    /// Script-visible car the human is getting into or was thrown out of
    /// (mafia-re 113b5b6 game/actors/C_human.h:707-713: 0x9c C_car *using_car;
    /// the name is ours per the upstream comment). Cleared by the
    /// Intern_FromCar placement core of this slice; the Use_Actor writer
    /// that populates it stays DEFERRED. Starts at no car and the GameInit
    /// prefix re-clears it (C_human.cpp:3175). No invented boarding
    /// semantics beyond the latch.
    using_car: Option<ActorReferenceId>,
    /// Live HUMAN_ACTION_* word (mafia-re 113b5b6 game/actors/C_human.h:824
    /// -826: 0x40c int action_state; the name is ours). STAND is produced by
    /// the GameInit prefix, STAND and CAR by the FromCar/UseCar seat cores
    /// of this slice; the Movement and remaining car-function writers of the
    /// other values stay DEFERRED, so no meaning beyond STAND/CAR is
    /// assigned. Starts at STAND; the GameInit prefix re-affirms it
    /// (C_human.cpp:3176).
    action_state: i32,
    /// Seat index in the car (mafia-re 113b5b6 game/actors/C_human.h:718-719:
    /// 0xac, PS2 0x238; the field name is itself an upstream guess),
    /// HUMAN_SEAT_NONE for none. Written by the Intern_UseCar seat core and
    /// read by the Intern_FromCar and enter guards; notably NOT cleared by
    /// Intern_FromCar (C_human.cpp:5656-5717 carries no seat write). Starts
    /// at none; the GameInit body (:3162-3407) writes no seat word, so the
    /// GameInit prefix leaves it alone. No overlay entry of its own; rides
    /// the 0x17e020/0x17e500/0x1808c0 seat-placement entries.
    seat: i32,
    /// Liveness word (mafia-re 113b5b6 game/actors/C_actor.h:355-358: 0x5d
    /// bool alive, true from GameInit till the end; the name is ours). Read
    /// by the Do_StopAnim core of this slice; the GameInit writer that sets
    /// it and the death writer that clears it (C_human.cpp:2326) stay
    /// DEFERRED. Starts at not alive (working deterministic default for the
    /// pre-GameInit seed; the native C_actor ctor leaves it true).
    /// No overlay entry of its own, rides the 0x17d2e0 stop entry.
    alive: bool,
    /// Current animation id (mafia-re 113b5b6 game/actors/C_human.h:690:
    /// 0x70 int anim_id, PS2 0x200; the name is ours). Written by the
    /// Do_StopAnim core of this slice; the PersonAnim/Update/Movement writers
    /// stay DEFERRED. Starts at the cleared value 0 (the constructor leaves
    /// the word untouched; 0 is what Do_StopAnim and SaveGameLoad write).
    /// No overlay entry of its own; rides the 0x17d2e0 stop entry.
    anim_id: i32,
    /// Next animation id PersonAnim plays (mafia-re 113b5b6
    /// game/actors/C_human.h:691-692: 0x74 int next_anim_id, PS2 0x204; the
    /// name is ours). Written by the Do_StopAnim core of this slice; the
    /// PersonAnim/Update/Movement/GameInit writers stay DEFERRED. Starts at
    /// 0 (no selection yet). No overlay entry of its own; rides the
    /// 0x17d2e0 stop entry.
    next_anim_id: i32,
    /// Pending PersonAnim handoff for the future animation owner (see
    /// HumanPersonAnimRequest). The Do_StopAnim core records
    /// PersonAnim(0.0, true, false) here on the alive path only; the dead
    /// path leaves a previous request untouched, exactly matching the
    /// original's call placement (C_human.cpp:5362-5371).
    person_anim_request: Option<HumanPersonAnimRequest>,
    /// Marker-only staging of C_human::ghost_colls @0x698 (mafia-re 113b5b6
    /// game/actors/C_human.h:970-973): the AI ghosts' collisions the move
    /// tests leave out besides colls. Filled by push_ghost_coll (GAME 0x52f920
    /// push) and emptied by empty_ghost_colls (GAME 0x507b50 empty, called from
    /// C_entity::GameInit whose body stays DEFERRED). Pedestrian C_entity AI
    /// Tick (GAME 0x508150) that refreshes the source ai_ghost vector stays
    /// DEFERRED; this is staging only, never refreshed here.
    ghost_colls: Vec<HumanDynCollId>,
    /// Marker-only staging of C_human::next_coll_owners @0x6a8 (mafia-re
    /// 113b5b6 game/actors/C_human.h:974-976): the merged list
    /// g_collision::DynSetNextCollOwnerMult takes (colls plus ghost_colls).
    /// Rebuilt by rebuild_next_coll_owners (GAME 0x57cfe0); the collision call
    /// itself stays with the collision manager (DEFERRED).
    next_coll_owners: Vec<HumanDynCollId>,
    /// The steps that leave a track, the count they started from (which fades
    /// them) and the track texture index (mafia-re 113b5b6
    /// game/actors/C_human.h:1000-1005: track_steps_max @0x6ec,
    /// track_steps @0x6ee, track_texture @0x6f0). Written by cb_coll_tester
    /// (GAME 0x587d30) of this slice; the GameInit/constructor writers that
    /// zero track_steps (C_human.cpp:3200, :9079) and the track-tick consumer
    /// (:821-840) stay DEFERRED.
    track_steps_max: i16,
    track_steps: i16,
    track_texture: u8,
}
impl HumanState {
    pub(crate) fn construct(seed: HumanSeed) -> Self {
        Self {
            previous_animation_code: seed.previous_animation_code,
            requested_animation_code: seed.requested_animation_code,
            human_type_mask: seed.human_type_mask,
            voice_index: 12,
            properties: HumanProperties::constructor(),
            current_properties: seed.current_properties,
            target_e: seed.target_e,
            target_n: seed.target_n,
            base_frame: seed.base_frame,
            animation_machine: HumanAnimationMachine::construct(seed.animation_machine),
            inventory: HumanInventory::construct(HumanInventorySeed {
                allocator_tag: seed.allocator_tag,
            }),
            custom_animations: EmptyHumanCollection::construct(seed.allocator_tag),
            first_auxiliary: EmptyHumanCollection::construct(seed.allocator_tag),
            second_auxiliary: EmptyHumanCollection::construct(seed.allocator_tag),
            last_auxiliary: EmptyHumanCollection::construct(seed.allocator_tag),
            auxiliary: seed.auxiliary.map(HumanAuxiliaryRecord::construct),
            car: None,
            using_car: None,
            action_state: HUMAN_ACTION_STAND,
            seat: HUMAN_SEAT_NONE,
            alive: false,
            anim_id: 0,
            next_anim_id: 0,
            person_anim_request: None,
            ghost_colls: Vec::new(),
            next_coll_owners: Vec::new(),
            track_steps_max: 0,
            track_steps: 0,
            track_texture: 0,
        }
    }
    pub fn human_type_mask(&self) -> u32 {
        self.human_type_mask
    }
    pub fn voice_index(&self) -> u32 {
        self.voice_index
    }
    /// Serialized maximum-property bank. The GameInit prefix copies it over
    /// the current bank (apply_game_init_prefix, C_human.cpp:3180).
    pub fn properties(&self) -> &HumanProperties {
        &self.properties
    }
    pub fn current_properties(&self) -> &HumanProperties {
        &self.current_properties
    }
    pub fn target_e(&self) -> Option<FrameReferenceId> {
        self.target_e
    }
    pub fn target_n(&self) -> Option<FrameReferenceId> {
        self.target_n
    }
    pub fn base_frame(&self) -> Option<FrameReferenceId> {
        self.base_frame
    }
    pub fn animation_machine(&self) -> &HumanAnimationMachine {
        &self.animation_machine
    }
    pub fn inventory(&self) -> &HumanInventory {
        &self.inventory
    }
    pub fn previous_animation_code(&self) -> u32 {
        self.previous_animation_code
    }
    pub fn requested_animation_code(&self) -> u32 {
        self.requested_animation_code
    }
    pub fn auxiliary_records(&self) -> &[HumanAuxiliaryRecord; 2] {
        &self.auxiliary
    }
    /// Script-visible car for HUMAN_GETOWNER reads.
    pub fn car(&self) -> Option<ActorReferenceId> {
        self.car
    }
    /// Test/host setter for the enter-car latch (AI population deferred).
    pub fn set_car(&mut self, car: Option<ActorReferenceId>) {
        self.car = car;
    }
    /// Script-visible using-car latch for the car-function door notes.
    pub fn using_car(&self) -> Option<ActorReferenceId> {
        self.using_car
    }
    /// Test/host setter for the using-car latch (car-function writers deferred).
    pub fn set_using_car(&mut self, car: Option<ActorReferenceId>) {
        self.using_car = car;
    }
    /// Test/host setter for the liveness word (GameInit/death writers deferred).
    pub fn set_alive(&mut self, alive: bool) {
        self.alive = alive;
    }
    /// Live HUMAN_ACTION_* word; STAND and CAR are produced by this owner so far.
    pub fn action_state(&self) -> i32 {
        self.action_state
    }
    /// Liveness word (C_actor 0x5d); the Do_StopAnim core reads it.
    pub fn alive(&self) -> bool {
        self.alive
    }
    /// Current animation id; the Do_StopAnim core clears it on the alive path.
    pub fn anim_id(&self) -> i32 {
        self.anim_id
    }
    /// Next animation id; the Do_StopAnim core selects the breath on the alive path.
    pub fn next_anim_id(&self) -> i32 {
        self.next_anim_id
    }
    /// Pending PersonAnim handoff for the future animation owner, if any.
    pub fn person_anim_request(&self) -> Option<HumanPersonAnimRequest> {
        self.person_anim_request
    }
    /// Take the pending PersonAnim handoff (future PersonAnim owner consumes it).
    pub(crate) fn take_person_anim_request(&mut self) -> Option<HumanPersonAnimRequest> {
        self.person_anim_request.take()
    }
    /// Seat index in the car, HUMAN_SEAT_NONE for none.
    pub fn seat(&self) -> i32 {
        self.seat
    }
    pub fn collection_is_empty(&self, slot: HumanResourceSlot) -> bool {
        match slot {
            HumanResourceSlot::LastAuxiliary => self.last_auxiliary.is_empty(),
            HumanResourceSlot::SecondAuxiliary => self.second_auxiliary.is_empty(),
            HumanResourceSlot::FirstAuxiliary => self.first_auxiliary.is_empty(),
            HumanResourceSlot::Inventory => self.inventory.items.is_empty(),
            HumanResourceSlot::CustomAnimations => self.custom_animations.is_empty(),
            HumanResourceSlot::Player => false,
        }
    }
    pub(crate) fn load_source_data(
        &mut self,
        reader: &mut ActorDataReader<'_>,
    ) -> Result<(), HumanDataError> {
        let mut version = [0];
        if !reader.read_bounded(&mut version) {
            return Err(HumanDataError::MissingVersion);
        }
        match version[0] {
            1..=4 => Err(HumanDataError::UnsupportedConversion(version[0])),
            5 | 6 => {
                reader.read_word_bounded(&mut self.human_type_mask);
                if version[0] == 6 {
                    reader.read_word_bounded(&mut self.voice_index);
                }
                reader.read_bounded(&mut self.properties.bytes);
                Ok(())
            }
            _ => Ok(()),
        }
    }
    /// GAME 0x56f400 (RVA 0x16f400) C_human::DuplicateFrom (mafia-re
    /// game/actors/C_human.cpp:1303): copy the maximum-property bank and the
    /// human type word. The C_actor::DuplicateFrom prefix is the linker's
    /// folded empty no-op (mafia-re game/actors/C_actor.cpp:925), so there is
    /// no base state to forward. The original casts its C_actor source to
    /// C_human without a type check; the typed source reference carries that
    /// caller precondition. Current properties, voice, frame targets,
    /// inventory and auxiliary records are preserved.
    pub(crate) fn duplicate_from(&mut self, source: &HumanState) {
        self.properties = source.properties.clone();
        self.human_type_mask = source.human_type_mask;
    }
    /// GAME 0x574bd0 (RVA 0x174bd0) C_human::GameInit PREFIX SLICE (mafia-re
    /// 113b5b6 game/actors/C_human.cpp, non-contiguous statements :3174-3176
    /// plus :3180; interleaved :3177-3179 deferred): clear the car and
    /// using_car latches to NULL, set action_state to HUMAN_ACTION_STAND
    /// (C_human.h:137), and copy the maximum-property bank over the current
    /// bank (memcpy(properties, max_properties, sizeof(properties))). SCOPE:
    /// prefix only. The rest of the :3162-3407 body stays DEFERRED with no new
    /// owner here: orig_model and the C_actor::GameInit prefix, base_frame
    /// lookup plus frame refcount/DeleteFrame, DebustizeFrame and
    /// NoShadowFrame, vehicle_action, anim_machine.Init plus next_anim_id, the
    /// anim/velocity/turn/fall/guard/idle/breath/combat/dead/track/move/hit/
    /// reload/voice/death/car-use/shot words (incl. stealing_car, last_car,
    /// locked_car, vehicle_action, car_stance), pose creation and SetPose, sound
    /// frame creation, shot_skeleton plus notify callback, anims reset, shadow
    /// registration, weapon models and hand targets, fire words, inventory
    /// reset plus ChangeWeaponModel, PoseValidate, ground snapping,
    /// LightnessAdd, and the loc/blob/cutscene/tick words. AI/Update stay
    /// DEFERRED.
    pub(crate) fn apply_game_init_prefix(&mut self) {
        self.car = None;
        self.using_car = None;
        self.action_state = HUMAN_ACTION_STAND;
        self.current_properties = self.properties.clone();
    }
    /// GAME 0x57e020 (RVA 0x17e020) C_human::Intern_UseCar SEAT-PLACEMENT
    /// CORE SLICE (mafia-re 113b5b6 game/actors/C_human.cpp:5568-5655):
    /// the :5570 guard's seat-range arm (`c->GetNumSeats() <= s`) plus the
    /// owned state writes :5589 (`car = c`), :5601 (`seat = s`) and
    /// :5610 (`action_state = HUMAN_ACTION_CAR`). `num_seats` is the car's
    /// `GetNumSeats()` value (native `int` on both sides, so i32 here).
    /// Returns true on admission; on refusal the state is untouched and
    /// false is returned (the original is void with an early return; the
    /// bool is the admission signal, not new state). Negative seats refuse:
    /// the original indexes `v_seats[s]` with the raw int, so admitting a
    /// sub-zero seat would read out of bounds; the bound is a defensive
    /// refusal, not original behavior. SCOPE: seat-placement core only.
    /// DEFERRED with explicit markers, no new owner here: the :5570
    /// `box || dead_body` guard arms (no box/dead-body owners exist; callers
    /// must establish them as a precondition), `SetOwner` (car owner),
    /// `track_test = 0` and `unknown_b0 = 0` (unowned human words),
    /// `last_car = c` (no owner yet; NOT written here),
    /// `PlayerInTaxi`/`VyfunOdTeMasiny` (traffic owner), `DelExtendedAnim`/
    /// `PersonAnim`/`Tick` and the `next_anim_id` select (animation owner;
    /// the select itself is owned by `get_anim_id_for_use_car` plus
    /// `CarInitAnimationCatalog`), `Do_Aimed`/`PoseNullRotate`/
    /// `PoseInterpolate` (pose/aim owners), `DeactivateColls` (collision
    /// owner), `SetEngineOn`/`SetSpeedLimit`/`auto_gearing` (car owner),
    /// `DelFrameForShadow`/`shadow` (frame owner), `pos`/`dir` plus
    /// `SetPos`/`SetDir`/`LinkTo` (frame owner), camera/indicators/
    /// player-alpha/lights/dynamic-colls (no owners), `SetInCar` (inventory
    /// owner), `ChangeWeaponModel`/weapon models (weapon owner),
    /// `shvestky.DriveCar` (police owner) and `HideLegs`. `using_car` is
    /// deliberately untouched: the body writes no such word on this path.
    pub(crate) fn apply_intern_use_car(
        &mut self,
        car: ActorReferenceId,
        seat: i32,
        num_seats: i32,
    ) -> bool {
        if seat < 0 || num_seats <= seat {
            return false;
        }
        self.car = Some(car);
        self.seat = seat;
        self.action_state = HUMAN_ACTION_CAR;
        true
    }
    /// GAME 0x57e500 (RVA 0x17e500) C_human::Intern_FromCar PLACEMENT CORE
    /// SLICE (mafia-re 113b5b6 game/actors/C_human.cpp:5656-5717): the :5658
    /// guard (`!frm || !car || car->type != ACTOR_CAR || seat == -1`) plus
    /// the owned state writes :5668 (`action_state = HUMAN_ACTION_STAND`),
    /// :5674 (`car = NULL`) and :5675 (`using_car = NULL`).
    /// `destination_present` is the `frm` null check and `car_is_car_actor`
    /// the `car->type == ACTOR_CAR` check; both live with owners outside
    /// this slice, so the caller supplies them. Returns true on admission;
    /// on refusal the state is untouched and false is returned (the original
    /// is void with an early return). The seat latch is deliberately NOT
    /// cleared: the :5656-5717 body carries no seat write, so the index is
    /// retained while the car latches drop. SCOPE: placement core only.
    /// DEFERRED with explicit markers, no new owner here: `PlayerInTaxi`
    /// (traffic owner), `Do_Aimed`/free-look (aim/camera owners),
    /// `SetEngineOn`/`DriveCar` (car/police owners), `pos`/`dir`/frame
    /// `SetPos`/`SetDir`/`LinkTo`/`SetFrameSectorPos` (frame owner),
    /// indicators/camera (no owners), `SetOwner(NULL, ...)` (car owner),
    /// `ActivateColls`/shadow (collision/frame owners), `SetInCar`/`dropped`
    /// /`DropOutItems` (inventory owner), `ChangeWeaponModel`/weapon models
    /// (weapon owner), `StopAnims`/`Reset`/`PersonAnim` (animation owner)
    /// and `HideLegs`.
    pub(crate) fn apply_intern_from_car(
        &mut self,
        destination_present: bool,
        car_is_car_actor: bool,
    ) -> bool {
        if !destination_present
            || self.car.is_none()
            || !car_is_car_actor
            || self.seat == HUMAN_SEAT_NONE
        {
            return false;
        }
        self.action_state = HUMAN_ACTION_STAND;
        self.car = None;
        self.using_car = None;
        true
    }
    /// GAME 0x57d2e0 (RVA 0x17d2e0) C_human::Do_StopAnim STOP CORE (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:5362, ~12 lines): set action_state to
    /// HUMAN_ACTION_STAND (C_human.h:137); when alive, clear anim_id to 0,
    /// select next_anim_id ANIM_BREATH01A and invoke
    /// PersonAnim(0.0, true, false). The PersonAnim invocation is recorded as
    /// a HumanPersonAnimRequest handoff for the future animation owner; the
    /// frame/animation-machine effects inside the PersonAnim body (stance and
    /// anim-set remap, breath guards, SetAnim) stay DEFERRED with no invented
    /// behavior. The dead path writes only action_state, leaving the anim
    /// words and any pending request untouched, exactly as the original's
    /// `if (alive)` placement. SCOPE: stop core only. DEFERRED with explicit
    /// markers, no new owner here: the TestMoveColl move_coll word (0x414),
    /// crouching/weapon/aimed reads inside PersonAnim, and every Movement,
    /// Update and car-function writer of these words.
    pub(crate) fn apply_do_stop_anim(&mut self) {
        self.action_state = HUMAN_ACTION_STAND;
        if self.alive {
            self.anim_id = 0;
            self.next_anim_id = ANIM_BREATH01A;
            self.person_anim_request = Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            });
        }
    }
    /// GAME 0x5808c0 (RVA 0x1808c0) C_human::Can_EnterToCar OWNED-CAR GUARD
    /// MARKER (mafia-re 113b5b6 game/actors/C_human.cpp:6373, ~54 lines):
    /// only the `if (car) return false;` arm is owned here; a human latched
    /// to a car cannot enter another. True admits to the deferred probes,
    /// false refuses. CONTRACT CEILING: no collision, scene or door owner
    /// exists for the remainder, so the rest stays DEFERRED with explicit
    /// markers and no invented geometry: `GetSeatProperty` (owned as
    /// `car_get_seat_property`; wiring deferred), the 1.3 m side offset and
    /// the ground `TestLineV`/room `TestLineH`/`TestSphereB` probes plus the
    /// `DynSetNextCollOwnerMult` plumbing (collision owner), and the
    /// `test_line` `TestColHierarchy` probe (scene owner).
    pub(crate) fn can_enter_to_car_guard(&self) -> bool {
        self.car.is_none()
    }
    /// Staged `C_human::ghost_colls` @0x698 view (mafia-re 113b5b6
    /// game/actors/C_human.h:970-973). Empty at construction; filled from the
    /// `C_entity::ai_ghost` vector (whose Tick refresh stays DEFERRED as
    /// pedestrian AI, GAME 0x508150) and consumed only by
    /// [`HumanState::rebuild_next_coll_owners`].
    pub fn ghost_colls(&self) -> &[HumanDynCollId] {
        &self.ghost_colls
    }
    /// Staged `C_human::next_coll_owners` @0x6a8 view (mafia-re 113b5b6
    /// game/actors/C_human.h:974-976). Empty at construction; rebuilt around
    /// each `TestMoveColl` sweep.
    pub fn next_coll_owners(&self) -> &[HumanDynCollId] {
        &self.next_coll_owners
    }
    /// GAME 0x52f920 (RVA 0x12f920) `ghost_colls` push leaf (mafia-re 113b5b6
    /// game/actors/C_human.h:970-973: `C_entity` puts the ghosts' collisions
    /// here whenever the ghosts change). Marker-only staging: appends one
    /// opaque collision identity. The ghost-vector refresh, the actor/time/
    /// radius bookkeeping (`C_entity::ai_ghost` @0x1268, `ai_ghost_actor`
    /// items) and the pedestrian `C_entity::AI` Tick (GAME 0x508150) stay
    /// DEFERRED with the entity owner.
    pub fn push_ghost_coll(&mut self, coll: HumanDynCollId) {
        self.ghost_colls.push(coll);
    }
    /// GAME 0x507b50 (RVA 0x107b50) `ghost_colls` empty leaf (mafia-re 113b5b6
    /// game/actors/C_human.h:970-973: `C_entity::GameInit` and 0x507b50 empty
    /// it). Marker-only lifecycle: clears the staged vector. The surrounding
    /// `C_entity::GameInit` body (GAME 0x5074d0, `C_human::GameInit` prefix
    /// included) stays DEFERRED; callers threading `GameInit` invoke this to
    /// keep the staging empty.
    pub fn empty_ghost_colls(&mut self) {
        self.ghost_colls.clear();
    }
    /// GAME 0x57cfe0 (RVA 0x17cfe0) `C_human::DynSetNextCollOwnerMult` merge
    /// leaf (mafia-re 113b5b6 game/actors/C_human.cpp:5308-5322, called at
    /// :5048 for the `MovementIntern` ground probe and at :5296 for the
    /// `TestMoveColl` sweep; :5342 `TestMoveCollInOwner` passes `colls` only).
    /// With staged ghosts, clears and refills `next_coll_owners` with `colls`
    /// reversed then `ghost_colls` reversed, exactly matching the original's
    /// two reverse `push_back` loops; without ghosts the original passes
    /// `colls` straight through, so the staging is cleared and left empty as
    /// the marker for "pass `colls` directly". `colls` arrives from the
    /// caller because the `colls`/`num_colls` owner (@0x688/0x690) does not
    /// exist here yet. The `g_collision::DynSetNextCollOwnerMult` call itself
    /// stays with the collision manager (DEFERRED). `is_player` does not enter
    /// this merge; it selects `flags2` in `human_move_coll::test_move_coll_prefix`
    /// (C_human.cpp:5293-5295).
    pub fn rebuild_next_coll_owners(&mut self, colls: &[HumanDynCollId]) {
        self.next_coll_owners.clear();
        if self.ghost_colls.is_empty() {
            return;
        }
        self.next_coll_owners.reserve(colls.len() + self.ghost_colls.len());
        for coll in colls.iter().rev() {
            self.next_coll_owners.push(*coll);
        }
        for ghost in self.ghost_colls.iter().rev() {
            self.next_coll_owners.push(*ghost);
        }
    }
    /// Remaining bloody-track steps (mafia-re 113b5b6
    /// game/actors/C_human.h:1003: track_steps @0x6ee).
    pub fn track_steps(&self) -> i16 {
        self.track_steps
    }
    /// Step count the current track started from, fading it (mafia-re 113b5b6
    /// game/actors/C_human.h:1002: track_steps_max @0x6ec).
    pub fn track_steps_max(&self) -> i16 {
        self.track_steps_max
    }
    /// Track texture index (mafia-re 113b5b6 game/actors/C_human.h:1004:
    /// track_texture @0x6f0).
    pub fn track_texture(&self) -> u8 {
        self.track_texture
    }
    /// GAME 0x587d30 (RVA 0x187d30) C_human::cbCollTester marker slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:8167-8181): while game
    /// effects are HIGH or above, restart bloody-footprint tracking (10 steps
    /// of TRACK_TEXTURE_BFOOTPRINT) when no steps remain, then publish the
    /// live count as the fade maximum. The collided object never enters this
    /// slice; collision ownership stays DEFERRED. Animal bodies, the
    /// track-tick consumer (C_human.cpp:821-840), ThrowCocot/TryGoToAssHole
    /// AI/nav and the police effects stay DEFERRED with their owners.
    pub fn cb_coll_tester(&mut self, game_effects: i32) {
        if game_effects >= OPTION_HIGH {
            let mut steps = self.track_steps as u16;
            if steps == 0 {
                steps = 10;
                self.track_texture = TRACK_TEXTURE_BFOOTPRINT;
                self.track_steps = steps as i16;
            }
            self.track_steps_max = steps as i16;
        }
    }
    /// GAME 0x56f440 (RVA 0x16f440) C_human::SaveData (mafia-re
    /// game/actors/C_human.cpp:1312): version byte HUMAN_DATA_VERSION
    /// (6, game/actors/C_human.h:131), human type word, voice word, then the
    /// 64-byte maximum-property bank; 73 bytes total. The C_actor::SaveData
    /// prefix writes nothing and returns true (mafia-re
    /// game/actors/C_actor.cpp:1896). Voice is native `int`
    /// (game/actors/C_human.h:961) written as four bytes with no conversion,
    /// so the bit-preserving u32 encoding is exact. Pairs with
    /// load_source_data's version-6 path. The destructor's StopAnimation(0)
    /// device effect (GAME 0x56f210, C_human.cpp:1275) stays with the frame
    /// owner (ActorWorld/ModelArena): DEFERRED, no new owner filed here.
    pub(crate) fn encode_source_data(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(73);
        bytes.push(6);
        bytes.extend_from_slice(&self.human_type_mask.to_le_bytes());
        bytes.extend_from_slice(&self.voice_index.to_le_bytes());
        bytes.extend_from_slice(self.properties.record());
        bytes
    }
    pub(crate) fn clear_resource(&mut self, slot: HumanResourceSlot) {
        match slot {
            HumanResourceSlot::LastAuxiliary => self.last_auxiliary.clear(),
            HumanResourceSlot::SecondAuxiliary => self.second_auxiliary.clear(),
            HumanResourceSlot::FirstAuxiliary => self.first_auxiliary.clear(),
            HumanResourceSlot::Inventory => self.inventory.items.clear(),
            HumanResourceSlot::CustomAnimations => self.custom_animations.clear(),
            HumanResourceSlot::Player => unreachable!("derived player owner"),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum HumanDataError {
    MissingVersion,
    UnsupportedConversion(u8),
}
/// Callback-free read-mode C_chunk boundary over an already selected payload.
/// Bounded reads skip an oversized whole field without advancing or mutation;
/// later CProgram reads use the same cursor with its strict source contract.
pub struct ActorDataReader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> ActorDataReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub fn position(&self) -> usize {
        self.position
    }
    pub fn read_bounded(&mut self, destination: &mut [u8]) -> bool {
        let Some(end) = self.position.checked_add(destination.len()) else {
            return false;
        };
        let Some(bytes) = self.bytes.get(self.position..end) else {
            return false;
        };
        destination.copy_from_slice(bytes);
        self.position = end;
        true
    }
    fn read_word_bounded(&mut self, destination: &mut u32) {
        let mut bytes = destination.to_le_bytes();
        if self.read_bounded(&mut bytes) {
            *destination = u32::from_le_bytes(bytes);
        }
    }
}
impl ProgramReader for ActorDataReader<'_> {
    fn read_exact(&mut self, destination: &mut [u8]) -> Result<(), String> {
        if self.read_bounded(destination) {
            Ok(())
        } else {
            Err("truncated actor program chunk".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn seed() -> HumanSeed {
        HumanSeed {
            allocator_tag: 0xd7,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xa5a5_a5a5,
            },
            previous_animation_code: 0x1122_3344,
            requested_animation_code: 0x5566_7788,
            human_type_mask: 0xa5a5_a5a5,
            current_properties: HumanProperties::from_record([0xa5; 64]),
            target_e: None,
            target_n: None,
            base_frame: None,
            auxiliary: std::array::from_fn(|_| HumanAuxiliarySeed {
                untouched_word: 0xa5a5_a5a5,
                padding: [0xa5; 3],
                untouched_tail: [0xa5; 16],
            }),
        }
    }
    #[test]
    // Predates the GameInit prefix (construct now also sets the car latches
    // to None and action_state to STAND); asserts the untouched remainder.
    // Overlap with the prefix defaults is covered by the prefix test below.
    fn human_constructor_defaults_do_not_overwrite_unwritten_seed_or_run_game_init() {
        let human = HumanState::construct(seed());
        assert_eq!(human.human_type_mask(), 0xa5a5_a5a5);
        assert_eq!(human.voice_index(), 12);
        assert_eq!(human.previous_animation_code(), 0x1122_3344);
        assert_eq!(human.requested_animation_code(), 0x5566_7788);
        assert_eq!(human.current_properties().record(), &[0xa5; 64]);
        assert_eq!(
            human.properties().component_bits(),
            [
                0x3f333333, 0x42c80000, 0x41a00000, 0x41a00000, 0x41a00000, 0x41a00000, 0x3f333333,
                0x3f800000, 0x3f19999a, 0x3f4ccccd, 0x3f333333, 0x3f800000, 0x3f800000, 0x3f4ccccd,
                0x42a00000, 0x3f000000
            ]
        );
        assert_eq!(human.animation_machine().owner(), None);
        assert_eq!(human.animation_machine().model(), None);
        for collection in [
            &human.custom_animations,
            &human.first_auxiliary,
            &human.second_auxiliary,
            &human.last_auxiliary,
        ] {
            assert_eq!(collection.allocator_tag, 0xd7);
            assert!(collection.is_empty());
        }
        assert_eq!(human.inventory().allocator_tag(), 0xd7);
        for record in human.auxiliary_records() {
            assert_eq!(record.constructor_words(), &[0; 8]);
            assert_eq!(record.constructor_byte(), 0);
            assert_eq!(record.untouched_seed().untouched_word, 0xa5a5_a5a5);
            assert_eq!(record.untouched_seed().padding, [0xa5; 3]);
            assert_eq!(record.untouched_seed().untouched_tail, [0xa5; 16]);
        }
    }
    #[test]
    fn all_142_v5_v6_short_bounds_preserve_skipped_fields_and_return_success() {
        for version in [5u8, 6] {
            let mut payload = vec![version];
            payload.extend(0x1234_5678u32.to_le_bytes());
            if version == 6 {
                payload.extend(0x89ab_cdefu32.to_le_bytes());
            }
            payload.extend([0xff; 64]);
            for length in 1..=payload.len() {
                let mut human = HumanState::construct(seed());
                let original_properties = human.properties.clone();
                let mut reader = ActorDataReader::new(&payload[..length]);
                human.load_source_data(&mut reader).unwrap();
                assert_eq!(
                    human.human_type_mask(),
                    if length >= 5 {
                        0x1234_5678
                    } else {
                        0xa5a5_a5a5
                    },
                    "version{version} length{length}"
                );
                assert_eq!(
                    human.voice_index(),
                    if version == 6 && length >= 9 {
                        0x89ab_cdef
                    } else {
                        12
                    }
                );
                assert_eq!(
                    human.properties().record(),
                    if length == payload.len() {
                        &[0xff; 64]
                    } else {
                        original_properties.record()
                    }
                );
                let expected = if length == payload.len() {
                    length
                } else if version == 6 && length >= 9 {
                    9
                } else if length >= 5 {
                    5
                } else {
                    1
                };
                assert_eq!(reader.position(), expected);
                assert_eq!(human.current_properties().record(), &[0xa5; 64]);
            }
        }
    }
    #[test]
    fn missing_version_and_reached_old_conversions_are_not_fabricated_defaults() {
        let mut human = HumanState::construct(seed());
        let mut empty = ActorDataReader::new(&[]);
        assert_eq!(
            human.load_source_data(&mut empty),
            Err(HumanDataError::MissingVersion)
        );
        assert_eq!(empty.position(), 0);
        for version in 1..=4 {
            let bytes = [version, 9, 8, 7, 6];
            let mut reader = ActorDataReader::new(&bytes);
            assert_eq!(
                human.load_source_data(&mut reader),
                Err(HumanDataError::UnsupportedConversion(version))
            );
            assert_eq!(reader.position(), 1);
        }
        for version in [0, 7, 255] {
            let bytes = [version, 9, 8, 7, 6];
            let mut reader = ActorDataReader::new(&bytes);
            human.load_source_data(&mut reader).unwrap();
            assert_eq!(reader.position(), 1);
        }
        assert_eq!(human.human_type_mask(), 0xa5a5_a5a5);
    }
    #[test]
    fn machine_constructor_preserves_untouched_word_and_does_not_initialize_from_an_actor() {
        let mut machine = HumanAnimationMachine::construct(HumanAnimationMachineSeed {
            untouched_word: 0xa5f1_8732,
        });
        assert_eq!(machine.owner(), None);
        assert_eq!(machine.model(), None);
        machine.destruct();
        assert_eq!(machine.untouched_constructor_word(), 0xa5f1_8732);
    }
    #[test]
    fn inventory_constructor_owns_empty_storage_and_resets_all_item_records() {
        let inventory = HumanInventory::construct(HumanInventorySeed {
            allocator_tag: 0xa7,
        });
        assert_eq!(inventory.allocator_tag(), 0xa7);
        assert!(inventory.items().is_empty());
        assert_eq!(inventory.cycle_state, 1);
        assert_eq!(inventory.cycle_words, [0; 2]);
        assert_eq!(inventory.cycle_mask, 0x00ff_ffff);
        for item in inventory.first_slots.iter().chain(&inventory.second_slots) {
            assert_eq!(item.representation_bits(), &[0; 4]);
        }
        assert_eq!(
            (
                inventory.first_parameters,
                inventory.second_parameters,
                inventory.last_parameters
            ),
            ([0; 4], [0; 4], [0; 4])
        );
    }
    #[test]
    fn duplicate_from_copies_max_bank_and_type_and_preserves_live_state() {
        use crate::script_scheduler::FrameReferenceId;
        let mut target = HumanState::construct(seed());
        target.human_type_mask = 0x1111_1111;
        target.voice_index = 7;
        target.properties = HumanProperties::from_record([0x11; 64]);
        target.current_properties = HumanProperties::from_record([0x22; 64]);
        target.target_e = Some(FrameReferenceId::from_existing(3));
        target.target_n = Some(FrameReferenceId::from_existing(4));
        target.base_frame = Some(FrameReferenceId::from_existing(5));
        let mut source = HumanState::construct(seed());
        source.human_type_mask = 0x1234_5678;
        source.voice_index = 9;
        source.properties = HumanProperties::from_record([0x33; 64]);
        source.current_properties = HumanProperties::from_record([0x44; 64]);
        target.duplicate_from(&source);
        assert_eq!(target.human_type_mask(), 0x1234_5678);
        assert_eq!(target.properties().record(), &[0x33; 64]);
        assert_eq!(target.current_properties().record(), &[0x22; 64]);
        assert_eq!(target.voice_index(), 7);
        assert_eq!(target.target_e(), Some(FrameReferenceId::from_existing(3)));
        assert_eq!(target.target_n(), Some(FrameReferenceId::from_existing(4)));
        assert_eq!(
            target.base_frame(),
            Some(FrameReferenceId::from_existing(5))
        );
    }
    #[test]
    fn save_data_encodes_version_type_voice_and_max_bank_byte_exact() {
        let mut human = HumanState::construct(seed());
        human.human_type_mask = 0x1234_5678;
        human.voice_index = 12;
        let record: [u8; 64] = std::array::from_fn(|index| index as u8);
        human.properties = HumanProperties::from_record(record);
        // Hand-derived from C_human::SaveData field order: version byte,
        // human_type word, voice word, 64 maximum-property bytes.
        let mut expected = vec![6u8];
        expected.extend_from_slice(&0x1234_5678u32.to_le_bytes());
        expected.extend_from_slice(&12u32.to_le_bytes());
        expected.extend_from_slice(&record);
        let bytes = human.encode_source_data();
        assert_eq!(bytes.len(), 73);
        assert_eq!(bytes, expected);
    }
    #[test]
    fn save_then_load_round_trips_version_six_fields() {
        let mut source = HumanState::construct(seed());
        source.human_type_mask = 0xdead_beef;
        source.voice_index = 0x1d;
        source.properties = HumanProperties::from_record([0x5a; 64]);
        let bytes = source.encode_source_data();
        let mut target = HumanState::construct(seed());
        let mut reader = ActorDataReader::new(&bytes);
        target.load_source_data(&mut reader).unwrap();
        assert_eq!(reader.position(), bytes.len());
        assert_eq!(target.human_type_mask(), 0xdead_beef);
        assert_eq!(target.voice_index(), 0x1d);
        assert_eq!(target.properties().record(), &[0x5a; 64]);
        assert_eq!(target.current_properties().record(), &[0xa5; 64]);
    }
    #[test]
    fn car_latch_starts_empty_and_stores_owner() {
        use crate::script_scheduler::ActorReferenceId;
        let mut human = HumanState::construct(seed());
        assert_eq!(human.car(), None);
        let car = ActorReferenceId::from_existing(11);
        human.set_car(Some(car));
        assert_eq!(human.car(), Some(car));
        human.set_car(None);
        assert_eq!(human.car(), None);
    }
    #[test]
    fn game_init_prefix_defaults_to_null_cars_stand_and_max_bank_copy() {
        let mut human = HumanState::construct(seed());
        // Hand-derived from C_human::GameInit (mafia-re 113b5b6
        // game/actors/C_human.cpp:3174-3176,3180): NULL cars, STAND
        // (1, C_human.h:137), max bank copied over the current bank.
        assert_eq!(human.car(), None);
        assert_eq!(human.using_car(), None);
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        assert_eq!(human.action_state(), 1);
        assert_ne!(
            human.current_properties().record(),
            human.properties().record()
        );
        human.apply_game_init_prefix();
        assert_eq!(human.car(), None);
        assert_eq!(human.using_car(), None);
        assert_eq!(human.action_state(), 1);
        assert_eq!(
            human.current_properties().record(),
            human.properties().record()
        );
        assert_eq!(
            human.current_properties().component_bits(),
            [
                0x3f333333, 0x42c80000, 0x41a00000, 0x41a00000, 0x41a00000, 0x41a00000, 0x3f333333,
                0x3f800000, 0x3f19999a, 0x3f4ccccd, 0x3f333333, 0x3f800000, 0x3f800000, 0x3f4ccccd,
                0x42a00000, 0x3f000000
            ]
        );
    }
    #[test]
    fn game_init_prefix_clears_latched_cars_restores_bank_and_stand() {
        use crate::script_scheduler::ActorReferenceId;
        let mut human = HumanState::construct(seed());
        human.set_car(Some(ActorReferenceId::from_existing(11)));
        human.set_using_car(Some(ActorReferenceId::from_existing(12)));
        // Direct word write: no other owner produces action values yet
        // (Movement/car AI DEFERRED); 9 is HUMAN_ACTION_CAR (C_human.h:145).
        human.action_state = 9;
        human.current_properties = HumanProperties::from_record([0x77; 64]);
        human.apply_game_init_prefix();
        assert_eq!(human.car(), None);
        assert_eq!(human.using_car(), None);
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        assert_eq!(
            human.current_properties().record(),
            human.properties().record()
        );
        // Repeating the prefix over init state stays a pure no-op.
        let bank = human.current_properties().clone();
        human.apply_game_init_prefix();
        assert_eq!(human.car(), None);
        assert_eq!(human.using_car(), None);
        assert_eq!(human.action_state(), 1);
        assert_eq!(human.current_properties(), &bank);
    }
    #[test]
    fn intern_use_car_seat_core_admits_range_and_leaves_refusals_untouched() {
        use crate::script_scheduler::ActorReferenceId;
        let mut human = HumanState::construct(seed());
        // Hand-derived from C_human::Intern_UseCar (mafia-re 113b5b6
        // game/actors/C_human.cpp:5570 guard, :5589 car, :5601 seat,
        // :5610 HUMAN_ACTION_CAR 9, C_human.h:145): a 2-seat car admits
        // seats 0 and 1, starting from the no-seat sentinel.
        assert_eq!(human.seat(), HUMAN_SEAT_NONE);
        assert_eq!(human.seat(), -1);
        let car = ActorReferenceId::from_existing(21);
        assert!(human.apply_intern_use_car(car, 0, 2));
        assert_eq!(human.car(), Some(car));
        assert_eq!(human.seat(), 0);
        assert_eq!(human.action_state(), HUMAN_ACTION_CAR);
        // The :5570 range arm refuses seat == num_seats with the latched
        // state untouched.
        let other = ActorReferenceId::from_existing(22);
        assert!(!human.apply_intern_use_car(other, 2, 2));
        assert_eq!(human.car(), Some(car));
        assert_eq!(human.seat(), 0);
        assert_eq!(human.action_state(), HUMAN_ACTION_CAR);
        // Negative seats refuse: the body indexes v_seats[s] with the raw
        // int, so admission would read out of bounds.
        assert!(!human.apply_intern_use_car(other, -1, 2));
        assert_eq!(human.car(), Some(car));
        assert_eq!(human.seat(), 0);
        // A latched using_car survives refusal (no path here writes it).
        human.set_using_car(Some(ActorReferenceId::from_existing(23)));
        assert!(!human.apply_intern_use_car(other, 2, 2));
        assert_eq!(human.car(), Some(car));
        assert_eq!(human.seat(), 0);
        assert_eq!(
            human.using_car(),
            Some(ActorReferenceId::from_existing(23))
        );
        // The passenger seat admits over a previous latch.
        assert!(human.apply_intern_use_car(other, 1, 2));
        assert_eq!(human.car(), Some(other));
        assert_eq!(human.seat(), 1);
        assert_eq!(human.action_state(), HUMAN_ACTION_CAR);
        // The using_car latch is untouched on this path (the body writes no
        // such word here), across both refusals and admissions; last_car
        // has no owner yet and stays unwritten.
        assert_eq!(
            human.using_car(),
            Some(ActorReferenceId::from_existing(23))
        );
    }
    #[test]
    fn intern_from_car_placement_clears_cars_keeps_seat_and_refuses_empty() {
        use crate::script_scheduler::ActorReferenceId;
        let mut human = HumanState::construct(seed());
        // Hand-derived from the C_human::Intern_FromCar guard (mafia-re
        // 113b5b6 game/actors/C_human.cpp:5658): no car, no destination, a
        // non-car carrier, or the no-seat sentinel each refuse with the
        // state untouched.
        assert!(!human.apply_intern_from_car(true, true));
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        let car = ActorReferenceId::from_existing(21);
        assert!(human.apply_intern_use_car(car, 1, 2));
        human.set_using_car(Some(ActorReferenceId::from_existing(23)));
        assert!(!human.apply_intern_from_car(false, true));
        assert!(!human.apply_intern_from_car(true, false));
        assert_eq!(human.car(), Some(car));
        assert_eq!(human.action_state(), HUMAN_ACTION_CAR);
        // Refusals leave using_car and seat untouched.
        assert_eq!(
            human.using_car(),
            Some(ActorReferenceId::from_existing(23))
        );
        assert_eq!(human.seat(), 1);
        assert!(human.apply_intern_from_car(true, true));
        assert_eq!(human.car(), None);
        assert_eq!(human.using_car(), None);
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        // The :5656-5717 body carries no seat write: the index is retained
        // while the car latches drop.
        assert_eq!(human.seat(), 1);
    }
    #[test]
    fn can_enter_to_car_owned_guard_tracks_car_latch_only() {
        use crate::script_scheduler::ActorReferenceId;
        // Marker for GAME 0x5808c0 (mafia-re 113b5b6
        // game/actors/C_human.cpp:6373): only the `if (car) return false;`
        // arm is owned; seat-property and collision probes stay DEFERRED.
        let mut human = HumanState::construct(seed());
        assert!(human.can_enter_to_car_guard());
        human.set_car(Some(ActorReferenceId::from_existing(21)));
        assert!(!human.can_enter_to_car_guard());
        human.set_car(None);
        assert!(human.can_enter_to_car_guard());
    }
    #[test]
    fn stop_anim_core_starts_cleared_and_dead_with_no_request() {
        // Construction defaults for the Do_StopAnim words (GAME 0x57d2e0,
        // mafia-re 113b5b6 game/actors/C_human.cpp:5362): pre-GameInit not
        // alive, anim words cleared, no pending PersonAnim handoff.
        let human = HumanState::construct(seed());
        assert!(!human.alive());
        assert_eq!(human.anim_id(), 0);
        assert_eq!(human.next_anim_id(), 0);
        assert_eq!(human.person_anim_request(), None);
    }
    #[test]
    fn do_stop_anim_dead_path_writes_only_stand() {
        // Hand-derived from C_human::Do_StopAnim (mafia-re 113b5b6
        // game/actors/C_human.cpp:5362): the dead path writes only
        // action_state = HUMAN_ACTION_STAND (1, C_human.h:137); the anim
        // words and any pending PersonAnim request stay untouched.
        let mut human = HumanState::construct(seed());
        // Direct word writes: no other owner produces these values yet
        // (Movement/Update DEFERRED); 2 is HUMAN_ACTION_WALK (C_human.h:138).
        human.action_state = 2;
        human.anim_id = 7;
        human.next_anim_id = 0x42;
        assert!(!human.alive());
        human.apply_do_stop_anim();
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        assert_eq!(human.action_state(), 1);
        assert_eq!(human.anim_id(), 7);
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
        // A stale pending request also survives the dead path (the original
        // issues no PersonAnim call there).
        human.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 1.5,
            stance: false,
            play_once: true,
        });
        human.action_state = 2;
        human.apply_do_stop_anim();
        assert_eq!(human.action_state(), 1);
        assert_eq!(human.anim_id(), 7);
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(
            human.person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 1.5,
                stance: false,
                play_once: true,
            })
        );
    }
    #[test]
    fn do_stop_anim_alive_path_clears_anim_selects_breath_and_hands_off() {
        // Hand-derived from C_human::Do_StopAnim (mafia-re 113b5b6
        // game/actors/C_human.cpp:5362): the alive path sets STAND,
        // anim_id = 0, next_anim_id = ANIM_BREATH01A (1, C_actor.h:548) and
        // invokes PersonAnim(0.0, true, false), recorded here as the handoff.
        let mut human = HumanState::construct(seed());
        human.set_alive(true);
        human.action_state = 2;
        human.anim_id = 7;
        human.next_anim_id = 0x42;
        human.apply_do_stop_anim();
        assert_eq!(human.action_state(), 1);
        assert_eq!(human.anim_id(), 0);
        assert_eq!(human.next_anim_id(), ANIM_BREATH01A);
        assert_eq!(human.next_anim_id(), 1);
        assert_eq!(
            human.person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            })
        );
        // Repeating over the stopped state is a pure no-op (same words, same
        // handoff overwritten with an equal value).
        human.apply_do_stop_anim();
        assert_eq!(human.action_state(), 1);
        assert_eq!(human.anim_id(), 0);
        assert_eq!(human.next_anim_id(), 1);
        assert_eq!(
            human.person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            })
        );
        // The future PersonAnim owner takes the handoff exactly once.
        assert_eq!(
            human.take_person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            })
        );
        assert_eq!(human.person_anim_request(), None);
    }
    #[test]
    fn ghost_colls_push_empty_and_merge_follow_original_order() {
        // Marker-only contract for C_human::ghost_colls @0x698 and
        // C_human::next_coll_owners @0x6a8 (mafia-re 113b5b6
        // game/actors/C_human.h:970-976) with the GAME 0x57cfe0 merge
        // (game/actors/C_human.cpp:5308-5322): construction stages empty
        // vectors; GAME 0x52f920 pushes append; GAME 0x507b50 empties;
        // the merge reverses colls then ghosts, and the ghost-free path
        // leaves the staging empty (pass colls straight through).
        let mut human = HumanState::construct(seed());
        assert!(human.ghost_colls().is_empty());
        assert!(human.next_coll_owners().is_empty());
        human.push_ghost_coll(HumanDynCollId::from_raw(7));
        human.push_ghost_coll(HumanDynCollId::from_raw(9));
        assert_eq!(
            human.ghost_colls(),
            &[
                HumanDynCollId::from_raw(7),
                HumanDynCollId::from_raw(9)
            ]
        );
        let colls = [
            HumanDynCollId::from_raw(1),
            HumanDynCollId::from_raw(2),
            HumanDynCollId::from_raw(3),
        ];
        human.rebuild_next_coll_owners(&colls);
        assert_eq!(
            human.next_coll_owners(),
            &[
                HumanDynCollId::from_raw(3),
                HumanDynCollId::from_raw(2),
                HumanDynCollId::from_raw(1),
                HumanDynCollId::from_raw(9),
                HumanDynCollId::from_raw(7),
            ]
        );
        human.empty_ghost_colls();
        assert!(human.ghost_colls().is_empty());
        human.rebuild_next_coll_owners(&colls);
        assert!(human.next_coll_owners().is_empty());
    }
    #[test]
    fn cb_coll_tester_restarts_bloody_tracks_only_on_high_effects() {
        use crate::mission_options::{OPTION_HIGH, OPTION_LOW};
        // Hand-derived from C_human::cbCollTester (mafia-re 113b5b6
        // game/actors/C_human.cpp:8167-8181) with TRACK_TEXTURE_BFOOTPRINT 6
        // (game/mission/game.h:92-94): below HIGH nothing happens even with
        // no steps; on HIGH empty steps restart to 10 of texture 6 with max
        // 10; nonzero steps publish the live count and keep the texture.
        let mut human = HumanState::construct(seed());
        assert_eq!(human.track_steps(), 0);
        human.cb_coll_tester(OPTION_LOW);
        assert_eq!(
            (
                human.track_steps(),
                human.track_steps_max(),
                human.track_texture()
            ),
            (0, 0, 0)
        );
        human.cb_coll_tester(OPTION_HIGH);
        assert_eq!(
            (
                human.track_steps(),
                human.track_steps_max(),
                human.track_texture()
            ),
            (10, 10, TRACK_TEXTURE_BFOOTPRINT)
        );
        human.track_steps = 7;
        human.track_steps_max = 3;
        human.track_texture = 9;
        human.cb_coll_tester(OPTION_HIGH);
        assert_eq!(
            (
                human.track_steps(),
                human.track_steps_max(),
                human.track_texture()
            ),
            (7, 7, 9)
        );
        human.cb_coll_tester(OPTION_LOW);
        assert_eq!(
            (
                human.track_steps(),
                human.track_steps_max(),
                human.track_texture()
            ),
            (7, 7, 9)
        );
    }
}
