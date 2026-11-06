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

/// Owned execution view of CMD_DETECTOR_INRANGE (29) for C_detector::Process
/// (mafia-re 113b5b6 game/actors/C_detector.cpp:205-227, GAME RVA 0x433d10):
/// S_int_float_params (game/mission/C_program.h:568-574) decoded as two
/// integers — value2 keeps integer text (`(float)atoi`), so the range word is
/// an integer cast to float. The decoded payload stays RemainingParameter;
/// this maps the two admitted words to the destination register plus the
/// precomputed squared range (`range² = value2²`, C_detector.cpp:208).
/// Only the native-valid two-word shape maps; any other admitted Remaining
/// shape fails closed so dispatch never invents register or range values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectorInRangeParameter {
    pub register: i32,
    pub range_squared: f32,
}

impl DetectorInRangeParameter {
    pub fn from_words(values: &[Option<i32>]) -> Option<Self> {
        if values.len() != 2 {
            return None;
        }
        let register = values[0]?;
        let range = values[1]?;
        let range = range as f32;
        Some(Self {
            register,
            range_squared: range * range,
        })
    }
}

/// Owned execution view of CMD_DETECTOR_WAITFORUSE (32) for
/// C_detector::Process (mafia-re 113b5b6 game/actors/C_detector.cpp:229-235):
/// the native two-integer params with missing words defaulting to 0
/// (game/mission/C_program.cpp:781-801, `num` 0..=2). The arm drops the
/// previous use object and calls CreateUO(first, second == 0); both parameter
/// meanings stay unknown per game/actors/C_detector.h:55-57, so the fields
/// keep neutral names. Extra admitted Remaining words (a non-native shape)
/// are ignored after the first two; an empty word list fails closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DetectorWaitForUseParameter {
    pub first: i32,
    pub second_negated: bool,
}

impl DetectorWaitForUseParameter {
    pub fn from_words(values: &[Option<i32>]) -> Option<Self> {
        let first = *values.first()?;
        Some(Self {
            first: first.unwrap_or(0),
            second_negated: values.get(1).copied().flatten().unwrap_or(0) == 0,
        })
    }
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

    #[test]
    fn inrange_maps_register_and_squared_integer_range() {
        // value2 keeps integer text ((float)atoi): a range word of 10 scans
        // range² = 100.0, and a negative word squares the same as positive.
        assert_eq!(
            DetectorInRangeParameter::from_words(&[Some(0), Some(10)]),
            Some(DetectorInRangeParameter {
                register: 0,
                range_squared: 100.0,
            })
        );
        assert_eq!(
            DetectorInRangeParameter::from_words(&[Some(2), Some(-3)]),
            Some(DetectorInRangeParameter {
                register: 2,
                range_squared: 9.0,
            })
        );
        // Only the native-valid two-word shape maps; any other admitted
        // Remaining shape fails closed instead of inventing values.
        for words in [
            vec![],
            vec![Some(0)],
            vec![None, Some(10)],
            vec![Some(0), None],
            vec![Some(0), Some(10), Some(1)],
        ] {
            assert_eq!(DetectorInRangeParameter::from_words(&words), None);
        }
    }

    #[test]
    fn wait_for_use_defaults_missing_words_to_zero() {
        // Native admits 0..=2 words with missing defaulting to 0
        // (C_program.cpp:781-801); decode admission never yields empty, which
        // fails closed. The second field negates like CreateUO(p[0], !p[1]).
        assert_eq!(DetectorWaitForUseParameter::from_words(&[]), None);
        assert_eq!(
            DetectorWaitForUseParameter::from_words(&[Some(5)]),
            Some(DetectorWaitForUseParameter {
                first: 5,
                second_negated: true,
            })
        );
        assert_eq!(
            DetectorWaitForUseParameter::from_words(&[Some(5), Some(0)]),
            Some(DetectorWaitForUseParameter {
                first: 5,
                second_negated: true,
            })
        );
        assert_eq!(
            DetectorWaitForUseParameter::from_words(&[Some(5), Some(1)]),
            Some(DetectorWaitForUseParameter {
                first: 5,
                second_negated: false,
            })
        );
        assert_eq!(
            DetectorWaitForUseParameter::from_words(&[None, None]),
            Some(DetectorWaitForUseParameter {
                first: 0,
                second_negated: true,
            })
        );
    }
}
