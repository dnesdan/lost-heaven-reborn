//! Shared scene/model membership over the same frame arena and transform graph.
//!
//! Scene construction is deliberately bounded at the viewport/resource boundary
//! of PC RVA 0x48ab0, after the four retained sectors. It is not a completed
//! driver CreateFrame, scene constructor, resource loader or scene destructor.
//! AddFrame/DeleteFrame use RVAs 0x49290/0x49400 for admitted non-emitter frames.
//! Close (0x4a390) exposes unimplemented external effects as ordered boundaries.

use crate::frame_camera::CameraKey;
use crate::frame_lookup::{
    ENUM_CANCELED, FrameLookupError, enum_subtree, find_child_callback_slot,
};
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
    pub scene: Option<SceneKey>,
    pub geometry: crate::frame_sector::SectorGeometry,
    pub sector_flags: u32,
    /// Only primary/backdrop receive SetBoundVolume. None denotes the untouched
    /// invalid constructor bounds, not a valid empty bounding box.
    pub bounds: Option<[[f32; 3]; 2]>,
    pub bounding_sphere: Option<[f32; 4]>,
    pub next_visual: Option<FrameKey>,
    /// Native non-owning I3D_light pointer list; light.sectors is reciprocal.
    pub lights: Vec<FrameKey>,
    /// Native fog registration alongside lights, selected by AddLight/UpdateV.
    pub num_fog_lights: u32,
    pub fog_light: Option<FrameKey>,
    /// Host retry cursor for an interrupted native-order AddLight prefix.
    pub(crate) pending_light_add: Option<PendingLightAdd>,
    /// Native non-owning I3D_sound list; sound.sectors is reciprocal.
    pub sounds: Vec<FrameKey>,
    pub(crate) hull_occluder: Option<crate::frame_occluder::OccluderKey>,
    pub(crate) occluder_list: Option<crate::frame_occluder::OccluderKey>,
    pub sound_properties: [u32; crate::scene_weather::SECTOR_SOUND_PROPERTIES_WORDS],
    pub sound_words_1e8_1ec: [u32; 2],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PendingLightAdd {
    pub(crate) light: FrameKey,
    primary_appended: bool,
    light_type: u32,
    on: bool,
}
/// Opaque native SCN_CALLBACK* token (PC RVA 0x49120). The engine never calls
/// through it here; the integer preserves the stored address at the boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SceneCallback(pub u32);

/// Frustum-tick flag word, I3D.h FRMFLAGS_TICK = 0x400000, evidenced by the
/// TEST EAX,0x400000 probe in RVA 0x49400 bytes and the OR/AND pair in the
/// RVAs 0x49f90/0x4a050 Ghidra bodies.
pub const SCENE_TICK_FLAG: u32 = 0x0040_0000;
/// sceneSS skip mask, TARGET|INTERPOLATOR, evidenced by the
/// TEST [ESI+0xAC],0x10004000 probe in the RVA 0x4a950 Ghidra body.
pub const SCENE_SS_SKIP_FLAGS: u32 = 0x1000_4000;
pub const SCENE_OK: u32 = 0;
/// ls3df.h I3DERR_OBJECTNOTFOUND, evidenced by the DeleteTickFrame/DeleteShadow
/// Ghidra bodies and the DeleteFrame byte walk.
pub const SCENE_OBJECT_NOT_FOUND: u32 = 0x8000_0006;
/// I3D.h visual discriminants used by the sceneSS SetSpace predicate.
pub const SCENE_VISUAL_OBJECT: u32 = 0x5f4a_424f;
pub const SCENE_VISUAL_LIT_OBJECT: u32 = 0x5041_4d4c;
/// Working name for the `SCN_CALLBACK_PRE_RENDER` gate (mafia-re
/// ls3df/I3D/I3D.h:1315-1318 at pin 113b5b6: the scene began, before anything
/// is drawn; `cb_flags` carries bit n for message n). The reference records
/// no LS3DF FUNCTION address for a standalone reader; the bit is produced by
/// `SetCallback` (PC RVA 0x49120, already occupied, not claimed here) and
/// consumed inline by the render pass. Exact bit test on the caller-supplied
/// flags word; no owner or device is consulted.
pub fn scene_callback_wants_pre_render(cb_flags: u32) -> bool {
    cb_flags & 0x1 != 0
}
/// Working name for the `SCN_CALLBACK_POST_SCENE` gate (mafia-re
/// ls3df/I3D/I3D.h:1319 at pin 113b5b6: the scene is drawn, before the near
/// sector and the lens flares). Same inline-only status as
/// `scene_callback_wants_pre_render`: no standalone LS3DF FUNCTION entry
/// exists, so no RVA is claimed here. Exact bit test; no state moves.
pub fn scene_callback_wants_post_scene(cb_flags: u32) -> bool {
    cb_flags & 0x2 != 0
}
/// Working name for the `SCN_CALLBACK_POST_RENDER` gate (mafia-re
/// ls3df/I3D/I3D.h:1320 at pin 113b5b6: everything is drawn, before the scene
/// ends). Same inline-only status as `scene_callback_wants_pre_render`: no
/// standalone LS3DF FUNCTION entry exists, so no RVA is claimed here. Exact
/// bit test; no state moves.
pub fn scene_callback_wants_post_render(cb_flags: u32) -> bool {
    cb_flags & 0x4 != 0
}

#[derive(Debug)]
pub struct SceneFrameOwner {
    pub key: SceneKey,
    pub(crate) current_camera: Option<crate::frame_camera::CameraKey>,
    /// Native callback at this+0x140 with flags at +0x144 (PC RVA 0x49120).
    /// Constructor-zeroed; a null store forces the flags word to zero.
    pub(crate) callback: Option<SceneCallback>,
    pub(crate) callback_flags: u32,
    /// Tick C_list triple at this+0x148/0x14C/0x150/0x154 (PC RVAs
    /// 0x49f90/0x4a050). Count is tick_frames.len(); capacity/growth mirror
    /// the native words (ctor: capacity 0, growth 32).
    pub(crate) tick_frames: Vec<FrameKey>,
    pub(crate) tick_capacity: usize,
    pub(crate) tick_growth: usize,
    /// Last SetSpace world value applied (PC RVA 0x4a8d0). The native
    /// SetSpaceParam is a file-static u8; the host keeps it per-scene so two
    /// live scenes cannot clobber each other (documented deviation).
    pub(crate) world_space: bool,
    pub(crate) area_head: Option<crate::frame_area::AreaKey>,
    pub(crate) area_count: u32,
    pub(crate) areas_changed: bool,
    pub(crate) portal_head: Option<crate::frame_portal::PortalKey>,
    pub(crate) portal_count: u32,
    pub(crate) portal_version: u32,
    /// Host CPU intake guard, not a reconstructed native field.
    pub(crate) cpu_world_intake_active: bool,
    pub(crate) sector_head: Option<SectorKey>,
    pub(crate) sector_count: u32,
    pub(crate) occluder_head: Option<crate::frame_occluder::OccluderKey>,
    pub(crate) occluder_count: u32,
    pub(crate) occluder_flag_274: bool,
    pub(crate) occluders_changed: bool,
    pub(crate) backdrop_range: [f32; 2],
    pub(crate) field_220: [u32; 3],
    /// Primary, backdrop, omni and near, in the original retained-reference order.
    pub sectors: [SectorKey; 4],
    pub(crate) frames: Vec<FrameKey>,
    pub(crate) native_capacity: usize,
    pub(crate) growth: usize,
    /// Native shadow list (PC RVAs 0x4a5a0/0x4a640). Membership is the host
    /// analog of the element back-pointer at +0x188: the native same-scene
    /// guard and the delete swap-remove order are preserved on it.
    pub(crate) shadows: Vec<FrameKey>,
    /// Native mirror list (PC RVAs 0x4a6f0/0x4a790), same discipline with the
    /// element back-pointer at +0x1e0.
    pub(crate) mirrors: Vec<FrameKey>,
    /// Weather dummy registry (native ws_dummies, PC RVAs 0x4bbc0/0x4bc30).
    /// Typed entries; raw C_list words stay in the scene_weather word view.
    pub(crate) weather_dummies: Vec<crate::scene_weather_dummy::WeatherDummyEntry>,
    /// Scene-owned IPolSet (native ipol_set at scene+0x178, PC RVAs
    /// 0x4b0f0..0x4b390). Separate owner from every model controller; a scene
    /// allocation never aliases a model allocation and vice versa.
    pub(crate) scene_animation: Option<crate::scene_animation_runtime::OriginalSceneController>,
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
    FrameNotOwned {
        scene: SceneKey,
        frame: FrameKey,
    },
    BackendUnavailable(SceneCloseEffect),
    AllocationIncomplete(SceneKey),
    LinkRejected {
        scene: SceneKey,
        result: u32,
    },
    SectorGeometryUnavailable(SceneKey),
    LightnessUnavailable(SectorKey),
    LightAddIncomplete {
        sector: SectorKey,
        light: FrameKey,
    },
    /// A compatibility-imported scene controller without its resource owner.
    UnsupportedActiveSceneController(SceneKey),
    /// Sector EnumFrames mask filtering lives in the unported sector-enum
    /// layer (PC RVA 0x49180); an explicit mask is rejected, never filtered.
    EnumMaskUnavailable,
}
impl From<SceneOwnerError> for ModelError {
    fn from(error: SceneOwnerError) -> Self {
        Self::Scene(error)
    }
}

/// Native sceneSS visit decision inputs (PC RVA 0x4a950). The classifier
/// backend reads live visual state; the traversal owns order, the
/// TARGET|INTERPOLATOR skip, and child/next linkage. Required predicate
/// (I3d_scene.cpp:683-690): skip subtree for native types MODEL/VOLUME;
/// SetSpace iff (is_visual_frame && vt==OBJ_) or vt==LMAP, with
/// SCENE_VISUAL_OBJECT/LIT_OBJECT above.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneSpaceVisit {
    SkipSubtree,
    Recurse,
    SetSpaceAndRecurse,
}
/// Unported per-visual/per-portal effect boundary for SetSpace (PC RVA
/// 0x4a8d0), sceneSS (0x4a950) and MakeReadySectors (0x4a880). The native
/// I3D_visual::SetSpace virtual and I3D_portal::UpdatePortal stay here.
pub trait SceneSpaceBackend {
    fn visit(&self, kind: crate::frame_transform::FrameKind, frame: FrameKey) -> SceneSpaceVisit;
    fn set_space(&mut self, frame: FrameKey, world: bool);
    fn update_portal(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
        portal: crate::frame_portal::PortalKey,
    ) -> Result<(), ModelError>;
}
/// Ordered SceneOpen phases (PC RVA 0x4a300). CloseScene runs even for a null
/// filename; LoadHierarchy's word propagates verbatim; MakeReadySectors runs
/// through the real make_ready_sectors, not through this backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneOpenEffect {
    CloseScene,
    LoadHierarchy,
    UpdateCamera,
}
/// LoadHierarchy file wiring, camera Update virtual and Close effects stay
/// backend-owned; only their order and words are reconstructed here.
pub trait SceneOpenBackend {
    fn open_effect(
        &mut self,
        arena: &ModelArena,
        scene: SceneKey,
        effect: SceneOpenEffect,
    ) -> Result<u32, ModelError>;
}

