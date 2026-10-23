//! Bounded byte projections of three original C_program parsing helpers.
//!
//! These functions borrow the caller's writable, NUL-terminated storage. They
//! return offsets into that storage, never native pointers or new string owners.
//! Only the first C string is inspected; bytes after its NUL remain untouched.
//! Rust references exclude null pointers. Missing terminators and unsupported
//! numeric inputs are host admission errors, distinct from native parse failure.
//!
//! PC canonical entry RVAs: ExtrudeParams 0x61830, DecodeOperandInt 0x619b0,
//! ExtrudeString 0x61a60. Actual PS2 C_program symbols support the original names;
//! their correspondence to these PC bodies is structural. This module does not
//! implement DecodeParams, Compile, floating conversion, or instruction execution.

use std::ops::Range;

/// Original ExtrudeParams pointer-array capacity, including slot zero.
pub const PARAMETER_CAPACITY: usize = 16;

/// Host admission for C-string length, excluding NUL. ExtrudeParams uses signed
/// 32-bit scan comparisons; larger strings are outside this projection.
pub const MAX_PARSER_STRING_LENGTH: usize = i32::MAX as usize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramParserError {
    MissingTerminator,
    InputTooLong,
    /// Native instruction recognition reads the byte before an empty input.
    EmptyInstruction,
    /// Native trailing-space trimming reads before an empty operand's start.
    EmptyOperand,
    /// Non-ASCII CRT classification/case folding is outside this projection.
    UnsupportedCrtByte {
        offset: usize,
        byte: u8,
    },
}

impl std::fmt::Display for ProgramParserError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingTerminator => formatter.write_str("missing NUL terminator"),
            Self::InputTooLong => formatter.write_str("C string exceeds parser length admission"),
            Self::EmptyInstruction => formatter.write_str("empty instruction input"),
            Self::EmptyOperand => formatter.write_str("empty or all-space numeric operand"),
            Self::UnsupportedCrtByte { offset, byte } => {
                write!(formatter, "unsupported CRT byte {byte:#04x} at {offset}")
            }
        }
    }
}

impl std::error::Error for ProgramParserError {}

/// Observed pointer-array writes, represented relative to the input slice.
///
/// Slot zero is initialized even when count is zero. A trailing comma can also
/// initialize an uncounted slot. Other slots are None because the original does
/// not write them. Only `offsets[..count]` describe counted parameters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtrudedParams {
    pub count: usize,
    pub offsets: [Option<usize>; PARAMETER_CAPACITY],
}

/// Original ExtrudeParams: split commas in place, then trim ASCII spaces.
///
/// Quotes do not protect commas. The sixteenth comma becomes NUL and stops the
/// split scan; later bytes are not scanned. Interior empty arguments count,
/// while a final empty argument does not. Leading spaces advance each counted
/// offset; trailing spaces become NUL. Tabs are preserved. Empty input is valid.
/// MissingTerminator leaves the complete input unchanged.
pub fn extrude_params(source: &mut [u8]) -> Result<ExtrudedParams, ProgramParserError> {
    let length = c_length(source)?;
    let mut result = ExtrudedParams {
        count: 0,
        offsets: [None; PARAMETER_CAPACITY],
    };
    result.offsets[0] = Some(0);
    for (position, byte) in source[..length].iter_mut().enumerate() {
        if *byte == b',' {
            *byte = 0;
            result.count += 1;
            if result.count == PARAMETER_CAPACITY {
                break;
            }
            result.offsets[result.count] = Some(position + 1);
        }
    }
    if result.count < PARAMETER_CAPACITY {
        let start = result.offsets[result.count].expect("current parameter slot initialized");
        if source[start] != 0 {
            result.count += 1;
        }
    }
    for offset in &mut result.offsets[..result.count] {
        let mut start = offset.expect("counted parameter slot initialized");
        while source[start] == b' ' {
            start += 1;
        }
        *offset = Some(start);
        // Every counted parameter remains terminated by a comma write or the
        // original NUL. Leading-space trimming stops at that terminator.
        let mut end = start;
        while source[end] != 0 {
            end += 1;
        }
        while end > start && source[end - 1] == b' ' {
            end -= 1;
            source[end] = 0;
        }
    }
    Ok(result)
}

/// Original ExtrudeString: require a quote after leading ASCII spaces, then
/// replace the first following quote with NUL and return the inner byte range.
///
/// There is no escape syntax or trailing-text check. Missing opening/closing
/// quotes return None without mutation; an empty quoted string returns an empty
/// range. Byte values and tabs inside the quotes are preserved.
pub fn extrude_string(source: &mut [u8]) -> Result<Option<Range<usize>>, ProgramParserError> {
    let length = c_length(source)?;
    let start = source[..length]
        .iter()
        .position(|&byte| byte != b' ')
        .unwrap_or(length);
    if source[start] != b'"' {
        return Ok(None);
    }
    let inside = start + 1;
    let Some(relative_end) = source[inside..length].iter().position(|&byte| byte == b'"') else {
        return Ok(None);
    };
    let end = inside + relative_end;
    source[end] = 0;
    Ok(Some(inside..end))
}

