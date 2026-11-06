//! Actual scene/AE20/AE50 owners, not a second program or mock lookup world.
use super::*;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
#[allow(unused_imports)]
use crate::entity_actor::EntityProcessState;
use crate::itexture::{CpuTextureStorage, TextureImageError, TextureImageSource, TextureResources};
use crate::mission_scene_loading::MissionSceneLoader;
use crate::original_data_chunk::chunk;
use crate::original_program::{
    ProgramSliceReader, RuntimeEffect, RuntimeReferenceSlot, RuntimeStage,
};
use crate::program_parameters::ScalarParameter;
use crate::scene_resources::{SceneResources, SceneScreen};

struct Screen;
impl SceneScreen for Screen {
    type Error = ();
    fn screen_sx(&mut self) -> Result<i32, ()> {
        Ok(64)
    }
    fn screen_sy(&mut self) -> Result<i32, ()> {
        Ok(64)
    }
}
impl DriverSceneScreen for Screen {
    fn viewport_rectangle(&mut self) -> Result<[i32; 4], ()> {
        Ok([0, 0, 64, 64])
    }
}
struct Images;
impl TextureImageSource for Images {
    fn read_image(&mut self, _: &[u8]) -> Result<Vec<u8>, TextureImageError> {
        Err(TextureImageError::Missing)
    }
}
#[derive(Default)]
struct Effects {
    events: Vec<RuntimeEffect>,
    fail: Option<RuntimeEffect>,
}
impl RuntimeBackend for Effects {
    fn effect(
        &mut self,
        _: RuntimeView<'_>,
        effect: RuntimeEffect,
        _: Option<&ScalarParameter>,
    ) -> Result<(), String> {
        self.events.push(effect);
        if self.fail == Some(effect) {
            return Err("required fixture effect unavailable".into());
        }
        Ok(())
    }
}
fn locales() -> ProgramWorldLocales {
    ProgramWorldLocales {
        game_default_c_locale: true,
        ls3df_default_c_locale: true,
    }
}
#[derive(Default)]
struct Diagnostic {
    calls: usize,
    redirect: Option<(ActorReferenceId, FrameKey, ActorReferenceId)>,
    panic: bool,
    missing_backing: bool,
    redirect_scene: Option<FrameReferenceId>,
}
impl MissionProgramDiagnostic for Diagnostic {
    fn message_beep(
        &mut self,
        world: &mut ActorWorld,
        arena: &mut ModelArena,
        mut fields: RuntimeView<'_>,
        owner: &mut Option<ActorReferenceId>,
        scene: &mut FrameReferenceId,
        argument: i32,
    ) -> Result<(), LookupError> {
        assert_eq!(argument, -1);
        self.calls += 1;
        if let Some(reference) = self.redirect_scene {
            *scene = reference;
        }
        if let Some((replacement, frame, removed)) = self.redirect {
            fields.set_program_counter(3);
            *fields.actor_count = 2;
            *fields.actors = (!self.missing_backing)
                .then(|| vec![RuntimeReferenceSlot::Empty; 2].into_boxed_slice());
            *owner = Some(replacement);
            assert!(world.unload_actor(Some(removed)));
            arena.set_frame_name(frame, Some(b"changed")).unwrap();
        }
        if self.panic {
            panic!("actual mission diagnostic mutation then unwind");
        }
        Ok(())
    }
}
fn program_record(name: &[u8], source: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0];
    bytes.extend((name.len() as u32).to_le_bytes());
    bytes.extend(name);
    bytes.extend((source.len() as u32).to_le_bytes());
    bytes.extend(source);
    bytes
}
struct Fixture {
    arena: ModelArena,
    scene: SceneKey,
    scene_reference: FrameReferenceId,
    loader: MissionSceneLoader,
    driver: DriverSceneOwner,
    resources: SceneResources,
    textures: TextureResources,
    storage: CpuTextureStorage,
    roots: [FrameKey; 3],
    actors: [ActorReferenceId; 3],
    effects: Effects,
}
impl Fixture {
    fn new(programs: &[(&[u8], &[u8])]) -> Self {
        let mut arena = ModelArena::default();
        let mut resources = SceneResources::default();
        let mut textures = TextureResources::new().unwrap();
        let mut storage = CpuTextureStorage::default();
        let mut driver = DriverSceneOwner::default();
        let scene = driver
            .create_scene(
                &mut DriverSceneContext {
                    arena: &mut arena,
                    resources: &mut resources,
                    textures: &mut textures,
                    storage: &mut storage,
                },
                &mut Screen,
                &mut Images,
                NativeSceneSoundDriver::Absent,
            )
            .unwrap();
        let scene_reference = arena.lookup_reference(scene.root());
        let primary = arena.scene(scene).unwrap().sectors[0];
        let names = [b"alpha".as_slice(), b"beta", b"chcipnuti chlapka"];
        let roots = std::array::from_fn(|index| {
            let model = arena.create_model().unwrap();
            arena
                .set_frame_name(model.root(), Some(names[index]))
                .unwrap();
            arena.scene_add_frame(scene, model.root()).unwrap();
            arena
                .link_frame(model.root(), Some(primary.frame()), 0)
                .unwrap();
            arena.release(model.root()).unwrap();
            model.root()
        });
        let actor_records: Vec<_> = names
            .iter()
            .map(|name| {
                let mut name = name.to_vec();
                name.push(0);
                chunk(
                    0xae21,
                    &[chunk(0xae23, &name), chunk(0xae22, &1u32.to_le_bytes())].concat(),
                )
            })
            .collect();
        let programs: Vec<_> = programs
            .iter()
            .map(|(name, source)| chunk(0xae51, &program_record(name, source)))
            .collect();
        let source = chunk(
            0x4c53,
            &[
                chunk(1, &[5, 0, 1, 0]),
                chunk(0xae20, &actor_records.concat()),
                chunk(0xae50, &programs.concat()),
            ]
            .concat(),
        );
        let mut loader = MissionSceneLoader::new(&arena, scene)
            .map_err(|error| error.detail)
            .unwrap();
        loader
            .load_prefix(
                &mut driver,
                &mut arena,
                &resources,
                &mut Screen,
                crate::game_camera_projection::GameCameraProjection {
                    global_mode: 0,
                    screen_flag_46f4: false,
                },
                &source,
            )
            .unwrap();
        let actors: Vec<_> = loader
            .content
            .actors
            .world
            .source(ActorSource::Primary)
            .iter()
            .copied()
            .map(Option::unwrap)
            .collect();
        let actors = actors.try_into().unwrap();
        Self {
            arena,
            scene,
            scene_reference,
            loader,
            driver,
            resources,
            textures,
            storage,
            roots,
            actors,
            effects: Default::default(),
        }
    }
    fn init(&mut self) {
        self.loader
            .content
            .global_program_game_init(&mut self.effects)
            .unwrap();
    }
    fn process(
        &mut self,
        index: usize,
        diagnostic: &mut Diagnostic,
    ) -> Result<(), MissionGlobalProgramError> {
        self.loader.content.process_program_in_world(
            index,
            7,
            &mut DispatchBudget::new(32).unwrap(),
            &mut MissionProgramWorldContext::new(
                &mut self.arena,
                &mut self.scene_reference,
                &mut self.effects,
                diagnostic,
                locales(),
            ),
        )
    }
    fn finish(mut self) {
        if self.loader.content.programs().iter().all(|program| {
            program.runtime().is_some_and(|runtime| {
                matches!(
                    runtime.stage,
                    RuntimeStage::Ready | RuntimeStage::ProcessInterrupted
                )
            })
        }) {
            self.loader
                .content
                .global_program_game_done(&mut self.effects)
                .unwrap();
        }
        self.loader
            .content
            .discard_global_program_runtimes_host()
            .unwrap();
        self.loader
            .content
            .discard_terminal(&mut self.arena)
            .unwrap();
        assert_eq!(self.loader.content.live_allocation_count(), 0);
        self.resources
            .close_constructed_scene(
                &mut self.arena,
                self.scene,
                &mut self.textures,
                &mut self.storage,
            )
            .unwrap();
        self.driver
            .release_scene(
                &mut DriverSceneContext {
                    arena: &mut self.arena,
                    resources: &mut self.resources,
                    textures: &mut self.textures,
                    storage: &mut self.storage,
                },
                self.scene,
            )
            .unwrap();
        self.driver.release_empty_registry_storage().unwrap();
        assert_eq!(self.arena.live_frame_count(), 0);
    }
}

