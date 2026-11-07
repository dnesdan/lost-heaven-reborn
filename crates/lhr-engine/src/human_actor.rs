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
//! The on-foot micro-state (Do_Breath, Go_Front/Back/Left/Right plus the
//! diagonal Go_FrontLeft/FrontRight/BackLeft/BackRight blends, Do_Aimed,
//! the Do_Crouched latch slice, Turn_Turn, the Do_CornerBreath/View selects,
//! Do_ShockUp/Stumble, the Do_DeadBodyDrop anim slice, Force_CanWorking and
//! InventoryTruncate) is owned here as marker-only leaves over the velocity,
//! turn_speed, dir, crouch/box/dead-body latches, aim/guard/change latches
//! and carried-weapon ammo words staged in this file; the PersonAnim,
//! ChangeWeaponModel and DelExtendedAnim invocations inside them are recorded
//! as call markers, and collision, animation-machine, frame, tick, table and
//! car/seat owner-lifecycle effects stay DEFERRED.
//! The dtor animation-stop device effect (GAME 0x56f210) and the Init frame
//! effects (GAME 0x56f350, owned by ActorWorld::init_human_owner) stay with the
//! frame owner; detector/police owners do not exist yet and are DEFERRED. AI
//! ghost staging only is owned here (ghost_colls @0x698 push/empty plus the
//! next_coll_owners @0x6a8 merge leaf); the C_entity ai_ghost @0x1268 refresh
//! and the pedestrian AI Tick (GAME 0x508150) stay DEFERRED.
//! The collision-set selects (ActivateColls/DeactivateColls), the shock-step
//! fill (Do_ShockDown), the dead-body anim select (ChangeDeadAnim), the
//! fire-word reset (ResetWeaponEffects), the stroke-target scan
//! (Can_SpecialStroke), the scream gate (NewScream), the cutscene-link
//! staging (SetCutsceneLink) and the walk-speed publish (GetActorSpeed) are
//! owned here as pure selects and staged-word leaves; the PoseValidate,
//! Can_DropOut*, SoundsChange and force-feedback bodies are explicit
//! unsupported stubs (frame/collision/sound/device owners DEFERRED).
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
    /// Item-table index (mafia-re 113b5b6 game/actors/g_inventory.h:16:
    /// ushort id at +0x0; stored widened, the low 16 bits carry the value).
    pub fn item_id(&self) -> u16 {
        self.bits[0] as u16
    }
    /// Loaded ammunition (mafia-re 113b5b6 game/actors/g_inventory.h:22:
    /// int ammo_loaded at +0x4).
    pub fn ammo_loaded(&self) -> i32 {
        self.bits[1] as i32
    }
    /// Reserve ammunition (mafia-re 113b5b6 game/actors/g_inventory.h:23:
    /// int ammo_spare at +0x8).
    pub fn ammo_spare(&self) -> i32 {
        self.bits[2] as i32
    }
    /// Test/host setter for the ammunition words (the table-clamp writer is
    /// the truncate core; the using-object word stays untouched).
    pub fn set_ammo(&mut self, loaded: i32, spare: i32) {
        self.bits[1] = loaded as u32;
        self.bits[2] = spare as u32;
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
    /// Marker-only staging of the carried weapons the truncate core clamps
    /// (mafia-re 113b5b6 game/actors/g_inventory.h:142-144: right_hand 0x20,
    /// pockets[INVENTORY_POCKETS] 0x30, big_weapon 0x80). G_Inventory
    /// Select/Holster/Insert/SetInCar placement plus the gameItemTable owner
    /// stay DEFERRED; the truncate caller supplies the table maxima per id.
    right_hand: HumanInventoryItem,
    big_weapon: HumanInventoryItem,
    pockets: [HumanInventoryItem; INVENTORY_POCKETS],
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
            right_hand: HumanInventoryItem::construct(),
            big_weapon: HumanInventoryItem::construct(),
            pockets: [HumanInventoryItem::construct(); INVENTORY_POCKETS],
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
    /// Carried right-hand weapon staging for the truncate core (mafia-re
    /// 113b5b6 game/actors/g_inventory.h:142: right_hand 0x20).
    pub fn right_hand(&self) -> &HumanInventoryItem {
        &self.right_hand
    }
    /// Carried big-weapon staging for the truncate core (mafia-re 113b5b6
    /// game/actors/g_inventory.h:144: big_weapon 0x80).
    pub fn big_weapon(&self) -> &HumanInventoryItem {
        &self.big_weapon
    }
    /// Carried pocket-weapon staging for the truncate core (mafia-re 113b5b6
    /// game/actors/g_inventory.h:143: pockets[INVENTORY_POCKETS] 0x30).
    pub fn pockets(&self) -> &[HumanInventoryItem; INVENTORY_POCKETS] {
        &self.pockets
    }
    /// Test/host setter for the right-hand staging (placement writers stay
    /// DEFERRED).
    pub fn set_right_hand(&mut self, item: HumanInventoryItem) {
        self.right_hand = item;
    }
    /// Test/host setter for the big-weapon staging (placement writers stay
    /// DEFERRED).
    pub fn set_big_weapon(&mut self, item: HumanInventoryItem) {
        self.big_weapon = item;
    }
    /// Mutable pocket staging for the truncate core and test/host setup.
    pub fn pocket_mut(&mut self, index: usize) -> &mut HumanInventoryItem {
        &mut self.pockets[index]
    }
    /// GAME 0x57f9a0 (RVA 0x17f9a0) C_human::InventoryTruncate (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6133-6140): clamp right_hand,
    /// big_weapon and every pocket through the TruncateAmmo leaf
    /// (C_human.cpp:6121-6131). `num_game_items` is `numOfGameItems` and
    /// `ammo_limits` the `gameItemTable` (ammo_max, spare_max) lookup by id;
    /// out-of-range ids and missing table entries are left untouched.
    /// SCOPE: ammunition clamp only. DEFERRED with explicit markers, no new
    /// owner here: the material-table owner, collision, tick, and every
    /// Select/Holster/Insert/SetInCar placement writer.
    pub fn apply_inventory_truncate(
        &mut self,
        num_game_items: u32,
        ammo_limits: &dyn Fn(u16) -> Option<(u32, u32)>,
    ) {
        Self::truncate_slot(&mut self.right_hand, num_game_items, ammo_limits);
        Self::truncate_slot(&mut self.big_weapon, num_game_items, ammo_limits);
        for pocket in self.pockets.iter_mut() {
            Self::truncate_slot(pocket, num_game_items, ammo_limits);
        }
    }
    fn truncate_slot(
        item: &mut HumanInventoryItem,
        num_game_items: u32,
        ammo_limits: &dyn Fn(u16) -> Option<(u32, u32)>,
    ) {
        let id = item.item_id();
        if id > 1 && u32::from(id) < num_game_items {
            if let Some((ammo_max, spare_max)) = ammo_limits(id) {
                truncate_item_ammo(item, ammo_max, spare_max);
            }
        }
    }
}

/// GAME 0x57f9a0 (RVA 0x17f9a0) TruncateAmmo clamp leaf (mafia-re 113b5b6
/// game/actors/C_human.cpp:6120-6131, `static inline` ahead of
/// C_human::InventoryTruncate): unsigned compare of each ammunition word
/// against its table maximum (`(uint)loaded > ammo_max`, `(uint)spare >
/// spare_max`), clamping down only. The id-range gate (`id > 1 && id <
/// numOfGameItems`, C_human.cpp:6123 with `numOfGameItems` and `gameItemTable`
/// from mafia-re 113b5b6 game/g_material.h:203-224 and .cpp:16,40) and the
/// table lookup live with the material-table owner (DEFERRED); the caller
/// supplies the looked-up maxima. The using-object word stays untouched.
pub(crate) fn truncate_item_ammo(
    item: &mut HumanInventoryItem,
    ammo_max: u32,
    spare_max: u32,
) {
    if item.bits[1] > ammo_max {
        item.bits[1] = ammo_max;
    }
    if item.bits[2] > spare_max {
        item.bits[2] = spare_max;
    }
}

/// Replication of S_vector::Normalize for the diagonal Go_* strafe blends
/// (mafia-re 113b5b6 ls3df/I3D/I3d_math.h:153-171, inlined at the call sites
/// with no out-of-line copy, so it carries no GAME address of its own):
/// near-unit vectors (|sq - 1| < MRG_ZERO, 1e-8 per ls3df/ls3df.h:48) pass
/// through; near-zero vectors (sq < MRG_ZERO) become the unit vector along
/// the first nonzero axis (x, else z, else y-by-sign with 0.0 counting as
/// positive); otherwise the vector scales by 1/sqrt(sq). All arithmetic is
/// f32, matching the original.
fn normalize_strafe_vector(vector: [f32; 3]) -> [f32; 3] {
    const MRG_ZERO: f32 = 1e-8;
    let square = vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2];
    if (square - 1.0).abs() < MRG_ZERO {
        return vector;
    }
    if square < MRG_ZERO {
        if vector[0] != 0.0 {
            return [if vector[0] < 0.0 { -1.0 } else { 1.0 }, 0.0, 0.0];
        }
        if vector[2] != 0.0 {
            return [0.0, 0.0, if vector[2] < 0.0 { -1.0 } else { 1.0 }];
        }
        return [0.0, if vector[1] < 0.0 { -1.0 } else { 1.0 }, 0.0];
    }
    let factor = 1.0 / square.sqrt();
    [
        vector[0] * factor,
        vector[1] * factor,
        vector[2] * factor,
    ]
}

