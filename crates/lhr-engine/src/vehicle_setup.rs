//! Final original Init helper at Game RVA0xbfa50 (descriptive working name).
//! Its own0x1218-byte caller-supplied stack stays distinct from Init's stack.
use super::wheels::WheelsResult;
use super::*;
use crate::native_transform::{SMatrix, SQuat};

pub trait VehicleSetupBackend:
    VehicleComponentTailBackend + VehicleSourceParametersBackend + VehicleLightInitBackend
{
    fn enumerate_setup_frames(
        &mut self,
        model: u32,
        pattern: &[u8],
        mask: u32,
        target: &mut VehicleSetupEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    fn setup_frame_bounds(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn setup_deform(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        stack: u32,
    ) -> Result<(), Self::Error>;
}
pub struct VehicleSetupEnumeration<'a> {
    memory: VehicleComponentTailMemory<'a>,
    address: u32,
    collect: bool,
}
impl VehicleSetupEnumeration<'_> {
    pub fn visit<B: VehicleInitBackend>(&mut self, b: &mut B, frame: u32) -> Result<u32, B::Error> {
        if self.collect {
            let mut projection = super::component_tail::Projection {
                backend: b,
                memory: VehicleComponentTailMemory {
                    vehicle_address: self.memory.vehicle_address,
                    vehicle: self.memory.vehicle,
                    scratch: self.memory.scratch,
                },
            };
            components::vehicle_collect_frame(&mut projection, self.address, frame)
        } else {
            self.memory
                .write_word_with_backend(b, self.address, frame)?;
            Ok(0)
        }
    }
}
struct Setup<'a, B> {
    state: &'a mut VehicleInitState,
    b: &'a mut B,
    stack: u32,
    vehicle: u32,
}
impl<B: VehicleSetupBackend> Setup<'_, B> {
    fn word(&mut self, p: u32) -> WheelsResult<B, u32> {
        self.state.wheel_word(self.b, p)
    }
    fn put(&mut self, p: u32, v: u32) -> WheelsResult<B, ()> {
        self.state.wheel_store(self.b, p, v)
    }
    fn byte(&mut self, p: u32) -> WheelsResult<B, u8> {
        self.state.wheel_byte(self.b, p)
    }
    fn put_byte(&mut self, p: u32, v: u8) -> WheelsResult<B, ()> {
        self.state
            .live_tail_memory()
            .write_byte_with_backend(self.b, p, v)
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    fn sw(&mut self, o: u32) -> WheelsResult<B, u32> {
        self.word(self.stack.wrapping_add(o))
    }
    fn sp(&mut self, o: u32, v: u32) -> WheelsResult<B, ()> {
        self.put(self.stack.wrapping_add(o), v)
    }
    fn vw(&mut self, o: u32) -> WheelsResult<B, u32> {
        self.word(self.vehicle.wrapping_add(o))
    }
    fn vp(&mut self, o: u32, v: u32) -> WheelsResult<B, ()> {
        self.put(self.vehicle.wrapping_add(o), v)
    }
    fn copy(&mut self, d: u32, s: u32, n: u32) -> WheelsResult<B, ()> {
        for o in (0..n).step_by(4) {
            let v = self.word(s.wrapping_add(o))?;
            self.put(d.wrapping_add(o), v)?;
        }
        Ok(())
    }
    fn vector(&mut self, p: u32) -> WheelsResult<B, [u32; 3]> {
        let mut v = [0; 3];
        for (i, x) in v.iter_mut().enumerate() {
            *x = self.word(p.wrapping_add(i as u32 * 4))?;
        }
        Ok(v)
    }
    fn transform(&mut self, point: u32, matrix: u32) -> WheelsResult<B, ()> {
        use crate::native_matrix::{checked_f32_add as add, checked_f32_mul as mul};
        for column in 0..3u32 {
            let x = f32::from_bits(self.word(point)?);
            let m = f32::from_bits(self.word(matrix.wrapping_add(column * 4))?);
            let first = mul(x, m).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let translation = f32::from_bits(self.word(matrix.wrapping_add(48 + column * 4))?);
            let translated =
                add(first, translation).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let y = f32::from_bits(self.word(point.wrapping_add(4))?);
            let m = f32::from_bits(self.word(matrix.wrapping_add(16 + column * 4))?);
            let second = mul(y, m).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let accumulated =
                add(translated, second).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let z = f32::from_bits(self.word(point.wrapping_add(8))?);
            let m = f32::from_bits(self.word(matrix.wrapping_add(32 + column * 4))?);
            let third = mul(z, m).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let result =
                add(accumulated, third).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.sp(0x1c + column * 4, result.to_bits())?;
        }
        Ok(())
    }
    fn normalize_wheel(&mut self, wheel: u32) -> WheelsResult<B, [u32; 3]> {
        use crate::native_matrix::{checked_f32_add as add, checked_f32_mul as mul};
        let fail = |_| VehicleWheelsUnavailable::NumericDomain;
        let x = self.word(wheel.wrapping_add(0x70))?;
        self.sp(0x1c, x)?;
        let xx = mul(f32::from_bits(x), f32::from_bits(x)).map_err(fail)?;
        let y = self.word(wheel.wrapping_add(0x74))?;
        let z = self.word(wheel.wrapping_add(0x78))?;
        self.sp(0x20, y)?;
        let yy = mul(f32::from_bits(y), f32::from_bits(y)).map_err(fail)?;
        self.sp(0x24, z)?;
        let xy = add(xx, yy).map_err(fail)?;
        let zz = mul(f32::from_bits(z), f32::from_bits(z)).map_err(fail)?;
        let squared = add(xy, zz).map_err(fail)?;
        self.sp(0x44, squared.to_bits())?;
        let delta = add(squared, -1.).map_err(fail)?;
        if f64::from(delta.abs()) >= f64::from(1e-8_f32) {
            let squared = f32::from_bits(self.sw(0x44)?);
            if squared < 1e-8_f32 {
                let x = f32::from_bits(self.sw(0x1c)?);
                let (offset, value) = if x != 0. {
                    (0x1c, x)
                } else {
                    let z = f32::from_bits(self.sw(0x24)?);
                    if z != 0. {
                        (0x24, z)
                    } else {
                        (0x20, f32::from_bits(self.sw(0x20)?))
                    }
                };
                self.sp(
                    offset,
                    if value < 0. {
                        (-1f32).to_bits()
                    } else {
                        1f32.to_bits()
                    },
                )?;
            } else {
                let squared = f32::from_bits(self.sw(0x44)?);
                let length = squared.sqrt();
                let reciprocal = 1. / length;
                if !length.is_normal() || !reciprocal.is_normal() {
                    return Err(VehicleWheelsUnavailable::NumericDomain);
                }
                for offset in [0x1c, 0x20, 0x24] {
                    let value = f32::from_bits(self.sw(offset)?);
                    let result = mul(value, reciprocal).map_err(fail)?;
                    self.sp(offset, result.to_bits())?;
                }
            }
        }
        self.vector(self.stack.wrapping_add(0x1c))
    }
    fn enumerate(
        &mut self,
        pattern: &[u8],
        mask: u32,
        address: u32,
        collect: bool,
    ) -> WheelsResult<B, ()> {
        let model = self.vw(0x34c)?;
        self.b.enumerate_setup_frames(
            model,
            pattern,
            mask,
            &mut VehicleSetupEnumeration {
                memory: self.state.live_tail_memory(),
                address,
                collect,
            },
        )?;
        Ok(())
    }
    fn short(&mut self, p: u32, v: u16) -> WheelsResult<B, ()> {
        for (i, x) in v.to_le_bytes().into_iter().enumerate() {
            self.put_byte(p.wrapping_add(i as u32), x)?;
        }
        Ok(())
    }
    fn count(&mut self, header: u32, stride: u32) -> WheelsResult<B, u32> {
        let begin = self.word(header.wrapping_add(4))?;
        if begin == 0 {
            return Ok(0);
        }
        let end = self.word(header.wrapping_add(8))?;
        Ok(((end.wrapping_sub(begin) as i32) / (stride as i32)) as u32)
    }
    fn run(&mut self) -> WheelsResult<B, bool> {
        let model = self.vw(0x34c)?;
        self.copy(
            self.vehicle.wrapping_add(0x320),
            model.wrapping_add(0x80),
            12,
        )?;
        let flags = self.word(model.wrapping_add(0xac))?;
        if flags & 8 == 0 {
            self.put(model.wrapping_add(0xac), flags | 8)?;
            self.b
                .materialize_frame_quaternion(model)
                .map_err(VehicleInitUnavailable::Effect)?;
        }
        self.copy(self.stack.wrapping_add(0x54), model.wrapping_add(0x9c), 16)?;
        let mut q = [0.; 4];
        for (i, x) in q.iter_mut().enumerate() {
            *x = f32::from_bits(self.sw(0x54 + i as u32 * 4)?);
        }
        let mut m = SMatrix::identity();
        m.try_set_rot3(SQuat { wxyz: q })
            .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
        for (i, x) in m.elements.into_iter().enumerate() {
            self.sp(0x11c8 + i as u32 * 4, x.to_bits())?;
        }
        self.sp(0x1c, 0)?;
        self.copy(
            self.stack.wrapping_add(0x64),
            self.stack.wrapping_add(0x11c8),
            64,
        )?;
        let first = self.sw(0x1c)?;
        self.sp(0x20, 0)?;
        let second = self.sw(0x20)?;
        self.sp(0x24, 0x3f800000)?;
        self.vp(0xc3c, first)?;
        let third = self.sw(0x24)?;
        self.sp(0x1c, 0x3f800000)?;
        self.vp(0xc40, second)?;
        self.sp(0x20, 0)?;
        self.sp(0x24, 0)?;
        self.vp(0xc44, third)?;
        let first = self.sw(0x1c)?;
        let second = self.sw(0x20)?;
        self.vp(0xc48, first)?;
        let third = self.sw(0x24)?;
        self.vp(0xc4c, second)?;
        self.sp(0x1c, 0)?;
        self.sp(0x20, 0x3f800000)?;
        self.vp(0xc50, third)?;
        let first = self.sw(0x1c)?;
        let second = self.sw(0x20)?;
        self.vp(0xc54, first)?;
        self.sp(0x24, 0)?;
        let third = self.sw(0x24)?;
        self.vp(0xc58, second)?;
        self.vp(0xc5c, third)?;
        for offset in [0xc3c, 0xc48, 0xc54] {
            self.transform(
                self.vehicle.wrapping_add(offset),
                self.stack.wrapping_add(0x64),
            )?;
            self.copy(
                self.vehicle.wrapping_add(offset),
                self.stack.wrapping_add(0x1c),
                12,
            )?;
        }
        self.vp(0xc28, 0)?;
        self.enumerate(b"SWHEEL", 1, self.vehicle.wrapping_add(0xc28), false)?;
        let wheel = self.vw(0xc28)?;
        if wheel != 0 {
            let flags = self.word(wheel.wrapping_add(0xac))?;
            let direction = if flags & 0x10 == 0 {
                self.put(wheel.wrapping_add(0xac), flags | 0x10)?;
                self.b
                    .normalize_frame_quaternion(wheel)
                    .map_err(VehicleInitUnavailable::Effect)?;
                self.b
                    .set_frame_rotation_matrix(wheel)
                    .map_err(VehicleInitUnavailable::Effect)?;
                self.vector(wheel.wrapping_add(0x70))?
            } else {
                self.normalize_wheel(wheel)?
            };
            for (i, x) in direction.into_iter().enumerate() {
                self.vp(0x37c + i as u32 * 4, x)?;
            }
        }
        self.sp(0x44, 0)?;
        self.enumerate(b"SMOKE_MOTOR", 0x20, self.stack.wrapping_add(0x44), false)?;
        let smoke = self.sw(0x44)?;
        if smoke != 0 {
            let body = self.vw(0xe68)?;
            self.b
                .prepare_frame_matrix(body)
                .map_err(VehicleInitUnavailable::Effect)?;
            self.transform(smoke.wrapping_add(0x80), body.wrapping_add(0x50))?;
            self.copy(
                self.vehicle.wrapping_add(0x1e0),
                self.stack.wrapping_add(0x1c),
                12,
            )?;
            let smoke = self.sw(0x44)?;
            self.b
                .link_frame(smoke, 0, 0)
                .map_err(VehicleInitUnavailable::Effect)?;
            let model = self.vw(0x34c)?;
            let smoke = self.sw(0x44)?;
            self.b
                .remove_model_child(model, smoke)
                .map_err(VehicleInitUnavailable::Effect)?;
        } else {
            for o in [0x1c, 0x20, 0x24] {
                self.sp(o, 0)?;
            }
            self.copy(
                self.vehicle.wrapping_add(0x1e0),
                self.stack.wrapping_add(0x1c),
                12,
            )?;
        }
        let marker = self.byte(self.stack.wrapping_add(0x4f))?;
        self.sp(0x38, 0)?;
        self.put_byte(self.stack.wrapping_add(0x34), marker)?;
        self.sp(0x3c, 0)?;
        self.sp(0x40, 0)?;
        self.sp(0x1210, 0)?;
        for (pattern, side, kind) in [
            (b"STRIPL*".as_slice(), 0u32, 0u16),
            (b"STRIPML*", 0, 1),
            (b"STRIPFL*", 0, 3),
            (b"STRIPBL*", 0, 5),
            (b"STRIPR*", 1, 0),
            (b"STRIPMR*", 1, 1),
            (b"STRIPFR*", 1, 3),
            (b"STRIPBR*", 1, 5),
        ] {
            self.enumerate(pattern, 0x10001, self.stack.wrapping_add(0x34), true)?;
            let mut index = 0;
            let begin_after = loop {
                let begin = self.sw(0x38)?;
                let end = self.sw(0x3c)?;
                if begin == 0 || index >= ((end.wrapping_sub(begin) as i32) >> 2) as u32 {
                    break begin;
                }
                for o in [0x10, 0x14, 0x18] {
                    self.sp(o, 0)?;
                }
                let slot = begin.wrapping_add(index.wrapping_mul(4));
                let frame = self.word(slot)?;
                self.sp(0x10, frame)?;
                if kind != 0 {
                    self.short(self.stack.wrapping_add(0x1a), kind)?;
                    let current = if self.byte(frame.wrapping_add(0xac))? & 0x40 == 0 {
                        self.b
                            .setup_frame_bounds(frame)
                            .map_err(VehicleInitUnavailable::Effect)?;
                        self.sw(0x10)?
                    } else {
                        frame
                    };
                    self.copy(self.stack.wrapping_add(0x1c), frame.wrapping_add(0xb0), 24)?;
                    let zmax = f32::from_bits(self.sw(0x30)?);
                    let zmin = f32::from_bits(self.sw(0x24)?);
                    let length = crate::native_matrix::checked_f32_add(zmax, -zmin)
                        .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
                    self.sp(0x14, length.to_bits())?;
                    let mut name = self.word(current.wrapping_add(0x100))?;
                    if name == 0 {
                        name = 0x63d2a0;
                    }
                    let base_name = self.vw(0x4bc)?;
                    let mut len = 0u32;
                    while self.byte(base_name.wrapping_add(len))? != 0 {
                        len = len.wrapping_add(1);
                    }
                    let number = self
                        .b
                        .parse_component_number(name.wrapping_add(len).wrapping_add(8))
                        .map_err(VehicleInitUnavailable::Effect)?;
                    self.short(
                        self.stack.wrapping_add(0x18),
                        (number.wrapping_mul(2).wrapping_add(side as i32)) as u16,
                    )?;
                }
                let header = self
                    .vehicle
                    .wrapping_add(if side == 0 { 0xd8 } else { 0xe8 });
                let insertion = self.word(header.wrapping_add(8))?;
                let mut projection = self.state.tail_projection(self.b, self.vehicle);
                super::setup_records::vehicle_setup_records_insert(
                    &mut projection,
                    header,
                    insertion,
                    1,
                    self.stack.wrapping_add(0x10),
                )
                .map_err(VehicleInitUnavailable::Effect)?;
                index = index.wrapping_add(1);
            };
            self.sp(0x3c, begin_after)?;
        }
        let mut i = 0u32;
        while (i as i32) < self.vw(0x4c4)? as i32 {
            let array = self.vw(0xc38)?;
            let wheel = self.word(array.wrapping_add(i * 4))?;
            let parent = self.word(wheel.wrapping_add(8))?;
            self.copy(wheel.wrapping_add(0x10), parent.wrapping_add(0x80), 12)?;
            for offsets in [
                &[0x94][..],
                &[0x4c, 0x50, 0x54][..],
                &[0x58, 0x5c, 0x60][..],
                &[0x114][..],
            ] {
                let array = self.vw(0xc38)?;
                let wheel = self.word(array.wrapping_add(i * 4))?;
                for o in offsets {
                    self.put(wheel.wrapping_add(*o), 0)?;
                }
            }
            i = i.wrapping_add(1);
        }
        self.put_byte(self.vehicle.wrapping_add(0x4cc), 1)?;
        self.b
            .setup_deform(self.state.live_tail_memory(), self.stack)
            .map_err(VehicleInitUnavailable::Effect)?;
        let header = self.vehicle.wrapping_add(0xca0);
        let mut count = self.count(header, 36)?;
        self.sp(0x50, count)?;
        let mut offset = count.wrapping_mul(36);
        while count != 0 {
            offset = offset.wrapping_sub(36);
            self.sp(0x48, offset)?;
            let begin = self.vw(0xca4)?;
            let record = self.word(begin.wrapping_add(offset).wrapping_add(32))?;
            if record != 0 {
                let frame = self.word(record.wrapping_add(0x34))?;
                if frame != 0 {
                    let mut n = self.count(self.vehicle.wrapping_add(0x214), 52)?;
                    if n != 0 {
                        let begin = self.vw(0x218)?;
                        let mut end = begin.wrapping_add(n.wrapping_mul(52));
                        while n != 0 {
                            end = end.wrapping_sub(52);
                            n = n.wrapping_sub(1);
                            if self.word(end)? == frame {
                                self.short(record.wrapping_add(0x54), n as u16)?;
                                break;
                            }
                        }
                    }
                }
            }
            count = self.sw(0x50)?.wrapping_sub(1);
            self.sp(0x50, count)?;
            offset = self.sw(0x48)?;
        }
        self.state
            .initialize_lights(self.b, self.stack.wrapping_sub(0xe8))
            .map_err(|error| match error {
                VehicleLightInitUnavailable::Effect(e) => {
                    VehicleWheelsUnavailable::Init(VehicleInitUnavailable::Effect(e))
                }
                VehicleLightInitUnavailable::NumericDomain => {
                    VehicleWheelsUnavailable::NumericDomain
                }
                VehicleLightInitUnavailable::NoContinuation => {
                    VehicleWheelsUnavailable::NoContinuation
                }
            })?;
        {
            let mut projection = self.state.tail_projection(self.b, self.vehicle);
            vehicle_setup_default_source(&mut projection, self.vehicle, self.stack)
                .map_err(VehicleInitUnavailable::Effect)?;
        }
        let result = self
            .state
            .init_car_parameters(self.b, self.stack.wrapping_add(0xa4), 0)?;
        let allocation = self.sw(0x38)?;
        self.state
            .init_free(self.b, allocation)
            .map_err(VehicleInitUnavailable::Effect)?;
        Ok(result)
    }
}
impl VehicleInitState {
    pub fn setup<B: VehicleSetupBackend>(
        &mut self,
        b: &mut B,
        stack: u32,
    ) -> WheelsResult<B, bool> {
        if !std::mem::take(&mut self.final_setup_ready) {
            return Err(VehicleWheelsUnavailable::NoContinuation);
        }
        let vehicle = self
            .tail_vehicle_address
            .ok_or(VehicleWheelsUnavailable::NoContinuation)?;
        Setup {
            state: self,
            b,
            stack,
            vehicle,
        }
        .run()
    }
}
