//! Constructor-only C_player ownership. Game08398317... ctor58f170,
//! destructor58f220; deleting wrapper58f200. No Player GameInit/control policy.
//! PLAYER_LOCKCONTROLS (mafia-re 113b5b6 game/mission/C_program.cpp:4172)
//! LockControls GAME 0x595f00 stores its bool here; Do_Breath stays DEFERRED
//! (needs player animation/breath owner, no new RVAs). The navigation point
//! (SetPlayerNaviPoint GAME 0x58f160) and the fade words (FadeReset GAME
//! 0x5961b0 with the SetAlpha GAME 0x595e10 call DEFERRED) are staged here.
//! Follow-up slice (mafia-re fe7e746): small C_player getters, guards and
//! flag transitions below; scene/device effects stay with their owners.
#![forbid(unsafe_code)]
use crate::human_actor::{EmptyHumanCollection, HumanSeed, HumanState};
use crate::script_scheduler::FrameReferenceId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSeed {
    pub human: HumanSeed,
    pub allocator_tag: u8,
}

/// Packed C_player save payload after the human base (mafia-re fe7e746
/// game/actors/C_player.h:18-33): version, controls, turn words, fade
/// pair, recoil, aim time and three flag bytes.
pub const PLAYER_SAVEGAME_BYTES: usize = 0x22;
/// Aim hold written by ShootDeAimed (mafia-re fe7e746
/// game/actors/C_player.cpp:1517-1530).
pub const SHOOT_AIM_TIME: i32 = 3000;
/// Console text id base added to the item id by ShowWeaponName (mafia-re
/// fe7e746 game/actors/C_player.cpp:1749-1762).
pub const WEAPON_TEXT_BASE: u32 = 0xce4;

