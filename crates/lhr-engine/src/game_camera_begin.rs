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
const OFF_LINK: usize = 0x0;
const OFF_CAMERA: usize = 0x4;
const OFF_PLAYER: usize = 0x8;
const OFF_CAR: usize = 0xc;
const OFF_MODE: usize = 0x10;
const OFF_NEXT_MODE: usize = 0x14;
const OFF_FLAGS: usize = 0x18;
const OFF_PARAMS: usize = 0x1c;
const OFF_UNKNOWN_22: usize = 0x22;
const OFF_LOOK_AROUND_WANT: usize = 0x2c;
const OFF_LOOK_AROUND: usize = 0x30;
const OFF_SWING: usize = 0x58;
const OFF_SWING_AMOUNT: usize = 0x5c;
const OFF_SWING_FRAME: usize = 0x60;
const OFF_MODE_CHANGED: usize = 0x64;
const OFF_LOOK_TO: usize = 0x88;
const OFF_SNIPER_FOV: usize = 0x90;
const OFF_FREE_LOOK: usize = 0x94;

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
// LookAround's turn targets (mafia-re `game/camera/g_camera.h:71-74` @ fe7e746;
// the names are upstream working guesses, the values are the original's).
pub const LOOK_LEFT: i32 = -1;
pub const LOOK_AHEAD: i32 = 0;
pub const LOOK_RIGHT: i32 = 1;
pub const LOOK_BEHIND: i32 = 2;
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
    CameraEntry {
        id: CAMERA_P_BEHIND,
        next: 0,
        flags: CAMPF_EYES,
    }, // "P Behind"
    CameraEntry {
        id: CAMERA_P_IN,
        next: CAMERA_P_BEHIND,
        flags: CAMPF_EYES,
    }, // "P In"
    CameraEntry {
        id: CAMERA_DEATH,
        next: 0,
        flags: 0,
    }, // "P Behind"
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
    CameraEntry {
        id: CAMERA_C_IN,
        next: CAMERA_C_OVER,
        flags: 0,
    }, // "C In"
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
    /// CP_SNIPER: HUD indicator swap (HUD owner; no relink). The native arm is
    /// `mafiaIndicators.show_flags &= ~INDF_CROSSHAIR; show_flags |=
    /// INDF_SNIPER` (`game/camera/g_camera.cpp:1213-1215`); the owned
    /// transition is `crate::indicators::IndicatorsState::apply_camera_begin_sniper`.
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
    /// CP_SNIPER arm: HUD indicators back to CROSSHAIR. The native arm is
    /// `mafiaIndicators.show_flags &= ~INDF_SNIPER; show_flags |=
    /// INDF_CROSSHAIR` (`game/camera/g_camera.cpp:1238-1240`); the owned
    /// transition is
    /// `crate::indicators::IndicatorsState::apply_camera_end_sniper`.
    /// DEFERRED (HUD owner).
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
    let end_outcome = if begin_flag {
        Some(end(&*camera)?)
    } else {
        None
    };
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

// ---- Small `G_Camera` setters, getters and state flags (reference pin fe7e746).
//
// The fifteen items below port the remaining small `G_Camera` bodies of
// `game/camera/g_camera.cpp` that need no scene, collision, HUD or model
// device beyond what this module already borrows: the 0xa0-byte camera slice,
// the live `needLookFromEyes` bits and the two free-look angle globals.
// (`game/actors/C_animations.cpp` needs nothing: every one of its FUNCTION
// entries already lives in `actor_animation` or `animation_blending`.)
// Bigger neighbors stay unclaimed here on purpose: `WantPosCar`/`WantPos`
// (GAME 0x5f2100/0x5f2580, filter math plus scene owners), `FreeLook`/
// `FreeLookBase` (GAME 0x5f3520/0x5f3810, profile sensitivity plus the I3D
// camera device), `LockAt`/`CumNaNe` (GAME 0x5f39f0/0x5f3a70, collision plus
// camera placement), `PushMode`/`PopMode` (GAME 0x5f3ff0/0x5f4220, camera
// FOV/range device reads plus the `push_pop` vector owner), `DialogLook`/
// `DialogStart`/`DialogStop` (GAME 0x5f42e0/0x5f4370/0x5f4650, human and
// dialog-list owners), `AddRaceCamera`/`RemoveRaceCamera`/`InitRaceCameras`
// (GAME 0x5f46a0/0x5f4870/0x5f48d0, race/circuit owners), `GameDone` (GAME
// 0x5ed120, a `PopMode` loop with the same owners) and the constructor/
// destructor pair (GAME 0x5ecf90/0x5ed000: the constructor is `Reset`, already
// claimed at RVA 0x1ed040; the destructor is empty). `SetModeChanged` (inline
// copy at GAME 0x5b4550, inside the `C_game::Init` range) and `GetMode`
// (folded copies only) claim no standalone camera code and stay working names
// without an RVA. `C_animations.cpp` is fully covered already: every one of
// its FUNCTION entries lives in `actor_animation` (`GetTrack`) or
// `animation_blending` with overlay pins and tests.
// The tests for the items below are hand-derived vectors authored without
// cargo (no execution is claimed), placed in this module's existing tests
// which already own the `camera_with` helper.

/// Live profile camera numbers for the setters that store them.
///
/// Only the taken branch reads: `set_car` reads one of the two, `link` reads
/// the foot camera only when the link frame is NULL, `leze_do_toyoty` reads
/// the car camera only on its off path.
pub trait CameraSettings {
    type Error;
    fn car_camera(&mut self) -> Result<u32, Self::Error>;
    fn foot_camera(&mut self) -> Result<u32, Self::Error>;
}

