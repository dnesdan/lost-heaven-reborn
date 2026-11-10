//! C_airplane small-function ports for the original PC actor lifecycle.
//!
//! Game.exe SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Reference: mafia-re at pin fe7e746, game/vehicles/C_airplane.h (vtable VA
//! 0x623318, 0x3f0-byte object, engine energies at +0x1c8/+0x1cc) and
//! game/vehicles/C_airplane.cpp (FUNCTION/STUB markers name Game.exe code).
//! Only the three FUNCTION bodies with local semantics are reconstructed; the
//! remaining small entries below are explicit unsupported boundaries because
//! the flight-model, HUD and base-chunk owners stay outside this contract.

/// Extra save-game bytes written by `C_airplane` past the base `C_actor`
/// payload.
pub const AIRPLANE_SAVE_GAME_EXTRA: i32 = 124;

/// `C_airplane::SaveGameGetSize` entry, GAME RVA 0xd6f0 (VA 0x40d6f0).
pub const AIRPLANE_SAVE_GAME_GET_SIZE_ENTRY: u32 = 0x40d6f0;

/// Original `C_airplane::SaveGameGetSize`, GAME RVA 0xd6f0 (VA 0x40d6f0;
/// mafia-re game/vehicles/C_airplane.cpp:104-108 at pin fe7e746; `FUNCTION: GAME`
/// code in Game.exe, not a data offset and not another binary): the base size
/// plus 124. 32-bit wrapping matches the native `int` addition.
pub fn airplane_save_game_size(base_size: i32) -> i32 {
    base_size.wrapping_add(AIRPLANE_SAVE_GAME_EXTRA)
}

/// Material index returned by `C_airplane::GetMaterialID`: a materials.def
/// index for metal structures (mafia-re game/g_material.h:13 at pin fe7e746
/// defines `MAT_METAL_STRUCTURE 0x13`).
pub const AIRPLANE_MATERIAL_ID: i32 = 0x13;

/// `C_airplane::GetMaterialID` entry, GAME RVA 0xdf30 (VA 0x40df30).
pub const AIRPLANE_GET_MATERIAL_ID_ENTRY: u32 = 0x40df30;

/// Original `C_airplane::GetMaterialID`, GAME RVA 0xdf30 (VA 0x40df30;
/// mafia-re game/vehicles/C_airplane.cpp:131-135 at pin fe7e746; `FUNCTION: GAME`
/// code in Game.exe, not a data offset and not another binary): returns
/// `MAT_METAL_STRUCTURE` and ignores the frame. The frame owner stays outside
/// this contract, so no frame parameter is staged.
pub fn airplane_material_id() -> i32 {
    AIRPLANE_MATERIAL_ID
}

/// The two engine energies read by `C_airplane::IsDestroyed` (mafia-re
/// game/vehicles/C_airplane.h:42-45 at pin fe7e746: `engine_energy_1` at
/// +0x1c8, `engine_energy_2` at +0x1cc). Only this pair is modeled; the
/// surrounding 0x3f0-byte object stays caller-owned.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AirplaneEngines {
    pub energy_1: f32,
    pub energy_2: f32,
}

impl AirplaneEngines {
    pub fn new(energy_1: f32, energy_2: f32) -> Self {
        Self {
            energy_1,
            energy_2,
        }
    }
}

/// `C_airplane::IsDestroyed` entry, GAME RVA 0xd6c0 (VA 0x40d6c0).
pub const AIRPLANE_IS_DESTROYED_ENTRY: u32 = 0x40d6c0;

/// Original `C_airplane::IsDestroyed`, GAME RVA 0xd6c0 (VA 0x40d6c0;
/// mafia-re game/vehicles/C_airplane.cpp:28-33 at pin fe7e746; `FUNCTION: GAME`
/// code in Game.exe, not a data offset and not another binary): true when both
/// engine energies are below 0.01, checking `engine_energy_2` first. Each `f32`
/// is promoted to `f64` for the comparison, matching the native
/// float-against-double-constant comparison.
pub fn airplane_is_destroyed(engines: &AirplaneEngines) -> bool {
    f64::from(engines.energy_2) < 0.01 && f64::from(engines.energy_1) < 0.01
}

