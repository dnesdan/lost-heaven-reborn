//! Original AXIS discovery at PC RVA0xc0f70 and its property-number intake.
use super::wheels::WheelsResult;
use super::*;

/// Required model enumeration and native CRT decimal conversion.
pub trait VehicleAxisBackend: VehicleInitBackend {
    fn enumerate_axes(
        &mut self,
        model: u32,
        target: &mut VehicleAxisEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>>;
    fn parse_axis_number(&mut self, address: u32) -> Result<i32, Self::Error>;
}
/// AXIS* callback destination, mask0x10001. The vector header is live backend
/// stack storage; the ordinary collecting callback supplies allocation behavior.
pub struct VehicleAxisEnumeration<'a> {
    state: &'a mut VehicleInitState,
    header: u32,
}
impl VehicleAxisEnumeration<'_> {
    pub fn visit<B: VehicleInitBackend>(
        &mut self,
        frame: u32,
        backend: &mut B,
    ) -> Result<u32, VehicleInitUnavailable<B::Error>> {
        backend.collect_frame_projected(self.state.live_tail_memory(), self.header, frame)
    }
}
impl VehicleInitState {
    fn axis_byte_store<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        address: u32,
        value: u8,
    ) -> WheelsResult<B, ()> {
        self.tail_projection(b, self.tail_vehicle_address.unwrap())
            .write_byte(address, value)
            .map_err(VehicleInitUnavailable::Effect)
            .map_err(Into::into)
    }
    fn axis_strlen<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        mut address: u32,
    ) -> WheelsResult<B, u32> {
        let mut remaining = u32::MAX;
        while remaining != 0 {
            let byte = self.wheel_byte(b, address)?;
            address = address.wrapping_add(1);
            remaining -= 1;
            if byte == 0 {
                break;
            }
        }
        Ok((!remaining).wrapping_sub(1))
    }
    fn axis_frame<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        stack: u32,
        index: u32,
    ) -> WheelsResult<B, u32> {
        let begin = self.wheel_word(b, stack.wrapping_add(0x14))?;
        self.wheel_word(b, begin.wrapping_add(index.wrapping_mul(4)))
    }
    fn axis_erase<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        stack: u32,
        index: u32,
    ) -> WheelsResult<B, ()> {
        let begin = self.wheel_word(b, stack.wrapping_add(0x14))?;
        let end = self.wheel_word(b, stack.wrapping_add(0x18))?;
        let mut source = begin.wrapping_add(index.wrapping_mul(4)).wrapping_add(4);
        while source != end {
            let value = self.wheel_word(b, source)?;
            self.wheel_store(b, source.wrapping_sub(4), value)?;
            source = source.wrapping_add(4);
        }
        let end = self.wheel_word(b, stack.wrapping_add(0x18))?;
        self.wheel_store(b, stack.wrapping_add(0x18), end.wrapping_sub(4))
    }
    fn axis_angles<B: VehicleInitBackend>(
        &mut self,
        b: &mut B,
        frame: u32,
        stack: u32,
    ) -> WheelsResult<B, ()> {
        let properties = self.wheel_word(b, frame.wrapping_add(12))?;
        let properties = if properties == 0 {
            0x63d2a0
        } else {
            properties
        };
        let length = self.axis_strlen(b, properties)?;
        if length <= 2 {
            return Ok(());
        }
        let mut offset = 0;
        while self.wheel_byte(b, properties.wrapping_add(offset))? != b'[' {
            if offset >= length {
                return Ok(());
            }
            offset = offset.wrapping_add(1);
        }
        if offset >= length {
            return Ok(());
        }
        let address = properties.wrapping_add(offset).wrapping_add(1);
        {
            let base = self.tail_vehicle_address.unwrap();
            let mut memory = self.tail_projection(b, base);
            vehicle_axis_scan_angles(&mut memory, address, stack.wrapping_add(0x28))?;
        }
        for offset in [0x28, 0x2c, 0x30] {
            let value = f32::from_bits(self.wheel_word(b, stack.wrapping_add(offset))?);
            let value = crate::native_matrix::checked_f32_mul(value, f32::from_bits(0x3c23d70a))
                .map_err(|_| VehicleWheelsUnavailable::NumericDomain)?;
            self.wheel_store(b, stack.wrapping_add(offset), value.to_bits())?;
        }
        Ok(())
    }
    /// Executes the original complete AXIS discovery helper. `stack` denotes its
    /// ESP after prologue, ordinarily the parent Init ESP minus0xf4. Unknown
    /// original padding remains supplied backend storage; reached stores are
    /// performed here. Enumeration and every insertion retain their prefix.
    pub fn setup_axes<B: VehicleAxisBackend>(
        &mut self,
        b: &mut B,
        stack: u32,
    ) -> WheelsResult<B, ()> {
        use VehicleInitUnavailable::Effect;
        let base = self
            .tail_vehicle_address
            .ok_or(VehicleWheelsUnavailable::NoContinuation)?;
        let marker = self.wheel_byte(b, stack.wrapping_add(0x3f))?;
        self.wheel_store(b, stack.wrapping_add(0x40), base)?;
        self.axis_byte_store(b, stack.wrapping_add(0x10), marker)?;
        for offset in [0x14, 0x18, 0x1c] {
            self.wheel_store(b, stack.wrapping_add(offset), 0)?;
        }
        let model = self.load(0x34c)?;
        b.enumerate_axes(
            model,
            &mut VehicleAxisEnumeration {
                state: self,
                header: stack.wrapping_add(0x10),
            },
        )?;
        let name = self.load(0x4bc)?;
        let suffix = self.axis_strlen(b, name)?.wrapping_add(6);
        self.wheel_store(b, stack.wrapping_add(0x38), suffix)?;
        let begin = self.wheel_word(b, stack.wrapping_add(0x14))?;
        let mut remaining = if begin == 0 {
            0
        } else {
            ((self
                .wheel_word(b, stack.wrapping_add(0x18))?
                .wrapping_sub(begin) as i32)
                >> 2) as u32
        };
        while remaining != 0 {
            remaining = remaining.wrapping_sub(1);
            let suffix = self.wheel_word(b, stack.wrapping_add(0x38))?;
            let frame = self.axis_frame(b, stack, remaining)?;
            let name = self.wheel_word(b, frame.wrapping_add(0x100))?;
            let name = if name == 0 { 0x63d2a0 } else { name };
            let number = name.wrapping_add(suffix);
            let axle = b.parse_axis_number(number).map_err(Effect)?;
            if axle >= self.load::<B::Error>(0x4c0)? as i32
                || self.wheel_byte(b, number.wrapping_sub(1))? != b'0'
            {
                continue;
            }
            for offset in (0x20..0x38).step_by(4) {
                self.wheel_store(b, stack.wrapping_add(offset), 0)?;
            }
            for offset in [0x28, 0x2c, 0x30] {
                self.wheel_store(b, stack.wrapping_add(offset), 0)?;
            }
            let frame = self.axis_frame(b, stack, remaining)?;
            self.axis_angles(b, frame, stack)?;
            let frame = self.axis_frame(b, stack, remaining)?;
            self.wheel_store(b, stack.wrapping_add(0x20), frame)?;
            let axle = b.parse_axis_number(number).map_err(Effect)?;
            self.wheel_store(b, stack.wrapping_add(0x24), axle as u32)?;
            let insertion = self.load(0x334)?;
            self.axis_byte_store(b, stack.wrapping_add(0x35), 1)?;
            vehicle_axis_records_insert(
                &mut self.tail_projection(b, base),
                base.wrapping_add(0x32c),
                insertion,
                1,
                stack.wrapping_add(0x20),
            )
            .map_err(Effect)?;
            self.axis_erase(b, stack, remaining)?;
        }
        for offset in (0x44..0xe4).step_by(4) {
            self.wheel_store(b, stack.wrapping_add(offset), 0)?;
        }
        let name = self.load(0x4bc)?;
        let suffix = self.axis_strlen(b, name)?.wrapping_add(6);
        self.wheel_store(b, stack.wrapping_add(0x38), suffix)?;
        let begin = self.wheel_word(b, stack.wrapping_add(0x14))?;
        remaining = if begin == 0 {
            0
        } else {
            ((self
                .wheel_word(b, stack.wrapping_add(0x18))?
                .wrapping_sub(begin) as i32)
                >> 2) as u32
        };
        while remaining != 0 {
            remaining = remaining.wrapping_sub(1);
            let suffix = self.wheel_word(b, stack.wrapping_add(0x38))?;
            let frame = self.axis_frame(b, stack, remaining)?;
            let name = self.wheel_word(b, frame.wrapping_add(0x100))?;
            let name = if name == 0 { 0x63d2a0 } else { name };
            self.axis_byte_store(b, stack.wrapping_add(0x35), 0)?;
            for offset in [0x28, 0x2c, 0x30] {
                self.wheel_store(b, stack.wrapping_add(offset), 0)?;
            }
            let frame = self.axis_frame(b, stack, remaining)?;
            self.axis_angles(b, frame, stack)?;
            let number = name.wrapping_add(suffix);
            let axle = b.parse_axis_number(number).map_err(Effect)?;
            let left = axle < self.load::<B::Error>(0x4c0)? as i32
                && self.wheel_byte(b, number.wrapping_sub(1))? == b'L';
            if !left {
                let axle = b.parse_axis_number(number).map_err(Effect)?;
                if axle >= self.load::<B::Error>(0x4c0)? as i32
                    || self.wheel_byte(b, number.wrapping_sub(1))? != b'R'
                {
                    continue;
                }
            }
            let frame = self.axis_frame(b, stack, remaining)?;
            self.wheel_store(b, stack.wrapping_add(0x20), frame)?;
            let axle = b.parse_axis_number(number).map_err(Effect)?;
            self.wheel_store(b, stack.wrapping_add(0x24), axle as u32)?;
            if !left {
                let angle = self.wheel_word(b, stack.wrapping_add(0x28))?;
                self.wheel_store(b, stack.wrapping_add(0x28), angle ^ 0x80000000)?;
            }
            self.axis_byte_store(b, stack.wrapping_add(0x34), u8::from(left))?;
            let insertion = self.load(0x344)?;
            vehicle_axis_records_insert(
                &mut self.tail_projection(b, base),
                base.wrapping_add(0x33c),
                insertion,
                1,
                stack.wrapping_add(0x20),
            )
            .map_err(Effect)?;
            self.axis_erase(b, stack, remaining)?;
            let axle = self.wheel_word(b, stack.wrapping_add(0x24))?;
            let address = stack
                .wrapping_add(if left { 0x44 } else { 0x94 })
                .wrapping_add(axle.wrapping_mul(4));
            let count = self.wheel_word(b, address)?;
            self.wheel_store(b, address, count.wrapping_add(1))?;
        }
        let begin = self.load::<B::Error>(0x340)?;
        let mut count = if begin == 0 {
            0
        } else {
            ((self.load::<B::Error>(0x344)?.wrapping_sub(begin) as i32) / 24) as u32
        };
        let mut offset = count.wrapping_mul(24);
        while count != 0 {
            offset = offset.wrapping_sub(24);
            let record = self.load::<B::Error>(0x340)?.wrapping_add(offset);
            let left = self.wheel_byte(b, record.wrapping_add(0x14))? != 0;
            let axle = self.wheel_word(b, record.wrapping_add(4))?;
            let address = stack
                .wrapping_add(if left { 0x44 } else { 0x94 })
                .wrapping_add(axle.wrapping_mul(4));
            if self.wheel_word(b, address)? == 1 {
                self.axis_byte_store(b, record.wrapping_add(0x15), 1)?;
            }
            count = count.wrapping_sub(1);
        }
        let allocation = self.wheel_word(b, stack.wrapping_add(0x14))?;
        self.init_free(b, allocation).map_err(Effect)?;
        Ok(())
    }
}

