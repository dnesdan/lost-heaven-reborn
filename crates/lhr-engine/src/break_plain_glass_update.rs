//! Original C_Break_plain_glass::Tick (PC5611b0, PS2TickFUi3b75c0).
//! The52-byte rows hold linear velocity and spin angle/speed, not a fade timer.
//! Positions, real queried planes, flags and the completed-window ring belong
//! to the caller's existing canonical frame/heap owners.
use crate::break_plain_glass::{
    glass_random, glass_read_float, glass_read_vector, glass_write_float, glass_write_vector,
    BreakPlainGlassBackend, GlassError,
};
use crate::break_plain_glass_merge::GlassMergeBackend;
use crate::native_transform::{SMatrix, SQuat};
use crate::ph_thing_response::PhThingNumber as R;
use crate::vehicle_init::VehicleInitBackend;
type Out<T, E> = Result<T, GlassError<E>>;
fn host<T, E>(x: Result<T, E>) -> Out<T, E> {
    x.map_err(GlassError::Host)
}
fn math<T, E>(x: Result<T, &'static str>) -> Out<T, E> {
    x.map_err(GlassError::NumericDomain)
}
fn n<E>(x: f32) -> Out<R, E> {
    math(R::load(x))
}
fn spill<E>(x: R) -> Out<f32, E> {
    math(x.store())
}
fn word<B: VehicleInitBackend>(b: &mut B, a: u32) -> Out<u32, B::Error> {
    host(b.read_word(a))
}
fn put<B: VehicleInitBackend>(b: &mut B, a: u32, v: u32) -> Out<(), B::Error> {
    host(b.write_word(a, v))
}
fn dot<E>(a: [f32; 3], b: [f32; 3], order: [usize; 3]) -> Out<R, E> {
    let p = |i| math(n(a[i])?.mul(n(b[i])?));
    math(math(p(order[0])?.add(p(order[1])?))?.add(p(order[2])?))
}
fn normalized<E>(v: [R; 3], order: [usize; 3]) -> Out<[f32; 3], E> {
    let square = math(
        math(math(v[order[0]].mul(v[order[0]]))?.add(math(v[order[1]].mul(v[order[1]]))?))?
            .add(math(v[order[2]].mul(v[order[2]]))?),
    )?;
    let stored = spill(square)?;
    let mut out = v;
    if math(square.sub(n(1.)?))?.value().abs() >= f64::from(1e-8_f32) {
        if stored >= 1e-8_f32 {
            let scale = math(n(1.)?.div(math(n(stored)?.sqrt())?))?;
            for x in &mut out {
                *x = math(x.mul(scale))?;
            }
        } else {
            let axis = if v[0].value() != 0. {
                0
            } else if v[2].value() != 0. {
                2
            } else {
                1
            };
            out[axis] = n(if v[axis].value() < 0. { -1. } else { 1. })?;
        }
    }
    Ok([spill(out[0])?, spill(out[1])?, spill(out[2])?])
}
fn length_xyz<E>(v: [f32; 3]) -> Out<f32, E> {
    let squared = spill(dot(v, v, [0, 1, 2])?)?;
    if (squared.to_bits() as i32) < 0x322bcc77 {
        Ok(0.)
    } else if math(n(squared)?.sub(n(1.)?))?.value().abs() < f64::from(1e-8_f32) {
        Ok(1.)
    } else {
        spill(math(n(squared)?.sqrt())?)
    }
}
fn position<B: BreakPlainGlassBackend>(
    b: &mut B,
    frame: u32,
    value: [f32; 3],
) -> Out<(), B::Error> {
    if host(b.read_byte(frame + 0xac))? & 0x80 == 0 {
        glass_write_vector(b, frame + 0x80, value)?;
        let flags = word(b, frame + 0xac)?;
        put(b, frame + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
    }
    Ok(())
}
fn publish<B: BreakPlainGlassBackend>(b: &mut B, frame: u32) -> Out<(), B::Error> {
    if host(b.read_byte(frame + 0xac))? & 0x20 == 0 {
        host(b.glass_update_world(frame))?;
    }
    host(b.glass_place_sector(frame))?;
    host(b.glass_frame_virtual(frame, 0x18, &[]))?;
    Ok(())
}
fn row<B: VehicleInitBackend>(b: &mut B, entry: u32, offset: u32) -> Out<u32, B::Error> {
    word(b, entry + 0x10)?
        .checked_add(offset)
        .ok_or(GlassError::StorageRange)
}
fn plane_query<B: BreakPlainGlassBackend>(
    b: &mut B,
    r: u32,
    origin: [f32; 3],
    reset_normal: bool,
) -> Out<(), B::Error> {
    let point = [
        origin[0],
        spill(math(n(origin[1])?.sub(n(10000.)?))?)?,
        origin[2],
    ];
    glass_write_vector(b, r + 0x18, point)?;
    if reset_normal {
        glass_write_vector(b, r + 0x24, [0., 1., 0.])?;
    }
    let mut point = glass_read_vector(b, r + 0x18)?;
    let mut normal = glass_read_vector(b, r + 0x24)?;
    let call = host(b.glass_test_line(origin, [0., -500., 0.], 0xffff, 0, &mut point, &mut normal));
    // These are actual mutable reference arguments of TestLineV. Preserve a
    // callee's completed output writes even if its later host effect fails.
    glass_write_vector(b, r + 0x18, point)?;
    glass_write_vector(b, r + 0x24, normal)?;
    call?;
    Ok(())
}
fn local_direction<B: BreakPlainGlassBackend>(b: &mut B, frame: u32) -> Out<[f32; 3], B::Error> {
    let flags = word(b, frame + 0xac)?;
    if flags & 0x10 == 0 {
        put(b, frame + 0xac, flags | 0x10)?;
        let mut q = SQuat {
            wxyz: [
                glass_read_float(b, frame + 0x9c)?,
                glass_read_float(b, frame + 0xa0)?,
                glass_read_float(b, frame + 0xa4)?,
                glass_read_float(b, frame + 0xa8)?,
            ],
        };
        // The shared verified Normalize/SetRot3 worker uses ordinary products.
        // Wider exponent profiles remain explicit rather than silently spilling.
        for x in q.wxyz {
            let sq = math(n(x)?.mul(n(x)?))?.value();
            if sq != 0. && sq.abs() < f64::from(f32::MIN_POSITIVE) {
                return Err(GlassError::NumericDomain(
                    "glass quaternion product exponent",
                ));
            }
        }
        q.normalize();
        for (i, x) in q.wxyz.into_iter().enumerate() {
            n::<B::Error>(x)?;
            glass_write_float(b, frame + 0x9c + i as u32 * 4, x)?;
        }
        let mut matrix = SMatrix::identity();
        matrix.set_rot3(q);
        for i in [0, 1, 2, 4, 5, 6, 8, 9, 10] {
            glass_write_float(b, frame + 0x50 + i as u32 * 4, matrix.elements[i])?;
        }
        glass_read_vector(b, frame + 0x70)
    } else {
        let v = glass_read_vector(b, frame + 0x70)?;
        normalized([n(v[0])?, n(v[1])?, n(v[2])?], [0, 1, 2])
    }
}
fn settle<B: BreakPlainGlassBackend>(b: &mut B, entry: u32, offset: u32) -> Out<(), B::Error> {
    let r = row(b, entry, offset)?;
    let frame = word(b, r)?;
    let mut origin = glass_read_vector(b, frame + 0x80)?;
    // The original fallback point uses the unraised frame position, while the
    // ray starts0.1 above it. plane_query writes its fallback separately here.
    let fallback = [
        origin[0],
        spill(math(n(origin[1])?.sub(n(10000.)?))?)?,
        origin[2],
    ];
    glass_write_vector(b, r + 0x18, fallback)?;
    origin[1] = spill(math(n(origin[1])?.add(n(0.1)?))?)?;
    let mut point = glass_read_vector(b, r + 0x18)?;
    let mut normal = glass_read_vector(b, r + 0x24)?;
    let call = host(b.glass_test_line(origin, [0., -500., 0.], 0xffff, 0, &mut point, &mut normal));
    glass_write_vector(b, r + 0x18, point)?;
    glass_write_vector(b, r + 0x24, normal)?;
    call?;
    let r = row(b, entry, offset)?;
    let frame = word(b, r)?;
    let mut point = glass_read_vector(b, r + 0x18)?;
    point[1] = spill(math(n(point[1])?.add(n(0.05)?))?)?;
    position(b, frame, point)?;
    if host(b.read_byte(frame + 0xac))? & 0x20 == 0 {
        host(b.glass_update_world(frame))?;
    }
    host(b.glass_place_sector(frame))?;
    let r = row(b, entry, offset)?;
    let normal = glass_read_vector(b, r + 0x24)?;
    host(b.glass_frame_direction(frame, normal, 0.))?;
    host(b.glass_frame_virtual(frame, 0x18, &[]))?;
    let r = row(b, entry, offset)?;
    host(b.write_byte(r + 0x30, 0))?;
    Ok(())
}
fn bounce<B: BreakPlainGlassBackend>(
    b: &mut B,
    entry: u32,
    offset: u32,
    mut point: [f32; 3],
    distance: f32,
) -> Out<([f32; 3], bool), B::Error> {
    let r = row(b, entry, offset)?;
    let speed = length_xyz(glass_read_vector(b, r + 4)?)?;
    let normal = glass_read_vector(b, r + 0x24)?;
    let plane = glass_read_vector(b, r + 0x18)?;
    let plane_constant = spill(math(dot(normal, plane, [2, 1, 0])?.mul(n(-1.)?))?)?;
    let mirror_scale = math(n(distance)?.mul(n(-2.)?))?;
    let mx = spill(math(
        math(n(normal[0])?.mul(mirror_scale))?.add(n(point[0])?),
    )?)?;
    let my_step = spill(math(n(normal[1])?.mul(mirror_scale))?)?;
    let mz_step = spill(math(n(normal[2])?.mul(mirror_scale))?)?;
    let mirror = [
        mx,
        spill(math(n(point[1])?.add(n(my_step)?))?)?,
        spill(math(n(point[2])?.add(n(mz_step)?))?)?,
    ];
    let scale = math(n(distance)?.mul(n(3.)?))?;
    let ray = [
        spill(math(n(normal[0])?.mul(scale))?)?,
        spill(math(n(normal[1])?.mul(scale))?)?,
        spill(math(n(normal[2])?.mul(scale))?)?,
    ];
    let denominator = dot(ray, normal, [2, 1, 0])?;
    let denominator_float = spill(denominator)?;
    if denominator.value().abs() >= f64::from(1e-8_f32) {
        let numerator = math(dot(normal, mirror, [2, 1, 0])?.add(n(plane_constant)?))?;
        let fraction = spill(math(
            math(numerator.div(n(denominator_float)?))?.mul(n(-1.)?),
        )?)?;
        for i in 0..3 {
            let step = spill(math(n(ray[i])?.mul(n(fraction)?))?)?;
            point[i] = spill(math(n(step)?.add(n(mirror[i])?))?)?;
        }
    }
    point[0] = spill(math(math(n(normal[0])?.mul(n(0.02)?))?.add(n(point[0])?))?)?;
    point[1] = spill(math(n(point[1])?.add(math(n(normal[1])?.mul(n(0.02)?))?))?)?;
    let dz = spill(math(n(normal[2])?.mul(n(0.02)?))?)?;
    point[2] = spill(math(n(point[2])?.add(n(dz)?))?)?;
    if speed < 2. {
        let r = row(b, entry, offset)?;
        let frame = word(b, r)?;
        position(b, frame, point)?;
        let r = row(b, entry, offset)?;
        host(b.write_byte(r + 0x31, 0))?;
        return Ok((point, true));
    }
    let rebound = spill(math(n(speed)?.mul(n(0.4)?))?)?;
    let r1 = (glass_random(b)? % 200) as f32;
    let r1 = math(math(n(r1)?.sub(n(100.)?))?.mul(n(0.01)?))?;
    let nx = spill(math(n(normal[0])?.mul(r1))?)?;
    let nz = spill(math(n(normal[2])?.mul(r1))?)?;
    let negative_y = spill(math(n(-normal[1])?.mul(r1))?)?;
    let r2 = (glass_random(b)? % 200) as f32;
    let r2 = math(math(n(r2)?.sub(n(100.)?))?.mul(n(0.01)?))?;
    let x = math(math(math(n(normal[2])?.mul(r2))?.add(n(nx)?))?.add(n(normal[0])?))?;
    let y2 = spill(math(n(normal[1])?.mul(r2))?)?;
    let z2 = spill(math(n(-normal[0])?.mul(r2))?)?;
    let y = spill(math(n(y2)?.add(n(nz)?))?)?;
    let y = spill(math(n(y)?.add(n(normal[1])?))?)?;
    let z = spill(math(n(z2)?.add(n(negative_y)?))?)?;
    let z = spill(math(n(z)?.add(n(normal[2])?))?)?;
    let direction = normalized([x, n(y)?, n(z)?], [1, 2, 0])?;
    let velocity = [
        spill(math(n(direction[0])?.mul(n(rebound)?))?)?,
        spill(math(n(direction[1])?.mul(n(rebound)?))?)?,
        spill(math(n(direction[2])?.mul(n(rebound)?))?)?,
    ];
    let r = row(b, entry, offset)?;
    glass_write_vector(b, r + 4, velocity)?;
    host(b.write_byte(r + 0x32, 0))?;
    Ok((point, false))
}
fn moving<B: BreakPlainGlassBackend>(
    b: &mut B,
    manager: u32,
    entry: u32,
    offset: u32,
    dt: f32,
) -> Out<(), B::Error> {
    let r = row(b, entry, offset)?;
    let velocity = glass_read_vector(b, r + 4)?;
    let gravity = glass_read_vector(b, manager + 0x2c)?;
    let vz = spill(math(n(dt)?.mul(n(gravity[2])?))?)?;
    let next = [
        spill(math(
            math(n(dt)?.mul(n(gravity[0])?))?.add(n(velocity[0])?),
        )?)?,
        spill(math(
            math(n(dt)?.mul(n(gravity[1])?))?.add(n(velocity[1])?),
        )?)?,
        spill(math(n(vz)?.add(n(velocity[2])?))?)?,
    ];
    glass_write_vector(b, r + 4, next)?;
    let r = row(b, entry, offset)?;
    let velocity = glass_read_vector(b, r + 4)?;
    if velocity[1] < -50. {
        host(b.write_byte(r + 0x30, 0))?;
        return Ok(());
    }
    let step = [
        spill(math(n(dt)?.mul(n(velocity[0])?))?)?,
        spill(math(n(dt)?.mul(n(velocity[1])?))?)?,
        spill(math(n(dt)?.mul(n(velocity[2])?))?)?,
    ];
    let frame = word(b, r)?;
    let old = glass_read_vector(b, frame + 0x80)?;
    if host(b.read_byte(r + 0x32))? == 0 && velocity[1] < 0. {
        plane_query(b, r, old, true)?;
        let r = row(b, entry, offset)?;
        let y = spill(math(n(glass_read_float(b, r + 0x1c)?)?.add(n(0.05)?))?)?;
        glass_write_float(b, r + 0x1c, y)?;
        host(b.write_byte(r + 0x32, 1))?;
    }
    let mut point = [
        spill(math(n(step[0])?.add(n(old[0])?))?)?,
        spill(math(n(step[1])?.add(n(old[1])?))?)?,
        spill(math(n(step[2])?.add(n(old[2])?))?)?,
    ];
    let r = row(b, entry, offset)?;
    let normal = glass_read_vector(b, r + 0x24)?;
    let plane = glass_read_vector(b, r + 0x18)?;
    let point_dot = spill(dot(point, normal, [2, 1, 0])?)?;
    let distance = math(n(point_dot)?.sub(dot(plane, normal, [2, 1, 0])?))?;
    let distance_float = spill(distance)?;
    if distance.value() < 0. {
        let (result, stopped) = bounce(b, entry, offset, point, distance_float)?;
        point = result;
        if stopped {
            return Ok(());
        }
    }
    let r = row(b, entry, offset)?;
    let roll = spill(math(
        math(n(dt)?.mul(n(glass_read_float(b, r + 0x14)?)?))?
            .add(n(glass_read_float(b, r + 0x10)?)?),
    )?)?;
    glass_write_float(b, r + 0x10, roll)?;
    let r = row(b, entry, offset)?;
    let frame = word(b, r)?;
    let direction = local_direction(b, frame)?;
    let r = row(b, entry, offset)?;
    let roll = glass_read_float(b, r + 0x10)?;
    host(b.glass_frame_direction(frame, direction, roll))?;
    let r = row(b, entry, offset)?;
    let frame = word(b, r)?;
    position(b, frame, point)?;
    publish(b, frame)
}
/// Milliseconds are clamped to1..83 by the original unsigned caller contract.
/// Completed entries run the real mesh merge and source cache/erase workers.
pub fn glass_tick<B: GlassMergeBackend>(
    b: &mut B,
    manager: u32,
    milliseconds: u32,
) -> Out<(), B::Error> {
    let milliseconds = milliseconds.clamp(1, 83);
    let dt = spill(math(
        n(milliseconds as f32)?.mul(n(f32::from_bits(0x3a83126f))?),
    )?)?;
    let begin = word(b, manager + 0x14)?;
    if begin == 0 {
        return Ok(());
    }
    let end = word(b, manager + 0x18)?;
    let bytes = end
        .checked_sub(begin)
        .filter(|n| n % 24 == 0 && n / 24 <= 1 << 20)
        .ok_or(GlassError::StorageRange)?;
    let mut remaining = bytes / 24;
    while remaining != 0 {
        remaining -= 1;
        let entry = word(b, manager + 0x14)?
            .checked_add(remaining.checked_mul(24).ok_or(GlassError::StorageRange)?)
            .ok_or(GlassError::StorageRange)?;
        let count = word(b, entry + 0x14)?;
        if count > 1 << 20 {
            return Err(GlassError::StorageRange);
        }
        let mut inactive = 0;
        for index in (0..count).rev() {
            let offset = index.checked_mul(52).ok_or(GlassError::StorageRange)?;
            let r = row(b, entry, offset)?;
            if host(b.read_byte(r + 0x30))? == 0 {
                inactive += 1;
            } else if host(b.read_byte(r + 0x31))? == 0 {
                settle(b, entry, offset)?;
            } else {
                moving(b, manager, entry, offset, dt)?;
            }
        }
        if inactive == word(b, entry + 0x14)? {
            crate::break_plain_glass_lifecycle::glass_finish_entry(b, manager, remaining)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        break_plain_glass::GlassPendingResource,
        glass_subdivision::{GlassGeometry, GlassVertex},
        vehicle_frame_backend::{VehicleFrameMemory, VehicleMemoryError},
        vehicle_init::{
            VehicleInitEnumeration, VehicleInitFrameVector, VehicleInitQuery,
            VehicleInitUnavailable,
        },
    };
    const MANAGER: u32 = 0x1000;
    const ENTRY: u32 = 0x2000;
    const ROW: u32 = 0x3000;
    const FRAME: u32 = 0x4000;
    type LineObservation = ([f32; 3], [f32; 3], u32, u32, [f32; 3], [f32; 3]);
    struct Backend {
        memory: VehicleFrameMemory,
        events: Vec<&'static str>,
        queries: Vec<LineObservation>,
        hit: bool,
        pending: Vec<GlassPendingResource>,
        rolls: Vec<f32>,
    }
    impl VehicleInitBackend for Backend {
        type Error = VehicleMemoryError;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.memory.read_byte(a)
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            self.memory.read_word(a)
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            self.memory.write_byte(a, v)
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.memory.write_word(a, v)
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            self.memory.allocate(n)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.memory.free(a)
        }
        fn collect_frame(
            &mut self,
            v: &mut VehicleInitFrameVector,
            f: u32,
        ) -> Result<(), Self::Error> {
            self.memory.collect_frame(v, f)
        }
        fn enumerate(
            &mut self,
            m: u32,
            q: VehicleInitQuery,
            t: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            self.memory.enumerate(m, q, t)
        }
    }
    impl BreakPlainGlassBackend for Backend {
        fn glass_frame_virtual(
            &mut self,
            _: u32,
            slot: u32,
            _: &[u32],
        ) -> Result<u32, Self::Error> {
            if slot == 0x18 {
                self.events.push("update");
                Ok(0)
            } else {
                Err(VehicleMemoryError::FrameAdapterRequired)
            }
        }
        fn glass_update_world(&mut self, f: u32) -> Result<(), Self::Error> {
            self.events.push("world");
            let p = glass_read_vector(self, f + 0x80).unwrap();
            glass_write_vector(self, f + 0x40, p).unwrap();
            let flags = self.read_word(f + 0xac)?;
            self.write_word(f + 0xac, flags | 0x20)
        }
        fn glass_frame_direction(
            &mut self,
            f: u32,
            d: [f32; 3],
            roll: f32,
        ) -> Result<(), Self::Error> {
            self.events.push("direction");
            self.rolls.push(roll);
            let q = SQuat::try_make_frame_direction(d, roll)
                .map_err(|_| VehicleMemoryError::FrameAdapterRequired)?;
            for (i, x) in q.wxyz.into_iter().enumerate() {
                self.write_word(f + 0x9c + i as u32 * 4, x.to_bits())?;
            }
            let flags = self.read_word(f + 0xac)?;
            self.write_word(f + 0xac, (flags & 0xffff_fecb) | 0x4000_0008)
        }
        fn glass_begin_fragment(&mut self, _: u32) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_cleanup_resources(&mut self) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_new_frame(&mut self) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_source_geometry(&mut self, _: u32) -> Result<GlassGeometry, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_new_material(&mut self) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_copy_source_material(&mut self, _: u32, _: u32) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_release_material(&mut self, _: u32) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_install_fragment(
            &mut self,
            _: u32,
            _: u32,
            _: &[GlassVertex; 6],
        ) -> Result<bool, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_place_sector(&mut self, _: u32) -> Result<(), Self::Error> {
            self.events.push("sector");
            Ok(())
        }
        fn glass_debustize(&mut self, _: u32) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_ground_plane(&mut self, _: [f32; 3]) -> Result<([f32; 3], [f32; 3]), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_play_sound(&mut self, _: u32, _: [f32; 3]) -> Result<(), Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_test_line(
            &mut self,
            o: [f32; 3],
            d: [f32; 3],
            s: u32,
            f: u32,
            p: &mut [f32; 3],
            n: &mut [f32; 3],
        ) -> Result<u32, Self::Error> {
            self.events.push("line");
            self.queries.push((o, d, s, f, *p, *n));
            if self.hit {
                *p = [o[0], 0., o[2]];
                *n = [0., 1., 0.];
                Ok(7)
            } else {
                Ok(0)
            }
        }
        fn glass_merge_fragments(&mut self, _: u32) -> Result<bool, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_allocator_marker(&self) -> u8 {
            0xa5
        }
        fn glass_track(&mut self, r: GlassPendingResource) {
            self.pending.push(r)
        }
        fn glass_untrack(&mut self, r: GlassPendingResource) -> Result<(), Self::Error> {
            let p = self
                .pending
                .iter()
                .position(|x| *x == r)
                .ok_or(VehicleMemoryError::FrameAdapterRequired)?;
            self.pending.remove(p);
            Ok(())
        }
        fn glass_pending_last(&self) -> Option<GlassPendingResource> {
            self.pending.last().copied()
        }
    }
    impl GlassMergeBackend for Backend {
        fn glass_merge_material(&mut self, _: u32) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_merge_read_triangle(&mut self, _: u32) -> Result<[GlassVertex; 3], Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_merge_begin_group(&mut self, _: u32, _: &[[u16; 3]]) -> Result<u32, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
        fn glass_merge_install(
            &mut self,
            _: u32,
            _: u32,
            _: &[GlassVertex],
        ) -> Result<bool, Self::Error> {
            Err(VehicleMemoryError::FrameAdapterRequired)
        }
    }
    fn setup() -> Backend {
        let mut memory = VehicleFrameMemory::default();
        for (a, n) in [
            (MANAGER, 0x38),
            (ENTRY, 24),
            (ROW, 52),
            (FRAME, 0x200),
            (0x63a200, 4),
        ] {
            memory.map_bytes(a, &vec![0; n]).unwrap();
        }
        let mut b = Backend {
            memory,
            events: Vec::new(),
            queries: Vec::new(),
            hit: true,
            pending: Vec::new(),
            rolls: Vec::new(),
        };
        b.write_word(MANAGER + 0x14, ENTRY).unwrap();
        b.write_word(MANAGER + 0x18, ENTRY + 24).unwrap();
        glass_write_vector(&mut b, MANAGER + 0x2c, [0., -8., 0.]).unwrap();
        b.write_word(ENTRY + 0x10, ROW).unwrap();
        b.write_word(ENTRY + 0x14, 1).unwrap();
        b.write_word(ROW, FRAME).unwrap();
        glass_write_vector(&mut b, ROW + 4, [1., 0., 0.]).unwrap();
        glass_write_float(&mut b, ROW + 0x14, 2.).unwrap();
        glass_write_vector(&mut b, ROW + 0x24, [0., 1., 0.]).unwrap();
        for i in [0x30, 0x31, 0x32] {
            b.write_byte(ROW + i, 1).unwrap();
        }
        b.write_byte(ROW + 0x33, 0xa5).unwrap();
        glass_write_vector(&mut b, FRAME + 0x80, [0., 10., 0.]).unwrap();
        for (i, x) in SMatrix::identity().elements.into_iter().enumerate() {
            b.write_word(FRAME + 0x50 + i as u32 * 4, x.to_bits())
                .unwrap();
        }
        glass_write_vector(&mut b, FRAME + 0x80, [0., 10., 0.]).unwrap();
        b.write_word(FRAME + 0x9c, 1_f32.to_bits()).unwrap();
        b.write_word(FRAME + 0xac, 0x30).unwrap();
        b.write_word(0x63a200, 1).unwrap();
        b
    }
    #[test]
    fn flight_integrates_canonical_velocity_position_and_spin_then_publishes_in_source_order() {
        let mut b = setup();
        glass_tick(&mut b, MANAGER, 16).unwrap();
        let velocity = glass_read_vector(&mut b, ROW + 4).unwrap();
        let p = glass_read_vector(&mut b, FRAME + 0x80).unwrap();
        assert_eq!(velocity, [1., -0.128, 0.]);
        assert_eq!(p, [0.016, 9.997952, 0.]);
        assert_eq!(b.rolls, [0.032]);
        assert_eq!(b.events, ["direction", "world", "sector", "update"]);
        assert_eq!(b.read_byte(ROW + 0x33).unwrap(), 0xa5);
        assert_eq!(b.read_word(0x63a200).unwrap(), 1);
    }
    #[test]
    fn low_speed_plane_contact_stops_then_settles_from_real_query_without_a_lifetime_delete() {
        let mut b = setup();
        glass_write_vector(&mut b, ROW + 4, [0.; 3]).unwrap();
        glass_write_vector(&mut b, FRAME + 0x80, [0., -0.1, 0.]).unwrap();
        glass_tick(&mut b, MANAGER, 16).unwrap();
        assert_eq!(b.read_byte(ROW + 0x31).unwrap(), 0);
        assert_eq!(b.read_byte(ROW + 0x30).unwrap(), 1);
        assert!(b.events.is_empty());
        assert!((glass_read_float(&mut b, FRAME + 0x84).unwrap() - 0.02).abs() < 1e-6);
        glass_tick(&mut b, MANAGER, 16).unwrap();
        assert_eq!(b.read_byte(ROW + 0x30).unwrap(), 0);
        assert_eq!(glass_read_float(&mut b, FRAME + 0x84).unwrap(), 0.05);
        assert_eq!(b.queries[0].1, [0., -500., 0.]);
        assert_eq!(b.queries[0].2, 0xffff);
        assert_eq!(b.events, ["line", "world", "sector", "direction", "update"]);
        assert_eq!(b.rolls, [0.]);
        assert_eq!(b.read_word(MANAGER + 0x18).unwrap(), ENTRY + 24);
    }
    #[test]
    fn missed_plane_requery_retains_native_outputs_and_does_not_substitute_ground_zero() {
        let mut b = setup();
        b.hit = false;
        b.write_byte(ROW + 0x32, 0).unwrap();
        glass_tick(&mut b, MANAGER, 16).unwrap();
        assert_eq!(b.queries.len(), 1);
        let q = b.queries[0];
        assert_eq!(q.0, [0., 10., 0.]);
        assert_eq!(q.4, [0., -9990., 0.]);
        assert_eq!(q.5, [0., 1., 0.]);
        assert_eq!(
            glass_read_float(&mut b, ROW + 0x1c).unwrap(),
            -9990_f32 + 0.05
        );
        assert_eq!(b.read_byte(ROW + 0x32).unwrap(), 1);
        assert_eq!(b.read_byte(ROW + 0x31).unwrap(), 1);
    }
    #[test]
    fn energetic_plane_bounce_consumes_two_canonical_random_draws_and_rearms_plane_query() {
        let mut b = setup();
        glass_write_vector(&mut b, ROW + 4, [0., -4., 0.]).unwrap();
        glass_write_vector(&mut b, FRAME + 0x80, [0., -0.1, 0.]).unwrap();
        glass_tick(&mut b, MANAGER, 16).unwrap();
        let velocity = glass_read_vector(&mut b, ROW + 4).unwrap();
        assert!(velocity[1] > 0. && velocity[2] > 0.);
        let magnitude = length_xyz::<VehicleMemoryError>(velocity).unwrap();
        assert!((magnitude - 1.6512).abs() < 1e-5);
        assert_eq!(b.read_word(0x63a200).unwrap(), 3357800067);
        assert_eq!(b.read_byte(ROW + 0x30).unwrap(), 1);
        assert_eq!(b.read_byte(ROW + 0x31).unwrap(), 1);
        assert_eq!(b.read_byte(ROW + 0x32).unwrap(), 0);
        assert_eq!(b.rolls, [0.032]);
        assert_eq!(b.events, ["direction", "world", "sector", "update"]);
    }
    #[test]
    fn unsigned_time_clamp_matches_one_and_eighty_three_millisecond_source_limits() {
        for (input, expected) in [(0, 1), (u32::MAX, 83)] {
            let mut a = setup();
            let mut b = setup();
            glass_tick(&mut a, MANAGER, input).unwrap();
            glass_tick(&mut b, MANAGER, expected).unwrap();
            for base in [ROW, FRAME] {
                let len = if base == ROW { 52 } else { 0x200 };
                let av: Vec<_> = (0..len).map(|o| a.read_byte(base + o).unwrap()).collect();
                let bv: Vec<_> = (0..len).map(|o| b.read_byte(base + o).unwrap()).collect();
                assert_eq!(av, bv);
            }
            assert_eq!(a.events, b.events);
        }
    }
}