#[test]
fn scene_source_globals_lookup_their_actual_loaded_actors_and_frames() {
    let mut f = Fixture::new(&[(b"main", b"commandblock 1\ndim_act 1\ndim_frm 2\nfindactor 0,\"alpha\"\nfindframe 0,\"beta\"\nfindframe 1,\"missing\"\ncommandblock 0")]);
    let source = f.loader.content.programs()[0]
        .source()
        .unwrap()
        .bytes
        .clone();
    f.init();
    f.process(0, &mut Diagnostic::default()).unwrap();
    let fields = &f.loader.content.programs()[0].runtime().unwrap().fields;
    assert_eq!(
        fields.actors.as_ref().unwrap()[0].reference(),
        Ok(Some(&f.actors[0]))
    );
    assert_eq!(
        fields.frames.as_ref().unwrap()[0].reference(),
        Ok(Some(&f.arena.lookup_reference(f.roots[1])))
    );
    assert_eq!(fields.frames.as_ref().unwrap()[1].reference(), Ok(None));
    assert_eq!(fields.program_counter(), 7);
    assert_eq!(
        f.loader.content.programs()[0].source().unwrap().bytes,
        source
    );
    let crate::actor_frames::FrameUserData::ActorRecord(record) =
        f.arena.user_data(f.roots[0]).unwrap()
    else {
        panic!("actual source actor record")
    };
    assert_eq!(
        f.arena.actor_data_record(record).unwrap(),
        Some(f.actors[0])
    );
    f.finish();
}

#[test]
fn diagnostic_mutates_actual_scene_world_and_instruction_before_lookup_rereads() {
    let mut f = Fixture::new(&[(b"main", b"dim_frm 1\ndim_act 1\nfindactor 0,\"unread\"\nfindactor 1,\"changed\"\nfindframe 0,\"changed\"")]);
    f.loader.content.programs[0]
        .set_owner(Some(f.actors[2]))
        .unwrap();
    f.init();
    let mut diagnostic = Diagnostic {
        redirect: Some((f.actors[1], f.roots[1], f.actors[0])),
        ..Default::default()
    };
    f.process(0, &mut diagnostic).unwrap();
    f.process(0, &mut diagnostic).unwrap();
    f.process(0, &mut diagnostic).unwrap();
    let runtime = f.loader.content.programs()[0].runtime().unwrap();
    assert_eq!(runtime.fields.program_counter(), 4);
    assert_eq!(runtime.fields.actor_count, 2);
    assert_eq!(
        runtime.fields.actors.as_ref().unwrap()[1].reference(),
        Ok(Some(&f.actors[1]))
    );
    assert_eq!(f.loader.content.programs()[0].owner(), Some(f.actors[1]));
    assert_eq!(f.arena.frame(f.roots[1]).unwrap().name_bytes, b"changed");
    assert!(!f
        .loader
        .content
        .actors
        .world
        .source(ActorSource::Primary)
        .contains(&Some(f.actors[0])));
    f.process(0, &mut diagnostic).unwrap();
    assert_eq!(
        f.loader.content.programs()[0]
            .runtime()
            .unwrap()
            .fields
            .frames
            .as_ref()
            .unwrap()[0]
            .reference(),
        Ok(Some(&f.arena.lookup_reference(f.roots[1])))
    );
    assert_eq!(diagnostic.calls, 1);
    f.finish();
}

