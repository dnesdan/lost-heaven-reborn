//! One retained random stream for serialized game execution.
//! Reuses the VM's MSVC sequence; ownership wiring, not new function credit.
//! Reference: mafia-re fe7e7464, C_actor.cpp rnd overloads (RVA 0x8450/0x8470).
#![forbid(unsafe_code)]
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Weak};

fn step(state: u32) -> u32 {
    state.wrapping_mul(214013).wrapping_add(2531011)
}

/// The represented game owns the stream. Default is the CRT startup seed,
/// not a fresh seed per script. Native Init's host-time seed is caller supplied.
#[derive(Debug)]
pub struct GameRandom(Arc<AtomicU32>);
impl Default for GameRandom {
    fn default() -> Self { Self(Arc::new(AtomicU32::new(1))) }
}
impl GameRandom {
    pub fn seed(&mut self, seed: u32) { self.0.store(seed, Ordering::Relaxed); }
    /// Read-only observation; never a replacement gameplay owner.
    pub fn state(&self) -> u32 { self.0.load(Ordering::Relaxed) }
    pub fn source(&self) -> GameRandomSource {
        GameRandomSource(Arc::downgrade(&self.0))
    }
}
impl Clone for GameRandom {
    fn clone(&self) -> Self { Self(Arc::new(AtomicU32::new(self.state()))) }
}
impl PartialEq for GameRandom {
    fn eq(&self, other: &Self) -> bool { self.state() == other.state() }
}
impl Eq for GameRandom {}

/// A consumer cannot keep a discarded game alive or rebind to its replacement.
/// Atomic updates prevent lost draws; deterministic ordering still requires the
/// game's serialized caller order, not arbitrary concurrent scheduling.
#[derive(Clone, Debug)]
pub struct GameRandomSource(Weak<AtomicU32>);
impl GameRandomSource {
    pub fn next_raw(&mut self) -> Option<u32> {
        let state = self.0.upgrade()?;
        let previous = state.fetch_update(Ordering::Relaxed, Ordering::Relaxed,
            |value| Some(step(value))).expect("random update always supplies a value");
        Some((step(previous) >> 16) & 0x7fff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn consumers_share_the_existing_msvc_sequence() {
        let owner = GameRandom::default();
        let mut a = owner.source();
        let mut b = a.clone();
        assert_eq!([a.next_raw(), b.next_raw(), a.next_raw(), b.next_raw(), a.next_raw()],
            [Some(41), Some(18467), Some(6334), Some(26500), Some(19169)]);
    }
    #[test]
    fn reseeding_is_visible_to_existing_consumers() {
        let mut owner = GameRandom::default();
        let mut source = owner.source();
        assert_eq!(source.next_raw(), Some(41));
        owner.seed(1);
        assert_eq!(source.next_raw(), Some(41));
        owner.seed(0);
        assert_eq!(source.next_raw(), Some(38));
        owner.seed(u32::MAX);
        assert_eq!(source.next_raw(), Some(35));
    }
    #[test]
    fn readers_expire_and_value_clones_are_independent() {
        let owner = GameRandom::default();
        let mut original = owner.source();
        let copy = owner.clone();
        let mut copied = copy.source();
        assert_eq!(original.next_raw(), Some(41));
        assert_eq!(original.next_raw(), Some(18467));
        assert_eq!(copied.next_raw(), Some(41));
        drop(owner);
        assert_eq!(original.next_raw(), None);
        assert_eq!(copied.next_raw(), Some(18467));
    }
}
