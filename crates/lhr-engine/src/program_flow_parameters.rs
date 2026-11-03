//! Owned GOTO/GOSUB/WAIT/LET/IF/LABEL parameter payloads (opcodes 0, 1, 2, 4, 8, 9).
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_LABEL/GOTO/GOSUB/WAIT/LET/IF arms with the DecodeOperand,
//! DecodeLet and DecodeIf helpers (PC entries 0x461900/0x461aa0/0x461bd0), and
//! C_program.h S_operand/S_goto_params/S_let_params/S_if_params layouts.
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This module reconstructs source decoding only, not command execution,
//! native allocation failure, or original-runtime equivalence.
#![forbid(unsafe_code)]
use crate::program_parameters::{ascii_float_prefix, ascii_integer_prefix};
use crate::program_parser::{c_length, extrude_params, ExtrudedParams, ProgramParserError};

/// Operand of WAIT, GOTO-target, LET and IF: a float-bank register or a
/// literal. Mirrors S_operand { int reg; float value; }; reg -1 is a literal.
/// Operands the original never resolves keep value zero: the original leaves
/// the field uninitialized, which safe code must not reproduce.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowOperand {
    pub register: i32,
    pub value: f32,
}

/// Original DecodeOperand under the ASCII/C-locale admission: trim spaces in
/// place, then compare `flt[` case-insensitively. A register operand
/// overwrites the first `]` with NUL and writes the register index, leaving
/// value zero. A literal writes register -1 and its atof-prefix value.
/// A missing `]` returns None after trimming, without writing the outputs.
/// An empty operand is a zero literal; the original reads next to its NUL
/// there, which this port refuses to reproduce.
pub fn decode_flow_operand(source: &mut [u8]) -> Result<Option<FlowOperand>, ProgramParserError> {
    let length = c_length(source)?;
    if let Some((offset, &byte)) = source[..length]
        .iter()
        .enumerate()
        .find(|(_, byte)| !byte.is_ascii())
    {
        return Err(ProgramParserError::UnsupportedCrtByte { offset, byte });
    }
    let mut start = 0;
    while start < length && source[start] == b' ' {
        start += 1;
    }
    let mut end = length;
    while end > start && source[end - 1] == b' ' {
        end -= 1;
        source[end] = 0;
    }
    let token = &source[start..end];
    if token.len() < 4 || !token[..4].eq_ignore_ascii_case(b"flt[") {
        return Ok(Some(FlowOperand {
            register: -1,
            value: ascii_float_prefix(token)?,
        }));
    }
    let inside = start + 4;
    let Some(relative_end) = source[inside..end].iter().position(|&byte| byte == b']') else {
        return Ok(None);
    };
    let close = inside + relative_end;
    source[close] = 0;
    Ok(Some(FlowOperand {
        register: ascii_integer_prefix(&source[inside..close])?,
        value: 0.0,
    }))
}

/// GOTO/GOSUB target: a label, or a register holding the command to go to.
/// Mirrors S_goto_params { S_operand target; char label[16]; }; label is empty
/// for the register form, whose target value stays zero as documented above.
#[derive(Debug, Clone, PartialEq)]
pub struct GotoParameter {
    pub label: Box<[u8]>,
    pub target: FlowOperand,
}

/// LET destination, operands and operator. Mirrors S_let_params; b is the
/// zero literal for LET_OP_NONE, when the original never decodes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LetOp {
    None,
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LetParameter {
    pub dest: i32,
    pub a: FlowOperand,
    pub b: FlowOperand,
    pub op: LetOp,
}

/// IF operands, comparison and branch labels. Mirrors S_if_params; both
/// labels are at most 15 bytes, matching the original length admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IfOp {
    Less,
    Greater,
    Equal,
    NotEqual,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfParameter {
    pub a: FlowOperand,
    pub b: FlowOperand,
    pub op: IfOp,
    pub label1: Box<[u8]>,
    pub label2: Box<[u8]>,
}

