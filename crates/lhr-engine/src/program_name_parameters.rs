//! FINDFRAME / FINDACTOR / RECADDACTOR parameters on the existing tokenizer.
//! Reference: mafia-re 6c9f71c9bb52da7188eced90f9f732e7290657e9,
//! game/mission/C_program.cpp DecodeParams and C_program.h S_name_params.
//! This is source-backed parameter handling, not original-runtime equivalence
//! or execution of the three commands. The original fixed name field is 64 bytes.
#![forbid(unsafe_code)]

use crate::program_parser::{c_length, extrude_params, extrude_string, ProgramParserError};

const NAME_CAPACITY: usize = 64;

/// One payload owner. Only the written register and terminated name are exposed;
/// the unused host array tail is not evidence of initialized native storage.
#[derive(Debug, PartialEq, Eq)]
pub struct NameParameter {
    register: Option<i32>,
    name_bytes: [u8; NAME_CAPACITY],
    name_len: Option<usize>,
}
impl Default for NameParameter {
    fn default() -> Self {
        Self {
            register: None,
            name_bytes: [0; NAME_CAPACITY],
            name_len: None,
        }
    }
}
impl NameParameter {
    pub fn register(&self) -> Option<i32> {
        self.register
    }
    /// Original bytes, without the NUL. No UTF-8 conversion or case folding.
    pub fn name(&self) -> Option<&[u8]> {
        self.name_len.map(|length| &self.name_bytes[..length])
    }
    fn write_name(&mut self, name: &[u8]) {
        self.name_bytes[..name.len()].copy_from_slice(name);
        self.name_bytes[name.len()] = 0;
        self.name_len = Some(name.len());
    }
}

/// Required effect boundaries. Allocation transfers into the slot before the
/// integer conversion. Rejection releases only after that conversion/store.
/// These callbacks cannot replace the slot or reenter it while it is borrowed.
pub trait NameParameterBackend {
    type Error;
    fn allocate_name(&mut self) -> Result<Option<Box<NameParameter>>, Self::Error>;
    fn convert_register(
        &mut self,
        token: &[u8],
        published: &NameParameter,
    ) -> Result<i32, Self::Error>;
    fn release_rejected_name(&mut self, published: &NameParameter) -> Result<(), Self::Error>;
}

#[derive(Debug, PartialEq, Eq)]
pub enum NameDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    Conversion(E),
    RejectionRelease(E),
    /// Host boundary before the original strcpy would exceed its 64-byte field.
    /// The closing-quote write, allocation and register store have already run.
    NameTooLong { length: usize },
}

/// One fresh instruction slot. A returned error or unwound callback cannot be
/// retried against possibly released external backing. Dropping the retained
/// Box is host cleanup, not a repeated native free callback.
#[derive(Debug, Default)]
pub struct NameParameterSlot {
    entered: bool,
    parameter: Option<Box<NameParameter>>,
}
impl NameParameterSlot {
    pub fn parameter(&self) -> Option<&NameParameter> {
        self.parameter.as_deref()
    }
    pub fn decode<B: NameParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, NameDecodeError<B::Error>> {
        if self.entered {
            return Err(NameDecodeError::AlreadyEntered);
        }
        self.entered = true;
        // Common tokenization happens even on unsupported/count-rejected arms.
        let parameters = extrude_params(source).map_err(NameDecodeError::Parser)?;
        if !supports_name_opcode(opcode) {
            return Err(NameDecodeError::UnsupportedOpcode(opcode));
        }
        if !(1..=2).contains(&parameters.count) {
            return Ok(false);
        }
        self.parameter = backend.allocate_name().map_err(NameDecodeError::Allocation)?;
        let parameter = self.parameter.as_mut().ok_or(NameDecodeError::NullAllocation)?;
        parameter.register = None;
        parameter.name_len = None;
        let first = parameters.offsets[0].expect("counted first token");
        let length = c_length(&source[first..]).map_err(NameDecodeError::Parser)?;
        parameter.register = Some(
            backend
                .convert_register(&source[first..first + length], parameter)
                .map_err(NameDecodeError::Conversion)?,
        );
        if parameters.count == 1 {
            parameter.write_name(b"");
            return Ok(true);
        }
        let second = parameters.offsets[1].expect("counted second token");
        let range = extrude_string(&mut source[second..]).map_err(NameDecodeError::Parser)?;
        let Some(range) = range else {
            // Native failure leaves an unusable freed pointer. Rust retains no
            // live payload after successful release; it is never appended.
            backend
                .release_rejected_name(parameter)
                .map_err(NameDecodeError::RejectionRelease)?;
            self.parameter = None;
            return Ok(false);
        };
        if range.len() >= NAME_CAPACITY {
            return Err(NameDecodeError::NameTooLong { length: range.len() });
        }
        parameter.write_name(&source[second + range.start..second + range.end]);
        Ok(true)
    }
}

pub fn supports_name_opcode(opcode: u32) -> bool {
    matches!(opcode, 13 | 18 | 43)
}

#[cfg(test)]
#[path = "program_name_parameters_tests.rs"]
mod tests;
