//! Local, nonowning scene callback dispatch, independent of rendering and Game phases.
//!
//! Original LS3DF SHA-256:
//! `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`;
//! entry RVA `0x4a0e0`, evidence `PC-SCENE-ORACLE-STEP3-20261021`.
//! The safe adapter accepts valid logical collections, not corrupt native counts or
//! stale backing-array reads. It does not implement the interpolator or actor lifetime.

/// Nonowning frame identity; equality corresponds to native pointer equality.
/// Reusing the same identity does not create a new generation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct FrameId(pub u32);

/// Nonowning interpolator identity, resolved and owned by the callback backend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct InterpolatorId(pub u32);

/// Owns the live collection of identities, not the represented objects.
#[derive(Debug, Default)]
pub struct LocalScene {
    frames: Vec<FrameId>,
    interpolator: Option<InterpolatorId>,
}

/// Owns objects and callback behavior. A callback may mutate the scene, immediately
/// destroy a removed object, or synchronously call `scene.tick(self, delta)`.
/// Implementations must not retain an object borrow across mutation or reentry.
pub trait SceneBackend {
    fn tick_frame(&mut self, scene: &mut LocalScene, frame: FrameId, delta: i32);
    fn tick_interpolator(
        &mut self,
        scene: &mut LocalScene,
        interpolator: InterpolatorId,
        delta: i32,
    );
}

impl LocalScene {
    pub fn new(frames: Vec<FrameId>, interpolator: Option<InterpolatorId>) -> Self {
        Self {
            frames,
            interpolator,
        }
    }

    pub fn frames(&self) -> &[FrameId] {
        &self.frames
    }

    pub fn frames_mut(&mut self) -> &mut Vec<FrameId> {
        &mut self.frames
    }

    pub fn interpolator(&self) -> Option<InterpolatorId> {
        self.interpolator
    }

    pub fn set_interpolator(&mut self, interpolator: Option<InterpolatorId>) {
        self.interpolator = interpolator;
    }

