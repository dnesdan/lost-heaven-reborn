use super::*;
use crate::entity_actor::EntityProcessState;
use crate::script_lookup::{LookupError, LookupWorldBackend};
use std::collections::HashMap;
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
fn derived_program_source_lifetime_survives_explicit_base_runtime_admission_rejection() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_act 2", &mut memory);
    p.set_detector_vtable();
    let before = p.source().unwrap().bytes.clone();
    memory.events.clear();
    assert_eq!(
        p.init_fresh_runtime(&mut memory),
        Err(RuntimeError::UnsupportedDerivedRuntime(
            ProgramVtable::Detector
        ))
    );
    assert!(p.runtime().is_none());
    assert!(!p.runtime_started);
    assert_eq!(p.vtable(), ProgramVtable::Detector);
    assert_eq!(p.source().unwrap().bytes, before);
    assert!(memory.events.is_empty());
    p.set_source_code(Some(b"dim_frm 3"), &mut memory).unwrap();
    assert_eq!(p.source().unwrap().c_bytes(), Some(b"dim_frm 3".as_slice()));
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
fn dialogue_commands_advance_as_noop_and_count_in_discard() {
    let mut memory = Memory::default();
    let mut p = program(
        b"human_talk 0,100\ndialog_begin 0,1\ndialog_end\nsubtitle_add 5",
        &mut memory,
    );
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (4, 0));
    for expected in 1..=4 {
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), expected);
    }
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.human_talk_allocations, 1);
}
#[test]
fn mission_flow_commands_advance_as_noop_and_count_in_discard() {
    let mut memory = Memory::default();
    let mut p = program(b"mission_objectives 5\nmission_objectivesclear\nendofmission 1,2\nchange_mission \"a\",\"b\",1.0", &mut memory);
    let progress = p.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (4, 0));
    for expected in 1..=4 {
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.program_counter(), expected);
    }
    let report = p.discard_runtime_host().unwrap();
    assert_eq!(report.end_of_mission_allocations, 1);
    assert_eq!(report.change_mission_allocations, 1);
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
    let mut p = program(b"dim_act 2\nfrm_seton 0, 1", &mut memory);
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
    assert!(p
        .call_owned_process(7, &mut DispatchBudget::new(5).unwrap(), &mut memory)
        .unwrap_err()
        .to_debug()
        .contains("UnsupportedCommand(180)"));
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
fn original_frame_visibility_parameters_compile_on_the_owned_runtime_but_do_not_fake_game_effects()
{
    let mut memory = Memory::default();
    let mut program = program(
        b"commandblock 1\nfrm_seton 0, 1\nfrm_seton1 2, 0\ncommandblock 0",
        &mut memory,
    );
    let progress = program.init_fresh_runtime(&mut memory).unwrap();
    assert_eq!((progress.accepted, progress.errors), (4, 0));
    let instructions = program.runtime().unwrap().instructions.active();
    assert_eq!(instructions[1].opcode, 14);
    assert_eq!(instructions[2].opcode, 15);
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
        instructions[2]
            .parameter
            .as_ref()
            .unwrap()
            .pair_parameter()
            .unwrap()
            .values(),
        [Some(2), Some(0)]
    );
    assert_eq!(
        memory
            .events
            .iter()
            .filter(|event| event.0 == RuntimeEffect::AllocatePair)
            .count(),
        2
    );

    let mut budget = DispatchBudget::new(8).unwrap();
    assert!(program
        .call_owned_process(9, &mut budget, &mut memory)
        .unwrap_err()
        .to_debug()
        .contains("UnsupportedCommand(14)"));
    let fields = &program.runtime().unwrap().fields;
    assert_eq!((fields.program_counter(), fields.elapsed), (1, 9));

    memory.events.clear();
    program.done_fresh_runtime(&mut memory).unwrap();
    let releases: Vec<_> = memory
        .events
        .iter()
        .filter_map(|event| matches!(event.0, RuntimeEffect::ReleasePair(_)).then_some(event.0))
        .collect();
    assert_eq!(
        releases,
        [
            RuntimeEffect::ReleasePair(15),
            RuntimeEffect::ReleasePair(14)
        ]
    );
    assert_eq!(program.runtime().unwrap().instructions.active().len(), 0);
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
    assert!(p
        .call_owned_process(1, &mut DispatchBudget::new(10).unwrap(), &mut memory)
        .is_err());
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
