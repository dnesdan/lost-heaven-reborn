//! Synthetic execution tests over one CProgram and a borrowed lookup world.
use super::*;
use super::super::{CProgram, ProgramAllocation, ProgramBackend, ProgramBuffer, ProgramSeed, RuntimeStage};
use crate::frame_lookup::{FrameLifetime, FrameLookupError, FrameView, FrameLookupBackend, SceneRoots};
use crate::script_lookup::ActorLifetime;
use crate::script_scheduler::DispatchBudget;

#[derive(Default)]
struct Memory { next: u64 }
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
    fn effect(&mut self, _: RuntimeView<'_>, _: RuntimeEffect, _: Option<&ScalarParameter>) -> Result<(), String> {
        Ok(())
    }
}
fn compiled(source: &[u8], memory: &mut Memory) -> CProgram {
    let mut program = CProgram::construct(ProgramSeed {
        address_high_byte: 0x81, padding_0d: [5; 3],
        words_1c_2c: [0xdeadbeef; 5], padding_31: [7; 3],
        word_40: 0x12345678, bytes_64: [0xff; 4],
    });
    program.set_source_code(Some(source), memory).unwrap();
    program.init_fresh_runtime(memory).unwrap();
    program
}
fn finish(program: &mut CProgram, memory: &mut Memory) {
    program.done_fresh_runtime(memory).unwrap();
    program.discard_runtime_host().unwrap();
    program.destruct(memory).unwrap();
}

struct World {
    actor: ActorLifetime,
    frame: FrameLifetime,
    name: Vec<u8>,
    primary: Vec<Option<ActorReferenceId>>,
    diagnostics: usize,
}
impl World {
    fn new(name: &[u8]) -> Self {
        let actor = ActorLifetime::for_existing(17);
        Self {
            primary: vec![Some(actor.reference())],
            actor,
            frame: FrameLifetime::for_existing(42),
            name: name.to_vec(),
            diagnostics: 0,
        }
    }
}
impl FrameLookupBackend for World {
    fn default_c_locale(&self) -> bool { true }
    fn frame(&self, key: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
        if key != self.frame.reference() {
            return Err(FrameLookupError::StaleFrame(key));
        }
        Ok(FrameView { name: Some(&self.name), child: None, next: None })
    }
}
impl LookupWorldBackend for World {
    fn diagnostic_default_c_locale(&self) -> bool { true }
    fn owner(&self) -> Result<Option<ActorReferenceId>, LookupError> {
        panic!("CProgram must supply its own owner")
    }
    fn actor_frame(&self, actor: ActorReferenceId) -> Result<Option<FrameReferenceId>, LookupError> {
        if actor != self.actor.reference() {
            return Err(LookupError::StaleActor(actor));
        }
        Ok(Some(self.frame.reference()))
    }
    fn primary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> { Ok(&self.primary) }
    fn players(&self) -> Result<[Option<ActorReferenceId>; 2], LookupError> { Ok([None; 2]) }
    fn secondary(&self) -> Result<&[Option<ActorReferenceId>], LookupError> { Ok(&[]) }
    fn frames(&self) -> Result<&dyn FrameLookupBackend, LookupError> { Ok(self) }
    fn scene(&self) -> Result<SceneRoots, LookupError> {
        Ok(SceneRoots { primary: Some(self.frame.reference()), backdrop: None })
    }
    fn message_beep(&mut self, _: i32) -> Result<(), LookupError> {
        panic!("diagnostic must receive actual CProgram owner")
    }
}
impl RuntimeLookupWorld for World {
    fn runtime_message_beep(
        &mut self,
        _: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        value: i32,
    ) -> Result<(), LookupError> {
        assert_eq!(value, -1);
        assert_eq!(*owner, Some(self.actor.reference()));
        self.diagnostics += 1;
        Ok(())
    }
}
fn execute(program: &mut CProgram, memory: &mut Memory, world: &mut World)
    -> Result<(), RuntimeError>
{
    program.call_owned_process(
        7, &mut DispatchBudget::new(16).unwrap(),
        &mut LookupRuntimeBackend::new(memory, world),
    )
}
#[test]
fn source_owned_program_writes_directly_to_both_register_banks() {
    let mut memory = Memory::default();
    let mut world = World::new(b"alpha");
    let mut program = compiled(
        b"commandblock 1\ndim_act 1\ndim_frm 1\nfindactor 0,\"alpha\"\nfindframe 0,\"alpha\"\ncommandblock 0",
        &mut memory,
    );
    let original = program.source().unwrap().bytes.clone();
    execute(&mut program, &mut memory, &mut world).unwrap();
    let runtime = program.runtime().unwrap();
    assert_eq!(runtime.stage, RuntimeStage::Ready);
    assert_eq!(runtime.fields.program_counter(), 6);
    assert_eq!(runtime.fields.actors.as_ref().unwrap()[0].reference().unwrap().copied(),
               Some(world.actor.reference()));
    assert_eq!(runtime.fields.frames.as_ref().unwrap()[0].reference().unwrap().copied(),
               Some(world.frame.reference()));
    assert_eq!(program.source().unwrap().bytes, original);
    finish(&mut program, &mut memory);
}
#[test]
fn diagnostic_precedes_live_name_lookup_and_runs_once() {
    let mut memory = Memory::default();
    let mut world = World::new(b"chcipnuti chlapka");
    let mut program = compiled(
        b"dim_act 1\nfindactor 0\nfindactor 1",
        &mut memory,
    );
    program.set_owner(Some(world.actor.reference())).unwrap();
    execute(&mut program, &mut memory, &mut world).unwrap();
    assert_eq!(world.diagnostics, 2);
    assert_eq!(
        program.runtime().unwrap().fields.actors.as_ref().unwrap()[0]
            .reference().unwrap().copied(),
        Some(world.actor.reference()),
    );
    assert_eq!(program.runtime().unwrap().fields.program_counter(), 3);
    finish(&mut program, &mut memory);
}
#[test]
fn an_unbound_world_is_a_terminal_lookup_error_not_a_synthetic_success() {
    let mut memory = Memory::default();
    let mut program = compiled(b"dim_frm 1\nfindframe 0,\"alpha\"", &mut memory);
    let result = program.call_owned_process(7, &mut DispatchBudget::new(8).unwrap(), &mut memory);
    assert_eq!(
        result,
        Err(RuntimeError::Lookup(LookupError::UnsupportedWorldBackend)),
    );
    assert_eq!(program.runtime().unwrap().stage, RuntimeStage::ProcessInterrupted);
    assert_eq!(program.runtime().unwrap().fields.program_counter(), 1);
    finish(&mut program, &mut memory);
}
