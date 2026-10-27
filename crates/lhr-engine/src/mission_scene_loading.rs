//! Original scene2 loader's local name-map initialization, Game RVA 0x14120a.
//! PC SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This borrows an existing scene and executes its ordered camera/header/sector
//! prefix, admitted occluders/lights and fresh model construction. Original
//! static model-cache loading and ordered source-only actor/program installation.
//! World SceneOpen and gameplay initialization remain
//! explicit boundaries; a constructor-empty scene is not native mission context.
use crate::driver_scene::DriverSceneOwner;
use crate::frame_camera::CameraKey;
use crate::frame_transform::FrameKey;
use crate::frame_transform::FrameKind;
use crate::game_camera_projection::GameCameraProjection;
use crate::mission_actor_stream::OriginalLoadMap;
use crate::mission_scene_stream::MissionSceneInput;
use crate::model_loading::ModelArena;
use crate::native_transform::SQuat;
use crate::program_patch::ProgramPatchTable;
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneResources, SceneScreen};
use crate::script_scheduler::FrameReferenceId;
use crate::sector_sound_data::SectorSoundInput;
use std::collections::HashSet;

#[derive(Clone, Copy)]
enum ResourceOpenKind {
    Model,
    Sound,
}
type ResourceOpenAdapter<'a> =
    dyn FnMut(&mut ModelArena, FrameKey, &[u8], ResourceOpenKind) -> Result<i32, String> + 'a;
type FactoryNameAllocator =
    Box<dyn FnMut(usize) -> Result<Vec<u8>, crate::frame_fast_lookup::FastFrameError>>;

/// Publication order is retained separately from the logical map's key order.
#[derive(Debug, PartialEq, Eq)]
pub struct LoadMapPublication {
    pub name: Vec<u8>,
    pub frame: FrameReferenceId,
}
pub struct SceneLoadMapPrefix {
    pub map: OriginalLoadMap,
    pub publications: Vec<LoadMapPublication>,
}
pub struct SceneLoadMapError {
    pub prefix: SceneLoadMapPrefix,
    pub detail: String,
}

