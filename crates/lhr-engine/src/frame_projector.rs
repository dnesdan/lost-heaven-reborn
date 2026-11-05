//! Actual PS2 I3D_projector symbols corroborate SetTexture, SetOrthogonal,
//! SetColor, SetMode and SetProjectorLock.
//! Original projector constructor/property/lifetime projection (PC LS3DF
//! 45e30,46000,46240..46480). Render-time clipping/material/IB/VB allocation is
//! separate: these objects start with the actual null render allocations and
//! only admitted operations may mutate their owned state.
use crate::{
    frame_transform::{FrameError, FrameKey, FrameKind, I3dFrameState},
    mesh_bounds::MeshBoundVolume,
    model_loading::{ModelArena, ModelError},
    visual_resources::{MaterialKey, TextureKey},
};

#[derive(Clone, Debug)]
pub struct I3dProjectorState {
    pub(crate) texture: Option<TextureKey>,
    // LS3DF e211c850…, RVA 0x46270: I3D_projector::SetMaterial stores
    // [this+0x200] with AddRef INC [mat+4]; Release on replace (differential:
    // Ghidra body 47 B, 19 insn, 6 BB). The refcount traffic itself is a
    // backend contract with no host owner yet (disclosed): the store order,
    // identity no-op and no-invalidate shape below are the credited behavior.
    pub(crate) material: Option<MaterialKey>,
    pub orthogonal: u8,
    pub range: f32,
    pub alpha: f32,
    pub color: [f32; 3],
    pub lock: u32,
    pub mode: u32,
    pub center_t: [f32; 3],
    pub reverse: f32,
    pub(crate) cache_254: u32,
    pub(crate) cache_25c: u32,
}
impl I3dProjectorState {
    pub(crate) fn constructor() -> Self {
        Self {
            texture: None,
            material: None,
            orthogonal: 1,
            range: 100.,
            alpha: 1.,
            color: [0.; 3],
            lock: 0,
            mode: 0,
            center_t: [0.; 3],
            reverse: 0.5,
            cache_254: 0,
            cache_25c: 0,
        }
    }
    pub fn texture(&self) -> Option<TextureKey> {
        self.texture
    }
    pub fn material(&self) -> Option<MaterialKey> {
        self.material
    }
    fn invalidate(&mut self) {
        self.cache_25c = 0;
        self.cache_254 = 0;
    }
}
impl ModelArena {
    /// Construct an actual CPU-owned projector frame; no graphics allocations
    /// are created by the original constructor either.
    pub fn create_projector(&mut self) -> Result<FrameKey, ModelError> {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Projector;
        state.visual_flags = 0x2200;
        let key = self.insert(state, None, Vec::new(), Vec::new(), None)?;
        let index = self.frame_index(key)?;
        self.frames[index].projector = Some(I3dProjectorState::constructor());
        self.frames[index].visual_bounds = Some(MeshBoundVolume {
            bbox: [[-1., -1., 0.], [1., 1., 1.]],
            center: [0., 0., 0.5],
            radius: 1.5,
        });
        Ok(key)
    }
    pub fn projector(&self, key: FrameKey) -> Result<&I3dProjectorState, ModelError> {
        self.frame(key)?
            .projector
            .as_ref()
            .ok_or(ModelError::Frame(FrameError::UnsupportedFrameUpdate(key)))
    }
    fn projector_mut(&mut self, key: FrameKey) -> Result<&mut I3dProjectorState, ModelError> {
        let index = self.frame_index(key)?;
        self.frames[index]
            .projector
            .as_mut()
            .ok_or(ModelError::Frame(FrameError::UnsupportedFrameUpdate(key)))
    }
    /// SetTexture releases the previous reference before storing and retaining
    /// the new one. Identical identity is a true no-op, including null.
    pub fn set_projector_texture(
        &mut self,
        key: FrameKey,
        texture: Option<TextureKey>,
    ) -> Result<(), ModelError> {
        let previous = self.projector(key)?.texture;
        if previous == texture {
            return Ok(());
        }
        if let Some(texture) = texture {
            let references = self
                .visual_resource_owner_mut()?
                .backend()
                .texture_references(texture)
                .ok_or(ModelError::LoadedVisual(
                    crate::loaded_visual_resources::LoadedVisualError::Backend(
                        "unknown CPU texture",
                    ),
                ))?;
            if references == u32::MAX {
                return Err(ModelError::AllocationOverflow);
            }
        }
        if let Some(previous) = previous {
            self.release_cpu_texture(previous)?;
        }
        self.projector_mut(key)?.texture = texture;
        if let Some(texture) = texture {
            self.retain_cpu_texture(texture)?;
        }
        Ok(())
    }
    pub fn set_projector_orthogonal(
        &mut self,
        key: FrameKey,
        orthogonal: u8,
    ) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.orthogonal = orthogonal;
        projector.invalidate();
        Ok(())
    }
    // Keep the original ordered comparisons and unmodified NaN/zero bits.
    #[allow(clippy::manual_clamp)]
    pub fn set_projector_color(
        &mut self,
        key: FrameKey,
        color: [f32; 3],
    ) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.color = color;
        for component in &mut projector.color {
            if *component < 0. {
                *component = 0.;
            } else if *component > 1. {
                *component = 1.;
            }
        }
        projector.invalidate();
        Ok(())
    }
    pub fn set_projector_mode(&mut self, key: FrameKey, mode: u32) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.mode = mode;
        projector.invalidate();
        Ok(())
    }
    pub fn set_projector_lock(&mut self, key: FrameKey, lock: u32) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.lock = lock;
        projector.invalidate();
        Ok(())
    }
    // LS3DF e211c850…, RVA 0x463a0: SetAlpha stores [this+0x1e8], clamps only
    // a < 0.0f to 0 (x87 FCOMP/JP keeps NaN; -0.0 compares equal, kept), then
    // clears receivers words. Contract: no Ghidra body (B1 probes + linear
    // decode). Precision: ordered f32 compares; NaN flows through unchanged.
    #[allow(clippy::manual_clamp)]
    pub fn set_projector_alpha(&mut self, key: FrameKey, alpha: f32) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.alpha = alpha;
        if projector.alpha < 0. {
            projector.alpha = 0.;
        }
        projector.invalidate();
        Ok(())
    }
    // LS3DF e211c850…, RVA 0x463f0: SetReverseFactor stores [this+0x260],
    // clamps f < 0 to 0 else f > 1 to 1.0f (second stage re-tests the stored
    // value; NaN kept), then clears receivers words. Contract, same basis.
    #[allow(clippy::manual_clamp)]
    pub fn set_projector_reverse_factor(
        &mut self,
        key: FrameKey,
        factor: f32,
    ) -> Result<(), ModelError> {
        let projector = self.projector_mut(key)?;
        projector.reverse = factor;
        if projector.reverse < 0. {
            projector.reverse = 0.;
        } else if projector.reverse > 1. {
            projector.reverse = 1.;
        }
        projector.invalidate();
        Ok(())
    }
    // LS3DF e211c850…, RVA 0x46270: SetMaterial. Identity (incl. null) is a
    // true no-op; else release old, store, AddRef new. No receiver
    // invalidation (19-insn body has no +0x254/+0x25c stores). Differential
    // (formed body); refcount traffic is the disclosed backend contract.
    pub fn set_projector_material(
        &mut self,
        key: FrameKey,
        material: Option<MaterialKey>,
    ) -> Result<(), ModelError> {
        let previous = self.projector(key)?.material;
        if previous == material {
            return Ok(());
        }
        self.projector_mut(key)?.material = material;
        Ok(())
    }
    // LS3DF e211c850…, RVA 0x464a0: I3D_projector::UpdateV (thiscall). If
    // !(vis_flags & 0x40) && (frame_flags & 0x40002000): vis_flags &= ~0x1c0000
    // (AND 0xffe3ffff), sector word +0x258 = 0 (host: projector_cache_258).
    // Contract: no Ghidra body (B1 probes + 9-insn decode). Integer-exact;
    // effect order pinned (mask store, then sector clear); short-circuit
    // preserved (flags unread when 0x40 set).
    pub fn update_projector_v(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.projector(key)?;
        let (vis, frm) = {
            let s = self.state(key)?;
            (s.visual_flags, s.flags)
        };
        if (vis & 0x40) == 0 && (frm & 0x40002000) != 0 {
            let st = self
                .graph
                .projected_state_mut(key)
                .map_err(ModelError::Frame)?;
            st.visual_flags &= !0x1c0000;
            st.projector_cache_258 = 0;
        }
        Ok(())
    }
    // LS3DF e211c850…, RVA 0x460c0: I3D_projector::Duplicate. Contract (B1:
    // entry + vtable DATA ref + RET probe, ~118-insn decode, all targets
    // walked). Order-exact: null/self analogue -> 0x80000007 (no stores,
    // checked first as natively); common duplicate_frame first (native
    // I3D_frame::Duplicate call first); then source-visual dispatch.
    // visible_range is a direct store with NO invalidate. This matches the
    // reference's SetVisibleRange spelling (I3D.h:4854-6: a plain store, no
    // invalidate) and the walked decode's FLD/FSTP pair; the tier stays
    // contract because no Ghidra body was formed. rfg delete[] is a no-op
    // over the admitted constructor-null lifetime; rfg=NULL/sector=NULL map
    // to the sector-word clear. Mesh-visual sources (OBJECT/LIT/BILLBOARD/
    // MORPH/SINGLEMESH/SINGLEMORPH/LENSF) take the null-tolerant chain arm:
    // null links are silently skipped and the result is always Ok, as
    // natively (I3d_projector.cpp:133-148); the mesh→LOD(0)→FGroup(0)→
    // material link walk itself stays a backend contract, so the material
    // copy is disclosed as not-yet-ported rather than erroring.
    pub fn duplicate_projector_derived(
        &mut self,
        dst: FrameKey,
        src: FrameKey,
    ) -> Result<(), ModelError> {
        if dst == src {
            return Err(ModelError::NativeModelOpenResult(0x80000007));
        }
        self.frame(src).map_err(|_| ModelError::UnknownFrame(src))?;
        self.frame(dst).map_err(|_| ModelError::UnknownFrame(dst))?;
        self.duplicate_frame(dst, src)?;
        let di = self.frame_index(dst)?;
        if self.frames[di].projector.is_none() {
            return Err(ModelError::Frame(FrameError::UnsupportedFrameUpdate(dst)));
        }
        if self.frames[self.frame_index(src)?].projector.is_some() {
            let s = self.projector(src)?.clone();
            self.set_projector_texture(dst, s.texture)?;
            self.set_projector_material(dst, s.material)?;
            self.set_projector_orthogonal(dst, s.orthogonal)?;
            self.projector_mut(dst)?.range = s.range;
            self.set_projector_color(dst, s.color)?;
            self.set_projector_alpha(dst, s.alpha)?;
            self.set_projector_reverse_factor(dst, s.reverse)?;
            self.set_projector_mode(dst, s.mode)?;
            self.set_projector_lock(dst, s.lock)?;
            self.graph
                .projected_state_mut(dst)
                .map_err(ModelError::Frame)?
                .projector_cache_258 = 0;
            return Ok(());
        }
        // Mesh-visual arm: native walks the mesh chain with null-tolerant
        // skips and always returns Ok. The chain walk is unported (backend
        // contract); the dst keeps its common-duplicate stores.
        Ok(())
    }
    pub(crate) fn release_projector_resources(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if let Some(texture) = self.projector(key)?.texture {
            self.release_cpu_texture(texture)?;
            self.projector_mut(key)?.texture = None;
        }
        // Native dtor order (RVA 0x46000): texture, then material. The
        // material release is backend-contracted (no host owner); the store
        // clear is the credited order.
        if self.projector(key)?.material.is_some() {
            self.projector_mut(key)?.material = None;
        }
        // Material, vertex/face/group allocations and manager block handles
        // are constructor-null throughout this admitted pre-render lifetime.
        let index = self.frame_index(key)?;
        self.frames[index].projector = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ctor_defaults_pin_zero_color_and_widths() {
        // RVA 0x45e30: color source [0x101c5188] is 12 zero bytes, so the
        // binary pins [0,0,0] here; the upstream reconstruction's
        // `color = UnitVector` spelling (I3d_projector.cpp:84) is superseded
        // on this point (binary wins). Orthogonal is a byte store;
        // radius = FSQRT(2.25) = 1.5 exact.
        let mut arena = ModelArena::default();
        let p = arena.create_projector().unwrap();
        let s = arena.projector(p).unwrap();
        assert_eq!(s.color, [0., 0., 0.]);
        assert_eq!(s.color.map(f32::to_bits), [0u32, 0, 0]);
        assert_eq!(s.orthogonal, 1);
        assert_eq!(s.range, 100.);
        assert_eq!(s.alpha, 1.);
        assert_eq!(s.reverse, 0.5);
        assert_eq!(s.mode, 0);
        assert_eq!(s.lock, 0);
        assert_eq!(s.center_t, [0., 0., 0.]);
        assert_eq!(s.texture(), None);
        assert_eq!(s.material(), None);
        assert_eq!(arena.state(p).unwrap().visual_flags, 0x2200);
    }
    #[test]
    fn alpha_clamp_roundtrip_with_nan_and_invalidate() {
        // RVA 0x463a0: only a<0 -> 0; NaN/-0.0 kept bits; always invalidates.
        let mut arena = ModelArena::default();
        let p = arena.create_projector().unwrap();
        for (input, expect_bits) in [
            (-1.5f32, 0f32.to_bits()),
            (f32::NEG_INFINITY, 0u32),
            (-0.0f32, (-0.0f32).to_bits()),
            (0f32, 0u32),
            (0.25f32, 0.25f32.to_bits()),
            (1f32, 1f32.to_bits()),
            (2.5f32, 2.5f32.to_bits()),
            (f32::INFINITY, f32::INFINITY.to_bits()),
            (f32::NAN, f32::NAN.to_bits()),
        ] {
            let di = arena.frame_index(p).unwrap();
            arena.frames[di].projector.as_mut().unwrap().cache_254 = 0x5A5A5A5A;
            arena.frames[di].projector.as_mut().unwrap().cache_25c = 0x5A5A5A5A;
            arena.set_projector_alpha(p, input).unwrap();
            arena.set_projector_alpha(p, input).unwrap();
            assert_eq!(
                arena.projector(p).unwrap().alpha.to_bits(),
                expect_bits,
                "alpha {input}"
            );
            assert_eq!(arena.projector(p).unwrap().cache_254, 0);
            assert_eq!(arena.projector(p).unwrap().cache_25c, 0);
        }
    }
    #[test]
    fn reverse_clamp_roundtrip_with_nan() {
        // RVA 0x463f0: f<0->0 else f>1->1; NaN kept; always invalidates.
        let mut arena = ModelArena::default();
        let p = arena.create_projector().unwrap();
        for (input, expect) in [
            (-2f32, 0f32),
            (-0.0, -0.0),
            (0.0, 0.0),
            (0.5, 0.5),
            (1.0, 1.0),
            (1.5, 1.0),
            (f32::INFINITY, 1.0),
        ] {
            arena.set_projector_reverse_factor(p, input).unwrap();
            assert_eq!(
                arena.projector(p).unwrap().reverse.to_bits(),
                expect.to_bits()
            );
        }
        arena.set_projector_reverse_factor(p, f32::NAN).unwrap();
        assert!(arena.projector(p).unwrap().reverse.is_nan());
        assert_eq!(arena.projector(p).unwrap().cache_254, 0);
        assert_eq!(arena.projector(p).unwrap().cache_25c, 0);
    }
    #[test]
    fn identity_setters_touch_no_owner_and_no_state() {
        // RVAs 0x46240/0x46270: identical (incl. null) pointers skip everything.
        let mut arena = ModelArena::default();
        let p = arena.create_projector().unwrap();
        let di = arena.frame_index(p).unwrap();
        arena.frames[di].projector.as_mut().unwrap().cache_254 = 7;
        arena.frames[di].projector.as_mut().unwrap().cache_25c = 9;
        arena.set_projector_texture(p, None).unwrap();
        arena.set_projector_material(p, None).unwrap();
        assert!(arena.loaded_visual_resources().is_none());
        assert_eq!(arena.projector(p).unwrap().cache_254, 7);
        assert_eq!(arena.projector(p).unwrap().cache_25c, 9);
    }
    #[test]
    fn non_identity_setters_always_invalidate() {
        // RVAs 0x462a0/0x46460/0x46480: no early-out; store + clear both words.
        let mut arena = ModelArena::default();
        let p = arena.create_projector().unwrap();
        arena.set_projector_orthogonal(p, 1).unwrap();
        assert_eq!(
            (
                arena.projector(p).unwrap().cache_254,
                arena.projector(p).unwrap().cache_25c
            ),
            (0, 0)
        );
        arena.set_projector_orthogonal(p, 0).unwrap();
        assert_eq!(arena.projector(p).unwrap().orthogonal, 0);
        arena.set_projector_mode(p, 5).unwrap();
        assert_eq!(arena.projector(p).unwrap().mode, 5);
        arena.set_projector_lock(p, 3).unwrap();
        assert_eq!(arena.projector(p).unwrap().lock, 3);
        arena.set_projector_color(p, [f32::NAN, -1., 2.]).unwrap();
        let c = arena.projector(p).unwrap().color;
        assert!(c[0].is_nan());
        assert_eq!(
            [c[1].to_bits(), c[2].to_bits()],
            [0f32.to_bits(), 1f32.to_bits()]
        );
    }
    #[test]
    fn update_v_three_way_matrix() {
        // RVA 0x464a0: skip iff (vis&0x40)!=0 OR (flags&0x40002000)==0.
        let mut arena = ModelArena::default();
        let p = arena.create_projector().unwrap();
        let st = arena.graph.projected_state_mut(p).unwrap();
        st.visual_flags = 0x2200 | 0x1c0000 | 0x40;
        st.flags = 0x40002000;
        st.projector_cache_258 = 11;
        arena.update_projector_v(p).unwrap();
        assert_eq!(arena.state(p).unwrap().visual_flags & 0x1c0000, 0x1c0000);
        assert_eq!(arena.state(p).unwrap().projector_cache_258, 11);
        let st = arena.graph.projected_state_mut(p).unwrap();
        st.visual_flags = 0x2200 | 0x1c0000;
        st.flags = 0x3d; // ON word without either UPDATE or CHANGED bits
        arena.update_projector_v(p).unwrap();
        assert_eq!(arena.state(p).unwrap().visual_flags & 0x1c0000, 0x1c0000);
        let st = arena.graph.projected_state_mut(p).unwrap();
        st.flags = 0x40002000;
        arena.update_projector_v(p).unwrap();
        assert_eq!(arena.state(p).unwrap().visual_flags & 0x1c0000, 0);
        assert_eq!(arena.state(p).unwrap().visual_flags & 0x2200, 0x2200);
        assert_eq!(arena.state(p).unwrap().projector_cache_258, 0);
    }
    #[test]
    fn duplicate_projector_guards_orders_and_residue() {
        // RVA 0x460c0: invalid/unknown guards; common-first order;
        // projector copy incl. unclamped range direct store; src untouched.
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let dst = arena.create_projector().unwrap();
        let src = arena.create_projector().unwrap();
        assert!(matches!(
            arena.duplicate_projector_derived(dst, dst),
            Err(ModelError::NativeModelOpenResult(0x80000007))
        ));
        let dead = FrameKey(99999);
        assert!(matches!(
            arena.duplicate_projector_derived(dst, dead),
            Err(ModelError::UnknownFrame(k)) if k == dead
        ));
        let di = arena.frame_index(dst).unwrap();
        arena.frames[di].projector.as_mut().unwrap().orthogonal = 0;
        arena.frames[di].projector.as_mut().unwrap().cache_254 = 77;
        arena
            .duplicate_projector_derived(dst, model.root())
            .unwrap();
        assert_eq!(arena.projector(dst).unwrap().orthogonal, 0);
        assert_eq!(arena.projector(dst).unwrap().cache_254, 77);
        arena.set_projector_orthogonal(src, 0).unwrap();
        arena.set_projector_color(src, [-5., 0.5, 0.5]).unwrap();
        arena.set_projector_alpha(src, -2.).unwrap();
        arena.set_projector_reverse_factor(src, 9.).unwrap();
        arena.set_projector_mode(src, 2).unwrap();
        arena.set_projector_lock(src, 1).unwrap();
        let si = arena.frame_index(src).unwrap();
        arena.frames[si].projector.as_mut().unwrap().range = 1e30;
        arena.frames[si].projector.as_mut().unwrap().cache_254 = 0x12345678;
        arena
            .graph
            .projected_state_mut(dst)
            .unwrap()
            .projector_cache_258 = 44;
        arena.duplicate_projector_derived(dst, src).unwrap();
        let d = arena.projector(dst).unwrap();
        assert_eq!(
            (d.orthogonal, d.range.to_bits(), d.mode, d.lock),
            (0, 1e30f32.to_bits(), 2, 1)
        );
        assert_eq!(d.color, [0., 0.5, 0.5]);
        assert_eq!((d.alpha, d.reverse), (0., 1.));
        assert_eq!(
            arena
                .graph
                .projected_state_mut(dst)
                .unwrap()
                .projector_cache_258,
            0
        );
        assert_eq!(arena.projector(src).unwrap().cache_254, 0x12345678);
    }
    #[test]
    fn properties_update_caches_and_scene_close_preserves_external_projector() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let projector = arena.create_projector().unwrap();
        assert_eq!(arena.projector(projector).unwrap().color, [0.; 3]);
        arena.add_frame(model, projector).unwrap();
        arena.release(projector).unwrap();
        arena.set_projector_orthogonal(projector, 0).unwrap();
        arena
            .set_projector_color(projector, [-1., 0.5, 2.])
            .unwrap();
        arena.set_projector_mode(projector, 3).unwrap();
        arena.set_projector_lock(projector, 1).unwrap();
        assert_eq!(arena.projector(projector).unwrap().color, [0., 0.5, 1.]);
        arena
            .graph
            .projected_state_mut(projector)
            .unwrap()
            .projector_cache_258 = 17;
        arena.graph.update(projector).unwrap();
        assert_eq!(arena.state(projector).unwrap().projector_cache_258, 0);
        assert_eq!(
            arena.hr_bound_volume(projector).unwrap().bbox,
            [[-1., -1., 0.], [1., 1., 1.]]
        );
        arena.retain(projector).unwrap();
        arena.close(model).unwrap();
        assert_eq!(arena.frame(projector).unwrap().references, 1);
        arena.release(projector).unwrap();
        assert!(arena.projector(projector).is_err());
    }
}
