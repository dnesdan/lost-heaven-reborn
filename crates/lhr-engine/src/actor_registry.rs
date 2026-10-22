//! Bounded extraction from an existing actor/model cache.
//!
//! Descriptive working names are used here. Records and handles must come from
//! an actual owning backend. This module does not construct actors, insert cache
//! entries, or implement the required actor reset routines. Calls are serialized;
//! native allocator behavior and callback reentrancy are outside this adapter.
//! Type-4 scalar transfer models masked x87 exceptions; FP status and unmasked
//! exception/trap behavior are not represented.
use crate::frame_transform::FrameKey;

/// Stable handle to an actor already owned by the supplied backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorKey(pub u32);

/// An owned native-comparable basename in the admitted ASCII domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheName(Vec<u8>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedCacheName;

impl CacheName {
    /// Non-ASCII and embedded NUL require native behavior outside this domain.
    pub fn new(bytes: Vec<u8>) -> Result<Self, UnsupportedCacheName> {
        validate_name(&bytes)?;
        Ok(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn validate_name(bytes: &[u8]) -> Result<(), UnsupportedCacheName> {
    if bytes.is_ascii() && !bytes.contains(&0) {
        Ok(())
    } else {
        Err(UnsupportedCacheName)
    }
}

/// All fields of an existing cache record, including its opaque ownership word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheRecord {
    pub actor: Option<ActorKey>,
    pub frame: Option<FrameKey>,
    pub basename: CacheName,
    pub raw_ownership: u32,
}

/// Receives ownership of a removed basename before logical record removal.
/// The Rust allocation is an adapter; native heap/allocator parity is not claimed.
pub trait CacheNameBackend {
    fn release_cached_name(&mut self, name: Vec<u8>);
}

/// Required original dependencies, with descriptive effect labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorCacheEffect {
    /// Set the raw actor state from 2 to 0 and mark the owning mission dirty.
    ReactivateStateTwo,
    Type4Reset {
        argument_0: u32,
        argument_1: u32,
    },
    /// Apply the scalar through the actor's evidenced embedded subobject.
    /// The masked x87 load/store transfer quiets signaling NaNs first.
    Type4ApplyRawValue {
        bits: u32,
    },
    Type27Reset,
    Type2Or27Reset,
}

/// Access to real actors and their reset routines. There is no success backend.
pub trait ActorCacheBackend: CacheNameBackend {
    type Error;
    fn actor_kind(&mut self, actor: ActorKey) -> Result<u32, Self::Error>;
    fn actor_raw_state(&mut self, actor: ActorKey) -> Result<u32, Self::Error>;
    fn type4_raw_value(&mut self, actor: ActorKey) -> Result<u32, Self::Error>;
    fn apply_actor_effect(
        &mut self,
        actor: ActorKey,
        effect: ActorCacheEffect,
    ) -> Result<(), Self::Error>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CacheTakeStage {
    SelectionKind,
    RawState,
    Effect(ActorCacheEffect),
    KindAfterRemoval,
    Type4RawValue,
}

/// A host availability error, rather than a native return-code translation.
#[derive(Debug, PartialEq, Eq)]
pub enum CacheTakeError<E> {
    UnsupportedName,
    Backend {
        actor: ActorKey,
        stage: CacheTakeStage,
        /// If true, the record/name have been consumed. The backend still owns
        /// this actor; the caller must retain its handle when handling failure.
        removed: bool,
        source: E,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActorRegistry {
    records: Vec<CacheRecord>,
}

impl ActorRegistry {
    /// Supply an existing cache projection; this is not native cache insertion.
    pub fn from_records(records: Vec<CacheRecord>) -> Self {
        Self { records }
    }
    pub fn records(&self) -> &[CacheRecord] {
        &self.records
    }

    /// Reverse search and transfer of a frame-only record. Actor-backed records
    /// are skipped even when they also carry a frame handle.
    pub fn take_frame<B: CacheNameBackend + ?Sized>(
        &mut self,
        name: &[u8],
        backend: &mut B,
    ) -> Result<Option<FrameKey>, UnsupportedCacheName> {
        validate_name(name)?;
        let found = self.records.iter().rposition(|r| {
            r.actor.is_none() && r.frame.is_some() && r.basename.0.eq_ignore_ascii_case(name)
        });
        let Some(index) = found else { return Ok(None) };
        let frame = self.records[index].frame;
        self.remove_record(index, backend);
        Ok(frame)
    }

    /// Transfer an already constructed actor only after all required backend
    /// effects complete. A miss preserves the entire registry.
    pub fn take_actor<B: ActorCacheBackend + ?Sized>(
        &mut self,
        name: &[u8],
        kind: u32,
        backend: &mut B,
    ) -> Result<Option<ActorKey>, CacheTakeError<B::Error>> {
        validate_name(name).map_err(|_| CacheTakeError::UnsupportedName)?;
        let mut found = None;
        for index in (0..self.records.len()).rev() {
            let record = &self.records[index];
            if !record.basename.0.eq_ignore_ascii_case(name) {
                continue;
            }
            let Some(actor) = record.actor else { continue };
            let actual_kind =
                backend
                    .actor_kind(actor)
                    .map_err(|source| CacheTakeError::Backend {
                        actor,
                        stage: CacheTakeStage::SelectionKind,
                        removed: false,
                        source,
                    })?;
            if actual_kind == kind {
                found = Some((index, actor));
                break;
            }
        }
        let Some((index, actor)) = found else {
            return Ok(None);
        };
        let state = backend
            .actor_raw_state(actor)
            .map_err(|source| CacheTakeError::Backend {
                actor,
                stage: CacheTakeStage::RawState,
                removed: false,
                source,
            })?;
        if state == 2 {
            apply(backend, actor, ActorCacheEffect::ReactivateStateTwo, false)?;
        }
        self.remove_record(index, backend);
        let kind = backend
            .actor_kind(actor)
            .map_err(|source| CacheTakeError::Backend {
                actor,
                stage: CacheTakeStage::KindAfterRemoval,
                removed: true,
                source,
            })?;
        match kind {
            4 => {
                apply(
                    backend,
                    actor,
                    ActorCacheEffect::Type4Reset {
                        argument_0: 0,
                        argument_1: 1,
                    },
                    true,
                )?;
                // Read after the first routine: it may have changed this word.
                let bits =
                    backend
                        .type4_raw_value(actor)
                        .map_err(|source| CacheTakeError::Backend {
                            actor,
                            stage: CacheTakeStage::Type4RawValue,
                            removed: true,
                            source,
                        })?;
                apply(
                    backend,
                    actor,
                    ActorCacheEffect::Type4ApplyRawValue {
                        bits: native_float_argument_bits(bits),
                    },
                    true,
                )?;
            }
            27 => {
                apply(backend, actor, ActorCacheEffect::Type27Reset, true)?;
                apply(backend, actor, ActorCacheEffect::Type2Or27Reset, true)?;
            }
            2 => apply(backend, actor, ActorCacheEffect::Type2Or27Reset, true)?,
            _ => {}
        }
        Ok(Some(actor))
    }

    fn remove_record<B: CacheNameBackend + ?Sized>(&mut self, index: usize, backend: &mut B) {
        let name = std::mem::take(&mut self.records[index].basename.0);
        backend.release_cached_name(name);
        self.records.swap_remove(index);
    }
}

// The original transfers this f32 through FLD/FSTP before the call. Under
// masked x87 exceptions this preserves every non-NaN bit and quiets NaNs.
// Floating-point exception flags and unmasked traps are outside this adapter.
fn native_float_argument_bits(bits: u32) -> u32 {
    if bits & 0x7f80_0000 == 0x7f80_0000 && bits & 0x007f_ffff != 0 {
        bits | 0x0040_0000
    } else {
        bits
    }
}

fn apply<B: ActorCacheBackend + ?Sized>(
    backend: &mut B,
    actor: ActorKey,
    effect: ActorCacheEffect,
    removed: bool,
) -> Result<(), CacheTakeError<B::Error>> {
    backend
        .apply_actor_effect(actor, effect)
        .map_err(|source| CacheTakeError::Backend {
            actor,
            stage: CacheTakeStage::Effect(effect),
            removed,
            source,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Name(Vec<u8>),
        Effect(ActorKey, ActorCacheEffect),
    }
    struct Backend {
        kinds: [u32; 4],
        states: [u32; 4],
        scalar: u32,
        dirty: bool,
        events: Vec<Event>,
        fail: Option<ActorCacheEffect>,
    }
    impl Backend {
        fn new() -> Self {
            Self {
                kinds: [4, 2, 27, 9],
                states: [2; 4],
                scalar: 0x8000_0000,
                dirty: false,
                events: Vec::new(),
                fail: None,
            }
        }
    }
    impl CacheNameBackend for Backend {
        fn release_cached_name(&mut self, name: Vec<u8>) {
            self.events.push(Event::Name(name));
        }
    }
    impl ActorCacheBackend for Backend {
        type Error = &'static str;
        fn actor_kind(&mut self, actor: ActorKey) -> Result<u32, Self::Error> {
            self.kinds
                .get(actor.0 as usize)
                .copied()
                .ok_or("missing actor")
        }
        fn actor_raw_state(&mut self, actor: ActorKey) -> Result<u32, Self::Error> {
            self.states
                .get(actor.0 as usize)
                .copied()
                .ok_or("missing actor")
        }
        fn type4_raw_value(&mut self, _: ActorKey) -> Result<u32, Self::Error> {
            Ok(self.scalar)
        }
        fn apply_actor_effect(
            &mut self,
            actor: ActorKey,
            effect: ActorCacheEffect,
        ) -> Result<(), Self::Error> {
            self.events.push(Event::Effect(actor, effect));
            if self.fail == Some(effect) {
                return Err("unavailable");
            }
            if effect == ActorCacheEffect::ReactivateStateTwo {
                self.states[actor.0 as usize] = 0;
                self.dirty = true;
            }
            if matches!(effect, ActorCacheEffect::Type4Reset { .. }) {
                self.scalar = 0x7f80_0001;
            }
            Ok(())
        }
    }
    fn record(actor: Option<u32>, frame: Option<u32>, name: &[u8], word: u32) -> CacheRecord {
        CacheRecord {
            actor: actor.map(ActorKey),
            frame: frame.map(FrameKey),
            basename: CacheName::new(name.to_vec()).unwrap(),
            raw_ownership: word,
        }
    }

    #[test]
    fn native_scalar_argument_preserves_values_and_quiets_nan_payloads() {
        for bits in [
            0,
            0x8000_0000,
            1,
            0x807f_ffff,
            0x0080_0000,
            0x3f80_0000,
            0xff7f_ffff,
            0x7f80_0000,
            0xff80_0000,
            0x7fc0_0001,
            0xffc1_2345,
        ] {
            assert_eq!(native_float_argument_bits(bits), bits);
        }
        assert_eq!(native_float_argument_bits(0x7f80_0001), 0x7fc0_0001);
        assert_eq!(native_float_argument_bits(0xffa1_2345), 0xffe1_2345);
    }

    #[test]
    fn reverse_actor_lookup_preserves_complete_swapped_record_and_effect_order() {
        let last = record(None, Some(9), b"other", 0xdead_beef);
        let mut registry = ActorRegistry::from_records(vec![
            record(Some(0), Some(1), b"taxi", 1),
            record(Some(0), None, b"TAXI", 2),
            last.clone(),
        ]);
        let mut backend = Backend::new();
        assert_eq!(
            registry.take_actor(b"TaXi", 4, &mut backend),
            Ok(Some(ActorKey(0)))
        );
        assert_eq!(registry.records()[1], last);
        assert_eq!(registry.records().len(), 2);
        assert_eq!(backend.states[0], 0);
        assert!(backend.dirty);
        assert_eq!(
            backend.events,
            vec![
                Event::Effect(ActorKey(0), ActorCacheEffect::ReactivateStateTwo),
                Event::Name(b"TAXI".to_vec()),
                Event::Effect(
                    ActorKey(0),
                    ActorCacheEffect::Type4Reset {
                        argument_0: 0,
                        argument_1: 1
                    }
                ),
                Event::Effect(
                    ActorKey(0),
                    ActorCacheEffect::Type4ApplyRawValue { bits: 0x7fc0_0001 }
                ),
            ]
        );
    }

    #[test]
    fn frame_take_skips_actor_records_and_transfers_last_matching_frame() {
        let mut registry = ActorRegistry::from_records(vec![
            record(None, Some(1), b"", 7),
            record(None, Some(2), b"", 8),
            record(Some(0), Some(3), b"", 9),
            record(None, None, b"", 10),
        ]);
        let mut backend = Backend::new();
        assert_eq!(
            registry.take_frame(b"", &mut backend),
            Ok(Some(FrameKey(2)))
        );
        assert_eq!(registry.records()[1].raw_ownership, 10);
        assert_eq!(backend.events, vec![Event::Name(Vec::new())]);
        assert_eq!(registry.take_frame(b"missing", &mut backend), Ok(None));
    }

    #[test]
    fn type_hooks_are_required_and_state_reactivation_is_conditional() {
        for (actor, kind, effects) in [
            (1, 2, vec![ActorCacheEffect::Type2Or27Reset]),
            (
                2,
                27,
                vec![
                    ActorCacheEffect::Type27Reset,
                    ActorCacheEffect::Type2Or27Reset,
                ],
            ),
            (3, 9, vec![]),
        ] {
            let mut registry =
                ActorRegistry::from_records(vec![record(Some(actor), None, b"a", 0)]);
            let mut backend = Backend::new();
            backend.states[actor as usize] = 1;
            assert_eq!(
                registry.take_actor(b"a", kind, &mut backend),
                Ok(Some(ActorKey(actor)))
            );
            let mut expected = vec![Event::Name(b"a".to_vec())];
            expected.extend(
                effects
                    .into_iter()
                    .map(|e| Event::Effect(ActorKey(actor), e)),
            );
            assert_eq!(backend.events, expected);
            assert!(!backend.dirty);
        }
    }

    #[test]
    fn misses_and_unavailable_names_leave_records_unchanged() {
        let original = vec![record(Some(0), Some(1), b"model", u32::MAX)];
        let mut registry = ActorRegistry::from_records(original.clone());
        let mut backend = Backend::new();
        assert_eq!(registry.take_actor(b"model", 2, &mut backend), Ok(None));
        assert_eq!(registry.take_actor(b"other", 4, &mut backend), Ok(None));
        assert_eq!(
            registry.take_actor(b"model\0", 4, &mut backend),
            Err(CacheTakeError::UnsupportedName)
        );
        assert_eq!(
            registry.take_frame(&[0xff], &mut backend),
            Err(UnsupportedCacheName)
        );
        assert!(CacheName::new(vec![0]).is_err());
        assert_eq!(registry.records(), original);
        assert!(backend.events.is_empty());
    }

    #[test]
    fn interrupted_backend_reports_whether_actor_was_transferred() {
        for (effect, removed) in [
            (ActorCacheEffect::ReactivateStateTwo, false),
            (
                ActorCacheEffect::Type4Reset {
                    argument_0: 0,
                    argument_1: 1,
                },
                true,
            ),
        ] {
            let mut registry = ActorRegistry::from_records(vec![record(Some(0), None, b"a", 1)]);
            let mut backend = Backend::new();
            backend.fail = Some(effect);
            assert_eq!(
                registry.take_actor(b"a", 4, &mut backend),
                Err(CacheTakeError::Backend {
                    actor: ActorKey(0),
                    stage: CacheTakeStage::Effect(effect),
                    removed,
                    source: "unavailable",
                })
            );
            assert_eq!(registry.records().is_empty(), removed);
        }
    }
}
