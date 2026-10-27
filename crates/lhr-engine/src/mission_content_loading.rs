//! Host composition of the source-backed scene2 actor and program arms.
//!
//! Actors attach to the caller's live arena and load map. Programs retain actual
//! CProgram source owners; no compiler, GameInit or scheduler is supplied here.
//! CPU allocation seeds describe this host's zero-filled backing, not observed
//! original heap bytes. This composition receives no original-function credit.
use crate::actor_frames::ModelActorFrames;
use crate::actor_world::*;
use crate::frame_lookup::ModelFrameLookup;
use crate::mission_actor_stream::{MissionActorLoader, OriginalLoadMap};
use crate::model_loading::ModelArena;
use crate::original_program::*;
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
struct ContentMemory {
    next: u32,
    actors: BTreeMap<u32, Vec<u8>>,
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
        let (identity, backing) = self.allocation(bytes)?;
        self.actors.insert(identity, backing);
        Ok(identity)
    }
}
impl ActorAllocationBackend for ContentMemory {
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
