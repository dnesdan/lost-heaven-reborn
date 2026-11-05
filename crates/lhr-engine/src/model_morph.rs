//! Bounded local-space I3D_morph owner. Pinned mafia-re eb5a921; PC LS3DF
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! REA corroborates the morphBase constructor/Clear/LoadImageB and MRPH
//! constructor; virtual entries retain vtable/instruction evidence but unresolved
//! Ghidra function boundaries. No whole-function or original-runtime credit.
//! Source Arc<Model> retains the shared immutable MorphDef; each live frame owns
//! raw weights. The existing visual resource owner separately owns the shared
//! canonical mesh and each visual's retained render LOD positions/update count.
//! Port symbols below are exercised by their pinning tests and the rewrite
//! overlay; render-pass callers land separately. Silence lib-build dead-code
//! noise without weakening any lint elsewhere.
#![allow(dead_code)]
use super::*;
use crate::singlemorph_state::{SinglemorphError, SinglemorphState};

#[derive(Clone, Debug)]
pub(crate) struct MorphFrame {
    pub(super) weights: SinglemorphState,
}
impl MorphFrame {
    /// I3D_morph working-label dtor, morph-owned prefix (RVA 0x34eb0):
    /// delegates to the free-function entry point below.
    pub(super) fn morph_teardown_order(&mut self, order: &mut Vec<&'static str>) -> usize {
        morph_teardown_order(&mut self.weights, order)
    }
    pub(super) fn load(record: &lhr_assets::MorphRecord) -> Result<Self, ModelError> {
        Ok(Self {
            weights: SinglemorphState::from_record(record).map_err(ModelError::Morph)?,
        })
    }
    pub(super) fn duplicate(&self) -> Result<Self, ModelError> {
        Ok(Self {
            weights: self
                .weights
                .duplicate_morph_def()
                .map_err(ModelError::Morph)?,
        })
    }
}
/// I3D_morph working-label ctor words (LS3DF RVA 0x34e30). Ghidra
/// FUN_10034e30 (17 insn/1 BB): parent ctor (caller) → six zero words at
/// +0x220..0x234, flags 5 at +0x21c, vtables, magic. The morphBase prefix
/// reuses `SinglemorphState::new()`.
pub const MORPH_MAGIC: u32 = 0x4850_524d; // LE bytes "MRPH"
                                          // Flag words reuse the evidenced VISF_ names (I3D.h:2688,2702) instead of
                                          // module-local aliases: `crate::frame_mirror::VISF_WBOUND_VALID` (0x40000),
                                          // `crate::projector_pass::VISF_WORLD_SPACE` (0x40).

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MorphCtorFields {
    pub flags: u32,
    pub reserved: [u32; 6],
    pub magic: u32,
}

impl MorphCtorFields {
    pub fn morph_ctor_fields() -> Self {
        morph_ctor_fields()
    }
}

/// Free-function entry point for the ctor words so the overlay symbol
/// resolves literally; the associated function delegates here (single
/// implementation, no drift).
pub fn morph_ctor_fields() -> MorphCtorFields {
    MorphCtorFields {
        flags: 5,
        reserved: [0; 6],
        magic: MORPH_MAGIC,
    }
}

/// I3D_morph working-label dtor, morph-owned prefix (RVA 0x34eb0).
/// Ghidra FUN_10034eb0 (99 insn/11 BB): vtable restores → Clear-equivalent
/// (flags `(f&!2)|1`, counts/weights dropped) → parent dtor tail (caller).
/// Logs the observable prefix order and returns freed weight bytes.
/// Free-function entry point so the overlay symbol resolves literally;
/// `MorphFrame::morph_teardown_order` delegates here. Takes the weight state
/// directly (keeps `MorphFrame` crate-private).
pub fn morph_teardown_order(weights: &mut SinglemorphState, order: &mut Vec<&'static str>) -> usize {
    order.push("morph-clear");
    weights.clear_morph()
}

/// I3D_morph::SetMesh / LoadImage shared tail (RVAs 0x35130/0x36750,
/// contract — no Ghidra body). After the caller's mesh-compat gate and
/// 10-dword bound copy: `if (!(flags & 0x40)) flags &= ~0x40000`.
pub fn morph_set_mesh_tail(visual_flags: &mut u32, world_space: bool) {
    if !world_space {
        *visual_flags &= !crate::frame_mirror::VISF_WBOUND_VALID;
    }
}

