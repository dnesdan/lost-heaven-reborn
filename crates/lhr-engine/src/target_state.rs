//! Owned LS3DF Target links and its deferred-update queue.
//!
//! LS3DF SHA-256 `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`:
//! constructor RVA `0x59490`, LoadImage `0x59f10`, AddToTarget `0x596f0`,
//! removal `0x597f0`, UpdateV `0x59920`, UpdateFrames `0x599b0`,
//! SetTargetFlags `0x596d0`. Parsed intake projects the derived fields after
//! base-frame loading and successful reference resolution; it does not execute
//! native allocators, raw reads, resource registration, or full LoadImage.
//! Nonzero orientation/position/scale modes require further math dependencies.

use crate::frame_transform::FrameKey;
use crate::{
    native_matrix::{checked_f32_add, checked_f32_mul},
    native_transform::SMatrix,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetError {
    DuplicateTarget(FrameKey),
    UnknownTarget(FrameKey),
    TooManyFileReferences,
    InvalidFileReference(u16),
    AllocationOverflow,
    HostAllocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetUpdateError<E> {
    World(E),
    /// The target world cache has already been requested when this is returned.
    UnsupportedTargetFlags(u32),
    NumericDomain,
}

/// Required original frame operations used by Target's look-at mode. World
/// access includes the lazy cache guard. SetDirWorld and Update have independent
/// graph/state effects; errors stop at the unavailable call and retain prefixes.
pub trait TargetFrameBackend {
    type Error;
    fn world_matrix(&mut self, frame: FrameKey) -> Result<SMatrix, Self::Error>;
    fn set_direction_world(
        &mut self,
        frame: FrameKey,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), Self::Error>;
    fn update_frame(&mut self, frame: FrameKey) -> Result<(), Self::Error>;
}

const TARGET_EPSILON: f64 = f32::from_bits(0x322b_cc77) as f64;

fn target_normalized(mut vector: [f32; 3]) -> Option<[f32; 3]> {
    let mul = |a, b| checked_f32_mul(a, b).ok();
    let add = |a, b| checked_f32_add(a, b).ok();
    let squared = add(
        add(mul(vector[2], vector[2])?, mul(vector[1], vector[1])?)?,
        mul(vector[0], vector[0])?,
    )?;
    // Original compares against a double constant, after PC24 subtraction.
    if f64::from(add(squared, -1.0)?.abs()) >= TARGET_EPSILON {
        if f64::from(squared) >= TARGET_EPSILON {
            let reciprocal = 1.0 / squared.sqrt();
            for value in &mut vector {
                *value = mul(*value, reciprocal)?;
            }
        } else {
            let axis = if vector[0] != 0.0 {
                0
            } else if vector[2] != 0.0 {
                2
            } else {
                1
            };
            vector[axis] = if vector[axis] < 0.0 { -1.0 } else { 1.0 };
        }
    }
    Some(vector)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct I3dTargetState {
    flags: u32,
    frames: Vec<FrameKey>,
    frame_capacity: u32,
}

impl Default for I3dTargetState {
    /// Derived-field projection of the original constructor: empty list,
    /// capacity/count zero, growth 32, and target flags 1. The base frame and
    /// original global registration have separate owners.
    fn default() -> Self {
        Self {
            flags: 1,
            frames: Vec::new(),
            frame_capacity: 0,
        }
    }
}

impl I3dTargetState {
    /// Import successfully parsed fields into a fresh Target. LoadImage writes
    /// only the low two bytes of constructor flags, reserves raw u8 count slots,
    /// and appends only nonzero u16 references. The enclosing model loader maps
    /// each reference N to model_frames[N-1], then sets that frame's 0x4000 bit.
    /// The caller must apply that bit to every returned `frames()` entry after
    /// insertion. Duplicate references remain duplicates. This intake rejects
    /// invalid references before mutation, rather than inventing loader recovery.
    pub fn from_parsed_fields(
        flags: u16,
        raw_indices: &[u16],
        model_frames: &[FrameKey],
    ) -> Result<Self, TargetError> {
        if raw_indices.len() > u8::MAX as usize {
            return Err(TargetError::TooManyFileReferences);
        }
        for &index in raw_indices {
            if index != 0 && index as usize > model_frames.len() {
                return Err(TargetError::InvalidFileReference(index));
            }
        }
        let mut frames = Vec::new();
        frames
            .try_reserve_exact(raw_indices.len())
            .map_err(|_| TargetError::HostAllocation)?;
        for &index in raw_indices {
            if index != 0 {
                frames.push(model_frames[index as usize - 1]);
            }
        }
        Ok(Self {
            flags: u32::from(flags),
            frames,
            frame_capacity: raw_indices.len() as u32,
        })
    }

    pub fn flags(&self) -> u32 {
        self.flags
    }

    pub fn frames(&self) -> &[FrameKey] {
        &self.frames
    }

    pub fn frame_capacity(&self) -> u32 {
        self.frame_capacity
    }

    pub fn storage_bytes(&self) -> Result<usize, TargetError> {
        self.frames
            .capacity()
            .checked_mul(std::mem::size_of::<FrameKey>())
            .ok_or(TargetError::AllocationOverflow)
    }

    /// Original UpdateFrames look-at branch (flag bit 1 takes precedence over
    /// all other mode bits), with finite normal-or-zero PC24 intermediates.
    /// Target position is captured once. Each link requests its world cache
    /// twice, normalizes displacement and existing world Z, then invokes
    /// SetDirWorld(link, displacement, 0) and Update(link) only when a component
    /// differs by more than the native double 1e-8 tolerance. Degenerate vectors
    /// retain unselected tiny components, matching the original axis fallback.
    pub fn update_frames_with_backend<B: TargetFrameBackend + ?Sized>(
        &self,
        target: FrameKey,
        backend: &mut B,
    ) -> Result<(), TargetUpdateError<B::Error>> {
        if self.frames.is_empty() {
            return Ok(());
        }
        let world = backend
            .world_matrix(target)
            .map_err(TargetUpdateError::World)?;
        let target_position = &world.elements[12..15];
        if self.flags & 1 == 0 {
            return if self.flags & 0xe == 0 {
                Ok(())
            } else {
                Err(TargetUpdateError::UnsupportedTargetFlags(self.flags))
            };
        }
        for &frame in &self.frames {
            let linked = backend
                .world_matrix(frame)
                .map_err(TargetUpdateError::World)?;
            let mut displacement = [0.; 3];
            for axis in 0..3 {
                displacement[axis] =
                    checked_f32_add(target_position[axis], -linked.elements[12 + axis])
                        .map_err(|_| TargetUpdateError::NumericDomain)?;
            }
            let desired =
                target_normalized(displacement).ok_or(TargetUpdateError::NumericDomain)?;
            let linked = backend
                .world_matrix(frame)
                .map_err(TargetUpdateError::World)?;
            let existing = target_normalized(linked.elements[8..11].try_into().unwrap())
                .ok_or(TargetUpdateError::NumericDomain)?;
            let mut changed = false;
            for axis in 0..3 {
                let difference = checked_f32_add(existing[axis], -desired[axis])
                    .map_err(|_| TargetUpdateError::NumericDomain)?;
                if f64::from(difference.abs()) > TARGET_EPSILON {
                    changed = true;
                    break;
                }
            }
            if changed {
                backend
                    .set_direction_world(frame, desired, 0.)
                    .map_err(TargetUpdateError::World)?;
                backend
                    .update_frame(frame)
                    .map_err(TargetUpdateError::World)?;
            }
        }
        Ok(())
    }

    /// Bounded UpdateFrames projection. Empty targets skip all matrix access.
    /// Nonempty targets first request their world cache, even when target flags
    /// are zero. Flags with low nibble zero then leave linked frames untouched.
    /// The callback owns the original lazy world-cache guard and its effects.
    pub fn update_frames<E>(
        &self,
        target: FrameKey,
        mut ensure_world: impl FnMut(FrameKey) -> Result<(), E>,
    ) -> Result<(), TargetUpdateError<E>> {
        if self.frames.is_empty() {
            return Ok(());
        }
        ensure_world(target).map_err(TargetUpdateError::World)?;
        if self.flags & 0xf != 0 {
            return Err(TargetUpdateError::UnsupportedTargetFlags(self.flags));
        }
        Ok(())
    }

    /// Original SetTargetFlags stores the complete u32 before UpdateFrames.
    /// A required matrix/backend error preserves that preceding flag write.
    pub fn set_target_flags<E>(
        &mut self,
        target: FrameKey,
        flags: u32,
        ensure_world: impl FnMut(FrameKey) -> Result<(), E>,
    ) -> Result<(), TargetUpdateError<E>> {
        self.flags = flags;
        self.update_frames(target, ensure_world)
    }

    pub fn set_target_flags_with_backend<B: TargetFrameBackend + ?Sized>(
        &mut self,
        target: FrameKey,
        flags: u32,
        backend: &mut B,
    ) -> Result<(), TargetUpdateError<B::Error>> {
        self.flags = flags;
        self.update_frames_with_backend(target, backend)
    }
}

/// Stable graph keys replace native TargetList pointers. Registration order
/// matters when parsed data leaves the same frame in multiple target lists.
/// Native allocation identities and reference-counted destruction are outside
/// this owned graph projection; host allocation errors remain explicit.
#[derive(Debug, Default)]
pub struct TargetRegistry {
    entries: Vec<(FrameKey, I3dTargetState)>,
}

impl TargetRegistry {
    pub fn storage_bytes(&self) -> Result<usize, TargetError> {
        self.entries.iter().try_fold(
            self.entries
                .capacity()
                .checked_mul(std::mem::size_of::<(FrameKey, I3dTargetState)>())
                .ok_or(TargetError::AllocationOverflow)?,
            |total, entry| {
                total
                    .checked_add(entry.1.storage_bytes()?)
                    .ok_or(TargetError::AllocationOverflow)
            },
        )
    }

    pub fn insert(&mut self, key: FrameKey, state: I3dTargetState) -> Result<(), TargetError> {
        if self.entries.iter().any(|entry| entry.0 == key) {
            return Err(TargetError::DuplicateTarget(key));
        }
        self.entries
            .try_reserve(1)
            .map_err(|_| TargetError::HostAllocation)?;
        self.entries.push((key, state));
        Ok(())
    }

    fn index(&self, key: FrameKey) -> Result<usize, TargetError> {
        self.entries
            .iter()
            .position(|entry| entry.0 == key)
            .ok_or(TargetError::UnknownTarget(key))
    }

    pub fn state(&self, key: FrameKey) -> Result<&I3dTargetState, TargetError> {
        Ok(&self.entries[self.index(key)?].1)
    }

    pub fn state_mut(&mut self, key: FrameKey) -> Result<&mut I3dTargetState, TargetError> {
        let index = self.index(key)?;
        Ok(&mut self.entries[index].1)
    }

    /// Original removal searches the first matching link, replaces it with the
    /// last link, and clears the frame's membership bit even if a duplicate
    /// remains. `false` corresponds to native 0x80000006 (not found).
    pub fn remove_from_target(
        &mut self,
        owner: FrameKey,
        frame: FrameKey,
        frame_flags: &mut u32,
    ) -> Result<bool, TargetError> {
        let state = self.state_mut(owner)?;
        if let Some(index) = state.frames.iter().position(|&key| key == frame) {
            state.frames.swap_remove(index);
            *frame_flags &= !0x4000;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Original AddToTarget first removes the frame from the first registered
    /// owner if its membership bit is set, then appends and sets that bit.
    /// A self-link does nothing. Parsed LoadImage links must use direct intake
    /// above, since that path does not perform this ownership normalization.
    pub fn add_to_target(
        &mut self,
        owner: FrameKey,
        frame: FrameKey,
        frame_flags: &mut u32,
    ) -> Result<(), TargetError> {
        let destination = self.index(owner)?;
        if owner == frame {
            return Ok(());
        }
        if *frame_flags & 0x4000 != 0 {
            if let Some(previous) = self
                .entries
                .iter()
                .find(|entry| entry.1.frames.contains(&frame))
                .map(|entry| entry.0)
            {
                self.remove_from_target(previous, frame, frame_flags)?;
            }
        }
        let state = &mut self.entries[destination].1;
        if state.frames.len() >= state.frame_capacity as usize {
            state.frame_capacity = state
                .frame_capacity
                .checked_add(32)
                .filter(|&capacity| capacity <= i32::MAX as u32 / 4)
                .ok_or(TargetError::AllocationOverflow)?;
            state
                .frames
                .try_reserve_exact(state.frame_capacity as usize - state.frames.len())
                .map_err(|_| TargetError::HostAllocation)?;
        }
        state.frames.push(frame);
        *frame_flags |= 0x4000;
        Ok(())
    }
}

/// Projection of UpdateTargetList and its reentrancy byte. Duplicate queue
/// entries are intentional. The graph flushes this after ordinary Frame::Update
/// clears its root dirty flags (RVA 0x1cb98), not during child traversal.
#[derive(Debug, Default)]
pub struct TargetUpdateQueue {
    pending: Vec<FrameKey>,
    flushing: bool,
}

impl TargetUpdateQueue {
    pub fn storage_bytes(&self) -> Result<usize, TargetError> {
        self.pending
            .capacity()
            .checked_mul(std::mem::size_of::<FrameKey>())
            .ok_or(TargetError::AllocationOverflow)
    }

    /// Host preallocation for graph admission; native UpdateV may still append
    /// duplicates, so this is a reservation, never an artificial queue limit.
    pub fn reserve_pending(&mut self, count: usize) -> Result<(), TargetError> {
        self.pending
            .try_reserve_exact(count.saturating_sub(self.pending.len()))
            .map_err(|_| TargetError::HostAllocation)
    }

    /// Original Target::UpdateV checks only active frame bit 1 and dirty bits;
    /// it never checks target flags or clears any frame flags itself.
    pub fn update_v(&mut self, target: FrameKey, frame_flags: u32) -> Result<(), TargetError> {
        if frame_flags & 1 != 0 && frame_flags & 0x4000_2000 != 0 {
            self.pending
                .try_reserve(1)
                .map_err(|_| TargetError::HostAllocation)?;
            self.pending.push(target);
        }
        Ok(())
    }

    pub fn pending(&self) -> &[FrameKey] {
        &self.pending
    }

    pub fn is_flushing(&self) -> bool {
        self.flushing
    }

    /// Snapshot the initial count, setting the guard before the first callback.
    /// The caller visits precisely this many pending keys in forward order.
    /// On an unavailable callback, retain this state and stop graph execution;
    /// calling finish would falsely represent successful native completion.
    pub fn begin_flush(&mut self) -> Option<usize> {
        if self.flushing || self.pending.is_empty() {
            return None;
        }
        self.flushing = true;
        Some(self.pending.len())
    }

    /// Native completion resets count to zero, including links appended during
    /// callbacks, then clears the reentrancy guard. It retains queue allocation.
    pub fn finish_flush(&mut self) {
        self.pending.clear();
        self.flushing = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_references_are_one_based_skip_zero_and_retain_capacity_and_duplicates() {
        let keys: Vec<_> = (101..125).map(FrameKey).collect();
        let state = I3dTargetState::from_parsed_fields(0, &[0, 11, 0, 1, 11], &keys).unwrap();
        assert_eq!(state.flags(), 0);
        assert_eq!(state.frame_capacity(), 5);
        assert_eq!(state.frames(), &[keys[10], keys[0], keys[10]]);
        assert_eq!(I3dTargetState::default().flags(), 1);
        assert_eq!(
            I3dTargetState::from_parsed_fields(0, &[25], &keys),
            Err(TargetError::InvalidFileReference(25))
        );
    }

    #[test]
    fn zero_flags_still_request_own_world_and_failed_set_preserves_flags() {
        let mut state = I3dTargetState::from_parsed_fields(0, &[1], &[FrameKey(9)]).unwrap();
        let mut calls = Vec::new();
        state
            .update_frames(FrameKey(3), |key| {
                calls.push(key);
                Ok::<_, ()>(())
            })
            .unwrap();
        assert_eq!(calls, [FrameKey(3)]);
        assert_eq!(
            state.set_target_flags(FrameKey(3), 7, |_| Err("world")),
            Err(TargetUpdateError::World("world"))
        );
        assert_eq!(state.flags(), 7);
        assert_eq!(
            state.update_frames(FrameKey(3), |_| Ok::<_, ()>(())),
            Err(TargetUpdateError::UnsupportedTargetFlags(7))
        );
        I3dTargetState::default()
            .update_frames(FrameKey(3), |_| Err::<(), _>("unused"))
            .unwrap();
    }

    #[test]
    fn membership_moves_first_registered_owner_and_removal_swaps_last() {
        let mut registry = TargetRegistry::default();
        registry
            .insert(
                FrameKey(1),
                I3dTargetState::from_parsed_fields(0, &[1, 2, 1], &[FrameKey(9), FrameKey(10)])
                    .unwrap(),
            )
            .unwrap();
        registry
            .insert(FrameKey(2), I3dTargetState::default())
            .unwrap();
        let mut flags = 0x4000;
        registry
            .add_to_target(FrameKey(2), FrameKey(9), &mut flags)
            .unwrap();
        assert_eq!(
            registry.state(FrameKey(1)).unwrap().frames(),
            &[FrameKey(9), FrameKey(10)]
        );
        assert_eq!(
            registry.state(FrameKey(2)).unwrap().frames(),
            &[FrameKey(9)]
        );
        assert_eq!(registry.state(FrameKey(2)).unwrap().frame_capacity(), 32);
        assert_eq!(flags, 0x4000);
        assert!(registry
            .remove_from_target(FrameKey(1), FrameKey(9), &mut flags)
            .unwrap());
        assert_eq!(
            registry.state(FrameKey(1)).unwrap().frames(),
            &[FrameKey(10)]
        );
        assert_eq!(flags, 0);
    }

    #[test]
    fn queue_retains_duplicates_guard_and_snapshot_count() {
        let mut queue = TargetUpdateQueue::default();
        for flags in [0, 1, 0x2000, 0x2001, 0x4000_0001] {
            queue.update_v(FrameKey(2), flags).unwrap();
        }
        assert_eq!(queue.pending(), &[FrameKey(2), FrameKey(2)]);
        assert_eq!(queue.begin_flush(), Some(2));
        assert_eq!(queue.begin_flush(), None);
        queue.update_v(FrameKey(3), 0x2001).unwrap();
        assert_eq!(queue.pending().len(), 3);
        queue.finish_flush();
        assert!(queue.pending().is_empty());
        assert!(!queue.is_flushing());
    }

    #[test]
    fn look_at_orders_required_calls_and_preserves_set_before_failed_update() {
        struct Backend {
            target: SMatrix,
            frame: SMatrix,
            events: Vec<(u8, FrameKey)>,
            fail_update: bool,
        }
        impl TargetFrameBackend for Backend {
            type Error = &'static str;
            fn world_matrix(&mut self, frame: FrameKey) -> Result<SMatrix, Self::Error> {
                self.events.push((0, frame));
                Ok(if frame == FrameKey(1) {
                    self.target
                } else {
                    self.frame
                })
            }
            fn set_direction_world(
                &mut self,
                frame: FrameKey,
                direction: [f32; 3],
                roll: f32,
            ) -> Result<(), Self::Error> {
                self.events.push((1, frame));
                assert_eq!(direction, [1., 0., 0.]);
                assert_eq!(roll.to_bits(), 0);
                self.frame.elements[8..11].copy_from_slice(&direction);
                Ok(())
            }
            fn update_frame(&mut self, frame: FrameKey) -> Result<(), Self::Error> {
                self.events.push((2, frame));
                if self.fail_update {
                    Err("update")
                } else {
                    Ok(())
                }
            }
        }
        let mut backend = Backend {
            target: SMatrix::identity(),
            frame: SMatrix::identity(),
            events: Vec::new(),
            fail_update: true,
        };
        backend.target.elements[12] = 1.;
        let mut state = I3dTargetState::from_parsed_fields(0, &[1, 1], &[FrameKey(2)]).unwrap();
        assert_eq!(
            state.set_target_flags_with_backend(FrameKey(1), 0xf, &mut backend),
            Err(TargetUpdateError::World("update"))
        );
        assert_eq!(state.flags(), 0xf);
        assert_eq!(
            backend.events,
            [
                (0, FrameKey(1)),
                (0, FrameKey(2)),
                (0, FrameKey(2)),
                (1, FrameKey(2)),
                (2, FrameKey(2))
            ]
        );
        assert_eq!(&backend.frame.elements[8..11], &[1., 0., 0.]);
        backend.events.clear();
        backend.fail_update = false;
        state
            .update_frames_with_backend(FrameKey(1), &mut backend)
            .unwrap();
        assert_eq!(
            backend.events,
            [
                (0, FrameKey(1)),
                (0, FrameKey(2)),
                (0, FrameKey(2)),
                (0, FrameKey(2)),
                (0, FrameKey(2))
            ]
        );
    }

    #[test]
    fn degenerate_look_at_retains_unselected_components_and_signed_zeros() {
        assert_eq!(
            target_normalized([1e-6, 2e-6, 3e-6]),
            Some([1., 2e-6, 3e-6])
        );
        assert_eq!(
            target_normalized([-0., 0., -0.]).unwrap().map(f32::to_bits),
            [0x80000000, 0x3f800000, 0x80000000]
        );
        assert_eq!(target_normalized([1e30, 0., 0.]), None);
    }
}
