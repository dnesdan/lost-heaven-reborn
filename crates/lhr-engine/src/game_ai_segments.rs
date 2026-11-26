//! Mission AI check-segment storage and ordered `clusters.seg` loading.
//! Payload arrays retain opaque file records, not simulated native objects.
//! Spatial queries and interpretation of unknown payload fields are not implemented.

#![forbid(unsafe_code)]

const HEADER: &[u8; 16] = b"AISEG-V1.1-JIPO\0";

pub trait SegmentsIo {
    type File;
    type Error;

    /// Returns `None` only if lookup found no source.
    fn open(&mut self, path: &[u8], archive: bool) -> Result<Option<Self::File>, Self::Error>;
    fn read(&mut self, file: &mut Self::File, output: &mut [u8]) -> Result<usize, Self::Error>;
    fn close(&mut self, file: Self::File) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentsLoadStage {
    MissionPath,
    Initialize,
    Open,
    Header,
    RecordCount,
    Flags { segment: u32 },
    UnknownWord { segment: u32 },
    Position { segment: u32 },
    IndexCount { segment: u32 },
    Index { segment: u32, index: u32 },
    Data { segment: u32 },
    Specials { segment: u32 },
    Special { segment: u32, index: u32 },
    AddSegment { segment: u32 },
    Close,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SegmentsLoadState {
    #[default]
    Cleared,
    Initialized,
    Reading(SegmentsLoadStage),
    MissingSource,
    RejectedHeader,
    Loaded,
    Interrupted(SegmentsLoadStage),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SegmentsUnsupportedDependency {
    MissionPathBuffer { required: usize },
    ShortNativeRead { requested: usize, received: usize },
    InvalidReadCount { requested: usize, received: usize },
    SegmentDataType { kind: u32 },
    SpecialDataType { kind: u32 },
}

#[derive(Debug, Eq, PartialEq)]
pub enum SegmentsLoadFailure<E> {
    Io(E),
    Unsupported(SegmentsUnsupportedDependency),
    Allocation,
    RequiresClear,
}

#[derive(Debug, Eq, PartialEq)]
pub struct SegmentsLoadError<E> {
    pub stage: SegmentsLoadStage,
    pub failure: SegmentsLoadFailure<E>,
    pub close_error: Option<E>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CheckSegData {
    OpaqueType1([u8; 0x28]),
    OpaqueType2([u8; 0x34]),
    OpaqueType4([u8; 0x10]),
}

impl CheckSegData {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::OpaqueType1(bytes) => bytes,
            Self::OpaqueType2(bytes) => bytes,
            Self::OpaqueType4(bytes) => bytes,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct AiSegCorner {
    record: [u8; 0x38],
}

impl AiSegCorner {
    pub fn bytes(&self) -> &[u8; 0x38] {
        &self.record
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum CheckSegSpecial {
    OpaqueType1([u8; 0x38]),
    Corner(AiSegCorner),
    OpaqueType4([u8; 0x18]),
    OpaqueType8([u8; 0x2c]),
}

impl CheckSegSpecial {
    pub fn bytes(&self) -> &[u8] {
        match self {
            Self::OpaqueType1(bytes) => bytes,
            Self::Corner(corner) => corner.bytes(),
            Self::OpaqueType4(bytes) => bytes,
            Self::OpaqueType8(bytes) => bytes,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct CheckSeg {
    flags: u32,
    unknown_word: Option<u32>,
    position_bits: Option<[u32; 3]>,
    declared_index_count: Option<u32>,
    indices: Vec<u32>,
    data: Option<CheckSegData>,
    specials: Vec<CheckSegSpecial>,
}

impl CheckSeg {
    fn pending(flags: u32) -> Self {
        Self {
            flags,
            unknown_word: None,
            position_bits: None,
            declared_index_count: None,
            indices: Vec::new(),
            data: None,
            specials: Vec::new(),
        }
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn get_type(&self) -> u32 {
        self.flags & 0xf
    }

    pub fn num_specials(&self) -> u32 {
        self.flags >> 28
    }

    pub fn unknown_word(&self) -> Option<u32> {
        self.unknown_word
    }

    pub fn position_bits(&self) -> Option<[u32; 3]> {
        self.position_bits
    }

    pub fn declared_index_count(&self) -> Option<u32> {
        self.declared_index_count
    }

    pub fn indices(&self) -> &[u32] {
        &self.indices
    }

    pub fn data(&self) -> Option<&CheckSegData> {
        self.data.as_ref()
    }

    pub fn specials(&self) -> &[CheckSegSpecial] {
        &self.specials
    }

    pub fn get_special(&self, index: u32) -> Option<(u32, &CheckSegSpecial)> {
        let special = self.specials.get(index as usize)?;
        Some((self.special_type(index), special))
    }

    fn special_type(&self, index: u32) -> u32 {
        self.flags
            .wrapping_shr(index.wrapping_mul(4).wrapping_add(4))
            & 0xf
    }

    fn clear(&mut self) {
        self.data = None;
        self.specials.clear();
        self.flags = 0;
        self.indices.clear();
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct CheckSegCluster {
    segments: [Option<CheckSeg>; 4],
}

impl CheckSegCluster {
    pub fn segments(&self) -> &[Option<CheckSeg>; 4] {
        &self.segments
    }
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct CheckSegClusters {
    clusters: Vec<CheckSegCluster>,
    pending: Option<CheckSeg>,
    declared_segment_count: Option<u32>,
    state: SegmentsLoadState,
}

impl CheckSegClusters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> SegmentsLoadState {
        self.state
    }

    pub fn clusters(&self) -> &[CheckSegCluster] {
        &self.clusters
    }

    pub fn pending_segment(&self) -> Option<&CheckSeg> {
        self.pending.as_ref()
    }

    pub fn declared_segment_count(&self) -> Option<u32> {
        self.declared_segment_count
    }

    pub fn num_segments(&self) -> u32 {
        let Some(last) = self.clusters.last() else {
            return 0;
        };
        let previous = (self.clusters.len() as u32).wrapping_mul(4).wrapping_sub(4);
        last.segments
            .iter()
            .flatten()
            .fold(previous, |count, segment| {
                count.wrapping_add(u32::from(segment.flags != 0))
            })
    }

    pub fn get_segment(&self, index: u32) -> Option<&CheckSeg> {
        let cluster = self.clusters.get((index >> 2) as usize)?;
        cluster.segments[(index & 3) as usize]
            .as_ref()
            .filter(|segment| segment.flags != 0)
    }

    pub fn clear(&mut self) {
        for cluster in &mut self.clusters {
            for segment in cluster.segments.iter_mut().flatten() {
                segment.clear();
            }
        }
        self.clusters = Vec::new();
        if let Some(segment) = &mut self.pending {
            segment.clear();
        }
        self.pending = None;
        self.declared_segment_count = None;
        self.state = SegmentsLoadState::Cleared;
    }

    /// An absent source or unrecognized header returns false without records.
    /// Each read is issued once. Interrupted calls require explicit `clear`.
    /// An opened handle is consumed by one close attempt, also on failure.
    pub fn load<I: SegmentsIo>(
        &mut self,
        mission_name: &[u8],
        io: &mut I,
    ) -> Result<bool, SegmentsLoadError<I::Error>> {
        match self.state {
            SegmentsLoadState::Reading(stage) | SegmentsLoadState::Interrupted(stage) => {
                return Err(SegmentsLoadError {
                    stage,
                    failure: SegmentsLoadFailure::RequiresClear,
                    close_error: None,
                });
            }
            _ => {}
        }
        let name_end = mission_name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(mission_name.len());
        let name = &mission_name[..name_end];
        let required = b"Missions\\".len() + name.len() + b"\\clusters.seg".len() + 1;
        if required > 64 {
            return Err(SegmentsLoadError {
                stage: SegmentsLoadStage::MissionPath,
                failure: SegmentsLoadFailure::Unsupported(
                    SegmentsUnsupportedDependency::MissionPathBuffer { required },
                ),
                close_error: None,
            });
        }
        let mut path = [0; 64];
        let prefix_end = b"Missions\\".len();
        path[..prefix_end].copy_from_slice(b"Missions\\");
        let name_end = prefix_end + name.len();
        path[prefix_end..name_end].copy_from_slice(name);
        path[name_end..required - 1].copy_from_slice(b"\\clusters.seg");

        self.clear();
        self.state = SegmentsLoadState::Reading(SegmentsLoadStage::Initialize);
        if self.clusters.try_reserve_exact(1).is_err() {
            self.state = SegmentsLoadState::Interrupted(SegmentsLoadStage::Initialize);
            return Err(SegmentsLoadError {
                stage: SegmentsLoadStage::Initialize,
                failure: SegmentsLoadFailure::Allocation,
                close_error: None,
            });
        }
        self.clusters.push(CheckSegCluster::default());
        self.state = SegmentsLoadState::Initialized;
        self.state = SegmentsLoadState::Reading(SegmentsLoadStage::Open);
        let mut file = match io.open(&path[..required - 1], true) {
            Ok(Some(file)) => file,
            Ok(None) => {
                self.state = SegmentsLoadState::MissingSource;
                return Ok(false);
            }
            Err(error) => {
                self.state = SegmentsLoadState::Interrupted(SegmentsLoadStage::Open);
                return Err(SegmentsLoadError {
                    stage: SegmentsLoadStage::Open,
                    failure: SegmentsLoadFailure::Io(error),
                    close_error: None,
                });
            }
        };
        let loaded = match self.read_file(&mut file, io) {
            Ok(loaded) => loaded,
            Err(mut error) => {
                self.state = SegmentsLoadState::Interrupted(error.stage);
                error.close_error = io.close(file).err();
                return Err(error);
            }
        };
        self.state = SegmentsLoadState::Reading(SegmentsLoadStage::Close);
        if let Err(error) = io.close(file) {
            self.state = SegmentsLoadState::Interrupted(SegmentsLoadStage::Close);
            return Err(SegmentsLoadError {
                stage: SegmentsLoadStage::Close,
                failure: SegmentsLoadFailure::Io(error),
                close_error: None,
            });
        }
        self.state = if loaded {
            SegmentsLoadState::Loaded
        } else {
            SegmentsLoadState::RejectedHeader
        };
        Ok(loaded)
    }

    fn read_file<I: SegmentsIo>(
        &mut self,
        file: &mut I::File,
        io: &mut I,
    ) -> Result<bool, SegmentsLoadError<I::Error>> {
        let header = self.read::<I, 16>(file, io, SegmentsLoadStage::Header)?;
        if !header.eq_ignore_ascii_case(HEADER) {
            return Ok(false);
        }
        let count = self.read_word(file, io, SegmentsLoadStage::RecordCount)?;
        self.declared_segment_count = Some(count);
        for segment in 0..count {
            let flags = self.read_word(file, io, SegmentsLoadStage::Flags { segment })?;
            self.pending = Some(CheckSeg::pending(flags));
            let word = self.read_word(file, io, SegmentsLoadStage::UnknownWord { segment })?;
            self.pending_mut().unknown_word = Some(word);
            let position = self.read::<I, 12>(file, io, SegmentsLoadStage::Position { segment })?;
            self.pending_mut().position_bits = Some([
                u32::from_le_bytes(position[0..4].try_into().unwrap()),
                u32::from_le_bytes(position[4..8].try_into().unwrap()),
                u32::from_le_bytes(position[8..12].try_into().unwrap()),
            ]);
            let count = self.read_word(file, io, SegmentsLoadStage::IndexCount { segment })?;
            self.pending_mut().declared_index_count = Some(count);
            for index in 0..count {
                let stage = SegmentsLoadStage::Index { segment, index };
                let value = self.read_word(file, io, stage)?;
                self.pending_mut()
                    .indices
                    .try_reserve(1)
                    .map_err(|_| Self::allocation_error(stage))?;
                self.pending_mut().indices.push(value);
            }
            let stage = SegmentsLoadStage::Data { segment };
            self.state = SegmentsLoadState::Reading(stage);
            let data = match self.pending_mut().get_type() {
                1 => CheckSegData::OpaqueType1(self.read::<I, 0x28>(file, io, stage)?),
                2 => CheckSegData::OpaqueType2(self.read::<I, 0x34>(file, io, stage)?),
                4 => CheckSegData::OpaqueType4(self.read::<I, 0x10>(file, io, stage)?),
                kind => {
                    return Err(Self::unsupported_error(
                        stage,
                        SegmentsUnsupportedDependency::SegmentDataType { kind },
                    ));
                }
            };
            self.pending_mut().data = Some(data);
            let count = self.pending_mut().num_specials();
            let stage = SegmentsLoadStage::Specials { segment };
            self.state = SegmentsLoadState::Reading(stage);
            self.pending_mut()
                .specials
                .try_reserve_exact(count as usize)
                .map_err(|_| Self::allocation_error(stage))?;
            for index in 0..count {
                let stage = SegmentsLoadStage::Special { segment, index };
                self.state = SegmentsLoadState::Reading(stage);
                let special = match self.pending_mut().special_type(index) {
                    1 => CheckSegSpecial::OpaqueType1(self.read::<I, 0x38>(file, io, stage)?),
                    2 => {
                        let mut record = self.read::<I, 0x38>(file, io, stage)?;
                        record[0x30..0x38].fill(0);
                        CheckSegSpecial::Corner(AiSegCorner { record })
                    }
                    4 => CheckSegSpecial::OpaqueType4(self.read::<I, 0x18>(file, io, stage)?),
                    8 => CheckSegSpecial::OpaqueType8(self.read::<I, 0x2c>(file, io, stage)?),
                    kind => {
                        return Err(Self::unsupported_error(
                            stage,
                            SegmentsUnsupportedDependency::SpecialDataType { kind },
                        ));
                    }
                };
                self.pending_mut().specials.push(special);
            }
            self.add_pending(segment)?;
        }
        Ok(true)
    }

    fn pending_mut(&mut self) -> &mut CheckSeg {
        self.pending.as_mut().unwrap()
    }

    fn add_pending<E>(&mut self, segment: u32) -> Result<(), SegmentsLoadError<E>> {
        let stage = SegmentsLoadStage::AddSegment { segment };
        self.state = SegmentsLoadState::Reading(stage);
        let last = self.clusters.last().unwrap();
        let slot = last.segments.iter().position(Option::is_none);
        let slot = match slot {
            Some(slot) => slot,
            None => {
                self.clusters
                    .try_reserve_exact(1)
                    .map_err(|_| Self::allocation_error(stage))?;
                self.clusters.push(CheckSegCluster::default());
                0
            }
        };
        self.clusters.last_mut().unwrap().segments[slot] = self.pending.take();
        Ok(())
    }

    fn read<I: SegmentsIo, const WIDTH: usize>(
        &mut self,
        file: &mut I::File,
        io: &mut I,
        stage: SegmentsLoadStage,
    ) -> Result<[u8; WIDTH], SegmentsLoadError<I::Error>> {
        self.state = SegmentsLoadState::Reading(stage);
        let mut bytes = [0; WIDTH];
        let received = io
            .read(file, &mut bytes)
            .map_err(|error| SegmentsLoadError {
                stage,
                failure: SegmentsLoadFailure::Io(error),
                close_error: None,
            })?;
        if received < WIDTH {
            return Err(Self::unsupported_error(
                stage,
                SegmentsUnsupportedDependency::ShortNativeRead {
                    requested: WIDTH,
                    received,
                },
            ));
        }
        if received > WIDTH {
            return Err(Self::unsupported_error(
                stage,
                SegmentsUnsupportedDependency::InvalidReadCount {
                    requested: WIDTH,
                    received,
                },
            ));
        }
        Ok(bytes)
    }

    fn read_word<I: SegmentsIo>(
        &mut self,
        file: &mut I::File,
        io: &mut I,
        stage: SegmentsLoadStage,
    ) -> Result<u32, SegmentsLoadError<I::Error>> {
        self.read::<I, 4>(file, io, stage).map(u32::from_le_bytes)
    }

    fn allocation_error<E>(stage: SegmentsLoadStage) -> SegmentsLoadError<E> {
        SegmentsLoadError {
            stage,
            failure: SegmentsLoadFailure::Allocation,
            close_error: None,
        }
    }

    fn unsupported_error<E>(
        stage: SegmentsLoadStage,
        dependency: SegmentsUnsupportedDependency,
    ) -> SegmentsLoadError<E> {
        SegmentsLoadError {
            stage,
            failure: SegmentsLoadFailure::Unsupported(dependency),
            close_error: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Eq, PartialEq)]
    enum Event {
        Open(Vec<u8>, bool),
        Read { offset: usize, width: usize },
        Close,
    }

    #[derive(Default)]
    struct FixtureIo {
        bytes: Vec<u8>,
        cursor: usize,
        events: Vec<Event>,
        reads: usize,
        fail_open: bool,
        missing_source: bool,
        fail_read: Option<usize>,
        fail_close: bool,
        replace_after_header: Option<Vec<u8>>,
    }

    impl FixtureIo {
        fn new(bytes: Vec<u8>) -> Self {
            Self {
                bytes,
                ..Self::default()
            }
        }

        fn read_widths(&self) -> Vec<usize> {
            self.events
                .iter()
                .filter_map(|event| match event {
                    Event::Read { width, .. } => Some(*width),
                    _ => None,
                })
                .collect()
        }
    }

    impl SegmentsIo for FixtureIo {
        type File = ();
        type Error = &'static str;

        fn open(&mut self, path: &[u8], archive: bool) -> Result<Option<()>, Self::Error> {
            self.events.push(Event::Open(path.to_vec(), archive));
            if self.fail_open {
                Err("open")
            } else {
                Ok((!self.missing_source).then_some(()))
            }
        }

        fn read(&mut self, _: &mut (), output: &mut [u8]) -> Result<usize, Self::Error> {
            let ordinal = self.reads;
            self.reads += 1;
            self.events.push(Event::Read {
                offset: self.cursor,
                width: output.len(),
            });
            if self.fail_read == Some(ordinal) {
                return Err("read");
            }
            let length = output.len().min(self.bytes.len() - self.cursor);
            output[..length].copy_from_slice(&self.bytes[self.cursor..self.cursor + length]);
            self.cursor += length;
            if ordinal == 0 {
                if let Some(bytes) = self.replace_after_header.take() {
                    self.bytes = bytes;
                }
            }
            Ok(length)
        }

        fn close(&mut self, _: ()) -> Result<(), Self::Error> {
            self.events.push(Event::Close);
            if self.fail_close {
                Err("close")
            } else {
                Ok(())
            }
        }
    }

    fn record(flags: u32, indices: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&flags.to_le_bytes());
        bytes.extend_from_slice(&0xf123_4567u32.to_le_bytes());
        for component in [0x8000_0000u32, 0x7fc0_1234, 0xff80_0000] {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
        bytes.extend_from_slice(&(indices.len() as u32).to_le_bytes());
        for index in indices {
            bytes.extend_from_slice(&index.to_le_bytes());
        }
        let width = match flags & 0xf {
            1 => 0x28,
            2 => 0x34,
            4 => 0x10,
            _ => 0,
        };
        bytes.extend(std::iter::repeat_n(0x31, width));
        for index in 0..flags >> 28 {
            let kind = flags.wrapping_shr(index * 4 + 4) & 0xf;
            let width = match kind {
                1 | 2 => 0x38,
                4 => 0x18,
                8 => 0x2c,
                _ => break,
            };
            bytes.extend(std::iter::repeat_n(0x70 + index as u8, width));
        }
        bytes
    }

    fn fixture(records: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = HEADER.to_vec();
        bytes.extend_from_slice(&(records.len() as u32).to_le_bytes());
        for record in records {
            bytes.extend_from_slice(record);
        }
        bytes
    }

    fn occupied(owner: &CheckSegClusters) -> Vec<&CheckSeg> {
        owner
            .clusters()
            .iter()
            .flat_map(|cluster| cluster.segments().iter().flatten())
            .collect()
    }

    #[test]
    fn zero_record_file_initializes_one_cluster_and_reads_only_header_and_count() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&[]));
        assert_eq!(owner.state(), SegmentsLoadState::Cleared);
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(true));
        assert_eq!(owner.state(), SegmentsLoadState::Loaded);
        assert_eq!(owner.clusters().len(), 1);
        assert!(owner.clusters()[0].segments().iter().all(Option::is_none));
        assert_eq!(owner.declared_segment_count(), Some(0));
        assert_eq!(io.read_widths(), [16, 4]);
        assert_eq!(
            io.events[0],
            Event::Open(b"Missions\\synthetic\\clusters.seg".to_vec(), true)
        );
        assert_eq!(io.events.last(), Some(&Event::Close));
    }

    #[test]
    fn records_preserve_unsigned_words_nan_bits_and_all_evidenced_payload_widths() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&[
            record(0x4008_4211, &[u32::MAX, 0x8000_0000]),
            record(2, &[]),
            record(4, &[7]),
        ]));
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(true));
        let segments = occupied(&owner);
        assert_eq!(segments.len(), 3);
        assert_eq!(segments[0].unknown_word(), Some(0xf123_4567));
        assert_eq!(
            segments[0].position_bits(),
            Some([0x8000_0000, 0x7fc0_1234, 0xff80_0000])
        );
        assert_eq!(segments[0].indices(), [u32::MAX, 0x8000_0000]);
        assert_eq!(segments[0].get_type(), 1);
        assert_eq!(segments[0].num_specials(), 4);
        assert_eq!(segments[0].data().unwrap().bytes(), [0x31; 0x28]);
        assert_eq!(segments[1].data().unwrap().bytes().len(), 0x34);
        assert_eq!(segments[2].data().unwrap().bytes().len(), 0x10);
        assert_eq!(
            io.read_widths(),
            [
                16, 4, 4, 4, 12, 4, 4, 4, 0x28, 0x38, 0x38, 0x18, 0x2c, 4, 4, 12, 4, 0x34, 4, 4,
                12, 4, 4, 0x10,
            ]
        );
        for (index, kind) in [1, 2, 4, 8].into_iter().enumerate() {
            assert_eq!(segments[0].get_special(index as u32).unwrap().0, kind);
        }
        let CheckSegSpecial::Corner(corner) = &segments[0].specials()[1] else {
            panic!("expected corner");
        };
        assert_eq!(corner.bytes()[..0x30], [0x71; 0x30]);
        assert_eq!(corner.bytes()[0x30..], [0; 8]);
        assert!(owner.pending_segment().is_none());
    }

    #[test]
    fn fifth_record_grows_once_and_retains_four_record_cluster_groups() {
        let records: Vec<_> = (0..5).map(|index| record(1, &[index])).collect();
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&records));
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(true));
        assert_eq!(owner.clusters().len(), 2);
        assert!(owner.clusters()[0].segments().iter().all(Option::is_some));
        assert!(owner.clusters()[1].segments()[1..]
            .iter()
            .all(Option::is_none));
        for (index, segment) in occupied(&owner).into_iter().enumerate() {
            assert_eq!(segment.indices(), [index as u32]);
        }
    }

    #[test]
    fn getters_borrow_live_slots_and_count_the_occupied_last_cluster_prefix() {
        let mut owner = CheckSegClusters::new();
        assert_eq!(owner.num_segments(), 0);
        assert!(owner.get_segment(0).is_none());
        for count in [0u32, 1, 3, 4, 5, 8, 9] {
            let records: Vec<_> = (0..count).map(|index| record(4, &[index])).collect();
            let mut io = FixtureIo::new(fixture(&records));
            assert_eq!(owner.load(b"synthetic", &mut io), Ok(true));
            assert_eq!(owner.num_segments(), count);
            for index in 0..count {
                let segment = owner.get_segment(index).unwrap();
                assert_eq!(segment.indices(), [index]);
                let slot = owner.clusters()[(index >> 2) as usize].segments()[(index & 3) as usize]
                    .as_ref()
                    .unwrap();
                assert!(std::ptr::eq(segment, slot));
            }
            for index in [count, count + 3, u32::MAX] {
                assert!(owner.get_segment(index).is_none());
            }
        }
        owner.clear();
        assert_eq!(owner.num_segments(), 0);
        assert!(owner.get_segment(0).is_none());
    }

    #[test]
    fn each_future_field_is_read_after_prior_io_effects_not_from_a_file_snapshot() {
        let future = fixture(&[record(4, &[91])]);
        let mut io = FixtureIo::new(fixture(&[]));
        io.replace_after_header = Some(future);
        let mut owner = CheckSegClusters::new();
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(true));
        assert_eq!(owner.declared_segment_count(), Some(1));
        assert_eq!(occupied(&owner)[0].indices(), [91]);
    }

    #[test]
    fn header_mismatch_closes_before_returning_false_and_drops_old_clusters() {
        let mut owner = CheckSegClusters::new();
        let mut initial = FixtureIo::new(fixture(&[record(1, &[])]));
        assert_eq!(owner.load(b"synthetic", &mut initial), Ok(true));
        let mut bytes = fixture(&[]);
        bytes[0] = b'X';
        let mut io = FixtureIo::new(bytes);
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(false));
        assert_eq!(owner.state(), SegmentsLoadState::RejectedHeader);
        assert_eq!(owner.clusters().len(), 1);
        assert!(occupied(&owner).is_empty());
        assert_eq!(owner.declared_segment_count(), None);
        assert_eq!(io.read_widths(), [16]);
        assert_eq!(io.events.last(), Some(&Event::Close));
    }

