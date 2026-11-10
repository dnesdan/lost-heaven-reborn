use super::*;
use crate::entity_actor::EntityProcessState;
use crate::script_lookup::{
    DetectorCollisionFlags, FrameSetOnOutcome, LookupError, LookupWorldBackend,
};
use std::collections::{HashMap, HashSet};
#[derive(Default)]
struct Memory {
    next: u64,
    events: Vec<(RuntimeEffect, u32, u32, u32, u32)>,
    fail: Option<RuntimeEffect>,
    redirect_release: bool,
    entity_world: FakeEntityWorld,
}
#[derive(Default)]
struct FakeEntityWorld {
    states: HashMap<ActorReferenceId, EntityProcessState>,
    act_states: HashMap<ActorReferenceId, u32>,
    weapons: HashMap<ActorReferenceId, Vec<(u16, i32, i32)>>,
    fail_writes: bool,
    set_event_calls: Vec<(ActorReferenceId, i32)>,
    set_event_results: HashMap<(ActorReferenceId, i32), bool>,
    set_event_fail: Option<ActorReferenceId>,
    actor_frames: HashMap<ActorReferenceId, FrameReferenceId>,
    frame_switches: Vec<(FrameReferenceId, bool)>,
    frame_states: HashMap<FrameReferenceId, bool>,
    sound_frames: HashSet<FrameReferenceId>,
    fail_frame_switch: bool,
    detector_signals: HashMap<ActorReferenceId, bool>,
    detector_sets: Vec<(ActorReferenceId, bool)>,
    non_detectors: HashSet<ActorReferenceId>,
    fail_detector: Option<ActorReferenceId>,
    dead: HashMap<ActorReferenceId, bool>,
    fail_dead: Option<ActorReferenceId>,
    frame_positions: HashMap<FrameReferenceId, [f32; 3]>,
    fail_frame_pos: Option<FrameReferenceId>,
    human_cars: HashMap<ActorReferenceId, Option<ActorReferenceId>>,
    non_humans: HashSet<ActorReferenceId>,
    fail_human_car: Option<ActorReferenceId>,
    car_speeds: HashMap<ActorReferenceId, f32>,
    non_cars: HashSet<ActorReferenceId>,
    fail_car: Option<ActorReferenceId>,
    break_motors: HashMap<ActorReferenceId, bool>,
    force_stops: Vec<ActorReferenceId>,
    player_locked: Option<bool>,
    fail_player_lock: bool,
    detector_types: HashMap<ActorReferenceId, u32>,
    fail_detector_types: Option<ActorReferenceId>,
    active_list: Vec<Option<ActorReferenceId>>,
    fail_active_list: bool,
    scan_candidates: HashMap<ActorReferenceId, Option<(Option<FrameReferenceId>, u32)>>,
    fail_candidate: Option<ActorReferenceId>,
    collision_flags: HashMap<ActorReferenceId, DetectorCollisionFlags>,
    fail_collision_flags: Option<ActorReferenceId>,
}
impl LookupWorldBackend for FakeEntityWorld {
    fn diagnostic_default_c_locale(&self) -> bool {
        true
    }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn actor_frame(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<FrameReferenceId>, LookupError> {
        if let Some(frame) = self.actor_frames.get(&actor) {
            return Ok(Some(*frame));
        }
        Err(LookupError::StaleActor(actor))
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn frames(&self) -> Result<&dyn crate::frame_lookup::FrameLookupBackend, LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn scene(&self) -> Result<crate::frame_lookup::SceneRoots, LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
        Err(LookupError::UnsupportedWorldBackend)
    }
    fn load_entity_process_state(
        &mut self,
        owner: Option<ActorReferenceId>,
    ) -> Result<EntityProcessState, LookupError> {
        let actor =
            owner.ok_or_else(|| LookupError::Backend("entity command without owner".into()))?;
        Ok(self.states.get(&actor).copied().unwrap_or_default())
    }
    fn store_entity_process_state(
        &mut self,
        owner: Option<ActorReferenceId>,
        state: &EntityProcessState,
    ) -> Result<(), LookupError> {
        let actor =
            owner.ok_or_else(|| LookupError::Backend("entity command without owner".into()))?;
        self.states.insert(actor, *state);
        Ok(())
    }
    fn set_actor_act_state(
        &mut self,
        actor: ActorReferenceId,
        state: u32,
    ) -> Result<(), LookupError> {
        if self.fail_writes {
            return Err(LookupError::Backend("injected write failure".into()));
        }
        self.act_states.insert(actor, state);
        Ok(())
    }
    fn human_add_weapon(
        &mut self,
        actor: ActorReferenceId,
        id: u16,
        loaded: i32,
        spare: i32,
    ) -> Result<(), LookupError> {
        self.weapons
            .entry(actor)
            .or_default()
            .push((id, loaded, spare));
        Ok(())
    }
    fn set_program_event(
        &mut self,
        actor: ActorReferenceId,
        event_id: i32,
    ) -> Result<bool, LookupError> {
        if let Some(fail_actor) = self.set_event_fail {
            if fail_actor == actor {
                return Err(LookupError::Backend("injected set-event failure".into()));
            }
        }
        self.set_event_calls.push((actor, event_id));
        Ok(self
            .set_event_results
            .get(&(actor, event_id))
            .copied()
            .unwrap_or(true))
    }
    fn set_frame_on(
        &mut self,
        frame: FrameReferenceId,
        on: bool,
    ) -> Result<FrameSetOnOutcome, LookupError> {
        if self.fail_frame_switch {
            return Err(LookupError::Backend("injected frame-switch failure".into()));
        }
        self.frame_switches.push((frame, on));
        if self.sound_frames.contains(&frame) {
            // Sound frames take the device switch; no arena store runs.
            return Ok(FrameSetOnOutcome {
                core_applied: false,
                device_effects_deferred: true,
            });
        }
        self.frame_states.insert(frame, on);
        Ok(FrameSetOnOutcome {
            core_applied: true,
            device_effects_deferred: true,
        })
    }
    fn detector_signal(&self, actor: ActorReferenceId) -> Result<Option<bool>, LookupError> {
        if self.fail_detector == Some(actor) {
            return Err(LookupError::Backend("injected detector-signal failure".into()));
        }
        if self.non_detectors.contains(&actor) {
            return Ok(None);
        }
        Ok(Some(self.detector_signals.get(&actor).copied().unwrap_or(false)))
    }
    fn set_detector_signal(&mut self, actor: ActorReferenceId, on: bool) -> Result<(), LookupError> {
        if self.fail_detector == Some(actor) {
            return Err(LookupError::Backend("injected detector-signal failure".into()));
        }
        if self.non_detectors.contains(&actor) {
            return Ok(());
        }
        self.detector_sets.push((actor, on));
        self.detector_signals.insert(actor, on);
        Ok(())
    }
    fn actor_dead(&self, actor: ActorReferenceId) -> Result<bool, LookupError> {
        if self.fail_dead == Some(actor) {
            return Err(LookupError::Backend("injected dead failure".into()));
        }
        Ok(self.dead.get(&actor).copied().unwrap_or(false))
    }
    fn frame_world_position(
        &mut self,
        frame: FrameReferenceId,
    ) -> Result<[f32; 3], LookupError> {
        if self.fail_frame_pos == Some(frame) {
            return Err(LookupError::Backend("injected frame-pos failure".into()));
        }
        self.frame_positions
            .get(&frame)
            .copied()
            .ok_or(LookupError::Backend("frame position unavailable".into()))
    }
    fn human_car(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<Option<ActorReferenceId>>, LookupError> {
        if self.fail_human_car == Some(actor) {
            return Err(LookupError::Backend("injected human-car failure".into()));
        }
        if self.non_humans.contains(&actor) {
            return Ok(None);
        }
        if let Some(car) = self.human_cars.get(&actor) {
            return Ok(Some(*car));
        }
        Err(LookupError::StaleActor(actor))
    }
    fn car_speed(&self, actor: ActorReferenceId) -> Result<Option<f32>, LookupError> {
        if self.fail_car == Some(actor) {
            return Err(LookupError::Backend("injected car failure".into()));
        }
        if self.non_cars.contains(&actor) {
            return Ok(None);
        }
        if let Some(speed) = self.car_speeds.get(&actor) {
            return Ok(Some(*speed));
        }
        Err(LookupError::StaleActor(actor))
    }
    fn is_car_usable(&self, actor: ActorReferenceId) -> Result<Option<bool>, LookupError> {
        if self.fail_car == Some(actor) {
            return Err(LookupError::Backend("injected car failure".into()));
        }
        if self.non_cars.contains(&actor) {
            return Ok(None);
        }
        if self.car_speeds.contains_key(&actor) || self.break_motors.contains_key(&actor) {
            return Ok(Some(false));
        }
        Err(LookupError::StaleActor(actor))
    }
    fn set_car_break_motor(
        &mut self,
        actor: ActorReferenceId,
        on: bool,
    ) -> Result<bool, LookupError> {
        if self.fail_car == Some(actor) {
            return Err(LookupError::Backend("injected car failure".into()));
        }
        if self.non_cars.contains(&actor) {
            return Ok(false);
        }
        if self.car_speeds.contains_key(&actor) || self.break_motors.contains_key(&actor) {
            self.break_motors.insert(actor, on);
            return Ok(true);
        }
        Err(LookupError::StaleActor(actor))
    }
    fn force_stop_car(&mut self, actor: ActorReferenceId) -> Result<bool, LookupError> {
        if self.fail_car == Some(actor) {
            return Err(LookupError::Backend("injected car failure".into()));
        }
        if self.non_cars.contains(&actor) {
            return Ok(false);
        }
        if self.car_speeds.contains_key(&actor) || self.break_motors.contains_key(&actor) {
            self.force_stops.push(actor);
            return Ok(true);
        }
        Err(LookupError::StaleActor(actor))
    }
    fn set_player_controls_locked(&mut self, locked: bool) -> Result<(), LookupError> {
        if self.fail_player_lock {
            return Err(LookupError::Backend("injected player-lock failure".into()));
        }
        self.player_locked = Some(locked);
        Ok(())
    }
    fn detector_human_types(&self, actor: ActorReferenceId) -> Result<u32, LookupError> {
        if self.fail_detector_types == Some(actor) {
            return Err(LookupError::Backend("injected human-types failure".into()));
        }
        if let Some(types) = self.detector_types.get(&actor) {
            return Ok(*types);
        }
        if self.non_detectors.contains(&actor) {
            return Err(LookupError::Backend("actor is not a detector".into()));
        }
        Err(LookupError::StaleActor(actor))
    }
    fn active_actors(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        if self.fail_active_list {
            return Err(LookupError::Backend("injected active-list failure".into()));
        }
        Ok(&self.active_list)
    }
    fn detector_inrange_candidate(
        &self,
        actor: ActorReferenceId,
    ) -> Result<Option<(Option<FrameReferenceId>, u32)>, LookupError> {
        if self.fail_candidate == Some(actor) {
            return Err(LookupError::Backend("injected candidate failure".into()));
        }
        if let Some(candidate) = self.scan_candidates.get(&actor) {
            return Ok(*candidate);
        }
        Err(LookupError::StaleActor(actor))
    }
    fn detector_collision_flags(
        &self,
        actor: ActorReferenceId,
    ) -> Result<DetectorCollisionFlags, LookupError> {
        if self.fail_collision_flags == Some(actor) {
            return Err(LookupError::Backend("injected collision-flags failure".into()));
        }
        if let Some(flags) = self.collision_flags.get(&actor) {
            return Ok(*flags);
        }
        Err(LookupError::StaleActor(actor))
    }
    fn set_detector_collision_flags(
        &mut self,
        actor: ActorReferenceId,
        flags: DetectorCollisionFlags,
    ) -> Result<(), LookupError> {
        if self.fail_collision_flags == Some(actor) {
            return Err(LookupError::Backend("injected collision-flags failure".into()));
        }
        if self.collision_flags.contains_key(&actor) {
            self.collision_flags.insert(actor, flags);
            return Ok(());
        }
        Err(LookupError::StaleActor(actor))
    }
    // detector_erase_colls, detector_create_colls and detector_create_use_object
    // keep the trait default refusal (UnsupportedWorldBackend): no
    // dynamic-collision or using-object owner exists in the test world, which
    // is exactly the deferred production state.
}
impl ProgramBackend for Memory {
    fn allocate(
        &mut self,
        _: &CProgram,
        _: ProgramBuffer,
        _: u32,
    ) -> Result<ProgramAllocation, String> {
        self.next += 1;
        Ok(ProgramAllocation(self.next))
    }
    fn free(
        &mut self,
        _: &CProgram,
        _: ProgramBuffer,
        _: Option<ProgramAllocation>,
    ) -> Result<(), String> {
        Ok(())
    }
}
impl RuntimeBackend for Memory {
    fn effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        published: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        self.events.push((
            effect,
            fields.scheduler.program_counter,
            *fields.elapsed,
            *fields.actor_count,
            *fields.frame_count,
        ));
        if effect == RuntimeEffect::ConvertScalar {
            assert!(published.is_some());
            assert_eq!(published.unwrap().value(), None);
        }
        if self.redirect_release && effect == RuntimeEffect::ReleaseActors {
            fields.scheduler.program_counter = 2;
            self.redirect_release = false;
        }
        if self.fail == Some(effect) {
            return Err("required boundary interrupted".into());
        }
        Ok(())
    }
    fn lookup_world(&mut self) -> Result<&mut dyn LookupWorldBackend, LookupError> {
        Ok(&mut self.entity_world)
    }
}
fn program(source: &[u8], memory: &mut Memory) -> CProgram {
    let mut p = CProgram::construct(ProgramSeed {
        address_high_byte: 0x81,
        padding_0d: [5; 3],
        words_1c_2c: [0xdeadbeef; 5],
        padding_31: [7; 3],
        word_40: 0x12345678,
        bytes_64: [0xff; 4],
    });
    p.set_source_code(Some(source), memory).unwrap();
    p
}
#[test]
fn actual_source_compile_init_and_process_use_one_owned_runtime() {
    let mut m = Memory::default();
    let source = b"commandblock 1\ndim_act 3\ndim_frm 2\ncommandblock 0";
    let mut p = program(source, &mut m);
    let original = p.source().unwrap().bytes.clone();
    let progress = p.init_fresh_runtime(&mut m).unwrap();
    assert_eq!((progress.accepted, progress.errors), (4, 0));
    assert_eq!(p.source().unwrap().bytes, original);
    assert!(p.runtime().unwrap().working_source.is_none());
    assert_eq!(p.runtime().unwrap().fields.scheduler.program_counter, 0);
    assert_eq!(p.runtime().unwrap().fields.command_budget_bits, 0x3f000000);
    let mut budget = DispatchBudget::new(20).unwrap();
    p.call_owned_process(7, &mut budget, &mut m).unwrap();
    let r = p.runtime().unwrap();
    assert_eq!(
        (
            r.fields.scheduler.program_counter,
            r.fields.last_pc,
            r.fields.elapsed,
            r.fields.scheduler.continuation
        ),
        (4, 3, 0, 0)
    );
    assert_eq!((r.fields.actor_count, r.fields.frame_count), (3, 2));
    assert_eq!(r.fields.actors.as_ref().unwrap().len(), 3);
    assert_eq!(r.fields.frames.as_ref().unwrap().len(), 2);
    assert!(r
        .fields
        .actors
        .as_ref()
        .unwrap()
        .iter()
        .all(|s| matches!(s, RuntimeReferenceSlot::Empty)));
    assert!(matches!(
        p.set_source_code(Some(b"dim_act 0"), &mut m),
        Err(ProgramError::Unavailable)
    ));
}
#[test]
fn dim_flt_allocates_zeroed_bank_and_advances() {
    let mut m = Memory::default();
    let mut p = program(b"commandblock 1\ndim_flt 2\ncommandblock 0", &mut m);
    p.init_fresh_runtime(&mut m).unwrap();
    let mut budget = DispatchBudget::new(10).unwrap();
    p.call_owned_process(9, &mut budget, &mut m).unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(
        (f.scheduler.program_counter, f.last_pc, f.elapsed),
        (3, 2, 0)
    );
    assert_eq!(f.float_count, 2);
    assert_eq!(f.floats.as_ref().unwrap().as_ref(), &[0.0f32, 0.0f32]);
    assert!(m
        .events
        .iter()
        .any(|event| event.0 == RuntimeEffect::AllocateFloats(2)));
}
#[test]
fn incomplete_compile_retains_source_pending_and_prefix_but_is_not_executable() {
    let mut m = Memory::default();
    let mut p = program(b"commandblock 1\ndim_act 2\nfindactor 0,\"person\"", &mut m);
    m.fail = Some(RuntimeEffect::AllocateName);
    let source = p.source().unwrap().bytes.clone();
    assert!(p.init_fresh_runtime(&mut m).is_err());
    let r = p.runtime().unwrap();
    assert_eq!(r.stage, RuntimeStage::CompileInterrupted);
    assert_eq!(r.instructions.active().len(), 2);
    assert_eq!(r.pending.as_ref().unwrap().opcode, 18);
    assert!(r.working_source.is_some());
    assert_eq!(p.source().unwrap().bytes, source);
    assert!(p
        .call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut m)
        .is_err());
    assert_eq!(p.runtime().unwrap().fields.scheduler.program_counter, 0);
}
#[test]
fn ordinary_native_parameter_rejection_counts_error_and_continues_compile() {
    let mut m = Memory::default();
    let mut p = program(b"dim_act 1,2\ndim_frm 3", &mut m);
    let progress = p.init_fresh_runtime(&mut m).unwrap();
    assert_eq!((progress.errors, progress.accepted), (1, 1));
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::Ready);
    assert_eq!(p.runtime().unwrap().instructions.active()[0].opcode, 11);
}
#[test]
fn done_releases_reverse_parameters_then_frame_actor_and_preserves_capacity() {
    let mut m = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_act 3\ndim_frm 2\ncommandblock 0",
        &mut m,
    );
    p.init_fresh_runtime(&mut m).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut m)
        .unwrap();
    let cap = p.runtime().unwrap().instructions.capacity();
    m.events.clear();
    p.done_fresh_runtime(&mut m).unwrap();
    assert_eq!(
        m.events.iter().map(|e| e.0).collect::<Vec<_>>(),
        vec![
            RuntimeEffect::ReleaseScalar(34),
            RuntimeEffect::ReleaseScalar(11),
            RuntimeEffect::ReleaseScalar(12),
            RuntimeEffect::ReleaseScalar(34),
            RuntimeEffect::ReleaseFrames,
            RuntimeEffect::ReleaseActors
        ]
    );
    let r = p.runtime().unwrap();
    assert_eq!(r.stage, RuntimeStage::Done);
    assert_eq!(r.instructions.capacity(), cap);
    assert_eq!(r.instructions.records.len(), 4);
    assert!(r.instructions.records.iter().all(|i| i.parameter.is_none()));
    assert!(r.instructions.active().is_empty());
    assert_eq!((r.fields.actor_count, r.fields.frame_count), (0, 0));
    assert!(p.destruct(&mut m).is_err());
    assert_eq!(p.discard_runtime_host().unwrap().scalar_allocations, 0);
    assert!(p.destruct(&mut m).is_ok());
}
#[test]
fn failed_done_retains_reached_reverse_prefix_and_blocks_destructive_replay() {
    let mut m = Memory::default();
    let mut p = program(b"dim_act 3\ndim_frm 2", &mut m);
    p.init_fresh_runtime(&mut m).unwrap();
    m.fail = Some(RuntimeEffect::ReleaseScalar(12));
    assert!(p.done_fresh_runtime(&mut m).is_err());
    let r = p.runtime().unwrap();
    assert_eq!(r.stage, RuntimeStage::DisposalInterrupted);
    assert!(r.instructions.records[1].parameter.is_none());
    assert!(r.instructions.records[0].parameter.is_some());
    let events = m.events.len();
    assert!(p.done_fresh_runtime(&mut m).is_err());
    assert_eq!(m.events.len(), events);
}
#[test]
fn native_declaration_rereads_actual_pc_parameter_after_release_callback() {
    let mut m = Memory::default();
    let mut p = program(b"dim_act 2\ndim_act 4\ndim_act 1", &mut m);
    p.init_fresh_runtime(&mut m).unwrap();
    let mut budget = DispatchBudget::new(20).unwrap();
    p.call_owned_process(1, &mut budget, &mut m).unwrap();
    assert_eq!(p.runtime().unwrap().fields.actor_count, 2);
    m.redirect_release = true;
    p.call_owned_process(1, &mut budget, &mut m).unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.actor_count, 1);
    assert_eq!(f.actors.as_ref().unwrap().len(), 1);
    assert_eq!(f.scheduler.program_counter, 3);
}
#[test]
fn allocation_failure_keeps_published_count_after_free_before_new_allocation() {
    let mut m = Memory::default();
    let mut p = program(b"dim_act 2\ndim_act 4", &mut m);
    p.init_fresh_runtime(&mut m).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut m)
        .unwrap();
    m.fail = Some(RuntimeEffect::AllocateActors(4));
    assert!(p
        .call_owned_process(5, &mut DispatchBudget::new(4).unwrap(), &mut m)
        .is_err());
    let f = &p.runtime().unwrap().fields;
    assert_eq!(
        (f.actor_count, f.scheduler.program_counter, f.elapsed),
        (4, 1, 5)
    );
    assert!(f.actors.is_none());
}
#[test]
fn explicit_host_discard_releases_partial_runtime_without_claiming_done_or_reinitializing() {
    let mut m = Memory::default();
    let mut p = program(b"dim_act 2\nfindactor 0,\"person\"", &mut m);
    m.fail = Some(RuntimeEffect::AllocateName);
    assert!(p.init_fresh_runtime(&mut m).is_err());
    assert!(p.destruct(&mut m).is_err());
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.scalar_allocations, 1);
    assert!(report.working_source_bytes > 0);
    assert!(p.runtime().is_none());
    assert!(p.init_fresh_runtime(&mut m).is_err());
    assert!(p.destruct(&mut m).is_ok());
}
#[test]
fn allocation_callback_reduced_count_leaves_unreached_tail_uninitialized() {
    struct Shrink;
    impl RuntimeBackend for Shrink {
        fn effect(
            &mut self,
            fields: RuntimeView<'_>,
            effect: RuntimeEffect,
            _: Option<&ScalarParameter>,
        ) -> Result<(), String> {
            match effect {
                RuntimeEffect::AllocateActors(4) => *fields.actor_count = 2,
                RuntimeEffect::AllocateFrames(4) => *fields.frame_count = 2,
                _ => {}
            }
            Ok(())
        }
    }
    for source in [b"dim_act 4".as_slice(), b"dim_frm 4".as_slice()] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut Shrink)
            .unwrap();
        let f = &p.runtime().unwrap().fields;
        let slots: Vec<_> = if let Some(a) = &f.actors {
            a.iter()
                .map(|s| s.reference().map(|v| v.is_none()))
                .collect()
        } else {
            f.frames
                .as_ref()
                .unwrap()
                .iter()
                .map(|s| s.reference().map(|v| v.is_none()))
                .collect()
        };
        assert_eq!(
            slots,
            vec![
                Ok(true),
                Ok(true),
                Err(RuntimeReferenceError::Uninitialized),
                Err(RuntimeReferenceError::Uninitialized)
            ]
        );
        p.discard_runtime_host().unwrap();
    }
}
#[test]
fn oversized_zero_fill_keeps_allocated_written_prefix_before_first_unavailable_slot() {
    struct Grow;
    impl RuntimeBackend for Grow {
        fn effect(
            &mut self,
            fields: RuntimeView<'_>,
            effect: RuntimeEffect,
            _: Option<&ScalarParameter>,
        ) -> Result<(), String> {
            match effect {
                RuntimeEffect::AllocateActors(2) => *fields.actor_count = 4,
                RuntimeEffect::AllocateFrames(2) => *fields.frame_count = 4,
                _ => {}
            }
            Ok(())
        }
    }
    for source in [b"dim_act 2".as_slice(), b"dim_frm 2".as_slice()] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        assert!(p
            .call_owned_process(7, &mut DispatchBudget::new(4).unwrap(), &mut Grow)
            .is_err());
        let r = p.runtime().unwrap();
        assert_eq!(r.stage, RuntimeStage::ProcessInterrupted);
        assert_eq!(r.fields.program_counter(), 0);
        assert_eq!(r.fields.elapsed, 7);
        if let Some(a) = &r.fields.actors {
            assert_eq!(r.fields.actor_count, 4);
            assert_eq!(a.len(), 2);
            assert!(a.iter().all(|s| s.reference() == Ok(None)));
        } else {
            let a = r.fields.frames.as_ref().unwrap();
            assert_eq!(r.fields.frame_count, 4);
            assert_eq!(a.len(), 2);
            assert!(a.iter().all(|s| s.reference() == Ok(None)));
        }
        p.discard_runtime_host().unwrap();
    }
}
#[test]
fn host_count_limit_keeps_native_free_clear_and_count_store_prefix() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_act 2\ndim_act 70000", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .unwrap();
    memory.events.clear();
    assert!(p
        .call_owned_process(5, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .is_err());
    let f = &p.runtime().unwrap().fields;
    assert!(f.actors.is_none());
    assert_eq!(f.actor_count, 70000);
    assert_eq!(f.elapsed, 5);
    assert_eq!(
        memory.events.iter().map(|e| e.0).collect::<Vec<_>>(),
        vec![RuntimeEffect::ReleaseActors]
    );
    p.discard_runtime_host().unwrap();
}
#[test]
fn initially_oversized_request_uses_callback_reloaded_allowed_parameter() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_act 2\ndim_act 70000\ndim_act 1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .unwrap();
    memory.redirect_release = true;
    p.call_owned_process(5, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.actor_count, 1);
    assert_eq!(f.actors.as_ref().unwrap().len(), 1);
    assert_eq!(f.program_counter(), 3);
    p.discard_runtime_host().unwrap();
}
#[test]
fn detector_source_compiles_with_base_table_but_refuses_subroutine_entry() {
    // C_detector::DecodeInstruction/DecodeParams forward to C_program
    // (mafia-re 113b5b6 game/actors/C_detector.cpp:433ce0-433d00), so the
    // C_detector::GameInit (GAME 0x432a00) C_program::Init prefix admits
    // detector sources with the base table. Detector Process dispatch (GAME
    // RVA 0x33d10) runs through the Detector vtable inside CallProcess;
    // CallSubroutine stays base-only.
    let mut memory = Memory::default();
    let mut p = program(
        b"dim_act 2\ndetector_setsignal -1, 1\ndetector_inrange 0, 10",
        &mut memory,
    );
    p.set_detector_vtable();
    let before = p.source().unwrap().bytes.clone();
    memory.events.clear();
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!(
        progress,
        CompileProgress {
            errors: 0,
            accepted: 3
        }
    );
    assert!(p.runtime().is_some());
    assert!(p.runtime_started);
    assert_eq!(p.vtable(), ProgramVtable::Detector);
    assert_eq!(p.source().unwrap().bytes, before);
    // A repeated Init refuses with the source lifetime intact.
    assert_eq!(
        p.init_fresh_runtime(&mut memory),
        Err(RuntimeError::Domain("fresh source program required"))
    );
    assert!(p.runtime().is_some());
    assert_eq!(p.source().unwrap().bytes, before);
    // Subroutine entry stays base-only for derived programs.
    assert_eq!(
        p.call_owned_subroutine(&mut DispatchBudget::new(8).unwrap(), &mut memory),
        Err(RuntimeError::UnsupportedDerivedRuntime(
            ProgramVtable::Detector
        ))
    );
    // The admitted INRANGE now runs the detector scan, which needs the
    // script owner: each call runs one instruction, so dim_act and the
    // ownerless setsignal (silent gate, base dispatch) advance first, and the
    // third call stops loudly at the scan instead of advancing.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    assert_eq!(
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory),
        Err(RuntimeError::Lookup(LookupError::Backend(
            "detector command without owner".into()
        )))
    );
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    assert_eq!(
        p.runtime().unwrap().stage,
        RuntimeStage::ProcessInterrupted
    );
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.pair_allocations, 1);
    assert_eq!(report.remaining_allocations, 1);
    p.destruct(&mut memory).unwrap();
    assert_eq!(p.phase(), ProgramPhase::Dead);
}
#[test]
fn entity_source_compiles_mixed_tables_but_refuses_derived_execution() {
    let mut memory = Memory::default();
    let mut p = program(
        b"enemy_wait\nenemy_vidim 0\nlabel start\ngoto start",
        &mut memory,
    );
    p.set_entity_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(9)))
        .unwrap();
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!(
        progress,
        CompileProgress {
            errors: 0,
            accepted: 4
        }
    );
    let opcodes: Vec<u32> = p
        .runtime()
        .unwrap()
        .instructions
        .active()
        .iter()
        .map(|instruction| instruction.opcode)
        .collect();
    assert_eq!(opcodes, [0x1004, 0x101a, 0, 1]);
    let payloads: Vec<Option<usize>> = p
        .runtime()
        .unwrap()
        .instructions
        .active()
        .iter()
        .map(|instruction| {
            instruction
                .parameter
                .as_ref()
                .and_then(|parameter| parameter.entity_parameter())
                .map(|parameter| parameter.count())
        })
        .collect();
    assert_eq!(payloads, [Some(0), Some(1), None, None]);
    assert_eq!(
        p.runtime().unwrap().instructions.active()[1]
            .parameter
            .as_ref()
            .and_then(|parameter| parameter.entity_parameter())
            .and_then(|parameter| parameter.word(0)),
        Some(b"0".as_slice())
    );
    // A fresh WAIT parks the program counter: the no-flag path stays.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    // Armed by the wait protocol flag, the same WAIT advances; VIDIM "0"
    // decodes beside both blind branches and advances unchanged.
    memory.entity_world.states.insert(
        ActorReferenceId::from_existing(9),
        EntityProcessState {
            process_flags: 0x10,
            visibility_bits: 3,
            shut_up: 9,
        },
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    assert_eq!(
        memory
            .entity_world
            .states
            .get(&ActorReferenceId::from_existing(9)),
        Some(&EntityProcessState {
            process_flags: 0x10,
            visibility_bits: 3,
            shut_up: 9,
        })
    );
    // The subroutine entry stays refused for entity programs.
    assert_eq!(
        p.call_owned_subroutine(&mut DispatchBudget::new(8).unwrap(), &mut memory),
        Err(RuntimeError::UnsupportedDerivedRuntime(
            ProgramVtable::Entity
        ))
    );
    p.discard_runtime_host().unwrap();
}
#[test]
fn entity_source_rejects_nonconforming_word_counts_like_the_native_compiler() {
    let mut memory = Memory::default();
    let mut p = program(b"enemy_lookto 1, 2", &mut memory);
    p.set_entity_vtable();
    assert!(p.init_fresh_runtime(&mut memory).is_err());
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::CompileInterrupted);
}
#[test]
fn entity_owner_words_apply_state_and_advance() {
    let mut memory = Memory::default();
    let mut p = program(
        b"enemy_vidim 1\nenemy_shut_up yes\nenemy_naprdelivaute no\nenemy_followplayer yes",
        &mut memory,
    );
    p.set_entity_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    memory.entity_world.states.insert(
        ActorReferenceId::from_existing(7),
        EntityProcessState {
            process_flags: 0,
            visibility_bits: 3,
            shut_up: 9,
        },
    );
    for expected in 1..=4 {
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(
            p.runtime().unwrap().fields.program_counter(),
            expected,
            "after dispatch {expected}"
        );
    }
    // VIDIM "1" clears the blind bit, SHUT_UP "yes" clears the mute byte,
    // NAPRDELIVAUTE "no" clears its flag, FOLLOWPLAYER "yes" sets its flag.
    assert_eq!(
        memory
            .entity_world
            .states
            .get(&ActorReferenceId::from_existing(7)),
        Some(&EntityProcessState {
            process_flags: 0x10000,
            visibility_bits: 1,
            shut_up: 0,
        })
    );
    // Past the end the scheduler stops without touching state.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 4);
}
#[test]
fn entity_wait_parks_until_armed_and_unimplemented_commands_refuse() {
    let mut memory = Memory::default();
    let mut p = program(b"enemy_wait\nenemy_move 5", &mut memory);
    p.set_entity_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(8)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    // The timer branch has no game-condition source yet: it parks like the
    // native wait while its condition is unmet.
    memory.entity_world.states.insert(
        ActorReferenceId::from_existing(8),
        EntityProcessState {
            process_flags: 0x20,
            visibility_bits: 0,
            shut_up: 0,
        },
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    memory.entity_world.states.insert(
        ActorReferenceId::from_existing(8),
        EntityProcessState {
            process_flags: 0x10,
            visibility_bits: 0,
            shut_up: 0,
        },
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("UnsupportedCommand(4096)"),
        "{error:?}"
    );
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
}
#[test]
fn entity_commands_without_owner_or_world_refuse_explicitly() {
    let mut memory = Memory::default();
    let mut p = program(b"enemy_vidim 1", &mut memory);
    p.set_entity_vtable();
    p.init_fresh_runtime(&mut memory).unwrap();
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(format!("{error:?}").contains("without owner"), "{error:?}");
}
fn interrupted_process_release_blocks_done_and_execution(
    source: &[u8],
    release: RuntimeEffect,
    unwind: bool,
) {
    struct UncertainRelease {
        release: RuntimeEffect,
        calls: usize,
        observable_frees: usize,
        unwind: bool,
    }
    impl RuntimeBackend for UncertainRelease {
        fn effect(
            &mut self,
            _: RuntimeView<'_>,
            effect: RuntimeEffect,
            _: Option<&ScalarParameter>,
        ) -> Result<(), String> {
            self.calls += 1;
            if effect == self.release {
                self.observable_frees += 1;
                if self.unwind {
                    panic!("external free completed before unwind");
                }
                return Err("external free completed before interruption".into());
            }
            Ok(())
        }
    }
    let mut memory = Memory::default();
    let mut p = program(source, &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .unwrap();
    let mut backend = UncertainRelease {
        release,
        calls: 0,
        observable_frees: 0,
        unwind,
    };
    if unwind {
        assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            p.call_owned_process(5, &mut DispatchBudget::new(4).unwrap(), &mut backend)
        }))
        .is_err());
    } else {
        assert!(
            matches!(p.call_owned_process(5,&mut DispatchBudget::new(4).unwrap(),&mut backend),Err(RuntimeError::DestructiveEffect{effect,..}) if effect==release)
        );
    }
    // Assert before Done can latch its own disposal stage. Moving the Process
    // guard after the callback would leave ProcessInterrupted on unwind.
    assert_eq!(
        p.runtime().unwrap().stage,
        RuntimeStage::DisposalInterrupted
    );
    let fields = &p.runtime().unwrap().fields;
    assert_eq!(
        (fields.program_counter(), fields.last_pc, fields.elapsed),
        (1, 0, 5)
    );
    if release == RuntimeEffect::ReleaseActors {
        assert_eq!((fields.actor_count, fields.frame_count), (2, 0));
        assert_eq!(fields.actors.as_ref().unwrap().len(), 2);
        assert!(fields.frames.is_none());
    } else {
        assert_eq!((fields.actor_count, fields.frame_count), (0, 2));
        assert_eq!(fields.frames.as_ref().unwrap().len(), 2);
        assert!(fields.actors.is_none());
    }
    assert_eq!(backend.calls, 1);
    assert_eq!(backend.observable_frees, 1);
    let calls = backend.calls;
    assert!(p.done_fresh_runtime(&mut backend).is_err());
    assert!(p
        .call_owned_process(5, &mut DispatchBudget::new(4).unwrap(), &mut backend)
        .is_err());
    assert_eq!((backend.calls, backend.observable_frees), (calls, 1));
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.actor_slots + report.frame_slots, 2);
    assert_eq!((backend.calls, backend.observable_frees), (calls, 1));
    assert!(p.destruct(&mut memory).is_ok());
}

