//! Vehicle GameInit's owned model pairs (PC0x4c4ff5..0x4c5447).
use super::*;

impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    pub(super) fn game_initialize_skid_models(
        &mut self,
        mut memory: VehicleComponentTailMemory<'_>,
        vehicle: u32,
        position_pointer_slot: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        // Full saved-register ESP is main GameInit ESP-0xfc. Pair is ESP+0x30,
        // while the caller's retained position pointer is ESP+0x48.
        let pair = position_pointer_slot.wrapping_sub(0x18);
        let mut remaining = memory.read_word_with_backend(self, vehicle + 0x4c4)?;
        while remaining != 0 {
            remaining -= 1;
            let wheels = memory.read_word_with_backend(self, vehicle + 0xc38)?;
            let wheel = memory.read_word_with_backend(self, wheels + remaining * 4)?;
            let frame = memory.read_word_with_backend(self, wheel + 8)?;
            let properties = memory.read_word_with_backend(self, frame + 0xc)?;
            if properties != 0 {
                let mut bytes = Vec::new();
                for i in 0..32768u32 {
                    let byte = memory.read_byte_with_backend(self, properties.wrapping_add(i))?;
                    if byte == 0 {
                        break;
                    }
                    bytes.push(byte);
                    if i == 32767 {
                        return Err(VehicleFrameError::UnavailableFrameMethod(
                            "skid properties length",
                        ));
                    }
                }
                if bytes.len() > 2 {
                    if let Some(index) = bytes.iter().position(|&byte| byte == b'[') {
                        if let Some(width) = scan_width(&bytes[index + 1..])
                            .map_err(|_| VehicleFrameError::NumericDomain("skid width sscanf"))?
                        {
                            memory.write_word_with_backend(self, wheel + 0x138, width.to_bits())?;
                        }
                        let width =
                            f32::from_bits(memory.read_word_with_backend(self, wheel + 0x138)?);
                        let width = crate::native_matrix::checked_f32_mul(
                            width,
                            f32::from_bits(0x3c23d70a),
                        )
                        .map_err(|_| VehicleFrameError::NumericDomain("skid width scale"))?;
                        memory.write_word_with_backend(self, wheel + 0x138, width.to_bits())?;
                    }
                }
            }
            let mut index = 0i32;
            loop {
                let count = memory.read_word_with_backend(self, vehicle + 0x2bc)? as i32;
                if index >= count {
                    break;
                }
                let first = self.arena.create_model()?;
                let first_address = self.frame_address(first.root())?;
                let second = self.arena.create_model()?;
                let second_address = self.frame_address(second.root())?;
                self.game_open_model(first_address, "shlaif01.i3d")?;
                self.game_open_model(second_address, "shlaif02.i3d")?;
                memory.write_word_with_backend(self, pair, first_address)?;
                memory.write_word_with_backend(self, pair + 8, second_address)?;
                for (model, child_slot) in [(first_address, pair + 4), (second_address, pair + 12)]
                {
                    let scene = memory.read_word_with_backend(self, 0x63d9d4)?;
                    let primary = memory.read_word_with_backend(self, scene + 0x210)?;
                    self.bodycol_link_frame(model, primary, 0)?;
                    let child = self.game_virtual(model, 0x30, &[0])?;
                    memory.write_word_with_backend(self, child_slot, child)?;
                    self.game_virtual(model, 0x24, &[0])?;
                    self.game_virtual(model, 0x18, &[])?;
                }
                // Only these two bytes were initialized in native caller storage.
                memory.write_byte_with_backend(self, pair + 0x10, 0)?;
                memory.write_byte_with_backend(self, pair + 0x12, 0)?;
                let wheels = memory.read_word_with_backend(self, vehicle + 0xc38)?;
                let wheel = memory.read_word_with_backend(self, wheels + remaining * 4)?;
                let end = memory.read_word_with_backend(self, wheel + 0x130)?;
                let mut projection = GameProjection {
                    backend: self,
                    memory: memory.reborrow(),
                };
                crate::vehicle_component_records::vehicle_skid_model_pairs_insert(
                    &mut projection,
                    wheel + 0x128,
                    end,
                    pair,
                )?;
                index += 1;
            }
        }
        Ok(())
    }
}

// The same admitted C-locale finite decimal domain as vehicle_axis_scan_angles,
// with exactly one conversion. No matching decimal leaves the old width intact.
fn scan_width(input: &[u8]) -> Result<Option<f32>, ()> {
    let mut i = 0;
    while input
        .get(i)
        .is_some_and(|b| b.is_ascii_whitespace() || *b == 0x0b)
    {
        i += 1;
    }
    let start = i;
    if matches!(input.get(i), Some(b'+' | b'-')) {
        i += 1;
    }
    let mut digits = 0;
    let mut significant = 0;
    let mut nonzero = false;
    let mut decimal = false;
    while let Some(&byte) = input.get(i) {
        if byte.is_ascii_digit() {
            digits += 1;
            nonzero |= byte != b'0';
            significant += usize::from(nonzero);
        } else if byte == b'.' && !decimal {
            decimal = true;
        } else {
            break;
        }
        i += 1;
    }
    if digits == 0 {
        return Ok(None);
    }
    if matches!(input.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(input.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        let exponent = i;
        while input.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if i == exponent {
            return Err(());
        }
    }
    if significant > 9 {
        return Err(());
    }
    let value = std::str::from_utf8(&input[start..i])
        .map_err(|_| ())?
        .parse::<f64>()
        .map_err(|_| ())? as f32;
    if !value.is_finite() || (value != 0. && !value.is_normal()) || (value == 0. && nonzero) {
        return Err(());
    }
    Ok(Some(value))
}

#[cfg(test)]
mod tests {
    use super::scan_width;
    #[test]
    fn width_scan_retains_original_on_no_match_and_limits_ambiguous_numbers() {
        assert_eq!(scan_width(b" 35.5]rest"), Ok(Some(35.5)));
        assert_eq!(scan_width(b"-2e1]"), Ok(Some(-20.)));
        assert_eq!(scan_width(b"wrong]"), Ok(None));
        assert!(scan_width(b"1e]").is_err());
        assert!(scan_width(b"1234567890]").is_err());
        assert!(scan_width(b"1e1000]").is_err());
    }
}
