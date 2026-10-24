//! Original `C_Vehicle::UpdateAxis`, PC RVA 0xd6840.
//!
//! Record collections, wheel identities and frame fields are live backend
//! storage. The arithmetic domain is Game's PC24/RN finite normal-or-zero
//! domain; an unavailable operation retains earlier frame writes and calls.
use super::*;
use crate::native_matrix::{checked_f32_add, checked_f32_mul};

/// Actual frame calls made by UpdateAxis. These are required dependencies,
/// not callbacks that stand in for the axis record walk or its arithmetic.
pub trait VehicleAxisUpdateBackend: VehicleComponentTailBackend {
    /// Frame virtual +0x24, original SetOn.
    fn set_axis_frame_on(&mut self, frame: u32, on: u32) -> Result<(), Self::Error>;
    /// Frame virtual +0x0c, original SetDir (RVA0x1ab50), roll exactly zero.
    /// This is distinct from SetWorldDir and matrix SetDir3.
    fn set_axis_frame_direction(
        &mut self,
        frame: u32,
        direction: [u32; 3],
    ) -> Result<(), Self::Error>;
    /// Imported S_matrix::SetDir3 at frame+0x50. Preserve its reached basis
    /// writes on unavailability; translation and homogeneous fields survive.
    fn set_axis_matrix_direction(
        &mut self,
        frame: u32,
        direction: [u32; 3],
        up: [u32; 3],
    ) -> Result<(), Self::Error>;
}

type AxisResult<B, T> = Result<T, VehicleWheelsUnavailable<<B as VehicleInitBackend>::Error>>;
fn add(a: f32, b: f32) -> Result<f32, ()> {
    checked_f32_add(a, b).map_err(|_| ())
}
fn mul(a: f32, b: f32) -> Result<f32, ()> {
    checked_f32_mul(a, b).map_err(|_| ())
}
fn sub(a: f32, b: f32) -> Result<f32, ()> {
    add(a, -b)
}
fn squared(v: [f32; 3], order: [usize; 3]) -> Result<f32, ()> {
    add(
        add(
            mul(v[order[0]], v[order[0]])?,
            mul(v[order[1]], v[order[1]])?,
        )?,
        mul(v[order[2]], v[order[2]])?,
    )
}
fn normalize(mut v: [f32; 3], order: [usize; 3]) -> Result<[f32; 3], ()> {
    let n = squared(v, order)?;
    if f64::from(sub(n, 1.)?.abs()) < f64::from(1e-8_f32) {
        return Ok(v);
    }
    if n < 1e-8_f32 {
        let axis = if v[0] != 0. {
            0
        } else if v[2] != 0. {
            2
        } else {
            1
        };
        v[axis] = if v[axis] < 0. { -1. } else { 1. };
    } else {
        let root = n.sqrt();
        crate::native_precision::record_sqrt(n, root);
        let inverse = 1. / root;
        crate::native_precision::record_div(1., root, inverse);
        if !root.is_normal() || !inverse.is_normal() {
            return Err(());
        }
        for value in &mut v {
            *value = mul(*value, inverse)?;
        }
    }
    Ok(v)
}
fn length(v: [f32; 3], order: [usize; 3]) -> Result<f32, ()> {
    let n = squared(v, order)?;
    if (n.to_bits() as i32) < 0x322b_cc77 {
        return Ok(0.);
    }
    if f64::from(sub(n, 1.)?.abs()) < f64::from(1e-8_f32) {
        return Ok(1.);
    }
    let result = n.sqrt();
    crate::native_precision::record_sqrt(n, result);
    if result == 0. || result.is_normal() {
        Ok(result)
    } else {
        Err(())
    }
}
// Preserve the native multiply/subtract order, including signed zero.
fn up_vector(v: [f32; 3], left: bool) -> Result<[f32; 3], ()> {
    let zero = mul(v[2], 0.)?;
    let sign = if left { 1. } else { -1. };
    Ok([
        sub(mul(v[1], sign)?, zero)?,
        sub(zero, mul(v[0], sign)?)?,
        sub(mul(v[0], 0.)?, mul(v[1], 0.)?)?,
    ])
}

