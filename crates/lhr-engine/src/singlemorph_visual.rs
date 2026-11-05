//! Singlemorph (SMMR) thin layer over the ported morphBase weights (PC LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! GROUP X5: 3 differential + 8 contract).
//!
//! Differentials: ctor 0x56e80 (22 insn/1 BB: singlemesh base → morphBase
//! `new` → vtable + `SMMR` words), dtor 0x56f10 (22/1: vtable restore →
//! morphBase::Clear → singlemesh dtor), UpdateLodVertices 0x57590 (22/3: dirty
//! gate → UpdateMorphLod → virtual → singlemesh tail). Contracts (no Ghidra
//! body, same standard as committed 0x2b830): NumLevels 0x56f70, NumRegions
//! 0x56f80, SetWeight 0x56f90, GetWeight 0x57030, SetMesh 0x570b0, Duplicate
//! 0x57100, UpdateBoundProc 0x57580, LoadImage 0x575d0.
//! Upstream names `I3D_singlemorph::*` are mafia-re working labels, not
//! evidenced original symbols. Float math lives only in `SinglemorphState`
//! (x87 PC24/RN); this layer stores weight bits verbatim (no clamp, no finite
//! gate — deferred to `normalize_weights`' numeric domain, exactly as natively).
//! Mesh/LOD/file/device/UpdateVertices/singlemesh tails are caller contracts.
#![forbid(unsafe_code)]

use crate::singlemorph_state::{SinglemorphError, SinglemorphState};
use lhr_assets::{MorphRecord, Vertex};

/// Observed `MOV [ESI+0x1d4]` in the 0x56e80 body, LE "SMMR".
pub const SINGLEMORPH_MAGIC: u32 = 0x524d_4d53;
/// `vis_flags &= ~` word in SetWeight/Duplicate/SetMesh/LoadImage.
pub const SINGLEMORPH_WBOUND_VALID: u32 = 0x4_0000;
/// Duplicate tail mask (same word as the object-level mask).
pub const SINGLEMORPH_DUPLICATE_MASK: u32 = 0x2_ffd0;
/// Proxy `goto obj` gate in Duplicate.
pub const SINGLEMORPH_VISF_PROXY: u32 = 0x8000;
pub const SINGLEMORPH_VISF_WORLD_SPACE: u32 = 0x40;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinglemorphVisualError {
    WeightIndex,
    Definition(SinglemorphError),
    DuplicateSource,
    Backend(&'static str),
}
impl From<SinglemorphError> for SinglemorphVisualError {
    fn from(e: SinglemorphError) -> Self {
        match e {
            SinglemorphError::WeightIndex => Self::WeightIndex,
            other => Self::Definition(other),
        }
    }
}
impl std::fmt::Display for SinglemorphVisualError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WeightIndex => write!(f, "singlemorph unavailable: WeightIndex"),
            Self::Definition(e) => write!(f, "singlemorph unavailable: {e}"),
            Self::DuplicateSource => write!(f, "singlemorph unavailable: DuplicateSource"),
            Self::Backend(op) => write!(f, "singlemorph unavailable: {op}"),
        }
    }
}
impl std::error::Error for SinglemorphVisualError {}

/// Typed live view of the singlemorph-owned words around `SinglemorphState`.
/// Borrows actual state; `weights` reads happen where the original reads them.
#[derive(Clone, Debug)]
pub struct SinglemorphVisual {
    pub weights: SinglemorphState,
    pub joints_changed: u32,
    pub visual_flags: u32,
    pub magic: u32,
}

