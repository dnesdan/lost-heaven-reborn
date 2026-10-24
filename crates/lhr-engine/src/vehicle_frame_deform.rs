//! Deformation resources are addresses of the arena's existing typed owners.
use super::*;
use crate::visual_resources::{FaceGroupKey, MaterialKey, MeshLevelKey, MeshObjectKey};
const MESH: u32 = 0xc100_0000;
const LOD: u32 = 0xc200_0000;
const GROUP: u32 = 0xc300_0000;
const MATERIAL: u32 = 0xc400_0000;
const WIDTH: u32 = 0x0100_0000;
pub(super) fn address<E>(base: u32, slot: u32) -> Result<u32, VehicleFrameError<E>> {
    slot.checked_mul(256)
        .filter(|v| *v < WIDTH)
        .and_then(|v| base.checked_add(v))
        .ok_or(VehicleFrameError::Address(base))
}
pub(super) fn slot<E>(base: u32, p: u32) -> Result<u32, VehicleFrameError<E>> {
    if !(base..base + WIDTH).contains(&p) || !(p - base).is_multiple_of(256) {
        Err(VehicleFrameError::Address(p))
    } else {
        Ok((p - base) / 256)
    }
}
impl<B: VehicleInitBackend> VehicleFrameBackend<'_, B> {
    pub(super) fn resource_owner(
        &self,
    ) -> Result<&crate::loaded_visual_resources::LoadedVisualResources, VehicleFrameError<B::Error>>
    {
        self.arena
            .loaded_visual_resources()
            .ok_or(VehicleFrameError::UnavailableFrameMethod(
                "visual resource owner",
            ))
    }
    pub(super) fn mesh_key(&self, p: u32) -> Result<MeshObjectKey, VehicleFrameError<B::Error>> {
        Ok(self
            .resource_owner()?
            .lookup_mesh(slot(MESH, p)?)
            .map_err(ModelError::LoadedVisual)?)
    }
    pub(super) fn lod_key(&self, p: u32) -> Result<MeshLevelKey, VehicleFrameError<B::Error>> {
        Ok(self
            .resource_owner()?
            .lookup_lod(slot(LOD, p)?)
            .map_err(ModelError::LoadedVisual)?)
    }
    pub(super) fn group_key(&self, p: u32) -> Result<FaceGroupKey, VehicleFrameError<B::Error>> {
        Ok(self
            .resource_owner()?
            .lookup_face_group(slot(GROUP, p)?)
            .map_err(ModelError::LoadedVisual)?)
    }
    pub(super) fn material_key(&self, p: u32) -> Result<MaterialKey, VehicleFrameError<B::Error>> {
        Ok(self
            .resource_owner()?
            .lookup_material(slot(MATERIAL, p)?)
            .map_err(ModelError::LoadedVisual)?)
    }
    pub(super) fn release_deform_resource(
        &mut self,
        p: u32,
    ) -> Result<bool, VehicleFrameError<B::Error>> {
        if (MESH..MESH + WIDTH).contains(&p) {
            let k = self.mesh_key(p)?;
            self.arena
                .visual_resource_owner_mut()?
                .deform_release_mesh(k)
                .map_err(ModelError::LoadedVisual)?;
            Ok(true)
        } else if (MATERIAL..MATERIAL + WIDTH).contains(&p) {
            let k = self.material_key(p)?;
            self.arena
                .visual_resource_owner_mut()?
                .deform_release_material(k)
                .map_err(ModelError::LoadedVisual)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
impl<B: VehicleInitBackend> VehicleDeformBackend for VehicleFrameBackend<'_, B> {
    fn deform_visual_mesh(&mut self, frame: u32) -> Result<u32, Self::Error> {
        let frame = self.frame_key(frame)?;
        let object = self.arena.frame_object_resources(frame)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("visual object resource"),
        )?;
        let mesh = self
            .resource_owner()?
            .object_mesh(object)
            .map_err(ModelError::LoadedVisual)?;
        mesh.map_or(Ok(0), |k| address(MESH, k.slot()))
    }
    fn deform_set_visual_mesh(
        &mut self,
        frame: u32,
        mesh: u32,
        flags: u32,
    ) -> Result<(), Self::Error> {
        let frame = self.frame_key(frame)?;
        let mesh = if mesh == 0 {
            None
        } else {
            Some(self.mesh_key(mesh)?)
        };
        self.arena.set_frame_mesh(frame, mesh, flags != 0)?;
        Ok(())
    }
    fn deform_release_mesh(&mut self, mesh: u32) -> Result<(), Self::Error> {
        let k = self.mesh_key(mesh)?;
        self.arena
            .visual_resource_owner_mut()?
            .deform_release_mesh(k)
            .map_err(ModelError::LoadedVisual)?;
        Ok(())
    }
    fn deform_create_mesh(&mut self) -> Result<u32, Self::Error> {
        let k = self
            .arena
            .visual_resource_owner_mut()?
            .deform_create_empty_mesh()
            .map_err(ModelError::LoadedVisual)?;
        address(MESH, k.slot())
    }
    fn deform_duplicate_mesh(&mut self, d: u32, s: u32) -> Result<u32, Self::Error> {
        let d = self.mesh_key(d)?;
        let s = if s == 0 {
            None
        } else {
            Some(self.mesh_key(s)?)
        };
        Ok(self
            .arena
            .visual_resource_owner_mut()?
            .deform_duplicate_mesh(d, s)
            .map_err(ModelError::LoadedVisual)? as u32)
    }
    fn deform_mesh_lod_count(&mut self, m: u32) -> Result<u32, Self::Error> {
        let m = self.mesh_key(m)?;
        Ok(self
            .resource_owner()?
            .deform_mesh_lod_count(m, 0)
            .map_err(ModelError::LoadedVisual)?)
    }
    fn deform_remove_mesh_lod(&mut self, m: u32, i: u32) -> Result<(), Self::Error> {
        let m = self.mesh_key(m)?;
        self.arena
            .visual_resource_owner_mut()?
            .deform_remove_mesh_lod(m, 0, i)
            .map_err(ModelError::LoadedVisual)?;
        Ok(())
    }
    fn deform_mesh_lod(&mut self, m: u32, i: u32) -> Result<u32, Self::Error> {
        let m = self.mesh_key(m)?;
        self.resource_owner()?
            .deform_mesh_lod(m, 0, i)
            .map_err(ModelError::LoadedVisual)?
            .map_or(Ok(0), |k| address(LOD, k.slot()))
    }
    fn deform_lod_face_group(&mut self, l: u32, i: u32) -> Result<u32, Self::Error> {
        let l = self.lod_key(l)?;
        self.resource_owner()?
            .deform_lod_face_group(l, i)
            .map_err(ModelError::LoadedVisual)?
            .map_or(Ok(0), |k| address(GROUP, k.slot()))
    }
    fn deform_face_group_material(&mut self, g: u32) -> Result<u32, Self::Error> {
        let g = self.group_key(g)?;
        self.resource_owner()?
            .deform_face_group_material(g)
            .map_err(ModelError::LoadedVisual)?
            .map_or(Ok(0), |k| address(MATERIAL, k.slot()))
    }
    fn deform_retain_material(&mut self, m: u32) -> Result<(), Self::Error> {
        let m = self.material_key(m)?;
        self.arena
            .visual_resource_owner_mut()?
            .deform_retain_material(m)
            .map_err(ModelError::LoadedVisual)?;
        Ok(())
    }
    fn deform_create_material(&mut self) -> Result<u32, Self::Error> {
        let m = self
            .arena
            .visual_resource_owner_mut()?
            .deform_create_material()
            .map_err(ModelError::LoadedVisual)?;
        address(MATERIAL, m.slot())
    }
    fn deform_copy_material(&mut self, d: u32, s: u32) -> Result<(), Self::Error> {
        let d = self.material_key(d)?;
        let s = if s == 0 {
            None
        } else {
            Some(self.material_key(s)?)
        };
        self.arena
            .visual_resource_owner_mut()?
            .deform_set_material_copy(d, s)
            .map_err(ModelError::LoadedVisual)?;
        Ok(())
    }
}

