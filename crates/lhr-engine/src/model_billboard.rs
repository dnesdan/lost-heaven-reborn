//! Live I3D_billboard frame state and inherited bound projection.
//! PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6:
//! ctor 0x10005db0, LoadImage 0x100094d0, ProductBillMatrix 0x10006090.
use super::*;
use crate::billboard_state::{
    product_bill_matrix, BillboardMatrixInput, BillboardNumericUnavailable, BillboardOrientation,
};
use crate::frame_camera::{CameraError, CameraKey};
use crate::native_transform::SMatrix;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BillboardFrame {
    pub source: Option<lhr_assets::BillboardSource>,
    pub orientation: BillboardOrientation,
}
impl BillboardFrame {
    pub fn from_source(source: lhr_assets::BillboardSource) -> Self {
        Self {
            source: Some(source),
            orientation: BillboardOrientation::default(),
        }
    }
    /// Native Billboard::LoadImage reads its five-byte tail only after the
    /// inherited Object::LoadImage has produced its reached resource prefix.
    pub fn apply_load_suffix(&mut self) {
        if let Some(source) = self.source {
            self.orientation =
                BillboardOrientation::from_source(source.rot_axis_word, source.axis_mode_byte);
        }
    }
}

/// UpdateBoundProc changes only the local AABB to the original mesh box's
/// all-axis minimum and maximum. The inherited sphere remains mesh-owned.
pub(crate) fn cube_local_bound(
    mut bound: crate::mesh_bounds::MeshBoundVolume,
) -> crate::mesh_bounds::MeshBoundVolume {
    // Native S_bbox::IsValid checks the X endpoints. An empty mesh retains
    // the inherited invalid box, rather than manufacturing a valid cube.
    if !matches!(
        bound.bbox[0][0].partial_cmp(&bound.bbox[1][0]),
        Some(std::cmp::Ordering::Less | std::cmp::Ordering::Equal)
    ) {
        return bound;
    }
    let mut min = bound.bbox[0][0];
    let mut max = bound.bbox[1][0];
    for axis in 1..3 {
        if bound.bbox[0][axis] < min {
            min = bound.bbox[0][axis];
        }
        if bound.bbox[1][axis] > max {
            max = bound.bbox[1][axis];
        }
    }
    bound.bbox = [[min; 3], [max; 3]];
    bound
}

