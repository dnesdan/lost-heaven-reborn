//! Bounded integer-scalar arm of original C_program::DecodeParams.
//!
//! Game SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! canonical RVA0x61c70, arm VA0x461e32. This is a branch, not a separate
//! recovered function. Source tokenization precedes count validation; allocation
//! is published before the required CRT integer conversion. No original Init,
//! Done, compiled instruction vector or VM execution is supplied here.
#![forbid(unsafe_code)]
use crate::program_parser::{extrude_params, ProgramParserError};

/// Four-byte scalar's semantic value. None records the reached allocation
/// before conversion/store; it is not an invented initialized native value.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ScalarParameter {
    value: Option<i32>,
}
impl ScalarParameter {
    pub fn value(&self) -> Option<i32> {
        self.value
    }
}

/// Actual host allocation and required conversion boundaries. The Box returned
/// by allocate_scalar transfers into the slot before convert_integer runs.
/// A null allocation is outside safe admission: original code dereferences it.
/// Conversion is original CRT integer-prefix conversion, not DecodeOperandInt.
/// Errors must preserve any reached backend effects; no rollback is requested.
pub trait ScalarParameterBackend {
    type Error;
    fn allocate_scalar(&mut self) -> Result<Option<Box<ScalarParameter>>, Self::Error>;
    fn convert_integer(
        &mut self,
        token: &[u8],
        published: &ScalarParameter,
    ) -> Result<i32, Self::Error>;
}

/// Concrete ASCII C-locale boundary. Host Box allocation supplies the actual
/// parameter storage; process-wide allocator exhaustion is not modeled as a
/// recoverable native result. Custom backends can expose interruption/null.
#[derive(Default)]
pub struct AsciiScalarParameterBackend;
impl ScalarParameterBackend for AsciiScalarParameterBackend {
    type Error = ProgramParserError;
    fn allocate_scalar(&mut self) -> Result<Option<Box<ScalarParameter>>, Self::Error> {
        Ok(Some(Box::new(ScalarParameter::default())))
    }
    fn convert_integer(&mut self, token: &[u8], _: &ScalarParameter) -> Result<i32, Self::Error> {
        ascii_integer_prefix(token)
    }
}

/// CRT wrapper RVA0x210bf5 calls helper RVA0x210b6a. In admitted ASCII C
/// locale, skip whitespace, consume optional sign and decimal prefix, wrapping
/// each multiply/add and final negation at32 bits. No digits means zero.
/// Non-ASCII tokens are outside this locale admission, even after a nondigit.
/// This converts integer text directly; flt[...] is not a register operand.
pub fn ascii_integer_prefix(token: &[u8]) -> Result<i32, ProgramParserError> {
    if let Some((offset, &byte)) = token.iter().enumerate().find(|(_, byte)| !byte.is_ascii()) {
        return Err(ProgramParserError::UnsupportedCrtByte { offset, byte });
    }
    let mut position = 0;
    while token
        .get(position)
        .is_some_and(|byte| matches!(byte, b' ' | b'\t'..=b'\r'))
    {
        position += 1;
    }
    let negative = token.get(position) == Some(&b'-');
    if negative || token.get(position) == Some(&b'+') {
        position += 1;
    }
    let mut value = 0u32;
    while let Some(&byte @ b'0'..=b'9') = token.get(position) {
        value = value.wrapping_mul(10).wrapping_add(u32::from(byte - b'0'));
        position += 1;
    }
    Ok(if negative {
        value.wrapping_neg()
    } else {
        value
    } as i32)
}

#[derive(Debug, PartialEq, Eq)]
pub enum ScalarDecodeError<E> {
    OccupiedSlot,
    Parser(ProgramParserError),
    UnsupportedOpcode(u32),
    Allocation(E),
    NullAllocation,
    Conversion(E),
}

/// One fresh instruction parameter slot, corresponding to Compile's initially
/// null parameter field. Its Box is the actual allocation owner, not an opaque
/// native address or a second allocation map. Dropping it releases host storage;
/// that is not implementation of original C_program::Done callback ordering.
#[derive(Debug, Default)]
pub struct ScalarParameterSlot {
    parameter: Option<Box<ScalarParameter>>,
}
impl ScalarParameterSlot {
    pub fn parameter(&self) -> Option<&ScalarParameter> {
        self.parameter.as_deref()
    }

