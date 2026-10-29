//! Owned Game C_program source lifetime, separate from the diagnostic scheduler.
//!
//! Source loading and replacement do not compile instructions or initialize a
//! running program. Init, Done, register ownership and instruction payloads are
//! outside this source-only lifetime. Explicit destruction calls the allocator;
//! dropping this host projection cannot perform native allocator effects.
use crate::script_scheduler::ActorReferenceId;

/// Host admission per raw name/source allocation, including its terminating
/// byte. This bounds untrusted advertised lengths; it is not a native limit.
pub const MAX_PROGRAM_BUFFER_BYTES: u32 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgramAllocation(pub u64);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramBuffer {
    Name,
    Source,
    ReturnStack,
    Instructions,
}
/// Unique nonzero allocations belong to one compatible backend identity domain.
/// Failed callbacks stop the operation; they must retain any unresolved backing.
pub trait ProgramBackend {
    fn allocate(
        &mut self,
        program: &CProgram,
        buffer: ProgramBuffer,
        bytes: u32,
    ) -> Result<ProgramAllocation, String>;
    fn free(
        &mut self,
        program: &CProgram,
        buffer: ProgramBuffer,
        allocation: Option<ProgramAllocation>,
    ) -> Result<(), String>;
}
/// Each call is one original chunk read. A failing reader may have written a
/// prefix; that storage remains inspectable but is not an admitted C string.
/// Readers must enforce open/read mode and the current nested chunk boundary.
pub trait ProgramReader {
    fn read_exact(&mut self, destination: &mut [u8]) -> Result<(), String>;
}
/// Bounded original-format intake for an already selected read-mode chunk.
pub struct ProgramSliceReader<'a> {
    bytes: &'a [u8],
    position: usize,
}
impl<'a> ProgramSliceReader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    pub fn position(&self) -> usize {
        self.position
    }
}
impl ProgramReader for ProgramSliceReader<'_> {
    fn read_exact(&mut self, destination: &mut [u8]) -> Result<(), String> {
        let end = self
            .position
            .checked_add(destination.len())
            .ok_or("read extent")?;
        let source = self
            .bytes
            .get(self.position..end)
            .ok_or("truncated chunk")?;
        destination.copy_from_slice(source);
        self.position = end;
        Ok(())
    }
}
/// Fields untouched by source construction. No scheduler state is invented.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramSeed {
    /// PUSH ECX / [ESP+3] copies the high byte of the native this address into
    /// both vector allocator bytes (+0xc and +0x30).
    pub address_high_byte: u8,
    pub padding_0d: [u8; 3],
    pub words_1c_2c: [u32; 5],
    pub padding_31: [u8; 3],
    pub word_40: u32,
    pub bytes_64: [u8; 4],
}
#[derive(Debug, PartialEq, Eq)]
pub struct ProgramString {
    pub allocation: ProgramAllocation,
    /// Advertised bytes plus the appended NUL. Before a failed read finishes,
    /// unwritten host bytes are zeroed and have no claimed native value.
    pub bytes: Vec<u8>,
    pub complete: bool,
    /// Destruction preserves pointer observations without retaining live backing.
    pub released: bool,
    /// A failed free has no admitted live/freed result; bytes are only a host
    /// snapshot and the allocation token cannot be submitted again.
    pub disposal_uncertain: bool,
}
impl ProgramString {
    pub fn c_bytes(&self) -> Option<&[u8]> {
        if !self.complete || self.released || self.disposal_uncertain {
            return None;
        }
        Some(c_prefix(&self.bytes))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramPhase {
    Source,
    Interrupted,
    /// A failed disposal may already have freed backing. Only explicit backend
    /// recovery can resolve it; no operation may submit that token again.
    Poisoned,
    Destroying,
    Dead,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramVtable {
    Base,
    Detector,
    Entity,
}
#[derive(Debug)]
pub struct CProgram {
    name: Option<ProgramString>,
    source: Option<ProgramString>,
    owner: Option<ActorReferenceId>,
    seed: ProgramSeed,
    phase: ProgramPhase,
    vtable: ProgramVtable,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramError {
    Unavailable,
    InvalidExtent,
    HostAllocation,
    InvalidAllocation,
    UnsupportedNullPath,
    UnsupportedLocale,
    Effect { stage: &'static str, detail: String },
}
/// These records own raw strings; None is a null pointer, never a wildcard.
/// Table population is a caller-owned dependency, not inferred from content.
#[derive(Debug, PartialEq, Eq)]
pub struct ProgramReplacement {
    pub mission: Option<Vec<u8>>,
    pub name: Option<Vec<u8>>,
    pub frame: Option<Vec<u8>>,
    pub source: Option<Vec<u8>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OwnerFrameName<'a> {
    NoFrame,
    /// A frame with null +0x100 name compares as a nonnull empty string.
    Frame(Option<&'a [u8]>),
}
/// Resolve actual live mission and owner/frame context only when visited by the
/// native replacement loop. Missing context is an error, not a fabricated name.
pub trait ProgramContext {
    /// Admission of the original CRT's default C locale. Other locale tables
    /// require the unreconstructed multibyte/case conversion dependency.
    fn default_c_locale(&self) -> bool;
    fn mission_path(&mut self) -> Result<Option<&[u8]>, String>;
    fn owner_frame_name(
        &mut self,
        owner: Option<ActorReferenceId>,
    ) -> Result<OwnerFrameName<'_>, String>;
}
fn c_prefix(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len())]
}
fn equal(a: &[u8], b: &[u8]) -> bool {
    c_prefix(a).eq_ignore_ascii_case(c_prefix(b))
}
fn optional_equal(a: Option<&[u8]>, b: Option<&[u8]>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => equal(a, b),
        _ => false,
    }
}
/// Original EqualFinalPath 0x4613c0 in the admitted default C locale. Compare
/// only final backslash components; forward slashes are ordinary bytes. Native
/// pointers are required nonnull (an allocated empty string is valid).
pub fn equal_final_path(a: &[u8], b: &[u8]) -> bool {
    fn component(path: &[u8]) -> &[u8] {
        c_prefix(path).rsplit(|b| *b == b'\\').next().unwrap()
    }
    equal(component(a), component(b))
}
impl CProgram {
    /// Source-only constructor: both vectors/register buffers and owner are null;
    /// budget +0x60 is 0.5. Untouched seed bytes are retained verbatim.
    pub fn construct(seed: ProgramSeed) -> Self {
        Self {
            name: None,
            source: None,
            owner: None,
            seed,
            phase: ProgramPhase::Source,
            vtable: ProgramVtable::Base,
        }
    }
    pub fn name(&self) -> Option<&ProgramString> {
        self.name.as_ref()
    }
    pub fn source(&self) -> Option<&ProgramString> {
        self.source.as_ref()
    }
    pub fn owner(&self) -> Option<ActorReferenceId> {
        self.owner
    }
    pub fn seed(&self) -> &ProgramSeed {
        &self.seed
    }
    pub fn phase(&self) -> ProgramPhase {
        self.phase
    }
    pub fn vtable(&self) -> ProgramVtable {
        self.vtable
    }
    pub(crate) fn set_detector_vtable(&mut self) {
        self.vtable = ProgramVtable::Detector;
    }
    pub(crate) fn set_entity_vtable(&mut self) {
        self.vtable = ProgramVtable::Entity;
    }
    /// Used by the evidenced owning actor Init; does not compile or schedule.
    pub fn set_owner(&mut self, owner: Option<ActorReferenceId>) -> Result<(), ProgramError> {
        self.available()?;
        self.owner = owner;
        Ok(())
    }
    fn available(&self) -> Result<(), ProgramError> {
        if self.phase == ProgramPhase::Source {
            Ok(())
        } else {
            Err(ProgramError::Unavailable)
        }
    }
    fn effect(&mut self, stage: &'static str, detail: String) -> ProgramError {
        self.phase = if stage == "free" {
            ProgramPhase::Poisoned
        } else {
            ProgramPhase::Interrupted
        };
        ProgramError::Effect { stage, detail }
    }
    fn clear<B: ProgramBackend + ?Sized>(
        &mut self,
        buffer: ProgramBuffer,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        let allocation = match buffer {
            ProgramBuffer::Name => self.name.as_ref(),
            ProgramBuffer::Source => self.source.as_ref(),
            _ => unreachable!(),
        }
        .map(|s| s.allocation);
        if let Some(allocation) = allocation {
            if let Err(error) = backend.free(self, buffer, Some(allocation)) {
                match buffer {
                    ProgramBuffer::Name => self.name.as_mut(),
                    _ => self.source.as_mut(),
                }
                .unwrap()
                .disposal_uncertain = true;
                return Err(self.effect("free", error));
            }
            match buffer {
                ProgramBuffer::Name => self.name = None,
                _ => self.source = None,
            }
        }
        Ok(())
    }
    fn allocate<B: ProgramBackend + ?Sized>(
        &mut self,
        buffer: ProgramBuffer,
        len: usize,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        // Native len+1 wrapping, NULL allocation and huge extents are outside
        // this memory-safe admission. Prior field/read effects remain applied.
        let size = u32::try_from(len)
            .ok()
            .and_then(|v| v.checked_add(1))
            .filter(|n| *n <= MAX_PROGRAM_BUFFER_BYTES)
            .ok_or(ProgramError::InvalidExtent)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size as usize)
            .map_err(|_| ProgramError::HostAllocation)?;
        bytes.resize(size as usize, 0);
        let allocation = backend
            .allocate(self, buffer, size)
            .map_err(|e| self.effect("allocate", e))?;
        if allocation.0 == 0
            || self
                .name
                .as_ref()
                .is_some_and(|s| s.allocation == allocation)
            || self
                .source
                .as_ref()
                .is_some_and(|s| s.allocation == allocation)
        {
            self.phase = ProgramPhase::Interrupted;
            return Err(ProgramError::InvalidAllocation);
        }
        let value = Some(ProgramString {
            allocation,
            bytes,
            complete: false,
            released: false,
            disposal_uncertain: false,
        });
        match buffer {
            ProgramBuffer::Name => self.name = value,
            _ => self.source = value,
        }
        Ok(())
    }
    /// Free/zero old source first; nonnull empty allocates a single NUL byte.
    /// Input cannot alias this owned buffer through Rust's borrowing boundary.
    pub fn set_source_code<B: ProgramBackend + ?Sized>(
        &mut self,
        source: Option<&[u8]>,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        self.available()?;
        self.clear(ProgramBuffer::Source, backend)?;
        if let Some(source) = source {
            let source = c_prefix(source);
            self.allocate(ProgramBuffer::Source, source.len(), backend)?;
            let target = self.source.as_mut().unwrap();
            target.bytes[..source.len()].copy_from_slice(source);
            target.complete = true;
        }
        Ok(())
    }
    fn read<R: ProgramReader + ?Sized>(
        &mut self,
        reader: &mut R,
        output: &mut [u8],
    ) -> Result<(), ProgramError> {
        reader
            .read_exact(output)
            .map_err(|e| self.effect("read", e))
    }
    fn read_string<R: ProgramReader + ?Sized, B: ProgramBackend + ?Sized>(
        &mut self,
        buffer: ProgramBuffer,
        reader: &mut R,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        let mut length = [0; 4];
        self.read(reader, &mut length)?;
        let length = u32::from_le_bytes(length) as usize;
        if length != 0 {
            self.allocate(buffer, length, backend)?;
            let target = match buffer {
                ProgramBuffer::Name => self.name.as_mut(),
                _ => self.source.as_mut(),
            }
            .unwrap();
            let result = reader.read_exact(&mut target.bytes[..length]);
            if let Err(e) = result {
                return Err(self.effect("read", e));
            }
            let target = match buffer {
                ProgramBuffer::Name => self.name.as_mut(),
                _ => self.source.as_mut(),
            }
            .unwrap();
            target.bytes[length] = 0;
            target.complete = true;
        }
        Ok(())
    }
    /// Incremental LoadSourceCode: disposal precedes version/name/source reads;
    /// the ignored version byte is accepted verbatim. Host failures stop safely
    /// with the applied prefix retained; no native truncated-success claim.
    pub fn load_source_code<
        R: ProgramReader + ?Sized,
        B: ProgramBackend + ?Sized,
        C: ProgramContext + ?Sized,
    >(
        &mut self,
        reader: &mut R,
        replacements: &[ProgramReplacement],
        context: &mut C,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        self.available()?;
        self.clear(ProgramBuffer::Name, backend)?;
        self.clear(ProgramBuffer::Source, backend)?;
        self.read(reader, &mut [0])?;
        self.read_string(ProgramBuffer::Name, reader, backend)?;
        self.read_string(ProgramBuffer::Source, reader, backend)?;
        self.try_change_program_data(replacements, context, backend)
    }
    pub fn try_change_program_data<B: ProgramBackend + ?Sized, C: ProgramContext + ?Sized>(
        &mut self,
        replacements: &[ProgramReplacement],
        context: &mut C,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        self.available()?;
        for replacement in replacements.iter().rev() {
            if !context.default_c_locale() {
                return Err(ProgramError::UnsupportedLocale);
            }
            let path = context
                .mission_path()
                .map_err(|e| self.effect("mission_context", e))?;
            let path = path.ok_or(ProgramError::UnsupportedNullPath)?;
            let mission = replacement
                .mission
                .as_deref()
                .ok_or(ProgramError::UnsupportedNullPath)?;
            if !equal_final_path(mission, path) {
                continue;
            }
            let name_matches = optional_equal(
                self.name.as_ref().and_then(ProgramString::c_bytes),
                replacement.name.as_deref(),
            );
            // Native still reads frame context when the program-name comparison
            // fails; preserving that access matters for interrupted effects.
            let frame = context
                .owner_frame_name(self.owner)
                .map_err(|e| self.effect("frame_context", e))?;
            let frame_matches = match frame {
                OwnerFrameName::NoFrame => replacement.frame.is_none(),
                OwnerFrameName::Frame(name) => replacement
                    .frame
                    .as_deref()
                    .is_some_and(|r| equal(name.unwrap_or(b""), r)),
            };
            if name_matches && frame_matches {
                return self.set_source_code(replacement.source.as_deref(), backend);
            }
        }
        Ok(())
    }
    /// Nondeleting destructor, not Done: conditional name/source frees retain
    /// their pointer observations; null return/instruction backings are freed in
    /// order. Only constructor-owned empty vectors are admitted by this type.
    pub fn destruct<B: ProgramBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), ProgramError> {
        if !matches!(self.phase, ProgramPhase::Source | ProgramPhase::Interrupted) {
            return Err(ProgramError::Unavailable);
        }
        self.phase = ProgramPhase::Destroying;
        self.vtable = ProgramVtable::Base;
        for buffer in [ProgramBuffer::Name, ProgramBuffer::Source] {
            let string = match buffer {
                ProgramBuffer::Name => &self.name,
                _ => &self.source,
            };
            if let Some(string) = string {
                if let Err(detail) = backend.free(self, buffer, Some(string.allocation)) {
                    match buffer {
                        ProgramBuffer::Name => self.name.as_mut(),
                        _ => self.source.as_mut(),
                    }
                    .unwrap()
                    .disposal_uncertain = true;
                    return Err(ProgramError::Effect {
                        stage: "destruct_free",
                        detail,
                    });
                }
                match buffer {
                    ProgramBuffer::Name => self.name.as_mut(),
                    _ => self.source.as_mut(),
                }
                .unwrap()
                .released = true;
            }
        }
        for buffer in [ProgramBuffer::ReturnStack, ProgramBuffer::Instructions] {
            backend
                .free(self, buffer, None)
                .map_err(|e| ProgramError::Effect {
                    stage: "destruct_free",
                    detail: e,
                })?;
        }
        self.phase = ProgramPhase::Dead;
        Ok(())
    }
}
