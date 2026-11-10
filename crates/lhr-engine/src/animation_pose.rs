//! LS3DF `I3D_pose` / `I3D_pose_set` (PC LS3DF.dll
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
//! RVAs 0x45930-0x45d80: 7 differential + 9 contract).
//!
//! Class names `I3D_pose` / `I3D_pose_set` are evidenced (PS2 linker,
//! I3D.h:1141-1146,1199-1202). Method names (`Release`, `Clear`, `SetPos`, …)
//! are Insanity3D-attributed working names (I3D.h:1152) — NOT evidenced
//! original symbols; every contract below is keyed by SHA+RVA.
//!
//! Precision policy: no float arithmetic here — bitwise `[f32; N]` copies,
//! one exact `1.0f` constant (`0x3F800000`), `roll` passthrough to the quat
//! backend. NaN payloads preserved by copy. Widths from instructions:
//! `ref`/sizes are `i32` (`DEC`+`JNS`, signed `JL`/`JLE`), flag words `u32`,
//! `name` nullable bytes.
#![forbid(unsafe_code)]

use std::cell::RefCell;
use std::rc::Rc;

/// I3D.h:1136 (meanings guessed; values are header data).
pub const POSEF_POS: u32 = 0x1;
/// I3D.h:1137.
pub const POSEF_SCALE: u32 = 0x2;
/// I3D.h:1138. Rotation turns (adds to) the animation's rotation.
pub const POSEF_ROT_ADD: u32 = 0x4;
/// I3D.h:1139. Rotation replaces the animation's rotation.
pub const POSEF_ROT: u32 = 0x8;

/// Working name for the `POSEF_POS` bit test (mafia-re ls3df/I3D/I3D.h:1138).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is read by `I3D_interpolator::Update` (mafia-re
/// ls3df/I3D/anim/I3d_interpolator.cpp:227,244) to decide whether the pose
/// carries a position. Reports whether the pose value is active; no state
/// moves.
pub fn pose_pos_enabled(flags: u32) -> bool {
    flags & POSEF_POS != 0
}
/// Working name for the `POSEF_SCALE` bit test (mafia-re ls3df/I3D/I3D.h:1139).
/// The reference records no LS3DF FUNCTION address for a standalone reader;
/// the bit is read by `I3D_interpolator::Update` (mafia-re
/// ls3df/I3D/anim/I3d_interpolator.cpp:229,251) to decide whether the pose
/// carries a scale. Reports whether the pose value is active; no state moves.
pub fn pose_scale_enabled(flags: u32) -> bool {
    flags & POSEF_SCALE != 0
}

/// Initial `ref` stored by both constructors (0x45930, 0x45b90 dossiers).
pub const POSE_INITIAL_REFCOUNT: i32 = 1;
/// `C_list` growth quantum stored at set+0x14 by 0x45b90; used by 0x45cb0 growth.
pub const POSE_SET_GROWTH_QUANTUM: u32 = 0x20;
/// `I3D_OK` — evidenced as `XOR EAX,EAX` on all success paths of 0x45d80.
pub const I3D_OK: i32 = 0;
/// `I3DERR_OBJECTNOTFOUND` — evidenced as `MOV EAX,0x80000006` at 0x10045dfe.
pub const I3DERR_OBJECTNOTFOUND: i32 = 0x8000_0006u32 as i32;

/// Exact native diagnostic words (failure words preserved verbatim, spacing included).
pub const REFCOUNT_MSG_POSE: &str = "!! RefCount:0x%x   I3D_pose";
/// Exact native diagnostic words (4 spaces, @0x100a83f0).
pub const REFCOUNT_MSG_POSE_SET: &str = "!! RefCount:0x%x    I3D_pose_set";
/// Exact native diagnostic words (2 spaces, @0x100a83c8).
pub const OUT_OF_MEMORY_SET_NAME_MSG: &str = "!! Out of memory  I3D_pose::SetName()";

/// Which vtable owner a refcount-underflow message belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PoseOwner {
    Pose,
    PoseSet,
}

/// Ordered observable effects, pushed in exact native effect order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PoseEffect {
    /// `--ref` went negative; native logs then returns 0 WITHOUT freeing.
    RefUnderflow { owner: PoseOwner },
    /// Old `name` freed (`delete[]`) before any copy attempt.
    NameFreed,
    /// New name bytes stored (native `new[]` result stored before null check).
    NameStored { len: usize },
    /// Pose handle appended to the set vector.
    PosePushed,
    /// `pose->ref++` after the push (x86 `INC`: wraps).
    PoseRefIncremented { refcount: i32 },
    /// Swap-with-last removal done (size already decremented) before Release.
    PoseRemoved { index: usize },
    /// Member released during set teardown, in ascending index order.
    MemberReleased { index: usize, refcount: i32 },
}