/// Owner/settings failure of the small setters below.
#[derive(Debug, PartialEq, Eq)]
pub enum CameraSetterError<E> {
    UnavailableOwner,
    Settings(E),
}

/// Shared `End`-then-`Begin` outcome of the member rebinds (`set_camera`,
/// `set_player`, `leze_do_toyoty`): `end` saw the previous member word,
/// `begin` sees the newly stored one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RebindOutcome {
    pub end: EndOutcome,
    pub begin: BeginOutcome,
}

fn word_f32(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn set_word_f32(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn map_owner<E>(_: CameraBeginError) -> CameraSetterError<E> {
    CameraSetterError::UnavailableOwner
}

/// `G_Camera::GetCar` (GAME VA 0x47b4d0 / RVA 0xb4d0; evidenced original
/// symbol `?GetCar@G_Camera@@QBEPAVC_car@@XZ`, mafia-re
/// `game/camera/g_camera.h:270-274` @ fe7e746; the reference keeps the single
/// folded copy the linker retains, declared at `g_camera.h:264-268`).
/// Reads the car word (`G_Camera+0xc`, `g_camera.h:128` @ fe7e746): `None`
/// is the native NULL. Read-only; only an actual owned 0xa0-byte camera slice
/// is admitted.
pub fn get_car(camera: &[u8]) -> Result<Option<u32>, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    Ok(match word(camera, OFF_CAR) {
        0 => None,
        car => Some(car),
    })
}

/// `G_Camera::SetCamera` (GAME VA 0x5ed150 / RVA 0x1ed150; mafia-re
/// `game/camera/g_camera.cpp:225-232` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Native order: `End()` on the old camera word, store `cam` at
/// `G_Camera+0x4`, `Begin()` on the new word. The stores after `End` run even
/// when `End` reported `NoCamera`, so rebinding from NULL can still let
/// `Begin` apply.
pub fn set_camera(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    cam: u32,
) -> Result<RebindOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    let end_outcome = end(&*camera)?;
    set_word(camera, OFF_CAMERA, cam);
    let begin_outcome = begin(camera, need_look_from_eyes)?;
    Ok(RebindOutcome {
        end: end_outcome,
        begin: begin_outcome,
    })
}

/// `G_Camera::SetPlayer` (GAME VA 0x5ed170 / RVA 0x1ed170; mafia-re
/// `game/camera/g_camera.cpp:233-241` @ fe7e746; PS2 name per
/// `game/camera/g_camera.h` @ fe7e746).
/// Same shape as `set_camera` for the player word (`G_Camera+0x8`): `End()`,
/// store, `Begin()`.
pub fn set_player(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    player: u32,
) -> Result<RebindOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    let end_outcome = end(&*camera)?;
    set_word(camera, OFF_PLAYER, player);
    let begin_outcome = begin(camera, need_look_from_eyes)?;
    Ok(RebindOutcome {
        end: end_outcome,
        begin: begin_outcome,
    })
}

/// `G_Camera::SetCar` (GAME VA 0x5ed190 / RVA 0x1ed190; mafia-re
/// `game/camera/g_camera.cpp:242-258` @ fe7e746; PS2 name per
/// `game/camera/g_camera.h` @ fe7e746).
/// Native order: `End()`, store `car` at `G_Camera+0xc`, then with a car the
/// profile's car camera becomes `mode`, on foot the profile's foot camera
/// becomes `mode` and `free_look` (`G_Camera+0x94`) clears, then `Begin()` and
/// `mode_changed = true`. Unlike `SetMode` no profile keep is evaluated and
/// neither callee is skipped; the settings reads are live through
/// [`CameraSettings`] (only the taken branch reads).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetCarOutcome {
    pub end: EndOutcome,
    pub begin: BeginOutcome,
}

pub fn set_car<S: CameraSettings>(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    car: u32,
    settings: &mut S,
) -> Result<SetCarOutcome, CameraSetterError<S::Error>> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraSetterError::UnavailableOwner);
    }
    let end_outcome = end(&*camera).map_err(map_owner)?;
    set_word(camera, OFF_CAR, car);
    if car != 0 {
        let kept = settings.car_camera().map_err(CameraSetterError::Settings)?;
        set_word(camera, OFF_MODE, kept);
    } else {
        let kept = settings
            .foot_camera()
            .map_err(CameraSetterError::Settings)?;
        set_word(camera, OFF_MODE, kept);
        camera[OFF_FREE_LOOK] = 0;
    }
    let begin_outcome = begin(camera, need_look_from_eyes).map_err(map_owner)?;
    camera[OFF_MODE_CHANGED] = 1;
    Ok(SetCarOutcome {
        end: end_outcome,
        begin: begin_outcome,
    })
}

/// `G_Camera::Link` (GAME VA 0x5ed1e0 / RVA 0x1ed1e0; mafia-re
/// `game/camera/g_camera.cpp:259-268` @ fe7e746; PS2 name per
/// `game/camera/g_camera.h` @ fe7e746).
/// Stores the link frame (`G_Camera+0x0`): a non-NULL frame selects
/// `CAMERA_C_RAILWAY_IN` through `SetMode`, NULL selects the profile's foot
/// camera (read live, only on that path) through `SetMode`. Returns the
/// `SetMode` outcome with `begin` requested.
pub fn link<S: CameraSettings>(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    frame: u32,
    settings: &mut S,
) -> Result<SetModeOutcome, CameraSetterError<S::Error>> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraSetterError::UnavailableOwner);
    }
    set_word(camera, OFF_LINK, frame);
    let mode = if frame != 0 {
        CAMERA_C_RAILWAY_IN
    } else {
        settings
            .foot_camera()
            .map_err(CameraSetterError::Settings)?
    };
    set_mode(camera, need_look_from_eyes, mode, true).map_err(map_owner)
}

