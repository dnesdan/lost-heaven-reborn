//! Light/projector operations share the frame arena and its loaded texture owner.
use super::*;
use crate::visual_resources::TextureKey;
const TEXTURE_END: u32 = 0xc100_0000;
impl<B: VehicleInitBackend> VehicleFrameBackend<'_, B> {
    pub(super) fn texture_address(
        &self,
        key: TextureKey,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let owner = self
            .arena
            .loaded_visual_resources()
            .ok_or(VehicleFrameError::UnavailableFrameMethod("texture owner"))?;
        owner.cpu_texture(key).map_err(ModelError::LoadedVisual)?;
        u32::try_from(key.0)
            .ok()
            .and_then(|v| v.checked_mul(16))
            .and_then(|v| RESOURCE_BASE.checked_add(v))
            .filter(|p| *p < TEXTURE_END)
            .ok_or(VehicleFrameError::Address(RESOURCE_BASE))
    }
    pub(super) fn texture_key(
        &self,
        address: u32,
    ) -> Result<TextureKey, VehicleFrameError<B::Error>> {
        if !(RESOURCE_BASE..TEXTURE_END).contains(&address)
            || !(address - RESOURCE_BASE).is_multiple_of(16)
        {
            return Err(VehicleFrameError::Address(address));
        }
        let key = TextureKey(u64::from((address - RESOURCE_BASE) / 16));
        self.texture_address(key)?;
        Ok(key)
    }
}

