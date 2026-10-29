//! Bounded original car parameter-source intake and its four-byte COMP codec.
//! Buffers are serialized records, not another car/vehicle object. The caller
//! supplies actual conversion/application dependencies and explicit allocation
//! boundaries. No mission initialization or physics fallback is provided.
#![forbid(unsafe_code)]

#[derive(Debug, PartialEq, Eq)]
pub struct CarDataBuffer {
    bytes: Vec<u8>,
    initialized: Vec<bool>,
}
impl CarDataBuffer {
    /// Host storage only: the fill bytes are not initialized original values.
    pub fn uninitialized(size: usize) -> Self {
        Self {
            bytes: vec![0; size],
            initialized: vec![false; size],
        }
    }
    pub fn from_serialized(bytes: Vec<u8>) -> Self {
        Self {
            initialized: vec![true; bytes.len()],
            bytes,
        }
    }
    pub fn len(&self) -> usize {
        self.bytes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub fn read(&self, offset: usize, width: usize) -> Option<&[u8]> {
        let end = offset.checked_add(width)?;
        let initialized = self.initialized.get(offset..end)?;
        if !initialized.iter().all(|v| *v) {
            return None;
        }
        self.bytes.get(offset..end)
    }
    /// Opaque binary transport only: the byte and initializedness travel
    /// together. When false, the storage byte is a host placeholder and must
    /// never be interpreted as an original scalar value. `read` stays strict.
    pub(crate) fn opaque_byte(&self, offset: usize) -> Option<(u8, bool)> {
        Some((*self.bytes.get(offset)?, *self.initialized.get(offset)?))
    }
    /// One reached store, checked in full before application.
    pub fn write(&mut self, offset: usize, bytes: &[u8]) -> bool {
        let Some(end) = offset.checked_add(bytes.len()) else {
            return false;
        };
        let Some(target) = self.bytes.get_mut(offset..end) else {
            return false;
        };
        target.copy_from_slice(bytes);
        self.initialized[offset..end].fill(true);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarDataAllocation {
    Compressed,
    Decoded,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputPublication {
    Unchanged,
    Null,
    Allocated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompPublication {
    pub size: Option<u32>,
    pub output: OutputPublication,
}
impl Default for CompPublication {
    fn default() -> Self {
        Self {
            size: None,
            output: OutputPublication::Unchanged,
        }
    }
}

pub trait CompAllocationBackend {
    type Error;
    /// Called only after the original size publication and header reads. A
    /// backend host limit belongs here; returning None is native allocation null.
    fn allocate_decoded(
        &mut self,
        size: u32,
        publication: CompPublication,
    ) -> Result<Option<CarDataBuffer>, Self::Error>;
}
#[derive(Debug, PartialEq, Eq)]
pub enum CompDecodeError<E> {
    AlreadyStarted,
    InputUnavailable { offset: u32, width: u32 },
    OutputUnavailable { offset: u32, width: u32 },
    Allocation(E),
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CompDecoder {
    started: bool,
    publication: CompPublication,
    output: Option<CarDataBuffer>,
    input_position: u32,
    output_position: u32,
}
impl CompDecoder {
    pub fn publication(&self) -> CompPublication {
        self.publication
    }
    pub fn output(&self) -> Option<&CarDataBuffer> {
        self.output.as_ref()
    }
    pub fn input_position(&self) -> u32 {
        self.input_position
    }
    pub fn output_position(&self) -> u32 {
        self.output_position
    }
    fn read<E>(
        source: &CarDataBuffer,
        offset: u32,
        width: u32,
    ) -> Result<&[u8], CompDecodeError<E>> {
        source
            .read(offset as usize, width as usize)
            .ok_or(CompDecodeError::InputUnavailable { offset, width })
    }
    fn word<E>(source: &CarDataBuffer, offset: u32) -> Result<u32, CompDecodeError<E>> {
        Ok(u32::from_le_bytes(
            Self::read(source, offset, 4)?.try_into().unwrap(),
        ))
    }
    fn store<E>(&mut self, value: u8, width: u32) -> Result<(), CompDecodeError<E>> {
        let offset = self.output_position;
        if !self
            .output
            .as_mut()
            .unwrap()
            .write(offset as usize, &[value; 4][..width as usize])
        {
            return Err(CompDecodeError::OutputUnavailable { offset, width });
        }
        self.output_position = self.output_position.wrapping_add(width);
        Ok(())
    }
    /// Original decode arm. `supplied_length` is deliberately not a preflight:
    /// the original uses its header's encoded end, independently of that input.
    /// Immutable source and distinct destination are this adapter's domain.
    pub fn decode<B: CompAllocationBackend>(
        &mut self,
        source: &CarDataBuffer,
        _supplied_length: u32,
        backend: &mut B,
    ) -> Result<bool, CompDecodeError<B::Error>> {
        if self.started {
            return Err(CompDecodeError::AlreadyStarted);
        }
        self.started = true;
        if Self::word(source, 0)? != 0x504d_4f43 {
            self.publication.size = Some(0);
            self.publication.output = OutputPublication::Null;
            return Ok(false);
        }
        let size = Self::word(source, 4)?;
        self.publication.size = Some(size);
        let end = Self::word(source, 8)?;
        let marker = Self::read(source, 12, 1)?[0];
        self.output = backend
            .allocate_decoded(size, self.publication)
            .map_err(CompDecodeError::Allocation)?;
        if self.output.is_none() {
            self.publication.size = Some(0);
            self.publication.output = OutputPublication::Null;
            return Ok(false);
        }
        self.publication.output = OutputPublication::Allocated;
        self.input_position = 13;
        while self.input_position < end {
            let first = Self::read(source, self.input_position, 1)?[0];
            if first == marker {
                let value = Self::read(source, self.input_position.wrapping_add(1), 1)?[0];
                // The native increments its input cursor after the value load,
                // before the separate count load.
                self.input_position = self.input_position.wrapping_add(2);
                let count = Self::read(source, self.input_position, 1)?[0];
                self.input_position = self.input_position.wrapping_add(1);
                for _ in 0..count / 4 {
                    self.store(value, 4)?;
                }
                for _ in 0..count % 4 {
                    self.store(value, 1)?;
                }
            } else {
                self.store(first, 1)?;
                self.input_position = self.input_position.wrapping_add(1);
            }
        }
        // Native accepts untouched tails and does not compare written vs size.
        Ok(true)
    }
}

/// Whole-field bounded C_chunk source domain. A skipped read changes neither
/// destination nor cursor. Physical partial-IO writes are outside this trait.
pub trait CarSourceReader {
    fn read_bounded(&mut self, destination: &mut [u8]) -> bool;
}
pub trait CarSourceBackend: CompAllocationBackend {
    fn allocate_compressed(&mut self, size: u32) -> Result<Option<CarDataBuffer>, Self::Error>;
    /// Err may follow an external free; the owner will never retry that effect.
    fn free_buffer(
        &mut self,
        kind: CarDataAllocation,
        buffer: &CarDataBuffer,
    ) -> Result<(), Self::Error>;
    /// Actual existing packed-record conversion, including its car-level stores.
    /// The destination is the caller's live record; preserve unwritten padding.
    fn convert_parameters(
        &mut self,
        packed: &CarDataBuffer,
        parameters: &mut CarDataBuffer,
    ) -> Result<(), Self::Error>;
    /// Actual existing Vehicle InitCar application with mode zero. Its bool is
    /// ignored by original outer LoadData, but unavailable effects propagate.
    fn apply_parameters(&mut self, parameters: &mut CarDataBuffer) -> Result<bool, Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarSourceStage {
    Fresh,
    Reading,
    Decoding,
    FreeCompressed,
    Converting,
    FreeDecoded,
    Applying,
    Complete,
    Rejected,
    Interrupted,
}
#[derive(Debug, PartialEq, Eq)]
pub enum CarSourceError<E> {
    AlreadyStarted,
    MissingVersion,
    AllocationStorage,
    Effect(E),
    Decode(CompDecodeError<E>),
}
#[derive(Debug, PartialEq, Eq)]
pub struct CarSourceLoad {
    stage: CarSourceStage,
    compressed: Option<CarDataBuffer>,
    decoder: CompDecoder,
    parameters: CarDataBuffer,
    interrupted_at: Option<CarSourceStage>,
}
impl CarSourceLoad {
    /// Caller storage belongs to this binary-boundary owner, never a second car.
    /// Uninitialized padding is legal and must remain unavailable if read later.
    pub fn new(parameters: CarDataBuffer) -> Self {
        Self {
            stage: CarSourceStage::Fresh,
            compressed: None,
            decoder: CompDecoder::default(),
            parameters,
            interrupted_at: None,
        }
    }
    pub fn stage(&self) -> CarSourceStage {
        self.stage
    }
    pub fn compressed(&self) -> Option<&CarDataBuffer> {
        self.compressed.as_ref()
    }
    pub fn decoder(&self) -> &CompDecoder {
        &self.decoder
    }
    pub fn parameters(&self) -> &CarDataBuffer {
        &self.parameters
    }
    pub fn interrupted_at(&self) -> Option<CarSourceStage> {
        self.interrupted_at
    }
    pub fn load<R: CarSourceReader, B: CarSourceBackend>(
        &mut self,
        reader: &mut R,
        backend: &mut B,
    ) -> Result<bool, CarSourceError<B::Error>> {
        if self.stage != CarSourceStage::Fresh {
            return Err(CarSourceError::AlreadyStarted);
        }
        self.stage = CarSourceStage::Reading;
        let result = self.load_once(reader, backend);
        if result.is_err() {
            self.interrupted_at = Some(self.stage);
            self.stage = CarSourceStage::Interrupted;
        }
        result
    }
    fn load_once<R: CarSourceReader, B: CarSourceBackend>(
        &mut self,
        reader: &mut R,
        backend: &mut B,
    ) -> Result<bool, CarSourceError<B::Error>> {
        let mut word = [0; 4];
        if !reader.read_bounded(&mut word) {
            return Err(CarSourceError::MissingVersion);
        }
        if u32::from_le_bytes(word) != 1 {
            self.stage = CarSourceStage::Rejected;
            return Ok(false);
        }
        // The native initializes this size local to zero before its guarded read.
        word = [0; 4];
        reader.read_bounded(&mut word);
        let size = u32::from_le_bytes(word);
        self.compressed = backend
            .allocate_compressed(size)
            .map_err(CarSourceError::Effect)?;
        let Some(compressed) = &mut self.compressed else {
            self.stage = CarSourceStage::Rejected;
            return Ok(false);
        };
        if compressed.len() != size as usize {
            return Err(CarSourceError::AllocationStorage);
        }
        // Allocation bytes begin unknown. Whole-field read cannot expose host fill.
        if reader.read_bounded(&mut compressed.bytes) {
            compressed.initialized.fill(true);
        }
        self.stage = CarSourceStage::Decoding;
        let accepted = self
            .decoder
            .decode(compressed, size, backend)
            .map_err(CarSourceError::Decode)?;
        self.stage = CarSourceStage::FreeCompressed;
        backend
            .free_buffer(
                CarDataAllocation::Compressed,
                self.compressed.as_ref().unwrap(),
            )
            .map_err(CarSourceError::Effect)?;
        self.compressed = None;
        if !accepted {
            self.stage = CarSourceStage::Rejected;
            return Ok(false);
        }
        self.stage = CarSourceStage::Converting;
        backend
            .convert_parameters(self.decoder.output.as_ref().unwrap(), &mut self.parameters)
            .map_err(CarSourceError::Effect)?;
        self.stage = CarSourceStage::FreeDecoded;
        backend
            .free_buffer(
                CarDataAllocation::Decoded,
                self.decoder.output.as_ref().unwrap(),
            )
            .map_err(CarSourceError::Effect)?;
        self.decoder.output = None;
        self.stage = CarSourceStage::Applying;
        let _ignored_boolean = backend
            .apply_parameters(&mut self.parameters)
            .map_err(CarSourceError::Effect)?;
        self.stage = CarSourceStage::Complete;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct Backend {
        events: Vec<&'static str>,
        null: bool,
        fail: Option<usize>,
        allocation_size: Option<u32>,
        seen_publication: Option<CompPublication>,
    }
    impl Backend {
        fn event(&mut self, event: &'static str) -> Result<(), &'static str> {
            self.events.push(event);
            if self.fail == Some(self.events.len() - 1) {
                Err(event)
            } else {
                Ok(())
            }
        }
    }
    impl CompAllocationBackend for Backend {
        type Error = &'static str;
        fn allocate_decoded(
            &mut self,
            size: u32,
            publication: CompPublication,
        ) -> Result<Option<CarDataBuffer>, Self::Error> {
            self.seen_publication = Some(publication);
            self.allocation_size = Some(size);
            self.event("allocate_decoded")?;
            Ok((!self.null).then(|| CarDataBuffer::uninitialized(size as usize)))
        }
    }
    impl CarSourceBackend for Backend {
        fn allocate_compressed(&mut self, size: u32) -> Result<Option<CarDataBuffer>, Self::Error> {
            self.allocation_size = Some(size);
            self.event("allocate_compressed")?;
            Ok((!self.null).then(|| CarDataBuffer::uninitialized(size as usize)))
        }
        fn free_buffer(
            &mut self,
            kind: CarDataAllocation,
            _: &CarDataBuffer,
        ) -> Result<(), Self::Error> {
            self.event(match kind {
                CarDataAllocation::Compressed => "free_compressed",
                CarDataAllocation::Decoded => "free_decoded",
            })
        }
        fn convert_parameters(
            &mut self,
            packed: &CarDataBuffer,
            parameters: &mut CarDataBuffer,
        ) -> Result<(), Self::Error> {
            self.event("convert")?;
            let value = packed.read(0, 1).ok_or("unknown packed byte")?;
            assert!(parameters.write(0, value));
            Ok(())
        }
        fn apply_parameters(
            &mut self,
            parameters: &mut CarDataBuffer,
        ) -> Result<bool, Self::Error> {
            assert_eq!(parameters.read(0, 1), Some(b"a".as_slice()));
            self.event("apply")?;
            Ok(false)
        }
    }
    fn record(size: u32, marker: u8, payload: &[u8]) -> CarDataBuffer {
        let mut bytes = Vec::from(*b"COMP");
        bytes.extend_from_slice(&size.to_le_bytes());
        bytes.extend_from_slice(&(13 + payload.len() as u32).to_le_bytes());
        bytes.push(marker);
        bytes.extend_from_slice(payload);
        CarDataBuffer::from_serialized(bytes)
    }
    #[test]
    fn opaque_transport_pairs_unknown_storage_with_validity_without_admitting_scalar_reads() {
        let mut buffer = CarDataBuffer::uninitialized(2);
        let (_, initialized) = buffer.opaque_byte(0).unwrap();
        assert!(!initialized);
        assert_eq!(buffer.read(0, 1), None);
        assert!(buffer.write(1, &[0x55]));
        assert_eq!(buffer.opaque_byte(1), Some((0x55, true)));
        assert_eq!(buffer.opaque_byte(2), None);
        assert_eq!(buffer.read(1, 1), Some([0x55].as_slice()));
        assert_eq!(buffer.read(0, 2), None);
    }
    #[test]
    fn all_sentinels_and_run_lengths_match_constructed_expected_bytes() {
        for sentinel in 0..=255u8 {
            for count in 0..=255u8 {
                let source = record(257, sentinel, &[sentinel, sentinel ^ 0x80, count]);
                let mut decoder = CompDecoder::default();
                let mut backend = Backend::default();
                assert_eq!(decoder.decode(&source, 0, &mut backend), Ok(true));
                assert_eq!(decoder.output_position(), u32::from(count));
                assert_eq!(
                    decoder.output().unwrap().read(0, count as usize),
                    Some(vec![sentinel ^ 0x80; count as usize].as_slice())
                );
                assert_eq!(decoder.output().unwrap().read(count as usize, 1), None);
                assert_eq!(
                    backend.seen_publication,
                    Some(CompPublication {
                        size: Some(257),
                        output: OutputPublication::Unchanged
                    })
                );
            }
        }
    }
    #[test]
    fn four_byte_magic_and_header_end_control_decode_not_passed_length() {
        let source = record(4, 0xff, b"abc");
        let mut decoder = CompDecoder::default();
        assert_eq!(
            decoder.decode(&source, 0, &mut Backend::default()),
            Ok(true)
        );
        assert_eq!(
            decoder.output().unwrap().read(0, 3),
            Some(b"abc".as_slice())
        );
        assert_eq!(decoder.output().unwrap().read(3, 1), None);
        assert_ne!(source.read(4, 1), Some(b"!".as_slice()));
    }
    #[test]
    fn bad_magic_and_null_allocation_reset_publications() {
        let mut bad = record(8, 0xff, b"a");
        assert!(bad.write(0, b"FAIL"));
        let mut backend = Backend::default();
        let mut decoder = CompDecoder::default();
        assert_eq!(decoder.decode(&bad, 100, &mut backend), Ok(false));
        assert!(backend.events.is_empty());
        assert_eq!(
            decoder.publication(),
            CompPublication {
                size: Some(0),
                output: OutputPublication::Null
            }
        );
        let mut decoder = CompDecoder::default();
        backend.null = true;
        assert_eq!(
            decoder.decode(&record(8, 0xff, b"a"), 0, &mut backend),
            Ok(false)
        );
        assert_eq!(backend.seen_publication.unwrap().size, Some(8));
        assert_eq!(
            decoder.publication(),
            CompPublication {
                size: Some(0),
                output: OutputPublication::Null
            }
        );
    }
    #[test]
    fn header_input_failure_preserves_reached_size_and_does_not_allocate() {
        let source =
            CarDataBuffer::from_serialized([b"COMP".as_slice(), &7u32.to_le_bytes()].concat());
        let mut decoder = CompDecoder::default();
        let mut backend = Backend::default();
        assert_eq!(
            decoder.decode(&source, 8, &mut backend),
            Err(CompDecodeError::InputUnavailable {
                offset: 8,
                width: 4
            })
        );
        assert_eq!(decoder.publication().size, Some(7));
        assert!(backend.events.is_empty());
    }
    #[test]
    fn short_marker_count_retains_allocation_and_native_input_increment_prefix() {
        let mut source = record(8, 0xfe, &[0xfe, 3]);
        assert!(source.write(8, &16u32.to_le_bytes()));
        let mut decoder = CompDecoder::default();
        assert_eq!(
            decoder.decode(&source, 15, &mut Backend::default()),
            Err(CompDecodeError::InputUnavailable {
                offset: 15,
                width: 1
            })
        );
        assert_eq!(decoder.input_position(), 15);
        assert_eq!(decoder.output_position(), 0);
        assert_eq!(decoder.publication().output, OutputPublication::Allocated);
        assert_eq!(decoder.output().unwrap().read(0, 1), None);
    }
    #[test]
    fn output_failure_keeps_complete_native_dword_stores_and_blocks_retry() {
        let source = record(6, 0xfe, &[0xfe, 7, 8]);
        let mut decoder = CompDecoder::default();
        let mut backend = Backend::default();
        assert_eq!(
            decoder.decode(&source, 16, &mut backend),
            Err(CompDecodeError::OutputUnavailable {
                offset: 4,
                width: 4
            })
        );
        assert_eq!(
            decoder.output().unwrap().read(0, 4),
            Some([7; 4].as_slice())
        );
        assert_eq!(decoder.output().unwrap().read(4, 1), None);
        let events = backend.events.clone();
        assert_eq!(
            decoder.decode(&source, 16, &mut backend),
            Err(CompDecodeError::AlreadyStarted)
        );
        assert_eq!(backend.events, events);
    }
    #[test]
    fn header_end_before_tokens_succeeds_with_unknown_output_tail() {
        let mut source = record(2, 0xff, b"");
        assert!(source.write(8, &12u32.to_le_bytes()));
        let mut decoder = CompDecoder::default();
        assert_eq!(
            decoder.decode(&source, 0, &mut Backend::default()),
            Ok(true)
        );
        assert_eq!(decoder.output().unwrap().read(0, 1), None);
    }
    struct Reader {
        bytes: Vec<u8>,
        cursor: usize,
    }
    impl CarSourceReader for Reader {
        fn read_bounded(&mut self, target: &mut [u8]) -> bool {
            let Some(bytes) = self.bytes.get(self.cursor..self.cursor + target.len()) else {
                return false;
            };
            target.copy_from_slice(bytes);
            self.cursor += target.len();
            true
        }
    }
    fn reader() -> Reader {
        let compressed = record(1, 0xff, b"a");
        Reader {
            bytes: [
                1u32.to_le_bytes().as_slice(),
                &(compressed.len() as u32).to_le_bytes(),
                compressed.read(0, compressed.len()).unwrap(),
            ]
            .concat(),
            cursor: 0,
        }
    }
    #[test]
    fn outer_order_reuses_live_parameter_record_and_ignores_apply_false() {
        let mut source = reader();
        let mut backend = Backend::default();
        let mut owner = CarSourceLoad::new(CarDataBuffer::uninitialized(4388));
        assert_eq!(owner.load(&mut source, &mut backend), Ok(true));
        assert_eq!(source.cursor, source.bytes.len());
        assert_eq!(
            backend.events,
            [
                "allocate_compressed",
                "allocate_decoded",
                "free_compressed",
                "convert",
                "free_decoded",
                "apply"
            ]
        );
        assert_eq!(owner.stage(), CarSourceStage::Complete);
        assert!(owner.compressed().is_none());
        assert!(owner.decoder().output().is_none());
        assert_eq!(owner.parameters().read(1, 1), None);
    }
    #[test]
    fn every_outer_effect_failure_retains_prefix_and_never_retries() {
        let expected = [
            "allocate_compressed",
            "allocate_decoded",
            "free_compressed",
            "convert",
            "free_decoded",
            "apply",
        ];
        let phases = [
            CarSourceStage::Reading,
            CarSourceStage::Decoding,
            CarSourceStage::FreeCompressed,
            CarSourceStage::Converting,
            CarSourceStage::FreeDecoded,
            CarSourceStage::Applying,
        ];
        for failure in 0..expected.len() {
            let mut owner = CarSourceLoad::new(CarDataBuffer::uninitialized(4388));
            let mut backend = Backend {
                fail: Some(failure),
                ..Default::default()
            };
            let mut source = reader();
            assert!(owner.load(&mut source, &mut backend).is_err());
            assert_eq!(backend.events, expected[..=failure]);
            assert_eq!(owner.interrupted_at(), Some(phases[failure]));
            assert_eq!(owner.stage(), CarSourceStage::Interrupted);
            assert_eq!(owner.compressed().is_some(), (1..=2).contains(&failure));
            assert_eq!(
                owner.decoder().output().is_some(),
                (2..=4).contains(&failure)
            );
            assert_eq!(
                owner.load(&mut source, &mut backend),
                Err(CarSourceError::AlreadyStarted)
            );
            assert_eq!(backend.events, expected[..=failure]);
        }
    }
    #[test]
    fn skipped_size_keeps_native_zero_local_then_reaches_allocation() {
        let mut source = Reader {
            bytes: 1u32.to_le_bytes().to_vec(),
            cursor: 0,
        };
        let mut backend = Backend::default();
        let mut owner = CarSourceLoad::new(CarDataBuffer::uninitialized(4388));
        assert!(matches!(
            owner.load(&mut source, &mut backend),
            Err(CarSourceError::Decode(CompDecodeError::InputUnavailable {
                offset: 0,
                width: 4
            }))
        ));
        assert_eq!(backend.allocation_size, Some(0));
        assert_eq!(backend.events, ["allocate_compressed"]);
        assert_eq!(source.cursor, 4);
        assert!(owner.compressed().is_some());
    }
    #[test]
    fn outer_rejection_and_skipped_compressed_read_do_not_invent_input() {
        let mut source = Reader {
            bytes: 2u32.to_le_bytes().to_vec(),
            cursor: 0,
        };
        let mut backend = Backend::default();
        let mut owner = CarSourceLoad::new(CarDataBuffer::uninitialized(4388));
        assert_eq!(owner.load(&mut source, &mut backend), Ok(false));
        assert_eq!(source.cursor, 4);
        assert!(backend.events.is_empty());
        let mut source = reader();
        source.bytes.pop();
        let mut owner = CarSourceLoad::new(CarDataBuffer::uninitialized(4388));
        assert!(matches!(
            owner.load(&mut source, &mut backend),
            Err(CarSourceError::Decode(_))
        ));
        assert_eq!(source.cursor, 8);
        assert_eq!(backend.events, ["allocate_compressed"]);
        assert_eq!(owner.compressed().unwrap().read(0, 1), None);
    }
}