impl SinglemorphVisual {
    /// Ctor 0x56e80 differential: singlemesh base (caller) → morphBase `new()`
    /// (flags 5, levels/regions 0) → vtable words + `SMMR` magic. `0x2200` is
    /// the `I3D_visual` base word shared by every visual carrier in this tree
    /// (no SMMR-specific evidence — disclosed); `SMMR` + base/morphBase order
    /// come from the 22-insn body. `joints_changed` starts at 0 here while the
    /// singlemesh base ctor stores −300: the SMMR layer owns its own counter
    /// word (no inherited store observed in the 22-insn body — disclosed).
    pub fn singlemorph_ctor_fields() -> Self {
        Self {
            weights: SinglemorphState::new(),
            joints_changed: 0,
            visual_flags: 0x2200,
            magic: SINGLEMORPH_MAGIC,
        }
    }
    /// Dtor 0x56f10 differential order: morphBase::Clear call from the
    /// 22-insn body (mafia-re shows an empty body — the Ghidra
    /// `CALL 0x10034390` wins for order, disclosed) then the singlemesh tail
    /// (caller). Returns the dropped weight storage bytes for residue
    /// accounting.
    pub fn singlemorph_teardown_order(&mut self) -> usize {
        let residue = self.weights.storage_bytes();
        // morphBase::Clear equivalent: levels/regions/weights cleared; the
        // caller runs the singlemesh tail next.
        self.weights = SinglemorphState::new();
        residue
    }
    /// NumLevels 0x56f70 contract: trivial getter, no effects.
    pub fn singlemorph_level_count(&self) -> usize {
        self.weights.level_count()
    }
    /// NumRegions 0x56f80 contract: trivial getter, no effects.
    pub fn singlemorph_region_count(&self) -> usize {
        self.weights.region_count()
    }
    /// SetWeight 0x56f90 contract: signed gate → joints inc → flag clear →
    /// SetWeightB. No clamp, no finite gate. Miss logs natively
    /// (`!! index out of range  I3D_singlemorph::SetWeight() …`) then
    /// INVALIDPARAMS, modeled here as `WeightIndex`.
    pub fn singlemorph_set_weight(
        &mut self,
        region: i32,
        level: i32,
        weight: f32,
    ) -> Result<(), SinglemorphVisualError> {
        if region < 0 || level < 0 {
            return Err(SinglemorphVisualError::WeightIndex);
        }
        let (r, l) = (region as usize, level as usize);
        if r >= self.weights.region_count() || l >= self.weights.level_count() {
            return Err(SinglemorphVisualError::WeightIndex);
        }
        self.joints_changed = self.joints_changed.wrapping_add(1);
        self.visual_flags &= !SINGLEMORPH_WBOUND_VALID;
        self.weights.set_weight(r, l, weight)?;
        Ok(())
    }
    /// GetWeight 0x57030 contract: signed gate; miss → 0.0 (native logs
    /// `…GetWeight() …`). No failure word.
    pub fn singlemorph_weight(&self, region: i32, level: i32) -> f32 {
        if region < 0 || level < 0 {
            return 0.0;
        }
        let (r, l) = (region as usize, level as usize);
        if r >= self.weights.region_count() || l >= self.weights.level_count() {
            return 0.0;
        }
        self.weights.weights()[r * self.weights.level_count() + l]
    }
    /// SetMesh 0x570b0 contract tail after the caller's mesh-compat gate:
    /// bound copy is caller-side, then `if (!WORLD_SPACE) clear WBOUND_VALID`.
    pub fn singlemorph_set_mesh_tail(&mut self, world_space: bool) {
        if !world_space {
            self.visual_flags &= !SINGLEMORPH_WBOUND_VALID;
        }
    }
    /// Duplicate 0x57100 contract, weight-relevant prefix/suffix only.
    /// Caller owns frame/mesh/joint/lod/flag/proxy/branch effects; this covers
    /// the joints inc, flag-mask clear, and `duplicate_morph_def` (= raw
    /// weights clone, normalized None, DIRTY|LOADED). Unloaded sources report
    /// `Definition` (the caller takes the SetMesh path there instead).
    pub fn singlemorph_duplicate_weights(
        &self,
        dst_joints_changed: u32,
    ) -> Result<(Self, usize), SinglemorphVisualError> {
        let weights = self.weights.duplicate_morph_def()?;
        let mut out = Self {
            weights,
            // Native increments the DESTINATION counter (fresh dst starts 0);
            // the source value is never the base (disclosed).
            joints_changed: dst_joints_changed.wrapping_add(1),
            visual_flags: self.visual_flags & SINGLEMORPH_DUPLICATE_MASK,
            magic: self.magic,
        };
        out.visual_flags &= !SINGLEMORPH_WBOUND_VALID;
        let bytes = out.weights.storage_bytes();
        Ok((out, bytes))
    }
    /// UpdateBoundProc 0x57580 contract: tail-calls the singlemesh bound proc
    /// only. Delegation marker preserving the call order (no new math).
    pub fn forward_singlemesh_bound_update(&self, update_bound: impl FnOnce()) {
        update_bound();
    }
    /// UpdateLodVertices 0x57590 differential order (dirty gate + call order
    /// only). `update_morph` runs `SinglemorphState::update_morph_lod`
    /// (dirty+loaded gate inside); `update_vertices` is the `vtable+0x78`
    /// backend; `singlemesh_tail` is the 0x54f60 backend. Returns whether the
    /// morph update ran (dirty cleared inside weights).
    pub fn singlemorph_update_lod_vertices(
        &mut self,
        record: &MorphRecord,
        lod: usize,
        vertices: &mut [Vertex],
        update_vertices: impl FnOnce(),
        singlemesh_tail: impl FnOnce(),
    ) -> Result<bool, SinglemorphVisualError> {
        let ran = if self.weights.is_dirty() {
            let ran = self.weights.update_morph_lod(record, lod, vertices)?;
            update_vertices();
            ran
        } else {
            false
        };
        singlemesh_tail();
        Ok(ran)
    }
}

