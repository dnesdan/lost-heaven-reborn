//! Game-owned TCK metadata. The original loader is 0x40e020 (RVA 0xe020).
//! This owned adapter preserves serialized fields and reverse cache selection;
//! it does not project the original allocator or 100-byte cache heap layout.

use crate::player_control::AnimationIndexTable;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarTrackError {
    OwnerRequired,
    InstallationRequired,
    Asset(String),
    InvalidName,
    InvalidCatalogIndex(i32),
    DuplicateCatalogCode(u32),
    CatalogUnavailable,
    UnsupportedAnimationId(u32),
    Truncated { offset: usize, bytes: usize },
    Extent,
    MissingPositions,
    NumericDomain,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarAnimationResource {
    pub name: Vec<u8>,
    pub id: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarTrack {
    pub initial_vector: [u32; 3],
    pub final_vector: [u32; 3],
    pub field20: u32,
    pub field24: u32,
    pub positions: Vec<[u32; 3]>,
    pub rotations: Vec<[u32; 3]>,
}

impl CarTrack {
    /// Version mismatch is the original false result. Unavailable required
    /// bytes are distinct: native reads could otherwise consume stack garbage.
    pub fn decode(bytes: &[u8]) -> Result<Option<Self>, CarTrackError> {
        struct Reader<'a> {
            bytes: &'a [u8],
            offset: usize,
        }
        impl Reader<'_> {
            fn word(&mut self) -> Result<u32, CarTrackError> {
                let end = self.offset.checked_add(4).ok_or(CarTrackError::Extent)?;
                let value = self
                    .bytes
                    .get(self.offset..end)
                    .ok_or(CarTrackError::Truncated {
                        offset: self.offset,
                        bytes: 4,
                    })?;
                self.offset = end;
                Ok(u32::from_le_bytes(value.try_into().unwrap()))
            }
            fn vector(&mut self) -> Result<[u32; 3], CarTrackError> {
                Ok([self.word()?, self.word()?, self.word()?])
            }
            fn vectors(&mut self) -> Result<Vec<[u32; 3]>, CarTrackError> {
                let count = self.word()? as usize;
                let size = count.checked_mul(12).ok_or(CarTrackError::Extent)?;
                let end = self.offset.checked_add(size).ok_or(CarTrackError::Extent)?;
                if end > self.bytes.len() {
                    return Err(CarTrackError::Truncated {
                        offset: self.offset,
                        bytes: size,
                    });
                }
                let mut out = Vec::new();
                out.try_reserve_exact(count)
                    .map_err(|_| CarTrackError::Extent)?;
                for _ in 0..count {
                    out.push(self.vector()?);
                }
                Ok(out)
            }
        }
        let mut r = Reader { bytes, offset: 0 };
        if r.word()? != 4 {
            return Ok(None);
        }
        Ok(Some(Self {
            initial_vector: r.vector()?,
            final_vector: r.vector()?,
            field20: r.word()?,
            field24: r.word()?,
            positions: r.vectors()?,
            rotations: r.vectors()?,
        }))
    }
}

/// Explicit Game owner, independent of any one car or LS3DF model. Cars borrow
/// this owner; their destruction never clears shared animation metadata.
#[derive(Clone, Debug)]
pub struct CarTrackResources {
    catalog: Vec<CarAnimationResource>,
    index: AnimationIndexTable,
    cache: Vec<(Vec<u8>, CarTrack)>,
    bounded_catalog: bool,
}

