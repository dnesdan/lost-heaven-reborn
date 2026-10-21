//! Descriptive named-program caller contracts for Game RVAs 0x140580,
//! 0x1afd50 and 0x6cbc0. Original function names are not established.
//! Target SHA256: 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27.
//! ASCII names and readable retained vectors are an explicit adapter domain.
//! This module does not implement the script scheduler or command effects.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OwnerId(pub u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramOwner {
    Mission(OwnerId),
    Context(OwnerId),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramView {
    pub allocation: u32,
    pub count: usize,
}
#[derive(Debug, PartialEq, Eq)]
pub enum LookupError<E> {
    Backend(E),
    MissingBacking,
    NonAsciiName,
    EmbeddedNul,
}
/// Accessors must reject unknown identities and unreadable retained storage.
/// Names are bounded byte slices without their native trailing NUL. No callback
/// has a default implementation; execution must perform the actual bound action.
pub trait NamedProgramBackend {
    type Error;
    fn mission(&self, context: OwnerId) -> Result<OwnerId, Self::Error>;
    fn programs(&self, owner: ProgramOwner) -> Result<Option<ProgramView>, Self::Error>;
    fn program_at(&self, allocation: u32, index: usize) -> Result<ProgramId, Self::Error>;
    fn program_name(&self, program: ProgramId) -> Result<&[u8], Self::Error>;
    /// Execute immediately. A real adapter can connect reset_and_start and its
    /// required scheduler backend; recording observers establish only ordering.
    fn execute(&mut self, program: ProgramId) -> Result<(), Self::Error>;
}
fn validate_name<E>(name: &[u8]) -> Result<(), LookupError<E>> {
    if !name.is_ascii() {
        return Err(LookupError::NonAsciiName);
    }
    if name.contains(&0) {
        return Err(LookupError::EmbeddedNul);
    }
    Ok(())
}
fn search<B: NamedProgramBackend>(
    backend: &mut B,
    owner: ProgramOwner,
    name: &[u8],
) -> Result<Option<ProgramId>, LookupError<B::Error>> {
    let count = backend
        .programs(owner)
        .map_err(LookupError::Backend)?
        .map_or(0, |view| view.count);
    for index in (0..count).rev() {
        let allocation = backend
            .programs(owner)
            .map_err(LookupError::Backend)?
            .ok_or(LookupError::MissingBacking)?
            .allocation;
        let program = backend
            .program_at(allocation, index)
            .map_err(LookupError::Backend)?;
        let candidate = backend
            .program_name(program)
            .map_err(LookupError::Backend)?;
        validate_name(candidate)?;
        if candidate.eq_ignore_ascii_case(name) {
            // Native code reloads both the vector begin and slot after compare.
            let allocation = backend
                .programs(owner)
                .map_err(LookupError::Backend)?
                .ok_or(LookupError::MissingBacking)?
                .allocation;
            let selected = backend
                .program_at(allocation, index)
                .map_err(LookupError::Backend)?;
            backend.execute(selected).map_err(LookupError::Backend)?;
            return Ok(Some(selected));
        }
    }
    Ok(None)
}
/// RVA 0x1afd50 caller behavior. Some corresponds to native AL=1, None to AL=0.
/// Actual CRT locale behavior outside ASCII is not supplied by this adapter.
pub fn run_mission_named<B: NamedProgramBackend>(
    backend: &mut B,
    mission: OwnerId,
    name: &[u8],
) -> Result<Option<ProgramId>, LookupError<B::Error>> {
    validate_name(name)?;
    search(backend, ProgramOwner::Mission(mission), name)
}
/// RVA 0x140580: mission first, then context fallback. The selected identity is
/// a Rust observation convenience; the original context function has no defined
/// result value. Neither path delays execution until a later frame.
pub fn run_context_named<B: NamedProgramBackend>(
    backend: &mut B,
    context: OwnerId,
    name: &[u8],
) -> Result<Option<ProgramId>, LookupError<B::Error>> {
    validate_name(name)?;
    let mission = backend.mission(context).map_err(LookupError::Backend)?;
    if let Some(program) = search(backend, ProgramOwner::Mission(mission), name)? {
        return Ok(Some(program));
    }
    search(backend, ProgramOwner::Context(context), name)
}

