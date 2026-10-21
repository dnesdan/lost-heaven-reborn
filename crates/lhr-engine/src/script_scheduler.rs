//! Game.exe's live command scheduler (entry RVA 0x6ccb0), with a bounded
//! fixed-program adapter for selected base-dispatcher control and startup commands.
//! Names here are descriptive working names; no original Game symbols are claimed.
//! The adapter is not the original compiler, actor dispatcher, or full command VM.

use crate::named_script::{ProgramScheduler, ResetState};
use lhr_assets::script::parse_script;

/// Identity of retained eight-byte instruction storage, independent of its live length.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InstructionView {
    pub allocation: u32,
    pub count: u32,
}

/// Every read is live. A callback may rebase/shrink storage, replace its dispatch
/// table, or synchronously invoke `run_scheduler` again. Implementations must report
/// unavailable storage/callees, including allocations invalidated by callbacks.
pub trait SchedulerBackend {
    type Error;
    fn instructions(&self) -> Result<Option<InstructionView>, Self::Error>;
    fn opcode(&self, allocation: u32, index: u32) -> Result<u32, Self::Error>;
    /// Invoke the currently bound virtual slot +0x1c, rather than a cached callee.
    fn dispatch(&mut self, state: &mut ResetState, delta: i32) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchedulerError<E> {
    Backend(E),
    BudgetExhausted,
    InvalidBudget,
    InvalidCount(u32),
}

/// Additional project work limit. The native scheduler has no dispatch limit.
/// The caller may reuse it across sequential invocations. Reentrant callbacks
/// must independently bound their nested invocations; this API does not pass
/// the outer budget to them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DispatchBudget {
    remaining: usize,
}
impl DispatchBudget {
    pub fn new(dispatches: usize) -> Result<Self, SchedulerError<std::convert::Infallible>> {
        if dispatches == 0 || dispatches > 1_000_000 {
            return Err(SchedulerError::InvalidBudget);
        }
        Ok(Self {
            remaining: dispatches,
        })
    }
    pub fn remaining(&self) -> usize {
        self.remaining
    }
}

/// The native unsigned PC guard, END pre-clear, unchanged delta, and live rereads.
/// Null/empty/exhausted programs preserve continuation. A zero continuation does
/// not prevent the first dispatch. Errors preserve all changes already performed.
/// Valid backing spans are aligned eight-byte records below 2 GiB; malformed native
/// pointer arithmetic is outside this safe storage interface.
pub fn run_scheduler<B: SchedulerBackend>(
    state: &mut ResetState,
    backend: &mut B,
    delta: i32,
    budget: &mut DispatchBudget,
) -> Result<(), SchedulerError<B::Error>> {
    loop {
        let Some(view) = backend.instructions().map_err(SchedulerError::Backend)? else {
            return Ok(());
        };
        if view.count > 0x0fff_ffff {
            return Err(SchedulerError::InvalidCount(view.count));
        }
        if state.program_counter >= view.count {
            return Ok(());
        }
        // Safety limit is checked before the next dispatch's END pre-clear.
        if budget.remaining == 0 {
            return Err(SchedulerError::BudgetExhausted);
        }
        let opcode = backend
            .opcode(view.allocation, state.program_counter)
            .map_err(SchedulerError::Backend)?;
        if opcode == 6 {
            state.continuation = 0;
        }
        budget.remaining -= 1;
        backend
            .dispatch(state, delta)
            .map_err(SchedulerError::Backend)?;
        if state.continuation == 0 {
            return Ok(());
        }
    }
}

/// Original registry command names, with literal parameters only. Callers must
/// supply real world-effect implementations; the scheduler provides no no-op effects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupCommand {
    GameNightMission(bool),
    UseLightcache,
    PoliceManagerOn {
        argument_0: i32,
        argument_1_nonzero: bool,
    },
    EnableMap(bool),
}

