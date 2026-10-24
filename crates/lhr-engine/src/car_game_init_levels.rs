//! Original packed vehicle-level conversion and table loading dependencies.
use super::*;
#[path = "car_game_init_levels_convert.rs"]
mod convert;
pub use convert::car_convert_packed_vehicle_parameters;

/// Calls the existing original parameter worker with a live canonical view.
/// The flags2 sound dependency receives that same view rather than a snapshot.
/// This is a host composition adapter, not a new original entry point.
pub fn car_apply_level_source_parameters<B: VehicleSourceParametersBackend>(
    backend: &mut B,
    mut memory: VehicleComponentTailMemory<'_>,
    vehicle: u32,
    source: u32,
    flags: u8,
) -> Result<bool, VehicleWheelsUnavailable<B::Error>> {
    let accepted = {
        let mut projection = super::super::component_tail::Projection {
            backend: &mut *backend,
            memory: memory.reborrow(),
        };
        super::super::source_parameters::apply_parameters(
            &mut projection,
            vehicle,
            source,
            flags & !2,
        )?
    };
    if accepted && flags & 2 != 0 {
        backend
            .parameter_initialize_sounds(Some(memory), vehicle, vehicle.wrapping_add(0xe6c))
            .map_err(VehicleInitUnavailable::Effect)?;
    }
    Ok(accepted)
}

