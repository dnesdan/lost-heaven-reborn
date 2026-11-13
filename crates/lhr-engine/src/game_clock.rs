//! One game-owned clock with non-owning script readers.
//! Host wiring, not an original function or a Game::Tick scheduler.
#![forbid(unsafe_code)]
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::{Arc, Weak};

/// The game owns writes. A value clone owns an independent clock.
#[derive(Debug, Default)]
pub struct GameClock(Arc<AtomicI32>);
impl GameClock {
    pub fn get(&self) -> i32 {
        self.0.load(Ordering::Relaxed)
    }
    pub fn set(&mut self, value: i32) {
        self.0.store(value, Ordering::Relaxed);
    }
    pub fn reader(&self) -> GameClockSource {
        GameClockSource(Arc::downgrade(&self.0))
    }
}
impl Clone for GameClock {
    fn clone(&self) -> Self {
        Self(Arc::new(AtomicI32::new(self.get())))
    }
}
impl PartialEq for GameClock {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get()
    }
}
impl Eq for GameClock {}

/// Readers do not keep a discarded game alive or accept a replacement owner.
/// Every read is live. Relaxed accesses transport only this scalar, not a
/// synchronization contract for gameplay; scheduling remains serialized.
#[derive(Clone, Debug)]
pub struct GameClockSource(Weak<AtomicI32>);
impl GameClockSource {
    pub fn read(&self) -> Option<i32> {
        self.0.upgrade().map(|time| time.load(Ordering::Relaxed))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn readers_observe_live_signed_values_and_expire_with_owner() {
        let mut owner = GameClock::default();
        let first = owner.reader();
        let second = first.clone();
        for value in [0, 7, -1, i32::MIN, i32::MAX] {
            owner.set(value);
            assert_eq!(first.read(), Some(value));
            assert_eq!(second.read(), Some(value));
        }
        drop(owner);
        assert_eq!(first.read(), None);
        assert_eq!(second.read(), None);
    }
    #[test]
    fn value_clone_cannot_modify_the_original_clock() {
        let mut owner = GameClock::default();
        owner.set(7);
        let reader = owner.reader();
        let mut copy = owner.clone();
        assert_eq!(owner, copy);
        copy.set(99);
        assert_eq!(reader.read(), Some(7));
        assert_eq!(copy.get(), 99);
        drop(owner);
        assert_eq!(reader.read(), None);
    }
}
