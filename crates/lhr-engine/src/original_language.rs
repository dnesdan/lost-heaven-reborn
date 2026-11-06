//! Source-only language routing from pinned mafia-re Main.cpp's InitSystem
//! and ui/g_loadsave.cpp / ui/g_indicators.cpp. No new native parity credit.

/// The two-byte data version becomes the original multicharacter language
/// constant. Missing/short input selects English. Preserve the source's unusual
/// uppercase subtraction rather than silently replacing it with lowercasing.
pub fn language_code_from_version(bytes: &[u8]) -> u32 {
    let Some(first) = bytes.get(..2) else {
        return 0x656e;
    };
    let letters = [first[0], first[1]].map(|byte| {
        if byte.is_ascii_uppercase() {
            byte.wrapping_sub(0x20)
        } else {
            byte
        }
    });
    (u32::from(letters[0]) << 8) | u32::from(letters[1])
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageResourcePaths {
    pub text_database: String,
    pub primary_map: String,
}
/// G_LoadSaveClass and G_IndicatorsClass insert the two letters into these
/// exact names for the ordinary version-derived codes used by the menu.
/// Unsafe/non-ASCII names remain a host admission boundary; this does not
/// implement G_LoadSaveClass's -1/game-state or indicator-only zero branches.
pub fn language_resource_paths(code: u32) -> Result<LanguageResourcePaths, &'static str> {
    let letters = [(code >> 8) as u8, code as u8];
    if !letters
        .iter()
        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'/' | b'\\' | b':'))
    {
        return Err("Unsupported language filename bytes");
    }
    let language = std::str::from_utf8(&letters).unwrap();
    Ok(LanguageResourcePaths {
        text_database: format!("tables/textdb_{language}.def"),
        primary_map: format!("tables/lang_{language}.def"),
    })
}

/// `GM_Menu::CreateCompsByDef` localized menu lookup (GAME 0x5ea370,
/// mafia-re `game/ui/g_menu.cpp:646-660`): `tables\menu\<name>_<lang>.mnu`
/// with the two `G_IndicatorsClass::GetLanguage` (GAME 0x5f70d0) letters,
/// the first in the low byte. Zero skips the localized attempt and the
/// caller falls back to `menu_fallback_path`, exactly like the native
/// `if (language)` gate. Note the convention differs from
/// `language_code_from_version` (first letter high, as
/// `G_LoadSaveClass::LanguageSelect` splices it high-first into
/// `tables\lang_<language>.def`); the menu path splices low-first.
/// Host staging rejects non-graphic or separator letters, mirroring the
/// admission boundary of `language_resource_paths`.
pub fn menu_path_for_language(name: &str, language: u32) -> Result<Option<String>, &'static str> {
    if language == 0 {
        return Ok(None);
    }
    let letters = [(language & 0xff) as u8, ((language >> 8) & 0xff) as u8];
    if !letters
        .iter()
        .all(|byte| byte.is_ascii_graphic() && !matches!(byte, b'/' | b'\\' | b':'))
    {
        return Err("Unsupported language filename bytes");
    }
    let language = std::str::from_utf8(&letters).unwrap();
    Ok(Some(format!("tables\\menu\\{name}_{language}.mnu")))
}

/// `GM_Menu::CreateCompsByDef` fallback (GAME 0x5ea370,
/// `g_menu.cpp:654-660`): `tables\menu\<name>.mnu`. `name` stays
/// caller-staged, as the original passes it straight to `sprintf`.
pub fn menu_fallback_path(name: &str) -> String {
    format!("tables\\menu\\{name}.mnu")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn version_bytes_preserve_source_order_short_default_and_uppercase_subtraction() {
        for bytes in [b"".as_slice(), b"c"] {
            assert_eq!(language_code_from_version(bytes), 0x656e);
        }
        assert_eq!(language_code_from_version(b"cz-extra"), 0x637a);
        assert_eq!(language_code_from_version(b"pl"), 0x706c);
        assert_eq!(language_code_from_version(b"EN"), 0x252e);
        assert_eq!(
            language_resource_paths(0x637a).unwrap().text_database,
            "tables/textdb_cz.def"
        );
        assert_eq!(
            language_resource_paths(0x656e).unwrap().primary_map,
            "tables/lang_en.def"
        );
        for code in [0, 0x2f2e, 0x3a2e, 0xffff] {
            assert!(language_resource_paths(code).is_err());
        }
    }
    #[test]
    fn menu_paths_follow_get_language_low_first_convention() {
        // `GetLanguage` returns 'e','n' as 0x6e65 (first letter low).
        assert_eq!(
            menu_path_for_language("main", 0x6e65).unwrap(),
            Some("tables\\menu\\main_en.mnu".to_string())
        );
        assert_eq!(
            menu_path_for_language("main", 0x7a63).unwrap(),
            Some("tables\\menu\\main_cz.mnu".to_string())
        );
        // Zero skips the localized attempt; the caller tries the fallback.
        assert_eq!(menu_path_for_language("main", 0).unwrap(), None);
        assert_eq!(menu_fallback_path("main"), "tables\\menu\\main.mnu");
        for code in [0x2f2e, 0x3a2e, 0xffff, 0x2000] {
            assert!(menu_path_for_language("main", code).is_err());
        }
    }
}