/// Executes the fixed no-mask EnumFrames(0x4ffff, 5455e0) projection, then
/// explicitly publishes primary, backdrop and omni sectors (541315..5414db).
/// Enumeration excludes sector roots and visits primary children followed by
/// backdrop children in native child/next order. All low sixteen type bits are
/// enabled; omni and near subtree flags are absent. Callback5455e0 copies the
/// current full name (+100), mapping a null name pointer to an empty key.
/// Native C_str allocation/refcounts and map balancing remain host projections.
/// The host map preserves non-NUL name bytes in native unsigned strcmp order;
/// invalid names retain the already-published prefix without further traversal.
pub fn initialize_scene_load_map(
    arena: &ModelArena,
    scene: SceneKey,
) -> Result<SceneLoadMapPrefix, SceneLoadMapError> {
    let mut prefix = SceneLoadMapPrefix {
        map: OriginalLoadMap::for_preceding_loader(),
        publications: Vec::new(),
    };
    let result = (|| -> Result<(), String> {
        let sectors = arena.scene(scene).map_err(|e| format!("{e:?}"))?.sectors;
        for sector in &sectors[..2] {
            // A read-only callback never destroys or relinks frames, so the
            // original saved-next and lastDestroyedFrame branches cannot fire.
            let mut stack = Vec::new();
            if let Some(child) = arena
                .links(sector.frame())
                .map_err(|e| format!("{e:?}"))?
                .child
            {
                stack.push(child);
            }
            let mut visited = HashSet::new();
            while let Some(key) = stack.pop() {
                if !visited.insert(key.0) || visited.len() > 65_536 {
                    return Err("invalid or over-budget original enumeration hierarchy".into());
                }
                let links = arena.links(key).map_err(|e| format!("{e:?}"))?;
                publish(arena, key, &mut prefix)?;
                if let Some(next) = links.next {
                    stack.push(next);
                }
                if let Some(child) = links.child {
                    stack.push(child);
                }
            }
        }
        for sector in &sectors[..3] {
            publish(arena, sector.frame(), &mut prefix)?;
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(prefix),
        Err(detail) => Err(SceneLoadMapError { prefix, detail }),
    }
}

fn publish(
    arena: &ModelArena,
    key: FrameKey,
    prefix: &mut SceneLoadMapPrefix,
) -> Result<(), String> {
    let loaded = arena.frame(key).map_err(|e| format!("{e:?}"))?;
    let name = if loaded.name_is_null {
        &[][..]
    } else {
        loaded.name_bytes.as_slice()
    };
    let frame = arena.lookup_reference(key);
    prefix.map.publish_loaded_frame(arena, name, frame)?;
    prefix.publications.push(LoadMapPublication {
        name: name.to_vec(),
        frame,
    });
    Ok(())
}

#[derive(Debug)]
pub struct MissionSceneLoadError {
    pub cursor: usize,
    /// Reached original instruction, not a standalone reconstructed function.
    pub entry: u32,
    pub detail: String,
}
/// Raw scratch fields staged by this original object record. Option marks
/// an unwritten native stack value, not a fabricated zero initializer.
#[derive(Debug)]
pub struct MissionObjectFields {
    pub flags: u32,
    pub position: Option<[u32; 3]>,
    pub rotation: Option<[u32; 4]>,
    pub scale: [u32; 3],
    pub fallback_position: Option<[u32; 3]>,
    pub parent: Option<FrameReferenceId>,
    pub parent_name: Option<Vec<u8>>,
    pub geometry_build_result: Option<u32>,
    pub visual_subtype_request: Option<u32>,
    pub visual_subtype_current: Option<u32>,
    pub saved_lightmap_consumed: Option<usize>,
    pub saved_lightmap_result: Option<u32>,
}
impl Default for MissionObjectFields {
    fn default() -> Self {
        Self {
            flags: 0,
            position: None,
            rotation: None,
            scale: [1f32.to_bits(); 3],
            fallback_position: None,
            parent: None,
            parent_name: None,
            geometry_build_result: None,
            visual_subtype_request: None,
            visual_subtype_current: None,
            saved_lightmap_consumed: None,
            saved_lightmap_result: None,
        }
    }
}
/// Game-owned temporary geometry survives an unavailable reached read/Build.
/// Unwritten heap bytes are not exposed as invented initialized vertices.
#[derive(Debug)]
pub struct MissionOccluderGeometry {
    pub vertex_count: u32,
    pub vertices: Option<Vec<[u32; 3]>>,
    pub face_count: Option<u32>,
    pub faces: Option<Vec<[u16; 3]>>,
}
/// Actual Game-owned temporary arrays for the AREA Build caller. None in an
/// allocated element denotes bytes the original default constructor never wrote.
#[derive(Debug)]
pub struct MissionAreaGeometry {
    pub vertex_count: u32,
    pub vertices: Option<Vec<Option<[u32; 3]>>>,
    pub face_count: Option<u32>,
    pub faces: Option<Vec<Option<[u16; 3]>>>,
}
#[derive(Debug)]
pub struct MissionModelOpenRequest {
    pub model: FrameReferenceId,
    pub input_name: Vec<u8>,
    pub cache_name: Vec<u8>,
}
#[derive(Debug)]
pub struct PendingLightSectorLink {
    pub light: FrameReferenceId,
    pub name: Vec<u8>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundLinkKind {
    Sector,
    Area,
}
#[derive(Debug)]
pub struct PendingSoundLink {
    pub sound: FrameReferenceId,
    pub name: Vec<u8>,
    pub kind: SoundLinkKind,
}
/// State of one original scene2 invocation. The caller retains its real scene,
/// driver and resources; an error leaves their executed effects in place.
pub struct MissionSceneLoader {
    pub scene: SceneKey,
    /// Source-only actors/programs use this loader's live map and arena.
    pub content: crate::mission_content_loading::MissionContentOwner,
    pub load_map: SceneLoadMapPrefix,
    pub camera: Option<CameraKey>,
    pub version: [Option<u16>; 2],
    /// Raw mission+0/+4 fields written by afff; no guessed semantic names.
    pub mission_words: [Option<u32>; 2],
    pub mission_far_range: Option<u32>,
    pub current_kind: u32,
    pub current_name: Vec<u8>,
    pub current_frame: Option<FrameReferenceId>,
    pub finished_objects: usize,
    pub cursor: usize,
    pub object_fields: MissionObjectFields,
    /// Host retention of a genuine unfinished factory creator reference.
    pub pending_factory_creator: Option<FrameReferenceId>,
    pub pending_occluder_geometry: Option<MissionOccluderGeometry>,
    pub pending_area_geometry: Option<MissionAreaGeometry>,
    pub pending_model_open: Option<MissionModelOpenRequest>,
    pub pending_light_sector_link: Option<PendingLightSectorLink>,
    /// Game's two temporary request arrays at stack +0x60 and +0x84.
    /// Their entries remain in insertion order until the final reverse walk.
    pub pending_sound_sector_links: Vec<PendingSoundLink>,
    pub pending_sound_area_links: Vec<PendingSoundLink>,
    pub pending_visual_replacement: Option<crate::visual_replacement::PendingVisualReplacement>,
    /// Optional host allocator for the reached object-factory SetName call.
    /// The default uses ModelArena's normal allocator. A failing allocator
    /// retains the native creator reference and the executed name prefix.
    factory_name_allocator: Option<FactoryNameAllocator>,
    entered: bool,
}
impl MissionSceneLoader {
    pub fn new(arena: &ModelArena, scene: SceneKey) -> Result<Self, SceneLoadMapError> {
        Ok(Self {
            scene,
            content: crate::mission_content_loading::MissionContentOwner::default(),
            load_map: initialize_scene_load_map(arena, scene)?,
            camera: None,
            version: [None; 2],
            mission_words: [None; 2],
            mission_far_range: None,
            current_kind: 0,
            current_name: Vec::new(),
            current_frame: None,
            finished_objects: 0,
            cursor: 0,
            object_fields: MissionObjectFields::default(),
            pending_factory_creator: None,
            pending_occluder_geometry: None,
            pending_area_geometry: None,
            pending_model_open: None,
            pending_light_sector_link: None,
            pending_sound_sector_links: Vec::new(),
            pending_sound_area_links: Vec::new(),
            pending_visual_replacement: None,
            factory_name_allocator: None,
            entered: false,
        })
    }
    pub fn set_factory_name_allocator(
        &mut self,
        allocate: impl FnMut(usize) -> Result<Vec<u8>, crate::frame_fast_lookup::FastFrameError>
            + 'static,
    ) {
        self.factory_name_allocator = Some(Box::new(allocate));
    }
    fn error(&self, entry: u32, detail: impl Into<String>) -> MissionSceneLoadError {
        MissionSceneLoadError {
            cursor: self.cursor,
            entry,
            detail: detail.into(),
        }
    }
    /// Terminal host shutdown: dispose source actor/program owners and release
    /// outstanding visual creator references before scene/cache teardown.
    /// Non-owning map/current diagnostics may then be stale and cannot resume
    /// this invocation. An unavailable destructor retains the pending owner.
    pub fn release_pending_visual_replacement(
        &mut self,
        arena: &mut ModelArena,
    ) -> Result<(), crate::model_loading::ModelError> {
        self.entered = true;
        // Every caller already performs this terminal step before releasing the
        // scene. Source actors/programs must die while their real frames live.
        self.content
            .discard_terminal(arena)
            .map_err(crate::model_loading::ModelError::Asset)?;
        if let Some(pending) = self.pending_factory_creator {
            let key = FrameKey(pending.identity());
            if arena.lookup_reference(key) != pending {
                return Err(crate::model_loading::ModelError::UnknownFrame(key));
            }
            arena.release(key)?;
            self.pending_factory_creator = None;
        }
        if let Some(pending) = self.pending_visual_replacement {
            arena.release(pending.new)?;
            self.pending_visual_replacement = None;
        }
        Ok(())
    }
    /// Executes camera setup before reading the LS header, then consumes fields
    /// in actual order. This currently stops at the first unavailable reached
    /// factory/field; it never jumps over objects to reach actor installation.
    pub fn load_prefix<S: SceneScreen>(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
        bytes: &[u8],
    ) -> Result<(), MissionSceneLoadError>
    where
        S::Error: std::fmt::Debug,
    {
        self.load_prefix_inner(
            driver,
            arena,
            resources,
            screen,
            projection,
            bytes,
            &mut None,
            &ProgramPatchTable::default(),
        )
    }
    /// Use the same cache owner/directories as startup and mission UnInit.
    #[allow(clippy::too_many_arguments)] // Original invocation plus explicit resource owners.
    pub fn load_prefix_with_model_cache<
        S: SceneScreen,
        B: crate::mission_resources::MissionCacheAllocator,
    >(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
        bytes: &[u8],
        caches: &mut crate::mission_resources::MissionCaches,
        installation: &lhr_assets::Installation,
        memory: &mut B,
    ) -> Result<(), MissionSceneLoadError>
    where
        S::Error: std::fmt::Debug,
        B::Error: std::fmt::Debug,
    {
        let mut open = |arena: &mut ModelArena, frame, name: &[u8], kind| match kind {
            ResourceOpenKind::Model => caches
                .open_static_model(
                    arena,
                    installation,
                    crate::model_loading::ModelKey(frame),
                    name,
                    memory,
                )
                .map_err(|e| format!("{e:?}")),
            ResourceOpenKind::Sound => {
                Err("sound cache owner is absent from this legacy model-only invocation".into())
            }
        };
        self.load_prefix_inner(
            driver,
            arena,
            resources,
            screen,
            projection,
            bytes,
            &mut Some(&mut open),
            &ProgramPatchTable::default(),
        )
    }
    /// Ordered static scene ingestion with the real model and sound cache
    /// owners. Both cache calls borrow the same MissionCaches and allocator.
    #[allow(clippy::too_many_arguments)]
    pub fn load_prefix_with_resource_cache<
        S: SceneScreen,
        B: crate::mission_resources::MissionCacheAllocator,
    >(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
        bytes: &[u8],
        caches: &mut crate::mission_resources::MissionCaches,
        installation: &lhr_assets::Installation,
        memory: &mut B,
    ) -> Result<(), MissionSceneLoadError>
    where
        S::Error: std::fmt::Debug,
        B::Error: std::fmt::Debug,
    {
        self.load_prefix_with_resource_cache_and_patches(
            driver,
            arena,
            resources,
            screen,
            projection,
            bytes,
            caches,
            installation,
            memory,
            &ProgramPatchTable::default(),
        )
    }
    /// Borrow the actual startup table for this entire ordered input. Mission
    /// content stores no table snapshot; each reached root reads this owner.
    #[allow(clippy::too_many_arguments)]
    pub fn load_prefix_with_resource_cache_and_patches<
        S: SceneScreen,
        B: crate::mission_resources::MissionCacheAllocator,
    >(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
        bytes: &[u8],
        caches: &mut crate::mission_resources::MissionCaches,
        installation: &lhr_assets::Installation,
        memory: &mut B,
        patches: &ProgramPatchTable,
    ) -> Result<(), MissionSceneLoadError>
    where
        S::Error: std::fmt::Debug,
        B::Error: std::fmt::Debug,
    {
        let mut open = |arena: &mut ModelArena, frame, name: &[u8], kind| match kind {
            ResourceOpenKind::Model => caches
                .open_static_model(
                    arena,
                    installation,
                    crate::model_loading::ModelKey(frame),
                    name,
                    memory,
                )
                .map_err(|e| format!("{e:?}")),
            ResourceOpenKind::Sound => {
                let sound = arena.sound_key(frame).map_err(|e| format!("{e:?}"))?;
                caches
                    .open_static_sound(arena, installation, sound, name, memory)
                    .map_err(|e| format!("{e:?}"))
            }
        };
        self.load_prefix_inner(
            driver,
            arena,
            resources,
            screen,
            projection,
            bytes,
            &mut Some(&mut open),
            patches,
        )
    }
    #[allow(clippy::too_many_arguments)] // Original invocation plus explicit resource owners.
    fn load_prefix_inner<S: SceneScreen>(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
        bytes: &[u8],
        resource_open: &mut Option<&mut ResourceOpenAdapter<'_>>,
        patches: &ProgramPatchTable,
    ) -> Result<(), MissionSceneLoadError>
    where
        S::Error: std::fmt::Debug,
    {
        if self.entered {
            return Err(self.error(0x5414fa, "scene2 invocation already entered"));
        }
        self.entered = true;
        driver
            .validate_scene_domain(arena, resources, self.scene)
            .map_err(|e| self.error(0x5414fa, format!("{e:?}")))?;
        self.prepare_camera(driver, arena, resources, screen, projection)?;
        let mut input = MissionSceneInput::new(bytes).map_err(|e| self.error(0x54163b, e))?;
        self.cursor = input.cursor();
        let result = self.read_root(
            driver,
            arena,
            resources,
            screen,
            projection,
            &mut input,
            resource_open,
            patches,
        );
        self.cursor = input.cursor();
        result.map_err(|(entry, detail)| self.error(entry, detail))
    }
    fn prepare_camera<S: SceneScreen>(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
    ) -> Result<(), MissionSceneLoadError>
    where
        S::Error: std::fmt::Debug,
    {
        self.camera = arena
            .scene_active_camera(self.scene)
            .map_err(|e| self.error(0x541502, format!("{e:?}")))?;
        if self.camera.is_none() {
            let camera = match driver.create_camera_frame(arena, resources, self.scene, screen) {
                Ok(camera) => camera,
                Err(failure) => {
                    self.camera = failure.camera;
                    return Err(self.error(0x54151e, format!("{:?}", failure.error)));
                }
            };
            self.camera = Some(camera);
            arena
                .set_frame_name(camera.frame(), Some(b"GameCamera"))
                .map_err(|e| self.error(0x54152f, format!("{e:?}")))?;
            arena
                .scene_set_active_camera(self.scene, Some(camera))
                .map_err(|e| self.error(0x541539, format!("{e:?}")))?;
            arena
                .camera_set_direction(camera, [-1., f32::from_bits(0xbdcccccd), -1.], 0.)
                .map_err(|e| self.error(0x541569, format!("{e:?}")))?;
            arena
                .camera_set_position(camera, [10., 5., 10.])
                .map_err(|e| self.error(0x541595, format!("{e:?}")))?;
            arena
                .camera_set_range(camera, f32::from_bits(0x3d4ccccd), 250.)
                .map_err(|e| self.error(0x5415dd, format!("{e:?}")))?;
            self.mission_far_range = Some(250f32.to_bits());
            arena
                .camera_update(camera)
                .map_err(|e| self.error(0x5415e9, format!("{e:?}")))?;
            arena
                .release(camera.frame())
                .map_err(|e| self.error(0x5415ef, format!("{e:?}")))?;
        }
        let camera = self.camera.unwrap();
        arena
            .camera_set_orthographic(camera, false)
            .map_err(|e| self.error(0x5415f5, format!("{e:?}")))?;
        arena
            .camera_set_range(camera, f32::from_bits(0x3d4ccccd), 250.)
            .map_err(|e| self.error(0x541601, format!("{e:?}")))?;
        self.mission_far_range = Some(250f32.to_bits());
        game_camera_projection(
            arena,
            camera,
            screen,
            projection,
            f32::from_bits(0x3fc90fdb),
        )
        .map_err(|e| self.error(0x541615, e))?;
        arena
            .camera_update(camera)
            .map_err(|e| self.error(0x54161d, format!("{e:?}")))?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)] // Original invocation plus explicit resource owners.
    fn read_root<S: SceneScreen>(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        projection: GameCameraProjection,
        input: &mut MissionSceneInput<'_>,
        resource_open: &mut Option<&mut ResourceOpenAdapter<'_>>,
        patches: &ProgramPatchTable,
    ) -> Result<(), (u32, String)>
    where
        S::Error: std::fmt::Debug,
    {
        while let Some(chunk) = input.next_chunk().map_err(|e| (0x5416df, e))? {
            match chunk.kind {
                1 => {
                    self.version[0] = Some(input.read_u16().map_err(|e| (0x5417e7, e))?);
                    self.version[1] = Some(input.read_u16().map_err(|e| (0x541859, e))?);
                }
                0xafff => {
                    let bytes = input.read(8).map_err(|e| (0x544cc3, e))?;
                    self.mission_words = [
                        Some(u32::from_le_bytes(bytes[..4].try_into().unwrap())),
                        Some(u32::from_le_bytes(bytes[4..].try_into().unwrap())),
                    ];
                }
                0x3200 => {
                    input.read(12).map_err(|e| (0x5447c4, e))?;
                    arena
                        .scene_set_zero_field_220(self.scene)
                        .map_err(|e| (0x54481d, format!("{e:?}")))?;
                }
                0x3010..=0x3013 => {
                    let camera = self.camera.unwrap();
                    match chunk.kind {
                        0x3012 => arena
                            .camera_set_orthographic(camera, true)
                            .map_err(|e| (0x54187c, format!("{e:?}")))?,
                        _ => {
                            let word = input.read_u32().map_err(|e| (0x5418ee, e))?;
                            let value = f32::from_bits(word);
                            match chunk.kind {
                                0x3010 => {
                                    game_camera_projection(arena, camera, screen, projection, value)
                                        .map_err(|e| (0x54196a, e))?
                                }
                                0x3011 => {
                                    arena
                                        .camera_set_range(camera, f32::from_bits(0x3d4ccccd), value)
                                        .map_err(|e| (0x54193e, format!("{e:?}")))?;
                                    self.mission_far_range = Some(word);
                                }
                                _ => arena
                                    .camera_set_ortho_scale(camera, value)
                                    .map_err(|e| (0x54191d, format!("{e:?}")))?,
                            }
                        }
                    }
                }
                0x3211 => {
                    let data = input.read(8).map_err(|e| (0x544715, e))?;
                    let near = f32::from_bits(u32::from_le_bytes(data[..4].try_into().unwrap()));
                    let far = f32::from_bits(u32::from_le_bytes(data[4..].try_into().unwrap()));
                    let far = if far < 3000. { 3000. } else { far };
                    arena
                        .scene_set_backdrop_range(self.scene, near, far)
                        .map_err(|e| (0x54474f, format!("{e:?}")))?;
                }
                0x4000 => {
                    input.enter_current().map_err(|e| (0x54199c, e))?;
                    self.read_objects(driver, arena, resources, screen, input, resource_open)?;
                    input.leave_container().map_err(|e| (0x5446aa, e))?;
                    continue;
                }
                0xae20 => {
                    let result =
                        self.content
                            .load_actors(chunk, arena, &mut self.load_map.map, patches);
                    input
                        .advance_current_to(self.content.cursor)
                        .map_err(|e| (0x544a16, e))?;
                    result.map_err(|detail| (0x544a16, detail))?;
                }
                0xae50 => {
                    let result = self.content.load_programs(chunk, arena, patches);
                    input
                        .advance_current_to(self.content.cursor)
                        .map_err(|e| (0x544b92, e))?;
                    result.map_err(|detail| (0x544b92, detail))?;
                }
                _ => {
                    return Err((
                        0x541743,
                        format!("unavailable reached scene2 root arm {:04x}", chunk.kind),
                    ));
                }
            }
            input.close_current().map_err(|e| (0x404ce0, e))?;
        }
        self.resolve_deferred_sound_links(arena)?;
        Ok(())
    }
    fn resolve_deferred_sound_links(
        &mut self,
        arena: &mut ModelArena,
    ) -> Result<(), (u32, String)> {
        // Game 0x544efb and 0x545030 walk separate vectors backwards after
        // scene object input. Missing/wrong-type targets are ignored there.
        for (kind, links) in [
            (SoundLinkKind::Sector, &self.pending_sound_sector_links),
            (SoundLinkKind::Area, &self.pending_sound_area_links),
        ] {
            for pending in links.iter().rev() {
                let sound_frame = FrameKey(pending.sound.identity());
                if arena.lookup_reference(sound_frame) != pending.sound {
                    return Err((0x544f9a, "stale deferred sound frame".into()));
                }
                let sound = arena
                    .sound_key(sound_frame)
                    .map_err(|e| (0x544f9a, format!("{e:?}")))?;
                let Some(target) = self.load_map.map.slot(&pending.name).flatten() else {
                    continue;
                };
                let frame = FrameKey(target.identity());
                if arena.lookup_reference(frame) != target {
                    return Err((0x544f81, "stale deferred sound target".into()));
                }
                let expected = if kind == SoundLinkKind::Sector {
                    FrameKind::Sector
                } else {
                    FrameKind::Area
                };
                if arena
                    .state(frame)
                    .map_err(|e| (0x544f81, format!("{e:?}")))?
                    .kind
                    != expected
                {
                    continue;
                }
                match kind {
                    SoundLinkKind::Sector => {
                        arena.add_sound_to_sector(crate::scene_ownership::SectorKey(frame), sound)
                    }
                    SoundLinkKind::Area => arena.add_sound_to_area(
                        arena
                            .area_key(frame)
                            .map_err(|e| (0x5450ca, format!("{e:?}")))?,
                        sound,
                    ),
                }
                .map_err(|e| {
                    (
                        if kind == SoundLinkKind::Sector {
                            0x54500f
                        } else {
                            0x5450c0
                        },
                        format!("{e:?}"),
                    )
                })?;
            }
        }
        self.pending_sound_sector_links.clear();
        self.pending_sound_area_links.clear();
        Ok(())
    }
    #[allow(clippy::too_many_arguments)] // Original invocation plus explicit screen and owners.
    fn read_objects<S: SceneScreen>(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        screen: &mut S,
        input: &mut MissionSceneInput<'_>,
        resource_open: &mut Option<&mut ResourceOpenAdapter<'_>>,
    ) -> Result<(), (u32, String)>
    where
        S::Error: std::fmt::Debug,
    {
        while let Some(chunk) = input.next_chunk().map_err(|e| (0x541a22, e))? {
            if chunk.kind != 0x4010 {
                return Err((
                    0x541a7b,
                    format!("unavailable object record {:04x}", chunk.kind),
                ));
            }
            self.current_kind = 0;
            self.current_frame = None;
            self.object_fields = MissionObjectFields::default();
            self.pending_occluder_geometry = None;
            input.enter_current().map_err(|e| (0x541ae4, e))?;
            while let Some(field) = input.next_chunk().map_err(|e| (0x541b36, e))? {
                match field.kind {
                    0x4011 => self.current_kind = input.read_u32().map_err(|e| (0x54225f, e))?,
                    0x10 => {
                        let bytes = input
                            .read(field.payload.len().min(999))
                            .map_err(|e| (0x541c0f, e))?;
                        let end = bytes.iter().position(|b| *b == 0).ok_or_else(|| {
                            (
                                0x541c20,
                                "unterminated object name beyond admitted stack-buffer domain"
                                    .into(),
                            )
                        })?;
                        self.current_name = bytes[..end].to_vec();
                        input.close_current().map_err(|e| (0x541c29, e))?;
                        let frame = self
                            .load_map
                            .map
                            .lookup_or_insert_null(&self.current_name)
                            .map_err(|e| {
                                (
                                    if self.current_kind == 0 {
                                        0x541c6b
                                    } else {
                                        0x541cc1
                                    },
                                    e,
                                )
                            })?;
                        if self.current_kind == 0 {
                            self.current_frame = frame;
                        } else if frame.is_none() {
                            let key = match self.current_kind {
                                12 => driver
                                    .create_occluder_frame(arena, resources, self.scene)
                                    .map_err(|e| (0x541d05, format!("{e:?}")))?
                                    .frame(),
                                3 => match driver
                                    .create_camera_frame(arena, resources, self.scene, screen)
                                {
                                    Ok(camera) => camera.frame(),
                                    Err(failure) => {
                                        self.pending_factory_creator = failure
                                            .camera
                                            .map(|camera| arena.lookup_reference(camera.frame()));
                                        return Err((0x541d05, format!("{:?}", failure.error)));
                                    }
                                },
                                4 => {
                                    match driver.create_sound_frame(arena, resources, self.scene) {
                                        Ok(sound) => sound.frame(),
                                        Err(failure) => {
                                            self.pending_factory_creator = failure
                                                .frame
                                                .map(|sound| arena.lookup_reference(sound.frame()));
                                            return Err((0x541d05, format!("{:?}", failure.error)));
                                        }
                                    }
                                }
                                14 => driver
                                    .create_area_frame(arena, resources, self.scene)
                                    .map_err(|e| (0x541d05, format!("{e:?}")))?
                                    .frame(),
                                6 => driver
                                    .create_dummy_frame(arena, resources, self.scene)
                                    .map_err(|e| (0x541d05, format!("{e:?}")))?,
                                2 => driver
                                    .create_light_frame(arena, resources, self.scene)
                                    .map_err(|e| (0x541d05, format!("{e:?}")))?,
                                9 => driver
                                    .create_model_frame(arena, resources, self.scene)
                                    .map_err(|e| (0x541d05, format!("{e:?}")))?
                                    .root(),
                                _ => {
                                    return Err((
                                        if self.current_kind == 1 {
                                            0x541cfe
                                        } else {
                                            0x541d05
                                        },
                                        format!(
                                            "unavailable original driver frame factory kind {}; null map slot retained",
                                            self.current_kind
                                        ),
                                    ));
                                }
                            };
                            self.pending_factory_creator = Some(arena.lookup_reference(key));
                            self.current_frame = Some(arena.lookup_reference(key));
                            if let Some(allocate) = self.factory_name_allocator.as_mut() {
                                arena
                                    .set_frame_name_with_allocator(
                                        key,
                                        Some(&self.current_name),
                                        allocate,
                                    )
                                    .map_err(|e| (0x541d2c, format!("{e:?}")))?;
                            } else {
                                arena
                                    .set_frame_name(key, Some(&self.current_name))
                                    .map_err(|e| (0x541d2c, format!("{e:?}")))?;
                            }
                            self.object_fields.flags |= 0x20000;
                            arena
                                .scene_add_frame(self.scene, key)
                                .map_err(|e| (0x541d47, format!("{e:?}")))?;
                            arena
                                .release(key)
                                .map_err(|e| (0x541d4e, format!("{e:?}")))?;
                            self.pending_factory_creator = None;
                            publish(arena, key, &mut self.load_map).map_err(|e| (0x541e0d, e))?;
                        }
                        continue;
                    }
                    0x20 | 0x2c | 0x2d => {
                        let raw = input.read(12).map_err(|e| {
                            (
                                match field.kind {
                                    0x20 => 0x541e52,
                                    0x2c => 0x541ea7,
                                    _ => 0x541f5a,
                                },
                                e,
                            )
                        })?;
                        let words = std::array::from_fn(|n| {
                            u32::from_le_bytes(raw[n * 4..n * 4 + 4].try_into().unwrap())
                        });
                        input.close_current().map_err(|e| (0x404ce0, e))?;
                        match field.kind {
                            0x20 => {
                                self.object_fields.position = Some(words);
                                self.object_fields.flags |= 1;
                            }
                            0x2c => self.object_fields.fallback_position = Some(words),
                            _ => {
                                self.object_fields.scale = words;
                                self.object_fields.flags |= 4;
                            }
                        }
                        continue;
                    }
                    0x22 => {
                        let raw = input.read(16).map_err(|e| (0x541ef7, e))?;
                        let words = std::array::from_fn(|n| {
                            u32::from_le_bytes(raw[n * 4..n * 4 + 4].try_into().unwrap())
                        });
                        input.close_current().map_err(|e| (0x541f00, e))?;
                        self.object_fields.rotation = Some(words);
                        self.object_fields.flags |= 2;
                        continue;
                    }
                    0x4020 => {
                        self.read_parent(arena, input)?;
                        if input.current().is_none() {
                            continue;
                        }
                    }
                    0x4040 => {
                        self.read_light(arena, input)?;
                        if input.current().is_none() {
                            continue;
                        }
                    }
                    0x4050 => {
                        self.read_dummy_bound_box(arena, input)?;
                        continue;
                    }
                    0x4060 => {
                        self.read_sound(arena, input, resource_open)?;
                        if input.current().is_none() {
                            continue;
                        }
                    }
                    0x2012 => {
                        self.read_model_open(arena, input, resource_open)?;
                        if input.current().is_none() {
                            continue;
                        }
                    }
                    0xb151 => {
                        self.read_area_geometry(arena, input)?;
                        continue;
                    }
                    0x4083 => self.read_occluder_geometry(arena, input)?,
                    0x4090 => {
                        self.read_visual_subtype(arena, input)?;
                        continue;
                    }
                    0x40a0 => {
                        self.read_saved_lightmaps(arena, input)?;
                        continue;
                    }
                    0xb401 => {
                        self.read_sector_sound(arena, input)?;
                        if input.current().is_none() {
                            continue;
                        }
                    }
                    _ => {
                        return Err((
                            0x541b89,
                            format!("unavailable reached object field {:04x}", field.kind),
                        ));
                    }
                }
                input.close_current().map_err(|e| (0x404ce0, e))?;
            }
            if let Some(frame) = self.current_frame {
                let key = FrameKey(frame.identity());
                if arena.lookup_reference(key) != frame {
                    return Err((0x54462d, "stale original object frame".into()));
                }
                if self.object_fields.flags & 0x10 != 0 {
                    if let Some(parent) = self.object_fields.parent {
                        let parent_key = FrameKey(parent.identity());
                        if arena.lookup_reference(parent_key) != parent {
                            return Err((0x5445de, "stale original parent frame".into()));
                        }
                        let _native_hresult = arena
                            .link_frame(key, Some(parent_key), 0)
                            .map_err(|e| (0x5445de, format!("{e:?}")))?;
                    }
                }
                if self.object_fields.flags & 1 != 0 {
                    let position = self
                        .object_fields
                        .position
                        .ok_or((0x5445f0, "position flag without initialized input".into()))?;
                    arena
                        .graph
                        .assign_position(key, position.map(f32::from_bits))
                        .map_err(|e| (0x5445f0, format!("{e:?}")))?;
                }
                if self.object_fields.flags & 2 != 0 {
                    let rotation = self
                        .object_fields
                        .rotation
                        .ok_or((0x544604, "rotation flag without initialized input".into()))?;
                    arena
                        .graph
                        .assign_rotation(
                            key,
                            SQuat {
                                wxyz: rotation.map(f32::from_bits),
                            },
                        )
                        .map_err(|e| (0x544604, format!("{e:?}")))?;
                }
                if self.object_fields.flags & 4 != 0 {
                    arena
                        .graph
                        .assign_scale(key, self.object_fields.scale.map(f32::from_bits))
                        .map_err(|e| (0x544618, format!("{e:?}")))?;
                }
                if self.object_fields.flags & 0x10000 != 0 {
                    arena
                        .set_frame_on(key, 0)
                        .map_err(|e| (0x54462a, format!("{e:?}")))?;
                }
                arena
                    .update_frame(key)
                    .map_err(|e| (0x544631, format!("{e:?}")))?;
                if arena
                    .state(key)
                    .map_err(|e| (0x544636, format!("{e:?}")))?
                    .kind
                    == FrameKind::Model
                {
                    let set = arena
                        .original_model_animation_set(crate::model_loading::ModelKey(key), 0)
                        .map_err(|e| (0x544645, format!("{e:?}")))?;
                    if set.is_some() {
                        return Err((
                            0x54466a,
                            "original animated-model list insertion546d30 unavailable".into(),
                        ));
                    }
                }
            }
            if self.object_fields.flags & 0x20000 != 0 && self.object_fields.flags & 0x10 == 0 {
                if let Some(frame) = self.current_frame {
                    let key = FrameKey(frame.identity());
                    let world = arena
                        .graph
                        .world_matrix(key)
                        .map_err(|e| (0x544683, format!("{e:?}")))?;
                    let position = world.elements[12..15].try_into().unwrap();
                    let _native_hresult = arena
                        .set_frame_sector_pos(self.scene, key, position)
                        .map_err(|e| (0x544683, format!("{e:?}")))?;
                }
            }
            self.finished_objects += 1;
            input.leave_container().map_err(|e| (0x5446aa, e))?;
        }
        Ok(())
    }
    fn read_sound(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
        resource_open: &mut Option<&mut ResourceOpenAdapter<'_>>,
    ) -> Result<(), (u32, String)> {
        let Some(frame) = self.current_frame else {
            return Ok(());
        };
        let key = FrameKey(frame.identity());
        if arena.lookup_reference(key) != frame {
            return Err((0x542c55, "stale sound object frame".into()));
        }
        if arena
            .state(key)
            .map_err(|e| (0x542c5d, format!("{e:?}")))?
            .kind
            != FrameKind::Sound
        {
            return Ok(());
        }
        let sound = arena
            .sound_key(key)
            .map_err(|e| (0x542c5d, format!("{e:?}")))?;
        let audio = arena
            .host_sound_backend()
            .map_err(|e| (0x542c6f, format!("{e:?}")))?
            .audio
            .clone();
        audio
            .sound_virtual(key.0, 0x24, &[0])
            .map_err(|e| (0x542c6f, e.to_string()))?;
        audio
            .sound_virtual(key.0, 0x6c, &[0])
            .map_err(|e| (0x542c77, e.to_string()))?;
        input.enter_current().map_err(|e| (0x542c80, e))?;
        while let Some(field) = input.next_chunk().map_err(|e| (0x542c80, e))? {
            match field.kind {
                0x10 => {
                    let bytes = input
                        .read(field.payload.len().min(999))
                        .map_err(|e| (0x542dfb, e))?;
                    let end = bytes.iter().position(|&b| b == 0).ok_or((
                        0x542e0c,
                        "unterminated sound filename outside original buffer".into(),
                    ))?;
                    let name = bytes[..end].to_vec();
                    input.close_current().map_err(|e| (0x542e16, e))?;
                    let Some(open) = resource_open.as_mut() else {
                        return Err((
                            0x542e9a,
                            "original sound cache Open requires initialized host resource owner"
                                .into(),
                        ));
                    };
                    open(arena, key, &name, ResourceOpenKind::Sound).map_err(|e| (0x542e9a, e))?;
                    continue;
                }
                0x4061 | 0x4062 | 0x4063 | 0xb800 => {
                    let word = input.read_u32().map_err(|e| (0x542d95, e))?;
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    let slot = match field.kind {
                        0x4061 => 0x58,
                        0x4062 => 0x68,
                        0x4063 => 0x64,
                        _ => 0x74,
                    };
                    audio
                        .sound_virtual(key.0, slot, &[word])
                        .map_err(|e| (0x542dad, e.to_string()))?;
                    continue;
                }
                0x4064 => {
                    let raw = input.read(8).map_err(|e| (0x5432c4, e))?;
                    let args = [
                        u32::from_le_bytes(raw[0..4].try_into().unwrap()),
                        u32::from_le_bytes(raw[4..8].try_into().unwrap()),
                    ];
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    audio
                        .sound_virtual(key.0, 0x60, &args)
                        .map_err(|e| (0x5432c4, e.to_string()))?;
                    continue;
                }
                0x4065 => {
                    let raw = input.read(12).map_err(|e| (0x543111, e))?;
                    let args = [
                        u32::from_le_bytes(raw[0..4].try_into().unwrap()),
                        u32::from_le_bytes(raw[4..8].try_into().unwrap()),
                        u32::from_le_bytes(raw[8..12].try_into().unwrap()),
                        1.0f32.to_bits(),
                    ];
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    audio
                        .sound_virtual(key.0, 0x5c, &args)
                        .map_err(|e| (0x5431c6, e.to_string()))?;
                    continue;
                }
                0x4068 => {
                    let raw = input.read(16).map_err(|e| (0x5435ac, e))?;
                    let args: [u32; 4] = std::array::from_fn(|i| {
                        u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap())
                    });
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    audio
                        .sound_virtual(key.0, 0x5c, &args)
                        .map_err(|e| (0x5435ad, e.to_string()))?;
                    continue;
                }
                0x4066 => {
                    audio
                        .sound_virtual(key.0, 0x6c, &[1])
                        .map_err(|e| (0x5432d1, e.to_string()))?;
                }
                0xb200 | 0xb201 => {
                    let bytes = input
                        .read(field.payload.len().min(999))
                        .map_err(|e| (0x54365d, e))?;
                    let end = bytes.iter().position(|&b| b == 0).ok_or((
                        0x543671,
                        "unterminated sound link name outside original buffer".into(),
                    ))?;
                    let name = bytes[..end].to_vec();
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    let kind = if field.kind == 0xb200 {
                        SoundLinkKind::Sector
                    } else {
                        SoundLinkKind::Area
                    };
                    self.link_sound_or_defer(arena, sound, frame, name, kind)?;
                    continue;
                }
                _ => {} // 0x4067 and other tags take the original close/skip arm.
            }
            input.close_current().map_err(|e| (0x404ce0, e))?;
        }
        input.leave_container().map_err(|e| (0x54393f, e))?;
        self.object_fields.flags |= 0x40;
        audio
            .sound_virtual(key.0, 0x24, &[1])
            .map_err(|e| (0x54394f, e.to_string()))?;
        Ok(())
    }

    fn link_sound_or_defer(
        &mut self,
        arena: &mut ModelArena,
        sound: crate::frame_sound::SoundKey,
        reference: FrameReferenceId,
        name: Vec<u8>,
        kind: SoundLinkKind,
    ) -> Result<(), (u32, String)> {
        let target = self.load_map.map.slot(&name).flatten();
        if let Some(target) = target {
            let frame = FrameKey(target.identity());
            if arena.lookup_reference(frame) != target {
                return Err((0x5436d5, "stale sound link map frame".into()));
            }
            let type_matches = arena
                .state(frame)
                .map_err(|e| (0x5436d5, format!("{e:?}")))?
                .kind
                == match kind {
                    SoundLinkKind::Sector => FrameKind::Sector,
                    SoundLinkKind::Area => FrameKind::Area,
                };
            if type_matches {
                match kind {
                    SoundLinkKind::Sector => {
                        arena.add_sound_to_sector(crate::scene_ownership::SectorKey(frame), sound)
                    }
                    SoundLinkKind::Area => arena.add_sound_to_area(
                        arena
                            .area_key(frame)
                            .map_err(|e| (0x5438c6, format!("{e:?}")))?,
                        sound,
                    ),
                }
                .map_err(|e| {
                    (
                        if kind == SoundLinkKind::Sector {
                            0x5436e2
                        } else {
                            0x5438c6
                        },
                        format!("{e:?}"),
                    )
                })?;
                return Ok(());
            }
        }
        let pending = PendingSoundLink {
            sound: reference,
            name,
            kind,
        };
        let list = if kind == SoundLinkKind::Sector {
            &mut self.pending_sound_sector_links
        } else {
            &mut self.pending_sound_area_links
        };
        list.try_reserve(1)
            .map_err(|_| (0x548040, "sound link request allocation failed".into()))?;
        list.push(pending);
        Ok(())
    }

    fn read_parent(
        &mut self,
        arena: &ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        if self.current_frame.is_none() {
            return Ok(());
        }
        input.enter_current().map_err(|e| (0x54230d, e))?;
        let child = input
            .next_chunk()
            .map_err(|e| (0x5422cf, e))?
            .ok_or((0x5422cf, "short original parent nested header".into()))?;
        if child.kind == 0x10 {
            let bytes = input
                .read(child.payload.len().min(999))
                .map_err(|e| (0x542376, e))?;
            let end = bytes.iter().position(|&x| x == 0).ok_or((
                0x542387,
                "unterminated parent name outside admitted buffer domain".into(),
            ))?;
            let name = bytes[..end].to_vec();
            input.close_current().map_err(|e| (0x542390, e))?;
            self.object_fields.parent_name = Some(name.clone());
            let parent = self
                .load_map
                .map
                .lookup_or_insert_null(&name)
                .map_err(|e| (0x5423c4, e))?;
            if let Some(parent) = parent {
                let key = FrameKey(parent.identity());
                if arena.lookup_reference(key) != parent || arena.frame(key).is_err() {
                    return Err((0x5423e3, "stale parent map frame".into()));
                }
                self.object_fields.parent = Some(parent);
                self.object_fields.flags |= 0x10;
            } else if let Some(position) = self.object_fields.fallback_position {
                self.object_fields.position = Some(position);
            }
        } else {
            input.close_current().map_err(|e| (0x5424bb, e))?;
        }
        input.close_container().map_err(|e| (0x5424c0, e))?;
        Ok(())
    }
    fn read_dummy_bound_box(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        // Game5439bc reads+closes even when there is no frame or its type is
        // not DUMMY. Only the later Game546d10 helper writes the derived box.
        let raw = input.read(24).map_err(|e| (0x5439d6, e))?;
        let bounds = std::array::from_fn(|row| {
            std::array::from_fn(|axis| {
                let at = (row * 3 + axis) * 4;
                f32::from_bits(u32::from_le_bytes(raw[at..at + 4].try_into().unwrap()))
            })
        });
        input.close_current().map_err(|e| (0x5439df, e))?;
        if let Some(frame) = self.current_frame {
            let key = FrameKey(frame.identity());
            if arena.lookup_reference(key) != frame {
                return Err((0x5439e4, "stale dummy box frame".into()));
            }
            if arena
                .state(key)
                .map_err(|e| (0x5439f6, format!("{e:?}")))?
                .kind
                == FrameKind::Dummy
            {
                arena
                    .set_dummy_bound_box(key, bounds)
                    .map_err(|e| (0x543a06, format!("{e:?}")))?;
            }
        }
        Ok(())
    }
    fn read_area_geometry(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        if let Some(frame) = self.current_frame {
            let key = FrameKey(frame.identity());
            if arena.lookup_reference(key) != frame {
                return Err((0x543d4d, "stale AREA geometry frame".into()));
            }
            if arena
                .state(key)
                .map_err(|e| (0x543d4d, format!("{e:?}")))?
                .kind
                == FrameKind::Area
            {
                let count = input.read_u32().map_err(|e| (0x543d6c, e))?;
                self.pending_area_geometry = Some(MissionAreaGeometry {
                    vertex_count: count,
                    vertices: None,
                    face_count: None,
                    faces: None,
                });
                // Explicit host allocation quota, not a native count rejection.
                if count > 1_000_000 {
                    return Err((
                        0x543d7e,
                        "AREA temporary vertex allocation outside host quota".into(),
                    ));
                }
                let mut vertices = Vec::new();
                vertices.try_reserve_exact(count as usize).map_err(|_| {
                    (
                        0x543d7e,
                        "AREA temporary vertex allocation unavailable".into(),
                    )
                })?;
                vertices.resize(count as usize, None);
                self.pending_area_geometry.as_mut().unwrap().vertices = Some(vertices);
                let bytes = (count as usize)
                    .checked_mul(12)
                    .ok_or((0x543dc2, "AREA vertex read extent".into()))?;
                let raw = input.read(bytes).map_err(|e| (0x543dc2, e))?;
                for (destination, raw) in self
                    .pending_area_geometry
                    .as_mut()
                    .unwrap()
                    .vertices
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .zip(raw.as_chunks::<12>().0)
                {
                    *destination = Some(std::array::from_fn(|axis| {
                        u32::from_le_bytes(raw[axis * 4..axis * 4 + 4].try_into().unwrap())
                    }));
                }
                let count = input.read_u32().map_err(|e| (0x543ddc, e))?;
                self.pending_area_geometry.as_mut().unwrap().face_count = Some(count);
                if count > 1_000_000 {
                    return Err((
                        0x543dee,
                        "AREA temporary face allocation outside host quota".into(),
                    ));
                }
                let mut faces = Vec::new();
                faces.try_reserve_exact(count as usize).map_err(|_| {
                    (
                        0x543dee,
                        "AREA temporary face allocation unavailable".into(),
                    )
                })?;
                faces.resize(count as usize, None);
                self.pending_area_geometry.as_mut().unwrap().faces = Some(faces);
                let bytes = (count as usize)
                    .checked_mul(6)
                    .ok_or((0x543e0a, "AREA face read extent".into()))?;
                let raw = input.read(bytes).map_err(|e| (0x543e0a, e))?;
                for (destination, raw) in self
                    .pending_area_geometry
                    .as_mut()
                    .unwrap()
                    .faces
                    .as_mut()
                    .unwrap()
                    .iter_mut()
                    .zip(raw.as_chunks::<6>().0)
                {
                    *destination = Some(std::array::from_fn(|axis| {
                        u16::from_le_bytes(raw[axis * 2..axis * 2 + 2].try_into().unwrap())
                    }));
                }
                let pending = self.pending_area_geometry.as_ref().unwrap();
                let vertices = pending
                    .vertices
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|v| v.unwrap().map(f32::from_bits))
                    .collect::<Vec<_>>();
                let faces = pending
                    .faces
                    .as_ref()
                    .unwrap()
                    .iter()
                    .map(|f| f.unwrap())
                    .collect::<Vec<_>>();
                let area = arena
                    .area_key(key)
                    .map_err(|e| (0x543e24, format!("{e:?}")))?;
                let result = arena
                    .area_build(area, &vertices, &faces)
                    .map_err(|e| (0x543e24, format!("{e:?}")))?;
                self.object_fields.geometry_build_result = Some(result as u32);
                // Deleting the temporary vertices precedes deleting the faces.
                self.pending_area_geometry = None;
                self.object_fields.flags |= 0x100000;
            }
        }
        input.close_current().map_err(|e| (0x543e47, e))?;
        Ok(())
    }
    fn read_occluder_geometry(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        let Some(frame) = self.current_frame else {
            return Ok(());
        };
        let key = FrameKey(frame.identity());
        if arena.lookup_reference(key) != frame {
            return Err((0x543bb6, "stale occluder geometry frame".into()));
        }
        if arena
            .state(key)
            .map_err(|e| (0x543bb6, format!("{e:?}")))?
            .kind
            != FrameKind::Occluder
        {
            return Ok(());
        }
        let count = input.read_u32().map_err(|e| (0x543bd5, e))?;
        self.pending_occluder_geometry = Some(MissionOccluderGeometry {
            vertex_count: count,
            vertices: None,
            face_count: None,
            faces: None,
        });
        let bytes = usize::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(12))
            .ok_or((
                0x543be7,
                "occluder vertex extent outside host domain".into(),
            ))?;
        let raw = input.read(bytes).map_err(|e| (0x543c2b, e))?;
        let vertices = raw
            .as_chunks::<12>()
            .0
            .iter()
            .map(|raw| {
                std::array::from_fn(|n| {
                    u32::from_le_bytes(raw[n * 4..n * 4 + 4].try_into().unwrap())
                })
            })
            .collect::<Vec<_>>();
        self.pending_occluder_geometry.as_mut().unwrap().vertices = Some(vertices);
        let count = input.read_u32().map_err(|e| (0x543c45, e))?;
        self.pending_occluder_geometry.as_mut().unwrap().face_count = Some(count);
        let bytes = usize::try_from(count)
            .ok()
            .and_then(|n| n.checked_mul(6))
            .ok_or((0x543c57, "occluder face extent outside host domain".into()))?;
        let raw = input.read(bytes).map_err(|e| (0x543c73, e))?;
        let faces = raw
            .as_chunks::<6>()
            .0
            .iter()
            .map(|raw| {
                std::array::from_fn(|n| {
                    u16::from_le_bytes(raw[n * 2..n * 2 + 2].try_into().unwrap())
                })
            })
            .collect::<Vec<_>>();
        self.pending_occluder_geometry.as_mut().unwrap().faces = Some(faces);
        let pending = self.pending_occluder_geometry.as_ref().unwrap();
        let vertices = pending
            .vertices
            .as_ref()
            .unwrap()
            .iter()
            .map(|value| value.map(f32::from_bits))
            .collect::<Vec<_>>();
        let occluder = arena
            .occluder_key(key)
            .map_err(|e| (0x543c8d, format!("{e:?}")))?;
        let result = arena
            .occluder_build(occluder, &vertices, pending.faces.as_ref().unwrap())
            .map_err(|e| (0x543c8d, format!("{e:?}")))?;
        self.object_fields.geometry_build_result = Some(result);
        self.pending_occluder_geometry = None;
        self.object_fields.flags |= 0x200;
        Ok(())
    }
    /// Game543a10 first reads/closes the DWORD chunk through548490, before
    /// checking the current frame. A null/nonvisual frame takes its real skip.
    fn read_visual_subtype(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        let requested = input.read_u32().map_err(|e| (0x548490, e))?;
        self.object_fields.visual_subtype_request = Some(requested);
        input.close_current().map_err(|e| (0x54855d, e))?;
        let Some(frame) = self.current_frame else {
            return Ok(());
        };
        let key = FrameKey(frame.identity());
        if arena.lookup_reference(key) != frame {
            return Err((0x543a25, "stale original visual frame".into()));
        }
        let kind = arena
            .state(key)
            .map_err(|e| (0x543a25, format!("{e:?}")))?
            .kind;
        if !matches!(
            kind,
            FrameKind::Mesh | FrameKind::Projector | FrameKind::Singlemesh
        ) {
            return Ok(());
        }
        let current = arena
            .owned_object_visual_type(key)
            .map_err(|e| (0x543a35, format!("{e:?}")))?;
        self.object_fields.visual_subtype_current = Some(current);
        if current == requested {
            return Ok(());
        }
        let new = match arena.create_owned_object_visual(requested) {
            Ok(Some(new)) => new,
            Ok(None) => return Ok(()),
            Err(error) => {
                self.pending_visual_replacement =
                    error
                        .created
                        .map(|new| crate::visual_replacement::PendingVisualReplacement {
                            old: key,
                            new,
                            entry: 0x543a4b,
                        });
                return Err((0x543a4b, format!("{:?}", error.error)));
            }
        };
        self.pending_visual_replacement =
            Some(crate::visual_replacement::PendingVisualReplacement {
                old: key,
                new,
                entry: 0x543a5c,
            });
        // Host availability errors retain the real new allocation and creator
        // reference. They are not an invented negative native Duplicate result.
        arena
            .duplicate_owned_object_visual(new, key)
            .map_err(|e| (0x543a5c, format!("{e:?}")))?;
        self.pending_visual_replacement.as_mut().unwrap().entry = 0x543a76;
        let parent = arena
            .links(key)
            .map_err(|e| (0x543a69, format!("{e:?}")))?
            .parent;
        arena
            .link_frame(new, parent, 0)
            .map_err(|e| (0x543a76, format!("{e:?}")))?;
        self.pending_visual_replacement.as_mut().unwrap().entry = 0x543a94;
        while let Some(child) = arena
            .links(key)
            .map_err(|e| (0x543a7b, format!("{e:?}")))?
            .child
        {
            arena
                .link_frame(child, Some(new), 0)
                .map_err(|e| (0x543a94, format!("{e:?}")))?;
        }
        self.pending_visual_replacement.as_mut().unwrap().entry = 0x543aa6;
        arena
            .update_frame(new)
            .map_err(|e| (0x543aa6, format!("{e:?}")))?;
        self.pending_visual_replacement.as_mut().unwrap().entry = 0x543ab1;
        arena
            .link_frame(key, None, 0)
            .map_err(|e| (0x543ab1, format!("{e:?}")))?;
        self.pending_visual_replacement.as_mut().unwrap().entry = 0x543adc;
        let name = arena.frame(key).map_err(|e| (0x543ab6, format!("{e:?}")))?;
        let name = if name.name_is_null {
            Vec::new()
        } else {
            name.name_bytes.clone()
        };
        let reference = arena.lookup_reference(new);
        self.load_map
            .map
            .publish_loaded_frame(arena, &name, reference)
            .map_err(|e| (0x543adc, e))?;
        self.load_map.publications.push(LoadMapPublication {
            name,
            frame: reference,
        });
        let mut parent = arena
            .links(new)
            .map_err(|e| (0x543afb, format!("{e:?}")))?
            .parent;
        let mut owner = None;
        while let Some(ancestor) = parent {
            if arena
                .state(ancestor)
                .map_err(|e| (0x543b08, format!("{e:?}")))?
                .kind
                == FrameKind::Model
            {
                owner = Some(crate::model_loading::ModelKey(ancestor));
                break;
            }
            parent = arena
                .links(ancestor)
                .map_err(|e| (0x543afb, format!("{e:?}")))?
                .parent;
        }
        if let Some(model) = owner {
            self.pending_visual_replacement.as_mut().unwrap().entry = 0x543b16;
            arena
                .delete_frame(model, key)
                .map_err(|e| (0x543b16, format!("{e:?}")))?;
            self.pending_visual_replacement.as_mut().unwrap().entry = 0x543b1d;
            arena
                .add_frame(model, new)
                .map_err(|e| (0x543b1d, format!("{e:?}")))?;
        } else {
            self.pending_visual_replacement.as_mut().unwrap().entry = 0x543b2d;
            arena
                .scene_delete_frame(self.scene, key)
                .map_err(|e| (0x543b2d, format!("{e:?}")))?;
            self.pending_visual_replacement.as_mut().unwrap().entry = 0x543b37;
            arena
                .scene_add_frame(self.scene, new)
                .map_err(|e| (0x543b37, format!("{e:?}")))?;
        }
        self.current_frame = Some(reference);
        self.object_fields.flags |= 0x400;
        self.pending_visual_replacement.as_mut().unwrap().entry = 0x543b4e;
        arena
            .release(new)
            .map_err(|e| (0x543b4e, format!("{e:?}")))?;
        self.pending_visual_replacement = None;
        Ok(())
    }
    /// Game543b55..543ba7 ignores the native Load return and sets bit0x400.
    /// Host availability failures retain the consumed input/resource prefix and
    /// remain explicit; they do not become invented native negative results.
    fn read_saved_lightmaps(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        let version = self.version[0].ok_or((
            0x543b55,
            "saved lighting requires original version header".into(),
        ))?;
        if version as i16 >= 3 {
            if let Some(frame) = self.current_frame {
                let key = FrameKey(frame.identity());
                if arena.lookup_reference(key) != frame {
                    return Err((0x543b66, "stale saved-lighting frame".into()));
                }
                let kind = arena
                    .state(key)
                    .map_err(|e| (0x543b66, format!("{e:?}")))?
                    .kind;
                if matches!(
                    kind,
                    FrameKind::Mesh | FrameKind::Projector | FrameKind::Singlemesh
                ) && arena
                    .owned_object_visual_type(key)
                    .map_err(|e| (0x543b72, format!("{e:?}")))?
                    == crate::visual_replacement::LIT_OBJECT_VISUAL
                {
                    let object = arena
                        .frame_object_resources(key)
                        .map_err(|e| (0x543b8d, format!("{e:?}")))?
                        .ok_or((0x543b8d, "missing saved-lighting resource owner".into()))?;
                    let field = input
                        .current()
                        .ok_or((0x543b86, "missing saved-lighting field".into()))?;
                    let result = arena.visual_resource_owner_mut().map_err(|e| (0x543b8d, format!("{e:?}")))?
                        .load_saved_lightmaps_native(object, field.payload,
                            crate::loaded_visual_resources::SavedLightmapLoadContext::TrueColorRgba8);
                    let consumed = match &result {
                        Ok(outcome) => outcome.consumed,
                        Err(failure) => failure.consumed,
                    };
                    self.object_fields.saved_lightmap_consumed = Some(consumed);
                    input.read(consumed).map_err(|e| (0x543b8d, e))?;
                    match result {
                        Ok(outcome) => {
                            self.object_fields.saved_lightmap_result =
                                Some(outcome.native_result as u32);
                            self.object_fields.flags |= 0x400;
                        }
                        Err(failure) => return Err((0x543b8d, format!("{:?}", failure.error))),
                    }
                }
            }
        }
        input.close_current().map_err(|e| (0x543ba2, e))?;
        Ok(())
    }

    fn read_model_open(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
        resource_open: &mut Option<&mut ResourceOpenAdapter<'_>>,
    ) -> Result<(), (u32, String)> {
        let Some(frame) = self.current_frame else {
            return Ok(());
        };
        let key = FrameKey(frame.identity());
        if arena.lookup_reference(key) != frame {
            return Err((0x541fa1, "stale original model frame".into()));
        }
        if arena
            .state(key)
            .map_err(|e| (0x541fa9, format!("{e:?}")))?
            .kind
            != FrameKind::Model
        {
            return Ok(());
        }
        let field = input
            .current()
            .ok_or((0x541fff, "missing current model filename field".into()))?;
        let bytes = input
            .read(field.payload.len().min(999))
            .map_err(|e| (0x541fff, e))?;
        let end = bytes.iter().position(|&x| x == 0).ok_or((
            0x542010,
            "unterminated model name outside admitted buffer domain".into(),
        ))?;
        let name = bytes[..end].to_vec();
        input.close_current().map_err(|e| (0x542019, e))?;
        if name.is_empty() {
            return Err((
                0x542081,
                "original empty-model SceneRemoveFrame/refcount branch unavailable".into(),
            ));
        }
        let mut cache_name = name.clone();
        if let Some(dot) = cache_name.iter().rposition(|&x| x == b'.') {
            cache_name.truncate(dot);
        }
        cache_name.extend_from_slice(b".i3d");
        self.pending_model_open = Some(MissionModelOpenRequest {
            model: frame,
            input_name: name,
            cache_name,
        });
        let Some(open) = resource_open.as_mut() else {
            return Err((
                0x54220a,
                "original C_I3D_model_cache::Open4087e0 resource loading unavailable".into(),
            ));
        };
        let model = crate::model_loading::ModelKey(key);
        let requested = &self.pending_model_open.as_ref().unwrap().cache_name;
        let result = open(arena, model.root(), requested, ResourceOpenKind::Model)
            .map_err(|e| (0x54220a, e))?;
        if result >= 0 {
            let mut stack = Vec::new();
            if let Some(child) = arena
                .links(key)
                .map_err(|e| (0x54222c, format!("{e:?}")))?
                .child
            {
                stack.push(child);
            }
            let mut visited = HashSet::new();
            while let Some(frame) = stack.pop() {
                if !visited.insert(frame.0) || visited.len() > 65_536 {
                    return Err((0x54222c, "invalid model enumeration hierarchy".into()));
                }
                let links = arena
                    .links(frame)
                    .map_err(|e| (0x54222c, format!("{e:?}")))?;
                publish(arena, frame, &mut self.load_map).map_err(|e| (0x5455e0, e))?;
                if let Some(next) = links.next {
                    stack.push(next);
                }
                if let Some(child) = links.child {
                    stack.push(child);
                }
            }
        }
        self.pending_model_open = None;
        Ok(())
    }
    fn read_light(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        let Some(frame) = self.current_frame else {
            return Ok(());
        };
        let key = FrameKey(frame.identity());
        if arena.lookup_reference(key) != frame {
            return Err((0x542554, "stale original light frame".into()));
        }
        if arena
            .state(key)
            .map_err(|e| (0x542554, format!("{e:?}")))?
            .kind
            != FrameKind::Light
        {
            return Ok(());
        }
        arena
            .release_light_sectors(key)
            .map_err(|e| (0x542579, format!("{e:?}")))?;
        input.enter_current().map_err(|e| (0x54254c, e))?;
        while let Some(field) = input.next_chunk().map_err(|e| (0x5425d4, e))? {
            match field.kind {
                0x4041 | 0x4042 | 0x4045 => {
                    let entry = match field.kind {
                        0x4041 => 0x542689,
                        0x4042 => 0x542659,
                        _ => 0x5429fa,
                    };
                    let mut word = input.read_u32().map_err(|e| (entry, e))?;
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    match field.kind {
                        0x4041 => arena
                            .graph
                            .set_light_type(key, word)
                            .map_err(|e| (0x5426a3, format!("{e:?}")))?,
                        0x4042 => arena
                            .graph
                            .set_light_diffuse_power(key, f32::from_bits(word))
                            .map_err(|e| (0x542673, format!("{e:?}")))?,
                        _ => {
                            let major = self.version[0].ok_or((
                                0x542a08,
                                "light mode requires original version header".into(),
                            ))? as i16;
                            let minor = self.version[1].ok_or((
                                0x542a1d,
                                "light mode requires original version header".into(),
                            ))? as i16;
                            if i32::from(major)
                                .wrapping_mul(100)
                                .wrapping_add(i32::from(minor))
                                < 510
                            {
                                word |= 0x40;
                            }
                            arena
                                .graph
                                .set_light_mode(key, word)
                                .map_err(|e| (0x542a38, format!("{e:?}")))?;
                        }
                    }
                    continue;
                }
                0x26 => {
                    let bytes = input.read(12).map_err(|e| (0x5426b9, e))?;
                    let color = std::array::from_fn(|n| {
                        f32::from_bits(u32::from_le_bytes(
                            bytes[n * 4..n * 4 + 4].try_into().unwrap(),
                        ))
                    });
                    input.close_current().map_err(|e| (0x5426c2, e))?;
                    arena
                        .graph
                        .set_light_diffuse(key, color)
                        .map_err(|e| (0x5426fd, format!("{e:?}")))?;
                    continue;
                }
                0x4043 | 0x4044 => {
                    let bytes = input.read(8).map_err(|e| {
                        (
                            if field.kind == 0x4043 {
                                0x54276a
                            } else {
                                0x542aa5
                            },
                            e,
                        )
                    })?;
                    let mut values: [f32; 2] = std::array::from_fn(|n| {
                        f32::from_bits(u32::from_le_bytes(
                            bytes[n * 4..n * 4 + 4].try_into().unwrap(),
                        ))
                    });
                    input.close_current().map_err(|e| (0x404ce0, e))?;
                    if field.kind == 0x4043 {
                        arena
                            .graph
                            .set_light_cone(key, values[0], values[1])
                            .map_err(|e| (0x542804, format!("{e:?}")))?;
                    } else {
                        if values[0] <= f32::from_bits(0x322bcc77) {
                            values[0] = f32::from_bits(0x3dcccccd);
                        }
                        if values[1] <= values[0] {
                            values[1] = crate::native_matrix::checked_f32_add(
                                values[0],
                                f32::from_bits(0x3dcccccd),
                            )
                            .map_err(|e| (0x542b66, format!("{e:?}")))?;
                        }
                        arena
                            .graph
                            .set_light_range(key, values[0], values[1])
                            .map_err(|e| (0x542b87, format!("{e:?}")))?;
                    }
                    continue;
                }
                0x4046 => {
                    let bytes = input
                        .read(field.payload.len().min(999))
                        .map_err(|e| (0x5428f8, e))?;
                    let end = bytes.iter().position(|&x| x == 0).ok_or((
                        0x542909,
                        "unterminated light sector name outside admitted buffer domain".into(),
                    ))?;
                    let name = bytes[..end].to_vec();
                    input.close_current().map_err(|e| (0x542912, e))?;
                    let target = self
                        .load_map
                        .map
                        .lookup_or_insert_null(&name)
                        .map_err(|e| (0x542946, e))?;
                    let sector = if let Some(target) = target {
                        let sector = FrameKey(target.identity());
                        if arena.lookup_reference(sector) != target {
                            return Err((0x542965, "stale original light sector reference".into()));
                        }
                        (arena
                            .state(sector)
                            .map_err(|e| (0x54296c, format!("{e:?}")))?
                            .kind
                            == FrameKind::Sector)
                            .then_some(sector)
                    } else {
                        None
                    };
                    if let Some(sector) = sector {
                        arena
                            .sector_add_light(crate::scene_ownership::SectorKey(sector), key)
                            .map_err(|e| (0x542979, format!("{e:?}")))?;
                    } else {
                        let mut name = name;
                        name.push(0);
                        self.pending_light_sector_link =
                            Some(PendingLightSectorLink { light: frame, name });
                        return Err((
                            0x5429e0,
                            "original deferred light-sector request collection unavailable".into(),
                        ));
                    }
                    continue;
                }
                _ => {} // Original default closes this unknown nested field.
            }
            input.close_current().map_err(|e| (0x5428a5, e))?;
        }
        input.leave_container().map_err(|e| (0x542b99, e))?;
        self.object_fields.flags |= 0x20;
        Ok(())
    }
    fn read_sector_sound(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
    ) -> Result<(), (u32, String)> {
        let Some(frame) = self.current_frame else {
            return Ok(());
        };
        let key = FrameKey(frame.identity());
        if arena.lookup_reference(key) != frame {
            return Err((0x54431e, "stale sector sound frame".into()));
        }
        if arena
            .state(key)
            .map_err(|e| (0x54431e, format!("{e:?}")))?
            .kind
            != FrameKind::Sector
        {
            return Ok(());
        }
        input.enter_current().map_err(|e| (0x54432b, e))?;
        while let Some(field) = input.next_chunk().map_err(|e| (0x544378, e))? {
            let data = if matches!(field.kind, 20 | 21) {
                {
                    let data = input.read(12).map_err(|e| (0x54443b, e))?;
                    SectorSoundInput::Pan(std::array::from_fn(|i| {
                        u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap())
                    }))
                }
            } else {
                SectorSoundInput::Scalar(input.read_u32().map_err(|e| (0x5444a7, e))?)
            };
            let _native_hresult = arena
                .set_sector_sound_data(
                    crate::scene_ownership::SectorKey(key),
                    i32::from(field.kind),
                    data,
                )
                .map_err(|e| (0x5444b9, format!("{e:?}")))?;
            input.close_current().map_err(|e| (0x544540, e))?;
        }
        input.leave_container().map_err(|e| (0x54454a, e))?;
        // The caller's enclosing close_current is not needed after entering a
        // container: its actual extent has already been consumed and closed.
        Ok(())
    }
}

