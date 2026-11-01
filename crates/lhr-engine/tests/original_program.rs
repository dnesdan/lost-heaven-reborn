use lhr_engine::original_program::*;
use lhr_engine::script_scheduler::ActorReferenceId;

fn seed() -> ProgramSeed {
    ProgramSeed {
        address_high_byte: 0x57,
        padding_0d: [1, 2, 3],
        words_1c_2c: [4, 5, 6, 7, 8],
        padding_31: [9, 10, 11],
        word_40: 12,
        bytes_64: [13, 14, 15, 16],
    }
}
#[derive(Debug, PartialEq, Eq)]
enum Effect {
    Allocate(ProgramBuffer, u32, bool, bool),
    Free(ProgramBuffer, Option<u64>, bool, bool),
}
#[derive(Default)]
struct Backend {
    next: u64,
    effects: Vec<Effect>,
    fail: Option<usize>,
}
impl Backend {
    fn check(&self) -> Result<(), String> {
        if self.fail == Some(self.effects.len()) {
            Err("injected".into())
        } else {
            Ok(())
        }
    }
}
impl ProgramBackend for Backend {
    fn allocate(
        &mut self,
        p: &CProgram,
        b: ProgramBuffer,
        bytes: u32,
    ) -> Result<ProgramAllocation, String> {
        self.effects.push(Effect::Allocate(
            b,
            bytes,
            p.name().is_some(),
            p.source().is_some(),
        ));
        self.check()?;
        self.next += 1;
        Ok(ProgramAllocation(self.next))
    }
    fn free(
        &mut self,
        p: &CProgram,
        b: ProgramBuffer,
        a: Option<ProgramAllocation>,
    ) -> Result<(), String> {
        self.effects.push(Effect::Free(
            b,
            a.map(|v| v.0),
            p.name().is_some(),
            p.source().is_some(),
        ));
        self.check()
    }
}
struct Context {
    mission: Option<Vec<u8>>,
    frame: Option<Option<Vec<u8>>>,
    calls: Vec<&'static str>,
    available: bool,
    locale: bool,
}
impl ProgramContext for Context {
    fn default_c_locale(&self) -> bool {
        self.locale
    }
    fn mission_path(&mut self) -> Result<Option<&[u8]>, String> {
        self.calls.push("mission");
        if !self.available {
            return Err("missing real mission".into());
        }
        Ok(self.mission.as_deref())
    }
    fn owner_frame_name(
        &mut self,
        _: Option<ActorReferenceId>,
    ) -> Result<OwnerFrameName<'_>, String> {
        self.calls.push("frame");
        Ok(match &self.frame {
            None => OwnerFrameName::NoFrame,
            Some(name) => OwnerFrameName::Frame(name.as_deref()),
        })
    }
}
fn context() -> Context {
    Context {
        mission: Some(b"root\\mission".to_vec()),
        frame: None,
        calls: vec![],
        available: true,
        locale: true,
    }
}
fn payload(name: &[u8], source: &[u8]) -> Vec<u8> {
    let mut bytes = vec![0xef];
    bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(&(source.len() as u32).to_le_bytes());
    bytes.extend_from_slice(source);
    bytes
}
fn replacement(source: Option<&[u8]>) -> ProgramReplacement {
    ProgramReplacement {
        mission: Some(b"MISSION".to_vec()),
        name: None,
        frame: None,
        source: source.map(Vec::from),
    }
}

