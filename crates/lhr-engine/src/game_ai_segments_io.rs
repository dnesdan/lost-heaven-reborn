//! Installation-backed IO for ordered `clusters.seg` loading.
//!
//! This is a decompressed-file-buffer boundary, not a native archive stream.
//! `open` calls `Installation::read_original_path` once; existing source selection,
//! admission checks, decompression and internal stream cleanup happen there.
//! Their errors remain open errors, with no native handle exposed for retention.
//! Internal stream cleanup, including `NativeStream`'s close-on-drop behavior,
//! remains backend-owned. The adapter neither retries it nor reopens Installation.
//! No segment records are parsed here. Each read consumes only the requested
//! available bytes; close consumes the buffer without another backend effect.

#![forbid(unsafe_code)]

use crate::game_ai_segments::SegmentsIo;
use lhr_assets::Installation;
use std::fmt;

pub struct InstallationSegmentsIo<'a> {
    installation: &'a Installation,
}

impl<'a> InstallationSegmentsIo<'a> {
    pub fn new(installation: &'a Installation) -> Self {
        Self { installation }
    }
}

#[derive(Debug)]
pub struct SegmentsFile {
    bytes: Vec<u8>,
    position: usize,
}

impl SegmentsFile {
    pub fn position(&self) -> usize {
        self.position
    }

    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[derive(Debug)]
pub enum SegmentsIoError {
    UnsupportedArchiveMode,
    NonAsciiPath,
    Asset(lhr_assets::Error),
}

impl fmt::Display for SegmentsIoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedArchiveMode => {
                formatter.write_str("segments IO requires archive-enabled lookup")
            }
            Self::NonAsciiPath => formatter.write_str("non-ASCII original filename unsupported"),
            Self::Asset(error) => write!(formatter, "segments source: {error}"),
        }
    }
}

impl std::error::Error for SegmentsIoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Asset(error) => Some(error),
            _ => None,
        }
    }
}