#[test]
fn diagnostic_unwind_retains_scene_world_and_runtime_mutations_without_replay() {
    let mut f = Fixture::new(&[(
        b"main",
        b"dim_frm 1\ndim_act 1\nfindactor 0,\"unread\"\nfindactor 1,\"changed\"",
    )]);
    f.loader.content.programs[0]
        .set_owner(Some(f.actors[2]))
        .unwrap();
    f.init();
    let mut diagnostic = Diagnostic {
        redirect: Some((f.actors[1], f.roots[1], f.actors[0])),
        panic: true,
        redirect_scene: Some(
            f.arena
                .lookup_reference(f.arena.scene(f.scene).unwrap().sectors[0].frame()),
        ),
        ..Default::default()
    };
    f.process(0, &mut diagnostic).unwrap();
    f.process(0, &mut diagnostic).unwrap();
    assert!(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
        || f.process(0, &mut diagnostic)
    ))
    .is_err());
    let runtime = f.loader.content.programs()[0].runtime().unwrap();
    assert_eq!(runtime.stage, RuntimeStage::ProcessInterrupted);
    assert_eq!(
        (
            runtime.fields.program_counter(),
            runtime.fields.actor_count,
            runtime.fields.elapsed
        ),
        (3, 2, 7)
    );
    assert_eq!(f.loader.content.programs()[0].owner(), Some(f.actors[1]));
    assert_eq!(f.arena.frame(f.roots[1]).unwrap().name_bytes, b"changed");
    assert!(!f
        .loader
        .content
        .actors
        .world
        .source(ActorSource::Primary)
        .contains(&Some(f.actors[0])));
    assert!(f.process(0, &mut diagnostic).is_err());
    assert_eq!(diagnostic.calls, 1);
    assert_eq!(f.scene_reference, diagnostic.redirect_scene.unwrap());
    f.finish();
}

#[test]
fn foreign_scene_identity_is_rejected_only_when_named_frame_lookup_reaches_scene() {
    let mut f = Fixture::new(&[(b"main", b"dim_frm 1\nfindframe 0,\"alpha\"")]);
    let mut other = Fixture::new(&[]);
    f.init();
    let mut diagnostic = Diagnostic::default();
    let foreign = f.scene_reference;
    {
        let mut context = MissionProgramWorldContext::new(
            &mut other.arena,
            &mut f.scene_reference,
            &mut f.effects,
            &mut diagnostic,
            locales(),
        );
        // DIM has no Scene dependency, even though the supplied Scene is foreign.
        f.loader
            .content
            .process_program_in_world(0, 7, &mut DispatchBudget::new(8).unwrap(), &mut context)
            .unwrap();
        assert!(
            matches!(f.loader.content.process_program_in_world(0, 7, &mut DispatchBudget::new(8).unwrap(), &mut context),
        Err(MissionGlobalProgramError::Runtime(RuntimeError::Lookup(LookupError::Frame(FrameLookupError::StaleFrame(reference))))) if reference == foreign)
        );
    }
    other.finish();
    f.finish();
}

#[test]
fn required_effect_failure_is_forwarded_before_a_missing_scene_is_read() {
    let mut f = Fixture::new(&[(b"main", b"dim_frm 1")]);
    f.init();
    f.effects.fail = Some(RuntimeEffect::AllocateFrames(1));
    // A same-arena sector is not a Scene. DIM must not inspect it at all.
    f.scene_reference = f
        .arena
        .lookup_reference(f.arena.scene(f.scene).unwrap().sectors[0].frame());
    let error = f.process(0, &mut Diagnostic::default()).unwrap_err();
    assert!(format!("{error:?}").contains("required fixture effect unavailable"));
    let fields = &f.loader.content.programs()[0].runtime().unwrap().fields;
    assert_eq!(
        (fields.program_counter(), fields.frame_count, fields.elapsed),
        (0, 1, 7)
    );
    assert!(fields.frames.is_none());
    assert_eq!(
        f.effects.events.last(),
        Some(&RuntimeEffect::AllocateFrames(1))
    );
    f.effects.fail = None;
    f.finish();
}

#[test]
fn a_sector_reference_cannot_masquerade_as_the_actual_scene_owner() {
    let mut f = Fixture::new(&[(b"main", b"dim_frm 1\nfindframe 0,\"alpha\"")]);
    f.init();
    f.scene_reference = f
        .arena
        .lookup_reference(f.arena.scene(f.scene).unwrap().sectors[0].frame());
    f.process(0, &mut Diagnostic::default()).unwrap();
    let error = f.process(0, &mut Diagnostic::default()).unwrap_err();
    assert!(
        matches!(error, MissionGlobalProgramError::Runtime(RuntimeError::Lookup(LookupError::Backend(ref detail))) if detail.starts_with("program Scene:"))
    );
    assert_eq!(
        f.loader.content.programs()[0]
            .runtime()
            .unwrap()
            .fields
            .program_counter(),
        1
    );
    f.finish();
}

