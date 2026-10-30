//! Actual CProgram compilation/lifetime, not a second compiler or VM.
use super::*;

#[derive(Default)]
struct Probe {
    next: u64,
    events: Vec<RuntimeEffect>,
    names: Vec<(RuntimeEffect, Option<i32>, Option<Vec<u8>>)>,
    fail: Option<RuntimeEffect>,
}
impl ProgramBackend for Probe {
    fn allocate(&mut self, _: &CProgram, _: ProgramBuffer, _: u32) -> Result<ProgramAllocation, String> {
        self.next += 1;
        Ok(ProgramAllocation(self.next))
    }
    fn free(&mut self, _: &CProgram, _: ProgramBuffer, _: Option<ProgramAllocation>) -> Result<(), String> {
        Ok(())
    }
}
impl RuntimeBackend for Probe {
    fn effect(&mut self, _: RuntimeView<'_>, effect: RuntimeEffect, _: Option<&ScalarParameter>) -> Result<(), String> {
        self.events.push(effect);
        if self.fail == Some(effect) {
            return Err("injected required effect failure".into());
        }
        Ok(())
    }
    fn name_parameter_effect(&mut self, fields: RuntimeView<'_>, effect: RuntimeEffect, p: Option<&NameParameter>) -> Result<(), String> {
        self.names.push((effect, p.and_then(NameParameter::register), p.and_then(NameParameter::name).map(<[u8]>::to_vec)));
        if effect == RuntimeEffect::ConvertNameRegister {
            assert!(p.is_some());
            assert_eq!((p.unwrap().register(), p.unwrap().name()), (None, None));
            // The effect borrows the actual runtime fields, not a detached copy.
            *fields.last_pc = 123;
        }
        self.effect(fields, effect, None)
    }
}
fn program(source: &[u8], backend: &mut Probe) -> CProgram {
    let mut program = CProgram::construct(ProgramSeed {
        address_high_byte: 0x81,
        padding_0d: [5; 3],
        words_1c_2c: [0xdeadbeef; 5],
        padding_31: [7; 3],
        word_40: 0x12345678,
        bytes_64: [0xff; 4],
    });
    program.set_source_code(Some(source), backend).unwrap();
    program
}
fn name(program: &CProgram, index: usize) -> &NameParameter {
    program.runtime().unwrap().instructions.active()[index]
        .parameter.as_ref().unwrap().name_parameter().unwrap()
}
fn cleanup(program: &mut CProgram, backend: &mut Probe) {
    program.done_fresh_runtime(backend).unwrap();
    let report = program.discard_runtime_host().unwrap();
    assert_eq!((report.scalar_allocations, report.name_allocations), (0, 0));
    program.destruct(backend).unwrap();
    assert_eq!(program.phase(), ProgramPhase::Dead);
}

#[test]
fn actual_compiler_owns_all_three_named_payloads_and_preserves_source() {
    let mut backend = Probe::default();
    let source = b"findframe 0,\"frame_A\"\nfindactor -2,\"Person\"\nrecaddactor 3,\"record\"";
    let mut p = program(source, &mut backend);
    let before = p.source().unwrap().bytes.clone();
    let progress = p.init_fresh_runtime(&mut backend).unwrap();
    assert_eq!((progress.accepted, progress.errors), (3, 0));
    assert_eq!(p.source().unwrap().bytes, before);
    assert!(p.runtime().unwrap().working_source.is_none());
    assert_eq!(p.runtime().unwrap().fields.last_pc, 123);
    for (index, opcode, reg, bytes) in [
        (0, 13, 0, b"frame_A".as_slice()),
        (1, 18, -2, b"Person".as_slice()),
        (2, 43, 3, b"record".as_slice()),
    ] {
        assert_eq!(p.runtime().unwrap().instructions.active()[index].opcode, opcode);
        assert_eq!((name(&p, index).register(), name(&p, index).name()), (Some(reg), Some(bytes)));
        assert!(p.runtime().unwrap().instructions.active()[index].parameter.as_ref().unwrap().parameter().is_none());
    }
    assert_eq!(backend.events.iter().filter(|&&e| e == RuntimeEffect::AllocateName).count(), 3);
    assert!(!backend.events.contains(&RuntimeEffect::AllocateScalar));
    cleanup(&mut p, &mut backend);
}

