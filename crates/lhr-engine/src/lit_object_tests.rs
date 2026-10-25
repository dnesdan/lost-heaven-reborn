//! Synthetic source-to-owner-to-snapshot regressions; no original assets.
use crate::{
    car_inspection::snapshot_arena_tree,
    i3d_object::ObjectVisualType,
    model_loading::{ModelArena, ModelKey},
};
use std::{path::PathBuf, sync::Arc};

fn fixture(textured: bool, flags: u32) -> (PathBuf, ModelArena, ModelKey) {
    let path = std::env::temp_dir().join(format!(
        "lhr-lit-{}-{}",
        std::process::id(),
        crate::frame_lookup::incarnation()
    ));
    std::fs::create_dir(&path).unwrap();
    std::fs::create_dir(path.join("maps")).unwrap();
    let mut tga = vec![0; 18];
    tga[2] = 2;
    tga[12..14].copy_from_slice(&1u16.to_le_bytes());
    tga[14..16].copy_from_slice(&1u16.to_le_bytes());
    tga[16] = 32;
    tga[17] = 0x28;
    tga.extend_from_slice(&[32, 64, 128, 17]);
    std::fs::write(path.join("maps/synthetic.tga"), tga).unwrap();
    let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
    frame.visual = Some(1);
    frame.render_flags = 0;
    frame.logic_flags = 0;
    frame.lods.push(lhr_assets::Lod {
        distance: 0.,
        vertices: [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]]
            .map(|position| lhr_assets::Vertex {
                position,
                normal: [0.9; 3],
                uv: [0.25, 0.75],
            })
            .to_vec(),
        groups: vec![lhr_assets::FaceGroup {
            material: 0,
            indices: vec![0, 1, 2],
        }],
    });
    let source = Arc::new(lhr_assets::Model {
        materials: vec![lhr_assets::Material {
            flags: flags | 0x10000000 | if textured { 0x40000 } else { 0 },
            ambient: [0.; 3],
            diffuse: [1., 0., 0.],
            texture: if textured {
                "synthetic.tga".into()
            } else {
                String::new()
            },
            emission: [0.7; 3],
            opacity: 0.25,
            alpha_texture: None,
            reflection: None,
            environment_texture: None,
            animation: None,
        }],
        frames: vec![frame],
    });
    let installation = lhr_assets::Installation::open(&path).unwrap();
    let mut arena = ModelArena::default();
    let model = arena.create_model().unwrap();
    arena.load_parsed_into_model(model, source).unwrap();
    arena
        .load_plain_visual_resources(model, &installation)
        .unwrap();
    (path, arena, model)
}

#[test]
fn lit_constructor_private_geometry_class_witness_and_close_use_one_owner() {
    let (path, mut arena, model) = fixture(true, 0);
    let frame = arena.model(model).unwrap().frames[0];
    assert!(arena.plain_object_visual_type(frame).is_err());
    let object = arena.frame_object_resources(frame).unwrap().unwrap();
    let resources = arena.loaded_visual_resources().unwrap();
    let record = resources.objects().object(object).unwrap();
    assert_eq!(record.visual_type().source_fourcc(), 0x50414d4c);
    assert_eq!(record.visual_type(), ObjectVisualType::LitUnconstructed);
    assert_eq!(record.lods, [None; 10]);
    let lit = record.lit_object().unwrap();
    assert_eq!(lit.active_level, 0);
    for (index, level) in lit.levels.iter().enumerate() {
        assert!(!level.constructed);
        assert_eq!((level.mode, level.resolution, level.angle), (2, 5., 45.));
        assert_eq!(level.lods.iter().flatten().count(), usize::from(index == 0));
    }
    let lod = lit.levels[0].lods[0].unwrap();
    let view = resources.inspection_mesh(object, 1).unwrap().unwrap();
    assert_eq!(view.uv_byte_offset, 16);
    assert_eq!(
        f32::from_le_bytes(view.vertex_bytes[16..20].try_into().unwrap()),
        0.25
    );
    assert_eq!(
        f32::from_le_bytes(view.vertex_bytes[20..24].try_into().unwrap()),
        0.75
    );
    arena.close(model).unwrap();
    let resources = arena.loaded_visual_resources().unwrap();
    assert!(resources.objects().object(object).is_err());
    assert!(resources.objects().lod(lod).is_err());
    assert!(resources.objects().lod_bindings().iter().any(
        |b| b.lod == lod && b.state == crate::managed_visual_resources::BindingState::Consumed
    ));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn lit_snapshot_uses_cyan_factor_ignores_ordinary_diffuse_opacity_and_preserves_texture_alpha() {
    for textured in [false, true] {
        let (path, mut arena, model) = fixture(textured, if textured { 0 } else { 0xe0000000 });
        let snapshot = snapshot_arena_tree(&mut arena, model.root()).unwrap();
        let mesh = &snapshot.scene.meshes[0];
        let lhr_render::RenderMaterial::Original(policy) = mesh.material else {
            panic!()
        };
        assert_eq!(policy.color, [0., 1., 1., 1.]);
        assert_eq!(policy.alpha, lhr_render::AlphaMode::Opaque);
        assert_eq!(policy.queue, lhr_render::MaterialQueue::Opaque);
        assert_eq!(mesh.vertices[0].uv, [0.25, 0.75]);
        assert_eq!(
            mesh.texture.as_ref().unwrap().rgba,
            if textured {
                vec![128, 64, 32, 17]
            } else {
                vec![255; 4]
            }
        );
        assert_eq!(snapshot.omitted_material_effects, 0);
        arena.close(model).unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn lit_actual_owned_snapshot_reaches_gpu_with_cyan_and_selectarg1_alpha() {
    let instance = lhr_render::instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    // Existing host upload samples sRGB textures into a linear Unorm target:
    // green64 -> 13 and blue32 -> 4. Alpha remains the raw texture17.
    for (textured, expected) in [(false, [0, 255, 255, 255]), (true, [0, 13, 4, 17])] {
        let (path, mut arena, model) = fixture(textured, 0);
        let snapshot = snapshot_arena_tree(&mut arena, model.root()).unwrap();
        let renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
            &adapter,
            &snapshot.scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
        let pixel = &rgba[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4];
        assert_eq!(pixel, expected, "textured={textured}");
        arena.close(model).unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}
