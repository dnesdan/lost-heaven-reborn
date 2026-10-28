//! Synthetic decoder/owner tests; no original parameter decoding or VM execution.
use super::*;
use crate::program_instructions::INSTRUCTION_KEYWORDS;

#[derive(Default)]
struct Sink {
    instructions: Vec<(i32, Vec<u8>)>,
    calls: Vec<(i32, Vec<u8>)>,
    clears: usize,
    fail_clear: bool,
}

impl CompileBackend for Sink {
    type Error = &'static str;

    fn clear_instructions(&mut self) -> Result<(), Self::Error> {
        self.clears += 1;
        if self.fail_clear {
            return Err("clear unavailable");
        }
        self.instructions.clear();
        Ok(())
    }

    fn decode_and_append(
        &mut self,
        opcode: i32,
        parameters: &mut [u8],
    ) -> Result<bool, Self::Error> {
        assert_eq!(parameters.last(), Some(&0));
        self.calls.push((opcode, parameters.to_vec()));
        if parameters.starts_with(b"unavailable") {
            return Err("parameter decoder unavailable");
        }
        if parameters.starts_with(b"reject") {
            return Ok(false);
        }
        if parameters.starts_with(b"mutate,") {
            parameters[6] = 0;
            return Ok(false);
        }
        if parameters.starts_with(b"flip") {
            *parameters.last_mut().unwrap() = b'/';
        }
        self.instructions.push((opcode, parameters.to_vec()));
        Ok(true)
    }
}

fn sentinel() -> CompileDiagnostic {
    CompileDiagnostic {
        position: 901,
        length: 902,
    }
}

fn run(
    bytes: &[u8],
    stop: bool,
) -> (CompileProgress, Vec<u8>, Sink, CompileDiagnostic) {
    let mut source = bytes.to_vec();
    let mut sink = Sink::default();
    let mut diagnostic = sentinel();
    let progress = compile(
        Some(&mut source),
        stop,
        Some(&mut diagnostic),
        ParserLocale::AsciiC,
        &mut sink,
    )
    .unwrap();
    (progress, source, sink, diagnostic)
}

#[test]
fn null_and_empty_source_clear_live_instructions_without_resetting_diagnostic() {
    for bytes in [None, Some(&b"\0untouched"[..])] {
        let mut source = bytes.map(<[u8]>::to_vec);
        let mut sink = Sink {
            instructions: vec![(6, vec![0])],
            ..Sink::default()
        };
        let mut diagnostic = sentinel();
        assert_eq!(
            compile(
                source.as_deref_mut(),
                false,
                Some(&mut diagnostic),
                ParserLocale::AsciiC,
                &mut sink,
            ),
            Ok(CompileProgress::default())
        );
        assert_eq!(sink.clears, 1);
        assert!(sink.instructions.is_empty());
        assert!(sink.calls.is_empty());
        assert_eq!(diagnostic, sentinel());
        assert_eq!(source.as_deref(), bytes);
    }
}

#[test]
fn failed_clear_precedes_even_missing_terminator_admission() {
    let mut source = b"END".to_vec();
    let mut sink = Sink {
        instructions: vec![(6, vec![0])],
        fail_clear: true,
        ..Sink::default()
    };
    assert_eq!(
        compile(Some(&mut source), false, None, ParserLocale::AsciiC, &mut sink),
        Err(CompileError::Backend {
            stage: CompileStage::ClearInstructions,
            offset: 0,
            error: "clear unavailable",
            progress: CompileProgress::default(),
        })
    );
    assert_eq!(source, b"END");
    assert_eq!(sink.instructions, [(6, vec![0])]);
    assert!(sink.calls.is_empty());
}

#[test]
fn missing_terminator_preserves_bytes_after_the_reached_clear() {
    let mut source = b"END\nWAIT 2".to_vec();
    let mut sink = Sink {
        instructions: vec![(6, vec![0])],
        ..Sink::default()
    };
    assert_eq!(
        compile(Some(&mut source), false, None, ParserLocale::AsciiC, &mut sink),
        Err(CompileError::Parser {
            offset: 0,
            error: ProgramParserError::MissingTerminator,
            progress: CompileProgress::default(),
        })
    );
    assert!(sink.instructions.is_empty());
    assert_eq!(source, b"END\nWAIT 2");
}

#[test]
fn source_delimiters_are_modified_in_place_and_crlf_controls_are_only_skipped() {
    let (progress, source, sink, diagnostic) =
        run(b" END\r\n\tWAIT  7\n//ignored\rEND\0tail", false);
    assert_eq!(progress, CompileProgress { errors: 0, accepted: 3 });
    assert_eq!(source, b" END\0\n\tWAIT  7\0//ignored\0END\0tail");
    assert_eq!(sink.calls, [(6, b"\0".to_vec()), (4, b"7\0".to_vec()), (6, b"\0".to_vec())]);
    assert_eq!(diagnostic, sentinel());
}

