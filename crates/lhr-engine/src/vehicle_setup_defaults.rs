//! Default source-parameter construction within original Setup, PC
//! 0x4c0654..0x4c0f34. The helper writes the same addressable stack record and
//! re-reads the live axle count at its original loop boundary.
use super::VehicleInitBackend;
fn words<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    stack: u32,
    values: &[(u32, u32)],
) -> Result<(), B::Error> {
    for &(offset, value) in values {
        b.write_word(stack.wrapping_add(offset), value)?;
    }
    Ok(())
}
/// Descriptive inline helper; not an independently named original function.
/// `stack` is Setup ESP after its four saved registers. Source begins at+a4.
/// This prepares, but does not invoke, InitCar with original flags zero.
pub fn vehicle_setup_default_source<B: VehicleInitBackend + ?Sized>(
    b: &mut B,
    vehicle: u32,
    stack: u32,
) -> Result<(), B::Error> {
    for word in 0..0x449u32 {
        b.write_word(stack.wrapping_add(0xa4 + word * 4), 0)?;
    }
    let axles = b.read_word(vehicle.wrapping_add(0x4c0))?;
    words(
        b,
        stack,
        &[
            (0xa4, 0x44af0000),
            (0xa8, 0x43020000),
            (0xac, 0x42a00000),
            (0xb0, 0x45834000),
            (0xb4, 0x458fc000),
            (0xb8, 0x44098000),
            (0xbc, 0x43700000),
            (0xc0, 0x42a00000),
            (0xc4, 0x43200000),
            (0xc8, 0x452f0000),
            (0xcc, 0x45480000),
            (0xd0, 0x42c80000),
            (0xd4, 0x3e75c28f),
            (0xd8, 0x43aa0000),
            (0xdc, 0x3e800000),
            (0xe0, 0x3f19999a),
            (0xe4, 0x44098000),
            (0xe8, 0x44098000),
            (0xec, 0x452f0000),
            (0xf0, 0x40800000),
            (0xf4, 0x41600000),
            (0xf8, 0x41600000),
            (0xfc, 0x411ccccd),
            (0x100, 0x40d00000),
            (0x104, 0x40900000),
            (0x124, 0x42960000),
            (0x128, 0x4297999a),
            (0x12c, 0x42a06666),
            (0x154, 0x41a40000),
            (0x158, 0x41a73333),
            (0x15c, 0x41a40000),
            (0x178, 0x3f19999a),
            (0x17c, 0x3fd0a3d7),
            (0x180, 0x3f99999a),
            (0x184, 0x3fb33333),
            (0x188, 0x3f99999a),
            (0x18c, 0x3fb33333),
            (0x190, 0x42200000),
            (0x194, 0x41f00000),
            (0x198, 0x41200000),
            (0x19c, 0x42700000),
            (0x1a0, 0x42c80000),
            (0x1a4, 0x42700000),
            (0x1a8, 0x42c80000),
            (0x1ac, 0x3f000000),
            (0x1b0, 0x3e4ccccd),
            (0x1b4, 0x3f000000),
            (0x1bc, 0x42480000),
            (0x1c0, 0x418c0000),
            (0x694, 0x00000014),
        ],
    )?;
    for offset in [0x698, 0x699, 0x69a] {
        b.write_byte(stack.wrapping_add(offset), 0)?;
    }
    b.write_word(stack.wrapping_add(0x69c), 0x3f800000)?;
    b.write_byte(stack.wrapping_add(0x6a0), 1)?;
    b.write_word(stack.wrapping_add(0x6a4), 0)?;
    words(
        b,
        stack,
        &[
            (0x6a8, 0x420c0000),
            (0x6ac, 0x3e4ccccd),
            (0x6b0, 0x3fb33333),
            (0x6b4, 0),
            (0x6b8, 0x40400000),
            (0x6bc, 0xc0800000),
            (0x6c0, 0x40000000),
        ],
    )?;
    if axles as i32 > 0 {
        b.write_word(stack.wrapping_add(0x48), axles.wrapping_sub(1))?;
        let mut index = 0u32;
        loop {
            let address = stack.wrapping_add(index.wrapping_mul(4));
            words(
                b,
                address,
                &[
                    (0x1c4, 0x42480000),
                    (0x1ec, 0x3e4ccccd),
                    (0x214, 0x44480000),
                    (0x23c, 0x44fa0000),
                    (0x264, 0),
                    (0x28c, 0x41400000),
                    (0x2b4, 0x41100000),
                    (0x2dc, 0x3f666666),
                    (0x304, 0x42200000),
                    (0x32c, 0x3f4ccccd),
                    (0x354, 0x3fc00000),
                    (0x37c, 0x3f4ccccd),
                    (0x3a4, 0x3fc00000),
                    (0x3cc, 0x3ca3d70a),
                    (0x3f4, 0x41400000),
                    (0x41c, 0x41100000),
                    (0x444, 0x3f666666),
                    (0x46c, 0x41f00000),
                    (0x494, 0x3ee66666),
                    (0x4bc, 0x3f8ccccd),
                    (0x4e4, 0x3ee66666),
                    (0x50c, 0x3f8ccccd),
                    (0x534, 0x3dcccccd),
                    (0x55c, 0x42c80000),
                ],
            )?;
            if index == 1 {
                b.write_byte(stack.wrapping_add(0x585), 1)?;
            } else if index == 0 {
                b.write_byte(stack.wrapping_add(0x58e), 1)?;
            }
            if index == b.read_word(stack.wrapping_add(0x48))? {
                b.write_byte(stack.wrapping_add(0x598).wrapping_add(index), 1)?;
            }
            let factor = if (index as i32) < 1 {
                0x42c80000
            } else {
                0x42a00000
            };
            b.write_word(address.wrapping_add(0x5a4), factor)?;
            b.write_word(address.wrapping_add(0x5cc), factor)?;
            let axles = b.read_word(vehicle.wrapping_add(0x4c0))?;
            words(
                b,
                address,
                &[
                    (0x5f4, 0x41a00000),
                    (0x61c, 0x40c00000),
                    (0x644, 0x41880000),
                ],
            )?;
            index = index.wrapping_add(1);
            if index as i32 >= axles as i32 {
                break;
            }
        }
    }
    b.write_byte(stack.wrapping_add(0x6c4), 1)?;
    words(
        b,
        stack,
        &[
            (0x6c8, 0x41a00000),
            (0x6cc, 0x41200000),
            (0x6d0, 0x40000000),
            (0x6d4, 0x3f800000),
            (0x6d8, 0x3ca3d70a),
            (0x1b8, 0x41200000),
            (0x66c, 0x45098000),
            (0x670, 0x44a28000),
            (0x674, 0x45098000),
            (0x67c, 0x3f800000),
            (0x678, 0x40c00000),
            (0x680, 0x41f00000),
            (0x684, 0x3f333333),
        ],
    )?;
    b.write_byte(stack.wrapping_add(0x688), 1)?;
    words(
        b,
        stack,
        &[
            (0x68c, 0x3ea3d70a),
            (0x690, 0x3f800000),
            (0x6dc, 0x430c0000),
            (0x6e0, 0x41a00000),
            (0x6e4, 0x42c80000),
            (0x6e8, 0x42aa0000),
            (0x6ec, 0x3f000000),
            (0x6f0, 0x42700000),
            (0x6f4, 0x41f00000),
            (0x6f8, 0x420c0000),
            (0x6fc, 0x00000000),
            (0x704, 0x42c80000),
            (0x708, 0x41a00000),
            (0x70c, 0x42c80000),
            (0x710, 0x42960000),
            (0x714, 0x42c80000),
            (0x718, 0x41f00000),
            (0x71c, 0x42c80000),
            (0x720, 0x42480000),
            (0x724, 0x41700000),
            (0x728, 0x42700000),
            (0x72c, 0x428c0000),
            (0x730, 0x41200000),
            (0x734, 0x41f00000),
            (0x738, 0x41f00000),
            (0x73c, 0x41f00000),
            (0x740, 0x41f00000),
            (0x744, 0x42c80000),
            (0x748, 0x41200000),
            (0x74c, 0x42c80000),
            (0x750, 0x41f00000),
            (0x754, 0x41200000),
            (0x758, 0x41200000),
            (0x75c, 0x41f00000),
            (0x760, 0x41f00000),
            (0x764, 0x41200000),
            (0x768, 0x42c80000),
            (0x76c, 0x00000000),
            (0x770, 0x40a00000),
            (0x774, 0x40a00000),
            (0x778, 0x40a00000),
            (0x77c, 0x42a00000),
        ],
    )?;
    for (source, destination) in [(0x634944u32, 0x780u32), (0x634950, 0x790)] {
        let mut offset = 0u32;
        loop {
            let byte = b.read_byte(source.wrapping_add(offset))?;
            b.write_byte(stack.wrapping_add(destination).wrapping_add(offset), byte)?;
            offset = offset.wrapping_add(1);
            if byte == 0 {
                break;
            }
        }
    }
    // Original arguments are pushed before the last scalar source stores.
    b.write_word(stack.wrapping_sub(4), 0)?;
    b.write_word(stack.wrapping_sub(8), stack.wrapping_add(0xa4))?;
    words(
        b,
        stack,
        &[
            (0x7a0, 0x42480000),
            (0x7a4, 0x41200000),
            (0x7a8, 0x42c80000),
            (0x7ac, 0x42a00000),
            (0x7b0, 0x41a00000),
            (0x7b4, 0x40a00000),
            (0x7b8, 0x40a00000),
            (0x7bc, 0x40a00000),
            (0x7c0, 0x42480000),
            (0x7c4, 0x41200000),
            (0x7c8, 0x42c80000),
            (0x7cc, 0x41a00000),
            (0x7d0, 0x40a00000),
            (0x7d4, 0x40a00000),
            (0x7d8, 0x40a00000),
            (0x7dc, 0x42480000),
            (0x7e0, 0x41200000),
            (0x7e4, 0x42c80000),
            (0x7e8, 0x41a00000),
            (0x7ec, 0x40a00000),
            (0x7f0, 0x40a00000),
            (0x7f4, 0x40a00000),
            (0x7f8, 0x42480000),
            (0x7fc, 0x41200000),
            (0x800, 0x42c80000),
            (0x804, 0x41a00000),
            (0x808, 0x41200000),
            (0x80c, 0x40a00000),
            (0x810, 0x41200000),
            (0x814, 0x42480000),
            (0x818, 0x41200000),
            (0x81c, 0x42c80000),
            (0x820, 0x41a00000),
            (0x824, 0x41200000),
            (0x828, 0x40a00000),
            (0x82c, 0x40a00000),
            (0x830, 0x42480000),
            (0x834, 0x41200000),
            (0x838, 0x42c80000),
            (0x83c, 0x41a00000),
            (0x840, 0x41200000),
            (0x844, 0x40a00000),
            (0x848, 0x40a00000),
            (0x84c, 0x40a00000),
            (0x850, 0x41200000),
            (0x854, 0x40200000),
            (0x858, 0x00000000),
            (0x85c, 0x41a00000),
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::wheels::tests::Memory;
    use super::*;
    #[test]
    fn default_source_preserves_outer_stack_and_adjusts_stores_after_argument_pushes() {
        let mut backend = Memory::default();
        backend.put(0x0ff8, &[0xa5; 0x1300]);
        backend.word(0x104c0, 3);
        backend.put(0x634944, b"first\0");
        backend.put(0x634950, b"second\0");
        vehicle_setup_default_source(&mut backend, 0x10000, 0x1000).unwrap();
        assert_eq!(backend.get(0x109c), 0xa5a5a5a5);
        assert_eq!(backend.get(0x21c8), 0xa5a5a5a5);
        assert_eq!(backend.get(0x0ffc), 0);
        assert_eq!(backend.get(0x0ff8), 0x10a4);
        assert_eq!(backend.get(0x17a0), 50f32.to_bits());
        assert_eq!(backend.get(0x185c), 20f32.to_bits());
        assert_eq!(backend.get(0x1860), 0);
        assert_eq!(backend.bytes[&0x1585], 1);
        assert_eq!(backend.bytes[&0x158e], 1);
        assert_eq!(backend.bytes[&0x159a], 1);
        assert_eq!(backend.bytes[&0x1598], 0);
    }
    #[test]
    fn missing_constant_string_keeps_the_preceding_native_default_stores() {
        let mut backend = Memory::default();
        backend.word(0x104c0, 0);
        backend.word(0x0ff8, 0x12345678);
        assert_eq!(
            vehicle_setup_default_source(&mut backend, 0x10000, 0x1000),
            Err(())
        );
        assert_eq!(backend.get(0x10a4), 1400f32.to_bits());
        assert_eq!(backend.get(0x17a0), 0);
        assert_eq!(backend.get(0x0ff8), 0x12345678);
        assert_eq!(backend.get(0x11c4), 0);
        assert_eq!(backend.bytes[&0x16c4], 1);
    }
}
