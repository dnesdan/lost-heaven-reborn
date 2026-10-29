//! Host composition of the source-backed scene2 actor and program arms.
//!
//! Actors attach to the caller's live arena and load map. Programs retain actual
//! CProgram source owners; no compiler, GameInit or scheduler is supplied here.
//! Allocation seeds use an explicit host zero-value policy, not observed
//! original heap bytes. Actor backing is a typed lifetime lease, not a byte
//! object. This composition receives no original-function credit.
use crate::actor_frames::ModelActorFrames;
use crate::actor_world::*;
use crate::entity_actor::{EntityResourceSlot, EntitySeed};
use crate::frame_lookup::ModelFrameLookup;
use crate::human_actor::{
    HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties, HumanResourceSlot, HumanSeed,
};
use crate::mission_actor_stream::{MissionActorLoader, OriginalLoadMap};
use crate::model_loading::ModelArena;
use crate::original_program::*;
use crate::player_actor::PlayerSeed;
use crate::railway_actor::{RailwayAllocation, RailwaySeed, RailwayVectorSlot};
use crate::script_scheduler::ActorReferenceId;
use lhr_assets::scene_actor_stream::Chunk;
use std::collections::BTreeMap;

/// One actual source owner shared by every reached actor/program root.
/// Terminal disposal is host cleanup of these source lifetimes, not GameDone.
pub struct MissionContentOwner {
    pub actors: MissionActorLoader,
    programs: Vec<CProgram>,
    pending_program: Option<CProgram>,
    memory: ContentMemory,
    mission_path: Option<Vec<u8>>,
    pub cursor: usize,
    stopped: bool,
    terminal_entered: bool,
    terminal_complete: bool,
}
impl Default for MissionContentOwner {
    fn default() -> Self {
        let empty = SourceBacking {
            storage: None,
            capacity: 0,
        };
        Self {
            actors: MissionActorLoader::borrowing_scene_map(
                ActorWorld::new(0, 0, empty, empty, empty).expect("empty actor collections"),
            ),
            programs: Vec::new(),
            pending_program: None,
            memory: ContentMemory::default(),
            mission_path: None,
            cursor: 0,
            stopped: false,
            terminal_entered: false,
            terminal_complete: false,
        }
    }
}
impl MissionContentOwner {
    /// Caller supplies the live mission path before root dispatch. The actual
    /// startup replacement owner is borrowed separately at each invocation.
    pub fn set_program_context(&mut self, path: Option<Vec<u8>>) -> Result<(), String> {
        if self.cursor != 0 || self.stopped || self.terminal_entered {
            return Err("mission content invocation already entered".into());
        }
        self.mission_path = path;
        Ok(())
    }
    pub fn mission_path(&self) -> Option<&[u8]> {
        self.mission_path.as_deref()
    }
    pub fn programs(&self) -> &[CProgram] {
        &self.programs
    }
    pub fn pending_program(&self) -> Option<&CProgram> {
        self.pending_program.as_ref()
    }
    pub fn live_allocation_count(&self) -> usize {
        self.memory.actors.len() + self.memory.collections.len() + self.memory.programs.len()
    }
    pub fn live_program_count(&self) -> usize {
        self.programs
            .iter()
            .chain(self.pending_program.iter())
            .filter(|program| program.phase() != ProgramPhase::Dead)
            .count()
    }
    pub fn terminal_complete(&self) -> bool {
        self.terminal_complete
    }
    pub fn load_actors(
        &mut self,
        root: Chunk<'_>,
        arena: &mut ModelArena,
        map: &mut OriginalLoadMap,
        patches: &crate::program_patch::ProgramPatchTable,
    ) -> Result<(), String> {
        self.available()?;
        let result = self.actors.load_collection_with_map(
            root,
            arena,
            map,
            self.mission_path.as_deref(),
            patches.replacements(),
            &mut self.memory,
        );
        self.cursor = self.actors.cursor;
        if let Err(error) = result {
            self.stopped = true;
            return Err(error.detail);
        }
        Ok(())
    }
    fn available(&self) -> Result<(), String> {
        if self.stopped || self.terminal_entered {
            Err("mission content loading is unavailable".into())
        } else {
            Ok(())
        }
    }
    pub fn load_programs(
        &mut self,
        root: Chunk<'_>,
        arena: &ModelArena,
        patches: &crate::program_patch::ProgramPatchTable,
    ) -> Result<(), String> {
        self.available()?;
        let result = self.load_programs_inner(root, arena, patches);
        if result.is_err() {
            self.stopped = true;
        }
        result
    }
    fn load_programs_inner(
        &mut self,
        root: Chunk<'_>,
        arena: &ModelArena,
        patches: &crate::program_patch::ProgramPatchTable,
    ) -> Result<(), String> {
        self.cursor = root.offset + 6;
        if root.kind != 0xae50 {
            return Err("expected script collection ae50".into());
        }
        for record in root.children() {
            let record = record.map_err(|error| error.to_string())?;
            self.cursor = record.offset + 6;
            if record.kind == 0xae51 {
                // Admission precedes construction so push cannot lose a loaded owner.
                if self.programs.len() >= crate::script_lookup::MAX_ACTOR_SOURCES {
                    return Err("host program source limit".into());
                }
                self.programs
                    .try_reserve(1)
                    .map_err(|_| "host program collection allocation")?;
                self.pending_program = Some(CProgram::construct(program_seed()));
                let mut reader = ProgramSliceReader::new(record.payload);
                let frames = ModelFrameLookup::borrow(arena, true);
                let mut context = ActorProgramContext {
                    world: &self.actors.world,
                    frames: &frames,
                    mission_path: self.mission_path.as_deref(),
                };
                let result = self.pending_program.as_mut().unwrap().load_source_code(
                    &mut reader,
                    patches.replacements(),
                    &mut context,
                    &mut self.memory,
                );
                self.cursor = record.offset + 6 + reader.position();
                result.map_err(|error| format!("{error:?}"))?;
                // Native push follows LoadSourceCode; an interrupted source
                // remains pending, never enters the mission program collection.
                self.programs.push(self.pending_program.take().unwrap());
            }
            self.cursor = record.offset + record.raw.len();
        }
        self.cursor = root.offset + root.raw.len();
        Ok(())
    }
    /// One terminal host disposal attempt, before the borrowed scene dies.
    /// Failures retain the reached owner/prefix and cannot replay destructors.
    pub fn discard_terminal(&mut self, arena: &mut ModelArena) -> Result<(), String> {
        if self.terminal_complete {
            return Ok(());
        }
        if self.terminal_entered {
            return Err("mission content disposal already entered".into());
        }
        self.terminal_entered = true;
        if let Some(program) = self.pending_program.as_mut() {
            program
                .destruct(&mut self.memory)
                .map_err(|error| format!("pending program disposal: {error:?}"))?;
        }
        for program in self.programs.iter_mut().rev() {
            program
                .destruct(&mut self.memory)
                .map_err(|error| format!("program disposal: {error:?}"))?;
        }
        // Identity worklist only. Each destructor rereads its live actor and
        // frame state; no decoded world or frame state is copied here.
        let actors: Vec<_> = self.actors.world.owned_actor_references().collect();
        for actor in actors.into_iter().rev() {
            let world = &mut self.actors.world;
            let vtable = world.state(actor).map_err(|e| format!("{e:?}"))?.vtable;
            let mut frames = ModelActorFrames::borrow(arena, true);
            match vtable {
                ActorVtable::Detector => {
                    world.delete_detector(actor, 1, &mut frames, &mut self.memory)
                }
                ActorVtable::Railway => {
                    world.delete_railway(actor, 1, &mut frames, &mut self.memory)
                }
                ActorVtable::Human => world.delete_human(actor, 1, &mut frames, &mut self.memory),
                ActorVtable::Player => world.delete_player(actor, 1, &mut frames, &mut self.memory),
                ActorVtable::Entity => world.delete_entity(actor, 1, &mut frames, &mut self.memory),
                ActorVtable::Base => world.delete_base(
                    actor,
                    1,
                    &mut ContentFrames {
                        frames,
                        memory: &mut self.memory,
                    },
                ),
                ActorVtable::Opaque(_) => return Err("unavailable derived actor disposal".into()),
            }
            .map_err(|error| format!("actor disposal: {error:?}"))?;
        }
        self.actors
            .world
            .discard_empty_source_backings(&mut self.memory)
            .map_err(|e| format!("{e:?}"))?;
        if self.live_allocation_count() != 0 {
            return Err("mission content backing remains owned".into());
        }
        self.terminal_complete = true;
        Ok(())
    }
}
fn program_seed() -> ProgramSeed {
    ProgramSeed {
        address_high_byte: 0,
        padding_0d: [0; 3],
        words_1c_2c: [0; 5],
        padding_31: [0; 3],
        word_40: 0,
        bytes_64: [0; 4],
    }
}
fn human_seed() -> HumanSeed {
    // Explicit host seed policy, not an assertion about original allocator
    // bytes. Constructor-unwritten current properties stay separate from the
    // constructor/loaded maxima; no GameInit copy or animation request occurs.
    HumanSeed {
        allocator_tag: 0,
        animation_machine: HumanAnimationMachineSeed { untouched_word: 0 },
        previous_animation_code: 0,
        requested_animation_code: 0,
        human_type_mask: 0,
        current_properties: HumanProperties::from_record([0; 64]),
        target_e: None,
        target_n: None,
        base_frame: None,
        auxiliary: std::array::from_fn(|_| HumanAuxiliarySeed {
            untouched_word: 0,
            padding: [0; 3],
            untouched_tail: [0; 16],
        }),
    }
}
fn base_seed() -> BaseActorSeed {
    BaseActorSeed {
        word_0e: 0,
        actor_id: 0,
        state_1c: 0,
        bytes_21_5f: [0; 63],
        bytes_64_65: [0; 2],
        byte_67: 0,
        release_frame: 0,
        bytes_6d_6f: [0; 3],
    }
}
/// Real CPU backing tracked independently from the logical typed owners.
#[derive(Default)]
struct ActorStorageLease {
    _native_extent: u32,
}
#[derive(Default)]
struct ContentMemory {
    next: u32,
    actors: BTreeMap<u32, ActorStorageLease>,
    collections: BTreeMap<u32, (ActorSource, Vec<u8>)>,
    programs: BTreeMap<u64, (ProgramBuffer, Vec<u8>)>,
}
impl ContentMemory {
    fn allocation(&mut self, bytes: u32) -> Result<(u32, Vec<u8>), String> {
        let next = self
            .next
            .checked_add(1)
            .ok_or("mission allocation identity exhausted")?;
        let mut backing = Vec::new();
        backing
            .try_reserve_exact(bytes as usize)
            .map_err(|_| "mission host allocation")?;
        backing.resize(bytes as usize, 0);
        self.next = next;
        Ok((next, backing))
    }
    fn actor(&mut self, bytes: u32, expected: u32) -> Result<u32, String> {
        if bytes != expected {
            return Err("invalid actor allocation size".into());
        }
        // The actual ordinary object lives in ActorWorld's typed owner. This
        // heap-boundary lease records the requested native extent; no byte
        // array shadows a Human/Player/Entity or any other actor.
        let identity = self
            .next
            .checked_add(1)
            .ok_or("mission allocation identity exhausted")?;
        self.next = identity;
        self.actors.insert(
            identity,
            ActorStorageLease {
                _native_extent: bytes,
            },
        );
        Ok(identity)
    }
}
impl ActorAllocationBackend for ContentMemory {
    fn allocate_player(
        &mut self,
        bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, PlayerSeed)>, String> {
        Ok(Some((
            self.actor(bytes, 0xb00)?,
            base_seed(),
            PlayerSeed {
                human: human_seed(),
                allocator_tag: 0,
            },
        )))
    }
    fn allocate_entity(
        &mut self,
        bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, EntitySeed)>, String> {
        Ok(Some((
            self.actor(bytes, 0x128c)?,
            base_seed(),
            EntitySeed {
                human: human_seed(),
                program: program_seed(),
                allocator_tag_a: 0,
                allocator_tag_b: 0,
                untouched_word: 0,
                untouched_padding: [0; 3],
            },
        )))
    }
    fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
        Ok(Some((self.actor(bytes, 0x70)?, base_seed())))
    }
    fn allocate_detector(
        &mut self,
        bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
        Ok(Some((
            self.actor(bytes, 0x128)?,
            base_seed(),
            DetectorSeed {
                program: program_seed(),
                bytes_d8_ef: [0; 24],
                word_f4: 0,
                bytes_105_107: [0; 3],
                bytes_118_127: [0; 16],
            },
        )))
    }
    fn allocate_railway(
        &mut self,
        bytes: u32,
    ) -> Result<Option<(u32, BaseActorSeed, RailwaySeed)>, String> {
        Ok(Some((
            self.actor(bytes, 0x370)?,
            base_seed(),
            RailwaySeed {
                bytes_70_36f: [0; 0x300],
            },
        )))
    }
}
impl ActorCollectionBackend for ContentMemory {
    fn allocate_collection(
        &mut self,
        _: &ActorWorld,
        source: ActorSource,
        bytes: u32,
    ) -> Result<ActorStorageId, String> {
        let (identity, backing) = self.allocation(bytes)?;
        self.collections.insert(identity, (source, backing));
        Ok(ActorStorageId(identity))
    }
    fn free_collection(
        &mut self,
        _: &ActorWorld,
        source: ActorSource,
        old: Option<ActorStorageId>,
    ) -> Result<(), String> {
        if let Some(old) = old {
            if !self
                .collections
                .get(&old.0)
                .is_some_and(|(owner, _)| *owner == source)
            {
                return Err("foreign mission actor collection allocation".into());
            }
            self.collections.remove(&old.0);
        }
        Ok(())
    }
}
impl ActorDeallocationBackend for ContentMemory {
    fn free_human_collection(
        &mut self,
        world: &ActorWorld,
        actor: ActorReferenceId,
        slot: HumanResourceSlot,
    ) -> Result<(), String> {
        let empty = if slot == HumanResourceSlot::Player {
            world
                .player(actor)
                .map_err(|e| format!("{e:?}"))?
                .collection
                .is_empty()
        } else {
            world
                .human(actor)
                .map_err(|e| format!("{e:?}"))?
                .collection_is_empty(slot)
        };
        if !empty {
            return Err("populated human collection disposal unavailable".into());
        }
        // Actual native free(NULL) has no allocation to release.
        Ok(())
    }
    fn free_entity_collection(
        &mut self,
        world: &ActorWorld,
        actor: ActorReferenceId,
        slot: EntityResourceSlot,
    ) -> Result<(), String> {
        if !world
            .entity(actor)
            .map_err(|e| format!("{e:?}"))?
            .collection_is_empty(slot)
        {
            return Err("populated entity collection disposal unavailable".into());
        }
        Ok(())
    }
    fn free_actor(&mut self, _: &ActorWorld, identity: u32) -> Result<(), String> {
        self.actors
            .remove(&identity)
            .map(|_| ())
            .ok_or_else(|| "foreign mission actor allocation".into())
    }
    fn free_railway_allocation(
        &mut self,
        _: &ActorWorld,
        _: ActorReferenceId,
        _: RailwayVectorSlot,
        allocation: Option<RailwayAllocation>,
    ) -> Result<(), String> {
        if allocation.is_some() {
            Err("unavailable railway vector backing owner".into())
        } else {
            Ok(())
        }
    }
}
impl ProgramBackend for ContentMemory {
    fn allocate(
        &mut self,
        _: &CProgram,
        buffer: ProgramBuffer,
        bytes: u32,
    ) -> Result<ProgramAllocation, String> {
        let (identity, backing) = self.allocation(bytes)?;
        self.programs.insert(identity as u64, (buffer, backing));
        Ok(ProgramAllocation(identity as u64))
    }
    fn free(
        &mut self,
        _: &CProgram,
        buffer: ProgramBuffer,
        allocation: Option<ProgramAllocation>,
    ) -> Result<(), String> {
        if let Some(allocation) = allocation {
            if !self
                .programs
                .get(&allocation.0)
                .is_some_and(|(owner, _)| *owner == buffer)
            {
                return Err("foreign mission program allocation".into());
            }
            self.programs.remove(&allocation.0);
        }
        Ok(())
    }
}
// Join actual frame effects with scalar free for the inherited deleting wrapper.
struct ContentFrames<'a, 'b> {
    frames: ModelActorFrames<'a>,
    memory: &'b mut ContentMemory,
}
impl ActorDeallocationBackend for ContentFrames<'_, '_> {
    fn free_actor(&mut self, world: &ActorWorld, identity: u32) -> Result<(), String> {
        self.memory.free_actor(world, identity)
    }
}
impl ActorFrameBackend for ContentFrames<'_, '_> {
    fn actor_data(
        &mut self,
        w: &mut ActorWorld,
        f: crate::script_scheduler::FrameReferenceId,
    ) -> Result<FrameActorData, String> {
        ActorFrameBackend::actor_data(&mut self.frames, w, f)
    }
    fn clear_actor_data(
        &mut self,
        w: &mut ActorWorld,
        f: crate::script_scheduler::FrameReferenceId,
    ) -> Result<(), String> {
        ActorFrameBackend::clear_actor_data(&mut self.frames, w, f)
    }
    fn find_child(
        &mut self,
        w: &mut ActorWorld,
        f: crate::script_scheduler::FrameReferenceId,
        n: &[u8],
        flags: u32,
    ) -> Result<Option<crate::script_scheduler::FrameReferenceId>, String> {
        ActorFrameBackend::find_child(&mut self.frames, w, f, n, flags)
    }
    fn set_child_enabled(
        &mut self,
        w: &mut ActorWorld,
        f: crate::script_scheduler::FrameReferenceId,
        e: u32,
    ) -> Result<(), String> {
        ActorFrameBackend::set_child_enabled(&mut self.frames, w, f, e)
    }
    fn release_frame(
        &mut self,
        w: &mut ActorWorld,
        f: crate::script_scheduler::FrameReferenceId,
    ) -> Result<(), String> {
        ActorFrameBackend::release_frame(&mut self.frames, w, f)
    }
}

