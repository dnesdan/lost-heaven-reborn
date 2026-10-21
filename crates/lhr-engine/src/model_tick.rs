//! Bounded PC model tick dispatch (`PC-MODEL-TICK-20261021`).
//!
//! Original LS3DF SHA-256:
//! `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`;
//! canonical entry RVA `0x34010`. Eleven controlled calls to the unchanged
//! original DLL exercised guards, callback mutation and reentry. The final
//! virtual update remains a required backend operation, not a pose-update stub.

use crate::scene_tick::InterpolatorId;

/// Nonowning identity of the model's current virtual table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ModelVTableId(pub u32);

/// Only fields observed by the recovered model tick wrapper.
#[derive(Debug, Eq, PartialEq)]
pub struct ModelTickState {
    pub vtable: ModelVTableId,
    pub interpolator: Option<InterpolatorId>,
}

/// Resolves the captured interpolator and implements the two required calls.
///
/// `evaluator_count` is a pure field read. Callbacks may mutate the model or
/// synchronously reenter `model.tick(self, delta)`; no field borrow spans a call.
/// An unavailable interpolator or final update must return an error. The final
/// call represents virtual slot `+0x18`, which takes the model and no delta.
pub trait ModelBackend {
    type Error;

    fn evaluator_count(&self, interpolator: InterpolatorId) -> Result<u32, Self::Error>;

    fn tick_interpolator(
        &mut self,
        model: &mut ModelTickState,
        interpolator: InterpolatorId,
        delta: i32,
    ) -> Result<(), Self::Error>;

    fn update_model(
        &mut self,
        model: &mut ModelTickState,
        vtable: ModelVTableId,
    ) -> Result<(), Self::Error>;
}

impl ModelTickState {
    /// Construct this view of a fresh model: the original constructor leaves
    /// its interpolator pointer null. Loading/binding is a separate operation.
    pub fn new(vtable: ModelVTableId) -> Self {
        Self {
            vtable,
            interpolator: None,
        }
    }

    /// Tick only a nonnull interpolator with a nonzero evaluator count, then
    /// invoke the model's live virtual update. Guards are read once: callbacks
    /// changing the pointer/count do not suppress the final update. A zero delta
    /// or disabled slots do not skip it when the entry guards passed.
    pub fn tick<B: ModelBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        delta: i32,
    ) -> Result<(), B::Error> {
        let Some(interpolator) = self.interpolator else {
            return Ok(());
        };
        if backend.evaluator_count(interpolator)? == 0 {
            return Ok(());
        }
        backend.tick_interpolator(self, interpolator, delta)?;
        let vtable = self.vtable;
        backend.update_model(self, vtable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, Eq, PartialEq)]
    enum Call {
        Tick(InterpolatorId, i32),
        Update(ModelVTableId),
    }

    #[derive(Default)]
    struct Backend {
        count: u32,
        reads: Cell<u32>,
        mutate: bool,
        reenter: bool,
        update_reenter: bool,
        update_swap: bool,
        calls: Vec<Call>,
        unavailable: bool,
    }

    impl ModelBackend for Backend {
        type Error = &'static str;

        fn evaluator_count(&self, _: InterpolatorId) -> Result<u32, Self::Error> {
            self.reads.set(self.reads.get() + 1);
            Ok(self.count)
        }

        fn tick_interpolator(
            &mut self,
            model: &mut ModelTickState,
            interpolator: InterpolatorId,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.calls.push(Call::Tick(interpolator, delta));
            if self.mutate {
                self.mutate = false;
                model.interpolator = None;
                self.count = 0;
                model.vtable = ModelVTableId(2);
            }
            if self.reenter {
                self.reenter = false;
                model.tick(self, 1)?;
            }
            Ok(())
        }

        fn update_model(
            &mut self,
            model: &mut ModelTickState,
            table: ModelVTableId,
        ) -> Result<(), Self::Error> {
            self.calls.push(Call::Update(table));
            if self.update_reenter {
                self.update_reenter = false;
                model.tick(self, 3)?;
            }
            if self.update_swap {
                model.vtable = ModelVTableId(2);
            }
            if self.unavailable {
                Err("update unavailable")
            } else {
                Ok(())
            }
        }
    }

    fn bound() -> ModelTickState {
        ModelTickState {
            vtable: ModelVTableId(1),
            interpolator: Some(InterpolatorId(7)),
        }
    }

    #[test]
    fn fresh_and_empty_models_skip_both_calls() {
        let mut backend = Backend::default();
        ModelTickState::new(ModelVTableId(1))
            .tick(&mut backend, 7)
            .unwrap();
        assert_eq!(backend.reads.get(), 0);
        bound().tick(&mut backend, 7).unwrap();
        assert_eq!(backend.reads.get(), 1);
        assert!(backend.calls.is_empty());
    }

    #[test]
    fn nonempty_zero_and_negative_delta_reach_update() {
        for delta in [0, -7, i32::MIN] {
            let mut backend = Backend {
                count: 1,
                ..Backend::default()
            };
            bound().tick(&mut backend, delta).unwrap();
            assert_eq!(
                backend.calls,
                [
                    Call::Tick(InterpolatorId(7), delta),
                    Call::Update(ModelVTableId(1))
                ]
            );
        }
    }

    #[test]
    fn callback_mutations_do_not_repeat_guards_and_reload_vtable() {
        let mut model = bound();
        let mut backend = Backend {
            count: 1,
            mutate: true,
            ..Backend::default()
        };
        model.tick(&mut backend, 40).unwrap();
        assert_eq!(model.interpolator, None);
        assert_eq!(backend.reads.get(), 1);
        assert_eq!(
            backend.calls,
            [
                Call::Tick(InterpolatorId(7), 40),
                Call::Update(ModelVTableId(2))
            ]
        );
    }

    #[test]
    fn synchronous_reentry_completes_before_outer_update() {
        let mut backend = Backend {
            count: 1,
            reenter: true,
            ..Backend::default()
        };
        bound().tick(&mut backend, 40).unwrap();
        assert_eq!(
            backend.calls,
            [
                Call::Tick(InterpolatorId(7), 40),
                Call::Tick(InterpolatorId(7), 1),
                Call::Update(ModelVTableId(1)),
                Call::Update(ModelVTableId(1))
            ]
        );
    }

    #[test]
    fn final_update_can_reenter_before_outer_return() {
        let mut backend = Backend {
            count: 1,
            update_reenter: true,
            ..Backend::default()
        };
        bound().tick(&mut backend, 7).unwrap();
        assert_eq!(
            backend.calls,
            [
                Call::Tick(InterpolatorId(7), 7),
                Call::Update(ModelVTableId(1)),
                Call::Tick(InterpolatorId(7), 3),
                Call::Update(ModelVTableId(1))
            ]
        );
    }

    #[test]
    fn final_update_vtable_mutation_is_visible_next_call() {
        let mut backend = Backend {
            count: 1,
            update_swap: true,
            ..Backend::default()
        };
        let mut model = bound();
        model.tick(&mut backend, 7).unwrap();
        model.tick(&mut backend, 3).unwrap();
        assert_eq!(
            backend.calls,
            [
                Call::Tick(InterpolatorId(7), 7),
                Call::Update(ModelVTableId(1)),
                Call::Tick(InterpolatorId(7), 3),
                Call::Update(ModelVTableId(2))
            ]
        );
    }

    #[test]
    fn missing_update_is_reported() {
        let mut backend = Backend {
            count: 1,
            unavailable: true,
            ..Backend::default()
        };
        assert_eq!(bound().tick(&mut backend, 7), Err("update unavailable"));
    }
}
