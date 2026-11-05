#![forbid(unsafe_code)]
//! Index-buffer manager construction and VRAM reclamation.
//!
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! RVAs 0x5b390 (`C_IBManager::C_IBManager`), 0x5b5c0
//! (`C_IBManager::~C_IBManager`), 0x5baa0
//! (`C_IBManager::m_FreeAllVRAMIBuffers`). Pinned mafia-re ab67b13
//! `ls3df/I3D/render/IBManager.cpp:221-239,341-355` and
//! `ls3df/I3D/render/IBManager.h:42-57`.
//!
//! Contract tier this wave: reviewed upstream source plus Rust tests, no
//! Ghidra body claimed. This extends the ledger-ported block semantics in
//! `crate::buffer_managers` (RVAs 0x5abf0/0x5ae00/0x5aff0/0x5b1b0/0x5b6c0/
//! 0x5b940/0x5b960/0x5b9d0/0x5ba30, never re-claimed here): only manager
//! construction/destruction order and the DEFAULT-pool restart scan are
//! ported. Device destruction stays behind a backend trait.

use crate::buffer_managers::Pool;
use std::collections::BTreeMap;

/// Minimum buffer indices (0x10000, the 0x20000 bytes of
/// `RB_MANAGER_MEM_SIZE`; IBManager.cpp:92-93).
pub const IB_MANAGER_MIN_INDICES: u32 = 0x10000;

/// Typed index-buffer manager: one pool word per live buffer in manager
/// order. Block partitioning itself stays with `buffer_managers.rs`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IbManager {
    /// Live buffer pools in manager order.
    pub pools: Vec<Pool>,
    /// `locked_buffers` reservation (256).
    pub locked_reserved: usize,
    /// `locked_buffers` grow size (256).
    pub locked_grow: usize,
}

/// Device-adjacent buffer destruction behind a backend trait.
pub trait IbManagerBackend {
    /// `delete buffer` at manager position `index` with this pool.
    fn destroy_ibuffer(&mut self, index: usize, pool: Pool);
}

/// LS3DF RVA 0x5b390 `C_IBManager::C_IBManager` (mafia-re
/// `ls3df/I3D/render/IBManager.cpp:221-229`): empty membership with the
/// 256-entry locked reservation. Pure CPU words; no device call.
pub fn new_ib_manager() -> IbManager {
    IbManager {
        pools: Vec::new(),
        locked_reserved: 256,
        locked_grow: 256,
    }
}

/// LS3DF RVA 0x5b5c0 `C_IBManager::~C_IBManager` (mafia-re
/// `ls3df/I3D/render/IBManager.cpp:231-239`): deletes every buffer in
/// manager order, then clears membership. No unreleased warning (unlike
/// `m_FreeAllIBuffers`); destruction effects stay behind the backend.
/// Order is host insertion order (same limitation as the FreeAll scan).
pub fn drop_ib_manager(manager: &mut IbManager, backend: &mut impl IbManagerBackend) {
    for (index, pool) in manager.pools.iter().enumerate() {
        backend.destroy_ibuffer(index, *pool);
    }
    manager.pools.clear();
}

/// LS3DF RVA 0x5baa0 `C_IBManager::m_FreeAllVRAMIBuffers` (mafia-re
/// `ls3df/I3D/render/IBManager.cpp:341-355`): deletes buffers in the
/// default pool, which a reset device loses. Restart-from-begin after each
/// delete (`goto restart`). Managed buffers are retained untouched.
/// Scan order is host insertion order, not the native sorted-multiset
/// order (disclosed); tests pin set membership, not sorted order.
pub fn free_all_vram_ibuffers(manager: &mut IbManager, backend: &mut impl IbManagerBackend) {
    loop {
        let mut found: Option<usize> = None;
        for (index, pool) in manager.pools.iter().enumerate() {
            if *pool == Pool::Default {
                found = Some(index);
                break;
            }
        }
        let Some(index) = found else {
            break;
        };
        let pool = manager.pools[index];
        backend.destroy_ibuffer(index, pool);
        manager.pools.remove(index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Log {
        destroyed: Vec<(usize, Pool)>,
    }
    impl IbManagerBackend for Log {
        fn destroy_ibuffer(&mut self, index: usize, pool: Pool) {
            self.destroyed.push((index, pool));
        }
    }

    #[test]
    fn new_manager_is_empty_with_locked_reservation() {
        let manager = new_ib_manager();
        assert!(manager.pools.is_empty());
        assert_eq!(manager.locked_reserved, 256);
        assert_eq!(manager.locked_grow, 256);
        assert_eq!(IB_MANAGER_MIN_INDICES, 0x10000);
    }

    #[test]
    fn drop_deletes_every_buffer_in_order_then_clears() {
        let mut manager = IbManager {
            pools: vec![Pool::Default, Pool::Managed, Pool::Default],
            locked_reserved: 256,
            locked_grow: 256,
        };
        let mut backend = Log {
            destroyed: Vec::new(),
        };
        drop_ib_manager(&mut manager, &mut backend);
        assert_eq!(
            backend.destroyed,
            vec![(0, Pool::Default), (1, Pool::Managed), (2, Pool::Default),]
        );
        assert!(manager.pools.is_empty());
    }

    #[test]
    fn vram_walk_frees_only_default_with_restart() {
        let mut manager = IbManager {
            pools: vec![Pool::Default, Pool::Managed, Pool::Default, Pool::Managed],
            locked_reserved: 256,
            locked_grow: 256,
        };
        let mut backend = Log {
            destroyed: Vec::new(),
        };
        free_all_vram_ibuffers(&mut manager, &mut backend);
        assert_eq!(manager.pools, vec![Pool::Managed, Pool::Managed]);
        assert_eq!(backend.destroyed.len(), 2);
        for (_, pool) in &backend.destroyed {
            assert_eq!(*pool, Pool::Default);
        }
        backend.destroyed.clear();
        free_all_vram_ibuffers(&mut manager, &mut backend);
        assert!(backend.destroyed.is_empty());
    }
}

// ---------------------------------------------------------------------------
// VC6 red-black-tree (`<xtree>`) instantiations behind the IB/VB managers.
//
// LS3DF RVAs 0x5ab00-0x5d000 (VA 0x1005ab00-0x1005d000); pinned mafia-re
// ab67b13 `ls3df/I3D/render/IBManager.h:184-286`, all TEMPLATE, NOT game
// logic. Every name below is an explicitly descriptive working name, NOT an
// original symbol: VC6 emits these per-instantiation (`multiset<S_IB_block
// *>`, `multiset<C_IBuffer *>`, plus the VBManager-folded copies
// `begin<VB_block>`, `_Buynode<VBuffer>`, `_Construct<VB_block*>`,
// `C_list<IB_block*>::reserve` and the `pair<iterator, bool>` ctor).
//
// Host coverage follows the rw_data STL precedent (host containers cover VC6
// instantiations): ordered-multiset behavior lives in `BTreeMap`-backed typed
// multisets keyed by (sort word, insertion sequence), so duplicates keep a
// stable order exactly like the native tree's equal-key runs. Node-level
// `_Tree` mechanics (rotations, `_Min`, `_Buynode`, `_Init`, `_Erase`,
// `_Insert`, successor/predecessor steps) live in the synthetic
// `SyntheticTree` with explicit parent/child/color links, hand-derived from
// the VC6 `<xtree>` roles described in `IBManager.h:184-286`; its fixup
// mirrors the reviewed `buffer_managers::ManagerTree` insert path. The host
// has no black-nil sentinel or allocator object: an empty tree is `root:
// None` with no nodes, and iterators are `Option<key>` (`None` is `end()`).
// Native-undefined inputs (missing keys, vacant ids, occupied link slots)
// are total host no-ops or explicit `StlError`s, disclosed per item. All
// contract tier (wave policy: no Ghidra bodies observed).

/// Host admission or link-consistency failure for the synthetic STL tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StlError {
    HostAllocation,
    UnknownNode,
    SlotOccupied,
}

/// Host sort word for one free block (`S_IB_block_less`: smallest `size`
/// first; the sequence keeps multiset duplicates in insertion order).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IbBlockSortKey {
    pub size: u32,
    pub seq: u64,
}