impl PoseEffect {
    /// Exact native `dbgPrintf` text for the loggable failure effects.
    pub fn native_message(&self, this: u32) -> Option<String> {
        match self {
            PoseEffect::RefUnderflow {
                owner: PoseOwner::Pose,
            } => Some(format!("!! RefCount:0x{:x}   I3D_pose", this)),
            PoseEffect::RefUnderflow {
                owner: PoseOwner::PoseSet,
            } => Some(format!("!! RefCount:0x{:x}    I3D_pose_set", this)),
            _ => None,
        }
    }
}

/// Backend contract for `S_quat::SetDir` (separate native function — helper,
/// no function credit). `roll` passes through unmodified.
pub trait DirToQuatBackend {
    fn build_quat(&self, dir: [f32; 3], roll: f32) -> [f32; 4];
}

/// `I3D_pose`: 0x38 bytes on PC (I3D.h:1141). Member names are upstream
/// guesses; offsets are used here.
#[derive(Clone, Debug, PartialEq)]
pub struct I3dPose {
    refcount: i32,
    name: Option<Vec<u8>>,
    flags: u32,
    pos: [f32; 3],
    scale: [f32; 3],
    /// Raw words as stored; first word is `1.0f` at rest. No axis claim.
    rot: [f32; 4],
}

/// Shared pose handle. Native identity is the pointer; `Rc::ptr_eq` is the
/// exact equivalent. The explicit `refcount` field is authoritative (it can
/// go negative — `Rc` cannot).
pub type PoseHandle = Rc<RefCell<I3dPose>>;

impl I3dPose {
    /// Working-name ctor. DIFFERENTIAL — RVA 0x45930 (23 ins, 1 BB).
    /// Driver factory 0x19320 `I3D_driver::CreatePose` (mafia-re
    /// `ls3df/I3D/I3d_driver.cpp:1419`, evidenced PS2 original class name):
    /// operator-new over this ctor; the caller owns the fresh pose (no null
    /// check in the factory body; no native handler at runtime).
    /// `unit_vector` is a backend-contract input: the datum copied from the
    /// engine's static unit vector (@0x101c5188/8c/90; value not observed —
    /// never invented here).
    pub fn new_in(unit_vector: [f32; 3]) -> Self {
        Self {
            refcount: POSE_INITIAL_REFCOUNT,
            name: None,
            flags: 0,
            pos: [0.0, 0.0, 0.0],
            scale: unit_vector,
            rot: [f32::from_bits(0x3F80_0000), 0.0, 0.0, 0.0],
        }
    }

    pub fn refcount(&self) -> i32 {
        self.refcount
    }
    pub fn flags(&self) -> u32 {
        self.flags
    }
    pub fn pos(&self) -> [f32; 3] {
        self.pos
    }
    pub fn scale(&self) -> [f32; 3] {
        self.scale
    }
    pub fn rot(&self) -> [f32; 4] {
        self.rot
    }
    /// Inline `GetName` helper (I3d_pose.cpp:168-176): `name ? name : ""`.
    /// No credit.
    pub fn name_or_empty(&self) -> &[u8] {
        self.name.as_deref().unwrap_or(b"")
    }

    /// Decrement-and-store (`DEC`+`MOV` before any branch). Pure; no log.
    fn dec_ref(&mut self) -> i32 {
        self.refcount = self.refcount.wrapping_sub(1);
        self.refcount
    }

    /// Working-name `Release`. DIFFERENTIAL — RVA 0x45980 (25 ins, 5 BB).
    /// Negative path returns 0 with the count left negative; zero path
    /// returns 0 and the caller must drop the handle (`delete this` runs the
    /// inlined `~I3D_pose` = `delete[] name`, then frees the object); else
    /// returns `ref`. `RET 0x4` cleanup is native-only.
    pub fn release(&mut self, log: &mut Vec<PoseEffect>) -> i32 {
        let refcount = self.dec_ref();
        if refcount < 0 {
            log.push(PoseEffect::RefUnderflow {
                owner: PoseOwner::Pose,
            });
            return 0;
        }
        if refcount == 0 {
            return 0;
        }
        refcount
    }

