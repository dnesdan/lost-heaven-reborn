//! The same simulation/frame/world owners implement CreateCrashActors effects.
use super::*;
use crate::crash_actors::{CrashActorsBackend, CrashSourcePose};
impl<B> CrashActorsBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn crash_track(&mut self, resource: crate::crash_actors::CrashPendingResource) {
        self.state.crash_pending.push(resource);
    }
    fn crash_untrack(
        &mut self,
        resource: crate::crash_actors::CrashPendingResource,
    ) -> Result<(), String> {
        let index = self
            .state
            .crash_pending
            .iter()
            .rposition(|item| *item == resource)
            .ok_or_else(|| format!("Missing pending crash ownership {resource:?}"))?;
        self.state.crash_pending.remove(index);
        Ok(())
    }
    fn crash_pending_last(&self) -> Option<crate::crash_actors::CrashPendingResource> {
        self.state.crash_pending.last().copied()
    }
    fn crash_observe_piece(
        &mut self,
        model: u32,
        actor: u32,
        frame: u32,
        parameters: [u32; 5],
    ) -> Result<(), String> {
        if self.state.crash_pieces.len() >= 4096 {
            return Ok(());
        }
        let key = self.frames.frame_key(frame).map_err(failure)?;
        let loaded = self.frames.arena().frame(key).map_err(failure)?;
        let source_index = loaded.source_index;
        let name = loaded.name_bytes.clone();
        let model = crate::model_loading::ModelKey(self.frames.frame_key(model).map_err(failure)?);
        let model_resource = self
            .frames
            .arena()
            .model_filename(model)
            .map_err(failure)?
            .map(|filename| {
                self.frames
                    .arena()
                    .filename(filename)
                    .map(|record| record.bytes().to_vec())
                    .map_err(failure)
            })
            .transpose()?;
        let local_bbox = self
            .frames
            .arena_mut()
            .hr_bound_volume(key)
            .map_err(failure)?
            .bbox;
        let inertia = [
            self.read_word(actor + 0x214)?,
            self.read_word(actor + 0x218)?,
            self.read_word(actor + 0x21c)?,
        ];
        self.state
            .crash_pieces
            .push(crate::crash_actors::CrashPieceObservation {
                actor,
                frame,
                source_index,
                name,
                model_resource,
                parameters,
                local_bbox,
                inertia,
            });
        Ok(())
    }
    fn crash_source_pose(&mut self, identity: u32) -> Result<CrashSourcePose, String> {
        if (crate::driving_world_assets::DRIVING_WORLD_FRAME_BASE
            ..crate::driving_world_assets::DRIVING_WORLD_FRAME_END)
            .contains(&identity)
        {
            let key = self.world.crash_frame_key(identity)?;
            let source = self.world.frame_source(key)?;
            let local_scale = source.local_scale;
            let has_parent = source.parent.is_some();
            let matrix = self.world.frame_world_matrix(key)?;
            Ok(CrashSourcePose {
                matrix,
                local_scale,
                has_parent,
            })
        } else {
            let key = self.frames.frame_key(identity).map_err(failure)?;
            let local_scale = self.frames.arena().state(key).map_err(failure)?.scale;
            let has_parent = self
                .frames
                .arena()
                .links(key)
                .map_err(failure)?
                .parent
                .is_some();
            let matrix = self.frames.arena_mut().world_matrix(key).map_err(failure)?;
            Ok(CrashSourcePose {
                matrix,
                local_scale,
                has_parent,
            })
        }
    }
    fn crash_new_model(&mut self) -> Result<u32, String> {
        self.frames.crash_model_create().map_err(failure)
    }
    fn crash_model_open(&mut self, model: u32, path: &[u8]) -> Result<i32, String> {
        self.frames
            .crash_model_open_original(model, path)
            .map_err(failure)
    }
    fn crash_model_duplicate(&mut self, destination: u32, source: u32) -> Result<(), String> {
        self.frames
            .crash_model_duplicate_pristine(destination, source)
            .map_err(failure)
    }
    fn crash_frame_virtual(&mut self, frame: u32, slot: u32, args: &[u32]) -> Result<u32, String> {
        self.frames
            .crash_frame_call(frame, slot, args)
            .map_err(failure)
    }
    fn crash_update_world(&mut self, frame: u32) -> Result<(), String> {
        self.frames.crash_frame_world(frame).map_err(failure)
    }
    fn crash_place_frame_sector(&mut self, frame: u32) -> Result<(), String> {
        self.frames.crash_frame_place_sector(frame).map_err(failure)
    }
    fn crash_test_col_hierarchy(
        &mut self,
        point: [f32; 3],
        delta: [f32; 3],
        model: u32,
    ) -> Result<u32, String> {
        self.frames
            .crash_test_hierarchy(point, delta, model)
            .map_err(failure)
    }
    fn crash_allocate_actor(&mut self) -> Result<u32, String> {
        let actor = self.allocate(0x3c0)?;
        if actor != 0 {
            self.crash_track(crate::crash_actors::CrashPendingResource::ActorAllocation(
                actor,
            ));
            let fill = self.state.allocator_marker;
            for offset in 0..0x3c0 {
                self.write_byte(actor + offset, fill)?;
            }
        }
        Ok(actor)
    }
    fn crash_allocator_marker(&self) -> u8 {
        self.state.allocator_marker
    }
    fn crash_actor_kick(&mut self, actor: u32) -> Result<(), String> {
        crate::crash_actor::crash_actor_kick(self, actor).map_err(failure)
    }
}