struct GamePrograms {
    programs: Vec<CProgram>,
    memory: super::super::ContentMemory,
    swap_mission: bool,
    unavailable: bool,
    redirect_scene: Option<FrameReferenceId>,
}
impl GamePrograms {
    fn new(sources: &[(&[u8], &[u8])], effects: &mut Effects) -> Self {
        let mut memory = super::super::ContentMemory::default();
        let mut programs = Vec::new();
        for (name, source) in sources {
            let mut program = CProgram::construct(super::super::program_seed());
            program
                .load_source_code(
                    &mut ProgramSliceReader::new(&program_record(name, source)),
                    &[],
                    &mut EmptyContext,
                    &mut memory,
                )
                .unwrap();
            program.init_fresh_runtime(effects).unwrap();
            programs.push(program);
        }
        Self {
            programs,
            memory,
            swap_mission: false,
            unavailable: false,
            redirect_scene: None,
        }
    }
    fn finish(mut self, effects: &mut Effects) {
        for program in self.programs.iter_mut().rev() {
            program.done_fresh_runtime(effects).unwrap();
            program.discard_runtime_host().unwrap();
            program.destruct(&mut self.memory).unwrap();
        }
        assert!(self.memory.programs.is_empty());
    }
}
struct EmptyContext;
impl crate::original_program::ProgramContext for EmptyContext {
    fn default_c_locale(&self) -> bool {
        true
    }
    fn mission_path(&mut self) -> Result<Option<&[u8]>, String> {
        panic!("no replacement records")
    }
    fn owner_frame_name(
        &mut self,
        _: Option<ActorReferenceId>,
    ) -> Result<crate::original_program::OwnerFrameName<'_>, String> {
        panic!("no replacement records")
    }
}
impl<D: MissionProgramDiagnostic> GameGlobalProgramWorldRunner<Effects, D> for GamePrograms {
    fn run_game_global(
        &mut self,
        mission: &mut MissionContentOwner,
        name: &[u8],
        budget: &mut DispatchBudget,
        context: &mut MissionProgramWorldContext<'_, Effects, D>,
    ) -> Result<bool, String> {
        if self.unavailable {
            return Err("actual game owner unavailable".into());
        }
        // This is an explicitly supplied source-owning fixture, not C_game.
        for index in (0..self.programs.len()).rev() {
            if self.programs[index]
                .name()
                .unwrap()
                .c_bytes()
                .unwrap()
                .eq_ignore_ascii_case(name)
            {
                context
                    .call_subroutine(&mut self.programs[index], &mut mission.actors.world, budget)
                    .map_err(|error| format!("{error:?}"))?;
                return Ok(true);
            }
        }
        if self.swap_mission {
            mission.programs.swap(0, 1);
        }
        if let Some(reference) = self.redirect_scene {
            *context.scene_mut() = reference;
        }
        Ok(false)
    }
}
#[test]
fn game_global_first_then_current_mission_reverse_priority_use_the_same_world() {
    let first = b"commandblock 1\ndim_frm 1\nfindframe 0,\"alpha\"\ncommandblock 0".as_slice();
    let last = b"commandblock 1\ndim_frm 1\nfindframe 0,\"beta\"\ncommandblock 0".as_slice();
    let mut f = Fixture::new(&[(b"target", first), (b"TARGET", last)]);
    f.init();
    let mut game = GamePrograms::new(&[(b"TaRgEt", first)], &mut f.effects);
    let mut diagnostic = Diagnostic::default();
    let run = |f: &mut Fixture, game: &mut GamePrograms, diagnostic: &mut Diagnostic| {
        f.loader.content.global_program_run_in_world(
            b"target",
            game,
            &mut DispatchBudget::new(32).unwrap(),
            &mut MissionProgramWorldContext::new(
                &mut f.arena,
                &mut f.scene_reference,
                &mut f.effects,
                diagnostic,
                locales(),
            ),
        )
    };
    assert_eq!(
        run(&mut f, &mut game, &mut diagnostic),
        Ok(NamedGlobalProgramOutcome::GameHandled)
    );
    assert!(f.loader.content.programs().iter().all(|p| p
        .runtime()
        .unwrap()
        .fields
        .frames
        .is_none()));
    assert_eq!(
        game.programs[0]
            .runtime()
            .unwrap()
            .fields
            .frames
            .as_ref()
            .unwrap()[0]
            .reference(),
        Ok(Some(&f.arena.lookup_reference(f.roots[0])))
    );
    game.finish(&mut f.effects);
    let mut game = GamePrograms::new(&[], &mut f.effects);
    game.swap_mission = true;
    assert_eq!(
        run(&mut f, &mut game, &mut diagnostic),
        Ok(NamedGlobalProgramOutcome::MissionHandled { index: 1 })
    );
    assert_eq!(
        f.loader.content.programs()[1]
            .runtime()
            .unwrap()
            .fields
            .frames
            .as_ref()
            .unwrap()[0]
            .reference(),
        Ok(Some(&f.arena.lookup_reference(f.roots[0])))
    );
    assert!(f.loader.content.programs()[0]
        .runtime()
        .unwrap()
        .fields
        .frames
        .is_none());
    game.finish(&mut f.effects);
    f.finish();
}
#[test]
fn unavailable_game_owner_never_enters_mission_fallback() {
    let mut f = Fixture::new(&[(b"target", b"dim_frm 1")]);
    f.init();
    let mut game = GamePrograms::new(&[], &mut f.effects);
    game.unavailable = true;
    assert!(matches!(
        f.loader.content.global_program_run_in_world(
            b"target",
            &mut game,
            &mut DispatchBudget::new(8).unwrap(),
            &mut MissionProgramWorldContext::new(
                &mut f.arena,
                &mut f.scene_reference,
                &mut f.effects,
                &mut Diagnostic::default(),
                locales()
            )
        ),
        Err(NamedGlobalProgramError::GameOwner(_))
    ));
    assert!(f.loader.content.programs()[0]
        .runtime()
        .unwrap()
        .fields
        .frames
        .is_none());
    game.finish(&mut f.effects);
    f.finish();
}

#[test]
fn game_callback_redirects_the_same_scene_slot_before_mission_lookup() {
    let mut f = Fixture::new(&[(
        b"target",
        b"commandblock 1\ndim_frm 1\nfindframe 0,\"alpha\"\ncommandblock 0",
    )]);
    f.init();
    let sector = f
        .arena
        .lookup_reference(f.arena.scene(f.scene).unwrap().sectors[0].frame());
    let mut game = GamePrograms::new(&[], &mut f.effects);
    game.redirect_scene = Some(sector);
    let error = f
        .loader
        .content
        .global_program_run_in_world(
            b"target",
            &mut game,
            &mut DispatchBudget::new(16).unwrap(),
            &mut MissionProgramWorldContext::new(
                &mut f.arena,
                &mut f.scene_reference,
                &mut f.effects,
                &mut Diagnostic::default(),
                locales(),
            ),
        )
        .unwrap_err();
    assert!(
        matches!(error, NamedGlobalProgramError::Runtime { error: RuntimeError::Lookup(LookupError::Backend(ref detail)), .. } if detail.starts_with("program Scene:"))
    );
    assert_eq!(f.scene_reference, sector);
    assert_eq!(
        f.loader.content.programs()[0]
            .runtime()
            .unwrap()
            .fields
            .program_counter(),
        2
    );
    game.finish(&mut f.effects);
    f.finish();
}

