use super::*;
#[derive(Default)]
struct Memory {
    next: u64,
    events: Vec<(RuntimeEffect, u32, u32, u32, u32)>,
    fail: Option<RuntimeEffect>,
    redirect_release: bool,
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
fn decoded_unsupported_dispatch_keeps_reached_elapsed_prefix() {
    let mut m = Memory::default();
    let mut p = program(b"commandblock 1\ndim_flt 2\ncommandblock 0", &mut m);
    p.init_fresh_runtime(&mut m).unwrap();
    let mut budget = DispatchBudget::new(10).unwrap();
    assert!(p
        .call_owned_process(9, &mut budget, &mut m)
        .unwrap_err()
        .to_debug()
        .contains("UnsupportedCommand(7)"));
    let f = &p.runtime().unwrap().fields;
    assert_eq!(
        (f.scheduler.program_counter, f.last_pc, f.elapsed),
        (1, 0, 9)
    );
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
    for vtable in [ProgramVtable::Entity, ProgramVtable::Detector] {
        let mut memory = Memory::default();
        let mut p = program(b"dim_act 2", &mut memory);
        match vtable {
            ProgramVtable::Entity => p.set_entity_vtable(),
            ProgramVtable::Detector => p.set_detector_vtable(),
            ProgramVtable::Base => unreachable!(),
        }
        let before = p.source().unwrap().bytes.clone();
        memory.events.clear();
        assert_eq!(
            p.init_fresh_runtime(&mut memory),
            Err(RuntimeError::UnsupportedDerivedRuntime(vtable))
        );
        assert!(p.runtime().is_none());
        assert!(!p.runtime_started);
        assert_eq!(p.vtable(), vtable);
        assert_eq!(p.source().unwrap().bytes, before);
        assert!(memory.events.is_empty());
        p.set_source_code(Some(b"dim_frm 3"), &mut memory).unwrap();
        assert_eq!(p.source().unwrap().c_bytes(), Some(b"dim_frm 3".as_slice()));
        p.destruct(&mut memory).unwrap();
        assert_eq!(p.phase(), ProgramPhase::Dead);
    }
}
fn interrupted_process_release_blocks_done_and_execution(
    source: &[u8],
    release: RuntimeEffect,
) {
    struct UncertainRelease {
        release: RuntimeEffect,
        calls: usize,
        observable_frees: usize,
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
    };
    assert!(
        matches!(p.call_owned_process(5,&mut DispatchBudget::new(4).unwrap(),&mut backend),Err(RuntimeError::DestructiveEffect{effect,..}) if effect==release)
    );
    assert_eq!(
        p.runtime().unwrap().stage,
        RuntimeStage::DisposalInterrupted
    );
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
fn failed_process_actor_release_is_uncertain_and_never_retried_by_done() {
    interrupted_process_release_blocks_done_and_execution(
        b"dim_act 2\ndim_act 4",
        RuntimeEffect::ReleaseActors,
    );
}
#[test]
fn failed_process_frame_release_is_uncertain_and_never_retried_by_done() {
    interrupted_process_release_blocks_done_and_execution(
        b"dim_frm 2\ndim_frm 4",
        RuntimeEffect::ReleaseFrames,
    );
}
#[test]
fn nondestructive_process_failure_still_allows_done_cleanup() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_act 2\ndim_flt 3", &mut memory);
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
    assert!(p
        .call_owned_process(9, &mut DispatchBudget::new(5).unwrap(), &mut memory)
        .unwrap_err()
        .to_debug()
        .contains("UnsupportedCommand(6)"));
    let f = &p.runtime().unwrap().fields;
    assert_eq!(f.program_counter(), 1);
    assert!(!f.continuation());
    assert_eq!(f.elapsed, 9);
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
        .any(|event| event.0 == RuntimeEffect::AllocateActors(2)
            && event.1 == 0
            && event.2 == 56));
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
