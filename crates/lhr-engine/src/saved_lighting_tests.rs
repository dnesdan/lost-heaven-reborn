use super::*;
use crate::{
    lit_object_tests::fixture_lods,
    model_loading::{ModelArena, ModelKey},
};
use std::path::PathBuf;

fn record(mode: u8, number: u8, colors: &[&[u32]]) -> Vec<u8> {
    let mut bytes = vec![0x21, mode];
    bytes.extend_from_slice(&(colors.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&f32::from_bits(0x7fc01234).to_le_bytes());
    bytes.extend_from_slice(&(-0.0f32).to_le_bytes());
    bytes.push(number);
    for color in colors {
        bytes.extend_from_slice(&0xa501u16.to_le_bytes());
        if mode & 4 != 0 {
            bytes.extend_from_slice(&(color.len() as u32).to_le_bytes());
        }
        for value in *color {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
    bytes
}
fn payload(mode: u8, number: u8, colors: &[&[u32]]) -> Vec<u8> {
    [vec![0x80], record(mode, number, colors)].concat()
}
fn close(path: PathBuf, mut arena: ModelArena, model: ModelKey) {
    arena.close(model).unwrap();
    arena.release(model.root()).unwrap();
    let owner = arena.loaded_visual_resources_mut().unwrap();
    owner.reclaim_unused_buffers().unwrap();
    assert_eq!(owner.backend().live_graphics(), 0);
    assert_eq!(owner.backend().live_textures(), 0);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn saved_vertex_lighting_reaches_owned_buffers_with_real_selected_lod_layout() {
    for mode in [1, 5] {
        let (path, mut arena, model) = fixture_lods(true, 0, &[5., 10.]);
        let frame = arena.model(model).unwrap().frames[0];
        let object = arena.frame_object_resources(frame).unwrap().unwrap();
        let old = arena
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0]
            .lods;
        let near = [0x00112233, 0x7f445566, 0xffaabbcc];
        let far = [0xff010203, 0x80040506, 0x00070809, 0x400a0b0c];
        let mut data = payload(mode, 1, &[&near, &far]);
        let end = data.len();
        data.extend_from_slice(b"next chunk is caller owned");
        assert_eq!(
            arena
                .load_frame_saved_vertex_lighting(frame, &data)
                .unwrap(),
            end
        );
        let owner = arena.loaded_visual_resources().unwrap();
        let state = owner.objects().object(object).unwrap();
        assert_eq!(state.visual_type(), ObjectVisualType::Lit);
        let lighting = &state.lit_object().unwrap().levels[0];
        assert!(lighting.constructed);
        assert_eq!(lighting.mode, u32::from(mode));
        assert_eq!(lighting.resolution.to_bits(), 0x7fc01234);
        assert_eq!(lighting.angle.to_bits(), (-0.0f32).to_bits());
        for old in old.iter().flatten() {
            assert!(owner.objects().lod(*old).is_err());
        }
        for (index, colors) in [near.as_slice(), far.as_slice()].into_iter().enumerate() {
            let view = owner
                .inspection_mesh_at_lod(object, index as u32, 10)
                .unwrap()
                .unwrap();
            assert_eq!(view.vertex_stride, 24);
            assert_eq!(view.uv_byte_offset, 16);
            assert_eq!(view.saved_vertex_colors, Some(colors));
            for (bytes, color) in view.vertex_bytes.as_chunks::<24>().0.iter().zip(colors) {
                assert_eq!(
                    u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
                    *color
                );
            }
            assert_eq!(
                f32::from_le_bytes(view.vertex_bytes[16..20].try_into().unwrap()),
                if index == 0 { 0.25 } else { 0.75 }
            );
        }
        close(path, arena, model);
    }
}

#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn loaded_saved_colors_reach_gpu_and_reload_preserves_texture_alpha() {
    let instance = lhr_render::instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("saved lighting test needs Vulkan or Metal");
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let (path, mut arena, model) = fixture_lods(true, 0, &[0.]);
        let frame = arena.model(model).unwrap().frames[0];
        let mut renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
            &adapter,
            &lhr_render::Scene { meshes: Vec::new() },
            format,
            false,
        ))
        .unwrap();
        let mut old_lod = None;
        let mut draw_count = 0;
        for (mode, color, expected) in [
            (1, 0x0040c080, [64u8, 96, 32, 17]),
            (5, 0x00ff2060, [255, 16, 24, 17]),
        ] {
            let data = payload(mode, 1, &[&[color; 3]]);
            assert_eq!(
                arena
                    .load_frame_saved_vertex_lighting(frame, &data)
                    .unwrap(),
                data.len()
            );
            let object = arena.frame_object_resources(frame).unwrap().unwrap();
            let owner = arena.loaded_visual_resources().unwrap();
            if let Some(old) = old_lod {
                assert!(owner.objects().lod(old).is_err());
            }
            old_lod = owner
                .objects()
                .object(object)
                .unwrap()
                .lit_object()
                .unwrap()
                .levels[0]
                .lods[0];
            let snapshot = crate::car_inspection::snapshot_arena_tree_with_view(
                &mut arena,
                model.root(),
                crate::inspection_lod::InspectionLodView {
                    camera_pc: [0., 0., -1.],
                    rs_lod_quality: 1.,
                },
            )
            .unwrap();
            assert_eq!(snapshot.scene.meshes.len(), 1);
            let mesh = &snapshot.scene.meshes[0];
            let lhr_render::RenderMaterial::Original(policy) = mesh.material else {
                panic!("actual lit owner did not select its material policy")
            };
            assert!(policy.vertex_lighting);
            assert_eq!(policy.color, [1.; 4]);
            assert_eq!(mesh.texture.as_ref().unwrap().rgba, [128, 64, 32, 17]);
            renderer
                .replace_meshes(0..draw_count, &snapshot.scene.meshes)
                .unwrap();
            draw_count = snapshot.scene.meshes.len();
            let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
            let pixel = &rgba[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4];
            // Source stage zero uses raw texture bytes with RGB MODULATE2X;
            // SELECTARG1 keeps texture alpha 17 even though saved alpha is 0.
            // The fixture's red diffuse, emission and opacity must not tint it.
            for (&got, wanted) in pixel.iter().zip(expected) {
                assert!(
                    (i16::from(got) - i16::from(wanted)).abs() <= 1,
                    "{format:?}, mode {mode}: pixel {pixel:?}, expected {expected:?}"
                );
            }
        }
        close(path, arena, model);
    }
}

