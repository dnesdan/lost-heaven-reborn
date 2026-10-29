//! Authored 4DS fixtures through the real Installation and world-owner path.
use super::*;
use crate::billboard_state::BillboardOrientation;
use crate::i3d_object::ObjectVisualType;

struct Scratch(std::path::PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "lhr-world-billboards-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&root).unwrap();
        Self(root)
    }
    fn install(&self, bytes: &[u8]) -> lhr_assets::Installation {
        std::fs::write(self.0.join("world.4ds"), bytes).unwrap();
        lhr_assets::Installation::open(&self.0).unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn floats(bytes: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}
fn string(bytes: &mut Vec<u8>, value: &[u8]) {
    bytes.push(u8::try_from(value.len()).unwrap());
    bytes.extend_from_slice(value);
}
fn common(bytes: &mut Vec<u8>, name: &[u8], parent: u16) {
    bytes.extend_from_slice(&parent.to_le_bytes());
    floats(bytes, &[0.0; 3]);
    floats(bytes, &[1.0; 3]);
    floats(bytes, &[1.0, 0.0, 0.0, 0.0]);
    bytes.push(0); // culling
    string(bytes, name);
    bytes.push(0); // no properties
}
fn dummy() -> Vec<u8> {
    let mut bytes = vec![6];
    common(&mut bytes, b"root", 0);
    floats(&mut bytes, &[-2.0, -2.0, -2.0, 2.0, 2.0, 2.0]);
    bytes
}
fn visual(name: &[u8], parent: u16, instance: u16, suffix: Option<(u32, u8)>) -> Vec<u8> {
    let mut bytes = vec![1, if suffix.is_some() { 4 } else { 0 }, 0, 0];
    common(&mut bytes, name, parent);
    bytes.extend_from_slice(&instance.to_le_bytes());
    if instance == 0 {
        bytes.push(1); // one LOD
        floats(&mut bytes, &[16.0]);
        bytes.extend_from_slice(&4u16.to_le_bytes());
        for (position, uv) in [
            ([-2.0, -1.0, 0.0], [0.0, 0.0]),
            ([2.0, -1.0, 0.0], [1.0, 0.0]),
            ([2.0, 1.0, 0.0], [1.0, 1.0]),
            ([-2.0, 1.0, 0.0], [0.0, 1.0]),
        ] {
            floats(&mut bytes, &position);
            floats(&mut bytes, &[0.0, 0.0, 1.0]);
            floats(&mut bytes, &uv);
        }
        bytes.push(1); // one face group
        for index in [2u16, 0, 1, 2, 0, 2, 3, 1] {
            // face count, six indices, one-based material
            bytes.extend_from_slice(&index.to_le_bytes());
        }
    }
    if let Some((axis, mode)) = suffix {
        bytes.extend_from_slice(&axis.to_le_bytes());
        bytes.push(mode);
    }
    bytes
}
fn world(frames: &[Vec<u8>], texture: &[u8]) -> Vec<u8> {
    let mut bytes = b"4DS\0".to_vec();
    bytes.extend_from_slice(&29u16.to_le_bytes());
    bytes.extend_from_slice(&[0; 8]);
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes()); // ordinary opaque material
    floats(&mut bytes, &[1.0; 6]); // ambient and diffuse
    floats(&mut bytes, &[0.0; 3]); // emission
    floats(&mut bytes, &[1.0]); // opacity
    string(&mut bytes, texture);
    bytes.extend_from_slice(&u16::try_from(frames.len()).unwrap().to_le_bytes());
    for frame in frames {
        bytes.extend_from_slice(frame);
    }
    bytes.push(0); // original LoadAnimCount remains zero
    bytes
}
fn close_world(arena: &mut ModelArena, scene: SceneKey) {
    let frames = arena.scene(scene).unwrap().frames().to_vec();
    for frame in frames.into_iter().rev() {
        arena.release(frame).unwrap();
    }
    assert!(arena.scene(scene).unwrap().frames().is_empty());
    arena.reclaim_unused_visual_buffers().unwrap();
    for sector in arena.scene(scene).unwrap().sectors {
        arena.release(sector.frame()).unwrap();
    }
    arena.destroy_empty_scene_base(scene.root()).unwrap();
    assert_eq!(arena.live_frame_count(), 0);
    if let Some(resources) = arena.loaded_visual_resources() {
        assert_eq!(resources.backend().live_textures(), 0);
        assert_eq!(resources.backend().live_graphics(), 0);
    }
}
fn assert_empty_failure(arena: &ModelArena, scene: SceneKey, failure: &CpuSceneWorldFailure) {
    assert_eq!(failure.progress.phase(), CpuSceneWorldPhase::Input);
    assert!(failure.progress.staging_model().is_none());
    assert!(failure.progress.frames().is_empty());
    assert!(arena.scene(scene).unwrap().frames().is_empty());
    assert!(!arena.scenes[arena.scene_index(scene).unwrap()].cpu_world_intake_active);
}

