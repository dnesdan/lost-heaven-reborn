//! Bounded source-backed C_program::Compile control flow, not a complete compiler.
//!
//! Reference: mafia-re dc867c3238488de27d094bb79b6550e392566e3c,
//! game/mission/C_program.cpp, GAME 0x46c770 (entry RVA 0x6c770).
//! This composes the existing 398-keyword recognizer with a caller's actual
//! instruction owner and parameter decoder. No default decoder accepts unknown
//! parameters, and this module neither initializes nor executes mission scripts.
//! Source comparison alone does not establish original-runtime equivalence.
#![forbid(unsafe_code)]

use crate::program_instructions::decode_instruction;
use crate::program_parser::{ParserLocale, ProgramParserError, c_length};

/// The original writes this pair only on a native compile error, and only when
/// both output pointers are supplied. A single optional value models that pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompileDiagnostic {
    pub position: usize,
    pub length: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompileProgress {
    pub errors: u32,
    pub accepted: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompileStage {
    ClearInstructions,
    DecodeParameters,
}

/// An interruption is not a native compile-error count. The input, previously
/// appended instructions and decoder-owned partial effects remain as reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileError<E> {
    Parser {
        /// Start of the slice supplied to the parser; byte offsets in `error`
        /// are relative to that slice, not necessarily the complete source.
        offset: usize,
        error: ProgramParserError,
        progress: CompileProgress,
    },
    Backend {
        stage: CompileStage,
        offset: usize,
        error: E,
        progress: CompileProgress,
    },
}

/// Boundary to the real compiled-instruction owner. Implementations must retain
/// reached partial effects on Err; they must not install a permissive fallback.
/// Parameter decoding/ownership is deliberately not inferred from opcode names.
pub trait CompileBackend {
    type Error;

    /// Clear the live instruction collection before testing the source. The
    /// backend owns the correct lifetime policy for any instruction payloads.
    fn clear_instructions(&mut self) -> Result<(), Self::Error>;

    /// Decode one recognized instruction's writable, NUL-terminated parameter
    /// slice. Initialize the instruction record as required by its decoder;
    /// append it only on successful decoding. Return false for an ordinary
    /// parameter rejection, Err for an unavailable dependency or host failure.
    /// The slice aliases the caller's source: in-place changes are not restored.
    fn decode_and_append(
        &mut self,
        opcode: i32,
        parameters: &mut [u8],
    ) -> Result<bool, Self::Error>;
}

/// Execute Compile's ordered line scan over the first C string.
///
/// `stop_on_error` is the original `one_line` argument: it stops on the first
/// compile error, NOT after the first successful line. Only leading ASCII spaces
/// and whole-line `//` comments are skipped. Inline comments are parameters.
/// CR and LF terminate lines; after a separator the original skips control bytes
/// below ASCII space (including tabs), but an initial tab is not leading space.
/// Error positions refer to the trimmed line; the last error wins when continuing.
///
/// Null/empty source clears instructions without changing the diagnostic. Empty
/// physical lines that reach DecodeInstruction retain its explicit unsupported
/// boundary; they are not silently dropped. Non-ASCII control classification is
/// likewise not guessed from the host's `char` signedness. Comment bodies may
/// contain arbitrary bytes because they do not reach that classification.
///
/// There is no transactional rollback and no implicit script activation. On an
/// interruption, `progress` counts only callbacks which completed successfully.
pub fn compile<B: CompileBackend>(
    source: Option<&mut [u8]>,
    stop_on_error: bool,
    mut diagnostic: Option<&mut CompileDiagnostic>,
    locale: ParserLocale,
    backend: &mut B,
) -> Result<CompileProgress, CompileError<B::Error>> {
    let mut progress = CompileProgress::default();
    backend
        .clear_instructions()
        .map_err(|error| CompileError::Backend {
            stage: CompileStage::ClearInstructions,
            offset: 0,
            error,
            progress,
        })?;
    let Some(source) = source else {
        return Ok(progress);
    };
    let length = c_length(source).map_err(|error| CompileError::Parser {
        offset: 0,
        error,
        progress,
    })?;
    if length == 0 {
        return Ok(progress);
    }

    let mut line = 0;
    let mut end = 0;
    loop {
        while end < length && !matches!(source[end], 0 | b'\r' | b'\n') {
            end += 1;
        }
        let last = source[end] == 0;
        source[end] = 0;
        while source[line] == b' ' {
            line += 1;
        }
        if !source[line..end].starts_with(b"//") {
            let mut compared = 0;
            let opcode = decode_instruction(&source[line..=end], &mut compared, locale).map_err(
                |error| CompileError::Parser {
                    offset: line,
                    error,
                    progress,
                },
            )?;
            let accepted = if opcode == -1 {
                false
            } else {
                let mut params = line + compared;
                while source[params] == b' ' {
                    params += 1;
                }
                backend
                    .decode_and_append(opcode, &mut source[params..=end])
                    .map_err(|error| CompileError::Backend {
                        stage: CompileStage::DecodeParameters,
                        offset: params,
                        error,
                        progress,
                    })?
            };
            if accepted {
                progress.accepted += 1;
            } else {
                progress.errors += 1;
                if let Some(output) = diagnostic.as_deref_mut() {
                    *output = CompileDiagnostic {
                        position: line,
                        length: end - line,
                    };
                }
                if stop_on_error {
                    break;
                }
            }
        }
        if last {
            break;
        }
        // Read the live separator after the decoder, rather than a copied line.
        // The backend can only modify the current slice, so the original final
        // NUL still bounds this scan whenever `last` is false.
        loop {
            let byte = source[end];
            if !byte.is_ascii() {
                return Err(CompileError::Parser {
                    offset: end,
                    error: ProgramParserError::UnsupportedCrtByte { offset: 0, byte },
                    progress,
                });
            }
            if byte >= b' ' {
                break;
            }
            end += 1;
            if source[end] == 0 {
                break;
            }
        }
        if source[end] == 0 {
            break;
        }
        line = end;
    }
    Ok(progress)
}

#[cfg(test)]
#[path = "program_compiler_tests.rs"]
mod tests;
