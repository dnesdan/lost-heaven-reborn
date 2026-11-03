//! Owned four-integer C_program::DecodeParams branch (4 original selectors).
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_TIMERON..CMD_QUAT_SLERP case group. These commands require
//! exactly four comma-separated words and store atoi of each in order.
//! This module reconstructs source decoding only, not command execution,
//! native allocation failure, or original-runtime equivalence.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct QuadParameter {
    values: [Option<i32>; 4],
}

impl QuadParameter {
    pub fn values(&self) -> [Option<i32>; 4] {
        self.values
    }
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait QuadParameterBackend {
    type Error;
    fn allocate_quad(&mut self) -> Result<Option<Box<QuadParameter>>, Self::Error>;
    fn convert_integer(
        &mut self,
        index: usize,
        token: &[u8],
        published: &QuadParameter,
    ) -> Result<i32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiQuadParameterBackend;

impl QuadParameterBackend for AsciiQuadParameterBackend {
    type Error = ProgramParserError;

    fn allocate_quad(&mut self) -> Result<Option<Box<QuadParameter>>, Self::Error> {
        Ok(Some(Box::new(QuadParameter::default())))
    }

    fn convert_integer(
        &mut self,
        _index: usize,
        token: &[u8],
        _published: &QuadParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum QuadDecodeError<E> {
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
pub struct QuadParameterSlot {
    entered: bool,
    parameter: Option<Box<QuadParameter>>,
}

impl QuadParameterSlot {
    pub fn parameter(&self) -> Option<&QuadParameter> {
        self.parameter.as_deref()
    }

    /// Only a count of four is accepted. The source is split in place before
    /// count validation, as in the original DecodeParams call chain.
    pub fn decode<B: QuadParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, QuadDecodeError<B::Error>> {
        if self.entered {
            return Err(QuadDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(QuadDecodeError::Parser)?;
        if !supports_quad_opcode(opcode) {
            return Err(QuadDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 4 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_quad()
            .map_err(QuadDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(QuadDecodeError::NullAllocation)?;
        parameter.values = [None, None, None, None];

        for index in 0..4 {
            let start = parameters.offsets[index].expect("counted parameter token");
            let length = c_length(&source[start..]).map_err(QuadDecodeError::Parser)?;
            let value = backend
                .convert_integer(index, &source[start..start + length], parameter)
                .map_err(|error| QuadDecodeError::Conversion { index, error })?;
            parameter.values[index] = Some(value);
        }
        Ok(true)
    }
}

/// All selectors in the same original DecodeParams four-atoi arm: TIMERON,
/// HUMAN_ADDWEAPON, RACING_MISSION6_INIT, QUAT_SLERP. Sorted by the numeric
/// IDs in pinned mafia-re C_program.h.
pub const QUAD_OPCODE_IDS: [u32; 4] = [50, 59, 167, 294];

/// Parameter-layout admission only; these opcodes do not become executable by
/// decoding (execution is admitted per opcode in dispatch).
pub fn supports_quad_opcode(opcode: u32) -> bool {
    QUAD_OPCODE_IDS.binary_search(&opcode).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Boundary {
        calls: Vec<(&'static str, usize, [Option<i32>; 4])>,
        fail_third: bool,
        null: bool,
    }

    impl QuadParameterBackend for Boundary {
        type Error = &'static str;

        fn allocate_quad(&mut self) -> Result<Option<Box<QuadParameter>>, Self::Error> {
            self.calls.push(("allocate", 0, [None, None, None, None]));
            Ok((!self.null).then(|| Box::new(QuadParameter::default())))
        }

        fn convert_integer(
            &mut self,
            index: usize,
            token: &[u8],
            published: &QuadParameter,
        ) -> Result<i32, Self::Error> {
            self.calls.push(("convert", index, published.values()));
            if index == 2 && self.fail_third {
                return Err("third conversion unavailable");
            }
            ascii_integer_prefix(token).map_err(|_| "unsupported integer locale")
        }
    }

    #[test]
    fn entire_four_integer_family_accepts_only_four_parameters() {
        assert_eq!(QUAD_OPCODE_IDS, [50, 59, 167, 294]);
        for opcode in QUAD_OPCODE_IDS {
            let mut source = b"-3suffix, +12tail, 7, 100\0".to_vec();
            let mut slot = QuadParameterSlot::default();
            assert_eq!(
                slot.decode(opcode, &mut source, &mut AsciiQuadParameterBackend),
                Ok(true),
                "opcode {opcode}"
            );
            assert_eq!(
                slot.parameter().unwrap().values(),
                [Some(-3), Some(12), Some(7), Some(100)],
                "opcode {opcode}"
            );
        }
        for opcode in [0, 10, 26, 49, 51, 58, 60, 166, 168, 293, 295] {
            assert!(!supports_quad_opcode(opcode), "opcode {opcode}");
        }
        for count in [0, 1, 2, 3, 5] {
            let mut source = Vec::new();
            for index in 0..count {
                if index > 0 {
                    source.push(b',');
                }
                source.push(b'1');
            }
            source.push(0);
            let mut slot = QuadParameterSlot::default();
            assert_eq!(
                slot.decode(59, &mut source, &mut AsciiQuadParameterBackend),
                Ok(false),
                "count {count}"
            );
            assert!(slot.parameter().is_none());
        }
    }

    #[test]
    fn allocation_and_conversion_failures_preserve_reached_prefix() {
        let mut slot = QuadParameterSlot::default();
        let mut backend = Boundary {
            null: true,
            ..Boundary::default()
        };
        let mut source = b"1,2,3,4\0".to_vec();
        assert_eq!(
            slot.decode(59, &mut source, &mut backend),
            Err(QuadDecodeError::NullAllocation)
        );
        let mut slot = QuadParameterSlot::default();
        let mut backend = Boundary {
            fail_third: true,
            ..Boundary::default()
        };
        let mut source = b"1,2,3,4\0".to_vec();
        assert_eq!(
            slot.decode(59, &mut source, &mut backend),
            Err(QuadDecodeError::Conversion {
                index: 2,
                error: "third conversion unavailable"
            })
        );
        assert_eq!(
            slot.parameter().unwrap().values(),
            [Some(1), Some(2), None, None]
        );
        assert_eq!(
            slot.decode(59, &mut source, &mut backend),
            Err(QuadDecodeError::AlreadyEntered)
        );
    }
}
