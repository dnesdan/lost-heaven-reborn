//! `G_Camera::SetMode`, `Begin` and `End` pure state on the borrowed Game+0x4c camera.
//!
//! Game.exe SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! base 0x400000. Original entries VA 0x5ed400 / RVA 0x1ed400 (`G_Camera::SetMode`),
//! VA 0x5f2ea0 / RVA 0x1f2ea0 (`G_Camera::Begin`), VA 0x5f3450 / RVA 0x1f3450
//! (`G_Camera::End`); the names are evidenced PS2 names
//! (mafia-re `game/camera/g_camera.h:200-213` @ 113b5b6). Bodies read first at
//! mafia-re 113b5b6: `game/camera/g_camera.cpp:323-338` (SetMode),
//! `game/camera/g_camera.cpp:1145-1228` (Begin), `game/camera/g_camera.cpp:1232-1255`
//! (End). No REA body was read for these three entries (marker-only, no ev IDs;
//! no REA session was opened).
//!
//! Slice layout check (`game/camera/g_camera.h:122-169`, members named after the
//! setters at `g_camera.h:120-121`): mode 0x10, next_mode 0x14, flags 0x18, params
//! 0x1c, unknown_22 0x22, tilt 0x28, look_around_want 0x2c, look_around 0x30, swing
//! 0x58, swing_amount 0x5c, swing_frame 0x60, mode_changed 0x64. All fit the
//! existing 0xa0-byte Game+0x4c borrow (`game_camera_reset.rs`); tilt/swing are
//! untouched by these three bodies and are only verified present by the length
//! admission. New slice reads beyond the reset/prologue precedent are the car
//! word 0xc (reset-zeroed there) for null checks only; link 0x0 is never read
//! (the relink arms are selected by mode alone, the pointer stays I3D-owned).
//!
//! Owner: the existing Game+0x4c borrow stays the only camera owner, plus the
//! borrowed `needLookFromEyes` global (GAME 0x6bdb10 scope, `g_camera.cpp:154-156`)
//! as live bits (`&mut u32`, the `look_from_eyes` convention of
//! `game_camera_reset.rs`). Mode constants, flag constants and the 17-entry
//! `cameraParams` id/next/flags table come from `g_camera.h:37-83,105` and
//! `g_camera.cpp:55-90`; every one of those names is a mafia-re working guess
//! ("the names are ours", "the name is a guess" for menu-adjacent ones), never an
//! original symbol. Only the three `G_Camera` function names are evidenced.
//! `params` words are retained native addresses (entry values = 0x638700 + i*0x44
//! + 0x8), the same convention as `FIRST_CAMERA_VALUES` 0x638708 in
//! `game_camera_reset.rs` (entry 0 here resolves to exactly that address).
//!
//! Pure integer/pointer words only: no float arithmetic, so no precision policy
//! beyond bit retention (`0.0f32.to_bits()` is exactly 0).
//!
//! DEFERRED with explicit markers (no credit): every dereference and every
//! backend effect. `End` performs zero slice stores natively; its I3D relink,
//! `Update`, HUD indicator swap and car frame restore are classified in the
//! returned outcome for their future owners. `Begin`'s car `frame` dereference
//! plus `SetOn`/`hidden_by_camera` (car owner), the player `GetType` check plus
//! `FadeReset` (human owner, concurrent unit), all `LinkTo`/`SetPos`/`SetDir`
//! placements plus `Update` (I3D/mission owners), the sniper indicator flags (HUD
//! owner), `rotInAct` (C_IN path) and the death globals `cameraInDeath`/
//! `interpInDeath`/`destInDeath` (future C_IN/death Tick-block units) stay
//! deferred; the outcome reports which of them the native body would perform.
//! `SetMode`'s profile keeps (`settings.car_camera`/`settings.camera`) are
//! returned as decisions for the future settings owner, in native order (before
//! the mode store); nothing is written to a settings object here. The Tick mode
//! dispatch (`g_camera.cpp:390` on) and the `mode_changed` clear at the end of
//! Tick stay with the future Tick-block units; `Begin`'s `mode_changed = true`
//! tail is applied here because it is inside the Begin body.
//!
//! The tests in this module are hand-derived vectors authored without cargo
//! (no execution is claimed).

#![forbid(unsafe_code)]

use crate::game_camera_reset::CAMERA_BYTES;

