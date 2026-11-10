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
//! turn_speed, dir, crouch/box/dead-body latches, aim/guard/change latches,
//! note-fire/reload/displaced/effects, sniper/ground/drown/backswing/strike/
//! tilt and limiter/bonus latches and carried-weapon ammo words staged in
//! this file; the PersonAnim,
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

/// GAME 0x58d270 (RVA 0x18d270) C_human::BezZasobnikuWeaponEffect (mafia-re
/// 113b5b6 game/actors/C_human.h:667, game/actors/C_human.cpp:9658-9667;
/// `BezZasobniku` is the upstream working name for the magazine-less effect,
/// NOT original): hide the weapon's "clip" frame when an effect step with id
/// 9 or 10 runs on a visual clip. The 9..=10 gate is the original literal.
/// The caller stages `clip_present` (the FindChildFrame("clip") hit) and
/// `clip_is_visual` (the FRAME_VISUAL type gate) because the model/frame
/// owner stays DEFERRED. Returns whether the clip is hidden by this leaf.
pub fn clip_hidden_without_mag(
    item_id: i32,
    clip_present: bool,
    clip_is_visual: bool,
) -> bool {
    (9..=10).contains(&item_id) && clip_present && clip_is_visual
}

/// Original Colt 1911 game-item id (mafia-re 113b5b6 game/g_material.h:127:
/// GAMEITEM_COLT_1911 9, "Colt 1911").
pub const GAMEITEM_COLT_1911: i32 = 9;

/// Slide travel of the Colt 1911 dry-fire effect (mafia-re 113b5b6
/// game/actors/C_human.cpp:10068: `z->SetPos(0.0f, 0.0f, -.04f)`).
pub const COLT_SLIDE_RECOIL_OFFSET_Z: f32 = -0.04;

/// GAME 0x58e210 (RVA 0x18e210) C_human::FakeWeaponEffect (mafia-re 113b5b6
/// game/actors/C_human.h:671, game/actors/C_human.cpp:10059-10071): for a
/// Colt 1911 the leaf slides the "zaver1" child back and updates it; any
/// other item id leaves the frame alone, and the trailing bool is unread.
/// The caller stages `slide_present` (the FindChildFrame("zaver1",
/// ENUMF_ALL) hit) because the model/frame owner stays DEFERRED. Returns the
/// new local slide position when the leaf applies, None otherwise.
pub fn colt_slide_recoil_shift(item_id: i32, slide_present: bool) -> Option<[f32; 3]> {
    if item_id == GAMEITEM_COLT_1911 && slide_present {
        Some([0.0, 0.0, COLT_SLIDE_RECOIL_OFFSET_Z])
    } else {
        None
    }
}

/// Door-placement geometry of C_human::Do_ThrowCocotFromCar (mafia-re 113b5b6
/// game/actors/C_human.h:631): the normalized side vector plus the selected
/// door offset, both in car-local space before the frame matrix runs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CocotDoorPlacement {
    pub side: [f32; 3],
    pub offset: [f32; 3],
}

/// GAME 0x587d70 (RVA 0x187d70) C_human::Do_ThrowCocotFromCar door-placement
/// core (mafia-re 113b5b6 game/actors/C_human.cpp:8199-8228): the side vector
/// from the car's world direction, negated on the seat-side flag and
/// normalized, plus the door offset selected by the door flag and the seat.
/// `seat_left_flag` (b1) and `door_flag` (b2) ride the car seat-property
/// owner, and the matrix/frame/animation/police/entity effects after the
/// placement stay DEFERRED; this leaf is pure caller-staged geometry.
pub fn throw_cocot_door_placement(
    car_dir: [f32; 3],
    seat_left_flag: bool,
    door_flag: bool,
    seat: i32,
) -> CocotDoorPlacement {
    let mut side = [car_dir[2], 0.0, -car_dir[0]];
    if seat_left_flag {
        side = [-side[0], -side[1], -side[2]];
    }
    let offset = match (door_flag, seat) {
        (true, 1) | (true, 3) => [1.581, 0.0, 0.251],
        (true, _) => [-1.2517, 0.0, -0.6723],
        (false, 1) | (false, 3) => [1.343, 0.0, 1.148],
        (false, _) => [-1.364, 0.0, 0.231],
    };
    CocotDoorPlacement {
        side: normalize_strafe_vector(side),
        offset,
    }
}

/// Caller-staged bridge projection for the high-bridge scan: the world
/// position plus the kind word (mafia-re 113b5b6
/// game/actors/C_human.cpp:6271: `unknown_144`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BridgeCandidate {
    pub position: [f32; 3],
    pub kind: i32,
}

/// Squared search radius of the high-bridge scan (mafia-re 113b5b6
/// game/actors/C_human.cpp:6271: `d.x * d.x + d.y * d.y + d.z * d.z < 10000.0f`,
/// i.e. 100 world units).
pub const HIGH_BRIDGE_RANGE_SQUARED: f32 = 10000.0;
/// Accepted bridge-kind window of the high-bridge scan (mafia-re 113b5b6
/// game/actors/C_human.cpp:6271: `unknown_144 >= 3 && unknown_144 <= 5`).
pub const HIGH_BRIDGE_KIND_MIN: i32 = 3;
/// Accepted bridge-kind window upper bound (see [`HIGH_BRIDGE_KIND_MIN`]).
pub const HIGH_BRIDGE_KIND_MAX: i32 = 5;

/// GAME 0x5801f0 (RVA 0x1801f0) high-bridge scan (mafia-re 113b5b6
/// game/actors/C_human.h:584, game/actors/C_human.cpp:6259-6277; the
/// upstream working name `TozNaselJsemTenNejvyssiMost` is a researcher joke,
/// NOT original): while seated in the target car, return the reverse-scan
/// first bridge within range whose kind sits in 3..=5. The in-car and
/// car-type gates arrive staged as `in_target_car`, and the global-actor type
/// filter as the pre-filtered `candidates` (frame/matrix owners stay
/// DEFERRED). Returns the winning candidate index in slice order.
pub fn find_nearby_high_bridge(
    own_pos: [f32; 3],
    in_target_car: bool,
    candidates: &[BridgeCandidate],
) -> Option<usize> {
    if !in_target_car {
        return None;
    }
    candidates.iter().enumerate().rev().find_map(|(index, candidate)| {
        let dx = own_pos[0] - candidate.position[0];
        let dy = own_pos[1] - candidate.position[1];
        let dz = own_pos[2] - candidate.position[2];
        let in_range = dx * dx + dy * dy + dz * dz < HIGH_BRIDGE_RANGE_SQUARED;
        let kind_ok = (HIGH_BRIDGE_KIND_MIN..=HIGH_BRIDGE_KIND_MAX).contains(&candidate.kind);
        if in_range && kind_ok {
            Some(index)
        } else {
            None
        }
    })
}

/// One staged move-animation slot for [`replace_move_anim`]: the table id
/// plus the replacement name (mafia-re 113b5b6 game/actors/C_human.h:696:
/// the `anims` vector of `S_anim_info` at 0x7c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveAnimSlot {
    pub id: i32,
    pub name: String,
}

/// GAME 0x57dd00 (RVA 0x17dd00) C_human::ChangeAnim (mafia-re 113b5b6
/// game/actors/C_human.h:565, game/actors/C_human.cpp:5509-5537): look the
/// table id up by case-insensitive name (ASCII, like `_strcmpi`), swap-remove
/// the staged slot carrying that id, stage the replacement name under the
/// same id, and zero the current animation id. The global animation-info
/// table arrives as `table`; the animation-cache/model owners stay DEFERRED.
pub fn replace_move_anim(
    table: &[(i32, &str)],
    slots: &mut Vec<MoveAnimSlot>,
    old_name: &str,
    new_name: Option<&str>,
    anim_id: &mut i32,
) {
    if let Some((id, _)) = table
        .iter()
        .find(|(_, name)| name.eq_ignore_ascii_case(old_name))
    {
        let id = *id;
        if let Some(position) = slots.iter().position(|slot| slot.id == id) {
            slots.swap_remove(position);
        }
        if let Some(new_name) = new_name {
            slots.push(MoveAnimSlot {
                id,
                name: new_name.to_string(),
            });
        }
    }
    *anim_id = 0;
}

/// Guard verdict of the C_human::RepairWeaponPosition prefix (mafia-re 113b5b6
/// game/actors/C_human.h:573).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RepairWeaponGate {
    LoadingShells,
    FireBusy,
    Settled,
    Proceed,
}

/// GAME 0x57e980 (RVA 0x17e980) C_human::RepairWeaponPosition guard chain
/// (mafia-re 113b5b6 game/actors/C_human.cpp:5719-5731): while shells load,
/// while the fire table runs past its end off the ready/aimed/on-off tables,
/// or while the weapon sits undisplaced, the leaf returns before touching the
/// model. The caller stages `fire_settled` for the table gate; the surviving
/// arm clears the displaced latch and continues into the rifle/smg/hand/frame
/// owners, which stay DEFERRED. This leaf only reports the selected arm.
pub fn repair_weapon_position_gate(
    loading_shells: bool,
    fire_settled: bool,
    weapon_displaced: bool,
) -> RepairWeaponGate {
    if loading_shells {
        RepairWeaponGate::LoadingShells
    } else if !fire_settled {
        RepairWeaponGate::FireBusy
    } else if !weapon_displaced {
        RepairWeaponGate::Settled
    } else {
        RepairWeaponGate::Proceed
    }
}

/// Selected arm of the C_human::Do_WeaponDrop carry gate (mafia-re 113b5b6
/// game/actors/C_human.cpp:7583-7592).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeaponDropCarry {
    CarryBoxDrop,
    CarryDeadBodyDrop,
    Inventory,
}

