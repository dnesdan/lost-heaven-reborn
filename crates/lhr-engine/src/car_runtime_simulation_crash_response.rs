//! Canonical descriptor bridge for the original physical-object response.
use super::*;
use crate::ph_thing_response::{PhThingCollisionDescriptor, PhThingResponseBackend};

impl<B> PhThingResponseBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn ph_thing_collision_descriptor(
        &mut self,
        descriptor: u32,
    ) -> Result<PhThingCollisionDescriptor, Self::Error> {
        if let Some(source) = self.world.collision().primitive(descriptor) {
            let properties = source.properties as u8;
            // The original static KLZ classifications have no dynamic flag40.
            // A reached dynamic owner requires its actual canonical record.
            if properties & 0x40 != 0 {
                return Err(format!(
                    "Dynamic actor owner for static-world descriptor {descriptor}"
                ));
            }
            return Ok(PhThingCollisionDescriptor {
                properties,
                material: source.material(),
                owner: 0,
            });
        }
        let properties = self.read_byte(descriptor)?;
        let owner = if properties & 0x40 != 0 {
            self.read_word(descriptor.wrapping_add(0x44))?
        } else {
            0
        };
        let material = self.read_byte(descriptor.wrapping_add(3))?;
        Ok(PhThingCollisionDescriptor {
            properties,
            material,
            owner,
        })
    }
}