/// Explicit caller-selected admission for the original CRT conversion path.
/// This does not inspect or change the process locale.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParserLocale {
    /// Admit ASCII input under C-locale whitespace, digits and case folding.
    AsciiC,
}

/// Separate caller-owned outputs, preserving fields the original does not write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntegerOperand {
    pub register_index: i32,
    pub literal: i32,
}

/// Original DecodeOperandInt under an explicit ASCII/C-locale admission.
///
/// Trim only ASCII spaces in place, then compare `flt[` case-insensitively. A
/// register operand overwrites the first `]` with NUL and writes register_index,
/// leaving literal unchanged; suffix text after `]` is ignored. A missing `]`
/// returns false after trimming, without writing either output. A literal writes
/// register_index = -1 and its permissively converted integer value.
///
/// Conversion skips C-locale whitespace, consumes one optional sign and decimal
/// digit prefix, and wraps at 32 bits, matching the original x86 multiply/add and
/// negation. No digits means zero; trailing nondigits do not fail conversion.
/// Missing NUL, empty/all-space operands and non-ASCII bytes return admission
/// errors before any source or output mutation. No register bounds are imposed.
pub fn decode_operand_int(
    source: &mut [u8],
    output: &mut IntegerOperand,
    locale: ParserLocale,
) -> Result<bool, ProgramParserError> {
    let ParserLocale::AsciiC = locale;
    let length = c_length(source)?;
    if let Some((offset, &byte)) = source[..length]
        .iter()
        .enumerate()
        .find(|(_, byte)| !byte.is_ascii())
    {
        return Err(ProgramParserError::UnsupportedCrtByte { offset, byte });
    }
    let start = source[..length]
        .iter()
        .position(|&byte| byte != b' ')
        .ok_or(ProgramParserError::EmptyOperand)?;
    let mut end = length;
    while source[end - 1] == b' ' {
        end -= 1;
        source[end] = 0;
    }
    let token = &source[start..end];
    if token.len() < 4 || !token[..4].eq_ignore_ascii_case(b"flt[") {
        output.register_index = -1;
        output.literal = integer_prefix(token);
        return Ok(true);
    }
    let inside = start + 4;
    let Some(relative_end) = source[inside..end].iter().position(|&byte| byte == b']') else {
        return Ok(false);
    };
    let close = inside + relative_end;
    source[close] = 0;
    output.register_index = integer_prefix(&source[inside..close]);
    Ok(true)
}

pub(crate) fn c_length(source: &[u8]) -> Result<usize, ProgramParserError> {
    let length = source
        .iter()
        .position(|&byte| byte == 0)
        .ok_or(ProgramParserError::MissingTerminator)?;
    if length > MAX_PARSER_STRING_LENGTH {
        return Err(ProgramParserError::InputTooLong);
    }
    Ok(length)
}

