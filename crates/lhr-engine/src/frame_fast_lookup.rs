//! LS3DF fast frame tree, separate from the case-insensitive scene search.
//!
//! `FindFrameFast`, `FindFrameFast_`, `AddToFFTree`, `RemoveFromFFTree`,
//! `SetName` and `SetOn` correspond to PC DLL RVAs 0x19b10, 0x19c10,
//! 0x1b500, 0x1b430, 0x1b6f0 and 0x1b400. Original names also occur in
//! the PS2 symbols; the traversal here follows the PC instructions.
//!
//! The caller supplies actual frame/scene identities and existing storage.
//! Importing storage does not construct a scene, sector or model and does not
//! register its name. The identity index owns only this field projection;
//! name searches follow the original mutable tree links and ancestor links.
//! Allocation failure and malformed host storage stop at explicit boundaries.

use crate::frame_transform::FrameKey;
use std::{cmp::Ordering, collections::BTreeMap};

pub const FAST_FRAME_TREE_FLAG: u32 = 0x2000_0000;

/// Projection of the original-name pointer, including a released allocation
/// after a failed SetName allocation. No dangling Rust reference is retained.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OriginalFrameName {
    Null,
    Offset(usize),
    Released { offset: usize },
}

/// Caller-supplied frame fields, not an original constructor or resource owner.
/// Names have C byte-string semantics (the first zero terminates the name).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FastFrameStorage {
    pub frame: FrameKey,
    pub name: Option<Vec<u8>>,
    pub original_name: OriginalFrameName,
    pub frame_type: u32,
    pub parent: Option<FrameKey>,
    /// Native sector +0x214; read only for frame type 5. Null is a real value.
    pub sector_scene: Option<u32>,
    pub flags: u32,
    pub left: Option<FrameKey>,
    pub right: Option<FrameKey>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FastFrameError {
    MissingFrame(FrameKey),
    DuplicateStorage(FrameKey),
    /// AddToFFTree directly dereferences stored names when comparing them.
    NullInsertionName(FrameKey),
    Cycle,
    AllocationFailed,
    InvalidNameAllocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FastFrameRemoval {
    Removed,
    /// Native clears the registration bit, then returns without clearing edges.
    NullName,
    /// Native emits a diagnostic and clears this frame's two tree edges.
    NotFound,
}

/// Original global tree state. The map indexes opaque identities only; it is
/// never used to search, sort, normalize or disambiguate names.
#[derive(Clone, Debug)]
pub struct I3dFastFrameTree {
    frames: BTreeMap<u32, FastFrameStorage>,
    pub root: Option<FrameKey>,
    pub last_up: Option<FrameKey>,
    pub add_counter: u32,
    /// Projection of RS_FAST_FIND. Changing it alone does not rebuild the tree.
    pub fast_find_enabled: bool,
}

impl I3dFastFrameTree {
    pub fn new(fast_find_enabled: bool) -> Self {
        Self {
            frames: BTreeMap::new(),
            root: None,
            last_up: None,
            add_counter: 0,
            fast_find_enabled,
        }
    }

    /// Admit externally owned frame storage. No AddToFFTree call is implied.
    pub fn import_frame(&mut self, storage: FastFrameStorage) -> Result<(), FastFrameError> {
        if self.frames.contains_key(&storage.frame.0) {
            return Err(FastFrameError::DuplicateStorage(storage.frame));
        }
        self.frames.insert(storage.frame.0, storage);
        Ok(())
    }

    pub fn frame(&self, frame: FrameKey) -> Result<&FastFrameStorage, FastFrameError> {
        self.frames
            .get(&frame.0)
            .ok_or(FastFrameError::MissingFrame(frame))
    }

    /// Host synchronization boundary for original fields owned by other frame
    /// methods, including parent/sector ownership. This is not LinkTo.
    pub fn frame_storage_mut(
        &mut self,
        frame: FrameKey,
    ) -> Result<&mut FastFrameStorage, FastFrameError> {
        self.frames
            .get_mut(&frame.0)
            .ok_or(FastFrameError::MissingFrame(frame))
    }

    pub fn frames(&self) -> impl Iterator<Item = &FastFrameStorage> {
        self.frames.values()
    }

    fn step(&self, steps: &mut usize) -> Result<(), FastFrameError> {
        if *steps >= self.frames.len() {
            return Err(FastFrameError::Cycle);
        }
        *steps += 1;
        Ok(())
    }

    fn name(&self, frame: FrameKey) -> Result<&[u8], FastFrameError> {
        Ok(c_name(self.frame(frame)?.name.as_deref().unwrap_or(&[])))
    }

    fn insertion_name(&self, frame: FrameKey) -> Result<&[u8], FastFrameError> {
        self.frame(frame)?
            .name
            .as_deref()
            .map(c_name)
            .ok_or(FastFrameError::NullInsertionName(frame))
    }

    pub fn find_frame_fast(
        &self,
        scene: Option<u32>,
        name: &[u8],
    ) -> Result<Option<FrameKey>, FastFrameError> {
        let name = c_name(name);
        let mut current = self.root;
        let mut steps = 0;
        while let Some(frame) = current {
            self.step(&mut steps)?;
            match name.cmp(self.name(frame)?) {
                Ordering::Less => current = self.frame(frame)?.left,
                Ordering::Greater => current = self.frame(frame)?.right,
                Ordering::Equal => {
                    let mut candidate = Some(frame);
                    // Equal names are a contiguous chain of right edges.
                    while let Some(frame) = candidate {
                        let mut ancestor = Some(frame);
                        let mut ancestor_steps = 0;
                        while let Some(parent) = ancestor {
                            self.step(&mut ancestor_steps)?;
                            let storage = self.frame(parent)?;
                            if storage.frame_type == 5 && storage.sector_scene == scene {
                                return Ok(Some(frame));
                            }
                            // PC continues past a sector of a different scene.
                            ancestor = storage.parent;
                        }
                        candidate = self.frame(frame)?.right;
                        if let Some(next) = candidate {
                            self.step(&mut steps)?;
                            if name != self.name(next)? {
                                return Ok(None);
                            }
                        }
                    }
                    return Ok(None);
                }
            }
        }
        Ok(None)
    }

    /// Exact identity search, including the native pFFLastUp_ side effect on
    /// successful edges, exhausted duplicates and unequal-name termination.
    pub fn find_frame_fast_(
        &mut self,
        name: &[u8],
        wanted: FrameKey,
    ) -> Result<Option<FrameKey>, FastFrameError> {
        find_exact(&self.frames, self.root, name, wanted, &mut self.last_up)
    }

    pub fn add_to_ff_tree(&mut self, frame: FrameKey) -> Result<(), FastFrameError> {
        // The native counter increments before any tree/name dereference.
        self.add_counter = self.add_counter.wrapping_add(1);
        let mut current = self.root;
        let mut steps = 0;
        if current.is_none() {
            self.root = Some(frame);
        }
        while let Some(parent) = current {
            self.step(&mut steps)?;
            match self
                .insertion_name(frame)?
                .cmp(self.insertion_name(parent)?)
            {
                Ordering::Less => {
                    current = self.frame(parent)?.left;
                    if current.is_none() {
                        self.frame_storage_mut(parent)?.left = Some(frame);
                    }
                }
                Ordering::Greater => {
                    current = self.frame(parent)?.right;
                    if current.is_none() {
                        self.frame_storage_mut(parent)?.right = Some(frame);
                    }
                }
                Ordering::Equal => {
                    let mut last_equal = parent;
                    while let Some(next) = self.frame(last_equal)?.right {
                        self.step(&mut steps)?;
                        if self.insertion_name(frame)? != self.insertion_name(next)? {
                            break;
                        }
                        last_equal = next;
                    }
                    let old_right = self.frame(last_equal)?.right;
                    self.frame_storage_mut(frame)?.right = old_right;
                    self.frame_storage_mut(last_equal)?.right = Some(frame);
                    break;
                }
            }
        }
        self.frame_storage_mut(frame)?.flags |= FAST_FRAME_TREE_FLAG;
        Ok(())
    }

    pub fn remove_from_ff_tree(
        &mut self,
        frame: FrameKey,
    ) -> Result<FastFrameRemoval, FastFrameError> {
        self.frame_storage_mut(frame)?.flags &= !FAST_FRAME_TREE_FLAG;
        let Some(name) = self
            .frames
            .get(&frame.0)
            .ok_or(FastFrameError::MissingFrame(frame))?
            .name
            .as_deref()
        else {
            return Ok(FastFrameRemoval::NullName);
        };
        let found =
            find_exact(&self.frames, self.root, name, frame, &mut self.last_up)? == Some(frame);
        if found {
            let (left, right) = {
                let storage = self.frame(frame)?;
                (storage.left, storage.right)
            };
            let replacement = right.or(left);
            if self.root == Some(frame) {
                self.root = replacement;
            } else {
                let parent = self.last_up.ok_or(FastFrameError::Cycle)?;
                let parent = self.frame_storage_mut(parent)?;
                if parent.left == Some(frame) {
                    parent.left = replacement;
                } else {
                    parent.right = replacement;
                }
            }
            if let (Some(mut right), Some(left)) = (right, left) {
                let mut steps = 0;
                loop {
                    self.step(&mut steps)?;
                    match self.frame(right)?.left {
                        Some(next) => right = next,
                        None => break,
                    }
                }
                self.frame_storage_mut(right)?.left = Some(left);
            }
        }
        let storage = self.frame_storage_mut(frame)?;
        storage.left = None;
        storage.right = None;
        Ok(if found {
            FastFrameRemoval::Removed
        } else {
            FastFrameRemoval::NotFound
        })
    }

    /// Successful SetName path, composed with removal and registration. Name
    /// allocation uses host fallible storage. On allocation failure the old
    /// name is released and orig_name remains an explicitly released identity;
    /// the native diagnostic/ErrorHandler call is outside this host boundary.
    pub fn set_name(
        &mut self,
        frame: FrameKey,
        new_name: Option<&[u8]>,
    ) -> Result<(), FastFrameError> {
        self.set_name_with_allocator(frame, new_name, allocate_name)
    }

    /// The actual SetName allocation boundary, reached only after the previous
    /// name was released and registered storage was removed. The allocator
    /// returns exactly `byte_count` writable bytes, including the terminator.
    /// Returning AllocationFailed exposes the native ErrorHandler boundary;
    /// no heap allocator or fatal handler implementation is implied.
    pub fn set_name_with_allocator(
        &mut self,
        frame: FrameKey,
        new_name: Option<&[u8]>,
        allocate: impl FnOnce(usize) -> Result<Vec<u8>, FastFrameError>,
    ) -> Result<(), FastFrameError> {
        self.set_name_with_allocator_state(frame, new_name, |_, byte_count| allocate(byte_count))
    }

    /// Read-only observation of the allocation boundary. Released name memory
    /// has no Rust slice; `name == None` and `original_name == Released { offset }` expose
    /// that release without dereferencing the original dangling pointer. The
    /// native name word is overwritten by the allocator result upon return.
    pub fn set_name_with_allocator_state(
        &mut self,
        frame: FrameKey,
        new_name: Option<&[u8]>,
        allocate: impl FnOnce(&Self, usize) -> Result<Vec<u8>, FastFrameError>,
    ) -> Result<(), FastFrameError> {
        let new_name = new_name.map(c_name);
        let storage = self.frame(frame)?;
        if storage.name.as_deref().map(c_name) == new_name {
            return Ok(());
        }
        if storage.flags & FAST_FRAME_TREE_FLAG != 0 {
            self.remove_from_ff_tree(frame)?;
        }
        let storage = self.frame_storage_mut(frame)?;
        if storage.name.take().is_some() {
            storage.original_name = match storage.original_name {
                OriginalFrameName::Offset(offset) | OriginalFrameName::Released { offset } => {
                    OriginalFrameName::Released { offset }
                }
                OriginalFrameName::Null => OriginalFrameName::Null,
            };
        }
        let Some(name) = new_name else {
            storage.original_name = OriginalFrameName::Null;
            return Ok(());
        };
        let byte_count = name
            .len()
            .checked_add(1)
            .ok_or(FastFrameError::AllocationFailed)?;
        let mut allocation = allocate(self, byte_count)?;
        if allocation.len() != byte_count {
            return Err(FastFrameError::InvalidNameAllocation);
        }
        allocation[..name.len()].copy_from_slice(name);
        allocation[name.len()] = 0;
        let storage = self.frame_storage_mut(frame)?;
        storage.name = Some(allocation);
        storage.original_name = OriginalFrameName::Offset(0);
        if self.fast_find_enabled {
            self.add_to_ff_tree(frame)?;
        }
        Ok(())
    }

    pub fn set_on(&mut self, frame: FrameKey, on: bool) -> Result<(), FastFrameError> {
        let flags = &mut self.frame_storage_mut(frame)?.flags;
        *flags = if on { *flags | 1 } else { *flags & !1 };
        Ok(())
    }
}

fn c_name(name: &[u8]) -> &[u8] {
    &name[..name
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name.len())]
}

