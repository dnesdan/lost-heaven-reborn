//! Bounded fresh C_program runtime owned by the actual source program.
//!
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! Init RVA0x61580, Done RVA0x616a0, Compile RVA0x6c770, CallProcess RVA0x6ccb0.
//! Fresh compilation also owns the named payload family 13/18/43. Instruction
//! recognition retains its existing ASCII admission.
//! Process also executes FINDFRAME/FINDACTOR with an explicit borrowed lookup world.
//! Recording and remaining unsupported commands still stop when reached.
//! Fresh compilation admits base and entity sources (entity word-count
//! admission only); detector sources stay refused and no derived dispatch runs:
//! entity opcodes stop at UnsupportedCommand and CallSubroutine stays base-only.
//! Unsupported dependencies retain reached state and never become no-op success.
//! Saved-code chains, repeated Init and runtime-owning source destruction remain
//! outside admission. Explicit host discard releases actual Rust allocations;
//! it is additional cleanup, not original Done or a mission completion result.
use super::*;
#[path = "program_runtime_lookup.rs"]
mod lookup;
pub use lookup::{LookupRuntimeBackend, RuntimeLookupWorld};
#[path = "program_act_state_parameters.rs"]
mod act_state_parameters;
#[path = "program_event_parameters.rs"]
mod event_parameters;
#[path = "program_flow_parameters.rs"]
mod flow_parameters;
#[path = "program_human_talk_parameters.rs"]
mod human_talk_parameters;
#[path = "program_name_parameters.rs"]
mod name_parameters;
#[path = "program_pair_parameters.rs"]
mod pair_parameters;
#[path = "program_quad_parameters.rs"]
mod quad_parameters;
use crate::named_script::ResetState;
use crate::program_compiler::{self, CompileBackend, CompileDiagnostic, CompileProgress};
use crate::program_entity_parameters::{
    supports_entity_opcode, EntityParameter, EntityParameterError, EntityParameterSlot,
};
use crate::program_parameters::{
    ascii_integer_prefix, ScalarDecodeError, ScalarParameter, ScalarParameterBackend,
    ScalarParameterSlot,
};
use crate::program_parser::ParserLocale;
use crate::script_scheduler::{
    run_scheduler, ActorReferenceId, DispatchBudget, FrameReferenceId, InstructionView,
    SchedulerBackend,
};
pub use act_state_parameters::{
    supports_act_state_opcode, ActState, ActStateDecodeError, ActStateParameter,
    ActStateParameterBackend, ActStateParameterSlot, AsciiActStateParameterBackend,
};
pub use event_parameters::{
    decode_event_id, supports_event_opcode, supports_set_event_opcode, AsciiEventParameterBackend,
    AsciiSetEventParameterBackend, EventDecodeError, EventParameter, EventParameterBackend,
    EventParameterSlot, SetEventDecodeError, SetEventParameter, SetEventParameterBackend,
    SetEventParameterSlot,
};
pub use flow_parameters::{
    supports_flow_opcode, AsciiFlowParameterBackend, FlowDecodeError, FlowOperand, FlowParameter,
    FlowParameterBackend, FlowParameterSlot, GotoParameter, IfOp, IfParameter, LetOp, LetParameter,
};
pub use human_talk_parameters::{
    supports_human_talk_opcode, AsciiHumanTalkParameterBackend, HumanTalkDecodeError,
    HumanTalkParameter, HumanTalkParameterBackend, HumanTalkParameterSlot,
};
pub use name_parameters::{
    supports_name_opcode, NameDecodeError, NameParameter, NameParameterBackend, NameParameterSlot,
};
pub use pair_parameters::{
    supports_pair_opcode, AsciiPairParameterBackend, PairDecodeError, PairParameter,
    PairParameterBackend, PairParameterSlot,
};
pub use quad_parameters::{
    supports_quad_opcode, AsciiQuadParameterBackend, QuadDecodeError, QuadParameter,
    QuadParameterBackend, QuadParameterSlot,
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
    pub float_count: u32,
    pub floats: Option<Box<[f32]>>,
    pub gosub_stack: Vec<u32>,
    pub command_time: u32,
    pub event_command_time: u32,
    pub event_time: u32,
    pub rand_state: u32,
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
            float_count: 0,
            floats: None,
            gosub_stack: Vec::new(),
            command_time: 0,
            event_command_time: 0,
            event_time: 0,
            // The C runtime seeds rand() from the host clock at GameInit;
            // that lifecycle is not implemented, so scripts observe the CRT
            // default sequence deterministically instead of host time.
            rand_state: 1,
        }
    }
}
/// The payload is owned once by its instruction, including a pending compile.
/// A named payload cannot also masquerade as a scalar allocation.
#[derive(Debug)]
pub enum InstructionParameter {
    Scalar(ScalarParameterSlot),
    Name(NameParameterSlot),
    Pair(PairParameterSlot),
    Flow(FlowParameterSlot),
    Entity(EntityParameterSlot),
    Event(EventParameterSlot),
    SetEvent(SetEventParameterSlot),
    HumanTalk(HumanTalkParameterSlot),
    Quad(QuadParameterSlot),
    ActState(ActStateParameterSlot),
}
impl InstructionParameter {
    /// Scalar-only accessor retained for existing runtime diagnostics.
    pub fn parameter(&self) -> Option<&ScalarParameter> {
        match self {
            Self::Scalar(slot) => slot.parameter(),
            Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn name_parameter(&self) -> Option<&NameParameter> {
        match self {
            Self::Scalar(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
            Self::Name(slot) => slot.parameter(),
        }
    }
    pub fn pair_parameter(&self) -> Option<&PairParameter> {
        match self {
            Self::Pair(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn flow_parameter(&self) -> Option<&FlowParameter> {
        match self {
            Self::Flow(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn entity_parameter(&self) -> Option<&EntityParameter> {
        match self {
            Self::Entity(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn event_parameter(&self) -> Option<&EventParameter> {
        match self {
            Self::Event(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn set_event_parameter(&self) -> Option<&SetEventParameter> {
        match self {
            Self::SetEvent(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::HumanTalk(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn human_talk_parameter(&self) -> Option<&HumanTalkParameter> {
        match self {
            Self::HumanTalk(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn human_talk_parameter_mut(&mut self) -> Option<&mut HumanTalkParameter> {
        match self {
            Self::HumanTalk(slot) => slot.parameter_mut(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::Quad(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn quad_parameter(&self) -> Option<&QuadParameter> {
        match self {
            Self::Quad(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::ActState(_) => None,
        }
    }
    pub fn act_state_parameter(&self) -> Option<&ActStateParameter> {
        match self {
            Self::ActState(slot) => slot.parameter(),
            Self::Scalar(_)
            | Self::Name(_)
            | Self::Pair(_)
            | Self::Flow(_)
            | Self::Entity(_)
            | Self::Event(_)
            | Self::SetEvent(_)
            | Self::HumanTalk(_)
            | Self::Quad(_) => None,
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
    AllocatePair,
    ConvertPair(usize),
    ReleasePair(u32),
    AllocateQuad,
    ConvertQuad(usize),
    ReleaseQuad(u32),
    AllocateActState,
    ConvertActStateActor,
    ConvertActState,
    ReleaseActState(u32),
    AllocateEvent,
    ConvertEventId,
    ConvertEventExtra,
    ReleaseEvent(u32),
    AllocateSetEvent,
    ConvertSetEventRegister,
    ConvertSetEventId,
    ReleaseSetEvent(u32),
    AllocateHumanTalk,
    ConvertHumanTalk(usize),
    ReleaseHumanTalk(u32),
    AllocateFlow,
    ConvertFlowOperand(usize),
    ReleaseFlow(u32),
    AllocateRecords(usize),
    ReleaseOldRecordBacking,
    ReleaseScalar(u32),
    ReleaseActors,
    ReleaseFrames,
    AllocateActors(u32),
    AllocateFrames(u32),
    AllocateFloats(u32),
    ReleaseFloats,
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
    pub float_count: &'a mut u32,
    pub floats: &'a mut Option<Box<[f32]>>,
    pub gosub_stack: &'a mut Vec<u32>,
    pub command_time: &'a mut u32,
    pub event_command_time: &'a mut u32,
    pub event_time: &'a mut u32,
    pub rand_state: &'a mut u32,
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
            float_count: &mut self.float_count,
            floats: &mut self.floats,
            gosub_stack: &mut self.gosub_stack,
            command_time: &mut self.command_time,
            event_command_time: &mut self.event_command_time,
            event_time: &mut self.event_time,
            rand_state: &mut self.rand_state,
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
    ) -> Result<&mut dyn crate::script_lookup::LookupWorldBackend, crate::script_lookup::LookupError>
    {
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
    /// Paired numeric parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn pair_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&PairParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
    /// Event parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn event_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&EventParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
    /// HumanTalk parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn human_talk_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&HumanTalkParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
    /// SetEvent parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn set_event_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&SetEventParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
    /// Four-integer parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn quad_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&QuadParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
    /// Act-state parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn act_state_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&ActStateParameter>,
    ) -> Result<(), String> {
        self.effect(fields, effect, None)
    }
    /// Control-flow parameters have their own actual instruction-owned payload.
    /// A default backend still observes the required allocation/conversion/free.
    fn flow_parameter_effect(
        &mut self,
        fields: RuntimeView<'_>,
        effect: RuntimeEffect,
        _published: Option<&FlowParameter>,
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
struct PairBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> PairParameterBackend for PairBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_pair(&mut self) -> Result<Option<Box<PairParameter>>, RuntimeError> {
        self.backend
            .pair_parameter_effect(self.fields.view(), RuntimeEffect::AllocatePair, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(PairParameter::default())))
    }
    fn convert_integer(
        &mut self,
        index: usize,
        token: &[u8],
        published: &PairParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .pair_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertPair(index),
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|error| RuntimeError::Parameter(error.to_string()))
    }
}
struct QuadBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> QuadParameterBackend for QuadBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_quad(&mut self) -> Result<Option<Box<QuadParameter>>, RuntimeError> {
        self.backend
            .quad_parameter_effect(self.fields.view(), RuntimeEffect::AllocateQuad, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(QuadParameter::default())))
    }
    fn convert_integer(
        &mut self,
        index: usize,
        token: &[u8],
        published: &QuadParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .quad_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertQuad(index),
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|error| RuntimeError::Parameter(error.to_string()))
    }
}
struct EventBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> EventParameterBackend for EventBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_event(&mut self) -> Result<Option<Box<EventParameter>>, RuntimeError> {
        self.backend
            .event_parameter_effect(self.fields.view(), RuntimeEffect::AllocateEvent, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(EventParameter::default())))
    }
    fn convert_event_id(
        &mut self,
        token: &[u8],
        published: &EventParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .event_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertEventId,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        decode_event_id(token).map_err(|e| RuntimeError::Parameter(e.to_string()))
    }
    fn convert_event_extra(
        &mut self,
        token: &[u8],
        published: &EventParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .event_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertEventExtra,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|e| RuntimeError::Parameter(e.to_string()))
    }
}
struct SetEventBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> SetEventParameterBackend for SetEventBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_set_event(&mut self) -> Result<Option<Box<SetEventParameter>>, RuntimeError> {
        self.backend
            .set_event_parameter_effect(self.fields.view(), RuntimeEffect::AllocateSetEvent, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(SetEventParameter::default())))
    }
    fn convert_set_event_register(
        &mut self,
        token: &[u8],
        published: &SetEventParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .set_event_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertSetEventRegister,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|e| RuntimeError::Parameter(e.to_string()))
    }
    fn convert_set_event_id(
        &mut self,
        token: &[u8],
        published: &SetEventParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .set_event_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertSetEventId,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|e| RuntimeError::Parameter(e.to_string()))
    }
}
struct HumanTalkBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> HumanTalkParameterBackend for HumanTalkBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_human_talk(&mut self) -> Result<Option<Box<HumanTalkParameter>>, RuntimeError> {
        self.backend
            .human_talk_parameter_effect(self.fields.view(), RuntimeEffect::AllocateHumanTalk, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(HumanTalkParameter::default())))
    }
    fn convert_human_talk(
        &mut self,
        index: usize,
        token: &[u8],
        published: &HumanTalkParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .human_talk_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertHumanTalk(index),
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|e| RuntimeError::Parameter(e.to_string()))
    }
}
struct ActStateBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> ActStateParameterBackend for ActStateBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_act_state(&mut self) -> Result<Option<Box<ActStateParameter>>, RuntimeError> {
        self.backend
            .act_state_parameter_effect(self.fields.view(), RuntimeEffect::AllocateActState, None)
            .map_err(RuntimeError::Effect)?;
        Ok(Some(Box::new(ActStateParameter::default())))
    }
    fn convert_actor(
        &mut self,
        token: &[u8],
        published: &ActStateParameter,
    ) -> Result<i32, RuntimeError> {
        self.backend
            .act_state_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertActStateActor,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        ascii_integer_prefix(token).map_err(|error| RuntimeError::Parameter(error.to_string()))
    }
    fn convert_state(
        &mut self,
        token: &[u8],
        published: &ActStateParameter,
    ) -> Result<ActState, RuntimeError> {
        self.backend
            .act_state_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertActState,
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        if token.eq_ignore_ascii_case(b"ACTIVE") {
            Ok(ActState::Active)
        } else if token.eq_ignore_ascii_case(b"INACTIVE") {
            Ok(ActState::Inactive)
        } else {
            Ok(ActState::None)
        }
    }
}
struct FlowBoundary<'a, B> {
    fields: &'a mut RuntimeFields,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> FlowParameterBackend for FlowBoundary<'_, B> {
    type Error = RuntimeError;
    fn allocate_flow(&mut self) -> Result<bool, RuntimeError> {
        self.backend
            .flow_parameter_effect(self.fields.view(), RuntimeEffect::AllocateFlow, None)
            .map_err(RuntimeError::Effect)?;
        Ok(true)
    }
    fn convert_operand(
        &mut self,
        index: usize,
        token: &mut [u8],
        published: &FlowParameter,
    ) -> Result<Option<FlowOperand>, RuntimeError> {
        self.backend
            .flow_parameter_effect(
                self.fields.view(),
                RuntimeEffect::ConvertFlowOperand(index),
                Some(published),
            )
            .map_err(RuntimeError::Effect)?;
        flow_parameters::decode_flow_operand(token)
            .map_err(|error| RuntimeError::Parameter(error.to_string()))
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
            } else if supports_pair_opcode(opcode) {
                Some(InstructionParameter::Pair(PairParameterSlot::default()))
            } else if supports_flow_opcode(opcode) {
                Some(InstructionParameter::Flow(FlowParameterSlot::default()))
            } else if supports_event_opcode(opcode) {
                Some(InstructionParameter::Event(EventParameterSlot::default()))
            } else if supports_set_event_opcode(opcode) {
                Some(InstructionParameter::SetEvent(
                    SetEventParameterSlot::default(),
                ))
            } else if supports_human_talk_opcode(opcode) {
                Some(InstructionParameter::HumanTalk(
                    HumanTalkParameterSlot::default(),
                ))
            } else if supports_quad_opcode(opcode) {
                Some(InstructionParameter::Quad(QuadParameterSlot::default()))
            } else if supports_act_state_opcode(opcode) {
                Some(InstructionParameter::ActState(
                    ActStateParameterSlot::default(),
                ))
            } else if supports_entity_opcode(opcode) {
                Some(InstructionParameter::Entity(EntityParameterSlot::default()))
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
                InstructionParameter::Pair(slot) => {
                    let mut boundary = PairBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
                InstructionParameter::Flow(slot) => {
                    let mut boundary = FlowBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
                InstructionParameter::Entity(slot) => {
                    slot.decode(opcode, parameters)
                        .map_err(|error| match error {
                            EntityParameterError::UnsupportedOpcode(id) => {
                                RuntimeError::UnsupportedParameters(id)
                            }
                            other => RuntimeError::Parameter(format!("{other:?}")),
                        })?;
                    true
                }
                InstructionParameter::Event(slot) => {
                    let mut boundary = EventBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
                InstructionParameter::SetEvent(slot) => {
                    let mut boundary = SetEventBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
                InstructionParameter::HumanTalk(slot) => {
                    let mut boundary = HumanTalkBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
                InstructionParameter::Quad(slot) => {
                    let mut boundary = QuadBoundary {
                        fields: self.fields,
                        backend: self.backend,
                    };
                    slot.decode(opcode, parameters, &mut boundary)
                        .map_err(|error| RuntimeError::Parameter(format!("{error:?}")))?
                }
                InstructionParameter::ActState(slot) => {
                    let mut boundary = ActStateBoundary {
                        fields: self.fields,
                        backend: self.backend,
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
    pub pair_allocations: usize,
    pub flow_allocations: usize,
    pub event_allocations: usize,
    pub set_event_allocations: usize,
    pub human_talk_allocations: usize,
    pub quad_allocations: usize,
    pub act_state_allocations: usize,
    pub actor_slots: usize,
    pub frame_slots: usize,
    pub float_slots: usize,
    pub working_source_bytes: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub enum SetEventOutcome {
    NoEvent,
    Started,
    Busy,
}
impl CProgram {
    /// Original C_program::SetEvent (GAME 0x46cd90): search from the last
    /// instruction for the last EVENT of that id, check the gosub stack for
    /// an existing event, then jump and set the event times and actor state.
    /// Returns the outcome for the caller and world to act on. This handles
    /// the program-owned stack and times only; the caller's world handles the
    /// actor SetActState and any global game time snapshot.
    pub fn try_set_event(&mut self, event_id: i32) -> Result<SetEventOutcome, String> {
        let runtime = match &mut self.runtime {
            Some(runtime) => runtime,
            None => return Ok(SetEventOutcome::NoEvent),
        };
        // Search for the last EVENT with that id (reverse search, first hit
        // from the end wins, matching the native while(i--) scan).
        let mut found: Option<usize> = None;
        for (index, instruction) in runtime.instructions.active().iter().enumerate().rev() {
            if instruction.opcode == 73 {
                if let Some(param) = instruction
                    .parameter
                    .as_ref()
                    .and_then(|p| p.event_parameter())
                {
                    if param.id() == Some(event_id) {
                        found = Some(index);
                        break;
                    }
                }
            }
        }
        let found = match found {
            Some(idx) => idx,
            None => return Ok(SetEventOutcome::NoEvent),
        };
        // Already inside an event if any stack entry has GOSUB_EVENT (0x8000).
        if runtime
            .fields
            .gosub_stack
            .iter()
            .any(|entry| entry & 0x8000 != 0)
        {
            return Ok(SetEventOutcome::Busy);
        }
        // CallBack(CB_EVENT_BEGIN) is a no-op in the current reconstruction
        // (detector base is empty, entity is a stub that just logs).
        let cur = runtime.fields.scheduler.program_counter;
        runtime.fields.gosub_stack.push(cur | 0x8000 | 0x4000);
        runtime.fields.scheduler.program_counter = found as u32;
        runtime.fields.event_command_time = runtime.fields.command_time;
        // Global game time is not yet reconstructed; snapshot as 0. RETURN's
        // time restoration will be provisional (event_command_time only) until
        // the Game tick loop provides a global clock.
        runtime.fields.event_time = 0;
        runtime.fields.command_time = 0;
        // The native SetActState(ACTSTATE_0) on the program's actor is
        // handled by the world after this call (the program owns no actor
        // state directly).
        Ok(SetEventOutcome::Started)
    }
    pub fn runtime(&self) -> Option<&ProgramRuntime> {
        self.runtime.as_ref()
    }
    /// First Init only. Allocation seedword40 is deliberately overwritten, not
    /// interpreted as an existing saved-code chain. Existing source stays immutable.
    pub fn init_fresh_runtime<B: RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<CompileProgress, RuntimeError> {
        // Base programs decode with the 398-keyword table; entity programs
        // decode embedded AI sources with the entity table plus base fallback
        // (word-count admission only). Detector programs stay refused, and no
        // vtable executes derived dispatch: entity opcodes stop at
        // UnsupportedCommand and CallSubroutine stays base-only.
        let decoder = match self.vtable {
            ProgramVtable::Base => crate::program_instructions::InstructionDecoder::Base,
            ProgramVtable::Entity => crate::program_instructions::InstructionDecoder::Entity,
            _ => return Err(RuntimeError::UnsupportedDerivedRuntime(self.vtable)),
        };
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
            decoder,
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
    /// copy is constructed. Entity CallProcess remains unadmitted: compiled
    /// entity sources stop at UnsupportedCommand on dispatch and here.
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
        // The native subroutine entry clears the return stack; the float bank,
        // command clock and random sequence persist across the call.
        runtime.fields.gosub_stack.clear();
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
            float_count,
            floats,
            gosub_stack,
            command_time,
            event_command_time,
            event_time,
            rand_state,
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
            float_count,
            floats,
            gosub_stack,
            command_time,
            event_command_time,
            event_time,
            rand_state,
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
            crate::script_scheduler::SchedulerError::Backend(error @ RuntimeError::Lookup(_)) => {
                error
            }
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
            pair_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.pair_parameter().is_some())
                })
                .count(),
            flow_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.flow_parameter().is_some())
                })
                .count(),
            event_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.event_parameter().is_some())
                })
                .count(),
            set_event_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.set_event_parameter().is_some())
                })
                .count(),
            human_talk_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.human_talk_parameter().is_some())
                })
                .count(),
            quad_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.quad_parameter().is_some())
                })
                .count(),
            act_state_allocations: runtime
                .instructions
                .records
                .iter()
                .chain(runtime.pending.iter())
                .filter(|instruction| {
                    instruction
                        .parameter
                        .as_ref()
                        .is_some_and(|payload| payload.act_state_parameter().is_some())
                })
                .count(),
            actor_slots: runtime.fields.actors.as_ref().map_or(0, |a| a.len()),
            frame_slots: runtime.fields.frames.as_ref().map_or(0, |a| a.len()),
            float_slots: runtime.fields.floats.as_ref().map_or(0, |f| f.len()),
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
                    InstructionParameter::Pair(slot) => backend.pair_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleasePair(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::Flow(slot) => backend.flow_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseFlow(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::Event(slot) => backend.event_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseEvent(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::SetEvent(slot) => backend.set_event_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseSetEvent(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::HumanTalk(slot) => backend.human_talk_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseHumanTalk(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::Quad(slot) => backend.quad_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseQuad(opcode),
                        slot.parameter(),
                    ),
                    InstructionParameter::ActState(slot) => backend.act_state_parameter_effect(
                        runtime.fields.view(),
                        RuntimeEffect::ReleaseActState(opcode),
                        slot.parameter(),
                    ),
                    // Entity payloads are pure host word copies: decode runs
                    // no backend allocation boundary, so disposal is host
                    // cleanup only, unlike the backend-allocated families.
                    InstructionParameter::Entity(_) => Ok(()),
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
        if runtime.fields.floats.is_some() {
            if let Err(e) =
                backend.effect(runtime.fields.view(), RuntimeEffect::ReleaseFloats, None)
            {
                runtime.stage = RuntimeStage::DisposalInterrupted;
                return Err(RuntimeError::Effect(e));
            }
            runtime.fields.floats = None;
        }
        runtime.fields.float_count = 0;
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
    float_count: &'a mut u32,
    floats: &'a mut Option<Box<[f32]>>,
    gosub_stack: &'a mut Vec<u32>,
    command_time: &'a mut u32,
    event_command_time: &'a mut u32,
    event_time: &'a mut u32,
    rand_state: &'a mut u32,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> Process<'_, B> {
    /// Original actor-register read shared by register-addressed commands:
    /// negative or out-of-range registers and empty slots resolve to no
    /// actor, matching the native register-validity guards.
    fn actor_register(&self, register: i32) -> Option<ActorReferenceId> {
        if register < 0 {
            return None;
        }
        match self.actors.as_deref()?.get(register as usize)? {
            RuntimeReferenceSlot::Reference(actor) => Some(*actor),
            _ => None,
        }
    }
    /// Current ACT_SETSTATE payload: the actor register (negative selects the
    /// script's own actor) and the decoded state keyword.
    fn act_state_payload(&self, state: &ResetState) -> Result<(i32, ActState), RuntimeError> {
        let parameter = self
            .instructions
            .active()
            .get(state.program_counter as usize)
            .and_then(|instruction| instruction.parameter.as_ref())
            .and_then(|parameter| parameter.act_state_parameter())
            .ok_or(RuntimeError::Domain(
                "current act-state parameter unavailable",
            ))?;
        let actor = parameter
            .actor()
            .ok_or(RuntimeError::Domain("current act-state actor unavailable"))?;
        let state = parameter
            .state()
            .ok_or(RuntimeError::Domain("current act-state value unavailable"))?;
        Ok((actor, state))
    }
    /// Current SETEVENT payload: actor register, event id and unquoted label.
    fn set_event_payload(&self, state: &ResetState) -> Result<(i32, i32, Box<[u8]>), RuntimeError> {
        let parameter = self
            .instructions
            .active()
            .get(state.program_counter as usize)
            .and_then(|instruction| instruction.parameter.as_ref())
            .and_then(|parameter| parameter.set_event_parameter())
            .ok_or(RuntimeError::Domain(
                "current set-event parameter unavailable",
            ))?;
        let register = parameter.register().ok_or(RuntimeError::Domain(
            "current set-event register unavailable",
        ))?;
        let event = parameter
            .event()
            .ok_or(RuntimeError::Domain("current set-event id unavailable"))?;
        let label = parameter
            .label()
            .ok_or(RuntimeError::Domain("current set-event label unavailable"))?
            .to_vec()
            .into_boxed_slice();
        Ok((register, event, label))
    }
    #[allow(dead_code)]
    /// Current HumanTalk payload: actor register, speech id, kind and channel.
    fn human_talk_payload(&self, state: &ResetState) -> Result<[i32; 4], RuntimeError> {
        let parameter = self
            .instructions
            .active()
            .get(state.program_counter as usize)
            .and_then(|instruction| instruction.parameter.as_ref())
            .and_then(|parameter| parameter.human_talk_parameter())
            .ok_or(RuntimeError::Domain(
                "current human-talk parameter unavailable",
            ))?;
        let values = parameter.values();
        if values.iter().all(|v| v.is_some()) {
            Ok([
                values[0].unwrap(),
                values[1].unwrap(),
                values[2].unwrap(),
                values[3].unwrap(),
            ])
        } else {
            Err(RuntimeError::Domain(
                "current human-talk values unavailable",
            ))
        }
    }
    /// Current four-integer payload (TIMERON/HUMAN_ADDWEAPON and group).
    fn quad_payload(&self, state: &ResetState) -> Result<[i32; 4], RuntimeError> {
        let parameter = self
            .instructions
            .active()
            .get(state.program_counter as usize)
            .and_then(|instruction| instruction.parameter.as_ref())
            .and_then(|parameter| parameter.quad_parameter())
            .ok_or(RuntimeError::Domain("current quad parameter unavailable"))?;
        let values = parameter.values();
        if values.iter().all(|value| value.is_some()) {
            Ok([
                values[0].unwrap(),
                values[1].unwrap(),
                values[2].unwrap(),
                values[3].unwrap(),
            ])
        } else {
            Err(RuntimeError::Domain("current quad values unavailable"))
        }
    }
    /// C_entity::Process owner-command dispatch (GAME 0x51fda0) for the
    /// commands whose effects are fully evidenced: WAIT parks on the flag
    /// protocol, VIDIM/SHUT_UP/NAPRDELIVAUTE/FOLLOWPLAYER apply their words.
    /// The actor stays the authoritative state owner: every value is loaded
    /// through the lookup backend and round-tripped back after the command.
    /// WAIT's timer branch and the 0x2000/0x200 advance protocol have no
    /// producer yet; parked waits stay, which matches the native wait while
    /// its game condition is unmet.
    fn dispatch_entity(
        &mut self,
        state: &mut ResetState,
        opcode: u32,
    ) -> Result<Advance, RuntimeError> {
        let owner = *self.owner;
        let world = self.backend.lookup_world().map_err(RuntimeError::Lookup)?;
        let mut process = world
            .load_entity_process_state(owner)
            .map_err(RuntimeError::Lookup)?;
        let advance = match opcode {
            0x1004 => {
                if process.process_flags & 0x20 == 0 {
                    if process.process_flags & 0x10 == 0 {
                        Advance::Stay
                    } else {
                        Advance::Next
                    }
                } else {
                    Advance::Stay
                }
            }
            0x101a | 0x1031 | 0x103a | 0x103b => {
                let word = self
                    .instructions
                    .active()
                    .get(state.program_counter as usize)
                    .and_then(|instruction| instruction.parameter.as_ref())
                    .and_then(|parameter| parameter.entity_parameter())
                    .and_then(|parameter| parameter.word(0))
                    .ok_or(RuntimeError::Domain("current entity parameter unavailable"))?;
                match opcode {
                    0x101a => {
                        let bits = decode_entity_vidim_bits(word);
                        if bits == 0 {
                            process.visibility_bits &= !2;
                        } else if bits == 1 {
                            process.visibility_bits |= 2;
                        }
                    }
                    0x1031 => {
                        process.shut_up = u8::from(decode_entity_yes_no(word) == 0);
                    }
                    0x103a => {
                        if decode_entity_yes_no_other(word) == 1 {
                            process.process_flags |= 0x8000;
                        } else {
                            process.process_flags &= !0x8000;
                        }
                    }
                    _ => {
                        if decode_entity_yes_no_other(word) == 1 {
                            process.process_flags |= 0x10000;
                        } else {
                            process.process_flags &= !0x10000;
                        }
                    }
                }
                Advance::Next
            }
            _ => return Err(RuntimeError::UnsupportedCommand(opcode)),
        };
        self.backend
            .lookup_world()
            .map_err(RuntimeError::Lookup)?
            .store_entity_process_state(owner, &process)
            .map_err(RuntimeError::Lookup)?;
        Ok(advance)
    }
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
        let destructive = matches!(
            effect,
            RuntimeEffect::ReleaseActors | RuntimeEffect::ReleaseFrames
        );
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
                float_count: self.float_count,
                floats: self.floats,
                gosub_stack: self.gosub_stack,
                command_time: self.command_time,
                event_command_time: self.event_command_time,
                event_time: self.event_time,
                rand_state: self.rand_state,
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
    fn flow_payload(&self, state: &ResetState) -> Result<FlowParameter, RuntimeError> {
        self.instructions
            .active()
            .get(state.program_counter as usize)
            .and_then(|i| i.parameter.as_ref())
            .and_then(|p| p.flow_parameter())
            .cloned()
            .ok_or(RuntimeError::Domain("current flow parameter unavailable"))
    }

    /// Reverse label search from the last instruction, ASCII case-insensitive,
    /// matching NextCommand. A missing label is not an error.
    fn jump_to_label(&self, label: &[u8]) -> Option<u32> {
        for (index, instruction) in self.instructions.active().iter().enumerate().rev() {
            if instruction.opcode != 0 {
                continue;
            }
            if let Some(FlowParameter::Label(name)) = instruction
                .parameter
                .as_ref()
                .and_then(|p| p.flow_parameter())
            {
                if name.eq_ignore_ascii_case(label) {
                    return Some(index as u32);
                }
            }
        }
        None
    }
    fn leave_compact_block(&mut self, state: &mut ResetState, up: bool) -> bool {
        if !self.flags.in_block {
            return false;
        }
        let dir: i32 = match self.flags.block_mode {
            1 => -1,
            2 => 1,
            _ => {
                if up {
                    1
                } else {
                    -1
                }
            }
        };
        while let Some(instr) = self
            .instructions
            .active()
            .get(state.program_counter as usize)
        {
            let opcode = instr.opcode;
            if opcode == 70 && dir == -1 {
                break;
            }
            if opcode == 71 && dir == 1 {
                break;
            }
            let next = state.program_counter as i32 + dir;
            if next < 0 {
                state.program_counter = 0;
                break;
            }
            if next as usize >= self.instructions.active().len() {
                let len = self.instructions.active().len();
                if len == 0 {
                    break;
                }
                state.program_counter = (len - 1) as u32;
                break;
            }
            state.program_counter = next as u32;
        }
        true
    }

    /// Operand resolution shared by WAIT/LET/IF/GOTO: -1 is the literal, a
    /// valid index reads the live float bank. Any other index is a fail-closed
    /// domain error; the original reads out of bounds there, which safe code
    /// must not reproduce. Counts compare signed, matching the original ints.
    fn resolve_float(&self, register: i32, literal: f32) -> Result<f32, RuntimeError> {
        if register == -1 {
            return Ok(literal);
        }
        let num = *self.float_count as i32;
        if register >= 0 && register < num {
            return self
                .floats
                .as_ref()
                .and_then(|bank| bank.get(register as usize).copied())
                .ok_or(RuntimeError::Domain(
                    "float bank unavailable for admitted count",
                ));
        }
        Err(RuntimeError::Domain("float register outside admitted bank"))
    }

    /// Original `(int)(value + .5f)` truncation for WAIT/GOTO-indirect words.
    /// In-domain values truncate toward zero exactly like the C cast;
    /// out-of-domain inputs saturate where the original is undefined.
    fn wait_word(value: f32) -> u32 {
        (value + 0.5) as i32 as u32
    }

    /// Original `sprintf(label, "%i", ...)` register-indirect jump name.
    fn indirect_label(value: f32) -> String {
        ((value + 0.5) as i32).to_string()
    }
}
/// Post-dispatch program-counter movement. Mirrors NextCommand: every taken
/// path resets the command clock; only staying on WAIT preserves it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Advance {
    Next,
    Jump(u32),
    Stay,
}

/// Original MSVC rand() used by RND: linear congruential step 214013/2531011
/// with the high 15 bits out. RAND_MAX is 0x7fff.
fn msvc_rand_step(state: &mut u32) -> u32 {
    *state = state.wrapping_mul(214013).wrapping_add(2531011);
    (*state >> 16) & 0x7fff
}

/// Original `1.0f / RAND_MAX` multiplier, evaluated once as in the C
/// translation unit.
const RAND_MAX_RECIPROCAL: f32 = 1.0 / 32767.0;

/// Original LET division guard (ls3df.h MRG_ZERO): magnitudes below this
/// divide to zero instead of running the division.
const MRG_ZERO: f32 = 1e-8;

/// C_entity::DecodeParams yes/no word decoding shared by the SHUT_UP,
/// NAPRDELIVAUTE and FOLLOWPLAYER arms. "yes" decodes to 1, "no" to 0; any
/// other admitted word is fail-closed to 0 (the native fallthrough keeps an
/// incidentally matching value there, which no shipped line exercises).
fn decode_entity_yes_no(word: &[u8]) -> u32 {
    u32::from(word.eq_ignore_ascii_case(b"yes"))
}

/// C_entity::DecodeParams NAPRDELIVAUTE/FOLLOWPLAYER word decoding, including
/// the native fallthrough: "yes" stores 1, "no" stores 0, any other admitted
/// word keeps the preseeded 1.
fn decode_entity_yes_no_other(word: &[u8]) -> u32 {
    if word.eq_ignore_ascii_case(b"yes") {
        1
    } else if word.eq_ignore_ascii_case(b"no") {
        0
    } else {
        1
    }
}

/// C_entity::DecodeParams VIDIM word decoding: the comparator maps "0" and
/// "<|>" to 1.0 and everything else to 0.0, and the Process compares the
/// float bits, so only 0.0 clears the blind bit while the 1 branch is dead
/// by decode. Both stages are mirrored exactly.
fn decode_entity_vidim_bits(word: &[u8]) -> u32 {
    if word.eq_ignore_ascii_case(b"0") || word.eq_ignore_ascii_case(b"<|>") {
        0x3f800000
    } else {
        0
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
        *self.command_time = self.command_time.wrapping_add(delta as u32);
        let opcode = self.instructions.active()[state.program_counter as usize].opcode;
        let mut advance = Advance::Next;
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
            0 => {
                // LABEL has no Process case; the common tail advances past it.
            }
            1 | 2 => {
                if opcode == 2 {
                    self.gosub_stack.push(state.program_counter.wrapping_add(1));
                }
                let goto = match self.flow_payload(state)? {
                    FlowParameter::Goto(goto) => goto,
                    _ => {
                        return Err(RuntimeError::Domain("current goto parameter unavailable"));
                    }
                };
                if !goto.label.is_empty() {
                    if let Some(target) = self.jump_to_label(&goto.label) {
                        advance = Advance::Jump(target);
                    }
                } else {
                    // Register-indirect jumps always carry a register: the
                    // label form never leaves the label empty. Anything else
                    // would read out of bounds in the original.
                    if goto.target.register < 0 {
                        return Err(RuntimeError::Domain(
                            "register-indirect jump without register",
                        ));
                    }
                    let value = self.resolve_float(goto.target.register, goto.target.value)?;
                    let name = Self::indirect_label(value);
                    if let Some(target) = self.jump_to_label(name.as_bytes()) {
                        advance = Advance::Jump(target);
                    }
                }
            }
            3 => {
                if let Some(ret) = self.gosub_stack.pop() {
                    let is_event = ret & 0x8000 != 0;
                    let back_in_time = ret & 0x4000 != 0;
                    // CallBack(CB_EVENT_END) is a no-op in the current
                    // reconstruction (detector base empty, entity stub).
                    let target = ret & 0x3FFF;
                    // Native does NextCommand(NULL) (prev=cur, command_time=0, cur++)
                    // then cur=ret, then optional time restore and stop_at_return.
                    // We model the same: last_pc becomes RETURN's PC, command_time
                    // cleared, cur set to target, then LeaveCompactBlock check.
                    *self.last_pc = state.program_counter;
                    *self.elapsed = 0;
                    *self.command_time = 0;
                    state.program_counter = target;
                    if is_event && back_in_time && !self.leave_compact_block(state, false) {
                        // Delta GetGameTime - event_time omitted until global clock
                        *self.command_time = *self.event_command_time;
                    }
                    if self.flags.stop_at_return {
                        self.flags.stop_at_return = false;
                        state.continuation = 0;
                    }
                    return Ok(());
                }
            }
            4 => {
                let operand = match self.flow_payload(state)? {
                    FlowParameter::Wait(operand) => operand,
                    _ => {
                        return Err(RuntimeError::Domain("current wait parameter unavailable"));
                    }
                };
                let value = self.resolve_float(operand.register, operand.value)?;
                if *self.command_time < Self::wait_word(value) {
                    advance = Advance::Stay;
                }
            }
            6 => {
                // ClearAfterEnd needs the saved-code chain and actor owners,
                // which do not exist yet; the scheduler already stopped the
                // loop. Stay without advancing so a repeated call observes the
                // same end instead of running past it.
                advance = Advance::Stay;
            }
            7 => {
                let value = self.value_at(state)?;
                let requested = value as u32;
                let current = *self.float_count;
                if requested == 0 || (current as i32) < value {
                    let allocated = self.floats.is_some();
                    if allocated {
                        self.effect(state, RuntimeEffect::ReleaseFloats)?;
                        *self.floats = None;
                    }
                    // Original reloads current PC/instruction parameter after
                    // the release callback.
                    let requested = self.value_at(state)? as u32;
                    *self.float_count = requested;
                    if requested != 0 {
                        // Host allocation admission is reached only after the
                        // native free/count store, matching the bank arms.
                        if requested > 65536 {
                            return Err(RuntimeError::ReferenceCount(requested));
                        }
                        self.effect(state, RuntimeEffect::AllocateFloats(requested))?;
                        *self.floats = Some(vec![0.0f32; requested as usize].into_boxed_slice());
                    }
                }
            }
            8 => {
                let let_param = match self.flow_payload(state)? {
                    FlowParameter::Let(let_param) => let_param,
                    _ => {
                        return Err(RuntimeError::Domain("current let parameter unavailable"));
                    }
                };
                let num = *self.float_count as i32;
                let a = self.resolve_float(let_param.a.register, let_param.a.value)?;
                let b = self.resolve_float(let_param.b.register, let_param.b.value)?;
                if num > let_param.dest
                    && let_param.dest >= 0
                    && num > let_param.a.register
                    && num > let_param.b.register
                {
                    let result = match let_param.op {
                        LetOp::None => a,
                        LetOp::Add => a + b,
                        LetOp::Sub => a - b,
                        LetOp::Mul => a * b,
                        LetOp::Div => {
                            if b.abs() < MRG_ZERO {
                                0.0
                            } else {
                                a / b
                            }
                        }
                    };
                    let bank = self
                        .floats
                        .as_mut()
                        .ok_or(RuntimeError::Domain("float bank unavailable for store"))?;
                    let slot =
                        bank.get_mut(let_param.dest as usize)
                            .ok_or(RuntimeError::Domain(
                                "float destination outside admitted bank",
                            ))?;
                    *slot = result;
                }
            }
            9 => {
                let if_param = match self.flow_payload(state)? {
                    FlowParameter::If(if_param) => if_param,
                    _ => {
                        return Err(RuntimeError::Domain("current if parameter unavailable"));
                    }
                };
                let num = *self.float_count as i32;
                let a = self.resolve_float(if_param.a.register, if_param.a.value)?;
                let b = self.resolve_float(if_param.b.register, if_param.b.value)?;
                if num > if_param.a.register && num > if_param.b.register {
                    let result = match if_param.op {
                        IfOp::Less => a < b,
                        IfOp::Greater => a > b,
                        IfOp::Equal => a == b,
                        IfOp::NotEqual => a != b,
                    };
                    let label = if result {
                        &if_param.label1
                    } else {
                        &if_param.label2
                    };
                    // NextCommand falls through to the next instruction when
                    // the label is missing; only a found label jumps.
                    if let Some(target) = self.jump_to_label(label) {
                        advance = Advance::Jump(target);
                    }
                }
            }
            10 => {
                let pair = self
                    .instructions
                    .active()
                    .get(state.program_counter as usize)
                    .and_then(|i| i.parameter.as_ref())
                    .and_then(|p| p.pair_parameter())
                    .and_then(|p| p.values().into_iter().collect::<Option<Vec<_>>>())
                    .ok_or(RuntimeError::Domain("current rnd parameter unavailable"))?;
                let (reg, range) = (pair[0], pair[1]);
                if reg < 0 {
                    return Err(RuntimeError::Domain("rnd target outside admitted bank"));
                }
                let num = *self.float_count as i32;
                if num > reg {
                    // The original calls rnd(range) for range > 1 and the
                    // float rnd() otherwise; rnd(int) itself guards n < 1,
                    // which the range check already excludes here.
                    let sample = if range > 1 {
                        (msvc_rand_step(&mut *self.rand_state) % (range as u32)) as f32
                    } else {
                        msvc_rand_step(&mut *self.rand_state) as f32 * RAND_MAX_RECIPROCAL
                    };
                    let bank = self
                        .floats
                        .as_mut()
                        .ok_or(RuntimeError::Domain("float bank unavailable for store"))?;
                    let slot = bank
                        .get_mut(reg as usize)
                        .ok_or(RuntimeError::Domain("rnd target outside admitted bank"))?;
                    *slot = sample;
                }
            }
            26 => {
                // Original CMD_ACT_SETSTATE: the script's actor without a
                // register, else the register's actor. Unresolved actors
                // advance without effect; host defects propagate.
                let (register, act) = self.act_state_payload(state)?;
                // Out-of-range and negative registers keep the script's
                // owner; an in-range empty slot resolves to no actor (the
                // native overwrite with NULL), never back to the owner.
                let actor = if register < 0 {
                    *self.owner
                } else if let Some(slots) = self.actors.as_deref() {
                    if (register as usize) < slots.len() {
                        match &slots[register as usize] {
                            RuntimeReferenceSlot::Reference(actor) => Some(*actor),
                            _ => None,
                        }
                    } else {
                        *self.owner
                    }
                } else {
                    *self.owner
                };
                if let Some(actor) = actor {
                    // Upstream-assigned ACT_STATE_* decodes to ACTSTATE_2/0/1,
                    // not identity.
                    let native = match act {
                        ActState::None => 2,
                        ActState::Active => 0,
                        ActState::Inactive => 1,
                    };
                    self.backend
                        .lookup_world()
                        .map_err(RuntimeError::Lookup)?
                        .set_actor_act_state(actor, native)
                        .map_err(RuntimeError::Lookup)?;
                }
            }
            59 => {
                // Original CMD_HUMAN_ADDWEAPON: the register's actor must
                // resolve (no owner fallback); anything else advances without
                // effect. Model attach and weapon choice stay open.
                let values = self.quad_payload(state)?;
                let actor = if values[0] >= 0 {
                    self.actor_register(values[0])
                } else {
                    None
                };
                if let Some(actor) = actor {
                    self.backend
                        .lookup_world()
                        .map_err(RuntimeError::Lookup)?
                        .human_add_weapon(actor, values[1] as u16, values[2], values[3])
                        .map_err(RuntimeError::Lookup)?;
                }
            }
            73 => {
                // Original CMD_EVENT: no Process case; the common tail
                // advances past the marker. Treat as a no-op (like LABEL).
            }
            74 => {
                // Original CMD_SETEVENT: the event of an AI human's or
                // detector's script; where it can't begin now, jump to the
                // label. Bad or missing actor registers advance without effect;
                // non-entity/detector actors also advance; only the
                // target's SetEvent decides the jump.
                let (register, event_id, label) = self.set_event_payload(state)?;
                let target_actor = if register < 0 {
                    None
                } else {
                    self.actor_register(register)
                };
                if let Some(actor) = target_actor {
                    let ok = self
                        .backend
                        .lookup_world()
                        .map_err(RuntimeError::Lookup)?
                        .set_program_event(actor, event_id)
                        .map_err(RuntimeError::Lookup)?;
                    if ok {
                        advance = Advance::Next;
                    } else if !label.is_empty() {
                        if let Some(target) = self.jump_to_label(&label) {
                            advance = Advance::Jump(target);
                        }
                    }
                }
            }
            75 => {
                // Original CMD_EVENT_USE_CB: toggles GOSUB_EVENT_TIME on the
                // top of the gosub stack. Empty stack advances without effect.
                let value = self.value_at(state)?;
                if let Some(top) = self.gosub_stack.last_mut() {
                    if value != 0 {
                        *top |= 0x4000;
                    } else {
                        *top &= !0x4000;
                    }
                }
            }
            78 | 79 | 80 | 132 | 239 | 240 => {
                // Dialogue commands: DIALOG_BEGIN (78), DIALOG_END (79),
                // DIALOG_CAMSWITCH (80), HUMAN_TALK (132), INTRO_SUBTITLE_ADD
                // (239), SUBTITLE_ADD (240). Decode is admitted; execution
                // is a no-op advance for now (camera/dabing are stubs). The
                // talk channel persistence (v[3]) is preserved in the
                // instruction's HumanTalkParameter but not yet consumed.
            }
            0x1004 | 0x101a | 0x1031 | 0x103a | 0x103b => {
                advance = self.dispatch_entity(state, opcode)?;
            }
            0x1000..=0x103f => return Err(RuntimeError::UnsupportedCommand(opcode)),
            opcode => return Err(RuntimeError::UnsupportedCommand(opcode)),
        }
        *self.last_pc = state.program_counter;
        *self.elapsed = 0;
        match advance {
            Advance::Next => {
                *self.command_time = 0;
                state.program_counter = state.program_counter.wrapping_add(1);
            }
            Advance::Jump(target) => {
                *self.command_time = 0;
                state.program_counter = target;
            }
            Advance::Stay => {}
        }
        Ok(())
    }
}
#[cfg(test)]
#[path = "program_runtime_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "program_runtime_name_tests.rs"]
mod name_tests;
