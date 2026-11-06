//! Game.exe FINDACTOR / FINDFRAME dispatcher arms. Ordered source collections
//! borrow existing world objects. No mission world, resource, actor or frame is
//! constructed here, and no lookup miss schedules deferred name resolution.
use crate::frame_lookup::{self, FrameLookupBackend, FrameLookupError, SceneRoots};
use crate::script_scheduler::{ActorReferenceId, ControlState, FrameReferenceId};
use std::collections::HashMap;
pub const MAX_LOOKUP_NAME: usize = 255;
pub const MAX_ACTOR_SOURCES: usize = 65_536;

/// Identity lease retained by the actual actor owner, with a new lease required
/// for every replacement/recreation. Dropping script slots never drops this lease.
#[derive(Debug)]
pub struct ActorLifetime {
    reference: ActorReferenceId,
}
impl ActorLifetime {
    pub fn for_existing(identity: u32) -> Self {
        Self {
            reference: ActorReferenceId::scoped(identity, frame_lookup::incarnation()),
        }
    }
    pub fn reference(&self) -> ActorReferenceId {
        self.reference
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LookupError {
    UnsupportedWorldBackend,
    UnsupportedDiagnosticLocale,
    MissingScene,
    NegativeIndex(i32),
    UnsupportedName,
    StaleActor(ActorReferenceId),
    NullPrimaryActor,
    MissingGlobalFrame(ActorReferenceId),
    DuplicateIdentity,
    SourceLimit,
    Allocation,
    Frame(FrameLookupError),
    Backend(String),
}
impl From<FrameLookupError> for LookupError {
    fn from(error: FrameLookupError) -> Self {
        Self::Frame(error)
    }
}
/// Actual MessageBeep(-1) dependency. Implementations must report an unavailable
/// diagnostic effect; there is intentionally no silent default implementation.
pub trait LookupDiagnosticBackend {
    fn message_beep(&mut self, argument: i32) -> Result<(), LookupError>;
}
/// Accessors describe a serialized live world. Reads are stable until the
/// explicit diagnostic callback; that callback may change owner or collections,
/// and subsequent lookup reads observe those changes. Reentrant read callbacks
/// or concurrent mutation require a separate native-equivalence admission.
pub trait LookupWorldBackend {
    /// Explicit admission for Game.exe's CRT comparator. This locale state is
    /// independent of the LS3DF CRT state used by FrameLookupBackend.
    fn diagnostic_default_c_locale(&self) -> bool;
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError>;
    fn actor_frame(&self, actor: ActorReferenceId)
        -> Result<Option<FrameReferenceId>, LookupError>;
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError>;
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError>;
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError>;
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError>;
    fn scene(&self) -> Result<SceneRoots, LookupError>;
    fn message_beep(&mut self, argument: i32) -> Result<(), LookupError>;
    /// Live C_entity Process scratch for the executing owner. The default
    /// refuses; the mission world serves the owner's actual entity state.
    fn load_entity_process_state(
        &mut self,
        owner: Option<ActorReferenceId>,
    ) -> Result<crate::entity_actor::EntityProcessState, LookupError> {
        let _ = owner;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Round-trip after each entity command. The default refuses.
    fn store_entity_process_state(
        &mut self,
        owner: Option<ActorReferenceId>,
        state: &crate::entity_actor::EntityProcessState,
    ) -> Result<(), LookupError> {
        let _ = owner;
        let _ = state;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_actor::SetActState word store for the resolved actor. The
    /// default refuses; the mission world serves the live actor.
    fn set_actor_act_state(
        &mut self,
        actor: ActorReferenceId,
        state: u32,
    ) -> Result<(), LookupError> {
        let _ = actor;
        let _ = state;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_program::SetEvent for a target actor's program (EVENT
    /// 73/SETEVENT 74). Returns true if the caller should advance (no event or
    /// event started), false if the caller should jump to its label (already
    /// inside an event on the target). The default refuses.
    fn set_program_event(
        &mut self,
        actor: ActorReferenceId,
        event_id: i32,
    ) -> Result<bool, LookupError> {
        let _ = actor;
        let _ = event_id;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original CMD_HUMAN_ADDWEAPON inventory push for a resolved human. The
    /// default refuses; the mission world serves the live actor.
    fn human_add_weapon(
        &mut self,
        actor: ActorReferenceId,
        id: u16,
        loaded: i32,
        spare: i32,
    ) -> Result<(), LookupError> {
        let _ = actor;
        let _ = id;
        let _ = loaded;
        let _ = spare;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original CMD_FRM_SETON frame switch (mafia-re 113b5b6
    /// game/mission/C_program.cpp:3134) for a resolved frame. The world
    /// applies the core arena on/off store and reports the device side
    /// effects it deferred. The default refuses; a world with arena ownership
    /// serves the live frame.
    fn set_frame_on(
        &mut self,
        frame: FrameReferenceId,
        on: bool,
    ) -> Result<FrameSetOnOutcome, LookupError> {
        let _ = frame;
        let _ = on;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_detector::GetSignal read (mafia-re 113b5b6
    /// game/actors/C_detector.h:60-64, GAME 0x47b750) for a resolved
    /// detector, serving the C_program DETECTOR_ISSIGNAL case (mafia-re
    /// game/mission/C_program.cpp:3351-3365). Some carries the live +0xdc
    /// signal bit; None is the native type gate (resolved actor exists but
    /// is not a detector), where the caller advances without jumping. The
    /// default refuses; the mission world serves the live actor.
    fn detector_signal(&self, actor: ActorReferenceId) -> Result<Option<bool>, LookupError> {
        let _ = actor;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_detector::SetSignal store (mafia-re 113b5b6
    /// game/actors/C_detector.h:65-71, GAME 0x47b760) for a resolved
    /// detector, serving the C_program DETECTOR_SETSIGNAL case (mafia-re
    /// game/mission/C_program.cpp:3341-3350). A resolved non-detector is
    /// the native silent type gate and reports success without storing,
    /// like the human_add_weapon NotHuman gate in the mission world. The
    /// default refuses; the mission world serves the live actor.
    fn set_detector_signal(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<(), LookupError> {
        let _ = actor;
        let _ = on;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_actor::GetDead read (mafia-re 113b5b6
    /// game/actors/C_actor.h:359, GAME 0x47afc0) for GETACTORSDIST. The
    /// default refuses; the mission world serves the live actor.
    fn actor_dead(&self, actor: ActorReferenceId) -> Result<bool, LookupError> {
        let _ = actor;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original frame world position (C_actor::GetFrame()->GetWorldPos,
    /// S_vector::Magnitude for GETACTORSDIST, mafia-re 113b5b6
    /// game/mission/C_program.cpp:3631) for a resolved frame. The default
    /// refuses; a world with arena ownership serves the live frame.
    fn frame_world_position(
        &mut self,
        frame: FrameReferenceId,
    ) -> Result<[f32; 3], LookupError> {
        let _ = frame;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_human::GetCar read (mafia-re 113b5b6
    /// game/actors/C_human.h:1120, GAME 0x47b7b0) for HUMAN_GETOWNER
    /// (C_program.cpp:4325). None-outer is the native PLAYER/ENTITY type
    /// gate (caller advances without dest write); Some carries the live car
    /// latch (None for no car, caller writes null). The seat/action_state
    /// nulling gate stays DEFERRED. The default refuses.
    fn human_car(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<Option<ActorReferenceId>>, LookupError> {
        let _ = actor;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_Vehicle::GetSpeed read (mafia-re 113b5b6
    /// game/vehicles/C_Vehicle.h:266, GAME 0x47b0f0) for CAR_GETSPEED
    /// (C_program.cpp:4994). None is the native CAR type gate (caller leaves
    /// the float bank untouched); Some carries m/s. The default refuses.
    fn car_speed(&self, actor: ActorReferenceId) -> Result<Option<f32>, LookupError> {
        let _ = actor;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_car::IsCarUsable read (mafia-re 113b5b6
    /// game/vehicles/C_car.h:160, GAME 0x429850 STUB false;
    /// C_program.cpp:4954) for ISCARUSABLE. None is the native CAR type gate;
    /// Some carries the predicate (always false until the stub is replaced).
    /// Partial: the full predicate stays DEFERRED. The default refuses.
    fn is_car_usable(&self, actor: ActorReferenceId) -> Result<Option<bool>, LookupError> {
        let _ = actor;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_car::SetBreakMotor store (mafia-re 113b5b6
    /// game/vehicles/C_car.h:193, GAME 0x47b3e0; C_program.cpp:5002) for
    /// CAR_BREAKMOTOR. False is the native CAR type gate (no store); true
    /// after storing. The default refuses.
    fn set_car_break_motor(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<bool, LookupError> {
        let _ = actor;
        let _ = on;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original CAR_FORCESTOP tail (mafia-re 113b5b6
    /// game/mission/C_program.cpp:5803: `Deactivate(0)` + `Reset(0.0f,
    /// true)`) for CAR_FORCESTOP. False is the native CAR type gate (no
    /// store); true after latching. Physics/AI effects stay DEFERRED. The
    /// default refuses.
    fn force_stop_car(&mut self, actor: ActorReferenceId) -> Result<bool, LookupError> {
        let _ = actor;
        Err(LookupError::UnsupportedWorldBackend)
    }
    /// Original C_player::LockControls store (mafia-re 113b5b6
    /// game/actors/C_player.h:60, GAME 0x595f00; C_program.cpp:4172) for
    /// PLAYER_LOCKCONTROLS. No player advances without effect; Do_Breath
    /// stays DEFERRED. The default refuses.
    fn set_player_controls_locked(&mut self, locked: bool) -> Result<(), LookupError> {
        let _ = locked;
        Err(LookupError::UnsupportedWorldBackend)
    }
}
pub struct LiveActor<'a> {
    pub lifetime: &'a ActorLifetime,
    pub frame: Option<FrameReferenceId>,
}
/// Core CMD_FRM_SETON switch report. The device side effects below stay
/// deferred to game/device owners, like other deferred device effects: sound
/// frames take the device switch instead of the arena store, the chase-camera
/// guard needs the player car and camera mode, placement/projector updates
/// need the game scene, and shadow add/delete plus the car tick need the actor
/// owners. Each flag is an explicit unavailable marker for its effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameSetOnOutcome {
    /// The arena on/off store ran (false for sound frames, whose device
    /// switch stays deferred and runs no arena store, matching the native
    /// break).
    pub core_applied: bool,
    /// Sound, chase-camera, placement, projector and shadow side effects stay
    /// deferred; true while any applies to the switched frame.
    pub device_effects_deferred: bool,
}
/// Live read lease, valid only while all object owners and collection storage are
/// borrowed. Rebuilding it after deletion excludes removed actors/frames; old
/// slot identities then fail resolution, even when raw identities are reused.
pub struct LiveLookupWorld<'a> {
    actors: &'a [LiveActor<'a>],
    identities: HashMap<ActorReferenceId, usize>,
    pub owner: Option<ActorReferenceId>,
    pub primary: &'a [Option<ActorReferenceId>],
    pub players: [Option<ActorReferenceId>; 2],
    pub secondary: &'a [Option<ActorReferenceId>],
    pub frames: &'a dyn FrameLookupBackend,
    pub scene: Option<SceneRoots>,
    pub diagnostic: &'a mut dyn LookupDiagnosticBackend,
    /// Defaults to false until the caller admits Game.exe's CRT locale state.
    pub diagnostic_default_c_locale: bool,
}
impl<'a> LiveLookupWorld<'a> {
    pub fn borrow(
        actors: &'a [LiveActor<'a>],
        frames: &'a dyn FrameLookupBackend,
        diagnostic: &'a mut dyn LookupDiagnosticBackend,
    ) -> Result<Self, LookupError> {
        if actors.len() > MAX_ACTOR_SOURCES {
            return Err(LookupError::SourceLimit);
        }
        let mut identities = HashMap::new();
        identities
            .try_reserve(actors.len())
            .map_err(|_| LookupError::Allocation)?;
        for (index, actor) in actors.iter().enumerate() {
            if identities
                .insert(actor.lifetime.reference(), index)
                .is_some()
            {
                return Err(LookupError::DuplicateIdentity);
            }
        }
        Ok(Self {
            actors,
            identities,
            owner: None,
            primary: &[],
            players: [None; 2],
            secondary: &[],
            frames,
            scene: None,
            diagnostic,
            diagnostic_default_c_locale: false,
        })
    }
}
impl LookupWorldBackend for LiveLookupWorld<'_> {
    fn diagnostic_default_c_locale(&self) -> bool {
        self.diagnostic_default_c_locale
    }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        Ok(self.owner)
    }
    fn actor_frame(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<FrameReferenceId>, LookupError> {
        let index = self
            .identities
            .get(&actor)
            .ok_or(LookupError::StaleActor(actor))?;
        Ok(self.actors[*index].frame)
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.primary)
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
        Ok(self.players)
    }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Ok(self.secondary)
    }
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> {
        Ok(self.frames)
    }
    fn scene(&self) -> Result<SceneRoots, LookupError> {
        self.scene.ok_or(LookupError::MissingScene)
    }
    fn message_beep(&mut self, argument: i32) -> Result<(), LookupError> {
        self.diagnostic.message_beep(argument)
    }
}
fn valid_name(name: &[u8]) -> Result<(), LookupError> {
    if name.len() > MAX_LOOKUP_NAME || name.contains(&0) {
        return Err(LookupError::UnsupportedName);
    }
    Ok(())
}
fn actor_matches(
    world: &dyn LookupWorldBackend,
    actor: ActorReferenceId,
    name: &[u8],
    require_frame: bool,
) -> Result<bool, LookupError> {
    let Some(frame) = world.actor_frame(actor)? else {
        return if require_frame {
            Err(LookupError::MissingGlobalFrame(actor))
        } else {
            Ok(false)
        };
    };
    let stored = world.frames()?.frame(frame)?.name.unwrap_or(b"");
    if stored.contains(&0) {
        return Err(LookupError::UnsupportedName);
    }
    Ok(stored == name)
}
/// Borrow the caller's live destination bank, not a copied ControlState.
/// Counts and backing must be checked separately when a write is reached.
pub trait LookupRegisters {
    fn actor_register_count(&self) -> usize;
    fn frame_register_count(&self) -> usize;
    fn write_actor_register(
        &mut self,
        index: u32,
        value: Option<ActorReferenceId>,
    ) -> Result<(), LookupError>;
    fn write_frame_register(
        &mut self,
        index: u32,
        value: Option<FrameReferenceId>,
    ) -> Result<(), LookupError>;
}
impl LookupRegisters for ControlState {
    fn actor_register_count(&self) -> usize {
        self.actor_references().len()
    }
    fn frame_register_count(&self) -> usize {
        self.frame_references().len()
    }
    fn write_actor_register(
        &mut self,
        index: u32,
        value: Option<ActorReferenceId>,
    ) -> Result<(), LookupError> {
        self.set_actor_reference(index, value)
            .map_err(|_| LookupError::Backend("actor register backing unavailable".into()))
    }
    fn write_frame_register(
        &mut self,
        index: u32,
        value: Option<FrameReferenceId>,
    ) -> Result<(), LookupError> {
        self.set_frame_reference(index, value)
            .map_err(|_| LookupError::Backend("frame register backing unavailable".into()))
    }
}

/// The caller can check the original diagnostic before reading the instruction.
/// An eventual owned-program callback may change the program counter and owner.
pub fn actor_lookup_needs_beep(world: &dyn LookupWorldBackend) -> Result<bool, LookupError> {
    if let Some(owner) = world.owner()? {
        if let Some(frame) = world.actor_frame(owner)? {
            let frames = world.frames()?;
            let owner_name = frames.frame(frame)?.name.unwrap_or(b"");
            if !world.diagnostic_default_c_locale() {
                return Err(LookupError::UnsupportedDiagnosticLocale);
            }
            if !owner_name.is_ascii() || owner_name.contains(&0) {
                return Err(FrameLookupError::UnsupportedName.into());
            }
            if owner_name.eq_ignore_ascii_case(b"chcipnuti chlapka") {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Legacy entry retains its original diagnostic and live-world semantics.
pub fn find_actor(
    control: &mut ControlState,
    world: &mut dyn LookupWorldBackend,
    index: i32,
    name: &[u8],
) -> Result<(), LookupError> {
    if actor_lookup_needs_beep(world)? {
        world.message_beep(-1)?;
    }
    find_actor_after_diagnostic(control, world, index, name)
}

/// Common search suffix. Called after the FINDACTOR diagnostic, using live
/// instruction/owner/slot reads; this operation never runs that callback twice.
pub fn find_actor_after_diagnostic(
    control: &mut impl LookupRegisters,
    world: &dyn LookupWorldBackend,
    index: i32,
    name: &[u8],
) -> Result<(), LookupError> {
    if index < 0 {
        return Err(LookupError::NegativeIndex(index));
    }
    let index = index as u32;
    if index as usize >= control.actor_register_count() {
        return Ok(());
    }
    valid_name(name)?;
    if name.is_empty() {
        let owner = world.owner()?;
        control.write_actor_register(index, owner)?;
        return Ok(());
    }
    let primary = world.primary()?;
    if primary.len() > MAX_ACTOR_SOURCES {
        return Err(LookupError::SourceLimit);
    }
    for actor in primary.iter().rev() {
        let actor = actor.ok_or(LookupError::NullPrimaryActor)?;
        if actor_matches(world, actor, name, false)? {
            control.write_actor_register(index, Some(actor))?;
            return Ok(());
        }
    }
    for actor in world.players()?.into_iter().flatten() {
        if actor_matches(world, actor, name, true)? {
            control.write_actor_register(index, Some(actor))?;
            break;
        }
    }
    let secondary = world.secondary()?;
    if secondary.len() > MAX_ACTOR_SOURCES {
        return Err(LookupError::SourceLimit);
    }
    for actor in secondary.iter().rev().flatten() {
        if actor_matches(world, *actor, name, false)? {
            control.write_actor_register(index, Some(*actor))?;
            return Ok(());
        }
    }
    Ok(())
}
pub fn find_frame(
    control: &mut ControlState,
    world: &mut dyn LookupWorldBackend,
    index: i32,
    name: &[u8],
) -> Result<(), LookupError> {
    find_frame_into(control, world, index, name)
}

/// Search over the same actual frame register bank used by the interpreter.
pub fn find_frame_into(
    control: &mut impl LookupRegisters,
    world: &dyn LookupWorldBackend,
    index: i32,
    name: &[u8],
) -> Result<(), LookupError> {
    if index < 0 {
        return Err(LookupError::NegativeIndex(index));
    }
    let index = index as u32;
    if index as usize >= control.frame_register_count() {
        return Ok(());
    }
    valid_name(name)?;
    let frame = if name.is_empty() {
        match world.owner()? {
            Some(actor) => world.actor_frame(actor)?,
            None => None,
        }
    } else {
        frame_lookup::find_frame(
            world.frames()?,
            world.scene()?,
            name,
            frame_lookup::SCRIPT_FIND_FLAGS,
            frame_lookup::MAX_LOOKUP_VISITS,
        )?
    };
    control.write_frame_register(index, frame)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_lookup::{FrameLifetime, FrameView};
    use crate::named_script::ResetState;
    use crate::script_scheduler::{
        self, ExecutionError, Program, ProgramBackend, SchedulerBackend, ScriptEffects,
        StartupCommand,
    };
    struct World {
        actors: Vec<ActorLifetime>,
        frames: Vec<FrameLifetime>,
        names: Vec<Vec<u8>>,
        actor_frames: Vec<Option<usize>>,
        primary: Vec<Option<ActorReferenceId>>,
        secondary: Vec<Option<ActorReferenceId>>,
        players: [Option<ActorReferenceId>; 2],
        owner: Option<ActorReferenceId>,
        scene: Option<SceneRoots>,
        beeps: usize,
        fail_beep: bool,
        mutate_beep: bool,
        fail_secondary: bool,
        fail_frames: bool,
        game_default_c_locale: bool,
        ls_default_c_locale: bool,
    }
    impl World {
        fn new() -> Self {
            Self {
                actors: (0..6).map(ActorLifetime::for_existing).collect(),
                frames: (0..6).map(FrameLifetime::for_existing).collect(),
                names: vec![b"alpha".to_vec(); 6],
                actor_frames: (0..6).map(Some).collect(),
                primary: vec![],
                secondary: vec![],
                players: [None; 2],
                owner: None,
                scene: None,
                beeps: 0,
                fail_beep: false,
                mutate_beep: false,
                fail_secondary: false,
                fail_frames: false,
                game_default_c_locale: true,
                ls_default_c_locale: true,
            }
        }
        fn a(&self, i: usize) -> ActorReferenceId {
            self.actors[i].reference()
        }
        fn f(&self, i: usize) -> FrameReferenceId {
            self.frames[i].reference()
        }
    }
    impl FrameLookupBackend for World {
        fn default_c_locale(&self) -> bool {
            self.ls_default_c_locale
        }
        fn frame(&self, reference: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
            let i = self
                .frames
                .iter()
                .position(|f| f.reference() == reference)
                .ok_or(FrameLookupError::StaleFrame(reference))?;
            Ok(FrameView {
                name: Some(&self.names[i]),
                child: None,
                next: None,
            })
        }
    }
    impl LookupWorldBackend for World {
        fn diagnostic_default_c_locale(&self) -> bool {
            self.game_default_c_locale
        }
        fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
            Ok(self.owner)
        }
        fn actor_frame(
            &self,
            actor: ActorReferenceId,
        ) -> Result<Option<FrameReferenceId>, LookupError> {
            let i = self
                .actors
                .iter()
                .position(|a| a.reference() == actor)
                .ok_or(LookupError::StaleActor(actor))?;
            Ok(self.actor_frames[i].map(|j| self.f(j)))
        }
        fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
            Ok(&self.primary)
        }
        fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
            if self.fail_secondary {
                Err(LookupError::Backend("secondary unavailable".into()))
            } else {
                Ok(&self.secondary)
            }
        }
        fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
            Ok(self.players)
        }
        fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> {
            if self.fail_frames {
                Err(LookupError::Backend("frame backend unavailable".into()))
            } else {
                Ok(self)
            }
        }
        fn scene(&self) -> Result<SceneRoots, LookupError> {
            self.scene.ok_or(LookupError::MissingScene)
        }
        fn message_beep(&mut self, argument: i32) -> Result<(), LookupError> {
            assert_eq!(argument, -1);
            self.beeps += 1;
            if self.fail_beep {
                return Err(LookupError::Backend("beep unavailable".into()));
            }
            if self.mutate_beep {
                self.primary = vec![Some(self.a(2))];
            }
            Ok(())
        }
    }
    struct Effects<'a>(&'a mut World);
    impl ScriptEffects for Effects<'_> {
        type Error = &'static str;
        fn execute(
            &mut self,
            _: StartupCommand,
            _: &mut ControlState,
            _: &mut ResetState,
            _: i32,
        ) -> Result<(), Self::Error> {
            Err("unexpected startup effect")
        }
        fn lookup_world(&mut self) -> Result<&mut dyn LookupWorldBackend, LookupError> {
            Ok(self.0)
        }
    }
    fn state() -> ResetState {
        ResetState {
            program_counter: 0,
            short_stack_begin: 0,
            short_stack_end: 0,
            field_44: 0,
            continuation: 1,
        }
    }
    fn control() -> ControlState {
        let mut c = ControlState::new(0, 0x3f).unwrap();
        let mut w = World::new();
        for declaration in ["dim_act 2", "dim_frm 2"] {
            dispatch(&mut c, &mut w, declaration).unwrap();
        }
        c
    }
    fn dispatch(
        c: &mut ControlState,
        w: &mut World,
        source: &str,
    ) -> Result<(), ExecutionError<&'static str>> {
        let p = Program::from_source(source).unwrap();
        let mut effects = Effects(w);
        ProgramBackend {
            program: &p,
            control: c,
            effects: &mut effects,
        }
        .dispatch(&mut state(), 3)
    }
    #[test]
    fn ordered_actor_sources_and_distinct_slot_miss_semantics() {
        let mut c = control();
        let mut w = World::new();
        w.primary = vec![Some(w.a(0)), Some(w.a(1))];
        w.players = [Some(w.a(2)), Some(w.a(3))];
        w.secondary = vec![Some(w.a(4)), Some(w.a(5))];
        dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(1))));
        w.primary.clear();
        dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(5))));
        w.secondary.clear();
        dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(2))));
        w.players = [None; 2];
        dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(2))));
        w.primary = vec![Some(w.a(1))];
        dispatch(&mut c, &mut w, "findactor 0,\"ALPHA\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(2))));
        c.set_frame_reference(0, Some(w.f(1))).unwrap();
        w.scene = Some(SceneRoots {
            primary: Some(w.f(0)),
            backdrop: Some(w.f(0)),
        });
        dispatch(&mut c, &mut w, "findframe 0,\"ALPHA\"").unwrap();
        assert_eq!(c.frame_reference(0), Ok(Some(w.f(0))));
        dispatch(&mut c, &mut w, "findframe 0,\"absent\"").unwrap();
        assert_eq!(c.frame_reference(0), Ok(None));
    }
    #[test]
    fn diagnostic_before_index_and_live_reads_and_required_failure() {
        let mut c = control();
        let mut w = World::new();
        w.owner = Some(w.a(0));
        w.names[0] = b"CHCIPNUTI CHLAPKA".to_vec();
        w.mutate_beep = true;
        dispatch(&mut c, &mut w, "findactor 9,\"alpha\"").unwrap();
        assert_eq!(w.beeps, 1);
        assert_eq!(
            dispatch(&mut c, &mut w, "findactor -1,\"alpha\""),
            Err(ExecutionError::Lookup(LookupError::NegativeIndex(-1)))
        );
        assert_eq!(w.beeps, 2);
        dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(2))));
        w.fail_beep = true;
        assert!(matches!(
            dispatch(&mut c, &mut w, "findactor 9,\"alpha\""),
            Err(ExecutionError::Lookup(LookupError::Backend(_)))
        ));
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(2))));
    }
    #[test]
    fn owner_null_frame_null_and_no_actual_scene() {
        let mut c = control();
        let mut w = World::new();
        c.set_actor_reference(0, Some(w.a(1))).unwrap();
        c.set_frame_reference(0, Some(w.f(1))).unwrap();
        dispatch(&mut c, &mut w, "findactor 0,\"\"").unwrap();
        dispatch(&mut c, &mut w, "findframe 0,\"\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(None));
        assert_eq!(c.frame_reference(0), Ok(None));
        w.owner = Some(w.a(0));
        w.actor_frames[0] = None;
        dispatch(&mut c, &mut w, "findactor 0,\"\"").unwrap();
        dispatch(&mut c, &mut w, "findframe 0,\"\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(0))));
        assert_eq!(c.frame_reference(0), Ok(None));
        assert_eq!(
            dispatch(&mut c, &mut w, "findframe 0,\"alpha\""),
            Err(ExecutionError::Lookup(LookupError::MissingScene))
        );
        dispatch(&mut c, &mut w, "findframe 9,\"alpha\"").unwrap();
        assert_eq!(
            dispatch(&mut c, &mut w, "findframe -1,\"alpha\""),
            Err(ExecutionError::Lookup(LookupError::NegativeIndex(-1)))
        );
    }
    #[test]
    fn native_write_before_later_error_and_early_match_skips_unreachable_sources() {
        let mut c = control();
        let mut w = World::new();
        w.fail_secondary = true;
        w.players[0] = Some(w.a(2));
        assert!(dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").is_err());
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(2))));
        w.primary.push(Some(w.a(1)));
        dispatch(&mut c, &mut w, "findactor 0,\"alpha\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(1))));
        // A total miss retains even an old identity which is no longer live.
        w.fail_secondary = false;
        w.primary.clear();
        w.players = [None; 2];
        let old = ActorLifetime::for_existing(1).reference();
        c.set_actor_reference(0, Some(old)).unwrap();
        dispatch(&mut c, &mut w, "findactor 0,\"missing\"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(old)));
    }
    #[test]
    fn literal_frontend_rejects_expressions_escapes_and_unbounded_names() {
        let mut c = control();
        let mut w = World::new();
        for source in [
            "findactor $n,\"alpha\"",
            "findframe 0,alpha",
            "findactor 0,\"a\"\"b\"",
            "findactor 1+1,\"alpha\"",
            "findframe 0,\"ž\"",
            "findactor 0,\"alpha\" //comment",
        ] {
            let p = Program::from_source(source).unwrap();
            let mut effects = Effects(&mut w);
            assert!(
                matches!(
                    (ProgramBackend {
                        program: &p,
                        control: &mut c,
                        effects: &mut effects
                    })
                    .opcode(0, 0),
                    Err(ExecutionError::UnsupportedCommand { .. })
                ),
                "{source}"
            );
        }
        let p = Program::from_source(&format!(
            "findactor 0,\"{}\"",
            "a".repeat(MAX_LOOKUP_NAME + 1)
        ))
        .unwrap();
        let mut effects = Effects(&mut w);
        assert!(matches!(
            (ProgramBackend {
                program: &p,
                control: &mut c,
                effects: &mut effects
            })
            .opcode(0, 0),
            Err(ExecutionError::UnsupportedCommand { .. })
        ));
        // Known following opcodes remain in the same fixed program; no skipping.
        assert_eq!(
            script_scheduler::Program::from_source("findactor 0,\"alpha\"\nunknown\nend")
                .unwrap()
                .len(),
            3
        );
    }
    #[test]
    fn diagnostic_literal_space_is_distinct_from_underscore() {
        let mut c = control();
        let mut w = World::new();
        w.owner = Some(w.a(0));
        w.names[0] = b"chcipnuti_chlapka".to_vec();
        find_actor(&mut c, &mut w, 99, b"alpha").unwrap();
        assert_eq!(w.beeps, 0);
        w.names[0] = b"chcipnuti chlapka".to_vec();
        find_actor(&mut c, &mut w, 99, b"alpha").unwrap();
        assert_eq!(w.beeps, 1);
    }
    #[test]
    fn live_borrowed_world_rejects_old_incarnations_without_touching_retained_slots() {
        struct Diagnostic;
        impl LookupDiagnosticBackend for Diagnostic {
            fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
                Err(LookupError::Backend("not bound".into()))
            }
        }
        let old = ActorLifetime::for_existing(9).reference();
        let live = ActorLifetime::for_existing(9);
        let records = [LiveActor {
            lifetime: &live,
            frame: None,
        }];
        let frame_records = [];
        let frames = frame_lookup::LiveFrameTree::borrow(&frame_records, true).unwrap();
        let mut diagnostic = Diagnostic;
        let mut world = LiveLookupWorld::borrow(&records, &frames, &mut diagnostic).unwrap();
        assert_eq!(world.actor_frame(old), Err(LookupError::StaleActor(old)));
        assert_eq!(
            world.actor_frame(ActorReferenceId::from_existing(9)),
            Err(LookupError::StaleActor(ActorReferenceId::from_existing(9)))
        );
        let mut c = control();
        c.set_actor_reference(0, Some(old)).unwrap();
        find_actor(&mut c, &mut world, 0, b"absent").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(old)));
        world.owner = Some(live.reference());
        find_actor(&mut c, &mut world, 0, b"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(live.reference())));
    }
    #[test]
    fn empty_frame_lookup_copies_without_reading_frame_names_or_scene() {
        let mut c = control();
        let mut w = World::new();
        w.owner = Some(w.a(0));
        w.fail_frames = true;
        find_frame(&mut c, &mut w, 0, b"").unwrap();
        assert_eq!(c.frame_reference(0), Ok(Some(w.f(0))));
        w.actor_frames[0] = None;
        find_frame(&mut c, &mut w, 0, b"").unwrap();
        assert_eq!(c.frame_reference(0), Ok(None));
        w.owner = None;
        find_frame(&mut c, &mut w, 0, b"").unwrap();
        assert_eq!(c.frame_reference(0), Ok(None));
    }
    #[test]
    fn game_and_ls3df_crt_locale_admission_are_independent_and_lazy() {
        let mut c = control();
        let mut w = World::new();
        w.owner = Some(w.a(0));
        w.names[0] = b"chcipnuti chlapka".to_vec();
        w.ls_default_c_locale = false;
        find_actor(&mut c, &mut w, 99, b"alpha").unwrap();
        assert_eq!(w.beeps, 1);
        w.game_default_c_locale = false;
        w.ls_default_c_locale = true;
        assert_eq!(
            find_actor(&mut c, &mut w, 99, b"alpha"),
            Err(LookupError::UnsupportedDiagnosticLocale)
        );
        w.owner = None;
        w.primary = vec![Some(w.a(1))];
        find_actor(&mut c, &mut w, 0, b"alpha").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(1))));
        w.owner = Some(w.a(0));
        w.actor_frames[0] = None;
        find_actor(&mut c, &mut w, 0, b"").unwrap();
        assert_eq!(c.actor_reference(0), Ok(Some(w.a(0))));
    }
}