fn allocate_name(byte_count: usize) -> Result<Vec<u8>, FastFrameError> {
    let mut allocation = Vec::new();
    allocation
        .try_reserve_exact(byte_count)
        .map_err(|_| FastFrameError::AllocationFailed)?;
    allocation.resize(byte_count, 0);
    Ok(allocation)
}

fn find_exact(
    frames: &BTreeMap<u32, FastFrameStorage>,
    root: Option<FrameKey>,
    name: &[u8],
    wanted: FrameKey,
    last_up: &mut Option<FrameKey>,
) -> Result<Option<FrameKey>, FastFrameError> {
    *last_up = None;
    let name = c_name(name);
    let get = |frame: FrameKey| {
        frames
            .get(&frame.0)
            .ok_or(FastFrameError::MissingFrame(frame))
    };
    let stored_name = |frame| Ok(c_name(get(frame)?.name.as_deref().unwrap_or(&[])));
    let mut current = root;
    let mut steps = 0;
    while let Some(frame) = current {
        if steps >= frames.len() {
            return Err(FastFrameError::Cycle);
        }
        steps += 1;
        match name.cmp(stored_name(frame)?) {
            Ordering::Less | Ordering::Greater => {
                let next = if name < stored_name(frame)? {
                    get(frame)?.left
                } else {
                    get(frame)?.right
                };
                if next.is_none() {
                    return Ok(None);
                }
                *last_up = Some(frame);
                current = next;
            }
            Ordering::Equal => {
                let mut candidate = frame;
                loop {
                    if candidate == wanted {
                        return Ok(Some(candidate));
                    }
                    let Some(next) = get(candidate)?.right else {
                        *last_up = None;
                        return Ok(None);
                    };
                    *last_up = Some(candidate);
                    if steps >= frames.len() {
                        return Err(FastFrameError::Cycle);
                    }
                    steps += 1;
                    candidate = next;
                    if name != stored_name(candidate)? {
                        return Ok(None);
                    }
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(id: u32, name: Option<&[u8]>, parent: Option<u32>) -> FastFrameStorage {
        FastFrameStorage {
            frame: FrameKey(id),
            name: name.map(<[u8]>::to_vec),
            original_name: if name.is_some() {
                OriginalFrameName::Offset(0)
            } else {
                OriginalFrameName::Null
            },
            frame_type: 1,
            parent: parent.map(FrameKey),
            sector_scene: None,
            flags: 0x4000_003c,
            left: None,
            right: None,
        }
    }

    fn tree() -> I3dFastFrameTree {
        let mut tree = I3dFastFrameTree::new(true);
        let mut sector = frame(100, Some(b"sector"), None);
        sector.frame_type = 5;
        sector.sector_scene = Some(7);
        tree.import_frame(sector).unwrap();
        tree
    }

    #[test]
    fn imported_storage_is_not_registered_until_original_add() {
        let mut tree = tree();
        tree.import_frame(frame(1, Some(b"Mesh"), Some(100)))
            .unwrap();
        assert_eq!(tree.find_frame_fast(Some(7), b"Mesh"), Ok(None));
        tree.add_to_ff_tree(FrameKey(1)).unwrap();
        assert_eq!(
            tree.find_frame_fast(Some(7), b"Mesh"),
            Ok(Some(FrameKey(1)))
        );
        assert_eq!(tree.find_frame_fast(Some(7), b"mesh"), Ok(None));
        assert_eq!(
            tree.find_frame_fast(Some(7), b"Mesh\0suffix"),
            Ok(Some(FrameKey(1)))
        );
        assert_eq!(tree.add_counter, 1);
    }

    #[test]
    fn duplicates_keep_registration_order_and_adopt_the_old_right_subtree() {
        let mut tree = tree();
        for (id, name) in [(1, &b"m"[..]), (2, b"z"), (3, b"a"), (4, b"m"), (5, b"m")] {
            tree.import_frame(frame(id, Some(name), Some(100))).unwrap();
            tree.add_to_ff_tree(FrameKey(id)).unwrap();
        }
        assert_eq!(tree.frame(FrameKey(1)).unwrap().left, Some(FrameKey(3)));
        assert_eq!(tree.frame(FrameKey(1)).unwrap().right, Some(FrameKey(4)));
        assert_eq!(tree.frame(FrameKey(4)).unwrap().right, Some(FrameKey(5)));
        assert_eq!(tree.frame(FrameKey(5)).unwrap().right, Some(FrameKey(2)));
        assert_eq!(
            tree.find_frame_fast_(b"m", FrameKey(5)),
            Ok(Some(FrameKey(5)))
        );
        assert_eq!(tree.last_up, Some(FrameKey(4)));
        assert_eq!(tree.find_frame_fast_(b"m", FrameKey(99)), Ok(None));
        assert_eq!(tree.last_up, Some(FrameKey(5))); // unequal z preserves parent
        tree.remove_from_ff_tree(FrameKey(2)).unwrap();
        assert_eq!(tree.find_frame_fast_(b"m", FrameKey(99)), Ok(None));
        assert_eq!(tree.last_up, None); // exhausted equal chain resets parent
    }

    #[test]
    fn wrong_scene_duplicate_and_outer_sector_are_both_examined() {
        let mut tree = tree();
        let mut wrong_sector = frame(101, Some(b"wrong"), None);
        wrong_sector.frame_type = 5;
        wrong_sector.sector_scene = Some(8);
        tree.import_frame(wrong_sector).unwrap();
        tree.import_frame(frame(1, Some(b"same"), Some(101)))
            .unwrap();
        tree.import_frame(frame(2, Some(b"same"), Some(100)))
            .unwrap();
        tree.add_to_ff_tree(FrameKey(1)).unwrap();
        tree.add_to_ff_tree(FrameKey(2)).unwrap();
        assert_eq!(
            tree.find_frame_fast(Some(7), b"same"),
            Ok(Some(FrameKey(2)))
        );
        tree.frame_storage_mut(FrameKey(101)).unwrap().parent = Some(FrameKey(100));
        assert_eq!(
            tree.find_frame_fast(Some(7), b"same"),
            Ok(Some(FrameKey(1)))
        );
        assert_eq!(tree.find_frame_fast(Some(9), b"same"), Ok(None));
    }

    #[test]
    fn removing_root_promotes_right_and_grafts_left_at_its_leftmost_node() {
        let mut tree = tree();
        for (id, name) in [(1, &b"m"[..]), (2, b"a"), (3, b"z"), (4, b"q")] {
            tree.import_frame(frame(id, Some(name), Some(100))).unwrap();
            tree.add_to_ff_tree(FrameKey(id)).unwrap();
        }
        assert_eq!(
            tree.remove_from_ff_tree(FrameKey(1)),
            Ok(FastFrameRemoval::Removed)
        );
        assert_eq!(tree.root, Some(FrameKey(3)));
        assert_eq!(tree.frame(FrameKey(4)).unwrap().left, Some(FrameKey(2)));
        let removed = tree.frame(FrameKey(1)).unwrap();
        assert_eq!(
            (
                removed.left,
                removed.right,
                removed.flags & FAST_FRAME_TREE_FLAG
            ),
            (None, None, 0)
        );
        assert_eq!(tree.find_frame_fast(Some(7), b"a"), Ok(Some(FrameKey(2))));
    }

    #[test]
    fn null_names_are_empty_for_lookup_but_not_for_insertion_comparisons() {
        let mut tree = tree();
        tree.import_frame(frame(1, None, Some(100))).unwrap();
        tree.import_frame(frame(2, Some(b""), Some(100))).unwrap();
        tree.add_to_ff_tree(FrameKey(1)).unwrap(); // no name read on empty tree
        assert_eq!(tree.find_frame_fast(Some(7), b""), Ok(Some(FrameKey(1))));
        assert_eq!(
            tree.add_to_ff_tree(FrameKey(2)),
            Err(FastFrameError::NullInsertionName(FrameKey(1)))
        );
        assert_eq!(tree.add_counter, 2);
        assert_eq!(
            tree.frame(FrameKey(2)).unwrap().flags & FAST_FRAME_TREE_FLAG,
            0
        );
        tree.last_up = Some(FrameKey(100));
        assert_eq!(
            tree.remove_from_ff_tree(FrameKey(1)),
            Ok(FastFrameRemoval::NullName)
        );
        assert_eq!(tree.root, Some(FrameKey(1)));
        assert_eq!(tree.last_up, Some(FrameKey(100)));
    }

    #[test]
    fn set_name_composes_removal_registration_and_equal_name_no_op() {
        let mut tree = tree();
        tree.import_frame(frame(1, None, Some(100))).unwrap();
        tree.set_name(FrameKey(1), Some(b"old")).unwrap();
        tree.set_name(FrameKey(1), Some(b"old\0ignored")).unwrap();
        assert_eq!(tree.add_counter, 1);
        tree.set_name(FrameKey(1), Some(b"new")).unwrap();
        assert_eq!(tree.add_counter, 2);
        assert_eq!(tree.find_frame_fast(Some(7), b"old"), Ok(None));
        assert_eq!(tree.find_frame_fast(Some(7), b"new"), Ok(Some(FrameKey(1))));
        tree.set_name(FrameKey(1), None).unwrap();
        assert_eq!(tree.root, None);
        assert_eq!(
            tree.frame(FrameKey(1)).unwrap().original_name,
            OriginalFrameName::Null
        );
        tree.fast_find_enabled = false;
        tree.set_name(FrameKey(1), Some(b"unregistered")).unwrap();
        assert_eq!(tree.root, None);
        assert_eq!(tree.add_counter, 2);
    }

    #[test]
    fn malformed_parent_and_cycle_fail_only_when_lookup_reaches_them() {
        let mut tree = tree();
        tree.import_frame(frame(1, Some(b"a"), Some(1))).unwrap();
        tree.add_to_ff_tree(FrameKey(1)).unwrap();
        assert_eq!(tree.find_frame_fast(Some(7), b"missing"), Ok(None));
        assert_eq!(
            tree.find_frame_fast(Some(7), b"a"),
            Err(FastFrameError::Cycle)
        );
        tree.frame_storage_mut(FrameKey(1)).unwrap().parent = Some(FrameKey(999));
        assert_eq!(
            tree.find_frame_fast(Some(7), b"a"),
            Err(FastFrameError::MissingFrame(FrameKey(999)))
        );
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn set_name_allocation_boundary_retains_removal_release_and_original_pointer_state() {
        let mut tree = I3dFastFrameTree::new(true);
        tree.import_frame(FastFrameStorage {
            frame: FrameKey(1),
            name: Some(b"old\0".to_vec()),
            original_name: OriginalFrameName::Offset(2),
            frame_type: 1,
            parent: None,
            sector_scene: None,
            flags: 0x4000_003c,
            left: None,
            right: None,
        })
        .unwrap();
        tree.add_to_ff_tree(FrameKey(1)).unwrap();
        tree.set_name_with_allocator(FrameKey(1), Some(b"old"), |_| {
            panic!("equal names must not allocate")
        })
        .unwrap();
        let result = tree.set_name_with_allocator(FrameKey(1), Some(b"new"), |bytes| {
            assert_eq!(bytes, 4);
            Err(FastFrameError::AllocationFailed)
        });
        assert_eq!(result, Err(FastFrameError::AllocationFailed));
        assert_eq!(tree.root, None);
        assert_eq!(tree.add_counter, 1);
        let frame = tree.frame(FrameKey(1)).unwrap();
        assert_eq!(frame.name, None);
        assert_eq!(
            frame.original_name,
            OriginalFrameName::Released { offset: 2 }
        );
        assert_eq!(frame.flags & FAST_FRAME_TREE_FLAG, 0);
    }
}