#[cfg(test)]
mod human_source_tests {
    use super::*;
    use crate::actor_frames::FrameUserData;
    use crate::actor_world::{ActorPhase, ActorSource, ActorVtable};
    use crate::model_loading::tests::frame;
    use lhr_assets::scene_actor_stream::scene2_root;
    use std::sync::Arc;
    fn bind_actual_two_slot_model_controller(
        arena: &mut ModelArena,
        model: crate::model_loading::ModelKey,
    ) {
        use crate::model_animation_runtime::{
            ModelAnimationDestination, ModelControllerAllocation, ModelControllerAllocationFailure,
            ModelControllerFactory,
        };
        struct Factory;
        impl ModelControllerFactory for Factory {
            type Error = std::convert::Infallible;
            fn allocate(
                &mut self,
                owner: ModelAnimationDestination,
            ) -> Result<Option<ModelControllerAllocation>, Self::Error> {
                Ok(Some(ModelControllerAllocation::new(owner, false)))
            }
            fn allocation_failed(
                &mut self,
                _: ModelControllerAllocationFailure,
            ) -> Result<(), Self::Error> {
                unreachable!("fixture allocation")
            }
        }
        let mut payload = vec![1, 0, 10, 0];
        payload.extend(12u32.to_le_bytes());
        payload.extend(17u32.to_le_bytes());
        payload.extend(b"base\0");
        payload.extend(2u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        payload.extend(1u16.to_le_bytes());
        payload.extend(0u16.to_le_bytes());
        payload.extend([0; 12]);
        let mut bytes = b"5DS\0".to_vec();
        bytes.extend(20u16.to_le_bytes());
        bytes.extend([0; 8]);
        bytes.extend((payload.len() as u32).to_le_bytes());
        bytes.extend(payload);
        let parsed = Arc::new(lhr_assets::animation::parse_5ds(&bytes).unwrap());
        let resource = crate::animation_controller::LiveAnimationResource::new(parsed, 2).unwrap();
        for slot in [0, 1] {
            assert_eq!(
                arena
                    .set_original_model_animation(model, Some(&resource), slot, 1., 0, &mut Factory)
                    .unwrap(),
                0
            );
        }
    }
    fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
        [
            kind.to_le_bytes().as_slice(),
            &((payload.len() + 6) as u32).to_le_bytes(),
            payload,
        ]
        .concat()
    }
    fn input(kind: u32, payload: &[u8]) -> Vec<u8> {
        chunk(
            0x4c53,
            &chunk(
                0xae20,
                &chunk(
                    0xae21,
                    &[
                        chunk(0xae23, b"human-root\0"),
                        chunk(0xae22, &kind.to_le_bytes()),
                        chunk(0xae24, payload),
                    ]
                    .concat(),
                ),
            ),
        )
    }
    fn collection(bytes: &[u8]) -> Chunk<'_> {
        scene2_root(bytes)
            .unwrap()
            .children()
            .next()
            .unwrap()
            .unwrap()
    }
    fn actor_data(entity: bool) -> Vec<u8> {
        let mut bytes = vec![6];
        bytes.extend(0x12000001u32.to_le_bytes());
        bytes.extend(17u32.to_le_bytes());
        bytes.extend([0xff; 64]);
        if entity {
            bytes.extend([0xfe, 1]);
            bytes.extend(0u32.to_le_bytes());
            bytes.extend(4u32.to_le_bytes());
            bytes.extend(b"End;");
        }
        bytes
    }
    fn setup(
        root: bool,
    ) -> (
        ModelArena,
        crate::model_loading::ModelKey,
        OriginalLoadMap,
        Vec<crate::frame_transform::FrameKey>,
    ) {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let mut frames = Vec::new();
        for (kind, name) in [
            (7, b"targetE".as_slice()),
            (7, b"targetN".as_slice()),
            (6, b"base".as_slice()),
        ] {
            let mut f = frame(kind, None, [0.; 3]);
            f.name_bytes = name.to_vec();
            if kind == 7 {
                f.target = Some(lhr_assets::TargetRecord {
                    flags: 0,
                    frame_indices: vec![],
                });
            }
            frames.push(f);
        }
        let content = arena
            .load_parsed_into_model(
                model,
                Arc::new(lhr_assets::Model {
                    materials: vec![],
                    frames,
                }),
            )
            .unwrap();
        let keys = content.frames().to_vec();
        let key = if root { model.root() } else { keys[2] };
        let reference = arena.lookup_reference(key);
        let mut map = OriginalLoadMap::for_preceding_loader();
        map.publish_loaded_frame(&arena, b"human-root", reference)
            .unwrap();
        (arena, model, map, keys)
    }
    #[test]
    fn actual_model_player_and_entity_admission_publishes_userdata_after_real_init_and_owns_source()
    {
        for kind in [2, 27] {
            let (mut arena, model, mut map, keys) = setup(true);
            bind_actual_two_slot_model_controller(&mut arena, model);
            let mut owner = MissionContentOwner::default();
            let bytes = input(kind, &actor_data(kind == 27));
            owner
                .load_actors(
                    collection(&bytes),
                    &mut arena,
                    &mut map,
                    &crate::program_patch::ProgramPatchTable::default(),
                )
                .unwrap();
            let a = owner.actors.current_actor.unwrap();
            let world = &owner.actors.world;
            assert_eq!(
                world.state(a).unwrap().vtable,
                if kind == 2 {
                    ActorVtable::Player
                } else {
                    ActorVtable::Entity
                }
            );
            assert_eq!(
                world.state(a).unwrap().frame,
                Some(arena.lookup_reference(model.root()))
            );
            assert_eq!(world.state(a).unwrap().byte_66, 1);
            assert_eq!(world.source(ActorSource::Primary), &[Some(a)]);
            assert!(world.source(ActorSource::Active).is_empty());
            assert_eq!(world.human(a).unwrap().human_type_mask(), 0x12000001);
            assert_eq!(world.human(a).unwrap().voice_index(), 17);
            assert_eq!(world.human(a).unwrap().properties().record(), &[0xff; 64]);
            assert_eq!(
                world.human(a).unwrap().current_properties().record(),
                &[0; 64]
            );
            assert_eq!(world.human(a).unwrap().animation_machine().model(), None);
            assert_eq!(
                world.human(a).unwrap().base_frame(),
                Some(arena.lookup_reference(keys[2]))
            );
            for key in &keys[..2] {
                assert_eq!(arena.state(*key).unwrap().flags & 1, 0);
            }
            let FrameUserData::ActorRecord(record) = arena.user_data(model.root()).unwrap() else {
                panic!("actual record required")
            };
            assert_eq!(arena.actor_data_record(record).unwrap(), Some(a));
            if kind == 27 {
                let p = world.entity(a).unwrap().program().unwrap();
                assert_eq!(p.owner(), Some(a));
                assert_eq!(p.source().unwrap().c_bytes(), Some(b"End;".as_slice()));
                assert_eq!(owner.memory.programs.len(), 1);
            }
            assert_eq!(owner.memory.actors.len(), 1);
            owner.discard_terminal(&mut arena).unwrap();
            assert!(owner.terminal_complete());
            assert_eq!(owner.live_allocation_count(), 0);
            assert_eq!(owner.actors.world.inspect(a).unwrap().0, ActorPhase::Dead);
            let controller = arena.original_model_controller(model).unwrap().unwrap();
            assert!(!controller.state().slots[0].enabled);
            assert!(controller.state().slots[1].enabled);
            arena.close(model).unwrap();
        }
    }
    #[test]
    fn non_model_human_init_false_releases_actual_derived_owner_without_userdata_publication() {
        for kind in [2, 27] {
            let (mut arena, model, mut map, keys) = setup(false);
            let mut owner = MissionContentOwner::default();
            let bytes = input(kind, &actor_data(kind == 27));
            owner
                .load_actors(
                    collection(&bytes),
                    &mut arena,
                    &mut map,
                    &crate::program_patch::ProgramPatchTable::default(),
                )
                .unwrap();
            assert_eq!(owner.actors.current_actor, None);
            assert!(owner.actors.world.owned_actor_references().next().is_none());
            assert!(owner.actors.world.source(ActorSource::Static).is_empty());
            assert!(owner.actors.world.source(ActorSource::Primary).is_empty());
            assert_eq!(arena.user_data(keys[2]).unwrap(), FrameUserData::Null);
            assert!(owner.memory.actors.is_empty());
            owner.discard_terminal(&mut arena).unwrap();
            assert_eq!(owner.live_allocation_count(), 0);
            arena.close(model).unwrap();
        }
    }
    #[test]
    fn unsupported_older_human_data_retains_registered_actor_userdata_and_exact_read_prefix() {
        let (mut arena, model, mut map, _) = setup(true);
        let mut owner = MissionContentOwner::default();
        let bytes = input(2, &[4, 0x99, 0x88]);
        let root = collection(&bytes);
        let field = root
            .children()
            .next()
            .unwrap()
            .unwrap()
            .children()
            .nth(2)
            .unwrap()
            .unwrap();
        assert!(owner
            .load_actors(
                root,
                &mut arena,
                &mut map,
                &crate::program_patch::ProgramPatchTable::default()
            )
            .is_err());
        let a = owner.actors.current_actor.unwrap();
        assert_eq!(owner.cursor, field.offset + 7);
        assert_eq!(
            owner.actors.pending_load_data.as_deref(),
            Some([4, 0x99, 0x88].as_slice())
        );
        assert_eq!(
            owner.actors.world.phase(a).unwrap(),
            ActorPhase::LoadingInterrupted
        );
        assert_eq!(owner.actors.world.source(ActorSource::Primary), &[Some(a)]);
        let FrameUserData::ActorRecord(record) = arena.user_data(model.root()).unwrap() else {
            panic!("registered prefix")
        };
        assert_eq!(arena.actor_data_record(record).unwrap(), Some(a));
        owner.discard_terminal(&mut arena).unwrap();
        assert_eq!(owner.live_allocation_count(), 0);
        arena.close(model).unwrap();
    }
}
