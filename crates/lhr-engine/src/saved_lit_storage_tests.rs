//! Canonical source->payload->managed storage->snapshot regressions.
use super::*;
use crate::{
    i3d_object::{LitStorageCounts, LitStoragePhase},
    model_loading::{ModelArena, ModelKey},
};
use std::{path::PathBuf, sync::Arc};
struct Fixture {
    path: PathBuf,
    arena: ModelArena,
    model: ModelKey,
    object: ObjectResourceKey,
}
impl Fixture {
    fn new(lods: usize) -> Self {
        let path = std::env::temp_dir().join(format!(
            "lhr-lit-storage-{}-{}",
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
        tga.extend([32, 64, 128, 17]);
        std::fs::write(path.join("maps/synthetic.tga"), tga).unwrap();
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.visual = Some(1);
        frame.logic_flags = 0;
        frame.render_flags = 0;
        let near = lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., 1.],
                    uv: [0.25, 0.75],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        };
        frame.lods.push(near.clone());
        if lods == 2 {
            let mut far = near;
            far.distance = 100.;
            for v in &mut far.vertices {
                v.uv = [0.75, 0.25];
            }
            far.vertices.push(lhr_assets::Vertex {
                position: [2., 2., 0.],
                normal: [0., 0., 1.],
                uv: [0.75, 0.25],
            });
            far.groups[0].indices = vec![0, 1, 2, 1, 3, 2];
            frame.lods.push(far);
        }
        let source = Arc::new(lhr_assets::Model {
            frames: vec![frame],
            materials: vec![lhr_assets::Material {
                flags: 0x10040000,
                ambient: [0.; 3],
                diffuse: [1., 0., 0.],
                texture: "synthetic.tga".into(),
                emission: [0.; 3],
                opacity: 0.25,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
        });
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let frames = arena.load_parsed_into_model(model, source).unwrap();
        let frame = frames.frames()[0];
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let object = arena.frame_object_resources(frame).unwrap().unwrap();
        Self {
            path,
            arena,
            model,
            object,
        }
    }
    fn owner(&mut self) -> &mut LoadedVisualResources {
        self.arena.loaded_visual_resources_mut().unwrap()
    }
    fn load(&mut self, bytes: &[u8]) -> usize {
        let object = self.object;
        self.owner()
            .load_saved_lightmaps(object, bytes, SavedLightmapLoadContext::TrueColorRgba8)
            .unwrap()
    }
    fn prepare(&mut self, index: u32) {
        let object = self.object;
        self.owner()
            .prepare_saved_lit_storage(object, index)
            .unwrap();
    }
    fn close(mut self) {
        self.arena.close(self.model).unwrap();
        self.arena.release(self.model.root()).unwrap();
        let owner = self.owner();
        owner.reclaim_unused_buffers().unwrap();
        assert_eq!(
            owner.objects.lit_storage_counts(),
            LitStorageCounts::default()
        );
        assert_eq!(
            (
                owner.backend.live_allocations(),
                owner.backend.live_graphics(),
                owner.backend.live_textures()
            ),
            (0, 0, 0)
        );
        std::fs::remove_dir_all(self.path).unwrap();
    }
}
fn header(mode: u8, level: u8, lods: u32) -> Vec<u8> {
    let mut b = vec![0x21, mode];
    b.extend(lods.to_le_bytes());
    b.extend(5f32.to_le_bytes());
    b.extend((-0f32).to_le_bytes());
    b.push(level);
    b
}
fn colors(mode: u8, level: u8, lods: &[&[u32]]) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(header(mode, level, lods.len() as u32));
    for colors in lods {
        b.extend(0xa501u16.to_le_bytes());
        if mode & 4 != 0 {
            b.extend((colors.len() as u32).to_le_bytes());
        }
        for c in *colors {
            b.extend(c.to_le_bytes());
        }
    }
    b
}
fn lightmap(level: u8, expanded: bool) -> Vec<u8> {
    let mut b = vec![1];
    b.extend(header(2, level, 1));
    b.extend(0xa501u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.push(1);
    b.extend(0xff204080u32.to_le_bytes());
    b.extend(0u32.to_le_bytes());
    let count = if expanded { 4u32 } else { 3 };
    b.extend(count.to_le_bytes());
    for i in 0..count {
        let uv = match i {
            0 => [0f32, 0.],
            1 => [1., 0.],
            2 => [0., 1.],
            _ => [0.5, 0.5],
        };
        for f in uv {
            b.extend(f.to_le_bytes());
        }
    }
    for i in 0..count {
        let source = if expanded {
            [2u16, 0, 1, 0][i as usize]
        } else {
            i as u16
        };
        b.extend(source.to_le_bytes());
        b.extend(0u16.to_le_bytes());
    }
    b.extend(3u32.to_le_bytes());
    for i in if expanded { [1u16, 2, 0] } else { [0, 1, 2] } {
        b.extend(i.to_le_bytes());
    }
    b.extend(1u32.to_le_bytes());
    b.extend(0u16.to_le_bytes());
    b
}
fn lod_key(
    owner: &LoadedVisualResources,
    object: ObjectResourceKey,
    level: usize,
    index: usize,
) -> crate::i3d_object::PlainLodKey {
    owner
        .objects
        .object(object)
        .unwrap()
        .lit_object()
        .unwrap()
        .levels[level]
        .lods[index]
        .unwrap()
}
#[test]
fn selected_vertex_lods_use_actual_typed_manager_layout_and_live_bytes() {
    for mode in [1, 5] {
        let mut f = Fixture::new(2);
        let near = [0x00112233, 0x7f445566, 0xffaabbcc];
        let far = [0xff010203, 0x80040506, 0x00070809, 0x400a0b0c];
        let mut b = colors(mode, 1, &[&near, &far]);
        let end = b.len();
        b.extend(b"caller tail");
        assert_eq!(f.load(&b), end);
        let object = f.object;
        assert_eq!(f.owner().objects.lit_storage_counts().ready_lods, 0);
        for (index, wanted) in [near.as_slice(), far.as_slice()].into_iter().enumerate() {
            f.prepare(index as u32);
            let key = lod_key(f.owner(), object, 0, index);
            assert!(!f.owner().objects.lod(key).unwrap().dirty);
            let view = f
                .owner()
                .inspection_mesh_at_lod(object, index as u32, 10)
                .unwrap()
                .unwrap();
            assert_eq!(
                (
                    view.vertex_stride,
                    view.uv_byte_offset,
                    view.color_byte_offset
                ),
                (24, 16, Some(12))
            );
            assert_eq!(view.vertex_count as usize, wanted.len());
            for (v, c) in view.vertex_bytes.as_chunks::<24>().0.iter().zip(wanted) {
                assert_eq!(u32::from_le_bytes(v[12..16].try_into().unwrap()), *c);
            }
            assert_eq!(
                f32::from_le_bytes(view.vertex_bytes[16..20].try_into().unwrap()),
                if index == 0 { 0.25 } else { 0.75 }
            );
        }
        let owner = f.owner();
        let key = lod_key(owner, object, 0, 0);
        let descriptor = owner.objects.lod(key).unwrap().vertex_buffer;
        let actual = owner
            .backend
            .get_graphics_mut(GraphicsBuffer::Vertex(descriptor.graphics.unwrap()))
            .unwrap();
        actual.bytes[descriptor.base as usize * 24 + 12..descriptor.base as usize * 24 + 16]
            .copy_from_slice(&0x00f10203u32.to_le_bytes());
        let view = owner
            .inspection_mesh_at_lod(object, 0, 10)
            .unwrap()
            .unwrap();
        assert_eq!(&view.vertex_bytes[12..16], &0x00f10203u32.to_le_bytes());
        let crate::i3d_object::saved_lightmaps::SavedLitLod::VertexColors { colors, .. } =
            owner.saved_lit_lod(object, 0).unwrap().unwrap()
        else {
            panic!()
        };
        assert_eq!(colors[0], near[0]); // No decoded-payload fallback hides the live manager change.
        f.close();
    }
}
#[test]
fn expanded_lightmap_geometry_indices_and_uvs_come_from_the_managed_lod() {
    let mut f = Fixture::new(1);
    f.load(&lightmap(1, true));
    f.prepare(0);
    let object = f.object;
    let owner = f.owner();
    let view = owner
        .inspection_mesh_at_lod(object, 0, 10)
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            view.vertex_stride,
            view.vertex_count,
            view.lightmap_uv_byte_offset
        ),
        (32, 4, Some(16))
    );
    assert_eq!(view.groups[0].index_bytes, [1, 0, 2, 0, 0, 0]);
    assert_eq!(
        f32::from_le_bytes(view.vertex_bytes[4..8].try_into().unwrap()),
        2.
    );
    let key = lod_key(owner, object, 0, 0);
    let storage = owner.objects.lit_lod_storage(key).unwrap();
    let ib = storage.index_buffer;
    let vb = owner.objects.lod(key).unwrap().vertex_buffer;
    owner
        .backend
        .get_graphics_mut(GraphicsBuffer::Index(ib.graphics.unwrap()))
        .unwrap()
        .bytes[ib.base as usize * 2..ib.base as usize * 2 + 2]
        .copy_from_slice(&3u16.to_le_bytes());
    owner
        .backend
        .get_graphics_mut(GraphicsBuffer::Vertex(vb.graphics.unwrap()))
        .unwrap()
        .bytes[vb.base as usize * 32 + 16..vb.base as usize * 32 + 20]
        .copy_from_slice(&0.125f32.to_le_bytes());
    let model = f.model;
    let snapshot = crate::car_inspection::snapshot_arena_tree(&mut f.arena, model.root()).unwrap();
    assert_eq!(snapshot.scene.meshes[0].vertices.len(), 4);
    assert_eq!(snapshot.scene.meshes[0].indices, [3, 2, 0]);
    assert_eq!(snapshot.lighting[0].as_ref().unwrap().uv2[0][0], 0.125);
    assert_eq!(f.owner().objects.lit_storage_counts().live_index_owners, 1);
    f.close();
}
#[test]
fn source_vertex_writes_rebuild_the_selected_vb_and_retain_its_actual_ib() {
    let mut f = Fixture::new(1);
    f.load(&lightmap(1, true));
    f.prepare(0);
    let object = f.object;
    let owner = f.owner();
    let key = lod_key(owner, object, 0, 0);
    let ib = owner.objects.lit_lod_storage(key).unwrap().index_buffer;
    let source = owner
        .objects
        .lit_lod_storage(key)
        .unwrap()
        .source_binding
        .unwrap();
    owner
        .backend
        .upload_graphics(
            GraphicsBuffer::Vertex(source.vertices.graphics.unwrap()),
            source.vertices.base as usize * 32,
            &3f32.to_le_bytes(),
        )
        .unwrap();
    owner.prepare_saved_lit_storage(object, 0).unwrap();
    assert_eq!(owner.objects.lit_lod_storage(key).unwrap().index_buffer, ib);
    let view = owner
        .inspection_mesh_at_lod(object, 0, 10)
        .unwrap()
        .unwrap();
    assert_eq!(
        f32::from_le_bytes(view.vertex_bytes[32..36].try_into().unwrap()),
        3.
    );
    let counts = owner.objects.lit_storage_counts();
    assert_eq!(
        (
            counts.ready_lods,
            counts.live_vertex_owners,
            counts.live_index_owners
        ),
        (1, 1, 1)
    );
    f.close();
}
#[test]
fn switching_lighting_frees_only_vb_and_same_level_rebuilds_with_the_same_ib() {
    let mut f = Fixture::new(1);
    let a = lightmap(1, false);
    let b = lightmap(2, false);
    let data = [vec![3], a[1..].to_vec(), b[1..].to_vec()].concat();
    f.load(&data);
    f.prepare(0);
    let object = f.object;
    let owner = f.owner();
    let first = lod_key(owner, object, 0, 0);
    let second = lod_key(owner, object, 1, 0);
    let ib = owner.objects.lit_lod_storage(first).unwrap().index_buffer;
    let clone = owner.saved_lit_lod(object, 0).unwrap().unwrap().clone();
    owner.set_active_lighting(object, 1).unwrap();
    assert_eq!(
        owner.objects.lod(first).unwrap().vertex_buffer,
        Default::default()
    );
    assert_eq!(owner.objects.lit_storage_counts().live_index_owners, 1);
    assert_eq!(owner.objects.lit_storage_counts().ready_lods, 0);
    owner.prepare_saved_lit_storage(object, 0).unwrap();
    assert_eq!(owner.objects.lit_storage_counts().live_index_owners, 2);
    owner.set_active_lighting(object, 0).unwrap();
    assert_eq!(
        owner.objects.lod(second).unwrap().vertex_buffer,
        Default::default()
    );
    owner.prepare_saved_lit_storage(object, 0).unwrap();
    assert_eq!(
        owner.objects.lit_lod_storage(first).unwrap().index_buffer,
        ib
    );
    owner.set_active_lighting(object, 0).unwrap();
    assert_eq!(owner.objects.lit_storage_counts().live_vertex_owners, 0);
    owner.prepare_saved_lit_storage(object, 0).unwrap();
    assert_eq!(owner.objects.lit_storage_counts().live_vertex_owners, 1);
    owner
        .load_saved_lightmaps(object, &[0], SavedLightmapLoadContext::TrueColorRgba8)
        .unwrap();
    assert!(owner.objects.lod(first).is_err());
    assert!(owner.objects.lod(second).is_ok());
    assert_eq!(owner.objects.lit_storage_counts().live_index_owners, 1);
    drop(clone);
    f.close();
}
#[test]
fn nonactive_payload_replacement_retires_its_actual_ib_without_touching_other_slots() {
    let mut f = Fixture::new(1);
    let a = lightmap(1, false);
    let b = lightmap(2, false);
    f.load(&[vec![3], a[1..].to_vec(), b[1..].to_vec()].concat());
    f.prepare(0);
    let object = f.object;
    f.owner().set_active_lighting(object, 1).unwrap();
    f.prepare(0);
    let old = lod_key(f.owner(), object, 1, 0);
    f.owner().set_active_lighting(object, 0).unwrap();
    assert_eq!(f.owner().objects.lit_storage_counts().live_index_owners, 2);
    f.load(&b);
    assert!(f.owner().objects.lod(old).is_err());
    assert_eq!(f.owner().objects.lit_storage_counts().live_index_owners, 0);
    let current = lod_key(f.owner(), object, 1, 0);
    assert_ne!(old, current);
    assert_eq!(
        f.owner()
            .objects
            .lit_lod_storage_records()
            .iter()
            .find(|s| s.lod == old)
            .unwrap()
            .phase,
        LitStoragePhase::Retired
    );
    f.owner().set_active_lighting(object, 1).unwrap();
    f.prepare(0);
    assert_eq!(f.owner().objects.lit_storage_counts().live_index_owners, 1);
    f.close();
}
#[test]
fn foreign_owner_is_rejected_before_any_storage_effect() {
    let mut a = Fixture::new(1);
    let b = Fixture::new(1);
    a.load(&lightmap(1, false));
    let foreign = b.object;
    let owner = a.owner();
    let before = (
        owner.backend.live_allocations(),
        owner.backend.live_graphics(),
        owner.objects.lit_storage_counts(),
    );
    assert!(owner.prepare_saved_lit_storage(foreign, 0).is_err());
    assert_eq!(
        (
            owner.backend.live_allocations(),
            owner.backend.live_graphics(),
            owner.objects.lit_storage_counts()
        ),
        before
    );
    assert!(!owner.incomplete);
    a.close();
    b.close();
}
#[test]
fn reused_incompatible_vb_layout_is_refused_before_writing_any_shared_block() {
    let mut f = Fixture::new(1);
    f.load(&colors(1, 1, &[&[0xff102030; 3]]));
    let object = f.object;
    let owner = f.owner();
    let spare = owner.objects.create_plain_lod().unwrap();
    owner
        .objects
        .allocate_plain_lod_vertices(
            spare,
            VertexRequest {
                fvf: 0x142,
                usage: 0x8,
                stride: 32,
                count: 3,
                format: 6,
                exclusive: false,
            },
            &mut owner.backend,
        )
        .unwrap();
    let descriptor = owner.objects.lod(spare).unwrap().vertex_buffer;
    owner
        .backend
        .upload_graphics(
            GraphicsBuffer::Vertex(descriptor.graphics.unwrap()),
            descriptor.base as usize * 32,
            &[0xa5; 96],
        )
        .unwrap();
    owner
        .objects
        .unlock_all_vertices(&mut owner.backend)
        .unwrap();
    let error = owner.prepare_saved_lit_storage(object, 0).unwrap_err();
    assert_eq!(
        error,
        LoadedVisualError::Backend("saved lighting manager returned an incompatible layout")
    );
    let actual = owner
        .backend
        .get_graphics_mut(GraphicsBuffer::Vertex(descriptor.graphics.unwrap()))
        .unwrap();
    assert_eq!(
        &actual.bytes[descriptor.base as usize * 32..descriptor.base as usize * 32 + 96],
        &[0xa5; 96]
    );
    assert!(owner.incomplete);
    assert!(!owner.objects.is_poisoned());
    assert_eq!(owner.objects.lit_storage_counts().interrupted_lods, 1);
    owner
        .objects
        .unlock_all_vertices(&mut owner.backend)
        .unwrap();
    owner
        .objects
        .destroy_plain_lod(spare, &mut owner.backend)
        .unwrap();
    f.close();
}
#[test]
fn real_allocation_upload_and_unlock_failures_keep_reachable_nonreplayable_prefixes() {
    for stage in [
        CpuStorageTestFailure::BlockAllocate(BufferKind::Index),
        CpuStorageTestFailure::BlockAllocate(BufferKind::Vertex),
        CpuStorageTestFailure::UploadAfterWrite(BufferKind::Index),
        CpuStorageTestFailure::UploadAfterWrite(BufferKind::Vertex),
        CpuStorageTestFailure::UnlockAfterUnlock(BufferKind::Index),
        CpuStorageTestFailure::UnlockAfterUnlock(BufferKind::Vertex),
    ] {
        let mut f = Fixture::new(1);
        f.load(&lightmap(1, true));
        let object = f.object;
        let owner = f.owner();
        let key = lod_key(owner, object, 0, 0);
        owner.backend.test_failure = Some(stage);
        assert!(
            owner.prepare_saved_lit_storage(object, 0).is_err(),
            "{stage:?}"
        );
        assert!(owner.incomplete);
        let storage = owner.objects.lit_lod_storage(key).unwrap();
        assert_eq!(storage.phase, LitStoragePhase::Interrupted);
        assert!(storage.index_owner.is_some());
        assert_eq!(owner.objects.lit_storage_counts().interrupted_lods, 1);
        let before = (
            owner.backend.live_allocations(),
            owner.backend.live_graphics(),
        );
        assert!(owner.prepare_saved_lit_storage(object, 0).is_err());
        assert_eq!(
            (
                owner.backend.live_allocations(),
                owner.backend.live_graphics()
            ),
            before
        );
        // This deliberately interrupted owner stays inspectable until fixture drop;
        // it is not described as successfully cleaned up or replayed.
        std::fs::remove_dir_all(f.path).unwrap();
    }
}
#[test]
fn derived_ib_free_failure_keeps_actual_moved_arrays_after_atlas_release() {
    let mut f = Fixture::new(1);
    f.load(&lightmap(1, true));
    f.prepare(0);
    let object = f.object;
    let owner = f.owner();
    let key = lod_key(owner, object, 0, 0);
    let crate::i3d_object::saved_lightmaps::SavedLitLod::Lightmap(payload) =
        owner.saved_lit_lod(object, 0).unwrap().unwrap()
    else {
        panic!()
    };
    let atlas = Arc::downgrade(&payload.atlas);
    let blocks = owner.backend.blocks.len();
    owner.backend.test_failure = Some(CpuStorageTestFailure::BlockFreeAfterFree(BufferKind::Index));
    let failure = owner
        .load_saved_lightmaps_native(object, &[0], SavedLightmapLoadContext::TrueColorRgba8)
        .unwrap_err();
    assert_eq!(failure.consumed, 0);
    assert_eq!(owner.backend.blocks.len() + 1, blocks);
    assert_eq!(atlas.strong_count(), 0);
    let storage = owner.objects.lit_lod_storage(key).unwrap();
    assert_eq!(storage.phase, LitStoragePhase::Interrupted);
    assert!(storage.retiring_arrays.is_some());
    assert!(owner
        .objects
        .lod(key)
        .unwrap()
        .vertex_buffer
        .graphics
        .is_some());
    assert_eq!(owner.objects.lit_storage_counts().interrupted_lods, 1);
    std::fs::remove_dir_all(f.path).unwrap();
}
#[test]
fn active_switch_failure_keeps_old_level_and_actual_ib_and_reports_interrupted_storage() {
    let mut f = Fixture::new(1);
    f.load(&lightmap(1, false));
    f.prepare(0);
    let object = f.object;
    let owner = f.owner();
    let key = lod_key(owner, object, 0, 0);
    owner.backend.test_failure = Some(CpuStorageTestFailure::BlockFreeAfterFree(
        BufferKind::Vertex,
    ));
    assert!(owner.set_active_lighting(object, 1).is_err());
    assert_eq!(
        owner
            .objects
            .object(object)
            .unwrap()
            .lit_object()
            .unwrap()
            .active_level,
        0
    );
    assert_eq!(
        owner.objects.lit_lod_storage(key).unwrap().phase,
        LitStoragePhase::Interrupted
    );
    assert_eq!(owner.objects.lit_storage_counts().live_index_owners, 1);
    assert!(owner.saved_lit_lod(object, 0).unwrap().is_some());
    assert!(owner.objects.is_poisoned());
    std::fs::remove_dir_all(f.path).unwrap();
}
#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn actual_managed_saved_buffers_feed_gpu_vertex_and_dual_uv_lightmap_draws() {
    let instance = lhr_render::instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
    ] {
        let mut f = Fixture::new(1);
        let object = f.object;
        for (data, expected, expanded) in [
            (colors(5, 1, &[&[0x0040c080; 3]]), [64u8, 96, 32, 17], false),
            (lightmap(1, true), [32, 32, 32, 17], true),
        ] {
            f.load(&data);
            let model = f.model;
            let snapshot =
                crate::car_inspection::snapshot_arena_tree(&mut f.arena, model.root()).unwrap();
            assert_eq!(
                snapshot.scene.meshes[0].vertices.len(),
                if expanded { 4 } else { 3 }
            );
            assert_eq!(f.owner().objects.lit_storage_counts().ready_lods, 1);
            let mut renderer = pollster::block_on(lhr_render::Renderer::with_adapter(
                &adapter,
                &snapshot.scene,
                format,
                false,
            ))
            .unwrap();
            for (index, lighting) in snapshot.lighting.iter().enumerate() {
                renderer
                    .set_mesh_lighting(index, lighting.as_ref().unwrap())
                    .unwrap();
            }
            let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
            let pixel = &rgba[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4];
            for (&got, wanted) in pixel.iter().zip(expected) {
                assert!(
                    (i16::from(got) - i16::from(wanted)).abs() <= 1,
                    "{format:?} expanded={expanded}, pixel={pixel:?}"
                );
            }
            if !expanded {
                let owner = f.owner();
                let key = lod_key(owner, object, 0, 0);
                let descriptor = owner.objects.lod(key).unwrap().vertex_buffer;
                let graphics = owner
                    .backend
                    .get_graphics_mut(GraphicsBuffer::Vertex(descriptor.graphics.unwrap()))
                    .unwrap();
                for i in 0..3 {
                    let at = (descriptor.base as usize + i) * 24 + 12;
                    graphics.bytes[at..at + 4].copy_from_slice(&0x00ff2060u32.to_le_bytes());
                }
                let snapshot =
                    crate::car_inspection::snapshot_arena_tree(&mut f.arena, model.root()).unwrap();
                renderer
                    .set_mesh_lighting(0, snapshot.lighting[0].as_ref().unwrap())
                    .unwrap();
                let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
                let pixel = &rgba[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4];
                for (&got, wanted) in pixel.iter().zip([255u8, 16, 24, 17]) {
                    assert!(
                        (i16::from(got) - i16::from(wanted)).abs() <= 1,
                        "live managed color {format:?}: {pixel:?}"
                    );
                }
            }
        }
        f.close();
    }
}
