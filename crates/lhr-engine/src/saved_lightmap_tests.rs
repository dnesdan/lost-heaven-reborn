use super::*;
use crate::lightmap_atlas::AtlasStatistics;
use fixture::{body, fixture, payload, record};

fn indices(bytes: &[u8]) -> Vec<u16> {
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|word| u16::from_le_bytes(*word))
        .collect()
}
fn pixel(texture: &lhr_assets::Texture, x: usize, y: usize) -> &[u8] {
    let offset = (y * texture.width as usize + x) * 4;
    &texture.rgba[offset..offset + 4]
}

fn vertex_record(mode: u8, level: u8, colors: [u32; 4]) -> Vec<u8> {
    let mut bytes = vec![0x21, mode];
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&5f32.to_le_bytes());
    bytes.extend_from_slice(&45f32.to_le_bytes());
    bytes.push(level);
    bytes.extend_from_slice(&0u16.to_le_bytes());
    if mode & 4 != 0 {
        bytes.extend_from_slice(&4u32.to_le_bytes());
    }
    for color in colors {
        bytes.extend_from_slice(&color.to_le_bytes());
    }
    bytes
}

#[test]
fn generic_load_owns_mixed_modes_but_vertex_only_entry_keeps_its_strict_boundary() {
    let mut f = fixture(1, 1);
    let frame = f.frame(0);
    let colors = [0x00123456, 0x40789abc, 0x80def012, 0xff345678];
    let (body, _) = body(0x004080c0, false);
    let bytes = [
        vec![0x85],
        record(1, &[body]),
        vertex_record(1, 2, colors),
        vertex_record(5, 3, colors),
    ]
    .concat();
    assert_eq!(
        f.arena.load_frame_saved_lighting(frame, &bytes).unwrap(),
        bytes.len()
    );
    let object = f.arena.frame_object_resources(frame).unwrap().unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    let levels = &owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels;
    assert!(levels[..3].iter().all(|level| level.constructed));
    assert_eq!([levels[0].mode, levels[1].mode, levels[2].mode], [2, 1, 5]);
    assert!(owner
        .inspection_mesh(object, 2)
        .unwrap()
        .unwrap()
        .lightmap
        .is_some());
    let retained: Vec<_> = levels[1..3]
        .iter()
        .map(|level| level.lods[0].unwrap())
        .collect();
    for (level, &lod) in levels[1..3].iter().zip(&retained) {
        assert_eq!(
            level.saved_vertex_colors[0].as_deref(),
            Some(colors.as_slice())
        );
        assert!(owner.objects().lightmap_lod(lod).unwrap().is_none());
        let descriptor = owner.objects().lod(lod).unwrap().vertex_buffer;
        let raw = owner
            .inspection_graphics(
                GraphicsBuffer::Vertex(descriptor.graphics.unwrap()),
                descriptor.base as usize * 24,
                4 * 24,
                Some(0x142),
            )
            .unwrap();
        for (vertex, color) in raw.as_chunks::<24>().0.iter().zip(colors) {
            assert_eq!(
                u32::from_le_bytes(vertex[12..16].try_into().unwrap()),
                color
            );
        }
    }
    f.arena.load_frame_saved_lighting(frame, &[0]).unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    assert_eq!(
        owner.objects().lightmap_atlases().statistics(),
        AtlasStatistics::default()
    );
    for lod in retained {
        assert!(owner.objects().lod(lod).is_ok());
    }
    f.close();

    let mut strict = fixture(1, 1);
    let frame = strict.frame(0);
    let error = strict
        .arena
        .load_frame_saved_vertex_lighting(frame, &payload(1, 0))
        .unwrap_err();
    assert_eq!(error.cursor, 16); // Header mutation is retained, body is not read.
    assert_eq!(
        error.error,
        LoadedVisualError::Backend(
            "saved lightmap/unknown mode requires its own constructed resource owner"
        )
    );
    let owner = strict.arena.loaded_visual_resources().unwrap();
    assert!(owner.objects().lit_index_bindings().is_empty());
    assert_eq!(
        owner.objects().lightmap_atlases().statistics(),
        AtlasStatistics::default()
    );
    strict.close();
}

