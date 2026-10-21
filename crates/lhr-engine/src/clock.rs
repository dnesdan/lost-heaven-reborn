//! Shared engine clock behavior reconstructed from a bounded original-PC oracle.
//!
//! Contract `PC-CLOCK-ORACLE-STEP2-20261021` targets `LS3DF.dll` SHA-256
//! `e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6`:
//! Original PS2 symbols and the PC address crosswalk identify the methods as
//! `NullTimer` at RVA `0x720f0` and `GetTimer` at RVA `0x72100`. Native calls
//! exercised 20 controlled cases under Wine/QEMU; they verify method arithmetic
//! and dependency interactions, not real scheduler timing or whole-game behavior.
//! This clock does not assume a 60 Hz tick rate.

/// Supplies the two operating-system effects used by the original clock methods.
///
/// A backend can be shared by a single [`EngineClock`] and can also be passed
/// through a trait object. The standard backend models the original
/// `NullTimer` behavior: sample the counter and replace the shared sample.
/// Custom IGraph overrides are outside this contract. The usual platform
/// implementation should map `sample_ms` to an unsigned millisecond counter
/// and `sleep_ms` to the platform's sleep call.
pub trait ClockBackend {
    /// Return the current unsigned millisecond sample.
    fn sample_ms(&mut self) -> u32;

    /// Request a sleep for the given unsigned millisecond duration.
    fn sleep_ms(&mut self, milliseconds: u32);
}

/// One timer sample shared by serialized engine callers.
///
/// The original methods used shared global state. Keep one instance at the
/// engine level and pass it to callers; do not create a separate clock per
/// actor. Calls are expected to be serialized on one engine thread. This type
/// does not model concurrent or reentrant access to the native global. The
/// explicit constructor sample is an adapter policy, since native constructor
/// semantics have not been established.
#[derive(Debug, PartialEq, Eq)]
pub struct EngineClock {
    last_sample: u32,
}

impl EngineClock {
    /// Create the shared clock with an explicit initial backend sample.
    pub const fn from_sample(initial_sample: u32) -> Self {
        Self {
            last_sample: initial_sample,
        }
    }

    /// Return the currently stored shared sample.
    pub const fn last_sample(&self) -> u32 {
        self.last_sample
    }

    /// Replace the stored sample with one fresh backend sample.
    ///
    /// This corresponds to the original `NullTimer` method at RVA `0x720f0`.
    pub fn null_timer<B: ClockBackend + ?Sized>(&mut self, backend: &mut B) {
        self.last_sample = backend.sample_ms();
    }