#[test]
fn constructor_retains_pointer_byte_and_untouched_seed_without_runtime_init() {
    let original = seed();
    let p = CProgram::construct(original.clone());
    assert_eq!(p.seed(), &original);
    assert!(p.name().is_none() && p.source().is_none() && p.owner().is_none());
    assert_eq!(p.phase(), ProgramPhase::Source);
}
#[test]
fn load_keeps_advertised_bytes_but_set_copies_only_c_prefix() {
    let mut p = CProgram::construct(seed());
    let mut b = Backend::default();
    let mut c = context();
    c.available = false;
    let bytes = payload(b"n\0tail", b"s\0tail");
    let mut reader = ProgramSliceReader::new(&bytes);
    p.load_source_code(&mut reader, &[], &mut c, &mut b)
        .unwrap();
    assert_eq!(reader.position(), bytes.len());
    assert!(c.calls.is_empty());
    assert_eq!(p.name().unwrap().bytes, b"n\0tail\0");
    assert_eq!(p.source().unwrap().bytes, b"s\0tail\0");
    p.set_source_code(Some(b"new\0ignored"), &mut b).unwrap();
    assert_eq!(p.source().unwrap().bytes, b"new\0");
    assert_eq!(
        b.effects[2],
        Effect::Free(ProgramBuffer::Source, Some(2), true, true)
    );
    assert_eq!(
        b.effects[3],
        Effect::Allocate(ProgramBuffer::Source, 4, true, false)
    );
}
#[test]
fn nullable_source_differs_from_allocated_empty_and_zero_advertised_length() {
    let mut p = CProgram::construct(seed());
    let mut b = Backend::default();
    p.set_source_code(Some(b""), &mut b).unwrap();
    assert_eq!(p.source().unwrap().bytes, b"\0");
    p.set_source_code(None, &mut b).unwrap();
    assert!(p.source().is_none());
    p.load_source_code(
        &mut ProgramSliceReader::new(&payload(b"", b"")),
        &[],
        &mut context(),
        &mut b,
    )
    .unwrap();
    assert!(p.name().is_none() && p.source().is_none());
    assert_eq!(b.effects.len(), 2);
}
#[test]
fn reverse_first_match_requires_null_equality_and_real_frame_null_name_is_empty() {
    let mut p = CProgram::construct(seed());
    let mut b = Backend::default();
    let mut c = context();
    let mut wrong_name = replacement(Some(b"wrong"));
    wrong_name.name = Some(vec![]);
    let rows = [
        replacement(Some(b"first")),
        replacement(Some(b"last")),
        wrong_name,
    ];
    p.try_change_program_data(&rows, &mut c, &mut b).unwrap();
    assert_eq!(p.source().unwrap().c_bytes(), Some(&b"last"[..]));
    assert_eq!(c.calls, ["mission", "frame", "mission", "frame"]);
    c.frame = Some(None);
    let mut empty_frame = replacement(Some(b"frame"));
    empty_frame.frame = Some(vec![]);
    p.try_change_program_data(&[empty_frame, replacement(Some(b"null"))], &mut c, &mut b)
        .unwrap();
    assert_eq!(p.source().unwrap().c_bytes(), Some(&b"frame"[..]));
}
#[test]
fn path_component_and_default_locale_contract() {
    assert!(equal_final_path(
        b"A\\B\\Mission\0ignored",
        b"other\\mISSION"
    ));
    assert!(equal_final_path(b"a\\", b""));
    assert!(!equal_final_path(b"a/Mission", b"mission"));
    assert!(!equal_final_path(b"\xc0", b"\xe0"));
    let mut p = CProgram::construct(seed());
    let mut c = context();
    c.locale = false;
    assert_eq!(
        p.try_change_program_data(&[replacement(None)], &mut c, &mut Backend::default()),
        Err(ProgramError::UnsupportedLocale)
    );
    assert!(c.calls.is_empty());
    c.locale = true;
    c.mission = None;
    assert_eq!(
        p.try_change_program_data(&[replacement(None)], &mut c, &mut Backend::default()),
        Err(ProgramError::UnsupportedNullPath)
    );
}
#[test]
fn read_failure_retains_disposal_and_published_incomplete_name() {
    let mut p = CProgram::construct(seed());
    let mut b = Backend::default();
    p.set_source_code(Some(b"old"), &mut b).unwrap();
    let bytes = [1, 3, 0, 0, 0, b'x'];
    let mut reader = ProgramSliceReader::new(&bytes);
    assert!(p
        .load_source_code(&mut reader, &[], &mut context(), &mut b)
        .is_err());
    assert!(p.source().is_none());
    assert_eq!(p.phase(), ProgramPhase::Interrupted);
    assert!(!p.name().unwrap().complete);
    assert!(p.name().unwrap().c_bytes().is_none());
    assert_eq!(reader.position(), 5);
    assert_eq!(
        b.effects[1],
        Effect::Free(ProgramBuffer::Source, Some(1), false, true)
    );
}
#[test]
fn allocation_and_free_failures_stop_at_ordered_prefix_and_huge_length_is_bounded() {
    let mut p = CProgram::construct(seed());
    let mut b = Backend::default();
    p.set_source_code(Some(b"old"), &mut b).unwrap();
    b.fail = Some(2);
    assert!(p.set_source_code(Some(b"new"), &mut b).is_err());
    assert_eq!(p.source().unwrap().bytes, b"old\0");
    assert_eq!(p.source().unwrap().c_bytes(), None);
    assert!(p.source().unwrap().disposal_uncertain);
    assert_eq!(b.effects.len(), 2);
    assert_eq!(p.phase(), ProgramPhase::Poisoned);
    assert_eq!(p.destruct(&mut b), Err(ProgramError::Unavailable));
    assert_eq!(b.effects.len(), 2);
    let mut p = CProgram::construct(seed());
    let mut bytes = vec![0];
    bytes.extend_from_slice(&MAX_PROGRAM_BUFFER_BYTES.to_le_bytes());
    let mut reader = ProgramSliceReader::new(&bytes);
    let mut b = Backend::default();
    assert_eq!(
        p.load_source_code(&mut reader, &[], &mut context(), &mut b),
        Err(ProgramError::InvalidExtent)
    );
    assert_eq!(reader.position(), 5);
    assert!(b.effects.is_empty());
}

