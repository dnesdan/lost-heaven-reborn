use super::*;
use crate::indicators::TimerClock;
use crate::itexture::{CpuTextureSurface, TextureKey};
use crate::menu_draw_resources::{MenuTextDatabase, MenuTextureBackend};
use std::sync::Arc;

const OLD_TEXT: &[u8] = b"old";
const NEW_TEXT: &[u8] = b"\x81\x90\xff~new";
const OLD_DATABASE_HEADER: u32 = 0x1234_5678;
const NEW_DATABASE_HEADER: u32 = 0x8765_4321;
const OLD_INDICATOR_LANGUAGE: [u8; 4] = *b"pl\0\0";
const NEW_PRIMARY_LANGUAGE: [u8; 4] = [b'f', b'r', 0, b'Q'];
const OLD_PRIMARY_GLYPH: u16 = 11;
const OLD_CREDITS_GLYPH: u16 = 12;
const NEW_PRIMARY_GLYPH: u16 = 21;
const NEW_CREDITS_GLYPH: u16 = 22;

type ReadStep = (&'static str, Result<Option<Vec<u8>>, String>);

struct NoTextureOperations;

impl MenuTextureBackend for NoTextureOperations {
    fn create_alpha_texture(&mut self, _name: &[u8]) -> Result<Option<TextureKey>, String> {
        panic!("startup language intake must not create textures")
    }

    fn release_texture(&mut self, _key: TextureKey) -> Result<(), String> {
        panic!("startup language intake must not release textures")
    }

    fn texture_surface(&self, _key: TextureKey) -> Result<&CpuTextureSurface, String> {
        panic!("startup language intake must not read texture surfaces")
    }
}

fn database_image(text: &[u8], unknown: u32) -> Vec<u8> {
    let mut bytes = vec![0; 32];
    bytes[..4].copy_from_slice(&3u32.to_le_bytes());
    bytes[4..8].copy_from_slice(&unknown.to_le_bytes());
    for (index, (text_id, text_bytes)) in [
        (1u32, text),
        (2u32, b"second".as_slice()),
        (3u32, b"third".as_slice()),
    ]
    .into_iter()
    .enumerate()
    {
        let table_offset = 8 + index * 8;
        let text_offset = bytes.len() as u32;
        bytes[table_offset..table_offset + 4].copy_from_slice(&text_id.to_le_bytes());
        bytes[table_offset + 4..table_offset + 8].copy_from_slice(&text_offset.to_le_bytes());
        bytes.extend_from_slice(text_bytes);
        bytes.push(0);
    }
    bytes
}

fn language_image(language: [u8; 4], glyph: u16) -> Vec<u8> {
    let mut bytes = vec![0; 56 + 256 * 2];
    bytes[..4].copy_from_slice(b"Lang");
    bytes[4..8].copy_from_slice(&language);
    bytes[8..17].copy_from_slice(b"Synthetic");
    bytes[40..44].copy_from_slice(b"_syn");
    let glyph_offset = 56 + usize::from(b'A') * 2;
    bytes[glyph_offset..glyph_offset + 2].copy_from_slice(&glyph.to_le_bytes());
    bytes
}

fn empty_owners() -> (MenuDrawResources, IndicatorsState) {
    let resources = MenuDrawResources::new(MenuTextDatabase::new(), NoTextureOperations);
    let mut indicators = IndicatorsState::constructed(TimerClock {
        hour: 13,
        minute: 30,
        second: 45,
    });
    indicators.set_language_from_table(OLD_INDICATOR_LANGUAGE);
    (resources, indicators)
}

fn populated_owners() -> (MenuDrawResources, IndicatorsState) {
    let (mut resources, indicators) = empty_owners();
    resources
        .text_database_mut()
        .load_from_bytes(&database_image(OLD_TEXT, OLD_DATABASE_HEADER))
        .unwrap();
    resources
        .install_language(
            MenuFontLanguage::Primary,
            &language_image(*b"de\0\0", OLD_PRIMARY_GLYPH),
        )
        .unwrap();
    resources
        .install_language(
            MenuFontLanguage::Credits,
            &language_image(*b"cz\0\0", OLD_CREDITS_GLYPH),
        )
        .unwrap();
    (resources, indicators)
}

fn selection_images() -> [(&'static str, Vec<u8>); 3] {
    [
        (
            "tables/textdb_en.def",
            database_image(NEW_TEXT, NEW_DATABASE_HEADER),
        ),
        (
            "tables/lang_en.def",
            language_image(NEW_PRIMARY_LANGUAGE, NEW_PRIMARY_GLYPH),
        ),
        (
            "tables/lang_cz.def",
            language_image(*b"cz\0\0", NEW_CREDITS_GLYPH),
        ),
    ]
}

fn select_with_reads(
    requested_language: u32,
    resources: &mut MenuDrawResources,
    indicators: &mut IndicatorsState,
    expected_reads: Vec<ReadStep>,
) -> Result<(), String> {
    let mut reads = expected_reads.into_iter();
    let result = select_startup_menu_language_from_source(
        requested_language,
        resources,
        indicators,
        |path| {
            let (expected_path, response) = reads.next().expect("unexpected or replayed read");
            assert_eq!(path, expected_path);
            response
        },
    );
    assert!(reads.next().is_none(), "expected read was not reached");
    result
}

fn assert_database(resources: &mut MenuDrawResources, replaced: bool) {
    assert!(resources.text_database().is_loaded());
    assert_eq!(resources.text_database().len(), 3);
    assert_eq!(
        resources.text_database().unknown(),
        Some(if replaced {
            NEW_DATABASE_HEADER
        } else {
            OLD_DATABASE_HEADER
        })
    );
    assert_eq!(
        resources.text(1).unwrap().unwrap().as_ref(),
        if replaced { NEW_TEXT } else { OLD_TEXT }
    );
}

fn assert_maps(
    resources: &MenuDrawResources,
    indicators: &IndicatorsState,
    primary_replaced: bool,
    credits_replaced: bool,
) {
    assert_eq!(
        resources
            .language(MenuFontLanguage::Primary)
            .unwrap()
            .glyph(b'A'),
        if primary_replaced {
            NEW_PRIMARY_GLYPH
        } else {
            OLD_PRIMARY_GLYPH
        }
    );
    assert_eq!(
        resources
            .language(MenuFontLanguage::Credits)
            .unwrap()
            .glyph(b'A'),
        if credits_replaced {
            NEW_CREDITS_GLYPH
        } else {
            OLD_CREDITS_GLYPH
        }
    );
    assert_eq!(
        indicators.get_language(),
        u32::from_le_bytes(if primary_replaced {
            NEW_PRIMARY_LANGUAGE
        } else {
            OLD_INDICATOR_LANGUAGE
        })
    );
}

#[test]
fn complete_images_install_in_order_with_raw_text_and_all_four_primary_header_bytes() {
    let (mut resources, mut indicators) = populated_owners();
    let issued_text = resources.text(1).unwrap().unwrap();
    select_with_reads(
        0x656e,
        &mut resources,
        &mut indicators,
        selection_images()
            .into_iter()
            .map(|(path, bytes)| (path, Ok(Some(bytes))))
            .collect(),
    )
    .unwrap();
    assert_database(&mut resources, true);
    assert_maps(&resources, &indicators, true, true);
    assert_eq!(issued_text.as_ref(), OLD_TEXT);
    assert_eq!(
        &resources
            .language(MenuFontLanguage::Primary)
            .unwrap()
            .raw_header[4..8],
        &NEW_PRIMARY_LANGUAGE
    );
    assert!(resources.text(0).unwrap().is_none());
    for font in 0..4 {
        assert!(resources.font(font).is_err());
    }
}

#[test]
fn all_missing_combinations_preserve_the_corresponding_existing_owners() {
    for missing_mask in 0u32..8 {
        let (mut resources, mut indicators) = populated_owners();
        let issued_text = resources.text(1).unwrap().unwrap();
        let reads = selection_images()
            .into_iter()
            .enumerate()
            .map(|(index, (path, bytes))| {
                (
                    path,
                    Ok((missing_mask & (1 << index) == 0).then_some(bytes)),
                )
            })
            .collect();
        select_with_reads(0x656e, &mut resources, &mut indicators, reads).unwrap();
        assert_database(&mut resources, missing_mask & 1 == 0);
        assert_maps(
            &resources,
            &indicators,
            missing_mask & 2 == 0,
            missing_mask & 4 == 0,
        );
        if missing_mask & 1 != 0 {
            assert!(Arc::ptr_eq(
                &issued_text,
                &resources.text(1).unwrap().unwrap()
            ));
        }
    }
}

#[test]
fn absent_images_leave_uncomposed_owners_uncomposed_without_a_default() {
    let (mut resources, mut indicators) = empty_owners();
    select_with_reads(
        0x656e,
        &mut resources,
        &mut indicators,
        vec![
            ("tables/textdb_en.def", Ok(None)),
            ("tables/lang_en.def", Ok(None)),
            ("tables/lang_cz.def", Ok(None)),
        ],
    )
    .unwrap();
    assert!(!resources.text_database().is_loaded());
    assert!(resources.text_database().is_empty());
    assert!(resources.language(MenuFontLanguage::Primary).is_err());
    assert!(resources.language(MenuFontLanguage::Credits).is_err());
    assert_eq!(
        indicators.get_language(),
        u32::from_le_bytes(OLD_INDICATOR_LANGUAGE)
    );
}

#[test]
fn missing_primary_does_not_require_a_previous_primary_map_or_block_credits() {
    let (mut resources, mut indicators) = empty_owners();
    select_with_reads(
        0x656e,
        &mut resources,
        &mut indicators,
        vec![
            (
                "tables/textdb_en.def",
                Ok(Some(database_image(NEW_TEXT, NEW_DATABASE_HEADER))),
            ),
            ("tables/lang_en.def", Ok(None)),
            (
                "tables/lang_cz.def",
                Ok(Some(language_image(*b"cz\0\0", NEW_CREDITS_GLYPH))),
            ),
        ],
    )
    .unwrap();
    assert_database(&mut resources, true);
    assert!(resources.language(MenuFontLanguage::Primary).is_err());
    assert_eq!(
        resources
            .language(MenuFontLanguage::Credits)
            .unwrap()
            .glyph(b'A'),
        NEW_CREDITS_GLYPH
    );
    assert_eq!(
        indicators.get_language(),
        u32::from_le_bytes(OLD_INDICATOR_LANGUAGE)
    );
}

#[test]
fn host_errors_stop_at_each_reached_prefix_and_do_not_replay_reads() {
    for failed_stage in 0..3 {
        let (mut resources, mut indicators) = populated_owners();
        let error = format!("selected-source read failure at stage {failed_stage}");
        let reads = selection_images()
            .into_iter()
            .enumerate()
            .take(failed_stage + 1)
            .map(|(index, (path, bytes))| {
                (
                    path,
                    if index == failed_stage {
                        Err(error.clone())
                    } else {
                        Ok(Some(bytes))
                    },
                )
            })
            .collect();
        assert_eq!(
            select_with_reads(0x656e, &mut resources, &mut indicators, reads),
            Err(error)
        );
        assert_database(&mut resources, failed_stage > 0);
        assert_maps(&resources, &indicators, failed_stage > 1, false);
    }
}

#[test]
fn malformed_database_discards_old_database_and_stops_before_map_reads() {
    let mut invalid_offset = database_image(NEW_TEXT, NEW_DATABASE_HEADER);
    invalid_offset[12..16].copy_from_slice(&0u32.to_le_bytes());
    for malformed in [Vec::new(), invalid_offset] {
        let (mut resources, mut indicators) = populated_owners();
        let issued_text = resources.text(1).unwrap().unwrap();
        let error = select_with_reads(
            0x656e,
            &mut resources,
            &mut indicators,
            vec![("tables/textdb_en.def", Ok(Some(malformed)))],
        )
        .unwrap_err();
        assert!(error.contains("TextDB"));
        assert!(!resources.text_database().is_loaded());
        assert!(resources.text_database().is_empty());
        assert_eq!(resources.text_database().unknown(), None);
        assert_eq!(resources.text(1).unwrap().unwrap().as_ref(), b" ");
        assert_eq!(issued_text.as_ref(), OLD_TEXT);
        assert_maps(&resources, &indicators, false, false);
    }
}

#[test]
fn malformed_primary_preserves_old_primary_and_indicators_after_database_install() {
    for malformed in [Vec::new(), vec![0; 568]] {
        let (mut resources, mut indicators) = populated_owners();
        let error = select_with_reads(
            0x656e,
            &mut resources,
            &mut indicators,
            vec![
                (
                    "tables/textdb_en.def",
                    Ok(Some(database_image(NEW_TEXT, NEW_DATABASE_HEADER))),
                ),
                ("tables/lang_en.def", Ok(Some(malformed))),
            ],
        )
        .unwrap_err();
        assert!(error.starts_with("menu language intake: "));
        assert_database(&mut resources, true);
        assert_maps(&resources, &indicators, false, false);
    }
}

#[test]
fn malformed_credits_preserves_old_credits_after_primary_live_language_update() {
    for malformed in [Vec::new(), vec![0; 568]] {
        let (mut resources, mut indicators) = populated_owners();
        let mut reads: Vec<ReadStep> = selection_images()
            .into_iter()
            .take(2)
            .map(|(path, bytes)| (path, Ok(Some(bytes))))
            .collect();
        reads.push(("tables/lang_cz.def", Ok(Some(malformed))));
        let error = select_with_reads(0x656e, &mut resources, &mut indicators, reads).unwrap_err();
        assert!(error.starts_with("menu language intake: "));
        assert_database(&mut resources, true);
        assert_maps(&resources, &indicators, true, false);
    }
}

#[test]
fn unsupported_codes_fail_before_reads_and_leave_existing_owners_untouched() {
    for requested_language in [
        0,
        u32::MAX,
        0x0001_656e,
        0x656e_656e,
        0x006e,
        0x6500,
        0x206e,
        0x6520,
        0x7f6e,
        0x657f,
        0x806e,
        0x65ff,
        0x2f6e,
        0x652f,
        0x5c6e,
        0x655c,
        0x3a6e,
        0x653a,
    ] {
        let (mut resources, mut indicators) = populated_owners();
        let error = select_with_reads(
            requested_language,
            &mut resources,
            &mut indicators,
            Vec::new(),
        )
        .unwrap_err();
        assert!(error.starts_with("unsupported startup menu language code "));
        assert_database(&mut resources, false);
        assert_maps(&resources, &indicators, false, false);
    }
}

#[test]
fn explicit_codes_use_high_first_suffixes_without_selecting_a_default() {
    for (requested_language, database_path, primary_path) in [
        (0x656e, "tables/textdb_en.def", "tables/lang_en.def"),
        (0x637a, "tables/textdb_cz.def", "tables/lang_cz.def"),
        (0x706c, "tables/textdb_pl.def", "tables/lang_pl.def"),
    ] {
        let (mut resources, mut indicators) = populated_owners();
        select_with_reads(
            requested_language,
            &mut resources,
            &mut indicators,
            vec![
                (database_path, Ok(None)),
                (primary_path, Ok(None)),
                ("tables/lang_cz.def", Ok(None)),
            ],
        )
        .unwrap();
        assert_database(&mut resources, false);
        assert_maps(&resources, &indicators, false, false);
    }
}

#[test]
fn czech_primary_and_credits_read_the_same_path_separately_without_caching() {
    let (mut resources, mut indicators) = populated_owners();
    select_with_reads(
        0x637a,
        &mut resources,
        &mut indicators,
        vec![
            ("tables/textdb_cz.def", Ok(None)),
            (
                "tables/lang_cz.def",
                Ok(Some(language_image(
                    NEW_PRIMARY_LANGUAGE,
                    NEW_PRIMARY_GLYPH,
                ))),
            ),
            (
                "tables/lang_cz.def",
                Ok(Some(language_image(*b"cz\0\0", NEW_CREDITS_GLYPH))),
            ),
        ],
    )
    .unwrap();
    assert_database(&mut resources, false);
    assert_maps(&resources, &indicators, true, true);
}

#[test]
fn valid_zero_entry_database_replaces_old_data_instead_of_counting_as_missing() {
    let (mut resources, mut indicators) = populated_owners();
    let mut image = vec![0; 8];
    image[4..8].copy_from_slice(&NEW_DATABASE_HEADER.to_le_bytes());
    select_with_reads(
        0x656e,
        &mut resources,
        &mut indicators,
        vec![
            ("tables/textdb_en.def", Ok(Some(image))),
            ("tables/lang_en.def", Ok(None)),
            ("tables/lang_cz.def", Ok(None)),
        ],
    )
    .unwrap();
    assert!(resources.text_database().is_loaded());
    assert!(resources.text_database().is_empty());
    assert_eq!(
        resources.text_database().unknown(),
        Some(NEW_DATABASE_HEADER)
    );
    assert_eq!(resources.text(1).unwrap().unwrap().as_ref(), b" ");
    assert_maps(&resources, &indicators, false, false);
}