/// `G_Camera::SetSwing` (GAME VA 0x5ed210 / RVA 0x1ed210; mafia-re
/// `game/camera/g_camera.cpp:269-284` @ fe7e746; PS2 name per
/// `game/camera/g_camera.h` @ fe7e746).
/// Stores the swing triple (`G_Camera+0x58/0x5c/0x60`,
/// `g_camera.h:154-156` @ fe7e746). With `on == false` and a non-NULL frame
/// the scene and frame owners straighten the backdrop and the frame
/// (DEFERRED, reported in the outcome); the stores run first either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSwingOutcome {
    pub backdrop_restore_pending: bool,
}

pub fn set_swing(
    camera: &mut [u8],
    on: bool,
    amount: f32,
    frame: u32,
) -> Result<SetSwingOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    camera[OFF_SWING] = u8::from(on);
    set_word_f32(camera, OFF_SWING_AMOUNT, amount);
    set_word(camera, OFF_SWING_FRAME, frame);
    Ok(SetSwingOutcome {
        backdrop_restore_pending: !on && frame != 0,
    })
}

/// `G_Camera::LookAround` (GAME VA 0x5ed330 / RVA 0x1ed330; mafia-re
/// `game/camera/g_camera.cpp:285-313` @ fe7e746; PS2 name per
/// `game/camera/g_camera.h` @ fe7e746).
/// Turns the car cameras toward the `LOOK_*` angle: from behind on the right
/// the camera wraps the short way (`-= 2.0` past `+0.5`), mirrored from the
/// left, ahead clears the want, and behind picks the side the camera already
/// leans to. Only the `look_around_want` (`G_Camera+0x2c`) and `look_around`
/// (`G_Camera+0x30`) words move; other `dir` values are a no-op like the
/// native switch. Every constant and limit here is exactly representable, so
/// host single-rounding matches the native stores bit for bit.
pub fn look_around(camera: &mut [u8], dir: i32) -> Result<(), CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    match dir {
        LOOK_LEFT => {
            if word_f32(camera, OFF_LOOK_AROUND) > 0.5 {
                let updated = word_f32(camera, OFF_LOOK_AROUND) - 2.0;
                set_word_f32(camera, OFF_LOOK_AROUND, updated);
            }
            set_word_f32(camera, OFF_LOOK_AROUND_WANT, -0.5);
        }
        LOOK_AHEAD => set_word_f32(camera, OFF_LOOK_AROUND_WANT, 0.0),
        LOOK_RIGHT => {
            if word_f32(camera, OFF_LOOK_AROUND) < -0.5 {
                let updated = word_f32(camera, OFF_LOOK_AROUND) + 2.0;
                set_word_f32(camera, OFF_LOOK_AROUND, updated);
            }
            set_word_f32(camera, OFF_LOOK_AROUND_WANT, 0.5);
        }
        LOOK_BEHIND => {
            if word_f32(camera, OFF_LOOK_AROUND) > 0.0 {
                set_word_f32(camera, OFF_LOOK_AROUND_WANT, 1.0);
            } else {
                set_word_f32(camera, OFF_LOOK_AROUND_WANT, -1.0);
            }
        }
        _ => {}
    }
    Ok(())
}

/// `G_Camera::LookFromEyes` (GAME VA 0x5ed3e0 / RVA 0x1ed3e0; mafia-re
/// `game/camera/g_camera.cpp:314-318` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Pure read of the borrowed `needLookFromEyes` global (GAME 0x6bdb10 scope,
/// `g_camera.cpp:154-156` @ fe7e746): true while it is above `0.0`. NaN and
/// non-positive values report false, matching the native comparison.
pub fn look_from_eyes(need_look_from_eyes: u32) -> bool {
    f32::from_bits(need_look_from_eyes) > 0.0
}

/// `G_Camera::LezeDoToyoty` (GAME VA 0x5ed460 / RVA 0x1ed460; mafia-re
/// `game/camera/g_camera.cpp:341-360` @ fe7e746; PS2 name, Czech for "climbs
/// into the Toyota", per its declaration comment (`game/camera/g_camera.h` @ fe7e746).
/// With `on` the camera goes to `CAMERA_C_BEHIND_HELLBOY`, with `off` back to
/// the profile's car camera (read live, only on that path); an already
/// matching mode leaves every word untouched. The applied path is
/// `End()`, the mode store, `Begin()`, `mode_changed = true`, in that order.
/// The native off path reads the setting twice in the source text; the single
/// live read here is observably identical (no callbacks run between).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LezeOutcome {
    Unchanged,
    Applied(RebindOutcome),
}

pub fn leze_do_toyoty<S: CameraSettings>(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    on: bool,
    settings: &mut S,
) -> Result<LezeOutcome, CameraSetterError<S::Error>> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraSetterError::UnavailableOwner);
    }
    let target = if on {
        CAMERA_C_BEHIND_HELLBOY
    } else {
        settings.car_camera().map_err(CameraSetterError::Settings)?
    };
    if word(camera, OFF_MODE) == target {
        return Ok(LezeOutcome::Unchanged);
    }
    let end_outcome = end(&*camera).map_err(map_owner)?;
    set_word(camera, OFF_MODE, target);
    let begin_outcome = begin(camera, need_look_from_eyes).map_err(map_owner)?;
    camera[OFF_MODE_CHANGED] = 1;
    Ok(LezeOutcome::Applied(RebindOutcome {
        end: end_outcome,
        begin: begin_outcome,
    }))
}