/// Selected arm of the C_human::Do_FuckingBoxDrop cancel gate (mafia-re
/// 113b5b6 game/actors/C_human.cpp:9245-9250).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoxDropNext {
    NoBox,
    Cancelled,
    StageSceneDrop,
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
/// Original idle-breath animation id, second variant (mafia-re 113b5b6
/// game/actors/C_actor.h:549: 0x2, "breath01b.i3d"). Read by the Do_Jump
/// stance-arm select of this slice; the animation owner stays DEFERRED.
pub const ANIM_BREATH01B: i32 = 2;
/// Original idle-breath animation id, third variant (mafia-re 113b5b6
/// game/actors/C_actor.h:550: 0x3, "breath01c.i3d"). Read by the Do_Jump
/// stance-arm select of this slice; the animation owner stays DEFERRED.
pub const ANIM_BREATH01C: i32 = 3;
/// Original idle-breath animation id, fourth variant (mafia-re 113b5b6
/// game/actors/C_actor.h:551: 0x4, "breath01d.i3d"). Upper bound of the
/// Do_Jump stance-arm range; the animation owner stays DEFERRED.
pub const ANIM_BREATH01D: i32 = 4;
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
/// Original car seat-climb animation base (mafia-re 113b5b6
/// game/actors/C_actor.h:732: 0x130, "AutoPrelezLtoR.i3d"). ORed with the
/// seat parity by C_human::Do_ClimbInCarLR; the animation owner stays
/// DEFERRED.
pub const ANIM_AUTO_PRELEZ_L_TO_R: i32 = 0x130;
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
    /// Note-held fire latch (mafia-re 113b5b6 game/actors/C_human.h:739-741:
    /// 0x1ed note_fire; the field name is ours per the upstream comment).
    /// Working name, no RVA claimed: the word has no standalone FUNCTION
    /// copy and is set/cleared inline at the animation-note call sites
    /// (NOTE_UNKNOWN_1A/1B); Shooting reads it inline. The note and shot
    /// owners stay DEFERRED. Starts clear.
    note_fire: bool,
    /// Reload-under-way latch (mafia-re 113b5b6 game/actors/C_human.h:748
    /// -750: 0x1fc reloading; the name is ours). Working name, no RVA
    /// claimed: the word has no standalone FUNCTION copy and is set/cleared
    /// inline by the AI fire-step and fire-end paths. Those owners stay
    /// DEFERRED. Starts clear.
    reloading: bool,
    /// Forced-reload latch (mafia-re 113b5b6 game/actors/C_human.h:751-753:
    /// 0x1fd reload_forced; the name is ours). Working name, no RVA
    /// claimed: the word has no standalone FUNCTION copy and is set inline
    /// by Do_Reload and cleared inline at the fire end and by a shot. Those
    /// owners stay DEFERRED. Starts clear.
    reload_forced: bool,
    /// Shell-by-shell load latch (mafia-re 113b5b6
    /// game/actors/C_human.h:754-757: 0x1fe loading_shells; the name is
    /// ours). Working name, no RVA claimed: the word has no standalone
    /// FUNCTION copy and is set inline by Do_Reload/Shooting with the AI
    /// re-aim loop reading it. Those owners stay DEFERRED. Starts clear.
    loading_shells: bool,
    /// Displaced-weapon latch (mafia-re 113b5b6 game/actors/C_human.h:800
    /// -803: 0x580 weapon_displaced; the name is ours). Working name, no
    /// RVA claimed: the word has no standalone FUNCTION copy and is set
    /// inline by the reload notes with RepairWeaponPosition reading it. The
    /// weapon-frame owners stay DEFERRED. Starts clear (weapon seated).
    weapon_displaced: bool,
    /// Reload-effects latch (mafia-re 113b5b6 game/actors/C_human.h:905-908:
    /// 0x581 reload_effects; the name is ours). Working name, no RVA
    /// claimed: the word has no standalone FUNCTION copy and is set inline
    /// by Do_Reload/Shooting with the reload-effect notes reading it. The
    /// effect owners stay DEFERRED. Starts clear.
    reload_effects: bool,
    /// Sniper-mode latch (mafia-re 113b5b6 game/actors/C_human.h:784-786:
    /// 0x22d sniper_mode; the name is ours). Working name, no RVA claimed:
    /// the word has no standalone FUNCTION copy and is written inline by
    /// C_player::SetSniperMode with Shooting reading it. Those owners stay
    /// DEFERRED. Starts clear. Distinct from the C_player set-alpha/
    /// indicator stubs, which stage no human word.
    sniper_mode: bool,
    /// Ground-retest latch (mafia-re 113b5b6 game/actors/C_human.h:787-790:
    /// 0x22e test_ground, PS2 0x3ce; the name is ours). Working name, no
    /// RVA claimed: the word has no standalone FUNCTION copy and is
    /// set/cleared inline by the MovementIntern ground probes with Update
    /// and GameInit writing it. Those owners stay DEFERRED. Starts clear.
    test_ground: bool,
    /// Drowned latch (mafia-re 113b5b6 game/actors/C_human.h:791-793: 0x22f
    /// drowned, PS2 0x3cf; the name is ours). Working name, no RVA claimed:
    /// the word has no standalone FUNCTION copy and is set inline by the
    /// MovementIntern water-death path with GameInit clearing it. Those
    /// owners stay DEFERRED. Starts clear.
    drowned: bool,
    /// Bat-backswing latch (mafia-re 113b5b6 game/actors/C_human.h:773-775:
    /// 0x224 backswing; the name is ours). Working name, no RVA claimed:
    /// the word has no standalone FUNCTION copy and is set inline by
    /// Do_Shoot past the 200 ms hold with the strike path reading it. Those
    /// owners stay DEFERRED. Starts clear.
    backswing: bool,
    /// Strike-series index (mafia-re 113b5b6 game/actors/C_human.h:777-779:
    /// 0x228 strike_index, 0 to 3 with -1 past the series; the name is
    /// ours). Working name, no RVA claimed: the word has no standalone
    /// FUNCTION copy and is written inline by the Do_Shoot strike series
    /// with the AI damage-doubling read. Those owners stay DEFERRED. Starts
    /// at 0 (working deterministic default; the native constructor leaves
    /// the word untouched).
    strike_index: i32,
    /// Look tilt (mafia-re 113b5b6 game/actors/C_human.h:801-803: 0x238
    /// tilt, PS2 0x3d8; the name is ours, after the camera's). Working name,
    /// no RVA claimed: the word has no standalone FUNCTION copy and is moved
    /// inline by the player walk/AI paths with the camera tilt read. Those
    /// owners stay DEFERRED. Starts at 0.0.
    tilt: f32,
    /// Car speed-limiter latch (mafia-re 113b5b6
    /// game/actors/C_human.h:1077-1078: 0xa78 speed_limiter; the name is
    /// ours). Working name, no RVA claimed: the word has no standalone
    /// FUNCTION copy and selects the 16/1000 limit inline at the seat path.
    /// The car owner stays DEFERRED. Starts clear (no limit).
    speed_limiter: bool,
    /// Score-bonus latch (mafia-re 113b5b6 game/actors/C_human.h:1080-1081:
    /// 0xa79 score_bonus; the name is a guess per the upstream comment).
    /// Working name, no RVA claimed: the word has no standalone FUNCTION
    /// copy and gates the +500 player-kill score inline at the Hit/HitInCar
    /// death paths. Those owners stay DEFERRED. Starts clear.
    score_bonus: bool,
    /// Strike-animation time left (mafia-re 113b5b6
    /// game/actors/C_human.h:798-800: 0x234 strike_anim_time; the name is
    /// ours). Working name, no RVA claimed: the word has no standalone
    /// FUNCTION copy and is written inline by SetFreeBoxingAnim
    /// (game/actors/C_human.cpp:7269) with Update counting it down while it
    /// stays at or above zero (:2345-2346). Those owners stay DEFERRED.
    /// Starts at -1 (none), matching the GameInit clear (:3238).
    strike_anim_time: i32,
    /// Agony-animation selector (mafia-re 113b5b6 game/actors/C_human.h:863-
    /// 865: 0x458 agony_anim, 0 to 12 by 4, -1 for none; the name is ours).
    /// Working name, no RVA claimed: the word has no standalone FUNCTION
    /// copy and is written inline by HitAgony (game/actors/C_human.cpp:4099-
    /// 4105) with Hit reading it back (:3747). Those owners stay DEFERRED.
    /// Starts at -1 (none), matching the GameInit clear (:3194).
    agony_anim: i32,
    /// Agonies remaining after this one (mafia-re 113b5b6
    /// game/actors/C_human.h:866-868: 0x45c agony_count; the name is ours).
    /// Working name, no RVA claimed: the word has no standalone FUNCTION
    /// copy; HitAgony writes 5 to 12 (game/actors/C_human.cpp:4099) and
    /// Movement plays another while it counts down. Those owners stay
    /// DEFERRED. Starts at 0 (working deterministic default; the native
    /// constructor leaves the word untouched).
    agony_count: i32,
    /// Jump-fall latch (mafia-re 113b5b6 game/actors/C_human.h:835-837: 0x41c
    /// jump_fall; the name is ours). Working name, no RVA claimed: the word
    /// has no standalone FUNCTION copy; MovementIntern writes whether the
    /// fall started in a jump (:5261) and the landing-damage gate reads it
    /// (:5193-5194). Those owners stay DEFERRED. Starts clear.
    jump_fall: bool,
    /// Vehicle-use selector (mafia-re 113b5b6 game/actors/C_human.h:827-829:
    /// 0x410 vehicle_action, HUMAN_VEHICLE_*; the name is ours). Working
    /// name, no RVA claimed: the word has no standalone FUNCTION copy;
    /// Update and the car-function writers set it with Update dispatching on
    /// it (game/actors/C_human.cpp:2443, :3793). Those owners stay DEFERRED.
    /// Starts at 0 (nothing), matching the GameInit clear (:3177).
    vehicle_action: i32,
    /// Shot-death latch (mafia-re 113b5b6 game/actors/C_human.h:861: 0x454
    /// killed_by_shot; the name is ours). Working name, no RVA claimed: the
    /// word has no standalone FUNCTION copy; Hit sets it for a HIT_SHOT
    /// death (game/actors/C_human.cpp:3987-3988) and Movement reads it for
    /// the death-camera path (:4840). Those owners stay DEFERRED. Starts
    /// clear, matching the GameInit/Activate clears (:3223, :9113).
    killed_by_shot: bool,
    /// Rail-wagon selector (mafia-re 113b5b6 game/actors/C_human.h:909-913:
    /// 0x584 wagon, from 1, 0 for the vehicle itself; the name is ours).
    /// Working name, no RVA claimed: the word has no standalone FUNCTION
    /// copy; Intern_UseRailway reads it to pick the wagon frame
    /// (game/actors/C_human.cpp:2166, :2172) while the Use_Actor rail writer
    /// stays DEFERRED. The savegame persists it (:1471, :1565). Starts at 0
    /// (the vehicle itself).
    wagon: i32,
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
            note_fire: false,
            reloading: false,
            reload_forced: false,
            loading_shells: false,
            weapon_displaced: false,
            reload_effects: false,
            sniper_mode: false,
            test_ground: false,
            drowned: false,
            backswing: false,
            strike_index: 0,
            tilt: 0.0,
            speed_limiter: false,
            score_bonus: false,
            strike_anim_time: -1,
            agony_anim: -1,
            agony_count: 0,
            jump_fall: false,
            vehicle_action: 0,
            killed_by_shot: false,
            wagon: 0,
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
    /// Working-name reader for the 0x1ed note-fire latch staged by
    /// set_note_fire (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn note_fire(&self) -> bool {
        self.note_fire
    }
    /// Working-name writer for the 0x1ed note-fire latch (NOTE_UNKNOWN_1A
    /// sets it with the held shot, NOTE_UNKNOWN_1B clears both; Shooting
    /// reads it; those owners stay DEFERRED).
    pub fn set_note_fire(&mut self, held: bool) {
        self.note_fire = held;
    }
    /// Working-name reader for the 0x1fc reload-under-way latch staged by
    /// set_reloading (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn reloading(&self) -> bool {
        self.reloading
    }
    /// Working-name writer for the 0x1fc reload-under-way latch (AI sets it
    /// at the fire's FIRE_ACTION_RELOAD step and clears it at the fire end;
    /// those owners stay DEFERRED).
    pub fn set_reloading(&mut self, reloading: bool) {
        self.reloading = reloading;
    }
    /// Working-name reader for the 0x1fd forced-reload latch staged by
    /// set_reload_forced (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn reload_forced(&self) -> bool {
        self.reload_forced
    }
    /// Working-name writer for the 0x1fd forced-reload latch (Do_Reload asks
    /// for it; cleared at the fire end and by a shot; those owners stay
    /// DEFERRED).
    pub fn set_reload_forced(&mut self, forced: bool) {
        self.reload_forced = forced;
    }
    /// Working-name reader for the 0x1fe shell-by-shell latch staged by
    /// set_loading_shells (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn loading_shells(&self) -> bool {
        self.loading_shells
    }
    /// Working-name writer for the 0x1fe shell-by-shell latch (Do_Reload and
    /// Shooting set it; a shot or a started strike ends it; the AI re-aim
    /// loop reads it; those owners stay DEFERRED).
    pub fn set_loading_shells(&mut self, loading: bool) {
        self.loading_shells = loading;
    }
    /// Working-name reader for the 0x580 displaced-weapon latch staged by
    /// set_weapon_displaced (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn weapon_displaced(&self) -> bool {
        self.weapon_displaced
    }
    /// Working-name writer for the 0x580 displaced-weapon latch (reload
    /// notes take the weapon out; RepairWeaponPosition puts it back; those
    /// owners stay DEFERRED).
    pub fn set_weapon_displaced(&mut self, displaced: bool) {
        self.weapon_displaced = displaced;
    }
    /// Working-name reader for the 0x581 reload-effects latch staged by
    /// set_reload_effects (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn reload_effects(&self) -> bool {
        self.reload_effects
    }
    /// Working-name writer for the 0x581 reload-effects latch (Do_Reload
    /// sets it; the fire end and ResetWeaponEffects clear it; the weapon
    /// notes read it; those owners stay DEFERRED).
    pub fn set_reload_effects(&mut self, effects: bool) {
        self.reload_effects = effects;
    }
    /// Working-name reader for the 0x22d sniper-mode latch staged by
    /// set_sniper_mode (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn sniper_mode(&self) -> bool {
        self.sniper_mode
    }
    /// Working-name writer for the 0x22d sniper-mode latch
    /// (C_player::SetSniperMode writes it; Shooting reads it; those owners
    /// stay DEFERRED). Distinct from the C_player set-alpha/indicator
    /// stubs, which stage no human word.
    pub fn set_sniper_mode(&mut self, on: bool) {
        self.sniper_mode = on;
    }
    /// Working-name reader for the 0x22e ground-retest latch staged by
    /// set_test_ground (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn test_ground(&self) -> bool {
        self.test_ground
    }
    /// Working-name writer for the 0x22e ground-retest latch (MovementIntern
    /// probes write it; Update/GameInit write it; those owners stay
    /// DEFERRED).
    pub fn set_test_ground(&mut self, test: bool) {
        self.test_ground = test;
    }
    /// Working-name reader for the 0x22f drowned latch staged by
    /// set_drowned (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn drowned(&self) -> bool {
        self.drowned
    }
    /// Working-name writer for the 0x22f drowned latch (MovementIntern sets
    /// it on the MAT_WATER_DEATH path; GameInit clears it; those owners
    /// stay DEFERRED).
    pub fn set_drowned(&mut self, drowned: bool) {
        self.drowned = drowned;
    }
    /// Working-name reader for the 0x224 bat-backswing latch staged by
    /// set_backswing (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn backswing(&self) -> bool {
        self.backswing
    }
    /// Working-name writer for the 0x224 bat-backswing latch (Do_Shoot sets
    /// it past the 200 ms hold; the strike path reads it; those owners stay
    /// DEFERRED).
    pub fn set_backswing(&mut self, backswing: bool) {
        self.backswing = backswing;
    }
    /// Working-name reader for the 0x228 strike-series index staged by
    /// set_strike_index (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn strike_index(&self) -> i32 {
        self.strike_index
    }
    /// Working-name writer for the 0x228 strike-series index (0 to 3, -1
    /// past the series; the AI damage-doubling read stays DEFERRED).
    pub fn set_strike_index(&mut self, index: i32) {
        self.strike_index = index;
    }
    /// Working-name reader for the 0x238 look-tilt word staged by set_tilt
    /// (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn tilt(&self) -> f32 {
        self.tilt
    }
    /// Working-name writer for the 0x238 look-tilt word (player walk/AI
    /// writers and the camera tilt read stay DEFERRED).
    pub fn set_tilt(&mut self, tilt: f32) {
        self.tilt = tilt;
    }
    /// Working-name reader for the 0xa78 speed-limiter latch staged by
    /// set_speed_limiter (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn speed_limiter(&self) -> bool {
        self.speed_limiter
    }
    /// Working-name writer for the 0xa78 speed-limiter latch (the seat path
    /// selects the 16/1000 car limit through it; the car owner stays
    /// DEFERRED).
    pub fn set_speed_limiter(&mut self, limit: bool) {
        self.speed_limiter = limit;
    }
    /// Working-name reader for the 0xa79 score-bonus latch staged by
    /// set_score_bonus (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn score_bonus(&self) -> bool {
        self.score_bonus
    }
    /// Working-name writer for the 0xa79 score-bonus latch (the Hit/HitInCar
    /// +500 player-kill score gate reads it; those owners stay DEFERRED).
    pub fn set_score_bonus(&mut self, bonus: bool) {
        self.score_bonus = bonus;
    }
    /// Working-name reader for the 0x234 strike-animation time staged by
    /// set_strike_anim_time (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn strike_anim_time(&self) -> i32 {
        self.strike_anim_time
    }
    /// Working-name writer for the 0x234 strike-animation time (the
    /// SetFreeBoxingAnim end-time write and the Update countdown read it;
    /// those owners stay DEFERRED).
    pub fn set_strike_anim_time(&mut self, time: i32) {
        self.strike_anim_time = time;
    }
    /// Working-name reader for the 0x458 agony-animation selector staged by
    /// set_agony_anim (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn agony_anim(&self) -> i32 {
        self.agony_anim
    }
    /// Working-name writer for the 0x458 agony-animation selector (the
    /// HitAgony write and the Hit read-back stay DEFERRED).
    pub fn set_agony_anim(&mut self, anim: i32) {
        self.agony_anim = anim;
    }
    /// Working-name reader for the 0x45c agony counter staged by
    /// set_agony_count (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn agony_count(&self) -> i32 {
        self.agony_count
    }
    /// Working-name writer for the 0x45c agony counter (the HitAgony write
    /// and the Movement countdown read stay DEFERRED).
    pub fn set_agony_count(&mut self, count: i32) {
        self.agony_count = count;
    }
    /// Working-name reader for the 0x41c jump-fall latch staged by
    /// set_jump_fall (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn jump_fall(&self) -> bool {
        self.jump_fall
    }
    /// Working-name writer for the 0x41c jump-fall latch (the MovementIntern
    /// write and the landing-damage read stay DEFERRED).
    pub fn set_jump_fall(&mut self, jump_fall: bool) {
        self.jump_fall = jump_fall;
    }
    /// Working-name reader for the 0x410 vehicle-use selector staged by
    /// set_vehicle_action (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn vehicle_action(&self) -> i32 {
        self.vehicle_action
    }
    /// Working-name writer for the 0x410 vehicle-use selector (the Update
    /// and car-function writers plus the Update dispatch read stay
    /// DEFERRED).
    pub fn set_vehicle_action(&mut self, action: i32) {
        self.vehicle_action = action;
    }
    /// Working-name reader for the 0x454 shot-death latch staged by
    /// set_killed_by_shot (no standalone FUNCTION copy, so no RVA is
    /// claimed).
    pub fn killed_by_shot(&self) -> bool {
        self.killed_by_shot
    }
    /// Working-name writer for the 0x454 shot-death latch (the Hit write and
    /// the Movement death-camera read stay DEFERRED).
    pub fn set_killed_by_shot(&mut self, killed: bool) {
        self.killed_by_shot = killed;
    }
    /// Working-name reader for the 0x584 rail-wagon selector staged by
    /// set_wagon (no standalone FUNCTION copy, so no RVA is claimed).
    pub fn wagon(&self) -> i32 {
        self.wagon
    }
    /// Working-name writer for the 0x584 rail-wagon selector (the Use_Actor
    /// rail writer stays DEFERRED; the savegame persistence is staged by the
    /// caller-supplied word).
    pub fn set_wagon(&mut self, wagon: i32) {
        self.wagon = wagon;
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
    /// the GAME 0x57dfc0 select itself is a frozen-ledger port, not owned
    /// here; `CarInitAnimationCatalog` stages the catalog side), `Do_Aimed`/`PoseNullRotate`/
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
    /// Working-name seat default of C_human::Can_DropOutFromCar (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6328-6329: `if (s < 0) s = seat`; no
    /// standalone FUNCTION, the default is inlined, so no RVA claimed).
    /// Resolves the caller's -1 (own seat) to the staged seat latch.
    pub fn can_drop_seat_default(requested: i32, seat: i32) -> i32 {
        if requested < 0 {
            seat
        } else {
            requested
        }
    }
    /// Working-name strength gate of C_human::Can_SpecialStroke (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:7199:
    /// `properties[STRENGTH] * 10.0 < human->properties[ENERGY]`; no
    /// standalone FUNCTION, the comparison is inlined, so no RVA claimed).
    /// True when the victim's energy is above ten times the attacker's
    /// strength.
    pub fn special_stroke_strength_gate(strength: f32, energy: f32) -> bool {
        strength * 10.0 < energy
    }
    /// Working-name range gate of C_human::Can_SpecialStroke (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:7203: `d.Square() < 1.0`; no
    /// standalone FUNCTION, the comparison is inlined, so no RVA claimed).
    /// True when the squared distance is within one metre.
    pub fn special_stroke_in_range(squared_distance: f32) -> bool {
        squared_distance < 1.0
    }
    /// Working-name facing gates of C_human::Can_SpecialStroke (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:7203-7204:
    /// `d.AngleTo(frame_dir) < 0.3 && d.AngleTo(human_dir) > 2.85`; no
    /// standalone FUNCTION, the comparisons are inlined, so no RVA
    /// claimed). True when the target is ahead of this human and turned
    /// away from them.
    pub fn special_stroke_angles_ok(front_angle: f32, back_angle: f32) -> bool {
        front_angle < 0.3 && back_angle > 2.85
    }
    /// Working-name seat-side sign of the Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar offsets (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6292-6293, :6336-6337, :6380-6381:
    /// `if (!b1) off.x *= -1.0`; no standalone FUNCTION, the sign is
    /// inlined, so no RVA claimed). Returns +1 for the flag side and -1
    /// against it; the caller scales the 2.5/1.2/1.3 m base offset.
    pub fn drop_side_sign(seat_flag: bool) -> f32 {
        if seat_flag {
            1.0
        } else {
            -1.0
        }
    }
    /// Working-name ground-height staging of the Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar probes (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6303-6307, :6350-6354, :6395-6399:
    /// `pos.y = hit.y + 0.7` with ground, else `pos.y += 1.0`; no
    /// standalone FUNCTION, the assignment is inlined and the TestLineV
    /// probe stays with the collision owner, so no RVA claimed). Stages the
    /// caller's probe verdict into the adjusted height.
    pub fn drop_ground_adjust(has_ground: bool, hit_y: f32, pos_y: f32) -> f32 {
        if has_ground {
            hit_y + 0.7
        } else {
            pos_y + 1.0
        }
    }
    /// Working-name line-check arm of C_human::Can_EnterToCar (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6410-6417: with `test_line` the line
    /// from the exit point to a metre above the human must not cross the
    /// car (`TestColHierarchy`); no standalone FUNCTION, the arm is
    /// inlined and the hierarchy probe stays with the scene owner, so no
    /// RVA claimed). True when the staged probe reports a crossing while
    /// the check is requested (the caller refuses).
    pub fn enter_test_line_blocks(test_line: bool, crosses_car: bool) -> bool {
        test_line && crosses_car
    }
    /// Working-name null-car guard of C_human::Can_DropOutFromCar_Free
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:6286-6287: `if (!c)
    /// return false`; no standalone FUNCTION, the guard is inlined, so no
    /// RVA claimed). True only with a staged car (admission); the car and
    /// seat-property owners stay DEFERRED.
    pub fn can_drop_free_null_guard(car_present: bool) -> bool {
        car_present
    }
    /// Working-name side offset of C_human::Can_DropOutFromCar_Free (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6291-6293: `S_vector off(2.5f, 0, 0)`
    /// with `if (!b1) off.x *= -1.0`; no standalone FUNCTION, the vector is
    /// inlined, so no RVA claimed). The car matrix/frame owners stay DEFERRED;
    /// the caller rotates the staged offset.
    pub fn drop_free_side_offset(seat_flag: bool) -> [f32; 3] {
        [2.5 * Self::drop_side_sign(seat_flag), 0.0, 0.0]
    }
    /// Working-name side offset of C_human::Can_DropOutFromCar (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6336-6338: `S_vector off(1.2f, 0, 0)`
    /// with `if (!b1) off.x *= -1.0`; no standalone FUNCTION, the vector is
    /// inlined, so no RVA claimed). The frame/collision owners stay DEFERRED;
    /// the caller stages the frame-relative probe from this offset.
    pub fn drop_side_offset(seat_flag: bool) -> [f32; 3] {
        [1.2 * Self::drop_side_sign(seat_flag), 0.0, 0.0]
    }
    /// Working-name side offset of C_human::Can_EnterToCar (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6380-6382: `S_vector off(1.3f, 0, 0)` with
    /// `if (!b1) off.x *= -1.0`; no standalone FUNCTION, the vector is
    /// inlined, so no RVA claimed). The car matrix/frame owners stay DEFERRED;
    /// the caller rotates the staged offset.
    pub fn enter_side_offset(seat_flag: bool) -> [f32; 3] {
        [1.3 * Self::drop_side_sign(seat_flag), 0.0, 0.0]
    }
    /// Working-name GetSeatProperty gate shared by Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6289, :6334, :6378:
    /// `if (!c->GetSeatProperty(...)) return false`; no standalone FUNCTION,
    /// the gate is inlined and the car seat-property owner stays DEFERRED, so
    /// no RVA claimed). True when the staged seat lookup succeeds.
    pub fn can_seat_property_admits(has_property: bool) -> bool {
        has_property
    }
    /// Working-name room-probe conjunction of Can_DropOutFromCar_Free and
    /// Can_DropOutFromCar (mafia-re 113b5b6 game/actors/C_human.cpp:6308-6316,
    /// :6355-6363: two `TestSphereB` sweeps must both miss; no standalone
    /// FUNCTION, the conjunction is inlined and both probes stay with the
    /// collision owner, so no RVA claimed). True when neither staged sweep
    /// reports a block.
    pub fn can_drop_room_clears(first_blocked: bool, second_blocked: bool) -> bool {
        !first_blocked && !second_blocked
    }
    /// Working-name room-probe conjunction of C_human::Can_EnterToCar
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:6400-6408: `TestLineH` then
    /// `TestSphereB` must both miss; no standalone FUNCTION, the conjunction
    /// is inlined and both probes stay with the collision owner, so no RVA
    /// claimed). True when neither staged probe reports a block.
    pub fn enter_room_clears(line_blocked: bool, sphere_blocked: bool) -> bool {
        !line_blocked && !sphere_blocked
    }
    /// Working-name test-line vector of C_human::Can_EnterToCar (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6412-6414: `hit = frame->GetWorldPos()
    /// + S_vector(0, 1, 0) - pos`; no standalone FUNCTION, the arithmetic is
    /// inlined and the frame/hierarchy owners stay DEFERRED, so no RVA
    /// claimed). The caller stages the frame world position and the exit
    /// position (`pos + off` after the ground adjust).
    pub fn enter_test_line_vector(frame_world: [f32; 3], exit_pos: [f32; 3]) -> [f32; 3] {
        [
            frame_world[0] - exit_pos[0],
            frame_world[1] + 1.0 - exit_pos[1],
            frame_world[2] - exit_pos[2],
        ]
    }
    /// Working-name ground-probe legs shared by Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6303, :6350, :6395:
    /// `pos + off + S_vector(0.0f, 1.0f, 0.0f)` probed down
    /// `S_vector(0.0f, -2.0f, 0.0f)`; no standalone FUNCTION, the vectors
    /// are inlined and the TestLineV probe stays with the collision owner,
    /// so no RVA claimed). Stages the (lift, fall) pair the caller probes.
    pub fn drop_line_probe_legs() -> ([f32; 3], [f32; 3]) {
        ([0.0, 1.0, 0.0], [0.0, -2.0, 0.0])
    }
    /// Working-name sweep radius of the Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar room probes (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6308, :6355, :6404: `.3f`; no standalone
    /// FUNCTION, the literal is inlined and the TestSphereB probes stay with
    /// the collision owner, so no RVA claimed).
    pub fn drop_sweep_radius() -> f32 {
        0.3
    }
    /// Working-name exit-point shift of the Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar end probes (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6312, :6359, :6404: `pos + off`; no
    /// standalone FUNCTION, the addition is inlined, so no RVA claimed).
    /// The caller stages the ground-adjusted `pos` and the side `off`.
    pub fn drop_exit_point(pos: [f32; 3], off: [f32; 3]) -> [f32; 3] {
        [pos[0] + off[0], pos[1] + off[1], pos[2] + off[2]]
    }
    /// Working-name end-probe rise of Can_DropOutFromCar_Free and
    /// Can_DropOutFromCar (mafia-re 113b5b6 game/actors/C_human.cpp:6312,
    /// :6359: `S_vector(0.0f, .3f, 0.0f)`; no standalone FUNCTION, the
    /// vector is inlined and the TestSphereB probe stays with the collision
    /// owner, so no RVA claimed). The caller sweeps the exit point shifted
    /// by this rise.
    pub fn drop_end_probe_up() -> [f32; 3] {
        [0.0, 0.3, 0.0]
    }
    /// Working-name end-probe rise of C_human::Can_EnterToCar (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6404:
    /// `S_vector(0.0f, .5f, 0.0f)`; no standalone FUNCTION, the vector is
    /// inlined and the TestSphereB probe stays with the collision owner, so
    /// no RVA claimed). The caller sweeps the exit point shifted by this
    /// rise.
    pub fn enter_end_probe_up() -> [f32; 3] {
        [0.0, 0.5, 0.0]
    }
    /// Working-name head-height staging of C_human::Can_Aimed (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:8914: `pos.y += 1.5f`; no standalone
    /// FUNCTION, the lift is inlined and the frame read plus the TestLineH
    /// probe stay with their owners, so no RVA claimed). The caller lifts
    /// the staged head position by this offset before the wall probe.
    pub fn can_aimed_probe_offset() -> [f32; 3] {
        [0.0, 1.5, 0.0]
    }
    /// Working-name exit-offset flattening of the Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar offsets (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6296, :6341, :6386: `off.y = 0.0f` after the
    /// car/frame rotation; no standalone FUNCTION, the assignment is inlined
    /// and the matrix rotation stays with the frame owner, so no RVA
    /// claimed). The caller stages the rotated offset.
    pub fn drop_offset_flatten_y(off: [f32; 3]) -> [f32; 3] {
        [off[0], 0.0, off[2]]
    }
    /// Working-name exclusion-list order of the Can_DropOutFromCar_Free,
    /// Can_DropOutFromCar and Can_EnterToCar probes (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6298-6300, :6343-6346, :6389-6392: the
    /// human's own collisions reversed, then the car's dynamic collision;
    /// Free admits only the car; no standalone FUNCTION, the list build is
    /// inlined and the DynSetNextCollOwnerMult call stays with the collision
    /// owner, so no RVA claimed). An empty `own` stages the Free shape. The
    /// caller owns identity storage; only the order is staged here.
    pub fn drop_probe_owner_order(
        own: &[HumanDynCollId],
        car: HumanDynCollId,
    ) -> Vec<HumanDynCollId> {
        let mut order = Vec::with_capacity(own.len() + 1);
        for coll in own.iter().rev() {
            order.push(*coll);
        }
        order.push(car);
        order
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
    /// GAME 0x56b180 (RVA 0x16b180) C_human::AddToCutscene (mafia-re 113b5b6
    /// game/actors/C_human.cpp:42-48): the human files three record kinds
    /// (0x28, 0x2c and 0x38 bytes) and reports true. The recorder call
    /// (`cutscene.AddActor`) stays with the cutscene owner (DEFERRED); this
    /// leaf only stages the three size words plus the constant verdict, in
    /// (sizes, verdict) order.
    pub fn add_to_cutscene(&self) -> ([u32; 3], bool) {
        ([0x28, 0x2c, 0x38], true)
    }
    /// Static C_human::HideLegs no-op (mafia-re 113b5b6
    /// game/actors/C_human.h:672-673; working name, no standalone FUNCTION
    /// and no RVA claimed: hides or shows a driver's legs, but empty on the
    /// PC, so neither the model nor the flag is touched). The model owner is
    /// irrelevant here: there is no body to run.
    pub fn hide_legs_empty(_hide: bool) {}
    /// C_human::NetDirect empty leaf (working name, no RVA claimed:
    /// mafia-re 113b5b6 game/actors/C_human.h:509-510 keeps no out-of-line
    /// body; its calls go to the empty function the linker folded it into,
    /// 0x4ab130, whose RVA 0xab130 the frozen ledger already retains for
    /// another port, so no address is claimed here). The net owner stays
    /// DEFERRED; invoking it stages nothing.
    pub fn net_direct_empty() {}
    /// C_human::UniversalUse empty leaf (working name, no RVA claimed:
    /// mafia-re 113b5b6 game/actors/C_human.h:594-595 and
    /// game/actors/C_human.cpp:10078-10081 keep no out-of-line copy; empty on
    /// the PC, its calls go to the empty function the linker folded it into,
    /// 0x4aabe0, whose RVA 0xaabe0 the frozen ledger already retains, so no
    /// address is claimed here). No use program runs; invoking it stages
    /// nothing.
    pub fn universal_use_empty() {}
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
    /// GAME 0x570570 (RVA 0x170570) C_human::Death EXPLICIT UNSUPPORTED STUB
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:1671-1697): the body zeroes
    /// the velocities and the ramp, drops the shadow, null-rotates the pose,
    /// deactivates the collisions, clears the extended animation, removes the
    /// human from the police, switches the player camera to death, drops the
    /// base-frame lightness, unlocks the locked car/vehicle and latches dead.
    /// The velocity words stay with the movement owner and every other effect
    /// with the shadow/collision/animation/police/camera/lightness/lock
    /// owners, which do not exist here, so no state is touched and no owner
    /// is filed; the stub exists only to pin the address while those owners
    /// stay DEFERRED.
    pub fn death_unsupported() {}
    /// GAME 0x570660 (RVA 0x170660) C_human::Extern_AI_Update EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1699-1706):
    /// the body ticks the animation machine while active, ticks the frame
    /// for `time` and refreshes the dynamic collisions. The animation-machine,
    /// frame and collision owners do not exist here, so no state is touched
    /// and no owner is filed; the stub exists only to pin the address while
    /// those owners stay DEFERRED.
    pub fn extern_ai_update_unsupported(_time_ms: u32) {}
    /// GAME 0x5716d0 (RVA 0x1716d0) C_human::Intern_UseCar(bool) EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:2068-2154):
    /// the flag overload that seats the human from using_car at the
    /// animation's end (or clears the car latches on the way out), with the
    /// engine/speed/lock/guard writes, the frame link/pose, the player
    /// camera/indicator/light setup, the police handoff, the leg hide and the
    /// vehicle-action clear. Distinct from the claimed 0x57e020 seat-placement
    /// overload. The car/frame/police/indicator/shadow/inventory owners do not
    /// exist here, so no state is touched and no owner is filed; the stub
    /// exists only to pin the address while those owners stay DEFERRED.
    pub fn intern_use_car_flag_unsupported(_entering: bool) {}
    /// GAME 0x571b90 (RVA 0x171b90) C_human::Intern_UseRailway EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:2159-2232):
    /// the rail-vehicle enter/exit program (car/using_car latch swap, wagon
    /// matrix-relative placement, collision get in/out, animation-matrix
    /// recompile, camera link, rain-dummy and shadow refresh). The
    /// railway/frame/matrix/collision/camera/police/shadow owners do not
    /// exist here, so no state is touched and no owner is filed; the stub
    /// exists only to pin the address while those owners stay DEFERRED.
    pub fn intern_use_railway_unsupported(_entering: bool) {}
    /// GAME 0x5721b0 (RVA 0x1721b0) C_human::_intern_UseCarTruck EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:2234-2295):
    /// the truck-back ride enter/exit program (seat -1 latch, truck-relative
    /// placement, passenger add/remove, collision get in/out, shadow and
    /// camera refresh, STAND settle). The truck/frame/collision/camera/police/
    /// navigation owners do not exist here, so no state is touched and no
    /// owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED.
    pub fn intern_use_car_truck_unsupported() {}
    /// GAME 0x572530 (RVA 0x172530) C_human::ThrowFromCarTruck EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:2300-2331):
    /// thrown off the truck's back with the hit's damage on energy: the scene
    /// re-link, the collision get-out, the shadow and camera refresh, the
    /// police handoff, the energy subtraction with the death-talk/weapon-drop
    /// path and the HitFall. The truck/frame/scene/collision/damage owners do
    /// not exist here, so no state is touched and no owner is filed; the stub
    /// exists only to pin the address while those owners stay DEFERRED.
    pub fn throw_from_car_truck_unsupported() {}
    /// GAME 0x5757c0 (RVA 0x1757c0) C_human::GameDone EXPLICIT UNSUPPORTED
    /// STUB (mafia-re 113b5b6 game/actors/C_human.cpp:3407-3476 and following
    /// lines): the teardown program (truck-collision exit, model restore, leg
    /// show, clip release, using-object removals, base C_actor::GameDone, base
    /// frame lightness drop, shadow drop, animation stop, frame release, pose
    /// releases, collision erase, sound stops). The model/frame/shadow/sound/
    /// using-object owners do not exist here, so no state is touched and no
    /// owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED.
    pub fn human_game_done_unsupported() {}
    /// GAME 0x584620 (RVA 0x184620) C_human::Shooting EXPLICIT UNSUPPORTED
    /// STUB (mafia-re 113b5b6 game/actors/C_human.cpp:7351-7470 and following
    /// lines): the shot program (fire-table end gate, weapon change, ammo
    /// check with the player reload, car-seat shot, displacement repair, timed
    /// table select, grenade cheat, throw-dir aim gate, police firing note,
    /// spread direction over range, wagon hit, shoot/light/sound effects,
    /// weapon fire-table claim). The inventory/frame/police/sound/grenade/
    /// weapon/light/force-feedback owners do not exist here, so no state is
    /// touched and no owner is filed; the stub exists only to pin the address
    /// while those owners stay DEFERRED.
    pub fn shooting_unsupported() {}
    /// GAME 0x58b8d0 (RVA 0x18b8d0) C_human::RecompileDeathPos EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:9313-9358
    /// and following lines): the death animation's last frame drives a dummy
    /// frame until neck/feet/base settle on the ground, out of walls and off
    /// edges, and the human frame takes its rotation/position over death_length
    /// (ProcessDeathPos). The model/frame/animation-cache owners (SettleOn-
    /// Ground, PushOutOfWalls, SlideOffEdge) do not exist here, so no state
    /// is touched and no owner is filed; the stub exists only to pin the
    /// address while those owners stay DEFERRED.
    pub fn recompile_death_pos_unsupported() {}
    /// GAME 0x56b8d0 (RVA 0x16b8d0) C_human::PlayStep EXPLICIT UNSUPPORTED
    /// STUB (mafia-re 113b5b6 game/actors/C_human.cpp:250-366): the cutscene
    /// step player (blended record position/rotation, next-anim-id arming
    /// with the PersonAnim/cutscene-anim select, blend stepping, model
    /// SetAnimation/SetAnimTime ticks, sector placement and end-of-records
    /// handling). The records/cutscene/model/frame owners do not exist here,
    /// so no state is touched and no owner is filed; the stub exists only to
    /// pin the address while those owners stay DEFERRED.
    pub fn play_step_unsupported(_time_ms: u32) {}
    /// GAME 0x56c6e0 (RVA 0x16c6e0) C_human::DumpRecords EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:536-559):
    /// the debug dump of the record list to "<frame name>.dump" (last key
    /// time, then each record's time, animation id and flags). The frame-name,
    /// record-list and host-file owners do not exist here, so no state is
    /// touched and no owner is filed; the stub exists only to pin the address
    /// while those owners stay DEFERRED.
    pub fn dump_records_unsupported() {}
    /// GAME 0x578ee0 (RVA 0x178ee0) C_human::PoseInterpolate EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.h:537,
    /// game/actors/C_human.cpp:4557-4622): the body eases the three staged
    /// pose frames (back1, back2, neck) toward their target directions by
    /// `time / 1000 * PI` radians (at least 0.03) and detaches them once a
    /// dead human settles. Every read and write belongs to the pose/frame
    /// owners, which do not exist here, so no state is touched and no owner
    /// is filed; the stub exists only to pin the address while those owners
    /// stay DEFERRED.
    pub fn pose_interpolate_unsupported(_time: u32) {}
    /// GAME 0x585c60 (RVA 0x185c60) C_human::Do_Holster carry guard
    /// (mafia-re 113b5b6 game/actors/C_human.h:610,
    /// game/actors/C_human.cpp:7557-7561): a carried box or dead body diverts
    /// to the box drop first. The inventory Holster/ChangeWeaponModel/
    /// DropOutItems program past the guard stays DEFERRED with the weapon
    /// owner; this leaf only reports whether the carry must be dropped first.
    pub fn holster_requires_carry_drop(&self) -> bool {
        self.carrying_box || self.carrying_dead_body
    }
    /// GAME 0x585d90 (RVA 0x185d90) C_human::Do_WeaponDrop carry gate
    /// (mafia-re 113b5b6 game/actors/C_human.h:611,
    /// game/actors/C_human.cpp:7583-7592): a carried box diverts to the box
    /// drop, a carried dead body to the dead-body drop; otherwise the
    /// inventory Remove/DropOutItems/ChangeWeaponModel program runs, whose
    /// owners stay DEFERRED. This leaf only reports the selected arm.
    pub fn weapon_drop_carry_action(&self) -> WeaponDropCarry {
        if self.carrying_box {
            WeaponDropCarry::CarryBoxDrop
        } else if self.carrying_dead_body {
            WeaponDropCarry::CarryDeadBodyDrop
        } else {
            WeaponDropCarry::Inventory
        }
    }
    /// GAME 0x58b690 (RVA 0x18b690) C_human::Do_FuckingBoxDrop cancel gate
    /// (mafia-re 113b5b6 game/actors/C_human.h:655,
    /// game/actors/C_human.cpp:9245-9250; `FuckingBox` is the upstream
    /// working-name family for the carried box, NOT original): without a box
    /// the leaf ends at once; a successful box cancel ends it too. The
    /// scene/physics owners of the remaining arm (sector link,
    /// C_drop_in_weapon placement, temporary-actor filing) and the trailing
    /// box-release call stay DEFERRED; this leaf only reports the selected
    /// arm for the staged `cancel_succeeded` verdict.
    pub fn box_drop_after_cancel(&self, cancel_succeeded: bool) -> BoxDropNext {
        if !self.carrying_box {
            BoxDropNext::NoBox
        } else if cancel_succeeded {
            BoxDropNext::Cancelled
        } else {
            BoxDropNext::StageSceneDrop
        }
    }
    /// GAME 0x5747d0 (RVA 0x1747d0, VA 0x5747d0) C_human::ShiftAnimation
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:3033): the body looks the current animation up
    /// in the global table, creates its set through the animation cache, and
    /// ticks the animation machine and the frame at each matching notify key.
    /// The animation-cache, table and frame owners do not exist here, so no
    /// state is touched and no owner is filed; the stub exists only to pin
    /// the address while those owners stay DEFERRED.
    pub fn shift_animation_unsupported(_note: i32) {}
    /// GAME 0x5748f0 (RVA 0x1748f0, VA 0x5748f0) C_human::ChangeState
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:3085): the body switches on the actor state and
    /// drives collision sets, using-object filing, shadow frames and dead-body
    /// pickup records. The collision, mission, shadow and frame owners do not
    /// exist here, so no state is touched and no owner is filed; the stub
    /// exists only to pin the address while those owners stay DEFERRED.
    pub fn change_state_unsupported() {}
    /// GAME 0x575ca0 (RVA 0x175ca0, VA 0x575ca0) C_human::CreateDynColls
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:3561): the body allocates the two dynamic
    /// collision objects and links them to the model's frames. The
    /// frame/collision owners do not exist here, so no state is touched and
    /// no owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED.
    pub fn create_dyn_colls_unsupported() {}
    /// GAME 0x575ed0 (RVA 0x175ed0, VA 0x575ed0) C_human::EraseDynColls
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:3598): the body erases the dynamic collision
    /// objects through the global collision owner and frees them. The
    /// collision owner does not exist here, so no state is touched and no
    /// owner is filed; the stub exists only to pin the address while that
    /// owner stays DEFERRED.
    pub fn erase_dyn_colls_unsupported() {}
    /// GAME 0x576040 (RVA 0x176040, VA 0x576040) C_human::UpdateDynColl
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:3667): the body re-poses the dynamic collision
    /// spheres from the model's frames while they are active. The
    /// frame/collision owners do not exist here, so no state is touched and
    /// no owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED.
    pub fn update_dyn_coll_unsupported() {}
    /// GAME 0x579630 (RVA 0x179630, VA 0x579630) C_human::PoseSetPoseNormal
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:4626): the verdict needs the neck child frame,
    /// the world-direction math and the base frame (pose flag/latch writes).
    /// The frame owner does not exist here, so no verdict is produced and no
    /// owner is filed; the stub exists only to pin the address while that
    /// owner stays DEFERRED. The native `bool` answer is preserved in the
    /// return type.
    pub fn pose_set_pose_normal_unsupported() -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: POSE_SET_POSE_NORMAL_ENTRY,
        })
    }
    /// GAME 0x579ea0 (RVA 0x179ea0, VA 0x579ea0) C_human::PoseSetPoseAimed
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:4667): same frame/matrix boundary as
    /// PoseSetPoseNormal with the narrower aimed cone (PI/3). The frame owner
    /// does not exist here, so no verdict is produced and no owner is filed;
    /// the stub exists only to pin the address while that owner stays
    /// DEFERRED. The native `bool` answer is preserved in the return type.
    pub fn pose_set_pose_aimed_unsupported() -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: POSE_SET_POSE_AIMED_ENTRY,
        })
    }
    /// GAME 0x57ec20 (RVA 0x17ec20, VA 0x57ec20) C_human::ChangeWeaponModel
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:5773): the body resets the weapon-effect words
    /// and rebuilds the hand weapon models through the model owner. The
    /// model/animation owners do not exist here, so no state is touched and
    /// no owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED.
    pub fn change_weapon_model_unsupported() {}
    /// GAME 0x57faa0 (RVA 0x17faa0, VA 0x57faa0) C_human::DropOutItems
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:6146): the entry gate (both hand frames present
    /// plus a non-empty staged list, see `drop_out_items_gate`) is pure, but
    /// the drop program creates model frames, opens them through the model
    /// cache and files physical weapon actors through the mission/scene
    /// owners. Those owners do not exist here, so no verdict is produced and
    /// no owner is filed; the stub exists only to pin the address while they
    /// stay DEFERRED. The native `bool` answer is preserved in the return
    /// type.
    pub fn drop_out_items_unsupported() -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: DROP_OUT_ITEMS_ENTRY,
        })
    }
    /// GAME 0x580ca0 (RVA 0x180ca0, VA 0x580ca0)
    /// C_human::ZasrateLezeniZAuta_ve2rano EXPLICIT UNSUPPORTED STUB (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:6427; the name is the upstream working
    /// name for the wreck-climb exit, NOT original): the verdict runs the
    /// seat-side matrix exit, the ground/room collision probes and the
    /// car/scene handoff (owner clear, shadow, indicators, camera, inventory
    /// drop, weapon remodel, climb animation). The car, collision, scene and
    /// animation owners do not exist here, so no verdict is produced and no
    /// owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED. The native `bool` answer is preserved in the
    /// return type.
    pub fn zasrate_lezeni_ve2rano_unsupported() -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: ZASRATE_LEZENI_VE2RANO_ENTRY,
        })
    }
    /// GAME 0x581220 (RVA 0x181220, VA 0x581220)
    /// C_human::ZasrateVybliknutiZAuta_ve3rano EXPLICIT UNSUPPORTED STUB
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:6519; the name is the
    /// upstream working name for the wreck-throw exit, NOT original): the body
    /// places the human beside the car's front AI point through the car
    /// matrix and runs the same car/scene handoff as the Lezeni exit. The
    /// car, collision, scene and animation owners do not exist here, so no
    /// state is touched and no owner is filed; the stub exists only to pin
    /// the address while those owners stay DEFERRED.
    pub fn zasrate_vybliknuti_ve3rano_unsupported() {}
    /// GAME 0x5878d0 (RVA 0x1878d0, VA 0x5878d0) C_human::_intern_ForceDeath
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:8061): the body picks a random death animation
    /// through the animation cache, stops and replays the model channels, and
    /// then writes the energy/alive/disabled words. The animation-cache and
    /// model owners do not exist here, so no state is touched (not even the
    /// trailing words, which the cache run owns) and no owner is filed; the
    /// stub exists only to pin the address while those owners stay DEFERRED.
    pub fn intern_force_death_unsupported() {}
    /// GAME 0x588b40 (RVA 0x188b40, VA 0x588b40)
    /// C_human::_intern_ThrowMeFromCar EXPLICIT UNSUPPORTED STUB (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:8374): the body reads the seat
    /// properties through the car, clears the driver seat and re-links the
    /// frame through the car matrix. The car, seat-property and frame owners
    /// do not exist here, so no state is touched and no owner is filed; the
    /// stub exists only to pin the address while those owners stay DEFERRED.
    pub fn intern_throw_me_from_car_unsupported() {}
    /// GAME 0x58ac90 (RVA 0x18ac90, VA 0x58ac90) C_human::Activate EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:9043): the
    /// body re-arms the human (alive/render/act-state words) through the
    /// frame SetOn and the animation-machine Init plus the property-bank
    /// copy. The frame and animation-machine owners do not exist here, so no
    /// state is touched and no owner is filed; the stub exists only to pin
    /// the address while those owners stay DEFERRED.
    pub fn activate_unsupported() {}
    /// GAME 0x58b330 (RVA 0x18b330, VA 0x58b330) C_human::Deactivate EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:9177): the
    /// body unlinks the using objects and the shadow through the mission,
    /// restores the pre-cutscene model and releases the animation machine.
    /// The mission, model and animation owners do not exist here, so no state
    /// is touched and no owner is filed; the stub exists only to pin the
    /// address while those owners stay DEFERRED.
    pub fn deactivate_unsupported() {}
    /// GAME 0x58d020 (RVA 0x18d020, VA 0x58d020) C_human::FreeHand EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:9626):
    /// the body runs the Do_Holster carry gate (owned separately as
    /// `holster_requires_carry_drop`), then harvests the hand frames into a
    /// vector and files each as a physical thing through the
    /// scene/physics/mission owners. The frame, scene and physics owners do
    /// not exist here, so no state is touched and no owner is filed; the
    /// stub exists only to pin the address while those owners stay DEFERRED.
    pub fn free_hand_unsupported() {}
    /// GAME 0x58d2c0 (RVA 0x18d2c0, VA 0x58d2c0) C_human::StartWeaponEffect
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:9673): the body arms the reload/shot effect
    /// moves (cylinder slide, clip drop, shells, left-hand clip) through the
    /// weapon-model frames, the driver visuals and the physics/mission
    /// owners. The inventory, frame, driver and physics owners do not exist
    /// here, so no state is touched (not even the leading `effect_step = 0`,
    /// which the effect program owns) and no owner is filed; the stub exists
    /// only to pin the address while those owners stay DEFERRED.
    pub fn start_weapon_effect_unsupported(_effect: i32) {}
    /// GAME 0x58da70 (RVA 0x18da70, VA 0x58da70) C_human::TickWeaponEffect
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:9872): the body advances `effect_pos` by
    /// `time * effect_speed` and poses the effect frame (cylinder/slide/
    /// foregrip) per the right-hand item. The effect-frame and inventory
    /// owners do not exist here, so no state is touched and no owner is
    /// filed; the stub exists only to pin the address while those owners
    /// stay DEFERRED.
    pub fn tick_weapon_effect_unsupported(_time_ms: u32) {}
    /// GAME 0x58a5a0 (RVA 0x18a5a0, VA 0x58a5a0)
    /// C_human::EineMeineKleineAutoInLuft EXPLICIT UNSUPPORTED STUB
    /// (mafia-re 113b5b6 game/actors/C_human.cpp:8852; the name is the
    /// upstream PS2 working name for the burning-wreck ride, NOT original):
    /// the body links the frame to the wreck, swaps the model to the burnt
    /// one, kills the human through Death and stages the death animation at
    /// its last frame. The frame, model-change, death, animation and
    /// indicator owners do not exist here, so no state is touched and no
    /// owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED.
    pub fn eine_meine_kleine_auto_in_luft_unsupported() {}
    /// GAME 0x587190 (RVA 0x187190, VA 0x587190)
    /// C_human::_intern_ChangeModel EXPLICIT UNSUPPORTED STUB (mafia-re
    /// 113b5b6 game/actors/C_human.cpp:7883): the body keeps the original
    /// model name, invalidates the actor, drops the shadow, stops the
    /// animations, erases the dynamic collisions and rebuilds poses, hand
    /// targets and shadow for the new model. The frame, model-cache,
    /// collision, shadow and mission owners do not exist here, so no state
    /// is touched and no owner is filed; the stub exists only to pin the
    /// address while those owners stay DEFERRED.
    pub fn intern_change_model_unsupported() {}
    /// GAME 0x582180 (RVA 0x182180, VA 0x582180) C_human::Use_Actor EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:6681): the
    /// body dispatches the use action over doors, turnouts, cars, railway
    /// wagons and trucks (seat checks through Can_EnterToCar and
    /// GetSeatProperty, traffic/police handoffs, climb animations). The
    /// door/turnout/car/rail actors plus the scene, police and animation
    /// owners do not exist here, so no state is touched and no owner is
    /// filed; the stub exists only to pin the address while those owners
    /// stay DEFERRED.
    pub fn use_actor_unsupported() {}
    /// GAME 0x56b1a0 (RVA 0x16b1a0, VA 0x56b1a0) C_human::RecordKey EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:54): the
    /// body decides from the record cursor, the last key time, the pose flags
    /// and the cutscene link whether a key is due at `time`, then stages the
    /// frame position/rotation into the record. The record store, frame and
    /// cutscene-link owners do not exist here, so no verdict is produced and
    /// no owner is filed; the stub exists only to pin the address while those
    /// owners stay DEFERRED. The native `int` answer is preserved in the
    /// return type.
    pub fn record_key_unsupported(_time_ms: u32) -> Result<i32, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: RECORD_KEY_ENTRY,
        })
    }
    /// GAME 0x56b670 (RVA 0x16b670, VA 0x56b670) C_human::PlayRecord EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:165): the
    /// body applies the current record at `time`, selecting or resetting the
    /// staged animation through the animation machine or a cutscene anim on
    /// the frame. The record store, animation-machine, cutscene and frame
    /// owners do not exist here, so no state is touched and no owner is
    /// filed; the stub exists only to pin the address while those owners stay
    /// DEFERRED.
    pub fn play_record_unsupported(_time_ms: u32) {}
    /// GAME 0x56bec0 (RVA 0x16bec0, VA 0x56bec0) C_human::PlaceAtTime EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:368): the
    /// body seeks the record stream to `time` and places the human from the
    /// selected record. The record store and frame owners do not exist here,
    /// so no verdict is produced and no owner is filed; the stub exists only
    /// to pin the address while those owners stay DEFERRED. The native
    /// `bool` answer is preserved in the return type.
    pub fn place_at_time_unsupported(_time_ms: u32) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: PLACE_AT_TIME_ENTRY,
        })
    }
    /// GAME 0x56c7c0 (RVA 0x16c7c0, VA 0x56c7c0) C_human::GetRecordPos
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:565): the body walks the record blocks up to
    /// `time` and interpolates the recorded position. The record store owner
    /// does not exist here, so no position is produced and no owner is filed;
    /// the stub exists only to pin the address while that owner stays
    /// DEFERRED. The native `S_vector` answer is preserved as `[f32; 3]` in
    /// the return type.
    pub fn get_record_pos_unsupported(_time_ms: u32) -> Result<[f32; 3], HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: GET_RECORD_POS_ENTRY,
        })
    }
    /// GAME 0x56cc70 (RVA 0x16cc70, VA 0x56cc70) C_human::GetRecordRot
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:668): the body walks the record blocks up to
    /// `time` and interpolates the recorded rotation. The record store owner
    /// does not exist here, so no rotation is produced and no owner is filed;
    /// the stub exists only to pin the address while that owner stays
    /// DEFERRED. The native `S_quat` answer is preserved as `[f32; 4]` in
    /// the return type.
    pub fn get_record_rot_unsupported(_time_ms: u32) -> Result<[f32; 4], HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: GET_RECORD_ROT_ENTRY,
        })
    }
    /// GAME 0x56e3b0 (RVA 0x16e3b0, VA 0x56e3b0) ai_loc::Update EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1079): the
    /// body steps the AI location. The AI owner does not exist here, so no
    /// state is touched and no owner is filed; the stub exists only to pin
    /// the address while that owner stays DEFERRED.
    pub fn ai_loc_update_unsupported(&mut self) {}
    /// GAME 0x56efd0 (RVA 0x16efd0, VA 0x56efd0) C_human::C_human EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1250): the
    /// constructor body. Construction stays with the actor owner; the stub
    /// exists only to pin the address while that owner stays DEFERRED.
    pub fn ctor_unsupported(&mut self) {}
    /// GAME 0x56f350 (RVA 0x16f350, VA 0x56f350) C_human::Init EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1284): the
    /// body inits the human on a frame. The frame owner does not exist here,
    /// so no verdict is produced and no owner is filed; the stub exists only
    /// to pin the address while that owner stays DEFERRED. The native `bool`
    /// answer is preserved in the return type.
    pub fn init_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: INIT_ENTRY,
        })
    }
    /// GAME 0x56f4e0 (RVA 0x16f4e0, VA 0x56f4e0) C_human::LoadData EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1377): the
    /// body loads the human from a chunk. The chunk owner does not exist
    /// here, so no verdict is produced and no owner is filed; the stub
    /// exists only to pin the address while that owner stays DEFERRED. The
    /// native `bool` answer is preserved in the return type.
    pub fn load_data_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: LOAD_DATA_ENTRY,
        })
    }
    /// GAME 0x56fa20 (RVA 0x16fa20, VA 0x56fa20) C_human::SaveGameSave
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:1428): the body saves the human into a buffer.
    /// The buffer owner does not exist here, so no verdict is produced and no
    /// owner is filed; the stub exists only to pin the address while that
    /// owner stays DEFERRED. The native `bool` answer is preserved in the
    /// return type.
    pub fn save_game_save_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: SAVE_GAME_SAVE_ENTRY,
        })
    }
    /// GAME 0x56fd30 (RVA 0x16fd30, VA 0x56fd30) C_human::SaveGameLoad
    /// EXPLICIT UNSUPPORTED STUB (mafia-re 113b5b6
    /// game/actors/C_human.cpp:1494): the body loads the human from a buffer.
    /// The buffer owner does not exist here, so no verdict is produced and no
    /// owner is filed; the stub exists only to pin the address while that
    /// owner stays DEFERRED. The native `bool` answer is preserved in the
    /// return type.
    pub fn save_game_load_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: SAVE_GAME_LOAD_ENTRY,
        })
    }
    /// GAME 0x570740 (RVA 0x170740, VA 0x570740) C_human::AI EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:1730): the
    /// body ticks the human AI. The AI owner does not exist here, so no state
    /// is touched and no owner is filed; the stub exists only to pin the
    /// address while that owner stays DEFERRED.
    pub fn ai_unsupported(&mut self, _time_ms: u32) {}
    /// GAME 0x572b80 (RVA 0x172b80, VA 0x572b80) C_human::Update EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:2338): the
    /// body ticks the human simulation. The simulation owners do not exist
    /// here, so no state is touched and no owner is filed; the stub exists
    /// only to pin the address while those owners stay DEFERRED.
    pub fn update_unsupported(&mut self, _time_ms: u32) {}
    /// GAME 0x573e50 (RVA 0x173e50, VA 0x573e50) C_human::PersonAnim EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:2743): the
    /// body picks a person animation. The animation owner does not exist here,
    /// so no state is touched and no owner is filed; the stub exists only to
    /// pin the address while that owner stays DEFERRED.
    pub fn person_anim_unsupported(&mut self, _f: f32, _b1: bool, _b2: bool) {}
    /// GAME 0x5762a0 (RVA 0x1762a0, VA 0x5762a0) C_human::Hit EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:3729): the
    /// body applies a hit to the human. The collision/actor owners do not
    /// exist here, so no verdict is produced and no owner is filed; the stub
    /// exists only to pin the address while those owners stay DEFERRED. The
    /// native `bool` answer is preserved in the return type.
    pub fn hit_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: HIT_ENTRY,
        })
    }
    /// GAME 0x578290 (RVA 0x178290, VA 0x578290) C_human::HitInCar EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:4358): the
    /// body applies a hit to the human in a car. The collision/actor owners
    /// do not exist here, so no verdict is produced and no owner is filed;
    /// the stub exists only to pin the address while those owners stay
    /// DEFERRED. The native `bool` answer is preserved in the return type.
    pub fn hit_in_car_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: HIT_IN_CAR_ENTRY,
        })
    }
    /// GAME 0x5784a0 (RVA 0x1784a0, VA 0x5784a0) C_human::Collision EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_human.cpp:4413): the
    /// body runs human collision. The collision/actor owners do not exist
    /// here, so no verdict is produced and no owner is filed; the stub exists
    /// only to pin the address while those owners stay DEFERRED. The native
    /// `bool` answer is preserved in the return type.
    pub fn collision_unsupported(&mut self) -> Result<bool, HumanSmallProbeError> {
        Err(HumanSmallProbeError::Unsupported {
            entry: COLLISION_ENTRY,
        })
    }
}

