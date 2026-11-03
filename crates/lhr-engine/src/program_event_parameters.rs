//! Owned C_program::DecodeParams branches for EVENT (73) and SETEVENT (74).
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_EVENT case (:677-708) and CMD_SETEVENT case (:1173-1183),
//! event id constants EVENT_USE_AB..EVENT_DETECTOR_HIT in C_program.h (:417-422)
//! and S_event_params in C_program.h (:559-566). This module reconstructs
//! source decoding only, not command execution, native allocation failure, or
//! original-runtime equivalence.
#![forbid(unsafe_code)]

use crate::program_parameters::ascii_integer_prefix;
use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Upstream-assigned event ids named after the scripts (C_program.h notes the
/// names are theirs, after the scripts').
pub const EVENT_USE_AB: i32 = 60000;
pub const EVENT_DEATH: i32 = 60001;
pub const EVENT_ENEMY_TO_AI: i32 = 60002;
pub const EVENT_ENEMY_TO_SCRIPT: i32 = 60003;
pub const EVENT_HIT_FROM_PLAYER: i32 = 60004;
pub const EVENT_DETECTOR_HIT: i32 = 60005;

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EventParameter {
    id: Option<i32>,
    extra: Option<i32>,
}

impl EventParameter {
    pub fn id(&self) -> Option<i32> {
        self.id
    }

    pub fn extra(&self) -> Option<i32> {
        self.extra
    }
}

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SetEventParameter {
    register: Option<i32>,
    event: Option<i32>,
    label: Option<Box<[u8]>>,
}

impl SetEventParameter {
    pub fn register(&self) -> Option<i32> {
        self.register
    }

    pub fn event(&self) -> Option<i32> {
        self.event
    }

    pub fn label(&self) -> Option<&[u8]> {
        self.label.as_deref()
    }
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait EventParameterBackend {
    type Error;
    fn allocate_event(&mut self) -> Result<Option<Box<EventParameter>>, Self::Error>;
    fn convert_event_id(
        &mut self,
        token: &[u8],
        published: &EventParameter,
    ) -> Result<i32, Self::Error>;
    fn convert_event_extra(
        &mut self,
        token: &[u8],
        published: &EventParameter,
    ) -> Result<i32, Self::Error>;
}

/// The actual allocation is transferred into the instruction before any
/// conversion. Callbacks may inspect that published prefix, but cannot replace
/// the borrowed instruction. No failed callback is silently retried.
pub trait SetEventParameterBackend {
    type Error;
    fn allocate_set_event(&mut self) -> Result<Option<Box<SetEventParameter>>, Self::Error>;
    fn convert_set_event_register(
        &mut self,
        token: &[u8],
        published: &SetEventParameter,
    ) -> Result<i32, Self::Error>;
    fn convert_set_event_id(
        &mut self,
        token: &[u8],
        published: &SetEventParameter,
    ) -> Result<i32, Self::Error>;
}

/// Original CMD_EVENT keyword map: six case-insensitive names, anything else
/// is atoi of the first word.
pub fn decode_event_id(token: &[u8]) -> Result<i32, ProgramParserError> {
    if token.eq_ignore_ascii_case(b"use_AB") {
        Ok(EVENT_USE_AB)
    } else if token.eq_ignore_ascii_case(b"death") {
        Ok(EVENT_DEATH)
    } else if token.eq_ignore_ascii_case(b"enemy_to_AI") {
        Ok(EVENT_ENEMY_TO_AI)
    } else if token.eq_ignore_ascii_case(b"enemy_to_Script") {
        Ok(EVENT_ENEMY_TO_SCRIPT)
    } else if token.eq_ignore_ascii_case(b"hit_from_player") {
        Ok(EVENT_HIT_FROM_PLAYER)
    } else if token.eq_ignore_ascii_case(b"detector_hit") {
        Ok(EVENT_DETECTOR_HIT)
    } else {
        ascii_integer_prefix(token)
    }
}

#[derive(Default)]
pub struct AsciiEventParameterBackend;

impl EventParameterBackend for AsciiEventParameterBackend {
    type Error = ProgramParserError;

    fn allocate_event(&mut self) -> Result<Option<Box<EventParameter>>, Self::Error> {
        Ok(Some(Box::new(EventParameter::default())))
    }

    fn convert_event_id(
        &mut self,
        token: &[u8],
        _published: &EventParameter,
    ) -> Result<i32, Self::Error> {
        decode_event_id(token)
    }

    fn convert_event_extra(
        &mut self,
        token: &[u8],
        _published: &EventParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Default)]
pub struct AsciiSetEventParameterBackend;

impl SetEventParameterBackend for AsciiSetEventParameterBackend {
    type Error = ProgramParserError;

    fn allocate_set_event(&mut self) -> Result<Option<Box<SetEventParameter>>, Self::Error> {
        Ok(Some(Box::new(SetEventParameter::default())))
    }

    fn convert_set_event_register(
        &mut self,
        token: &[u8],
        _published: &SetEventParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }

    fn convert_set_event_id(
        &mut self,
        token: &[u8],
        _published: &SetEventParameter,
    ) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EventDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    IdConversion(E),
    ExtraConversion(E),
}

#[derive(Debug, PartialEq, Eq)]
pub enum SetEventDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    RegisterConversion(E),
    IdConversion(E),
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if conversion or a backend callback unwinds.
#[derive(Debug, Default)]
pub struct EventParameterSlot {
    entered: bool,
    parameter: Option<Box<EventParameter>>,
}

impl EventParameterSlot {
    pub fn parameter(&self) -> Option<&EventParameter> {
        self.parameter.as_deref()
    }

    /// Only counts of one or two are accepted; the second word defaults to
    /// zero. The source is split in place before count validation, as in the
    /// original DecodeParams call chain.
    pub fn decode<B: EventParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, EventDecodeError<B::Error>> {
        if self.entered {
            return Err(EventDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(EventDecodeError::Parser)?;
        if !supports_event_opcode(opcode) {
            return Err(EventDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 1 && parameters.count != 2 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_event()
            .map_err(EventDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(EventDecodeError::NullAllocation)?;
        let first = parameters.offsets[0].expect("counted parameter token");
        let first_length = c_length(&source[first..]).map_err(EventDecodeError::Parser)?;
        parameter.id = Some(
            backend
                .convert_event_id(&source[first..first + first_length], parameter)
                .map_err(EventDecodeError::IdConversion)?,
        );
        if parameters.count == 2 {
            let second = parameters.offsets[1].expect("counted parameter token");
            let second_length = c_length(&source[second..]).map_err(EventDecodeError::Parser)?;
            parameter.extra = Some(
                backend
                    .convert_event_extra(&source[second..second + second_length], parameter)
                    .map_err(EventDecodeError::ExtraConversion)?,
            );
        } else {
            parameter.extra = Some(0);
        }
        Ok(true)
    }
}

/// One instruction-owned parameter allocation. Entered is latched before
/// tokenization, even if conversion or a backend callback unwinds.
#[derive(Debug, Default)]
pub struct SetEventParameterSlot {
    entered: bool,
    parameter: Option<Box<SetEventParameter>>,
}

impl SetEventParameterSlot {
    pub fn parameter(&self) -> Option<&SetEventParameter> {
        self.parameter.as_deref()
    }

    /// Only a count of three is accepted, with the label (an unquoted name of
    /// at most 15 bytes) copied raw. The source is split in place before
    /// count validation, as in the original DecodeParams call chain.
    pub fn decode<B: SetEventParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, SetEventDecodeError<B::Error>> {
        if self.entered {
            return Err(SetEventDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let parameters = extrude_params(source).map_err(SetEventDecodeError::Parser)?;
        if !supports_set_event_opcode(opcode) {
            return Err(SetEventDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 3 {
            return Ok(false);
        }
        let label_start = parameters.offsets[2].expect("counted parameter token");
        let label_length = c_length(&source[label_start..]).map_err(SetEventDecodeError::Parser)?;
        if label_length > 15 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_set_event()
            .map_err(SetEventDecodeError::Allocation)?;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(SetEventDecodeError::NullAllocation)?;
        let first = parameters.offsets[0].expect("counted parameter token");
        let first_length = c_length(&source[first..]).map_err(SetEventDecodeError::Parser)?;
        parameter.register = Some(
            backend
                .convert_set_event_register(&source[first..first + first_length], parameter)
                .map_err(SetEventDecodeError::RegisterConversion)?,
        );
        let second = parameters.offsets[1].expect("counted parameter token");
        let second_length = c_length(&source[second..]).map_err(SetEventDecodeError::Parser)?;
        parameter.event = Some(
            backend
                .convert_set_event_id(&source[second..second + second_length], parameter)
                .map_err(SetEventDecodeError::IdConversion)?,
        );
        parameter.label = Some(
            source[label_start..label_start + label_length]
                .to_vec()
                .into_boxed_slice(),
        );
        Ok(true)
    }
}

/// Parameter-layout admission only; this opcode does not become executable by
/// decoding (execution is admitted in dispatch).
pub fn supports_event_opcode(opcode: u32) -> bool {
    opcode == 73
}

/// Parameter-layout admission only; this opcode does not become executable by
/// decoding (execution is admitted in dispatch).
pub fn supports_set_event_opcode(opcode: u32) -> bool {
    opcode == 74
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_accepts_one_or_two_words_with_keyword_or_integer_id() {
        for (source, id, extra) in [
            (&b"use_AB\0"[..], 60000, 0),
            (&b"DEATH\0"[..], 60001, 0),
            (&b"Enemy_To_Script,7\0"[..], 60003, 7),
            (&b"detector_hit, 12\0"[..], 60005, 12),
            (&b"4242\0"[..], 4242, 0),
            (&b"-3, -9\0"[..], -3, -9),
        ] {
            let mut buffer = source.to_vec();
            let mut slot = EventParameterSlot::default();
            assert_eq!(
                slot.decode(73, &mut buffer, &mut AsciiEventParameterBackend),
                Ok(true),
                "{source:?}"
            );
            let parameter = slot.parameter().unwrap();
            assert_eq!((parameter.id(), parameter.extra()), (Some(id), Some(extra)));
        }
        assert!(supports_event_opcode(73));
        for opcode in [0, 72, 74, 75, 397] {
            assert!(!supports_event_opcode(opcode), "opcode {opcode}");
        }
        for source in [&b"\0"[..], &b"1,2,3\0"[..]] {
            let mut buffer = source.to_vec();
            let mut slot = EventParameterSlot::default();
            assert_eq!(
                slot.decode(73, &mut buffer, &mut AsciiEventParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        let mut buffer = b"death\0".to_vec();
        let mut slot = EventParameterSlot::default();
        assert_eq!(
            slot.decode(74, &mut buffer, &mut AsciiEventParameterBackend),
            Err(EventDecodeError::UnsupportedOpcode(74))
        );
    }

    #[test]
    fn setevent_accepts_three_words_with_raw_label_bound() {
        let mut buffer = b"0,60001,retry_label\0".to_vec();
        let mut slot = SetEventParameterSlot::default();
        assert_eq!(
            slot.decode(74, &mut buffer, &mut AsciiSetEventParameterBackend),
            Ok(true)
        );
        let parameter = slot.parameter().unwrap();
        assert_eq!(parameter.register(), Some(0));
        assert_eq!(parameter.event(), Some(60001));
        assert_eq!(parameter.label(), Some(&b"retry_label"[..]));
        // An explicit trailing space after the second comma yields an
        // empty label (native checks length <= 15 only); a bare trailing
        // comma without following bytes is not a third parameter in the
        // extruder and thus fails admission.
        let mut buffer = b"1,2, \0".to_vec();
        let mut slot = SetEventParameterSlot::default();
        assert_eq!(
            slot.decode(74, &mut buffer, &mut AsciiSetEventParameterBackend),
            Ok(true)
        );
        assert_eq!(slot.parameter().unwrap().label(), Some(&b""[..]));
        let mut buffer = b"1,2,\0".to_vec();
        let mut slot = SetEventParameterSlot::default();
        assert_eq!(
            slot.decode(74, &mut buffer, &mut AsciiSetEventParameterBackend),
            Ok(false)
        );
        // Fifteen bytes admit, sixteen refuse.
        let mut buffer = b"1,2,0123456789abcdef\0".to_vec();
        let mut slot = SetEventParameterSlot::default();
        assert_eq!(
            slot.decode(74, &mut buffer, &mut AsciiSetEventParameterBackend),
            Ok(false)
        );
        assert!(slot.parameter().is_none());
        let mut buffer = b"1,2,0123456789abcde\0".to_vec();
        let mut slot = SetEventParameterSlot::default();
        assert_eq!(
            slot.decode(74, &mut buffer, &mut AsciiSetEventParameterBackend),
            Ok(true)
        );
        assert!(supports_set_event_opcode(74));
        for opcode in [0, 73, 75, 342, 397] {
            assert!(!supports_set_event_opcode(opcode), "opcode {opcode}");
        }
        for source in [&b"\0"[..], &b"1,2\0"[..], &b"1,2,3,4\0"[..]] {
            let mut buffer = source.to_vec();
            let mut slot = SetEventParameterSlot::default();
            assert_eq!(
                slot.decode(74, &mut buffer, &mut AsciiSetEventParameterBackend),
                Ok(false),
                "{source:?}"
            );
            assert!(slot.parameter().is_none());
        }
        let mut buffer = b"1,2,x\0".to_vec();
        let mut slot = SetEventParameterSlot::default();
        assert_eq!(
            slot.decode(73, &mut buffer, &mut AsciiSetEventParameterBackend),
            Err(SetEventDecodeError::UnsupportedOpcode(73))
        );
    }

    #[test]
    fn conversion_failures_preserve_reached_prefix() {
        let mut slot = EventParameterSlot::default();
        let mut backend = AsciiEventParameterBackend;
        // Non-ASCII byte triggers UnsupportedCrtByte after allocation.
        let mut source = b"\xFF\0".to_vec();
        assert!(slot.decode(73, &mut source, &mut backend).is_err());
        assert_eq!(
            slot.parameter().unwrap(),
            &EventParameter {
                id: None,
                extra: None
            }
        );
        assert_eq!(
            slot.decode(73, &mut source, &mut backend),
            Err(EventDecodeError::AlreadyEntered)
        );
    }
}
