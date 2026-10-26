use super::*;

fn lod_view() -> crate::inspection_lod::InspectionLodView {
    crate::inspection_lod::InspectionLodView {
        camera_pc: [0., 0., -1.],
        rs_lod_quality: 1.,
    }
}

#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn actual_lightmap_owner_atlas_expanded_indices_and_reload_reach_gpu() {
    use crate::loaded_visual_resources::saved_lightmap_fixture::{fixture, payload};
    let instance = lhr_render::instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let mut f = fixture(2, 1);
        let a = f.frame(0);
        let b = f.frame(1);
        for frame in [a, b] {
            f.arena
                .load_frame_saved_lighting(frame, &payload(1, 0x004080c0))
                .unwrap();
        }
        let context = StaticLightmapContext {
            scene: f.scene,
            sector: f.sector,
        };
        assert!(
            snapshot_arena_tree_with_view(&mut f.arena, f.models[0].root(), lod_view()).is_err()
        );
        let before = snapshot_arena_tree_with_view_and_lighting(
            &mut f.arena,
            f.sector.frame(),
            lod_view(),
            context,
        )
        .unwrap();
        assert_eq!(before.scene.meshes.len(), 4);
        let first_atlas = before.scene.meshes[0].lightmap.as_ref().unwrap();
        for mesh in &before.scene.meshes {
            assert!(Arc::ptr_eq(first_atlas, mesh.lightmap.as_ref().unwrap()));
            assert_eq!(mesh.vertices.len(), 6);
            assert!(mesh.vertices.iter().all(|v| v.color == [0.; 3]));
            assert!(
                matches!(mesh.material, lhr_render::RenderMaterial::Original(m) if m.lighting == lhr_render::LightingMode::SavedTexture)
            );
        }
        let mut renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
            &adapter,
            &before.scene,
            format,
            false,
        ))
        .unwrap();
        let pixels = |renderer: &lhr_render::Renderer| {
            let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
            [8usize, 24].map(|p| {
                <[u8; 4]>::try_from(&rgba[(p * 32 + p) * 4..(p * 32 + p) * 4 + 4]).unwrap()
            })
        };
        let check = |got: [u8; 4], expected: [u8; 4]| {
            for (a, b) in got.into_iter().zip(expected) {
                assert!(
                    (i16::from(a) - i16::from(b)).abs() <= 1,
                    "actual {got:?}, expected {expected:?}"
                );
            }
        };
        let initial = pixels(&renderer);
        check(initial[0], [32, 32, 24, 17]);
        check(initial[1], [64, 64, 48, 17]);
        // Reload A while B keeps a reference to the shared real atlas. The
        // next snapshot must publish fresh pixels/UVs, not an identity-only cache.
        f.arena
            .load_frame_saved_lighting(a, &payload(1, 0x00c04080))
            .unwrap();
        let after = snapshot_arena_tree_with_view_and_lighting(
            &mut f.arena,
            f.models[0].root(),
            lod_view(),
            context,
        )
        .unwrap();
        assert!(!Arc::ptr_eq(
            first_atlas,
            after.scene.meshes[0].lightmap.as_ref().unwrap()
        ));
        renderer
            .replace_meshes(0..before.scene.meshes.len(), &after.scene.meshes)
            .unwrap();
        let changed = pixels(&renderer);
        check(changed[0], [32, 32, 24, 17]);
        check(changed[1], [193, 32, 32, 17]);

        let light = f.arena.create_light().unwrap();
        f.arena.sector_add_light(f.sector, light).unwrap();
        assert!(snapshot_arena_tree_with_view_and_lighting(
            &mut f.arena,
            f.models[0].root(),
            lod_view(),
            context
        )
        .is_err());
        f.arena.sector_delete_light(f.sector, light).unwrap();
        f.arena.release(light).unwrap();
        snapshot_arena_tree_with_view_and_lighting(
            &mut f.arena,
            f.models[0].root(),
            lod_view(),
            context,
        )
        .unwrap();
        f.close();
    }
}