/// Small-probe boundary for the value-returning C_human leaves pinned above.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanSmallProbeError {
    Unsupported { entry: u32 },
}

/// `C_human::PoseSetPoseNormal` entry, GAME VA 0x579630.
pub const POSE_SET_POSE_NORMAL_ENTRY: u32 = 0x579630;
/// `C_human::PoseSetPoseAimed` entry, GAME VA 0x579ea0.
pub const POSE_SET_POSE_AIMED_ENTRY: u32 = 0x579ea0;
/// `C_human::DropOutItems` entry, GAME VA 0x57faa0.
pub const DROP_OUT_ITEMS_ENTRY: u32 = 0x57faa0;
/// `C_human::ZasrateLezeniZAuta_ve2rano` entry, GAME VA 0x580ca0.
pub const ZASRATE_LEZENI_VE2RANO_ENTRY: u32 = 0x580ca0;
/// `C_human::RecordKey` entry, GAME VA 0x56b1a0.
pub const RECORD_KEY_ENTRY: u32 = 0x56b1a0;
/// `C_human::PlayRecord` entry, GAME VA 0x56b670.
pub const PLAY_RECORD_ENTRY: u32 = 0x56b670;
/// `C_human::PlaceAtTime` entry, GAME VA 0x56bec0.
pub const PLACE_AT_TIME_ENTRY: u32 = 0x56bec0;
/// `C_human::GetRecordPos` entry, GAME VA 0x56c7c0.
pub const GET_RECORD_POS_ENTRY: u32 = 0x56c7c0;
/// `C_human::GetRecordRot` entry, GAME VA 0x56cc70.
pub const GET_RECORD_ROT_ENTRY: u32 = 0x56cc70;
/// ai_loc::Update entry, GAME VA 0x56e3b0.
pub const AI_LOC_UPDATE_ENTRY: u32 = 0x56e3b0;
/// `C_human::C_human` entry, GAME VA 0x56efd0.
pub const CTOR_ENTRY: u32 = 0x56efd0;
/// `C_human::Init` entry, GAME VA 0x56f350.
pub const INIT_ENTRY: u32 = 0x56f350;
/// `C_human::LoadData` entry, GAME VA 0x56f4e0.
pub const LOAD_DATA_ENTRY: u32 = 0x56f4e0;
/// `C_human::SaveGameSave` entry, GAME VA 0x56fa20.
pub const SAVE_GAME_SAVE_ENTRY: u32 = 0x56fa20;
/// `C_human::SaveGameLoad` entry, GAME VA 0x56fd30.
pub const SAVE_GAME_LOAD_ENTRY: u32 = 0x56fd30;
/// `C_human::AI` entry, GAME VA 0x570740.
pub const AI_ENTRY: u32 = 0x570740;
/// `C_human::Update` entry, GAME VA 0x572b80.
pub const UPDATE_ENTRY: u32 = 0x572b80;
/// `C_human::PersonAnim` entry, GAME VA 0x573e50.
pub const PERSON_ANIM_ENTRY: u32 = 0x573e50;
/// `C_human::Hit` entry, GAME VA 0x5762a0.
pub const HIT_ENTRY: u32 = 0x5762a0;
/// `C_human::HitInCar` entry, GAME VA 0x578290.
pub const HIT_IN_CAR_ENTRY: u32 = 0x578290;
/// `C_human::Collision` entry, GAME VA 0x5784a0.
pub const COLLISION_ENTRY: u32 = 0x5784a0;

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
    // Working-name 0x234 strike-animation time: starts at -1 (GameInit
    // clear) and round-trips through the setter.
    fn strike_anim_time_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.strike_anim_time(), -1);
        human.set_strike_anim_time(120);
        assert_eq!(human.strike_anim_time(), 120);
        human.set_strike_anim_time(-1);
        assert_eq!(human.strike_anim_time(), -1);
    }
    #[test]
    // Working-name 0x234 strike-animation time: the setter stores the exact
    // staged word without touching neighbours.
    fn set_strike_anim_time_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_strike_anim_time(0);
        assert_eq!(human.strike_anim_time(), 0);
        assert_eq!(human.agony_anim(), -1);
    }
    #[test]
    // Working-name 0x458 agony-animation selector: starts at -1 (GameInit
    // clear) and round-trips through the setter.
    fn agony_anim_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.agony_anim(), -1);
        human.set_agony_anim(8);
        assert_eq!(human.agony_anim(), 8);
        human.set_agony_anim(-1);
        assert_eq!(human.agony_anim(), -1);
    }
    #[test]
    // Working-name 0x458 agony-animation selector: the setter stores the
    // exact staged word (a HitAgony 12/8/4/0 pick) without touching
    // neighbours.
    fn set_agony_anim_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_agony_anim(12);
        assert_eq!(human.agony_anim(), 12);
        assert_eq!(human.agony_count(), 0);
    }
    #[test]
    // Working-name 0x45c agony counter: starts at 0 and round-trips through
    // the setter.
    fn agony_count_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.agony_count(), 0);
        human.set_agony_count(9);
        assert_eq!(human.agony_count(), 9);
        human.set_agony_count(0);
        assert_eq!(human.agony_count(), 0);
    }
    #[test]
    // Working-name 0x45c agony counter: the setter stores the exact staged
    // word (a HitAgony 5..12 pick) without touching neighbours.
    fn set_agony_count_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_agony_count(5);
        assert_eq!(human.agony_count(), 5);
        assert_eq!(human.agony_anim(), -1);
    }
    #[test]
    // Working-name 0x41c jump-fall latch: starts clear and round-trips
    // through the setter.
    fn jump_fall_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert!(!human.jump_fall());
        human.set_jump_fall(true);
        assert!(human.jump_fall());
        human.set_jump_fall(false);
        assert!(!human.jump_fall());
    }
    #[test]
    // Working-name 0x41c jump-fall latch: the setter stores the exact staged
    // latch without touching neighbours.
    fn set_jump_fall_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_jump_fall(true);
        assert!(human.jump_fall());
        assert_eq!(human.vehicle_action(), 0);
    }
    #[test]
    // Working-name 0x410 vehicle-use selector: starts at 0 (GameInit clear)
    // and round-trips through the setter.
    fn vehicle_action_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.vehicle_action(), 0);
        human.set_vehicle_action(3);
        assert_eq!(human.vehicle_action(), 3);
        human.set_vehicle_action(0);
        assert_eq!(human.vehicle_action(), 0);
    }
    #[test]
    // Working-name 0x410 vehicle-use selector: the setter stores the exact
    // staged word without touching neighbours.
    fn set_vehicle_action_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_vehicle_action(1);
        assert_eq!(human.vehicle_action(), 1);
        assert!(!human.jump_fall());
    }
    #[test]
    // Working-name 0x454 shot-death latch: starts clear (GameInit clear) and
    // round-trips through the setter.
    fn killed_by_shot_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert!(!human.killed_by_shot());
        human.set_killed_by_shot(true);
        assert!(human.killed_by_shot());
        human.set_killed_by_shot(false);
        assert!(!human.killed_by_shot());
    }
    #[test]
    // Working-name 0x454 shot-death latch: the setter stores the exact staged
    // latch without touching neighbours.
    fn set_killed_by_shot_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_killed_by_shot(true);
        assert!(human.killed_by_shot());
        assert_eq!(human.wagon(), 0);
    }
    #[test]
    // Working-name 0x584 rail-wagon selector: starts at 0 (the vehicle
    // itself) and round-trips through the setter.
    fn wagon_working_name_defaults_and_round_trips() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.wagon(), 0);
        human.set_wagon(2);
        assert_eq!(human.wagon(), 2);
        human.set_wagon(0);
        assert_eq!(human.wagon(), 0);
    }
    #[test]
    // Working-name 0x584 rail-wagon selector: the setter stores the exact
    // staged word without touching neighbours.
    fn set_wagon_working_name_stores_word() {
        let mut human = HumanState::construct(seed());
        human.set_wagon(1);
        assert_eq!(human.wagon(), 1);
        assert!(!human.killed_by_shot());
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
    // Working-name seat default of C_human::Can_DropOutFromCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6328-6329; inlined, no RVA claimed):
    // a negative request resolves to the staged seat latch.
    fn can_drop_seat_default_working_name_resolves_negative() {
        assert_eq!(HumanState::can_drop_seat_default(-1, 2), 2);
        assert_eq!(HumanState::can_drop_seat_default(0, 2), 0);
        assert_eq!(HumanState::can_drop_seat_default(3, 2), 3);
    }
    #[test]
    // Working-name strength gate of C_human::Can_SpecialStroke (mafia-re
    // 113b5b6 game/actors/C_human.cpp:7199; inlined, no RVA claimed):
    // energy above ten times strength admits.
    fn special_stroke_strength_gate_working_name_scales_tenfold() {
        assert!(HumanState::special_stroke_strength_gate(2.0, 21.0));
        assert!(!HumanState::special_stroke_strength_gate(2.0, 20.0));
        assert!(!HumanState::special_stroke_strength_gate(0.0, 0.0));
    }
    #[test]
    // Working-name range gate of C_human::Can_SpecialStroke (mafia-re 113b5b6
    // game/actors/C_human.cpp:7203; inlined, no RVA claimed): squared
    // distance below one admits.
    fn special_stroke_in_range_working_name_bounds_one_metre() {
        assert!(HumanState::special_stroke_in_range(0.99));
        assert!(!HumanState::special_stroke_in_range(1.0));
        assert!(!HumanState::special_stroke_in_range(4.0));
    }
    #[test]
    // Working-name facing gates of C_human::Can_SpecialStroke (mafia-re
    // 113b5b6 game/actors/C_human.cpp:7203-7204; inlined, no RVA claimed):
    // ahead within 0.3 rad and turned away beyond 2.85 rad admits.
    fn special_stroke_angles_ok_working_name_checks_facing_pair() {
        assert!(HumanState::special_stroke_angles_ok(0.29, 2.86));
        assert!(!HumanState::special_stroke_angles_ok(0.3, 2.86));
        assert!(!HumanState::special_stroke_angles_ok(0.29, 2.85));
    }
    #[test]
    // Working-name seat-side sign of the drop/enter offsets (mafia-re 113b5b6
    // game/actors/C_human.cpp:6292-6293, :6336-6337, :6380-6381; inlined, no
    // RVA claimed): the flag side keeps the sign, otherwise it flips.
    fn drop_side_sign_working_name_mirrors_flag_side() {
        assert_eq!(HumanState::drop_side_sign(true), 1.0);
        assert_eq!(HumanState::drop_side_sign(false), -1.0);
    }
    #[test]
    // Working-name ground-height staging of the drop/enter probes (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6303-6307; inlined, no RVA claimed):
    // ground stages hit + 0.7, otherwise the position lifts by 1.0.
    fn drop_ground_adjust_working_name_lifts_or_settles() {
        assert_eq!(HumanState::drop_ground_adjust(true, 0.0, 10.0), 0.7);
        assert_eq!(HumanState::drop_ground_adjust(false, 0.0, 10.0), 11.0);
    }
    #[test]
    // Working-name line-check arm of C_human::Can_EnterToCar (mafia-re 113b5b6
    // game/actors/C_human.cpp:6410-6417; inlined, no RVA claimed): only a
    // requested check over a crossing car refuses.
    fn enter_test_line_blocks_working_name_gates_crossing() {
        assert!(HumanState::enter_test_line_blocks(true, true));
        assert!(!HumanState::enter_test_line_blocks(true, false));
        assert!(!HumanState::enter_test_line_blocks(false, true));
        assert!(!HumanState::enter_test_line_blocks(false, false));
    }
    #[test]
    // Working-name null-car guard of C_human::Can_DropOutFromCar_Free
    // (mafia-re 113b5b6 game/actors/C_human.cpp:6286-6287; inlined, no RVA
    // claimed): admission needs a staged car.
    fn can_drop_free_null_guard_working_name_needs_car() {
        assert!(HumanState::can_drop_free_null_guard(true));
        assert!(!HumanState::can_drop_free_null_guard(false));
    }
    #[test]
    // Working-name side offset of C_human::Can_DropOutFromCar_Free (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6291-6293; inlined 2.5 m vector, no RVA
    // claimed): the flag side keeps +2.5 on x, otherwise -2.5.
    fn drop_free_side_offset_working_name_scales_flag_side() {
        assert_eq!(HumanState::drop_free_side_offset(true), [2.5, 0.0, 0.0]);
        assert_eq!(HumanState::drop_free_side_offset(false), [-2.5, 0.0, 0.0]);
    }
    #[test]
    // Working-name side offset of C_human::Can_DropOutFromCar (mafia-re 113b5b6
    // game/actors/C_human.cpp:6336-6338; inlined 1.2 m vector, no RVA
    // claimed): the flag side keeps +1.2 on x, otherwise -1.2.
    fn drop_side_offset_working_name_scales_flag_side() {
        assert_eq!(HumanState::drop_side_offset(true), [1.2, 0.0, 0.0]);
        assert_eq!(HumanState::drop_side_offset(false), [-1.2, 0.0, 0.0]);
    }
    #[test]
    // Working-name side offset of C_human::Can_EnterToCar (mafia-re 113b5b6
    // game/actors/C_human.cpp:6380-6382; inlined 1.3 m vector, no RVA
    // claimed): the flag side keeps +1.3 on x, otherwise -1.3.
    fn enter_side_offset_working_name_scales_flag_side() {
        assert_eq!(HumanState::enter_side_offset(true), [1.3, 0.0, 0.0]);
        assert_eq!(HumanState::enter_side_offset(false), [-1.3, 0.0, 0.0]);
    }
    #[test]
    // Working-name GetSeatProperty gate shared by the drop/enter guards
    // (mafia-re 113b5b6 game/actors/C_human.cpp:6289, :6334, :6378; inlined,
    // no RVA claimed): admission needs a staged seat lookup hit.
    fn can_seat_property_admits_working_name_needs_lookup_hit() {
        assert!(HumanState::can_seat_property_admits(true));
        assert!(!HumanState::can_seat_property_admits(false));
    }
    #[test]
    // Working-name room-probe conjunction of the drop guards (mafia-re 113b5b6
    // game/actors/C_human.cpp:6308-6316, :6355-6363; two inlined TestSphereB
    // sweeps, no RVA claimed): both staged sweeps must miss.
    fn can_drop_room_clears_working_name_needs_both_misses() {
        assert!(HumanState::can_drop_room_clears(false, false));
        assert!(!HumanState::can_drop_room_clears(true, false));
        assert!(!HumanState::can_drop_room_clears(false, true));
        assert!(!HumanState::can_drop_room_clears(true, true));
    }
    #[test]
    // Working-name room-probe conjunction of C_human::Can_EnterToCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6400-6408; inlined TestLineH plus
    // TestSphereB, no RVA claimed): both staged probes must miss.
    fn enter_room_clears_working_name_needs_both_misses() {
        assert!(HumanState::enter_room_clears(false, false));
        assert!(!HumanState::enter_room_clears(true, false));
        assert!(!HumanState::enter_room_clears(false, true));
        assert!(!HumanState::enter_room_clears(true, true));
    }
    #[test]
    // Working-name test-line vector of C_human::Can_EnterToCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6412-6414; inlined arithmetic, no RVA
    // claimed): a metre above the frame world position minus the exit.
    fn enter_test_line_vector_working_name_lifts_frame_over_exit() {
        assert_eq!(
            HumanState::enter_test_line_vector([1.0, 2.0, 3.0], [4.0, 5.0, 6.0]),
            [-3.0, -2.0, -3.0]
        );
        assert_eq!(
            HumanState::enter_test_line_vector([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            [0.0, 1.0, 0.0]
        );
    }
    #[test]
    // Working-name ground-probe legs shared by the drop/enter guards
    // (mafia-re 113b5b6 game/actors/C_human.cpp:6303, :6350, :6395; inlined
    // vectors, no RVA claimed): one metre up, two metres down.
    fn drop_line_probe_legs_working_name_stages_lift_and_fall() {
        assert_eq!(
            HumanState::drop_line_probe_legs(),
            ([0.0, 1.0, 0.0], [0.0, -2.0, 0.0])
        );
    }
    #[test]
    // Working-name sweep radius of the drop/enter room probes (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6308, :6355, :6404; inlined 0.3
    // literal, no RVA claimed).
    fn drop_sweep_radius_working_name_is_three_tenths() {
        assert_eq!(HumanState::drop_sweep_radius(), 0.3);
    }
    #[test]
    // Working-name exit-point shift of the drop/enter end probes (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6312, :6359, :6404; inlined addition,
    // no RVA claimed): component-wise sum of the adjusted position and the
    // side offset.
    fn drop_exit_point_working_name_adds_pos_and_off() {
        assert_eq!(
            HumanState::drop_exit_point([1.0, 2.0, 3.0], [0.5, -1.0, 2.0]),
            [1.5, 1.0, 5.0]
        );
        assert_eq!(
            HumanState::drop_exit_point([0.0, 0.0, 0.0], [0.0, 0.0, 0.0]),
            [0.0, 0.0, 0.0]
        );
    }
    #[test]
    // Working-name end-probe rise of the drop guards (mafia-re 113b5b6
    // game/actors/C_human.cpp:6312, :6359; inlined 0.3 vector, no RVA
    // claimed).
    fn drop_end_probe_up_working_name_rises_three_tenths() {
        assert_eq!(HumanState::drop_end_probe_up(), [0.0, 0.3, 0.0]);
    }
    #[test]
    // Working-name end-probe rise of C_human::Can_EnterToCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6404; inlined 0.5 vector, no RVA
    // claimed).
    fn enter_end_probe_up_working_name_rises_half() {
        assert_eq!(HumanState::enter_end_probe_up(), [0.0, 0.5, 0.0]);
    }
    #[test]
    // Working-name head-height staging of C_human::Can_Aimed (mafia-re
    // 113b5b6 game/actors/C_human.cpp:8914; inlined 1.5 lift, no RVA
    // claimed).
    fn can_aimed_probe_offset_working_name_lifts_head() {
        assert_eq!(HumanState::can_aimed_probe_offset(), [0.0, 1.5, 0.0]);
    }
    #[test]
    // Working-name exit-offset flattening of the drop/enter guards (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6296, :6341, :6386; inlined y clear,
    // no RVA claimed): the y lane is dropped, x/z pass through.
    fn drop_offset_flatten_y_working_name_zeroes_y_lane() {
        assert_eq!(
            HumanState::drop_offset_flatten_y([1.0, 2.0, 3.0]),
            [1.0, 0.0, 3.0]
        );
        assert_eq!(
            HumanState::drop_offset_flatten_y([-2.5, -0.5, 0.25]),
            [-2.5, 0.0, 0.25]
        );
    }
    #[test]
    // Working-name exclusion-list order of the drop/enter probes (mafia-re
    // 113b5b6 game/actors/C_human.cpp:6298-6300, :6343-6346, :6389-6392;
    // inlined list build, no RVA claimed): own collisions reversed, then
    // the car; empty own stages the Free car-only shape.
    fn drop_probe_owner_order_working_name_reverses_own_then_car() {
        let car = HumanDynCollId::from_raw(9);
        assert_eq!(HumanState::drop_probe_owner_order(&[], car), [car]);
        let own = [
            HumanDynCollId::from_raw(1),
            HumanDynCollId::from_raw(2),
            HumanDynCollId::from_raw(3),
        ];
        assert_eq!(
            HumanState::drop_probe_owner_order(&own, car),
            [
                HumanDynCollId::from_raw(3),
                HumanDynCollId::from_raw(2),
                HumanDynCollId::from_raw(1),
                car,
            ]
        );
    }
    #[test]
    // Working-name fire-latch staging (mafia-re 113b5b6
    // game/actors/C_human.h:739-741 0x1ed note_fire, :748-750 0x1fc
    // reloading, :751-753 0x1fd reload_forced, :754-757 0x1fe
    // loading_shells, :800-803 0x580 weapon_displaced, :905-908 0x581
    // reload_effects; inline words, no RVA claimed): all start clear and
    // round-trip through their setters.
    fn fire_latches_working_name_start_clear_and_round_trip() {
        let mut human = HumanState::construct(seed());
        assert!(!human.note_fire());
        assert!(!human.reloading());
        assert!(!human.reload_forced());
        assert!(!human.loading_shells());
        assert!(!human.weapon_displaced());
        assert!(!human.reload_effects());
        human.set_note_fire(true);
        human.set_reloading(true);
        human.set_reload_forced(true);
        human.set_loading_shells(true);
        human.set_weapon_displaced(true);
        human.set_reload_effects(true);
        assert!(human.note_fire());
        assert!(human.reloading());
        assert!(human.reload_forced());
        assert!(human.loading_shells());
        assert!(human.weapon_displaced());
        assert!(human.reload_effects());
        human.set_note_fire(false);
        human.set_reloading(false);
        human.set_reload_forced(false);
        human.set_loading_shells(false);
        human.set_weapon_displaced(false);
        human.set_reload_effects(false);
        assert!(!human.note_fire());
        assert!(!human.reloading());
        assert!(!human.reload_forced());
        assert!(!human.loading_shells());
        assert!(!human.weapon_displaced());
        assert!(!human.reload_effects());
    }
    #[test]
    // Working-name stance-latch staging (mafia-re 113b5b6
    // game/actors/C_human.h:784-786 0x22d sniper_mode, :787-790 0x22e
    // test_ground, :791-793 0x22f drowned, :773-775 0x224 backswing,
    // :777-779 0x228 strike_index, :801-803 0x238 tilt; inline words, no
    // RVA claimed): flags start clear, words at 0/0.0, and all round-trip.
    fn stance_latches_working_name_start_clear_and_round_trip() {
        let mut human = HumanState::construct(seed());
        assert!(!human.sniper_mode());
        assert!(!human.test_ground());
        assert!(!human.drowned());
        assert!(!human.backswing());
        assert_eq!(human.strike_index(), 0);
        assert_eq!(human.tilt(), 0.0);
        human.set_sniper_mode(true);
        human.set_test_ground(true);
        human.set_drowned(true);
        human.set_backswing(true);
        human.set_strike_index(2);
        human.set_tilt(-0.25);
        assert!(human.sniper_mode());
        assert!(human.test_ground());
        assert!(human.drowned());
        assert!(human.backswing());
        assert_eq!(human.strike_index(), 2);
        assert_eq!(human.tilt(), -0.25);
        human.set_sniper_mode(false);
        human.set_test_ground(false);
        human.set_drowned(false);
        human.set_backswing(false);
        human.set_strike_index(-1);
        human.set_tilt(0.5);
        assert!(!human.sniper_mode());
        assert!(!human.test_ground());
        assert!(!human.drowned());
        assert!(!human.backswing());
        assert_eq!(human.strike_index(), -1);
        assert_eq!(human.tilt(), 0.5);
    }
    #[test]
    // Working-name limiter/bonus staging (mafia-re 113b5b6
    // game/actors/C_human.h:1077-1078 0xa78 speed_limiter, :1080-1081 0xa79
    // score_bonus; inline words, no RVA claimed): both start clear and
    // round-trip through their setters.
    fn limiter_bonus_working_name_start_clear_and_round_trip() {
        let mut human = HumanState::construct(seed());
        assert!(!human.speed_limiter());
        assert!(!human.score_bonus());
        human.set_speed_limiter(true);
        human.set_score_bonus(true);
        assert!(human.speed_limiter());
        assert!(human.score_bonus());
        human.set_speed_limiter(false);
        human.set_score_bonus(false);
        assert!(!human.speed_limiter());
        assert!(!human.score_bonus());
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
    #[test]
    // GAME 0x578ee0 (RVA 0x178ee0) C_human::PoseInterpolate (mafia-re 113b5b6
    // game/actors/C_human.cpp:4557-4562): explicit unsupported stub,
    // pose/frame owners; invoking it touches no staged words.
    fn pose_interpolate_rva_178ee0_stub_runs_without_state() {
        HumanState::pose_interpolate_unsupported(0);
        HumanState::pose_interpolate_unsupported(1000);
    }
    #[test]
    // GAME 0x570570 (RVA 0x170570) C_human::Death (mafia-re 113b5b6
    // game/actors/C_human.cpp:1671): explicit unsupported stub, shadow/
    // collision/animation/police/camera/lightness/lock owners; invoking it
    // touches no staged words.
    fn death_rva_170570_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        human.set_alive(true);
        let before = (human.alive(), human.action_state(), human.anim_id());
        HumanState::death_unsupported();
        assert_eq!((human.alive(), human.action_state(), human.anim_id()), before);
    }
    #[test]
    // GAME 0x570660 (RVA 0x170660) C_human::Extern_AI_Update (mafia-re 113b5b6
    // game/actors/C_human.cpp:1699): explicit unsupported stub, animation-
    // machine/frame/collision owners; invoking it touches no staged words.
    fn extern_ai_update_rva_170660_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.action_state(), human.next_anim_id(), human.turn_speed());
        HumanState::extern_ai_update_unsupported(16);
        assert_eq!(
            (human.action_state(), human.next_anim_id(), human.turn_speed()),
            before
        );
    }
    #[test]
    // GAME 0x5716d0 (RVA 0x1716d0) C_human::Intern_UseCar(bool) (mafia-re
    // 113b5b6 game/actors/C_human.cpp:2068): explicit unsupported stub, car/
    // frame/police/indicator/shadow/inventory owners; distinct from the
    // claimed 0x57e020 seat-placement overload. Invoking it touches no staged
    // words.
    fn intern_use_car_flag_rva_1716d0_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.car(), human.using_car(), human.action_state());
        HumanState::intern_use_car_flag_unsupported(true);
        HumanState::intern_use_car_flag_unsupported(false);
        assert_eq!((human.car(), human.using_car(), human.action_state()), before);
    }
    #[test]
    // GAME 0x571b90 (RVA 0x171b90) C_human::Intern_UseRailway (mafia-re
    // 113b5b6 game/actors/C_human.cpp:2159): explicit unsupported stub,
    // railway/frame/matrix/collision/camera/police/shadow owners; invoking
    // it touches no staged words.
    fn intern_use_railway_rva_171b90_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.car(), human.using_car(), human.action_state());
        HumanState::intern_use_railway_unsupported(true);
        HumanState::intern_use_railway_unsupported(false);
        assert_eq!((human.car(), human.using_car(), human.action_state()), before);
    }
    #[test]
    // GAME 0x5721b0 (RVA 0x1721b0) C_human::_intern_UseCarTruck (mafia-re
    // 113b5b6 game/actors/C_human.cpp:2234): explicit unsupported stub,
    // truck/frame/collision/camera/police/navigation owners; invoking it
    // touches no staged words.
    fn intern_use_car_truck_rva_1721b0_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.car(), human.using_car(), human.seat());
        HumanState::intern_use_car_truck_unsupported();
        assert_eq!((human.car(), human.using_car(), human.seat()), before);
    }
    #[test]
    // GAME 0x572530 (RVA 0x172530) C_human::ThrowFromCarTruck (mafia-re
    // 113b5b6 game/actors/C_human.cpp:2300): explicit unsupported stub,
    // truck/frame/scene/collision/damage owners; invoking it touches no
    // staged words.
    fn throw_from_car_truck_rva_172530_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.car(), human.action_state(), human.velocity());
        HumanState::throw_from_car_truck_unsupported();
        assert_eq!((human.car(), human.action_state(), human.velocity()), before);
    }
    #[test]
    // GAME 0x5757c0 (RVA 0x1757c0) C_human::GameDone (mafia-re 113b5b6
    // game/actors/C_human.cpp:3407): explicit unsupported stub, model/frame/
    // shadow/sound/using-object owners; invoking it touches no staged words.
    fn human_game_done_rva_1757c0_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.action_state(), human.anim_id(), human.next_anim_id());
        HumanState::human_game_done_unsupported();
        assert_eq!((human.action_state(), human.anim_id(), human.next_anim_id()), before);
    }
    #[test]
    // GAME 0x584620 (RVA 0x184620) C_human::Shooting (mafia-re 113b5b6
    // game/actors/C_human.cpp:7351): explicit unsupported stub, inventory/
    // frame/police/sound/grenade/weapon/light/force-feedback owners;
    // invoking it touches no staged words.
    fn shooting_rva_184620_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.reloading(), human.fire_held(), human.note_fire());
        HumanState::shooting_unsupported();
        assert_eq!((human.reloading(), human.fire_held(), human.note_fire()), before);
    }
    #[test]
    // GAME 0x58b8d0 (RVA 0x18b8d0) C_human::RecompileDeathPos (mafia-re
    // 113b5b6 game/actors/C_human.cpp:9313): explicit unsupported stub,
    // model/frame/animation-cache owners; invoking it touches no staged
    // words.
    fn recompile_death_pos_rva_18b8d0_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.action_state(), human.anim_id(), human.next_anim_id());
        HumanState::recompile_death_pos_unsupported();
        assert_eq!((human.action_state(), human.anim_id(), human.next_anim_id()), before);
    }
    #[test]
    // GAME 0x56b8d0 (RVA 0x16b8d0) C_human::PlayStep (mafia-re 113b5b6
    // game/actors/C_human.cpp:250): explicit unsupported stub, records/
    // cutscene/model/frame owners; invoking it touches no staged words.
    fn play_step_rva_16b8d0_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        let before = (human.action_state(), human.anim_id(), human.next_anim_id());
        HumanState::play_step_unsupported(100);
        assert_eq!((human.action_state(), human.anim_id(), human.next_anim_id()), before);
    }
    #[test]
    // GAME 0x56c6e0 (RVA 0x16c6e0) C_human::DumpRecords (mafia-re 113b5b6
    // game/actors/C_human.cpp:536): explicit unsupported stub, frame-name/
    // record-list/host-file owners; invoking it touches no staged words.
    fn dump_records_rva_16c6e0_stub_runs_without_state() {
        let human = HumanState::construct(seed());
        let before = (human.action_state(), human.anim_id(), human.next_anim_id());
        HumanState::dump_records_unsupported();
        assert_eq!((human.action_state(), human.anim_id(), human.next_anim_id()), before);
    }
    #[test]
    // GAME 0x5747d0 (RVA 0x1747d0) C_human::ShiftAnimation (mafia-re 113b5b6
    // game/actors/C_human.cpp:3033): explicit unsupported stub, animation
    // cache/table/frame owners; invoking it touches no staged words.
    fn rva_1747d0_shift_animation_stub_runs_without_state() {
        HumanState::shift_animation_unsupported(0);
        HumanState::shift_animation_unsupported(1);
    }
    #[test]
    // GAME 0x5748f0 (RVA 0x1748f0) C_human::ChangeState (mafia-re 113b5b6
    // game/actors/C_human.cpp:3085): explicit unsupported stub, collision/
    // mission/shadow/frame owners; invoking it touches no staged words.
    fn rva_1748f0_change_state_stub_runs_without_state() {
        HumanState::change_state_unsupported();
    }
    #[test]
    // GAME 0x575ca0 (RVA 0x175ca0) C_human::CreateDynColls (mafia-re 113b5b6
    // game/actors/C_human.cpp:3561): explicit unsupported stub, frame/
    // collision owners; invoking it files no owner.
    fn rva_175ca0_create_dyn_colls_stub_runs_without_state() {
        HumanState::create_dyn_colls_unsupported();
    }
    #[test]
    // GAME 0x575ed0 (RVA 0x175ed0) C_human::EraseDynColls (mafia-re 113b5b6
    // game/actors/C_human.cpp:3598): explicit unsupported stub, collision
    // owner; invoking it touches no staged words.
    fn rva_175ed0_erase_dyn_colls_stub_runs_without_state() {
        HumanState::erase_dyn_colls_unsupported();
    }
    #[test]
    // GAME 0x576040 (RVA 0x176040) C_human::UpdateDynColl (mafia-re 113b5b6
    // game/actors/C_human.cpp:3667): explicit unsupported stub, frame/
    // collision owners; invoking it touches no staged words.
    fn rva_176040_update_dyn_coll_stub_runs_without_state() {
        HumanState::update_dyn_coll_unsupported();
    }
    #[test]
    // GAME 0x579630 (RVA 0x179630) C_human::PoseSetPoseNormal (mafia-re
    // 113b5b6 game/actors/C_human.cpp:4626): explicit unsupported stub, frame
    // owner; the native bool verdict is preserved as Err.
    fn rva_179630_pose_set_pose_normal_reports_unsupported() {
        assert_eq!(POSE_SET_POSE_NORMAL_ENTRY, 0x579630);
        assert_eq!(
            HumanState::pose_set_pose_normal_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x579630 })
        );
    }
    #[test]
    // GAME 0x579ea0 (RVA 0x179ea0) C_human::PoseSetPoseAimed (mafia-re 113b5b6
    // game/actors/C_human.cpp:4667): explicit unsupported stub, frame owner;
    // the native bool verdict is preserved as Err.
    fn rva_179ea0_pose_set_pose_aimed_reports_unsupported() {
        assert_eq!(POSE_SET_POSE_AIMED_ENTRY, 0x579ea0);
        assert_eq!(
            HumanState::pose_set_pose_aimed_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x579ea0 })
        );
    }
    #[test]
    // GAME 0x57ec20 (RVA 0x17ec20) C_human::ChangeWeaponModel (mafia-re
    // 113b5b6 game/actors/C_human.cpp:5773): explicit unsupported stub,
    // model/animation owners; invoking it touches no staged words.
    fn rva_17ec20_change_weapon_model_stub_runs_without_state() {
        HumanState::change_weapon_model_unsupported();
    }
    #[test]
    // GAME 0x57faa0 (RVA 0x17faa0) C_human::DropOutItems (mafia-re 113b5b6
    // game/actors/C_human.cpp:6146): explicit unsupported stub, model-cache/
    // driver/scene owners past the pure `drop_out_items_gate`; the native
    // bool verdict is preserved as Err.
    fn rva_17faa0_drop_out_items_reports_unsupported() {
        assert_eq!(DROP_OUT_ITEMS_ENTRY, 0x57faa0);
        assert!(drop_out_items_gate(true, true, 1));
        assert_eq!(
            HumanState::drop_out_items_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x57faa0 })
        );
    }
    #[test]
    // GAME 0x580ca0 (RVA 0x180ca0) C_human::ZasrateLezeniZAuta_ve2rano
    // (mafia-re 113b5b6 game/actors/C_human.cpp:6427): explicit unsupported
    // stub, car/collision/scene/animation owners; the native bool verdict is
    // preserved as Err.
    fn rva_180ca0_zasrate_lezeni_reports_unsupported() {
        assert_eq!(ZASRATE_LEZENI_VE2RANO_ENTRY, 0x580ca0);
        assert_eq!(
            HumanState::zasrate_lezeni_ve2rano_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x580ca0 })
        );
    }
    #[test]
    // GAME 0x581220 (RVA 0x181220) C_human::ZasrateVybliknutiZAuta_ve3rano
    // (mafia-re 113b5b6 game/actors/C_human.cpp:6519): explicit unsupported
    // stub, car/collision/scene/animation owners; invoking it touches no
    // staged words.
    fn rva_181220_zasrate_vybliknuti_stub_runs_without_state() {
        HumanState::zasrate_vybliknuti_ve3rano_unsupported();
    }
    #[test]
    // GAME 0x56b1a0 (RVA 0x16b1a0) C_human::RecordKey (mafia-re 113b5b6
    // game/actors/C_human.cpp:54): explicit unsupported stub, record-store/
    // frame/cutscene-link owners; the native int verdict is preserved as Err.
    fn rva_16b1a0_record_key_reports_unsupported() {
        assert_eq!(RECORD_KEY_ENTRY, 0x56b1a0);
        assert_eq!(
            HumanState::record_key_unsupported(100),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56b1a0 })
        );
    }
    #[test]
    // GAME 0x56b670 (RVA 0x16b670) C_human::PlayRecord (mafia-re 113b5b6
    // game/actors/C_human.cpp:165): explicit unsupported stub, record-store/
    // animation-machine/cutscene/frame owners; invoking it touches no staged
    // words.
    fn rva_16b670_play_record_stub_runs_without_state() {
        HumanState::play_record_unsupported(100);
    }
    #[test]
    // GAME 0x56bec0 (RVA 0x16bec0) C_human::PlaceAtTime (mafia-re 113b5b6
    // game/actors/C_human.cpp:368): explicit unsupported stub, record-store/
    // frame owners; the native bool verdict is preserved as Err.
    fn rva_16bec0_place_at_time_reports_unsupported() {
        assert_eq!(PLACE_AT_TIME_ENTRY, 0x56bec0);
        assert_eq!(
            HumanState::place_at_time_unsupported(100),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56bec0 })
        );
    }
    #[test]
    // GAME 0x56c7c0 (RVA 0x16c7c0) C_human::GetRecordPos (mafia-re 113b5b6
    // game/actors/C_human.cpp:565): explicit unsupported stub, record-store
    // owner; the native S_vector verdict is preserved as Err.
    fn rva_16c7c0_get_record_pos_reports_unsupported() {
        assert_eq!(GET_RECORD_POS_ENTRY, 0x56c7c0);
        assert_eq!(
            HumanState::get_record_pos_unsupported(100),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56c7c0 })
        );
    }
    #[test]
    // GAME 0x56cc70 (RVA 0x16cc70) C_human::GetRecordRot (mafia-re 113b5b6
    // game/actors/C_human.cpp:668): explicit unsupported stub, record-store
    // owner; the native S_quat verdict is preserved as Err.
    fn rva_16cc70_get_record_rot_reports_unsupported() {
        assert_eq!(GET_RECORD_ROT_ENTRY, 0x56cc70);
        assert_eq!(
            HumanState::get_record_rot_unsupported(100),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56cc70 })
        );
    }
    #[test]
    // GAME 0x56e3b0 (RVA 0x16e3b0) ai_loc::Update (mafia-re 113b5b6
    // game/actors/C_human.cpp:1079): explicit unsupported stub, AI owner;
    // invoking it touches no staged words.
    fn rva_16e3b0_ai_loc_update_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        human.ai_loc_update_unsupported();
    }
    #[test]
    // GAME 0x56efd0 (RVA 0x16efd0) C_human::C_human (mafia-re 113b5b6
    // game/actors/C_human.cpp:1250): explicit unsupported stub; construction
    // stays with the actor owner.
    fn rva_16efd0_ctor_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        human.ctor_unsupported();
    }
    #[test]
    // GAME 0x56f350 (RVA 0x16f350) C_human::Init (mafia-re 113b5b6
    // game/actors/C_human.cpp:1284): explicit unsupported stub, frame owner;
    // the native bool verdict is preserved as Err.
    fn rva_16f350_init_reports_unsupported() {
        assert_eq!(INIT_ENTRY, 0x56f350);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.init_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56f350 })
        );
    }
    #[test]
    // GAME 0x56f4e0 (RVA 0x16f4e0) C_human::LoadData (mafia-re 113b5b6
    // game/actors/C_human.cpp:1377): explicit unsupported stub, chunk owner;
    // the native bool verdict is preserved as Err.
    fn rva_16f4e0_load_data_reports_unsupported() {
        assert_eq!(LOAD_DATA_ENTRY, 0x56f4e0);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.load_data_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56f4e0 })
        );
    }
    #[test]
    // GAME 0x56fa20 (RVA 0x16fa20) C_human::SaveGameSave (mafia-re 113b5b6
    // game/actors/C_human.cpp:1428): explicit unsupported stub, buffer owner;
    // the native bool verdict is preserved as Err.
    fn rva_16fa20_save_game_save_reports_unsupported() {
        assert_eq!(SAVE_GAME_SAVE_ENTRY, 0x56fa20);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.save_game_save_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56fa20 })
        );
    }
    #[test]
    // GAME 0x56fd30 (RVA 0x16fd30) C_human::SaveGameLoad (mafia-re 113b5b6
    // game/actors/C_human.cpp:1494): explicit unsupported stub, buffer owner;
    // the native bool verdict is preserved as Err.
    fn rva_16fd30_save_game_load_reports_unsupported() {
        assert_eq!(SAVE_GAME_LOAD_ENTRY, 0x56fd30);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.save_game_load_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x56fd30 })
        );
    }
    #[test]
    // GAME 0x570740 (RVA 0x170740) C_human::AI (mafia-re 113b5b6
    // game/actors/C_human.cpp:1730): explicit unsupported stub, AI owner;
    // invoking it touches no staged words.
    fn rva_170740_ai_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        human.ai_unsupported(16);
    }
    #[test]
    // GAME 0x572b80 (RVA 0x172b80) C_human::Update (mafia-re 113b5b6
    // game/actors/C_human.cpp:2338): explicit unsupported stub, simulation
    // owners; invoking it touches no staged words.
    fn rva_172b80_update_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        human.update_unsupported(16);
    }
    #[test]
    // GAME 0x573e50 (RVA 0x173e50) C_human::PersonAnim (mafia-re 113b5b6
    // game/actors/C_human.cpp:2743): explicit unsupported stub, animation
    // owner; invoking it touches no staged words.
    fn rva_173e50_person_anim_stub_runs_without_state() {
        let mut human = HumanState::construct(seed());
        human.person_anim_unsupported(0.5, true, false);
    }
    #[test]
    // GAME 0x5762a0 (RVA 0x1762a0) C_human::Hit (mafia-re 113b5b6
    // game/actors/C_human.cpp:3729): explicit unsupported stub, collision/
    // actor owners; the native bool verdict is preserved as Err.
    fn rva_1762a0_hit_reports_unsupported() {
        assert_eq!(HIT_ENTRY, 0x5762a0);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.hit_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x5762a0 })
        );
    }
    #[test]
    // GAME 0x578290 (RVA 0x178290) C_human::HitInCar (mafia-re 113b5b6
    // game/actors/C_human.cpp:4358): explicit unsupported stub, collision/
    // actor owners; the native bool verdict is preserved as Err.
    fn rva_178290_hit_in_car_reports_unsupported() {
        assert_eq!(HIT_IN_CAR_ENTRY, 0x578290);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.hit_in_car_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x578290 })
        );
    }
    #[test]
    // GAME 0x5784a0 (RVA 0x1784a0) C_human::Collision (mafia-re 113b5b6
    // game/actors/C_human.cpp:4413): explicit unsupported stub, collision/
    // actor owners; the native bool verdict is preserved as Err.
    fn rva_1784a0_collision_reports_unsupported() {
        assert_eq!(COLLISION_ENTRY, 0x5784a0);
        let mut human = HumanState::construct(seed());
        assert_eq!(
            human.collision_unsupported(),
            Err(HumanSmallProbeError::Unsupported { entry: 0x5784a0 })
        );
    }
    #[test]
    // GAME 0x5878d0 (RVA 0x1878d0) C_human::_intern_ForceDeath (mafia-re
    // 113b5b6 game/actors/C_human.cpp:8061): explicit unsupported stub,
    // animation-cache/model owners; invoking it touches no staged words.
    fn rva_1878d0_intern_force_death_stub_runs_without_state() {
        HumanState::intern_force_death_unsupported();
    }
    #[test]
    // GAME 0x588b40 (RVA 0x188b40) C_human::_intern_ThrowMeFromCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:8374): explicit unsupported stub, car/
    // seat-property/frame owners; invoking it touches no staged words.
    fn rva_188b40_intern_throw_me_from_car_stub_runs_without_state() {
        HumanState::intern_throw_me_from_car_unsupported();
    }
    #[test]
    // GAME 0x58ac90 (RVA 0x18ac90) C_human::Activate (mafia-re 113b5b6
    // game/actors/C_human.cpp:9043): explicit unsupported stub, frame/
    // animation-machine owners; invoking it touches no staged words.
    fn rva_18ac90_activate_stub_runs_without_state() {
        HumanState::activate_unsupported();
    }
    #[test]
    // GAME 0x58b330 (RVA 0x18b330) C_human::Deactivate (mafia-re 113b5b6
    // game/actors/C_human.cpp:9177): explicit unsupported stub, mission/model/
    // animation owners; invoking it touches no staged words.
    fn rva_18b330_deactivate_stub_runs_without_state() {
        HumanState::deactivate_unsupported();
    }
    #[test]
    // GAME 0x58d020 (RVA 0x18d020) C_human::FreeHand (mafia-re 113b5b6
    // game/actors/C_human.cpp:9626): explicit unsupported stub, frame/scene/
    // physics owners past the Do_Holster carry gate; invoking it touches no
    // staged words.
    fn rva_18d020_free_hand_stub_runs_without_state() {
        HumanState::free_hand_unsupported();
    }
    #[test]
    // GAME 0x58d2c0 (RVA 0x18d2c0) C_human::StartWeaponEffect (mafia-re 113b5b6
    // game/actors/C_human.cpp:9673): explicit unsupported stub, inventory/
    // frame/driver/physics owners; invoking it touches no staged words.
    fn rva_18d2c0_start_weapon_effect_stub_runs_without_state() {
        HumanState::start_weapon_effect_unsupported(0);
        HumanState::start_weapon_effect_unsupported(1);
    }
    #[test]
    // GAME 0x58da70 (RVA 0x18da70) C_human::TickWeaponEffect (mafia-re 113b5b6
    // game/actors/C_human.cpp:9872): explicit unsupported stub, effect-frame/
    // inventory owners; invoking it touches no staged words.
    fn rva_18da70_tick_weapon_effect_stub_runs_without_state() {
        HumanState::tick_weapon_effect_unsupported(0);
        HumanState::tick_weapon_effect_unsupported(16);
    }
    #[test]
    // GAME 0x58a5a0 (RVA 0x18a5a0) C_human::EineMeineKleineAutoInLuft
    // (mafia-re 113b5b6 game/actors/C_human.cpp:8852): explicit unsupported
    // stub, frame/model-change/death/animation/indicator owners; invoking it
    // touches no staged words.
    fn rva_18a5a0_eine_meine_stub_runs_without_state() {
        HumanState::eine_meine_kleine_auto_in_luft_unsupported();
    }
    #[test]
    // GAME 0x587190 (RVA 0x187190) C_human::_intern_ChangeModel (mafia-re
    // 113b5b6 game/actors/C_human.cpp:7883): explicit unsupported stub,
    // frame/model-cache/collision/shadow/mission owners; invoking it touches
    // no staged words.
    fn rva_187190_intern_change_model_stub_runs_without_state() {
        HumanState::intern_change_model_unsupported();
    }
    #[test]
    // GAME 0x582180 (RVA 0x182180) C_human::Use_Actor (mafia-re 113b5b6
    // game/actors/C_human.cpp:6681): explicit unsupported stub, door/turnout/
    // car/rail actors plus scene/police/animation owners; invoking it touches
    // no staged words.
    fn rva_182180_use_actor_stub_runs_without_state() {
        HumanState::use_actor_unsupported();
    }
    #[test]
    // GAME 0x58bf30 (RVA 0x18bf30) SettleOnGround (mafia-re 113b5b6
    // game/actors/C_human.cpp:9427): explicit unsupported stub, frame/
    // collision owners; invoking it stages no height.
    fn rva_18bf30_settle_on_ground_stub_runs_without_state() {
        assert_eq!(SETTLE_ON_GROUND_ENTRY, 0x58bf30);
        settle_on_ground_unsupported();
    }
    #[test]
    // GAME 0x58c980 (RVA 0x18c980) SlideOffEdge (mafia-re 113b5b6
    // game/actors/C_human.cpp:9514): explicit unsupported stub, frame/
    // collision owners; the native bool verdict is preserved as Err.
    fn rva_18c980_slide_off_edge_reports_unsupported() {
        assert_eq!(SLIDE_OFF_EDGE_ENTRY, 0x58c980);
        assert_eq!(
            slide_off_edge_unsupported(),
            Err(WallPushError::Unsupported { entry: 0x58c980 })
        );
    }
    #[test]
    // GAME 0x56d130 (RVA 0x16d130) human_notify_callback (mafia-re 113b5b6
    // game/actors/C_human.cpp:784): explicit unsupported stub, frame/sound/
    // scene/material owners; the native uint answer is preserved as Err.
    fn rva_16d130_human_notify_callback_reports_unsupported() {
        assert_eq!(HUMAN_NOTIFY_CALLBACK_ENTRY, 0x56d130);
        assert_eq!(
            human_notify_callback_unsupported(0, 1, 0),
            Err(HumanNotifyError::Unsupported { entry: 0x56d130 })
        );
    }
    #[test]
    // GAME 0x58d270 (RVA 0x18d270) C_human::BezZasobnikuWeaponEffect (mafia-re
    // 113b5b6 game/actors/C_human.cpp:9660): only ids 9-10 hide a present
    // visual clip; missing or non-visual clips stay untouched.
    fn clip_gate_rva_18d270_hides_only_ranged_visual_clip() {
        assert!(clip_hidden_without_mag(9, true, true));
        assert!(clip_hidden_without_mag(10, true, true));
        assert!(!clip_hidden_without_mag(8, true, true));
        assert!(!clip_hidden_without_mag(11, true, true));
        assert!(!clip_hidden_without_mag(9, false, true));
        assert!(!clip_hidden_without_mag(9, true, false));
        assert!(!clip_hidden_without_mag(0, false, false));
    }
    #[test]
    // GAME 0x58e210 (RVA 0x18e210) C_human::FakeWeaponEffect (mafia-re 113b5b6
    // game/actors/C_human.cpp:10061-10070): only the Colt 1911 (id 9) with a
    // present slide repositions it to (0, 0, -0.04); other ids, a missing
    // slide and the trailing bool leave the frame alone.
    fn colt_slide_rva_18e210_shifts_only_present_colt_slide() {
        assert_eq!(
            colt_slide_recoil_shift(GAMEITEM_COLT_1911, true),
            Some([0.0, 0.0, -0.04])
        );
        assert_eq!(colt_slide_recoil_shift(GAMEITEM_COLT_1911, false), None);
        assert_eq!(colt_slide_recoil_shift(10, true), None);
        assert_eq!(colt_slide_recoil_shift(0, false), None);
    }
    #[test]
    // GAME 0x587d70 (RVA 0x187d70) C_human::Do_ThrowCocotFromCar (mafia-re
    // 113b5b6 game/actors/C_human.cpp:8199-8228): the side vector from the car
    // direction (negated on the seat-side flag, normalized) plus the door
    // offset table over (door flag, seat).
    fn cocot_door_rva_187d70_selects_side_and_offset() {
        let front = throw_cocot_door_placement([0.0, 0.0, 1.0], false, true, 1);
        assert_eq!(front.side, [1.0, 0.0, 0.0]);
        assert_eq!(front.offset, [1.581, 0.0, 0.251]);
        let flipped = throw_cocot_door_placement([0.0, 0.0, 1.0], true, true, 3);
        assert_eq!(flipped.side, [-1.0, 0.0, 0.0]);
        assert_eq!(flipped.offset, [1.581, 0.0, 0.251]);
        let rear = throw_cocot_door_placement([1.0, 0.0, 0.0], false, true, 2);
        assert_eq!(rear.side, [0.0, 0.0, -1.0]);
        assert_eq!(rear.offset, [-1.2517, 0.0, -0.6723]);
        let plain = throw_cocot_door_placement([0.0, 0.0, 1.0], false, false, 1);
        assert_eq!(plain.offset, [1.343, 0.0, 1.148]);
        let other = throw_cocot_door_placement([0.0, 0.0, 1.0], true, false, 0);
        assert_eq!(other.side, [-1.0, 0.0, 0.0]);
        assert_eq!(other.offset, [-1.364, 0.0, 0.231]);
    }
    #[test]
    // GAME 0x5801f0 (RVA 0x1801f0) high-bridge scan (mafia-re 113b5b6
    // game/actors/C_human.cpp:6261-6276): outside the car nothing matches; in
    // the car the reverse-scan first bridge within 100 units of kind 3-5 wins.
    fn high_bridge_rva_1801f0_matches_reverse_in_range_kind() {
        let near = BridgeCandidate {
            position: [3.0, 0.0, 4.0],
            kind: 4,
        };
        let far = BridgeCandidate {
            position: [1000.0, 0.0, 0.0],
            kind: 4,
        };
        let wrong_kind = BridgeCandidate {
            position: [3.0, 0.0, 4.0],
            kind: 6,
        };
        assert_eq!(find_nearby_high_bridge([0.0, 0.0, 0.0], false, &[near]), None);
        assert_eq!(
            find_nearby_high_bridge([0.0, 0.0, 0.0], true, &[far, wrong_kind]),
            None
        );
        assert_eq!(
            find_nearby_high_bridge([0.0, 0.0, 0.0], true, &[near, far]),
            Some(0)
        );
        // Reverse scan: the later slice entry wins over the earlier one.
        assert_eq!(
            find_nearby_high_bridge([0.0, 0.0, 0.0], true, &[near, near]),
            Some(1)
        );
        // Squared distance is strict: exactly 100 units out is no match.
        let edge = BridgeCandidate {
            position: [100.0, 0.0, 0.0],
            kind: 3,
        };
        assert_eq!(find_nearby_high_bridge([0.0, 0.0, 0.0], true, &[edge]), None);
    }
    #[test]
    // GAME 0x57dd00 (RVA 0x17dd00) C_human::ChangeAnim (mafia-re 113b5b6
    // game/actors/C_human.cpp:5509-5537): case-insensitive table lookup,
    // swap-remove of the matching slot, optional replacement staging, and the
    // animation id zeroed in every arm.
    fn change_anim_rva_17dd00_replaces_case_insensitive_slot() {
        let table = [(5, "walk1.i3d"), (6, "run1.i3d")];
        let mut slots = vec![
            MoveAnimSlot {
                id: 5,
                name: "old".to_string(),
            },
            MoveAnimSlot {
                id: 7,
                name: "keep".to_string(),
            },
        ];
        let mut anim_id = 42;
        replace_move_anim(&table, &mut slots, "WALK1.I3D", Some("walk2.i3d"), &mut anim_id);
        assert_eq!(anim_id, 0);
        assert_eq!(
            slots,
            vec![
                MoveAnimSlot {
                    id: 7,
                    name: "keep".to_string(),
                },
                MoveAnimSlot {
                    id: 5,
                    name: "walk2.i3d".to_string(),
                },
            ]
        );
        // Unknown name: no slot touched, id still zeroed.
        replace_move_anim(&table, &mut slots, "nope.i3d", Some("x.i3d"), &mut anim_id);
        assert_eq!(slots.len(), 2);
        // No replacement: the slot is removed without a push.
        let mut anim_id = 9;
        replace_move_anim(&table, &mut slots, "run1.i3d", None, &mut anim_id);
        assert_eq!(anim_id, 0);
        assert_eq!(
            slots,
            vec![
                MoveAnimSlot {
                    id: 7,
                    name: "keep".to_string(),
                },
                MoveAnimSlot {
                    id: 5,
                    name: "walk2.i3d".to_string(),
                },
            ]
        );
    }
    #[test]
    // GAME 0x57e980 (RVA 0x17e980) C_human::RepairWeaponPosition (mafia-re
    // 113b5b6 game/actors/C_human.cpp:5719-5731): the guard chain order is
    // shells, then the fire-table gate, then the displaced latch.
    fn repair_gate_rva_17e980_orders_shells_fire_and_latch() {
        assert_eq!(
            repair_weapon_position_gate(true, true, true),
            RepairWeaponGate::LoadingShells
        );
        assert_eq!(
            repair_weapon_position_gate(false, false, true),
            RepairWeaponGate::FireBusy
        );
        assert_eq!(
            repair_weapon_position_gate(false, true, false),
            RepairWeaponGate::Settled
        );
        assert_eq!(
            repair_weapon_position_gate(false, true, true),
            RepairWeaponGate::Proceed
        );
    }
    #[test]
    // GAME 0x585c60 (RVA 0x185c60) C_human::Do_Holster (mafia-re 113b5b6
    // game/actors/C_human.cpp:7557-7561): a carried box or dead body diverts
    // to the box drop first; free hands run the inventory program.
    fn holster_rva_185c60_diverts_only_while_carrying() {
        let mut human = HumanState::construct(seed());
        assert!(!human.holster_requires_carry_drop());
        human.carrying_box = true;
        assert!(human.holster_requires_carry_drop());
        human.carrying_box = false;
        human.carrying_dead_body = true;
        assert!(human.holster_requires_carry_drop());
    }
    #[test]
    // GAME 0x585d90 (RVA 0x185d90) C_human::Do_WeaponDrop (mafia-re 113b5b6
    // game/actors/C_human.cpp:7583-7592): the box arm wins over the dead-body
    // arm; free hands run the inventory program.
    fn weapon_drop_rva_185d90_selects_carry_arm_in_order() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.weapon_drop_carry_action(), WeaponDropCarry::Inventory);
        human.carrying_dead_body = true;
        assert_eq!(
            human.weapon_drop_carry_action(),
            WeaponDropCarry::CarryDeadBodyDrop
        );
        human.carrying_box = true;
        assert_eq!(
            human.weapon_drop_carry_action(),
            WeaponDropCarry::CarryBoxDrop
        );
    }
    #[test]
    // GAME 0x56b180 (RVA 0x16b180) C_human::AddToCutscene (mafia-re 113b5b6
    // game/actors/C_human.cpp:42-48): three record kinds of 0x28, 0x2c and
    // 0x38 bytes, verdict true.
    fn add_to_cutscene_rva_16b180_stages_three_kind_sizes() {
        let human = HumanState::construct(seed());
        assert_eq!(human.add_to_cutscene(), ([0x28, 0x2c, 0x38], true));
    }
    #[test]
    // Static C_human::HideLegs (mafia-re 113b5b6
    // game/actors/C_human.h:672-673; working name, no RVA claimed: empty on
    // the PC): hiding or showing touches no state.
    fn hide_legs_working_name_is_empty_noop() {
        HumanState::hide_legs_empty(true);
        HumanState::hide_legs_empty(false);
    }
    #[test]
    // GAME 0x58b690 (RVA 0x18b690) C_human::Do_FuckingBoxDrop (mafia-re 113b5b6
    // game/actors/C_human.cpp:9245-9250): without a box the leaf ends at
    // once; otherwise the staged box-cancel verdict selects the arm.
    fn box_drop_rva_18b690_gates_box_then_cancel() {
        let mut human = HumanState::construct(seed());
        assert_eq!(human.box_drop_after_cancel(false), BoxDropNext::NoBox);
        assert_eq!(human.box_drop_after_cancel(true), BoxDropNext::NoBox);
        human.carrying_box = true;
        assert_eq!(human.box_drop_after_cancel(true), BoxDropNext::Cancelled);
        assert_eq!(
            human.box_drop_after_cancel(false),
            BoxDropNext::StageSceneDrop
        );
    }
}