#[test]
fn initial_tab_is_not_trimmed_but_tab_after_a_separator_is_skipped() {
    let (progress, _, sink, diagnostic) = run(b"\tEND\n\tEND\0", false);
    assert_eq!(progress, CompileProgress { errors: 1, accepted: 1 });
    assert_eq!(sink.calls, [(6, vec![0])]);
    assert_eq!(diagnostic, CompileDiagnostic { position: 0, length: 4 });
}

#[test]
fn whole_line_comments_allow_raw_bytes_but_inline_comment_text_reaches_decoder() {
    let (progress, _, sink, diagnostic) =
        run(b" //\xff comment\nWAIT 4 //still parameters\nEND\0", false);
    assert_eq!(progress, CompileProgress { errors: 0, accepted: 2 });
    assert_eq!(sink.calls[0], (4, b"4 //still parameters\0".to_vec()));
    assert_eq!(sink.calls[1], (6, vec![0]));
    assert_eq!(diagnostic, sentinel());
}

#[test]
fn first_nul_hides_later_commands_and_unsupported_bytes() {
    let (progress, source, sink, _) = run(b"END\0\xffWAIT 3\nBAD\0", false);
    assert_eq!(progress.accepted, 1);
    assert_eq!(source, b"END\0\xffWAIT 3\nBAD\0");
    assert_eq!(sink.calls, [(6, vec![0])]);
}

#[test]
fn continuing_reports_the_last_error_at_its_trimmed_original_position() {
    let input = b" BAD\nEND\n  ALSO_BAD\0";
    let (progress, source, sink, diagnostic) = run(input, false);
    assert_eq!(progress, CompileProgress { errors: 2, accepted: 1 });
    assert_eq!(diagnostic, CompileDiagnostic { position: 11, length: 8 });
    assert_eq!(sink.calls, [(6, vec![0])]);
    assert_eq!(source, b" BAD\0END\0  ALSO_BAD\0");
}

#[test]
fn stop_on_error_leaves_the_unvisited_source_suffix_unchanged() {
    let (progress, source, sink, diagnostic) = run(b"BAD\r\nEND\nWAIT 3\0", true);
    assert_eq!(progress, CompileProgress { errors: 1, accepted: 0 });
    assert_eq!(source, b"BAD\0\nEND\nWAIT 3\0");
    assert!(sink.calls.is_empty());
    assert_eq!(diagnostic, CompileDiagnostic { position: 0, length: 3 });
}

#[test]
fn one_line_flag_does_not_stop_after_successful_lines() {
    let (progress, _, sink, diagnostic) = run(b"END\nWAIT 3\nEND\0", true);
    assert_eq!(progress, CompileProgress { errors: 0, accepted: 3 });
    assert_eq!(sink.instructions.len(), 3);
    assert_eq!(diagnostic, sentinel());
}

#[test]
fn parameter_rejections_count_as_native_errors_without_appending_instructions() {
    for stop in [false, true] {
        let (progress, _, sink, diagnostic) = run(b"END\nWAIT reject\nEND\0", stop);
        assert_eq!(progress.errors, 1);
        assert_eq!(progress.accepted, if stop { 1 } else { 2 });
        assert_eq!(sink.calls.len(), if stop { 2 } else { 3 });
        assert_eq!(diagnostic, CompileDiagnostic { position: 4, length: 11 });
        assert!(sink.instructions.iter().all(|(opcode, _)| *opcode == 6));
    }
}

#[test]
fn unavailable_decoder_is_an_interruption_not_a_skipped_instruction() {
    let mut source = b"END\nBAD\nWAIT unavailable\nEND\0".to_vec();
    let mut sink = Sink::default();
    let mut diagnostic = sentinel();
    assert_eq!(
        compile(Some(&mut source), false, Some(&mut diagnostic), ParserLocale::AsciiC, &mut sink),
        Err(CompileError::Backend {
            stage: CompileStage::DecodeParameters,
            offset: 13,
            error: "parameter decoder unavailable",
            progress: CompileProgress { errors: 1, accepted: 1 },
        })
    );
    assert_eq!(diagnostic, CompileDiagnostic { position: 4, length: 3 });
    assert_eq!(sink.instructions, [(6, vec![0])]);
    assert_eq!(source, b"END\0BAD\0WAIT unavailable\0END\0");
}

