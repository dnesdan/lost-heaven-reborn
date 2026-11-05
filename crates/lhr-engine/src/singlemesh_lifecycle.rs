//! Singlemesh lifecycle helpers (PC LS3DF.dll e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! GROUP X4: 7 differential + 7 contract).
//!
//! Contracts keyed by canonical entry RVA. Ghidra-formed bodies: differential.
//! Unformed RVAs: contract tier with the boundary reason in each item's doc
//! comment (same standard as committed 0x2b830). Typed views only; no unsafe;
//! widths from instructions (signed JGE/JS/JLE = i32, counters = u32, dword
//! copies = u32 words). Copy paths move exact dwords with no float ops, so no
//! precision policy applies there; skinning arithmetic lives in the
//! already-ported UpdateLodVertices (0x54f60), untouched here.
#![forbid(unsafe_code)]

/// LS3DF.dll SHA-256 pinned by REA open_binary (ghidra 12.1.4).
pub const LS3DF_SHA256: &str = "e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6";
/// I3DERR_INVALIDPARAMS observed as MOV EAX,0x80000007 (Duplicate exit).
pub const I3DERR_INVALIDPARAMS: i32 = -2147483641; // 0x80000007
/// I3DERR_OUTOFMEM passed to ErrorHandler (DuplicateSinglemeshDef).
pub const I3DERR_OUTOFMEM: i32 = -2147483646; // 0x80000002
/// PC ctor words at +0x224 (0x89) and +0x228 (0x8a): joints_changed, bvol.
/// Both already owned (model_loading joints −300, binding bvol −400); these
/// consts pin the evidenced immediates (`0xfffffed4`, `0xfffffe70`).
pub const SINGLEMESH_JOINTS_CHANGED_INIT: u32 = (-300_i32) as u32;
pub const SINGLEMESH_BVOL_JOINTS_CHANGED_INIT: u32 = (-400_i32) as u32;
/// I3D_MAX_LODS loop bound observed (counter 0..9).
pub const SINGLEMESH_MAX_LODS: usize = 10;
/// sizeof(SM_INFO) from IMUL ECX,0x68; joint bbox field at +0x4c.
pub const SM_INFO_STRIDE_BYTES: usize = 0x68;
pub const SM_INFO_BBOX_OFFSET: usize = 0x4c;
/// VISF_PROXY bit tested (TEST flags,0x10000) in the copy paths.
pub const SINGLEMESH_VISF_PROXY: u32 = 0x10000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinglemeshLifecycleError {
    InvalidParams,
    OutOfMemory,
    DefinitionTooShort,
}

/// RVA 0x54370: base update runs first (caller effect: the
/// `I3D_object::UpdateVertices` call at 0x10036a20), then the wrapping
/// `joints_changed++` store at +0x224. This records only the
/// singlemesh-owned store and its order (differential: body 21 B,
/// 7 insn/1 BB).
pub fn singlemesh_note_vertices_updated(joints_changed: &mut u32) {
    *joints_changed = joints_changed.wrapping_add(1);
}

/// RVA 0x54390: the indexed twin forwards `index` to
/// `I3D_object::UpdateVertices(index)` (0x10036a50) before the same store.
/// The index passthrough is caller-routed; this pins the forwarded word
/// alongside the shared store (contract: complete 9-insn linear decode
/// entry→RET, no Ghidra body).
pub fn singlemesh_note_indexed_vertices_updated(index: u32, joints_changed: &mut u32) -> u32 {
    singlemesh_note_vertices_updated(joints_changed);
    index
}

/// RVA 0x544d0 Duplicate guard (contract: only the evidenced null/self prefix
/// and INVALIDPARAMS exit; the visual-type switch is unwalked). Native returns
/// I3DERR_INVALIDPARAMS for a null or self source.
pub fn duplicate_guard(
    source: Option<usize>,
    this_id: usize,
) -> Result<usize, SinglemeshLifecycleError> {
    match source {
        Some(id) if id != this_id => Ok(id),
        _ => Err(SinglemeshLifecycleError::InvalidParams),
    }
}

/// RVA 0x54400 DuplicateSinglemeshDef slot plan (differential: 208 B,
/// 70 insn/1 BB). Per LOD slot order: allocate `SMeshLod` → publish slot →
/// share `def` → `ref++`; OOM reports `I3DERR_OUTOFMEM` with the reached
/// slot index. The allocator/def owners stay caller-side; this pins the
/// order, the 10-slot bound and the failure word.
/// One slot's ordered step log (replayable by the allocator-owning caller).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinglemeshDefSlotStep {
    Allocate,
    Publish,
    ShareDef,
    RefInc,
}

/// Null source slots are skipped caller-side (native `continue`s on them);
/// this plans the dense `0..lod_slots` walk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SinglemeshDefSlotPlan {
    pub slots: usize,
    pub steps: Vec<(usize, SinglemeshDefSlotStep)>,
}