impl<B: VehicleComponentTailBackend> VehicleLightResourceBackend for VehicleFrameBackend<'_, B> {
    fn create_vehicle_light_visual(&mut self, subtype: u32) -> Result<u32, Self::Error> {
        if subtype != 0x4a4f5250 {
            return Err(VehicleFrameError::UnavailableFrameMethod(
                "CreateVisual subtype",
            ));
        }
        let key = self.arena.create_projector()?;
        self.frame_address(key)
    }
    fn create_vehicle_light_frame(&mut self, kind: u32) -> Result<u32, Self::Error> {
        if kind != 2 {
            return Err(VehicleFrameError::UnavailableFrameMethod(
                "CreateFrame kind",
            ));
        }
        let key = self.arena.create_light()?;
        self.frame_address(key)
    }
    fn vehicle_light_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, Self::Error> {
        if (RESOURCE_BASE..TEXTURE_END).contains(&object) {
            if slot != 0 || !args.is_empty() {
                return Err(VehicleFrameError::UnavailableFrameMethod("texture virtual"));
            }
            let key = self.texture_key(object)?;
            self.arena.release_cpu_texture(key)?;
            return Ok(self
                .arena
                .loaded_visual_resources()
                .and_then(|r| r.backend().texture_references(key))
                .unwrap_or(0));
        }
        let key = self.frame_key(object)?;
        let kind = self.arena.graph.state(key).map_err(ModelError::from)?.kind;
        match (slot, args) {
            (0, []) => {
                let refs = self.arena.frame(key)?.references;
                self.arena.release(key)?;
                return Ok(refs.wrapping_sub(1));
            }
            (0x18, []) => {
                self.arena.graph.update(key).map_err(ModelError::from)?;
            }
            (0x24, [on]) => {
                self.arena.set_frame_on(key, *on)?;
            }
            (0x2c, [parent, flags]) => {
                let parent = if *parent == 0 {
                    None
                } else {
                    Some(self.frame_key(*parent)?)
                };
                self.arena.link_frame(key, parent, *flags)?;
            }
            (0x70, [texture]) if kind == FrameKind::Projector => {
                let texture = if *texture == 0 {
                    None
                } else {
                    Some(self.texture_key(*texture)?)
                };
                self.arena.set_projector_texture(key, texture)?;
            }
            (0x78, [orthogonal]) if kind == FrameKind::Projector => {
                self.arena
                    .set_projector_orthogonal(key, *orthogonal as u8)?;
            }
            (0x84, [pointer]) if kind == FrameKind::Projector => {
                let mut color = [0.; 3];
                for (i, x) in color.iter_mut().enumerate() {
                    *x = f32::from_bits(self.read_word(pointer.wrapping_add(i as u32 * 4))?);
                }
                self.arena.set_projector_color(key, color)?;
            }
            (0x88, [mode]) if kind == FrameKind::Projector => {
                self.arena.set_projector_mode(key, *mode)?;
            }
            (0x8c, [lock]) if kind == FrameKind::Projector => {
                self.arena.set_projector_lock(key, *lock)?;
            }
            (0x50, [light_type]) if kind == FrameKind::Light => {
                self.arena
                    .graph
                    .set_light_type(key, *light_type)
                    .map_err(ModelError::from)?;
            }
            (0x60, [mode]) if kind == FrameKind::Light => {
                self.arena
                    .graph
                    .set_light_mode(key, *mode)
                    .map_err(ModelError::from)?;
            }
            (0x5c, [power]) if kind == FrameKind::Light => {
                self.arena
                    .graph
                    .set_light_diffuse_power(key, f32::from_bits(*power))
                    .map_err(ModelError::from)?;
            }
            (0x64, [near, far]) if kind == FrameKind::Light => {
                self.arena
                    .graph
                    .set_light_range(key, f32::from_bits(*near), f32::from_bits(*far))
                    .map_err(ModelError::from)?;
            }
            (0x68, [inner, outer]) if kind == FrameKind::Light => {
                self.arena
                    .graph
                    .set_light_cone(key, f32::from_bits(*inner), f32::from_bits(*outer))
                    .map_err(ModelError::from)?;
            }
            (0x58, [pointer]) if kind == FrameKind::Light => {
                let mut color = [0.; 3];
                for (i, x) in color.iter_mut().enumerate() {
                    *x = f32::from_bits(self.read_word(pointer.wrapping_add(i as u32 * 4))?);
                }
                self.arena
                    .graph
                    .set_light_diffuse(key, color)
                    .map_err(ModelError::from)?;
            }
            _ => {
                return Err(VehicleFrameError::UnavailableFrameMethod(
                    "light/projector virtual",
                ));
            }
        }
        // These native setter/update return registers are unused by their callers.
        Ok(0)
    }
    fn vehicle_light_texture_cache(&mut self, name: &[u8]) -> Result<(u32, u32), Self::Error> {
        let installation = self
            .installation
            .ok_or(VehicleFrameError::UnavailableFrameMethod(
                "texture installation",
            ))?;
        let name = std::str::from_utf8(name)
            .map_err(|_| VehicleFrameError::UnavailableFrameMethod("texture filename encoding"))?;
        let key = self.arena.load_cpu_texture(installation, name)?;
        Ok((0, self.texture_address(key)?))
    }
}
impl<B: VehicleComponentTailBackend> VehicleLightInitBackend for VehicleFrameBackend<'_, B> {
    fn enumerate_vehicle_lights(
        &mut self,
        root: u32,
        query: VehicleLightQuery,
        target: &mut VehicleLightEnumeration<'_>,
    ) -> Result<(), Self::Error> {
        self.enumerate_frames(root, query.pattern(), query.mask(), |b, f| {
            target.visit(b, f)
        })
        .map(|_| ())
    }
    fn find_vehicle_light_billboard(&mut self, frame: u32) -> Result<u32, Self::Error> {
        let mut found = 0;
        self.enumerate_frames(frame, b"BILLBOARD*", 0x1ffff, |_, f| {
            found = f;
            Ok::<_, Self::Error>(2)
        })?;
        Ok(found)
    }
    fn set_vehicle_light_on(&mut self, frame: u32, on: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena.set_frame_on(key, on)?;
        Ok(())
    }
}

#[cfg(test)]
mod projector_dispatch_tests {
    use super::*;

    #[test]
    fn original_projector_slots_keep_mode_and_lock_independent() {
        let mut arena = ModelArena::default();
        let mut memory = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let address = backend.create_vehicle_light_visual(0x4a4f5250).unwrap();
        let key = backend.frame_key(address).unwrap();
        assert_eq!(backend.arena.projector(key).unwrap().color, [0.; 3]);
        backend.vehicle_light_virtual(address, 0x88, &[4]).unwrap();
        assert_eq!(backend.arena.projector(key).unwrap().mode, 4);
        assert_eq!(backend.arena.projector(key).unwrap().lock, 0);
        backend.vehicle_light_virtual(address, 0x8c, &[7]).unwrap();
        assert_eq!(backend.arena.projector(key).unwrap().mode, 4);
        assert_eq!(backend.arena.projector(key).unwrap().lock, 7);
        // Reverse-factor has its own slot; it must not alias mode or lock.
        assert!(backend.vehicle_light_virtual(address, 0x80, &[0]).is_err());
        assert_eq!(backend.arena.projector(key).unwrap().reverse, 0.5);
        assert_eq!(backend.vehicle_light_virtual(address, 0, &[]).unwrap(), 0);
        assert!(backend.arena.projector(key).is_err());
    }
}
