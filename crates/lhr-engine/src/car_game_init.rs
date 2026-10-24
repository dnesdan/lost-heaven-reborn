//! Original PC C_car::GameInit (RVA 0x1ed00). All effects see the canonical
//! car, heap and caller storage. An unavailable host effect leaves its reached
//! prefix in place; it is never reported as a completed GameInit.
use super::*;
#[path = "car_game_init_vehicle_resources.rs"]
mod vehicle_resources;
pub use vehicle_resources::{
    vehicle_game_init_resource_dependency, CarGameInitResourceBackend,
    VehicleGameInitResourceDependency, VehicleResourceEffect,
};
#[path = "car_game_done_vehicle.rs"]
mod vehicle_game_done;
pub use vehicle_game_done::{vehicle_end_skid, vehicle_game_done};
#[path = "car_game_done.rs"]
mod game_done;
pub use game_done::{actor_game_done, CarGameDoneBackend, CarGameDoneEffect};
#[path = "car_game_init_actor.rs"]
mod actor;
#[path = "car_game_init_vehicle.rs"]
mod vehicle_game_init;
pub use actor::actor_game_init;
#[path = "car_game_init_dependencies.rs"]
mod dependencies;
#[path = "car_game_init_levels.rs"]
mod levels;
pub use levels::{car_apply_level_source_parameters, car_convert_packed_vehicle_parameters};
#[path = "car_game_init_support.rs"]
mod support;
pub use dependencies::{
    using_object_add_object, vehicle_game_init_small_dependency, VehicleGameInitSmallDependency,
};
pub use support::{car_game_init_support, CarGameInitSupportBackend, CarGameInitSupportEffect};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CarGameInitScratch {
    pub memory: VehicleComponentTailScratch,
    /// Native ESP after SUB ESP,0x1c and the four saved registers.
    pub stack: u32,
    pub registers: CarInitRegisters,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarGameInitEffect {
    /// A separately reconstructed dependency, with its original call contract.
    CarDependency {
        entry: u32,
        car: u32,
        argument: u32,
    },
    VehicleDependency {
        entry: u32,
        vehicle: u32,
        arguments: [u32; 2],
    },
    InsertVehicleCollision {
        header: u32,
        insertion: u32,
        record: u32,
    },
    InsertVehicleCollisionPointer {
        header: u32,
        insertion: u32,
        source: u32,
    },
    InitializeSkidModels {
        vehicle: u32,
        position_pointer_slot: u32,
    },
    FrameVirtual {
        frame: u32,
        slot: u32,
        arguments: [u32; 4],
        count: u8,
    },
    AddSectorLight {
        sector: u32,
        light: u32,
    },
    DynamicCollision {
        entry: u32,
        manager: u32,
        object: u32,
    },
    FlagsCallback {
        function: u32,
        context: u32,
    },
    AddUsingObject {
        manager: u32,
        object: u32,
    },
    CreateShadowModel {
        driver: u32,
    },
    OpenShadowModel {
        model: u32,
    },
    FindShadowBox {
        model: u32,
    },
    LinkFrame {
        frame: u32,
        parent: u32,
        flags: u32,
    },
    SetFrameOn {
        frame: u32,
        enabled: bool,
    },
    /// Inherited SetDir with zero roll; direction words are caller values.
    SetFrameDirection {
        frame: u32,
        direction: [u32; 3],
    },
    MaterializeQuaternion {
        frame: u32,
    },
    NormalizeFrameQuaternion {
        frame: u32,
    },
    SetFrameRotationMatrix {
        frame: u32,
    },
    UpdateFrame {
        frame: u32,
    },
    UpdateWorldMatrix {
        frame: u32,
    },
    RemoveModelFrame {
        model: u32,
        frame: u32,
    },
    ReleaseFrame {
        frame: u32,
    },
    SetFrameSector {
        scene: u32,
        frame: u32,
        position: u32,
    },
}
pub trait CarGameInitBackend: VehicleInitBackend {
    fn game_init_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameInitEffect,
    ) -> Result<u32, Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CarGameInitUnavailable<E> {
    Storage,
    Effect(E),
    NumericDomain(&'static str),
    Dependency { entry: u32 },
}
type Out<B, T> = Result<T, CarGameInitUnavailable<<B as VehicleInitBackend>::Error>>;
struct GameInit<'a, B> {
    memory: VehicleComponentTailMemory<'a>,
    backend: &'a mut B,
    car: u32,
    stack: u32,
}
impl<B: CarGameInitBackend> GameInit<'_, B> {
    fn read(&mut self, address: u32) -> Out<B, u32> {
        self.memory
            .read_word_with_backend(self.backend, address)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn byte(&mut self, address: u32) -> Out<B, u8> {
        self.memory
            .read_byte_with_backend(self.backend, address)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn put(&mut self, address: u32, value: u32) -> Out<B, ()> {
        self.memory
            .write_word_with_backend(self.backend, address, value)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn put_byte(&mut self, address: u32, value: u8) -> Out<B, ()> {
        self.memory
            .write_byte_with_backend(self.backend, address, value)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn effect(&mut self, effect: CarGameInitEffect) -> Out<B, u32> {
        self.backend
            .game_init_effect(self.memory.reborrow(), effect)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn call(&mut self, ret: u32, args: &[u32]) -> Out<B, ()> {
        for (i, &word) in args.iter().rev().enumerate() {
            self.put(self.stack.wrapping_sub((i as u32 + 1) * 4), word)?;
        }
        self.put(self.stack.wrapping_sub((args.len() as u32 + 1) * 4), ret)
    }
    fn copy(&mut self, source: u32, target: u32, count: u32) -> Out<B, ()> {
        for i in 0..count {
            let word = self.read(source.wrapping_add(4 * i))?;
            self.put(target.wrapping_add(4 * i), word)?;
        }
        Ok(())
    }
    fn float(&mut self, address: u32) -> Out<B, f32> {
        let value = f32::from_bits(self.read(address)?);
        if value == 0. || value.is_normal() {
            Ok(value)
        } else {
            Err(CarGameInitUnavailable::NumericDomain("operand"))
        }
    }
    fn arithmetic(value: f32) -> Out<B, f32> {
        if value == 0. || value.is_normal() {
            Ok(value)
        } else {
            Err(CarGameInitUnavailable::NumericDomain("intermediate"))
        }
    }
    fn add(left: f32, right: f32) -> Out<B, f32> {
        crate::native_matrix::checked_f32_add(left, right)
            .map_err(|_| CarGameInitUnavailable::NumericDomain("addition"))
    }
    fn subtract(left: f32, right: f32) -> Out<B, f32> {
        Self::add(left, -right)
    }
    fn multiply(left: f32, right: f32) -> Out<B, f32> {
        crate::native_matrix::checked_f32_mul(left, right)
            .map_err(|_| CarGameInitUnavailable::NumericDomain("multiplication"))
    }
    fn divide(left: f32, right: f32) -> Out<B, f32> {
        let result = left / right;
        let exact = left as f64 / right as f64;
        if !exact.is_finite()
            || (exact != 0. && exact.abs() < f32::MIN_POSITIVE as f64)
            || exact.abs() > f32::MAX as f64
        {
            return Err(CarGameInitUnavailable::NumericDomain("division"));
        }
        let result = Self::arithmetic(result)?;
        crate::native_precision::record_div(left, right, result);
        Ok(result)
    }
    fn random(&mut self) -> Out<B, u32> {
        let seed = self
            .read(0x63a200)?
            .wrapping_mul(0x343fd)
            .wrapping_add(0x269ec3);
        self.put(0x63a200, seed)?;
        Ok((seed >> 16) & 0x7fff)
    }
    fn car_dependency(&mut self, entry: u32, argument: u32) -> Out<B, ()> {
        self.effect(CarGameInitEffect::CarDependency {
            entry,
            car: self.car,
            argument,
        })?;
        Ok(())
    }
    fn flags_callback(&mut self) -> Out<B, ()> {
        let function = self.read(self.car + 0x378)?;
        if function != 0 {
            let context = self.read(self.car + 0xce4)?;
            self.effect(CarGameInitEffect::FlagsCallback { function, context })?;
        }
        Ok(())
    }
    fn count(&mut self, begin_offset: u32, end_offset: u32, stride: u32) -> Out<B, u32> {
        let begin = self.read(self.car + begin_offset)?;
        if begin == 0 {
            return Ok(0);
        }
        let end = self.read(self.car + end_offset)?;
        let count = (end.wrapping_sub(begin) as i32) / stride as i32;
        if count < 0 {
            return Err(CarGameInitUnavailable::Storage);
        }
        Ok(count as u32)
    }
    fn initialize_using_objects(&mut self) -> Out<B, ()> {
        let mut index = 0u32;
        self.put(self.stack + 0x10, 0)?;
        loop {
            if index >= self.count(0x21cc, 0x21d0, 0xa0)? {
                break;
            }
            let offset = index.wrapping_mul(0xa0);
            let base = self.read(self.car + 0x21cc)?;
            for i in 0..14 {
                self.put(base + offset + 0x14 + i * 4, 0)?;
            }
            for (field, value) in [
                (0x20, 0x3f19999a),
                (0x24, self.car + 0xd58),
                (0x28, 0),
                (0x2c, 0),
                (0x30, self.car),
            ] {
                let base = self.read(self.car + 0x21cc)?;
                self.put(base + offset + field, value)?;
            }
            let base = self.read(self.car + 0x21cc)?;
            self.put_byte(base + offset + 0x3c, 1)?;
            self.put_byte(base + offset + 0x3d, 0)?;
            let base = self.read(self.car + 0x21cc)?;
            self.put(base + offset + 0x48, base + offset + 0x14)?;
            let base = self.read(self.car + 0x21cc)?;
            self.put(base + offset + 0x40, if index < 2 { 0x7d3 } else { 0x7d4 })?;
            let base = self.read(self.car + 0x21cc)?;
            if self.byte(base + offset + 0xc)? != 0 {
                let doors = self.read(self.car + 0xd14)?;
                self.copy(doors + index * 0x24, base + offset + 0x14, 3)?;
                let base = self.read(self.car + 0x21cc)?;
                let x = self.float(base + offset + 0x14)?;
                // GetVehicleSize(false), GetVehiclePivotPos(false) return
                // pointers to existing subobject vectors, established below.
                let vehicle = self.car + 0x70;
                let pivot = self.float(vehicle + 0x170)?;
                let x = if x > 0. {
                    let size = self.float(vehicle + 0x158)?;
                    let v = Self::subtract(size, pivot)?;
                    let v = Self::subtract(v, x)?;
                    let v = Self::subtract(v, f32::from_bits(0x3d4ccccd))?;
                    Self::add(v, x)?
                } else {
                    let v = Self::add(x, pivot)?;
                    let v = Self::add(v, f32::from_bits(0x3d4ccccd))?;
                    Self::subtract(x, v)?
                };
                self.put(base + offset + 0x14, x.to_bits())?;
                let base = self.read(self.car + 0x21cc)?;
                let y = Self::add(self.float(base + offset + 0x18)?, 0.5)?;
                self.put(base + offset + 0x18, y.to_bits())?;
                let mask = self.read(self.car + 0x21c0)? | 1u32.wrapping_shl(index);
                self.put(self.car + 0x21c0, mask)?;
                let base = self.read(self.car + 0x21cc)?;
                let object = base + offset + 0x14;
                let game = self.read(0x63788c)?;
                let mission = self.read(game + 0x24)?;
                self.call(0x41ef13, &[object])?;
                using_object_add_object(
                    self.backend,
                    self.memory.reborrow(),
                    mission + 0x29ec,
                    object,
                    self.stack - 4,
                )?;
                let base = self.read(self.car + 0x21cc)?;
                self.copy(base + offset + 0x14, base + offset + 0x4c, 14)?;
                let base = self.read(self.car + 0x21cc)?;
                self.put(base + offset + 0x78, 0x7df)?;
                let base = self.read(self.car + 0x21cc)?;
                self.put(base + offset + 0x80, base + offset + 0x4c)?;
                if index == 1 {
                    let base = self.read(self.car + 0x21cc)?;
                    let game = self.read(0x63788c)?;
                    let mission = self.read(game + 0x24)?;
                    self.call(0x41ef71, &[base + 0xec])?;
                    using_object_add_object(
                        self.backend,
                        self.memory.reborrow(),
                        mission + 0x29ec,
                        base + 0xec,
                        self.stack - 4,
                    )?;
                    index = index.wrapping_add(1);
                    self.put(self.stack + 0x10, index)?;
                    continue;
                }
            }
            if index > 1 {
                self.set_max_door_angle(index, f32::from_bits(0x3fc51eb8))?;
            }
            index = index.wrapping_add(1);
            self.put(self.stack + 0x10, index)?;
        }
        Ok(())
    }
    fn set_max_door_angle(&mut self, index: u32, angle: f32) -> Out<B, ()> {
        self.vehicle_door_helper(0x4cdac0, index, angle.to_bits(), 0)
    }
    fn vehicle_door_helper(
        &mut self,
        entry: u32,
        index: u32,
        value: u32,
        extra: u32,
    ) -> Out<B, ()> {
        let vehicle = self.car + 0x70;
        let base = self.read(vehicle + 0xca4)?;
        let door = self.read(
            base.wrapping_add(index.wrapping_mul(0x24))
                .wrapping_add(0x20),
        )?;
        if door == 0 {
            return Ok(());
        }
        match entry {
            0x4cdac0 => self.put(door + 0x28, value),
            0x4cdae0 => self.put(door + 0x2c, value),
            0x4cda20 => {
                let angle = f32::from_bits(value);
                let maximum = self.float(door + 0x28)?;
                let target = Self::multiply(angle, maximum)?;
                self.put(door + 0x24, target.to_bits())?;
                let base = self.read(vehicle + 0xca4)?;
                let door = self.read(base + index * 0x24 + 0x20)?;
                let flags = self.byte(door + 0x30)?;
                self.put_byte(door + 0x30, flags | 2)?;
                if extra as u8 != 0 {
                    let base = self.read(vehicle + 0xca4)?;
                    let door = self.read(base + index * 0x24 + 0x20)?;
                    let target = self.read(door + 0x24)?;
                    self.put(door + 0x20, target)?;
                    let sounds = self.byte(vehicle + 0x432)?;
                    self.put_byte(vehicle + 0x432, 0)?;
                    self.vehicle_dependency(0x4d7750, [0x358637bd, 0])?;
                    self.put_byte(vehicle + 0x432, sounds)?;
                    let base = self.read(vehicle + 0xca4)?;
                    let door = self.read(base + index * 0x24 + 0x20)?;
                    let frame = self.read(door)?;
                    self.effect(CarGameInitEffect::UpdateFrame { frame })?;
                    Ok(())
                } else {
                    self.flags_callback()
                }
            }
            _ => Err(CarGameInitUnavailable::Dependency { entry }),
        }
    }
    fn enable_sounds(&mut self, enabled: bool) -> Out<B, ()> {
        let vehicle = self.car + 0x70;
        if !enabled && self.byte(vehicle + 0x432)? != 0 {
            self.vehicle_dependency(0x4eec80, [0, 0])?;
        }
        self.put_byte(vehicle + 0x432, u8::from(enabled))
    }
    fn shadow(&mut self) -> Out<B, ()> {
        self.put(self.car + 0x21d8, 0)?;
        let driver = self.read(0x647ed8)?;
        self.call(0x41f1d1, &[driver, 9])?;
        let model = self.effect(CarGameInitEffect::CreateShadowModel { driver })?;
        if model == 0 {
            return Ok(());
        }
        self.call(0x41f1ef, &[model, 0x632388, 0, 0, 0, 0])?;
        let opened = self.effect(CarGameInitEffect::OpenShadowModel { model })?;
        if opened == 0 {
            self.call(0x41f204, &[model, 0x632380, 1])?;
            let frame = self.effect(CarGameInitEffect::FindShadowBox { model })?;
            self.put(self.car + 0x21d8, frame)?;
            if frame != 0 {
                let scene = self.read(0x63d9d4)?;
                let parent = self.read(scene + 0x210)?;
                self.effect(CarGameInitEffect::LinkFrame {
                    frame,
                    parent,
                    flags: 0,
                })?;
                let frame = self.read(self.car + 0x21d8)?;
                let refs = self.read(frame + 4)?.wrapping_add(1);
                self.put(frame + 4, refs)?;
                self.effect(CarGameInitEffect::SetFrameOn {
                    frame,
                    enabled: false,
                })?;
                let x = self.read(self.car + 0x1c8)?;
                let z = self.read(self.car + 0x1d0)?;
                for (i, v) in [x, 1f32.to_bits(), z].into_iter().enumerate() {
                    self.put(self.stack + 0x20 + i as u32 * 4, v)?;
                    self.put(frame + 0x90 + i as u32 * 4, v)?;
                }
                if self.byte(frame + 0xac)? & 8 == 0 {
                    self.effect(CarGameInitEffect::MaterializeQuaternion { frame })?;
                }
                let flags = (self.read(frame + 0xac)? & 0xfffffecb) | 0x40000008;
                self.put(frame + 0xac, flags)?;
                self.effect(CarGameInitEffect::UpdateFrame { frame })?;
                self.effect(CarGameInitEffect::RemoveModelFrame { model, frame })?;
            }
        }
        self.effect(CarGameInitEffect::ReleaseFrame { frame: model })?;
        Ok(())
    }
    fn run(&mut self) -> Out<B, ()> {
        self.put(self.stack + 0x14, self.car + 0x70)?;
        self.call(0x41ed1a, &[0])?;
        self.enable_sounds(false)?;
        self.call(0x41ed21, &[])?;
        // Actor::GameInit and all remaining dependencies retain their own
        // state/effect ordering rather than injecting a final object image.
        self.car_dependency(0x405fa0, 0)?;
        self.put(self.car + 0x21ac, u32::MAX)?;
        self.call(0x41ed32, &[])?;
        self.car_dependency(0x41e270, 0)?;
        for o in [0x207c, 0x2219, 0x2131] {
            self.put_byte(self.car + o, 0)?;
        }
        self.call(0x41ed49, &[])?;
        let random = self.random()?;
        self.put(self.car + 0x21c0, 0)?;
        self.put(self.stack + 0x10, (random as i32 % 200 - 100) as u32)?;
        let variation = Self::multiply(
            (random as i32 % 200 - 100) as f32,
            f32::from_bits(0x3c23d70a),
        )?;
        self.put(self.car + 0x2134, variation.to_bits())?;
        self.initialize_using_objects()?;
        self.put(self.car + 0x21c4, u32::MAX)?;
        self.call(0x41efcb, &[0])?;
        self.car_dependency(0x41ea00, 0)?;
        self.put(self.car + 0x20dc, 1f32.to_bits())?;
        self.put(self.car + 0x20d8, 1f32.to_bits())?;
        self.copy(self.car + 0x24, self.car + 0x390, 3)?;
        self.copy(self.car + 0x30, self.car + 0xcac, 3)?;
        let a = self.float(self.car + 0x5e4)?;
        let b = self.float(self.car + 0x4e8)?;
        let c = self.float(self.car + 0x664)?;
        let d = self.float(self.car + 0x620)?;
        let ab = Self::multiply(a, b)?;
        let cd = Self::multiply(c, d)?;
        let ratio = Self::divide(ab, cd)?;
        self.put(self.car + 0x20e0, ratio.to_bits())?;
        for o in 0x518..0x51c {
            self.put_byte(self.car + o, 0)?;
        }
        self.call(0x41f053, &[])?;
        self.vehicle_game_init()?;
        self.call(0x41f05f, &[1000f32.to_bits()])?;
        // SetFuel clamps to the original fuel capacity; see its dependency port.
        self.set_fuel(1000.)?;
        let flags = self.read(self.car + 0x18c)?;
        self.put(self.car + 0x18c, flags | 8)?;
        self.put(self.car + 0x18c, flags | 0x808)?;
        self.put(self.car + 0x18c, flags | 0x8808)?;
        self.put(self.car + 0x18c, (flags | 0x8808) & !0x80)?;
        self.flags_callback()?;
        for mask in [0x10, 0x20] {
            let flags = self.read(self.car + 0x18c)?;
            if flags & 0x80 == 0 {
                self.put(self.car + 0x18c, flags & !mask)?;
                self.flags_callback()?;
            }
        }
        for mask in [1, 2] {
            let flags = self.read(self.car + 0x18c)?;
            if flags & 8 == 0 {
                self.put(self.car + 0x18c, flags & !mask)?;
            }
        }
        for o in [0x20, 0x24, 0x28] {
            self.put(self.stack + o, 15f32.to_bits())?;
        }
        self.set_projector_size()?;
        let game = self.read(0x63788c)?;
        let mission = self.read(game + 0x24)?;
        let debug = self.byte(mission + 0x2d44)?;
        let flags = self.read(self.car + 0x18c)?;
        self.put(
            self.car + 0x18c,
            if debug == 0 {
                flags & 0xf00fffff
            } else {
                flags | 0x0ff00000
            },
        )?;
        self.flags_callback()?;
        let count = self.count(0x21cc, 0x21d0, 0xa0)?;
        for i in (0..count).rev() {
            let begin = self.read(self.car + 0x21cc)?;
            self.put(begin + i * 0xa0 + 0x10, 0)?;
            let begin = self.read(self.car + 0x21cc)?;
            self.put(begin + i * 0xa0 + 0x9c, 0)?;
        }
        self.shadow()?;
        for o in [0x2214, 0x2215, 0x2216, 0x2217, 0x220a, 0x220b, 0x5e, 0x20f0] {
            self.put_byte(self.car + o, 0)?;
        }
        self.put(self.car + 0x20e8, 0)?;
        self.put(self.car + 0x21dc, 0)?;
        let v = self.read(self.car + 0x2158)?;
        self.put(self.car + 0x2154, v)?;
        self.put_byte(self.car + 0x2218, 1)?;
        for o in [0x20f5, 0x20f4, 0x2170, 0x210d] {
            self.put_byte(self.car + o, 0)?;
        }
        self.call(0x41f35b, &[1])?;
        self.car_dependency(0x429bb0, 1)?;
        let begin = self.read(self.car + 0x21f8)?;
        self.put(self.car + 0x21fc, begin)?;
        let flags = self.read(self.car + 4)?;
        self.put_byte(self.car + 0x2204, ((flags >> 1) & 1) as u8)?;
        self.call(0x41f39c, &[0])?;
        self.car_dependency(0x423340, 0)?;
        self.call(0x41f3a7, &[])?;
        let frame = self.read(self.car + 0x3bc)?;
        self.effect(CarGameInitEffect::CarDependency {
            entry: 0x407cd0,
            car: frame,
            argument: 0,
        })?;
        let frame = self.read(self.car + 0x3bc)?;
        self.effect(CarGameInitEffect::UpdateFrame { frame })?;
        let frame = self.read(self.car + 0x3bc)?;
        if self.byte(frame + 0xac)? & 0x20 == 0 {
            self.effect(CarGameInitEffect::UpdateWorldMatrix { frame })?;
        }
        let x = self.read(frame + 0x40)?;
        let y = Self::add(self.float(frame + 0x44)?, 1.)?;
        let z = self.read(frame + 0x48)?;
        for (i, v) in [x, y.to_bits(), z].into_iter().enumerate() {
            self.put(self.stack + 0x20 + i as u32 * 4, v)?;
        }
        let scene = self.read(0x63d9d4)?;
        let frame = self.read(self.car + 0x3bc)?;
        self.effect(CarGameInitEffect::SetFrameSector {
            scene,
            frame,
            position: self.stack + 0x20,
        })?;
        self.finish()
    }
    fn set_fuel(&mut self, fuel: f32) -> Out<B, ()> {
        let capacity = self.float(self.car + 0x70 + 0x1fa0)?;
        self.put(
            self.car + 0x70 + 0xc30,
            if fuel > capacity { capacity } else { fuel }.to_bits(),
        )
    }
    fn set_projector_size(&mut self) -> Out<B, ()> {
        let vehicle = self.car + 0x70;
        for kinds in [
            &[1u16, 2][..],
            &[5u16, 6][..],
            &[3u16, 4][..],
            &[7u16][..],
            &[8u16][..],
            &[9u16][..],
        ] {
            let begin = self.read(vehicle + 0x100)?;
            if begin == 0 {
                continue;
            }
            let end = self.read(vehicle + 0x104)?;
            let count = (end.wrapping_sub(begin) as i32) >> 5;
            if count < 0 {
                return Err(CarGameInitUnavailable::Storage);
            }
            for i in (0..count as u32).rev() {
                let begin = self.read(vehicle + 0x100)?;
                let record = begin + i * 0x20;
                let kind =
                    u16::from_le_bytes([self.byte(record + 0x10)?, self.byte(record + 0x11)?]);
                if !kinds.contains(&kind) {
                    continue;
                }
                let frame = self.read(record + 8)?;
                if frame == 0 {
                    continue;
                }
                self.copy(self.stack + 0x20, frame + 0x90, 3)?;
                if self.byte(frame + 0xac)? & 8 == 0 {
                    self.effect(CarGameInitEffect::MaterializeQuaternion { frame })?;
                }
                let flags = (self.read(frame + 0xac)? & 0xfffffecb) | 0x40000008;
                self.put(frame + 0xac, flags)?;
            }
        }
        Ok(())
    }
    fn finish(&mut self) -> Out<B, ()> {
        let v = self.read(self.car + 0x20bc)?;
        self.put(self.car + 0x20c4, v)?;
        self.put_byte(self.car + 0x20f6, 0)?;
        self.copy(self.car + 0x24, self.car + 0x2120, 3)?;
        for o in [0x2104, 0x2105, 0x20f1, 0x221a, 0x210c, 0x2208, 0x217c] {
            self.put_byte(self.car + o, 0)?;
        }
        for o in [0x21e0, 0x21e4, 0x2108, 0x20f8, 0x20fc, 0x20cc] {
            self.put(self.car + o, 0)?;
        }
        self.put(self.car + 0x2100, 1000f32.to_bits())?;
        self.put(self.car + 0x2174, 1f32.to_bits())?;
        self.put(self.car + 0x2178, 3)?;
        let count = self.count(0xd14, 0xd18, 0x24)?;
        for i in (0..count).rev() {
            self.call(0x41f4ce, &[i, 0, 1])?;
            self.vehicle_door_helper(0x4cda20, i, 0, 1)?;
            self.call(0x41f4db, &[i, 2f32.to_bits()])?;
            self.vehicle_door_helper(0x4cdae0, i, 2f32.to_bits(), 0)?;
        }
        for (o, v) in [(0x20, 0), (0x24, 0xc95bba00), (0x28, 0)] {
            self.put(self.stack + o, v)?;
        }
        for (o, v) in [
            (0x2160, 0),
            (0x2164, 0xc95bba00),
            (0x2168, 0),
            (0x20d0, 0),
            (0x20ec, 0),
            (0x212c, 0),
            (0x2110, 0),
            (0x2114, 0),
            (0x216c, 1000),
            (0x2210, 0),
        ] {
            self.put(self.car + o, v)?;
        }
        for o in [0x20b0, 0x2106, 0x20f2, 0x2118, 0x217e, 0x20b3, 0x20d4] {
            self.put_byte(self.car + o, 0)?;
        }
        self.put_byte(0x63d6c4, 0)?;
        self.call(0x41f57e, &[])?;
        let random = self.random()?;
        self.put(self.stack + 0x1c, 0)?;
        for o in [0x20f3, 0x215c, 0x2119, 0x221b] {
            self.put_byte(self.car + o, 0)?;
        }
        self.put(self.car + 0x20e4, 0)?;
        self.put_byte(self.car + 0x20b1, 1)?;
        let delay = random.wrapping_mul(150).wrapping_add(0x3d478);
        self.put(self.stack + 0x1c, delay)?;
        // Native FILD qword consumes the adjoining +0x20 word, which was
        // explicitly cleared at 41f4e1. Preserve this dependency on live memory.
        let low = self.read(self.stack + 0x1c)?;
        let high = self.read(self.stack + 0x20)?;
        let integer = (((high as u64) << 32) | low as u64) as i64;
        let magnitude = integer.unsigned_abs();
        let significant_bits = 64 - magnitude.leading_zeros();
        if significant_bits > 24 && magnitude.trailing_zeros() < significant_bits - 24 {
            crate::native_precision::record_precision(true);
        }
        let delay = integer as f32;
        self.put(self.car + 0x2014, delay.to_bits())?;
        self.call(0x41f5d2, &[1])?;
        self.enable_sounds(true)
    }
}
impl VehicleInitState {
    fn validate_car_game_call_scratch<E>(
        &self,
        scratch: &CarGameInitScratch,
    ) -> Result<u32, CarGameInitUnavailable<E>> {
        let Some(car) = self.car.as_ref() else {
            return Err(CarGameInitUnavailable::Storage);
        };
        let address = car.address;
        let end = scratch
            .memory
            .address
            .checked_add(VEHICLE_INIT_STACK_BYTES as u32)
            .ok_or(CarGameInitUnavailable::Storage)?;
        let active_end = scratch
            .stack
            .checked_add(0x30)
            .ok_or(CarGameInitUnavailable::Storage)?;
        if scratch.stack < scratch.memory.address.saturating_add(0x120) || active_end > end {
            return Err(CarGameInitUnavailable::Storage);
        }
        let car_end = address
            .checked_add(0x70 + self.bytes.len() as u32)
            .ok_or(CarGameInitUnavailable::Storage)?;
        let init_end = car
            .scratch
            .address
            .checked_add(CAR_INIT_STACK_BYTES as u32)
            .ok_or(CarGameInitUnavailable::Storage)?;
        if address < end && scratch.memory.address < car_end
            || car.scratch.address < end && scratch.memory.address < init_end
        {
            return Err(CarGameInitUnavailable::Storage);
        }
        Ok(address)
    }
    pub fn car_game_init<B: CarGameInitBackend>(
        &mut self,
        backend: &mut B,
        scratch: &mut CarGameInitScratch,
    ) -> Out<B, ()> {
        let address = self.validate_car_game_call_scratch(scratch)?;
        let stack = scratch.stack;
        let registers = scratch.registers;
        let memory = self
            .car_live_memory(&mut scratch.memory)
            .ok_or(CarGameInitUnavailable::Storage)?;
        let mut call = GameInit {
            memory,
            backend,
            car: address,
            stack,
        };
        for (o, v) in [
            (0, registers.edi),
            (4, registers.esi),
            (8, registers.ebp),
            (12, registers.ebx),
        ] {
            call.put(stack + o, v)?;
        }
        call.run()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    struct Host {
        external: BTreeMap<u32, u8>,
        effects: Vec<CarGameInitEffect>,
    }
    impl VehicleInitBackend for Host {
        type Error = &'static str;
        fn read_byte(&mut self, address: u32) -> Result<u8, Self::Error> {
            self.external.get(&address).copied().ok_or("unmapped")
        }
        fn read_word(&mut self, address: u32) -> Result<u32, Self::Error> {
            Ok(u32::from_le_bytes([
                self.read_byte(address)?,
                self.read_byte(address + 1)?,
                self.read_byte(address + 2)?,
                self.read_byte(address + 3)?,
            ]))
        }
        fn write_byte(&mut self, address: u32, value: u8) -> Result<(), Self::Error> {
            self.external.insert(address, value);
            Ok(())
        }
        fn write_word(&mut self, address: u32, value: u32) -> Result<(), Self::Error> {
            for (i, byte) in value.to_le_bytes().into_iter().enumerate() {
                self.write_byte(address + i as u32, byte)?;
            }
            Ok(())
        }
        fn free(&mut self, _: u32) -> Result<(), Self::Error> {
            Err("unexpected free")
        }
        fn allocate(&mut self, _: u32) -> Result<u32, Self::Error> {
            Err("unexpected allocation")
        }
        fn collect_frame(
            &mut self,
            _: &mut VehicleInitFrameVector,
            _: u32,
        ) -> Result<(), Self::Error> {
            Err("unexpected collector")
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
            Err(VehicleInitUnavailable::Effect("unexpected enumeration"))
        }
    }
    impl CarGameInitBackend for Host {
        fn game_init_effect(
            &mut self,
            mut memory: VehicleComponentTailMemory<'_>,
            effect: CarGameInitEffect,
        ) -> Result<u32, Self::Error> {
            self.effects.push(effect);
            match effect {
                CarGameInitEffect::CarDependency {
                    entry: 0x405fa0,
                    car,
                    ..
                } => {
                    memory.write_word(car + 0x24, 111f32.to_bits()).unwrap();
                }
                CarGameInitEffect::CarDependency {
                    entry: 0x41e270,
                    car,
                    ..
                } => {
                    for (offset, value) in [(0x5e4, 3f32), (0x4e8, 4.), (0x664, 2.), (0x620, 3.)] {
                        memory.write_word(car + offset, value.to_bits()).unwrap();
                    }
                }
                CarGameInitEffect::CarDependency {
                    entry: 0x41ea00, ..
                } => {}
                CarGameInitEffect::UpdateFrame { .. } => {
                    assert_eq!(memory.read_word(0x1000 + 0x390), Some(111f32.to_bits()));
                    memory.write_byte(0x1000 + 0x64, 7).unwrap();
                    return Err("frame boundary");
                }
                _ => return Err("unexpected effect"),
            }
            Ok(0)
        }
    }
    fn fixture() -> (VehicleInitState, Host, CarGameInitScratch) {
        let mut bytes = vec![0; 0x221c];
        bytes[0x3bc..0x3c0].copy_from_slice(&0x6000_u32.to_le_bytes());
        let state = VehicleInitState::from_car_bytes(
            0x1000,
            bytes,
            CarInitScratch {
                address: 0x200000,
                bytes: [0; CAR_INIT_STACK_BYTES],
            },
            CarInitRegisters {
                ebx: 0,
                ebp: 0,
                esi: 0,
                edi: 0,
            },
        )
        .unwrap();
        let mut host = Host {
            external: BTreeMap::new(),
            effects: vec![],
        };
        host.write_word(0x63a200, 1).unwrap();
        let scratch = CarGameInitScratch {
            memory: VehicleComponentTailScratch {
                address: 0x100000,
                bytes: [0xa5; VEHICLE_INIT_STACK_BYTES],
            },
            stack: 0x100140,
            registers: CarInitRegisters {
                ebx: 1,
                ebp: 2,
                esi: 3,
                edi: 4,
            },
        };
        (state, host, scratch)
    }
    #[test]
    fn live_dependency_writes_flow_into_physics_prefix_and_survive_host_failure() {
        let (mut state, mut host, mut scratch) = fixture();
        assert_eq!(
            state.car_game_init(&mut host, &mut scratch),
            Err(CarGameInitUnavailable::Effect("frame boundary"))
        );
        let bytes = state.car_bytes().unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[0x20e0..0x20e4].try_into().unwrap()),
            2f32.to_bits()
        );
        assert_eq!(bytes[0x64], 7);
        assert_eq!(
            host.read_word(0x63a200).unwrap(),
            1_u32.wrapping_mul(0x343fd).wrapping_add(0x269ec3)
        );
        assert_eq!(host.effects.len(), 4);
    }
    #[test]
    fn invalid_nested_stack_span_stops_before_any_original_store() {
        let (mut state, mut host, mut scratch) = fixture();
        scratch.stack = scratch.memory.address + 0x100;
        let before = state.car_bytes();
        let stack_before = scratch.memory.clone();
        assert_eq!(
            state.car_game_init(&mut host, &mut scratch),
            Err(CarGameInitUnavailable::Storage)
        );
        assert_eq!(state.car_bytes(), before);
        assert_eq!(scratch.memory, stack_before);
        assert!(host.effects.is_empty());
    }
}
