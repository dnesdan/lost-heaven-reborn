//! Recovered Game context dispatch (RVA0x1407e0), separate from mission initialization.
//! Original SHA `08398317c8dc423dbb3e559f0ba53c207a8eaf5bcefa915dff83b7a86416ab27`.
//! Objects and physical record storage belong to the backend. Invalid/freed
//! storage is an explicit adapter error; native invalid memory is not emulated.
//! Floating exception/status flags are outside the signed16 alpha-ratio contract.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ObjectId(pub u32);
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct ModelAllocationId(pub u32);
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TimedAllocationId(pub u32);
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelView {
    pub allocation: ModelAllocationId,
    pub count: usize,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TimedView {
    pub allocation: TimedAllocationId,
    pub count: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ModelRecord {
    pub object: ObjectId,
    pub opaque: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimedRecord {
    pub object: ObjectId,
    pub elapsed: u16,
    pub duration: u16,
}

/// Explicit host state, not a recovered Game context constructor.
#[derive(Debug)]
pub struct FrameState {
    pub pending_flag: u8,
    pub pending_counter: i32,
    pub pending_target: Option<ObjectId>,
    pub pending_argument: u32,
    pub context_object: Option<ObjectId>,
    pub scene: Option<ObjectId>,
    pub manager: Option<ObjectId>,
    pub models: ModelView,
    pub mission: Option<ObjectId>,
    pub timed: TimedView,
    /// Adapter's live x87 control policy; callbacks that change it update this.
    pub control_word: u16,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DispatchError<E> {
    Backend(E),
    MissingManager,
    EmptyTimedAfterCallback,
    UnsupportedFloatPolicy,
}

/// Storage lookup/write methods are field accesses, not callbacks. Physical
/// allocation bounds are distinct from logical view counts. The backend must
/// reject unreadable/freed storage; replacing a view never implies deferred free.
/// Callback methods can change state/storage and synchronously reenter dispatch.
/// No default successful no-op dependencies are supplied.
pub trait FrameBackend {
    type Error;
    fn model_at(
        &self,
        allocation: ModelAllocationId,
        index: usize,
    ) -> Result<ModelRecord, Self::Error>;
    fn timed_at(
        &self,
        allocation: TimedAllocationId,
        index: usize,
    ) -> Result<TimedRecord, Self::Error>;
    fn write_timed(
        &mut self,
        allocation: TimedAllocationId,
        index: usize,
        record: TimedRecord,
    ) -> Result<(), Self::Error>;
    fn pending(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        argument: u32,
    ) -> Result<(), Self::Error>;
    fn context_tick(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error>;
    fn scene_tick(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error>;
    fn manager_tick(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error>;
    fn model_tick(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error>;
    fn mission_enabled(&self, object: ObjectId) -> Result<bool, Self::Error>;
    fn mission_tick(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        delta: i32,
    ) -> Result<(), Self::Error>;
    fn set_alpha(
        &mut self,
        state: &mut FrameState,
        object: ObjectId,
        alpha_bits: u32,
    ) -> Result<(), Self::Error>;
    fn release(&mut self, state: &mut FrameState, object: ObjectId) -> Result<(), Self::Error>;
}

impl FrameState {
    pub fn tick<B: FrameBackend + ?Sized>(
        &mut self,
        backend: &mut B,
        delta: i32,
    ) -> Result<(), DispatchError<B::Error>> {
        if self.pending_flag != 0 && self.pending_counter < 1 {
            self.pending_flag = 0;
            if let Some(object) = self.pending_target {
                backend
                    .pending(self, object, self.pending_argument)
                    .map_err(DispatchError::Backend)?;
            }
        }
        if let Some(object) = self.context_object {
            backend
                .context_tick(self, object, delta)
                .map_err(DispatchError::Backend)?;
        }
        if let Some(object) = self.scene {
            backend
                .scene_tick(self, object, delta)
                .map_err(DispatchError::Backend)?;
        }
        let manager = self.manager.ok_or(DispatchError::MissingManager)?;
        backend
            .manager_tick(self, manager, delta)
            .map_err(DispatchError::Backend)?;
        let initial_count = self.models.count;
        for index in (0..initial_count).rev() {
            let record = backend
                .model_at(self.models.allocation, index)
                .map_err(DispatchError::Backend)?;
            backend
                .model_tick(self, record.object, delta)
                .map_err(DispatchError::Backend)?;
        }
        if let Some(object) = self.mission {
            if backend
                .mission_enabled(object)
                .map_err(DispatchError::Backend)?
            {
                backend
                    .mission_tick(self, object, delta)
                    .map_err(DispatchError::Backend)?;
            }
        }
        let mut index = 0;
        while index < self.timed.count {
            let mut record = backend
                .timed_at(self.timed.allocation, index)
                .map_err(DispatchError::Backend)?;
            record.elapsed = record.elapsed.wrapping_add(delta as u16);
            backend
                .write_timed(self.timed.allocation, index, record)
                .map_err(DispatchError::Backend)?;
            let record = backend
                .timed_at(self.timed.allocation, index)
                .map_err(DispatchError::Backend)?;
            if (record.elapsed as i16) >= (record.duration as i16) {
                backend
                    .set_alpha(self, record.object, 0x3f800000)
                    .map_err(DispatchError::Backend)?;
                // Alpha may replace the object that is about to be released.
                let current = backend
                    .timed_at(self.timed.allocation, index)
                    .map_err(DispatchError::Backend)?;
                backend
                    .release(self, current.object)
                    .map_err(DispatchError::Backend)?;
                let last = self
                    .timed
                    .count
                    .checked_sub(1)
                    .ok_or(DispatchError::EmptyTimedAfterCallback)?;
                let record = backend
                    .timed_at(self.timed.allocation, last)
                    .map_err(DispatchError::Backend)?;
                backend
                    .write_timed(self.timed.allocation, index, record)
                    .map_err(DispatchError::Backend)?;
                self.timed.count -= 1;
                // The replacement at this index participates immediately.
            } else {
                let bits = ratio_bits(
                    record.elapsed as i16,
                    record.duration as i16,
                    self.control_word,
                )
                .ok_or(DispatchError::UnsupportedFloatPolicy)?;
                backend
                    .set_alpha(self, record.object, bits)
                    .map_err(DispatchError::Backend)?;
                index += 1;
            }
        }
        Ok(())
    }
}

fn rounded(
    quotient: u128,
    remainder: u128,
    denominator: u128,
    negative: bool,
    rounding: u16,
) -> u128 {
    let increment = match rounding {
        0 => remainder * 2 > denominator || (remainder * 2 == denominator && quotient & 1 != 0),
        1 => negative && remainder != 0,
        2 => !negative && remainder != 0,
        _ => false,
    };
    quotient + u128::from(increment)
}

/// Exact signed16 rational, first rounded to the selected x87 precision, then
/// to stored binary32 under the same rounding control. No host float arithmetic.
fn ratio_bits(numerator: i16, denominator: i16, word: u16) -> Option<u32> {
    if word & 0x3f != 0x3f {
        return None;
    }
    let precision = match (word >> 8) & 3 {
        0 => 24,
        2 => 53,
        3 => 64,
        _ => return None,
    };
    let rounding = (word >> 10) & 3;
    let negative = (numerator < 0) ^ (denominator < 0);
    let sign = u32::from(negative) << 31;
    let a = u128::from(numerator.unsigned_abs());
    let b = u128::from(denominator.unsigned_abs());
    if b == 0 {
        return Some(if a == 0 {
            0xffc00000
        } else {
            sign | 0x7f800000
        });
    }
    if a == 0 {
        return Some(sign);
    }
    let mut exponent = (128 - a.leading_zeros()) as i32 - (128 - b.leading_zeros()) as i32;
    let below = if exponent >= 0 {
        a < (b << exponent)
    } else {
        (a << (-exponent)) < b
    };
    if below {
        exponent -= 1;
    }
    let scaled = a << ((precision - 1) - exponent);
    let mut significand = rounded(scaled / b, scaled % b, b, negative, rounding);
    if significand == (1u128 << precision) {
        significand >>= 1;
        exponent += 1;
    }
    if precision > 24 {
        let discard = precision - 24;
        let denominator = 1u128 << discard;
        significand = rounded(
            significand >> discard,
            significand & (denominator - 1),
            denominator,
            negative,
            rounding,
        );
        if significand == 1 << 24 {
            significand >>= 1;
            exponent += 1;
        }
    }
    Some(sign | (((exponent + 127) as u32) << 23) | (significand as u32 & 0x7fffff))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    #[derive(Clone, Debug)]
    pub(crate) struct Input {
        pub mutation: u32,
        pub delta: i32,
        pub word: u16,
        pub pending: u8,
        pub counter: i32,
        pub hook: bool,
        pub auxiliary: bool,
        pub scene: bool,
        pub mission: bool,
        pub active: bool,
        pub models: Vec<u32>,
        pub timed: Vec<(u32, u32)>,
    }
    #[derive(Debug, PartialEq)]
    pub(crate) struct Output {
        pub pending: u8,
        pub counter: i32,
        pub model_count: usize,
        pub timed_count: usize,
        pub reentries: u32,
        pub models: Vec<u32>,
        pub timed: Vec<(u32, u32)>,
        pub events: Vec<[u32; 4]>,
    }
    struct Harness {
        mutation: u32,
        acted: bool,
        active: bool,
        depth: u32,
        reentries: u32,
        models: [Vec<ModelRecord>; 2],
        timed: [Vec<TimedRecord>; 2],
        events: Vec<[u32; 4]>,
    }
    impl Harness {
        fn run(
            &mut self,
            state: &mut FrameState,
            delta: i32,
        ) -> Result<(), DispatchError<&'static str>> {
            self.depth += 1;
            let result = state.tick(self, delta);
            self.depth -= 1;
            result
        }
        fn callback(
            &mut self,
            state: &mut FrameState,
            kind: u32,
            object: ObjectId,
            arg: u32,
        ) -> Result<(), &'static str> {
            self.events.push([kind, object.0, arg, self.depth]);
            let trigger = match self.mutation {
                1 | 4 | 20 => kind == 3,
                2 | 19 => kind == 1,
                3 => kind == 2,
                5 | 10 => kind == 4,
                6..=9 => kind == 5 && object.0 == 3,
                11 => kind == 5,
                12..=14 | 16..=18 => kind == 7 && object.0 == 1,
                15 => kind == 8 && object.0 == 1,
                _ => false,
            };
            if self.acted || !trigger {
                return Ok(());
            }
            self.acted = true;
            let m = (state.models.allocation.0 - 1) as usize;
            let t = (state.timed.allocation.0 - 1) as usize;
            match self.mutation {
                1 => state.manager = Some(ObjectId(203)),
                2 => {
                    state.context_object = Some(ObjectId(204));
                    state.scene = Some(ObjectId(201));
                }
                3 => state.scene = None,
                4 => {
                    self.models[1][0].object = ObjectId(3);
                    self.models[1][1].object = ObjectId(2);
                    state.models = ModelView {
                        allocation: ModelAllocationId(2),
                        count: 2,
                    };
                }
                5 | 9 => {
                    self.models[m][state.models.count].object = ObjectId(4);
                    state.models.count += 1;
                }
                6 => {
                    self.models[1] = self.models[m].clone();
                    for (i, id) in [1, 4, 2].into_iter().enumerate() {
                        self.models[1][i].object = ObjectId(id);
                    }
                    state.models.allocation = ModelAllocationId(2);
                }
                7 => {
                    self.models[m][0] = self.models[m][1];
                    self.models[m][1] = self.models[m][2];
                    state.models.count -= 1;
                }
                8 => state.models.count -= 1,
                10 => self.active = false,
                11 => self.active = true,
                12 => self.timed[t][0].object = ObjectId(2),
                13 | 15 => {
                    self.timed[1][0] = TimedRecord {
                        object: ObjectId(2),
                        elapsed: 0,
                        duration: 0,
                    };
                    self.timed[1][1] = TimedRecord {
                        object: ObjectId(3),
                        elapsed: 0,
                        duration: 1000,
                    };
                    state.timed = TimedView {
                        allocation: TimedAllocationId(2),
                        count: 2,
                    };
                }
                14 | 17 => {
                    self.timed[t][state.timed.count] = TimedRecord {
                        object: ObjectId(3),
                        elapsed: 0,
                        duration: 1000,
                    };
                    state.timed.count += 1;
                }
                16 => {
                    for i in 0..state.timed.count - 1 {
                        self.timed[t][i] = self.timed[t][i + 1];
                    }
                    state.timed.count -= 1;
                }
                18 | 19 => {
                    self.reentries += 1;
                    self.run(state, if self.mutation == 18 { 0 } else { 7 })
                        .map_err(|_| "Nested dispatch failed")?;
                }
                20 => {
                    state.pending_counter = 1;
                    state.pending_flag = 1;
                }
                _ => {}
            }
            Ok(())
        }
    }
    impl FrameBackend for Harness {
        type Error = &'static str;
        fn model_at(
            &self,
            allocation: ModelAllocationId,
            index: usize,
        ) -> Result<ModelRecord, Self::Error> {
            self.models
                .get(allocation.0.wrapping_sub(1) as usize)
                .and_then(|v| v.get(index))
                .copied()
                .ok_or("Invalid model storage")
        }
        fn timed_at(
            &self,
            allocation: TimedAllocationId,
            index: usize,
        ) -> Result<TimedRecord, Self::Error> {
            self.timed
                .get(allocation.0.wrapping_sub(1) as usize)
                .and_then(|v| v.get(index))
                .copied()
                .ok_or("Invalid timed storage")
        }
        fn write_timed(
            &mut self,
            allocation: TimedAllocationId,
            index: usize,
            record: TimedRecord,
        ) -> Result<(), Self::Error> {
            *self
                .timed
                .get_mut(allocation.0.wrapping_sub(1) as usize)
                .and_then(|v| v.get_mut(index))
                .ok_or("Invalid timed storage")? = record;
            Ok(())
        }
        fn pending(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            arg: u32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 1, id, arg)
        }
        fn context_tick(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 2, id, delta as u32)
        }
        fn scene_tick(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 3, id, delta as u32)
        }
        fn manager_tick(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 4, id, delta as u32)
        }
        fn model_tick(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 5, id, delta as u32)
        }
        fn mission_enabled(&self, _: ObjectId) -> Result<bool, Self::Error> {
            Ok(self.active)
        }
        fn mission_tick(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            delta: i32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 6, id, delta as u32)
        }
        fn set_alpha(
            &mut self,
            state: &mut FrameState,
            id: ObjectId,
            bits: u32,
        ) -> Result<(), Self::Error> {
            self.callback(state, 7, id, bits)
        }
        fn release(&mut self, state: &mut FrameState, id: ObjectId) -> Result<(), Self::Error> {
            self.callback(state, 8, id, 0)
        }
    }
    pub(crate) fn execute(input: Input) -> Result<Output, DispatchError<&'static str>> {
        let nmodels = input.models.len();
        let ntimed = input.timed.len();
        let mut harness = Harness {
            mutation: input.mutation,
            acted: false,
            active: input.active,
            depth: 0,
            reentries: 0,
            models: std::array::from_fn(|_| {
                vec![
                    ModelRecord {
                        object: ObjectId(0),
                        opaque: 0
                    };
                    8
                ]
            }),
            timed: std::array::from_fn(|_| {
                vec![
                    TimedRecord {
                        object: ObjectId(0),
                        elapsed: 0,
                        duration: 0
                    };
                    8
                ]
            }),
            events: vec![],
        };
        for (i, id) in input.models.into_iter().enumerate() {
            harness.models[0][i].object = ObjectId(id);
        }
        for (i, (id, packed)) in input.timed.into_iter().enumerate() {
            harness.timed[0][i] = TimedRecord {
                object: ObjectId(id),
                elapsed: packed as u16,
                duration: (packed >> 16) as u16,
            };
        }
        let mut state = FrameState {
            pending_flag: input.pending,
            pending_counter: input.counter,
            pending_target: input.hook.then_some(ObjectId(102)),
            pending_argument: 123,
            context_object: input.auxiliary.then_some(ObjectId(104)),
            scene: input.scene.then_some(ObjectId(101)),
            manager: Some(ObjectId(103)),
            models: ModelView {
                allocation: ModelAllocationId(1),
                count: nmodels,
            },
            mission: input.mission.then_some(ObjectId(105)),
            timed: TimedView {
                allocation: TimedAllocationId(1),
                count: ntimed,
            },
            control_word: input.word,
        };
        harness.run(&mut state, input.delta)?;
        Ok(Output {
            pending: state.pending_flag,
            counter: state.pending_counter,
            model_count: state.models.count,
            timed_count: state.timed.count,
            reentries: harness.reentries,
            models: harness.models[(state.models.allocation.0 - 1) as usize]
                .iter()
                .map(|r| r.object.0)
                .collect(),
            timed: harness.timed[(state.timed.allocation.0 - 1) as usize]
                .iter()
                .map(|r| {
                    (
                        r.object.0,
                        u32::from(r.elapsed) | (u32::from(r.duration) << 16),
                    )
                })
                .collect(),
            events: harness.events,
        })
    }

    #[test]
    fn twenty_one_native_mutation_cases() {
        let cases = [
            (
                "normal-order-expiry-retry",
                Input {
                    mutation: 0,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "manager-replaced-by-scene",
                Input {
                    mutation: 1,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 203, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "hook-replaces-aux-and-scene",
                Input {
                    mutation: 2,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 204, 17, 1],
                        [3, 201, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "aux-clears-scene",
                Input {
                    mutation: 3,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "scene-replaces-model-base-count",
                Input {
                    mutation: 4,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 2,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![3, 2, 0, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 2, 17, 1],
                        [5, 3, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "manager-appends-model",
                Input {
                    mutation: 5,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 4,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 4, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 4, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "model-rebases-model-vector",
                Input {
                    mutation: 6,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 4, 2, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 4, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "model-erases-earlier-record",
                Input {
                    mutation: 7,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 2,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![2, 3, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "model-shrinks-live-count",
                Input {
                    mutation: 8,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 2,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "model-appends-beyond-cached-count",
                Input {
                    mutation: 9,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 4,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 4, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "manager-stops-mission",
                Input {
                    mutation: 10,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "model-starts-mission",
                Input {
                    mutation: 11,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: false,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "expired-alpha-replaces-release-target",
                Input {
                    mutation: 12,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 0)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 0,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (2, 17),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
            (
                "expired-alpha-rebases-timed-vector",
                Input {
                    mutation: 13,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 0)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (3, 65536017),
                        (3, 65536000),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1065353216, 1],
                        [8, 2, 0, 1],
                        [7, 3, 1015759766, 1],
                    ],
                },
            ),
            (
                "expired-alpha-appends-timed",
                Input {
                    mutation: 14,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 0)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (3, 65536017),
                        (3, 65536000),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1065353216, 1],
                        [8, 1, 0, 1],
                        [7, 3, 1015759766, 1],
                    ],
                },
            ),
            (
                "release-rebases-timed-vector",
                Input {
                    mutation: 15,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 0)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (3, 65536017),
                        (3, 65536000),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1065353216, 1],
                        [8, 1, 0, 1],
                        [7, 3, 1015759766, 1],
                    ],
                },
            ),
            (
                "nonexpiry-alpha-erases-current-skips-successor",
                Input {
                    mutation: 16,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 65536000)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (2, 65536000),
                        (2, 65536000),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                    ],
                },
            ),
            (
                "nonexpiry-alpha-appends-visible",
                Input {
                    mutation: 17,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 65536000)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 3,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 65536017),
                        (3, 65536017),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1015759766, 1],
                        [7, 3, 1015759766, 1],
                    ],
                },
            ),
            (
                "alpha-reentrant-coordinator",
                Input {
                    mutation: 18,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 65536000)],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 2,
                    reentries: 1,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 65536017),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [2, 104, 0, 2],
                        [3, 101, 0, 2],
                        [4, 103, 0, 2],
                        [5, 3, 0, 2],
                        [5, 2, 0, 2],
                        [5, 1, 0, 2],
                        [6, 105, 0, 2],
                        [7, 1, 1015759766, 2],
                        [7, 2, 0, 2],
                        [7, 2, 1015759766, 1],
                    ],
                },
            ),
            (
                "hook-reentrant-flag-cleared",
                Input {
                    mutation: 19,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![],
                },
                Output {
                    pending: 0,
                    counter: 0u32 as i32,
                    model_count: 3,
                    timed_count: 0,
                    reentries: 1,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 7, 2],
                        [3, 101, 7, 2],
                        [4, 103, 7, 2],
                        [5, 3, 7, 2],
                        [5, 2, 7, 2],
                        [5, 1, 7, 2],
                        [6, 105, 7, 2],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                    ],
                },
            ),
            (
                "scene-arms-pending-during-call",
                Input {
                    mutation: 20,
                    delta: 17u32 as i32,
                    word: 63,
                    pending: 1,
                    counter: 0u32 as i32,
                    hook: true,
                    auxiliary: true,
                    scene: true,
                    mission: true,
                    active: true,
                    models: vec![1, 2, 3],
                    timed: vec![(1, 65536000), (2, 983054)],
                },
                Output {
                    pending: 1,
                    counter: 1u32 as i32,
                    model_count: 3,
                    timed_count: 1,
                    reentries: 0,
                    models: vec![1, 2, 3, 0, 0, 0, 0, 0],
                    timed: vec![
                        (1, 65536017),
                        (2, 983071),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                        (0, 0),
                    ],
                    events: vec![
                        [1, 102, 123, 1],
                        [2, 104, 17, 1],
                        [3, 101, 17, 1],
                        [4, 103, 17, 1],
                        [5, 3, 17, 1],
                        [5, 2, 17, 1],
                        [5, 1, 17, 1],
                        [6, 105, 17, 1],
                        [7, 1, 1015759766, 1],
                        [7, 2, 1065353216, 1],
                        [8, 2, 0, 1],
                    ],
                },
            ),
        ];
        for (name, input, expected) in cases {
            assert_eq!(execute(input).unwrap(), expected, "{name}");
        }
    }

    #[test]
    fn signed_ratios_obey_explicit_precision_and_rounding() {
        for precision in [0, 0x200, 0x300] {
            assert_eq!(ratio_bits(1, 3, 0x3f | precision), Some(0x3eaaaaab));
            assert_eq!(ratio_bits(1, 3, 0x43f | precision), Some(0x3eaaaaaa));
            assert_eq!(ratio_bits(-1, 3, 0x43f | precision), Some(0xbeaaaaab));
            assert_eq!(ratio_bits(1, 3, 0x83f | precision), Some(0x3eaaaaab));
            assert_eq!(ratio_bits(-1, 3, 0x83f | precision), Some(0xbeaaaaaa));
            assert_eq!(ratio_bits(-1, 0, 0xc3f | precision), Some(0xff800000));
        }
        assert_eq!(ratio_bits(1, 3, 0), None);
        assert_eq!(ratio_bits(1, 3, 0x13f), None);
    }

    #[test]
    fn unsupported_storage_is_an_error_not_a_snapshot_or_skip() {
        let mut state = FrameState {
            pending_flag: 0,
            pending_counter: 0,
            pending_target: None,
            pending_argument: 0,
            context_object: None,
            scene: None,
            manager: None,
            models: ModelView::default(),
            mission: None,
            timed: TimedView::default(),
            control_word: 0x3f,
        };
        let mut backend = Harness {
            mutation: 0,
            acted: false,
            active: false,
            depth: 1,
            reentries: 0,
            models: std::array::from_fn(|_| vec![]),
            timed: std::array::from_fn(|_| vec![]),
            events: vec![],
        };
        assert_eq!(
            state.tick(&mut backend, 1),
            Err(DispatchError::MissingManager)
        );
        state.manager = Some(ObjectId(103));
        state.models = ModelView {
            allocation: ModelAllocationId(1),
            count: 1,
        };
        assert_eq!(
            state.tick(&mut backend, 1),
            Err(DispatchError::Backend("Invalid model storage"))
        );
    }
}