// Slice-evidenced `G_Camera` words/bytes touched here (see module docs).
const OFF_CAMERA: usize = 0x4;
const OFF_PLAYER: usize = 0x8;
const OFF_CAR: usize = 0xc;
const OFF_MODE: usize = 0x10;
const OFF_NEXT_MODE: usize = 0x14;
const OFF_FLAGS: usize = 0x18;
const OFF_PARAMS: usize = 0x1c;
const OFF_UNKNOWN_22: usize = 0x22;
const OFF_MODE_CHANGED: usize = 0x64;

// Camera numbers (S_camera_params::id, G_Camera::mode). All names are mafia-re
// working guesses (`g_camera.h:34-67`); only modes referenced by the three bodies
// are listed.
pub const CAMERA_P_BEHIND: u32 = 1;
pub const CAMERA_P_IN: u32 = 3;
pub const CAMERA_CP_SNIPER: u32 = 5;
pub const CAMERA_DEATH: u32 = 6;
pub const CAMERA_C_BEHIND_HELLBOY: u32 = 7;
pub const CAMERA_C_BEHIND_EMETH: u32 = 8;
pub const CAMERA_C_BEHIND3: u32 = 9;
pub const CAMERA_C_BEHIND_HELLBOY_11: u32 = 11;
pub const CAMERA_C_CHASE: u32 = 12;
pub const CAMERA_C_IN: u32 = 13;
pub const CAMERA_C_OVER: u32 = 14;
pub const CAMERA_C_RAILWAY_IN: u32 = 17;
pub const CAMERA_C_DIALOG: u32 = 19;
pub const CAMERA_C_RACING: u32 = 20;
pub const CAMERA_C_LOOK_TO: u32 = 21;
pub const CAMERA_C_LOCK: u32 = 22;
pub const CAMERA_P_BEHIND_23: u32 = 23;
// SetMode's next-mode request (`g_camera.h:60-61`, name ours).
pub const CAMERA_NEXT: u32 = 0xffff_ffff;
// Profile-kept ranges (`g_camera.h:62-67`, names ours).
pub const CAMERA_P_FIRST: u32 = 1;
pub const CAMERA_P_END: u32 = 4;
pub const CAMERA_C_FIRST: u32 = 7;
pub const CAMERA_C_END: u32 = 16;

// S_camera_params flags (`g_camera.h:76-83`, names ours, by value).
pub const CAMPF_UNKNOWN_1: u32 = 1;
pub const CAMPF_HIDE_CAR: u32 = 2;
pub const CAMPF_EYES: u32 = 4;
pub const CAMPF_UNKNOWN_8: u32 = 8;

// The camera table (`g_camera.h:105`, name ours): 17 entries at GAME 0x638700,
// 0x44 bytes each (`S_camera_params`: id 0x0, next 0x4, values[10] 0x8,
// name[16] 0x30, flags 0x40; `g_camera.cpp:55-90`).
const NUM_CAMERAS: usize = 17;
const CAMERA_PARAMS_BASE: u32 = 0x638700;
const CAMERA_ENTRY_BYTES: u32 = 0x44;
const CAMERA_VALUES_OFFSET: u32 = 0x8;

struct CameraEntry {
    id: u32,
    next: u32,
    flags: u32,
}

