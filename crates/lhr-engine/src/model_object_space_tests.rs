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

#[test]
fn bake_stops_outside_the_guarded_domain() {
    let m = matrix([
        [f32::MAX, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ]);
    assert_eq!(
        bake_world_vertex([2.0, 0.0, 0.0], [0.0, 0.0, 1.0], &m),
        Err(())
    );
}

struct Fixture {
    dir: std::path::PathBuf,
    arena: ModelArena,
    model: ModelKey,
    visual: FrameKey,
    dummy: FrameKey,
}

fn fixture(name: &str) -> Fixture {
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
            .map(|position| lhr_assets::Vertex {
                position,
                normal: [0., 0., 2.],
                uv: [0.25, 0.75],
            })
            .to_vec(),
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
    let flags = f.arena.state(visual).unwrap().visual_flags;
    assert_eq!((flags & 0x40, flags & 0x40000), (0x40, 0));
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
