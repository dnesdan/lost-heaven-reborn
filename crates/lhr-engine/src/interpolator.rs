//! Local IPolTick caller control flow; evaluator/pose processing remains a dependency.
//!
//! LS3DF SHA `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`,
//! RVA `0x1e470`, evidence `PC-IPOL-CALLBACK-ORACLE-STEP3-20261021`.
//! The backend owns evaluator allocations. Cached views must remain readable for
//! their original count or return an error; this adapter never emulates native UAF.

use crate::scene_tick::FrameId;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AnimSetId(pub u32);
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EvaluatorId(pub u32);
/// Identifies backing storage, distinct from its current logical view/count.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AllocationId(pub u32);
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EvaluatorView {
    pub allocation: AllocationId,
    pub count: usize,
}

/// `Default` is empty host-adapter state, not recovered native initialization.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Slot {
    pub animset: Option<AnimSetId>,
    pub enabled: bool,
    pub mode: i32,
    pub current: i32,
    pub previous: i32,
    pub last_delta: i32,
    /// Carried state; the caller does not interpret or modify this value.
    pub weight: f32,
}

/// `Default` is empty host-adapter state; native constructor parity is unverified.
#[derive(Debug, Default)]
pub struct InterpolatorSet {
    pub slots: [Slot; 8],
    pub dirty: bool,
    pub evaluators: EvaluatorView,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TickError {
    InvalidEvaluatorStorage {
        allocation: AllocationId,
        index: usize,
    },
    MissingAnimationSet {
        slot: usize,
    },
    MissingEvaluator(EvaluatorId),
    MissingDuration(AnimSetId),
    MissingFrame(FrameId),
}

/// Object lookup and callbacks are backend responsibilities, including actual
/// helper evaluation and destination eligibility (nonnull frame and flag0x800).
/// End object/storage borrows before mutation or synchronous `set.tick(self, d)`.
/// Replacing a view does not implicitly retain its old allocation: unreadable
/// cached entries must return an error. Effects before an error are not rolled back.
/// Allocation IDs may not be rebound to different storage while a view is active.
pub trait InterpolatorBackend {
    fn evaluator_at(
        &self,
        allocation: AllocationId,
        index: usize,
    ) -> Result<EvaluatorId, TickError>;
    fn evaluate(
        &mut self,
        set: &mut InterpolatorSet,
        evaluator: EvaluatorId,
    ) -> Result<(), TickError>;
    fn boundary_destination(&self, evaluator: EvaluatorId) -> Result<Option<FrameId>, TickError>;
    fn notify_boundary(
        &mut self,
        set: &mut InterpolatorSet,
        frame: FrameId,
        slot: usize,
    ) -> Result<(), TickError>;
    fn duration(&mut self, set: &mut InterpolatorSet, animset: AnimSetId)
        -> Result<i32, TickError>;
}

impl InterpolatorSet {
    fn animation(&self, slot: usize) -> Result<AnimSetId, TickError> {
        self.slots[slot]
            .animset
            .ok_or(TickError::MissingAnimationSet { slot })
    }

