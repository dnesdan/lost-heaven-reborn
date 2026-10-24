//! Separately reconstructed Vehicle GameInit dependencies. Each receives the
//! same live projection; optional callbacks are explicit observable effects.
use super::*;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VehicleGameInitSmallDependency {
    InitDamage,
    ClearBodyCollAddTrack { header: u32 },
    AddProjectorLightsToSectors,
    SetDynamicCollsOn { first: u8, second: u8 },
}
pub fn vehicle_game_init_small_dependency<B: CarGameInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    vehicle: u32,
    stack: u32,
    dependency: VehicleGameInitSmallDependency,
) -> Out<B, ()> {
    let mut call = GameInit {
        backend,
        memory,
        car: vehicle.wrapping_sub(0x70),
        stack,
    };
    match dependency {
        VehicleGameInitSmallDependency::InitDamage => call.init_damage(),
        VehicleGameInitSmallDependency::ClearBodyCollAddTrack { header } => {
            call.clear_body_coll_add_track(header)
        }
        VehicleGameInitSmallDependency::AddProjectorLightsToSectors => {
            call.add_projector_lights_to_sectors()
        }
        VehicleGameInitSmallDependency::SetDynamicCollsOn { first, second } => {
            call.set_dynamic_colls_on(first, second)
        }
    }
}
impl<B: CarGameInitBackend> GameInit<'_, B> {
    fn vector_count_at(&mut self, header: u32, stride: u32) -> Out<B, u32> {
        let begin = self.read(header + 4)?;
        if begin == 0 {
            return Ok(0);
        }
        let end = self.read(header + 8)?;
        let count = (end.wrapping_sub(begin) as i32) / stride as i32;
        if count < 0 {
            return Err(CarGameInitUnavailable::Storage);
        }
        Ok(count as u32)
    }
    fn init_damage(&mut self) -> Out<B, ()> {
        let v = self.car + 0x70;
        let word = self.read(v + 0x1f0)?;
        self.put(v + 0x1ec, word)?;
        let word = self.read(v + 0x1b8)?;
        self.put(v + 0x1b4, word)?;
        self.put(v + 0x1b0, 1f32.to_bits())?;
        let mut i = 0;
        while (i as i32) < self.read(v + 0x4c4)? as i32 {
            let wheels = self.read(v + 0xc38)?;
            let wheel = self.read(wheels + i * 4)?;
            self.put(wheel + 0x188, 0)?;
            let wheels = self.read(v + 0xc38)?;
            let wheel = self.read(wheels + i * 4)?;
            let damage = self.read(wheel + 0x190)?;
            self.put(wheel + 0x18c, damage)?;
            i += 1;
        }
        let count = self.vector_count_at(v + 0xfc, 0x20)?;
        for i in (0..count).rev() {
            let begin = self.read(v + 0x100)?;
            let word = self.read(v + 0xcc)?;
            self.put(begin + i * 0x20 + 0x18, word)?;
            let begin = self.read(v + 0x100)?;
            let flags = self.read(begin + i * 0x20 + 0x14)?;
            self.put(begin + i * 0x20 + 0x14, flags & !1)?;
        }
        let count = self.vector_count_at(v + 0x10c, 0x1c)?;
        for i in (0..count).rev() {
            let begin = self.read(v + 0x110)?;
            self.put_byte(begin + i * 0x1c + 0x18, 3)?;
            self.put_byte(begin + i * 0x1c + 0x19, 0)?;
            let begin = self.read(v + 0x110)?;
            let frame = self.read(begin + i * 0x1c + 4)?;
            let argument = self.read(begin + i * 0x1c + 12)?;
            self.effect(CarGameInitEffect::FrameVirtual {
                frame,
                slot: 0x70,
                arguments: [argument, 0, 0, 0],
                count: 1,
            })?;
            let begin = self.read(v + 0x110)?;
            let frame = self.read(begin + i * 0x1c + 8)?;
            self.effect(CarGameInitEffect::FrameVirtual {
                frame,
                slot: 0x5c,
                arguments: [2f32.to_bits(), 0, 0, 0],
                count: 1,
            })?;
        }
        Ok(())
    }
    fn clear_body_coll_add_track(&mut self, header: u32) -> Out<B, ()> {
        let count = self.vector_count_at(header, 0x74)?;
        for i in (0..count).rev() {
            let begin = self.read(header + 4)?;
            self.clear_three_words(begin + i * 0x74 + 0x3c)?;
            let begin = self.read(header + 4)?;
            let subheader = begin + i * 0x74;
            let count = self.vector_count_at(subheader, 0x3c)?;
            for j in (0..count).rev() {
                let begin = self.read(header + 4)?;
                let points = self.read(begin + i * 0x74 + 4)?;
                self.clear_three_words(points + j * 0x3c + 0x1c)?;
            }
            let begin = self.read(header + 4)?;
            let nested = begin + i * 0x74 + 0x10;
            if self.vector_count_at(nested, 0x74)? != 0 {
                self.clear_body_coll_add_track(nested)?;
            }
        }
        Ok(())
    }
    fn clear_three_words(&mut self, address: u32) -> Out<B, ()> {
        for i in 0..3 {
            self.put(address + i * 4, 0)?;
        }
        Ok(())
    }
    fn add_projector_lights_to_sectors(&mut self) -> Out<B, ()> {
        let v = self.car + 0x70;
        let mut sectors = self.read(0x63d894)?;
        let count = self.vector_count_at(v + 0xfc, 0x20)?;
        for i in (0..count).rev() {
            let begin = self.read(v + 0x100)?;
            let light = self.read(begin + i * 0x20 + 12)?;
            if light == 0 {
                continue;
            }
            let count = self.vector_count_at(v + 0x10c, 0x1c)?;
            let mut found = false;
            let begin = self.read(v + 0x110)?;
            for j in (0..count).rev() {
                if self.read(begin + j * 0x1c + 8)? == light {
                    found = true;
                    break;
                }
            }
            if !found && sectors != 0 {
                let end = self.read(0x63d898)?;
                let count = (end.wrapping_sub(sectors) as i32) >> 2;
                if count < 0 {
                    return Err(CarGameInitUnavailable::Storage);
                }
                for j in (0..count as u32).rev() {
                    let sector = self.read(sectors + j * 4)?;
                    let begin = self.read(v + 0x100)?;
                    let light = self.read(begin + i * 0x20 + 12)?;
                    self.effect(CarGameInitEffect::AddSectorLight { sector, light })?;
                    sectors = self.read(0x63d894)?;
                }
            }
        }
        let count = self.vector_count_at(v + 0x10c, 0x1c)?;
        for i in (0..count).rev() {
            if sectors == 0 {
                continue;
            }
            let end = self.read(0x63d898)?;
            let count = (end.wrapping_sub(sectors) as i32) >> 2;
            if count < 0 {
                return Err(CarGameInitUnavailable::Storage);
            }
            for j in (0..count as u32).rev() {
                let sector = self.read(sectors + j * 4)?;
                let begin = self.read(v + 0x110)?;
                let light = self.read(begin + i * 0x1c + 8)?;
                self.effect(CarGameInitEffect::AddSectorLight { sector, light })?;
                sectors = self.read(0x63d894)?;
            }
        }
        Ok(())
    }
    fn dynamic_object(&mut self, object: u32, enabled: bool) -> Out<B, ()> {
        let entries = if enabled {
            &[0x5c36c0, 0x5c3ac0][..]
        } else {
            &[0x5c38b0][..]
        };
        for &entry in entries {
            self.effect(CarGameInitEffect::DynamicCollision {
                entry,
                manager: 0x647f48,
                object,
            })?;
        }
        Ok(())
    }
    fn set_dynamic_colls_on(&mut self, first: u8, second: u8) -> Out<B, ()> {
        let v = self.car + 0x70;
        if first != self.byte(v + 0x1fab)? {
            self.put_byte(v + 0x1fab, first)?;
            let many = self.byte(v + 0x1faa)? != 0;
            if many {
                let count = self.vector_count_at(v + 0x1fe0, 0x48)?;
                for i in (0..count).rev() {
                    let begin = self.read(v + 0x1fe4)?;
                    if first != 0 {
                        self.copy(v + 0x320, begin + i * 0x48 + 0x10, 3)?;
                        let flags = self.read(v + 0x1ffc)?;
                        if self.byte(flags + i * 8 + 4)? == 0 {
                            continue;
                        }
                    }
                    let begin = self.read(v + 0x1fe4)?;
                    self.dynamic_object(begin + i * 0x48, first != 0)?;
                }
            } else {
                if first != 0 {
                    self.copy(v + 0x320, v + 0x23c, 3)?;
                }
                self.dynamic_object(v + 0x22c, first != 0)?;
            }
        }
        if second != self.byte(v + 0x1fac)? {
            self.put_byte(v + 0x1fac, second)?;
            if second != 0 {
                self.copy(v + 0x320, v + 0x284, 3)?;
            }
            self.dynamic_object(v + 0x274, second != 0)?;
        }
        Ok(())
    }
}