pub trait ScriptEffects {
    type Error;
    /// Runs synchronously after elapsed-time accumulation and before the common
    /// live-PC advance. Mutations and errors remain visible to the caller.
    fn execute(
        &mut self,
        command: StartupCommand,
        control: &mut ControlState,
        state: &mut ResetState,
        delta: i32,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramError {
    Source(String),
    TooManyStatements,
    Allocation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExecutionError<E> {
    UnsupportedCommand { line: usize, command: String },
    InvalidProgramCounter(u32),
    InvalidInstructionAllocation(u32),
    InvalidControlWord(u16),
    InvalidReturnStorage,
    ReturnStackLimit,
    UnsupportedReturnFlags(u16),
    UnsupportedInterruptState,
    UnsupportedEndContext,
    Effects(E),
}

#[derive(Clone, Debug)]
enum Operation {
    Label(String),
    Goto(String),
    Gosub(String),
    Return,
    Wait(u32),
    CommandBlock(bool),
    End,
    Startup(StartupCommand),
    Unsupported(String),
}
impl Operation {
    fn opcode(&self) -> Option<u32> {
        Some(match self {
            Self::Label(_) => 0,
            Self::Goto(_) => 1,
            Self::Gosub(_) => 2,
            Self::Return => 3,
            Self::Wait(_) => 4,
            Self::End => 6,
            Self::CommandBlock(_) => 34,
            Self::Startup(StartupCommand::GameNightMission(_)) => 0x51,
            Self::Startup(StartupCommand::UseLightcache) => 0xb4,
            Self::Startup(StartupCommand::PoliceManagerOn { .. }) => 0x87,
            Self::Startup(StartupCommand::EnableMap(_)) => 0x139,
            Self::Unsupported(_) => return None,
        })
    }
}

/// A read-only executable subset of diagnostic source intake. Unsupported lines
/// remain explicit execution frontiers; they are never erased or executed as no-ops.
/// Original source bytes are not rewritten. This is not a claim of compiler parity.
#[derive(Clone, Debug)]
pub struct Program {
    operations: Vec<(usize, Operation)>,
}
impl Program {
    pub fn from_source(source: &str) -> Result<Self, ProgramError> {
        let parsed = parse_script(source).map_err(|e| ProgramError::Source(e.to_string()))?;
        if parsed.statements.len() > 16_383 {
            return Err(ProgramError::TooManyStatements);
        }
        let mut operations = Vec::new();
        operations
            .try_reserve_exact(parsed.statements.len())
            .map_err(|_| ProgramError::Allocation)?;
        let mut raw_lines = source.split('\n');
        let mut raw_line = 0;
        for statement in parsed.statements {
            let mut raw = "";
            while raw_line < statement.line {
                raw = raw_lines.next().unwrap_or("");
                raw_line += 1;
            }
            let canonical = !raw.contains("//")
                && !raw
                    .chars()
                    .any(|c| c.is_whitespace() && !matches!(c, ' ' | '\r'));
            let argument = statement.arguments.as_str();
            let unsupported = || Operation::Unsupported(statement.command.clone());
            let operation = if !canonical {
                unsupported()
            } else {
                match statement.command.as_str() {
                    "label" if literal_label(argument, 31) => Operation::Label(argument.to_owned()),
                    "goto" if literal_label(argument, 15) => Operation::Goto(argument.to_owned()),
                    "gosub" if literal_label(argument, 15) => Operation::Gosub(argument.to_owned()),
                    "return" if argument.is_empty() => Operation::Return,
                    "end" if argument.is_empty() => Operation::End,
                    "wait" => argument
                        .parse::<u32>()
                        .ok()
                        .filter(|&v| v <= 16_777_215)
                        .map_or_else(unsupported, Operation::Wait),
                    "commandblock" => argument
                        .parse::<i32>()
                        .map_or_else(|_| unsupported(), |v| Operation::CommandBlock(v != 0)),
                    "game_nightmission" => argument.parse::<i32>().map_or_else(
                        |_| unsupported(),
                        |v| Operation::Startup(StartupCommand::GameNightMission(v != 0)),
                    ),
                    "use_lightcache" if argument.is_empty() => {
                        Operation::Startup(StartupCommand::UseLightcache)
                    }
                    "enablemap" => argument.parse::<i32>().map_or_else(
                        |_| unsupported(),
                        |v| Operation::Startup(StartupCommand::EnableMap(v != 0)),
                    ),
                    "policemanager_on" => match integer_pair(argument) {
                        Some((argument_0, enabled)) => {
                            Operation::Startup(StartupCommand::PoliceManagerOn {
                                argument_0,
                                argument_1_nonzero: enabled != 0,
                            })
                        }
                        None => unsupported(),
                    },
                    _ => unsupported(),
                }
            };
            operations.push((statement.line, operation));
        }
        Ok(Self { operations })
    }
    pub fn len(&self) -> usize {
        self.operations.len()
    }
    pub fn is_empty(&self) -> bool {
        self.operations.is_empty()
    }
}

fn literal_label(argument: &str, maximum: usize) -> bool {
    !argument.is_empty()
        && argument.len() <= maximum
        && argument.is_ascii()
        && !argument
            .bytes()
            .any(|c| c.is_ascii_whitespace() || matches!(c, b',' | b'"' | b'[' | b']'))
}
fn integer_pair(argument: &str) -> Option<(i32, i32)> {
    let (a, b) = argument.split_once(',')?;
    Some((
        a.trim_matches(' ').parse().ok()?,
        b.trim_matches(' ').parse().ok()?,
    ))
}

/// Selected base-dispatcher fields. The short-stack addresses remain in ResetState;
/// retained return storage is never shortened when the logical native end shrinks.
#[derive(Clone, Debug)]
pub struct ControlState {
    pub last_program_counter: u32,
    pub elapsed: u32,
    pub control_word: u16,
    pub interrupt_on_return: u8,
    pub replacement_program_present: bool,
    return_storage: Vec<u16>,
}
impl ControlState {
    /// Explicit initial state, not an inferred native constructor. CW must describe
    /// the original caller's environment; scheduling itself never changes it.
    pub fn new(return_capacity: usize, control_word: u16) -> Result<Self, ProgramError> {
        if return_capacity > 1024 {
            return Err(ProgramError::Allocation);
        }
        let mut return_storage = Vec::new();
        return_storage
            .try_reserve_exact(return_capacity)
            .map_err(|_| ProgramError::Allocation)?;
        return_storage.resize(return_capacity, 0);
        Ok(Self {
            last_program_counter: u32::MAX,
            elapsed: 0,
            control_word,
            interrupt_on_return: 0,
            replacement_program_present: false,
            return_storage,
        })
    }
    pub fn return_storage(&self) -> &[u16] {
        &self.return_storage
    }
    pub fn return_storage_mut(&mut self) -> &mut [u16] {
        &mut self.return_storage
    }
    fn stack_count<E>(&self, state: &ResetState) -> Result<usize, ExecutionError<E>> {
        let bytes = state
            .short_stack_end
            .checked_sub(state.short_stack_begin)
            .ok_or(ExecutionError::InvalidReturnStorage)?;
        if bytes % 2 != 0 || bytes as usize / 2 > self.return_storage.len() {
            return Err(ExecutionError::InvalidReturnStorage);
        }
        Ok(bytes as usize / 2)
    }
    fn advance(&mut self, state: &mut ResetState) {
        self.last_program_counter = state.program_counter;
        self.elapsed = 0;
        state.program_counter = state.program_counter.wrapping_add(1);
    }
    fn jump(&mut self, state: &mut ResetState, program: &Program, label: &str) {
        self.advance(state);
        if let Some(index) = program.operations.iter().rposition(
            |(_, op)| matches!(op, Operation::Label(name) if name.eq_ignore_ascii_case(label)),
        ) {
            state.program_counter = index as u32;
        }
    }
}

/// Adapter for a retained fixed program. Generic live collection/table mutation is
/// supported by SchedulerBackend; this adapter deliberately owns no actor, variable
/// evaluator, replacement program, interrupt delivery, or world initialization.
pub struct ProgramAdapter<'a, E> {
    pub program: &'a Program,
    pub control: &'a mut ControlState,
    pub effects: &'a mut E,
    pub budget: &'a mut DispatchBudget,
}
impl<E: ScriptEffects> ProgramScheduler for ProgramAdapter<'_, E> {
    type Error = SchedulerError<ExecutionError<E::Error>>;
    fn schedule(&mut self, state: &mut ResetState, delta: i32) -> Result<(), Self::Error> {
        // Reborrow the budget separately so callback errors preserve exact state.
        let mut backend = ProgramBackend {
            program: self.program,
            control: self.control,
            effects: self.effects,
        };
        run_scheduler(state, &mut backend, delta, self.budget)
    }
}
/// Fixed-program virtual-dispatch backend, usable with the generic live scheduler.
pub struct ProgramBackend<'a, E> {
    pub program: &'a Program,
    pub control: &'a mut ControlState,
    pub effects: &'a mut E,
}
impl<E: ScriptEffects> SchedulerBackend for ProgramBackend<'_, E> {
    type Error = ExecutionError<E::Error>;
    fn instructions(&self) -> Result<Option<InstructionView>, Self::Error> {
        Ok(Some(InstructionView {
            allocation: 0,
            count: self.program.operations.len() as u32,
        }))
    }
    fn opcode(&self, allocation: u32, index: u32) -> Result<u32, Self::Error> {
        if allocation != 0 {
            return Err(ExecutionError::InvalidInstructionAllocation(allocation));
        }
        let (line, operation) = self
            .program
            .operations
            .get(index as usize)
            .ok_or(ExecutionError::InvalidProgramCounter(index))?;
        operation
            .opcode()
            .ok_or_else(|| ExecutionError::UnsupportedCommand {
                line: *line,
                command: match operation {
                    Operation::Unsupported(command) => command.clone(),
                    _ => unreachable!(),
                },
            })
    }
    fn dispatch(&mut self, state: &mut ResetState, delta: i32) -> Result<(), Self::Error> {
        let (_, operation) = self
            .program
            .operations
            .get(state.program_counter as usize)
            .ok_or(ExecutionError::InvalidProgramCounter(state.program_counter))?;
        self.control.elapsed = self.control.elapsed.wrapping_add(delta as u32);
        match operation {
            Operation::Label(_) => self.control.advance(state),
            Operation::Goto(label) => self.control.jump(state, self.program, label),
            Operation::Gosub(label) => {
                let count = self.control.stack_count(state)?;
                if count == self.control.return_storage.len() {
                    return Err(ExecutionError::ReturnStackLimit);
                }
                let end = state
                    .short_stack_end
                    .checked_add(2)
                    .ok_or(ExecutionError::InvalidReturnStorage)?;
                self.control.return_storage[count] = (state.program_counter as u16).wrapping_add(1);
                state.short_stack_end = end;
                self.control.jump(state, self.program, label);
            }
            Operation::Return => {
                let count = self.control.stack_count(state)?;
                if count == 0 {
                    self.control.advance(state);
                } else {
                    let target = self.control.return_storage[count - 1];
                    if target & 0xc000 != 0 {
                        return Err(ExecutionError::UnsupportedReturnFlags(target));
                    }
                    if self.control.interrupt_on_return != 0 {
                        return Err(ExecutionError::UnsupportedInterruptState);
                    }
                    state.short_stack_end -= 2;
                    self.control.advance(state);
                    state.program_counter = (target & 0x3fff) as u32;
                }
            }
            Operation::Wait(value) => {
                let threshold = wait_threshold(*value, self.control.control_word)?;
                if self.control.elapsed >= threshold {
                    self.control.advance(state);
                }
            }
            Operation::CommandBlock(enabled) => {
                state.continuation = u8::from(*enabled);
                self.control.advance(state);
            }
            Operation::End => {
                if state.field_44 != 0 || self.control.replacement_program_present {
                    return Err(ExecutionError::UnsupportedEndContext);
                }
                // Native ownerless END preserves PC/elapsed/lastPC; scheduler pre-clears continuation.
            }
            Operation::Startup(command) => {
                self.effects
                    .execute(*command, self.control, state, delta)
                    .map_err(ExecutionError::Effects)?;
                self.control.advance(state);
            }
            Operation::Unsupported(command) => {
                return Err(ExecutionError::UnsupportedCommand {
                    line: self.program.operations[state.program_counter as usize].0,
                    command: command.clone(),
                })
            }
        }
        Ok(())
    }
}

