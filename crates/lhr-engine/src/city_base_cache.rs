//! Session-owned city base-block cache (`cache.bin` intake target).
//!
//! Reference: mafia-re `113b5b6`, game/mission/m_cache.h + m_cache.cpp
//! (PC file 0x401030–0x404cdc; PS2 m_cache.cpp correspondence). Layouts are
//! PS2-attributed with PC sizes: `S_cache_base_block` 0x74 bytes,
//! `S_cache_model_inbb` 0x44 bytes (model-set index u16 at +0x2),
//! `S_cache_model_set` 0x14 bytes, `S_cache_model_one` 8 bytes (frame word
//! at +0x0 plus 4 unknown bytes). Field names stay descriptive; only the
//! offsets above plus constructor/Close effects are ported.
//!
//! Ported effects: constructor 0x401070 (`open = false`, `unknown_28 = 0`,
//! `unknown_20` untouched) and `Close` 0x4011f0 (`open = false` first, then
//! `blocks.clear`, then model sets torn down in reverse order with names
//! released and frames unlinked/released before `model_sets.clear`).
//! `Open` 0x4013f0 (which closes first), `Tick` 0x4020b0 and
//! `VisibleAllFrames` 0x402dc0 remain explicit boundaries. Native frame
//! words are presence flags here, never host pointers; unlink/release of
//! live frames stays with the scene owner.
#![forbid(unsafe_code)]

/// `C_cache_base_block::Open` entry for the explicit boundary.
pub const BASE_CACHE_OPEN_ENTRY: u32 = 0x4013f0;
/// `C_cache_base_block::Tick` entry for the explicit boundary.
pub const BASE_CACHE_TICK_ENTRY: u32 = 0x4020b0;
/// `C_cache_base_block::VisibleAllFrames` entry for the explicit boundary.
pub const BASE_CACHE_VISIBLE_ALL_ENTRY: u32 = 0x402dc0;
/// Successful-`Open` marker for `unknown_28` (native epilogue constant).
pub const BASE_CACHE_OPEN_UNKNOWN_28: i32 = 0x0FFF_FFFF;

/// Native `Open` return codes recovered from the 0x4013f0 body: 0 success,
/// 1 bad root/version, 2 non-chunk input, 3 model load failure, 4 unknown
/// model kind (with the "Chybí model" message box).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseCacheOpenOutcome {
    Ok = 0,
    BadRootOrVersion = 1,
    NonChunkInput = 2,
    ModelLoadFailed = 3,
    ModelKindUnknown = 4,
}

/// Cache model filename the native `Open` rewrites in place: a
/// case-insensitive `__strcmpi` match of `0rek23.i3d` (bytes at 0x62e460)
/// is overwritten with `0rek17.i3d` (bytes at 0x62e454) before the
/// model-kind check. Returns the replacement when the rule fires.
pub fn rewrite_unsupported_cache_model(name: &str) -> Option<&'static str> {
    if name.eq_ignore_ascii_case("0rek23.i3d") {
        Some("0rek17.i3d")
    } else {
        None
    }
}

/// One placed model reference (PC `S_cache_model_inbb`, 0x44 bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheModelPlacement {
    pub unknown_0: [u8; 2],
    /// Model-set index at +0x2.
    pub model_set: u16,
    pub unknown_4: [u8; 0x40],
}

/// One model-set element (PC `S_cache_model_one`, 8 bytes). The native frame
/// word is a presence flag; live-frame unlink/release is the scene owner's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheModelOne {
    pub frame_present: bool,
    pub unknown_4: [u8; 4],
}

/// One model set (PC `S_cache_model_set`): file-name bytes plus elements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheModelSet {
    pub name: Vec<u8>,
    pub ones: Vec<CacheModelOne>,
}

/// One base block (PC `S_cache_base_block`, 0x74 bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheBaseBlock {
    pub unknown_0: i32,
    pub unknown_4: u8,
    pub values: Vec<u32>,
    pub unknown_18: u32,
    pub unknown_1c: u32,
    pub unknown_20: u32,
    pub pairs_a: [[u8; 8]; 4],
    pub pairs_b: [[u8; 8]; 4],
    pub models: Vec<CacheModelPlacement>,
}

/// Destructor accounting for one `Close` pass.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BaseCacheCloseReport {
    pub names_released: usize,
    pub ones_released: usize,
    pub frames_present: usize,
}

/// Still-unimplemented base-cache operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BaseCacheError {
    Unsupported { entry: u32 },
}