#[test]
fn act_setstate_and_addweapon_resolve_owners_and_registers() {
    use crate::original_program::RuntimeReferenceSlot;
    let mut memory = Memory::default();
    let mut p = program(
        b"act_setstate -1,inactive\nhuman_addweapon 0,9,7,100",
        &mut memory,
    );
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (2, 0));
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    // INACTIVE maps to ACTSTATE_1 on the owner.
    assert_eq!(
        memory
            .entity_world
            .act_states
            .get(&ActorReferenceId::from_existing(7)),
        Some(&1)
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    assert_eq!(
        memory
            .entity_world
            .weapons
            .get(&ActorReferenceId::from_existing(11)),
        Some(&vec![(9, 7, 100)])
    );
}
#[test]
fn act_setstate_in_range_empty_slot_resolves_to_no_actor() {
    use crate::original_program::RuntimeReferenceSlot;
    let mut memory = Memory::default();
    let mut p = program(b"act_setstate 0,active", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    // Slot 0 exists but is empty: the native overwrite with NULL applies,
    // so the owner must NOT be mutated.
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert!(memory.entity_world.act_states.is_empty());
    // An out-of-range register keeps the owner instead.
    let mut memory = Memory::default();
    let mut p = program(b"act_setstate 9,active", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        memory
            .entity_world
            .act_states
            .get(&ActorReferenceId::from_existing(7)),
        Some(&0)
    );
}
#[test]
fn weapon_id_truncates_to_u16_and_host_failures_propagate() {
    use crate::original_program::RuntimeReferenceSlot;
    let mut memory = Memory::default();
    let mut p = program(b"human_addweapon 0,-1,7,100", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        memory
            .entity_world
            .weapons
            .get(&ActorReferenceId::from_existing(11)),
        Some(&vec![(0xffff, 7, 100)])
    );
    // A failing world write propagates instead of advancing silently.
    memory.entity_world.fail_writes = true;
    let mut p = program(b"act_setstate -1,active", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("injected write failure"),
        "{error:?}"
    );
}
#[test]
fn quad_family_refuses_execution_and_reports_disposal() {
    let mut memory = Memory::default();
    let mut p = program(b"timeron 1,2,3,4", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (1, 0));
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("UnsupportedCommand(50)"),
        "{error:?}"
    );
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.quad_allocations, 1);
    assert_eq!(report.act_state_allocations, 0);
}
#[test]
fn event_marker_advances_like_label_and_counts_in_discard() {
    let mut memory = Memory::default();
    let mut p = program(b"event 60001\nevent death,7\nlabel after", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (3, 0));
    // Both events advance without effect, like LABEL.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.event_allocations, 2);
    assert_eq!(report.set_event_allocations, 0);
}
#[test]
fn setevent_resolves_register_and_jumps_on_event_busy() {
    use crate::original_program::RuntimeReferenceSlot;
    // Program: setevent 0,60001,retry -> dim 55 -> label retry -> dim 99
    // Success (ok true) advances to dim 55; busy (ok false) jumps to retry.
    let mut memory = Memory::default();
    let mut p = program(
        b"setevent 0,60001,retry\ndim_flt 1\nlabel retry\ndim_flt 2",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    // Success path: world returns true (no jump)
    memory
        .entity_world
        .set_event_results
        .insert((ActorReferenceId::from_existing(11), 60001), true);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        memory.entity_world.set_event_calls,
        vec![(ActorReferenceId::from_existing(11), 60001)]
    );
    // Reset to setevent and test busy path (jump)
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.command_time = 0;
    memory.entity_world.set_event_calls.clear();
    memory
        .entity_world
        .set_event_results
        .insert((ActorReferenceId::from_existing(11), 60001), false);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    // Jumped to label retry at index 2
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    // Missing actor (empty slot) advances without calling world
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    memory.entity_world.set_event_calls.clear();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert!(memory.entity_world.set_event_calls.is_empty());
    // Host failure propagates
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    memory.entity_world.set_event_fail = Some(ActorReferenceId::from_existing(11));
    let err = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{err:?}").contains("injected set-event failure"),
        "{err:?}"
    );
    memory.entity_world.set_event_fail = None;
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.set_event_allocations, 1);
}
#[test]
fn detector_setsignal_then_issignal_round_trip_jumps_by_signal() {
    use crate::original_program::RuntimeReferenceSlot;
    // 0: setsignal 0,1 | 1: issignal 0,hit1,miss1 | 2: label hit1
    // 3: setsignal 0,0 | 4: issignal 0,hit2,miss2 | 5: label miss1
    // 6: label hit2 | 7: label miss2
    let mut memory = Memory::default();
    let mut p = program(
        b"detector_setsignal 0,1\ndetector_issignal 0,hit1,miss1\nlabel hit1\ndetector_setsignal 0,0\ndetector_issignal 0,hit2,miss2\nlabel miss1\nlabel hit2\nlabel miss2",
        &mut memory,
    );
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (8, 0));
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    // SETSIGNAL stores p[1] != 0 through dispatch.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        memory.entity_world.detector_sets,
        vec![(ActorReferenceId::from_existing(11), true)]
    );
    // ISSIGNAL reads the set bit and jumps to label1.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    // LABEL advances past hit1 to the clearing store.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 3);
    // SETSIGNAL with 0 clears the bit through dispatch.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 4);
    assert_eq!(
        memory
            .entity_world
            .detector_signals
            .get(&ActorReferenceId::from_existing(11)),
        Some(&false)
    );
    // ISSIGNAL reads the cleared bit and jumps to label2 (miss2 at 7).
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 7);
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.detector_signal_allocations, 2);
}
#[test]
fn detector_issignal_empty_label_falls_through() {
    use crate::original_program::RuntimeReferenceSlot;
    // 0: setsignal 0,1 | 1: issignal 0,,miss | 2: setsignal 0,0 | 3: label miss.
    // The empty first label decodes (interior empty arguments count) and, like
    // NextCommand(NULL), advances instead of jumping.
    let mut memory = Memory::default();
    let mut p = program(
        b"detector_setsignal 0,1\ndetector_issignal 0,,miss\ndetector_setsignal 0,0\nlabel miss",
        &mut memory,
    );
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (4, 0));
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    // Signal reads set but label1 is empty: fall through to the next command.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    // Clear the bit, then reread: label2 still jumps.
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 3);
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 1;
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 3);
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.detector_signal_allocations, 1);
}
#[test]
fn detector_arms_resolve_owner_and_gate_non_detectors() {
    use crate::original_program::RuntimeReferenceSlot;
    // Negative registers keep the script's own actor for both arms.
    let mut memory = Memory::default();
    let mut p = program(
        b"detector_setsignal -1,5\ndetector_issignal -1,hit,miss\nlabel hit\nlabel miss",
        &mut memory,
    );
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        memory.entity_world.detector_sets,
        vec![(ActorReferenceId::from_existing(7), true)]
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    // An out-of-range register keeps the owner instead of the empty slot.
    let mut memory = Memory::default();
    let mut p = program(b"detector_setsignal 9,1", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        memory.entity_world.detector_sets,
        vec![(ActorReferenceId::from_existing(7), true)]
    );
    // A resolved non-detector advances both arms without effect or jump.
    let mut memory = Memory::default();
    let mut p = program(
        b"detector_setsignal 0,1\ndetector_issignal 0,hit,miss\nlabel hit\nlabel miss",
        &mut memory,
    );
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    memory
        .entity_world
        .non_detectors
        .insert(ActorReferenceId::from_existing(12));
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(12),
        )]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert!(memory.entity_world.detector_sets.is_empty());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    // An in-range empty slot resolves to no actor: the owner stays untouched.
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert!(memory.entity_world.detector_sets.is_empty());
    assert!(
        !memory
            .entity_world
            .detector_signals
            .contains_key(&ActorReferenceId::from_existing(7))
    );
    // A missing label advances like the empty label.
    let mut memory = Memory::default();
    let mut p = program(
        b"detector_setsignal 0,1\ndetector_issignal 0,nowhere1,nowhere2",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(11),
        )]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    // Host failures on either arm propagate instead of advancing silently.
    memory.entity_world.fail_detector = Some(ActorReferenceId::from_existing(11));
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("injected detector-signal failure"),
        "{error:?}"
    );
    // A failed call latches ProcessInterrupted; re-arm the host stage to
    // probe the second arm on the same program (test-harness stepping, like
    // the program_counter sets above).
    p.runtime.as_mut().unwrap().stage = RuntimeStage::Ready;
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 1;
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("injected detector-signal failure"),
        "{error:?}"
    );
}
#[test]
fn detector_inrange_counts_live_in_range_humans() {
    // Detector vtable, owner 7 with human-type mask 0b11, frame 100 at the
    // origin; range word 10 scans range² = 100.0 (C_detector.cpp:205-227).
    let mut memory = Memory::default();
    let mut p = program(b"dim_flt 1\ndetector_inrange 0, 10", &mut memory);
    p.set_detector_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    let world = &mut memory.entity_world;
    world
        .detector_types
        .insert(ActorReferenceId::from_existing(7), 0b11);
    world.actor_frames.insert(
        ActorReferenceId::from_existing(7),
        FrameReferenceId::from_existing(100),
    );
    world.frame_positions.insert(FrameReferenceId::from_existing(100), [0.0, 0.0, 0.0]);
    world.active_list = vec![
        Some(ActorReferenceId::from_existing(20)),
        None,
        Some(ActorReferenceId::from_existing(21)),
        Some(ActorReferenceId::from_existing(22)),
        Some(ActorReferenceId::from_existing(23)),
        Some(ActorReferenceId::from_existing(24)),
        Some(ActorReferenceId::from_existing(25)),
    ];
    // 20: ENTITY/PLAYER type with matching bit 0, distance 5 (3,4,0).
    world.scan_candidates.insert(
        ActorReferenceId::from_existing(20),
        Some((Some(FrameReferenceId::from_existing(200)), 0b01)),
    );
    world.frame_positions.insert(FrameReferenceId::from_existing(200), [3.0, 4.0, 0.0]);
    // 21: matching bit 1 exactly at range (10,0,0); the native `<=` counts it.
    world.scan_candidates.insert(
        ActorReferenceId::from_existing(21),
        Some((Some(FrameReferenceId::from_existing(201)), 0b10)),
    );
    world.frame_positions.insert(FrameReferenceId::from_existing(201), [10.0, 0.0, 0.0]);
    // 22: no shared bit with 0b11, skipped despite proximity.
    world.scan_candidates.insert(
        ActorReferenceId::from_existing(22),
        Some((Some(FrameReferenceId::from_existing(202)), 0b100)),
    );
    world.frame_positions.insert(FrameReferenceId::from_existing(202), [1.0, 0.0, 0.0]);
    // 23: native ENTITY/PLAYER type gate, skipped.
    world
        .scan_candidates
        .insert(ActorReferenceId::from_existing(23), None);
    // 24: frameless host-only actor, skipped like GETACTORSDIST.
    world.scan_candidates.insert(
        ActorReferenceId::from_existing(24),
        Some((None, 0b11)),
    );
    // 25: matching bits but out of range (11,0,0), skipped.
    world.scan_candidates.insert(
        ActorReferenceId::from_existing(25),
        Some((Some(FrameReferenceId::from_existing(203)), 0b11)),
    );
    world.frame_positions.insert(FrameReferenceId::from_existing(203), [11.0, 0.0, 0.0]);
    // Each call runs one instruction: dim_flt first, then the scan.
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let fields = &p.runtime().unwrap().fields;
    assert_eq!(fields.program_counter(), 2);
    assert_eq!(fields.floats.as_ref().unwrap()[0], 2.0);
    // NextCommand zeroed the clock, then the arm added this tick's delta.
    assert_eq!(fields.command_time, 5);
    // A refusing frame position stops the scan loudly instead of skipping.
    p.runtime.as_mut().unwrap().stage = RuntimeStage::Ready;
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 1;
    memory.entity_world.fail_frame_pos = Some(FrameReferenceId::from_existing(200));
    let error = p
        .call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("injected frame-pos failure"),
        "{error:?}"
    );
}
#[test]
fn detector_inrange_out_of_range_register_still_advances() {
    // The native register guard (`0 <= reg < num_float_regs`) skips the
    // zeroing and the scan but still takes NextCommand plus the tick's delta.
    let mut memory = Memory::default();
    let mut p = program(b"dim_flt 1\ndetector_inrange 9, 10", &mut memory);
    p.set_detector_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let fields = &p.runtime().unwrap().fields;
    assert_eq!(fields.program_counter(), 2);
    assert_eq!(fields.floats.as_ref().unwrap()[0], 0.0);
    assert_eq!(fields.command_time, 5);
    assert!(memory.entity_world.detector_types.is_empty());
}
#[test]
fn detector_inrange_ignores_base_programs() {
    // Opcode 29 in a base program keeps the Remaining no-op advance: no scan,
    // no float store, and the shared tail zeroes the clock.
    let mut memory = Memory::default();
    let mut p = program(b"dim_flt 1\ndetector_inrange 0, 10", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let fields = &p.runtime().unwrap().fields;
    assert_eq!(fields.program_counter(), 2);
    assert_eq!(fields.floats.as_ref().unwrap()[0], 0.0);
    assert_eq!(fields.command_time, 0);
}
#[test]
fn detector_storage_parks_with_actstate_none() {
    // No saved-code chain exists in fresh admission, so STORAGE always takes
    // the SetActState(ACTSTATE_NONE = 2) branch and stays on the command,
    // accumulating the tick's delta like the native `command_time += time`.
    let mut memory = Memory::default();
    let mut p = program(b"program_storage", &mut memory);
    p.set_detector_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(3, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    assert_eq!(
        memory.entity_world.act_states.get(&ActorReferenceId::from_existing(7)),
        Some(&2)
    );
    assert_eq!(p.runtime().unwrap().fields.command_time, 3);
    p.call_owned_process(4, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    assert_eq!(p.runtime().unwrap().fields.command_time, 7);
}
#[test]
fn detector_waitforhit_latches_then_defers_to_collisions() {
    let mut memory = Memory::default();
    let mut p = program(b"detector_waitforhit", &mut memory);
    p.set_detector_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    memory.entity_world.collision_flags.insert(
        ActorReferenceId::from_existing(7),
        DetectorCollisionFlags {
            wait_for_hit: false,
            dyn_colls: false,
        },
    );
    // Latch first: wait_for_hit stores, ACTSTATE_INACTIVE (1) stores, then the
    // missing static collisions refuse loudly; the command does not advance.
    let error = p
        .call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Lookup(LookupError::UnsupportedWorldBackend)
    );
    assert_eq!(
        memory.entity_world.collision_flags.get(&ActorReferenceId::from_existing(7)),
        Some(&DetectorCollisionFlags {
            wait_for_hit: true,
            dyn_colls: false,
        })
    );
    assert_eq!(
        memory.entity_world.act_states.get(&ActorReferenceId::from_existing(7)),
        Some(&1)
    );
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    // With dynamic collisions already running, the arm advances past the
    // command with no stores and no collision touch.
    p.runtime.as_mut().unwrap().stage = RuntimeStage::Ready;
    memory.entity_world.collision_flags.insert(
        ActorReferenceId::from_existing(7),
        DetectorCollisionFlags {
            wait_for_hit: false,
            dyn_colls: true,
        },
    );
    memory.entity_world.act_states.clear();
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(p.runtime().unwrap().fields.command_time, 5);
    assert!(memory.entity_world.act_states.is_empty());
    assert_eq!(
        memory.entity_world.collision_flags.get(&ActorReferenceId::from_existing(7)),
        Some(&DetectorCollisionFlags {
            wait_for_hit: false,
            dyn_colls: true,
        })
    );
}
#[test]
fn detector_create_and_erase_colls_latch_around_unavailable_collisions() {
    let mut memory = Memory::default();
    let mut p = program(
        b"detector_createdyncoll\ndetector_erasedyncoll",
        &mut memory,
    );
    p.set_detector_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    memory.entity_world.collision_flags.insert(
        ActorReferenceId::from_existing(7),
        DetectorCollisionFlags {
            wait_for_hit: false,
            dyn_colls: false,
        },
    );
    // CREATE latches dyn_colls, then the missing collision registry refuses.
    let error = p
        .call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Lookup(LookupError::UnsupportedWorldBackend)
    );
    assert_eq!(
        memory.entity_world.collision_flags.get(&ActorReferenceId::from_existing(7)),
        Some(&DetectorCollisionFlags {
            wait_for_hit: false,
            dyn_colls: true,
        })
    );
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    // ERASE clears the latched flag, then the missing registry refuses.
    p.runtime.as_mut().unwrap().stage = RuntimeStage::Ready;
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 1;
    let error = p
        .call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Lookup(LookupError::UnsupportedWorldBackend)
    );
    assert_eq!(
        memory.entity_world.collision_flags.get(&ActorReferenceId::from_existing(7)),
        Some(&DetectorCollisionFlags {
            wait_for_hit: false,
            dyn_colls: false,
        })
    );
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    // While waiting for a hit, CREATE advances without touching anything.
    p.runtime.as_mut().unwrap().stage = RuntimeStage::Ready;
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    memory.entity_world.collision_flags.insert(
        ActorReferenceId::from_existing(7),
        DetectorCollisionFlags {
            wait_for_hit: true,
            dyn_colls: false,
        },
    );
    p.call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(p.runtime().unwrap().fields.command_time, 5);
    assert_eq!(
        memory.entity_world.collision_flags.get(&ActorReferenceId::from_existing(7)),
        Some(&DetectorCollisionFlags {
            wait_for_hit: true,
            dyn_colls: false,
        })
    );
}
#[test]
fn detector_waitforuse_reaches_unavailable_use_object() {
    // WAITFORUSE always arms a new use object, so with no using-object owner
    // it refuses loudly and stays on the command.
    let mut memory = Memory::default();
    let mut p = program(b"detector_waitforuse 4, 0", &mut memory);
    p.set_detector_vtable();
    p.set_owner(Some(ActorReferenceId::from_existing(7)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    let error = p
        .call_owned_process(5, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert_eq!(
        error,
        RuntimeError::Lookup(LookupError::UnsupportedWorldBackend)
    );
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
}
#[test]
fn event_use_cb_toggles_top_flag_and_needs_stack() {
    let mut memory = Memory::default();
    // event_use_cb 1 sets GOSUB_EVENT_TIME (0x4000) on the top entry
    let mut p = program(b"event_use_cb 1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.gosub_stack = vec![1];
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.gosub_stack, vec![0x4001]);
    // event_use_cb 0 clears it
    let mut p2 = program(b"event_use_cb 0", &mut memory);
    p2.init_fresh_runtime(&mut memory).unwrap();
    p2.runtime.as_mut().unwrap().fields.gosub_stack = vec![0x4001];
    p2.runtime
        .as_mut()
        .unwrap()
        .fields
        .scheduler
        .program_counter = 0;
    p2.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p2.runtime().unwrap().fields.gosub_stack, vec![0x0001]);
    // Empty stack advances without effect
    let mut p3 = program(b"event_use_cb 1", &mut memory);
    p3.init_fresh_runtime(&mut memory).unwrap();
    p3.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p3.runtime().unwrap().fields.program_counter(), 1);
    assert!(p3.runtime().unwrap().fields.gosub_stack.is_empty());
}
#[test]
fn dialogue_commands_without_effect_owners_refuse_to_advance() {
    let cases: &[(&[u8], u32)] = &[
        (b"human_talk 0,100", 132),
        (b"dialog_begin 0,1", 78),
        (b"dialog_end", 79),
        (b"subtitle_add 5", 240),
    ];
    for &(source, opcode) in cases {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (1, 0));
        let effects_before = memory.events.len();
        let result = p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory);
        match result {
            Err(RuntimeError::Scheduler(message)) => {
                assert!(
                    message.contains(&format!("UnsupportedCommand({opcode})")),
                    "{message}"
                );
            }
            other => panic!("expected unsupported opcode {opcode}, got {other:?}"),
        }
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
        assert_eq!(memory.events.len(), effects_before);
        let report = p.discard_runtime_host().unwrap();
        assert_eq!(report.human_talk_allocations, usize::from(opcode == 132));
    }
}
#[test]
fn unimplemented_mission_flow_refuses_execution_without_advancing() {
    let cases: &[(&[u8], u32)] = &[
        (b"mission_objectives 5", 164),
        (b"mission_objectivesclear", 166),
        (b"endofmission 1,2", 37),
        (b"change_mission \"a\",\"b\",1.0", 52),
    ];
    for &(source, opcode) in cases {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (1, 0));
        let before = memory.events.len();
        // The public scheduler API wraps non-destructive VM errors.
        let result = p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory);
        match result {
            Err(RuntimeError::Scheduler(message)) => {
                assert!(
                    message.contains(&format!("UnsupportedCommand({opcode})")),
                    "{message}"
                );
            }
            other => panic!("expected unsupported opcode {opcode}, got {other:?}"),
        }
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
        assert_eq!(memory.events.len(), before);
        let report = p.discard_runtime_host().unwrap();
        assert_eq!(report.end_of_mission_allocations, usize::from(opcode == 37));
        assert_eq!(report.change_mission_allocations, usize::from(opcode == 52));
    }
}
#[test]
fn get_tick_and_game_time_write_float_regs() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_flt 2\ngetticktime 0\ngetgametime 1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    // First call with delta 7: game_time becomes 7, command_time 7
    p.call_owned_process(7, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.call_owned_process(7, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let floats = p.runtime().unwrap().fields.floats.as_ref().unwrap();
    assert_eq!(floats[0], 7.0);
    p.call_owned_process(7, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let floats = p.runtime().unwrap().fields.floats.as_ref().unwrap();
    assert_eq!(floats[1], 21.0);
    let before = p.runtime().unwrap().fields.game_time;
    p.call_owned_process(3, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    // At PC 3 (past end), run_scheduler returns immediately without dispatch, so game_time stays
    assert_eq!(p.runtime().unwrap().fields.game_time, before);
}
#[test]
fn try_set_event_on_program_jumps_and_pushes_event_stack() {
    let mut memory = Memory::default();
    // Program with two events, the last (index 1) should be found
    let mut p = program(b"event 60001\nevent 60001\nlabel after", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    // First SetEvent should succeed and jump to last EVENT (index 1)
    let ok = p.try_set_event(60001).unwrap();
    assert_eq!(ok, crate::original_program::SetEventOutcome::Started);
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        p.runtime().unwrap().fields.gosub_stack,
        vec![0x8000 | 0x4000]
    );
    assert_eq!(p.runtime().unwrap().fields.event_command_time, 0);
    assert_eq!(p.runtime().unwrap().fields.command_time, 0);
    // Already inside event -> second call should return false (jump to label)
    let ok2 = p.try_set_event(60001).unwrap();
    assert_eq!(ok2, crate::original_program::SetEventOutcome::Busy);
    // Missing event id -> true (no jump)
    let ok3 = p.try_set_event(99999).unwrap();
    assert_eq!(ok3, crate::original_program::SetEventOutcome::NoEvent);
    // Plain return should pop and restore
    p.runtime.as_mut().unwrap().fields.command_time = 7;
    p.runtime.as_mut().unwrap().fields.event_command_time = 42;
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    // The program at PC 1 is EVENT, which advances to 2 (label) on next tick
    // But we set PC to 1 and have stack with event, then return at PC 1? Actually
    // we need to test return: set PC to a return instruction
    let mut p2 = program(b"return", &mut memory);
    p2.init_fresh_runtime(&mut memory).unwrap();
    p2.runtime.as_mut().unwrap().fields.gosub_stack = vec![0x8000 | 0x4000 | 5];
    p2.runtime.as_mut().unwrap().fields.event_command_time = 11;
    p2.runtime.as_mut().unwrap().fields.command_time = 3;
    p2.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p2.runtime().unwrap().fields.program_counter(), 5);
    assert_eq!(p2.runtime().unwrap().fields.command_time, 12);
}
#[test]
fn return_handles_event_time_restoration_and_plain_pop() {
    let mut memory = Memory::default();
    // Event return with back_in_time set should restore event_command_time
    let mut p = program(b"return", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    {
        let fields = &mut p.runtime.as_mut().unwrap().fields;
        fields.gosub_stack = vec![0x8000 | 0x4000 | 5]; // event + time
        fields.event_command_time = 42;
        fields.command_time = 7;
        fields.scheduler.program_counter = 0;
    }
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 5);
    // With game_time, command_time = event_command_time + (game_time - event_time) = 42 + (1) = 43
    assert_eq!(p.runtime().unwrap().fields.command_time, 43);
    assert!(p.runtime().unwrap().fields.gosub_stack.is_empty());
    // Plain gosub return just jumps without time restore
    let mut p2 = program(b"return", &mut memory);
    p2.init_fresh_runtime(&mut memory).unwrap();
    {
        let fields = &mut p2.runtime.as_mut().unwrap().fields;
        fields.gosub_stack = vec![9];
        fields.command_time = 7;
        fields.event_command_time = 99;
        fields.scheduler.program_counter = 0;
    }
    p2.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p2.runtime().unwrap().fields.program_counter(), 9);
    assert_eq!(p2.runtime().unwrap().fields.command_time, 0);
}
#[test]
fn act_setstate_and_addweapon_advance_silently_without_resolution() {
    let mut memory = Memory::default();
    let mut p = program(
        b"act_setstate 5,active\nhuman_addweapon 5,9,7,100",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    // No registers and no owner: both advance without effect or error.
    for expected in 1..=2 {
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), expected);
    }
    assert!(memory.entity_world.act_states.is_empty());
    assert!(memory.entity_world.weapons.is_empty());
}