struct AxisUpdate<'a> {
    memory: VehicleComponentTailMemory<'a>,
    vehicle: u32,
}
impl AxisUpdate<'_> {
    fn word<B: VehicleInitBackend>(&mut self, b: &mut B, address: u32) -> AxisResult<B, u32> {
        self.memory
            .read_word_with_backend(b, address)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn byte<B: VehicleInitBackend>(&mut self, b: &mut B, address: u32) -> AxisResult<B, u8> {
        self.memory
            .read_byte_with_backend(b, address)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn store<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        address: u32,
        value: u32,
    ) -> AxisResult<B, ()> {
        self.memory
            .write_word_with_backend(b, address, value)
            .map_err(|e| VehicleInitUnavailable::Effect(e).into())
    }
    fn relative_word<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        offset: u32,
    ) -> AxisResult<B, u32> {
        self.word(b, self.vehicle.wrapping_add(offset))
    }
    fn update_axis_vector<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        address: u32,
    ) -> AxisResult<B, [f32; 3]> {
        let mut out = [0.; 3];
        for (i, value) in out.iter_mut().enumerate() {
            *value = f32::from_bits(self.word(b, address.wrapping_add(i as u32 * 4))?);
        }
        Ok(out)
    }
    fn update_axis_count<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        begin: u32,
        end: u32,
    ) -> AxisResult<B, u32> {
        let pointer = self.relative_word(b, begin)?;
        Ok(if pointer == 0 {
            0
        } else {
            ((self.relative_word(b, end)?.wrapping_sub(pointer) as i32) / 24) as u32
        })
    }
    fn update_axis_record<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        header: u32,
        offset: u32,
    ) -> AxisResult<B, u32> {
        let begin = self.relative_word(b, header)?;
        Ok(begin.wrapping_add(offset))
    }
    fn update_axis_wheel<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        index: u32,
    ) -> AxisResult<B, u32> {
        let order = self.relative_word(b, 0xc38)?;
        self.word(b, order.wrapping_add(index.wrapping_mul(4)))
    }
    fn update_axis_wheel_frame<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        index: u32,
    ) -> AxisResult<B, u32> {
        let wheel = self.update_axis_wheel(b, index)?;
        self.word(b, wheel.wrapping_add(8))
    }
    fn update_axis_anchor<B: VehicleAxisUpdateBackend>(
        &mut self,
        b: &mut B,
        index: u32,
    ) -> AxisResult<B, [f32; 3]> {
        use VehicleInitUnavailable::Effect;
        let frame = self.update_axis_wheel_frame(b, index)?;
        b.prepare_frame_matrix(frame).map_err(Effect)?;
        let wheel = self.update_axis_wheel(b, index)?;
        let source = self.word(b, wheel.wrapping_add(4))?;
        let mut projection = super::component_tail::Projection {
            backend: b,
            memory: self.memory.reborrow(),
        };
        let result = super::components::transform_sources(
            &mut projection,
            super::components::MatrixSource::External(frame.wrapping_add(0x50)),
            super::components::PointSource::External(source.wrapping_add(0x80)),
        )
        .map_err(|error| match error {
            VehicleComponentsUnavailable::Init(e) => VehicleWheelsUnavailable::Init(e),
            _ => VehicleWheelsUnavailable::NumericDomain,
        })?;
        Ok(result.map(f32::from_bits))
    }
    fn update_axis_scale<B: VehicleAxisUpdateBackend>(
        &mut self,
        b: &mut B,
        frame: u32,
        z: f32,
    ) -> AxisResult<B, ()> {
        use VehicleInitUnavailable::Effect;
        for (offset, value) in [(0x90, 1f32), (0x94, 1f32), (0x98, z)] {
            self.store(b, frame.wrapping_add(offset), value.to_bits())?;
        }
        let flags = self.word(b, frame.wrapping_add(0xac))?;
        if flags & 8 == 0 {
            b.materialize_frame_quaternion(frame).map_err(Effect)?;
        }
        let flags = self.word(b, frame.wrapping_add(0xac))?;
        self.store(
            b,
            frame.wrapping_add(0xac),
            (flags & 0xffff_fecb) | 0x4000_0008,
        )
    }
    // The clear-0x10 branch returns the newly materialized Z row directly.
    // The set branch normalizes a copy, without overwriting the frame's row.
    fn update_axis_z<B: VehicleAxisUpdateBackend>(
        &mut self,
        b: &mut B,
        frame: u32,
        order: [usize; 3],
    ) -> AxisResult<B, [f32; 3]> {
        use VehicleInitUnavailable::Effect;
        let flags = self.word(b, frame.wrapping_add(0xac))?;
        if flags & 0x10 == 0 {
            self.store(b, frame.wrapping_add(0xac), flags | 0x10)?;
            b.normalize_frame_quaternion(frame).map_err(Effect)?;
            b.set_frame_rotation_matrix(frame).map_err(Effect)?;
            self.update_axis_vector(b, frame.wrapping_add(0x70))
        } else {
            normalize(self.update_axis_vector(b, frame.wrapping_add(0x70))?, order)
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)
        }
    }
    fn update_axis_basis<B: VehicleAxisUpdateBackend>(
        &mut self,
        b: &mut B,
        frame: u32,
        direction: [f32; 3],
        up: [f32; 3],
    ) -> AxisResult<B, ()> {
        b.set_axis_matrix_direction(frame, direction.map(f32::to_bits), up.map(f32::to_bits))
            .map_err(VehicleInitUnavailable::Effect)?;
        let flags = self.word(b, frame.wrapping_add(0xac))?;
        self.store(
            b,
            frame.wrapping_add(0xac),
            (flags & 0xffff_fed3) | 0x4000_0010,
        )
    }
    /// Executes both reverse axis-record passes and the original enable-state
    /// transition. Existing wheel order and AXIS records must be genuine Init
    /// output. A normal return here does not complete the parent Init.
    fn update_axes<B: VehicleAxisUpdateBackend>(&mut self, b: &mut B) -> AxisResult<B, ()> {
        use VehicleInitUnavailable::Effect;
        let current = self.relative_word(b, 0xc68)? & 1;
        if self.relative_word(b, 0xc6c)? & 1 != current {
            for (begin, end) in [(0x340, 0x344), (0x330, 0x334)] {
                let mut remaining = self.update_axis_count(b, begin, end)?;
                let mut offset = remaining.wrapping_mul(24);
                while remaining != 0 {
                    offset = offset.wrapping_sub(24);
                    let record = self.update_axis_record(b, begin, offset)?;
                    let frame = self.word(b, record)?;
                    b.set_axis_frame_on(frame, current).map_err(Effect)?;
                    remaining = remaining.wrapping_sub(1);
                }
            }
        }
        if self.relative_word(b, 0xc68)? & 1 == 0 {
            return Ok(());
        }
        let mut remaining = self.update_axis_count(b, 0x340, 0x344)?;
        let mut offset = remaining.wrapping_mul(24);
        while remaining != 0 {
            offset = offset.wrapping_sub(24);
            let record = self.update_axis_record(b, 0x340, offset)?;
            let left = self.byte(b, record.wrapping_add(0x14))? != 0;
            let axle = self.word(b, record.wrapping_add(4))?;
            let index = axle.wrapping_mul(2).wrapping_add(u32::from(!left));
            let anchor = self.update_axis_anchor(b, index)?;
            let displacement = self.update_axis_vector(b, record.wrapping_add(8))?;
            let frame = self.word(b, record)?;
            let position = self.update_axis_vector(b, frame.wrapping_add(0x80))?;
            let mut direction = [0.; 3];
            for i in [2, 1, 0] {
                direction[i] = add(displacement[i], anchor[i])
                    .and_then(|v| sub(v, position[i]))
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            }
            let distance = length(direction, [0, 1, 2])
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let record = self.update_axis_record(b, 0x340, offset)?;
            let frame = self.word(b, record)?;
            self.update_axis_scale(b, frame, distance)?;
            let direction = normalize(direction, [0, 2, 1])
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let record = self.update_axis_record(b, 0x340, offset)?;
            let frame = self.word(b, record)?;
            b.set_axis_frame_direction(frame, direction.map(f32::to_bits))
                .map_err(Effect)?;
            let record = self.update_axis_record(b, 0x340, offset)?;
            if self.byte(b, record.wrapping_add(0x15))? != 0 {
                let left = self.byte(b, record.wrapping_add(0x14))? != 0;
                let up = up_vector(direction, left)
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
                let frame = self.update_axis_wheel_frame(b, index)?;
                let z = self.update_axis_z(b, frame, if left { [0, 1, 2] } else { [1, 2, 0] })?;
                let frame = self.update_axis_wheel_frame(b, index)?;
                self.update_axis_basis(b, frame, z, up)?;
            }
            remaining = remaining.wrapping_sub(1);
        }
        let mut remaining = self.update_axis_count(b, 0x330, 0x334)?;
        let mut offset = remaining.wrapping_mul(24);
        while remaining != 0 {
            offset = offset.wrapping_sub(24);
            let record = self.update_axis_record(b, 0x330, offset)?;
            let index = self.word(b, record.wrapping_add(4))?.wrapping_mul(2);
            let mut displacement = self.update_axis_vector(b, record.wrapping_add(8))?;
            displacement[0] = -displacement[0];
            let anchor = self.update_axis_anchor(b, index.wrapping_add(1))?;
            let mut right = [0.; 3];
            for i in [2, 1, 0] {
                right[i] = add(displacement[i], anchor[i])
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            }
            let record = self.update_axis_record(b, 0x330, offset)?;
            let anchor = self.update_axis_anchor(b, index)?;
            let displacement = self.update_axis_vector(b, record.wrapping_add(8))?;
            let mut delta = [0.; 3];
            for i in [2, 0, 1] {
                delta[i] = add(displacement[i], anchor[i])
                    .and_then(|v| sub(v, right[i]))
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            }
            let right_frame = self.update_axis_wheel_frame(b, index.wrapping_add(1))?;
            let left_frame = self.update_axis_wheel_frame(b, index)?;
            let right_position = self.update_axis_vector(b, right_frame.wrapping_add(0x80))?;
            let left_position = self.update_axis_vector(b, left_frame.wrapping_add(0x80))?;
            let mut direction = [0.; 3];
            for i in [2, 1, 0] {
                direction[i] = sub(left_position[i], right_position[i])
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            }
            let record = self.update_axis_record(b, 0x330, offset)?;
            let frame = self.word(b, record)?;
            if self.byte(b, frame.wrapping_add(0xac))? & 0x80 == 0 {
                for (i, value) in right.iter().enumerate() {
                    self.store(b, frame.wrapping_add(0x80 + i as u32 * 4), value.to_bits())?;
                }
                let flags = self.word(b, frame.wrapping_add(0xac))?;
                self.store(
                    b,
                    frame.wrapping_add(0xac),
                    (flags & 0xffff_fedf) | 0x4000_0000,
                )?;
            }
            let distance =
                length(delta, [1, 0, 2]).map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let record = self.update_axis_record(b, 0x330, offset)?;
            let frame = self.word(b, record)?;
            self.update_axis_scale(b, frame, distance)?;
            let direction = normalize(direction, [2, 1, 0])
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            let record = self.update_axis_record(b, 0x330, offset)?;
            let frame = self.word(b, record)?;
            b.set_axis_frame_direction(frame, direction.map(f32::to_bits))
                .map_err(Effect)?;
            let record = self.update_axis_record(b, 0x330, offset)?;
            if self.byte(b, record.wrapping_add(0x15))? != 0 {
                let up = up_vector(direction, true)
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
                let frame = self.update_axis_wheel_frame(b, index)?;
                let z = self.update_axis_z(b, frame, [1, 2, 0])?;
                let frame = self.update_axis_wheel_frame(b, index)?;
                self.update_axis_basis(b, frame, z, up)?;
                let up = up_vector(direction, true)
                    .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
                let frame = self.update_axis_wheel_frame(b, index.wrapping_add(1))?;
                let z = self.update_axis_z(b, frame, [1, 2, 0])?;
                let frame = self.update_axis_wheel_frame(b, index.wrapping_add(1))?;
                self.update_axis_basis(b, frame, z, up)?;
            }
            remaining = remaining.wrapping_sub(1);
        }
        Ok(())
    }
}

