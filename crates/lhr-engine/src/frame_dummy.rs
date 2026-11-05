//! I3D_dummy's real constructor/local box in the canonical frame arena.
//! PC LS3DF e211c850... RVA1a1e0; Game08398317... helper546d10.
//! The Game box helper copies six DWORDs only: it does not normalize, update
//! transforms, change cache flags or write the base hierarchical bound volume.
use crate::{
    frame_transform::{FrameKey, FrameKind, I3dFrameState},
    model_loading::{ModelArena, ModelError},
    native_transform::SQuat,
    scene_weather_dummy::WEATHER_FLAG,
};

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
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