#[test]
fn failed_process_actor_release_is_uncertain_and_never_retried_by_done() {
    interrupted_process_release_blocks_done_and_execution(
        b"dim_act 2\ndim_act 4",
        RuntimeEffect::ReleaseActors,
        false,
    );
}
#[test]
fn failed_process_frame_release_is_uncertain_and_never_retried_by_done() {
    interrupted_process_release_blocks_done_and_execution(
        b"dim_frm 2\ndim_frm 4",
        RuntimeEffect::ReleaseFrames,
        false,
    );
}
#[test]
fn unwound_process_actor_release_is_uncertain_and_never_retried_by_done() {
    interrupted_process_release_blocks_done_and_execution(
        b"dim_act 2\ndim_act 4",
        RuntimeEffect::ReleaseActors,
        true,
    );
}
#[test]
fn unwound_process_frame_release_is_uncertain_and_never_retried_by_done() {
    interrupted_process_release_blocks_done_and_execution(
        b"dim_frm 2\ndim_frm 4",
        RuntimeEffect::ReleaseFrames,
        true,
    );
}
#[test]
fn nondestructive_process_failure_still_allows_done_cleanup() {
    let mut memory = Memory::default();
    // FRM_LINKTO stays unimplemented and stops dispatch without effects.
    let mut p = program(b"dim_act 2\nfrm_linkto 0, 1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .unwrap();
    assert!(p
        .call_owned_process(5, &mut DispatchBudget::new(4).unwrap(), &mut memory)
        .is_err());
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    memory.events.clear();
    p.done_fresh_runtime(&mut memory).unwrap();
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::Done);
    assert!(memory
        .events
        .iter()
        .any(|event| event.0 == RuntimeEffect::ReleaseActors));
    p.discard_runtime_host().unwrap();
}
#[test]
fn no_payload_instructions_compile_without_scalar_allocation_and_stop_at_dispatch_dependency() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\nuse_lightcache ignored, arguments",
        &mut memory,
    );
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (2, 0));
    assert!(p.runtime().unwrap().instructions.active()[1]
        .parameter
        .is_none());
    assert_eq!(
        memory
            .events
            .iter()
            .filter(|event| event.0 == RuntimeEffect::AllocateScalar)
            .count(),
        1
    );
    // Opcode 180 executes through the backend boundary; inject its failure
    // to keep the dispatch-dependency stop this test pins.
    memory.fail = Some(RuntimeEffect::UseLightCache);
    assert!(p
        .call_owned_process(7, &mut DispatchBudget::new(5).unwrap(), &mut memory)
        .unwrap_err()
        .to_debug()
        .contains("required boundary interrupted"));
    assert_eq!(p.runtime().unwrap().fields.elapsed, 7);
    p.done_fresh_runtime(&mut memory).unwrap();
    assert_eq!(
        memory
            .events
            .iter()
            .filter(|event| matches!(event.0, RuntimeEffect::ReleaseScalar(_)))
            .count(),
        1
    );
    p.discard_runtime_host().unwrap();
}
#[test]
fn compiled_end_keeps_scheduler_preclear_but_never_fakes_end_execution() {
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\nend", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    // The scheduler stops the loop; END itself stays on its instruction
    // without advancing and without saved-code/actor effects, so a repeated
    // call observes the same end instead of running past it.
    p.call_owned_process(9, &mut DispatchBudget::new(5).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 1);
    assert_eq!(f.last_pc, 1);
    assert!(!f.continuation());
    assert_eq!(f.elapsed, 0);
    p.call_owned_process(9, &mut DispatchBudget::new(5).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.done_fresh_runtime(&mut memory).unwrap();
    p.discard_runtime_host().unwrap();
}
#[test]
fn owned_subroutine_resets_actual_owner_and_pc_before_synchronous_delta_one_process() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_act 2", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(9)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    {
        let f = &mut p.runtime.as_mut().unwrap().fields;
        f.scheduler.program_counter = 37;
        f.last_pc = 19;
        f.elapsed = 55;
        f.scheduler.continuation = 0;
    }
    p.call_owned_subroutine(&mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(
        (f.program_counter(), f.last_pc, f.elapsed, f.actor_count),
        (1, 0, 0, 2)
    );
    assert!(f.continuation());
    assert_eq!(p.owner(), None);
    // Native CallSubroutine passes delta1 without resetting elapsed55.
    // DIM sees56 at allocation; common advance resets elapsed afterward.
    assert!(memory
        .events
        .iter()
        .any(|event| event.0 == RuntimeEffect::AllocateActors(2) && event.1 == 0 && event.2 == 56));
    p.done_fresh_runtime(&mut memory).unwrap();
    p.discard_runtime_host().unwrap();
}
#[test]
fn owned_subroutine_required_effect_failure_keeps_reset_prefix_and_delta_one_elapsed() {
    let mut memory = Memory::default();
    let mut p = program(b"use_lightcache", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(9)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 12;
    // Opcode 180 executes now; fail its backend effect to keep the failure
    // path (reset prefix, delta-one elapsed, cleared owner) this test pins.
    memory.fail = Some(RuntimeEffect::UseLightCache);
    assert!(p
        .call_owned_subroutine(&mut DispatchBudget::new(8).unwrap(), &mut memory)
        .is_err());
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 0);
    assert_eq!(f.elapsed, 1);
    assert!(f.continuation());
    assert_eq!(p.owner(), None);
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    p.done_fresh_runtime(&mut memory).unwrap();
    p.discard_runtime_host().unwrap();
}
impl RuntimeError {
    fn to_debug(&self) -> String {
        format!("{self:?}")
    }
}