fn game_camera_projection<S: SceneScreen>(
    arena: &mut ModelArena,
    camera: CameraKey,
    screen: &mut S,
    projection: GameCameraProjection,
    input: f32,
) -> Result<(), String>
where
    S::Error: std::fmt::Debug,
{
    let fov = projection.fov(input)?;
    arena
        .camera_set_fov(camera, fov)
        .map_err(|e| format!("{e:?}"))?;
    let width = screen.screen_sx().map_err(|e| format!("{e:?}"))?;
    let width = GameCameraProjection::width_single(width);
    let height = screen.screen_sy().map_err(|e| format!("{e:?}"))?;
    let aspect = projection.aspect(width, height);
    arena
        .camera_set_aspect_ratio(camera, aspect)
        .map_err(|e| format!("{e:?}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver_scene::{DriverSceneContext, DriverSceneScreen, NativeSceneSoundDriver};
    use crate::itexture::{
        CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources,
    };
    struct Screen {
        calls: Vec<&'static str>,
    }
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sy(&mut self) -> Result<i32, ()> {
            self.calls.push("sy");
            Ok(600)
        }
        fn screen_sx(&mut self) -> Result<i32, ()> {
            self.calls.push("sx");
            Ok(800)
        }
    }
    impl DriverSceneScreen for Screen {
        fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
            Ok([0, 0, 800, 600])
        }
    }
    struct MissingImages;
    impl TextureImageSource for MissingImages {
        fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
            Err(TextureImageError::Missing)
        }
    }
    fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((payload.len() + 6) as u32).to_le_bytes(),
            payload,
        ]
        .concat()
    }
    fn prefix_input() -> Vec<u8> {
        let sector = chunk(
            0x4010,
            &[
                chunk(0x10, b"Primary sector\0"),
                chunk(
                    0xb401,
                    &[
                        chunk(0, &0.5f32.to_bits().to_le_bytes()),
                        chunk(22, &0x12345678u32.to_le_bytes()),
                        chunk(
                            20,
                            &[
                                1f32.to_bits().to_le_bytes(),
                                2f32.to_bits().to_le_bytes(),
                                3f32.to_bits().to_le_bytes(),
                            ]
                            .concat(),
                        ),
                    ]
                    .concat(),
                ),
            ]
            .concat(),
        );
        let blocked = chunk(
            0x4010,
            &[
                chunk(0x4011, &10u32.to_le_bytes()),
                chunk(0x10, b"blocked\0"),
                chunk(0x20, &[0; 12]),
            ]
            .concat(),
        );
        chunk(
            0x4c53,
            &[
                chunk(1, &[5u16.to_le_bytes(), 1u16.to_le_bytes()].concat()),
                chunk(
                    0xafff,
                    &[0x11223344u32.to_le_bytes(), 0x55667788u32.to_le_bytes()].concat(),
                ),
                chunk(0x3200, &[1; 12]),
                chunk(0x3010, &1f32.to_bits().to_le_bytes()),
                chunk(0x3011, &500f32.to_bits().to_le_bytes()),
                chunk(
                    0x3211,
                    &[
                        0.1f32.to_bits().to_le_bytes(),
                        1000f32.to_bits().to_le_bytes(),
                    ]
                    .concat(),
                ),
                chunk(0x4000, &[sector, blocked].concat()),
            ]
            .concat(),
        )
    }
    #[test]
    fn ordered_prefix_uses_live_driver_camera_sector_and_retains_factory_boundary() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut screen = Screen { calls: Vec::new() };
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            screen.calls.clear();
            let mut loader = match MissionSceneLoader::new(&arena, scene) {
                Ok(x) => x,
                Err(e) => panic!("{}", e.detail),
            };
            let bytes = prefix_input();
            let error = loader
                .load_prefix(
                    &mut driver,
                    &mut arena,
                    &resources,
                    &mut screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            let end_name = bytes.windows(8).position(|v| v == b"blocked\0").unwrap() + 8;
            assert_eq!(error.cursor, end_name, "{error:?}");
            assert_eq!(error.entry, 0x541d05);
            assert_eq!(loader.current_kind, 10);
            assert_eq!(loader.current_name, b"blocked");
            assert_eq!(loader.load_map.map.slot(b"blocked"), Some(None));
            assert_eq!(loader.finished_objects, 1);
            assert_eq!(loader.version, [Some(5), Some(1)]);
            assert_eq!(loader.mission_words, [Some(0x11223344), Some(0x55667788)]);
            assert_eq!(loader.mission_far_range, Some(500f32.to_bits()));
            assert_eq!(arena.scene_field_220(scene).unwrap(), [0; 3]);
            assert_eq!(arena.scene_backdrop_range(scene).unwrap(), [0.1, 3000.]);
            let primary = arena.scene(scene).unwrap().sectors[0];
            assert_eq!(
                arena.sector(primary).unwrap().sound_properties[0],
                0.5f32.to_bits()
            );
            assert_ne!(arena.sector(primary).unwrap().sector_flags & 0x800, 0);
            let camera = loader.camera.unwrap();
            assert_eq!(arena.scene_active_camera(scene).unwrap(), Some(camera));
            assert_eq!(arena.frame(camera.frame()).unwrap().references, 1);
            assert_eq!(
                arena.frame(camera.frame()).unwrap().name_bytes,
                b"GameCamera"
            );
            assert_eq!(loader.load_map.map.slot(b"GameCamera"), None);
            assert_eq!(screen.calls, ["sx", "sy", "sx", "sy", "sx", "sy"]);
            resources
                .close_constructed_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            assert_eq!(arena.frame(camera.frame()).unwrap().references, 1);
            assert_eq!(arena.scene_active_camera(scene).unwrap(), Some(camera));
            assert_eq!(driver.scenes(), [scene]);
            assert_eq!(resources.ws_ref_count(), 1);
            driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    scene,
                )
                .unwrap();
            assert!(driver.scenes().is_empty());
            assert!(arena.frame(camera.frame()).is_err());
            assert_eq!(resources.ws_ref_count(), 0);
            driver.release_empty_registry_storage().unwrap();
        });
    }
    #[test]
    fn short_pan_and_backdrop_keep_the_original_atomic_read_cursor() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            for pan in [false, true] {
                let mut arena = ModelArena::default();
                let mut resources = SceneResources::default();
                let mut textures = TextureResources::new().unwrap();
                let mut storage = CpuTextureStorage::default();
                let mut driver = DriverSceneOwner::default();
                let mut screen = Screen { calls: vec![] };
                let scene = driver
                    .create_scene(
                        &mut DriverSceneContext {
                            arena: &mut arena,
                            resources: &mut resources,
                            textures: &mut textures,
                            storage: &mut storage,
                        },
                        &mut screen,
                        &mut MissingImages,
                        NativeSceneSoundDriver::Absent,
                    )
                    .unwrap();
                let primary = arena.scene(scene).unwrap().sectors[0];
                let before = arena.sector(primary).unwrap().sound_properties;
                let payload = if pan {
                    chunk(
                        0x4000,
                        &chunk(
                            0x4010,
                            &[
                                chunk(0x10, b"Primary sector\0"),
                                chunk(0xb401, &chunk(20, &[0; 8])),
                            ]
                            .concat(),
                        ),
                    )
                } else {
                    chunk(0x3211, &[0; 4])
                };
                let bytes = chunk(0x4c53, &payload);
                let mut loader = match MissionSceneLoader::new(&arena, scene) {
                    Ok(x) => x,
                    Err(e) => panic!("{}", e.detail),
                };
                let error = loader
                    .load_prefix(
                        &mut driver,
                        &mut arena,
                        &resources,
                        &mut screen,
                        GameCameraProjection {
                            global_mode: 0,
                            screen_flag_46f4: false,
                        },
                        &bytes,
                    )
                    .unwrap_err();
                assert_eq!(error.cursor, bytes.len() - if pan { 8 } else { 4 });
                assert_eq!(error.entry, if pan { 0x54443b } else { 0x544715 });
                assert_eq!(arena.sector(primary).unwrap().sound_properties, before);
                resources
                    .close_constructed_scene(&mut arena, scene, &mut textures, &mut storage)
                    .unwrap();
                driver
                    .release_scene(
                        &mut DriverSceneContext {
                            arena: &mut arena,
                            resources: &mut resources,
                            textures: &mut textures,
                            storage: &mut storage,
                        },
                        scene,
                    )
                    .unwrap();
                driver.release_empty_registry_storage().unwrap();
            }
        });
    }

    #[test]
    fn original_initializer_and_callback_eight_native_schedules() {
        // Synthetic normalized identities from unchanged native block/callback
        // execution, including actual scene/frame/subtree enumeration at fixed
        // flags4ffff. Native allocation and tree insertion were controlled.
        let expected: [&[(usize, &[u8])]; 8] = [
            &[
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"duplicate"),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (4, b"duplicate"),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"duplicate"),
                (4, b"duplicate"),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"Primary sector"),
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (4, b"duplicate"),
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
            &[
                (3, b"Primary sector"),
                (4, b"duplicate"),
                (5, b""),
                (0, b"Primary sector"),
                (1, b"Backdrop sector"),
                (2, b"Omni sector"),
            ],
        ];
        for (case, expected) in expected.iter().enumerate() {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let sectors = arena.scene(scene).unwrap().sectors;
            let frames = [
                sectors[0].frame(),
                sectors[1].frame(),
                sectors[2].frame(),
                arena.create_model().unwrap().root(),
                arena.create_model().unwrap().root(),
                arena.create_model().unwrap().root(),
            ];
            arena
                .set_frame_name(
                    frames[3],
                    Some(if case >= 4 {
                        b"Primary sector"
                    } else {
                        b"duplicate"
                    }),
                )
                .unwrap();
            arena.set_frame_name(frames[4], Some(b"duplicate")).unwrap();
            arena
                .set_frame_name(frames[5], if case % 2 == 1 { None } else { Some(b"") })
                .unwrap();
            for index in (0..3).rev() {
                if case & (1 << index) != 0 {
                    arena
                        .link_frame(
                            frames[index + 3],
                            Some(sectors[usize::from(index == 2)].frame()),
                            0,
                        )
                        .unwrap();
                }
            }
            let result = match initialize_scene_load_map(&arena, scene) {
                Ok(x) => x,
                Err(e) => panic!("{}", e.detail),
            };
            let actual = result
                .publications
                .iter()
                .map(|p| (p.frame, p.name.as_slice()))
                .collect::<Vec<_>>();
            let expected = expected
                .iter()
                .map(|(id, name)| (arena.lookup_reference(frames[*id]), *name))
                .collect::<Vec<_>>();
            assert_eq!(actual, expected, "native schedule {case}");
            for (_, name) in expected.iter().rev() {
                let last = expected
                    .iter()
                    .rev()
                    .find(|(_, key)| key == name)
                    .unwrap()
                    .0;
                assert_eq!(result.map.slot(name), Some(Some(last)));
            }
        }
    }

    #[test]
    fn existing_scene_hierarchy_publication_and_sector_overwrite() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let a = arena.create_model().unwrap().root();
        let b = arena.create_model().unwrap().root();
        let c = arena.create_model().unwrap().root();
        let omitted = arena.create_model().unwrap().root();
        arena.set_frame_name(a, Some(b"Primary sector")).unwrap();
        arena.set_frame_name(b, Some(b"duplicate")).unwrap();
        arena.set_frame_name(c, Some(b"duplicate")).unwrap();
        arena.set_frame_name(omitted, Some(b"omitted")).unwrap();
        arena.link_frame(a, Some(sectors[0].frame()), 0).unwrap();
        arena.link_frame(b, Some(a), 0).unwrap();
        arena.link_frame(c, Some(sectors[1].frame()), 0).unwrap();
        arena
            .link_frame(omitted, Some(sectors[2].frame()), 0)
            .unwrap();
        let result = match initialize_scene_load_map(&arena, scene) {
            Ok(result) => result,
            Err(error) => panic!("{}", error.detail),
        };
        assert_eq!(
            result
                .publications
                .iter()
                .map(|x| x.name.as_slice())
                .collect::<Vec<_>>(),
            [
                b"Primary sector".as_slice(),
                b"duplicate",
                b"duplicate",
                b"Primary sector",
                b"Backdrop sector",
                b"Omni sector"
            ]
        );
        assert_eq!(
            result.map.slot(b"duplicate"),
            Some(Some(arena.lookup_reference(c)))
        );
        assert_eq!(
            result.map.slot(b"Primary sector"),
            Some(Some(arena.lookup_reference(sectors[0].frame())))
        );
        assert_eq!(result.map.slot(b"omitted"), None);
        assert_eq!(result.map.slot(b"Near sector"), None);
    }

    #[test]
    fn null_and_allocated_empty_full_names_share_the_empty_key() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let null = arena.create_model().unwrap().root();
        let empty = arena.create_model().unwrap().root();
        arena.set_frame_name(empty, Some(b"")).unwrap();
        arena.link_frame(null, Some(sectors[0].frame()), 0).unwrap();
        arena
            .link_frame(empty, Some(sectors[1].frame()), 0)
            .unwrap();
        let result = match initialize_scene_load_map(&arena, scene) {
            Ok(result) => result,
            Err(error) => panic!("{}", error.detail),
        };
        assert_eq!(result.publications[0].name, b"");
        assert_eq!(result.publications[1].name, b"");
        assert_eq!(
            result.map.slot(b""),
            Some(Some(arena.lookup_reference(empty)))
        );
    }

    #[test]
    fn unsupported_name_preserves_already_published_live_identity() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        let good = arena.create_model().unwrap().root();
        let unsupported = arena.create_model().unwrap().root();
        arena.set_frame_name(good, Some(b"retained")).unwrap();
        arena.set_frame_name(unsupported, Some(&[0xe8])).unwrap();
        arena.link_frame(good, Some(sectors[0].frame()), 0).unwrap();
        arena
            .link_frame(unsupported, Some(sectors[1].frame()), 0)
            .unwrap();
        let error = match initialize_scene_load_map(&arena, scene) {
            Ok(_) => panic!("unsupported host map name accepted"),
            Err(error) => error,
        };
        assert_eq!(error.prefix.publications.len(), 1);
        assert_eq!(
            error.prefix.map.slot(b"retained"),
            Some(Some(arena.lookup_reference(good)))
        );
        assert_eq!(error.prefix.map.slot(b"Primary sector"), None);
        assert!(error.detail.contains("name domain"));
    }
    fn with_scene(
        test: impl FnOnce(
            &mut DriverSceneOwner,
            &mut ModelArena,
            &SceneResources,
            &mut Screen,
            SceneKey,
        ),
    ) {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut screen = Screen { calls: Vec::new() };
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            test(&mut driver, &mut arena, &resources, &mut screen, scene);
            resources
                .close_constructed_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    scene,
                )
                .unwrap();
            assert!(driver.scenes().is_empty());
            assert_eq!(resources.ws_ref_count(), 0);
        });
    }
    fn words(values: &[f32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|x| x.to_bits().to_le_bytes())
            .collect()
    }
    fn tetrahedron() -> Vec<u8> {
        let vertices = words(&[0., 0., 0., 2., 0., 0., 0., 3., 0., 0., 0., 4.]);
        let faces = [0u16, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect::<Vec<_>>();
        [
            4u32.to_le_bytes().as_slice(),
            &vertices,
            4u32.to_le_bytes().as_slice(),
            &faces,
        ]
        .concat()
    }
    fn new_object(kind: u32, name: &[u8], fields: &[u8]) -> Vec<u8> {
        chunk(
            0x4010,
            &[
                chunk(0x4011, &kind.to_le_bytes()),
                chunk(0x10, name),
                fields.to_vec(),
            ]
            .concat(),
        )
    }
    fn input_objects(objects: &[u8]) -> Vec<u8> {
        chunk(
            0x4c53,
            &[
                chunk(1, &[5u16.to_le_bytes(), 1u16.to_le_bytes()].concat()),
                chunk(0x4000, objects),
            ]
            .concat(),
        )
    }
    #[test]
    fn sound_fields_and_forward_area_link_use_live_owners_in_file_order() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let audio = crate::runtime_audio::RuntimeAudio::default();
            driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
            let mut screen = Screen { calls: Vec::new() };
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                )
                .unwrap();
            let nested = chunk(
                0x4060,
                &[
                    chunk(0x4061, &1u32.to_le_bytes()),
                    chunk(0x4062, &0.75f32.to_bits().to_le_bytes()),
                    chunk(0x4065, &words(&[2.0, 30.0, 0.5])),
                    chunk(0x4066, &[]),
                    chunk(0xb200, b"Primary sector\0"),
                    chunk(0xb201, b"future\0"),
                    chunk(0x4067, &[]),
                ]
                .concat(),
            );
            let bytes = input_objects(
                &[
                    new_object(4, b"source sound\0", &nested),
                    new_object(14, b"future\0", &[]),
                ]
                .concat(),
            );
            let mut loader = MissionSceneLoader::new(&arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            loader
                .load_prefix(
                    &mut driver,
                    &mut arena,
                    &resources,
                    &mut screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap();
            assert_eq!(loader.finished_objects, 2);
            assert!(loader.pending_sound_area_links.is_empty());
            let sound_frame = FrameKey(
                loader
                    .load_map
                    .map
                    .slot(b"source sound")
                    .flatten()
                    .unwrap()
                    .identity(),
            );
            let area_frame = FrameKey(
                loader
                    .load_map
                    .map
                    .slot(b"future")
                    .flatten()
                    .unwrap()
                    .identity(),
            );
            let sound = arena.sound_key(sound_frame).unwrap();
            let area = arena.area_key(area_frame).unwrap();
            assert_eq!(arena.sound(sound).unwrap().areas(), &[area]);
            assert_eq!(arena.area(area).unwrap().sounds, [sound_frame]);
            assert_eq!(audio.sound(sound_frame.0).unwrap().volume, 0.75);
            assert_eq!(
                audio.sound(sound_frame.0).unwrap().range,
                [2.0, 30.0, 0.5, 1.0]
            );
            assert!(audio.sound(sound_frame.0).unwrap().looping);
            resources
                .close_constructed_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    scene,
                )
                .unwrap();
            assert_eq!(audio.live_sound_count().unwrap(), 0);
        });
    }
    #[test]
    fn actor_boundary_retains_unresolved_sound_requests_without_early_link_pass() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let audio = crate::runtime_audio::RuntimeAudio::default();
            driver.bind_host_sound_backend(&mut arena, &audio).unwrap();
            let mut screen = Screen { calls: Vec::new() };
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Initialized,
                )
                .unwrap();
            let sound = new_object(
                4,
                b"pending sound\0",
                &chunk(0x4060, &chunk(0xb201, b"future area\0")),
            );
            let bytes = chunk(
                0x4c53,
                &[
                    chunk(1, &[5u16.to_le_bytes(), 1u16.to_le_bytes()].concat()),
                    chunk(0x4000, &sound),
                    chunk(
                        0xae20,
                        &chunk(
                            0xae21,
                            &[
                                chunk(0xae23, b"pending sound\0"),
                                chunk(0xae22, &2u32.to_le_bytes()),
                            ]
                            .concat(),
                        ),
                    ),
                ]
                .concat(),
            );
            let mut loader = MissionSceneLoader::new(&arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            let error = loader
                .load_prefix(
                    &mut driver,
                    &mut arena,
                    &resources,
                    &mut screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            assert_eq!(error.entry, 0x544a16);
            assert_eq!(loader.finished_objects, 1);
            assert_eq!(loader.pending_sound_area_links.len(), 1);
            assert_eq!(loader.pending_sound_area_links[0].name, b"future area");
            let frame = FrameKey(
                loader
                    .load_map
                    .map
                    .slot(b"pending sound")
                    .flatten()
                    .unwrap()
                    .identity(),
            );
            assert!(arena
                .sound(arena.sound_key(frame).unwrap())
                .unwrap()
                .areas()
                .is_empty());
            resources
                .close_constructed_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    scene,
                )
                .unwrap();
            assert_eq!(audio.live_sound_count().unwrap(), 0);
        });
    }
    #[test]
    fn dummy_and_camera_use_real_factory_owners_and_derived_box() {
        with_scene(|driver, arena, resources, screen, scene| {
            let parent = chunk(0x4020, &chunk(0x10, b"Primary sector\0"));
            let box_words = [[-2., -3., -4.], [2., 3., 4.]];
            let objects = [
                new_object(
                    6,
                    b"fixture dummy\0",
                    &[
                        parent.clone(),
                        chunk(0x4050, &words(&box_words.concat())),
                        chunk(0x20, &words(&[7., 8., 9.])),
                    ]
                    .concat(),
                ),
                new_object(3, b"fixture camera\0", &parent),
            ]
            .concat();
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &input_objects(&objects),
                )
                .unwrap();
            assert_eq!(loader.finished_objects, 2);
            assert_eq!(loader.pending_factory_creator, None);
            let dummy = FrameKey(
                loader
                    .load_map
                    .map
                    .slot(b"fixture dummy")
                    .unwrap()
                    .unwrap()
                    .identity(),
            );
            assert_eq!(arena.state(dummy).unwrap().kind, FrameKind::Dummy);
            assert_eq!(arena.dummy_bound_box(dummy).unwrap(), box_words);
            assert_eq!(arena.frame(dummy).unwrap().references, 1);
            assert_eq!(
                arena.frame(dummy).unwrap().owner,
                Some(crate::scene_ownership::FrameOwner::Scene(scene))
            );
            assert_eq!(
                &arena.graph.world_matrix(dummy).unwrap().elements[12..15],
                &[7., 8., 9.]
            );
            let camera = FrameKey(
                loader
                    .load_map
                    .map
                    .slot(b"fixture camera")
                    .unwrap()
                    .unwrap()
                    .identity(),
            );
            assert_eq!(arena.state(camera).unwrap().kind, FrameKind::Camera);
            assert_eq!(arena.frame(camera).unwrap().references, 1);
            assert_ne!(camera, loader.camera.unwrap().frame());
        });
    }
    #[test]
    fn dummy_box_reads_and_closes_before_null_or_wrong_class_guard() {
        for with_frame in [false, true] {
            with_scene(|driver, arena, resources, screen, scene| {
                let name = if with_frame {
                    b"Primary sector\0".as_slice()
                } else {
                    b"missing\0".as_slice()
                };
                let before = *arena
                    .state(arena.scene(scene).unwrap().sectors[0].frame())
                    .unwrap();
                let objects = new_object(
                    0,
                    name,
                    &[
                        chunk(0x4050, &words(&[-2., -3., -4., 2., 3., 4.])),
                        chunk(0xf123, &[9, 8, 7]),
                    ]
                    .concat(),
                );
                let bytes = input_objects(&objects);
                let mut loader = MissionSceneLoader::new(arena, scene)
                    .map_err(|e| e.detail)
                    .unwrap();
                let error = loader
                    .load_prefix(
                        driver,
                        arena,
                        resources,
                        screen,
                        GameCameraProjection {
                            global_mode: 0,
                            screen_flag_46f4: false,
                        },
                        &bytes,
                    )
                    .unwrap_err();
                assert_eq!(error.entry, 0x541b89);
                assert_eq!(error.cursor, bytes.len() - 3);
                assert_eq!(loader.finished_objects, 0);
                // Type guard does not call the Dummy setter on a real Sector.
                let after = arena
                    .state(arena.scene(scene).unwrap().sectors[0].frame())
                    .unwrap();
                assert_eq!(after.kind, before.kind);
                assert_eq!(after.local, before.local);
            });
        }
    }
    #[test]
    fn short_dummy_box_preserves_factory_owner_and_exact_read_cursor() {
        with_scene(|driver, arena, resources, screen, scene| {
            let bytes = input_objects(&new_object(6, b"short dummy\0", &chunk(0x4050, &[1, 2, 3])));
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            let error = loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            assert_eq!(error.entry, 0x5439d6);
            assert_eq!(error.cursor, bytes.len() - 3);
            let key = FrameKey(loader.current_frame.unwrap().identity());
            assert_eq!(arena.frame(key).unwrap().references, 1);
            assert_eq!(
                arena.dummy_bound_box(key).unwrap(),
                crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR.bbox
            );
            assert_eq!(loader.pending_factory_creator, None);
            assert_eq!(loader.finished_objects, 0);
        });
    }
    #[test]
    fn interrupted_camera_constructor_retains_actual_creator_and_null_map_slot() {
        struct FailingScreen {
            calls: usize,
        }
        impl SceneScreen for FailingScreen {
            type Error = &'static str;
            fn screen_sx(&mut self) -> Result<i32, Self::Error> {
                self.calls += 1;
                if self.calls > 2 {
                    Err("screen interrupted")
                } else {
                    Ok(800)
                }
            }
            fn screen_sy(&mut self) -> Result<i32, Self::Error> {
                self.calls += 1;
                if self.calls > 2 {
                    Err("screen interrupted")
                } else {
                    Ok(600)
                }
            }
        }
        with_scene(|driver, arena, resources, screen, scene| {
            let active = driver
                .create_camera_frame(arena, resources, scene, screen)
                .unwrap();
            arena.scene_set_active_camera(scene, Some(active)).unwrap();
            arena.release(active.frame()).unwrap();
            let bytes = input_objects(&new_object(3, b"partial camera\0", &[]));
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            let error = loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    &mut FailingScreen { calls: 0 },
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            assert_eq!(error.entry, 0x541d05);
            assert_eq!(error.cursor, bytes.len());
            assert_eq!(loader.current_frame, None);
            assert_eq!(loader.load_map.map.slot(b"partial camera"), Some(None));
            let partial = loader.pending_factory_creator.unwrap();
            let key = FrameKey(partial.identity());
            assert_eq!(arena.state(key).unwrap().kind, FrameKind::Camera);
            assert_eq!(arena.frame(key).unwrap().owner, None);
            assert_eq!(arena.frame(key).unwrap().references, 1);
            assert!(loader.release_pending_visual_replacement(arena).is_err());
            assert_eq!(loader.pending_factory_creator, Some(partial));
            assert_eq!(arena.frame(key).unwrap().references, 1);
        });
    }
    #[test]
    fn factory_set_name_allocation_failure_keeps_creator_until_explicit_cleanup() {
        with_scene(|driver, arena, resources, screen, scene| {
            let bytes = input_objects(&new_object(6, b"name allocation failure\0", &[]));
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            loader.set_factory_name_allocator(|_| {
                Err(crate::frame_fast_lookup::FastFrameError::AllocationFailed)
            });
            let error = loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            assert_eq!(error.entry, 0x541d2c);
            assert_eq!(loader.finished_objects, 0);
            assert_eq!(
                loader.load_map.map.slot(b"name allocation failure"),
                Some(None)
            );
            let creator = loader.pending_factory_creator.unwrap();
            assert_eq!(loader.current_frame, Some(creator));
            let key = FrameKey(creator.identity());
            let frame = arena.frame(key).unwrap();
            assert_eq!(frame.references, 1);
            assert_eq!(frame.owner, None);
            assert!(frame.name_is_null);
            assert_eq!(arena.state(key).unwrap().kind, FrameKind::Dummy);
            assert_eq!(
                loader
                    .load_prefix(
                        driver,
                        arena,
                        resources,
                        screen,
                        GameCameraProjection {
                            global_mode: 0,
                            screen_flag_46f4: false,
                        },
                        &bytes,
                    )
                    .unwrap_err()
                    .entry,
                0x5414fa
            );
            loader.release_pending_visual_replacement(arena).unwrap();
            assert_eq!(loader.pending_factory_creator, None);
            assert!(arena.frame(key).is_err());
        });
    }
    #[test]
    fn geometry_and_light_execute_before_real_named_model_cache_boundary() {
        with_scene(|driver, arena, resources, screen, scene| {
            let parent = chunk(
                0x4020,
                &[chunk(0x10, b"Primary sector\0"), vec![1, 2, 3]].concat(),
            );
            let occluder = new_object(
                12,
                b"fixture occluder\0",
                &[
                    chunk(0x20, &words(&[2., 3., 4.])),
                    chunk(0x22, &words(&[2., 0., 0., 0.])),
                    chunk(0x2d, &words(&[1.; 3])),
                    parent.clone(),
                    chunk(0x4083, &tetrahedron()),
                ]
                .concat(),
            );
            let light = new_object(
                2,
                b"fixture light\0",
                &[
                    chunk(0x20, &words(&[5., 6., 7.])),
                    chunk(0x22, &words(&[1., 0., 0., 0.])),
                    parent,
                    chunk(
                        0x4040,
                        &[
                            chunk(0x4041, &1u32.to_le_bytes()),
                            chunk(0x26, &words(&[0.25, 0.5, 0.75])),
                            chunk(0x4042, &words(&[3.])),
                            chunk(0x4043, &words(&[0.3, 0.6])),
                            chunk(0x4044, &words(&[1., 25.])),
                            chunk(0x4045, &0u32.to_le_bytes()),
                            chunk(0x4046, b"Primary sector\0"),
                        ]
                        .concat(),
                    ),
                ]
                .concat(),
            );
            let model = new_object(
                9,
                b"fixture model\0",
                &[
                    chunk(0x20, &words(&[99.; 3])),
                    chunk(0x2012, b"fixture.name.ext\0"),
                ]
                .concat(),
            );
            let bytes = input_objects(&[occluder, light, model].concat());
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            let error = loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            assert_eq!(error.entry, 0x54220a, "{error:?}");
            assert_eq!(loader.finished_objects, 2);
            assert_eq!(
                loader.pending_model_open.as_ref().unwrap().cache_name,
                b"fixture.name.i3d"
            );
            let frame = loader.current_frame.unwrap();
            let model = FrameKey(frame.identity());
            assert_eq!(arena.frame(model).unwrap().references, 1);
            assert_eq!(
                &arena.state(model).unwrap().local.elements[12..15],
                &[0.; 3]
            );
            let occluder = loader
                .load_map
                .map
                .slot(b"fixture occluder")
                .unwrap()
                .unwrap();
            let occluder = FrameKey(occluder.identity());
            assert_eq!(arena.frame(occluder).unwrap().references, 1);
            assert_eq!(
                &arena.state(occluder).unwrap().local.elements[12..15],
                &[2., 3., 4.]
            );
            assert_eq!(
                arena.state(occluder).unwrap().rotation.wxyz,
                [1., 0., 0., 0.]
            );
            let light = loader.load_map.map.slot(b"fixture light").unwrap().unwrap();
            let light = FrameKey(light.identity());
            assert_eq!(arena.light(light).unwrap().mode, 0x40);
            assert_eq!(arena.light(light).unwrap().color, [0.25, 0.5, 0.75]);
            assert_eq!(
                arena.light(light).unwrap().sectors,
                vec![arena.scene(scene).unwrap().sectors[0].frame()]
            );
        });
    }
    #[test]
    fn scene2_fog_light_owns_primary_membership_and_closes_with_scene() {
        with_scene(|driver, arena, resources, screen, scene| {
            // MISE01's `mlha` uses this field order: type 5, color, power,
            // cone, percent range, mode, then its Primary-sector name.
            let fog = new_object(
                2,
                b"mlha\0",
                &chunk(
                    0x4040,
                    &[
                        chunk(0x4041, &5u32.to_le_bytes()),
                        chunk(0x26, &words(&[0.067, 0.086, 0.141])),
                        chunk(0x4042, &words(&[1.])),
                        chunk(0x4043, &words(&[0.35, 0.7])),
                        chunk(0x4044, &words(&[20., 90.])),
                        chunk(0x4045, &99u32.to_le_bytes()),
                        chunk(0x4046, b"Primary sector\0"),
                    ]
                    .concat(),
                ),
            );
            let blocked = new_object(10, b"next factory\0", &[]);
            let bytes = input_objects(&[fog, blocked].concat());
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|error| error.detail)
                .unwrap();
            let error = loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap_err();
            assert_eq!(error.entry, 0x541d05, "{error:?}");
            assert_eq!(loader.finished_objects, 1);
            let light = FrameKey(
                loader
                    .load_map
                    .map
                    .slot(b"mlha")
                    .unwrap()
                    .unwrap()
                    .identity(),
            );
            let primary = arena.scene(scene).unwrap().sectors[0];
            let fog = arena.light(light).unwrap();
            assert_eq!(fog.light_type, 5);
            assert_eq!(fog.near_scaled.to_bits(), (20f32 * 0.01).to_bits());
            assert_eq!(fog.far_scaled.to_bits(), (90f32 * 0.01).to_bits());
            assert_eq!(fog.sectors, [primary.frame()]);
            let sector = arena.sector(primary).unwrap();
            assert_eq!(sector.lights, [light]);
            assert_eq!(sector.num_fog_lights, 1);
            assert_eq!(sector.fog_light, Some(light));
        });
    }
    #[test]
    fn short_geometry_reads_retain_exact_ordered_frame_and_temporary_prefix() {
        for stage in 0..4 {
            with_scene(|driver, arena, resources, screen, scene| {
                let mut geometry = Vec::new();
                if stage > 0 {
                    geometry.extend_from_slice(&1u32.to_le_bytes());
                }
                if stage > 1 {
                    geometry.extend_from_slice(&words(&[1., 2., 3.]));
                }
                if stage > 2 {
                    geometry.extend_from_slice(&1u32.to_le_bytes());
                }
                let bytes = input_objects(&new_object(
                    12,
                    b"partial occluder\0",
                    &chunk(0x4083, &geometry),
                ));
                let mut loader = MissionSceneLoader::new(arena, scene)
                    .map_err(|e| e.detail)
                    .unwrap();
                let error = loader
                    .load_prefix(
                        driver,
                        arena,
                        resources,
                        screen,
                        GameCameraProjection {
                            global_mode: 0,
                            screen_flag_46f4: false,
                        },
                        &bytes,
                    )
                    .unwrap_err();
                assert_eq!(
                    error.entry,
                    [0x543bd5, 0x543c2b, 0x543c45, 0x543c73][stage],
                    "{error:?}"
                );
                let payload = bytes
                    .windows(2)
                    .position(|word| word == [0x83, 0x40])
                    .unwrap()
                    + 6;
                assert_eq!(error.cursor, payload + [0, 4, 16, 20][stage]);
                assert_eq!(loader.finished_objects, 0);
                let frame = loader.current_frame.unwrap();
                assert_eq!(
                    loader.load_map.map.slot(b"partial occluder"),
                    Some(Some(frame))
                );
                assert_eq!(
                    arena.frame(FrameKey(frame.identity())).unwrap().references,
                    1
                );
                if stage == 0 {
                    assert!(loader.pending_occluder_geometry.is_none());
                } else {
                    let pending = loader.pending_occluder_geometry.as_ref().unwrap();
                    assert_eq!(pending.vertex_count, 1);
                    assert_eq!(pending.vertices.is_some(), stage > 1);
                    assert_eq!(pending.face_count.is_some(), stage > 2);
                    assert!(pending.faces.is_none());
                }
            });
        }
    }
    #[test]
    fn absent_parent_uses_deferred_position_then_actual_sector_fallback() {
        with_scene(|driver, arena, resources, screen, scene| {
            let occluder = new_object(
                12,
                b"fallback occluder\0",
                &[
                    chunk(0x20, &words(&[1., 2., 3.])),
                    chunk(0x22, &words(&[1., 0., 0., 0.])),
                    chunk(0x2c, &words(&[7., 8., 9.])),
                    chunk(0x4020, &chunk(0x10, b"not-yet-a-sector\0")),
                    chunk(0x4083, &tetrahedron()),
                ]
                .concat(),
            );
            let bytes = input_objects(&occluder);
            let mut loader = MissionSceneLoader::new(arena, scene)
                .map_err(|e| e.detail)
                .unwrap();
            loader
                .load_prefix(
                    driver,
                    arena,
                    resources,
                    screen,
                    GameCameraProjection {
                        global_mode: 0,
                        screen_flag_46f4: false,
                    },
                    &bytes,
                )
                .unwrap();
            assert_eq!(loader.finished_objects, 1);
            assert_eq!(loader.load_map.map.slot(b"not-yet-a-sector"), Some(None));
            let frame = FrameKey(loader.current_frame.unwrap().identity());
            assert_eq!(
                arena.links(frame).unwrap().parent,
                Some(arena.scene(scene).unwrap().sectors[0].frame())
            );
            assert_eq!(
                &arena.state(frame).unwrap().local.elements[12..15],
                &[7., 8., 9.]
            );
        });
    }
    #[test]
    fn visual_subtype_reads_before_null_or_nonvisual_guard_and_short_read_keeps_cursor() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let mut arena = ModelArena::default();
            let mut resources = SceneResources::default();
            let mut textures = TextureResources::new().unwrap();
            let mut storage = CpuTextureStorage::default();
            let mut driver = DriverSceneOwner::default();
            let mut screen = Screen { calls: Vec::new() };
            let scene = driver
                .create_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    &mut screen,
                    &mut MissingImages,
                    NativeSceneSoundDriver::Absent,
                )
                .unwrap();
            let mut loader =
                MissionSceneLoader::new(&arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
            for length in 0..4 {
                let data = chunk(0x4c53, &chunk(0x4090, &[0x41; 4][..length]));
                let mut input = MissionSceneInput::new(&data).unwrap();
                input.next_chunk().unwrap();
                assert_eq!(
                    loader
                        .read_visual_subtype(&mut arena, &mut input)
                        .unwrap_err()
                        .0,
                    0x548490
                );
                assert_eq!(input.cursor(), 12);
                assert!(input.current().is_some());
            }
            let model = arena.create_model().unwrap();
            for frame in [None, Some(arena.lookup_reference(model.root()))] {
                loader.current_frame = frame;
                let data = chunk(0x4c53, &chunk(0x4090, &0x12345678u32.to_le_bytes()));
                let mut input = MissionSceneInput::new(&data).unwrap();
                input.next_chunk().unwrap();
                loader.read_visual_subtype(&mut arena, &mut input).unwrap();
                assert_eq!(input.cursor(), data.len());
                assert!(input.current().is_none());
                assert_eq!(
                    loader.object_fields.visual_subtype_request,
                    Some(0x12345678)
                );
            }
            arena.release(model.root()).unwrap();
            resources
                .close_constructed_scene(&mut arena, scene, &mut textures, &mut storage)
                .unwrap();
            driver
                .release_scene(
                    &mut DriverSceneContext {
                        arena: &mut arena,
                        resources: &mut resources,
                        textures: &mut textures,
                        storage: &mut storage,
                    },
                    scene,
                )
                .unwrap();
            driver.release_empty_registry_storage().unwrap();
            assert_eq!(arena.live_frame_count(), 0);
        });
    }
    include!("mission_visual_replacement_tests.rs");
}