    #[test]
    fn header_comparison_accepts_ascii_case_but_not_a_nonzero_terminator() {
        for valid in [true, false] {
            let mut bytes = fixture(&[]);
            bytes[..15].make_ascii_lowercase();
            if !valid {
                bytes[15] = b'X';
            }
            let mut owner = CheckSegClusters::new();
            let mut io = FixtureIo::new(bytes);
            assert_eq!(owner.load(b"synthetic", &mut io), Ok(valid));
        }
    }

    #[test]
    fn read_failure_keeps_completed_prefix_and_requires_explicit_clear_before_reload() {
        let bytes = fixture(&[record(1, &[]), record(2, &[])]);
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(bytes.clone());
        io.fail_read = Some(9);
        let error = owner.load(b"synthetic", &mut io).unwrap_err();
        let stage = SegmentsLoadStage::Position { segment: 1 };
        assert_eq!(error.stage, stage);
        assert_eq!(error.failure, SegmentsLoadFailure::Io("read"));
        assert_eq!(error.close_error, None);
        assert_eq!(owner.state(), SegmentsLoadState::Interrupted(stage));
        assert_eq!(occupied(&owner).len(), 1);
        assert_eq!(owner.num_segments(), 1);
        assert_eq!(owner.get_segment(0).unwrap().get_type(), 1);
        assert!(owner.get_segment(1).is_none());
        let pending = owner.pending_segment().unwrap();
        assert_eq!(pending.flags(), 2);
        assert_eq!(pending.unknown_word(), Some(0xf123_4567));
        assert_eq!(pending.position_bits(), None);
        let events = io.events.len();
        assert_eq!(
            owner.load(b"synthetic", &mut io).unwrap_err().failure,
            SegmentsLoadFailure::RequiresClear
        );
        assert_eq!(io.events.len(), events);
        owner.clear();
        owner.clear();
        assert_eq!(owner.state(), SegmentsLoadState::Cleared);
        assert!(owner.clusters().is_empty());
        assert!(owner.pending_segment().is_none());
        assert_eq!(io.events.len(), events);
        assert_eq!(
            owner.load(b"synthetic", &mut FixtureIo::new(bytes)),
            Ok(true)
        );
    }

