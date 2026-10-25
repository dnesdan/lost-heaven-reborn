//! Original scene2 loader's local name-map initialization, Game RVA 0x14120a.
//! PC SHA256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This borrows an existing scene and executes its ordered camera/header/sector
//! prefix. Object factories and actor installation remain explicit boundaries.
use crate::driver_scene::DriverSceneOwner;
use crate::frame_camera::CameraKey;
use crate::frame_transform::FrameKey;
use crate::frame_transform::FrameKind;
use crate::game_camera_projection::GameCameraProjection;
use crate::mission_actor_stream::OriginalLoadMap;
use crate::mission_scene_stream::MissionSceneInput;
use crate::model_loading::ModelArena;
use crate::scene_ownership::SceneKey;
use crate::scene_resources::{SceneResources, SceneScreen};
use crate::script_scheduler::FrameReferenceId;
use crate::sector_sound_data::SectorSoundInput;
use std::collections::HashSet;

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
        let result = self.read_root(arena, screen, projection, &mut input);
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
    fn read_root<S: SceneScreen>(
        &mut self,
        arena: &mut ModelArena,
        screen: &mut S,
        projection: GameCameraProjection,
        input: &mut MissionSceneInput<'_>,
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
                    self.read_objects(arena, input)?;
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
        arena: &mut ModelArena,
        input: &mut MissionSceneInput<'_>,
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
                            return Err((if self.current_kind == 1 { 0x541cfe } else { 0x541d05 }, format!("unavailable original driver frame factory kind {}; null map slot retained", self.current_kind)));
                        }
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
                arena
                    .update_frame(key)
                    .map_err(|e| (0x544631, format!("{e:?}")))?;
                if arena
                    .state(key)
                    .map_err(|e| (0x544636, format!("{e:?}")))?
                    .kind
                    == FrameKind::Model
                {
                    return Err((
                        0x544645,
                        "unavailable original model post-object virtual+64".into(),
                    ));
                }
            }
            self.finished_objects += 1;
            input.leave_container().map_err(|e| (0x5446aa, e))?;
        }
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
                chunk(0x4011, &12u32.to_le_bytes()),
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
            assert_eq!(loader.current_kind, 12);
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
}