#[test]
fn omitted_and_explicit_empty_names_compile_as_valid_empty_payloads() {
    let mut backend = Probe::default();
    let mut p = program(b"findframe 0\nfindactor 1,\"\"\nrecaddactor 2,", &mut backend);
    assert_eq!(p.init_fresh_runtime(&mut backend).unwrap().accepted, 3);
    for index in 0..3 {
        assert_eq!(name(&p, index).register(), Some(index as i32));
        assert_eq!(name(&p, index).name(), Some(b"".as_slice()));
    }
    cleanup(&mut p, &mut backend);
}

#[test]
fn native_count_and_quote_rejections_continue_without_publishing_freed_payloads() {
    let mut backend = Probe::default();
    let mut p = program(b"findframe 1,\"a,b\"\nfindactor 2,unquoted\nrecaddactor 3,\"ok\"", &mut backend);
    let progress = p.init_fresh_runtime(&mut backend).unwrap();
    assert_eq!((progress.accepted, progress.errors), (1, 2));
    assert_eq!(p.runtime().unwrap().instructions.active()[0].opcode, 43);
    assert!(p.runtime().unwrap().pending.is_none());
    assert_eq!(backend.events.iter().filter(|&&e| e == RuntimeEffect::AllocateName).count(), 2);
    assert!(backend.names.contains(&(RuntimeEffect::ReleaseName(18), Some(2), None)));
    cleanup(&mut p, &mut backend);
    assert_eq!(backend.events.iter().filter(|&&e| e == RuntimeEffect::ReleaseName(18)).count(), 1);
    assert_eq!(backend.events.iter().filter(|&&e| e == RuntimeEffect::ReleaseName(43)).count(), 1);
    assert!(!backend.events.contains(&RuntimeEffect::ReleaseName(13)));
}

#[test]
fn oversized_name_keeps_allocated_pending_prefix_and_requires_host_discard() {
    let mut backend = Probe::default();
    let source = format!("dim_frm 1\nfindframe 7,\"{}\"\ndim_act 3", "x".repeat(64));
    let mut p = program(source.as_bytes(), &mut backend);
    assert!(format!("{:?}", p.init_fresh_runtime(&mut backend).unwrap_err()).contains("NameTooLong"));
    let runtime = p.runtime().unwrap();
    assert_eq!(runtime.stage, RuntimeStage::CompileInterrupted);
    assert_eq!(runtime.instructions.active().len(), 1);
    let pending = runtime.pending.as_ref().unwrap().parameter.as_ref().unwrap().name_parameter().unwrap();
    assert_eq!((pending.register(), pending.name()), (Some(7), None));
    assert!(runtime.working_source.is_some());
    assert!(p.done_fresh_runtime(&mut backend).is_err());
    let report = p.discard_runtime_host().unwrap();
    assert_eq!((report.scalar_allocations, report.name_allocations), (1, 1));
    assert!(report.working_source_bytes > 0);
    assert!(!backend.events.contains(&RuntimeEffect::ReleaseName(13)));
    p.destruct(&mut backend).unwrap();
}

#[test]
fn name_decoding_never_pretends_to_execute_lookup_or_record_binding() {
    for (source, opcode) in [(b"findframe 0,\"x\"".as_slice(), 13), (b"findactor 0,\"x\"".as_slice(), 18), (b"recaddactor 0,\"x\"".as_slice(), 43)] {
        let mut backend = Probe::default();
        let mut p = program(source, &mut backend);
        p.init_fresh_runtime(&mut backend).unwrap();
        let error = p.call_owned_process(7, &mut DispatchBudget::new(4).unwrap(), &mut backend).unwrap_err();
        assert!(format!("{error:?}").contains(&format!("UnsupportedCommand({opcode})")));
        let runtime = p.runtime().unwrap();
        assert_eq!(runtime.stage, RuntimeStage::ProcessInterrupted);
        assert_eq!((runtime.fields.program_counter(), runtime.fields.elapsed), (0, 7));
        assert!(runtime.fields.actors.is_none());
        assert!(runtime.fields.frames.is_none());
        cleanup(&mut p, &mut backend);
    }
}