impl<B: VehicleComponentTailBackend + VehicleSourceParametersBackend> VehicleSetupBackend
    for VehicleFrameBackend<'_, B>
{
    fn enumerate_setup_frames(
        &mut self,
        model: u32,
        pattern: &[u8],
        mask: u32,
        target: &mut VehicleSetupEnumeration<'_>,
    ) -> Result<(), VehicleInitUnavailable<Self::Error>> {
        self.enumerate_frames(model, pattern, mask, |b, f| {
            target.visit(b, f).map_err(VehicleInitUnavailable::Effect)
        })
        .map(|_| ())
    }
    fn setup_frame_bounds(&mut self, frame: u32) -> Result<(), Self::Error> {
        let key = self.frame_key(frame)?;
        self.arena.update_hr_bound_vol(key)?;
        Ok(())
    }
    fn setup_deform(
        &mut self,
        memory: VehicleComponentTailMemory<'_>,
        stack: u32,
    ) -> Result<(), Self::Error> {
        let mut target = VehicleDeformEnumeration::new(memory, stack);
        let model = target.model(self)?;
        self.enumerate_frames_at_depth(model, b"*", 0x10001, |b, f, depth| {
            target.visit(b, f, depth)
        })?;
        target.prepare(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_addresses_survive_adapter_recreation_and_reject_destroyed_owners() {
        let mut arena = ModelArena::default();
        let mut memory = VehicleFrameMemory::default();
        let (mesh, duplicate, material, replacement) = {
            let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
            let mesh = backend.deform_create_mesh().unwrap();
            let duplicate = backend.deform_create_mesh().unwrap();
            assert_ne!(mesh, duplicate);
            assert_eq!(backend.deform_duplicate_mesh(duplicate, mesh), Ok(0));
            let material = backend.deform_create_material().unwrap();
            let replacement = backend.deform_create_material().unwrap();
            backend.deform_copy_material(replacement, material).unwrap();
            backend.deform_retain_material(material).unwrap();
            (mesh, duplicate, material, replacement)
        };
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        assert_eq!(backend.deform_mesh_lod_count(mesh), Ok(0));
        assert_eq!(backend.deform_mesh_lod_count(duplicate), Ok(0));
        assert!(backend.deform_mesh_lod_count(mesh + 1).is_err());
        assert!(backend.deform_mesh_lod_count(material).is_err());
        backend.release_vehicle_object(mesh).unwrap();
        assert!(backend.deform_mesh_lod_count(mesh).is_err());
        assert_eq!(backend.deform_mesh_lod_count(duplicate), Ok(0));
        backend.release_vehicle_object(material).unwrap();
        assert!(backend.material_key(material).is_ok());
        backend.release_vehicle_object(material).unwrap();
        assert!(backend.material_key(material).is_err());
        backend.release_vehicle_object(replacement).unwrap();
        backend.release_vehicle_object(duplicate).unwrap();
        assert!(backend.material_key(replacement).is_err());
        assert!(backend.deform_mesh_lod_count(duplicate).is_err());
    }
}
