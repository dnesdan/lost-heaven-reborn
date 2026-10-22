//! Caller-supplied projection of Game's animation-index bootstrap.
//!
//! Game SHA-256 `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`:
//! init RVA `0x86b0` (148 bytes), cleanup `0x8750` (13 bytes). The original PS2
//! symbol `InitAnimIndex__Fv` establishes the init name; cleanup has no recovered
//! original name. The PC sort at RVA `0x2107af` and its two helpers are projected
//! only for eight-byte index records and the unsigned-code comparator.
//! No resource catalog, allocator implementation, or original executable is
//! supplied here. Native identities are integers, never host pointers.

use crate::player_control::{AnimationIndexTable, AnimationIndexTableError};

/// Original eight-byte resource record. The resource word is opaque to init;
/// only `code == 0xffff` terminates its scan (even when the resource is null).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationResourceRecord {
    pub resource_identity: u32,
    pub code: u32,
}

/// Explicit implementation of the original allocation/release boundaries.
/// An error means execution stops at that call. `allocate` may return zero;
/// init publishes it before the original would fault on its first write.
/// A release error must mean release did not take place; successful release
/// makes the memory unavailable. This models an unavailable required call,
/// rather than inventing a recoverable error return for native `delete`.
/// The backend owns native allocation identities, including allocations lost
/// by repeated init. Dropping Rust projections never invokes native release.
pub trait AnimationIndexBackend {
    type Error;
    fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error>;
    fn release(&mut self, pointer_identity: u32) -> Result<(), Self::Error>;
}