/// MorphDef working-label ctor words (RVA 0x36580; name is mafia-re's).
/// Ghidra FUN_10036580 (87 insn/8 BB): refcount 1; signed `JLE` gates
/// (widths are `i32`); bbox invalidate words; zero sphere. Allocation
/// itself stays caller-side. Words carried as bits, no float arithmetic
/// (decimals are comments only).
pub const MORPH_BBOX_MIN_WORD: u32 = 0x5A0E_1BCA; // 1.0000000272564224e16f32
pub const MORPH_BBOX_MAX_WORD: u32 = 0xDA0E_1BCA; // -1.0000000272564224e16f32

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MorphDefFields {
    pub ref_count: i32,
    pub num_lods: i32,
    pub num_regions: i32,
    pub bbox_min_words: [u32; 3],
    pub bbox_max_words: [u32; 3],
    pub sphere_pos_words: [u32; 3],
    pub sphere_radius_word: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphDefCtorError {
    InvalidParameter,
}

pub fn morph_def_ctor_fields(lods: i32, regs: i32) -> Result<MorphDefFields, MorphDefCtorError> {
    if lods <= 0 || regs <= 0 {
        return Err(MorphDefCtorError::InvalidParameter);
    }
    Ok(MorphDefFields {
        ref_count: 1,
        num_lods: lods,
        num_regions: regs,
        bbox_min_words: [MORPH_BBOX_MIN_WORD; 3],
        bbox_max_words: [MORPH_BBOX_MAX_WORD; 3],
        sphere_pos_words: [0; 3],
        sphere_radius_word: 0,
    })
}

/// MorphDef::Release state machine (RVA 0x366b0). Ghidra FUN_100366b0
/// (36 insn/4 BB): `DEC; JNS` → negative logs + 0; zero deletes + 0; else
/// retained count. Deletion itself stays caller-side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MorphDefRelease {
    LoggedInvalid,
    Freed,
    Retained(i32),
}

pub fn morph_def_release(ref_count: i32) -> (i32, MorphDefRelease) {
    let next = ref_count.wrapping_sub(1);
    if next < 0 {
        (next, MorphDefRelease::LoggedInvalid)
    } else if next == 0 {
        (next, MorphDefRelease::Freed)
    } else {
        (next, MorphDefRelease::Retained(next))
    }
}

/// morph_region working-label dtor order (RVA 0x36720). Ghidra FUN_10036720
/// (14 insn/2 BB): `delete[]` indices, then null-guarded `_aligned_free`
/// verts. Both frees are caller/backend effects; this pins their order.
pub fn morph_region_teardown_order(order: &mut Vec<&'static str>) {
    order.push("delete-indices");
    order.push("aligned-free-verts");
}

