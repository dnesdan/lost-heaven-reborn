//! C_program::Process guard order at mafia-re fe7e7464, RVA 0x6d3b0.
//! These fixtures exercise the actual owned runtime, not a second interpreter.
use super::*;

#[test]
fn wait_goto_and_gosub_skip_registers_outside_the_logical_bank() {
    for (command, pushes_return) in [("wait", false), ("goto", false), ("gosub", true)] {
        for (count, register) in [(0, 0), (1, 1), (1, 9)] {
            let source = format!("{command} flt[{register}]\nlabel next");
            let mut memory = Memory::default();
            let mut p = program(source.as_bytes(), &mut memory);
            let progress = p.init_fresh_runtime(&mut memory).unwrap();
            assert_eq!((progress.accepted, progress.errors), (2, 0), "{source}");
            p.runtime.as_mut().unwrap().fields.float_count = count;
            p.runtime.as_mut().unwrap().fields.floats = None;
            let before = memory.events.len();
            p.call_owned_process(0, &mut DispatchBudget::new(8).unwrap(), &mut memory)
                .unwrap();
            let runtime = p.runtime().unwrap();
            assert_eq!(runtime.stage, RuntimeStage::Ready, "{source}");
            assert_eq!(runtime.fields.program_counter(), 1, "{source}");
            assert_eq!(runtime.fields.command_time, 0, "{source}");
            assert!(runtime.fields.floats.is_none());
            let expected: &[u32] = if pushes_return { &[1] } else { &[] };
            assert_eq!(runtime.fields.gosub_stack.as_slice(), expected, "{source}");
            assert_eq!(memory.events.len(), before, "{source}");
        }
    }
}

#[test]
fn wait_guard_ignores_even_present_values_outside_the_logical_count() {
    let mut memory = Memory::default();
    let mut p = program(b"wait flt[1]", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    fields.float_count = 1;
    fields.floats = Some(vec![3.0, 1000.0].into_boxed_slice());
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_deref().unwrap(),
        &[3.0, 1000.0]
    );
}

#[test]
fn jump_guard_does_not_use_surplus_backing_as_a_label() {
    for command in ["goto", "gosub"] {
        let source = format!("{command} flt[1]\nlabel skipped\nlabel 7");
        let mut memory = Memory::default();
        let mut p = program(source.as_bytes(), &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        let fields = &mut p.runtime.as_mut().unwrap().fields;
        fields.float_count = 1;
        fields.floats = Some(vec![0.0, 7.0].into_boxed_slice());
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(
            p.runtime().unwrap().fields.program_counter(),
            1,
            "{command}"
        );
    }
}

#[test]
fn signed_register_count_guards_do_not_read_unavailable_backing() {
    for count in [0x8000_0000, u32::MAX] {
        for command in ["wait", "goto", "gosub"] {
            let source = format!("{command} flt[0]");
            let mut memory = Memory::default();
            let mut p = program(source.as_bytes(), &mut memory);
            p.init_fresh_runtime(&mut memory).unwrap();
            p.runtime.as_mut().unwrap().fields.float_count = count;
            p.call_owned_process(0, &mut DispatchBudget::new(8).unwrap(), &mut memory)
                .unwrap();
            assert_eq!(p.runtime().unwrap().fields.program_counter(), 1, "{source}");
        }
    }
}

#[test]
fn an_admitted_register_with_missing_backing_still_interrupts() {
    for command in ["wait", "goto", "gosub"] {
        let source = format!("{command} flt[0]");
        let mut memory = Memory::default();
        let mut p = program(source.as_bytes(), &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        p.runtime.as_mut().unwrap().fields.float_count = 1;
        let before = memory.events.len();
        assert!(p
            .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .is_err());
        let runtime = p.runtime().unwrap();
        assert_eq!(runtime.stage, RuntimeStage::ProcessInterrupted);
        assert_eq!(runtime.fields.program_counter(), 0);
        let expected: &[u32] = if command == "gosub" { &[1] } else { &[] };
        assert_eq!(runtime.fields.gosub_stack.as_slice(), expected);
        assert_eq!(memory.events.len(), before);
    }
}

#[test]
fn admitted_wait_still_reads_its_register_and_waits() {
    let mut memory = Memory::default();
    let mut p = program(b"wait flt[0]", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    let fields = &mut p.runtime.as_mut().unwrap().fields;
    fields.float_count = 1;
    fields.floats = Some(vec![30.0].into_boxed_slice());
    p.call_owned_process(16, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    p.call_owned_process(16, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
}

#[test]
fn immediate_wait_does_not_require_a_register_bank() {
    let mut memory = Memory::default();
    let mut p = program(b"wait 30", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.float_count = u32::MAX;
    p.call_owned_process(16, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    p.call_owned_process(16, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
}

#[test]
fn named_jump_does_not_require_a_register_bank() {
    for command in ["goto", "gosub"] {
        let source = format!("{command} target\nlabel skipped\nlabel target");
        let mut memory = Memory::default();
        let mut p = program(source.as_bytes(), &mut memory);
        p.init_fresh_runtime(&mut memory).unwrap();
        p.runtime.as_mut().unwrap().fields.float_count = u32::MAX;
        p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(
            p.runtime().unwrap().fields.program_counter(),
            2,
            "{command}"
        );
    }
}
