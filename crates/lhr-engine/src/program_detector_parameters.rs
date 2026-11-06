//! Owned C_program::DecodeParams branch for DETECTOR_ISSIGNAL (27).
//!
//! Reference: mafia-re 113b5b6, game/mission/C_program.cpp, DecodeParams
//! CMD_DETECTOR_ISSIGNAL case (:1158-1168) and S_signal_params in
//! C_program.h (:552-557): three words, an atoi register plus two raw labels
//! of at most 15 bytes each. This module reconstructs source decoding only,
//! not command execution, native allocation failure, or original-runtime
//! equivalence.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct DetectorSignalParameter {
    register: Option<i32>,
    label1: Option<Box<[u8]>>,
    label2: Option<Box<[u8]>>,
}

impl DetectorSignalParameter {
    pub fn register(&self) -> Option<i32> {
        self.register
    }

    pub fn label1(&self) -> Option<&[u8]> {
        self.label1.as_deref()
    }

    pub fn label2(&self) -> Option<&[u8]> {
        self.label2.as_deref()
    }
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait DetectorSignalParameterBackend {
    type Error;
    fn allocate_detector_signal(
        &mut self,
    ) -> Result<Option<Box<DetectorSignalParameter>>, Self::Error>;
    fn convert_detector_signal_register(
        &mut self,
        token: &[u8],
        published: &DetectorSignalParameter,
    ) -> Result<i32, Self::Error>;
}

#[derive(Default)]
pub struct AsciiDetectorSignalParameterBackend;

impl DetectorSignalParameterBackend for AsciiDetectorSignalParameterBackend {
    type Error = ProgramParserError;

    fn allocate_detector_signal(
        &mut self,
    ) -> Result<Option<Box<DetectorSignalParameter>>, Self::Error> {
        Ok(Some(Box::new(DetectorSignalParameter::default())))
    }