/// One instruction-owned flow payload. Labels keep their exact trimmed bytes;
/// the original LABEL arm copies into a 32-byte field and overflows longer
/// names, which this port refuses to reproduce.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowParameter {
    Label(Box<[u8]>),
    Goto(GotoParameter),
    Wait(FlowOperand),
    Let(LetParameter),
    If(IfParameter),
}

/// Actual host allocation and required conversion boundaries, mirroring the
/// scalar/name/pair arms. Structural source shapes are parsed before the
/// allocation is published; every conversion the original performs runs
/// through its own callback in original order, so an interrupted conversion
/// retains the completed prefix. Conversions the original skips (LET's b
/// without an operator, GOTO's target for the label form) run no callback.
/// A callback returning None maps the native ignored-DecodeOperand result:
/// LET/IF substitute the zero literal and continue, while GOTO/WAIT reject
/// the instruction, exactly where the original checks or ignores the result.
pub trait FlowParameterBackend {
    type Error;
    fn allocate_flow(&mut self) -> Result<bool, Self::Error>;
    fn convert_operand(
        &mut self,
        index: usize,
        token: &mut [u8],
        published: &FlowParameter,
    ) -> Result<Option<FlowOperand>, Self::Error>;
}

/// Concrete ASCII C-locale boundary. Host allocation supplies the actual
/// parameter storage; conversions decode in the caller's working source, as
/// the original writes its NUL splits into the parameter buffer.
#[derive(Default)]
pub struct AsciiFlowParameterBackend;

impl FlowParameterBackend for AsciiFlowParameterBackend {
    type Error = ProgramParserError;

    fn allocate_flow(&mut self) -> Result<bool, Self::Error> {
        Ok(true)
    }

