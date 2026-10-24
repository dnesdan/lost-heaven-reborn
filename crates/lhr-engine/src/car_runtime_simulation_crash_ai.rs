//! The physical-object AI borrows the same frame, Game heap and collision grid.
use super::*;
use crate::ph_thing_ai::{PhThingAiBackend, PhThingEffect, PhThingLineHit};

impl<B> PhThingAiBackend for CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    fn ph_thing_effect(&mut self, effect: PhThingEffect) -> Result<u32, String> {
        match effect {
            PhThingEffect::UpdateWorld { frame } => {
                self.frames.crash_frame_world(frame).map_err(failure)?;
                Ok(0)
            }
            PhThingEffect::FrameVirtual { frame, slot } => self
                .frames
                .crash_frame_call(frame, slot, &[])
                .map_err(failure),
            PhThingEffect::SetNextCollisionOwner { owner, flags: 0 } => {
                // Original 5c3d10, the same manager state used by Move's
                // BodyCol group calls. Both callers pass a zero second word.
                self.body_collision_filter(VehicleBodyCollisionFilter::Group(owner))?;
                Ok(0)
            }
            other => Err(format!("C_PhThing AI dependency {other:?}")),
        }
    }
    fn ph_thing_test_line_v(
        &mut self,
        actor: u32,
        origin: [f32; 3],
        delta: [f32; 3],
    ) -> Result<PhThingLineHit, String> {
        let request = CollisionLineRequest {
            origin,
            delta,
            selector: 2,
            reject_mask: 0,
        };
        let mut outputs = CollisionLineOutputs {
            point: Some([0.; 3]),
            normal: Some(self.vector(actor + 0x100)?),
        };
        let hit = self.contact_test_line_v(request, &mut outputs)?;
        Ok(PhThingLineHit {
            object: hit.map_or(0, |hit| hit.primitive),
            position: outputs.point.ok_or("MoveBox TestLineV point output")?,
            normal: outputs.normal.ok_or("MoveBox TestLineV normal output")?,
        })
    }
    fn ph_thing_actor_world_position(&mut self, actor: u32) -> Result<[f32; 3], String> {
        // C_actor::GetWorldPos407fe0: a model-less actor uses its cached
        // position; a model-backed actor reads the actual updated frame.
        let frame = self.read_word(actor + 0x68)?;
        if frame == 0 {
            self.vector(actor + 0x24)
        } else {
            if self.read_byte(frame + 0xac)? & 0x20 == 0 {
                self.frames.crash_frame_world(frame).map_err(failure)?;
            }
            self.vector(frame + 0x40)
        }
    }
}
