//! Owned PC car/player index file storage and the car-model lookup contract.
//! Table intake is a one-shot host read adapter, not a reconstruction of
//! G_CarDatabase::Init's dta handle, allocation and short-read fault behavior.
//! GAME SHA-256 08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27:
//! GetCarModel RVA 0x20a6a0, REA ev_db6ce3f1a46c909e35446fc8fde7199c430dde2bf9942088caa9f158c85d9882;
//! Init RVA 0x20a350, REA ev_3c75140c2868961462f5e5b7cb3fe63de1ce7d48c96c1f1d67c553a8a00ea9f4.
#![forbid(unsafe_code)]

pub const CAR_INDEX_RECORD_BYTES: usize = 0xa8;
pub const PLAYER_INDEX_RECORD_BYTES: usize = 0x24;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IndexTable {
    Cars,
    Players,
}

impl IndexTable {
    pub fn path(self) -> &'static str {
        match self {
            Self::Cars => "tables\\carindex.def",
            Self::Players => "tables\\playerindex.def",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum IndexIntake {
    #[default]
    NotStarted,
    Reading(IndexTable),
    Interrupted(IndexTable),
    Complete,
}

#[derive(Debug, Eq, PartialEq)]
pub enum CarDatabaseError {
    IntakeAlreadyEntered,
    Source { table: IndexTable, error: String },
    FileExtent(IndexTable),
    UnterminatedCarId,
    OutputExtent { required: usize, available: usize },
}

/// Byte arrays are file-record storage, not a simulated native game object.
/// Incomplete final records remain owned but do not increase the native count.
#[derive(Debug, Default)]
pub struct CarDatabase {
    cars: Option<Box<[u8]>>,
    players: Option<Box<[u8]>>,
    intake: IndexIntake,
}

impl CarDatabase {
    pub fn intake(&self) -> IndexIntake {
        self.intake
    }

    pub fn car_count(&self) -> u32 {
        (self.cars.as_ref().map_or(0, |bytes| bytes.len()) / CAR_INDEX_RECORD_BYTES) as u32
    }

    pub fn player_count(&self) -> u32 {
        (self.players.as_ref().map_or(0, |bytes| bytes.len()) / PLAYER_INDEX_RECORD_BYTES) as u32
    }

    pub fn index_storage_bytes(&self) -> usize {
        self.cars.as_ref().map_or(0, |bytes| bytes.len())
            + self.players.as_ref().map_or(0, |bytes| bytes.len())
    }

    /// Read complete files through the installation's existing loose/DTA owner.
    /// A source error retains the completed table prefix and forbids replay.
    /// Missing files leave the corresponding storage untouched. No native
    /// dtaRead result, exception, uninitialized bytes or allocator is invented.
    pub fn read_index_tables(
        &mut self,
        mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, String>,
    ) -> Result<(), CarDatabaseError> {
        if self.intake != IndexIntake::NotStarted {
            return Err(CarDatabaseError::IntakeAlreadyEntered);
        }
        for table in [IndexTable::Cars, IndexTable::Players] {
            self.intake = IndexIntake::Reading(table);
            let bytes = match read(table.path()) {
                Ok(bytes) => bytes,
                Err(error) => {
                    self.intake = IndexIntake::Interrupted(table);
                    return Err(CarDatabaseError::Source { table, error });
                }
            };
            if let Some(bytes) = bytes {
                if bytes.len() > i32::MAX as usize {
                    self.intake = IndexIntake::Interrupted(table);
                    return Err(CarDatabaseError::FileExtent(table));
                }
                match table {
                    IndexTable::Cars => self.cars = Some(bytes.into_boxed_slice()),
                    IndexTable::Players => self.players = Some(bytes.into_boxed_slice()),
                }
            }
        }
        self.intake = IndexIntake::Complete;
        Ok(())
    }

    /// G_CarDatabase::GetCarModel, GAME RVA 0x20a6a0, mafia-re fe7e746.
    /// The output is an actual mutable C-string destination. An invalid index
    /// returns false without touching it. Host admission refuses an unterminated
    /// source allocation or undersized destination before any undefined access.
    pub fn get_car_model(
        &self,
        car: u32,
        destination: &mut [u8],
        second: bool,
    ) -> Result<bool, CarDatabaseError> {
        let index = car & 0xffff;
        if index >= self.car_count() {
            return Ok(false);
        }
        let source = &self.cars.as_ref().unwrap()[index as usize * CAR_INDEX_RECORD_BYTES..];
        let length = source
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(CarDatabaseError::UnterminatedCarId)?;
        let suffix = format!("{:02}.i3d", (car >> 24) + if second { 10 } else { 0 });
        let required = length + suffix.len() + 1;
        if required > destination.len() {
            return Err(CarDatabaseError::OutputExtent {
                required,
                available: destination.len(),
            });
        }
        destination[..length].copy_from_slice(&source[..length]);
        destination[length..required - 1].copy_from_slice(suffix.as_bytes());
        destination[required - 1] = 0;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(ids: &[&[u8]], trailing: usize) -> Vec<u8> {
        let mut bytes = vec![0; ids.len() * CAR_INDEX_RECORD_BYTES + trailing];
        for (index, id) in ids.iter().enumerate() {
            let start = index * CAR_INDEX_RECORD_BYTES;
            bytes[start..start + id.len()].copy_from_slice(id);
        }
        bytes
    }

    fn database(bytes: Vec<u8>) -> CarDatabase {
        let mut database = CarDatabase::default();
        let mut bytes = Some(bytes);
        database
            .read_index_tables(|path| {
                Ok(if path == IndexTable::Cars.path() {
                    bytes.take()
                } else {
                    None
                })
            })
            .unwrap();
        database
    }

    #[test]
    fn original_record_strides_floor_counts_and_preserve_trailing_bytes() {
        let mut database = CarDatabase::default();
        let mut paths = Vec::new();
        database
            .read_index_tables(|path| {
                paths.push(path.to_owned());
                Ok(Some(match path {
                    "tables\\carindex.def" => table(&[b"alpha", b"beta"], 167),
                    "tables\\playerindex.def" => vec![0x81; 3 * 0x24 + 35],
                    _ => panic!("unexpected path"),
                }))
            })
            .unwrap();
        assert_eq!(paths, ["tables\\carindex.def", "tables\\playerindex.def"]);
        assert_eq!(database.intake(), IndexIntake::Complete);
        assert_eq!(database.car_count(), 2);
        assert_eq!(database.player_count(), 3);
        assert_eq!(
            database.index_storage_bytes(),
            2 * 0xa8 + 167 + 3 * 0x24 + 35
        );
    }

    #[test]
    fn missing_files_remain_empty_and_intake_cannot_be_replayed() {
        let mut database = CarDatabase::default();
        database.read_index_tables(|_| Ok(None)).unwrap();
        assert_eq!(database.car_count(), 0);
        assert_eq!(database.player_count(), 0);
        assert_eq!(database.index_storage_bytes(), 0);
        assert_eq!(
            database.read_index_tables(|_| panic!("must not read")),
            Err(CarDatabaseError::IntakeAlreadyEntered)
        );
    }

    #[test]
    fn source_failure_keeps_the_completed_prefix_and_is_sticky() {
        for fail in [IndexTable::Cars, IndexTable::Players] {
            let mut database = CarDatabase::default();
            assert_eq!(
                database.read_index_tables(|path| {
                    if path == fail.path() {
                        Err("interrupted".into())
                    } else {
                        Ok(Some(table(&[b"retained"], 0)))
                    }
                }),
                Err(CarDatabaseError::Source {
                    table: fail,
                    error: "interrupted".into(),
                })
            );
            assert_eq!(database.intake(), IndexIntake::Interrupted(fail));
            assert_eq!(database.car_count(), u32::from(fail == IndexTable::Players));
            assert_eq!(database.player_count(), 0);
            assert_eq!(
                database.read_index_tables(|_| panic!("must not retry")),
                Err(CarDatabaseError::IntakeAlreadyEntered)
            );
        }
    }

    #[test]
    fn descriptor_mask_shift_second_variant_and_minimum_decimal_width() {
        let database = database(table(&[b"first", b"second"], 0));
        for (descriptor, second, expected) in [
            (1, false, b"second00.i3d".as_slice()),
            (0x00ff_0001, false, b"second00.i3d".as_slice()),
            (0x0900_0001, false, b"second09.i3d".as_slice()),
            (0x0a00_0000, false, b"first10.i3d".as_slice()),
            (0xff00_0001, false, b"second255.i3d".as_slice()),
            (0, true, b"first10.i3d".as_slice()),
            (0xff00_0001, true, b"second265.i3d".as_slice()),
        ] {
            let mut output = [0xa5; 64];
            assert_eq!(
                database.get_car_model(descriptor, &mut output, second),
                Ok(true)
            );
            assert_eq!(&output[..expected.len()], expected);
            assert_eq!(output[expected.len()], 0);
            assert!(output[expected.len() + 1..]
                .iter()
                .all(|byte| *byte == 0xa5));
        }
    }

    #[test]
    fn invalid_indices_do_not_read_or_modify_the_destination() {
        for database in [CarDatabase::default(), database(table(&[b"valid"], 167))] {
            for descriptor in [1, 0xffff, u32::MAX] {
                let mut output = [0x81; 19];
                assert_eq!(
                    database.get_car_model(descriptor, &mut output, true),
                    Ok(false)
                );
                assert_eq!(output, [0x81; 19]);
            }
            assert_eq!(database.get_car_model(u32::MAX, &mut [], false), Ok(false));
        }
    }

    #[test]
    fn source_c_string_is_not_truncated_to_the_nominal_id_field() {
        let mut bytes = vec![b'x'; CAR_INDEX_RECORD_BYTES];
        bytes[36] = 0;
        let database = database(bytes);
        let mut output = [0; 64];
        assert_eq!(database.get_car_model(0, &mut output, false), Ok(true));
        assert_eq!(&output[..36], &[b'x'; 36]);
        assert_eq!(&output[36..43], b"00.i3d\0");
    }

    #[test]
    fn raw_bytes_and_host_refusals_do_not_invent_a_filename() {
        let database = database(table(&[&[0xff, 0x81]], 0));
        let mut output = [0xa5; 8];
        assert!(matches!(
            database.get_car_model(0, &mut output, false),
            Err(CarDatabaseError::OutputExtent {
                required: 9,
                available: 8
            })
        ));
        assert_eq!(output, [0xa5; 8]);
        let mut output = [0xa5; 9];
        assert_eq!(database.get_car_model(0, &mut output, false), Ok(true));
        assert_eq!(&output, b"\xff\x8100.i3d\0");
        let database = CarDatabase {
            cars: Some(vec![0xff; CAR_INDEX_RECORD_BYTES].into_boxed_slice()),
            ..Default::default()
        };
        let mut output = [0xa5; 256];
        assert_eq!(
            database.get_car_model(0, &mut output, false),
            Err(CarDatabaseError::UnterminatedCarId)
        );
        assert_eq!(output, [0xa5; 256]);
    }
}
