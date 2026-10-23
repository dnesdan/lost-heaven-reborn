//! Shared scene/model membership over the same frame arena and transform graph.
//!
//! Scene construction is deliberately bounded at the viewport/resource boundary
//! of PC RVA 0x48ab0, after the four retained sectors. It is not a completed
//! driver CreateFrame, scene constructor, resource loader or scene destructor.
//! AddFrame/DeleteFrame use RVAs 0x49290/0x49400 for admitted non-emitter frames.
//! Close (0x4a390) exposes unimplemented external effects as ordered boundaries.

use crate::frame_transform::{FrameKey, FrameKind, I3dFrameState};
use crate::model_loading::{ModelArena, ModelError, ModelKey};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneKey(pub(crate) FrameKey);
impl SceneKey {
    pub fn root(self) -> FrameKey {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SectorKey(pub(crate) FrameKey);
impl SectorKey {
    pub fn frame(self) -> FrameKey {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameOwner {
    Model(ModelKey),
    Scene(SceneKey),
}
impl FrameOwner {
    pub fn root(self) -> FrameKey {
        match self {
            Self::Model(key) => key.root(),
            Self::Scene(key) => key.root(),
        }
    }
}
impl From<ModelKey> for FrameOwner {
    fn from(key: ModelKey) -> Self {
        Self::Model(key)
    }
}
impl From<SceneKey> for FrameOwner {
    fn from(key: SceneKey) -> Self {
        Self::Scene(key)
    }
}

#[derive(Clone, Debug)]
pub struct SectorFrame {
    pub key: SectorKey,
    /// Sector +0x214, independent of membership and hierarchy parent.
    pub scene: SceneKey,
    pub sector_flags: u32,
    /// Only primary/backdrop receive SetBoundVolume. None denotes the untouched
    /// invalid constructor bounds, not a valid empty bounding box.
    pub bounds: Option<[[f32; 3]; 2]>,
    pub bounding_sphere: Option<[f32; 4]>,
    pub next_visual: Option<FrameKey>,
}
#[derive(Debug)]
pub struct SceneFrameOwner {
    pub key: SceneKey,
    /// Primary, backdrop, omni and near, in the original retained-reference order.
    pub sectors: [SectorKey; 4],
    pub(crate) frames: Vec<FrameKey>,
    pub(crate) native_capacity: usize,
    pub(crate) growth: usize,
    phase: ClosePhase,
    close_cursor: usize,
    effect_cursor: usize,
    allocation_incomplete: bool,
}
impl SceneFrameOwner {
    pub fn frames(&self) -> &[FrameKey] {
        &self.frames
    }
    pub fn capacity(&self) -> usize {
        self.native_capacity
    }
    pub fn close_phase(&self) -> ClosePhase {
        self.phase
    }
    pub fn close_cursor(&self) -> usize {
        self.close_cursor
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ClosePhase {
    #[default]
    Idle,
    Begin,
    BeforeFrames,
    Frames,
    AfterFrames,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneCloseEffect {
    UnloadManagedTextures,
    FreeActiveAreas,
    DeletePrimarySounds,
    ReleaseLensflares,
    ClearDriverSectors,
    ClearTickList,
    DeleteInterpolation,
    ClearCameraAndSectorState,
    ResetPrimarySound,
    /// Device vtable +0x14 with argument zero (ResourceManagerDiscardBytes).
    DeviceVirtual14,
    /// PC RVA 0x69820, vertex-buffer manager with a null parameter.
    FreeUnusedVertexBuffers,
    /// PC RVA 0x5b9d0, index-buffer manager.
    FreeUnusedIndexBuffers,
    ClearShadowBackPointers,
}
const BEFORE_FRAMES: &[SceneCloseEffect] = &[
    SceneCloseEffect::FreeActiveAreas,
    SceneCloseEffect::DeletePrimarySounds,
    SceneCloseEffect::ReleaseLensflares,
    SceneCloseEffect::ClearDriverSectors,
    SceneCloseEffect::ClearTickList,
    SceneCloseEffect::DeleteInterpolation,
];
const AFTER_FRAMES: &[SceneCloseEffect] = &[
    SceneCloseEffect::ClearCameraAndSectorState,
    SceneCloseEffect::ResetPrimarySound,
    SceneCloseEffect::DeviceVirtual14,
    SceneCloseEffect::FreeUnusedVertexBuffers,
    SceneCloseEffect::FreeUnusedIndexBuffers,
    SceneCloseEffect::ClearShadowBackPointers,
];
/// No production success adapter is supplied. A backend must perform its actual
/// effects, or reject the boundary. Error leaves this phase pending; completed
/// earlier phases and frame releases are never replayed on resume. A backend
/// with several effects must itself retain its progress before returning Err.
/// Reentrant arena mutation is outside this bounded interface: observation is
/// read-only, and owned resource effects must not mutate arena membership.
pub trait SceneCloseBackend {
    fn close_effect(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
        effect: SceneCloseEffect,
    ) -> Result<(), ModelError>;
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SceneOwnerError {
    UnknownScene(SceneKey),
    UnknownSector(SectorKey),
    CloseIncomplete(SceneKey),
    FrameNotOwned { scene: SceneKey, frame: FrameKey },
    BackendUnavailable(SceneCloseEffect),
    AllocationIncomplete(SceneKey),
    LinkRejected { scene: SceneKey, result: u32 },
}
impl From<SceneOwnerError> for ModelError {
    fn from(error: SceneOwnerError) -> Self {
        Self::Scene(error)
    }
}

impl ModelArena {
    /// Construct the common scene/sector ownership fields through PC 0x48e29.
    /// The four sector references are retained directly: neither AddFrame nor
    /// LinkTo runs. Driver/GPU viewport, weather and shared snow resources are
    /// not executed; this API deliberately does not claim a full constructor.
    pub fn construct_scene_owner_prefix(&mut self) -> Result<SceneKey, ModelError> {
        let mut state = I3dFrameState::model();
        state.kind = FrameKind::Scene;
        state.flags |= 0x80;
        let key = SceneKey(self.insert(state, None, Vec::new(), Vec::new(), None)?);
        let mut sectors = Vec::new();
        for (index, (name, flags)) in [
            (b"Primary sector".as_slice(), 0x200),
            (b"Backdrop sector", 0x1000),
            (b"Omni sector", 0x2000),
            (b"Near sector", 0x4000),
        ]
        .into_iter()
        .enumerate()
        {
            let mut state = I3dFrameState::model();
            state.kind = FrameKind::Sector;
            state.flags |= 0x0808_0080 | if index < 2 { 0x40 } else { 0 };
            let sector = SectorKey(self.insert(state, None, Vec::new(), Vec::new(), None)?);
            self.set_frame_name(sector.frame(), Some(name))?;
            if index < 2 {
                let fi = self.frame_index(sector.frame())?;
                self.frames[fi].hr_bounds = crate::mesh_bounds::MeshBoundVolume {
                    bbox: [[-10000.; 3], [10000.; 3]],
                    center: [0.; 3],
                    radius: f32::from_bits(0x46875104),
                };
            }
            self.sectors.push(SectorFrame {
                key: sector,
                scene: key,
                sector_flags: 0x801 | flags,
                bounds: (index < 2).then_some([[-10000.; 3], [10000.; 3]]),
                bounding_sphere: (index < 2).then_some([0., 0., 0., f32::from_bits(0x46875104)]),
                next_visual: None,
            });
            sectors.push(sector);
        }
        self.scenes.push(SceneFrameOwner {
            key,
            sectors: sectors.try_into().unwrap(),
            frames: Vec::new(),
            native_capacity: 0,
            growth: 32,
            phase: ClosePhase::Idle,
            close_cursor: 0,
            effect_cursor: 0,
            allocation_incomplete: false,
        });
        Ok(key)
    }
    pub(crate) fn scene_index(&self, scene: SceneKey) -> Result<usize, ModelError> {
        self.frame(scene.root())?;
        self.scenes
            .iter()
            .position(|s| s.key == scene)
            .ok_or_else(|| SceneOwnerError::UnknownScene(scene).into())
    }
    pub fn scene(&self, scene: SceneKey) -> Result<&SceneFrameOwner, ModelError> {
        Ok(&self.scenes[self.scene_index(scene)?])
    }
    pub fn sector(&self, sector: SectorKey) -> Result<&SectorFrame, ModelError> {
        self.frame(sector.frame())?;
        self.sectors
            .iter()
            .find(|s| s.key == sector)
            .ok_or_else(|| SceneOwnerError::UnknownSector(sector).into())
    }
    pub(crate) fn check_scene_frame_available(&self, key: FrameKey) -> Result<(), ModelError> {
        for scene in &self.scenes {
            if scene.phase == ClosePhase::Frames
                && scene.frames[scene.close_cursor..].contains(&key)
            {
                return Err(SceneOwnerError::CloseIncomplete(scene.key).into());
            }
        }
        Ok(())
    }
    pub(crate) fn check_owner_available(&self, owner: FrameOwner) -> Result<(), ModelError> {
        match owner {
            FrameOwner::Model(key) if self.model(key)?.closing => {
                Err(ModelError::CloseIncomplete(key))
            }
            FrameOwner::Scene(key) => {
                let scene = self.scene(key)?;
                if scene.allocation_incomplete {
                    Err(SceneOwnerError::AllocationIncomplete(key).into())
                } else if scene.phase != ClosePhase::Idle {
                    Err(SceneOwnerError::CloseIncomplete(key).into())
                } else {
                    Ok(())
                }
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn owner_frames(&self, owner: FrameOwner) -> Result<&[FrameKey], ModelError> {
        Ok(match owner {
            FrameOwner::Model(key) => &self.model(key)?.frames,
            FrameOwner::Scene(key) => &self.scene(key)?.frames,
        })
    }
    pub(crate) fn owner_frames_mut(
        &mut self,
        owner: FrameOwner,
    ) -> Result<&mut Vec<FrameKey>, ModelError> {
        Ok(match owner {
            FrameOwner::Model(key) => {
                let i = self.model_index(key)?;
                &mut self.models[i].frames
            }
            FrameOwner::Scene(key) => {
                let i = self.scene_index(key)?;
                &mut self.scenes[i].frames
            }
        })
    }
    pub(crate) fn delete_from_owner_key(
        &mut self,
        owner: FrameOwner,
        key: FrameKey,
    ) -> Result<(), ModelError> {
        match owner {
            FrameOwner::Model(model) => self.delete_frame(model, key),
            FrameOwner::Scene(scene) => self.scene_delete_frame(scene, key),
        }
    }
    pub fn delete_from_owner(&mut self, key: FrameKey) -> Result<(), ModelError> {
        if let Some(owner) = self.frame(key)?.owner {
            self.delete_from_owner_key(owner, key)?;
        }
        Ok(())
    }
    pub fn scene_add_frame(&mut self, scene: SceneKey, key: FrameKey) -> Result<(), ModelError> {
        self.scene_add_frame_with_allocator(scene, key, |_, _| Ok(()))
    }
    /// Native allocation boundary follows retain, old-owner deletion, new-owner
    /// assignment and capacity growth. An error preserves that prefix and locks
    /// the invalid array state; native null-copy/store has no recovery branch.
    /// The callback observes the pre-allocation state; host Vec allocation then
    /// supplies storage. This is not an original heap or ErrorHandler adapter.
    pub fn scene_add_frame_with_allocator(
        &mut self,
        scene: SceneKey,
        key: FrameKey,
        allocate: impl FnOnce(&Self, usize) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        let fi = self.frame_index(key)?;
        self.check_scene_frame_available(key)?;
        // Actual first guard is owner==frame, not owner==destination scene.
        if self.frames[fi]
            .owner
            .is_some_and(|owner| owner.root() == key)
        {
            return Ok(());
        }
        self.check_owner_available(scene.into())?;
        let mut ancestor = Some(FrameOwner::Scene(scene));
        while let Some(owner) = ancestor {
            if owner.root() == key {
                return Err(ModelError::OwnershipCycle);
            }
            ancestor = self.frame(owner.root())?.owner;
        }
        if let Some(owner) = self.frames[fi].owner {
            self.check_owner_available(owner)?;
        }
        self.retain(key)?;
        self.delete_from_owner(key)?;
        self.frames[fi].owner = Some(scene.into());
        if self.scenes[si].frames.len() >= self.scenes[si].native_capacity {
            self.scenes[si].native_capacity = self.scenes[si]
                .native_capacity
                .checked_add(self.scenes[si].growth)
                .ok_or(ModelError::AllocationOverflow)?;
            self.scenes[si].allocation_incomplete = true;
            allocate(self, self.scenes[si].native_capacity)?;
            let additional = self.scenes[si].native_capacity - self.scenes[si].frames.len();
            self.scenes[si]
                .frames
                .try_reserve(additional)
                .map_err(|_| ModelError::AllocationOverflow)?;
            self.scenes[si].allocation_incomplete = false;
        }
        self.scenes[si].frames.push(key);
        Ok(())
    }
    pub fn scene_delete_frame(&mut self, scene: SceneKey, key: FrameKey) -> Result<(), ModelError> {
        self.check_owner_available(scene.into())?;
        let si = self.scene_index(scene)?;
        let fi = self.frame_index(key)?;
        let pos = self.scenes[si]
            .frames
            .iter()
            .position(|&f| f == key)
            .ok_or(SceneOwnerError::FrameNotOwned { scene, frame: key })?;
        self.scenes[si].frames.swap_remove(pos);
        self.frames[fi].owner = None;
        // Native signed-positive guard. Arena-created frames admit positive refs.
        if (self.frames[fi].references as i32) > 0 {
            self.release(key)?;
        }
        Ok(())
    }
    pub fn close_scene_with_backend(
        &mut self,
        scene: SceneKey,
        backend: &mut impl SceneCloseBackend,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        if self.scenes[si].allocation_incomplete {
            return Err(SceneOwnerError::AllocationIncomplete(scene).into());
        }
        if self.scenes[si].phase == ClosePhase::Idle {
            self.scenes[si].phase = ClosePhase::Begin;
        }
        if self.scenes[si].phase == ClosePhase::Begin {
            backend.close_effect(self, scene, SceneCloseEffect::UnloadManagedTextures)?;
            for sector in self.scenes[si].sectors {
                self.sectors
                    .iter_mut()
                    .find(|s| s.key == sector)
                    .unwrap()
                    .next_visual = None;
            }
            self.scenes[si].phase = ClosePhase::BeforeFrames;
        }
        if self.scenes[si].phase == ClosePhase::BeforeFrames {
            while self.scenes[si].effect_cursor < BEFORE_FRAMES.len() {
                backend.close_effect(self, scene, BEFORE_FRAMES[self.scenes[si].effect_cursor])?;
                self.scenes[si].effect_cursor += 1;
            }
            self.scenes[si].effect_cursor = 0;
            self.scenes[si].phase = ClosePhase::Frames;
        }
        while self.scenes[si].phase == ClosePhase::Frames
            && self.scenes[si].close_cursor < self.scenes[si].frames.len()
        {
            let key = self.scenes[si].frames[self.scenes[si].close_cursor];
            let fi = self.frame_index(key)?;
            self.frames[fi].owner = None;
            self.release_inner(key)?;
            self.scenes[si].close_cursor += 1;
        }
        if self.scenes[si].phase == ClosePhase::Frames {
            self.scenes[si].frames.clear();
            self.scenes[si].close_cursor = 0;
            self.scenes[si].phase = ClosePhase::AfterFrames;
        }
        if self.scenes[si].phase == ClosePhase::AfterFrames {
            while self.scenes[si].effect_cursor < AFTER_FRAMES.len() {
                let effect = AFTER_FRAMES[self.scenes[si].effect_cursor];
                if effect == SceneCloseEffect::ResetPrimarySound {
                    let primary = self.scenes[si].sectors[0];
                    self.sectors
                        .iter_mut()
                        .find(|s| s.key == primary)
                        .unwrap()
                        .sector_flags |= 0x800;
                }
                backend.close_effect(self, scene, effect)?;
                self.scenes[si].effect_cursor += 1;
            }
            self.scenes[si].effect_cursor = 0;
            self.scenes[si].phase = ClosePhase::Idle;
        }
        Ok(())
    }
    /// Ownership is unaffected by LinkTo. All name searches read these links.
    pub fn link_frame(
        &mut self,
        key: FrameKey,
        parent: Option<FrameKey>,
        flags: u32,
    ) -> Result<u32, ModelError> {
        self.frame(key)?;
        // Scene virtual LinkTo (PC RVA 0x4e9a0) returns this error without
        // inspecting parent/flags or touching the common hierarchy fields.
        if self.state(key)?.kind == FrameKind::Scene {
            return Err(SceneOwnerError::LinkRejected {
                scene: SceneKey(key),
                result: 0x8000_000e,
            }
            .into());
        }
        if let Some(parent) = parent {
            self.frame(parent)?;
        }
        Ok(self.graph.link_to_with_flags(key, parent, flags)?)
    }
    pub fn get_sector(&self, key: FrameKey) -> Result<Option<SectorKey>, ModelError> {
        let mut current = Some(key);
        while let Some(key) = current {
            if self.state(key)?.kind == FrameKind::Sector {
                return Ok(Some(SectorKey(key)));
            }
            current = self.links(key)?.parent;
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame_fast_lookup::{FastFrameError, OriginalFrameName, FAST_FRAME_TREE_FLAG};
    use crate::frame_transform::FrameError;

    // Controlled empty-resource effects are test inputs, never a runtime adapter.
    #[derive(Default)]
    struct Effects {
        calls: Vec<SceneCloseEffect>,
        reject: Option<SceneCloseEffect>,
    }
    impl SceneCloseBackend for Effects {
        fn close_effect(
            &mut self,
            _: &ModelArena,
            _: SceneKey,
            effect: SceneCloseEffect,
        ) -> Result<(), ModelError> {
            self.calls.push(effect);
            if self.reject == Some(effect) {
                return Err(SceneOwnerError::BackendUnavailable(effect).into());
            }
            Ok(())
        }
    }
    fn dummy(arena: &mut ModelArena, name: &[u8]) -> FrameKey {
        let key = arena
            .insert(I3dFrameState::dummy(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena.set_frame_name(key, Some(name)).unwrap();
        key
    }
    #[test]
    fn construction_prefix_retains_four_distinct_sectors_without_membership_or_parenting() {
        let mut arena = ModelArena::default();
        arena.set_fast_find_enabled(true);
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.state(scene.root()).unwrap().kind, FrameKind::Scene);
        assert_eq!(arena.state(scene.root()).unwrap().flags, 0x4000_00bd);
        let sectors = arena.scene(scene).unwrap().sectors;
        assert!(arena.scene(scene).unwrap().frames().is_empty());
        for (i, name) in [
            b"Primary sector".as_slice(),
            b"Backdrop sector",
            b"Omni sector",
            b"Near sector",
        ]
        .into_iter()
        .enumerate()
        {
            let key = sectors[i].frame();
            let frame = arena.frame(key).unwrap();
            assert_eq!(frame.references, 1);
            assert_eq!(frame.owner, None);
            assert_eq!(frame.name_bytes, name);
            assert_eq!(arena.links(key).unwrap().parent, None);
            assert_eq!(arena.state(key).unwrap().kind, FrameKind::Sector);
            assert_eq!(
                arena.state(key).unwrap().flags,
                FAST_FRAME_TREE_FLAG | if i < 2 { 0x4808_00fd } else { 0x4808_00bd }
            );
            assert_eq!(arena.sector(sectors[i]).unwrap().scene, scene);
            assert_eq!(
                arena.sector(sectors[i]).unwrap().sector_flags,
                [0xa01, 0x1801, 0x2801, 0x4801][i]
            );
            assert_eq!(arena.sector(sectors[i]).unwrap().bounds.is_some(), i < 2);
            assert_eq!(arena.find_frame_fast(scene, name).unwrap(), Some(key));
            assert!(sectors[..i].iter().all(|&s| s != sectors[i]));
        }
        assert!(matches!(
            arena.release(scene.root()),
            Err(ModelError::Frame(FrameError::UnsupportedDestruction(_)))
        ));
    }
    #[test]
    fn scene_and_sector_virtual_updates_stop_at_unimplemented_subtype_effects() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0].frame();
        for key in [scene.root(), sector] {
            let before = *arena.state(key).unwrap();
            assert_eq!(
                arena.graph.update_v(key),
                Err(FrameError::UnsupportedFrameUpdate(key))
            );
            assert_eq!(
                arena.graph.update(key),
                Err(FrameError::UnsupportedFrameUpdate(key))
            );
            assert_eq!(*arena.state(key).unwrap(), before);
        }
    }
    #[test]
    fn scene_virtual_link_rejects_before_reading_parent_or_flags() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let model = arena.create_model().unwrap();
        let scene_before = *arena.state(scene.root()).unwrap();
        let model_before = *arena.state(model.root()).unwrap();
        for (parent, flags) in [
            (None, 0),
            (Some(model.root()), 0),
            (Some(FrameKey(u32::MAX)), u32::MAX),
        ] {
            assert_eq!(
                arena.link_frame(scene.root(), parent, flags),
                Err(SceneOwnerError::LinkRejected {
                    scene,
                    result: 0x8000_000e
                }
                .into())
            );
            assert_eq!(*arena.state(scene.root()).unwrap(), scene_before);
            assert_eq!(*arena.state(model.root()).unwrap(), model_before);
            assert_eq!(arena.links(scene.root()).unwrap(), Default::default());
            assert_eq!(arena.links(model.root()).unwrap(), Default::default());
        }
    }
    #[test]
    fn model_close_bound_invalidation_stops_before_sector_and_its_ancestors() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0].frame();
        let model = arena.create_model().unwrap();
        let outer = arena.create_model().unwrap();
        arena.link_frame(sector, Some(outer.root()), 0).unwrap();
        arena.link_frame(model.root(), Some(sector), 0).unwrap();
        // Seed valid cached bounds after linking, as a completed bounds setter
        // would do. Close must invalidate only below the Sector boundary.
        for key in [model.root(), sector, outer.root()] {
            arena.graph.change_flags(key, 0, 0x140).unwrap();
        }
        let sector_before = *arena.state(sector).unwrap();
        let outer_before = *arena.state(outer.root()).unwrap();
        arena.close(model).unwrap();
        assert_eq!(arena.state(model.root()).unwrap().flags & 0x140, 0);
        assert_eq!(*arena.state(sector).unwrap(), sector_before);
        assert_eq!(*arena.state(outer.root()).unwrap(), outer_before);
        arena.graph.discard_hr_bound_vol(sector).unwrap();
        assert_eq!(*arena.state(sector).unwrap(), sector_before);
        assert_eq!(*arena.state(outer.root()).unwrap(), outer_before);
    }
    #[test]
    fn original_self_owner_guard_returns_without_reference_or_membership_changes() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let model = arena.create_model().unwrap();
        let index = arena.frame_index(model.root()).unwrap();
        // Explicitly seeded pathological native owner field; normal admission
        // rejects ownership cycles rather than constructing this state.
        arena.frames[index].owner = Some(model.into());
        arena.scene_add_frame(scene, model.root()).unwrap();
        assert_eq!(arena.frame(model.root()).unwrap().references, 1);
        assert_eq!(arena.frame(model.root()).unwrap().owner, Some(model.into()));
        assert!(arena.scene(scene).unwrap().frames().is_empty());
    }
    #[test]
    fn model_scene_transfers_keep_parenting_separate_and_same_owner_reorders() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        let a = dummy(&mut arena, b"a");
        let b = dummy(&mut arena, b"b");
        let c = dummy(&mut arena, b"c");
        arena.link_frame(a, Some(sector.frame()), 0).unwrap();
        arena.add_frame(model, a).unwrap();
        arena.release(a).unwrap();
        arena.scene_add_frame(scene, a).unwrap();
        assert!(arena.model(model).unwrap().frames.is_empty());
        assert_eq!(arena.frame(a).unwrap().references, 1);
        assert_eq!(arena.frame(a).unwrap().owner, Some(scene.into()));
        for key in [b, c] {
            arena.scene_add_frame(scene, key).unwrap();
            arena.release(key).unwrap();
        }
        arena.scene_add_frame(scene, a).unwrap();
        assert_eq!(arena.scene(scene).unwrap().frames(), &[c, b, a]);
        assert_eq!(arena.frame(a).unwrap().references, 1);
        arena.add_frame(model, a).unwrap();
        assert_eq!(arena.scene(scene).unwrap().frames(), &[c, b]);
        assert_eq!(arena.frame(a).unwrap().owner, Some(model.into()));
        assert_eq!(arena.links(a).unwrap().parent, Some(sector.frame()));
        arena.scene_delete_frame(scene, c).unwrap();
        assert_eq!(arena.scene(scene).unwrap().frames(), &[b]);
        assert!(matches!(arena.frame(c), Err(ModelError::UnknownFrame(_))));
    }
    #[test]
    fn lookup_reads_current_graph_sector_even_when_owner_is_another_scene() {
        let mut arena = ModelArena::default();
        let a = arena.construct_scene_owner_prefix().unwrap();
        let b = arena.construct_scene_owner_prefix().unwrap();
        arena.set_fast_find_enabled(true);
        let first = dummy(&mut arena, b"same");
        let second = dummy(&mut arena, b"same");
        let sa = arena.scene(a).unwrap().sectors[0].frame();
        let sb = arena.scene(b).unwrap().sectors[0].frame();
        arena.scene_add_frame(a, first).unwrap();
        arena.link_frame(first, Some(sb), 0).unwrap();
        arena.link_frame(second, Some(sa), 0).unwrap();
        assert_eq!(arena.find_frame_fast(a, b"same").unwrap(), Some(second));
        assert_eq!(arena.find_frame_fast(b, b"same").unwrap(), Some(first));
        assert_eq!(arena.frame(second).unwrap().owner, None);
        // Native continues through a sector belonging to a different scene.
        arena.link_frame(sb, Some(sa), 0).unwrap();
        assert_eq!(arena.find_frame_fast(a, b"same").unwrap(), Some(first));
        arena.link_frame(first, None, 0).unwrap();
        assert_eq!(arena.find_frame_fast(b, b"same").unwrap(), None);
    }
    #[test]
    fn close_retains_sectors_capacity_survivors_hierarchy_and_name_registration() {
        let mut arena = ModelArena::default();
        arena.set_fast_find_enabled(true);
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let parent = dummy(&mut arena, b"parent");
        let child = dummy(&mut arena, b"child");
        arena
            .link_frame(parent, Some(sectors[0].frame()), 0)
            .unwrap();
        arena.link_frame(child, Some(parent), 0).unwrap();
        arena.scene_add_frame(scene, parent).unwrap();
        arena.scene_add_frame(scene, child).unwrap();
        arena.release(child).unwrap();
        let mut effects = Effects::default();
        arena.close_scene_with_backend(scene, &mut effects).unwrap();
        assert_eq!(effects.calls.len(), 13);
        assert_eq!(arena.frame(parent).unwrap().references, 1);
        assert_eq!(arena.frame(parent).unwrap().owner, None);
        assert!(arena.frame(child).is_err());
        assert_eq!(
            arena.links(parent).unwrap().parent,
            Some(sectors[0].frame())
        );
        assert_eq!(
            arena.find_frame_fast(scene, b"parent").unwrap(),
            Some(parent)
        );
        assert_eq!(arena.find_frame_fast(scene, b"child").unwrap(), None);
        assert_eq!(arena.scene(scene).unwrap().capacity(), 32);
        assert_eq!(arena.scene(scene).unwrap().sectors, sectors);
        arena.scene_add_frame(scene, parent).unwrap();
        arena.close_scene_with_backend(scene, &mut effects).unwrap();
        assert_eq!(arena.frame(parent).unwrap().references, 1);
        arena.release(parent).unwrap();
        assert_eq!(arena.find_frame_fast(scene, b"parent").unwrap(), None);
        assert_eq!(arena.links(sectors[0].frame()).unwrap().child, None);
    }
    #[test]
    fn unavailable_close_effect_resumes_without_replaying_completed_prefix() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let child = dummy(&mut arena, b"retained");
        arena.scene_add_frame(scene, child).unwrap();
        let mut effects = Effects {
            reject: Some(SceneCloseEffect::ClearDriverSectors),
            ..Effects::default()
        };
        assert!(arena.close_scene_with_backend(scene, &mut effects).is_err());
        assert_eq!(arena.frame(child).unwrap().references, 2);
        assert_eq!(arena.frame(child).unwrap().owner, Some(scene.into()));
        assert_eq!(
            arena.scene(scene).unwrap().close_phase(),
            ClosePhase::BeforeFrames
        );
        assert!(arena.scene_add_frame(scene, child).is_err());
        effects.reject = Some(SceneCloseEffect::DeviceVirtual14);
        assert!(arena.close_scene_with_backend(scene, &mut effects).is_err());
        assert_eq!(arena.frame(child).unwrap().references, 1);
        assert_eq!(arena.frame(child).unwrap().owner, None);
        assert_eq!(
            arena.scene(scene).unwrap().close_phase(),
            ClosePhase::AfterFrames
        );
        effects.reject = None;
        arena.close_scene_with_backend(scene, &mut effects).unwrap();
        assert_eq!(arena.frame(child).unwrap().references, 1);
        assert_eq!(
            effects
                .calls
                .iter()
                .filter(|&&e| e == SceneCloseEffect::UnloadManagedTextures)
                .count(),
            1
        );
        assert_eq!(
            effects
                .calls
                .iter()
                .filter(|&&e| e == SceneCloseEffect::ClearDriverSectors)
                .count(),
            2
        );
        assert_eq!(
            effects
                .calls
                .iter()
                .filter(|&&e| e == SceneCloseEffect::DeviceVirtual14)
                .count(),
            2
        );
    }
    #[test]
    fn unsupported_release_leaves_owner_null_and_pending_close_cannot_be_transferred() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let other = arena.construct_scene_owner_prefix().unwrap();
        let first = dummy(&mut arena, b"first");
        let mesh = arena
            .insert(I3dFrameState::mesh(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        for key in [first, mesh] {
            arena.scene_add_frame(scene, key).unwrap();
            arena.release(key).unwrap();
        }
        let mut effects = Effects::default();
        assert!(arena.close_scene_with_backend(scene, &mut effects).is_err());
        assert!(arena.frame(first).is_err());
        assert_eq!(arena.frame(mesh).unwrap().owner, None);
        assert_eq!(arena.frame(mesh).unwrap().references, 1); // explicit unavailable destructor preflight
        assert_eq!(arena.scene(scene).unwrap().close_cursor(), 1);
        assert!(arena.scene_add_frame(other, mesh).is_err());
        assert!(arena.release(mesh).is_err());
        arena.retain(mesh).unwrap(); // supply an external survivor; no destructor is now required
        arena.close_scene_with_backend(scene, &mut effects).unwrap();
        assert_eq!(arena.frame(mesh).unwrap().references, 1);
        assert_eq!(
            effects
                .calls
                .iter()
                .filter(|&&e| e == SceneCloseEffect::UnloadManagedTextures)
                .count(),
            1
        );
    }
    #[test]
    fn allocation_failure_exposes_reference_transfer_owner_store_and_capacity_growth() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let key = dummy(&mut arena, b"member");
        arena.add_frame(model, key).unwrap();
        arena.release(key).unwrap();
        let err = arena.scene_add_frame_with_allocator(scene, key, |arena, capacity| {
            assert_eq!(capacity, 32);
            assert!(arena.model(model).unwrap().frames.is_empty());
            assert_eq!(arena.frame(key).unwrap().references, 1);
            assert_eq!(arena.frame(key).unwrap().owner, Some(scene.into()));
            assert_eq!(arena.scene(scene).unwrap().capacity(), 32);
            assert!(arena.scene(scene).unwrap().frames().is_empty());
            Err(ModelError::AllocationOverflow)
        });
        assert_eq!(err, Err(ModelError::AllocationOverflow));
        assert!(matches!(
            arena.scene_add_frame(scene, key),
            Err(ModelError::Scene(SceneOwnerError::AllocationIncomplete(_)))
        ));
    }
    #[test]
    fn rename_failure_removes_shared_tree_and_marks_original_pointer_released() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        arena.set_fast_find_enabled(true);
        let key = dummy(&mut arena, b"old");
        let sector = arena.scene(scene).unwrap().sectors[0];
        arena.link_frame(key, Some(sector.frame()), 0).unwrap();
        assert_eq!(arena.find_frame_fast(scene, b"old").unwrap(), Some(key));
        assert!(arena
            .set_frame_name_with_allocator(key, Some(b"new"), |_| Err(
                FastFrameError::AllocationFailed
            ))
            .is_err());
        assert_eq!(arena.find_frame_fast(scene, b"old").unwrap(), None);
        assert_eq!(
            arena.original_name_pointer(key).unwrap(),
            OriginalFrameName::Released { offset: 0 }
        );
        assert!(arena.frame(key).unwrap().name_bytes.is_empty());
        arena.set_frame_name(key, Some(b"new")).unwrap();
        assert_eq!(arena.find_frame_fast(scene, b"new").unwrap(), Some(key));
    }
}