const CAMERA_TABLE: [CameraEntry; NUM_CAMERAS] = [
    CameraEntry { id: CAMERA_P_BEHIND, next: 0, flags: CAMPF_EYES }, // "P Behind"
    CameraEntry {
        id: CAMERA_P_IN,
        next: CAMERA_P_BEHIND,
        flags: CAMPF_EYES,
    }, // "P In"
    CameraEntry { id: CAMERA_DEATH, next: 0, flags: 0 }, // "P Behind"
    CameraEntry {
        id: CAMERA_C_BEHIND_HELLBOY_11,
        next: CAMERA_C_OVER,
        flags: 0,
    }, // "C Beh. Hellboy"
    CameraEntry {
        id: CAMERA_C_BEHIND_HELLBOY,
        next: CAMERA_C_BEHIND_EMETH,
        flags: 0,
    }, // "C Beh. Hellboy"
    CameraEntry {
        id: CAMERA_C_BEHIND_EMETH,
        next: CAMERA_C_BEHIND3,
        flags: 0,
    }, // "C Behind Emeth"
    CameraEntry {
        id: CAMERA_C_BEHIND3,
        next: CAMERA_C_CHASE,
        flags: 0,
    }, // "C Behind3"
    CameraEntry {
        id: CAMERA_C_CHASE,
        next: CAMERA_C_IN,
        flags: CAMPF_HIDE_CAR,
    }, // "C Chase"
    CameraEntry { id: CAMERA_C_IN, next: CAMERA_C_OVER, flags: 0 }, // "C In"
    CameraEntry {
        id: CAMERA_C_OVER,
        next: CAMERA_C_BEHIND_HELLBOY,
        flags: 0,
    }, // "C Over"
    CameraEntry {
        id: CAMERA_C_RAILWAY_IN,
        next: 0,
        flags: CAMPF_UNKNOWN_1,
    }, // "C RailwayIn"
    CameraEntry {
        id: CAMERA_P_BEHIND_23,
        next: 0,
        flags: CAMPF_UNKNOWN_1,
    }, // "P Behind"
    CameraEntry {
        id: CAMERA_CP_SNIPER,
        next: 0,
        flags: CAMPF_UNKNOWN_1,
    }, // "CP Sniper"
    CameraEntry {
        id: CAMERA_C_LOOK_TO,
        next: 0,
        flags: CAMPF_UNKNOWN_8,
    }, // "C LookTo"
    CameraEntry {
        id: CAMERA_C_LOCK,
        next: 0,
        flags: CAMPF_UNKNOWN_8,
    }, // "C Lock"
    CameraEntry {
        id: CAMERA_C_DIALOG,
        next: 0,
        flags: CAMPF_UNKNOWN_8,
    }, // "C Dialog"
    CameraEntry {
        id: CAMERA_C_RACING,
        next: CAMERA_C_BEHIND_HELLBOY,
        flags: 0,
    }, // "C Racing"
];

fn entry_values_address(index: usize) -> u32 {
    CAMERA_PARAMS_BASE + index as u32 * CAMERA_ENTRY_BYTES + CAMERA_VALUES_OFFSET
}

#[derive(Debug, PartialEq, Eq)]
pub enum CameraBeginError {
    UnavailableOwner,
}

/// Which profile keep the native `SetMode` performs, in order before the mode
/// store. Returned as a decision: the settings owner applies it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKeep {
    None,
    FootCamera(u32),
    CarCamera(u32),
}