#[test]
fn actual_lightmap_owner_expands_vertices_and_uses_its_own_index_ranges() {
    let mut f = fixture(1, 2);
    let frame = f.frame(0);
    let object = f.arena.frame_object_resources(frame).unwrap().unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    let old = owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0]
        .lods;
    let source_second_group = owner.inspection_mesh(object, 2).unwrap().unwrap().groups[1]
        .index_bytes
        .to_vec();
    assert_eq!(indices(&source_second_group), [1, 3, 2]);
    let (body, _) = body(0x00604020, true);
    let mut bytes = [vec![0x80], record(1, &[body.clone(), body])].concat();
    let consumed = bytes.len();
    bytes.extend_from_slice(b"caller-owned tail");
    assert_eq!(
        f.arena.load_frame_saved_lighting(frame, &bytes).unwrap(),
        consumed
    );
    let owner = f.arena.loaded_visual_resources().unwrap();
    let lighting = &owner
        .objects()
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0];
    assert!(lighting.constructed);
    assert_eq!(lighting.mode, 2);
    assert!(lighting.saved_vertex_colors.iter().all(Option::is_none));
    for old in old.iter().flatten() {
        assert!(owner.objects().lod(*old).is_err());
    }
    for index in 0..2 {
        let view = owner
            .inspection_mesh_at_lod(object, index, 2)
            .unwrap()
            .unwrap();
        assert_eq!(
            (view.vertex_count, view.vertex_stride, view.uv_byte_offset),
            (6, 32, 24)
        );
        assert!(view.saved_vertex_colors.is_none());
        assert_eq!(indices(view.groups[0].index_bytes), [0, 1, 2]);
        assert_eq!(indices(view.groups[1].index_bytes), [3, 4, 5]);
        let atlas = view.lightmap.unwrap();
        assert_eq!(atlas.uv_byte_offset, 16);
        let data = owner
            .objects()
            .lightmap_lod(lighting.lods[index as usize].unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(
            data.vertex_info[3],
            LitVertexInfo {
                vertex: 1,
                rectangle: 1
            }
        );
        assert_eq!(data.base_uv[3], data.base_uv[1]);
        assert_ne!(data.lightmap_uv[3], data.lightmap_uv[1]);
        assert_eq!(
            data.face_rectangles,
            [vec![0].into_boxed_slice(), vec![1].into_boxed_slice()]
        );
        if index == 0 {
            assert_eq!(data.lightmap_uv[0], [0.5 / 128., 0.5 / 128.]);
            assert_eq!(data.lightmap_uv[1], [3.5 / 128., 0.5 / 128.]);
            assert_eq!(data.lightmap_uv[3], [0.5 / 128., 4.5 / 128.]);
            assert_eq!(pixel(atlas.texture, 1, 2), [50, 50, 80, 255]);
            assert_eq!(pixel(atlas.texture, 0, 4), [96, 64, 32, 255]);
        }
        assert_eq!(
            owner
                .objects()
                .lightmap_atlases()
                .allocation(data.allocation.unwrap())
                .unwrap()
                .source_memory_bytes,
            100
        );
    }
    // The original face-group buffer remains the original mesh's storage.
    let mesh = owner.objects().object(object).unwrap().mesh.unwrap();
    let resources = owner.objects().managed().resources();
    let level = resources
        .mesh_level(resources.mesh_object(mesh).unwrap().levels[0].unwrap())
        .unwrap();
    let first = resources.face_group(level.fgroups.unwrap()).unwrap();
    let second = resources.face_group(first.next.unwrap()).unwrap();
    let raw = owner
        .inspection_graphics(
            GraphicsBuffer::Index(second.index_buffer.graphics.unwrap()),
            second.index_buffer.base as usize * 2,
            6,
            None,
        )
        .unwrap();
    assert_eq!(raw, source_second_group);
    f.close();
}