impl ModelArena {
    pub fn global_sector_version(&self) -> u32 {
        self.sector_version
    }
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
            let mut sound_properties = [0; crate::scene_weather::SECTOR_SOUND_PROPERTIES_WORDS];
            crate::scene_weather::setup_sector_sound_properties_defaults(&mut sound_properties);
            self.sector_version = self.sector_version.wrapping_add(1);
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
                scene: Some(key),
                geometry: crate::frame_sector::SectorGeometry::default(),
                sector_flags: 0x801 | flags,
                bounds: (index < 2).then_some([[-10000.; 3], [10000.; 3]]),
                bounding_sphere: (index < 2).then_some([0., 0., 0., f32::from_bits(0x46875104)]),
                next_visual: None,
                lights: Vec::new(),
                num_fog_lights: 0,
                fog_light: None,
                pending_light_add: None,
                sounds: Vec::new(),
                hull_occluder: None,
                occluder_list: None,
                sound_properties,
                sound_words_1e8_1ec: [0; 2],
            });
            sectors.push(sector);
        }
        self.scenes.push(SceneFrameOwner {
            key,
            current_camera: None,
            callback: None,
            callback_flags: 0,
            tick_frames: Vec::new(),
            tick_capacity: 0,
            tick_growth: 32,
            world_space: false,
            area_head: None,
            area_count: 0,
            areas_changed: false,
            portal_head: None,
            portal_count: 0,
            portal_version: 0,
            cpu_world_intake_active: false,
            sector_head: None,
            sector_count: 0,
            occluder_head: None,
            occluder_count: 0,
            occluder_flag_274: false,
            occluders_changed: false,
            backdrop_range: [1., 10000.],
            field_220: [0, 0x3f000000, 0x3ecccccd],
            sectors: sectors.try_into().unwrap(),
            frames: Vec::new(),
            native_capacity: 0,
            growth: 32,
            shadows: Vec::new(),
            mirrors: Vec::new(),
            weather_dummies: Vec::new(),
            scene_animation: None,
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
    /// Original SetCallback, PC RVA 0x49120 (contract: Ghidra formed no body,
    /// LAB_10049120/not_in_procedure; 256 B byte decode + mafia-re
    /// I3d_scene.cpp:185-194). Null stores force the flags word to zero; the
    /// previous callback is returned on both paths (observed RETN 12 pair).
    pub fn scene_set_callback(
        &mut self,
        scene: SceneKey,
        callback: Option<SceneCallback>,
        flags: u32,
    ) -> Result<Option<SceneCallback>, ModelError> {
        let si = self.scene_index(scene)?;
        // Effect order per the byte decode: flags store, old read, callback
        // store. No validation, no callback invocation at this boundary.
        self.scenes[si].callback_flags = if callback.is_none() { 0 } else { flags };
        let old = self.scenes[si].callback;
        self.scenes[si].callback = callback;
        Ok(old)
    }
    /// Helper-phase read of the callback words (no function credit).
    pub fn scene_callback(
        &self,
        scene: SceneKey,
    ) -> Result<(Option<SceneCallback>, u32), ModelError> {
        let owner = self.scene(scene)?;
        Ok((owner.callback, owner.callback_flags))
    }
    /// Original AddTickFrame, PC RVA 0x49f90 (differential: Ghidra body 188 B,
    /// 56 insn/10 BB). push_back_unique then an unconditional TICK OR. The
    /// search precedes the reserve, so host OOM leaves no flag residue.
    /// check_owner_available is reused host admission without native evidence.
    pub fn scene_add_tick_frame(
        &mut self,
        scene: SceneKey,
        key: FrameKey,
    ) -> Result<(), ModelError> {
        self.scene_add_tick_frame_with_reserve(scene, key, |list| {
            list.try_reserve(1)
                .map_err(|_| ModelError::AllocationOverflow)
        })
    }
    pub fn scene_add_tick_frame_with_reserve(
        &mut self,
        scene: SceneKey,
        key: FrameKey,
        mut reserve: impl FnMut(&mut Vec<FrameKey>) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        self.frame(key)?;
        self.check_owner_available(scene.into())?;
        if self.scenes[si].tick_frames.contains(&key) {
            self.graph.change_flags(key, 0, SCENE_TICK_FLAG)?;
            return Ok(());
        }
        if self.scenes[si].tick_frames.len() >= self.scenes[si].tick_capacity {
            self.scenes[si].tick_capacity = self.scenes[si]
                .tick_capacity
                .checked_add(self.scenes[si].tick_growth)
                .ok_or(ModelError::AllocationOverflow)?;
            reserve(&mut self.scenes[si].tick_frames)?;
        }
        self.scenes[si].tick_frames.push(key);
        self.graph.change_flags(key, 0, SCENE_TICK_FLAG)?;
        Ok(())
    }
    /// Driver 0x1a0a0 DeleteFromSceneTickList (mafia-re
    /// `ls3df/I3D/I3d_driver.cpp:1744-1750`, evidenced PS2 original name):
    /// newest scene first (`for (i = scenes.size(); i--;)`), stop after the
    /// first per-scene DeleteTickFrame reporting `I3D_OK`. Void natively.
    /// Per-scene misses (and dead keys, disclosed host hardening — native
    /// would dereference) continue the walk instead of stopping it.
    pub fn driver_delete_from_scene_tick_list(&mut self, key: FrameKey) {
        for i in (0..self.scenes.len()).rev() {
            let scene = self.scenes[i].key;
            if self
                .scene_delete_tick_frame(scene, key)
                .map(|r| r == SCENE_OK)
                .unwrap_or(false)
            {
                break;
            }
        }
    }
    /// Original DeleteTickFrame, PC RVA 0x4a050 (differential: Ghidra body 141 B,
    /// 51 insn/12 BB). Swap-with-last removal with the count update and slot
    /// fill preceding the TICK clear; absent members return the
    /// OBJECTNOTFOUND word with no state touched.
    pub fn scene_delete_tick_frame(
        &mut self,
        scene: SceneKey,
        key: FrameKey,
    ) -> Result<u32, ModelError> {
        let si = self.scene_index(scene)?;
        self.frame(key)?;
        self.check_owner_available(scene.into())?;
        let Some(pos) = self.scenes[si].tick_frames.iter().position(|&f| f == key) else {
            return Ok(SCENE_OBJECT_NOT_FOUND);
        };
        self.scenes[si].tick_frames.swap_remove(pos);
        self.graph.change_flags(key, SCENE_TICK_FLAG, 0)?;
        Ok(SCENE_OK)
    }
    /// Original DeleteFrame, PC RVA 0x49400 (contract: Ghidra formed no body,
    /// LAB_10049400/not_in_procedure; entry bytes read this+0x234 and test
    /// FRMFLAGS_TICK, I3D_OK RETN-8 path in-window; mafia-re
    /// I3d_scene.cpp:275-288). Word-return form of scene_delete_frame: owned
    /// removal, owner NULL, TICK untrack, conditional release. Membership
    /// precedes frame liveness, as in the original: a non-member yields the
    /// miss word even for a destroyed key (host admission — native would be
    /// a dangling-pointer call). The TICK live-read runs after owned removal,
    /// as in the original; removal runs no callbacks so the read is stable.
    /// The miss word matches the evidenced DeleteTickFrame/DeleteShadow
    /// 0x80000006 (ls3df.h:27).
    pub fn scene_delete_frame_word(
        &mut self,
        scene: SceneKey,
        key: FrameKey,
    ) -> Result<u32, ModelError> {
        let si = self.scene_index(scene)?;
        self.check_owner_available(scene.into())?;
        let Some(pos) = self.scenes[si].frames.iter().position(|&f| f == key) else {
            return Ok(SCENE_OBJECT_NOT_FOUND);
        };
        // Member frames are live by construction (admission rejects dead
        // keys); a failure here is host-state corruption, never a word.
        let fi = self.frame_index(key)?;
        self.scenes[si].frames.swap_remove(pos);
        self.frames[fi].owner = None;
        if self.state(key)?.flags & SCENE_TICK_FLAG != 0 {
            self.graph.change_flags(key, SCENE_TICK_FLAG, 0)?;
            if let Some(tp) = self.scenes[si].tick_frames.iter().position(|&f| f == key) {
                self.scenes[si].tick_frames.swap_remove(tp);
            }
        }
        if (self.frames[fi].references as i32) > 0 {
            self.release(key)?;
        }
        Ok(SCENE_OK)
    }
    /// Destructor camera step of PC RVA 0x48ed0 (differential evidence for the
    /// ordered prefix: vtable reset + Close precede this; the listener/VB and
    /// shared-snow teardown stay backend-owned, see OPEN below). The camera's
    /// sector link clears through the existing producer, then the scene's
    /// reference releases and the slot nulls. The native scene-link null at
    /// camera+0x3F4 has no typed accessor yet; clear_sector covers the sector
    /// word and clear_scene is deliberately not invoked (the scene row dies).
    pub fn scene_destructor_detach_camera(&mut self, scene: SceneKey) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        if let Some(camera) = self.scenes[si].current_camera {
            self.camera_mut(camera)?.clear_sector();
            self.release(camera.frame())?;
            self.scenes[si].current_camera = None;
        }
        Ok(())
    }
    /// Destructor registry step of PC RVA 0x48ed0 (contract: Ghidra body
    /// 588B/165insn/28BB evidences the ordered prefix — vtable reset, Close,
    /// camera detach, 4 sector releases, listener delete, WSRefCnt/snow/VB
    /// block, driver-list removal, 7 list buffers — of which only the
    /// camera-detach, controller-close and registry-removal steps are
    /// reconstructed here; sector releases, listener/VB/shared-snow teardown
    /// and driver-list internals stay backend-owned, disclosed). Allowed only
    /// after Close completed (phase Idle) with no owned frames and no pending
    /// allocation; otherwise the destructor order is violated. Runs the
    /// scene-controller close first (native `delete ipol_set` after Close);
    /// an unsupported compat-imported controller blocks forgetting. Sector
    /// records would otherwise keep a stale scene link (native sector dtors
    /// null the ring links); the host nulls the scene side here without
    /// running sector destructors (disclosed partial).
    pub fn scene_forget_after_close(&mut self, scene: SceneKey) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        let owner = &self.scenes[si];
        if owner.phase != ClosePhase::Idle
            || !owner.frames.is_empty()
            || owner.allocation_incomplete
        {
            return Err(SceneOwnerError::CloseIncomplete(scene).into());
        }
        self.close_original_scene_controller(scene)?;
        for sector in self.sectors.iter_mut() {
            if sector.scene == Some(scene) {
                sector.scene = None;
            }
        }
        let si = self.scene_index(scene)?;
        self.scenes.swap_remove(si);
        Ok(())
    }
    /// Original AddShadow, PC RVA 0x4a5a0 (differential: Ghidra body 44
    /// insn/7 BB). The element back-pointer guard (+0x188) precedes growth;
    /// membership is its host analog. Host Vec discipline follows the
    /// sector_add_light_with_reserve precedent: fallible try_reserve(1)
    /// first, so AllocationOverflow mutates nothing (disclosed divergence
    /// from the native capacity-bump-before-new order, which shows no
    /// recovery branch in the 44-insn body).
    pub fn scene_add_shadow(
        &mut self,
        scene: SceneKey,
        shadow: FrameKey,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        self.frame(shadow)?;
        self.check_owner_available(scene.into())?;
        if self.scenes[si].shadows.contains(&shadow) {
            return Ok(());
        }
        self.scenes[si]
            .shadows
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        self.scenes[si].shadows.push(shadow);
        Ok(())
    }
    /// Original DeleteShadow, PC RVA 0x4a640 (differential: Ghidra body 58
    /// insn/11 BB). Linear search; size-1 fast path and size>1
    /// swap-with-last share the back-pointer clear; a miss keeps state and
    /// returns 0x80000006 (the dbgPrintf miss string has no host log analog).
    pub fn scene_delete_shadow(
        &mut self,
        scene: SceneKey,
        shadow: FrameKey,
    ) -> Result<u32, ModelError> {
        let si = self.scene_index(scene)?;
        self.frame(shadow)?;
        self.check_owner_available(scene.into())?;
        let Some(pos) = self.scenes[si].shadows.iter().position(|&f| f == shadow) else {
            return Ok(SCENE_OBJECT_NOT_FOUND);
        };
        self.scenes[si].shadows.swap_remove(pos);
        Ok(SCENE_OK)
    }
    /// Original AddMirror, PC RVA 0x4a6f0 (differential: Ghidra body 44
    /// insn/7 BB, byte-identical shape to AddShadow at the mirror offsets
    /// with the back-pointer at +0x1e0). Same order and OOM discipline.
    pub fn scene_add_mirror(
        &mut self,
        scene: SceneKey,
        mirror: FrameKey,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        self.frame(mirror)?;
        self.check_owner_available(scene.into())?;
        if self.scenes[si].mirrors.contains(&mirror) {
            return Ok(());
        }
        self.scenes[si]
            .mirrors
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        self.scenes[si].mirrors.push(mirror);
        Ok(())
    }
    /// Original DeleteMirror, PC RVA 0x4a790 (differential: Ghidra body 58
    /// insn/11 BB, mirror-offset twin of DeleteShadow). Same words.
    pub fn scene_delete_mirror(
        &mut self,
        scene: SceneKey,
        mirror: FrameKey,
    ) -> Result<u32, ModelError> {
        let si = self.scene_index(scene)?;
        self.frame(mirror)?;
        self.check_owner_available(scene.into())?;
        let Some(pos) = self.scenes[si].mirrors.iter().position(|&f| f == mirror) else {
            return Ok(SCENE_OBJECT_NOT_FOUND);
        };
        self.scenes[si].mirrors.swap_remove(pos);
        Ok(SCENE_OK)
    }
    /// Original SetGlobalColor, PC RVA 0x49520 (contract: Ghidra formed no
    /// body, LAB_10049520/not_in_procedure; entry bytes fetch this+0x210,
    /// corroborating the primary-sector access; mafia-re
    /// I3d_scene.cpp:290-298). In-order diffuse store over the primary
    /// sector's light list; no validation, no return, no fog/sector-flag
    /// effects. An empty list is a no-op. The diffuse setter and any device
    /// call stay backend contracts. No float math (bitwise color copy).
    /// Host numeric-unavailable errors from the light cache surface only
    /// after every member was stored: the loop never aborts early, so the
    /// native hit-every-light order holds and the first error reports.
    pub fn scene_set_global_color(
        &mut self,
        scene: SceneKey,
        color: [f32; 3],
    ) -> Result<(), ModelError> {
        let primary = self.scene(scene)?.sectors[0];
        // Live re-read per member: the setter may mutate sector state.
        let lights = self.sector(primary)?.lights.clone();
        let mut first_error = None;
        for light in lights {
            if let Err(error) = self
                .graph
                .set_light_diffuse(light, color)
                .map_err(ModelError::from)
            {
                first_error = first_error.or(Some(error));
            }
        }
        if let Some(error) = first_error {
            return Err(error);
        }
        Ok(())
    }
    /// Original CreateSector, PC RVA 0x4a140 (differential: Ghidra body 38
    /// insn/6 BB: operator_new(0x234), null-checked, ctor, ring link via
    /// FUN_1004f260). The host reuses create_sector_frame (alloc+ctor
    /// defaults) plus sector_add_to_scene_ring (borrowed scene ring). A fresh
    /// sector carries scene None and no occluder, so the ring link cannot hit
    /// the old-ring-removal or occluder rejections; scene_index above covers
    /// the unknown-scene arm. Either OOM precedes any arena push.
    pub fn create_scene_sector(&mut self, scene: SceneKey) -> Result<SectorKey, ModelError> {
        self.scene_index(scene)?;
        self.check_owner_available(scene.into())?;
        let key = self.create_sector_frame()?;
        self.sector_add_to_scene_ring(key, Some(scene))?;
        Ok(key)
    }
    /// Original GetSector list overload, PC RVA 0x4a240 (contract: Ghidra
    /// formed no body, procedure:null; 24-insn probe walk with RET 0x10 and
    /// the reverse scale-4 index MOV EAX,[EDI+ESI*4-4]; mafia-re
    /// I3d_scene.cpp:505-512). Reverse list order with primary fallback; an
    /// empty list returns primary. Precision policy reuses sector_check_point
    /// (PC24); no new float arithmetic here.
    pub fn scene_get_sector_from_list(
        &mut self,
        scene: SceneKey,
        position: [f32; 3],
        list: &[SectorKey],
    ) -> Result<SectorKey, ModelError> {
        let si = self.scene_index(scene)?;
        for &key in list.iter().rev() {
            self.sector(key)?;
            if self.sector_check_point(key, position)? {
                return Ok(key);
            }
        }
        Ok(self.scenes[si].sectors[0])
    }
    /// Original MakeReadySectors, PC RVA 0x4a880 (differential: Ghidra body 23
    /// insn/5 BB: EnumFrames SECTOR with the 0x4a8c0 helper, then the portal
    /// loop over count [ESI+0x254]/head [ESI+0x258]). Order: sector-ring
    /// MakeReady via the existing producer, then portals in scene order via
    /// the backend (UpdatePortal 0x419e0 is unported). The host walks the
    /// scene ring where native enumerates primary/backdrop/omni/near (see
    /// the scene_set_space disclosure). The EnumProc helper gets no function
    /// credit. Void native: the first host error aborts; completed MakeReady
    /// flags are never rolled back.
    pub fn make_ready_sectors(
        &mut self,
        scene: SceneKey,
        backend: &mut impl SceneSpaceBackend,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        let mut current = self.scenes[si].sector_head;
        for _ in 0..self.scenes[si].sector_count {
            let key = current.ok_or(crate::frame_sector::SectorError::InvalidRing)?;
            self.sector_make_ready(key)?;
            current = self.sector(key)?.geometry.next;
        }
        let mut portal = self.scenes[si].portal_head;
        for _ in 0..self.scenes[si].portal_count {
            let key = portal.ok_or(crate::frame_sector::SectorError::InvalidRing)?;
            backend.update_portal(self, scene, key)?;
            portal = self.portal(key)?.next;
        }
        Ok(())
    }
    /// Original SetSpace + sceneSS, PC RVAs 0x4a8d0 (contract: Ghidra formed
    /// no body, procedure:null; 33-insn walk with the file-static byte store
    /// MOV [0x101b4e5c],CL, the primary fetch at +0x210, the
    /// TEST [ESI+0xac],0x10004000 skip and the CMP EAX,0xb VOLUME skip) and
    /// 0x4a950 (differential: Ghidra body 31 insn/10 BB, preorder with live
    /// next-reads and the (is_visual && vt==OBJ_)|vt==LMAP predicate). One
    /// Rust implementation covers both RVAs: the entry path (param store +
    /// recurse from primary) and the sceneSS traversal share it, so both
    /// overlay entries point here with their respective tiers. The param
    /// stores before traversal (native order). The traversal is iterative
    /// pre-order over the same visit order with a bounded host stack; host
    /// Vec growth may fail mid-traversal after partial backend effects with
    /// no rollback (native is infallible — disclosed). Likewise MakeReady
    /// below walks the scene ring where native runs EnumFrames(ENUMF_SECTOR)
    /// over primary/backdrop/omni/near: order/coverage equivalence is
    /// asserted for the registered loaded sectors, not body-evidenced
    /// (disclosed). Native-owned: TARGET|INTERPOLATOR skip, child/next
    /// linkage. Backend-owned: per-type skip (MODEL/VOLUME; the arena has no
    /// volume kind) and the visual-type classification (visual_type has no
    /// arena home yet).
    pub fn scene_set_space(
        &mut self,
        scene: SceneKey,
        world: bool,
        backend: &mut impl SceneSpaceBackend,
    ) -> Result<(), ModelError> {
        let si = self.scene_index(scene)?;
        self.check_owner_available(scene.into())?;
        self.scenes[si].world_space = world;
        let root = self.scenes[si].sectors[0].frame();
        // Iterative pre-order mirroring the recursive native visit order.
        let mut stack = Vec::new();
        stack
            .try_reserve(1)
            .map_err(|_| ModelError::AllocationOverflow)?;
        stack.push(root);
        while let Some(frame) = stack.pop() {
            if self.state(frame)?.flags & SCENE_SS_SKIP_FLAGS != 0 {
                continue;
            }
            match backend.visit(self.state(frame)?.kind, frame) {
                SceneSpaceVisit::SkipSubtree => continue,
                SceneSpaceVisit::Recurse => {}
                SceneSpaceVisit::SetSpaceAndRecurse => backend.set_space(frame, world),
            }
            let mut child = self.links(frame)?.child;
            let mut siblings = Vec::new();
            while let Some(next) = child {
                siblings.push(next);
                child = self.links(next)?.next;
            }
            stack
                .try_reserve(siblings.len())
                .map_err(|_| ModelError::AllocationOverflow)?;
            for sibling in siblings.into_iter().rev() {
                stack.push(sibling);
            }
        }
        Ok(())
    }
    /// Original Open, PC RVA 0x4a300 (differential: Ghidra body 51 insn/9 BB).
    /// Effect order, phased like close_scene_with_backend with no replay:
    /// Begin closes via the backend (native vtable+0xc4 Close runs first,
    /// even before the null-filename check); Load runs the hierarchy intake
    /// as a backend effect with its word preserved verbatim (failures
    /// propagate); Finish updates the camera via the backend, then runs the
    /// real make_ready_sectors. Null filename returns I3D_OK (0) after Close
    /// ran. LoadHierarchy wiring, portal UpdatePortal and the camera Update
    /// virtual stay backend effects.
    pub fn open_scene_with_backend(
        &mut self,
        scene: SceneKey,
        file_name: Option<&[u8]>,
        backend: &mut impl SceneOpenBackend,
        space: &mut impl SceneSpaceBackend,
    ) -> Result<u32, ModelError> {
        self.scene_index(scene)?;
        backend.open_effect(self, scene, SceneOpenEffect::CloseScene)?;
        let Some(_) = file_name else {
            return Ok(SCENE_OK);
        };
        let ret = backend.open_effect(self, scene, SceneOpenEffect::LoadHierarchy)?;
        backend.open_effect(self, scene, SceneOpenEffect::UpdateCamera)?;
        self.make_ready_sectors(scene, space)?;
        Ok(ret)
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
        // ClearTickList (one of the BeforeFrames effects above) releases the
        // native tick C_list; the arena mirror drops membership and capacity
        // here, before the ordered frame walk. Idempotent under effect retry.
        if self.scenes[si].phase == ClosePhase::Frames {
            self.scenes[si].tick_frames.clear();
            self.scenes[si].tick_capacity = 0;
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
                if effect == SceneCloseEffect::ClearCameraAndSectorState {
                    if let Some(camera) = self.scenes[si].current_camera {
                        self.camera_mut(camera)?.clear_sector();
                    }
                }
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
    /// Host CPU composition boundary, deliberately separate from native Close.
    /// The native backend has rejected DeviceVirtual14. Only a proved absence
    /// of native managed residency permits advancing this adapter's cursor;
    /// no device invocation or original Close function credit is implied.
    pub(crate) fn project_scene_cpu_device_boundary(
        &mut self,
        scene: SceneKey,
        _: crate::scene_resources::CpuManagedResidencyAbsent,
    ) -> Result<(), ModelError> {
        let index = self.scene_index(scene)?;
        if self.scenes[index].phase != ClosePhase::AfterFrames
            || AFTER_FRAMES.get(self.scenes[index].effect_cursor)
                != Some(&SceneCloseEffect::DeviceVirtual14)
        {
            return Err(
                SceneOwnerError::BackendUnavailable(SceneCloseEffect::DeviceVirtual14).into(),
            );
        }
        self.scenes[index].effect_cursor += 1;
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
        if self.state(key)?.kind == FrameKind::Area {
            return self.link_area_frame_derived(key, parent, flags);
        }
        if self.state(key)?.kind == FrameKind::Occluder {
            return self.link_occluder_frame_derived(key, parent, flags);
        }
        if let Some(parent) = parent {
            self.frame(parent)?;
        }
        self.link_frame_base(key, parent, flags)
    }
    pub fn sector_lights_changed(&self, sector: SectorKey) -> Result<u32, ModelError> {
        self.sector(sector)?;
        Ok(self
            .graph
            .sector_light_changes
            .get(&sector.frame().0)
            .copied()
            .unwrap_or(0))
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x508b0, VA 0x100508b0.
    /// I3D_sector::SetGroupBits (mafia-re ls3df/I3D/I3D.h:948-954), evidenced
    /// original symbol. Stores the group bit word then bumps the
    /// lights-changed counter once (native order: store, then
    /// `lights_changed++`). Wrapping arithmetic matches the native 32-bit
    /// increment. No portal, sound, or render effects.
    pub fn sector_set_group_bits(
        &mut self,
        sector: SectorKey,
        bits: u32,
    ) -> Result<(), ModelError> {
        self.sector(sector)?;
        let si = self
            .sectors
            .iter()
            .position(|entry| entry.key == sector)
            .expect("validated sector has a registry entry");
        self.sectors[si].geometry.group_bits = bits;
        let changed = self
            .graph
            .sector_light_changes
            .entry(sector.frame().0)
            .or_default();
        *changed = changed.wrapping_add(1);
        Ok(())
    }
    /// Upstream working name, not an evidenced original symbol: direct read
    /// of the group-bit word the occupied `SetGroupBits` setter (PC RVA
    /// 0x508b0; mafia-re ls3df/I3D/I3D.h:948-954 in-class body at pin
    /// 113b5b6) stores. The original exposes no `I3D_sector::GetGroupBits`.
    /// No new RVA is claimed; same availability guard as `sector`.
    pub fn sector_group_bits_of(&self, sector: SectorKey) -> Result<u32, ModelError> {
        Ok(self.sector(sector)?.geometry.group_bits)
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x4e970, VA 0x1004e970.
    /// I3D_scene::Update (mafia-re ls3df/I3D/I3D.h:1452, in-class empty body;
    /// linker copies at ls3df/I3D/I3D.h:1764), evidenced original symbol. The
    /// native body is empty: no state is read or written. The host validates
    /// the scene handle and performs no other effect.
    pub fn scene_update_noop(&self, scene: SceneKey) -> Result<(), ModelError> {
        self.scene(scene)?;
        Ok(())
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x4e980, VA 0x1004e980.
    /// I3D_scene::SetDir (mafia-re ls3df/I3D/I3D.h:1450, in-class empty body;
    /// linker copies at ls3df/I3D/I3D.h:1770), evidenced original symbol. The
    /// native body is empty: direction and roll are accepted and ignored with
    /// no float conversion, so nonfinite inputs are preserved (never read).
    pub fn scene_set_dir_noop(
        &self,
        scene: SceneKey,
        _dir: [f32; 3],
        _roll: f32,
    ) -> Result<(), ModelError> {
        self.scene(scene)?;
        Ok(())
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x4e990, VA 0x1004e990.
    /// I3D_scene::SetName (mafia-re ls3df/I3D/I3D.h:1455, in-class
    /// `return I3D_OK` body; linker copies at ls3df/I3D/I3D.h:1776), evidenced
    /// original symbol. The name bytes are never read; the result word is
    /// always I3D_OK (0).
    pub fn scene_set_name_ok(
        &self,
        scene: SceneKey,
        _name: Option<&[u8]>,
    ) -> Result<u32, ModelError> {
        self.scene(scene)?;
        Ok(0)
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x4e9a0, VA 0x1004e9a0.
    /// I3D_scene::LinkTo (mafia-re ls3df/I3D/I3D.h:1456, in-class
    /// `return I3DERR_UNSUPPORTED` body; linker copies at
    /// ls3df/I3D/I3D.h:1782), evidenced original symbol. Parent and flags are
    /// never inspected and no hierarchy fields move; the result word is
    /// always I3DERR_UNSUPPORTED (0x8000000e).
    pub fn scene_link_to_unsupported(
        &self,
        scene: SceneKey,
        _frame: Option<FrameKey>,
        _flags: u32,
    ) -> Result<u32, ModelError> {
        self.scene(scene)?;
        Ok(0x8000_000e)
    }
    /// Working name for the LS3DF-inlined `I3D_scene::GetPrimarySector`
    /// read (mafia-re ls3df/I3D/I3D.h:1546-1549). No LS3DF FUNCTION
    /// address exists for it: the reference gives only the inline
    /// declaration, and the game's copy is not claimed here. The
    /// constructor prefix retains the four sectors in
    /// primary/backdrop/omni/near order, so index 0 is the primary
    /// sector; no caches move.
    pub fn scene_primary_sector(&self, scene: SceneKey) -> Result<SectorKey, ModelError> {
        Ok(self.scene(scene)?.sectors[0])
    }
    /// Working name for the LS3DF-inlined `I3D_scene::backdrop_sector` read
    /// (mafia-re ls3df/I3D/I3D.h:1565). No LS3DF FUNCTION address exists
    /// for it: the member is only stored by the sector ring setup. Slot 1
    /// of the retained primary/backdrop/omni/near order; no caches move.
    pub fn scene_backdrop_sector(&self, scene: SceneKey) -> Result<SectorKey, ModelError> {
        Ok(self.scene(scene)?.sectors[1])
    }
    /// Working name for the LS3DF-inlined `I3D_scene::omni_sector` read
    /// (mafia-re ls3df/I3D/I3D.h:1566). No LS3DF FUNCTION address exists
    /// for it. Slot 2 of the retained primary/backdrop/omni/near order.
    pub fn scene_omni_sector(&self, scene: SceneKey) -> Result<SectorKey, ModelError> {
        Ok(self.scene(scene)?.sectors[2])
    }
    /// Working name for the LS3DF-inlined `I3D_scene::near_sector` read
    /// (mafia-re ls3df/I3D/I3D.h:1567). No LS3DF FUNCTION address exists
    /// for it. Slot 3 of the retained primary/backdrop/omni/near order.
    pub fn scene_near_sector(&self, scene: SceneKey) -> Result<SectorKey, ModelError> {
        Ok(self.scene(scene)?.sectors[3])
    }
    /// Original I3D_scene::FindFrame, PC RVA 0x49210 (VA 0x10049210; REA
    /// formed Ghidra body FUN_10049210 in LS3DF.dll e211c850...,
    /// ev_f460b45f7931c1a6a849b30b4de3fb024f64b368264dc1a5fdf4c2ffc9ab;
    /// mafia-re ls3df/I3D/I3d_scene.cpp:226-249 at pin 113b5b6). Order-exact dispatch over the retained
    /// primary/backdrop/omni/near order: the primary sector first, then the
    /// ENUMF-gated backdrop (0x40000), omni (0x80000) and near (0x100000)
    /// sectors; the first hit across the chain wins and later sectors stay
    /// unvisited. Each step runs the shared `enum_subtree` (PC RVA 0x1c020)
    /// store-first-hit composition with the `FindChildCallBack` word (PC RVA
    /// 0x1c310) instead of a direct `GetChildFrame` vcall: the two walk the
    /// same self-first preorder for a first-hit search, but full-name
    /// (model-prefixed) matching stays with the unported full `GetChildFrame`
    /// matcher (original-name matching only, disclosed). A null mask matches
    /// the first visited frame, like the native NULL-name path. An unknown
    /// scene maps to `MissingPrimaryRoot`, matching a missing primary root.
    pub fn scene_find_frame(
        &self,
        scene: SceneKey,
        name: Option<&[u8]>,
        flags: u32,
        visits: usize,
    ) -> Result<Option<FrameKey>, FrameLookupError> {
        let sectors = self
            .scene(scene)
            .map(|owner| owner.sectors)
            .map_err(|_| FrameLookupError::MissingPrimaryRoot)?;
        let chain = [
            (sectors[0], 0),
            (sectors[1], 0x40000),
            (sectors[2], 0x80000),
            (sectors[3], 0x100000),
        ];
        for (sector, gate) in chain {
            if gate != 0 && flags & gate == 0 {
                continue;
            }
            let mut hit = None;
            let outcome = enum_subtree(
                self,
                sector.frame(),
                &mut |key| find_child_callback_slot(&mut hit, key),
                flags,
                name,
                visits,
            )?;
            if outcome == ENUM_CANCELED {
                return Ok(hit);
            }
        }
        Ok(None)
    }
    /// Working name for the `occluders_changed` state-flag read at +0x275
    /// (mafia-re ls3df/I3D/I3D.h:1581). No LS3DF FUNCTION address exists
    /// for it: the flag is only stored, cleared by the constructor
    /// (mafia-re ls3df/I3D/I3d_scene.cpp:109) and set by the occluder ring
    /// joins (mafia-re ls3df/I3D/frames/I3d_occluder.cpp:125,143).
    pub fn scene_occluders_changed(&self, scene: SceneKey) -> Result<bool, ModelError> {
        Ok(self.scene(scene)?.occluders_changed)
    }
    /// Working name for the `areas_changed` state-flag read at +0x280
    /// (mafia-re ls3df/I3D/I3D.h:1582). No LS3DF FUNCTION address exists
    /// for it: the flag is only stored, cleared by the constructor
    /// (mafia-re ls3df/I3D/I3d_scene.cpp:112) and set by the area ring
    /// joins (mafia-re ls3df/I3D/frames/I3d_area.cpp:66,83).
    pub fn scene_areas_changed(&self, scene: SceneKey) -> Result<bool, ModelError> {
        Ok(self.scene(scene)?.areas_changed)
    }
    /// Working name for the in-class empty `I3D_scene::SetOn`
    /// (mafia-re ls3df/I3D/I3D.h:1454). No LS3DF FUNCTION address exists
    /// for it and the reference marks no Game.exe out-of-line copy either;
    /// the body is empty, so the flag byte is accepted and ignored with no
    /// state change beyond handle validation.
    pub fn scene_set_on_noop(&self, scene: SceneKey, _on: bool) -> Result<(), ModelError> {
        self.scene(scene)?;
        Ok(())
    }
    /// Working name for the in-class empty `I3D_scene::SetCallback`
    /// `I3D_CALLBACK` overload (mafia-re ls3df/I3D/I3D.h:1453). No LS3DF
    /// FUNCTION address exists for it and the reference marks no Game.exe
    /// out-of-line copy either. This overload is distinct from the stored
    /// `SCN_CALLBACK` one (RVA 0x49120): the callback and mask are accepted
    /// and ignored with no store and no result word.
    pub fn scene_frame_callback_noop(
        &self,
        scene: SceneKey,
        _callback: Option<u32>,
        _flags: u32,
    ) -> Result<(), ModelError> {
        self.scene(scene)?;
        Ok(())
    }
    /// Working name for the in-class `I3D_scene::Duplicate` returning
    /// `I3D_OK` (mafia-re ls3df/I3D/I3D.h:1458). No LS3DF FUNCTION address
    /// exists for it and the reference marks no Game.exe out-of-line copy
    /// either. The source frame is validated and otherwise unread; the
    /// result word is always `I3D_OK` (0).
    pub fn scene_duplicate_ok(
        &self,
        scene: SceneKey,
        _frame: Option<FrameKey>,
    ) -> Result<u32, ModelError> {
        self.scene(scene)?;
        Ok(0)
    }
    /// Working name for the in-class empty `I3D_scene::vfunc_66`
    /// (mafia-re ls3df/I3D/I3D.h:1529-1530; PC only, the name is a guess).
    /// No LS3DF FUNCTION address exists for it and the reference marks no
    /// Game.exe out-of-line copy either; the body is empty, so both words
    /// are accepted and ignored with no state change beyond handle
    /// validation.
    pub fn scene_vfunc_66_noop(&self, scene: SceneKey, _a: u32, _b: u32) -> Result<(), ModelError> {
        self.scene(scene)?;
        Ok(())
    }
    /// Working name for the sector ring head read at scene +0x264
    /// (mafia-re `ls3df/I3D/I3D.h:1575-1577`). No LS3DF FUNCTION address
    /// exists for a head getter: the ring is walked directly by the portal
    /// connection and `MakeReadySectors` passes. Borrows the live head
    /// link; the ring linkage itself stays with the sector join/leave
    /// paths. Unknown scenes are an explicit host error.
    pub fn scene_sector_head(&self, scene: SceneKey) -> Result<Option<SectorKey>, ModelError> {
        Ok(self.scene(scene)?.sector_head)
    }
    /// Working name for the LS3DF-inlined `I3D_scene::SetFrameSector`
    /// (mafia-re ls3df/I3D/I3D.h:5096-5100). No LS3DF FUNCTION address
    /// exists for it; the only out-of-line copy is Game.exe 0x47adf0, which
    /// is not claimed here. Composes the live world position with the owned
    /// `SetFrameSectorPos` (RVA 0x4a280) backend in native order; no device
    /// or render state moves.
    pub fn scene_set_frame_sector(
        &mut self,
        scene: SceneKey,
        frame: FrameKey,
    ) -> Result<u32, ModelError> {
        let world = self.world_matrix(frame)?;
        let position: [f32; 3] = world.elements[12..15].try_into().map_err(|_| {
            ModelError::Frame(crate::frame_transform::FrameError::NumericDomain(
                "SetFrameSector position",
            ))
        })?;
        self.set_frame_sector_pos(scene, frame, position)
    }
    /// Working name for the tick-list count at scene +0x148 (mafia-re
    /// `ls3df/I3D/I3D.h:1554`; `tick_frames` C_list). No LS3DF FUNCTION
    /// address exists for a count getter: only `AddTickFrame` (RVA 0x49f90)
    /// and `DeleteTickFrame` (RVA 0x4a050) have code addresses, both already
    /// ported and not re-credited here.
    pub fn scene_tick_frame_count(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.tick_frames.len())
    }
    /// Working name for the shadow-list count at scene +0x158 (mafia-re
    /// `ls3df/I3D/I3D.h:1555`). No LS3DF FUNCTION address exists for a count
    /// getter: only `AddShadow` (RVA 0x4a5a0) and `DeleteShadow`
    /// (RVA 0x4a640) have code addresses, both already ported.
    pub fn scene_shadow_count(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.shadows.len())
    }
    /// Working name for the mirror-list count at scene +0x168 (mafia-re
    /// `ls3df/I3D/I3D.h:1556`). No LS3DF FUNCTION address exists for a count
    /// getter: only `AddMirror` (RVA 0x4a6f0) and `DeleteMirror`
    /// (RVA 0x4a790) have code addresses, both already ported.
    pub fn scene_mirror_count(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.mirrors.len())
    }
    /// Working name for the owned-frame count of the scene `C_list` at
    /// +0x22c (mafia-re `ls3df/I3D/I3D.h:1569`). No LS3DF FUNCTION address
    /// exists for a count getter: only `AddFrame` (RVA 0x49290) and
    /// `DeleteFrame` (RVA 0x49400) have code addresses, both already ported.
    /// Unknown scenes are an explicit host error.
    pub fn scene_owned_frame_count(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.frames.len())
    }
    /// Working name for the owned-frame list capacity word kept next to the
    /// +0x22c list (same `C_list` capacity discipline as the tick list at
    /// +0x148, whose capacity/growth readers already exist). The reference
    /// records no LS3DF FUNCTION address for a standalone reader. Reports
    /// the live capacity; unknown scenes are an explicit host error.
    pub fn scene_owned_capacity_of(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.native_capacity)
    }
    /// Working name for the owned-frame list growth step kept next to the
    /// +0x22c list (same discipline as `tick_growth`). The reference records
    /// no LS3DF FUNCTION address for a standalone reader. Reports the live
    /// step; unknown scenes are an explicit host error.
    pub fn scene_owned_growth_of(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.growth)
    }
    /// Working name for the `num_occluders` ring count at scene +0x26c
    /// (mafia-re `ls3df/I3D/I3D.h:1578-1579`). No LS3DF FUNCTION address
    /// exists for a count getter: the ring is maintained by occluder
    /// join/leave paths. Unknown scenes are an explicit host error.
    pub fn scene_occluder_count(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.occluder_count)
    }
    /// Working name for the `num_areas` ring count at scene +0x278 (mafia-re
    /// `ls3df/I3D/I3D.h:1583-1584`). No LS3DF FUNCTION address exists for a
    /// count getter: the ring is maintained by area join/leave paths.
    pub fn scene_area_count(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.area_count)
    }
    /// Working name for the last `SetSpace` world value applied (mafia-re
    /// `ls3df/I3D/I3d_scene.cpp:672-676`; file-static `SetSpaceParam` at
    /// `I3d_scene.cpp:56-60`). No LS3DF FUNCTION address exists for a
    /// getter: only the `SetSpace` setter (RVA 0x4a8d0, already ported) has
    /// one. The host keeps the flag per scene so two live scenes cannot
    /// clobber each other (documented deviation from the file-static).
    pub fn scene_world_space(&self, scene: SceneKey) -> Result<bool, ModelError> {
        Ok(self.scene(scene)?.world_space)
    }
    /// Working name for the `unknown_274` occluder-ring flag at scene +0x274
    /// (mafia-re `ls3df/I3D/I3D.h:1580`; set when an occluder joins the ring
    /// in `I3d_occluder.cpp`). No LS3DF FUNCTION address exists for it: the
    /// flag is only stored, never exposed through a discrete getter.
    pub fn scene_occluder_flag_274(&self, scene: SceneKey) -> Result<bool, ModelError> {
        Ok(self.scene(scene)?.occluder_flag_274)
    }
    /// Working name for the `ipol_set` presence read at scene +0x178
    /// (mafia-re `ls3df/I3D/I3D.h:1557`; made by the first `SetAnimation`,
    /// RVA 0x4b0f0, already ported). No LS3DF FUNCTION address exists for a
    /// presence getter: the animation verbs dereference the set directly.
    pub fn scene_has_animation_set(&self, scene: SceneKey) -> Result<bool, ModelError> {
        Ok(self.scene(scene)?.scene_animation.is_some())
    }
    /// Working name for the `bg_color` words at scene +0x220
    /// (mafia-re `ls3df/I3D/I3D.h:1568`; stored by the constructor
    /// `I3d_scene.cpp:96` as (0.0, 0.5, 0.4)). No LS3DF FUNCTION
    /// address exists for a getter: only `SetGlobalColor`
    /// (RVA 0x49520, already ported) writes lights. Reports the raw
    /// stored words; no render state moves.
    pub fn scene_bg_color(&self, scene: SceneKey) -> Result<[u32; 3], ModelError> {
        Ok(self.scene(scene)?.field_220)
    }
    /// Working name for the `portal_version` word at scene +0x25c
    /// (mafia-re `ls3df/I3D/I3D.h:1576`). No LS3DF FUNCTION address
    /// exists for a getter: the word is only maintained by the portal
    /// connection pass. Reports the live word.
    pub fn scene_portal_version(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.portal_version)
    }
    /// Working name for the areas ring head at scene +0x27c
    /// (mafia-re `ls3df/I3D/I3D.h:1583-1584`). No LS3DF FUNCTION
    /// address exists for a head getter: the ring is maintained by
    /// area join/leave paths. Borrows the live head link.
    pub fn scene_area_head(
        &self,
        scene: SceneKey,
    ) -> Result<Option<crate::frame_area::AreaKey>, ModelError> {
        Ok(self.scene(scene)?.area_head)
    }
    /// Working name for the occluders ring head at scene +0x270
    /// (mafia-re `ls3df/I3D/I3D.h:1578-1579`). No LS3DF FUNCTION
    /// address exists for a head getter: the ring is maintained by
    /// occluder join/leave paths. Borrows the live head link.
    pub fn scene_occluder_head(
        &self,
        scene: SceneKey,
    ) -> Result<Option<crate::frame_occluder::OccluderKey>, ModelError> {
        Ok(self.scene(scene)?.occluder_head)
    }
    /// Working name for the portals ring head at scene +0x258
    /// (mafia-re `ls3df/I3D/I3D.h:1572-1573`). The reference records no
    /// LS3DF FUNCTION address for a head getter: only the portal
    /// interconnection pass links it. Borrows the live head link; unknown
    /// scenes are an explicit host error.
    pub fn scene_portal_head(
        &self,
        scene: SceneKey,
    ) -> Result<Option<crate::frame_portal::PortalKey>, ModelError> {
        Ok(self.scene(scene)?.portal_head)
    }
    /// Working name for the tick-frames list at scene +0x148
    /// (mafia-re `ls3df/I3D/I3D.h:1554`: the frames the scene ticks,
    /// FRMFLAGS_TICK). The reference records no LS3DF FUNCTION address for
    /// a list getter: only `AddTickFrame` (RVA 0x49f90) and
    /// `DeleteTickFrame` (RVA 0x4a050) have code addresses, both already
    /// ported. Borrows the live list; no state moves.
    pub fn scene_tick_frames(&self, scene: SceneKey) -> Result<&[FrameKey], ModelError> {
        Ok(&self.scene(scene)?.tick_frames)
    }
    /// Working name for the shadows list at scene +0x158
    /// (mafia-re `ls3df/I3D/I3D.h:1555`). The reference records no LS3DF
    /// FUNCTION address for a list getter: only `AddShadow` (RVA 0x4a5a0)
    /// and `DeleteShadow` (RVA 0x4a640) have code addresses, both already
    /// ported. Borrows the live list; no state moves.
    pub fn scene_shadows(&self, scene: SceneKey) -> Result<&[FrameKey], ModelError> {
        Ok(&self.scene(scene)?.shadows)
    }
    /// Working name for the mirrors list at scene +0x168
    /// (mafia-re `ls3df/I3D/I3D.h:1556`). The reference records no LS3DF
    /// FUNCTION address for a list getter: only `AddMirror` (RVA 0x4a6f0)
    /// and `DeleteMirror` (RVA 0x4a790) have code addresses, both already
    /// ported. Borrows the live list; no state moves.
    pub fn scene_mirrors(&self, scene: SceneKey) -> Result<&[FrameKey], ModelError> {
        Ok(&self.scene(scene)?.mirrors)
    }
    /// Working name for the tick-list capacity at scene +0x148
    /// (mafia-re `ls3df/I3D/I3D.h:1554`; grown by `AddTickFrame`,
    /// RVA 0x49f90, already ported). No LS3DF FUNCTION address exists
    /// for a capacity getter. Reports the live host capacity word.
    pub fn scene_tick_capacity(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.tick_capacity)
    }
    /// Working name for the tick-list growth step (ctor 32)
    /// (mafia-re `ls3df/I3D/I3d_scene.cpp:90-148` list-growth comment).
    /// No LS3DF FUNCTION address exists for a growth getter.
    pub fn scene_tick_growth(&self, scene: SceneKey) -> Result<usize, ModelError> {
        Ok(self.scene(scene)?.tick_growth)
    }
    /// Working name for the `num_sectors` ring count at scene +0x260
    /// (mafia-re `ls3df/I3D/I3D.h:1575-1577`). No LS3DF FUNCTION address
    /// exists for a count getter: the ring is maintained by sector
    /// join/leave paths. Reports the live word.
    pub fn scene_sector_count_of(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.sector_count)
    }
    /// Working name for the `num_portals` ring count at scene +0x254
    /// (mafia-re `ls3df/I3D/I3D.h:1571-1572`). No LS3DF FUNCTION address
    /// exists for a count getter: the ring is maintained by portal
    /// join/leave paths. Reports the live word.
    pub fn scene_portal_count_of(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.portal_count)
    }
    /// Working name for the backdrop range pair at scene +0x24c
    /// (mafia-re `ls3df/I3D/I3D.h:1571`; written by `SetBackdropRange`).
    /// No LS3DF FUNCTION address exists for a range getter: only the setter
    /// has one. Reports the stored pair bit-exactly; no render moves.
    pub fn scene_backdrop_range_of(&self, scene: SceneKey) -> Result<[f32; 2], ModelError> {
        Ok(self.scene(scene)?.backdrop_range)
    }
    /// Working name for the backdrop near plane at scene +0x24c
    /// (mafia-re `ls3df/I3D/I3D.h:1571` `backdrop_ncp`; written by
    /// `SetBackdropRange`). No LS3DF FUNCTION address exists for a near
    /// getter: only the setter has one. Reports the stored word
    /// bit-exactly; no render moves.
    pub fn scene_backdrop_near_of(&self, scene: SceneKey) -> Result<f32, ModelError> {
        Ok(self.scene(scene)?.backdrop_range[0])
    }
    /// Working name for the backdrop far plane at scene +0x250
    /// (mafia-re `ls3df/I3D/I3D.h:1571` `backdrop_fcp`; written by
    /// `SetBackdropRange`). No LS3DF FUNCTION address exists for a far
    /// getter: only the setter has one. Reports the stored word
    /// bit-exactly; no render moves.
    pub fn scene_backdrop_far_of(&self, scene: SceneKey) -> Result<f32, ModelError> {
        Ok(self.scene(scene)?.backdrop_range[1])
    }
    /// Working name for the callback presence at scene +0x140
    /// (mafia-re `ls3df/I3D/I3D.h:1552` `callback`; written by
    /// `SetCallback` PC RVA 0x49120, already occupied). No LS3DF FUNCTION
    /// address exists for a presence getter. Reports whether a callback is
    /// installed; no device is consulted.
    pub fn scene_has_callback(&self, scene: SceneKey) -> Result<bool, ModelError> {
        Ok(self.scene(scene)?.callback.is_some())
    }
    /// Working name for the callback flags word at scene +0x144
    /// (mafia-re `ls3df/I3D/I3D.h:1553` `cb_flags`; written by
    /// `SetCallback` PC RVA 0x49120, already occupied). No LS3DF FUNCTION
    /// address exists for a flags getter: the word is only read back with
    /// the callback. A null callback always reads zero, like the native
    /// null store. Reports the live word; no device is consulted.
    pub fn scene_callback_flags_of(&self, scene: SceneKey) -> Result<u32, ModelError> {
        Ok(self.scene(scene)?.callback_flags)
    }
    /// LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6,
    /// RVA 0x4a8c0, VA 0x1004a8c0. Upstream static
    /// `MakeReadySectors::EnumHelp::I3D_EnumProc` used by
    /// `I3D_scene::MakeReadySectors` (mafia-re ls3df/I3D/I3d_scene.cpp:651-657),
    /// not an evidenced original symbol. The reference annotates this
    /// address as a FUNCTION in LS3DF.dll (code, not data; no Game.exe
    /// address is used). This RVA is already credited to
    /// `frame_sector::ModelArena::make_ready_sectors_enum_proc`, so this
    /// wrapper carries no new function credit: same native, second host
    /// spelling. The native blind cast trusts the ENUMF_SECTOR
    /// filter; the host rejects other kinds as missing sector records.
    /// Success is the native I3DENUMRET_OK path (0).
    pub fn scene_make_ready_enum_proc(
        &mut self,
        frame: FrameKey,
        _context: u32,
    ) -> Result<u32, ModelError> {
        self.sector_make_ready(crate::scene_ownership::SectorKey(frame))?;
        Ok(0)
    }
    /// AddLight (PC RVA0x4f3d0), including the native fog-light selection.
    /// These arrays contain borrowed pointers; registration acquires no refs.
    pub fn sector_add_light(
        &mut self,
        sector: SectorKey,
        light: FrameKey,
    ) -> Result<(), ModelError> {
        self.sector_add_light_with_reserve(sector, light, |list| {
            list.try_reserve(1)
                .map_err(|_| ModelError::AllocationOverflow)
        })
    }
    fn sector_add_light_with_reserve(
        &mut self,
        sector: SectorKey,
        light: FrameKey,
        mut reserve: impl FnMut(&mut Vec<FrameKey>) -> Result<(), ModelError>,
    ) -> Result<(), ModelError> {
        self.sector(sector)?;
        self.light(light)?;
        if let Some(interrupted) = self
            .sectors
            .iter()
            .find(|entry| entry.pending_light_add.is_some())
        {
            let pending = interrupted.pending_light_add.unwrap();
            if interrupted.key != sector || pending.light != light {
                return Err(SceneOwnerError::LightAddIncomplete {
                    sector: interrupted.key,
                    light: pending.light,
                }
                .into());
            }
        }
        let si = self.sectors.iter().position(|s| s.key == sector).unwrap();
        if let Some(pending) = self.sectors[si].pending_light_add {
            if pending.light != light
                || pending.light_type != self.graph.light_state(light)?.light_type
                || pending.on != (self.state(light)?.flags & 1 != 0)
            {
                return Err(SceneOwnerError::LightAddIncomplete {
                    sector,
                    light: pending.light,
                }
                .into());
            }
        } else {
            if self.sectors[si].lights.contains(&light) {
                return Ok(());
            }
            // Native AddLight changes fog selection and lights_changed before
            // either vector push. Retain that exact prefix across host
            // allocation interruption, without replaying it on retry.
            let light_type = self.graph.light_state(light)?.light_type;
            let on = self.state(light)?.flags & 1 != 0;
            if (5..=7).contains(&light_type) {
                self.sectors[si].num_fog_lights = self.sectors[si].num_fog_lights.wrapping_add(1);
                if on {
                    self.sectors[si].fog_light = Some(light);
                }
            }
            let changed = self
                .graph
                .sector_light_changes
                .entry(sector.frame().0)
                .or_default();
            *changed = changed.wrapping_add(1);
            self.sectors[si].pending_light_add = Some(PendingLightAdd {
                light,
                primary_appended: false,
                light_type,
                on,
            });
            self.graph.light_state_mut(light)?.pending_sector_add = true;
        }
        if !self.sectors[si].pending_light_add.unwrap().primary_appended {
            reserve(&mut self.sectors[si].lights)?;
            self.sectors[si].lights.push(light);
            self.sectors[si]
                .pending_light_add
                .as_mut()
                .unwrap()
                .primary_appended = true;
        }
        if !self
            .graph
            .light_state(light)?
            .sectors
            .contains(&sector.frame())
        {
            reserve(&mut self.graph.light_state_mut(light)?.sectors)?;
            self.graph
                .light_state_mut(light)?
                .sectors
                .push(sector.frame());
        }
        self.sectors[si].pending_light_add = None;
        self.graph.light_state_mut(light)?.pending_sector_add = false;
        Ok(())
    }
    pub fn sector_delete_light(
        &mut self,
        sector: SectorKey,
        light: FrameKey,
    ) -> Result<u32, ModelError> {
        self.sector(sector)?;
        self.graph.light_state(light)?;
        let si = self.sectors.iter().position(|s| s.key == sector).unwrap();
        if let Some(pending) = self.sectors[si].pending_light_add {
            return Err(SceneOwnerError::LightAddIncomplete {
                sector,
                light: pending.light,
            }
            .into());
        }
        let Some(index) = self.sectors[si]
            .lights
            .iter()
            .position(|&frame| frame == light)
        else {
            return Ok(0x80000006);
        };
        if (5..=7).contains(&self.graph.light_state(light)?.light_type) {
            self.sectors[si].num_fog_lights = self.sectors[si].num_fog_lights.wrapping_sub(1);
            self.sectors[si].fog_light = None;
            if self.sectors[si].num_fog_lights != 0 {
                // The original scans before removing the departing member.
                for &candidate in &self.sectors[si].lights {
                    if self.state(candidate)?.flags & 1 != 0
                        && (5..=7).contains(&self.graph.light_state(candidate)?.light_type)
                    {
                        self.sectors[si].fog_light = Some(candidate);
                        break;
                    }
                }
            }
        }
        let changed = self
            .graph
            .sector_light_changes
            .entry(sector.frame().0)
            .or_default();
        *changed = changed.wrapping_add(1);
        let sectors = &mut self.graph.light_state_mut(light)?.sectors;
        if let Some(index) = sectors.iter().position(|&frame| frame == sector.frame()) {
            sectors.swap_remove(index);
        }
        self.sectors[si].lights.swap_remove(index);
        Ok(0)
    }
    /// Fog-select arm of light UpdateV/SetOn (PC RVAs 0x20d10/0x21400,
    /// mafia-re I3d_light.cpp:224-231): ON selects this light as the sector
    /// fog light; turning off unlinks and reselects the first ON fog-kind
    /// light in sector order. Runs after the graph caches (native UpdateV
    /// inline order, split across owners here); rereads the live sector
    /// list. Non-fog lights are a guarded no-op (host admission — native only
    /// reaches this arm for fog kinds).
    pub fn light_update_fog_selection(
        &mut self,
        light: FrameKey,
        on: bool,
    ) -> Result<(), ModelError> {
        if !crate::frame_light::light_is_fog_kind(
            self.graph
                .light_state(light)
                .map_err(ModelError::from)?
                .light_type,
        ) {
            return Ok(());
        }
        let sectors: Vec<SectorKey> = self
            .graph
            .light_state(light)
            .map_err(ModelError::from)?
            .sectors
            .iter()
            .map(|&frame| SectorKey(frame))
            .collect();
        for sector in sectors {
            let si = self
                .sectors
                .iter()
                .position(|entry| entry.key == sector)
                .ok_or(SceneOwnerError::UnknownSector(sector))?;
            if on {
                self.sectors[si].fog_light = Some(light);
            } else if self.sectors[si].fog_light == Some(light) {
                self.sectors[si].fog_light = None;
                let order = self.sectors[si].lights.clone();
                for candidate in order {
                    if self.state(candidate)?.flags & 1 != 0
                        && crate::frame_light::light_is_fog_kind(
                            self.graph
                                .light_state(candidate)
                                .map_err(ModelError::from)?
                                .light_type,
                        )
                    {
                        self.sectors[si].fog_light = Some(candidate);
                        break;
                    }
                }
            }
        }
        Ok(())
    }
    pub(crate) fn release_light_sectors(&mut self, light: FrameKey) -> Result<(), ModelError> {
        if self.state(light)?.kind != FrameKind::Light {
            return Ok(());
        }
        if let Some(sector) = self.sectors.iter().find(|sector| {
            sector
                .pending_light_add
                .is_some_and(|pending| pending.light == light)
        }) {
            return Err(SceneOwnerError::LightAddIncomplete {
                sector: sector.key,
                light,
            }
            .into());
        }
        // Actual 20760 snapshots the initial pointer/count, then advances the
        // physical pointer despite reciprocal swap-removals. The admitted lists
        // contain unique sector references, so this preserves initial order.
        let sectors = self.graph.light_state(light)?.sectors.clone();
        for sector in sectors {
            self.sector_delete_light(SectorKey(sector), light)?;
        }
        self.graph.light_state_mut(light)?.sectors = Vec::new();
        Ok(())
    }
    /// GetSector's constructor-only spatial-list domain. A loaded spatial
    /// sector requires its real CheckPoint geometry before position selection.
    pub fn scene_sector_at_position(
        &self,
        scene: SceneKey,
        _position: [f32; 3],
    ) -> Result<Option<SectorKey>, ModelError> {
        let owner = self.scene(scene)?;
        if self
            .sectors
            .iter()
            .any(|sector| sector.scene == Some(scene) && !owner.sectors.contains(&sector.key))
        {
            return Err(SceneOwnerError::SectorGeometryUnavailable(scene).into());
        }
        Ok(Some(owner.sectors[0]))
    }
    /// GetLightness's reached empty-list branch, using GetSector's actual
    /// primary fallback. Loaded spatial sectors and nonempty lighting remain
    /// explicit dependencies rather than an invented lightness result.
    pub fn scene_empty_lightness(
        &self,
        scene: SceneKey,
        _position: [f32; 3],
    ) -> Result<f32, ModelError> {
        let owner = self.scene(scene)?;
        if self
            .sectors
            .iter()
            .any(|sector| sector.scene == Some(scene) && !owner.sectors.contains(&sector.key))
        {
            return Err(SceneOwnerError::SectorGeometryUnavailable(scene).into());
        }
        let primary = owner.sectors[0];
        if !self.sector(primary)?.lights.is_empty() {
            return Err(SceneOwnerError::LightnessUnavailable(primary).into());
        }
        Ok(0.)
    }
    /// Original SetFrameSectorPos (PC RVA0x4a280), using the same loaded-sector
    /// CheckPoint/GetSector producer as the camera and portal interconnection.
    pub fn set_frame_sector_pos(
        &mut self,
        scene: SceneKey,
        frame: FrameKey,
        position: [f32; 3],
    ) -> Result<u32, ModelError> {
        self.scene_index(scene)?;
        self.frame(frame)?;
        let mut ancestor = self.links(frame)?.parent;
        while let Some(key) = ancestor {
            if self.state(key)?.kind == FrameKind::Sector {
                let flags = self.sector(SectorKey(key))?.sector_flags;
                if flags & (0x1000 | 0x2000) != 0 {
                    return Ok(0);
                }
                break;
            }
            ancestor = self.links(key)?.parent;
        }
        // The native call passes NULL, not the old ancestor as a spatial hint.
        let selected = self.scene_get_sector(scene, position, None)?.frame();
        if ancestor == Some(selected) {
            return Ok(0);
        }
        self.link_frame(frame, Some(selected), 1)
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
    fn sector_light_registration_is_reciprocal_nonowning_and_invalidated_by_setters() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let light = arena.create_light().unwrap();
        let references = arena.frame(light).unwrap().references;
        arena.sector_add_light(sectors[0], light).unwrap();
        arena.sector_add_light(sectors[1], light).unwrap();
        arena.sector_add_light(sectors[0], light).unwrap();
        assert_eq!(arena.sector(sectors[0]).unwrap().lights, [light]);
        assert_eq!(
            arena.light(light).unwrap().sectors,
            [sectors[0].frame(), sectors[1].frame()]
        );
        assert_eq!(arena.frame(light).unwrap().references, references);
        assert_eq!(arena.sector_lights_changed(sectors[0]), Ok(1));
        assert!(arena.scene_empty_lightness(scene, [0.; 3]).is_err());
        arena.graph.set_light_diffuse_power(light, 0.5).unwrap();
        assert_eq!(arena.sector_lights_changed(sectors[0]), Ok(2));
        assert_eq!(arena.sector_delete_light(sectors[0], light), Ok(0));
        assert_eq!(arena.sector_delete_light(sectors[0], light), Ok(0x80000006));
        assert_eq!(arena.scene_empty_lightness(scene, [0.; 3]), Ok(0.));
        arena.release(light).unwrap();
        assert!(arena.sector(sectors[1]).unwrap().lights.is_empty());
        assert_eq!(arena.sector_lights_changed(sectors[1]), Ok(3));
    }
    #[test]
    fn scene_sector_slots_and_change_flags_read_ctor_state() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        assert_eq!(arena.scene_primary_sector(scene), Ok(sectors[0]));
        assert_eq!(arena.scene_backdrop_sector(scene), Ok(sectors[1]));
        assert_eq!(arena.scene_omni_sector(scene), Ok(sectors[2]));
        assert_eq!(arena.scene_near_sector(scene), Ok(sectors[3]));
        assert_eq!(arena.scene_occluders_changed(scene), Ok(false));
        assert_eq!(arena.scene_areas_changed(scene), Ok(false));
    }
    #[test]
    fn scene_owned_list_words_read_ctor_state_and_add_prefix() {
        // Working names for the +0x22c owned-frame C_list words
        // (mafia-re ls3df/I3D/I3D.h:1569): no LS3DF FUNCTION addresses
        // exist for standalone readers; only AddFrame/DeleteFrame do.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_owned_frame_count(scene), Ok(0));
        assert_eq!(arena.scene_owned_capacity_of(scene), Ok(0));
        assert_eq!(arena.scene_owned_growth_of(scene), Ok(32));
        let frame = arena
            .insert(I3dFrameState::dummy(), None, Vec::new(), Vec::new(), None)
            .unwrap();
        arena.scene_add_frame(scene, frame).unwrap();
        assert_eq!(arena.scene_owned_frame_count(scene), Ok(1));
        assert_eq!(arena.scene_owned_capacity_of(scene), Ok(32));
        assert_eq!(arena.scene_owned_growth_of(scene), Ok(32));
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_owned_frame_count(bad).is_err());
        assert!(arena.scene_owned_capacity_of(bad).is_err());
        assert!(arena.scene_owned_growth_of(bad).is_err());
    }
    #[test]
    fn scene_empty_vfunc_66_and_sector_head_read_ctor_state() {
        // Working names for the in-class empty vfunc_66 (I3D.h:1529-1530)
        // and the sector ring head at +0x264: no LS3DF FUNCTION entries.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        arena.scene_vfunc_66_noop(scene, 1, 2).unwrap();
        arena.scene_vfunc_66_noop(scene, u32::MAX, u32::MAX).unwrap();
        assert_eq!(arena.scene_sector_head(scene), Ok(None));
        let sector = arena.create_sector_frame().unwrap();
        arena.sector_add_to_scene_ring(sector, Some(scene)).unwrap();
        assert_eq!(arena.scene_sector_head(scene), Ok(Some(sector)));
        assert!(arena
            .scene_sector_head(SceneKey(FrameKey(u32::MAX)))
            .is_err());
        assert!(arena.scene_vfunc_66_noop(SceneKey(FrameKey(u32::MAX)), 0, 0).is_err());
    }
    #[test]
    fn fog_sector_membership_selects_latest_on_light_and_releases_reciprocals() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        let first = arena.create_light().unwrap();
        let second = arena.create_light().unwrap();
        arena.graph.set_light_type(first, 5).unwrap();
        arena.graph.set_light_type(second, 6).unwrap();
        arena.sector_add_light(primary, first).unwrap();
        arena.sector_add_light(primary, first).unwrap();
        arena.sector_add_light(primary, second).unwrap();
        let sector = arena.sector(primary).unwrap();
        assert_eq!(sector.lights, [first, second]);
        assert_eq!(sector.num_fog_lights, 2);
        assert_eq!(sector.fog_light, Some(second));
        assert_eq!(arena.light(first).unwrap().sectors, [primary.frame()]);
        assert_eq!(arena.light(second).unwrap().sectors, [primary.frame()]);
        assert!(arena.graph.update_v(second).is_err());
        arena.set_frame_on(second, 0).unwrap();
        arena.update_frame(second).unwrap();
        assert_eq!(arena.sector(primary).unwrap().fog_light, Some(first));
        arena.set_frame_on(second, 1).unwrap();
        arena.update_frame(second).unwrap();
        assert_eq!(arena.sector(primary).unwrap().fog_light, Some(second));
        assert_eq!(arena.sector_delete_light(primary, second), Ok(0));
        let sector = arena.sector(primary).unwrap();
        assert_eq!(sector.lights, [first]);
        assert_eq!(sector.num_fog_lights, 1);
        assert_eq!(sector.fog_light, Some(first));
        arena.release(first).unwrap();
        let sector = arena.sector(primary).unwrap();
        assert!(sector.lights.is_empty());
        assert_eq!(sector.num_fog_lights, 0);
        assert_eq!(sector.fog_light, None);
        arena.release(second).unwrap();
    }
    #[test]
    fn fog_add_light_second_reservation_failure_retains_ordered_prefix() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        let backdrop = arena.scene(scene).unwrap().sectors[1];
        let light = arena.create_light().unwrap();
        arena.graph.set_light_type(light, 5).unwrap();
        let mut reservations = 0;
        assert!(matches!(
            arena.sector_add_light_with_reserve(primary, light, |_| {
                reservations += 1;
                if reservations == 2 {
                    Err(ModelError::AllocationOverflow)
                } else {
                    Ok(())
                }
            }),
            Err(ModelError::AllocationOverflow)
        ));
        assert_eq!(reservations, 2);
        let sector = arena.sector(primary).unwrap();
        assert_eq!(sector.lights, [light]);
        assert_eq!(sector.num_fog_lights, 1);
        assert_eq!(sector.fog_light, Some(light));
        assert!(sector.pending_light_add.unwrap().primary_appended);
        assert!(arena.graph.light_state(light).unwrap().pending_sector_add);
        assert!(arena.light(light).unwrap().sectors.is_empty());
        assert_eq!(arena.sector_lights_changed(primary), Ok(1));
        assert!(arena.graph.update_v(light).is_err());
        assert!(arena.update_frame(light).is_err());
        assert!(matches!(
            arena.release(light),
            Err(ModelError::Scene(
                SceneOwnerError::LightAddIncomplete { .. }
            ))
        ));
        let other = arena.create_light().unwrap();
        assert!(matches!(
            arena.sector_add_light(primary, other),
            Err(ModelError::Scene(
                SceneOwnerError::LightAddIncomplete { .. }
            ))
        ));
        assert!(matches!(
            arena.sector_add_light(backdrop, other),
            Err(ModelError::Scene(
                SceneOwnerError::LightAddIncomplete { .. }
            ))
        ));
        arena.set_frame_on(light, 0).unwrap();
        assert!(matches!(
            arena.sector_add_light(primary, light),
            Err(ModelError::Scene(
                SceneOwnerError::LightAddIncomplete { .. }
            ))
        ));
        arena.set_frame_on(light, 1).unwrap();
        arena.graph.set_light_type(light, 4).unwrap();
        assert!(matches!(
            arena.sector_add_light(primary, light),
            Err(ModelError::Scene(
                SceneOwnerError::LightAddIncomplete { .. }
            ))
        ));
        arena.graph.set_light_type(light, 5).unwrap();
        arena.sector_add_light(primary, light).unwrap();
        assert_eq!(arena.sector(primary).unwrap().num_fog_lights, 1);
        assert_eq!(arena.sector_lights_changed(primary), Ok(1));
        assert_eq!(arena.sector(primary).unwrap().pending_light_add, None);
        assert!(!arena.graph.light_state(light).unwrap().pending_sector_add);
        assert_eq!(arena.light(light).unwrap().sectors, [primary.frame()]);
        arena.update_frame(light).unwrap();
        arena.release(light).unwrap();
        arena.release(other).unwrap();
    }
    #[test]
    fn fog_first_reservation_failure_blocks_other_sector_light_update() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        let other = arena.create_light().unwrap();
        let pending = arena.create_light().unwrap();
        arena.graph.set_light_type(other, 5).unwrap();
        arena.graph.set_light_type(pending, 5).unwrap();
        arena.sector_add_light(primary, other).unwrap();
        assert!(matches!(
            arena.sector_add_light_with_reserve(primary, pending, |_| {
                Err(ModelError::AllocationOverflow)
            }),
            Err(ModelError::AllocationOverflow)
        ));
        let sector = arena.sector(primary).unwrap();
        assert_eq!(sector.lights, [other]);
        assert_eq!(sector.num_fog_lights, 2);
        assert_eq!(sector.fog_light, Some(pending));
        assert!(!sector.pending_light_add.unwrap().primary_appended);
        arena.set_frame_on(other, 0).unwrap();
        assert!(matches!(
            arena.update_frame(other),
            Err(ModelError::Scene(
                SceneOwnerError::LightAddIncomplete { .. }
            ))
        ));
        assert!(arena.graph.update_v(other).is_err());
        arena.sector_add_light(primary, pending).unwrap();
        arena.update_frame(other).unwrap();
        assert_eq!(arena.sector(primary).unwrap().fog_light, Some(pending));
        arena.release(other).unwrap();
        arena.release(pending).unwrap();
        assert_eq!(arena.sector(primary).unwrap().num_fog_lights, 0);
        assert_eq!(arena.sector(primary).unwrap().fog_light, None);
    }

    #[test]
    fn empty_spatial_sector_search_reparents_to_primary_and_retains_backdrop() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let model = arena.create_model().unwrap();
        arena
            .graph
            .projected_state_mut(model.root())
            .unwrap()
            .local
            .elements[12] = 3.;
        let references = arena.frame(model.root()).unwrap().references;
        assert_eq!(
            arena.set_frame_sector_pos(scene, model.root(), [3., 2., 1.]),
            Ok(0)
        );
        assert_eq!(
            arena.links(model.root()).unwrap().parent,
            Some(sectors[0].frame())
        );
        assert_eq!(arena.frame(model.root()).unwrap().references, references);
        arena
            .link_frame(model.root(), Some(sectors[1].frame()), 0)
            .unwrap();
        assert_eq!(
            arena.set_frame_sector_pos(scene, model.root(), [100., 200., 300.]),
            Ok(0)
        );
        assert_eq!(
            arena.links(model.root()).unwrap().parent,
            Some(sectors[1].frame())
        );
    }
    #[test]
    fn loaded_hull_target_relinks_preserving_world_transform_and_special_sector_guard() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let a = crate::scene_lightness::loaded_sector_fixture(&mut arena, scene, [0.; 3]);
            let b = crate::scene_lightness::loaded_sector_fixture(&mut arena, scene, [10., 0., 0.]);
            let model = arena.create_model().unwrap();
            arena
                .graph
                .assign_position(model.root(), [2., 3., 4.])
                .unwrap();
            let original = arena.world_matrix(model.root()).unwrap();
            for (position, target) in [
                ([0.; 3], a),
                ([10., 0., 0.], b),
                ([30., 0., 0.], arena.scene(scene).unwrap().sectors[0]),
            ] {
                assert_eq!(
                    arena
                        .set_frame_sector_pos(scene, model.root(), position)
                        .unwrap(),
                    0
                );
                assert_eq!(
                    arena.links(model.root()).unwrap().parent,
                    Some(target.frame())
                );
                assert_eq!(arena.world_matrix(model.root()).unwrap(), original);
            }
            let special = arena.scene(scene).unwrap().sectors;
            for retained in [special[1], special[2]] {
                arena
                    .link_frame(model.root(), Some(retained.frame()), 1)
                    .unwrap();
                // Source returns before attempting any spatial arithmetic here.
                assert_eq!(
                    arena
                        .set_frame_sector_pos(scene, model.root(), [f32::NAN; 3])
                        .unwrap(),
                    0
                );
                assert_eq!(
                    arena.links(model.root()).unwrap().parent,
                    Some(retained.frame())
                );
            }
        });
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
            assert_eq!(arena.sector(sectors[i]).unwrap().scene, Some(scene));
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
    fn lookup_and_sector_ring_follow_a_real_cross_scene_transition() {
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
        arena.link_frame(sb, Some(sa), 0).unwrap();
        assert_eq!(arena.sector(SectorKey(sb)).unwrap().scene, Some(a));
        let ai = arena.scene_index(a).unwrap();
        let bi = arena.scene_index(b).unwrap();
        assert_eq!(arena.scenes[ai].sector_head, Some(SectorKey(sb)));
        assert_eq!(arena.scenes[ai].sector_count, 1);
        assert_eq!(arena.scenes[bi].sector_count, 0);
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
    #[test]
    fn tick_add_is_unique_ordered_and_sets_flag_after_store() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(
            (
                arena.scene(scene).unwrap().tick_capacity,
                arena.scene(scene).unwrap().tick_growth
            ),
            (0, 32)
        );
        let a = dummy(&mut arena, b"tick-a");
        let b = dummy(&mut arena, b"tick-b");
        arena.scene_add_tick_frame(scene, a).unwrap();
        arena.scene_add_tick_frame(scene, b).unwrap();
        arena.scene_add_tick_frame(scene, a).unwrap();
        assert_eq!(arena.scene(scene).unwrap().tick_frames, [a, b]);
        assert_eq!(arena.scene(scene).unwrap().tick_capacity, 32);
        for key in [a, b] {
            assert_ne!(arena.state(key).unwrap().flags & SCENE_TICK_FLAG, 0);
        }
        assert_eq!(arena.scene_delete_tick_frame(scene, a), Ok(0));
        assert_eq!(arena.scene(scene).unwrap().tick_frames, [b]);
        assert_eq!(arena.state(a).unwrap().flags & SCENE_TICK_FLAG, 0);
        assert_ne!(arena.state(b).unwrap().flags & SCENE_TICK_FLAG, 0);
        assert_eq!(
            arena.scene_delete_tick_frame(scene, a),
            Ok(SCENE_OBJECT_NOT_FOUND)
        );
    }
    #[test]
    fn tick_reserve_failure_leaves_no_flag_or_membership_residue() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let a = dummy(&mut arena, b"tick-oom");
        assert!(arena
            .scene_add_tick_frame_with_reserve(scene, a, |_| Err(ModelError::AllocationOverflow))
            .is_err());
        assert!(arena.scene(scene).unwrap().tick_frames.is_empty());
        assert_eq!(arena.state(a).unwrap().flags & SCENE_TICK_FLAG, 0);
        // Capacity was charged before the failing reserve (native unchecked
        // growth); retry with a working allocator still succeeds, and the
        // duplicate add after success is a no-op re-OR.
        arena.scene_add_tick_frame(scene, a).unwrap();
        arena.scene_add_tick_frame(scene, a).unwrap();
        assert_eq!(arena.scene(scene).unwrap().tick_frames, [a]);
    }
    #[test]
    fn delete_frame_word_clears_tick_untracks_and_returns_words() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let member = dummy(&mut arena, b"owned-tick");
        arena.scene_add_frame(scene, member).unwrap();
        arena.scene_add_tick_frame(scene, member).unwrap();
        assert_eq!(arena.scene_delete_frame_word(scene, member), Ok(SCENE_OK));
        assert!(arena.scene(scene).unwrap().frames().is_empty());
        assert!(arena.scene(scene).unwrap().tick_frames.is_empty());
        assert_eq!(arena.state(member).unwrap().flags & SCENE_TICK_FLAG, 0);
        assert_eq!(
            arena.scene_delete_frame_word(scene, member),
            Ok(SCENE_OBJECT_NOT_FOUND)
        );
        // Membership precedes liveness: a destroyed non-member yields the
        // miss word, never Err.
        let dead = dummy(&mut arena, b"dead");
        arena.release(dead).unwrap();
        assert_eq!(
            arena.scene_delete_frame_word(scene, dead),
            Ok(SCENE_OBJECT_NOT_FOUND)
        );
    }
    #[test]
    fn callback_store_null_forces_flags_and_returns_old() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_callback(scene).unwrap(), (None, 0));
        assert_eq!(
            arena
                .scene_set_callback(scene, Some(SceneCallback(0x1234)), 7)
                .unwrap(),
            None
        );
        assert_eq!(
            arena.scene_callback(scene).unwrap(),
            (Some(SceneCallback(0x1234)), 7)
        );
        assert_eq!(
            arena.scene_set_callback(scene, None, 99).unwrap(),
            Some(SceneCallback(0x1234))
        );
        assert_eq!(arena.scene_callback(scene).unwrap(), (None, 0));
    }
    #[test]
    fn scene_active_camera_reports_live_handle_and_rejects_unknown_scene() {
        // Working names for inlined I3D_scene::GetActiveCamera (I3D.h:1548,
        // defined at I3D.h:5085-5089) and GetPrimarySector (I3D.h:1549): no
        // LS3DF FUNCTION addresses exist (the reference marks the only
        // out-of-line copies as the game's, unclaimed here).
        struct Screen;
        impl crate::scene_resources::SceneScreen for Screen {
            type Error = ();
            fn screen_sx(&mut self) -> Result<i32, ()> {
                Ok(800)
            }
            fn screen_sy(&mut self) -> Result<i32, ()> {
                Ok(600)
            }
        }
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            assert_eq!(arena.scene_active_camera(scene).unwrap(), None);
            let camera = arena.create_camera(&mut Screen).unwrap();
            let si = arena.scene_index(scene).unwrap();
            arena.scenes[si].current_camera = Some(camera);
            assert_eq!(arena.scene_active_camera(scene).unwrap(), Some(camera));
            assert!(arena
                .scene_active_camera(SceneKey(FrameKey(u32::MAX)))
                .is_err());
            let sectors = arena.scene(scene).unwrap().sectors;
            assert_eq!(arena.scene_primary_sector(scene).unwrap(), sectors[0]);
            assert_eq!(
                arena.sector(sectors[0]).unwrap().sector_flags & 0x200,
                0x200
            );
            assert!(arena
                .scene_primary_sector(SceneKey(FrameKey(u32::MAX)))
                .is_err());
        });
    }
    #[test]
    fn scene_empty_setters_and_duplicate_are_noop_ok() {
        // Working names for in-class empty I3D_scene::SetOn (I3D.h:1454) and
        // I3D_scene::SetCallback I3D_CALLBACK overload (I3D.h:1453), plus
        // I3D_scene::Duplicate returning I3D_OK (I3D.h:1458): no LS3DF
        // FUNCTION addresses and no Game.exe copies exist for any of them.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        arena.scene_set_on_noop(scene, true).unwrap();
        arena.scene_set_on_noop(scene, false).unwrap();
        arena.scene_frame_callback_noop(scene, None, 0).unwrap();
        arena
            .scene_frame_callback_noop(scene, Some(0x1234), 0x1e00)
            .unwrap();
        assert_eq!(arena.scene_duplicate_ok(scene, None).unwrap(), 0);
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_set_on_noop(bad, true).is_err());
        assert!(arena.scene_frame_callback_noop(bad, None, 0).is_err());
        assert!(arena.scene_duplicate_ok(bad, None).is_err());
    }
    #[test]
    fn scene_set_frame_sector_uses_live_world_position() {
        // Working name for inlined I3D_scene::SetFrameSector (I3D.h:5096-5100):
        // no LS3DF FUNCTION address exists (only Game.exe 0x47adf0,
        // unclaimed here). Composes GetWorldPos with SetFrameSectorPos.
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let member = dummy(&mut arena, b"member");
            arena.scene_add_frame(scene, member).unwrap();
            assert_eq!(arena.scene_set_frame_sector(scene, member).unwrap(), 0);
            assert!(arena
                .scene_set_frame_sector(SceneKey(FrameKey(u32::MAX)), member)
                .is_err());
            assert!(arena
                .scene_set_frame_sector(scene, FrameKey(u32::MAX))
                .is_err());
        });
    }
    #[test]
    fn working_name_tick_frame_count_tracks_add_and_delete() {
        // Working name, no LS3DF FUNCTION: C_list count at +0x148.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_tick_frame_count(scene).unwrap(), 0);
        let member = dummy(&mut arena, b"tick-count");
        arena.scene_add_frame(scene, member).unwrap();
        arena.scene_add_tick_frame(scene, member).unwrap();
        assert_eq!(arena.scene_tick_frame_count(scene).unwrap(), 1);
        arena.scene_delete_tick_frame(scene, member).unwrap();
        assert_eq!(arena.scene_tick_frame_count(scene).unwrap(), 0);
        assert!(arena
            .scene_tick_frame_count(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_shadow_count_tracks_add_and_delete() {
        // Working name, no LS3DF FUNCTION: shadow list count at +0x158.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_shadow_count(scene).unwrap(), 0);
        let member = dummy(&mut arena, b"shadow-count");
        arena.scene_add_shadow(scene, member).unwrap();
        assert_eq!(arena.scene_shadow_count(scene).unwrap(), 1);
        arena.scene_delete_shadow(scene, member).unwrap();
        assert_eq!(arena.scene_shadow_count(scene).unwrap(), 0);
        assert!(arena
            .scene_shadow_count(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_mirror_count_tracks_add_and_delete() {
        // Working name, no LS3DF FUNCTION: mirror list count at +0x168.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_mirror_count(scene).unwrap(), 0);
        let member = dummy(&mut arena, b"mirror-count");
        arena.scene_add_mirror(scene, member).unwrap();
        assert_eq!(arena.scene_mirror_count(scene).unwrap(), 1);
        arena.scene_delete_mirror(scene, member).unwrap();
        assert_eq!(arena.scene_mirror_count(scene).unwrap(), 0);
        assert!(arena
            .scene_mirror_count(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_scene_lists_and_portal_head_read_live_links() {
        // Working names, no LS3DF FUNCTION: portals head at +0x258 and the
        // tick/shadow/mirror lists at +0x148/+0x158/+0x168 (I3D.h:1554-1556,
        // 1572-1573). The constructor leaves the head empty and the lists
        // borrowed empty; add/delete paths move both.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_portal_head(scene).unwrap(), None);
        assert!(arena.scene_tick_frames(scene).unwrap().is_empty());
        assert!(arena.scene_shadows(scene).unwrap().is_empty());
        assert!(arena.scene_mirrors(scene).unwrap().is_empty());
        let member = dummy(&mut arena, b"scene-lists");
        arena.scene_add_frame(scene, member).unwrap();
        arena.scene_add_tick_frame(scene, member).unwrap();
        arena.scene_add_shadow(scene, member).unwrap();
        arena.scene_add_mirror(scene, member).unwrap();
        assert_eq!(arena.scene_tick_frames(scene).unwrap(), &[member][..]);
        assert_eq!(arena.scene_shadows(scene).unwrap(), &[member][..]);
        assert_eq!(arena.scene_mirrors(scene).unwrap(), &[member][..]);
        arena.scene_delete_tick_frame(scene, member).unwrap();
        arena.scene_delete_shadow(scene, member).unwrap();
        arena.scene_delete_mirror(scene, member).unwrap();
        assert!(arena.scene_tick_frames(scene).unwrap().is_empty());
        assert!(arena.scene_shadows(scene).unwrap().is_empty());
        assert!(arena.scene_mirrors(scene).unwrap().is_empty());
        let unknown = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_portal_head(unknown).is_err());
        assert!(arena.scene_tick_frames(unknown).is_err());
        assert!(arena.scene_shadows(unknown).is_err());
        assert!(arena.scene_mirrors(unknown).is_err());
    }
    #[test]
    fn working_name_occluder_count_reads_ctor_zero() {
        // Working name, no LS3DF FUNCTION: num_occluders at +0x26c.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_occluder_count(scene).unwrap(), 0);
        assert!(arena
            .scene_occluder_count(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_area_count_reads_ctor_zero() {
        // Working name, no LS3DF FUNCTION: num_areas at +0x278.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_area_count(scene).unwrap(), 0);
        assert!(arena
            .scene_area_count(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_scene_flag_words_report_ctor_defaults() {
        // Working names, no LS3DF FUNCTION addresses: bg_color at +0x220
        // (0.0, 0.5, 0.4 bits), portal_version at +0x25c, tick
        // capacity/growth at +0x148.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(
            arena.scene_bg_color(scene).unwrap(),
            [0, 0x3f000000, 0x3ecccccd]
        );
        assert_eq!(arena.scene_portal_version(scene).unwrap(), 0);
        assert_eq!(arena.scene_tick_capacity(scene).unwrap(), 0);
        assert_eq!(arena.scene_tick_growth(scene).unwrap(), 32);
        assert_eq!(arena.scene_area_head(scene).unwrap(), None);
        assert_eq!(arena.scene_occluder_head(scene).unwrap(), None);
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_bg_color(bad).is_err());
        assert!(arena.scene_portal_version(bad).is_err());
        assert!(arena.scene_tick_capacity(bad).is_err());
        assert!(arena.scene_tick_growth(bad).is_err());
        assert!(arena.scene_area_head(bad).is_err());
        assert!(arena.scene_occluder_head(bad).is_err());
    }
    #[test]
    fn make_ready_enum_proc_marks_sector_ready() {
        // Same native as LS3DF RVA 0x4a8c0 (already credited to
        // frame_sector::make_ready_sectors_enum_proc): no new credit here.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        assert_eq!(arena.scene_make_ready_enum_proc(sector.frame(), 0).unwrap(), 0);
        assert_eq!(arena.scene_make_ready_enum_proc(sector.frame(), 999).unwrap(), 0);
        let member = dummy(&mut arena, b"not-a-sector");
        assert!(arena.scene_make_ready_enum_proc(member, 0).is_err());
    }
    #[test]
    fn scene_find_frame_rva_0x49210_primary_then_gated_sectors() {
        // PC RVA 0x49210 (VA 0x10049210; REA formed Ghidra body FUN_10049210,
        // ev_f460b45f...; mafia-re I3d_scene.cpp:226-249): the primary
        // sector first, then the ENUMF-gated backdrop/omni/near sectors;
        // the first hit across the chain wins and ungated sectors stay
        // unvisited. A null mask matches the primary sector itself, like
        // the native NULL-name path through GetChildFrame.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let primary_hit = dummy(&mut arena, b"alpha");
        arena
            .link_frame(primary_hit, Some(sectors[0].frame()), 0)
            .unwrap();
        let backdrop_hit = dummy(&mut arena, b"beta");
        arena
            .link_frame(backdrop_hit, Some(sectors[1].frame()), 0)
            .unwrap();
        let backdrop_alpha = dummy(&mut arena, b"alpha");
        arena
            .link_frame(backdrop_alpha, Some(sectors[1].frame()), 0)
            .unwrap();
        let omni_hit = dummy(&mut arena, b"gamma");
        arena
            .link_frame(omni_hit, Some(sectors[2].frame()), 0)
            .unwrap();
        let near_hit = dummy(&mut arena, b"delta");
        arena
            .link_frame(near_hit, Some(sectors[3].frame()), 0)
            .unwrap();
        // Primary first: beats the backdrop's duplicate name.
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"alpha"), 0xffff, 64),
            Ok(Some(primary_hit))
        );
        // Ungated sectors stay unvisited.
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"beta"), 0xffff, 64),
            Ok(None)
        );
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"beta"), 0xffff | 0x40000, 64),
            Ok(Some(backdrop_hit))
        );
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"gamma"), 0xffff, 64),
            Ok(None)
        );
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"gamma"), 0xffff | 0x80000, 64),
            Ok(Some(omni_hit))
        );
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"delta"), 0xffff, 64),
            Ok(None)
        );
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"delta"), 0xffff | 0x100000, 64),
            Ok(Some(near_hit))
        );
        // Misses stay misses; the null mask matches the primary sector.
        assert_eq!(
            arena.scene_find_frame(scene, Some(b"absent"), 0xffff | 0x1c0000, 64),
            Ok(None)
        );
        assert_eq!(
            arena.scene_find_frame(scene, None, 0xffff, 64),
            Ok(Some(sectors[0].frame()))
        );
        // Unknown scenes are explicit errors.
        assert_eq!(
            arena.scene_find_frame(
                SceneKey(FrameKey(u32::MAX)),
                Some(b"alpha"),
                0xffff,
                64
            ),
            Err(crate::frame_lookup::FrameLookupError::MissingPrimaryRoot)
        );
    }
    #[test]
    fn working_name_world_space_defaults_false_and_rejects_unknown() {
        // Working name, no LS3DF FUNCTION: per-scene projection of the
        // file-static SetSpaceParam (only the setter has RVA 0x4a8d0).
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_world_space(scene).unwrap(), false);
        assert!(arena
            .scene_world_space(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_occluder_flag_274_defaults_false() {
        // Working name, no LS3DF FUNCTION: unknown_274 at +0x274.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_occluder_flag_274(scene).unwrap(), false);
        assert!(arena
            .scene_occluder_flag_274(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_animation_set_absent_after_ctor() {
        // Working name, no LS3DF FUNCTION: ipol_set presence at +0x178.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_has_animation_set(scene).unwrap(), false);
        assert!(arena
            .scene_has_animation_set(SceneKey(FrameKey(u32::MAX)))
            .is_err());
    }
    #[test]
    fn working_name_ring_counts_and_backdrop_range_report_live_words() {
        // Working names for inlined I3D.h reads (+0x260/+0x254/+0x24c): no
        // LS3DF FUNCTION addresses exist for standalone getters.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_sector_count_of(scene).unwrap(), 0);
        assert_eq!(arena.scene_portal_count_of(scene).unwrap(), 0);
        // Constructor stores the native [1, 10000] backdrop pair.
        assert_eq!(
            arena.scene_backdrop_range_of(scene).unwrap().map(f32::to_bits),
            [1f32.to_bits(), 10000f32.to_bits()]
        );
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_sector_count_of(bad).is_err());
        assert!(arena.scene_portal_count_of(bad).is_err());
        assert!(arena.scene_backdrop_range_of(bad).is_err());
    }
    #[test]
    fn working_name_backdrop_near_reports_first_range_word() {
        // Working name for inlined backdrop_ncp read (I3D.h:1571): no LS3DF
        // FUNCTION address exists for a standalone getter.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_backdrop_near_of(scene).unwrap(), 1.);
        assert_eq!(
            arena.scene_backdrop_near_of(scene).unwrap().to_bits(),
            arena.scene_backdrop_range_of(scene).unwrap()[0].to_bits()
        );
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_backdrop_near_of(bad).is_err());
    }
    #[test]
    fn working_name_backdrop_far_reports_second_range_word() {
        // Working name for inlined backdrop_fcp read (I3D.h:1571): no LS3DF
        // FUNCTION address exists for a standalone getter.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_backdrop_far_of(scene).unwrap(), 10000.);
        assert_eq!(
            arena.scene_backdrop_far_of(scene).unwrap().to_bits(),
            arena.scene_backdrop_range_of(scene).unwrap()[1].to_bits()
        );
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_backdrop_far_of(bad).is_err());
    }
    #[test]
    fn working_name_callback_presence_tracks_set_callback() {
        // Working name for inlined callback presence read (I3D.h:1552): no
        // LS3DF FUNCTION address exists; only SetCallback (PC RVA 0x49120,
        // already occupied) writes it.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_has_callback(scene).unwrap(), false);
        arena.scene_set_callback(scene, Some(SceneCallback(0x1234)), 0x7).unwrap();
        assert_eq!(arena.scene_has_callback(scene).unwrap(), true);
        arena.scene_set_callback(scene, None, 0x7).unwrap();
        assert_eq!(arena.scene_has_callback(scene).unwrap(), false);
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_has_callback(bad).is_err());
    }
    #[test]
    fn working_name_callback_flags_track_set_callback() {
        // Working name for inlined cb_flags read (I3D.h:1553): no LS3DF
        // FUNCTION address exists; null forces zero like the native store.
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_callback_flags_of(scene).unwrap(), 0);
        arena.scene_set_callback(scene, Some(SceneCallback(0x1234)), 0x7).unwrap();
        assert_eq!(arena.scene_callback_flags_of(scene).unwrap(), 0x7);
        arena.scene_set_callback(scene, None, 0x7).unwrap();
        assert_eq!(arena.scene_callback_flags_of(scene).unwrap(), 0);
        let bad = SceneKey(FrameKey(u32::MAX));
        assert!(arena.scene_callback_flags_of(bad).is_err());
    }
    #[test]
    fn lifecycle_construct_tick_close_forget_transitions() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let owner = arena.scene(scene).unwrap();
        assert_eq!(
            (
                owner.tick_capacity,
                owner.tick_growth,
                owner.callback_flags,
                owner.world_space
            ),
            (0, 32, 0, false)
        );
        let member = dummy(&mut arena, b"life");
        arena.scene_add_frame(scene, member).unwrap();
        arena.scene_add_tick_frame(scene, member).unwrap();
        // Forget before Close is rejected (dtor runs after Close).
        assert!(arena.scene_forget_after_close(scene).is_err());
        // Camera detach with no camera is a no-op step.
        arena.scene_destructor_detach_camera(scene).unwrap();
        // Close clears the arena tick mirror with the frame walk.
        let mut effects = Effects::default();
        arena.close_scene_with_backend(scene, &mut effects).unwrap();
        assert!(arena.scene(scene).unwrap().tick_frames.is_empty());
        let sectors = arena.scene(scene).unwrap().sectors;
        arena.scene_forget_after_close(scene).unwrap();
        assert!(arena.scene(scene).is_err());
        // Sector records keep no dangling scene link after forgetting.
        for sector in sectors {
            assert_eq!(arena.sector(sector).unwrap().scene, None);
        }
    }
    #[test]
    fn shadow_and_mirror_roundtrips_are_idempotent_with_failure_words() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let (a, b, c) = (
            dummy(&mut arena, b"shadow-a"),
            dummy(&mut arena, b"shadow-b"),
            dummy(&mut arena, b"shadow-c"),
        );
        arena.scene_add_shadow(scene, a).unwrap();
        arena.scene_add_shadow(scene, a).unwrap();
        assert_eq!(arena.scene(scene).unwrap().shadows, [a]);
        assert_eq!(arena.scene_delete_shadow(scene, a), Ok(0));
        assert!(arena.scene(scene).unwrap().shadows.is_empty());
        assert_eq!(
            arena.scene_delete_shadow(scene, a),
            Ok(SCENE_OBJECT_NOT_FOUND)
        );
        for mirror in [a, b, c] {
            arena.scene_add_mirror(scene, mirror).unwrap();
        }
        assert_eq!(arena.scene_delete_mirror(scene, b), Ok(0));
        assert_eq!(arena.scene(scene).unwrap().mirrors, [a, c]);
        arena.scene_add_mirror(scene, b).unwrap();
        let absent = dummy(&mut arena, b"absent");
        assert_eq!(
            arena.scene_delete_mirror(scene, absent),
            Ok(SCENE_OBJECT_NOT_FOUND)
        );
    }
    struct SpaceMap {
        skip: Vec<FrameKey>,
        set: Vec<FrameKey>,
        calls: Vec<(FrameKey, bool)>,
        portals: Vec<crate::frame_portal::PortalKey>,
    }
    impl SceneSpaceBackend for SpaceMap {
        fn visit(
            &self,
            kind: crate::frame_transform::FrameKind,
            frame: FrameKey,
        ) -> SceneSpaceVisit {
            if self.skip.contains(&frame) {
                SceneSpaceVisit::SkipSubtree
            } else if self.set.contains(&frame) {
                SceneSpaceVisit::SetSpaceAndRecurse
            } else if kind == crate::frame_transform::FrameKind::Model {
                SceneSpaceVisit::SkipSubtree
            } else {
                SceneSpaceVisit::Recurse
            }
        }
        fn set_space(&mut self, frame: FrameKey, world: bool) {
            self.calls.push((frame, world));
        }
        fn update_portal(
            &mut self,
            _: &ModelArena,
            _: SceneKey,
            portal: crate::frame_portal::PortalKey,
        ) -> Result<(), ModelError> {
            self.portals.push(portal);
            Ok(())
        }
    }
    fn space_map() -> SpaceMap {
        SpaceMap {
            skip: Vec::new(),
            set: Vec::new(),
            calls: Vec::new(),
            portals: Vec::new(),
        }
    }
    #[test]
    fn set_space_skips_flagged_subtrees_and_applies_world_in_child_order() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let root = arena.scene(scene).unwrap().sectors[0].frame();
        let c1 = dummy(&mut arena, b"ss-c1");
        let c2 = dummy(&mut arena, b"ss-c2");
        arena.link_frame(c1, Some(root), 0).unwrap();
        arena.link_frame(c2, Some(root), 0).unwrap();
        // Native TARGET|INTERPOLATOR skip is traversal-owned: flag c1.
        arena
            .graph
            .change_flags(c1, 0, SCENE_SS_SKIP_FLAGS)
            .unwrap();
        let mut backend = space_map();
        backend.set = vec![c2];
        backend.skip = vec![c1];
        arena.scene_set_space(scene, true, &mut backend).unwrap();
        assert_eq!(backend.calls, [(c2, true)]);
        assert!(arena.scene(scene).unwrap().world_space);
        arena.scene_set_space(scene, false, &mut backend).unwrap();
        assert_eq!(backend.calls, [(c2, true), (c2, false)]);
        assert!(!arena.scene(scene).unwrap().world_space);
    }
    #[test]
    fn global_color_contract_hits_every_primary_light_without_validation() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        arena
            .scene_set_global_color(scene, [0.25, 0.5, 1.0])
            .unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        let first = arena.create_light().unwrap();
        let second = arena.create_light().unwrap();
        arena.sector_add_light(primary, first).unwrap();
        arena.sector_add_light(primary, second).unwrap();
        arena
            .scene_set_global_color(scene, [0.25, 0.5, 1.0])
            .unwrap();
        for light in [first, second] {
            assert_eq!(
                arena.graph.light_state(light).unwrap().color,
                [0.25, 0.5, 1.0]
            );
        }
        // No validation per the source: NaN/negative pass through the store;
        // the host light-cache numeric error surfaces only after every
        // member was stored.
        assert!(arena
            .scene_set_global_color(scene, [f32::NAN, -1.0, 2.0])
            .is_err());
        for light in [first, second] {
            assert_eq!(
                arena
                    .graph
                    .light_state(light)
                    .unwrap()
                    .color
                    .map(f32::to_bits),
                [f32::NAN, -1.0, 2.0].map(f32::to_bits)
            );
        }
        arena.release(first).unwrap();
        arena.release(second).unwrap();
    }
    struct OpenSeq {
        calls: Vec<SceneOpenEffect>,
        load_word: u32,
    }
    impl SceneOpenBackend for OpenSeq {
        fn open_effect(
            &mut self,
            _: &ModelArena,
            _: SceneKey,
            effect: SceneOpenEffect,
        ) -> Result<u32, ModelError> {
            self.calls.push(effect);
            Ok(match effect {
                SceneOpenEffect::LoadHierarchy => self.load_word,
                _ => 0,
            })
        }
    }
    #[test]
    fn open_null_file_returns_ok_word_after_close_prefix() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let created = arena.create_scene_sector(scene).unwrap();
        assert_eq!(arena.sector(created).unwrap().scene, Some(scene));
        assert_eq!(arena.scene_loaded_sector_count(scene).unwrap(), 1);
        let mut seq = OpenSeq {
            calls: Vec::new(),
            load_word: 0x4000_0001,
        };
        assert_eq!(
            arena
                .open_scene_with_backend(scene, None, &mut seq, &mut space_map())
                .unwrap(),
            0
        );
        assert_eq!(seq.calls, [SceneOpenEffect::CloseScene]);
        let mut seq = OpenSeq {
            calls: Vec::new(),
            load_word: 0x4000_0001,
        };
        assert_eq!(
            arena
                .open_scene_with_backend(scene, Some(b"maps\\m1.i3d"), &mut seq, &mut space_map())
                .unwrap(),
            0x4000_0001
        );
        assert_eq!(
            seq.calls,
            [
                SceneOpenEffect::CloseScene,
                SceneOpenEffect::LoadHierarchy,
                SceneOpenEffect::UpdateCamera
            ]
        );
        // MakeReady marks ring sectors (flag 4) with no portals.
        arena.make_ready_sectors(scene, &mut space_map()).unwrap();
        assert_ne!(arena.sector(created).unwrap().sector_flags & 4, 0);
    }
    #[test]
    fn get_sector_list_overload_prefers_reverse_order_then_primary() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let a = crate::scene_lightness::loaded_sector_fixture(&mut arena, scene, [0.; 3]);
            let b = crate::scene_lightness::loaded_sector_fixture(&mut arena, scene, [10., 0., 0.]);
            let primary = arena.scene(scene).unwrap().sectors[0];
            assert_eq!(
                arena
                    .scene_get_sector_from_list(scene, [10., 0., 0.], &[a, b])
                    .unwrap(),
                b
            );
            assert_eq!(
                arena
                    .scene_get_sector_from_list(scene, [10., 0., 0.], &[b, a])
                    .unwrap(),
                b
            );
            assert_eq!(
                arena
                    .scene_get_sector_from_list(scene, [30., 0., 0.], &[a, b])
                    .unwrap(),
                primary
            );
            assert_eq!(
                arena
                    .scene_get_sector_from_list(scene, [0.; 3], &[])
                    .unwrap(),
                primary
            );
        });
    }
    #[test]
    fn delete_nonfog_light_keeps_fog_selection_and_miss_word_is_residueless() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0];
        let fog = arena.create_light().unwrap();
        let plain = arena.create_light().unwrap();
        arena.graph.set_light_type(fog, 5).unwrap();
        arena.graph.set_light_type(plain, 4).unwrap();
        arena.sector_add_light(primary, fog).unwrap();
        arena.sector_add_light(primary, plain).unwrap();
        // Miss word with no state touched (native I3DERR_OBJECTNOTFOUND).
        let ghost = arena.create_light().unwrap();
        let before = arena.sector(primary).unwrap().clone();
        let changed_before = arena.sector_lights_changed(primary).unwrap();
        assert_eq!(arena.sector_delete_light(primary, ghost), Ok(0x80000006));
        assert_eq!(arena.sector(primary).unwrap().lights, before.lights);
        assert_eq!(arena.sector(primary).unwrap().fog_light, before.fog_light);
        assert_eq!(
            arena.sector(primary).unwrap().num_fog_lights,
            before.num_fog_lights
        );
        assert_eq!(arena.sector_lights_changed(primary), Ok(changed_before));
        // Non-fog delete: fog selection and count survive, changed bumps once.
        assert_eq!(arena.sector_delete_light(primary, plain), Ok(0));
        let sector = arena.sector(primary).unwrap();
        assert_eq!(sector.lights, [fog]);
        assert_eq!(sector.num_fog_lights, 1);
        assert_eq!(sector.fog_light, Some(fog));
        assert_eq!(arena.sector_lights_changed(primary), Ok(changed_before + 1));
        arena.release(fog).unwrap();
        arena.release(plain).unwrap();
        arena.release(ghost).unwrap();
    }
    #[test]
    fn driver_delete_from_scene_tick_list_walks_newest_first_and_stops_after_first_ok() {
        // Driver 0x1a0a0 (mafia-re I3d_driver.cpp:1744-1750): newest scene
        // first (`for (i = scenes.size(); i--;)`), stop after the first
        // DeleteTickFrame that returns I3D_OK. Hand-derived vectors.
        let mut arena = ModelArena::default();
        let older = arena.construct_scene_owner_prefix().unwrap();
        let newer = arena.construct_scene_owner_prefix().unwrap();
        let shared = dummy(&mut arena, b"shared-tick");
        arena.scene_add_tick_frame(older, shared).unwrap();
        arena.scene_add_tick_frame(newer, shared).unwrap();
        // Newest-first walk: the newer scene reports OK first, so the older
        // scene keeps its membership (early stop). The TICK flag clears on
        // the per-scene removal exactly like the source (I3d_scene.cpp:456-458
        // clears unconditionally, even with other registrations left).
        arena.driver_delete_from_scene_tick_list(shared);
        assert!(arena.scene(newer).unwrap().tick_frames.is_empty());
        assert_eq!(arena.scene(older).unwrap().tick_frames, [shared]);
        assert_eq!(arena.state(shared).unwrap().flags & SCENE_TICK_FLAG, 0);
        // Second driver pass removes the older registration.
        arena.driver_delete_from_scene_tick_list(shared);
        assert!(arena.scene(older).unwrap().tick_frames.is_empty());
        assert_eq!(arena.state(shared).unwrap().flags & SCENE_TICK_FLAG, 0);
    }
    #[test]
    fn scene_callback_stage_bits_report_only_their_bits() {
        // Working names for the inlined SCN_CALLBACK_* gates (I3D.h:1315-1321):
        // no LS3DF FUNCTION addresses exist for standalone readers.
        assert!(scene_callback_wants_pre_render(0x1));
        assert!(scene_callback_wants_post_scene(0x2));
        assert!(scene_callback_wants_post_render(0x4));
        assert!(scene_callback_wants_pre_render(0x7));
        assert!(scene_callback_wants_post_scene(0x7));
        assert!(scene_callback_wants_post_render(0x7));
        assert!(!scene_callback_wants_pre_render(0));
        assert!(!scene_callback_wants_post_scene(0x1));
        assert!(!scene_callback_wants_post_render(0x3));
        assert!(!scene_callback_wants_pre_render(0x6));
        assert!(!scene_callback_wants_post_scene(u32::MAX & !0x2));
        assert!(!scene_callback_wants_post_render(u32::MAX & !0x4));
        assert!(scene_callback_wants_pre_render(u32::MAX));
        assert!(scene_callback_wants_post_scene(u32::MAX));
        assert!(scene_callback_wants_post_render(u32::MAX));
    }
}

