fn read_subtype_fixture(
    loader: &mut MissionSceneLoader,
    arena: &mut ModelArena,
    requested: u32,
) -> Result<(), (u32, String)> {
    let data = chunk(0x4c53, &chunk(0x4090, &requested.to_le_bytes()));
    let mut input = MissionSceneInput::new(&data).unwrap();
    input.next_chunk().unwrap();
    let result = loader.read_visual_subtype(arena, &mut input);
    assert_eq!(input.cursor(), data.len());
    assert!(input.current().is_none());
    result
}

#[test]
fn scene2_replacement_moves_children_republishes_map_and_transfers_real_membership() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        for model_owned in [false, true] {
            let mut arena = ModelArena::default();
            let scene = arena.construct_scene_owner_prefix().unwrap();
            let old = crate::visual_replacement::tests::triangle(&mut arena, scene, model_owned);
            let owner = arena.frame(old).unwrap().owner;
            let parent = arena.links(old).unwrap().parent;
            let old_reference = arena.lookup_reference(old);
            let mut children = Vec::new();
            for _ in 0..3 {
                let child = arena
                    .insert(
                        crate::frame_transform::I3dFrameState::dummy(),
                        None,
                        Vec::new(),
                        Vec::new(),
                        None,
                    )
                    .unwrap();
                arena.link_frame(child, Some(old), 0).unwrap();
                children.push(child);
            }
            let data = crate::actor_frames::FrameUserData::ExternalRecord(991);
            arena.set_user_data(old, data).unwrap();
            if model_owned {
                arena.retain(old).unwrap();
            }
            let mut loader =
                MissionSceneLoader::new(&arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
            loader.current_frame = Some(old_reference);
            read_subtype_fixture(
                &mut loader,
                &mut arena,
                crate::visual_replacement::LIT_OBJECT_VISUAL,
            )
            .unwrap();
            let new = FrameKey(loader.current_frame.unwrap().identity());
            assert_ne!(new, old);
            assert_eq!(arena.frame(new).unwrap().owner, owner);
            assert_eq!(arena.frame(new).unwrap().references, 1);
            assert_eq!(arena.links(new).unwrap().parent, parent);
            assert_eq!(
                arena.owned_object_visual_type(new).unwrap(),
                crate::visual_replacement::LIT_OBJECT_VISUAL
            );
            assert_eq!(
                arena.user_data(new).unwrap(),
                crate::actor_frames::FrameUserData::Null
            );
            assert_eq!(
                loader.load_map.map.slot(b"fixture"),
                Some(loader.current_frame)
            );
            assert_ne!(loader.object_fields.flags & 0x400, 0);
            assert!(loader.pending_visual_replacement.is_none());
            // Repeated child-zero removal and prepend reverses the old chain.
            let mut next = arena.links(new).unwrap().child;
            for child in &children {
                assert_eq!(next, Some(*child));
                assert_eq!(arena.frame(*child).unwrap().references, 1);
                next = arena.links(*child).unwrap().next;
            }
            assert_eq!(next, None);
            if model_owned {
                assert_eq!(arena.frame(old).unwrap().owner, None);
                assert_eq!(arena.links(old).unwrap().parent, None);
                assert_eq!(arena.links(old).unwrap().child, None);
                assert_eq!(arena.user_data(old).unwrap(), data);
                assert_eq!(arena.frame(old).unwrap().references, 1);
                arena.release(old).unwrap();
            } else {
                assert!(arena.frame(old).is_err());
            }
        }
    });
}

#[test]
fn replacement_membership_failure_keeps_executed_graph_and_map_prefix() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let old = crate::visual_replacement::tests::triangle(&mut arena, scene, false);
        let parent_model = arena.create_model().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0].frame();
        arena
            .link_frame(parent_model.root(), Some(primary), 0)
            .unwrap();
        arena.link_frame(old, Some(parent_model.root()), 0).unwrap();
        let reference = arena.lookup_reference(old);
        let mut loader =
            MissionSceneLoader::new(&arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
        loader.current_frame = Some(reference);
        let error = read_subtype_fixture(
            &mut loader,
            &mut arena,
            crate::visual_replacement::LIT_OBJECT_VISUAL,
        )
        .unwrap_err();
        assert_eq!(error.0, 0x543b16);
        let pending = loader.pending_visual_replacement.unwrap();
        assert_eq!(pending.entry, error.0);
        assert_eq!(pending.old, old);
        assert_eq!(arena.links(old).unwrap().parent, None);
        assert_eq!(arena.frame(old).unwrap().owner, Some(scene.into()));
        assert_eq!(
            arena.links(pending.new).unwrap().parent,
            Some(parent_model.root())
        );
        assert_eq!(arena.frame(pending.new).unwrap().references, 1);
        assert_eq!(arena.frame(pending.new).unwrap().owner, None);
        assert_eq!(
            loader.load_map.map.slot(b"fixture"),
            Some(Some(arena.lookup_reference(pending.new)))
        );
        assert_eq!(loader.current_frame, Some(reference));
        assert_eq!(loader.object_fields.flags & 0x400, 0);
        assert!(arena.frame_object_resources(pending.new).unwrap().is_some());
        loader
            .release_pending_visual_replacement(&mut arena)
            .unwrap();
        assert!(loader.pending_visual_replacement.is_none());
        assert!(arena.frame(pending.new).is_err());
        assert_eq!(arena.frame(old).unwrap().owner, Some(scene.into()));
        // Repeated shutdown does not release another owner's reference.
        loader
            .release_pending_visual_replacement(&mut arena)
            .unwrap();
    });
}

