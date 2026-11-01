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

fn load_lightmap(
    w: &mut World,
    frame: FrameKey,
) -> crate::i3d_object::lightmap_atlas::LightmapAtlasOwner {
    let object = w.arena.frame_object_resources(frame).unwrap().unwrap();
    let resources = w.arena.loaded_visual_resources_mut().unwrap();
    resources
        .load_saved_lightmaps(
            object,
            &payload(),
            crate::loaded_visual_resources::SavedLightmapLoadContext::TrueColorRgba8,
        )
        .unwrap();
    let Some(crate::i3d_object::saved_lightmaps::SavedLitLod::Lightmap(lod)) =
        resources.saved_lit_lod(object, 0).unwrap()
    else {
        panic!("loaded lightmap")
    };
    Arc::clone(&lod.atlas)
}

fn canonical_snapshot(w: &mut World, number: u32) -> crate::scene_render::CanonicalSceneSnapshot {
    let viewport = w
        .resources
        .construction(w.scene)
        .unwrap()
        .viewport_projection
        .unwrap();
    crate::scene_render::snapshot_scene(&mut w.arena, w.scene, viewport, [64, 64], number).unwrap()
}

#[test]
fn canonical_saved_lightmap_images_persist_by_source_revision() {
    persistent_lightmap_fixture(false);
}

#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn canonical_saved_lightmap_uploads_persist_across_snapshots() {
    persistent_lightmap_fixture(true);
}

fn persistent_lightmap_fixture(gpu: bool) {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut w = World::new();
        let primary = w.arena.scene(w.scene).unwrap().sectors[0];
        let a = w.visual(primary, true);
        let b = w.visual(primary, true);
        let source = load_lightmap(&mut w, a);
        let shared = load_lightmap(&mut w, b);
        assert!(Arc::ptr_eq(&source, &shared));
        drop(shared);
        let source_weak = Arc::downgrade(&source);
        let first = canonical_snapshot(&mut w, 1);
        assert_eq!(first.lighting.len(), 2);
        let image = |snapshot: &crate::scene_render::CanonicalSceneSnapshot, index: usize| {
            Arc::clone(
                snapshot.lighting[index]
                    .as_ref()
                    .unwrap()
                    .atlas
                    .as_ref()
                    .unwrap(),
            )
        };
        let original = image(&first, 0);
        assert!(Arc::ptr_eq(&original, &image(&first, 1)));
        let mut renderer = gpu.then(|| {
            let instance = lhr_render::instance();
            let adapter =
                pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
            pollster::block_on(lhr_render::Renderer::with_adapter(
                &adapter,
                &first.scene,
                wgpu::TextureFormat::Rgba8Unorm,
                false,
            ))
            .unwrap()
        });
        let upload = |renderer: &mut Option<lhr_render::Renderer>,
                      snapshot: &crate::scene_render::CanonicalSceneSnapshot| {
            if let Some(renderer) = renderer {
                for (index, lighting) in snapshot.lighting.iter().enumerate() {
                    renderer
                        .set_mesh_lighting(index, lighting.as_ref().unwrap())
                        .unwrap();
                }
            }
        };
        upload(&mut renderer, &first);
        let initial_writes = renderer.as_ref().map(|r| r.lighting_upload_statistics());
        if let Some(writes) = initial_writes {
            assert_eq!(writes.atlas_uploads, 1);
            assert_eq!(writes.vertex_uploads, 2);
        }
        // Fresh canonical visibility/camera/snapshot work does not change image identity.
        let second = canonical_snapshot(&mut w, 2);
        assert!(Arc::ptr_eq(&original, &image(&second, 0)));
        upload(&mut renderer, &second);
        let legacy = crate::car_inspection::snapshot_arena_tree(&mut w.arena, a).unwrap();
        assert!(Arc::ptr_eq(
            &original,
            legacy.lighting[0].as_ref().unwrap().atlas.as_ref().unwrap()
        ));
        drop(legacy);
        // Pose and camera changes read the same saved UV/atlas owners.
        w.arena.graph.assign_position(a, [0.25, 0., 5.]).unwrap();
        let camera = w.arena.scene_active_camera(w.scene).unwrap().unwrap();
        w.arena.camera_set_position(camera, [0.1, 0., 0.]).unwrap();
        let moved = canonical_snapshot(&mut w, 3);
        assert_eq!(moved.lighting.len(), 2);
        assert!(Arc::ptr_eq(&original, &image(&moved, 0)));
        upload(&mut renderer, &moved);
        assert_eq!(
            renderer.as_ref().map(|r| r.lighting_upload_statistics()),
            initial_writes
        );
        // Mutate the actual source owner while keeping old snapshots alive.
        {
            let mut atlas = source.write().unwrap();
            atlas.rgba[0] ^= 0xff;
            atlas.revision += 1;
        }
        let changed = canonical_snapshot(&mut w, 4);
        let revised = image(&changed, 0);
        let revised_weak = Arc::downgrade(&revised);
        assert!(!Arc::ptr_eq(&original, &revised));
        assert!(Arc::ptr_eq(&revised, &image(&changed, 1)));
        assert_ne!(original.rgba[0], revised.rgba[0]);
        upload(&mut renderer, &changed);
        let repeated = canonical_snapshot(&mut w, 5);
        assert!(Arc::ptr_eq(&revised, &image(&repeated, 0)));
        upload(&mut renderer, &repeated);
        if let Some(renderer) = &renderer {
            assert_eq!(renderer.lighting_upload_statistics().atlas_uploads, 2);
            assert_eq!(renderer.lighting_upload_statistics().vertex_uploads, 2);
        }
        drop((
            first, second, moved, changed, repeated, original, revised, source,
        ));
        for frame in [a, b] {
            let Some(crate::scene_ownership::FrameOwner::Model(model)) =
                w.arena.frame(frame).unwrap().owner
            else {
                panic!("model-owned visual")
            };
            w.arena.scene_delete_frame(w.scene, model.root()).unwrap();
        }
        assert!(
            source_weak.upgrade().is_none(),
            "presentation does not retain the source"
        );
        let empty = canonical_snapshot(&mut w, 6);
        assert!(empty.lighting.is_empty());
        if let Some(renderer) = &mut renderer {
            renderer.replace_meshes(0..2, &empty.scene.meshes).unwrap();
        }
        assert!(
            revised_weak.upgrade().is_none(),
            "next snapshot prunes images of dead owners"
        );
        w.close();
        // A new scene/source domain, even with identical pixels/revision, uploads once.
        let mut reloaded = World::new();
        let primary = reloaded.arena.scene(reloaded.scene).unwrap().sectors[0];
        let frame = reloaded.visual(primary, true);
        drop(load_lightmap(&mut reloaded, frame));
        let fresh = canonical_snapshot(&mut reloaded, 1);
        if let Some(renderer) = &mut renderer {
            renderer.replace_meshes(0..0, &fresh.scene.meshes).unwrap();
            renderer
                .set_mesh_lighting(0, fresh.lighting[0].as_ref().unwrap())
                .unwrap();
            assert_eq!(renderer.lighting_upload_statistics().atlas_uploads, 3);
        }
        drop(fresh);
        reloaded.close();
    });
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
