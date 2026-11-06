//! `G_Camera::Tick` prologue guards plus `dt`/`naklon` (Game.exe, NOT LS3DF).
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000. Original entry VA 0x5ed4c0 / RVA 0x1ed4c0, evidenced name
//! `G_Camera::Tick` (mafia-re `game/camera/g_camera.h:177`,
//! `game/camera/g_camera.cpp:368` @ 113b5b6). Rizin-observed binary bounds
//! 0x5ed4c0-0x5f20e0; the Ghidra function split is deferred, so this port
//! claims contract tier at most (contract ceiling).
//!
//! Ported slice, in native order (mafia-re `game/camera/g_camera.cpp:368-389`):
//! 1. `params` snapshot (`float *p = params`, G_Camera+0x1c).
//! 2. Null-camera early-out (G_Camera+0x4).
//! 3. Time clamp to `MAX_TICK_TIME` 83 (`game/camera/g_camera.cpp:39`, a
//!    working name acknowledged there) and `dt = time / 1000.0f`.
//! 4. MENU-mode (24, `game/camera/g_camera.h:58`, a guessed name there) plus
//!    `unknown_20` (G_Camera+0x20) guards.
//! 5. Null-player early-out (G_Camera+0x8).
//! 6. `naklon += dt * 0.3f`, wrapping by 2.0 when exceeded (global `naklon`,
//!    native 0x6bdb04, `game/camera/g_camera.cpp:142-144`; sibling globals
//!    surveyed at `game/camera/g_camera.cpp:45-176`).
//!
//! Owner: the existing Game+0x4c borrow (the 0xa0-byte slice of
//! `game_camera_reset.rs`, via [`crate::game_camera_reset::CAMERA_BYTES`])
//! stays the only camera owner; this module adds ONE new `f32` global owner,
//! the borrowed `naklon`. All five camera reads are slice-evidenced there:
//! camera word 0x4, player word 0x8, mode word 0x10, params word 0x1c and
//! `unknown_20` byte 0x20 (mafia-re layout `game/camera/g_camera.h:122-260`,
//! members at lines 125-137). No unavailable markers were needed and no other
//! Tick state is read; the camera borrow is read-only here.
//!
//! Pure arithmetic: no human/car/collision/dialog/race borrowers and no
//! backend trait. Precision policy: the clamped time (0..=83) is exact in
//! binary32; `dt`, the 0.3 rate product, the accumulation and the 2.0 wrap
//! subtraction each round once on the host, recorded through
//! `crate::native_precision` like the existing camera arithmetic. Any
//! retained x87 double-rounding divergence is bounded to the last ulp; the
//! tests below pin the host bits.
//!
//! DEFERRED with explicit markers (no credit): everything from the mode
//! dispatch (`game/camera/g_camera.cpp:390 switch (mode)`) on — every mode
//! block, the CAMPF_EYES/look-around/car/on-foot tails, the `mode_changed`
//! clear, and the callees `WantPosCar` (`g_camera.cpp:1024`, GAME 0x5f2100),
//! `WantPos` (`g_camera.cpp:1054`, GAME 0x5f2580), `Begin` (`g_camera.cpp:1145`,
//! GAME 0x5f2ea0) and `End` (`g_camera.cpp:1232`, GAME 0x5f3450). An
//! [`TickPrologueOutcome::Advanced`] return carries the snapshotted `params`
//! and `dt` for those later slices; it never advances them automatically.
//!
//! The tests in this module are hand-derived vectors authored without cargo
//! (no execution is claimed).

#![forbid(unsafe_code)]

use crate::game_camera_reset::CAMERA_BYTES;
use crate::native_precision::{add, div, mul, sub};

/// Longest frame the camera advances, in ms (mafia-re working name,
/// `game/camera/g_camera.cpp:39`).
pub const MAX_TICK_TIME: u32 = 83;
/// Menu camera mode (mafia-re guessed name, `game/camera/g_camera.h:58`).
pub const CAMERA_MENU: u32 = 24;

// Slice-evidenced `G_Camera` reads (see module docs); nothing else is read.
const OFF_CAMERA: usize = 0x4;
const OFF_PLAYER: usize = 0x8;
const OFF_MODE: usize = 0x10;
const OFF_PARAMS: usize = 0x1c;
const OFF_UNKNOWN_20: usize = 0x20;

// The 0.3f rate literal and the 2.0f sway wrap limit, by bits.
const NAKLON_RATE_BITS: u32 = 0x3e99999a;
const NAKLON_LIMIT_BITS: u32 = 0x40000000;

#[derive(Debug, PartialEq, Eq)]
pub enum CameraTickPrologueError {
    UnavailableOwner,
}

