//! Owned C_program::DecodeParams branch for HUMAN_TALK (132) and related
//! dialogue subtitle commands (239/240). This module reconstructs source
//! decoding only, not command execution, native allocation failure, or
//! original-runtime equivalence.
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_HUMAN_TALK 1364-1378 (int[4] with v[3]=-1), and the
//! subtitle single/double-int groups. Dialogue camera commands (78/79/80) are
//! already decoded via the pair family (int[2]) and remain there.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct HumanTalkParameter {
    values: [Option<i32>; 4],
}

impl HumanTalkParameter {
    pub fn values(&self) -> [Option<i32>; 4] {
        self.values
    }

    pub fn channel_mut(&mut self) -> &mut Option<i32> {
        &mut self.values[3]
    }
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait HumanTalkParameterBackend {
    type Error;
    fn allocate_human_talk(&mut self) -> Result<Option<Box<HumanTalkParameter>>, Self::Error>;
    fn convert_human_talk(
        &mut self,
        index: usize,
        token: &[u8],
        published: &HumanTalkParameter,
    ) -> Result<i32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiHumanTalkParameterBackend;

impl HumanTalkParameterBackend for AsciiHumanTalkParameterBackend {
    type Error = ProgramParserError;

    fn allocate_human_talk(&mut self) -> Result<Option<Box<HumanTalkParameter>>, Self::Error> {
        Ok(Some(Box::new(HumanTalkParameter::default())))
    }

    fn convert_human_talk(
        &mut self,
        _index: usize,
        token: &[u8],
        _published: &HumanTalkParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum HumanTalkDecodeError<E> {
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
pub struct HumanTalkParameterSlot {
    entered: bool,
    parameter: Option<Box<HumanTalkParameter>>,
}

impl HumanTalkParameterSlot {
    pub fn parameter(&self) -> Option<&HumanTalkParameter> {
        self.parameter.as_deref()
    }

    pub fn parameter_mut(&mut self) -> Option<&mut HumanTalkParameter> {
        self.parameter.as_deref_mut()
    }

    /// Only counts of two or three are accepted. The source is split in
    /// place before count validation, as in the original DecodeParams call
    /// chain. The fourth word is the persistent channel, initialized to -1.
    pub fn decode<B: HumanTalkParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, HumanTalkDecodeError<B::Error>> {
        if self.entered {
            return Err(HumanTalkDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(HumanTalkDecodeError::Parser)?;
        if !supports_human_talk_opcode(opcode) {
            return Err(HumanTalkDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 2 && parameters.count != 3 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_human_talk()
            .map_err(HumanTalkDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(HumanTalkDecodeError::NullAllocation)?;
        parameter.values = [None, None, None, Some(-1)];

        for index in 0..parameters.count {
            let start = parameters.offsets[index].expect("counted parameter token");
            let length = c_length(&source[start..]).map_err(HumanTalkDecodeError::Parser)?;
            let value = backend
                .convert_human_talk(index, &source[start..start + length], parameter)
                .map_err(|error| HumanTalkDecodeError::Conversion { index, error })?;
            parameter.values[index] = Some(value);
        }
        // Third word (kind) defaults to 0 when only two params
        if parameters.count == 2 {
            parameter.values[2] = Some(0);
        }
        Ok(true)
    }
}

/// Parameter-layout admission only; this opcode does not become executable by
/// decoding (execution is admitted in dispatch).
pub fn supports_human_talk_opcode(opcode: u32) -> bool {
    opcode == 132
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_talk_accepts_two_or_three_words_with_channel_minus_one() {
        for (source, expected) in [
            (&b"0,100\0"[..], [Some(0), Some(100), Some(0), Some(-1)]),
            (&b"1, 200, 3\0"[..], [Some(1), Some(200), Some(3), Some(-1)]),
            (&b"-1, 400\0"[..], [Some(-1), Some(400), Some(0), Some(-1)]),
        ] {
            let mut buffer = source.to_vec();
            let mut slot = HumanTalkParameterSlot::default();
            assert_eq!(
                slot.decode(132, &mut buffer, &mut AsciiHumanTalkParameterBackend),
                Ok(true),
                "{source:?}"
            );
            assert_eq!(slot.parameter().unwrap().values(), expected);
        }
        assert!(supports_human_talk_opcode(132));
        for opcode in [0, 78, 79, 80, 133, 239, 240, 397] {
            assert!(!supports_human_talk_opcode(opcode), "opcode {opcode}");
        }
        for source in [
            &b"\0"[..],
            &b"1\0"[..],
            &b"1,2,3,4\0"[..],
            &b"1,2,3,4,5\0"[..],
        ] {
            let mut buffer = source.to_vec();
            let mut slot = HumanTalkParameterSlot::default();
            assert_eq!(
                slot.decode(132, &mut buffer, &mut AsciiHumanTalkParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        let mut buffer = b"0,100\0".to_vec();
        let mut slot = HumanTalkParameterSlot::default();
        assert_eq!(
            slot.decode(78, &mut buffer, &mut AsciiHumanTalkParameterBackend),
            Err(HumanTalkDecodeError::UnsupportedOpcode(78))
        );
    }

    #[test]
    fn allocation_and_conversion_failures_preserve_reached_prefix() {
        struct FailBackend {
            fail_index: Option<usize>,
            null: bool,
        }
        impl HumanTalkParameterBackend for FailBackend {
            type Error = &'static str;
            fn allocate_human_talk(
                &mut self,
            ) -> Result<Option<Box<HumanTalkParameter>>, Self::Error> {
                if self.null {
                    return Ok(None);
                }
                Ok(Some(Box::new(HumanTalkParameter::default())))
            }
            fn convert_human_talk(
                &mut self,
                index: usize,
                token: &[u8],
                published: &HumanTalkParameter,
            ) -> Result<i32, Self::Error> {
                if Some(index) == self.fail_index {
                    return Err("injected");
                }
                assert_eq!(published.values[3], Some(-1));
                ascii_integer_prefix(token).map_err(|_| "bad int")
            }
        }
        let mut slot = HumanTalkParameterSlot::default();
        let mut backend = FailBackend {
            fail_index: None,
            null: true,
        };
        let mut source = b"0,100\0".to_vec();
        assert_eq!(
            slot.decode(132, &mut source, &mut backend),
            Err(HumanTalkDecodeError::NullAllocation)
        );
        let mut slot = HumanTalkParameterSlot::default();
        let mut backend = FailBackend {
            fail_index: Some(1),
            null: false,
        };
        let mut source = b"0,100,3\0".to_vec();
        assert_eq!(
            slot.decode(132, &mut source, &mut backend),
            Err(HumanTalkDecodeError::Conversion {
                index: 1,
                error: "injected"
            })
        );
        assert_eq!(
            slot.parameter().unwrap().values(),
            [Some(0), None, None, Some(-1)]
        );
        assert_eq!(
            slot.decode(132, &mut source, &mut backend),
            Err(HumanTalkDecodeError::AlreadyEntered)
        );
    }
}