impl<B: CarGameInitSupportBackend> GameInit<'_, B> {
    fn level_read(&mut self, ret: u32, handle: u32, target: u32, bytes: u32) -> Out<B, ()> {
        self.call(ret, &[handle, target, bytes])?;
        self.support_effect(CarGameInitSupportEffect::ReadLevelTable {
            handle,
            target,
            bytes,
        })?;
        Ok(())
    }
    fn level_close(&mut self, ret: u32, handle: u32) -> Out<B, ()> {
        self.call(ret, &[handle])?;
        self.support_effect(CarGameInitSupportEffect::CloseLevelTable { handle })?;
        Ok(())
    }
    fn level_free(&mut self, ret: u32, address: u32) -> Out<B, ()> {
        self.call(ret, &[address])?;
        self.backend
            .free_projected(&mut self.memory, address)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn level_allocate(&mut self, ret: u32, bytes: u32) -> Out<B, u32> {
        self.call(ret, &[bytes])?;
        self.backend
            .allocate_projected(&mut self.memory, bytes)
            .map_err(CarGameInitUnavailable::Effect)
    }
    fn uppercase_level_name(&mut self, address: u32) -> Out<B, ()> {
        if self.read(0x6d87c4)? != 0 {
            return Err(CarGameInitUnavailable::Dependency { entry: 0x610acc });
        }
        let mut offset = 0u32;
        loop {
            let byte = self.byte(address.wrapping_add(offset))?;
            if byte == 0 {
                return Ok(());
            }
            // Actual locale-zero CRT branch uses signed-char comparisons and
            // changes only lowercase ASCII. Other byte values remain intact.
            if byte.is_ascii_lowercase() {
                self.put_byte(address.wrapping_add(offset), byte.to_ascii_uppercase())?;
            }
            offset = offset.wrapping_add(1);
        }
    }
    fn equal_level_names(&mut self, left: u32, right: u32) -> Out<B, bool> {
        let mut offset = 0u32;
        loop {
            let a = self.byte(left.wrapping_add(offset))?;
            let b = self.byte(right.wrapping_add(offset))?;
            if a != b {
                return Ok(false);
            }
            if a == 0 {
                return Ok(true);
            }
            offset = offset.wrapping_add(1);
        }
    }
    fn append_level_record(&mut self, record: u32, capacity: u32) -> Out<B, ()> {
        const STRIDE: u32 = 0x113c;
        let end = self.read(self.car + 0x21b8)?;
        let free = (capacity.wrapping_sub(end) as i32) / STRIDE as i32;
        if free as u32 >= 1 {
            // The original insertion point is always the current end. Its
            // general middle-insertion branches are unreachable in this entry.
            self.copy(record, end, 0x44f)?;
            let end = self.read(self.car + 0x21b8)?;
            self.put(self.car + 0x21b8, end.wrapping_add(STRIDE))?;
            return Ok(());
        }
        let begin = self.read(self.car + 0x21b4)?;
        let length = if begin == 0 {
            0
        } else {
            (end.wrapping_sub(begin) as i32) / STRIDE as i32
        };
        let growth = if begin == 0 || length as u32 <= 1 {
            1
        } else {
            length
        };
        let count = length.wrapping_add(growth);
        let allocated_count = if count < 0 { 0 } else { count as u32 };
        let new_begin = self.level_allocate(0x41e4bb, allocated_count.wrapping_mul(STRIDE))?;
        let old_begin = self.read(self.car + 0x21b4)?;
        if length < 0 {
            return Err(CarGameInitUnavailable::Storage);
        }
        self.copy(old_begin, new_begin, (length as u32).wrapping_mul(0x44f))?;
        let new_end = new_begin.wrapping_add((length as u32).wrapping_mul(STRIDE));
        self.put(self.stack.wrapping_add(0x2c), new_end)?;
        self.copy(record, new_end, 0x44f)?;
        let old_begin = self.read(self.car + 0x21b4)?;
        self.level_free(0x41e50f, old_begin)?;
        self.put(
            self.car + 0x21bc,
            new_begin.wrapping_add((count as u32).wrapping_mul(STRIDE)),
        )?;
        let old_begin = self.read(self.car + 0x21b4)?;
        let length = if old_begin == 0 {
            0
        } else {
            (self.read(self.car + 0x21b8)?.wrapping_sub(old_begin) as i32) / STRIDE as i32
        };
        self.put(self.car + 0x21b4, new_begin)?;
        self.put(
            self.car + 0x21b8,
            new_begin.wrapping_add((length as u32).wrapping_add(1).wrapping_mul(STRIDE)),
        )?;
        Ok(())
    }
    /// C_car::LoadLevelTables, PC canonical RVA1e270. File operations use the
    /// original DTA boundary, not an executable or a caller-invented record.
    pub(super) fn load_level_tables(&mut self) -> Out<B, u32> {
        let _old_end = self.read(self.car + 0x21b8)?;
        let begin = self.read(self.car + 0x21b4)?;
        self.put(self.stack.wrapping_add(0x20), self.car)?;
        self.put(self.car + 0x21b8, begin)?;
        self.call(0x41e2bd, &[0x63236c, 0])?;
        let handle = self.support_effect(CarGameInitSupportEffect::OpenLevelTable {
            path: 0x63236c,
            flags: 0,
        })?;
        self.put(self.stack.wrapping_add(0x1c), handle)?;
        if handle == u32::MAX {
            self.level_free(0x41e2cf, 0)?;
            return Ok(0);
        }
        self.level_read(0x41e2ec, handle, self.stack.wrapping_add(0x18), 4)?;
        if self.read(self.stack.wrapping_add(0x18))? != 1 {
            self.level_close(0x41e2f9, handle)?;
            self.level_free(0x41e300, 0)?;
            return Ok(0);
        }
        self.level_read(0x41e31d, handle, self.stack.wrapping_add(0x10), 4)?;
        while self.read(self.stack.wrapping_add(0x10))? != 0 {
            let remaining = self.read(self.stack.wrapping_add(0x10))?.wrapping_sub(1);
            self.put(self.stack.wrapping_add(0x10), remaining)?;
            self.level_read(0x41e33f, handle, self.stack.wrapping_add(0x18), 4)?;
            self.level_read(0x41e34c, handle, self.stack.wrapping_add(0x30), 0x20)?;
            self.call(0x41e356, &[self.stack.wrapping_add(0x30)])?;
            self.uppercase_level_name(self.stack.wrapping_add(0x30))?;
            if self
                .equal_level_names(self.stack.wrapping_add(0x30), self.car.wrapping_add(0x218c))?
            {
                self.level_read(0x41e3a9, handle, self.stack.wrapping_add(0x14), 4)?;
                while self.read(self.stack.wrapping_add(0x14))? != 0 {
                    let remaining = self.read(self.stack.wrapping_add(0x14))?.wrapping_sub(1);
                    self.put(self.stack.wrapping_add(0x14), remaining)?;
                    self.level_read(0x41e3cb, handle, self.stack.wrapping_add(0x24), 4)?;
                    let bytes = self.read(self.stack.wrapping_add(0x24))?;
                    let packed = self.level_allocate(0x41e3d5, bytes)?;
                    self.put(self.stack.wrapping_add(0x28), packed)?;
                    if packed == 0 {
                        self.level_free(0x41e300, 0)?;
                        return Ok(0);
                    }
                    let bytes = self.read(self.stack.wrapping_add(0x24))?;
                    self.level_read(0x41e3f6, handle, packed, bytes)?;
                    let car = self.read(self.stack.wrapping_add(0x20))?;
                    if car != self.car {
                        return Err(CarGameInitUnavailable::Storage);
                    }
                    let converted = self.stack.wrapping_add(0x118c);
                    self.call(0x41e40b, &[packed, converted, 0])?;
                    self.convert_packed_parameters(packed, converted, false)?;
                    let capacity = self.read(self.car + 0x21bc)?;
                    self.copy(converted, self.stack.wrapping_add(0x50), 0x449)?;
                    self.copy(
                        self.car.wrapping_add(0x20b4),
                        self.stack.wrapping_add(0x1174),
                        6,
                    )?;
                    self.append_level_record(self.stack.wrapping_add(0x50), capacity)?;
                    let packed = self.read(self.stack.wrapping_add(0x28))?;
                    self.level_free(0x41e62e, packed)?;
                }
                let remaining = self.read(self.stack.wrapping_add(0x14))?.wrapping_sub(1);
                self.put(self.stack.wrapping_add(0x14), remaining)?;
            } else {
                let offset = self.read(self.stack.wrapping_add(0x18))?.wrapping_sub(0x20);
                self.call(0x41e657, &[handle, offset, 1])?;
                self.support_effect(CarGameInitSupportEffect::SeekLevelTable {
                    handle,
                    offset,
                    origin: 1,
                })?;
            }
        }
        let remaining = self.read(self.stack.wrapping_add(0x10))?.wrapping_sub(1);
        self.put(self.stack.wrapping_add(0x10), remaining)?;
        self.level_close(0x41e672, handle)?;
        self.level_free(0x41e679, 0)?;
        Ok(1)
    }
}
