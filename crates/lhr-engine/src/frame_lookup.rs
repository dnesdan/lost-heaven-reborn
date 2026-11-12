//! LS3DF I3D_scene::FindFrame / I3D_frame::GetChildFrame, bounded to flags
//! 0x4ffff and exact ASCII names under the original default C-locale branch.
//! Views borrow existing live frame owners; this module constructs no scene.
//! These PC method names follow the pinned mafia-re reconstruction, corroborated
//! by actual PS2 linker names. New PC debug symbols, complete PC/PS2 body
//! correspondence, layouts and full-function parity are not claimed.
use crate::{
    frame_transform::{FrameKey, FrameKind},
    model_loading::ModelArena,
    script_scheduler::FrameReferenceId,
};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SCRIPT_FIND_FLAGS: u32 = 0x4ffff;
pub const MAX_LOOKUP_VISITS: usize = 65_536;
/// I3D_frame::EnumSubtree result words (OK and CANCELED).
pub const ENUM_OK: u32 = 0;
pub const ENUM_SKIPCHILDREN: u32 = 1;
pub const ENUM_CANCEL: u32 = 2;
pub const ENUM_CANCELED: u32 = 0x8000_000d;
/// Working name for the `ENUMF_BACKDROP` scope gate (mafia-re ls3df/I3D/I3D.h:1329
/// at pin 113b5b6). The reference records no LS3DF FUNCTION address for a
/// standalone reader; the bit is consumed inline by `EnumFrames`/`FindFrame`
/// (PC RVAs 0x49180/0x49210, already occupied, not claimed here). Exact bit
/// test on the caller-supplied flags word; no owner is consulted.
pub fn enum_scope_includes_backdrop(flags: u32) -> bool {
    flags & 0x40000 != 0
}
/// Working name for the `ENUMF_OMNI` scope gate (mafia-re ls3df/I3D/I3D.h:1330
/// at pin 113b5b6). Same inline-only status as `enum_scope_includes_backdrop`:
/// no standalone LS3DF FUNCTION entry exists, so no RVA is claimed here.
/// Exact bit test; no state moves.
pub fn enum_scope_includes_omni(flags: u32) -> bool {
    flags & 0x80000 != 0
}
/// Working name for the `ENUMF_NEAR` scope gate (mafia-re ls3df/I3D/I3D.h:1331
/// at pin 113b5b6). Same inline-only status as `enum_scope_includes_backdrop`:
/// no standalone LS3DF FUNCTION entry exists, so no RVA is claimed here.
/// Exact bit test; no state moves.
pub fn enum_scope_includes_near(flags: u32) -> bool {
    flags & 0x100000 != 0
}
/// Working name for the `ENUMF_WILDMASK` gate (mafia-re ls3df/I3D/I3D.h:226 at
/// pin 113b5b6: `'*'` ends the name, `'?'` stands for any character). Same
/// inline-only status as `enum_scope_includes_backdrop`: no standalone LS3DF
/// FUNCTION entry exists, so no RVA is claimed here. Exact bit test on the
/// caller-supplied flags word; the mask text itself stays caller-owned.
pub fn enum_scope_wildmask(flags: u32) -> bool {
    flags & 0x10000 != 0
}
/// Working name for the `ENUMF_CASESENS` gate (mafia-re ls3df/I3D/I3D.h:227 at
/// pin 113b5b6: case-sensitive name match). Same inline-only status as
/// `enum_scope_includes_backdrop`: no standalone LS3DF FUNCTION entry exists,
/// so no RVA is claimed here. Exact bit test; no locale or owner moves.
pub fn enum_scope_case_sensitive(flags: u32) -> bool {
    flags & 0x20000 != 0
}
/// Working name for the `ENUMF_VISUAL` kind gate (mafia-re ls3df/I3D/I3D.h:211
/// at pin 113b5b6: match visual frames by name). The reference records no
/// LS3DF FUNCTION address for a standalone reader: the bit is consumed inline
/// by `EnumSubtree` (mafia-re ls3df/I3D/frames/I3d_frame.cpp:631-651; PC RVA
/// 0x1c020, already occupied, not claimed here), which keeps only the frame
/// type's own bit and clears the rest. Exact bit test on the caller-supplied
/// flags word; no owner or cache is consulted.
pub fn enum_wants_visual(flags: u32) -> bool {
    flags & 0x1 != 0
}
/// Working name for the `ENUMF_LIGHT` kind gate (mafia-re ls3df/I3D/I3D.h:212
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_light(flags: u32) -> bool {
    flags & 0x2 != 0
}
/// Working name for the `ENUMF_CAMERA` kind gate (mafia-re ls3df/I3D/I3D.h:213
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_camera(flags: u32) -> bool {
    flags & 0x4 != 0
}
/// Working name for the `ENUMF_SOUND` kind gate (mafia-re ls3df/I3D/I3D.h:214
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_sound(flags: u32) -> bool {
    flags & 0x8 != 0
}
/// Working name for the `ENUMF_SECTOR` kind gate (mafia-re ls3df/I3D/I3D.h:215
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_sector(flags: u32) -> bool {
    flags & 0x10 != 0
}
/// Working name for the `ENUMF_DUMMY` kind gate (mafia-re ls3df/I3D/I3D.h:216
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_dummy(flags: u32) -> bool {
    flags & 0x20 != 0
}
/// Working name for the `ENUMF_TARGET` kind gate (mafia-re ls3df/I3D/I3D.h:217
/// at pin 113b5b6: the value is a guess). Same inline-only status as
/// `enum_wants_visual`: no standalone LS3DF FUNCTION entry exists, so no RVA
/// is claimed here. Exact bit test; no state moves.
pub fn enum_wants_target(flags: u32) -> bool {
    flags & 0x40 != 0
}
/// Working name for the `ENUMF_MODEL` kind gate (mafia-re ls3df/I3D/I3D.h:218
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_model(flags: u32) -> bool {
    flags & 0x80 != 0
}
/// Working name for the `ENUMF_USER` kind gate (mafia-re ls3df/I3D/I3D.h:219
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_user(flags: u32) -> bool {
    flags & 0x100 != 0
}
/// Working name for the `ENUMF_VOLUME` kind gate (mafia-re ls3df/I3D/I3D.h:220
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_volume(flags: u32) -> bool {
    flags & 0x200 != 0
}
/// Working name for the `ENUMF_JOINT` kind gate (mafia-re ls3df/I3D/I3D.h:221
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_joint(flags: u32) -> bool {
    flags & 0x400 != 0
}
/// Working name for the `ENUMF_OCCLUDER` kind gate (mafia-re ls3df/I3D/I3D.h:222
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`, with one
/// disclosed asymmetry: the native `EnumSubtree` switch has no occluder arm,
/// so an occluder frame never matches a partial mask (like the scene,
/// landscape and emitter kinds, it falls into `default: type_flags = 0`).
/// This gate only reports whether the caller asked for occluders; the
/// never-match rule stays with the walker. Exact bit test; no state moves.
pub fn enum_wants_occluder(flags: u32) -> bool {
    flags & 0x800 != 0
}
/// Working name for the `ENUMF_AREA` kind gate (mafia-re ls3df/I3D/I3D.h:223
/// at pin 113b5b6). Same inline-only status as `enum_wants_visual`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn enum_wants_area(flags: u32) -> bool {
    flags & 0x1000 != 0
}
/// Working name for the `ENUMF_SHADOW` kind gate (mafia-re ls3df/I3D/I3D.h:224
/// at pin 113b5b6: the value is a guess). Same inline-only status as
/// `enum_wants_visual`: no standalone LS3DF FUNCTION entry exists, so no RVA
/// is claimed here. Exact bit test; no state moves.
pub fn enum_wants_shadow(flags: u32) -> bool {
    flags & 0x2000 != 0
}
/// Working name for the `ENUMF_ALL` bypass (mafia-re ls3df/I3D/I3D.h:225 at
/// pin 113b5b6: match every frame type). Same inline-only status as
/// `enum_wants_visual`: no standalone LS3DF FUNCTION entry exists, so no RVA
/// is claimed here. Models the native `type_flags != ENUMF_ALL` branch
/// (mafia-re ls3df/I3D/frames/I3d_frame.cpp:631-632): only a full low word
/// skips the per-kind switch. Exact word test; no state moves.
pub fn enum_matches_all_kinds(flags: u32) -> bool {
    flags & 0xffff == 0xffff
}
/// 0x1c310 projection (LS3DF.dll e211c850…). Upstream working name
/// FindChildCallBack (mafia-re I3d_frame.cpp:717-721), not an evidenced
/// original symbol. No Ghidra body in session -> tier contract per the
/// 0x2b830 standard. Store-then-CANCEL order preserved; the x86 stdcall
/// callee-cleanup has no host equivalent, only the u32 CANCEL word (=2).
pub fn find_child_callback_slot(
    slot: &mut Option<crate::frame_transform::FrameKey>,
    frame: crate::frame_transform::FrameKey,
) -> u32 {
    *slot = Some(frame);
    2 // I3DENUMRET_CANCEL
}
static NEXT_INCARNATION: AtomicU64 = AtomicU64::new(1);
pub(crate) fn incarnation() -> u64 {
    NEXT_INCARNATION
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
        .expect("lookup lifetime identity exhausted")
}

/// One identity lease per actual owned frame incarnation. The actual frame owner
/// retains this noncloneable lease and drops it on deletion; recreating a frame
/// (even at the same raw identity) requires a fresh lease. This is not a frame
/// constructor and performs no native AddRef/Release.
#[derive(Debug)]
pub struct FrameLifetime {
    reference: FrameReferenceId,
}
impl FrameLifetime {
    pub fn for_existing(identity: u32) -> Self {
        Self {
            reference: FrameReferenceId::scoped(identity, incarnation()),
        }
    }
    pub fn reference(&self) -> FrameReferenceId {
        self.reference
    }
}

/// Arena-owned namespace. Stable keys within that arena are never reused.
#[derive(Debug)]
pub(crate) struct FrameLookupOwner(u64);
impl Default for FrameLookupOwner {
    fn default() -> Self {
        Self(incarnation())
    }
}
impl FrameLookupOwner {
    pub(crate) fn reference(&self, identity: u32) -> FrameReferenceId {
        FrameReferenceId::scoped(identity, self.0)
    }
}

/// Borrows the existing materialized model hierarchy, including current liveness
/// and native LinkTo child order. Locale admission is explicitly caller supplied.
/// This does not create primary/backdrop sectors or a mission scene.
pub struct ModelFrameLookup<'a> {
    arena: &'a crate::model_loading::ModelArena,
    default_c_locale: bool,
}
impl<'a> ModelFrameLookup<'a> {
    pub fn borrow(arena: &'a crate::model_loading::ModelArena, default_c_locale: bool) -> Self {
        Self {
            arena,
            default_c_locale,
        }
    }
    pub fn reference(
        &self,
        key: crate::frame_transform::FrameKey,
    ) -> Result<FrameReferenceId, FrameLookupError> {
        let reference = self.arena.lookup_reference(key);
        self.arena
            .frame(key)
            .map_err(|_| FrameLookupError::StaleFrame(reference))?;
        Ok(reference)
    }
    pub(crate) fn arena_frame(
        arena: &crate::model_loading::ModelArena,
        reference: FrameReferenceId,
    ) -> Result<FrameView<'_>, FrameLookupError> {
        let key = crate::frame_transform::FrameKey(reference.identity());
        if arena.lookup_reference(key) != reference {
            return Err(FrameLookupError::StaleFrame(reference));
        }
        let frame = arena
            .frame(key)
            .map_err(|_| FrameLookupError::StaleFrame(reference))?;
        let links = arena
            .links(key)
            .map_err(|_| FrameLookupError::StaleFrame(reference))?;
        Ok(FrameView {
            name: (!frame.name_bytes.is_empty()).then_some(frame.name_bytes.as_slice()),
            child: links.child.map(|key| arena.lookup_reference(key)),
            next: links.next.map(|key| arena.lookup_reference(key)),
        })
    }
}
impl FrameLookupBackend for ModelFrameLookup<'_> {
    fn default_c_locale(&self) -> bool {
        self.default_c_locale
    }
    fn supports_empty_query(&self) -> bool {
        false
    }
    fn frame(&self, reference: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
        Self::arena_frame(self.arena, reference)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameLookupError {
    UnsupportedFlags(u32),
    UnsupportedLocale,
    UnsupportedName,
    MissingPrimaryRoot,
    MissingBackdropRoot,
    StaleFrame(FrameReferenceId),
    DuplicateIdentity,
    Cycle(FrameReferenceId),
    InvalidBudget,
    VisitLimit,
    Allocation,
    Backend(String),
}
#[derive(Clone, Copy, Debug)]
pub struct FrameView<'a> {
    pub name: Option<&'a [u8]>,
    pub child: Option<FrameReferenceId>,
    pub next: Option<FrameReferenceId>,
}
/// Each read must validate that the borrowed identity still names a live frame.
/// A backend may not resolve a deleted frame's identity to a replacement frame.
pub trait FrameLookupBackend {
    fn default_c_locale(&self) -> bool;
    /// False when the owning representation cannot distinguish a native null
    /// stored-name pointer from an allocated empty C string.
    fn supports_empty_query(&self) -> bool {
        true
    }
    fn frame(&self, frame: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError>;
}
/// Borrowed projections retain the true owner leases for their entire lifetime.
#[derive(Debug)]
pub struct LiveFrame<'a> {
    pub lifetime: &'a FrameLifetime,
    pub name: Option<&'a [u8]>,
    pub child: Option<FrameReferenceId>,
    pub next: Option<FrameReferenceId>,
}
pub struct LiveFrameTree<'a> {
    frames: &'a [LiveFrame<'a>],
    identities: HashMap<FrameReferenceId, usize>,
    default_c_locale: bool,
}
impl<'a> LiveFrameTree<'a> {
    pub fn borrow(
        frames: &'a [LiveFrame<'a>],
        default_c_locale: bool,
    ) -> Result<Self, FrameLookupError> {
        if frames.len() > MAX_LOOKUP_VISITS {
            return Err(FrameLookupError::VisitLimit);
        }
        let mut identities = HashMap::new();
        identities
            .try_reserve(frames.len())
            .map_err(|_| FrameLookupError::Allocation)?;
        for (index, frame) in frames.iter().enumerate() {
            if identities
                .insert(frame.lifetime.reference(), index)
                .is_some()
            {
                return Err(FrameLookupError::DuplicateIdentity);
            }
        }
        Ok(Self {
            frames,
            identities,
            default_c_locale,
        })
    }
}
impl FrameLookupBackend for LiveFrameTree<'_> {
    fn default_c_locale(&self) -> bool {
        self.default_c_locale
    }
    fn frame(&self, reference: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
        let index = self
            .identities
            .get(&reference)
            .ok_or(FrameLookupError::StaleFrame(reference))?;
        let frame = &self.frames[*index];
        Ok(FrameView {
            name: frame.name,
            child: frame.child,
            next: frame.next,
        })
    }
}
/// Reached roots must exist. None represents an unavailable native root and
/// returns an error; a primary hit never consults the backdrop root.
#[derive(Clone, Copy, Debug, Default)]
pub struct SceneRoots {
    pub primary: Option<FrameReferenceId>,
    pub backdrop: Option<FrameReferenceId>,
}
fn validate<B: FrameLookupBackend + ?Sized>(
    backend: &B,
    name: &[u8],
    flags: u32,
) -> Result<(), FrameLookupError> {
    if flags != SCRIPT_FIND_FLAGS {
        return Err(FrameLookupError::UnsupportedFlags(flags));
    }
    if !backend.default_c_locale() {
        return Err(FrameLookupError::UnsupportedLocale);
    }
    if name.is_empty() && !backend.supports_empty_query() {
        return Err(FrameLookupError::UnsupportedName);
    }
    if !name.is_ascii() || name.contains(&0) {
        return Err(FrameLookupError::UnsupportedName);
    }
    Ok(())
}
fn traverse<B: FrameLookupBackend + ?Sized>(
    backend: &B,
    root: FrameReferenceId,
    name: &[u8],
    remaining: &mut usize,
) -> Result<Option<FrameReferenceId>, FrameLookupError> {
    let mut pending = Vec::new();
    let mut active = HashSet::new();
    // Enter/exit tasks preserve the recursive active path, permitting shared
    // descendants while rejecting ancestor cycles. Sibling-only cycles exhaust
    // the bounded visit budget. No host recursion or unbounded work storage.
    pending
        .try_reserve(*remaining * 2 + 1)
        .map_err(|_| FrameLookupError::Allocation)?;
    active
        .try_reserve(*remaining)
        .map_err(|_| FrameLookupError::Allocation)?;
    pending.push((root, true, false));
    while let Some((reference, enter, include_next)) = pending.pop() {
        if !enter {
            active.remove(&reference);
            continue;
        }
        if *remaining == 0 {
            return Err(FrameLookupError::VisitLimit);
        }
        *remaining -= 1;
        if !active.insert(reference) {
            return Err(FrameLookupError::Cycle(reference));
        }
        let view = backend.frame(reference)?;
        if let Some(stored) = view.name {
            if !stored.is_ascii() || stored.contains(&0) {
                return Err(FrameLookupError::UnsupportedName);
            }
            if stored.eq_ignore_ascii_case(name) {
                return Ok(Some(reference));
            }
        }
        // Sibling enters after this frame exits; only its descendants retain
        // this frame on the active path. Root.next lies outside this subtree.
        if include_next {
            if let Some(next) = view.next {
                pending.push((next, true, true));
            }
        }
        pending.push((reference, false, false));
        if let Some(child) = view.child {
            pending.push((child, true, true));
        }
    }
    Ok(None)
}
/// Self-first depth-first child/next search. The root and its descendants are
/// visited. The native NULL-query shortcut is outside this nonnull byte-slice
/// query API; a stored null name remains distinct from an allocated empty name.
/// Original I3D_frame::GetChildFrame, PC RVA 0x1c360 (VA 0x1001c360; REA
/// formed Ghidra body FUN_1001c360 in LS3DF.dll
/// e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
/// ev_fc2ff1e5fcc990b1b951997b24b0ff30a04f3f177283f76296945c4adf3073;
/// mafia-re ls3df/I3D/frames/I3d_frame.cpp:732-795 at pin 113b5b6).
/// Confirm-only on this pre-existing port: self-first preorder with the
/// first hit winning matches the native self-check-then-recurse order.
/// Covered subset only: case-insensitive exact matching under
/// SCRIPT_FIND_FLAGS with original-name (not full-name) comparison.
/// The native type gate, case-sensitive and wildcard arms, and full-name
/// matching stay unported (disclosed); embedded NUL and non-ASCII inputs
/// are explicit host errors, and ancestor cycles, visit exhaustion and
/// deleted-mid-enumeration keys keep the existing host guards.
pub fn get_child_frame<B: FrameLookupBackend + ?Sized>(
    backend: &B,
    root: FrameReferenceId,
    name: &[u8],
    flags: u32,
    visits: usize,
) -> Result<Option<FrameReferenceId>, FrameLookupError> {
    validate(backend, name, flags)?;
    if visits == 0 || visits > MAX_LOOKUP_VISITS {
        return Err(FrameLookupError::InvalidBudget);
    }
    let mut remaining = visits;
    traverse(backend, root, name, &mut remaining)
}
/// Primary sector precedes backdrop. Other scene sectors are deliberately absent
/// from this evidenced scene search; no driver name index is consulted.
pub fn find_frame<B: FrameLookupBackend + ?Sized>(
    backend: &B,
    roots: SceneRoots,
    name: &[u8],
    flags: u32,
    visits: usize,
) -> Result<Option<FrameReferenceId>, FrameLookupError> {
    validate(backend, name, flags)?;
    if visits == 0 || visits > MAX_LOOKUP_VISITS {
        return Err(FrameLookupError::InvalidBudget);
    }
    let mut remaining = visits;
    let primary = roots.primary.ok_or(FrameLookupError::MissingPrimaryRoot)?;
    if let Some(found) = traverse(backend, primary, name, &mut remaining)? {
        return Ok(Some(found));
    }
    let backdrop = roots
        .backdrop
        .ok_or(FrameLookupError::MissingBackdropRoot)?;
    if let Some(found) = traverse(backend, backdrop, name, &mut remaining)? {
        return Ok(Some(found));
    }
    Ok(None)
}
/// Original I3D_frame::FindChildFrame, PC RVA 0x1c330 (VA 0x1001c330; REA
/// formed Ghidra body FUN_1001c330 in LS3DF.dll
/// e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
/// ev_4eafe6bf1ad468f54c7f73d52ac8c50cc4a19fed6626a1738341378c2b0be7e8;
/// mafia-re ls3df/I3D/frames/I3d_frame.cpp:723-730 at pin 113b5b6).
/// Order-exact composition of the two already-ported pieces:
/// `EnumFrames` over the root's children (each child subtree through
/// `enum_subtree`, PC RVA 0x1c020) with `find_child_callback_slot`
/// (PC RVA 0x1c310) storing the first hit and returning CANCEL.
/// The root itself is never matched, matching the native children-only
/// walk; the first hit in child-list order wins and later subtrees are
/// never entered. Name matching, type gates, cycle guards, visit budget
/// and the last-destroyed-frame skip are inherited from `enum_subtree`
/// unchanged. A stale root maps to `StaleFrame`, like `enum_subtree`.
pub fn find_child_frame(
    arena: &ModelArena,
    root: FrameKey,
    name: Option<&[u8]>,
    flags: u32,
    visits: usize,
) -> Result<Option<FrameKey>, FrameLookupError> {
    let mut at = arena
        .links(root)
        .map_err(|_| FrameLookupError::StaleFrame(FrameReferenceId::from_existing(root.0)))?
        .child;
    let mut hit = None;
    while let Some(child) = at {
        let outcome = enum_subtree(
            arena,
            child,
            &mut |key| find_child_callback_slot(&mut hit, key),
            flags,
            name,
            visits,
        )?;
        if outcome == ENUM_CANCELED || hit.is_some() {
            break;
        }
        at = arena
            .links(child)
            .map_err(|_| FrameLookupError::StaleFrame(FrameReferenceId::from_existing(child.0)))?
            .next;
    }
    Ok(hit)
}

fn subtree_c_str(bytes: &[u8]) -> &[u8] {
    // Native strlen truncation; the mask pattern is truncated silently while
    // a stored name with an embedded NUL is an explicit host error (see the
    // call site): asymmetric by design, both disclosed.
    &bytes[..bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len())]
}

fn subtree_kind_mask(kind: FrameKind) -> u32 {
    match kind {
        FrameKind::Mesh | FrameKind::Singlemesh | FrameKind::Projector | FrameKind::Lensflare => {
            0x1
        }
        FrameKind::Light => 0x2,
        FrameKind::Camera => 0x4,
        FrameKind::Sound => 0x8,
        FrameKind::Sector => 0x10,
        FrameKind::Dummy => 0x20,
        FrameKind::Target => 0x40,
        FrameKind::Model => 0x80,
        FrameKind::Joint => 0x400,
        FrameKind::Area => 0x1000,
        FrameKind::Occluder | FrameKind::Scene => 0,
    }
}

fn subtree_mask_matches(pattern: &[u8], stored: &[u8], flags: u32) -> bool {
    let pat = subtree_c_str(pattern);
    let name = subtree_c_str(stored);
    if flags & 0x10000 == 0 {
        if flags & 0x20000 != 0 {
            pat == name
        } else {
            pat.eq_ignore_ascii_case(name)
        }
    } else {
        let len = pat.len().min(name.len()) + 1;
        for i in 0..len {
            let a = pat.get(i).copied().unwrap_or(0);
            let b = name.get(i).copied().unwrap_or(0);
            if a == b'*' {
                break;
            }
            if a != b'?' {
                let same = if flags & 0x20000 != 0 {
                    a == b
                } else {
                    a.eq_ignore_ascii_case(&b)
                };
                if !same {
                    return false;
                }
            }
        }
        true
    }
}
/// Working name for the self type gate of `I3D_frame::GetChildFrame` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:737-755 at pin 113b5b6: the `type_flags`
/// switch on `m_Type`). The `GetChildFrame` body lives at already-occupied PC
/// RVA 0x1c360 (confirm-only port `get_child_frame`, not claimed here); only
/// this first gate is modeled, and the child recursion stays caller-owned.
/// A full low word (`ENUMF_ALL`) accepts every kind without consulting the
/// switch. Otherwise the frame's own kind bit must be set in the caller
/// flags. The native switch has no `JOINT` arm, so a joint never self-matches
/// even with `ENUMF_JOINT` set (it falls into `default: ok = 0`); that
/// never-match rule is modeled here, not in the walker. Kinds without a
/// `FrameKind` variant (user, volume, shadow, emitor) are unrepresented, the
/// same disclosed coverage the walker already has. Exact word test; no state
/// moves.
pub fn get_child_frame_self_type_accepts(kind: FrameKind, flags: u32) -> bool {
    if flags & 0xffff == 0xffff {
        return true;
    }
    if matches!(kind, FrameKind::Joint) {
        return false;
    }
    subtree_kind_mask(kind) & flags != 0
}
/// Working name for the self name gate of `I3D_frame::GetChildFrame` (mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:756-786 at pin 113b5b6). Same occupied-body
/// status as `get_child_frame_self_type_accepts` (PC RVA 0x1c360): only this
/// gate is modeled. Unlike the subtree walk, which compares the model-stripped
/// original name, this gate compares the full stored name (`m_pName`). A null
/// query accepts without reading the name; a null stored name rejects.
/// Otherwise the exact, case-sensitive and wildcard arms run exactly as in
/// `subtree_mask_matches` (the native spells the insensitive arm with
/// `toupper`, ASCII-only here like the shared core -- disclosed). No state
/// moves.
pub fn get_child_frame_self_name_accepts(
    stored: Option<&[u8]>,
    query: Option<&[u8]>,
    flags: u32,
) -> bool {
    let Some(pattern) = query else {
        return true;
    };
    let Some(name) = stored else {
        return false;
    };
    subtree_mask_matches(pattern, name, flags)
}
/// Sectors `I3D_scene::FindFrame` visits, in order (mafia-re
/// ls3df/I3D/I3d_scene.cpp:228-249 at pin 113b5b6: primary first, then the
/// flag-gated backdrop, omni and near sectors, returning the first hit).
/// The `FindFrame` body lives at frozen ledger RVA 0x49210 (not claimed here);
/// the current `find_frame` above walks primary then backdrop only, so the
/// omni/near arms stay unported (disclosed). Only the flag-gated visit order
/// is modeled; traversal and the first-hit return stay caller-owned. Exact
/// flag tests; no state moves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FindFrameSector {
    Primary,
    Backdrop,
    Omni,
    Near,
}
/// Visit order for `find_frame_sector_visit_order`: primary always, then each
/// further sector only when its `ENUMF_*` bit is set. Absent sectors are
/// `None`, never reordered.
pub fn find_frame_sector_visit_order(flags: u32) -> [Option<FindFrameSector>; 4] {
    [
        Some(FindFrameSector::Primary),
        enum_scope_includes_backdrop(flags).then_some(FindFrameSector::Backdrop),
        enum_scope_includes_omni(flags).then_some(FindFrameSector::Omni),
        enum_scope_includes_near(flags).then_some(FindFrameSector::Near),
    ]
}
/// Working name for the per-sector error short-circuit of
/// `I3D_scene::EnumFrames` (mafia-re ls3df/I3D/I3d_scene.cpp:206-223 at pin
/// 113b5b6: `if (ret < 0) return ret` after the primary, backdrop and omni
/// sectors). The `EnumFrames` body lives at already-occupied PC RVA 0x49180
/// (not claimed here); only the negative-word stop rule is modeled, and the
/// sector enumeration stays caller-owned. Reports whether the sector walk
/// stops with the carried result word instead of visiting the next sector.
/// No state moves.
pub fn scene_enum_frames_stops_on_error(result: i32) -> bool {
    result < 0
}
/// Working name for the first-hit return of `I3D_scene::FindFrame` (mafia-re
/// ls3df/I3D/I3d_scene.cpp:230-248 at pin 113b5b6: each sector's hit returns
/// immediately instead of consulting the next sector). Same frozen-body
/// status as `find_frame_sector_visit_order` (ledger RVA 0x49210, not claimed
/// here); only the stop rule is modeled, and the per-sector `GetChildFrame`
/// calls stay caller-owned. Reports whether the sector walk returns the hit.
/// No state moves.
pub fn find_frame_stops_at_first_hit(hit: bool) -> bool {
    hit
}

/// Original I3D_frame::EnumSubtree, PC RVA 0x1c020 (already ledger-credited,
/// frozen port `LS3DF bounded actor frame effects over owned ModelArena`: NO overlay
/// entry here; probe-verified formed Ghidra body, confirm-only; mafia-re
/// ls3df/I3D/frames/I3d_frame.cpp:629-697). Self-inclusive preorder over
/// hierarchy links with saved-next discipline and the shared
/// lastDestroyedFrame child guard. Type filter uses the low word with an
/// all-types bypass; Occluder/Scene never match partial masks. Unrepresented
/// native kinds (User 0x100, Volume 0x200, Shadow 0x2000 have no FrameKind
/// variant yet) need mask bits when those frame types are introduced.
/// Mask compares
/// original names with exact or wild/card case arms. Callback words are
/// OK(0), SKIPCHILDREN(1) and CANCEL(2); CANCEL maps to CANCELED
/// (0x8000000d). Deleted-mid-enumeration keys are skipped defensively;
/// ancestor cycles, visit exhaustion and non-ASCII eligible names are
/// explicit errors. No separate credit for the 0x1a050 fast-tree helper.
pub fn enum_subtree(
    arena: &ModelArena,
    root: FrameKey,
    proc: &mut impl FnMut(FrameKey) -> u32,
    flags: u32,
    mask: Option<&[u8]>,
    visits: usize,
) -> Result<u32, FrameLookupError> {
    if visits == 0 || visits > MAX_LOOKUP_VISITS {
        return Err(FrameLookupError::InvalidBudget);
    }
    if let Some(pattern) = mask {
        if !subtree_c_str(pattern).is_ascii() {
            return Err(FrameLookupError::UnsupportedName);
        }
    }
    arena
        .frame(root)
        .map_err(|_| FrameLookupError::StaleFrame(FrameReferenceId::from_existing(root.0)))?;
    let mut active = Vec::new();
    active
        .try_reserve(visits)
        .map_err(|_| FrameLookupError::Allocation)?;
    let mut remaining = visits;
    enum_recurse(arena, root, proc, flags, mask, &mut active, &mut remaining)
}

fn subtree_stale(key: FrameKey) -> FrameLookupError {
    FrameLookupError::StaleFrame(FrameReferenceId::from_existing(key.0))
}

fn enum_recurse(
    arena: &ModelArena,
    key: FrameKey,
    proc: &mut impl FnMut(FrameKey) -> u32,
    flags: u32,
    mask: Option<&[u8]>,
    active: &mut Vec<FrameKey>,
    remaining: &mut usize,
) -> Result<u32, FrameLookupError> {
    if *remaining == 0 {
        return Err(FrameLookupError::VisitLimit);
    }
    *remaining -= 1;
    if active.contains(&key) {
        return Err(FrameLookupError::Cycle(FrameReferenceId::from_existing(
            key.0,
        )));
    }
    active.push(key);
    let outcome = enum_visit(arena, key, proc, flags, mask, active, remaining);
    active.pop();
    outcome
}

fn enum_visit(
    arena: &ModelArena,
    key: FrameKey,
    proc: &mut impl FnMut(FrameKey) -> u32,
    flags: u32,
    mask: Option<&[u8]>,
    active: &mut Vec<FrameKey>,
    remaining: &mut usize,
) -> Result<u32, FrameLookupError> {
    let type_pass = if flags & 0xffff == 0xffff {
        true
    } else {
        let kind = arena.state(key).map_err(|_| subtree_stale(key))?.kind;
        let bit = subtree_kind_mask(kind);
        bit != 0 && (flags & bit) != 0
    };
    let call_proc = type_pass;
    if call_proc {
        if let Some(pattern) = mask {
            let stored = arena.frame(key).map_err(|_| subtree_stale(key))?;
            match stored.original_name_bytes.as_deref() {
                None => {
                    return enum_children(arena, key, proc, flags, mask, active, remaining);
                }
                Some(orig) => {
                    // Native compares truncated C strings (strlen semantics);
                    // embedded NUL is an explicit host error, non-ASCII likewise.
                    if !orig.is_ascii() || orig.contains(&0) {
                        return Err(FrameLookupError::UnsupportedName);
                    }
                    if !subtree_mask_matches(pattern, orig, flags) {
                        return enum_children(arena, key, proc, flags, mask, active, remaining);
                    }
                }
            }
        }
    } else if mask.is_some() {
        return enum_children(arena, key, proc, flags, mask, active, remaining);
    }
    if call_proc {
        let ret = proc(key);
        if ret == ENUM_CANCEL {
            return Ok(ENUM_CANCELED);
        }
        if ret == ENUM_SKIPCHILDREN {
            return Ok(ENUM_OK);
        }
    }
    enum_children(arena, key, proc, flags, mask, active, remaining)
}

fn enum_children(
    arena: &ModelArena,
    key: FrameKey,
    proc: &mut impl FnMut(FrameKey) -> u32,
    flags: u32,
    mask: Option<&[u8]>,
    active: &mut Vec<FrameKey>,
    remaining: &mut usize,
) -> Result<u32, FrameLookupError> {
    if arena.last_destroyed_frame == Some(key) {
        return Ok(ENUM_OK);
    }
    let links = arena.links(key).map_err(|_| subtree_stale(key))?;
    let count = links.child_count as usize;
    let mut snapshot = Vec::new();
    snapshot
        .try_reserve(count)
        .map_err(|_| FrameLookupError::Allocation)?;
    let mut seen = HashSet::new();
    seen.try_reserve(count)
        .map_err(|_| FrameLookupError::Allocation)?;
    let mut at = links.child;
    while let Some(cur) = at {
        if !seen.insert(cur.0) {
            return Err(FrameLookupError::Cycle(FrameReferenceId::from_existing(
                cur.0,
            )));
        }
        if snapshot.len() >= MAX_LOOKUP_VISITS {
            return Err(FrameLookupError::VisitLimit);
        }
        arena.frame(cur).map_err(|_| subtree_stale(cur))?;
        snapshot.push(cur);
        at = arena.links(cur).map_err(|_| subtree_stale(cur))?.next;
    }
    for child in snapshot {
        if arena.frame(child).is_err() {
            continue;
        }
        if enum_recurse(arena, child, proc, flags, mask, active, remaining)? == ENUM_CANCELED {
            return Ok(ENUM_CANCELED);
        }
    }
    Ok(ENUM_OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn self_first_depth_first_order_and_sector_precedence() {
        let ids: Vec<_> = (0..6).map(FrameLifetime::for_existing).collect();
        let r = |i: usize| ids[i].reference();
        let frames = [
            LiveFrame {
                lifetime: &ids[0],
                name: Some(b"root"),
                child: Some(r(1)),
                next: Some(r(5)),
            },
            LiveFrame {
                lifetime: &ids[1],
                name: Some(b"branch"),
                child: Some(r(2)),
                next: Some(r(3)),
            },
            LiveFrame {
                lifetime: &ids[2],
                name: Some(b"AlPhA"),
                child: None,
                next: None,
            },
            LiveFrame {
                lifetime: &ids[3],
                name: Some(b"alpha"),
                child: None,
                next: None,
            },
            LiveFrame {
                lifetime: &ids[4],
                name: Some(b"alpha"),
                child: None,
                next: None,
            },
            LiveFrame {
                lifetime: &ids[5],
                name: Some(b"excluded"),
                child: None,
                next: None,
            },
        ];
        let tree = LiveFrameTree::borrow(&frames, true).unwrap();
        let roots = SceneRoots {
            primary: Some(r(0)),
            backdrop: Some(r(4)),
        };
        assert_eq!(
            find_frame(&tree, roots, b"ALPHA", SCRIPT_FIND_FLAGS, 6),
            Ok(Some(r(2)))
        );
        assert_eq!(
            get_child_frame(&tree, r(0), b"root", SCRIPT_FIND_FLAGS, 1),
            Ok(Some(r(0)))
        );
        assert_eq!(
            get_child_frame(&tree, r(0), b"excluded", SCRIPT_FIND_FLAGS, 6),
            Ok(None)
        );
        assert_eq!(
            find_frame(&tree, roots, b"alpha*", SCRIPT_FIND_FLAGS, 6),
            Ok(None)
        );
        assert_eq!(
            find_frame(
                &tree,
                SceneRoots {
                    primary: Some(r(5)),
                    backdrop: Some(r(4))
                },
                b"alpha",
                SCRIPT_FIND_FLAGS,
                2
            ),
            Ok(Some(r(4)))
        );
        // A small budget must still permit the first branch's early match even
        // when a later sibling would exceed the budget if traversal continued.
        assert_eq!(
            get_child_frame(&tree, r(0), b"alpha", SCRIPT_FIND_FLAGS, 3),
            Ok(Some(r(2)))
        );
    }
    #[test]
    fn get_child_frame_rva_0x1c360_self_first_first_hit_wins() {
        // PC RVA 0x1c360 (VA 0x1001c360; REA formed Ghidra body FUN_1001c360,
        // ev_fc2ff1e5...; mafia-re I3d_frame.cpp:732-795): the native checks
        // self first, then recurses children in list order with the same
        // match. The first hit wins; a miss returns NULL (None here).
        // Covered subset only (see the function docs): case-insensitive
        // exact matching under SCRIPT_FIND_FLAGS.
        let ids: Vec<_> = (0..4).map(FrameLifetime::for_existing).collect();
        let r = |i: usize| ids[i].reference();
        let frames = [
            LiveFrame {
                lifetime: &ids[0],
                name: Some(b"root"),
                child: Some(r(1)),
                next: None,
            },
            LiveFrame {
                lifetime: &ids[1],
                name: Some(b"alpha"),
                child: Some(r(2)),
                next: Some(r(3)),
            },
            LiveFrame {
                lifetime: &ids[2],
                name: Some(b"alpha"),
                child: None,
                next: None,
            },
            LiveFrame {
                lifetime: &ids[3],
                name: Some(b"alpha"),
                child: None,
                next: None,
            },
        ];
        let tree = LiveFrameTree::borrow(&frames, true).unwrap();
        // A self match beats every descendant.
        assert_eq!(
            get_child_frame(&tree, r(1), b"ALPHA", SCRIPT_FIND_FLAGS, 3),
            Ok(Some(r(1)))
        );
        // The first descendant in order wins over later duplicates.
        assert_eq!(
            get_child_frame(&tree, r(0), b"alpha", SCRIPT_FIND_FLAGS, 4),
            Ok(Some(r(1)))
        );
        // Misses stay misses.
        assert_eq!(
            get_child_frame(&tree, r(0), b"absent", SCRIPT_FIND_FLAGS, 4),
            Ok(None)
        );
    }
    #[test]
    fn find_child_frame_rva_0x1c330_children_only_cancel_first_hit() {
        // PC RVA 0x1c330 (VA 0x1001c330; REA formed Ghidra body FUN_1001c330,
        // ev_4eafe6bf...; mafia-re I3d_frame.cpp:723-730): EnumFrames over
        // the root's children with FindChildCallBack (store + CANCEL).
        // Children-only: the root itself never matches. The first hit in
        // child-list order wins and later subtrees are never entered.
        use crate::frame_transform::FrameKey;
        use crate::model_loading::ModelArena;
        let mut arena = ModelArena::default();
        arena.set_fast_find_enabled(true);
        let root = arena.create_model().unwrap().root();
        arena.set_frame_name(root, Some(b"hit")).unwrap();
        let second = arena.create_dummy_frame().unwrap();
        arena.set_frame_name(second, Some(b"hit")).unwrap();
        let first = arena.create_dummy_frame().unwrap();
        arena.set_frame_name(first, Some(b"miss")).unwrap();
        let deep = arena.create_dummy_frame().unwrap();
        arena.set_frame_name(deep, Some(b"hit")).unwrap();
        // Prepend order: linking second first leaves [first, second].
        arena.link_frame(second, Some(root), 0).unwrap();
        arena.link_frame(first, Some(root), 0).unwrap();
        arena.link_frame(deep, Some(first), 0).unwrap();
        assert_eq!(arena.links(root).unwrap().child, Some(first));
        // The root's own name never matches (children-only walk); the deep
        // match in the first subtree wins over the second subtree's match.
        assert_eq!(
            find_child_frame(&arena, root, Some(b"hit"), 0xffff, 64),
            Ok(Some(deep))
        );
        // A direct child match short-circuits before deeper subtrees.
        assert_eq!(
            find_child_frame(&arena, root, Some(b"miss"), 0xffff, 64),
            Ok(Some(first))
        );
        // Misses stay misses.
        assert_eq!(
            find_child_frame(&arena, root, Some(b"absent"), 0xffff, 64),
            Ok(None)
        );
        // A null mask matches the first child unconditionally.
        assert_eq!(
            find_child_frame(&arena, root, None, 0xffff, 64),
            Ok(Some(first))
        );
        // Childless frames match nothing.
        let leaf = arena.create_dummy_frame().unwrap();
        assert_eq!(
            find_child_frame(&arena, leaf, Some(b"hit"), 0xffff, 64),
            Ok(None)
        );
        // Stale roots are explicit errors, like enum_subtree.
        assert_eq!(
            find_child_frame(&arena, FrameKey(u32::MAX), Some(b"hit"), 0xffff, 64),
            Err(FrameLookupError::StaleFrame(
                FrameReferenceId::from_existing(u32::MAX)
            ))
        );
        // Re-setting an identical name is a content-equal no-op: no new
        // registration lands in the enabled fast tree (native pointer-then-
        // strcmp arms folded into one content compare, disclosed).
        let adds = arena.fast_tree_add_counter();
        arena.set_frame_name(first, Some(b"miss")).unwrap();
        assert_eq!(arena.fast_tree_add_counter(), adds);
    }
    #[test]
    fn cycles_stale_refs_and_limits_are_errors_not_misses() {
        let id = FrameLifetime::for_existing(7);
        let frames = [LiveFrame {
            lifetime: &id,
            name: Some(b"root"),
            child: Some(id.reference()),
            next: None,
        }];
        let tree = LiveFrameTree::borrow(&frames, true).unwrap();
        assert_eq!(
            get_child_frame(&tree, id.reference(), b"absent", SCRIPT_FIND_FLAGS, 2),
            Err(FrameLookupError::Cycle(id.reference()))
        );
        assert_eq!(
            get_child_frame(&tree, id.reference(), b"absent", SCRIPT_FIND_FLAGS, 1),
            Err(FrameLookupError::VisitLimit)
        );
        let replacement = FrameLifetime::for_existing(7);
        assert_eq!(
            tree.frame(replacement.reference()).unwrap_err(),
            FrameLookupError::StaleFrame(replacement.reference())
        );
        assert_eq!(
            tree.frame(FrameReferenceId::from_existing(7)).unwrap_err(),
            FrameLookupError::StaleFrame(FrameReferenceId::from_existing(7))
        );
        assert_eq!(
            get_child_frame(&tree, id.reference(), b"root", 0, 1),
            Err(FrameLookupError::UnsupportedFlags(0))
        );
        assert_eq!(
            get_child_frame(&tree, id.reference(), b"root", SCRIPT_FIND_FLAGS, 0),
            Err(FrameLookupError::InvalidBudget)
        );
    }
    #[test]
    fn locale_and_non_ascii_domains_stay_explicit() {
        let id = FrameLifetime::for_existing(0);
        let frames = [LiveFrame {
            lifetime: &id,
            name: Some(b"\xff"),
            child: None,
            next: None,
        }];
        let tree = LiveFrameTree::borrow(&frames, false).unwrap();
        assert_eq!(
            get_child_frame(&tree, id.reference(), b"name", SCRIPT_FIND_FLAGS, 1),
            Err(FrameLookupError::UnsupportedLocale)
        );
        let tree = LiveFrameTree::borrow(&frames, true).unwrap();
        assert_eq!(
            get_child_frame(&tree, id.reference(), b"name", SCRIPT_FIND_FLAGS, 1),
            Err(FrameLookupError::UnsupportedName)
        );
    }
    #[test]
    fn deep_linked_hierarchy_uses_bounded_work_storage() {
        let owners: Vec<_> = (0..4096).map(FrameLifetime::for_existing).collect();
        let frames: Vec<_> = owners
            .iter()
            .enumerate()
            .map(|(i, lifetime)| LiveFrame {
                lifetime,
                name: Some(if i == 4095 { b"leaf" } else { b"node" }),
                child: owners.get(i + 1).map(FrameLifetime::reference),
                next: None,
            })
            .collect();
        let tree = LiveFrameTree::borrow(&frames, true).unwrap();
        assert_eq!(
            get_child_frame(
                &tree,
                owners[0].reference(),
                b"leaf",
                SCRIPT_FIND_FLAGS,
                4096
            ),
            Ok(Some(owners[4095].reference()))
        );
        assert_eq!(
            get_child_frame(
                &tree,
                owners[0].reference(),
                b"leaf",
                SCRIPT_FIND_FLAGS,
                4095
            ),
            Err(FrameLookupError::VisitLimit)
        );
    }
    #[test]
    fn nullable_names_missing_roots_and_shared_descendants() {
        let ids: Vec<_> = (0..4).map(FrameLifetime::for_existing).collect();
        let r = |i: usize| ids[i].reference();
        let frames = [
            LiveFrame {
                lifetime: &ids[0],
                name: None,
                child: Some(r(1)),
                next: None,
            },
            LiveFrame {
                lifetime: &ids[1],
                name: Some(b""),
                child: Some(r(3)),
                next: Some(r(2)),
            },
            LiveFrame {
                lifetime: &ids[2],
                name: Some(b"branch"),
                child: Some(r(3)),
                next: None,
            },
            LiveFrame {
                lifetime: &ids[3],
                name: Some(b"leaf"),
                child: None,
                next: None,
            },
        ];
        let tree = LiveFrameTree::borrow(&frames, true).unwrap();
        assert_eq!(
            get_child_frame(&tree, r(0), b"", SCRIPT_FIND_FLAGS, 5),
            Ok(Some(r(1)))
        );
        // A shared descendant is revisited and is not an ancestor cycle.
        assert_eq!(
            get_child_frame(&tree, r(0), b"absent", SCRIPT_FIND_FLAGS, 5),
            Ok(None)
        );
        assert_eq!(
            find_frame(&tree, SceneRoots::default(), b"leaf", SCRIPT_FIND_FLAGS, 5),
            Err(FrameLookupError::MissingPrimaryRoot)
        );
        let roots = SceneRoots {
            primary: Some(r(0)),
            backdrop: None,
        };
        assert_eq!(
            find_frame(&tree, roots, b"leaf", SCRIPT_FIND_FLAGS, 5),
            Ok(Some(r(3)))
        );
        assert_eq!(
            find_frame(&tree, roots, b"absent", SCRIPT_FIND_FLAGS, 6),
            Err(FrameLookupError::MissingBackdropRoot)
        );
    }
    #[test]
    fn find_child_callback_slot_stores_then_cancels() {
        // RVA 0x1c310: unconditional store + CANCEL word (2). The x86
        // stdcall callee-cleanup has no host equivalent.
        use crate::frame_transform::FrameKey;
        let mut slot = None;
        assert_eq!(find_child_callback_slot(&mut slot, FrameKey(9)), 2);
        assert_eq!(slot, Some(FrameKey(9)));
        // Overwrites unconditionally (native stores before returning).
        assert_eq!(find_child_callback_slot(&mut slot, FrameKey(3)), 2);
        assert_eq!(slot, Some(FrameKey(3)));
    }
    #[test]
    fn enum_subtree_self_first_type_mask_and_cancel() {
        // RVA 0x1c020 (mafia-re I3d_frame.cpp:629): self-first preorder.
        use crate::frame_transform::{FrameKey, I3dFrameState};
        use crate::model_loading::ModelArena;
        let mut arena = ModelArena::default();
        let root = arena.create_model().unwrap().root();
        arena.set_frame_name(root, Some(b"root")).unwrap();
        let dummy = arena.create_dummy_frame().unwrap();
        arena.set_frame_name(dummy, Some(b"HIT")).unwrap();
        let target = arena
            .insert(I3dFrameState::target(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena.set_frame_name(target, Some(b"hit")).unwrap();
        arena.link_frame(dummy, Some(root), 0).unwrap();
        arena.link_frame(target, Some(root), 0).unwrap();
        let mut seen = Vec::new();
        let mut collect = |key: FrameKey| {
            seen.push(key);
            ENUM_OK
        };
        assert_eq!(
            enum_subtree(&arena, root, &mut collect, 0x40, Some(b"hit"), 16),
            Ok(ENUM_OK)
        );
        assert_eq!(seen, vec![target]);
        let mut calls = 0;
        let mut cancel = |_: FrameKey| {
            calls += 1;
            ENUM_CANCEL
        };
        assert_eq!(
            enum_subtree(&arena, root, &mut cancel, 0xffff, None, 16),
            Ok(ENUM_CANCELED)
        );
        assert_eq!(calls, 1);
    }
    #[test]
    fn enum_subtree_wild_skip_null_and_budget() {
        use crate::frame_transform::I3dFrameState;
        use crate::model_loading::ModelArena;
        let mut arena = ModelArena::default();
        let root = arena.create_model().unwrap().root();
        arena.set_frame_name(root, Some(b"root")).unwrap();
        let child = arena.create_dummy_frame().unwrap();
        arena.set_frame_name(child, Some(b"abc")).unwrap();
        let grand = arena
            .insert(I3dFrameState::target(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena.set_frame_name(grand, Some(b"abd")).unwrap();
        arena.link_frame(child, Some(root), 0).unwrap();
        arena.link_frame(grand, Some(child), 0).unwrap();
        // Wildcard prefix with SKIPCHILDREN skips the grandchild.
        let mut seen = Vec::new();
        let mut skip = |key: crate::frame_transform::FrameKey| {
            seen.push(key);
            if key == child {
                ENUM_SKIPCHILDREN
            } else {
                ENUM_OK
            }
        };
        assert_eq!(
            enum_subtree(&arena, root, &mut skip, 0x1ffff, Some(b"ab*"), 16),
            Ok(ENUM_OK)
        );
        assert_eq!(seen, vec![child]);
        // Null original names never match a present mask but still descend.
        let bare = arena.create_dummy_frame().unwrap();
        arena.link_frame(bare, Some(root), 0).unwrap();
        let count = std::cell::Cell::new(0u32);
        let mut all = |_: crate::frame_transform::FrameKey| {
            count.set(count.get() + 1);
            ENUM_OK
        };
        assert_eq!(
            enum_subtree(&arena, root, &mut all, 0x1ffff, Some(b"ab*"), 16),
            Ok(ENUM_OK)
        );
        // Child and (unskipped) grandchild fire; root mismatches, the
        // null-named bare frame never matches though it still descends.
        assert_eq!(count.get(), 2);
        // Exhausted budget and stale roots are explicit.
        assert_eq!(
            enum_subtree(&arena, root, &mut all, 0xffff, None, 0),
            Err(FrameLookupError::InvalidBudget)
        );
        assert_eq!(
            enum_subtree(
                &arena,
                crate::frame_transform::FrameKey(u32::MAX),
                &mut all,
                0xffff,
                None,
                16
            ),
            Err(FrameLookupError::StaleFrame(
                FrameReferenceId::from_existing(u32::MAX)
            ))
        );
    }
    #[test]
    fn enum_scope_bits_report_only_their_bits() {
        // Working names for the inlined ENUMF_* scope gates (I3D.h:226-227,
        // 1329-1331): no LS3DF FUNCTION addresses exist for standalone readers.
        assert!(enum_scope_includes_backdrop(0x40000));
        assert!(enum_scope_includes_omni(0x80000));
        assert!(enum_scope_includes_near(0x100000));
        assert!(enum_scope_wildmask(0x10000));
        assert!(enum_scope_case_sensitive(0x20000));
        assert!(!enum_scope_includes_backdrop(0));
        assert!(!enum_scope_includes_omni(0x40000));
        assert!(!enum_scope_includes_near(0x80000));
        assert!(!enum_scope_wildmask(0x20000));
        assert!(!enum_scope_case_sensitive(0x10000));
        assert!(!enum_scope_includes_backdrop(u32::MAX & !0x40000));
        assert!(!enum_scope_includes_omni(u32::MAX & !0x80000));
        assert!(!enum_scope_includes_near(u32::MAX & !0x100000));
        assert!(!enum_scope_wildmask(u32::MAX & !0x10000));
        assert!(!enum_scope_case_sensitive(u32::MAX & !0x20000));
        assert!(enum_scope_includes_backdrop(u32::MAX));
        assert!(enum_scope_includes_omni(u32::MAX));
        assert!(enum_scope_includes_near(u32::MAX));
        assert!(enum_scope_wildmask(u32::MAX));
        assert!(enum_scope_case_sensitive(u32::MAX));
    }
    #[test]
    fn enum_wants_visual_reports_only_its_bit() {
        // Working name for the ENUMF_VISUAL gate (I3D.h:211): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_visual(0x1));
        assert!(enum_wants_visual(0xffff));
        assert!(enum_wants_visual(u32::MAX));
        assert!(!enum_wants_visual(0));
        assert!(!enum_wants_visual(0x2));
        assert!(!enum_wants_visual(u32::MAX & !0x1));
    }
    #[test]
    fn enum_wants_light_reports_only_its_bit() {
        // Working name for the ENUMF_LIGHT gate (I3D.h:212): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_light(0x2));
        assert!(enum_wants_light(0xffff));
        assert!(!enum_wants_light(0));
        assert!(!enum_wants_light(0x1));
        assert!(!enum_wants_light(u32::MAX & !0x2));
    }
    #[test]
    fn enum_wants_camera_reports_only_its_bit() {
        // Working name for the ENUMF_CAMERA gate (I3D.h:213): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_camera(0x4));
        assert!(enum_wants_camera(0xffff));
        assert!(!enum_wants_camera(0));
        assert!(!enum_wants_camera(0x8));
        assert!(!enum_wants_camera(u32::MAX & !0x4));
    }
    #[test]
    fn enum_wants_sound_reports_only_its_bit() {
        // Working name for the ENUMF_SOUND gate (I3D.h:214): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_sound(0x8));
        assert!(enum_wants_sound(0xffff));
        assert!(!enum_wants_sound(0));
        assert!(!enum_wants_sound(0x4));
        assert!(!enum_wants_sound(u32::MAX & !0x8));
    }
    #[test]
    fn enum_wants_sector_reports_only_its_bit() {
        // Working name for the ENUMF_SECTOR gate (I3D.h:215): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_sector(0x10));
        assert!(enum_wants_sector(0xffff));
        assert!(!enum_wants_sector(0));
        assert!(!enum_wants_sector(0x20));
        assert!(!enum_wants_sector(u32::MAX & !0x10));
    }
    #[test]
    fn enum_wants_dummy_reports_only_its_bit() {
        // Working name for the ENUMF_DUMMY gate (I3D.h:216): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_dummy(0x20));
        assert!(enum_wants_dummy(0xffff));
        assert!(!enum_wants_dummy(0));
        assert!(!enum_wants_dummy(0x10));
        assert!(!enum_wants_dummy(u32::MAX & !0x20));
    }
    #[test]
    fn enum_wants_target_reports_only_its_bit() {
        // Working name for the ENUMF_TARGET gate (I3D.h:217, value a guess):
        // no LS3DF FUNCTION address exists for a standalone reader.
        assert!(enum_wants_target(0x40));
        assert!(enum_wants_target(0xffff));
        assert!(!enum_wants_target(0));
        assert!(!enum_wants_target(0x80));
        assert!(!enum_wants_target(u32::MAX & !0x40));
    }
    #[test]
    fn enum_wants_model_reports_only_its_bit() {
        // Working name for the ENUMF_MODEL gate (I3D.h:218): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_model(0x80));
        assert!(enum_wants_model(0xffff));
        assert!(!enum_wants_model(0));
        assert!(!enum_wants_model(0x40));
        assert!(!enum_wants_model(u32::MAX & !0x80));
    }
    #[test]
    fn enum_wants_user_reports_only_its_bit() {
        // Working name for the ENUMF_USER gate (I3D.h:219): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_user(0x100));
        assert!(enum_wants_user(0xffff));
        assert!(!enum_wants_user(0));
        assert!(!enum_wants_user(0x200));
        assert!(!enum_wants_user(u32::MAX & !0x100));
    }
    #[test]
    fn enum_wants_volume_reports_only_its_bit() {
        // Working name for the ENUMF_VOLUME gate (I3D.h:220): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_volume(0x200));
        assert!(enum_wants_volume(0xffff));
        assert!(!enum_wants_volume(0));
        assert!(!enum_wants_volume(0x100));
        assert!(!enum_wants_volume(u32::MAX & !0x200));
    }
    #[test]
    fn enum_wants_joint_reports_only_its_bit() {
        // Working name for the ENUMF_JOINT gate (I3D.h:221): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_joint(0x400));
        assert!(enum_wants_joint(0xffff));
        assert!(!enum_wants_joint(0));
        assert!(!enum_wants_joint(0x800));
        assert!(!enum_wants_joint(u32::MAX & !0x400));
    }
    #[test]
    fn enum_wants_occluder_reports_only_its_bit() {
        // Working name for the ENUMF_OCCLUDER gate (I3D.h:222): no LS3DF
        // FUNCTION address exists for a standalone reader. The gate only
        // reports the request bit; the native never-match rule for partial
        // masks stays with the walker (disclosed in the function docs).
        assert!(enum_wants_occluder(0x800));
        assert!(enum_wants_occluder(0xffff));
        assert!(!enum_wants_occluder(0));
        assert!(!enum_wants_occluder(0x400));
        assert!(!enum_wants_occluder(u32::MAX & !0x800));
    }
    #[test]
    fn enum_wants_area_reports_only_its_bit() {
        // Working name for the ENUMF_AREA gate (I3D.h:223): no LS3DF
        // FUNCTION address exists for a standalone reader.
        assert!(enum_wants_area(0x1000));
        assert!(enum_wants_area(0xffff));
        assert!(!enum_wants_area(0));
        assert!(!enum_wants_area(0x2000));
        assert!(!enum_wants_area(u32::MAX & !0x1000));
    }
    #[test]
    fn enum_wants_shadow_reports_only_its_bit() {
        // Working name for the ENUMF_SHADOW gate (I3D.h:224, value a guess):
        // no LS3DF FUNCTION address exists for a standalone reader.
        assert!(enum_wants_shadow(0x2000));
        assert!(enum_wants_shadow(0xffff));
        assert!(!enum_wants_shadow(0));
        assert!(!enum_wants_shadow(0x1000));
        assert!(!enum_wants_shadow(u32::MAX & !0x2000));
    }
    #[test]
    fn enum_matches_all_kinds_reports_only_the_full_low_word() {
        // Working name for the ENUMF_ALL bypass (I3D.h:225): no LS3DF
        // FUNCTION address exists for a standalone reader. Only a full low
        // word skips the native per-kind switch; scope bits above it never
        // affect the bypass.
        assert!(enum_matches_all_kinds(0xffff));
        assert!(enum_matches_all_kinds(u32::MAX));
        assert!(!enum_matches_all_kinds(0));
        assert!(!enum_matches_all_kinds(0xfffe));
        assert!(!enum_matches_all_kinds(0x1ffff & !0x8000));
        assert!(!enum_matches_all_kinds(u32::MAX & !0x4000));
    }
    #[test]
    fn child_frame_self_type_gate_models_joint_quirk_and_all_bypass() {
        // Working names for the GetChildFrame self type gate
        // (I3d_frame.cpp:737-755): no LS3DF FUNCTION address exists for a
        // standalone reader (PC RVA 0x1c360 is occupied, not claimed here).
        assert!(get_child_frame_self_type_accepts(FrameKind::Model, 0xffff));
        assert!(get_child_frame_self_type_accepts(FrameKind::Joint, 0xffff));
        assert!(get_child_frame_self_type_accepts(FrameKind::Model, 0x80));
        assert!(get_child_frame_self_type_accepts(FrameKind::Dummy, 0x20));
        assert!(get_child_frame_self_type_accepts(FrameKind::Area, 0x1000));
        assert!(!get_child_frame_self_type_accepts(FrameKind::Model, 0x1));
        assert!(!get_child_frame_self_type_accepts(FrameKind::Dummy, 0));
        // The native switch has no JOINT arm: a joint never self-matches,
        // even with ENUMF_JOINT set (default: ok = 0).
        assert!(!get_child_frame_self_type_accepts(FrameKind::Joint, 0x400));
        assert!(!get_child_frame_self_type_accepts(FrameKind::Joint, 0x4ff));
        // Occluder and scene frames have no arm either (mask bit zero).
        assert!(!get_child_frame_self_type_accepts(
            FrameKind::Occluder,
            0x800
        ));
        assert!(!get_child_frame_self_type_accepts(FrameKind::Scene, 0x800));
    }
    #[test]
    fn child_frame_self_name_gate_models_null_and_mask_arms() {
        // Working names for the GetChildFrame self name gate
        // (I3d_frame.cpp:756-786): no LS3DF FUNCTION address exists for a
        // standalone reader (PC RVA 0x1c360 is occupied, not claimed here).
        assert!(get_child_frame_self_name_accepts(Some(b"a"), None, 0));
        assert!(get_child_frame_self_name_accepts(None, None, 0x20000));
        assert!(!get_child_frame_self_name_accepts(None, Some(b"a"), 0));
        assert!(get_child_frame_self_name_accepts(
            Some(b"hit"),
            Some(b"hit"),
            0x20000
        ));
        assert!(!get_child_frame_self_name_accepts(
            Some(b"hit"),
            Some(b"miss"),
            0x20000
        ));
        assert!(get_child_frame_self_name_accepts(
            Some(b"AlPhA"),
            Some(b"alpha"),
            0
        ));
        assert!(get_child_frame_self_name_accepts(
            Some(b"prefix_rest"),
            Some(b"prefix*"),
            0x10000
        ));
        assert!(!get_child_frame_self_name_accepts(
            Some(b"other"),
            Some(b"prefix*"),
            0x10000
        ));
    }
    #[test]
    fn find_frame_visit_order_is_primary_then_flag_gated_sectors() {
        // Working names for the FindFrame sector order (I3d_scene.cpp:228-249):
        // no LS3DF FUNCTION address exists for a standalone reader (ledger
        // RVA 0x49210 is occupied, not claimed here).
        use FindFrameSector::{Backdrop, Near, Omni, Primary};
        assert_eq!(
            find_frame_sector_visit_order(0),
            [Some(Primary), None, None, None]
        );
        assert_eq!(
            find_frame_sector_visit_order(0x40000),
            [Some(Primary), Some(Backdrop), None, None]
        );
        assert_eq!(
            find_frame_sector_visit_order(0x80000),
            [Some(Primary), None, Some(Omni), None]
        );
        assert_eq!(
            find_frame_sector_visit_order(0x100000),
            [Some(Primary), None, None, Some(Near)]
        );
        assert_eq!(
            find_frame_sector_visit_order(0x1c0000),
            [Some(Primary), Some(Backdrop), Some(Omni), Some(Near)]
        );
        assert_eq!(
            find_frame_sector_visit_order(u32::MAX),
            [Some(Primary), Some(Backdrop), Some(Omni), Some(Near)]
        );
    }
    #[test]
    fn sector_walk_stop_rules_match_native_branches() {
        // Working names for the EnumFrames error short-circuit
        // (I3d_scene.cpp:206-223, PC RVA 0x49180 occupied) and the FindFrame
        // first-hit return (I3d_scene.cpp:230-248, ledger RVA 0x49210
        // occupied): no standalone LS3DF FUNCTION entries exist, so no RVA is
        // claimed here.
        assert!(scene_enum_frames_stops_on_error(-1));
        assert!(scene_enum_frames_stops_on_error(i32::MIN));
        assert!(!scene_enum_frames_stops_on_error(0));
        assert!(!scene_enum_frames_stops_on_error(1));
        assert!(find_frame_stops_at_first_hit(true));
        assert!(!find_frame_stops_at_first_hit(false));
    }
}
