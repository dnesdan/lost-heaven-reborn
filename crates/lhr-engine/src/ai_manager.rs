//! The `AIManager` members `C_game::Init` resets right after its
//! `ResetMissionState` span: the police notice board, the groups, the
//! following and the fights (PS2 `ai_manager`, mafia-re fe7e746
//! game/ai/jp_ai_main.h; Game.exe global at VA 0x646c80, whose `Init`
//! call site loads `ECX` = 0x646d10 / 0x646d44 / 0x646d6c / 0x646d80 for
//! the four calls at 0x5a0996 / 0x5a09a0 / 0x5a09aa / 0x5a09b4).
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Class and record names are PS2's (mafia-re); member names are mafia-re's
//! working names, not original. The implicit constructor (GAME 0x538760,
//! REA ev_3993cd63ba) constructs only the vectors and leaves every scalar of
//! the zero-initialized global at zero, which [`AiManager::default`] mirrors.
//!
//! Records carry only what their destructors release plus the borrowed
//! identities they name: every element destructor these `Init`s reach
//! (`ai_police_action`/`ai_fight_followship` GAME 0x538c30, `ai_group`
//! 0x531060, `ai_follow_group` 0x44b220) frees vector storage through
//! `operator delete` (0x60fcf6) and nothing else, so dropping the typed
//! record is the whole release. Referenced actors and frames are borrowed
//! identities and are never released by these clears. The scalar words of
//! the records stay unrepresented until a writer is ported; their writers
//! (`JaChcuCislo`, `NewGroup`, `AddFollower`, the fights' `Tick`, ...) are
//! not ported, so a live session holds every vector empty.
#![forbid(unsafe_code)]

use crate::ai_driver_tick::AiFreeActorStore;
use crate::frame_transform::FrameKey;
use crate::script_scheduler::ActorReferenceId;

/// `unknown_24`'s value after `ZacatekSmeny` (`mov [ebx+0x24],0xfffffff` at
/// GAME 0x53bb32): 28 set bits, not -1.
pub const POLICE_SHIFT_START_UNKNOWN_24: u32 = 0x0fff_ffff;
/// `speed_factor` after `ZacatekSmeny` (`mov [ebx+0x28],0x3f800000` at
/// GAME 0x53bb3c).
pub const POLICE_SHIFT_START_SPEED_FACTOR_BITS: u32 = 0x3f80_0000;
/// `ai_groups::unlock_time` after `Init` (GAME 0x531136): none.
pub const AI_GROUPS_NO_UNLOCK: u32 = 0xffff_ffff;

/// One `ai_anim_item` (0x10 bytes): only the frame at +0xc is carried, as a
/// borrowed identity. `ai_animitem_manager::Clear` (GAME 0x53d8c0, unported)
/// unlinks and releases it; `Init`'s `clear()` drops the entry without any
/// release, as the native copy loop does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiAnimItem {
    pub frame: FrameKey,
}

/// One `ai_police_action` (0x6c bytes): only the `members` vector at +0x18
/// that its destructor (GAME 0x538c30) frees, holding borrowed actors.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiPoliceAction {
    pub members: Vec<ActorReferenceId>,
}

/// `ai_police_nastenka` (AIManager +0x90, 0x34 bytes), including its
/// `ai_animitem_manager` base's `items` vector at +0x0.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiPoliceNastenka {
    /// +0x0 (base `ai_animitem_manager::items`).
    items: Vec<AiAnimItem>,
    /// +0x10.
    actions: Vec<AiPoliceAction>,
    /// +0x20, the next action number (`JaChcuCislo`'s; sign unproven).
    next_id: u32,
    /// +0x24 (`Tick` counts it down unsigned to 0).
    unknown_24: u32,
    /// +0x28, `POLICE_SPEED_FACTOR`'s factor.
    speed_factor: f32,
    /// +0x2c, the byte `PanoveHrajeteSiNaPoldy` returns.
    on_duty: u8,
    /// +0x30, `TuMasZakaznika`'s entity; `None` is native NULL.
    customer: Option<ActorReferenceId>,
}

