//! Original C_car GameInit support entries over the same canonical memory.
//! Effects are dependency boundaries, never successful placeholder backends.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarGameInitSupportEffect {
    OpenLevelTable {
        path: u32,
        flags: u32,
    },
    ReadLevelTable {
        handle: u32,
        target: u32,
        bytes: u32,
    },
    SeekLevelTable {
        handle: u32,
        offset: u32,
        origin: u32,
    },
    CloseLevelTable {
        handle: u32,
    },
    VehicleSoundDependency {
        entry: u32,
        vehicle: u32,
    },
    ApplySourceParameters {
        vehicle: u32,
        source: u32,
        flags: u8,
    },
    FindShadowType {
        manager: u32,
        name: u32,
    },
    InstallShadow {
        game: u32,
        object: u32,
        index: u32,
        kind: u32,
        model: u32,
        shadow_type: u32,
    },
    RemoveShadow {
        game: u32,
        object: u32,
        index: u32,
    },
    EnumerateShadowReceivers {
        root: u32,
        callback: u32,
        context: u32,
        mask: u32,
        pattern: u32,
    },
}

pub trait CarGameInitSupportBackend: CarGameInitBackend {
    fn car_game_init_support_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitSupportEffect,
    ) -> Result<u32, Self::Error>;
}

