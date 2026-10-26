//! Actual Scene/sector/light owners feeding the selected-frame material bridge.
use super::*;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::itexture::{CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources};
use crate::scene_resources::{SceneResources, SceneScreen};
struct Screen;
impl SceneScreen for Screen {
    type Error = ();
    fn screen_sx(&mut self) -> Result<i32, ()> {
        Ok(64)
    }
    fn screen_sy(&mut self) -> Result<i32, ()> {
        Ok(64)
    }
}
impl DriverSceneScreen for Screen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
        Ok([0, 0, 64, 64])
    }
}
struct Images;
impl TextureImageSource for Images {
    fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        Err(TextureImageError::Missing)
    }
}
struct World {
    arena: ModelArena,
    scene: crate::scene_ownership::SceneKey,
    resources: SceneResources,
    driver: DriverSceneOwner,
    textures: TextureResources,
    storage: CpuTextureStorage,
    path: std::path::PathBuf,
    installation: lhr_assets::Installation,
}
impl World {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-sector-lighting-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut arena = ModelArena::default();
        let mut resources = SceneResources::default();
        let mut driver = DriverSceneOwner::default();
        let mut textures = TextureResources::new().unwrap();
        let mut storage = CpuTextureStorage::default();
        let scene = driver
            .create_scene(
                &mut DriverSceneContext {
                    arena: &mut arena,
                    resources: &mut resources,
                    textures: &mut textures,
                    storage: &mut storage,
                },
                &mut Screen,
                &mut Images,
                NativeSceneSoundDriver::Absent,
            )
            .unwrap();
        let camera = arena.create_camera(&mut Screen).unwrap();
        arena.scene_set_active_camera(scene, Some(camera)).unwrap();
        arena.release(camera.frame()).unwrap();
        Self {
            arena,
            scene,
            resources,
            driver,
            textures,
            storage,
            path,
            installation,
        }
    }
    fn visual(&mut self, sector: crate::scene_ownership::SectorKey, lit: bool) -> FrameKey {
        let model = self.arena.create_model().unwrap();
        self.arena
            .scene_add_frame(self.scene, model.root())
            .unwrap();
        self.arena
            .link_frame(model.root(), Some(sector.frame()), 0)
            .unwrap();
        let mut source = crate::model_loading::tests::frame(1, None, [0., 0., 5.]);
        source.render_flags = 0;
        source.logic_flags = 0;
        source.visual = Some(u8::from(lit));
        source.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., -1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: if lit {
                    0
                } else {
                    lhr_assets::UNASSIGNED_MATERIAL
                },
                indices: vec![0, 1, 2],
            }],
        });
        let source = Arc::new(lhr_assets::Model {
            frames: vec![source],
            materials: if lit {
                vec![lhr_assets::Material {
                    flags: 0,
                    ambient: [0.; 3],
                    diffuse: [1.; 3],
                    texture: String::new(),
                    emission: [0.; 3],
                    opacity: 1.,
                    alpha_texture: None,
                    reflection: None,
                    environment_texture: None,
                    animation: None,
                }]
            } else {
                vec![]
            },
        });
        let frames = self.arena.load_parsed_into_model(model, source).unwrap();
        let key = frames.frames()[0];
        self.arena
            .load_plain_visual_resources(model, &self.installation)
            .unwrap();
        self.arena.release(model.root()).unwrap();
        key
    }
    fn ambient(
        &mut self,
        sector: crate::scene_ownership::SectorKey,
        color: [f32; 3],
        mode: u32,
    ) -> FrameKey {
        let light = self.arena.create_light().unwrap();
        self.arena.graph.set_light_type(light, 4).unwrap();
        self.arena.graph.set_light_mode(light, mode).unwrap();
        self.arena.graph.set_light_diffuse(light, color).unwrap();
        self.arena.graph.set_light_diffuse_power(light, 1.).unwrap();
        self.arena.scene_add_frame(self.scene, light).unwrap();
        self.arena.sector_add_light(sector, light).unwrap();
        self.arena.release(light).unwrap();
        light
    }
    fn close(mut self) {
        self.resources
            .close_constructed_scene(
                &mut self.arena,
                self.scene,
                &mut self.textures,
                &mut self.storage,
            )
            .unwrap();
        self.driver
            .release_scene(
                &mut DriverSceneContext {
                    arena: &mut self.arena,
                    resources: &mut self.resources,
                    textures: &mut self.textures,
                    storage: &mut self.storage,
                },
                self.scene,
            )
            .unwrap();
        self.driver.release_empty_registry_storage().unwrap();
        self.arena.reclaim_unused_visual_buffers().unwrap();
        assert_eq!(self.arena.live_frame_count(), 0);
        std::fs::remove_dir_all(self.path).unwrap();
    }
}
fn payload() -> Vec<u8> {
    let mut b = vec![1, 0x21, 2];
    b.extend(1u32.to_le_bytes());
    b.extend(5f32.to_le_bytes());
    b.extend(45f32.to_le_bytes());
    b.push(1);
    b.extend(0u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.push(1);
    b.extend(0xff123456u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    b.extend(3u32.to_le_bytes());
    for f in [0f32, 0., 1., 0., 0., 1.] {
        b.extend(f.to_le_bytes());
    }
    for i in 0u16..3 {
        b.extend(i.to_le_bytes());
        b.extend(0u16.to_le_bytes());
    }
    b.extend(3u32.to_le_bytes());
    for i in 0u16..3 {
        b.extend(i.to_le_bytes());
    }
    b.extend(1u32.to_le_bytes());
    b.extend(0u16.to_le_bytes());
    b
}
#[test]
fn actual_omni_keeps_its_lights_and_near_uses_the_camera_sector() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut w = World::new();
        let [primary, _, omni, near] = w.arena.scene(w.scene).unwrap().sectors;
        let a = w.visual(omni, false);
        let b = w.visual(near, false);
        w.ambient(primary, [1., 0., 0.], 1);
        w.ambient(omni, [0., 1., 0.], 1);
        w.ambient(near, [0., 0., 1.], 1);
        let view = crate::scene_render::active_lod_view(&mut w.arena, w.scene, 1.).unwrap();
        let snapshot = snapshot_selected_frames_with_view(&mut w.arena, &[a, b], view).unwrap();
        let colors = |frame| {
            snapshot.lighting[snapshot
                .draw_frames
                .iter()
                .position(|&f| f == frame)
                .unwrap()]
            .as_ref()
            .unwrap()
            .colors[0]
        };
        assert_eq!(colors(a), [0., 128. / 255., 0., 1.]);
        assert_eq!(colors(b), [128. / 255., 0., 0., 1.]);
        w.close();
    });
}
#[test]
fn saved_lightmap_dynamic_lights_include_both_signed_threshold_equalities() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut w = World::new();
        let primary = w.arena.scene(w.scene).unwrap().sectors[0];
        let frame = w.visual(primary, true);
        let object = w.arena.frame_object_resources(frame).unwrap().unwrap();
        w.arena
            .loaded_visual_resources_mut()
            .unwrap()
            .load_saved_lightmaps(
                object,
                &payload(),
                crate::loaded_visual_resources::SavedLightmapLoadContext::TrueColorRgba8,
            )
            .unwrap();
        let light = w.ambient(primary, [1.; 3], 0x10);
        let view = crate::scene_render::active_lod_view(&mut w.arena, w.scene, 1.).unwrap();
        for bits in [0x322bcc76, 0xb22bcc76, 0x322bcc77, 0xb22bcc77] {
            w.arena
                .graph
                .set_light_diffuse_power(light, f32::from_bits(bits))
                .unwrap();
            let result = snapshot_selected_frames_with_view(&mut w.arena, &[frame], view);
            if bits & 0x7fff_ffff < 0x322bcc77 {
                assert!(result.is_ok(), "{bits:x}");
            } else {
                assert!(
                    result
                        .err()
                        .unwrap()
                        .contains("dynamic light/emissive evaluation is unavailable"),
                    "{bits:x}"
                );
            }
        }
        w.close();
    });
}