/// The three `%f` conversions of the original AXIS `sscanf` format ` %f %f %f`.
/// Successful assignments are retained if a later conversion does not match.
/// C-locale finite decimals with at most nine significant digits are admitted;
/// nonfinite, subnormal and ambiguous/incomplete exponent inputs remain explicit
/// host unavailability rather than silently selecting a different CRT result.
pub fn vehicle_axis_scan_angles<B: VehicleInitBackend>(
    b: &mut B,
    mut input: u32,
    output: u32,
) -> Result<u32, VehicleWheelsUnavailable<B::Error>> {
    use VehicleInitUnavailable::Effect;
    let mut assigned = 0;
    for slot in 0..3 {
        while matches!(
            b.read_byte(input).map_err(Effect)?,
            b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c
        ) {
            input = input.wrapping_add(1);
        }
        let mut token = Vec::new();
        let first = b.read_byte(input).map_err(Effect)?;
        if first == b'+' || first == b'-' {
            token.push(first);
            input = input.wrapping_add(1);
        }
        let mut digits = 0;
        let mut significant = 0;
        let mut nonzero = false;
        let mut decimal = false;
        loop {
            let byte = b.read_byte(input).map_err(Effect)?;
            if byte.is_ascii_digit() {
                digits += 1;
                nonzero |= byte != b'0';
                if nonzero {
                    significant += 1;
                }
            } else if byte == b'.' && !decimal {
                decimal = true;
            } else {
                break;
            }
            token.push(byte);
            input = input.wrapping_add(1);
        }
        if digits == 0 {
            return Ok(assigned);
        }
        let byte = b.read_byte(input).map_err(Effect)?;
        if byte == b'e' || byte == b'E' {
            token.push(byte);
            input = input.wrapping_add(1);
            let byte = b.read_byte(input).map_err(Effect)?;
            if byte == b'+' || byte == b'-' {
                token.push(byte);
                input = input.wrapping_add(1);
            }
            let mut exponent_digits = 0;
            loop {
                let byte = b.read_byte(input).map_err(Effect)?;
                if !byte.is_ascii_digit() {
                    break;
                }
                token.push(byte);
                input = input.wrapping_add(1);
                exponent_digits += 1;
            }
            if exponent_digits == 0 {
                return Err(VehicleWheelsUnavailable::NumericDomain);
            }
        }
        if significant > 9 {
            return Err(VehicleWheelsUnavailable::NumericDomain);
        }
        let value = std::str::from_utf8(&token)
            .unwrap()
            .parse::<f64>()
            .map_err(|_| VehicleWheelsUnavailable::NumericDomain)? as f32;
        if !value.is_finite() || (value != 0. && !value.is_normal()) || (value == 0. && nonzero) {
            return Err(VehicleWheelsUnavailable::NumericDomain);
        }
        b.write_word(output.wrapping_add(slot * 4), value.to_bits())
            .map_err(Effect)?;
        assigned += 1;
    }
    Ok(assigned)
}

