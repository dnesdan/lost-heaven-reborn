//! Shared synthetic saved-lightmap inputs for owner and GPU integration tests.
//! All images, meshes and saved payload bytes are generated here; no game data.
use crate::{
    frame_transform::FrameKey,
    lightmap_atlas::AtlasStatistics,
    model_loading::{ModelArena, ModelKey},
    scene_ownership::{SceneKey, SectorKey},
};
use std::{path::PathBuf, sync::Arc};

pub(crate) struct Fixture {
    pub path: PathBuf,
    pub arena: ModelArena,
    pub models: Vec<ModelKey>,
    pub scene: SceneKey,
    pub sector: SectorKey,
}
impl Fixture {
    pub fn frame(&self, model_index: usize) -> FrameKey {
        self.arena.model(self.models[model_index]).unwrap().frames[0]
    }
    /// Retire actual model resources and their roots. The bounded scene prefix
    /// itself is host-dropped; this helper does not claim native Scene::Close.
    pub fn close(mut self) {
        for model in self.models {
            self.arena.close(model).unwrap();
            self.arena.link_frame(model.root(), None, 0).unwrap();
            self.arena.release(model.root()).unwrap();
        }
        let resources = self.arena.loaded_visual_resources_mut().unwrap();
        resources.reclaim_unused_buffers().unwrap();
        assert_eq!(resources.backend().live_graphics(), 0);
        assert_eq!(resources.backend().live_textures(), 0);
        assert_eq!(
            resources.objects().lightmap_atlases().statistics(),
            AtlasStatistics::default()
        );
        std::fs::remove_dir_all(self.path).unwrap();
    }
}

pub(crate) fn fixture(model_count: usize, lod_count: usize) -> Fixture {
    fixture_with_emissions(model_count, lod_count, [[0.; 3]; 2])
}