#[test]
fn installed_world_keeps_billboard_class_suffix_bounds_and_scene_ownership() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let scratch = Scratch::new();
        let bytes = world(&[visual(b"sprite", 0, 0, Some((0, 1)))], b"");
        // The strict scene parser must continue to reject a visual-4 source.
        assert!(
            lhr_assets::parse_four_ds_scene(&bytes)
                .unwrap_err()
                .is_unsupported_billboard()
        );
        let installation = scratch.install(&bytes);
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let progress = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap();
        assert_eq!(progress.phase(), CpuSceneWorldPhase::Complete);
        assert!(progress.staging_model().is_none());
        assert_eq!(progress.transferred_frames(), 1);
        assert_eq!(arena.scene(scene).unwrap().frames(), progress.frames());
        let key = progress.frames()[0];
        let frame = arena.frame(key).unwrap();
        assert_eq!(frame.owner, Some(scene.into()));
        assert_eq!(frame.name_bytes, b"sprite");
        assert_eq!(frame.source_index, Some(0));
        assert_eq!(
            frame.billboard.unwrap().orientation,
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 1,
            }
        );
        assert_eq!(frame.billboard.unwrap().source.unwrap().frame_index, 0);
        assert_eq!(frame.visual_bounds.unwrap().bbox, [[-2.0; 3], [2.0; 3]]);
        let object = frame.object_resources.unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0].frame();
        assert_eq!(arena.links(key).unwrap().parent, Some(primary));
        // Resource publication after the staging model dies must retain BBRD.
        arena.publish_runtime_buffers().unwrap();
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .inspection_mesh(object, 8)
                .unwrap()
                .unwrap()
                .visual_type,
            ObjectVisualType::Billboard
        );
        close_world(&mut arena, scene);
    });
}

