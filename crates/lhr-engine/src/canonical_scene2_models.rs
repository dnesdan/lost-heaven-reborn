//! Correspondence for ordinary external scene2 models in canonical driving.
//! This adapter owns no frames. The completed source record offset identifies
//! an instance; names are never used to infer ownership. Mutations borrow the
//! actual Scene model at the reached effect point, including stopped prefixes.
#![forbid(unsafe_code)]
use crate::driving_world_assets::{DrivingWorldAssets, DrivingWorldIdentity};
use crate::frame_transform::FrameKey;
use crate::mission_scene_loading::LoadedSceneObject;
use crate::model_loading::{ModelArena, ModelKey};
use crate::scene_ownership::{FrameOwner, SceneKey};
use crate::script_scheduler::FrameReferenceId;
use std::collections::BTreeMap;

type Out<T> = Result<T, String>;
fn error(e: impl std::fmt::Debug) -> String {
    format!("canonical scene2 models: {e:?}")
}
#[derive(Debug)]
struct ModelBinding {
    reference: FrameReferenceId,
    epoch: u64,
}
#[derive(Debug)]
pub(crate) struct CanonicalScene2Models {
    arena: FrameReferenceId,
    scene: SceneKey,
    scene_reference: FrameReferenceId,
    world: DrivingWorldIdentity,
    models: BTreeMap<u32, ModelBinding>,
}
impl CanonicalScene2Models {
    pub(crate) fn bind(
        arena: &ModelArena,
        scene: SceneKey,
        world: &DrivingWorldAssets,
        scene2: &[u8],
        objects: &[LoadedSceneObject],
    ) -> Out<Self> {
        if world.scene2_source() != scene2 {
            return Err("canonical scene2 models: different scene2 source".into());
        }
        arena.scene(scene).map_err(error)?;
        let mut models = BTreeMap::new();
        for (&root, &offset) in world.scene2_models() {
            let mut matches = objects.iter().filter(|o| o.source_offset == offset);
            // A stopped loader has no completed binding for its unreached or
            // partial objects. Do not invent one or fail ahead of SetOn.
            let Some(record) = matches.next() else {
                continue;
            };
            if matches.next().is_some() {
                return Err("canonical scene2 models: duplicate source record".into());
            }
            let Some(epoch) = record.model_epoch else {
                continue;
            };
            let Some(reference) = record.frame else {
                continue;
            };
            let model = ModelKey(FrameKey(reference.identity()));
            if arena.lookup_reference(model.root()) != reference {
                return Err("canonical scene2 models: stale completed source record".into());
            }
            arena.model(model).map_err(error)?;
            models.insert(root, ModelBinding { reference, epoch });
        }
        Ok(Self {
            arena: arena.lookup_reference(FrameKey(0)),
            scene,
            scene_reference: arena.lookup_reference(scene.root()),
            world: world.identity(),
            models,
        })
    }
    /// None means a different world resource family (e.g. city-cache). An
    /// admitted ordinary external model with no live correspondence is an error.
    pub(crate) fn resolve(
        &self,
        arena: &ModelArena,
        world: &DrivingWorldAssets,
        root: FrameKey,
    ) -> Out<Option<FrameKey>> {
        if self.arena != arena.lookup_reference(FrameKey(0))
            || self.scene_reference != arena.lookup_reference(self.scene.root())
            || self.world != world.identity()
        {
            return Err("canonical scene2 models: foreign arena/scene/world".into());
        }
        let scene = arena.scene(self.scene).map_err(error)?;
        if !world.scene2_models().contains_key(&root.0) {
            return Ok(None);
        }
        let binding = self
            .models
            .get(&root.0)
            .ok_or("canonical scene2 models: source record has no completed model")?;
        let key = FrameKey(binding.reference.identity());
        if arena.lookup_reference(key) != binding.reference {
            return Err("canonical scene2 models: stale model reference".into());
        }
        if arena.lifecycle_epoch(ModelKey(key)).map_err(error)? != binding.epoch
            || arena.frame(key).map_err(error)?.owner != Some(FrameOwner::Scene(self.scene))
            || !scene.frames().contains(&key)
        {
            return Err("canonical scene2 models: reloaded or foreign model membership".into());
        }
        Ok(Some(key))
    }
}