/// Host interruption of the staged `Open` intake, distinct from the native
/// `Open` outcome codes (which are `Ok` results, not host errors).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BaseCacheOpenError {
    /// Empty input has no root chunk to walk; the already-closed state is
    /// preserved. This is a host admission boundary, not native code 1..4.
    EmptyInput,
}

/// Owned city base-block cache (PC `C_cache_base_block`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CityBaseCache {
    blocks: Vec<CacheBaseBlock>,
    model_sets: Vec<CacheModelSet>,
    unknown_20: u32,
    open: bool,
    unknown_28: i32,
    unknown_2c: [u8; 12],
}

impl CityBaseCache {
    /// Original constructor 0x401070: `open = false`, `unknown_28 = 0`.
    /// `unknown_20` is left untouched by the native body, so it arrives as
    /// an explicit retained input.
    pub fn new(retained_unknown_20: u32) -> Self {
        Self {
            blocks: Vec::new(),
            model_sets: Vec::new(),
            unknown_20: retained_unknown_20,
            open: false,
            unknown_28: 0,
            unknown_2c: [0u8; 12],
        }
    }

    /// Whether the blocks are open (native `IsOpen` copy).
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Host admission query over this owner, not another original function.
    /// Close removes live entries but retains the successful-Open epilogue
    /// marker; a closed cache can therefore still belong to a prior attempt.
    pub(crate) fn has_opened_or_retained_data(&self) -> bool {
        self.open || !self.blocks.is_empty() || !self.model_sets.is_empty() || self.unknown_28 != 0
    }

    pub fn unknown_20(&self) -> u32 {
        self.unknown_20
    }

    pub fn unknown_28(&self) -> i32 {
        self.unknown_28
    }

    pub fn unknown_2c(&self) -> &[u8; 12] {
        &self.unknown_2c
    }

    pub fn blocks(&self) -> &[CacheBaseBlock] {
        &self.blocks
    }

    pub fn model_sets(&self) -> &[CacheModelSet] {
        &self.model_sets
    }

    /// Test-only intake for teardown tests; production intake arrives with
    /// `Open` and its scene owner.
    #[cfg(test)]
    pub(crate) fn stage_for_tests(
        &mut self,
        blocks: Vec<CacheBaseBlock>,
        model_sets: Vec<CacheModelSet>,
    ) {
        self.blocks = blocks;
        self.model_sets = model_sets;
    }

    /// Original `Close` 0x4011f0: `open = false` first, `blocks.clear`,
    /// then model sets in reverse order (names released, elements in
    /// reverse with live frames counted for the scene owner), then
    /// `model_sets.clear`. Frame unlink/release itself stays with the
    /// scene owner; the report carries the counts it must perform.
    pub fn close(&mut self) -> BaseCacheCloseReport {
        self.open = false;
        self.blocks.clear();
        let mut report = BaseCacheCloseReport::default();
        for set in self.model_sets.iter().rev() {
            report.names_released += 1;
            for one in set.ones.iter().rev() {
                report.ones_released += 1;
                if one.frame_present {
                    report.frames_present += 1;
                }
            }
        }
        self.model_sets.clear();
        report
    }

    /// Original `Open` 0x4013f0 (which closes first): unimplemented pending
    /// the scene-integrated port; recorded here so callers cannot mistake
    /// intake for done.
    pub fn open(&mut self) -> Result<(), BaseCacheError> {
        Err(BaseCacheError::Unsupported {
            entry: BASE_CACHE_OPEN_ENTRY,
        })
    }