    fn convert_detector_signal_register(
        &mut self,
        token: &[u8],
        _published: &DetectorSignalParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum DetectorSignalDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    RegisterConversion(E),
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if conversion or a backend callback unwinds.
#[derive(Debug, Default)]
pub struct DetectorSignalParameterSlot {
    entered: bool,
    parameter: Option<Box<DetectorSignalParameter>>,
}

impl DetectorSignalParameterSlot {
    pub fn parameter(&self) -> Option<&DetectorSignalParameter> {
        self.parameter.as_deref()
    }

    /// Only a count of three is accepted, with both labels (unquoted names of
    /// at most 15 bytes each) copied raw. The source is split in place before
    /// count validation, as in the original DecodeParams call chain.
    pub fn decode<B: DetectorSignalParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, DetectorSignalDecodeError<B::Error>> {
        if self.entered {
            return Err(DetectorSignalDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(DetectorSignalDecodeError::Parser)?;
        if !supports_detector_signal_opcode(opcode) {
            return Err(DetectorSignalDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 3 {
            return Ok(false);
        }
        let label1_start = parameters.offsets[1].expect("counted parameter token");
        let label1_length =
            c_length(&source[label1_start..]).map_err(DetectorSignalDecodeError::Parser)?;
        if label1_length > 15 {
            return Ok(false);
        }
        let label2_start = parameters.offsets[2].expect("counted parameter token");
        let label2_length =
            c_length(&source[label2_start..]).map_err(DetectorSignalDecodeError::Parser)?;
        if label2_length > 15 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_detector_signal()
            .map_err(DetectorSignalDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(DetectorSignalDecodeError::NullAllocation)?;
        let first = parameters.offsets[0].expect("counted parameter token");
        let first_length = c_length(&source[first..]).map_err(DetectorSignalDecodeError::Parser)?;
        parameter.register = Some(
            backend
                .convert_detector_signal_register(
                    &source[first..first + first_length],
                    parameter,
                )
                .map_err(DetectorSignalDecodeError::RegisterConversion)?,
        );
        parameter.label1 = Some(
            source[label1_start..label1_start + label1_length]
                .to_vec()
                .into_boxed_slice(),
        );
        parameter.label2 = Some(
            source[label2_start..label2_start + label2_length]
                .to_vec()
                .into_boxed_slice(),
        );
        Ok(true)
    }
}

/// Parameter-layout admission only; execution is admitted in dispatch.
pub fn supports_detector_signal_opcode(opcode: u32) -> bool {
    opcode == 27
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detector_signal_accepts_register_and_two_raw_labels() {
        let mut buffer = b"0,hit,miss\0".to_vec();
        let mut slot = DetectorSignalParameterSlot::default();
        assert_eq!(
            slot.decode(27, &mut buffer, &mut AsciiDetectorSignalParameterBackend),
            Ok(true)
        );
        let parameter = slot.parameter().unwrap();
        assert_eq!(parameter.register(), Some(0));
        assert_eq!(parameter.label1(), Some(&b"hit"[..]));
        assert_eq!(parameter.label2(), Some(&b"miss"[..]));
        // Interior empty arguments count, so an empty first label decodes;
        // dispatch falls through past it like NextCommand(NULL).
        let mut buffer = b"0,,miss\0".to_vec();
        let mut slot = DetectorSignalParameterSlot::default();
        assert_eq!(
            slot.decode(27, &mut buffer, &mut AsciiDetectorSignalParameterBackend),
            Ok(true)
        );
        assert_eq!(slot.parameter().unwrap().label1(), Some(&b""[..]));
        assert!(supports_detector_signal_opcode(27));
        for opcode in [0, 26, 28, 29, 74, 397] {
            assert!(!supports_detector_signal_opcode(opcode), "opcode {opcode}");
        }
    }

    #[test]
    fn detector_signal_refuses_long_labels_and_other_counts() {
        // Fifteen bytes admit, sixteen refuse, per label independently.
        let mut buffer = b"1,0123456789abcde,ok\0".to_vec();
        let mut slot = DetectorSignalParameterSlot::default();
        assert_eq!(
            slot.decode(27, &mut buffer, &mut AsciiDetectorSignalParameterBackend),
            Ok(true)
        );
        for source in [
            &b"1,0123456789abcdef,ok\0"[..],
            &b"1,ok,0123456789abcdef\0"[..],
        ] {
            let mut buffer = source.to_vec();
            let mut slot = DetectorSignalParameterSlot::default();
            assert_eq!(
                slot.decode(27, &mut buffer, &mut AsciiDetectorSignalParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        for source in [&b"\0"[..], &b"1,hit\0"[..], &b"1,hit,miss,extra\0"[..]] {
            let mut buffer = source.to_vec();
            let mut slot = DetectorSignalParameterSlot::default();
            assert_eq!(
                slot.decode(27, &mut buffer, &mut AsciiDetectorSignalParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        let mut buffer = b"1,hit,miss\0".to_vec();
        let mut slot = DetectorSignalParameterSlot::default();
        assert_eq!(
            slot.decode(28, &mut buffer, &mut AsciiDetectorSignalParameterBackend),
            Err(DetectorSignalDecodeError::UnsupportedOpcode(28))
        );
    }

    #[test]
    fn conversion_failure_preserves_reached_prefix() {
        let mut slot = DetectorSignalParameterSlot::default();
        let mut backend = AsciiDetectorSignalParameterBackend;
        // Non-ASCII byte triggers UnsupportedCrtByte after allocation.
        let mut source = b"\xFF,hit,miss\0".to_vec();
        assert!(slot.decode(27, &mut source, &mut backend).is_err());
        assert_eq!(
            slot.parameter().unwrap(),
            &DetectorSignalParameter {
                register: None,
                label1: None,
                label2: None,
            }
        );
        assert_eq!(
            slot.decode(27, &mut source, &mut backend),
            Err(DetectorSignalDecodeError::AlreadyEntered)
        );
    }
}
