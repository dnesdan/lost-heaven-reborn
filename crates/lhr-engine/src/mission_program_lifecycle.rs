//! Ordered C_mission global program lifecycle loops, not C_game initialization.
//! Game08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27,
//! GlobalProgramGameInit RVA140520, GlobalProgramGameDone RVA140550.
//! Actual calls are direct C_program Init/Done; their internal virtual decoding
//! remains a separate dependency. No mission/actor GameInit completion flag.
#![forbid(unsafe_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlobalProgramOperation {
    Initialize,
    Finish,
}
#[derive(Debug, PartialEq, Eq)]
pub enum GlobalProgramError<E> {
    EntryCountUnavailable(E),
    CountOutsideDomain(usize),
    Program {
        operation: GlobalProgramOperation,
        index: usize,
        error: E,
    },
}
/// Borrows the actual program collection/resource owners. Entry count is read
/// once. Each operation must resolve the current backing/member at its index,
/// not an eager list cached before earlier Init/Done effects.
pub trait GlobalProgramOwner {
    type Error;
    fn entry_program_count(&self) -> Result<usize, Self::Error>;
    fn initialize_program_at(&mut self, index: usize) -> Result<(), Self::Error>;
    fn finish_program_at(&mut self, index: usize) -> Result<(), Self::Error>;
}
fn visit<O: GlobalProgramOwner>(
    owner: &mut O,
    operation: GlobalProgramOperation,
) -> Result<(), GlobalProgramError<O::Error>> {
    let count = owner
        .entry_program_count()
        .map_err(GlobalProgramError::EntryCountUnavailable)?;
    // Host admission for the original aligned ordered4byte vector span<2GiB.
    if count > 0x1fff_ffff {
        return Err(GlobalProgramError::CountOutsideDomain(count));
    }
    for index in (0..count).rev() {
        let result = match operation {
            GlobalProgramOperation::Initialize => owner.initialize_program_at(index),
            GlobalProgramOperation::Finish => owner.finish_program_at(index),
        };
        result.map_err(|error| GlobalProgramError::Program {
            operation,
            index,
            error,
        })?;
    }
    Ok(())
}
pub fn initialize_global_programs<O: GlobalProgramOwner>(
    owner: &mut O,
) -> Result<(), GlobalProgramError<O::Error>> {
    visit(owner, GlobalProgramOperation::Initialize)
}
pub fn finish_global_programs<O: GlobalProgramOwner>(
    owner: &mut O,
) -> Result<(), GlobalProgramError<O::Error>> {
    visit(owner, GlobalProgramOperation::Finish)
}

use super::MissionContentOwner;
use crate::original_program::{HostRuntimeDiscard, RuntimeBackend, RuntimeError};