impl AiPoliceNastenka {
    /// FUNCTION: GAME VA 0x53ba50, RVA 0x13ba50 (`ai_police_nastenka::
    /// ZacatekSmeny`, mafia-re game/ai/jp_ai_prislusnici.cpp:14-24). REA
    /// ev_8a80560aeb, Ghidra body 0x53ba50..0x53bb4e (255 bytes): the
    /// inlined `actions.erase(begin, end)` (degenerate copy loop, then the
    /// element destructor 0x538c30 over every action, `_Last = _First`),
    /// then the stores in instruction order `next_id = 0` (+0x20),
    /// `unknown_24 = 0x0fffffff` (+0x24), `on_duty = 0` (+0x2c byte),
    /// `speed_factor = 1.0` (+0x28), `customer = NULL` (+0x30), and the tail
    /// `JMP 0x53d9c0` ([`AiPoliceNastenka::init_items`]).
    pub fn zacatek_smeny(&mut self) {
        self.actions.clear();
        self.next_id = 0;
        self.unknown_24 = POLICE_SHIFT_START_UNKNOWN_24;
        self.on_duty = 0;
        self.speed_factor = f32::from_bits(POLICE_SHIFT_START_SPEED_FACTOR_BITS);
        self.customer = None;
        self.init_items();
    }

    /// FUNCTION: GAME VA 0x53d9c0, RVA 0x13d9c0 (`ai_animitem_manager::
    /// Init`, mafia-re jp_ai_prislusnici.cpp:149-153). REA ev_4150e27126,
    /// Ghidra body 0x53d9c0..0x53d9fb (60 bytes): the inlined
    /// `items.erase(begin, end)` with a trivially destructible 0x10-byte
    /// element, so the frames are neither unlinked nor released.
    pub fn init_items(&mut self) {
        self.items.clear();
    }

    pub fn items(&self) -> &[AiAnimItem] {
        &self.items
    }

    pub fn actions(&self) -> &[AiPoliceAction] {
        &self.actions
    }

    pub fn next_id(&self) -> u32 {
        self.next_id
    }

    pub fn unknown_24(&self) -> u32 {
        self.unknown_24
    }

    pub fn speed_factor(&self) -> f32 {
        self.speed_factor
    }

    pub fn on_duty(&self) -> u8 {
        self.on_duty
    }

    pub fn customer(&self) -> Option<ActorReferenceId> {
        self.customer
    }

    /// Test staging while the board's writers stay unported.
    #[cfg(test)]
    pub(crate) fn stage(
        &mut self,
        items: Vec<AiAnimItem>,
        actions: Vec<AiPoliceAction>,
        customer: Option<ActorReferenceId>,
    ) {
        self.items = items;
        self.actions = actions;
        self.next_id = 7;
        self.unknown_24 = 3;
        self.speed_factor = 0.5;
        self.on_duty = 1;
        self.customer = customer;
    }
}

/// One `ai_group_member` (8 bytes): the borrowed entity at +0x0 only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiGroupMember {
    pub entity: ActorReferenceId,
}

/// One `ai_group_event` (0x14 bytes): the borrowed entity (+0xc) and
/// attacker (+0x10) only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiGroupEvent {
    pub entity: Option<ActorReferenceId>,
    pub attacker: Option<ActorReferenceId>,
}

/// One `ai_group` (0x60 bytes): the five vectors its destructor (GAME
/// 0x531060, REA ev_75fcc4bc3a) frees, last first (+0x50, +0x40, +0x30,
/// +0x20, +0x10). Rust drops them in declaration order; the order of these
/// storage frees has no other observable effect.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiGroup {
    pub members: Vec<AiGroupMember>,
    pub attackers: Vec<ActorReferenceId>,
    pub events: Vec<AiGroupEvent>,
    pub cars: Vec<ActorReferenceId>,
    pub car_factors: Vec<f32>,
}

/// `ai_groups` (AIManager +0xc4, 0x28 bytes).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiGroups {
    /// +0x0, the game time `Tick` unlocks the player's controls at
    /// (compared unsigned); [`AI_GROUPS_NO_UNLOCK`] for none.
    unlock_time: u32,
    /// +0x4, the last group number (`NewGroup`'s; sign unproven).
    last_id: u32,
    /// +0x8.
    groups: Vec<AiGroup>,
    /// +0x18, the single owner of the `free_actors` vector.
    free_actors: AiFreeActorStore,
}

impl AiGroups {
    /// FUNCTION: GAME VA 0x531130, RVA 0x131130 (`ai_groups::Init`,
    /// mafia-re game/ai/jp_ai_groups.cpp:27-34). REA ev_4edd8145f3, Ghidra
    /// body 0x531130..0x53119c (109 bytes): `unlock_time = -1`, `last_id =
    /// 0`, then the inlined `groups.erase(begin, end)` (destructor 0x531060
    /// per 0x60-byte group) and `free_actors.erase(begin, end)` (4-byte
    /// borrowed pointers).
    pub fn init(&mut self) {
        self.unlock_time = AI_GROUPS_NO_UNLOCK;
        self.last_id = 0;
        self.groups.clear();
        self.free_actors.clear_for_init();
    }

