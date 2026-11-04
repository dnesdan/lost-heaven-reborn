//! I3D_dummy's real constructor/local box in the canonical frame arena.
//! PC LS3DF e211c850... RVA1a1e0; Game08398317... helper546d10.
//! The Game box helper copies six DWORDs only: it does not normalize, update
//! transforms, change cache flags or write the base hierarchical bound volume.
use crate::{
    frame_transform::{FrameKey, FrameKind, I3dFrameState},
    model_loading::{ModelArena, ModelError},
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
}