#[cfg(test)]
mod scene_group_bits_and_trivial_rva_tests {
    use super::*;

    #[test]
    fn sector_set_group_bits_rva_0x508b0_stores_and_bumps_counter() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        assert_eq!(arena.sector(sector).unwrap().geometry.group_bits, 0);
        assert_eq!(arena.sector_lights_changed(sector).unwrap(), 0);
        arena.sector_set_group_bits(sector, 0x1122_3344).unwrap();
        assert_eq!(
            arena.sector(sector).unwrap().geometry.group_bits,
            0x1122_3344
        );
        assert_eq!(arena.sector_lights_changed(sector).unwrap(), 1);
        arena.sector_set_group_bits(sector, 0).unwrap();
        assert_eq!(arena.sector(sector).unwrap().geometry.group_bits, 0);
        assert_eq!(arena.sector_lights_changed(sector).unwrap(), 2);
    }

    #[test]
    fn sector_group_bits_reader_reports_setter_store() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sector = arena.scene(scene).unwrap().sectors[0];
        assert_eq!(arena.sector_group_bits_of(sector).unwrap(), 0);
        arena.sector_set_group_bits(sector, 0x1122_3344).unwrap();
        assert_eq!(arena.sector_group_bits_of(sector).unwrap(), 0x1122_3344);
    }

    #[test]
    fn scene_update_noop_rva_0x4e970_accepts_valid_scene() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert!(arena.scene_update_noop(scene).is_ok());
    }

    #[test]
    fn scene_set_dir_noop_rva_0x4e980_ignores_nonfinite_input() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert!(arena
            .scene_set_dir_noop(scene, [f32::NAN, f32::INFINITY, 0.0], f32::NEG_INFINITY)
            .is_ok());
        assert!(arena.scene_set_dir_noop(scene, [0.0, 1.0, 0.0], 0.5).is_ok());
    }

    #[test]
    fn scene_set_name_ok_rva_0x4e990_always_returns_zero() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(arena.scene_set_name_ok(scene, None).unwrap(), 0);
        assert_eq!(arena.scene_set_name_ok(scene, Some(&b"name"[..])).unwrap(), 0);
    }

    #[test]
    fn scene_link_to_unsupported_rva_0x4e9a0_always_returns_code() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        assert_eq!(
            arena.scene_link_to_unsupported(scene, None, 0).unwrap(),
            0x8000_000e
        );
        assert_eq!(
            arena
                .scene_link_to_unsupported(scene, Some(scene.root()), 7)
                .unwrap(),
            0x8000_000e
        );
    }
}