#[test]
fn frm_seton_switches_register_frames_and_leaves_seton1_unsupported() {
    use crate::original_program::RuntimeReferenceSlot;
    // Empty slots advance without a world call.
    let mut memory = Memory::default();
    let mut empty = program(b"dim_frm 2\nfrm_seton 0, 1\nfrm_seton 1, 0", &mut memory);
    let progress = empty.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (3, 0));
    let instructions = empty.runtime().unwrap().instructions.active();
    assert_eq!(instructions[1].opcode, 14);
    assert_eq!(
        instructions[1]
            .parameter
            .as_ref()
            .unwrap()
            .pair_parameter()
            .unwrap()
            .values(),
        [Some(0), Some(1)]
    );
    assert_eq!(
        memory
            .events
            .iter()
            .filter(|event| event.0 == RuntimeEffect::AllocatePair)
            .count(),
        2
    );
    empty.call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    empty.call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    empty.call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(empty.runtime().unwrap().fields.program_counter(), 3);
    assert!(memory.entity_world.frame_switches.is_empty());
    empty.done_fresh_runtime(&mut memory).unwrap();
    empty.discard_runtime_host().unwrap();

    // Live slots switch through the world: the plain frame stores the core
    // state, the sound frame only records the deferred device switch.
    let mut memory = Memory::default();
    let mut live = program(b"dim_frm 2\nfrm_seton 0, 1\nfrm_seton 1, 0", &mut memory);
    live.init_fresh_runtime(&mut memory).unwrap();
    live
        .call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert!(memory.entity_world.frame_switches.is_empty());
    let plain = FrameReferenceId::from_existing(41);
    let sound = FrameReferenceId::from_existing(42);
    live.runtime.as_mut().unwrap().fields.frames = Some(
        vec![
            RuntimeReferenceSlot::Reference(plain),
            RuntimeReferenceSlot::Reference(sound),
        ]
        .into_boxed_slice(),
    );
    memory.entity_world.sound_frames.insert(sound);
    live.runtime.as_mut().unwrap().fields.scheduler.program_counter = 1;
    live.call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    live.call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        memory.entity_world.frame_switches,
        vec![(plain, true), (sound, false)]
    );
    assert_eq!(memory.entity_world.frame_states.get(&plain), Some(&true));
    assert!(!memory.entity_world.frame_states.contains_key(&sound));
    assert_eq!(live.runtime().unwrap().fields.program_counter(), 3);
    live.done_fresh_runtime(&mut memory).unwrap();
    live.discard_runtime_host().unwrap();

    // FRM_SETON1 decodes beside FRM_SETON but stays an explicit stop, with
    // reverse release order.
    let mut memory = Memory::default();
    let mut program = program(b"dim_frm 1\nfrm_seton1 2, 0", &mut memory);
    let progress = program.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (2, 0));
    assert_eq!(
        program.runtime().unwrap().instructions.active()[1]
            .parameter
            .as_ref()
            .unwrap()
            .pair_parameter()
            .unwrap()
            .values(),
        [Some(2), Some(0)]
    );
    let error = {
        program
            .call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        program
            .call_owned_process(9, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap_err()
    };
    assert!(
        format!("{error:?}").contains("UnsupportedCommand(15)"),
        "{error:?}"
    );
    assert_eq!(program.runtime().unwrap().fields.program_counter(), 1);
    memory.events.clear();
    program.done_fresh_runtime(&mut memory).unwrap();
    let releases: Vec<_> = memory
        .events
        .iter()
        .filter_map(|event| matches!(event.0, RuntimeEffect::ReleasePair(_)).then_some(event.0))
        .collect();
    assert_eq!(releases, [RuntimeEffect::ReleasePair(15)]);
    assert_eq!(program.runtime().unwrap().instructions.active().len(), 0);
    let report = program.discard_runtime_host().unwrap();
    assert_eq!(report.pair_allocations, 0);
}