struct UnwindBackend {
    memory: Backend,
    panic_free: Option<ProgramBuffer>,
}

#[test]
fn load_clear_free_errors_preserve_completed_prefix_and_success_restores_admission() {
    for failed_free in [Some(1), Some(2), None] {
        let mut program = CProgram::construct(seed());
        let mut backend = Backend::default();
        program
            .load_source_code(
                &mut ProgramSliceReader::new(&payload(b"name", b"code")),
                &[],
                &mut context(),
                &mut backend,
            )
            .unwrap();
        backend.effects.clear();
        backend.fail = failed_free;
        let replacement = payload(b"new name", b"new code");
        let mut reader = ProgramSliceReader::new(&replacement);
        let result = program.load_source_code(&mut reader, &[], &mut context(), &mut backend);
        match failed_free {
            Some(count) => {
                assert_eq!(
                    result,
                    Err(ProgramError::Effect {
                        stage: "free",
                        detail: "injected".into()
                    })
                );
                assert_eq!(program.phase(), ProgramPhase::Poisoned);
                assert_eq!(reader.position(), 0);
                assert_eq!(backend.effects.len(), count);
                assert_eq!(program.name().is_some(), count == 1);
                if let Some(name) = program.name() {
                    assert_eq!(name.bytes, b"name\0");
                    assert!(name.disposal_uncertain);
                    assert_eq!(name.c_bytes(), None);
                }
                let source = program.source().unwrap();
                assert_eq!(source.bytes, b"code\0");
                assert_eq!(source.disposal_uncertain, count == 2);
                assert_eq!(
                    program.destruct(&mut backend),
                    Err(ProgramError::Unavailable)
                );
                assert_eq!(backend.effects.len(), count);
            }
            None => {
                result.unwrap();
                assert_eq!(program.phase(), ProgramPhase::Source);
                assert_eq!(reader.position(), replacement.len());
                assert_eq!(
                    program.name().unwrap().c_bytes(),
                    Some(b"new name".as_slice())
                );
                assert_eq!(
                    program.source().unwrap().c_bytes(),
                    Some(b"new code".as_slice())
                );
                assert!(!program.name().unwrap().disposal_uncertain);
                assert!(!program.source().unwrap().disposal_uncertain);
                assert_eq!(
                    backend.effects,
                    [
                        Effect::Free(ProgramBuffer::Name, Some(1), true, true),
                        Effect::Free(ProgramBuffer::Source, Some(2), false, true),
                        Effect::Allocate(ProgramBuffer::Name, 9, false, false),
                        Effect::Allocate(ProgramBuffer::Source, 9, true, false),
                    ]
                );
                program.destruct(&mut backend).unwrap();
            }
        }
    }
}
impl ProgramBackend for UnwindBackend {
    fn allocate(
        &mut self,
        program: &CProgram,
        buffer: ProgramBuffer,
        bytes: u32,
    ) -> Result<ProgramAllocation, String> {
        self.memory.allocate(program, buffer, bytes)
    }
    fn free(
        &mut self,
        program: &CProgram,
        buffer: ProgramBuffer,
        allocation: Option<ProgramAllocation>,
    ) -> Result<(), String> {
        self.memory.free(program, buffer, allocation)?;
        if self.panic_free == Some(buffer) {
            assert_eq!(program.phase(), ProgramPhase::Poisoned);
            let (string, expected) = match buffer {
                ProgramBuffer::Name => (program.name().unwrap(), b"name\0".as_slice()),
                ProgramBuffer::Source => (program.source().unwrap(), b"code\0".as_slice()),
                _ => unreachable!(),
            };
            // Native pointer observations and underlying bytes remain intact.
            // c_bytes is host admission and refuses uncertain external backing.
            assert_eq!(Some(string.allocation), allocation);
            assert_eq!(string.bytes, expected);
            assert!(string.complete && !string.released && string.disposal_uncertain);
            assert_eq!(string.c_bytes(), None);
            panic!("external free completed before unwind");
        }
        Ok(())
    }
}