    #[test]
    fn open_error_interrupts_after_initializing_empty_cluster() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo {
            fail_open: true,
            ..Default::default()
        };
        let error = owner.load(b"synthetic", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::Open);
        assert_eq!(error.failure, SegmentsLoadFailure::Io("open"));
        assert_eq!(owner.clusters().len(), 1);
        assert!(matches!(io.events.as_slice(), [Event::Open(_, true)]));
    }

    #[test]
    fn absent_source_only_runs_one_lookup() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo {
            missing_source: true,
            ..Default::default()
        };
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(false));
        assert_eq!(owner.state(), SegmentsLoadState::MissingSource);
        assert_eq!(
            io.events,
            [Event::Open(
                b"Missions\\synthetic\\clusters.seg".to_vec(),
                true
            )]
        );
    }

    #[test]
    fn short_read_does_not_retry_or_use_zero_filled_unread_bytes() {
        let mut bytes = fixture(&[record(1, &[19, 23])]);
        bytes.truncate(16 + 4 + 4 + 4 + 12 + 4 + 4 + 2);
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(bytes);
        let error = owner.load(b"synthetic", &mut io).unwrap_err();
        assert_eq!(
            error.stage,
            SegmentsLoadStage::Index {
                segment: 0,
                index: 1
            }
        );
        assert_eq!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::ShortNativeRead {
                requested: 4,
                received: 2
            })
        );
        assert_eq!(owner.pending_segment().unwrap().indices(), [19]);
        assert_eq!(
            owner.pending_segment().unwrap().declared_index_count(),
            Some(2)
        );
        assert!(occupied(&owner).is_empty());
        assert_eq!(io.read_widths().last(), Some(&4));
        assert_eq!(io.events.last(), Some(&Event::Close));
    }

    #[test]
    fn unsupported_segment_type_stops_only_after_its_indices_without_inventing_data() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&[record(3, &[41, 7])]));
        let error = owner.load(b"synthetic", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::Data { segment: 0 });
        assert_eq!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::SegmentDataType {
                kind: 3
            })
        );
        assert_eq!(owner.pending_segment().unwrap().indices(), [41, 7]);
        assert!(owner.pending_segment().unwrap().data().is_none());
        assert_eq!(io.read_widths(), [16, 4, 4, 4, 12, 4, 4, 4]);
    }

    #[test]
    fn unsupported_special_keeps_loaded_data_and_earlier_specials_uncommitted() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&[record(0x2000_0011, &[])]));
        let error = owner.load(b"synthetic", &mut io).unwrap_err();
        assert_eq!(
            error.stage,
            SegmentsLoadStage::Special {
                segment: 0,
                index: 1
            }
        );
        assert_eq!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::SpecialDataType {
                kind: 0
            })
        );
        let pending = owner.pending_segment().unwrap();
        assert!(pending.data().is_some());
        assert_eq!(pending.specials().len(), 1);
        assert_eq!(pending.num_specials(), 2);
        assert!(occupied(&owner).is_empty());
        assert_eq!(io.read_widths(), [16, 4, 4, 4, 12, 4, 0x28, 0x38]);
    }

    #[test]
    fn x86_special_nibble_shift_wraps_after_bit_31_instead_of_rejecting_count_eight() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&[record(0x8111_1111, &[])]));
        assert_eq!(owner.load(b"synthetic", &mut io), Ok(true));
        let segment = occupied(&owner)[0];
        assert_eq!(segment.num_specials(), 8);
        assert_eq!(segment.get_special(6).unwrap().0, 8);
        assert_eq!(segment.get_special(6).unwrap().1.bytes().len(), 0x2c);
        assert_eq!(segment.get_special(7).unwrap().0, 1);
        assert_eq!(segment.get_special(7).unwrap().1.bytes().len(), 0x38);
    }

    #[test]
    fn close_failures_keep_primary_error_and_never_replay_close_or_open() {
        for fail_read in [None, Some(0)] {
            let mut owner = CheckSegClusters::new();
            let mut io = FixtureIo::new(fixture(&[record(4, &[])]));
            io.fail_close = true;
            io.fail_read = fail_read;
            let error = owner.load(b"synthetic", &mut io).unwrap_err();
            if fail_read.is_some() {
                assert_eq!(error.failure, SegmentsLoadFailure::Io("read"));
                assert_eq!(error.close_error, Some("close"));
            } else {
                assert_eq!(error.stage, SegmentsLoadStage::Close);
                assert_eq!(error.failure, SegmentsLoadFailure::Io("close"));
                assert_eq!(occupied(&owner).len(), 1);
            }
            assert_eq!(
                io.events
                    .iter()
                    .filter(|event| **event == Event::Close)
                    .count(),
                1
            );
            let events = io.events.len();
            assert_eq!(
                owner.load(b"synthetic", &mut io).unwrap_err().failure,
                SegmentsLoadFailure::RequiresClear
            );
            owner.clear();
            assert_eq!(io.events.len(), events);
        }
    }

    #[test]
    fn path_uses_c_string_bytes_and_overflow_boundary_precedes_clear_or_open() {
        let mut owner = CheckSegClusters::new();
        let mut io = FixtureIo::new(fixture(&[]));
        assert_eq!(owner.load(b"synth\xff\0ignored", &mut io), Ok(true));
        assert_eq!(
            io.events[0],
            Event::Open(b"Missions\\synth\xff\\clusters.seg".to_vec(), true)
        );
        let state = owner.state();
        let clusters = owner.clusters().len();
        let mut next = FixtureIo::default();
        let error = owner.load(&[b'a'; 42], &mut next).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::MissionPath);
        assert!(matches!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::MissionPathBuffer {
                required: 65
            })
        ));
        assert_eq!(owner.state(), state);
        assert_eq!(owner.clusters().len(), clusters);
        assert!(next.events.is_empty());
        assert_eq!(
            owner.load(&[b'a'; 41], &mut FixtureIo::new(fixture(&[]))),
            Ok(true)
        );
    }
}
