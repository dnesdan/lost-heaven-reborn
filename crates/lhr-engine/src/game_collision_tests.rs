use super::*;
use lhr_assets::klz::Klz;

fn put(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn push_f32s(bytes: &mut Vec<u8>, values: &[f32]) {
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
}

const FACE_FLAGS: u32 = 0x1234_5600;

/// A synthetic tree.klz over a 2x1 grid (x 0..10, z 0..5) with three level
/// vertices.
struct Source {
    version: u32,
    links: Vec<(u32, &'static [u8])>,
    dyn_cell_size: f32,
    /// (flags word, link, indices).
    faces: Vec<(u32, u32, [u16; 3])>,
    /// (flags word, link).
    aabbs: Vec<(u32, u32)>,
    /// Each cell's volume words.
    cells: [Vec<u32>; 2],
}

impl Source {
    /// Links `vis` (KLZF_VISUAL) and `dummy` (0); faces on the level pool,
    /// `vis` 0,1,2, `vis` 0,0,1 (coincident) and `dummy` 0,1,2; one AABB on
    /// `dummy`. Cell 0 holds faces 0-2 and the AABB, cell 1 face 3.
    fn standard(dyn_cell_size: f32) -> Self {
        Self {
            version: 5,
            links: vec![(KLZF_VISUAL, b"vis"), (0, b"dummy")],
            dyn_cell_size,
            faces: vec![
                (FACE_FLAGS, u32::from(KLZ_LEVEL_VERTICES), [0, 1, 2]),
                (FACE_FLAGS, 0, [0, 1, 2]),
                (FACE_FLAGS, 0, [0, 0, 1]),
                (FACE_FLAGS, 1, [0, 1, 2]),
            ],
            aabbs: vec![(0x81, 1)],
            cells: [vec![0, 32, 64, 0x8100_0000], vec![96]],
        }
    }

    fn build(&self) -> Vec<u8> {
        let mut b = vec![0_u8; 24];
        b[..4].copy_from_slice(b"GifC");
        put(&mut b, 4, self.version);
        put(&mut b, 12, self.links.len() as u32);
        b.resize(24 + 4 * self.links.len(), 0);
        for (index, (flags, name)) in self.links.iter().enumerate() {
            let record = b.len() as u32;
            put(&mut b, 24 + 4 * index, record);
            b.extend_from_slice(&flags.to_le_bytes());
            b.extend_from_slice(name);
            b.push(0);
            while !b.len().is_multiple_of(4) {
                b.push(0);
            }
        }
        let grid = b.len();
        put(&mut b, 8, grid as u32);
        b.resize(grid + 104, 0);
        for (i, value) in [0_f32, 0., 10., 5., 5., 5.].into_iter().enumerate() {
            put(&mut b, grid + i * 4, value.to_bits());
        }
        put(&mut b, grid + 24, 2);
        put(&mut b, grid + 28, 1);
        put(&mut b, grid + 0x20, self.dyn_cell_size.to_bits());
        put(&mut b, grid + 0x30, self.faces.len() as u32);
        put(&mut b, grid + 0x40, self.aabbs.len() as u32);
        put(&mut b, grid + 0x60, 3);
        push_f32s(&mut b, &[0., 5., 10.]);
        push_f32s(&mut b, &[0., 5.]);
        b.extend_from_slice(&[0; 4]);
        for (flags, link, indices) in &self.faces {
            b.extend_from_slice(&flags.to_le_bytes());
            for index in indices {
                b.extend_from_slice(&((link << 16) | u32::from(*index)).to_le_bytes());
            }
            push_f32s(&mut b, &[0., 1., 0., 0.]);
        }
        for (flags, link) in &self.aabbs {
            b.extend_from_slice(&flags.to_le_bytes());
            b.extend_from_slice(&link.to_le_bytes());
            push_f32s(&mut b, &[0., 0., 0., 1., 1., 1.]);
        }
        push_f32s(&mut b, &[0., 0., 0., 4., 0., 0., 0., 0., 4.]);
        b.extend_from_slice(&[0; 4]);
        for refs in &self.cells {
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
}

/// The standard synthetic tree.klz (see [`Source::standard`]).
pub(crate) fn fixture(dyn_cell_size: f32) -> Vec<u8> {
    Source::standard(dyn_cell_size).build()
}

const VIS: FrameKey = FrameKey(10);
const DUMMY: FrameKey = FrameKey(11);

#[derive(Default)]
struct Scene {
    source: Option<Vec<u8>>,
    frames: Vec<(&'static [u8], FrameKey, u32)>,
    lod0: Vec<(FrameKey, LodVertexBuffer)>,
    fail_set_space: bool,
    fail_lock: Option<FrameKey>,
    /// A write the owner makes into a buffer while `frame` is locked.
    on_lock: Option<(FrameKey, LodVertexBuffer, u16, [f32; 3])>,
    calls: Vec<String>,
}

impl Scene {
    fn with(source: Vec<u8>) -> Self {
        Self {
            source: Some(source),
            frames: vec![(b"vis", VIS, FRAME_VISUAL), (b"dummy", DUMMY, 6)],
            lod0: vec![(
                VIS,
                LodVertexBuffer::new(vec![[0., 0., 0.], [4., 0., 0.], [0., 0., 4.]]),
            )],
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
    ) -> Result<Option<LodVertexBuffer>, Self::Error> {
        self.calls.push(format!("lock {}", frame.0));
        if self.fail_lock == Some(frame) {
            return Err("no LOD owner");
        }
        if let Some((_, buffer, index, position)) =
            self.on_lock.as_ref().filter(|(key, ..)| *key == frame)
        {
            assert!(buffer.set_position(*index, *position));
        }
        Ok(self
            .lod0
            .iter()
            .find(|(key, _)| *key == frame)
            .map(|(_, vertices)| vertices.clone()))
    }
}

fn properties(level: &CollisionLevel) -> Vec<u32> {
    level.triangles.iter().map(|t| t.properties).collect()
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
    assert_eq!(level.phase(), CollisionLevelPhase::Coherent);
    assert_eq!(level.grid().unwrap().width, 2);
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
    assert_eq!(properties(level), [0x1234_5604, 0x1234_5604, 0x20, 0x20]);
    assert_eq!(
        level.triangles[0].vertices,
        TriangleVertices::Level([0, 1, 2])
    );
    assert_eq!(level.triangle_plane(0), Some([0., 1., 0., 0.]));
    assert_eq!(
        level.triangles[3].vertices,
        TriangleVertices::Unbound {
            link: 1,
            indices: [0, 1, 2]
        }
    );
    assert_eq!(
        level.triangle_positions(0),
        Some([[0., 0., 0.], [4., 0., 0.], [0., 0., 4.]])
    );
    assert_eq!(
        level.triangle_positions(1),
        Some([[0., 0., 0.], [4., 0., 0.], [0., 0., 4.]])
    );
    assert_eq!(level.triangle_positions(3), None);
    assert_eq!(level.cells.len(), 2);
    assert_eq!(
        level.cells[1].references,
        [CellVolume::Primitive(PrimitiveId {
            kind: PrimitiveKind::Triangle,
            index: 3
        })]
    );
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
    assert_eq!(level.volumes[0].frame, None);
    // Every frame face is now unbound and disabled; the pool face is not.
    assert_eq!(properties(level), [0x1234_5604, 0x20, 0x20, 0x20]);
    // Without dyn no grid is created.
    assert_eq!(owner.dynamic_grid(), None);
}

#[test]
fn locked_lod_buffers_stay_live_across_later_locks_and_after_install() {
    let mut source = Source::standard(2.0);
    source.links[1].0 = KLZF_VISUAL;
    let mut scene = Scene::with(source.build());
    scene.frames[1].2 = FRAME_VISUAL;
    // `dummy`'s buffer has coincident corners 0 and 1 when it is locked
    // (first, backwards); locking `vis` afterwards moves corner 1 away.
    let shared = LodVertexBuffer::new(vec![[0., 0., 0.], [0., 0., 0.], [0., 0., 4.]]);
    scene.lod0.push((DUMMY, shared.clone()));
    scene.on_lock = Some((VIS, shared.clone(), 1, [4., 0., 0.]));
    let vis_buffer = scene.lod0[0].1.clone();
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Ok(LoadCollisionOutcome::Installed)
    );
    assert_eq!(
        scene.calls,
        [
            "read x",
            "find dummy",
            "type 11",
            "set_space 11",
            "find vis",
            "type 10",
            "set_space 10",
            "type 11",
            "lock 11",
            "type 10",
            "lock 10",
        ]
    );
    let level = owner.level().unwrap();
    // Face 3 was bound after `lock 10` wrote into the already locked
    // buffer: its corners are read live, so it stays enabled and is
    // classified. A copy taken at `lock 11` would have disabled it.
    assert_eq!(properties(level)[3], 0x1234_5604);
    match &level.triangles[3].vertices {
        TriangleVertices::Frame { link, buffer, .. } => {
            assert_eq!(*link, 1);
            assert!(buffer.same_buffer(&shared));
            assert!(!buffer.same_buffer(&vis_buffer));
        }
        other => panic!("face 3 bound to {other:?}"),
    }
    // The installed level keeps reading the owner's buffer.
    shared.set_position(1, [9., 0., 0.]);
    assert_eq!(
        level.triangle_positions(3),
        Some([[0., 0., 0.], [9., 0., 0.], [0., 0., 4.]])
    );
}

#[test]
fn lod_lock_stop_keeps_the_published_level_and_bound_volumes() {
    let mut scene = Scene::with(fixture(2.0));
    scene.fail_lock = Some(VIS);
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Err(LoadCollisionError::Scene {
            stage: LoadCollisionStage::LockLod0 { link: 0 },
            error: "no LOD owner"
        })
    );
    assert!(owner.loaded_level());
    assert_ne!(owner.level_size(), 0);
    assert_eq!(
        owner.counts(),
        GameCollisionCounts {
            levels: 1,
            dynamic_grids: 0
        }
    );
    let level = owner.level().unwrap();
    assert_eq!(level.phase(), CollisionLevelPhase::VolumesBound);
    assert_eq!(level.grid().unwrap().height, 1);
    assert_eq!(level.volumes[0].frame, Some(DUMMY));
    assert!(level.triangles.is_empty() && level.cells.is_empty());
}

#[test]
fn coherence_stop_keeps_the_dynamic_grid_and_earlier_classifications() {
    let mut source = Source::standard(2.0);
    // An AABB without the non-face bit: GrantCoherence reaches it after
    // classifying faces 0 and 1 of cell 0.
    source.aabbs[0].0 = 0x01;
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut Scene::with(source.build())),
        Err(LoadCollisionError::NonFaceClassification {
            primitive: PrimitiveId {
                kind: PrimitiveKind::Aabb,
                index: 0
            }
        })
    );
    let level = owner.level().unwrap();
    assert_eq!(level.phase(), CollisionLevelPhase::CellsBound);
    assert_eq!(properties(level), [0x1234_5604, 0x1234_5604, 0x20, 0x20]);
    assert_eq!(owner.dynamic_grid().map(|grid| grid.width), Some(5));
    assert_eq!(
        owner.counts(),
        GameCollisionCounts {
            levels: 1,
            dynamic_grids: 1
        }
    );
}

#[test]
fn native_accepts_inputs_the_audit_reader_rejects() {
    // An unchecked version word and a level-pool face stored disabled: the
    // native loader binds it and GrantCoherence skips it unread.
    let mut source = Source::standard(2.0);
    source.version = 6;
    source.faces[0].0 = FACE_FLAGS | VOLF_DISABLED;
    let bytes = source.build();
    assert!(Klz::parse(&bytes).is_err());
    source.version = 5;
    assert!(Klz::parse(&source.build()).is_err());
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut Scene::with(bytes)),
        Ok(LoadCollisionOutcome::Installed)
    );
    let level = owner.level().unwrap();
    assert_eq!(
        level.triangles[0].vertices,
        TriangleVertices::Level([0, 1, 2])
    );
    assert_eq!(properties(level), [0x1234_5620, 0x1234_5604, 0x20, 0x20]);

    // The last cell's per-volume bytes are not read by the load.
    let mut bytes = fixture(2.0);
    bytes.truncate(bytes.len() - 4);
    assert!(Klz::parse(&bytes).is_err());
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut Scene::with(bytes)),
        Ok(LoadCollisionOutcome::Installed)
    );
}

