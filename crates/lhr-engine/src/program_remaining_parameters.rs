//! Owned C_program::DecodeParams branches for remaining MISE01 commands
//! that are not yet covered by other families. This module reconstructs
//! source decoding only, not command execution, native allocation failure, or
//! original-runtime equivalence.
//!
//! AUTOSAVEGAME, AUTOSAVEGAMEFULL and ZATMYSE follow DecodeParams at
//! Marvisak/mafia-re fe7e74643f5415c358aa312dd4e7d9da5f60ddcc,
//! game/mission/C_program.cpp (DecodeParams): two owned words, with an
//! optional second argument defaulting to 500 after the first conversion.
//! HUMAN_SETFIRETARGET defaults its second word to -1. EMITPARTICLE owns
//! three words and publishes the third-word default (-1) after conversion
//! of the second word, before any explicit third-word conversion.
//! Other selectors retain the legacy 1-3-integer fallback. That fallback
//! is not evidence of their original parameter layouts or execution.

#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RemainingParameter {
    pub values: Vec<Option<i32>>,
}

impl RemainingParameter {
    pub fn values(&self) -> &[Option<i32>] {
        &self.values
    }
}

pub trait RemainingParameterBackend {
    type Error;
    fn allocate_remaining(
        &mut self,
        count: usize,
    ) -> Result<Option<Box<RemainingParameter>>, Self::Error>;
    fn convert_remaining(
        &mut self,
        index: usize,
        token: &[u8],
        published: &RemainingParameter,
    ) -> Result<i32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiRemainingParameterBackend;
impl RemainingParameterBackend for AsciiRemainingParameterBackend {
    type Error = ProgramParserError;
    fn allocate_remaining(
        &mut self,
        count: usize,
    ) -> Result<Option<Box<RemainingParameter>>, Self::Error> {
        Ok(Some(Box::new(RemainingParameter {
            values: vec![None; count],
        })))
    }
    fn convert_remaining(
        &mut self,
        _: usize,
        token: &[u8],
        _: &RemainingParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum RemainingDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    Conversion { index: usize, error: E },
}

#[derive(Debug, Default)]
pub struct RemainingParameterSlot {
    entered: bool,
    parameter: Option<Box<RemainingParameter>>,
}

impl RemainingParameterSlot {
    pub fn parameter(&self) -> Option<&RemainingParameter> {
        self.parameter.as_deref()
    }

    pub fn decode<B: RemainingParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, RemainingDecodeError<B::Error>> {
        if self.entered {
            return Err(RemainingDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let params = extrude_params(source).map_err(RemainingDecodeError::Parser)?;
        if !supports_remaining_opcode(opcode) {
            return Err(RemainingDecodeError::UnsupportedOpcode(opcode));
        }
        let second_default = optional_second_default(opcode);
        let third_default = optional_third_default(opcode);
        let words = if second_default.is_some() {
            if params.count != 1 && params.count != 2 {
                return Ok(false);
            }
            2
        } else if third_default.is_some() {
            if params.count != 2 && params.count != 3 {
                return Ok(false);
            }
            3
        } else {
            // Legacy admission for selectors whose layouts are still unaudited.
            if params.count < 1 || params.count > 3 {
                return Ok(false);
            }
            params.count
        };
        self.parameter = backend
            .allocate_remaining(words)
            .map_err(RemainingDecodeError::Allocation)?;
        let p = self
            .parameter
            .as_mut()
            .ok_or(RemainingDecodeError::NullAllocation)?;
        for i in 0..params.count {
            let s = params.offsets[i].unwrap();
            let l = c_length(&source[s..]).map_err(RemainingDecodeError::Parser)?;
            let v = backend
                .convert_remaining(i, &source[s..s + l], p)
                .map_err(|e| RemainingDecodeError::Conversion { index: i, error: e })?;
            p.values[i] = Some(v);
            if i == 1 {
                if let Some(default) = third_default {
                    // EMITPARTICLE publishes -1 before converting its
                    // optional third token, including when conversion fails.
                    p.values[2] = Some(default);
                }
            }
        }
        // Do not publish the default before conversion of the first word, or
        // before an explicit second-word conversion that can fail or unwind.
        if params.count == 1 {
            if let Some(default) = second_default {
                p.values[1] = Some(default);
            }
        }
        Ok(true)
    }
}

fn optional_second_default(opcode: u32) -> Option<i32> {
    match opcode {
        129 | 130 | 234 => Some(500),
        142 => Some(-1),
        _ => None,
    }
}

fn optional_third_default(opcode: u32) -> Option<i32> {
    match opcode {
        113 => Some(-1),
        _ => None,
    }
}

pub const REMAINING_OPCODE_IDS: [u32; 109] = [
    5, 21, 22, 29, 32, 35, 36, 44, 45, 48, 49, 53, 54, 56, 60, 62, 67, 68, 72, 76, 77, 82, 83, 85,
    86, 87, 88, 90, 96, 101, 102, 103, 106, 111, 113, 116, 122, 126, 128, 129, 130, 135, 136, 137,
    138, 139, 141, 142, 144, 148, 153, 155, 171, 175, 181, 187, 199, 205, 208, 212, 218, 219, 222,
    227, 228, 229, 230, 231, 232, 234, 236, 237, 261, 263, 273, 274, 275, 282, 283, 284, 289, 291,
    295, 297, 298, 305, 308, 310, 317, 320, 322, 323, 326, 338, 342, 345, 349, 356, 358, 363, 365,
    367, 373, 377, 380, 385, 387, 390, 395,
];

pub fn supports_remaining_opcode(opcode: u32) -> bool {
    REMAINING_OPCODE_IDS.binary_search(&opcode).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_remaining_accepts_one_to_three_ints() {
        for opcode in REMAINING_OPCODE_IDS {
            if optional_second_default(opcode).is_some() || optional_third_default(opcode).is_some()
            {
                continue;
            }
            let mut buf = b"1,2,3\0".to_vec();
            let mut slot = RemainingParameterSlot::default();
            assert_eq!(
                slot.decode(opcode, &mut buf, &mut AsciiRemainingParameterBackend),
                Ok(true)
            );
            assert_eq!(
                slot.parameter().unwrap().values(),
                &[Some(1), Some(2), Some(3)]
            );
        }
        for opcode in [0, 1, 14, 20, 26, 27, 37, 52, 57, 70, 71, 73, 78, 132, 999] {
            assert!(!supports_remaining_opcode(opcode));
        }
        // Opcode 27 left Remaining for its own DetectorSignal family: the
        // Remaining slot now refuses it instead of admitting an empty word.
        let mut buf = b"\0".to_vec();
        let mut slot = RemainingParameterSlot::default();
        assert_eq!(
            slot.decode(27, &mut buf, &mut AsciiRemainingParameterBackend),
            Err(RemainingDecodeError::UnsupportedOpcode(27))
        );
        // Opcode 57 left Remaining for its own GetActorsDist family (3-or-4
        // words with defaulted fourth): Remaining now refuses it.
        let mut buf = b"\0".to_vec();
        let mut slot = RemainingParameterSlot::default();
        assert_eq!(
            slot.decode(57, &mut buf, &mut AsciiRemainingParameterBackend),
            Err(RemainingDecodeError::UnsupportedOpcode(57))
        );
    }
}

#[cfg(test)]
#[path = "program_optional_pair_tests.rs"]
mod optional_pair_tests;
