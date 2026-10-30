//! Actual CProgram compilation/dispatch with controlled live world callbacks.
use super::*;
use super::super::{
    CProgram, ProgramAllocation, ProgramBackend, ProgramBuffer, ProgramSeed, RuntimeStage,
};
use crate::frame_lookup::{FrameLifetime, FrameLookupError, FrameView};
use crate::script_lookup::ActorLifetime;
use crate::script_scheduler::DispatchBudget;

#[derive(Default)]
struct Memory {
    next: u64,
    events: Vec<RuntimeEffect>,
    panic_release: bool,
}
impl ProgramBackend for Memory {
    fn allocate(&mut self, _: &CProgram, _: ProgramBuffer, _: u32) -> Result<ProgramAllocation, String> {
        self.next += 1;
        Ok(ProgramAllocation(self.next))
    }
    fn free(&mut self, _: &CProgram, _: ProgramBuffer, _: Option<ProgramAllocation>) -> Result<(), String> {
        Ok(())
    }
}
impl RuntimeBackend for Memory {
    fn effect(&mut self, _: RuntimeView<'_>, effect: RuntimeEffect, _: Option<&ScalarParameter>) -> Result<(), String> {
        self.events.push(effect);
        if self.panic_release && effect == RuntimeEffect::ReleaseActors {
            panic!("release performed before controlled unwind");
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
    p.init_fresh_runtime(memory).unwrap();
    p
}
fn owner(p: &mut CProgram, value: Option<ActorReferenceId>) {
    // Tests live mutation through the same owner passed to diagnostics. There
    // is no second owner in RuntimeFields or a manufactured actor allocation.
    p.owner = value;
}
fn finish(p: &mut CProgram, memory: &mut Memory) {
    p.done_fresh_runtime(memory).unwrap();
    let report = p.discard_runtime_host().unwrap();
    assert_eq!((report.scalar_allocations, report.name_allocations), (0, 0));
    p.destruct(memory).unwrap();
}
#[derive(Clone, Copy, Default)]
enum Beep {
    #[default]
    None,
    Redirect,
    FailAfterRedirect,
    PanicAfterRedirect,
    RetireMatch,
}
struct World {
    actors: Vec<ActorLifetime>,
    frames: Vec<FrameLifetime>,
    names: Vec<Vec<u8>>,
    primary: Vec<Option<ActorReferenceId>>,
    secondary: Vec<Option<ActorReferenceId>>,
    players: [Option<ActorReferenceId>; 2],
    roots: Option<SceneRoots>,
    beep: Beep,
    beeps: usize,
    fail_secondary: bool,
    game_locale: bool,
    frame_locale: bool,
    reads: std::cell::Cell<usize>,
}
impl World {
    fn new() -> Self {
        let mut w = Self {
            actors: (0..6).map(ActorLifetime::for_existing).collect(),
            frames: (0..6).map(FrameLifetime::for_existing).collect(),
            names: vec![b"alpha".to_vec(); 6],
            primary: Vec::new(), secondary: Vec::new(), players: [None; 2], roots: None,
            beep: Beep::None, beeps: 0, fail_secondary: false,
            game_locale: true, frame_locale: true, reads: std::cell::Cell::new(0),
        };
        w.roots = Some(SceneRoots { primary: Some(w.f(1)), backdrop: Some(w.f(2)) });
        w
    }
    fn a(&self, index: usize) -> ActorReferenceId { self.actors[index].reference() }
    fn f(&self, index: usize) -> FrameReferenceId { self.frames[index].reference() }
}
impl FrameLookupBackend for World {
    fn default_c_locale(&self) -> bool { self.frame_locale }
    fn frame(&self, reference: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
        let i = self.frames.iter().position(|f| f.reference() == reference)
            .ok_or(FrameLookupError::StaleFrame(reference))?;
        Ok(FrameView { name: Some(&self.names[i]), child: None, next: None })
    }
}
impl LookupWorldBackend for World {
    fn diagnostic_default_c_locale(&self) -> bool { self.game_locale }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        panic!("CProgram's owner must override the world's detached owner")
    }
    fn actor_frame(&self, actor: ActorReferenceId) -> Result<Option<FrameReferenceId>, LookupError> {
        self.reads.set(self.reads.get() + 1);
        let i = self.actors.iter().position(|a| a.reference() == actor)
            .ok_or(LookupError::StaleActor(actor))?;
        Ok(Some(self.f(i)))
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> { Ok(&self.primary) }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> {
        if self.fail_secondary { Err(LookupError::Backend("secondary unavailable".into())) }
        else { Ok(&self.secondary) }
    }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> { Ok(self.players) }
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> { Ok(self) }
    fn scene(&self) -> Result<SceneRoots, LookupError> { self.roots.ok_or(LookupError::MissingScene) }
    fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
        panic!("diagnostic must receive the real runtime owner")
    }
}
impl RuntimeLookupWorld for World {
    fn runtime_message_beep(&mut self, mut fields: RuntimeView<'_>, owner: &mut Option<ActorReferenceId>, argument: i32) -> Result<(), LookupError> {
        assert_eq!(argument, -1);
        self.beeps += 1;
        match self.beep {
            Beep::None => {}
            Beep::RetireMatch => {
                self.actors[1] = ActorLifetime::for_existing(1);
            }
            Beep::Redirect | Beep::FailAfterRedirect | Beep::PanicAfterRedirect => {
                fields.set_program_counter(2);
                *fields.actor_count = 2;
                *fields.actors = Some(vec![RuntimeReferenceSlot::Empty; 2].into_boxed_slice());
                *owner = Some(self.a(2));
                self.primary.clear();
                if matches!(self.beep, Beep::FailAfterRedirect) {
                    return Err(LookupError::Backend("beep failed after mutation".into()));
                }
                if matches!(self.beep, Beep::PanicAfterRedirect) {
                    panic!("beep unwound after mutation");
                }
            }
        }
        Ok(())
    }
}
fn step(p: &mut CProgram, memory: &mut Memory, world: &mut World, delta: i32) -> Result<(), RuntimeError> {
    p.call_owned_process(delta, &mut DispatchBudget::new(32).unwrap(), &mut LookupRuntimeBackend::new(memory, world))
}
fn a(p: &CProgram, index: usize) -> Option<ActorReferenceId> {
    p.runtime().unwrap().fields.actors.as_ref().unwrap()[index].reference().unwrap().copied()
}
fn f(p: &CProgram, index: usize) -> Option<FrameReferenceId> {
    p.runtime().unwrap().fields.frames.as_ref().unwrap()[index].reference().unwrap().copied()
}

#[test]
fn compiled_lookup_uses_the_actual_registers_and_keeps_source_immutable() {
    let mut memory = Memory::default();
    let mut w = World::new(); w.primary.push(Some(w.a(1)));
    let mut p = program(b"commandblock 1\ndim_act 1\ndim_frm 1\nfindactor 0,\"alpha\"\nfindframe 0,\"ALPHA\"\ncommandblock 0", &mut memory);
    let source = p.source().unwrap().bytes.clone();
    step(&mut p, &mut memory, &mut w, 7).unwrap();
    assert_eq!((a(&p, 0), f(&p, 0)), (Some(w.a(1)), Some(w.f(1))));
    let r = p.runtime().unwrap();
    assert_eq!((r.fields.program_counter(), r.fields.last_pc, r.fields.elapsed), (6, 5, 0));
    assert_eq!(r.stage, RuntimeStage::Ready);
    assert_eq!(p.source().unwrap().bytes, source);
    finish(&mut p, &mut memory);
}
#[test]
fn primary_reverse_order_then_kept_actor_then_reverse_globals_preserve_miss_semantics() {
    let mut memory = Memory::default(); let mut w = World::new();
    let mut p = program(b"dim_act 1\nfindactor 0,\"alpha\"\nfindactor 0,\"alpha\"\nfindactor 0,\"alpha\"\nfindactor 0,\"missing\"", &mut memory);
    w.primary = vec![Some(w.a(0)), Some(w.a(1))];
    w.players = [Some(w.a(2)), None]; w.secondary = vec![Some(w.a(3)), Some(w.a(4))];
    step(&mut p,&mut memory,&mut w,1).unwrap(); step(&mut p,&mut memory,&mut w,1).unwrap();
    assert_eq!(a(&p,0), Some(w.a(1)));
    w.primary.clear(); step(&mut p,&mut memory,&mut w,1).unwrap();
    assert_eq!(a(&p,0), Some(w.a(4)));
    w.secondary.clear(); step(&mut p,&mut memory,&mut w,1).unwrap();
    let retained = w.a(2); assert_eq!(a(&p,0), Some(retained));
    w.players = [None; 2]; w.actors[2] = ActorLifetime::for_existing(2);
    step(&mut p,&mut memory,&mut w,1).unwrap();
    assert_eq!(a(&p,0), Some(retained)); // even a stale old slot survives a miss
    finish(&mut p,&mut memory);
}
#[test]
fn frame_search_includes_backdrop_and_clears_on_miss_without_actor_case_folding() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[2] = b"BackdropObject".to_vec();
    let mut p = program(b"dim_frm 1\nfindframe 0,\"backdropobject\"\nfindframe 0,\"absent\"", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); step(&mut p,&mut memory,&mut w,1).unwrap();
    assert_eq!(f(&p,0), Some(w.f(2)));
    step(&mut p,&mut memory,&mut w,1).unwrap(); assert_eq!(f(&p,0), None);
    finish(&mut p,&mut memory);
}
#[test]
fn omitted_names_read_the_cprogram_owner_without_a_scene() {
    let mut memory = Memory::default(); let mut w = World::new(); w.roots = None;
    let mut p = program(b"commandblock 1\ndim_act 1\ndim_frm 1\nfindactor 0\nfindframe 0\ncommandblock 0", &mut memory);
    owner(&mut p,Some(w.a(3)));
    step(&mut p,&mut memory,&mut w,1).unwrap();
    assert_eq!((a(&p,0),f(&p,0)), (Some(w.a(3)),Some(w.f(3))));
    finish(&mut p,&mut memory);
}
#[test]
fn callback_redirect_rereads_named_operand_owner_and_register_backing() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[0] = b"CHCIPNUTI CHLAPKA".to_vec(); w.beep = Beep::Redirect;
    let mut p = program(b"dim_act 1\nfindactor 0,\"ignored\"\nfindactor 1", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); owner(&mut p,Some(w.a(0)));
    step(&mut p,&mut memory,&mut w,9).unwrap();
    assert_eq!((a(&p,0),a(&p,1)), (None,Some(w.a(2))));
    assert_eq!(p.owner(),Some(w.a(2)));
    let r = p.runtime().unwrap();
    assert_eq!((r.fields.program_counter(),r.fields.last_pc,r.fields.elapsed), (3,2,0));
    assert_eq!(w.beeps,1); finish(&mut p,&mut memory);
}
#[test]
fn diagnostic_precedes_negative_and_upper_bound_checks() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[0] = b"chcipnuti chlapka".to_vec();
    let mut p = program(b"dim_act 1\nfindactor 99\nfindactor -1", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); owner(&mut p,Some(w.a(0)));
    step(&mut p,&mut memory,&mut w,2).unwrap(); assert_eq!(w.beeps,1);
    assert_eq!(step(&mut p,&mut memory,&mut w,3),Err(RuntimeError::Lookup(LookupError::NegativeIndex(-1))));
    assert_eq!(w.beeps,2); assert_eq!(a(&p,0),None);
    assert_eq!((p.runtime().unwrap().fields.program_counter(),p.runtime().unwrap().fields.elapsed),(2,3));
    finish(&mut p,&mut memory);
}
#[test]
fn failing_diagnostic_keeps_mutated_runtime_and_owner_without_repeating_effects() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[0] = b"chcipnuti chlapka".to_vec(); w.beep = Beep::FailAfterRedirect;
    let mut p = program(b"dim_act 1\nfindactor 0\nfindactor 1", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); owner(&mut p,Some(w.a(0)));
    assert!(step(&mut p,&mut memory,&mut w,9).is_err());
    assert_eq!(p.owner(),Some(w.a(2))); assert_eq!(a(&p,1),None);
    assert_eq!((p.runtime().unwrap().fields.program_counter(),p.runtime().unwrap().fields.elapsed),(2,9));
    assert!(step(&mut p,&mut memory,&mut w,9).is_err()); assert_eq!(w.beeps,1);
    finish(&mut p,&mut memory);
}
#[test]
fn unwound_diagnostic_cannot_leave_ready_or_replay_but_allows_normal_done() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[0] = b"chcipnuti chlapka".to_vec(); w.beep = Beep::PanicAfterRedirect;
    let mut p = program(b"dim_act 1\nfindactor 0\nfindactor 1", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); owner(&mut p,Some(w.a(0)));
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| step(&mut p,&mut memory,&mut w,9))).is_err());
    assert_eq!(p.runtime().unwrap().stage,RuntimeStage::ProcessInterrupted);
    assert_eq!(p.owner(),Some(w.a(2)));
    assert!(step(&mut p,&mut memory,&mut w,9).is_err()); assert_eq!(w.beeps,1);
    finish(&mut p,&mut memory);
}
#[test]
fn kept_actor_write_survives_a_later_global_source_failure() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.players[0] = Some(w.a(2)); w.fail_secondary = true;
    let mut p = program(b"dim_act 1\nfindactor 0,\"alpha\"", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap();
    assert!(step(&mut p,&mut memory,&mut w,9).is_err());
    assert_eq!(a(&p,0),Some(w.a(2)));
    assert_eq!((p.runtime().unwrap().fields.program_counter(),p.runtime().unwrap().fields.elapsed),(1,9));
    finish(&mut p,&mut memory);
}
#[test]
fn primary_match_skips_unavailable_later_sources() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.primary.push(Some(w.a(1))); w.fail_secondary = true;
    let mut p = program(b"commandblock 1\ndim_act 1\nfindactor 0,\"alpha\"\ncommandblock 0", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); assert_eq!(a(&p,0),Some(w.a(1)));
    finish(&mut p,&mut memory);
}
#[test]
fn callback_retirement_does_not_resolve_a_new_actor_with_the_same_raw_id() {
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[0] = b"chcipnuti chlapka".to_vec(); w.beep = Beep::RetireMatch;
    let old = w.a(1); w.primary.push(Some(old));
    let mut p = program(b"dim_act 1\nfindactor 0,\"alpha\"", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap(); owner(&mut p,Some(w.a(0)));
    assert_eq!(step(&mut p,&mut memory,&mut w,1),Err(RuntimeError::Lookup(LookupError::StaleActor(old))));
    assert_ne!(old,w.a(1)); assert_eq!(a(&p,0),None); finish(&mut p,&mut memory);
}
#[test]
fn invalid_frame_root_is_rejected_without_replacing_the_existing_slot() {
    let mut memory = Memory::default(); let mut w = World::new();
    let old = w.f(1); w.frames[1] = FrameLifetime::for_existing(1);
    let mut p = program(b"dim_frm 1\nfindframe 0,\"alpha\"", &mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap();
    assert_eq!(step(&mut p,&mut memory,&mut w,1),Err(RuntimeError::Lookup(LookupError::Frame(FrameLookupError::StaleFrame(old)))));
    assert_eq!(f(&p,0),None); finish(&mut p,&mut memory);
}
#[test]
fn out_of_range_and_null_owner_copy_need_no_world_backend() {
    let mut memory = Memory::default();
    let mut p = program(b"commandblock 1\ndim_act 1\ndim_frm 1\nfindactor 99,\"x\"\nfindframe 99,\"x\"\nfindactor 0\nfindframe 0\ncommandblock 0", &mut memory);
    p.call_owned_process(3,&mut DispatchBudget::new(32).unwrap(),&mut memory).unwrap();
    assert_eq!((a(&p,0),f(&p,0)),(None,None)); assert_eq!(p.runtime().unwrap().fields.program_counter(),8);
    finish(&mut p,&mut memory);
}
#[test]
fn logical_count_is_distinct_from_backing_and_validation_occurs_at_the_write() {
    for (name, fails) in [("missing",false),("alpha",true)] {
        let mut memory = Memory::default(); let mut w = World::new(); w.primary.push(Some(w.a(1)));
        let mut p = program(format!("dim_act 1\nfindactor 2,\"{name}\"").as_bytes(),&mut memory);
        step(&mut p,&mut memory,&mut w,1).unwrap(); p.runtime.as_mut().unwrap().fields.actor_count = 3;
        let result = step(&mut p,&mut memory,&mut w,7);
        assert_eq!(result.is_err(),fails); assert_eq!(a(&p,0),None);
        if fails { assert!(format!("{result:?}").contains("actor register backing unavailable")); }
        finish(&mut p,&mut memory);
    }
}
#[test]
fn assignment_initializes_an_unwritten_slot_and_does_not_read_its_previous_value() {
    let mut memory = Memory::default(); let mut w = World::new();
    let mut p = program(b"dim_frm 1\nfindframe 0,\"alpha\"",&mut memory);
    step(&mut p,&mut memory,&mut w,1).unwrap();
    p.runtime.as_mut().unwrap().fields.frames.as_mut().unwrap()[0] = RuntimeReferenceSlot::Uninitialized;
    step(&mut p,&mut memory,&mut w,1).unwrap(); assert_eq!(f(&p,0),Some(w.f(1)));
    finish(&mut p,&mut memory);
}
#[test]
fn negative_native_count_is_an_upper_bound_noop_without_world_access() {
    let mut memory = Memory::default();
    let mut p = program(b"findframe 0,\"x\"",&mut memory);
    p.runtime.as_mut().unwrap().fields.frame_count = u32::MAX;
    p.call_owned_process(1,&mut DispatchBudget::new(4).unwrap(),&mut memory).unwrap();
    assert!(p.runtime().unwrap().fields.frames.is_none());
    assert_eq!(p.runtime().unwrap().fields.program_counter(),1); finish(&mut p,&mut memory);
}
#[test]
fn destructive_release_unwind_still_blocks_done_and_external_free_replay() {
    let mut memory = Memory::default(); let mut p = program(b"dim_act 1\ndim_act 2",&mut memory);
    p.call_owned_process(1,&mut DispatchBudget::new(4).unwrap(),&mut memory).unwrap();
    memory.panic_release = true;
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| p.call_owned_process(1,&mut DispatchBudget::new(4).unwrap(),&mut memory))).is_err());
    assert_eq!(p.runtime().unwrap().stage,RuntimeStage::DisposalInterrupted);
    let events = memory.events.len();
    assert!(p.done_fresh_runtime(&mut memory).is_err());
    assert!(p.call_owned_process(1,&mut DispatchBudget::new(4).unwrap(),&mut memory).is_err());
    p.discard_runtime_host().unwrap(); assert_eq!(memory.events.len(),events); p.destruct(&mut memory).unwrap();
}

