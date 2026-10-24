//! Original C_car::GameDone (PC RVA1f5e0) and C_actor::GameDone (RVA63e0).
//! They share the canonical owner used by GameInit. Required Vehicle, using
//! object, shadow and indicator effects remain explicit; Drop performs none.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CarGameDoneEffect {
    VehicleGameDone {
        vehicle: u32,
    },
    RemoveUsingObject {
        manager: u32,
        object: u32,
    },
    ShadowCarOn {
        car: u32,
        enabled: bool,
    },
    RadarRemoveCar {
        manager: u32,
        car: u32,
    },
    /// PC tail target; its correspondence to the PS2 traffic method does not
    /// establish a PC implicit-this argument.
    TrafficCleanup {
        entry: u32,
    },
}
pub trait CarGameDoneBackend: CarGameInitBackend {
    fn game_done_effect(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        effect: CarGameDoneEffect,
    ) -> Result<(), Self::Error>;
}

/// The base actor restores its saved pose and frees only its owned userdata
/// record. Frame pointers are non-owning and are reloaded after each effect.
pub fn actor_game_done<B: CarGameInitBackend>(
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
    .actor_game_done_body()
}
impl<B: CarGameInitBackend> GameInit<'_, B> {
    fn actor_game_done_body(&mut self) -> Out<B, ()> {
        let frame = self.read(self.car + 0x68)?;
        if frame == 0 {
            return Ok(());
        }
        self.effect(CarGameInitEffect::SetFrameOn {
            frame,
            enabled: true,
        })?;
        let frame = self.read(self.car + 0x68)?;
        let parent = self.read(self.car + 0x58)?;
        self.effect(CarGameInitEffect::LinkFrame {
            frame,
            parent,
            flags: 0,
        })?;
        let frame = self.read(self.car + 0x68)?;
        if self.byte(frame + 0xac)? & 0x80 == 0 {
            self.copy(self.car + 0x3c, frame + 0x80, 3)?;
            let flags = self.read(frame + 0xac)?;
            self.put(frame + 0xac, (flags & 0xffff_fedf) | 0x4000_0000)?;
        }
        let frame = self.read(self.car + 0x68)?;
        self.copy(self.car + 0x48, frame + 0x9c, 4)?;
        self.effect(CarGameInitEffect::NormalizeFrameQuaternion { frame })?;
        let flags = self.read(frame + 0xac)?;
        self.put(frame + 0xac, (flags & 0xffff_fecb) | 0x4000_0008)?;
        let current = self.read(self.car + 0x68)?;
        self.effect(CarGameInitEffect::FrameVirtual {
            frame: current,
            slot: 0x18,
            arguments: [0; 4],
            count: 0,
        })?;
        let current = self.read(self.car + 0x68)?;
        let enabled = self.byte(self.car + 0x5c)? != 0;
        self.effect(CarGameInitEffect::SetFrameOn {
            frame: current,
            enabled,
        })?;
        if self.byte(self.car + 0x64)? == 0 {
            return Ok(());
        }
        let current = self.read(self.car + 0x68)?;
        let mut record = self.read(current + 8)?;
        if record == 0 || self.read(record)? == 0 {
            return Ok(());
        }
        self.put(record, 0)?;
        let current = self.read(self.car + 0x68)?;
        let old = self.read(current + 8)?;
        let mut free_record = old == record;
        if !free_record {
            self.backend
                .free_projected(&mut self.memory, old)
                .map_err(CarGameInitUnavailable::Effect)?;
            free_record = self.read(record)? == 0;
        }
        if free_record {
            self.backend
                .free_projected(&mut self.memory, record)
                .map_err(CarGameInitUnavailable::Effect)?;
            record = 0;
        }
        self.put(current + 8, record)
    }
}
impl<B: CarGameDoneBackend> GameInit<'_, B> {
    fn done_effect(&mut self, effect: CarGameDoneEffect) -> Out<B, ()> {
        self.backend
            .game_done_effect(self.memory.reborrow(), effect)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn remove_using_object(&mut self, object: u32) -> Out<B, ()> {
        let game = self.read(0x63788c)?;
        let mission = self.read(game + 0x24)?;
        self.done_effect(CarGameDoneEffect::RemoveUsingObject {
            manager: mission + 0x29ec,
            object,
        })
    }
    fn car_game_done_body(&mut self) -> Out<B, ()> {
        self.enable_sounds(false)?;
        self.done_effect(CarGameDoneEffect::VehicleGameDone {
            vehicle: self.car + 0x70,
        })?;
        self.actor_game_done_body()?;
        let count = self.count(0x21cc, 0x21d0, 0xa0)?;
        for i in (0..count).rev() {
            let begin = self.read(self.car + 0x21cc)?;
            if self.byte(begin + i * 0xa0 + 0xc)? != 0 {
                self.remove_using_object(begin + i * 0xa0 + 0x14)?;
                if i == 1 {
                    let begin = self.read(self.car + 0x21cc)?;
                    self.remove_using_object(begin + 0xec)?;
                }
            }
        }
        self.put(self.car + 0x21c0, 0)?;
        let begin = self.read(self.car + 0x21b4)?;
        self.put(self.car + 0x21b8, begin)?;
        self.done_effect(CarGameDoneEffect::ShadowCarOn {
            car: self.car,
            enabled: false,
        })?;
        let shadow = self.read(self.car + 0x21d8)?;
        if shadow != 0 {
            self.effect(CarGameInitEffect::LinkFrame {
                frame: shadow,
                parent: 0,
                flags: 0,
            })?;
            let shadow = self.read(self.car + 0x21d8)?;
            self.effect(CarGameInitEffect::ReleaseFrame { frame: shadow })?;
            self.put(self.car + 0x21d8, 0)?;
        }
        let count = self.count(0xd14, 0xd18, 0x24)?;
        for i in (0..count).rev() {
            self.vehicle_door_helper(0x4cda20, i, 0, 1)?;
        }
        if self.byte(self.car + 0x220c)? != 0 {
            let model = self.read(self.car + 0x3bc)?;
            let record = self.read(model + 8)?;
            self.backend
                .free_projected(&mut self.memory, record)
                .map_err(CarGameInitUnavailable::Effect)?;
            self.put(model + 8, 0)?;
            let model = self.read(self.car + 0x3bc)?;
            self.effect(CarGameInitEffect::LinkFrame {
                frame: model,
                parent: 0,
                flags: 0,
            })?;
            let model = self.read(self.car + 0x3bc)?;
            self.effect(CarGameInitEffect::ReleaseFrame { frame: model })?;
            self.put(self.car + 0x3bc, 0)?;
            self.put(self.car + 0x68, 0)?;
        }
        self.done_effect(CarGameDoneEffect::RadarRemoveCar {
            manager: 0x6bf980,
            car: self.car,
        })?;
        if self.byte(self.car + 0x20f3)? != 0 {
            self.done_effect(CarGameDoneEffect::TrafficCleanup { entry: 0x4b9580 })?;
        }
        Ok(())
    }
}

impl VehicleInitState {
    /// Explicit GameDone over the same owner. The caller supplies fresh call
    /// storage and tracks successful GameInit/shutdown ordering. Interrupted
    /// cleanup retains its prefix and must not be retried as a transaction.
    pub fn car_game_done<B: CarGameDoneBackend>(
        &mut self,
        backend: &mut B,
        scratch: &mut CarGameInitScratch,
    ) -> Out<B, ()> {
        let car = self.validate_car_game_call_scratch(scratch)?;
        let memory = self
            .car_live_memory(&mut scratch.memory)
            .ok_or(CarGameInitUnavailable::Storage)?;
        GameInit {
            backend,
            memory,
            car,
            stack: scratch.stack,
        }
        .car_game_done_body()
    }
}