#[test]
fn mixed_world_preserves_source_indices_parents_and_per_instance_orientation() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let scratch = Scratch::new();
        let installation = scratch.install(&world(
            &[
                dummy(),
                visual(b"solid", 1, 0, None),
                visual(b"sprite", 1, 2, Some((1, 1))),
                visual(b"sprite2", 0, 2, Some((2, 0))),
            ],
            b"",
        ));
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let progress = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap();
        assert_eq!(progress.transferred_frames(), 4);
        let keys = progress.frames();
        let primary = arena.scene(scene).unwrap().sectors[0].frame();
        for (index, &key) in keys.iter().enumerate() {
            assert_eq!(arena.frame(key).unwrap().source_index, Some(index));
            assert_eq!(arena.frame(key).unwrap().owner, Some(scene.into()));
            let parent = if index == 1 || index == 2 {
                keys[0]
            } else {
                primary
            };
            assert_eq!(arena.links(key).unwrap().parent, Some(parent));
        }
        assert!(arena.frame(keys[0]).unwrap().billboard.is_none());
        assert!(arena.frame(keys[1]).unwrap().billboard.is_none());
        assert_eq!(
            arena
                .frame(keys[2])
                .unwrap()
                .billboard
                .unwrap()
                .source
                .unwrap()
                .frame_index,
            2
        );
        assert_eq!(
            arena.frame(keys[2]).unwrap().billboard.unwrap().orientation,
            BillboardOrientation {
                rot_axis: 1,
                axis_mode: 1,
            }
        );
        assert_eq!(
            arena
                .frame(keys[3])
                .unwrap()
                .billboard
                .unwrap()
                .source
                .unwrap()
                .frame_index,
            3
        );
        assert_eq!(
            arena.frame(keys[3]).unwrap().billboard.unwrap().orientation,
            BillboardOrientation {
                rot_axis: 2,
                axis_mode: 0,
            }
        );
        let first = arena.frame(keys[1]).unwrap().object_resources.unwrap();
        let a = arena.frame(keys[2]).unwrap().object_resources.unwrap();
        let b = arena.frame(keys[3]).unwrap().object_resources.unwrap();
        let resources = arena.loaded_visual_resources().unwrap();
        assert_eq!(
            resources.object_mesh(first).unwrap(),
            resources.object_mesh(a).unwrap()
        );
        assert_eq!(
            resources.object_mesh(a).unwrap(),
            resources.object_mesh(b).unwrap()
        );
        close_world(&mut arena, scene);
    });
}

#[test]
fn plain_world_keeps_ordinary_class_and_non_cubic_bounds() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let scratch = Scratch::new();
        let installation = scratch.install(&world(&[visual(b"solid", 0, 0, None)], b""));
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let progress = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap();
        let frame = arena.frame(progress.frames()[0]).unwrap();
        assert!(frame.billboard.is_none());
        assert_eq!(
            frame.visual_bounds.unwrap().bbox,
            [[-2.0, -1.0, 0.0], [2.0, 1.0, 0.0]]
        );
        let object = frame.object_resources.unwrap();
        arena.publish_runtime_buffers().unwrap();
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .inspection_mesh(object, 8)
                .unwrap()
                .unwrap()
                .visual_type,
            ObjectVisualType::Plain
        );
        close_world(&mut arena, scene);
    });
}

#[test]
fn other_frame_classes_still_stop_before_world_owner_mutation() {
    let scratch = Scratch::new();
    let installation = scratch.install(&world(&[], b""));
    for (kind, visual) in [
        (1, Some(1)),
        (1, Some(2)),
        (1, Some(3)),
        (1, Some(5)),
        (1, Some(6)),
        (2, None),
        (7, None),
        (10, None),
        (12, None),
    ] {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let mut frame = super::super::tests::frame(kind, None, [0.0; 3]);
        frame.visual = visual;
        let source = Arc::new(Model {
            materials: Vec::new(),
            frames: vec![frame],
        });
        let mut progress = CpuSceneWorldProgress::empty(&arena, scene);
        assert_eq!(
            arena.load_scene_world_parsed_cpu(scene, &installation, source, &[], &mut progress),
            Err(ModelError::UnsupportedFrame {
                index: 0,
                kind,
                visual
            })
        );
        assert!(progress.staging_model().is_none());
        assert!(arena.scene(scene).unwrap().frames().is_empty());
        close_world(&mut arena, scene);
    }
}

#[test]
fn truncated_billboard_suffix_never_creates_scene_frames() {
    let scratch = Scratch::new();
    let bytes = world(&[visual(b"sprite", 0, 0, Some((1, 1)))], b"");
    for keep in bytes.len() - 6..bytes.len() {
        let installation = scratch.install(&bytes[..keep]);
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let failure = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap_err();
        assert_empty_failure(&arena, scene, &failure);
        close_world(&mut arena, scene);
    }
}

#[test]
fn nonzero_load_animation_count_remains_an_explicit_boundary() {
    let scratch = Scratch::new();
    let mut bytes = world(&[visual(b"sprite", 0, 0, Some((1, 1)))], b"");
    *bytes.last_mut().unwrap() = 1;
    let installation = scratch.install(&bytes);
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let failure = arena
        .load_scene_world_cpu(scene, &installation, "world.i3d")
        .unwrap_err();
    assert_empty_failure(&arena, scene, &failure);
    close_world(&mut arena, scene);
}