#[test]
fn bound_world_does_not_silently_substitute_for_a_missing_diagnostic_effect() {
    struct NoDiagnostic<'a> { memory: &'a mut Memory, world: &'a mut World }
    impl RuntimeBackend for NoDiagnostic<'_> {
        fn effect(&mut self, fields: RuntimeView<'_>, effect: RuntimeEffect, value: Option<&ScalarParameter>) -> Result<(), String> {
            self.memory.effect(fields, effect, value)
        }
        fn lookup_world(&mut self) -> Result<&mut dyn LookupWorldBackend, LookupError> { Ok(self.world) }
    }
    let mut memory = Memory::default(); let mut w = World::new();
    w.names[0] = b"chcipnuti chlapka".to_vec();
    let mut p = program(b"dim_act 1\nfindactor 0", &mut memory);
    step(&mut p, &mut memory, &mut w, 1).unwrap(); owner(&mut p, Some(w.a(0)));
    let error = p.call_owned_process(5, &mut DispatchBudget::new(4).unwrap(),
        &mut NoDiagnostic { memory: &mut memory, world: &mut w }).unwrap_err();
    assert_eq!(error, RuntimeError::Lookup(LookupError::Backend("runtime lookup diagnostic unavailable".into())));
    assert_eq!(w.beeps, 0); assert_eq!(a(&p, 0), None);
    assert_eq!(p.runtime().unwrap().fields.elapsed, 5);
    finish(&mut p, &mut memory);
}