/// C_using_object::AddObject, PC RVA0x15dd60. Uniqueness uses pointer identity;
/// insertion and any allocator mutation resolve through the same live graph.
/// `parameter` is the address of this call's already admitted object argument.
pub fn using_object_add_object<B: VehicleInitBackend>(
    backend: &mut B,
    memory: VehicleComponentTailMemory<'_>,
    manager: u32,
    object: u32,
    parameter: u32,
) -> Out<B, ()> {
    let mut projection = super::super::component_tail::Projection { backend, memory };
    let begin = projection
        .read_word(manager.wrapping_add(4))
        .map_err(CarGameInitUnavailable::Effect)?;
    if begin != 0 {
        let end = projection
            .read_word(manager.wrapping_add(8))
            .map_err(CarGameInitUnavailable::Effect)?;
        let count = (end.wrapping_sub(begin) as i32) >> 2;
        if count < 0 {
            return Err(CarGameInitUnavailable::Storage);
        }
        for i in (0..count as u32).rev() {
            if projection
                .read_word(begin.wrapping_add(i * 4))
                .map_err(CarGameInitUnavailable::Effect)?
                == object
            {
                return Ok(());
            }
        }
    }
    let position = projection
        .read_word(manager.wrapping_add(8))
        .map_err(CarGameInitUnavailable::Effect)?;
    crate::vehicle_component_records::vehicle_component_pointers_insert(
        &mut projection,
        manager,
        position,
        1,
        parameter,
    )
    .map_err(CarGameInitUnavailable::Effect)
}