    pub fn unlock_time(&self) -> u32 {
        self.unlock_time
    }

    pub fn last_id(&self) -> u32 {
        self.last_id
    }

    pub fn groups(&self) -> &[AiGroup] {
        &self.groups
    }

    pub fn free_actors(&self) -> &AiFreeActorStore {
        &self.free_actors
    }

    /// `ai_groups::AddFreeActor` (GAME 0x47b870) over the live owner.
    pub fn add_free_actor(&mut self, actor: ActorReferenceId) {
        self.free_actors.add_free_actor(actor);
    }

    /// Test staging while `NewGroup` and the `Tick` writers stay unported.
    #[cfg(test)]
    pub(crate) fn stage(&mut self, groups: Vec<AiGroup>, unlock_time: u32, last_id: u32) {
        self.groups = groups;
        self.unlock_time = unlock_time;
        self.last_id = last_id;
    }
}

/// One `ai_follow_group` (0x38 bytes): only the `followers` vector at +0x0
/// that its destructor (GAME 0x44b220, REA ev_f81679bd23, a folded vector
/// destructor freeing `[this+4]`) releases.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiFollowGroup {
    pub followers: Vec<ActorReferenceId>,
}

/// `ai_follow_manager` (AIManager +0xec, 0x14 bytes).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiFollowManager {
    /// +0x0.
    groups: Vec<AiFollowGroup>,
    /// +0x10, `Update`'s next look (ms).
    check_time: u32,
}

impl AiFollowManager {
    /// FUNCTION: GAME VA 0x532da0, RVA 0x132da0 (`ai_follow_manager::Init`,
    /// mafia-re game/ai/jp_ai_groups.cpp:337-342). REA ev_727dc08d05,
    /// Ghidra body 0x532da0..0x532e29 (138 bytes): `check_time = 0` (stored
    /// at 0x532dae before the erase), then the inlined `groups.erase(begin,
    /// end)` with the destructor 0x44b220 per 0x38-byte group.
    pub fn init(&mut self) {
        self.check_time = 0;
        self.groups.clear();
    }

    pub fn groups(&self) -> &[AiFollowGroup] {
        &self.groups
    }

    pub fn check_time(&self) -> u32 {
        self.check_time
    }

    /// Test staging while `AddFollower` and `Update` stay unported.
    #[cfg(test)]
    pub(crate) fn stage(&mut self, groups: Vec<AiFollowGroup>, check_time: u32) {
        self.groups = groups;
        self.check_time = check_time;
    }
}

/// One `ai_fight_member` (0x14 bytes): the borrowed entity at +0x0 only.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiFightMember {
    pub entity: ActorReferenceId,
}

/// One `ai_fight_followship` (0x48 bytes): only the `members` vector at
/// +0x18 that its destructor (GAME 0x538c30, folded with the police
/// actions') frees.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiFightFollowship {
    pub members: Vec<AiFightMember>,
}

/// `ai_fight_manager` (AIManager +0x100, 0x10 bytes).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AiFightManager {
    /// +0x0.
    parties: Vec<AiFightFollowship>,
}

impl AiFightManager {
    /// FUNCTION: GAME VA 0x534ec0, RVA 0x134ec0 (`ai_fight_manager::Init`,
    /// mafia-re game/ai/jp_ai_groups.cpp:393-397). REA ev_7b25d0f3f8, Ghidra
    /// body 0x534ec0..0x534f61 (162 bytes): the inlined `parties.erase(begin,
    /// end)` with the destructor 0x538c30 per 0x48-byte party.
    pub fn init(&mut self) {
        self.parties.clear();
    }

    pub fn parties(&self) -> &[AiFightFollowship] {
        &self.parties
    }

    /// Test staging while the fights' `Tick` stays unported.
    #[cfg(test)]
    pub(crate) fn stage(&mut self, parties: Vec<AiFightFollowship>) {
        self.parties = parties;
    }
}

/// The `AIManager` global (GAME 0x646c80, 0x130 bytes), represented by the
/// four members `C_game::Init` resets. `segments`, `wayfind` and `sight`
/// stay outside this owner; `AIManager.segments.Load` remains its own
/// unowned `Init` step.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiManager {
    /// +0x90.
    pub police: AiPoliceNastenka,
    /// +0xc4.
    pub groups: AiGroups,
    /// +0xec.
    pub follow: AiFollowManager,
    /// +0x100.
    pub fight: AiFightManager,
}

