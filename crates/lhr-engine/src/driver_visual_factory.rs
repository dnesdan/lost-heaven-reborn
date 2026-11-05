#![forbid(unsafe_code)]
//! I3D_driver visual factory: CreateVisual, CreateTrifaceArray, EnumVisualTypes
//! (LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6).
//! Upstream reference mafia-re ls3df/I3D/I3d_driver.cpp @ ab67b13
//! (CreateVisual 1313-1360, CreateTrifaceArray 1384-1388, EnumVisualTypes
//! 1504-1529), visual fourccs I3D.h:85-98, result codes ls3df.h:20-35. Names
//! CreateVisual/CreateTrifaceArray/EnumVisualTypes are evidenced PS2 originals
//! (I3D_driver vtable); Rust spelling is snake_case per repo rule. Tier
//! contract (new-module wave policy: contract regardless of bodies; all three
//! LAB_ entry shapes per assignment).
//!
//! Typed factory state: visual kinds are an enum over the 12 evidenced fourcc
//! words, handles are owner-scoped typed ids, and the triface count keeps its
//! native i32 width. Operator-new bodies and the fatal handler stay behind
//! allocator traits; the invalid-type diagnostic text (which formats
//! VisualType2String) stays behind a diagnostic trait. CreateVisual performs
//! no driver-list insert. Triface arrays are caller-owned (delete[] boundary
//! is an explicit free call). EnumVisualTypes never caches across the mutating
//! callback: the nine fixed calls then the three conditional calls run in
//! native order on every invocation.

use std::sync::atomic::{AtomicU64, Ordering};

/// LS3D_RESULT words used by EnumVisualTypes.
pub const VISUAL_OK: u32 = 0;
pub const VISUAL_INVALIDPARAMS: u32 = 0x8000_0007;

/// Evidenced visual fourccs (I3D.h:85-98).
pub const VISUAL_OBJECT: u32 = 0x5f4a_424f;
pub const VISUAL_LIT_OBJECT: u32 = 0x5041_4d4c;
pub const VISUAL_BILLBOARD: u32 = 0x4452_4242;
pub const VISUAL_SINGLEMESH: u32 = 0x4d47_4e53;
pub const VISUAL_LENSF: u32 = 0x534e_454c;
pub const VISUAL_MORPH: u32 = 0x4850_524d;
pub const VISUAL_SINGLEMORPH: u32 = 0x524d_4d53;
pub const VISUAL_PROJECTOR: u32 = 0x4a4f_5250;
pub const VISUAL_PART_BASE: u32 = 0x4254_5250;
pub const VISUAL_PART_ELEMENT: u32 = 0x4554_5250;
pub const VISUAL_LANDPATCH: u32 = 0x5044_4e4c;
pub const VISUAL_MIRROR: u32 = 0x5252_494d;
pub const VISUAL_SHADOW: u32 = 0x0044_4853;

/// The 12 visual kinds constructed by CreateVisual. SHADOW is not constructed
/// here; it only appears in the EnumVisualTypes all-list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VisualKind {
    Object,
    LitObject,
    Billboard,
    Singlemesh,
    Lensflare,
    Morph,
    Singlemorph,
    Projector,
    PartBase,
    PartElement,
    Landpatch,
    Mirror,
}

impl VisualKind {
    pub fn word(self) -> u32 {
        match self {
            Self::Object => VISUAL_OBJECT,
            Self::LitObject => VISUAL_LIT_OBJECT,
            Self::Billboard => VISUAL_BILLBOARD,
            Self::Singlemesh => VISUAL_SINGLEMESH,
            Self::Lensflare => VISUAL_LENSF,
            Self::Morph => VISUAL_MORPH,
            Self::Singlemorph => VISUAL_SINGLEMORPH,
            Self::Projector => VISUAL_PROJECTOR,
            Self::PartBase => VISUAL_PART_BASE,
            Self::PartElement => VISUAL_PART_ELEMENT,
            Self::Landpatch => VISUAL_LANDPATCH,
            Self::Mirror => VISUAL_MIRROR,
        }
    }

    pub fn from_word(word: u32) -> Option<Self> {
        match word {
            VISUAL_OBJECT => Some(Self::Object),
            VISUAL_LIT_OBJECT => Some(Self::LitObject),
            VISUAL_BILLBOARD => Some(Self::Billboard),
            VISUAL_SINGLEMESH => Some(Self::Singlemesh),
            VISUAL_LENSF => Some(Self::Lensflare),
            VISUAL_MORPH => Some(Self::Morph),
            VISUAL_SINGLEMORPH => Some(Self::Singlemorph),
            VISUAL_PROJECTOR => Some(Self::Projector),
            VISUAL_PART_BASE => Some(Self::PartBase),
            VISUAL_PART_ELEMENT => Some(Self::PartElement),
            VISUAL_LANDPATCH => Some(Self::Landpatch),
            VISUAL_MIRROR => Some(Self::Mirror),
            _ => None,
        }
    }
}

