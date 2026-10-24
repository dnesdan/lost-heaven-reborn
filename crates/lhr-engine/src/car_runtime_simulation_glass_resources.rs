//! Glass effects resolve the simulation's same heap/frame/world/audio owners.
use super::*;
use crate::break_plain_glass::{BreakPlainGlassBackend, GlassPendingResource};
use crate::glass_subdivision::{GlassGeometry, GlassVertex};
impl<B> BreakPlainGlassBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn glass_frame_virtual(&mut self, frame: u32, slot: u32, args: &[u32]) -> Result<u32, String> {
        self.frames
            .crash_frame_call(frame, slot, args)
            .map_err(failure)
    }
    fn glass_update_world(&mut self, frame: u32) -> Result<(), String> {
        self.frames.crash_frame_world(frame).map_err(failure)
    }
    fn glass_frame_direction(
        &mut self,
        frame: u32,
        direction: [f32; 3],
        roll: f32,
    ) -> Result<(), String> {
        self.frames
            .glass_frame_direction(frame, direction, roll)
            .map_err(failure)
    }
    fn glass_new_frame(&mut self) -> Result<u32, String> {
        self.frames.glass_object_create().map_err(failure)
    }
    fn glass_source_geometry(&mut self, frame: u32) -> Result<GlassGeometry, String> {
        self.frames.glass_source_geometry(frame).map_err(failure)
    }
    fn glass_new_material(&mut self) -> Result<u32, String> {
        self.frames.glass_new_material().map_err(failure)
    }
    fn glass_copy_source_material(&mut self, frame: u32, material: u32) -> Result<(), String> {
        self.frames
            .glass_copy_source_material(frame, material)
            .map_err(failure)
    }
    fn glass_release_material(&mut self, material: u32) -> Result<(), String> {
        self.frames
            .glass_release_material(material)
            .map_err(failure)
    }
    fn glass_begin_fragment(&mut self, material: u32) -> Result<u32, String> {
        self.frames.glass_begin_fragment(material).map_err(failure)
    }
    fn glass_cleanup_resources(&mut self) -> Result<(), String> {
        self.frames.glass_cleanup_resources().map_err(failure)
    }
    fn glass_install_fragment(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex; 6],
    ) -> Result<bool, String> {
        self.frames
            .glass_install_fragment(frame, group, vertices)
            .map_err(failure)
    }
    fn glass_place_sector(&mut self, frame: u32) -> Result<(), String> {
        self.frames.crash_frame_place_sector(frame).map_err(failure)
    }
    fn glass_debustize(&mut self, frame: u32) -> Result<(), String> {
        crate::crash_actors::crash_debustize(self, frame).map_err(failure)
    }
    fn glass_ground_plane(&mut self, position: [f32; 3]) -> Result<([f32; 3], [f32; 3]), String> {
        let mut point = [
            position[0],
            crate::native_precision::add(position[1], 10000.),
            position[2],
        ];
        let mut normal = [0., 1., 0.];
        self.glass_test_line(
            position,
            [0., -500., 0.],
            0xffff,
            0,
            &mut point,
            &mut normal,
        )?;
        Ok((point, normal))
    }
    fn glass_test_line(
        &mut self,
        origin: [f32; 3],
        delta: [f32; 3],
        selector: u32,
        flags: u32,
        point: &mut [f32; 3],
        normal: &mut [f32; 3],
    ) -> Result<u32, String> {
        let mut outputs = CollisionLineOutputs {
            point: Some(*point),
            normal: Some(*normal),
        };
        let result = self
            .contact_test_line_v(
                CollisionLineRequest {
                    origin,
                    delta,
                    selector: selector as i32,
                    reject_mask: flags,
                },
                &mut outputs,
            )
            .map_err(failure)?;
        *point = outputs.point.ok_or("glass line missing point output")?;
        *normal = outputs.normal.ok_or("glass line missing normal output")?;
        Ok(result.map_or(0, |hit| hit.primitive))
    }
    fn glass_play_sound(&mut self, name: u32, position: [f32; 3]) -> Result<(), String> {
        self.play_glass_break_sound(name, position)
    }
    fn glass_merge_fragments(&mut self, entry: u32) -> Result<bool, String> {
        crate::break_plain_glass_merge::merge_glass_fragments(self, entry).map_err(failure)
    }
    fn glass_allocator_marker(&self) -> u8 {
        self.state.allocator_marker
    }
    fn glass_track(&mut self, resource: GlassPendingResource) {
        self.state.glass_pending.push(resource)
    }
    fn glass_untrack(&mut self, resource: GlassPendingResource) -> Result<(), String> {
        let index = self
            .state
            .glass_pending
            .iter()
            .rposition(|r| *r == resource)
            .ok_or_else(|| format!("Missing pending glass ownership {resource:?}"))?;
        self.state.glass_pending.remove(index);
        Ok(())
    }
    fn glass_pending_last(&self) -> Option<GlassPendingResource> {
        self.state.glass_pending.last().copied()
    }
}

impl<B> crate::break_plain_glass_merge::GlassMergeBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn glass_merge_material(&mut self, frame: u32) -> Result<u32, String> {
        self.frames.glass_merge_material(frame).map_err(failure)
    }
    fn glass_merge_read_triangle(&mut self, frame: u32) -> Result<[GlassVertex; 3], String> {
        self.frames
            .glass_merge_read_triangle(frame)
            .map_err(failure)
    }
    fn glass_merge_begin_group(
        &mut self,
        material: u32,
        faces: &[[u16; 3]],
    ) -> Result<u32, String> {
        self.frames
            .glass_merge_begin_group(material, faces)
            .map_err(failure)
    }
    fn glass_merge_install(
        &mut self,
        frame: u32,
        group: u32,
        vertices: &[GlassVertex],
    ) -> Result<bool, String> {
        self.frames
            .glass_merge_install(frame, group, vertices)
            .map_err(failure)
    }
}
