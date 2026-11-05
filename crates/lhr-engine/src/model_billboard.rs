//! Live I3D_billboard frame state and inherited bound projection.
//! PC LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6:
//! ctor 0x10005db0, LoadImage 0x100094d0, ProductBillMatrix 0x10006090.
// Port symbols below are exercised by their pinning tests and the rewrite
// overlay; render-pass callers land separately. Silence lib-build dead-code
// noise without weakening any lint elsewhere.
#![allow(dead_code)]
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

/// LS3DF e211c850… RVA 0x5db0 (differential: Ghidra body 62 B/1 BB/13 insn;
/// mafia-re I3d_billboard.cpp:19-25). Constructor words: visual-type BBRD
/// 0x44524242, rot_axis 2 (u32 dword store), axis_mode 0 (u8 low byte of
/// dword-cleared slot), env_dummy null. Re-affirms
/// `BillboardOrientation::default` + `BillboardFrame::default`; no new type.
pub(crate) fn billboard_ctor_orientation() -> BillboardOrientation {
    debug_assert_eq!(BillboardOrientation::default().rot_axis, 2);
    debug_assert_eq!(BillboardOrientation::default().axis_mode, 0);
    BillboardOrientation::default()
}

/// LS3DF e211c850… RVA 0x5e10 (differential: 21 B/1 BB/3 insn;
/// I3d_billboard.cpp:31-33). Empty derived tail (vtable restore + base-dtor
/// tailjump); base I3D_object teardown owns LOD/mesh release. Helper: no
/// function credit beyond the empty-tail contract.
pub(crate) fn billboard_derived_drop_is_empty() -> bool {
    true
}

/// LS3DF e211c850… RVA 0x5e40 (contract: no Ghidra body; entry bytes only;
/// I3d_billboard.cpp:42-61). Flag word VISF_BOUND_VALID 0x20000 is set
/// first; the mesh-valid branch cubes the local AABB (reuse
/// `cube_local_bound`) and preserves the mesh sphere.
pub(crate) const BILLBOARD_BOUND_VALID: u32 = 0x20000;
pub(crate) fn billboard_update_bound_guard_words() -> u32 {
    BILLBOARD_BOUND_VALID
}

/// LS3DF e211c850… RVA 0x5f40 (contract: no Ghidra body; entry bytes only;
/// I3d_billboard.cpp:65-69). Effect order: object SetMesh resource prefix
/// THEN UpdateBoundProc cube. The caller runs the mesh-compat path then
/// `cube_local_bound`; this helper records the order only. Helper: no credit.
pub(crate) const BILLBOARD_SET_MESH_ORDER: [&str; 2] = ["object_set_mesh", "update_bound_cube"];

/// LS3DF e211c850… RVA 0x5f60 (differential: 291 B/24 BB/88 insn;
/// I3d_billboard.cpp:73-104). Null/self -> I3DERR_INVALIDPARAMS 0x80000007
/// (first, before any effect). Non-visual source: emitor arm shares mesh
/// with keep_lods=true, else OK. Visual source: vis_flags masked with
/// 0x2ffd0 (== DUPLICATE_VIS_MASK), 10-dword bound copy, world-space branch,
/// BBRD orientation copy, then SetMesh(shared, keep_lods=false). Widths:
/// rot_axis u32, axis_mode u8.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BillboardDuplicateError {
    InvalidParams,
    UnsupportedVisual(u32),
}
pub(crate) fn billboard_duplicate_guard(
    dst_is_src_or_null: bool,
    src_is_visual: bool,
    src_is_emitor: bool,
    src_visual_type: u32,
) -> Result<bool, BillboardDuplicateError> {
    if dst_is_src_or_null {
        return Err(BillboardDuplicateError::InvalidParams);
    }
    if !src_is_visual {
        return Ok(src_is_emitor);
    }
    const BBRD: u32 = 0x44524242;
    // Admit set == the Duplicate switch fallthrough (I3d_billboard.cpp:90-101
    // over I3D.h:85-98): BBRD copies orientation, OBJECT/LIT/SNGM/LENS/MORPH/
    // SMMR share the mesh; anything else is I3DERR_GENERIC.
    const ADMITTED: [u32; 7] = [
        BBRD, 0x50414D4C, 0x5F4A424F, 0x4850524D, 0x4D474E53, 0x524D4D53, 0x534E454C,
    ];
    if ADMITTED.contains(&src_visual_type) {
        Ok(false)
    } else {
        Err(BillboardDuplicateError::UnsupportedVisual(src_visual_type))
    }
}
/// Mask + orientation copy half of 0x5f60 (frame Duplicate prefix + mesh
/// share stay with their owners). Reuses DUPLICATE_VIS_MASK + orientation.
pub(crate) fn billboard_duplicate_fields(
    dst_flags: &mut u32,
    dst_orientation: &mut BillboardOrientation,
    src_flags: u32,
    src_orientation: BillboardOrientation,
    src_visual_type: u32,
) {
    *dst_flags = src_flags & crate::i3d_object::DUPLICATE_VIS_MASK;
    if src_visual_type == 0x44524242 {
        *dst_orientation = src_orientation;
    }
}

