//! Owned C_entity::DecodeParams word-count admission (GAME 0x51b470) for the
//! 0x1000-0x103f command range.
//!
//! Reference: Game.exe SHA-256
//! 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! entity DecodeParams with its 64 `case 0x1000`-`case 0x103f` arms and the
//! shared success/failure exits. Word splitting reuses the existing
//! comma-split tokenizer (16-word cap included), matching the original
//! `local_4c[16]` token array.
//!
//! Only admission is reconstructed, not values or execution: the original
//! converts words with total atoi/atof helpers (no failure mode) and maps
//! keyword words to flag values consumed solely by the unimplemented entity
//! Process. Rejection is purely word-count driven in every arm — keyword
//! mismatches only select default flag values — so the ported predicate admits
//! exactly the original's admitted shapes and retains the raw words for a
//! future execution owner. Two arms (0x1021 STEPAWAY, 0x1024 USE_DETECTOR)
//! read word zero unconditionally; an absent word is fail-closed here because
//! the original out-of-bounds read is not reproducible.
//!
//! This is source-backed parameter handling, not original-runtime equivalence
//! or execution of any entity command: dispatch refuses 0x1000+ opcodes.
#![forbid(unsafe_code)]

use crate::program_parser::{extrude_params, ProgramParserError};

/// First entity command ID (MOVE pair).
pub const ENTITY_OPCODE_MIN: u32 = 0x1000;
/// Last entity command ID (BLASTFIRE pair).
pub const ENTITY_OPCODE_MAX: u32 = 0x103f;

pub fn supports_entity_opcode(opcode: u32) -> bool {
    (ENTITY_OPCODE_MIN..=ENTITY_OPCODE_MAX).contains(&opcode)
}

/// Original word-count admission per command ID. `count` is the comma-split
/// word count (0 for empty parameters, capped at 16 by the shared tokenizer).
/// Every arm below mirrors its native `case` body's count checks: arms without
/// checks admit any count, arms reading word N unconditionally require at
/// least N + 1 words, and bounded arms admit exactly their listed counts.
pub(crate) fn admitted_word_count(opcode: u32, count: usize) -> bool {
    match opcode {
        0x1002 | 0x1004 | 0x1007 | 0x100a | 0x100f | 0x1017 | 0x1018 | 0x101b | 0x101d | 0x1020
        | 0x1023 | 0x1028 | 0x102a | 0x102c | 0x102d | 0x102f | 0x1030 | 0x1038 | 0x103c
        | 0x103d | 0x103e | 0x103f => true,
        0x101a | 0x1033 => count <= 1,
        0x1003 | 0x1005 | 0x1010 | 0x1015 | 0x101f | 0x1027 | 0x1029 | 0x1031 | 0x1034 | 0x1039
        | 0x103a | 0x103b => count == 1,
        0x1032 | 0x1037 => count == 2,
        0x1009 | 0x1019 => count == 3,
        0x1000 | 0x1011 | 0x1013 | 0x1014 | 0x101e | 0x102e => count == 1 || count == 2,
        0x100b | 0x101c | 0x1016 | 0x102b | 0x1036 => count == 2 || count == 3,
        0x1001 | 0x1006 | 0x1025 | 0x1026 => count == 1 || count == 2 || count == 3,
        0x1008 => count == 0 || count == 2,
        0x100c | 0x1012 | 0x1022 => count >= 2,
        0x1021 | 0x1024 => count >= 1,
        0x100e => count == 2 || count == 3 || count == 4,
        0x100d => count == 3 || count == 4 || count == 6 || count == 7,
        0x1035 => count == 5,
        _ => false,
    }
}

/// One validated entity command payload: the NUL-separated parameter line plus
/// per-word spans. Values are intentionally uninterpreted; conversions and
/// keyword flags belong to the unimplemented Process owner.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EntityParameter {
    text: Vec<u8>,
    spans: [(usize, usize); 16],
    count: usize,
}

impl EntityParameter {
    pub fn count(&self) -> usize {
        self.count
    }

    /// Raw word bytes without surrounding margin. Words keep their original
    /// quotes and case; trimming is limited to the shared leading-space pass.
    pub fn word(&self, index: usize) -> Option<&[u8]> {
        if index >= self.count {
            return None;
        }
        let (start, length) = self.spans[index];
        Some(&self.text[start..start + length])
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum EntityParameterError {
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    RejectedWordCount { opcode: u32, count: usize },
}

/// One fresh instruction slot. Double decode is rejected; a returned error
/// leaves no partial payload behind. The payload is boxed like the other
/// parameter families so the shared instruction enum stays compact.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EntityParameterSlot {
    entered: bool,
    parameter: Option<Box<EntityParameter>>,
}

impl EntityParameterSlot {
    pub fn parameter(&self) -> Option<&EntityParameter> {
        self.parameter.as_deref()
    }

