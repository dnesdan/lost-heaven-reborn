//! Glass damage uses the simulation's canonical rows, visual owner and game.
use super::*;
use crate::vehicle_damage_glass::{
    CarDamageGlassBackend, VehicleDamageGlassBackend, VehicleDamageGlassCallback,
    VehicleGlassBreakCall,
};

impl<B> VehicleDamageGlassBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn damage_glass_callback(&mut self, call: VehicleDamageGlassCallback) -> Result<(), String> {
        crate::vehicle_damage_glass::car_damage_glass_callback(self, call).map_err(failure)
    }
}

impl<B> CarDamageGlassBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn car_glass_break(&mut self, call: VehicleGlassBreakCall) -> Result<(), String> {
        crate::break_plain_glass::break_plain_glass(
            self,
            call.manager,
            call.frame,
            call.direction,
            call.sound_name,
            call.retain_original,
        )
        .map_err(failure)
    }
}
