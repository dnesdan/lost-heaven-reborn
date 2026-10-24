//! Original table intake into the standalone caller's one addressable owner.
use super::*;
impl CarRuntime {
    pub(super) fn own_game_table(&mut self, bytes: &[u8]) -> Result<u32, String> {
        let address = VehicleInitBackend::allocate(
            &mut self.memory,
            bytes
                .len()
                .try_into()
                .map_err(|_| "Game table allocation extent")?,
        )
        .map_err(|e| format!("Game table allocation: {e:?}"))?;
        if address == 0 {
            return Err("Game table allocation returned null".into());
        }
        self.services.table_allocations.push(address);
        for (i, &byte) in bytes.iter().enumerate() {
            VehicleInitBackend::write_byte(&mut self.memory, address + i as u32, byte)
                .map_err(|e| format!("Game table bytes: {e:?}"))?;
        }
        Ok(address)
    }
    pub(super) fn load_driving_tables(&mut self) -> Result<(), String> {
        use crate::driving_vehicle_tables::DrivingVehicleTables;
        let tables = DrivingVehicleTables::load(&self.installation)?;
        let source_materials = self.own_game_table(tables.source_material_bytes())?;
        self.memory
            .map_bytes(0x6d4c24, &source_materials.to_le_bytes())
            .map_err(|e| format!("Original material definitions: {e:?}"))?;
        let count = (tables.source_material_bytes().len()
            / crate::driving_vehicle_tables::MATERIAL_SOURCE_STRIDE) as u32;
        self.memory
            .map_bytes(0x6d4c28, &count.to_le_bytes())
            .map_err(|e| format!("Original material count: {e:?}"))?;
        let materials = self.own_game_table(tables.material_bytes())?;
        self.memory
            .map_bytes(0x63d8b0, tables.material_index_map())
            .map_err(|e| format!("Game material index: {e:?}"))?;
        self.memory
            .map_bytes(0x63d9b4, &materials.to_le_bytes())
            .map_err(|e| format!("Game material pointer: {e:?}"))?;
        let mut tyres = DrivingVehicleTables::user_tyre_record().to_vec();
        for tyre in tables.tyres() {
            let mut surfaces = Vec::with_capacity(tyre.surfaces.len() * 0x5c);
            for row in &tyre.surfaces {
                surfaces.extend_from_slice(row)
            }
            let begin = self.own_game_table(&surfaces)?;
            let end = begin + surfaces.len() as u32;
            let mut header = vec![0; 16];
            header[4..8].copy_from_slice(&begin.to_le_bytes());
            header[8..12].copy_from_slice(&end.to_le_bytes());
            header[12..16].copy_from_slice(&end.to_le_bytes());
            let header = self.own_game_table(&header)?;
            tyres.extend_from_slice(&tyre.id.to_le_bytes());
            tyres.extend_from_slice(&tyre.name_bytes);
            tyres.extend_from_slice(&header.to_le_bytes());
        }
        let begin = self.own_game_table(&tyres)?;
        VehicleInitBackend::write_word(&mut self.memory, 0x63d9c8, begin)
            .map_err(|e| format!("Game tyre pointer: {e:?}"))?;
        VehicleInitBackend::write_word(&mut self.memory, 0x63d9cc, begin + tyres.len() as u32)
            .map_err(|e| format!("Game tyre extent: {e:?}"))?;
        Ok(())
    }
    pub(super) fn release_driving_tables(&mut self) -> Result<(), String> {
        while let Some(&address) = self.services.table_allocations.last() {
            VehicleInitBackend::free(&mut self.memory, address)
                .map_err(|e| format!("Game table release: {e:?}"))?;
            self.services.table_allocations.pop();
        }
        if self.game_init_started {
            for address in [0x63d9c8, 0x63d9cc, 0x63d9b4, 0x6d4c24, 0x6d4c28] {
                VehicleInitBackend::write_word(&mut self.memory, address, 0)
                    .map_err(|e| format!("Game table reset: {e:?}"))?;
            }
            if self.driving.is_some() {
                for address in [0x647de8, 0x647dec, 0x647df0] {
                    VehicleInitBackend::write_word(&mut self.memory, address, 0)
                        .map_err(|e| format!("Model cache directory reset: {e:?}"))?;
                }
            }
        }
        Ok(())
    }
}