/// Explicit boundary for the still-unimplemented small airplane entries below.
/// The entries name the original PC addresses (VA); the flight-model, HUD and
/// base-chunk owners stay outside this contract, so each reports the boundary
/// instead of staging a value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AirplaneStubError {
    Unsupported { entry: u32 },
}

/// `C_airplane::Start` entry, GAME RVA 0xc610 (VA 0x40c610; mafia-re
/// game/vehicles/C_airplane.cpp:16-20 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): takeoff, whose speed
/// the scripts' AIRPLANE_START passes in m/s.
pub const AIRPLANE_START_ENTRY: u32 = 0x40c610;

/// `C_airplane::Start2` entry, GAME RVA 0xc7f0 (VA 0x40c7f0; mafia-re
/// game/vehicles/C_airplane.cpp:22-26 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): the scripts'
/// AIRPLANE_START2 takeoff.
pub const AIRPLANE_START2_ENTRY: u32 = 0x40c7f0;

/// `C_airplane::ShowDamageIndicator` entry, GAME RVA 0xddd0 (VA 0x40ddd0;
/// mafia-re game/vehicles/C_airplane.cpp:35-39 at pin fe7e746; `STUB: GAME`
/// code in Game.exe, not a data offset and not another binary): shows or hides
/// the damage bar on the HUD.
pub const AIRPLANE_SHOW_DAMAGE_INDICATOR_ENTRY: u32 = 0x40ddd0;

/// `C_airplane::ChangeState` entry, GAME RVA 0xdce0 (VA 0x40dce0; mafia-re
/// game/vehicles/C_airplane.cpp:124-129 at pin fe7e746; `STUB: GAME` code in
/// Game.exe, not a data offset and not another binary): delegates to the base
/// `C_actor::ChangeState`.
pub const AIRPLANE_CHANGE_STATE_ENTRY: u32 = 0x40dce0;

/// Original `C_airplane::Start` stub (mafia-re
/// game/vehicles/C_airplane.cpp:16-20 at pin fe7e746; `STUB: GAME` VA
/// 0x40c610, RVA 0xc610): the original only logs. The flight-model owner stays
/// outside this contract, so the boundary is explicit. The parameter names
/// follow the reference; the speed unit (m/s) comes from the scripts'
/// AIRPLANE_START call.
pub fn airplane_start(_unknown: u32, _speed: f32) -> Result<(), AirplaneStubError> {
    Err(AirplaneStubError::Unsupported {
        entry: AIRPLANE_START_ENTRY,
    })
}

/// Original `C_airplane::Start2` stub (mafia-re
/// game/vehicles/C_airplane.cpp:22-26 at pin fe7e746; `STUB: GAME` VA
/// 0x40c7f0, RVA 0xc7f0): the original only logs. The flight-model owner stays
/// outside this contract, so the boundary is explicit.
pub fn airplane_start2() -> Result<(), AirplaneStubError> {
    Err(AirplaneStubError::Unsupported {
        entry: AIRPLANE_START2_ENTRY,
    })
}

/// Original `C_airplane::ShowDamageIndicator` stub (mafia-re
/// game/vehicles/C_airplane.cpp:35-39 at pin fe7e746; `STUB: GAME` VA
/// 0x40ddd0, RVA 0xddd0): the original only logs. The HUD owner stays outside
/// this contract, so the boundary is explicit.
pub fn airplane_show_damage_indicator(_show: bool) -> Result<(), AirplaneStubError> {
    Err(AirplaneStubError::Unsupported {
        entry: AIRPLANE_SHOW_DAMAGE_INDICATOR_ENTRY,
    })
}

