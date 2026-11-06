//! Owned C_program::DecodeParams branch for the command block opener (`{`, 70).
//!
//! Reference: Marvisak/mafia-re 113b5b6, game/mission/C_program.cpp,
//! DecodeParams CMD_BLOCK_BEGIN case (:705-719), CMD_* ids and BLOCK_MODE_*
//! in C_program.h (:82, :425-427). This module reconstructs source decoding
//! only, not command execution, native allocation failure, or
//! original-runtime equivalence.
//!
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
#![forbid(unsafe_code)]

use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Command block modes, the parameter of `{` (C_program.h names, after the
/// scripts).
pub const BLOCK_MODE_NONE: u8 = 0;
pub const BLOCK_MODE_UP: u8 = 1;
pub const BLOCK_MODE_DOWN: u8 = 2;

/// Stores are published one at a time. The mode is the decoded `{` word.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct BlockParameter {
    mode: Option<u8>,
}

impl BlockParameter {
    pub fn mode(&self) -> Option<u8> {
        self.mode
    }
}

/// The actual allocation is transferred into the instruction before the mode
/// word is read. The word compare runs inline, as the original `_strnicmp`
/// does; there is no numeric conversion callback to interrupt.
pub trait BlockParameterBackend {
    type Error;
    fn allocate_block(&mut self) -> Result<Option<Box<BlockParameter>>, Self::Error>;
}

#[derive(Default)]
pub struct AsciiBlockParameterBackend;

impl BlockParameterBackend for AsciiBlockParameterBackend {
    type Error = ProgramParserError;

    fn allocate_block(&mut self) -> Result<Option<Box<BlockParameter>>, Self::Error> {
        Ok(Some(Box::new(BlockParameter::default())))
    }
}