#[cfg(test)]
mod tests {
    use super::super::wheels::tests::Memory;
    use super::*;
    impl VehicleAxisBackend for Memory {
        fn enumerate_axes(
            &mut self,
            _: u32,
            target: &mut VehicleAxisEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<()>> {
            for frame in self.frames.clone() {
                target.visit(frame, self)?;
            }
            Ok(())
        }
        fn parse_axis_number(&mut self, address: u32) -> Result<i32, ()> {
            self.parse_wheel_number(address)
        }
    }
    #[test]
    fn property_scanning_retains_partial_conversions_and_c_locale_whitespace() {
        for (input, expected, count) in [
            (b" 1 -2.5 3e1]".as_slice(), [1., -2.5, 30.], 3),
            (b"1,2,3]".as_slice(), [1., 9., 9.], 1),
            (b"1 2 ]".as_slice(), [1., 2., 9.], 2),
            (b"]".as_slice(), [9., 9., 9.], 0),
            (b"-0 +0 0.0]".as_slice(), [-0., 0., 0.], 3),
        ] {
            let mut b = Memory::default();
            b.put(0x1000, input);
            b.put(0x1000 + input.len() as u32, &[0]);
            for i in 0..3 {
                b.word(0x2000 + i * 4, 9f32.to_bits());
            }
            assert_eq!(vehicle_axis_scan_angles(&mut b, 0x1000, 0x2000), Ok(count));
            assert_eq!(
                (0..3).map(|i| b.get(0x2000 + i * 4)).collect::<Vec<_>>(),
                expected.map(f32::to_bits)
            );
        }
    }
    #[test]
    fn rejected_numeric_conversion_does_not_discard_earlier_assignments() {
        let mut b = Memory::default();
        b.put(0x1000, b"1 1e-99 3\0");
        for i in 0..3 {
            b.word(0x2000 + i * 4, 9f32.to_bits());
        }
        assert_eq!(
            vehicle_axis_scan_angles(&mut b, 0x1000, 0x2000),
            Err(VehicleWheelsUnavailable::NumericDomain)
        );
        assert_eq!(b.get(0x2000), 1f32.to_bits());
        assert_eq!(b.get(0x2004), 9f32.to_bits());
    }
    #[test]
    fn axis_discovery_builds_central_and_left_right_records_in_reverse_traversal_order() {
        let mut state = VehicleInitState::from_bytes(vec![0; 0x2400]);
        state.tail_vehicle_address = Some(0x10000);
        state.tail_scratch = Some(VehicleComponentTailScratch {
            address: 0x9000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        });
        state.store::<()>(0x4c0, 2).unwrap();
        state.store::<()>(0x4bc, 0x3000).unwrap();
        let mut b = Memory::default();
        b.put(0x3000, b"car\0");
        b.put(0x7f00, &[0xa5; 0x100]);
        b.allocations = [0x5000, 0x5400, 0x5800, 0x6000, 0x6400, 0x6800].into();
        for (i, name) in [b"car.AXIS00\0".as_slice(), b"car.AXISL0\0", b"car.AXISR0\0"]
            .into_iter()
            .enumerate()
        {
            let frame = 0x1000 + i as u32 * 0x200;
            let name_address = 0x3400 + i as u32 * 32;
            b.frames.push(frame);
            b.word(frame + 0x100, name_address);
            b.put(name_address, name);
            b.word(frame + 12, 0x3800);
            b.put(0x3800, b"[10 20 30]\0");
        }
        state.setup_axes(&mut b, 0x7f00).unwrap();
        assert_eq!(state.load::<()>(0x330), Ok(0x6000));
        assert_eq!(b.get(0x6000), 0x1000);
        assert_eq!(b.get(0x6004), 0);
        assert_eq!(b.get(0x6008), 0.099999994f32.to_bits());
        assert_eq!(b.bytes[&0x6015], 1);
        assert_eq!(state.load::<()>(0x340), Ok(0x6800));
        assert_eq!(b.get(0x6800), 0x1400);
        assert_eq!(b.get(0x6818), 0x1200);
        assert_eq!(b.bytes[&0x6814], 0);
        assert_eq!(b.bytes[&0x682c], 1);
        assert_eq!(b.bytes[&0x6815], 1);
        assert_eq!(b.bytes[&0x682d], 1);
        assert_eq!(b.get(0x6808), (-0.099999994f32).to_bits());
    }
}
