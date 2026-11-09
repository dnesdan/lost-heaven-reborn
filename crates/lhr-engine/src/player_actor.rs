//! Constructor-only C_player ownership. Game08398317... ctor58f170,
//! destructor58f220; deleting wrapper58f200. No Player GameInit/control policy.
//! PLAYER_LOCKCONTROLS (mafia-re 113b5b6 game/mission/C_program.cpp:4172)
//! LockControls GAME 0x595f00 stores its bool here; Do_Breath stays DEFERRED
//! (needs player animation/breath owner, no new RVAs). The navigation point
//! (SetPlayerNaviPoint GAME 0x58f160) and the fade words (FadeReset GAME
//! 0x5961b0 with the SetAlpha GAME 0x595e10 call DEFERRED) are staged here.
#![forbid(unsafe_code)]
use crate::human_actor::{EmptyHumanCollection, HumanSeed, HumanState};
use crate::script_scheduler::FrameReferenceId;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlayerSeed {
    pub human: HumanSeed,
    pub allocator_tag: u8,
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
}
