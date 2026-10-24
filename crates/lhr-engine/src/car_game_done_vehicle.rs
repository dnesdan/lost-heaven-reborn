//! Original embedded Vehicle GameDone, PC RVA 0xc55a0. Explicit shutdown,
//! including frame restoration and ownership release, uses the live owner.
use super::*;

pub fn vehicle_game_done<B: CarGameInitResourceBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    vehicle: u32,
    stack: u32,
) -> Out<B, ()> {
    GameInit {
        backend,
        memory,
        car: vehicle.wrapping_sub(0x70),
        stack,
    }
    .vehicle_game_done_body()
}
pub fn vehicle_end_skid<B: CarGameInitResourceBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    vehicle: u32,
    stack: u32,
    index: u32,
) -> Out<B, ()> {
    GameInit {
        backend,
        memory,
        car: vehicle.wrapping_sub(0x70),
        stack,
    }
    .resource_end_skid(index)
}
impl<B: CarGameInitResourceBackend> GameInit<'_, B> {
    fn vehicle_game_done_body(&mut self) -> Out<B, ()> {
        let v = self.car + 0x70;
        vehicle_game_init_small_dependency(
            self.backend,
            self.memory.reborrow(),
            v,
            self.stack,
            VehicleGameInitSmallDependency::SetDynamicCollsOn {
                first: 0,
                second: 0,
            },
        )?;
        // The two range destructors are empty; vector storage remains owned.
        let begin = self.read(v + 0x1fe4)?;
        self.put(v + 0x1fe8, begin)?;
        let begin = self.read(v + 0x1fd4)?;
        self.put(v + 0x1fd8, begin)?;
        self.resource_release_sounds()?;
        let mut wheel = self.read(v + 0xc34)?;
        while wheel != 0 {
            if self.byte(wheel + 0x120)? & 1 != 0 {
                let frame = self.read(wheel + 8)?;
                for (i, value) in [1f32.to_bits(), 0, 0, 0].into_iter().enumerate() {
                    self.put(frame + 0x9c + i as u32 * 4, value)?;
                }
                self.effect(CarGameInitEffect::NormalizeFrameQuaternion { frame })?;
                let flags = self.read(frame + 0xac)?;
                self.put(frame + 0xac, (flags & 0xffff_fecb) | 0x4000_0008)?;
                let current = self.read(wheel + 8)?;
                self.resource_virtual(current, 0x18, &[])?;
            }
            let frame = self.read(wheel + 4)?;
            self.resource_virtual(frame, 0x24, &[1])?;
            wheel = self.read(wheel + 0x1d0)?;
        }
        let mut i = 0;
        while (i as i32) < self.read(v + 0x4c4)? as i32 {
            self.resource_end_skid(i)?;
            let wheels = self.read(v + 0xc38)?;
            let wheel = self.read(wheels + i * 4)?;
            let frame = self.read(wheel + 8)?;
            if self.byte(frame + 0xac)? & 0x80 == 0 {
                self.copy(wheel + 0x10, frame + 0x80, 3)?;
                let flags = self.read(frame + 0xac)?;
                self.put(frame + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
            }
            let wheels = self.read(v + 0xc38)?;
            let wheel = self.read(wheels + i * 4)?;
            let frame = self.read(wheel + 8)?;
            self.resource_virtual(frame, 0x18, &[])?;
            let wheels = self.read(v + 0xc38)?;
            let wheel = self.read(wheels + i * 4)?;
            let count = self.resource_count(wheel + 0x12c, wheel + 0x130, 0x14)?;
            for j in (0..count).rev() {
                let wheels = self.read(v + 0xc38)?;
                let wheel = self.read(wheels + i * 4)?;
                let begin = self.read(wheel + 0x12c)?;
                let object = self.read(begin + j * 0x14)?;
                self.resource_virtual(object, 0, &[])?;
                let wheels = self.read(v + 0xc38)?;
                let wheel = self.read(wheels + i * 4)?;
                let begin = self.read(wheel + 0x12c)?;
                let object = self.read(begin + j * 0x14 + 8)?;
                self.resource_virtual(object, 0, &[])?;
            }
            let wheels = self.read(v + 0xc38)?;
            let wheel = self.read(wheels + i * 4)?;
            let begin = self.read(wheel + 0x12c)?;
            self.put(wheel + 0x130, begin)?;
            i += 1;
        }
        self.resource_init_deform(false)?;
        self.put(v + 0x11c, 0)?;
        self.resource_do_lights(0x3dcccccd)?;
        self.resource_remove_sounds_from_sectors()?;
        self.resource_remove_projectors()
    }
    fn resource_end_skid(&mut self, index: u32) -> Out<B, ()> {
        let v = self.car + 0x70;
        let wheels = self.read(v + 0xc38)?;
        let wheel = self.read(wheels + index * 4)?;
        let count = self.resource_count(wheel + 0x12c, wheel + 0x130, 0x14)?;
        let index = self.read(wheel + 0x16c)?;
        if index < count {
            let begin = self.read(wheel + 0x12c)?;
            if self.byte(begin + index * 0x14 + 0x10)? != 0 {
                self.put_byte(begin + index * 0x14 + 0x10, 0)?;
                let index = self.read(wheel + 0x16c)?.wrapping_add(1);
                self.put(wheel + 0x16c, index)?;
                let count = self.resource_count(wheel + 0x12c, wheel + 0x130, 0x14)?;
                if index >= count {
                    self.put(wheel + 0x16c, 0)?;
                }
            }
        }
        Ok(())
    }
    fn resource_remove_projectors(&mut self) -> Out<B, ()> {
        let v = self.car + 0x70;
        let count = self.resource_count(v + 0x100, v + 0x104, 0x20)?;
        for i in (0..count).rev() {
            let begin = self.read(v + 0x100)?;
            let light = self.read(begin + i * 0x20 + 0xc)?;
            if light == 0 {
                continue;
            }
            let count = self.resource_count(v + 0x110, v + 0x114, 0x1c)?;
            let mut found = false;
            let begin = self.read(v + 0x110)?;
            for j in (0..count).rev() {
                if self.read(begin + j * 0x1c + 8)? == light {
                    found = true;
                    break;
                }
            }
            if !found {
                let count = self.resource_count(0x63d894, 0x63d898, 4)?;
                for j in (0..count).rev() {
                    let begin = self.read(0x63d894)?;
                    let sector = self.read(begin + j * 4)?;
                    let begin = self.read(v + 0x100)?;
                    let light = self.read(begin + i * 0x20 + 0xc)?;
                    self.resource_virtual(sector, 0x54, &[light])?;
                }
            }
            let begin = self.read(v + 0x100)?;
            let light = self.read(begin + i * 0x20 + 0xc)?;
            self.resource_virtual(light, 0x2c, &[0, 0])?;
        }
        let count = self.resource_count(v + 0x110, v + 0x114, 0x1c)?;
        for i in (0..count).rev() {
            let count = self.resource_count(0x63d894, 0x63d898, 4)?;
            for j in (0..count).rev() {
                let begin = self.read(0x63d894)?;
                let sector = self.read(begin + j * 4)?;
                let begin = self.read(v + 0x110)?;
                let light = self.read(begin + i * 0x1c + 8)?;
                self.resource_virtual(sector, 0x54, &[light])?;
            }
            let begin = self.read(v + 0x110)?;
            let light = self.read(begin + i * 0x1c + 8)?;
            self.resource_virtual(light, 0x2c, &[0, 0])?;
        }
        Ok(())
    }
}