impl ModelArena {
    pub fn morph_level_count(&self, key: FrameKey) -> Result<usize, ModelError> {
        Ok(self
            .frame(key)?
            .morph
            .as_ref()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?
            .weights
            .level_count())
    }
    pub fn morph_region_count(&self, key: FrameKey) -> Result<usize, ModelError> {
        Ok(self
            .frame(key)?
            .morph
            .as_ref()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?
            .weights
            .region_count())
    }
    /// I3D_morph::GetWeight's checked signed indices; invalid indices yield zero.
    pub fn morph_weight(&self, key: FrameKey, region: i32, level: i32) -> Result<f32, ModelError> {
        let weights = &self
            .frame(key)?
            .morph
            .as_ref()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?
            .weights;
        if region < 0
            || level < 0
            || region as usize >= weights.region_count()
            || level as usize >= weights.level_count()
        {
            return Ok(0.);
        }
        Ok(weights.weights()[region as usize * weights.level_count() + level as usize])
    }
    /// SetWeight's successful prefix: wrapping frame update count, raw weight
    /// store/dirty bit, then world-bound invalidation. No clamp/finite gate.
    pub fn set_morph_weight(
        &mut self,
        key: FrameKey,
        region: i32,
        level: i32,
        weight: f32,
    ) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        let morph = self.frames[fi]
            .morph
            .as_ref()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        if region < 0
            || level < 0
            || region as usize >= morph.weights.region_count()
            || level as usize >= morph.weights.level_count()
        {
            return Err(ModelError::Morph(SinglemorphError::WeightIndex));
        }
        let state = self.graph.projected_state_mut(key)?;
        state.update_count = state.update_count.wrapping_add(1);
        self.frames[fi]
            .morph
            .as_mut()
            .unwrap()
            .weights
            .set_weight(region as usize, level as usize, weight)
            .map_err(ModelError::Morph)?;
        self.graph.projected_state_mut(key)?.visual_flags &= !0x40000;
        self.frames[fi].hr_world_bounds_error = None;
        Ok(())
    }
    /// Called only after drawable selected geometry, before lighting reads it.
    /// PC's one global dirty gate is intentional: selecting a different LOD
    /// does not invent a new dirty bit or restore unmorphed source vertices.
    pub(crate) fn prepare_owned_selected_morph_lod(
        &mut self,
        key: FrameKey,
        lod: u32,
    ) -> Result<(), ModelError> {
        let fi = self.frame_index(key)?;
        let count = self.state(key)?.update_count;
        let frame = &self.frames[fi];
        let Some(morph) = &frame.morph else {
            return Ok(());
        };
        let object = frame
            .object_resources
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let source = frame
            .source
            .as_ref()
            .cloned()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let index = frame
            .source_index
            .ok_or(ModelError::VisualBoundsUnavailable(key))?;
        let record = source.frames[index]
            .morph
            .as_ref()
            .ok_or(ModelError::Morph(SinglemorphError::Definition))?;
        // Stage arithmetic and CPU publication; unsupported arithmetic retains
        // dirty and the old cache count, rather than publishing a false success.
        let mut weights = morph.weights.clone();
        self.loaded_visual_resources
            .as_mut()
            .ok_or(ModelError::VisualBoundsUnavailable(key))?
            .update_selected_morph_lod(object, lod, count, record, &mut weights)
            .map_err(ModelError::LoadedVisual)?;
        let morph = self.frames[fi].morph.as_mut().unwrap();
        morph.weights = weights;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::car_inspection::snapshot_arena_tree;
    use crate::i3d_object::ObjectVisualType;
    fn source() -> Arc<Model> {
        use lhr_assets::*;
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.visual = Some(5);
        frame.render_flags = 0;
        frame.logic_flags = 0;
        let triangle = [[-0.8, -0.5, 0.], [-0.2, -0.5, 0.], [-0.5, 0.5, 0.]];
        frame.lods = [0., 100.]
            .into_iter()
            .map(|distance| Lod {
                distance,
                uv_only: None,
                vertices: triangle
                    .map(|position| Vertex {
                        position,
                        normal: [0., 0., -1.],
                        uv: [0.25, 0.75],
                    })
                    .to_vec(),
                groups: vec![FaceGroup {
                    material: 0,
                    indices: vec![0, 1, 2],
                }],
            })
            .collect();
        frame.morph = Some(MorphRecord {
            target_count: 2,
            definition: Some(MorphDefinition {
                channel_count: 1,
                lods: (0..2)
                    .map(|lod| MorphLodRecord {
                        channels: vec![MorphChannelRecord {
                            vertex_count: 3,
                            index_mode: Some(1),
                            vertex_indices: vec![0, 1, 2],
                            samples: triangle
                                .iter()
                                .flat_map(|&p| {
                                    [
                                        MorphSampleRecord {
                                            position: [p[0], p[1], lod as f32 * 0.1],
                                            normal: [0., 0., -1.],
                                        },
                                        MorphSampleRecord {
                                            position: [p[0] + 1., p[1], lod as f32 * 0.1],
                                            normal: [0., 1., 0.],
                                        },
                                    ]
                                })
                                .collect(),
                        }],
                    })
                    .collect(),
                trailer_words: [-1., -1., -0.2, 1., 1., 0.2, 0., 0., 0., 1.5].map(f32::to_bits),
            }),
        });
        Arc::new(Model {
            frames: vec![frame],
            materials: vec![Material {
                flags: 0x10008000,
                ambient: [0.; 3],
                diffuse: [1., 0., 0.],
                texture: String::new(),
                emission: [0.; 3],
                opacity: 1.,
                reflection: None,
                environment_texture: None,
                alpha_texture: None,
                animation: None,
            }],
        })
    }
    fn loaded() -> (
        ModelArena,
        ModelKey,
        FrameKey,
        Arc<Model>,
        std::path::PathBuf,
    ) {
        let path = std::env::temp_dir().join(format!(
            "lhr-morph-{}-{}",
            std::process::id(),
            crate::frame_lookup::incarnation()
        ));
        std::fs::create_dir(&path).unwrap();
        let install = lhr_assets::Installation::open(&path).unwrap();
        let source = source();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let key = arena
            .load_parsed_into_model(model, source.clone())
            .unwrap()
            .frames()[0];
        arena.load_plain_visual_resources(model, &install).unwrap();
        arena.publish_runtime_buffers().unwrap();
        (arena, model, key, source, path)
    }
    fn vertex(arena: &ModelArena, key: FrameKey, lod: u32) -> [f32; 8] {
        let object = arena.frame_object_resources(key).unwrap().unwrap();
        let view = arena
            .loaded_visual_resources()
            .unwrap()
            .inspection_mesh_at_lod(object, lod, 10)
            .unwrap()
            .unwrap();
        std::array::from_fn(|i| {
            f32::from_le_bytes(view.vertex_bytes[i * 4..i * 4 + 4].try_into().unwrap())
        })
    }
    #[test]
    fn mrph_live_weights_selected_lod_and_cache_copy_own_real_meshes() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, model, key, source, path) = loaded();
            let object = arena.frame_object_resources(key).unwrap().unwrap();
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .objects()
                    .object(object)
                    .unwrap()
                    .visual_type(),
                ObjectVisualType::Morph
            );
            assert_eq!(arena.morph_level_count(key).unwrap(), 2);
            assert_eq!(arena.morph_region_count(key).unwrap(), 1);
            assert_eq!(arena.frame(key).unwrap().visual_bounds.unwrap().radius, 1.5);
            let old = arena.state(key).unwrap().update_count;
            assert!(arena.set_morph_weight(key, -1, 0, 2.).is_err());
            assert_eq!(arena.state(key).unwrap().update_count, old);
            arena.set_morph_weight(key, 0, 0, 0.).unwrap();
            arena.set_morph_weight(key, 0, 1, 1.).unwrap();
            assert_eq!(arena.state(key).unwrap().update_count, old.wrapping_add(2));
            assert_eq!(arena.morph_weight(key, 0, 1).unwrap(), 1.);
            assert_eq!(arena.morph_weight(key, 0, 9).unwrap(), 0.);
            arena.prepare_owned_selected_morph_lod(key, 1).unwrap();
            assert_eq!(vertex(&arena, key, 1)[0], 0.19999999);
            assert_eq!(&vertex(&arena, key, 1)[3..6], &[0., 1., 0.]);
            assert_eq!(&vertex(&arena, key, 1)[6..8], &[0.25, 0.75]);
            assert_eq!(vertex(&arena, key, 0)[0], -0.8);
            // PC dirty is global; moving to untouched LOD0 must not invent an
            // update or erase the already updated canonical LOD1 buffer.
            arena.prepare_owned_selected_morph_lod(key, 0).unwrap();
            assert_eq!(vertex(&arena, key, 0)[0], -0.8);
            assert_eq!(vertex(&arena, key, 1)[0], 0.19999999);
            let copy = arena.create_model().unwrap();
            arena.duplicate_pristine_cache_model(copy, model).unwrap();
            arena.publish_runtime_buffers().unwrap();
            let copied = arena.model(copy).unwrap().frames[0];
            assert!(Arc::ptr_eq(
                arena.frame(key).unwrap().source.as_ref().unwrap(),
                arena.frame(copied).unwrap().source.as_ref().unwrap()
            ));
            let copy_object = arena.frame_object_resources(copied).unwrap().unwrap();
            assert_ne!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .objects()
                    .object(object)
                    .unwrap()
                    .mesh,
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .objects()
                    .object(copy_object)
                    .unwrap()
                    .mesh
            );
            assert_eq!(arena.morph_weight(copied, 0, 1).unwrap(), 1.);
            assert!(arena
                .frame(copied)
                .unwrap()
                .morph
                .as_ref()
                .unwrap()
                .weights
                .normalized_weights()
                .is_none());
            assert_eq!(vertex(&arena, copied, 1)[0], 0.19999999);
            arena.set_morph_weight(copied, 0, 0, 1.).unwrap();
            arena.set_morph_weight(copied, 0, 1, 0.).unwrap();
            arena.prepare_owned_selected_morph_lod(copied, 1).unwrap();
            assert_eq!(vertex(&arena, copied, 1)[0], -0.8);
            assert_eq!(vertex(&arena, key, 1)[0], 0.19999999);
            assert_eq!(source.frames[0].lods[1].vertices[0].position[0], -0.8);
            arena.close(model).unwrap();
            assert_eq!(arena.morph_weight(copied, 0, 0).unwrap(), 1.);
            arena.close(copy).unwrap();
            arena
                .loaded_visual_resources_mut()
                .unwrap()
                .reclaim_unused_buffers()
                .unwrap();
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .backend()
                    .graphics_bytes(),
                0
            );
            std::fs::remove_dir(path).unwrap();
        });
    }
    #[test]
    fn mrph_file_instance_weights_write_the_shared_canonical_mesh() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let path = std::env::temp_dir().join(format!(
                "lhr-morph-alias-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&path).unwrap();
            let install = lhr_assets::Installation::open(&path).unwrap();
            let mut source = (*source()).clone();
            let mut instance = source.frames[0].clone();
            instance.lods.clear();
            instance.instance = Some(0);
            source.frames.push(instance);
            let mut arena = ModelArena::default();
            let model = arena.create_model().unwrap();
            let keys = arena
                .load_parsed_into_model(model, Arc::new(source))
                .unwrap()
                .frames()
                .to_vec();
            arena.load_plain_visual_resources(model, &install).unwrap();
            arena.publish_runtime_buffers().unwrap();
            let mesh = |arena: &ModelArena, key| {
                let object = arena.frame_object_resources(key).unwrap().unwrap();
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .objects()
                    .object(object)
                    .unwrap()
                    .mesh
            };
            assert_eq!(mesh(&arena, keys[0]), mesh(&arena, keys[1]));
            arena.set_morph_weight(keys[0], 0, 0, 0.).unwrap();
            arena.set_morph_weight(keys[0], 0, 1, 1.).unwrap();
            arena.prepare_owned_selected_morph_lod(keys[0], 0).unwrap();
            assert_eq!(vertex(&arena, keys[1], 0)[0], 0.19999999);
            arena.prepare_owned_selected_morph_lod(keys[1], 0).unwrap();
            assert_eq!(vertex(&arena, keys[0], 0)[0], -0.8);
            arena.prepare_owned_selected_morph_lod(keys[0], 0).unwrap();
            assert_eq!(vertex(&arena, keys[0], 0)[0], -0.8);
            arena.close(model).unwrap();
            arena
                .loaded_visual_resources_mut()
                .unwrap()
                .reclaim_unused_buffers()
                .unwrap();
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .backend()
                    .graphics_bytes(),
                0
            );
            std::fs::remove_dir(path).unwrap();
        });
    }
    #[test]
    fn mrph_unsupported_numeric_update_never_substitutes_source_geometry() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, model, key, _, path) = loaded();
            arena.set_morph_weight(key, 0, 0, 0.).unwrap();
            arena.set_morph_weight(key, 0, 1, 1.).unwrap();
            arena.prepare_owned_selected_morph_lod(key, 0).unwrap();
            let committed = vertex(&arena, key, 0);
            arena.set_morph_weight(key, 0, 1, f32::NAN).unwrap();
            assert!(arena.prepare_owned_selected_morph_lod(key, 0).is_err());
            assert_eq!(vertex(&arena, key, 0), committed);
            assert!(snapshot_arena_tree(&mut arena, model.root()).is_err());
            arena.set_morph_weight(key, 0, 1, 1.).unwrap();
            assert!(snapshot_arena_tree(&mut arena, model.root()).is_ok());
            arena.close(model).unwrap();
            std::fs::remove_dir(path).unwrap();
        });
    }
    fn alias_render_fixture(gpu: bool) {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let path = std::env::temp_dir().join(format!(
                "lhr-morph-render-alias-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&path).unwrap();
            let install = lhr_assets::Installation::open(&path).unwrap();
            let mut source = (*source()).clone();
            let mut instance = source.frames[0].clone();
            instance.lods.clear();
            instance.instance = Some(0);
            source.frames.push(instance);
            let mut arena = ModelArena::default();
            let model = arena.create_model().unwrap();
            let keys = arena
                .load_parsed_into_model(model, Arc::new(source))
                .unwrap()
                .frames()
                .to_vec();
            arena.load_plain_visual_resources(model, &install).unwrap();
            arena.publish_runtime_buffers().unwrap();
            arena.set_morph_weight(keys[0], 0, 0, 0.).unwrap();
            arena.set_morph_weight(keys[0], 0, 1, 1.).unwrap();
            let first = snapshot_arena_tree(&mut arena, model.root()).unwrap();
            let draw_position = |snapshot: &crate::car_inspection::CarInspectionScene, key| {
                let index = snapshot
                    .draw_frames
                    .iter()
                    .position(|&frame| frame == key)
                    .unwrap();
                snapshot.scene.meshes[index].vertices[0].position
            };
            assert_eq!(draw_position(&first, keys[0])[0], 0.19999999);
            assert_eq!(draw_position(&first, keys[1])[0], -0.8);
            let repeated = snapshot_arena_tree(&mut arena, model.root()).unwrap();
            for &key in &keys {
                assert_eq!(draw_position(&first, key), draw_position(&repeated, key));
            }
            let camera =
                crate::car_inspection::camera_from_arena_bounds(&mut arena, model.root()).unwrap();
            let mut renderer = gpu.then(|| {
                let adapter =
                    pollster::block_on(lhr_render::instance().request_adapter(&Default::default()))
                        .expect("morph alias GPU adapter");
                pollster::block_on(lhr_render::Renderer::with_adapter(
                    &adapter,
                    &first.scene,
                    wgpu::TextureFormat::Rgba8Unorm,
                    false,
                ))
                .unwrap()
            });
            let before = renderer
                .as_mut()
                .map(|r| r.readback(64, 64, &camera).unwrap());
            if let Some(renderer) = renderer.as_mut() {
                renderer
                    .replace_meshes(0..2, &repeated.scene.meshes)
                    .unwrap();
                assert!(
                    before.as_ref().unwrap() == &renderer.readback(64, 64, &camera).unwrap(),
                    "unchanged aliased draws must retain their pixels"
                );
            }
            arena.set_morph_weight(keys[0], 0, 0, 1.).unwrap();
            arena.set_morph_weight(keys[0], 0, 1, 0.).unwrap();
            let changed = snapshot_arena_tree(&mut arena, model.root()).unwrap();
            assert_eq!(draw_position(&changed, keys[0])[0], -0.8);
            assert_eq!(
                draw_position(&changed, keys[1]),
                draw_position(&first, keys[1])
            );
            // The aliased canonical mesh reflects A's new writes while B's
            // existing render positions survive without any weight change.
            assert_eq!(vertex(&arena, keys[1], 0)[0], -0.8);
            let again = snapshot_arena_tree(&mut arena, model.root()).unwrap();
            for &key in &keys {
                assert_eq!(draw_position(&changed, key), draw_position(&again, key));
            }
            if let Some(renderer) = renderer.as_mut() {
                renderer
                    .replace_meshes(0..2, &changed.scene.meshes)
                    .unwrap();
                let after = renderer.readback(64, 64, &camera).unwrap();
                assert!(
                    before.as_ref().unwrap() != &after,
                    "changing only A must change the actual aliased draw pixels"
                );
                renderer.replace_meshes(0..2, &again.scene.meshes).unwrap();
                assert!(
                    after == renderer.readback(64, 64, &camera).unwrap(),
                    "changed aliased draws must remain stable on repetition"
                );
            }
            arena.close(model).unwrap();
            arena.reclaim_unused_visual_buffers().unwrap();
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .backend()
                    .graphics_bytes(),
                0
            );
            std::fs::remove_dir(path).unwrap();
        });
    }
    #[test]
    fn mrph_aliased_draw_positions_survive_repeated_and_single_owner_weight_changes() {
        alias_render_fixture(false);
    }
    #[test]
    #[ignore = "requires a Vulkan adapter"]
    fn gpu_mrph_aliased_draws_retain_distinct_render_positions() {
        alias_render_fixture(true);
    }
    #[test]
    fn mrph_partial_region_updates_preserve_untouched_owned_render_positions() {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let path = std::env::temp_dir().join(format!(
                "lhr-morph-partial-alias-{}-{}",
                std::process::id(),
                crate::frame_lookup::incarnation()
            ));
            std::fs::create_dir(&path).unwrap();
            let install = lhr_assets::Installation::open(&path).unwrap();
            let mut source = (*source()).clone();
            let mut instance = source.frames[0].clone();
            instance.lods.clear();
            instance.instance = Some(0);
            source.frames.push(instance);
            for (index, frame) in source.frames.iter_mut().enumerate() {
                for lod in &mut frame
                    .morph
                    .as_mut()
                    .unwrap()
                    .definition
                    .as_mut()
                    .unwrap()
                    .lods
                {
                    let channel = &mut lod.channels[0];
                    channel.samples = channel.samples[index * 2..index * 2 + 2].to_vec();
                    channel.vertex_count = 1;
                    channel.vertex_indices = vec![index as u16];
                }
            }
            let mut arena = ModelArena::default();
            let model = arena.create_model().unwrap();
            let keys = arena
                .load_parsed_into_model(model, Arc::new(source))
                .unwrap()
                .frames()
                .to_vec();
            arena.load_plain_visual_resources(model, &install).unwrap();
            arena.publish_runtime_buffers().unwrap();
            for &key in &keys {
                arena.set_morph_weight(key, 0, 0, 0.).unwrap();
                arena.set_morph_weight(key, 0, 1, 1.).unwrap();
                arena.prepare_owned_selected_morph_lod(key, 0).unwrap();
            }
            let owned_position = |arena: &ModelArena, key, index| {
                let object = arena.frame_object_resources(key).unwrap().unwrap();
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .morph_render_positions(object, 0)
                    .unwrap()
                    .position(index)
                    .unwrap()
            };
            let untouched = owned_position(&arena, keys[0], 1);
            assert_eq!(untouched[0], -0.2);
            assert_eq!(owned_position(&arena, keys[1], 1)[0], 0.8);
            arena.set_morph_weight(keys[0], 0, 0, 1.).unwrap();
            arena.set_morph_weight(keys[0], 0, 1, 0.).unwrap();
            arena.prepare_owned_selected_morph_lod(keys[0], 0).unwrap();
            assert_eq!(owned_position(&arena, keys[0], 1), untouched);
            assert_eq!(owned_position(&arena, keys[1], 1)[0], 0.8);
            let object = arena.frame_object_resources(keys[0]).unwrap().unwrap();
            let view = arena
                .loaded_visual_resources()
                .unwrap()
                .inspection_mesh_at_lod(object, 0, 10)
                .unwrap()
                .unwrap();
            assert_eq!(
                f32::from_le_bytes(view.vertex_bytes[32..36].try_into().unwrap()),
                0.8
            );
            arena.close(model).unwrap();
            arena.reclaim_unused_visual_buffers().unwrap();
            assert_eq!(
                arena
                    .loaded_visual_resources()
                    .unwrap()
                    .backend()
                    .graphics_bytes(),
                0
            );
            std::fs::remove_dir(path).unwrap();
        });
    }
    fn render_fixture(gpu: bool) {
        crate::native_precision::Pc24PrecisionState::default().run(|| {
            let (mut arena, model, key, source, path) = loaded();
            let first = snapshot_arena_tree(&mut arena, model.root()).unwrap();
            assert_eq!(first.scene.meshes[0].vertices[0].position[0], -0.8);
            let camera =
                crate::car_inspection::camera_from_arena_bounds(&mut arena, model.root()).unwrap();
            let mut renderer = gpu.then(|| {
                let instance = lhr_render::instance();
                let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
                    .expect("morph GPU adapter");
                pollster::block_on(lhr_render::Renderer::with_adapter(
                    &adapter,
                    &first.scene,
                    wgpu::TextureFormat::Rgba8Unorm,
                    false,
                ))
                .unwrap()
            });
            let before = renderer
                .as_mut()
                .map(|r| r.readback(64, 64, &camera).unwrap());
            arena.set_morph_weight(key, 0, 0, 0.).unwrap();
            arena.set_morph_weight(key, 0, 1, 1.).unwrap();
            let second = snapshot_arena_tree(&mut arena, model.root()).unwrap();
            assert_eq!(second.scene.meshes[0].vertices[0].position[0], 0.19999999);
            assert_eq!(source.frames[0].lods[0].vertices[0].position[0], -0.8);
            if let Some(renderer) = renderer.as_mut() {
                renderer.replace_meshes(0..1, &second.scene.meshes).unwrap();
                let after = renderer.readback(64, 64, &camera).unwrap();
                assert!(
                    after
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .any(|p| *p != [6, 9, 14, 255]),
                    "morphed triangle must remain visible"
                );
                assert!(
                    before.as_ref().unwrap() != &after,
                    "live morph must change rendered pixels"
                );
            }
            arena.close(model).unwrap();
            std::fs::remove_dir(path).unwrap();
        });
    }
    #[test]
    fn mrph_snapshot_reads_changed_canonical_vertices() {
        render_fixture(false);
    }
    #[test]
    #[ignore = "requires a Vulkan adapter"]
    fn gpu_mrph_live_weights_change_rendered_pixels() {
        render_fixture(true);
    }

    #[test]
    fn morph_ctor_publishes_mrph_and_morphbase() {
        let f = MorphCtorFields::morph_ctor_fields();
        assert_eq!(MORPH_MAGIC.to_le_bytes(), *b"MRPH");
        assert_eq!(f.magic, 0x4850_524d);
        assert_eq!(f.flags, 5);
        assert_eq!(f.reserved, [0; 6]);
        let w = SinglemorphState::new();
        assert_eq!((w.flags(), w.level_count(), w.region_count()), (5, 0, 0));
    }
    #[test]
    fn morph_teardown_clears_before_parent_tail() {
        let (mut arena, _model, key, _source, _path) = loaded();
        let fi = arena.frame_index(key).unwrap();
        let mut order = Vec::new();
        let freed = arena.frames[fi]
            .morph
            .as_mut()
            .unwrap()
            .morph_teardown_order(&mut order);
        assert!(freed >= 8);
        order.push("parent-tail"); // CALL 0x10036840 equivalent, caller-side
        assert_eq!(order, ["morph-clear", "parent-tail"]);
        assert_eq!(arena.frames[fi].morph.as_ref().unwrap().weights.flags(), 1);
    }
    #[test]
    fn morphdef_ctor_words_and_invalid_arm() {
        let f = morph_def_ctor_fields(2, 3).unwrap();
        assert_eq!((f.ref_count, f.num_lods, f.num_regions), (1, 2, 3));
        assert_eq!(f.bbox_min_words, [0x5A0E_1BCA; 3]);
        assert_eq!(f.bbox_max_words, [0xDA0E_1BCA; 3]);
        assert_eq!(f.sphere_pos_words, [0; 3]);
        assert_eq!(f.sphere_radius_word, 0);
        assert_eq!(
            f32::from_bits(MORPH_BBOX_MIN_WORD) as f64,
            1.0000000272564224e16
        );
        assert!(morph_def_ctor_fields(0, 3).is_err());
        assert!(morph_def_ctor_fields(-1, -1).is_err()); // signed JLE gates
    }
    #[test]
    fn morphdef_release_arms() {
        assert_eq!(morph_def_release(1), (0, MorphDefRelease::Freed));
        assert_eq!(morph_def_release(2), (1, MorphDefRelease::Retained(1)));
        assert_eq!(morph_def_release(0), (-1, MorphDefRelease::LoggedInvalid));
        assert_eq!(morph_def_release(-5), (-6, MorphDefRelease::LoggedInvalid));
        assert_eq!(
            morph_def_release(i32::MIN),
            (i32::MAX, MorphDefRelease::Retained(i32::MAX))
        ); // DEC wraps MIN->MAX; JNS tests the wrapped result (non-negative)
    }
    #[test]
    fn morph_region_teardown_frees_indices_before_verts() {
        let mut order = Vec::new();
        morph_region_teardown_order(&mut order);
        assert_eq!(order, ["delete-indices", "aligned-free-verts"]);
    }
    #[test]
    fn morph_set_mesh_tail_clears_bound_flag_unless_world_space() {
        use crate::frame_mirror::VISF_WBOUND_VALID;
        use crate::projector_pass::VISF_WORLD_SPACE;
        let mut flags = VISF_WBOUND_VALID | VISF_WORLD_SPACE;
        morph_set_mesh_tail(&mut flags, false);
        assert_eq!(flags & VISF_WBOUND_VALID, 0);
        let mut flags = VISF_WBOUND_VALID | VISF_WORLD_SPACE;
        morph_set_mesh_tail(&mut flags, true);
        assert_eq!(flags & VISF_WBOUND_VALID, VISF_WBOUND_VALID);
    }
    #[test]
    fn duplicate_reports_hrwords_and_clones_raw_weights() {
        const I3DERR_GENERIC: i32 = 0x8000_0005u32 as i32;
        const I3DERR_OUTOFMEM: i32 = 0x8000_0002u32 as i32;
        assert!(I3DERR_GENERIC < 0 && I3DERR_OUTOFMEM < 0); // native `r < 0` arms
        let empty = SinglemorphState::new(); // unloaded: no def
        assert!(empty.duplicate_morph_def().is_err()); // maps to GENERIC arm
                                                       // Loaded source clones raw weights with normalized dropped.
        let (arena, _model, key, _source, _path) = loaded();
        let weights = &arena.frame(key).unwrap().morph.as_ref().unwrap().weights;
        let dup = weights.duplicate_morph_def().unwrap();
        assert_eq!(dup.weights(), weights.weights());
        assert_eq!(dup.flags(), weights.flags() | 0x1);
        assert!(dup.normalized_weights().is_none());
    }
    #[test]
    fn load_image_b_success_words() {
        let (arena, _model, key, _source, _path) = loaded();
        let weights = &arena.frame(key).unwrap().morph.as_ref().unwrap().weights;
        assert_eq!(weights.flags() & 0x4, 0); // UNKNOWN_4 cleared (AND 0xFFFFFFFB)
        assert_eq!(weights.flags() & 0x2, 0x2); // LOADED set (OR 2)
        assert_eq!(weights.weights(), [1.0, 0.0]); // 0x3F800000 level-0 init
    }
}