/// Original `{` word mapping: a leading `UP` (first two bytes,
/// case-insensitive) selects UP, else a leading `DOWN` (first four bytes)
/// selects DOWN, else the mode stays NONE. Shorter words never match, as the
/// native `_strnicmp` compares the NUL against the missing byte.
fn decode_block_mode(token: &[u8]) -> u8 {
    if token.len() >= 2 && token[..2].eq_ignore_ascii_case(b"UP") {
        BLOCK_MODE_UP
    } else if token.len() >= 4 && token[..4].eq_ignore_ascii_case(b"DOWN") {
        BLOCK_MODE_DOWN
    } else {
        BLOCK_MODE_NONE
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BlockDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if the allocation callback unwinds.
#[derive(Debug, Default)]
pub struct BlockParameterSlot {
    entered: bool,
    parameter: Option<Box<BlockParameter>>,
}

impl BlockParameterSlot {
    pub fn parameter(&self) -> Option<&BlockParameter> {
        self.parameter.as_deref()
    }

    /// Only counts of zero or one are accepted, matching the native
    /// `if (num < 2)` arm; longer lines reject the instruction. The source is
    /// split in place before count validation, as in the original DecodeParams
    /// call chain.
    pub fn decode<B: BlockParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, BlockDecodeError<B::Error>> {
        if self.entered {
            return Err(BlockDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(BlockDecodeError::Parser)?;
        if !supports_block_opcode(opcode) {
            return Err(BlockDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count > 1 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_block()
            .map_err(BlockDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(BlockDecodeError::NullAllocation)?;
        if parameters.count == 1 {
            let start = parameters.offsets[0].expect("counted parameter token");
            let length = c_length(&source[start..]).map_err(BlockDecodeError::Parser)?;
            parameter.mode = Some(decode_block_mode(&source[start..start + length]));
        } else {
            parameter.mode = Some(BLOCK_MODE_NONE);
        }
        Ok(true)
    }
}

/// Parameter-layout admission only; this opcode does not become executable by
/// decoding (execution is admitted in dispatch).
pub fn supports_block_opcode(opcode: u32) -> bool {
    opcode == 70
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(opcode: u32, text: &[u8]) -> Result<bool, BlockDecodeError<ProgramParserError>> {
        let mut backend = AsciiBlockParameterBackend;
        let mut slot = BlockParameterSlot::default();
        let mut source = text.to_vec();
        source.push(0);
        slot.decode(opcode, &mut source, &mut backend)
    }

    fn mode_of(text: &[u8]) -> u8 {
        let mut backend = AsciiBlockParameterBackend;
        let mut slot = BlockParameterSlot::default();
        let mut source = text.to_vec();
        source.push(0);
        assert_eq!(slot.decode(70, &mut source, &mut backend), Ok(true));
        slot.parameter().unwrap().mode().unwrap()
    }

    #[test]
    fn block_words_map_to_modes_with_native_prefix_strictness() {
        assert_eq!(mode_of(b""), BLOCK_MODE_NONE);
        assert_eq!(mode_of(b"UP"), BLOCK_MODE_UP);
        assert_eq!(mode_of(b"up"), BLOCK_MODE_UP);
        assert_eq!(mode_of(b"Upstairs"), BLOCK_MODE_UP);
        assert_eq!(mode_of(b"DOWN"), BLOCK_MODE_DOWN);
        assert_eq!(mode_of(b"down"), BLOCK_MODE_DOWN);
        assert_eq!(mode_of(b"DOWNSTREAM"), BLOCK_MODE_DOWN);
        // Shorter words never match the native NUL-terminated compare.
        assert_eq!(mode_of(b"U"), BLOCK_MODE_NONE);
        assert_eq!(mode_of(b"DOW"), BLOCK_MODE_NONE);
        // Anything else, including numbers, stays NONE.
        assert_eq!(mode_of(b"1"), BLOCK_MODE_NONE);
        assert_eq!(mode_of(b"SIDEWAYS"), BLOCK_MODE_NONE);
        // UP wins over DOWN where both prefixes match, as the original tests
        // UP first.
        assert_eq!(mode_of(b"UPDOWN"), BLOCK_MODE_UP);
    }

    #[test]
    fn block_rejects_counts_and_foreign_opcodes() {
        assert_eq!(decode(70, b"UP,DOWN"), Ok(false));
        assert_eq!(decode(70, b"a,b"), Ok(false));
        assert_eq!(
            decode(71, b"UP"),
            Err(BlockDecodeError::UnsupportedOpcode(71))
        );
        assert_eq!(
            decode(20, b""),
            Err(BlockDecodeError::UnsupportedOpcode(20))
        );
        let mut backend = AsciiBlockParameterBackend;
        let mut slot = BlockParameterSlot::default();
        let mut first = b"UP\0".to_vec();
        assert_eq!(slot.decode(70, &mut first, &mut backend), Ok(true));
        let mut second = b"DOWN\0".to_vec();
        assert_eq!(
            slot.decode(70, &mut second, &mut backend),
            Err(BlockDecodeError::AlreadyEntered)
        );
    }

    #[test]
    fn refused_allocation_reports_null_without_payload() {
        struct NullBackend;
        impl BlockParameterBackend for NullBackend {
            type Error = &'static str;
            fn allocate_block(&mut self) -> Result<Option<Box<BlockParameter>>, Self::Error> {
                Ok(None)
            }
        }
        let mut backend = NullBackend;
        let mut slot = BlockParameterSlot::default();
        let mut source = b"UP\0".to_vec();
        assert_eq!(
            slot.decode(70, &mut source, &mut backend),
            Err(BlockDecodeError::NullAllocation)
        );
        assert!(slot.parameter().is_none());
    }

    #[test]
    fn interrupted_allocation_leaves_no_payload() {
        struct FailingBackend;
        impl BlockParameterBackend for FailingBackend {
            type Error = &'static str;
            fn allocate_block(&mut self) -> Result<Option<Box<BlockParameter>>, Self::Error> {
                Err("interrupted")
            }
        }
        let mut backend = FailingBackend;
        let mut slot = BlockParameterSlot::default();
        let mut source = b"DOWN\0".to_vec();
        assert_eq!(
            slot.decode(70, &mut source, &mut backend),
            Err(BlockDecodeError::Allocation("interrupted"))
        );
        assert!(slot.parameter().is_none());
    }
}