/// `G_Camera::EnableFreeLook` (GAME VA 0x5f34c0 / RVA 0x1f34c0; mafia-re
/// `game/camera/g_camera.cpp:1259-1270` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Native order over the borrowed camera and the live `freeLookX`/`freeLookZ`
/// globals (GAME 0x6bdb20/0x6bdb24, `g_camera.cpp:169-172` @ fe7e746): when
/// free look was off both angles restart from zero, then `free_look`
/// (`G_Camera+0x94`) stores `on`, then with `behind` the car cameras from
/// `CAMERA_C_CHASE` on (up to `CAMERA_C_END`, exclusive) go back to
/// `CAMERA_C_BEHIND_HELLBOY` through `SetMode` with `begin`. Returns the
/// `SetMode` outcome only when that rebind ran.
pub fn enable_free_look(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    free_look_x: &mut f32,
    free_look_z: &mut f32,
    on: bool,
    behind: bool,
) -> Result<Option<SetModeOutcome>, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    if camera[OFF_FREE_LOOK] == 0 {
        *free_look_x = 0.0;
        *free_look_z = 0.0;
    }
    camera[OFF_FREE_LOOK] = u8::from(on);
    let mode = word(camera, OFF_MODE);
    if behind && mode > CAMERA_C_BEHIND_HELLBOY && mode < CAMERA_C_END && mode >= CAMERA_C_CHASE {
        Ok(Some(set_mode(
            camera,
            need_look_from_eyes,
            CAMERA_C_BEHIND_HELLBOY,
            true,
        )?))
    } else {
        Ok(None)
    }
}

/// `G_Camera::Unlock` (GAME VA 0x5f3fd0 / RVA 0x1f3fd0; mafia-re
/// `game/camera/g_camera.cpp:1429-1435` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Pure classification: only `CAMERA_C_LOCK` pops back through
/// `PopMode(clear)` (DEFERRED: the mode/vector owners apply it); any other
/// mode leaves every word untouched. No slice stores happen here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnlockOutcome {
    Unchanged,
    PopPending { clear: bool },
}

pub fn unlock(camera: &[u8], clear: bool) -> Result<UnlockOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    if word(camera, OFF_MODE) == CAMERA_C_LOCK {
        Ok(UnlockOutcome::PopPending { clear })
    } else {
        Ok(UnlockOutcome::Unchanged)
    }
}

/// `G_Camera::IsInDialog` (GAME VA 0x5f4690 / RVA 0x1f4690; mafia-re
/// `game/camera/g_camera.cpp:1531-1535` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Pure mode read: true exactly while `mode == CAMERA_C_DIALOG`.
pub fn is_in_dialog(camera: &[u8]) -> Result<bool, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    Ok(word(camera, OFF_MODE) == CAMERA_C_DIALOG)
}

/// `G_Camera::SetSniperFov` (GAME VA 0x5f4a70 / RVA 0x1f4a70; mafia-re
/// `game/camera/g_camera.cpp:1608-1617` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Stores the sniper field of view (`G_Camera+0x90`); in `CAMERA_CP_SNIPER`
/// the HUD applies it to the camera at once and updates it (DEFERRED,
/// reported in the outcome). The store runs first either way.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SetSniperFovOutcome {
    pub hud_apply_pending: bool,
}

pub fn set_sniper_fov(
    camera: &mut [u8],
    fov: f32,
) -> Result<SetSniperFovOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    set_word_f32(camera, OFF_SNIPER_FOV, fov);
    Ok(SetSniperFovOutcome {
        hud_apply_pending: word(camera, OFF_MODE) == CAMERA_CP_SNIPER,
    })
}

/// `G_Camera::SetSniperMode` (GAME VA 0x5f4ab0 / RVA 0x1f4ab0; mafia-re
/// `game/camera/g_camera.cpp:1619-1632` @ fe7e746, declared in
/// `game/camera/g_camera.h` @ fe7e746; the header marks no separate PS2
/// provenance for this declarator, so only the address and the body order are
/// claimed).
/// Pure dispatch classification: enabling from a non-sniper mode pushes the
/// state and rebinds through `SetMode` (DEFERRED), disabling from the sniper
/// mode pops back (DEFERRED); matching requests are a no-op. No slice stores
/// happen here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SniperModeOutcome {
    Unchanged,
    EnterSniperPending,
    ExitSniperPending,
}

pub fn set_sniper_mode(camera: &[u8], on: bool) -> Result<SniperModeOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    let sniper = word(camera, OFF_MODE) == CAMERA_CP_SNIPER;
    if on {
        if sniper {
            Ok(SniperModeOutcome::Unchanged)
        } else {
            Ok(SniperModeOutcome::EnterSniperPending)
        }
    } else if sniper {
        Ok(SniperModeOutcome::ExitSniperPending)
    } else {
        Ok(SniperModeOutcome::Unchanged)
    }
}

/// `G_Camera::SetLookTo` (GAME VA 0x5f4a00 / RVA 0x1f4a00; mafia-re
/// `game/camera/g_camera.cpp:1596-1606` @ fe7e746; PS2 name per its declaration
/// comment in `game/camera/g_camera.h` @ fe7e746).
/// Stores the look-at frame (`G_Camera+0x88`), rebinds through
/// `SetMode(CAMERA_C_LOOK_TO, true)`, and with a non-NULL camera the I3D owner
/// places it at `pos` and updates it (DEFERRED; `pos` is carried in the
/// outcome for that owner since this slice keeps no copy).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetLookToOutcome {
    pub mode: SetModeOutcome,
    pub camera_place_pending: bool,
    pub pos: [f32; 3],
}