/// Staged S_vector::AngleTo for the Can_SpecialStroke scan (LS3DF native;
/// the vector owner stays DEFERRED): the angle between two vectors through
/// the clamped normalized dot product. A zero-length side yields NaN, which
/// fails both of the scan's angle gates exactly like a missed probe.
fn vector_angle(a: [f32; 3], b: [f32; 3]) -> f32 {
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let len_a = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    let len_b = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
    if len_a == 0.0 || len_b == 0.0 {
        return f32::NAN;
    }
    (dot / (len_a * len_b)).clamp(-1.0, 1.0).acos()
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
    /// Live float view of one property component (little-endian f32 words,
    /// matching the constructor's bit patterns). The Go_* walk cores read
    /// index HUMAN_PROPERTY_SPEED through here (mafia-re 113b5b6
    /// game/actors/C_human.h:92); like the original there is no bounds
    /// concept, so an out-of-range index panics instead of reading adjacent
    /// memory.
    pub fn float_component(&self, component: usize) -> f32 {
        f32::from_le_bytes(
            self.bytes[component * 4..component * 4 + 4]
                .try_into()
                .unwrap(),
        )
    }
    /// Live "Speed" multiplier for the Go_* walk cores (mafia-re 113b5b6
    /// game/actors/C_human.h:92 HUMAN_PROPERTY_SPEED 7; read at
    /// game/actors/C_human.cpp:5395 and its Go_* siblings).
    pub fn speed_factor(&self) -> f32 {
        self.float_component(HUMAN_PROPERTY_SPEED)
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
/// Original "Speed" property index (mafia-re 113b5b6
/// game/actors/C_human.h:92 HUMAN_PROPERTY_SPEED 7): the multiplier the Go_*
/// walk cores scale their base speeds by. Read from the live bank
/// (`current_properties`); the maximum bank only feeds GameInit's copy.
pub const HUMAN_PROPERTY_SPEED: usize = 7;
/// Original run animation id (mafia-re 113b5b6 game/actors/C_actor.h:552:
/// 0x5, "run1.i3d"). Written by the Go_Front core of this slice; the
/// PersonAnim stance/anim-set remap of the id stays DEFERRED.
pub const ANIM_RUN1: i32 = 5;
/// Original walk animation id (mafia-re 113b5b6 game/actors/C_actor.h:553:
/// 0x6, "walk1.i3d"). Written by the Go_Front core of this slice; the
/// PersonAnim stance/anim-set remap of the id stays DEFERRED.
pub const ANIM_WALK1: i32 = 6;
/// Original back-walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:554: 0x7, "back1.i3d"). Written by the Go_Back core
/// of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_BACK1: i32 = 7;
/// Original left-strafe walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:558: 0xb, "strafL1.i3d"). Written by the Go_Left
/// core of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_STRAFL1: i32 = 0xb;
/// Original right-strafe walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:559: 0xc, "strafR1.i3d"). Written by the Go_Right
/// core of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_STRAFR1: i32 = 0xc;
/// Original left-strafe run animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:560: 0xd, "strafRL1.i3d"). Written by the Go_Left
/// core of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_STRAFRL1: i32 = 0xd;
/// Original right-strafe run animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:561: 0xe, "strafRR1.i3d"). Written by the Go_Right
/// core of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_STRAFRR1: i32 = 0xe;
/// Original front-left run animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:562: 0xf, "runL1.i3d"). Written by the Go_FrontLeft
/// core of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_RUNL1: i32 = 0xf;
/// Original front-right run animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:563: 0x10, "runR1.i3d"). Written by the
/// Go_FrontRight core of this slice; the PersonAnim stance/anim-set remap
/// stays DEFERRED.
pub const ANIM_RUNR1: i32 = 0x10;
/// Original front-left walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:564: 0x11, "walkL1.i3d"). Written by the
/// Go_FrontLeft core of this slice; the PersonAnim stance/anim-set remap
/// stays DEFERRED.
pub const ANIM_WALKL1: i32 = 0x11;
/// Original front-right walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:565: 0x12, "walkR1.i3d"). Written by the
/// Go_FrontRight core of this slice; the PersonAnim stance/anim-set remap
/// stays DEFERRED.
pub const ANIM_WALKR1: i32 = 0x12;
/// Original back-left walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:566: 0x13, "backL1.i3d"). Written by the Go_BackLeft
/// core of this slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_BACKL1: i32 = 0x13;
/// Original back-right walk animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:567: 0x14, "backR1.i3d"). Written by the
/// Go_BackRight core of this slice; the PersonAnim stance/anim-set remap
/// stays DEFERRED.
pub const ANIM_BACKR1: i32 = 0x14;
/// Original turn animation id (mafia-re 113b5b6 game/actors/C_actor.h:557:
/// 0xa, "turn1.i3d"). Written by the Turn_Turn core of this slice; the
/// PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_TURN1: i32 = 0xa;
/// Original panic-crouch-down stativ animation ids (mafia-re 113b5b6
/// game/actors/C_actor.h:597-601: 0x3d "x panika drep dolu.i3d", 0x3e
/// "x panika drep stativ.i3d", 0x40 "x panika na4 dolu.i3d", 0x41
/// "x panika na4 stativ.i3d", 0x43 "x panika leh dolu.i3d", 0x44
/// "x panika leh stativ.i3d"). Read by the Do_ShockUp core of this slice,
/// which advances a matching stativ by one; the PersonAnim remap stays
/// DEFERRED.
pub const ANIM_X_PANIKA_DREP_STATIV: i32 = 0x3e;
/// Original panic-on-all-fours stativ animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:599: 0x41). Read by the Do_ShockUp core.
pub const ANIM_X_PANIKA_NA4_STATIV: i32 = 0x41;
/// Original panic-lying stativ animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:601: 0x44). Read by the Do_ShockUp core.
pub const ANIM_X_PANIKA_LEH_STATIV: i32 = 0x44;
/// Original panic-run animation ids (mafia-re 113b5b6
/// game/actors/C_actor.h:603-605: 0x46 "panika run01a.i3d", 0x48
/// "panika run03a.i3d", 0x4a "panika run02a.i3d"). Read by the
/// Do_ShockStumble core of this slice, which advances a matching run by one;
/// the PersonAnim remap stays DEFERRED.
pub const ANIM_PANIKA_RUN01A: i32 = 0x46;
/// Original panic-run animation id, second variant (mafia-re 113b5b6
/// game/actors/C_actor.h:604: 0x48). Read by the Do_ShockStumble core.
pub const ANIM_PANIKA_RUN03A: i32 = 0x48;
/// Original panic-run animation id, third variant (mafia-re 113b5b6
/// game/actors/C_actor.h:605: 0x4a). Read by the Do_ShockStumble core.
pub const ANIM_PANIKA_RUN02A: i32 = 0x4a;
/// Original corner-stand breath animation ids (mafia-re 113b5b6
/// game/actors/C_actor.h:606-607: 0x4e "enm roh stoj stat L.i3d", 0x4f
/// "enm roh stoj stat R.i3d"). Written by the Do_CornerBreath core of this
/// slice; the PersonAnim stance/anim-set remap stays DEFERRED.
pub const ANIM_ENM_ROH_STOJ_STAT_L: i32 = 0x4e;
/// Original right corner-stand breath animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:607: 0x4f). Written by the Do_CornerBreath core.
pub const ANIM_ENM_ROH_STOJ_STAT_R: i32 = 0x4f;
/// Original corner-stand peek animation ids (mafia-re 113b5b6
/// game/actors/C_actor.h:608-610: 0x50 "enm roh stoj nahl L.i3d", 0x51
/// "enm roh stoj nahl R.i3d", 0x52 "enm roh drep nahl T.i3d"). Written by the
/// Do_CornerView core of this slice; the PersonAnim/SetWaitAnim remap stays
/// DEFERRED.
pub const ANIM_ENM_ROH_STOJ_NAHL_L: i32 = 0x50;
/// Original right corner-stand peek animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:609: 0x51). Written by the Do_CornerView core.
pub const ANIM_ENM_ROH_STOJ_NAHL_R: i32 = 0x51;
/// Original crouched corner peek animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:610: 0x52, crouched only). Written by the
/// Do_CornerView core on the crouched CORNER_2 path.
pub const ANIM_ENM_ROH_DREP_NAHL_T: i32 = 0x52;
/// Original dead-body-drop animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:658: 0xa8, "NoseniMrtvolyTomShozeni.i3d"). Written
/// by the Do_DeadBodyDrop core of this slice; the PersonAnim/SetWaitAnim
/// remap stays DEFERRED.
pub const ANIM_NOSENI_MRTVOLY_TOM_SHOZENI: i32 = 0xa8;
/// Original "Strength" property index (mafia-re 113b5b6
/// game/actors/C_human.h:38 HUMAN_PROPERTY_STRENGTH 0): read from the live
/// bank by the Do_Castrate/Do_HardHead strike cores.
pub const HUMAN_PROPERTY_STRENGTH: usize = 0;
/// Original "Energy" property index (mafia-re 113b5b6
/// game/actors/C_human.h:39 HUMAN_PROPERTY_ENERGY 1, the health): read and
/// written on the live bank by the Do_Castrate/Do_HardHead strike cores.
pub const HUMAN_PROPERTY_ENERGY: usize = 1;
/// Original player actor kind (mafia-re 113b5b6 game/actors/C_actor.h:19
/// ACTOR_PLAYER 2): scanned by the IsThisCarStealing claimant loop.
pub const ACTOR_PLAYER: u32 = 2;
/// Original entity actor kind (mafia-re 113b5b6 game/actors/C_actor.h:41
/// ACTOR_ENTITY 0x1b): scanned by the IsThisCarStealing claimant loop.
pub const ACTOR_ENTITY: u32 = 0x1b;
/// Original empty-hands weapon kind (mafia-re 113b5b6 game/g_material.h:170
/// WEAPON_KIND_FISTS 0): the corner-fire guard refuses rifles and fists.
pub const WEAPON_KIND_FISTS: u32 = 0;
/// Original rifle weapon kind (mafia-re 113b5b6 game/g_material.h:172
/// WEAPON_KIND_RIFLE 2, the Springfield and the Mosin-Nagant): refused by
/// the corner-fire guard.
pub const WEAPON_KIND_RIFLE: u32 = 2;
/// Original Thompson fire-animation number (mafia-re 113b5b6
/// game/g_material.h:183 GUN_ANIM_THOMPSON 4): the corner fire selects its
/// own SAP animation for it when standing.
pub const GUN_ANIM_THOMPSON: u32 = 4;
/// Original head hit-part value (mafia-re 113b5b6 game/actors/C_human.h:53
/// HIT_PART_HEAD 6): selects the head branch of GetAnimIDForDeath.
pub const HIT_PART_HEAD: u32 = 6;
/// Original friend human type (mafia-re 113b5b6 game/actors/C_human.h:58
/// HUMAN_TYPE_FRIEND 2): the strike cheat gate spares friends.
pub const HUMAN_TYPE_FRIEND: u32 = 2;
/// Original Formula get-in animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:744: 0x13c, "FormuleNastupL.i3d"). Read by the
/// GetAnimIDForUseCar select; the animation owner stays DEFERRED.
pub const ANIM_FORMULE_NASTUP_L: i32 = 0x13c;
/// Original Formula get-out animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:745: 0x13d, "FormuleVystupL.i3d"). Read by the
/// GetAnimIDForUseCar select; the animation owner stays DEFERRED.
pub const ANIM_FORMULE_VYSTUP_L: i32 = 0x13d;
/// Original car get-in base animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:730: 0x10f, "AutoSmNas FL.i3d"). Base of the
/// GetAnimIDForUseCar bit-built select; the animation owner stays DEFERRED.
pub const ANIM_AUTO_SM_NAS_FL: i32 = 0x10f;
/// Original crouched death-from-behind animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:647: 0x8c, "smrt drep zada A.I3D"). Read by the
/// GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_DREP_ZADA_A: i32 = 0x8c;
/// Original crouched death-from-front animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:648: 0x8e, "smrt drep bricho A.I3D"). Read by the
/// GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_DREP_BRICHO_A: i32 = 0x8e;
/// Original crouched head-death-from-behind animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:649: 0x90, "smrt drep hlava zezadu.I3D"). Read by
/// the GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_DREP_HLAVA_ZEZADU: i32 = 0x90;
/// Original crouched head-death-from-front animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:650: 0x91, "smrt drep hlava zepredu.I3D"). Read by
/// the GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_DREP_HLAVA_ZEPREDU: i32 = 0x91;
/// Original standing death-from-behind animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:643: 0x83, "smrt stoj zada A.I3D"). Read by the
/// GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_STOJ_ZADA_A: i32 = 0x83;
/// Original standing death-from-front animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:644: 0x86, "smrt stoj bricho A.i3d"). Read by the
/// GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_STOJ_BRICHO_A: i32 = 0x86;
/// Original standing head-death-from-behind animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:645: 0x89, "smrt stoj hlava zezadu.i3d"). Read by
/// the GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_STOJ_HLAVA_ZEZADU: i32 = 0x89;
/// Original standing head-death-from-front animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:646: 0x8a, "smrt stoj hlava zepredu.I3D"). Read by
/// the GetAnimIDForDeath select; the animation owner stays DEFERRED.
pub const ANIM_SMRT_STOJ_HLAVA_ZEPREDU: i32 = 0x8a;
/// Original groin-strike animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:710: 0xf1, "boj box rozkrok.i3d"). Written by the
/// Do_Castrate core; the PersonAnim remap stays DEFERRED.
pub const ANIM_BOJ_BOX_ROZKROK: i32 = 0xf1;
/// Original groin-strike victim animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:711: 0xf2, "boj zasah rozkrok sm.i3d"). Written by
/// the Do_Castrate core on the target; the PersonAnim remap stays DEFERRED.
pub const ANIM_BOJ_ZASAH_ROZKROK_SM: i32 = 0xf2;
/// Original headbutt animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:712: 0xf3, "boj box hlavicka.i3d"). Written by the
/// Do_HardHead core; the PersonAnim remap stays DEFERRED.
pub const ANIM_BOJ_BOX_HLAVICKA: i32 = 0xf3;
/// Original headbutt victim animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:713: 0xf4, "boj zasah hlavicka.i3d"). Written by
/// the Do_HardHead core on the target; the PersonAnim remap stays DEFERRED.
pub const ANIM_BOJ_ZASAH_HLAVICKA: i32 = 0xf4;
/// Original startle animation id, first variant (mafia-re 113b5b6
/// game/actors/C_actor.h:594: 0x2f, "panika leknuti01L.i3d"). Written by the
/// Do_ShockStart core on a nonzero draw; the PersonAnim remap stays
/// DEFERRED.
pub const ANIM_PANIKA_LEKNUTI01L: i32 = 0x2f;
/// Original startle animation id, second variant (mafia-re 113b5b6
/// game/actors/C_actor.h:595: 0x33, "panika leknuti02L.i3d"). Written by the
/// Do_ShockStart core on a zero draw; the PersonAnim remap stays DEFERRED.
pub const ANIM_PANIKA_LEKNUTI02L: i32 = 0x33;
/// Original standing left corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:611: 0x53, "enm roh pis stoj fire01 L.i3d").
/// Written by the Do_CornerFire core; the PersonAnim/SetWaitAnim remap
/// stays DEFERRED.
pub const ANIM_ENM_ROH_FIRE01_L: i32 = 0x53;
/// Original standing left Thompson corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:612: 0x54, "enm roh sap stoj fire01 L.i3d").
/// Written by the Do_CornerFire core for the Thompson while standing.
pub const ANIM_ENM_ROH_SAP_STOJ_FIRE01_L: i32 = 0x54;
/// Original standing right corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:613: 0x55, "enm roh pis stoj fire01 R.i3d").
/// Written by the Do_CornerFire core; the PersonAnim/SetWaitAnim remap
/// stays DEFERRED.
pub const ANIM_ENM_ROH_FIRE01_R: i32 = 0x55;
/// Original standing right Thompson corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:614: 0x56, "enm roh sap stoj fire01 R.i3d").
/// Written by the Do_CornerFire core for the Thompson while standing.
pub const ANIM_ENM_ROH_SAP_STOJ_FIRE01_R: i32 = 0x56;
/// Original crouched corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:615: 0x57, "enm roh pis drep fire01 T.i3d").
/// Written by the Do_CornerFire core on the crouched CORNER_2 path.
pub const ANIM_ENM_ROH_DREP_FIRE01_T: i32 = 0x57;
/// Original standing left aimed corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:616: 0x58, "enm roh pis stoj fire02 L.i3d").
/// Written by the Do_CornerFireAimed core; the PersonAnim remap stays
/// DEFERRED.
pub const ANIM_ENM_ROH_FIRE02_L: i32 = 0x58;
/// Original standing left Thompson aimed corner-fire animation id (mafia-re
/// 113b5b6 game/actors/C_actor.h:617: 0x59, "enm roh sap stoj fire02
/// L.i3d"). Written by the Do_CornerFireAimed core for the Thompson while
/// standing.
pub const ANIM_ENM_ROH_SAP_STOJ_FIRE02_L: i32 = 0x59;
/// Original standing right aimed corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:618: 0x5a, "enm roh pis stoj fire02 R.i3d").
/// Written by the Do_CornerFireAimed core; the PersonAnim remap stays
/// DEFERRED.
pub const ANIM_ENM_ROH_FIRE02_R: i32 = 0x5a;
/// Original standing right Thompson aimed corner-fire animation id (mafia-re
/// 113b5b6 game/actors/C_actor.h:619: 0x5b, "enm roh sap stoj fire02
/// R.i3d"). Written by the Do_CornerFireAimed core for the Thompson while
/// standing.
pub const ANIM_ENM_ROH_SAP_STOJ_FIRE02_R: i32 = 0x5b;
/// Original crouched aimed corner-fire animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:620: 0x5c, "enm roh pis drep fire02 T.i3d").
/// Written by the Do_CornerFireAimed core on the crouched CORNER_2 path.
pub const ANIM_ENM_ROH_DREP_FIRE02_T: i32 = 0x5c;
/// Pocket count the truncate core iterates (mafia-re 113b5b6
/// game/actors/g_inventory.h:41 INVENTORY_POCKETS 5, iterated at
/// game/actors/C_human.cpp:6138).
pub const INVENTORY_POCKETS: usize = 5;
/// Bloody-footprint track texture index (mafia-re 113b5b6
/// game/mission/game.h:92-94: TRACK_TEXTURE_BFOOTPRINT 6, the
/// C_game::track_textures index of "2bfootprint.bmp", given to the steps of
/// a human through blood by cbCollTester).
pub const TRACK_TEXTURE_BFOOTPRINT: u8 = 6;
/// Original standing stills action value for the animation-step player
/// (mafia-re 113b5b6 game/actors/C_human.h:142 HUMAN_ACTION_STEPS 6):
/// the Do_ShockDown core selects it after filling the steps.
pub const HUMAN_ACTION_STEPS: i32 = 6;
/// Original animation-step kinds (mafia-re 113b5b6
/// game/actors/C_human.h:168-171 HUMAN_STEP_WAIT 0, HUMAN_STEP_ANIM 1,
/// HUMAN_STEP_AGONY 2, HUMAN_STEP_END 3): the Do_ShockDown core writes ANIM
/// and END steps.
pub const HUMAN_STEP_WAIT: i32 = 0;
/// Original play-animation step kind (mafia-re 113b5b6
/// game/actors/C_human.h:169 HUMAN_STEP_ANIM 1).
pub const HUMAN_STEP_ANIM: i32 = 1;
/// Original end-of-steps kind (mafia-re 113b5b6
/// game/actors/C_human.h:171 HUMAN_STEP_END 3).
pub const HUMAN_STEP_END: i32 = 3;
/// Original animation-id low-bits mask (mafia-re 113b5b6
/// game/actors/C_actor.h:535 ANIM_BASE 0x1ff): the ChangeDeadAnim select
/// masks the carrier's next id with it.
pub const ANIM_BASE: i32 = 0x1ff;
/// Original left-step animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:555: 0x8, "left1.i3d"). Read by the ChangeDeadAnim
/// select; the animation owner stays DEFERRED.
pub const ANIM_LEFT1: i32 = 0x8;
/// Original right-step animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:556: 0x9, "right1.i3d"). Read by the ChangeDeadAnim
/// select; the animation owner stays DEFERRED.
pub const ANIM_RIGHT1: i32 = 0x9;
/// Original panic-crouch-down animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:597: 0x3d, "x panika drep dolu.i3d"). Base of the
/// Do_ShockDown select (`i * 3 +` it); the PersonAnim remap stays DEFERRED.
pub const ANIM_X_PANIKA_DREP_DOLU: i32 = 0x3d;
/// Original carried-dead pickup animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:657: 0xa7, "NoseniMrtvolyTomNahozeni.i3d"). Read
/// by the ChangeDeadAnim select; the animation owner stays DEFERRED.
pub const ANIM_NOSENI_MRTVOLY_TOM_NAHOZENI: i32 = 0xa7;
/// Original active collision set selector (mafia-re 113b5b6
/// game/actors/C_human.h:308-310 HUMAN_COLLS_ALL 0, HUMAN_COLLS_UPPER 1,
/// HUMAN_COLLS_LOWER 2): the ActivateColls mode.
pub const HUMAN_COLLS_ALL: i32 = 0;
/// Original upper-collisions-only selector (mafia-re 113b5b6
/// game/actors/C_human.h:309 HUMAN_COLLS_UPPER 1).
pub const HUMAN_COLLS_UPPER: i32 = 1;
/// Original lower-collisions-only selector (mafia-re 113b5b6
/// game/actors/C_human.h:310 HUMAN_COLLS_LOWER 2).
pub const HUMAN_COLLS_LOWER: i32 = 2;
/// Original lower-sphere collision flag (mafia-re 113b5b6
/// game/collision/g_colls.h:49 DYNCOLLF_LOWER 0x1): the ActivateColls LOWER
/// arm and the DeactivateColls keep-upper arm test it.
pub const DYNCOLLF_LOWER: u32 = 0x1;
/// Original upper-sphere collision flag (mafia-re 113b5b6
/// game/collision/g_colls.h:50 DYNCOLLF_UPPER 0x2): the ActivateColls ALL and
/// UPPER arms and the DeactivateColls keep-upper arm test it.
pub const DYNCOLLF_UPPER: u32 = 0x2;
/// Original cutscene-recorded actor flag (mafia-re 113b5b6
/// game/actors/C_actor.h:82 ACTF_CUTSCENE_RECORDED 0x2): the SetCutsceneLink
/// gate. The act_flags word stays with the actor owner (DEFERRED); the
/// caller stages the verdict.
pub const ACTF_CUTSCENE_RECORDED: i32 = 0x2;
/// Cutscene link name capacity (mafia-re 113b5b6
/// game/actors/C_actor.h:97: `char name[0x24]` in S_cutscene_link).
pub const CUTSCENE_LINK_NAME_LEN: usize = 0x24;

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

/// Corners a human looks, fires and breathes around (mafia-re 113b5b6
/// game/actors/C_human.h:464-470: PS2 C_human::E_corner_view, CORNER_0,
/// CORNER_1, CORNER_2 with the crouched-only note; the upstream values are
/// not evidenced beyond the declaration order, so the plain 0/1/2 order is
/// the working representation). Read by the Do_CornerBreath/Do_CornerView/
/// Do_CornerFire/Do_CornerFireAimed cores of this slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CornerView {
    Corner0 = 0,
    Corner1 = 1,
    Corner2 = 2,
}

/// Caller-supplied projection of one active actor for the IsThisCarStealing
/// claimant scan (mafia-re 113b5b6 game/actors/C_human.cpp:9597-9606):
/// the actor kind (ACTOR_PLAYER/ACTOR_ENTITY pass the gate) plus the three
/// car latches the scan compares (`stealing_car`, `car`, `using_car`; None
/// for NULL). The active-actor list owner stays DEFERRED.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CarTheftClaimant {
    pub actor_type: u32,
    pub stealing_car: Option<ActorReferenceId>,
    pub car: Option<ActorReferenceId>,
    pub using_car: Option<ActorReferenceId>,
}

/// One staged C_human animation step (mafia-re 113b5b6
/// game/actors/C_human.h:179 struct S_anim_step, 0x14 bytes; the field
/// names are ours): the kind (`type`), the played id, the wait time in ms,
/// the PersonAnim stance and the step flags. Do_ShockDown fills the first
/// two slots and ends the third; the Movement HUMAN_ACTION_STEPS player
/// stays DEFERRED.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HumanAnimStep {
    pub step_type: i32,
    pub anim_id: i32,
    pub wait_ms: i32,
    pub stance: bool,
    pub flags: i32,
}

impl HumanAnimStep {
    /// Deterministic idle step (working default; the native constructor
    /// leaves the words untouched): a WAIT of nothing.
    pub(crate) fn idle() -> Self {
        Self {
            step_type: HUMAN_STEP_WAIT,
            anim_id: 0,
            wait_ms: 0,
            stance: false,
            flags: 0,
        }
    }
}

/// Staged identity of the C_human fire-action table (mafia-re 113b5b6
/// game/actors/C_human.h:744-747: `const S_fire_action *fire_action` @0x1f0
/// with `fire_step` @0x1f4 and `fire_time` @0x1f8; the table contents live
/// with the weapon owner). Only the tables ResetWeaponEffects and
/// RepairWeaponPosition compare against are named; any other table stages
/// as Custom (no invented table contents).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FireActionTable {
    Ready,
    Aimed,
    OnOff,
    Custom,
}

/// Staged C_human cutscene link (mafia-re 113b5b6
/// game/actors/C_actor.h:93-100 struct S_cutscene_link: `is_new` @0x0,
/// `kind` @0x4, `name` @0x8, `pos` @0x2c, `rot` @0x38; the field names are
/// ours). SetCutsceneLink allocates and fills one while the human is
/// recorded; RecordKey consumes `is_new`. The frame placement and the
/// linked actor's frame name arrive from the caller because both frame
/// owners stay DEFERRED.
#[derive(Clone, Debug, PartialEq)]
pub struct HumanCutsceneLink {
    pub is_new: bool,
    pub kind: i32,
    pub name: [u8; CUTSCENE_LINK_NAME_LEN],
    pub pos: [f32; 3],
    pub rot: [f32; 4],
}

