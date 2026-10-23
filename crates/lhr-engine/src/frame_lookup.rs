//! LS3DF I3D_scene::FindFrame / I3D_frame::GetChildFrame, bounded to flags
//! 0x4ffff and exact ASCII names under the original default C-locale branch.
//! Views borrow existing live frame owners; this module constructs no scene.
//! These PC method names follow the pinned mafia-re reconstruction, corroborated
//! by actual PS2 linker names. New PC debug symbols, complete PC/PS2 body
//! correspondence, layouts and full-function parity are not claimed.
use crate::script_scheduler::FrameReferenceId;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};

pub const SCRIPT_FIND_FLAGS: u32 = 0x4ffff;
pub const MAX_LOOKUP_VISITS: usize = 65_536;
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
}
impl FrameLookupBackend for ModelFrameLookup<'_> {
    fn default_c_locale(&self) -> bool {
        self.default_c_locale
    }
    fn supports_empty_query(&self) -> bool {
        false
    }
    fn frame(&self, reference: FrameReferenceId) -> Result<FrameView<'_>, FrameLookupError> {
        let key = crate::frame_transform::FrameKey(reference.identity());
        if self.arena.lookup_reference(key) != reference {
            return Err(FrameLookupError::StaleFrame(reference));
        }
        let frame = self
            .arena
            .frame(key)
            .map_err(|_| FrameLookupError::StaleFrame(reference))?;
        let links = self
            .arena
            .links(key)
            .map_err(|_| FrameLookupError::StaleFrame(reference))?;
        Ok(FrameView {
            // Empty metadata is ambiguous; this adapter rejects empty queries.
            name: (!frame.name_bytes.is_empty()).then_some(frame.name_bytes.as_slice()),
            child: links.child.map(|key| self.arena.lookup_reference(key)),
            next: links.next.map(|key| self.arena.lookup_reference(key)),
        })
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
}