#[test]
fn successful_reload_destroys_only_active_lighting_and_zero_mask_restores_unconstructed_view() {
    let (path, mut arena, model) = fixture_lods(true, 0, &[0.]);
    let frame = arena.model(model).unwrap().frames[0];
    let object = arena.frame_object_resources(frame).unwrap().unwrap();
    let first = [0xff001122; 3];
    let second = [0x00123456; 3];
    let data = [
        vec![0x81],
        record(1, 1, &[&first]),
        record(5, 2, &[&second]),
    ]
    .concat();
    arena
        .load_frame_saved_vertex_lighting(frame, &data)
        .unwrap();
    let other = arena
        .loaded_visual_resources()
        .unwrap()
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[1]
        .lods[0]
        .unwrap();
    let old_active = arena
        .loaded_visual_resources()
        .unwrap()
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0]
        .lods[0]
        .unwrap();
    arena
        .load_frame_saved_vertex_lighting(frame, &payload(5, 1, &[&second]))
        .unwrap();
    let owner = arena.loaded_visual_resources().unwrap();
    assert!(owner.objects().lod(old_active).is_err());
    assert!(owner.objects().lod(other).is_ok());
    assert_eq!(
        owner
            .inspection_mesh(object, 1)
            .unwrap()
            .unwrap()
            .saved_vertex_colors,
        Some(second.as_slice())
    );
    assert_eq!(
        arena
            .load_frame_saved_vertex_lighting(frame, &[0, 99])
            .unwrap(),
        1
    );
    let owner = arena.loaded_visual_resources().unwrap();
    let state = owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap();
    assert!(!state.levels[0].constructed);
    assert!(state.levels[0].saved_vertex_colors[0].is_none());
    assert_eq!(
        state.levels[1].saved_vertex_colors[0].as_deref(),
        Some(second.as_slice())
    );
    assert!(owner.objects().lod(other).is_ok());
    let view = owner.inspection_mesh(object, 1).unwrap().unwrap();
    assert_eq!(view.vertex_stride, 32);
    assert!(view.saved_vertex_colors.is_none());
    close(path, arena, model);
}