impl CarTrackResources {
    /// The caller supplies the admitted physical catalog and its initialized
    /// GetAnimIndex view. No original executable is required at runtime.
    pub fn new(catalog: Vec<CarAnimationResource>, index: AnimationIndexTable) -> Self {
        Self {
            catalog,
            index,
            cache: Vec::new(),
            bounded_catalog: false,
        }
    }
    /// Convenience admission for catalogs whose codes are unique. Duplicate
    /// catalogs need the original initialized index to preserve sort selection.
    pub fn from_unique_catalog(catalog: Vec<CarAnimationResource>) -> Result<Self, CarTrackError> {
        if catalog.len() > i32::MAX as usize {
            return Err(CarTrackError::Extent);
        }
        let mut entries: Vec<_> = catalog
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id, index as i32))
            .collect();
        entries.sort_by_key(|entry| entry.0);
        for rows in entries.windows(2) {
            if rows[0].0 == rows[1].0 {
                return Err(CarTrackError::DuplicateCatalogCode(rows[0].0));
            }
        }
        let index = AnimationIndexTable::from_existing_entries(if entries.is_empty() {
            None
        } else {
            Some(entries)
        })
        .map_err(|_| CarTrackError::CatalogUnavailable)?;
        Ok(Self::new(catalog, index))
    }
    pub fn animation_name(&self, id: u32) -> Result<Option<Vec<u8>>, CarTrackError> {
        let index = self.index.get_anim_index(id);
        if index < 0 {
            if self.bounded_catalog {
                return Err(CarTrackError::UnsupportedAnimationId(id));
            }
            return Ok(None);
        }
        let resource = self
            .catalog
            .get(index as usize)
            .ok_or(CarTrackError::InvalidCatalogIndex(index))?;
        Ok(Some(resource.name.clone()))
    }
    /// An explicitly bounded reconstructed catalog cannot classify other Game
    /// animation identifiers as native NotFound.
    pub fn restrict_to_supplied_catalog(mut self) -> Self {
        self.bounded_catalog = true;
        self
    }
    pub fn cached_tracks(&self) -> impl Iterator<Item = (&[u8], &CarTrack)> {
        self.cache
            .iter()
            .map(|(name, track)| (name.as_slice(), track))
    }
    /// Explicit animation-owner reset. This is not called by car destruction.
    pub fn reset_track_cache(&mut self) {
        self.cache.clear();
    }
    pub fn load_with(
        &mut self,
        name: &[u8],
        mut read: impl FnMut(&str) -> Result<Option<Vec<u8>>, CarTrackError>,
    ) -> Result<Option<CarTrack>, CarTrackError> {
        if let Some((_, track)) = self
            .cache
            .iter()
            .rev()
            .find(|(cached, _)| cached.eq_ignore_ascii_case(name))
        {
            return Ok(Some(track.clone()));
        }
        if name.len() >= 32 || name.contains(&0) || !name.is_ascii() {
            return Err(CarTrackError::InvalidName);
        }
        let text = std::str::from_utf8(name).map_err(|_| CarTrackError::InvalidName)?;
        let path = format!("anims\\{text}");
        let Some(bytes) = read(&path)? else {
            return Ok(None);
        };
        let Some(track) = CarTrack::decode(&bytes)? else {
            return Ok(None);
        };
        self.cache.push((name.to_vec(), track.clone()));
        Ok(Some(track))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Vec<u8> {
        [
            4u32, 1, 2, 3, 4, 5, 6, 700, 20, 2, 7, 8, 9, 10, 11, 12, 1, 13, 14, 15,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect()
    }
    #[test]
    fn parser_preserves_raw_header_and_array_words() {
        let parsed = CarTrack::decode(&fixture()).unwrap().unwrap();
        assert_eq!(parsed.initial_vector, [1, 2, 3]);
        assert_eq!(parsed.final_vector, [4, 5, 6]);
        assert_eq!((parsed.field20, parsed.field24), (700, 20));
        assert_eq!(parsed.positions, [[7, 8, 9], [10, 11, 12]]);
        assert_eq!(parsed.rotations, [[13, 14, 15]]);
        assert_eq!(CarTrack::decode(&3u32.to_le_bytes()), Ok(None));
        assert!(matches!(
            CarTrack::decode(&fixture()[..75]),
            Err(CarTrackError::Truncated { .. })
        ));
    }
    #[test]
    fn cache_owns_arrays_reuses_case_insensitive_names_and_has_explicit_reset() {
        let mut resources = CarTrackResources::from_unique_catalog(vec![]).unwrap();
        let first = resources
            .load_with(b"synthetic.tck", |path| {
                assert_eq!(path, "anims\\synthetic.tck");
                Ok(Some(fixture()))
            })
            .unwrap()
            .unwrap();
        let second = resources
            .load_with(b"SYNTHETIC.TCK", |_| panic!("cached"))
            .unwrap()
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(resources.cached_tracks().count(), 1);
        resources.reset_track_cache();
        assert_eq!(resources.cached_tracks().count(), 0);
        assert_eq!(resources.load_with(b"missing.tck", |_| Ok(None)), Ok(None));
        assert_eq!(resources.cached_tracks().count(), 0);
    }
}
