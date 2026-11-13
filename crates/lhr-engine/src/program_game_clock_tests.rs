//! Live game-clock tests through the actual compiled program owner.
use super::*;

#[test]
fn a_mutating_callback_changes_the_next_read_in_the_same_dispatch() {
    struct ClockMutation<'a>(&'a mut Memory);
    impl RuntimeBackend for ClockMutation<'_> {
        fn game_clock(&self) -> Option<crate::game_housekeeping::GameClockSource> {
            Some(self.0.clock.reader())
        }
        fn effect(
            &mut self,
            fields: RuntimeView<'_>,
            effect: RuntimeEffect,
            published: Option<&ScalarParameter>,
        ) -> Result<(), String> {
            if matches!(effect, RuntimeEffect::SetNightMission(_)) {
                self.0.clock.set(63);
            }
            self.0.effect(fields, effect, published)
        }
    }
    let mut memory = Memory::default();
    memory.clock.set(7);
    let mut p = program(b"commandblock 1\ndim_flt 2\ngetgametime 0\ngame_nightmission 1\ngetgametime 1\ncommandblock 0", &mut memory);
    let mut backend = ClockMutation(&mut memory);
    p.init_fresh_runtime(&mut backend).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(16).unwrap(), &mut backend)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_deref().unwrap(),
        &[7.0, 63.0]
    );
}

#[test]
fn compact_block_reads_one_live_time_not_instruction_count() {
    let mut memory = Memory::default();
    memory.clock.set(7);
    let mut p = program(
        b"commandblock 1\ndim_flt 2\ngetgametime 0\ngetgametime 1\ncommandblock 0",
        &mut memory,
    );
    assert_eq!(p.init_fresh_runtime(&mut memory).unwrap().errors, 0);
    p.call_owned_process(7, &mut DispatchBudget::new(16).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(
        p.runtime().unwrap().fields.floats.as_deref().unwrap(),
        &[7.0, 7.0]
    );
    assert_eq!(memory.clock.get(), 7);
}
#[test]
fn separate_programs_and_late_initialization_share_the_game_clock() {
    let mut memory = Memory::default();
    memory.clock.set(100);
    let source = b"dim_flt 2\ngetgametime 0\ngetgametime 1";
    let mut first = program(source, &mut memory);
    first.init_fresh_runtime(&mut memory).unwrap();
    first
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    first
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    let mut later = program(source, &mut memory);
    later.init_fresh_runtime(&mut memory).unwrap();
    later
        .call_owned_process(50, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    later
        .call_owned_process(50, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    memory.clock.set(137);
    for p in [&mut first, &mut later] {
        p.call_owned_process(999, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(
            p.runtime().unwrap().fields.floats.as_deref().unwrap(),
            &[100.0, 137.0]
        );
    }
    assert_eq!(memory.clock.get(), 137);
}
#[test]
fn game_time_uses_the_signed_game_accessor_value() {
    for time in [-1, i32::MIN, i32::MAX] {
        let mut memory = Memory::default();
        memory.clock.set(time);
        let mut p = program(
            b"commandblock 1\ndim_flt 1\ngetgametime 0\ncommandblock 0",
            &mut memory,
        );
        p.init_fresh_runtime(&mut memory).unwrap();
        p.call_owned_process(7, &mut DispatchBudget::new(8).unwrap(), &mut memory)
            .unwrap();
        assert_eq!(p.runtime().unwrap().fields.get_float_reg(0), time as f32);
    }
}
#[test]
fn event_start_and_return_read_the_clock_with_wrapping_elapsed_time() {
    let mut memory = Memory::default();
    let mut p = program(b"event 60001\nreturn", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.runtime.as_mut().unwrap().fields.command_time = 12;
    memory.clock.set(-10);
    assert_eq!(p.try_set_event(60001).unwrap(), SetEventOutcome::Started);
    assert_eq!(p.runtime().unwrap().fields.event_time, (-10i32) as u32);
    p.call_owned_process(500, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    memory.clock.set(5);
    p.runtime.as_mut().unwrap().fields.flags.stop_at_return = true;
    p.call_owned_process(500, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    assert_eq!(p.runtime().unwrap().fields.command_time, 27);
    assert_eq!(memory.clock.get(), 5);
}
#[test]
fn replaced_clock_cannot_silently_rebind_a_retained_program() {
    let mut memory = Memory::default();
    let mut p = program(b"dim_flt 1\ngetgametime 0", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap();
    memory.clock = crate::game_housekeeping::GameClock::default();
    memory.clock.set(999);
    let error = p
        .call_owned_process(1, &mut DispatchBudget::new(8).unwrap(), &mut memory)
        .unwrap_err();
    assert!(format!("{error:?}").contains("game clock owner unavailable"));
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 1);
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
}
#[test]
fn event_after_done_finds_no_event_like_the_cleared_native_list() {
    let mut memory = Memory::default();
    let mut p = program(b"event 60001\nreturn", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    p.done_fresh_runtime(&mut memory).unwrap();
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::Done);
    assert_eq!(p.try_set_event(60001).unwrap(), SetEventOutcome::NoEvent);
    assert!(p.runtime().unwrap().fields.gosub_stack.is_empty());
}
#[test]
fn absent_event_skips_clock_and_failed_event_retains_its_prefix() {
    let mut memory = Memory::default();
    let mut p = program(b"event 60001", &mut memory);
    p.init_fresh_runtime(&mut memory).unwrap();
    memory.clock = crate::game_housekeeping::GameClock::default();
    assert_eq!(p.try_set_event(99999).unwrap(), SetEventOutcome::NoEvent);
    assert!(p
        .try_set_event(60001)
        .unwrap_err()
        .contains("game clock owner unavailable"));
    assert_eq!(p.runtime().unwrap().fields.gosub_stack, vec![0xc000]);
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    assert!(p.try_set_event(60001).is_err());
    assert_eq!(p.runtime().unwrap().fields.gosub_stack, vec![0xc000]);
}
