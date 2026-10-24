//! Original glass factories attach fragments to the one live frame/resource owner.
use super::*;
use crate::glass_subdivision::{GlassGeometry, GlassVertex};
impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    pub(crate) fn glass_object_create(&mut self) -> Result<u32, VehicleFrameError<B::Error>> {
        let key = self.arena.create_glass_object()?;
        self.frame_address(key)
    }
    pub(crate) fn glass_source_geometry(
        &mut self,
        frame: u32,
    ) -> Result<GlassGeometry, VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let object = self.arena.frame_object_resources(key)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("glass Object owner"),
        )?;
        self.arena
            .visual_resource_owner_mut()?
            .glass_source_geometry(object)
            .map_err(ModelError::LoadedVisual)
            .map_err(Into::into)
    }
    pub(crate) fn glass_new_material(&mut self) -> Result<u32, VehicleFrameError<B::Error>> {
        let material = self
            .arena
            .visual_resource_owner_mut()?
            .deform_create_material()
            .map_err(ModelError::LoadedVisual)?;
        deform::address(0xc400_0000, material.slot())
    }
    pub(crate) fn glass_copy_source_material(
        &mut self,
        frame: u32,
        material: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let object = self.arena.frame_object_resources(key)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("glass Object owner"),
        )?;
        let material = self.material_key(material)?;
        self.arena
            .visual_resource_owner_mut()?
            .glass_copy_material_to(object, material)
            .map_err(ModelError::LoadedVisual)?;
        Ok(())
    }
    pub(crate) fn glass_begin_fragment(
        &mut self,
        material: u32,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let material = self.material_key(material)?;
        let group = self
            .arena
            .visual_resource_owner_mut()?
            .glass_begin_group(material, &[[0, 1, 2], [3, 4, 5]])
            .map_err(ModelError::LoadedVisual)?;
        deform::address(0xc300_0000, group.slot())
    }
    pub(crate) fn glass_cleanup_resources(&mut self) -> Result<(), VehicleFrameError<B::Error>> {
        self.arena.cleanup_glass_resources()?;
        Ok(())
    }
    pub(crate) fn glass_install_fragment(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex; 6],
    ) -> Result<bool, VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let group = self.group_key(group)?;
        let mesh = self
            .arena
            .visual_resource_owner_mut()?
            .glass_finish_group(group, vertices)
            .map_err(ModelError::LoadedVisual)?;
        self.arena.set_frame_mesh(key, Some(mesh), false)?;
        self.arena
            .visual_resource_owner_mut()?
            .glass_release_mesh(mesh)
            .map_err(ModelError::LoadedVisual)?;
        let object = self.arena.frame_object_resources(key)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("glass Object owner"),
        )?;
        self.arena
            .visual_resource_owner_mut()?
            .game_update_object_vertices(object, 0)
            .map_err(ModelError::LoadedVisual)?;
        Ok(true)
    }
    pub(crate) fn glass_frame_direction(
        &mut self,
        frame: u32,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let rotation = SQuat::try_make_frame_direction(direction, roll)
            .map_err(|_| VehicleFrameError::NumericDomain("glass frame SetDir"))?;
        let state = self
            .arena
            .graph
            .projected_state_mut(key)
            .map_err(ModelError::from)?;
        state.rotation = rotation;
        state.flags = (state.flags & 0xffff_fecb) | 0x4000_0008;
        Ok(())
    }
}

impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    pub(crate) fn glass_release_material(
        &mut self,
        material: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.material_key(material)?;
        self.arena
            .visual_resource_owner_mut()?
            .deform_release_material(key)
            .map_err(ModelError::LoadedVisual)?;
        Ok(())
    }
}

impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    pub(crate) fn glass_merge_material(
        &self,
        frame: u32,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let object = self.arena.frame_object_resources(key)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("glass merge Object"),
        )?;
        let material = self
            .resource_owner()?
            .glass_borrow_material(object)
            .map_err(ModelError::LoadedVisual)?;
        deform::address(0xc400_0000, material.slot())
    }
    pub(crate) fn glass_merge_read_triangle(
        &mut self,
        frame: u32,
    ) -> Result<[GlassVertex; 3], VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let object = self.arena.frame_object_resources(key)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("glass merge Object"),
        )?;
        self.arena
            .visual_resource_owner_mut()?
            .glass_read_triangle(object)
            .map_err(ModelError::LoadedVisual)
            .map_err(Into::into)
    }
    pub(crate) fn glass_merge_begin_group(
        &mut self,
        material: u32,
        faces: &[[u16; 3]],
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let material = self.material_key(material)?;
        let group = self
            .arena
            .visual_resource_owner_mut()?
            .glass_begin_group(material, faces)
            .map_err(ModelError::LoadedVisual)?;
        deform::address(0xc300_0000, group.slot())
    }
    pub(crate) fn glass_merge_install(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex],
    ) -> Result<bool, VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let group = self.group_key(group)?;
        let mesh = self
            .arena
            .visual_resource_owner_mut()?
            .glass_finish_group(group, vertices)
            .map_err(ModelError::LoadedVisual)?;
        self.arena.set_frame_mesh(key, Some(mesh), false)?;
        self.arena
            .visual_resource_owner_mut()?
            .glass_release_mesh(mesh)
            .map_err(ModelError::LoadedVisual)?;
        let object = self.arena.frame_object_resources(key)?.ok_or(
            VehicleFrameError::UnavailableFrameMethod("glass merge Object"),
        )?;
        self.arena
            .visual_resource_owner_mut()?
            .game_update_object_vertices(object, 0)
            .map_err(ModelError::LoadedVisual)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fallible_frame_mesh_attachment_keeps_temporary_mesh_owner_until_explicit_close() {
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let mut memory = VehicleFrameMemory::default();
        let mut b = VehicleFrameBackend::new(&mut arena, &mut memory);
        let frame = b.frame_address(model.root()).unwrap();
        let material = b.glass_new_material().unwrap();
        let vertex = GlassVertex {
            position: [0., 0., 0.],
            normal: [0., 1., 0.],
            uv: [0.; 2],
        };
        let group = b.glass_merge_begin_group(material, &[[0, 1, 2]]).unwrap();
        assert!(b.glass_merge_install(frame, group, &[vertex; 3]).is_err());
        assert!(b.resource_owner().unwrap().backend().live_graphics() >= 2);
        b.arena_mut().publish_runtime_buffers().unwrap();
        b.glass_cleanup_resources().unwrap();
        b.glass_release_material(material).unwrap();
        b.arena_mut().release(model.root()).unwrap();
        b.arena_mut().reclaim_unused_visual_buffers().unwrap();
        let resources = b.resource_owner().unwrap();
        assert_eq!(resources.backend().live_graphics(), 0);
        assert_eq!(resources.backend().live_allocations(), 0);
    }
}
