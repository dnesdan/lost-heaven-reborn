//! Byte-name comparison used by LS3DF EnumSubtree (RVA 0x1c020).
//!
//! LS3DF SHA256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! REA ev_7d5a8510e9b0cba55a5df710c55d75d10f4a3e648de3cda6d3a0428ff7c14645
//! establishes the initial-C-locale branch of upstream-labeled __strcmpi,
//! RVA 0x98830:
//! LC_CTYPE at RVA 0x1caa84 selects ASCII A..Z folding, leaving other bytes
//! unchanged. The PE initializes that word to zero. Nonzero locale handling
//! is not reconstructed. This state is independent of the Game CRT locale.
//! ModelArena owns the library context for this host session, rather than a
//! process-global CRT. No implemented locale setter changes its initial state.
//! EnumSubtree's wildcard branch uses inline I3Dtoupper (ASCII a..z only),
//! not the CRT toupper used by the separate self-first GetChildFrame routine.
//! No whole-function or original-runtime parity credit is claimed here.

#![forbid(unsafe_code)]

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Ls3dfNameLocale {
    #[default]
    InitialC,
    NonDefault,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NameComparisonError {
    EmbeddedNul,
    UnsupportedLocale,
}

pub(crate) fn enum_name_matches(
    locale: Ls3dfNameLocale,
    pattern: &[u8],
    name: &[u8],
    flags: u32,
) -> Result<bool, NameComparisonError> {
    if pattern.contains(&0) || name.contains(&0) {
        return Err(NameComparisonError::EmbeddedNul);
    }
    let case_sensitive = flags & 0x20000 != 0;
    if flags & 0x10000 == 0 {
        if case_sensitive {
            return Ok(pattern == name);
        }
        if locale != Ls3dfNameLocale::InitialC {
            return Err(NameComparisonError::UnsupportedLocale);
        }
        return Ok(pattern.eq_ignore_ascii_case(name));
    }
    for index in 0..=pattern.len().min(name.len()) {
        let pattern_byte = pattern.get(index).copied().unwrap_or(0);
        let name_byte = name.get(index).copied().unwrap_or(0);
        if pattern_byte == b'*' {
            return Ok(true);
        }
        if pattern_byte != b'?'
            && if case_sensitive {
                pattern_byte != name_byte
            } else {
                !pattern_byte.eq_ignore_ascii_case(&name_byte)
            }
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_locale_compares_all_bytes_without_decoding_or_extended_case_folding() {
        for byte in 1..=u8::MAX {
            assert_eq!(
                enum_name_matches(Ls3dfNameLocale::InitialC, &[byte], &[byte], 0),
                Ok(true)
            );
            for other in 1..=u8::MAX {
                assert_eq!(
                    enum_name_matches(Ls3dfNameLocale::InitialC, &[byte], &[other], 0),
                    Ok(byte == other || (byte.is_ascii_alphabetic() && (byte ^ 0x20) == other))
                );
            }
        }
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::InitialC, b"\xa8BODY", b"\xa8body", 0),
            Ok(true)
        );
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::InitialC, b"\xc0", b"\xe0", 0),
            Ok(false)
        );
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::InitialC, b"BODY", b"body", 0x20000),
            Ok(false)
        );
    }

    #[test]
    fn wildcard_retains_native_terminator_and_star_suffix_behavior() {
        for (pattern, name, expected) in [
            (b"\xa8b?*ignored".as_slice(), b"\xa8BoDy".as_slice(), true),
            (b"?".as_slice(), b"".as_slice(), true),
            (b"??".as_slice(), b"".as_slice(), true),
            (b"A?".as_slice(), b"a".as_slice(), true),
            (b"A*ignored".as_slice(), b"a".as_slice(), true),
            (b"\xa9*".as_slice(), b"\xa8body".as_slice(), false),
        ] {
            assert_eq!(
                enum_name_matches(Ls3dfNameLocale::NonDefault, pattern, name, 0x10000),
                Ok(expected)
            );
        }
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::InitialC, b"a*", b"A", 0x30000),
            Ok(false)
        );
    }

    #[test]
    fn invalid_storage_and_unreconstructed_locale_remain_errors() {
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::InitialC, b"BODY\0", b"BODY", 0),
            Err(NameComparisonError::EmbeddedNul)
        );
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::InitialC, b"BODY", b"BODY\0", 0),
            Err(NameComparisonError::EmbeddedNul)
        );
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::NonDefault, b"BODY", b"BODY", 0),
            Err(NameComparisonError::UnsupportedLocale)
        );
        assert_eq!(
            enum_name_matches(Ls3dfNameLocale::NonDefault, b"\xa8", b"\xa8", 0x20000),
            Ok(true)
        );
    }
}
