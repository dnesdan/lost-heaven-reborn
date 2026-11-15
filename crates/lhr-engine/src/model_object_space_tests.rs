use super::*;
use crate::model_loading::tests::frame;
use std::sync::Arc;

fn bits(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}

/// A row-major S_matrix: rows 0..2 the rotated axes, row 3 the translation.
fn matrix(rows: [[f32; 4]; 4]) -> SMatrix {
    let mut elements = [0.0; 16];
    for (r, row) in rows.iter().enumerate() {
        elements[r * 4..r * 4 + 4].copy_from_slice(row);
    }
    SMatrix { elements }
}

#[test]
fn position_transform_follows_the_mutating_operator_order() {
    // Translation added last, after the z, y, x (x axis) or z, x, y (y and
    // z axes) products, as 0x2e920 orders them.
    let m = matrix([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 2.0, 0.0],
        [10.0, 20.0, 30.0, 1.0],
    ]);
    assert_eq!(
        transform_position([1.0, 2.0, 3.0], &m.elements),
        Ok([8.0, 21.0, 36.0])
    );
    // Association is observable: 1e8 + 1 - 1e8 rounds differently by order.
    let m = matrix([
        [1.0, 0.0, 0.0, 0.0],
        [1.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    // x' = ((-1*z + 1*y) + 1*x) with z=1e8, y=1e8, x=1 → (0 + 1) = 1.
    assert_eq!(
        transform_position([1.0, 1.0e8, 1.0e8], &m.elements).unwrap()[0],
        1.0
    );
}

#[test]
fn normal_rotation_has_no_translation() {
    let m = matrix([
        [0.0, 1.0, 0.0, 0.0],
        [-1.0, 0.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [10.0, 20.0, 30.0, 1.0],
    ]);
    assert_eq!(
        rotate_normal([1.0, 0.0, 0.0], &m.elements),
        Ok([0.0, 1.0, 0.0])
    );
}

#[test]
fn inlined_normalize_keeps_near_unit_and_scales_otherwise() {
    let near = [0.6, 0.8, 0.0];
    assert_eq!(bits(normalize_normal(near).unwrap()), bits(near));
    let scaled = normalize_normal([0.0, 3.0, 4.0]).unwrap();
    let r = 1.0f32 / 25.0f32.sqrt();
    assert_eq!(bits(scaled), bits([0.0, 3.0 * r, 4.0 * r]));
    // A non-unit length that the epsilon does not absorb.
    let long = normalize_normal([2.0, 0.0, 0.0]).unwrap();
    assert_eq!(long, [1.0, 0.0, 0.0]);
}

#[test]
fn inlined_normalize_degenerate_picks_x_then_z_then_y() {
    // len2 below 1e-8: only the first non-zero of x, z, y becomes its sign.
    assert_eq!(normalize_normal([-1e-5, 2e-5, 0.0]), Ok([-1.0, 2e-5, 0.0]));
    assert_eq!(normalize_normal([0.0, 2e-5, 3e-5]), Ok([0.0, 2e-5, 1.0]));
    assert_eq!(normalize_normal([0.0, -2e-5, 0.0]), Ok([0.0, -1.0, 0.0]));
    // All zero: y becomes +1; -0.0 compares equal to zero.
    assert_eq!(normalize_normal([-0.0, 0.0, 0.0]), Ok([-0.0, 1.0, 0.0]));
}

#[test]
fn tiny_components_stay_normal_in_the_register() {
    // A MISE01 normal: z*z (1.3e-39) is subnormal as binary32 but a normal
    // x87 register value; the PC24 sum is exactly 1, so the normal is kept.
    let n = [
        f32::from_bits(0xa82a_aaab),
        1.0,
        f32::from_bits(0x1f2a_aaab),
    ];
    assert_eq!(bits(normalize_normal(n).unwrap()), bits(n));
    // A sub-epsilon length whose squares are all tiny takes the sign path.
    assert_eq!(normalize_normal([0.0, 0.0, 1e-30]), Ok([0.0, 0.0, 1.0]));
}

#[test]
fn x87_rounding_is_twenty_four_bits_with_a_wide_exponent() {
    // 1 + 2^-24 is a tie at 24 bits and rounds to even (1.0).
    assert_eq!(pc24(1.0 + 2f64.powi(-24)), Ok(1.0));
    assert_eq!(
        pc24(1.0 + 3.0 * 2f64.powi(-24)),
        Ok(1.0 + 2.0 * 2f64.powi(-23))
    );
    // Below binary32's normal range the register keeps the value.
    let tiny = 2f64.powi(-140);
    assert_eq!(pc24(tiny * (1.0 + 2f64.powi(-30))), Ok(tiny));
    assert_eq!(
        pc24(tiny * (1.0 + 2f64.powi(-22))),
        Ok(tiny * (1.0 + 2f64.powi(-22)))
    );
    assert_eq!(store(1e-45), Ok(f32::from_bits(1)));
    assert_eq!(store(1e39), Err(()));
}

/// The stores one vertex's bake issues, in order, and how it ended.
type BakeStores = (Vec<(usize, f32)>, Result<(), VertexBakeStop<()>>);
fn bake_stores(position: [f32; 3], normal: [f32; 3], m: &SMatrix) -> BakeStores {
    let mut stores = vec![];
    let result = bake_world_vertex(position, normal, m, |word, value| {
        stores.push((word, value));
        Ok(())
    });
    (stores, result)
}

#[test]
fn bake_stops_outside_the_guarded_domain_after_the_earlier_stores() {
    let m = matrix([
        [f32::MAX, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    // 0x2e920 stores z first; the x store overflows binary32 and stops.
    assert_eq!(
        bake_stores([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], &m),
        (vec![(2, 0.0)], Err(VertexBakeStop::Numeric))
    );
}

#[test]
fn a_stop_inside_the_normal_keeps_the_position_and_rotated_normal_stores() {
    // pos *= m stores z, x, y; the rotated normal is copied x, y, z; the
    // Normalize length (3 * MAX^2) overflows its f32 store and stops there.
    let (stores, result) = bake_stores([1.0, 2.0, 3.0], [f32::MAX; 3], &SMatrix::identity());
    assert_eq!(result, Err(VertexBakeStop::Numeric));
    assert_eq!(
        stores,
        [
            (2, 3.0),
            (0, 1.0),
            (1, 2.0),
            (3, f32::MAX),
            (4, f32::MAX),
            (5, f32::MAX)
        ]
    );
    // The scaling branch stores x, y, z after the full position and copy.
    let (stores, result) = bake_stores([0.0; 3], [0.0, 3.0, 4.0], &SMatrix::identity());
    assert_eq!(result, Ok(()));
    let r = 1.0f32 / 25.0f32.sqrt();
    assert_eq!(
        stores[3..],
        [
            (3, 0.0),
            (4, 3.0),
            (5, 4.0),
            (3, 0.0),
            (4, 3.0 * r),
            (5, 4.0 * r)
        ]
    );
}

#[test]
fn the_bake_raises_sticky_precision_in_the_active_scope() {
    use crate::native_precision::Pc24PrecisionState;
    // 0.6f * 0.6f is inexact at 24 bits: the native fmul raises PE even
    // though the kept normal stores nothing.
    let mut precision = Pc24PrecisionState::new(false);
    let (stores, result) =
        precision.run(|| bake_stores([0.0; 3], [0.6, 0.8, 0.0], &SMatrix::identity()));
    assert_eq!(result, Ok(()));
    assert_eq!(stores.len(), 6);
    assert!(precision.precision_raised());
    // Exact operations and stores leave it clear.
    let mut precision = Pc24PrecisionState::new(false);
    let (_, result) =
        precision.run(|| bake_stores([1.0, 2.0, 3.0], [0.0, 0.0, 1.0], &SMatrix::identity()));
    assert_eq!(result, Ok(()));
    assert!(!precision.precision_raised());
    // A store that rounds raises it too.
    let mut precision = Pc24PrecisionState::new(false);
    precision.run(|| store(0.1).unwrap());
    assert!(precision.precision_raised());
}

struct Fixture {
    dir: std::path::PathBuf,
    arena: ModelArena,
    model: ModelKey,
    visual: FrameKey,
    dummy: FrameKey,
}

fn fixture(name: &str) -> Fixture {
    fixture_with(name, [[0., 0., 2.]; 3])
}

fn fixture_with(name: &str, normals: [[f32; 3]; 3]) -> Fixture {
    let dir = std::env::temp_dir().join(format!(
        "lhr-object-space-{name}-{}-{}",
        std::process::id(),
        crate::frame_lookup::incarnation()
    ));
    std::fs::create_dir(&dir).unwrap();
    let installation = lhr_assets::Installation::open(&dir).unwrap();
    let mut mesh = frame(1, None, [3., 4., 5.]);
    mesh.render_flags = 0;
    mesh.logic_flags = 0;
    mesh.lods.push(lhr_assets::Lod {
        distance: 0.,
        uv_only: None,
        vertices: [[-1., -2., -3.], [1., 2., 3.], [0., 0., 0.]]
            .into_iter()
            .zip(normals)
            .map(|(position, normal)| lhr_assets::Vertex {
                position,
                normal,
                uv: [0.25, 0.75],
            })
            .collect(),
        groups: vec![lhr_assets::FaceGroup {
            material: lhr_assets::UNASSIGNED_MATERIAL,
            indices: vec![0, 1, 2],
        }],
    });
    let dummy = frame(6, None, [0.; 3]);
    let mut arena = ModelArena::default();
    let model = arena.create_model().unwrap();
    let loaded = arena
        .load_parsed_into_model(
            model,
            Arc::new(lhr_assets::Model {
                materials: vec![],
                frames: vec![mesh, dummy],
            }),
        )
        .unwrap();
    let (visual, dummy) = (loaded.frames()[0], loaded.frames()[1]);
    arena
        .load_plain_visual_resources(model, &installation)
        .unwrap();
    Fixture {
        dir,
        arena,
        model,
        visual,
        dummy,
    }
}

impl Fixture {
    fn close(mut self) {
        self.arena.publish_runtime_buffers().unwrap();
        self.arena.close(self.model).unwrap();
        self.arena.reclaim_unused_visual_buffers().unwrap();
        let backend = self.arena.loaded_visual_resources().unwrap().backend();
        assert_eq!(backend.live_graphics(), 0);
        std::fs::remove_dir_all(self.dir).unwrap();
    }
}

#[test]
fn set_space_world_bakes_live_vertices_and_is_not_repeated() {
    let mut f = fixture("bake");
    let visual = f.visual;
    // LoadCollision's lock order is the reverse, but a view taken before the
    // bake must see it: it is the live storage, not a copy.
    let early = f.arena.lock_visual_lod0_vertices(visual).unwrap().unwrap();
    assert_eq!(early.position(1), Some([1., 2., 3.]));
    assert_eq!(
        f.arena.graph.visual_render_matrix(visual).unwrap().elements[12..15],
        [3., 4., 5.]
    );
    assert_eq!(
        f.arena.set_object_space_world(visual),
        Ok(ObjectSpaceWorld::Baked {
            detached: false,
            levels: 1,
            vertices: 3
        })
    );
    assert_eq!(early.position(0), Some([2., 2., 2.]));
    assert_eq!(early.position(1), Some([4., 6., 8.]));
    assert_eq!(early.position(2), Some([3., 4., 5.]));
    // UpdateWBoundProc ran first (0x40000 set, then cleared by the final
    // store) and its UpdateBoundProc set VISF_BOUND_VALID, which stays.
    let flags = f.arena.state(visual).unwrap().visual_flags;
    assert_eq!(flags & 0x60040, 0x20040);
    assert_eq!(
        f.arena.graph.visual_render_matrix(visual).unwrap(),
        SMatrix::identity()
    );
    let object = f.arena.frame_object_resources(visual).unwrap().unwrap();
    let resources = f.arena.loaded_visual_resources().unwrap();
    assert!(resources.objects().object(object).unwrap().world_space);
    // Normals were rotated (identity rotation) and normalized; UVs untouched.
    let mesh = resources.objects().object(object).unwrap().mesh.unwrap();
    let level = resources.objects().get_lod(mesh, 0).unwrap().unwrap();
    let window = resources.game_level_vertex_window(level).unwrap();
    assert_eq!(
        resources.game_vertex_position_normal(window, 1).unwrap(),
        [[4., 6., 8.], [0., 0., 1.]]
    );
    // UpdateBBox ran over the baked positions.
    assert_eq!(
        resources
            .objects()
            .managed()
            .resources()
            .mesh_level(level)
            .unwrap()
            .bbox,
        [[2., 2., 2.], [4., 6., 8.]]
    );
    // A second SetSpace(true) returns at the first test.
    assert_eq!(
        f.arena.set_object_space_world(visual),
        Ok(ObjectSpaceWorld::AlreadyWorld)
    );
    assert_eq!(early.position(1), Some([4., 6., 8.]));
    // The collision lock after the bake reads the same live storage.
    let late = f.arena.lock_visual_lod0_vertices(visual).unwrap().unwrap();
    assert!(late.same_buffer(&early));
    assert_eq!(late.len(), 3);
    f.close();
}

#[test]
fn set_space_and_lock_stop_on_visuals_without_an_object_owner() {
    let mut f = fixture("unsupported");
    let dummy = f.dummy;
    assert_eq!(
        f.arena.set_object_space_world(dummy),
        Err(ModelError::SetSpaceUnsupportedVisual(dummy))
    );
    assert_eq!(
        f.arena.lock_visual_lod0_vertices(dummy),
        Err(ModelError::SetSpaceUnsupportedVisual(dummy))
    );
    // No effect on the visual itself.
    assert_eq!(f.arena.state(f.visual).unwrap().visual_flags & 0x40, 0);
    f.close();
}

#[test]
fn a_meshless_object_only_takes_the_flags() {
    let mut f = fixture("meshless");
    let visual = f.visual;
    f.arena.set_frame_mesh(visual, None, true).unwrap();
    assert_eq!(
        f.arena.set_object_space_world(visual),
        Ok(ObjectSpaceWorld::Meshless)
    );
    assert_eq!(f.arena.state(visual).unwrap().visual_flags & 0x40040, 0x40);
    assert_eq!(f.arena.lock_visual_lod0_vertices(visual), Ok(None));
    f.close();
}

#[test]
fn a_shared_mesh_is_detached_before_the_bake_and_the_other_owner_keeps_it() {
    let mut f = fixture("detach");
    let visual = f.visual;
    let object = f.arena.frame_object_resources(visual).unwrap().unwrap();
    let owner = f.arena.loaded_visual_resources_mut().unwrap();
    let other = owner.load_plain_instance(object).unwrap();
    let shared = owner.objects().object(object).unwrap().mesh.unwrap();
    assert_eq!(
        owner
            .objects()
            .managed()
            .resources()
            .mesh_object(shared)
            .unwrap()
            .references,
        2
    );
    let shared_view = f.arena.lock_visual_lod0_vertices(visual).unwrap().unwrap();
    assert_eq!(
        f.arena.set_object_space_world(visual),
        Ok(ObjectSpaceWorld::Baked {
            detached: true,
            levels: 1,
            vertices: 3
        })
    );
    let owner = f.arena.loaded_visual_resources().unwrap();
    let own = owner.objects().object(object).unwrap().mesh.unwrap();
    assert_ne!(own, shared);
    assert_eq!(owner.objects().object(other).unwrap().mesh, Some(shared));
    assert_eq!(
        owner
            .objects()
            .managed()
            .resources()
            .mesh_object(shared)
            .unwrap()
            .references,
        1
    );
    // The other owner's (shared) storage is untouched; the private copy holds
    // the baked positions with the source's normals and UVs carried over.
    assert_eq!(shared_view.position(1), Some([1., 2., 3.]));
    let baked = f.arena.lock_visual_lod0_vertices(visual).unwrap().unwrap();
    assert!(!baked.same_buffer(&shared_view));
    assert_eq!(baked.position(1), Some([4., 6., 8.]));
    let owner = f.arena.loaded_visual_resources().unwrap();
    let level = owner.objects().get_lod(own, 0).unwrap().unwrap();
    let window = owner.game_level_vertex_window(level).unwrap();
    assert_eq!(
        owner.game_vertex_position_normal(window, 0).unwrap(),
        [[2., 2., 2.], [0., 0., 1.]]
    );
    f.arena.publish_runtime_buffers().unwrap();
    f.arena
        .loaded_visual_resources_mut()
        .unwrap()
        .destroy_object(other)
        .unwrap();
    f.close();
}

#[test]
fn update_wbound_proc_sets_bound_valid_and_fills_the_world_cache_before_the_bake() {
    let mut f = fixture("wbound");
    let visual = f.visual;
    assert_eq!(f.arena.state(visual).unwrap().visual_flags & 0x60040, 0);
    let expected = f.arena.visual_world_bounds(visual).unwrap();
    f.arena.set_object_space_world(visual).unwrap();
    // The cache holds the box computed before the bake from the local box
    // and the frame matrix; the final flag store marks it stale (0x40000
    // clear) but keeps 0x20000.
    let (cache, error) = f.arena.visual_world_bound_snapshot(visual).unwrap();
    assert_eq!(error, None);
    assert_eq!(cache, expected);
    assert_eq!(
        f.arena.state(visual).unwrap().visual_flags & 0x60040,
        0x20040
    );
    f.close();
}

#[test]
fn an_interrupted_bake_is_not_replayed() {
    // The first vertex bakes; the second normal's Normalize length overflows.
    let mut f = fixture_with("interrupted", [[0., 0., 2.], [f32::MAX; 3], [0., 0., 2.]]);
    let visual = f.visual;
    let view = f.arena.lock_visual_lod0_vertices(visual).unwrap().unwrap();
    assert_eq!(
        f.arena.set_object_space_world(visual),
        Err(ModelError::SetSpaceNumeric(visual))
    );
    // Vertex 0 is baked; vertex 1 kept its position and rotated normal
    // stores before the stop; vertex 2 is untouched.
    let baked = [Some([2., 2., 2.]), Some([4., 6., 8.]), Some([0., 0., 0.])];
    assert_eq!([0, 1, 2].map(|i| view.position(i)), baked);
    let object = f.arena.frame_object_resources(visual).unwrap().unwrap();
    let resources = f.arena.loaded_visual_resources().unwrap();
    let mesh = resources.objects().object(object).unwrap().mesh.unwrap();
    let level = resources.objects().get_lod(mesh, 0).unwrap().unwrap();
    let window = resources.game_level_vertex_window(level).unwrap();
    assert_eq!(
        resources.game_vertex_position_normal(window, 1).unwrap(),
        [[4., 6., 8.], [f32::MAX; 3]]
    );
    assert_eq!(f.arena.state(visual).unwrap().visual_flags & 0x40, 0);
    // A replay is refused before any write: vertex 0 is not moved again.
    assert_eq!(
        f.arena.set_object_space_world(visual),
        Err(ModelError::SetSpaceInterrupted(visual))
    );
    assert_eq!([0, 1, 2].map(|i| view.position(i)), baked);
    f.close();
}

#[test]
fn owner_writes_refuse_a_borrowed_allocation_and_views_stay_read_only() {
    use crate::loaded_visual_resources::{LoadedVisualError, LoadedVisualResources};
    let mut f = fixture("borrowed");
    let visual = f.visual;
    let view = f.arena.lock_visual_lod0_vertices(visual).unwrap().unwrap();
    let object = f.arena.frame_object_resources(visual).unwrap().unwrap();
    let resources = f.arena.loaded_visual_resources().unwrap();
    let mesh = resources.objects().object(object).unwrap().mesh.unwrap();
    let level = resources.objects().get_lod(mesh, 0).unwrap().unwrap();
    let window = resources.game_level_vertex_window(level).unwrap();
    let graphics = LoadedVisualResources::game_window_graphics(window);
    let shared = resources.backend().graphics_shared_bytes(graphics).unwrap();
    let revision = resources.backend().graphics_revision(graphics).unwrap();
    // A borrow held elsewhere (an inspection view's `Ref`) makes the
    // owner's write a reported refusal, not a panic; nothing changes.
    let held = shared.borrow();
    let owner = f.arena.loaded_visual_resources_mut().unwrap();
    assert!(matches!(
        owner.game_write_vertex_word(window, 1, 0, 9.0),
        Err(LoadedVisualError::Backend(_))
    ));
    assert_eq!(owner.backend().graphics_revision(graphics), Some(revision));
    // The view reads through a shared borrow alongside it.
    assert_eq!(view.position(1), Some([1., 2., 3.]));
    drop(held);
    // The owner's write lands, advances the revision for derived data, and
    // the view sees it.
    let owner = f.arena.loaded_visual_resources_mut().unwrap();
    owner.game_write_vertex_word(window, 1, 0, 9.0).unwrap();
    assert_eq!(
        owner.backend().graphics_revision(graphics),
        Some(revision + 1)
    );
    assert_eq!(view.position(1), Some([9., 2., 3.]));
    // With the buffers published (unlocked) and a renderer inspection view
    // holding its borrow, the collision view still reads.
    f.arena.publish_runtime_buffers().unwrap();
    let resources = f.arena.loaded_visual_resources().unwrap();
    let inspection = resources.inspection_mesh(object, 16).unwrap().unwrap();
    assert_eq!(view.position(1), Some([9., 2., 3.]));
    drop(inspection);
    f.close();
}

#[test]
fn detach_keeps_the_native_locks_and_other_buffers_locks() {
    let mut f = fixture("detach-locks");
    let visual = f.visual;
    let object = f.arena.frame_object_resources(visual).unwrap().unwrap();
    let owner = f.arena.loaded_visual_resources_mut().unwrap();
    let other = owner.load_plain_instance(object).unwrap();
    let shared = owner.objects().object(object).unwrap().mesh.unwrap();
    let level = owner.objects().get_lod(shared, 0).unwrap().unwrap();
    let group = owner
        .objects()
        .managed()
        .resources()
        .mesh_level(level)
        .unwrap()
        .fgroups
        .unwrap();
    // An index lock taken before SetSpace (another owner's GetFaces) and a
    // LOD0 vertex lock (an earlier LoadCollision link's LockVertices(1)).
    assert!(owner.game_lock_group_indices_for_test(group).unwrap());
    assert!(owner.game_lock_level_vertices(level, 1).unwrap());
    let managers = |arena: &ModelArena| {
        let managers = arena
            .loaded_visual_resources()
            .unwrap()
            .objects()
            .managed()
            .managers();
        (
            managers.index_locked().to_vec(),
            managers.vertex_locked().to_vec(),
        )
    };
    let (indices_before, vertices_before) = managers(&f.arena);
    assert_eq!((indices_before.len(), vertices_before.len()), (1, 1));
    assert!(matches!(
        f.arena.set_object_space_world(visual),
        Ok(ObjectSpaceWorld::Baked { detached: true, .. })
    ));
    let (indices, vertices) = managers(&f.arena);
    // The earlier locks stay: the Duplicate only takes its own locks and
    // unlocks nothing.
    assert!(indices.starts_with(&indices_before), "{indices:?}");
    assert!(
        vertices_before.iter().all(|key| vertices.contains(key)),
        "{vertices:?}"
    );
    let owner = f.arena.loaded_visual_resources().unwrap();
    let own = owner.objects().object(object).unwrap().mesh.unwrap();
    let copy = owner.objects().get_lod(own, 0).unwrap().unwrap();
    let copy_group = owner
        .objects()
        .managed()
        .resources()
        .mesh_level(copy)
        .unwrap()
        .fgroups
        .unwrap();
    assert_ne!(copy_group, group);
    f.arena.publish_runtime_buffers().unwrap();
    f.arena
        .loaded_visual_resources_mut()
        .unwrap()
        .destroy_object(other)
        .unwrap();
    f.close();
}
