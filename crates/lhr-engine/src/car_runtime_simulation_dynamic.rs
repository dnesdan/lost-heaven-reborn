//! Original dynamic/static sphere composition on one shared query parameter.
use super::*;
impl<B> CarRuntimeSimulationBackend<'_, '_, B>
where
    B: VehicleComponentTailBackend,
    B::Error: std::fmt::Debug,
{
    pub(super) fn body_sphere_dynamic_and_static(
        &mut self,
        q: VehicleBodySphereSweep,
    ) -> Result<Option<VehicleBodySweepHit>, String> {
        let request = crate::collision_swept::SphereSweepRequest {
            origin: q.start,
            delta: q.displacement,
            radius: q.radius,
            selector: q.mask as i32,
            reject_mask: q.flags,
        };
        let mut param = crate::collision_swept::SphereParam::new(&request).map_err(failure)?;
        let dynamic_hit = if q.mask != u32::MAX {
            crate::dynamic_collision_queries::sphere::sphere_in_dynamics(
                self, 0x647f48, q.mask, &mut param,
            )
            .map_err(failure)?
        } else {
            false
        };
        let normal = self.vector(0x63d9d8)?;
        let mut outputs = CollisionLineOutputs {
            point: Some([0.; 3]),
            normal: Some(normal),
        };
        let result = crate::collision_swept_world::test_sphere_b_continue(
            self.world.collision(),
            &request,
            &mut outputs,
            &mut self.state.sphere_traversal,
            param,
            dynamic_hit,
        )
        .map_err(failure)?;
        if let (Some(_), Some(normal)) = (result, outputs.normal) {
            for (i, x) in normal.into_iter().enumerate() {
                self.write_word(0x63d9d8 + i as u32 * 4, x.to_bits())?;
            }
        }
        Ok(result.map(|hit| VehicleBodySweepHit {
            descriptor: hit.primitive,
            position: outputs.point.unwrap(),
            normal: outputs.normal.unwrap(),
        }))
    }
}