#[test]
fn shared_atlas_and_private_indices_survive_another_objects_reload() {
    let mut f = fixture(2, 1);
    let a = f.frame(0);
    let b = f.frame(1);
    f.arena
        .load_frame_saved_lighting(a, &payload(1, 0x00604020))
        .unwrap();
    f.arena
        .load_frame_saved_lighting(b, &payload(1, 0x00204060))
        .unwrap();
    let a_object = f.arena.frame_object_resources(a).unwrap().unwrap();
    let b_object = f.arena.frame_object_resources(b).unwrap().unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    let a_lod = owner
        .objects()
        .object(a_object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0]
        .lods[0]
        .unwrap();
    let b_lod = owner
        .objects()
        .object(b_object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[0]
        .lods[0]
        .unwrap();
    let a_allocation = owner
        .objects()
        .lightmap_lod(a_lod)
        .unwrap()
        .unwrap()
        .allocation
        .unwrap();
    let b_allocation = owner
        .objects()
        .lightmap_lod(b_lod)
        .unwrap()
        .unwrap()
        .allocation
        .unwrap();
    let view = owner
        .objects()
        .lightmap_atlases()
        .allocation(b_allocation)
        .unwrap();
    let atlas = view.atlas;
    let b_offsets: Vec<_> = view
        .placements
        .iter()
        .map(|placement| placement.offset)
        .collect();
    let b_pixels: Vec<_> = b_offsets
        .iter()
        .map(|&[x, y]| pixel(view.texture, x as usize, y as usize).to_vec())
        .collect();
    assert_eq!(
        owner
            .objects()
            .lightmap_atlases()
            .allocation(a_allocation)
            .unwrap()
            .atlas,
        atlas
    );
    assert_eq!(
        owner
            .objects()
            .lightmap_atlases()
            .atlas_diagnostics(atlas)
            .unwrap()
            .references,
        2
    );
    f.arena.load_frame_saved_lighting(a, &[0]).unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    assert!(owner.objects().lod(a_lod).is_err());
    assert!(owner.objects().lod(b_lod).is_ok());
    assert!(owner
        .objects()
        .lightmap_atlases()
        .allocation(a_allocation)
        .is_err());
    assert_eq!(
        pixel(
            owner.objects().lightmap_atlases().texture(atlas).unwrap(),
            0,
            0
        ),
        [255; 4]
    );
    assert_eq!(
        owner
            .objects()
            .lit_index_bindings()
            .iter()
            .find(|binding| binding.lod == a_lod)
            .unwrap()
            .state,
        BindingState::Consumed
    );
    f.arena
        .load_frame_saved_lighting(a, &payload(1, 0x00802060))
        .unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    let image = owner.objects().lightmap_atlases().texture(atlas).unwrap();
    for (&[x, y], expected) in b_offsets.iter().zip(b_pixels) {
        assert_eq!(pixel(image, x as usize, y as usize), expected);
    }
    assert_eq!(
        owner.objects().lightmap_atlases().statistics().allocations,
        2
    );
    assert_eq!(owner.objects().lightmap_atlases().statistics().atlases, 1);
    f.close();
}

#[test]
fn partial_second_lod_keeps_first_and_retires_its_pending_index_owner() {
    let (body, offsets) = body(0x00302010, false);
    for tail in [offsets.indices + 3, offsets.face_counts[1] + 5] {
        let mut f = fixture(1, 2);
        let frame = f.frame(0);
        let object = f.arena.frame_object_resources(frame).unwrap().unwrap();
        let mut bytes = [vec![1], record(1, &[body.clone(), body.clone()])].concat();
        bytes.truncate(16 + 2 + body.len() + 2 + tail);
        let error = f
            .arena
            .load_frame_saved_lighting(frame, &bytes)
            .unwrap_err();
        assert_eq!(error.cursor, bytes.len());
        assert!(error.pending_cleanup.is_none());
        let owner = f.arena.loaded_visual_resources().unwrap();
        let level = &owner
            .objects()
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .levels[0];
        assert!(!level.constructed);
        assert!(level.lods[0].is_some());
        assert!(level.lods[1].is_none());
        assert_eq!(
            owner.objects().lit_index_bindings().last().unwrap().state,
            BindingState::Consumed
        );
        assert_eq!(
            owner.objects().lightmap_atlases().statistics().allocations,
            1
        );
        assert!(owner.inspection_mesh(object, 2).is_err());
        f.close();
    }
}

#[test]
fn postprocess_vi_failure_releases_the_actual_shared_atlas_allocation() {
    let mut f = fixture(2, 1);
    let existing = f.frame(0);
    f.arena
        .load_frame_saved_lighting(existing, &payload(1, 0x00604020))
        .unwrap();
    let object = f.arena.frame_object_resources(existing).unwrap().unwrap();
    let owner = f.arena.loaded_visual_resources().unwrap();
    let atlas = owner
        .inspection_mesh(object, 2)
        .unwrap()
        .unwrap()
        .lightmap
        .unwrap()
        .atlas;
    let revision = owner
        .objects()
        .lightmap_atlases()
        .atlas_diagnostics(atlas)
        .unwrap()
        .revision;
    let (mut body, offsets) = body(0x00773311, false);
    body[offsets.vertex_info + 2..offsets.vertex_info + 4].copy_from_slice(&99u16.to_le_bytes());
    let bytes = [vec![1], record(1, &[body])].concat();
    let frame = f.frame(1);
    let error = f
        .arena
        .load_frame_saved_lighting(frame, &bytes)
        .unwrap_err();
    assert_eq!(error.cursor, bytes.len());
    assert_eq!(
        error.error,
        LoadedVisualError::Backend("saved lightmap vertex rectangle outside atlas")
    );
    assert!(error.pending_cleanup.is_none());
    let owner = f.arena.loaded_visual_resources().unwrap();
    let registry = owner.objects().lightmap_atlases();
    assert_eq!(registry.statistics().allocations, 1);
    let diagnostic = registry.atlas_diagnostics(atlas).unwrap();
    assert_eq!(diagnostic.references, 1);
    assert_eq!(diagnostic.revision, revision + 2); // upload then source whitening/reclaim
    assert_eq!(
        pixel(registry.texture(atlas).unwrap(), 0, 0),
        [32, 64, 96, 255]
    );
    assert_eq!(
        owner.objects().lit_index_bindings().last().unwrap().state,
        BindingState::Consumed
    );
    f.close();
}

#[test]
fn mode2_structural_mismatches_preserve_the_reached_read_cursor() {
    let (body, offsets) = body(0x00203040, false);
    for (offset, word, expected_cursor) in [
        (offsets.count, 0u32, 18 + offsets.count + 4),
        (offsets.index_count, 9, 18 + offsets.index_count + 4),
        (offsets.face_counts[0], 2, 18 + offsets.face_counts[0] + 4),
    ] {
        let mut f = fixture(1, 1);
        let mut bad = body.clone();
        bad[offset..offset + 4].copy_from_slice(&word.to_le_bytes());
        let bytes = [vec![1], record(1, &[bad])].concat();
        let frame = f.frame(0);
        let error = f
            .arena
            .load_frame_saved_lighting(frame, &bytes)
            .unwrap_err();
        assert_eq!(error.cursor, expected_cursor);
        assert!(error.pending_cleanup.is_none());
        assert_eq!(
            f.arena
                .loaded_visual_resources()
                .unwrap()
                .objects()
                .lightmap_atlases()
                .statistics(),
            AtlasStatistics::default()
        );
        f.close();
    }
}

#[test]
fn nonfinite_lightmap_uv_retires_the_allocated_atlas_and_private_indices() {
    let mut f = fixture(1, 1);
    let (mut body, offsets) = body(0x004080c0, false);
    body[offsets.uv..offsets.uv + 4].copy_from_slice(&f32::NAN.to_le_bytes());
    let bytes = [vec![1], record(1, &[body])].concat();
    let frame = f.frame(0);
    let error = f
        .arena
        .load_frame_saved_lighting(frame, &bytes)
        .unwrap_err();
    assert_eq!(error.cursor, bytes.len());
    assert_eq!(
        error.error,
        LoadedVisualError::Backend("saved lightmap UV outside finite presentation domain")
    );
    assert!(error.pending_cleanup.is_none());
    let owner = f.arena.loaded_visual_resources().unwrap();
    assert_eq!(
        owner.objects().lightmap_atlases().statistics(),
        AtlasStatistics::default()
    );
    assert_eq!(
        owner.objects().lit_index_bindings().last().unwrap().state,
        BindingState::Consumed
    );
    f.close();
}

#[test]
fn index_allocator_failure_retains_typed_pending_resources_and_primary_cause() {
    let mut f = fixture(1, 1);
    let frame = f.frame(0);
    let (body, offsets) = body(0x00302010, false);
    let bytes = [vec![1], record(1, &[body])].concat();
    f.arena
        .loaded_visual_resources_mut()
        .unwrap()
        .backend
        .fail_next_graphics_allocation = true;
    let error = f
        .arena
        .load_frame_saved_lighting(frame, &bytes)
        .unwrap_err();
    assert_eq!(error.cursor, 18 + offsets.index_count + 4);
    assert_eq!(
        error.error,
        LoadedVisualError::Backend("injected CPU graphics allocation failure")
    );
    let pending = error.pending_cleanup.unwrap();
    assert_eq!(
        pending.error,
        LoadedVisualError::Object(ObjectError::Poisoned)
    );
    let owner = f.arena.loaded_visual_resources().unwrap();
    assert!(owner.objects().is_poisoned());
    let data = owner.objects().lightmap_lod(pending.lod).unwrap().unwrap();
    assert_eq!(data.vertex_count, 6);
    assert_eq!(data.vertex_info.len(), 6);
    assert_eq!(data.pending_rectangles.len(), 2);
    let RectangleInput::Rgb {
        width,
        height,
        pixels,
    } = &data.pending_rectangles[0]
    else {
        panic!("the parsed RGB input must stay with its poisoned pending owner");
    };
    assert_eq!((*width, *height), (4, 4));
    assert_eq!(pixels.as_ref(), [32, 64, 96].repeat(16));
    assert_eq!(
        data.pending_rectangles[1],
        RectangleInput::SingleColor { argb: 0x00302010 }
    );
    assert!(data.allocation.is_none());
    assert!(!data.ready);
    assert_eq!(
        owner.objects().lit_index_bindings().last().unwrap().state,
        BindingState::Pending
    );
    assert_eq!(
        owner.objects().lightmap_atlases().statistics(),
        AtlasStatistics::default()
    );
    let retained = (
        owner.backend().live_graphics(),
        owner.backend().live_allocations(),
    );
    assert!(f.arena.close(f.models[0]).is_err());
    let owner = f.arena.loaded_visual_resources().unwrap();
    assert_eq!(
        (
            owner.backend().live_graphics(),
            owner.backend().live_allocations()
        ),
        retained
    );
    let path = f.path.clone();
    drop(f); // host memory release, not successful source cleanup
    std::fs::remove_dir_all(path).unwrap();
}