pub(crate) fn fixture_with_emissions(
    model_count: usize,
    lod_count: usize,
    emission: [[f32; 3]; 2],
) -> Fixture {
    assert!(model_count > 0);
    assert!((1..=2).contains(&lod_count));
    let path = std::env::temp_dir().join(format!(
        "lhr-saved-lightmap-{}-{}",
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
    tga.extend_from_slice(&[32, 64, 128, 17]);
    std::fs::write(path.join("maps/synthetic.tga"), tga).unwrap();

    let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
    frame.visual = Some(1);
    frame.render_flags = 0;
    frame.logic_flags = 0;
    let lod = lhr_assets::Lod {
        distance: if lod_count == 1 { 0. } else { 5. },
        uv_only: None,
        vertices: [
            ([0., 0., 0.], [0., 0.]),
            ([2., 0., 0.], [1., 0.]),
            ([0., 2., 0.], [0., 1.]),
            ([2., 2., 0.], [1., 1.]),
        ]
        .map(|(position, uv)| lhr_assets::Vertex {
            position,
            normal: [0., 0., -1.],
            uv,
        })
        .to_vec(),
        groups: vec![
            lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            },
            lhr_assets::FaceGroup {
                material: 1,
                indices: vec![1, 3, 2],
            },
        ],
    };
    frame.lods.push(lod.clone());
    if lod_count == 2 {
        frame.lods.push(lhr_assets::Lod {
            distance: 10.,
            ..lod
        });
    }
    let source = Arc::new(lhr_assets::Model {
        materials: emission
            .map(|emission| lhr_assets::Material {
                flags: 0x10000000 | 0x40000,
                ambient: [0.4, 0.2, 0.1],
                diffuse: [0.2, 0.5, 0.7],
                texture: "synthetic.tga".into(),
                emission,
                opacity: 0.25,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            })
            .to_vec(),
        frames: vec![frame],
    });
    let installation = lhr_assets::Installation::open(&path).unwrap();
    let mut arena = ModelArena::default();
    let scene = arena.construct_scene_owner_prefix().unwrap();
    let sector = arena.scene(scene).unwrap().sectors[0];
    let mut models = Vec::new();
    for _ in 0..model_count {
        let model = arena.create_model().unwrap();
        arena
            .link_frame(model.root(), Some(sector.frame()), 0)
            .unwrap();
        arena.load_parsed_into_model(model, source.clone()).unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        models.push(model);
    }
    Fixture {
        path,
        arena,
        models,
        scene,
        sector,
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct BodyOffsets {
    /// The i32 expanded-vertex count, after the rectangle records.
    pub count: usize,
    pub uv: usize,
    pub vertex_info: usize,
    pub index_count: usize,
    pub indices: usize,
    pub face_counts: [usize; 2],
}

/// Mode2 body starting immediately after its u16 LOD flags. The discarded
/// second RGB map and the single color's dimension-only map catch cursor drift.
pub(crate) fn body(single_argb: u32, gradient: bool) -> (Vec<u8>, BodyOffsets) {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&2u16.to_le_bytes()); // rectangles
    bytes.extend_from_slice(&2u16.to_le_bytes()); // face groups
    bytes.push(0); // RGB rectangle
    bytes.extend_from_slice(&2u32.to_le_bytes()); // maps
    bytes.extend_from_slice(&4i32.to_le_bytes());
    bytes.extend_from_slice(&4i32.to_le_bytes());
    for y in 0u8..4 {
        for x in 0u8..4 {
            bytes.extend_from_slice(&if gradient {
                [40 + 10 * x, 20 + 15 * y, 80]
            } else {
                [32, 64, 96]
            });
        }
    }
    bytes.extend_from_slice(&2i32.to_le_bytes());
    bytes.extend_from_slice(&1i32.to_le_bytes());
    bytes.extend_from_slice(&[201, 202, 203, 204, 205, 206]);
    bytes.push(1); // single-color rectangle
    bytes.extend_from_slice(&single_argb.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes()); // map: dimensions only
    bytes.extend_from_slice(&17i32.to_le_bytes());
    bytes.extend_from_slice(&23i32.to_le_bytes());
    let count = bytes.len();
    bytes.extend_from_slice(&6i32.to_le_bytes());
    let uv = bytes.len();
    for _ in 0..2 {
        for pair in [[0.0f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
            for value in pair {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
    }
    let vertex_info = bytes.len();
    for (vertex, rectangle) in [(0u16, 0u16), (1, 0), (2, 0), (1, 1), (3, 1), (2, 1)] {
        bytes.extend_from_slice(&vertex.to_le_bytes());
        bytes.extend_from_slice(&rectangle.to_le_bytes());
    }
    let index_count = bytes.len();
    bytes.extend_from_slice(&6i32.to_le_bytes());
    let indices = bytes.len();
    for index in 0u16..6 {
        bytes.extend_from_slice(&index.to_le_bytes());
    }
    let mut face_counts = [0; 2];
    for (index, offset) in face_counts.iter_mut().enumerate() {
        *offset = bytes.len();
        bytes.extend_from_slice(&1i32.to_le_bytes());
        bytes.extend_from_slice(&(index as u16).to_le_bytes());
    }
    (
        bytes,
        BodyOffsets {
            count,
            uv,
            vertex_info,
            index_count,
            indices,
            face_counts,
        },
    )
}

pub(crate) fn record(level: u8, bodies: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = vec![0x21, 2];
    bytes.extend_from_slice(&u32::try_from(bodies.len()).unwrap().to_le_bytes());
    bytes.extend_from_slice(&5f32.to_le_bytes());
    bytes.extend_from_slice(&45f32.to_le_bytes());
    bytes.push(level);
    for body in bodies {
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(body);
    }
    bytes
}

pub(crate) fn payload(lods: usize, single_argb: u32) -> Vec<u8> {
    let mut bytes = vec![0x80];
    bytes.extend_from_slice(&record(1, &vec![body(single_argb, false).0; lods]));
    bytes
}
