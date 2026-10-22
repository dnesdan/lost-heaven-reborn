//! Existing animation-machine track lookup, before resource loading/selection.
//!
//! Game SHA-256 `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`,
//! RVA `0xe4a0`: PC correspondence of the original PS2 symbol
//! `C_animations_machine_de_luxe::GetTrack(char*)` (`0x11f870`). PC records are
//! 100 bytes. PS2 symbols establish the name, not PC layout or behavior.
//! This neither constructs an actor/animation machine nor loads TCK resources.

/// Original global begin/end words for existing contiguous track records.
/// Words are opaque 32-bit identities, never host pointers. Capacity is unused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AnimationTrackBounds {
    pub begin: u32,
    pub end: u32,
}

/// Required PC CRT `_stricmp` boundary at Game RVA `0x21d0b0`.
/// Compare the NUL-terminated query and record-name strings using the original
/// locale; zero means equality. Missing storage or locale must return an error,
/// never a fabricated miss. The caller intentionally observes changes to the
/// global begin word after a missed comparison; end is read only initially.
pub trait TrackNameBackend {
    type Error;
    fn compare_track_name(
        &mut self,
        query_identity: u32,
        record_identity: u32,
        bounds: &mut AnimationTrackBounds,
    ) -> Result<i32, Self::Error>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GetTrackError<E> {
    /// Host admission: negative native record counts would walk outside the
    /// admitted forward record extent. No comparison has happened yet.
    NegativeRecordCount,
    Backend(E),
}

/// Game RVA `0xe4a0`: search existing records backwards, returning the last
/// matching zero-based index or -1. The native signed subtraction wraps before
/// division by 100, which truncates toward zero; trailing bytes are ignored.
/// Null begin or zero count returns -1 without accessing the query or backend.
/// Opaque record-address addition wraps like x86; resolving identities belongs
/// to the backend. Backend errors stop at that unavailable call and preserve
/// its changes to global words. The original function has no error return.
pub fn get_track<B: TrackNameBackend + ?Sized>(
    bounds: &mut AnimationTrackBounds,
    query_identity: u32,
    backend: &mut B,
) -> Result<i32, GetTrackError<B::Error>> {
    let mut begin = bounds.begin;
    if begin == 0 {
        return Ok(-1);
    }
    let mut remaining = (bounds.end.wrapping_sub(begin) as i32) / 100;
    if remaining < 0 {
        return Err(GetTrackError::NegativeRecordCount);
    }
    if remaining == 0 {
        return Ok(-1);
    }
    let mut offset = (remaining as u32).wrapping_mul(100);
    loop {
        offset = offset.wrapping_sub(100);
        remaining -= 1;
        let comparison = backend
            .compare_track_name(query_identity, begin.wrapping_add(offset), bounds)
            .map_err(GetTrackError::Backend)?;
        if comparison == 0 {
            return Ok(remaining);
        }
        if remaining == 0 {
            return Ok(-1);
        }
        begin = bounds.begin;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Backend {
        calls: Vec<(u32, u32)>,
        results: Vec<Result<i32, &'static str>>,
        changed_begin: Option<u32>,
    }
    impl TrackNameBackend for Backend {
        type Error = &'static str;
        fn compare_track_name(
            &mut self,
            query: u32,
            record: u32,
            bounds: &mut AnimationTrackBounds,
        ) -> Result<i32, Self::Error> {
            self.calls.push((query, record));
            if let Some(begin) = self.changed_begin {
                bounds.begin = begin;
                bounds.end = 0; // The native function never re-reads end.
            }
            self.results.remove(0)
        }
    }

    #[test]
    fn reverse_search_stops_at_last_match_and_keeps_query_identity() {
        let mut bounds = AnimationTrackBounds {
            begin: 0x1000,
            end: 0x112c,
        };
        let mut backend = Backend {
            results: vec![Ok(1), Ok(0)],
            ..Backend::default()
        };
        assert_eq!(get_track(&mut bounds, 0x45, &mut backend), Ok(1));
        assert_eq!(backend.calls, [(0x45, 0x10c8), (0x45, 0x1064)]);
    }

    #[test]
    fn null_and_truncated_zero_counts_do_not_access_names() {
        for (begin, end) in [(0, 1234), (500, 500), (500, 599), (500, 401)] {
            let mut bounds = AnimationTrackBounds { begin, end };
            let mut backend = Backend::default();
            assert_eq!(get_track(&mut bounds, 0, &mut backend), Ok(-1));
            assert!(backend.calls.is_empty());
        }
        let mut bounds = AnimationTrackBounds {
            begin: 500,
            end: 400,
        };
        let mut backend = Backend::default();
        assert_eq!(
            get_track(&mut bounds, 0, &mut backend),
            Err(GetTrackError::NegativeRecordCount)
        );
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn missed_comparison_reloads_begin_and_backend_failure_keeps_prefix() {
        let mut bounds = AnimationTrackBounds {
            begin: 0x1000,
            end: 0x112d,
        };
        let mut backend = Backend {
            results: vec![Ok(-1), Err("name unavailable")],
            changed_begin: Some(0x2000),
            ..Backend::default()
        };
        assert_eq!(
            get_track(&mut bounds, 7, &mut backend),
            Err(GetTrackError::Backend("name unavailable"))
        );
        assert_eq!(backend.calls, [(7, 0x10c8), (7, 0x2064)]);
        assert_eq!(
            bounds,
            AnimationTrackBounds {
                begin: 0x2000,
                end: 0
            }
        );
    }

    #[test]
    fn opaque_address_arithmetic_wraps() {
        let mut bounds = AnimationTrackBounds {
            begin: 0xffff_ffc0,
            end: 0x9e,
        };
        let mut backend = Backend {
            results: vec![Ok(1), Ok(1)],
            ..Backend::default()
        };
        assert_eq!(get_track(&mut bounds, 0, &mut backend), Ok(-1));
        assert_eq!(backend.calls, [(0, 0x24), (0, 0xffff_ffc0)]);
    }
}