    /// Working-name `Clear`. CONTRACT — RVA 0x459d0 (vtable slot only, no
    /// Ghidra body; I3d_pose.cpp:43-48).
    pub fn clear(&mut self) {
        self.flags = 0;
    }

    /// Working-name `SetRot`. CONTRACT — RVA 0x459e0 (I3d_pose.cpp:52-59).
    /// Null clears only `POSEF_ROT_ADD` (`POSEF_ROT` survives — the trap).
    pub fn set_rot(&mut self, q: Option<[f32; 4]>) {
        if let Some(words) = q {
            self.rot = words;
            self.flags = (self.flags & !POSEF_ROT) | POSEF_ROT_ADD;
        } else {
            self.flags &= !POSEF_ROT_ADD;
        }
    }

    /// Working-name `SetRotAbs`. CONTRACT — RVA 0x45a30 (I3d_pose.cpp:63-70).
    pub fn set_rot_abs(&mut self, q: Option<[f32; 4]>) {
        if let Some(words) = q {
            self.rot = words;
            self.flags = (self.flags & !POSEF_ROT_ADD) | POSEF_ROT;
        } else {
            self.flags &= !(POSEF_ROT_ADD | POSEF_ROT);
        }
    }

    /// Working-name `SetDir` (pointer). CONTRACT — RVA 0x45a80
    /// (I3d_pose.cpp:74-81). Null never calls the backend.
    pub fn set_dir_ptr(
        &mut self,
        dir: Option<[f32; 3]>,
        roll: f32,
        backend: &dyn DirToQuatBackend,
    ) {
        if let Some(d) = dir {
            self.rot = backend.build_quat(d, roll);
            self.flags = (self.flags & !POSEF_ROT) | POSEF_ROT_ADD;
        } else {
            self.flags &= !(POSEF_ROT_ADD | POSEF_ROT);
        }
    }

    /// Working-name `SetDir` (reference). CONTRACT — RVA 0x45ac0
    /// (I3d_pose.cpp:86-90). `POSEF_ROT` is left as-is (differs from others).
    pub fn set_dir(&mut self, dir: [f32; 3], roll: f32, backend: &dyn DirToQuatBackend) {
        self.rot = backend.build_quat(dir, roll);
        self.flags |= POSEF_ROT_ADD;
    }

    /// Working-name `SetDirAbs`. CONTRACT — RVA 0x45ae0 (I3d_pose.cpp:94-98).
    pub fn set_dir_abs(&mut self, dir: [f32; 3], roll: f32, backend: &dyn DirToQuatBackend) {
        self.rot = backend.build_quat(dir, roll);
        self.flags = (self.flags & !POSEF_ROT_ADD) | POSEF_ROT;
    }

    /// Working-name `SetPos`. CONTRACT — RVA 0x45b10 (I3d_pose.cpp:102-109).
    pub fn set_pos(&mut self, p: Option<[f32; 3]>) {
        if let Some(v) = p {
            self.pos = v;
            self.flags |= POSEF_POS;
        } else {
            self.flags &= !POSEF_POS;
        }
    }

    /// Working-name `SetScale`. CONTRACT — RVA 0x45b50 (I3d_pose.cpp:113-120).
    pub fn set_scale(&mut self, s: Option<[f32; 3]>) {
        if let Some(v) = s {
            self.scale = v;
            self.flags |= POSEF_SCALE;
        } else {
            self.flags &= !POSEF_SCALE;
        }
    }

    /// Inlined `SetName` helper (I3d_pose.cpp:124-134, also inlined in the
    /// 0x45cb0 body). Helper phase — no function credit. Order pinned by the
    /// 0x45cb0 dossier: free old first, then copy; the stored result precedes
    /// any failure log.
    /// DIVERGENCE: native leaves `name` dangling when `n` is null and stores
    /// null + logs on OOM (old bytes gone either way); Rust stores `None`.
    /// Host `Vec` growth cannot fail like `new[]`, so the OOM log path is
    /// modeled but unreachable-in-practice here.
    pub(crate) fn set_name(&mut self, n: Option<&[u8]>, log: &mut Vec<PoseEffect>) {
        if self.name.is_some() {
            log.push(PoseEffect::NameFreed);
        }
        self.name = None;
        if let Some(bytes) = n {
            self.name = Some(bytes.to_vec());
            log.push(PoseEffect::NameStored { len: bytes.len() });
        }
    }
}