#[test]
fn mixed_payloads_release_once_in_reverse_instruction_order() {
    let mut backend = Probe::default();
    let mut p = program(b"findframe 0,\"x\"\nfindactor 1\ncommandblock 0\nrecaddactor 2\ndim_frm 3", &mut backend);
    p.init_fresh_runtime(&mut backend).unwrap();
    backend.events.clear();
    cleanup(&mut p, &mut backend);
    assert_eq!(backend.events, [RuntimeEffect::ReleaseScalar(11), RuntimeEffect::ReleaseName(43), RuntimeEffect::ReleaseScalar(34), RuntimeEffect::ReleaseName(18), RuntimeEffect::ReleaseName(13)]);
}

#[test]
fn allocation_conversion_and_record_growth_failures_retain_the_owned_pending_payload() {
    for failure in [RuntimeEffect::AllocateName, RuntimeEffect::ConvertNameRegister, RuntimeEffect::AllocateRecords(2)] {
        let mut backend = Probe { fail: Some(failure), ..Probe::default() };
        let mut p = program(b"dim_frm 1\nfindframe 2,\"x\"", &mut backend);
        assert!(p.init_fresh_runtime(&mut backend).is_err());
        let runtime = p.runtime().unwrap();
        assert_eq!(runtime.stage, RuntimeStage::CompileInterrupted);
        assert_eq!(runtime.instructions.active().len(), 1);
        let pending = runtime.pending.as_ref().unwrap().parameter.as_ref().unwrap().name_parameter();
        assert_eq!(pending.is_some(), failure != RuntimeEffect::AllocateName);
        if failure == RuntimeEffect::ConvertNameRegister {
            assert_eq!(pending.unwrap().register(), None);
            assert_eq!(runtime.fields.last_pc, 123);
        }
        if failure == RuntimeEffect::AllocateRecords(2) {
            assert_eq!(pending.unwrap().name(), Some(b"x".as_slice()));
        }
        let calls = backend.events.len();
        assert!(p.init_fresh_runtime(&mut backend).is_err());
        assert!(p.done_fresh_runtime(&mut backend).is_err());
        assert_eq!(backend.events.len(), calls);
        let report = p.discard_runtime_host().unwrap();
        assert_eq!(report.scalar_allocations, 1);
        assert_eq!(report.name_allocations, usize::from(failure != RuntimeEffect::AllocateName));
        p.destruct(&mut backend).unwrap();
    }
}

#[test]
fn interrupted_quote_rejection_never_repeats_its_release_callback() {
    let mut backend = Probe { fail: Some(RuntimeEffect::ReleaseName(13)), ..Probe::default() };
    let mut p = program(b"findframe 4,invalid\ndim_act 3", &mut backend);
    assert!(p.init_fresh_runtime(&mut backend).is_err());
    assert!(p.runtime().unwrap().instructions.active().is_empty());
    assert_eq!(p.runtime().unwrap().pending.as_ref().unwrap().opcode, 13);
    let calls = backend.events.len();
    assert!(p.init_fresh_runtime(&mut backend).is_err());
    assert!(p.done_fresh_runtime(&mut backend).is_err());
    assert_eq!(backend.events.len(), calls);
    assert_eq!(p.discard_runtime_host().unwrap().name_allocations, 1);
    assert_eq!(backend.events.len(), calls);
    p.destruct(&mut backend).unwrap();
}

#[test]
fn failed_done_keeps_completed_reverse_prefix_and_blocks_another_name_free() {
    let mut backend = Probe::default();
    let mut p = program(b"findframe 0\nrecaddactor 1", &mut backend);
    p.init_fresh_runtime(&mut backend).unwrap();
    backend.fail = Some(RuntimeEffect::ReleaseName(13));
    backend.events.clear();
    assert!(p.done_fresh_runtime(&mut backend).is_err());
    let runtime = p.runtime().unwrap();
    assert_eq!(runtime.stage, RuntimeStage::DisposalInterrupted);
    assert!(runtime.instructions.active()[1].parameter.is_none());
    assert!(runtime.instructions.active()[0].parameter.is_some());
    assert_eq!(backend.events, [RuntimeEffect::ReleaseName(43), RuntimeEffect::ReleaseName(13)]);
    let calls = backend.events.len();
    assert!(p.done_fresh_runtime(&mut backend).is_err());
    assert!(p.call_owned_process(1, &mut DispatchBudget::new(4).unwrap(), &mut backend).is_err());
    assert_eq!(p.discard_runtime_host().unwrap().name_allocations, 1);
    assert_eq!(backend.events.len(), calls);
    p.destruct(&mut backend).unwrap();
}