#[test]
fn frm_seton_guard_paths_advance_silently_or_stop_explicitly() {
    use crate::original_program::RuntimeReferenceSlot;
    // Out-of-range and negative registers without an owner advance silently.
    for source in [b"frm_seton 5, 1".as_slice(), b"frm_seton -1, 1".as_slice()] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
        assert!(memory.entity_world.frame_switches.is_empty());
        p.discard_runtime_host().unwrap();
    }
    // Either form falls back to the script actor's frame when owned.
    for (source, on) in [
        (b"frm_seton 9, 0".as_slice(), false),
        (b"frm_seton -1, 1".as_slice(), true),
    ] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        let owner = ActorReferenceId::from_existing(7);
        let frame = FrameReferenceId::from_existing(43);
        p.set_owner(Some(owner)).unwrap();
        p.init_fresh_runtime(&mut memory).unwrap();
        memory.entity_world.actor_frames.insert(owner, frame);
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
        assert_eq!(memory.entity_world.frame_switches, vec![(frame, on)]);
        assert_eq!(memory.entity_world.frame_states.get(&frame), Some(&on));
        p.discard_runtime_host().unwrap();
    }
    // A stale owner stops dispatch instead of switching a wrong frame.
    let mut memory = Memory::default();
    let mut p = program(b"frm_seton 9, 1", &mut memory);
    p.set_owner(Some(ActorReferenceId::from_existing(8)))
        .unwrap();
    p.init_fresh_runtime(&mut memory).unwrap();
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("StaleActor"),
        "{error:?}"
    );
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    p.discard_runtime_host().unwrap();
    // A refusing world is a nondestructive stop: Done still runs.
    let mut memory = Memory::default();
    let mut p = program(b"dim_frm 1\nfrm_seton 0, 1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert!(memory.entity_world.frame_switches.is_empty());
    // Seed a live slot and rewind to the switch; DIM_FRM owns the first pass.
    p.runtime.as_mut().unwrap().fields.frames = Some(
        vec![RuntimeReferenceSlot::Reference(
            FrameReferenceId::from_existing(44),
        )]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 1;
    memory.entity_world.fail_frame_switch = true;
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("injected frame-switch failure"),
        "{error:?}"
    );
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    memory.entity_world.fail_frame_switch = false;
    p.done_fresh_runtime(&mut memory).unwrap();
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::Done);
    p.discard_runtime_host().unwrap();
}