pub fn set_look_to(
    camera: &mut [u8],
    need_look_from_eyes: &mut u32,
    frame: u32,
    pos: [f32; 3],
) -> Result<SetLookToOutcome, CameraBeginError> {
    if camera.len() != CAMERA_BYTES {
        return Err(CameraBeginError::UnavailableOwner);
    }
    set_word(camera, OFF_LOOK_TO, frame);
    let mode = set_mode(camera, need_look_from_eyes, CAMERA_C_LOOK_TO, true)?;
    Ok(SetLookToOutcome {
        camera_place_pending: word(camera, OFF_CAMERA) != 0,
        mode,
        pos,
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
        bytes[OFF_NEXT_MODE..OFF_NEXT_MODE + 4].copy_from_slice(&next_mode.to_le_bytes());
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
            assert_eq!(end(&full[..short]), Err(CameraBeginError::UnavailableOwner));
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
    #[test]
    fn sniper_swap_routes_through_hud_owner() {
        // `Begin` selects the sniper arm by mode alone; the flag swap itself
        // is owned by `IndicatorsState` (`g_camera.cpp:1213-1215,1238-1240`).
        use crate::indicators::{IndicatorsState, TimerClock, INDF_CROSSHAIR, INDF_SNIPER};
        let mut bytes = camera_with(0x1000, 0x2000, 0x3000, CAMERA_CP_SNIPER, 0, 0);
        let mut need_look = 0u32;
        match begin(&mut bytes, &mut need_look).unwrap() {
            BeginOutcome::Applied(applied) => assert_eq!(applied.link, BeginLink::Sniper),
            BeginOutcome::NoCamera => panic!("expected Applied"),
        }
        let mut hud = IndicatorsState::constructed(TimerClock {
            hour: 0,
            minute: 0,
            second: 0,
        });
        hud.set_flags(INDF_CROSSHAIR);
        hud.apply_camera_begin_sniper();
        assert!(!hud.is_shown(INDF_CROSSHAIR));
        assert!(hud.is_shown(INDF_SNIPER));
        // `End` still sees the sniper mode and classifies the restore.
        match end(&bytes).unwrap() {
            EndOutcome::Classified(applied) => assert!(applied.sniper_hud_restore),
            EndOutcome::NoCamera => panic!("expected Classified"),
        }
        hud.apply_camera_end_sniper();
        assert!(hud.is_shown(INDF_CROSSHAIR));
        assert!(!hud.is_shown(INDF_SNIPER));
    }
    struct Settings {
        car: u32,
        foot: u32,
        car_reads: usize,
        foot_reads: usize,
        fail: bool,
    }
    impl CameraSettings for Settings {
        type Error = &'static str;
        fn car_camera(&mut self) -> Result<u32, Self::Error> {
            self.car_reads += 1;
            if self.fail {
                Err("settings unavailable")
            } else {
                Ok(self.car)
            }
        }
        fn foot_camera(&mut self) -> Result<u32, Self::Error> {
            self.foot_reads += 1;
            if self.fail {
                Err("settings unavailable")
            } else {
                Ok(self.foot)
            }
        }
    }
    fn settings(car: u32, foot: u32) -> Settings {
        Settings {
            car,
            foot,
            car_reads: 0,
            foot_reads: 0,
            fail: false,
        }
    }
    #[test]
    fn get_car_rva_b4d0_reads_car_word() {
        // RVA 0xb4d0: NULL car reads as None, nonzero as Some; read-only.
        let bytes = camera_with(0x1000, 0x2000, 0, 1, 0, 0xa5);
        let before = bytes;
        assert_eq!(get_car(&bytes), Ok(None));
        assert_eq!(bytes, before);
        let bytes = camera_with(0x1000, 0x2000, 0x3000, 1, 0, 0xa5);
        assert_eq!(get_car(&bytes), Ok(Some(0x3000)));
        assert_eq!(
            get_car(&bytes[..CAMERA_BYTES - 1]),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn set_camera_rva_1ed150_end_old_begin_new() {
        // RVA 0x1ed150: End sees the old camera word, Begin the new one.
        let mut bytes = camera_with(0x1000, 0x2000, 0x3000, 1, 4, 0);
        let mut need_look = 0xdead_beef;
        let outcome = set_camera(&mut bytes, &mut need_look, 0x4000).unwrap();
        assert_eq!(
            outcome.end,
            EndOutcome::Classified(EndApplied {
                sniper_hud_restore: false,
                relink_primary: false,
                car_restore_pending: true,
                camera_update_pending: true,
            })
        );
        match outcome.begin {
            BeginOutcome::Applied(applied) => assert!(applied.matched),
            BeginOutcome::NoCamera => panic!("expected Applied"),
        }
        assert_eq!(word(&bytes, OFF_CAMERA), 0x4000);
        // Rebinding from NULL: End reports NoCamera, the store still runs,
        // and Begin applies on the new word.
        let mut bytes = camera_with(0, 0x2000, 0x3000, 12, 0, 0);
        let mut need_look = 0u32;
        let outcome = set_camera(&mut bytes, &mut need_look, 0x4000).unwrap();
        assert_eq!(outcome.end, EndOutcome::NoCamera);
        assert_eq!(word(&bytes, OFF_PARAMS), 0x6388e4);
        assert_eq!(need_look, 0);
        // Rebinding to NULL: End classifies, Begin reports NoCamera.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let mut need_look = 0x1111_1111;
        let outcome = set_camera(&mut bytes, &mut need_look, 0).unwrap();
        assert!(matches!(outcome.end, EndOutcome::Classified(_)));
        assert_eq!(outcome.begin, BeginOutcome::NoCamera);
        assert_eq!(need_look, 0x1111_1111);
    }
    #[test]
    fn set_player_rva_1ed170_end_old_begin_new() {
        // RVA 0x1ed170: same End/store/Begin shape for the player word.
        // Mode 12 (C Chase) takes the zero path, so the live player word
        // drives the deferred fade gate and clears needLook.
        let mut bytes = camera_with(0x1000, 0x2000, 0, 12, 0, 0);
        let mut need_look = 0xdead_beef;
        let outcome = set_player(&mut bytes, &mut need_look, 0x3000).unwrap();
        assert!(matches!(outcome.end, EndOutcome::Classified(_)));
        match outcome.begin {
            BeginOutcome::Applied(applied) => {
                assert!(applied.matched);
                assert!(!applied.eyes);
                assert!(applied.fade_reset_pending);
                assert!(applied.need_look_cleared);
            }
            BeginOutcome::NoCamera => panic!("expected Applied"),
        }
        assert_eq!(need_look, 0);
        assert_eq!(word(&bytes, OFF_PLAYER), 0x3000);
    }
    #[test]
    fn set_car_rva_1ed190_branch_settings_and_flags() {
        // RVA 0x1ed190, car path: the car camera becomes the mode.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0xa5);
        bytes[OFF_FREE_LOOK] = 1;
        let mut need_look = 0u32;
        let mut profile = settings(12, 1);
        let outcome = set_car(&mut bytes, &mut need_look, 0x3000, &mut profile).unwrap();
        assert_eq!(word(&bytes, OFF_CAR), 0x3000);
        assert_eq!(word(&bytes, OFF_MODE), 12);
        assert_eq!(bytes[OFF_FREE_LOOK], 1);
        assert_eq!(bytes[OFF_MODE_CHANGED], 1);
        assert_eq!((profile.car_reads, profile.foot_reads), (1, 0));
        assert!(matches!(outcome.begin, BeginOutcome::Applied(_)));
        // Foot path: the foot camera becomes the mode and free look clears.
        let mut bytes = camera_with(0x1000, 0, 0x3000, 12, 0, 0xa5);
        bytes[OFF_FREE_LOOK] = 1;
        let mut need_look = 0x2222_2222;
        let mut profile = settings(12, 3);
        let outcome = set_car(&mut bytes, &mut need_look, 0, &mut profile).unwrap();
        assert_eq!(word(&bytes, OFF_MODE), 3);
        assert_eq!(bytes[OFF_FREE_LOOK], 0);
        assert_eq!((profile.car_reads, profile.foot_reads), (0, 1));
        assert!(matches!(outcome.begin, BeginOutcome::Applied(_)));
        // A failing settings read leaves the car store visible and reports.
        let mut bytes = camera_with(0x1000, 0, 0, 12, 0, 0);
        let mut need_look = 0u32;
        let mut profile = settings(12, 3);
        profile.fail = true;
        assert_eq!(
            set_car(&mut bytes, &mut need_look, 0, &mut profile),
            Err(CameraSetterError::Settings("settings unavailable"))
        );
        assert_eq!(word(&bytes, OFF_CAR), 0);
    }
    #[test]
    fn link_rva_1ed1e0_frame_selects_railway() {
        // RVA 0x1ed1e0: a frame selects C_RAILWAY_IN without settings reads.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let mut need_look = 0u32;
        let mut profile = settings(12, 1);
        let outcome = link(&mut bytes, &mut need_look, 0x5000, &mut profile).unwrap();
        assert_eq!(word(&bytes, OFF_LINK), 0x5000);
        assert_eq!(word(&bytes, OFF_MODE), CAMERA_C_RAILWAY_IN);
        assert_eq!((profile.car_reads, profile.foot_reads), (0, 0));
        assert!(outcome.applied);
        assert_eq!(outcome.keep, ProfileKeep::None);
        // NULL selects the live foot camera.
        let mut bytes = camera_with(0x1000, 0, 0, 17, 0, 0);
        let mut need_look = 0u32;
        let mut profile = settings(12, 3);
        let outcome = link(&mut bytes, &mut need_look, 0, &mut profile).unwrap();
        assert_eq!(word(&bytes, OFF_MODE), 3);
        assert_eq!(outcome.keep, ProfileKeep::FootCamera(3));
        assert_eq!((profile.car_reads, profile.foot_reads), (0, 1));
    }
    #[test]
    fn set_swing_rva_1ed210_stores_and_defers_restore() {
        // RVA 0x1ed210: the triple stores; only off-with-frame defers.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let outcome = set_swing(&mut bytes, true, 2.5, 0x6000).unwrap();
        assert_eq!(bytes[OFF_SWING], 1);
        assert_eq!(
            word_f32(&bytes, OFF_SWING_AMOUNT).to_bits(),
            2.5f32.to_bits()
        );
        assert_eq!(word(&bytes, OFF_SWING_FRAME), 0x6000);
        assert!(!outcome.backdrop_restore_pending);
        let outcome = set_swing(&mut bytes, false, 0.0, 0x6000).unwrap();
        assert_eq!(bytes[OFF_SWING], 0);
        assert!(outcome.backdrop_restore_pending);
        let outcome = set_swing(&mut bytes, false, 0.0, 0).unwrap();
        assert!(!outcome.backdrop_restore_pending);
        assert_eq!(
            set_swing(&mut bytes[..OFF_SWING], true, 1.0, 0),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn look_around_rva_1ed330_turns_want_and_wraps() {
        // RVA 0x1ed330: LEFT wraps the short way, RIGHT mirrors it.
        let mut bytes = [0u8; CAMERA_BYTES];
        bytes[OFF_LOOK_AROUND..OFF_LOOK_AROUND + 4].copy_from_slice(&0.7f32.to_le_bytes());
        look_around(&mut bytes, LOOK_LEFT).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND).to_bits(),
            (-1.3f32).to_bits()
        );
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND_WANT).to_bits(),
            (-0.5f32).to_bits()
        );
        let mut bytes = [0u8; CAMERA_BYTES];
        bytes[OFF_LOOK_AROUND..OFF_LOOK_AROUND + 4].copy_from_slice(&(-0.7f32).to_le_bytes());
        look_around(&mut bytes, LOOK_RIGHT).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND).to_bits(),
            1.3f32.to_bits()
        );
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND_WANT).to_bits(),
            0.5f32.to_bits()
        );
        // No wrap past the thresholds; AHEAD only clears the want.
        let mut bytes = [0u8; CAMERA_BYTES];
        bytes[OFF_LOOK_AROUND..OFF_LOOK_AROUND + 4].copy_from_slice(&0.5f32.to_le_bytes());
        look_around(&mut bytes, LOOK_LEFT).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND).to_bits(),
            0.5f32.to_bits()
        );
        look_around(&mut bytes, LOOK_AHEAD).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND_WANT).to_bits(),
            0.0f32.to_bits()
        );
        // BEHIND follows the side the camera already leans to.
        let mut bytes = [0u8; CAMERA_BYTES];
        bytes[OFF_LOOK_AROUND..OFF_LOOK_AROUND + 4].copy_from_slice(&0.25f32.to_le_bytes());
        look_around(&mut bytes, LOOK_BEHIND).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND_WANT).to_bits(),
            1.0f32.to_bits()
        );
        let mut bytes = [0u8; CAMERA_BYTES];
        look_around(&mut bytes, LOOK_BEHIND).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_LOOK_AROUND_WANT).to_bits(),
            (-1.0f32).to_bits()
        );
        // Unknown directions are a no-op.
        let mut bytes = [0x3c; CAMERA_BYTES];
        let before = bytes;
        look_around(&mut bytes, 99).unwrap();
        assert_eq!(bytes, before);
        assert_eq!(
            look_around(&mut bytes[..4], LOOK_AHEAD),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn look_from_eyes_rva_1ed3e0_positive_float_only() {
        // RVA 0x1ed3e0: strictly above zero; NaN and negatives are false.
        assert!(look_from_eyes(f32::MIN_POSITIVE.to_bits()));
        assert!(!look_from_eyes(0.0f32.to_bits()));
        assert!(!look_from_eyes((-1.0f32).to_bits()));
        assert!(!look_from_eyes(f32::NAN.to_bits()));
        assert!(!look_from_eyes(u32::MAX));
    }
    #[test]
    fn leze_do_toyoty_rva_1ed460_guarded_rebind() {
        // RVA 0x1ed460, on path: rebinds unless already behind Hellboy.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let mut need_look = 0u32;
        let mut profile = settings(12, 3);
        match leze_do_toyoty(&mut bytes, &mut need_look, true, &mut profile).unwrap() {
            LezeOutcome::Applied(rebind) => {
                assert!(matches!(rebind.begin, BeginOutcome::Applied(_)));
            }
            LezeOutcome::Unchanged => panic!("expected Applied"),
        }
        assert_eq!(word(&bytes, OFF_MODE), CAMERA_C_BEHIND_HELLBOY);
        assert_eq!(bytes[OFF_MODE_CHANGED], 1);
        assert_eq!((profile.car_reads, profile.foot_reads), (0, 0));
        let mut bytes = camera_with(0x1000, 0, 0, CAMERA_C_BEHIND_HELLBOY, 0, 0);
        let before = bytes;
        let mut need_look = 0x99u32;
        let mut profile = settings(12, 3);
        assert_eq!(
            leze_do_toyoty(&mut bytes, &mut need_look, true, &mut profile),
            Ok(LezeOutcome::Unchanged)
        );
        assert_eq!(bytes, before);
        // Off path: back to the live car camera, unchanged when it matches.
        let mut bytes = camera_with(0x1000, 0, 0, 7, 0, 0);
        let mut need_look = 0u32;
        let mut profile = settings(12, 3);
        assert!(matches!(
            leze_do_toyoty(&mut bytes, &mut need_look, false, &mut profile).unwrap(),
            LezeOutcome::Applied(_)
        ));
        assert_eq!(word(&bytes, OFF_MODE), 12);
        assert_eq!(profile.car_reads, 1);
        let mut bytes = camera_with(0x1000, 0, 0, 12, 0, 0);
        let before = bytes;
        let mut need_look = 0u32;
        let mut profile = settings(12, 3);
        assert_eq!(
            leze_do_toyoty(&mut bytes, &mut need_look, false, &mut profile),
            Ok(LezeOutcome::Unchanged)
        );
        assert_eq!(bytes, before);
        assert_eq!(profile.car_reads, 1);
    }
    #[test]
    fn enable_free_look_rva_1f34c0_zeroes_and_rebinds() {
        // RVA 0x1f34c0: a settled-off free look restarts the angles from zero.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let mut need_look = 0u32;
        let (mut x, mut z) = (1.5f32, -2.5f32);
        assert_eq!(
            enable_free_look(&mut bytes, &mut need_look, &mut x, &mut z, true, false),
            Ok(None)
        );
        assert_eq!((x.to_bits(), z.to_bits()), (0, 0));
        assert_eq!(bytes[OFF_FREE_LOOK], 1);
        // An already-on free look keeps the angles.
        let (mut x, mut z) = (1.5f32, -2.5f32);
        assert_eq!(
            enable_free_look(&mut bytes, &mut need_look, &mut x, &mut z, false, false),
            Ok(None)
        );
        assert_eq!(
            (x.to_bits(), z.to_bits()),
            (1.5f32.to_bits(), (-2.5f32).to_bits())
        );
        assert_eq!(bytes[OFF_FREE_LOOK], 0);
        // Behind rebinds only CHASE..END (12..16); 11 and 16 stay put.
        for (mode, rebinds) in [
            (7u32, false),
            (11, false),
            (12, true),
            (15, true),
            (16, false),
        ] {
            let mut bytes = camera_with(0x1000, 0, 0, mode, 0, 0);
            let mut need_look = 0u32;
            let (mut x, mut z) = (0.0f32, 0.0f32);
            let outcome =
                enable_free_look(&mut bytes, &mut need_look, &mut x, &mut z, true, true).unwrap();
            assert_eq!(outcome.is_some(), rebinds, "mode {}", mode);
            if rebinds {
                let outcome = outcome.unwrap();
                assert_eq!(word(&bytes, OFF_MODE), CAMERA_C_BEHIND_HELLBOY);
                assert!(outcome.applied);
            } else {
                assert_eq!(word(&bytes, OFF_MODE), mode);
            }
        }
        // Without behind no mode dispatch runs even inside the range.
        let mut bytes = camera_with(0x1000, 0, 0, 12, 0, 0);
        let mut need_look = 0u32;
        let (mut x, mut z) = (0.0f32, 0.0f32);
        assert_eq!(
            enable_free_look(&mut bytes, &mut need_look, &mut x, &mut z, true, false),
            Ok(None)
        );
        assert_eq!(word(&bytes, OFF_MODE), 12);
    }
    #[test]
    fn unlock_rva_1f3fd0_classifies_pop() {
        // RVA 0x1f3fd0: only CAMERA_C_LOCK defers a pop; no stores either way.
        let bytes = camera_with(0x1000, 0, 0, CAMERA_C_LOCK, 0, 0xa5);
        assert_eq!(
            unlock(&bytes, true),
            Ok(UnlockOutcome::PopPending { clear: true })
        );
        assert_eq!(
            unlock(&bytes, false),
            Ok(UnlockOutcome::PopPending { clear: false })
        );
        for mode in [1u32, 7, 12, 19, 23] {
            let bytes = camera_with(0x1000, 0, 0, mode, 0, 0xa5);
            assert_eq!(
                unlock(&bytes, true),
                Ok(UnlockOutcome::Unchanged),
                "mode {}",
                mode
            );
            assert_eq!(bytes, camera_with(0x1000, 0, 0, mode, 0, 0xa5));
        }
        assert_eq!(
            unlock(&bytes[..4], true),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn is_in_dialog_rva_1f4690_mode_compare() {
        // RVA 0x1f4690: true exactly in CAMERA_C_DIALOG.
        for mode in [1u32, 5, 7, 12, 17, 23, 24] {
            let bytes = camera_with(0x1000, 0, 0, mode, 0, 0);
            assert_eq!(is_in_dialog(&bytes), Ok(false), "mode {}", mode);
        }
        let bytes = camera_with(0x1000, 0, 0, CAMERA_C_DIALOG, 0, 0);
        assert_eq!(is_in_dialog(&bytes), Ok(true));
        assert_eq!(
            is_in_dialog(&bytes[..4]),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn set_sniper_fov_rva_1f4a70_store_and_hud_gate() {
        // RVA 0x1f4a70: the store always runs; HUD applies only in sniper.
        let mut bytes = camera_with(0x1000, 0, 0, CAMERA_CP_SNIPER, 0, 0);
        let outcome = set_sniper_fov(&mut bytes, 0.35).unwrap();
        assert_eq!(
            word_f32(&bytes, OFF_SNIPER_FOV).to_bits(),
            0.35f32.to_bits()
        );
        assert!(outcome.hud_apply_pending);
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let outcome = set_sniper_fov(&mut bytes, 0.35).unwrap();
        assert!(!outcome.hud_apply_pending);
        assert_eq!(
            set_sniper_fov(&mut bytes[..OFF_SNIPER_FOV], 0.35),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn set_sniper_mode_rva_1f4ab0_dispatch() {
        // RVA 0x1f4ab0: matching requests are a no-op; the rest defers.
        let bytes = camera_with(0x1000, 0, 0, CAMERA_CP_SNIPER, 0, 0);
        assert_eq!(
            set_sniper_mode(&bytes, true),
            Ok(SniperModeOutcome::Unchanged)
        );
        assert_eq!(
            set_sniper_mode(&bytes, false),
            Ok(SniperModeOutcome::ExitSniperPending)
        );
        let bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        assert_eq!(
            set_sniper_mode(&bytes, true),
            Ok(SniperModeOutcome::EnterSniperPending)
        );
        assert_eq!(
            set_sniper_mode(&bytes, false),
            Ok(SniperModeOutcome::Unchanged)
        );
        assert_eq!(
            set_sniper_mode(&bytes[..4], true),
            Err(CameraBeginError::UnavailableOwner)
        );
    }
    #[test]
    fn set_look_to_rva_1f4a00_store_and_rebind() {
        // RVA 0x1f4a00: stores the frame, rebinds through SetMode, and the
        // camera placement stays deferred with the position carried along.
        let mut bytes = camera_with(0x1000, 0, 0, 1, 0, 0);
        let mut need_look = 0u32;
        let outcome = set_look_to(&mut bytes, &mut need_look, 0x7000, [1.0, 2.0, 3.0]).unwrap();
        assert_eq!(word(&bytes, OFF_LOOK_TO), 0x7000);
        assert_eq!(word(&bytes, OFF_MODE), CAMERA_C_LOOK_TO);
        assert!(outcome.camera_place_pending);
        assert_eq!(outcome.pos, [1.0, 2.0, 3.0]);
        assert!(outcome.mode.applied);
        // NULL camera: SetMode still stores the mode; placement stays off.
        let mut bytes = camera_with(0, 0, 0, 1, 0, 0);
        let mut need_look = 0u32;
        let outcome = set_look_to(&mut bytes, &mut need_look, 0x7000, [0.0, 0.0, 0.0]).unwrap();
        assert_eq!(outcome.mode.begin, Some(BeginOutcome::NoCamera));
        assert!(!outcome.camera_place_pending);
    }
}