    /// Staged `Open` 0x4013f0 intake over caller-owned `cache.bin` bytes,
    /// without the scene owner. The native body closes first; this does the
    /// same, so every return preserves the closed prefix. Reference: mafia-re
    /// `113b5b6`, game/mission/m_cache.cpp `Open` stub called by
    /// `C_mission::Open` (PC `C_mission::Open` 0x5409d0); REA Ghidra
    /// 0x4013f0 chunk walk 500/1 -> 1000 -> 2000 matches
    /// `lhr_assets::parse_city_cache`, with the case-insensitive
    /// `0rek23.i3d` -> `0rek17.i3d` rewrite and the success epilogue
    /// (`unknown_20 = 0`, `open = true`, `unknown_28 = 0x0FFFFFFF` plus 12
    /// opaque helper bytes). Parse failures are native outcome codes
    /// (`BadRootOrVersion`/`NonChunkInput`), not host errors; the closed
    /// state is preserved. Model loading and live-frame wiring stay with
    /// the scene owner: staged placements use `frame_present = false`,
    /// `unknown_0` carries the part index as a host guess (native epilogue
    /// bytes unknown), and the trailer is a host zero placeholder pending
    /// the scene-integrated port. Frames are unwired here by design. This
    /// is staged intake filed under the 0x4013f0 entry, not the full native
    /// `Open` with scene wiring. This is intake, not gameplay.
    pub fn open_from_bytes(
        &mut self,
        bytes: &[u8],
    ) -> Result<BaseCacheOpenOutcome, BaseCacheOpenError> {
        self.close();
        if bytes.is_empty() {
            return Err(BaseCacheOpenError::EmptyInput);
        }
        let cache = match lhr_assets::parse_city_cache(bytes) {
            Ok(cache) => cache,
            Err(error) => {
                // Only the exact version-1 root error maps to
                // BadRootOrVersion; every other intake failure (quota, NUL,
                // truncation, short reads) is NonChunkInput. Mapped strings
                // are listed here so the mapping stays exact: "expected city
                // cache version 1" alone takes the version arm.
                if error.to_string() == "expected city cache version 1" {
                    return Ok(BaseCacheOpenOutcome::BadRootOrVersion);
                }
                return Ok(BaseCacheOpenOutcome::NonChunkInput);
            }
        };
        let mut distinct: std::collections::BTreeMap<String, u16> =
            std::collections::BTreeMap::new();
        let mut sets: Vec<CacheModelSet> = Vec::new();
        for part in &cache.parts {
            for object in &part.objects {
                let rewritten = rewrite_unsupported_cache_model(&object.model)
                    .unwrap_or(object.model.as_str())
                    .to_owned();
                if !distinct.contains_key(&rewritten) {
                    // The parser caps objects at 65535, so distinct sets can
                    // never exceed the u16 placement index (dead budget arm
                    // removed; a breach here is an internal invariant break).
                    let index =
                        u16::try_from(distinct.len()).expect("parser quota bounds distinct sets");
                    distinct.insert(rewritten.clone(), index);
                    sets.push(CacheModelSet {
                        name: rewritten.into_bytes(),
                        ones: Vec::new(),
                    });
                }
            }
        }
        let mut blocks: Vec<CacheBaseBlock> = Vec::new();
        for (part_index, part) in cache.parts.iter().enumerate() {
            let mut models: Vec<CacheModelPlacement> = Vec::new();
            for object in &part.objects {
                let rewritten =
                    rewrite_unsupported_cache_model(&object.model).unwrap_or(object.model.as_str());
                let index = *distinct.get(rewritten).expect("staged distinct model");
                models.push(CacheModelPlacement {
                    unknown_0: [0, 0],
                    model_set: index,
                    unknown_4: [0; 0x40],
                });
                let set = &mut sets[usize::from(index)];
                set.ones.push(CacheModelOne {
                    frame_present: false,
                    unknown_4: [0; 4],
                });
            }
            blocks.push(CacheBaseBlock {
                unknown_0: part_index as i32,
                unknown_4: 0,
                values: Vec::new(),
                unknown_18: 0,
                unknown_1c: 0,
                unknown_20: 0,
                pairs_a: [[0; 8]; 4],
                pairs_b: [[0; 8]; 4],
                models,
            });
        }
        self.blocks = blocks;
        self.model_sets = sets;
        self.apply_open_success([0; 12]);
        Ok(BaseCacheOpenOutcome::Ok)
    }

    /// Successful-`Open` epilogue from the 0x4013f0 tail: `unknown_20 = 0`,
    /// `open = true`, `unknown_28 = 0x0FFFFFFF`, plus the 12 opaque bytes
    /// the `FUN_00402a30(0, 0xc47a0000, 0)` helper yields into +0x2C/+0x30/
    /// +0x34. The chunk walk, model loading, and frame wiring that precede
    /// it need the scene owner and stay open.
    pub fn apply_open_success(&mut self, trailer: [u8; 12]) {
        self.unknown_20 = 0;
        self.open = true;
        self.unknown_28 = BASE_CACHE_OPEN_UNKNOWN_28;
        self.unknown_2c = trailer;
    }

    /// Original `Tick` 0x4020b0: unimplemented.
    pub fn tick(&mut self, _time: i32) -> Result<(), BaseCacheError> {
        Err(BaseCacheError::Unsupported {
            entry: BASE_CACHE_TICK_ENTRY,
        })
    }