#[test]
fn frm_seton_outcome_marks_core_and_deferred_device_effects() {
    let mut world = FakeEntityWorld::default();
    let plain = FrameReferenceId::from_existing(7);
    let sound = FrameReferenceId::from_existing(8);
    world.sound_frames.insert(sound);
    assert_eq!(
        world.set_frame_on(plain, true).unwrap(),
        FrameSetOnOutcome {
            core_applied: true,
            device_effects_deferred: true,
        }
    );
    assert_eq!(
        world.set_frame_on(sound, true).unwrap(),
        FrameSetOnOutcome {
            core_applied: false,
            device_effects_deferred: true,
        }
    );
    assert_eq!(world.frame_states.get(&plain), Some(&true));
    assert!(!world.frame_states.contains_key(&sound));
}

#[test]
fn block_begin_end_set_clear_and_nest_modes() {
    // Each program parks on a WAIT so one call observes the flags mid-script.
    for (source, accepted, in_block, mode) in [
        (&b"{\nwait 1000"[..], 2, true, BLOCK_MODE_NONE),
        (&b"{ UP\nwait 1000"[..], 2, true, BLOCK_MODE_UP),
        (&b"{ DOWN\nwait 1000"[..], 2, true, BLOCK_MODE_DOWN),
        (&b"{ sideways\nwait 1000"[..], 2, true, BLOCK_MODE_NONE),
        (&b"{ UP\n}\nwait 1000"[..], 3, false, BLOCK_MODE_NONE),
        (&b"}\nwait 1000"[..], 2, false, BLOCK_MODE_NONE),
        (&b"{ UP\n{ DOWN\nwait 1000"[..], 3, true, BLOCK_MODE_DOWN),
        (&b"{ DOWN\n{ UP\n}\nwait 1000"[..], 4, false, BLOCK_MODE_NONE),
    ] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!(
            (progress.accepted, progress.errors),
            (accepted, 0),
            "{source:?}"
        );
        // One owned-process call retires exactly one instruction (the
        // continuation starts cleared); step through the whole prefix so a
        // trailing WAIT parks mid-script and the flags are observable.
        for _ in 0..accepted {
            p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
                .unwrap();
        }
        let fields = &p.runtime().unwrap().fields;
        assert_eq!(
            (fields.flags.in_block, fields.flags.block_mode),
            (in_block, mode),
            "{source:?}"
        );
        p.discard_runtime_host().unwrap();
    }
    // The opener takes the allocation boundary and releases in reverse order.
    let mut memory = Memory::default();
    let mut p = program(b"{ UP\n}", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    assert!(memory
        .events
        .iter()
        .any(|event| event.0 == RuntimeEffect::AllocateBlock));
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert!(!p.runtime().unwrap().fields.flags.in_block);
    memory.events.clear();
    p.done_fresh_runtime(&mut memory).unwrap();
    assert!(memory
        .events
        .iter()
        .any(|event| event.0 == RuntimeEffect::ReleaseBlock(70)));
    let report = p.discard_runtime_host().unwrap();
    assert_eq!((report.block_allocations, report.ifflt_allocations), (0, 0));
    // A two-word opener rejects the instruction but keeps the rest admitted.
    let mut memory = Memory::default();
    let mut p = program(b"{ UP, DOWN\ndim_flt 4", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (1, 1));
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.float_count, 4);
    assert!(!p.runtime().unwrap().fields.flags.in_block);
    p.discard_runtime_host().unwrap();
}

#[test]
fn iffltinrange_jumps_only_inside_the_closed_range() {
    for (value, range, expected) in [
        ("5", "iffltinrange 0, 1.0, 10.0, hit", 5.0f32),
        ("1.0", "iffltinrange 0, 1.0, 10.0, hit", 1.0f32),
        ("10.0", "iffltinrange 0, 1.0, 10.0, hit", 10.0f32),
        ("0", "iffltinrange 0, 1.0, 10.0, hit", 99.0f32),
        ("11", "iffltinrange 0, 1.0, 10.0, hit", 99.0f32),
        ("nan", "iffltinrange 0, 1.0, 10.0, hit", 99.0f32),
        ("5", "iffltinrange 9, 1.0, 10.0, hit", 99.0f32),
        ("5", "iffltinrange -1, 1.0, 10.0, hit", 99.0f32),
        ("5", "iffltinrange 0, 1.0, 10.0, nowhere", 99.0f32),
        ("5", "iffltinrange 0, 10.0, 1.0, hit", 99.0f32),
    ] {
        let source = format!("dim_flt 1\nlet flt[0] = {value}\n{range}\nlet flt[0] = 99\nlabel hit");
        let mut memory = Memory::default();
        let mut p = program(source.as_bytes(), &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (5, 0), "{source:?}");
        // Single-step semantics: one call retires one instruction; the
        // conditional jump skips the overwritten store when taken.
        for _ in 0..6 {
            if p.runtime().unwrap().fields.program_counter() == 5 {
                break;
            }
            p.call_owned_process(1, &mut DispatchBudget::new(12).unwrap(), &mut memory)
                .unwrap();
        }
        let bank = p.runtime().unwrap().fields.floats.as_ref().unwrap().clone();
        assert_eq!(bank[0].to_bits(), expected.to_bits(), "{source:?}");
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 5);
        p.discard_runtime_host().unwrap();
    }
    // The range takes the allocation/conversion boundary and releases by opcode.
    let mut memory = Memory::default();
    let mut p = program(b"iffltinrange 0, 1.0, 10.0, hit", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    for effect in [
        RuntimeEffect::AllocateIfFlt,
        RuntimeEffect::ConvertIfFlt(0),
        RuntimeEffect::ConvertIfFlt(1),
        RuntimeEffect::ConvertIfFlt(2),
    ] {
        assert!(
            memory.events.iter().any(|event| event.0 == effect),
            "{effect:?}"
        );
    }
    memory.events.clear();
    p.done_fresh_runtime(&mut memory).unwrap();
    assert!(memory
        .events
        .iter()
        .any(|event| event.0 == RuntimeEffect::ReleaseIfFlt(20)));
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.ifflt_allocations, 0);
    // Bad shapes reject the instruction without becoming executable.
    for source in [
        b"iffltinrange 0, 1, 2".as_slice(),
        b"iffltinrange 0, 1, 2, 0123456789abcdef".as_slice(),
    ] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (0, 1), "{source:?}");
        p.discard_runtime_host().unwrap();
    }
}

#[test]
fn interrupted_second_pair_conversion_keeps_the_first_word_owned_until_host_discard() {
    let mut memory = Memory::default();
    let mut program = program(b"frm_seton 3, 1", &mut memory);
    memory.fail = Some(RuntimeEffect::ConvertPair(1));
    assert!(program.init_fresh_runtime(&mut memory).is_err());
    let runtime = program.runtime().unwrap();
    assert_eq!(runtime.stage, RuntimeStage::CompileInterrupted);
    let pending = runtime.pending.as_ref().unwrap();
    assert_eq!(pending.opcode, 14);
    assert_eq!(
        pending
            .parameter
            .as_ref()
            .unwrap()
            .pair_parameter()
            .unwrap()
            .values(),
        [Some(3), None]
    );
    assert!(runtime.working_source.is_some());
    let report = program.discard_runtime_host().unwrap();
    assert_eq!(report.pair_allocations, 1);
    assert_eq!(report.scalar_allocations, 0);
    assert_eq!(report.name_allocations, 0);
}

#[test]
fn goto_label_jumps_forward_and_skips_unreached_bank() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ngoto target\ndim_flt 99\nlabel target\ndim_flt 2\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 6);
    assert_eq!(f.float_count, 2);
    assert_eq!(f.floats.as_ref().unwrap().as_ref(), &[0.0f32, 0.0f32]);
}

#[test]
fn goto_missing_label_falls_through_and_backward_jump_loops() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ngoto nowhere\ndim_flt 5\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.float_count, 5);

    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\nlabel top\ngoto top\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    let result = p.call_owned_process(1, &mut DispatchBudget::new(3).unwrap(), &mut memory);
    assert!(result.is_err());
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
}

#[test]
fn goto_register_indirect_uses_truncated_bank_value() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 2\nlet flt[0] = 1\ngoto flt[0]\ndim_flt 7\nlabel 1\ndim_flt 3\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(20).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 8);
    assert_eq!(f.float_count, 3);
}

#[test]
fn label_search_prefers_last_match_case_insensitively() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\nlabel X\ngoto x\nlabel X\ndim_flt 8\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.float_count, 8);
}

#[test]
fn gosub_return_nests_and_empty_stack_advances() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ngosub sub\ndim_flt 9\nlabel sub\ndim_flt 4\nreturn\ndim_flt 5\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(20).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 8);
    // DIM only grows the bank (or frees it), so the later smaller sizes keep
    // the first nine zeroed slots; the count proves the re-entry happened.
    assert_eq!(f.float_count, 9);
    assert!(f.gosub_stack.is_empty());
    // The second bank allocation lands back on the gosub return target: the
    // program-counter trail proves the backward jump instead of a fallthrough.
    let allocations: Vec<u32> = memory
        .events
        .iter()
        .filter(|event| matches!(event.0, RuntimeEffect::AllocateFloats(_)))
        .map(|event| event.1)
        .collect();
    assert_eq!(allocations, vec![4, 2]);
}