    pub fn decode(
        &mut self,
        opcode: u32,
        parameters: &mut [u8],
    ) -> Result<(), EntityParameterError> {
        if self.entered {
            return Err(EntityParameterError::UnsupportedOpcode(opcode));
        }
        self.entered = true;
        if !supports_entity_opcode(opcode) {
            return Err(EntityParameterError::UnsupportedOpcode(opcode));
        }
        // The compile path always passes a NUL-terminated remainder (possibly
        // just the terminator); a truly empty direct input is normalized to
        // the same terminator so it admits zero words.
        let mut text = parameters.to_vec();
        if text.is_empty() {
            text.push(0);
        }
        let extruded = extrude_params(&mut text).map_err(EntityParameterError::Parser)?;
        let count = extruded.count;
        if !admitted_word_count(opcode, count) {
            return Err(EntityParameterError::RejectedWordCount { opcode, count });
        }
        let mut spans = [(0usize, 0usize); 16];
        for (index, offset) in extruded.offsets.iter().enumerate().take(count) {
            let start = offset.expect("counted word has a start");
            let mut end = start;
            while end < text.len() && text[end] != 0 {
                end += 1;
            }
            spans[index] = (start, end - start);
        }
        self.parameter = Some(Box::new(EntityParameter { text, spans, count }));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(
        opcode: u32,
        parameters: &mut [u8],
    ) -> Result<EntityParameterSlot, EntityParameterError> {
        let mut slot = EntityParameterSlot::default();
        slot.decode(opcode, parameters)?;
        Ok(slot)
    }

    #[test]
    fn opcode_range_covers_exactly_the_entity_command_ids() {
        assert!(!supports_entity_opcode(0x0fff));
        assert!(supports_entity_opcode(0x1000));
        assert!(supports_entity_opcode(0x103f));
        assert!(!supports_entity_opcode(0x1040));
        assert!(!supports_entity_opcode(0));
        assert!(!supports_entity_opcode(u32::MAX));
    }

    #[test]
    fn every_command_id_admits_its_evidenced_counts() {
        let accepted: [(u32, &[usize]); 64] = [
            (0x1000, &[1, 2]),
            (0x1001, &[1, 2, 3]),
            (0x1002, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1003, &[1]),
            (0x1004, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1005, &[1]),
            (0x1006, &[1, 2, 3]),
            (0x1007, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1008, &[0, 2]),
            (0x1009, &[3]),
            (0x100a, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x100b, &[2, 3]),
            (0x100c, &[2, 3, 4, 5, 6, 7, 8]),
            (0x100d, &[3, 4, 6, 7]),
            (0x100e, &[2, 3, 4]),
            (0x100f, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1010, &[1]),
            (0x1011, &[1, 2]),
            (0x1012, &[2, 3, 4, 5, 6, 7, 8]),
            (0x1013, &[1, 2]),
            (0x1014, &[1, 2]),
            (0x1015, &[1]),
            (0x1016, &[2, 3]),
            (0x1017, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1018, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1019, &[3]),
            (0x101a, &[0, 1]),
            (0x101b, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x101c, &[2, 3]),
            (0x101d, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x101e, &[1, 2]),
            (0x101f, &[1]),
            (0x1020, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1021, &[1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1022, &[2, 3, 4, 5, 6, 7, 8]),
            (0x1023, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1024, &[1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1025, &[1, 2, 3]),
            (0x1026, &[1, 2, 3]),
            (0x1027, &[1]),
            (0x1028, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1029, &[1]),
            (0x102a, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x102b, &[2, 3]),
            (0x102c, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x102d, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x102e, &[1, 2]),
            (0x102f, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1030, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1031, &[1]),
            (0x1032, &[2]),
            (0x1033, &[0, 1]),
            (0x1034, &[1]),
            (0x1035, &[5]),
            (0x1036, &[2, 3]),
            (0x1037, &[2]),
            (0x1038, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x1039, &[1]),
            (0x103a, &[1]),
            (0x103b, &[1]),
            (0x103c, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x103d, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x103e, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
            (0x103f, &[0, 1, 2, 3, 4, 5, 6, 7, 8]),
        ];
        assert_eq!(accepted.len(), 64);
        for (opcode, counts) in accepted {
            assert!(supports_entity_opcode(opcode), "{opcode:#x}");
            for count in 0..=8 {
                assert_eq!(
                    admitted_word_count(opcode, count),
                    counts.contains(&count),
                    "opcode {opcode:#x} count {count}"
                );
            }
        }
    }

    #[test]
    fn unsupported_opcodes_reject_without_reading_parameters() {
        let mut params = b"1, 2\0".to_vec();
        assert_eq!(
            decode(0x0fff, &mut params),
            Err(EntityParameterError::UnsupportedOpcode(0x0fff))
        );
        assert_eq!(
            decode(0x1040, &mut params),
            Err(EntityParameterError::UnsupportedOpcode(0x1040))
        );
        assert_eq!(params, b"1, 2\0");
    }

    #[test]
    fn rejected_counts_report_opcode_and_count_and_leave_no_payload() {
        let mut slot = EntityParameterSlot::default();
        let mut params = b"1, 2, 3, 4\0".to_vec();
        assert_eq!(
            slot.decode(0x1003, &mut params),
            Err(EntityParameterError::RejectedWordCount {
                opcode: 0x1003,
                count: 4
            })
        );
        assert!(slot.parameter().is_none());
        // The original's native compile error also drops the pending
        // instruction; a failed slot must not decode again.
        assert_eq!(
            slot.decode(0x1003, &mut b"1\0".to_vec()),
            Err(EntityParameterError::UnsupportedOpcode(0x1003))
        );
    }

    #[test]
    fn admitted_words_are_retained_raw_with_quotes_and_case() {
        let slot = decode(0x1001, &mut b"\"x barman umyvani.i3d\", 0\0".to_vec()).unwrap();
        let parameter = slot.parameter().unwrap();
        assert_eq!(parameter.count(), 2);
        assert_eq!(
            parameter.word(0),
            Some(b"\"x barman umyvani.i3d\"".as_slice())
        );
        assert_eq!(parameter.word(1), Some(b"0".as_slice()));
        assert_eq!(parameter.word(2), None);
        let slot = decode(0x1026, &mut b"panic_freeze\0".to_vec()).unwrap();
        assert_eq!(
            slot.parameter().unwrap().word(0),
            Some(b"panic_freeze".as_slice())
        );
    }

    #[test]
    fn empty_parameters_decode_to_zero_words() {
        let slot = decode(0x1004, &mut b"\0".to_vec()).unwrap();
        assert_eq!(slot.parameter().unwrap().count(), 0);
        assert!(decode(0x1003, &mut b"\0".to_vec()).is_err());
    }

    #[test]
    fn census_shapes_decode_as_the_original_admits_them() {
        // Exact argument shapes observed across shipped missions.
        let cases: [(u32, &[u8]); 24] = [
            (0x1000, b"104\0"),
            (0x1000, b"12,run\0"),
            (0x1001, b"\"GestSklStativ.I3D\"\0"),
            (0x1003, b"5\0"),
            (0x1004, b"\0"),
            (0x1005, b"10\0"),
            (0x1006, b"00950112\0"),
            (0x1006, b"1, 00950224, 4\0"),
            (0x1008, b"\0"),
            (0x1008, b"0, 1\0"),
            (0x100b, b"0, 1\0"),
            (0x100c, b"0,1,bokovka\0"),
            (0x100c, b"4, 5\0"),
            (0x100c, b"0, 1, bokovka, 10, 0.5\0"),
            (0x100d, b"0, 1, 0, -1, 40, 300, 1\0"),
            (0x100e, b"1, 0, vzad\0"),
            (0x1012, b"1, 2.0, CROUCH, 3\0"),
            (0x101a, b"0\0"),
            (0x101c, b"1, 2, 3\0"),
            (0x101e, b"2\0"),
            (0x101f, b"3\0"),
            (0x102b, b"0, 1\0"),
            (0x102e, b"5\0"),
            (0x102e, b"5, 50.0\0"),
        ];
        for (opcode, params) in cases {
            let mut buffer = params.to_vec();
            decode(opcode, &mut buffer)
                .unwrap_or_else(|error| panic!("opcode {opcode:#x} params {params:?}: {error:?}"));
        }
        // The one shipped line the original also rejects: NAPRDELIVAUTE takes
        // exactly one word, so an empty argument list is a native compile
        // error there too.
        assert_eq!(
            decode(0x103a, &mut b"\0".to_vec()),
            Err(EntityParameterError::RejectedWordCount {
                opcode: 0x103a,
                count: 0
            })
        );
    }
}