#[test]
fn instruction_storage_growth_keeps_each_distinct_owned_name() {
    let mut backend = Probe::default();
    let source = (0..96).map(|i| format!("findframe {i},\"frame{i}\"")).collect::<Vec<_>>().join("\n");
    let mut p = program(source.as_bytes(), &mut backend);
    assert_eq!(p.init_fresh_runtime(&mut backend).unwrap().accepted, 96);
    for index in 0..96 {
        assert_eq!(name(&p, index).register(), Some(index as i32));
        assert_eq!(name(&p, index).name(), Some(format!("frame{index}").as_bytes()));
    }
    cleanup(&mut p, &mut backend);
}

#[test]
fn default_named_effect_hook_delegates_to_the_required_backend() {
    struct Stop;
    impl RuntimeBackend for Stop {
        fn effect(&mut self, _: RuntimeView<'_>, effect: RuntimeEffect, published: Option<&ScalarParameter>) -> Result<(), String> {
            assert!(published.is_none());
            assert_eq!(effect, RuntimeEffect::AllocateName);
            Err("observed named allocation".into())
        }
    }
    let mut fields = RuntimeFields::initialized();
    assert_eq!(Stop.name_parameter_effect(fields.view(), RuntimeEffect::AllocateName, None), Err("observed named allocation".into()));
}

#[test]
fn non_ascii_instruction_admission_is_not_silently_widened_by_the_name_decoder() {
    let mut backend = Probe::default();
    let mut p = program(b"findframe 0,\"frame\x80\"", &mut backend);
    let before = p.source().unwrap().bytes.clone();
    assert!(format!("{:?}", p.init_fresh_runtime(&mut backend).unwrap_err()).contains("UnsupportedCrtByte"));
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::CompileInterrupted);
    assert!(p.runtime().unwrap().instructions.active().is_empty());
    assert!(p.runtime().unwrap().pending.is_none());
    assert!(!backend.events.contains(&RuntimeEffect::AllocateName));
    assert_eq!(p.source().unwrap().bytes, before);
    assert_eq!(p.discard_runtime_host().unwrap().name_allocations, 0);
    p.destruct(&mut backend).unwrap();
}

#[test]
fn unwound_done_release_cannot_be_replayed_for_either_payload_kind() {
    struct PanicRelease(usize);
    impl RuntimeBackend for PanicRelease {
        fn effect(&mut self, _: RuntimeView<'_>, effect: RuntimeEffect, _: Option<&ScalarParameter>) -> Result<(), String> {
            assert!(matches!(effect, RuntimeEffect::ReleaseName(13) | RuntimeEffect::ReleaseScalar(11)));
            self.0 += 1;
            panic!("external free occurred before unwind");
        }
    }
    for source in [b"findframe 0".as_slice(), b"dim_frm 1"] {
        let mut backend = Probe::default();
        let mut p = program(source, &mut backend);
        p.init_fresh_runtime(&mut backend).unwrap();
        let mut release = PanicRelease(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = p.done_fresh_runtime(&mut release);
        }));
        assert!(result.is_err());
        assert_eq!(p.runtime().unwrap().stage, RuntimeStage::DisposalInterrupted);
        assert!(p.done_fresh_runtime(&mut release).is_err());
        assert!(p.call_owned_process(1, &mut DispatchBudget::new(2).unwrap(), &mut release).is_err());
        let report = p.discard_runtime_host().unwrap();
        assert_eq!(report.scalar_allocations + report.name_allocations, 1);
        assert_eq!(release.0, 1);
        p.destruct(&mut backend).unwrap();
    }
}