#[test]
fn return_with_stop_flag_clears_and_stops_loop() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ngosub s\nlabel s\nreturn\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.flags.stop_at_return = true;
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 2);
    assert!(!f.continuation());
    assert!(!f.flags.stop_at_return);
}

#[test]
fn wait_holds_pc_across_calls_until_clock_reaches_threshold() {
    let mut memory = Memory::default();
    let mut p = program(b"wait 30", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(16, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    p.call_owned_process(16, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
}

#[test]
fn wait_register_operand_holds_then_releases() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 1\nlet flt[0] = 50\nwait flt[0]\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(16, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 5);
}

#[test]
fn jump_resets_command_clock_while_stay_preserves_it() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ngoto w\nlabel w\nwait 40\ndim_flt 3\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    // goto, label, then three held WAIT dispatches, the DIM and the closing
    // block: eight dispatches. A budget of four must exhaust instead. The
    // exhausted first call leaves the runtime interrupted, so the full run
    // uses a fresh program with the same source.
    let result = p.call_owned_process(16, &mut DispatchBudget::new(4).unwrap(), &mut memory);
    assert!(result.is_err());
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ngoto w\nlabel w\nwait 40\ndim_flt 3\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(16, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.float_count, 3);
}

#[test]
fn let_executes_all_operators_over_live_bank() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 5\nlet flt[0] = 6\nlet flt[1] = flt[0] + 4\nlet flt[2] = flt[1] - 20\nlet flt[3] = flt[1] * flt[2]\nlet flt[4] = flt[3] / flt[1]\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(20).unwrap(), &mut memory)
        .unwrap();
    let bank = p.runtime().unwrap().fields.floats.as_ref().unwrap().clone();
    assert_eq!(
        bank.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        [6.0f32, 10.0, -10.0, -100.0, -10.0]
            .iter()
            .map(|v| v.to_bits())
            .collect::<Vec<_>>()
    );
}

#[test]
fn let_division_guards_zero_and_mrg_zero_boundary() {
    for (source, expected) in [
        (
            &b"commandblock 1\ndim_flt 1\nlet flt[0] = 1 / 0\ncommandblock 0"[..],
            0.0f32,
        ),
        (
            &b"commandblock 1\ndim_flt 1\nlet flt[0] = 1 / 0.000000001\ncommandblock 0"[..],
            0.0f32,
        ),
        (
            &b"commandblock 1\ndim_flt 1\nlet flt[0] = 1 / 0.0000001\ncommandblock 0"[..],
            10000000.0f32,
        ),
    ] {
        let mut memory = Memory::default();
        let mut p = program(source, &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(
            p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
            expected.to_bits()
        );
    }
}

#[test]
fn let_guard_failure_and_bad_register_leave_bank_untouched() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 2\nlet flt[9] = 1\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 4);
    assert_eq!(f.floats.as_ref().unwrap().as_ref(), &[0.0f32, 0.0f32]);

    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 2\nlet flt[0] = flt[9] + 1\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 4);
    assert_eq!(
        p.runtime()
            .unwrap()
            .fields
            .floats
            .as_ref()
            .unwrap()
            .as_ref(),
        &[0.0f32, 0.0f32]
    );
}

#[test]
fn if_out_of_range_register_skips_without_reading_it() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 2\nif flt[9] = 0, yes, no\nlet flt[0] = 17\ncommandblock 0",
        &mut memory,
    );
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (5, 0));
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 5);
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0],
        17.0
    );
}

#[test]
fn if_branches_taken_not_taken_and_missing_labels() {
    let taken = b"commandblock 1\ndim_flt 2\nlet flt[0] = 5\nlet flt[1] = 5\nif flt[0] = flt[1], yes, no\nlabel no\nlet flt[0] = 222\ngoto done\nlabel yes\nlet flt[0] = 111\nlabel done\ncommandblock 0";
    let mut memory = Memory::default();
    let mut p = program(taken, &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(20).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        111.0f32.to_bits()
    );

    let not_taken = b"commandblock 1\ndim_flt 2\nlet flt[0] = 5\nlet flt[1] = 5\nif flt[0] < flt[1], yes, no\nlabel no\nlet flt[0] = 222\ngoto done\nlabel yes\nlet flt[0] = 111\nlabel done\ncommandblock 0";
    let mut memory = Memory::default();
    let mut p = program(not_taken, &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(20).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        222.0f32.to_bits()
    );

    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 1\nif flt[0] = 0, missing, also_missing\nlet flt[0] = 333\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        333.0f32.to_bits()
    );
}

#[test]
fn rnd_follows_msvc_sequence_range_rule_and_guards() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 3\nrnd 0, 100\nrnd 1, 1\nrnd 2, 0\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let bank = p.runtime().unwrap().fields.floats.as_ref().unwrap().clone();
    // LCG from seed 1 draws 41, 18467, 6334; 41 % 100 is 41.
    assert_eq!(bank[0].to_bits(), 0x42240000);
    assert_eq!(bank[1].to_bits(), 0x3f104721);
    assert_eq!(bank[2].to_bits(), 0x3e45f18c);
    assert_eq!(p.runtime().unwrap().fields.rand_state, 415139642);

    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 1\nrnd 9, 100\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime()
            .unwrap()
            .fields
            .floats
            .as_ref()
            .unwrap()
            .as_ref(),
        &[0.0f32]
    );

    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 1\nrnd -1, 5\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    assert!(p
        .call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .is_err());
}

#[test]
fn done_releases_float_bank_before_frames_and_actors() {
    let mut memory = Memory::default();
    let mut p = program(
        b"commandblock 1\ndim_flt 2\ndim_frm 1\ndim_act 1\ncommandblock 0",
        &mut memory,
    );
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    p.done_fresh_runtime(&mut memory).unwrap();
    let floats = memory
        .events
        .iter()
        .position(|event| event.0 == RuntimeEffect::ReleaseFloats)
        .unwrap();
    let frames = memory
        .events
        .iter()
        .position(|event| event.0 == RuntimeEffect::ReleaseFrames)
        .unwrap();
    let actors = memory
        .events
        .iter()
        .position(|event| event.0 == RuntimeEffect::ReleaseActors)
        .unwrap();
    assert!(floats < frames && frames < actors);
    let f = &p.runtime().unwrap().fields;
    assert_eq!((f.float_count, f.frame_count, f.actor_count), (0, 0, 0));
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.float_slots, 0);
    assert_eq!(report.flow_allocations, 0);
}

#[test]
fn game_nightmission_reports_nonzero_test_through_backend() {
    for (argument, expected) in [(1, true), (0, false), (-5, true)] {
        let mut memory = Memory::default();
        let source = format!("game_nightmission {argument}");
        let mut p = program(source.as_bytes(), &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (1, 0));
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
        assert!(
            memory
                .events
                .iter()
                .any(|event| event.0 == RuntimeEffect::SetNightMission(expected)),
            "missing SetNightMission({expected}) for argument {argument}"
        );
    }
}

#[test]
fn use_lightcache_reports_operandless_request_through_backend() {
    let mut memory = Memory::default();
    let mut p = program(b"use_lightcache", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (1, 0));
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert!(
        memory
            .events
            .iter()
            .any(|event| event.0 == RuntimeEffect::UseLightCache),
        "missing UseLightCache effect"
    );
}

#[test]
fn getactorsdist_measures_world_distance_and_dead_sign() {
    use crate::original_program::RuntimeReferenceSlot;
    // Hand-derived: A at [0,0,0], B at [3,4,0] gives 5.0; fourth word 0
    // skips the dead gate. One call retires one instruction.
    let mut memory = Memory::default();
    let mut p = program(b"getactorsdist 0,1,0", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (1, 0));
    let actor_a = ActorReferenceId::from_existing(11);
    let actor_b = ActorReferenceId::from_existing(12);
    let frame_a = FrameReferenceId::from_existing(21);
    let frame_b = FrameReferenceId::from_existing(22);
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(actor_a),
            RuntimeReferenceSlot::Reference(actor_b),
        ]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![0.0f32].into_boxed_slice());
    memory.entity_world.actor_frames.insert(actor_a, frame_a);
    memory.entity_world.actor_frames.insert(actor_b, frame_b);
    memory.entity_world.frame_positions.insert(frame_a, [0.0, 0.0, 0.0]);
    memory.entity_world.frame_positions.insert(frame_b, [3.0, 4.0, 0.0]);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        5.0f32.to_bits()
    );
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.getactorsdist_allocations, 1);

    // Dead gate: same point gives 0.0, clamped to 0.1 then negated to -0.1.
    let mut memory = Memory::default();
    let mut p = program(b"getactorsdist 0,1,0,1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(actor_a),
            RuntimeReferenceSlot::Reference(actor_b),
        ]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![0.0f32].into_boxed_slice());
    memory.entity_world.actor_frames.insert(actor_a, frame_a);
    memory.entity_world.actor_frames.insert(actor_b, frame_b);
    memory.entity_world.frame_positions.insert(frame_a, [1.0, 2.0, 2.0]);
    memory.entity_world.frame_positions.insert(frame_b, [1.0, 2.0, 2.0]);
    memory.entity_world.dead.insert(actor_b, true);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        (-0.1f32).to_bits()
    );

    // Alive second actor with the flag set keeps the positive distance.
    memory.entity_world.dead.insert(actor_b, false);
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![9.0f32].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        0.0f32.to_bits()
    );

    // Bad float register leaves the bank untouched and advances.
    let mut memory = Memory::default();
    let mut p = program(b"getactorsdist 0,1,9", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(actor_a),
            RuntimeReferenceSlot::Reference(actor_b),
        ]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![7.0f32].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        7.0f32.to_bits()
    );

    // Missing source actor stores 0.0 first and advances.
    let mut memory = Memory::default();
    let mut p = program(b"getactorsdist 0,1,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Empty,
            RuntimeReferenceSlot::Reference(actor_b),
        ]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![7.0f32].into_boxed_slice());
    memory.entity_world.actor_frames.insert(actor_b, frame_b);
    memory.entity_world.frame_positions.insert(frame_b, [3.0, 4.0, 0.0]);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        0.0f32.to_bits()
    );

    // Stale actor frame stops dispatch instead of inventing a distance.
    let mut memory = Memory::default();
    let mut p = program(b"getactorsdist 0,1,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let stale = ActorReferenceId::from_existing(99);
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(stale),
            RuntimeReferenceSlot::Reference(actor_b),
        ]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![0.0f32].into_boxed_slice());
    memory.entity_world.actor_frames.insert(actor_b, frame_b);
    memory.entity_world.frame_positions.insert(frame_b, [0.0, 0.0, 0.0]);
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(format!("{error:?}").contains("StaleActor"), "{error:?}");
    p.discard_runtime_host().unwrap();
}

#[test]
fn human_getowner_copies_live_car_and_gates_non_humans() {
    use crate::original_program::RuntimeReferenceSlot;
    // Human with a car writes the car; one call retires one instruction.
    let mut memory = Memory::default();
    let mut p = program(b"human_getowner 0,1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let human = ActorReferenceId::from_existing(11);
    let car = ActorReferenceId::from_existing(12);
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(human),
            RuntimeReferenceSlot::Empty,
        ]
        .into_boxed_slice(),
    );
    memory.entity_world.human_cars.insert(human, Some(car));
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        p.runtime().unwrap().fields.actors.as_ref().unwrap()[1],
        RuntimeReferenceSlot::Reference(car)
    );

    // Human with no car writes null (empty).
    memory.entity_world.human_cars.insert(human, None);
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(human),
            RuntimeReferenceSlot::Reference(car),
        ]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.actors.as_ref().unwrap()[1],
        RuntimeReferenceSlot::Empty
    );

    // Resolved non-human advances without touching the destination.
    let mut memory = Memory::default();
    let mut p = program(b"human_getowner 0,1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let other = ActorReferenceId::from_existing(13);
    let keep = ActorReferenceId::from_existing(14);
    p.runtime.as_mut().unwrap().fields.actor_count = 2;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Reference(other),
            RuntimeReferenceSlot::Reference(keep),
        ]
        .into_boxed_slice(),
    );
    memory.entity_world.non_humans.insert(other);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.actors.as_ref().unwrap()[1],
        RuntimeReferenceSlot::Reference(keep)
    );

    // Empty source leaves the destination untouched.
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![
            RuntimeReferenceSlot::Empty,
            RuntimeReferenceSlot::Reference(keep),
        ]
        .into_boxed_slice(),
    );
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.actors.as_ref().unwrap()[1],
        RuntimeReferenceSlot::Reference(keep)
    );

    // Out-of-range source breaks without a world call.
    let mut memory = Memory::default();
    let mut p = program(b"human_getowner 9,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    p.discard_runtime_host().unwrap();
}

#[test]
fn car_getspeed_scales_mps_to_kmh_and_gates_non_cars() {
    use crate::original_program::RuntimeReferenceSlot;
    // Hand-derived: 10.0 m/s * 3.6 = 36.0 km/h.
    let mut memory = Memory::default();
    let mut p = program(b"car_getspeed 0,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let car = ActorReferenceId::from_existing(11);
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(car)].into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![0.0f32].into_boxed_slice());
    memory.entity_world.car_speeds.insert(car, 10.0);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        36.0f32.to_bits()
    );

    // Resolved non-car leaves the float bank untouched.
    let mut memory = Memory::default();
    let mut p = program(b"car_getspeed 0,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let other = ActorReferenceId::from_existing(13);
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(other)].into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![7.0f32].into_boxed_slice());
    memory.entity_world.non_cars.insert(other);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        7.0f32.to_bits()
    );

    // Bad actor register breaks without a world call.
    p.runtime.as_mut().unwrap().fields.scheduler.program_counter = 0;
    p.runtime.as_mut().unwrap().fields.actors =
        Some(vec![RuntimeReferenceSlot::Empty].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);

    // Stale car stops dispatch.
    let mut memory = Memory::default();
    let mut p = program(b"car_getspeed 0,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(
            ActorReferenceId::from_existing(99),
        )]
        .into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![0.0f32].into_boxed_slice());
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(format!("{error:?}").contains("StaleActor"), "{error:?}");
    p.discard_runtime_host().unwrap();
}