/// Host payload for one free block (`S_IB_block`: owner-cleared index/size
/// words; the native owner pointer stays with `buffer_managers`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IbBlockEntry {
    pub index: u32,
    pub size: u32,
}

/// Host `multiset<S_IB_block *, S_IB_block_less>` ordered membership.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IbBlockMultiset {
    entries: BTreeMap<IbBlockSortKey, IbBlockEntry>,
    next_seq: u64,
}

impl IbBlockMultiset {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Host sort word for one manager buffer (`S_IBuffer_less`: least `num_free`
/// first, as of insertion; the sequence keeps duplicates stable).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct IBufferSortKey {
    pub num_free: u32,
    pub seq: u64,
}

/// Host payload for one manager buffer (buffer id plus its free count).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IBufferEntry {
    pub buffer: u32,
    pub num_free: u32,
}

/// Host `multiset<C_IBuffer *, S_IBuffer_less>` ordered membership. Sequence
/// words on keys keep duplicates stable; `ibuffer_multiset_insert` mints
/// them monotonically (a caller-minted `len` would collide after erases).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IBufferMultiset {
    entries: BTreeMap<IBufferSortKey, IBufferEntry>,
    next_seq: u64,
}

impl IBufferMultiset {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Multiset insert with a monotonic sequence word (mirrors the IbBlock twin:
/// caller-minted `len` collides after erases and would overwrite a survivor
/// in the ordered map; the sequence never reuses while the set lives, with
/// wrapping admission past u64::MAX).
pub fn ibuffer_multiset_insert(
    set: &mut IBufferMultiset,
    num_free: u32,
    buffer: u32,
) -> (IBufferSortKey, bool) {
    let key = IBufferSortKey {
        num_free,
        seq: set.next_seq,
    };
    set.next_seq = set.next_seq.wrapping_add(1);
    set.entries.insert(key, IBufferEntry { buffer, num_free });
    (key, true)
}

/// Host `pair<multiset<C_IBuffer *>::iterator, bool>` insert result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IBufferInsertResult {
    pub pos: Option<IBufferSortKey>,
    pub inserted: bool,
}

/// Host sort word for one free vertex block (`S_VB_block_less`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VbBlockSortKey {
    pub size: u32,
    pub seq: u64,
}

/// Host payload for one free vertex block (index/size words).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VbBlockEntry {
    pub index: u32,
    pub size: u32,
}

/// Host `multiset<S_VB_block *, S_VB_block_less>` membership (VBManager-folded
/// `begin`/`_Construct` copies address this type). Sequences come from
/// `vb_block_multiset_insert`, like on the IBuffer twin.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VbBlockMultiset {
    entries: BTreeMap<VbBlockSortKey, VbBlockEntry>,
    next_seq: u64,
}

impl VbBlockMultiset {
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Multiset insert with a monotonic sequence word (same never-reuse rule as
/// the IBuffer twin above).
pub fn vb_block_multiset_insert(
    set: &mut VbBlockMultiset,
    size: u32,
    index: u32,
) -> (VbBlockSortKey, bool) {
    let key = VbBlockSortKey {
        size,
        seq: set.next_seq,
    };
    set.next_seq = set.next_seq.wrapping_add(1);
    set.entries.insert(key, VbBlockEntry { index, size });
    (key, true)
}

/// Synthetic VC6 `_Tree` node: explicit parent/child links plus color and a
/// host sort word. Vacant `nodes` slots are the host free list reused by
/// `_Buynode`; ids are slot indices. Shared by the IB-block and IBuffer
/// instantiations exactly like the native `<xtree>` logic is shared; the
/// per-instantiation distinction is carried by the calling free function
/// (separate RVA credit each).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyntheticTreeNode {
    pub parent: Option<u32>,
    pub left: Option<u32>,
    pub right: Option<u32>,
    pub red: bool,
    pub sort: u64,
}

/// Synthetic VC6 `_Tree` with explicit links (no sentinel head node).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyntheticTree {
    pub nodes: Vec<Option<SyntheticTreeNode>>,
    pub root: Option<u32>,
}

fn node_ref(tree: &SyntheticTree, id: u32) -> Option<SyntheticTreeNode> {
    tree.nodes.get(id as usize)?.as_ref().copied()
}

fn is_red(tree: &SyntheticTree, id: Option<u32>) -> bool {
    id.is_some_and(|id| node_ref(tree, id).is_some_and(|node| node.red))
}

fn paint(tree: &mut SyntheticTree, id: Option<u32>, red: bool) {
    if let Some(id) = id {
        if let Some(slot) = tree.nodes.get_mut(id as usize).and_then(Option::as_mut) {
            slot.red = red;
        }
    }
}

/// Promote `replacement` into `old`'s link (root when parentless). Links that
/// do not reference `old` are left untouched (host admission; native links
/// are consistent by construction).
fn replace(tree: &mut SyntheticTree, old: u32, replacement: Option<u32>) {
    let parent = node_ref(tree, old).and_then(|node| node.parent);
    if let Some(parent) = parent {
        if let Some(slot) = tree.nodes.get_mut(parent as usize).and_then(Option::as_mut) {
            if slot.right == Some(old) {
                slot.right = replacement;
            } else if slot.left == Some(old) {
                slot.left = replacement;
            }
        }
    } else {
        tree.root = replacement;
    }
    if let Some(replacement) = replacement {
        if let Some(slot) = tree
            .nodes
            .get_mut(replacement as usize)
            .and_then(Option::as_mut)
        {
            slot.parent = parent;
        }
    }
}

