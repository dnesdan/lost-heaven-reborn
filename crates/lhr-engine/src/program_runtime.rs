//! Bounded fresh C_program runtime owned by the actual source program.
//!
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! Init RVA0x61580, Done RVA0x616a0, Compile RVA0x6c770, CallProcess RVA0x6ccb0.
//! Fresh compilation also owns the named payload family 13/18/43. Instruction
//! recognition retains its existing ASCII admission.
//! Process also executes FINDFRAME/FINDACTOR with an explicit borrowed lookup world.
//! Recording and remaining unsupported commands still stop when reached.
//! Admission is explicitly base C_program only; derived Entity/Detector virtual
//! decoder/dispatcher execution is not supplied by this runtime.
//! Unsupported dependencies retain reached state and never become no-op success.
//! Saved-code chains, repeated Init and runtime-owning source destruction remain
//! outside admission. Explicit host discard releases actual Rust allocations;
//! it is additional cleanup, not original Done or a mission completion result.
use super::*;
#[path = "program_runtime_lookup.rs"]
mod lookup;
pub use lookup::{LookupRuntimeBackend, RuntimeLookupWorld};
#[path = "program_name_parameters.rs"]
mod name_parameters;
pub use name_parameters::{
    supports_name_opcode, NameDecodeError, NameParameter, NameParameterBackend, NameParameterSlot,
};
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
/// The payload is owned once by its instruction, including a pending compile.
/// A named payload cannot also masquerade as a scalar allocation.
#[derive(Debug)]
pub enum InstructionParameter {
    Scalar(ScalarParameterSlot),
    Name(NameParameterSlot),
}
impl InstructionParameter {
    /// Scalar-only accessor retained for existing runtime diagnostics.
    pub fn parameter(&self) -> Option<&ScalarParameter> {
        match self {
            Self::Scalar(slot) => slot.parameter(),
            Self::Name(_) => None,
        }
    }
    pub fn name_parameter(&self) -> Option<&NameParameter> {
        match self {
            Self::Scalar(_) => None,
            Self::Name(slot) => slot.parameter(),
        }
    }
}
#[derive(Debug)]
pub struct OwnedInstruction {
    pub opcode: u32,
    pub parameter: Option<InstructionParameter>,
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
    AllocateName,
    ConvertNameRegister,
    ReleaseName(u32),
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
    /// Required lookup world; allocation-only backends explicitly refuse lookup.
    fn lookup_world(
        &mut self,
    ) -> Result<&mut dyn crate::script_lookup::LookupWorldBackend, crate::script_lookup::LookupError> {
        Err(crate::script_lookup::LookupError::UnsupportedWorldBackend)
    }
    /// FINDACTOR diagnostic borrows the actual program state, including its owner.
    fn lookup_message_beep(
        &mut self,
        _fields: RuntimeView<'_>,
        _owner: &mut Option<ActorReferenceId>,
        _argument: i32,
    ) -> Result<(), crate::script_lookup::LookupError> {
        Err(crate::script_lookup::LookupError::Backend(
            "runtime lookup diagnostic unavailable".into(),
        ))
    }
    /// Named allocation/conversion/release uses the same required effect
    /// boundary. Backends may inspect its published payload by overriding this
    /// method; the default delegates, never manufactures effect success.
    fn name_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&NameParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
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
    Lookup(crate::script_lookup::LookupError),
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
struct NameBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
    opcode: u32,
}
impl<B: RuntimeBackend> NameParameterBackend for NameBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_name(&mut self) -> Result<Option<Box<NameParameter>>, RuntimeError> {
        self.backend
            .name_parameter_effect(self.fields.view(), RuntimeEffect::AllocateName, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(NameParameter::default())))
    }
    fn convert_register(
        &mut self,
        token: &[u8],
        published: &NameParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .name_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertNameRegister,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|error| RuntimeError::Parameter(error.to_string()))
    }
    fn release_rejected_name(&mut self, published: &NameParameter) -> Result<(), RuntimeError> {
        self.backend
            .name_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ReleaseName(self.opcode),
                Some(published),
            )
            .map_err(|detail| RuntimeError::DestructiveEffect {
                effect: RuntimeEffect::ReleaseName(self.opcode),
                detail,
            })
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
            parameter: if zero_payload {
                None
            } else if supports_name_opcode(opcode) {
                Some(InstructionParameter::Name(NameParameterSlot::default()))
            } else {
                Some(InstructionParameter::Scalar(ScalarParameterSlot::default()))
            },
        });
        let pending = self.pending.as_mut().unwrap();
        if zero_payload {
            crate::program_zero_parameters::decode_zero_parameters(opcode, parameters)
                .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?;
        } else {
            let accepted = match pending.parameter.as_mut().unwrap() {
                InstructionParameter::Scalar(slot) => {
                    let mut boundary = ParameterBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| match error {
                            ScalarDecodeError::UnsupportedOpcode(id) => {
                                RuntimeError::UnsupportedParameters(id)
                            }
                            other => RuntimeError::Parameter(format!("{other:?}")),
                        })?
                }
                InstructionParameter::Name(slot) => {
                    let mut boundary = NameBoundary {
                        fields: self.fields,
                        backend: self.backend,
                        opcode,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
            };
            if !accepted {
                *self.pending = None;
                return Ok(false);
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
    pub name_allocations: usize,
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
        // A diagnostic that unwinds must not leave executable state ready.
        runtime.stage = RuntimeStage::ProcessInterrupted;
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
            owner: &mut self.owner,
            stage: &mut runtime.stage,
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
        if result.is_ok() {
            runtime.stage = RuntimeStage::Ready;
        }
        result.map_err(|error| match error {
            crate::script_scheduler::SchedulerError::Backend(
                error @ RuntimeError::DestructiveEffect { .. },
            ) => error,
            crate::script_scheduler::SchedulerError::Backend(error @ RuntimeError::Lookup(_)) => error,
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
            name_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.name_parameter().is_some())
                })
                .count(),
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
        // Latch before the first external release. A host unwind may follow a
        // completed free just as an Err can; neither permits replay by Done.
        // RuntimeView exposes the actual fields, not this host admission state.
        runtime.stage = RuntimeStage::DisposalInterrupted;
        for index in (0..runtime.instructions.logical_end).rev() {
            let opcode = runtime.instructions.records[index].opcode;
            if let Some(payload) = runtime.instructions.records[index].parameter.as_ref() {
                let result = match payload {
                    InstructionParameter::Scalar(slot) => backend.effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseScalar(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::Name(slot) => backend.name_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseName(opcode),
                        slot.parameter(),
                    ),
                };
                if let Err(error) = result {
                    runtime.stage = RuntimeStage::DisposalInterrupted;
                    return Err(RuntimeError::Effect(error));
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
    owner: &'a mut Option<ActorReferenceId>,
    stage: &'a mut RuntimeStage,
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
        let destructive = matches!(effect, RuntimeEffect::ReleaseActors | RuntimeEffect::ReleaseFrames);
        if destructive {
            *self.stage = RuntimeStage::DisposalInterrupted;
        }
        let result = self.backend.effect(
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
        );
        if destructive && result.is_ok() {
            *self.stage = RuntimeStage::ProcessInterrupted;
        }
        result.map_err(|detail| {
            if destructive {
                RuntimeError::DestructiveEffect { effect, detail }
            } else {
                RuntimeError::Effect(detail)
            }
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
        let opcode = self.instructions.active()[state.program_counter as usize].opcode;
        match opcode {
            13 | 18 => self.dispatch_lookup(state, opcode == 18)?,
            34 => {
                let value = self.value_at(state)?;
                state.continuation = u8::from(value != 0);
            }
            11 | 12 => {
                let value = self.value_at(state)?;
                let requested = value as u32;
                let actors = opcode == 12;
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
#[path = "program_runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "program_runtime_name_tests.rs"]
mod name_tests;