#[test]
fn a_later_defect_stops_only_where_native_reaches_it() {
    let reached = [
        "read x",
        "find dummy",
        "find vis",
        "type 10",
        "set_space 10",
        "type 11",
        "type 10",
        "lock 10",
    ];
    // An unknown tag in cell 1: resolved as stored, read by GrantCoherence
    // after cell 0 was classified.
    let mut source = Source::standard(2.0);
    source.cells[1] = vec![0x5500_0000];
    let bytes = source.build();
    assert!(Klz::parse(&bytes).is_err());
    let mut scene = Scene::with(bytes);
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Err(LoadCollisionError::CellReference {
            cell: 1,
            reference: 0
        })
    );
    assert_eq!(scene.calls, reached);
    let level = owner.level().unwrap();
    assert_eq!(level.phase(), CollisionLevelPhase::CellsBound);
    assert_eq!(
        level.cells[1].references,
        [CellVolume::Unresolved(0x5500_0000)]
    );
    assert_eq!(properties(level)[..2], [0x1234_5604, 0x1234_5604]);
    assert!(owner.dynamic_grid().is_some());

    // A cell table cut inside cell 1's volume words stops the cell loop
    // after the faces were bound.
    let mut bytes = fixture(2.0);
    bytes.truncate(bytes.len() - 6);
    let mut scene = Scene::with(bytes);
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Err(LoadCollisionError::CellRecord { cell: 1 })
    );
    assert_eq!(scene.calls, reached);
    let level = owner.level().unwrap();
    assert_eq!(level.phase(), CollisionLevelPhase::FacesBound);
    assert_eq!(level.cells.len(), 1);
    assert_eq!(properties(level), [FACE_FLAGS, FACE_FLAGS, 0x20, 0x20]);
    assert_eq!(owner.dynamic_grid(), None);
}

#[test]
fn shared_link_records_read_the_earlier_link_word() {
    let shared = || {
        let mut bytes = fixture(2.0);
        let first = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        put(&mut bytes, 28, first);
        bytes
    };
    // Link 1 resolves `vis` and stores its frame where link 0's flags are.
    let mut scene = Scene::with(shared());
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Err(LoadCollisionError::LinkAlias { link: 0 })
    );
    assert_eq!(
        scene.calls,
        ["read x", "find vis", "type 10", "set_space 10"]
    );
    assert!(!owner.loaded_level());
    assert_eq!(owner.counts(), GameCollisionCounts::default());

    // An unresolved link stores null: link 0 reads zero flags and skips
    // the visual gate.
    let mut scene = Scene::with(shared());
    scene.frames.clear();
    let mut owner = GameCollision::default();
    assert_eq!(
        owner.load_collision("x", true, &mut scene),
        Ok(LoadCollisionOutcome::Installed)
    );
    assert_eq!(scene.calls, ["read x", "find vis", "find vis"]);
    assert_eq!(
        properties(owner.level().unwrap()),
        [0x1234_5604, 0x20, 0x20, 0x20]
    );
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
