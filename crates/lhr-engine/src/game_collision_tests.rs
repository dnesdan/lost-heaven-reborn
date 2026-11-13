use super::*;

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

/// Synthetic tree.klz: links `vis` (KLZF_VISUAL) and `dummy` (0), a 2x1
/// grid over x 0..10, z 0..5, `dyn_cell_size` as given, four faces, one AABB
/// and three level vertices. Cell 0 holds faces 0-2 and the AABB, cell 1
/// face 3.
pub(crate) fn fixture(dyn_cell_size: f32) -> Vec<u8> {
    let mut b = vec![0_u8; 24];
    b[..4].copy_from_slice(b"GifC");
    put(&mut b, 4, 5);
    put(&mut b, 12, 2);
    b.resize(24 + 8, 0);
    let mut records = Vec::new();
    for (flags, name) in [(KLZF_VISUAL, &b"vis"[..]), (0, &b"dummy"[..])] {
        records.push(b.len());
        b.extend_from_slice(&flags.to_le_bytes());
        b.extend_from_slice(name);
        b.push(0);
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
    }
    put(&mut b, 24, records[0] as u32);
    put(&mut b, 28, records[1] as u32);
    let grid = b.len();
    put(&mut b, 8, grid as u32);
    b.resize(grid + 104, 0);
    for (i, value) in [0_f32, 0., 10., 5., 5., 5.].into_iter().enumerate() {
        put(&mut b, grid + i * 4, value.to_bits());
    }
    put(&mut b, grid + 24, 2);
    put(&mut b, grid + 28, 1);
    put(&mut b, grid + 0x20, dyn_cell_size.to_bits());
    put(&mut b, grid + 0x30, 4);
    put(&mut b, grid + 0x40, 1);
    put(&mut b, grid + 0x60, 3);
    push_f32s(&mut b, &[0., 5., 10.]);
    push_f32s(&mut b, &[0., 5.]);
    b.extend_from_slice(&[0; 4]);
    // Faces: level pool; vis 0,1,2; vis 0,0,1 (coincident); dummy 0,1,2.
    let faces: [(u32, [u16; 3]); 4] = [
        (u32::from(KLZ_LEVEL_VERTICES), [0, 1, 2]),
        (0, [0, 1, 2]),
        (0, [0, 0, 1]),
        (1, [0, 1, 2]),
    ];
    for (link, indices) in faces {
        b.extend_from_slice(&0x1234_5600_u32.to_le_bytes());
        for index in indices {
            b.extend_from_slice(&((link << 16) | u32::from(index)).to_le_bytes());
        }
        push_f32s(&mut b, &[0., 1., 0., 0.]);
    }
    // AABB on `dummy`.
    b.extend_from_slice(&0x0000_0081_u32.to_le_bytes());
    b.extend_from_slice(&1_u32.to_le_bytes());
    push_f32s(&mut b, &[0., 0., 0., 1., 1., 1.]);
    push_f32s(&mut b, &[0., 0., 0., 4., 0., 0., 0., 0., 4.]);
    b.extend_from_slice(&[0; 4]);
    for refs in [&[0_u32, 32, 64, 0x8100_0000][..], &[96][..]] {
        b.extend_from_slice(&(refs.len() as u32).to_le_bytes());
        b.extend_from_slice(&[0; 8]);
        push_f32s(&mut b, &[2.5]);
        for packed in refs {
            b.extend_from_slice(&packed.to_le_bytes());
        }
        b.extend(std::iter::repeat_n(0, refs.len()));
        while !b.len().is_multiple_of(4) {
            b.push(0);
        }
    }
    b
}

const VIS: FrameKey = FrameKey(10);
const DUMMY: FrameKey = FrameKey(11);

