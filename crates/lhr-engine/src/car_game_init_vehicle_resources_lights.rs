//! The original DoLights scalar body and extinguished-light branches needed
//! by GameDone. Illuminated projector branches remain explicitly unavailable.
use super::*;
impl<B: CarGameInitResourceBackend> GameInit<'_, B> {
    pub(in super::super) fn resource_do_lights(&mut self, seconds: u32) -> Out<B, ()> {
        let v = self.car + 0x70;
        if self.byte(v + 0xc68)? & 0x80 == 0 && self.byte(v + 0xc6c)? & 0x80 == 0 {
            return Ok(());
        }
        let dt = f32::from_bits(seconds);
        let timer = Self::add(dt, self.float(v + 0x138)?)?;
        self.put(v + 0x138, timer.to_bits())?;
        let flags = self.read(v + 0x11c)?;
        if flags & 0x80 != 0 {
            if timer > self.float(v + 0x134)? {
                self.put(v + 0x138, 0)?;
                self.put(v + 0x11c, flags ^ 0x2000_0000)?;
            }
            let flags = self.read(v + 0x11c)?;
            let flags = if flags & 0x2000_0000 == 0 {
                (flags & !0x10) | 0x20
            } else {
                (flags & !0x20) | 0x10
            };
            self.put(v + 0x11c, flags)?;
        } else if self.byte(v + 0x120)? & 0x80 != 0 {
            self.put(v + 0x11c, flags & !0x30)?;
        }
        let timer = Self::add(dt, self.float(v + 0x12c)?)?;
        self.put(v + 0x12c, timer.to_bits())?;
        let flags = self.read(v + 0x11c)?;
        let threshold = self.float(
            v + if flags & 0x1000_0000 == 0 {
                0x128
            } else {
                0x124
            },
        )?;
        if timer >= threshold {
            self.put(v + 0x12c, 0)?;
            self.put(v + 0x11c, flags ^ 0x1000_0000)?;
        }
        let flags = self.read(v + 0x11c)?;
        if flags & 0x8000 != 0 {
            let reverse = (self.read(v + 0x55c)? as i32) < 0;
            self.put(
                v + 0x11c,
                if reverse {
                    flags | 0x1000
                } else {
                    flags & !0x1000
                },
            )?;
        }
        let flags = self.read(v + 0x11c)?;
        if flags & 0x800 != 0 {
            let braking = self.float(v + 0x5b8)? > 0.;
            self.put(
                v + 0x11c,
                if braking {
                    flags | 0x100
                } else {
                    flags & !0x100
                },
            )?;
        }
        let flags = self.read(v + 0x11c)?;
        if flags & 8 != 0 {
            let steering = self.float(v + 0x628)?;
            let threshold = self.float(v + 0x130)?;
            let flags = if steering.abs() <= threshold {
                flags & !3
            } else if steering <= 0. {
                (flags & !2) | 1
            } else {
                (flags & !1) | 2
            };
            self.put(v + 0x11c, flags)?;
        }
        let count = self.resource_count(v + 0x110, v + 0x114, 0x1c)?;
        for i in (0..count).rev() {
            let begin = self.read(v + 0x110)?;
            let address = begin + i * 0x1c + 0x18;
            let flags = self.byte(address)?;
            self.put_byte(address, flags & 0x7f)?;
        }
        let count = self.resource_count(v + 0x100, v + 0x104, 0x20)?;
        for i in (0..count).rev() {
            let record = self.resource_light_record(i)?;
            let kind = self.read(record + 0x10)? & 0xffff;
            if (1..=9).contains(&kind) {
                // GameDone clears the request word. Its timer may set only
                // the blink phase. A callback can change the word, so reload
                // it before each record and reject the unported branch.
                if self.read(v + 0x11c)? & !0x1000_0000 != 0 {
                    let entries = [
                        0x4d7f05, 0x4d88fa, 0x4d92b9, 0x4da5e3, 0x4dad6f, 0x4db131, 0x4db4f3,
                        0x4db914, 0x4dbcd8,
                    ];
                    return Err(CarGameInitUnavailable::Dependency {
                        entry: entries[kind as usize - 1],
                    });
                }
                let frame = self.read(record + 4)?;
                if frame != 0 {
                    self.resource_virtual(frame, 0x24, &[0])?;
                }
                let record = self.resource_light_record(i)?;
                let light = self.read(record + 8)?;
                if light != 0 {
                    let flags = self.read(record + 0x14)?;
                    let mut shared_on = false;
                    if (flags as i32) < 0 {
                        let index = self.read(record + 0x1c)? & 0xffff;
                        let begin = self.read(v + 0x110)?;
                        shared_on = self.byte(begin + index * 0x1c + 0x18)? & 0x80 != 0;
                    }
                    if !shared_on {
                        let record = self.resource_light_record(i)?;
                        let light = self.read(record + 8)?;
                        self.resource_virtual(light, 0x24, &[0])?;
                        let record = self.resource_light_record(i)?;
                        let light = self.read(record + 0xc)?;
                        self.resource_virtual(light, 0x24, &[0])?;
                    }
                }
            }
            let record = self.resource_light_record(i)?;
            let frame = self.read(record + 4)?;
            if frame != 0 {
                self.resource_virtual(frame, 0x18, &[])?;
            }
        }
        let flags = self.read(v + 0x11c)?;
        self.put(v + 0x120, flags)
    }
    fn resource_light_record(&mut self, index: u32) -> Out<B, u32> {
        let begin = self.read(self.car + 0x70 + 0x100)?;
        Ok(begin + index * 0x20)
    }
}
