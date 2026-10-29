//! Actor frame effects over actual materialized ModelArena ownership.
//!
//! Common userdata and SetOn stores follow LS3DF PC RVAs0x1a7f0/0x1b400.
//! Actor virtual+0x38 is FindChildFrame0x1c330, using EnumFrames0x1c2a0 and
//! EnumSubtree0x1c020. It excludes self and compares original names (+0x104),
//! unlike the separate GetChildFrame0x1c360 helper used by script frame lookup.
//! This borrows existing frames; it creates no scene sectors or mission actors.
//! Names and derived resource construction remain the existing owner's work.
use crate::actor_world::{ActorFrameBackend, ActorWorld, FrameActorData};
use crate::frame_lookup::{FrameLookupError, MAX_LOOKUP_VISITS};
use crate::frame_transform::{FrameKey, FrameKind};
use crate::model_loading::{ModelArena, ModelError};
use crate::script_scheduler::{ActorReferenceId, FrameReferenceId};

pub const ACTOR_FIND_FLAGS: u32 = 0x40;
pub const ACTOR_FIND_ALL_FLAGS: u32 = 0xffff;

/// Pointer-to-record projection of frame+8. Clearing the pointer does not clear
/// the pointed record or retain/release its actor. Opaque userdata cannot be
/// dereferenced as an actor record by this bounded implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameUserData {
    Null,
    ActorRecord(ActorDataRecordId),
    /// Non-owning pointer into an explicitly supplied addressable owner. The
    /// vehicle frame adapter resolves it through that owner's live storage;
    /// mission actor lookup cannot treat it as an ActorWorld record.
    ExternalRecord(u32),
    Opaque,
}
/// Independent owner-backed record identity. Several frame userdata pointers
/// may refer to one record. Frames do not own or destroy that record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorDataRecordId(pub(crate) FrameReferenceId);
#[derive(Debug)]
pub(crate) struct ActorDataRecord {
    pub(crate) identity: ActorDataRecordId,
    pub(crate) actor: Option<ActorReferenceId>,
    pub(crate) alive: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActorFrameError {
    Lookup(FrameLookupError),
    Model(ModelError),
    UnsupportedUserData(FrameReferenceId),
    UnsupportedSceneEnumeration(FrameReferenceId),
}
impl From<ModelError> for ActorFrameError {
    fn from(value: ModelError) -> Self {
        Self::Model(value)
    }
}
/// Mutable view of one arena. References remain valid across view recreation
/// and model membership removal, until that owned frame actually dies.
pub struct ModelActorFrames<'a> {
    arena: &'a mut ModelArena,
    default_c_locale: bool,
}
impl<'a> ModelActorFrames<'a> {
    pub fn borrow(arena: &'a mut ModelArena, default_c_locale: bool) -> Self {
        Self {
            arena,
            default_c_locale,
        }
    }
    pub fn reference(&self, key: FrameKey) -> Result<FrameReferenceId, ActorFrameError> {
        let reference = self.arena.lookup_reference(key);
        self.resolve(reference)?;
        Ok(reference)
    }
    fn resolve(&self, reference: FrameReferenceId) -> Result<FrameKey, ActorFrameError> {
        let key = FrameKey(reference.identity());
        if self.arena.lookup_reference(key) != reference || self.arena.frame(key).is_err() {
            return Err(ActorFrameError::Lookup(FrameLookupError::StaleFrame(
                reference,
            )));
        }
        Ok(key)
    }
    pub fn actor_data(&self, frame: FrameReferenceId) -> Result<FrameActorData, ActorFrameError> {
        let key = self.resolve(frame)?;
        match self.arena.user_data(key)? {
            FrameUserData::Null => Ok(FrameActorData::Null),
            FrameUserData::ActorRecord(record) => Ok(FrameActorData::Present(
                self.arena.actor_data_record(record)?,
            )),
            FrameUserData::Opaque | FrameUserData::ExternalRecord(_) => {
                Err(ActorFrameError::UnsupportedUserData(frame))
            }
        }
    }
    pub fn clear_actor_data(&mut self, frame: FrameReferenceId) -> Result<(), ActorFrameError> {
        let key = self.resolve(frame)?;
        self.arena.set_user_data(key, FrameUserData::Null)?;
        Ok(())
    }
    /// First matching descendant in actual child/next depth-first order. Only
    /// flags0x40/0xffff, nonempty exact ASCII names and the default C locale are
    /// admitted. This is not an EnumFrames callback API and cannot mutate links
    /// during traversal; native saved-next behavior therefore has no reentry.
    pub fn find_child_frame(
        &self,
        root: FrameReferenceId,
        name: &[u8],
        flags: u32,
        visits: usize,
    ) -> Result<Option<FrameReferenceId>, ActorFrameError> {
        let fail = |error| ActorFrameError::Lookup(error);
        if !matches!(flags, ACTOR_FIND_FLAGS | ACTOR_FIND_ALL_FLAGS) {
            return Err(fail(FrameLookupError::UnsupportedFlags(flags)));
        }
        if visits == 0 || visits > MAX_LOOKUP_VISITS {
            return Err(fail(FrameLookupError::InvalidBudget));
        }
        if !self.default_c_locale {
            return Err(fail(FrameLookupError::UnsupportedLocale));
        }
        if name.is_empty() || !name.is_ascii() || name.contains(&0) {
            return Err(fail(FrameLookupError::UnsupportedName));
        }
        let root_reference = root;
        let root = self.resolve(root)?;
        // FindChildFrame invokes virtual EnumFrames. Scene's RVA 0x49180
        // override traverses its retained sectors, not the scene-root children.
        // That enumeration dependency is not represented by this base traversal.
        if self.arena.state(root)?.kind == FrameKind::Scene {
            return Err(ActorFrameError::UnsupportedSceneEnumeration(root_reference));
        }
        let mut pending = Vec::new();
        pending
            .try_reserve(visits + 1)
            .map_err(|_| fail(FrameLookupError::Allocation))?;
        if let Some(child) = self.arena.links(root)?.child {
            pending.push(child);
        }
        let mut remaining = visits;
        while let Some(key) = pending.pop() {
            if remaining == 0 {
                return Err(fail(FrameLookupError::VisitLimit));
            }
            remaining -= 1;
            let links = self.arena.links(key)?;
            // Native captures next before descending into this child's subtree.
            // The root's own next lies outside this search.
            if let Some(next) = links.next {
                pending.push(next);
            }
            // EnumSubtree1001c02c..3a masks the low word and bypasses the
            // entire type switch for0xffff. Human Init uses this for `base`.
            if flags == ACTOR_FIND_ALL_FLAGS || self.arena.state(key)?.kind == FrameKind::Target {
                if let Some(stored) = &self.arena.frame(key)?.original_name_bytes {
                    // Class admission precedes stored-name inspection. A
                    // filtered visual/dummy name cannot interrupt this search.
                    if !stored.is_ascii() || stored.contains(&0) {
                        return Err(fail(FrameLookupError::UnsupportedName));
                    }
                    if stored.eq_ignore_ascii_case(name) {
                        return Ok(Some(self.arena.lookup_reference(key)));
                    }
                }
            }
            if let Some(child) = links.child {
                pending.push(child);
            }
        }
        Ok(None)
    }
    pub fn set_enabled(
        &mut self,
        frame: FrameReferenceId,
        enabled: u32,
    ) -> Result<(), ActorFrameError> {
        let key = self.resolve(frame)?;
        self.arena.set_frame_on(key, enabled)?;
        Ok(())
    }
    /// Survivor decrements and the arena's existing supported final-destructor
    /// subset only. Unavailable derived/resource effects reject the final
    /// release before decrementing, an explicit host admission boundary.
    pub fn release(&mut self, frame: FrameReferenceId) -> Result<(), ActorFrameError> {
        let key = self.resolve(frame)?;
        self.arena.release(key)?;
        Ok(())
    }
}

