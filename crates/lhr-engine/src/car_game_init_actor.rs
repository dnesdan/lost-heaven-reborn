//! Base C_actor::GameInit (PC RVA0x5fa0), sharing the caller's live graph.
use super::*;

/// Original base-class call usable from the GameInit host dispatcher. The
/// scratch address identifies caller storage; this routine does not clone the
/// actor or frame graph. Frame ownership/allocator effects remain explicit.
pub fn actor_game_init<B: CarGameInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    actor: u32,
    stack: u32,
) -> Out<B, ()> {
    GameInit {
        backend,
        memory,
        car: actor,
        stack,
    }
    .actor_game_init_body()
}
impl<B: CarGameInitBackend> GameInit<'_, B> {
    fn actor_link(&mut self, frame: u32, parent: u32) -> Out<B, ()> {
        self.effect(CarGameInitEffect::LinkFrame {
            frame,
            parent,
            flags: 1,
        })?;
        Ok(())
    }
    fn actor_world(&mut self, frame: u32) -> Out<B, ()> {
        if self.byte(frame + 0xac)? & 0x20 == 0 {
            self.effect(CarGameInitEffect::UpdateWorldMatrix { frame })?;
        }
        Ok(())
    }
    fn actor_game_init_body(&mut self) -> Out<B, ()> {
        self.put(self.car + 0x1c, 0)?;
        let kind = self.read(self.car + 0x10)?;
        self.put_byte(self.car + 0x65, u8::from(kind == 1))?;
        let frame = self.read(self.car + 0x68)?;
        if frame != 0 {
            if kind == 4 {
                let parent = self.read(frame + 0x120)?;
                if parent != 0 && self.read(parent + 0x110)? != 5 {
                    let game = self.read(0x63788c)?;
                    let scene = self.read(game + 0x10)?;
                    let sector = self.read(scene + 0x210)?;
                    self.actor_link(frame, sector)?;
                    let frame = self.read(self.car + 0x68)?;
                    self.actor_world(frame)?;
                    let x = self.read(frame + 0x40)?;
                    let y = Self::add(self.float(frame + 0x44)?, 1.)?;
                    let z = self.read(frame + 0x48)?;
                    for (i, value) in [x, y.to_bits(), z].into_iter().enumerate() {
                        self.put(self.stack + 0x10 + i as u32 * 4, value)?;
                    }
                    let game = self.read(0x63788c)?;
                    let scene = self.read(game + 0x10)?;
                    let frame = self.read(self.car + 0x68)?;
                    self.effect(CarGameInitEffect::SetFrameSector {
                        scene,
                        frame,
                        position: self.stack + 0x10,
                    })?;
                }
            }
            let frame = self.read(self.car + 0x68)?;
            let record = self.read(frame + 8)?;
            if record != 0 && self.read(record)? != 0 {
                self.put_byte(self.car + 0x64, 0)?;
            } else {
                self.put_byte(self.car + 0x64, 1)?;
                let mut record = self.read(frame + 8)?;
                if record == 0 {
                    record = self
                        .backend
                        .allocate_projected(&mut self.memory, 4)
                        .map_err(CarGameInitUnavailable::Effect)?;
                    self.put(frame + 8, record)?;
                }
                self.put(record, self.car)?;
                let frame = self.read(self.car + 0x68)?;
                let old = self.read(frame + 8)?;
                if old != record {
                    self.backend
                        .free_projected(&mut self.memory, old)
                        .map_err(CarGameInitUnavailable::Effect)?;
                }
                if self.read(record)? == 0 {
                    self.backend
                        .free_projected(&mut self.memory, record)
                        .map_err(CarGameInitUnavailable::Effect)?;
                    record = 0;
                }
                self.put(frame + 8, record)?;
            }
            let frame = self.read(self.car + 0x68)?;
            let flags = self.read(frame + 0xac)?;
            self.put_byte(self.car + 0x5c, (flags & 1) as u8)?;
            self.copy(frame + 0x80, self.car + 0x3c, 3)?;
            let flags = self.read(frame + 0xac)?;
            if flags & 8 == 0 {
                self.put(frame + 0xac, flags | 8)?;
                self.effect(CarGameInitEffect::MaterializeQuaternion { frame })?;
            }
            self.copy(frame + 0x9c, self.car + 0x48, 4)?;
            let frame = self.read(self.car + 0x68)?;
            let kind = self.read(self.car + 0x10)?;
            let parent = self.read(frame + 0x120)?;
            self.put(self.car + 0x58, parent)?;
            if kind != 1 && kind != 0x21 && parent != 0 && self.read(parent + 0x110)? == 5 {
                let game = self.read(0x63788c)?;
                let scene = self.read(game + 0x10)?;
                self.actor_world(frame)?;
                self.effect(CarGameInitEffect::SetFrameSector {
                    scene,
                    frame,
                    position: frame + 0x40,
                })?;
            }
            let kind = self.read(self.car + 0x10)?;
            if kind != 1 && kind != 0x21 {
                let frame = self.read(self.car + 0x68)?;
                if self.read(frame + 0x110)? == 9 {
                    let parent = self.read(frame + 0x120)?;
                    let mut ancestor = parent;
                    while ancestor != 0 && self.read(ancestor + 0x110)? != 5 {
                        ancestor = self.read(ancestor + 0x120)?;
                    }
                    if ancestor != 0 && ancestor != parent {
                        self.actor_link(frame, ancestor)?;
                        let frame = self.read(self.car + 0x68)?;
                        self.effect(CarGameInitEffect::UpdateFrame { frame })?;
                    }
                }
            }
            let frame = self.read(self.car + 0x68)?;
            self.copy(frame + 0x80, self.car + 0x24, 3)?;
            let flags = self.read(frame + 0xac)?;
            if flags & 0x10 == 0 {
                self.put(frame + 0xac, flags | 0x10)?;
                self.effect(CarGameInitEffect::NormalizeFrameQuaternion { frame })?;
                self.effect(CarGameInitEffect::SetFrameRotationMatrix { frame })?;
                self.copy(frame + 0x70, self.car + 0x30, 3)?;
            } else {
                let mut direction = [
                    self.float(frame + 0x70)?,
                    self.float(frame + 0x74)?,
                    self.float(frame + 0x78)?,
                ];
                let [x, y, z] = direction;
                let squared = Self::add(
                    Self::add(Self::multiply(z, z)?, Self::multiply(x, x)?)?,
                    Self::multiply(y, y)?,
                )?;
                if (crate::native_precision::sub(squared, 1.) as f64).abs() >= 1e-8 {
                    if squared < 1e-8_f32 {
                        let axis = if x != 0. {
                            0
                        } else if z != 0. {
                            2
                        } else {
                            1
                        };
                        direction[axis] = if direction[axis] < 0. { -1. } else { 1. };
                    } else {
                        let reciprocal = Self::divide(1., crate::native_precision::sqrt(squared))?;
                        for value in &mut direction {
                            *value = Self::multiply(*value, reciprocal)?;
                        }
                    }
                }
                for (i, value) in direction.into_iter().enumerate() {
                    self.put(self.car + 0x30 + i as u32 * 4, value.to_bits())?;
                }
            }
        } else {
            for (offset, value) in [
                (0x24, 0),
                (0x28, 0),
                (0x2c, 0),
                (0x30, 0),
                (0x34, 0),
                (0x38, 0x3f800000),
            ] {
                self.put(self.car + offset, value)?;
            }
        }
        self.put_byte(self.car + 0x5e, 0)?;
        self.put_byte(self.car + 0x5d, 1)?;
        self.put_byte(self.car + 0x5f, 0)?;
        if self.read(self.car + 0x10)? == 1 {
            let frame = self.read(self.car + 0x68)?;
            if frame != 0 {
                self.effect(CarGameInitEffect::CarDependency {
                    entry: 0x407cd0,
                    car: frame,
                    argument: 0,
                })?;
                let frame = self.read(self.car + 0x68)?;
                self.effect(CarGameInitEffect::CarDependency {
                    entry: 0x407d10,
                    car: frame,
                    argument: 0,
                })?;
            }
        }
        Ok(())
    }
}
