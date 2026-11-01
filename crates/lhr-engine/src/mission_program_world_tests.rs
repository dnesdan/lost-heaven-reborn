//! Actual scene/AE20/AE50 owners, not a second program or mock lookup world.
use super::*;
use crate::driver_scene::{
    DriverSceneContext, DriverSceneOwner, DriverSceneScreen, NativeSceneSoundDriver,
};
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