#[test]
fn native_invalid_visual_factory_tag_preserves_current_frame_and_map() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let old = crate::visual_replacement::tests::triangle(&mut arena, scene, false);
        let reference = arena.lookup_reference(old);
        let mut loader =
            MissionSceneLoader::new(&arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
        loader.current_frame = Some(reference);
        let live = arena.live_frame_count();
        read_subtype_fixture(&mut loader, &mut arena, 0x12345678).unwrap();
        assert_eq!(loader.current_frame, Some(reference));
        assert_eq!(loader.load_map.map.slot(b"fixture"), Some(Some(reference)));
        assert_eq!(arena.live_frame_count(), live);
        assert!(loader.pending_visual_replacement.is_none());
        assert_eq!(loader.object_fields.flags & 0x400, 0);
    });
}

fn saved_vertex_colors(version: u8, level: u8) -> Vec<u8> {
    let mut data = vec![1, version, 1]; // one lighting, format, vertex mode
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&5f32.to_le_bytes());
    data.extend_from_slice(&45f32.to_le_bytes());
    data.push(level);
    data.extend_from_slice(&0u16.to_le_bytes());
    for color in [0xff102030u32, 0xff405060, 0xff708090] {
        data.extend_from_slice(&color.to_le_bytes());
    }
    data
}

#[test]
fn saved_lighting_field_uses_replacement_owner_and_ignores_native_negative_status() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let old = crate::visual_replacement::tests::triangle(&mut arena, scene, false);
        let mut loader = MissionSceneLoader::new(&arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
        loader.current_frame = Some(arena.lookup_reference(old));
        loader.version[0] = Some(3);
        read_subtype_fixture(&mut loader, &mut arena, crate::visual_replacement::LIT_OBJECT_VISUAL).unwrap();
        let key = FrameKey(loader.current_frame.unwrap().identity());
        let object = arena.frame_object_resources(key).unwrap().unwrap();
        for version in [0x21, 0x20] {
            loader.object_fields.flags &= !0x400;
            let payload = saved_vertex_colors(version, 1);
            let bytes = chunk(0x4c53, &chunk(0x40a0, &payload));
            let mut input = MissionSceneInput::new(&bytes).unwrap();
            input.next_chunk().unwrap();
            loader.read_saved_lightmaps(&mut arena, &mut input).unwrap();
            assert_eq!(input.cursor(), bytes.len());
            assert!(input.current().is_none());
            assert_ne!(loader.object_fields.flags & 0x400, 0);
            assert_eq!(loader.object_fields.saved_lightmap_result, Some(if version == 0x21 { 0 } else { 0x80000007 }));
            let saved = arena.loaded_visual_resources().unwrap().saved_lit_lod(object, 0).unwrap();
            if version == 0x21 {
                assert!(matches!(saved, Some(crate::i3d_object::saved_lightmaps::SavedLitLod::VertexColors { colors, .. }) if colors.len() == 3));
            } else {
                // The second Load first destructs the active real lighting,
                // then returns its negative version status to the Game caller.
                assert!(saved.is_none());
            }
        }
    });
}

#[test]
fn saved_lighting_host_boundary_keeps_consumed_cursor_and_resource_prefix() {
    crate::native_precision::Pc24PrecisionState::default().run(|| {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let old = crate::visual_replacement::tests::triangle(&mut arena, scene, false);
        let mut loader =
            MissionSceneLoader::new(&arena, scene).unwrap_or_else(|e| panic!("{}", e.detail));
        loader.current_frame = Some(arena.lookup_reference(old));
        loader.version[0] = Some(3);
        read_subtype_fixture(
            &mut loader,
            &mut arena,
            crate::visual_replacement::LIT_OBJECT_VISUAL,
        )
        .unwrap();
        loader.object_fields.flags &= !0x400;
        let bytes = chunk(0x4c53, &chunk(0x40a0, &saved_vertex_colors(0x21, 0)));
        let mut input = MissionSceneInput::new(&bytes).unwrap();
        input.next_chunk().unwrap();
        let error = loader
            .read_saved_lightmaps(&mut arena, &mut input)
            .unwrap_err();
        assert_eq!(error.0, 0x543b8d);
        assert_eq!(loader.object_fields.saved_lightmap_consumed, Some(16));
        assert_eq!(input.cursor(), 12 + 16);
        assert!(input.current().is_some());
        assert_eq!(loader.object_fields.saved_lightmap_result, None);
        assert_eq!(loader.object_fields.flags & 0x400, 0);
        let key = FrameKey(loader.current_frame.unwrap().identity());
        assert_eq!(arena.frame(key).unwrap().owner, Some(scene.into()));
        assert!(arena.frame_object_resources(key).unwrap().is_some());
    });
}