#[test]
fn leading_empty_or_space_only_line_keeps_the_recognizers_admission_error() {
    for (input, start) in [(&b"\nEND\0"[..], 0), (&b"  \rEND\0"[..], 2)] {
        let mut source = input.to_vec();
        let mut sink = Sink::default();
        let mut diagnostic = sentinel();
        assert_eq!(
            compile(Some(&mut source), false, Some(&mut diagnostic), ParserLocale::AsciiC, &mut sink),
            Err(CompileError::Parser {
                offset: start,
                error: ProgramParserError::EmptyInstruction,
                progress: CompileProgress::default(),
            })
        );
        assert!(sink.calls.is_empty());
        assert_eq!(diagnostic, sentinel());
        assert_eq!(source[start], 0);
    }
}

#[test]
fn blank_control_runs_after_an_instruction_do_not_create_empty_instructions() {
    let (progress, source, sink, _) = run(b"END\r\n\n\t\x0b\x0c\x1fEND\n\0", false);
    assert_eq!(progress, CompileProgress { errors: 0, accepted: 2 });
    assert_eq!(sink.calls, [(6, vec![0]), (6, vec![0])]);
    assert_eq!(source, b"END\0\n\n\t\x0b\x0c\x1fEND\0\0");
}

#[test]
fn parameter_mutations_remain_visible_and_error_extent_uses_the_original_line_end() {
    let (progress, source, sink, diagnostic) = run(b" WAIT mutate,tail\nEND\0", false);
    assert_eq!(progress, CompileProgress { errors: 1, accepted: 1 });
    assert_eq!(source, b" WAIT mutate\0tail\0END\0");
    assert_eq!(diagnostic, CompileDiagnostic { position: 1, length: 16 });
    assert_eq!(sink.instructions, [(6, vec![0])]);
}

#[test]
fn next_line_scan_reads_a_separator_modified_by_the_decoder() {
    let (progress, source, sink, _) = run(b"WAIT flip\n/ignored\nEND\0", false);
    assert_eq!(progress, CompileProgress { errors: 0, accepted: 2 });
    assert_eq!(sink.calls, [(4, b"flip\0".to_vec()), (6, vec![0])]);
    assert_eq!(source, b"WAIT flip//ignored\0END\0");
}

#[test]
fn non_ascii_control_classification_stops_at_the_reached_byte() {
    let mut source = b"END\n\xffEND\0".to_vec();
    let mut sink = Sink::default();
    assert_eq!(
        compile(Some(&mut source), false, None, ParserLocale::AsciiC, &mut sink),
        Err(CompileError::Parser {
            offset: 4,
            error: ProgramParserError::UnsupportedCrtByte { offset: 0, byte: 255 },
            progress: CompileProgress { errors: 0, accepted: 1 },
        })
    );
    assert_eq!(source, b"END\0\xffEND\0");
    assert_eq!(sink.instructions, [(6, vec![0])]);
}

#[test]
fn all_keywords_reach_the_existing_recognizer_without_invented_parameter_support() {
    for keyword in INSTRUCTION_KEYWORDS {
        let mut input = keyword.name.as_bytes().to_vec();
        input.extend_from_slice(b"unavailable\0");
        let mut sink = Sink::default();
        assert_eq!(
            compile(Some(&mut input), false, None, ParserLocale::AsciiC, &mut sink),
            Err(CompileError::Backend {
                stage: CompileStage::DecodeParameters,
                offset: keyword.name.len(),
                error: "parameter decoder unavailable",
                progress: CompileProgress::default(),
            }),
            "{}",
            keyword.name
        );
        assert_eq!(sink.calls[0].0, keyword.id);
        assert!(sink.instructions.is_empty());
    }
}

#[test]
fn no_diagnostic_output_does_not_change_error_or_append_behavior() {
    let mut source = b"BAD\nWAIT reject\nEND\0".to_vec();
    let mut sink = Sink::default();
    assert_eq!(
        compile(Some(&mut source), false, None, ParserLocale::AsciiC, &mut sink),
        Ok(CompileProgress { errors: 2, accepted: 1 })
    );
    assert_eq!(sink.instructions, [(6, vec![0])]);
}

#[test]
fn bare_and_space_terminated_keywords_supply_an_empty_c_string() {
    for input in [&b"END\0"[..], &b"  END   \0"[..]] {
        let (progress, _, sink, diagnostic) = run(input, false);
        assert_eq!(progress, CompileProgress { errors: 0, accepted: 1 });
        assert_eq!(sink.calls, [(6, vec![0])]);
        assert_eq!(diagnostic, sentinel());
    }
}