#[derive(Default)]
struct Scene {
    source: Option<Vec<u8>>,
    frames: Vec<(&'static [u8], FrameKey, u32)>,
    lod0: Vec<(FrameKey, Vec<[f32; 3]>)>,
    fail_set_space: bool,
    calls: Vec<String>,
}

impl Scene {
    fn with(source: Vec<u8>) -> Self {
        Self {
            source: Some(source),
            frames: vec![(b"vis", VIS, FRAME_VISUAL), (b"dummy", DUMMY, 6)],
            lod0: vec![(VIS, vec![[0., 0., 0.], [4., 0., 0.], [0., 0., 4.]])],
            ..Self::default()
        }
    }
}

impl CollisionLevelScene for Scene {
    type Error = &'static str;
    fn read_source(&mut self, name: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        self.calls.push(format!("read {name}"));
        Ok(self.source.clone())
    }
    fn find_frame_fast(&mut self, name: &[u8]) -> Result<Option<FrameKey>, Self::Error> {
        self.calls
            .push(format!("find {}", String::from_utf8_lossy(name)));
        Ok(self
            .frames
            .iter()
            .find(|(n, _, _)| *n == name)
            .map(|(_, key, _)| *key))
    }
    fn frame_type(&mut self, frame: FrameKey) -> Result<u32, Self::Error> {
        self.calls.push(format!("type {}", frame.0));
        Ok(self
            .frames
            .iter()
            .find(|(_, key, _)| *key == frame)
            .unwrap()
            .2)
    }
    fn set_visual_space_world(&mut self, frame: FrameKey) -> Result<(), Self::Error> {
        self.calls.push(format!("set_space {}", frame.0));
        if self.fail_set_space {
            return Err("no bake owner");
        }
        Ok(())
    }
    fn lock_lod0_vertices(
        &mut self,
        frame: FrameKey,
    ) -> Result<Option<Vec<[f32; 3]>>, Self::Error> {
        self.calls.push(format!("lock {}", frame.0));
        Ok(self
            .lod0
            .iter()
            .find(|(key, _)| *key == frame)
            .map(|(_, vertices)| vertices.clone()))
    }
}

#[test]
fn load_collision_installs_in_native_call_order() {
    let source = fixture(2.0);
    let data_offset = u32::from_le_bytes(source[8..12].try_into().unwrap());
    let mut scene = Scene::with(source.clone());
    let mut owner = GameCollision::default();
    owner.set_step_actor(Some(ActorReferenceId::from_existing(3)));
    assert_eq!(
        owner.load_collision("missions/x/tree.klz", true, &mut scene),
        Ok(LoadCollisionOutcome::Installed)
    );
    // Links backwards with the visual gate, then the LOD0 loop backwards
    // rereading the live type.
    assert_eq!(
        scene.calls,
        [
            "read missions/x/tree.klz",
            "find dummy",
            "find vis",
            "type 10",
            "set_space 10",
            "type 11",
            "type 10",
            "lock 10",
        ]
    );
    assert_eq!(owner.step_actor(), None);
    assert!(owner.loaded_level());
    assert_eq!(owner.level_size(), source.len() as u32 - data_offset);
    let level = owner.level().unwrap();
    assert_eq!(level.frames, [Some(VIS), Some(DUMMY)]);
    assert!(level.frame_vertices[1].is_none());
    assert_eq!(
        level.volumes,
        [LevelVolume {
            primitive: PrimitiveId {
                kind: PrimitiveKind::Aabb,
                index: 0
            },
            properties: 0x81,
            frame: Some(DUMMY),
        }]
    );
    // Face 0 (level pool) and face 1 are classified from their normal (Y)
    // and first edge along X: 4. Face 2 has coincident corners, face 3 a
    // frame without vertices: whole-word 0x20, never classified.
    let properties: Vec<u32> = level.triangles.iter().map(|t| t.properties).collect();
    assert_eq!(properties, [0x1234_5604, 0x1234_5604, 0x20, 0x20]);
    assert_eq!(
        level.triangles[0].vertices,
        TriangleVertices::Level([0, 1, 2])
    );
    assert_eq!(
        level.triangles[3].vertices,
        TriangleVertices::Unbound {
            link: 1,
            indices: [0, 1, 2]
        }
    );
    assert_eq!(
        level.triangle_positions(1),
        Some([[0., 0., 0.], [4., 0., 0.], [0., 0., 4.]])
    );
    assert_eq!(level.triangle_positions(3), None);
    assert_eq!(level.cells.len(), 2);
    assert_eq!(level.cells[1].references[0].index, 3);
    assert_eq!(level.cells[0].max_y, 2.5);
    assert_eq!(
        owner.dynamic_grid(),
        Some(&DynamicCollisionGrid {
            min_x: 0.,
            min_z: 0.,
            cell_size_x: 2.,
            cell_size_z: 2.5,
            width: 5,
            height: 2,
        })
    );
    assert_eq!(
        owner.counts(),
        GameCollisionCounts {
            levels: 1,
            dynamic_grids: 1
        }
    );
    assert_eq!(
        owner.load_progress(),
        LoadCollisionProgress {
            links: 2,
            links_visited: 2,
            links_resolved: 2,
            set_space_calls: 1
        }
    );
}

#[test]
fn absent_or_foreign_source_returns_with_the_previous_level_deleted() {
    let mut owner = GameCollision::default();
    let mut scene = Scene::with(fixture(2.0));
    owner.load_collision("a", true, &mut scene).unwrap();
    let level_size = owner.level_size();
    owner.set_step_actor(Some(ActorReferenceId::from_existing(1)));
    scene.source = None;
    scene.calls.clear();
    assert_eq!(
        owner.load_collision("b", true, &mut scene),
        Ok(LoadCollisionOutcome::SourceAbsent)
    );
    assert_eq!(scene.calls, ["read b"]);
    assert_eq!(owner.counts(), GameCollisionCounts::default());
    assert_eq!(owner.step_actor(), None);
    // Neither field is touched by DeleteCollsLevel or the early return.
    assert_eq!(owner.level_size(), level_size);
    assert!(owner.loaded_level());

    let mut foreign = fixture(2.0);
    foreign[..4].copy_from_slice(b"GifD");
    let mut scene = Scene::with(foreign);
    assert_eq!(
        owner.load_collision("c", true, &mut scene),
        Ok(LoadCollisionOutcome::MagicMismatch)
    );
    assert_eq!(scene.calls, ["read c"]);
    assert_eq!(owner.level_size(), level_size);
    let mut scene = Scene::with(b"Gif".to_vec());
    assert_eq!(
        owner.load_collision("d", true, &mut scene),
        Err(LoadCollisionError::TruncatedHeader)
    );
}

#[test]
fn set_space_stop_keeps_reached_effects_and_publishes_no_level() {
    let source = fixture(2.0);
    let mut scene = Scene::with(source.clone());
    scene.fail_set_space = true;
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Err(LoadCollisionError::Scene {
            stage: LoadCollisionStage::SetSpace { link: 0 },
            error: "no bake owner"
        })
    );
    assert_eq!(
        scene.calls,
        [
            "read x",
            "find dummy",
            "find vis",
            "type 10",
            "set_space 10"
        ]
    );
    // level_size is stored before the links; LoadedLevel only after them.
    assert_ne!(owner.level_size(), 0);
    assert!(!owner.loaded_level());
    assert_eq!(owner.counts(), GameCollisionCounts::default());
    assert_eq!(
        owner.load_progress(),
        LoadCollisionProgress {
            links: 2,
            links_visited: 1,
            links_resolved: 1,
            set_space_calls: 1
        }
    );
}