/// Caller-supplied projection of one actor for the Can_SpecialStroke scan
/// (mafia-re 113b5b6 game/actors/C_human.cpp:7190): the actor kind
/// (ACTOR_PLAYER/ACTOR_ENTITY pass the gate), whether its state is active
/// (stages `!act_state`, ACTSTATE_ACTIVE 0), its crouch latch, the IsReady
/// inputs (alive, action word, animation-machine blocking verdict) and the
/// property words the strike gate compares (`properties` strength on the
/// searcher side, `properties` energy here), plus the world position and
/// facing the distance/angle gates read. The global-actor list, the frames
/// and the random-free IsReady machine stay DEFERRED.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SpecialStrokeCandidate {
    pub actor_type: u32,
    pub active_state: bool,
    pub crouching: bool,
    pub alive: bool,
    pub action_state: i32,
    pub anim_blocking: bool,
    pub energy: f32,
    pub position: [f32; 3],
    pub facing: [f32; 3],
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
    /// Walk velocity per second (mafia-re 113b5b6 game/actors/C_human.h:839
    /// -841: 0x420 S_vector velocity, PS2 0xc0; C_human.cpp:5395 scales the
    /// facing dir by the Go_* base speeds). Written by the Do_Breath/Go_*
    /// cores of this slice; MovementIntern/Update motion effects stay
    /// DEFERRED.
    velocity: [f32; 3],
    /// Facing direction the Go_* cores scale (the frame direction; the frame
    /// owner stays DEFERRED, so callers stage it via `set_dir`). Deterministic
    /// working default zero; no original default is evidenced on this path.
    dir: [f32; 3],
    /// Turn speed in radians per second (mafia-re 113b5b6
    /// game/actors/C_human.h:842-844: 0x42c turn_speed; C_human.cpp:5417,
    /// :5426, :5381 stop it). Cleared by Do_Breath/Go_Left/Go_Right; Update's
    /// dir integration stays DEFERRED.
    turn_speed: f32,
    /// Crouch latch (mafia-re 113b5b6 game/actors/C_human.h:728: 0x1e4
    /// crouching). Forces the walk variants of Go_Front/Left/Right
    /// (C_human.cpp:5392,5414,5431); the Do_Crouched writer stays DEFERRED,
    /// callers stage it via `set_crouching`.
    crouching: bool,
    /// Box-carried latch staging the box frame pointer (mafia-re 113b5b6
    /// game/actors/C_human.h:1008: 0x6f8 I3D_frame *box). Same guard role as
    /// crouching plus the Do_Aimed clear (C_human.cpp:6082); the box frame
    /// owner stays DEFERRED.
    carrying_box: bool,
    /// Dead-body-carried latch staging the dead-body frame pointer (mafia-re
    /// 113b5b6 game/actors/C_human.h:1010: 0x700 I3D_frame *dead_body). Same
    /// guard role as the box (C_human.cpp:5392,5414,5431,6082); the body/cart
    /// owners stay DEFERRED.
    carrying_dead_body: bool,
    /// Aim latch (mafia-re 113b5b6 game/actors/C_human.h:729-731: 0x1e5
    /// aimed). Written by the Do_Aimed core of this slice; the Do_Shoot,
    /// reload and weapon-change writers stay DEFERRED.
    aimed: bool,
    /// Melee-guard latch (mafia-re 113b5b6 game/actors/C_human.h:780-783:
    /// 0x22c guarding, ended by Do_Aimed(false) at C_human.cpp:6090-6094).
    /// Cleared here through the DelExtendedAnim marker; the Update and
    /// SetFreeBoxingAnim writers stay DEFERRED.
    guarding: bool,
    /// Weapon-change-in-progress latch (mafia-re 113b5b6
    /// game/actors/C_human.h:794-796: 0x230 changing_weapon). Read by the
    /// Do_Aimed core for the ChangeWeaponModel marker (C_human.cpp:6087); the
    /// Do_ChangeWeapon and NOTE_CHANGE_WEAPON writers stay DEFERRED.
    changing_weapon: bool,
    /// Marker-only ChangeWeaponModel call count (mafia-re 113b5b6
    /// game/actors/C_human.h:574; C_human.cpp:6087-6088). The weapon model,
    /// sound and kind effects stay with the weapon owner (DEFERRED).
    change_weapon_model_calls: u32,
    /// Marker-only DelExtendedAnim call count (Do_Aimed passes 4.0f;
    /// mafia-re 113b5b6 game/actors/C_human.cpp:6092). The animation effect
    /// stays with the animation owner (DEFERRED).
    del_extended_anim_calls: u32,
    /// Blend of the last staged DelExtendedAnim marker (4.0f per
    /// C_human.cpp:6092), None before the first staged call.
    del_extended_anim_blend: Option<f32>,
    /// Look-frame latch (mafia-re 113b5b6 game/actors/C_human.h:822:
    /// 0x2d8 I3D_frame *look_frame; SetLookFrame GAME 0x47b7a0 writes it).
    /// The camera/frame readers stay DEFERRED. Starts at none.
    look_frame: Option<FrameReferenceId>,
    /// HUD wingman indicator words (mafia-re 113b5b6
    /// game/actors/C_human.h:993-995: 0x6d8 int wingman, -1 for none, and
    /// 0x6dc uint wingman_name, 0 to 7; SetWingman GAME 0x47b7c0 writes
    /// both). The indicator consumer stays DEFERRED. Starts at none/0.
    wingman: i32,
    wingman_name: u32,
    /// Script fire-target latch (mafia-re 113b5b6 game/actors/C_human.h:996:
    /// 0x6e0 I3D_frame *fire_target, HUMAN_SETFIRETARGET's, NULL for none;
    /// SetFireTarget GAME 0x47b7e0 writes it). The fire writer stays
    /// DEFERRED. Starts at none.
    fire_target: Option<FrameReferenceId>,
    /// No-anim-hit latch (mafia-re 113b5b6 game/actors/C_human.h:1022:
    /// 0x720 no_anim_hit; SetNoAnimHit GAME 0x47b7f0 writes it). The hit
    /// animation gate stays DEFERRED. Starts clear.
    no_anim_hit: bool,
    /// Idle-animation latch (mafia-re 113b5b6 game/actors/C_human.h:926:
    /// 0x598 idle_anims; SetIdleAnims GAME 0x47b800 writes it). The Update
    /// idle-animation producer stays DEFERRED. Starts clear.
    idle_anims: bool,
    /// Shut-up latch (mafia-re 113b5b6 game/actors/C_human.h:963: 0x684
    /// shut_up, HUMAN_SHUTUP's; SetShutUp GAME 0x47b850 writes it). The
    /// NewScream gate stays DEFERRED. Starts clear.
    shut_up: bool,
    /// Held-shot latch (mafia-re 113b5b6 game/actors/C_human.h:737: 0x1ec,
    /// the shot or strike held; Do_Shoot sets it on the press and clears it
    /// on the release). Cleared by the ResetActions core of this slice; the
    /// Do_Shoot writers and the INDF_THIEF indicator effect stay DEFERRED.
    /// Starts clear.
    fire_held: bool,
    /// Active-collision latch (mafia-re 113b5b6 game/actors/C_human.h:968:
    /// 0x694 colls_active; ActivateColls sets it). Written by the
    /// ActivateColls/DeactivateColls select slices of this slice; the
    /// colls/colls[i] array and every DynInsert/DynErase/InvalidateColl
    /// effect stay DEFERRED with the collision owner. Starts clear (working
    /// deterministic default; the native constructor leaves it untouched).
    colls_active: bool,
    /// Animation-step staging (mafia-re 113b5b6 game/actors/C_human.h:806:
    /// `S_anim_step anim_steps[5]` @0x23c, played by Movement under
    /// HUMAN_ACTION_STEPS). Do_ShockDown fills the first two slots and ends
    /// the third; the Movement player stays DEFERRED. Starts idle (working
    /// deterministic default; the native constructor leaves the words
    /// untouched).
    anim_steps: [HumanAnimStep; 5],
    /// Current step index into the staged steps (mafia-re 113b5b6
    /// game/actors/C_human.h:807: 0x2a0 int anim_step). Written by the
    /// Do_ShockDown core of this slice; the Movement advance stays
    /// DEFERRED. Starts at 0.
    anim_step: i32,
    /// Fire-action table identity (mafia-re 113b5b6
    /// game/actors/C_human.h:744-747: `const S_fire_action *fire_action`
    /// @0x1f0 with `fire_step` @0x1f4 and `fire_time` @0x1f8). The
    /// ResetWeaponEffects core of this slice selects Ready and clears the
    /// step and the time; the table contents, the weapon-model frame
    /// effects and every Shooting/Do_Reload writer stay DEFERRED with the
    /// weapon owner. Starts at Custom (working deterministic default
    /// meaning "no known table claimed"; the native constructor leaves the
    /// pointer untouched).
    fire_action: FireActionTable,
    /// Current fire-action step (mafia-re 113b5b6
    /// game/actors/C_human.h:746: 0x1f4 int fire_step). Cleared by the
    /// ResetWeaponEffects core of this slice; the fire writers stay
    /// DEFERRED. Starts at 0.
    fire_step: i32,
    /// Fire-action step time (mafia-re 113b5b6 game/actors/C_human.h:747:
    /// 0x1f8 int fire_time). Cleared by the ResetWeaponEffects core of this
    /// slice; the fire writers stay DEFERRED. Starts at 0.
    fire_time: i32,
    /// Voice priority word (mafia-re 113b5b6 game/actors/C_human.h:991:
    /// 0x6d0 int voice_priority, -1 for none). Written by the NewScream
    /// gate of this slice; the voice/sound-cache owners stay DEFERRED.
    /// Starts at -1 (none).
    voice_priority: i32,
    /// Cutscene-recorded staging of the act_flags ACTF_CUTSCENE_RECORDED bit
    /// (mafia-re 113b5b6 game/actors/C_actor.h:82; the act_flags word stays
    /// with the actor owner, DEFERRED). Gates the SetCutsceneLink core of
    /// this slice. Starts clear.
    cutscene_recorded: bool,
    /// Staged cutscene link (mafia-re 113b5b6 game/actors/C_actor.h:93-100
    /// struct S_cutscene_link). Written by the SetCutsceneLink core of this
    /// slice while recorded; the RecordKey consumer stays DEFERRED. Starts
    /// at none.
    cutscene_link: Option<HumanCutsceneLink>,
    /// Weapon-kind word (mafia-re 113b5b6 game/g_material.h:167-177:
    /// S_item_def kind, WEAPON_KIND_*). Read by the Do_CornerFire and
    /// Do_CornerFireAimed guards of this slice; the GameInit writer (FISTS)
    /// and every ChangeWeaponModel/inventory writer stay DEFERRED. Starts at
    /// FISTS (working deterministic default matching the GameInit write;
    /// the native constructor leaves the word untouched).
    weapon_kind: u32,
    /// Staged fire-animation number of the right-hand weapon (mafia-re 113b5b6
    /// game/actors/g_material.h:194-196: S_item_def fire_anim, GUN_ANIM_*;
    /// read as inventory.GetWeaponInRightHand()->fire_anim). Read by the
    /// corner-fire Thompson selects of this slice; the inventory Select/
    /// Holster/Insert placement writers stay DEFERRED. Starts at 0 (no
    /// Thompson select).
    right_hand_fire_anim: u32,
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
            velocity: [0.0; 3],
            dir: [0.0; 3],
            turn_speed: 0.0,
            crouching: false,
            carrying_box: false,
            carrying_dead_body: false,
            aimed: false,
            guarding: false,
            changing_weapon: false,
            change_weapon_model_calls: 0,
            del_extended_anim_calls: 0,
            del_extended_anim_blend: None,
            look_frame: None,
            wingman: -1,
            wingman_name: 0,
            fire_target: None,
            no_anim_hit: false,
            idle_anims: false,
            shut_up: false,
            fire_held: false,
            colls_active: false,
            anim_steps: [HumanAnimStep::idle(); 5],
            anim_step: 0,
            fire_action: FireActionTable::Custom,
            fire_step: 0,
            fire_time: 0,
            voice_priority: -1,
            cutscene_recorded: false,
            cutscene_link: None,
            weapon_kind: WEAPON_KIND_FISTS,
            right_hand_fire_anim: 0,
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
    /// Walk velocity per second staged by the Do_Breath/Go_* cores.
    pub fn velocity(&self) -> [f32; 3] {
        self.velocity
    }
    /// Facing direction staged for the Go_* cores (frame owner DEFERRED).
    pub fn dir(&self) -> [f32; 3] {
        self.dir
    }
    /// Test/host setter for the facing direction (frame owner DEFERRED).
    pub fn set_dir(&mut self, dir: [f32; 3]) {
        self.dir = dir;
    }
    /// Turn speed in radians per second.
    pub fn turn_speed(&self) -> f32 {
        self.turn_speed
    }
    /// Crouch latch forcing the Go_* walk variants.
    pub fn crouching(&self) -> bool {
        self.crouching
    }
    /// Test/host setter for the crouch latch (Do_Crouched writer DEFERRED).
    pub fn set_crouching(&mut self, crouching: bool) {
        self.crouching = crouching;
    }
    /// Box-carried latch (box frame owner DEFERRED).
    pub fn carrying_box(&self) -> bool {
        self.carrying_box
    }
    /// Test/host setter for the box-carried latch.
    pub fn set_carrying_box(&mut self, carrying: bool) {
        self.carrying_box = carrying;
    }
    /// Dead-body-carried latch (body/cart owners DEFERRED).
    pub fn carrying_dead_body(&self) -> bool {
        self.carrying_dead_body
    }
    /// Test/host setter for the dead-body-carried latch.
    pub fn set_carrying_dead_body(&mut self, carrying: bool) {
        self.carrying_dead_body = carrying;
    }
    /// Aim latch written by the Do_Aimed core.
    pub fn aimed(&self) -> bool {
        self.aimed
    }
    /// Melee-guard latch cleared by Do_Aimed(false).
    pub fn guarding(&self) -> bool {
        self.guarding
    }
    /// Test/host setter for the melee-guard latch (Update/SetFreeBoxingAnim
    /// writers DEFERRED).
    pub fn set_guarding(&mut self, guarding: bool) {
        self.guarding = guarding;
    }
    /// Weapon-change-in-progress latch read by the Do_Aimed core.
    pub fn changing_weapon(&self) -> bool {
        self.changing_weapon
    }
    /// Test/host setter for the weapon-change latch (Do_ChangeWeapon/NOTE
    /// writers DEFERRED).
    pub fn set_changing_weapon(&mut self, changing: bool) {
        self.changing_weapon = changing;
    }
    /// Staged ChangeWeaponModel call count (weapon owner DEFERRED).
    pub fn change_weapon_model_calls(&self) -> u32 {
        self.change_weapon_model_calls
    }
    /// Staged DelExtendedAnim call count (animation owner DEFERRED).
    pub fn del_extended_anim_calls(&self) -> u32 {
        self.del_extended_anim_calls
    }
    /// Blend of the last staged DelExtendedAnim marker, if any.
    pub fn del_extended_anim_blend(&self) -> Option<f32> {
        self.del_extended_anim_blend
    }
    /// GAME 0x47b7a0 (RVA 0x7b7a0) C_human::SetLookFrame (mafia-re 113b5b6
    /// game/actors/C_human.h:1114-1118; field game/actors/C_human.h:822):
    /// store the look frame, None for NULL.
    pub fn set_look_frame(&mut self, frame: Option<FrameReferenceId>) {
        self.look_frame = frame;
    }
    /// Look frame staged by SetLookFrame, None for NULL.
    pub fn look_frame(&self) -> Option<FrameReferenceId> {
        self.look_frame
    }
    /// GAME 0x47b7c0 (RVA 0x7b7c0) C_human::SetWingman (mafia-re 113b5b6
    /// game/actors/C_human.h:1124-1129; fields game/actors/C_human.h:993
    /// -995): store the HUD wingman indicator index and name.
    pub fn set_wingman(&mut self, index: i32, name: u32) {
        self.wingman = index;
        self.wingman_name = name;
    }
    /// Wingman indicator index (-1 for none) staged by SetWingman.
    pub fn wingman(&self) -> i32 {
        self.wingman
    }
    /// Wingman indicator name (0 to 7) staged by SetWingman.
    pub fn wingman_name(&self) -> u32 {
        self.wingman_name
    }
    /// GAME 0x47b7e0 (RVA 0x7b7e0) C_human::SetFireTarget (mafia-re 113b5b6
    /// game/actors/C_human.h:1130-1134; field game/actors/C_human.h:996):
    /// store the script fire target, None for NULL.
    pub fn set_fire_target(&mut self, frame: Option<FrameReferenceId>) {
        self.fire_target = frame;
    }
    /// Script fire target staged by SetFireTarget, None for NULL.
    pub fn fire_target(&self) -> Option<FrameReferenceId> {
        self.fire_target
    }
    /// GAME 0x47b7f0 (RVA 0x7b7f0) C_human::SetNoAnimHit (mafia-re 113b5b6
    /// game/actors/C_human.h:1135-1139; field game/actors/C_human.h:1022):
    /// store the no-anim-hit latch.
    pub fn set_no_anim_hit(&mut self, on: bool) {
        self.no_anim_hit = on;
    }
    /// No-anim-hit latch staged by SetNoAnimHit.
    pub fn no_anim_hit(&self) -> bool {
        self.no_anim_hit
    }
    /// GAME 0x47b800 (RVA 0x7b800) C_human::SetIdleAnims (mafia-re 113b5b6
    /// game/actors/C_human.h:1140-1144; field game/actors/C_human.h:926):
    /// store the idle-animation latch.
    pub fn set_idle_anims(&mut self, value: bool) {
        self.idle_anims = value;
    }
    /// Idle-animation latch staged by SetIdleAnims.
    pub fn idle_anims(&self) -> bool {
        self.idle_anims
    }
    /// GAME 0x47b850 (RVA 0x7b850) C_human::SetShutUp (mafia-re 113b5b6
    /// game/actors/C_human.h:1152-1156; field game/actors/C_human.h:963):
    /// store the shut-up latch.
    pub fn set_shut_up(&mut self, on: bool) {
        self.shut_up = on;
    }
    /// Shut-up latch staged by SetShutUp.
    pub fn shut_up(&self) -> bool {
        self.shut_up
    }
    /// Test/host setter for the held-shot latch (Do_Shoot press/release
    /// writers DEFERRED; the ResetActions core of this slice clears it).
    pub fn set_fire_held(&mut self, held: bool) {
        self.fire_held = held;
    }
    /// Held-shot latch cleared by the ResetActions core.
    pub fn fire_held(&self) -> bool {
        self.fire_held
    }
    /// Test/host setter for the weapon-kind staging (GameInit/inventory
    /// writers DEFERRED; the corner-fire guards of this slice read it).
    pub fn set_weapon_kind(&mut self, kind: u32) {
        self.weapon_kind = kind;
    }
    /// Weapon-kind staging read by the corner-fire guards.
    pub fn weapon_kind(&self) -> u32 {
        self.weapon_kind
    }
    /// Test/host setter for the right-hand fire-animation staging
    /// (inventory placement writers DEFERRED).
    pub fn set_right_hand_fire_anim(&mut self, fire_anim: u32) {
        self.right_hand_fire_anim = fire_anim;
    }
    /// Right-hand fire-animation staging read by the corner-fire Thompson
    /// selects.
    pub fn right_hand_fire_anim(&self) -> u32 {
        self.right_hand_fire_anim
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
    /// Shared car-seat guard of the Go_* cores (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5390,5402,5412,5427: `if (car && car->type ==
    /// ACTOR_CAR && seat != -1) return;`). `car_is_car_actor` carries the
    /// car-type owner's verdict (DEFERRED); the seat latch is owned here while
    /// the car/seat owner lifecycle stays DEFERRED.
    fn seated_in_car(&self, car_is_car_actor: bool) -> bool {
        self.car.is_some() && car_is_car_actor && self.seat != HUMAN_SEAT_NONE
    }
    /// GAME 0x57d310 (RVA 0x17d310) C_human::Do_Breath (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5373-5385): when alive and the action is
    /// STAND, WALK or FALL, settle to STAND, stop the turn (turn_speed 0),
    /// zero the velocity and select ANIM_BREATH01A; otherwise a no-op (early
    /// return, not even the STAND write). No PersonAnim call: the original
    /// only arms next_anim_id here. SCOPE: breath settle only. DEFERRED with
    /// explicit markers, no new owner here: the PersonAnim/Update/Movement
    /// writers of these words, collision, tick.
    pub fn apply_do_breath(&mut self) {
        if !self.alive {
            return;
        }
        if self.action_state == HUMAN_ACTION_STAND
            || self.action_state == HUMAN_ACTION_WALK
            || self.action_state == HUMAN_ACTION_FALL
        {
            self.action_state = HUMAN_ACTION_STAND;
            self.turn_speed = 0.0;
            self.velocity = [0.0; 3];
            self.next_anim_id = ANIM_BREATH01A;
        }
    }
    /// GAME 0x57d360 (RVA 0x17d360) C_human::Go_Front (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5387-5397): guarded no-op while seated in a
    /// car; crouching, a carried box or a carried dead body force the walk
    /// variant. Otherwise WALK with velocity dir * (run ? 5.0 : 1.7) *
    /// properties[SPEED] and next_anim_id RUN1/WALK1. turn_speed is
    /// deliberately untouched (the original writes none here, unlike the
    /// strafes). Returns true on admission. SCOPE: velocity/anim select only.
    /// DEFERRED with explicit markers, no new owner here: the car-type
    /// verdict arrives via `car_is_car_actor` (car/seat owner lifecycle
    /// DEFERRED), plus collision, PersonAnim, tick.
    pub fn apply_go_front(&mut self, run: bool, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let run =
            run && !self.crouching && !self.carrying_box && !self.carrying_dead_body;
        self.action_state = HUMAN_ACTION_WALK;
        let base = if run { 5.0 } else { 1.7 };
        let speed = base * self.current_properties.speed_factor();
        self.velocity = [
            self.dir[0] * speed,
            self.dir[1] * speed,
            self.dir[2] * speed,
        ];
        self.next_anim_id = if run { ANIM_RUN1 } else { ANIM_WALK1 };
        true
    }
    /// GAME 0x57d430 (RVA 0x17d430) C_human::Go_Back (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5399-5407): pinned symmetry with Go_Front: the
    /// same car-seat guard and WALK entry, but no crouch/box/dead-body arm
    /// (the original has none there), velocity -dir * 1.5 *
    /// properties[SPEED], next_anim_id BACK1, turn_speed untouched. Returns
    /// true on admission. SCOPE and DEFERRED markers as in `apply_go_front`.
    pub fn apply_go_back(&mut self, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        self.action_state = HUMAN_ACTION_WALK;
        let speed = 1.5 * self.current_properties.speed_factor();
        self.velocity = [
            -self.dir[0] * speed,
            -self.dir[1] * speed,
            -self.dir[2] * speed,
        ];
        self.next_anim_id = ANIM_BACK1;
        true
    }
    /// GAME 0x57d4d0 (RVA 0x17d4d0) C_human::Go_Left (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5409-5423): guarded no-op while seated in a
    /// car; crouching, a carried box or a carried dead body force the walk
    /// variant. Otherwise WALK with the turn stopped (turn_speed 0), the
    /// strafe perpendicular velocity.x = -dir.z, velocity.z = dir.x,
    /// velocity.y = dir.y scaled by (run ? 3.0 : 1.5) * properties[SPEED],
    /// and next_anim_id STRAFRL1/STRAFL1. Returns true on admission. SCOPE
    /// and DEFERRED markers as in `apply_go_front`.
    pub fn apply_go_left(&mut self, run: bool, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let run =
            run && !self.crouching && !self.carrying_box && !self.carrying_dead_body;
        self.action_state = HUMAN_ACTION_WALK;
        self.turn_speed = 0.0;
        let base = if run { 3.0 } else { 1.5 };
        let speed = base * self.current_properties.speed_factor();
        self.velocity = [
            -self.dir[2] * speed,
            self.dir[1] * speed,
            self.dir[0] * speed,
        ];
        self.next_anim_id = if run {
            ANIM_STRAFRL1
        } else {
            ANIM_STRAFL1
        };
        true
    }
    /// GAME 0x57d5a0 (RVA 0x17d5a0) C_human::Go_Right (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5425-5439): mirror of Go_Left with velocity.x
    /// = dir.z, velocity.z = -dir.x, velocity.y = dir.y scaled by (run ? 3.0
    /// : 1.5) * properties[SPEED], next_anim_id STRAFRR1/STRAFR1, turn
    /// stopped, same car-seat guard. Returns true on admission. SCOPE and
    /// DEFERRED markers as in `apply_go_front`.
    pub fn apply_go_right(&mut self, run: bool, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let run =
            run && !self.crouching && !self.carrying_box && !self.carrying_dead_body;
        self.action_state = HUMAN_ACTION_WALK;
        self.turn_speed = 0.0;
        let base = if run { 3.0 } else { 1.5 };
        let speed = base * self.current_properties.speed_factor();
        self.velocity = [
            self.dir[2] * speed,
            self.dir[1] * speed,
            -self.dir[0] * speed,
        ];
        self.next_anim_id = if run {
            ANIM_STRAFRR1
        } else {
            ANIM_STRAFR1
        };
        true
    }
    /// GAME 0x57d670 (RVA 0x17d670) C_human::Go_FrontLeft (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5442): guarded no-op while seated in a car;
    /// crouching, a carried box or a carried dead body force the walk variant.
    /// Otherwise WALK with the turn stopped (turn_speed 0), the normalized
    /// (side + dir) blend (side.x = -dir.z, side.y = dir.y, side.z = dir.x)
    /// scaled by (run ? 5.0 : 1.7) * properties[SPEED] and next_anim_id
    /// RUNL1/WALKL1. Returns true on admission. SCOPE and DEFERRED markers
    /// as in `apply_go_front`.
    pub fn apply_go_front_left(&mut self, run: bool, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let run =
            run && !self.crouching && !self.carrying_box && !self.carrying_dead_body;
        let side = [-self.dir[2], self.dir[1], self.dir[0]];
        self.action_state = HUMAN_ACTION_WALK;
        self.turn_speed = 0.0;
        let blended = normalize_strafe_vector([
            side[0] + self.dir[0],
            side[1] + self.dir[1],
            side[2] + self.dir[2],
        ]);
        let base = if run { 5.0 } else { 1.7 };
        let speed = base * self.current_properties.speed_factor();
        self.velocity = [blended[0] * speed, blended[1] * speed, blended[2] * speed];
        self.next_anim_id = if run { ANIM_RUNL1 } else { ANIM_WALKL1 };
        true
    }
    /// GAME 0x57d860 (RVA 0x17d860) C_human::Go_FrontRight (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5458): mirror of Go_FrontLeft with side.x =
    /// dir.z, side.y = dir.y, side.z = -dir.x, the same car-seat guard, walk
    /// forcing, normalization, scaling and turn stop, and next_anim_id
    /// RUNR1/WALKR1. Returns true on admission. SCOPE and DEFERRED markers
    /// as in `apply_go_front`.
    pub fn apply_go_front_right(&mut self, run: bool, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let run =
            run && !self.crouching && !self.carrying_box && !self.carrying_dead_body;
        let side = [self.dir[2], self.dir[1], -self.dir[0]];
        self.action_state = HUMAN_ACTION_WALK;
        self.turn_speed = 0.0;
        let blended = normalize_strafe_vector([
            side[0] + self.dir[0],
            side[1] + self.dir[1],
            side[2] + self.dir[2],
        ]);
        let base = if run { 5.0 } else { 1.7 };
        let speed = base * self.current_properties.speed_factor();
        self.velocity = [blended[0] * speed, blended[1] * speed, blended[2] * speed];
        self.next_anim_id = if run { ANIM_RUNR1 } else { ANIM_WALKR1 };
        true
    }
    /// GAME 0x57da50 (RVA 0x17da50) C_human::Go_BackLeft (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5474): guarded no-op while seated in a car;
    /// no crouch/box/dead-body arm (the original has none here). Otherwise
    /// WALK with the turn stopped (turn_speed 0), the normalized (side +
    /// -dir) blend (side.x = -dir.z, side.y = dir.y, side.z = dir.x) scaled
    /// by properties[SPEED] * 1.5 and next_anim_id BACKL1. Returns true on
    /// admission. SCOPE and DEFERRED markers as in `apply_go_front`.
    pub fn apply_go_back_left(&mut self, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let side = [-self.dir[2], self.dir[1], self.dir[0]];
        self.action_state = HUMAN_ACTION_WALK;
        self.turn_speed = 0.0;
        let blended = normalize_strafe_vector([
            side[0] - self.dir[0],
            side[1] - self.dir[1],
            side[2] - self.dir[2],
        ]);
        let speed = self.current_properties.speed_factor() * 1.5;
        self.velocity = [blended[0] * speed, blended[1] * speed, blended[2] * speed];
        self.next_anim_id = ANIM_BACKL1;
        true
    }
    /// GAME 0x57dc10 (RVA 0x17dc10) C_human::Go_BackRight (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5488): mirror of Go_BackLeft with side.x =
    /// dir.z, side.y = dir.y, side.z = -dir.x, the same car-seat guard, WALK
    /// entry and turn stop, scaled by properties[SPEED] * 1.5 with
    /// next_anim_id BACKR1, but WITHOUT the normalization: the original only
    /// tests IsMrgZeroLess(velocity.Square()) and discards the result (noted
    /// at C_human.cpp:5496-5500 as going on like Go_BackLeft), so the raw
    /// blend is scaled directly. The discarded pure read has no observable
    /// effect and is not replicated. Returns true on admission. SCOPE and
    /// DEFERRED markers as in `apply_go_front`.
    pub fn apply_go_back_right(&mut self, car_is_car_actor: bool) -> bool {
        if self.seated_in_car(car_is_car_actor) {
            return false;
        }
        let side = [self.dir[2], self.dir[1], -self.dir[0]];
        self.action_state = HUMAN_ACTION_WALK;
        self.turn_speed = 0.0;
        let speed = self.current_properties.speed_factor() * 1.5;
        self.velocity = [
            (side[0] - self.dir[0]) * speed,
            (side[1] - self.dir[1]) * speed,
            (side[2] - self.dir[2]) * speed,
        ];
        self.next_anim_id = ANIM_BACKR1;
        true
    }
    /// GAME 0x57f830 (RVA 0x17f830) C_human::Do_Aimed (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6079-6095): carrying a box or a dead body
    /// clears the aim latch and returns; an aim request while changing the
    /// weapon stages a ChangeWeaponModel marker; the latch takes `aim`;
    /// leaving aim while guarding stages a DelExtendedAnim(4.0) marker and
    /// clears the guard. Marker-only: the weapon-model/sound/kind effects
    /// (ChangeWeaponModel, C_human.h:574) and the extended-animation effect
    /// (DelExtendedAnim) stay with their owners (DEFERRED); the staged counts
    /// and blend expose the calls. SCOPE: aim latch plus call markers only.
    /// DEFERRED with explicit markers, no new owner here: collision, tick,
    /// and the Do_Shoot/reload/change writers of the latch.
    pub fn apply_do_aimed(&mut self, aim: bool) {
        if self.carrying_box || self.carrying_dead_body {
            self.aimed = false;
            return;
        }
        if aim && self.changing_weapon {
            self.change_weapon_model_calls += 1;
        }
        self.aimed = aim;
        if !aim && self.guarding {
            self.del_extended_anim_calls += 1;
            self.del_extended_anim_blend = Some(4.0);
            self.guarding = false;
        }
    }
    /// GAME 0x57f8a0 (RVA 0x17f8a0) C_human::Do_Crouched owned latch slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:6098-6118): carrying a box
    /// or a dead body clears the crouch latch and returns; otherwise a stand
    /// request while crouched is held down (crouch stays true) unless the
    /// headroom probe clears it, and the latch takes `crouch`. The stand-up
    /// headroom probe (frame GetWorldPos + 1.0 y lift, TestSphereB radius 0.3
    /// dir (0, 0.55, 0) excluding the colls/ghosts, DynSetNextCollOwnerMult
    /// plumbing at C_human.cpp:6108-6113) has no frame or collision owner yet,
    /// so the caller supplies its verdict as `headroom_clear` (true when the
    /// probe reports no hit). SCOPE: latch only. DEFERRED with explicit
    /// markers, no new owner here: the frame and collision owners of the
    /// probe, plus the PersonAnim/Update/Movement writers of the latch.
    pub fn apply_do_crouched(&mut self, crouch: bool, headroom_clear: bool) {
        if self.carrying_box || self.carrying_dead_body {
            self.crouching = false;
            return;
        }
        let mut crouch = crouch;
        if self.crouching && !crouch && !headroom_clear {
            crouch = true;
        }
        self.crouching = crouch;
    }
    /// GAME 0x58ac50 (RVA 0x18ac50) C_human::Force_CanWorking (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:9028-9031): set action_state to
    /// HUMAN_ACTION_STAND (C_human.h:137). No guard, no anim write; the
    /// HUMAN_WAITTOREADY gate that reads it stays DEFERRED.
    pub fn apply_force_can_working(&mut self) {
        self.action_state = HUMAN_ACTION_STAND;
    }
    /// GAME 0x58ac60 (RVA 0x18ac60) C_human::Turn_Turn owned select slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:9034-9040): select
    /// next_anim_id ANIM_TURN1 and invoke PersonAnim(0.0, true, false),
    /// recorded here as the handoff for the future animation owner (same
    /// shape as the Do_StopAnim alive path). The velocity_ramp = 0 and
    /// last_velocity.Zero() writes have no owners yet and stay DEFERRED with
    /// an explicit marker; no invented motion state.
    pub fn apply_turn_turn(&mut self) {
        self.next_anim_id = ANIM_TURN1;
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
    }
    /// GAME 0x58a960 (RVA 0x18a960) C_human::Do_CornerBreath (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:8922-8936): carrying a box or a dead
    /// body refuses; CORNER_0 selects ANIM_ENM_ROH_STOJ_STAT_L and CORNER_1
    /// selects ANIM_ENM_ROH_STOJ_STAT_R (both return true); CORNER_2 has no
    /// arm and refuses (false), matching the original switch fall-through.
    /// No PersonAnim call: the original only arms next_anim_id here. SCOPE:
    /// anim select only. DEFERRED with explicit markers, no new owner here:
    /// the PersonAnim/Update/Movement writers of the word, collision, tick.
    pub fn apply_do_corner_breath(&mut self, view: CornerView) -> bool {
        if self.carrying_box || self.carrying_dead_body {
            return false;
        }
        match view {
            CornerView::Corner0 => {
                self.next_anim_id = ANIM_ENM_ROH_STOJ_STAT_L;
                true
            }
            CornerView::Corner1 => {
                self.next_anim_id = ANIM_ENM_ROH_STOJ_STAT_R;
                true
            }
            CornerView::Corner2 => false,
        }
    }
    /// GAME 0x58a9a0 (RVA 0x18a9a0) C_human::Do_CornerView owned select
    /// slice (mafia-re 113b5b6 game/actors/C_human.cpp:8939-8961): carrying
    /// a box or a dead body refuses; CORNER_0 selects
    /// ANIM_ENM_ROH_STOJ_NAHL_L and CORNER_1 selects ANIM_ENM_ROH_STOJ_NAHL_R;
    /// CORNER_2 refuses unless crouching and then selects
    /// ANIM_ENM_ROH_DREP_NAHL_T; any other view (unrepresentable here)
    /// refuses. On admission the inline SetWaitAnim runs: its PersonAnim(0.0,
    /// true, false) invocation is recorded here as the handoff for the future
    /// animation owner, while its action_time/last_velocity/velocity_ramp
    /// writes plus the HUMAN_ACTION_WAIT select (C_human.h:1160-1172) have no
    /// owners yet and stay DEFERRED with an explicit marker; no invented
    /// wait state. Returns true on admission.
    pub fn apply_do_corner_view(&mut self, view: CornerView) -> bool {
        if self.carrying_box || self.carrying_dead_body {
            return false;
        }
        match view {
            CornerView::Corner0 => {
                self.next_anim_id = ANIM_ENM_ROH_STOJ_NAHL_L;
            }
            CornerView::Corner1 => {
                self.next_anim_id = ANIM_ENM_ROH_STOJ_NAHL_R;
            }
            CornerView::Corner2 => {
                if !self.crouching {
                    return false;
                }
                self.next_anim_id = ANIM_ENM_ROH_DREP_NAHL_T;
            }
        }
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        true
    }
    /// GAME 0x587cb0 (RVA 0x187cb0) C_human::Do_ShockUp (mafia-re 113b5b6
    /// game/actors/C_human.cpp:8143-8153): carrying a box or a dead body is a
    /// no-op; when the current animation is one of the panic stativs
    /// (ANIM_X_PANIKA_DREP_STATIV, ANIM_X_PANIKA_NA4_STATIV,
    /// ANIM_X_PANIKA_LEH_STATIV), the next animation advances by exactly one
    /// and PersonAnim(0.0, true, false) runs, recorded here as the handoff
    /// for the future animation owner. Any other anim_id leaves the state
    /// untouched. SCOPE: anim advance only. DEFERRED with explicit markers,
    /// no new owner here: the PersonAnim/Update/Movement writers, collision,
    /// tick.
    pub fn apply_do_shock_up(&mut self) {
        if self.carrying_box || self.carrying_dead_body {
            return;
        }
        if self.anim_id == ANIM_X_PANIKA_DREP_STATIV
            || self.anim_id == ANIM_X_PANIKA_NA4_STATIV
            || self.anim_id == ANIM_X_PANIKA_LEH_STATIV
        {
            self.next_anim_id = self.anim_id + 1;
            self.person_anim_request = Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            });
        }
    }
    /// GAME 0x587cf0 (RVA 0x187cf0) C_human::Do_ShockStumble (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:8156-8165): carrying a box or a dead
    /// body is a no-op; when the current animation is one of the panic runs
    /// (ANIM_PANIKA_RUN01A, ANIM_PANIKA_RUN03A, ANIM_PANIKA_RUN02A), the next
    /// animation advances by exactly one (verbatim `anim_id + 1`, gaps
    /// included) and PersonAnim(0.0, true, false) runs, recorded here as the
    /// handoff. Any other anim_id leaves the state untouched. SCOPE and
    /// DEFERRED markers as in `apply_do_shock_up`.
    pub fn apply_do_shock_stumble(&mut self) {
        if self.carrying_box || self.carrying_dead_body {
            return;
        }
        if self.anim_id == ANIM_PANIKA_RUN01A
            || self.anim_id == ANIM_PANIKA_RUN03A
            || self.anim_id == ANIM_PANIKA_RUN02A
        {
            self.next_anim_id = self.anim_id + 1;
            self.person_anim_request = Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            });
        }
    }
    /// GAME 0x58b7e0 (RVA 0x18b7e0) C_human::Do_DeadBodyDrop owned anim
    /// slice (mafia-re 113b5b6 game/actors/C_human.cpp:9271-9280): without a
    /// carried dead body this is a no-op; otherwise the body release runs
    /// (Do_DeadBody(NULL) with the body/cart owners, DEFERRED with an
    /// explicit marker and no latch write here: the drop body itself carries
    /// no latch store, matching the Intern_FromCar seat convention), then
    /// anim_id clears to 0, next_anim_id selects
    /// ANIM_NOSENI_MRTVOLY_TOM_SHOZENI and PersonAnim(0.0, true, false) runs,
    /// recorded here as the handoff. The trailing inline SetWaitAnim's
    /// PersonAnim part is that same handoff; its
    /// action_time/last_velocity/velocity_ramp writes plus the
    /// HUMAN_ACTION_WAIT select (C_human.h:1160-1172) stay DEFERRED with an
    /// explicit marker. SCOPE: anim words plus handoff only.
    pub fn apply_do_dead_body_drop(&mut self) {
        if !self.carrying_dead_body {
            return;
        }
        self.anim_id = 0;
        self.next_anim_id = ANIM_NOSENI_MRTVOLY_TOM_SHOZENI;
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
    }
    /// GAME 0x47b810 (RVA 0x7b810) C_human::IsReady (mafia-re 113b5b6
    /// game/actors/C_human.h:1145-1151): alive with the action STAND, WALK or
    /// CAR and a non-blocking animation machine. `anim_blocking` stages the
    /// `anim_machine.IsBlocking()` verdict, whose animation-machine owner
    /// stays DEFERRED; no invented blocking state.
    pub fn is_ready(&self, anim_blocking: bool) -> bool {
        self.alive
            && (self.action_state == HUMAN_ACTION_STAND
                || self.action_state == HUMAN_ACTION_WALK
                || self.action_state == HUMAN_ACTION_CAR)
            && !anim_blocking
    }
    /// GAME 0x57a630 (RVA 0x17a630) C_human::GetAnimIDForDeath pure select
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:4723-4748): the death
    /// animation id from the hit part, the crouch arm, the facing-angle
    /// verdict and the native random draws. `close_range` stages
    /// `v2.AngleTo(v1) < PI / 2` (vector owner DEFERRED); `draw2`/`draw3`
    /// stage the native `rnd(2)`/`rnd(3)` draws, so the caller supplies
    /// values already in 0..2/0..3. The random source and the animation
    /// owner stay DEFERRED.
    pub fn get_anim_id_for_death(
        hit_part: u32,
        crouched: bool,
        close_range: bool,
        draw2: u32,
        draw3: u32,
    ) -> i32 {
        if crouched {
            if hit_part == HIT_PART_HEAD {
                if close_range {
                    return (if draw2 != 0 { 2 } else { 0 }) + ANIM_SMRT_DREP_HLAVA_ZEZADU;
                }
                return (if draw2 != 0 { 1 } else { 0 }) + ANIM_SMRT_DREP_HLAVA_ZEPREDU;
            }
            if close_range {
                return draw2 as i32 + ANIM_SMRT_DREP_ZADA_A;
            }
            return draw2 as i32 + ANIM_SMRT_DREP_BRICHO_A;
        }
        if hit_part == HIT_PART_HEAD {
            if close_range {
                return (if draw2 != 0 { 2 } else { 0 }) + ANIM_SMRT_STOJ_HLAVA_ZEZADU;
            }
            return (if draw2 != 0 { 1 } else { 0 }) + ANIM_SMRT_STOJ_HLAVA_ZEPREDU;
        }
        if close_range {
            return draw3 as i32 + ANIM_SMRT_STOJ_ZADA_A;
        }
        draw3 as i32 + ANIM_SMRT_STOJ_BRICHO_A
    }
    /// GAME 0x58e270 (RVA 0x18e270) C_human::Can_Hit_FilterHumanType pure
    /// table leaf (mafia-re 113b5b6 game/actors/C_human.cpp:10083-10129):
    /// whether the shooter type may hit the target type, by the types' bit
    /// numbers in the static 11x11 table (GLOBAL GAME 0x636908, embedded here
    /// verbatim). A zero mask on either side admits; bit numbers above 10
    /// admit. On the (1, 0) cell the table's `true` is gated by the entity
    /// anger log (`ai_character_log`), whose owner stays DEFERRED, so the
    /// caller supplies `target_is_angry_focus` for that single cell.
    pub fn can_hit_filter_human_type(
        shooter_type: u32,
        target_type: u32,
        target_is_angry_focus: bool,
    ) -> bool {
        const CAN_HIT: [[bool; 11]; 11] = [
            [true, true, true, true, true, true, true, true, true, true, true],
            [true, false, true, true, true, true, true, true, true, true, true],
            [true, true, false, true, true, true, true, true, true, true, true],
            [true, true, true, false, false, true, true, true, true, true, true],
            [true, true, true, false, false, true, true, true, true, true, true],
            [true, true, true, true, true, false, true, true, true, true, true],
            [true, true, true, true, true, true, false, true, true, true, true],
            [true, true, true, true, true, true, true, true, true, true, true],
            [true, true, true, true, true, true, true, true, true, true, true],
            [true, true, true, true, true, true, true, true, true, true, true],
            [true, true, true, true, true, true, true, true, true, true, true],
        ];
        let mut mask = shooter_type;
        if mask == 0 {
            return true;
        }
        let mut shooter_bit = 0;
        while mask & 1 == 0 {
            mask >>= 1;
            shooter_bit += 1;
        }
        mask = target_type;
        if mask == 0 {
            return true;
        }
        let mut target_bit = 0;
        while mask & 1 == 0 {
            mask >>= 1;
            target_bit += 1;
        }
        if shooter_bit > 10 || target_bit > 10 {
            return true;
        }
        if shooter_bit == 1 && target_bit == 0 {
            return target_is_angry_focus;
        }
        CAN_HIT[shooter_bit as usize][target_bit as usize]
    }
    /// GAME 0x58b870 (RVA 0x18b870) C_human::ResetActions owned latch slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:9282-9300): stand the human
    /// down through Do_Crouched(false) (the headroom probe verdict arrives as
    /// `headroom_clear`, its frame/collision owners stay DEFERRED), clear the
    /// aimed and held-shot latches, and on the alive path clear anim_id,
    /// select ANIM_BREATH01A and invoke PersonAnim(0.0, true, false),
    /// recorded here as the handoff for the future animation owner. SCOPE:
    /// latch and anim-select words only. DEFERRED with explicit markers, no
    /// new owner here: PoseNullRotate (no pose owner, ledger-claimed at
    /// 0x17a560 outside this module), the player fade_alpha/SetAlpha branch
    /// (no actor-type/player owner), the INDF_THIEF indicator effect, and
    /// every Movement/Update writer of these words.
    pub fn apply_reset_actions(&mut self, headroom_clear: bool) {
        self.apply_do_crouched(false, headroom_clear);
        self.aimed = false;
        self.fire_held = false;
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
    /// GAME 0x58a790 (RVA 0x18a790) C_human::Can_Aimed probe verdict leaf
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:8911-8918): true when the
    /// head-height forward TestLineH probe finds no wall. The frame read
    /// (1.5 m lift, world direction) and the collision probe stay DEFERRED
    /// with their owners; the caller supplies `wall_ahead` (the probe hit
    /// verdict) and this leaf owns only the inversion.
    pub fn can_aimed(wall_ahead: bool) -> bool {
        !wall_ahead
    }
    /// GAME 0x58cfb0 (RVA 0x18cfb0) C_human::IsThisCarStealing claimant scan
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:9592-9610): whether any
    /// PLAYER/ENTITY claimant holds the car as stealing_car, car or
    /// using_car. The active-actor scan stays with the mission owner
    /// (DEFERRED); the caller supplies the claimant projection and this leaf
    /// owns only the type gate and the identity compare. None (NULL) admits
    /// no claimant.
    pub fn car_has_human_claimant(
        car: Option<ActorReferenceId>,
        claimants: &[CarTheftClaimant],
    ) -> bool {
        let Some(car) = car else {
            return false;
        };
        claimants.iter().any(|claimant| {
            (claimant.actor_type == ACTOR_PLAYER || claimant.actor_type == ACTOR_ENTITY)
                && (claimant.stealing_car == Some(car)
                    || claimant.car == Some(car)
                    || claimant.using_car == Some(car))
        })
    }
    /// GAME 0x58aa60 (RVA 0x18aa60) C_human::Do_CornerFire owned select slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:8963-8993): carrying a box
    /// or a dead body refuses; rifles and fists refuse. Otherwise CORNER_0
    /// selects FIRE01_L (SAP_STOJ_FIRE01_L for the Thompson while standing),
    /// CORNER_1 the right-hand pair, and CORNER_2 refuses unless crouching
    /// and then selects DREP_FIRE01_T. On admission the inline SetWaitAnim
    /// runs: its PersonAnim(0.0, true, false) invocation is recorded here as
    /// the handoff for the future animation owner, while its
    /// action_time/last_velocity/velocity_ramp writes plus the
    /// HUMAN_ACTION_WAIT select stay DEFERRED with an explicit marker; then
    /// aimed latches. The weapon-kind and right-hand fire-animation reads
    /// stage the inventory/weapon owners (DEFERRED). Returns true on
    /// admission.
    pub fn apply_do_corner_fire(&mut self, view: CornerView) -> bool {
        if self.carrying_box || self.carrying_dead_body {
            return false;
        }
        if self.weapon_kind == WEAPON_KIND_RIFLE || self.weapon_kind == WEAPON_KIND_FISTS {
            return false;
        }
        let thompson_standing =
            !self.crouching && self.right_hand_fire_anim == GUN_ANIM_THOMPSON;
        match view {
            CornerView::Corner0 => {
                self.next_anim_id = if thompson_standing {
                    ANIM_ENM_ROH_SAP_STOJ_FIRE01_L
                } else {
                    ANIM_ENM_ROH_FIRE01_L
                };
            }
            CornerView::Corner1 => {
                self.next_anim_id = if thompson_standing {
                    ANIM_ENM_ROH_SAP_STOJ_FIRE01_R
                } else {
                    ANIM_ENM_ROH_FIRE01_R
                };
            }
            CornerView::Corner2 => {
                if !self.crouching {
                    return false;
                }
                self.next_anim_id = ANIM_ENM_ROH_DREP_FIRE01_T;
            }
        }
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        self.aimed = true;
        true
    }
    /// GAME 0x58ab80 (RVA 0x18ab80) C_human::Do_CornerFireAimed owned select
    /// slice (mafia-re 113b5b6 game/actors/C_human.cpp:8995-9025): the same
    /// guards and FIRE02/SAP_STOJ_FIRE02/DREP_FIRE02_T selects as
    /// Do_CornerFire, but PersonAnim(0.0, true, false) runs directly (no
    /// SetWaitAnim wait words) before the aimed latch. SCOPE and DEFERRED
    /// markers as in `apply_do_corner_fire`. Returns true on admission.
    pub fn apply_do_corner_fire_aimed(&mut self, view: CornerView) -> bool {
        if self.carrying_box || self.carrying_dead_body {
            return false;
        }
        if self.weapon_kind == WEAPON_KIND_RIFLE || self.weapon_kind == WEAPON_KIND_FISTS {
            return false;
        }
        let thompson_standing =
            !self.crouching && self.right_hand_fire_anim == GUN_ANIM_THOMPSON;
        match view {
            CornerView::Corner0 => {
                self.next_anim_id = if thompson_standing {
                    ANIM_ENM_ROH_SAP_STOJ_FIRE02_L
                } else {
                    ANIM_ENM_ROH_FIRE02_L
                };
            }
            CornerView::Corner1 => {
                self.next_anim_id = if thompson_standing {
                    ANIM_ENM_ROH_SAP_STOJ_FIRE02_R
                } else {
                    ANIM_ENM_ROH_FIRE02_R
                };
            }
            CornerView::Corner2 => {
                if !self.crouching {
                    return false;
                }
                self.next_anim_id = ANIM_ENM_ROH_DREP_FIRE02_T;
            }
        }
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        self.aimed = true;
        true
    }
    /// GAME 0x585f40 (RVA 0x185f40) C_human::GetBoundPoints2D pure geometry
    /// leaf (mafia-re 113b5b6 game/actors/C_human.cpp:7630-7638): the four
    /// 2D bound points around the world position, 0.7 m out on each axis.
    /// The frame read stays with the frame owner (DEFERRED); the caller
    /// stages the position as `center_x`/`center_z`. The original returns
    /// true unconditionally, which carries no information and is not
    /// replicated.
    pub fn bound_points_2d(center_x: f32, center_z: f32) -> [[f32; 2]; 4] {
        [
            [center_x + 0.7, center_z],
            [center_x - 0.7, center_z],
            [center_x, center_z + 0.7],
            [center_x, center_z - 0.7],
        ]
    }
    /// GAME 0x584480 (RVA 0x184480) C_human::Do_Castrate owned strike slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:7213-7234): a missing target
    /// or an unready target refuses; the invulnerability cheat gate (player
    /// or friend target spared) arrives as the caller verdict
    /// `cheat_refusal`, its game/player/type owners stay DEFERRED. On
    /// admission the striker selects ANIM_BOJ_BOX_ROZKROK and the target
    /// ANIM_BOJ_ZASAH_ROZKROK_SM, both with a PersonAnim(0.0, true, false)
    /// handoff for the future animation owner, and the target loses
    /// `strength * 10` energy on the live property bank. SCOPE: anim selects,
    /// handoffs and the energy write only. DEFERRED with explicit markers:
    /// the target IsReady probe behind `target_ready`, collision, tick, and
    /// every frame/weapon effect of the strike.
    pub fn apply_do_castrate(
        &mut self,
        target: Option<&mut HumanState>,
        target_ready: bool,
        cheat_refusal: bool,
    ) -> bool {
        let Some(target) = target else {
            return false;
        };
        if !target_ready {
            return false;
        }
        if cheat_refusal {
            return false;
        }
        self.next_anim_id = ANIM_BOJ_BOX_ROZKROK;
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        target.next_anim_id = ANIM_BOJ_ZASAH_ROZKROK_SM;
        target.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        let strength = self
            .current_properties
            .float_component(HUMAN_PROPERTY_STRENGTH);
        let energy = target
            .current_properties
            .float_component(HUMAN_PROPERTY_ENERGY);
        target
            .current_properties
            .set_component_bits(HUMAN_PROPERTY_ENERGY, (energy - strength * 10.0).to_bits());
        true
    }
    /// GAME 0x584550 (RVA 0x184550) C_human::Do_HardHead owned strike slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:7236-7257): the same guards
    /// as Do_Castrate, but the striker selects ANIM_BOJ_BOX_HLAVICKA and the
    /// target ANIM_BOJ_ZASAH_HLAVICKA, with the same handoffs and the same
    /// `strength * 10` live-energy write. SCOPE and DEFERRED markers as in
    /// `apply_do_castrate`.
    pub fn apply_do_hard_head(
        &mut self,
        target: Option<&mut HumanState>,
        target_ready: bool,
        cheat_refusal: bool,
    ) -> bool {
        let Some(target) = target else {
            return false;
        };
        if !target_ready {
            return false;
        }
        if cheat_refusal {
            return false;
        }
        self.next_anim_id = ANIM_BOJ_BOX_HLAVICKA;
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        target.next_anim_id = ANIM_BOJ_ZASAH_HLAVICKA;
        target.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
        let strength = self
            .current_properties
            .float_component(HUMAN_PROPERTY_STRENGTH);
        let energy = target
            .current_properties
            .float_component(HUMAN_PROPERTY_ENERGY);
        target
            .current_properties
            .set_component_bits(HUMAN_PROPERTY_ENERGY, (energy - strength * 10.0).to_bits());
        true
    }
    /// GAME 0x587bc0 (RVA 0x187bc0) C_human::Do_ShockStart owned select slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:8107-8118): carrying a box
    /// or a dead body is a no-op; otherwise the variant clamps to 0..3 and
    /// the next animation selects LEKNUTI01L plus the variant on a nonzero
    /// native `rnd(2)` draw, LEKNUTI02L plus the variant on zero, staged here
    /// as `draw`. PersonAnim(0.0, true, false) runs, recorded here as the
    /// handoff for the future animation owner. The random source stays
    /// DEFERRED. SCOPE: anim select plus handoff only. DEFERRED with explicit
    /// markers, no new owner here: the PersonAnim/Update/Movement writers of
    /// the word, collision, tick.
    pub fn apply_do_shock_start(&mut self, variant: i32, draw: u32) {
        if self.carrying_box || self.carrying_dead_body {
            return;
        }
        let variant = variant.clamp(0, 3);
        self.next_anim_id = if draw != 0 {
            ANIM_PANIKA_LEKNUTI01L + variant
        } else {
            ANIM_PANIKA_LEKNUTI02L + variant
        };
        self.person_anim_request = Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        });
    }
    /// GAME 0x57f9a0 (RVA 0x17f9a0) C_human::InventoryTruncate (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6133-6140): clamp right_hand,
    /// big_weapon and every pocket through the TruncateAmmo leaf. Delegates
    /// to the inventory owner; the gameItemTable owner stays DEFERRED (the
    /// caller supplies `num_game_items`/`ammo_limits`), as do collision and
    /// tick.
    pub fn apply_inventory_truncate(
        &mut self,
        num_game_items: u32,
        ammo_limits: &dyn Fn(u16) -> Option<(u32, u32)>,
    ) {
        self.inventory
            .apply_inventory_truncate(num_game_items, ammo_limits);
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
    /// Active-collision latch staged by the ActivateColls/DeactivateColls
    /// select slices.
    pub fn colls_active(&self) -> bool {
        self.colls_active
    }
    /// Staged animation steps (Do_ShockDown fills the first two and ends
    /// the third; the Movement player stays DEFERRED).
    pub fn anim_steps(&self) -> &[HumanAnimStep; 5] {
        &self.anim_steps
    }
    /// Current staged-step index written by the Do_ShockDown core.
    pub fn anim_step(&self) -> i32 {
        self.anim_step
    }
    /// Staged fire-action table identity (weapon owner DEFERRED).
    pub fn fire_action(&self) -> FireActionTable {
        self.fire_action
    }
    /// Current fire-action step cleared by ResetWeaponEffects.
    pub fn fire_step(&self) -> i32 {
        self.fire_step
    }
    /// Fire-action step time cleared by ResetWeaponEffects.
    pub fn fire_time(&self) -> i32 {
        self.fire_time
    }
    /// Voice priority word written by the NewScream gate.
    pub fn voice_priority(&self) -> i32 {
        self.voice_priority
    }
    /// Cutscene-recorded staging gating the SetCutsceneLink core.
    pub fn cutscene_recorded(&self) -> bool {
        self.cutscene_recorded
    }
    /// Test/host setter for the cutscene-recorded staging (act_flags owner
    /// DEFERRED).
    pub fn set_cutscene_recorded(&mut self, recorded: bool) {
        self.cutscene_recorded = recorded;
    }
    /// Staged cutscene link written by the SetCutsceneLink core, if any.
    pub fn cutscene_link(&self) -> Option<&HumanCutsceneLink> {
        self.cutscene_link.as_ref()
    }
    /// GAME 0x575fc0 (RVA 0x175fc0) C_human::ActivateColls select slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:3635): seated in a car the
    /// body returns at once with no latch write (None). Otherwise the
    /// active-collision latch sets and each live collision selects
    /// insertion by mode: ALL inserts an upper-flagged coll or any coll
    /// while alive; UPPER inserts an upper-flagged coll while alive; LOWER
    /// inserts a lower-flagged coll while alive. `coll_flags` stages the
    /// live `colls[i]->flags` words in order (NULL entries are the caller's
    /// to drop; the colls array owner stays DEFERRED) and the returned mask
    /// runs in the same order for the collision manager. `mode` stages
    /// HUMAN_COLLS_* verbatim, so an unknown mode inserts nothing, exactly
    /// like the original's switch. SCOPE: latch plus select only. DEFERRED
    /// with explicit markers, no new owner here: every
    /// `gifCollision.DynInsert` call and the colls array owner.
    pub fn apply_activate_colls(
        &mut self,
        in_car: bool,
        mode: i32,
        coll_flags: &[u32],
    ) -> Option<Vec<bool>> {
        if in_car {
            return None;
        }
        self.colls_active = true;
        Some(
            coll_flags
                .iter()
                .map(|flags| match mode {
                    HUMAN_COLLS_ALL => (flags & DYNCOLLF_UPPER) != 0 || self.alive,
                    HUMAN_COLLS_UPPER => (flags & DYNCOLLF_UPPER) != 0 && self.alive,
                    HUMAN_COLLS_LOWER => (flags & DYNCOLLF_LOWER) != 0 && self.alive,
                    _ => false,
                })
                .collect(),
        )
    }
    /// GAME 0x575f60 (RVA 0x175f60) C_human::DeactivateColls select slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:3619): clear the
    /// active-collision latch, then erase every live collision except an
    /// upper-flagged one while `keep_upper` holds. `coll_flags` stages the
    /// live `colls[i]->flags` words in order (NULL entries are the caller's
    /// to drop; the colls array owner stays DEFERRED) and the returned mask
    /// runs in the same order for the collision manager. SCOPE: latch plus
    /// select only. DEFERRED with explicit markers, no new owner here: every
    /// `gifCollision.DynErase`/`InvalidateColl` call and the colls array
    /// owner.
    pub fn apply_deactivate_colls(
        &mut self,
        keep_upper: bool,
        coll_flags: &[u32],
    ) -> Vec<bool> {
        self.colls_active = false;
        coll_flags
            .iter()
            .map(|flags| !(keep_upper && (flags & DYNCOLLF_UPPER) != 0))
            .collect()
    }
    /// GAME 0x587c20 (RVA 0x187c20) C_human::Do_ShockDown owned step slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:8121): carrying a box or a
    /// dead body refuses. Otherwise the variant clamps to 0..2 and fills
    /// the staged steps: step 0 plays `variant * 3 + ANIM_X_PANIKA_DREP_DOLU`
    /// once (unstanced, no flags), step 1 plays `variant * 3 +`
    /// ANIM_X_PANIKA_DREP_STATIV the same way, step 2 ends the steps; then
    /// HUMAN_ACTION_STEPS selects and the step index zeroes. Only the words
    /// the original writes are written (the step waits keep their staged
    /// values). SCOPE: step and action words only. DEFERRED with explicit
    /// markers, no new owner here: the Movement HUMAN_ACTION_STEPS player
    /// and every PersonAnim effect of the steps.
    pub fn apply_do_shock_down(&mut self, variant: i32) {
        if self.carrying_box || self.carrying_dead_body {
            return;
        }
        let clamped = variant.clamp(0, 2);
        self.anim_steps[0].anim_id = clamped * 3 + ANIM_X_PANIKA_DREP_DOLU;
        self.anim_steps[0].step_type = HUMAN_STEP_ANIM;
        self.anim_steps[0].stance = false;
        self.anim_steps[0].flags = 0;
        self.anim_steps[1].anim_id = clamped * 3 + ANIM_X_PANIKA_DREP_STATIV;
        self.anim_steps[1].step_type = HUMAN_STEP_ANIM;
        self.anim_steps[1].stance = false;
        self.anim_steps[1].flags = 0;
        self.anim_steps[2].step_type = HUMAN_STEP_END;
        self.action_state = HUMAN_ACTION_STEPS;
        self.anim_step = 0;
    }
    /// GAME 0x573ce0 (RVA 0x173ce0) C_human::ChangeDeadAnim pure select
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:2688): the carried dead's
    /// animation from the carrier's next id masked with ANIM_BASE: walks
    /// map to MrtWalk, backs to MrtBack, sidesteps to MrtLeft/MrtRight,
    /// strafes to MrtStrafL/MrtStrafR, the pickup to MrtNahozeni and the
    /// drop to MrtShozeni, anything else to MrtStat. Returns the animation
    /// file plus the Do_PlayAnim simple/unknown pair (`!loop`, `unknown`):
    /// only the pickup and the drop play once, only the pickup sets the
    /// extra flag. The dead_body/dead_body_human presence gate is the
    /// caller's (carrying_dead_body latch); the dead_body_human Do_PlayAnim
    /// call and every using-object/scene write stay DEFERRED with their
    /// owners.
    pub fn dead_body_anim_for_carrier(
        carrier_next_anim_id: i32,
    ) -> (&'static str, bool, bool) {
        match carrier_next_anim_id & ANIM_BASE {
            ANIM_WALK1 | ANIM_WALKL1 | ANIM_WALKR1 => {
                ("NoseniMrtvolyMrtWalk.I3D", false, false)
            }
            ANIM_BACK1 | ANIM_BACKL1 | ANIM_BACKR1 => {
                ("NoseniMrtvolyMrtBack.I3D", false, false)
            }
            ANIM_LEFT1 => ("NoseniMrtvolyMrtLeft.I3D", false, false),
            ANIM_RIGHT1 => ("NoseniMrtvolyMrtRight.I3D", false, false),
            ANIM_STRAFL1 => ("NoseniMrtvolyMrtStrafL.I3D", false, false),
            ANIM_STRAFR1 => ("NoseniMrtvolyMrtStrafR.I3D", false, false),
            ANIM_NOSENI_MRTVOLY_TOM_NAHOZENI => {
                ("NoseniMrtvolyMrtNahozeni.I3D", true, true)
            }
            ANIM_NOSENI_MRTVOLY_TOM_SHOZENI => {
                ("NoseniMrtvolyMrtShozeni.I3D", true, false)
            }
            _ => ("NoseniMrtvolyMrtStat.I3D", false, false),
        }
    }
    /// GAME 0x58df60 (RVA 0x18df60) C_human::ResetWeaponEffects owned latch
    /// slice (mafia-re 113b5b6 game/actors/C_human.cpp:9998): select the
    /// ready fire-action table and clear the step and its time. SCOPE: the
    /// three fire words only. DEFERRED with explicit markers, no new owner
    /// here: the effect_frame switch over the right-hand item
    /// (GAMEITEM_*, weapon-model owner) and every Shooting/Do_Reload writer
    /// of these words.
    pub fn apply_reset_weapon_effects(&mut self) {
        self.fire_action = FireActionTable::Ready;
        self.fire_step = 0;
        self.fire_time = 0;
    }
    /// GAME 0x584010 (RVA 0x184010) C_human::Can_SpecialStroke claimant
    /// scan (mafia-re 113b5b6 game/actors/C_human.cpp:7190): the tail-first
    /// scan admits the last matching stranger within a metre ahead, turned
    /// away: an
    /// ACTOR_PLAYER/ACTOR_ENTITY claimant in the active state, uncrouched,
    /// ready (alive with a STAND/WALK/CAR action and a non-blocking
    /// animation, the IsReady gate with the candidate's words), whose
    /// energy tops ten times this human's `properties` strength, closer
    /// than 1.0 in squared 3D distance, within 0.3 rad of this human's
    /// facing and over 2.85 rad away from its own facing. The tail-first
    /// order matches the original's reverse global-actor walk, so the LAST
    /// matching slice entry wins; the caller drops this human itself (the
    /// `human != this`
    /// identity). Angles stage S_vector::AngleTo through the local helper
    /// (LS3DF native; the vector owner stays DEFERRED): a zero-length side
    /// yields NaN, which fails both gates exactly like a missed probe. The
    /// active-actor list, the frames and the property banks behind the
    /// staging stay DEFERRED with their owners.
    pub fn find_special_stroke_target(
        &self,
        self_position: [f32; 3],
        candidates: &[SpecialStrokeCandidate],
    ) -> Option<usize> {
        let strength = self
            .properties
            .float_component(HUMAN_PROPERTY_STRENGTH);
        candidates.iter().rposition(|candidate| {
            if candidate.actor_type != ACTOR_PLAYER
                && candidate.actor_type != ACTOR_ENTITY
            {
                return false;
            }
            if !candidate.active_state || candidate.crouching {
                return false;
            }
            let ready = candidate.alive
                && (candidate.action_state == HUMAN_ACTION_STAND
                    || candidate.action_state == HUMAN_ACTION_WALK
                    || candidate.action_state == HUMAN_ACTION_CAR)
                && !candidate.anim_blocking;
            if !ready {
                return false;
            }
            if strength * 10.0 >= candidate.energy {
                return false;
            }
            let delta = [
                candidate.position[0] - self_position[0],
                candidate.position[1] - self_position[1],
                candidate.position[2] - self_position[2],
            ];
            let dist_sq = delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2];
            if dist_sq >= 1.0 {
                return false;
            }
            vector_angle(delta, self.dir) < 0.3
                && vector_angle(delta, candidate.facing) > 2.85
        })
    }
    /// GAME 0x58b470 (RVA 0x18b470) C_human::NewScream owned gate slice
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:9208): the voice says
    /// voices.def's entry `voice_id - 2` in its voice type unless shut up,
    /// voiceless, out of the camera's scream areas, past the voice table,
    /// talked over (a playing voice of at least the stored priority), over
    /// priority 500, or soundless. `scream_areas`/`camera_scream_areas`
    /// stage the ushort words widened (the game owner stays DEFERRED);
    /// `voice_playing` stages `voice->IsPlaying(false)`,
    /// `candidate_priority`/`candidate_sound_present` stage
    /// `gameVoices[index].priority` and its `sounds[voice_type]` word (the
    /// voice-table owner stays DEFERRED). The `voice_id - 2` wraps like the
    /// native uint, so ids below 2 miss the table exactly like the
    /// original. On admission the stored priority takes the candidate
    /// (native uint-to-int wrap preserved) and true returns. SCOPE: gate
    /// plus the priority word only. DEFERRED with explicit markers, no new
    /// owner here: the voice/sound-cache pipeline (SetOn/Open/Loop/LinkTo/
    /// SetSoundType/SetRange/SetVolume and the NOTE queue).
    pub fn apply_new_scream_gate(
        &mut self,
        voice_present: bool,
        scream_areas: u32,
        camera_scream_areas: u32,
        voice_id: u32,
        num_game_voices: u32,
        voice_playing: bool,
        candidate_priority: u32,
        candidate_sound_present: bool,
    ) -> bool {
        if self.shut_up || !voice_present {
            return false;
        }
        if scream_areas & camera_scream_areas == 0 {
            return false;
        }
        let index = voice_id.wrapping_sub(2);
        if index >= num_game_voices {
            return false;
        }
        if voice_playing && candidate_priority <= self.voice_priority as u32 {
            return false;
        }
        if self.voice_priority > 500 {
            return false;
        }
        if !candidate_sound_present {
            return false;
        }
        self.voice_priority = candidate_priority as i32;
        true
    }
    /// GAME 0x578e20 (RVA 0x178e20) C_human::SetCutsceneLink owned staging
    /// slice (mafia-re 113b5b6 game/actors/C_human.cpp:4539): while the
    /// human is not recorded nothing happens and no link is allocated.
    /// While recorded the link is allocated on first use, then marked new
    /// with the kind, the frame placement and the linked actor's frame name
    /// ("" for none). `frame_pos`/`frame_rot` stage `frame->GetPos/GetRot`
    /// and `actor_name` stages the linked `act->frame->GetName()` (both
    /// frame owners stay DEFERRED); the name copies at most
    /// CUTSCENE_LINK_NAME_LEN - 1 bytes plus the terminator, a host bound
    /// for a word the original `strcpy`s without one. The RecordKey
    /// consumer of `is_new` stays DEFERRED. SCOPE: link staging only.
    pub fn apply_set_cutscene_link(
        &mut self,
        kind: i32,
        actor_name: Option<&str>,
        frame_pos: [f32; 3],
        frame_rot: [f32; 4],
    ) {
        if !self.cutscene_recorded {
            return;
        }
        let mut name = [0u8; CUTSCENE_LINK_NAME_LEN];
        if let Some(actor_name) = actor_name {
            let bytes = actor_name.as_bytes();
            let len = bytes.len().min(CUTSCENE_LINK_NAME_LEN - 1);
            name[..len].copy_from_slice(&bytes[..len]);
        }
        match self.cutscene_link.as_mut() {
            Some(link) => {
                link.is_new = true;
                link.kind = kind;
                link.name = name;
                link.pos = frame_pos;
                link.rot = frame_rot;
            }
            None => {
                self.cutscene_link = Some(HumanCutsceneLink {
                    is_new: true,
                    kind,
                    name,
                    pos: frame_pos,
                    rot: frame_rot,
                });
            }
        }
    }
    /// GAME 0x51af20 (RVA 0x11af20) C_human::GetActorSpeed (mafia-re 113b5b6
    /// game/actors/C_human.h:678, inline: "the human's speed, the walk's"):
    /// copy the walk velocity into the caller's speed word. The velocity
    /// writers (Go_*/Movement) stay DEFERRED; this leaf only publishes the
    /// staged word.
    pub fn actor_speed_into(&self, speed: &mut [f32; 3]) {
        *speed = self.velocity;
    }
    /// GAME 0x57a5f0 (RVA 0x17a5f0) C_human::PoseValidate EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:4714):
    /// the body is frame-only (`frame->SetPoseBlend(valid ? 1.0 : 0.0)` plus
    /// `frame->Tick(1)`), reads and writes no human state, and both calls
    /// belong to the frame/model owner, which does not exist here. No
    /// verdict is produced and no owner is filed; the stub exists only to
    /// pin the address while the frame owner stays DEFERRED.
    pub fn pose_validate_unsupported(_valid: bool) {}
    /// GAME 0x5802f0 (RVA 0x1802f0) C_human::Can_DropOutFromCar_Free
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6284): the verdict is a collision-probe
    /// program (seat-side offset through the car matrix, ground TestLineV,
    /// two room TestSphereB sweeps around DynSetNextCollOwnerMult
    /// plumbing). The seat-property, matrix, ground and room probes all
    /// belong to the car/collision owners, which do not exist here, so no
    /// verdict is produced and no owner is filed; the stub exists only to
    /// pin the address while those owners stay DEFERRED.
    pub fn can_drop_out_from_car_free_unsupported() {}
    /// GAME 0x5805c0 (RVA 0x1805c0) C_human::Can_DropOutFromCar EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:6326):
    /// same boundary as Can_DropOutFromCar_Free (seat defaulting plus the
    /// frame-relative side offset and the colls/ghost probe program). The
    /// seat, frame and collision owners do not exist here, so no verdict
    /// is produced and no owner is filed; the stub exists only to pin the
    /// address while those owners stay DEFERRED.
    pub fn can_drop_out_from_car_unsupported() {}
    /// GAME 0x5706a0 (RVA 0x1706a0) C_human::SoundsChange EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1710):
    /// the body switches six sounds off (step, weapon, reload, pump and
    /// voice, plus the player's). Every handle belongs to the
    /// sound/voice/player owners, which do not exist here, so no state is
    /// touched and no owner is filed; the stub exists only to pin the
    /// address while those owners stay DEFERRED.
    pub fn apply_sounds_change_unsupported() {}
    /// GAME 0x5877a0 (RVA 0x1877a0) C_human::InitFFEffects EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:8037):
    /// the body creates four joystick force-feedback effects (shot, jump,
    /// hit, fall) through the global FF manager. The joystick device owner
    /// does not exist here, so no state is touched and no owner is filed;
    /// the stub exists only to pin the address while that owner stays
    /// DEFERRED.
    pub fn init_ff_effects_unsupported() {}
    /// GAME 0x587880 (RVA 0x187880) C_human::DoneFFEffects EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:8050):
    /// the body deletes the four joystick force-feedback effects through
    /// the global FF manager. Same device boundary as InitFFEffects: no
    /// state is touched and no owner is filed; the stub exists only to pin
    /// the address while that owner stays DEFERRED.
    pub fn done_ff_effects_unsupported() {}
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
    // GAME 0x47b7a0/0x47b7c0/0x47b7e0/0x47b7f0/0x47b800/0x47b850 (RVA
    // 0x7b7a0/0x7b7c0/0x7b7e0/0x7b7f0/0x7b800/0x7b850): the small
    // C_human setters store their words and start at the cleared/
    // none defaults.
    fn small_human_setters_round_trip_and_start_cleared() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.look_frame(), None);
        assert_eq!((human.wingman(), human.wingman_name()), (-1, 0));
        assert_eq!(human.fire_target(), None);
        assert!(!human.no_anim_hit());
        assert!(!human.idle_anims());
        assert!(!human.shut_up());
        human.set_look_frame(Some(FrameReferenceId::from_existing(7)));
        assert_eq!(human.look_frame(), Some(FrameReferenceId::from_existing(7)));
        human.set_look_frame(None);
        assert_eq!(human.look_frame(), None);
        human.set_wingman(2, 5);
        assert_eq!((human.wingman(), human.wingman_name()), (2, 5));
        human.set_fire_target(Some(FrameReferenceId::from_existing(9)));
        assert_eq!(
            human.fire_target(),
            Some(FrameReferenceId::from_existing(9))
        );
        human.set_fire_target(None);
        assert_eq!(human.fire_target(), None);
        human.set_no_anim_hit(true);
        assert!(human.no_anim_hit());
        human.set_idle_anims(true);
        assert!(human.idle_anims());
        human.set_shut_up(true);
        assert!(human.shut_up());
        human.set_no_anim_hit(false);
        human.set_idle_anims(false);
        human.set_shut_up(false);
        assert!(!human.no_anim_hit());
        assert!(!human.idle_anims());
        assert!(!human.shut_up());
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
    fn speed_unit_seed() -> HumanSeed {
        // Hand-derived helper: a 1.0 Speed multiplier (mafia-re 113b5b6
        // game/actors/C_human.h:92 HUMAN_PROPERTY_SPEED 7) so the Go_*
        // expectations read as plain base speeds.
        let mut seed_record = [0u8; 64];
        seed_record[7 * 4..7 * 4 + 4].copy_from_slice(&1.0f32.to_le_bytes());
        HumanSeed {
            allocator_tag: 0xd7,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xa5a5_a5a5,
            },
            previous_animation_code: 0,
            requested_animation_code: 0,
            human_type_mask: 0,
            current_properties: HumanProperties::from_record(seed_record),
            target_e: None,
            target_n: None,
            base_frame: None,
            auxiliary: std::array::from_fn(|_| HumanAuxiliarySeed {
                untouched_word: 0,
                padding: [0; 3],
                untouched_tail: [0; 16],
            }),
        }
    }
    #[test]
    // GAME 0x57d670 (RVA 0x17d670) C_human::Go_FrontLeft (mafia-re 113b5b6
    // game/actors/C_human.cpp:5442): the seat guard refuses, crouching forces
    // the walk blend, and the run blend normalizes (side + dir) before the
    // 5.0 scaling.
    fn go_front_left_rva_17d670_walk_run_and_seat_guard() {
        use crate::script_scheduler::ActorReferenceId;
        let mut human = HumanState::construct(speed_unit_seed());
        human.set_dir([0.0, 0.0, 1.0]);
        human.set_car(Some(ActorReferenceId::from_existing(3)));
        human.seat = 0;
        assert!(!human.apply_go_front_left(true, true));
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        human.set_car(None);
        // dir (0,0,1): side (-1,0,0), blend (-1,0,1), |blend| = sqrt(2).
        assert!(human.apply_go_front_left(true, false));
        assert_eq!(human.action_state(), HUMAN_ACTION_WALK);
        assert_eq!(human.turn_speed(), 0.0);
        assert_eq!(human.next_anim_id(), ANIM_RUNL1);
        let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        for (got, want) in human
            .velocity()
            .iter()
            .zip([-5.0 * inv_sqrt2, 0.0, 5.0 * inv_sqrt2])
        {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
        // Crouching forces the 1.7 walk blend and the WALKL1 select.
        human.set_crouching(true);
        assert!(human.apply_go_front_left(true, false));
        assert_eq!(human.next_anim_id(), ANIM_WALKL1);
        for (got, want) in human
            .velocity()
            .iter()
            .zip([-1.7 * inv_sqrt2, 0.0, 1.7 * inv_sqrt2])
        {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
    }
    #[test]
    // GAME 0x57d860 (RVA 0x17d860) C_human::Go_FrontRight (mafia-re 113b5b6
    // game/actors/C_human.cpp:5458): the mirror blend (side + dir) with
    // side = (dir.z, dir.y, -dir.x) and the RUNR1/WALKR1 select.
    fn go_front_right_rva_17d860_mirrors_left_blend() {
        let mut human = HumanState::construct(speed_unit_seed());
        human.set_dir([0.0, 0.0, 1.0]);
        // dir (0,0,1): side (1,0,0), blend (1,0,1), |blend| = sqrt(2).
        assert!(human.apply_go_front_right(false, false));
        assert_eq!(human.action_state(), HUMAN_ACTION_WALK);
        assert_eq!(human.turn_speed(), 0.0);
        assert_eq!(human.next_anim_id(), ANIM_WALKR1);
        let inv_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        for (got, want) in human
            .velocity()
            .iter()
            .zip([1.7 * inv_sqrt2, 0.0, 1.7 * inv_sqrt2])
        {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
        assert!(human.apply_go_front_right(true, false));
        assert_eq!(human.next_anim_id(), ANIM_RUNR1);
    }
    #[test]
    // GAME 0x57da50 (RVA 0x17da50) C_human::Go_BackLeft (mafia-re 113b5b6
    // game/actors/C_human.cpp:5474): no crouch arm, the normalized
    // (side + -dir) blend scaled by SPEED * 1.5 and the BACKL1 select; the
    // zero-dir degenerate arm of Normalize yields (0, 1, 0).
    fn go_back_left_rva_17da50_normalizes_reverse_blend() {
        let mut human = HumanState::construct(speed_unit_seed());
        human.set_dir([1.0, 0.0, 0.0]);
        // dir (1,0,0): side (0,0,1), blend (-1,0,1), |blend| = sqrt(2).
        assert!(human.apply_go_back_left(false));
        assert_eq!(human.action_state(), HUMAN_ACTION_WALK);
        assert_eq!(human.turn_speed(), 0.0);
        assert_eq!(human.next_anim_id(), ANIM_BACKL1);
        let unit = 1.5 * std::f32::consts::FRAC_1_SQRT_2;
        for (got, want) in human.velocity().iter().zip([-unit, 0.0, unit]) {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
        // Crouching does not force anything here: the original has no such
        // arm on this path, so the blend is unchanged.
        human.set_crouching(true);
        assert!(human.apply_go_back_left(false));
        assert_eq!(human.next_anim_id(), ANIM_BACKL1);
        // Degenerate zero direction: side and blend are zero, Normalize maps
        // them to (0, 1, 0) (first-nonzero-axis arm with 0.0 as positive).
        human.set_dir([0.0, 0.0, 0.0]);
        assert!(human.apply_go_back_left(false));
        for (got, want) in human.velocity().iter().zip([0.0, 1.5, 0.0]) {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
    }
    #[test]
    // GAME 0x57dc10 (RVA 0x17dc10) C_human::Go_BackRight (mafia-re 113b5b6
    // game/actors/C_human.cpp:5488): same guard/entry/scaling as Go_BackLeft
    // with the mirrored blend and the BACKR1 select, but no normalization:
    // the raw blend scales directly (the IsMrgZeroLess probe result is
    // discarded in the original and has no observable effect).
    fn go_back_right_rva_17dc10_scales_raw_blend() {
        let mut human = HumanState::construct(speed_unit_seed());
        human.set_dir([1.0, 0.0, 0.0]);
        // dir (1,0,0): side (0,0,-1), raw blend (-1,0,-1) scaled by 1.5.
        assert!(human.apply_go_back_right(false));
        assert_eq!(human.action_state(), HUMAN_ACTION_WALK);
        assert_eq!(human.turn_speed(), 0.0);
        assert_eq!(human.next_anim_id(), ANIM_BACKR1);
        for (got, want) in human.velocity().iter().zip([-1.5, 0.0, -1.5]) {
            assert!((got - want).abs() < 1e-5, "got {got}, want {want}");
        }
    }
    #[test]
    // GAME 0x57f8a0 (RVA 0x17f8a0) C_human::Do_Crouched (mafia-re 113b5b6
    // game/actors/C_human.cpp:6098): carried box/dead body clears the latch;
    // standing up while crouched needs the headroom verdict; otherwise the
    // latch takes the request.
    fn do_crouched_rva_17f8a0_carry_hold_and_release() {
        let mut human = HumanState::construct(seed());
        assert!(!human.crouching());
        human.apply_do_crouched(true, true);
        assert!(human.crouching());
        // Standing up with headroom: the latch releases.
        human.apply_do_crouched(false, true);
        assert!(!human.crouching());
        // Standing up without headroom: the latch holds (probe hit).
        human.apply_do_crouched(true, true);
        human.apply_do_crouched(false, false);
        assert!(human.crouching());
        // A carried box clears the latch and returns, ignoring the request.
        human.set_carrying_box(true);
        human.apply_do_crouched(true, true);
        assert!(!human.crouching());
        human.set_carrying_box(false);
        // A carried dead body behaves like the box.
        human.set_carrying_dead_body(true);
        human.apply_do_crouched(true, true);
        assert!(!human.crouching());
        human.set_carrying_dead_body(false);
        assert!(!human.crouching());
    }
    #[test]
    // GAME 0x58ac50 (RVA 0x18ac50) C_human::Force_CanWorking (mafia-re
    // 113b5b6 game/actors/C_human.cpp:9028): unconditionally restores
    // HUMAN_ACTION_STAND.
    fn force_can_working_rva_18ac50_restores_stand() {
        let mut human = HumanState::construct(seed());
        human.action_state = HUMAN_ACTION_WALK;
        human.apply_force_can_working();
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        human.action_state = HUMAN_ACTION_JUMP;
        human.apply_force_can_working();
        assert_eq!(human.action_state(), 1);
    }
    #[test]
    // GAME 0x58ac60 (RVA 0x18ac60) C_human::Turn_Turn (mafia-re 113b5b6
    // game/actors/C_human.cpp:9034): selects ANIM_TURN1 and stages the
    // PersonAnim(0.0, true, false) handoff; velocity_ramp/last_velocity stay
    // with their future owners.
    fn turn_turn_rva_18ac60_selects_turn_and_hands_off() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.person_anim_request(), None);
        human.apply_turn_turn();
        assert_eq!(human.next_anim_id(), ANIM_TURN1);
        assert_eq!(human.next_anim_id(), 0xa);
        assert_eq!(
            human.person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            })
        );
    }
    #[test]
    // GAME 0x58a960 (RVA 0x18a960) C_human::Do_CornerBreath (mafia-re 113b5b6
    // game/actors/C_human.cpp:8922): the corner arms select the stat anims
    // and admit; CORNER_2 and carried loads refuse with the state untouched.
    fn corner_breath_rva_18a960_selects_stat_anims() {
        let mut human = HumanState::construct(seed());
        assert!(human.apply_do_corner_breath(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_STAT_L);
        assert!(human.apply_do_corner_breath(CornerView::Corner1));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_STAT_R);
        assert!(!human.apply_do_corner_breath(CornerView::Corner2));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_STAT_R);
        // No PersonAnim call on this path: only next_anim_id is armed.
        assert_eq!(human.person_anim_request(), None);
        human.set_carrying_box(true);
        assert!(!human.apply_do_corner_breath(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_STAT_R);
        human.set_carrying_box(false);
        human.set_carrying_dead_body(true);
        assert!(!human.apply_do_corner_breath(CornerView::Corner1));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_STAT_R);
    }
    #[test]
    // GAME 0x58a9a0 (RVA 0x18a9a0) C_human::Do_CornerView (mafia-re 113b5b6
    // game/actors/C_human.cpp:8939): the stand peeks admit with the
    // SetWaitAnim handoff; CORNER_2 needs the crouch and otherwise refuses;
    // carried loads refuse.
    fn corner_view_rva_18a9a0_gates_crouch_peek() {
        let mut human = HumanState::construct(seed());
        assert!(human.apply_do_corner_view(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_NAHL_L);
        assert_eq!(
            human.person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            })
        );
        assert!(human.apply_do_corner_view(CornerView::Corner1));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_NAHL_R);
        // Standing CORNER_2 refuses with the state untouched.
        assert!(!human.apply_do_corner_view(CornerView::Corner2));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_STOJ_NAHL_R);
        human.set_crouching(true);
        assert!(human.apply_do_corner_view(CornerView::Corner2));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_DREP_NAHL_T);
        human.set_carrying_dead_body(true);
        assert!(!human.apply_do_corner_view(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_DREP_NAHL_T);
    }
    #[test]
    // GAME 0x587cb0 (RVA 0x187cb0) C_human::Do_ShockUp (mafia-re 113b5b6
    // game/actors/C_human.cpp:8143): only the three panic stativs advance by
    // one with the PersonAnim handoff; other anims and carried loads leave
    // the state untouched.
    fn shock_up_rva_187cb0_advances_only_stativs() {
        let mut human = HumanState::construct(seed());
        for (stativ, advanced) in [
            (ANIM_X_PANIKA_DREP_STATIV, 0x3f),
            (ANIM_X_PANIKA_NA4_STATIV, 0x42),
            (ANIM_X_PANIKA_LEH_STATIV, 0x45),
        ] {
            human.anim_id = stativ;
            human.next_anim_id = 0;
            human.apply_do_shock_up();
            assert_eq!(human.next_anim_id(), advanced);
            assert_eq!(
                human.person_anim_request(),
                Some(HumanPersonAnimRequest {
                    blend: 0.0,
                    stance: true,
                    play_once: false,
                })
            );
            human.take_person_anim_request();
        }
        // A non-stativ anim leaves the words and the handoff untouched.
        human.anim_id = ANIM_BREATH01A;
        human.next_anim_id = 0x42;
        human.apply_do_shock_up();
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
        // Carried loads guard the whole body.
        human.anim_id = ANIM_X_PANIKA_DREP_STATIV;
        human.set_carrying_box(true);
        human.apply_do_shock_up();
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
    }
    #[test]
    // GAME 0x587cf0 (RVA 0x187cf0) C_human::Do_ShockStumble (mafia-re 113b5b6
    // game/actors/C_human.cpp:8156): only the three panic runs advance by one
    // (verbatim anim_id + 1) with the PersonAnim handoff.
    fn shock_stumble_rva_187cf0_advances_only_panic_runs() {
        let mut human = HumanState::construct(seed());
        for (run, advanced) in [
            (ANIM_PANIKA_RUN01A, 0x47),
            (ANIM_PANIKA_RUN03A, 0x49),
            (ANIM_PANIKA_RUN02A, 0x4b),
        ] {
            human.anim_id = run;
            human.next_anim_id = 0;
            human.apply_do_shock_stumble();
            assert_eq!(human.next_anim_id(), advanced);
            assert_eq!(
                human.person_anim_request(),
                Some(HumanPersonAnimRequest {
                    blend: 0.0,
                    stance: true,
                    play_once: false,
                })
            );
            human.take_person_anim_request();
        }
        human.anim_id = ANIM_WALK1;
        human.next_anim_id = 0x42;
        human.apply_do_shock_stumble();
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
        human.anim_id = ANIM_PANIKA_RUN01A;
        human.set_carrying_dead_body(true);
        human.apply_do_shock_stumble();
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
    }
    #[test]
    // GAME 0x58b7e0 (RVA 0x18b7e0) C_human::Do_DeadBodyDrop (mafia-re 113b5b6
    // game/actors/C_human.cpp:9271): without a carried body this is a no-op;
    // otherwise anim_id clears, the shozeni anim selects and the
    // PersonAnim/SetWaitAnim handoff stages (the body release and the wait
    // timer stay with their owners).
    fn dead_body_drop_rva_18b7e0_guards_and_hands_off() {
        let mut human = HumanState::construct(seed());
        human.anim_id = 7;
        human.next_anim_id = 0x42;
        human.apply_do_dead_body_drop();
        assert_eq!(human.anim_id(), 7);
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
        human.set_carrying_dead_body(true);
        human.apply_do_dead_body_drop();
        assert_eq!(human.anim_id(), 0);
        assert_eq!(human.next_anim_id(), ANIM_NOSENI_MRTVOLY_TOM_SHOZENI);
        assert_eq!(human.next_anim_id(), 0xa8);
        assert_eq!(
            human.person_anim_request(),
            Some(HumanPersonAnimRequest {
                blend: 0.0,
                stance: true,
                play_once: false,
            })
        );
        // The body release itself stays with the body owner: the latch is
        // not cleared by this slice.
        assert!(human.carrying_dead_body());
    }
}