/// UpdateAxis on an existing live car/vehicle projection. No temporary vehicle
/// owner or copyback is created, including at GameInit and simulation callsites.
pub fn update_vehicle_axes<B: VehicleAxisUpdateBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    vehicle: u32,
) -> AxisResult<B, ()> {
    AxisUpdate { memory, vehicle }.update_axes(backend)
}
impl VehicleInitState {
    pub fn update_axes<B: VehicleAxisUpdateBackend>(
        &mut self,
        backend: &mut B,
    ) -> AxisResult<B, ()> {
        let vehicle = self
            .tail_vehicle_address
            .ok_or(VehicleWheelsUnavailable::NoContinuation)?;
        update_vehicle_axes(backend, self.live_tail_memory(), vehicle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axis_normalization_tracks_reciprocal_and_length_tracks_sqrt() {
        let mut exact = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            exact.run(|| normalize([0., 0., 2.], [0, 1, 2])).unwrap(),
            [0., 0., 1.]
        );
        assert!(!exact.precision_raised());
        // Squared length 9, square root 3 and all final products by 1 or 2
        // are exact. Only the original reciprocal raises precision here.
        let mut reciprocal = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            reciprocal
                .run(|| normalize([1., 2., 2.], [0, 1, 2]))
                .unwrap(),
            [1. / 3., 2. / 3., 2. / 3.]
        );
        assert!(reciprocal.precision_raised());
        let mut sqrt = crate::native_precision::Pc24PrecisionState::default();
        assert_eq!(
            sqrt.run(|| length([1., 1., 0.], [0, 1, 2])).unwrap(),
            2_f32.sqrt()
        );
        assert!(sqrt.precision_raised());
    }
}