/// Original `C_airplane::ChangeState` stub (mafia-re
/// game/vehicles/C_airplane.cpp:124-129 at pin fe7e746; `STUB: GAME` VA
/// 0x40dce0, RVA 0xdce0): the original only logs and runs the base
/// `C_actor::ChangeState`. The base state owner stays outside this contract,
/// so the boundary is explicit.
pub fn airplane_change_state() -> Result<(), AirplaneStubError> {
    Err(AirplaneStubError::Unsupported {
        entry: AIRPLANE_CHANGE_STATE_ENTRY,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rva_d6c0_is_destroyed_needs_both_engines_spent() {
        // C_airplane::IsDestroyed GAME RVA 0xd6c0 (VA 0x40d6c0).
        assert_eq!(AIRPLANE_IS_DESTROYED_ENTRY, 0x40d6c0);
        assert!(airplane_is_destroyed(&AirplaneEngines::new(0.0, 0.0)));
        assert!(airplane_is_destroyed(&AirplaneEngines::new(0.009, 0.009)));
        assert!(airplane_is_destroyed(&AirplaneEngines::new(-1.0, 0.0)));
        // Either engine at or above the threshold keeps the plane alive. Note
        // 0.01f32 itself sits just below the double 0.01, so the strict
        // native comparison still counts it as spent; 0.02 is safely above.
        assert!(!airplane_is_destroyed(&AirplaneEngines::new(0.02, 0.0)));
        assert!(!airplane_is_destroyed(&AirplaneEngines::new(0.0, 0.5)));
        assert!(!airplane_is_destroyed(&AirplaneEngines::new(1.0, 1.0)));
    }

    #[test]
    fn rva_d6f0_save_game_size_adds_124() {
        // C_airplane::SaveGameGetSize GAME RVA 0xd6f0 (VA 0x40d6f0).
        assert_eq!(AIRPLANE_SAVE_GAME_GET_SIZE_ENTRY, 0x40d6f0);
        assert_eq!(AIRPLANE_SAVE_GAME_EXTRA, 124);
        assert_eq!(airplane_save_game_size(0), 124);
        assert_eq!(airplane_save_game_size(100), 224);
        assert_eq!(
            airplane_save_game_size(i32::MAX),
            i32::MIN.wrapping_add(123)
        );
    }

    #[test]
    fn rva_df30_get_material_id_returns_metal_structure() {
        // C_airplane::GetMaterialID GAME RVA 0xdf30 (VA 0x40df30).
        assert_eq!(AIRPLANE_GET_MATERIAL_ID_ENTRY, 0x40df30);
        assert_eq!(AIRPLANE_MATERIAL_ID, 0x13);
        assert_eq!(airplane_material_id(), 0x13);
    }

    #[test]
    fn rva_c610_start_reports_unsupported() {
        // C_airplane::Start GAME RVA 0xc610 (VA 0x40c610).
        assert_eq!(AIRPLANE_START_ENTRY, 0x40c610);
        assert_eq!(
            airplane_start(0, 0.0),
            Err(AirplaneStubError::Unsupported { entry: 0x40c610 })
        );
        assert_eq!(
            airplane_start(1, 55.0),
            Err(AirplaneStubError::Unsupported { entry: 0x40c610 })
        );
    }

    #[test]
    fn rva_c7f0_start2_reports_unsupported() {
        // C_airplane::Start2 GAME RVA 0xc7f0 (VA 0x40c7f0).
        assert_eq!(AIRPLANE_START2_ENTRY, 0x40c7f0);
        assert_eq!(
            airplane_start2(),
            Err(AirplaneStubError::Unsupported { entry: 0x40c7f0 })
        );
    }

    #[test]
    fn rva_ddd0_show_damage_indicator_reports_unsupported() {
        // C_airplane::ShowDamageIndicator GAME RVA 0xddd0 (VA 0x40ddd0).
        assert_eq!(AIRPLANE_SHOW_DAMAGE_INDICATOR_ENTRY, 0x40ddd0);
        assert_eq!(
            airplane_show_damage_indicator(true),
            Err(AirplaneStubError::Unsupported { entry: 0x40ddd0 })
        );
        assert_eq!(
            airplane_show_damage_indicator(false),
            Err(AirplaneStubError::Unsupported { entry: 0x40ddd0 })
        );
    }

    #[test]
    fn rva_dce0_change_state_reports_unsupported() {
        // C_airplane::ChangeState GAME RVA 0xdce0 (VA 0x40dce0).
        assert_eq!(AIRPLANE_CHANGE_STATE_ENTRY, 0x40dce0);
        assert_eq!(
            airplane_change_state(),
            Err(AirplaneStubError::Unsupported { entry: 0x40dce0 })
        );
    }
}
