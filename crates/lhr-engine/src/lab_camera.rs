//! Host-only camera and model preview operations. These never edit source owners.
use crate::lab_state::{CameraView, ModelTransform};
use glam::{EulerRot, Mat4, Quat, Vec3};
use lhr_render::{Camera, Scene};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bookmark {
    eye: Vec3,
    target: Vec3,
    up: Vec3,
    near: f32,
    far: f32,
    fov: f32,
}

impl Bookmark {
    pub fn capture(camera: &Camera) -> Self {
        Self {
            eye: camera.eye,
            target: camera.target,
            up: camera.up,
            near: camera.near,
            far: camera.far,
            fov: camera.vertical_fov_radians,
        }
    }

    pub fn camera(self) -> Camera {
        Camera {
            eye: self.eye,
            target: self.target,
            up: self.up,
            near: self.near,
            far: self.far,
            vertical_fov_radians: self.fov,
        }
    }
}

pub fn model_matrix(settings: ModelTransform, pivot: Vec3, turntable_degrees: f32) -> Mat4 {
    let rotation = settings.rotation_degrees.map(f32::to_radians);
    Mat4::from_translation(Vec3::from_array(settings.translation))
        * Mat4::from_translation(pivot)
        * Mat4::from_quat(
            Quat::from_rotation_y(turntable_degrees.to_radians())
                * Quat::from_euler(EulerRot::XYZ, rotation[0], rotation[1], rotation[2]),
        )
        * Mat4::from_scale(Vec3::splat(settings.scale))
        * Mat4::from_translation(-pivot)
}

pub fn fit(scene: &Scene, transform: Mat4, selected: Option<usize>) -> Result<Camera, String> {
    if selected.is_some_and(|index| index >= scene.meshes.len()) {
        return Err("Selected draw is no longer available".into());
    }
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for (index, mesh) in scene.meshes.iter().enumerate() {
        if selected.is_some_and(|selected| selected != index) {
            continue;
        }
        for vertex in &mesh.vertices {
            let point = transform.transform_point3(Vec3::from_array(vertex.position));
            if !point.is_finite() {
                return Err("Preview bounds contain a nonfinite coordinate".into());
            }
            min = min.min(point);
            max = max.max(point);
        }
    }
    Camera::fit_bounds(min.to_array(), max.to_array())
}

pub fn set_view(camera: &mut Camera, view: CameraView) -> Result<(), String> {
    let direction = match view {
        CameraView::Front => Vec3::Z,
        CameraView::Back => -Vec3::Z,
        CameraView::Left => -Vec3::X,
        CameraView::Right => Vec3::X,
        CameraView::Top => Vec3::Y,
        CameraView::Isometric => Vec3::new(1., 0.65, 1.).normalize(),
    };
    let mut next = Bookmark::capture(camera).camera();
    next.eye = next.target + direction * (camera.eye - camera.target).length();
    next.up = if view == CameraView::Top {
        -Vec3::Z
    } else {
        Vec3::Y
    };
    next.validate()?;
    *camera = next;
    Ok(())
}

/// Radians and a multiplicative distance. Bound pitch and distance before
/// publication so a long wheel/drag event cannot put the camera in its target.
pub fn orbit(camera: &mut Camera, yaw: f32, pitch: f32, zoom: f32) -> Result<(), String> {
    if !yaw.is_finite() || !pitch.is_finite() || !zoom.is_finite() || zoom <= 0. {
        return Err("Invalid camera motion".into());
    }
    if yaw == 0. && pitch == 0. && zoom == 1. {
        return Ok(());
    }
    let offset = camera.eye - camera.target;
    let distance = offset.length();
    let azimuth = offset.x.atan2(offset.z) + yaw;
    let elevation = ((offset.y / distance).clamp(-1., 1.).asin() + pitch).clamp(-1.55, 1.55);
    let distance = (distance * zoom).clamp(camera.near * 2., camera.far * 0.8);
    let mut next = Bookmark::capture(camera).camera();
    next.eye = camera.target
        + Vec3::new(
            elevation.cos() * azimuth.sin(),
            elevation.sin(),
            elevation.cos() * azimuth.cos(),
        ) * distance;
    next.up = Vec3::Y;
    next.validate()?;
    *camera = next;
    Ok(())
}

pub fn pan(camera: &mut Camera, horizontal: f32, vertical: f32) -> Result<(), String> {
    if !horizontal.is_finite() || !vertical.is_finite() {
        return Err("Invalid camera pan".into());
    }
    let direction = (camera.target - camera.eye).normalize();
    let right = direction.cross(camera.up).normalize();
    let up = right.cross(direction).normalize();
    let displacement = (right * horizontal + up * vertical) * (camera.eye - camera.target).length();
    let mut next = Bookmark::capture(camera).camera();
    next.eye += displacement;
    next.target += displacement;
    next.validate()?;
    *camera = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn top_view_orbits_without_collinear_up_and_invalid_motion_is_atomic() {
        let mut camera = Camera::fit_bounds([-1.; 3], [1.; 3]).unwrap();
        set_view(&mut camera, CameraView::Top).unwrap();
        camera.validate().unwrap();
        orbit(&mut camera, 0.2, -0.1, 0.5).unwrap();
        camera.validate().unwrap();
        let before = Bookmark::capture(&camera);
        assert!(orbit(&mut camera, f32::NAN, 0., 1.).is_err());
        assert!(pan(&mut camera, f32::INFINITY, 0.).is_err());
        assert_eq!(Bookmark::capture(&camera), before);
    }

    #[test]
    fn model_rotation_keeps_its_pivot_and_pan_preserves_the_view_vector() {
        let pivot = Vec3::new(4., 2., -3.);
        let transform = ModelTransform {
            translation: [1., 2., 3.],
            rotation_degrees: [20., 50., 10.],
            scale: 2.,
        };
        let matrix = model_matrix(transform, pivot, 40.);
        assert!((matrix.transform_point3(pivot) - pivot - Vec3::new(1., 2., 3.)).length() < 0.0001);
        let mut camera = Camera::fit_bounds([-1.; 3], [1.; 3]).unwrap();
        let offset = camera.eye - camera.target;
        pan(&mut camera, 0.2, 0.3).unwrap();
        assert!((camera.eye - camera.target - offset).length() < 0.0001);
    }

    #[test]
    fn framing_uses_preview_transform_and_rejects_stale_selection() {
        let scene = crate::synthetic();
        let fitted = fit(
            &scene,
            Mat4::from_translation(Vec3::new(7., 3., 2.)),
            Some(0),
        )
        .unwrap();
        assert_eq!(fitted.target, Vec3::new(7., 3., 2.));
        assert!(fit(&scene, Mat4::IDENTITY, Some(1)).is_err());
    }
}