/// Native `_Lrotate` mechanics: promote the right child. A missing child is
/// a host no-op (native dereference there is undefined).
fn rotate_left(tree: &mut SyntheticTree, node: u32) {
    let pivot = match node_ref(tree, node).and_then(|node| node.right) {
        Some(pivot) if node_ref(tree, pivot).is_some() => pivot,
        _ => return,
    };
    let middle = node_ref(tree, pivot).and_then(|node| node.left);
    if let Some(slot) = tree.nodes.get_mut(node as usize).and_then(Option::as_mut) {
        slot.right = middle;
    }
    if let Some(middle) = middle {
        if let Some(slot) = tree.nodes.get_mut(middle as usize).and_then(Option::as_mut) {
            slot.parent = Some(node);
        }
    }
    replace(tree, node, Some(pivot));
    if let Some(slot) = tree.nodes.get_mut(pivot as usize).and_then(Option::as_mut) {
        slot.left = Some(node);
    }
    if let Some(slot) = tree.nodes.get_mut(node as usize).and_then(Option::as_mut) {
        slot.parent = Some(pivot);
    }
}

/// Native `_Rrotate` mechanics: promote the left child (mirror of left).
fn rotate_right(tree: &mut SyntheticTree, node: u32) {
    let pivot = match node_ref(tree, node).and_then(|node| node.left) {
        Some(pivot) if node_ref(tree, pivot).is_some() => pivot,
        _ => return,
    };
    let middle = node_ref(tree, pivot).and_then(|node| node.right);
    if let Some(slot) = tree.nodes.get_mut(node as usize).and_then(Option::as_mut) {
        slot.left = middle;
    }
    if let Some(middle) = middle {
        if let Some(slot) = tree.nodes.get_mut(middle as usize).and_then(Option::as_mut) {
            slot.parent = Some(node);
        }
    }
    replace(tree, node, Some(pivot));
    if let Some(slot) = tree.nodes.get_mut(pivot as usize).and_then(Option::as_mut) {
        slot.right = Some(node);
    }
    if let Some(slot) = tree.nodes.get_mut(node as usize).and_then(Option::as_mut) {
        slot.parent = Some(pivot);
    }
}

/// Native `_Insert` rebalance after linking a red node. Mirrors the reviewed
/// `buffer_managers::ManagerTree` insert path (red-uncle recolor, inner
/// rotation, outer rotation, root repaint). The iteration cap only bounds
/// inconsistent-link input; valid trees settle in height steps.
fn insert_fixup(tree: &mut SyntheticTree, mut id: u32) {
    let cap = tree.nodes.len().saturating_add(2);
    for _ in 0..cap {
        let parent = match node_ref(tree, id).and_then(|node| node.parent) {
            Some(parent) => parent,
            None => break,
        };
        if !is_red(tree, Some(parent)) {
            break;
        }
        // A red parent without a grandparent is inconsistent (the root stays
        // black); repaint the root and stop instead of unwrapping.
        let grand = match node_ref(tree, parent).and_then(|node| node.parent) {
            Some(grand) => grand,
            None => break,
        };
        let parent_is_right = node_ref(tree, grand).and_then(|node| node.right) == Some(parent);
        let uncle = node_ref(tree, grand).and_then(|node| {
            if parent_is_right {
                node.left
            } else {
                node.right
            }
        });
        if is_red(tree, uncle) {
            paint(tree, Some(parent), false);
            paint(tree, uncle, false);
            paint(tree, Some(grand), true);
            id = grand;
        } else {
            let id_is_inner = node_ref(tree, parent).and_then(|node| {
                if parent_is_right {
                    node.left
                } else {
                    node.right
                }
            }) == Some(id);
            if id_is_inner {
                id = parent;
                if parent_is_right {
                    rotate_right(tree, id);
                } else {
                    rotate_left(tree, id);
                }
            }
            let parent = match node_ref(tree, id).and_then(|node| node.parent) {
                Some(parent) => parent,
                None => break,
            };
            let grand = match node_ref(tree, parent).and_then(|node| node.parent) {
                Some(grand) => grand,
                None => break,
            };
            paint(tree, Some(parent), false);
            paint(tree, Some(grand), true);
            if parent_is_right {
                rotate_left(tree, grand);
            } else {
                rotate_right(tree, grand);
            }
        }
    }
    paint(tree, tree.root, false);
}

/// Allocate one unlinked node (`_Buynode` mechanics): reuse the first vacant
/// slot, else grow. The parent word is stored without linking the parent's
/// child pointer; linking is `_Insert`'s job.
fn buy_node(tree: &mut SyntheticTree, parent: Option<u32>, red: bool) -> Result<u32, StlError> {
    if let Some(parent) = parent {
        if node_ref(tree, parent).is_none() {
            return Err(StlError::UnknownNode);
        }
    }
    if let Some(vacant) = tree.nodes.iter().position(Option::is_none) {
        let id = u32::try_from(vacant).map_err(|_| StlError::HostAllocation)?;
        tree.nodes[vacant] = Some(SyntheticTreeNode {
            parent,
            left: None,
            right: None,
            red,
            sort: 0,
        });
        return Ok(id);
    }
    if tree.nodes.len() >= u32::MAX as usize {
        return Err(StlError::HostAllocation);
    }
    tree.nodes
        .try_reserve(1)
        .map_err(|_| StlError::HostAllocation)?;
    tree.nodes.push(Some(SyntheticTreeNode {
        parent,
        left: None,
        right: None,
        red,
        sort: 0,
    }));
    u32::try_from(tree.nodes.len() - 1).map_err(|_| StlError::HostAllocation)
}

/// Native `_Min` mechanics: leftmost descendant. A vacant start or a
/// dangling left link stops the walk (host admission).
fn leftmost(tree: &SyntheticTree, mut id: u32) -> Option<u32> {
    node_ref(tree, id)?;
    for _ in 0..tree.nodes.len().saturating_add(1) {
        let left = match node_ref(tree, id).and_then(|node| node.left) {
            Some(left) => left,
            None => break,
        };
        if node_ref(tree, left).is_none() {
            break;
        }
        id = left;
    }
    Some(id)
}

/// Native `_Inc` mechanics on links: leftmost of the right subtree, else the
/// first ancestor reached from its left subtree. Bounded like the other
/// walks; dangling links stop with `None` (host admission).
fn successor(tree: &SyntheticTree, id: u32) -> Option<u32> {
    let node = node_ref(tree, id)?;
    if let Some(right) = node.right {
        if node_ref(tree, right).is_some() {
            return leftmost(tree, right);
        }
        return None;
    }
    let mut child = id;
    let mut parent = node.parent;
    for _ in 0..tree.nodes.len().saturating_add(1) {
        let up = parent?;
        let up_node = node_ref(tree, up)?;
        if up_node.left == Some(child) {
            return Some(up);
        }
        child = up;
        parent = up_node.parent;
    }
    None
}

