//! Actual owned-runtime regressions for the shared game stream.
use super::*;
use crate::game_housekeeping::GameRandom;
fn initialized(source: &[u8], memory: &mut Memory) -> CProgram {
    let mut p = program(source, memory);
    let progress = p.init_fresh_runtime(memory).unwrap();
    assert_eq!(progress.errors, 0);
    p
}
fn run(p: &mut CProgram, memory: &mut Memory) -> Result<(), RuntimeError> {
    p.call_owned_process(7, &mut DispatchBudget::new(32).unwrap(), memory)
}
fn value(p: &CProgram) -> f32 { p.runtime().unwrap().fields.floats.as_ref().unwrap()[0] }
const SCRIPT: &[u8] = b"commandblock 1\ndim_flt 1\nrnd 0,100\ncommandblock 0";
#[test]
fn separate_and_late_programs_consume_one_sequence() {
    let mut memory = Memory::default();
    let mut first = initialized(SCRIPT, &mut memory);
    let mut second = initialized(SCRIPT, &mut memory);
    run(&mut first, &mut memory).unwrap();
    run(&mut second, &mut memory).unwrap();
    let mut late = initialized(SCRIPT, &mut memory);
    run(&mut late, &mut memory).unwrap();
    assert_eq!([value(&first), value(&second), value(&late)], [41.0, 67.0, 34.0]);
}
#[test]
fn non_script_draws_and_reseed_are_visible_to_retained_programs() {
    let mut memory = Memory::default();
    let mut p = initialized(SCRIPT, &mut memory);
    assert_eq!(memory.random.source().next_raw(), Some(41));
    run(&mut p, &mut memory).unwrap();
    assert_eq!(value(&p), 67.0);
    let mut q = initialized(SCRIPT, &mut memory);
    memory.random.seed(0);
    run(&mut q, &mut memory).unwrap();
    assert_eq!(value(&q), 38.0);
}
#[test]
fn rejected_registers_do_not_consume_a_draw() {
    for source in [b"rnd 9,100".as_slice(), b"rnd -1,100".as_slice()] {
        let mut memory = Memory::default();
        let mut p = initialized(source, &mut memory);
        let _ = run(&mut p, &mut memory);
        assert_eq!(memory.random.state(), 1);
    }
}
#[test]
fn failed_store_keeps_the_consumed_draw_and_refuses_replay() {
    let mut memory = Memory::default();
    let mut p = initialized(b"rnd 0,100", &mut memory);
    p.runtime.as_mut().unwrap().fields.float_count = 1;
    assert!(run(&mut p, &mut memory).is_err());
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 0);
    assert_eq!(p.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    let after = memory.random.state();
    assert_ne!(after, 1);
    assert!(run(&mut p, &mut memory).is_err());
    assert_eq!(memory.random.state(), after);
    assert_eq!(memory.random.source().next_raw(), Some(18467));
}
#[test]
fn expired_owner_cannot_rebind_to_a_replacement() {
    let mut memory = Memory::default();
    let mut p = initialized(SCRIPT, &mut memory);
    memory.random = GameRandom::default();
    let error = run(&mut p, &mut memory).unwrap_err();
    assert!(format!("{error:?}").contains("game random owner unavailable"));
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    assert_eq!(memory.random.state(), 1);
    assert_eq!(value(&p), 0.0);
}
#[test]
fn missing_source_fails_only_at_a_reached_draw() {
    let mut memory = Memory::default();
    let mut skipped = initialized(b"rnd 9,100", &mut memory);
    skipped.runtime.as_mut().unwrap().fields.random_source = None;
    run(&mut skipped, &mut memory).unwrap();
    let mut p = initialized(SCRIPT, &mut memory);
    p.runtime.as_mut().unwrap().fields.random_source = None;
    assert!(run(&mut p, &mut memory).is_err());
    assert_eq!(p.runtime().unwrap().fields.program_counter(), 2);
    assert_eq!(memory.random.state(), 1);
}
#[test]
fn callbacks_share_the_same_consumption_order() {
    struct Callback { memory: Memory }
    impl RuntimeBackend for Callback {
        fn random_source(&self) -> Option<crate::game_housekeeping::GameRandomSource> {
            Some(self.memory.random.source())
        }
        fn effect(&mut self, fields: RuntimeView<'_>, effect: RuntimeEffect,
            published: Option<&ScalarParameter>) -> Result<(), String> {
            if matches!(effect, RuntimeEffect::AllocateFloats(_)) {
                assert_eq!(self.memory.random.source().next_raw(), Some(41));
            }
            self.memory.effect(fields, effect, published)
        }
    }
    let mut backend = Callback { memory: Memory::default() };
    let mut p = program(SCRIPT, &mut backend.memory);
    p.init_fresh_runtime(&mut backend).unwrap();
    p.call_owned_process(7, &mut DispatchBudget::new(32).unwrap(), &mut backend).unwrap();
    assert_eq!(value(&p), 67.0);
}