    /// Original `VisibleAllFrames` 0x402dc0: unimplemented.
    pub fn visible_all_frames(&mut self) -> Result<(), BaseCacheError> {
        Err(BaseCacheError::Unsupported {
            entry: BASE_CACHE_VISIBLE_ALL_ENTRY,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> CacheBaseBlock {
        CacheBaseBlock {
            unknown_0: 7,
            unknown_4: 1,
            values: vec![11, 22],
            unknown_18: 1,
            unknown_1c: 2,
            unknown_20: 3,
            pairs_a: [[0xaa; 8]; 4],
            pairs_b: [[0xbb; 8]; 4],
            models: vec![CacheModelPlacement {
                unknown_0: [1, 2],
                model_set: 0,
                unknown_4: [3; 0x40],
            }],
        }
    }

    fn set(name: &[u8], frames: &[bool]) -> CacheModelSet {
        CacheModelSet {
            name: name.to_vec(),
            ones: frames
                .iter()
                .map(|present| CacheModelOne {
                    frame_present: *present,
                    unknown_4: [0xcc; 4],
                })
                .collect(),
        }
    }

    #[test]
    fn constructor_clears_open_and_leaves_unknown_20() {
        let cache = CityBaseCache::new(0x1234_5678);
        assert!(!cache.is_open());
        assert_eq!(cache.unknown_20(), 0x1234_5678);
        assert_eq!(cache.unknown_28(), 0);
        assert!(cache.blocks().is_empty());
        assert!(cache.model_sets().is_empty());
    }

    #[test]
    fn close_clears_open_first_and_tears_down_in_reverse() {
        let mut cache = CityBaseCache::new(0);
        cache.stage_for_tests(
            vec![block(), block()],
            vec![set(b"a.i3d", &[true, false]), set(b"b.i3d", &[true])],
        );
        let report = cache.close();
        assert!(!cache.is_open());
        assert!(cache.blocks().is_empty());
        assert!(cache.model_sets().is_empty());
        assert_eq!(
            report,
            BaseCacheCloseReport {
                names_released: 2,
                ones_released: 3,
                frames_present: 2,
            }
        );
        // Second close is a no-op success with empty accounting.
        assert_eq!(cache.close(), BaseCacheCloseReport::default());
    }

    #[test]
    fn unimplemented_operations_carry_their_entries() {
        let mut cache = CityBaseCache::new(0);
        assert_eq!(
            cache.open(),
            Err(BaseCacheError::Unsupported {
                entry: BASE_CACHE_OPEN_ENTRY
            })
        );
        assert_eq!(BASE_CACHE_OPEN_ENTRY, 0x4013f0);
        assert_eq!(
            cache.tick(100),
            Err(BaseCacheError::Unsupported {
                entry: BASE_CACHE_TICK_ENTRY
            })
        );
        assert_eq!(
            cache.visible_all_frames(),
            Err(BaseCacheError::Unsupported {
                entry: BASE_CACHE_VISIBLE_ALL_ENTRY
            })
        );
    }

    #[test]
    fn unsupported_model_rewrite_matches_native_strcmpi_rule() {
        assert_eq!(
            rewrite_unsupported_cache_model("0rek23.i3d"),
            Some("0rek17.i3d")
        );
        assert_eq!(
            rewrite_unsupported_cache_model("0REK23.I3D"),
            Some("0rek17.i3d")
        );
        assert_eq!(rewrite_unsupported_cache_model("0rek17.i3d"), None);
        assert_eq!(rewrite_unsupported_cache_model("car.i3d"), None);
        assert_eq!(rewrite_unsupported_cache_model(""), None);
    }

    #[test]
    fn open_success_epilogue_sets_evidence_markers() {
        let mut cache = CityBaseCache::new(0xdead_beef);
        cache.apply_open_success([0x11; 12]);
        assert!(cache.is_open());
        assert_eq!(cache.unknown_20(), 0);
        assert_eq!(cache.unknown_28(), BASE_CACHE_OPEN_UNKNOWN_28);
        assert_eq!(cache.unknown_28(), 0x0FFF_FFFF);
        assert_eq!(cache.unknown_2c(), &[0x11; 12]);
        assert_eq!(BaseCacheOpenOutcome::Ok as u32, 0);
        assert_eq!(BaseCacheOpenOutcome::BadRootOrVersion as u32, 1);
        assert_eq!(BaseCacheOpenOutcome::NonChunkInput as u32, 2);
        assert_eq!(BaseCacheOpenOutcome::ModelLoadFailed as u32, 3);
        assert_eq!(BaseCacheOpenOutcome::ModelKindUnknown as u32, 4);
    }

    fn cache_bytes(models: &[&str]) -> Vec<u8> {
        fn chunk(kind: u16, body: &[u8]) -> Vec<u8> {
            [
                kind.to_le_bytes().as_slice(),
                &((body.len() + 6) as u32).to_le_bytes(),
                body,
            ]
            .concat()
        }
        fn text(value: &str) -> Vec<u8> {
            [
                (value.len() as u32).to_le_bytes().as_slice(),
                value.as_bytes(),
            ]
            .concat()
        }
        let mut frames = Vec::new();
        for model in models {
            let mut body = text(model);
            for value in [1f32, 2., 3., 1., 0., 0., 0., 1., 2., 1.] {
                body.extend(value.to_le_bytes());
            }
            body.extend(7u32.to_le_bytes());
            for value in [2f32, 3., 4.] {
                body.extend(value.to_le_bytes());
            }
            frames.extend(chunk(2000, &body));
        }
        let mut part = text("district");
        part.extend([0; 19 * 4]);
        part.extend(frames);
        let mut root = 1u32.to_le_bytes().to_vec();
        root.extend(chunk(1000, &part));
        chunk(500, &root)
    }

    #[test]
    fn staged_open_closes_first_stages_blocks_and_applies_rewrite() {
        let mut cache = CityBaseCache::new(0xdead_beef);
        cache.stage_for_tests(vec![block()], vec![set(b"old.i3d", &[true])]);
        let bytes = cache_bytes(&["0REK23.I3D", "car.i3d", "car.i3d"]);
        assert_eq!(cache.open_from_bytes(&bytes), Ok(BaseCacheOpenOutcome::Ok));
        assert!(cache.is_open());
        assert_eq!(cache.unknown_20(), 0);
        assert_eq!(cache.unknown_28(), BASE_CACHE_OPEN_UNKNOWN_28);
        assert_eq!(cache.blocks().len(), 1);
        assert_eq!(cache.blocks()[0].models.len(), 3);
        assert_eq!(cache.model_sets().len(), 2);
        assert_eq!(cache.model_sets()[0].name, b"0rek17.i3d");
        assert_eq!(cache.model_sets()[1].name, b"car.i3d");
        assert_eq!(cache.model_sets()[0].ones.len(), 1);
        assert_eq!(cache.model_sets()[1].ones.len(), 2);
        assert!(cache
            .model_sets()
            .iter()
            .flat_map(|set| &set.ones)
            .all(|one| !one.frame_present));
        let report = cache.close();
        assert!(!cache.is_open());
        assert!(cache.blocks().is_empty());
        assert!(cache.model_sets().is_empty());
        assert_eq!(report.names_released, 2);
        assert_eq!(report.ones_released, 3);
        assert_eq!(report.frames_present, 0);
        assert_eq!(cache.open_from_bytes(&bytes), Ok(BaseCacheOpenOutcome::Ok));
        assert!(cache.is_open());
    }

    #[test]
    fn staged_open_preserves_closed_prefix_on_empty_and_bad_inputs() {
        let mut cache = CityBaseCache::new(0x1234);
        cache.stage_for_tests(vec![block()], vec![set(b"old.i3d", &[false])]);
        assert_eq!(
            cache.open_from_bytes(&[]),
            Err(BaseCacheOpenError::EmptyInput)
        );
        assert!(!cache.is_open());
        assert!(cache.blocks().is_empty());
        assert!(cache.model_sets().is_empty());
        cache.stage_for_tests(vec![block()], vec![set(b"old.i3d", &[false])]);
        assert_eq!(
            cache.open_from_bytes(&[1, 2, 3]),
            Ok(BaseCacheOpenOutcome::NonChunkInput)
        );
        assert!(!cache.is_open());
        assert!(cache.blocks().is_empty());
        let mut bad = cache_bytes(&["car.i3d"]);
        bad[6] = 2;
        assert_eq!(
            CityBaseCache::new(0).open_from_bytes(&bad),
            Ok(BaseCacheOpenOutcome::BadRootOrVersion)
        );
    }

    #[test]
    fn staged_open_u16_budget_held_by_parser_quota() {
        // The parser caps objects at 65535, so 65535 distinct names still
        // fit the u16 index and the budget arm is dead: full intake Ok.
        let names: Vec<String> = (0..65535u32).map(|n| format!("m{n:05}.i3d")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let bytes = cache_bytes(&refs);
        let mut cache = CityBaseCache::new(0);
        assert_eq!(cache.open_from_bytes(&bytes), Ok(BaseCacheOpenOutcome::Ok));
        assert_eq!(cache.model_sets().len(), 65535);
    }
}
