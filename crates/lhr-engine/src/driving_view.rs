//! Host input, fixed clock and chase view for the owned car simulation.
//! This module never integrates or changes the vehicle's body transform.
use glam::Vec3;
use lhr_render::{Camera, Mesh};
use std::{collections::HashSet, ops::Range};
use winit::keyboard::KeyCode;

pub const STEP_MS: u32 = 16;

/// Refresh the retained original static scene after each 25 source units of
/// horizontal travel, rather than rebuilding world uploads every redraw.
pub fn world_bucket(position: [f32; 3]) -> Result<[i32; 2], String> {
    if position.iter().any(|v| !v.is_finite() || v.abs() > 1.0e8) {
        return Err("Driving presentation position outside supported range".into());
    }
    Ok([
        (position[0] / 25.).floor() as i32,
        (position[2] / 25.).floor() as i32,
    ])
}

fn same_draw_storage(a: &Mesh, b: &Mesh) -> bool {
    a.vertices.len() == b.vertices.len()
        && a.indices == b.indices
        && match (&a.texture, &b.texture) {
            (None, None) => true,
            (Some(a), Some(b)) => {
                std::sync::Arc::ptr_eq(a, b)
                    || (a.width == b.width
                        && a.height == b.height
                        && a.repeat == b.repeat
                        && a.rgba == b.rgba)
            }
            _ => false,
        }
}