    fn convert_operand(
        &mut self,
        _index: usize,
        token: &mut [u8],
        _published: &FlowParameter,
    ) -> Result<Option<FlowOperand>, Self::Error> {
        decode_flow_operand(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum FlowDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    Conversion { index: usize, error: E },
}

fn token_range(source: &[u8], parameters: &ExtrudedParams, slot: usize) -> (usize, usize) {
    let start = parameters.offsets[slot].expect("counted parameter token is initialized");
    let mut end = start;
    while source[end] != 0 {
        end += 1;
    }
    (start, end)
}

/// Split LET shape: destination, operator and operand token ranges. The ranges
/// borrow the caller's working source; conversions run separately through the
/// backend callbacks in original order.
struct LetSplit {
    dest: i32,
    op: LetOp,
    a: (usize, usize),
    b: Option<(usize, usize)>,
}

/// Original DecodeLet on one trimmed token, splitting in place: `flt[dest]`,
/// then the first of `+ / * -` found in that priority order, splits the
/// operands. A missing shape returns None. An empty operand side decodes as
/// the zero literal instead of reading next to the split NUL as the original
/// does.
fn split_let(
    source: &mut [u8],
    start: usize,
    end: usize,
) -> Result<Option<LetSplit>, ProgramParserError> {
    let mut position = start;
    while position < end && source[position] == b' ' {
        position += 1;
    }
    if end - position < 4 || !source[position..position + 4].eq_ignore_ascii_case(b"flt[") {
        return Ok(None);
    }
    let inside = position + 4;
    let Some(relative_end) = source[inside..end].iter().position(|&byte| byte == b']') else {
        return Ok(None);
    };
    let close = inside + relative_end;
    let dest = ascii_integer_prefix(&source[inside..close])?;
    source[close] = 0;
    let Some(relative_eq) = source[close + 1..end].iter().position(|&byte| byte == b'=') else {
        return Ok(None);
    };
    let equation = close + 1 + relative_eq + 1;
    let string_end = end;
    let mut op = LetOp::None;
    let mut split = string_end;
    for (byte, candidate) in [
        (b'+', LetOp::Add),
        (b'/', LetOp::Div),
        (b'*', LetOp::Mul),
        (b'-', LetOp::Sub),
    ] {
        if let Some(relative) = source[equation..string_end]
            .iter()
            .position(|&cell| cell == byte)
        {
            op = candidate;
            split = equation + relative;
            break;
        }
    }
    source[split] = 0;
    let b = if split == string_end {
        None
    } else {
        Some((split + 1, string_end))
    };
    Ok(Some(LetSplit {
        dest,
        op,
        a: (equation, split),
        b,
    }))
}

/// Original DecodeIf on one trimmed token: the first of `< > = !` found in
/// that priority order splits the operands. A missing comparison returns None.
/// Split IF shape: comparison and operand token ranges. See split_let for the
/// borrowing and conversion contract.
struct IfSplit {
    op: IfOp,
    a: (usize, usize),
    b: (usize, usize),
}

/// Original DecodeIf on one trimmed token: the first of `< > = !` found in
/// that priority order splits the operands. A missing comparison returns None.
fn split_if(
    source: &mut [u8],
    start: usize,
    end: usize,
) -> Result<Option<IfSplit>, ProgramParserError> {
    let mut op = None;
    let mut split = end;
    for (byte, candidate) in [
        (b'<', IfOp::Less),
        (b'>', IfOp::Greater),
        (b'=', IfOp::Equal),
        (b'!', IfOp::NotEqual),
    ] {
        if let Some(relative) = source[start..end].iter().position(|&cell| cell == byte) {
            op = Some(candidate);
            split = start + relative;
            break;
        }
    }
    let Some(op) = op else {
        return Ok(None);
    };
    source[split] = 0;
    Ok(Some(IfSplit {
        op,
        a: (start, split),
        b: (split + 1, end),
    }))
}

/// One fresh instruction flow-parameter slot. A `false` result maps every
/// native DecodeParams `false` path (count, length and shape rejections):
/// the pending instruction is dropped and compilation records an error.
#[derive(Debug, Default)]
pub struct FlowParameterSlot {
    entered: bool,
    parameter: Option<Box<FlowParameter>>,
}

impl FlowParameterSlot {
    pub fn parameter(&self) -> Option<&FlowParameter> {
        self.parameter.as_deref()
    }

    pub fn decode<B: FlowParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, FlowDecodeError<B::Error>> {
        if self.entered {
            return Err(FlowDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(FlowDecodeError::Parser)?;
        if !supports_flow_opcode(opcode) {
            return Err(FlowDecodeError::UnsupportedOpcode(opcode));
        }
        let zero_operand = FlowOperand {
            register: -1,
            value: 0.0,
        };
        match opcode {
            0 => {
                if parameters.count != 1 {
                    return Ok(false);
                }
                if !backend
                    .allocate_flow()
                    .map_err(FlowDecodeError::Allocation)?
                {
                    return Err(FlowDecodeError::NullAllocation);
                }
                let (start, end) = token_range(source, &parameters, 0);
                self.parameter = Some(Box::new(FlowParameter::Label(
                    source[start..end].to_vec().into_boxed_slice(),
                )));
                Ok(true)
            }
            1 | 2 => {
                if parameters.count != 1 {
                    return Ok(false);
                }
                let (start, end) = token_range(source, &parameters, 0);
                if end - start >= 16 {
                    return Ok(false);
                }
                let takes_register =
                    end - start >= 4 && source[start..start + 4].eq_ignore_ascii_case(b"flt[");
                if !backend
                    .allocate_flow()
                    .map_err(FlowDecodeError::Allocation)?
                {
                    return Err(FlowDecodeError::NullAllocation);
                }
                if takes_register {
                    self.parameter = Some(Box::new(FlowParameter::Goto(GotoParameter {
                        label: Box::default(),
                        target: zero_operand,
                    })));
                    let published = self.parameter.as_ref().expect("published goto skeleton");
                    let target = backend
                        .convert_operand(0, &mut source[start..], published)
                        .map_err(|error| FlowDecodeError::Conversion { index: 0, error })?;
                    let Some(target) = target else {
                        self.parameter = None;
                        return Ok(false);
                    };
                    if let Some(FlowParameter::Goto(goto)) = self.parameter.as_deref_mut() {
                        goto.target = target;
                    }
                    Ok(true)
                } else {
                    self.parameter = Some(Box::new(FlowParameter::Goto(GotoParameter {
                        label: source[start..end].to_vec().into_boxed_slice(),
                        target: zero_operand,
                    })));
                    Ok(true)
                }
            }
            4 => {
                if parameters.count != 1 {
                    return Ok(false);
                }
                if !backend
                    .allocate_flow()
                    .map_err(FlowDecodeError::Allocation)?
                {
                    return Err(FlowDecodeError::NullAllocation);
                }
                let (start, _) = token_range(source, &parameters, 0);
                self.parameter = Some(Box::new(FlowParameter::Wait(zero_operand)));
                let published = self.parameter.as_ref().expect("published wait skeleton");
                let operand = backend
                    .convert_operand(0, &mut source[start..], published)
                    .map_err(|error| FlowDecodeError::Conversion { index: 0, error })?;
                let Some(operand) = operand else {
                    self.parameter = None;
                    return Ok(false);
                };
                if let Some(FlowParameter::Wait(wait)) = self.parameter.as_deref_mut() {
                    *wait = operand;
                }
                Ok(true)
            }
            8 => {
                if parameters.count != 1 {
                    return Ok(false);
                }
                if !backend
                    .allocate_flow()
                    .map_err(FlowDecodeError::Allocation)?
                {
                    return Err(FlowDecodeError::NullAllocation);
                }
                let (start, end) = token_range(source, &parameters, 0);
                let Some(LetSplit {
                    dest,
                    op,
                    a: (a_start, _),
                    b: b_range,
                }) = split_let(source, start, end).map_err(FlowDecodeError::Parser)?
                else {
                    return Ok(false);
                };
                self.parameter = Some(Box::new(FlowParameter::Let(LetParameter {
                    dest,
                    a: zero_operand,
                    b: zero_operand,
                    op,
                })));
                let published = self.parameter.as_ref().expect("published let skeleton");
                let a = backend
                    .convert_operand(0, &mut source[a_start..], published)
                    .map_err(|error| FlowDecodeError::Conversion { index: 0, error })?
                    .unwrap_or(zero_operand);
                if let Some(FlowParameter::Let(let_param)) = self.parameter.as_deref_mut() {
                    let_param.a = a;
                }
                if let Some((b_start, _)) = b_range {
                    let published = self.parameter.as_ref().expect("published let skeleton");
                    let b = backend
                        .convert_operand(1, &mut source[b_start..], published)
                        .map_err(|error| FlowDecodeError::Conversion { index: 1, error })?
                        .unwrap_or(zero_operand);
                    if let Some(FlowParameter::Let(let_param)) = self.parameter.as_deref_mut() {
                        let_param.b = b;
                    }
                }
                Ok(true)
            }
            9 => {
                if parameters.count != 3 {
                    return Ok(false);
                }
                let (first_start, first_end) = token_range(source, &parameters, 0);
                let (label1_start, label1_end) = token_range(source, &parameters, 1);
                let (label2_start, label2_end) = token_range(source, &parameters, 2);
                if label1_end - label1_start > 15 || label2_end - label2_start > 15 {
                    return Ok(false);
                }
                if !backend
                    .allocate_flow()
                    .map_err(FlowDecodeError::Allocation)?
                {
                    return Err(FlowDecodeError::NullAllocation);
                }
                let Some(IfSplit {
                    op,
                    a: (a_start, _),
                    b: (b_start, _),
                }) = split_if(source, first_start, first_end).map_err(FlowDecodeError::Parser)?
                else {
                    return Ok(false);
                };
                self.parameter = Some(Box::new(FlowParameter::If(IfParameter {
                    a: zero_operand,
                    b: zero_operand,
                    op,
                    label1: source[label1_start..label1_end].to_vec().into_boxed_slice(),
                    label2: source[label2_start..label2_end].to_vec().into_boxed_slice(),
                })));
                let published = self.parameter.as_ref().expect("published if skeleton");
                let a = backend
                    .convert_operand(0, &mut source[a_start..], published)
                    .map_err(|error| FlowDecodeError::Conversion { index: 0, error })?
                    .unwrap_or(zero_operand);
                if let Some(FlowParameter::If(if_param)) = self.parameter.as_deref_mut() {
                    if_param.a = a;
                }
                let published = self.parameter.as_ref().expect("published if skeleton");
                let b = backend
                    .convert_operand(1, &mut source[b_start..], published)
                    .map_err(|error| FlowDecodeError::Conversion { index: 1, error })?
                    .unwrap_or(zero_operand);
                if let Some(FlowParameter::If(if_param)) = self.parameter.as_deref_mut() {
                    if_param.b = b;
                }
                Ok(true)
            }
            _ => Err(FlowDecodeError::UnsupportedOpcode(opcode)),
        }
    }
}

/// Parameter-layout admission only; these opcodes do not become executable by
/// decoding. Opcode 10 (RND) already decodes through the pair arm.
pub fn supports_flow_opcode(opcode: u32) -> bool {
    matches!(opcode, 0 | 1 | 2 | 4 | 8 | 9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::original_program::{supports_name_opcode, supports_pair_opcode};
    use crate::program_parameters::supports_scalar_opcode;
    use crate::program_parser::ProgramParserError;
    use crate::program_zero_parameters::supports_zero_parameter_opcode;

    fn decode(opcode: u32, text: &[u8]) -> Result<bool, FlowDecodeError<ProgramParserError>> {
        let mut backend = AsciiFlowParameterBackend;
        let mut slot = FlowParameterSlot::default();
        let mut source = text.to_vec();
        source.push(0);
        slot.decode(opcode, &mut source, &mut backend)
    }

    fn payload(opcode: u32, text: &[u8]) -> FlowParameter {
        let mut backend = AsciiFlowParameterBackend;
        let mut slot = FlowParameterSlot::default();
        let mut source = text.to_vec();
        source.push(0);
        assert_eq!(slot.decode(opcode, &mut source, &mut backend), Ok(true));
        slot.parameter().cloned().expect("decoded payload")
    }

    #[test]
    fn flow_opcode_family_is_disjoint_from_existing_arms() {
        for opcode in 0..398u32 {
            let mut owners = 0;
            if supports_flow_opcode(opcode) {
                owners += 1;
            }
            if supports_scalar_opcode(opcode) {
                owners += 1;
            }
            if supports_zero_parameter_opcode(opcode) {
                owners += 1;
            }
            if supports_name_opcode(opcode) {
                owners += 1;
            }
            if supports_pair_opcode(opcode) {
                owners += 1;
            }
            assert!(owners <= 1, "opcode {opcode} claimed twice");
        }
        for opcode in [0u32, 1, 2, 4, 8, 9] {
            assert!(supports_flow_opcode(opcode));
        }
    }

    #[test]
    fn operand_literal_register_and_bracket_shapes() {
        let mut source = b"  17suffix\0".to_vec();
        assert_eq!(
            decode_flow_operand(&mut source),
            Ok(Some(FlowOperand {
                register: -1,
                value: 17.0
            }))
        );
        let mut source = b"FLT[12]\0".to_vec();
        assert_eq!(
            decode_flow_operand(&mut source),
            Ok(Some(FlowOperand {
                register: 12,
                value: 0.0
            }))
        );
        assert_eq!(source, b"FLT[12\0\0".to_vec());
        let mut source = b"flt[3\0".to_vec();
        assert_eq!(decode_flow_operand(&mut source), Ok(None));
        let mut source = b"\0".to_vec();
        assert_eq!(
            decode_flow_operand(&mut source),
            Ok(Some(FlowOperand {
                register: -1,
                value: 0.0
            }))
        );
        let mut source = vec![b' ', 0xff, b' ', 0];
        assert_eq!(
            decode_flow_operand(&mut source),
            Err(ProgramParserError::UnsupportedCrtByte {
                offset: 1,
                byte: 0xff
            })
        );
    }

    #[test]
    fn label_copies_exact_bytes_and_rejects_counts() {
        assert_eq!(
            payload(0, b"loop_start"),
            FlowParameter::Label(b"loop_start".to_vec().into_boxed_slice())
        );
        assert_eq!(decode(0, b"a,b"), Ok(false));
        assert_eq!(decode(0, b""), Ok(false));
        assert_eq!(decode(7, b"1"), Err(FlowDecodeError::UnsupportedOpcode(7)));
        let mut backend = AsciiFlowParameterBackend;
        let mut slot = FlowParameterSlot::default();
        let mut first = b"x\0".to_vec();
        assert_eq!(slot.decode(0, &mut first, &mut backend), Ok(true));
        let mut second = b"y\0".to_vec();
        assert_eq!(
            slot.decode(0, &mut second, &mut backend),
            Err(FlowDecodeError::AlreadyEntered)
        );
    }

    #[test]
    fn goto_label_and_register_forms_with_length_gate() {
        assert_eq!(
            payload(1, b"loop_start"),
            FlowParameter::Goto(GotoParameter {
                label: b"loop_start".to_vec().into_boxed_slice(),
                target: FlowOperand {
                    register: -1,
                    value: 0.0
                },
            })
        );
        assert_eq!(
            payload(2, b"flt[4]"),
            FlowParameter::Goto(GotoParameter {
                label: Box::default(),
                target: FlowOperand {
                    register: 4,
                    value: 0.0
                },
            })
        );
        assert_eq!(decode(1, b"0123456789abcdef"), Ok(false));
        assert_eq!(decode(1, b"a,b"), Ok(false));
        assert_eq!(decode(2, b"flt[5"), Ok(false));
    }

    #[test]
    fn wait_accepts_literal_and_register_and_rejects_bracket() {
        assert_eq!(
            payload(4, b"150"),
            FlowParameter::Wait(FlowOperand {
                register: -1,
                value: 150.0
            })
        );
        assert_eq!(
            payload(4, b"flt[2]"),
            FlowParameter::Wait(FlowOperand {
                register: 2,
                value: 0.0
            })
        );
        assert_eq!(decode(4, b"flt[2"), Ok(false));
        assert_eq!(decode(4, b"1,2"), Ok(false));
    }

    #[test]
    fn let_operators_split_in_original_priority_order() {
        assert_eq!(
            payload(8, b"flt[0] = flt[1] + 2"),
            FlowParameter::Let(LetParameter {
                dest: 0,
                a: FlowOperand {
                    register: 1,
                    value: 0.0
                },
                b: FlowOperand {
                    register: -1,
                    value: 2.0
                },
                op: LetOp::Add,
            })
        );
        // '+' wins over '*' even when '*' comes first: b keeps its raw text.
        assert_eq!(
            payload(8, b"flt[0]=1+2*3"),
            FlowParameter::Let(LetParameter {
                dest: 0,
                a: FlowOperand {
                    register: -1,
                    value: 1.0
                },
                b: FlowOperand {
                    register: -1,
                    value: 2.0
                },
                op: LetOp::Add,
            })
        );
        // '/' wins over '-': b keeps its raw text.
        assert_eq!(
            payload(8, b"flt[3]=8/2-1"),
            FlowParameter::Let(LetParameter {
                dest: 3,
                a: FlowOperand {
                    register: -1,
                    value: 8.0
                },
                b: FlowOperand {
                    register: -1,
                    value: 2.0
                },
                op: LetOp::Div,
            })
        );
        // A leading '-' is the operator; the empty left side is zero.
        assert_eq!(
            payload(8, b"flt[2]=-1-2"),
            FlowParameter::Let(LetParameter {
                dest: 2,
                a: FlowOperand {
                    register: -1,
                    value: 0.0
                },
                b: FlowOperand {
                    register: -1,
                    value: 1.0
                },
                op: LetOp::Sub,
            })
        );
        assert_eq!(
            payload(8, b"flt[5] = 7"),
            FlowParameter::Let(LetParameter {
                dest: 5,
                a: FlowOperand {
                    register: -1,
                    value: 7.0
                },
                b: FlowOperand {
                    register: -1,
                    value: 0.0
                },
                op: LetOp::None,
            })
        );
        assert_eq!(decode(8, b"flt[0] = 5, extra"), Ok(false));
        assert_eq!(decode(8, b"oops"), Ok(false));
        assert_eq!(decode(8, b"flt[0] oops"), Ok(false));
    }

    #[test]
    fn if_comparisons_branches_and_label_gates() {
        assert_eq!(
            payload(9, b"flt[0] < flt[1], yes, no"),
            FlowParameter::If(IfParameter {
                a: FlowOperand {
                    register: 0,
                    value: 0.0
                },
                b: FlowOperand {
                    register: 1,
                    value: 0.0
                },
                op: IfOp::Less,
                label1: b"yes".to_vec().into_boxed_slice(),
                label2: b"no".to_vec().into_boxed_slice(),
            })
        );
        assert!(matches!(
            payload(9, b"3 >= 4, a, b"),
            FlowParameter::If(IfParameter {
                op: IfOp::Greater,
                ..
            })
        ));
        assert!(matches!(
            payload(9, b"x = y, a, b"),
            FlowParameter::If(IfParameter {
                op: IfOp::Equal,
                ..
            })
        ));
        assert!(matches!(
            payload(9, b"x ! y, a, b"),
            FlowParameter::If(IfParameter {
                op: IfOp::NotEqual,
                ..
            })
        ));
        assert_eq!(decode(9, b"flt[0] = 5"), Ok(false));
        assert_eq!(decode(9, b"a < b, only-one"), Ok(false));
        assert_eq!(decode(9, b"a < b, ok, 0123456789abcdef"), Ok(false));
    }

    #[test]
    fn interrupted_conversion_retains_completed_prefix() {
        struct FailingBackend {
            fail_at: usize,
            conversions: Vec<usize>,
        }
        impl FlowParameterBackend for FailingBackend {
            type Error = &'static str;
            fn allocate_flow(&mut self) -> Result<bool, Self::Error> {
                Ok(true)
            }
            fn convert_operand(
                &mut self,
                index: usize,
                token: &mut [u8],
                _published: &FlowParameter,
            ) -> Result<Option<FlowOperand>, Self::Error> {
                self.conversions.push(index);
                if index == self.fail_at {
                    return Err("interrupted");
                }
                decode_flow_operand(token).map_err(|_| "parser")
            }
        }
        let mut backend = FailingBackend {
            fail_at: 1,
            conversions: Vec::new(),
        };
        let mut slot = FlowParameterSlot::default();
        let mut source = b"flt[0] = flt[1] + 5\0".to_vec();
        assert_eq!(
            slot.decode(8, &mut source, &mut backend),
            Err(FlowDecodeError::Conversion {
                index: 1,
                error: "interrupted"
            })
        );
        assert_eq!(
            slot.parameter().cloned(),
            Some(FlowParameter::Let(LetParameter {
                dest: 0,
                a: FlowOperand {
                    register: 1,
                    value: 0.0
                },
                b: FlowOperand {
                    register: -1,
                    value: 0.0
                },
                op: LetOp::Add,
            }))
        );
        assert_eq!(backend.conversions, vec![0, 1]);
    }

    #[test]
    fn refused_allocation_reports_null_without_payload() {
        struct NullBackend;
        impl FlowParameterBackend for NullBackend {
            type Error = &'static str;
            fn allocate_flow(&mut self) -> Result<bool, Self::Error> {
                Ok(false)
            }
            fn convert_operand(
                &mut self,
                _index: usize,
                _token: &mut [u8],
                _published: &FlowParameter,
            ) -> Result<Option<FlowOperand>, Self::Error> {
                panic!("no conversion without allocation")
            }
        }
        let mut backend = NullBackend;
        let mut slot = FlowParameterSlot::default();
        let mut source = b"target\0".to_vec();
        assert_eq!(
            slot.decode(1, &mut source, &mut backend),
            Err(FlowDecodeError::NullAllocation)
        );
        assert!(slot.parameter().is_none());
    }
}