/// Exact result of F32(integer literal) + 0.5 under the caller's x87 precision /
/// rounding policy followed by the original truncating signed conversion. Only
/// this nonnegative exact-integer source domain is supported; no host float mode leaks in.
fn wait_threshold<E>(literal: u32, word: u16) -> Result<u32, ExecutionError<E>> {
    if word & 0x3f != 0x3f || word & 0x300 == 0x100 || word & !0x1f7f != 0 {
        return Err(ExecutionError::InvalidControlWord(word));
    }
    if word & 0x300 == 0 && literal >= 0x80_0000 {
        Ok(match (word >> 10) & 3 {
            0 => literal + (literal & 1),
            2 => literal + 1,
            _ => literal,
        })
    } else {
        Ok(literal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::named_script::reset_and_start;

    struct Effects {
        events: Vec<(StartupCommand, i32, u32, u32)>,
        fail: bool,
    }
    impl ScriptEffects for Effects {
        type Error = &'static str;
        fn execute(
            &mut self,
            command: StartupCommand,
            control: &mut ControlState,
            state: &mut ResetState,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.events
                .push((command, delta, state.program_counter, control.elapsed));
            if self.fail {
                state.program_counter = 7;
                control.elapsed = 99;
                Err("missing effect dependency")
            } else {
                Ok(())
            }
        }
    }
    fn state() -> ResetState {
        ResetState {
            program_counter: 0,
            short_stack_begin: 100,
            short_stack_end: 100,
            field_44: 0,
            continuation: 0,
        }
    }
    fn effects() -> Effects {
        Effects {
            events: Vec::new(),
            fail: false,
        }
    }

    #[test]
    fn reset_start_wait_continuation_and_end_match_original_sequence() {
        let program = Program::from_source("commandblock 0\nwait 17\nend").unwrap();
        let mut control = ControlState::new(8, 0x3f).unwrap();
        control.elapsed = 19;
        let mut effects = effects();
        let mut budget = DispatchBudget::new(32).unwrap();
        let mut adapter = ProgramAdapter {
            program: &program,
            control: &mut control,
            effects: &mut effects,
            budget: &mut budget,
        };
        let mut state = state();
        state.short_stack_end += 4;
        state.field_44 = 42;
        reset_and_start(&mut state, &mut adapter).unwrap();
        assert_eq!(
            (
                state.program_counter,
                state.continuation,
                state.short_stack_end,
                state.field_44
            ),
            (1, 0, 100, 0)
        );
        adapter.schedule(&mut state, 16).unwrap();
        assert_eq!((state.program_counter, adapter.control.elapsed), (1, 16));
        adapter.schedule(&mut state, 16).unwrap();
        assert_eq!(
            (
                state.program_counter,
                adapter.control.elapsed,
                adapter.control.last_program_counter
            ),
            (2, 0, 1)
        );
        adapter.schedule(&mut state, 16).unwrap();
        assert_eq!(
            (
                state.program_counter,
                state.continuation,
                adapter.control.elapsed,
                adapter.control.last_program_counter
            ),
            (2, 0, 16, 1)
        );
    }

    #[test]
    fn reset_repeats_identical_delta_until_wait_and_end() {
        let program = Program::from_source("wait 7\nend").unwrap();
        let mut control = ControlState::new(0, 0x37f).unwrap();
        let mut effects = effects();
        let mut budget = DispatchBudget::new(32).unwrap();
        let mut state = state();
        reset_and_start(
            &mut state,
            &mut ProgramAdapter {
                program: &program,
                control: &mut control,
                effects: &mut effects,
                budget: &mut budget,
            },
        )
        .unwrap();
        assert_eq!(
            (
                state.program_counter,
                state.continuation,
                control.elapsed,
                budget.remaining()
            ),
            (1, 0, 1, 24)
        );
    }

    #[test]
    fn subroutine_duplicate_labels_and_retained_short_stack() {
        let program = Program::from_source("gosub a\nend\nlabel A\nend\nlabel a\nreturn").unwrap();
        let mut control = ControlState::new(2, 0x3f).unwrap();
        let mut effects = effects();
        let mut budget = DispatchBudget::new(16).unwrap();
        let mut state = state();
        reset_and_start(
            &mut state,
            &mut ProgramAdapter {
                program: &program,
                control: &mut control,
                effects: &mut effects,
                budget: &mut budget,
            },
        )
        .unwrap();
        assert_eq!(
            (
                state.program_counter,
                state.short_stack_end,
                control.elapsed,
                control.last_program_counter
            ),
            (1, 100, 1, 5)
        );
        assert_eq!(control.return_storage()[0], 1);
    }

    #[test]
    fn startup_callbacks_are_required_and_failures_keep_partial_state() {
        let program = Program::from_source(
            "game_nightmission 256\nuse_lightcache\npolicemanager_on -1,0\nenablemap 0\nend",
        )
        .unwrap();
        let mut control = ControlState::new(0, 0x3f).unwrap();
        let mut effects = effects();
        let mut budget = DispatchBudget::new(16).unwrap();
        let mut state = state();
        reset_and_start(
            &mut state,
            &mut ProgramAdapter {
                program: &program,
                control: &mut control,
                effects: &mut effects,
                budget: &mut budget,
            },
        )
        .unwrap();
        assert_eq!(
            effects.events,
            vec![
                (StartupCommand::GameNightMission(true), 1, 0, 1),
                (StartupCommand::UseLightcache, 1, 1, 1),
                (
                    StartupCommand::PoliceManagerOn {
                        argument_0: -1,
                        argument_1_nonzero: false
                    },
                    1,
                    2,
                    1
                ),
                (StartupCommand::EnableMap(false), 1, 3, 1),
            ]
        );
        effects.fail = true;
        state.program_counter = 0;
        let error = ProgramAdapter {
            program: &program,
            control: &mut control,
            effects: &mut effects,
            budget: &mut budget,
        }
        .schedule(&mut state, 16)
        .unwrap_err();
        assert_eq!(
            error,
            SchedulerError::Backend(ExecutionError::Effects("missing effect dependency"))
        );
        assert_eq!((state.program_counter, control.elapsed), (7, 99));
    }

    #[test]
    fn unknown_noncanonical_and_dynamic_lines_remain_frontiers() {
        for source in [
            "wait flt[0]",
            "wait 1 // comment",
            "\twait 1",
            "enablemap 1,2",
            "policemanager_on 0",
            "actor_stand 1",
            "end extra",
        ] {
            let program = Program::from_source(source).unwrap();
            let mut control = ControlState::new(0, 0x3f).unwrap();
            let mut effects = effects();
            let mut budget = DispatchBudget::new(8).unwrap();
            let mut state = state();
            let error = ProgramAdapter {
                program: &program,
                control: &mut control,
                effects: &mut effects,
                budget: &mut budget,
            }
            .schedule(&mut state, 16)
            .unwrap_err();
            assert!(matches!(
                error,
                SchedulerError::Backend(ExecutionError::UnsupportedCommand { .. })
            ));
            assert_eq!(
                (state.program_counter, control.elapsed, budget.remaining()),
                (0, 0, 8)
            );
        }
    }

    #[test]
    fn original_wait_pc24_ties_and_other_precision_modes() {
        // Original-body observed threshold for the exact integer literal 8,388,609.
        for (cw, threshold) in [
            (0x003f, 8_388_610),
            (0x043f, 8_388_609),
            (0x083f, 8_388_610),
            (0x0c3f, 8_388_609),
            (0x023f, 8_388_609),
            (0x063f, 8_388_609),
            (0x0a3f, 8_388_609),
            (0x0e3f, 8_388_609),
            (0x033f, 8_388_609),
            (0x073f, 8_388_609),
            (0x0b3f, 8_388_609),
            (0x0f3f, 8_388_609),
        ] {
            let program = Program::from_source("wait 8388609\nend").unwrap();
            let mut control = ControlState::new(0, cw).unwrap();
            control.elapsed = 8_388_609;
            let mut effects = effects();
            let mut budget = DispatchBudget::new(8).unwrap();
            let mut state = state();
            ProgramAdapter {
                program: &program,
                control: &mut control,
                effects: &mut effects,
                budget: &mut budget,
            }
            .schedule(&mut state, 0)
            .unwrap();
            assert_eq!(
                state.program_counter,
                u32::from(threshold == 8_388_609),
                "CW {cw:04x}"
            );
        }
    }

    #[test]
    fn explicit_safety_and_unsupported_state_boundaries() {
        assert!(DispatchBudget::new(0).is_err());
        let program = Program::from_source("goto a\nlabel a\ngoto a").unwrap();
        let mut control = ControlState::new(0, 0x3f).unwrap();
        let mut effects = effects();
        let mut budget = DispatchBudget::new(3).unwrap();
        let mut state = state();
        let error = reset_and_start(
            &mut state,
            &mut ProgramAdapter {
                program: &program,
                control: &mut control,
                effects: &mut effects,
                budget: &mut budget,
            },
        )
        .unwrap_err();
        assert_eq!(error, SchedulerError::BudgetExhausted);
        assert_eq!((state.program_counter, state.continuation), (1, 1));
        let program = Program::from_source("return").unwrap();
        let mut control = ControlState::new(1, 0x3f).unwrap();
        control.return_storage_mut()[0] = 0x8001;
        state.program_counter = 0;
        state.short_stack_end = 102;
        let mut budget = DispatchBudget::new(2).unwrap();
        let error = ProgramAdapter {
            program: &program,
            control: &mut control,
            effects: &mut effects,
            budget: &mut budget,
        }
        .schedule(&mut state, -1)
        .unwrap_err();
        assert_eq!(
            error,
            SchedulerError::Backend(ExecutionError::UnsupportedReturnFlags(0x8001))
        );
        assert_eq!((state.short_stack_end, control.elapsed), (102, u32::MAX));
        assert!(matches!(
            wait_threshold::<()>(1, 0x013f),
            Err(ExecutionError::InvalidControlWord(0x013f))
        ));
    }
}