/// Minimal driven-car projection for UpdateControls: whether the entered
/// vehicle is a car and which seat the player took (0 drives).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DrivenCarRef {
    /// True only for a vehicle of car type (native ACTOR_CAR check).
    pub is_car: bool,
    /// Seat the player took; 0 drives (native `!seat`).
    pub seat: i32,
}
#[derive(Debug)]
pub struct PlayerState {
    pub(crate) constructor_words: [u32; 2],
    pub(crate) collection: EmptyHumanCollection,
    controls_locked: bool,
    /// Card-damage HUD latch (mafia-re 113b5b6 game/actors/C_player.h:86-87:
    /// static bool can_car_damage, cleared by C_game::Init and set by the
    /// script's CARDAMAGEVISIBLE; SetCanCarDamage GAME 0x47b860 writes it).
    /// Native storage is a C_player static; this stages the per-player
    /// projection (the single-player game owns one player). Starts cleared,
    /// matching the C_game::Init clear; the HUD consumer stays DEFERRED.
    can_car_damage: bool,
    /// Navigation-point frame (mafia-re 113b5b6 game/actors/C_player.h:89:
    /// static I3D_frame *navi_frame, a frame or NULL). Native storage is a
    /// C_player.cpp static written by SetPlayerNaviPoint; this stages the
    /// per-player projection. Starts at none; the compass consumer stays
    /// DEFERRED.
    navi_frame: Option<FrameReferenceId>,
    /// The player's alpha and the one it fades to (mafia-re 113b5b6
    /// game/actors/C_player.h:67-70: 0xab8 fade_alpha and 0xabc fade_target,
    /// 1 from C_game::Init and FadeReset). Written by the FadeReset core of
    /// this slice; the SetAlpha consumer (GAME 0x595e10, frame owner) stays
    /// DEFERRED. Starts at the fully-visible unit pair (working deterministic
    /// default: the C_player constructor at GAME 0x58f170 is an unowned
    /// STUB, so no original default is evidenced on this path).
    fade_alpha: f32,
    fade_target: f32,
    /// Shot recoil staged by ShootDeAimed (mafia-re fe7e746
    /// game/actors/C_player.h:133-135: 0xad0, decayed by AI, 0 from GameInit).
    recoil: f32,
    /// Time the weapon stays aimed after a shot or strike (mafia-re fe7e746
    /// game/actors/C_player.h:136-138: 0xad4, 3000 from ShootDeAimed, 0 from
    /// GameInit; the AI countdown stays DEFERRED).
    aim_time: i32,
    /// Struck car whose damage the HUD shows (mafia-re fe7e746
    /// game/actors/C_player.h:177-180: 0xadc time, 0xae0 car; opaque native
    /// identity, None for NULL; the HUD bar effect stays with its owner).
    car_damage_time: i32,
    damage_car: Option<u32>,
    /// Footstep horn sound played by AI_walk (mafia-re fe7e746
    /// game/actors/C_player.h:173-176: 0xaf8, NULL from GameInit; opaque
    /// native identity; stop/release stays with the sound owner).
    horn_sound: Option<u32>,
    /// Weapon whose name ShowWeaponName displayed last (mafia-re fe7e746
    /// game/actors/C_player.h:162-164: 0xaf0, -1 from GameInit; the console
    /// write stays with the HUD owner).
    shown_weapon: i32,
}
impl PlayerState {
    pub(crate) fn construct(allocator_tag: u8, human: &mut HumanState) -> Self {
        for component in 2..=5 {
            human.properties.set_component_bits(component, 0x4220_0000);
        }
        human.properties.set_component_bits(1, 0x4348_0000);
        human.properties.set_component_bits(10, 0x3f80_0000);
        human.voice_index = 3;
        Self {
            constructor_words: [0, 200],
            collection: EmptyHumanCollection::construct(allocator_tag),
            controls_locked: false,
            can_car_damage: false,
            navi_frame: None,
            fade_alpha: 1.0,
            fade_target: 1.0,
            recoil: 0.0,
            aim_time: 0,
            car_damage_time: -1,
            damage_car: None,
            horn_sound: None,
            shown_weapon: -1,
        }
    }
    pub fn constructor_words(&self) -> &[u32; 2] {
        &self.constructor_words
    }
    pub fn allocator_tag(&self) -> u8 {
        self.collection.allocator_tag
    }
    /// Original C_player lock word for PLAYER_LOCKCONTROLS.
    pub fn controls_locked(&self) -> bool {
        self.controls_locked
    }
    /// Original LockControls(bool) store; Do_Breath is not run here.
    pub fn set_controls_locked(&mut self, locked: bool) {
        self.controls_locked = locked;
    }
    /// GAME 0x47b860 (RVA 0x7b860) C_player::SetCanCarDamage (mafia-re
    /// 113b5b6 game/actors/C_player.h:92-96): store the card-damage HUD
    /// latch.
    pub fn set_can_car_damage(&mut self, on: bool) {
        self.can_car_damage = on;
    }
    /// Card-damage HUD latch staged by SetCanCarDamage, cleared at start.
    pub fn can_car_damage(&self) -> bool {
        self.can_car_damage
    }
    /// GAME 0x58f160 (RVA 0x18f160) SetPlayerNaviPoint (mafia-re 113b5b6
    /// game/actors/C_player.cpp:16-19): store the navigation point, None for
    /// NULL. Native storage is the C_player.cpp static; the compass setter
    /// and C_game::Init clearer stay DEFERRED with the mission owner.
    pub fn set_player_navi_point(&mut self, frame: Option<FrameReferenceId>) {
        self.navi_frame = frame;
    }
    /// Navigation point staged by SetPlayerNaviPoint, None for NULL.
    pub fn navi_frame(&self) -> Option<FrameReferenceId> {
        self.navi_frame
    }
    /// GAME 0x5961b0 (RVA 0x1961b0) C_player::FadeReset (mafia-re 113b5b6
    /// game/actors/C_player.cpp:59-64): restore fade_alpha and fade_target to
    /// 1.0 (fully visible). The trailing SetAlpha(fade_alpha) call (GAME
    /// 0x595e10) stays with the frame/alpha owner (DEFERRED) with an explicit
    /// marker; no alpha effect happens here.
    pub fn apply_fade_reset(&mut self) {
        self.fade_alpha = 1.0;
        self.fade_target = 1.0;
    }
    /// GAME 0x595440 (RVA 0x195440) C_player::SetSniperMode EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_player.h:51,
    /// game/actors/C_player.cpp:40-43: body not reconstructed, STUB): the
    /// sniper-mode latch and its camera/aim owners do not exist here, so no
    /// state is touched and no owner is filed; the stub exists only to pin
    /// the address while those owners stay DEFERRED.
    pub fn set_sniper_mode_unsupported(_on: bool) {}
    /// GAME 0x595e10 (RVA 0x195e10) C_player::SetAlpha EXPLICIT UNSUPPORTED
    /// STUB (mafia-re 113b5b6 game/actors/C_player.h:55,
    /// game/actors/C_player.cpp:45-49: body not reconstructed, STUB): the
    /// frame alpha effect belongs to the frame owner, which does not exist
    /// here, so the staged fade words above are left untouched and no owner
    /// is filed; the stub exists only to pin the address while that owner
    /// stays DEFERRED.
    pub fn set_alpha_unsupported(_alpha: f32) {}
    /// GAME 0x5952f0 (RVA 0x1952f0) C_player::IndicatorSetAimed EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_player.h:58,
    /// game/actors/C_player.cpp:33-37: body not reconstructed, STUB): the HUD
    /// indicator owner does not exist here, so no state is touched and no
    /// owner is filed; the stub exists only to pin the address while that
    /// owner stays DEFERRED.
    pub fn indicator_set_aimed_unsupported() {}
    /// GAME 0x595c30 (RVA 0x195c30) C_player::SetCarDamage EXPLICIT
    /// UNSUPPORTED STUB (mafia-re 113b5b6 game/actors/C_player.h:62,
    /// game/actors/C_player.cpp:157-161: body not reconstructed, STUB): the
    /// car argument plus the card-damage HUD/model owners do not exist here,
    /// so no state is touched and no owner is filed; the stub exists only to
    /// pin the address while those owners stay DEFERRED.
    pub fn set_car_damage_unsupported() {}
    /// Player alpha staged by FadeReset (1.0 fully visible).
    pub fn fade_alpha(&self) -> f32 {
        self.fade_alpha
    }
    /// Fade target staged by FadeReset (1.0 fully visible).
    pub fn fade_target(&self) -> f32 {
        self.fade_target
    }
    /// Current shot recoil staged by ShootDeAimed.
    pub fn recoil(&self) -> f32 {
        self.recoil
    }
    /// Time the weapon stays aimed.
    pub fn aim_time(&self) -> i32 {
        self.aim_time
    }
    /// GAME 0x58f2e0 (RVA 0x18f2e0) C_player::SaveGameGetSize (mafia-re
    /// fe7e746 game/actors/C_player.cpp:111-116): the human base size plus
    /// the packed player payload, 0x22 bytes (mafia-re fe7e746
    /// game/actors/C_player.h:18-33).
    pub fn savegame_size(base_size: usize) -> usize {
        base_size + PLAYER_SAVEGAME_BYTES
    }
    /// GAME 0x594180 (RVA 0x194180) C_player::GameDone horn shutdown
    /// (mafia-re fe7e746 game/actors/C_player.cpp:1190-1200): hands the
    /// horn sound out and clears the slot. The caller (sound owner) stops
    /// it and releases it in that order; the C_human::GameDone call stays
    /// DEFERRED with the human owner.
    pub fn take_horn_sound_for_shutdown(&mut self) -> Option<u32> {
        self.horn_sound.take()
    }
    /// GAME 0x5941c0 (RVA 0x1941c0) C_player::Collision lock guard
    /// (mafia-re fe7e746 game/actors/C_player.cpp:1203-1210): while the
    /// controls are locked the hit lands only where the police predicate
    /// allows it; otherwise the human base handles it (DEFERRED).
    pub fn collision_blocked_by_lock(controls_locked: bool, police_permits_hit: bool) -> bool {
        controls_locked && !police_permits_hit
    }
    /// GAME 0x594210 (RVA 0x194210) C_player::Hit lives projection
    /// (mafia-re fe7e746 game/actors/C_player.cpp:1214-1230): energy as a
    /// percent of the maximum, forced to at least 1 while alive. Requires
    /// `max_energy > 0` (200.0 from GameInit). The sniper-mode exit and the
    /// HUD follow stay with the mission/HUD owners (DEFERRED).
    pub fn player_hit_lives(energy: f32, max_energy: f32, alive: bool) -> u32 {
        let lives = (energy / max_energy * 100.0) as i32;
        if alive && lives < 1 {
            1
        } else {
            lives as u32
        }
    }
    /// GAME 0x5942c0 (RVA 0x1942c0) C_player::UpdateControls (mafia-re
    /// fe7e746 game/actors/C_player.cpp:1233-1238): steering linearity
    /// applies only while the game runs and the player sits in a driven
    /// car (seat 0). The SetSteeringLinearity write stays with the
    /// car/settings owners (DEFERRED).
    pub fn should_apply_steering_linearity(
        game_initialized: bool,
        car: Option<DrivenCarRef>,
    ) -> bool {
        match car {
            Some(current) => game_initialized && current.is_car && current.seat == 0,
            None => false,
        }
    }
    /// GAME 0x595390 (RVA 0x195390) C_player::ShootDeAimed recoil core
    /// (mafia-re fe7e746 game/actors/C_player.cpp:1517-1530): the shot
    /// adds the weapon recoil (a tenth in sniper mode) and resamples
    /// through one uniform draw on overflow past 1.0. `rng_unit` is that
    /// single [0, 1] sample, consumed only on overflow. The aim hold,
    /// HUD accuracy, camera recoil and fire event stay DEFERRED.
    pub fn shoot_recoil_next(
        recoil: f32,
        weapon_recoil: f32,
        sniper_mode: bool,
        rng_unit: f32,
    ) -> f32 {
        let mut next = recoil
            + if sniper_mode {
                weapon_recoil * 0.1
            } else {
                weapon_recoil
            };
        if next > 1.0 {
            next = rng_unit * 0.2 + 0.8;
        }
        next
    }
    /// HUD accuracy from the recoil (ShootDeAimed tail): `recoil * 3 + 1`.
    pub fn shoot_accuracy(recoil: f32) -> f32 {
        recoil * 3.0 + 1.0
    }
    /// Applies the ShootDeAimed recoil core: holds the aim for
    /// SHOOT_AIM_TIME and stages the new recoil.
    pub fn apply_shoot_recoil(&mut self, weapon_recoil: f32, sniper_mode: bool, rng_unit: f32) {
        self.aim_time = SHOOT_AIM_TIME;
        self.recoil = Self::shoot_recoil_next(self.recoil, weapon_recoil, sniper_mode, rng_unit);
    }
    /// GAME 0x595c60 (RVA 0x195c60) C_player::InvalidateCarDamage
    /// (mafia-re fe7e746 game/actors/C_player.cpp:1671-1681): forgets the
    /// struck car when it is gone. Returns the HUD-bar hide flag for the
    /// HUD owner only when a watched car matched (`None` changes nothing);
    /// `driving_car_type_is_car` must be true only while the player
    /// currently drives a car.
    pub fn invalidate_car_damage(
        &mut self,
        car: u32,
        driving_car_type_is_car: bool,
    ) -> Option<bool> {
        if car != 0 && self.damage_car == Some(car) {
            self.damage_car = None;
            self.car_damage_time = -1;
            Some(!driving_car_type_is_car)
        } else {
            None
        }
    }
    /// GAME 0x596090 (RVA 0x196090) C_player::SetBoxer EXPLICIT
    /// UNSUPPORTED STUB (mafia-re fe7e746 game/actors/C_player.cpp:1824-1843):
    /// scales the r_hand/l_hand frames; the frame owner with
    /// FindChildFrame/SetScale/Update does not exist here, so no state is
    /// touched and no owner is filed.
    pub fn set_boxer_unsupported(_on: bool) {}
    /// GAME 0x595e70 (RVA 0x195e70) C_player::ShowWeaponName (mafia-re
    /// fe7e746 game/actors/C_player.cpp:1749-1762): reports the console
    /// text id once per weapon. `None` means no console update (same
    /// weapon, or an empty hand in a driven car that holds an item); the
    /// console write stays with the HUD owner.
    pub fn poll_weapon_name(
        &mut self,
        current_item: i32,
        right_hand_id: u32,
        car_item_id: u32,
        in_driven_car: bool,
    ) -> Option<u32> {
        if current_item == self.shown_weapon {
            return None;
        }
        self.shown_weapon = current_item;
        if in_driven_car && right_hand_id == 0 && car_item_id != 0 {
            None
        } else {
            Some((current_item as u32).wrapping_add(WEAPON_TEXT_BASE))
        }
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;
    use crate::human_actor::{HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties};
    fn human_seed() -> HumanSeed {
        HumanSeed {
            allocator_tag: 0xd7,
            animation_machine: HumanAnimationMachineSeed {
                untouched_word: 0xa5a5_a5a5,
            },
            previous_animation_code: 0,
            requested_animation_code: 0,
            human_type_mask: 0,
            current_properties: HumanProperties::from_record([0xa5; 64]),
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
    fn lock_flag_starts_unlocked_and_stores_bool() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert!(!player.controls_locked());
        player.set_controls_locked(true);
        assert!(player.controls_locked());
        player.set_controls_locked(false);
        assert!(!player.controls_locked());
    }
    #[test]
    // GAME 0x47b860 (RVA 0x7b860) C_player::SetCanCarDamage: the latch
    // starts cleared like the C_game::Init clear and stores the bool.
    fn can_car_damage_rva_7b860_starts_cleared_and_stores_bool() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert!(!player.can_car_damage());
        player.set_can_car_damage(true);
        assert!(player.can_car_damage());
        player.set_can_car_damage(false);
        assert!(!player.can_car_damage());
    }
    #[test]
    // GAME 0x58f160 (RVA 0x18f160) SetPlayerNaviPoint (mafia-re 113b5b6
    // game/actors/C_player.cpp:16-19): the point starts at none (NULL) and
    // round-trips through the setter.
    fn navi_point_rva_18f160_starts_none_and_stores_frame() {
        use crate::script_scheduler::FrameReferenceId;
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert_eq!(player.navi_frame(), None);
        player.set_player_navi_point(Some(FrameReferenceId::from_existing(7)));
        assert_eq!(
            player.navi_frame(),
            Some(FrameReferenceId::from_existing(7))
        );
        player.set_player_navi_point(None);
        assert_eq!(player.navi_frame(), None);
    }
    #[test]
    // GAME 0x5961b0 (RVA 0x1961b0) C_player::FadeReset (mafia-re 113b5b6
    // game/actors/C_player.cpp:59-64): both fade words restore to 1.0; the
    // SetAlpha call stays with the frame owner.
    fn fade_reset_rva_1961b0_restores_unit_fades() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert_eq!((player.fade_alpha(), player.fade_target()), (1.0, 1.0));
        player.fade_alpha = 0.25;
        player.fade_target = 0.0;
        player.apply_fade_reset();
        assert_eq!((player.fade_alpha(), player.fade_target()), (1.0, 1.0));
    }
    #[test]
    // GAME 0x595440 (RVA 0x195440) C_player::SetSniperMode (mafia-re 113b5b6
    // game/actors/C_player.cpp:40-43): body not reconstructed (STUB);
    // invoking it touches no staged words.
    fn sniper_mode_rva_195440_stub_runs_without_state() {
        PlayerState::set_sniper_mode_unsupported(true);
        PlayerState::set_sniper_mode_unsupported(false);
    }
    #[test]
    // GAME 0x595e10 (RVA 0x195e10) C_player::SetAlpha (mafia-re 113b5b6
    // game/actors/C_player.cpp:45-49): body not reconstructed (STUB);
    // invoking it leaves the staged fade words untouched.
    fn set_alpha_rva_195e10_stub_preserves_fade_words() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        PlayerState::set_alpha_unsupported(0.0);
        assert_eq!((player.fade_alpha(), player.fade_target()), (1.0, 1.0));
        player.apply_fade_reset();
        PlayerState::set_alpha_unsupported(1.0);
        assert_eq!((player.fade_alpha(), player.fade_target()), (1.0, 1.0));
    }
    #[test]
    // GAME 0x5952f0 (RVA 0x1952f0) C_player::IndicatorSetAimed (mafia-re
    // 113b5b6 game/actors/C_player.cpp:33-37): body not reconstructed
    // (STUB); invoking it touches no staged words.
    fn indicator_aimed_rva_1952f0_stub_runs_without_state() {
        PlayerState::indicator_set_aimed_unsupported();
    }
    #[test]
    // GAME 0x595c30 (RVA 0x195c30) C_player::SetCarDamage (mafia-re 113b5b6
    // game/actors/C_player.cpp:157-161): body not reconstructed (STUB);
    // invoking it touches no staged words.
    fn car_damage_rva_195c30_stub_runs_without_state() {
        PlayerState::set_car_damage_unsupported();
    }
    #[test]
    // GAME 0x58f2e0 (RVA 0x18f2e0) C_player::SaveGameGetSize (mafia-re
    // fe7e746 game/actors/C_player.cpp:111-116): base size plus the 0x22
    // packed payload (C_player.h:18-33).
    fn savegame_size_rva_18f2e0_adds_packed_payload() {
        assert_eq!(super::PLAYER_SAVEGAME_BYTES, 0x22);
        assert_eq!(PlayerState::savegame_size(0), 0x22);
        assert_eq!(PlayerState::savegame_size(64), 64 + 0x22);
    }
    #[test]
    // GAME 0x594180 (RVA 0x194180) C_player::GameDone horn shutdown
    // (mafia-re fe7e746 game/actors/C_player.cpp:1190-1200): the staged
    // slot clears and the handle goes out for the sound owner.
    fn horn_shutdown_rva_194180_clears_slot_and_hands_handle_out() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert_eq!(player.take_horn_sound_for_shutdown(), None);
        player.horn_sound = Some(7);
        assert_eq!(player.take_horn_sound_for_shutdown(), Some(7));
        assert_eq!(player.horn_sound, None);
    }
    #[test]
    // GAME 0x5941c0 (RVA 0x1941c0) C_player::Collision lock guard
    // (mafia-re fe7e746 game/actors/C_player.cpp:1203-1210): locked
    // controls block the hit unless the police predicate allows it.
    fn collision_rva_1941c0_locked_only_without_police_permit() {
        assert!(!PlayerState::collision_blocked_by_lock(false, false));
        assert!(!PlayerState::collision_blocked_by_lock(false, true));
        assert!(PlayerState::collision_blocked_by_lock(true, false));
        assert!(!PlayerState::collision_blocked_by_lock(true, true));
    }
    #[test]
    // GAME 0x594210 (RVA 0x194210) C_player::Hit lives projection
    // (mafia-re fe7e746 game/actors/C_player.cpp:1214-1230): percent of
    // max energy, at least 1 while alive.
    fn hit_lives_rva_194210_percent_clamped_to_one_while_alive() {
        assert_eq!(PlayerState::player_hit_lives(150.0, 200.0, true), 75);
        assert_eq!(PlayerState::player_hit_lives(200.0, 200.0, true), 100);
        assert_eq!(PlayerState::player_hit_lives(0.0, 200.0, true), 1);
        assert_eq!(PlayerState::player_hit_lives(0.0, 200.0, false), 0);
    }
    #[test]
    // GAME 0x5942c0 (RVA 0x1942c0) C_player::UpdateControls (mafia-re
    // fe7e746 game/actors/C_player.cpp:1233-1238): linearity applies only
    // while the game runs and the player drives a car.
    fn steering_linearity_rva_1942c0_only_for_driven_car() {
        use super::DrivenCarRef;
        let car = Some(DrivenCarRef {
            is_car: true,
            seat: 0,
        });
        assert!(PlayerState::should_apply_steering_linearity(true, car));
        assert!(!PlayerState::should_apply_steering_linearity(false, car));
        assert!(!PlayerState::should_apply_steering_linearity(true, None));
        assert!(!PlayerState::should_apply_steering_linearity(
            true,
            Some(DrivenCarRef {
                is_car: false,
                seat: 0,
            })
        ));
        assert!(!PlayerState::should_apply_steering_linearity(
            true,
            Some(DrivenCarRef {
                is_car: true,
                seat: 1,
            })
        ));
    }
    #[test]
    // GAME 0x595390 (RVA 0x195390) C_player::ShootDeAimed recoil core
    // (mafia-re fe7e746 game/actors/C_player.cpp:1517-1530): weapon recoil
    // adds (a tenth in sniper mode), overflow resamples through one draw,
    // aim holds SHOOT_AIM_TIME.
    fn shoot_recoil_rva_195390_sniper_scales_and_overflow_resamples() {
        assert_eq!(super::SHOOT_AIM_TIME, 3000);
        assert_eq!(PlayerState::shoot_recoil_next(0.25, 0.5, false, 0.0), 0.75);
        assert_eq!(PlayerState::shoot_recoil_next(0.0, 2.0, true, 0.0), 0.2);
        assert_eq!(PlayerState::shoot_recoil_next(4.0, 4.0, false, 0.0), 0.8);
        assert_eq!(PlayerState::shoot_recoil_next(4.0, 4.0, false, 1.0), 1.0);
        assert_eq!(PlayerState::shoot_accuracy(0.75), 3.25);
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        player.apply_shoot_recoil(0.5, false, 0.0);
        assert_eq!((player.aim_time(), player.recoil()), (3000, 0.5));
    }
    #[test]
    // GAME 0x595c60 (RVA 0x195c60) C_player::InvalidateCarDamage
    // (mafia-re fe7e746 game/actors/C_player.cpp:1671-1681): a watched
    // car clears the slot and reports the HUD hide flag; other cars
    // change nothing.
    fn invalidate_damage_rva_195c60_clears_match_and_reports_hide() {
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert_eq!(player.invalidate_car_damage(9, false), None);
        player.damage_car = Some(9);
        player.car_damage_time = 3000;
        assert_eq!(player.invalidate_car_damage(4, false), None);
        assert_eq!(player.damage_car, Some(9));
        assert_eq!(player.invalidate_car_damage(9, false), Some(true));
        assert_eq!((player.damage_car, player.car_damage_time), (None, -1));
        player.damage_car = Some(9);
        player.car_damage_time = 3000;
        assert_eq!(player.invalidate_car_damage(9, true), Some(false));
        assert_eq!((player.damage_car, player.car_damage_time), (None, -1));
        assert_eq!(player.invalidate_car_damage(0, false), None);
    }
    #[test]
    // GAME 0x596090 (RVA 0x196090) C_player::SetBoxer (mafia-re fe7e746
    // game/actors/C_player.cpp:1824-1843): frame owner missing (STUB);
    // invoking it touches no staged words.
    fn boxer_rva_196090_stub_runs_without_state() {
        PlayerState::set_boxer_unsupported(true);
        PlayerState::set_boxer_unsupported(false);
    }
    #[test]
    // GAME 0x595e70 (RVA 0x195e70) C_player::ShowWeaponName (mafia-re
    // fe7e746 game/actors/C_player.cpp:1749-1762): the text id goes out
    // once per weapon; an empty hand in a car holding an item stays quiet.
    fn weapon_name_rva_195e70_reports_once_per_weapon() {
        use super::WEAPON_TEXT_BASE;
        let mut human = HumanState::construct(human_seed());
        let mut player = PlayerState::construct(0xd7, &mut human);
        assert_eq!(
            player.poll_weapon_name(5, 1, 0, false),
            Some(5 + WEAPON_TEXT_BASE)
        );
        assert_eq!(player.poll_weapon_name(5, 1, 0, false), None);
        assert_eq!(player.poll_weapon_name(6, 0, 9, true), None);
        assert_eq!(player.poll_weapon_name(6, 0, 9, true), None);
        assert_eq!(
            player.poll_weapon_name(7, 1, 9, true),
            Some(7 + WEAPON_TEXT_BASE)
        );
        assert_eq!(
            player.poll_weapon_name(8, 0, 0, true),
            Some(8 + WEAPON_TEXT_BASE)
        );
    }
}
