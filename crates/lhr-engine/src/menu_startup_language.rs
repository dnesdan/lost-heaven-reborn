//! Bounded complete-image startup language intake, not native LanguageSelect.
//!
//! Source-only ordering from mafia-re fe7e74643f5415c358aa312dd4e7d9da5f60ddcc:
//! `game/ui/g_loadsave.cpp:913`, `game/ui/g_indicators.cpp:219`, and
//! `game/ui/g_textdb.cpp:26`. No new native function or parity credit.

#![forbid(unsafe_code)]

use crate::indicators::IndicatorsState;
use crate::menu_draw_resources::{MenuDrawResources, MenuFontLanguage};
use crate::original_language::language_resource_paths;
use lhr_assets::Installation;

/// Intake an already-resolved explicit startup language code, high byte first.
///
/// The host path domain requires zero upper 16 bits and the two graphic ASCII
/// filename bytes admitted by `language_resource_paths` (excluding '/', '\\',
/// and ':'). Unsupported codes, including zero and `u32::MAX`, fail before any
/// read or owner mutation. No default language or native special branch is used.
///
/// Reads and installs the raw text database, Primary map, then Czech Credits map
/// in that order. Absence preserves the corresponding existing owner; host or
/// parse errors stop with the reached prefix, without rollback or automatic
/// replay. A successful Primary install immediately updates the live indicators
/// from all four bytes of its canonical `raw_header[4..8]`, before Credits intake.
///
/// `Installation::read_optional` supplies complete images, not native
/// open/read/close effects. In particular, native LoadDatabase discards the old
/// database after a successful open but before its read; a host read error here
/// leaves that owner untouched. Such stream-failure inputs are not admitted for
/// native fidelity. Once a complete database image reaches its owner, that
/// owner's reset-before-parse failure semantics apply without restoration.
///
/// `Ok(())` only completes these optional image intakes. It does not initialize
/// indicators, load/restart fonts or atlases, admit incoming numeric precision
/// or menu layout, or constitute a production startup gate or full LanguageSelect.
pub fn select_startup_menu_language(
    installation: &Installation,
    requested_language: u32,
    resources: &mut MenuDrawResources,
    indicators: &mut IndicatorsState,
) -> Result<(), String> {
    select_startup_menu_language_from_source(requested_language, resources, indicators, |path| {
        installation
            .read_optional(path)
            .map_err(|error| error.to_string())
    })
}

fn select_startup_menu_language_from_source(
    requested_language: u32,
    resources: &mut MenuDrawResources,
    indicators: &mut IndicatorsState,
    mut read_optional: impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
) -> Result<(), String> {
    if requested_language > u32::from(u16::MAX) {
        return Err(format!(
            "unsupported startup menu language code {requested_language:#010x}: upper 16 bits must be zero"
        ));
    }
    let paths = language_resource_paths(requested_language).map_err(|error| {
        format!("unsupported startup menu language code {requested_language:#010x}: {error}")
    })?;

    if let Some(bytes) = read_optional(&paths.text_database)? {
        resources.text_database_mut().load_from_bytes(&bytes)?;
    }
    if let Some(bytes) = read_optional(&paths.primary_map)? {
        resources.install_language(MenuFontLanguage::Primary, &bytes)?;
        let header = &resources.language(MenuFontLanguage::Primary)?.raw_header;
        indicators.set_language_from_table([header[4], header[5], header[6], header[7]]);
    }
    if let Some(bytes) = read_optional("tables/lang_cz.def")? {
        resources.install_language(MenuFontLanguage::Credits, &bytes)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "menu_startup_language_tests.rs"]
mod tests;