#[test]
fn tick_entity_programs_steps_owned_live_state_through_context() {
    use super::{MissionProgramWorldContext, ProgramWorldLocales};
    use crate::actor_world::{
        ActorAllocationBackend, ActorCollectionBackend, ActorSource, ActorStorageId, ActorWorld,
        BaseActorSeed, SourceBacking,
    };
    #[allow(unused_imports)]
    use crate::entity_actor::EntityProcessState;
    use crate::entity_actor::EntitySeed;
    use crate::human_actor::{
        HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties, HumanSeed,
    };
    use crate::model_loading::ModelArena;
    use crate::original_program::{ProgramAllocation, ProgramBackend, ProgramBuffer, ProgramSeed};
    use crate::script_scheduler::{ActorReferenceId, DispatchBudget, FrameReferenceId};

    struct Heap {
        next: u32,
        program_next: u64,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_entity(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, EntitySeed)>, String> {
            assert_eq!(bytes, 0x128c);
            self.next += 1;
            Ok(Some((
                self.next,
                BaseActorSeed {
                    word_0e: 0,
                    actor_id: 0,
                    state_1c: 0,
                    bytes_21_5f: [0; 63],
                    bytes_64_65: [0; 2],
                    byte_67: 0,
                    release_frame: 0,
                    bytes_6d_6f: [0; 3],
                },
                EntitySeed {
                    human: HumanSeed {
                        allocator_tag: 0,
                        animation_machine: HumanAnimationMachineSeed { untouched_word: 0 },
                        previous_animation_code: 0,
                        requested_animation_code: 0,
                        human_type_mask: 0,
                        current_properties: HumanProperties::from_record([0; 64]),
                        target_e: None,
                        target_n: None,
                        base_frame: None,
                        auxiliary: [
                            HumanAuxiliarySeed {
                                untouched_word: 0,
                                padding: [0; 3],
                                untouched_tail: [0; 16],
                            },
                            HumanAuxiliarySeed {
                                untouched_word: 0,
                                padding: [0; 3],
                                untouched_tail: [0; 16],
                            },
                        ],
                    },
                    program: ProgramSeed {
                        address_high_byte: 0,
                        padding_0d: [0; 3],
                        words_1c_2c: [0; 5],
                        padding_31: [0; 3],
                        word_40: 0,
                        bytes_64: [0; 4],
                    },
                    allocator_tag_a: 0,
                    allocator_tag_b: 0,
                    untouched_word: 0,
                    untouched_padding: [0; 3],
                },
            )))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<crate::actor_world::ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ProgramBackend for Heap {
        fn allocate(
            &mut self,
            _: &crate::original_program::CProgram,
            _: ProgramBuffer,
            _: u32,
        ) -> Result<ProgramAllocation, String> {
            self.program_next += 1;
            Ok(ProgramAllocation(self.program_next))
        }
        fn free(
            &mut self,
            _: &crate::original_program::CProgram,
            _: ProgramBuffer,
            _: Option<ProgramAllocation>,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    struct NoopDiagnostic;
    impl super::MissionProgramDiagnostic for NoopDiagnostic {
        fn message_beep(
            &mut self,
            _: &mut ActorWorld,
            _: &mut ModelArena,
            _: crate::original_program::RuntimeView<'_>,
            _: &mut Option<ActorReferenceId>,
            _: &mut FrameReferenceId,
            _: i32,
        ) -> Result<(), crate::script_lookup::LookupError> {
            Ok(())
        }
    }

    let backing = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 16,
    };
    let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
    let mut heap = Heap {
        next: 0,
        program_next: 0,
    };
    let actor = world.create_actor(27, &mut heap).unwrap().unwrap();
    let mut program = world.take_entity_program(actor).unwrap().unwrap();
    program
        .set_source_code(Some(b"enemy_vidim 1\nenemy_wait"), &mut heap)
        .unwrap();
    program.set_owner(Some(actor)).unwrap();
    let mut effects = Effects::default();
    world.return_entity_program(actor, Some(program)).unwrap();
    world
        .store_entity_process_state(
            actor,
            &EntityProcessState {
                process_flags: 0,
                visibility_bits: 3,
                shut_up: 0,
            },
        )
        .unwrap();
    // Exercise the Ready gate through the public phase query.
    assert_eq!(
        world.phase(actor),
        Ok(crate::actor_world::ActorPhase::Ready)
    );
    let mut arena = ModelArena::default();
    let mut scene = FrameReferenceId::from_existing(1);
    let mut diagnostic = NoopDiagnostic;
    let mut context = MissionProgramWorldContext::new(
        &mut arena,
        &mut scene,
        &mut effects,
        &mut diagnostic,
        ProgramWorldLocales {
            game_default_c_locale: true,
            ls3df_default_c_locale: true,
        },
    );
    // The init pass compiles the sourceless-runtime program and leaves
    // initialized programs untouched.
    assert_eq!(context.init_entity_programs(&mut world), Ok(1));
    assert!(world
        .entity(actor)
        .unwrap()
        .program()
        .is_some_and(|program| program.runtime().is_some()));
    assert_eq!(context.init_entity_programs(&mut world), Ok(0));
    let mut budget = DispatchBudget::new(8).unwrap();
    assert_eq!(
        context.tick_entity_programs(&mut world, 1, &mut budget),
        Ok(1)
    );
    // VIDIM "1" cleared the blind bit and advanced past the first line; the
    // program is back inside its authoritative owner.
    assert_eq!(
        world.entity_process_state(actor).unwrap(),
        EntityProcessState {
            process_flags: 0,
            visibility_bits: 1,
            shut_up: 0,
        }
    );
    assert!(world.entity(actor).unwrap().program().is_some());
    // The second tick parks on the fresh WAIT without advancing.
    assert_eq!(
        context.tick_entity_programs(&mut world, 1, &mut budget),
        Ok(1)
    );
}

#[test]
fn lookup_world_maps_type_mismatch_silently_and_propagates_stale() {
    use super::MissionLookupWorld;
    use crate::actor_world::{
        ActorAllocationBackend, ActorCollectionBackend, ActorSource, ActorStorageId, ActorWorld,
        BaseActorSeed, DetectorSeed, SourceBacking,
    };
    use crate::entity_actor::EntityProcessState;
    use crate::model_loading::ModelArena;
    use crate::original_program::ProgramSeed;

    struct Heap {
        next: u32,
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Err("base allocation outside fixture".into())
        }
        fn allocate_detector(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
            assert_eq!(bytes, 0x128);
            self.next += 1;
            Ok(Some((
                self.next,
                BaseActorSeed {
                    word_0e: 0,
                    actor_id: 0,
                    state_1c: 0,
                    bytes_21_5f: [0; 63],
                    bytes_64_65: [0; 2],
                    byte_67: 0,
                    release_frame: 0,
                    bytes_6d_6f: [0; 3],
                },
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
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    struct NoopDiagnostic;
    impl super::MissionProgramDiagnostic for NoopDiagnostic {
        fn message_beep(
            &mut self,
            _: &mut ActorWorld,
            _: &mut ModelArena,
            _: crate::original_program::RuntimeView<'_>,
            _: &mut Option<crate::script_scheduler::ActorReferenceId>,
            _: &mut crate::script_scheduler::FrameReferenceId,
            _: i32,
        ) -> Result<(), crate::script_lookup::LookupError> {
            Ok(())
        }
    }

    let backing = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 16,
    };
    let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
    let mut heap = Heap { next: 0 };
    let detector = world.create_actor(5, &mut heap).unwrap().unwrap();
    let mut arena = ModelArena::default();
    let mut scene = crate::script_scheduler::FrameReferenceId::from_existing(1);
    let mut diagnostic = NoopDiagnostic;
    let mut lookup = MissionLookupWorld {
        world: &mut world,
        arena: &mut arena,
        scene: &mut scene,
        diagnostic: &mut diagnostic,
        locales: super::ProgramWorldLocales {
            game_default_c_locale: true,
            ls3df_default_c_locale: true,
        },
    };
    // A detector is not human: the native type-break maps to a silent
    // no-op success, while act-state writes land.
    assert_eq!(lookup.human_add_weapon(detector, 9, 7, 100), Ok(()));
    lookup.set_actor_act_state(detector, 1).unwrap();
    assert_eq!(lookup.world.state(detector).unwrap().state_1c, 1);
    // Entity state round-trips through the same live owner.
    let stale = crate::script_scheduler::ActorReferenceId::from_existing(999);
    assert!(lookup.load_entity_process_state(Some(detector)).is_err());
    assert!(lookup
        .store_entity_process_state(
            Some(detector),
            &EntityProcessState {
                process_flags: 1,
                visibility_bits: 2,
                shut_up: 3,
            },
        )
        .is_err());
    assert!(matches!(
        lookup.set_actor_act_state(stale, 0),
        Err(crate::script_lookup::LookupError::Backend(_))
    ));
    assert!(matches!(
        lookup.human_add_weapon(stale, 0, 0, 0),
        Err(crate::script_lookup::LookupError::Backend(_))
    ));
}

#[test]
fn program_set_event_via_world_sets_jump_and_act_state() {
    use crate::actor_world::{ActorStorageId, BaseActorSeed, SourceBacking};
    #[allow(unused_imports)]
    use crate::entity_actor::EntityProcessState;
    use crate::entity_actor::EntitySeed;
    use crate::human_actor::{
        HumanAnimationMachineSeed, HumanAuxiliarySeed, HumanProperties, HumanSeed,
    };
    use crate::original_program::{
        CProgram, ProgramAllocation, ProgramBackend, ProgramBuffer, ProgramSeed,
    };

    struct ProgMem {
        next: u64,
    }
    impl ProgramBackend for ProgMem {
        fn allocate(
            &mut self,
            _: &CProgram,
            _: ProgramBuffer,
            _: u32,
        ) -> Result<ProgramAllocation, String> {
            self.next += 1;
            Ok(ProgramAllocation(self.next))
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
    struct WorldHeap {
        next: u32,
    }
    impl crate::actor_world::ActorAllocationBackend for WorldHeap {
        fn allocate_actor(&mut self, _: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            Ok(Some((
                self.next,
                BaseActorSeed {
                    word_0e: 0,
                    actor_id: 0,
                    state_1c: 5,
                    bytes_21_5f: [0; 63],
                    bytes_64_65: [0; 2],
                    byte_67: 0,
                    release_frame: 0,
                    bytes_6d_6f: [0; 3],
                },
            )))
        }
        fn allocate_entity(
            &mut self,
            _: u32,
        ) -> Result<Option<(u32, BaseActorSeed, EntitySeed)>, String> {
            let base = BaseActorSeed {
                word_0e: 0,
                actor_id: 0,
                state_1c: 5,
                bytes_21_5f: [0; 63],
                bytes_64_65: [0; 2],
                byte_67: 0,
                release_frame: 0,
                bytes_6d_6f: [0; 3],
            };
            let human = HumanSeed {
                allocator_tag: 0x3c,
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
            };
            let prog_seed = ProgramSeed {
                address_high_byte: 0xff,
                padding_0d: [0; 3],
                words_1c_2c: [0; 5],
                padding_31: [0; 3],
                word_40: 0,
                bytes_64: [0; 4],
            };
            let entity = EntitySeed {
                human,
                program: prog_seed,
                allocator_tag_a: 0,
                allocator_tag_b: 0,
                untouched_word: 0,
                untouched_padding: [0; 3],
            };
            self.next += 1;
            Ok(Some((self.next, base, entity)))
        }
    }
    impl crate::actor_world::ActorCollectionBackend for WorldHeap {
        fn allocate_collection(
            &mut self,
            _: &crate::actor_world::ActorWorld,
            _: crate::actor_world::ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &crate::actor_world::ActorWorld,
            _: crate::actor_world::ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    let backing = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 16,
    };
    let mut world = crate::actor_world::ActorWorld::new(0, 0, backing, backing, backing).unwrap();
    let mut heap = WorldHeap { next: 0x81000000 };
    let base = BaseActorSeed {
        word_0e: 0,
        actor_id: 0,
        state_1c: 5,
        bytes_21_5f: [0; 63],
        bytes_64_65: [0; 2],
        byte_67: 0,
        release_frame: 0,
        bytes_6d_6f: [0; 3],
    };
    let human = HumanSeed {
        allocator_tag: 0x3c,
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
    };
    let prog_seed = ProgramSeed {
        address_high_byte: 0xff,
        padding_0d: [0; 3],
        words_1c_2c: [0; 5],
        padding_31: [0; 3],
        word_40: 0,
        bytes_64: [0; 4],
    };
    let entity_seed = EntitySeed {
        human,
        program: prog_seed,
        allocator_tag_a: 0,
        allocator_tag_b: 0,
        untouched_word: 0,
        untouched_padding: [0; 3],
    };
    let actor = world
        .construct_entity(0x81000000, base, entity_seed, &mut heap)
        .unwrap();
    // Install a program with an EVENT and take it through the world
    let mut prog = world.take_entity_program(actor).unwrap().unwrap();
    let mut prog_mem = ProgMem { next: 0 };
    prog.set_source_code(Some(b"event 60001\nlabel after"), &mut prog_mem)
        .unwrap();
    // Minimal runtime backend for init
    struct RuntimeMem;
    impl crate::original_program::RuntimeBackend for RuntimeMem {
        fn effect(
            &mut self,
            _: crate::original_program::RuntimeView<'_>,
            _: crate::original_program::RuntimeEffect,
            _: Option<&crate::program_parameters::ScalarParameter>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    let mut rt_mem = RuntimeMem;
    prog.init_fresh_runtime(&mut rt_mem).unwrap();
    world.return_entity_program(actor, Some(prog)).unwrap();
    assert_eq!(world.state(actor).unwrap().state_1c, 5);
    // First SetEvent should succeed, jump to EVENT (index 0) and set act_state to 0
    let ok = world.program_set_event(actor, 60001).unwrap();
    assert!(ok);
    let prog_ref = world.entity(actor).unwrap().program.as_ref().unwrap();
    let rt = prog_ref.runtime().unwrap();
    assert_eq!(rt.fields.program_counter(), 0);
    assert_eq!(rt.fields.gosub_stack, vec![0x8000 | 0x4000]);
    assert_eq!(world.state(actor).unwrap().state_1c, 0);
    // Already inside event -> second call should return false (busy)
    let busy = world.program_set_event(actor, 60001).unwrap();
    assert!(!busy);
    // Missing event -> true (no jump, no state change)
    let missing = world.program_set_event(actor, 99999).unwrap();
    assert!(missing);
    // Stale actor propagates
    let stale = crate::script_scheduler::ActorReferenceId::from_existing(9999);
    assert!(world.program_set_event(stale, 60001).is_err());
    // Non-entity actor returns true without effect (native type gate)
    let backing2 = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 16,
    };
    let mut world2 =
        crate::actor_world::ActorWorld::new(0, 0, backing2, backing2, backing2).unwrap();
    let mut heap2 = WorldHeap { next: 0x82000000 };
    let base2 = BaseActorSeed {
        word_0e: 0,
        actor_id: 0,
        state_1c: 7,
        bytes_21_5f: [0; 63],
        bytes_64_65: [0; 2],
        byte_67: 0,
        release_frame: 0,
        bytes_6d_6f: [0; 3],
    };
    let base_actor = world2
        .construct_base(0x83000000, base2, &mut heap2)
        .unwrap();
    assert!(world2.program_set_event(base_actor, 60001).unwrap());
    assert_eq!(world2.state(base_actor).unwrap().state_1c, 7);
}

#[test]
fn init_detector_programs_compiles_fresh_detector_sources_through_context() {
    use crate::actor_world::{
        ActorAllocationBackend, ActorCollectionBackend, ActorSource, ActorStorageId, ActorWorld,
        BaseActorSeed, DetectorSeed, SourceBacking,
    };
    use crate::model_loading::ModelArena;
    use crate::original_program::{
        ProgramAllocation, ProgramBackend, ProgramBuffer, ProgramSeed,
    };

    struct Heap {
        next: u32,
        program_next: u64,
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
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            assert_eq!(bytes, 0x70);
            self.next += 1;
            Ok(Some((self.next, base_seed())))
        }
        fn allocate_detector(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
            assert_eq!(bytes, 0x128);
            self.next += 1;
            Ok(Some((
                self.next,
                base_seed(),
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
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ProgramBackend for Heap {
        fn allocate(
            &mut self,
            _: &crate::original_program::CProgram,
            _: ProgramBuffer,
            _: u32,
        ) -> Result<ProgramAllocation, String> {
            self.program_next += 1;
            Ok(ProgramAllocation(self.program_next))
        }
        fn free(
            &mut self,
            _: &crate::original_program::CProgram,
            _: ProgramBuffer,
            _: Option<ProgramAllocation>,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    let backing = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 16,
    };
    let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
    let mut heap = Heap {
        next: 0,
        program_next: 0,
    };
    let detector = world.create_actor(5, &mut heap).unwrap().unwrap();
    // Non-detector actors are skipped by the init pass, mirroring entities.
    world.create_actor(1, &mut heap).unwrap().unwrap();
    // The live Init owns the program owner link; the fixture assigns it
    // explicitly like the entity pass test, then the pass compiles around it.
    let mut program = world.take_detector_program(detector).unwrap().unwrap();
    program
        .set_source_code(Some(b"dim_act 1\ndetector_setsignal -1, 1"), &mut heap)
        .unwrap();
    program.set_owner(Some(detector)).unwrap();
    world.return_detector_program(detector, Some(program)).unwrap();
    let mut arena = ModelArena::default();
    let mut scene = crate::script_scheduler::FrameReferenceId::from_existing(1);
    let mut effects = Effects::default();
    let mut diagnostic = Diagnostic::default();
    let mut context = MissionProgramWorldContext::new(
        &mut arena,
        &mut scene,
        &mut effects,
        &mut diagnostic,
        locales(),
    );
    // The init pass compiles the sourceless-runtime program and leaves
    // initialized programs untouched; the program stays with its owner.
    assert_eq!(context.init_detector_programs(&mut world), Ok(1));
    assert!(world.detector(detector).unwrap().program().is_some_and(
        |program| program.runtime().is_some()
    ));
    assert_eq!(
        world
            .detector(detector)
            .unwrap()
            .program()
            .unwrap()
            .owner(),
        Some(detector)
    );
    assert_eq!(context.init_detector_programs(&mut world), Ok(0));
    // The context borrows the effects backend; release it before arming the
    // failure injection below.
    drop(context);
    // A failing effects backend reports the runtime error yet restores the
    // taken program to its owner (return-on-every-path, mirroring entities):
    // a second fresh detector still needs its first Init when the backend
    // refuses the working-source allocation.
    let detector2 = world.create_actor(5, &mut heap).unwrap().unwrap();
    let mut program2 = world.take_detector_program(detector2).unwrap().unwrap();
    program2
        .set_source_code(Some(b"dim_act 1"), &mut heap)
        .unwrap();
    program2.set_owner(Some(detector2)).unwrap();
    world
        .return_detector_program(detector2, Some(program2))
        .unwrap();
    effects.fail = Some(RuntimeEffect::AllocateWorkingSource(
        // Working-source size is the C-string length plus its NUL.
        b"dim_act 1".len() + 1,
    ));
    let mut context = MissionProgramWorldContext::new(
        &mut arena,
        &mut scene,
        &mut effects,
        &mut diagnostic,
        locales(),
    );
    assert!(matches!(
        context.init_detector_programs(&mut world),
        Err(crate::mission_content_loading::program_world::DetectorTickError::Runtime(_))
    ));
    assert!(world.detector(detector2).unwrap().program().is_some());
}

#[test]
fn tick_detector_programs_steps_owned_live_state_through_context() {
    use crate::actor_world::{
        ActorAllocationBackend, ActorCollectionBackend, ActorSource, ActorStorageId, ActorWorld,
        BaseActorSeed, DetectorSeed, SourceBacking,
    };
    use crate::model_loading::ModelArena;
    use crate::original_program::{
        ProgramAllocation, ProgramBackend, ProgramBuffer, ProgramSeed,
    };

    struct Heap {
        next: u32,
        program_next: u64,
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
    fn detector_seed() -> DetectorSeed {
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
        }
    }
    impl ActorAllocationBackend for Heap {
        fn allocate_actor(&mut self, bytes: u32) -> Result<Option<(u32, BaseActorSeed)>, String> {
            assert_eq!(bytes, 0x70);
            self.next += 1;
            Ok(Some((self.next, base_seed())))
        }
        fn allocate_detector(
            &mut self,
            bytes: u32,
        ) -> Result<Option<(u32, BaseActorSeed, DetectorSeed)>, String> {
            assert_eq!(bytes, 0x128);
            self.next += 1;
            Ok(Some((self.next, base_seed(), detector_seed())))
        }
    }
    impl ActorCollectionBackend for Heap {
        fn allocate_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: u32,
        ) -> Result<ActorStorageId, String> {
            Ok(ActorStorageId(2))
        }
        fn free_collection(
            &mut self,
            _: &ActorWorld,
            _: ActorSource,
            _: Option<ActorStorageId>,
        ) -> Result<(), String> {
            Ok(())
        }
    }
    impl ProgramBackend for Heap {
        fn allocate(
            &mut self,
            _: &crate::original_program::CProgram,
            _: ProgramBuffer,
            _: u32,
        ) -> Result<ProgramAllocation, String> {
            self.program_next += 1;
            Ok(ProgramAllocation(self.program_next))
        }
        fn free(
            &mut self,
            _: &crate::original_program::CProgram,
            _: ProgramBuffer,
            _: Option<ProgramAllocation>,
        ) -> Result<(), String> {
            Ok(())
        }
    }

    let backing = SourceBacking {
        storage: Some(ActorStorageId(1)),
        capacity: 16,
    };
    let mut world = ActorWorld::new(0, 0, backing, backing, backing).unwrap();
    let mut heap = Heap {
        next: 0,
        program_next: 0,
    };
    let detector = world.create_actor(5, &mut heap).unwrap().unwrap();
    // A non-detector actor never ticks, mirroring the entity transport skip.
    world.create_actor(1, &mut heap).unwrap().unwrap();
    // PROGRAM_STORAGE parks on its command: the transport returns Ok(1) while
    // the program stays with its owner and accumulates command_time.
    let mut program = world.take_detector_program(detector).unwrap().unwrap();
    program
        .set_source_code(Some(b"program_storage"), &mut heap)
        .unwrap();
    program.set_owner(Some(detector)).unwrap();
    world.return_detector_program(detector, Some(program)).unwrap();
    // A WAITFORUSE arm reaches the unavailable using-object owner: the tick
    // reports the runtime error yet restores the program to its owner
    // (return-on-every-path, mirroring entities).
    let waiter = world.create_actor(5, &mut heap).unwrap().unwrap();
    let mut wait_program = world.take_detector_program(waiter).unwrap().unwrap();
    wait_program
        .set_source_code(Some(b"detector_waitforuse 4, 0"), &mut heap)
        .unwrap();
    wait_program.set_owner(Some(waiter)).unwrap();
    world
        .return_detector_program(waiter, Some(wait_program))
        .unwrap();
    let mut arena = ModelArena::default();
    let mut scene = crate::script_scheduler::FrameReferenceId::from_existing(1);
    let mut effects = Effects::default();
    let mut diagnostic = Diagnostic::default();
    let mut context = MissionProgramWorldContext::new(
        &mut arena,
        &mut scene,
        &mut effects,
        &mut diagnostic,
        locales(),
    );
    assert_eq!(context.init_detector_programs(&mut world), Ok(2));
    // The storage detector steps alone: the waiter fails after it, so the
    // shared transport reports the runtime error with one step taken.
    let mut budget = crate::script_scheduler::DispatchBudget::new(8).unwrap();
    assert!(matches!(
        context.tick_detector_programs(&mut world, 3, &mut budget),
        Err(crate::mission_content_loading::program_world::DetectorTickError::Runtime(_))
    ));
    let stored = world
        .detector(detector)
        .unwrap()
        .program()
        .unwrap()
        .runtime()
        .unwrap();
    assert_eq!(stored.fields.program_counter(), 0);
    assert_eq!(stored.fields.command_time, 3);
    assert!(world.detector(detector).unwrap().program().is_some());
    assert!(world.detector(waiter).unwrap().program().is_some());
    drop(context);
    // Without the waiter the storage detector ticks cleanly and accumulates
    // across takes: each pass takes the program out and returns it.
    let mut effects = Effects::default();
    let mut diagnostic = Diagnostic::default();
    let mut context = MissionProgramWorldContext::new(
        &mut arena,
        &mut scene,
        &mut effects,
        &mut diagnostic,
        locales(),
    );
    world
        .return_detector_program(waiter, None)
        .unwrap();
    assert_eq!(
        context.tick_detector_programs(&mut world, 4, &mut budget),
        Ok(1)
    );
    let stored = world
        .detector(detector)
        .unwrap()
        .program()
        .unwrap()
        .runtime()
        .unwrap();
    assert_eq!(stored.fields.program_counter(), 0);
    assert_eq!(stored.fields.command_time, 7);
    assert!(world.detector(detector).unwrap().program().is_some());
}
