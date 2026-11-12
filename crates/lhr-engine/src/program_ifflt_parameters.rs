//! Owned C_program::DecodeParams branch for IFFLTINRANGE (20).
//!
//! Reference: Marvisak/mafia-re 113b5b6, game/mission/C_program.cpp,
//! DecodeParams CMD_IFFLTINRANGE case (:1085-1096) and S_ifflt_params in
//! C_program.h (:533-539). This module reconstructs source decoding only,
//! not command execution, native allocation failure, or
//! original-runtime equivalence.
//!
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
#![forbid(unsafe_code)]

use crate::program_parameters::{ascii_float_prefix, ascii_integer_prefix};
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct IfFltParameter {
    register: Option<i32>,
    min: Option<f32>,
    max: Option<f32>,
    label: Option<Box<[u8]>>,
}

impl IfFltParameter {
    pub fn register(&self) -> Option<i32> {
        self.register
    }

    pub fn min(&self) -> Option<f32> {
        self.min
    }

    pub fn max(&self) -> Option<f32> {
        self.max
    }

    pub fn label(&self) -> Option<&[u8]> {
        self.label.as_deref()
    }
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried. The label
/// copies inline after the three conversions, as the original `strcpy` does.
pub trait IfFltParameterBackend {
    type Error;
    fn allocate_ifflt(&mut self) -> Result<Option<Box<IfFltParameter>>, Self::Error>;
    fn convert_ifflt_register(
        &mut self,
        token: &[u8],
        published: &IfFltParameter,
    ) -> Result<i32, Self::Error>;
    fn convert_ifflt_min(
        &mut self,
        token: &[u8],
        published: &IfFltParameter,
    ) -> Result<f32, Self::Error>;
    fn convert_ifflt_max(
        &mut self,
        token: &[u8],
        published: &IfFltParameter,
    ) -> Result<f32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiIfFltParameterBackend;

impl IfFltParameterBackend for AsciiIfFltParameterBackend {
    type Error = ProgramParserError;

    fn allocate_ifflt(&mut self) -> Result<Option<Box<IfFltParameter>>, Self::Error> {
        Ok(Some(Box::new(IfFltParameter::default())))
    }

    fn convert_ifflt_register(
        &mut self,
        token: &[u8],
        _published: &IfFltParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }

    fn convert_ifflt_min(
        &mut self,
        token: &[u8],
        _published: &IfFltParameter,
    ) -> Result<f32, Self::Error> {
        ascii_float_prefix(token)
    }