impl ActorFrameBackend for ModelActorFrames<'_> {
    fn frame_is_model(
        &mut self,
        _: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<bool, String> {
        let key = self.resolve(frame).map_err(|e| format!("{e:?}"))?;
        Ok(self.arena.state(key).map_err(|e| format!("{e:?}"))?.kind == FrameKind::Model)
    }
    fn stop_model_animation(
        &mut self,
        _: &mut ActorWorld,
        frame: FrameReferenceId,
        index: u32,
    ) -> Result<(), String> {
        let key = self.resolve(frame).map_err(|e| format!("{e:?}"))?;
        self.arena
            .stop_original_model_animation(crate::model_loading::ModelKey(key), index)
            .map_err(|e| format!("{e:?}"))
    }
    fn actor_data(
        &mut self,
        _: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<FrameActorData, String> {
        ModelActorFrames::actor_data(self, frame).map_err(|error| format!("{error:?}"))
    }
    fn clear_actor_data(
        &mut self,
        _: &mut ActorWorld,
        frame: FrameReferenceId,
    ) -> Result<(), String> {
        ModelActorFrames::clear_actor_data(self, frame).map_err(|error| format!("{error:?}"))
    }
    fn find_child(
        &mut self,
        _: &mut ActorWorld,
        frame: FrameReferenceId,
        name: &[u8],
        flags: u32,
    ) -> Result<Option<FrameReferenceId>, String> {
        self.find_child_frame(frame, name, flags, MAX_LOOKUP_VISITS)
            .map_err(|error| format!("{error:?}"))
    }
    fn set_child_enabled(
        &mut self,
        _: &mut ActorWorld,
        child: FrameReferenceId,
        enabled: u32,
    ) -> Result<(), String> {
        self.set_enabled(child, enabled)
            .map_err(|error| format!("{error:?}"))
    }
    fn release_frame(&mut self, _: &mut ActorWorld, frame: FrameReferenceId) -> Result<(), String> {
        self.release(frame).map_err(|error| format!("{error:?}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actor_world::{
        ActorCollectionBackend, ActorPhase, ActorSource, ActorStorageId, ActorWorldError,
        BaseActorSeed, SourceBacking,
    };
    use crate::frame_lookup::{ModelFrameLookup, SCRIPT_FIND_FLAGS};
    use crate::frame_transform::FrameError;
    use crate::model_loading::{tests::frame, MaterializedModel};
    use std::sync::Arc;

    fn named(kind: u8, parent: Option<usize>, name: &[u8]) -> lhr_assets::Frame {
        let mut frame = frame(kind, parent, [0.; 3]);
        frame.name_bytes = name.to_vec();
        if kind == 7 {
            frame.target = Some(lhr_assets::TargetRecord {
                flags: 0,
                frame_indices: vec![],
            });
        }
        frame
    }
    fn loaded(frames: Vec<lhr_assets::Frame>) -> MaterializedModel {
        MaterializedModel::from_parsed(Arc::new(lhr_assets::Model {
            materials: vec![],
            frames,
        }))
        .unwrap()
    }
    fn reference(model: &MaterializedModel, key: FrameKey) -> FrameReferenceId {
        ModelFrameLookup::borrow(model.arena(), true)
            .reference(key)
            .unwrap()
    }
    fn actor_world() -> ActorWorld {
        let backing = SourceBacking {
            storage: Some(ActorStorageId(1)),
            capacity: 4,
        };
        ActorWorld::new(0, 0, backing, backing, backing).unwrap()
    }
    struct NoGrowth;
    impl ActorCollectionBackend for NoGrowth {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            panic!("preallocated source must not allocate")
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            panic!("preallocated source must not free")
        }
    }
    fn actor(world: &mut ActorWorld, id: u32) -> ActorReferenceId {
        world
            .construct_base(
                id,
                BaseActorSeed {
                    word_0e: 0,
                    actor_id: 9,
                    state_1c: 7,
                    bytes_21_5f: [0; 63],
                    bytes_64_65: [0; 2],
                    byte_67: 0,
                    release_frame: 0,
                    bytes_6d_6f: [0; 3],
                },
                &mut NoGrowth,
            )
            .unwrap()
    }

    #[test]
    fn shared_userdata_reads_live_record_and_pointer_clear_keeps_record() {
        let mut model = loaded(vec![named(6, None, b"a"), named(6, None, b"b")]);
        let keys = model.source_frame_keys().to_vec();
        let refs = [reference(&model, keys[0]), reference(&model, keys[1])];
        let mut world = actor_world();
        let a = actor(&mut world, 11);
        let b = actor(&mut world, 12);
        let record = model.arena_mut().create_actor_data_record(None).unwrap();
        for key in &keys {
            model
                .arena_mut()
                .set_user_data(*key, FrameUserData::ActorRecord(record))
                .unwrap();
        }
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).actor_data(refs[0]),
            Ok(FrameActorData::Present(None))
        );
        model
            .arena_mut()
            .set_actor_data_record(record, Some(a))
            .unwrap();
        {
            let mut backend = ModelActorFrames::borrow(model.arena_mut(), true);
            assert_eq!(
                backend.actor_data(refs[0]),
                Ok(FrameActorData::Present(Some(a)))
            );
            backend.clear_actor_data(refs[0]).unwrap();
            assert_eq!(backend.actor_data(refs[0]), Ok(FrameActorData::Null));
            assert_eq!(
                backend.actor_data(refs[1]),
                Ok(FrameActorData::Present(Some(a)))
            );
        }
        model
            .arena_mut()
            .set_actor_data_record(record, Some(b))
            .unwrap();
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).actor_data(refs[1]),
            Ok(FrameActorData::Present(Some(b)))
        );
        assert_eq!(model.arena().actor_data_record(record), Ok(Some(b)));
    }

    #[test]
    fn retired_and_foreign_records_fail_without_becoming_null() {
        let mut model = loaded(vec![named(6, None, b"a")]);
        let key = model.source_frame_keys()[0];
        let r = reference(&model, key);
        let record = model.arena_mut().create_actor_data_record(None).unwrap();
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::ActorRecord(record))
            .unwrap();
        model.arena_mut().retire_actor_data_record(record).unwrap();
        let error = ActorFrameError::Model(ModelError::UnknownActorDataRecord(record));
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).actor_data(r),
            Err(error)
        );
        assert_eq!(
            model.arena().user_data(key),
            Ok(FrameUserData::ActorRecord(record))
        );
        let mut foreign = ModelArena::default();
        assert_eq!(
            foreign.actor_data_record(record),
            Err(ModelError::UnknownActorDataRecord(record))
        );
        let replacement = model.arena_mut().create_actor_data_record(None).unwrap();
        assert_ne!(record, replacement);
        let unrelated = foreign.create_actor_data_record(None).unwrap();
        assert_ne!(replacement, unrelated);
        assert_eq!(
            model.arena_mut().set_actor_data_record(unrelated, None),
            Err(ModelError::UnknownActorDataRecord(unrelated))
        );
    }

    #[test]
    fn opaque_userdata_is_not_dereferenced_but_can_be_detached() {
        let mut model = loaded(vec![named(6, None, b"a")]);
        let key = model.source_frame_keys()[0];
        let r = reference(&model, key);
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::Opaque)
            .unwrap();
        let mut backend = ModelActorFrames::borrow(model.arena_mut(), true);
        assert_eq!(
            backend.actor_data(r),
            Err(ActorFrameError::UnsupportedUserData(r))
        );
        backend.clear_actor_data(r).unwrap();
        assert_eq!(backend.actor_data(r), Ok(FrameActorData::Null));
    }

    #[test]
    fn actor_init_uses_real_targets_and_leaves_dummy_names_and_reference_counts() {
        let mut model = loaded(vec![
            named(7, None, b"TaRgEtE"),
            named(7, None, b"targetN"),
            named(6, None, b"targetE"),
        ]);
        let keys = model.source_frame_keys().to_vec();
        let root = reference(&model, model.root());
        let before: Vec<_> = keys
            .iter()
            .map(|&key| *model.arena().state(key).unwrap())
            .collect();
        let mut world = actor_world();
        let a = actor(&mut world, 11);
        let root_key = model.root();
        model
            .arena_mut()
            .set_actor_data(root_key, FrameActorData::Present(Some(a)))
            .unwrap();
        assert!(world
            .init(
                a,
                Some(root),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        assert_eq!(world.state(a).unwrap().frame, Some(root));
        assert_eq!(world.state(a).unwrap().release_frame, 0);
        assert_eq!(world.state(a).unwrap().state_1c, 0);
        assert_eq!(world.attachment_counter, 1);
        assert_eq!(
            model.arena().user_data(model.root()),
            Ok(FrameUserData::Null)
        );
        for (i, key) in keys.iter().enumerate() {
            let mut expected = before[i];
            if i < 2 {
                expected.flags &= !1;
            }
            assert_eq!(*model.arena().state(*key).unwrap(), expected);
            assert_eq!(model.arena().frame(*key).unwrap().references, 1);
        }
        assert_eq!(model.arena().frame(model.root()).unwrap().references, 1);
    }

    #[test]
    fn conflict_rejects_before_attachment_or_frame_writes() {
        let mut model = loaded(vec![named(7, None, b"targetE")]);
        let root_key = model.root();
        let root = reference(&model, root_key);
        let mut world = actor_world();
        let a = actor(&mut world, 11);
        let b = actor(&mut world, 12);
        model
            .arena_mut()
            .set_actor_data(root_key, FrameActorData::Present(Some(b)))
            .unwrap();
        let before = world.state(a).unwrap().clone();
        let pointer = model.arena().user_data(root_key).unwrap();
        assert!(!world
            .init(
                a,
                Some(root),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            )
            .unwrap());
        assert_eq!(world.state(a).unwrap(), &before);
        assert_eq!(world.attachment_counter, 0);
        assert_eq!(world.phase(a), Ok(ActorPhase::Ready));
        assert_eq!(model.arena().user_data(root_key), Ok(pointer));
        assert_eq!(
            model
                .arena()
                .state(model.source_frame_keys()[0])
                .unwrap()
                .flags
                & 1,
            1
        );
    }

    #[test]
    fn null_and_present_empty_userdata_both_attach() {
        for data in [FrameActorData::Null, FrameActorData::Present(None)] {
            let mut model = loaded(vec![]);
            let key = model.root();
            let r = reference(&model, key);
            model.arena_mut().set_actor_data(key, data).unwrap();
            let mut world = actor_world();
            let a = actor(&mut world, 11);
            assert_eq!(
                world.init(
                    a,
                    Some(r),
                    &mut ModelActorFrames::borrow(model.arena_mut(), true)
                ),
                Ok(true)
            );
            assert_eq!(model.arena().user_data(key), Ok(FrameUserData::Null));
            assert_eq!(model.arena().frame(key).unwrap().references, 1);
        }
    }

    #[test]
    fn opaque_init_failure_marks_interrupted_before_any_attachment_prefix() {
        let mut model = loaded(vec![]);
        let key = model.root();
        let r = reference(&model, key);
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::Opaque)
            .unwrap();
        let mut world = actor_world();
        let a = actor(&mut world, 11);
        assert!(matches!(
            world.init(
                a,
                Some(r),
                &mut ModelActorFrames::borrow(model.arena_mut(), true)
            ),
            Err(ActorWorldError::Effect {
                stage: "frame_actor_data",
                ..
            })
        ));
        assert_eq!(world.phase(a), Ok(ActorPhase::InitializationInterrupted));
        assert_eq!(world.state(a).unwrap().frame, None);
        assert_eq!(world.attachment_counter, 0);
        assert_eq!(model.arena().user_data(key), Ok(FrameUserData::Opaque));
    }

    #[test]
    fn unsupported_search_locale_keeps_real_init_prefix() {
        let mut model = loaded(vec![named(7, None, b"targetE")]);
        let key = model.root();
        let r = reference(&model, key);
        model
            .arena_mut()
            .set_actor_data(key, FrameActorData::Present(None))
            .unwrap();
        let mut world = actor_world();
        let a = actor(&mut world, 11);
        assert!(matches!(
            world.init(
                a,
                Some(r),
                &mut ModelActorFrames::borrow(model.arena_mut(), false)
            ),
            Err(ActorWorldError::Effect {
                stage: "find_target_child",
                ..
            })
        ));
        assert_eq!(world.phase(a), Ok(ActorPhase::InitializationInterrupted));
        assert_eq!(world.state(a).unwrap().frame, Some(r));
        assert_eq!(world.state(a).unwrap().actor_id, 1);
        assert_eq!(world.state(a).unwrap().state_1c, 7);
        assert_eq!(model.arena().user_data(key), Ok(FrameUserData::Null));
        assert_eq!(
            model
                .arena()
                .state(model.source_frame_keys()[0])
                .unwrap()
                .flags
                & 1,
            1
        );
    }

    #[test]
    fn child_search_excludes_self_and_root_siblings() {
        let mut model = loaded(vec![named(7, None, b"targetE"), named(7, None, b"targetN")]);
        let keys = model.source_frame_keys().to_vec();
        let root = reference(&model, model.root());
        let e = reference(&model, keys[0]);
        let n = reference(&model, keys[1]);
        let backend = ModelActorFrames::borrow(model.arena_mut(), true);
        assert_eq!(
            backend.find_child_frame(e, b"targetE", ACTOR_FIND_FLAGS, 2),
            Ok(None)
        );
        assert_eq!(
            backend.find_child_frame(n, b"targetE", ACTOR_FIND_FLAGS, 2),
            Ok(None)
        );
        assert_eq!(
            backend.find_child_frame(root, b"targetE", ACTOR_FIND_FLAGS, 2),
            Ok(Some(e))
        );
    }

    #[test]
    fn human_all_types_search_uses_original_names_and_same_live_depth_first_order() {
        let mut model = loaded(vec![named(7, None, b"base"), named(6, None, b"BASE")]);
        let keys = model.source_frame_keys().to_vec();
        let root = reference(&model, model.root());
        let target = reference(&model, keys[0]);
        let dummy = reference(&model, keys[1]);
        model
            .arena_mut()
            .set_frame_model_name(keys[1], b"prefixed")
            .unwrap();
        let backend = ModelActorFrames::borrow(model.arena_mut(), true);
        assert_eq!(
            backend.find_child_frame(root, b"base", ACTOR_FIND_FLAGS, 2),
            Ok(Some(target))
        );
        assert_eq!(
            backend.find_child_frame(root, b"base", ACTOR_FIND_ALL_FLAGS, 2),
            Ok(Some(dummy))
        );
        assert_eq!(
            backend.find_child_frame(dummy, b"base", ACTOR_FIND_ALL_FLAGS, 2),
            Ok(None)
        );
    }

    #[test]
    fn depth_first_link_order_filters_classes_before_reading_their_names() {
        let mut model = loaded(vec![
            named(7, None, b"hit"),
            named(6, None, b"\xff"),
            named(7, Some(1), b"HIT"),
        ]);
        let keys = model.source_frame_keys().to_vec();
        let root = reference(&model, model.root());
        let expected = reference(&model, keys[2]);
        let backend = ModelActorFrames::borrow(model.arena_mut(), true);
        // LinkTo prepends the later dummy; its target descendant wins over the
        // earlier root-level target even though its own name is inadmissible.
        assert_eq!(
            backend.find_child_frame(root, b"hit", ACTOR_FIND_FLAGS, 2),
            Ok(Some(expected))
        );
        assert_eq!(
            backend.find_child_frame(root, b"absent", ACTOR_FIND_FLAGS, 3),
            Ok(None)
        );
        assert_eq!(
            backend.find_child_frame(root, b"hit*", ACTOR_FIND_FLAGS, 3),
            Ok(None)
        );
    }

    #[test]
    fn search_limits_and_unsupported_inputs_are_explicit() {
        let mut model = loaded(vec![named(6, None, b"a"), named(7, Some(0), b"hit")]);
        let root = reference(&model, model.root());
        let target = reference(&model, model.source_frame_keys()[1]);
        let backend = ModelActorFrames::borrow(model.arena_mut(), true);
        assert_eq!(
            backend.find_child_frame(root, b"hit", ACTOR_FIND_FLAGS, 1),
            Err(ActorFrameError::Lookup(FrameLookupError::VisitLimit))
        );
        assert_eq!(
            backend.find_child_frame(root, b"hit", ACTOR_FIND_FLAGS, 2),
            Ok(Some(target))
        );
        for visits in [0, MAX_LOOKUP_VISITS + 1] {
            assert_eq!(
                backend.find_child_frame(root, b"hit", ACTOR_FIND_FLAGS, visits),
                Err(ActorFrameError::Lookup(FrameLookupError::InvalidBudget))
            );
        }
        for name in [b"".as_slice(), b"h\0it", b"\xff"] {
            assert_eq!(
                backend.find_child_frame(root, name, ACTOR_FIND_FLAGS, 2),
                Err(ActorFrameError::Lookup(FrameLookupError::UnsupportedName))
            );
        }
        assert_eq!(
            backend.find_child_frame(root, b"hit", SCRIPT_FIND_FLAGS, 2),
            Err(ActorFrameError::Lookup(FrameLookupError::UnsupportedFlags(
                SCRIPT_FIND_FLAGS
            )))
        );
    }

    #[test]
    fn eligible_non_ascii_name_interrupts_before_later_match() {
        let mut model = loaded(vec![named(7, None, b"hit"), named(7, None, b"\xff")]);
        let root = reference(&model, model.root());
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).find_child_frame(
                root,
                b"hit",
                ACTOR_FIND_FLAGS,
                2
            ),
            Err(ActorFrameError::Lookup(FrameLookupError::UnsupportedName))
        );
    }

    #[test]
    fn set_on_uses_only_low_byte_and_preserves_all_other_state() {
        let mut model = loaded(vec![
            named(7, None, b"a"),
            named(6, None, b"b"),
            named(1, None, b"c"),
        ]);
        let keys = std::iter::once(model.root())
            .chain(model.source_frame_keys().iter().copied())
            .collect::<Vec<_>>();
        for key in keys {
            let r = reference(&model, key);
            let original = *model.arena().state(key).unwrap();
            for on in [0, 0x100, 0x101, 0xffff_ff00, 0x80] {
                ModelActorFrames::borrow(model.arena_mut(), true)
                    .set_enabled(r, on)
                    .unwrap();
                let mut expected = original;
                expected.flags = (expected.flags & !1) | u32::from(on as u8 != 0);
                assert_eq!(*model.arena().state(key).unwrap(), expected);
            }
        }
    }

    #[test]
    fn scene_set_on_override_is_empty_while_sectors_keep_base_low_byte_behavior() {
        let mut arena = ModelArena::default();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let sectors = arena.scene(scene).unwrap().sectors;
        for key in std::iter::once(scene.root()).chain(sectors.map(|sector| sector.frame())) {
            let frame = ModelActorFrames::borrow(&mut arena, true)
                .reference(key)
                .unwrap();
            let original = *arena.state(key).unwrap();
            for enabled in [0, 1, 0x100, 0x101, 0xffff_ff00, 0x80, u32::MAX] {
                ModelActorFrames::borrow(&mut arena, true)
                    .set_enabled(frame, enabled)
                    .unwrap();
                let mut expected = original;
                if key != scene.root() {
                    expected.flags = (expected.flags & !1) | u32::from(enabled as u8 != 0);
                }
                assert_eq!(*arena.state(key).unwrap(), expected);
            }
        }
    }
    #[test]
    fn scene_find_child_rejects_unavailable_virtual_sector_enumeration() {
        let mut model = loaded(vec![named(7, None, b"target")]);
        let target = model.source_frame_keys()[0];
        let arena = model.arena_mut();
        let scene = arena.construct_scene_owner_prefix().unwrap();
        let primary = arena.scene(scene).unwrap().sectors[0].frame();
        arena.link_frame(target, Some(primary), 0).unwrap();
        let frames = ModelActorFrames::borrow(arena, true);
        let scene_ref = frames.reference(scene.root()).unwrap();
        let primary_ref = frames.reference(primary).unwrap();
        let target_ref = frames.reference(target).unwrap();
        assert_eq!(
            frames.find_child_frame(scene_ref, b"target", ACTOR_FIND_FLAGS, 8),
            Err(ActorFrameError::UnsupportedSceneEnumeration(scene_ref))
        );
        assert_eq!(
            frames.find_child_frame(primary_ref, b"target", ACTOR_FIND_FLAGS, 8),
            Ok(Some(target_ref))
        );
    }
    #[test]
    fn foreign_frame_identity_rejects_every_effect_and_preserves_owner() {
        let mut model = loaded(vec![named(7, None, b"a")]);
        let foreign_model = loaded(vec![]);
        let foreign = reference(&foreign_model, foreign_model.root());
        let root = model.root();
        let before = *model.arena().state(root).unwrap();
        let stale = ActorFrameError::Lookup(FrameLookupError::StaleFrame(foreign));
        let mut backend = ModelActorFrames::borrow(model.arena_mut(), true);
        assert_eq!(backend.actor_data(foreign), Err(stale.clone()));
        assert_eq!(backend.clear_actor_data(foreign), Err(stale.clone()));
        assert_eq!(backend.set_enabled(foreign, 0), Err(stale.clone()));
        assert_eq!(backend.release(foreign), Err(stale.clone()));
        assert_eq!(
            backend.find_child_frame(foreign, b"a", ACTOR_FIND_FLAGS, 1),
            Err(stale)
        );
        assert_eq!(*model.arena().state(root).unwrap(), before);
        assert_eq!(model.arena().frame(root).unwrap().references, 1);
    }

    #[test]
    fn retained_member_survives_removal_and_view_recreation_until_final_release() {
        let mut model = loaded(vec![named(6, None, b"a")]);
        let key = model.source_frame_keys()[0];
        let r = reference(&model, key);
        let owner = model.model_key();
        model.arena_mut().retain(key).unwrap();
        model
            .arena_mut()
            .set_actor_data(key, FrameActorData::Present(None))
            .unwrap();
        model.arena_mut().delete_frame(owner, key).unwrap();
        assert_eq!(model.arena().frame(key).unwrap().owner, None);
        assert_eq!(model.arena().frame(key).unwrap().references, 1);
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).actor_data(r),
            Ok(FrameActorData::Present(None))
        );
        let mut backend = ModelActorFrames::borrow(model.arena_mut(), true);
        backend.set_enabled(r, 0).unwrap();
        backend.release(r).unwrap();
        assert_eq!(
            backend.actor_data(r),
            Err(ActorFrameError::Lookup(FrameLookupError::StaleFrame(r)))
        );
        assert_eq!(
            backend.release(r),
            Err(ActorFrameError::Lookup(FrameLookupError::StaleFrame(r)))
        );
    }

    #[test]
    fn frame_death_never_retires_external_userdata_record() {
        let mut model = loaded(vec![named(6, None, b"a")]);
        let key = model.source_frame_keys()[0];
        let r = reference(&model, key);
        let record = model.arena_mut().create_actor_data_record(None).unwrap();
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::ActorRecord(record))
            .unwrap();
        ModelActorFrames::borrow(model.arena_mut(), true)
            .release(r)
            .unwrap();
        assert_eq!(model.arena().actor_data_record(record), Ok(None));
        assert!(model
            .arena()
            .model(model.model_key())
            .unwrap()
            .frames
            .is_empty());
    }

    #[test]
    fn surviving_visual_release_and_unsupported_final_release_are_distinct() {
        let mut model = loaded(vec![named(1, None, b"mesh")]);
        let key = model.source_frame_keys()[0];
        let r = reference(&model, key);
        let state = *model.arena().state(key).unwrap();
        model.arena_mut().retain(key).unwrap();
        ModelActorFrames::borrow(model.arena_mut(), true)
            .release(r)
            .unwrap();
        assert_eq!(model.arena().frame(key).unwrap().references, 1);
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).release(r),
            Err(ActorFrameError::Model(ModelError::Frame(
                FrameError::UnsupportedDestruction(key)
            )))
        );
        assert_eq!(model.arena().frame(key).unwrap().references, 1);
        assert_eq!(
            model.arena().frame(key).unwrap().owner,
            Some(model.model_key().into())
        );
        assert_eq!(*model.arena().state(key).unwrap(), state);
    }

    #[test]
    fn close_and_reopen_preserve_retained_identity_userdata_and_old_source() {
        let mut model = loaded(vec![named(7, None, b"targetE")]);
        let key = model.source_frame_keys()[0];
        let r = reference(&model, key);
        let root = reference(&model, model.root());
        model.arena_mut().retain(key).unwrap();
        model
            .arena_mut()
            .set_actor_data(key, FrameActorData::Present(None))
            .unwrap();
        let source = model.source().clone();
        model.close().unwrap();
        assert_eq!(model.arena().frame(key).unwrap().owner, None);
        assert!(Arc::ptr_eq(
            model.arena().frame(key).unwrap().source.as_ref().unwrap(),
            &source
        ));
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).find_child_frame(
                root,
                b"targetE",
                ACTOR_FIND_FLAGS,
                1
            ),
            Ok(Some(r))
        );
        model.reopen_from_parsed(source).unwrap();
        let new = model.source_frame_keys()[0];
        assert_ne!(key, new);
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true).actor_data(r),
            Ok(FrameActorData::Present(None))
        );
        ModelActorFrames::borrow(model.arena_mut(), true)
            .release(r)
            .unwrap();
        assert!(model.arena().frame(new).is_ok());
    }

    #[test]
    fn actual_actor_destruction_releases_conditionally_and_keeps_failure_prefix() {
        for (kind, release, succeeds) in [(6, 0, true), (6, 1, true), (1, 1, false)] {
            let mut model = loaded(vec![named(kind, None, b"frame")]);
            let key = model.source_frame_keys()[0];
            let r = reference(&model, key);
            let mut world = actor_world();
            let a = actor(&mut world, 11);
            assert!(world
                .init(
                    a,
                    Some(r),
                    &mut ModelActorFrames::borrow(model.arena_mut(), true)
                )
                .unwrap());
            world.state_mut(a).unwrap().release_frame = release;
            let result =
                world.destroy_base(a, &mut ModelActorFrames::borrow(model.arena_mut(), true));
            assert_eq!(result.is_ok(), succeeds);
            assert_eq!(world.actor_count, 0);
            if succeeds {
                assert!(world.source(ActorSource::Static).is_empty());
                assert_eq!(model.arena().frame(key).is_ok(), release == 0);
            } else {
                assert_eq!(world.phase(a), Ok(ActorPhase::DestructionInterrupted));
                assert_eq!(world.source(ActorSource::Static), &[Some(a)]);
                assert_eq!(model.arena().frame(key).unwrap().references, 1);
            }
        }
    }
}
