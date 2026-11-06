//! Owned C_program::DecodeParams branches for remaining MISE01 commands
//! that are not yet covered by other families. This module reconstructs
//! source decoding only, not command execution, native allocation failure, or
//! original-runtime equivalence.
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp
//! DecodeParams groups for the remaining 18 commands (autosavegame etc).
//! For minimal parity we admit 1-3 integer words as int[] with atoi, which
//! matches the native int[1]/int[2]/int[3] groups for these commands.

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
        // Remaining commands admit 1-3 words (most are 1 or 2, a few 3). We admit any 1-3.
        if params.count < 1 || params.count > 3 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_remaining(params.count)
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
        }
        Ok(true)
    }
}

pub const REMAINING_OPCODE_IDS: [u32; 111] = [
    5, 21, 22, 27, 29, 32, 35, 36, 44, 45, 48, 49, 53, 54, 56, 57, 60, 62, 67, 68, 72, 76,
    77, 82, 83, 85, 86, 87, 88, 90, 96, 101, 102, 103, 106, 111, 113, 116, 122, 126, 128, 129, 130,
    135, 136, 137, 138, 139, 141, 142, 144, 148, 153, 155, 171, 175, 181, 187, 199, 205, 208, 212,
    218, 219, 222, 227, 228, 229, 230, 231, 232, 234, 236, 237, 261, 263, 273, 274, 275, 282, 283,
    284, 289, 291, 295, 297, 298, 305, 308, 310, 317, 320, 322, 323, 326, 338, 342, 345, 349, 356,
    358, 363, 365, 367, 373, 377, 380, 385, 387, 390, 395,
];

pub fn supports_remaining_opcode(opcode: u32) -> bool {
    REMAINING_OPCODE_IDS.binary_search(&opcode).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remaining_accepts_one_to_three_ints() {
        for opcode in REMAINING_OPCODE_IDS {
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
        for opcode in [0, 1, 14, 20, 26, 37, 52, 70, 71, 73, 78, 132, 999] {
            assert!(!supports_remaining_opcode(opcode));
        }
        let mut buf = b"\0".to_vec();
        let mut slot = RemainingParameterSlot::default();
        assert_eq!(
            slot.decode(27, &mut buf, &mut AsciiRemainingParameterBackend),
            Ok(false)
        );
    }
}