    /// Dispatch the live collection, retrying an index when its identity changes.
    /// There is intentionally no visited set, mutation budget or deferred callback
    /// queue: callbacks that continually rearrange the list may not terminate.
    pub fn tick<B: SceneBackend + ?Sized>(&mut self, backend: &mut B, delta: i32) {
        let mut index = 0;
        while let Some(frame) = self.frames.get(index).copied() {
            backend.tick_frame(self, frame, delta);
            match self.frames.get(index) {
                Some(current) if *current == frame => index += 1,
                Some(_) => {}
                // Native backing storage can outlive its logical count. Avoid its
                // stale/OOB read when the callback shrinks the logical collection.
                None => break,
            }
        }
        // Read after all frame callbacks and any synchronous nested ticks.
        if let Some(interpolator) = self.interpolator {
            backend.tick_interpolator(self, interpolator, delta);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, collections::HashMap, rc::Rc};

    #[derive(Clone, Copy, Debug, PartialEq)]
    enum Action {
        Stable,
        RemoveCurrent,
        RemoveFuture,
        Append,
        Prepend,
        Swap,
        Replace,
        ReplaceVector,
        Clear,
        Recursive,
        ReplaceInterpolator,
        NullInterpolator,
        FreeCurrent,
        ReuseIdentity,
        RemoveLast,
        ClearInterpolator,
    }

    #[derive(Debug, PartialEq)]
    enum Event {
        Frame(u32, u32, i32, i32), // identity, reentry depth, delta, original IPol time
        Interpolator(u32, u32, i32, usize, usize), // identity, depth, delta, calls, freed
    }

    struct Object(Rc<Cell<usize>>);
    impl Drop for Object {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    // Test-only counter/observer, not an IPol implementation. Its job is to expose
    // dispatch phase ordering and synchronous reentry against native observations.
    struct Backend {
        action: Action,
        acted: bool,
        depth: u32,
        frame_calls: usize,
        times: [i32; 2],
        trace: Vec<Event>,
        objects: HashMap<FrameId, Object>,
        freed: Rc<Cell<usize>>,
    }

    impl Backend {
        fn new(action: Action) -> Self {
            let freed = Rc::new(Cell::new(0));
            Self {
                action,
                acted: false,
                depth: 0,
                frame_calls: 0,
                times: [0; 2],
                trace: Vec::new(),
                objects: (1..=5)
                    .map(|id| (FrameId(id), Object(freed.clone())))
                    .collect(),
                freed,
            }
        }
    }

    impl SceneBackend for Backend {
        fn tick_frame(&mut self, scene: &mut LocalScene, frame: FrameId, delta: i32) {
            assert!(
                self.objects.contains_key(&frame),
                "must not revisit a freed object"
            );
            self.frame_calls += 1;
            self.trace
                .push(Event::Frame(frame.0, self.depth, delta, self.times[0]));
            if self.acted || (frame != FrameId(1) && self.action != Action::RemoveLast) {
                return;
            }
            if self.action == Action::RemoveLast && frame != FrameId(3) {
                return;
            }
            self.acted = true;
            match self.action {
                Action::Stable | Action::NullInterpolator => {}
                Action::RemoveCurrent | Action::FreeCurrent => {
                    scene.frames_mut().remove(0);
                    if self.action == Action::FreeCurrent {
                        drop(self.objects.remove(&frame));
                        assert_eq!(self.freed.get(), 1);
                    }
                }
                Action::RemoveFuture => {
                    scene.frames_mut().remove(1);
                }
                Action::Append => scene.frames_mut().push(FrameId(4)),
                Action::Prepend => scene.frames_mut().insert(0, FrameId(4)),
                Action::Swap => scene.frames_mut().swap(0, 1),
                Action::Replace => scene.frames_mut()[0] = FrameId(4),
                Action::ReplaceVector => *scene.frames_mut() = vec![FrameId(4), FrameId(5)],
                Action::Clear => scene.frames_mut().clear(),
                Action::Recursive => {
                    self.depth += 1;
                    scene.tick(self, 3);
                    self.depth -= 1;
                }
                Action::ReplaceInterpolator => scene.set_interpolator(Some(InterpolatorId(1))),
                Action::ReuseIdentity => {
                    drop(self.objects.remove(&frame));
                    self.objects.insert(frame, Object(self.freed.clone()));
                    scene.frames_mut()[0] = frame;
                }
                Action::RemoveLast => {
                    scene.frames_mut().pop();
                }
                Action::ClearInterpolator => scene.set_interpolator(None),
            }
        }

        fn tick_interpolator(
            &mut self,
            _: &mut LocalScene,
            interpolator: InterpolatorId,
            delta: i32,
        ) {
            self.trace.push(Event::Interpolator(
                interpolator.0,
                self.depth,
                delta,
                self.frame_calls,
                self.freed.get(),
            ));
            self.times[interpolator.0 as usize] += delta;
        }
    }

    #[test]
    fn thirteen_native_scene_dispatch_cases() {
        use Action::*;
        // Literal scene-dispatch projection of the unchanged native DLL oracle.
        // Eight other vectors concern IPol scalar state/interface behavior and are
        // deliberately not counted as Scene implementation comparisons here.
        let cases: [(Action, &[u32], usize, usize); 13] = [
            (Stable, &[1, 2, 3], 3, 0),
            (RemoveCurrent, &[1, 2, 3], 2, 0),
            (RemoveFuture, &[1, 3], 2, 0),
            (Append, &[1, 2, 3, 4], 4, 0),
            (Prepend, &[1, 4, 1, 2, 3], 4, 0),
            (Swap, &[1, 2, 1, 3], 3, 0),
            (Replace, &[1, 4, 2, 3], 3, 0),
            (ReplaceVector, &[1, 4, 5], 2, 0),
            (Clear, &[1], 0, 0),
            (Recursive, &[1, 1, 2, 3, 2, 3], 3, 0),
            (ReplaceInterpolator, &[1, 2, 3], 3, 0),
            (NullInterpolator, &[1, 2, 3], 3, 0),
            (FreeCurrent, &[1, 2, 3], 2, 1),
        ];
        for (action, ids, count_after, freed) in cases {
            let interpolator = (action != NullInterpolator).then_some(InterpolatorId(0));
            let mut scene = LocalScene::new(vec![FrameId(1), FrameId(2), FrameId(3)], interpolator);
            let mut backend = Backend::new(action);
            scene.tick(&mut backend, 7);
            let expected = if action == Recursive {
                vec![
                    Event::Frame(1, 0, 7, 0),
                    Event::Frame(1, 1, 3, 0),
                    Event::Frame(2, 1, 3, 0),
                    Event::Frame(3, 1, 3, 0),
                    Event::Interpolator(0, 1, 3, 4, 0),
                    Event::Frame(2, 0, 7, 3),
                    Event::Frame(3, 0, 7, 3),
                    Event::Interpolator(0, 0, 7, 6, 0),
                ]
            } else {
                let mut events: Vec<_> = ids.iter().map(|id| Event::Frame(*id, 0, 7, 0)).collect();
                if action != NullInterpolator {
                    events.push(Event::Interpolator(
                        u32::from(action == ReplaceInterpolator),
                        0,
                        7,
                        ids.len(),
                        freed,
                    ));
                }
                events
            };
            assert_eq!(backend.trace, expected, "{action:?}");
            assert_eq!(scene.frames().len(), count_after, "{action:?}");
            assert_eq!(backend.freed.get(), freed, "{action:?}");
            let times = match action {
                Recursive => [10, 0],
                ReplaceInterpolator => [0, 7],
                NullInterpolator => [0, 0],
                _ => [7, 0],
            };
            assert_eq!(backend.times, times, "{action:?}");
        }
    }

    #[test]
    fn same_identity_replacement_advances_without_a_generation() {
        let mut scene = LocalScene::new(
            vec![FrameId(1), FrameId(2), FrameId(3)],
            Some(InterpolatorId(0)),
        );
        let mut backend = Backend::new(Action::ReuseIdentity);
        scene.tick(&mut backend, 7);
        assert_eq!(
            backend.trace,
            vec![
                Event::Frame(1, 0, 7, 0),
                Event::Frame(2, 0, 7, 0),
                Event::Frame(3, 0, 7, 0),
                Event::Interpolator(0, 0, 7, 3, 1)
            ]
        );
    }

    #[test]
    fn removing_final_current_index_is_safe() {
        let mut scene = LocalScene::new(vec![FrameId(1), FrameId(2), FrameId(3)], None);
        let mut backend = Backend::new(Action::RemoveLast);
        scene.tick(&mut backend, 7);
        assert_eq!(scene.frames(), &[FrameId(1), FrameId(2)]);
        assert_eq!(backend.frame_calls, 3);
    }

    #[test]
    fn clearing_interpolator_in_callback_skips_phase() {
        let mut scene = LocalScene::new(vec![FrameId(1)], Some(InterpolatorId(0)));
        let mut backend = Backend::new(Action::ClearInterpolator);
        scene.tick(&mut backend, 7);
        assert_eq!(scene.interpolator(), None);
        assert_eq!(backend.trace, vec![Event::Frame(1, 0, 7, 0)]);
    }

    #[test]
    fn empty_scene_and_signed_delta_forwarding() {
        for delta in [0, -7, i32::MIN, i32::MAX] {
            let mut scene = LocalScene::new(vec![], Some(InterpolatorId(0)));
            let mut backend = Backend::new(Action::Stable);
            scene.tick(&mut backend, delta);
            assert_eq!(backend.trace, vec![Event::Interpolator(0, 0, delta, 0, 0)]);
            assert_eq!(backend.times[0], delta);
        }
        let mut scene = LocalScene::default();
        let mut backend = Backend::new(Action::Stable);
        scene.tick(&mut backend, 7);
        assert!(backend.trace.is_empty());
    }
}