    /// Return false only for the native counted-parameter rejection. Unknown
    /// arms remain explicit errors after the reached common tokenizer writes.
    /// Occupied slots are outside this fresh-record API's host admission.
    pub fn decode<B: ScalarParameterBackend>(
        &mut self,
        opcode: u32,
        source: &mut [u8],
        backend: &mut B,
    ) -> Result<bool, ScalarDecodeError<B::Error>> {
        if self.parameter.is_some() {
            return Err(ScalarDecodeError::OccupiedSlot);
        }
        let parameters = extrude_params(source).map_err(ScalarDecodeError::Parser)?;
        if !supports_scalar_opcode(opcode) {
            return Err(ScalarDecodeError::UnsupportedOpcode(opcode));
        }
        if parameters.count != 1 {
            return Ok(false);
        }
        let allocation = backend
            .allocate_scalar()
            .map_err(ScalarDecodeError::Allocation)?;
        self.parameter = allocation;
        let parameter = self
            .parameter
            .as_mut()
            .ok_or(ScalarDecodeError::NullAllocation)?;
        // The allocator's physical prior bytes are not an initialized scalar.
        parameter.value = None;
        let start = parameters.offsets[0].expect("counted first token");
        let end = source[start..]
            .iter()
            .position(|&byte| byte == 0)
            .map(|length| start + length)
            .expect("tokenizer preserves a terminator");
        let value = backend
            .convert_integer(&source[start..end], parameter)
            .map_err(ScalarDecodeError::Conversion)?;
        parameter.value = Some(value);
        Ok(true)
    }
}