    /// Advance all enabled slots, evaluate a cached allocation/count with live
    /// entries, then process boundaries using the current view for each slot.
    pub fn tick<B: InterpolatorBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        delta: i32,
    ) -> Result<(), TickError> {
        if delta != 0 {
            self.dirty = true;
            for slot in &mut self.slots {
                if slot.enabled {
                    slot.current = slot.current.wrapping_add(delta);
                    slot.last_delta = delta;
                }
            }
        }
        if !self.dirty {
            return Ok(());
        }
        let cached = self.evaluators;
        self.dirty = false;
        for index in 0..cached.count {
            let evaluator = backend.evaluator_at(cached.allocation, index)?;
            backend.evaluate(self, evaluator)?;
        }
        for index in 0..8 {
            if !self.slots[index].enabled {
                continue;
            }
            self.slots[index].previous = self.slots[index].current;
            let outside = if self.slots[index].current < 0 {
                true
            } else {
                let animation = self.animation(index)?;
                let duration = backend.duration(self, animation)?;
                // Duration is a virtual call; inspect the current state after it.
                self.slots[index].current >= duration
            };
            if !outside {
                continue;
            }
            let current = self.evaluators;
            for entry in 0..current.count {
                let evaluator = backend.evaluator_at(current.allocation, entry)?;
                if let Some(frame) = backend.boundary_destination(evaluator)? {
                    backend.notify_boundary(self, frame, index)?;
                    break;
                }
            }
            // Notification may replace mode, time, animation, enabled and dirty,
            // or synchronously tick the set. Do not restore earlier values.
            if self.slots[index].mode != 2 {
                self.slots[index].enabled = false;
            } else {
                let negative = self.slots[index].current < 0;
                let animation = self.animation(index)?;
                let duration = backend.duration(self, animation)?;
                let current = self.slots[index].current;
                self.slots[index].current = if negative {
                    current.wrapping_add(duration)
                } else {
                    current.wrapping_sub(duration)
                };
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Event {
        frame: u32,
        event: u32,
        arg: u32,
        time: i32,
        depth: u32,
        dirty: bool,
        enabled: bool,
        current: i32,
        previous: i32,
    }
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    struct Observation {
        enabled: bool,
        mode: i32,
        current: i32,
        previous: i32,
        last_delta: i32,
    }
    impl From<Slot> for Observation {
        fn from(slot: Slot) -> Self {
            Self {
                enabled: slot.enabled,
                mode: slot.mode,
                current: slot.current,
                previous: slot.previous,
                last_delta: slot.last_delta,
            }
        }
    }
    struct Case {
        name: &'static str,
        events: &'static [Event],
        dirty: bool,
        count: usize,
        freed: usize,
        slots: [Observation; 2],
    }

    // Observer dependency only: the native oracle exercised the real helper's
    // single positive-note crossing. This backend emits that observation to test
    // caller control flow; it is NOT a pose/helper or general note-stream port.
    struct Backend {
        case: &'static str,
        notes: bool,
        acted: bool,
        depth: u32,
        allocations: HashMap<AllocationId, Vec<EvaluatorId>>,
        events: Vec<Event>,
        evaluated: Vec<u32>,
        freed: usize,
        durations: Vec<u32>,
    }
    impl Backend {
        fn new(case: &'static str) -> Self {
            Self {
                case,
                notes: case.starts_with("note_"),
                acted: false,
                depth: 0,
                allocations: HashMap::from([
                    (
                        AllocationId(1),
                        vec![EvaluatorId(1), EvaluatorId(2), EvaluatorId(3)],
                    ),
                    (AllocationId(2), vec![EvaluatorId(3)]),
                ]),
                events: vec![],
                evaluated: vec![],
                freed: 0,
                durations: vec![],
            }
        }
        fn event(
            &mut self,
            set: &mut InterpolatorSet,
            frame: FrameId,
            event: u32,
            arg: u32,
            time: i32,
        ) -> Result<(), TickError> {
            let slot = set.slots[0];
            self.events.push(Event {
                frame: frame.0,
                event,
                arg,
                time,
                depth: self.depth,
                dirty: set.dirty,
                enabled: slot.enabled,
                current: slot.current,
                previous: slot.previous,
            });
            if self.acted {
                return Ok(());
            }
            self.acted = true;
            match self.case {
                "boundary_change_mode" => set.slots[0].mode = 1,
                "boundary_change_current_inrange" => set.slots[0].current = 50,
                "boundary_change_current_negative" => set.slots[0].current = -10,
                "boundary_change_animset" => set.slots[0].animset = Some(AnimSetId(2)),
                "boundary_disable_current" | "note_disable_slot" => set.slots[0].enabled = false,
                "boundary_set_dirty" | "note_set_dirty" => set.dirty = true,
                "boundary_reenter" | "note_reenter" => {
                    self.depth += 1;
                    if self.notes {
                        set.dirty = true;
                    }
                    let result = set.tick(self, i32::from(!self.notes));
                    self.depth -= 1;
                    result?;
                }
                "boundary_reorder_evaluators" | "note_reorder_in_place" => self
                    .allocations
                    .get_mut(&AllocationId(1))
                    .unwrap()
                    .swap(0, 1),
                "boundary_replace_evaluator_base"
                | "boundary_replace_and_free_old_vector"
                | "note_replace_vector_base" => {
                    set.evaluators = EvaluatorView {
                        allocation: AllocationId(2),
                        count: 1,
                    };
                    if self.case == "boundary_replace_and_free_old_vector" {
                        self.allocations.remove(&AllocationId(1));
                        self.freed += 1;
                    }
                }
                "boundary_remove_all_evaluators" | "note_shrink_count" => set.evaluators.count = 0,
                "note_append_count" => set.evaluators.count = 3,
                "boundary_enable_next_slot" => set.slots[1].enabled = true,
                "boundary_disable_next_slot" => set.slots[1].enabled = false,
                "free_cached_storage" => {
                    self.allocations.remove(&AllocationId(1));
                    self.freed += 1;
                }
                _ => {}
            }
            Ok(())
        }
    }
    impl InterpolatorBackend for Backend {
        fn evaluator_at(
            &self,
            allocation: AllocationId,
            index: usize,
        ) -> Result<EvaluatorId, TickError> {
            self.allocations
                .get(&allocation)
                .and_then(|values| values.get(index))
                .copied()
                .ok_or(TickError::InvalidEvaluatorStorage { allocation, index })
        }
        fn evaluate(
            &mut self,
            set: &mut InterpolatorSet,
            evaluator: EvaluatorId,
        ) -> Result<(), TickError> {
            self.evaluated.push(evaluator.0);
            if self.case == "advance_all_before_callbacks" {
                assert!(set
                    .slots
                    .iter()
                    .all(|slot| slot.current == 7 && slot.last_delta == 7));
            }
            let slot = set.slots[0];
            if (self.notes || self.case == "free_cached_storage")
                && slot.enabled
                && slot.previous < 40
                && slot.current >= 40
            {
                self.event(set, FrameId(evaluator.0), 0x200, 99 + evaluator.0, 40)?;
            }
            Ok(())
        }
        fn boundary_destination(
            &self,
            evaluator: EvaluatorId,
        ) -> Result<Option<FrameId>, TickError> {
            Ok(
                (self.case != "boundary_first_eligible_destination" || evaluator.0 != 1)
                    .then_some(FrameId(evaluator.0)),
            )
        }
        fn notify_boundary(
            &mut self,
            set: &mut InterpolatorSet,
            frame: FrameId,
            slot: usize,
        ) -> Result<(), TickError> {
            self.event(set, frame, 0x800, slot as u32, 0)
        }
        fn duration(
            &mut self,
            set: &mut InterpolatorSet,
            animation: AnimSetId,
        ) -> Result<i32, TickError> {
            self.durations.push(animation.0);
            if self.case == "duration_mutation" {
                set.slots[0].current = if self.durations.len() == 1 { 500 } else { -50 };
            }
            match animation.0 {
                1 => Ok(400),
                2 => Ok(100),
                _ => Err(TickError::MissingDuration(animation)),
            }
        }
    }
    fn set_for(case: &str) -> InterpolatorSet {
        let notes = case.starts_with("note_") || case == "free_cached_storage";
        let two = matches!(
            case,
            "boundary_reorder_evaluators"
                | "boundary_replace_evaluator_base"
                | "boundary_remove_all_evaluators"
                | "boundary_disable_next_slot"
                | "boundary_replace_and_free_old_vector"
        );
        let mut set = InterpolatorSet {
            evaluators: EvaluatorView {
                allocation: AllocationId(1),
                count: 2,
            },
            ..Default::default()
        };
        set.slots[0] = Slot {
            animset: Some(AnimSetId(1)),
            enabled: true,
            mode: 2,
            current: if notes { 0 } else { 360 },
            ..Default::default()
        };
        set.slots[1] = Slot {
            animset: Some(AnimSetId(1)),
            enabled: two,
            mode: 2,
            current: if two { 360 } else { 390 },
            ..Default::default()
        };
        set
    }

    // Literal sanitized native event/state observations, not helper implementation.
    const CASES: &[Case] = &[
        Case {
            name: "boundary_baseline",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_change_mode",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: false,
                    mode: 1,
                    current: 400,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_change_current_inrange",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: -350,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_change_current_negative",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 390,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_change_animset",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 300,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_disable_current",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_set_dirty",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: true,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_reenter",
            events: &[
                Event {
                    frame: 1,
                    event: 2048,
                    arg: 0,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 400,
                    previous: 400,
                },
                Event {
                    frame: 1,
                    event: 2048,
                    arg: 0,
                    time: 0,
                    depth: 1,
                    dirty: false,
                    enabled: true,
                    current: 401,
                    previous: 401,
                },
            ],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: -399,
                    previous: 401,
                    last_delta: 1,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_reorder_evaluators",
            events: &[
                Event {
                    frame: 1,
                    event: 2048,
                    arg: 0,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 400,
                    previous: 400,
                },
                Event {
                    frame: 2,
                    event: 2048,
                    arg: 1,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 0,
                    previous: 400,
                },
            ],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
            ],
        },
        Case {
            name: "boundary_replace_evaluator_base",
            events: &[
                Event {
                    frame: 1,
                    event: 2048,
                    arg: 0,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 400,
                    previous: 400,
                },
                Event {
                    frame: 3,
                    event: 2048,
                    arg: 1,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 0,
                    previous: 400,
                },
            ],
            dirty: false,
            count: 1,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
            ],
        },
        Case {
            name: "boundary_remove_all_evaluators",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 0,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
            ],
        },
        Case {
            name: "boundary_enable_next_slot",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 390,
                    previous: 390,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "boundary_disable_next_slot",
            events: &[Event {
                frame: 1,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 400,
                    previous: 0,
                    last_delta: 40,
                },
            ],
        },
        Case {
            name: "boundary_replace_and_free_old_vector",
            events: &[
                Event {
                    frame: 1,
                    event: 2048,
                    arg: 0,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 400,
                    previous: 400,
                },
                Event {
                    frame: 3,
                    event: 2048,
                    arg: 1,
                    time: 0,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 0,
                    previous: 400,
                },
            ],
            dirty: false,
            count: 1,
            freed: 1,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
            ],
        },
        Case {
            name: "boundary_first_eligible_destination",
            events: &[Event {
                frame: 2,
                event: 2048,
                arg: 0,
                time: 0,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 400,
                previous: 400,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 0,
                    previous: 400,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_evaluator_order",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 2,
                    event: 512,
                    arg: 101,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_reorder_in_place",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_replace_vector_base",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 2,
                    event: 512,
                    arg: 101,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: false,
            count: 1,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_shrink_count",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 2,
                    event: 512,
                    arg: 101,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: false,
            count: 0,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_append_count",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 2,
                    event: 512,
                    arg: 101,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: false,
            count: 3,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_reenter",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 1,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 2,
                    event: 512,
                    arg: 101,
                    time: 40,
                    depth: 1,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_set_dirty",
            events: &[
                Event {
                    frame: 1,
                    event: 512,
                    arg: 100,
                    time: 40,
                    depth: 0,
                    dirty: false,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
                Event {
                    frame: 2,
                    event: 512,
                    arg: 101,
                    time: 40,
                    depth: 0,
                    dirty: true,
                    enabled: true,
                    current: 40,
                    previous: 0,
                },
            ],
            dirty: true,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: true,
                    mode: 2,
                    current: 40,
                    previous: 40,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
        Case {
            name: "note_disable_slot",
            events: &[Event {
                frame: 1,
                event: 512,
                arg: 100,
                time: 40,
                depth: 0,
                dirty: false,
                enabled: true,
                current: 40,
                previous: 0,
            }],
            dirty: false,
            count: 2,
            freed: 0,
            slots: [
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 40,
                    previous: 0,
                    last_delta: 40,
                },
                Observation {
                    enabled: false,
                    mode: 2,
                    current: 390,
                    previous: 0,
                    last_delta: 0,
                },
            ],
        },
    ];

    #[test]
    fn twenty_three_native_callback_projections() {
        assert_eq!(CASES.len(), 23);
        for case in CASES {
            let mut set = set_for(case.name);
            let mut backend = Backend::new(case.name);
            set.tick(&mut backend, 40).unwrap();
            assert_eq!(backend.events, case.events, "{}", case.name);
            assert_eq!(
                [
                    Observation::from(set.slots[0]),
                    Observation::from(set.slots[1])
                ],
                case.slots,
                "{}",
                case.name
            );
            assert_eq!(set.dirty, case.dirty, "{}", case.name);
            assert_eq!(set.evaluators.count, case.count, "{}", case.name);
            assert_eq!(backend.freed, case.freed, "{}", case.name);
            let expected = if case.name == "note_reorder_in_place" {
                vec![1, 1]
            } else if matches!(case.name, "boundary_reenter" | "note_reenter") {
                vec![1, 1, 2, 2]
            } else {
                vec![1, 2]
            };
            // Boundary reentry happens after the entire outer evaluation pass.
            let expected = if case.name == "boundary_reenter" {
                vec![1, 2, 1, 2]
            } else {
                expected
            };
            assert_eq!(backend.evaluated, expected, "{}", case.name);
        }
    }

    #[test]
    fn eight_native_scalar_and_interface_cases() {
        // Earlier empty-evaluator scalar/interface observations. Two native scene
        // count domains (empty and negative) both map to an empty evaluator view;
        // this does not emulate corrupt signed counts in Rust collections.
        let cases = [
            (
                "disable_slot_in_frame_callback",
                false,
                false,
                0,
                0,
                0,
                2,
                7,
                false,
                0,
                0,
                0,
                false,
                0,
            ),
            (
                "empty_frames_ipol_still_ticks",
                true,
                false,
                0,
                0,
                0,
                2,
                7,
                true,
                7,
                7,
                7,
                false,
                1,
            ),
            (
                "negative_count_ipol_still_ticks",
                true,
                false,
                0,
                0,
                0,
                2,
                7,
                true,
                7,
                7,
                7,
                false,
                1,
            ),
            (
                "zero_delta_clean",
                true,
                false,
                0,
                0,
                0,
                2,
                0,
                true,
                0,
                0,
                0,
                false,
                0,
            ),
            (
                "dirty_zero_wrap_once",
                true,
                true,
                1240,
                0,
                0,
                2,
                0,
                true,
                840,
                1240,
                0,
                false,
                2,
            ),
            (
                "negative_wrap_once",
                true,
                true,
                -840,
                0,
                0,
                2,
                0,
                true,
                -440,
                -840,
                0,
                false,
                1,
            ),
            (
                "once_disable_at_boundary",
                true,
                true,
                400,
                0,
                0,
                1,
                0,
                false,
                400,
                400,
                0,
                false,
                1,
            ),
            (
                "signed_time_add_wrap",
                true,
                false,
                i32::MAX,
                0,
                0,
                2,
                1,
                true,
                -2147483248,
                i32::MIN,
                1,
                false,
                1,
            ),
        ];
        for (
            name,
            enabled,
            dirty,
            current,
            previous,
            last_delta,
            mode,
            delta,
            out_enabled,
            out_current,
            out_previous,
            out_delta,
            out_dirty,
            duration_calls,
        ) in cases
        {
            let mut set = InterpolatorSet {
                dirty,
                ..Default::default()
            };
            set.slots[0] = Slot {
                animset: Some(AnimSetId(1)),
                enabled,
                mode,
                current,
                previous,
                last_delta,
                ..Default::default()
            };
            let mut backend = Backend::new(name);
            set.tick(&mut backend, delta).unwrap();
            assert_eq!(
                Observation::from(set.slots[0]),
                Observation {
                    enabled: out_enabled,
                    mode,
                    current: out_current,
                    previous: out_previous,
                    last_delta: out_delta
                },
                "{name}"
            );
            assert_eq!(set.dirty, out_dirty, "{name}");
            assert_eq!(backend.durations.len(), duration_calls, "{name}");
            assert!(backend.events.is_empty(), "{name}");
        }
    }

    #[test]
    fn freed_cached_allocation_is_an_explicit_error_not_deferred() {
        let mut set = set_for("free_cached_storage");
        let mut backend = Backend::new("free_cached_storage");
        assert_eq!(
            set.tick(&mut backend, 40),
            Err(TickError::InvalidEvaluatorStorage {
                allocation: AllocationId(1),
                index: 1
            })
        );
        assert_eq!(backend.freed, 1);
        assert_eq!(backend.evaluated, vec![1]);
        assert_eq!(set.slots[0].current, 40);
        assert_eq!(set.slots[0].previous, 0); // partial effects, no rollback
    }

    #[test]
    fn invalid_view_count_and_missing_animation_are_rejected() {
        let mut set = InterpolatorSet {
            dirty: true,
            evaluators: EvaluatorView {
                allocation: AllocationId(1),
                count: 4,
            },
            ..Default::default()
        };
        let mut backend = Backend::new("invalid_count");
        assert_eq!(
            set.tick(&mut backend, 0),
            Err(TickError::InvalidEvaluatorStorage {
                allocation: AllocationId(1),
                index: 3
            })
        );
        let mut set = InterpolatorSet::default();
        set.slots[0].enabled = true;
        assert_eq!(
            set.tick(&mut backend, 1),
            Err(TickError::MissingAnimationSet { slot: 0 })
        );
    }

    #[test]
    fn duration_calls_reload_current_but_preserve_pre_call_wrap_sign() {
        let mut set = set_for("duration_mutation");
        let mut backend = Backend::new("duration_mutation");
        set.tick(&mut backend, 40).unwrap();
        assert_eq!(backend.durations, vec![1, 1]);
        assert_eq!(set.slots[0].previous, 400);
        // First duration changes current500, making it outside; second changes
        // it to-50, but the wrapping direction was captured while nonnegative.
        assert_eq!(set.slots[0].current, -450);
    }

    #[test]
    fn all_eight_enabled_slots_advance_before_first_evaluator() {
        let mut set = InterpolatorSet {
            slots: [Slot {
                enabled: true,
                animset: Some(AnimSetId(1)),
                mode: 2,
                ..Default::default()
            }; 8],
            evaluators: EvaluatorView {
                allocation: AllocationId(1),
                count: 2,
            },
            ..Default::default()
        };
        let mut backend = Backend::new("advance_all_before_callbacks");
        set.tick(&mut backend, 7).unwrap();
        assert!(set.slots.iter().all(|slot| slot.previous == 7));
    }
}