/// `stack` is native ESP after the selected helper's own prologue. It is not
/// C_car::GameInit's local-frame base. Reached stores/effects survive failure.
pub fn car_game_init_support<B: CarGameInitSupportBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    car: u32,
    stack: u32,
    entry: u32,
    argument: u32,
) -> Out<B, u32> {
    let mut call = GameInit {
        backend,
        memory,
        car,
        stack,
    };
    match entry {
        0x41e270 => call.load_level_tables(),
        0x41ea00 => call.set_actual_level(argument),
        0x429bb0 => {
            call.shadow_car_on(argument as u8 != 0)?;
            Ok(0)
        }
        0x423340 => {
            call.set_shadows_to_me()?;
            Ok(0)
        }
        0x407cd0 => {
            call.debustize_frame(car)?;
            Ok(0)
        }
        _ => Err(CarGameInitUnavailable::Dependency { entry }),
    }
}
impl<B: CarGameInitSupportBackend> GameInit<'_, B> {
    pub(super) fn support_effect(&mut self, effect: CarGameInitSupportEffect) -> Out<B, u32> {
        self.backend
            .car_game_init_support_effect(self.memory.reborrow(), effect)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn set_actual_level(&mut self, index: u32) -> Out<B, u32> {
        let begin = self.read(self.car + 0x21b4)?;
        if begin == 0 {
            return Ok(0);
        }
        let end = self.read(self.car + 0x21b8)?;
        let count = (end.wrapping_sub(begin) as i32) / 0x113c;
        if index >= count as u32 {
            return Ok(0);
        }
        let vehicle = self.car.wrapping_add(0x70);
        self.put(self.stack.wrapping_add(0x14), vehicle)?;
        self.call(0x41ea48, &[])?;
        self.support_effect(CarGameInitSupportEffect::VehicleSoundDependency {
            entry: 0x4eb5e0,
            vehicle,
        })?;
        let offset = index.wrapping_mul(0x113c);
        let begin = self.read(self.car + 0x21b4)?;
        let weight = Self::subtract(self.float(self.car + 0x4e8)?, 1.0)?;
        self.put(
            begin.wrapping_add(offset).wrapping_add(0x1c),
            weight.to_bits(),
        )?;
        let begin = self.read(self.car + 0x21b4)?;
        let source = begin.wrapping_add(offset);
        self.call(0x41ea83, &[source, 2])?;
        let initialized = self.support_effect(CarGameInitSupportEffect::ApplySourceParameters {
            vehicle,
            source,
            flags: 2,
        })? as u8
            != 0;
        self.call(if initialized { 0x41ea9c } else { 0x41ea8e }, &[])?;
        self.support_effect(CarGameInitSupportEffect::VehicleSoundDependency {
            entry: 0x4eb110,
            vehicle,
        })?;
        if !initialized {
            return Ok(0);
        }
        let begin = self.read(self.car + 0x21b4)?;
        let preserved = self.float(self.car + 0x20c4)?;
        self.copy(
            begin.wrapping_add(offset).wrapping_add(0x1124),
            self.car + 0x20b4,
            6,
        )?;
        self.put(self.car + 0x20c4, preserved.to_bits())?;
        let numerator =
            Self::multiply(self.float(self.car + 0x5e4)?, self.float(self.car + 0x4e8)?)?;
        let denominator =
            Self::multiply(self.float(self.car + 0x664)?, self.float(self.car + 0x620)?)?;
        let ratio = Self::divide(numerator, denominator)?;
        self.put(self.car + 0x21ac, index)?;
        self.put(self.car + 0x20e0, ratio.to_bits())?;
        Ok(1)
    }
    fn shadow_car_on(&mut self, enabled: bool) -> Out<B, ()> {
        let current = self.byte(self.car + 0x207c)? != 0;
        if enabled && !current {
            let model = self.read(self.car + 0x21d8)?;
            if model != 0 {
                if self.byte(self.car + 0x5e)? == 0 {
                    self.effect(CarGameInitEffect::SetFrameOn {
                        frame: model,
                        enabled: true,
                    })?;
                }
                let app = self.read(0x63788c)?;
                let game = self.read(app + 0x24)?;
                let shadow_type =
                    self.support_effect(CarGameInitSupportEffect::FindShadowType {
                        manager: 0x6d4560,
                        name: self.car + 0x218c,
                    })?;
                let model = self.read(self.car + 0x21d8)?;
                let object = self.read(self.car + 0x68)?;
                self.support_effect(CarGameInitSupportEffect::InstallShadow {
                    game,
                    object,
                    index: 1,
                    kind: 4,
                    model,
                    shadow_type,
                })?;
            }
            self.put_byte(self.car + 0x207c, 1)?;
        } else if !enabled && current {
            let app = self.read(0x63788c)?;
            let object = self.read(self.car + 0x68)?;
            let game = self.read(app + 0x24)?;
            self.support_effect(CarGameInitSupportEffect::RemoveShadow {
                game,
                object,
                index: 1,
            })?;
            let model = self.read(self.car + 0x21d8)?;
            if model != 0 {
                self.effect(CarGameInitEffect::SetFrameOn {
                    frame: model,
                    enabled: false,
                })?;
            }
            self.put_byte(self.car + 0x207c, 0)?;
        }
        Ok(())
    }
    fn set_shadows_to_me(&mut self) -> Out<B, ()> {
        let original = self.byte(self.stack.wrapping_add(0xb))?;
        self.put_byte(self.stack.wrapping_add(0xc), original)?;
        for offset in [0x10, 0x14, 0x18] {
            self.put(self.stack.wrapping_add(offset), 0)?;
        }
        let root = self.read(self.car + 0x3bc)?;
        self.put(self.stack.wrapping_add(0x24), 0)?;
        self.call(
            0x423394,
            &[
                root,
                0x4d0eb0,
                self.stack.wrapping_add(0xc),
                0x10001,
                0x6323b4,
            ],
        )?;
        self.support_effect(CarGameInitSupportEffect::EnumerateShadowReceivers {
            root,
            callback: 0x4d0eb0,
            context: self.stack.wrapping_add(0xc),
            mask: 0x10001,
            pattern: 0x6323b4,
        })?;
        let mut begin = self.read(self.stack.wrapping_add(0x10))?;
        if begin != 0 {
            let end = self.read(self.stack.wrapping_add(0x14))?;
            let mut count = (end.wrapping_sub(begin) as i32) >> 2;
            if count < 0 {
                return Err(CarGameInitUnavailable::Storage);
            }
            while count != 0 {
                let frame = self.read(begin.wrapping_add((count as u32 - 1) * 4))?;
                count -= 1;
                self.call(0x4233b3, &[frame, 0])?;
                self.effect(CarGameInitEffect::FrameVirtual {
                    frame,
                    slot: 0x54,
                    arguments: [0, 0, 0, 0],
                    count: 1,
                })?;
                begin = self.read(self.stack.wrapping_add(0x10))?;
            }
        }
        self.call(0x4233c1, &[begin])?;
        self.backend
            .free_projected(&mut self.memory, begin)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn debustize_frame(&mut self, frame: u32) -> Out<B, ()> {
        let mut pending = vec![(frame, None)];
        while let Some((frame, remaining)) = pending.pop() {
            if frame == 0 {
                continue;
            }
            let count = if let Some(remaining) = remaining {
                remaining
            } else {
                if self.read(frame.wrapping_add(0x110))? == 1 {
                    self.effect(CarGameInitEffect::FrameVirtual {
                        frame,
                        slot: 0x5c,
                        arguments: [0, 0, 0, 0],
                        count: 1,
                    })?;
                }
                self.read(frame.wrapping_add(0x11c))?
            };
            if count != 0 {
                let index = count.wrapping_sub(1);
                let child = self.effect(CarGameInitEffect::FrameVirtual {
                    frame,
                    slot: 0x30,
                    arguments: [index, 0, 0, 0],
                    count: 1,
                })?;
                pending.push((frame, Some(index)));
                pending.push((child, None));
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
    struct Host {
        memory: BTreeMap<u32, u8>,
        effects: Vec<CarGameInitSupportEffect>,
        frames: Vec<CarGameInitEffect>,
        file: Vec<u8>,
        position: usize,
        next: u32,
        allocations: BTreeMap<u32, u32>,
        frees: Vec<u32>,
        fail_install: bool,
        source_result: u32,
    }
    impl Host {
        fn word(&mut self, a: u32, v: u32) {
            for (i, b) in v.to_le_bytes().into_iter().enumerate() {
                self.memory.insert(a + i as u32, b);
            }
        }
        fn region(&mut self, a: u32, n: u32, b: u8) {
            for i in 0..n {
                self.memory.insert(a + i, b);
            }
        }
        fn get(&self, a: u32) -> u32 {
            u32::from_le_bytes([
                self.memory[&a],
                self.memory[&(a + 1)],
                self.memory[&(a + 2)],
                self.memory[&(a + 3)],
            ])
        }
    }
    impl VehicleInitBackend for Host {
        type Error = &'static str;
        fn read_byte(&mut self, a: u32) -> Result<u8, Self::Error> {
            self.memory.get(&a).copied().ok_or("missing byte")
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Self::Error> {
            Ok(u32::from_le_bytes([
                self.read_byte(a)?,
                self.read_byte(a + 1)?,
                self.read_byte(a + 2)?,
                self.read_byte(a + 3)?,
            ]))
        }
        fn write_byte(&mut self, a: u32, v: u8) -> Result<(), Self::Error> {
            self.memory.insert(a, v);
            Ok(())
        }
        fn write_word(&mut self, a: u32, v: u32) -> Result<(), Self::Error> {
            self.word(a, v);
            Ok(())
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Self::Error> {
            let a = self.next;
            self.next += n + 16;
            self.region(a, n, 0xcc);
            self.allocations.insert(a, n);
            Ok(a)
        }
        fn free(&mut self, a: u32) -> Result<(), Self::Error> {
            self.frees.push(a);
            if a != 0 {
                self.allocations.remove(&a).ok_or("unknown allocation")?;
            }
            Ok(())
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err("unavailable collection")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            Err(VehicleInitUnavailable::Effect("unavailable enumeration"))
        }
    }
    impl CarGameInitBackend for Host {
        fn game_init_effect(
            &mut self,
            _: VehicleComponentTailMemory<'_>,
            effect: CarGameInitEffect,
        ) -> Result<u32, Self::Error> {
            self.frames.push(effect);
            match effect {
                CarGameInitEffect::FrameVirtual {
                    frame,
                    slot: 0x30,
                    arguments,
                    count: 1,
                } => self.read_word(frame + 0x200 + arguments[0] * 4),
                _ => Ok(0),
            }
        }
    }
    impl CarGameInitSupportBackend for Host {
        fn car_game_init_support_effect(
            &mut self,
            mut memory: VehicleComponentTailMemory<'_>,
            effect: CarGameInitSupportEffect,
        ) -> Result<u32, Self::Error> {
            self.effects.push(effect);
            match effect {
                CarGameInitSupportEffect::FindShadowType { .. } => Ok(77),
                CarGameInitSupportEffect::InstallShadow { .. } if self.fail_install => {
                    Err("shadow unavailable")
                }
                CarGameInitSupportEffect::ApplySourceParameters { .. } => Ok(self.source_result),
                CarGameInitSupportEffect::OpenLevelTable {
                    path: 0x63236c,
                    flags: 0,
                } => {
                    self.position = 0;
                    Ok(91)
                }
                CarGameInitSupportEffect::ReadLevelTable {
                    handle: 91,
                    target,
                    bytes,
                } => {
                    let n = (bytes as usize).min(self.file.len() - self.position);
                    let source = self.file[self.position..self.position + n].to_vec();
                    self.position += n;
                    for (index, value) in source.into_iter().enumerate() {
                        memory.write_byte_with_backend(self, target + index as u32, value)?;
                    }
                    Ok(n as u32)
                }
                CarGameInitSupportEffect::SeekLevelTable {
                    handle: 91,
                    offset,
                    origin: 1,
                } => {
                    self.position = (self.position as i64 + offset as i32 as i64) as usize;
                    Ok(0)
                }
                CarGameInitSupportEffect::CloseLevelTable { handle: 91 } => Ok(0),
                _ => Ok(0),
            }
        }
    }
    fn run(
        h: &mut Host,
        car: u32,
        stack: u32,
        entry: u32,
        arg: u32,
    ) -> Result<u32, CarGameInitUnavailable<&'static str>> {
        let mut empty = [];
        let mut scratch = VehicleComponentTailScratch {
            address: 0x79001000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        };
        car_game_init_support(
            h,
            VehicleComponentTailMemory {
                car: None,
                vehicle_address: 0,
                vehicle: &mut empty,
                scratch: &mut scratch,
            },
            car,
            stack,
            entry,
            arg,
        )
    }
    #[test]
    fn unavailable_shadow_install_keeps_reached_frame_enable_without_own_flag() {
        let c = 0x10000;
        let mut h = Host::default();
        h.region(c, 0x221c, 0);
        h.word(c + 0x21d8, 0x50000);
        h.word(c + 0x68, 0x60000);
        h.word(0x63788c, 0x70000);
        h.word(0x70024, 0x80000);
        h.fail_install = true;
        assert_eq!(
            run(&mut h, c, 0x700000, 0x429bb0, 1),
            Err(CarGameInitUnavailable::Effect("shadow unavailable"))
        );
        assert_eq!(h.memory[&(c + 0x207c)], 0);
        assert_eq!(
            h.frames,
            [CarGameInitEffect::SetFrameOn {
                frame: 0x50000,
                enabled: true
            }]
        );
        assert_eq!(h.effects.len(), 2);
    }
    #[test]
    fn rejected_level_parameters_restores_sound_phase_without_selecting_level() {
        let c = 0x10000;
        let s = 0x700000;
        let mut h = Host::default();
        h.region(c, 0x221c, 0);
        h.region(s - 0x20, 0x80, 0xa5);
        h.region(0x30000, 0x113c, 0);
        h.word(c + 0x21b4, 0x30000);
        h.word(c + 0x21b8, 0x3113c);
        h.word(c + 0x4e8, 1000f32.to_bits());
        h.word(c + 0x21ac, 77);
        assert_eq!(run(&mut h, c, s, 0x41ea00, 0), Ok(0));
        assert_eq!(h.get(0x3001c), 999f32.to_bits());
        assert_eq!(h.get(c + 0x21ac), 77);
        assert!(matches!(
            h.effects.as_slice(),
            [
                CarGameInitSupportEffect::VehicleSoundDependency {
                    entry: 0x4eb5e0,
                    ..
                },
                CarGameInitSupportEffect::ApplySourceParameters { flags: 2, .. },
                CarGameInitSupportEffect::VehicleSoundDependency {
                    entry: 0x4eb110,
                    ..
                }
            ]
        ));
    }
    #[test]
    fn debustize_visits_reverse_children_and_changes_only_visual_receivers() {
        let mut h = Host::default();
        for (frame, kind, count) in [
            (0x10000, 9, 2),
            (0x11000, 1, 1),
            (0x12000, 1, 0),
            (0x13000, 6, 0),
        ] {
            h.word(frame + 0x110, kind);
            h.word(frame + 0x11c, count);
        }
        h.word(0x10200, 0x11000);
        h.word(0x10204, 0x13000);
        h.word(0x11200, 0x12000);
        assert_eq!(run(&mut h, 0x10000, 0x700000, 0x407cd0, 0), Ok(0));
        let changed = h
            .frames
            .iter()
            .filter_map(|e| {
                if let CarGameInitEffect::FrameVirtual {
                    frame,
                    slot: 0x5c,
                    arguments: [0, 0, 0, 0],
                    count: 1,
                } = e
                {
                    Some(*frame)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        assert_eq!(changed, [0x11000, 0x12000]);
        assert_eq!(
            h.frames[0],
            CarGameInitEffect::FrameVirtual {
                frame: 0x10000,
                slot: 0x30,
                arguments: [1, 0, 0, 0],
                count: 1
            }
        );
    }
    #[test]
    fn level_loader_skips_other_names_converts_two_levels_and_grows_original_vector() {
        let c = 0x10000;
        let s = 0x700000;
        let mut h = Host::default();
        h.region(c, 0x221c, 0);
        h.region(s - 0x40, 0x22f0, 0xa5);
        h.next = 0x30000;
        h.word(c + 0x664, 1f32.to_bits());
        h.word(0x6d87c4, 0);
        for (index, byte) in b"TAXI\0".iter().enumerate() {
            h.memory.insert(c + 0x218c + index as u32, *byte);
        }
        h.file.extend(1u32.to_le_bytes());
        h.file.extend(2u32.to_le_bytes());
        let mut skipped = vec![0; 36];
        skipped[..6].copy_from_slice(b"other\0");
        h.file.extend((skipped.len() as u32).to_le_bytes());
        h.file.extend(skipped);
        let mut payload = vec![0; 36];
        payload[..5].copy_from_slice(b"TaXi\0");
        payload[32..36].copy_from_slice(&2u32.to_le_bytes());
        for _ in 0..2 {
            payload.extend(4385u32.to_le_bytes());
            payload.extend(vec![0; 4385]);
        }
        h.file.extend((payload.len() as u32).to_le_bytes());
        h.file.extend(payload);
        assert_eq!(run(&mut h, c, s, 0x41e270, 0), Ok(1));
        let begin = h.get(c + 0x21b4);
        assert_eq!(h.get(c + 0x21b8) - begin, 2 * 0x113c);
        assert_eq!(h.get(c + 0x21bc) - begin, 2 * 0x113c);
        assert_eq!(h.allocations.len(), 1);
        assert_eq!(h.get(s + 0x10), u32::MAX);
        assert_eq!(
            [
                h.memory[&(begin + 0x5e5)],
                h.memory[&(begin + 0x5e6)],
                h.memory[&(begin + 0x5e7)]
            ],
            [0xa5; 3]
        );
        assert!(matches!(
            h.effects.last(),
            Some(CarGameInitSupportEffect::CloseLevelTable { handle: 91 })
        ));
    }
    #[test]
    fn packed_parameters_transpose_rows_preserve_boolean_padding_and_string_tails() {
        let car = 0x10000;
        let source = 0x30000;
        let target = 0x40000;
        let mut h = Host::default();
        h.region(car, 0x221c, 0);
        h.region(source, 4385, 0);
        h.region(target, 4388, 0xa5);
        h.word(car + 0x664, 1f32.to_bits());
        for row in 0..11u32 {
            for column in 0..3u32 {
                h.word(
                    source + 0x50 + row * 12 + column * 4,
                    1000 + column * 100 + row,
                );
            }
        }
        for row in 0..10u32 {
            for column in 0..24u32 {
                h.word(
                    source + 0x120 + row * 0x77 + column * 4,
                    0x12000000 + column * 0x100 + row,
                );
            }
            for column in 0..3u32 {
                h.memory.insert(
                    source + 0x120 + row * 0x77 + 0x60 + column,
                    (column * 10 + row) as u8,
                );
            }
            for column in 0..5u32 {
                h.word(
                    source + 0x120 + row * 0x77 + 0x63 + column * 4,
                    0x34000000 + column * 0x100 + row,
                );
            }
        }
        let mut empty = [];
        let mut scratch = VehicleComponentTailScratch {
            address: 0x79001000,
            bytes: [0; VEHICLE_INIT_STACK_BYTES],
        };
        car_convert_packed_vehicle_parameters(
            &mut h,
            VehicleComponentTailMemory {
                car: None,
                vehicle_address: 0,
                vehicle: &mut empty,
                scratch: &mut scratch,
            },
            car,
            source,
            target,
            false,
        )
        .unwrap();
        for row in 0..11u32 {
            for column in 0..3u32 {
                assert_eq!(
                    h.get(target + (0x14 + column * 11 + row) * 4),
                    1000 + column * 100 + row
                );
            }
        }
        for row in 0..10u32 {
            for column in 0..24u32 {
                assert_eq!(
                    h.get(target + (0x48 + column * 10 + row) * 4),
                    0x12000000 + column * 0x100 + row
                );
            }
            for column in 0..3u32 {
                assert_eq!(
                    h.memory[&(target + 0x4e0 + column * 10 + row)],
                    (column * 10 + row) as u8
                );
            }
            for column in 0..5u32 {
                assert_eq!(
                    h.get(target + (0x140 + column * 10 + row) * 4),
                    0x34000000 + column * 0x100 + row
                );
            }
        }
        assert_eq!(h.memory[&(target + 0x5e4)], 0);
        assert_eq!(h.memory[&(target + 0x5e5)], 0xa5);
        assert_eq!(h.memory[&(target + 0x6dc)], 0);
        assert_eq!(h.memory[&(target + 0x6dd)], 0xa5);
    }
}