    /// Sample elapsed unsigned milliseconds, applying the original minimum
    /// sleep branch and optional maximum clamp.
    ///
    /// This corresponds to `GetTimer` at RVA `0x72100`. It samples exactly
    /// once. When elapsed is below `minimum`, it sleeps once for the difference,
    /// advances the stored sample by the requested duration with wrapping
    /// arithmetic, and returns `minimum` immediately. Sleep overshoot is not
    /// sampled again. Otherwise, a nonzero `maximum` caps only the returned
    /// value; the stored sample remains the fresh backend sample.
    pub fn get_timer<B: ClockBackend + ?Sized>(
        &mut self,
        minimum: u32,
        maximum: u32,
        backend: &mut B,
    ) -> u32 {
        let previous = self.last_sample;
        self.null_timer(backend);
        let elapsed = self.last_sample.wrapping_sub(previous);

        if elapsed < minimum {
            let requested_sleep = minimum - elapsed;
            backend.sleep_ms(requested_sleep);
            self.last_sample = self.last_sample.wrapping_add(requested_sleep);
            return minimum;
        }

        if maximum > 0 && elapsed > maximum {
            maximum
        } else {
            elapsed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ClockBackend, EngineClock};

    #[derive(Debug, PartialEq, Eq)]
    enum Event {
        Sample,
        Sleep(u32),
    }

    #[derive(Debug)]
    struct ControlledBackend {
        sample: u32,
        overshoot: u32,
        wall: u32,
        events: Vec<Event>,
    }

    impl ControlledBackend {
        fn new(sample: u32, overshoot: u32) -> Self {
            Self {
                sample,
                overshoot,
                wall: sample,
                events: Vec::new(),
            }
        }
    }

    impl ClockBackend for ControlledBackend {
        fn sample_ms(&mut self) -> u32 {
            self.events.push(Event::Sample);
            self.wall = self.sample;
            self.sample
        }

        fn sleep_ms(&mut self, milliseconds: u32) {
            self.events.push(Event::Sleep(milliseconds));
            self.wall = self
                .wall
                .wrapping_add(milliseconds)
                .wrapping_add(self.overshoot);
        }
    }

    // Literal expected outputs copied from the private native oracle's
    // public-safe synthetic vectors. Keeping expected values explicit makes
    // these differential checks independent of the Rust implementation.
    struct Vector {
        name: &'static str,
        previous: u32,
        sample: u32,
        minimum: u32,
        maximum: u32,
        overshoot: u32,
        returned: u32,
        global_after: u32,
        wall_after: u32,
        events: &'static [Event],
    }

    const VECTORS: [Vector; 16] = [
        Vector {
            name: "equal_zero_unbounded",
            previous: 100,
            sample: 100,
            minimum: 0,
            maximum: 0,
            overshoot: 0,
            returned: 0,
            global_after: 100,
            wall_after: 100,
            events: &[Event::Sample],
        },
        Vector {
            name: "unclamped",
            previous: 100,
            sample: 130,
            minimum: 16,
            maximum: 83,
            overshoot: 0,
            returned: 30,
            global_after: 130,
            wall_after: 130,
            events: &[Event::Sample],
        },
        Vector {
            name: "maximum_equal",
            previous: 100,
            sample: 183,
            minimum: 16,
            maximum: 83,
            overshoot: 0,
            returned: 83,
            global_after: 183,
            wall_after: 183,
            events: &[Event::Sample],
        },
        Vector {
            name: "maximum_clamp",
            previous: 100,
            sample: 1029,
            minimum: 16,
            maximum: 83,
            overshoot: 0,
            returned: 83,
            global_after: 1029,
            wall_after: 1029,
            events: &[Event::Sample],
        },
        Vector {
            name: "maximum_disabled",
            previous: 100,
            sample: 1029,
            minimum: 16,
            maximum: 0,
            overshoot: 0,
            returned: 929,
            global_after: 1029,
            wall_after: 1029,
            events: &[Event::Sample],
        },
        Vector {
            name: "minimum_equal",
            previous: 100,
            sample: 116,
            minimum: 16,
            maximum: 83,
            overshoot: 0,
            returned: 16,
            global_after: 116,
            wall_after: 116,
            events: &[Event::Sample],
        },
        Vector {
            name: "minimum_sleep",
            previous: 100,
            sample: 105,
            minimum: 16,
            maximum: 83,
            overshoot: 0,
            returned: 16,
            global_after: 116,
            wall_after: 116,
            events: &[Event::Sample, Event::Sleep(11)],
        },
        Vector {
            name: "sleep_overshoot_not_resampled",
            previous: 100,
            sample: 105,
            minimum: 16,
            maximum: 83,
            overshoot: 9,
            returned: 16,
            global_after: 116,
            wall_after: 125,
            events: &[Event::Sample, Event::Sleep(11)],
        },
        Vector {
            name: "minimum_above_maximum",
            previous: 100,
            sample: 105,
            minimum: 40,
            maximum: 10,
            overshoot: 0,
            returned: 40,
            global_after: 140,
            wall_after: 140,
            events: &[Event::Sample, Event::Sleep(35)],
        },
        Vector {
            name: "wrap_elapsed",
            previous: u32::MAX - 15,
            sample: 16,
            minimum: 16,
            maximum: 83,
            overshoot: 0,
            returned: 32,
            global_after: 16,
            wall_after: 16,
            events: &[Event::Sample],
        },
        Vector {
            name: "wrap_sleep_addition",
            previous: u32::MAX - 31,
            sample: u32::MAX - 15,
            minimum: 32,
            maximum: 83,
            overshoot: 0,
            returned: 32,
            global_after: 0,
            wall_after: 0,
            events: &[Event::Sample, Event::Sleep(16)],
        },
        Vector {
            name: "unsigned_backward_sample_clamped",
            previous: 100,
            sample: 99,
            minimum: 0,
            maximum: 83,
            overshoot: 0,
            returned: 83,
            global_after: 99,
            wall_after: 99,
            events: &[Event::Sample],
        },
        Vector {
            name: "unsigned_backward_sample_unbounded",
            previous: 100,
            sample: 99,
            minimum: 0,
            maximum: 0,
            overshoot: 0,
            returned: u32::MAX,
            global_after: 99,
            wall_after: 99,
            events: &[Event::Sample],
        },
        Vector {
            name: "unsigned_large_minimum",
            previous: 0,
            sample: 1,
            minimum: 0x8000_0000,
            maximum: 1,
            overshoot: 0,
            returned: 0x8000_0000,
            global_after: 0x8000_0000,
            wall_after: 0x8000_0000,
            events: &[Event::Sample, Event::Sleep(0x7fff_ffff)],
        },
        Vector {
            name: "unsigned_large_maximum",
            previous: 0,
            sample: 0x8000_0000,
            minimum: 0,
            maximum: u32::MAX,
            overshoot: 0,
            returned: 0x8000_0000,
            global_after: 0x8000_0000,
            wall_after: 0x8000_0000,
            events: &[Event::Sample],
        },
        Vector {
            name: "maximum_one",
            previous: 0,
            sample: u32::MAX,
            minimum: 0,
            maximum: 1,
            overshoot: 0,
            returned: 1,
            global_after: u32::MAX,
            wall_after: u32::MAX,
            events: &[Event::Sample],
        },
    ];

    #[test]
    fn matches_sixteen_native_oracle_vectors_and_event_order() {
        for vector in &VECTORS {
            let mut clock = EngineClock::from_sample(vector.previous);
            let mut backend = ControlledBackend::new(vector.sample, vector.overshoot);

            let returned = clock.get_timer(vector.minimum, vector.maximum, &mut backend);

            assert_eq!(returned, vector.returned, "{} return", vector.name);
            assert_eq!(
                clock.last_sample(),
                vector.global_after,
                "{} stored sample",
                vector.name
            );
            assert_eq!(
                backend.wall, vector.wall_after,
                "{} simulated wall",
                vector.name
            );
            assert_eq!(
                backend.events, vector.events,
                "{} backend event order",
                vector.name
            );
        }
    }

    #[test]
    fn null_timer_samples_once_and_replaces_shared_state() {
        let mut clock = EngineClock::from_sample(999);
        let mut backend = ControlledBackend::new(7, 0);

        clock.null_timer(&mut backend);

        assert_eq!(clock.last_sample(), 7);
        assert_eq!(backend.events, [Event::Sample]);
    }

    #[test]
    fn two_callers_reuse_one_shared_clock_sample() {
        let mut clock = EngineClock::from_sample(10);
        let mut first_backend = ControlledBackend::new(20, 0);
        let mut second_backend = ControlledBackend::new(25, 0);

        let first = clock.get_timer(0, 0, &mut first_backend);
        let second = clock.get_timer(0, 0, &mut second_backend);

        assert_eq!((first, second, clock.last_sample()), (10, 5, 25));
        assert_eq!(first_backend.events, [Event::Sample]);
        assert_eq!(second_backend.events, [Event::Sample]);
    }

    #[test]
    fn reset_and_delta_share_state_across_callers() {
        let mut clock = EngineClock::from_sample(900);
        let mut reset_backend = ControlledBackend::new(7, 0);
        clock.null_timer(&mut reset_backend);

        let mut delta_backend = ControlledBackend::new(7, 0);
        let returned = clock.get_timer(0, 0, &mut delta_backend);

        assert_eq!(returned, 0);
        assert_eq!(clock.last_sample(), 7);
        assert_eq!(reset_backend.events, [Event::Sample]);
        assert_eq!(delta_backend.events, [Event::Sample]);
    }

    #[test]
    fn backend_trait_dispatch_is_supported() {
        let mut clock = EngineClock::from_sample(90);
        let mut backend: Box<dyn ClockBackend> = Box::new(ControlledBackend::new(100, 0));

        let returned = clock.get_timer(0, 0, backend.as_mut());

        assert_eq!(returned, 10);
        assert_eq!(clock.last_sample(), 100);
    }
}