/// Native `_Erase(_Nodeptr)` mechanics: destroy the whole subtree. The entry
/// is detached first, then every slot whose ancestor chain still reaches it
/// is vacated with the entry itself cleared last (so descendant detection
/// keeps working during the sweep); no work stack, bounded walks only.
fn erase_subtree(tree: &mut SyntheticTree, id: u32) {
    if node_ref(tree, id).is_none() {
        return;
    }
    let parent = node_ref(tree, id).and_then(|node| node.parent);
    if let Some(parent) = parent {
        if let Some(slot) = tree.nodes.get_mut(parent as usize).and_then(Option::as_mut) {
            if slot.left == Some(id) {
                slot.left = None;
            } else if slot.right == Some(id) {
                slot.right = None;
            }
        }
    } else if tree.root == Some(id) {
        tree.root = None;
    }
    // Snapshot the parent words first: vacating in index order would
    // break live ancestor chains mid-sweep and strand descendants (native
    // destroys the whole subtree). Membership is decided from the snapshot,
    // then the set is cleared.
    let width = tree.nodes.len();
    let entry = id as usize;
    let parents: Vec<Option<u32>> = (0..width)
        .map(|i| {
            if i == entry {
                None
            } else {
                node_ref(tree, i as u32).and_then(|node| node.parent)
            }
        })
        .collect();
    let mut doomed = vec![false; width];
    doomed[entry] = true;
    for index in 0..width {
        if index == entry {
            continue;
        }
        let mut cursor = parents[index];
        for _ in 0..width.saturating_add(1) {
            match cursor {
                Some(at) if at as usize == entry => {
                    doomed[index] = true;
                    break;
                }
                Some(at) => {
                    cursor = parents.get(at as usize).copied().flatten();
                }
                None => break,
            }
        }
    }
    for (index, dead) in doomed.iter().enumerate() {
        if *dead {
            tree.nodes[index] = None;
        }
    }
    if tree.root == Some(id) {
        tree.root = None;
    }
}

/// Shared `_Insert(_Parent, _Left-side, _Value-sort)` link step used by both
/// tree instantiations: validate the slot, buy a red node, link it, rebalance.
fn insert_node_impl(
    tree: &mut SyntheticTree,
    parent: Option<u32>,
    left_side: bool,
    sort: u64,
) -> Result<u32, StlError> {
    if let Some(parent) = parent {
        let slot = node_ref(tree, parent).ok_or(StlError::UnknownNode)?;
        let occupied = if left_side { slot.left } else { slot.right };
        if occupied.is_some() {
            return Err(StlError::SlotOccupied);
        }
        if tree.root.is_none() {
            return Err(StlError::UnknownNode);
        }
    } else if tree.root.is_some() {
        return Err(StlError::SlotOccupied);
    }
    let id = buy_node(tree, parent, true)?;
    if let Some(slot) = tree.nodes.get_mut(id as usize).and_then(Option::as_mut) {
        slot.sort = sort;
    }
    if let Some(parent) = parent {
        if let Some(slot) = tree.nodes.get_mut(parent as usize).and_then(Option::as_mut) {
            if left_side {
                slot.left = Some(id);
            } else {
                slot.right = Some(id);
            }
        }
    } else {
        tree.root = Some(id);
    }
    insert_fixup(tree, id);
    Ok(id)
}

/// LS3DF RVA 0x5ab00, VA 0x1005ab00. VC6 `multiset<S_IB_block *>` dtor
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:184-186`
/// TEMPLATE, NOT game logic). Releases every entry and resets the sequence,
/// covering the native `_Erase(root)` sweep plus head-node release; native
/// `operator delete` has no host spelling. Tier contract.
pub fn ib_block_multiset_dtor(set: &mut IbBlockMultiset) {
    set.entries.clear();
    set.next_seq = 0;
}

/// LS3DF RVA 0x5b4d0, VA 0x1005b4d0. VC6 `multiset<C_IBuffer *>` dtor working
/// name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:188-190` TEMPLATE,
/// NOT game logic). Same teardown shape as `ib_block_multiset_dtor` for the
/// buffer-keyed instantiation. Tier contract.
pub fn ibuffer_multiset_dtor(set: &mut IBufferMultiset) {
    set.entries.clear();
    set.next_seq = 0;
}

/// LS3DF RVA 0x5bb20, VA 0x1005bb20. VC6 `C_list<S_IB_block *>::reserve`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:192`
/// TEMPLATE, NOT game logic). Growth mirrors the `reserve_draw_vertices`
/// precedent: `want = max(len, count)`, early return when capacity covers
/// it, else `try_reserve` the difference preserving elements; the native
/// uninitialized tail stays with the caller. Tier contract.
pub fn ib_block_list_reserve(list: &mut Vec<IbBlockEntry>, count: usize) -> Result<(), StlError> {
    let want = count.max(list.len());
    if list.capacity() >= want {
        return Ok(());
    }
    list.try_reserve(want - list.len())
        .map_err(|_| StlError::HostAllocation)
}

/// LS3DF RVA 0x5bb80, VA 0x1005bb80. VC6 `_Tree<S_VB_block *>::begin` working
/// name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:195` TEMPLATE, NOT
/// game logic; VBManager-folded copy). Smallest sort key, or `None` for the
/// empty tree (the native nil head). Tier contract.
pub fn vb_block_tree_begin(blocks: &VbBlockMultiset) -> Option<VbBlockSortKey> {
    blocks.entries.keys().next().copied()
}

/// LS3DF RVA 0x5bb90, VA 0x1005bb90. VC6 `_Tree<C_IBuffer *>::erase(iterator)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:201`
/// TEMPLATE, NOT game logic). Removes one entry and returns its successor
/// (`end()` as `None`); a missing key removes nothing and reports the
/// upper bound (native position there is undefined — host admission). Tier
/// contract.
pub fn ibuffer_tree_erase_one(
    set: &mut IBufferMultiset,
    pos: IBufferSortKey,
) -> Option<IBufferSortKey> {
    let next = set
        .entries
        .range((std::ops::Bound::Excluded(pos), std::ops::Bound::Unbounded))
        .next()
        .map(|(key, _)| *key);
    set.entries.remove(&pos);
    next
}

/// LS3DF RVA 0x5bfa0, VA 0x1005bfa0. VC6 `_Tree<C_IBuffer *>::erase(first,
/// last)` working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:205`
/// TEMPLATE, NOT game logic). Removes the half-open key range via paired
/// splits (the native per-node `_Erase` loop); an empty or inverted range
/// removes nothing. Tier contract.
pub fn ibuffer_tree_erase_range(
    set: &mut IBufferMultiset,
    first: IBufferSortKey,
    last: IBufferSortKey,
) {
    let mut tail = set.entries.split_off(&first);
    let mut keep = tail.split_off(&last);
    set.entries.append(&mut keep);
}

/// LS3DF RVA 0x5c070, VA 0x1005c070. VC6
/// `_Tree<C_IBuffer *>::const_iterator::_Inc` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:209` TEMPLATE, NOT game logic). Successor
/// key, or `None` past the end; a missing key reports the upper bound
/// (native iterator there is undefined — host admission). Tier contract.
pub fn ibuffer_tree_iter_inc(set: &IBufferMultiset, pos: IBufferSortKey) -> Option<IBufferSortKey> {
    set.entries
        .range((std::ops::Bound::Excluded(pos), std::ops::Bound::Unbounded))
        .next()
        .map(|(key, _)| *key)
}