/// Owned visual identity produced by the operator-new boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VisualId {
    arena: u64,
    slot: u32,
}

impl VisualId {
    pub fn fresh() -> Self {
        static NEXT_VISUAL: AtomicU64 = AtomicU64::new(1);
        let value = NEXT_VISUAL
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("visual identity exhausted");
        Self {
            arena: 1,
            slot: u32::try_from(value).expect("visual slot exhausted"),
        }
    }

    pub fn arena(self) -> u64 {
        self.arena
    }

    pub fn slot(self) -> u32 {
        self.slot
    }
}

/// Caller-owned triface array identity (array-new/delete[] boundary).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TrifaceArrayId {
    arena: u64,
    slot: u32,
}

impl TrifaceArrayId {
    pub fn fresh() -> Self {
        static NEXT_TRIFACE: AtomicU64 = AtomicU64::new(1);
        let value = NEXT_TRIFACE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .expect("triface identity exhausted");
        Self {
            arena: 1,
            slot: u32::try_from(value).expect("triface slot exhausted"),
        }
    }

    pub fn arena(self) -> u64 {
        self.arena
    }

    pub fn slot(self) -> u32 {
        self.slot
    }
}

/// Operator-new boundary for visuals, including the fatal OOM path.
pub trait VisualAllocator {
    fn allocate_visual(&mut self, kind: VisualKind) -> Option<VisualId>;
    fn out_of_memory_fatal(&mut self);
}

/// Diagnostic sink for the invalid visual-type message. Formatting (which
/// calls VisualType2String) is a backend contract.
pub trait VisualDiagnostic {
    fn invalid_visual_type(&mut self, type_word: u32);
}

/// Array-new boundary for triface arrays. The caller owns the array and frees
/// it with free_trifaces (native delete[]).
pub trait TrifaceAllocator {
    fn allocate_trifaces(&mut self, count: i32) -> Option<TrifaceArrayId>;
    fn free_trifaces(&mut self, id: TrifaceArrayId);
}

/// Enumeration callback for EnumVisualTypes. Names are the native ASCII
/// spellings without a terminator.
pub trait VisualTypeCallback {
    fn on_visual_type(&mut self, type_word: u32, name: &[u8]);
}

/// LS3DF RVA 0x18d80. Evidenced name I3D_driver::CreateVisual from mafia-re
/// I3d_driver.cpp:1313-1360. Twelve-type operator-new switch in native order;
/// an invalid word emits the diagnostic and returns NULL with no allocation;
/// a failed allocation invokes the fatal handler and returns NULL. The driver
/// performs no list insert for the new visual.
pub fn driver_create_visual(
    type_word: u32,
    diag: &mut impl VisualDiagnostic,
    alloc: &mut impl VisualAllocator,
) -> Option<VisualId> {
    let Some(kind) = VisualKind::from_word(type_word) else {
        diag.invalid_visual_type(type_word);
        return None;
    };
    match alloc.allocate_visual(kind) {
        Some(id) => Some(id),
        None => {
            alloc.out_of_memory_fatal();
            None
        }
    }
}

/// LS3DF RVA 0x19180. Evidenced name I3D_driver::CreateTrifaceArray from
/// mafia-re I3d_driver.cpp:1384-1388. Plain array-new: the count word passes
/// through untouched and the caller owns the array. No diagnostic and no
/// fatal handler exist on this path.
pub fn driver_create_triface_array(
    count: i32,
    alloc: &mut impl TrifaceAllocator,
) -> Option<TrifaceArrayId> {
    alloc.allocate_trifaces(count)
}