/// Original track-playing action value (mafia-re 113b5b6
/// game/actors/C_human.h:140 HUMAN_ACTION_TRACK 4): written by
/// C_human::Do_PlayAnim when the staged animation has a track.
pub const HUMAN_ACTION_TRACK: i32 = 4;
/// Original car actor kind (mafia-re 113b5b6 game/actors/C_actor.h:21
/// ACTOR_CAR 4): read by the Do_ClimbInCarLR seat gate.
pub const ACTOR_CAR: u32 = 4;
/// Original pistol weapon kind (mafia-re 113b5b6 game/g_material.h:171
/// WEAPON_KIND_PISTOL 1): admitted by the Do_Reload gate.
pub const WEAPON_KIND_PISTOL: u32 = 1;
/// Original submachine-gun weapon kind (mafia-re 113b5b6
/// game/g_material.h:173 WEAPON_KIND_SMG 3, the Thompson and the shotguns):
/// upper bound admitted by the Do_Reload gate.
pub const WEAPON_KIND_SMG: u32 = 3;
/// Original thrown-weapon kind (mafia-re 113b5b6 game/g_material.h:174
/// WEAPON_KIND_THROWN 4, the Molotov cocktail and the grenade): the
/// Do_ThrowGrenade/Do_Shoot thrown path.
pub const WEAPON_KIND_THROWN: u32 = 4;
/// Original knuckleduster kind (mafia-re 113b5b6 game/g_material.h:175
/// WEAPON_KIND_KNUCKLEDUSTER 5, boxes, as the fists): admitted by the
/// Do_Shoot melee arm.
pub const WEAPON_KIND_KNUCKLEDUSTER: u32 = 5;
/// Original knife kind (mafia-re 113b5b6 game/g_material.h:176
/// WEAPON_KIND_KNIFE 6, the knife and the bottle): admitted by the Do_Shoot
/// melee arm.
pub const WEAPON_KIND_KNIFE: u32 = 6;
/// Original bat kind (mafia-re 113b5b6 game/g_material.h:177
/// WEAPON_KIND_BAT 7, the bat, the bar, the crowbar, the board): admitted by
/// the Do_Shoot melee arm.
pub const WEAPON_KIND_BAT: u32 = 7;
/// Original fire-action end marker (mafia-re 113b5b6 game/g_material.h:158
/// FIRE_ACTION_END 3): the Do_Reload gate only runs at the end step.
pub const FIRE_ACTION_END: i32 = 3;
/// Original fire-action reload marker (mafia-re 113b5b6 game/g_material.h:163
/// FIRE_ACTION_RELOAD 1): the Do_Reload scan target.
pub const FIRE_ACTION_RELOAD: i32 = 1;
/// Original pump-shotgun game-item id (mafia-re 113b5b6 game/g_material.h:129
/// GAMEITEM_SHOTGUN 11): reloads a shell at a time.
pub const GAMEITEM_SHOTGUN: i32 = 11;
/// Original Molotov cocktail game-item id (mafia-re 113b5b6
/// game/g_material.h:123 GAMEITEM_MOLOTOV 5): admitted by Do_ThrowGrenade.
pub const GAMEITEM_MOLOTOV: i32 = 5;
/// Original grenade game-item id (mafia-re 113b5b6 game/g_material.h:133
/// GAMEITEM_GRENADE 15): admitted by Do_ThrowGrenade.
pub const GAMEITEM_GRENADE: i32 = 15;
/// Original take-out/put-away weapon program (mafia-re 113b5b6
/// game/actors/C_human.h:316 HUMAN_WEAPON_ON_OFF 0).
pub const HUMAN_WEAPON_ON_OFF: i32 = 0;
/// Original jump-from-stance animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:623 ANIM_JUMP1 0x5f).
pub const ANIM_JUMP1: i32 = 0x5f;
/// Original running-jump animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:625 ANIM_JUMPSTAT 0x61).
pub const ANIM_JUMPSTAT: i32 = 0x61;
/// Original left side-jump animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:626 ANIM_JUMPL1 0x62, "jumpL1.i3d"). Selected by the
/// Do_Jump left-strafe arm without a rifle or SMG; the animation owner stays
/// DEFERRED.
pub const ANIM_JUMPL1: i32 = 0x62;
/// Original right side-jump animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:627 ANIM_JUMPR1 0x63, "jumpR1.i3d"). Selected by the
/// Do_Jump right-strafe arm without a rifle or SMG; the animation owner stays
/// DEFERRED.
pub const ANIM_JUMPR1: i32 = 0x63;
/// Original left side-jump animation id with a rifle or SMG (mafia-re 113b5b6
/// game/actors/C_actor.h:628 ANIM_JUMPL3 0x64, "jumpL3.i3d"). Selected by the
/// Do_Jump left-strafe arm with a rifle or SMG; the animation owner stays
/// DEFERRED.
pub const ANIM_JUMPL3: i32 = 0x64;
/// Original right side-jump animation id with a rifle or SMG (mafia-re 113b5b6
/// game/actors/C_actor.h:629 ANIM_JUMPR3 0x65, "jumpR3.i3d"). Selected by the
/// Do_Jump right-strafe arm with a rifle or SMG; the animation owner stays
/// DEFERRED.
pub const ANIM_JUMPR3: i32 = 0x65;
/// Original low-climb animation id (mafia-re 113b5b6 game/actors/C_actor.h:632
/// ANIM_NA_BEDNU_06M 0x68, "na bednu 06m.i3d"). Selected by the Do_Climb
/// low-wall arm below 0.8 m; the animation owner stays DEFERRED.
pub const ANIM_NA_BEDNU_06M: i32 = 0x68;
/// Original high-climb animation id (mafia-re 113b5b6 game/actors/C_actor.h:633
/// ANIM_NA_BEDNU_1M 0x69, "na bednu 1m.i3d"). Selected by the Do_Climb
/// high-wall arm at or above 0.8 m; the animation owner stays DEFERRED.
pub const ANIM_NA_BEDNU_1M: i32 = 0x69;
/// Original overhand-throw animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:656 ANIM_HOD_VRCHEM_HOD 0xa3): played by
/// Do_ThrowGrenade through the animation owner.
pub const ANIM_HOD_VRCHEM_HOD: i32 = 0xa3;
/// Original side-jump animation ids (mafia-re 113b5b6
/// game/actors/C_actor.h:749-753): Do_SideJump reads the 0x142..=0x149
/// busy window and writes one of the four bases below.
pub const ANIM_USKOK_RIGHT_FRONT: i32 = 0x142;
/// Original side-jump animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:750 ANIM_USKOK_LEFT_BACK 0x144).
pub const ANIM_USKOK_LEFT_BACK: i32 = 0x144;
/// Original side-jump animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:751 ANIM_USKOK_FRONT_LEFT 0x146).
pub const ANIM_USKOK_FRONT_LEFT: i32 = 0x146;
/// Original side-jump animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:752 ANIM_USKOK_BACK_RIGHT 0x148).
pub const ANIM_USKOK_BACK_RIGHT: i32 = 0x148;
/// Original side-jump animation upper bound (mafia-re 113b5b6
/// game/actors/C_actor.h:753 ANIM_USKOK_FRONT_RIGHT 0x149).
pub const ANIM_USKOK_FRONT_RIGHT: i32 = 0x149;
/// Original fists direct-strike base animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:665 ANIM_BOJ_BOX_L_DIRECT 0xb9,
/// "boj box L direct.i3d"). The Do_Shoot fists STAND arm adds the staged
/// strike index to this base; the animation owner stays DEFERRED.
pub const ANIM_BOJ_BOX_L_DIRECT: i32 = 0xb9;
/// Original knife direct-strike base animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:662 ANIM_BOJ_NUZ_BOD 0xb2, "boj nuz bod.i3d").
/// The Do_Shoot knife STAND arm adds the staged strike index to this base;
/// the animation owner stays DEFERRED.
pub const ANIM_BOJ_NUZ_BOD: i32 = 0xb2;
/// Original bat strike base animation id (mafia-re 113b5b6
/// game/actors/C_actor.h:659 ANIM_BOJ_BASB_Z_LH 0xa9,
/// "boj basb z LH.i3d"). The Do_Shoot bat STAND arm adds the staged strike
/// index to this base; the animation owner stays DEFERRED.
pub const ANIM_BOJ_BASB_Z_LH: i32 = 0xa9;