impl AiManager {
    /// The four calls of `C_game::Init` after `unknown_140.clear()` in
    /// Game.exe order (call sites 0x5a0996 `ZacatekSmeny`, 0x5a09a0
    /// `ai_groups::Init`, 0x5a09aa `ai_follow_manager::Init`, 0x5a09b4
    /// `ai_fight_manager::Init`; mafia-re game/mission/game.cpp:285-288).
    /// Working name for that span of the occupied `Init` entry; no function
    /// credit is taken here.
    pub fn game_init_police_begin_shift(&mut self) {
        self.police.zacatek_smeny();
        self.groups.init();
        self.follow.init();
        self.fight.init();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actor(identity: u32) -> ActorReferenceId {
        ActorReferenceId::from_existing(identity)
    }

    #[test]
    fn constructed_manager_holds_zero_scalars_and_empty_vectors() {
        // GAME 0x538760 constructs the vectors only; the global's scalars
        // stay zero until the first Init.
        let manager = AiManager::default();
        assert_eq!(manager.police.unknown_24(), 0);
        assert_eq!(manager.police.speed_factor().to_bits(), 0);
        assert_eq!(manager.groups.unlock_time(), 0);
        assert_eq!(manager.follow.check_time(), 0);
        assert!(manager.police.actions().is_empty());
        assert!(manager.fight.parties().is_empty());
    }

    #[test]
    fn zacatek_smeny_resets_board_and_tail_clears_items_without_release() {
        let mut board = AiPoliceNastenka::default();
        let frame = FrameKey(9);
        board.stage(
            vec![AiAnimItem { frame }],
            vec![AiPoliceAction {
                members: vec![actor(1), actor(2)],
            }],
            Some(actor(3)),
        );
        board.zacatek_smeny();
        assert!(board.actions().is_empty());
        assert!(board.items().is_empty());
        assert_eq!(board.next_id(), 0);
        assert_eq!(board.unknown_24(), 0x0fff_ffff);
        assert_ne!(board.unknown_24(), u32::MAX);
        assert_eq!(board.on_duty(), 0);
        assert_eq!(board.speed_factor().to_bits(), 0x3f80_0000);
        assert_eq!(board.customer(), None);
    }

    #[test]
    fn items_init_leaves_actions_and_scalars() {
        let mut board = AiPoliceNastenka::default();
        board.stage(
            vec![AiAnimItem { frame: FrameKey(1) }],
            vec![AiPoliceAction::default()],
            None,
        );
        board.init_items();
        assert!(board.items().is_empty());
        assert_eq!(board.actions().len(), 1);
        assert_eq!(board.next_id(), 7);
        assert_eq!(board.on_duty(), 1);
    }

    #[test]
    fn groups_init_sets_no_unlock_and_empties_both_vectors() {
        let mut groups = AiGroups::default();
        groups.stage(
            vec![AiGroup {
                members: vec![AiGroupMember { entity: actor(4) }],
                attackers: vec![actor(5)],
                events: vec![AiGroupEvent {
                    entity: Some(actor(4)),
                    attacker: Some(actor(5)),
                }],
                cars: vec![actor(6)],
                car_factors: vec![1.0],
            }],
            1234,
            3,
        );
        groups.add_free_actor(actor(8));
        groups.init();
        assert_eq!(groups.unlock_time(), u32::MAX);
        assert_eq!(groups.last_id(), 0);
        assert!(groups.groups().is_empty());
        assert!(groups.free_actors().is_empty());
    }

    #[test]
    fn follow_and_fight_init_empty_their_records() {
        let mut follow = AiFollowManager::default();
        follow.stage(
            vec![AiFollowGroup {
                followers: vec![actor(1)],
            }],
            500,
        );
        follow.init();
        assert_eq!(follow.check_time(), 0);
        assert!(follow.groups().is_empty());

        let mut fight = AiFightManager::default();
        fight.stage(vec![AiFightFollowship {
            members: vec![AiFightMember { entity: actor(2) }],
        }]);
        fight.init();
        assert!(fight.parties().is_empty());
    }

    #[test]
    fn police_begin_shift_runs_all_four_members() {
        let mut manager = AiManager::default();
        manager
            .police
            .stage(vec![], vec![AiPoliceAction::default()], Some(actor(1)));
        manager.groups.stage(vec![AiGroup::default()], 9, 9);
        manager.follow.stage(vec![AiFollowGroup::default()], 9);
        manager.fight.stage(vec![AiFightFollowship::default()]);
        manager.game_init_police_begin_shift();
        assert!(manager.police.actions().is_empty());
        assert_eq!(manager.police.speed_factor(), 1.0);
        assert_eq!(manager.groups.unlock_time(), AI_GROUPS_NO_UNLOCK);
        assert!(manager.groups.groups().is_empty());
        assert!(manager.follow.groups().is_empty());
        assert!(manager.fight.parties().is_empty());
    }
}