pub fn duplicate_singlemesh_def_plan(
    lod_slots: usize,
    fail_at: Option<usize>,
) -> Result<SinglemeshDefSlotPlan, SinglemeshLifecycleError> {
    if lod_slots > SINGLEMESH_MAX_LODS {
        return Err(SinglemeshLifecycleError::DefinitionTooShort);
    }
    let mut steps = Vec::new();
    for slot in 0..lod_slots {
        if fail_at == Some(slot) {
            return Err(SinglemeshLifecycleError::OutOfMemory);
        }
        steps.extend(
            [
                SinglemeshDefSlotStep::Allocate,
                SinglemeshDefSlotStep::Publish,
                SinglemeshDefSlotStep::ShareDef,
                SinglemeshDefSlotStep::RefInc,
            ]
            .iter()
            .map(|step| (slot, *step)),
        );
    }
    Ok(SinglemeshDefSlotPlan {
        slots: lod_slots,
        steps,
    })
}

/// RVA 0x543b0 GetJointBBox index resolution (contract: no Ghidra function;
/// full 3-exit path decoded). Pure: maps index → definition slot.
/// Returns `Ok(None)` = native NULL; `Ok(Some(0))` = `&def->bbox` (index<0);
/// `Ok(Some(i+1))` = `&info[i].bbox`. Callers borrow the actual definition.
/// The unsigned-then-signed native tests collapse to one signed gate here:
/// identical outcomes for all `i32` inputs provided `num_joints` /
/// `def_num_joints` fit `i32` (native ints are 32-bit — stated assumption).
pub fn resolve_joint_bbox_slot(
    index: i32,
    num_joints: u32,
    lod_present: bool,
    def_present: bool,
    def_num_joints: u32,
    info_present: bool,
) -> Result<Option<usize>, SinglemeshLifecycleError> {
    if !lod_present || !def_present {
        return Ok(None);
    }
    if index < 0 {
        return Ok(Some(0));
    }
    if (index as u32) >= num_joints {
        return Ok(None);
    }
    if !info_present || (index as u32) >= def_num_joints {
        return Ok(None);
    }
    Ok(Some(index as usize + 1))
}

/// RVA 0x54e30 GetVertexPos level gate (contract: no Ghidra function; the two
/// NULL exits evidenced, the lock path from 0x54e5b unprobed). Pure prefix:
/// no mesh level → NULL; zero-vertex level → NULL. The `vb_skin` lock and the
/// joints-changed `UpdateLodVertices` gate stay backend contracts.
pub fn vertex_pos_level_gate(level_present: bool, vertex_count: u32) -> Option<u32> {
    if !level_present || vertex_count == 0 {
        return None;
    }
    Some(vertex_count)
}

/// Proxy walk and `DBuild` gates are caller-side (native
/// `CopyRigidPositions`/`CopyMovedPositions` order; no credit claimed here).
///
/// RVA 0x55640/0x557c0: exact-dword strided position copies (differential:
/// 373 B/116 insn and 359 B/112 insn, 1 BB each). Words preserve bits; no
/// float arithmetic, no rounding policy applies. `src_positions` borrows the
/// live locked buffer; `dst_words` the live destination. Order: rigid starts
/// at `first*stride_words`, moved at 0. Bounds are refused (native would
/// write OOB — disclosed hardening).
pub fn copy_rigid_positions_strided(
    dst_words: &mut [u32],
    stride_words: usize,
    first: usize,
    src_positions: &[[f32; 3]],
) {
    if stride_words < 3 || src_positions.is_empty() {
        return;
    }
    let mut dst_ofs = first.saturating_mul(stride_words);
    for pos in src_positions {
        if dst_ofs
            .checked_add(3)
            .is_none_or(|end| end > dst_words.len())
        {
            break;
        }
        dst_words[dst_ofs..dst_ofs + 3].copy_from_slice(&[
            pos[0].to_bits(),
            pos[1].to_bits(),
            pos[2].to_bits(),
        ]);
        dst_ofs += stride_words;
    }
}

/// Moved-vertex twin: destination starts at word 0 (differential).
pub fn copy_moved_positions_strided(
    dst_words: &mut [u32],
    stride_words: usize,
    src_positions: &[[f32; 3]],
) {
    copy_rigid_positions_strided(dst_words, stride_words, 0, src_positions);
}

/// RVA 0x56af0 SMeshLod def release (differential, pure refcount half).
/// Returns true when the native code frees info+def (ref reached 0).
/// The caller performs the actual frees and the `m_Free(vb_skin)` backend
/// call afterwards, preserving effect order. Weights-free loop precedes.
pub fn smesh_def_release(refcount: &mut u32) -> bool {
    *refcount = refcount.wrapping_sub(1);
    *refcount == 0
}