#[test]
fn clear_unwind_retains_exact_prefix_and_never_replays_name_or_source_free() {
    for (buffer, replace_source) in [
        (ProgramBuffer::Name, false),
        (ProgramBuffer::Source, false),
        (ProgramBuffer::Source, true),
    ] {
        let mut program = CProgram::construct(seed());
        let mut backend = UnwindBackend {
            memory: Backend::default(),
            panic_free: None,
        };
        program
            .load_source_code(
                &mut ProgramSliceReader::new(&payload(b"name", b"code")),
                &[],
                &mut context(),
                &mut backend,
            )
            .unwrap();
        let name = program.name().unwrap().allocation;
        let source = program.source().unwrap().allocation;
        backend.memory.effects.clear();
        backend.panic_free = Some(buffer);
        let replacement = payload(b"new name", b"new code");
        let mut reader = ProgramSliceReader::new(&replacement);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if replace_source {
                program.set_source_code(Some(b"replacement"), &mut backend)
            } else {
                program.load_source_code(&mut reader, &[], &mut context(), &mut backend)
            }
        }));
        assert!(result.is_err());
        assert_eq!(program.phase(), ProgramPhase::Poisoned);
        assert_eq!(reader.position(), 0);
        let expected = if replace_source {
            vec![Effect::Free(
                ProgramBuffer::Source,
                Some(source.0),
                true,
                true,
            )]
        } else if buffer == ProgramBuffer::Name {
            vec![Effect::Free(ProgramBuffer::Name, Some(name.0), true, true)]
        } else {
            vec![
                Effect::Free(ProgramBuffer::Name, Some(name.0), true, true),
                Effect::Free(ProgramBuffer::Source, Some(source.0), false, true),
            ]
        };
        assert_eq!(backend.memory.effects, expected);
        assert_eq!(
            program.name().is_some(),
            replace_source || buffer == ProgramBuffer::Name
        );
        if let Some(retained) = program.name() {
            assert_eq!(retained.allocation, name);
            assert_eq!(retained.bytes, b"name\0");
            assert_eq!(retained.disposal_uncertain, buffer == ProgramBuffer::Name);
        }
        let retained = program.source().unwrap();
        assert_eq!(retained.allocation, source);
        assert_eq!(retained.bytes, b"code\0");
        assert_eq!(retained.disposal_uncertain, buffer == ProgramBuffer::Source);
        backend.panic_free = None;
        assert_eq!(
            program.set_source_code(None, &mut backend),
            Err(ProgramError::Unavailable)
        );
        assert_eq!(
            program.load_source_code(&mut reader, &[], &mut context(), &mut backend),
            Err(ProgramError::Unavailable)
        );
        assert_eq!(
            program.destruct(&mut backend),
            Err(ProgramError::Unavailable)
        );
        assert_eq!(reader.position(), 0);
        assert_eq!(backend.memory.effects, expected);
        // Explicit host discard releases Rust snapshots, never uncertain native
        // backing. No Drop callback retries either this token or later owners.
        drop(program);
        assert_eq!(backend.memory.effects, expected);
    }
}
#[test]
fn destructor_preserves_pointer_observations_and_is_distinct_from_done() {
    let mut p = CProgram::construct(seed());
    let mut b = Backend::default();
    p.load_source_code(
        &mut ProgramSliceReader::new(&payload(b"name", b"code")),
        &[],
        &mut context(),
        &mut b,
    )
    .unwrap();
    let name = p.name().unwrap().allocation;
    let source = p.source().unwrap().allocation;
    p.destruct(&mut b).unwrap();
    assert_eq!(p.phase(), ProgramPhase::Dead);
    assert_eq!(p.name().unwrap().allocation, name);
    assert_eq!(p.source().unwrap().allocation, source);
    assert!(p.name().unwrap().released && p.source().unwrap().released);
    assert_eq!(
        &b.effects[2..],
        [
            Effect::Free(ProgramBuffer::Name, Some(1), true, true),
            Effect::Free(ProgramBuffer::Source, Some(2), true, true),
            Effect::Free(ProgramBuffer::ReturnStack, None, true, true),
            Effect::Free(ProgramBuffer::Instructions, None, true, true)
        ]
    );
    assert_eq!(p.destruct(&mut b), Err(ProgramError::Unavailable));
    assert_eq!(
        p.set_source_code(None, &mut b),
        Err(ProgramError::Unavailable)
    );
}