/// `I3D_pose_set`: 0x18 bytes on PC (I3D.h:1199).
#[derive(Clone, Debug, Default)]
pub struct I3dPoseSet {
    refcount: i32,
    poses: Vec<PoseHandle>,
}

impl I3dPoseSet {
    /// Working-name ctor. DIFFERENTIAL — RVA 0x45b90 (9 ins, 1 BB).
    /// Driver factory 0x19380 `I3D_driver::CreatePoseSet` (mafia-re
    /// `ls3df/I3D/I3d_driver.cpp:1426`, evidenced PS2 original class name):
    /// operator-new over this ctor; the caller owns the fresh set (no null
    /// check in the factory body; no native handler at runtime).
    /// Growth quantum `0x20` recorded for growth accounting (host `Vec`
    /// owns real growth).
    pub fn new() -> Self {
        Self {
            refcount: POSE_INITIAL_REFCOUNT,
            poses: Vec::new(),
        }
    }

    pub fn refcount(&self) -> i32 {
        self.refcount
    }
    pub fn len(&self) -> usize {
        self.poses.len()
    }
    pub fn is_empty(&self) -> bool {
        self.poses.is_empty()
    }
    pub fn growth_quantum(&self) -> u32 {
        POSE_SET_GROWTH_QUANTUM
    }

    fn dec_ref(&mut self) -> i32 {
        self.refcount = self.refcount.wrapping_sub(1);
        self.refcount
    }

    /// Working-name `Release`. DIFFERENTIAL — RVA 0x45c40 (23 ins, 5 BB).
    /// Zero path runs the 0x45bc0 teardown through `destroy`, then the
    /// caller drops the set.
    pub fn release(&mut self, log: &mut Vec<PoseEffect>) -> i32 {
        let refcount = self.dec_ref();
        if refcount < 0 {
            log.push(PoseEffect::RefUnderflow {
                owner: PoseOwner::PoseSet,
            });
            return 0;
        }
        if refcount == 0 {
            self.destroy(log);
            return 0;
        }
        refcount
    }

    /// Working-name dtor. DIFFERENTIAL — RVA 0x45bc0 (43 ins, 5 BB).
    /// Ascending member `Release` loop (each member goes through the same
    /// virtual Release as 0x45980, underflow logging included), then the
    /// array is freed and ptr/size/capacity zeroed (host `Vec::clear`).
    /// SEH juggling and the vptr store are native-only. Growth word
    /// untouched, as observed.
    pub fn destroy(&mut self, log: &mut Vec<PoseEffect>) {
        for (index, pose) in self.poses.iter().enumerate() {
            pose.borrow_mut().release(log);
            log.push(PoseEffect::MemberReleased {
                index,
                refcount: pose.borrow().refcount(),
            });
        }
        self.poses.clear();
    }

    /// Working-name `GetFrameName`. CONTRACT — RVA 0x45c80 (vtable slot only,
    /// no Ghidra body; I3d_pose.cpp:171-176). `index` is `i32`; the
    /// `>= size` bound is modeled with unsigned size comparison
    /// (negative → empty). Returns an owned copy: native hands out a raw
    /// pointer (dangling on missing names — not reproduced).
    pub fn frame_name(&self, index: i32) -> Vec<u8> {
        if index < 0 || (index as usize) >= self.poses.len() {
            return Vec::new();
        }
        self.poses[index as usize].borrow().name_or_empty().to_vec()
    }

    /// Working-name `AddPose`. DIFFERENTIAL — RVA 0x45cb0 (75 ins, 11 BB).
    /// Order: `SetName` (free-then-copy) → grow-if-needed → append → size++
    /// → `ref++` (wrapping) LAST. Takes an already-created handle (native
    /// receives the pointer).
    pub fn add_pose(
        &mut self,
        pose: PoseHandle,
        frame_name: Option<&[u8]>,
        log: &mut Vec<PoseEffect>,
    ) {
        pose.borrow_mut().set_name(frame_name, log);
        self.poses.push(pose.clone());
        log.push(PoseEffect::PosePushed);
        let refcount = {
            let mut p = pose.borrow_mut();
            p.refcount = p.refcount.wrapping_add(1);
            p.refcount
        };
        log.push(PoseEffect::PoseRefIncremented { refcount });
    }