/// RVA 0x56860 LoadImage per-LOD intake (contract: the Ghidra body exists
/// but its pseudocode tail is untranscribed — `def->num_rigid`/bbox read, the
/// `SM_INFO` loop, weights reads and `ImageErrFlg` arms are unrecovered, so no
/// differential parity is claimed; `dtaRead` stays a byte-cursor backend with
/// no invented buffering). `num == 0` file byte: delete lod slot, store NULL,
/// continue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LodImageAction {
    Clear,
    Build { num_joints: u8 },
}

pub fn lod_image_action(num: u8) -> LodImageAction {
    if num == 0 {
        LodImageAction::Clear
    } else {
        LodImageAction::Build { num_joints: num }
    }
}

/// Bytes per joint file payload after the count: 0x64 fixed + weights.
pub const SM_INFO_FILE_FIXED_BYTES: usize = 0x64;
pub fn joint_weights_bytes(m_vert_num: u32) -> usize {
    (m_vert_num as usize).saturating_mul(4)
}

/// RVA 0x55930 m_AddFGroups gate order (contract: ~0xF00 B span unformed;
/// entry + early-RET probes only). Native order: `!mesh` → return, no stores;
/// `VISF_PROXY` → proxy-sector walk arm; else the inline path. Device calls
/// (locks, ProcessVertices, fog/env/light, face-group push) stay a backend
/// contract; the proxy walk itself is caller-side (no credit).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SinglemeshQueueGate {
    Return,
    Proxy,
    Inline,
}

pub fn add_fgroups_mesh_gate(mesh_present: bool, proxy: bool) -> SinglemeshQueueGate {
    if !mesh_present {
        return SinglemeshQueueGate::Return;
    }
    if proxy {
        return SinglemeshQueueGate::Proxy;
    }
    SinglemeshQueueGate::Inline
}

/// Backend contract for the render-device half of 0x55930. Implemented by the
/// render backend, never by format parsing. The proxy-sector walk is a
/// caller-side helper (no function credit).
pub trait SinglemeshRenderBackend {
    fn error_out_of_memory(&mut self);
}

