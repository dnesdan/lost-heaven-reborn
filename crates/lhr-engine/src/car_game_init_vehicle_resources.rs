//! Vehicle resource initialization over the canonical live owner. Library
//! object calls are observable effects; their return values are never assumed.
use super::*;
#[path = "car_game_init_vehicle_resources_lights.rs"]
mod lights;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleResourceEffect {
    CreateSound {
        driver: u32,
        frame_type: u32,
    },
    LoadSound {
        vehicle: u32,
        sound: u32,
        name: u32,
        flags: u32,
    },
    SoundVirtual {
        sound: u32,
        slot: u32,
        arguments: [u32; 4],
        count: u8,
    },
    MeshVirtual {
        object: u32,
        slot: u32,
        arguments: [u32; 4],
        count: u8,
    },
    TextureOpen {
        manager: u32,
        name: u32,
        extension: u32,
        flags: u32,
        output: u32,
    },
    AddSectorSound {
        sector: u32,
        sound: u32,
    },
    DeleteSectorSound {
        sector: u32,
        sound: u32,
    },
    /// A required original leaf whose body has not yet been composed here.
    Dependency {
        entry: u32,
        vehicle: u32,
        arguments: [u32; 2],
    },
}
pub trait CarGameInitResourceBackend: CarGameInitBackend {
    fn vehicle_resource_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: VehicleResourceEffect,
    ) -> Result<u32, Self::Error>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleGameInitResourceDependency {
    SetSeatMass {
        index: i32,
        mass: u32,
    },
    InitDeformGlassTexture {
        first: u32,
        second: u32,
    },
    InitDeform {
        preserve_vertices: bool,
    },
    InitSounds {
        source: u32,
    },
    PrepareSound {
        name: u32,
        slot: u32,
        flags: u32,
        attenuation: [u32; 4],
        looping: bool,
        positioned: bool,
    },
    AddSoundsToSectors,
    StopSounds,
    ReleaseSounds,
    RemoveSoundsFromSectors,
    DoDoors {
        seconds: u32,
    },
    DoLights {
        seconds: u32,
    },
}
pub fn vehicle_game_init_resource_dependency<B: CarGameInitResourceBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    vehicle: u32,
    stack: u32,
    dependency: VehicleGameInitResourceDependency,
) -> Out<B, ()> {
    let mut call = GameInit {
        backend,
        memory,
        car: vehicle.wrapping_sub(0x70),
        stack,
    };
    match dependency {
        VehicleGameInitResourceDependency::SetSeatMass { index, mass } => {
            call.resource_set_seat_mass(index, mass)
        }
        VehicleGameInitResourceDependency::InitDeformGlassTexture { first, second } => {
            call.resource_init_glass(first, second)
        }
        VehicleGameInitResourceDependency::InitDeform { preserve_vertices } => {
            call.resource_init_deform(preserve_vertices)
        }
        VehicleGameInitResourceDependency::InitSounds { source } => {
            call.resource_init_sounds(source)
        }
        VehicleGameInitResourceDependency::PrepareSound {
            name,
            slot,
            flags,
            attenuation,
            looping,
            positioned,
        } => call.resource_prepare_sound(name, slot, flags, attenuation, looping, positioned),
        VehicleGameInitResourceDependency::AddSoundsToSectors => {
            call.resource_add_sounds_to_sectors()
        }
        VehicleGameInitResourceDependency::StopSounds => call.resource_stop_sounds(),
        VehicleGameInitResourceDependency::ReleaseSounds => call.resource_release_sounds(),
        VehicleGameInitResourceDependency::RemoveSoundsFromSectors => {
            call.resource_remove_sounds_from_sectors()
        }
        VehicleGameInitResourceDependency::DoDoors { seconds } => call.resource_do_doors(seconds),
        VehicleGameInitResourceDependency::DoLights { seconds } => call.resource_do_lights(seconds),
    }
}
/// Original traversal order, including the paired arrays traversed backwards.
fn sound_offsets() -> impl Iterator<Item = u32> {
    (0x64c..=0x6bc)
        .step_by(4)
        .chain((0..4).rev().flat_map(|i| [0x6c0 + i * 4, 0x6d0 + i * 4]))
        .chain([0x6e0, 0x6e4, 0x6e8])
        .chain((0x92c..=0x9a0).step_by(4))
        .chain((0..4).rev().flat_map(|i| [0x9a4 + i * 4, 0x9b4 + i * 4]))
        .chain([0x9c4, 0x9c8, 0x9cc])
}
impl<B: CarGameInitResourceBackend> GameInit<'_, B> {
    fn resource_sound(&mut self, sound: u32, slot: u32, args: &[u32]) -> Out<B, u32> {
        let mut arguments = [0; 4];
        if args.len() > 4 {
            return Err(CarGameInitUnavailable::Storage);
        }
        arguments[..args.len()].copy_from_slice(args);
        self.resource_effect(VehicleResourceEffect::SoundVirtual {
            sound,
            slot,
            arguments,
            count: args.len() as u8,
        })
    }
    pub(super) fn resource_release_sounds(&mut self) -> Out<B, ()> {
        for offset in sound_offsets() {
            let slot = self.car + 0x70 + offset;
            let sound = self.read(slot)?;
            if sound != 0 {
                self.resource_sound(sound, 0x2c, &[0, 0])?;
                let sound = self.read(slot)?;
                self.resource_sound(sound, 0, &[])?;
                self.put(slot, 0)?;
            }
        }
        Ok(())
    }
    pub(super) fn resource_remove_sounds_from_sectors(&mut self) -> Out<B, ()> {
        let count = self.resource_count(0x63d884, 0x63d888, 4)?;
        for index in (0..count).rev() {
            for offset in sound_offsets() {
                let sound = self.read(self.car + 0x70 + offset)?;
                if sound != 0 {
                    let sectors = self.read(0x63d884)?;
                    let sector = self.read(sectors + index * 4)?;
                    self.resource_effect(VehicleResourceEffect::DeleteSectorSound {
                        sector,
                        sound,
                    })?;
                }
            }
        }
        Ok(())
    }
    fn resource_sound_slot(&mut self, slot: u32, method: u32, args: &[u32]) -> Out<B, u32> {
        let sound = self.read(slot)?;
        self.resource_sound(sound, method, args)
    }
    fn resource_prepare_sound(
        &mut self,
        name: u32,
        slot: u32,
        flags: u32,
        attenuation: [u32; 4],
        looping: bool,
        positioned: bool,
    ) -> Out<B, ()> {
        let v = self.car + 0x70;
        let mut length = 0u32;
        while self.byte(
            name.checked_add(length)
                .ok_or(CarGameInitUnavailable::Storage)?,
        )? != 0
        {
            length = length
                .checked_add(1)
                .ok_or(CarGameInitUnavailable::Storage)?;
        }
        let old = self.read(slot)?;
        if length == 0 {
            if old != 0 {
                self.resource_sound(old, 0x2c, &[0, 0])?;
                self.resource_sound_slot(slot, 0x24, &[0])?;
                self.resource_sound_slot(slot, 0, &[])?;
                self.put(slot, 0)?;
            }
            return Ok(());
        }
        if old != 0 {
            let source = self.read(old + 0x160)?;
            let existing = if source == 0 {
                0x63d2a0
            } else {
                let string_slot = source
                    .checked_add(0xc)
                    .ok_or(CarGameInitUnavailable::Storage)?;
                let string = self.read(string_slot)?;
                if string == 0 {
                    string_slot
                } else {
                    string
                        .checked_add(8)
                        .ok_or(CarGameInitUnavailable::Storage)?
                }
            };
            let mut offset = 0;
            loop {
                let a = self.byte(name.wrapping_add(offset))?;
                let b = self.byte(existing.wrapping_add(offset))?;
                if a != b {
                    break;
                }
                if a == 0 {
                    return Ok(());
                }
                offset = offset
                    .checked_add(1)
                    .ok_or(CarGameInitUnavailable::Storage)?;
            }
            self.resource_sound(old, 0x24, &[0])?;
        } else {
            let driver = self.read(0x647ed8)?;
            let sound = self.resource_effect(VehicleResourceEffect::CreateSound {
                driver,
                frame_type: 4,
            })?;
            self.put(slot, sound)?;
            if sound == 0 {
                return Ok(());
            }
        }
        let sound = self.read(slot)?;
        let result = self.resource_effect(VehicleResourceEffect::LoadSound {
            vehicle: v,
            sound,
            name,
            flags: flags & 8,
        })?;
        if result != 0 {
            self.resource_sound_slot(slot, 0, &[])?;
            self.put(slot, 0)?;
            return Ok(());
        }
        self.resource_sound_slot(slot, 0x24, &[0])?;
        self.resource_sound_slot(slot, 0x68, &[1f32.to_bits()])?;
        self.resource_sound_slot(slot, 0x64, &[0])?;
        if looping {
            self.resource_sound_slot(slot, 0x6c, &[1])?;
        }
        let parent = if flags & 3 == 0 {
            self.resource_sound_slot(slot, 0x58, &[1])?;
            self.resource_sound_slot(slot, 0x60, &[0x40c90fda, 0x40c90fda])?;
            self.resource_sound_slot(slot, 0x5c, &attenuation)?;
            self.read(v + 0x34c)?
        } else {
            self.resource_sound_slot(slot, 0x58, &[3])?;
            self.resource_sound_slot(slot, 0x60, &[0, 0])?;
            let game = self.read(0x63d9d4)?;
            self.read(game + 0x210)?
        };
        self.resource_sound_slot(slot, 0x2c, &[parent, 0])?;
        let sound = self.read(slot)?;
        if positioned && self.byte(sound + 0xac)? & 0x80 == 0 {
            self.copy(v + 0x1e0, sound + 0x80, 3)?;
            let flags = self.read(sound + 0xac)?;
            self.put(sound + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
        }
        self.resource_sound_slot(slot, 0x18, &[])?;
        Ok(())
    }
    fn resource_init_sounds(&mut self, source: u32) -> Out<B, ()> {
        let v = self.car + 0x70;
        let normal = [0x40800000, 0x42480000, 0x3e99999a, 0x3e4ccccd];
        for (resource, offset, flags, looping, positioned) in [
            (0xc64, 0x92c, 0, false, true),
            (0xc74, 0x930, 0, false, true),
            (0xc84, 0x934, 0, false, true),
            (0xc94, 0x938, 8, true, true),
        ] {
            self.resource_prepare_sound(
                source + resource,
                v + offset,
                flags,
                normal,
                looping,
                positioned,
            )?;
        }
        for (resource, offset) in [(0xca4, 0x93c), (0xcf4, 0x950), (0xd44, 0x964)] {
            for i in 0..5 {
                self.resource_prepare_sound(
                    source + resource + i * 0x10,
                    v + offset + i * 4,
                    8,
                    normal,
                    true,
                    true,
                )?;
            }
        }
        self.resource_prepare_sound(source + 0xd94, v + 0x978, 0, normal, true, true)?;
        if self.float(v + 0x5ac)? > 0.01 {
            self.resource_prepare_sound(source + 0xda4, v + 0x97c, 8, normal, true, false)?;
        }
        self.resource_prepare_sound(source + 0xdb4, v + 0x980, 0, normal, true, false)?;
        self.resource_prepare_sound(
            source + 0xdc4,
            v + 0x984,
            0,
            [0x40800000, 0x42dc0000, 0x3ee66666, 0x3ebd70a4],
            true,
            false,
        )?;
        for (resource, offset) in [(0xdd4, 0x988), (0xde4, 0x98c), (0xdf4, 0x990)] {
            self.resource_prepare_sound(source + resource, v + offset, 0, normal, false, false)?;
        }
        for (resource, offset, flags, looping) in [
            (0xe04, 0x994, 8, true),
            (0xe14, 0x998, 0, true),
            (0xe24, 0x99c, 0, false),
            (0xe34, 0x9a0, 0, false),
        ] {
            self.resource_prepare_sound(
                source + resource,
                v + offset,
                flags,
                normal,
                looping,
                false,
            )?;
        }
        for i in (0..4).rev() {
            self.resource_prepare_sound(
                source + 0xe54 + i * 0x10,
                v + 0x9a4 + i * 4,
                0,
                normal,
                false,
                false,
            )?;
            self.resource_prepare_sound(
                source + 0xe94 + i * 0x10,
                v + 0x9b4 + i * 4,
                0,
                normal,
                false,
                false,
            )?;
        }
        self.resource_prepare_sound(source + 0xed4, v + 0x9c4, 0, normal, false, false)?;
        self.resource_prepare_sound(source + 0xee4, v + 0x9c8, 0, normal, false, false)?;
        self.resource_prepare_sound(
            0x634cd8,
            v + 0x9cc,
            8,
            [0x40c00000, 0x41f00000, 0x3e99999a, 0x3e4ccccd],
            true,
            false,
        )
    }
    fn resource_set_seat_mass(&mut self, index: i32, bits: u32) -> Out<B, ()> {
        let v = self.car + 0x70;
        let count = self.resource_count(v + 0xca4, v + 0xca8, 0x24)?;
        if index >= count as i32 {
            return Ok(());
        }
        if index < 0 {
            return Err(CarGameInitUnavailable::Storage);
        }
        let begin = self.read(v + 0xca4)?;
        self.put(begin + index as u32 * 0x24 + 0x18, bits)?;
        let extra_count = self.resource_count(v + 0xcc4, v + 0xcc8, 0x1c)?;
        let begin = self.read(v + 0xca4)?;
        let mut center = [
            self.float(begin)?,
            self.float(begin + 4)?,
            self.float(begin + 8)?,
        ];
        let mut mass = self.float(begin + 0x18)?;
        for i in 1..count {
            let next = begin + i * 0x24;
            let next_mass = self.float(next + 0x18)?;
            let mut delta = [0.; 3];
            for axis in 0..3 {
                delta[axis] = Self::subtract(self.float(next + axis as u32 * 4)?, center[axis])?;
            }
            if next_mass >= 0.001 || mass >= 0.001 {
                let ratio = Self::divide(next_mass, Self::add(mass, next_mass)?)?;
                for value in &mut delta {
                    *value = Self::multiply(*value, ratio)?;
                }
            } else {
                delta = [0.; 3];
            }
            for axis in 0..3 {
                center[axis] = Self::add(center[axis], delta[axis])?;
            }
            mass = Self::add(mass, next_mass)?;
        }
        for i in (0..extra_count).rev() {
            let begin = self.read(v + 0xcc4)?;
            let next = begin + i * 0x1c;
            let next_mass = self.float(next + 0x18)?;
            let mut delta = [0.; 3];
            for axis in 0..3 {
                delta[axis] = Self::subtract(self.float(next + axis as u32 * 4)?, center[axis])?;
            }
            if next_mass >= 0.001 || mass >= 0.001 {
                let ratio = Self::divide(next_mass, Self::add(mass, next_mass)?)?;
                for value in &mut delta {
                    *value = Self::multiply(*value, ratio)?;
                }
            } else {
                delta = [0.; 3];
            }
            for axis in 0..3 {
                center[axis] = Self::add(center[axis], delta[axis])?;
            }
            // The original reloads the extra-array base before accumulating.
            let begin = self.read(v + 0xcc4)?;
            mass = Self::add(mass, self.float(begin + i * 0x1c + 0x18)?)?;
        }
        let base = [
            self.float(v + 0x404)?,
            self.float(v + 0x408)?,
            self.float(v + 0x40c)?,
        ];
        let chassis_mass = self.float(v + 0x5f8)?;
        let ratio = Self::divide(mass, Self::add(mass, chassis_mass)?)?;
        let mut displacement = [0.; 3];
        for axis in 0..3 {
            displacement[axis] = Self::multiply(Self::subtract(center[axis], base[axis])?, ratio)?;
            let result = Self::add(base[axis], displacement[axis])?;
            self.put(v + 0x3f8 + axis as u32 * 4, result.to_bits())?;
        }
        // PC RVA 0x15ae0 uses X/Y/Z products, the native cutoff and unit shortcut.
        let [x, y, z] = displacement;
        let squared = Self::add(
            Self::add(Self::multiply(x, x)?, Self::multiply(y, y)?)?,
            Self::multiply(z, z)?,
        )?;
        let length = if (squared.to_bits() as i32) < 0x322bcc77 {
            0.
        } else if Self::subtract(squared, 1.)?.abs() < 1e-8 {
            1.
        } else {
            Self::arithmetic(crate::native_precision::sqrt(squared))?
        };
        let addition = Self::multiply(Self::multiply(length, length)?, mass)?;
        self.put_byte(v + 0xc60, 1)?;
        for axis in 0..3 {
            let inertia = self.float(v + 0x444 + axis * 4)?;
            let result = Self::divide(1., Self::add(Self::divide(1., inertia)?, addition)?)?;
            self.put(v + 0x438 + axis * 4, result.to_bits())?;
        }
        Ok(())
    }
    fn resource_do_doors(&mut self, seconds: u32) -> Out<B, ()> {
        let v = self.car + 0x70;
        let count = self.resource_count(v + 0xca4, v + 0xca8, 0x24)?;
        for i in (0..count).rev() {
            let begin = self.read(v + 0xca4)?;
            let door = self.read(begin + i * 0x24 + 0x20)?;
            if door == 0 {
                continue;
            }
            let flags = self.read(door + 0x30)? & 0xffff;
            if flags & 2 == 0 {
                continue;
            }
            let current = self.float(door + 0x20)?;
            let target = self.float(door + 0x24)?;
            let dt = f32::from_bits(seconds);
            let angle = if current < target {
                if current < 0.0001 {
                    self.resource_door_sound(i, true)?;
                }
                let speed = self.float(door + 0x2c)?;
                let result = Self::add(Self::multiply(dt, speed)?, self.float(door + 0x20)?)?;
                self.put(door + 0x20, result.to_bits())?;
                let target = self.float(door + 0x24)?;
                if target < result {
                    let flags = self.byte(door + 0x30)?;
                    self.put_byte(door + 0x30, flags & !2)?;
                    self.put(door + 0x20, target.to_bits())?;
                }
                self.float(door + 0x20)?
            } else {
                let speed = self.float(door + 0x2c)?;
                let result = Self::subtract(current, Self::multiply(dt, speed)?)?;
                self.put(door + 0x20, result.to_bits())?;
                if result <= target {
                    self.put_byte(door + 0x30, (flags & !2) as u8)?;
                    self.put_byte(door + 0x31, (flags >> 8) as u8)?;
                    self.put(door + 0x20, target.to_bits())?;
                    self.resource_door_sound(i, false)?;
                }
                self.float(door + 0x20)?
            };
            let flags = self.read(door + 0x30)? & 0xffff;
            let nonnegative = self.float(door + 0x14)? >= 0.;
            let special = flags & 4 != 0 && flags & 8 == 0;
            let angle = if nonnegative != special {
                angle
            } else {
                -angle
            };
            let rotation = crate::native_transform::SQuat::try_make_axis_angle([0., 1., 0.], angle)
                .map_err(|_| CarGameInitUnavailable::NumericDomain("door rotation"))?
                .wxyz;
            let base = [
                self.float(door + 4)?,
                self.float(door + 8)?,
                self.float(door + 12)?,
                self.float(door + 16)?,
            ];
            let [w, x, y, z] = rotation;
            let [bw, bx, by, bz] = base;
            let m = Self::multiply;
            let a = Self::add;
            let sub = Self::subtract;
            let output = [
                sub(m(w, bw)?, a(a(m(y, by)?, m(z, bz)?)?, m(x, bx)?)?)?,
                a(a(m(bx, w)?, m(x, bw)?)?, sub(m(y, bz)?, m(z, by)?)?)?,
                a(a(m(by, w)?, m(y, bw)?)?, sub(m(z, bx)?, m(x, bz)?)?)?,
                a(a(m(bz, w)?, m(z, bw)?)?, sub(m(x, by)?, m(y, bx)?)?)?,
            ];
            let frame = self.read(door)?;
            for (axis, value) in output.into_iter().enumerate() {
                self.put(frame + 0x9c + axis as u32 * 4, value.to_bits())?;
            }
            self.effect(CarGameInitEffect::NormalizeFrameQuaternion { frame })?;
            let flags = self.read(frame + 0xac)?;
            self.put(frame + 0xac, (flags & 0xffff_fecb) | 0x4000_0008)?;
            let frame = self.read(door)?;
            self.resource_virtual(frame, 0x18, &[])?;
        }
        Ok(())
    }
    fn resource_door_sound(&mut self, index: u32, opening: bool) -> Out<B, ()> {
        let v = self.car + 0x70;
        if self.byte(v + 0x432)? == 0 {
            return Ok(());
        }
        let bank = self.byte(v + 0x491)? != 0;
        let offset = match (bank, opening) {
            (true, true) => 0x6e0,
            (true, false) => 0x6e4,
            (false, true) => 0x9c4,
            (false, false) => 0x9c8,
        };
        let slot = v + offset;
        let sound = self.read(slot)?;
        if sound == 0 {
            return Ok(());
        }
        self.resource_sound(sound, 0x24, &[1])?;
        let sound = self.read(slot)?;
        let volume = self.read(0x634b00)?;
        self.resource_sound(sound, 0x68, &[volume])?;
        let sound = self.read(slot)?;
        if self.byte(sound + 0xac)? & 0x80 == 0 {
            let begin = self.read(v + 0xca4)?;
            let door = self.read(begin + index * 0x24 + 0x20)?;
            self.copy(door + 0x14, sound + 0x80, 3)?;
            let flags = self.read(sound + 0xac)?;
            self.put(sound + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
        }
        Ok(())
    }
    pub(super) fn resource_effect(&mut self, effect: VehicleResourceEffect) -> Out<B, u32> {
        self.backend
            .vehicle_resource_effect(self.memory.reborrow(), effect)
            .map_err(CarGameInitUnavailable::Effect)
    }
    pub(super) fn resource_count(
        &mut self,
        begin_slot: u32,
        end_slot: u32,
        stride: u32,
    ) -> Out<B, u32> {
        let begin = self.read(begin_slot)?;
        if begin == 0 {
            return Ok(0);
        }
        let end = self.read(end_slot)?;
        let bytes = end.wrapping_sub(begin) as i32;
        if bytes < 0 || !(bytes as u32).is_multiple_of(stride) {
            return Err(CarGameInitUnavailable::Storage);
        }
        Ok(bytes as u32 / stride)
    }
    pub(super) fn resource_virtual(&mut self, object: u32, slot: u32, args: &[u32]) -> Out<B, u32> {
        let mut arguments = [0; 4];
        if args.len() > 4 {
            return Err(CarGameInitUnavailable::Storage);
        }
        arguments[..args.len()].copy_from_slice(args);
        self.resource_effect(VehicleResourceEffect::MeshVirtual {
            object,
            slot,
            arguments,
            count: args.len() as u8,
        })
    }
    fn resource_stop_sounds(&mut self) -> Out<B, ()> {
        for offset in sound_offsets() {
            let sound = self.read(self.car + 0x70 + offset)?;
            if sound != 0 {
                self.resource_effect(VehicleResourceEffect::SoundVirtual {
                    sound,
                    slot: 0x88,
                    arguments: [0, 1, 0, 0],
                    count: 2,
                })?;
            }
        }
        Ok(())
    }
    fn resource_add_sounds_to_sectors(&mut self) -> Out<B, ()> {
        let count = self.resource_count(0x63d884, 0x63d888, 4)?;
        for index in (0..count).rev() {
            for offset in sound_offsets() {
                let sound = self.read(self.car + 0x70 + offset)?;
                if sound != 0 {
                    let sectors = self.read(0x63d884)?;
                    let sector = self.read(sectors + index * 4)?;
                    self.resource_effect(VehicleResourceEffect::AddSectorSound { sector, sound })?;
                }
            }
        }
        Ok(())
    }
    fn resource_init_glass(&mut self, first: u32, second: u32) -> Out<B, ()> {
        if first == 0 || second == 0 {
            return Ok(());
        }
        let v = self.car + 0x70;
        self.put(self.stack, first)?;
        let mut i = 0;
        loop {
            let count = self.resource_count(v + 0x218, v + 0x21c, 0x34)?;
            if i >= count {
                break;
            }
            let records = self.read(v + 0x218)?;
            let kind = self.read(records + i * 0x34 + 0xc)? & 0xffff;
            if kind == 3 {
                let output = self.stack;
                let manager = self.read(0x647ee0)?;
                let result = self.resource_effect(VehicleResourceEffect::TextureOpen {
                    manager,
                    name: 0x634944,
                    extension: 0x634950,
                    flags: 4,
                    output,
                })?;
                if result == 0 {
                    let records = self.read(v + 0x218)?;
                    let material = self.read(records + i * 0x34 + 8)?;
                    let texture = self.read(output)?;
                    self.resource_virtual(material, 0x14, &[texture])?;
                    let texture = self.read(output)?;
                    self.resource_virtual(texture, 0, &[])?;
                }
            }
            i += 1;
        }
        Ok(())
    }
    fn resource_mesh_at(&mut self, header: u32, index: u32) -> Out<B, u32> {
        let begin = self.read(header)?;
        self.read(begin + index * 0x34 + 4)
    }
    fn resource_copy_vertices(&mut self, source: u32, destination: u32) -> Out<B, ()> {
        let from = self.resource_virtual(source, 4, &[0])?;
        let to = self.resource_virtual(destination, 4, &[0])?;
        let count = self.read(destination + 8)?;
        for i in (0..count).rev() {
            self.copy(from + i * 0x20, to + i * 0x20, 3)?;
        }
        self.resource_virtual(source, 8, &[])?;
        self.resource_virtual(destination, 8, &[])?;
        Ok(())
    }
    pub(super) fn resource_init_deform(&mut self, preserve: bool) -> Out<B, ()> {
        let v = self.car + 0x70;
        let count = self.resource_count(v + 0x218, v + 0x21c, 0x34)?;
        for i in (0..count).rev() {
            if i >= self.resource_count(v + 0x208, v + 0x20c, 0x34)? {
                continue;
            }
            if !preserve {
                let old = self.resource_mesh_at(v + 0x208, i)?;
                let source = self.resource_virtual(old, 4, &[0])?;
                let new = self.resource_mesh_at(v + 0x218, i)?;
                let destination = self.resource_virtual(new, 4, &[0])?;
                self.resource_copy_vertices(source, destination)?;
                let old = self.resource_mesh_at(v + 0x208, i)?;
                if self.read(old + 8)? > 1 {
                    let source = self.resource_virtual(old, 4, &[1])?;
                    let new = self.resource_mesh_at(v + 0x218, i)?;
                    let destination = self.resource_virtual(new, 4, &[1])?;
                    if source != 0 && destination != 0 {
                        self.resource_copy_vertices(source, destination)?;
                    }
                }
                let begin = self.read(v + 0x218)?;
                if self.read(begin + i * 0x34 + 0xc)? & 0xffff == 3 {
                    let new = self.resource_mesh_at(v + 0x218, i)?;
                    let mesh = self.resource_virtual(new, 4, &[0])?;
                    let material = self.resource_virtual(mesh, 0x14, &[0])?;
                    let old = self.read(v + 0x208)?;
                    let texture = self.read(old + i * 0x34 + 8)?;
                    self.resource_virtual(material, 0, &[texture])?;
                    let begin = self.read(v + 0x218)?;
                    let frame = self.read(begin + i * 0x34)?;
                    self.resource_virtual(frame, 0x24, &[1])?;
                }
                let mesh = self.resource_mesh_at(v + 0x218, i)?;
                let part = self.resource_virtual(mesh, 4, &[0])?;
                self.resource_virtual(part, 0x28, &[])?;
                let old = self.resource_mesh_at(v + 0x208, i)?;
                if self.read(old + 8)? > 1 {
                    let mesh = self.resource_mesh_at(v + 0x218, i)?;
                    let part = self.resource_virtual(mesh, 4, &[1])?;
                    self.resource_virtual(part, 0x28, &[])?;
                }
                let mesh = self.resource_mesh_at(v + 0x218, i)?;
                self.resource_virtual(mesh, 0x14, &[])?;
                let begin = self.read(v + 0x218)?;
                let frame = self.read(begin + i * 0x34)?;
                self.resource_virtual(frame, 0x78, &[0])?;
                let old = self.resource_mesh_at(v + 0x208, i)?;
                if self.read(old + 8)? > 1 {
                    let begin = self.read(v + 0x218)?;
                    let frame = self.read(begin + i * 0x34)?;
                    self.resource_virtual(frame, 0x78, &[0])?;
                }
                let begin = self.read(v + 0x218)?;
                let frame = self.read(begin + i * 0x34)?;
                self.resource_virtual(frame, 0x24, &[1])?;
                let begin = self.read(v + 0x218)?;
                let frame = self.read(begin + i * 0x34)?;
                self.resource_virtual(frame, 0x18, &[])?;
                let begin = self.read(v + 0x218)?;
                self.put_byte(begin + i * 0x34 + 0xe, 0)?;
                self.put_byte(begin + i * 0x34 + 0xf, 0)?;
            }
            let begin = self.read(v + 0x218)?;
            let value = self.read(begin + i * 0x34 + 0x18)?;
            self.put(begin + i * 0x34 + 0x20, value)?;
        }
        let function = self.read(v + 0x318)?;
        if function != 0 {
            let context = self.read(v + 0xc74)?;
            self.effect(CarGameInitEffect::FlagsCallback { function, context })?;
        }
        Ok(())
    }
}