/// Exact registry IDs selecting the original one-integer/4-byte branch.
/// This proves parameter layout only, not execution of these commands.
pub fn supports_scalar_opcode(opcode: u32) -> bool {
    matches!(
        opcode,
        7 | 11
            | 12
            | 19
            | 24
            | 30
            | 34
            | 55
            | 75
            | 81
            | 84
            | 89
            | 91
            | 95
            | 97
            | 100
            | 104
            | 114
            | 118
            | 119
            | 121
            | 125
            | 127
            | 131
            | 133
            | 134
            | 149
            | 151
            | 152
            | 159
            | 164
            | 165
            | 168
            | 169
            | 172
            | 173
            | 183
            | 184
            | 185
            | 186
            | 204
            | 206
            | 209
            | 210
            | 211
            | 213
            | 215
            | 216
            | 217
            | 220
            | 221
            | 224
            | 235
            | 240
            | 241
            | 242
            | 243
            | 244
            | 245
            | 246
            | 270
            | 272
            | 276
            | 277
            | 279
            | 285
            | 292
            | 296
            | 300
            | 306
            | 309
            | 311
            | 312
            | 313
            | 315
            | 324
            | 332
            | 333
            | 334
            | 335
            | 339
            | 340
            | 350
            | 352
            | 353
            | 354
            | 355
            | 359
            | 364
            | 366
            | 368
            | 369
            | 370
            | 371
            | 374
            | 375
            | 376
            | 379
            | 386
            | 388
            | 392
            | 394
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Boundary {
        allocations: usize,
        tokens: Vec<Vec<u8>>,
        interrupt: bool,
        null: bool,
    }
    impl ScalarParameterBackend for Boundary {
        type Error = &'static str;
        fn allocate_scalar(&mut self) -> Result<Option<Box<ScalarParameter>>, Self::Error> {
            self.allocations += 1;
            Ok((!self.null).then(|| Box::new(ScalarParameter::default())))
        }
        fn convert_integer(
            &mut self,
            token: &[u8],
            published: &ScalarParameter,
        ) -> Result<i32, Self::Error> {
            assert_eq!(published.value(), None);
            self.tokens.push(token.to_vec());
            if self.interrupt {
                Err("conversion unavailable")
            } else {
                Ok(-73)
            }
        }
    }
    #[test]
    fn count_rejection_keeps_tokenizer_prefix_without_allocation() {
        let mut source = b"  one , two  \0tail".to_vec();
        let mut slot = ScalarParameterSlot::default();
        let mut b = Boundary::default();
        assert_eq!(slot.decode(12, &mut source, &mut b), Ok(false));
        assert_eq!(&source, b"  one\0\0 two\0\0\0tail");
        assert_eq!(b.allocations, 0);
        assert!(slot.parameter().is_none());
    }
    #[test]
    fn trailing_comma_single_token_publishes_before_conversion() {
        let mut source = b"  -73suffix ,\0".to_vec();
        let mut slot = ScalarParameterSlot::default();
        let mut b = Boundary::default();
        assert_eq!(slot.decode(34, &mut source, &mut b), Ok(true));
        assert_eq!(b.tokens, vec![b"-73suffix".to_vec()]);
        assert_eq!(slot.parameter().unwrap().value(), Some(-73));
        let previous = source.clone();
        assert_eq!(
            slot.decode(34, &mut source, &mut b),
            Err(ScalarDecodeError::OccupiedSlot)
        );
        assert_eq!(source, previous);
        assert_eq!(b.allocations, 1);
    }
    #[test]
    fn interrupted_conversion_keeps_published_allocation_and_unwritten_value() {
        let mut source = b"9\0".to_vec();
        let mut slot = ScalarParameterSlot::default();
        let mut b = Boundary {
            interrupt: true,
            ..Boundary::default()
        };
        assert_eq!(
            slot.decode(11, &mut source, &mut b),
            Err(ScalarDecodeError::Conversion("conversion unavailable"))
        );
        assert_eq!(slot.parameter().unwrap().value(), None);
        assert_eq!(b.allocations, 1);
    }
    #[test]
    fn unsupported_branch_and_null_allocation_never_report_native_success() {
        let mut source = b" a , b \0".to_vec();
        let mut slot = ScalarParameterSlot::default();
        let mut b = Boundary::default();
        assert_eq!(
            slot.decode(45, &mut source, &mut b),
            Err(ScalarDecodeError::UnsupportedOpcode(45))
        );
        assert_eq!(b.allocations, 0);
        assert!(source.contains(&0));
        let mut source = b"9\0".to_vec();
        b.null = true;
        assert_eq!(
            slot.decode(12, &mut source, &mut b),
            Err(ScalarDecodeError::NullAllocation)
        );
        assert!(b.tokens.is_empty());
        assert!(slot.parameter().is_none());
        assert_eq!(
            (0..398).filter(|&id| supports_scalar_opcode(id)).count(),
            102
        );
    }
    #[test]
    fn real_integer_conversion_preserves_crt_prefix_wrapping_and_not_register_semantics() {
        for (token, expected) in [
            ("", 0),
            ("\t -73suffix", -73),
            ("+4294967295", -1),
            ("4294967296", 0),
            ("-2147483649", 2147483647),
            ("flt[7]", 0),
            ("--9", 0),
        ] {
            assert_eq!(ascii_integer_prefix(token.as_bytes()), Ok(expected));
        }
        let mut backend = AsciiScalarParameterBackend;
        for opcode in (0..398).filter(|&id| supports_scalar_opcode(id)) {
            let mut slot = ScalarParameterSlot::default();
            let mut source = b" -4294967295tail,\0".to_vec();
            assert_eq!(slot.decode(opcode, &mut source, &mut backend), Ok(true));
            assert_eq!(slot.parameter().unwrap().value(), Some(1));
        }
    }
    #[test]
    fn concrete_converter_admission_failure_keeps_allocation_after_tokenization() {
        let mut backend = AsciiScalarParameterBackend;
        let mut slot = ScalarParameterSlot::default();
        let mut source = vec![b' ', 0xff, b' ', 0];
        assert_eq!(
            slot.decode(12, &mut source, &mut backend),
            Err(ScalarDecodeError::Conversion(
                ProgramParserError::UnsupportedCrtByte {
                    offset: 0,
                    byte: 0xff
                }
            ))
        );
        assert_eq!(slot.parameter().unwrap().value(), None);
        assert_eq!(source, vec![b' ', 0xff, 0, 0]);
    }
}
