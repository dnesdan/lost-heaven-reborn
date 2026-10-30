use super::*;
use crate::program_parameters::ascii_integer_prefix;

#[derive(Default)]
struct Backend {
    calls: Vec<&'static str>,
    stop: Option<&'static str>,
    null: bool,
    released_register: Option<i32>,
}
impl NameParameterBackend for Backend {
    type Error = &'static str;
    fn allocate_name(&mut self) -> Result<Option<Box<NameParameter>>, Self::Error> {
        self.calls.push("allocate");
        if self.stop == Some("allocate") {
            return Err("allocate");
        }
        Ok((!self.null).then(|| Box::new(NameParameter::default())))
    }
    fn convert_register(&mut self, token: &[u8], p: &NameParameter) -> Result<i32, Self::Error> {
        self.calls.push("convert");
        assert_eq!((p.register(), p.name()), (None, None));
        if self.stop == Some("convert") {
            return Err("convert");
        }
        ascii_integer_prefix(token).map_err(|_| "conversion domain")
    }
    fn release_rejected_name(&mut self, p: &NameParameter) -> Result<(), Self::Error> {
        self.calls.push("release");
        self.released_register = p.register();
        assert_eq!(p.name(), None);
        if self.stop == Some("release") {
            return Err("release");
        }
        Ok(())
    }
}
fn decode(opcode: u32, input: &[u8]) -> (NameParameterSlot, Backend, Vec<u8>, bool) {
    let mut slot = NameParameterSlot::default();
    let mut backend = Backend::default();
    let mut bytes = input.to_vec();
    let accepted = slot.decode(opcode, &mut bytes, &mut backend).unwrap();
    (slot, backend, bytes, accepted)
}

#[test]
fn all_three_selectors_keep_original_name_bytes_and_integer_prefix() {
    for opcode in [13, 18, 43] {
        let (slot, backend, bytes, accepted) = decode(opcode, b"  -4294967295junk, \"p\x80\xff\"tail \0after");
        assert!(accepted);
        assert_eq!(backend.calls, ["allocate", "convert"]);
        let p = slot.parameter().unwrap();
        assert_eq!(p.register(), Some(1));
        assert_eq!(p.name(), Some(b"p\x80\xff".as_slice()));
        assert_eq!(bytes, b"  -4294967295junk\0 \"p\x80\xff\0tail\0\0after");
    }
}

#[test]
fn omitted_empty_and_trailing_comma_names_follow_actual_parameter_counts() {
    for opcode in [13, 18, 43] {
        for source in [b"7\0".as_slice(), b"7,\0", b"7,\"\"\0"] {
            let (slot, _, _, accepted) = decode(opcode, source);
            assert!(accepted);
            assert_eq!(slot.parameter().unwrap().register(), Some(7));
            assert_eq!(slot.parameter().unwrap().name(), Some(b"".as_slice()));
        }
        // A trailing space is counted, unlike a trailing empty argument.
        let (slot, backend, _, accepted) = decode(opcode, b"7, \0");
        assert!(!accepted);
        assert!(slot.parameter().is_none());
        assert_eq!(backend.calls, ["allocate", "convert", "release"]);
    }
}

#[test]
fn count_rejections_mutate_tokenizer_storage_but_never_allocate() {
    for input in [b"\0".as_slice(), b"0,\"a,b\"\0", b"0,\"x\",2\0"] {
        let (_, backend, bytes, accepted) = decode(13, input);
        assert!(!accepted);
        assert!(backend.calls.is_empty());
        if input.contains(&b',') {
            assert_ne!(bytes, input);
            assert!(!bytes.contains(&b','));
        }
    }
}

#[test]
fn first_closing_quote_wins_without_escaping_or_suffix_validation() {
    let (slot, _, bytes, accepted) = decode(18, b"2,\"a\\\"tail\"suffix\0");
    assert!(accepted);
    assert_eq!(slot.parameter().unwrap().name(), Some(b"a\\".as_slice()));
    assert_eq!(bytes, b"2\0\"a\\\0tail\"suffix\0");
}

#[test]
fn missing_quotes_release_the_published_register_before_native_false() {
    for input in [b"9,noquote\0".as_slice(), b"9,\"unclosed\0", b"9,\t\"x\"\0"] {
        let (slot, backend, _, accepted) = decode(43, input);
        assert!(!accepted);
        assert!(slot.parameter().is_none());
        assert_eq!(backend.calls, ["allocate", "convert", "release"]);
        assert_eq!(backend.released_register, Some(9));
    }
}