/// Keep compatible draws at either end. Pose changes need only vertex uploads;
/// visibility, skid creation and lamp materials replace the intervening range.
pub fn mesh_replacement_ranges(current: &[Mesh], next: &[Mesh]) -> (Range<usize>, Range<usize>) {
    let prefix = current
        .iter()
        .zip(next)
        .take_while(|(a, b)| same_draw_storage(a, b))
        .count();
    let suffix = current[prefix..]
        .iter()
        .rev()
        .zip(next[prefix..].iter().rev())
        .take_while(|(a, b)| same_draw_storage(a, b))
        .count();
    (prefix..current.len() - suffix, prefix..next.len() - suffix)
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Controls {
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub handbrake: bool,
}
impl Controls {
    pub fn from_keys(keys: &HashSet<KeyCode>) -> Self {
        Self {
            throttle: u8::from(keys.contains(&KeyCode::KeyW)) as f32,
            brake: u8::from(keys.contains(&KeyCode::KeyS)) as f32,
            steer: u8::from(keys.contains(&KeyCode::KeyD)) as f32
                - u8::from(keys.contains(&KeyCode::KeyA)) as f32,
            handbrake: keys.contains(&KeyCode::Space),
        }
    }
}

#[derive(Default)]
pub struct FixedClock {
    remaining_ms: u32,
    pub ticks: u64,
}
impl FixedClock {
    /// Original car callers cap their elapsed update at 83 ms. The host
    /// schedules bounded 16 ms simulation calls independently of redraw rate.
    pub fn advance(&mut self, elapsed_ms: u32) -> u32 {
        self.remaining_ms += elapsed_ms.min(83);
        let count = self.remaining_ms / STEP_MS;
        self.remaining_ms %= STEP_MS;
        self.ticks += u64::from(count);
        count
    }
    pub fn reset(&mut self) {
        self.remaining_ms = 0;
        self.ticks = 0;
    }
}

/// Uses points from the live native frame graph after the simulation step.
/// Handedness conversion matches `car_inspection::snapshot_car_arena`.
pub fn chase_camera(
    arena: &mut lhr_engine::model_loading::ModelArena,
    root: lhr_engine::frame_transform::FrameKey,
) -> Result<Camera, String> {
    let world = arena.world_matrix(root).map_err(|e| e.to_string())?;
    let convert = |point| -> Result<Vec3, String> {
        let [x, y, z] = world
            .try_transform_point(point)
            .map_err(|_| "Driving camera transform outside verified arithmetic domain")?;
        Ok(Vec3::new(x, y, -z))
    };
    let center = convert([0., 0.8, 0.])?;
    let forward = (convert([0., 0.8, 1.])? - center).normalize_or_zero();
    if forward.length_squared() < 0.5 {
        return Err("Driving camera has a degenerate live vehicle basis".into());
    }
    let camera = Camera {
        eye: center - forward * 8.0 + Vec3::Y * 3.2,
        target: center + forward * 2.0,
        up: Vec3::Y,
        near: 0.1,
        far: 350.0,
    };
    camera.validate()?;
    Ok(camera)
}

/// Audio keeps original PC coordinates while presentation reflects Z.
/// Derive both listener fields from the camera that will actually be drawn.
pub fn sync_audio_listener(
    owner: &mut lhr_engine::car_runtime::CarRuntime,
    camera: &Camera,
) -> Result<(), String> {
    let (position, right) = listener_pose(camera)?;
    owner.driving_set_audio_listener(position, right)
}

fn listener_pose(camera: &Camera) -> Result<([f32; 3], [f32; 3]), String> {
    camera.validate()?;
    let right = (camera.target - camera.eye)
        .normalize()
        .cross(camera.up.normalize())
        .normalize();
    Ok((
        [camera.eye.x, camera.eye.y, -camera.eye.z],
        [right.x, right.y, -right.z],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(color: [u8; 4]) -> Mesh {
        Mesh {
            vertices: vec![
                lhr_render::Vertex {
                    position: [0.; 3],
                    uv: [0.; 2]
                };
                3
            ],
            indices: vec![0, 1, 2],
            texture: Some(std::sync::Arc::new(lhr_render::Texture {
                width: 1,
                height: 1,
                rgba: color.to_vec(),
                repeat: [false; 2],
            })),
        }
    }

    #[test]
    fn live_visibility_and_material_changes_preserve_compatible_surrounding_draws() {
        let red = [255, 0, 0, 255];
        let green = [0, 255, 0, 255];
        let blue = [0, 0, 255, 255];
        let yellow = [255, 255, 0, 255];
        let current = vec![triangle(red), triangle(green), triangle(blue)];
        let hidden = vec![triangle(red), triangle(blue)];
        assert_eq!(mesh_replacement_ranges(&current, &hidden), (1..2, 1..1));
        let created = vec![
            triangle(red),
            triangle(yellow),
            triangle(green),
            triangle(blue),
        ];
        assert_eq!(mesh_replacement_ranges(&current, &created), (1..1, 1..2));
        let material = vec![triangle(red), triangle(yellow), triangle(blue)];
        assert_eq!(mesh_replacement_ranges(&current, &material), (1..2, 1..2));
        let mut pose = vec![triangle(red), triangle(green), triangle(blue)];
        pose[1].vertices[0].position = [3., 2., 1.];
        assert_eq!(mesh_replacement_ranges(&current, &pose), (3..3, 3..3));
        pose[1].indices = vec![0, 2, 1];
        assert_eq!(mesh_replacement_ranges(&current, &pose), (1..2, 1..2));
    }

    #[test]
    fn world_refresh_handles_both_sides_of_source_zero() {
        assert_eq!(world_bucket([24.99, 100., -0.01]).unwrap(), [0, -1]);
        assert_eq!(world_bucket([25., 100., 0.]).unwrap(), [1, 0]);
        assert!(world_bucket([f32::NAN, 0., 0.]).is_err());
    }

    #[test]
    fn fixed_ticks_preserve_remainder_across_frames_and_bound_stalls() {
        let mut clock = FixedClock::default();
        assert_eq!(clock.advance(7), 0);
        assert_eq!(clock.advance(10), 1);
        assert_eq!(clock.advance(15), 1);
        assert_eq!(clock.advance(10_000), 5);
        assert_eq!(clock.advance(13), 1);
        assert_eq!(clock.ticks, 8);
        clock.reset();
        assert_eq!(clock.advance(15), 0);
        assert_eq!(clock.ticks, 0);
    }

    #[test]
    fn opposite_steering_cancels_and_released_keys_produce_neutral_input() {
        let mut keys = HashSet::from([KeyCode::KeyW, KeyCode::KeyA, KeyCode::Space]);
        assert_eq!(
            Controls::from_keys(&keys),
            Controls {
                throttle: 1.,
                brake: 0.,
                steer: -1.,
                handbrake: true
            }
        );
        keys.insert(KeyCode::KeyD);
        assert_eq!(Controls::from_keys(&keys).steer, 0.);
        keys.clear();
        assert_eq!(Controls::from_keys(&keys), Controls::default());
    }

    #[test]
    fn listener_tracks_actual_camera_position_and_horizontal_orientation() {
        let camera = Camera {
            eye: Vec3::new(10., 3., -20.),
            target: Vec3::new(11., 3., -20.),
            up: Vec3::Y,
            near: 0.1,
            far: 350.,
        };
        let (position, right) = listener_pose(&camera).unwrap();
        assert_eq!(position, [10., 3., 20.]);
        assert_eq!(right, [0., 0., -1.]);
        let pc_forward = Vec3::new(1., 0., 0.);
        assert_eq!(Vec3::from_array(right).dot(pc_forward), 0.);
    }
}
