//! I3D_dummy's real constructor/local box in the canonical frame arena.
//! PC LS3DF e211c850... RVA1a1e0; Game08398317... helper546d10.
//! The Game box helper copies six DWORDs only: it does not normalize, update
//! transforms, change cache flags or write the base hierarchical bound volume.
use crate::{
    frame_transform::{FrameKey, FrameKind, I3dFrameState},
    model_loading::{ModelArena, ModelError},
    native_transform::{SMatrix, SQuat},
    scene_weather_dummy::WEATHER_FLAG,
};

/// DUMMYF_INV_MAT_VALID 0x10 (mafia-re ls3df/I3D/I3D.h:1824 at pin 113b5b6):
/// inv_matrix is up to date (PC only); cleared when the frame moves or is
/// normalized, set by GetInvMatrix.
pub const DUMMYF_INV_MAT_VALID: u32 = 0x10;

/// Upstream working name, not an evidenced original symbol: direct read of
/// the DUMMYF_INV_MAT_VALID bit (I3D.h:1824) that GetInvMatrix gates on
/// (I3D.h:1852; mafia-re ls3df/I3D/I3D.h at pin 113b5b6). The original
/// exposes no named accessor: Normalize and GetInvMatrix test and store the
/// bit in place.
pub fn dummy_inv_matrix_valid(dummy_flags: u32) -> bool {
    dummy_flags & DUMMYF_INV_MAT_VALID != 0
}

/// DUMMYF_NORMALIZED 0x1 (mafia-re ls3df/I3D/I3D.h:1822): Normalize mapped
/// the box to (0, 0, 0)-(1, 1, 1).
pub const DUMMYF_NORMALIZED: u32 = 0x01;
/// DUMMYF_NORMALIZED_CENTER 0x2 (mafia-re ls3df/I3D/I3D.h:1823): Normalize
/// mapped the box to (-1, -1, -1)-(1, 1, 1).
pub const DUMMYF_NORMALIZED_CENTER: u32 = 0x02;

/// Upstream working name, not an evidenced original symbol: direct read of
/// the DUMMYF_NORMALIZED bit. The reference records no LS3DF FUNCTION
/// address for a standalone reader (I3D.h:1822; set by `Normalize`, mafia-re
/// ls3df/I3D/frames/I3d_dummy.cpp:62-69). Reports the (0, 0, 0)-(1, 1, 1)
/// normalization; no state moves.
pub fn dummy_is_normalized(dummy_flags: u32) -> bool {
    dummy_flags & DUMMYF_NORMALIZED != 0
}

/// Upstream working name, not an evidenced original symbol: direct read of
/// the DUMMYF_NORMALIZED_CENTER bit. The reference records no LS3DF FUNCTION
/// address for a standalone reader (I3D.h:1823; set by `Normalize`, mafia-re
/// ls3df/I3D/frames/I3d_dummy.cpp:52-60). Reports the centered (-1, -1,
/// -1)-(1, 1, 1) normalization; no state moves.
pub fn dummy_is_center_normalized(dummy_flags: u32) -> bool {
    dummy_flags & DUMMYF_NORMALIZED_CENTER != 0
}

/// Working name for the `Normalize` mode gate (mafia-re
/// ls3df/I3D/frames/I3d_dummy.cpp:48-71 at pin 113b5b6: center limb for
/// mode 2, min-corner limb otherwise). The reference records no LS3DF
/// FUNCTION address for a standalone gate reader; the CMP/mode,2/JNZ branch
/// lives inside the occupied `Normalize` body (PC RVA 0x1a350, already
/// ported, not claimed here). Reports whether the centered (-1,-1,-1) to
/// (1,1,1) limb applies; no state moves.
pub fn dummy_normalize_mode_is_center(mode: u32) -> bool {
    mode == 2
}