/// LoadImage 0x575d0 contract outcome: flag-4 cleared before
/// `SinglemorphState::from_record` (which itself clears flag 4 and installs
/// level0 = 1.0 + LOADED); the bound copy + world-space tail are caller-side;
/// file/singlemesh/`ImageErrFlg` arms stay backend contracts. Residue
/// disclosure: a `Definition` failure leaves the caller's live weights
/// untouched (native clears flag 4 on the live object first, but the flag
/// word has no live clearer on this side — the fresh-install path covers
/// every success).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinglemorphLoadOutcome {
    Loaded { levels: usize, regions: usize },
}

pub fn singlemorph_load_image(
    state: &mut SinglemorphVisual,
    record: &MorphRecord,
    world_space: bool,
) -> Result<SinglemorphLoadOutcome, SinglemorphVisualError> {
    let loaded = SinglemorphState::from_record(record)?;
    let (levels, regions) = (loaded.level_count(), loaded.region_count());
    state.weights = loaded;
    if !world_space {
        state.visual_flags &= !SINGLEMORPH_WBOUND_VALID;
    }
    Ok(SinglemorphLoadOutcome::Loaded { levels, regions })
}

#[cfg(test)]
mod singlemorph_visual_tests {
    use super::*;
    use lhr_assets::{
        MorphChannelRecord, MorphDefinition, MorphLodRecord, MorphRecord, MorphSampleRecord, Vertex,
    };

    fn rec() -> MorphRecord {
        MorphRecord {
            target_count: 2,
            definition: Some(MorphDefinition {
                channel_count: 1,
                lods: vec![MorphLodRecord {
                    channels: vec![MorphChannelRecord {
                        vertex_count: 1,
                        samples: vec![
                            MorphSampleRecord {
                                position: [2., 4., 6.],
                                normal: [1., 2., 3.],
                            },
                            MorphSampleRecord {
                                position: [4., 8., 12.],
                                normal: [3., 2., 1.],
                            },
                        ],
                        index_mode: Some(2),
                        vertex_indices: vec![0],
                    }],
                }],
                trailer_words: [0; 10],
            }),
        }
    }

    #[test]
    fn ctor_publishes_smmr_and_morphbase() {
        let v = SinglemorphVisual::singlemorph_ctor_fields();
        assert_eq!(v.magic, 0x524d_4d53);
        assert_eq!((v.weights.level_count(), v.weights.region_count()), (0, 0));
        assert_eq!(v.weights.flags(), 5);
    }