/// Material flag words (I3D.h:3855-3870, evidenced values — not the local
/// guesses of an earlier draft). Sort words (I3D.h:4578-4581).
pub(crate) const BILLBOARD_MATF_TEXTURE: u32 = 0x40000;
pub(crate) const BILLBOARD_MATF_TXT_ALPHA: u32 = 0x40000000;
pub(crate) const BILLBOARD_MATF_ADD_MODE: u32 = 0x80000000;
pub(crate) const BILLBOARD_MATF_CKEY_ALPHA: u32 = 0x20000000;
pub(crate) const BILLBOARD_SORT_ALPHA: u32 = 0x80000000;
pub(crate) const BILLBOARD_SORT_DIST_MASK: u32 = 0x3fffffff;
pub(crate) const BILLBOARD_SORT_MAT_MASK: u32 = 0x7fff;
pub(crate) const BILLBOARD_SORT_CKEY_ALPHA: u32 = 0x40000000;

/// 0x73c0 sort-word classification (I3d_billboard.cpp:554-567), pure.
/// The transparent arm carries the distance bits
/// (`(FloatToInt(sort_dist) & MASK) | ALPHA`); the signature takes the raw
/// distance so no caller can drop them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BillboardSortClass {
    OpaqueZero,
    TransparentAlpha,
    KeyedOrPlain(u32),
}
pub(crate) fn billboard_sort_word(
    has_material: bool,
    has_texture: bool,
    texture_uid: u32,
    mat_flags: u32,
    mat_alpha_times_fade: f32,
    sort_dist: f32,
) -> (BillboardSortClass, u32) {
    // Native FloatToInt chop toward zero; out-of-range saturation is the
    // disclosed host hardening (native wraps), bit-exact in range.
    fn trunc_u32(v: f32) -> u32 {
        v.trunc() as u32
    }
    if !has_material || (!has_texture && mat_alpha_times_fade >= 1.0) {
        return (BillboardSortClass::OpaqueZero, 0);
    }
    if mat_flags & (BILLBOARD_MATF_TXT_ALPHA | BILLBOARD_MATF_ADD_MODE) != 0
        || mat_alpha_times_fade < 1.0
    {
        return (
            BillboardSortClass::TransparentAlpha,
            (trunc_u32(sort_dist) & BILLBOARD_SORT_DIST_MASK) | BILLBOARD_SORT_ALPHA,
        );
    }
    if mat_flags & BILLBOARD_MATF_CKEY_ALPHA != 0 {
        (
            BillboardSortClass::KeyedOrPlain(texture_uid),
            (texture_uid & BILLBOARD_SORT_MAT_MASK) | BILLBOARD_SORT_CKEY_ALPHA,
        )
    } else {
        (
            BillboardSortClass::KeyedOrPlain(texture_uid),
            texture_uid & BILLBOARD_SORT_MAT_MASK,
        )
    }
}

/// 0x73c0 proxy order log (I3d_billboard.cpp:352-373): copy axis words,
/// resolve sector up the parent chain, recurse, remap new fgroups' vis,
/// then ProductBillMatrix. Backend trait owns sector/fgroup storage.
pub(crate) const BILLBOARD_PROXY_ORDER: [&str; 4] = [
    "copy_axis_words",
    "resolve_sector",
    "recurse_remap_vis",
    "product_bill_matrix",
];

