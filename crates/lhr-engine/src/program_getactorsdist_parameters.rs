//! Owned C_program::DecodeParams branch for GETACTORSDIST (57).
//!
//! Reference: mafia-re 113b5b6, game/mission/C_program.cpp DecodeParams
//! CMD_GETACTORSDIST case (:1375-1390): three or four words, atoi of each in
//! order, with the fourth defaulting to 0 when only three are present. The
//! words are actor_reg_a, actor_reg_b, float_reg, dead_sign_flag.
//! This module reconstructs source decoding only, not command execution,
//! native allocation failure, or original-runtime equivalence.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct GetActorsDistParameter {
    values: [Option<i32>; 4],
}

impl GetActorsDistParameter {
    pub fn values(&self) -> [Option<i32>; 4] {
        self.values
    }
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait GetActorsDistParameterBackend {
    type Error;
    fn allocate_getactorsdist(
        &mut self,
    ) -> Result<Option<Box<GetActorsDistParameter>>, Self::Error>;
    fn convert_integer(
        &mut self,
        index: usize,
        token: &[u8],
        published: &GetActorsDistParameter,
    ) -> Result<i32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiGetActorsDistParameterBackend;

impl GetActorsDistParameterBackend for AsciiGetActorsDistParameterBackend {
    type Error = ProgramParserError;

    fn allocate_getactorsdist(
        &mut self,
    ) -> Result<Option<Box<GetActorsDistParameter>>, Self::Error> {
        Ok(Some(Box::new(GetActorsDistParameter::default())))
    }

    fn convert_integer(
        &mut self,
        _index: usize,
        token: &[u8],
        _published: &GetActorsDistParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum GetActorsDistDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    Conversion { index: usize, error: E },
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if conversion or a backend callback unwinds.
#[derive(Debug, Default)]
pub struct GetActorsDistParameterSlot {
    entered: bool,
    parameter: Option<Box<GetActorsDistParameter>>,
}

impl GetActorsDistParameterSlot {
    pub fn parameter(&self) -> Option<&GetActorsDistParameter> {
        self.parameter.as_deref()
    }

    /// Only counts of three or four are accepted. With three words the fourth
    /// defaults to 0, matching the native `v[3] = 0` fallthrough. The source
    /// is split in place before count validation, as in the original
    /// DecodeParams call chain.
    pub fn decode<B: GetActorsDistParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, GetActorsDistDecodeError<B::Error>> {
        if self.entered {
            return Err(GetActorsDistDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(GetActorsDistDecodeError::Parser)?;
        if !supports_getactorsdist_opcode(opcode) {
            return Err(GetActorsDistDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 3 && parameters.count != 4 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_getactorsdist()
            .map_err(GetActorsDistDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(GetActorsDistDecodeError::NullAllocation)?;
        parameter.values = [None, None, None, None];

        for index in 0..parameters.count {
            let start = parameters.offsets[index].expect("counted parameter token");
            let length = c_length(&source[start..]).map_err(GetActorsDistDecodeError::Parser)?;
            let value = backend
                .convert_integer(index, &source[start..start + length], parameter)
                .map_err(|error| GetActorsDistDecodeError::Conversion { index, error })?;
            parameter.values[index] = Some(value);
        }
        if parameters.count == 3 {
            parameter.values[3] = Some(0);
        }
        Ok(true)
    }
}

/// Parameter-layout admission only; execution is admitted in dispatch.
pub fn supports_getactorsdist_opcode(opcode: u32) -> bool {
    opcode == 57
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn getactorsdist_accepts_three_with_default_and_four() {
        let mut source = b"0,1,2\0".to_vec();
        let mut slot = GetActorsDistParameterSlot::default();
        assert_eq!(
            slot.decode(57, &mut source, &mut AsciiGetActorsDistParameterBackend),
            Ok(true)
        );
        assert_eq!(
            slot.parameter().unwrap().values(),
            [Some(0), Some(1), Some(2), Some(0)]
        );
        let mut source = b"0,1,2,1\0".to_vec();
        let mut slot = GetActorsDistParameterSlot::default();
        assert_eq!(
            slot.decode(57, &mut source, &mut AsciiGetActorsDistParameterBackend),
            Ok(true)
        );
        assert_eq!(
            slot.parameter().unwrap().values(),
            [Some(0), Some(1), Some(2), Some(1)]
        );
        assert!(supports_getactorsdist_opcode(57));
        for opcode in [0, 56, 58, 59, 101, 397] {
            assert!(!supports_getactorsdist_opcode(opcode), "opcode {opcode}");
        }
    }

    #[test]
    fn getactorsdist_refuses_other_counts_and_unsupported_opcode() {
        for source in [&b"\0"[..], &b"0,1\0"[..], &b"0,1,2,3,4\0"[..]] {
            let mut buffer = source.to_vec();
            let mut slot = GetActorsDistParameterSlot::default();
            assert_eq!(
                slot.decode(57, &mut buffer, &mut AsciiGetActorsDistParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        let mut buffer = b"0,1,2\0".to_vec();
        let mut slot = GetActorsDistParameterSlot::default();
        assert_eq!(
            slot.decode(58, &mut buffer, &mut AsciiGetActorsDistParameterBackend),
            Err(GetActorsDistDecodeError::UnsupportedOpcode(58))
        );
    }
}