#[test]
fn failure_keeps_read_cursor_header_mutations_and_completed_lod_prefix() {
    let colors = [0xff112233; 3];
    let far = [0xff445566; 4];
    for tail_bytes in 0..16 {
        let (path, mut arena, model) = fixture_lods(true, 0, &[5., 10.]);
        let frame = arena.model(model).unwrap().frames[0];
        let object = arena.frame_object_resources(frame).unwrap().unwrap();
        let mut data = payload(1, 1, &[&colors, &far]);
        data.truncate(data.len() - 16 + tail_bytes);
        let error = arena
            .load_frame_saved_vertex_lighting(frame, &data)
            .unwrap_err();
        assert_eq!(error.cursor, data.len());
        let owner = arena.loaded_visual_resources().unwrap();
        assert!(owner.has_incomplete_admission());
        let state = owner
            .objects()
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap();
        assert!(!state.levels[0].constructed);
        assert_eq!(state.levels[0].mode, 1);
        assert_eq!(
            state.levels[0].saved_vertex_colors[0].as_deref(),
            Some(colors.as_slice())
        );
        assert!(state.levels[0].lods[0].is_some());
        assert!(state.levels[0].lods[1].is_none());
        assert!(state.levels[0].saved_vertex_colors[1].is_none());
        assert!(owner.inspection_mesh(object, 10).is_err());
        assert_eq!(
            arena
                .load_frame_saved_vertex_lighting(frame, &[0])
                .unwrap_err()
                .cursor,
            0
        );
        close(path, arena, model);
    }
}

