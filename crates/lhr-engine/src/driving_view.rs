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

/// Actual live-car presentation replacement. World draws follow the car range
/// and survive LOD face-group count changes, total culling and later re-entry.
pub fn refresh_car_draws(
    renderer: &mut lhr_render::Renderer,
    scene: &mut lhr_render::Scene,
    car_draws: &mut usize,
    next: Vec<Mesh>,
) -> Result<(), String> {
    if *car_draws > scene.meshes.len() {
        return Err("Car draw range exceeds current scene".into());
    }
    let (old_range, new_range) = mesh_replacement_ranges(&scene.meshes[..*car_draws], &next);
    if !old_range.is_empty() || !new_range.is_empty() {
        renderer.replace_meshes(old_range, &next[new_range.clone()])?;
    }
    for (index, mesh) in next.iter().enumerate() {
        if !new_range.contains(&index) {
            renderer.update_mesh_vertices(index, &mesh.vertices)?;
            renderer.update_render_material(index, mesh.material)?;
        }
    }
    let count = next.len();
    scene.meshes.splice(0..*car_draws, next);
    *car_draws = count;
    Ok(())
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
    camera.validate()?;
    let scene = owner.scene().ok_or("Canonical audio Scene absent")?;
    let (position, right) = lhr_engine::scene_render::active_listener(owner.arena_mut(), scene)?;
    owner.driving_set_audio_listener(position, right)
}

#[cfg(test)]
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

/// Existing inspection/chase controls author a real Scene camera frame. The
/// resulting camera, not the host control object, owns view/projection and LOD.
pub fn control_scene_camera(
    owner: &mut lhr_engine::car_runtime::CarRuntime,
    control: &Camera,
    viewport: [u32; 2],
) -> Result<(), String> {
    use lhr_engine::scene_resources::SceneScreen;
    struct Screen([u32; 2]);
    impl SceneScreen for Screen {
        type Error = ();
        fn screen_sx(&mut self) -> Result<i32, ()> {
            i32::try_from(self.0[0]).map_err(|_| ())
        }
        fn screen_sy(&mut self) -> Result<i32, ()> {
            i32::try_from(self.0[1]).map_err(|_| ())
        }
    }
    control.validate()?;
    if viewport.contains(&0) {
        return Err("Scene camera target is empty".into());
    }
    let scene = owner.scene().ok_or("Canonical car Scene absent")?;
    let arena = owner.arena_mut();
    let active = arena
        .scene_active_camera(scene)
        .map_err(|e| e.to_string())?;
    let camera = match active {
        Some(camera) => camera,
        None => {
            let camera = arena
                .create_camera(&mut Screen(viewport))
                .map_err(|e| format!("Scene camera construction: {e:?}"))?;
            arena
                .scene_set_active_camera(scene, Some(camera))
                .map_err(|e| e.to_string())?;
            // Scene retains this camera; release only the factory caller ref.
            arena.release(camera.frame()).map_err(|e| e.to_string())?;
            camera
        }
    };
    let direction = control.target - control.eye;
    arena
        .camera_set_position(camera, [control.eye.x, control.eye.y, -control.eye.z])
        .map_err(|e| e.to_string())?;
    arena
        .camera_set_direction(camera, [direction.x, direction.y, -direction.z], 0.)
        .map_err(|e| e.to_string())?;
    arena
        .camera_set_range(camera, control.near, control.far)
        .map_err(|e| e.to_string())?;
    arena
        .camera_set_aspect_ratio(camera, viewport[0] as f32 / viewport[1] as f32)
        .map_err(|e| e.to_string())?;
    arena
        .camera_set_fov(camera, control.horizontal_fov_radians(viewport)?)
        .map_err(|e| e.to_string())?;
    arena.camera_update(camera).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn snapshot_scene(
    owner: &mut lhr_engine::car_runtime::CarRuntime,
    control: &Camera,
    viewport: [u32; 2],
    render_number: u32,
) -> Result<lhr_engine::scene_render::CanonicalSceneSnapshot, String> {
    if owner.has_canonical_scene_world() {
        let model = owner.model().ok_or("Canonical driving model absent")?;
        let world = owner
            .arena_mut()
            .world_matrix(model.root())
            .map_err(|e| e.to_string())?;
        owner.refresh_scene_city_cache(world.elements[12..15].try_into().unwrap())?;
    }
    owner.set_scene_viewport([0, 0, viewport[0] as i32, viewport[1] as i32])?;
    control_scene_camera(owner, control, viewport)?;
    let scene = owner.scene().ok_or("Canonical car Scene absent")?;
    let rectangle = owner
        .scene_viewport()
        .ok_or("Canonical car viewport absent")?;
    lhr_engine::scene_render::snapshot_scene(
        owner.arena_mut(),
        scene,
        rectangle,
        viewport,
        render_number,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn triangle(color: [u8; 4]) -> Mesh {
        Mesh {
            material: Default::default(),
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
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn actual_refresh_keeps_world_draws_across_lod_count_change_cull_and_reentry() {
        use lhr_render::{Renderer, Scene, Texture, Vertex};
        use std::sync::Arc;
        let quad = |color: [u8; 4], x0: f32, x1: f32, y0: f32, y1: f32| Mesh {
            material: Default::default(),
            vertices: [[x0, y0, 0.], [x1, y0, 0.], [x1, y1, 0.], [x0, y1, 0.]]
                .map(|position| Vertex {
                    position,
                    uv: [0.; 2],
                })
                .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(Arc::new(Texture {
                width: 1,
                height: 1,
                rgba: Vec::from(color),
                repeat: [false; 2],
            })),
        };
        let world = quad([0, 0, 255, 255], 1., 2., 0., 2.);
        let world_texture = world.texture.clone().unwrap();
        let mut scene = Scene {
            meshes: vec![quad([255, 0, 0, 255], 0., 1., 0., 2.), world],
        };
        let instance = lhr_render::instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let mut count = 1;
        let mut check = |next: Vec<Mesh>, expected: [u8; 4]| {
            let target_count = next.len();
            refresh_car_draws(&mut renderer, &mut scene, &mut count, next).unwrap();
            assert_eq!(count, target_count);
            assert_eq!(scene.meshes.len(), count + 1);
            assert!(Arc::ptr_eq(
                scene.meshes.last().unwrap().texture.as_ref().unwrap(),
                &world_texture
            ));
            let rgba = renderer.readback_screen(32, 32, [2., 2.]).unwrap();
            assert_eq!(
                &rgba[(8 * 32 + 24) * 4..(8 * 32 + 24) * 4 + 4],
                &[0, 0, 255, 255]
            );
            assert_eq!(&rgba[(8 * 32 + 8) * 4..(8 * 32 + 8) * 4 + 4], &expected);
        };
        check(
            vec![
                quad([0, 255, 0, 255], 0., 1., 0., 1.),
                quad([255, 255, 0, 255], 0., 1., 1., 2.),
            ],
            [0, 255, 0, 255],
        );
        check(Vec::new(), [6, 9, 14, 255]);
        check(
            vec![quad([255, 0, 0, 255], 0., 1., 0., 2.)],
            [255, 0, 0, 255],
        );
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