/// 0x83b0 draw contract (I3d_billboard.cpp:580-842). Cull set once
/// (proxy NONE else material two-sided?NONE:CCW); rot_axis==4 suppresses
/// ZWRITE for the draw and restores after; reset mask bit0 TEXCOORDINDEX,
/// bit1 ADDRESSU, bit2 ADDRESSV restored in that order.
pub(crate) fn billboard_cull_word(is_proxy: bool, two_sided: bool) -> u32 {
    const D3DCULL_NONE: u32 = 1;
    const D3DCULL_CCW: u32 = 3;
    if is_proxy || two_sided {
        D3DCULL_NONE
    } else {
        D3DCULL_CCW
    }
}
pub(crate) fn billboard_zwrite_suppressed(rot_axis: u32) -> bool {
    rot_axis == 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn y5_ctor_load_words_widths_and_corrections() {
        assert_eq!(
            billboard_ctor_orientation(),
            BillboardOrientation::default()
        );
        assert_eq!(
            BillboardOrientation::from_source(2, 0),
            BillboardOrientation::default()
        );
        assert_eq!(
            BillboardOrientation::from_source(3, 0),
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 2
            }
        );
        assert_eq!(
            BillboardOrientation::from_source(1, 4),
            BillboardOrientation {
                rot_axis: 1,
                axis_mode: 1
            }
        );
        assert_eq!(BILLBOARD_BOUND_VALID, 0x20000);
        assert_eq!(
            BILLBOARD_SET_MESH_ORDER,
            ["object_set_mesh", "update_bound_cube"]
        );
        assert!(billboard_derived_drop_is_empty());
    }
    #[test]
    fn y5_duplicate_guards_mask_orientation_and_order() {
        use crate::i3d_object::DUPLICATE_VIS_MASK;
        assert_eq!(DUPLICATE_VIS_MASK, 0x2_ffd0);
        assert_eq!(
            billboard_duplicate_guard(true, true, false, 0x44524242),
            Err(BillboardDuplicateError::InvalidParams)
        );
        assert_eq!(billboard_duplicate_guard(false, false, true, 0), Ok(true));
        assert_eq!(billboard_duplicate_guard(false, false, false, 0), Ok(false));
        assert_eq!(
            billboard_duplicate_guard(false, true, false, 0x44524242),
            Ok(false)
        );
        assert_eq!(
            billboard_duplicate_guard(false, true, false, 0xDEAD_BEEF),
            Err(BillboardDuplicateError::UnsupportedVisual(0xDEAD_BEEF))
        );
        let (mut f, mut o) = (0xFFFF_FFFFu32, BillboardOrientation::default());
        billboard_duplicate_fields(
            &mut f,
            &mut o,
            0x1234_5678,
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 1,
            },
            0x44524242,
        );
        assert_eq!(f, 0x1234_5678 & DUPLICATE_VIS_MASK);
        assert_eq!(
            o,
            BillboardOrientation {
                rot_axis: 0,
                axis_mode: 1
            }
        );
        let (mut f2, mut o2) = (0, BillboardOrientation::default());
        billboard_duplicate_fields(
            &mut f2,
            &mut o2,
            0xFFFF_FFFF,
            BillboardOrientation {
                rot_axis: 1,
                axis_mode: 3,
            },
            0x50414D4c,
        );
        assert_eq!(f2, DUPLICATE_VIS_MASK);
        assert_eq!(o2, BillboardOrientation::default());
        // OBJECT fourcc admitted (was a 0x50414D4D typo for 0x5F4A424F).
        assert_eq!(
            billboard_duplicate_guard(false, true, false, 0x5F4A424F),
            Ok(false)
        );
    }
    #[test]
    fn y5_queue_render_contract_words_and_order() {
        assert_eq!(
            BILLBOARD_PROXY_ORDER,
            [
                "copy_axis_words",
                "resolve_sector",
                "recurse_remap_vis",
                "product_bill_matrix"
            ]
        );
        assert_eq!(billboard_cull_word(true, false), 1);
        assert_eq!(billboard_cull_word(false, false), 3);
        assert_eq!(billboard_cull_word(false, true), 1);
        assert!(billboard_zwrite_suppressed(4));
        assert!(!billboard_zwrite_suppressed(2));
        let (cls, w) = billboard_sort_word(false, false, 0, 0, 1.0, 0.0);
        assert_eq!((cls, w), (BillboardSortClass::OpaqueZero, 0));
        // Transparent arm carries the distance bits (I3d_billboard.cpp:557-559).
        let (_, w2) = billboard_sort_word(true, true, 7, BILLBOARD_MATF_TXT_ALPHA, 1.0, 100.0);
        assert_eq!(
            w2,
            (100u32 & BILLBOARD_SORT_DIST_MASK) | BILLBOARD_SORT_ALPHA
        );
        let (_, w2b) = billboard_sort_word(true, false, 0, 0, 0.5, 250.0);
        assert_eq!(
            w2b,
            (250u32 & BILLBOARD_SORT_DIST_MASK) | BILLBOARD_SORT_ALPHA
        );
        let (_, w3) = billboard_sort_word(true, true, 7, BILLBOARD_MATF_CKEY_ALPHA, 1.0, 0.0);
        assert_eq!(
            w3,
            (7 & BILLBOARD_SORT_MAT_MASK) | BILLBOARD_SORT_CKEY_ALPHA
        );
    }
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
