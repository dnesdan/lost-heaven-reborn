//! Original scene2 loader's local name-map initialization, Game RVA 0x14120a.
//! PC SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This borrows an existing scene and executes its ordered camera/header/sector
//! prefix, admitted occluders/lights and fresh model construction. Original
//! static model-cache loading. World SceneOpen and actor installation remain
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
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneResources, SceneScreen};
use crate::script_scheduler::FrameReferenceId;
use crate::sector_sound_data::SectorSoundInput;
use std::collections::HashSet;

type ModelOpenAdapter<'a> =
    dyn FnMut(&mut ModelArena, crate::model_loading::ModelKey, &[u8]) -> Result<i32, String> + 'a;

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
/// The existing host map admits ASCII names; other name bytes return the
/// already-published prefix without continuing to later frames or sectors.
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
/// State of one original scene2 invocation. The caller retains its real scene,
/// driver and resources; an error leaves their executed effects in place.
pub struct MissionSceneLoader {
    pub scene: SceneKey,
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
    pub pending_occluder_geometry: Option<MissionOccluderGeometry>,
    pub pending_model_open: Option<MissionModelOpenRequest>,
    pub pending_light_sector_link: Option<PendingLightSectorLink>,
    entered: bool,
}
impl MissionSceneLoader {
    pub fn new(arena: &ModelArena, scene: SceneKey) -> Result<Self, SceneLoadMapError> {
        Ok(Self {
            scene,
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
            pending_occluder_geometry: None,
            pending_model_open: None,
            pending_light_sector_link: None,
            entered: false,
        })
    }
    fn error(&self, entry: u32, detail: impl Into<String>) -> MissionSceneLoadError {
        MissionSceneLoadError {
            cursor: self.cursor,
            entry,
            detail: detail.into(),
        }
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
            driver, arena, resources, screen, projection, bytes, &mut None,
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
        let mut open = |arena: &mut ModelArena, model, name: &[u8]| {
            caches
                .open_static_model(arena, installation, model, name, memory)
                .map_err(|e| format!("{e:?}"))
        };
        self.load_prefix_inner(
            driver,
            arena,
            resources,
            screen,
            projection,
            bytes,
            &mut Some(&mut open),
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
        model_open: &mut Option<&mut ModelOpenAdapter<'_>>,
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
            driver, arena, resources, screen, projection, &mut input, model_open,
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
        model_open: &mut Option<&mut ModelOpenAdapter<'_>>,
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
                    self.read_objects(driver, arena, resources, input, model_open)?;
                    input.leave_container().map_err(|e| (0x5446aa, e))?;
                    continue;
                }
                _ => {
                    return Err((
                        0x541743,
                        format!("unavailable reached scene2 root arm {:04x}", chunk.kind),
                    ))
                }
            }
            input.close_current().map_err(|e| (0x404ce0, e))?;
        }
        Ok(())
    }
    fn read_objects(
        &mut self,
        driver: &mut DriverSceneOwner,
        arena: &mut ModelArena,
        resources: &SceneResources,
        input: &mut MissionSceneInput<'_>,
        model_open: &mut Option<&mut ModelOpenAdapter<'_>>,
    ) -> Result<(), (u32, String)> {
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
                            let key=match self.current_kind {
                                12=>driver.create_occluder_frame(arena,resources,self.scene).map_err(|e|(0x541d05,format!("{e:?}")))?.frame(),
                                2=>driver.create_light_frame(arena,resources,self.scene).map_err(|e|(0x541d05,format!("{e:?}")))?,
                                9=>driver.create_model_frame(arena,resources,self.scene).map_err(|e|(0x541d05,format!("{e:?}")))?.root(),
                                _=>return Err((if self.current_kind == 1 { 0x541cfe } else { 0x541d05 }, format!("unavailable original driver frame factory kind {}; null map slot retained", self.current_kind))),
                            };
                            self.current_frame = Some(arena.lookup_reference(key));
                            arena
                                .set_frame_name(key, Some(&self.current_name))
                                .map_err(|e| (0x541d2c, format!("{e:?}")))?;
                            self.object_fields.flags |= 0x20000;
                            arena
                                .scene_add_frame(self.scene, key)
                                .map_err(|e| (0x541d47, format!("{e:?}")))?;
                            arena
                                .release(key)
                                .map_err(|e| (0x541d4e, format!("{e:?}")))?;
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
                    0x2012 => {
                        self.read_model_open(arena, input, model_open)?;
                        if input.current().is_none() {
                            continue;
                        }
                    }
                    0x4083 => self.read_occluder_geometry(arena, input)?,
                    0x4090 => {
                        self.read_visual_subtype(arena, input)?;
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
                        ))
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
        arena: &ModelArena,
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
        Err((
            0x543a35,
            "original visual subtype getter/replacement requires its actual class owner".into(),
        ))
    }
    fn read_model_open(
        &mut self,
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
        model_open: &mut Option<&mut ModelOpenAdapter<'_>>,
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
        let Some(open) = model_open.as_mut() else {
            return Err((
                0x54220a,
                "original C_I3D_model_cache::Open4087e0 resource loading unavailable".into(),
            ));
        };
        let model = crate::model_loading::ModelKey(key);
        let requested = &self.pending_model_open.as_ref().unwrap().cache_name;
        let result = open(arena, model, requested).map_err(|e| (0x54220a, e))?;
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
                        .read_visual_subtype(&arena, &mut input)
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
                loader.read_visual_subtype(&arena, &mut input).unwrap();
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
}
