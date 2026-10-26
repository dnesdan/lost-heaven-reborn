use super::*;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
use crate::itexture::{CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources};
use crate::scene_resources::{SceneResources, SceneScreen};
use std::sync::Arc;
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
        Ok([8, 8, 56, 56])
    }
}
struct Images;
impl TextureImageSource for Images {
    fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        Err(TextureImageError::Missing)
    }
}

#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn canonical_scene_active_camera_visibility_owned_buffers_and_viewport_reach_pixels() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let dir = std::env::temp_dir().join(format!(
            "lhr-canonical-render-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        let mut arena = ModelArena::default();
        let mut resources = SceneResources::default();
        let mut textures = TextureResources::new().unwrap();
        let mut storage = CpuTextureStorage::default();
        let mut driver = DriverSceneOwner::default();
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
        let primary = arena.scene(scene).unwrap().sectors[0];
        let model = arena.create_model().unwrap();
        arena.scene_add_frame(scene, model.root()).unwrap();
        arena
            .link_frame(model.root(), Some(primary.frame()), 0)
            .unwrap();
        let mut visual = crate::model_loading::tests::frame(1, None, [0., 0., 5.]);
        visual.render_flags = 0;
        visual.logic_flags = 0;
        // Native face order agrees with the source normal toward the viewer.
        // The old opposite order plus TWO_SIDED masked the bridge cull error.
        visual.lods.push(lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-1., -1., 0.], [0., 1., 0.], [1., -1., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., -1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        });
        let frames = arena
            .load_parsed_into_model(
                model,
                Arc::new(lhr_assets::Model {
                    materials: vec![lhr_assets::Material {
                        flags: 0,
                        ambient: [0.; 3],
                        diffuse: [1., 0., 0.],
                        texture: String::new(),
                        emission: [0.; 3],
                        opacity: 1.,
                        alpha_texture: None,
                        reflection: None,
                        environment_texture: None,
                        animation: None,
                    }],
                    frames: vec![visual],
                }),
            )
            .unwrap();
        let visual = frames.frames()[0];
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        arena.release(model.root()).unwrap();
        let a = arena.create_camera(&mut Screen).unwrap();
        let b = arena.create_camera(&mut Screen).unwrap();
        arena.camera_set_range(a, 0.1, 100.).unwrap();
        arena.camera_set_range(b, 0.1, 100.).unwrap();
        arena.camera_set_position(b, [20., 0., 0.]).unwrap();
        arena.scene_set_active_camera(scene, Some(a)).unwrap();
        let viewport = resources
            .construction(scene)
            .unwrap()
            .viewport_projection
            .unwrap();
        let first = snapshot_scene(&mut arena, scene, viewport, [64, 64], 1).unwrap();
        assert!(first.draw_frames.contains(&visual));
        assert_eq!(first.frame.main.draws.len(), 1);
        let lhr_render::RenderMaterial::Original(front) = first.scene.meshes[0].material else {
            panic!("expected actual ordinary source material");
        };
        assert!(!front.two_sided);
        assert_eq!(front.front_face, lhr_render::FrontFace::Clockwise);
        let instance = lhr_render::instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
            &adapter,
            &first.scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let rgba = renderer.readback_scene_frame(64, 64, &first.frame).unwrap();
        assert_eq!(
            &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
            &[255, 0, 0, 255]
        );
        assert_eq!(
            &rgba[(2 * 64 + 2) * 4..(2 * 64 + 2) * 4 + 4],
            &[6, 9, 14, 255]
        );
        arena.scene_set_active_camera(scene, Some(b)).unwrap();
        let second = snapshot_scene(&mut arena, scene, viewport, [64, 64], 2).unwrap();
        assert!(second.frame.main.draws.is_empty());
        renderer
            .replace_meshes(0..first.scene.meshes.len(), &second.scene.meshes)
            .unwrap();
        let rgba = renderer
            .readback_scene_frame(64, 64, &second.frame)
            .unwrap();
        assert_eq!(
            &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
            &[6, 9, 14, 255]
        );
        arena.scene_set_active_camera(scene, Some(a)).unwrap();
        let third = snapshot_scene(&mut arena, scene, viewport, [64, 64], 3).unwrap();
        assert!(third.draw_frames.contains(&visual));
        renderer
            .replace_meshes(0..second.scene.meshes.len(), &third.scene.meshes)
            .unwrap();
        let rgba = renderer.readback_scene_frame(64, 64, &third.frame).unwrap();
        assert_eq!(
            &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
            &[255, 0, 0, 255]
        );
        // Keep the same source visual in the frustum from its reverse side.
        // It must remain in visibility/draw membership while culling its pixels.
        arena.camera_set_position(b, [0., 0., 10.]).unwrap();
        arena.camera_set_direction(b, [0., 0., -1.], 0.).unwrap();
        arena.scene_set_active_camera(scene, Some(b)).unwrap();
        let back = snapshot_scene(&mut arena, scene, viewport, [64, 64], 4).unwrap();
        assert!(back.draw_frames.contains(&visual));
        assert_eq!(back.frame.main.draws.len(), 1);
        renderer
            .replace_meshes(0..third.scene.meshes.len(), &back.scene.meshes)
            .unwrap();
        let rgba = renderer.readback_scene_frame(64, 64, &back.frame).unwrap();
        assert_eq!(
            &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
            &[6, 9, 14, 255]
        );
        let lhr_render::RenderMaterial::Original(mut two_sided) = back.scene.meshes[0].material
        else {
            panic!("expected actual ordinary source material");
        };
        two_sided.two_sided = true;
        renderer
            .update_render_material(0, lhr_render::RenderMaterial::Original(two_sided))
            .unwrap();
        let rgba = renderer.readback_scene_frame(64, 64, &back.frame).unwrap();
        assert_eq!(
            &rgba[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4],
            &[255, 0, 0, 255]
        );
        arena.release(a.frame()).unwrap();
        arena.release(b.frame()).unwrap();
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
        arena.reclaim_unused_visual_buffers().unwrap();
        assert_eq!(arena.live_frame_count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    });
}
