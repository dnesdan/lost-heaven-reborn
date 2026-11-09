//! Fast-tree operations over live arena names and graph parent links. No copied
//! FastFrameStorage or independently mutable hierarchy is used by this adapter.
use crate::frame_fast_lookup::{FastFrameError, OriginalFrameName, FAST_FRAME_TREE_FLAG};
use crate::frame_transform::{FrameKey, FrameKind};
use crate::model_loading::{ModelArena, ModelError};
use crate::scene_ownership::{SceneKey, SectorKey};
use std::cmp::Ordering;

#[derive(Debug, Default)]
pub(crate) struct ArenaFastTree {
    pub root: Option<FrameKey>,
    pub last_up: Option<FrameKey>,
    pub add_counter: u32,
    pub enabled: bool,
}
impl From<FastFrameError> for ModelError {
    fn from(error: FastFrameError) -> Self {
        Self::FastFrame(error)
    }
}
fn c_name(name: &[u8]) -> &[u8] {
    &name[..name.iter().position(|&b| b == 0).unwrap_or(name.len())]
}
impl ModelArena {
    pub fn set_fast_find_enabled(&mut self, enabled: bool) {
        self.fast_tree.enabled = enabled;
    }
    pub fn fast_tree_root(&self) -> Option<FrameKey> {
        self.fast_tree.root
    }
    pub fn fast_tree_add_counter(&self) -> u32 {
        self.fast_tree.add_counter
    }
    /// Original I3D_frame::GetName pointer projection, PC RVA 0x27570
    /// (mafia-re ls3df/I3D/visuals/I3d_lit_object.cpp:11 note, pure-REA
    /// out-of-line copy). Partial coverage: exposes the live original-name
    /// pointer identity (Null vs Offset) without dereferencing; the native
    /// `m_pName ? m_pName : ""` empty-for-null string semantics stay
    /// caller-owned and are unpinned here. Tier contract.
    pub fn original_name_pointer(&self, key: FrameKey) -> Result<OriginalFrameName, ModelError> {
        Ok(self.frame(key)?.name_pointer)
    }
    /// Working name for the LS3DF-inlined `I3D_frame::GetOrigName` read
    /// (mafia-re ls3df/I3D/I3D.h:589-593). The reference records no LS3DF
    /// FUNCTION address for it and no Game.exe out-of-line copy either:
    /// both binaries inline it. Reports the frame's own name (the model
    /// prefix is stripped by the model propagation, not here); an unset
    /// name reads as empty, like the native `m_pOrigName ? m_pOrigName
    /// : ""`. No cache moves.
    pub fn frame_orig_name(&self, key: FrameKey) -> Result<&[u8], ModelError> {
        Ok(self
            .frame(key)?
            .original_name_bytes
            .as_deref()
            .unwrap_or(b""))
    }
    fn insertion_name(&self, key: FrameKey) -> Result<&[u8], ModelError> {
        let frame = self.frame(key)?;
        if frame.name_is_null {
            return Err(FastFrameError::NullInsertionName(key).into());
        }
        Ok(c_name(&frame.name_bytes))
    }
    /// Arena projection of driver query 0x19b10 `I3D_driver::FindFrameFast`
    /// (mafia-re `ls3df/I3D/I3d_driver.cpp:1595`; canonical read-only walk is
    /// `frame_fast_lookup::I3dFastFrameTree::find_frame_fast`): same BST walk
    /// and PC-continues-past-other-scene-sectors semantics over live arena
    /// names and graph parent links.
    pub fn find_frame_fast(
        &self,
        scene: SceneKey,
        name: &[u8],
    ) -> Result<Option<FrameKey>, ModelError> {
        self.scene(scene)?;
        let name = c_name(name);
        let mut current = self.fast_tree.root;
        while let Some(key) = current {
            let frame = self.frame(key)?;
            match name.cmp(c_name(&frame.name_bytes)) {
                Ordering::Less => current = frame.ff_left,
                Ordering::Greater => current = frame.ff_right,
                Ordering::Equal => {
                    let mut candidate = Some(key);
                    while let Some(key) = candidate {
                        let mut parent = Some(key);
                        while let Some(at) = parent {
                            if self.state(at)?.kind == FrameKind::Sector
                                && self.sector(SectorKey(at))?.scene == Some(scene)
                            {
                                return Ok(Some(key));
                            }
                            parent = self.links(at)?.parent;
                        }
                        candidate = self.frame(key)?.ff_right;
                        if let Some(next) = candidate {
                            if c_name(&self.frame(next)?.name_bytes) != name {
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
    fn find_exact_name(&mut self, wanted: FrameKey) -> Result<bool, ModelError> {
        self.fast_tree.last_up = None;
        let name = self.frame(wanted)?.name_bytes.clone();
        let name = c_name(&name);
        let mut current = self.fast_tree.root;
        while let Some(key) = current {
            let frame = self.frame(key)?;
            match name.cmp(c_name(&frame.name_bytes)) {
                Ordering::Less | Ordering::Greater => {
                    let next = if name < c_name(&frame.name_bytes) {
                        frame.ff_left
                    } else {
                        frame.ff_right
                    };
                    if next.is_none() {
                        return Ok(false);
                    }
                    self.fast_tree.last_up = Some(key);
                    current = next;
                }
                Ordering::Equal => {
                    let mut candidate = key;
                    loop {
                        if candidate == wanted {
                            return Ok(true);
                        }
                        let Some(next) = self.frame(candidate)?.ff_right else {
                            self.fast_tree.last_up = None;
                            return Ok(false);
                        };
                        self.fast_tree.last_up = Some(candidate);
                        candidate = next;
                        if name != c_name(&self.frame(candidate)?.name_bytes) {
                            return Ok(false);
                        }
                    }
                }
            }
        }
        Ok(false)
    }
    /// Register an existing native name without changing it. Re-registering a
    /// currently registered identity would corrupt native tree links and is
    /// rejected at this host boundary.
    pub fn register_frame_name(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if self.state(key)?.flags & FAST_FRAME_TREE_FLAG != 0 {
            return Err(FastFrameError::DuplicateStorage(key).into());
        }
        self.insertion_name(key)?;
        self.add_frame_name_registration(key)
    }
    fn add_frame_name_registration(&mut self, key: FrameKey) -> Result<(), ModelError> {
        self.fast_tree.add_counter = self.fast_tree.add_counter.wrapping_add(1);
        let mut current = self.fast_tree.root;
        if current.is_none() {
            self.fast_tree.root = Some(key);
        }
        while let Some(parent) = current {
            let pi = self.frame_index(parent)?;
            match self.insertion_name(key)?.cmp(self.insertion_name(parent)?) {
                Ordering::Less => {
                    current = self.frames[pi].ff_left;
                    if current.is_none() {
                        self.frames[pi].ff_left = Some(key);
                    }
                }
                Ordering::Greater => {
                    current = self.frames[pi].ff_right;
                    if current.is_none() {
                        self.frames[pi].ff_right = Some(key);
                    }
                }
                Ordering::Equal => {
                    let mut last = parent;
                    while let Some(next) = self.frame(last)?.ff_right {
                        if self.insertion_name(key)? != self.insertion_name(next)? {
                            break;
                        }
                        last = next;
                    }
                    let ki = self.frame_index(key)?;
                    let li = self.frame_index(last)?;
                    self.frames[ki].ff_right = self.frames[li].ff_right;
                    self.frames[li].ff_right = Some(key);
                    break;
                }
            }
        }
        self.graph.change_flags(key, 0, FAST_FRAME_TREE_FLAG)?;
        Ok(())
    }
    pub(crate) fn remove_frame_name_registration(
        &mut self,
        key: FrameKey,
    ) -> Result<(), ModelError> {
        if self.state(key)?.flags & FAST_FRAME_TREE_FLAG == 0 {
            return Ok(());
        }
        self.graph.change_flags(key, FAST_FRAME_TREE_FLAG, 0)?;
        if self.frame(key)?.name_is_null {
            return Ok(());
        }
        if self.find_exact_name(key)? {
            let frame = self.frame(key)?;
            let (left, right) = (frame.ff_left, frame.ff_right);
            let replacement = right.or(left);
            if self.fast_tree.root == Some(key) {
                self.fast_tree.root = replacement;
            } else {
                let pi = self.frame_index(self.fast_tree.last_up.ok_or(FastFrameError::Cycle)?)?;
                if self.frames[pi].ff_left == Some(key) {
                    self.frames[pi].ff_left = replacement;
                } else {
                    self.frames[pi].ff_right = replacement;
                }
            }
            if let (Some(mut right), Some(left)) = (right, left) {
                while let Some(next) = self.frame(right)?.ff_left {
                    right = next;
                }
                let ri = self.frame_index(right)?;
                self.frames[ri].ff_left = Some(left);
            }
        }
        let fi = self.frame_index(key)?;
        self.frames[fi].ff_left = None;
        self.frames[fi].ff_right = None;
        Ok(())
    }
    pub fn set_frame_name(&mut self, key: FrameKey, name: Option<&[u8]>) -> Result<(), ModelError> {
        self.set_frame_name_with_allocator(key, name, |size| {
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(size)
                .map_err(|_| FastFrameError::AllocationFailed)?;
            bytes.resize(size, 0);
            Ok(bytes)
        })
    }
    /// Base SetName, including allocation failure after registration and old
    /// allocation release. The released original pointer is inspectable safely.
    /// Model prefix propagation and sector-specific virtual naming are separate.
    pub fn set_frame_name_with_allocator(
        &mut self,
        key: FrameKey,
        name: Option<&[u8]>,
        allocate: impl FnOnce(usize) -> Result<Vec<u8>, FastFrameError>,
    ) -> Result<(), ModelError> {
        let name = name.map(c_name);
        let fi = self.frame_index(key)?;
        let frame = &self.frames[fi];
        if (!frame.name_is_null).then(|| c_name(&frame.name_bytes)) == name {
            return Ok(());
        }
        let generation = self.fresh_name_generation()?;
        self.remove_frame_name_registration(key)?;
        let frame = &mut self.frames[fi];
        frame.name_generation = generation;
        if !frame.name_is_null {
            frame.name_pointer = match frame.name_pointer {
                OriginalFrameName::Offset(offset) | OriginalFrameName::Released { offset } => {
                    OriginalFrameName::Released { offset }
                }
                OriginalFrameName::Null => OriginalFrameName::Null,
            };
        }
        frame.name_bytes = Vec::new();
        frame.original_name_bytes = None;
        frame.name_is_null = true;
        let Some(name) = name else {
            frame.name_pointer = OriginalFrameName::Null;
            return Ok(());
        };
        let size = name
            .len()
            .checked_add(1)
            .ok_or(FastFrameError::AllocationFailed)?;
        let mut bytes = allocate(size)?;
        if bytes.len() != size {
            return Err(FastFrameError::InvalidNameAllocation.into());
        }
        bytes[..name.len()].copy_from_slice(name);
        bytes[name.len()] = 0;
        bytes.pop();
        let frame = &mut self.frames[fi];
        frame.original_name_bytes = Some(name.to_vec());
        frame.name_bytes = bytes;
        frame.name_is_null = false;
        frame.name_pointer = OriginalFrameName::Offset(0);
        if self.fast_tree.enabled {
            self.add_frame_name_registration(key)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn get_name_pointer_tracks_null_and_allocation() {
        // PC RVA 0x27570 (mafia-re I3d_lit_object.cpp:11 note): GetName
        // exposes the live name pointer with empty-for-null semantics. The
        // pointer projection pins Null vs Offset across SetName (Offset(0)
        // is the fresh-arena first allocation; later arenas use whatever
        // offset the allocator hands out).
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        assert_eq!(
            arena.original_name_pointer(key).unwrap(),
            OriginalFrameName::Null
        );
        arena.set_frame_name(key, Some(b"name")).unwrap();
        assert_eq!(
            arena.original_name_pointer(key).unwrap(),
            OriginalFrameName::Offset(0)
        );
        arena.set_frame_name(key, None).unwrap();
        assert_eq!(
            arena.original_name_pointer(key).unwrap(),
            OriginalFrameName::Null
        );
        assert!(arena.original_name_pointer(FrameKey(u32::MAX)).is_err());
        arena.release(key).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
    }
    #[test]
    fn frame_orig_name_reads_unset_as_empty() {
        // Working name for inlined I3D_frame::GetOrigName (I3D.h:589-593):
        // no LS3DF FUNCTION address and no Game.exe copy exist.
        let mut arena = ModelArena::default();
        let key = arena.create_dummy_frame().unwrap();
        assert_eq!(arena.frame_orig_name(key).unwrap(), b"");
        arena.set_frame_name(key, Some(b"frame")).unwrap();
        assert_eq!(arena.frame_orig_name(key).unwrap(), b"frame");
        arena.set_frame_name(key, None).unwrap();
        assert_eq!(arena.frame_orig_name(key).unwrap(), b"");
        assert!(arena.frame_orig_name(FrameKey(u32::MAX)).is_err());
        arena.release(key).unwrap();
    }
}