/// Availability of the memory named by the retained global pointer word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationIndexStorage {
    Null,
    /// Existing memory, or a new allocation whose contents are unavailable.
    Unavailable,
    Live(Vec<(u32, i32)>),
    /// Cleanup returned successfully; the pointer word itself stays intact.
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimationIndexState {
    pointer_identity: u32,
    count: u32,
    storage: AnimationIndexStorage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationIndexError<E> {
    Backend(E),
    MissingSentinel,
    /// Host admission: eight-byte extent must fit positive signed x86 offsets.
    ExtentNotRepresentable,
    /// Host admission after pointer publication: native address span wraps.
    AllocationExtentWraps,
    NullAllocation,
    HostAllocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationIndexViewError {
    Unavailable,
    Released,
    CountExceedsStorage,
    Table(AnimationIndexTableError),
}

impl AnimationIndexState {
    /// Import global words without inventing pointed-to memory. A nonnull
    /// pointer starts unavailable; zero starts null, regardless of count.
    pub fn from_existing_globals(pointer_identity: u32, count: u32) -> Self {
        Self {
            pointer_identity,
            count,
            storage: if pointer_identity == 0 {
                AnimationIndexStorage::Null
            } else {
                AnimationIndexStorage::Unavailable
            },
        }
    }

    pub fn pointer_identity(&self) -> u32 {
        self.pointer_identity
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn storage(&self) -> &AnimationIndexStorage {
        &self.storage
    }

    /// Snapshot for the existing immutable `GetAnimIndex` implementation.
    /// Nonnull zero-count globals are explicitly rejected by that view: native
    /// lookup would still read entry zero. Cleanup never creates a null view.
    pub fn table(&self) -> Result<AnimationIndexTable, AnimationIndexViewError> {
        if self.pointer_identity == 0 {
            return AnimationIndexTable::from_existing_entries(None)
                .map_err(AnimationIndexViewError::Table);
        }
        let entries = match &self.storage {
            AnimationIndexStorage::Live(entries) => entries,
            AnimationIndexStorage::Released => return Err(AnimationIndexViewError::Released),
            _ => return Err(AnimationIndexViewError::Unavailable),
        };
        let entries = entries
            .get(..self.count as usize)
            .ok_or(AnimationIndexViewError::CountExceedsStorage)?;
        AnimationIndexTable::from_existing_entries(Some(entries.to_vec()))
            .map_err(AnimationIndexViewError::Table)
    }

    /// Game RVA `0x86b0`, PC correspondence of original `InitAnimIndex`.
    /// Count is reset and incremented during scanning. Empty input without a
    /// sentinel stops at the unavailable read. A zero-record catalog preserves
    /// the old pointer/storage. Allocator errors preserve the old pointer;
    /// success publishes the new pointer before filling. There is no implicit
    /// release of an earlier allocation. Only the first sentinel is consumed.
    ///
    /// The bounded domain excludes wrapped/signed-overflow native extents.
    /// Null allocation is an explicit fault boundary, not a fabricated table.
    pub fn init_anim_index<B: AnimationIndexBackend + ?Sized>(
        &mut self,
        records: &[AnimationResourceRecord],
        backend: &mut B,
    ) -> Result<(), AnimationIndexError<B::Error>> {
        self.count = 0;
        loop {
            let record = records
                .get(self.count as usize)
                .ok_or(AnimationIndexError::MissingSentinel)?;
            if record.code == 0xffff {
                break;
            }
            if self.count == i32::MAX as u32 / 8 {
                return Err(AnimationIndexError::ExtentNotRepresentable);
            }
            self.count += 1;
        }
        if self.count == 0 {
            return Ok(());
        }
        self.pointer_identity = backend
            .allocate(self.count * 8)
            .map_err(AnimationIndexError::Backend)?;
        self.storage = if self.pointer_identity == 0 {
            AnimationIndexStorage::Null
        } else {
            AnimationIndexStorage::Unavailable
        };
        if self.pointer_identity == 0 {
            return Err(AnimationIndexError::NullAllocation);
        }
        if self
            .pointer_identity
            .checked_add(self.count * 8 - 1)
            .is_none()
        {
            return Err(AnimationIndexError::AllocationExtentWraps);
        }
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(self.count as usize)
            .map_err(|_| AnimationIndexError::HostAllocation)?;
        for (index, record) in records[..self.count as usize].iter().enumerate() {
            entries.push((record.code, index as i32));
        }
        sort_index_records(&mut entries);
        self.storage = AnimationIndexStorage::Live(entries);
        Ok(())
    }

    /// Descriptive name for Game RVA `0x8750` (original name unknown).
    /// Always calls the required backend, including null, unavailable, and
    /// previously released identities. It preserves both native global words.
    /// Release errors retain the projected memory so cleanup can be retried.
    pub fn cleanup_anim_index<B: AnimationIndexBackend + ?Sized>(
        &mut self,
        backend: &mut B,
    ) -> Result<(), B::Error> {
        backend.release(self.pointer_identity)?;
        if self.pointer_identity != 0 {
            self.storage = AnimationIndexStorage::Released;
        }
        Ok(())
    }
}

// Exact specialized control flow of Game's CRT qsort, RVA 0x2107af:
// midpoint-to-first pivot, asymmetric <= / >= scans, first-maximum selection
// sort for <=8 records, and smaller-partition-first stack scheduling. These
// choices are observable for duplicate keys. The fixed 30-slot native stack
// suffices for the admitted positive signed eight-byte extent.
fn sort_index_records(entries: &mut [(u32, i32)]) {
    if entries.len() < 2 {
        return;
    }
    let mut stack = [(0usize, 0usize); 30];
    let mut depth = 0;
    let (mut low, mut high) = (0, entries.len() - 1);
    loop {
        if high - low < 8 {
            while high > low {
                let mut maximum = low;
                for index in low + 1..=high {
                    if entries[index].0 > entries[maximum].0 {
                        maximum = index;
                    }
                }
                entries.swap(maximum, high);
                high -= 1;
            }
        } else {
            // Retain the original inclusive-range pivot arithmetic.
            #[allow(clippy::manual_div_ceil)]
            let middle = low + (high - low + 1) / 2;
            entries.swap(middle, low);
            let pivot = entries[low].0;
            let mut left = low;
            let mut right = high + 1;
            loop {
                left += 1;
                while left <= high && entries[left].0 <= pivot {
                    left += 1;
                }
                right -= 1;
                while right > low && entries[right].0 >= pivot {
                    right -= 1;
                }
                if right < left {
                    break;
                }
                entries.swap(left, right);
            }
            entries.swap(low, right);
            // Original compares signed *byte* distances, with a one-byte -1.
            if ((right - low) as i64 * 8 - 1) < (high as i64 - left as i64) * 8 {
                if left < high {
                    stack[depth] = (left, high);
                    depth += 1;
                }
                if low + 1 < right {
                    high = right - 1;
                    continue;
                }
            } else {
                if low + 1 < right {
                    stack[depth] = (low, right - 1);
                    depth += 1;
                }
                if left < high {
                    low = left;
                    continue;
                }
            }
        }
        if depth == 0 {
            return;
        }
        depth -= 1;
        (low, high) = stack[depth];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Backend {
        allocations: Vec<u32>,
        releases: Vec<u32>,
        allocation: Result<u32, &'static str>,
        release_error: bool,
    }

    impl Default for Backend {
        fn default() -> Self {
            Self {
                allocations: Vec::new(),
                releases: Vec::new(),
                allocation: Ok(0),
                release_error: false,
            }
        }
    }

    impl AnimationIndexBackend for Backend {
        type Error = &'static str;
        fn allocate(&mut self, bytes: u32) -> Result<u32, Self::Error> {
            self.allocations.push(bytes);
            self.allocation
        }
        fn release(&mut self, identity: u32) -> Result<(), Self::Error> {
            self.releases.push(identity);
            if self.release_error {
                Err("release")
            } else {
                Ok(())
            }
        }
    }

    fn records(codes: &[u32]) -> Vec<AnimationResourceRecord> {
        codes
            .iter()
            .map(|&code| AnimationResourceRecord {
                resource_identity: 0,
                code,
            })
            .collect()
    }

    #[test]
    fn sentinel_is_code_only_and_empty_preserves_globals() {
        let mut state = AnimationIndexState::from_existing_globals(91, 42);
        let mut backend = Backend {
            allocation: Ok(17),
            ..Backend::default()
        };
        state
            .init_anim_index(&records(&[0xffff, 8]), &mut backend)
            .unwrap();
        assert_eq!((state.pointer_identity(), state.count()), (91, 0));
        assert_eq!(state.storage(), &AnimationIndexStorage::Unavailable);
        assert!(backend.allocations.is_empty());
        assert_eq!(
            state.init_anim_index(&records(&[4, 8]), &mut backend),
            Err(AnimationIndexError::MissingSentinel)
        );
        assert_eq!((state.pointer_identity(), state.count()), (91, 2));
    }

    #[test]
    fn allocation_and_cleanup_stop_at_the_required_call() {
        let mut state = AnimationIndexState::from_existing_globals(91, 42);
        let mut backend = Backend {
            allocation: Err("allocate"),
            ..Backend::default()
        };
        assert_eq!(
            state.init_anim_index(&records(&[7, 0xffff]), &mut backend),
            Err(AnimationIndexError::Backend("allocate"))
        );
        assert_eq!((state.pointer_identity(), state.count()), (91, 1));
        backend.allocation = Ok(0);
        assert_eq!(
            state.init_anim_index(&records(&[7, 0xffff]), &mut backend),
            Err(AnimationIndexError::NullAllocation)
        );
        assert_eq!((state.pointer_identity(), state.count()), (0, 1));
        backend.allocation = Ok(17);
        state
            .init_anim_index(&records(&[7, 0xffff]), &mut backend)
            .unwrap();
        backend.release_error = true;
        assert_eq!(state.cleanup_anim_index(&mut backend), Err("release"));
        assert_eq!(state.table().unwrap().get_anim_index(7), 0);
        backend.release_error = false;
        state.cleanup_anim_index(&mut backend).unwrap();
        assert_eq!((state.pointer_identity(), state.count()), (17, 1));
        assert_eq!(state.table(), Err(AnimationIndexViewError::Released));
        state.cleanup_anim_index(&mut backend).unwrap();
        assert_eq!(backend.releases, [17, 17, 17]);
    }

    #[test]
    fn duplicate_order_and_unsigned_codes_match_original_sort() {
        let mut state = AnimationIndexState::from_existing_globals(0, 0);
        let mut backend = Backend {
            allocation: Ok(17),
            ..Backend::default()
        };
        state
            .init_anim_index(&records(&[5, 5, 5, 5, 5, 5, 5, 5, 5, 0xffff]), &mut backend)
            .unwrap();
        assert_eq!(
            state.storage(),
            &AnimationIndexStorage::Live(vec![
                (5, 4),
                (5, 1),
                (5, 2),
                (5, 3),
                (5, 0),
                (5, 5),
                (5, 6),
                (5, 7),
                (5, 8)
            ])
        );
        state
            .init_anim_index(&records(&[u32::MAX, 0, 0x8000_0000, 0xffff]), &mut backend)
            .unwrap();
        assert_eq!(
            state.storage(),
            &AnimationIndexStorage::Live(vec![(0, 1), (0x8000_0000, 2), (u32::MAX, 0)])
        );
        assert_eq!(backend.allocations, [72, 24]);
    }

    #[test]
    fn repeated_init_retains_zero_count_memory_and_never_releases() {
        let mut state = AnimationIndexState::from_existing_globals(0, 9);
        let mut backend = Backend {
            allocation: Ok(17),
            ..Backend::default()
        };
        state.cleanup_anim_index(&mut backend).unwrap();
        assert_eq!(backend.releases, [0]);
        state
            .init_anim_index(&records(&[7, 0xffff, 9]), &mut backend)
            .unwrap();
        state
            .init_anim_index(&records(&[0xffff]), &mut backend)
            .unwrap();
        assert_eq!((state.pointer_identity(), state.count()), (17, 0));
        assert_eq!(state.storage(), &AnimationIndexStorage::Live(vec![(7, 0)]));
        assert_eq!(
            state.table(),
            Err(AnimationIndexViewError::Table(
                AnimationIndexTableError::EmptyNonnullTable
            ))
        );
        backend.allocation = Ok(25);
        state
            .init_anim_index(&records(&[8, 0xffff]), &mut backend)
            .unwrap();
        assert_eq!(backend.releases, [0]);
        backend.allocation = Ok(u32::MAX - 3);
        assert_eq!(
            state.init_anim_index(&records(&[8, 0xffff]), &mut backend),
            Err(AnimationIndexError::AllocationExtentWraps)
        );
        assert_eq!(state.pointer_identity(), u32::MAX - 3);
        assert_eq!(state.storage(), &AnimationIndexStorage::Unavailable);
    }
}