/// LS3DF RVA 0x5c0c0, VA 0x1005c0c0. VC6 `_Tree<S_IB_block *>` constructor
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:213`
/// TEMPLATE, NOT game logic). Fresh empty membership; the native nil-head
/// allocation has no host spelling. Tier contract.
pub fn ib_block_tree_ctor() -> IbBlockMultiset {
    IbBlockMultiset::default()
}

/// LS3DF RVA 0x5c0f0, VA 0x1005c0f0. VC6 `_Tree<S_IB_block *>::insert(value)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:216`
/// TEMPLATE, NOT game logic). A multiset always inserts: returns the stable
/// sequence key and `true` (the native `pair<iterator, bool>` with
/// second always true). Tier contract.
pub fn ib_block_tree_insert(
    set: &mut IbBlockMultiset,
    size: u32,
    index: u32,
) -> (IbBlockSortKey, bool) {
    let key = IbBlockSortKey {
        size,
        seq: set.next_seq,
    };
    set.next_seq = set.next_seq.wrapping_add(1);
    set.entries.insert(key, IbBlockEntry { index, size });
    (key, true)
}

/// LS3DF RVA 0x5c330, VA 0x1005c330. VC6 `_Tree<S_IB_block *>::erase(iterator)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:219`
/// TEMPLATE, NOT game logic). Same single-erase shape as
/// `ibuffer_tree_erase_one` for the block-keyed instantiation. Tier contract.
pub fn ib_block_tree_erase_one(
    set: &mut IbBlockMultiset,
    pos: IbBlockSortKey,
) -> Option<IbBlockSortKey> {
    let next = set
        .entries
        .range((std::ops::Bound::Excluded(pos), std::ops::Bound::Unbounded))
        .next()
        .map(|(key, _)| *key);
    set.entries.remove(&pos);
    next
}

/// LS3DF RVA 0x5c760, VA 0x1005c760. VC6
/// `_Tree<S_IB_block *>::const_iterator::_Inc` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:223` TEMPLATE, NOT game logic). Same
/// successor shape as `ibuffer_tree_iter_inc` for the block-keyed
/// instantiation. Tier contract.
pub fn ib_block_tree_iter_inc(
    set: &IbBlockMultiset,
    pos: IbBlockSortKey,
) -> Option<IbBlockSortKey> {
    set.entries
        .range((std::ops::Bound::Excluded(pos), std::ops::Bound::Unbounded))
        .next()
        .map(|(key, _)| *key)
}

/// LS3DF RVA 0x5c7b0, VA 0x1005c7b0. VC6 `_Tree<C_IBuffer *>::_Erase(node)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:227`
/// TEMPLATE, NOT game logic). Recursive subtree destruction used by the
/// dtor/clear paths (not the rebalancing single erase). Tier contract.
pub fn ibuffer_tree_erase_node(tree: &mut SyntheticTree, node: u32) {
    erase_subtree(tree, node);
}

/// LS3DF RVA 0x5c7f0, VA 0x1005c7f0. VC6 `_Tree<C_IBuffer *>::_Insert(node,
/// parent, value)` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:231` TEMPLATE, NOT game logic). Links a red
/// node at the hinted slot and rebalances. Tier contract.
pub fn ibuffer_tree_insert_node(
    tree: &mut SyntheticTree,
    parent: Option<u32>,
    left_side: bool,
    sort: u64,
) -> Result<u32, StlError> {
    insert_node_impl(tree, parent, left_side, sort)
}

/// LS3DF RVA 0x5ca60, VA 0x1005ca60. VC6 `_Tree<C_IBuffer *>::_Min(node)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:235`
/// TEMPLATE, NOT game logic). Leftmost descendant. Tier contract.
pub fn ibuffer_tree_min(tree: &SyntheticTree, node: u32) -> Option<u32> {
    leftmost(tree, node)
}

/// LS3DF RVA 0x5ca80, VA 0x1005ca80. VC6 `_Tree<S_IB_block *>::_Erase(node)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:239`
/// TEMPLATE, NOT game logic). Same subtree-destruction shape as
/// `ibuffer_tree_erase_node` for the block-keyed instantiation. Tier contract.
pub fn ib_block_tree_erase_node(tree: &mut SyntheticTree, node: u32) {
    erase_subtree(tree, node);
}

/// LS3DF RVA 0x5cac0, VA 0x1005cac0. VC6 `_Tree<S_IB_block *>::_Init` working
/// name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:243` TEMPLATE, NOT
/// game logic). Fresh empty links; the native head-node allocation has no
/// host spelling. Tier contract.
pub fn ib_block_tree_init(tree: &mut SyntheticTree) {
    tree.nodes = Vec::new();
    tree.root = None;
}

/// LS3DF RVA 0x5cb50, VA 0x1005cb50. VC6 `_Tree<S_IB_block *>::_Insert(node,
/// parent, value)` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:247` TEMPLATE, NOT game logic). Same
/// link-and-rebalance shape as `ibuffer_tree_insert_node` for the
/// block-keyed instantiation. Tier contract.
pub fn ib_block_tree_insert_node(
    tree: &mut SyntheticTree,
    parent: Option<u32>,
    left_side: bool,
    sort: u64,
) -> Result<u32, StlError> {
    insert_node_impl(tree, parent, left_side, sort)
}

/// LS3DF RVA 0x5cdc0, VA 0x1005cdc0. VC6 `_Tree<S_IB_block *>::_Lrotate`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:251`
/// TEMPLATE, NOT game logic). Promotes the right child without comparing
/// payloads. Tier contract.
pub fn ib_block_tree_lrotate(tree: &mut SyntheticTree, node: u32) {
    rotate_left(tree, node);
}

/// LS3DF RVA 0x5ce20, VA 0x1005ce20. VC6 `_Tree<S_IB_block *>::_Min(node)`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:254`
/// TEMPLATE, NOT game logic). Same leftmost shape as `ibuffer_tree_min` for
/// the block-keyed instantiation. Tier contract.
pub fn ib_block_tree_min(tree: &SyntheticTree, node: u32) -> Option<u32> {
    leftmost(tree, node)
}

/// LS3DF RVA 0x5ce40, VA 0x1005ce40. VC6 `_Tree<S_IB_block *>::_Rrotate`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:258`
/// TEMPLATE, NOT game logic). Promotes the left child without comparing
/// payloads. Tier contract.
pub fn ib_block_tree_rrotate(tree: &mut SyntheticTree, node: u32) {
    rotate_right(tree, node);
}

/// LS3DF RVA 0x5cea0, VA 0x1005cea0. VC6 `_Tree<S_IB_block *>::iterator`
/// post-increment working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:261` TEMPLATE, NOT game logic). Returns the
/// old cursor and advances it along the links (`end()` stays `end()`; an
/// invalid cursor stops at `end()` — host admissions, native is undefined
/// there). Tier contract.
pub fn ib_block_tree_iter_next(tree: &SyntheticTree, cursor: &mut Option<u32>) -> Option<u32> {
    let current = (*cursor)?;
    *cursor = successor(tree, current);
    Some(current)
}