    /// Working-name `DeletePose`. DIFFERENTIAL — RVA 0x45d80 (67 ins, 11 BB).
    /// Pointer-identity search (`Rc::ptr_eq`); swap-with-last fill (NOT
    /// order-preserving remove); size decrement BEFORE the virtual Release.
    /// Returns `I3D_OK` on hit regardless of the Release value (native
    /// ignores it), `I3DERR_OBJECTNOTFOUND` on miss.
    pub fn delete_pose(&mut self, pose: &PoseHandle, log: &mut Vec<PoseEffect>) -> i32 {
        let found = self.poses.iter().position(|p| Rc::ptr_eq(p, pose));
        match found {
            Some(index) => {
                self.poses.swap_remove(index);
                log.push(PoseEffect::PoseRemoved { index });
                pose.borrow_mut().release(log);
                I3D_OK
            }
            None => I3DERR_OBJECTNOTFOUND,
        }
    }
}

impl Drop for I3dPoseSet {
    /// Non-logging teardown. The evidenced logging path is `destroy`; `Drop`
    /// cannot carry a log sink, so underflow diagnostics inside implicit
    /// drops are unrepresentable (disclosed divergence — native dtor
    /// Releases can `dbgPrintf`).
    fn drop(&mut self) {
        for pose in &self.poses {
            pose.borrow_mut().dec_ref();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct CallLogBackend {
        calls: RefCell<Vec<([f32; 3], u32)>>,
        canned: [f32; 4],
    }
    impl CallLogBackend {
        fn new(canned: [f32; 4]) -> Self {
            Self {
                calls: RefCell::new(Vec::new()),
                canned,
            }
        }
    }
    impl DirToQuatBackend for CallLogBackend {
        fn build_quat(&self, dir: [f32; 3], roll: f32) -> [f32; 4] {
            self.calls.borrow_mut().push((dir, roll.to_bits()));
            self.canned
        }
    }
    fn handle(pose: I3dPose) -> PoseHandle {
        Rc::new(RefCell::new(pose))
    }
    fn unit() -> [f32; 3] {
        [1.0, 0.0, 0.0]
    }

    #[test]
    fn ctor_words_match_0x45930_body() {
        let p = I3dPose::new_in(unit());
        assert_eq!(p.refcount(), POSE_INITIAL_REFCOUNT);
        assert_eq!(p.flags(), 0);
        assert_eq!(p.pos(), [0.0, 0.0, 0.0]);
        assert_eq!(p.scale(), unit());
        assert_eq!(p.rot()[0].to_bits(), 0x3F80_0000);
        assert_eq!(&p.rot()[1..], &[0.0, 0.0, 0.0]);
        assert_eq!(p.name_or_empty(), b"");
        let q = I3dPose::new_in([0.0, 1.0, 0.0]);
        assert_eq!(q.scale(), [0.0, 1.0, 0.0]);
        assert_eq!(p.scale(), unit());
    }

    #[test]
    fn release_sequence_pins_store_then_branch_with_negative_residue() {
        let mut p = I3dPose::new_in(unit());
        p.refcount = 3;
        let mut log = Vec::new();
        assert_eq!(p.release(&mut log), 2);
        assert_eq!(p.release(&mut log), 1);
        // Zero is terminal natively (`delete this`); the host models the
        // underflow arm from a synthetic zero-count precondition instead of
        // a use-after-free second call (disclosed).
        assert_eq!(p.release(&mut log), 0);
        assert!(log.is_empty());
        let mut q = I3dPose::new_in(unit());
        q.refcount = 0;
        assert_eq!(q.release(&mut log), 0);
        assert_eq!(q.refcount(), -1); // MOV [ref],EAX precedes JNS
        assert_eq!(q.release(&mut log), 0);
        assert_eq!(q.refcount(), -2);
        assert_eq!(
            log,
            vec![
                PoseEffect::RefUnderflow {
                    owner: PoseOwner::Pose
                },
                PoseEffect::RefUnderflow {
                    owner: PoseOwner::Pose
                },
            ]
        );
        assert_eq!(
            PoseEffect::RefUnderflow {
                owner: PoseOwner::Pose
            }
            .native_message(0x1234_abcd)
            .as_deref(),
            Some("!! RefCount:0x1234abcd   I3D_pose")
        );
        assert_eq!(REFCOUNT_MSG_POSE, "!! RefCount:0x%x   I3D_pose");
        assert_eq!(REFCOUNT_MSG_POSE_SET, "!! RefCount:0x%x    I3D_pose_set");
        assert_eq!(
            OUT_OF_MEMORY_SET_NAME_MSG,
            "!! Out of memory  I3D_pose::SetName()"
        );
    }

    #[test]
    fn guard_words_prove_no_neighbor_clobber() {
        #[repr(C)]
        struct Guarded {
            pre: u64,
            pose: I3dPose,
            post: u64,
        }
        let mut g = Guarded {
            pre: 0xA5A5_A5A5_A5A5_A5A5,
            pose: I3dPose::new_in(unit()),
            post: 0x5A5A_5A5A_5A5A_5A5A,
        };
        let backend = CallLogBackend::new([0.0, 0.0, 0.0, 1.0]);
        g.pose.set_pos(Some([1.0, 2.0, 3.0]));
        g.pose.set_scale(Some([4.0, 5.0, 6.0]));
        g.pose.set_rot(Some([0.5, 0.5, 0.5, 0.5]));
        g.pose.set_dir([0.0, 0.0, 1.0], 0.25, &backend);
        g.pose.set_dir_abs([0.0, 0.0, 1.0], 0.5, &backend);
        g.pose.clear();
        assert_eq!(g.pre, 0xA5A5_A5A5_A5A5_A5A5);
        assert_eq!(g.post, 0x5A5A_5A5A_5A5A_5A5A);
        assert_eq!(g.pose.flags(), 0);
        assert_eq!(backend.calls.borrow().len(), 2);
    }

    #[test]
    fn flag_matrix_pins_null_traps() {
        let backend = CallLogBackend::new([0.1, 0.2, 0.3, 0.4]);
        let mut p = I3dPose::new_in(unit());
        p.flags = POSEF_ROT | POSEF_POS;
        p.set_rot(None);
        assert_eq!(p.flags(), POSEF_ROT | POSEF_POS);
        p.set_rot(Some([1.0, 0.0, 0.0, 0.0]));
        assert_eq!(p.flags(), POSEF_ROT_ADD | POSEF_POS);
        assert_eq!(p.rot(), [1.0, 0.0, 0.0, 0.0]);
        p.set_rot_abs(None);
        assert_eq!(p.flags(), POSEF_POS);
        p.set_rot_abs(Some([0.0, 1.0, 0.0, 0.0]));
        assert_eq!(p.flags(), POSEF_ROT | POSEF_POS);
        p.set_dir([0.0, 0.0, 1.0], 0.0, &backend);
        assert_eq!(p.flags(), POSEF_ROT | POSEF_POS | POSEF_ROT_ADD);
        assert_eq!(p.rot(), [0.1, 0.2, 0.3, 0.4]);
        let calls_before = backend.calls.borrow().len();
        p.set_dir_ptr(None, 9.0, &backend);
        assert_eq!(p.flags(), POSEF_POS);
        assert_eq!(backend.calls.borrow().len(), calls_before);
        p.set_dir_abs([1.0, 0.0, 0.0], 1.5, &backend);
        assert_eq!(p.flags(), POSEF_ROT | POSEF_POS);
        let (dir, roll_bits) = backend.calls.borrow()[1];
        assert_eq!(dir, [1.0, 0.0, 0.0]);
        assert_eq!(roll_bits, 1.5f32.to_bits());
        p.set_pos(None);
        p.set_scale(None);
        assert_eq!(p.flags(), POSEF_ROT);
        p.set_pos(Some([7.0, 8.0, 9.0]));
        p.set_scale(Some([1.0, 1.0, 1.0]));
        assert_eq!(p.flags(), POSEF_ROT | POSEF_POS | POSEF_SCALE);
        assert_eq!(p.pos(), [7.0, 8.0, 9.0]);
    }

    #[test]
    fn rot_copy_preserves_nan_payload_bits() {
        let mut p = I3dPose::new_in(unit());
        let nan = f32::from_bits(0x7FC0_0001);
        assert!(nan.is_nan());
        p.set_rot(Some([nan, 1.0, 2.0, 3.0]));
        assert_eq!(p.rot()[0].to_bits(), 0x7FC0_0001);
    }

    #[test]
    fn pos_scale_predicates_read_pose_flags() {
        assert!(pose_pos_enabled(POSEF_POS));
        assert!(pose_pos_enabled(POSEF_POS | POSEF_SCALE | POSEF_ROT));
        assert!(!pose_pos_enabled(0));
        assert!(!pose_pos_enabled(POSEF_SCALE));
        assert!(pose_scale_enabled(POSEF_SCALE));
        assert!(pose_scale_enabled(POSEF_POS | POSEF_SCALE | POSEF_ROT_ADD));
        assert!(!pose_scale_enabled(0));
        assert!(!pose_scale_enabled(POSEF_POS));
        // Round-trip through the setters: the predicates track the stores.
        let mut p = I3dPose::new_in(unit());
        assert!(!pose_pos_enabled(p.flags()));
        assert!(!pose_scale_enabled(p.flags()));
        p.set_pos(Some([1.0, 2.0, 3.0]));
        assert!(pose_pos_enabled(p.flags()));
        p.set_scale(Some([1.0, 1.0, 1.0]));
        assert!(pose_scale_enabled(p.flags()));
        p.set_pos(None);
        assert!(!pose_pos_enabled(p.flags()));
        assert!(pose_scale_enabled(p.flags()));
    }

    #[test]
    fn add_pose_order_log_and_refinc_last() {
        let mut set = I3dPoseSet::new();
        assert_eq!(set.refcount(), 1);
        assert_eq!(set.growth_quantum(), POSE_SET_GROWTH_QUANTUM);
        let mut log = Vec::new();
        let h = handle(I3dPose::new_in(unit()));
        set.add_pose(h.clone(), Some(b"frame01"), &mut log);
        assert_eq!(
            log,
            vec![
                PoseEffect::NameStored { len: 7 },
                PoseEffect::PosePushed,
                PoseEffect::PoseRefIncremented { refcount: 2 },
            ]
        );
        assert_eq!(h.borrow().refcount(), 2);
        assert_eq!(h.borrow().name_or_empty(), b"frame01");
        assert_eq!(set.len(), 1);
        log.clear();
        set.add_pose(h.clone(), Some(b"frame02-longer-name"), &mut log);
        assert_eq!(
            log,
            vec![
                PoseEffect::NameFreed,
                PoseEffect::NameStored { len: 19 },
                PoseEffect::PosePushed,
                PoseEffect::PoseRefIncremented { refcount: 3 },
            ]
        );
        assert_eq!(h.borrow().name_or_empty(), b"frame02-longer-name");
        log.clear();
        set.add_pose(h.clone(), None, &mut log);
        // Null name: free only in the SetName prefix; the push + refinc
        // still run (native AddPose has no name-gated early-out).
        assert_eq!(
            log,
            vec![
                PoseEffect::NameFreed,
                PoseEffect::PosePushed,
                PoseEffect::PoseRefIncremented { refcount: 4 },
            ]
        );
        assert_eq!(h.borrow().name_or_empty(), b"");
    }

    #[test]
    fn delete_pose_swap_remove_order_and_size_before_release() {
        let mut set = I3dPoseSet::new();
        let mut log = Vec::new();
        let a = handle(I3dPose::new_in(unit()));
        let b = handle(I3dPose::new_in(unit()));
        let c = handle(I3dPose::new_in(unit()));
        let d = handle(I3dPose::new_in(unit()));
        for (h, n) in [(&a, b"a"), (&b, b"b"), (&c, b"c"), (&d, b"d")] {
            set.add_pose(h.clone(), Some(n), &mut log);
        }
        log.clear();
        // Four elements distinguish swap-with-last [a,d,c] from
        // order-preserving shift [a,c,d]: native is the former.
        assert_eq!(set.delete_pose(&b, &mut log), I3D_OK);
        assert_eq!(set.len(), 3);
        assert_eq!(set.frame_name(0), b"a");
        assert_eq!(set.frame_name(1), b"d"); // last element filled the hole
        assert_eq!(set.frame_name(2), b"c");
        assert!(matches!(log[0], PoseEffect::PoseRemoved { index: 1 }));
        assert_eq!(b.borrow().refcount(), 1); // 2 (one AddPose) - 1 (Release)
        let ghost = handle(I3dPose::new_in(unit()));
        let log_len = log.len();
        assert_eq!(set.delete_pose(&ghost, &mut log), I3DERR_OBJECTNOTFOUND);
        assert_eq!(I3DERR_OBJECTNOTFOUND, -2_147_483_642); // 0x80000006 as i32
        assert_eq!(set.len(), 3);
        assert_eq!(log.len(), log_len);
        let mut solo = I3dPoseSet::new();
        solo.add_pose(a.clone(), None, &mut Vec::new());
        let mut log2 = Vec::new();
        assert_eq!(solo.delete_pose(&a, &mut log2), I3D_OK);
        assert!(solo.is_empty());
    }

    #[test]
    fn frame_name_bounds_and_empty_fallback() {
        let mut set = I3dPoseSet::new();
        let mut log = Vec::new();
        let named = handle(I3dPose::new_in(unit()));
        let anon = handle(I3dPose::new_in(unit()));
        set.add_pose(named.clone(), Some(b"door_l"), &mut log);
        set.add_pose(anon.clone(), None, &mut log);
        assert_eq!(set.frame_name(0), b"door_l");
        assert_eq!(set.frame_name(1), b"");
        assert_eq!(set.frame_name(2), b"");
        assert_eq!(set.frame_name(99), b"");
        assert_eq!(set.frame_name(-1), b"");
    }

    #[test]
    fn destroy_releases_in_ascending_order_then_empties() {
        let mut set = I3dPoseSet::new();
        let mut log = Vec::new();
        let a = handle(I3dPose::new_in(unit()));
        let b = handle(I3dPose::new_in(unit()));
        set.add_pose(a.clone(), None, &mut log);
        set.add_pose(b.clone(), None, &mut log);
        log.clear();
        set.destroy(&mut log);
        assert_eq!(
            log,
            vec![
                PoseEffect::MemberReleased {
                    index: 0,
                    refcount: 1
                },
                PoseEffect::MemberReleased {
                    index: 1,
                    refcount: 1
                },
            ]
        );
        assert!(set.is_empty());
        let mut set2 = I3dPoseSet::new();
        set2.add_pose(a.clone(), None, &mut Vec::new());
        let mut log2 = Vec::new();
        assert_eq!(set2.release(&mut log2), 0);
        assert!(set2.is_empty());
        assert_eq!(
            log2,
            vec![PoseEffect::MemberReleased {
                index: 0,
                refcount: 1
            }]
        );
    }

    #[test]
    fn refcount_increment_wraps_like_x86_inc() {
        let mut p = I3dPose::new_in(unit());
        p.refcount = i32::MAX;
        let mut set = I3dPoseSet::new();
        let h = handle(p);
        let mut log = Vec::new();
        set.add_pose(h.clone(), None, &mut log);
        assert_eq!(h.borrow().refcount(), i32::MIN);
    }
}

#[cfg(test)]
mod driver_factory_pinning {
    use super::*;

    fn unit() -> [f32; 3] {
        [1.0, 0.0, 0.0]
    }

    #[test]
    fn driver_create_pose_yields_caller_owned_fresh_pose() {
        // Driver factory 0x19320 (mafia-re I3d_driver.cpp:1419): operator-new
        // ownership is the caller; each call yields an independent fresh
        // pose with the 0x45930 ctor words. Hand-derived vectors.
        let a = I3dPose::new_in(unit());
        let b = I3dPose::new_in([0.0, 1.0, 0.0]);
        for (pose, scale) in [(&a, unit()), (&b, [0.0, 1.0, 0.0])] {
            assert_eq!(pose.refcount(), POSE_INITIAL_REFCOUNT);
            assert_eq!(POSE_INITIAL_REFCOUNT, 1);
            assert_eq!(pose.flags(), 0);
            assert_eq!(pose.pos(), [0.0, 0.0, 0.0]);
            assert_eq!(pose.scale(), scale);
            assert_eq!(pose.rot()[0].to_bits(), 0x3F80_0000);
            assert_eq!(&pose.rot()[1..], &[0.0, 0.0, 0.0]);
            assert_eq!(pose.name_or_empty(), b"");
        }
        // No aliasing between fresh values.
        let mut c = I3dPose::new_in(unit());
        c.set_pos(Some([9.0, 0.0, 0.0]));
        assert_eq!(a.pos(), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn driver_create_pose_set_yields_caller_owned_empty_set() {
        // Driver factory 0x19380 (mafia-re I3d_driver.cpp:1426): operator-new
        // ownership is the caller; fresh set is empty with refcount 1 and
        // growth quantum 0x20. Hand-derived vectors.
        let a = I3dPoseSet::new();
        let b = I3dPoseSet::new();
        for set in [&a, &b] {
            assert_eq!(set.refcount(), POSE_INITIAL_REFCOUNT);
            assert!(set.is_empty());
            assert_eq!(set.len(), 0);
            assert_eq!(set.growth_quantum(), POSE_SET_GROWTH_QUANTUM);
            assert_eq!(POSE_SET_GROWTH_QUANTUM, 0x20);
        }
        let mut c = I3dPoseSet::new();
        let mut log = Vec::new();
        c.add_pose(
            Rc::new(RefCell::new(I3dPose::new_in(unit()))),
            None,
            &mut log,
        );
        assert_eq!(c.len(), 1);
        assert!(a.is_empty() && b.is_empty());
    }
}