/// Private CRT 0x210b6a projection; its original function name is not established.
fn integer_prefix(bytes: &[u8]) -> i32 {
    let mut position = 0;
    while position < bytes.len() && matches!(bytes[position], b' ' | b'\t'..=b'\r') {
        position += 1;
    }
    let negative = bytes.get(position) == Some(&b'-');
    if negative || bytes.get(position) == Some(&b'+') {
        position += 1;
    }
    let mut value = 0_u32;
    while let Some(&byte @ b'0'..=b'9') = bytes.get(position) {
        value = value.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'));
        position += 1;
    }
    if negative {
        value = value.wrapping_neg();
    }
    value as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameters_preserve_empty_slots_and_exact_space_writes() {
        let mut source = b"  a  ,, \t b \t ,  ,\0suffix".to_vec();
        let result = extrude_params(&mut source).unwrap();
        assert_eq!(result.count, 4);
        assert_eq!(
            result.offsets[..5],
            [Some(2), Some(6), Some(8), Some(17), Some(18)]
        );
        assert_eq!(source, b"  a\0\0\0\0 \t b \t\0\0  \0\0suffix");
    }

    #[test]
    fn parameter_capacity_stops_on_sixteenth_comma() {
        let mut source = b"0,1,2,3,4,5,6,7,8,9,a,b,c,d,e,f,  untouched,tail\0".to_vec();
        let result = extrude_params(&mut source).unwrap();
        assert_eq!(result.count, PARAMETER_CAPACITY);
        assert_eq!(result.offsets, std::array::from_fn(|index| Some(index * 2)));
        assert_eq!(&source[32..], b"  untouched,tail\0");
        assert_eq!(source[31], 0);
    }

    #[test]
    fn parameter_final_empty_differs_from_final_spaces() {
        for (input, count, offsets) in [
            (&b"\0"[..], 0, [Some(0), None]),
            (&b",\0"[..], 1, [Some(0), Some(1)]),
            (&b", \0"[..], 2, [Some(0), Some(2)]),
        ] {
            let mut source = input.to_vec();
            let result = extrude_params(&mut source).unwrap();
            assert_eq!(result.count, count);
            assert_eq!(result.offsets[..2], offsets);
        }
    }

    #[test]
    fn quoting_has_no_escape_or_comma_protection() {
        let mut source = b" \"a\\\"tail\"suffix\0after".to_vec();
        assert_eq!(extrude_string(&mut source).unwrap(), Some(2..4));
        assert_eq!(source, b" \"a\\\0tail\"suffix\0after");
        let mut comma = b"\"a,b\"\0".to_vec();
        let params = extrude_params(&mut comma).unwrap();
        assert_eq!(params.count, 2);
        assert_eq!(extrude_string(&mut comma).unwrap(), None);
        let mut empty = b"\"\"\0".to_vec();
        assert_eq!(extrude_string(&mut empty).unwrap(), Some(1..1));
    }

    #[test]
    fn missing_quotes_or_tab_prefix_do_not_mutate() {
        for original in [&b"\t\"a\"\0"[..], b"  \"a\0", b"   \0", b"\0"] {
            let mut source = original.to_vec();
            assert_eq!(extrude_string(&mut source).unwrap(), None);
            assert_eq!(source, original);
        }
    }

    #[test]
    fn integer_literals_use_permissive_wrapping_conversion() {
        for (input, expected) in [
            (&b"  \t\r\n\x0b\x0c-12suffix  \0"[..], -12),
            (b"2147483648\0", i32::MIN),
            (b"4294967296\0", 0),
            (b"-2147483649\0", i32::MAX),
            (b"-4294967295\0", 1),
            (b"+00042.9\0", 42),
            (b"0x10\0", 0),
            (b"+-5\0", 0),
            (b"word\0", 0),
            (b"\t\0", 0),
        ] {
            let mut source = input.to_vec();
            let mut output = IntegerOperand {
                register_index: 91,
                literal: 92,
            };
            assert_eq!(
                decode_operand_int(&mut source, &mut output, ParserLocale::AsciiC),
                Ok(true)
            );
            assert_eq!(
                output,
                IntegerOperand {
                    register_index: -1,
                    literal: expected
                }
            );
        }
    }

    #[test]
    fn register_path_preserves_literal_and_suffix_bytes() {
        let mut source = b"  FlT[\t-4294967295]suffix  \0after".to_vec();
        let mut output = IntegerOperand {
            register_index: 91,
            literal: 92,
        };
        assert_eq!(
            decode_operand_int(&mut source, &mut output, ParserLocale::AsciiC),
            Ok(true)
        );
        assert_eq!(
            output,
            IntegerOperand {
                register_index: 1,
                literal: 92
            }
        );
        assert_eq!(source, b"  FlT[\t-4294967295\0suffix\0\0\0after");
        let mut empty_index = b"flt[]\0".to_vec();
        assert_eq!(
            decode_operand_int(&mut empty_index, &mut output, ParserLocale::AsciiC),
            Ok(true)
        );
        assert_eq!(
            output,
            IntegerOperand {
                register_index: 0,
                literal: 92
            }
        );
    }

    #[test]
    fn missing_bracket_retains_outputs_but_trims_source() {
        let mut source = b" flt[7  \0suffix".to_vec();
        let mut output = IntegerOperand {
            register_index: 91,
            literal: 92,
        };
        assert_eq!(
            decode_operand_int(&mut source, &mut output, ParserLocale::AsciiC),
            Ok(false)
        );
        assert_eq!(
            output,
            IntegerOperand {
                register_index: 91,
                literal: 92
            }
        );
        assert_eq!(source, b" flt[7\0\0\0suffix");
    }

    #[test]
    fn admission_errors_are_atomic_and_distinct_from_false() {
        for (input, expected) in [
            (&b"123"[..], ProgramParserError::MissingTerminator),
            (b"\0", ProgramParserError::EmptyOperand),
            (b"   \0", ProgramParserError::EmptyOperand),
            (
                b"12\xff  \0",
                ProgramParserError::UnsupportedCrtByte {
                    offset: 2,
                    byte: 255,
                },
            ),
        ] {
            let mut source = input.to_vec();
            let mut output = IntegerOperand {
                register_index: 91,
                literal: 92,
            };
            assert_eq!(
                decode_operand_int(&mut source, &mut output, ParserLocale::AsciiC),
                Err(expected)
            );
            assert_eq!(source, input);
            assert_eq!(
                output,
                IntegerOperand {
                    register_index: 91,
                    literal: 92
                }
            );
        }
        let mut missing = b"x,y".to_vec();
        assert_eq!(
            extrude_params(&mut missing),
            Err(ProgramParserError::MissingTerminator)
        );
        assert_eq!(
            extrude_string(&mut missing),
            Err(ProgramParserError::MissingTerminator)
        );
        assert_eq!(missing, b"x,y");
    }

    #[test]
    fn only_first_c_string_is_admitted() {
        let mut source = b"17\0\xff".to_vec();
        let mut output = IntegerOperand {
            register_index: 91,
            literal: 92,
        };
        assert_eq!(
            decode_operand_int(&mut source, &mut output, ParserLocale::AsciiC),
            Ok(true)
        );
        assert_eq!(output.literal, 17);
        assert_eq!(source, b"17\0\xff");
    }
}