#[test]
fn invalid_mesh_instance_does_not_fall_back_to_an_ordinary_object() {
    let scratch = Scratch::new();
    let installation = scratch.install(&world(&[visual(b"bad", 0, 2, Some((1, 1)))], b""));
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let failure = arena
        .load_scene_world_cpu(scene, &installation, "world.i3d")
        .unwrap_err();
    assert_empty_failure(&arena, scene, &failure);
    close_world(&mut arena, scene);
}

#[test]
fn resource_failure_retains_staging_and_never_applies_the_suffix_early() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let scratch = Scratch::new();
        let installation = scratch.install(&world(
            &[visual(b"sprite", 0, 0, Some((0, 1)))],
            b"absent.tga",
        ));
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let mut failure = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap_err();
        assert_eq!(failure.progress.phase(), CpuSceneWorldPhase::Resources);
        assert_eq!(failure.progress.transferred_frames(), 0);
        assert_eq!(failure.progress.frames().len(), 1);
        assert!(arena.scene(scene).unwrap().frames().is_empty());
        let staging = failure.progress.staging_model().unwrap();
        let key = failure.progress.frames()[0];
        let frame = arena.frame(key).unwrap();
        assert_eq!(frame.owner, Some(staging.into()));
        assert_eq!(
            frame.billboard.unwrap().orientation,
            BillboardOrientation::default()
        );
        assert_eq!(frame.billboard.unwrap().source.unwrap().rot_axis_word, 0);
        assert!(
            arena
                .load_scene_world_cpu(scene, &installation, "world.i3d")
                .is_err()
        );
        arena
            .discard_scene_world_staging_cpu(&mut failure.progress)
            .unwrap();
        assert!(failure.progress.staging_model().is_none());
        assert!(arena.frame(key).is_err());
        close_world(&mut arena, scene);
    });
}

#[test]
fn repeat_world_intake_is_rejected_without_replacing_existing_billboard_owners() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let scratch = Scratch::new();
        let installation = scratch.install(&world(&[visual(b"sprite", 0, 0, Some((1, 1)))], b""));
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let progress = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap();
        let key = progress.frames()[0];
        let object = arena.frame(key).unwrap().object_resources;
        let suffix = arena.frame(key).unwrap().billboard;
        let count = arena.live_frame_count();
        let failure = arena
            .load_scene_world_cpu(scene, &installation, "world.i3d")
            .unwrap_err();
        assert_eq!(failure.progress.phase(), CpuSceneWorldPhase::Admission);
        assert!(failure.progress.frames().is_empty());
        assert_eq!(arena.live_frame_count(), count);
        assert_eq!(arena.frame(key).unwrap().object_resources, object);
        assert_eq!(arena.frame(key).unwrap().billboard, suffix);
        close_world(&mut arena, scene);
    });
}

#[test]
fn world_suffix_uses_existing_native_orientation_corrections() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        for (axis, mode, expected_axis, expected_mode) in [(3, 0, 0, 2), (2, 9, 2, 1)] {
            let scratch = Scratch::new();
            let installation =
                scratch.install(&world(&[visual(b"sprite", 0, 0, Some((axis, mode)))], b""));
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let progress = arena
                .load_scene_world_cpu(scene, &installation, "world.i3d")
                .unwrap();
            let state = arena
                .frame(progress.frames()[0])
                .unwrap()
                .billboard
                .unwrap();
            assert_eq!(state.source.unwrap().rot_axis_word, axis);
            assert_eq!(state.source.unwrap().axis_mode_byte, mode);
            assert_eq!(
                state.orientation,
                BillboardOrientation {
                    rot_axis: expected_axis,
                    axis_mode: expected_mode,
                }
            );
            close_world(&mut arena, scene);
        }
    });
}
