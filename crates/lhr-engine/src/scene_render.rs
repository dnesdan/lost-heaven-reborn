//! Canonical Scene active-camera projection into the GPU presentation boundary.
//! Camera evaluation and all source transforms remain ModelArena operations.
use crate::{
    frame_camera::CameraKey, model_loading::ModelArena, scene_ownership::SceneKey,
    scene_resources::SceneViewport,
};
use glam::{Mat4, Vec3};
use lhr_render::{SceneRenderFrame, SceneRenderPass};

/// Caller-owned traversal identifiers for one loaded Scene. Every prepared
/// snapshot consumes an identifier, even if its surface frame is never shown.
/// Native visibility caches must not see a repeated number in a new output list.
#[derive(Default)]
pub struct SceneSnapshotSequence {
    number: u32,
}

impl SceneSnapshotSequence {
    pub fn next_number(&mut self) -> Result<u32, String> {
        let next = self
            .number
            .checked_add(1)
            .ok_or("Scene snapshot sequence exhausted; reload the scene")?;
        self.number = next;
        Ok(next)
    }
}

#[derive(Clone, Debug, Default)]
pub struct ScenePassDraws {
    pub backdrop: Vec<usize>,
    pub main: Vec<usize>,
    pub near: Vec<usize>,
}
/// A presentation-space mesh already reflects source Z during CPU intake.
/// Native SMatrix uses row vectors; its contiguous rows are the transposed
/// column-vector matrix, so from_cols_array consumes the existing bits directly.
fn gpu_matrix(matrix: crate::native_transform::SMatrix) -> Mat4 {
    Mat4::from_cols_array(&matrix.elements) * Mat4::from_scale(Vec3::new(1., 1., -1.))
}
pub fn camera_frame(
    arena: &mut ModelArena,
    scene: SceneKey,
    viewport: SceneViewport,
    draws: ScenePassDraws,
) -> Result<(CameraKey, SceneRenderFrame), String> {
    let camera = arena
        .scene_active_camera(scene)
        .map_err(|e| e.to_string())?
        .ok_or("Canonical Scene has no active camera")?;
    arena.camera_update(camera).map_err(|e| e.to_string())?;
    let camera_state = arena.camera(camera).map_err(|e| e.to_string())?;
    let matrix = |offset| {
        camera_state
            .matrix(offset)
            .map(gpu_matrix)
            .ok_or("Canonical camera view-projection is incomplete")
    };
    let rectangle = viewport.rectangle.map(u32::try_from);
    let [Ok(left), Ok(top), Ok(right), Ok(bottom)] = rectangle else {
        return Err(
            "Canonical Scene viewport has negative boundaries unsupported by the GPU".into(),
        );
    };
    Ok((
        camera,
        SceneRenderFrame {
            viewport: [left, top, right, bottom],
            initial_cull: None,
            backdrop: SceneRenderPass {
                view_projection: matrix(0x324)?,
                draws: draws.backdrop,
            },
            main: SceneRenderPass {
                view_projection: matrix(0x1e4)?,
                draws: draws.main,
            },
            near: SceneRenderPass {
                view_projection: matrix(0x3a4)?,
                draws: draws.near,
            },
        },
    ))
}
/// LOD distance and horizontal FOV read the exact same active camera as drawing.
pub fn active_lod_view(
    arena: &mut ModelArena,
    scene: SceneKey,
    quality: f32,
) -> Result<crate::inspection_lod::InspectionLodView, String> {
    let camera = arena
        .scene_active_camera(scene)
        .map_err(|e| e.to_string())?
        .ok_or("Canonical Scene has no active camera")?;
    arena.camera_update(camera).map_err(|e| e.to_string())?;
    let world = arena
        .world_matrix(camera.frame())
        .map_err(|e| e.to_string())?;
    let fov = arena
        .camera(camera)
        .map_err(|e| e.to_string())?
        .scalar(0x140)
        .ok_or("Active camera FOV unavailable")?;
    if !fov.is_finite() || fov <= 0. || !quality.is_finite() || quality < 0. {
        return Err("Active camera LOD domain unavailable".into());
    }
    let correction = crate::native_matrix::checked_f32_div(std::f32::consts::FRAC_PI_2, fov)
        .map_err(|_| "LOD FOV correction unavailable")?;
    let rs_lod_quality = crate::native_matrix::checked_f32_mul(quality, correction)
        .map_err(|_| "LOD quality unavailable")?;
    Ok(crate::inspection_lod::InspectionLodView {
        camera_pc: world.elements[12..15].try_into().unwrap(),
        rs_lod_quality,
    })
}
/// Listener pose from the actual canonical camera, in unreflected PC space.
pub fn active_listener(
    arena: &mut ModelArena,
    scene: SceneKey,
) -> Result<([f32; 3], [f32; 3]), String> {
    let camera = arena
        .scene_active_camera(scene)
        .map_err(|e| e.to_string())?
        .ok_or("Canonical Scene has no active camera")?;
    arena.camera_update(camera).map_err(|e| e.to_string())?;
    let world = arena
        .world_matrix(camera.frame())
        .map_err(|e| e.to_string())?;
    Ok((
        world.elements[12..15].try_into().unwrap(),
        world.elements[0..3].try_into().unwrap(),
    ))
}

#[path = "scene_render_owner.rs"]
mod owner;
pub use owner::{
    cleanup_failure_slots, inspect_cleanup_failure, snapshot_scene, CanonicalSceneOwner,
    CanonicalSceneSnapshot, CanonicalSceneState,
};

#[cfg(test)]
#[path = "scene_render_tests.rs"]
mod tests;