impl ModelArena {
    pub fn create_dummy_frame(&mut self) -> Result<FrameKey, ModelError> {
        self.insert(
            I3dFrameState::dummy(),
            None,
            Vec::new(),
            Vec::new(),
            Some(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR.bbox),
        )
    }
    pub fn dummy_bound_box(&self, key: FrameKey) -> Result<[[f32; 3]; 2], ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        self.frame(key)?
            .dummy_bounds
            .ok_or(ModelError::DummyFrameRequired(key))
    }
    /// Game546d10's six raw stores. Nonfinite bit patterns are retained just as
    /// ordinary file-loaded dummy bounds; consumers own their numeric domains.
    pub fn set_dummy_bound_box(
        &mut self,
        key: FrameKey,
        bounds: [[f32; 3]; 2],
    ) -> Result<(), ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        let index = self.frame_index(key)?;
        self.frames[index].dummy_bounds = Some(bounds);
        Ok(())
    }
    /// Original Duplicate, PC RVA 0x1a2e0 (contract: entry/mid/byte probes,
    /// no Ghidra body; mafia-re I3d_dummy.cpp:27-36). Upstream working name,
    /// not an evidenced original symbol. Order-exact: null limb (typed keys
    /// cannot spell null — UnknownFrame is its analogue) -> derived bbox
    /// copy for Dummy-kind sources -> common duplicate_frame tail.
    pub fn duplicate_dummy_frame(
        &mut self,
        dst: FrameKey,
        src: FrameKey,
    ) -> Result<(), ModelError> {
        self.frame(src).map_err(|_| ModelError::UnknownFrame(src))?;
        self.frame(dst).map_err(|_| ModelError::UnknownFrame(dst))?;
        if self.state(src)?.kind == FrameKind::Dummy {
            let bounds = self.dummy_bound_box(src)?;
            if self.state(dst)?.kind == FrameKind::Dummy {
                let di = self.frame_index(dst)?;
                self.frames[di].dummy_bounds = Some(bounds);
            }
        }
        self.duplicate_frame(dst, src)
    }
    /// Original Normalize, PC RVA 0x1a350 (contract: mode-CMP + branch
    /// probes, no Ghidra body; mafia-re I3d_dummy.cpp:48-71). I3D.h:1813-1815
    /// bits: NORMALIZED 0x1, NORMALIZED_CENTER 0x2, INV_MAT_VALID 0x10;
    /// I3DDUMMY_NORMALIZE_CENTER 2 (I3D.h:1819). Center limb (mode==2):
    /// pos=(max+min)*0.5, size=(max-min)*0.5, box=±1,
    /// flags=(flags&!(INV|NORM))|CENTER. Min-corner limb (else, matching CMP
    /// mode,2/JNZ): pos=min, size=max-min, min=0/max=1,
    /// flags=(flags&!(INV|CENTER))|NORM. Host f32 round-to-nearest-even, no
    /// FMA; *0.5/±1.0/0.0 exact; every intermediate finiteness-checked, first
    /// failure returns NumericDomain with preceding stores retained. The
    /// SetPos/SetScale transform writes (pos*Local, size*scale) need matrix
    /// reads plus a scale setter the graph does not expose — caller boundary
    /// (disclosed); bounds + flags are the durable port.
    pub fn normalize_dummy_bounds(&mut self, key: FrameKey, mode: u32) -> Result<(), ModelError> {
        use crate::native_matrix::{
            checked_f32_add, checked_f32_mul, checked_f32_sub, MatrixArithmeticUnavailable,
        };
        fn numeric<T>(value: Result<T, MatrixArithmeticUnavailable>) -> Result<T, ModelError> {
            value.map_err(|_| {
                ModelError::Frame(crate::frame_transform::FrameError::NumericDomain(
                    "Normalize",
                ))
            })
        }
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        let [min, max] = self.dummy_bound_box(key)?;
        if mode == 2 {
            for axis in 0..3 {
                numeric(checked_f32_mul(
                    numeric(checked_f32_add(max[axis], min[axis]))?,
                    0.5,
                ))?;
                numeric(checked_f32_mul(
                    numeric(checked_f32_sub(max[axis], min[axis]))?,
                    0.5,
                ))?;
            }
            self.set_dummy_bound_box(key, [[-1.; 3], [1.; 3]])?;
            self.graph.change_dummy_flags(key, 0x10 | 0x1, 0x2)?;
        } else {
            for axis in 0..3 {
                numeric(checked_f32_sub(max[axis], min[axis]))?;
            }
            self.set_dummy_bound_box(key, [[0.; 3], [1.; 3]])?;
            self.graph.change_dummy_flags(key, 0x10 | 0x2, 0x1)?;
        }
        Ok(())
    }
    /// Original I3D_dummy::~I3D_dummy, PC RVA 0x1a260 (already ledger-credited,
    /// frozen port `LS3DF bounded base, dummy and unbound Joint destruction`:
    /// NO overlay entry here; probe-verified formed Ghidra body, confirm-only;
    /// mafia-re ls3df/I3D/frames/I3d_dummy.cpp:19). Derived-only weather
    /// deregistration: when FRMFLAGS_WEATHER (I3D.h:288, 0x80000000) is set,
    /// the last registry entry per scene is removed in reverse-scan order
    /// and the flag is cleared, matching `WeatherSystemDelDummy`
    /// (I3d_scene.cpp:1072: reverse scan, erase, break). A clear flag is a
    /// no-op. Kind-guarded;
    /// hierarchy, names and the base frame destructor remain caller-owned.
    pub fn destroy_dummy_frame(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        if self.state(key)?.flags & WEATHER_FLAG == 0 {
            return Ok(());
        }
        for scene in &mut self.scenes {
            if let Some(index) = scene
                .weather_dummies
                .iter()
                .rposition(|entry| entry.dummy == key)
            {
                scene.weather_dummies.remove(index);
            }
        }
        self.graph.change_flags(key, WEATHER_FLAG, 0)?;
        Ok(())
    }
    /// Original I3D_dummy::LoadImage, PC RVA 0x1a6a0 (already ledger-credited,
    /// frozen port `LS3DF parsed model ownership and frame materialization`:
    /// NO overlay entry here; probe-verified formed Ghidra body, confirm-only;
    /// mafia-re ls3df/I3D/frames/I3d_dummy.cpp:75). Order-exact: common
    /// I3D_frame::LoadImage field stores first, then the 24-byte bbox read
    /// into the derived dummy bounds. Raw float bits are preserved; name,
    /// property and allocation effects remain loader-owned.
    pub fn apply_dummy_load_image_fields(
        &mut self,
        key: FrameKey,
        position: [f32; 3],
        scale: [f32; 3],
        rotation: SQuat,
        raw_logic_flags: u8,
        bbox: [[f32; 3]; 2],
    ) -> Result<(), ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        self.graph.apply_common_load_image_fields(
            key,
            position,
            scale,
            rotation,
            raw_logic_flags,
        )?;
        let index = self.frame_index(key)?;
        self.frames[index].dummy_bounds = Some(bbox);
        Ok(())
    }
    /// Original I3D_dummy::GetInvMatrix, PC RVA 0x68840 (differential: REA
    /// formed Ghidra body FUN_10068840 5BB/60B/18insn; mafia-re
    /// ls3df/I3D/I3D.h:1850, original Insanity3D-sourced name, not a working
    /// name; render/T_l.cpp:6 copy-ordering caveat: the T_l.cpp copy sits after
    /// EnvProcess, its first caller, while the original sits at the file end).
    /// Order-exact: dummy_flags&0x10 (DUMMYF_INV_MAT_VALID) lazy gate returns
    /// the cache, else sets 0x10 first, then the 0x20-gated UpdateWMatrixProc
    /// path (RVA 0x1c7d0) via world_matrix, then S_matrix::Inverse (RVA
    /// 0x2d0b0, reused native_matrix::inverse), returning the cache (native
    /// this+0x158). The native Inverse boolean is discarded: a singular world
    /// yields the identity, matching LinkTo. Flag stores use the existing
    /// change_dummy_flags handling. Float policy: world and inverse run under
    /// the guarded PC24/RN policy; an unavailable world or inverse retains
    /// the already-set flag and marks the interrupted cache unavailable, so a
    /// later read reports the same NumericDomain instead of a partial matrix.
    pub fn dummy_inv_matrix(&mut self, key: FrameKey) -> Result<SMatrix, ModelError> {
        if self.state(key)?.kind != FrameKind::Dummy {
            return Err(ModelError::DummyFrameRequired(key));
        }
        if self.graph.state(key)?.dummy_flags & 0x10 != 0 {
            if let Some(error) = self.graph.dummy_inv_failure(key)? {
                return Err(ModelError::Frame(error));
            }
            if let Some(cached) = self.graph.dummy_inv_cached(key)? {
                return Ok(cached);
            }
            // Flag set with neither error nor matrix is unreachable through
            // this entry (flag and stores change together below); fall through
            // recomputes rather than exposing a partial cache. Direct helper
            // misuse (preset flag, empty cache) safely tightens to recompute.
        } else {
            self.graph.change_dummy_flags(key, 0, 0x10)?;
        }
        let result = (|| {
            let world = self.graph.world_matrix(key)?;
            let (inverse, _) = world.inverse().map_err(|_| {
                crate::frame_transform::FrameError::NumericDomain("dummy GetInvMatrix inverse")
            })?;
            Ok(inverse)
        })();
        match result {
            Ok(matrix) => {
                self.graph.store_dummy_inv(key, Some(matrix), None)?;
                Ok(matrix)
            }
            Err(error) => {
                self.graph.store_dummy_inv(key, None, Some(error))?;
                Err(ModelError::Frame(error))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inv_matrix_valid_reader_reports_only_bit_four() {
        assert_eq!(DUMMYF_INV_MAT_VALID, 0x10);
        assert!(dummy_inv_matrix_valid(DUMMYF_INV_MAT_VALID));
        assert!(dummy_inv_matrix_valid(u32::MAX));
        assert!(!dummy_inv_matrix_valid(0));
        assert!(!dummy_inv_matrix_valid(0x03));
        assert!(!dummy_inv_matrix_valid(0x20));
    }
    #[test]
    fn normalized_reader_reports_only_bit_zero() {
        assert_eq!(DUMMYF_NORMALIZED, 0x01);
        assert!(dummy_is_normalized(DUMMYF_NORMALIZED));
        assert!(dummy_is_normalized(u32::MAX));
        assert!(!dummy_is_normalized(0));
        assert!(!dummy_is_normalized(DUMMYF_NORMALIZED_CENTER));
        assert!(!dummy_is_normalized(DUMMYF_INV_MAT_VALID));
        assert!(!dummy_is_normalized(u32::MAX & !DUMMYF_NORMALIZED));
    }
    #[test]
    fn center_normalized_reader_reports_only_bit_one() {
        assert_eq!(DUMMYF_NORMALIZED_CENTER, 0x02);
        assert!(dummy_is_center_normalized(DUMMYF_NORMALIZED_CENTER));
        assert!(dummy_is_center_normalized(u32::MAX));
        assert!(!dummy_is_center_normalized(0));
        assert!(!dummy_is_center_normalized(DUMMYF_NORMALIZED));
        assert!(!dummy_is_center_normalized(DUMMYF_INV_MAT_VALID));
        assert!(!dummy_is_center_normalized(
            u32::MAX & !DUMMYF_NORMALIZED_CENTER
        ));
    }
    #[test]
    fn normalize_mode_gate_reports_only_center_mode() {
        // Working name for the Normalize mode==2 gate (I3d_dummy.cpp:48-71):
        // no LS3DF FUNCTION address exists for a standalone reader.
        assert!(dummy_normalize_mode_is_center(2));
        assert!(!dummy_normalize_mode_is_center(0));
        assert!(!dummy_normalize_mode_is_center(1));
        assert!(!dummy_normalize_mode_is_center(99));
        assert!(!dummy_normalize_mode_is_center(u32::MAX));
    }
    #[test]
    fn genuine_dummy_constructor_box_copy_and_last_release_share_one_owner() {
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        assert_eq!(arena.state(key).unwrap().kind, FrameKind::Dummy);
        assert_eq!(arena.state(key).unwrap().dummy_flags, 0);
        assert_eq!(
            arena.dummy_bound_box(key).unwrap(),
            crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR.bbox
        );
        assert_eq!(arena.frame(key).unwrap().references, 1);
        assert_eq!(arena.frame(key).unwrap().owner, None);
        let state = *arena.state(key).unwrap();
        let bounds = [[f32::from_bits(0x7fc01234), -0., -2.], [4., 5., 6.]];
        arena.set_dummy_bound_box(key, bounds).unwrap();
        assert_eq!(
            arena
                .dummy_bound_box(key)
                .unwrap()
                .map(|r| r.map(f32::to_bits)),
            bounds.map(|r| r.map(f32::to_bits))
        );
        assert_eq!(*arena.state(key).unwrap(), state);
        arena.release(key).unwrap();
        assert!(arena.frame(key).is_err());
        assert_eq!(arena.live_frame_count(), 0);
    }

    #[test]
    fn dummy_duplicate_copies_bounds_then_common_tail() {
        let mut arena = ModelArena::default();
        let src = arena.create_dummy_frame().unwrap();
        let dst = arena.create_dummy_frame().unwrap();
        let bounds = [[1., 2., 3.], [4., 5., 6.]];
        arena.set_dummy_bound_box(src, bounds).unwrap();
        arena.duplicate_dummy_frame(dst, src).unwrap();
        assert_eq!(arena.dummy_bound_box(dst).unwrap(), bounds);
        assert_eq!(arena.dummy_bound_box(src).unwrap(), bounds);
        assert!(arena
            .duplicate_dummy_frame(dst, FrameKey(u32::MAX))
            .is_err());
        arena.release(src).unwrap();
        arena.release(dst).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn dummy_update_v_gates_on_update_or_changed() {
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        // Default frame flags carry bit 30: clear the gate first.
        arena.graph.change_flags(key, 0x4000_2000, 0).unwrap();
        arena.graph.change_dummy_flags(key, 0, 0x10).unwrap();
        arena.graph.dummy_update_v(key).unwrap();
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0x10);
        // UPDATE|CHANGED set: INV_MAT_VALID clears.
        arena.graph.change_flags(key, 0, 0x4000_2000).unwrap();
        arena.graph.dummy_update_v(key).unwrap();
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0);
        arena.release(key).unwrap();
    }
    #[test]
    fn dummy_normalize_limbs_set_box_and_flag_words() {
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        arena
            .set_dummy_bound_box(key, [[1., 2., 3.], [4., 5., 6.]])
            .unwrap();
        // Center limb (mode 2): box to ±1, flags (cleared INV|NORM)|CENTER.
        arena
            .graph
            .change_dummy_flags(key, 0, 0x10 | 0x1 | 0x2)
            .unwrap();
        arena.normalize_dummy_bounds(key, 2).unwrap();
        assert_eq!(arena.dummy_bound_box(key).unwrap(), [[-1.; 3], [1.; 3]]);
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x13, 0x2);
        // Min-corner limb (else, incl. unknown modes): min 0/max 1.
        arena
            .set_dummy_bound_box(key, [[1., 2., 3.], [4., 5., 6.]])
            .unwrap();
        arena
            .graph
            .change_dummy_flags(key, 0, 0x10 | 0x1 | 0x2)
            .unwrap();
        arena.normalize_dummy_bounds(key, 0).unwrap();
        assert_eq!(arena.dummy_bound_box(key).unwrap(), [[0.; 3], [1.; 3]]);
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x13, 0x1);
        arena.normalize_dummy_bounds(key, 99).unwrap();
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x13, 0x1);
        arena.release(key).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn dummy_destructor_clears_weather_registry_and_flag_only_when_set() {
        // PC RVA 0x1a260 (mafia-re I3d_dummy.cpp:19): derived weather loop.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let key = arena.create_dummy_frame().unwrap();
        arena
            .set_dummy_bound_box(key, [[-1., -1., -1.], [1., 1., 1.]])
            .unwrap();
        // Flag clear is a no-op that preserves a live frame (scene prefix
        // holds 5 frames, plus the dummy).
        arena.destroy_dummy_frame(key).unwrap();
        assert_eq!(arena.live_frame_count(), 6);
        // Non-dummy frames are rejected without touching the registry.
        let model = arena.create_model().unwrap();
        assert!(matches!(
            arena.destroy_dummy_frame(model.root()),
            Err(ModelError::DummyFrameRequired(_))
        ));
        assert_eq!(arena.weather_dummy_count(scene).unwrap(), 0);
        // Registered dummy: derived teardown removes every scene entry and
        // clears WEATHER while keeping the frame alive for the base tail.
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            assert!(arena.weather_add_dummy(scene, key).unwrap());
        });
        assert_ne!(arena.state(key).unwrap().flags & 0x8000_0000, 0);
        arena.destroy_dummy_frame(key).unwrap();
        assert_eq!(arena.state(key).unwrap().flags & 0x8000_0000, 0);
        assert_eq!(arena.weather_dummy_count(scene).unwrap(), 0);
        assert!(arena.frame(key).is_ok());
        arena.release(model.root()).unwrap();
        arena.release(key).unwrap();
        // The scene-owner prefix (5 frames) outlives the model and dummy.
        assert_eq!(arena.live_frame_count(), 5);
    }
    #[test]
    fn dummy_load_image_stores_common_fields_then_bbox_bits() {
        // PC RVA 0x1a6a0 (mafia-re I3d_dummy.cpp:75): common tail + bbox.
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        let rotation = crate::native_transform::SQuat {
            wxyz: [1., 0., 0., 0.],
        };
        let bbox = [[f32::from_bits(0x7fc01234), -0., 1.], [4., 5., 6.]];
        arena
            .apply_dummy_load_image_fields(key, [7., 8., 9.], [2., 3., 4.], rotation, 0x05, bbox)
            .unwrap();
        let state = *arena.state(key).unwrap();
        assert_eq!(state.local.elements[12..15], [7., 8., 9.]);
        assert_eq!(state.scale, [2., 3., 4.]);
        assert_eq!(
            state.rotation.wxyz.map(f32::to_bits),
            rotation.wxyz.map(f32::to_bits)
        );
        assert_eq!(state.flags, 0x40000009);
        assert_eq!(
            arena
                .dummy_bound_box(key)
                .unwrap()
                .map(|r| r.map(f32::to_bits)),
            bbox.map(|r| r.map(f32::to_bits))
        );
        // Non-dummy frames are rejected before any store.
        let model = arena.create_model().unwrap();
        assert!(matches!(
            arena.apply_dummy_load_image_fields(model.root(), [0.; 3], [1.; 3], rotation, 0, bbox),
            Err(ModelError::DummyFrameRequired(_))
        ));
        arena.release(model.root()).unwrap();
        arena.release(key).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn dummy_inv_matrix_lazy_gate_caches_inverse_and_reports_singular_identity() {
        // PC RVA 0x68840 (mafia-re ls3df/I3D/I3D.h:1850, original
        // Insanity3D-sourced name; render/T_l.cpp:6 copy-ordering caveat): flag 0x10
        // set first, then 0x20-gated world, then Inverse(0x2d0b0). Float
        // policy: guarded PC24/RN world plus native_matrix::inverse; singular
        // discards the boolean and returns identity.
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0);
        let first = arena.dummy_inv_matrix(key).unwrap();
        assert_eq!(first, SMatrix::identity());
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0x10);
        arena.graph.assign_position(key, [5., 0., 0.]).unwrap();
        let stale = arena.dummy_inv_matrix(key).unwrap();
        assert_eq!(stale, SMatrix::identity());
        arena.graph.dummy_update_v(key).unwrap();
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0);
        let refreshed = arena.dummy_inv_matrix(key).unwrap();
        assert_eq!(refreshed.elements[0], 1.);
        assert_eq!(refreshed.elements[5], 1.);
        assert_eq!(refreshed.elements[10], 1.);
        assert_eq!(refreshed.elements[12..15], [-5., 0., 0.]);
        assert_eq!(arena.dummy_inv_matrix(key).unwrap(), refreshed);
        arena.graph.assign_scale(key, [0., 0., 0.]).unwrap();
        arena.graph.dummy_update_v(key).unwrap();
        let singular = arena.dummy_inv_matrix(key).unwrap();
        assert_eq!(singular, SMatrix::identity());
        // Interrupted path: non-finite scale poisons the world, so the
        // inverse reports NumericDomain with the flag retained and no cache;
        // a second read reports the identical error without recompute.
        arena.graph.assign_scale(key, [f32::NAN, 1., 1.]).unwrap();
        arena.graph.dummy_update_v(key).unwrap();
        let first_err = arena.dummy_inv_matrix(key).unwrap_err();
        assert!(matches!(
            first_err,
            ModelError::Frame(crate::frame_transform::FrameError::NumericDomain(_))
        ));
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0x10);
        assert!(arena.graph.dummy_inv_cached(key).unwrap().is_none());
        let second_err = arena.dummy_inv_matrix(key).unwrap_err();
        assert_eq!(format!("{first_err:?}"), format!("{second_err:?}"));
        // Clearing the flag retries cold on the next call.
        arena.graph.dummy_update_v(key).unwrap();
        assert_eq!(arena.state(key).unwrap().dummy_flags & 0x10, 0);
        let model = arena.create_model().unwrap();
        assert!(matches!(
            arena.dummy_inv_matrix(model.root()),
            Err(ModelError::DummyFrameRequired(_))
        ));
        arena.release(model.root()).unwrap();
        arena.release(key).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
}