#[test]
fn iscarusable_stores_stub_false_and_gates_non_cars() {
    use crate::original_program::RuntimeReferenceSlot;
    // The native body is a STUB returning false; a car stores 0.0.
    let mut memory = Memory::default();
    let mut p = program(b"iscarusable 0,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let car = ActorReferenceId::from_existing(11);
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(car)].into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![7.0f32].into_boxed_slice());
    memory.entity_world.car_speeds.insert(car, 0.0);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        0.0f32.to_bits()
    );

    // Resolved non-car leaves the bank untouched (native type gate).
    let mut memory = Memory::default();
    let mut p = program(b"iscarusable 0,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let other = ActorReferenceId::from_existing(13);
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(other)].into_boxed_slice(),
    );
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    p.runtime.as_mut().unwrap().fields.floats = Some(vec![7.0f32].into_boxed_slice());
    memory.entity_world.non_cars.insert(other);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_ref().unwrap()[0].to_bits(),
        7.0f32.to_bits()
    );
    p.discard_runtime_host().unwrap();
}

#[test]
fn car_breakmotor_and_forcestop_latch_on_car_owner() {
    use crate::original_program::RuntimeReferenceSlot;
    // BREAKMOTOR stores p[1] != 0 on the car.
    let mut memory = Memory::default();
    let mut p = program(b"car_breakmotor 0,1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let car = ActorReferenceId::from_existing(11);
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(car)].into_boxed_slice(),
    );
    memory.entity_world.car_speeds.insert(car, 0.0);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(memory.entity_world.break_motors.get(&car), Some(&true));

    // Clearing stores false through the same latch.
    let mut memory = Memory::default();
    let mut p = program(b"car_breakmotor 0,0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(car)].into_boxed_slice(),
    );
    memory.entity_world.car_speeds.insert(car, 0.0);
    memory.entity_world.break_motors.insert(car, true);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(memory.entity_world.break_motors.get(&car), Some(&false));

    // Non-car advances without storing.
    let mut memory = Memory::default();
    let mut p = program(b"car_breakmotor 0,1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let other = ActorReferenceId::from_existing(13);
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(other)].into_boxed_slice(),
    );
    memory.entity_world.non_cars.insert(other);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert!(memory.entity_world.break_motors.is_empty());

    // FORCESTOP latches Deactivate+Reset as a flag; physics stays deferred.
    let mut memory = Memory::default();
    let mut p = program(b"car_forcestop 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(car)].into_boxed_slice(),
    );
    memory.entity_world.car_speeds.insert(car, 5.0);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(memory.entity_world.force_stops, vec![car]);

    // Non-car forcestop advances without effect.
    let mut memory = Memory::default();
    let mut p = program(b"car_forcestop 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.actor_count = 1;
    p.runtime.as_mut().unwrap().fields.actors = Some(
        vec![RuntimeReferenceSlot::Reference(other)].into_boxed_slice(),
    );
    memory.entity_world.non_cars.insert(other);
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert!(memory.entity_world.force_stops.is_empty());
    p.discard_runtime_host().unwrap();
}

#[test]
fn player_lockcontrols_stores_bool_and_defers_breath() {
    // Hand-derived: LockControls(param != 0); Do_Breath is not run here.
    for (argument, expected) in [(1, true), (0, false), (-5, true)] {
        let mut memory = Memory::default();
        let source = format!("player_lockcontrols {argument}");
        let mut p = program(source.as_bytes(), &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (1, 0));
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
        assert_eq!(memory.entity_world.player_locked, Some(expected));
    }
    // A refusing world stops dispatch instead of faking the lock.
    let mut memory = Memory::default();
    let mut p = program(b"player_lockcontrols 1", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    memory.entity_world.fail_player_lock = true;
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("injected player-lock failure"),
        "{error:?}"
    );
    p.discard_runtime_host().unwrap();
}

#[test]
fn setcitytrafficvisible_and_noanimpreload_record_game_flags() {
    for (argument, expected) in [(1, true), (0, false), (-5, true)] {
        let mut memory = Memory::default();
        let source = format!("setcitytrafficvisible {argument}");
        let mut p = program(source.as_bytes(), &mut memory);
        let progress = p.init_fresh_runtime(&mut memory).unwrap();
        assert_eq!((progress.accepted, progress.errors), (1, 0));
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
        assert!(
            memory
                .events
                .iter()
                .any(|event| event.0 == RuntimeEffect::SetCityTrafficVisible(expected)),
            "missing SetCityTrafficVisible({expected}) for argument {argument}"
        );
    }
    let mut memory = Memory::default();
    let mut p = program(b"noanimpreload", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (1, 0));
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert!(
        memory
            .events
            .iter()
            .any(|event| event.0 == RuntimeEffect::DisableAnimPreload),
        "missing DisableAnimPreload effect"
    );
    p.discard_runtime_host().unwrap();
}

#[test]
fn rva_6ca80_get_float_reg_reads_bank_or_zero() {
    // GAME VA 0x46ca80 (RVA 0x6ca80): in-range reads the bank, anything else
    // reads 0.0.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_flt 2\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let fields = &p.runtime().unwrap().fields;
    assert_eq!(fields.get_float_reg(0), 0.0);
    assert_eq!(fields.get_float_reg(1), 0.0);
    assert_eq!(fields.get_float_reg(2), 0.0);
    assert_eq!(fields.get_float_reg(-1), 0.0);
    assert_eq!(fields.get_float_reg(i32::MAX), 0.0);
}

#[test]
fn rva_6cae0_set_float_reg_stores_in_range_only() {
    // GAME VA 0x46cae0 (RVA 0x6cae0): negative, out-of-range and missing-bank
    // stores are no-ops.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_flt 2\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    fields.set_float_reg(1, 2.5);
    assert_eq!(fields.get_float_reg(1), 2.5);
    fields.set_float_reg(2, 9.0);
    fields.set_float_reg(-1, 9.0);
    assert_eq!(fields.get_float_reg(0), 0.0);
    assert_eq!(fields.get_float_reg(2), 0.0);
    let mut bare = program(b"commandblock 1\ncommandblock 0", &mut memory);
    bare.init_fresh_runtime(&mut memory).unwrap();
    let bare_fields = &mut bare.runtime.as_mut().unwrap().fields;
    bare_fields.set_float_reg(0, 1.0);
    assert_eq!(bare_fields.get_float_reg(0), 0.0);
}

#[test]
fn rva_6caa0_get_frame_reg_reads_reference_or_none() {
    // GAME VA 0x46caa0 (RVA 0x6caa0): a stored reference reads back, empty
    // slots and out-of-range indexes read as None.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_frm 2\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let frame = FrameReferenceId::from_existing(11);
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    assert_eq!(fields.get_frame_reg(0), None);
    fields.set_frame_reg(0, Some(frame));
    assert_eq!(fields.get_frame_reg(0), Some(frame));
    assert_eq!(fields.get_frame_reg(1), None);
    assert_eq!(fields.get_frame_reg(2), None);
    assert_eq!(fields.get_frame_reg(-1), None);
}

#[test]
fn rva_6cb00_set_frame_reg_stores_reference_or_null() {
    // GAME VA 0x46cb00 (RVA 0x6cb00): None stores the empty (native null)
    // slot; out-of-range and missing-bank stores are no-ops.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_frm 1\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let frame = FrameReferenceId::from_existing(7);
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    fields.set_frame_reg(0, Some(frame));
    fields.set_frame_reg(0, None);
    assert_eq!(fields.get_frame_reg(0), None);
    fields.set_frame_reg(5, Some(frame));
    fields.set_frame_reg(-1, Some(frame));
    assert_eq!(fields.get_frame_reg(5), None);
}

#[test]
fn rva_6cac0_get_actor_reg_reads_reference_or_none() {
    // GAME VA 0x46cac0 (RVA 0x6cac0): a stored reference reads back, empty
    // slots and out-of-range indexes read as None.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_act 2\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let actor = ActorReferenceId::from_existing(11);
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    assert_eq!(fields.get_actor_reg(0), None);
    fields.set_actor_reg(0, Some(actor));
    assert_eq!(fields.get_actor_reg(0), Some(actor));
    assert_eq!(fields.get_actor_reg(1), None);
    assert_eq!(fields.get_actor_reg(2), None);
    assert_eq!(fields.get_actor_reg(-1), None);
}

#[test]
fn rva_6cb20_set_actor_reg_stores_reference_or_null() {
    // GAME VA 0x46cb20 (RVA 0x6cb20): None stores the empty (native null)
    // slot; out-of-range and missing-bank stores are no-ops.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_act 1\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let actor = ActorReferenceId::from_existing(13);
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    fields.set_actor_reg(0, Some(actor));
    fields.set_actor_reg(0, None);
    assert_eq!(fields.get_actor_reg(0), None);
    fields.set_actor_reg(4, Some(actor));
    fields.set_actor_reg(-1, Some(actor));
    assert_eq!(fields.get_actor_reg(4), None);
}

#[test]
fn rva_6cec0_invalidate_actor_reg_clears_matching_slots() {
    // GAME VA 0x46cec0 (RVA 0x6cec0): every register holding the actor goes
    // back to empty; other slots keep their contents.
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_act 3\ncommandblock 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .unwrap();
    let target = ActorReferenceId::from_existing(21);
    let other = ActorReferenceId::from_existing(22);
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    fields.set_actor_reg(0, Some(target));
    fields.set_actor_reg(1, Some(other));
    fields.set_actor_reg(2, Some(target));
    fields.invalidate_actor_reg(target);
    assert_eq!(fields.get_actor_reg(0), None);
    assert_eq!(fields.get_actor_reg(1), Some(other));
    assert_eq!(fields.get_actor_reg(2), None);
    fields.invalidate_actor_reg(ActorReferenceId::from_existing(99));
    assert_eq!(fields.get_actor_reg(1), Some(other));
}

#[test]
fn rva_6ce70_program_is_event_id_scans_back_to_front() {
    // GAME VA 0x46ce70 (RVA 0x6ce70): decoded EVENT ids report found with
    // their stored extra (0 for a parameterless `event <id>`, like the
    // native unconditional `*param = p[1]` write); anything else reports
    // not found.
    let mut memory = Memory::default();
    let mut p = program(b"event 60001\nevent 60002\nlabel after", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let runtime = p.runtime().unwrap();
    assert_eq!(program_is_event_id(&runtime.instructions, 60001), (true, Some(0)));
    assert_eq!(program_is_event_id(&runtime.instructions, 60002), (true, Some(0)));
    assert_eq!(program_is_event_id(&runtime.instructions, 7), (false, None));
}

#[test]
fn rva_6d380_program_find_actor_later_reports_unsupported() {
    // GAME VA 0x46d380 (RVA 0x6d380): the game actor-by-GUID owner stays
    // outside this contract.
    assert_eq!(FIND_ACTOR_LATER_ENTRY, 0x46d380);
    assert_eq!(
        program_find_actor_later(0, 11),
        Err(ProgramRegistryError::Unsupported {
            entry: FIND_ACTOR_LATER_ENTRY
        })
    );
}

#[test]
fn rva_6ccf0_program_clear_after_end_reports_unsupported() {
    // GAME VA 0x46ccf0 (RVA 0x6ccf0): the saved-code chain and the program
    // actor owners stay outside this contract.
    assert_eq!(CLEAR_AFTER_END_ENTRY, 0x46ccf0);
    assert_eq!(
        program_clear_after_end(),
        Err(ProgramRegistryError::Unsupported {
            entry: CLEAR_AFTER_END_ENTRY
        })
    );
}

#[test]
fn rva_6cef0_program_save_game_size_reports_unsupported() {
    // GAME VA 0x46cef0 (RVA 0x6cef0): the original only logs; the save-chunk
    // owner stays outside this contract.
    assert_eq!(PROGRAM_SAVE_GAME_SIZE_ENTRY, 0x46cef0);
    assert_eq!(
        program_save_game_size(),
        Err(ProgramRegistryError::Unsupported {
            entry: PROGRAM_SAVE_GAME_SIZE_ENTRY
        })
    );
}

#[test]
fn rva_6cf70_program_save_game_save_reports_unsupported() {
    // GAME VA 0x46cf70 (RVA 0x6cf70): the original only logs; the save-chunk
    // owner stays outside this contract.
    assert_eq!(PROGRAM_SAVE_GAME_SAVE_ENTRY, 0x46cf70);
    assert_eq!(
        program_save_game_save(),
        Err(ProgramRegistryError::Unsupported {
            entry: PROGRAM_SAVE_GAME_SAVE_ENTRY
        })
    );
}

#[test]
fn rva_6d100_program_save_game_load_reports_unsupported() {
    // GAME VA 0x46d100 (RVA 0x6d100): the original only logs; the save-chunk
    // owner stays outside this contract.
    assert_eq!(PROGRAM_SAVE_GAME_LOAD_ENTRY, 0x46d100);
    assert_eq!(
        program_save_game_load(),
        Err(ProgramRegistryError::Unsupported {
            entry: PROGRAM_SAVE_GAME_LOAD_ENTRY
        })
    );
}

#[test]
fn rva_7b010_program_set_ai_priority_stores_word() {
    // GAME VA 0x47b010 (RVA 0x7b010): the caller-owned priority word store.
    let mut slot = 0.0f32;
    program_set_ai_priority(&mut slot, 2.5);
    assert_eq!(slot, 2.5);
    program_set_ai_priority(&mut slot, -1.0);
    assert_eq!(slot, -1.0);
}

#[path = "program_flow_register_guards_tests.rs"]
mod flow_register_guards_tests;
