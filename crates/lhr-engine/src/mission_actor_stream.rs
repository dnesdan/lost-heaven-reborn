//! Ordered actor arm of Game.exe scene2 loader RVA0x141030 (544a16..544b8d).
//! PC SHA25608398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! This requires the preceding loader's actual name map and materialized frames.
//! It is not MissionOpen, GameInit, a scene constructor or a running mission.
use crate::actor_frames::{FrameUserData, ModelActorFrames};
use crate::actor_world::*;
use crate::frame_lookup::ModelFrameLookup;
use crate::frame_transform::FrameKey;
use crate::model_loading::ModelArena;
use crate::original_program::{ProgramBackend, ProgramReplacement, ProgramSliceReader};
use crate::script_scheduler::{ActorReferenceId, FrameReferenceId};
use lhr_assets::scene_actor_stream::{scene2_root, Chunk};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct MissionActorError {
    pub offset: usize,
    pub actor: Option<ActorReferenceId>,
    pub detail: String,
}
/// The map is populated only by explicit preceding-loader publication, in that
/// caller's actual order. An absent lookup inserts a null slot, as547170 does.
/// It never enumerates a scene or manufactures frames from inspection metadata.
/// This is a logical host map projection; native C_str/map allocation, reference
/// counting and tree balancing are not claimed as reconstructed functions.
pub struct OriginalLoadMap {
    slots: BTreeMap<Vec<u8>, Option<FrameReferenceId>>,
}
impl OriginalLoadMap {
    /// Begin the actual preceding loader's empty local map. This does not assert
    /// that its scene/model construction phase has been implemented or finished.
    pub fn for_preceding_loader() -> Self {
        Self {
            slots: BTreeMap::new(),
        }
    }
    pub fn publish_loaded_frame(
        &mut self,
        arena: &ModelArena,
        name: &[u8],
        frame: FrameReferenceId,
    ) -> Result<(), String> {
        valid_name(name)?;
        validate_frame(arena, frame)?;
        self.slots.insert(name.to_vec(), Some(frame));
        Ok(())
    }
    pub fn slot(&self, name: &[u8]) -> Option<Option<FrameReferenceId>> {
        self.slots.get(name).copied()
    }
    /// Original object/actor loader lookup: a missing key acquires a null slot
    /// before a later factory call. No frame is manufactured by the map.
    pub fn lookup_or_insert_null(
        &mut self,
        name: &[u8],
    ) -> Result<Option<FrameReferenceId>, String> {
        valid_name(name)?;
        Ok(*self.slots.entry(name.to_vec()).or_insert(None))
    }
}
pub struct MissionActorLoader {
    pub world: ActorWorld,
    load_map: Option<OriginalLoadMap>,
    pub current_actor: Option<ActorReferenceId>,
    pub current_frame: Option<FrameReferenceId>,
    current_kind: Option<u32>,
    pub interrupted_offset: Option<usize>,
    /// Absolute byte position reached by the current bounded read.
    pub cursor: usize,
    /// Exact current ae21 bytes survive an interrupted Installation call.
    pub current_record: Vec<u8>,
    pub pending_load_data: Option<Vec<u8>>,
}
impl MissionActorLoader {
    pub fn new(world: ActorWorld, load_map: OriginalLoadMap) -> Self {
        Self {
            world,
            load_map: Some(load_map),
            current_actor: None,
            current_frame: None,
            current_kind: None,
            interrupted_offset: None,
            cursor: 0,
            current_record: Vec::new(),
            pending_load_data: None,
        }
    }
    /// An enclosing scene loader supplies its actual map at each root. No
    /// independent map or scene snapshot is constructed by this owner.
    pub fn borrowing_scene_map(world: ActorWorld) -> Self {
        Self {
            world,
            load_map: None,
            current_actor: None,
            current_frame: None,
            current_kind: None,
            interrupted_offset: None,
            cursor: 0,
            current_record: Vec::new(),
            pending_load_data: None,
        }
    }
    /// Publish the slot assignment performed by the preceding original loader
    /// after it has created/loaded this frame. The live arena identity is checked.
    pub fn publish_loaded_frame(
        &mut self,
        arena: &ModelArena,
        name: &[u8],
        frame: FrameReferenceId,
    ) -> Result<(), String> {
        self.load_map
            .as_mut()
            .ok_or("actor loader borrows the enclosing scene map")?
            .publish_loaded_frame(arena, name, frame)
    }
    pub fn load_map_slot(&self, name: &[u8]) -> Option<Option<FrameReferenceId>> {
        self.load_map.as_ref().and_then(|map| map.slot(name))
    }
    /// Actual installation bytes feed the same ordered executor used by an
    /// enclosing scene loader. Non-actor phases must already have been loaded
    /// by the caller; this method supplies no substitute implementation for them.
    pub fn load_from_installation<
        B: ActorAllocationBackend + ActorCollectionBackend + ActorDeallocationBackend + ProgramBackend,
    >(
        &mut self,
        installation: &lhr_assets::Installation,
        mission: &str,
        arena: &mut ModelArena,
        replacements: &[ProgramReplacement],
        backend: &mut B,
    ) -> Result<(), MissionActorError> {
        let path = format!("missions/{mission}/scene2.bin");
        let bytes = installation
            .read(&path)
            .map_err(|e| self.error(0, e.to_string()))?;
        let native_mission_path = format!("missions\\{mission}");
        let root = scene2_root(&bytes).map_err(|e| self.error(0, e.to_string()))?;
        for chunk in root.children() {
            let chunk = chunk.map_err(|e| self.error(0, e.to_string()))?;
            if chunk.kind == 0xae20 {
                self.load_collection(
                    chunk,
                    arena,
                    Some(native_mission_path.as_bytes()),
                    replacements,
                    backend,
                )?;
            }
        }
        Ok(())
    }
    fn error(&mut self, offset: usize, detail: String) -> MissionActorError {
        if self.interrupted_offset.is_none() {
            self.interrupted_offset = Some(offset);
        }
        MissionActorError {
            offset,
            actor: self.current_actor,
            detail,
        }
    }
    pub fn load_collection<
        B: ActorAllocationBackend + ActorCollectionBackend + ActorDeallocationBackend + ProgramBackend,
    >(
        &mut self,
        collection: Chunk<'_>,
        arena: &mut ModelArena,
        mission_path: Option<&[u8]>,
        replacements: &[ProgramReplacement],
        backend: &mut B,
    ) -> Result<(), MissionActorError> {
        let Some(mut map) = self.load_map.take() else {
            return Err(self.error(
                collection.offset,
                "actor loader requires the enclosing scene map".into(),
            ));
        };
        // Move, rather than copy, the original map. Restore it on every result.
        let result = self.load_collection_with_map(
            collection,
            arena,
            &mut map,
            mission_path,
            replacements,
            backend,
        );
        self.load_map = Some(map);
        result
    }
    pub fn load_collection_with_map<
        B: ActorAllocationBackend + ActorCollectionBackend + ActorDeallocationBackend + ProgramBackend,
    >(
        &mut self,
        collection: Chunk<'_>,
        arena: &mut ModelArena,
        map: &mut OriginalLoadMap,
        mission_path: Option<&[u8]>,
        replacements: &[ProgramReplacement],
        backend: &mut B,
    ) -> Result<(), MissionActorError> {
        if self.interrupted_offset.is_some() {
            return Err(self.error(
                collection.offset,
                "mission actor loading is interrupted".into(),
            ));
        }
        if collection.kind != 0xae20 {
            return Err(self.error(
                collection.offset,
                "expected original actor collection ae20".into(),
            ));
        }
        self.cursor = collection.offset + 6;
        for record in collection.children() {
            let record = record.map_err(|e| self.error(collection.offset, e.to_string()))?;
            self.cursor = record.offset + 6;
            if record.kind != 0xae21 {
                self.cursor = record.offset + record.raw.len();
                continue;
            }
            self.current_record = record.raw.to_vec();
            self.pending_load_data = None;
            self.current_actor = None;
            self.current_frame = None;
            self.current_kind = None;
            for field in record.children() {
                let field = field.map_err(|e| self.error(record.offset, e.to_string()))?;
                self.cursor = field.offset + 6;
                if let Err(detail) =
                    self.field(field, arena, map, mission_path, replacements, backend)
                {
                    return Err(self.error(field.offset, detail));
                }
                self.cursor = field.offset + field.raw.len();
            }
            self.cursor = record.offset + record.raw.len();
        }
        self.cursor = collection.offset + collection.raw.len();
        Ok(())
    }
    fn field<
        B: ActorAllocationBackend + ActorCollectionBackend + ActorDeallocationBackend + ProgramBackend,
    >(
        &mut self,
        field: Chunk<'_>,
        arena: &mut ModelArena,
        map: &mut OriginalLoadMap,
        mission_path: Option<&[u8]>,
        replacements: &[ProgramReplacement],
        backend: &mut B,
    ) -> Result<(), String> {
        match field.kind {
            0xae23 => {
                let end = field
                    .payload
                    .iter()
                    .position(|x| *x == 0)
                    .ok_or("unterminated actor frame name")?;
                // C_chunk::ReadString descends to the declared field end
                // before returning to the map/frame effects, including padding.
                self.cursor = field.offset + field.raw.len();
                let name = &field.payload[..end];
                self.current_frame = map.lookup_or_insert_null(name)?;
                self.current_actor = None;
                self.current_kind = None;
                if let Some(frame) = self.current_frame {
                    validate_frame(arena, frame)?;
                    let frames = ModelActorFrames::borrow(arena, true);
                    if matches!(
                        frames.actor_data(frame).map_err(|e| format!("{e:?}"))?,
                        FrameActorData::Present(Some(_))
                    ) {
                        self.current_frame = None;
                    }
                }
            }
            0xae22 => {
                let raw = field.payload.get(..4).ok_or("truncated actor kind")?;
                let kind = u32::from_le_bytes(raw.try_into().unwrap());
                // C_chunk::ReadDword descends before CreateActor/Init. Keep
                // that reached input prefix even when a later effect fails.
                self.cursor = field.offset + field.raw.len();
                if let Some(frame) = self.current_frame {
                    self.current_actor = match self.world.create_actor(kind, backend) {
                        Ok(actor) => actor,
                        Err(error) => {
                            self.current_actor = error.actor;
                            return Err(format!("{error:?}"));
                        }
                    };
                    self.current_kind = Some(kind);
                    if let Some(actor) = self.current_actor {
                        let mut frames = ModelActorFrames::borrow(arena, true);
                        if self
                            .world
                            .init(actor, Some(frame), &mut frames)
                            .map_err(|e| format!("{e:?}"))?
                        {
                            //547220, then53fea0(actor,false), before frame userdata publication.
                            self.world
                                .mark_scene_loaded(actor)
                                .map_err(|e| format!("{e:?}"))?;
                            self.world
                                .add_actor(Some(actor), false, backend)
                                .map_err(|e| format!("{e:?}"))?;
                            let key = validate_frame(arena, frame)?;
                            publish_actor_data(arena, key, actor)?;
                        } else {
                            if kind == 8 {
                                self.world
                                    .release_railway(actor, &mut frames, backend)
                                    .map_err(|e| format!("{e:?}"))?;
                            } else if kind == 5 {
                                self.world
                                    .release_detector(actor, &mut frames, backend)
                                    .map_err(|e| format!("{e:?}"))?;
                            } else {
                                self.world
                                    .release(actor, &mut ReleaseFrames { frames, backend })
                                    .map_err(|e| format!("{e:?}"))?;
                            }
                            self.current_actor = None;
                        }
                    }
                }
            }
            0xae24 => {
                if let Some(actor) = self.current_actor {
                    if !field.payload.is_empty() {
                        self.pending_load_data = Some(field.payload.to_vec());
                        if self.current_kind == Some(1) {
                            // Base vtable623248+50 ->4aa490: MOV AL,1;RET4.
                            // This actual literal-success leaf reads no payload.
                            // The outer chunk reader skips its remaining bytes.
                            self.pending_load_data = None;
                            return Ok(());
                        }
                        if self.current_kind == Some(8) {
                            self.world
                                .load_railway_data(actor, field.payload)
                                .map_err(|e| format!("{e:?}"))?;
                            self.pending_load_data = None;
                            return Ok(());
                        }
                        if self.current_kind != Some(5) {
                            return Err(format!(
                                "unimplemented actor kind {:?} virtual LoadData+50; actor prefix retained",
                                self.current_kind
                            ));
                        }
                        let frames = ModelFrameLookup::borrow(arena, true);
                        let mut reader = ProgramSliceReader::new(field.payload);
                        let result = self.world.load_detector_data(
                            actor,
                            &mut reader,
                            replacements,
                            mission_path,
                            &frames,
                            backend,
                        );
                        self.cursor = field.offset + 6 + reader.position();
                        result.map_err(|e| format!("{e:?}"))?;
                        self.pending_load_data = None;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}
///544b2c..544b45 rereads current userdata after Init and AddActor. Existing
/// records are reused;547130 receives that same record and does not free it.
fn publish_actor_data(
    arena: &mut ModelArena,
    key: FrameKey,
    actor: ActorReferenceId,
) -> Result<(), String> {
    let record = match arena.user_data(key).map_err(|e| format!("{e:?}"))? {
        FrameUserData::Null => arena
            .create_actor_data_record(Some(actor))
            .map_err(|e| format!("{e:?}"))?,
        FrameUserData::ActorRecord(record) => {
            arena
                .set_actor_data_record(record, Some(actor))
                .map_err(|e| format!("{e:?}"))?;
            record
        }
        FrameUserData::Opaque | FrameUserData::ExternalRecord(_) => {
            return Err("unsupported mission frame userdata owner".into());
        }
    };
    arena
        .set_user_data(key, FrameUserData::ActorRecord(record))
        .map_err(|e| format!("{e:?}"))
}
fn valid_name(name: &[u8]) -> Result<(), String> {
    if name.is_ascii() && !name.contains(&0) {
        Ok(())
    } else {
        Err("unsupported actor map name domain".into())
    }
}
fn validate_frame(arena: &ModelArena, frame: FrameReferenceId) -> Result<FrameKey, String> {
    let key = FrameKey(frame.identity());
    if arena.lookup_reference(key) != frame {
        return Err("stale actor load-map frame".into());
    }
    arena.frame(key).map_err(|e| format!("{e:?}"))?;
    Ok(key)
}
struct ReleaseFrames<'a, 'b, B> {
    frames: ModelActorFrames<'a>,
    backend: &'b mut B,
}
impl<B: ActorDeallocationBackend> ActorDeallocationBackend for ReleaseFrames<'_, '_, B> {
    fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
        self.backend.free_actor(world, identity)
    }
}
impl<B> ActorFrameBackend for ReleaseFrames<'_, '_, B> {
    fn actor_data(
        &mut self,
        w: &mut ActorWorld,
        f: FrameReferenceId,
    ) -> Result<FrameActorData, String> {
        ActorFrameBackend::actor_data(&mut self.frames, w, f)
    }
    fn clear_actor_data(&mut self, w: &mut ActorWorld, f: FrameReferenceId) -> Result<(), String> {
        ActorFrameBackend::clear_actor_data(&mut self.frames, w, f)
    }
    fn find_child(
        &mut self,
        w: &mut ActorWorld,
        f: FrameReferenceId,
        n: &[u8],
        flags: u32,
    ) -> Result<Option<FrameReferenceId>, String> {
        ActorFrameBackend::find_child(&mut self.frames, w, f, n, flags)
    }
    fn set_child_enabled(
        &mut self,
        w: &mut ActorWorld,
        f: FrameReferenceId,
        e: u32,
    ) -> Result<(), String> {
        ActorFrameBackend::set_child_enabled(&mut self.frames, w, f, e)
    }
    fn release_frame(&mut self, w: &mut ActorWorld, f: FrameReferenceId) -> Result<(), String> {
        ActorFrameBackend::release_frame(&mut self.frames, w, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_loading::{tests::frame, MaterializedModel};
    use crate::original_program::{CProgram, ProgramAllocation, ProgramBuffer, ProgramSeed};
    use std::sync::Arc;
    #[derive(Default)]
    struct Storage {
        next: u32,
        railway_frees: usize,
    }
    fn seed() -> BaseActorSeed {
        BaseActorSeed {
            word_0e: 0,
            actor_id: 99,
            state_1c: 1,
            bytes_21_5f: [7; 63],
            bytes_64_65: [0; 2],
            byte_67: 0,
            release_frame: 1,
            bytes_6d_6f: [0; 3],
        }
    }
    impl ActorAllocationBackend for Storage {
        fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            assert_eq!(bytes, 0x70);
            self.next += 1;
            Ok(Some((self.next, seed())))
        }
        fn allocate_railway(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, crate::railway_actor::RailwaySeed)>, String>
        {
            assert_eq!(bytes, 0x370);
            self.next += 1;
            Ok(Some((
                self.next,
                seed(),
                crate::railway_actor::RailwaySeed {
                    bytes_70_36f: [0xa5; 0x300],
                },
            )))
        }
        fn allocate_detector(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
            assert_eq!(bytes, 0x128);
            self.next += 1;
            Ok(Some((
                self.next,
                seed(),
                DetectorSeed {
                    program: ProgramSeed {
                        address_high_byte: 0,
                        padding_0d: [0; 3],
                        words_1c_2c: [0; 5],
                        padding_31: [0; 3],
                        word_40: 0,
                        bytes_64: [0; 4],
                    },
                    bytes_d8_ef: [0; 24],
                    word_f4: 0,
                    bytes_105_107: [0; 3],
                    bytes_118_127: [0; 16],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Storage {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            panic!("source preallocated")
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            panic!("source preallocated")
        }
    }
    impl ActorDeallocationBackend for Storage {
        fn free_railway_allocation(
            &mut self,
            _: &ActorWorld,
            _: ActorReferenceId,
            _: crate::railway_actor::RailwayVectorSlot,
            allocation: Option<crate::railway_actor::RailwayAllocation>,
        ) -> Result<(), String> {
            assert_eq!(allocation, None);
            self.railway_frees += 1;
            Ok(())
        }
        fn free_actor(&mut self, _: &ActorWorld, _: u32) -> Result<(), String> {
            Ok(())
        }
    }
    impl ProgramBackend for Storage {
        fn allocate(
            &mut self,
            _: &CProgram,
            _: ProgramBuffer,
            _: u32,
        ) -> Result<ProgramAllocation, String> {
            self.next += 1;
            Ok(ProgramAllocation(self.next as u64))
        }
        fn free(
            &mut self,
            _: &CProgram,
            _: ProgramBuffer,
            _: Option<ProgramAllocation>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    fn chunk(kind: u16, b: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((b.len() + 6) as u32).to_le_bytes(),
            b,
        ]
        .concat()
    }
    fn input(fields: &[u8]) -> Vec<u8> {
        chunk(0x4c53, &chunk(0xae20, &chunk(0xae21, fields)))
    }
    fn collection(bytes: &[u8]) -> Chunk<'_> {
        scene2_root(bytes)
            .unwrap()
            .children()
            .next()
            .unwrap()
            .unwrap()
    }
    fn setup() -> (MissionActorLoader, MaterializedModel, FrameReferenceId) {
        let backing = SourceBacking {
            storage: Some(ActorStorageId(9)),
            capacity: 8,
        };
        let world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
        let mut f = frame(1, None, [0.; 3]);
        f.name_bytes = b"actual".to_vec();
        let model = MaterializedModel::from_parsed(Arc::new(lhr_assets::Model {
            materials: vec![],
            frames: vec![f],
        }))
        .unwrap();
        let r = ModelFrameLookup::borrow(model.arena(), true)
            .reference(model.source_frame_keys()[0])
            .unwrap();
        (
            MissionActorLoader::new(world, OriginalLoadMap::for_preceding_loader()),
            model,
            r,
        )
    }
    #[test]
    fn truncated_detector_payload_retains_registered_actor_and_real_frame() {
        let (mut loader, mut model, r) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", r)
            .unwrap();
        let bytes = input(
            &[
                chunk(0xae23, b"actual\0"),
                chunk(0xae22, &5u32.to_le_bytes()),
                chunk(0xae24, &[9, 8, 7]),
            ]
            .concat(),
        );
        let error = loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                Some(b"missions/test"),
                &[],
                &mut Storage::default(),
            )
            .unwrap_err();
        let a = error.actor.unwrap();
        assert!(error.detail.contains("detector_read"));
        assert_eq!(loader.world.source(ActorSource::Primary), [Some(a)]);
        assert!(loader.world.source(ActorSource::Active).is_empty());
        assert_eq!(loader.world.state(a).unwrap().byte_66, 1);
        assert_eq!(loader.world.state(a).unwrap().frame, Some(r));
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true)
                .actor_data(r)
                .unwrap(),
            FrameActorData::Present(Some(a))
        );
        assert_eq!(loader.pending_load_data, Some(vec![9, 8, 7]));
        assert_eq!(
            loader.current_record,
            collection(&bytes).children().next().unwrap().unwrap().raw
        );
    }
    #[test]
    fn default_factory_hole_does_not_hide_following_supported_actor_record() {
        let (mut loader, mut model, frame) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", frame)
            .unwrap();
        let record = |kind: u32| {
            chunk(
                0xae21,
                &[
                    chunk(0xae23, b"actual\0"),
                    chunk(0xae22, &kind.to_le_bytes()),
                    chunk(0xae24, b"ignored base payload"),
                ]
                .concat(),
            )
        };
        let bytes = chunk(0x4c53, &chunk(0xae20, &[record(3), record(1)].concat()));
        let mut storage = Storage::default();
        loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                None,
                &[],
                &mut storage,
            )
            .unwrap();
        let actor = loader.current_actor.unwrap();
        assert_eq!(storage.next, 1);
        assert_eq!(loader.world.actor_count, 1);
        assert_eq!(loader.world.source(ActorSource::Primary), [Some(actor)]);
        assert_eq!(loader.world.state(actor).unwrap().frame, Some(frame));
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true)
                .actor_data(frame)
                .unwrap(),
            FrameActorData::Present(Some(actor))
        );
        assert!(loader.interrupted_offset.is_none());
    }
    #[test]
    fn original_base_literal_load_data_leaves_opaque_payload_uninterpreted() {
        let (mut loader, mut model, r) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", r)
            .unwrap();
        let bytes = input(
            &[
                chunk(0xae23, b"actual\0"),
                chunk(0xae22, &1u32.to_le_bytes()),
                chunk(0xae24, &[255, 0, 9, 1, 8]),
            ]
            .concat(),
        );
        loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                None,
                &[],
                &mut Storage::default(),
            )
            .unwrap();
        let a = loader.current_actor.unwrap();
        assert_eq!(loader.world.state(a).unwrap().byte_66, 1);
        assert_eq!(loader.world.state(a).unwrap().word_10, 1);
        assert_eq!(loader.world.state(a).unwrap().frame, Some(r));
        assert!(loader.pending_load_data.is_none());
        assert_eq!(
            loader.current_record,
            collection(&bytes).children().next().unwrap().unwrap().raw
        );
    }
    #[test]
    fn userdata_publication_reuses_live_null_record_and_aliases() {
        let (mut loader, mut model, r) = setup();
        let a = loader
            .world
            .create_actor(1, &mut Storage::default())
            .unwrap()
            .unwrap();
        let key = validate_frame(model.arena(), r).unwrap();
        let record = model.arena_mut().create_actor_data_record(None).unwrap();
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::ActorRecord(record))
            .unwrap();
        publish_actor_data(model.arena_mut(), key, a).unwrap();
        assert_eq!(
            model.arena().user_data(key).unwrap(),
            FrameUserData::ActorRecord(record)
        );
        assert_eq!(model.arena().actor_data_record(record).unwrap(), Some(a));
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::Opaque)
            .unwrap();
        assert!(publish_actor_data(model.arena_mut(), key, a).is_err());
    }
    #[test]
    fn railway_record_uses_real_model_then_payload_and_derived_cleanup() {
        let backing = SourceBacking {
            storage: Some(ActorStorageId(1)),
            capacity: 8,
        };
        let world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let frame = ModelFrameLookup::borrow(&arena, true)
            .reference(model.root())
            .unwrap();
        let mut map = OriginalLoadMap::for_preceding_loader();
        map.publish_loaded_frame(&arena, b"vehicle", frame).unwrap();
        let mut loader = MissionActorLoader::new(world, map);
        let mut storage = Storage::default();
        let mut payload = vec![0];
        for value in [
            3u32, 0x41200000, 0x40c00000, 0x3fc00000, 0x3f400000, 0x40000000,
        ] {
            payload.extend(value.to_le_bytes());
        }
        let bytes = input(
            &[
                chunk(0xae23, b"vehicle\0"),
                chunk(0xae22, &8u32.to_le_bytes()),
                chunk(0xae24, &payload),
            ]
            .concat(),
        );
        loader
            .load_collection(
                collection(&bytes),
                &mut arena,
                Some(b"missions\\fixture"),
                &[],
                &mut storage,
            )
            .unwrap();
        let actor = loader.current_actor.unwrap();
        assert_eq!(
            loader.world.state(actor).unwrap().vtable,
            ActorVtable::Railway
        );
        assert_eq!(loader.world.state(actor).unwrap().frame, Some(frame));
        assert_eq!(loader.world.source(ActorSource::Primary), [Some(actor)]);
        assert_eq!(
            loader.world.railway(actor).unwrap().bytes()[0x1a8 - 0x70..0x1ac - 0x70],
            3u32.to_le_bytes()
        );
        assert_eq!(
            ModelActorFrames::borrow(&mut arena, true)
                .actor_data(frame)
                .unwrap(),
            FrameActorData::Present(Some(actor))
        );
        loader.world.unload_actor(Some(actor));
        loader
            .world
            .release_railway(
                actor,
                &mut ModelActorFrames::borrow(&mut arena, true),
                &mut storage,
            )
            .unwrap();
        assert_eq!(storage.railway_frees, 7);
        assert_eq!(loader.world.inspect(actor).unwrap().0, ActorPhase::Dead);
        assert_eq!(loader.world.actor_count, 0);
    }
    #[test]
    fn railway_wrong_frame_runs_derived_release_before_discarding_payload() {
        let (mut loader, mut model, frame) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", frame)
            .unwrap();
        let bytes = input(
            &[
                chunk(0xae23, b"actual\0"),
                chunk(0xae22, &8u32.to_le_bytes()),
                chunk(0xae24, b"not consumed"),
            ]
            .concat(),
        );
        let mut storage = Storage::default();
        loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                None,
                &[],
                &mut storage,
            )
            .unwrap();
        assert_eq!(storage.railway_frees, 7);
        assert!(loader.current_actor.is_none());
        assert_eq!(loader.world.actor_count, 0);
        assert!(loader.world.source(ActorSource::Primary).is_empty());
        assert!(loader.world.source(ActorSource::Static).is_empty());
        assert_eq!(
            ModelActorFrames::borrow(model.arena_mut(), true)
                .actor_data(frame)
                .unwrap(),
            FrameActorData::Null
        );
    }
    #[test]
    fn unsupported_next_factory_stops_at_kind_without_registering_later_actors() {
        let (mut loader, mut model, r) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", r)
            .unwrap();
        let bytes = input(
            &[
                chunk(0xae23, b"actual\0"),
                chunk(0xae22, &27u32.to_le_bytes()),
                chunk(0xae22, &5u32.to_le_bytes()),
            ]
            .concat(),
        );
        let mut storage = Storage::default();
        let error = loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                None,
                &[],
                &mut storage,
            )
            .unwrap_err();
        assert!(error.detail.contains("UnsupportedKind(27)"));
        assert_eq!(storage.next, 0);
        assert!(loader.world.source(ActorSource::Primary).is_empty());
        assert!(loader.world.source(ActorSource::Static).is_empty());
        assert!(loader.current_actor.is_none());
    }
    #[test]
    fn padded_frame_field_is_closed_before_live_userdata_admission_fails() {
        let (mut loader, mut model, frame) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", frame)
            .unwrap();
        let key = FrameKey(frame.identity());
        model
            .arena_mut()
            .set_user_data(key, FrameUserData::Opaque)
            .unwrap();
        let bytes = input(
            &[
                chunk(0xae23, b"actual\0unread padding"),
                chunk(0xae22, &1u32.to_le_bytes()),
            ]
            .concat(),
        );
        let record = collection(&bytes).children().next().unwrap().unwrap();
        let name = record.children().next().unwrap().unwrap();
        let mut storage = Storage::default();
        let error = loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                None,
                &[],
                &mut storage,
            )
            .unwrap_err();
        assert!(error.detail.contains("UnsupportedUserData"));
        assert_eq!(error.offset, name.offset);
        assert_eq!(loader.cursor, name.offset + name.raw.len());
        assert_eq!(loader.current_frame, Some(frame));
        assert_eq!(loader.interrupted_offset, Some(name.offset));
        assert_eq!(storage.next, 0);
        assert_eq!(loader.world.actor_count, 0);
        assert_eq!(model.arena().user_data(key).unwrap(), FrameUserData::Opaque);
        assert!(loader.current_actor.is_none());
        assert!(loader.current_kind.is_none());
    }
    #[test]
    fn missing_name_map_slot_never_constructs_metadata_actor() {
        let (mut loader, mut model, _) = setup();
        let bytes = input(
            &[
                chunk(0xae23, b"actual\0"),
                chunk(0xae22, &1u32.to_le_bytes()),
            ]
            .concat(),
        );
        let mut storage = Storage::default();
        loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                None,
                &[],
                &mut storage,
            )
            .unwrap();
        assert_eq!(storage.next, 0);
        assert!(loader.world.source(ActorSource::Primary).is_empty());
        assert_eq!(loader.load_map_slot(b"actual"), Some(None));
    }
    #[test]
    fn source_fields_are_executed_in_order_and_detector_owns_payload() {
        let (mut loader, mut model, r) = setup();
        loader
            .publish_loaded_frame(model.arena(), b"actual", r)
            .unwrap();
        // A kind before name has no frame and cannot allocate an actor. Payload
        // before construction has no actor. The later kind performs real Init.
        let mut payload = vec![1];
        payload.extend(0x12345678u32.to_le_bytes());
        payload.push(1);
        payload.extend(0u32.to_le_bytes());
        payload.extend(4u32.to_le_bytes());
        payload.extend(b"end\n");
        let bytes = input(
            &[
                chunk(0xae22, &5u32.to_le_bytes()),
                chunk(0xae24, b"ignored"),
                chunk(0xae23, b"actual\0"),
                chunk(0xae22, &5u32.to_le_bytes()),
                chunk(0xae24, &payload),
            ]
            .concat(),
        );
        loader
            .load_collection(
                collection(&bytes),
                model.arena_mut(),
                Some(b"missions/test"),
                &[],
                &mut Storage::default(),
            )
            .unwrap();
        let a = loader.current_actor.unwrap();
        let d = loader.world.detector(a).unwrap();
        assert_eq!(d.data_d8(), 0x12345678);
        assert_eq!(d.program().unwrap().owner(), Some(a));
        assert_eq!(
            d.program().unwrap().source().unwrap().c_bytes(),
            Some(&b"end\n"[..])
        );
        assert_eq!(loader.world.state(a).unwrap().byte_66, 1);
        assert!(loader.pending_load_data.is_none());
    }
}
