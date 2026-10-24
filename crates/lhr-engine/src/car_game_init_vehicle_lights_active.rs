//! Shared active/off operations of DoLights's nine original record branches.
use super::*;

impl<B: CarGameInitResourceBackend> GameInit<'_, B> {
    fn resource_shared_light_on(&mut self, index: u32) -> Out<B, bool> {
        let record = self.resource_light_record(index)?;
        if (self.read(record + 0x14)? as i32) >= 0 {
            return Ok(false);
        }
        let number = self.read(record + 0x1c)? & 0xffff;
        let shared = self.read(self.car + 0x70 + 0x110)? + number * 0x1c;
        Ok(self.byte(shared + 0x18)? & 0x80 != 0)
    }
    pub(super) fn resource_extinguish_light_record(&mut self, index: u32) -> Out<B, ()> {
        let record = self.resource_light_record(index)?;
        let frame = self.read(record + 4)?;
        if frame != 0 {
            self.resource_virtual(frame, 0x24, &[0])?;
        }
        let record = self.resource_light_record(index)?;
        let light = self.read(record + 8)?;
        if light != 0 && !self.resource_shared_light_on(index)? {
            self.resource_virtual(light, 0x24, &[0])?;
            let record = self.resource_light_record(index)?;
            let projector = self.read(record + 12)?;
            self.resource_virtual(projector, 0x24, &[0])?;
        }
        Ok(())
    }
    pub(super) fn resource_active_projectors(
        &mut self,
        index: u32,
        kind: u32,
        requests: u32,
    ) -> Out<B, ()> {
        let vehicle = self.car + 0x70;
        if self.read(vehicle + 0xc68)? & 0x100 != 0 {
            self.resource_position_light(index, 8, kind, requests)?;
        } else if !self.resource_shared_light_on(index)? {
            let record = self.resource_light_record(index)?;
            let light = self.read(record + 8)?;
            self.resource_virtual(light, 0x24, &[0])?;
        }
        if self.read(vehicle + 0xc68)? & 0x1000 != 0 {
            self.resource_position_light(index, 12, kind, requests)?;
        } else {
            // Original 4d9d18/4da533 disables record+8 here, including when
            // the preceding branch enabled it; it does not select record+12.
            let record = self.resource_light_record(index)?;
            let light = self.read(record + 8)?;
            if light != 0 && !self.resource_shared_light_on(index)? {
                self.resource_virtual(light, 0x24, &[0])?;
            }
        }
        Ok(())
    }
    fn resource_light_source(&mut self, index: u32, mark_shared: bool) -> Out<B, u32> {
        let record = self.resource_light_record(index)?;
        if (self.read(record + 0x14)? as i32) < 0 {
            let number = self.read(record + 0x1c)? & 0xffff;
            let shared = self.read(self.car + 0x70 + 0x110)? + number * 0x1c;
            if mark_shared {
                let flags = self.byte(shared + 0x18)?;
                self.put_byte(shared + 0x18, flags | 0x80)?;
            }
            self.read(shared)
        } else {
            self.read(record)
        }
    }
    fn resource_light_world(&mut self, frame: u32) -> Out<B, ()> {
        if self.byte(frame + 0xac)? & 0x20 == 0 {
            self.effect(CarGameInitEffect::UpdateWorldMatrix { frame })?;
        }
        Ok(())
    }
    fn resource_position_light(
        &mut self,
        index: u32,
        target_offset: u32,
        kind: u32,
        requests: u32,
    ) -> Out<B, ()> {
        let record = self.resource_light_record(index)?;
        let target = self.read(record + target_offset)?;
        let sector = self.read(self.car + 0x70 + 0xac)?;
        if self.read(target + 0x120)? != sector {
            self.resource_virtual(target, 0x2c, &[sector, 0])?;
        }
        let source = self.resource_light_source(index, true)?;
        self.resource_light_world(source)?;
        let record = self.resource_light_record(index)?;
        let target = self.read(record + target_offset)?;
        if self.byte(target + 0xac)? & 0x80 == 0 {
            self.copy(source + 0x40, target + 0x80, 3)?;
            let flags = self.read(target + 0xac)?;
            self.put(target + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
        }
        let source = self.resource_light_source(index, false)?;
        self.resource_light_world(source)?;
        let mut direction = [0.; 3];
        for (i, value) in direction.iter_mut().enumerate() {
            *value = self.float(source + 0x30 + i as u32 * 4)?;
        }
        // The inlined branches and GetWorldDir share this normalization;
        // keep the native instruction order rather than reassociating sums.
        let order = if (kind == 2 && target_offset == 8)
            || (kind == 3 && target_offset == 12 && requests & 1 != 0)
        {
            [0, 1, 2]
        } else {
            [1, 2, 0]
        };
        let square = Self::add(
            Self::add(
                Self::multiply(direction[order[0]], direction[order[0]])?,
                Self::multiply(direction[order[1]], direction[order[1]])?,
            )?,
            Self::multiply(direction[order[2]], direction[order[2]])?,
        )?;
        if f64::from(Self::subtract(square, 1.)?.abs()) >= f64::from(1e-8_f32) {
            if square >= 1e-8_f32 {
                let root = square.sqrt();
                crate::native_precision::record_sqrt(square, root);
                let inverse = Self::divide(1., root)?;
                for value in &mut direction {
                    *value = Self::multiply(*value, inverse)?;
                }
            } else {
                let axis = if direction[0] != 0. {
                    0
                } else if direction[2] != 0. {
                    2
                } else {
                    1
                };
                direction[axis] = if direction[axis] < 0. { -1. } else { 1. };
            }
        }
        let record = self.resource_light_record(index)?;
        let frame = self.read(record + target_offset)?;
        self.effect(CarGameInitEffect::SetFrameDirection {
            frame,
            direction: direction.map(f32::to_bits),
        })?;
        let record = self.resource_light_record(index)?;
        let frame = self.read(record + target_offset)?;
        self.resource_virtual(frame, 0x24, &[1])?;
        let record = self.resource_light_record(index)?;
        let frame = self.read(record + target_offset)?;
        self.resource_virtual(frame, 0x18, &[])?;
        Ok(())
    }
}