    #[test]
    fn weight_roundtrip_and_signed_gate_residue() {
        let mut v = SinglemorphVisual::singlemorph_ctor_fields();
        v.weights = SinglemorphState::from_record(&rec()).unwrap();
        let j0 = v.joints_changed;
        v.singlemorph_set_weight(0, 1, 0.5).unwrap();
        assert_eq!(v.joints_changed, j0.wrapping_add(1));
        assert_eq!(v.singlemorph_weight(0, 1), 0.5);
        assert_eq!(v.visual_flags & SINGLEMORPH_WBOUND_VALID, 0);
        assert!(v.singlemorph_set_weight(-1, 0, 1.).is_err());
        assert!(v.singlemorph_set_weight(0, 9, 1.).is_err());
        assert_eq!(v.singlemorph_weight(-1, 0), 0.0);
        assert_eq!(v.singlemorph_weight(0, 9), 0.0);
        let bytes = v.singlemorph_teardown_order();
        assert_eq!(v.weights.weights().len(), 0);
        // Residue is allocator capacity (Vec growth policy), not a fixed
        // byte count: >= 8 pins the fixed record fixture without coupling to
        // allocator behavior.
        assert!(bytes >= 8);
    }

    #[test]
    fn duplicate_clones_raw_weights_without_normalized() {
        let mut v = SinglemorphVisual::singlemorph_ctor_fields();
        v.weights = SinglemorphState::from_record(&rec()).unwrap();
        v.singlemorph_set_weight(0, 1, 1.).unwrap();
        let (dup, _) = v.singlemorph_duplicate_weights(0).unwrap();
        assert_eq!(dup.weights.weights(), &[1., 1.]);
        assert!(dup.weights.normalized_weights().is_none());
        assert!(dup.weights.is_dirty());
    }

    #[test]
    fn bound_update_forwards_in_order() {
        let v = SinglemorphVisual::singlemorph_ctor_fields();
        let mut order = vec![];
        v.forward_singlemesh_bound_update(|| order.push("bound"));
        assert_eq!(order, ["bound"]);
    }

    #[test]
    fn load_image_installs_levels_and_clears_bound_flag() {
        let mut v = SinglemorphVisual::singlemorph_ctor_fields();
        v.visual_flags |= SINGLEMORPH_WBOUND_VALID;
        let out = singlemorph_load_image(&mut v, &rec(), false).unwrap();
        assert_eq!(
            out,
            SinglemorphLoadOutcome::Loaded {
                levels: 2,
                regions: 1
            }
        );
        assert_eq!(v.visual_flags & SINGLEMORPH_WBOUND_VALID, 0);
        assert!(singlemorph_load_image(
            &mut v,
            &MorphRecord {
                target_count: 2,
                definition: None
            },
            false
        )
        .is_err());
    }

    #[test]
    fn dirty_gate_orders_morph_then_virtual_then_base() {
        let mut v = SinglemorphVisual::singlemorph_ctor_fields();
        v.weights = SinglemorphState::from_record(&rec()).unwrap();
        let order = std::cell::RefCell::new(Vec::new());
        let mut verts = vec![Vertex {
            position: [-1.; 3],
            normal: [-1.; 3],
            uv: [0., 0.],
        }];
        assert!(v
            .singlemorph_update_lod_vertices(
                &rec(),
                0,
                &mut verts,
                || order.borrow_mut().push("virtual"),
                || order.borrow_mut().push("base"),
            )
            .unwrap());
        assert_eq!(order.borrow().as_slice(), ["virtual", "base"]);
        assert_eq!(verts[0].position, [2., 4., 6.]);
        order.borrow_mut().clear();
        assert!(!v
            .singlemorph_update_lod_vertices(
                &rec(),
                0,
                &mut verts,
                || order.borrow_mut().push("virtual"),
                || order.borrow_mut().push("base"),
            )
            .unwrap());
        assert_eq!(order.borrow().as_slice(), ["base"]);
    }
}
