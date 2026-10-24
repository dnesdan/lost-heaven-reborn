//! Stable canonical resource fields and locked vertices of the live visual owner.
use super::*;
use crate::visual_resources::MeshLevelKey;
const VERTEX_BASE: u32 = 0xc500_0000;
const VERTEX_END: u32 = 0xf000_0000;
const VERTEX_SLOT: u32 = 0x10000;
const RESOURCE_SLOT: u32 = 0x100;

impl<B: VehicleInitBackend> VehicleFrameBackend<'_, B> {
    fn game_vertex_address(&self, key: MeshLevelKey) -> Result<u32, VehicleFrameError<B::Error>> {
        key.slot()
            .checked_mul(VERTEX_SLOT)
            .and_then(|offset| VERTEX_BASE.checked_add(offset))
            .filter(|address| *address < VERTEX_END)
            .ok_or(VehicleFrameError::Address(key.slot()))
    }
    pub(super) fn game_resource_byte(
        &self,
        address: u32,
    ) -> Result<Option<u8>, VehicleFrameError<B::Error>> {
        if (VERTEX_BASE..VERTEX_END).contains(&address) {
            let relative = address - VERTEX_BASE;
            let key = self
                .resource_owner()?
                .lookup_lod(relative / VERTEX_SLOT)
                .map_err(ModelError::LoadedVisual)?;
            return self
                .resource_owner()?
                .game_vertex_byte(key, (relative % VERTEX_SLOT) as usize)
                .map(Some)
                .map_err(ModelError::LoadedVisual)
                .map_err(Into::into);
        }
        let (base, kind) = if (0xc100_0000..0xc200_0000).contains(&address) {
            (0xc100_0000, 0)
        } else if (0xc200_0000..0xc300_0000).contains(&address) {
            (0xc200_0000, 1)
        } else {
            return Ok(None);
        };
        let object = base + (address - base) / RESOURCE_SLOT * RESOURCE_SLOT;
        let offset = (address - base) % RESOURCE_SLOT;
        let aligned = offset & !3;
        let word = if kind == 0 {
            self.resource_owner()?
                .game_mesh_word(self.mesh_key(object)?, aligned)
        } else {
            self.resource_owner()?
                .game_level_word(self.lod_key(object)?, aligned)
        }
        .map_err(ModelError::LoadedVisual)?;
        Ok(Some(word.to_le_bytes()[(offset & 3) as usize]))
    }
    pub(super) fn game_write_resource_byte(
        &mut self,
        address: u32,
        value: u8,
    ) -> Result<bool, VehicleFrameError<B::Error>> {
        if !(VERTEX_BASE..VERTEX_END).contains(&address) {
            return Ok(false);
        }
        let relative = address - VERTEX_BASE;
        let key = self
            .resource_owner()?
            .lookup_lod(relative / VERTEX_SLOT)
            .map_err(ModelError::LoadedVisual)?;
        self.arena
            .visual_resource_owner_mut()?
            .game_write_vertex_byte(key, (relative % VERTEX_SLOT) as usize, value)
            .map_err(ModelError::LoadedVisual)?;
        Ok(true)
    }
    pub(super) fn game_mesh_virtual(
        &mut self,
        object: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<Option<u32>, VehicleFrameError<B::Error>> {
        if (0xc100_0000..0xc200_0000).contains(&object) {
            let key = self.mesh_key(object)?;
            return match (slot, args) {
                (4, [index]) => Ok(Some(
                    self.resource_owner()?
                        .deform_mesh_lod(key, 0, *index)
                        .map_err(ModelError::LoadedVisual)?
                        .map(|level| deform::address(0xc200_0000, level.slot()))
                        .transpose()?
                        .unwrap_or(0),
                )),
                (0x14, []) => {
                    self.arena
                        .visual_resource_owner_mut()?
                        .game_update_mesh_bounds(key)
                        .map_err(ModelError::LoadedVisual)?;
                    Ok(Some(0))
                }
                _ => Err(VehicleFrameError::UnavailableFrameMethod(
                    "mesh object virtual",
                )),
            };
        }
        if (0xc200_0000..0xc300_0000).contains(&object) {
            let key = self.lod_key(object)?;
            return match (slot, args) {
                (4, [flags]) => {
                    let count = self
                        .resource_owner()?
                        .game_level_word(key, 8)
                        .map_err(ModelError::LoadedVisual)?;
                    if count > 2048 {
                        return Err(VehicleFrameError::UnavailableFrameMethod(
                            "canonical vertex pointer extent",
                        ));
                    }
                    let address = self.game_vertex_address(key)?;
                    let locked = self
                        .arena
                        .visual_resource_owner_mut()?
                        .game_lock_level_vertices(key, *flags)
                        .map_err(ModelError::LoadedVisual)?;
                    Ok(Some(if locked { address } else { 0 }))
                }
                (8, []) => {
                    self.arena
                        .visual_resource_owner_mut()?
                        .game_unlock_level_vertices(key)
                        .map_err(ModelError::LoadedVisual)?;
                    Ok(Some(0))
                }
                (0x14, [index]) => Ok(Some(
                    self.resource_owner()?
                        .deform_lod_face_group(key, *index)
                        .map_err(ModelError::LoadedVisual)?
                        .map(|group| deform::address(0xc300_0000, group.slot()))
                        .transpose()?
                        .unwrap_or(0),
                )),
                (0x28, []) => {
                    self.arena
                        .visual_resource_owner_mut()?
                        .game_update_level_bbox(key)
                        .map_err(ModelError::LoadedVisual)?;
                    Ok(Some(0))
                }
                _ => Err(VehicleFrameError::UnavailableFrameMethod(
                    "mesh level virtual",
                )),
            };
        }
        if (0xc300_0000..0xc400_0000).contains(&object) {
            if let (0, [material]) = (slot, args) {
                let group = self.group_key(object)?;
                let material = if *material == 0 {
                    None
                } else {
                    Some(self.material_key(*material)?)
                };
                self.arena
                    .visual_resource_owner_mut()?
                    .game_set_group_material(group, material)
                    .map_err(ModelError::LoadedVisual)?;
                return Ok(Some(0));
            }
            return Err(VehicleFrameError::UnavailableFrameMethod(
                "face group virtual",
            ));
        }
        if let (0x78, [index]) = (slot, args) {
            if let Ok(key) = self.frame_key(object) {
                if let Some(resources) = self.arena.frame_object_resources(key)? {
                    self.arena
                        .visual_resource_owner_mut()?
                        .game_update_object_vertices(resources, *index)
                        .map_err(ModelError::LoadedVisual)?;
                    return Ok(Some(0));
                }
            }
        }
        Ok(None)
    }
}