/// LS3DF RVA 0x5cf00, VA 0x1005cf00. VC6 `pair<iterator, bool>` constructor
/// working name for the IBuffer insert result (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:269` TEMPLATE, NOT game logic). Tier
/// contract.
pub fn ibuffer_insert_result_ctor(
    pos: Option<IBufferSortKey>,
    inserted: bool,
) -> IBufferInsertResult {
    IBufferInsertResult { pos, inserted }
}

/// LS3DF RVA 0x5cf20, VA 0x1005cf20. VC6 `_Tree<C_VBuffer *>::_Buynode`
/// working name (mafia-re ab67b13 `ls3df/I3D/render/IBManager.h:275`
/// TEMPLATE, NOT game logic; VBManager-folded copy). Allocates one node with
/// the requested color and the hinted parent word; the value word stays the
/// host default until `_Construct`/`_Insert` fills it. Tier contract.
pub fn vbuffer_tree_buy_node(
    tree: &mut SyntheticTree,
    parent: Option<u32>,
    red: bool,
) -> Result<u32, StlError> {
    buy_node(tree, parent, red)
}

/// LS3DF RVA 0x5cf40, VA 0x1005cf40. VC6
/// `_Tree<C_IBuffer *>::const_iterator::_Dec` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:278` TEMPLATE, NOT game logic). Predecessor
/// key; `None` (the `end()` iterator) reports the maximum, and the minimum
/// decrements to `None`. Tier contract.
pub fn ibuffer_tree_iter_dec(
    set: &IBufferMultiset,
    cursor: Option<IBufferSortKey>,
) -> Option<IBufferSortKey> {
    match cursor {
        None => set.entries.last_key_value().map(|(key, _)| *key),
        Some(pos) => set
            .entries
            .range((std::ops::Bound::Unbounded, std::ops::Bound::Excluded(pos)))
            .next_back()
            .map(|(key, _)| *key),
    }
}

/// LS3DF RVA 0x5cfa0, VA 0x1005cfa0. VC6
/// `_Tree<S_IB_block *>::const_iterator::_Dec` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:282` TEMPLATE, NOT game logic). Same
/// predecessor shape as `ibuffer_tree_iter_dec` for the block-keyed
/// instantiation. Tier contract.
pub fn ib_block_tree_iter_dec(
    set: &IbBlockMultiset,
    cursor: Option<IbBlockSortKey>,
) -> Option<IbBlockSortKey> {
    match cursor {
        None => set.entries.last_key_value().map(|(key, _)| *key),
        Some(pos) => set
            .entries
            .range((std::ops::Bound::Unbounded, std::ops::Bound::Excluded(pos)))
            .next_back()
            .map(|(key, _)| *key),
    }
}

/// LS3DF RVA 0x5d000, VA 0x1005d000. VC6 `std::_Construct(S_VB_block **,
/// const S_VB_block *&)` working name (mafia-re ab67b13
/// `ls3df/I3D/render/IBManager.h:286` TEMPLATE, NOT game logic;
/// VBManager-folded copy). Placement copy into the slot. The native word is
/// an owner pointer; the host slot carries index/size with no owner address
/// (disclosed projection — the drop side is documented on the twin).
/// Tier contract.
pub fn construct_vb_block_at(slot: &mut Option<VbBlockEntry>, value: VbBlockEntry) {
    *slot = Some(value);
}

#[cfg(test)]
mod stl_tests {
    use super::*;

    fn block_sizes(set: &IbBlockMultiset) -> Vec<u32> {
        let mut out = Vec::new();
        let mut cursor = set.entries.keys().next().copied();
        while let Some(pos) = cursor {
            out.push(pos.size);
            cursor = ib_block_tree_iter_inc(set, pos);
        }
        out
    }

    fn inorder_sorts(tree: &SyntheticTree) -> Vec<u64> {
        let mut out = Vec::new();
        let mut cursor = tree.root.and_then(|root| leftmost(tree, root));
        while let Some(id) = cursor {
            out.push(tree.nodes[id as usize].as_ref().unwrap().sort);
            cursor = successor(tree, id);
        }
        out
    }

    fn black_height(tree: &SyntheticTree, id: Option<u32>) -> Option<i32> {
        match id {
            None => Some(1),
            Some(id) => {
                let node = tree.nodes.get(id as usize)?.as_ref()?;
                if node.red {
                    for child in [node.left, node.right].into_iter().flatten() {
                        if tree.nodes.get(child as usize)?.as_ref()?.red {
                            return None;
                        }
                    }
                }
                let left = black_height(tree, node.left)?;
                let right = black_height(tree, node.right)?;
                if left != right {
                    return None;
                }
                Some(left + if node.red { 0 } else { 1 })
            }
        }
    }

    fn assert_rb_valid(tree: &SyntheticTree) {
        match tree.root {
            None => assert!(tree.nodes.iter().all(Option::is_none)),
            Some(root) => {
                let node = tree.nodes[root as usize].as_ref().unwrap();
                assert!(!node.red, "root stays black");
                assert!(
                    black_height(tree, Some(root)).is_some(),
                    "no red-red links and uniform black height"
                );
            }
        }
    }

    fn live_ids(tree: &SyntheticTree) -> Vec<u32> {
        tree.nodes
            .iter()
            .enumerate()
            .filter_map(|(index, slot)| slot.as_ref().map(|_| index as u32))
            .collect()
    }

    #[test]
    fn block_ctor_starts_empty() {
        let set = ib_block_tree_ctor();
        assert!(set.is_empty());
        assert_eq!(set.len(), 0);
    }

    #[test]
    fn block_insert_orders_by_size_with_stable_duplicates() {
        let mut set = ib_block_tree_ctor();
        for (size, index) in [(30u32, 0u32), (10, 1), (20, 2), (10, 3)] {
            let (_, inserted) = ib_block_tree_insert(&mut set, size, index);
            assert!(inserted, "multisets always insert");
        }
        assert_eq!(block_sizes(&set), vec![10, 10, 20, 30]);
        // Duplicate sizes keep insertion order through the sequence key.
        let dupes: Vec<u32> = set
            .entries
            .iter()
            .filter(|(key, _)| key.size == 10)
            .map(|(_, entry)| entry.index)
            .collect();
        assert_eq!(dupes, vec![1, 3]);
    }

    #[test]
    fn block_erase_one_returns_successor() {
        let mut set = ib_block_tree_ctor();
        let (first, _) = ib_block_tree_insert(&mut set, 10, 0);
        let (second, _) = ib_block_tree_insert(&mut set, 20, 1);
        let (third, _) = ib_block_tree_insert(&mut set, 30, 2);
        assert_eq!(ib_block_tree_erase_one(&mut set, second), Some(third));
        assert_eq!(block_sizes(&set), vec![10, 30]);
        assert_eq!(ib_block_tree_erase_one(&mut set, third), None);
        assert_eq!(block_sizes(&set), vec![10]);
        // A missing key removes nothing and reports the upper bound (none
        // here: 20 sorts past the surviving 10).
        assert_eq!(ib_block_tree_erase_one(&mut set, second), None);
        assert_eq!(block_sizes(&set), vec![10]);
        assert_eq!(ib_block_tree_iter_inc(&set, first), None);
    }

