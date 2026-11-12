//! C_mission lifetime, Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! Constructor RVA 0x13f010; Close RVA 0x5405e0, unopened and active paths.
//! Original PS2 C_mission names corroborate identity, not PC member offsets.
//! Explicit seeded storage preserves uninitialized pointers, padding and far range.
//! This owner does not open missions or initialize the game.

use crate::c_str::CStrBackend;
use crate::city_base_cache::{BaseCacheCloseReport, CityBaseCache};
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::itexture::TextureImageSource;
use crate::mission_components::{
    GameConstructorGlobals, GameConstructorInputs, MissionAllocation, MissionAllocator,
    MissionComponent, MissionComponentKind,
};
use crate::mission_options::MissionOptions;
use crate::mission_resources::MissionCaches;
use crate::scene_ownership::SceneKey;

#[derive(Clone, Copy, Debug)]
pub struct MissionInitInputs {
    /// Observer identity bound exclusively to the actual returned SceneKey.
    /// It is never dereferenced or used to create a second frame owner.
    pub scene_identity: u32,
    pub effects_stack_control: u8,
    pub game: GameConstructorInputs,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionPhase {
    CreateScene,
    Allocate(MissionComponentKind),
    Construct(MissionComponentKind),
    ClearCaches,
    Close,
    Destroy(MissionComponentKind),
    Free(MissionComponentKind),
    ReleaseScene,
    FreeUnknown68,
    DestroyVector(usize),
    Complete,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionFailure {
    pub phase: MissionPhase,
    pub detail: String,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LifecycleDomains {
    driver: u64,
    arena: crate::script_scheduler::FrameReferenceId,
    resources: u64,
    textures: u64,
    storage: u64,
    memory: crate::mission_components::MissionMemoryDomainId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ComponentDisposal {
    Ready,
    DestroyEntered,
    Destroyed,
    FreeEntered,
    Released,
}
struct OwnedComponent {
    value: MissionComponent,
    disposal: ComponentDisposal,
}
/// Host terminal progress, independent of the original Init/UnInit phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionTerminalPhase {
    NotStarted,
    Components,
    Scene,
    RetainedOwners,
    Complete,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionTerminalIssue {
    pub phase: MissionTerminalPhase,
    pub detail: String,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MissionTerminalProgress {
    pub phase: MissionTerminalPhase,
    pub components_released: usize,
    pub scene_reference_released: bool,
    pub retained: Vec<MissionTerminalIssue>,
}
impl Default for MissionTerminalProgress {
    fn default() -> Self {
        Self {
            phase: MissionTerminalPhase::NotStarted,
            components_released: 0,
            scene_reference_released: false,
            retained: Vec::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionTerminalCallError {
    NotInterrupted,
    AlreadyEntered,
    ForeignDomain,
    RetainedOwners,
}

/// Original PC allocation size, independent of the larger PS2 mission.
pub const MISSION_BYTES: usize = 0xdc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionLifecycleError {
    /// Active Close requires the game, actors, programs and scene Close owners.
    ActiveCloseUnavailable,
    Destructed,
    TerminalDiscardEntered,
}

/// Explicit unsupported boundary for `C_mission::Init`, GAME RVA 0x13f230, VA
/// 0x53f230 (mafia-re `game/mission/Mission.cpp:91-111` at pin `fe7e746`):
/// the scene driver/device owner is outside this lifecycle contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MissionInitError {
    SceneDriverUnavailable,
}

pub struct MissionLifecycle {
    identity: u32,
    bytes: [u8; MISSION_BYTES],
    destructed: bool,
    scene: Option<SceneKey>,
    pending_scene: Option<SceneKey>,
    pending_listener: Option<SceneKey>,
    components: Vec<OwnedComponent>,
    pending_component: Option<MissionComponent>,
    rejected_allocation: Option<MissionAllocation>,
    phase: MissionPhase,
    interrupted: bool,
    domains: Option<LifecycleDomains>,
    first_failure: Option<MissionFailure>,
    terminal: MissionTerminalProgress,
    scene_release_entered: bool,
}
impl MissionLifecycle {
    /// Execute the constructor's stores over caller-supplied allocation bytes.
    /// The vector control byte comes from saved ECX's most significant byte.
    pub fn construct(identity: u32, seed: [u8; MISSION_BYTES]) -> Self {
        let mut out = Self {
            identity,
            bytes: seed,
            destructed: false,
            scene: None,
            pending_scene: None,
            pending_listener: None,
            components: Vec::new(),
            pending_component: None,
            rejected_allocation: None,
            phase: MissionPhase::Complete,
            interrupted: false,
            domains: None,
            first_failure: None,
            terminal: MissionTerminalProgress::default(),
            scene_release_entered: false,
        };
        for offset in [0, 4, 0x10, 0x28, 0x68] {
            out.put_word(offset, 0);
        }
        out.bytes[0x0c] = 0;
        for offset in [0x14, 0x34, 0x44, 0x54, 0x80, 0xac, 0xbc, 0xcc] {
            out.bytes[offset] = (identity >> 24) as u8;
            for field in [4, 8, 12] {
                out.put_word(offset + field, 0);
            }
        }
        for offset in [0x90, 0x94] {
            out.put_word(offset, 1f32.to_bits());
        }
        for offset in [0x9c, 0xa0, 0xa8] {
            out.put_word(offset, 2);
        }
        out.put_word(0xa4, 3);
        out
    }
    pub fn scene(&self) -> Option<SceneKey> {
        self.scene
    }
    /// Trivial `C_mission::GetTShift` (FUNCTION: GAME 0x547120, mafia-re
    /// `game/mission/Mission.h:177-182`): reads the tshift member (mission
    /// +0x28, the `C_tshift` the mission owns). The host keeps the member's
    /// identity word; no tshift owner is resolved here.
    pub fn get_tshift(&self) -> i32 {
        self.word(0x28) as i32
    }
    /// Working name (no outline Game.exe entry is claimed; mafia-re
    /// `game/mission/Mission.h:228-233`, `C_mission::GetGame`): reads the
    /// game member (mission +0x24, the `C_game` the mission owns). The
    /// reference only ventures a guess at an outline copy (`Mission.h:230`,
    /// `0x47ae50`, inventory-only with no unique body), so no RVA is
    /// claimed here. The host keeps the member's identity word; no game
    /// owner is resolved here.
    pub fn get_game(&self) -> i32 {
        self.word(0x24) as i32
    }
    /// Working name (no outline Game.exe entry is claimed; mafia-re
    /// `game/mission/Mission.h:278-281`, `C_mission::GetScene`): reads the
    /// scene member (mission +0x10). No outline copy exists in the
    /// reference, so no RVA is claimed here. Unlike
    /// [`MissionLifecycle::scene`], which resolves the owned `SceneKey`,
    /// this reads the raw member word, like [`MissionLifecycle::get_tshift`].
    pub fn get_scene(&self) -> i32 {
        self.word(0x10) as i32
    }
    /// Working name (no outline Game.exe entry is claimed; mafia-re
    /// `game/mission/Mission.h:268-271`, `C_mission::GetOptions`): reads the
    /// staged video/audio option words (mission +0x90, `S_mission_options`
    /// layout `Mission.h:23-31`) into the option state. The reference folds
    /// the only outline copy with `I3D_frame::GetScale` (`0x47aec0`,
    /// inventory-only, no unique body), so no RVA is claimed here.
    pub fn get_options(&self) -> MissionOptions {
        MissionOptions {
            level_of_detail: f32::from_le_bytes(self.word(0x90).to_le_bytes()),
            draw_distance: f32::from_le_bytes(self.word(0x94).to_le_bytes()),
            far_range: f32::from_le_bytes(self.word(0x98).to_le_bytes()),
            game_effects: self.word(0x9c) as i32,
            game_sounds: self.word(0xa0) as i32,
            shadows: self.word(0xa4) as i32,
            particles: self.word(0xa8) as i32,
        }
    }

    /// Working name (no outline Game.exe entry is claimed; inline only,
    /// mafia-re `game/mission/Mission.h:290-294`,
    /// `C_mission::ResetMissionNumbers`): clears both staged mission numbers
    /// (mission +0x70 `next_mission_number`, +0x74 `mission_number`) through
    /// one load of the mission, as the reference does. The body is inlined
    /// at the single `main` call site, so no Game.exe function entry exists
    /// and none is claimed here.
    pub fn reset_mission_numbers(&mut self) {
        self.put_word(0x70, 0);
        self.put_word(0x74, 0);
    }

    /// Working name (no outline Game.exe entry is claimed; folded copy,
    /// mafia-re `game/mission/Mission.h:272-277`,
    /// `C_mission::GetMissionName`): reads the staged mission-name word
    /// (mission +0x68, `char *mission_name`). The reference folds the only
    /// outline copy with `C_actor::GetFrame` (kept as
    /// `BaseActorState::frame_id`, GAME 0x47afb0 / RVA 0x7afb0), so the copy
    /// names the shared body and no separate mission RVA is claimed here.
    /// The host keeps the word; string ownership stays with the mission
    /// name owner.
    pub fn mission_name_word(&self) -> u32 {
        self.word(0x68)
    }

    /// Working name (no outline Game.exe entry is claimed; inline only,
    /// mafia-re `game/mission/Mission.h:117-120`, `C_mission::last_actor_number`
    /// at +0x8): reads the last number `C_actor::Init` gave an actor. `Open`
    /// clears it. The native word is read inline with no dedicated getter,
    /// so no RVA is claimed.
    pub fn last_actor_number(&self) -> i32 {
        self.word(0x08) as i32
    }

    /// Working name (no outline Game.exe entry is claimed; inline only,
    /// mafia-re `game/mission/Mission.h:120-122`, `C_mission::opened` at
    /// +0xc): reports whether a mission is open. `Close` does nothing
    /// without it and clears it. No dedicated getter exists natively, so no
    /// RVA is claimed.
    pub fn is_opened(&self) -> bool {
        self.bytes[0x0c] != 0
    }

    /// Working name (no outline Game.exe entry is claimed; inline only,
    /// mafia-re `game/mission/Mission.h:135-136`, `C_mission::render_number`
    /// at +0x64): reads the scene render number `C_game::Tick` copies each
    /// frame. No dedicated getter exists natively, so no RVA is claimed.
    pub fn render_number(&self) -> i32 {
        self.word(0x64) as i32
    }

    /// Working name (no outline Game.exe entry is claimed; inline only,
    /// mafia-re `game/mission/Mission.h:79-82`, `C_mission::restore_volume`
    /// at +0x7c): reports the pending volume-restore latch `Open` sets while
    /// it mutes the sound. The first `Tick` with `skip_frames` below 1
    /// clears it. No dedicated getter exists natively, so no RVA is claimed.
    pub fn is_restore_volume(&self) -> bool {
        self.bytes[0x7c] != 0
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:117`, `C_mission::unknown_0` at
    /// +0x0): reads the staged word without attaching gameplay meaning. No
    /// dedicated native getter exists, so no RVA is claimed.
    pub fn unknown_0(&self) -> u32 {
        self.word(0x00)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:118`, `C_mission::unknown_4` at
    /// +0x4): reads the staged word without attaching gameplay meaning. No
    /// dedicated native getter exists, so no RVA is claimed.
    pub fn unknown_4(&self) -> u32 {
        self.word(0x04)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:125`, `C_mission::web_path` at
    /// +0x2c): reads the staged web-path member word. No dedicated native
    /// getter exists, so no RVA is claimed.
    pub fn web_path_word(&self) -> u32 {
        self.word(0x2c)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:127`, `C_mission::roads` at +0x30):
    /// reads the staged roads member word. No dedicated native getter
    /// exists, so no RVA is claimed.
    pub fn roads_word(&self) -> u32 {
        self.word(0x30)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:142`, `C_mission::effects` at +0x6c):
    /// reads the staged effects member word. No dedicated native getter
    /// exists, so no RVA is claimed.
    pub fn effects_word(&self) -> u32 {
        self.word(0x6c)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:144-148`, `C_mission::skip_frames`
    /// at +0x78): reads the staged skip-frames count `Tick` counts down
    /// without presenting. No dedicated native getter exists, so no RVA is
    /// claimed.
    pub fn skip_frames_word(&self) -> i32 {
        self.word(0x78) as i32
    }

    /// Working name (no outline Game.exe entry is claimed; raw store,
    /// mafia-re `game/mission/Mission.h:144-148`, `C_mission::skip_frames`
    /// at +0x78): stages the skip-frames count. The `Tick` countdown stays
    /// with the mission owner. No dedicated native setter exists, so no RVA
    /// is claimed.
    pub fn set_skip_frames_word(&mut self, value: i32) {
        self.put_word(0x78, value as u32);
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:140-141`, `C_mission::next_mission_number`
    /// at +0x70): reads the staged next-mission number. The native member is
    /// written by `SetMissionNumber` and read by `KeepMissionNumber` with no
    /// dedicated raw getter, so no RVA is claimed.
    pub fn next_mission_number_word(&self) -> i32 {
        self.word(0x70) as i32
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:140-141`, `C_mission::mission_number`
    /// at +0x74): reads the staged current mission number. The native member
    /// is read by `GetMissionNumber` with no dedicated raw getter, so no RVA
    /// is claimed.
    pub fn mission_number_word(&self) -> i32 {
        self.word(0x74) as i32
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:124`, `C_mission::anim_models` at
    /// +0x14, `S_anim_model` entries): reads the staged animation-model
    /// vector word. The native size is read inline at each loop bound with
    /// no dedicated getter, so no RVA is claimed.
    pub fn anim_models_word(&self) -> u32 {
        self.word(0x14)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:129`, `C_mission::actors` at +0x34):
    /// reads the staged actor vector word. The native size is read inline
    /// at each loop bound with no dedicated getter, so no RVA is claimed.
    pub fn actors_word(&self) -> u32 {
        self.word(0x34)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:132`, `C_mission::not_me_actors` at
    /// +0x44): reads the staged not-me actor vector word. The native size
    /// is read inline with no dedicated getter, so no RVA is claimed.
    pub fn not_me_actors_word(&self) -> u32 {
        self.word(0x44)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:134`, `C_mission::programs` at
    /// +0x54): reads the staged whole-game script vector word. The native
    /// size is read inline at each loop bound with no dedicated getter, so
    /// no RVA is claimed.
    pub fn programs_word(&self) -> u32 {
        self.word(0x54)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:153`, `C_mission::t_transparent` at
    /// +0x80, `S_t_transparent` entries): reads the staged fade-object
    /// vector word. The native size is read inline with no dedicated
    /// getter, so no RVA is claimed.
    pub fn t_transparent_word(&self) -> u32 {
        self.word(0x80)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:155`, `C_mission::frame_actor_buffer`
    /// at +0xac, `S_frame_actor_buffer` 0x10-byte entries): reads the staged
    /// kept-between-missions vector word. The native size is read inline at
    /// each loop bound with no dedicated getter, so no RVA is claimed.
    pub fn frame_actor_buffer_word(&self) -> u32 {
        self.word(0xac)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:156`, `C_mission::active_actors` at
    /// +0xbc): reads the staged stay-in-game actor vector word. The native
    /// size is read inline at each loop bound with no dedicated getter, so
    /// no RVA is claimed.
    pub fn active_actors_word(&self) -> u32 {
        self.word(0xbc)
    }

    /// Working name (no outline Game.exe entry is claimed; raw word,
    /// mafia-re `game/mission/Mission.h:157`, `C_mission::actor_sound_buffer`
    /// at +0xcc, `S_actor_sound_buffer` 0xc-byte entries): reads the staged
    /// kept actor-sound vector word. The native size is read inline at each
    /// loop bound with no dedicated getter, so no RVA is claimed.
    pub fn actor_sound_buffer_word(&self) -> u32 {
        self.word(0xcc)
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:129`, `actors` at +0x34): whether an
    /// actor vector word is staged. The native size is tested inline where
    /// the actor registry consumes it, with no dedicated flag getter, so no
    /// Game.exe function entry is claimed.
    pub fn has_actors_staged(&self) -> bool {
        self.word(0x34) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:132`, `not_me_actors` at +0x44):
    /// whether a not-me actor vector word is staged. Inline native use
    /// only, so no RVA is claimed.
    pub fn has_not_me_actors_staged(&self) -> bool {
        self.word(0x44) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:134`, `programs` at +0x54): whether
    /// a whole-game script vector word is staged. Inline native use only,
    /// so no RVA is claimed.
    pub fn has_programs_staged(&self) -> bool {
        self.word(0x54) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:153`, `t_transparent` at +0x80):
    /// whether a fade-object vector word is staged. Inline native use only,
    /// so no RVA is claimed.
    pub fn has_t_transparent_staged(&self) -> bool {
        self.word(0x80) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:156`, `active_actors` at +0xbc):
    /// whether a stay-in-game actor vector word is staged. Inline native
    /// use only, so no RVA is claimed.
    pub fn has_active_actors_staged(&self) -> bool {
        self.word(0xbc) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:157`, `actor_sound_buffer` at +0xcc):
    /// whether a kept actor-sound vector word is staged. Inline native use
    /// only, so no RVA is claimed.
    pub fn has_actor_sound_buffer_staged(&self) -> bool {
        self.word(0xcc) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:124`, `anim_models` at +0x14):
    /// whether an animation-model vector word is staged. The native size is
    /// read inline at each loop bound (`Mission.cpp:163,180,193`) with no
    /// dedicated flag getter, so no RVA is claimed.
    pub fn has_anim_models_staged(&self) -> bool {
        self.word(0x14) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:155`, `frame_actor_buffer` at +0xac,
    /// `S_frame_actor_buffer` 0x10-byte entries): whether a kept-between-missions
    /// vector word is staged. Inline native use only, so no RVA is claimed.
    pub fn has_frame_actor_buffer_staged(&self) -> bool {
        self.word(0xac) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:127`, `web_path` at +0x2c): whether
    /// a web-path member word is staged. `Init` (`Mission.cpp:99-100`)
    /// branches on the null word inline with no dedicated flag getter, so no
    /// RVA is claimed.
    pub fn has_web_path_staged(&self) -> bool {
        self.word(0x2c) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:128`, `roads` at +0x30): whether a
    /// roads member word is staged. `Init` (`Mission.cpp:101-102`) branches
    /// on the null word inline, so no RVA is claimed.
    pub fn has_roads_staged(&self) -> bool {
        self.word(0x30) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:140`, `effects` at +0x6c): whether
    /// an effects member word is staged. `Init` (`Mission.cpp:103-104`)
    /// branches on the null word inline, so no RVA is claimed.
    pub fn has_effects_staged(&self) -> bool {
        self.word(0x6c) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:126`, `tshift` at +0x28): whether a
    /// tshift member word is staged. `Init` (`Mission.cpp:97-98`) branches
    /// on the null word inline, so no RVA is claimed.
    pub fn has_tshift_staged(&self) -> bool {
        self.word(0x28) != 0
    }

    /// Working name (no outline Game.exe entry is claimed; nonzero test,
    /// mafia-re `game/mission/Mission.h:125`, `game` at +0x24): whether a
    /// game member word is staged. `Init` (`Mission.cpp:107`) branches on the
    /// null word inline with no dedicated flag getter, so no RVA is claimed.
    pub fn has_game_staged(&self) -> bool {
        self.word(0x24) != 0
    }
    pub fn pending_scene(&self) -> Option<SceneKey> {
        self.pending_scene
    }
    /// Listener retained at the terminal attempt, either behind a surviving
    /// Scene reference or after backend cleanup failed. External cleanup may
    /// subsequently retire this historical identity.
    pub fn pending_listener(&self) -> Option<SceneKey> {
        self.pending_listener
    }
    pub fn pending_component(&self) -> Option<&MissionComponent> {
        self.pending_component.as_ref()
    }
    pub fn rejected_allocation(&self) -> Option<&MissionAllocation> {
        self.rejected_allocation.as_ref()
    }
    /// The boolean records absence of a successful scalar free, not proof of a
    /// live object after a failed destructor/free. Interrupted owners are inspectable.
    pub fn components(&self) -> impl Iterator<Item = (&MissionComponent, bool)> {
        self.components
            .iter()
            .map(|entry| (&entry.value, entry.disposal != ComponentDisposal::Released))
    }
    pub fn phase(&self) -> MissionPhase {
        self.phase
    }
    pub fn interrupted(&self) -> bool {
        self.interrupted
    }
    fn fail(&mut self, detail: impl std::fmt::Debug) -> MissionFailure {
        self.interrupted = true;
        let failure = MissionFailure {
            phase: self.phase,
            detail: format!("{detail:?}"),
        };
        self.first_failure.get_or_insert_with(|| failure.clone());
        failure
    }
    /// Display-formatted failure for Close paths: stores the raw `Close:`
    /// detail without Debug quotes so production `starts_with("Close:")`
    /// checks hold (the legacy `fail` keeps Debug form for old paths).
    fn fail_display(&mut self, detail: impl std::fmt::Display) -> MissionFailure {
        self.interrupted = true;
        let failure = MissionFailure {
            phase: self.phase,
            detail: detail.to_string(),
        };
        self.first_failure.get_or_insert_with(|| failure.clone());
        failure
    }
    pub fn failure(&self) -> Option<&MissionFailure> {
        self.first_failure.as_ref()
    }
    pub fn terminal_progress(&self) -> &MissionTerminalProgress {
        &self.terminal
    }
    fn available(&self) -> Result<(), MissionFailure> {
        if self.destructed
            || self.interrupted
            || self.terminal.phase != MissionTerminalPhase::NotStarted
        {
            Err(MissionFailure {
                phase: self.phase,
                detail: "mission lifetime unavailable after destruction/interruption".into(),
            })
        } else {
            Ok(())
        }
    }
    fn bind<A: MissionAllocator>(
        &mut self,
        context: &DriverSceneContext<'_>,
        allocator: &A,
        driver: &DriverSceneOwner,
    ) -> Result<(), MissionFailure> {
        let domains = LifecycleDomains {
            driver: driver.identity(),
            arena: context
                .arena
                .lookup_reference(crate::frame_transform::FrameKey(0)),
            resources: context.resources.identity(),
            textures: context.textures.objects().managed().resources().identity(),
            storage: context.storage.identity(),
            memory: allocator.mission_memory_domain(),
        };
        if self.domains.is_some_and(|bound| bound != domains) {
            return Err(MissionFailure {
                phase: self.phase,
                detail: "foreign mission resource/allocator domain".into(),
            });
        }
        self.domains = Some(domains);
        Ok(())
    }
    /// Actual Init53f230 over the existing type13 SceneDriver/resources. The
    /// outer scene member is published only when its real factory returns.
    /// Unsupported work retains the executed prefix and its owned allocations.
    #[allow(clippy::too_many_arguments)]
    pub fn init<S: DriverSceneScreen, A: MissionAllocator>(
        &mut self,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        screen: &mut S,
        images: &mut impl TextureImageSource,
        sound: NativeSceneSoundDriver,
        allocator: &mut A,
        inputs: MissionInitInputs,
        globals: &mut GameConstructorGlobals,
    ) -> Result<(), MissionFailure>
    where
        A::Error: std::fmt::Debug,
        S::Error: std::fmt::Debug,
    {
        self.init_with_scene_allocator(
            driver,
            context,
            screen,
            images,
            sound,
            allocator,
            inputs,
            globals,
            |_, _| Ok(()),
        )
    }
    /// Same Init protocol with its real driver registry allocation dependency.
    #[allow(clippy::too_many_arguments)]
    fn init_with_scene_allocator<S: DriverSceneScreen, A: MissionAllocator>(
        &mut self,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        screen: &mut S,
        images: &mut impl TextureImageSource,
        sound: NativeSceneSoundDriver,
        allocator: &mut A,
        inputs: MissionInitInputs,
        globals: &mut GameConstructorGlobals,
        scene_allocate: impl FnMut(u32, &[SceneKey]) -> Result<(), ()>,
    ) -> Result<(), MissionFailure>
    where
        A::Error: std::fmt::Debug,
        S::Error: std::fmt::Debug,
    {
        self.available()?;
        // Native returns without even reading the driver in this branch.
        if self.word(0x10) != 0 {
            return Ok(());
        }
        self.bind(context, allocator, driver)?;
        self.phase = MissionPhase::CreateScene;
        if inputs.scene_identity == 0 {
            return Err(self.fail("null observer identity cannot represent the returned Scene"));
        }
        let scene = match driver.create_scene_with_allocator(
            context,
            screen,
            images,
            sound,
            scene_allocate,
        ) {
            Ok(scene) => scene,
            Err(error) => {
                self.pending_scene = error.scene;
                return Err(self.fail(error));
            }
        };
        self.scene = Some(scene);
        self.scene_release_entered = false;
        self.put_word(0x10, inputs.scene_identity);
        for (kind, offset) in [
            (MissionComponentKind::Tshift, 0x28),
            (MissionComponentKind::WebPath, 0x2c),
            (MissionComponentKind::Roads, 0x30),
            (MissionComponentKind::Effects, 0x6c),
        ] {
            if self.word(offset) != 0 {
                continue;
            }
            self.phase = MissionPhase::Allocate(kind);
            let allocation = allocator.allocate(kind).map_err(|e| self.fail(e))?;
            let Some(allocation) = allocation else {
                self.put_word(offset, 0);
                continue;
            };
            if allocation.domain != allocator.mission_memory_domain()
                || allocation.identity == 0
                || allocation.bytes.len() != kind.allocation_size()
            {
                self.rejected_allocation = Some(allocation);
                return Err(self.fail("invalid caller allocation"));
            }
            self.phase = MissionPhase::Construct(kind);
            let mut component = if kind == MissionComponentKind::Effects {
                MissionComponent::construct_effects_prefix(allocation, inputs.effects_stack_control)
                    .expect("validated allocation")
            } else {
                MissionComponent::construct(kind, allocation)
                    .expect("validated admitted constructor")
            };
            if kind == MissionComponentKind::Effects {
                if let Err(error) = component.finish_effects_constructor(allocator) {
                    self.pending_component = Some(component);
                    return Err(self.fail(error));
                }
            }
            self.put_word(offset, component.identity());
            self.components.push(OwnedComponent {
                value: component,
                disposal: ComponentDisposal::Ready,
            });
        }
        self.put_word(0x88, self.word(0x84));
        if self.word(0x24) == 0 {
            let kind = MissionComponentKind::Game;
            self.phase = MissionPhase::Allocate(kind);
            let allocation = allocator.allocate(kind).map_err(|e| self.fail(e))?;
            let Some(allocation) = allocation else {
                self.put_word(0x24, 0);
                return Err(self.fail("original null Game write at 0x53f38f to address0x44"));
            };
            if allocation.domain != allocator.mission_memory_domain()
                || allocation.identity == 0
                || allocation.bytes.len() != kind.allocation_size()
            {
                self.rejected_allocation = Some(allocation);
                return Err(self.fail("invalid caller Game allocation"));
            }
            self.phase = MissionPhase::Construct(kind);
            let mut component = MissionComponent::construct_game(allocation, inputs.game, globals)
                .expect("validated allocation");
            self.put_word(0x24, component.identity());
            component.put(0x44, self.identity);
            self.components.push(OwnedComponent {
                value: component,
                disposal: ComponentDisposal::Ready,
            });
        }
        self.phase = MissionPhase::Complete;
        Ok(())
    }
    /// Exact UnInit order over the same common caches and genuine scene owner.
    /// Only unopened Close and constructor-empty support members are admitted.
    pub fn uninit<A>(
        &mut self,
        caches: &mut MissionCaches,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
    ) -> Result<(), MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        self.available()?;
        self.bind(context, allocator, driver)?;
        if let Some(scene) = self.scene {
            driver
                .validate_scene_domain(context.arena, context.resources, scene)
                .map_err(|e| MissionFailure {
                    phase: self.phase,
                    detail: format!("{e:?}"),
                })?;
        }
        self.phase = MissionPhase::ClearCaches;
        caches
            .clear_for_mission(context.arena, allocator)
            .map_err(|e| self.fail(e))?;
        self.phase = MissionPhase::Close;
        self.close().map_err(|e| self.fail(e))?;
        for (kind, offset) in [
            (MissionComponentKind::Tshift, 0x28),
            (MissionComponentKind::WebPath, 0x2c),
            (MissionComponentKind::Roads, 0x30),
            (MissionComponentKind::Effects, 0x6c),
            (MissionComponentKind::Game, 0x24),
        ] {
            let identity = self.word(offset);
            if identity == 0 {
                continue;
            }
            self.phase = MissionPhase::Destroy(kind);
            let Some(index) = self.components.iter().position(|entry| {
                entry.disposal == ComponentDisposal::Ready
                    && entry.value.identity() == identity
                    && entry.value.kind() == kind
            }) else {
                return Err(self.fail("component pointer has no matching owned allocation"));
            };
            self.components[index].disposal = ComponentDisposal::DestroyEntered;
            self.components[index]
                .value
                .destroy(allocator)
                .map_err(|e| self.fail(e))?;
            self.components[index].disposal = ComponentDisposal::Destroyed;
            self.phase = MissionPhase::Free(kind);
            self.components[index].disposal = ComponentDisposal::FreeEntered;
            allocator.free(identity).map_err(|e| self.fail(e))?;
            self.components[index].disposal = ComponentDisposal::Released;
            self.put_word(offset, 0);
        }
        if self.word(0x10) != 0 {
            self.phase = MissionPhase::Close;
            self.close().map_err(|e| self.fail(e))?;
            self.phase = MissionPhase::ReleaseScene;
            let scene = self
                .scene
                .ok_or_else(|| self.fail("scene pointer has no matching owned SceneKey"))?;
            self.scene_release_entered = true;
            driver
                .release_scene(context, scene)
                .map_err(|e| self.fail(e))?;
            self.scene = None;
            self.put_word(0x10, 0);
        }
        self.phase = MissionPhase::FreeUnknown68;
        if self.word(0x68) != 0 {
            return Err(self.fail("unknown68 allocation owner unavailable"));
        }
        self.phase = MissionPhase::Complete;
        Ok(())
    }
    pub fn destruct<A>(
        &mut self,
        caches: &mut MissionCaches,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
    ) -> Result<(), MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        self.uninit(caches, driver, context, allocator)?;
        for offset in [0xcc, 0xbc, 0xac, 0x80, 0x54, 0x44, 0x34, 0x14] {
            self.phase = MissionPhase::DestroyVector(offset);
            if self.word(offset + 4) != 0 {
                return Err(self.fail("populated mission vector allocation owner unavailable"));
            }
            allocator.free(0).map_err(|e| self.fail(e))?;
            for n in [4, 8, 12] {
                self.put_word(offset + n, 0);
            }
        }
        self.destructed = true;
        self.phase = MissionPhase::Complete;
        Ok(())
    }
    /// One-shot host disposal of proven-owned resources after an interrupted
    /// lifecycle. This never reenters native UnInit or changes its reached phase
    /// and first failure. Each destructor/free is entered at most once. Unknown
    /// constructors, prior failed effects and foreign/raw pointers stay retained.
    pub fn discard_terminal<A: MissionAllocator>(
        &mut self,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
    ) -> Result<(), MissionTerminalCallError>
    where
        A::Error: std::fmt::Debug,
    {
        if self.terminal.phase != MissionTerminalPhase::NotStarted {
            return Err(MissionTerminalCallError::AlreadyEntered);
        }
        if !self.interrupted {
            return Err(MissionTerminalCallError::NotInterrupted);
        }
        // Admission failures perform no cleanup and do not consume the attempt.
        self.bind(context, allocator, driver)
            .map_err(|_| MissionTerminalCallError::ForeignDomain)?;
        if let Some(scene) = self.scene {
            if !self.scene_release_entered {
                driver
                    .validate_scene_domain(context.arena, context.resources, scene)
                    .map_err(|_| MissionTerminalCallError::ForeignDomain)?;
            }
        }
        self.terminal.phase = MissionTerminalPhase::Components;
        if self.bytes[0x0c] != 0 {
            self.retain_terminal("active mission requires unavailable GameDone/Close ownership");
            self.terminal.phase = MissionTerminalPhase::RetainedOwners;
            return Err(MissionTerminalCallError::RetainedOwners);
        }
        for index in 0..self.components.len() {
            let entry = &self.components[index];
            if entry.disposal == ComponentDisposal::Released {
                continue;
            }
            if entry.disposal != ComponentDisposal::Ready {
                self.retain_terminal(format!(
                    "component {:?} {:#x} prior {:?}; native effect must not be replayed",
                    entry.value.kind(),
                    entry.value.identity(),
                    entry.disposal
                ));
                continue;
            }
            let kind = entry.value.kind();
            let identity = entry.value.identity();
            self.components[index].disposal = ComponentDisposal::DestroyEntered;
            if let Err(error) = self.components[index].value.destroy(allocator) {
                self.retain_terminal(format!(
                    "component {kind:?} destruction retained: {error:?}"
                ));
                continue;
            }
            self.components[index].disposal = ComponentDisposal::Destroyed;
            self.components[index].disposal = ComponentDisposal::FreeEntered;
            if let Err(error) = allocator.free(identity) {
                self.retain_terminal(format!(
                    "component {kind:?} scalar free has unresolved outcome: {error:?}"
                ));
                continue;
            }
            self.components[index].disposal = ComponentDisposal::Released;
            self.terminal.components_released += 1;
            let offset = component_offset(kind);
            if self.word(offset) == identity {
                self.put_word(offset, 0);
            }
        }
        if let Some(pending) = &self.pending_component {
            self.retain_terminal(format!("incomplete {:?} constructor {:#x}; pending stream/partial destruction contract unavailable",
                pending.kind(), pending.identity()));
        }
        if self.rejected_allocation.is_some() {
            self.retain_terminal("rejected allocation ownership/extent remains unresolved");
        }
        for kind in [
            MissionComponentKind::Tshift,
            MissionComponentKind::WebPath,
            MissionComponentKind::Roads,
            MissionComponentKind::Effects,
            MissionComponentKind::Game,
        ] {
            let identity = self.word(component_offset(kind));
            if identity != 0
                && !self
                    .components
                    .iter()
                    .any(|entry| entry.value.identity() == identity && entry.value.kind() == kind)
            {
                self.retain_terminal(format!("unowned {kind:?} pointer {identity:#x} retained"));
            }
        }
        self.terminal.phase = MissionTerminalPhase::Scene;
        if self.scene_release_entered && self.scene.is_some() {
            self.retain_terminal(
                "prior scene release has unresolved outcome; release must not be replayed",
            );
        } else if let Some(scene) = self.scene {
            self.scene_release_entered = true;
            match driver.release_scene(context, scene) {
                Ok(()) => {
                    self.scene = None;
                    self.put_word(0x10, 0);
                    self.terminal.scene_reference_released = true;
                }
                Err(error) => self.retain_terminal(format!("scene release retained: {error:?}")),
            }
        }
        if let Some(scene) = self.pending_scene {
            // The factory's actual pending Scene may be fully constructed (for
            // example viewport failure) or incomplete. The existing resource
            // owner decides admission; no constructor/destructor is fabricated.
            match context.resources.release_scene(
                context.arena,
                scene,
                context.textures,
                context.storage,
            ) {
                Ok(_) => {
                    self.pending_scene = None;
                    self.terminal.scene_reference_released = true;
                    if context.arena.scene(scene).is_ok() {
                        // Release may only have decremented a retained Scene.
                        // Its listener follows actual destruction, not this
                        // owner's successful release of one reference.
                        match driver
                            .host_sound_backend()
                            .map(|backend| backend.listeners())
                            .transpose()
                        {
                            Ok(Some(listeners)) if listeners.contains(&scene) => {
                                self.pending_listener = Some(scene);
                                self.retain_terminal("unregistered scene listener retained behind surviving external Scene reference");
                            }
                            Err(error) => {
                                self.pending_listener = Some(scene);
                                self.retain_terminal(format!(
                                    "surviving Scene listener ownership query failed: {error:?}"
                                ));
                            }
                            _ => {}
                        }
                    } else if let Err(error) = driver.release_unregistered_scene_listener(scene) {
                        self.pending_listener = Some(scene);
                        self.retain_terminal(format!(
                            "unregistered scene listener retained: {error:?}"
                        ));
                    }
                }
                Err(error) => self.retain_terminal(format!("pending scene retained: {error:?}")),
            }
        }
        if self.word(0x68) != 0 {
            self.retain_terminal("unknown68 owner retained");
        }
        for offset in [0xcc, 0xbc, 0xac, 0x80, 0x54, 0x44, 0x34, 0x14] {
            if self.word(offset + 4) != 0 {
                self.retain_terminal(format!("unknown mission vector at {offset:#x} retained"));
            }
        }
        if self.terminal.retained.is_empty() {
            self.terminal.phase = MissionTerminalPhase::Complete;
            Ok(())
        } else {
            self.terminal.phase = MissionTerminalPhase::RetainedOwners;
            Err(MissionTerminalCallError::RetainedOwners)
        }
    }
    fn retain_terminal(&mut self, detail: impl Into<String>) {
        self.terminal.retained.push(MissionTerminalIssue {
            phase: self.terminal.phase,
            detail: detail.into(),
        });
    }
    pub fn identity(&self) -> u32 {
        self.identity
    }
    pub fn raw_bytes(&self) -> &[u8; MISSION_BYTES] {
        &self.bytes
    }
    pub fn is_destructed(&self) -> bool {
        self.destructed
    }
    /// `C_mission::~C_mission`, GAME RVA 0x13f110, VA 0x53f110 (mafia-re
    /// `game/mission/Mission.cpp:83-87` at pin `fe7e746`): the destructor body
    /// only runs `UnInit`. At this lifecycle level the admitted `UnInit`
    /// prefix is the unopened `close` guard; member drops stay with their
    /// owners. Marks destructed on success; an opened mission keeps its
    /// `ActiveCloseUnavailable` and stays undestructed.
    pub fn destructor_runs_uninit(&mut self) -> Result<(), MissionLifecycleError> {
        if self.destructed {
            return Err(MissionLifecycleError::Destructed);
        }
        self.close()?;
        self.destructed = true;
        Ok(())
    }
    /// Explicit unsupported boundary for `C_mission::Init`, GAME RVA 0x13f230,
    /// VA 0x53f230 (mafia-re `game/mission/Mission.cpp:91-111` at pin
    /// `fe7e746`): creates the scene through the driver (`CreateFrame`), the
    /// `C_tshift`/`C_web_path`/`C_roads`/`C_effects` members and the
    /// mission-owned `C_game`. The driver scene/device owner is outside this
    /// lifecycle contract, so no state is changed and no credit beyond the
    /// boundary is claimed.
    pub fn init_unavailable(&self) -> Result<(), MissionInitError> {
        Err(MissionInitError::SceneDriverUnavailable)
    }
    fn word(&self, offset: usize) -> u32 {
        u32::from_le_bytes(self.bytes[offset..offset + 4].try_into().unwrap())
    }
    fn put_word(&mut self, offset: usize, value: u32) {
        self.bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    /// The actual original early-return guard, with no fabricated scene Close.
    /// An active mission is rejected before entering its unavailable dependencies.
    pub fn close(&mut self) -> Result<(), MissionLifecycleError> {
        if self.destructed {
            return Err(MissionLifecycleError::Destructed);
        }
        if self.terminal.phase != MissionTerminalPhase::NotStarted {
            return Err(MissionLifecycleError::TerminalDiscardEntered);
        }
        if self.bytes[0xc] != 0 {
            return Err(MissionLifecycleError::ActiveCloseUnavailable);
        }
        Ok(())
    }

    /// Shared Close-entry checks (purity, admission, null-game note, vector
    /// and mission-name owners) used by both `close_active` (retain,
    /// GAME 0x5405e0) and `uninit_active` (destroy, GAME 0x53f3b0).
    /// Reference: mafia-re `113b5b6`. Returns Ok with `phase` still unset;
    /// the caller sets its phase on success.
    fn check_close_vectors<A>(
        &mut self,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
        driver: &mut DriverSceneOwner,
    ) -> Result<(), MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        self.available().map_err(|e| MissionFailure {
            phase: self.phase,
            detail: format!(
                "Close: lifecycle unavailable: {}",
                e.detail.trim_matches('"')
            ),
        })?;
        self.bind(context, allocator, driver)
            .map_err(|e| MissionFailure {
                phase: self.phase,
                detail: format!("Close: bind unavailable: {}", e.detail.trim_matches('"')),
            })?;
        if let Some(scene) = self.scene {
            driver
                .validate_scene_domain(context.arena, context.resources, scene)
                .map_err(|e| MissionFailure {
                    phase: self.phase,
                    detail: format!(
                        "Close: scene domain unavailable: {}",
                        format!("{e:?}").trim_matches('"')
                    ),
                })?;
        }
        // Native `if (game)` tolerates a null game (Done/DoneIntern skipped,
        // clears continue); the host has no Done owner, so a zero word only
        // skips the Done boundary and the component loop below still clears.
        // Vector-owner words in native `C_mission` layout order (cleared
        // trailers at +4); names keep the layout greppable per AGENTS.md.
        for (offset, _tail) in [
            (0xcc, "sound-tail"),
            (0xbc, "anim-tail"),
            (0xac, "model-tail"),
            (0x80, "cache-tail"),
            (0x54, "program-tail"),
            (0x44, "transparent-tail"),
            (0x34, "buffer-tail"),
            (0x14, "name-tail"),
        ] {
            if self.word(offset + 4) != 0 {
                return Err(self.fail_display(format!(
                    "Close: active Close requires unavailable vector owner at {offset:#x}"
                )));
            }
        }
        if self.word(0x68) != 0 {
            return Err(
                self.fail_display("Close: active Close requires unavailable mission-name owner")
            );
        }
        Ok(())
    }

    /// Native `C_mission::Close` (GAME 0x5405e0) retain semantics: clears
    /// without destroying, for `Open` reuse. Unopened is a pure no-op.
    /// Populated vectors, Ready components, a live scene, or a set
    /// mission-name word report unavailable owners instead of a silent
    /// clear, since the host has no DelActors/program/transparent/scene
    /// owners. Allocations are retained (unlike `uninit_active` below).
    pub fn close_active<A>(
        &mut self,
        caches: &mut MissionCaches,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
        base_cache: &mut CityBaseCache,
    ) -> Result<BaseCacheCloseReport, MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        if self.bytes[0x0c] == 0 {
            return Ok(BaseCacheCloseReport::default());
        }
        self.phase = MissionPhase::Close;
        self.check_close_vectors(context, allocator, driver)?;
        let report = base_cache.close();
        caches
            .clear_for_mission(context.arena, allocator)
            .map_err(|e| self.fail_display(format!("Close: caches clear: {e:?}")))?;
        for (kind, offset) in [
            (MissionComponentKind::Tshift, 0x28),
            (MissionComponentKind::WebPath, 0x2c),
            (MissionComponentKind::Roads, 0x30),
            (MissionComponentKind::Effects, 0x6c),
            (MissionComponentKind::Game, 0x24),
        ] {
            if self.word(offset) != 0 {
                return Err(self.fail_display(format!(
                    "Close: component clear needs unavailable owner at {offset:#x}"
                )));
            }
            let _ = kind;
        }
        if self.word(0x10) != 0 {
            return Err(self.fail_display("Close: scene Close needs unavailable scene owner"));
        }
        self.bytes[0x0c] = 0;
        self.phase = MissionPhase::Complete;
        Ok(report)
    }

    /// Native `C_mission::Close` (GAME 0x5405e0) full order for reference:
    /// game `Done`,
    /// `FrmActBuff_ClearAll`, cutscene `Close`, tshift/web/roads clears,
    /// `FrmActBuff_ClearAll`, cutscene `Close`, tshift/web/roads clears,
    /// `DelActors`, anim-model clear, `base_cache.Close`, cache `Clear`,
    /// program deletes, transparent clear, scene `Close`, `opened = false`.
    /// VM, player, camera and HUD intake stays out of scope: populated actor,
    /// program, transparent, buffer and mission-name vectors report their
    /// unavailable ownership instead of a silent clear. Every failure
    /// preserves its reached prefix via `MissionFailure` with a `Close:`
    /// detail prefix and never replays. This is teardown, not gameplay.
    ///
    /// Native `UnInit` (GAME 0x53f3b0) calls `Close()` first, then deletes
    /// tshift/web/roads/effects/game, releases the scene and drops the
    /// mission name. The host follows the same two phases: the clear phase
    /// below mirrors `Close` (base-cache close, cache clear; populated
    /// vectors report unavailable owners), then the destroy phase drops
    /// host-owned components and releases the scene for re-`Init` repeats
    /// (native retains for `Open` reuse — disclosed admission).
    pub fn uninit_active<A>(
        &mut self,
        caches: &mut MissionCaches,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        allocator: &mut A,
        base_cache: &mut CityBaseCache,
    ) -> Result<BaseCacheCloseReport, MissionFailure>
    where
        A: MissionAllocator + CStrBackend<Error = <A as MissionAllocator>::Error>,
        <A as MissionAllocator>::Error: std::fmt::Debug,
    {
        // Native `if (!opened) return;` first: unopened Close is pure and
        // touches no owner domain (checked before available/bind/validate).
        if self.bytes[0x0c] == 0 {
            return Ok(BaseCacheCloseReport::default());
        }
        self.phase = MissionPhase::Close;
        self.check_close_vectors(context, allocator, driver)?;
        let report = base_cache.close();
        caches
            .clear_for_mission(context.arena, allocator)
            .map_err(|e| self.fail_display(format!("Close: caches clear: {e:?}")))?;
        for (kind, offset) in [
            (MissionComponentKind::Tshift, 0x28),
            (MissionComponentKind::WebPath, 0x2c),
            (MissionComponentKind::Roads, 0x30),
            (MissionComponentKind::Effects, 0x6c),
            (MissionComponentKind::Game, 0x24),
        ] {
            let identity = self.word(offset);
            if identity == 0 {
                continue;
            }
            self.phase = MissionPhase::Destroy(kind);
            let Some(index) = self.components.iter().position(|entry| {
                entry.disposal == ComponentDisposal::Ready
                    && entry.value.identity() == identity
                    && entry.value.kind() == kind
            }) else {
                return Err(
                    self.fail_display("Close: component pointer has no matching owned allocation")
                );
            };
            self.components[index].disposal = ComponentDisposal::DestroyEntered;
            self.components[index]
                .value
                .destroy(allocator)
                .map_err(|e| self.fail_display(format!("Close: component destroy: {e:?}")))?;
            self.components[index].disposal = ComponentDisposal::Destroyed;
            self.phase = MissionPhase::Free(kind);
            self.components[index].disposal = ComponentDisposal::FreeEntered;
            allocator
                .free(identity)
                .map_err(|e| self.fail_display(format!("Close: component free: {e:?}")))?;
            self.components[index].disposal = ComponentDisposal::Released;
            self.put_word(offset, 0);
        }
        self.bytes[0x0c] = 0;
        if self.word(0x10) != 0 {
            self.phase = MissionPhase::ReleaseScene;
            let scene = self.scene.ok_or_else(|| {
                self.fail_display("Close: scene pointer has no matching owned SceneKey")
            })?;
            self.scene_release_entered = true;
            driver
                .release_scene(context, scene)
                .map_err(|e| self.fail_display(format!("Close: scene release: {e:?}")))?;
            self.scene = None;
            self.put_word(0x10, 0);
        }
        self.phase = MissionPhase::Complete;
        Ok(report)
    }
}

/// Texture-shift element flag: the shift sets the mesh texture offsets
/// (`u_ofs`, `v_ofs`; mafia-re `game/mission/tshift.h:41`).
pub const TSHIFT_TEXTURE: u16 = 1;
/// Texture-shift element flag: the shift sets the environment-map offsets
/// (`env_u_ofs`, `env_v_ofs`; mafia-re `game/mission/tshift.h:42`).
pub const TSHIFT_ENVIRONMENT: u16 = 2;

/// Staged texture-shift element (mafia-re `game/mission/tshift.h:15-38`,
/// `S_tshift_elem`, 0x1c bytes). Native `I3D_mesh_object`/`I3D_visual`
/// pointers are staged identity words; live mesh/visual ownership stays
/// with the scene owner.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TshiftElem {
    pub mesh: u32,
    pub visual: u32,
    pub u_speed: f32,
    pub v_speed: f32,
    pub u: f32,
    pub v: f32,
    pub flags: u16,
}

impl TshiftElem {
    /// FUNCTION: GAME RVA 0x145250, VA 0x545250 (mafia-re
    /// `game/mission/tshift.h:23-35`, `S_tshift_elem::Init`): stores the
    /// mesh/visual words and the per-second speeds, zeroes the shift and
    /// stores the flags. Store order is unspecified (VC6 groups the
    /// constant stores), so only the resulting values are contracted.
    pub fn init(mesh: u32, visual: u32, u_speed: f32, v_speed: f32, flags: u16) -> Self {
        Self {
            mesh,
            visual,
            u_speed,
            v_speed,
            u: 0.0,
            v: 0.0,
            flags,
        }
    }
}

/// Owned texture-shift list (mafia-re `game/mission/tshift.h:45-62`,
/// `C_tshift`, `elems` at +0x0). Element storage stays here; live
/// mesh/visual ownership stays with the scene owner.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TshiftStore {
    pub elems: Vec<TshiftElem>,
}

impl TshiftStore {
    /// FUNCTION: GAME RVA 0x145280, VA 0x545280 (mafia-re
    /// `game/mission/tshift.h:56-61`, `C_tshift::Add`): pushes the staged
    /// element. The inlined `push_back` growth stays with the host `Vec`.
    pub fn add(&mut self, elem: TshiftElem) {
        self.elems.push(elem);
    }

    /// Working name (no outline entry is claimed): `C_mission::Close`
    /// 0x5405e0 clears the shift list (`tshift->elems.clear()`, mafia-re
    /// `game/mission/Mission.cpp:521` at pin 113b5b6). The clear is inlined
    /// with no dedicated outline, so no RVA is claimed here.
    pub fn clear(&mut self) {
        self.elems.clear();
    }
}

/// Host sink for the flag-gated mesh offset writes of
/// [`tshift_elem_tick`]. Mirrors the two `SetOffsets` call sites (texture
/// vs. environment); live mesh/visual ownership stays deferred.
pub trait TshiftOffsetsBackend {
    fn set_texture_offsets(&mut self, u: f32, v: f32);
    fn set_env_offsets(&mut self, u: f32, v: f32);
}

/// FUNCTION: GAME RVA 0x160410, VA 0x560410 (mafia-re
/// `game/mission/tshift.cpp:19-52`, `S_tshift_elem::Tick`): advances the
/// shift by `t` seconds and wraps each axis into (-1.0, 1.0] via the
/// reference's `floor`/`ceil` subtraction (`floor` past +1.0, `ceil` past
/// -1.0, untouched otherwise, NaN included). The flag-gated offset stores
/// run through the backend in native order (texture, then environment);
/// the FPU copy width of the original is a host `f32` copy here.
pub fn tshift_elem_tick(elem: &mut TshiftElem, t: f32, backend: &mut impl TshiftOffsetsBackend) {
    elem.u += t * elem.u_speed;
    if elem.u > 1.0 {
        elem.u -= elem.u.floor();
    } else if elem.u < -1.0 {
        elem.u -= elem.u.ceil();
    }
    elem.v += t * elem.v_speed;
    if elem.v > 1.0 {
        elem.v -= elem.v.floor();
    } else if elem.v < -1.0 {
        elem.v -= elem.v.ceil();
    }
    if elem.flags & TSHIFT_TEXTURE != 0 {
        backend.set_texture_offsets(elem.u, elem.v);
    }
    if elem.flags & TSHIFT_ENVIRONMENT != 0 {
        backend.set_env_offsets(elem.u, elem.v);
    }
}

/// FUNCTION: GAME RVA 0x145fd0, VA 0x545fd0 (mafia-re
/// `game/mission/Mission.h:408-412` marker, `game/mission/Mission.cpp:1844-1850`,
/// static `ClearCarSeats`): empties the car's seats from the last one via
/// `SetOwner(NULL, i, 0.0)`. The car/seat owners stay deferred, so the host
/// takes the staged seat slots and clears them back-to-front in the same
/// order; `0.0` is the native detach parameter and carries no value here.
pub fn clear_car_seats(seats: &mut [Option<u32>]) {
    for slot in seats.iter_mut().rev() {
        *slot = None;
    }
}

fn component_offset(kind: MissionComponentKind) -> usize {
    match kind {
        MissionComponentKind::Tshift => 0x28,
        MissionComponentKind::WebPath => 0x2c,
        MissionComponentKind::Roads => 0x30,
        MissionComponentKind::Effects => 0x6c,
        MissionComponentKind::Game => 0x24,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constructor_preserves_nonmember_bytes_and_original_identity_control() {
        let seed = std::array::from_fn(|n| (n as u8).wrapping_mul(13).wrapping_add(7));
        let mut mission = MissionLifecycle::construct(0x7d12_3400, seed);
        for offset in [8, 0x24, 0x2c, 0x30, 0x64, 0x6c, 0x70, 0x74, 0x78, 0x98] {
            assert_eq!(
                &mission.raw_bytes()[offset..offset + 4],
                &seed[offset..offset + 4]
            );
        }
        for offset in [0x14, 0x34, 0x44, 0x54, 0x80, 0xac, 0xbc, 0xcc] {
            assert_eq!(mission.raw_bytes()[offset], 0x7d);
            assert_eq!(
                &mission.raw_bytes()[offset + 1..offset + 4],
                &seed[offset + 1..offset + 4]
            );
        }
        assert_eq!(mission.word(0x90), 1f32.to_bits());
        let before = *mission.raw_bytes();
        mission.close().unwrap();
        assert_eq!(*mission.raw_bytes(), before);
    }
    #[test]
    fn get_tshift_reads_member_word_as_identity() {
        // FUNCTION: GAME 0x547120, mafia-re `game/mission/Mission.h:177-182`:
        // the trivial `tshift` member (mission +0x28) read. The constructor
        // stores a null member, so the default read is zero.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.get_tshift(), 0);
        mission.put_word(0x28, 0x1234_5678);
        assert_eq!(mission.get_tshift(), 0x1234_5678 as i32);
    }
    #[test]
    fn get_tshift_round_trips_negative_bit_patterns() {
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        mission.put_word(0x28, (-1i32) as u32);
        assert_eq!(mission.get_tshift(), -1);
        assert_eq!(mission.get_tshift() as u32, mission.word(0x28));
    }
    #[test]
    fn mission_get_game_reads_staged_member_word() {
        // Working name, mafia-re `game/mission/Mission.h:228-233`: the game
        // member (mission +0x24) read. The constructor stores a null member,
        // so the default read is zero.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.get_game(), 0);
        mission.put_word(0x24, 0x1234_5678);
        assert_eq!(mission.get_game(), 0x1234_5678 as i32);
    }
    #[test]
    fn mission_get_scene_reads_staged_member_word() {
        // Working name, mafia-re `game/mission/Mission.h:278-281`: the scene
        // member (mission +0x10) read. Unlike `scene()`, which resolves the
        // owned key, this reads the raw staged identity word.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.get_scene(), 0);
        mission.put_word(0x10, (-42i32) as u32);
        assert_eq!(mission.get_scene(), -42);
    }
    #[test]
    fn mission_get_options_reads_staged_words() {
        // Working name, mafia-re `game/mission/Mission.h:268-271` with the
        // `S_mission_options` layout `Mission.h:23-31`: the seven staged
        // option words map onto the option state in member order.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        mission.put_word(0x90, 1f32.to_bits());
        mission.put_word(0x94, 0.5f32.to_bits());
        mission.put_word(0x98, 250.0f32.to_bits());
        mission.put_word(0x9c, 2);
        mission.put_word(0xa0, (-1i32) as u32);
        mission.put_word(0xa4, 6);
        mission.put_word(0xa8, 0);
        let options = mission.get_options();
        assert_eq!(options.level_of_detail, 1.0);
        assert_eq!(options.draw_distance, 0.5);
        assert_eq!(options.far_range, 250.0);
        assert_eq!(options.game_effects, 2);
        assert_eq!(options.game_sounds, -1);
        assert_eq!(options.shadows, 6);
        assert_eq!(options.particles, 0);
    }
    struct Heap {
        next: u32,
        frees: Vec<u32>,
        present: bool,
        allocations: usize,
        domain: crate::mission_components::MissionMemoryDomainId,
    }
    impl crate::mission_components::MissionMemoryDomain for Heap {
        fn mission_memory_domain(&self) -> crate::mission_components::MissionMemoryDomainId {
            self.domain
        }
    }
    impl MissionAllocator for Heap {
        type Error = ();
        fn allocate(
            &mut self,
            kind: MissionComponentKind,
        ) -> Result<Option<MissionAllocation>, ()> {
            self.allocations += 1;
            let identity = self.next;
            self.next += 0x4000;
            Ok(Some(MissionAllocation {
                domain: self.domain,
                identity,
                bytes: vec![0xa5; kind.allocation_size()],
            }))
        }
        fn free(&mut self, identity: u32) -> Result<(), ()> {
            self.frees.push(identity);
            Ok(())
        }
        fn open_effects_table(&mut self) -> Result<bool, ()> {
            Ok(self.present)
        }
    }
    impl CStrBackend for Heap {
        type Error = ();
        fn allocate_string(
            &mut self,
            _: u32,
        ) -> Result<Option<crate::c_str::StringAllocation>, ()> {
            panic!("empty caches allocate no strings")
        }
        fn free_string(&mut self, _: crate::c_str::StringAllocation) -> Result<(), ()> {
            panic!("empty caches release no strings")
        }
    }
    struct Screen;
    impl crate::scene_resources::SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> Result<i32, ()> {
            Ok(800)
        }
        fn screen_sy(&mut self) -> Result<i32, ()> {
            Ok(600)
        }
    }
    impl DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            Ok([10, 20, 810, 620])
        }
    }
    struct MissingImages;
    impl TextureImageSource for MissingImages {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, crate::itexture::TextureImageError> {
            Err(crate::itexture::TextureImageError::Missing)
        }
    }
    #[test]
    fn lifecycle_owns_real_scene_and_support_objects_until_last_release() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = crate::model_loading::ModelArena::default();
            let mut resources = crate::scene_resources::SceneResources::default();
            let mut textures = crate::itexture::TextureResources::new().unwrap();
            let mut storage = crate::itexture::CpuTextureStorage::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let mut driver = DriverSceneOwner::default();
            let mut caches = MissionCaches::empty().unwrap();
            let mut heap = Heap {
                next: 0x6d00_0000,
                frees: Vec::new(),
                present: false,
                allocations: 0,
                domain: crate::mission_components::MissionMemoryDomainId::fresh(),
            };
            let mut globals = GameConstructorGlobals {
                word_6d4b1c: 0x12345678,
                word_6bdb10: 9,
            };
            let inputs = MissionInitInputs {
                scene_identity: 0x6e00_0000,
                effects_stack_control: 0x39,
                game: GameConstructorInputs {
                    vector_control: 0x48,
                    embedded_4c_control: 0x57,
                },
            };
            let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
            mission
                .init(
                    &mut driver,
                    &mut context,
                    &mut Screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                    &mut heap,
                    inputs,
                    &mut globals,
                )
                .unwrap();
            let scene = mission.scene().unwrap();
            assert_eq!(driver.scenes(), &[scene]);
            assert_eq!(heap.allocations, 5);
            assert_eq!(mission.components().count(), 5);
            assert_eq!(globals.word_6bdb10, 0);
            let game = mission
                .components()
                .find(|(component, _)| component.kind() == MissionComponentKind::Game)
                .unwrap()
                .0;
            assert_eq!(game.word(0x44), mission.identity());
            assert_eq!(game.word(0x5c), 0x12345678);
            // Original entry guard returns before any factory/allocation work.
            mission
                .init(
                    &mut driver,
                    &mut context,
                    &mut Screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                    &mut heap,
                    inputs,
                    &mut globals,
                )
                .unwrap();
            assert_eq!(heap.allocations, 5);
            let original_domain = heap.domain;
            let before_frees = heap.frees.len();
            heap.domain = crate::mission_components::MissionMemoryDomainId::fresh();
            assert!(mission
                .uninit(&mut caches, &mut driver, &mut context, &mut heap)
                .is_err());
            assert_eq!(heap.frees.len(), before_frees);
            assert_eq!(driver.scenes(), &[scene]);
            heap.domain = original_domain;
            let mut foreign_driver = DriverSceneOwner::default();
            assert!(mission
                .uninit(&mut caches, &mut foreign_driver, &mut context, &mut heap)
                .is_err());
            assert_eq!(heap.frees.len(), before_frees);
            mission
                .destruct(&mut caches, &mut driver, &mut context, &mut heap)
                .unwrap();
            assert!(driver.scenes().is_empty());
            assert!(mission.is_destructed());
            assert!(mission.components().all(|(_, alive)| !alive));
            assert_eq!(
                heap.frees
                    .iter()
                    .copied()
                    .filter(|&x| x != 0)
                    .collect::<Vec<_>>(),
                vec![
                    0x6d00_0000,
                    0x6d00_4000,
                    0x6d00_8000,
                    0x6d00_c000,
                    0x6d01_0000
                ]
            );
            assert!(context.arena.scene(scene).is_err());
        });
    }
    #[test]
    fn present_effects_table_retains_constructor_prefix_without_publishing_it() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = crate::model_loading::ModelArena::default();
            let mut resources = crate::scene_resources::SceneResources::default();
            let mut textures = crate::itexture::TextureResources::new().unwrap();
            let mut storage = crate::itexture::CpuTextureStorage::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            let mut driver = DriverSceneOwner::default();
            let mut heap = Heap {
                next: 0x6d00_0000,
                frees: Vec::new(),
                present: true,
                allocations: 0,
                domain: crate::mission_components::MissionMemoryDomainId::fresh(),
            };
            let mut globals = GameConstructorGlobals {
                word_6d4b1c: 0,
                word_6bdb10: 9,
            };
            let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
            let error = mission
                .init(
                    &mut driver,
                    &mut context,
                    &mut Screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                    &mut heap,
                    MissionInitInputs {
                        scene_identity: 0x6e00_0000,
                        effects_stack_control: 0x39,
                        game: GameConstructorInputs {
                            vector_control: 0x48,
                            embedded_4c_control: 0x57,
                        },
                    },
                    &mut globals,
                )
                .unwrap_err();
            assert_eq!(
                error.phase,
                MissionPhase::Construct(MissionComponentKind::Effects)
            );
            assert_eq!(heap.allocations, 4);
            assert_eq!(mission.word(0x6c), 0);
            assert_eq!(mission.pending_component().unwrap().raw_bytes()[0x30], 0xa5);
            assert_eq!(mission.components().count(), 3);
            assert!(mission.scene().is_some());
            assert_eq!(globals.word_6bdb10, 9);
            assert!(heap.frees.is_empty());
        });
    }
    struct FaultHeap {
        domain: crate::mission_components::MissionMemoryDomainId,
        next: u32,
        live: std::collections::BTreeSet<u32>,
        free_calls: Vec<u32>,
        fail_allocate: Option<MissionComponentKind>,
        invalid_allocate: Option<MissionComponentKind>,
        fail_free_at: Option<usize>,
        effects_present: bool,
        fail_effects_open: bool,
        fail_effects_read: bool,
        effects_bytes: Option<Vec<u8>>,
    }
    impl Default for FaultHeap {
        fn default() -> Self {
            Self {
                domain: crate::mission_components::MissionMemoryDomainId::fresh(),
                next: 0x6d00_0000,
                live: Default::default(),
                free_calls: Vec::new(),
                fail_allocate: None,
                invalid_allocate: None,
                fail_free_at: None,
                effects_present: false,
                fail_effects_open: false,
                fail_effects_read: false,
                effects_bytes: None,
            }
        }
    }
    impl crate::mission_components::MissionMemoryDomain for FaultHeap {
        fn mission_memory_domain(&self) -> crate::mission_components::MissionMemoryDomainId {
            self.domain
        }
    }
    impl MissionAllocator for FaultHeap {
        type Error = &'static str;
        fn allocate(
            &mut self,
            kind: MissionComponentKind,
        ) -> Result<Option<MissionAllocation>, Self::Error> {
            if self.fail_allocate == Some(kind) {
                return Err("injected component allocation");
            }
            let identity = self.next;
            self.next += 0x4000;
            self.live.insert(identity);
            Ok(Some(MissionAllocation {
                domain: if self.invalid_allocate == Some(kind) {
                    crate::mission_components::MissionMemoryDomainId::fresh()
                } else {
                    self.domain
                },
                identity,
                bytes: vec![0xa5; kind.allocation_size()],
            }))
        }
        fn free(&mut self, identity: u32) -> Result<(), Self::Error> {
            self.free_calls.push(identity);
            if identity != 0 {
                assert!(
                    self.live.remove(&identity),
                    "allocation must not be freed twice"
                );
            }
            // Failure deliberately occurs AFTER the backing was freed. A caller
            // must retain the uncertain observation instead of repeating it.
            if self.fail_free_at == Some(self.free_calls.len()) {
                return Err("injected ambiguous scalar free");
            }
            Ok(())
        }
        fn open_effects_table(&mut self) -> Result<bool, Self::Error> {
            if self.fail_effects_open {
                Err("injected effects open")
            } else {
                Ok(self.effects_present)
            }
        }
        fn read_effects_table(&mut self) -> Result<Option<Vec<u8>>, Self::Error> {
            if self.fail_effects_read {
                Err("injected effects read")
            } else {
                Ok(self.effects_bytes.take())
            }
        }
    }
    impl CStrBackend for FaultHeap {
        type Error = &'static str;
        fn allocate_string(
            &mut self,
            _: u32,
        ) -> Result<Option<crate::c_str::StringAllocation>, Self::Error> {
            panic!("empty caches allocate no strings")
        }
        fn free_string(&mut self, _: crate::c_str::StringAllocation) -> Result<(), Self::Error> {
            panic!("empty caches release no strings")
        }
    }
    #[derive(Default)]
    struct FaultScreen {
        fail_size: bool,
        fail_viewport: bool,
    }
    impl crate::scene_resources::SceneScreen for FaultScreen {
        type Error = &'static str;
        fn screen_sx(&mut self) -> Result<i32, Self::Error> {
            if self.fail_size {
                Err("injected constructor screen")
            } else {
                Ok(800)
            }
        }
        fn screen_sy(&mut self) -> Result<i32, Self::Error> {
            if self.fail_size {
                Err("injected constructor screen")
            } else {
                Ok(600)
            }
        }
    }
    impl DriverSceneScreen for FaultScreen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], Self::Error> {
            if self.fail_viewport {
                Err("injected viewport")
            } else {
                Ok([0, 0, 800, 600])
            }
        }
    }
    fn with_fault_fixture(
        run: impl FnOnce(
            &mut MissionLifecycle,
            &mut DriverSceneOwner,
            &mut DriverSceneContext<'_>,
            &mut FaultHeap,
        ),
    ) {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = crate::model_loading::ModelArena::default();
            let mut resources = crate::scene_resources::SceneResources::default();
            let mut textures = crate::itexture::TextureResources::new().unwrap();
            let mut storage = crate::itexture::CpuTextureStorage::default();
            let mut context = DriverSceneContext {
                arena: &mut arena,
                resources: &mut resources,
                textures: &mut textures,
                storage: &mut storage,
            };
            run(
                &mut MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]),
                &mut DriverSceneOwner::default(),
                &mut context,
                &mut FaultHeap::default(),
            );
        });
    }
    fn fault_init(
        mission: &mut MissionLifecycle,
        driver: &mut DriverSceneOwner,
        context: &mut DriverSceneContext<'_>,
        heap: &mut FaultHeap,
        screen: &mut FaultScreen,
    ) -> Result<(), MissionFailure> {
        mission.init(
            driver,
            context,
            screen,
            &mut MissingImages,
            NativeSceneSoundDriver::Absent,
            heap,
            MissionInitInputs {
                scene_identity: 0x6e00_0000,
                effects_stack_control: 0x39,
                game: GameConstructorInputs {
                    vector_control: 0x48,
                    embedded_4c_control: 0x57,
                },
            },
            &mut GameConstructorGlobals {
                word_6d4b1c: 0,
                word_6bdb10: 9,
            },
        )
    }
    #[test]
    fn terminal_disposal_releases_every_proven_init_allocation_prefix_once() {
        for (count, kind) in [
            MissionComponentKind::Tshift,
            MissionComponentKind::WebPath,
            MissionComponentKind::Roads,
            MissionComponentKind::Effects,
            MissionComponentKind::Game,
        ]
        .into_iter()
        .enumerate()
        {
            with_fault_fixture(|mission, driver, context, heap| {
                heap.fail_allocate = Some(kind);
                let failure =
                    fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                        .unwrap_err();
                assert_eq!(failure.phase, MissionPhase::Allocate(kind));
                assert_eq!(mission.failure(), Some(&failure));
                assert_eq!(heap.live.len(), count);
                let scene = mission.scene().unwrap();
                mission.discard_terminal(driver, context, heap).unwrap();
                assert_eq!(
                    mission.terminal_progress().phase,
                    MissionTerminalPhase::Complete
                );
                assert_eq!(mission.terminal_progress().components_released, count);
                assert_eq!(mission.phase(), failure.phase);
                assert_eq!(mission.failure(), Some(&failure));
                assert!(!mission.is_destructed()); // no original destructor credit
                assert_eq!(
                    mission.close(),
                    Err(MissionLifecycleError::TerminalDiscardEntered)
                );
                assert!(heap.live.is_empty());
                assert!(driver.scenes().is_empty());
                assert!(context.arena.scene(scene).is_err());
                let frees = heap.free_calls.clone();
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::AlreadyEntered)
                );
                assert_eq!(heap.free_calls, frees);
            });
        }
    }
    #[test]
    fn terminal_scene_factory_cleanup_obeys_completed_versus_incomplete_owner_contract() {
        for incomplete in [false, true] {
            with_fault_fixture(|mission, driver, context, heap| {
                let mut screen = FaultScreen {
                    fail_size: incomplete,
                    fail_viewport: !incomplete,
                };
                let failure = fault_init(mission, driver, context, heap, &mut screen).unwrap_err();
                let scene = mission.pending_scene().unwrap();
                assert_eq!(failure.phase, MissionPhase::CreateScene);
                let result = mission.discard_terminal(driver, context, heap);
                if incomplete {
                    assert_eq!(result, Err(MissionTerminalCallError::RetainedOwners));
                    assert_eq!(mission.pending_scene(), Some(scene));
                    assert!(mission
                        .terminal_progress()
                        .retained
                        .iter()
                        .any(|issue| issue.detail.contains("pending scene retained")));
                    assert!(context.arena.scene(scene).is_ok());
                } else {
                    result.unwrap();
                    assert_eq!(mission.pending_scene(), None);
                    assert!(context.arena.scene(scene).is_err());
                }
                assert_eq!(mission.failure(), Some(&failure));
                assert_eq!(mission.phase(), failure.phase);
                assert!(heap.free_calls.is_empty());
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::AlreadyEntered)
                );
            });
        }
    }
    #[test]
    fn incomplete_effects_are_retained_while_independent_prefix_and_scene_are_released() {
        for phase in 0..4 {
            with_fault_fixture(|mission, driver, context, heap| {
                heap.effects_present = true;
                heap.fail_effects_open = phase == 0;
                heap.fail_effects_read = phase == 1;
                if phase == 3 {
                    heap.effects_bytes = Some(vec![1, 2, 3]);
                }
                let failure =
                    fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                        .unwrap_err();
                assert_eq!(
                    failure.phase,
                    MissionPhase::Construct(MissionComponentKind::Effects)
                );
                let pending = mission.pending_component().unwrap().identity();
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::RetainedOwners)
                );
                assert_eq!(heap.live.iter().copied().collect::<Vec<_>>(), [pending]);
                assert!(!heap.free_calls.contains(&pending));
                assert_eq!(mission.pending_component().unwrap().identity(), pending);
                assert_eq!(mission.terminal_progress().components_released, 3);
                assert!(mission.terminal_progress().scene_reference_released);
                assert!(driver.scenes().is_empty());
                assert_eq!(mission.failure(), Some(&failure));
            });
        }
    }
    #[test]
    fn terminal_disposal_never_replays_prior_uninit_destroy_or_ambiguous_free() {
        for failed_free in [false, true] {
            with_fault_fixture(|mission, driver, context, heap| {
                fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
                let first = mission.components().next().unwrap().0.identity();
                heap.fail_free_at = Some(heap.free_calls.len() + if failed_free { 2 } else { 1 });
                let failure = mission
                    .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                    .unwrap_err();
                assert_eq!(
                    failure.phase,
                    if failed_free {
                        MissionPhase::Free(MissionComponentKind::Tshift)
                    } else {
                        MissionPhase::Destroy(MissionComponentKind::Tshift)
                    }
                );
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::RetainedOwners)
                );
                assert_eq!(
                    heap.free_calls
                        .iter()
                        .filter(|&&identity| identity == first)
                        .count(),
                    usize::from(failed_free)
                );
                assert_eq!(mission.terminal_progress().components_released, 4);
                assert!(mission.terminal_progress().scene_reference_released);
                assert!(mission
                    .terminal_progress()
                    .retained
                    .iter()
                    .any(|issue| issue.detail.contains("must not be replayed")));
                assert_eq!(mission.failure(), Some(&failure));
                assert_eq!(mission.phase(), failure.phase);
                let frees = heap.free_calls.clone();
                assert_eq!(
                    mission.discard_terminal(driver, context, heap),
                    Err(MissionTerminalCallError::AlreadyEntered)
                );
                assert_eq!(heap.free_calls, frees);
                assert!(mission
                    .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                    .is_err());
                assert_eq!(heap.free_calls, frees);
            });
        }
    }
    #[test]
    fn foreign_terminal_domains_are_rejected_before_effects_without_consuming_the_attempt() {
        with_fault_fixture(|mission, driver, context, heap| {
            heap.fail_allocate = Some(MissionComponentKind::Roads);
            let failure = fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                .unwrap_err();
            let before = heap.free_calls.clone();
            let old_domain = heap.domain;
            heap.domain = crate::mission_components::MissionMemoryDomainId::fresh();
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::ForeignDomain)
            );
            heap.domain = old_domain;
            assert_eq!(
                mission.discard_terminal(&mut DriverSceneOwner::default(), context, heap),
                Err(MissionTerminalCallError::ForeignDomain)
            );
            assert_eq!(
                mission.terminal_progress().phase,
                MissionTerminalPhase::NotStarted
            );
            assert_eq!(heap.free_calls, before);
            assert_eq!(mission.failure(), Some(&failure));
            mission.discard_terminal(driver, context, heap).unwrap();
            assert!(heap.live.is_empty());
        });
    }
    #[test]
    fn rejected_allocations_and_active_close_remain_explicit_unresolved_owners() {
        with_fault_fixture(|mission, driver, context, heap| {
            heap.invalid_allocate = Some(MissionComponentKind::Roads);
            let failure = fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                .unwrap_err();
            let rejected = mission.rejected_allocation.as_ref().unwrap().identity;
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert!(!heap.free_calls.contains(&rejected));
            assert_eq!(heap.live.iter().copied().collect::<Vec<_>>(), [rejected]);
            assert_eq!(mission.failure(), Some(&failure));
        });
        with_fault_fixture(|mission, driver, context, heap| {
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::NotInterrupted)
            );
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            mission.bytes[0x0c] = 1;
            let failure = mission
                .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                .unwrap_err();
            let before = heap.free_calls.clone();
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert_eq!(heap.free_calls, before);
            assert_eq!(heap.live.len(), 5);
            assert_eq!(mission.failure(), Some(&failure));
        });
    }
    #[test]
    fn terminal_own_effect_failures_retain_only_the_uncertain_component_and_keep_original_error() {
        let destructor_calls = [1, 1, 2, 3];
        let preceding_calls = [0, 2, 4, 7];
        for index in 0..4 {
            for scalar_free in [false, true] {
                with_fault_fixture(|mission, driver, context, heap| {
                    heap.fail_allocate = Some(MissionComponentKind::Game);
                    let initial =
                        fault_init(mission, driver, context, heap, &mut FaultScreen::default())
                            .unwrap_err();
                    let identity = mission.components().nth(index).unwrap().0.identity();
                    heap.fail_free_at = Some(
                        heap.free_calls.len()
                            + preceding_calls[index]
                            + if scalar_free {
                                destructor_calls[index] + 1
                            } else {
                                1
                            },
                    );
                    assert_eq!(
                        mission.discard_terminal(driver, context, heap),
                        Err(MissionTerminalCallError::RetainedOwners)
                    );
                    assert_eq!(mission.terminal_progress().components_released, 3);
                    assert!(mission.terminal_progress().scene_reference_released);
                    assert_eq!(mission.failure(), Some(&initial));
                    assert_eq!(mission.phase(), initial.phase);
                    assert_eq!(
                        heap.free_calls
                            .iter()
                            .filter(|&&value| value == identity)
                            .count(),
                        usize::from(scalar_free)
                    );
                    assert_eq!(heap.live.contains(&identity), !scalar_free);
                    assert_eq!(mission.components().filter(|(_, alive)| *alive).count(), 1);
                    let frees = heap.free_calls.clone();
                    assert_eq!(
                        mission.discard_terminal(driver, context, heap),
                        Err(MissionTerminalCallError::AlreadyEntered)
                    );
                    assert_eq!(heap.free_calls, frees);
                });
            }
        }
    }
    #[test]
    fn terminal_never_retries_a_preexisting_scene_release_failure_even_after_dependency_changes() {
        with_fault_fixture(|mission, driver, context, heap| {
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            let scene = mission.scene().unwrap();
            let nested = driver
                .create_scene(
                    context,
                    &mut FaultScreen::default(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            context.arena.scene_add_frame(scene, nested.root()).unwrap();
            let initial = mission
                .uninit(&mut MissionCaches::empty().unwrap(), driver, context, heap)
                .unwrap_err();
            assert_eq!(initial.phase, MissionPhase::ReleaseScene);
            assert!(initial
                .detail
                .contains("foreign derived scene-frame resources"));
            assert!(heap.live.is_empty());
            // A changing dependency must not turn host terminal disposal into a
            // replay of a partially entered native scene destructor.
            context
                .arena
                .scene_delete_frame(scene, nested.root())
                .unwrap();
            let scenes = driver.scenes().to_vec();
            let frees = heap.free_calls.clone();
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert_eq!(mission.scene(), Some(scene));
            assert_eq!(driver.scenes(), scenes);
            assert_eq!(heap.free_calls, frees);
            assert!(!mission.terminal_progress().scene_reference_released);
            assert!(mission
                .terminal_progress()
                .retained
                .iter()
                .any(|issue| issue.detail.contains("prior scene release")));
            assert_eq!(mission.failure(), Some(&initial));
        });
    }
    #[test]
    fn terminal_retained_unregistered_scene_keeps_listener_until_last_alias_destruction() {
        with_fault_fixture(|mission, driver, context, heap| {
            let audio = crate::runtime_audio::RuntimeAudio::default();
            driver
                .bind_host_sound_backend(context.arena, &audio)
                .unwrap();
            let failure = mission
                .init_with_scene_allocator(
                    driver,
                    context,
                    &mut FaultScreen::default(),
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                    heap,
                    MissionInitInputs {
                        scene_identity: 0x6e00_0000,
                        effects_stack_control: 0x39,
                        game: GameConstructorInputs {
                            vector_control: 0x48,
                            embedded_4c_control: 0x57,
                        },
                    },
                    &mut GameConstructorGlobals {
                        word_6d4b1c: 0,
                        word_6bdb10: 9,
                    },
                    |capacity, scenes| {
                        assert_eq!(capacity, 32);
                        assert!(scenes.is_empty());
                        Err(())
                    },
                )
                .unwrap_err();
            assert_eq!(failure.phase, MissionPhase::CreateScene);
            assert!(failure.detail.contains("Registration"));
            assert!(driver.allocation_incomplete());
            assert!(driver.scenes().is_empty());
            let scene = mission.pending_scene().unwrap();
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            context.arena.retain(scene.root()).unwrap();
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 2);
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::RetainedOwners)
            );
            assert!(mission.terminal_progress().scene_reference_released);
            assert_eq!(mission.pending_scene(), None);
            assert_eq!(mission.pending_listener(), Some(scene));
            assert!(mission
                .terminal_progress()
                .retained
                .iter()
                .any(|issue| issue.detail.contains("surviving external Scene reference")));
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 1);
            assert!(context.arena.scene(scene).is_ok());
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            assert_eq!(mission.failure(), Some(&failure));
            assert_eq!(mission.phase(), failure.phase);
            assert!(heap.free_calls.is_empty());
            assert_eq!(
                mission.discard_terminal(driver, context, heap),
                Err(MissionTerminalCallError::AlreadyEntered)
            );
            assert_eq!(context.arena.frame(scene.root()).unwrap().references, 1);
            // The external owner releases the final reference through the real
            // unregistered Scene owner, then performs its listener cleanup.
            context
                .resources
                .release_scene(context.arena, scene, context.textures, context.storage)
                .unwrap();
            assert!(context.arena.scene(scene).is_err());
            assert_eq!(
                driver.host_sound_backend().unwrap().listeners().unwrap(),
                [scene]
            );
            driver.release_unregistered_scene_listener(scene).unwrap();
            assert!(driver
                .host_sound_backend()
                .unwrap()
                .listeners()
                .unwrap()
                .is_empty());
            assert_eq!(mission.failure(), Some(&failure));
        });
    }
    #[test]
    fn uninit_disposes_to_zero_and_restart_repeats() {
        with_fault_fixture(|mission, driver, context, heap| {
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            mission.bytes[0x0c] = 1;
            assert_eq!(
                mission.close(),
                Err(MissionLifecycleError::ActiveCloseUnavailable)
            );
            let mut caches = MissionCaches::empty().unwrap();
            let mut base = crate::city_base_cache::CityBaseCache::new(0);
            let report = mission
                .uninit_active(&mut caches, driver, context, heap, &mut base)
                .unwrap();
            assert_eq!(
                report,
                crate::city_base_cache::BaseCacheCloseReport::default()
            );
            assert_eq!(mission.bytes[0x0c], 0);
            assert!(mission.scene().is_none());
            assert!(mission.components().all(|(_, alive)| !alive));
            assert!(driver.scenes().is_empty());
            assert!(heap.live.is_empty());
            assert_eq!(mission.phase(), MissionPhase::Complete);
            assert!(mission.failure().is_none());
            assert_eq!(mission.close(), Ok(()));
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            assert!(mission.scene().is_some());
            assert_eq!(mission.components().filter(|(_, alive)| *alive).count(), 5);
            mission.bytes[0x0c] = 1;
            let mut base = crate::city_base_cache::CityBaseCache::new(0);
            mission
                .uninit_active(&mut caches, driver, context, heap, &mut base)
                .unwrap();
            assert!(mission.scene().is_none());
            assert!(mission.components().all(|(_, alive)| !alive));
            assert!(driver.scenes().is_empty());
            assert!(heap.live.is_empty());
        });
    }
    #[test]
    fn close_unopened_is_noop_without_touching_owners() {
        with_fault_fixture(|mission, driver, context, heap| {
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            let mut caches = MissionCaches::empty().unwrap();
            let mut base = crate::city_base_cache::CityBaseCache::new(0);
            let frees = heap.free_calls.clone();
            let report = mission
                .close_active(&mut caches, driver, context, heap, &mut base)
                .unwrap();
            assert_eq!(
                report,
                crate::city_base_cache::BaseCacheCloseReport::default()
            );
            assert!(mission.scene().is_some());
            assert_eq!(heap.free_calls, frees);
            assert_eq!(mission.bytes[0x0c], 0);
        });
    }
    #[test]
    fn uninit_requires_unavailable_vector_owner_with_prefix() {
        with_fault_fixture(|mission, driver, context, heap| {
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            mission.bytes[0x0c] = 1;
            mission.put_word(0x34 + 4, 1);
            let mut caches = MissionCaches::empty().unwrap();
            let mut base = crate::city_base_cache::CityBaseCache::new(0);
            let failure = mission
                .uninit_active(&mut caches, driver, context, heap, &mut base)
                .unwrap_err();
            assert_eq!(failure.phase, MissionPhase::Close);
            // Close paths store the raw detail (fail_display, no Debug quotes).
            assert!(failure.detail.starts_with("Close:"), "{}", failure.detail);
            assert!(failure.detail.contains("0x34"));
            assert_eq!(mission.bytes[0x0c], 1);
            assert!(mission.scene().is_some());
            assert_eq!(mission.failure(), Some(&failure));
        });
    }
    #[test]
    fn close_retains_empty_mission_for_open_reuse() {
        // Retain semantics (GAME 0x5405e0): an opened-but-empty mission
        // clears caches and flips opened=false while keeping every
        // allocation for Open reuse (no destroy, no free).
        with_fault_fixture(|mission, driver, context, heap| {
            mission.bytes[0x0c] = 1;
            let mut caches = MissionCaches::empty().unwrap();
            let mut base = crate::city_base_cache::CityBaseCache::new(0);
            let frees = heap.free_calls.clone();
            mission
                .close_active(&mut caches, driver, context, heap, &mut base)
                .unwrap();
            assert_eq!(mission.bytes[0x0c], 0);
            assert_eq!(mission.phase(), MissionPhase::Complete);
            assert_eq!(heap.free_calls, frees);
            assert!(mission.failure().is_none());
        });
    }
    #[test]
    fn close_reports_unavailable_component_clear() {
        // Populated components cannot clear without owners: Err, retained.
        with_fault_fixture(|mission, driver, context, heap| {
            fault_init(mission, driver, context, heap, &mut FaultScreen::default()).unwrap();
            mission.bytes[0x0c] = 1;
            let mut caches = MissionCaches::empty().unwrap();
            let mut base = crate::city_base_cache::CityBaseCache::new(0);
            let failure = mission
                .close_active(&mut caches, driver, context, heap, &mut base)
                .unwrap_err();
            assert!(failure.detail.starts_with("Close:"), "{}", failure.detail);
            assert_eq!(mission.bytes[0x0c], 1);
            assert_eq!(mission.components().filter(|(_, alive)| *alive).count(), 5);
        });
    }
    #[test]
    fn tshift_elem_init_rva_145250_stores_speeds_and_zeroes_shift() {
        // S_tshift_elem::Init GAME RVA 0x145250 (VA 0x545250).
        let elem = TshiftElem::init(0x11, 0x22, 0.5, -1.25, TSHIFT_TEXTURE | TSHIFT_ENVIRONMENT);
        assert_eq!(elem.mesh, 0x11);
        assert_eq!(elem.visual, 0x22);
        assert_eq!(elem.u_speed, 0.5);
        assert_eq!(elem.v_speed, -1.25);
        assert_eq!(elem.u, 0.0);
        assert_eq!(elem.v, 0.0);
        assert_eq!(elem.flags, TSHIFT_TEXTURE | TSHIFT_ENVIRONMENT);
    }
    #[test]
    fn tshift_add_rva_145280_pushes_elements_in_order() {
        // C_tshift::Add GAME RVA 0x145280 (VA 0x545280).
        let mut store = TshiftStore::default();
        let first = TshiftElem::init(1, 2, 0.0, 0.0, TSHIFT_TEXTURE);
        let second = TshiftElem::init(3, 4, 1.0, 1.0, TSHIFT_ENVIRONMENT);
        store.add(first);
        store.add(second);
        assert_eq!(store.elems, vec![first, second]);
    }
    struct OffsetLog {
        texture: Vec<[f32; 2]>,
        env: Vec<[f32; 2]>,
    }
    impl TshiftOffsetsBackend for OffsetLog {
        fn set_texture_offsets(&mut self, u: f32, v: f32) {
            self.texture.push([u, v]);
        }
        fn set_env_offsets(&mut self, u: f32, v: f32) {
            self.env.push([u, v]);
        }
    }
    #[test]
    fn tshift_elem_tick_rva_160410_wraps_and_gates_by_flags() {
        // S_tshift_elem::Tick GAME RVA 0x160410 (VA 0x560410). Exact binary
        // fractions keep the f32 wrap assertions bit-exact.
        let mut backend = OffsetLog {
            texture: vec![],
            env: vec![],
        };
        let mut elem = TshiftElem::init(1, 2, 0.75, -0.75, TSHIFT_TEXTURE | TSHIFT_ENVIRONMENT);
        tshift_elem_tick(&mut elem, 2.0, &mut backend);
        assert_eq!(elem.u, 0.5);
        assert_eq!(elem.v, -0.5);
        assert_eq!(backend.texture, vec![[0.5, -0.5]]);
        assert_eq!(backend.env, vec![[0.5, -0.5]]);
        // Texture-only: env sink stays quiet; in-range shift passes through.
        let mut backend = OffsetLog {
            texture: vec![],
            env: vec![],
        };
        let mut elem = TshiftElem::init(1, 2, 0.25, 0.25, TSHIFT_TEXTURE);
        tshift_elem_tick(&mut elem, 1.0, &mut backend);
        assert_eq!((elem.u, elem.v), (0.25, 0.25));
        assert_eq!(backend.texture.len(), 1);
        assert!(backend.env.is_empty());
        // No flags: no sink writes, shift still advances.
        let mut backend = OffsetLog {
            texture: vec![],
            env: vec![],
        };
        let mut elem = TshiftElem::init(1, 2, 0.5, 0.5, 0);
        tshift_elem_tick(&mut elem, 1.0, &mut backend);
        assert_eq!((elem.u, elem.v), (0.5, 0.5));
        assert!(backend.texture.is_empty() && backend.env.is_empty());
    }
    #[test]
    fn clear_car_seats_rva_145fd0_clears_back_to_front() {
        // Static ClearCarSeats GAME RVA 0x145fd0 (VA 0x545fd0).
        let mut seats = vec![Some(11u32), Some(22), Some(33)];
        clear_car_seats(&mut seats);
        assert_eq!(seats, vec![None, None, None]);
        let mut empty: Vec<Option<u32>> = vec![];
        clear_car_seats(&mut empty);
        assert!(empty.is_empty());
    }
    #[test]
    fn reset_mission_numbers_working_name_clears_both_staged_words() {
        // Inline C_mission::ResetMissionNumbers (Mission.h:290-294); no
        // outline RVA is claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        mission.put_word(0x70, 7);
        mission.put_word(0x74, 9);
        mission.reset_mission_numbers();
        assert_eq!(mission.word(0x70), 0);
        assert_eq!(mission.word(0x74), 0);
    }
    #[test]
    fn mission_name_word_working_name_reads_staged_pointer_word() {
        // C_mission::GetMissionName (Mission.h:272-277); the only outline
        // copy is folded with C_actor::GetFrame (0x47afb0), so no separate
        // mission RVA is claimed here.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.mission_name_word(), 0);
        mission.put_word(0x68, 0x1234_5678);
        assert_eq!(mission.mission_name_word(), 0x1234_5678);
    }
    #[test]
    fn last_actor_number_working_name_reads_staged_word() {
        // Inline C_mission::last_actor_number read (Mission.h:117-120,
        // +0x8); no outline RVA is claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.last_actor_number(), 0);
        mission.put_word(0x08, 41);
        assert_eq!(mission.last_actor_number(), 41);
        mission.put_word(0x08, (-7i32) as u32);
        assert_eq!(mission.last_actor_number(), -7);
    }
    #[test]
    fn is_opened_working_name_reads_flag_byte() {
        // Inline C_mission::opened read (Mission.h:120-122, +0xc); no
        // outline RVA is claimed. The constructor leaves it cleared.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert!(!mission.is_opened());
        mission.bytes[0x0c] = 1;
        assert!(mission.is_opened());
        mission.bytes[0x0c] = 0;
        assert!(!mission.is_opened());
    }
    #[test]
    fn render_number_working_name_reads_staged_word() {
        // Inline C_mission::render_number read (Mission.h:135-136, +0x64);
        // no outline RVA is claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.render_number(), 0);
        mission.put_word(0x64, 1234);
        assert_eq!(mission.render_number(), 1234);
    }
    #[test]
    fn is_restore_volume_working_name_reads_flag_byte() {
        // Inline C_mission::restore_volume read (Mission.h:79-82, +0x7c);
        // no outline RVA is claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert!(!mission.is_restore_volume());
        mission.bytes[0x7c] = 1;
        assert!(mission.is_restore_volume());
        mission.bytes[0x7c] = 0;
        assert!(!mission.is_restore_volume());
    }
    #[test]
    fn unknown_words_working_name_read_staged_words() {
        // Raw C_mission::unknown_0 (+0x0) and unknown_4 (+0x4) reads
        // (Mission.h:117-118); no meaning attached and no RVA is claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.unknown_0(), 0);
        assert_eq!(mission.unknown_4(), 0);
        mission.put_word(0x00, 0x1111_2222);
        mission.put_word(0x04, 0x3333_4444);
        assert_eq!(mission.unknown_0(), 0x1111_2222);
        assert_eq!(mission.unknown_4(), 0x3333_4444);
    }

    #[test]
    fn tshift_store_clear_working_name_empties_elements() {
        // Working name: C_mission::Close 0x5405e0 clears tshift->elems
        // (Mission.cpp:521); inlined, no outline RVA is claimed.
        let mut store = TshiftStore::default();
        assert!(store.elems.is_empty());
        store.add(TshiftElem::init(1, 2, 0.5, 0.25, TSHIFT_TEXTURE));
        store.add(TshiftElem::init(3, 4, 0.25, 0.5, TSHIFT_ENVIRONMENT));
        assert_eq!(store.elems.len(), 2);
        store.clear();
        assert!(store.elems.is_empty());
        store.clear();
        assert!(store.elems.is_empty());
    }

    #[test]
    fn staged_member_words_working_name_read_raw_handles() {
        // Working names: C_mission web_path (+0x2c), roads (+0x30) and
        // effects (+0x6c) member words (Mission.h:125-142); inline-only
        // reads with no outline RVA claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.web_path_word(), 0);
        assert_eq!(mission.roads_word(), 0);
        assert_eq!(mission.effects_word(), 0);
        mission.put_word(0x2c, 0x1111_1111);
        mission.put_word(0x30, 0x2222_2222);
        mission.put_word(0x6c, 0x3333_3333);
        assert_eq!(mission.web_path_word(), 0x1111_1111);
        assert_eq!(mission.roads_word(), 0x2222_2222);
        assert_eq!(mission.effects_word(), 0x3333_3333);
    }

    #[test]
    fn skip_frames_word_working_name_round_trips_staged_count() {
        // Working names: C_mission::skip_frames (+0x78) raw read/store
        // (Mission.h:144-148); inline-only with no outline RVA claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.skip_frames_word(), 0);
        mission.set_skip_frames_word(10);
        assert_eq!(mission.skip_frames_word(), 10);
        mission.set_skip_frames_word(-1);
        assert_eq!(mission.skip_frames_word(), -1);
        assert_eq!(mission.skip_frames_word() as u32, mission.word(0x78));
    }

    #[test]
    fn mission_number_words_working_name_read_staged_pair() {
        // Working names: C_mission next_mission_number (+0x70) and
        // mission_number (+0x74) raw reads (Mission.h:140-141); inline-only
        // with no outline RVA claimed.
        let mut mission = MissionLifecycle::construct(0x6c00_0000, [0; MISSION_BYTES]);
        assert_eq!(mission.next_mission_number_word(), 0);
        assert_eq!(mission.mission_number_word(), 0);
        mission.put_word(0x70, 7);
        mission.put_word(0x74, 9u32);
        assert_eq!(mission.next_mission_number_word(), 7);
        assert_eq!(mission.mission_number_word(), 9);
        mission.reset_mission_numbers();
        assert_eq!(mission.next_mission_number_word(), 0);
        assert_eq!(mission.mission_number_word(), 0);
    }

    #[test]
    fn mission_list_words_working_name_read_staged_vectors() {
        // Working names: C_mission actors (+0x34), not_me_actors (+0x44),
        // programs (+0x54) and t_transparent (+0x80) vector words
        // (Mission.h:129,132,134,153); inline-only reads with no outline
        // RVA claimed.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert_eq!(mission.actors_word(), 0);
        assert_eq!(mission.not_me_actors_word(), 0);
        assert_eq!(mission.programs_word(), 0);
        assert_eq!(mission.t_transparent_word(), 0);
        mission.put_word(0x34, 0x1111_1111);
        mission.put_word(0x44, 0x2222_2222);
        mission.put_word(0x54, 0x3333_3333);
        mission.put_word(0x80, 0x4444_4444);
        assert_eq!(mission.actors_word(), 0x1111_1111);
        assert_eq!(mission.not_me_actors_word(), 0x2222_2222);
        assert_eq!(mission.programs_word(), 0x3333_3333);
        assert_eq!(mission.t_transparent_word(), 0x4444_4444);
        assert_eq!(mission.actors_word(), mission.word(0x34));
        assert_eq!(mission.programs_word(), mission.word(0x54));
    }

    #[test]
    fn kept_buffer_words_working_name_read_staged_vectors() {
        // Working names: C_mission anim_models (+0x14), frame_actor_buffer
        // (+0xac), active_actors (+0xbc) and actor_sound_buffer (+0xcc)
        // vector words (Mission.h:124,155-157); inline-only reads with no
        // outline RVA claimed.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert_eq!(mission.anim_models_word(), 0);
        assert_eq!(mission.frame_actor_buffer_word(), 0);
        assert_eq!(mission.active_actors_word(), 0);
        assert_eq!(mission.actor_sound_buffer_word(), 0);
        mission.put_word(0x14, 0x5555_5555);
        mission.put_word(0xac, 0x6666_6666);
        mission.put_word(0xbc, 0x7777_7777);
        mission.put_word(0xcc, 0x8888_8888);
        assert_eq!(mission.anim_models_word(), 0x5555_5555);
        assert_eq!(mission.frame_actor_buffer_word(), 0x6666_6666);
        assert_eq!(mission.active_actors_word(), 0x7777_7777);
        assert_eq!(mission.actor_sound_buffer_word(), 0x8888_8888);
    }

    #[test]
    fn staged_list_presence_working_name_tracks_nonzero_words() {
        // Working names: nonzero tests of the staged actor (+0x34), not-me
        // (+0x44), program (+0x54), fade (+0x80), stay-in-game (+0xbc) and
        // actor-sound (+0xcc) vector words; inline-only with no outline RVA
        // claimed.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_actors_staged());
        assert!(!mission.has_not_me_actors_staged());
        assert!(!mission.has_programs_staged());
        assert!(!mission.has_t_transparent_staged());
        assert!(!mission.has_active_actors_staged());
        assert!(!mission.has_actor_sound_buffer_staged());
        mission.put_word(0x34, 1);
        mission.put_word(0x44, 2);
        mission.put_word(0x54, 3);
        mission.put_word(0x80, 4);
        mission.put_word(0xbc, 5);
        mission.put_word(0xcc, 6);
        assert!(mission.has_actors_staged());
        assert!(mission.has_not_me_actors_staged());
        assert!(mission.has_programs_staged());
        assert!(mission.has_t_transparent_staged());
        assert!(mission.has_active_actors_staged());
        assert!(mission.has_actor_sound_buffer_staged());
        mission.put_word(0x34, 0);
        mission.put_word(0xcc, 0);
        assert!(!mission.has_actors_staged());
        assert!(mission.has_programs_staged());
        assert!(!mission.has_actor_sound_buffer_staged());
    }

    #[test]
    fn has_anim_models_staged_working_name_tracks_vector_word() {
        // Working name: anim_models (+0x14); inline size test, no outline RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_anim_models_staged());
        mission.put_word(0x14, 0x1111_1111);
        assert!(mission.has_anim_models_staged());
        mission.put_word(0x14, 0);
        assert!(!mission.has_anim_models_staged());
    }

    #[test]
    fn has_frame_actor_buffer_staged_working_name_tracks_vector_word() {
        // Working name: frame_actor_buffer (+0xac); inline size test, no RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_frame_actor_buffer_staged());
        mission.put_word(0xac, 0x2222_2222);
        assert!(mission.has_frame_actor_buffer_staged());
        mission.put_word(0xac, 0);
        assert!(!mission.has_frame_actor_buffer_staged());
    }

    #[test]
    fn has_web_path_staged_working_name_tracks_member_word() {
        // Working name: web_path (+0x2c); Init null-branch, no outline RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_web_path_staged());
        mission.put_word(0x2c, 0x3333_3333);
        assert!(mission.has_web_path_staged());
        mission.put_word(0x2c, 0);
        assert!(!mission.has_web_path_staged());
    }

    #[test]
    fn has_roads_staged_working_name_tracks_member_word() {
        // Working name: roads (+0x30); Init null-branch, no outline RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_roads_staged());
        mission.put_word(0x30, 0x4444_4444);
        assert!(mission.has_roads_staged());
        mission.put_word(0x30, 0);
        assert!(!mission.has_roads_staged());
    }

    #[test]
    fn has_effects_staged_working_name_tracks_member_word() {
        // Working name: effects (+0x6c); Init null-branch, no outline RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_effects_staged());
        mission.put_word(0x6c, 0x5555_5555);
        assert!(mission.has_effects_staged());
        mission.put_word(0x6c, 0);
        assert!(!mission.has_effects_staged());
    }

    #[test]
    fn has_tshift_staged_working_name_tracks_member_word() {
        // Working name: tshift (+0x28); Init null-branch, no outline RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_tshift_staged());
        mission.put_word(0x28, 0x6666_6666);
        assert!(mission.has_tshift_staged());
        mission.put_word(0x28, 0);
        assert!(!mission.has_tshift_staged());
    }

    #[test]
    fn has_game_staged_working_name_tracks_member_word() {
        // Working name: game (+0x24); Init null-branch, no outline RVA.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.has_game_staged());
        mission.put_word(0x24, 0x7777_7777);
        assert!(mission.has_game_staged());
        mission.put_word(0x24, 0);
        assert!(!mission.has_game_staged());
    }

    #[test]
    fn rva_13f110_destructor_runs_unopened_close_and_marks_destructed() {
        // C_mission::~C_mission GAME RVA 0x13f110 (VA 0x53f110).
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert!(!mission.is_destructed());
        mission.destructor_runs_uninit().unwrap();
        assert!(mission.is_destructed());
        assert_eq!(
            mission.destructor_runs_uninit(),
            Err(MissionLifecycleError::Destructed)
        );
    }

    #[test]
    fn rva_13f110_destructor_opened_mission_keeps_active_guard() {
        // C_mission::~C_mission GAME RVA 0x13f110 (VA 0x53f110): UnInit's Close
        // step refuses an opened mission before touching members.
        let mut mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        mission.bytes[0x0c] = 1;
        assert_eq!(
            mission.destructor_runs_uninit(),
            Err(MissionLifecycleError::ActiveCloseUnavailable)
        );
        assert!(!mission.is_destructed());
    }

    #[test]
    fn rva_13f230_init_reports_scene_driver_unavailable() {
        // C_mission::Init GAME RVA 0x13f230 (VA 0x53f230): explicit
        // unsupported boundary, no state change.
        let mission = MissionLifecycle::construct(0, [0; MISSION_BYTES]);
        assert_eq!(
            mission.init_unavailable(),
            Err(MissionInitError::SceneDriverUnavailable)
        );
        assert!(!mission.is_destructed());
    }
}