#[test]
fn exact_name_extent_accepts_63_and_retains_prefix_at_64_without_truncation() {
    for length in [63, 64, 65] {
        let mut source = b"3,\"".to_vec();
        source.extend(std::iter::repeat_n(b'x', length));
        source.extend_from_slice(b"\"tail\0after");
        let mut slot = NameParameterSlot::default();
        let mut backend = Backend::default();
        let result = slot.decode(13, &mut source, &mut backend);
        assert_eq!(backend.calls, ["allocate", "convert"]);
        assert_eq!(source[3 + length], 0); // quote extrusion precedes the extent guard
        assert_eq!(&source[4 + length..], b"tail\0after");
        let p = slot.parameter().unwrap();
        assert_eq!(p.register(), Some(3));
        if length == 63 {
            assert_eq!(result, Ok(true));
            assert_eq!(p.name(), Some(vec![b'x'; length].as_slice()));
        } else {
            assert_eq!(result, Err(NameDecodeError::NameTooLong { length }));
            assert_eq!(p.name(), None);
        }
    }
}

#[test]
fn non_ascii_register_is_not_confused_with_allowed_non_ascii_name() {
    let mut source = b"1\xff,\"valid\"\0".to_vec();
    let mut slot = NameParameterSlot::default();
    let mut backend = Backend::default();
    assert_eq!(slot.decode(13, &mut source, &mut backend), Err(NameDecodeError::Conversion("conversion domain")));
    assert_eq!(source, b"1\xff\0\"valid\"\0"); // the closing quote is still untouched
    assert_eq!(slot.parameter().unwrap().register(), None);
    assert_eq!(slot.parameter().unwrap().name(), None);
}

#[test]
fn every_effect_failure_keeps_its_prefix_and_cannot_be_reentered() {
    for stop in ["allocate", "convert", "release"] {
        let mut backend = Backend { stop: Some(stop), ..Backend::default() };
        let mut slot = NameParameterSlot::default();
        let mut source = b"4,invalid\0".to_vec();
        let result = slot.decode(18, &mut source, &mut backend);
        let expected = match stop {
            "allocate" => NameDecodeError::Allocation(stop),
            "convert" => NameDecodeError::Conversion(stop),
            _ => NameDecodeError::RejectionRelease(stop),
        };
        assert_eq!(result, Err(expected));
        assert_eq!(slot.parameter().is_some(), stop != "allocate");
        if stop == "release" {
            assert_eq!(slot.parameter().unwrap().register(), Some(4));
        }
        let calls = backend.calls.clone();
        let written = source.clone();
        assert_eq!(slot.decode(18, &mut source, &mut backend), Err(NameDecodeError::AlreadyEntered));
        assert_eq!(backend.calls, calls);
        assert_eq!(source, written);
    }
}

#[test]
fn null_allocation_never_reaches_conversion() {
    let mut backend = Backend { null: true, ..Backend::default() };
    let mut slot = NameParameterSlot::default();
    assert_eq!(slot.decode(13, &mut b"0,\"x\"\0".to_vec(), &mut backend), Err(NameDecodeError::NullAllocation));
    assert!(slot.parameter().is_none());
    assert_eq!(backend.calls, ["allocate"]);
}

#[test]
fn unsupported_selector_runs_common_tokenization_only_and_missing_nul_preserves_input() {
    let mut slot = NameParameterSlot::default();
    let mut backend = Backend::default();
    let mut source = b" 1 , \"x\" \0suffix".to_vec();
    assert_eq!(slot.decode(67, &mut source, &mut backend), Err(NameDecodeError::UnsupportedOpcode(67)));
    assert_eq!(source, b" 1\0\0 \"x\"\0\0suffix");
    assert!(backend.calls.is_empty());
    let mut source = b"1,\"x\"".to_vec();
    let before = source.clone();
    assert_eq!(NameParameterSlot::default().decode(13, &mut source, &mut backend), Err(NameDecodeError::Parser(ProgramParserError::MissingTerminator)));
    assert_eq!(source, before);
    assert!(backend.calls.is_empty());
}

#[test]
fn unwound_rejection_release_keeps_the_register_and_prevents_replay() {
    struct PanicRelease(Backend);
    impl NameParameterBackend for PanicRelease {
        type Error = &'static str;
        fn allocate_name(&mut self) -> Result<Option<Box<NameParameter>>, Self::Error> {
            self.0.allocate_name()
        }
        fn convert_register(&mut self, token: &[u8], p: &NameParameter) -> Result<i32, Self::Error> {
            self.0.convert_register(token, p)
        }
        fn release_rejected_name(&mut self, _: &NameParameter) -> Result<(), Self::Error> {
            self.0.calls.push("release");
            panic!("release before unwind");
        }
    }
    let mut backend = PanicRelease(Backend::default());
    let mut slot = NameParameterSlot::default();
    let mut source = b"9,invalid\0".to_vec();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = slot.decode(13, &mut source, &mut backend);
    })).is_err());
    assert_eq!(slot.parameter().unwrap().register(), Some(9));
    assert_eq!(slot.decode(13, &mut source, &mut backend), Err(NameDecodeError::AlreadyEntered));
    assert_eq!(backend.0.calls, ["allocate", "convert", "release"]);
}