    #[test]
    fn buffer_erase_range_clears_half_open_interval() {
        let mut set = IBufferMultiset::default();
        let mut keys = Vec::new();
        for num_free in [5u32, 15, 25, 35] {
            let (key, inserted) = ibuffer_multiset_insert(&mut set, num_free, num_free);
            assert!(inserted);
            keys.push(key);
        }
        ibuffer_tree_erase_range(&mut set, keys[1], keys[3]);
        let rest: Vec<u32> = set.entries.keys().map(|key| key.num_free).collect();
        assert_eq!(rest, vec![5, 35]);
        // Empty and inverted ranges remove nothing.
        ibuffer_tree_erase_range(&mut set, keys[0], keys[0]);
        ibuffer_tree_erase_range(&mut set, keys[3], keys[0]);
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn buffer_iter_inc_dec_walk_both_directions() {
        let mut set = IBufferMultiset::default();
        let mut keys = Vec::new();
        for num_free in [5u32, 15, 25] {
            let (key, inserted) = ibuffer_multiset_insert(&mut set, num_free, num_free);
            assert!(inserted);
            keys.push(key);
        }
        assert_eq!(ibuffer_tree_iter_inc(&set, keys[0]), Some(keys[1]));
        assert_eq!(ibuffer_tree_iter_inc(&set, keys[2]), None);
        // Decrement from end() reaches the maximum, then walks down.
        assert_eq!(ibuffer_tree_iter_dec(&set, None), Some(keys[2]));
        assert_eq!(ibuffer_tree_iter_dec(&set, Some(keys[2])), Some(keys[1]));
        assert_eq!(ibuffer_tree_iter_dec(&set, Some(keys[0])), None);
        // Single erase returns the successor like the block twin.
        assert_eq!(ibuffer_tree_erase_one(&mut set, keys[1]), Some(keys[2]));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn block_iter_dec_from_end_reaches_maximum() {
        let mut set = ib_block_tree_ctor();
        let (first, _) = ib_block_tree_insert(&mut set, 10, 0);
        let (second, _) = ib_block_tree_insert(&mut set, 20, 1);
        assert_eq!(ib_block_tree_iter_dec(&set, None), Some(second));
        assert_eq!(ib_block_tree_iter_dec(&set, Some(second)), Some(first));
        assert_eq!(ib_block_tree_iter_dec(&set, Some(first)), None);
    }

    #[test]
    fn multiset_dtors_release_every_entry() {
        let mut blocks = ib_block_tree_ctor();
        ib_block_tree_insert(&mut blocks, 10, 0);
        ib_block_tree_insert(&mut blocks, 20, 1);
        ib_block_multiset_dtor(&mut blocks);
        assert!(blocks.is_empty());
        let mut buffers = IBufferMultiset::default();
        buffers.entries.insert(
            IBufferSortKey {
                num_free: 7,
                seq: 0,
            },
            IBufferEntry {
                buffer: 1,
                num_free: 7,
            },
        );
        ibuffer_multiset_dtor(&mut buffers);
        assert!(buffers.is_empty());
    }

    #[test]
    fn list_reserve_grows_capacity_without_changing_length() {
        let mut list = vec![IbBlockEntry { index: 0, size: 8 }];
        ib_block_list_reserve(&mut list, 64).unwrap();
        assert_eq!(list.len(), 1);
        assert!(list.capacity() >= 64);
        assert_eq!(list[0], IbBlockEntry { index: 0, size: 8 });
        ib_block_list_reserve(&mut list, 0).unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn vb_begin_reports_smallest_block() {
        let mut blocks = VbBlockMultiset::default();
        assert_eq!(vb_block_tree_begin(&blocks), None);
        for (size, index) in [(30u32, 0u32), (10, 1), (20, 2)] {
            let (key, inserted) = vb_block_multiset_insert(&mut blocks, size, index);
            assert!(inserted);
            let _ = key;
        }
        assert_eq!(
            vb_block_tree_begin(&blocks),
            Some(VbBlockSortKey { size: 10, seq: 1 })
        );
    }

    #[test]
    fn insert_result_ctor_carries_position_and_flag() {
        let key = IBufferSortKey {
            num_free: 9,
            seq: 0,
        };
        let result = ibuffer_insert_result_ctor(Some(key), true);
        assert_eq!(result.pos, Some(key));
        assert!(result.inserted);
    }

    #[test]
    fn buynode_allocates_colored_node_without_linking() {
        let mut tree = SyntheticTree::default();
        let root = vbuffer_tree_buy_node(&mut tree, None, false).unwrap();
        assert_eq!(tree.root, None);
        let child = vbuffer_tree_buy_node(&mut tree, Some(root), true).unwrap();
        let root_node = tree.nodes[root as usize].as_ref().unwrap();
        assert!(!root_node.red);
        assert_eq!(root_node.parent, None);
        let child_node = tree.nodes[child as usize].as_ref().unwrap();
        assert!(child_node.red);
        assert_eq!(child_node.parent, Some(root));
        // The parent's child pointer stays empty until _Insert links it.
        assert_eq!((root_node.left, root_node.right), (None, None));
        assert!(vbuffer_tree_buy_node(&mut tree, Some(99), true).is_err());
    }

    #[test]
    fn init_releases_all_nodes() {
        let mut tree = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut tree, None, true, 10).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(root), true, 5).unwrap();
        ib_block_tree_init(&mut tree);
        assert_eq!(tree.root, None);
        assert!(tree.nodes.iter().all(Option::is_none));
        assert_rb_valid(&tree);
    }

    #[test]
    fn lrotate_promotes_right_child_with_middle_subtree() {
        // Hand-built: 10 -> right 20 -> left 15. Left rotation at 10 must
        // yield root 20 with left 10 carrying 15 on its right.
        let mut tree = SyntheticTree::default();
        tree.nodes.push(Some(SyntheticTreeNode {
            parent: None,
            left: None,
            right: Some(1),
            red: false,
            sort: 10,
        }));
        tree.nodes.push(Some(SyntheticTreeNode {
            parent: Some(0),
            left: Some(2),
            right: None,
            red: false,
            sort: 20,
        }));
        tree.nodes.push(Some(SyntheticTreeNode {
            parent: Some(1),
            left: None,
            right: None,
            red: false,
            sort: 15,
        }));
        tree.root = Some(0);
        ib_block_tree_lrotate(&mut tree, 0);
        assert_eq!(tree.root, Some(1));
        assert_eq!(tree.nodes[1].as_ref().unwrap().left, Some(0));
        assert_eq!(
            tree.nodes[0].as_ref().unwrap(),
            &SyntheticTreeNode {
                parent: Some(1),
                left: None,
                right: Some(2),
                red: false,
                sort: 10,
            }
        );
        assert_eq!(tree.nodes[2].as_ref().unwrap().parent, Some(0));
        assert_eq!(inorder_sorts(&tree), vec![10, 15, 20]);
    }

    #[test]
    fn rrotate_promotes_left_child_with_middle_subtree() {
        // Mirror: 20 -> left 10 -> right 15. Right rotation at 20 must yield
        // root 10 with right 20 carrying 15 on its left.
        let mut tree = SyntheticTree::default();
        tree.nodes.push(Some(SyntheticTreeNode {
            parent: None,
            left: Some(1),
            right: None,
            red: false,
            sort: 20,
        }));
        tree.nodes.push(Some(SyntheticTreeNode {
            parent: Some(0),
            left: None,
            right: Some(2),
            red: false,
            sort: 10,
        }));
        tree.nodes.push(Some(SyntheticTreeNode {
            parent: Some(1),
            left: None,
            right: None,
            red: false,
            sort: 15,
        }));
        tree.root = Some(0);
        ib_block_tree_rrotate(&mut tree, 0);
        assert_eq!(tree.root, Some(1));
        assert_eq!(tree.nodes[1].as_ref().unwrap().right, Some(0));
        assert_eq!(tree.nodes[0].as_ref().unwrap().left, Some(2));
        assert_eq!(tree.nodes[2].as_ref().unwrap().parent, Some(0));
        assert_eq!(inorder_sorts(&tree), vec![10, 15, 20]);
    }

    #[test]
    fn rotates_without_child_are_noops() {
        let mut tree = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut tree, None, true, 10).unwrap();
        let before = tree.clone();
        ib_block_tree_lrotate(&mut tree, root);
        ib_block_tree_rrotate(&mut tree, root);
        ib_block_tree_lrotate(&mut tree, 99);
        assert_eq!(tree, before);
    }

