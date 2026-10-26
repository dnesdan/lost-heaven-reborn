use crate::{
    car_inspection::{camera_from_arena_bounds, snapshot_car_arena_with_view},
    frame_transform::FrameKey,
    inspection_lod::InspectionLodView,
    model_loading::{ModelArena, ModelKey},
};
use std::sync::Arc;
struct Fixture {
    arena: ModelArena,
    model: ModelKey,
    frames: Vec<FrameKey>,
    directory: std::path::PathBuf,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.arena.close(self.model);
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
fn fixture(position: [f32; 3], child: bool) -> Fixture {
    let directory = std::env::temp_dir().join(format!(
        "lhr-live-lod-{}-{}",
        std::process::id(),
        crate::frame_lookup::incarnation()
    ));
    std::fs::create_dir(&directory).unwrap();
    let installation = lhr_assets::Installation::open(&directory).unwrap();
    let mat = |diffuse| lhr_assets::Material {
        flags: 0,
        ambient: [0.; 3],
        diffuse,
        texture: String::new(),
        emission: [0.; 3],
        opacity: 1.,
        alpha_texture: None,
        reflection: None,
        environment_texture: None,
        animation: None,
    };
    let vertex = |position| lhr_assets::Vertex {
        position,
        normal: [0., 0., -1.],
        uv: [0.; 2],
    };
    let mut frame = crate::model_loading::tests::frame(1, None, position);
    frame.render_flags = 0;
    frame.logic_flags = 0;
    frame.lods = vec![
        lhr_assets::Lod {
            distance: 25.,
            uv_only: None,
            vertices: [[-1., -1., 0.], [1., -1., 0.], [0., 1., 2.]]
                .map(vertex)
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                // Match the declared source -Z normal and the native view.
                // The bridge reflects Z; it does not reverse these indices.
                indices: vec![0, 2, 1],
            }],
        },
        lhr_assets::Lod {
            distance: 100.,
            uv_only: None,
            vertices: [[-1., -1., 1.], [1., -1., 1.], [1., 1., 1.], [-1., 1., 1.]]
                .map(vertex)
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: 1,
                indices: vec![0, 2, 1, 0, 3, 2],
            }],
        },
    ];
    let mut frames = vec![frame.clone()];
    if child {
        frame.parent = Some(0);
        frame.position = [100., 0., 0.];
        frames.push(frame);
    }
    let source = Arc::new(lhr_assets::Model {
        materials: vec![mat([1., 0., 0.]), mat([0., 1., 0.])],
        frames,
    });
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let model = arena.create_model().unwrap();
    arena.scene_add_frame(scene, model.root()).unwrap();
    let frames = arena
        .load_parsed_into_model(model, source)
        .unwrap()
        .frames()
        .to_vec();
    arena
        .load_plain_visual_resources(model, &installation)
        .unwrap();
    Fixture {
        arena,
        model,
        frames,
        directory,
    }
}
fn view(camera_pc: [f32; 3]) -> InspectionLodView {
    InspectionLodView {
        camera_pc,
        rs_lod_quality: 1.,
    }
}
#[test]
fn selected_owned_level_threshold_fade_cull_and_reentry_reach_actual_snapshots() {
    use lhr_render::{MaterialQueue, RenderMaterial};
    let mut f = fixture([0.; 3], false);
    let root = f.model.root();
    let object = f
        .arena
        .frame_object_resources(f.frames[0])
        .unwrap()
        .unwrap();
    let near = snapshot_car_arena_with_view(&mut f.arena, root, view([0., 0., -4.])).unwrap();
    assert_eq!(near.scene.meshes[0].vertices.len(), 3); // exact distance^2 ==25 stays LOD0
    assert_eq!(near.scene.meshes[0].indices.len(), 3);
    let middle = snapshot_car_arena_with_view(&mut f.arena, root, view([0.5, 0., -8.])).unwrap(); // 81.25, fade .5
    assert_eq!(middle.scene.meshes[0].vertices.len(), 4);
    assert_eq!(middle.scene.meshes[0].indices.len(), 6);
    let RenderMaterial::Original(material) = middle.scene.meshes[0].material else {
        panic!()
    };
    assert_eq!(material.color, [0., 1., 0., 128. / 255.]);
    assert_eq!(material.queue, MaterialQueue::Transparent);
    let owner = f
        .arena
        .loaded_visual_resources()
        .unwrap()
        .objects()
        .object(object)
        .unwrap();
    assert_eq!((owner.cur_lod, owner.fade), (1, 0.5));
    let at_limit = snapshot_car_arena_with_view(&mut f.arena, root, view([0., 0., -9.])).unwrap();
    assert_eq!(at_limit.scene.meshes.len(), 1); // distance^2==100 still selected, fade zero
    let culled = snapshot_car_arena_with_view(&mut f.arena, root, view([0., 0., -10.])).unwrap();
    assert!(culled.scene.meshes.is_empty());
    assert_eq!(culled.lod_culled_meshes, 1);
    assert_eq!(
        f.arena
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .object(object)
            .unwrap()
            .cur_lod,
        2
    );
    assert!(f
        .arena
        .loaded_visual_resources()
        .unwrap()
        .inspection_mesh_at_lod(object, 2, 8)
        .is_err());
    let again = snapshot_car_arena_with_view(&mut f.arena, root, view([0., 0., -4.])).unwrap();
    assert_eq!(again.scene.meshes[0].indices.len(), 3);
    assert_eq!(
        f.arena
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .object(object)
            .unwrap()
            .fade,
        1.
    );
}
#[test]
fn lod_uses_own_visual_center_while_sort_uses_hr_center_and_bootstrap_reflects_asymmetric_z() {
    let mut f = fixture([10., 20., 30.], true);
    let root = f.model.root();
    let camera = camera_from_arena_bounds(&mut f.arena, root).unwrap();
    assert_eq!(camera.target.to_array(), [60., 20., -31.]); // child extends hierarchy X; Z extent is [30,32]
    let snapshot = snapshot_car_arena_with_view(&mut f.arena, root, view([10., 20., 26.])).unwrap();
    assert_eq!(snapshot.scene.meshes.len(), 1); // distant child culled, parent's own center remains near
    assert_eq!(snapshot.scene.meshes[0].indices.len(), 3);
    let lhr_render::RenderMaterial::Original(material) = snapshot.scene.meshes[0].material else {
        panic!()
    };
    assert_eq!(material.sort_center, [60., 20., -31.]);
    let own = f.arena.visual_world_bounds(f.frames[0]).unwrap();
    assert_eq!(own.center, [10., 20., 31.]);
}
#[test]
fn camera_horizontal_fov_and_world_scale_change_source_selection() {
    use glam::Vec3;
    use lhr_render::Camera;
    let camera = Camera {
        vertical_fov_radians: 50f32.to_radians(),
        eye: Vec3::new(0., 0., 7.),
        target: Vec3::new(0., 0., -1.),
        up: Vec3::Y,
        near: 0.1,
        far: 200.,
    };
    let square = InspectionLodView::from_camera(&camera, [100, 100], 1.).unwrap();
    let wide = InspectionLodView::from_camera(&camera, [1600, 900], 1.).unwrap();
    assert!(square.rs_lod_quality > wide.rs_lod_quality);
    let mut f = fixture([0.; 3], false);
    let root = f.model.root();
    let a = snapshot_car_arena_with_view(&mut f.arena, root, square).unwrap();
    let b = snapshot_car_arena_with_view(&mut f.arena, root, wide).unwrap();
    assert_eq!(a.scene.meshes[0].indices.len(), 3);
    assert_eq!(b.scene.meshes[0].indices.len(), 6);
    let object = f
        .arena
        .frame_object_resources(f.frames[0])
        .unwrap()
        .unwrap();
    let resources = f.arena.loaded_visual_resources_mut().unwrap();
    use crate::loaded_visual_resources::RenderLodSelection;
    assert!(matches!(
        resources
            .select_render_lod(object, 81.25, 1., 2., 0, 0)
            .unwrap(),
        RenderLodSelection::Selected { index: 0, .. }
    ));
    assert!(matches!(
        resources
            .select_render_lod(object, 81.25, 1., 1., 0, 0)
            .unwrap(),
        RenderLodSelection::Selected { index: 1, .. }
    ));
    let previous = resources.objects().object(object).unwrap().cur_lod;
    for (flags, visual) in [(0x400, 0), (0, 0x10), (0, 0x40), (0, 0x10000)] {
        assert!(resources
            .select_render_lod(object, 200., 1., 1., flags, visual)
            .is_err());
        assert_eq!(
            resources.objects().object(object).unwrap().cur_lod,
            previous
        );
    }
}
#[test]
fn view_car_wrapper_rejects_no_admitted_cpu_mesh_but_generic_empty_tree_is_valid() {
    let mut arena = ModelArena::default();
    let model = arena.create_model().unwrap();
    assert!(snapshot_car_arena_with_view(&mut arena, model.root(), view([0.; 3])).is_err());
    assert!(crate::car_inspection::snapshot_arena_tree_with_view(
        &mut arena,
        model.root(),
        view([0.; 3])
    )
    .unwrap()
    .scene
    .meshes
    .is_empty());
}
#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn actual_camera_selected_lod_and_fade_reach_gpu_pixels() {
    use glam::Vec3;
    use lhr_render::{Camera, Renderer, Scene};
    let mut f = fixture([0.; 3], false);
    let root = f.model.root();
    let instance = lhr_render::instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    let mut renderer = pollster::block_on(Renderer::with_adapter(
        &adapter,
        &Scene { meshes: Vec::new() },
        wgpu::TextureFormat::Rgba8Unorm,
        false,
    ))
    .unwrap();
    let mut count = 0;
    for (eye_z, expected_lod) in [(4., Some(0)), (11., Some(1)), (100., None), (4., Some(0))] {
        let camera = Camera {
            vertical_fov_radians: 50f32.to_radians(),
            eye: Vec3::new(0., 0., eye_z),
            target: Vec3::new(0., 0., -1.),
            up: Vec3::Y,
            near: 0.1,
            far: 200.,
        };
        let view = InspectionLodView::from_camera(&camera, [48, 32], 1.).unwrap();
        let snapshot = snapshot_car_arena_with_view(&mut f.arena, root, view).unwrap();
        renderer
            .replace_meshes(0..count, &snapshot.scene.meshes)
            .unwrap();
        count = snapshot.scene.meshes.len();
        let rgba = renderer.readback(48, 32, &camera).unwrap();
        let pixel: [u8; 4] = rgba[(16 * 48 + 24) * 4..(16 * 48 + 24) * 4 + 4]
            .try_into()
            .unwrap();
        match expected_lod {
            Some(0) => assert_eq!(pixel, [255, 0, 0, 255]),
            Some(1) => {
                let lhr_render::RenderMaterial::Original(material) =
                    snapshot.scene.meshes[0].material
                else {
                    panic!()
                };
                assert!(material.color[3] > 0. && material.color[3] < 1.);
                let expected_green =
                    ((material.color[3] + 0.035 * (1. - material.color[3])) * 255.).round() as i16;
                assert!((i16::from(pixel[1]) - expected_green).abs() <= 1);
                assert!(pixel[0] < 10);
            }
            None => {
                assert_eq!(count, 0);
                assert!(pixel[0] < 10 && pixel[1] < 15);
            }
            _ => unreachable!(),
        }
    }
}
