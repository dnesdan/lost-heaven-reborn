//! Embedded C_Vehicle::GameInit dependency, PC RVA0xc3fd0. Named from
//! the PS2 GameInit__9C_VehicleFv call in C_car::GameInit.
use super::*;
use crate::native_transform::{vector_length, SMatrix};

impl<B: CarGameInitBackend> GameInit<'_, B> {
    fn world_frame(&mut self, frame: u32) -> Out<B, ()> {
        if self.byte(frame + 0xac)? & 0x20 == 0 {
            self.effect(CarGameInitEffect::UpdateWorldMatrix { frame })?;
        }
        Ok(())
    }
    fn inverse_at(&mut self, address: u32) -> Out<B, ()> {
        let mut elements = [0.; 16];
        for (i, v) in elements.iter_mut().enumerate() {
            *v = self.float(address + i as u32 * 4)?;
        }
        let (result, _) = SMatrix { elements }
            .inverse()
            .map_err(|_| CarGameInitUnavailable::NumericDomain("Vehicle GameInit inverse"))?;
        for (i, v) in result.elements.into_iter().enumerate() {
            self.put(address + i as u32 * 4, v.to_bits())?;
        }
        Ok(())
    }
    fn bounds_radius(&mut self, lo: u32, hi: u32) -> Out<B, f32> {
        let mut low = [0.; 3];
        let mut high = [0.; 3];
        for i in 0..3 {
            low[i] = self.float(lo + i as u32 * 4)?;
            high[i] = self.float(hi + i as u32 * 4)?;
        }
        let mut maximum = 0f32;
        for bits in 0..8 {
            let mut point = [0.; 3];
            for i in 0..3 {
                point[i] = if bits & (1 << i) == 0 {
                    low[i]
                } else {
                    high[i]
                };
            }
            if !point
                .iter()
                .copied()
                .all(crate::native_transform::is_verified_scalar)
            {
                return Err(CarGameInitUnavailable::NumericDomain("Vehicle bounds"));
            }
            let radius = vector_length(point);
            if radius > maximum {
                maximum = radius;
            }
        }
        Self::add(maximum, f32::from_bits(0x3dcccccd))
    }
    fn clear_words(&mut self, address: u32, count: u32) -> Out<B, ()> {
        for i in 0..count {
            self.put(address + i * 4, 0)?;
        }
        Ok(())
    }
    pub(super) fn vehicle_dependency(&mut self, entry: u32, arguments: [u32; 2]) -> Out<B, ()> {
        self.effect(CarGameInitEffect::VehicleDependency {
            entry,
            vehicle: self.car + 0x70,
            arguments,
        })?;
        Ok(())
    }
    fn collision_record(
        &mut self,
        vehicle: u32,
        record: u32,
        bounds: [u32; 2],
        kind: u32,
        radius: f32,
        shape: u32,
    ) -> Out<B, ()> {
        self.clear_words(record, 18)?;
        self.put(record, shape)?;
        let frame = self.read(vehicle + 0x34c)?;
        self.world_frame(frame)?;
        self.copy(frame + 0x40, record + 0x10, 3)?;
        self.put(record + 0x1c, radius.to_bits())?;
        self.copy(bounds[0], record + 0x24, 3)?;
        self.copy(bounds[1], record + 0x30, 3)?;
        self.put(record + 0x3c, vehicle + 0xce8)?;
        self.put(record + 0x40, vehicle + 0xd28)?;
        let context = self.read(vehicle + 0xc74)?;
        self.put(record + 0x44, context)?;
        self.put(record + 4, frame)?;
        self.put(record + 8, kind)
    }
    pub(super) fn vehicle_game_init(&mut self) -> Out<B, ()> {
        let v = self.car + 0x70;
        let present = |s: &mut Self, begin: u32, end: u32| -> Out<B, bool> {
            let b = s.read(v + begin)?;
            if b == 0 {
                return Ok(false);
            }
            let e = s.read(v + end)?;
            Ok((e.wrapping_sub(b) as i32) / 12 != 0)
        };
        let any = present(self, 0xdc, 0xe0)? || present(self, 0xec, 0xf0)?;
        self.put_byte(v + 0x99, u8::from(any))?;
        for (o, w) in [
            (0x5e0, 0x3f800000),
            (0x5e4, 0x3f800000),
            (0xc0, 0),
            (0x11c, 0),
            (0x120, 0),
            (0x5e8, 0x20),
        ] {
            self.put(v + o, w)?;
        }
        self.put_byte(v + 0xbc, 0)?;
        let frame = self.read(v + 0x34c)?;
        self.effect(CarGameInitEffect::UpdateFrame { frame })?;
        let frame = self.read(v + 0x34c)?;
        self.world_frame(frame)?;
        self.copy(frame + 0x10, v + 0xce8, 16)?;
        self.copy(frame + 0x10, v + 0xd28, 16)?;
        self.inverse_at(v + 0xd28)?;
        self.copy(v + 0xce8, v + 0xd68, 16)?;
        self.copy(v + 0xd28, v + 0xda8, 16)?;
        for o in [0xd98, 0xd9c, 0xda0, 0xdd8, 0xddc, 0xde0] {
            self.put(v + o, 0)?;
        }
        for (src, dst) in [(0xce8, 0xc48), (0xcf8, 0xc54), (0xd08, 0xc3c)] {
            self.copy(v + src, v + dst, 3)?;
        }
        let frame = self.read(v + 0x34c)?;
        self.world_frame(frame)?;
        self.copy(frame + 0x40, v + 0x320, 3)?;
        self.put(self.stack - 0xb4, v + 0x320)?;
        let mut matrix = SMatrix { elements: [0.; 16] };
        let mut point = [0.; 3];
        for (i, x) in matrix.elements.iter_mut().enumerate() {
            *x = self.float(v + 0xce8 + i as u32 * 4)?;
        }
        for (i, x) in point.iter_mut().enumerate() {
            *x = self.float(v + 0x3f8 + i as u32 * 4)?;
        }
        let point = matrix
            .try_transform_point(point)
            .map_err(|_| CarGameInitUnavailable::NumericDomain("Vehicle pivot transform"))?;
        for (i, x) in point.into_iter().enumerate() {
            self.put(v + 0x398 + i as u32 * 4, x.to_bits())?;
        }
        let scene = self.read(0x63d9d4)?;
        let frame = self.read(v + 0x34c)?;
        self.effect(CarGameInitEffect::SetFrameSector {
            scene,
            frame,
            position: v + 0x398,
        })?;
        let frame = self.read(v + 0x34c)?;
        let sector = self.read(frame + 0x120)?;
        self.put(v + 0x350, 0)?;
        self.put(v + 0xac, sector)?;
        let radius = self.bounds_radius(v + 0x364, v + 0x370)?;
        let shape = self.read(v + 0x1ff0)?;
        self.clear_words(v + 0x22c, 18)?;
        self.put(v + 0x22c, shape)?;
        let frame = self.read(v + 0x34c)?;
        self.world_frame(frame)?;
        self.copy(frame + 0x40, v + 0x23c, 3)?;
        self.put(v + 0x248, radius.to_bits())?;
        for i in 0..3 {
            let x = Self::add(self.float(v + 0x364 + i * 4)?, 0.1)?;
            self.put(v + 0x250 + i * 4, x.to_bits())?;
        }
        for i in 0..3 {
            let x = Self::subtract(self.float(v + 0x370 + i * 4)?, 0.1)?;
            self.put(v + 0x25c + i * 4, x.to_bits())?;
        }
        let frame = self.read(v + 0x34c)?;
        self.world_frame(frame)?;
        self.copy(frame + 0x10, v + 0xce8, 16)?;
        self.put(v + 0x268, v + 0xce8)?;
        self.copy(v + 0xce8, v + 0xd28, 16)?;
        self.inverse_at(v + 0xd28)?;
        self.put(v + 0x26c, v + 0xd28)?;
        let frame = self.read(v + 0x34c)?;
        self.put(v + 0x230, frame)?;
        let low_y = Self::subtract(self.float(v + 0x368)?, 0.5)?;
        self.put(v + 0x368, low_y.to_bits())?;
        let context = self.read(v + 0xc74)?;
        self.put(v + 0x270, context)?;
        self.put(v + 0x234, 0x201)?;
        let radius = self.bounds_radius(v + 0x364, v + 0x370)?;
        self.collision_record(v, v + 0x274, [v + 0x364, v + 0x370], 0x202, radius, shape)?;
        let y = Self::add(self.float(v + 0x368)?, 0.5)?;
        self.put(v + 0x368, y.to_bits())?;
        let extra = self.read(v + 0x1fb4)?;
        if extra != 0 {
            let end = self.read(v + 0x1fb8)?;
            let count = ((end.wrapping_sub(extra) as i32) / 12) as u32 >> 1;
            for i in 0..count {
                let b = self.read(v + 0x1fb4)?;
                let lo = b + i * 24;
                let hi = lo + 12;
                let radius = self.bounds_radius(lo, hi)?;
                let shapes = self.read(v + 0x1ffc)?;
                let shape = self.read(shapes + i * 8)?;
                let record = self.stack - 0x4c;
                self.collision_record(v, record, [lo, hi], 0x201, radius, shape)?;
                let insertion = self.read(v + 0x1fe8)?;
                self.effect(CarGameInitEffect::InsertVehicleCollision {
                    header: v + 0x1fe0,
                    insertion,
                    record,
                })?;
            }
        }
        let mut index = 0u32;
        loop {
            let begin = self.read(v + 0x1fe4)?;
            if begin == 0 {
                break;
            }
            let end = self.read(v + 0x1fe8)?;
            let count = (end.wrapping_sub(begin) as i32) / 0x48;
            if count < 0 {
                return Err(CarGameInitUnavailable::Storage);
            }
            if index >= count as u32 {
                break;
            }
            let object = begin + index * 0x48;
            self.put(self.stack - 0xdc, object)?;
            let insertion = self.read(v + 0x1fd8)?;
            self.effect(CarGameInitEffect::InsertVehicleCollisionPointer {
                header: v + 0x1fd0,
                insertion,
                source: self.stack - 0xdc,
            })?;
            index += 1;
        }
        self.put_byte(v + 0x2c0, 0)?;
        for o in [0xf8, 0xcd0, 0xcd4, 0xcd8, 0xcdc, 0xce0, 0xce4] {
            self.put(v + o, 0)?;
        }
        for o in [0x1faa, 0x1fab, 0x1fac] {
            self.put_byte(v + o, 0)?;
        }
        self.vehicle_dependency(0x4cdcc0, [1, 1])?;
        self.put_byte(v + 0x394, 0)?;
        self.clear_words(v + 0xc78, 10)?;
        for o in [
            0x420, 0x424, 0x428, 0x1f90, 0x1f94, 0x1f98, 0x194, 0x358, 0x3b8, 0xb4, 0x35c, 0x464,
            0x1dc, 0x360,
        ] {
            self.put(v + o, 0)?;
        }
        self.put(v + 0x1a4, 1)?;
        self.put(v + 0xb0, 1f32.to_bits())?;
        self.put_byte(v + 0xc60, 0)?;
        for o in [
            0x1fa4, 0x59c, 0x5a0, 0x5a4, 0x5a8, 0x548, 0x54c, 0x1f90, 0x1f94, 0x1f98, 0x420, 0x424,
            0x428, 0x4a0, 0x434, 0x3cc,
        ] {
            self.put(v + o, 0)?;
        }
        for o in [0xc2c, 0x638] {
            self.put_byte(v + o, 0)?;
        }
        for (o, w) in [
            (0x5dc, 0x3f800000),
            (0x55c, 1),
            (0x560, 1),
            (0x4a4, 0x447a0000),
            (0x5b8, 0),
        ] {
            self.put(v + o, w)?;
        }
        self.put_byte(v + 0x5f0, 0)?;
        for o in [
            0x540, 0x544, 0x624, 0x494, 0x3a4, 0x3a8, 0xc30, 0x634, 0x628, 0x1a8, 0x3bc, 0x3c0,
            0x4c8, 0x5b4, 0x4c, 0x50, 0x54, 0x58, 0x5c, 0x60,
        ] {
            self.put(v + o, 0)?;
        }
        for o in [0x430, 0x431, 0x4a8, 0x4a9, 0x4aa, 0x4ab] {
            self.put_byte(v + o, 0)?;
        }
        self.put_byte(v + 0x14, 0x30)?;
        self.put_byte(v + 0x15, 0x75)?;
        let wheels = self.read(v + 0x4c4)?;
        if wheels > 1 {
            let ptr = self.read(v + 0xc38)?;
            for (first, last, target) in [(0, 1, 0x4c), (wheels - 2, wheels - 1, 0x58)] {
                let a = self.read(ptr + first * 4)?;
                let b = self.read(ptr + last * 4)?;
                for i in 0..3 {
                    let x = self.float(a + 0x10 + i * 4)?;
                    let y = self.float(b + 0x10 + i * 4)?;
                    let delta = Self::subtract(y, x)?;
                    let half = Self::multiply(delta, 0.5)?;
                    let mid = Self::add(half, x)?;
                    self.put(v + target + i * 4, mid.to_bits())?;
                }
            }
        }
        for i in (0..wheels).rev() {
            let ptr = self.read(v + 0xc38)?;
            let wheel = self.read(ptr + i * 4)?;
            let frame = self.read(wheel + 4)?;
            self.world_frame(frame)?;
            self.copy(frame + 0x40, wheel + 0x1c, 3)?;
            for o in [0x16c, 0x94] {
                self.put(wheel + o, 0)?;
            }
            let flags = self.read(wheel + 0x120)?;
            self.put(wheel + 0x120, flags & 0x2effff07)?;
            for o in [
                0x4c, 0x50, 0x54, 0x58, 0x5c, 0x60, 0x114, 0xb4, 0x1b8, 0x1bc, 0x1c0,
            ] {
                self.put(wheel + o, 0)?;
            }
            self.put(wheel + 0x1c8, 1000f32.to_bits())?;
            self.put(wheel + 0x124, 0)?;
            let frame = self.read(wheel + 4)?;
            self.effect(CarGameInitEffect::SetFrameOn {
                frame,
                enabled: true,
            })?;
            for o in [0x1cc, 0xb0, 0x90, 0xc8, 0x64, 0x68, 0x6c, 0x70, 0x74, 0x78] {
                self.put(wheel + o, 0)?;
            }
            self.put(wheel + 0xcc, 0x3f333333)?;
            let material = self.read(wheel + 0xac)? as i32;
            if material > 0 {
                let table = self.read(0x63d9c8)?;
                let source = self.read(table + 0x24 + material as u32 * 0x28)?;
                let begin = self.read(source + 4)?;
                let mut found = u32::MAX;
                if begin != 0 {
                    let end = self.read(source + 8)?;
                    let count = (end.wrapping_sub(begin) as i32) / 0x5c;
                    if count < 0 {
                        return Err(CarGameInitUnavailable::Storage);
                    }
                    for index in (0..count as u32).rev() {
                        let key = self.read(begin + index * 0x5c)?;
                        if key == self.read(wheel + 0xb4)? {
                            found = index;
                            break;
                        }
                    }
                }
                self.put(wheel + 0xb0, found)?;
            }
        }
        // The original non-network path reads wheel frame property strings and
        // allocates skid-mark model pairs. It is an explicit resource boundary.
        if self.byte(v + 0xc70)? & 1 == 0 {
            self.effect(CarGameInitEffect::InitializeSkidModels {
                vehicle: v,
                position_pointer_slot: self.stack - 0xb4,
            })?;
        }
        let doors = self.read(v + 0xca4)?;
        if doors != 0 {
            let end = self.read(v + 0xca8)?;
            let count = (end.wrapping_sub(doors) as i32) / 0x24;
            if count < 0 {
                return Err(CarGameInitUnavailable::Storage);
            }
            for i in (0..count as u32).rev() {
                let base = self.read(v + 0xca4)?;
                self.put(base + i * 0x24 + 0x18, 0)?;
            }
        }
        self.vehicle_dependency(0x4cc6e0, [0, 0])?;
        self.put_byte(v + 0x154, 0)?;
        self.put(v + 0xc64, 0)?;
        self.vehicle_dependency(0x4eac70, [v + 0xe6c, 0])?;
        self.vehicle_dependency(0x4eb110, [0, 0])?;
        self.vehicle_dependency(0x4d5150, [v + 0x1548, v + 0x1558])?;
        self.vehicle_dependency(0x4d54e0, [0, 0])?;
        self.vehicle_dependency(0x4d51f0, [1, 0])?;
        self.vehicle_dependency(0x4ce190, [v + 0x4ac, 0])?;
        self.put_byte(v + 0x48, 1)?;
        self.put_byte(v + 0x1fa8, 0)?;
        self.put_byte(v + 0x1faa, 0)?;
        self.vehicle_dependency(0x4ce290, [0, 0])?;
        for (o, w) in [
            (0x40, 0),
            (0x44, 0x3f800000),
            (0x30, 0),
            (0x34, 0),
            (0x38, 0),
            (0x5cc, 0xbf800000),
            (0x2c, 0x3f800000),
            (0x140, 0),
            (0x148, 0),
            (0x144, 0),
            (0x150, 0),
            (0x200, 0),
            (0x554, 0),
            (0x610, 0),
            (0x1c, 0),
            (0x20, 0),
            (0xc20, 0),
        ] {
            self.put(v + o, w)?;
        }
        for (o, b) in [(0x3c, 0), (0x28, 0), (0x4cc, 1), (0x1fa9, 0)] {
            self.put_byte(v + o, b)?;
        }
        self.vehicle_dependency(0x4d6840, [0, 0])?;
        let position = self.read(self.stack - 0xb4)?;
        self.put(v + 4, 0)?;
        self.copy(position, v + 8, 3)
    }
}
