//! Bounded fresh C_program runtime owned by the actual source program.
//!
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! Init RVA0x61580, Done RVA0x616a0, Compile RVA0x6c770, CallProcess RVA0x6ccb0.
//! Process RVA0x6d3b0 is admitted only for DIM_FRM, DIM_ACT and COMMANDBLOCK.
//! Admission is explicitly base C_program only; derived Entity/Detector virtual
//! decoder/dispatcher execution is not supplied by this runtime.
//! Unsupported dependencies retain reached state and never become no-op success.
//! Saved-code chains, repeated Init and runtime-owning source destruction remain
//! outside admission. Explicit host discard releases actual Rust allocations;
//! it is additional cleanup, not original Done or a mission completion result.
use super::*;
use crate::named_script::ResetState;
use crate::program_compiler::{self, CompileBackend, CompileDiagnostic, CompileProgress};
use crate::program_parameters::{
    ascii_integer_prefix, ScalarDecodeError, ScalarParameter, ScalarParameterBackend,
    ScalarParameterSlot,
};
use crate::program_parser::ParserLocale;
use crate::script_scheduler::{
    run_scheduler, ActorReferenceId, DispatchBudget, FrameReferenceId, InstructionView,
    SchedulerBackend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStage {
    Initializing,
    Ready,
    CompileInterrupted,
    ProcessInterrupted,
    DisposalInterrupted,
    Done,
}
/// Allocation does not establish a native reference value. Only reached
/// zero-fill slots become Empty; untouched slots remain explicitly unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeReferenceSlot<T> {
    Uninitialized,
    Empty,
    Reference(T),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeReferenceError {
    Uninitialized,
}
impl<T> RuntimeReferenceSlot<T> {
    pub fn reference(&self) -> Result<Option<&T>, RuntimeReferenceError> {
        match self {
            Self::Uninitialized => Err(RuntimeReferenceError::Uninitialized),
            Self::Empty => Ok(None),
            Self::Reference(value) => Ok(Some(value)),
        }
    }
}
#[derive(Debug, Default)]
pub struct RuntimeFlags {
    pub stop_at_return: bool,
    pub in_block: bool,
    pub block_mode: u8,
}
#[derive(Debug)]
/// Actual runtime owner fields. Scheduler legacy stack projections remain a
/// private compatibility boundary, not a new semantic stack/address API.
pub struct RuntimeFields {
    pub flags: RuntimeFlags,
    // Existing scheduler compatibility state is private and in-place. Fresh
    // runtime admits no short-return/saved context; its raw words stay zero.
    scheduler: ResetState,
    pub last_pc: u32,
    pub elapsed: u32,
    pub command_budget_bits: u32,
    pub actor_count: u32,
    pub frame_count: u32,
    pub actors: Option<Box<[RuntimeReferenceSlot<ActorReferenceId>]>>,
    pub frames: Option<Box<[RuntimeReferenceSlot<FrameReferenceId>]>>,
}
impl RuntimeFields {
    pub fn program_counter(&self) -> u32 {
        self.scheduler.program_counter
    }
    pub fn continuation(&self) -> bool {
        self.scheduler.continuation != 0
    }
}
impl RuntimeView<'_> {
    pub fn program_counter(&self) -> u32 {
        self.scheduler.program_counter
    }
    pub fn set_program_counter(&mut self, value: u32) {
        self.scheduler.program_counter = value;
    }
    pub fn continuation(&self) -> bool {
        self.scheduler.continuation != 0
    }
    pub fn set_continuation(&mut self, value: bool) {
        self.scheduler.continuation = u8::from(value);
    }
}
impl RuntimeFields {
    fn initialized() -> Self {
        Self {
            flags: RuntimeFlags::default(),
            scheduler: ResetState {
                program_counter: 0,
                short_stack_begin: 0,
                short_stack_end: 0,
                field_44: 0,
                continuation: 0,
            },
            last_pc: u32::MAX,
            elapsed: 0,
            command_budget_bits: 0x3f000000,
            actor_count: 0,
            frame_count: 0,
            actors: None,
            frames: None,
        }
    }
}
#[derive(Debug)]
pub struct OwnedInstruction {
    pub opcode: u32,
    pub parameter: Option<ScalarParameterSlot>,
}
#[derive(Debug, Default)]
pub struct InstructionStorage {
    records: Vec<OwnedInstruction>,
    logical_end: usize,
    native_capacity: usize,
}
impl InstructionStorage {
    pub fn active(&self) -> &[OwnedInstruction] {
        &self.records[..self.logical_end]
    }
    pub fn capacity(&self) -> usize {
        self.native_capacity
    }
}
#[derive(Debug)]
pub struct ProgramRuntime {
    pub stage: RuntimeStage,
    pub fields: RuntimeFields,
    pub instructions: InstructionStorage,
    pub working_source: Option<Vec<u8>>,
    pub pending: Option<OwnedInstruction>,
    pub diagnostic: CompileDiagnostic,
    pub compile_progress: Option<CompileProgress>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeEffect {
    AllocateWorkingSource(usize),
    ReleaseWorkingSource,
    AllocateScalar,
    ConvertScalar,
    AllocateRecords(usize),
    ReleaseOldRecordBacking,
    ReleaseScalar(u32),
    ReleaseActors,
    ReleaseFrames,
    AllocateActors(u32),
    AllocateFrames(u32),
}
/// Required external allocation boundary, borrowing actual fields at the call.
/// The runtime itself owns actual Rust allocations and performs release only
/// after callback success. No native allocator-pointer/layout equivalence.
/// This restricted backend cannot retire an instruction during its own borrow.
pub struct RuntimeView<'a> {
    pub flags: &'a mut RuntimeFlags,
    scheduler: &'a mut ResetState,
    pub last_pc: &'a mut u32,
    pub elapsed: &'a mut u32,
    pub command_budget_bits: &'a mut u32,
    pub actor_count: &'a mut u32,
    pub frame_count: &'a mut u32,
    pub actors: &'a mut Option<Box<[RuntimeReferenceSlot<ActorReferenceId>]>>,
    pub frames: &'a mut Option<Box<[RuntimeReferenceSlot<FrameReferenceId>]>>,
}
impl RuntimeFields {
    fn view(&mut self) -> RuntimeView<'_> {
        RuntimeView {
            flags: &mut self.flags,
            scheduler: &mut self.scheduler,
            last_pc: &mut self.last_pc,
            elapsed: &mut self.elapsed,
            command_budget_bits: &mut self.command_budget_bits,
            actor_count: &mut self.actor_count,
            frame_count: &mut self.frame_count,
            actors: &mut self.actors,
            frames: &mut self.frames,
        }
    }
}
pub trait RuntimeBackend {
    fn effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        published: Option<&ScalarParameter>,
    ) -> Result<(), String>;
}
#[derive(Debug, PartialEq, Eq)]
pub enum RuntimeError {
    UnsupportedDerivedRuntime(ProgramVtable),
    Domain(&'static str),
    Effect(String),
    /// A required destructive callback may already have released backing.
    /// This is host uncertainty, not a reconstructed native poison field.
    DestructiveEffect {
        effect: RuntimeEffect,
        detail: String,
    },
    UnsupportedParameters(u32),
    Parameter(String),
    Compile(String),
    UnsupportedCommand(u32),
    ReferenceCount(u32),
    Allocation,
    Scheduler(String),
}
impl ProgramRuntime {
    fn fresh() -> Self {
        Self {
            stage: RuntimeStage::Initializing,
            fields: RuntimeFields::initialized(),
            instructions: InstructionStorage::default(),
            working_source: None,
            pending: None,
            diagnostic: CompileDiagnostic {
                position: 0,
                length: 0,
            },
            compile_progress: None,
        }
    }
}
struct ParameterBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> ScalarParameterBackend for ParameterBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_scalar(&mut self) -> Result<Option<Box<ScalarParameter>>, RuntimeError> {
        self.backend
            .effect(self.fields.view(), RuntimeEffect::AllocateScalar, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(ScalarParameter::default())))
    }
    fn convert_integer(
        &mut self,
        token: &[u8],
        published: &ScalarParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .effect(
                self.fields.view(),
                RuntimeEffect::ConvertScalar,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|e| RuntimeError::Parameter(e.to_string()))
    }
}
struct Compiler<'a, B> {
    fields: &'a mut RuntimeFields,
    instructions: &'a mut InstructionStorage,
    pending: &'a mut Option<OwnedInstruction>,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> CompileBackend for Compiler<'_, B> {
    type Error = RuntimeError;
    fn clear_instructions(&mut self) -> Result<(), RuntimeError> {
        // Initial fresh domain: no payloads may be silently dropped.
        if !self.instructions.records.is_empty() {
            return Err(RuntimeError::Domain("nonempty compile reset unavailable"));
        }
        self.instructions.logical_end = 0;
        Ok(())
    }
    fn decode_and_append(
        &mut self,
        opcode: i32,
        parameters: &mut [u8],
    ) -> Result<bool, RuntimeError> {
        let opcode = opcode as u32;
        let zero_payload = crate::program_zero_parameters::supports_zero_parameter_opcode(opcode);
        *self.pending = Some(OwnedInstruction {
            opcode,
            parameter: (!zero_payload).then(ScalarParameterSlot::default),
        });
        let pending = self.pending.as_mut().unwrap();
        if zero_payload {
            crate::program_zero_parameters::decode_zero_parameters(opcode, parameters)
                .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?;
        } else {
            let mut boundary = ParameterBoundary {
                fields: self.fields,
                backend: self.backend,
            };
            match pending
                .parameter
                .as_mut()
                .unwrap()
                .decode(opcode, parameters, &mut boundary)
            {
                Ok(false) => {
                    *self.pending = None;
                    return Ok(false);
                }
                Ok(true) => {}
                Err(ScalarDecodeError::UnsupportedOpcode(id)) => {
                    return Err(RuntimeError::UnsupportedParameters(id));
                }
                Err(error) => return Err(RuntimeError::Parameter(format!("{error:?}"))),
            }
        }
        let storage = &mut self.instructions;
        if storage.logical_end == storage.native_capacity {
            let growth = storage.logical_end.max(1);
            let capacity = storage
                .logical_end
                .checked_add(growth)
                .ok_or(RuntimeError::Allocation)?;
            self.backend
                .effect(
                    self.fields.view(),
                    RuntimeEffect::AllocateRecords(capacity),
                    None,
                )
                .map_err(RuntimeError::Effect)?;
            storage
                .records
                .try_reserve_exact(capacity.saturating_sub(storage.records.len()))
                .map_err(|_| RuntimeError::Allocation)?;
            // Original copies records, then erases old8byte elements, frees old backing,
            // then publishes replacement. Rust Vec storage is actual typed ownership;
            // reserve relocation is not claimed native allocator failure-prefix parity.
            self.backend
                .effect(
                    self.fields.view(),
                    RuntimeEffect::ReleaseOldRecordBacking,
                    None,
                )
                .map_err(RuntimeError::Effect)?;
            storage.native_capacity = capacity;
        }
        storage.records.push(self.pending.take().unwrap());
        storage.logical_end += 1;
        Ok(true)
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct HostRuntimeDiscard {
    pub scalar_allocations: usize,
    pub actor_slots: usize,
    pub frame_slots: usize,
    pub working_source_bytes: usize,
}
impl CProgram {
    pub fn runtime(&self) -> Option<&ProgramRuntime> {
        self.runtime.as_ref()
    }
    /// First Init only. Allocation seedword40 is deliberately overwritten, not
    /// interpreted as an existing saved-code chain. Existing source stays immutable.
    pub fn init_fresh_runtime<B: RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<CompileProgress, RuntimeError> {
        // This bounded runtime supplies base C_program's virtual decoder and
        // dispatcher only. Derived program source lifetime remains supported,
        // but its different virtual behavior is outside this host admission.
        if self.vtable != ProgramVtable::Base {
            return Err(RuntimeError::UnsupportedDerivedRuntime(self.vtable));
        }
        if self.runtime.is_some() || self.runtime_started || self.phase != ProgramPhase::Source {
            return Err(RuntimeError::Domain("fresh source program required"));
        }
        self.runtime_started = true;
        self.runtime = Some(ProgramRuntime::fresh());
        let runtime = self.runtime.as_mut().unwrap();
        let Some(source) = self.source.as_ref() else {
            runtime.fields.command_budget_bits = 0;
            runtime.stage = RuntimeStage::Ready;
            runtime.compile_progress = Some(CompileProgress::default());
            return Ok(CompileProgress::default());
        };
        let source = source
            .c_bytes()
            .ok_or(RuntimeError::Domain("live source C string required"))?;
        let size = source
            .len()
            .checked_add(1)
            .ok_or(RuntimeError::Allocation)?;
        if let Err(e) = backend.effect(
            runtime.fields.view(),
            RuntimeEffect::AllocateWorkingSource(size),
            None,
        ) {
            runtime.stage = RuntimeStage::CompileInterrupted;
            return Err(RuntimeError::Effect(e));
        }
        let mut working = Vec::new();
        working
            .try_reserve_exact(size)
            .map_err(|_| RuntimeError::Allocation)?;
        working.extend_from_slice(source);
        working.push(0);
        runtime.working_source = Some(working);
        // Split actual fields; mutable source is still an owned field of CProgram's
        // runtime throughout callbacks. No source-only program shadow is constructed.
        let result = program_compiler::compile(
            runtime.working_source.as_deref_mut(),
            false,
            Some(&mut runtime.diagnostic),
            ParserLocale::AsciiC,
            &mut Compiler {
                fields: &mut runtime.fields,
                instructions: &mut runtime.instructions,
                pending: &mut runtime.pending,
                backend,
            },
        );
        let progress = match result {
            Ok(p) => p,
            Err(e) => {
                runtime.stage = RuntimeStage::CompileInterrupted;
                return Err(RuntimeError::Compile(format!("{e:?}")));
            }
        };
        runtime.compile_progress = Some(progress);
        if let Err(e) = backend.effect(
            runtime.fields.view(),
            RuntimeEffect::ReleaseWorkingSource,
            None,
        ) {
            runtime.stage = RuntimeStage::DisposalInterrupted;
            return Err(RuntimeError::Effect(e));
        }
        runtime.working_source = None;
        if runtime.instructions.logical_end == 0 {
            runtime.fields.command_budget_bits = 0;
        }
        runtime.stage = RuntimeStage::Ready;
        Ok(progress)
    }
    /// Original CallSubroutine (RVA0x6cbc0): reset the actual program's PC,
    /// empty-return cursor and owner, enable continuation, then synchronously
    /// enter its owned CallProcess with delta1. No diagnostic program or state
    /// copy is constructed. Derived virtual dispatch remains unadmitted.
    pub fn call_owned_subroutine<B: RuntimeBackend>(
        &mut self,
        budget: &mut DispatchBudget,
        backend: &mut B,
    ) -> Result<(), RuntimeError> {
        if self.vtable != ProgramVtable::Base {
            return Err(RuntimeError::UnsupportedDerivedRuntime(self.vtable));
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(RuntimeError::Domain("uninitialized program"))?;
        if runtime.stage != RuntimeStage::Ready {
            return Err(RuntimeError::Domain("runtime not ready"));
        }
        runtime.fields.scheduler.program_counter = 0;
        runtime.fields.scheduler.short_stack_end = runtime.fields.scheduler.short_stack_begin;
        // CProgram's typed owner is authoritative. The legacy scheduler word
        // stays private/zero in this empty-return, no-saved-program boundary.
        self.owner = None;
        runtime.fields.scheduler.field_44 = 0;
        runtime.fields.scheduler.continuation = 1;
        self.call_owned_process(1, budget, backend)
    }
    pub fn call_owned_process<B: RuntimeBackend>(
        &mut self,
        delta: i32,
        budget: &mut DispatchBudget,
        backend: &mut B,
    ) -> Result<(), RuntimeError> {
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(RuntimeError::Domain("uninitialized program"))?;
        if runtime.stage != RuntimeStage::Ready {
            return Err(RuntimeError::Domain("runtime not ready"));
        }
        let RuntimeFields {
            flags,
            scheduler,
            last_pc,
            elapsed,
            command_budget_bits,
            actor_count,
            frame_count,
            actors,
            frames,
        } = &mut runtime.fields;
        let mut dispatch = Process {
            flags,
            instructions: &runtime.instructions,
            last_pc,
            elapsed,
            command_budget_bits,
            actor_count,
            frame_count,
            actors,
            frames,
            backend,
        };
        let result = run_scheduler(scheduler, &mut dispatch, delta, budget);
        if result.is_err() {
            runtime.stage = if matches!(
                &result,
                Err(crate::script_scheduler::SchedulerError::Backend(
                    RuntimeError::DestructiveEffect { .. }
                ))
            ) {
                RuntimeStage::DisposalInterrupted
            } else {
                RuntimeStage::ProcessInterrupted
            };
        }
        result.map_err(|error| match error {
            crate::script_scheduler::SchedulerError::Backend(
                error @ RuntimeError::DestructiveEffect { .. },
            ) => error,
            other => RuntimeError::Scheduler(format!("{other:?}")),
        })
    }
    /// Additional host cleanup of actual Rust allocations, not original Done.
    /// Source/name owners stay live for their ordinary explicit destructor.
    /// This consumes the fresh-runtime admission permanently, including after
    /// interrupted operations; no uncertain external free callback is retried.
    pub fn discard_runtime_host(&mut self) -> Result<HostRuntimeDiscard, RuntimeError> {
        let runtime = self
            .runtime
            .take()
            .ok_or(RuntimeError::Domain("no owned runtime"))?;
        let report = HostRuntimeDiscard {
            scalar_allocations: runtime
                .instructions
                .records
                .iter()
                .filter(|i| {
                    i.parameter
                        .as_ref()
                        .is_some_and(|p| p.parameter().is_some())
                })
                .count()
                + usize::from(
                    runtime
                        .pending
                        .as_ref()
                        .and_then(|i| i.parameter.as_ref())
                        .is_some_and(|p| p.parameter().is_some()),
                ),
            actor_slots: runtime.fields.actors.as_ref().map_or(0, |a| a.len()),
            frame_slots: runtime.fields.frames.as_ref().map_or(0, |a| a.len()),
            working_source_bytes: runtime.working_source.as_ref().map_or(0, |s| s.len()),
        };
        drop(runtime);
        Ok(report)
    }
    pub fn done_fresh_runtime<B: RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), RuntimeError> {
        let runtime = self
            .runtime
            .as_mut()
            .ok_or(RuntimeError::Domain("uninitialized program"))?;
        if !matches!(
            runtime.stage,
            RuntimeStage::Ready | RuntimeStage::ProcessInterrupted
        ) {
            return Err(RuntimeError::Domain(
                "Done requires completed Init; recovery unavailable",
            ));
        }
        // Fresh no saved-code/return-storage domain only. This is not destructor.
        if runtime.fields.scheduler.short_stack_begin != runtime.fields.scheduler.short_stack_end {
            return Err(RuntimeError::Domain(
                "nonempty short-return storage unavailable",
            ));
        }
        for index in (0..runtime.instructions.logical_end).rev() {
            let opcode = runtime.instructions.records[index].opcode;
            if runtime.instructions.records[index].parameter.is_some() {
                if let Err(e) = backend.effect(
                    runtime.fields.view(),
                    RuntimeEffect::ReleaseScalar(opcode),
                    runtime.instructions.records[index]
                        .parameter
                        .as_ref()
                        .and_then(|slot| slot.parameter()),
                ) {
                    runtime.stage = RuntimeStage::DisposalInterrupted;
                    return Err(RuntimeError::Effect(e));
                }
                runtime.instructions.records[index].parameter = None;
            }
        }
        runtime.instructions.logical_end = 0;
        // Native Done register release order is float, frame, actor; no float owner
        // can be produced by the initially admitted command subset.
        if runtime.fields.frames.is_some() {
            if let Err(e) =
                backend.effect(runtime.fields.view(), RuntimeEffect::ReleaseFrames, None)
            {
                runtime.stage = RuntimeStage::DisposalInterrupted;
                return Err(RuntimeError::Effect(e));
            }
            runtime.fields.frames = None;
        }
        runtime.fields.frame_count = 0;
        if runtime.fields.actors.is_some() {
            if let Err(e) =
                backend.effect(runtime.fields.view(), RuntimeEffect::ReleaseActors, None)
            {
                runtime.stage = RuntimeStage::DisposalInterrupted;
                return Err(RuntimeError::Effect(e));
            }
            runtime.fields.actors = None;
        }
        runtime.fields.actor_count = 0;
        runtime.stage = RuntimeStage::Done;
        Ok(())
    }
}
struct Process<'a, B> {
    flags: &'a mut RuntimeFlags,
    instructions: &'a InstructionStorage,
    last_pc: &'a mut u32,
    elapsed: &'a mut u32,
    command_budget_bits: &'a mut u32,
    actor_count: &'a mut u32,
    frame_count: &'a mut u32,
    actors: &'a mut Option<Box<[RuntimeReferenceSlot<ActorReferenceId>]>>,
    frames: &'a mut Option<Box<[RuntimeReferenceSlot<FrameReferenceId>]>>,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> Process<'_, B> {
    fn value_at(&self, state: &ResetState) -> Result<i32, RuntimeError> {
        self.instructions
            .active()
            .get(state.program_counter as usize)
            .and_then(|i| i.parameter.as_ref())
            .and_then(|p| p.parameter())
            .and_then(|p| p.value())
            .ok_or(RuntimeError::Domain("current scalar parameter unavailable"))
    }
    fn effect(
        &mut self,
        state: &mut ResetState,
        effect: RuntimeEffect,
    ) -> Result<(), RuntimeError> {
        self.backend
            .effect(
                RuntimeView {
                    flags: self.flags,
                    scheduler: state,
                    last_pc: self.last_pc,
                    elapsed: self.elapsed,
                    command_budget_bits: self.command_budget_bits,
                    actor_count: self.actor_count,
                    frame_count: self.frame_count,
                    actors: self.actors,
                    frames: self.frames,
                },
                effect,
                None,
            )
            .map_err(|detail| match effect {
                RuntimeEffect::ReleaseActors | RuntimeEffect::ReleaseFrames => {
                    RuntimeError::DestructiveEffect { effect, detail }
                }
                _ => RuntimeError::Effect(detail),
            })
    }
}
impl<B: RuntimeBackend> SchedulerBackend for Process<'_, B> {
    type Error = RuntimeError;
    fn instructions(&self) -> Result<Option<InstructionView>, RuntimeError> {
        Ok(Some(InstructionView {
            allocation: 0,
            count: self.instructions.logical_end as u32,
        }))
    }
    fn opcode(&self, allocation: u32, index: u32) -> Result<u32, RuntimeError> {
        if allocation != 0 {
            return Err(RuntimeError::Domain("foreign instruction storage"));
        }
        self.instructions
            .active()
            .get(index as usize)
            .map(|i| i.opcode)
            .ok_or(RuntimeError::Domain("instruction index"))
    }
    fn dispatch(&mut self, state: &mut ResetState, delta: i32) -> Result<(), RuntimeError> {
        *self.elapsed = self.elapsed.wrapping_add(delta as u32);
        let instruction = &self.instructions.active()[state.program_counter as usize];
        match instruction.opcode {
            34 => {
                let value = self.value_at(state)?;
                state.continuation = u8::from(value != 0);
            }
            11 | 12 => {
                let value = self.value_at(state)?;
                let requested = value as u32;
                let actors = instruction.opcode == 12;
                let current = if actors {
                    *self.actor_count
                } else {
                    *self.frame_count
                };
                if requested == 0 || (current as i32) < value {
                    let allocated = if actors {
                        self.actors.is_some()
                    } else {
                        self.frames.is_some()
                    };
                    if allocated {
                        self.effect(
                            state,
                            if actors {
                                RuntimeEffect::ReleaseActors
                            } else {
                                RuntimeEffect::ReleaseFrames
                            },
                        )?;
                        if actors {
                            *self.actors = None
                        } else {
                            *self.frames = None
                        }
                    }
                    // Original reloads current PC/instruction parameter after release callback.
                    let requested = self.value_at(state)? as u32;
                    if actors {
                        *self.actor_count = requested
                    } else {
                        *self.frame_count = requested
                    }
                    if requested != 0 {
                        // Host allocation admission is reached only after the
                        // native free/pointer-clear/live reload/count store.
                        if requested > 65536 {
                            return Err(RuntimeError::ReferenceCount(requested));
                        }
                        self.effect(
                            state,
                            if actors {
                                RuntimeEffect::AllocateActors(requested)
                            } else {
                                RuntimeEffect::AllocateFrames(requested)
                            },
                        )?;
                        if actors {
                            *self.actors = Some(
                                vec![RuntimeReferenceSlot::Uninitialized; requested as usize]
                                    .into_boxed_slice(),
                            )
                        } else {
                            *self.frames = Some(
                                vec![RuntimeReferenceSlot::Uninitialized; requested as usize]
                                    .into_boxed_slice(),
                            )
                        }
                        // Native publishes allocation then rereads live count for zero fill.
                        let zero_count = if actors {
                            *self.actor_count
                        } else {
                            *self.frame_count
                        };
                        // Original SHL2 then SHR2 converts the live32-bit
                        // count to the reached number of zeroed reference words.
                        let zero_count = zero_count & 0x3fff_ffff;
                        for index in 0..zero_count as usize {
                            if actors {
                                let slot = self.actors.as_mut().unwrap().get_mut(index).ok_or(
                                    RuntimeError::Domain("zero fill reached beyond allocation"),
                                )?;
                                *slot = RuntimeReferenceSlot::Empty;
                            } else {
                                let slot = self.frames.as_mut().unwrap().get_mut(index).ok_or(
                                    RuntimeError::Domain("zero fill reached beyond allocation"),
                                )?;
                                *slot = RuntimeReferenceSlot::Empty;
                            }
                        }
                    }
                }
            }
            opcode => return Err(RuntimeError::UnsupportedCommand(opcode)),
        }
        *self.last_pc = state.program_counter;
        *self.elapsed = 0;
        state.program_counter = state.program_counter.wrapping_add(1);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
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
}