#[test]
fn actual_lightmap_snapshot_checks_first_material_emission_only() {
    use crate::loaded_visual_resources::saved_lightmap_fixture::{fixture_with_emissions, payload};
    for (emissions, accepted) in [
        ([[0.; 3], [0.5, 0.25, 1.]], true),
        ([[0.01, 0., 0.], [0.; 3]], false),
    ] {
        let mut f = fixture_with_emissions(1, 1, emissions);
        let frame = f.frame(0);
        f.arena
            .load_frame_saved_lighting(frame, &payload(1, 0x004080c0))
            .unwrap();
        let context = StaticLightmapContext {
            scene: f.scene,
            sector: f.sector,
        };
        let result = snapshot_arena_tree_with_view_and_lighting(
            &mut f.arena,
            f.models[0].root(),
            lod_view(),
            context,
        );
        assert_eq!(result.is_ok(), accepted);
        f.close();
    }
}

#[test]
fn static_lightmap_context_rechecks_real_links_lights_scene_and_near_sector() {
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sectors = arena.scene(scene).unwrap().sectors;
    let model = arena.create_model().unwrap();
    let primary = StaticLightmapContext {
        scene,
        sector: sectors[0],
    };
    let backdrop = StaticLightmapContext {
        scene,
        sector: sectors[1],
    };
    assert!(validate_static_lightmap_context(&arena, model.root(), primary).is_err());
    arena
        .link_frame(model.root(), Some(primary.sector.frame()), 0)
        .unwrap();
    validate_static_lightmap_context(&arena, model.root(), primary).unwrap();
    // Another empty sector is not evidence about this model's lighting.
    assert!(validate_static_lightmap_context(&arena, model.root(), backdrop).is_err());
    let light = arena.create_light().unwrap();
    arena.sector_add_light(primary.sector, light).unwrap();
    assert!(validate_static_lightmap_context(&arena, model.root(), primary).is_err());
    arena.sector_delete_light(primary.sector, light).unwrap();
    validate_static_lightmap_context(&arena, model.root(), primary).unwrap();
    arena.release(light).unwrap();
    arena
        .link_frame(model.root(), Some(backdrop.sector.frame()), 0)
        .unwrap();
    assert!(validate_static_lightmap_context(&arena, model.root(), primary).is_err());
    validate_static_lightmap_context(&arena, model.root(), backdrop).unwrap();
    let foreign_scene = arena.construct_scene_owner_prefix().unwrap();
    assert!(validate_static_lightmap_context(
        &arena,
        model.root(),
        StaticLightmapContext {
            scene: foreign_scene,
            ..backdrop
        }
    )
    .is_err());
    arena
        .link_frame(model.root(), Some(sectors[3].frame()), 0)
        .unwrap();
    assert!(validate_static_lightmap_context(
        &arena,
        model.root(),
        StaticLightmapContext {
            scene,
            sector: sectors[3]
        }
    )
    .is_err());
    arena.link_frame(model.root(), None, 0).unwrap();
    arena.close(model).unwrap();
    arena.release(model.root()).unwrap();
}

#[test]
fn static_lightmap_context_rejects_scene_with_interrupted_close() {
    use crate::scene_ownership::{SceneCloseBackend, SceneCloseEffect};
    struct Interrupt;
    impl SceneCloseBackend for Interrupt {
        fn close_effect(
            &mut self,
            _: &ModelArena,
            _: SceneKey,
            _: SceneCloseEffect,
        ) -> Result<(), crate::model_loading::ModelError> {
            Err(crate::model_loading::ModelError::IncompleteModel)
        }
    }
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sector = arena.scene(scene).unwrap().sectors[0];
    let model = arena.create_model().unwrap();
    arena
        .link_frame(model.root(), Some(sector.frame()), 0)
        .unwrap();
    let context = StaticLightmapContext { scene, sector };
    validate_static_lightmap_context(&arena, model.root(), context).unwrap();
    assert!(arena
        .close_scene_with_backend(scene, &mut Interrupt)
        .is_err());
    assert!(validate_static_lightmap_context(&arena, model.root(), context).is_err());
    // The interrupted owner is intentionally retained, never marked clean.
    assert_ne!(
        arena.scene(scene).unwrap().close_phase(),
        crate::scene_ownership::ClosePhase::Idle
    );
}
