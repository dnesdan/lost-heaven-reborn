//! Addressable C_car component helpers. Names in this module are descriptive
//! working names; integer resource identifiers are not owned object handles.

#[path = "car_effect_records.rs"]
mod records;
pub use records::{car_component_records_insert, CAR_COMPONENT_RECORD_BYTES};
#[path = "car_resource_tracks.rs"]
mod tracks;
pub use tracks::{CarAnimationResource, CarTrack, CarTrackError, CarTrackResources};

use crate::vehicle_init::{VehicleComponentTailMemory, VehicleInitBackend};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarEffectsUnavailable<E> {
    Effect(E),
    NumericDomain,
}

/// Required frame and Game animation effects. There are no success defaults.
pub trait CarEffectsBackend: VehicleInitBackend {
    fn car_normalize_frame_quaternion(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn car_set_frame_rotation_matrix(&mut self, frame: u32) -> Result<(), Self::Error>;
    fn car_animation_name(&mut self, id: u32) -> Result<Option<Vec<u8>>, Self::Error>;
    fn car_track_placement(
        &mut self,
        memory: Option<VehicleComponentTailMemory<'_>>,
        name: &[u8],
        position: u32,
        direction: u32,
    ) -> Result<bool, Self::Error>;
}

/// PC entry 0x41dc30 (RVA 0x1dc30), corresponding to original GetSeatProperty.
/// All four outputs are single bytes, including the caller's dword stack slots.
pub fn car_get_seat_property<B: CarEffectsBackend>(
    b: &mut B,
    car: u32,
    index: u32,
    output0: u32,
    output1: u32,
    output2: u32,
    output3: u32,
) -> Result<bool, CarEffectsUnavailable<B::Error>> {
    use CarEffectsUnavailable::Effect;
    if (index as i32) < 0 {
        return Ok(false);
    }
    let begin = b.read_word(car.wrapping_add(0x21cc)).map_err(Effect)?;
    if begin == 0 {
        return Ok(false);
    }
    let end = b.read_word(car.wrapping_add(0x21d0)).map_err(Effect)?;
    let count = ((end.wrapping_sub(begin) as i32) / 160) as u32;
    if index >= count {
        return Ok(false);
    }
    let offset = index.wrapping_mul(160);
    let begin = b.read_word(car.wrapping_add(0x21cc)).map_err(Effect)?;
    if b.read_byte(begin.wrapping_add(offset).wrapping_add(12))
        .map_err(Effect)?
        == 0
    {
        return Ok(false);
    }
    let seat_offset = index.wrapping_mul(36);
    let seats = b.read_word(car.wrapping_add(0xd14)).map_err(Effect)?;
    let side = f32::from_bits(
        b.read_word(seats.wrapping_add(seat_offset))
            .map_err(Effect)?,
    );
    b.write_byte(output0, u8::from(side > 0.)).map_err(Effect)?;
    let begin = b.read_word(car.wrapping_add(0x21cc)).map_err(Effect)?;
    let small = b
        .read_byte(begin.wrapping_add(offset).wrapping_add(13))
        .map_err(Effect)?;
    b.write_byte(output2, u8::from(small == 0))
        .map_err(Effect)?;
    b.write_byte(output1, 1).map_err(Effect)?;
    b.write_byte(output3, 0).map_err(Effect)?;
    let seats = b.read_word(car.wrapping_add(0xd14)).map_err(Effect)?;
    let door = b
        .read_word(seats.wrapping_add(seat_offset).wrapping_add(32))
        .map_err(Effect)?;
    if door != 0 {
        let frame = b.read_word(door).map_err(Effect)?;
        let flags = b.read_word(frame.wrapping_add(0xac)).map_err(Effect)?;
        let z = if flags & 0x10 == 0 {
            b.write_word(frame.wrapping_add(0xac), flags | 0x10)
                .map_err(Effect)?;
            b.car_normalize_frame_quaternion(frame).map_err(Effect)?;
            b.car_set_frame_rotation_matrix(frame).map_err(Effect)?;
            // The original still loads all three row components here.
            let _x = b.read_word(frame.wrapping_add(0x70)).map_err(Effect)?;
            let _y = b.read_word(frame.wrapping_add(0x74)).map_err(Effect)?;
            f32::from_bits(b.read_word(frame.wrapping_add(0x78)).map_err(Effect)?)
        } else {
            let x = f32::from_bits(b.read_word(frame.wrapping_add(0x70)).map_err(Effect)?);
            let y = f32::from_bits(b.read_word(frame.wrapping_add(0x74)).map_err(Effect)?);
            let z = f32::from_bits(b.read_word(frame.wrapping_add(0x78)).map_err(Effect)?);
            crate::native_transform::native_trig::car_resource_math::normalized_z([x, y, z])
                .map_err(|_| CarEffectsUnavailable::NumericDomain)?
        };
        if z < 0. || b.read_byte(door.wrapping_add(0x30)).map_err(Effect)? & 4 != 0 {
            b.write_byte(output1, 0).map_err(Effect)?;
        }
        let control = f32::from_bits(b.read_word(door.wrapping_add(0x20)).map_err(Effect)?);
        if control > 0. {
            b.write_byte(output3, 1).map_err(Effect)?;
        }
    }
    Ok(true)
}

/// PC entry 0x41c550 (RVA 0x1c550), corresponding to GetPlacementFromTCK.
/// A missing track is the nested loader's false result, which this caller
/// ignores before adding the component's local position and returning true.
pub fn car_get_placement_from_tck<B: CarEffectsBackend>(
    b: &mut B,
    car: u32,
    index: u32,
    animation_id: u32,
    position: u32,
    direction: u32,
) -> Result<bool, CarEffectsUnavailable<B::Error>> {
    use crate::native_transform::{native_trig::car_resource_math as math, SMatrix};
    use CarEffectsUnavailable::Effect;
    let offset = (index & 0xffff).wrapping_mul(160);
    let begin = b.read_word(car.wrapping_add(0x21cc)).map_err(Effect)?;
    if b.read_byte(begin.wrapping_add(offset).wrapping_add(12))
        .map_err(Effect)?
        == 0
    {
        return Ok(false);
    }
    let Some(mut name) = b.car_animation_name(animation_id).map_err(Effect)? else {
        return Ok(false);
    };
    if let Some(dot) = name.iter().rposition(|byte| *byte == b'.') {
        name.truncate(dot);
    }
    name.extend_from_slice(b".tck");
    let _nested_result = b
        .car_track_placement(None, &name, position, direction)
        .map_err(Effect)?;
    let begin = b.read_word(car.wrapping_add(0x21cc)).map_err(Effect)?;
    let record = begin.wrapping_add(offset);
    for field in [0u32, 4, 8] {
        let local = f32::from_bits(b.read_word(record.wrapping_add(field)).map_err(Effect)?);
        let value = f32::from_bits(b.read_word(position.wrapping_add(field)).map_err(Effect)?);
        let value = math::add(local, value).map_err(|_| CarEffectsUnavailable::NumericDomain)?;
        b.write_word(position.wrapping_add(field), value.to_bits())
            .map_err(Effect)?;
    }
    if index & 0xffff0000 == 0 {
        let mut matrix = SMatrix::identity();
        for field in 0..16 {
            matrix.elements[field] = f32::from_bits(
                b.read_word(car.wrapping_add(0xd58 + field as u32 * 4))
                    .map_err(Effect)?,
            );
        }
        let mut vector = [0.; 3];
        for (field, value) in vector.iter_mut().enumerate() {
            *value = f32::from_bits(
                b.read_word(position.wrapping_add(field as u32 * 4))
                    .map_err(Effect)?,
            );
        }
        let transformed = matrix
            .try_transform_point(vector)
            .map_err(|_| CarEffectsUnavailable::NumericDomain)?;
        for (field, value) in transformed.into_iter().enumerate() {
            b.write_word(position.wrapping_add(field as u32 * 4), value.to_bits())
                .map_err(Effect)?;
        }
        for field in 0..16 {
            matrix.elements[field] = f32::from_bits(
                b.read_word(car.wrapping_add(0xd58 + field as u32 * 4))
                    .map_err(Effect)?,
            );
        }
        matrix.elements[12..15].fill(0.);
        for (field, value) in vector.iter_mut().enumerate() {
            *value = f32::from_bits(
                b.read_word(direction.wrapping_add(field as u32 * 4))
                    .map_err(Effect)?,
            );
        }
        let transformed = matrix
            .try_transform_point(vector)
            .map_err(|_| CarEffectsUnavailable::NumericDomain)?;
        for (field, value) in transformed.into_iter().enumerate() {
            b.write_word(direction.wrapping_add(field as u32 * 4), value.to_bits())
                .map_err(Effect)?;
        }
    }
    Ok(true)
}

/// PC entry 0x57dfc0 (RVA 0x17dfc0): select an integer resource-table identifier.
/// `cl` and `dl` are the two byte register arguments. Only the low byte of
/// stack arguments zero, two and three is observed; argument one is unused.
pub fn get_anim_id_for_use_car(
    cl: u8,
    dl: u8,
    argument0: u32,
    _argument1: u32,
    argument2: u32,
    argument3: u32,
) -> u32 {
    if argument3 as u8 != 0 {
        0x13c + u32::from(argument2 as u8 != 0) + 2 * u32::from(cl != 0)
    } else {
        let mut flags = u32::from(cl != 0);
        if dl == 0 {
            flags |= 2;
        }
        if argument0 as u8 != 0 {
            flags |= 4;
        }
        if argument2 as u8 != 0 {
            flags |= 0x10;
        }
        0x10f + flags
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selector_fixed_branch_and_argument_width_cases() {
        for (arguments, expected) in [
            ((0, 0, 0, 0, 0, 0), 0x111),
            ((1, 1, 0, 0, 0, 0), 0x110),
            ((0, 1, 1, 0, 0, 0), 0x113),
            ((0, 1, 0, 0, 1, 0), 0x11f),
            ((0xff, 0, 0xff, 0x1234, 0xff, 0), 0x126),
            ((0, 1, 0xffff_ff00, 0xffff_ffff, 0xffff_ff00, 0x100), 0x10f),
            ((0, 0, 0, 0, 0, 1), 0x13c),
            ((0, 0, 0, 0, 1, 1), 0x13d),
            ((0x80, 0, 0, 0, 0, 1), 0x13e),
            ((1, 1, 0xffff_ffff, 0xffff_ffff, 1, 1), 0x13f),
        ] {
            let (cl, dl, a, b, c, d) = arguments;
            assert_eq!(get_anim_id_for_use_car(cl, dl, a, b, c, d), expected);
        }
    }
}