#[cfg(test)]
mod human_state_flags_tests {
    use super::*;
    use crate::script_scheduler::ActorReferenceId;
    fn seed() -> HumanSeed {
        HumanSeed {
            allocator_tag: 0xd7,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xa5a5_a5a5,
            },
            previous_animation_code: 0,
            requested_animation_code: 0,
            human_type_mask: 0,
            current_properties: HumanProperties::from_record([0; 64]),
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
    fn handoff() -> Option<HumanPersonAnimRequest> {
        Some(HumanPersonAnimRequest {
            blend: 0.0,
            stance: true,
            play_once: false,
        })
    }
    #[test]
    // GAME 0x47b810 (RVA 0x7b810) C_human::IsReady (mafia-re 113b5b6
    // game/actors/C_human.h:1147-1151): alive plus STAND/WALK/CAR without a
    // blocking animation; dead, other actions and blocking refuse.
    fn is_ready_rva_7b810_reads_alive_action_and_blocking() {
        let mut human = HumanState::construct(seed());
        assert!(!human.is_ready(false));
        human.set_alive(true);
        assert!(human.is_ready(false));
        assert!(!human.is_ready(true));
        human.action_state = HUMAN_ACTION_WALK;
        assert!(human.is_ready(false));
        human.action_state = HUMAN_ACTION_CAR;
        assert!(human.is_ready(false));
        human.action_state = HUMAN_ACTION_FALL;
        assert!(!human.is_ready(false));
        human.set_alive(false);
        assert!(!human.is_ready(false));
    }
    #[test]
    // GAME 0x57a630 (RVA 0x17a630) C_human::GetAnimIDForDeath (mafia-re
    // 113b5b6 game/actors/C_human.cpp:4723-4748): the crouch/head/range arms
    // with the caller draws added verbatim.
    fn death_anim_rva_17a630_selects_part_stance_and_range() {
        assert_eq!(
            HumanState::get_anim_id_for_death(HIT_PART_HEAD, false, true, 1, 0),
            0x89 + 2
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(HIT_PART_HEAD, false, true, 0, 0),
            0x89
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(HIT_PART_HEAD, false, false, 1, 0),
            0x8a + 1
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(HIT_PART_HEAD, true, true, 1, 0),
            0x90 + 2
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(HIT_PART_HEAD, true, false, 0, 0),
            0x91
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(0, false, true, 1, 1),
            0x83 + 1
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(0, true, false, 2, 0),
            0x8e + 2
        );
        assert_eq!(
            HumanState::get_anim_id_for_death(0, false, false, 0, 2),
            0x86 + 2
        );
    }
    #[test]
    // GAME 0x58e270 (RVA 0x18e270) C_human::Can_Hit_FilterHumanType (mafia-re
    // 113b5b6 game/actors/C_human.cpp:10083-10129): zero masks and
    // out-of-table bit numbers admit; the table cells refuse as evidenced;
    // the (1, 0) cell follows the caller anger verdict.
    fn hit_filter_rva_18e270_applies_table_and_gates() {
        assert!(HumanState::can_hit_filter_human_type(0, 5, false));
        assert!(HumanState::can_hit_filter_human_type(5, 0, false));
        assert!(HumanState::can_hit_filter_human_type(2, 1, true));
        assert!(!HumanState::can_hit_filter_human_type(2, 1, false));
        assert!(HumanState::can_hit_filter_human_type(4, 8, false));
        assert!(!HumanState::can_hit_filter_human_type(8, 8, false));
        assert!(!HumanState::can_hit_filter_human_type(8, 16, false));
        assert!(HumanState::can_hit_filter_human_type(16, 2, false));
        assert!(HumanState::can_hit_filter_human_type(0x800, 1, false));
        assert!(HumanState::can_hit_filter_human_type(1, 0x800, false));
    }
    #[test]
    // GAME 0x58b870 (RVA 0x18b870) C_human::ResetActions (mafia-re 113b5b6
    // game/actors/C_human.cpp:9282-9300): crouch/aim/held-shot latches drop
    // and the alive path re-arms the breath with the handoff; the dead path
    // leaves the anim words alone and a blocked headroom keeps the crouch.
    fn reset_actions_rva_18b870_drops_latches_and_rebreathes_alive() {
        let mut human = HumanState::construct(seed());
        human.set_alive(true);
        human.set_crouching(true);
        human.apply_do_aimed(true);
        human.set_fire_held(true);
        human.anim_id = 7;
        human.next_anim_id = 0x42;
        human.apply_reset_actions(true);
        assert!(!human.crouching());
        assert!(!human.aimed());
        assert!(!human.fire_held());
        assert_eq!(human.anim_id(), 0);
        assert_eq!(human.next_anim_id(), ANIM_BREATH01A);
        assert_eq!(human.person_anim_request(), handoff());
        let mut dead = HumanState::construct(seed());
        dead.anim_id = 7;
        dead.next_anim_id = 0x42;
        dead.apply_reset_actions(true);
        assert_eq!(dead.anim_id(), 7);
        assert_eq!(dead.next_anim_id(), 0x42);
        assert_eq!(dead.person_anim_request(), None);
        let mut low = HumanState::construct(seed());
        low.set_alive(true);
        low.set_crouching(true);
        low.apply_reset_actions(false);
        assert!(low.crouching());
    }
    #[test]
    // GAME 0x58a790 (RVA 0x18a790) C_human::Can_Aimed (mafia-re 113b5b6
    // game/actors/C_human.cpp:8911-8918): the probe verdict inverts.
    fn can_aimed_rva_18a790_inverts_wall_probe() {
        assert!(HumanState::can_aimed(false));
        assert!(!HumanState::can_aimed(true));
    }
    #[test]
    // GAME 0x58cfb0 (RVA 0x18cfb0) C_human::IsThisCarStealing (mafia-re
    // 113b5b6 game/actors/C_human.cpp:9592-9610): only PLAYER/ENTITY
    // claimants holding the car in any latch count; NULL admits nothing.
    fn car_claim_rva_18cfb0_matches_player_entity_latches() {
        let car = ActorReferenceId::from_existing(11);
        let other = ActorReferenceId::from_existing(12);
        let holder = CarTheftClaimant {
            actor_type: ACTOR_PLAYER,
            stealing_car: Some(car),
            car: None,
            using_car: None,
        };
        let driver = CarTheftClaimant {
            actor_type: ACTOR_ENTITY,
            stealing_car: None,
            car: None,
            using_car: Some(car),
        };
        let traffic = CarTheftClaimant {
            actor_type: 4,
            stealing_car: None,
            car: Some(car),
            using_car: None,
        };
        let stranger = CarTheftClaimant {
            actor_type: ACTOR_PLAYER,
            stealing_car: Some(other),
            car: None,
            using_car: None,
        };
        assert!(!HumanState::car_has_human_claimant(None, &[holder]));
        assert!(HumanState::car_has_human_claimant(Some(car), &[holder]));
        assert!(HumanState::car_has_human_claimant(
            Some(car),
            &[traffic, stranger, driver]
        ));
        assert!(!HumanState::car_has_human_claimant(
            Some(car),
            &[traffic, stranger]
        ));
        assert!(!HumanState::car_has_human_claimant(Some(car), &[]));
    }
    #[test]
    // GAME 0x58aa60 (RVA 0x18aa60) C_human::Do_CornerFire (mafia-re 113b5b6
    // game/actors/C_human.cpp:8963-8993): rifles, fists, loads and standing
    // CORNER_2 refuse; the Thompson selects its own standing anims and the
    // admission latches aim with the handoff.
    fn corner_fire_rva_18aa60_selects_sides_and_thompson() {
        let mut human = HumanState::construct(seed());
        assert!(!human.apply_do_corner_fire(CornerView::Corner0));
        human.set_weapon_kind(1);
        assert!(human.apply_do_corner_fire(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_FIRE01_L);
        assert!(human.aimed());
        assert_eq!(human.person_anim_request(), handoff());
        human.set_right_hand_fire_anim(GUN_ANIM_THOMPSON);
        assert!(human.apply_do_corner_fire(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_SAP_STOJ_FIRE01_L);
        assert!(human.apply_do_corner_fire(CornerView::Corner1));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_SAP_STOJ_FIRE01_R);
        human.set_right_hand_fire_anim(0);
        assert!(human.apply_do_corner_fire(CornerView::Corner1));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_FIRE01_R);
        assert!(!human.apply_do_corner_fire(CornerView::Corner2));
        human.set_crouching(true);
        assert!(human.apply_do_corner_fire(CornerView::Corner2));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_DREP_FIRE01_T);
        human.set_weapon_kind(WEAPON_KIND_RIFLE);
        assert!(!human.apply_do_corner_fire(CornerView::Corner0));
        human.set_weapon_kind(1);
        human.set_carrying_box(true);
        assert!(!human.apply_do_corner_fire(CornerView::Corner0));
    }
    #[test]
    // GAME 0x58ab80 (RVA 0x18ab80) C_human::Do_CornerFireAimed (mafia-re
    // 113b5b6 game/actors/C_human.cpp:8995-9025): the FIRE02-side mirror of
    // Do_CornerFire with the direct PersonAnim handoff.
    fn corner_fire_aimed_rva_18ab80_selects_aimed_sides() {
        let mut human = HumanState::construct(seed());
        human.set_weapon_kind(1);
        assert!(human.apply_do_corner_fire_aimed(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_FIRE02_L);
        human.set_right_hand_fire_anim(GUN_ANIM_THOMPSON);
        assert!(human.apply_do_corner_fire_aimed(CornerView::Corner0));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_SAP_STOJ_FIRE02_L);
        human.set_right_hand_fire_anim(0);
        assert!(human.apply_do_corner_fire_aimed(CornerView::Corner1));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_FIRE02_R);
        assert!(!human.apply_do_corner_fire_aimed(CornerView::Corner2));
        human.set_crouching(true);
        assert!(human.apply_do_corner_fire_aimed(CornerView::Corner2));
        assert_eq!(human.next_anim_id(), ANIM_ENM_ROH_DREP_FIRE02_T);
        assert!(human.aimed());
        assert_eq!(human.person_anim_request(), handoff());
        human.set_weapon_kind(WEAPON_KIND_FISTS);
        assert!(!human.apply_do_corner_fire_aimed(CornerView::Corner0));
    }
    #[test]
    // GAME 0x585f40 (RVA 0x185f40) C_human::GetBoundPoints2D (mafia-re
    // 113b5b6 game/actors/C_human.cpp:7630-7638): the four axis points 0.7 m
    // around the staged position.
    fn bound_points_rva_185f40_offsets_axes_by_seven_tenths() {
        let points = HumanState::bound_points_2d(1.0, 2.0);
        let want = [[1.7, 2.0], [0.3, 2.0], [1.0, 2.7], [1.0, 1.3]];
        for (got, want) in points.iter().flatten().zip(want.iter().flatten()) {
            assert!((got - want).abs() < 1e-6, "got {got}, want {want}");
        }
    }
    #[test]
    // GAME 0x584480 (RVA 0x184480) C_human::Do_Castrate (mafia-re 113b5b6
    // game/actors/C_human.cpp:7213-7234): missing/unready/cheated targets
    // refuse; admission selects both strike anims with handoffs and bills
    // strength * 10 energy to the target.
    fn castrate_rva_184480_bills_strike_energy() {
        let mut striker = HumanState::construct(seed());
        assert!(!striker.apply_do_castrate(None, true, false));
        let mut target = HumanState::construct(seed());
        assert!(!striker.apply_do_castrate(Some(&mut target), false, false));
        assert!(!striker.apply_do_castrate(Some(&mut target), true, true));
        striker
            .current_properties
            .set_component_bits(HUMAN_PROPERTY_STRENGTH, 2.0f32.to_bits());
        target
            .current_properties
            .set_component_bits(HUMAN_PROPERTY_ENERGY, 100.0f32.to_bits());
        assert!(striker.apply_do_castrate(Some(&mut target), true, false));
        assert_eq!(striker.next_anim_id(), ANIM_BOJ_BOX_ROZKROK);
        assert_eq!(striker.person_anim_request(), handoff());
        assert_eq!(target.next_anim_id(), ANIM_BOJ_ZASAH_ROZKROK_SM);
        assert_eq!(target.person_anim_request(), handoff());
        assert_eq!(
            target
                .current_properties
                .float_component(HUMAN_PROPERTY_ENERGY),
            80.0
        );
    }
    #[test]
    // GAME 0x584550 (RVA 0x184550) C_human::Do_HardHead (mafia-re 113b5b6
    // game/actors/C_human.cpp:7236-7257): the headbutt mirror of Do_Castrate.
    fn hard_head_rva_184550_bills_strike_energy() {
        let mut striker = HumanState::construct(seed());
        let mut target = HumanState::construct(seed());
        assert!(!striker.apply_do_castrate(None, true, false));
        assert!(!striker.apply_do_hard_head(Some(&mut target), false, false));
        assert!(!striker.apply_do_hard_head(Some(&mut target), true, true));
        striker
            .current_properties
            .set_component_bits(HUMAN_PROPERTY_STRENGTH, 1.5f32.to_bits());
        target
            .current_properties
            .set_component_bits(HUMAN_PROPERTY_ENERGY, 50.0f32.to_bits());
        assert!(striker.apply_do_hard_head(Some(&mut target), true, false));
        assert_eq!(striker.next_anim_id(), ANIM_BOJ_BOX_HLAVICKA);
        assert_eq!(target.next_anim_id(), ANIM_BOJ_ZASAH_HLAVICKA);
        assert_eq!(striker.person_anim_request(), handoff());
        assert_eq!(target.person_anim_request(), handoff());
        assert_eq!(
            target
                .current_properties
                .float_component(HUMAN_PROPERTY_ENERGY),
            35.0
        );
    }
    #[test]
    // GAME 0x587bc0 (RVA 0x187bc0) C_human::Do_ShockStart (mafia-re 113b5b6
    // game/actors/C_human.cpp:8107-8118): loads guard, the variant clamps to
    // 0..3, and the draw picks the startle id with the handoff.
    fn shock_start_rva_187bc0_clamps_variant_and_picks_draw() {
        let mut human = HumanState::construct(seed());
        human.next_anim_id = 0x42;
        human.set_carrying_box(true);
        human.apply_do_shock_start(2, 1);
        assert_eq!(human.next_anim_id(), 0x42);
        assert_eq!(human.person_anim_request(), None);
        human.set_carrying_box(false);
        human.apply_do_shock_start(-5, 1);
        assert_eq!(human.next_anim_id(), ANIM_PANIKA_LEKNUTI01L);
        assert_eq!(human.person_anim_request(), handoff());
        human.apply_do_shock_start(99, 0);
        assert_eq!(human.next_anim_id(), ANIM_PANIKA_LEKNUTI02L + 3);
        human.apply_do_shock_start(2, 7);
        assert_eq!(human.next_anim_id(), ANIM_PANIKA_LEKNUTI01L + 2);
    }
    #[test]
    // GAME 0x575fc0 (RVA 0x175fc0) C_human::ActivateColls (mafia-re 113b5b6
    // game/actors/C_human.cpp:3635): seated returns None with no latch
    // write; otherwise the latch sets and the mode arms select the insert
    // mask over the staged flag words (unknown modes insert nothing).
    fn activate_colls_rva_175fc0_gates_car_and_selects_by_mode() {
        let mut human = HumanState::construct(seed());
        human.set_alive(true);
        assert_eq!(
            human.apply_activate_colls(true, HUMAN_COLLS_ALL, &[DYNCOLLF_LOWER]),
            None
        );
        assert!(!human.colls_active());
        let flags = [DYNCOLLF_UPPER, DYNCOLLF_LOWER, 0];
        assert_eq!(
            human.apply_activate_colls(false, HUMAN_COLLS_ALL, &flags),
            Some(vec![true, true, true])
        );
        assert!(human.colls_active());
        assert_eq!(
            human.apply_activate_colls(false, HUMAN_COLLS_UPPER, &flags),
            Some(vec![true, false, false])
        );
        assert_eq!(
            human.apply_activate_colls(false, HUMAN_COLLS_LOWER, &flags),
            Some(vec![false, true, false])
        );
        assert_eq!(
            human.apply_activate_colls(false, 99, &flags),
            Some(vec![false, false, false])
        );
        human.set_alive(false);
        assert_eq!(
            human.apply_activate_colls(false, HUMAN_COLLS_ALL, &flags),
            Some(vec![true, false, false])
        );
        assert_eq!(
            human.apply_activate_colls(false, HUMAN_COLLS_UPPER, &flags),
            Some(vec![false, false, false])
        );
    }
    #[test]
    // GAME 0x575f60 (RVA 0x175f60) C_human::DeactivateColls (mafia-re 113b5b6
    // game/actors/C_human.cpp:3619): the latch clears and every staged flag
    // word selects erasure except an upper-flagged one while kept.
    fn deactivate_colls_rva_175f60_clears_latch_and_selects_erase() {
        let mut human = HumanState::construct(seed());
        let flags = [DYNCOLLF_UPPER, DYNCOLLF_LOWER];
        assert_eq!(
            human.apply_activate_colls(false, HUMAN_COLLS_ALL, &flags),
            Some(vec![true, false])
        );
        assert!(human.colls_active());
        assert_eq!(
            human.apply_deactivate_colls(false, &flags),
            vec![true, true]
        );
        assert!(!human.colls_active());
        assert_eq!(
            human.apply_deactivate_colls(true, &flags),
            vec![false, true]
        );
    }
    #[test]
    // GAME 0x587c20 (RVA 0x187c20) C_human::Do_ShockDown (mafia-re 113b5b6
    // game/actors/C_human.cpp:8121): carrying refuses; otherwise the variant
    // clamps to 0..2 and the two panic-down steps plus the end fill with the
    // STEPS action and a zeroed step index.
    fn shock_down_rva_187c20_clamps_variant_and_fills_steps() {
        let mut human = HumanState::construct(seed());
        human.set_carrying_box(true);
        human.apply_do_shock_down(1);
        assert_eq!(human.action_state(), HUMAN_ACTION_STAND);
        human.set_carrying_box(false);
        human.apply_do_shock_down(-5);
        assert_eq!(human.action_state(), HUMAN_ACTION_STEPS);
        assert_eq!(human.anim_step(), 0);
        assert_eq!(human.anim_steps()[0].anim_id, ANIM_X_PANIKA_DREP_DOLU);
        assert_eq!(human.anim_steps()[0].step_type, HUMAN_STEP_ANIM);
        assert!(!human.anim_steps()[0].stance);
        assert_eq!(human.anim_steps()[1].anim_id, ANIM_X_PANIKA_DREP_STATIV);
        assert_eq!(human.anim_steps()[1].step_type, HUMAN_STEP_ANIM);
        assert_eq!(human.anim_steps()[2].step_type, HUMAN_STEP_END);
        human.apply_do_shock_down(99);
        assert_eq!(
            human.anim_steps()[0].anim_id,
            2 * 3 + ANIM_X_PANIKA_DREP_DOLU
        );
        assert_eq!(
            human.anim_steps()[1].anim_id,
            2 * 3 + ANIM_X_PANIKA_DREP_STATIV
        );
    }
    #[test]
    // GAME 0x573ce0 (RVA 0x173ce0) C_human::ChangeDeadAnim (mafia-re 113b5b6
    // game/actors/C_human.cpp:2688): the carrier's masked next id selects
    // the dead's animation file plus the simple/unknown pair; set bits
    // above the mask select the same rows.
    fn dead_body_anim_rva_173ce0_maps_carrier_anim_to_dead_anim() {
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_WALK1),
            ("NoseniMrtvolyMrtWalk.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_WALKL1),
            ("NoseniMrtvolyMrtWalk.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_BACKR1),
            ("NoseniMrtvolyMrtBack.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_LEFT1),
            ("NoseniMrtvolyMrtLeft.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_RIGHT1),
            ("NoseniMrtvolyMrtRight.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_STRAFL1),
            ("NoseniMrtvolyMrtStrafL.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_STRAFR1),
            ("NoseniMrtvolyMrtStrafR.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_NOSENI_MRTVOLY_TOM_NAHOZENI),
            ("NoseniMrtvolyMrtNahozeni.I3D", true, true)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(ANIM_NOSENI_MRTVOLY_TOM_SHOZENI),
            ("NoseniMrtvolyMrtShozeni.I3D", true, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(0x42),
            ("NoseniMrtvolyMrtStat.I3D", false, false)
        );
        assert_eq!(
            HumanState::dead_body_anim_for_carrier(0x800 | ANIM_BACK1),
            ("NoseniMrtvolyMrtBack.I3D", false, false)
        );
    }
    #[test]
    // GAME 0x58df60 (RVA 0x18df60) C_human::ResetWeaponEffects (mafia-re
    // 113b5b6 game/actors/C_human.cpp:9998): the ready table selects and
    // the step and its time clear.
    fn reset_weapon_effects_rva_18df60_selects_ready_and_clears_words() {
        let mut human = HumanState::construct(seed());
        human.fire_step = 7;
        human.fire_time = 9;
        human.apply_reset_weapon_effects();
        assert_eq!(human.fire_action(), FireActionTable::Ready);
        assert_eq!(human.fire_step(), 0);
        assert_eq!(human.fire_time(), 0);
    }
    #[test]
    // GAME 0x584010 (RVA 0x184010) C_human::Can_SpecialStroke (mafia-re
    // 113b5b6 game/actors/C_human.cpp:7190): the tail-first scan admits a
    // close turned-away ready stranger of sufficient energy and refuses
    // wrong kinds, idle states, crouch, unreadiness, weak energy, distance
    // and facing misses.
    fn special_stroke_rva_184010_scans_tail_first_with_gates() {
        let mut searcher = HumanState::construct(seed());
        searcher.set_dir([0.0, 0.0, 1.0]);
        searcher
            .properties
            .set_component_bits(HUMAN_PROPERTY_STRENGTH, 1.0f32.to_bits());
        let candidate = SpecialStrokeCandidate {
            actor_type: ACTOR_ENTITY,
            active_state: true,
            crouching: false,
            alive: true,
            action_state: HUMAN_ACTION_STAND,
            anim_blocking: false,
            energy: 50.0,
            position: [0.0, 0.0, 0.5],
            facing: [0.0, 0.0, -1.0],
        };
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[candidate]),
            Some(0)
        );
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[candidate, candidate]),
            Some(1)
        );
        let mut bad = candidate;
        bad.actor_type = 7;
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.active_state = false;
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.crouching = true;
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.action_state = HUMAN_ACTION_JUMP;
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.anim_blocking = true;
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.energy = 5.0;
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.position = [0.0, 0.0, 2.0];
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
        bad = candidate;
        bad.facing = [0.0, 0.0, 1.0];
        assert_eq!(
            searcher.find_special_stroke_target([0.0, 0.0, 0.0], &[bad]),
            None
        );
    }
    #[test]
    // GAME 0x58b470 (RVA 0x18b470) C_human::NewScream (mafia-re 113b5b6
    // game/actors/C_human.cpp:9208): the gate admits and stores the
    // candidate priority; shut-up, voiceless, area, table, talk-over,
    // over-500 and soundless misses refuse without touching the word.
    fn scream_gate_rva_18b470_gates_and_stores_priority() {
        let mut human = HumanState::construct(seed());
        assert!(human.apply_new_scream_gate(true, 1, 1, 2, 7, false, 100, true));
        assert_eq!(human.voice_priority(), 100);
        assert!(!human.apply_new_scream_gate(true, 1, 1, 3, 7, true, 50, true));
        assert!(!human.apply_new_scream_gate(true, 1, 1, 3, 7, true, 100, true));
        human.voice_priority = 600;
        assert!(!human.apply_new_scream_gate(true, 1, 1, 3, 7, false, 100, true));
        human.voice_priority = -1;
        assert!(!human.apply_new_scream_gate(true, 1, 1, 1, 7, false, 100, true));
        assert!(!human.apply_new_scream_gate(true, 1, 1, 9, 7, false, 100, true));
        assert!(!human.apply_new_scream_gate(true, 1, 1, 2, 7, false, 100, false));
        assert!(!human.apply_new_scream_gate(true, 0, 1, 2, 7, false, 100, true));
        assert!(!human.apply_new_scream_gate(false, 1, 1, 2, 7, false, 100, true));
        human.set_shut_up(true);
        assert!(!human.apply_new_scream_gate(true, 1, 1, 2, 7, false, 100, true));
        assert_eq!(human.voice_priority(), -1);
    }
    #[test]
    // GAME 0x578e20 (RVA 0x178e20) C_human::SetCutsceneLink (mafia-re 113b5b6
    // game/actors/C_human.cpp:4539): unrecorded stages nothing; recorded
    // allocates once, then marks new with the kind, placement and name,
    // truncating overlong names to the link word.
    fn cutscene_link_rva_178e20_gates_recorded_and_fills_link() {
        let mut human = HumanState::construct(seed());
        human.apply_set_cutscene_link(3, Some("Tommy"), [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(human.cutscene_link(), None);
        human.set_cutscene_recorded(true);
        human.apply_set_cutscene_link(3, Some("Tommy"), [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 1.0]);
        let link = human.cutscene_link().expect("link staged while recorded");
        assert!(link.is_new);
        assert_eq!(link.kind, 3);
        assert_eq!(link.pos, [1.0, 2.0, 3.0]);
        assert_eq!(link.rot, [0.0, 0.0, 0.0, 1.0]);
        assert_eq!(&link.name[..6], b"Tommy\0");
        human.apply_set_cutscene_link(4, None, [0.0; 3], [0.0; 4]);
        let link = human.cutscene_link().expect("link allocation reused");
        assert_eq!(link.kind, 4);
        assert_eq!(link.name, [0u8; CUTSCENE_LINK_NAME_LEN]);
        let long = "x".repeat(100);
        human.apply_set_cutscene_link(1, Some(long.as_str()), [0.0; 3], [0.0; 4]);
        let link = human.cutscene_link().expect("link allocation reused");
        assert_eq!(link.name[CUTSCENE_LINK_NAME_LEN - 1], 0);
        assert_eq!(link.name[0], b'x');
    }
    #[test]
    // GAME 0x51af20 (RVA 0x11af20) C_human::GetActorSpeed (mafia-re 113b5b6
    // game/actors/C_human.h:678, inline): the staged walk velocity
    // publishes into the caller's speed word.
    fn actor_speed_rva_11af20_publishes_velocity() {
        let mut human = HumanState::construct(seed());
        human.velocity = [1.0, 2.0, 3.0];
        let mut speed = [9.0; 3];
        human.actor_speed_into(&mut speed);
        assert_eq!(speed, [1.0, 2.0, 3.0]);
    }
    #[test]
    // GAME 0x57a5f0 (RVA 0x17a5f0) C_human::PoseValidate (mafia-re 113b5b6
    // game/actors/C_human.cpp:4714): explicit unsupported stub, frame-only
    // body; invoking it touches no staged words.
    fn pose_validate_rva_17a5f0_stub_runs_without_state() {
        HumanState::pose_validate_unsupported(true);
        HumanState::pose_validate_unsupported(false);
    }
    #[test]
    // GAME 0x5802f0 (RVA 0x1802f0) C_human::Can_DropOutFromCar_Free (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6284): explicit unsupported stub,
    // collision-probe program; invoking it produces no verdict.
    fn drop_out_free_rva_1802f0_stub_runs_without_verdict() {
        HumanState::can_drop_out_from_car_free_unsupported();
    }
    #[test]
    // GAME 0x5805c0 (RVA 0x1805c0) C_human::Can_DropOutFromCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6326): explicit unsupported stub,
    // same collision boundary as the _Free probe; no verdict produced.
    fn drop_out_rva_1805c0_stub_runs_without_verdict() {
        HumanState::can_drop_out_from_car_unsupported();
    }
    #[test]
    // GAME 0x5706a0 (RVA 0x1706a0) C_human::SoundsChange (mafia-re 113b5b6
    // game/actors/C_human.cpp:1710): explicit unsupported stub, sound
    // owners; invoking it touches no staged words.
    fn sounds_change_rva_1706a0_stub_runs_without_state() {
        HumanState::apply_sounds_change_unsupported();
    }
    #[test]
    // GAME 0x5877a0 (RVA 0x1877a0) C_human::InitFFEffects (mafia-re 113b5b6
    // game/actors/C_human.cpp:8037): explicit unsupported stub, joystick
    // device owner; invoking it touches no staged words.
    fn init_ff_effects_rva_1877a0_stub_runs_without_state() {
        HumanState::init_ff_effects_unsupported();
    }
    #[test]
    // GAME 0x587880 (RVA 0x187880) C_human::DoneFFEffects (mafia-re 113b5b6
    // game/actors/C_human.cpp:8050): explicit unsupported stub, joystick
    // device owner; invoking it touches no staged words.
    fn done_ff_effects_rva_187880_stub_runs_without_state() {
        HumanState::done_ff_effects_unsupported();
    }
}
