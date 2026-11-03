//! Owned C_program::DecodeParams branch for ACT_SETSTATE (opcode 26).
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_ACT_SETSTATE case with C_program.h ACT_STATE_* codes.
//! Exactly two comma-separated words: an actor register (negative selects the
//! script's own actor) and a state keyword (ACTIVE/INACTIVE, anything else is
//! ACT_STATE_NONE, compared case-insensitively). This module reconstructs
//! source decoding only, not command execution, native allocation failure, or
//! original-runtime equivalence.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Decoded ACT_STATE_* selector in upstream C_program.h order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActState {
    None = 0,
    Active = 1,
    Inactive = 2,
}

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ActStateParameter {
    actor: Option<i32>,
    state: Option<ActState>,
}

impl ActStateParameter {
    pub fn actor(&self) -> Option<i32> {
        self.actor
    }

    pub fn state(&self) -> Option<ActState> {
        self.state
    }
}

/// The actual allocation is transferred into the instruction before either
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait ActStateParameterBackend {
    type Error;
    fn allocate_act_state(&mut self) -> Result<Option<Box<ActStateParameter>>, Self::Error>;
    fn convert_actor(
        &mut self,
        token: &[u8],
        published: &ActStateParameter,
    ) -> Result<i32, Self::Error>;
    fn convert_state(
        &mut self,
        token: &[u8],
        published: &ActStateParameter,
    ) -> Result<ActState, Self::Error>;
}

#[derive(Default)]
pub struct AsciiActStateParameterBackend;

impl ActStateParameterBackend for AsciiActStateParameterBackend {
    type Error = ProgramParserError;

    fn allocate_act_state(&mut self) -> Result<Option<Box<ActStateParameter>>, Self::Error> {
        Ok(Some(Box::new(ActStateParameter::default())))
    }

    fn convert_actor(
        &mut self,
        token: &[u8],
        _published: &ActStateParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }

    fn convert_state(
        &mut self,
        token: &[u8],
        _published: &ActStateParameter,
    ) -> Result<ActState, Self::Error> {
        if token.eq_ignore_ascii_case(b"ACTIVE") {
            Ok(ActState::Active)
        } else if token.eq_ignore_ascii_case(b"INACTIVE") {
            Ok(ActState::Inactive)
        } else {
            Ok(ActState::None)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ActStateDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    ActorConversion(E),
    StateConversion(E),
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if conversion or a backend callback unwinds.
#[derive(Debug, Default)]
pub struct ActStateParameterSlot {
    entered: bool,
    parameter: Option<Box<ActStateParameter>>,
}

impl ActStateParameterSlot {
    pub fn parameter(&self) -> Option<&ActStateParameter> {
        self.parameter.as_deref()
    }

    /// Only a count of two is accepted. The source is split in place before
    /// count validation, as in the original DecodeParams call chain.
    pub fn decode<B: ActStateParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, ActStateDecodeError<B::Error>> {
        if self.entered {
            return Err(ActStateDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(ActStateDecodeError::Parser)?;
        if !supports_act_state_opcode(opcode) {
            return Err(ActStateDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 2 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_act_state()
            .map_err(ActStateDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(ActStateDecodeError::NullAllocation)?;
        let first = parameters.offsets[0].expect("counted parameter token");
        let first_length = c_length(&source[first..]).map_err(ActStateDecodeError::Parser)?;
        parameter.actor = Some(
            backend
                .convert_actor(&source[first..first + first_length], parameter)
                .map_err(ActStateDecodeError::ActorConversion)?,
        );
        let second = parameters.offsets[1].expect("counted parameter token");
        let second_length = c_length(&source[second..]).map_err(ActStateDecodeError::Parser)?;
        parameter.state = Some(
            backend
                .convert_state(&source[second..second + second_length], parameter)
                .map_err(ActStateDecodeError::StateConversion)?,
        );
        Ok(true)
    }
}

/// Parameter-layout admission only; this opcode does not become executable by
/// decoding (execution is admitted in dispatch).
pub fn supports_act_state_opcode(opcode: u32) -> bool {
    opcode == 26
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn act_setstate_accepts_only_two_words_with_keyword_state() {
        for (source, actor, state) in [
            (&b"-1,inactive\0"[..], -1, ActState::Inactive),
            (&b"0,ACTIVE\0"[..], 0, ActState::Active),
            (&b"2,active\0"[..], 2, ActState::Active),
            (&b"3,unknown\0"[..], 3, ActState::None),
        ] {
            let mut buffer = source.to_vec();
            let mut slot = ActStateParameterSlot::default();
            assert_eq!(
                slot.decode(26, &mut buffer, &mut AsciiActStateParameterBackend),
                Ok(true),
                "{source:?}"
            );
            let parameter = slot.parameter().unwrap();
            assert_eq!(parameter.actor(), Some(actor));
            assert_eq!(parameter.state(), Some(state));
        }
        assert!(supports_act_state_opcode(26));
        for opcode in [0, 25, 27, 59, 397] {
            assert!(!supports_act_state_opcode(opcode), "opcode {opcode}");
        }
        for source in [&b"\0"[..], &b"1\0"[..], &b"1,2,3\0"[..], &b"4,\0"[..]] {
            let mut buffer = source.to_vec();
            let mut slot = ActStateParameterSlot::default();
            assert_eq!(
                slot.decode(26, &mut buffer, &mut AsciiActStateParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        let mut buffer = b"0,ACTIVE\0".to_vec();
        let mut slot = ActStateParameterSlot::default();
        assert_eq!(
            slot.decode(27, &mut buffer, &mut AsciiActStateParameterBackend),
            Err(ActStateDecodeError::UnsupportedOpcode(27))
        );
    }
}