impl SegmentsIo for InstallationSegmentsIo<'_> {
    type File = SegmentsFile;
    type Error = SegmentsIoError;

    fn open(&mut self, path: &[u8], archive: bool) -> Result<SegmentsFile, SegmentsIoError> {
        if !archive {
            return Err(SegmentsIoError::UnsupportedArchiveMode);
        }
        let name = std::str::from_utf8(path).map_err(|_| SegmentsIoError::NonAsciiPath)?;
        if !name.is_ascii() {
            return Err(SegmentsIoError::NonAsciiPath);
        }
        let bytes = self
            .installation
            .read_original_path(name)
            .map_err(SegmentsIoError::Asset)?;
        Ok(SegmentsFile { bytes, position: 0 })
    }

    fn read(
        &mut self,
        file: &mut SegmentsFile,
        output: &mut [u8],
    ) -> Result<usize, SegmentsIoError> {
        let count = output.len().min(file.bytes.len() - file.position);
        let end = file.position + count;
        output[..count].copy_from_slice(&file.bytes[file.position..end]);
        file.position = end;
        Ok(count)
    }

    fn close(&mut self, file: SegmentsFile) -> Result<(), SegmentsIoError> {
        drop(file);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game_ai_segments::{
        CheckSegClusters, SegmentsLoadFailure, SegmentsLoadStage, SegmentsLoadState,
        SegmentsUnsupportedDependency,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    const ORIGINAL_PATH: &[u8] = b"Missions\\SYNTHETIC\\clusters.seg";
    const LOOSE_PATH: &str = "Missions/SYNTHETIC/clusters.seg";
    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

    struct Directory {
        root: PathBuf,
        path: PathBuf,
    }

    impl Directory {
        fn new() -> Self {
            let root = fs::canonicalize(std::env::temp_dir()).unwrap();
            let path = root.join(format!(
                "lhr-segments-io-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self { root, path }
        }

        fn write_loose(&self, bytes: &[u8]) {
            let path = self.path.join(LOOSE_PATH);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }

        fn write_archive(&self, bytes: &[u8]) {
            fs::write(self.path.join("a4.dta"), archive(bytes)).unwrap();
        }
    }

    impl Drop for Directory {
        fn drop(&mut self) {
            if self.path.parent() == Some(self.root.as_path()) {
                let _ = fs::remove_dir_all(&self.path);
            }
        }
    }

    fn archive(content: &[u8]) -> Vec<u8> {
        assert!(!content.is_empty() && content.len() <= 32768);
        let key = lhr_assets::startup_archives::ARCHIVE_UNLOCK_KEYS[8] ^ 0x3498576239475694;
        let mut name = ORIGINAL_PATH.to_vec();
        name.push(0);
        let data_offset = 20 + 32 + name.len();
        let payload_length = content.len() + 1;
        let mut bytes = vec![0; 20];
        bytes[..4].copy_from_slice(b"ISD0");
        let mut record = [0; 32];
        for (offset, value) in [
            (0, (32 + name.len() + 4 + payload_length) as u32),
            (4, 32),
            (16, content.len() as u32),
            (20, 1),
        ] {
            record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        record[24..26].copy_from_slice(&(name.len() as u16 | 0x8000).to_le_bytes());
        lhr_assets::rw_data::de_crypt(&mut record, key);
        bytes.extend_from_slice(&record);
        lhr_assets::rw_data::de_crypt(&mut name, key);
        bytes.extend_from_slice(&name);
        bytes.extend_from_slice(&(payload_length as u32).to_le_bytes());
        let mut payload = vec![0];
        payload.extend_from_slice(content);
        lhr_assets::rw_data::de_crypt(&mut payload, key);
        bytes.extend_from_slice(&payload);
        let table_offset = bytes.len();
        let upper = ORIGINAL_PATH.to_ascii_uppercase();
        let sum = upper
            .iter()
            .fold(0u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
        let hash = ((upper.len() as u32) << 16) | u32::from(sum);
        let mut row = [0; 28];
        row[..4].copy_from_slice(&hash.to_le_bytes());
        row[4..8].copy_from_slice(&20u32.to_le_bytes());
        row[8..12].copy_from_slice(&(data_offset as u32).to_le_bytes());
        let suffix = &upper[upper.len().saturating_sub(16)..];
        row[12..12 + suffix.len()].copy_from_slice(suffix);
        lhr_assets::rw_data::de_crypt(&mut row, key);
        bytes.extend_from_slice(&row);
        let mut header = Vec::new();
        for value in [1, table_offset as u32, 28, 0] {
            header.extend_from_slice(&value.to_le_bytes());
        }
        lhr_assets::rw_data::de_crypt(&mut header, key);
        bytes[4..20].copy_from_slice(&header);
        bytes
    }

    fn empty_segments() -> Vec<u8> {
        let mut bytes = b"AISEG-V1.1-JIPO\0".to_vec();
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes
    }

    #[test]
    fn constructor_borrows_existing_installation_without_rediscovering_archives() {
        let directory = Directory::new();
        let installation = Installation::open(&directory.path).unwrap();
        directory.write_archive(b"new archive must not be registered");
        let mut io = InstallationSegmentsIo::new(&installation);
        assert!(std::ptr::eq(io.installation, &installation));
        directory.write_loose(b"late loose file");
        let mut file = io.open(ORIGINAL_PATH, true).unwrap();
        let mut output = [0; 15];
        assert_eq!(io.read(&mut file, &mut output).unwrap(), 15);
        assert_eq!(&output, b"late loose file");
        io.close(file).unwrap();
    }

    #[test]
    fn archive_lookup_preserves_precedence_and_original_windows_path() {
        let directory = Directory::new();
        directory.write_archive(b"archive bytes");
        directory.write_loose(b"loose bytes");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut file = io.open(b"mIsSiOnS\\synthetic\\CLUSTERS.SEG", true).unwrap();
        let mut output = [0xa5; 16];
        assert_eq!(io.read(&mut file, &mut output).unwrap(), 13);
        assert_eq!(&output[..13], b"archive bytes");
        assert_eq!(&output[13..], &[0xa5; 3]);
        io.close(file).unwrap();
    }

    #[test]
    fn file_buffer_is_acquired_at_open_and_not_reread_or_closed_again() {
        let directory = Directory::new();
        directory.write_loose(b"before");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        directory.write_loose(b"at open");
        let mut file = io.open(ORIGINAL_PATH, true).unwrap();
        fs::remove_file(directory.path.join(LOOSE_PATH)).unwrap();
        let mut output = [0; 7];
        assert_eq!(io.read(&mut file, &mut output).unwrap(), 7);
        assert_eq!(&output, b"at open");
        io.close(file).unwrap();
        assert!(matches!(
            io.open(ORIGINAL_PATH, true),
            Err(SegmentsIoError::Asset(_))
        ));
        directory.write_loose(b"installation remains live");
        assert_eq!(
            installation
                .read_original_path(std::str::from_utf8(ORIGINAL_PATH).unwrap())
                .unwrap(),
            b"installation remains live"
        );
    }

    #[test]
    fn sequential_reads_preserve_short_count_cursor_and_unwritten_bytes() {
        let directory = Directory::new();
        directory.write_loose(b"abcde");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut file = io.open(ORIGINAL_PATH, true).unwrap();
        assert_eq!(file.len(), 5);
        assert!(!file.is_empty());
        assert_eq!(file.position(), 0);
        let mut first = [0; 2];
        assert_eq!(io.read(&mut file, &mut first).unwrap(), 2);
        assert_eq!(&first, b"ab");
        assert_eq!(file.position(), 2);
        let mut tail = [0xa5; 6];
        assert_eq!(io.read(&mut file, &mut tail).unwrap(), 3);
        assert_eq!(&tail, b"cde\xa5\xa5\xa5");
        assert_eq!(file.position(), 5);
        for _ in 0..2 {
            assert_eq!(io.read(&mut file, &mut tail).unwrap(), 0);
            assert_eq!(file.position(), 5);
            assert_eq!(&tail, b"cde\xa5\xa5\xa5");
        }
        io.close(file).unwrap();
    }

    #[test]
    fn empty_reads_do_not_consume_available_bytes() {
        let directory = Directory::new();
        directory.write_loose(b"a");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut file = io.open(ORIGINAL_PATH, true).unwrap();
        assert_eq!(io.read(&mut file, &mut []).unwrap(), 0);
        assert_eq!(file.position(), 0);
        let mut output = [0];
        assert_eq!(io.read(&mut file, &mut output).unwrap(), 1);
        assert_eq!(&output, b"a");
        assert_eq!(io.read(&mut file, &mut []).unwrap(), 0);
        assert_eq!(file.position(), 1);
        io.close(file).unwrap();
    }

    #[test]
    fn open_files_have_independent_cursors_and_consuming_close() {
        let directory = Directory::new();
        directory.write_loose(b"abc");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut first = io.open(ORIGINAL_PATH, true).unwrap();
        let mut second = io.open(ORIGINAL_PATH, true).unwrap();
        assert_eq!(io.read(&mut first, &mut [0; 2]).unwrap(), 2);
        let mut output = [0];
        assert_eq!(io.read(&mut second, &mut output).unwrap(), 1);
        assert_eq!(&output, b"a");
        io.close(second).unwrap();
        assert_eq!(io.read(&mut first, &mut output).unwrap(), 1);
        assert_eq!(&output, b"c");
        io.close(first).unwrap();
    }

    #[test]
    fn archive_disabled_lookup_is_explicitly_unsupported() {
        let directory = Directory::new();
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        assert!(matches!(
            io.open(ORIGINAL_PATH, false),
            Err(SegmentsIoError::UnsupportedArchiveMode)
        ));
    }

    #[test]
    fn non_ascii_bytes_and_utf8_names_are_not_lossily_converted() {
        let directory = Directory::new();
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        for path in [
            b"Missions\\\xff\\clusters.seg".as_slice(),
            "Missions\\\u{e9}\\clusters.seg".as_bytes(),
        ] {
            assert!(matches!(
                io.open(path, true),
                Err(SegmentsIoError::NonAsciiPath)
            ));
        }
    }

    #[test]
    fn unsafe_paths_remain_asset_errors() {
        let directory = Directory::new();
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        for path in [
            b"Missions\\..\\clusters.seg".as_slice(),
            b"C:\\clusters.seg",
            b"\\clusters.seg",
            b"Missions\\bad\0\\clusters.seg",
            b"",
        ] {
            assert!(matches!(
                io.open(path, true),
                Err(SegmentsIoError::Asset(_))
            ));
        }
    }

    #[test]
    fn missing_source_is_an_error_and_interrupted_owner_requires_clear() {
        let directory = Directory::new();
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let error = io.open(ORIGINAL_PATH, true).unwrap_err();
        assert!(
            matches!(&error, SegmentsIoError::Asset(error) if error.0 == "original asset unavailable")
        );
        assert!(std::error::Error::source(&error).is_some());
        let mut segments = CheckSegClusters::new();
        let error = segments.load(b"SYNTHETIC", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::Open);
        assert!(matches!(
            error.failure,
            SegmentsLoadFailure::Io(SegmentsIoError::Asset(_))
        ));
        assert!(error.close_error.is_none());
        assert_eq!(
            segments.state(),
            SegmentsLoadState::Interrupted(SegmentsLoadStage::Open)
        );
        directory.write_loose(&empty_segments());
        assert!(matches!(
            segments.load(b"SYNTHETIC", &mut io).unwrap_err().failure,
            SegmentsLoadFailure::RequiresClear
        ));
        segments.clear();
        assert!(segments.load(b"SYNTHETIC", &mut io).unwrap());
    }

    #[test]
    fn malformed_segment_payload_is_not_eagerly_validated_at_open() {
        let directory = Directory::new();
        directory.write_archive(b"short");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let file = io.open(ORIGINAL_PATH, true).unwrap();
        assert_eq!(file.len(), 5);
        assert_eq!(file.position(), 0);
        io.close(file).unwrap();
        let mut segments = CheckSegClusters::new();
        let error = segments.load(b"SYNTHETIC", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::Header);
        assert!(matches!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::ShortNativeRead {
                requested: 16,
                received: 5
            })
        ));
        assert!(error.close_error.is_none());
    }

    #[test]
    fn complete_wrong_header_is_rejected_by_owner_not_the_adapter() {
        let directory = Directory::new();
        directory.write_archive(&[0; 16]);
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut segments = CheckSegClusters::new();
        assert!(!segments.load(b"SYNTHETIC", &mut io).unwrap());
        assert_eq!(segments.state(), SegmentsLoadState::RejectedHeader);
    }

    #[test]
    fn truncated_record_count_fails_only_when_owner_requests_it() {
        let directory = Directory::new();
        let mut bytes = b"AISEG-V1.1-JIPO\0".to_vec();
        bytes.extend_from_slice(&[0; 2]);
        directory.write_archive(&bytes);
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut segments = CheckSegClusters::new();
        let error = segments.load(b"SYNTHETIC", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::RecordCount);
        assert!(matches!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::ShortNativeRead {
                requested: 4,
                received: 2
            })
        ));
        assert!(segments.declared_segment_count().is_none());
        assert!(error.close_error.is_none());
    }

    #[test]
    fn valid_empty_segment_table_loads_from_archive_without_trailing_validation() {
        let directory = Directory::new();
        let mut bytes = empty_segments();
        bytes.extend_from_slice(b"unrequested trailing data");
        directory.write_archive(&bytes);
        directory.write_loose(b"wrong loose header");
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut segments = CheckSegClusters::new();
        assert!(segments.load(b"SYNTHETIC", &mut io).unwrap());
        assert_eq!(segments.state(), SegmentsLoadState::Loaded);
        assert_eq!(segments.declared_segment_count(), Some(0));
        assert_eq!(segments.clusters().len(), 1);
        assert_eq!(segments.num_segments(), 0);
    }

    #[test]
    fn empty_file_is_a_zero_read_not_a_successful_or_missing_table() {
        let directory = Directory::new();
        directory.write_loose(&[]);
        let installation = Installation::open(&directory.path).unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut file = io.open(ORIGINAL_PATH, true).unwrap();
        assert!(file.is_empty());
        let mut output = [0xa5; 16];
        assert_eq!(io.read(&mut file, &mut output).unwrap(), 0);
        assert_eq!(&output, &[0xa5; 16]);
        assert_eq!(file.position(), 0);
        io.close(file).unwrap();
        let mut segments = CheckSegClusters::new();
        let error = segments.load(b"SYNTHETIC", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::Header);
        assert!(matches!(
            error.failure,
            SegmentsLoadFailure::Unsupported(SegmentsUnsupportedDependency::ShortNativeRead {
                requested: 16,
                received: 0
            })
        ));
    }

    #[test]
    fn selected_archive_failure_is_not_replayed_or_replaced_with_loose_success() {
        let directory = Directory::new();
        directory.write_archive(&empty_segments());
        directory.write_loose(&empty_segments());
        let installation = Installation::open(&directory.path).unwrap();
        fs::write(directory.path.join("a4.dta"), b"changed selected archive").unwrap();
        let mut io = InstallationSegmentsIo::new(&installation);
        let mut segments = CheckSegClusters::new();
        let error = segments.load(b"SYNTHETIC", &mut io).unwrap_err();
        assert_eq!(error.stage, SegmentsLoadStage::Open);
        assert!(
            matches!(error.failure, SegmentsLoadFailure::Io(SegmentsIoError::Asset(error)) if error.0.starts_with("selected archive read:"))
        );
        assert!(error.close_error.is_none());
        assert_eq!(
            segments.state(),
            SegmentsLoadState::Interrupted(SegmentsLoadStage::Open)
        );
        assert!(matches!(
            segments.load(b"SYNTHETIC", &mut io).unwrap_err().failure,
            SegmentsLoadFailure::RequiresClear
        ));
    }
}