/// Entry verdict of the C_human::Do_ChangeWeapon latch (mafia-re 113b5b6
/// game/actors/C_human.cpp:5990-5994 and :6074-6076).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChangeWeaponEntry {
    AlreadyChanging,
    StartChange,
    DirectModel,
}

/// GAME 0x57f550 (RVA 0x17f550) C_human::Do_ChangeWeapon entry latch
/// (mafia-re 113b5b6 game/actors/C_human.cpp:5987-6076): in a car the model
/// just changes; on foot a running change returns at once, otherwise the
/// changing latch is claimed. The weapon-model visibility, ResetWeaponEffects,
/// DelExtendedAnim, idle/breath timers, fire-table select, animation-cache
/// program, skip-frames path and the camera/sound program past the latch stay
/// DEFERRED with the weapon/animation/scene/sound owners.
pub fn change_weapon_entry(car_present: bool, changing_weapon: bool) -> ChangeWeaponEntry {
    if car_present {
        ChangeWeaponEntry::DirectModel
    } else if changing_weapon {
        ChangeWeaponEntry::AlreadyChanging
    } else {
        ChangeWeaponEntry::StartChange
    }
}

/// Reload-table cursor selected by C_human::Do_Reload (mafia-re 113b5b6
/// game/actors/C_human.cpp:7528-7538): the fire step to resume from and the
/// step time carried with it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReloadFirePlan {
    pub step: usize,
    pub time: i32,
    pub shell_by_shell: bool,
}

/// GAME 0x585b40 (RVA 0x185b40) C_human::Do_Reload fire-table scan
/// (mafia-re 113b5b6 game/actors/C_human.cpp:7515-7551): carrying refuses,
/// the fire must sit at its end step, the weapon kind must be pistol through
/// SMG with spare ammunition, then the staged right-hand fire table
/// (`(action, time)` pairs) is scanned forward to the reload step and stepped
/// one back with its time. `shell_by_shell_staged` carries the
/// shotgun-plus-MuzeNabitJeden verdict because the inventory/left-hand-target
/// owners stay DEFERRED; the fire-table swap, forced/effects latches and the
/// pump-shotgun hand release are reported through the returned plan.
pub fn reload_fire_plan(
    carrying_box: bool,
    carrying_dead_body: bool,
    at_end: bool,
    weapon_kind: u32,
    spare: i32,
    fire_table: &[(i32, i32)],
    shell_by_shell_staged: bool,
) -> Option<ReloadFirePlan> {
    if carrying_box || carrying_dead_body {
        return None;
    }
    if !at_end {
        return None;
    }
    if weapon_kind <= WEAPON_KIND_FISTS || weapon_kind > WEAPON_KIND_SMG {
        return None;
    }
    if spare <= 0 {
        return None;
    }
    let mut step = 0usize;
    while step < fire_table.len()
        && fire_table[step].0 != FIRE_ACTION_END
        && fire_table[step].0 != FIRE_ACTION_RELOAD
    {
        step += 1;
    }
    if step >= fire_table.len() {
        return None;
    }
    let mut time = 0i32;
    if fire_table[step].0 == FIRE_ACTION_RELOAD && step > 0 {
        step -= 1;
        time = fire_table[step].1;
    }
    Some(ReloadFirePlan {
        step,
        time,
        shell_by_shell: shell_by_shell_staged,
    })
}

/// Original throw-away weapon program (mafia-re 113b5b6
/// game/actors/C_human.h:317 HUMAN_WEAPON_THROW_AWAY 1): drops the weapon
/// instead of holstering it.
pub const HUMAN_WEAPON_THROW_AWAY: i32 = 1;

/// Working-name animation-name select of C_human::Do_ChangeWeapon (no RVA
/// claimed: inline switch at mafia-re 113b5b6
/// game/actors/C_human.cpp:6021-6040 inside Do_ChangeWeapon 0x57f550, with no
/// standalone FUNCTION copy). Returns the staged animation-cache name for the
/// take-out/throw-away program, or None where the program just changes the
/// model. The animation-cache/model owners stay DEFERRED.
pub fn change_weapon_anim_name(how: i32, crouching: bool) -> Option<&'static str> {
    match how {
        HUMAN_WEAPON_ON_OFF => {
            if crouching {
                Some("gun drep on off.i3d")
            } else {
                Some("gun stoj on off.i3d")
            }
        }
        HUMAN_WEAPON_THROW_AWAY => {
            if crouching {
                Some("gun drep zahozeni.i3d")
            } else {
                Some("gun stoj zahozeni.i3d")
            }
        }
        _ => None,
    }
}

/// Deterministic latch words staged by the C_human::Do_ChangeWeapon reset
/// core (mafia-re 113b5b6 game/actors/C_human.cpp:5992-6013).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeWeaponLatchReset {
    pub changing: bool,
    pub aimed: bool,
    pub fire_held: bool,
    pub throw_swung: bool,
    pub reloading: bool,
    pub reload_forced: bool,
    pub loading_shells: bool,
    pub reload_effects: bool,
    pub strike_anim_time: i32,
    pub fire_step: i32,
    pub fire_time: i32,
}

/// Working-name latch reset of C_human::Do_ChangeWeapon (no RVA claimed:
/// inline clears at mafia-re 113b5b6 game/actors/C_human.cpp:5992-6013 inside
/// Do_ChangeWeapon 0x57f550, with no standalone FUNCTION copy). Reports the
/// deterministic words the core stages on foot; the random idle time, the
/// weapon-model visibility, ResetWeaponEffects, DelExtendedAnim, fire-table
/// select, animation-cache program, skip-frames path and camera/sound program
/// stay DEFERRED with their owners.
pub fn change_weapon_latch_reset() -> ChangeWeaponLatchReset {
    ChangeWeaponLatchReset {
        changing: true,
        aimed: false,
        fire_held: false,
        throw_swung: false,
        reloading: false,
        reload_forced: false,
        loading_shells: false,
        reload_effects: false,
        strike_anim_time: -1,
        fire_step: 0,
        fire_time: 0,
    }
}

/// Latch words staged by the C_human::Do_Reload tail (mafia-re 113b5b6
/// game/actors/C_human.cpp:7539-7549).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReloadLatchWords {
    pub reload_forced: bool,
    pub reload_effects: bool,
    pub loading_shells: bool,
    pub weapon_displaced: bool,
}

/// Working-name latch select of C_human::Do_Reload (no RVA claimed: inline
/// writes at mafia-re 113b5b6 game/actors/C_human.cpp:7539-7549 inside
/// Do_Reload 0x585b40, with no standalone FUNCTION copy). The forced/effects
/// latches are always claimed; the shell-by-shell and displacement latches
/// follow the staged shotgun verdict (`is_shotgun && can_load_one`) and the
/// staged left-hand-target hit. The inventory/model owners stay DEFERRED.
pub fn reload_latch_select(
    is_shotgun: bool,
    can_load_one: bool,
    left_hand_present: bool,
) -> ReloadLatchWords {
    let shell = is_shotgun && can_load_one;
    ReloadLatchWords {
        reload_forced: true,
        reload_effects: true,
        loading_shells: shell,
        weapon_displaced: shell && left_hand_present,
    }
}

/// Working-name invulnerability-cheat refusal shared by C_human::Do_Castrate
/// and C_human::Do_HardHead (no RVA claimed: inline gates at mafia-re 113b5b6
/// game/actors/C_human.cpp:7221-7227 and :7244-7250 inside Do_Castrate
/// 0x584480 and Do_HardHead 0x584550, with no standalone FUNCTION copy; only
/// the strike bodies carry those addresses). While the staged
/// invulnerability-cheat word is set, a player or friend target is spared.
/// The game/player/type owners stay DEFERRED.
pub fn strike_cheat_refusal(
    cheats_invulnerable: bool,
    target_is_player: bool,
    target_is_friend: bool,
) -> bool {
    cheats_invulnerable && (target_is_player || target_is_friend)
}

/// Working-name fire-table swap of C_human::Do_ChangeWeapon (no RVA claimed:
/// inline swap at mafia-re 113b5b6 game/actors/C_human.cpp:6017-6019 inside
/// Do_ChangeWeapon 0x57f550, with no standalone FUNCTION copy; only the
/// entry latch above carries that address). The caller runs this on the
/// admitted on-foot path while the game runs: the table swaps to the
/// take-out/put-away program with a zeroed step, and the step time clears.
/// The game/animation owners stay DEFERRED.
pub fn change_weapon_fire_select() -> (FireActionTable, i32) {
    (FireActionTable::OnOff, 0)
}

/// Working-name strike-index advance of C_human::Do_Shoot (no RVA claimed:
/// inline update at mafia-re 113b5b6 game/actors/C_human.cpp:7015-7022
/// inside Do_Shoot 0x583590, with no standalone FUNCTION copy). Advances the
/// staged series while strikes arrive within 1200 ms, restarting at zero
/// past the gap and wrapping past index 3. The game clock arrives staged as
/// `dt_ms`; the weapon/animation owners stay DEFERRED.
pub fn shoot_strike_index_next(prev_index: i32, dt_ms: u32) -> i32 {
    let next = if dt_ms < 1200 { prev_index + 1 } else { 0 };
    if next > 3 { 0 } else { next }
}

/// Working-name backswing-start gate of C_human::Do_Shoot (no RVA claimed:
/// inline gate at mafia-re 113b5b6 game/actors/C_human.cpp:7025-7037 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). Starts the bat
/// backswing only past 200 ms with a bat in hand and no backswing yet. The
/// game clock arrives staged as `dt_since_strike_ms`; the animation owner
/// stays DEFERRED.
pub fn shoot_backswing_gate(is_bat: bool, backswing: bool, dt_since_strike_ms: u32) -> bool {
    dt_since_strike_ms > 200 && is_bat && !backswing
}

/// Working-name throw-hold clamp of C_human::Do_Shoot (no RVA claimed: inline
/// window at mafia-re 113b5b6 game/actors/C_human.cpp:7053-7056 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). Releases the throw
/// past 400 ms, clamping the held time at 2000 ms for the force scale. The
/// strength scale, direction normalize and animation owners stay DEFERRED;
/// the game clock arrives staged as `dt_ms`.
pub fn throw_hold_clamp(dt_ms: u32) -> Option<u32> {
    if dt_ms > 400 {
        Some(dt_ms.min(2000))
    } else {
        None
    }
}

/// State words staged by the C_human::Do_Shoot gun press arm (mafia-re 113b5b6
/// game/actors/C_human.cpp:6973-6987).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShootGunPressWords {
    /// The held-shot latch always claims on the press.
    pub fire_held: bool,
    /// The aim arm taken when not aimed yet (the aimed fire table plus its
    /// zeroed step); None falls through to the deferred Shooting program.
    pub aim_table: Option<FireActionTable>,
    /// The zeroed fire step carried with the aim arm; None leaves it alone.
    pub fire_step: Option<i32>,
    /// The latched throw direction when the caller staged one.
    pub throw_dir: Option<[f32; 3]>,
}

/// Working-name gun-arm words of C_human::Do_Shoot (no RVA claimed: inline
/// writes at mafia-re 113b5b6 game/actors/C_human.cpp:6979-6986 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). The changing-weapon
/// handoff, the extended-anim clear, the throw timing, the aim-time stamp
/// and the Shooting program stay DEFERRED with the weapon/animation owners.
pub fn shoot_press_gun_words(aimed: bool, dir: Option<[f32; 3]>) -> ShootGunPressWords {
    ShootGunPressWords {
        fire_held: true,
        aim_table: if aimed {
            None
        } else {
            Some(FireActionTable::Aimed)
        },
        fire_step: if aimed { None } else { Some(0) },
        throw_dir: dir,
    }
}

/// State words staged by the C_human::Do_Shoot thrown-weapon press arm
/// (mafia-re 113b5b6 game/actors/C_human.cpp:6990-7003).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShootThrownPress {
    /// The swing start time latched on the first press; None keeps the old
    /// stamp on a held press.
    pub throw_time: Option<u32>,
    /// The throw always counts as swung after the press.
    pub throw_swung: bool,
}

/// Working-name thrown-arm words of C_human::Do_Shoot (no RVA claimed:
/// inline writes at mafia-re 113b5b6 game/actors/C_human.cpp:6991-7003
/// inside Do_Shoot 0x583590, with no standalone FUNCTION copy). The
/// extended-anim throw program stays DEFERRED with the animation owner;
/// `game_time` is the staged clock.
pub fn shoot_press_thrown_words(throw_swung: bool, game_time: u32) -> ShootThrownPress {
    ShootThrownPress {
        throw_time: if throw_swung { None } else { Some(game_time) },
        throw_swung: true,
    }
}

/// Release program of the C_human::Do_Shoot thrown-weapon arm (mafia-re
/// 113b5b6 game/actors/C_human.cpp:7048-7066).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShootThrownRelease {
    /// Nothing is swung, the release is a no-op.
    Idle,
    /// A swung throw released too early: only the swing clears.
    CancelSwing,
    /// A swung throw released in time, with the clamped hold for the force.
    ThrowStrike { clamped_dt_ms: u32 },
}

/// Working-name release program of C_human::Do_Shoot (no RVA claimed:
/// inline branch at mafia-re 113b5b6 game/actors/C_human.cpp:7049-7065
/// inside Do_Shoot 0x583590, with no standalone FUNCTION copy). Reuses the
/// [`throw_hold_clamp`] window (past 400 ms, clamped at 2000 ms). The thief
/// indicator clear, the direction normalize, the strength force scale, the
/// HOD animation select and the game-time stamp stay DEFERRED with the
/// indicator/animation owners.
pub fn shoot_release_thrown_program(throw_swung: bool, dt_ms: u32) -> ShootThrownRelease {
    if !throw_swung {
        return ShootThrownRelease::Idle;
    }
    match throw_hold_clamp(dt_ms) {
        None => ShootThrownRelease::CancelSwing,
        Some(clamped_dt_ms) => ShootThrownRelease::ThrowStrike { clamped_dt_ms },
    }
}

/// Working-name force magnitude of the C_human::Do_Shoot thrown release (no
/// RVA claimed: inline scale at mafia-re 113b5b6 game/actors/C_human.cpp
/// :7054-7058 inside Do_Shoot 0x583590, with no standalone FUNCTION copy).
/// `clamped_dt_ms` is the staged [`throw_hold_clamp`] output and `strength`
/// the staged HUMAN_PROPERTY_STRENGTH word; the direction normalize stays
/// with the vector owner.
pub fn throw_force_scale(clamped_dt_ms: u32, strength: f32) -> f32 {
    clamped_dt_ms as f32 * 0.015 * strength
}

/// Working-name kind test of the C_human::Do_Shoot press (no RVA claimed:
/// inline case list at mafia-re 113b5b6 game/actors/C_human.cpp:7006-7009
/// inside Do_Shoot 0x583590, with no standalone FUNCTION copy; only the
/// [`shoot_entry_gate`] gate carries that address). True for the four melee
/// kinds (fists, knuckleduster, knife, bat) whose press runs the strike
/// program.
pub fn shoot_is_melee_kind(weapon_kind: u32) -> bool {
    matches!(
        weapon_kind,
        WEAPON_KIND_FISTS | WEAPON_KIND_KNUCKLEDUSTER | WEAPON_KIND_KNIFE | WEAPON_KIND_BAT
    )
}

/// Working-name release verdict of C_human::Do_Shoot (no RVA claimed: inline
/// switch at mafia-re 113b5b6 game/actors/C_human.cpp:7043-7159 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy; only the
/// [`shoot_entry_gate`] gate carries that address). Every release arm clears
/// the held-shot latch except the thrown arm, which runs its own swing
/// program instead.
pub fn shoot_release_clears_fire(weapon_kind: u32) -> bool {
    weapon_kind != WEAPON_KIND_THROWN
}

/// Working-name model-change verdict of C_human::Do_Holster (no RVA claimed:
/// inline compare at mafia-re 113b5b6 game/actors/C_human.cpp:7563-7565
/// inside Do_Holster 0x585c60, with no standalone FUNCTION copy). Reports
/// whether the staged right-hand item id changed across the Holster call.
/// The inventory/ChangeWeaponModel/DropOutItems owners stay DEFERRED.
pub fn holster_needs_model_change(prev_id: u32, next_id: u32) -> bool {
    prev_id != next_id
}

/// Model step selected by C_human::Do_Holster after an item change (mafia-re
/// 113b5b6 game/actors/C_human.cpp:7565-7574).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HolsterModelStep {
    /// The right-hand item is unchanged: no model program runs.
    Idle,
    /// Skip-frames are pending: one is consumed and the model changes at
    /// once.
    SkipDecrementAndModel,
    /// No skip-frames: the take-out/put-away change-weapon program runs; its
    /// hide flag rides [`HolsterModelProgram::drop_out`].
    ChangeWeapon,
}

/// Model program plus drop verdict of C_human::Do_Holster (mafia-re 113b5b6
/// game/actors/C_human.cpp:7563-7576).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HolsterModelProgram {
    /// The model step for a changed right-hand item; Idle without a change.
    pub model: HolsterModelStep,
    /// Dropped items leave the hands through DropOutItems.
    pub drop_out: bool,
}

/// Working-name model program of C_human::Do_Holster (no RVA claimed: inline
/// branch at mafia-re 113b5b6 game/actors/C_human.cpp:7565-7576 inside
/// Do_Holster 0x585c60, with no standalone FUNCTION copy). `skip_frames` is
/// the staged mission skip counter; the Holster inventory call, the
/// ChangeWeaponModel/ChangeWeapon/DropOutItems effects and the thief
/// indicator clear stay DEFERRED with their owners.
pub fn holster_model_program(
    id_changed: bool,
    skip_frames: u32,
    dropped_any: bool,
) -> HolsterModelProgram {
    HolsterModelProgram {
        model: if !id_changed {
            HolsterModelStep::Idle
        } else if skip_frames > 0 {
            HolsterModelStep::SkipDecrementAndModel
        } else {
            HolsterModelStep::ChangeWeapon
        },
        drop_out: dropped_any,
    }
}

/// Working-name entry guard of C_human::DropOutItems (no RVA claimed: inline
/// guard at mafia-re 113b5b6 game/actors/C_human.cpp:6148-6151 inside
/// DropOutItems 0x57faa0, with no standalone FUNCTION copy). Requires both
/// staged hand frames and at least one staged item; the frame/game-drop
/// owners stay DEFERRED.
pub fn drop_out_items_gate(
    has_right_hand: bool,
    has_left_hand: bool,
    item_count: usize,
) -> bool {
    has_right_hand && has_left_hand && item_count > 0
}

/// Working-name throw-away verdict of C_human::Do_WeaponDrop (no RVA claimed:
/// inline chain at mafia-re 113b5b6 game/actors/C_human.cpp:7593-7604 inside
/// Do_WeaponDrop 0x585d90, with no standalone FUNCTION copy). The right-hand
/// item drops through the deferred Remove/DropOutItems owners; only when the
/// staged drop succeeds does the throw-away change-weapon program run.
/// `item_present` stages the right-hand item hit, `drop_succeeded` the
/// DropOutItems verdict.
pub fn weapon_drop_throw_away(item_present: bool, drop_succeeded: bool) -> bool {
    item_present && drop_succeeded
}

/// State words cleared by C_human::Do_PlayAnim (mafia-re 113b5b6
/// game/actors/C_human.cpp:7612-7619).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayAnimReset {
    pub idle_time: i32,
    pub breath_time: i32,
    pub velocity: [f32; 3],
    pub velocity_ramp: f32,
    pub test_ground: bool,
    pub fall: f32,
    pub throw_swung: bool,
}

/// Outcome of the C_human::Do_PlayAnim leaf: the cleared words, the track
/// action (Some(HUMAN_ACTION_TRACK) while the staged SetAnim track is live,
/// None when the action word is left untouched) and the staged active-anim
/// length passthrough.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayAnimOutcome {
    pub reset: PlayAnimReset,
    pub action: Option<i32>,
    pub length: u32,
}

/// GAME 0x585e80 (RVA 0x185e80) C_human::Do_PlayAnim reset leaf (mafia-re
/// 113b5b6 game/actors/C_human.cpp:7609-7627): clears the idle/breath timers,
/// velocity, ramp, ground test, fall and throw-swing words, then selects the
/// track action from the staged SetAnim track and passes the staged active
/// length through. The indicator clear, SetAnim call and PersonAnim handoff
/// stay DEFERRED with the indicator/animation owners; `track` and `length`
/// are the values those owners produce.
pub fn do_play_anim_leaf(track: i32, length: u32) -> PlayAnimOutcome {
    PlayAnimOutcome {
        reset: PlayAnimReset {
            idle_time: -1,
            breath_time: -1,
            velocity: [0.0, 0.0, 0.0],
            velocity_ramp: 0.0,
            test_ground: false,
            fall: 0.0,
            throw_swung: false,
        },
        action: if track != -1 {
            Some(HUMAN_ACTION_TRACK)
        } else {
            None
        },
        length,
    }
}

/// GAME 0x586190 (RVA 0x186190) C_human::Do_Jump entry gate (mafia-re 113b5b6
/// game/actors/C_human.cpp:7644-7648): a carried box, a carried body or a
/// car seat refuses at once. The run/stance/strafe anim-id select, the
/// Do_Climb fallback, the PersonAnim/DelExtendedAnim/ResetWeaponEffects
/// program and the force-feedback/indicator effects stay DEFERRED.
pub fn jump_entry_gate(carrying_box: bool, carrying_dead_body: bool, in_car: bool) -> bool {
    !carrying_box && !carrying_dead_body && !in_car
}

/// Jump program selected by C_human::Do_Jump (mafia-re 113b5b6
/// game/actors/C_human.cpp:7650-7710).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum JumpAnimSelect {
    /// No arm matches the masked id, or crouching blocks the run/stance arm.
    NoJump,
    /// Run arm: the caller attempts Do_Climb, then the JUMPSTAT program.
    RunClimbAttempt,
    /// Stance/walk arm: the caller attempts Do_Climb, then the JUMP1 program.
    StanceClimbAttempt,
    /// Left side-step arm with its animation and force-feedback direction.
    LeftStrafe { anim_id: i32, ff_direction: f32 },
    /// Right side-step arm with its animation and force-feedback direction.
    RightStrafe { anim_id: i32, ff_direction: f32 },
}

/// Working-name arm select of C_human::Do_Jump (no RVA claimed: inline
/// branch at mafia-re 113b5b6 game/actors/C_human.cpp:7650-7710 inside
/// Do_Jump 0x586190, with no standalone FUNCTION copy; only the entry gate
/// above carries that address). `masked_anim_id` is already masked with
/// ANIM_BASE, `weapon_kind` is the staged WEAPON_KIND_* word. The Do_Climb
/// fallback, the PersonAnim/SetWaitAnim/DelExtendedAnim handoffs, the
/// velocity write and the player force-feedback/indicator effects stay
/// DEFERRED with the collision/animation/device owners; `ff_direction`
/// stages the shake direction (0.0 run/stance, PI/2 left, 3PI/2 right) for
/// the deferred effect.
pub fn jump_anim_select(
    masked_anim_id: i32,
    weapon_kind: u32,
    crouching: bool,
) -> JumpAnimSelect {
    use core::f32::consts::PI;
    if masked_anim_id == ANIM_RUN1 {
        if crouching {
            return JumpAnimSelect::NoJump;
        }
        return JumpAnimSelect::RunClimbAttempt;
    }
    if (ANIM_BREATH01A..=ANIM_BREATH01D).contains(&masked_anim_id)
        || masked_anim_id == ANIM_WALK1
        || masked_anim_id == ANIM_LEFT1
        || masked_anim_id == ANIM_RIGHT1
    {
        if crouching {
            return JumpAnimSelect::NoJump;
        }
        return JumpAnimSelect::StanceClimbAttempt;
    }
    if masked_anim_id == ANIM_STRAFL1 || masked_anim_id == ANIM_STRAFRL1 {
        let anim_id = if weapon_kind == WEAPON_KIND_RIFLE || weapon_kind == WEAPON_KIND_SMG {
            ANIM_JUMPL3
        } else {
            ANIM_JUMPL1
        };
        return JumpAnimSelect::LeftStrafe {
            anim_id,
            ff_direction: PI / 2.0,
        };
    }
    if masked_anim_id == ANIM_STRAFR1 || masked_anim_id == ANIM_STRAFRR1 {
        let anim_id = if weapon_kind == WEAPON_KIND_RIFLE || weapon_kind == WEAPON_KIND_SMG {
            ANIM_JUMPR3
        } else {
            ANIM_JUMPR1
        };
        return JumpAnimSelect::RightStrafe {
            anim_id,
            ff_direction: PI * 3.0 / 2.0,
        };
    }
    JumpAnimSelect::NoJump
}

/// State words staged when C_human::Do_Jump takes a new jump animation
/// (mafia-re 113b5b6 game/actors/C_human.cpp:7712-7719).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JumpChangedWords {
    /// A new jump never counts as thrown.
    pub throw_swung: bool,
    /// The idle timer restarts.
    pub idle_time: i32,
    /// The breath timer restarts.
    pub breath_time: i32,
    /// The reload-effects latch clears with the weapon reset.
    pub reload_effects: bool,
}

/// Working-name change latch of C_human::Do_Jump (no RVA claimed: inline
/// clears at mafia-re 113b5b6 game/actors/C_human.cpp:7712-7719 inside
/// Do_Jump 0x586190, with no standalone FUNCTION copy; only the
/// [`jump_entry_gate`] gate carries that address). The thief indicator
/// clear, the DelExtendedAnim/ResetWeaponEffects/PersonAnim handoffs stay
/// DEFERRED with their owners.
pub fn jump_changed_words() -> JumpChangedWords {
    JumpChangedWords {
        throw_swung: false,
        idle_time: -1,
        breath_time: -1,
        reload_effects: false,
    }
}

/// Run-arm words staged by C_human::Do_Jump (mafia-re 113b5b6
/// game/actors/C_human.cpp:7655-7662).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JumpRunWords {
    /// The left-side flag from the active animation time (`time < length/2`).
    pub anim_left: bool,
    /// The run takeoff upward velocity.
    pub velocity_y: f32,
}

/// Working-name run-arm words of C_human::Do_Jump (no RVA claimed: inline
/// writes at mafia-re 113b5b6 game/actors/C_human.cpp:7655-7662 inside
/// Do_Jump 0x586190, with no standalone FUNCTION copy; only the
/// [`jump_entry_gate`] gate carries that address). The staged times are the
/// GetActiveAnimTime outputs; the Do_Climb fallback, the
/// PersonAnim/SetWaitAnim program and the force-feedback effect stay
/// DEFERRED with their owners.
pub fn jump_run_words(time_ms: u32, length_ms: u32) -> JumpRunWords {
    JumpRunWords {
        anim_left: time_ms < length_ms / 2,
        velocity_y: 6.0,
    }
}

/// GAME 0x586400 (RVA 0x186400) C_human::Do_Climb entry gate (mafia-re 113b5b6
/// game/actors/C_human.cpp:7730-7734): a carried box or a carried body
/// refuses at once. The knee/waist/head wall probes, the top-line descent,
/// the room checks and the climb animation stay DEFERRED with the collision
/// and animation owners.
pub fn climb_entry_gate(carrying_box: bool, carrying_dead_body: bool) -> bool {
    !carrying_box && !carrying_dead_body
}

