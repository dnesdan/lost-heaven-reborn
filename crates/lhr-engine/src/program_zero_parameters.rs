//! The no-payload arm of original C_program::DecodeParams (RVA0x61c70).
//! Game SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! dispatch arm VA0x4636b4. Common tokenization still runs; this arm does not
//! check token count, allocate, initialize or release a parameter payload.
//! Parameter admission does not implement any of the commands' execution.
#![forbid(unsafe_code)]
use crate::program_parser::{extrude_params, ProgramParserError};

#[derive(Debug, PartialEq, Eq)]
pub enum ZeroParameterError {
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
}
/// Decode a fresh instruction whose payload is genuinely absent. Even ignored
/// arguments retain the original common tokenizer's reached in-place writes.
pub fn decode_zero_parameters(opcode: u32, source: &mut [u8]) -> Result<(), ZeroParameterError> {
    extrude_params(source).map_err(ZeroParameterError::Parser)?;
    if supports_zero_parameter_opcode(opcode) {
        Ok(())
    } else {
        Err(ZeroParameterError::UnsupportedOpcode(opcode))
    }
}
/// Exact39 registry selectors for the original shared no-payload branch.
pub fn supports_zero_parameter_opcode(opcode: u32) -> bool {
    matches!(
        opcode,
        3 | 6
            | 33
            | 38
            | 42
            | 46
            | 47
            | 51
            | 69
            | 71
            | 79
            | 92
            | 93
            | 94
            | 98
            | 120
            | 154
            | 166
            | 180
            | 201
            | 202
            | 207
            | 225
            | 226
            | 301
            | 307
            | 318
            | 319
            | 328
            | 329
            | 346
            | 347
            | 351
            | 357
            | 378
            | 389
            | 393
            | 396
            | 397
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_original_selectors_accept_ignored_arguments_but_keep_token_mutations() {
        assert_eq!(
            (0..398)
                .filter(|&id| supports_zero_parameter_opcode(id))
                .count(),
            39
        );
        for opcode in (0..398).filter(|&id| supports_zero_parameter_opcode(id)) {
            let mut source = b"  one , two  \0tail".to_vec();
            assert_eq!(decode_zero_parameters(opcode, &mut source), Ok(()));
            assert_eq!(&source, b"  one\0\0 two\0\0\0tail");
        }
    }
    #[test]
    fn empty_non_ascii_and_over_capacity_argument_lists_do_not_acquire_payload_validation() {
        for source in [
            vec![0],
            vec![0xff, b',', b'a', 0],
            b",,,,,,,,,,,,,,,,,,tail\0".to_vec(),
        ] {
            let mut bytes = source;
            assert_eq!(decode_zero_parameters(6, &mut bytes), Ok(()));
        }
    }
    #[test]
    fn unknown_branch_stops_after_tokenization_and_missing_terminator_before_mutation() {
        let mut source = b"a,b\0".to_vec();
        assert_eq!(
            decode_zero_parameters(12, &mut source),
            Err(ZeroParameterError::UnsupportedOpcode(12))
        );
        assert_eq!(source, b"a\0b\0");
        let mut source = b"a,b".to_vec();
        let before = source.clone();
        assert_eq!(
            decode_zero_parameters(6, &mut source),
            Err(ZeroParameterError::Parser(
                ProgramParserError::MissingTerminator
            ))
        );
        assert_eq!(source, before);
    }
}
