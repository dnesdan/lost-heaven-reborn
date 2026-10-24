//! Original PC packed vehicle parameter conversion, canonical RVA0x28380.
//! Source and destination are live caller storage. Padding is preserved, and
//! every completed write remains visible on a later unavailable dependency.
use super::*;

/// Converts the original packed 4385-byte source record into the 4388-byte
/// parameter layout. The original entry also writes four car-level values.
/// Tyre remapping is explicit; LoadLevelTables calls with `remap_tyres=false`.
pub fn car_convert_packed_vehicle_parameters<B: CarGameInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    car: u32,
    source: u32,
    destination: u32,
    remap_tyres: bool,
) -> Out<B, ()> {
    let mut call = GameInit {
        backend,
        memory,
        car,
        stack: 0,
    };
    call.convert_packed_parameters(source, destination, remap_tyres)
}

impl<B: CarGameInitBackend> GameInit<'_, B> {
    fn packed_word(&mut self, source: u32, target: u32, input: u32, output: u32) -> Out<B, ()> {
        let word = self.read(source.wrapping_add(input))?;
        self.put(target.wrapping_add(output), word)
    }
    fn packed_byte(&mut self, source: u32, target: u32, input: u32, output: u32) -> Out<B, ()> {
        let byte = self.byte(source.wrapping_add(input))?;
        self.put_byte(target.wrapping_add(output), byte)
    }
    /// Original lstrcpyA copies the terminator and preserves remaining padding.
    fn packed_string(&mut self, source: u32, target: u32) -> Out<B, ()> {
        let mut offset = 0u32;
        loop {
            let byte = self.byte(source.wrapping_add(offset))?;
            self.put_byte(target.wrapping_add(offset), byte)?;
            if byte == 0 {
                return Ok(());
            }
            offset = offset.wrapping_add(1);
        }
    }
    fn packed_sound_rows(
        &mut self,
        source: u32,
        target: u32,
        input: u32,
        output_ends: [u32; 2],
        stride: u32,
        bool_end: Option<u32>,
    ) -> Out<B, ()> {
        let [names_end, columns_end] = output_ends;
        for row in 0..5u32 {
            let packed = input.wrapping_sub(row.wrapping_mul(stride));
            let name_source = packed.wrapping_sub(if bool_end.is_some() { 0x41 } else { 0x40 });
            self.packed_string(
                source.wrapping_add(name_source),
                target.wrapping_add(names_end.wrapping_sub(0x10 + row * 0x10)),
            )?;
            for column in 0..8u32 {
                let input = packed
                    .wrapping_sub(if bool_end.is_some() { 0x31 } else { 0x30 })
                    .wrapping_add(column * 4);
                let output = (columns_end
                    .wrapping_sub(row)
                    .wrapping_sub(6)
                    .wrapping_add(column * 5))
                    * 4;
                self.packed_word(source, target, input, output)?;
            }
            if let Some(end) = bool_end {
                self.packed_byte(
                    source,
                    target,
                    packed.wrapping_sub(0x11),
                    end.wrapping_sub(row),
                )?;
            }
        }
        Ok(())
    }
    fn packed_sound_pairs(
        &mut self,
        source: u32,
        target: u32,
        input: u32,
        names_end: u32,
        columns_end: u32,
        bool_end: u32,
    ) -> Out<B, ()> {
        for row in 0..4u32 {
            let packed = input.wrapping_sub(row * 0x25);
            let names = names_end.wrapping_sub(row * 0x10);
            self.packed_string(
                source.wrapping_add(packed.wrapping_sub(0x35)),
                target.wrapping_add(names.wrapping_sub(0x50)),
            )?;
            self.packed_string(
                source.wrapping_add(packed.wrapping_sub(0x25)),
                target.wrapping_add(names.wrapping_sub(0x10)),
            )?;
            self.packed_word(
                source,
                target,
                packed.wrapping_sub(0x15),
                (columns_end - 1 - row) * 4,
            )?;
            self.packed_byte(source, target, packed.wrapping_sub(0x11), bool_end - row)?;
        }
        Ok(())
    }
    pub(super) fn convert_packed_parameters(
        &mut self,
        source: u32,
        target: u32,
        remap_tyres: bool,
    ) -> Out<B, ()> {
        for word in 0..20u32 {
            self.packed_word(source, target, word * 4, word * 4)?;
        }
        for row in 0..11u32 {
            let input = 0x32 - row * 3;
            for (input, output) in [
                (input, 0x1e - row),
                (input + 1, 0x29 - row),
                (input + 2, 0x34 - row),
            ] {
                self.packed_word(source, target, input * 4, output * 4)?;
            }
        }
        for word in [
            0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x40, 0x3f, 0x41, 0x42,
            0x43, 0x44, 0x45, 0x47, 0x46,
        ] {
            self.packed_word(source, target, word * 4, word * 4)?;
        }
        for row in 0..10u32 {
            let packed = 0x5d6 - row * 0x77;
            let column = 0x5c - row;
            for field in 0..24u32 {
                let input = packed.wrapping_sub(0x87).wrapping_add(field * 4);
                let output = (column.wrapping_sub(0xb).wrapping_add(field * 10)) * 4;
                if field == 4 && remap_tyres {
                    let begin = self.read(0x63d9c8)?;
                    let value = if begin == 0 {
                        0
                    } else {
                        let index = self.read(source.wrapping_add(input))?;
                        let end = self.read(0x63d9cc)?;
                        let count = (end.wrapping_sub(begin) as i32) / 0x28;
                        if index >= count as u32 || (index as i32) < 0 {
                            0
                        } else {
                            self.read(begin.wrapping_add(index.wrapping_mul(0x28)))?
                        }
                    };
                    self.put(target.wrapping_add(output), value)?;
                } else {
                    self.packed_word(source, target, input, output)?;
                }
            }
            for (input, output) in [
                (packed - 0x27, 0x4e9 - row),
                (packed - 0x26, 0x4f3 - row),
                (packed - 0x25, 0x4fd - row),
            ] {
                self.packed_byte(source, target, input, output)?;
            }
            for field in 0..5u32 {
                self.packed_word(
                    source,
                    target,
                    packed - 0x24 + field * 4,
                    (column + 0xed + field * 10) * 4,
                )?;
            }
        }
        for field in 0..7u32 {
            self.packed_word(source, target, 0x5c6 + field * 4, (0x172 + field) * 4)?;
        }
        self.packed_byte(source, target, 0x5e2, 0x179 * 4)?;
        for field in 0..3u32 {
            self.packed_word(source, target, 0x5e3 + field * 4, (0x17a + field) * 4)?;
        }
        for (input, output) in [(0x5ef, 0x5f4), (0x5f0, 0x5f5), (0x5f1, 0x5f6)] {
            self.packed_byte(source, target, input, output)?;
        }
        self.packed_word(source, target, 0x5f2, 0x17e * 4)?;
        self.packed_byte(source, target, 0x5f6, 0x17f * 4)?;
        for field in 0..8u32 {
            self.packed_word(source, target, 0x5f7 + field * 4, (0x180 + field) * 4)?;
        }
        self.packed_byte(source, target, 0x617, 0x188 * 4)?;
        for word in 0x189..=0x1a1u32 {
            self.packed_word(source, target, (word - 3) * 4, word * 4)?;
        }
        for word in 0x1a2..=0x1b6u32 {
            self.packed_word(source, target, (word + 1) * 4, word * 4)?;
        }
        for word in 0x1bf..=0x1e9u32 {
            self.packed_word(source, target, (word + 1) * 4, word * 4)?;
        }
        for (input, output) in [(0x1b8 * 4, 0x1b7 * 4), (0x1bc * 4, 0x1bb * 4)] {
            self.packed_string(source.wrapping_add(input), target.wrapping_add(output))?;
        }
        for word in 0x1ea..=0x1f0u32 {
            self.packed_word(source, target, (word + 1) * 4, word * 4)?;
        }
        for (input, output) in [
            (0x7c9, 0x1f1),
            (0x7d9, 0x1f5),
            (0x7e9, 0x1f9),
            (0xad3, 0x239),
            (0xaeb, 0x23d),
            (0xb0f, 0x241),
            (0xb1f, 0x245),
            (0xb2f, 0x249),
            (0xb3f, 0x24d),
            (0xb4f, 0x251),
            (0xb5f, 0x255),
            (0xb6f, 0x259),
            (0xb7f, 0x25d),
            (0xb8f, 0x261),
            (0xb9f, 0x265),
        ] {
            self.packed_string(source.wrapping_add(input), target.wrapping_add(output * 4))?;
        }
        self.packed_sound_rows(source, target, 0x8fe, [0x211 * 4, 0x29b], 0x31, Some(0xae8))?;
        self.packed_sound_rows(source, target, 0x9f3, [0x225 * 4, 0x2c5], 0x31, Some(0xb90))?;
        self.packed_sound_rows(source, target, 0xae3, [0x239 * 4, 0x2ef], 0x30, None)?;
        for (input, output) in [
            (0xae3, 0x30d),
            (0xae7, 0x30e),
            (0xafb, 0x30f),
            (0xaff, 0x310),
            (0xb03, 0x311),
            (0xb07, 0x312),
            (0xb0b, 0x313),
        ] {
            self.packed_word(source, target, input, output * 4)?;
        }
        self.packed_sound_pairs(source, target, 0xc53, 0x289 * 4, 0x318, 0xc63)?;
        for (input, output) in [
            (0xc43, 0x289),
            (0xc53, 0x28d),
            (0xc63, 0x319),
            (0xc73, 0x31d),
            (0xc83, 0x321),
            (0xc93, 0x325),
            (0xf8d, 0x365),
            (0xfa5, 0x369),
            (0xfc9, 0x36d),
            (0xfd9, 0x371),
            (0xfe9, 0x375),
            (0xff9, 0x379),
            (0x1009, 0x37d),
            (0x1019, 0x381),
            (0x1029, 0x385),
            (0x1039, 0x389),
            (0x1049, 0x38d),
            (0x1059, 0x391),
        ] {
            self.packed_string(source.wrapping_add(input), target.wrapping_add(output * 4))?;
        }
        for field in 0..4u32 {
            self.packed_word(source, target, 0xca3 + field * 4, (0x3bd + field) * 4)?;
        }
        self.packed_sound_rows(source, target, 0xdb8, [0x33d * 4, 0x3cb], 0x31, Some(0xfa8))?;
        self.packed_sound_rows(
            source,
            target,
            0xead,
            [0x351 * 4, 0x3f5],
            0x31,
            Some(0x1050),
        )?;
        self.packed_sound_rows(source, target, 0xf9d, [0x365 * 4, 0x41f], 0x30, None)?;
        for (input, output) in [
            (0xf9d, 0x43d),
            (0xfa1, 0x43e),
            (0xfb5, 0x43f),
            (0xfb9, 0x440),
            (0xfbd, 0x441),
            (0xfc1, 0x442),
            (0xfc5, 0x443),
        ] {
            self.packed_word(source, target, input, output * 4)?;
        }
        self.packed_sound_pairs(source, target, 0x110d, 0x3b5 * 4, 0x448, 0x1123)?;
        for (input, output) in [(0x10fd, 0x3b5), (0x110d, 0x3b9)] {
            self.packed_string(source.wrapping_add(input), target.wrapping_add(output * 4))?;
        }
        let value = self.read(source + 0x1a1 * 4)?;
        self.put(self.car + 0x20bc, value)?;
        let first = self.float(source + 0x19f * 4)?;
        let factor = self.float(self.car + 0x664)?;
        let speed = Self::multiply(Self::multiply(first, factor)?, f32::from_bits(0x3e8e38e4))?;
        self.put(self.car + 0x20b4, speed.to_bits())?;
        let second = self.float(source + 0x1a0 * 4)?;
        let factor = self.float(self.car + 0x664)?;
        let speed = Self::multiply(Self::multiply(second, factor)?, f32::from_bits(0x3e8e38e4))?;
        self.put(self.car + 0x20b8, speed.to_bits())?;
        let value = self.read(source + 0x1a2 * 4)?;
        self.put(self.car + 0x20c0, value)?;
        Ok(())
    }
}