    #[test]
    fn min_reports_leftmost_and_rejects_vacant() {
        let mut tree = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut tree, None, false, 20).unwrap();
        let left = ib_block_tree_insert_node(&mut tree, Some(root), true, 10).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(left), true, 5).unwrap();
        // Rebalancing may move the root; both twins read the current one.
        let current = tree.root.unwrap();
        assert_eq!(
            ib_block_tree_min(&tree, current),
            ibuffer_tree_min(&tree, current)
        );
        assert_eq!(
            ib_block_tree_min(&tree, current)
                .map(|id| tree.nodes[id as usize].as_ref().unwrap().sort),
            Some(5)
        );
        assert_eq!(ib_block_tree_min(&tree, 99), None);
    }

    #[test]
    fn erase_node_clears_only_the_subtree() {
        let mut tree = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut tree, None, false, 20).unwrap();
        let left = ib_block_tree_insert_node(&mut tree, Some(root), true, 10).unwrap();
        let right = ib_block_tree_insert_node(&mut tree, Some(root), false, 30).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(left), true, 5).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(left), false, 15).unwrap();
        ibuffer_tree_erase_node(&mut tree, left);
        assert_eq!(tree.root, Some(root));
        assert_eq!(tree.nodes[root as usize].as_ref().unwrap().left, None);
        assert_eq!(
            tree.nodes[root as usize].as_ref().unwrap().right,
            Some(right)
        );
        assert_eq!(live_ids(&tree), vec![root, right]);
        // Invalid ids are no-ops.
        let before = tree.clone();
        ib_block_tree_erase_node(&mut tree, 99);
        ibuffer_tree_erase_node(&mut tree, left);
        assert_eq!(tree, before);
    }

    #[test]
    fn insert_nodes_keep_red_black_shape() {
        // Right-leaning run exercises the outer-rotation arm.
        let mut tree = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut tree, None, false, 10).unwrap();
        let right = ib_block_tree_insert_node(&mut tree, Some(root), false, 20).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(right), false, 30).unwrap();
        assert_rb_valid(&tree);
        assert_eq!(inorder_sorts(&tree), vec![10, 20, 30]);
        // Inner-rotation arm through the IBuffer twin.
        let mut other = SyntheticTree::default();
        let root = ibuffer_tree_insert_node(&mut other, None, false, 10).unwrap();
        let left = ibuffer_tree_insert_node(&mut other, Some(root), true, 5).unwrap();
        ibuffer_tree_insert_node(&mut other, Some(left), false, 7).unwrap();
        assert_rb_valid(&other);
        assert_eq!(inorder_sorts(&other), vec![5, 7, 10]);
        // Red-uncle recolor arm.
        let mut third = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut third, None, false, 10).unwrap();
        let left = ib_block_tree_insert_node(&mut third, Some(root), true, 5).unwrap();
        ib_block_tree_insert_node(&mut third, Some(root), false, 15).unwrap();
        ib_block_tree_insert_node(&mut third, Some(left), true, 1).unwrap();
        assert_rb_valid(&third);
        assert_eq!(inorder_sorts(&third), vec![1, 5, 10, 15]);
        let root_node = third.nodes[third.root.unwrap() as usize].as_ref().unwrap();
        assert!(!root_node.red);
        // Occupied and unknown slots are explicit errors, never corruption.
        assert_eq!(
            ib_block_tree_insert_node(&mut third, Some(root), true, 99),
            Err(StlError::SlotOccupied)
        );
        assert_eq!(
            ib_block_tree_insert_node(&mut third, Some(99), true, 99),
            Err(StlError::UnknownNode)
        );
        assert_eq!(
            ib_block_tree_insert_node(&mut third, None, true, 99),
            Err(StlError::SlotOccupied)
        );
    }

    #[test]
    fn iter_next_post_increment_walks_links_in_order() {
        let mut tree = SyntheticTree::default();
        let root = ib_block_tree_insert_node(&mut tree, None, false, 20).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(root), true, 10).unwrap();
        ib_block_tree_insert_node(&mut tree, Some(root), false, 30).unwrap();
        let mut cursor = ib_block_tree_min(&tree, root);
        let mut seen = Vec::new();
        while let Some(old) = ib_block_tree_iter_next(&tree, &mut cursor) {
            seen.push(tree.nodes[old as usize].as_ref().unwrap().sort);
        }
        assert_eq!(seen, vec![10, 20, 30]);
        assert_eq!(cursor, None);
        // Advancing end() stays at end().
        assert_eq!(ib_block_tree_iter_next(&tree, &mut cursor), None);
    }

    #[test]
    fn construct_vb_block_at_copies_pointer_word() {
        let mut slot: Option<VbBlockEntry> = None;
        construct_vb_block_at(&mut slot, VbBlockEntry { index: 4, size: 16 });
        assert_eq!(slot, Some(VbBlockEntry { index: 4, size: 16 }));
        construct_vb_block_at(&mut slot, VbBlockEntry { index: 8, size: 32 });
        assert_eq!(slot, Some(VbBlockEntry { index: 8, size: 32 }));
    }
}