/// LS3DF RVA 0x19880. Evidenced name I3D_driver::EnumVisualTypes from mafia-re
/// I3d_driver.cpp:1504-1529. A null callback reports INVALIDPARAMS; otherwise
/// the nine fixed calls run in native order, then the three conditional calls
/// when all is set, and the result is OK. Each call rereads the fixed
/// word/name pair, so a mutating callback cannot observe cached entries.
pub fn driver_enum_visual_types<C: VisualTypeCallback>(proc: Option<&mut C>, all: bool) -> u32 {
    let Some(proc) = proc else {
        return VISUAL_INVALIDPARAMS;
    };
    proc.on_visual_type(VISUAL_BILLBOARD, b"Billboard");
    proc.on_visual_type(VISUAL_LIT_OBJECT, b"Lit-object");
    proc.on_visual_type(VISUAL_MORPH, b"Morph");
    proc.on_visual_type(VISUAL_OBJECT, b"Object");
    proc.on_visual_type(VISUAL_SINGLEMESH, b"Single-mesh");
    proc.on_visual_type(VISUAL_SINGLEMORPH, b"Single-morph");
    proc.on_visual_type(VISUAL_LENSF, b"Lensflare/Glow");
    proc.on_visual_type(VISUAL_PROJECTOR, b"Projector");
    proc.on_visual_type(VISUAL_MIRROR, b"Mirror");
    if all {
        proc.on_visual_type(VISUAL_PART_ELEMENT, b"Particle element");
        proc.on_visual_type(VISUAL_SHADOW, b"Shadow");
        proc.on_visual_type(VISUAL_PART_BASE, b"Part. element base");
    }
    VISUAL_OK
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAlloc {
        kinds: Vec<VisualKind>,
        fail_on: Option<VisualKind>,
        fatal: usize,
        next: u32,
    }

    impl TestAlloc {
        fn ok() -> Self {
            Self {
                kinds: Vec::new(),
                fail_on: None,
                fatal: 0,
                next: 1,
            }
        }
    }

    impl VisualAllocator for TestAlloc {
        fn allocate_visual(&mut self, kind: VisualKind) -> Option<VisualId> {
            self.kinds.push(kind);
            if self.fail_on == Some(kind) {
                return None;
            }
            let id = VisualId {
                arena: 1,
                slot: self.next,
            };
            self.next += 1;
            Some(id)
        }

        fn out_of_memory_fatal(&mut self) {
            self.fatal += 1;
        }
    }

    #[derive(Default)]
    struct TestDiag {
        invalid: Vec<u32>,
    }

    impl VisualDiagnostic for TestDiag {
        fn invalid_visual_type(&mut self, type_word: u32) {
            self.invalid.push(type_word);
        }
    }

    struct TestTrifaces {
        counts: Vec<i32>,
        fail: bool,
        freed: Vec<TrifaceArrayId>,
    }

    impl TrifaceAllocator for TestTrifaces {
        fn allocate_trifaces(&mut self, count: i32) -> Option<TrifaceArrayId> {
            self.counts.push(count);
            if self.fail {
                return None;
            }
            Some(TrifaceArrayId::fresh())
        }

        fn free_trifaces(&mut self, id: TrifaceArrayId) {
            self.freed.push(id);
        }
    }

    struct Recorder {
        entries: Vec<(u32, Vec<u8>)>,
        calls: usize,
    }

    impl VisualTypeCallback for Recorder {
        fn on_visual_type(&mut self, type_word: u32, name: &[u8]) {
            self.calls += 1;
            self.entries.push((type_word, name.to_vec()));
        }
    }

    #[test]
    fn create_visual_covers_all_twelve_kinds_in_switch_order() {
        let words = [
            (VISUAL_OBJECT, VisualKind::Object),
            (VISUAL_LIT_OBJECT, VisualKind::LitObject),
            (VISUAL_BILLBOARD, VisualKind::Billboard),
            (VISUAL_SINGLEMESH, VisualKind::Singlemesh),
            (VISUAL_LENSF, VisualKind::Lensflare),
            (VISUAL_MORPH, VisualKind::Morph),
            (VISUAL_SINGLEMORPH, VisualKind::Singlemorph),
            (VISUAL_PROJECTOR, VisualKind::Projector),
            (VISUAL_PART_BASE, VisualKind::PartBase),
            (VISUAL_PART_ELEMENT, VisualKind::PartElement),
            (VISUAL_LANDPATCH, VisualKind::Landpatch),
            (VISUAL_MIRROR, VisualKind::Mirror),
        ];
        let mut diag = TestDiag::default();
        let mut alloc = TestAlloc::ok();
        for (word, kind) in words {
            let id = driver_create_visual(word, &mut diag, &mut alloc);
            assert!(id.is_some());
            assert_eq!(alloc.kinds.last(), Some(&kind));
            assert_eq!(kind.word(), word);
            assert_eq!(VisualKind::from_word(word), Some(kind));
        }
        assert!(diag.invalid.is_empty());
        assert_eq!(alloc.fatal, 0);
        assert_eq!(alloc.kinds.len(), 12);
    }

    #[test]
    fn create_visual_invalid_reports_and_skips_allocation() {
        let mut diag = TestDiag::default();
        let mut alloc = TestAlloc::ok();
        for word in [0u32, 1, VISUAL_SHADOW, 0x1234_5678, u32::MAX] {
            assert_eq!(driver_create_visual(word, &mut diag, &mut alloc), None);
        }
        assert_eq!(
            diag.invalid,
            vec![0, 1, VISUAL_SHADOW, 0x1234_5678, u32::MAX]
        );
        assert!(alloc.kinds.is_empty());
        assert_eq!(alloc.fatal, 0);
    }

    #[test]
    fn create_visual_oom_invokes_fatal_and_returns_null() {
        let mut diag = TestDiag::default();
        let mut alloc = TestAlloc::ok();
        alloc.fail_on = Some(VisualKind::Morph);
        assert_eq!(
            driver_create_visual(VISUAL_OBJECT, &mut diag, &mut alloc),
            Some(VisualId { arena: 1, slot: 1 })
        );
        assert_eq!(
            driver_create_visual(VISUAL_MORPH, &mut diag, &mut alloc),
            None
        );
        assert_eq!(alloc.fatal, 1);
        assert!(diag.invalid.is_empty());
        assert_eq!(alloc.kinds, vec![VisualKind::Object, VisualKind::Morph]);
    }

    #[test]
    fn create_triface_array_forwards_count_and_caller_owns() {
        let mut alloc = TestTrifaces {
            counts: Vec::new(),
            fail: false,
            freed: Vec::new(),
        };
        for count in [0, 1, 16, 1024] {
            let id = driver_create_triface_array(count, &mut alloc);
            assert!(id.is_some());
            alloc.free_trifaces(id.unwrap());
        }
        assert_eq!(alloc.counts, vec![0, 1, 16, 1024]);
        assert_eq!(alloc.freed.len(), 4);
        alloc.fail = true;
        assert_eq!(driver_create_triface_array(8, &mut alloc), None);
        assert_eq!(alloc.counts.last(), Some(&8));
    }

    #[test]
    fn enum_visual_types_null_nine_and_twelve_in_native_order() {
        let mut recorder = Recorder {
            entries: Vec::new(),
            calls: 0,
        };
        assert_eq!(
            driver_enum_visual_types(None::<&mut Recorder>, false),
            VISUAL_INVALIDPARAMS
        );
        assert_eq!(
            driver_enum_visual_types(None::<&mut Recorder>, true),
            VISUAL_INVALIDPARAMS
        );
        assert_eq!(
            driver_enum_visual_types(Some(&mut recorder), false),
            VISUAL_OK
        );
        assert_eq!(
            recorder
                .entries
                .iter()
                .map(|entry| entry.0)
                .collect::<Vec<_>>(),
            vec![
                VISUAL_BILLBOARD,
                VISUAL_LIT_OBJECT,
                VISUAL_MORPH,
                VISUAL_OBJECT,
                VISUAL_SINGLEMESH,
                VISUAL_SINGLEMORPH,
                VISUAL_LENSF,
                VISUAL_PROJECTOR,
                VISUAL_MIRROR,
            ]
        );
        assert_eq!(
            recorder
                .entries
                .iter()
                .map(|entry| entry.1.clone())
                .collect::<Vec<_>>(),
            vec![
                b"Billboard".to_vec(),
                b"Lit-object".to_vec(),
                b"Morph".to_vec(),
                b"Object".to_vec(),
                b"Single-mesh".to_vec(),
                b"Single-morph".to_vec(),
                b"Lensflare/Glow".to_vec(),
                b"Projector".to_vec(),
                b"Mirror".to_vec(),
            ]
        );
        recorder.entries.clear();
        assert_eq!(
            driver_enum_visual_types(Some(&mut recorder), true),
            VISUAL_OK
        );
        assert_eq!(recorder.entries.len(), 12);
        assert_eq!(
            &recorder.entries[9..],
            &[
                (VISUAL_PART_ELEMENT, b"Particle element".to_vec()),
                (VISUAL_SHADOW, b"Shadow".to_vec()),
                (VISUAL_PART_BASE, b"Part. element base".to_vec()),
            ]
        );
        let before = recorder.calls;
        assert_eq!(
            driver_enum_visual_types(Some(&mut recorder), false),
            VISUAL_OK
        );
        assert_eq!(recorder.calls, before + 9, "no cached entries reused");
    }
}