/// Waiting action-state value staged by the SetWaitAnim select (mafia-re
/// 113b5b6 game/actors/C_human.h:141 HUMAN_ACTION_WAIT 5).
pub const HUMAN_ACTION_WAIT: i32 = 5;

/// Outcome of the inline C_human::SetWaitAnim select (working name, no RVA
/// claimed: mafia-re 113b5b6 game/actors/C_human.h:1158-1172 keeps no
/// standalone copy of this inline body): the wait words staged when the
/// produced animation runs past 250 ms, else no write. The PersonAnim
/// handoff and the GetActiveAnimTime length producer stay DEFERRED with
/// the animation owner; `length` is the staged active length.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaitAnimWords {
    pub action_time: u32,
    pub last_velocity: [f32; 3],
    pub velocity_ramp: f32,
    pub action_state: i32,
}

/// Inline C_human::SetWaitAnim select (working name, no RVA claimed: no
/// standalone copy exists, see WaitAnimWords): stage the wait words past a
/// 250 ms active length, else keep the action word untouched.
pub fn set_wait_anim_select(length: u32) -> Option<WaitAnimWords> {
    if length > 250 {
        Some(WaitAnimWords {
            action_time: length - 250,
            last_velocity: [0.0, 0.0, 0.0],
            velocity_ramp: 0.0,
            action_state: HUMAN_ACTION_WAIT,
        })
    } else {
        None
    }
}

/// Inline C_human::SetFreeBoxingAnim (working name, no RVA claimed: no
/// standalone copy exists, mafia-re 113b5b6 game/actors/C_human.h:606 and
/// game/actors/C_human.cpp:7260-7276) EXPLICIT UNSUPPORTED STUB: the
/// strike-animation lookup and extended-anim handoff need the
/// animation-cache/model owners, so no strike time is staged and no guard
/// latch changes.
pub fn set_free_boxing_anim_unsupported() {}

/// Inline C_human::ShootingInCar (working name, no RVA claimed: no
/// standalone copy exists, mafia-re 113b5b6 game/actors/C_human.h:608 and
/// game/actors/C_human.cpp:7279-7293) EXPLICIT UNSUPPORTED STUB: the
/// seat-side throw-direction select needs the car, frame, shot-frame and
/// collision owners, so no throw direction is staged.
pub fn shooting_in_car_unsupported() {}

/// Inline C_human::TestShotCollision (working name, no RVA claimed: no
/// standalone copy exists, mafia-re 113b5b6 game/actors/C_human.h:683-688)
/// EXPLICIT UNSUPPORTED STUB: the bone-line test needs the shot-skeleton
/// owner, so no hit verdict is staged.
pub fn test_shot_collision_unsupported() {}

/// GAME 0x56f9e0 (RVA 0x16f9e0) C_human::SaveGameGetSize staged sum
/// (mafia-re 113b5b6 game/actors/C_human.cpp:1417-1425): the staged base
/// size plus sizeof(S_human_savegame) (0x17e per
/// game/actors/C_human.h:332-385, packed) plus the staged car and
/// using-car actor-reference sizes (C_actor::Actor_SaveGameGetSize cores)
/// plus the staged inventory size. The car/inventory owners stay DEFERRED;
/// the caller stages the four words.
pub fn human_savegame_size(
    base_size: u32,
    car_ref: u32,
    using_car_ref: u32,
    inventory_size: u32,
) -> u32 {
    const HUMAN_SAVEGAME_WORDS: u32 = 0x17e;
    base_size + HUMAN_SAVEGAME_WORDS + car_ref + using_car_ref + inventory_size
}

/// Signed atan2 difference wrapped into -PI..PI, replicating the game
/// C_actor.cpp AngleTo2 helper (mafia-re 113b5b6 game/actors/C_actor.cpp:1748-1760).
fn angle_to_2d(a: [f32; 2], b: [f32; 2]) -> f32 {
    use core::f32::consts::PI;
    let angle = b[1].atan2(b[0]) - a[1].atan2(a[0]);
    if angle < -PI {
        angle + PI * 2.0
    } else if angle > PI {
        angle - PI * 2.0
    } else {
        angle
    }
}

/// Signed S_vector2::DistanceToLine replica (mafia-re 113b5b6
/// ls3df/I3D/I3d_math.h:579-584): the normal (-dir.y, dir.x) normalized,
/// then the point projection difference. A zero direction yields zero.
fn distance_to_line_2d(point: [f32; 2], line_point: [f32; 2], line_dir: [f32; 2]) -> f32 {
    let mut normal = [-line_dir[1], line_dir[0]];
    let len = (normal[0] * normal[0] + normal[1] * normal[1]).sqrt();
    if len > 1e-8 {
        normal[0] /= len;
        normal[1] /= len;
    }
    (point[0] * normal[0] + point[1] * normal[1])
        - (line_point[0] * normal[0] + line_point[1] * normal[1])
}

/// GAME 0x587040 (RVA 0x187040) C_human::Do_SideJump select (mafia-re 113b5b6
/// game/actors/C_human.cpp:7849-7875): carrying refuses, as does an
/// in-flight side-jump id in 0x142..=0x149; otherwise the 3D angle between
/// the staged facing and target directions selects the back/front pair, the
/// 2D atan2 verdict breaks the tie, and the line side through the staged
/// position bumps the id by one. Frame/collision/animation owners stay
/// DEFERRED; every vector arrives staged.
pub fn side_jump_select(
    dir: [f32; 3],
    dir2: [f32; 3],
    pos: [f32; 3],
    pos2: [f32; 3],
    anim_id: i32,
    carrying_box: bool,
    carrying_dead_body: bool,
) -> Option<i32> {
    use core::f32::consts::PI;
    if carrying_box || carrying_dead_body {
        return None;
    }
    if (ANIM_USKOK_RIGHT_FRONT..=ANIM_USKOK_FRONT_RIGHT).contains(&anim_id) {
        return None;
    }
    let dot = dir[0] * dir2[0] + dir[1] * dir2[1] + dir[2] * dir2[2];
    let mag = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt()
        * (dir2[0] * dir2[0] + dir2[1] * dir2[1] + dir2[2] * dir2[2]).sqrt();
    let angle = if mag < 1e-8 {
        0.0
    } else {
        (dot / mag).clamp(-1.0, 1.0).acos()
    };
    let mut next = if angle < PI / 4.0 {
        ANIM_USKOK_LEFT_BACK
    } else if angle > PI * 3.0 / 4.0 {
        ANIM_USKOK_RIGHT_FRONT
    } else if angle_to_2d([dir[0], dir[2]], [dir2[0], dir2[2]]) > 0.0 {
        ANIM_USKOK_BACK_RIGHT
    } else {
        ANIM_USKOK_FRONT_LEFT
    };
    let side = distance_to_line_2d(
        [pos[0], pos[2]],
        [pos2[0], pos2[2]],
        [dir[0], dir2[2]],
    );
    if side < 0.0 {
        next += 1;
    }
    Some(next)
}

/// GAME 0x581eb0 (RVA 0x181eb0) C_human::Do_ClimbInCarLR seat gate (mafia-re
/// 113b5b6 game/actors/C_human.cpp:6617-6629): without a car, outside a car
/// actor, without a seat, or onto an occupied neighbour seat the leaf
/// refuses; otherwise it yields the paired seat (0<->1, 2<->3). The engine
/// hand-off, indicator clear, seat animation/mixing-point program,
/// PersonAnim handoff and ChangeWeaponModel calls stay DEFERRED with the
/// car/animation owners; `other_occupied` is the staged GetOwner verdict.
pub fn climb_seat_target(
    seat: i32,
    car_present: bool,
    is_car_actor: bool,
    other_occupied: bool,
) -> Option<i32> {
    if !car_present || !is_car_actor || seat < 0 {
        return None;
    }
    let other = if (seat & 1) != 0 { seat - 1 } else { seat + 1 };
    if other_occupied {
        None
    } else {
        Some(other)
    }
}

/// Working-name seat scale of C_human::Do_ClimbInCarLR (no RVA claimed: inline
/// ratio at mafia-re 113b5b6 game/actors/C_human.cpp:6653-6661 inside
/// Do_ClimbInCarLR 0x581eb0, with no standalone FUNCTION copy). `seat_gap_x`
/// is the staged seats' x distance, `mix_x` the staged TCK mixing-point x;
/// a mixing point at most 0.1 wide leaves the scale at one. The seat
/// animation/mixing-point program stays DEFERRED with the animation owner.
pub fn climb_seat_scale(seat_gap_x: f32, mix_x: f32) -> f32 {
    let len = mix_x.abs();
    if len > 0.1 {
        seat_gap_x.abs() / len
    } else {
        1.0
    }
}

/// Working-name reach select of C_human::Do_Climb (no RVA claimed: inline
/// select at mafia-re 113b5b6 game/actors/C_human.cpp:7818-7819 inside
/// Do_Climb 0x586400, with no standalone FUNCTION copy). The track stretch
/// along x doubles the reach only past half a metre; the wall probes stay
/// DEFERRED with the collision owner.
pub fn climb_reach_select(dist: f32) -> f32 {
    if dist < 0.5 {
        1.0
    } else {
        dist * 2.5
    }
}

/// Climb animation plus track scale selected by C_human::Do_Climb (mafia-re
/// 113b5b6 game/actors/C_human.cpp:7820-7826).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimbAnimSelect {
    /// The low (0x68) or high (0x69) climb animation.
    pub anim_id: i32,
    /// The track stretch along x: the height over 0.6 low, the height high.
    pub scale: f32,
}

/// Working-name animation select of C_human::Do_Climb (no RVA claimed:
/// inline select at mafia-re 113b5b6 game/actors/C_human.cpp:7820-7826
/// inside Do_Climb 0x586400, with no standalone FUNCTION copy). Walls below
/// 0.8 m climb low, higher walls climb high. The climb animation itself
/// stays DEFERRED with the animation owner.
pub fn climb_anim_select(height: f32) -> ClimbAnimSelect {
    if height < 0.8 {
        ClimbAnimSelect {
            anim_id: ANIM_NA_BEDNU_06M,
            scale: height / 0.6,
        }
    } else {
        ClimbAnimSelect {
            anim_id: ANIM_NA_BEDNU_1M,
            scale: height,
        }
    }
}

/// Working-name wall-height gate of C_human::Do_Climb (no RVA claimed: inline
/// refuses at mafia-re 113b5b6 game/actors/C_human.cpp:7787-7790 inside
/// Do_Climb 0x586400, with no standalone FUNCTION copy; only the
/// [`climb_entry_gate`] gate carries that address). The wall must rise 0.3
/// to 1.1 m above the feet; the probes and the climb program stay DEFERRED
/// with the collision/animation owners.
pub fn climb_wall_height_gate(height: f32) -> bool {
    (0.3..=1.1).contains(&height)
}

/// State words cleared when C_human::Do_Climb takes a new climb animation
/// (mafia-re 113b5b6 game/actors/C_human.cpp:7827-7843).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClimbChangedWords {
    pub throw_swung: bool,
    pub idle_time: i32,
    pub breath_time: i32,
    pub velocity_ramp: f32,
    pub last_velocity: [f32; 3],
    pub velocity: [f32; 3],
}

/// Working-name latch reset of C_human::Do_Climb (no RVA claimed: inline
/// clears at mafia-re 113b5b6 game/actors/C_human.cpp:7827-7843 inside
/// Do_Climb 0x586400, with no standalone FUNCTION copy). Only an animation
/// change stages the words; otherwise None leaves the state alone. The thief
/// indicator clear, the extended-anim clear, ResetWeaponEffects, the reload
/// sounds and the PersonAnim handoff stay DEFERRED with their owners.
pub fn climb_changed_reset(anim_id: i32, next_anim_id: i32) -> Option<ClimbChangedWords> {
    if anim_id == next_anim_id {
        return None;
    }
    Some(ClimbChangedWords {
        throw_swung: false,
        idle_time: -1,
        breath_time: -1,
        velocity_ramp: 0.0,
        last_velocity: [0.0; 3],
        velocity: [0.0; 3],
    })
}

/// Working-name seat-climb animation base of C_human::Do_ClimbInCarLR (no RVA
/// claimed: inline OR at mafia-re 113b5b6 game/actors/C_human.cpp:6644
/// inside Do_ClimbInCarLR 0x581eb0, with no standalone FUNCTION copy; only
/// the [`climb_seat_target`] gate carries that address). The seat parity
/// picks the climb-over direction off [`ANIM_AUTO_PRELEZ_L_TO_R`]; the seat
/// animation program stays DEFERRED with the animation owner.
pub fn climb_car_anim_base(seat: i32) -> i32 {
    (seat & 1) | ANIM_AUTO_PRELEZ_L_TO_R
}

/// Working-name model-change gate of C_human::Do_ClimbInCarLR (no RVA
/// claimed: inline gates at mafia-re 113b5b6 game/actors/C_human.cpp:6667
/// and :6669 inside Do_ClimbInCarLR 0x581eb0, with no standalone FUNCTION
/// copy; only the [`climb_seat_target`] gate carries that address). Rifles
/// and SMGs always change the model after a climb-over; pistols only from
/// the driver's seat. The ChangeWeaponModel call stays DEFERRED with the
/// weapon owner.
pub fn climb_car_weapon_model_gate(weapon_kind: u32, seat: i32) -> bool {
    weapon_kind == WEAPON_KIND_RIFLE
        || weapon_kind == WEAPON_KIND_SMG
        || (seat == 0 && weapon_kind == WEAPON_KIND_PISTOL)
}

/// GAME 0x583590 (RVA 0x183590) C_human::Do_Shoot entry gate (mafia-re 113b5b6
/// game/actors/C_human.cpp:6962-6966): a carried box or a carried body
/// refuses the press at once. The pistol/rifle/SMG/thrown/melee programs,
/// the changing-weapon handoff, the extended-anim clear, the throw timing
/// and the indicator/player-time effects stay DEFERRED.
pub fn shoot_entry_gate(carrying_box: bool, carrying_dead_body: bool) -> bool {
    !carrying_box && !carrying_dead_body
}

/// GAME 0x583f40 (RVA 0x183f40) C_human::Do_ThrowGrenade throw latch
/// (mafia-re 113b5b6 game/actors/C_human.cpp:7166-7183): while not ready, or
/// holding neither a Molotov nor a grenade, the leaf refuses; otherwise the
/// staged throw direction is latched and the swing cleared. The IsReady
/// verdict and the held-item id arrive staged (`is_ready`, `item_id`); the
/// DelExtendedAnim/PersonAnim calls, the HOD animation select, the indicator
/// clear and the game-time stamp stay DEFERRED with the animation/indicator
/// owners.
pub fn throw_grenade_throw_dir(
    is_ready: bool,
    item_id: i32,
    dir: [f32; 3],
) -> Option<[f32; 3]> {
    if !is_ready {
        return None;
    }
    if item_id != GAMEITEM_MOLOTOV && item_id != GAMEITEM_GRENADE {
        return None;
    }
    Some(dir)
}

/// State words staged by the admitted C_human::Do_ThrowGrenade path (mafia-re
/// 113b5b6 game/actors/C_human.cpp:7174-7181).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GrenadeThrowWords {
    /// The throw never counts as swung on the grenade path.
    pub throw_swung: bool,
    /// The overhand-throw animation selected through the animation owner.
    pub next_anim_id: i32,
    /// The game time stamped as the last fire time.
    pub last_fire_time: u32,
}

/// Working-name word latch of C_human::Do_ThrowGrenade (no RVA claimed:
/// inline writes at mafia-re 113b5b6 game/actors/C_human.cpp:7174-7181
/// inside Do_ThrowGrenade 0x583f40, with no standalone FUNCTION copy; only
/// the [`throw_grenade_throw_dir`] gate carries that address). The
/// DelExtendedAnim/PersonAnim calls and the indicator clear stay DEFERRED
/// with the animation/indicator owners; `game_time` is the staged clock.
pub fn grenade_throw_words(game_time: u32) -> GrenadeThrowWords {
    GrenadeThrowWords {
        throw_swung: false,
        next_anim_id: ANIM_HOD_VRCHEM_HOD,
        last_fire_time: game_time,
    }
}

/// Working-name melee-press gate of C_human::Do_Shoot (no RVA claimed:
/// inline gate at mafia-re 113b5b6 game/actors/C_human.cpp:7010 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy; only the entry gate
/// [`shoot_entry_gate`] carries that address). The press reaches the
/// fists/knuckle/knife/bat arm only outside a car seat and while no strike
/// animation runs (`strike_anim_time < 0`). `in_car_seat` stages whether the
/// caller rides a car actor seat; the game/animation owners stay DEFERRED.
pub fn shoot_melee_press_gate(in_car_seat: bool, strike_anim_time: i32) -> bool {
    !in_car_seat && strike_anim_time < 0
}

/// Working-name fists-release arm select of C_human::Do_Shoot (no RVA
/// claimed: inline branch at mafia-re 113b5b6 game/actors/C_human.cpp:7072
/// inside Do_Shoot 0x583590, with no standalone FUNCTION copy). True takes
/// the direct-strike arm (next id from the strike index), false the combo
/// reset arm. `action_state` is the staged HUMAN_ACTION_* word.
pub fn fists_release_continues(dt_ms: u32, strike_index: i32, action_state: i32) -> bool {
    dt_ms <= 500 && (strike_index != 3 || action_state != HUMAN_ACTION_STAND)
}

/// Working-name fists STAND strike id of C_human::Do_Shoot (no RVA claimed:
/// inline add at mafia-re 113b5b6 game/actors/C_human.cpp:7076 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). Adds the staged
/// strike index to [`ANIM_BOJ_BOX_L_DIRECT`]. The SetWaitAnim/DelExtendedAnim
/// handoff stays DEFERRED with the animation owner.
pub fn fists_stand_strike_anim(strike_index: i32) -> i32 {
    strike_index + ANIM_BOJ_BOX_L_DIRECT
}

/// Working-name knife-release arm select of C_human::Do_Shoot (no RVA
/// claimed: inline branch at mafia-re 113b5b6 game/actors/C_human.cpp:7112
/// inside Do_Shoot 0x583590, with no standalone FUNCTION copy). True takes
/// the bod arm, false the kombo arm.
pub fn knife_release_continues(dt_ms: u32, strike_index: i32) -> bool {
    dt_ms <= 500 && strike_index != 3
}

/// Working-name knife STAND strike id of C_human::Do_Shoot (no RVA claimed:
/// inline add at mafia-re 113b5b6 game/actors/C_human.cpp:7116 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). Adds the staged
/// strike index to [`ANIM_BOJ_NUZ_BOD`]. The SetWaitAnim/DelExtendedAnim
/// handoff stays DEFERRED with the animation owner.
pub fn knife_stand_strike_anim(strike_index: i32) -> i32 {
    strike_index + ANIM_BOJ_NUZ_BOD
}

/// Working-name bat-release arm select of C_human::Do_Shoot (no RVA claimed:
/// inline branch at mafia-re 113b5b6 game/actors/C_human.cpp:7138 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). True takes the
/// left-hand strike arm, false the right-hand kombo arm.
pub fn bat_release_continues(dt_ms: u32, strike_index: i32) -> bool {
    dt_ms <= 500 && strike_index != 3
}

/// Working-name bat STAND strike id of C_human::Do_Shoot (no RVA claimed:
/// inline add at mafia-re 113b5b6 game/actors/C_human.cpp:7142 inside
/// Do_Shoot 0x583590, with no standalone FUNCTION copy). Adds the staged
/// strike index to [`ANIM_BOJ_BASB_Z_LH`]. The SetWaitAnim/DelExtendedAnim
/// handoff stays DEFERRED with the animation owner.
pub fn bat_stand_strike_anim(strike_index: i32) -> i32 {
    strike_index + ANIM_BOJ_BASB_Z_LH
}

/// Working-name strike damage of C_human::Do_Castrate/Do_HardHead (no RVA
/// claimed: inline expression at mafia-re 113b5b6
/// game/actors/C_human.cpp:7232 and :7255 inside Do_Castrate 0x584480 and
/// Do_HardHead 0x584550, with no standalone FUNCTION copy; only the strike
/// slices carry those addresses). The target energy drops by ten times the
/// striker strength. Both words arrive staged as f32; the PersonAnim
/// handoffs and property owners stay DEFERRED.
pub fn strike_energy_after(energy: f32, strength: f32) -> f32 {
    energy - strength * 10.0
}

/// Working-name target readiness gate shared by C_human::Do_Castrate and
/// C_human::Do_HardHead (no RVA claimed: inline null/IsReady gates at
/// mafia-re 113b5b6 game/actors/C_human.cpp:7216-7219 and :7240-7242 inside
/// Do_Castrate 0x584480 and Do_HardHead 0x584550, with no standalone
/// FUNCTION copy). The invulnerability-cheat refusal arrives staged through
/// [`strike_cheat_refusal`]; the IsReady verdict arrives staged as
/// `target_ready`.
pub fn strike_target_gate(human_present: bool, target_ready: bool) -> bool {
    human_present && target_ready
}

/// Working-name variant clamp of C_human::Do_ShockStart (no RVA claimed:
/// inline clamp at mafia-re 113b5b6 game/actors/C_human.cpp:8112-8115 inside
/// Do_ShockStart 0x587bc0, with no standalone FUNCTION copy; only the owned
/// select slice carries that address). Clamps the staged variant to 0..3.
pub fn shock_start_variant_clamp(variant: i32) -> i32 {
    variant.clamp(0, 3)
}

/// Working-name variant clamp of C_human::Do_ShockDown (no RVA claimed:
/// inline clamp at mafia-re 113b5b6 game/actors/C_human.cpp:8125-8128 inside
/// Do_ShockDown 0x587c20, with no standalone FUNCTION copy; only the owned
/// step slice carries that address). Clamps the staged variant to 0..2.
pub fn shock_down_variant_clamp(variant: i32) -> i32 {
    variant.clamp(0, 2)
}

/// Latch words staged by the C_human::Do_FuckingBox pickup arm (mafia-re
/// 113b5b6 game/actors/C_human.cpp:8570-8577).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxPickupLatchReset {
    /// The pickup never counts as thrown.
    pub throw_swung: bool,
    /// The pickup clears the reload-effects latch.
    pub reload_effects: bool,
    /// The pickup clears the held-shot latch.
    pub fire_held: bool,
}

/// Working-name latch reset of the C_human::Do_FuckingBox pickup arm (no RVA
/// claimed: inline clears at mafia-re 113b5b6 game/actors/C_human.cpp:8570,
/// :8575 and :8577 inside Do_FuckingBox 0x589140, with no standalone FUNCTION
/// copy; only the [`fucking_box_entry_gate`] gate carries that address). The
/// ChangeAnim program, the frame link/sector/pose program, the weapon-model
/// visibility, DelExtendedAnim/ResetWeaponEffects/Do_Aimed handoffs and the
/// using-object bookkeeping stay DEFERRED with their owners.
pub fn box_pickup_latch_reset() -> BoxPickupLatchReset {
    BoxPickupLatchReset {
        throw_swung: false,
        reload_effects: false,
        fire_held: false,
    }
}

/// Words staged by the C_human::Do_DeadBody pickup arm (mafia-re 113b5b6
/// game/actors/C_human.cpp:8754-8755 and :8821-8828).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeadBodyPickupWords {
    /// No carrier animation runs yet on pickup.
    pub dead_body_time: i32,
    /// The pickup animation selected through the animation owner.
    pub next_anim_id: i32,
    /// The pickup never counts as thrown.
    pub throw_swung: bool,
    /// The pickup clears the reload-effects latch.
    pub reload_effects: bool,
    /// The pickup clears the held-shot latch.
    pub fire_held: bool,
}

/// Working-name word latch of the C_human::Do_DeadBody pickup arm (no RVA
/// claimed: inline writes at mafia-re 113b5b6 game/actors/C_human.cpp:8754,
/// :8755, :8821 and :8826-8828 inside Do_DeadBody 0x589c20, with no standalone
/// FUNCTION copy; only the [`dead_body_entry_gate`] gate carries that
/// address). The ChangeAnim program, the carrier-human lookup, the frame
/// link/sector/pose program, the pickup sound, DelExtendedAnim/
/// ResetWeaponEffects/Do_Aimed handoffs and the using-object bookkeeping stay
/// DEFERRED with their owners.
pub fn dead_body_pickup_words() -> DeadBodyPickupWords {
    DeadBodyPickupWords {
        dead_body_time: -1,
        next_anim_id: ANIM_NOSENI_MRTVOLY_TOM_NAHOZENI,
        throw_swung: false,
        reload_effects: false,
        fire_held: false,
    }
}

/// Working-name animation-base list restored by the C_human::Do_FuckingBox
/// put-down arm (no RVA claimed: inline ChangeAnim restores at mafia-re
/// 113b5b6 game/actors/C_human.cpp:8476-8502 inside Do_FuckingBox 0x589140,
/// with no standalone FUNCTION copy; only the [`fucking_box_entry_gate`]
/// gate carries that address). Each base is restored with a NULL replacement
/// through the animation owner, which stays DEFERRED.
pub fn box_put_down_anim_bases() -> [&'static str; 14] {
    [
        "breath01a.i3d",
        "breath01b.i3d",
        "breath01c.i3d",
        "breath01d.i3d",
        "walk1.i3d",
        "back1.i3d",
        "left1.i3d",
        "right1.i3d",
        "strafL1.i3d",
        "strafR1.i3d",
        "walkL1.i3d",
        "walkR1.i3d",
        "backL1.i3d",
        "backR1.i3d",
    ]
}

/// GAME 0x589140 (RVA 0x189140) C_human::Do_FuckingBox entry gate (mafia-re
/// 113b5b6 game/actors/C_human.cpp:8459-8469; `FuckingBox` is the upstream
/// working-name family for the carried box, NOT original): a carried dead
/// body, a frame while already carrying, no frame while carrying nothing, or
/// an empty right hand refuses at once. The ChangeAnim programs, the frame
/// link/sector/pose program and the using-object bookkeeping stay DEFERRED.
pub fn fucking_box_entry_gate(
    frm_present: bool,
    carrying_box: bool,
    carrying_dead_body: bool,
    right_hand_present: bool,
) -> bool {
    if carrying_dead_body {
        return false;
    }
    if frm_present && carrying_box {
        return false;
    }
    if !frm_present && !carrying_box {
        return false;
    }
    if !right_hand_present {
        return false;
    }
    true
}

/// GAME 0x589680 (RVA 0x189680) C_human::Do_FuckingBoxCancel entry gate
/// (mafia-re 113b5b6 game/actors/C_human.cpp:8598-8602; `FuckingBox` is the
/// upstream working-name family for the carried box, NOT original): without
/// a box or without a right hand there is nothing to cancel. The car-volume
/// scan, the destination/start-point program and the ChangeAnim/model/invalid
/// -item program stay DEFERRED with the actor/scene/animation owners.
pub fn fucking_box_cancel_entry_gate(carrying_box: bool, right_hand_present: bool) -> bool {
    carrying_box && right_hand_present
}

/// GAME 0x589c20 (RVA 0x189c20) C_human::Do_DeadBody entry gate (mafia-re
/// 113b5b6 game/actors/C_human.cpp:8689-8699): a carried box, a frame while
/// already carrying, no frame while carrying nothing, or an empty right hand
/// refuses at once. The ChangeAnim programs, the frame link/sector/pose
/// program, the carrier-human lookup and the using-object bookkeeping stay
/// DEFERRED.
pub fn dead_body_entry_gate(
    frm_present: bool,
    carrying_box: bool,
    carrying_dead_body: bool,
    right_hand_present: bool,
) -> bool {
    if carrying_box {
        return false;
    }
    if frm_present && carrying_dead_body {
        return false;
    }
    if !frm_present && !carrying_dead_body {
        return false;
    }
    if !right_hand_present {
        return false;
    }
    true
}

/// Original front-seat using action (mafia-re 113b5b6 game/tools.h:51
/// USING_ACTION_GET_IN_FRONT 0x7d3): turned off while the player carries a
/// box or a dead body.
pub const USING_ACTION_GET_IN_FRONT: u32 = 0x7d3;
/// Original back-seat using action (mafia-re 113b5b6 game/tools.h:52
/// USING_ACTION_GET_IN_BACK 0x7d4): turned off while the player carries a
/// box or a dead body.
pub const USING_ACTION_GET_IN_BACK: u32 = 0x7d4;
/// Original board-train using action (mafia-re 113b5b6 game/tools.h:53
/// USING_ACTION_BOARD_TRAIN 0x7d7): turned off while the player carries a
/// box or a dead body.
pub const USING_ACTION_BOARD_TRAIN: u32 = 0x7d7;
/// Original hijack-car using action (mafia-re 113b5b6 game/tools.h:54
/// USING_ACTION_HIJACK_CAR 0x7d9): turned off while the player carries a
/// box or a dead body.
pub const USING_ACTION_HIJACK_CAR: u32 = 0x7d9;
/// Original pick-up-box using action (mafia-re 113b5b6 game/tools.h:55
/// USING_ACTION_PICK_UP_BOX 0x7da): turned off while the player carries a
/// box or a dead body.
pub const USING_ACTION_PICK_UP_BOX: u32 = 0x7da;
/// Original search-dead-body using action (mafia-re 113b5b6 game/tools.h:56
/// USING_ACTION_SEARCH_DEAD_BODY 0x7dc): turned off while the player carries
/// a box or a dead body.
pub const USING_ACTION_SEARCH_DEAD_BODY: u32 = 0x7dc;
/// Original move-dead-body using action (mafia-re 113b5b6 game/tools.h:57
/// USING_ACTION_MOVE_DEAD_BODY 0x7dd): turned off while the player carries a
/// box or a dead body.
pub const USING_ACTION_MOVE_DEAD_BODY: u32 = 0x7dd;
/// Original get-in-as-driver using action (mafia-re 113b5b6 game/tools.h:58
/// USING_ACTION_GET_IN_AS_DRIVER 0x7df): turned off while the player carries
/// a box or a dead body.
pub const USING_ACTION_GET_IN_AS_DRIVER: u32 = 0x7df;

/// Carried-load invalid actions shared by C_human::Do_FuckingBox,
/// C_human::Do_FuckingBoxCancel and C_human::Do_DeadBody (mafia-re 113b5b6
/// game/tools.h:51-58; turned off while the player carries at
/// game/actors/C_human.cpp:8510-8517, :8670-8677 and :8737-8744, back on at
/// :8580-8587 and :8829-8836).
/// Working name: the list is inlined at each site with no standalone
/// FUNCTION copy.
pub fn box_carry_invalid_actions() -> [u32; 8] {
    [
        USING_ACTION_GET_IN_FRONT,
        USING_ACTION_GET_IN_BACK,
        USING_ACTION_BOARD_TRAIN,
        USING_ACTION_HIJACK_CAR,
        USING_ACTION_PICK_UP_BOX,
        USING_ACTION_SEARCH_DEAD_BODY,
        USING_ACTION_MOVE_DEAD_BODY,
        USING_ACTION_GET_IN_AS_DRIVER,
    ]
}

/// Working-name pickup animation pairs of C_human::Do_FuckingBox (no RVA
/// claimed: inline ChangeAnim pairs at mafia-re 113b5b6
/// game/actors/C_human.cpp:8529-8563 inside Do_FuckingBox 0x589140, with no
/// standalone FUNCTION copy; only the [`fucking_box_entry_gate`] gate
/// carries that address). Each base animation is replaced with its
/// box-carry ("Bedna") variant through the animation owner, which stays
/// DEFERRED.
pub fn box_pickup_anim_pairs() -> [(&'static str, &'static str); 14] {
    [
        ("breath01a.i3d", "BednaStativ.i3d"),
        ("breath01b.i3d", "BednaStativ.i3d"),
        ("breath01c.i3d", "BednaStativ.i3d"),
        ("breath01d.i3d", "BednaStativ.i3d"),
        ("walk1.i3d", "BednaWalk.i3d"),
        ("back1.i3d", "BednaBack.i3d"),
        ("left1.i3d", "BednaLeft.i3d"),
        ("right1.i3d", "BednaRight.i3d"),
        ("strafL1.i3d", "BednaStrafL.i3d"),
        ("strafR1.i3d", "BednaStrafR.i3d"),
        ("walkL1.i3d", "BednaWalkL.i3d"),
        ("walkR1.i3d", "BednaWalkR.i3d"),
        ("backL1.i3d", "BednaBackL.i3d"),
        ("backR1.i3d", "BednaBackR.i3d"),
    ]
}

/// Latch words staged when the carried box is put down (mafia-re 113b5b6
/// game/actors/C_human.cpp:8503-8517, same tail in the
/// C_human::Do_FuckingBoxCancel found arm at :8662-8677 and the
/// C_human::Do_DeadBody drop arm at :8729-8744).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxPutDownLatchReset {
    /// The carrier animation id clears.
    pub anim_id: i32,
    /// Both weapon models switch back on.
    pub weapon_models_on: bool,
}

/// Working-name put-down words of the box carry (no RVA claimed: inline
/// writes shared by C_human::Do_FuckingBox 0x589140,
/// C_human::Do_FuckingBoxCancel 0x589680 and C_human::Do_DeadBody 0x589c20,
/// with no standalone FUNCTION copy; only the entry gates carry those
/// addresses). The ChangeAnim programs, the frame/sector/shadow program and
/// the using-object bookkeeping stay DEFERRED with their owners.
pub fn box_put_down_latch_reset() -> BoxPutDownLatchReset {
    BoxPutDownLatchReset {
        anim_id: 0,
        weapon_models_on: true,
    }
}

