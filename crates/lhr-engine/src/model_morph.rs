//! Bounded local-space I3D_morph owner. Pinned mafia-re eb5a921; PC LS3DF
//! e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6.
//! REA corroborates the morphBase constructor/Clear/LoadImageB and MRPH
//! constructor; virtual entries retain vtable/instruction evidence but unresolved
//! Ghidra function boundaries. No whole-function or original-runtime credit.
//! Source Arc<Model> retains the shared immutable MorphDef; each live frame owns
//! raw weights. The existing visual resource owner separately owns the shared
//! canonical mesh and each visual's retained render LOD positions/update count.
use super::*;
use crate::singlemorph_state::{SinglemorphError, SinglemorphState};

#[derive(Clone, Debug)]
pub(crate) struct MorphFrame {
    pub(super) weights: SinglemorphState,
}
impl MorphFrame {
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
}