#[test]
fn allocator_failure_preserves_primary_error_cursor_and_blocked_pending_owner() {
    let (path, mut arena, model) = fixture_lods(true, 0, &[0.]);
    let frame = arena.model(model).unwrap().frames[0];
    let object = arena.frame_object_resources(frame).unwrap().unwrap();
    let owner = arena.loaded_visual_resources_mut().unwrap();
    let old = owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0]
        .lods[0]
        .unwrap();
    owner.backend.fail_next_graphics_allocation = true;
    let data = payload(5, 1, &[&[0x00123456; 3]]);
    let error = arena
        .load_frame_saved_vertex_lighting(frame, &data)
        .unwrap_err();
    assert_eq!(error.cursor, data.len());
    assert_eq!(
        error.error,
        LoadedVisualError::Backend("injected CPU graphics allocation failure")
    );
    let pending = error.pending_cleanup.as_ref().unwrap();
    assert_eq!(
        pending.error,
        LoadedVisualError::Object(ObjectError::Poisoned)
    );
    let owner = arena.loaded_visual_resources().unwrap();
    assert!(owner.has_incomplete_admission());
    assert!(owner.objects().is_poisoned());
    assert!(owner.objects().lod(old).is_err());
    let lod = owner.objects().lod(pending.lod).unwrap();
    assert!(lod.owner.is_none());
    let binding = owner.objects().lod_bindings().last().unwrap();
    assert_eq!(binding.lod, pending.lod);
    assert_eq!(
        binding.state,
        crate::managed_visual_resources::BindingState::Pending
    );
    let lighting = &owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0];
    assert!(!lighting.constructed);
    assert!(lighting.lods[0].is_none());
    assert!(lighting.saved_vertex_colors[0].is_none());
    let retained = (
        owner.backend().live_allocations(),
        owner.backend().live_graphics(),
        owner.backend().live_textures(),
    );
    assert!(arena.close(model).is_err());
    let owner = arena.loaded_visual_resources().unwrap();
    assert_eq!(
        (
            owner.backend().live_allocations(),
            owner.backend().live_graphics(),
            owner.backend().live_textures(),
        ),
        retained
    );
    assert!(owner.objects().lod(pending.lod).is_ok());
    // The poisoned diagnostic owner is retained until the host arena is
    // dropped. No successful native-style cleanup or rollback is claimed.
    drop(arena);
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn unsupported_headers_stop_after_real_destruct_and_header_stores_without_claiming_lightmaps() {
    for (version, mode, number, count) in [
        (0x20, 1, 1, 1),
        (0x21, 2, 1, 1),
        (0x21, 1, 1, 2),
        (0x21, 1, 0, 1),
    ] {
        let (path, mut arena, model) = fixture_lods(true, 0, &[0.]);
        let frame = arena.model(model).unwrap().frames[0];
        let object = arena.frame_object_resources(frame).unwrap().unwrap();
        let old = arena
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0]
            .lods[0]
            .unwrap();
        let mut data = payload(mode, number, &[&[0xff112233; 3]]);
        data[1] = version;
        data[3..7].copy_from_slice(&u32::to_le_bytes(count));
        let error = arena
            .load_frame_saved_vertex_lighting(frame, &data)
            .unwrap_err();
        assert_eq!(error.cursor, 16);
        let owner = arena.loaded_visual_resources().unwrap();
        assert!(owner.objects().lod(old).is_err());
        let level = &owner
            .objects()
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0];
        assert!(!level.constructed);
        assert!(level.lods.iter().all(Option::is_none));
        if number != 0 {
            assert_eq!(level.mode, u32::from(mode));
            assert_eq!(level.resolution.to_bits(), 0x7fc01234);
        }
        close(path, arena, model);
    }
}

#[test]
fn saved_level_above_six_targets_first_level_but_mask_bits_do_not_select_the_level() {
    for number in [7, 255] {
        let (path, mut arena, model) = fixture_lods(true, 0, &[0.]);
        let frame = arena.model(model).unwrap().frames[0];
        let object = arena.frame_object_resources(frame).unwrap().unwrap();
        arena
            .load_frame_saved_vertex_lighting(frame, &payload(1, number, &[&[1, 2, 3]]))
            .unwrap();
        let owner = arena.loaded_visual_resources().unwrap();
        assert_eq!(
            owner
                .inspection_mesh(object, 1)
                .unwrap()
                .unwrap()
                .saved_vertex_colors,
            Some([1, 2, 3].as_slice())
        );
        close(path, arena, model);
    }
}

#[test]
fn explicit_vertex_count_mismatch_does_not_publish_a_new_lod_or_consume_colors() {
    let (path, mut arena, model) = fixture_lods(true, 0, &[0.]);
    let frame = arena.model(model).unwrap().frames[0];
    let object = arena.frame_object_resources(frame).unwrap().unwrap();
    let data = payload(5, 1, &[&[0xff112233; 2]]);
    let error = arena
        .load_frame_saved_vertex_lighting(frame, &data)
        .unwrap_err();
    assert_eq!(error.cursor, 22);
    let owner = arena.loaded_visual_resources().unwrap();
    let level = &owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0];
    assert!(level.lods.iter().all(Option::is_none));
    assert!(level.saved_vertex_colors.iter().all(Option::is_none));
    close(path, arena, model);
}
