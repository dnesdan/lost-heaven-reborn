//! Current host-camera inputs for the source ordinary live-object LOD bridge.
//! Camera eye is converted back to PC coordinates; FOV correction uses the
//! renderer's actual HORIZONTAL FOV for the current viewport, not a second FOV.
use crate::native_matrix::{
    checked_f32_add as add, checked_f32_div as div, checked_f32_mul as mul, checked_f32_sub as sub,
};
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InspectionLodView {
    pub camera_pc: [f32; 3],
    /// Source render.cpp: RS_LOD_QUALITY * ((PI/2)/horizontal FOV).
    pub rs_lod_quality: f32,
}
impl InspectionLodView {
    pub fn from_camera(
        camera: &lhr_render::Camera,
        viewport: [u32; 2],
        quality: f32,
    ) -> Result<Self, String> {
        camera.validate()?;
        let fov = camera.horizontal_fov_radians(viewport)?;
        let correction =
            div(std::f32::consts::FRAC_PI_2, fov).map_err(|_| "LOD FOV correction unavailable")?;
        let quality = mul(quality, correction).map_err(|_| "LOD quality correction unavailable")?;
        Ok(Self {
            camera_pc: [camera.eye.x, camera.eye.y, -camera.eye.z],
            rs_lod_quality: quality,
        })
    }
    pub(crate) fn distance_squared(self, center: [f32; 3]) -> Result<f32, String> {
        let [x, y, z] = [0, 1, 2].map(|i| sub(center[i], self.camera_pc[i]));
        let value = (|| {
            let (x, y, z) = (x?, y?, z?);
            add(add(mul(x, x)?, mul(y, y)?)?, mul(z, z)?)
        })();
        value.map_err(|_| "LOD squared-distance arithmetic unavailable".into())
    }
}