#[test]
fn decoded_patch_files_feed_source_loader_with_last_matching_record_priority() {
    use lhr_engine::program_patch::ProgramPatchTable;
    fn patch(source: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        for string in [Some(b"MISSION\0".as_slice()), None, None, Some(source)] {
            body.extend_from_slice(&(string.map_or(0, |s| s.len()) as i32).to_le_bytes());
            if let Some(string) = string {
                body.extend(string.iter().map(|byte| byte ^ 0x85));
            }
        }
        let mut bytes = 0x64u16.to_le_bytes().to_vec();
        bytes.extend_from_slice(&((body.len() + 6) as i32).to_le_bytes());
        bytes.extend(body);
        bytes
    }
    let mut patches = ProgramPatchTable::default();
    patches.load(&patch(b"old\0")).unwrap();
    patches.load(&patch(b"new\0")).unwrap();
    let mut program = CProgram::construct(seed());
    let mut backend = Backend::default();
    let mut reader = ProgramSliceReader::new(&[0xef, 0, 0, 0, 0, 0, 0, 0, 0]);
    program
        .load_source_code(
            &mut reader,
            patches.replacements(),
            &mut context(),
            &mut backend,
        )
        .unwrap();
    assert_eq!(program.source().unwrap().c_bytes(), Some(b"new".as_slice()));
    assert!(program.name().is_none());
}
