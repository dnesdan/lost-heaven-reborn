//! Original C_car::Init body after the embedded C_Vehicle::Init returns true.
//! Frame and resource effects receive the same live car and stack projection.
use super::*;
use crate::car_effects::{
    car_component_records_insert, car_get_placement_from_tck, car_get_seat_property,
    get_anim_id_for_use_car, CarEffectsBackend, CarEffectsUnavailable,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarInitQuery {
    Ai00,
    Ai01,
}
impl CarInitQuery {
    pub fn pattern(self) -> &'static [u8] {
        match self {
            Self::Ai00 => b"AI00",
            Self::Ai01 => b"AI01",
        }
    }
    pub fn pattern_address(self) -> u32 {
        match self {
            Self::Ai00 => 0x632340,
            Self::Ai01 => 0x632338,
        }
    }
    pub fn mask(self) -> u32 {
        0x20
    }
}
pub trait CarInitBackend: VehicleInitBackend {
    fn enumerate_car(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        model: u32,
        query: CarInitQuery,
        output: u32,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    fn find_car_frame(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        model: u32,
        pattern: u32,
        mask: u32,
    ) -> Result<u32, Self::Error>;
    fn frame_local_matrix(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        frame: u32,
    ) -> Result<(), Self::Error>;
    fn transform_point(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        point: u32,
        matrix: u32,
        destination: u32,
    ) -> Result<u32, Self::Error>;
    fn detach_car_frame(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        frame: u32,
    ) -> Result<(), Self::Error>;
    fn remove_car_frame(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        model: u32,
        frame: u32,
    ) -> Result<(), Self::Error>;
    fn update_car_world_matrix(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        frame: u32,
    ) -> Result<(), Self::Error>;
    fn car_matrix_scale(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        matrix: u32,
        destination: u32,
    ) -> Result<(), Self::Error>;
    fn car_atoi(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        source: u32,
    ) -> Result<u32, Self::Error>;
    fn car_uppercase(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        source: u32,
    ) -> Result<(), Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarInitRejection {
    MissingAi00,
    MissingAi01,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarInitUnavailable<E> {
    Effect(E),
    Enumeration(VehicleInitUnavailable<E>),
    NoContinuation,
    Storage,
    NumericDomain(&'static str),
}
type ResultFor<B, T> = Result<T, CarInitUnavailable<<B as VehicleInitBackend>::Error>>;
struct Car<'a, B> {
    state: &'a mut VehicleInitState,
    backend: &'a mut B,
    address: u32,
    stack: u32,
}
impl<B: CarInitBackend> Car<'_, B> {
    fn read(&mut self, p: u32) -> ResultFor<B, u32> {
        self.state
            .live_tail_memory()
            .read_word_with_backend(self.backend, p)
            .map_err(CarInitUnavailable::Effect)
    }
    fn byte(&mut self, p: u32) -> ResultFor<B, u8> {
        self.state
            .live_tail_memory()
            .read_byte_with_backend(self.backend, p)
            .map_err(CarInitUnavailable::Effect)
    }
    fn put(&mut self, p: u32, v: u32) -> ResultFor<B, ()> {
        self.state
            .live_tail_memory()
            .write_word_with_backend(self.backend, p, v)
            .map_err(CarInitUnavailable::Effect)
    }
    fn put_byte(&mut self, p: u32, v: u8) -> ResultFor<B, ()> {
        self.state
            .live_tail_memory()
            .write_byte_with_backend(self.backend, p, v)
            .map_err(CarInitUnavailable::Effect)
    }
    /// Writes a native CALL's pushed words, followed by its return address.
    fn call(&mut self, return_address: u32, args: &[u32]) -> ResultFor<B, ()> {
        for (i, value) in args.iter().rev().enumerate() {
            self.put(self.stack.wrapping_sub(4 * (i as u32 + 1)), *value)?;
        }
        self.put(
            self.stack.wrapping_sub(4 * (args.len() as u32 + 1)),
            return_address,
        )
    }
    fn copy(&mut self, from: u32, to: u32, words: u32) -> ResultFor<B, ()> {
        for i in 0..words {
            let v = self.read(from.wrapping_add(i * 4))?;
            self.put(to.wrapping_add(i * 4), v)?;
        }
        Ok(())
    }
    fn model(&mut self) -> ResultFor<B, u32> {
        self.read(self.address.wrapping_add(0x3bc))
    }
    fn detach(&mut self, frame: u32, ret: u32) -> ResultFor<B, ()> {
        self.call(ret, &[frame, 0, 0])?;
        self.backend
            .detach_car_frame(self.state.live_tail_memory(), frame)
            .map_err(CarInitUnavailable::Effect)
    }
    fn remove(&mut self, frame: u32, ret: u32) -> ResultFor<B, ()> {
        let model = self.model()?;
        self.call(ret, &[model, frame])?;
        self.backend
            .remove_car_frame(self.state.live_tail_memory(), model, frame)
            .map_err(CarInitUnavailable::Effect)
    }
    fn find(&mut self, model: u32, pattern: u32, mask: u32, ret: u32) -> ResultFor<B, u32> {
        self.call(ret, &[model, pattern, mask])?;
        self.backend
            .find_car_frame(self.state.live_tail_memory(), model, pattern, mask)
            .map_err(CarInitUnavailable::Effect)
    }
    fn transform(&mut self, frame: u32, parent: u32, ret0: u32, ret1: u32) -> ResultFor<B, u32> {
        self.call(ret0, &[])?;
        self.backend
            .frame_local_matrix(self.state.live_tail_memory(), parent)
            .map_err(CarInitUnavailable::Effect)?;
        self.call(
            ret1,
            &[
                frame.wrapping_add(0x80),
                self.stack.wrapping_add(0x28),
                parent.wrapping_add(0x50),
            ],
        )?;
        self.backend
            .transform_point(
                self.state.live_tail_memory(),
                frame.wrapping_add(0x80),
                parent.wrapping_add(0x50),
                self.stack.wrapping_add(0x28),
            )
            .map_err(CarInitUnavailable::Effect)
    }
    fn name(&mut self, pattern: &str, index: u32) -> ResultFor<B, ()> {
        let name = format!("{pattern}{}*", index as i32);
        for (i, value) in name.bytes().chain(std::iter::once(0)).enumerate() {
            self.put_byte(self.stack.wrapping_add(0xe4 + i as u32), value)?;
        }
        Ok(())
    }
    fn cstrlen(&mut self, p: u32) -> ResultFor<B, u32> {
        let mut length = 0u32;
        while self.byte(p.wrapping_add(length))? != 0 {
            length = length.wrapping_add(1);
        }
        Ok(length)
    }
    fn copy_string(&mut self, mut from: u32, mut to: u32) -> ResultFor<B, ()> {
        loop {
            let v = self.byte(from)?;
            self.put_byte(to, v)?;
            if v == 0 {
                return Ok(());
            }
            from = from.wrapping_add(1);
            to = to.wrapping_add(1);
        }
    }
}
impl VehicleInitState {
    /// Consumes the successful embedded-vehicle continuation once. Every early
    /// false return preserves ownership for the original outer caller cleanup.
    pub fn continue_car_init<B: CarInitBackend + CarEffectsBackend>(
        &mut self,
        b: &mut B,
    ) -> Result<Option<CarInitRejection>, CarInitUnavailable<B::Error>> {
        let owned = self.car.as_mut().ok_or(CarInitUnavailable::Storage)?;
        if !std::mem::take(&mut owned.post_vehicle_ready) {
            return Err(CarInitUnavailable::NoContinuation);
        }
        let address = owned.address;
        let stack = owned.scratch.address;
        let mut c = Car {
            state: self,
            backend: b,
            address,
            stack,
        };
        for (offset, value, ret) in [
            (0x2ec, 0x425190, 0x41bdfe),
            (0x2e4, 0x425480, 0x41be0a),
            (0x2e8, 0x4256b0, 0x41be16),
            (0x2f0, 0x425970, 0x41be22),
            (0x2f4, 0x4259e0, 0x41be2e),
            (0x2f8, 0x426ec0, 0x41be3a),
            (0x2fc, 0x426dd0, 0x41be46),
            (0x300, 0x4262f0, 0x41be52),
            (0x308, 0, 0x41be5a),
            (0x30c, 0x426340, 0x41be66),
            (0x310, 0x426d90, 0x41be72),
            (0x314, 0x426290, 0x41be7e),
            (0x318, 0x4262c0, 0x41be8a),
        ]
        .into_iter()
        {
            c.call(ret, &[value])?;
            c.put(address.wrapping_add(0x70 + offset), value)?;
        }
        for offset in [
            0x2098, 0x209c, 0x20a0, 0x20a4, 0x20a8, 0x20ac, 0x2080, 0x2084, 0x2088, 0x208c, 0x2090,
            0x2094,
        ] {
            c.put(address.wrapping_add(offset), 0)?;
        }
        c.put(stack + 0x18, 0)?;
        c.put(stack + 0x14, 0)?;
        for (query, output, ret, rejection) in [
            (
                CarInitQuery::Ai00,
                stack + 0x14,
                0x41bef7,
                CarInitRejection::MissingAi00,
            ),
            (
                CarInitQuery::Ai01,
                stack + 0x18,
                0x41bf2a,
                CarInitRejection::MissingAi01,
            ),
        ] {
            let model = c.model()?;
            c.call(
                ret,
                &[model, 0x4d0ea0, output, 0x20, query.pattern_address()],
            )?;
            c.backend
                .enumerate_car(c.state.live_tail_memory(), model, query, output)
                .map_err(CarInitUnavailable::Enumeration)?;
            if c.byte(address + 0xce0)? & 2 == 0 && c.read(output)? == 0 {
                return Ok(Some(rejection));
            }
        }
        if c.read(stack + 0x14)? != 0 && c.read(stack + 0x18)? != 0 {
            let parent = c.read(address + 0xed8)?;
            let frame = c.read(stack + 0x14)?;
            let result = c.transform(frame, parent, 0x41bf5d, 0x41bf76)?;
            c.copy(result, address + 0x2080, 3)?;
            let parent = c.read(address + 0xed8)?;
            let frame = c.read(stack + 0x18)?;
            let result = c.transform(frame, parent, 0x41bf9d, 0x41bfb6)?;
            c.copy(result, address + 0x208c, 3)?;
            // PUSH EDI at 0x41bfb8 shifts ESP before [ESP+0x18]:
            // this first LinkTo argument is AI00 at caller-frame +0x14.
            let first = c.read(stack + 0x14)?;
            c.detach(first, 0x41bfdc)?;
            let first = c.read(stack + 0x14)?;
            c.remove(first, 0x41bfed)?;
            let second = c.read(stack + 0x18)?;
            c.detach(second, 0x41bff9)?;
            let second = c.read(stack + 0x18)?;
            c.remove(second, 0x41c00a)?;
        }
        for offset in [
            0x80, 0xb8, 0x84, 0xbc, 0x88, 0xc0, 0x68, 0x70, 0x74, 0x78, 0x8c, 0xa0, 0xa8, 0xac,
            0xb0, 0xc4,
        ] {
            c.put(stack + offset, 0)?;
        }
        c.put_byte(stack + 0x7c, 0)?;
        c.put_byte(stack + 0xb4, 0)?;
        let mut index = 0u32;
        loop {
            let begin = c.read(address + 0xd14)?;
            if begin == 0 {
                break;
            }
            let end = c.read(address + 0xd18)?;
            if index >= ((end.wrapping_sub(begin) as i32) / 0x24) as u32 {
                break;
            }
            for i in 0..0x28 {
                c.put(stack + 0x44 + i * 4, 0)?;
            }
            c.call(0x41c0cf, &[stack + 0xe4, 0x632330, index])?;
            c.name("SM", index)?;
            let model = c.read(stack + 0x26c)?;
            let mut frame = c.find(model, stack + 0xe4, 0x10020, 0x41c0ec)?;
            let first = frame != 0;
            if frame == 0 {
                c.call(0x41c105, &[stack + 0xe4, 0x632328, index])?;
                c.name("LG", index)?;
                frame = c.find(model, stack + 0xe4, 0x10020, 0x41c11b)?;
            }
            c.put_byte(stack + 0x51, u8::from(first))?;
            if frame != 0 {
                c.put_byte(stack + 0x50, 1)?;
                let parent = c.read(frame + 0x120)?;
                let result = c.transform(frame, parent, 0x41c13f, 0x41c154)?;
                c.copy(result, stack + 0x44, 3)?;
                c.detach(frame, 0x41c172)?;
                c.remove(frame, 0x41c17f)?;
            } else {
                c.put_byte(stack + 0x50, 0)?;
                for i in 0..3 {
                    c.put(stack + 0x44 + i * 4, 0)?;
                }
            }
            let insertion = c.read(address + 0x21d0)?;
            c.call(0x41c1b7, &[insertion, 1, stack + 0x44])?;
            {
                let mut memory = c.state.tail_projection(c.backend, address + 0x70);
                car_component_records_insert(
                    &mut memory,
                    address + 0x21c8,
                    insertion,
                    1,
                    stack + 0x44,
                )
                .map_err(CarInitUnavailable::Effect)?;
            }
            index = index.wrapping_add(1);
        }
        c.init_records()?;
        c.filename()?;
        c.markers()?;
        Ok(None)
    }
}
impl<B: CarInitBackend> Car<'_, B> {
    fn filename(&mut self) -> ResultFor<B, ()> {
        let model = self.read(self.stack + 0x26c)?;
        let mut source = self.read(model + 0x154)?;
        if source == 0 {
            source = 0x63d2a0
        }
        self.copy_string(source, self.stack + 0x164)?;
        let length = self.cstrlen(self.stack + 0x164)?;
        let number = self
            .stack
            .wrapping_add(0x164)
            .wrapping_add(length)
            .wrapping_sub(5);
        self.call(0x41c2f4, &[number])?;
        let value = self
            .backend
            .car_atoi(self.state.live_tail_memory(), number)
            .map_err(CarInitUnavailable::Effect)?;
        self.put(self.address + 0x211c, value)?;
        let length = self.cstrlen(self.stack + 0x164)?;
        self.put_byte(
            self.stack
                .wrapping_add(0x164)
                .wrapping_add(length)
                .wrapping_sub(6),
            0,
        )?;
        let mut length = self.cstrlen(self.stack + 0x164)?;
        while (length as i32) > 0
            && self.byte(self.stack.wrapping_add(0x164).wrapping_add(length))? != b'\\'
        {
            length = length.wrapping_sub(1);
        }
        self.copy_string(
            self.stack.wrapping_add(0x165).wrapping_add(length),
            self.address + 0x218c,
        )?;
        self.call(0x41c362, &[self.address + 0x218c])?;
        self.backend
            .car_uppercase(self.state.live_tail_memory(), self.address + 0x218c)
            .map_err(CarInitUnavailable::Effect)?;
        for (literal, count) in [(0x632320, 7), (0x632318, 5), (0x63230c, 9), (0x632300, 9)] {
            let mut equal = true;
            for i in 0..count {
                let lhs = self.byte(self.address + 0x218c + i)?;
                let rhs = self.byte(literal + i)?;
                if lhs != rhs {
                    equal = false;
                    break;
                }
            }
            if equal {
                self.put_byte(self.address + 0x20b2, 1)?;
                break;
            }
        }
        Ok(())
    }
    fn markers(&mut self) -> ResultFor<B, ()> {
        self.call(0x41c3c6, &[self.stack + 0xe4, 0x6322f8])?;
        self.copy_string(0x6322f8, self.stack + 0xe4)?;
        for i in 0..3 {
            self.put(self.address + 0x2138 + i * 4, 0)?;
        }
        let model = self.read(self.stack + 0x26c)?;
        let frame = self.find(model, self.stack + 0xe4, 0x20, 0x41c3f4)?;
        if frame != 0 {
            self.copy(frame + 0x80, self.address + 0x2138, 3)?;
            self.detach(frame, 0x41c422)?;
            self.remove(frame, 0x41c42f)?;
        }
        self.call(0x41c441, &[self.stack + 0xe4, 0x6322f0])?;
        self.copy_string(0x6322f0, self.stack + 0xe4)?;
        for i in 0..4 {
            self.put(self.address + 0x2144 + i * 4, 0)?;
        }
        let frame = self.find(model, self.stack + 0xe4, 0x20, 0x41c46c)?;
        if frame == 0 {
            return Ok(());
        }
        self.copy(frame + 0x80, self.address + 0x2144, 3)?;
        self.copy(frame + 0x140, self.stack + 0x28, 6)?;
        let ymax = f32::from_bits(self.read(self.stack + 0x38)?);
        let ymin = f32::from_bits(self.read(self.stack + 0x2c)?);
        let difference = crate::native_matrix::checked_f32_add(ymax, -ymin)
            .map_err(|_| CarInitUnavailable::NumericDomain("TANK height subtraction"))?;
        let half = crate::native_matrix::checked_f32_mul(difference.abs(), 0.5)
            .map_err(|_| CarInitUnavailable::NumericDomain("TANK half height"))?;
        self.put(self.address + 0x2150, half.to_bits())?;
        if self.read(frame + 0x120)? == 0 {
            self.copy(frame + 0x90, self.stack + 0x28, 3)?;
        } else {
            if self.byte(frame + 0xac)? & 0x20 == 0 {
                self.call(0x41c4f7, &[])?;
                self.backend
                    .update_car_world_matrix(self.state.live_tail_memory(), frame)
                    .map_err(CarInitUnavailable::Effect)?;
            }
            self.call(0x41c505, &[frame + 0x10, self.stack + 0x28])?;
            self.backend
                .car_matrix_scale(
                    self.state.live_tail_memory(),
                    frame + 0x10,
                    self.stack + 0x28,
                )
                .map_err(CarInitUnavailable::Effect)?;
        }
        let scale = f32::from_bits(self.read(self.stack + 0x2c)?);
        let half = f32::from_bits(self.read(self.address + 0x2150)?);
        let height = crate::native_matrix::checked_f32_mul(scale, half)
            .map_err(|_| CarInitUnavailable::NumericDomain("TANK scaled half height"))?;
        self.put(self.address + 0x2150, height.to_bits())?;
        self.detach(frame, 0x41c51f)?;
        self.remove(frame, 0x41c52d)?;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarInitOutcome {
    BaseActorRejected,
    VehicleRejected(VehicleCompleteInitRejection),
    CarRejected(CarInitRejection),
    /// The original C_car::Init returned AL=1 at 0x41c530.
    Initialized,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarCompleteInitUnavailable<E> {
    Vehicle(VehicleCarInitUnavailable<E>),
    Car(CarInitUnavailable<E>),
}
impl VehicleInitState {
    /// Full original C_car::Init over supplied constructor state. Both boolean
    /// outcomes retain resources; only an explicit caller destructor releases
    /// them. Host unavailability never becomes a native false return.
    pub fn init_car<B>(
        &mut self,
        backend: &mut B,
        model: u32,
        storage: VehicleComponentTailStorage,
    ) -> Result<CarInitOutcome, CarCompleteInitUnavailable<B::Error>>
    where
        B: CarInitBackend
            + CarEffectsBackend
            + VehicleCarCallerBackend
            + VehicleComponentTailBackend
            + VehicleWheelsBackend
            + VehicleAxisBackend
            + VehicleAxisUpdateBackend
            + VehicleInitFinishBackend
            + VehicleSetupBackend,
    {
        let car = self
            .car
            .as_ref()
            .ok_or(CarCompleteInitUnavailable::Car(CarInitUnavailable::Storage))?
            .address;
        match self
            .init_from_car(backend, car, model, storage)
            .map_err(CarCompleteInitUnavailable::Vehicle)?
        {
            VehicleCarInitOutcome::BaseActorRejected => Ok(CarInitOutcome::BaseActorRejected),
            VehicleCarInitOutcome::VehicleRejected(reason) => {
                Ok(CarInitOutcome::VehicleRejected(reason))
            }
            VehicleCarInitOutcome::PostVehicleFrontier => match self
                .continue_car_init(backend)
                .map_err(CarCompleteInitUnavailable::Car)?
            {
                Some(reason) => Ok(CarInitOutcome::CarRejected(reason)),
                None => Ok(CarInitOutcome::Initialized),
            },
        }
    }
}
impl<B: CarEffectsBackend> CarEffectsBackend for component_tail::Projection<'_, B> {
    fn car_normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.car_normalize_frame_quaternion(frame)
    }
    fn car_set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error> {
        self.backend.car_set_frame_rotation_matrix(frame)
    }
    fn car_animation_name(&mut self, id: u32) -> Result<Option<Vec<u8>>, Self::Error> {
        self.backend.car_animation_name(id)
    }
    fn car_track_placement(
        &mut self,
        _memory: Option<VehicleComponentTailMemory<'_>>,
        name: &[u8],
        position: u32,
        direction: u32,
    ) -> Result<bool, Self::Error> {
        self.backend
            .car_track_placement(Some(self.memory.reborrow()), name, position, direction)
    }
}
fn effects_error<E>(error: CarEffectsUnavailable<E>) -> CarInitUnavailable<E> {
    match error {
        CarEffectsUnavailable::Effect(error) => CarInitUnavailable::Effect(error),
        CarEffectsUnavailable::NumericDomain => {
            CarInitUnavailable::NumericDomain("car seat placement")
        }
    }
}
impl<B: CarInitBackend + CarEffectsBackend> Car<'_, B> {
    fn init_records(&mut self) -> ResultFor<B, ()> {
        let begin = self.read(self.address + 0x21cc)?;
        if begin == 0 {
            return Ok(());
        }
        let end = self.read(self.address + 0x21d0)?;
        let mut index = ((end.wrapping_sub(begin) as i32) / 160) as u32;
        if index == 0 {
            return Ok(());
        }
        let mut offset = index.wrapping_mul(160);
        loop {
            index = index.wrapping_sub(1);
            offset = offset.wrapping_sub(160);
            self.put(self.stack + 0x1c, offset)?;
            self.call(
                0x41c21b,
                &[
                    index,
                    self.stack + 0x12,
                    self.stack + 0x13,
                    self.stack + 0x24,
                    self.stack + 0x20,
                ],
            )?;
            let present = {
                let mut projection = self
                    .state
                    .tail_projection(self.backend, self.address + 0x70);
                car_get_seat_property(
                    &mut projection,
                    self.address,
                    index,
                    self.stack + 0x12,
                    self.stack + 0x13,
                    self.stack + 0x24,
                    self.stack + 0x20,
                )
                .map_err(effects_error)?
            };
            if present {
                let begin = self.read(self.address + 0xd14)?;
                let flag = if begin == 0 {
                    false
                } else {
                    let end = self.read(self.address + 0xd18)?;
                    let count = (end.wrapping_sub(begin) as i32) / 36;
                    self.put(self.stack + 0x40, count as u32)?;
                    count != 0 && self.read(begin + 0x20)? == 0
                };
                let argument1 = self.read(self.stack + 0x20)?;
                let cl = self.byte(self.stack + 0x12)?;
                let argument0 = self.read(self.stack + 0x24)?;
                let dl = self.byte(self.stack + 0x13)?;
                self.call(0x41c272, &[argument0, argument1, 0, u32::from(flag)])?;
                let id = get_anim_id_for_use_car(cl, dl, argument0, argument1, 0, u32::from(flag));
                let begin = self.read(self.address + 0x21cc)?;
                offset = self.read(self.stack + 0x1c)?;
                let position = begin.wrapping_add(offset).wrapping_add(0x84);
                let direction = begin.wrapping_add(offset).wrapping_add(0x90);
                self.call(0x41c29c, &[index | 0x80000000, id, position, direction])?;
                {
                    let mut projection = self
                        .state
                        .tail_projection(self.backend, self.address + 0x70);
                    car_get_placement_from_tck(
                        &mut projection,
                        self.address,
                        index | 0x80000000,
                        id,
                        position,
                        direction,
                    )
                    .map_err(effects_error)?;
                }
            } else {
                offset = self.read(self.stack + 0x1c)?;
            }
            if index == 0 {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct Memory {
        bytes: BTreeMap<u32, u8>,
        enumerated: Vec<CarInitQuery>,
        fail_find: bool,
        ai_frames: bool,
        detached: Vec<u32>,
        removed: Vec<u32>,
    }
    impl VehicleInitBackend for Memory {
        type Error = ();
        fn read_byte(&mut self, p: u32) -> Result<u8, ()> {
            self.bytes.get(&p).copied().ok_or(())
        }
        fn read_word(&mut self, p: u32) -> Result<u32, ()> {
            let mut v = [0; 4];
            for (i, b) in v.iter_mut().enumerate() {
                *b = self.read_byte(p + i as u32)?;
            }
            Ok(u32::from_le_bytes(v))
        }
        fn write_byte(&mut self, p: u32, v: u8) -> Result<(), ()> {
            self.bytes.insert(p, v);
            Ok(())
        }
        fn write_word(&mut self, p: u32, v: u32) -> Result<(), ()> {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.write_byte(p + i as u32, b)?;
            }
            Ok(())
        }
        fn free(&mut self, _: u32) -> Result<(), ()> {
            panic!("Init tail does not destroy retained ownership")
        }
        fn allocate(&mut self, _: u32) -> Result<u32, ()> {
            unreachable!()
        }
        fn collect_frame(&mut self, _: &mut VehicleInitFrameVector, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<()>> {
            unreachable!()
        }
    }
    impl CarInitBackend for Memory {
        fn enumerate_car(
            &mut self,
            mut memory: VehicleComponentTailMemory<'_>,
            _: u32,
            q: CarInitQuery,
            output: u32,
        ) -> Result<(), VehicleInitUnavailable<()>> {
            self.enumerated.push(q);
            if self.ai_frames {
                memory
                    .write_word(
                        output,
                        match q {
                            CarInitQuery::Ai00 => 0x6000,
                            CarInitQuery::Ai01 => 0x6200,
                        },
                    )
                    .unwrap();
            }
            Ok(())
        }
        fn find_car_frame(
            &mut self,
            mut m: VehicleComponentTailMemory<'_>,
            _: u32,
            _: u32,
            _: u32,
        ) -> Result<u32, ()> {
            if self.fail_find {
                m.write_word(0x10018, 0x12345678).unwrap();
                Err(())
            } else {
                Ok(0)
            }
        }
        fn frame_local_matrix(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            _: u32,
        ) -> Result<(), ()> {
            Ok(())
        }
        fn transform_point(
            &mut self,
            mut memory: VehicleComponentTailMemory<'_>,
            point: u32,
            _: u32,
            destination: u32,
        ) -> Result<u32, ()> {
            for (offset, value) in [(0, point), (4, 2), (8, 3)] {
                memory.write_word(destination + offset, value).unwrap();
            }
            Ok(destination)
        }
        fn detach_car_frame(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            frame: u32,
        ) -> Result<(), ()> {
            self.detached.push(frame);
            Ok(())
        }
        fn remove_car_frame(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            _: u32,
            frame: u32,
        ) -> Result<(), ()> {
            self.removed.push(frame);
            Ok(())
        }
        fn update_car_world_matrix(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            _: u32,
        ) -> Result<(), ()> {
            unreachable!()
        }
        fn car_matrix_scale(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            _: u32,
            _: u32,
        ) -> Result<(), ()> {
            unreachable!()
        }
        fn car_atoi(&mut self, _: VehicleComponentTailMemory<'_>, _: u32) -> Result<u32, ()> {
            Ok(0)
        }
        fn car_uppercase(&mut self, _: VehicleComponentTailMemory<'_>, _: u32) -> Result<(), ()> {
            Ok(())
        }
    }
    impl CarEffectsBackend for Memory {
        fn car_normalize_frame_quaternion(&mut self, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn car_set_frame_rotation_matrix(&mut self, _: u32) -> Result<(), ()> {
            unreachable!()
        }
        fn car_animation_name(&mut self, _: u32) -> Result<Option<Vec<u8>>, ()> {
            unreachable!()
        }
        fn car_track_placement(
            &mut self,
            _: Option<VehicleComponentTailMemory<'_>>,
            _: &[u8],
            _: u32,
            _: u32,
        ) -> Result<bool, ()> {
            unreachable!()
        }
    }
    fn fixture(flags: u8) -> (VehicleInitState, Memory) {
        let mut bytes = vec![0xa5; 0x2370];
        for (offset, value) in [(0x3bc, 0x4000u32), (0xd14, 0), (0x21cc, 0), (0x21d0, 0)] {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes[0xce0] = flags;
        bytes[0x20b2] = 0;
        let mut scratch = CarInitScratch {
            address: 0x80000,
            bytes: [0x7d; CAR_INIT_STACK_BYTES],
        };
        scratch.bytes[0x26c..0x270].copy_from_slice(&0x4000u32.to_le_bytes());
        let mut state = VehicleInitState::from_car_bytes(
            0x10000,
            bytes,
            scratch,
            CarInitRegisters {
                ebx: 1,
                ebp: 2,
                esi: 3,
                edi: 4,
            },
        )
        .unwrap();
        state.tail_vehicle_address = Some(0x10070);
        state.tail_scratch = Some(VehicleComponentTailScratch {
            address: 0x7fe30,
            bytes: [0x5c; VEHICLE_INIT_STACK_BYTES],
        });
        state.car.as_mut().unwrap().post_vehicle_ready = true;
        let mut b = Memory::default();
        b.write_word(0x4154, 0x5000).unwrap();
        for (address, bytes) in [
            (0x5000, b"models\\POLCAD00.4ds\0".as_slice()),
            (0x632320, b"POLCAD\0"),
            (0x632318, b"POLI\0"),
            (0x63230c, b"POLIMFOR\0"),
            (0x632300, b"POLIMTUD\0"),
            (0x6322f8, b"FUEL\0"),
            (0x6322f0, b"TANK\0"),
        ] {
            for (i, v) in bytes.iter().enumerate() {
                b.write_byte(address + i as u32, *v).unwrap();
            }
        }
        (state, b)
    }
    #[test]
    fn missing_ai_native_rejection_and_exempt_car_complete_with_exact_callback_slots() {
        for flags in [0, 3] {
            let (mut state, mut b) = fixture(flags);
            assert_eq!(
                state.continue_car_init(&mut b),
                Ok(if flags == 0 {
                    Some(CarInitRejection::MissingAi00)
                } else {
                    None
                })
            );
            assert_eq!(
                b.enumerated,
                if flags == 0 {
                    vec![CarInitQuery::Ai00]
                } else {
                    vec![CarInitQuery::Ai00, CarInitQuery::Ai01]
                }
            );
            let bytes = state.car_bytes().unwrap();
            for (offset, value) in [
                (0x2ec, 0x425190u32),
                (0x2e4, 0x425480),
                (0x2e8, 0x4256b0),
                (0x308, 0),
                (0x318, 0x4262c0),
            ] {
                assert_eq!(&bytes[0x70 + offset..0x74 + offset], &value.to_le_bytes());
            }
            for offset in [0x304, 0x31c] {
                assert_eq!(&bytes[0x70 + offset..0x74 + offset], &[0xa5; 4]);
            }
            if flags == 3 {
                assert_eq!(&bytes[0x218c..0x2193], b"POLCAD\0");
                assert_eq!(bytes[0x20b2], 1);
            }
            assert_eq!(
                state.continue_car_init(&mut b),
                Err(CarInitUnavailable::NoContinuation)
            );
        }
    }
    #[test]
    fn two_ai_frames_unlink_and_remove_in_original_stack_adjusted_order() {
        let (mut state, mut backend) = fixture(0);
        state.store::<()>(0xed8 - 0x70, 0x6400).unwrap();
        backend.ai_frames = true;
        assert_eq!(state.continue_car_init(&mut backend), Ok(None));
        assert_eq!(backend.detached, [0x6000, 0x6200]);
        assert_eq!(backend.removed, [0x6000, 0x6200]);
    }
    #[test]
    fn unavailable_late_effect_keeps_live_actor_mutation_and_consumes_continuation() {
        let (mut state, mut b) = fixture(3);
        b.fail_find = true;
        assert_eq!(
            state.continue_car_init(&mut b),
            Err(CarInitUnavailable::Effect(()))
        );
        assert_eq!(
            &state.car_bytes().unwrap()[0x18..0x1c],
            &0x12345678u32.to_le_bytes()
        );
        assert_eq!(
            state.continue_car_init(&mut b),
            Err(CarInitUnavailable::NoContinuation)
        );
    }
}