/// Prologue decision. Every variant carries the entry `params` snapshot
/// (`g_camera.cpp:375`), which the native code reads before all guards.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TickPrologueOutcome {
    /// Null camera (`g_camera.cpp:376-377`); `naklon` untouched.
    NoCamera { params: u32 },
    /// MENU mode without `unknown_20` (`g_camera.cpp:381-382`).
    MenuIdle { params: u32 },
    /// `unknown_20` set (`g_camera.cpp:383-384`).
    Suspended { params: u32 },
    /// Live mode without a player (`g_camera.cpp:385-386`).
    NoPlayer { params: u32 },
    /// Fell through to the sway update (`g_camera.cpp:387-389`); `dt` and
    /// `params` are carried for the DEFERRED mode dispatch, which this
    /// return never advances automatically.
    Advanced { params: u32, dt: f32 },
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

/// `G_Camera::Tick` prologue over the borrowed Game+0x4c camera and the live
/// `naklon` global. Effect order matches the native entry: params snapshot,
/// camera guard, clamp, `dt`, MENU/`unknown_20` guards, player guard, then the
/// sway update. Early-outs leave `naklon` untouched. Only an actual owned
/// 0xa0-byte camera slice is admitted.
pub fn tick_prologue(
    camera: &[u8],
    naklon: &mut f32,
    time: u32,
) -> Result<TickPrologueOutcome, CameraTickPrologueError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraTickPrologueError::UnavailableOwner);
    }
    let params = word(camera, OFF_PARAMS);
    if word(camera, OFF_CAMERA) == 0 {
        return Ok(TickPrologueOutcome::NoCamera { params });
    }
    let clamped = time.min(MAX_TICK_TIME);
    // clamped <= 83 converts exactly; one correctly-rounded division.
    let dt = div(clamped as f32, 1000.0f32);
    let mode = word(camera, OFF_MODE);
    let suspended = camera[OFF_UNKNOWN_20] != 0;
    if mode == CAMERA_MENU && !suspended {
        return Ok(TickPrologueOutcome::MenuIdle { params });
    }
    if suspended {
        return Ok(TickPrologueOutcome::Suspended { params });
    }
    if word(camera, OFF_PLAYER) == 0 {
        return Ok(TickPrologueOutcome::NoPlayer { params });
    }
    let step = mul(dt, f32::from_bits(NAKLON_RATE_BITS));
    let updated = add(*naklon, step);
    // Strict `>` like the native wrap: exactly 2.0 stays.
    *naklon = if updated > f32::from_bits(NAKLON_LIMIT_BITS) {
        sub(updated, f32::from_bits(NAKLON_LIMIT_BITS))
    } else {
        updated
    };
    Ok(TickPrologueOutcome::Advanced { params, dt })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn camera(
        camera_word: u32,
        player_word: u32,
        mode: u32,
        unknown_20: u8,
        params: u32,
    ) -> [u8; CAMERA_BYTES] {
        let mut bytes = [0u8; CAMERA_BYTES];
        bytes[OFF_CAMERA..OFF_CAMERA + 4].copy_from_slice(&camera_word.to_le_bytes());
        bytes[OFF_PLAYER..OFF_PLAYER + 4].copy_from_slice(&player_word.to_le_bytes());
        bytes[OFF_MODE..OFF_MODE + 4].copy_from_slice(&mode.to_le_bytes());
        bytes[OFF_PARAMS..OFF_PARAMS + 4].copy_from_slice(&params.to_le_bytes());
        bytes[OFF_UNKNOWN_20] = unknown_20;
        bytes
    }
    #[test]
    fn guard_matrix_follows_native_order_and_echoes_params() {
        let idle_bits = 1.25f32.to_bits();
        let mut naklon = 1.25f32;
        // Null camera wins over every later guard; params still snapshot.
        let bytes = camera(0, 0, CAMERA_MENU, 1, 0x638708);
        assert_eq!(
            tick_prologue(&bytes, &mut naklon, 83),
            Ok(TickPrologueOutcome::NoCamera { params: 0x638708 })
        );
        assert_eq!(naklon.to_bits(), idle_bits);
        // MENU idle precedes the player guard: null player still idles.
        let bytes = camera(0x1000, 0, CAMERA_MENU, 0, 0x1111_1111);
        assert_eq!(
            tick_prologue(&bytes, &mut naklon, 83),
            Ok(TickPrologueOutcome::MenuIdle {
                params: 0x1111_1111
            })
        );
        assert_eq!(naklon.to_bits(), idle_bits);
        // Nonzero unknown_20 suspends in MENU mode and outside it.
        let bytes = camera(0x1000, 0x2000, CAMERA_MENU, 1, 0x2222_2222);
        assert_eq!(
            tick_prologue(&bytes, &mut naklon, 83),
            Ok(TickPrologueOutcome::Suspended {
                params: 0x2222_2222
            })
        );
        let bytes = camera(0x1000, 0, 1, 0xff, 0x3333_3333);
        assert_eq!(
            tick_prologue(&bytes, &mut naklon, 83),
            Ok(TickPrologueOutcome::Suspended {
                params: 0x3333_3333
            })
        );
        assert_eq!(naklon.to_bits(), idle_bits);
        // Live mode without a player stops before the sway.
        let bytes = camera(0x1000, 0, 1, 0, 0x4444_4444);
        assert_eq!(
            tick_prologue(&bytes, &mut naklon, 83),
            Ok(TickPrologueOutcome::NoPlayer {
                params: 0x4444_4444
            })
        );
        assert_eq!(naklon.to_bits(), idle_bits);
        // Mode 0 (non-menu) with a player falls through.
        let bytes = camera(0x1000, 0x2000, 0, 0, 0x5555_5555);
        match tick_prologue(&bytes, &mut naklon, 0) {
            Ok(TickPrologueOutcome::Advanced { params, dt }) => {
                assert_eq!(params, 0x5555_5555);
                assert_eq!(dt.to_bits(), 0x00000000);
            }
            other => panic!("expected Advanced, got {:?}", other),
        }
        // Zero dt leaves the sway bit-identical.
        assert_eq!(naklon.to_bits(), idle_bits);
    }
    #[test]
    fn time_clamp_selects_83ms_dt_bits() {
        let bytes = camera(0x1000, 0x2000, 1, 0, 0x638708);
        for time in [83u32, 84, 100, u32::MAX] {
            let mut naklon = 0.0f32;
            match tick_prologue(&bytes, &mut naklon, time) {
                Ok(TickPrologueOutcome::Advanced { params, dt }) => {
                    assert_eq!(params, 0x638708);
                    assert_eq!(dt.to_bits(), 0x3da9fbe7, "clamped 83/1000.0f");
                }
                other => panic!("time {}: expected Advanced, got {:?}", time, other),
            }
        }
        let mut naklon = 0.0f32;
        match tick_prologue(&bytes, &mut naklon, 1) {
            Ok(TickPrologueOutcome::Advanced { dt, .. }) => {
                assert_eq!(dt.to_bits(), 0x3a83126f, "1/1000.0f");
            }
            other => panic!("expected Advanced, got {:?}", other),
        }
    }
    #[test]
    fn naklon_advance_and_wrap_match_single_rounding_bits() {
        let bytes = camera(0x1000, 0x2000, 1, 0, 0x638708);
        // 0 + (0.083 * 0.3f).
        let mut naklon = 0.0f32;
        tick_prologue(&bytes, &mut naklon, 83).unwrap();
        assert_eq!(naklon.to_bits(), 0x3ccbfb16);
        // 0 + (0.001 * 0.3f).
        let mut naklon = 0.0f32;
        tick_prologue(&bytes, &mut naklon, 1).unwrap();
        assert_eq!(naklon.to_bits(), 0x399d4952);
        // 1.99 + step(83) exceeds 2.0 and wraps once.
        let mut naklon = f32::from_bits(0x3ffeb852);
        tick_prologue(&bytes, &mut naklon, 83).unwrap();
        assert_eq!(naklon.to_bits(), 0x3c741f00);
        // Exactly 2.0 with zero dt does not wrap (strict `>`).
        let mut naklon = 2.0f32;
        tick_prologue(&bytes, &mut naklon, 0).unwrap();
        assert_eq!(naklon.to_bits(), 0x40000000);
        // Below the limit without a step stays put.
        let mut naklon = f32::from_bits(0x3ffeb852);
        tick_prologue(&bytes, &mut naklon, 0).unwrap();
        assert_eq!(naklon.to_bits(), 0x3ffeb852);
    }
    #[test]
    fn wrong_size_slice_is_unavailable_and_touches_nothing() {
        let full = camera(0x1000, 0x2000, 1, 0, 0x638708);
        let mut long = [0u8; CAMERA_BYTES + 4];
        long[..CAMERA_BYTES].copy_from_slice(&full);
        let mut naklon = 0.5f32;
        for short in [0, CAMERA_BYTES - 1] {
            assert_eq!(
                tick_prologue(&full[..short], &mut naklon, 83),
                Err(CameraTickPrologueError::UnavailableOwner)
            );
            assert_eq!(naklon.to_bits(), 0.5f32.to_bits());
        }
        assert_eq!(
            tick_prologue(&long, &mut naklon, 83),
            Err(CameraTickPrologueError::UnavailableOwner)
        );
        assert_eq!(naklon.to_bits(), 0.5f32.to_bits());
    }
}