/// Destination program of the C_human::Do_FuckingBoxCancel found arm
/// (mafia-re 113b5b6 game/actors/C_human.cpp:8634-8643).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxCancelDestination {
    /// The box lies farther from its start than from the enemy destination:
    /// the destination advances.
    AdvanceNextDest,
    /// Otherwise the placement info is restored and the box re-added.
    RestoreAndRecompile,
}

/// Working-name destination select of C_human::Do_FuckingBoxCancel (no RVA
/// claimed: inline compare at mafia-re 113b5b6
/// game/actors/C_human.cpp:8634-8643 inside Do_FuckingBoxCancel 0x589680,
/// with no standalone FUNCTION copy; only the
/// [`fucking_box_cancel_entry_gate`] gate carries that address). `farther`
/// stages whether the squared box-to-start distance beats the squared
/// box-to-destination distance; the placement/scene owners stay DEFERRED.
pub fn box_cancel_destination_pick(farther: bool) -> BoxCancelDestination {
    if farther {
        BoxCancelDestination::AdvanceNextDest
    } else {
        BoxCancelDestination::RestoreAndRecompile
    }
}

/// Working-name pickup animation pairs of C_human::Do_DeadBody (no RVA
/// claimed: inline ChangeAnim pairs at mafia-re 113b5b6
/// game/actors/C_human.cpp:8764-8798 inside Do_DeadBody 0x589c20, with no
/// standalone FUNCTION copy; only the [`dead_body_entry_gate`] gate carries
/// that address). Each base animation is replaced with its body-carry
/// ("NoseniMrtvolyTom") variant through the animation owner, which stays
/// DEFERRED.
pub fn dead_body_pickup_anim_pairs() -> [(&'static str, &'static str); 14] {
    [
        ("breath01a.i3d", "NoseniMrtvolyTomStat.I3D"),
        ("breath01b.i3d", "NoseniMrtvolyTomStat.I3D"),
        ("breath01c.i3d", "NoseniMrtvolyTomStat.I3D"),
        ("breath01d.i3d", "NoseniMrtvolyTomStat.I3D"),
        ("walk1.i3d", "NoseniMrtvolyTomWalk.I3D"),
        ("back1.i3d", "NoseniMrtvolyTomBack.I3D"),
        ("left1.i3d", "NoseniMrtvolyTomLeft.I3D"),
        ("right1.i3d", "NoseniMrtvolyTomRight.I3D"),
        ("strafL1.i3d", "NoseniMrtvolyTomStrafL.I3D"),
        ("strafR1.i3d", "NoseniMrtvolyTomStrafR.I3D"),
        ("walkL1.i3d", "NoseniMrtvolyTomWalkL.I3D"),
        ("walkR1.i3d", "NoseniMrtvolyTomWalkR.I3D"),
        ("backL1.i3d", "NoseniMrtvolyTomBackL.I3D"),
        ("backR1.i3d", "NoseniMrtvolyTomBackR.I3D"),
    ]
}

/// Working-name pickup-sound gate of C_human::Do_DeadBody (no RVA claimed:
/// inline gate at mafia-re 113b5b6 game/actors/C_human.cpp:8807 inside
/// Do_DeadBody 0x589c20, with no standalone FUNCTION copy; only the
/// [`dead_body_entry_gate`] gate carries that address). The pickup sound
/// plays at high game sounds within 20 m (squared distance below 400);
/// `game_sounds` is the staged options word. The sound/scene owners stay
/// DEFERRED.
pub fn dead_body_pickup_sound_gate(game_sounds: i32, dist2: f32) -> bool {
    game_sounds >= OPTION_HIGH && dist2 < 400.0
}

#[cfg(test)]
mod do_small_actions_rva_tests {
    use super::*;

    #[test]
    fn change_weapon_rva_17f550_selects_entry_arm() {
        // GAME 0x57f550 (RVA 0x17f550).
        assert_eq!(
            change_weapon_entry(false, true),
            ChangeWeaponEntry::AlreadyChanging
        );
        assert_eq!(
            change_weapon_entry(false, false),
            ChangeWeaponEntry::StartChange
        );
        assert_eq!(
            change_weapon_entry(true, false),
            ChangeWeaponEntry::DirectModel
        );
        assert_eq!(
            change_weapon_entry(true, true),
            ChangeWeaponEntry::DirectModel
        );
    }

    #[test]
    fn reload_rva_185b40_guards_then_scans_to_reload() {
        // GAME 0x585b40 (RVA 0x185b40).
        let table = [(0, 100), (FIRE_ACTION_RELOAD, 200), (FIRE_ACTION_END, 0)];
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_PISTOL, 3, &table, false),
            Some(ReloadFirePlan {
                step: 0,
                time: 100,
                shell_by_shell: false,
            })
        );
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_SMG, 1, &table, true),
            Some(ReloadFirePlan {
                step: 0,
                time: 100,
                shell_by_shell: true,
            })
        );
        // Guards refuse.
        assert_eq!(
            reload_fire_plan(true, false, true, WEAPON_KIND_PISTOL, 3, &table, false),
            None
        );
        assert_eq!(
            reload_fire_plan(false, true, true, WEAPON_KIND_PISTOL, 3, &table, false),
            None
        );
        assert_eq!(
            reload_fire_plan(false, false, false, WEAPON_KIND_PISTOL, 3, &table, false),
            None
        );
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_FISTS, 3, &table, false),
            None
        );
        assert_eq!(
            reload_fire_plan(false, false, true, 4, 3, &table, false),
            None
        );
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_PISTOL, 0, &table, false),
            None
        );
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_PISTOL, 3, &[], false),
            None
        );
        // Reload at step zero keeps step zero with zero time.
        let leading = [(FIRE_ACTION_RELOAD, 50), (FIRE_ACTION_END, 0)];
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_PISTOL, 2, &leading, false),
            Some(ReloadFirePlan {
                step: 0,
                time: 0,
                shell_by_shell: false,
            })
        );
        // No reload step: the end index is kept with zero time.
        let plain = [(0, 100), (0, 150), (FIRE_ACTION_END, 0)];
        assert_eq!(
            reload_fire_plan(false, false, true, WEAPON_KIND_PISTOL, 2, &plain, false),
            Some(ReloadFirePlan {
                step: 2,
                time: 0,
                shell_by_shell: false,
            })
        );
    }

    #[test]
    fn play_anim_rva_185e80_clears_words_and_passes_track() {
        // GAME 0x585e80 (RVA 0x185e80).
        let live = do_play_anim_leaf(7, 1234);
        assert_eq!(live.reset.idle_time, -1);
        assert_eq!(live.reset.breath_time, -1);
        assert_eq!(live.reset.velocity, [0.0, 0.0, 0.0]);
        assert_eq!(live.reset.velocity_ramp, 0.0);
        assert!(!live.reset.test_ground);
        assert_eq!(live.reset.fall, 0.0);
        assert!(!live.reset.throw_swung);
        assert_eq!(live.action, Some(HUMAN_ACTION_TRACK));
        assert_eq!(live.length, 1234);
        let kept = do_play_anim_leaf(-1, 9);
        assert_eq!(kept.action, None);
        assert_eq!(kept.length, 9);
    }

    #[test]
    fn jump_rva_186190_refuses_carry_and_car() {
        // GAME 0x586190 (RVA 0x186190).
        assert!(jump_entry_gate(false, false, false));
        assert!(!jump_entry_gate(true, false, false));
        assert!(!jump_entry_gate(false, true, false));
        assert!(!jump_entry_gate(false, false, true));
    }

    #[test]
    fn climb_rva_186400_refuses_carry() {
        // GAME 0x586400 (RVA 0x186400).
        assert!(climb_entry_gate(false, false));
        assert!(!climb_entry_gate(true, false));
        assert!(!climb_entry_gate(false, true));
    }

    #[test]
    fn set_wait_anim_inline_select_stages_wait_past_250ms() {
        // Inline C_human::SetWaitAnim (mafia-re 113b5b6
        // game/actors/C_human.h:1158-1172, working name, no RVA): only a
        // length past 250 ms stages the wait words.
        assert_eq!(set_wait_anim_select(250), None);
        assert_eq!(set_wait_anim_select(0), None);
        assert_eq!(
            set_wait_anim_select(251),
            Some(WaitAnimWords {
                action_time: 1,
                last_velocity: [0.0, 0.0, 0.0],
                velocity_ramp: 0.0,
                action_state: HUMAN_ACTION_WAIT,
            })
        );
        assert_eq!(
            set_wait_anim_select(1000),
            Some(WaitAnimWords {
                action_time: 750,
                last_velocity: [0.0, 0.0, 0.0],
                velocity_ramp: 0.0,
                action_state: HUMAN_ACTION_WAIT,
            })
        );
        assert_eq!(HUMAN_ACTION_WAIT, 5);
    }

    #[test]
    fn inline_strike_car_shot_stubs_pin_working_names() {
        // Inline C_human::SetFreeBoxingAnim, C_human::ShootingInCar and
        // C_human::TestShotCollision (mafia-re 113b5b6
        // game/actors/C_human.h:606-608 and :683-688, working names, no
        // RVA): explicit unsupported stubs while the animation-cache,
        // car/frame/shot and shot-skeleton owners stay DEFERRED.
        set_free_boxing_anim_unsupported();
        shooting_in_car_unsupported();
        test_shot_collision_unsupported();
    }

    #[test]
    fn human_savegame_rva_16f9e0_sums_staged_words() {
        // GAME 0x56f9e0 (RVA 0x16f9e0) C_human::SaveGameGetSize (mafia-re
        // 113b5b6 game/actors/C_human.cpp:1417-1425): base + 0x17e human
        // words + staged car/using-car/inventory words.
        assert_eq!(human_savegame_size(13, 8, 8, 0), 13 + 0x17e + 16);
        assert_eq!(human_savegame_size(0, 0, 0, 0), 0x17e);
        assert_eq!(
            human_savegame_size(100, 9, 10, 11),
            100 + 0x17e + 9 + 10 + 11
        );
    }

    #[test]
    fn side_jump_rva_187040_selects_base_and_side() {
        // GAME 0x587040 (RVA 0x187040).
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0, false, false),
            Some(ANIM_USKOK_LEFT_BACK)
        );
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0, false, false),
            Some(ANIM_USKOK_RIGHT_FRONT)
        );
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0, false, false),
            Some(ANIM_USKOK_BACK_RIGHT)
        );
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0, false, false),
            Some(ANIM_USKOK_FRONT_LEFT)
        );
        // Line side below the track bumps the id by one.
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, -1.0], [0.0, 0.0, 0.0], 0, false, false),
            Some(ANIM_USKOK_LEFT_BACK + 1)
        );
        // Carry and in-flight ids refuse.
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0, true, false),
            None
        );
        assert_eq!(
            side_jump_select([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 0, false, true),
            None
        );
        assert_eq!(
            side_jump_select(
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                ANIM_USKOK_LEFT_BACK,
                false,
                false
            ),
            None
        );
    }

    #[test]
    fn climb_seat_rva_181eb0_pairs_and_gates_occupancy() {
        // GAME 0x581eb0 (RVA 0x181eb0).
        assert_eq!(climb_seat_target(0, true, true, false), Some(1));
        assert_eq!(climb_seat_target(1, true, true, false), Some(0));
        assert_eq!(climb_seat_target(2, true, true, false), Some(3));
        assert_eq!(climb_seat_target(3, true, true, false), Some(2));
        assert_eq!(climb_seat_target(0, true, true, true), None);
        assert_eq!(climb_seat_target(0, false, true, false), None);
        assert_eq!(climb_seat_target(0, true, false, false), None);
        assert_eq!(climb_seat_target(-1, true, true, false), None);
    }

    #[test]
    fn shoot_rva_183590_refuses_carry() {
        // GAME 0x583590 (RVA 0x183590).
        assert!(shoot_entry_gate(false, false));
        assert!(!shoot_entry_gate(true, false));
        assert!(!shoot_entry_gate(false, true));
    }

    #[test]
    fn throw_grenade_rva_183f40_gates_ready_item_and_latches_dir() {
        // GAME 0x583f40 (RVA 0x183f40).
        let dir = [0.0, 1.0, 0.0];
        assert_eq!(throw_grenade_throw_dir(false, GAMEITEM_MOLOTOV, dir), None);
        assert_eq!(throw_grenade_throw_dir(true, 0, dir), None);
        assert_eq!(throw_grenade_throw_dir(true, GAMEITEM_MOLOTOV, dir), Some(dir));
        assert_eq!(throw_grenade_throw_dir(true, GAMEITEM_GRENADE, dir), Some(dir));
    }

    #[test]
    fn fucking_box_rva_189140_gates_frame_carry_and_hand() {
        // GAME 0x589140 (RVA 0x189140).
        assert!(fucking_box_entry_gate(true, false, false, true));
        assert!(fucking_box_entry_gate(false, true, false, true));
        assert!(!fucking_box_entry_gate(true, false, true, true));
        assert!(!fucking_box_entry_gate(true, true, false, true));
        assert!(!fucking_box_entry_gate(false, false, false, true));
        assert!(!fucking_box_entry_gate(true, false, false, false));
    }

    #[test]
    fn fucking_box_cancel_rva_189680_needs_box_and_hand() {
        // GAME 0x589680 (RVA 0x189680).
        assert!(fucking_box_cancel_entry_gate(true, true));
        assert!(!fucking_box_cancel_entry_gate(false, true));
        assert!(!fucking_box_cancel_entry_gate(true, false));
    }

    #[test]
    fn dead_body_rva_189c20_gates_frame_carry_and_hand() {
        // GAME 0x589c20 (RVA 0x189c20).
        assert!(dead_body_entry_gate(true, false, false, true));
        assert!(dead_body_entry_gate(false, false, true, true));
        assert!(!dead_body_entry_gate(true, true, false, true));
        assert!(!dead_body_entry_gate(true, false, true, true));
        assert!(!dead_body_entry_gate(false, false, false, true));
        assert!(!dead_body_entry_gate(true, false, false, false));
    }
}

#[cfg(test)]
mod working_name_small_leaves_tests {
    use super::*;

    #[test]
    fn change_weapon_anim_names_cover_how_and_stance() {
        // Working names: no RVA claimed (inline switch, C_human.cpp:6021-6040).
        assert_eq!(
            change_weapon_anim_name(HUMAN_WEAPON_ON_OFF, false),
            Some("gun stoj on off.i3d")
        );
        assert_eq!(
            change_weapon_anim_name(HUMAN_WEAPON_ON_OFF, true),
            Some("gun drep on off.i3d")
        );
        assert_eq!(
            change_weapon_anim_name(HUMAN_WEAPON_THROW_AWAY, false),
            Some("gun stoj zahozeni.i3d")
        );
        assert_eq!(
            change_weapon_anim_name(HUMAN_WEAPON_THROW_AWAY, true),
            Some("gun drep zahozeni.i3d")
        );
        assert_eq!(change_weapon_anim_name(2, false), None);
        assert_eq!(change_weapon_anim_name(-1, true), None);
    }

    #[test]
    fn change_weapon_latch_reset_clears_deterministic_words() {
        // Working name: no RVA claimed (inline clears, C_human.cpp:5992-6013).
        let reset = change_weapon_latch_reset();
        assert!(reset.changing);
        assert!(!reset.aimed);
        assert!(!reset.fire_held);
        assert!(!reset.throw_swung);
        assert!(!reset.reloading);
        assert!(!reset.reload_forced);
        assert!(!reset.loading_shells);
        assert!(!reset.reload_effects);
        assert_eq!(reset.strike_anim_time, -1);
        assert_eq!(reset.fire_step, 0);
        assert_eq!(reset.fire_time, 0);
    }

    #[test]
    fn reload_latch_select_claims_forced_and_shell_gates() {
        // Working name: no RVA claimed (inline writes, C_human.cpp:7539-7549).
        let plain = reload_latch_select(false, false, false);
        assert!(plain.reload_forced);
        assert!(plain.reload_effects);
        assert!(!plain.loading_shells);
        assert!(!plain.weapon_displaced);
        let shell = reload_latch_select(true, true, false);
        assert!(shell.loading_shells);
        assert!(!shell.weapon_displaced);
        let displaced = reload_latch_select(true, true, true);
        assert!(displaced.loading_shells);
        assert!(displaced.weapon_displaced);
        assert!(!reload_latch_select(true, false, true).loading_shells);
        assert!(!reload_latch_select(false, true, true).loading_shells);
    }

    #[test]
    fn strike_index_advances_within_window_and_wraps() {
        // Working name: no RVA claimed (inline update, C_human.cpp:7015-7022).
        assert_eq!(shoot_strike_index_next(0, 0), 1);
        assert_eq!(shoot_strike_index_next(2, 1199), 3);
        assert_eq!(shoot_strike_index_next(3, 0), 0);
        assert_eq!(shoot_strike_index_next(1, 1200), 0);
        assert_eq!(shoot_strike_index_next(3, 5000), 0);
        assert_eq!(shoot_strike_index_next(-1, 0), 0);
    }

    #[test]
    fn backswing_gate_needs_bat_and_fresh_strike() {
        // Working name: no RVA claimed (inline gate, C_human.cpp:7025-7037).
        assert!(shoot_backswing_gate(true, false, 201));
        assert!(!shoot_backswing_gate(true, false, 200));
        assert!(!shoot_backswing_gate(true, true, 500));
        assert!(!shoot_backswing_gate(false, false, 500));
        assert!(!shoot_backswing_gate(true, false, 0));
    }

    #[test]
    fn throw_hold_clamps_between_release_and_cap() {
        // Working name: no RVA claimed (inline window, C_human.cpp:7053-7056).
        assert_eq!(throw_hold_clamp(0), None);
        assert_eq!(throw_hold_clamp(400), None);
        assert_eq!(throw_hold_clamp(401), Some(401));
        assert_eq!(throw_hold_clamp(2000), Some(2000));
        assert_eq!(throw_hold_clamp(5000), Some(2000));
    }

    #[test]
    fn holster_model_change_tracks_item_id() {
        // Working name: no RVA claimed (inline compare, C_human.cpp:7563-7565).
        assert!(!holster_needs_model_change(7, 7));
        assert!(holster_needs_model_change(7, 8));
        assert!(holster_needs_model_change(0, 1));
    }

    #[test]
    fn drop_out_guard_needs_hands_and_items() {
        // Working name: no RVA claimed (inline guard, C_human.cpp:6148-6151).
        assert!(drop_out_items_gate(true, true, 1));
        assert!(!drop_out_items_gate(false, true, 1));
        assert!(!drop_out_items_gate(true, false, 1));
        assert!(!drop_out_items_gate(true, true, 0));
    }

    #[test]
    fn jump_anim_select_covers_run_stance_strafe_and_miss() {
        // Working name: no RVA claimed (inline branch, C_human.cpp:7650-7710).
        assert_eq!(
            jump_anim_select(ANIM_RUN1, WEAPON_KIND_PISTOL, false),
            JumpAnimSelect::RunClimbAttempt
        );
        assert_eq!(
            jump_anim_select(ANIM_RUN1, WEAPON_KIND_PISTOL, true),
            JumpAnimSelect::NoJump
        );
        assert_eq!(
            jump_anim_select(ANIM_WALK1, WEAPON_KIND_FISTS, false),
            JumpAnimSelect::StanceClimbAttempt
        );
        assert_eq!(
            jump_anim_select(ANIM_BREATH01C, WEAPON_KIND_FISTS, false),
            JumpAnimSelect::StanceClimbAttempt
        );
        assert_eq!(
            jump_anim_select(ANIM_WALK1, WEAPON_KIND_FISTS, true),
            JumpAnimSelect::NoJump
        );
        assert_eq!(
            jump_anim_select(ANIM_STRAFL1, WEAPON_KIND_PISTOL, false),
            JumpAnimSelect::LeftStrafe {
                anim_id: ANIM_JUMPL1,
                ff_direction: core::f32::consts::PI / 2.0,
            }
        );
        assert_eq!(
            jump_anim_select(ANIM_STRAFRL1, WEAPON_KIND_SMG, false),
            JumpAnimSelect::LeftStrafe {
                anim_id: ANIM_JUMPL3,
                ff_direction: core::f32::consts::PI / 2.0,
            }
        );
        assert_eq!(
            jump_anim_select(ANIM_STRAFR1, WEAPON_KIND_RIFLE, false),
            JumpAnimSelect::RightStrafe {
                anim_id: ANIM_JUMPR3,
                ff_direction: core::f32::consts::PI * 3.0 / 2.0,
            }
        );
        assert_eq!(
            jump_anim_select(ANIM_STRAFRR1, WEAPON_KIND_FISTS, true),
            JumpAnimSelect::RightStrafe {
                anim_id: ANIM_JUMPR1,
                ff_direction: core::f32::consts::PI * 3.0 / 2.0,
            }
        );
        assert_eq!(
            jump_anim_select(ANIM_JUMP1, WEAPON_KIND_PISTOL, false),
            JumpAnimSelect::NoJump
        );
        assert_eq!(
            jump_anim_select(0x7f, WEAPON_KIND_PISTOL, false),
            JumpAnimSelect::NoJump
        );
    }

    #[test]
    fn climb_reach_select_threshold_at_half_metre() {
        // Working name: no RVA claimed (inline select, C_human.cpp:7818-7819).
        assert_eq!(climb_reach_select(0.0), 1.0);
        assert_eq!(climb_reach_select(0.49), 1.0);
        assert_eq!(climb_reach_select(0.5), 1.25);
        assert_eq!(climb_reach_select(1.0), 2.5);
    }

    #[test]
    fn climb_anim_select_splits_at_zero_point_eight() {
        // Working name: no RVA claimed (inline select, C_human.cpp:7820-7826).
        assert_eq!(
            climb_anim_select(0.5),
            ClimbAnimSelect {
                anim_id: ANIM_NA_BEDNU_06M,
                scale: 0.5 / 0.6,
            }
        );
        assert_eq!(
            climb_anim_select(0.79),
            ClimbAnimSelect {
                anim_id: ANIM_NA_BEDNU_06M,
                scale: 0.79 / 0.6,
            }
        );
        assert_eq!(
            climb_anim_select(0.8),
            ClimbAnimSelect {
                anim_id: ANIM_NA_BEDNU_1M,
                scale: 0.8,
            }
        );
        assert_eq!(
            climb_anim_select(1.1),
            ClimbAnimSelect {
                anim_id: ANIM_NA_BEDNU_1M,
                scale: 1.1,
            }
        );
    }

    #[test]
    fn climb_seat_scale_dead_zone_and_ratio() {
        // Working name: no RVA claimed (inline ratio, C_human.cpp:6653-6661).
        assert_eq!(climb_seat_scale(2.0, 0.05), 1.0);
        assert_eq!(climb_seat_scale(2.0, -0.1), 1.0);
        assert_eq!(climb_seat_scale(1.5, 0.5), 3.0);
        assert_eq!(climb_seat_scale(-1.5, 0.5), 3.0);
    }

    #[test]
    fn climb_changed_reset_gates_on_animation_change() {
        // Working name: no RVA claimed (inline clears, C_human.cpp:7827-7843).
        assert_eq!(climb_changed_reset(3, 3), None);
        let words = climb_changed_reset(3, 4).expect("changed anim stages words");
        assert!(!words.throw_swung);
        assert_eq!(words.idle_time, -1);
        assert_eq!(words.breath_time, -1);
        assert_eq!(words.velocity_ramp, 0.0);
        assert_eq!(words.last_velocity, [0.0; 3]);
        assert_eq!(words.velocity, [0.0; 3]);
    }

    #[test]
    fn shoot_press_gun_words_cover_both_aim_arms() {
        // Working name: no RVA claimed (inline writes, C_human.cpp:6979-6986).
        let fresh = shoot_press_gun_words(false, Some([1.0, 2.0, 3.0]));
        assert!(fresh.fire_held);
        assert_eq!(fresh.aim_table, Some(FireActionTable::Aimed));
        assert_eq!(fresh.fire_step, Some(0));
        assert_eq!(fresh.throw_dir, Some([1.0, 2.0, 3.0]));
        let held = shoot_press_gun_words(true, None);
        assert!(held.fire_held);
        assert_eq!(held.aim_table, None);
        assert_eq!(held.fire_step, None);
        assert_eq!(held.throw_dir, None);
    }

    #[test]
    fn shoot_press_thrown_words_latch_first_swing() {
        // Working name: no RVA claimed (inline writes, C_human.cpp:6991-7003).
        assert_eq!(
            shoot_press_thrown_words(false, 1200),
            ShootThrownPress {
                throw_time: Some(1200),
                throw_swung: true,
            }
        );
        assert_eq!(
            shoot_press_thrown_words(true, 1200),
            ShootThrownPress {
                throw_time: None,
                throw_swung: true,
            }
        );
    }

    #[test]
    fn shoot_release_thrown_program_covers_idle_cancel_and_strike() {
        // Working name: no RVA claimed (inline branch, C_human.cpp:7049-7065).
        assert_eq!(
            shoot_release_thrown_program(false, 1000),
            ShootThrownRelease::Idle
        );
        assert_eq!(
            shoot_release_thrown_program(true, 400),
            ShootThrownRelease::CancelSwing
        );
        assert_eq!(
            shoot_release_thrown_program(true, 1000),
            ShootThrownRelease::ThrowStrike {
                clamped_dt_ms: 1000,
            }
        );
        assert_eq!(
            shoot_release_thrown_program(true, 5000),
            ShootThrownRelease::ThrowStrike {
                clamped_dt_ms: 2000,
            }
        );
    }

    #[test]
    fn throw_force_scale_matches_native_factor() {
        // Working name: no RVA claimed (inline scale, C_human.cpp:7054-7058).
        assert_eq!(throw_force_scale(1000, 2.0), 1000.0 * 0.015 * 2.0);
        assert_eq!(throw_force_scale(0, 5.0), 0.0);
        assert_eq!(throw_force_scale(2000, 0.0), 0.0);
    }

    #[test]
    fn holster_model_program_covers_all_branches() {
        // Working name: no RVA claimed (inline branch, C_human.cpp:7565-7576).
        assert_eq!(
            holster_model_program(false, 0, false),
            HolsterModelProgram {
                model: HolsterModelStep::Idle,
                drop_out: false,
            }
        );
        assert_eq!(
            holster_model_program(true, 2, true),
            HolsterModelProgram {
                model: HolsterModelStep::SkipDecrementAndModel,
                drop_out: true,
            }
        );
        assert_eq!(
            holster_model_program(true, 0, false),
            HolsterModelProgram {
                model: HolsterModelStep::ChangeWeapon,
                drop_out: false,
            }
        );
    }

    #[test]
    fn grenade_throw_words_latch_time_and_animation() {
        // Working name: no RVA claimed (inline writes, C_human.cpp:7174-7181).
        assert_eq!(
            grenade_throw_words(4321),
            GrenadeThrowWords {
                throw_swung: false,
                next_anim_id: ANIM_HOD_VRCHEM_HOD,
                last_fire_time: 4321,
            }
        );
    }

    #[test]
    fn weapon_drop_throw_away_needs_item_and_drop() {
        // Working name: no RVA claimed (inline chain, C_human.cpp:7593-7604).
        assert!(weapon_drop_throw_away(true, true));
        assert!(!weapon_drop_throw_away(false, true));
        assert!(!weapon_drop_throw_away(true, false));
        assert!(!weapon_drop_throw_away(false, false));
    }

    #[test]
    fn strike_cheat_refusal_spares_player_and_friend() {
        // Working name: no RVA claimed (inline gates, C_human.cpp:7221-7227
        // and :7244-7250).
        assert!(!strike_cheat_refusal(false, true, true));
        assert!(!strike_cheat_refusal(true, false, false));
        assert!(strike_cheat_refusal(true, true, false));
        assert!(strike_cheat_refusal(true, false, true));
    }

    #[test]
    fn change_weapon_fire_select_swaps_to_on_off() {
        // Working name: no RVA claimed (inline swap, C_human.cpp:6017-6019).
        assert_eq!(
            change_weapon_fire_select(),
            (FireActionTable::OnOff, 0)
        );
    }

    #[test]
    fn melee_press_gate_needs_open_seat_and_idle_strike() {
        // Working name: no RVA claimed (inline gate, C_human.cpp:7010).
        assert!(shoot_melee_press_gate(false, -1));
        assert!(!shoot_melee_press_gate(true, -1));
        assert!(!shoot_melee_press_gate(false, 0));
        assert!(!shoot_melee_press_gate(true, 100));
    }

    #[test]
    fn fists_release_selects_direct_or_combo_arm() {
        // Working name: no RVA claimed (inline branch, C_human.cpp:7072).
        assert!(fists_release_continues(0, 0, HUMAN_ACTION_STAND));
        assert!(fists_release_continues(500, 2, HUMAN_ACTION_STAND));
        assert!(!fists_release_continues(501, 0, HUMAN_ACTION_STAND));
        assert!(fists_release_continues(100, 3, HUMAN_ACTION_WALK));
        assert!(!fists_release_continues(100, 3, HUMAN_ACTION_STAND));
    }

    #[test]
    fn fists_stand_anim_adds_index_to_direct_base() {
        // Working name: no RVA claimed (inline add, C_human.cpp:7076).
        assert_eq!(fists_stand_strike_anim(0), ANIM_BOJ_BOX_L_DIRECT);
        assert_eq!(fists_stand_strike_anim(2), ANIM_BOJ_BOX_L_DIRECT + 2);
        assert_eq!(fists_stand_strike_anim(-1), ANIM_BOJ_BOX_L_DIRECT - 1);
    }

    #[test]
    fn knife_release_selects_bod_or_kombo_arm() {
        // Working name: no RVA claimed (inline branch, C_human.cpp:7112).
        assert!(knife_release_continues(0, 0));
        assert!(knife_release_continues(500, 2));
        assert!(!knife_release_continues(501, 0));
        assert!(!knife_release_continues(100, 3));
    }

    #[test]
    fn knife_stand_anim_adds_index_to_bod_base() {
        // Working name: no RVA claimed (inline add, C_human.cpp:7116).
        assert_eq!(knife_stand_strike_anim(0), ANIM_BOJ_NUZ_BOD);
        assert_eq!(knife_stand_strike_anim(1), ANIM_BOJ_NUZ_BOD + 1);
    }

    #[test]
    fn bat_release_selects_left_or_right_arm() {
        // Working name: no RVA claimed (inline branch, C_human.cpp:7138).
        assert!(bat_release_continues(0, 0));
        assert!(bat_release_continues(500, 3 - 1));
        assert!(!bat_release_continues(501, 0));
        assert!(!bat_release_continues(0, 3));
    }

    #[test]
    fn bat_stand_anim_adds_index_to_left_base() {
        // Working name: no RVA claimed (inline add, C_human.cpp:7142).
        assert_eq!(bat_stand_strike_anim(0), ANIM_BOJ_BASB_Z_LH);
        assert_eq!(bat_stand_strike_anim(3), ANIM_BOJ_BASB_Z_LH + 3);
    }

    #[test]
    fn strike_damage_subtracts_tenfold_strength() {
        // Working name: no RVA claimed (inline expr, C_human.cpp:7232/:7255).
        assert_eq!(strike_energy_after(100.0, 2.0), 80.0);
        assert_eq!(strike_energy_after(0.0, 0.0), 0.0);
        assert_eq!(strike_energy_after(50.0, 0.5), 45.0);
    }

    #[test]
    fn strike_target_gate_needs_presence_and_readiness() {
        // Working name: no RVA claimed (inline gates, C_human.cpp:7216-7219).
        assert!(strike_target_gate(true, true));
        assert!(!strike_target_gate(false, true));
        assert!(!strike_target_gate(true, false));
        assert!(!strike_target_gate(false, false));
    }

    #[test]
    fn shock_start_variant_clamp_matches_native_bounds() {
        // Working name: no RVA claimed (inline clamp, C_human.cpp:8112-8115).
        assert_eq!(shock_start_variant_clamp(-5), 0);
        assert_eq!(shock_start_variant_clamp(0), 0);
        assert_eq!(shock_start_variant_clamp(3), 3);
        assert_eq!(shock_start_variant_clamp(9), 3);
    }

    #[test]
    fn shock_down_variant_clamp_matches_native_bounds() {
        // Working name: no RVA claimed (inline clamp, C_human.cpp:8125-8128).
        assert_eq!(shock_down_variant_clamp(-1), 0);
        assert_eq!(shock_down_variant_clamp(2), 2);
        assert_eq!(shock_down_variant_clamp(3), 2);
    }

    #[test]
    fn box_pickup_latch_clears_throw_reload_and_hold() {
        // Working name: no RVA claimed (inline clears, C_human.cpp:8570-8577).
        assert_eq!(
            box_pickup_latch_reset(),
            BoxPickupLatchReset {
                throw_swung: false,
                reload_effects: false,
                fire_held: false,
            }
        );
    }

    #[test]
    fn dead_body_pickup_words_stage_time_anim_and_latches() {
        // Working name: no RVA claimed (inline writes, C_human.cpp:8754-8755
        // and :8821-8828).
        assert_eq!(
            dead_body_pickup_words(),
            DeadBodyPickupWords {
                dead_body_time: -1,
                next_anim_id: ANIM_NOSENI_MRTVOLY_TOM_NAHOZENI,
                throw_swung: false,
                reload_effects: false,
                fire_held: false,
            }
        );
    }

