//! Bounded control-flow subset established from the supplied PC executable with REA.
//! This is a standalone scheduler, without actor callbacks, nested replacement
//! programs, events, expressions, replay, audio or world effects. Unsupported source
//! stops at its original line. Delta values use the caller's original clock units.
use crate::{Error, Result, script::parse_script};
use std::collections::BTreeMap;

const MAX_PROGRAM: usize = 16_383;
const MAX_STACK: usize = 1_024;
const MAX_DISPATCH: usize = 1_000_000;

#[derive(Debug, Clone)]
enum Operation {
    Label,
    Goto(String),
    Gosub(String),
    Return,
    Wait(u32),
    CommandBlock(bool),
    End,
    Unsupported(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickStatus {
    /// A nonblocking instruction completed outside a command block.
    Yielded,
    /// WAIT has not reached its literal threshold outside a command block.
    Waiting,
    /// END remains the current instruction; no enclosing replacement program exists.
    Ended,
    /// The instruction pointer reached the end of the program.
    Complete,
    /// The project's dispatch budget interrupted a command block; state is retained.
    BudgetExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TickResult {
    pub status: TickStatus,
    pub dispatched: usize,
}

/// Executable support is deliberately narrower than read-only source intake.
/// LABEL/GOTO/GOSUB accept literal ASCII names without whitespace, WAIT accepts exact nonnegative
/// integer literals up to 16,777,215, and COMMANDBLOCK accepts an i32 literal.
pub struct ControlVm {
    operations: Vec<(usize, Operation)>,
    labels: BTreeMap<String, usize>,
    pc: usize,
    elapsed: u32,
    command_block: bool,
    stack: Vec<usize>,
}

impl ControlVm {
    pub fn new(source: &str) -> Result<Self> {
        let program = parse_script(source)?;
        if program.statements.len() > MAX_PROGRAM {
            return Err(Error(
                "control program exceeds 16383 statement quota".into(),
            ));
        }
        let mut operations = Vec::new();
        let mut labels = BTreeMap::new();
        let lines: Vec<_> = source.split('\n').collect();
        for statement in &program.statements {
            let argument = statement.arguments.as_str();
            let unsupported = || Operation::Unsupported(statement.command.clone());
            // Intake accepts a broader diagnostic grammar. The original compiler
            // only strips whole-line comments and leading ASCII spaces. Keep
            // transformed diagnostic lines outside this executable subset.
            let raw = lines[statement.line - 1];
            let canonical = !raw.contains("//")
                && !raw
                    .chars()
                    .any(|c| c.is_whitespace() && !matches!(c, ' ' | '\r'));
            let operation = match statement.command.as_str() {
                _ if !canonical => unsupported(),
                "label" if literal_label(argument, 31) => {
                    // The original jump helper searches from the end of the program.
                    labels.insert(argument.to_ascii_lowercase(), operations.len());
                    Operation::Label
                }
                "goto" if literal_label(argument, 15) => {
                    Operation::Goto(argument.to_ascii_lowercase())
                }
                "gosub" if literal_label(argument, 15) => {
                    Operation::Gosub(argument.to_ascii_lowercase())
                }
                "return" if argument.is_empty() => Operation::Return,
                "end" if argument.is_empty() => Operation::End,
                "wait" => match argument.parse::<u32>() {
                    Ok(value) if value <= 16_777_215 => Operation::Wait(value),
                    _ => unsupported(),
                },
                "commandblock" => match argument.parse::<i32>() {
                    Ok(value) => Operation::CommandBlock(value != 0),
                    _ => unsupported(),
                },
                _ => unsupported(),
            };
            operations.push((statement.line, operation));
        }
        Ok(Self {
            operations,
            labels,
            pc: 0,
            elapsed: 0,
            command_block: false,
            stack: Vec::new(),
        })
    }

    pub fn instruction_pointer(&self) -> usize {
        self.pc
    }
    pub fn elapsed_units(&self) -> u32 {
        self.elapsed
    }
    pub fn in_command_block(&self) -> bool {
        self.command_block
    }

    /// Run the original control scheduler with an additional project safety budget.
    /// Each dispatch receives the same delta, including repeated WAIT dispatches
    /// inside a command block. Instruction advance/jumps reset elapsed time, and
    /// unsigned elapsed accumulation wraps exactly at 32 bits. Unsupported commands
    /// return an error without advancing past them or applying their effects.
    pub fn tick(&mut self, delta_units: u32, budget: usize) -> Result<TickResult> {
        if budget == 0 || budget > MAX_DISPATCH {
            return Err(Error("dispatch budget must be within 1..=1000000".into()));
        }
        let mut dispatched = 0;
        loop {
            let Some((line, operation)) = self.operations.get(self.pc) else {
                return Ok(TickResult {
                    status: TickStatus::Complete,
                    dispatched,
                });
            };
            if dispatched == budget {
                return Ok(TickResult {
                    status: TickStatus::BudgetExhausted,
                    dispatched,
                });
            }
            if let Operation::Unsupported(command) = operation {
                return Err(Error(format!(
                    "unsupported program command {command} at line {line}"
                )));
            }
            self.elapsed = self.elapsed.wrapping_add(delta_units);
            dispatched += 1;
            let mut waiting = false;
            match operation {
                Operation::Label => self.advance(),
                Operation::Goto(label) => {
                    self.elapsed = 0;
                    self.pc = self.labels.get(label).copied().unwrap_or(self.pc + 1);
                }
                Operation::Gosub(label) => {
                    if self.stack.len() == MAX_STACK {
                        return Err(Error(format!(
                            "program return stack quota exceeded at line {line}"
                        )));
                    }
                    self.stack.push(self.pc + 1);
                    self.elapsed = 0;
                    self.pc = self.labels.get(label).copied().unwrap_or(self.pc + 1);
                }
                Operation::Return => {
                    self.elapsed = 0;
                    self.pc = self.stack.pop().unwrap_or(self.pc + 1);
                }
                Operation::Wait(threshold) => {
                    if self.elapsed >= *threshold {
                        self.advance();
                    } else {
                        waiting = true;
                    }
                }
                Operation::CommandBlock(enabled) => {
                    self.command_block = *enabled;
                    self.advance();
                }
                Operation::End => {
                    self.command_block = false;
                    return Ok(TickResult {
                        status: TickStatus::Ended,
                        dispatched,
                    });
                }
                Operation::Unsupported(_) => unreachable!(),
            }
            if !self.command_block {
                return Ok(TickResult {
                    status: if waiting {
                        TickStatus::Waiting
                    } else {
                        TickStatus::Yielded
                    },
                    dispatched,
                });
            }
        }
    }

    fn advance(&mut self) {
        self.pc += 1;
        self.elapsed = 0;
    }
}

fn literal_label(argument: &str, maximum: usize) -> bool {
    !argument.is_empty()
        && argument.len() <= maximum
        && argument.is_ascii()
        && !argument
            .bytes()
            .any(|c| c.is_ascii_control() || c.is_ascii_whitespace() || matches!(c, b',' | b'"'))
        && !argument.to_ascii_lowercase().starts_with("flt[")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn vm(source: &str) -> ControlVm {
        ControlVm::new(source).unwrap()
    }
    #[test]
    fn wait_threshold_repeats_and_unsigned_wrap_are_preserved() {
        let mut machine = vm("wait 10\nend");
        assert_eq!(machine.tick(9, 1).unwrap().status, TickStatus::Waiting);
        assert_eq!(machine.elapsed_units(), 9);
        assert_eq!(machine.tick(1, 1).unwrap().status, TickStatus::Yielded);
        assert_eq!(machine.elapsed_units(), 0);
        assert_eq!(machine.tick(0, 1).unwrap().status, TickStatus::Ended);
        assert_eq!(machine.instruction_pointer(), 1);
        assert_eq!(machine.tick(1, 1).unwrap().status, TickStatus::Ended);
        let mut wrapping = vm("wait 10");
        wrapping.elapsed = u32::MAX;
        assert_eq!(wrapping.tick(1, 1).unwrap().status, TickStatus::Waiting);
        assert_eq!(wrapping.elapsed_units(), 0);
    }
    #[test]
    fn command_block_reuses_delta_and_obeys_budget() {
        let mut machine = vm("commandblock 1\nwait 10\ncommandblock 0\nend");
        let result = machine.tick(3, 10).unwrap();
        assert_eq!(
            result,
            TickResult {
                status: TickStatus::Yielded,
                dispatched: 6
            }
        );
        assert_eq!(machine.instruction_pointer(), 3);
        assert!(!machine.in_command_block());
        let mut spin = vm("commandblock 1\nlabel repeat\ngoto repeat");
        assert_eq!(
            spin.tick(0, 20).unwrap().status,
            TickStatus::BudgetExhausted
        );
        assert!(spin.in_command_block());
        assert_eq!(spin.tick(0, 2).unwrap().status, TickStatus::BudgetExhausted);
        assert!(spin.tick(0, 0).is_err());
    }
    #[test]
    fn literal_jumps_use_last_case_insensitive_label_and_missing_falls_through() {
        let mut machine = vm("goto Mark\nlabel mark\nend\nlabel MARK\ngoto missing\nend");
        machine.tick(8, 1).unwrap();
        assert_eq!(machine.instruction_pointer(), 3);
        assert_eq!(machine.elapsed_units(), 0);
        machine.tick(8, 1).unwrap();
        machine.tick(8, 1).unwrap();
        assert_eq!(machine.instruction_pointer(), 5);
        assert_eq!(machine.tick(0, 1).unwrap().status, TickStatus::Ended);
    }
    #[test]
    fn subroutine_return_empty_return_and_explicit_unsupported_stop() {
        let mut machine = vm("gosub sub\nend\nlabel sub\nreturn");
        for _ in 0..3 {
            machine.tick(1, 1).unwrap();
        }
        assert_eq!(machine.instruction_pointer(), 1);
        assert_eq!(machine.tick(0, 1).unwrap().status, TickStatus::Ended);
        assert_eq!(vm("return").tick(0, 1).unwrap().status, TickStatus::Yielded);
        let mut blocked = vm("commandblock 1\nworld_action 1\nend");
        let error = blocked.tick(1, 3).unwrap_err();
        assert!(error.0.contains("world_action at line 2"));
        assert_eq!(blocked.instruction_pointer(), 1);
        for source in [
            "wait flt[0]",
            "wait 0.5",
            "wait -1",
            "goto flt[0]",
            "end extra",
            "goto missing // inspection comment",
            "\twait 1",
            "goto two names",
        ] {
            assert!(vm(source).tick(0, 1).unwrap_err().0.contains("unsupported"));
        }
    }
}