#[test]
fn visual_link_drops_a_non_visual_frame_and_unresolved_links_stay_null() {
    let mut scene = Scene::with(fixture(2.0));
    scene.frames = vec![(b"vis", VIS, 6)];
    let mut owner = GameCollision::default();
    owner.load_collision("x", false, &mut scene).unwrap();
    assert_eq!(scene.calls, ["read x", "find dummy", "find vis", "type 10"]);
    let level = owner.level().unwrap();
    assert_eq!(level.frames, [None, None]);
    assert_eq!(level.volumes[0].frame, None);
    // Every frame face is now unbound and disabled; the pool face is not.
    let properties: Vec<u32> = level.triangles.iter().map(|t| t.properties).collect();
    assert_eq!(properties, [0x1234_5604, 0x20, 0x20, 0x20]);
    // Without dyn no grid is created.
    assert_eq!(owner.dynamic_grid(), None);
}

#[test]
fn dynamic_grid_truncates_raises_zero_to_one_and_rejects_negative_counts() {
    let grid = |dyn_cell_size| CollisionLevelGrid {
        min_x: 0.,
        min_z: 0.,
        max_x: 10.,
        max_z: 5.,
        cell_size_x: 5.,
        cell_size_z: 5.,
        width: 2,
        height: 1,
        dyn_cell_size,
    };
    let three = dynamic_grid(&grid(3.0)).unwrap();
    assert_eq!((three.width, three.height), (3, 1));
    assert_eq!(three.cell_size_x, 10.0_f32 / 3.0);
    assert_eq!(three.cell_size_z, 5.);
    let wide = dynamic_grid(&grid(20.0)).unwrap();
    assert_eq!((wide.width, wide.height), (1, 1));
    assert_eq!((wide.cell_size_x, wide.cell_size_z), (10., 5.));
    // (-1, 0) truncates to zero and is raised to one.
    let tiny = dynamic_grid(&grid(-20.0)).unwrap();
    assert_eq!((tiny.width, tiny.height), (1, 1));
    assert_eq!(dynamic_grid(&grid(-2.0)), None);
    assert_eq!(dynamic_grid(&grid(0.0)), None);
    assert_eq!(dynamic_grid(&grid(f32::NAN)), None);
}

#[test]
fn delete_and_game_done_free_level_and_grid_only() {
    let mut owner = GameCollision::default();
    owner.delete_colls_level();
    assert_eq!(owner.counts(), GameCollisionCounts::default());
    owner
        .load_collision("x", true, &mut Scene::with(fixture(2.0)))
        .unwrap();
    let level_size = owner.level_size();
    owner.set_step_actor(Some(ActorReferenceId::from_existing(2)));
    owner.game_done();
    assert_eq!(owner.counts(), GameCollisionCounts::default());
    assert_eq!(owner.level_size(), level_size);
    assert_eq!(owner.step_actor(), Some(ActorReferenceId::from_existing(2)));
    assert!(owner.loaded_level());
    owner.delete_colls_level();
    assert_eq!(owner.counts(), GameCollisionCounts::default());
}