    #[test]
    fn box_put_down_restores_fourteen_base_slots() {
        // Working name: no RVA claimed (inline restores, C_human.cpp:8476-8502).
        let bases = box_put_down_anim_bases();
        assert_eq!(bases.len(), 14);
        assert_eq!(bases[0], "breath01a.i3d");
        assert_eq!(bases[4], "walk1.i3d");
        assert_eq!(bases[13], "backR1.i3d");
    }

    #[test]
    fn net_direct_empty_runs_without_state() {
        // Working name: no RVA claimed (linker-folded empty, C_human.h:509-510
        // into 0x4ab130, whose RVA 0xab130 the frozen ledger already retains).
        HumanState::net_direct_empty();
    }

    #[test]
    fn universal_use_empty_runs_without_state() {
        // Working name: no RVA claimed (empty on the PC, C_human.h:594-595 and
        // C_human.cpp:10078-10081, folded into 0x4aabe0, whose RVA 0xaabe0 the
        // frozen ledger already retains).
        HumanState::universal_use_empty();
    }

    #[test]
    fn box_pickup_anim_pairs_map_bases_to_bedna() {
        // Working names: no RVA claimed (inline pairs, C_human.cpp:8529-8563).
        let pairs = box_pickup_anim_pairs();
        assert_eq!(pairs.len(), 14);
        assert_eq!(pairs[0], ("breath01a.i3d", "BednaStativ.i3d"));
        assert_eq!(pairs[4], ("walk1.i3d", "BednaWalk.i3d"));
        assert_eq!(pairs[13], ("backR1.i3d", "BednaBackR.i3d"));
    }

    #[test]
    fn box_carry_invalid_actions_lists_eight_ids() {
        // Working names: no RVA claimed (tools.h:51-58).
        let actions = box_carry_invalid_actions();
        assert_eq!(actions.len(), 8);
        assert_eq!(actions[0], 0x7d3);
        assert_eq!(actions[4], 0x7da);
        assert_eq!(actions[7], 0x7df);
    }

    #[test]
    fn box_put_down_latch_clears_anim_and_shows_models() {
        // Working names: no RVA claimed (inline words, C_human.cpp:8503-8517).
        let words = box_put_down_latch_reset();
        assert_eq!(
            words,
            BoxPutDownLatchReset {
                anim_id: 0,
                weapon_models_on: true,
            }
        );
    }

    #[test]
    fn box_cancel_destination_pick_splits_on_farther() {
        // Working name: no RVA claimed (inline compare, C_human.cpp:8634-8643).
        assert_eq!(
            box_cancel_destination_pick(true),
            BoxCancelDestination::AdvanceNextDest
        );
        assert_eq!(
            box_cancel_destination_pick(false),
            BoxCancelDestination::RestoreAndRecompile
        );
    }

    #[test]
    fn dead_body_pickup_anim_pairs_map_bases_to_tom() {
        // Working names: no RVA claimed (inline pairs, C_human.cpp:8764-8798).
        let pairs = dead_body_pickup_anim_pairs();
        assert_eq!(pairs.len(), 14);
        assert_eq!(pairs[0], ("breath01a.i3d", "NoseniMrtvolyTomStat.I3D"));
        assert_eq!(pairs[4], ("walk1.i3d", "NoseniMrtvolyTomWalk.I3D"));
        assert_eq!(pairs[13], ("backR1.i3d", "NoseniMrtvolyTomBackR.I3D"));
    }

    #[test]
    fn dead_body_pickup_sound_gate_needs_high_sounds_nearby() {
        use crate::mission_options::OPTION_MEDIUM;
        // Working name: no RVA claimed (inline gate, C_human.cpp:8807).
        assert!(dead_body_pickup_sound_gate(OPTION_HIGH, 399.0));
        assert!(!dead_body_pickup_sound_gate(OPTION_HIGH, 400.0));
        assert!(!dead_body_pickup_sound_gate(OPTION_MEDIUM, 0.0));
    }

    #[test]
    fn jump_changed_words_clear_throw_idle_breath_and_effects() {
        // Working name: no RVA claimed (inline clears, C_human.cpp:7712-7719).
        assert_eq!(
            jump_changed_words(),
            JumpChangedWords {
                throw_swung: false,
                idle_time: -1,
                breath_time: -1,
                reload_effects: false,
            }
        );
    }

    #[test]
    fn jump_run_words_flag_left_and_stage_takeoff() {
        // Working name: no RVA claimed (inline writes, C_human.cpp:7655-7662).
        assert_eq!(
            jump_run_words(100, 300),
            JumpRunWords {
                anim_left: true,
                velocity_y: 6.0,
            }
        );
        assert_eq!(
            jump_run_words(150, 300),
            JumpRunWords {
                anim_left: false,
                velocity_y: 6.0,
            }
        );
    }

    #[test]
    fn climb_wall_height_gate_splits_at_point_three_and_one_one() {
        // Working name: no RVA claimed (inline refuses, C_human.cpp:7787-7790).
        assert!(climb_wall_height_gate(0.3));
        assert!(climb_wall_height_gate(1.1));
        assert!(!climb_wall_height_gate(0.29));
        assert!(!climb_wall_height_gate(1.11));
    }

    #[test]
    fn climb_car_anim_base_ors_seat_parity_with_prelez() {
        // Working names: no RVA claimed (inline OR, C_human.cpp:6644:
        // `seat & 1 | ANIM_AUTO_PRELEZ_L_TO_R`, & binds tighter than |).
        assert_eq!(climb_car_anim_base(0), 0x130);
        assert_eq!(climb_car_anim_base(1), 0x131);
        assert_eq!(climb_car_anim_base(2), 0x130);
        assert_eq!(climb_car_anim_base(3), 0x131);
    }

    #[test]
    fn climb_car_weapon_model_gate_matches_kind_and_seat() {
        // Working names: no RVA claimed (inline gates, C_human.cpp:6667-6669).
        assert!(climb_car_weapon_model_gate(WEAPON_KIND_RIFLE, 3));
        assert!(climb_car_weapon_model_gate(WEAPON_KIND_SMG, 2));
        assert!(climb_car_weapon_model_gate(WEAPON_KIND_PISTOL, 0));
        assert!(!climb_car_weapon_model_gate(WEAPON_KIND_PISTOL, 1));
        assert!(!climb_car_weapon_model_gate(WEAPON_KIND_FISTS, 0));
    }

    #[test]
    fn shoot_is_melee_kind_admits_four_strike_kinds() {
        // Working names: no RVA claimed (inline cases, C_human.cpp:7006-7009).
        assert!(shoot_is_melee_kind(WEAPON_KIND_FISTS));
        assert!(shoot_is_melee_kind(WEAPON_KIND_KNUCKLEDUSTER));
        assert!(shoot_is_melee_kind(WEAPON_KIND_KNIFE));
        assert!(shoot_is_melee_kind(WEAPON_KIND_BAT));
        assert!(!shoot_is_melee_kind(WEAPON_KIND_PISTOL));
        assert!(!shoot_is_melee_kind(WEAPON_KIND_RIFLE));
        assert!(!shoot_is_melee_kind(WEAPON_KIND_SMG));
        assert!(!shoot_is_melee_kind(WEAPON_KIND_THROWN));
    }

    #[test]
    fn shoot_release_clears_fire_except_for_thrown() {
        // Working names: no RVA claimed (inline switch, C_human.cpp:7043-7159).
        assert!(shoot_release_clears_fire(WEAPON_KIND_PISTOL));
        assert!(shoot_release_clears_fire(WEAPON_KIND_RIFLE));
        assert!(shoot_release_clears_fire(WEAPON_KIND_SMG));
        assert!(shoot_release_clears_fire(WEAPON_KIND_FISTS));
        assert!(shoot_release_clears_fire(WEAPON_KIND_KNIFE));
        assert!(shoot_release_clears_fire(WEAPON_KIND_BAT));
        assert!(!shoot_release_clears_fire(WEAPON_KIND_THROWN));
    }
}

/// GAME 0x56e370 (RVA 0x16e370, VA 0x56e370) ai_loc::Init (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1068-1075): stages the owning actor and
/// its staged world position, clears flags and time. The frame world-position
/// owner stays DEFERRED; the caller stages `world_pos`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiLoc {
    pub actor: u32,
    pub pos: [f32; 3],
    pub flags: u32,
    pub time: u32,
}

/// GAME 0x56e370 (RVA 0x16e370, VA 0x56e370) ai_loc::Init leaf.
pub fn ai_loc_init(actor: u32, world_pos: [f32; 3]) -> AiLoc {
    AiLoc {
        actor,
        pos: world_pos,
        flags: 0,
        time: 0,
    }
}

/// One staged ai_loc segment record (mafia-re 113b5b6
/// game/actors/C_human.h:221-229 ai_seg_stat 0x10 bytes): the special type,
/// the game time it was last found, the find count and the opaque special
/// identity. The segment owner stays DEFERRED; identity is never dereferenced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AiSegStat {
    pub seg_type: u32,
    pub time: u32,
    pub count: i32,
    pub special: u32,
}

/// GAME 0x56e6f0 (RVA 0x16e6f0, VA 0x56e6f0) ai_loc::AddSegment (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1141-1157): refreshes the matching special
/// (time update plus count increment) or pushes a new count-1 record. The
/// mission game-time owner stays DEFERRED; the caller stages `game_time`.
pub fn ai_loc_add_segment(
    segs: &mut Vec<AiSegStat>,
    seg_type: u32,
    special: u32,
    game_time: u32,
) {
    for seg in segs.iter_mut() {
        if seg.special == special {
            seg.time = game_time;
            seg.count = seg.count.wrapping_add(1);
            return;
        }
    }
    segs.push(AiSegStat {
        seg_type,
        time: game_time,
        count: 1,
        special,
    });
}

/// GAME 0x56e950 (RVA 0x16e950, VA 0x56e950) ai_loc::SegListUpdate
/// (mafia-re 113b5b6 game/actors/C_human.cpp:1159-1167): drops specials not
/// seen for 4 seconds by back-scan erase. The mission game-time owner stays
/// DEFERRED; the caller stages `game_time`. 32-bit wrapping matches the
/// native `uint` addition.
pub fn ai_loc_seg_list_update(segs: &mut Vec<AiSegStat>, game_time: u32) {
    for index in (0..segs.len()).rev() {
        if segs[index].time.wrapping_add(4000) < game_time {
            segs.remove(index);
        }
    }
}

/// Staged AI blob flags (mafia-re 113b5b6 game/actors/C_human.h:271-275).
pub const AIBLOB_RUNNING: u32 = 0x1;
/// Staged crouching flag of ai_blob::flags (mafia-re 113b5b6 C_human.h:272).
pub const AIBLOB_CROUCHING: u32 = 0x2;
/// Staged firing flag of ai_blob::flags (mafia-re 113b5b6 C_human.h:273).
pub const AIBLOB_FIRING: u32 = 0x4;
/// Staged jumping flag of ai_blob::flags (mafia-re 113b5b6 C_human.h:274).
pub const AIBLOB_JUMPING: u32 = 0x8;
/// Unnamed jump animation id read by the blob switch (mafia-re 113b5b6
/// game/actors/C_actor.h: ANIM_UNKNOWN_5E 0x5e).
pub const ANIM_UNKNOWN_5E: i32 = 0x5e;
/// Running-jump animation id read by the blob switch (mafia-re 113b5b6
/// game/actors/C_actor.h: ANIM_JUMPRX1 0x60).
pub const ANIM_JUMPRX1: i32 = 0x60;

/// One staged ai_blob record (mafia-re 113b5b6 game/actors/C_human.h:258-269
/// 0x24 bytes): metre-raised position, frame direction delta, game time,
/// state flags and opaque owner identity. Frame/model owners stay DEFERRED.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AiBlob {
    pub pos: [f32; 3],
    pub dir: [f32; 3],
    pub time: u32,
    pub flags: u32,
    pub actor: u32,
}

impl Default for AiBlob {
    fn default() -> Self {
        Self {
            pos: [0.0; 3],
            dir: [0.0; 3],
            time: 0,
            flags: 0,
            actor: 0,
        }
    }
}

/// Staged ai_blob_log owner (mafia-re 113b5b6 game/actors/C_human.h:277-292
/// 0x244 bytes): sixteen slots plus the last index, -1 for none.
#[derive(Clone, Debug, PartialEq)]
pub struct AiBlobLog {
    pub blobs: [AiBlob; 16],
    pub last: i32,
}

impl Default for AiBlobLog {
    fn default() -> Self {
        Self {
            blobs: [AiBlob::default(); 16],
            last: -1,
        }
    }
}

/// GAME 0x56eeb0 (RVA 0x16eeb0, VA 0x56eeb0) ai_blob_log::Clear (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1218-1223): zeroes the sixteen slots and
/// resets the last index to -1.
pub fn blob_log_clear(log: &mut AiBlobLog) {
    log.blobs = [AiBlob::default(); 16];
    log.last = -1;
}

/// GAME 0x56ee60 (RVA 0x16ee60, VA 0x56ee60) ai_blob_log::GetBlob (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1205-1216): copies the slot `back` before
/// the last (`(last - back) & 0xf`), or None for an empty log and for
/// `back > last`. The native heap copy becomes a value copy here; no owner
/// is allocated.
pub fn blob_log_get(log: &AiBlobLog, back: u32) -> Option<AiBlob> {
    if log.last < 0 {
        return None;
    }
    if back > log.last as u32 {
        return None;
    }
    let index = (log.last as u32).wrapping_sub(back) & 0xf;
    Some(log.blobs[index as usize])
}

/// GAME 0x56ea00 (RVA 0x16ea00, VA 0x56ea00) ai_blob_log::Blob (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1169-1203): logs the staged frame position
/// (raised by one metre) and direction unless the last record is less than
/// 500 ms old. Direction is the normalized delta from the previous slot when
/// it has an owner, else the staged frame direction. Flags stage crouching,
/// firing and the masked walk/jump animation switch. Frame, mission
/// game-time and animation-table owners stay DEFERRED; every input arrives
/// staged. Returns whether a slot was written.
pub fn blob_log_push(
    log: &mut AiBlobLog,
    owner: u32,
    frame_pos: [f32; 3],
    frame_dir: [f32; 3],
    game_time: u32,
    crouching: bool,
    fire_held: bool,
    anim_id: i32,
) -> bool {
    if log.last != -1 {
        let cur = log.blobs[(log.last as u32 & 0xf) as usize];
        if game_time < cur.time.wrapping_add(500) {
            return false;
        }
    }
    let prev_index = (log.last as u32 & 0xf) as usize;
    let prev_actor = log.blobs[prev_index].actor;
    let prev_pos = log.blobs[prev_index].pos;
    log.last = log.last.wrapping_add(1);
    let index = (log.last as u32 & 0xf) as usize;
    let slot = &mut log.blobs[index];
    slot.actor = owner;
    slot.time = game_time;
    slot.pos = [frame_pos[0], frame_pos[1] + 1.0, frame_pos[2]];
    if prev_actor != 0 {
        slot.dir = normalize_strafe_vector([
            slot.pos[0] - prev_pos[0],
            slot.pos[1] - prev_pos[1],
            slot.pos[2] - prev_pos[2],
        ]);
    } else {
        slot.dir = frame_dir;
    }
    slot.flags = 0;
    if crouching {
        slot.flags |= AIBLOB_CROUCHING;
    }
    if fire_held {
        slot.flags |= AIBLOB_FIRING;
    }
    match anim_id & ANIM_BASE {
        id if id == ANIM_RUN1 => slot.flags |= AIBLOB_RUNNING,
        id if id == ANIM_UNKNOWN_5E || id == ANIM_JUMP1 => slot.flags |= AIBLOB_JUMPING,
        id if id == ANIM_JUMPRX1 => slot.flags |= AIBLOB_JUMPING | AIBLOB_RUNNING,
        _ => {}
    }
    true
}

/// Ground-height probe boundary shared by the delta helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroundDeltaError {
    Unsupported { entry: u32 },
}

/// GAME 0x56eed0 (RVA 0x16eed0, VA 0x56eed0) _get_delta_y_ (mafia-re 113b5b6
/// game/actors/C_human.cpp:1225-1234) EXPLICIT UNSUPPORTED STUB: the height
/// of the ground below the staged position needs the global collision owner
/// (`gifCollision.TestLineV`), so no height is staged.
pub const GET_DELTA_Y_ENTRY: u32 = 0x56eed0;

/// GAME 0x56eed0 (RVA 0x16eed0, VA 0x56eed0) probe stub.
pub fn ground_delta_y(_pos: [f32; 3]) -> Result<f32, GroundDeltaError> {
    Err(GroundDeltaError::Unsupported {
        entry: GET_DELTA_Y_ENTRY,
    })
}

/// GAME 0x56ef50 (RVA 0x16ef50, VA 0x56ef50) _get_delta_nodyn_y_ (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1236-1245) EXPLICIT UNSUPPORTED STUB: the
/// moving-object-excluding variant needs the global collision owner
/// (`gifCollision.TestLineV` with `COLF_ALL`), so no height is staged.
pub const GET_DELTA_NODYN_Y_ENTRY: u32 = 0x56ef50;

/// GAME 0x56ef50 (RVA 0x16ef50, VA 0x56ef50) probe stub.
pub fn ground_delta_nodyn_y(_pos: [f32; 3]) -> Result<f32, GroundDeltaError> {
    Err(GroundDeltaError::Unsupported {
        entry: GET_DELTA_NODYN_Y_ENTRY,
    })
}

/// GAME 0x56f760 (RVA 0x16f760, VA 0x56f760) _ReadOldCharStruct (mafia-re
/// 113b5b6 game/actors/C_human.cpp:1323-1349): maps the first save versions'
/// sixteen floats onto the sixteen max-property slots, scaling the four limb
/// energies by `1 / (1 - old[1])` while `old[1]` sits in `(0, 1]`. The chunk
/// owner stays DEFERRED; the sixteen words arrive staged.
pub fn read_old_char_struct(old: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    let mut factor = 1.0f32;
    if old[1] > 0.0 && old[1] <= 1.0 {
        factor = 1.0 / (1.0 - old[1]);
    }
    out[0] = old[0];
    out[1] = old[2] * factor;
    out[2] = old[3] * factor;
    out[3] = old[4] * factor;
    out[4] = old[5] * factor;
    out[5] = old[6] * factor;
    out[6] = old[7];
    out[7] = old[8];
    out[8] = old[9];
    out[9] = old[10];
    out[10] = old[11];
    out[11] = old[12];
    out[12] = old[13];
    out[13] = old[14];
    out[14] = old[15];
    out[15] = 0.5;
    out
}

/// GAME 0x56f8c0 (RVA 0x16f8c0, VA 0x56f8c0) _ReadOldCharStructVer4
/// (mafia-re 113b5b6 game/actors/C_human.cpp:1351-1373): maps version 4's
/// fifteen floats onto the first fifteen max-property slots and fixes the
/// sixteenth to 0.5. The chunk owner stays DEFERRED; words arrive staged.
pub fn read_old_char_struct_ver4(old: &[f32; 15]) -> [f32; 16] {
    let mut out = [0.0f32; 16];
    for (index, word) in old.iter().enumerate() {
        out[index] = *word;
    }
    out[15] = 0.5;
    out
}

/// GAME 0x56f210 (RVA 0x16f210, VA 0x56f210) C_human::~C_human gate
/// (mafia-re 113b5b6 game/actors/C_human.cpp:1274-1280): the destructor stops
/// the model's first animation only while a frame is present and its type is
/// a model. The frame/model owner stays DEFERRED; the caller stages the two
/// verdicts. Returns whether `StopAnimation(0)` runs.
pub fn human_dtor_stop_anim(frame_present: bool, is_model: bool) -> bool {
    frame_present && is_model
}

/// Interpolated death step of C_human::ProcessDeathPos.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DeathStep {
    pub new_time: i32,
    pub pos: [f32; 3],
}

/// GAME 0x58cec0 (RVA 0x18cec0, VA 0x58cec0) C_human::ProcessDeathPos
/// (mafia-re 113b5b6 game/actors/C_human.cpp:9578-9590): advances the death
/// clock by the staged step, clamps it at the staged length and interpolates
/// `death_pos + death_move * (new_time / length)`. Returns None while the
/// length is negative or the clock already reached it. The frame
/// `SetPos/Update` owner stays DEFERRED; the position is returned staged.
pub fn process_death_pos(
    death_time: i32,
    death_length: i32,
    step_ms: u32,
    death_pos: [f32; 3],
    death_move: [f32; 3],
) -> Option<DeathStep> {
    if death_length < 0 || death_time >= death_length {
        return None;
    }
    let mut new_time = death_time.wrapping_add(step_ms as i32);
    if new_time > death_length {
        new_time = death_length;
    }
    let factor = new_time as f32 / death_length as f32;
    Some(DeathStep {
        new_time,
        pos: [
            death_pos[0] + death_move[0] * factor,
            death_pos[1] + death_move[1] * factor,
            death_pos[2] + death_move[2] * factor,
        ],
    })
}

/// Wall-egress probe boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallPushError {
    Unsupported { entry: u32 },
}

/// GAME 0x58c4f0 (RVA 0x18c4f0, VA 0x58c4f0) PushOutOfWalls (mafia-re 113b5b6
/// game/actors/C_human.cpp:9467-9507) EXPLICIT UNSUPPORTED STUB: moving the
/// frame out of steep walls needs the frame and global collision owners
/// (`GetMatrix/TestLineH/SetPos/Update`), so no displacement is staged.
pub const PUSH_OUT_OF_WALLS_ENTRY: u32 = 0x58c4f0;

/// GAME 0x58c4f0 (RVA 0x18c4f0, VA 0x58c4f0) egress stub.
pub fn push_out_of_walls_unsupported() -> Result<bool, WallPushError> {
    Err(WallPushError::Unsupported {
        entry: PUSH_OUT_OF_WALLS_ENTRY,
    })
}

/// GAME 0x58bf30 (RVA 0x18bf30, VA 0x58bf30) SettleOnGround (mafia-re 113b5b6
/// game/actors/C_human.cpp:9427) EXPLICIT UNSUPPORTED STUB: lifting/lowering
/// the frame by the ground mean height under the staged bone points and
/// turning it to the ground mean normal needs the frame and global collision
/// owners (`GetMatrix/TestLineV/SetPos/Update`), so no height is staged and
/// no frame is touched; the stub exists only to pin the address while those
/// owners stay DEFERRED.
pub const SETTLE_ON_GROUND_ENTRY: u32 = 0x58bf30;

/// GAME 0x58bf30 (RVA 0x18bf30, VA 0x58bf30) ground-settle stub.
pub fn settle_on_ground_unsupported() {}

/// GAME 0x58c980 (RVA 0x18c980, VA 0x58c980) SlideOffEdge (mafia-re 113b5b6
/// game/actors/C_human.cpp:9514) EXPLICIT UNSUPPORTED STUB: sliding the frame
/// off a ledge (or lowering it to the nearest ground within 50) needs the
/// frame and global collision owners (`GetMatrix/TestLineV/TestLineH/SetPos/
/// Update`), so no displacement is staged; the stub exists only to pin the
/// address while those owners stay DEFERRED. The native `bool` answer is
/// preserved in the return type.
pub const SLIDE_OFF_EDGE_ENTRY: u32 = 0x58c980;

/// GAME 0x58c980 (RVA 0x18c980, VA 0x58c980) ledge-slide stub.
pub fn slide_off_edge_unsupported() -> Result<bool, WallPushError> {
    Err(WallPushError::Unsupported {
        entry: SLIDE_OFF_EDGE_ENTRY,
    })
}

/// Animation-note callback boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HumanNotifyError {
    Unsupported { entry: u32 },
}

/// GAME 0x56d130 (RVA 0x16d130, VA 0x56d130) human_notify_callback (mafia-re
/// 113b5b6 game/actors/C_human.cpp:784) EXPLICIT UNSUPPORTED STUB: the
/// model's animation-note dispatch (step sounds/tracks, car doors, weapon
/// hand swaps, grenade throw, weapon effects, screams) needs the frame,
/// sound, scene and material owners, so no note is handled; the stub exists
/// only to pin the address while those owners stay DEFERRED. The native
/// `uint` answer is preserved in the return type.
pub const HUMAN_NOTIFY_CALLBACK_ENTRY: u32 = 0x56d130;

/// GAME 0x56d130 (RVA 0x16d130, VA 0x56d130) note-callback stub. The frame
/// owner is deferred, so only the staged note words cross the boundary.
pub fn human_notify_callback_unsupported(
    _message: u32,
    _param1: u32,
    _param2: u32,
) -> Result<u32, HumanNotifyError> {
    Err(HumanNotifyError::Unsupported {
        entry: HUMAN_NOTIFY_CALLBACK_ENTRY,
    })
}

#[cfg(test)]
mod ai_loc_blob_rva_tests {
    use super::*;

    #[test]
    fn ai_loc_init_rva_16e370_stages_actor_and_zeroes() {
        // GAME 0x56e370 (RVA 0x16e370).
        let loc = ai_loc_init(7, [1.0, 2.0, 3.0]);
        assert_eq!(loc.actor, 7);
        assert_eq!(loc.pos, [1.0, 2.0, 3.0]);
        assert_eq!(loc.flags, 0);
        assert_eq!(loc.time, 0);
    }

    #[test]
    fn ai_loc_add_segment_rva_16e6f0_refreshes_or_pushes() {
        // GAME 0x56e6f0 (RVA 0x16e6f0).
        let mut segs = Vec::new();
        ai_loc_add_segment(&mut segs, 3, 11, 1000);
        assert_eq!(
            segs,
            vec![AiSegStat {
                seg_type: 3,
                time: 1000,
                count: 1,
                special: 11
            }]
        );
        ai_loc_add_segment(&mut segs, 9, 11, 2000);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].time, 2000);
        assert_eq!(segs[0].count, 2);
        assert_eq!(segs[0].seg_type, 3);
        ai_loc_add_segment(&mut segs, 4, 12, 2500);
        assert_eq!(segs.len(), 2);
    }

    #[test]
    fn ai_loc_seg_list_update_rva_16e950_expires_after_four_seconds() {
        // GAME 0x56e950 (RVA 0x16e950).
        let mut segs = vec![
            AiSegStat {
                seg_type: 1,
                time: 1000,
                count: 2,
                special: 1
            },
            AiSegStat {
                seg_type: 2,
                time: 5000,
                count: 1,
                special: 2
            },
        ];
        ai_loc_seg_list_update(&mut segs, 4999);
        assert_eq!(segs.len(), 2);
        ai_loc_seg_list_update(&mut segs, 5001);
        assert_eq!(
            segs,
            vec![AiSegStat {
                seg_type: 2,
                time: 5000,
                count: 1,
                special: 2
            }]
        );
    }

    #[test]
    fn blob_log_clear_rva_16eeb0_zeroes_and_empties() {
        // GAME 0x56eeb0 (RVA 0x16eeb0).
        let mut log = AiBlobLog::default();
        assert!(blob_log_push(
            &mut log,
            5,
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            1000,
            false,
            false,
            ANIM_RUN1
        ));
        assert_ne!(log.last, -1);
        blob_log_clear(&mut log);
        assert_eq!(log.last, -1);
        assert!(log.blobs.iter().all(|slot| *slot == AiBlob::default()));
        assert_eq!(blob_log_get(&log, 0), None);
    }

    #[test]
    fn blob_log_get_rva_16ee60_copies_indexed_slot() {
        // GAME 0x56ee60 (RVA 0x16ee60).
        let mut log = AiBlobLog::default();
        assert_eq!(blob_log_get(&log, 0), None);
        assert!(blob_log_push(
            &mut log,
            1,
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            0,
            false,
            false,
            0
        ));
        assert!(blob_log_push(
            &mut log,
            2,
            [10.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            600,
            false,
            false,
            0
        ));
        let fresh = blob_log_get(&log, 0).unwrap();
        assert_eq!(fresh.actor, 2);
        let older = blob_log_get(&log, 1).unwrap();
        assert_eq!(older.actor, 1);
        assert_eq!(blob_log_get(&log, 2), None);
    }

    #[test]
    fn blob_log_push_rva_16ea00_gates_time_and_stages_flags() {
        // GAME 0x56ea00 (RVA 0x16ea00).
        let mut log = AiBlobLog::default();
        assert!(blob_log_push(
            &mut log,
            9,
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            1000,
            true,
            true,
            ANIM_RUN1
        ));
        let slot = blob_log_get(&log, 0).unwrap();
        assert_eq!(slot.pos, [1.0, 3.0, 3.0]);
        assert_eq!(slot.dir, [0.0, 0.0, 1.0]);
        assert_eq!(slot.flags & AIBLOB_CROUCHING, AIBLOB_CROUCHING);
        assert_eq!(slot.flags & AIBLOB_FIRING, AIBLOB_FIRING);
        assert_eq!(slot.flags & AIBLOB_RUNNING, AIBLOB_RUNNING);
        assert!(!blob_log_push(
            &mut log,
            9,
            [1.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            1200,
            false,
            false,
            0
        ));
        assert!(blob_log_push(
            &mut log,
            9,
            [4.0, 2.0, 3.0],
            [0.0, 0.0, 1.0],
            1600,
            false,
            false,
            ANIM_JUMPRX1
        ));
        let second = blob_log_get(&log, 0).unwrap();
        assert_eq!(
            second.flags & (AIBLOB_JUMPING | AIBLOB_RUNNING),
            AIBLOB_JUMPING | AIBLOB_RUNNING
        );
    }

    #[test]
    fn ground_delta_rva_16eed0_reports_unsupported() {
        // GAME 0x56eed0 (RVA 0x16eed0).
        assert_eq!(GET_DELTA_Y_ENTRY, 0x56eed0);
        assert_eq!(
            ground_delta_y([0.0, 0.0, 0.0]),
            Err(GroundDeltaError::Unsupported { entry: 0x56eed0 })
        );
    }

    #[test]
    fn ground_delta_nodyn_rva_16ef50_reports_unsupported() {
        // GAME 0x56ef50 (RVA 0x16ef50).
        assert_eq!(GET_DELTA_NODYN_Y_ENTRY, 0x56ef50);
        assert_eq!(
            ground_delta_nodyn_y([1.0, 2.0, 3.0]),
            Err(GroundDeltaError::Unsupported { entry: 0x56ef50 })
        );
    }

    #[test]
    fn read_old_char_struct_rva_16f760_scales_limb_energies() {
        // GAME 0x56f760 (RVA 0x16f760).
        let old = [
            0.7, 0.5, 100.0, 20.0, 20.0, 20.0, 20.0, 0.7, 1.0, 0.6, 0.8, 0.7, 1.0, 1.0, 0.8,
            80.0,
        ];
        let mapped = read_old_char_struct(&old);
        assert_eq!(mapped[0], 0.7);
        assert_eq!(mapped[1], 200.0);
        assert_eq!(mapped[15], 0.5);
        let flat = [1.0f32; 16];
        let unscaled = read_old_char_struct(&flat);
        // old[1] == 1.0 satisfies the native (0, 1] guard, so k = 1/0 = +inf
        // and the limb words scale to infinity — exactly like the original.
        assert_eq!(unscaled[1], f32::INFINITY);
    }

    #[test]
    fn read_old_char_struct_ver4_rva_16f8c0_copies_fifteen() {
        // GAME 0x56f8c0 (RVA 0x16f8c0).
        let old = [2.0f32; 15];
        let mapped = read_old_char_struct_ver4(&old);
        assert!(mapped[..15].iter().all(|word| *word == 2.0));
        assert_eq!(mapped[15], 0.5);
    }

    #[test]
    fn human_dtor_rva_16f210_gates_model_stop() {
        // GAME 0x56f210 (RVA 0x16f210).
        assert!(human_dtor_stop_anim(true, true));
        assert!(!human_dtor_stop_anim(true, false));
        assert!(!human_dtor_stop_anim(false, true));
        assert!(!human_dtor_stop_anim(false, false));
    }

    #[test]
    fn process_death_pos_rva_18cec0_advances_and_clamps() {
        // GAME 0x58cec0 (RVA 0x18cec0).
        assert_eq!(process_death_pos(5, -1, 10, [0.0; 3], [1.0; 3]), None);
        assert_eq!(process_death_pos(10, 10, 5, [0.0; 3], [1.0; 3]), None);
        let step = process_death_pos(0, 100, 25, [1.0, 2.0, 3.0], [4.0, 0.0, -2.0]).unwrap();
        assert_eq!(step.new_time, 25);
        assert_eq!(step.pos, [2.0, 2.0, 2.5]);
        let clamped =
            process_death_pos(90, 100, 50, [0.0, 0.0, 0.0], [10.0, 0.0, 0.0]).unwrap();
        assert_eq!(clamped.new_time, 100);
        assert_eq!(clamped.pos, [10.0, 0.0, 0.0]);
    }

    #[test]
    fn push_out_of_walls_rva_18c4f0_reports_unsupported() {
        // GAME 0x58c4f0 (RVA 0x18c4f0).
        assert_eq!(PUSH_OUT_OF_WALLS_ENTRY, 0x58c4f0);
        assert_eq!(
            push_out_of_walls_unsupported(),
            Err(WallPushError::Unsupported { entry: 0x58c4f0 })
        );
    }
}