/// `Begin`'s DEFERRED linkage arm, selected by mode alone (`g_camera.cpp:1179-1225`).
/// The pointer/placement work stays with the I3D/mission/HUD owners.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeginLink {
    /// C_OVER/C_IN/C_CHASE: `LinkTo(car->frame)` plus placed pos/dir (car+I3D
    /// owners; C_IN's `rotInAct = 0` deferred with it, no owner yet).
    CarFrame,
    /// C_RAILWAY_IN/P_BEHIND_23: `LinkTo(link)` (I3D owner).
    Link,
    /// CP_SNIPER: HUD indicator swap (HUD owner; no relink).
    Sniper,
    /// DEATH: `cameraInDeath`/`interpInDeath`/`destInDeath` snapshot plus relink
    /// to the primary sector (future death Tick-block unit + I3D owner).
    Death,
    /// Every other mode: `LinkTo(primary_sector)` (I3D owner).
    Primary,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeginApplied {
    /// A table entry matched `mode`; otherwise params/flags/unknown_22 keep
    /// their previous words while next_mode is still zeroed.
    pub matched: bool,
    /// The just-stored flags carry CAMPF_EYES (false when unmatched).
    pub eyes: bool,
    /// Matched entry with a nonzero car word: the car owner checks `car->frame`
    /// and applies `SetOn(!(flags & HIDE))` plus `hidden_by_camera`. DEFERRED.
    pub car_visibility_pending: bool,
    /// `needLookFromEyes` zero path with a nonzero player word: the human owner
    /// checks `ACTOR_PLAYER` and applies `FadeReset`. DEFERRED.
    pub fade_reset_pending: bool,
    /// Whether the `needLookFromEyes = 0` store ran (skipped only when the
    /// matched entry carries CAMPF_EYES).
    pub need_look_cleared: bool,
    /// DEFERRED switch arm for the I3D/mission/HUD owners.
    pub link: BeginLink,
    /// `camera->Update()` pending (I3D owner). DEFERRED.
    pub camera_update_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeginOutcome {
    /// Null camera (`g_camera.cpp:1147-1148`); every word untouched.
    NoCamera,
    Applied(BeginApplied),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EndApplied {
    /// CP_SNIPER arm: HUD indicators back to CROSSHAIR. DEFERRED (HUD owner).
    pub sniper_hud_restore: bool,
    /// C_RAILWAY_IN/P_BEHIND_23 arms: relink to the primary sector. DEFERRED.
    pub relink_primary: bool,
    /// Nonzero car word: the car owner applies `frame->SetOn(true)` plus
    /// `hidden_by_camera = false` (native dereferences without a frame check).
    /// DEFERRED.
    pub car_restore_pending: bool,
    /// `camera->Update()` pending (I3D owner). DEFERRED.
    pub camera_update_pending: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EndOutcome {
    /// Null camera (`g_camera.cpp:1234-1235`); nothing classified.
    NoCamera,
    Classified(EndApplied),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetModeOutcome {
    /// False only for the `CAMERA_NEXT`-with-zero-`next_mode` guard
    /// (`g_camera.cpp:325-326`): then nothing is stored and neither callee runs.
    pub applied: bool,
    /// Profile keep decision, evaluated on the NEXT-resolved mode.
    pub keep: ProfileKeep,
    /// `Some` only when `begin` was requested and the guard passed. `End` sees
    /// the pre-store mode and performs no slice stores.
    pub end: Option<EndOutcome>,
    /// `Some` only when `begin` was requested and the guard passed. `Begin`
    /// sees the newly stored mode.
    pub begin: Option<BeginOutcome>,
}

fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn set_word(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

/// `G_Camera::End` (GAME 0x5f3450) over the borrowed camera. Read-only: the
/// native body performs no slice stores, so this classifies the DEFERRED switch
/// arm and the car-restore gate without touching a byte.
pub fn end(camera: &[u8]) -> Result<EndOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    if word(camera, OFF_CAMERA) == 0 {
        return Ok(EndOutcome::NoCamera);
    }
    let mode = word(camera, OFF_MODE);
    let (sniper_hud_restore, relink_primary) = match mode {
        CAMERA_CP_SNIPER => (true, false),
        CAMERA_C_RAILWAY_IN | CAMERA_P_BEHIND_23 => (false, true),
        _ => (false, false),
    };
    Ok(EndOutcome::Classified(EndApplied {
        sniper_hud_restore,
        relink_primary,
        car_restore_pending: word(camera, OFF_CAR) != 0,
        camera_update_pending: true,
    }))
}

/// `G_Camera::Begin` (GAME 0x5f2ea0) pure subset over the borrowed camera and
/// the live `needLookFromEyes` bits. Effect order matches the native entry:
/// camera guard, `next_mode = 0`, first-match table scan with its four stores,
/// the car-visibility gate (null check only; the `frame` dereference stays
/// DEFERRED), the `needLookFromEyes` zero path (ungated by player null; only
/// `FadeReset` stays DEFERRED), the linkage switch classification, `Update`
/// classification, then `mode_changed = true`.
pub fn begin(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
) -> Result<BeginOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    if word(camera, OFF_CAMERA) == 0 {
        return Ok(BeginOutcome::NoCamera);
    }
    let mode = word(camera, OFF_MODE);
    set_word(camera, OFF_NEXT_MODE, 0);
    let mut matched = false;
    let mut eyes = false;
    for (index, entry) in CAMERA_TABLE.iter().enumerate() {
        if entry.id == mode {
            matched = true;
            set_word(camera, OFF_PARAMS, entry_values_address(index));
            set_word(camera, OFF_FLAGS, entry.flags);
            // Original derives this byte from its just-written flags member.
            camera[OFF_UNKNOWN_22] = u8::from((entry.flags & CAMPF_UNKNOWN_1) != 0);
            set_word(camera, OFF_NEXT_MODE, entry.next);
            eyes = (entry.flags & CAMPF_EYES) != 0;
            break;
        }
    }
    let car_visibility_pending = matched && word(camera, OFF_CAR) != 0;
    // DEFERRED: `car->frame` dereference plus `SetOn`/`hidden_by_camera`
    // (car owner); the null check above is the only pure part.
    let zero_path = !matched || !eyes;
    let fade_reset_pending = zero_path && word(camera, OFF_PLAYER) != 0;
    // DEFERRED: the `ACTOR_PLAYER` type check plus `FadeReset` (human owner,
    // concurrent unit); the zero store below runs regardless of player null.
    let mut need_look_cleared = false;
    if zero_path {
        *need_look_from_eyes = 0.0f32.to_bits();
        need_look_cleared = true;
    }
    // DEFERRED: every arm below is I3D/mission/HUD/global work without an
    // owner here; the mode alone selects it.
    let link = match mode {
        CAMERA_C_OVER | CAMERA_C_IN | CAMERA_C_CHASE => BeginLink::CarFrame,
        CAMERA_C_RAILWAY_IN | CAMERA_P_BEHIND_23 => BeginLink::Link,
        CAMERA_CP_SNIPER => BeginLink::Sniper,
        CAMERA_DEATH => BeginLink::Death,
        _ => BeginLink::Primary,
    };
    camera[OFF_MODE_CHANGED] = 1;
    Ok(BeginOutcome::Applied(BeginApplied {
        matched,
        eyes,
        car_visibility_pending,
        fade_reset_pending,
        need_look_cleared,
        link,
        camera_update_pending: true,
    }))
}

/// `G_Camera::SetMode` (GAME 0x5ed400) over the borrowed camera and the live
/// `needLookFromEyes` bits. Native order: NEXT guard on `next_mode`, `End()`
/// when `begin` (sees the old mode, stores nothing), NEXT resolution, profile
/// keeps (returned, not written), `mode = m` store, `Begin()` when `begin`
/// (sees the new mode). The keeps and the mode store are not camera-gated, so
/// they apply even with a null camera while both callees report `NoCamera`.
pub fn set_mode(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    requested: u32,
    begin_flag: bool,
) -> Result<SetModeOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    if requested == CAMERA_NEXT && word(camera, OFF_NEXT_MODE) == 0 {
        return Ok(SetModeOutcome {
            applied: false,
            keep: ProfileKeep::None,
            end: None,
            begin: None,
        });
    }
    let end_outcome = if begin_flag { Some(end(&*camera)?) } else { None };
    let mut resolved = requested;
    if resolved == CAMERA_NEXT {
        resolved = word(camera, OFF_NEXT_MODE);
    }
    let keep = if resolved >= CAMERA_C_FIRST && resolved < CAMERA_C_END {
        ProfileKeep::CarCamera(resolved)
    } else if resolved >= CAMERA_P_FIRST && resolved < CAMERA_P_END {
        ProfileKeep::FootCamera(resolved)
    } else {
        ProfileKeep::None
    };
    set_word(camera, OFF_MODE, resolved);
    let begin_outcome = if begin_flag {
        Some(begin(camera, need_look_from_eyes)?)
    } else {
        None
    };
    Ok(SetModeOutcome {
        applied: true,
        keep,
        end: end_outcome,
        begin: begin_outcome,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn camera_with(
        camera_word: u32,
        player_word: u32,
        car_word: u32,
        mode: u32,
        next_mode: u32,
        fill: u8,
    ) -> [u8; CAMERA_BYTES] {
        let mut bytes = [fill; CAMERA_BYTES];
        bytes[OFF_CAMERA..OFF_CAMERA + 4].copy_from_slice(&camera_word.to_le_bytes());
        bytes[OFF_PLAYER..OFF_PLAYER + 4].copy_from_slice(&player_word.to_le_bytes());
        bytes[OFF_CAR..OFF_CAR + 4].copy_from_slice(&car_word.to_le_bytes());
        bytes[OFF_MODE..OFF_MODE + 4].copy_from_slice(&mode.to_le_bytes());
        bytes[OFF_NEXT_MODE..OFF_NEXT_MODE + 4]
            .copy_from_slice(&next_mode.to_le_bytes());
        bytes
    }
    #[test]
    fn next_guard_keeps_and_ranges_follow_native_order() {
        // NEXT with zero next_mode stores nothing and runs neither callee.
        let mut bytes = camera_with(0x1000, 0x2000, 0x3000, 9, 0, 0xa5);
        let before = bytes;
        let mut need_look = 0xdead_beef;
        let outcome = set_mode(&mut bytes, &mut need_look, CAMERA_NEXT, true).unwrap();
        assert_eq!(
            outcome,
            SetModeOutcome {
                applied: false,
                keep: ProfileKeep::None,
                end: None,
                begin: None,
            }
        );
        assert_eq!(bytes, before);
        assert_eq!(need_look, 0xdead_beef);
        assert_eq!(CAMERA_NEXT, 0xffff_ffff);
        // Keep boundaries: P [1,4), C [7,16); 0/4/16/24 keep nothing.
        for (requested, keep) in [
            (0u32, ProfileKeep::None),
            (1, ProfileKeep::FootCamera(1)),
            (3, ProfileKeep::FootCamera(3)),
            (4, ProfileKeep::None),
            (7, ProfileKeep::CarCamera(7)),
            (15, ProfileKeep::CarCamera(15)),
            (16, ProfileKeep::None),
            (24, ProfileKeep::None),
        ] {
            let mut bytes = camera_with(0x1000, 0x2000, 0, 1, 0, 0);
            let mut need_look = 0u32;
            let outcome = set_mode(&mut bytes, &mut need_look, requested, false).unwrap();
            assert!(outcome.applied, "mode {}", requested);
            assert_eq!(outcome.keep, keep, "mode {}", requested);
            assert_eq!(outcome.end, None);
            assert_eq!(outcome.begin, None);
            assert_eq!(word(&bytes, OFF_MODE), requested);
        }
        // NEXT resolves through nonzero next_mode before the keeps.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 8, 0);
        let mut need_look = 0u32;
        let outcome = set_mode(&mut bytes, &mut need_look, CAMERA_NEXT, false).unwrap();
        assert_eq!(outcome.keep, ProfileKeep::CarCamera(8));
        assert_eq!(word(&bytes, OFF_MODE), 8);
    }
    #[test]
    fn keeps_and_mode_store_are_not_camera_gated() {
        // Null camera: keeps + mode store still apply; callees report NoCamera.
        let mut bytes = camera_with(0, 0, 0, 1, 0, 0);
        let mut need_look = 0xcafe_babe;
        let outcome = set_mode(&mut bytes, &mut need_look, 12, true).unwrap();
        assert!(outcome.applied);
        assert_eq!(outcome.keep, ProfileKeep::CarCamera(12));
        assert_eq!(word(&bytes, OFF_MODE), 12);
        assert_eq!(outcome.end, Some(EndOutcome::NoCamera));
        assert_eq!(outcome.begin, Some(BeginOutcome::NoCamera));
        assert_eq!(need_look, 0xcafe_babe);
        assert_eq!(word(&bytes, OFF_NEXT_MODE), 0);
    }
    #[test]
    fn set_mode_begin_composes_end_then_begin_in_order() {
        // Old mode 1 (default arms), new mode 12 (C Chase, HIDE_CAR).
        let mut bytes = camera_with(0x1000, 0x2000, 0x3000, 1, 0, 0);
        let mut need_look = 0x1234_5678;
        let outcome = set_mode(&mut bytes, &mut need_look, CAMERA_C_CHASE, true).unwrap();
        // End saw the OLD mode: default arm, car restore pending.
        assert_eq!(
            outcome.end,
            Some(EndOutcome::Classified(EndApplied {
                sniper_hud_restore: false,
                relink_primary: false,
                car_restore_pending: true,
                camera_update_pending: true,
            }))
        );
        assert_eq!(outcome.keep, ProfileKeep::CarCamera(12));
        // Begin saw the NEW mode: entry 7 -> values 0x638700 + 7*0x44 + 8.
        assert_eq!(word(&bytes, OFF_PARAMS), 0x6388e4);
        assert_eq!(word(&bytes, OFF_FLAGS), CAMPF_HIDE_CAR);
        assert_eq!(bytes[OFF_UNKNOWN_22], 0);
        assert_eq!(word(&bytes, OFF_NEXT_MODE), CAMERA_C_IN);
        assert_eq!(need_look, 0);
        assert_eq!(bytes[OFF_MODE_CHANGED], 1);
        match outcome.begin {
            Some(BeginOutcome::Applied(applied)) => {
                assert_eq!(
                    applied,
                    BeginApplied {
                        matched: true,
                        eyes: false,
                        car_visibility_pending: true,
                        fade_reset_pending: true,
                        need_look_cleared: true,
                        link: BeginLink::CarFrame,
                        camera_update_pending: true,
                    }
                );
            }
            other => panic!("expected Applied begin, got {:?}", other),
        }
    }
    #[test]
    fn begin_table_lookup_stores_eyes_path_and_no_match_path() {
        // Mode 1 "P Behind": EYES entry 0 -> params 0x638708 (== Reset's
        // FIRST_CAMERA_VALUES), needLook untouched, no fade, default link.
        let mut bytes = camera_with(0x1000, 0x2000, 0, 1, 0x7777_7777, 0);
        let mut need_look = 0xdead_beef;
        let outcome = begin(&mut bytes, &mut need_look).unwrap();
        assert_eq!(word(&bytes, OFF_PARAMS), 0x638708);
        assert_eq!(word(&bytes, OFF_FLAGS), CAMPF_EYES);
        assert_eq!(bytes[OFF_UNKNOWN_22], 0);
        assert_eq!(word(&bytes, OFF_NEXT_MODE), 0);
        assert_eq!(need_look, 0xdead_beef);
        assert_eq!(bytes[OFF_MODE_CHANGED], 1);
        match outcome {
            BeginOutcome::Applied(applied) => {
                assert!(applied.matched);
                assert!(applied.eyes);
                assert!(!applied.car_visibility_pending);
                assert!(!applied.fade_reset_pending);
                assert!(!applied.need_look_cleared);
                assert_eq!(applied.link, BeginLink::Primary);
            }
            BeginOutcome::NoCamera => panic!("expected Applied"),
        }
        // Mode 17 "C RailwayIn": UNKNOWN_1 sets unknown_22, Link arm, zero path.
        let mut bytes = camera_with(0x1000, 0, 0, CAMERA_C_RAILWAY_IN, 9, 0);
        let mut need_look = 0x1111_1111;
        match begin(&mut bytes, &mut need_look).unwrap() {
            BeginOutcome::Applied(applied) => {
                // Entry 10 -> values 0x638700 + 10*0x44 + 8 = 0x6389b0.
                assert_eq!(word(&bytes, OFF_PARAMS), 0x6389b0);
                assert_eq!(word(&bytes, OFF_FLAGS), CAMPF_UNKNOWN_1);
                assert_eq!(bytes[OFF_UNKNOWN_22], 1);
                assert!(!applied.eyes);
                assert!(!applied.fade_reset_pending);
                assert_eq!(applied.link, BeginLink::Link);
            }
            BeginOutcome::NoCamera => panic!("expected Applied"),
        }
        assert_eq!(need_look, 0);
        // Mode 10 (C_AROUND, upstream-guessed name): no table entry. Prior
        // params/flags/unknown_22 survive; next_mode still zeroes; zero path.
        let mut bytes = camera_with(0x1000, 0x2000, 0x3000, 10, 5, 0xa5);
        bytes[OFF_PARAMS..OFF_PARAMS + 4].copy_from_slice(&0x1122_3344u32.to_le_bytes());
        bytes[OFF_FLAGS..OFF_FLAGS + 4].copy_from_slice(&0x5566_7788u32.to_le_bytes());
        bytes[OFF_UNKNOWN_22] = 0x99;
        let mut need_look = 0x2222_2222;
        match begin(&mut bytes, &mut need_look).unwrap() {
            BeginOutcome::Applied(applied) => {
                assert!(!applied.matched);
                assert!(!applied.eyes);
                assert!(!applied.car_visibility_pending);
                assert!(applied.fade_reset_pending);
                assert_eq!(applied.link, BeginLink::Primary);
            }
            BeginOutcome::NoCamera => panic!("expected Applied"),
        }
        assert_eq!(word(&bytes, OFF_PARAMS), 0x1122_3344);
        assert_eq!(word(&bytes, OFF_FLAGS), 0x5566_7788);
        assert_eq!(bytes[OFF_UNKNOWN_22], 0x99);
        assert_eq!(word(&bytes, OFF_NEXT_MODE), 0);
        assert_eq!(need_look, 0);
        // Sniper + death arms classify without touching slice words beyond the
        // table stores and mode_changed.
        for (mode, link) in [
            (CAMERA_CP_SNIPER, BeginLink::Sniper),
            (CAMERA_DEATH, BeginLink::Death),
            (CAMERA_C_IN, BeginLink::CarFrame),
            (CAMERA_P_BEHIND_23, BeginLink::Link),
        ] {
            let mut bytes = camera_with(0x1000, 0, 0, mode, 0, 0);
            let mut need_look = 0u32;
            match begin(&mut bytes, &mut need_look).unwrap() {
                BeginOutcome::Applied(applied) => assert_eq!(applied.link, link),
                BeginOutcome::NoCamera => panic!("mode {}", mode),
            }
        }
        // Table shape: 17 entries, first-match ids unique.
        assert_eq!(CAMERA_TABLE.len(), 17);
        let mut seen = [false; 24];
        for entry in CAMERA_TABLE.iter() {
            assert!(entry.id < 24);
            assert!(!seen[entry.id as usize], "duplicate id {}", entry.id);
            seen[entry.id as usize] = true;
        }
    }
    #[test]
    fn begin_null_camera_touches_nothing() {
        let mut bytes = camera_with(0, 0x2000, 0x3000, 12, 5, 0xa5);
        let before = bytes;
        let mut need_look = 0x3333_3333;
        assert_eq!(
            begin(&mut bytes, &mut need_look),
            Ok(BeginOutcome::NoCamera)
        );
        assert_eq!(bytes, before);
        assert_eq!(need_look, 0x3333_3333);
    }
    #[test]
    fn end_classifies_arms_and_stores_nothing() {
        // Null camera classifies nothing.
        let bytes = camera_with(0, 0, 0x3000, CAMERA_CP_SNIPER, 0, 0);
        assert_eq!(end(&bytes), Ok(EndOutcome::NoCamera));
        // Sniper arm restores the HUD; railway/23 relink; default neither.
        // No arm stores a byte: the whole slice survives bit-identical.
        for (mode, sniper, relink, car, expected_car) in [
            (CAMERA_CP_SNIPER, true, false, 0u32, false),
            (CAMERA_CP_SNIPER, true, false, 0x3000, true),
            (CAMERA_C_RAILWAY_IN, false, true, 0, false),
            (CAMERA_P_BEHIND_23, false, true, 0x3000, true),
            (CAMERA_P_BEHIND, false, false, 0, false),
            (CAMERA_C_CHASE, false, false, 0x3000, true),
        ] {
            let bytes = camera_with(0x1000, 0x2000, car, mode, 4, 0xa5);
            let before = bytes;
            assert_eq!(
                end(&bytes),
                Ok(EndOutcome::Classified(EndApplied {
                    sniper_hud_restore: sniper,
                    relink_primary: relink,
                    car_restore_pending: expected_car,
                    camera_update_pending: true,
                })),
                "mode {}",
                mode
            );
            assert_eq!(bytes, before, "end stored into mode {}", mode);
        }
    }
    #[test]
    fn set_mode_begin_false_leaves_begin_state_untouched() {
        // begin=false applies keeps + mode only: next_mode, params, flags,
        // needLook and mode_changed all survive.
        let mut bytes = camera_with(0x1000, 0x2000, 0x3000, 1, 6, 0xa5);
        bytes[OFF_MODE_CHANGED] = 0;
        let mut need_look = 0x4444_4444;
        let outcome = set_mode(&mut bytes, &mut need_look, 13, false).unwrap();
        assert_eq!(outcome.keep, ProfileKeep::CarCamera(13));
        assert_eq!(word(&bytes, OFF_MODE), 13);
        assert_eq!(word(&bytes, OFF_NEXT_MODE), 6);
        assert_eq!(need_look, 0x4444_4444);
        assert_eq!(bytes[OFF_MODE_CHANGED], 0);
        assert_eq!(bytes[OFF_UNKNOWN_22], 0xa5);
    }
    #[test]
    fn wrong_size_slice_is_unavailable_and_touches_nothing() {
        let full = camera_with(0x1000, 0x2000, 0x3000, 1, 0, 0);
        let mut long = [0u8; CAMERA_BYTES + 4];
        long[..CAMERA_BYTES].copy_from_slice(&full);
        let mut need_look = 0x5555_5555;
        for short in [0, CAMERA_BYTES - 1] {
            assert_eq!(
                end(&full[..short]),
                Err(CameraBeginError::UnavailableOwner)
            );
            assert_eq!(
                begin(&mut long[..short], &mut need_look),
                Err(CameraBeginError::UnavailableOwner)
            );
            assert_eq!(
                set_mode(&mut long[..short], &mut need_look, 7, true),
                Err(CameraBeginError::UnavailableOwner)
            );
            assert_eq!(need_look, 0x5555_5555);
        }
        assert_eq!(end(&long), Err(CameraBeginError::UnavailableOwner));
        assert_eq!(
            begin(&mut long, &mut need_look),
            Err(CameraBeginError::UnavailableOwner)
        );
        assert_eq!(
            set_mode(&mut long, &mut need_look, 7, true),
            Err(CameraBeginError::UnavailableOwner)
        );
        assert_eq!(need_look, 0x5555_5555);
    }
}