impl ModelArena {
    /// I3D_billboard::ProductBillMatrix is reached only while drawing the
    /// selected visible LOD. The result is a draw matrix, never frame state.
    pub fn billboard_render_matrix(
        &mut self,
        frame: FrameKey,
        camera: CameraKey,
    ) -> Result<SMatrix, ModelError> {
        let orientation = self
            .frame(frame)?
            .billboard
            .ok_or(ModelError::OriginalModelOpenUnavailable(
                "billboard derived frame required",
            ))?
            .orientation;
        self.camera(camera)?;
        match orientation.axis_mode {
            0 | 1 => {
                let camera_world = self.world_matrix(camera.frame())?;
                let camera_world_position = camera_world.elements[12..15]
                    .try_into()
                    .expect("three position words");
                let world_space = self.state(frame)?.visual_flags & 0x40 != 0;
                let (inverse_matrix, render_matrix) = if world_space {
                    (SMatrix::identity(), SMatrix::identity())
                } else {
                    let world = self.world_matrix(frame)?;
                    let inverse = world
                        .inverse()
                        .map_err(|_| {
                            ModelError::BillboardNumeric(BillboardNumericUnavailable::Arithmetic)
                        })?
                        .0;
                    (inverse, world)
                };
                product_bill_matrix(
                    orientation,
                    &BillboardMatrixInput::Facing {
                        camera_world_position,
                        inverse_matrix: &inverse_matrix,
                        render_matrix: &render_matrix,
                    },
                )
                .map_err(ModelError::BillboardNumeric)
            }
            2 => {
                let camera_view_inverse = self
                    .camera(camera)?
                    .view_inverse_matrix()
                    .ok_or(CameraError::NumericUnavailable("camera view inverse"))?;
                let world = self.world_matrix(frame)?;
                let world_scale = world.try_get_scale().map_err(|_| {
                    ModelError::BillboardNumeric(BillboardNumericUnavailable::Arithmetic)
                })?;
                product_bill_matrix(
                    orientation,
                    &BillboardMatrixInput::CameraAligned {
                        camera_view_inverse: &camera_view_inverse,
                        world_scale,
                        world_matrix: &world,
                    },
                )
                .map_err(ModelError::BillboardNumeric)
            }
            3 => {
                let local = self.graph.local_matrix(frame)?;
                let camera_view_inverse = self
                    .camera(camera)?
                    .view_inverse_matrix()
                    .ok_or(CameraError::NumericUnavailable("camera view inverse"))?;
                let world = self.world_matrix(frame)?;
                product_bill_matrix(
                    orientation,
                    &BillboardMatrixInput::LocalAligned {
                        local_matrix: &local,
                        camera_view_inverse: &camera_view_inverse,
                        world_matrix: &world,
                    },
                )
                .map_err(ModelError::BillboardNumeric)
            }
            _ => Err(ModelError::BillboardNumeric(
                BillboardNumericUnavailable::InvalidOrientation,
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source() -> (Arc<Model>, [lhr_assets::BillboardSource; 1]) {
        let mut frame = super::super::tests::frame(1, None, [0.; 3]);
        frame.visual = Some(4);
        frame.render_flags = 0; // local-space owner exercises UpdateBoundProc's cube
        frame.name = "bb_test".into();
        frame.name_bytes = b"bb_test".to_vec();
        frame.lods = vec![lhr_assets::Lod {
            distance: 16.,
            uv_only: None,
            vertices: vec![
                lhr_assets::Vertex {
                    position: [-2., -1., 0.],
                    normal: [0., 0., 1.],
                    uv: [0., 0.],
                },
                lhr_assets::Vertex {
                    position: [2., -1., 0.],
                    normal: [0., 0., 1.],
                    uv: [1., 0.],
                },
                lhr_assets::Vertex {
                    position: [2., 1., 0.],
                    normal: [0., 0., 1.],
                    uv: [1., 1.],
                },
                lhr_assets::Vertex {
                    position: [-2., 1., 0.],
                    normal: [0., 0., 1.],
                    uv: [0., 1.],
                },
            ],
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2, 0, 2, 3],
            }],
        }];
        (
            Arc::new(Model {
                materials: vec![lhr_assets::Material {
                    flags: 1,
                    ambient: [1.; 3],
                    diffuse: [1.; 3],
                    texture: "bb.tga".into(),
                    emission: [0.; 3],
                    opacity: 1.,
                    alpha_texture: None,
                    reflection: None,
                    environment_texture: None,
                    animation: None,
                }],
                frames: vec![frame],
            }),
            [lhr_assets::BillboardSource {
                frame_index: 0,
                rot_axis_word: 2,
                axis_mode_byte: 0,
            }],
        )
    }

    #[test]
    fn billboard_source_and_cache_copy_own_derived_class_cube_and_shared_mesh() {
        let root = std::env::temp_dir().join(format!(
            "lhr-billboard-owner-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(root.join("maps")).unwrap();
        let mut tga = vec![0; 18];
        tga[2] = 2;
        tga[12] = 1;
        tga[14] = 1;
        tga[16] = 24;
        tga[17] = 0x20;
        tga.extend_from_slice(&[3, 5, 7]);
        std::fs::write(root.join("maps/bb.tga"), tga).unwrap();
        let installation = lhr_assets::Installation::open(&root).unwrap();
        let (source, suffix) = source();
        let mut arena = ModelArena::default();
        let original = arena.create_model().unwrap();
        let copy = arena.create_model().unwrap();
        arena
            .load_parsed_billboards_into_model(original, source, &suffix)
            .unwrap();
        arena
            .load_plain_visual_resources(original, &installation)
            .unwrap();
        arena.publish_runtime_buffers().unwrap();
        let frame = arena.model(original).unwrap().frames[0];
        let loaded = arena.frame(frame).unwrap();
        assert_eq!(loaded.billboard.unwrap().source, Some(suffix[0]));
        assert_eq!(
            loaded.billboard.unwrap().orientation,
            BillboardOrientation::default()
        );
        let object = loaded.object_resources.unwrap();
        let resources = arena.loaded_visual_resources().unwrap();
        assert_eq!(resources.backend().live_textures(), 1);
        assert_eq!(
            resources
                .inspection_mesh(object, 8)
                .unwrap()
                .unwrap()
                .visual_type,
            crate::i3d_object::ObjectVisualType::Billboard
        );
        let bound = arena.frame(frame).unwrap().visual_bounds.unwrap();
        assert_eq!(bound.bbox, [[-2.; 3], [2.; 3]]);
        // Duplicate reads the live derived fields, not the old 4DS suffix.
        let index = arena.frame_index(frame).unwrap();
        arena.frames[index].billboard.as_mut().unwrap().orientation = BillboardOrientation {
            rot_axis: 0,
            axis_mode: 1,
        };
        arena
            .duplicate_pristine_cache_model(copy, original)
            .unwrap();
        let copied = arena.model(copy).unwrap().frames[0];
        assert_eq!(
            arena.frame(copied).unwrap().billboard,
            arena.frame(frame).unwrap().billboard
        );
        assert_ne!(
            arena.frame(copied).unwrap().billboard.unwrap().orientation,
            BillboardOrientation::default()
        );
        let copied_object = arena.frame(copied).unwrap().object_resources.unwrap();
        let resources = arena.loaded_visual_resources().unwrap();
        assert_eq!(
            resources
                .inspection_mesh(copied_object, 8)
                .unwrap()
                .unwrap()
                .visual_type,
            crate::i3d_object::ObjectVisualType::Billboard
        );
        assert_eq!(
            resources.billboard_mesh_key(object).unwrap(),
            resources.billboard_mesh_key(copied_object).unwrap()
        );
        let created = arena
            .create_owned_object_visual(
                crate::i3d_object::ObjectVisualType::Billboard.source_fourcc(),
            )
            .unwrap_or_else(|error| panic!("{:?}", error.error))
            .unwrap();
        assert_eq!(
            arena.frame(created).unwrap().billboard.unwrap(),
            BillboardFrame::default()
        );
        arena.duplicate_owned_object_visual(created, frame).unwrap();
        assert_eq!(
            arena.frame(created).unwrap().billboard.unwrap().orientation,
            arena.frame(frame).unwrap().billboard.unwrap().orientation
        );
        let created_object = arena.frame(created).unwrap().object_resources.unwrap();
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .billboard_mesh_key(created_object)
                .unwrap(),
            arena
                .loaded_visual_resources()
                .unwrap()
                .billboard_mesh_key(object)
                .unwrap()
        );
        arena.close(original).unwrap();
        assert!(arena.frame_object_resources(copied).unwrap().is_some());
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend()
                .live_textures(),
            1
        );
        arena.close(copy).unwrap();
        assert!(arena.frame_object_resources(created).unwrap().is_some());
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend()
                .live_textures(),
            1
        );
        arena.release(created).unwrap();
        assert_eq!(
            arena
                .loaded_visual_resources()
                .unwrap()
                .backend()
                .live_textures(),
            0
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn billboard_bound_cubes_only_bbox_and_preserves_mesh_sphere() {
        let source = crate::mesh_bounds::MeshBoundVolume {
            bbox: [[-2., -4., 1.], [3., 6., 9.]],
            center: [4., 5., 6.],
            radius: 17.,
        };
        let cube = cube_local_bound(source);
        assert_eq!(cube.bbox, [[-4.; 3], [9.; 3]]);
        assert_eq!(cube.center, source.center);
        assert_eq!(cube.radius, source.radius);
        assert_eq!(
            cube_local_bound(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR),
            crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR
        );
    }

    #[test]
    fn world_space_billboard_retains_constructor_visual_bound_after_mesh_admission() {
        let path = std::env::temp_dir().join(format!(
            "lhr-billboard-world-space-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir_all(&path).unwrap();
        let installation = lhr_assets::Installation::open(&path).unwrap();
        let (mut source, suffix) = source();
        let source_mut = Arc::make_mut(&mut source);
        source_mut.frames[0].render_flags = 0x40;
        source_mut.materials[0].texture.clear();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        arena
            .load_parsed_billboards_into_model(model, source, &suffix)
            .unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        let frame = arena.model(model).unwrap().frames[0];
        assert_ne!(arena.state(frame).unwrap().visual_flags & 0x40, 0);
        let object = arena.frame(frame).unwrap().object_resources.unwrap();
        let mesh_bound = arena
            .loaded_visual_resources()
            .unwrap()
            .object_bounds(object)
            .unwrap();
        assert_eq!(mesh_bound.bbox, [[-2., -1., 0.], [2., 1., 0.]]);
        assert_eq!(
            arena.frame(frame).unwrap().visual_bounds,
            Some(crate::mesh_bounds::MeshBoundVolume::CONSTRUCTOR)
        );
        arena.close(model).unwrap();
        assert!(arena.frame(frame).is_err());
        arena.release(model.root()).unwrap();
        assert_eq!(arena.live_frame_count(), 0);
        std::fs::remove_dir_all(path).unwrap();
    }
}