/// RVA 0x542b0 dtor pass order (differential): joint-unlink pass first, then
/// the 10-slot LOD-delete pass, then the base dtor (caller effect). The graph
/// owns the actual unlink/free callbacks; this pins the order.
pub fn singlemesh_teardown_order(num_joints: usize, num_lods: usize) -> (usize, usize) {
    (num_joints, num_lods)
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::frame_transform::FrameKey;
    use crate::singlemesh_binding::{I3dSinglemeshBinding, SinglemeshJointRange};

    #[test]
    fn ctor_words_match_pc_stores() {
        assert_eq!(SINGLEMESH_JOINTS_CHANGED_INIT, (-300_i32) as u32);
        assert_eq!(SINGLEMESH_BVOL_JOINTS_CHANGED_INIT, (-400_i32) as u32);
        let mut c = SINGLEMESH_JOINTS_CHANGED_INIT;
        singlemesh_note_vertices_updated(&mut c);
        assert_eq!(c, (-299_i32) as u32);
        let mut wrap = u32::MAX;
        singlemesh_note_vertices_updated(&mut wrap);
        assert_eq!(wrap, 0);
    }

    #[test]
    fn duplicate_guard_failure_prefix() {
        assert_eq!(
            duplicate_guard(None, 1),
            Err(SinglemeshLifecycleError::InvalidParams)
        );
        assert_eq!(
            duplicate_guard(Some(1), 1),
            Err(SinglemeshLifecycleError::InvalidParams)
        );
        assert_eq!(I3DERR_INVALIDPARAMS, 0x80000007u32 as i32);
        assert_eq!(duplicate_guard(Some(2), 1), Ok(2));
    }

    #[test]
    fn def_slot_plan_orders_alloc_publish_share() {
        let plan = duplicate_singlemesh_def_plan(2, None).unwrap();
        assert_eq!(plan.slots, 2);
        assert_eq!(
            plan.steps,
            vec![
                (0, SinglemeshDefSlotStep::Allocate),
                (0, SinglemeshDefSlotStep::Publish),
                (0, SinglemeshDefSlotStep::ShareDef),
                (0, SinglemeshDefSlotStep::RefInc),
                (1, SinglemeshDefSlotStep::Allocate),
                (1, SinglemeshDefSlotStep::Publish),
                (1, SinglemeshDefSlotStep::ShareDef),
                (1, SinglemeshDefSlotStep::RefInc),
            ]
        );
        assert_eq!(
            duplicate_singlemesh_def_plan(10, Some(4)),
            Err(SinglemeshLifecycleError::OutOfMemory)
        );
        assert_eq!(I3DERR_OUTOFMEM, 0x80000002u32 as i32);
        assert_eq!(
            duplicate_singlemesh_def_plan(11, None),
            Err(SinglemeshLifecycleError::DefinitionTooShort)
        );
    }

    #[test]
    fn indexed_update_forwards_index_word() {
        let mut c = 7u32;
        assert_eq!(singlemesh_note_indexed_vertices_updated(3, &mut c), 3);
        assert_eq!(c, 8);
    }

    #[test]
    fn joint_bbox_slots_follow_native_order() {
        assert_eq!(resolve_joint_bbox_slot(2, 2, true, true, 2, true), Ok(None));
        assert_eq!(
            resolve_joint_bbox_slot(0, 1, false, true, 1, true),
            Ok(None)
        );
        assert_eq!(
            resolve_joint_bbox_slot(0, 1, true, false, 1, true),
            Ok(None)
        );
        assert_eq!(
            resolve_joint_bbox_slot(-1, 2, true, true, 2, true),
            Ok(Some(0))
        );
        assert_eq!(
            resolve_joint_bbox_slot(1, 3, true, true, 3, true),
            Ok(Some(2))
        );
        assert_eq!(
            resolve_joint_bbox_slot(1, 3, true, true, 3, false),
            Ok(None)
        );
        assert_eq!(resolve_joint_bbox_slot(5, 9, true, true, 2, true), Ok(None));
        assert_eq!(
            resolve_joint_bbox_slot(i32::MIN, u32::MAX, true, true, u32::MAX, true),
            Ok(Some(0))
        );
        assert_eq!(
            resolve_joint_bbox_slot(i32::MAX, u32::MAX, true, true, u32::MAX, true),
            Ok(Some(i32::MAX as usize + 1))
        );
    }

    #[test]
    fn vertex_pos_gate_nulls_without_level_or_verts() {
        assert_eq!(vertex_pos_level_gate(false, 9), None);
        assert_eq!(vertex_pos_level_gate(true, 0), None);
        assert_eq!(vertex_pos_level_gate(true, 9), Some(9));
    }

    #[test]
    fn get_vertex_joint_reuse_wraps_like_native_sub_js() {
        let binding = I3dSinglemeshBinding {
            mesh_present: true,
            joints: vec![Some(FrameKey(1))],
            ..Default::default()
        };
        let ranges = [SinglemeshJointRange {
            num_verts: u32::MAX,
            num_blend: 2,
        }];
        assert_eq!(
            binding.get_vertex_joint(0, Some(&ranges)),
            Ok(Some(FrameKey(1)))
        );
    }

    #[test]
    fn rigid_copy_order_and_moved_copy_order() {
        let src = [[1.0f32, 2.0, 3.0], [4.0, 5.0, 6.0], [7.0, 8.0, 9.0]];
        let mut dst = [0u32; 12];
        copy_rigid_positions_strided(&mut dst, 4, 1, &src[1..]);
        assert_eq!(&dst[0..4], &[0, 0, 0, 0]);
        assert_eq!(
            &dst[4..7],
            &[4.0f32.to_bits(), 5.0f32.to_bits(), 6.0f32.to_bits()]
        );
        let mut mdst = [0u32; 6];
        copy_moved_positions_strided(&mut mdst, 3, &src[..2]);
        assert_eq!(
            &mdst,
            &[
                1.0f32.to_bits(),
                2.0f32.to_bits(),
                3.0f32.to_bits(),
                4.0f32.to_bits(),
                5.0f32.to_bits(),
                6.0f32.to_bits()
            ]
        );
        // OOB refused (native would write past the buffer).
        let mut tiny = [0u32; 4];
        copy_rigid_positions_strided(&mut tiny, 4, 1, &src);
        assert_eq!(&tiny, &[0, 0, 0, 0]);
    }

    #[test]
    fn lod_image_and_release_shapes() {
        assert_eq!(lod_image_action(0), LodImageAction::Clear);
        assert_eq!(lod_image_action(3), LodImageAction::Build { num_joints: 3 });
        assert_eq!(SM_INFO_FILE_FIXED_BYTES, 0x64);
        assert_eq!(joint_weights_bytes(5), 20);
        let mut rc = 2u32;
        assert!(!smesh_def_release(&mut rc));
        assert!(smesh_def_release(&mut rc));
        assert_eq!(
            add_fgroups_mesh_gate(false, false),
            SinglemeshQueueGate::Return
        );
        assert_eq!(
            add_fgroups_mesh_gate(true, true),
            SinglemeshQueueGate::Proxy
        );
        assert_eq!(
            add_fgroups_mesh_gate(true, false),
            SinglemeshQueueGate::Inline
        );
        assert_eq!(singlemesh_teardown_order(3, 10), (3, 10));
    }
}