/// Only the fields touched by RVA 0x6cbc0; other object fields belong to the
/// adapter and are not reconstructed by this type. Stack tokens identify backing
/// storage; reset does not read, copy or free that storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResetState {
    pub program_counter: u32,
    pub short_stack_begin: u32,
    pub short_stack_end: u32,
    pub field_44: u32,
    pub continuation: u8,
}
pub trait ProgramScheduler {
    type Error;
    /// Required synchronous RVA 0x6ccb0 binding. This is not a deferred queue.
    fn schedule(&mut self, state: &mut ResetState, delta: i32) -> Result<(), Self::Error>;
}
/// Reset before the synchronous scheduler call with delta one. Callback writes
/// survive, and an error can follow observable changes; nothing is rolled back.
pub fn reset_and_start<B: ProgramScheduler>(
    state: &mut ResetState,
    backend: &mut B,
) -> Result<(), B::Error> {
    state.program_counter = 0;
    state.short_stack_end = state.short_stack_begin;
    state.field_44 = 0;
    state.continuation = 1;
    backend.schedule(state, 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Backend {
        mission: Option<Vec<ProgramId>>,
        context: Option<Vec<ProgramId>>,
        names: [&'static [u8]; 4],
        runs: Vec<ProgramId>,
        nested: bool,
        executing: bool,
        fail: bool,
    }
    impl Default for Backend {
        fn default() -> Self {
            Self {
                mission: None,
                context: None,
                names: [b"Alpha", b"Beta", b"Alpha", b"Alpha"],
                runs: Vec::new(),
                nested: false,
                executing: false,
                fail: false,
            }
        }
    }
    impl NamedProgramBackend for Backend {
        type Error = &'static str;
        fn mission(&self, context: OwnerId) -> Result<OwnerId, Self::Error> {
            if context != OwnerId(10) {
                return Err("unknown context");
            }
            Ok(OwnerId(20))
        }
        fn programs(&self, owner: ProgramOwner) -> Result<Option<ProgramView>, Self::Error> {
            let (allocation, vector) = match owner {
                ProgramOwner::Mission(OwnerId(20)) => (1, &self.mission),
                ProgramOwner::Context(OwnerId(10)) => (2, &self.context),
                _ => return Err("unknown owner"),
            };
            Ok(vector.as_ref().map(|v| ProgramView {
                allocation,
                count: v.len(),
            }))
        }
        fn program_at(&self, allocation: u32, index: usize) -> Result<ProgramId, Self::Error> {
            let vector = match allocation {
                1 => &self.mission,
                2 => &self.context,
                _ => return Err("unknown allocation"),
            };
            vector
                .as_ref()
                .and_then(|v| v.get(index))
                .copied()
                .ok_or("unreadable slot")
        }
        fn program_name(&self, program: ProgramId) -> Result<&[u8], Self::Error> {
            self.names
                .get(program.0.wrapping_sub(1) as usize)
                .copied()
                .ok_or("unknown program")
        }
        fn execute(&mut self, program: ProgramId) -> Result<(), Self::Error> {
            self.runs.push(program);
            if self.fail {
                return Err("execution failed");
            }
            if self.nested && !self.executing {
                self.executing = true;
                run_context_named(self, OwnerId(10), b"Beta").unwrap();
                self.executing = false;
            }
            Ok(())
        }
    }
    // Literal selected identities from thirteen controlled original-body cases.
    #[test]
    fn native_lookup_cases() {
        let expected: [Option<u32>; 13] = [
            None,
            Some(1),
            Some(1),
            Some(3),
            Some(1),
            Some(4),
            None,
            Some(1),
            None,
            None,
            Some(1),
            Some(1),
            None,
        ];
        for (case, selected) in expected.into_iter().enumerate() {
            let mut b = Backend::default();
            match case {
                0 => {}
                1 | 11 => b.mission = Some(vec![ProgramId(1)]),
                2 => {
                    b.mission = Some(vec![ProgramId(1)]);
                    b.context = Some(vec![ProgramId(2), ProgramId(4)]);
                }
                3 => b.mission = Some(vec![ProgramId(1), ProgramId(3)]),
                4 => {
                    b.mission = Some(vec![ProgramId(2)]);
                    b.context = Some(vec![ProgramId(1)]);
                }
                5 => b.context = Some(vec![ProgramId(2), ProgramId(4)]),
                6 => {
                    b.mission = Some(vec![ProgramId(2)]);
                    b.context = Some(vec![ProgramId(2)]);
                }
                7 => {
                    b.names[0] = b"aLpHa";
                    b.mission = Some(vec![ProgramId(1)]);
                }
                8 => b.context = Some(vec![]),
                9 => b.mission = Some(vec![]),
                10 => {
                    b.mission = Some(vec![ProgramId(1)]);
                    b.context = Some(vec![ProgramId(2)]);
                    b.nested = true;
                }
                12 => b.mission = Some(vec![ProgramId(2)]),
                _ => unreachable!(),
            }
            let result = if case >= 11 {
                run_mission_named(&mut b, OwnerId(20), b"Alpha")
            } else {
                run_context_named(&mut b, OwnerId(10), b"Alpha")
            };
            assert_eq!(result, Ok(selected.map(ProgramId)), "case {case}");
            let runs = if case == 10 {
                vec![ProgramId(1), ProgramId(2)]
            } else {
                selected.map(ProgramId).into_iter().collect()
            };
            assert_eq!(b.runs, runs, "case {case}");
        }
    }
    #[test]
    fn unknown_identity_and_execution_failure_are_explicit() {
        let mut b = Backend::default();
        assert_eq!(
            run_context_named(&mut b, OwnerId(99), b"Alpha"),
            Err(LookupError::Backend("unknown context"))
        );
        b.mission = Some(vec![ProgramId(1)]);
        b.fail = true;
        assert_eq!(
            run_context_named(&mut b, OwnerId(10), b"Alpha"),
            Err(LookupError::Backend("execution failed"))
        );
        assert_eq!(b.runs, [ProgramId(1)]);
    }
    #[test]
    fn names_outside_adapter_domain_are_rejected() {
        let mut b = Backend::default();
        assert_eq!(
            run_context_named(&mut b, OwnerId(10), &[0xff]),
            Err(LookupError::NonAsciiName)
        );
        assert_eq!(
            run_context_named(&mut b, OwnerId(10), b"A\0B"),
            Err(LookupError::EmbeddedNul)
        );
        b.mission = Some(vec![ProgramId(1)]);
        b.names[0] = &[0xff];
        assert_eq!(
            run_context_named(&mut b, OwnerId(10), b"Alpha"),
            Err(LookupError::NonAsciiName)
        );
        assert!(b.runs.is_empty());
    }
    #[derive(Default)]
    struct Scheduler {
        observations: Vec<(i32, ResetState)>,
        mode: u8,
    }
    impl ProgramScheduler for Scheduler {
        type Error = &'static str;
        fn schedule(&mut self, state: &mut ResetState, delta: i32) -> Result<(), Self::Error> {
            self.observations.push((delta, state.clone()));
            if self.mode == 7 {
                state.program_counter = 7;
            } else if self.mode == 8 && self.observations.len() == 1 {
                state.program_counter = 9;
                state.short_stack_end = 0x777;
                state.field_44 = 8;
                state.continuation = 0;
                reset_and_start(state, self)?;
            } else if self.mode == 9 {
                return Err("scheduler failed");
            }
            Ok(())
        }
    }
    fn initial(begin: u32) -> ResetState {
        ResetState {
            program_counter: 0xa5a5a5a5,
            short_stack_begin: begin,
            short_stack_end: begin.wrapping_add(0x20),
            field_44: 0xa5a5a5a5,
            continuation: 0xa5,
        }
    }
    #[test]
    fn native_reset_cases() {
        let begins = [0, 2, u32::MAX, 0x12345678, 0, 8, 0x80000000, 4, 12];
        for (case, begin) in begins.into_iter().enumerate() {
            let mut state = initial(begin);
            let mut backend = Scheduler {
                mode: case as u8,
                ..Scheduler::default()
            };
            reset_and_start(&mut state, &mut backend).unwrap();
            let expected = ResetState {
                program_counter: 0,
                short_stack_begin: begin,
                short_stack_end: begin,
                field_44: 0,
                continuation: 1,
            };
            assert_eq!(backend.observations[0], (1, expected.clone()));
            if case == 8 {
                assert_eq!(
                    backend.observations,
                    [(1, expected.clone()), (1, expected.clone())]
                );
            } else {
                assert_eq!(backend.observations.len(), 1);
            }
            assert_eq!(state.program_counter, if case == 7 { 7 } else { 0 });
            let mut final_expected = expected;
            final_expected.program_counter = if case == 7 { 7 } else { 0 };
            assert_eq!(state, final_expected);
        }
    }
    #[test]
    fn scheduler_error_preserves_reset_effects() {
        let mut state = initial(12);
        let mut backend = Scheduler {
            mode: 9,
            ..Scheduler::default()
        };
        assert_eq!(
            reset_and_start(&mut state, &mut backend),
            Err("scheduler failed")
        );
        assert_eq!(state, backend.observations[0].1);
        assert_eq!(state.program_counter, 0);
    }
}
