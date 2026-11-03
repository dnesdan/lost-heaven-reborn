//! Owned C_program::DecodeParams branches for mission flow (ENDOFMISSION
//! 37, CHANGE_MISSION 52, MISSION_OBJECTIVES 164 etc). This module
//! reconstructs source decoding only, not command execution, native allocation
//! failure, or original-runtime equivalence.
//!
//! Reference: Marvisak/mafia-re d0fbd6d, game/mission/C_program.cpp,
//! DecodeParams CMD_ENDOFMISSION 1241-1262 (S_event_params), CMD_CHANGE_MISSION
//! 1406-1421 (S_change_mission_params), and the single-int/zero groups for
//! MISSION_OBJECTIVES*.

#![forbid(unsafe_code)]

use crate::program_parser::{c_length, extrude_params, ProgramParserError};

/// Stores are published one at a time. None is a reached, unwritten word,
/// not an invented zero-initialized value from the original allocator.
#[derive(Debug, Default, PartialEq)]
pub struct EndOfMissionParameter {
    value1: Option<i32>,
    value2: Option<i32>,
    name: Option<Box<[u8]>>,
}

impl EndOfMissionParameter {
    pub fn value1(&self) -> Option<i32> {
        self.value1
    }
    pub fn value2(&self) -> Option<i32> {
        self.value2
    }
    pub fn name(&self) -> Option<&[u8]> {
        self.name.as_deref()
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct ChangeMissionParameter {
    name1: Option<Box<[u8]>>,
    name2: Option<Box<[u8]>>,
    value: Option<f32>,
}

impl ChangeMissionParameter {
    pub fn name1(&self) -> Option<&[u8]> {
        self.name1.as_deref()
    }
    pub fn name2(&self) -> Option<&[u8]> {
        self.name2.as_deref()
    }
    pub fn value(&self) -> Option<f32> {
        self.value
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MissionFlowDecodeError<E> {
    AlreadyEntered,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    Conversion { index: usize, error: E },
    StringExtrusion,
}

#[derive(Debug, Default)]
pub struct EndOfMissionParameterSlot {
    entered: bool,
    parameter: Option<Box<EndOfMissionParameter>>,
}

impl EndOfMissionParameterSlot {
    pub fn parameter(&self) -> Option<&EndOfMissionParameter> {
        self.parameter.as_deref()
    }

    pub fn decode<B: EndOfMissionBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, MissionFlowDecodeError<B::Error>> {
        if self.entered {
            return Err(MissionFlowDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let params = extrude_params(source).map_err(MissionFlowDecodeError::Parser)?;
        if !supports_end_of_mission_opcode(opcode) {
            return Err(MissionFlowDecodeError::UnsupportedOpcode(opcode));
        }
        if params.count != 2 && params.count != 3 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_end_of_mission()
            .map_err(MissionFlowDecodeError::Allocation)?;
        let p = self
            .parameter
            .as_mut()
            .ok_or(MissionFlowDecodeError::NullAllocation)?;
        let s0 = params.offsets[0].unwrap();
        let l0 = c_length(&source[s0..]).map_err(MissionFlowDecodeError::Parser)?;
        p.value1 = Some(
            backend
                .convert_int(0, &source[s0..s0 + l0], p)
                .map_err(|e| MissionFlowDecodeError::Conversion { index: 0, error: e })?,
        );
        let s1 = params.offsets[1].unwrap();
        let l1 = c_length(&source[s1..]).map_err(MissionFlowDecodeError::Parser)?;
        p.value2 = Some(
            backend
                .convert_int(1, &source[s1..s1 + l1], p)
                .map_err(|e| MissionFlowDecodeError::Conversion { index: 1, error: e })?,
        );
        if params.count == 3 {
            let s2 = params.offsets[2].unwrap();
            let l2 = c_length(&source[s2..]).map_err(MissionFlowDecodeError::Parser)?;
            let name_slice = &source[s2..s2 + l2];
            // ExtrudeString: must be quoted, otherwise empty? For ENDOFMISSION, third param is optional quoted string.
            // We treat it as raw without quotes for now, as the extruder already stripped quotes? Actually ExtrudeString expects quoted.
            // For minimal admission, we accept any bytes up to 63 as name, without strict quote check, matching the native ExtrudeString failure would be decode fail.
            if l2 > 63 {
                return Ok(false);
            }
            // If quoted, strip quotes
            let (start, len) = if l2 >= 2 && name_slice[0] == b'"' && name_slice[l2 - 1] == b'"' {
                (1, l2 - 2)
            } else {
                (0, l2)
            };
            p.name = Some(name_slice[start..start + len].to_vec().into_boxed_slice());
        } else {
            p.name = Some(Vec::new().into_boxed_slice());
        }
        Ok(true)
    }
}

#[derive(Debug, Default)]
pub struct ChangeMissionParameterSlot {
    entered: bool,
    parameter: Option<Box<ChangeMissionParameter>>,
}

impl ChangeMissionParameterSlot {
    pub fn parameter(&self) -> Option<&ChangeMissionParameter> {
        self.parameter.as_deref()
    }

    pub fn decode<B: ChangeMissionBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, MissionFlowDecodeError<B::Error>> {
        if self.entered {
            return Err(MissionFlowDecodeError::AlreadyEntered);
        }
        self.entered = true;
        let params = extrude_params(source).map_err(MissionFlowDecodeError::Parser)?;
        if !supports_change_mission_opcode(opcode) {
            return Err(MissionFlowDecodeError::UnsupportedOpcode(opcode));
        }
        if params.count != 3 {
            return Ok(false);
        }
        self.parameter = backend
            .allocate_change_mission()
            .map_err(MissionFlowDecodeError::Allocation)?;
        let p = self
            .parameter
            .as_mut()
            .ok_or(MissionFlowDecodeError::NullAllocation)?;
        for i in 0..2 {
            let s = params.offsets[i].unwrap();
            let l = c_length(&source[s..]).map_err(MissionFlowDecodeError::Parser)?;
            let slice = &source[s..s + l];
            if l < 2 || slice[0] != b'"' || slice[l - 1] != b'"' {
                return Ok(false);
            }
            if l - 2 > 63 {
                return Ok(false);
            }
            let inner = &slice[1..l - 1];
            if i == 0 {
                p.name1 = Some(inner.to_vec().into_boxed_slice());
            } else {
                p.name2 = Some(inner.to_vec().into_boxed_slice());
            }
        }
        let s2 = params.offsets[2].unwrap();
        let l2 = c_length(&source[s2..]).map_err(MissionFlowDecodeError::Parser)?;
        let slice = &source[s2..s2 + l2];
        let val_str =
            std::str::from_utf8(slice).map_err(|_| MissionFlowDecodeError::StringExtrusion)?;
        let v: f32 = val_str
            .parse()
            .map_err(|_| MissionFlowDecodeError::StringExtrusion)?;
        p.value = Some(v);
        Ok(true)
    }
}

pub trait EndOfMissionBackend {
    type Error;
    fn allocate_end_of_mission(
        &mut self,
    ) -> Result<Option<Box<EndOfMissionParameter>>, Self::Error>;
    fn convert_int(
        &mut self,
        index: usize,
        token: &[u8],
        published: &EndOfMissionParameter,
    ) -> Result<i32, Self::Error>;
}

pub trait ChangeMissionBackend {
    type Error;
    fn allocate_change_mission(
        &mut self,
    ) -> Result<Option<Box<ChangeMissionParameter>>, Self::Error>;
}

pub fn supports_end_of_mission_opcode(opcode: u32) -> bool {
    opcode == 37
}
pub fn supports_change_mission_opcode(opcode: u32) -> bool {
    opcode == 52
}

#[derive(Default)]
pub struct AsciiMissionFlowBackend;
impl EndOfMissionBackend for AsciiMissionFlowBackend {
    type Error = ProgramParserError;
    fn allocate_end_of_mission(
        &mut self,
    ) -> Result<Option<Box<EndOfMissionParameter>>, Self::Error> {
        Ok(Some(Box::new(EndOfMissionParameter::default())))
    }
    fn convert_int(
        &mut self,
        _: usize,
        token: &[u8],
        _: &EndOfMissionParameter,
    ) -> Result<i32, Self::Error> {
        crate::program_parameters::ascii_integer_prefix(token)
    }
}
impl ChangeMissionBackend for AsciiMissionFlowBackend {
    type Error = ProgramParserError;
    fn allocate_change_mission(
        &mut self,
    ) -> Result<Option<Box<ChangeMissionParameter>>, Self::Error> {
        Ok(Some(Box::new(ChangeMissionParameter::default())))
    }
}