#[derive(Debug, PartialEq, Eq)]
pub enum MissionGlobalProgramError {
    SourceUnavailable(String),
    PendingSource,
    MissingCurrentProgram(usize),
    Runtime(RuntimeError),
    ReportAllocation,
}
#[derive(Debug, PartialEq, Eq)]
pub struct GlobalProgramDiscard {
    pub index: usize,
    pub resources: HostRuntimeDiscard,
}
struct MissionPrograms<'a, B> {
    owner: &'a mut MissionContentOwner,
    backend: &'a mut B,
}
impl<B: RuntimeBackend> GlobalProgramOwner for MissionPrograms<'_, B> {
    type Error = MissionGlobalProgramError;
    fn entry_program_count(&self) -> Result<usize, Self::Error> {
        self.owner
            .available()
            .map_err(MissionGlobalProgramError::SourceUnavailable)?;
        if self.owner.pending_program.is_some() {
            return Err(MissionGlobalProgramError::PendingSource);
        }
        Ok(self.owner.programs.len())
    }
    fn initialize_program_at(&mut self, index: usize) -> Result<(), Self::Error> {
        self.owner
            .programs
            .get_mut(index)
            .ok_or(MissionGlobalProgramError::MissingCurrentProgram(index))?
            .init_fresh_runtime(self.backend)
            .map(|_| ())
            .map_err(MissionGlobalProgramError::Runtime)
    }
    fn finish_program_at(&mut self, index: usize) -> Result<(), Self::Error> {
        self.owner
            .programs
            .get_mut(index)
            .ok_or(MissionGlobalProgramError::MissingCurrentProgram(index))?
            .done_fresh_runtime(self.backend)
            .map_err(MissionGlobalProgramError::Runtime)
    }
}
/// Required actual C_game-global lookup/execution boundary for mission
/// GlobalProgramRun. No default treats an unavailable game owner as empty.
/// The same live mission is borrowed so successful game execution may mutate
/// its programs before the mission fallback re-reads them.
pub trait GameGlobalProgramRunner<B: RuntimeBackend> {
    fn run_game_global(
        &mut self,
        mission: &mut MissionContentOwner,
        name: &[u8],
        budget: &mut crate::script_scheduler::DispatchBudget,
        backend: &mut B,
    ) -> Result<bool, String>;
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NamedGlobalProgramOutcome {
    GameHandled,
    MissionHandled { index: usize },
    NoMatch,
}
#[derive(Debug, PartialEq, Eq)]
pub enum NamedGlobalProgramError {
    SourceUnavailable(String),
    PendingSource,
    NameOutsideDomain,
    GameOwner(String),
    MissingCurrentProgram(usize),
    MissingProgramName(usize),
    Runtime { index: usize, error: RuntimeError },
}
impl MissionContentOwner {
    /// Run only the original reverse global-program Init loop at its explicitly
    /// reached caller position. Success does not establish C_game/actor GameInit.
    /// Programs stay in their actual source owner; no diagnostic source parser
    /// or derived Entity/Detector decoder fallback is introduced.
    pub fn global_program_game_init<B: RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), GlobalProgramError<MissionGlobalProgramError>> {
        initialize_global_programs(&mut MissionPrograms {
            owner: self,
            backend,
        })
    }
    /// Reverse current global programs and call their actual owned Done. Every
    /// program's fresh/partial/uncertain-disposal admission remains authoritative.
    pub fn global_program_game_done<B: RuntimeBackend>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), GlobalProgramError<MissionGlobalProgramError>> {
        finish_global_programs(&mut MissionPrograms {
            owner: self,
            backend,
        })
    }
    /// Original mission GlobalProgramRun order: required game-global owner
    /// first, then the current mission collection in reverse name priority.
    /// The outcome is host inspection information, not a native HRESULT.
    pub fn global_program_run<B: RuntimeBackend, G: GameGlobalProgramRunner<B>>(
        &mut self,
        name: &[u8],
        game: &mut G,
        budget: &mut crate::script_scheduler::DispatchBudget,
        backend: &mut B,
    ) -> Result<NamedGlobalProgramOutcome, NamedGlobalProgramError> {
        self.available()
            .map_err(NamedGlobalProgramError::SourceUnavailable)?;
        if self.pending_program.is_some() {
            return Err(NamedGlobalProgramError::PendingSource);
        }
        if !name.is_ascii() || name.contains(&0) {
            return Err(NamedGlobalProgramError::NameOutsideDomain);
        }
        if game
            .run_game_global(self, name, budget, backend)
            .map_err(NamedGlobalProgramError::GameOwner)?
        {
            return Ok(NamedGlobalProgramOutcome::GameHandled);
        }
        // The game call may replace the mission's actual collection. Do not
        // cache its members or names across that call.
        self.available()
            .map_err(NamedGlobalProgramError::SourceUnavailable)?;
        if self.pending_program.is_some() {
            return Err(NamedGlobalProgramError::PendingSource);
        }
        let count = self.programs.len();
        for index in (0..count).rev() {
            let program = self
                .programs
                .get(index)
                .ok_or(NamedGlobalProgramError::MissingCurrentProgram(index))?;
            let candidate = program
                .name()
                .and_then(|n| n.c_bytes())
                .ok_or(NamedGlobalProgramError::MissingProgramName(index))?;
            if !candidate.is_ascii() {
                return Err(NamedGlobalProgramError::NameOutsideDomain);
            }
            if candidate.eq_ignore_ascii_case(name) {
                self.programs
                    .get_mut(index)
                    .ok_or(NamedGlobalProgramError::MissingCurrentProgram(index))?
                    .call_owned_subroutine(budget, backend)
                    .map_err(|error| NamedGlobalProgramError::Runtime { index, error })?;
                return Ok(NamedGlobalProgramOutcome::MissionHandled { index });
            }
        }
        Ok(NamedGlobalProgramOutcome::NoMatch)
    }
    /// Explicit additional host cleanup, not GlobalProgramGameDone. Invoke
    /// before source terminal disposal; callbacks and native cleanup are not
    /// replayed, including for interrupted CProgram runtimes.
    pub fn discard_global_program_runtimes_host(
        &mut self,
    ) -> Result<Vec<GlobalProgramDiscard>, MissionGlobalProgramError> {
        if self.terminal_entered {
            return Err(MissionGlobalProgramError::SourceUnavailable(
                "source terminal disposal already entered".into(),
            ));
        }
        let mut reports = Vec::new();
        reports
            .try_reserve_exact(self.programs.len())
            .map_err(|_| MissionGlobalProgramError::ReportAllocation)?;
        for index in (0..self.programs.len()).rev() {
            let program = &mut self.programs[index];
            if program.runtime().is_some() {
                let resources = program
                    .discard_runtime_host()
                    .map_err(MissionGlobalProgramError::Runtime)?;
                reports.push(GlobalProgramDiscard { index, resources });
            }
        }
        Ok(reports)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Owner {
        members: Vec<u32>,
        entry_count: usize,
        calls: Vec<(GlobalProgramOperation, usize, u32)>,
        rebase: bool,
        stop: Option<usize>,
    }
    impl GlobalProgramOwner for Owner {
        type Error = &'static str;
        fn entry_program_count(&self) -> Result<usize, Self::Error> {
            Ok(self.entry_count)
        }
        fn initialize_program_at(&mut self, index: usize) -> Result<(), Self::Error> {
            self.call(GlobalProgramOperation::Initialize, index)
        }
        fn finish_program_at(&mut self, index: usize) -> Result<(), Self::Error> {
            self.call(GlobalProgramOperation::Finish, index)
        }
    }
    impl Owner {
        fn call(&mut self, op: GlobalProgramOperation, index: usize) -> Result<(), &'static str> {
            let id = *self.members.get(index).ok_or("backing unavailable")?;
            self.calls.push((op, index, id));
            if self.stop == Some(index) {
                return Err("required dependency unavailable");
            }
            if self.rebase && index == 2 {
                self.members = vec![40, 50, 60, 70];
                self.entry_count = 4;
            }
            Ok(())
        }
    }
    #[test]
    fn reverse_order_rereads_current_backing_but_keeps_entry_count() {
        for operation in [
            GlobalProgramOperation::Initialize,
            GlobalProgramOperation::Finish,
        ] {
            let mut o = Owner {
                members: vec![1, 2, 3],
                entry_count: 3,
                calls: vec![],
                rebase: true,
                stop: None,
            };
            visit(&mut o, operation).unwrap();
            assert_eq!(
                o.calls,
                vec![(operation, 2, 3), (operation, 1, 50), (operation, 0, 40)]
            );
        }
    }
    #[test]
    fn failure_keeps_reached_reverse_prefix_and_never_visits_later_indices() {
        let mut o = Owner {
            members: vec![1, 2, 3],
            entry_count: 3,
            calls: vec![],
            rebase: false,
            stop: Some(1),
        };
        assert_eq!(
            initialize_global_programs(&mut o),
            Err(GlobalProgramError::Program {
                operation: GlobalProgramOperation::Initialize,
                index: 1,
                error: "required dependency unavailable"
            })
        );
        assert_eq!(o.calls.len(), 2);
        assert_eq!(o.calls[0].1, 2);
        assert_eq!(o.calls[1].1, 1);
    }
    #[test]
    fn empty_and_unavailable_later_backing_are_distinct() {
        let mut o = Owner {
            members: vec![],
            entry_count: 0,
            calls: vec![],
            rebase: false,
            stop: None,
        };
        finish_global_programs(&mut o).unwrap();
        assert!(o.calls.is_empty());
        o.entry_count = 2;
        assert_eq!(
            finish_global_programs(&mut o),
            Err(GlobalProgramError::Program {
                operation: GlobalProgramOperation::Finish,
                index: 1,
                error: "backing unavailable"
            })
        );
    }
    #[derive(Default)]
    struct Memory {
        working_sizes: Vec<usize>,
        scalar_allocations: usize,
        scalar_releases: usize,
        fail_name_allocation: bool,
        name_allocations: usize,
        name_releases: usize,
    }
    impl RuntimeBackend for Memory {
        fn effect(
            &mut self,
            _: crate::original_program::RuntimeView<'_>,
            effect: crate::original_program::RuntimeEffect,
            _: Option<&crate::program_parameters::ScalarParameter>,
        ) -> Result<(), String> {
            match effect {
                crate::original_program::RuntimeEffect::AllocateWorkingSource(size) => {
                    self.working_sizes.push(size)
                }
                crate::original_program::RuntimeEffect::AllocateScalar => {
                    self.scalar_allocations += 1
                }
                crate::original_program::RuntimeEffect::ReleaseScalar(_) => {
                    self.scalar_releases += 1
                }
                crate::original_program::RuntimeEffect::AllocateName => {
                    if self.fail_name_allocation {
                        return Err("injected named allocation failure".into());
                    }
                    self.name_allocations += 1;
                }
                crate::original_program::RuntimeEffect::ReleaseName(_) => {
                    self.name_releases += 1;
                }
                _ => {}
            }
            Ok(())
        }
    }
    fn content(sources: &[&[u8]]) -> MissionContentOwner {
        let mut owner = MissionContentOwner::default();
        for source in sources {
            let mut program =
                crate::original_program::CProgram::construct(super::super::program_seed());
            program
                .set_source_code(Some(source), &mut owner.memory)
                .unwrap();
            owner.programs.push(program);
        }
        owner
    }
    #[test]
    fn actual_global_source_owners_initialize_reverse_and_keep_zero_payloads() {
        let first = b"dim_act 2".as_slice();
        let last = b"commandblock 1\nuse_lightcache ignored".as_slice();
        let mut owner = content(&[first, last]);
        let source = owner
            .programs
            .iter()
            .map(|p| p.source().unwrap().bytes.clone())
            .collect::<Vec<_>>();
        let mut memory = Memory::default();
        owner.global_program_game_init(&mut memory).unwrap();
        assert_eq!(memory.working_sizes, vec![last.len() + 1, first.len() + 1]);
        assert_eq!(
            owner.programs[0]
                .runtime()
                .unwrap()
                .instructions
                .active()
                .len(),
            1
        );
        assert_eq!(
            owner.programs[1]
                .runtime()
                .unwrap()
                .instructions
                .active()
                .len(),
            2
        );
        assert!(
            owner.programs[1].runtime().unwrap().instructions.active()[1]
                .parameter
                .is_none()
        );
        for (program, source) in owner.programs.iter().zip(source) {
            assert_eq!(program.source().unwrap().bytes, source);
            assert_eq!(program.runtime().unwrap().fields.program_counter(), 0);
        }
        owner.global_program_game_done(&mut memory).unwrap();
        assert_eq!(memory.scalar_allocations, memory.scalar_releases);
        let reports = owner.discard_global_program_runtimes_host().unwrap();
        assert_eq!(
            reports.iter().map(|r| r.index).collect::<Vec<_>>(),
            vec![1, 0]
        );
        owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
        assert_eq!(owner.live_allocation_count(), 0);
    }
    #[test]
    fn required_parameter_failure_keeps_earlier_reverse_init_and_blocks_replay_without_prescan() {
        let mut owner = content(&[b"findactor 0,\"person\"", b"dim_frm 3"]);
        let mut memory = Memory::default();
        memory.fail_name_allocation = true;
        assert!(matches!(
            owner.global_program_game_init(&mut memory),
            Err(GlobalProgramError::Program {
                index: 0,
                error: MissionGlobalProgramError::Runtime(_),
                ..
            })
        ));
        assert_eq!(
            owner.programs[1].runtime().unwrap().stage,
            crate::original_program::RuntimeStage::Ready
        );
        assert_eq!(
            owner.programs[0].runtime().unwrap().stage,
            crate::original_program::RuntimeStage::CompileInterrupted
        );
        let effects = memory.working_sizes.len();
        assert!(matches!(
            owner.global_program_game_init(&mut memory),
            Err(GlobalProgramError::Program { index: 1, .. })
        ));
        assert_eq!(memory.working_sizes.len(), effects);
        owner.discard_global_program_runtimes_host().unwrap();
        owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
        assert_eq!(owner.live_allocation_count(), 0);
    }
    #[test]
    fn source_terminal_admission_requires_explicit_runtime_discard_before_any_destructor() {
        let mut owner = content(&[b"dim_act 2"]);
        let mut memory = Memory::default();
        owner.global_program_game_init(&mut memory).unwrap();
        assert!(owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .is_err());
        assert!(!owner.terminal_entered);
        assert_eq!(
            owner.programs[0].phase(),
            crate::original_program::ProgramPhase::Source
        );
        owner.discard_global_program_runtimes_host().unwrap();
        owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
        assert!(owner.terminal_complete);
        assert_eq!(owner.live_allocation_count(), 0);
    }
    fn named_content(entries: &[(&[u8], &[u8])]) -> MissionContentOwner {
        fn chunk(kind: u16, payload: &[u8]) -> Vec<u8> {
            let mut out = kind.to_le_bytes().to_vec();
            out.extend_from_slice(&((payload.len() + 6) as u32).to_le_bytes());
            out.extend_from_slice(payload);
            out
        }
        let mut records = Vec::new();
        for (name, source) in entries {
            let mut payload = vec![1];
            payload.extend_from_slice(&(name.len() as u32).to_le_bytes());
            payload.extend_from_slice(name);
            payload.extend_from_slice(&(source.len() as u32).to_le_bytes());
            payload.extend_from_slice(source);
            records.extend(chunk(0xae51, &payload));
        }
        let bytes = chunk(0x4c53, &chunk(0xae50, &records));
        let root = lhr_assets::scene_actor_stream::scene2_root(&bytes).unwrap();
        let programs = root.children().next().unwrap().unwrap();
        let mut owner = MissionContentOwner::default();
        owner
            .load_programs(
                programs,
                &crate::model_loading::ModelArena::default(),
                &crate::program_patch::ProgramPatchTable::default(),
            )
            .unwrap();
        owner
    }
    struct MissingGame;
    impl GameGlobalProgramRunner<Memory> for MissingGame {
        fn run_game_global(
            &mut self,
            _: &mut MissionContentOwner,
            _: &[u8],
            _: &mut crate::script_scheduler::DispatchBudget,
            _: &mut Memory,
        ) -> Result<bool, String> {
            Err("actual game-global owner unavailable".into())
        }
    }
    struct TestGame {
        programs: MissionContentOwner,
        swap_mission: bool,
    }
    impl GameGlobalProgramRunner<Memory> for TestGame {
        fn run_game_global(
            &mut self,
            mission: &mut MissionContentOwner,
            name: &[u8],
            budget: &mut crate::script_scheduler::DispatchBudget,
            backend: &mut Memory,
        ) -> Result<bool, String> {
            // Explicit controlled game collection fixture, not a production
            // claim that MissionContentOwner is the native C_game owner.
            for index in (0..self.programs.programs.len()).rev() {
                let p = &self.programs.programs[index];
                if p.name()
                    .unwrap()
                    .c_bytes()
                    .unwrap()
                    .eq_ignore_ascii_case(name)
                {
                    self.programs.programs[index]
                        .call_owned_subroutine(budget, backend)
                        .map_err(|e| format!("{e:?}"))?;
                    return Ok(true);
                }
            }
            if self.swap_mission {
                mission.programs.swap(0, 1);
            }
            Ok(false)
        }
    }
    #[test]
    fn named_run_requires_game_owner_and_does_not_fallback_on_unavailable() {
        let mut owner = named_content(&[(b"target", b"dim_act 2")]);
        let mut memory = Memory::default();
        owner.global_program_game_init(&mut memory).unwrap();
        assert!(matches!(
            owner.global_program_run(
                b"target",
                &mut MissingGame,
                &mut crate::script_scheduler::DispatchBudget::new(10).unwrap(),
                &mut memory
            ),
            Err(NamedGlobalProgramError::GameOwner(_))
        ));
        assert_eq!(owner.programs[0].runtime().unwrap().fields.actor_count, 0);
        owner.discard_global_program_runtimes_host().unwrap();
        owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
    }
    #[test]
    fn named_run_executes_actual_game_program_first_and_suppresses_mission_match() {
        let mut owner = named_content(&[(b"target", b"dim_act 2")]);
        let mut memory = Memory::default();
        owner.global_program_game_init(&mut memory).unwrap();
        let mut game = TestGame {
            programs: named_content(&[(b"TARGET", b"dim_frm 3")]),
            swap_mission: false,
        };
        game.programs.global_program_game_init(&mut memory).unwrap();
        assert_eq!(
            owner.global_program_run(
                b"target",
                &mut game,
                &mut crate::script_scheduler::DispatchBudget::new(10).unwrap(),
                &mut memory
            ),
            Ok(NamedGlobalProgramOutcome::GameHandled)
        );
        assert_eq!(
            game.programs.programs[0]
                .runtime()
                .unwrap()
                .fields
                .frame_count,
            3
        );
        assert_eq!(owner.programs[0].runtime().unwrap().fields.actor_count, 0);
        game.programs
            .discard_global_program_runtimes_host()
            .unwrap();
        game.programs
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
        owner.discard_global_program_runtimes_host().unwrap();
        owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
    }
    #[test]
    fn mission_named_fallback_rereads_live_collection_after_game_miss() {
        let mut owner = named_content(&[(b"target", b"dim_act 2"), (b"TARGET", b"dim_frm 3")]);
        let mut memory = Memory::default();
        owner.global_program_game_init(&mut memory).unwrap();
        let mut game = TestGame {
            programs: MissionContentOwner::default(),
            swap_mission: true,
        };
        assert_eq!(
            owner.global_program_run(
                b"TaRgEt",
                &mut game,
                &mut crate::script_scheduler::DispatchBudget::new(10).unwrap(),
                &mut memory
            ),
            Ok(NamedGlobalProgramOutcome::MissionHandled { index: 1 })
        );
        assert_eq!(owner.programs[1].runtime().unwrap().fields.actor_count, 2);
        assert_eq!(owner.programs[0].runtime().unwrap().fields.frame_count, 0);
        owner.discard_global_program_runtimes_host().unwrap();
        owner
            .discard_terminal(&mut crate::model_loading::ModelArena::default())
            .unwrap();
    }

    #[test]
    fn actual_global_sources_compile_names_and_release_their_owned_payloads() {
        let mut owner = content(&[
            b"findframe 0,\"frame\"",
            b"findactor 1,\"person\"\nrecaddactor 2",
        ]);
        let originals = owner.programs.iter()
            .map(|p| p.source().unwrap().bytes.clone()).collect::<Vec<_>>();
        let mut memory = Memory::default();
        owner.global_program_game_init(&mut memory).unwrap();
        assert_eq!(memory.name_allocations, 3);
        assert_eq!(memory.scalar_allocations, 0);
        for (program, original) in owner.programs.iter().zip(originals) {
            assert_eq!(program.source().unwrap().bytes, original);
            assert_eq!(program.runtime().unwrap().stage, crate::original_program::RuntimeStage::Ready);
            assert_eq!(program.runtime().unwrap().fields.program_counter(), 0);
            assert!(program.runtime().unwrap().instructions.active().iter().all(|instruction| {
                instruction.parameter.as_ref().unwrap().name_parameter().is_some()
            }));
        }
        owner.global_program_game_done(&mut memory).unwrap();
        assert_eq!(memory.name_releases, 3);
        let reports = owner.discard_global_program_runtimes_host().unwrap();
        assert!(reports.iter().all(|report| report.resources.name_allocations == 0));
        owner.discard_terminal(&mut crate::model_loading::ModelArena::default()).unwrap();
        assert_eq!(owner.live_allocation_count(), 0);
    }
}
