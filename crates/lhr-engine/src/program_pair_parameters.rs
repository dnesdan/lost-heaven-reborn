//! Owned two-integer C_program::DecodeParams branch for FRM_SETON/FRM_SETON1.
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_FRM_SETON / CMD_FRM_SETON1. These commands allocate
//! two native int words and store atoi(first), then atoi(second).
//! This module reconstructs source decoding only, not command execution,
//! native allocation failure, or original-runtime equivalence.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PairParameter {
    values: [Option<i32>; 2],
}

impl PairParameter {
    pub fn values(&self) -> [Option<i32>; 2] {
        self.values
    }
}

/// The actual allocation is transferred into the instruction before either
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait PairParameterBackend {
    type Error;
    fn allocate_pair(&mut self) -> Result<Option<Box<PairParameter>>, Self::Error>;
    fn convert_integer(
        &mut self,
        index: usize,
        token: &[u8],
        published: &PairParameter,
    ) -> Result<i32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiPairParameterBackend;

impl PairParameterBackend for AsciiPairParameterBackend {
    type Error = ProgramParserError;

    fn allocate_pair(&mut self) -> Result<Option<Box<PairParameter>>, Self::Error> {
        Ok(Some(Box::new(PairParameter::default())))
    }

    fn convert_integer(
        &mut self,
        _index: usize,
        token: &[u8],
        _published: &PairParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PairDecodeError<E> {
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
pub struct PairParameterSlot {
    entered: bool,
    parameter: Option<Box<PairParameter>>,
}

impl PairParameterSlot {
    pub fn parameter(&self) -> Option<&PairParameter> {
        self.parameter.as_deref()
    }

    /// Only a count of two is accepted. The source is split in place before
    /// count validation, as in the original DecodeParams call chain.
    pub fn decode<B: PairParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, PairDecodeError<B::Error>> {
        if self.entered {
            return Err(PairDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(PairDecodeError::Parser)?;
        if !supports_pair_opcode(opcode) {
            return Err(PairDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 2 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_pair()
            .map_err(PairDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(PairDecodeError::NullAllocation)?;
        parameter.values = [None, None];

        for index in 0..2 {
            let start = parameters.offsets[index].expect("counted parameter token");
            let length = c_length(&source[start..]).map_err(PairDecodeError::Parser)?;
            let value = backend
                .convert_integer(index, &source[start..start + length], parameter)
                .map_err(|error| PairDecodeError::Conversion { index, error })?;
            parameter.values[index] = Some(value);
        }
        Ok(true)
    }
}

/// Exactly the two reached command selectors admitted in this milestone.
/// Other two-int selectors remain explicit unsupported parameters.
pub fn supports_pair_opcode(opcode: u32) -> bool {
    matches!(opcode, 14 | 15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Boundary {
        calls: Vec<(&'static str, usize, [Option<i32>; 2])>,
        fail_second: bool,
        null: bool,
    }

    impl PairParameterBackend for Boundary {
        type Error = &'static str;

        fn allocate_pair(&mut self) -> Result<Option<Box<PairParameter>>, Self::Error> {
            self.calls.push(("allocate", 0, [None, None]));
            Ok((!self.null).then(|| Box::new(PairParameter::default())))
        }

        fn convert_integer(
            &mut self,
            index: usize,
            token: &[u8],
            published: &PairParameter,
        ) -> Result<i32, Self::Error> {
            self.calls.push(("convert", index, published.values()));
            if index == 1 && self.fail_second {
                return Err("second conversion unavailable");
            }
            ascii_integer_prefix(token).map_err(|_| "unsupported integer locale")
        }
    }

    #[test]
    fn both_selectors_store_two_original_integer_prefixes_in_order() {
        for opcode in [14, 15] {
            let mut slot = PairParameterSlot::default();
            let mut backend = Boundary::default();
            let mut source = b" -17suffix , +4294967295more \0untouched".to_vec();
            assert_eq!(slot.decode(opcode, &mut source, &mut backend), Ok(true));
            assert_eq!(slot.parameter().unwrap().values(), [Some(-17), Some(-1)]);
            assert_eq!(
                backend.calls,
                [
                    ("allocate", 0, [None, None]),
                    ("convert", 0, [None, None]),
                    ("convert", 1, [Some(-17), None]),
                ]
            );
            assert_eq!(&source[..11], b" -17suffix\0");
            assert!(source.ends_with(b"\0untouched"));
        }
    }

    #[test]
    fn count_rejection_does_not_allocate_but_preserves_tokenization() {
        for source in [b"1\0".as_slice(), b"1,\0", b"1,2,3\0"] {
            let mut bytes = source.to_vec();
            let mut slot = PairParameterSlot::default();
            let mut backend = Boundary::default();
            assert_eq!(slot.decode(14, &mut bytes, &mut backend), Ok(false));
            assert!(slot.parameter().is_none());
            assert!(backend.calls.is_empty());
            if source.contains(&b',') {
                assert!(!bytes.contains(&b','));
            }
        }
    }

    #[test]
    fn second_conversion_failure_retains_first_store_and_cannot_replay() {
        let mut slot = PairParameterSlot::default();
        let mut backend = Boundary {
            fail_second: true,
            ..Boundary::default()
        };
        let mut source = b"2,3\0".to_vec();
        assert_eq!(
            slot.decode(14, &mut source, &mut backend),
            Err(PairDecodeError::Conversion {
                index: 1,
                error: "second conversion unavailable"
            })
        );
        assert_eq!(slot.parameter().unwrap().values(), [Some(2), None]);
        let calls = backend.calls.clone();
        let bytes = source.clone();
        assert_eq!(
            slot.decode(14, &mut source, &mut backend),
            Err(PairDecodeError::AlreadyEntered)
        );
        assert_eq!(backend.calls, calls);
        assert_eq!(source, bytes);
    }

    #[test]
    fn null_allocation_and_unsupported_opcode_fail_closed() {
        let mut backend = Boundary {
            null: true,
            ..Boundary::default()
        };
        let mut slot = PairParameterSlot::default();
        assert_eq!(
            slot.decode(15, &mut b"0,1\0".to_vec(), &mut backend),
            Err(PairDecodeError::NullAllocation)
        );
        assert!(slot.parameter().is_none());

        let mut input = b"2,3\0".to_vec();
        let mut second = PairParameterSlot::default();
        assert_eq!(
            second.decode(16, &mut input, &mut Boundary::default()),
            Err(PairDecodeError::UnsupportedOpcode(16))
        );
        assert_eq!(input, [b'2', 0, b'3', 0]);
    }
}