    fn convert_ifflt_max(
        &mut self,
        token: &[u8],
        _published: &IfFltParameter,
    ) -> Result<f32, Self::Error> {
        ascii_float_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum IfFltDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    RegisterConversion(E),
    MinConversion(E),
    MaxConversion(E),
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if conversion or a backend callback unwinds.
#[derive(Debug, Default)]
pub struct IfFltParameterSlot {
    entered: bool,
    parameter: Option<Box<IfFltParameter>>,
}

impl IfFltParameterSlot {
    pub fn parameter(&self) -> Option<&IfFltParameter> {
        self.parameter.as_deref()
    }

    /// Only a count of four is accepted, with the label (an unquoted name of
    /// at most 15 bytes) copied raw. The source is split in place before
    /// count validation, as in the original DecodeParams call chain.
    pub fn decode<B: IfFltParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, IfFltDecodeError<B::Error>> {
        if self.entered {
            return Err(IfFltDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(IfFltDecodeError::Parser)?;
        if !supports_ifflt_opcode(opcode) {
            return Err(IfFltDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 4 {
            return Ok(false);
        }
        let label_start = parameters.offsets[3].expect("counted parameter token");
        let label_length = c_length(&source[label_start..]).map_err(IfFltDecodeError::Parser)?;
        if label_length > 15 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_ifflt()
            .map_err(IfFltDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(IfFltDecodeError::NullAllocation)?;
        let first = parameters.offsets[0].expect("counted parameter token");
        let first_length = c_length(&source[first..]).map_err(IfFltDecodeError::Parser)?;
        parameter.register = Some(
            backend
                .convert_ifflt_register(&source[first..first + first_length], parameter)
                .map_err(IfFltDecodeError::RegisterConversion)?,
        );
        let second = parameters.offsets[1].expect("counted parameter token");
        let second_length = c_length(&source[second..]).map_err(IfFltDecodeError::Parser)?;
        parameter.min = Some(
            backend
                .convert_ifflt_min(&source[second..second + second_length], parameter)
                .map_err(IfFltDecodeError::MinConversion)?,
        );
        let third = parameters.offsets[2].expect("counted parameter token");
        let third_length = c_length(&source[third..]).map_err(IfFltDecodeError::Parser)?;
        parameter.max = Some(
            backend
                .convert_ifflt_max(&source[third..third + third_length], parameter)
                .map_err(IfFltDecodeError::MaxConversion)?,
        );
        parameter.label = Some(
            source[label_start..label_start + label_length]
                .to_vec()
                .into_boxed_slice(),
        );
        Ok(true)
    }
}

/// Parameter-layout admission only; this opcode does not become executable by
/// decoding (execution is admitted in dispatch).
pub fn supports_ifflt_opcode(opcode: u32) -> bool {
    opcode == 20
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(text: &[u8]) -> IfFltParameter {
        let mut backend = AsciiIfFltParameterBackend;
        let mut slot = IfFltParameterSlot::default();
        let mut source = text.to_vec();
        source.push(0);
        assert_eq!(slot.decode(20, &mut source, &mut backend), Ok(true));
        slot.parameter().cloned().expect("decoded payload")
    }

    fn decode(opcode: u32, text: &[u8]) -> Result<bool, IfFltDecodeError<ProgramParserError>> {
        let mut backend = AsciiIfFltParameterBackend;
        let mut slot = IfFltParameterSlot::default();
        let mut source = text.to_vec();
        source.push(0);
        slot.decode(opcode, &mut source, &mut backend)
    }

    #[test]
    fn ifflt_accepts_register_float_bounds_and_short_label() {
        let parameter = payload(b"2, 1.5, 9.25, target");
        assert_eq!(parameter.register(), Some(2));
        assert_eq!(parameter.min(), Some(1.5));
        assert_eq!(parameter.max(), Some(9.25));
        assert_eq!(parameter.label(), Some(b"target".as_slice()));
        // Bounds keep their exact bits, including infinities; register keeps
        // its integer prefix.
        let parameter = payload(b"-3suffix, -inf, inf, x");
        assert_eq!(parameter.register(), Some(-3));
        assert_eq!(parameter.min(), Some(f32::NEG_INFINITY));
        assert_eq!(parameter.max(), Some(f32::INFINITY));
        // A fifteen-byte label is admitted.
        let parameter = payload(b"0, 0, 1, 0123456789abcde");
        assert_eq!(parameter.label(), Some(b"0123456789abcde".as_slice()));
    }

    #[test]
    fn ifflt_rejects_counts_long_labels_and_foreign_opcodes() {
        assert_eq!(decode(20, b"0, 1, 2"), Ok(false));
        assert_eq!(decode(20, b"0, 1, 2, a, b"), Ok(false));
        assert_eq!(decode(20, b""), Ok(false));
        assert_eq!(decode(20, b"0, 1, 2, 0123456789abcdef"), Ok(false));
        assert_eq!(
            decode(21, b"0, 1, 2, a"),
            Err(IfFltDecodeError::UnsupportedOpcode(21))
        );
        let mut backend = AsciiIfFltParameterBackend;
        let mut slot = IfFltParameterSlot::default();
        let mut first = b"0, 1, 2, a\0".to_vec();
        assert_eq!(slot.decode(20, &mut first, &mut backend), Ok(true));
        let mut second = b"0, 1, 2, b\0".to_vec();
        assert_eq!(
            slot.decode(20, &mut second, &mut backend),
            Err(IfFltDecodeError::AlreadyEntered)
        );
    }

    #[test]
    fn interrupted_conversion_retains_completed_prefix() {
        struct FailingBackend {
            fail_at: u8,
            conversions: Vec<u8>,
        }
        impl IfFltParameterBackend for FailingBackend {
            type Error = &'static str;
            fn allocate_ifflt(&mut self) -> Result<Option<Box<IfFltParameter>>, Self::Error> {
                Ok(Some(Box::new(IfFltParameter::default())))
            }
            fn convert_ifflt_register(
                &mut self,
                token: &[u8],
                _published: &IfFltParameter,
            ) -> Result<i32, Self::Error> {
                self.conversions.push(0);
                if self.fail_at == 0 {
                    return Err("interrupted");
                }
                ascii_integer_prefix(token).map_err(|_| "parser")
            }
            fn convert_ifflt_min(
                &mut self,
                token: &[u8],
                _published: &IfFltParameter,
            ) -> Result<f32, Self::Error> {
                self.conversions.push(1);
                if self.fail_at == 1 {
                    return Err("interrupted");
                }
                ascii_float_prefix(token).map_err(|_| "parser")
            }
            fn convert_ifflt_max(
                &mut self,
                _token: &[u8],
                _published: &IfFltParameter,
            ) -> Result<f32, Self::Error> {
                self.conversions.push(2);
                Err("interrupted")
            }
        }
        let mut backend = FailingBackend {
            fail_at: 2,
            conversions: Vec::new(),
        };
        let mut slot = IfFltParameterSlot::default();
        let mut source = b"4, 1.5, 9.25, target\0".to_vec();
        assert_eq!(
            slot.decode(20, &mut source, &mut backend),
            Err(IfFltDecodeError::MaxConversion("interrupted"))
        );
        let parameter = slot.parameter().expect("completed prefix retained");
        assert_eq!(
            (parameter.register(), parameter.min()),
            (Some(4), Some(1.5))
        );
        assert_eq!(parameter.max(), None);
        assert_eq!(parameter.label(), None);
        assert_eq!(backend.conversions, vec![0, 1, 2]);
    }

    #[test]
    fn refused_allocation_reports_null_without_payload() {
        struct NullBackend;
        impl IfFltParameterBackend for NullBackend {
            type Error = &'static str;
            fn allocate_ifflt(&mut self) -> Result<Option<Box<IfFltParameter>>, Self::Error> {
                Ok(None)
            }
            fn convert_ifflt_register(
                &mut self,
                _token: &[u8],
                _published: &IfFltParameter,
            ) -> Result<i32, Self::Error> {
                panic!("no conversion without allocation")
            }
            fn convert_ifflt_min(
                &mut self,
                _token: &[u8],
                _published: &IfFltParameter,
            ) -> Result<f32, Self::Error> {
                panic!("no conversion without allocation")
            }
            fn convert_ifflt_max(
                &mut self,
                _token: &[u8],
                _published: &IfFltParameter,
            ) -> Result<f32, Self::Error> {
                panic!("no conversion without allocation")
            }
        }
        let mut backend = NullBackend;
        let mut slot = IfFltParameterSlot::default();
        let mut source = b"0, 1, 2, a\0".to_vec();
        assert_eq!(
            slot.decode(20, &mut source, &mut backend),
            Err(IfFltDecodeError::NullAllocation)
        );
        assert!(slot.parameter().is_none());
    }
}
