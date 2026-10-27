//! Opt-in host presentation controls. These previews do not reconstruct native
//! weather, source light ownership, smooth vertex normals or shadow rendering.
use super::*;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Weather {
    #[default]
    Clear,
    Rain,
    Snow,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DebugView {
    /// Retain source materials and existing saved lighting.
    #[default]
    Source,
    /// Add a host directional-light preview using derivative face normals.
    Lit,
    Normals,
    Uv,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabEnvironment {
    pub enabled: bool,
    pub background: [f32; 3],
    pub ambient: f32,
    pub sun_intensity: f32,
    pub sun_azimuth_degrees: f32,
    pub sun_elevation_degrees: f32,
    pub sun_color: [f32; 3],
    pub exposure: f32,
    pub saturation: f32,
    pub fog_start: f32,
    pub fog_end: f32,
    pub fog_color: [f32; 3],
    pub fog_enabled: bool,
    pub weather: Weather,
    pub precipitation: f32,
    pub wind: f32,
    pub debug_view: DebugView,
}
impl Default for LabEnvironment {
    fn default() -> Self {
        Self {
            enabled: false,
            background: [0.025, 0.035, 0.055],
            ambient: 0.65,
            sun_intensity: 0.7,
            sun_azimuth_degrees: -35.,
            sun_elevation_degrees: 45.,
            sun_color: [1., 0.95, 0.85],
            exposure: 1.,
            saturation: 1.,
            fog_start: 20.,
            fog_end: 150.,
            fog_color: [0.55, 0.62, 0.7],
            fog_enabled: false,
            weather: Weather::Clear,
            precipitation: 0.5,
            wind: 0.15,
            debug_view: DebugView::Source,
        }
    }
}
impl LabEnvironment {
    /// Validate all fields, including disabled controls, before publication.
    pub fn validate(&self) -> Result<(), String> {
        let within =
            |value: f32, min: f32, max: f32| value.is_finite() && (min..=max).contains(&value);
        if self
            .background
            .iter()
            .chain(&self.sun_color)
            .chain(&self.fog_color)
            .any(|&v| !within(v, 0., 1.))
            || !within(self.ambient, 0., 2.)
            || !within(self.sun_intensity, 0., 4.)
            || !within(self.sun_azimuth_degrees, -180., 180.)
            || !within(self.sun_elevation_degrees, -90., 90.)
            || !within(self.exposure, 0.1, 4.)
            || !within(self.saturation, 0., 2.)
            || !within(self.fog_start, 0., 100_000.)
            || !within(self.fog_end, 0., 100_000.)
            || self.fog_end <= self.fog_start
            || !within(self.precipitation, 0., 1.)
            || !within(self.wind, -2., 2.)
        {
            return Err("Engine Lab environment is outside its supported range".into());
        }
        Ok(())
    }
}

// Vec4 fields retain the WGSL uniform alignment on every supported host.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct LabUniform {
    model: [f32; 16],
    eye_time: [f32; 4],
    sun_direction_ambient: [f32; 4],
    sun_color_intensity: [f32; 4],
    controls: [f32; 4],
    fog_color_start: [f32; 4],
    fog: [f32; 4],
    viewport_wind: [f32; 4],
}

impl Renderer {
    /// Opt-in presentation only; does not mutate the original Scene or lights.
    /// The caller supplies the world-space inspection eye and preview time.
    /// Time is wrapped for GPU precision; it does not advance the native clock.
    pub fn set_lab_environment(
        &mut self,
        environment: LabEnvironment,
        eye: [f32; 3],
        seconds: f32,
    ) -> Result<(), String> {
        environment.validate()?;
        if eye.iter().any(|v| !v.is_finite() || v.abs() > 1.0e8)
            || !seconds.is_finite()
            || !(0. ..=1.0e8).contains(&seconds)
        {
            return Err("Engine Lab camera or preview time is invalid".into());
        }
        self.lab_environment = environment;
        self.lab_eye = eye;
        self.lab_seconds = seconds;
        Ok(())
    }

    /// Standalone host preview transform only. Canonical Scene passes and all
    /// screen-space overlays ignore it. The actual model owner is unchanged.
    pub fn set_lab_model_transform(&mut self, transform: Mat4) -> Result<(), String> {
        validate_transform(transform)?;
        self.lab_model_transform = transform;
        Ok(())
    }

    /// Standalone draw visibility only. Original cull-state transitions still
    /// occur for hidden draws. Successful mesh replacement clears this set,
    /// because material-group indices can change; canonical passes ignore it.
    pub fn set_lab_hidden_draws(&mut self, hidden: &[usize]) -> Result<(), String> {
        if hidden.iter().any(|&i| i >= self.draws.len()) {
            return Err("Engine Lab hidden draw is unavailable".into());
        }
        self.lab_hidden_draws = hidden.iter().copied().collect::<HashSet<_>>();
        Ok(())
    }

    pub(super) fn lab_uniform(
        &self,
        width: u32,
        height: u32,
        active: bool,
        transform: bool,
    ) -> LabUniform {
        let e = self.lab_environment;
        let azimuth = e.sun_azimuth_degrees.to_radians();
        let elevation = e.sun_elevation_degrees.to_radians();
        LabUniform {
            model: if transform {
                self.lab_model_transform
            } else {
                Mat4::IDENTITY
            }
            .to_cols_array(),
            eye_time: [
                self.lab_eye[0],
                self.lab_eye[1],
                self.lab_eye[2],
                self.lab_seconds % 4096.,
            ],
            sun_direction_ambient: [
                elevation.cos() * azimuth.sin(),
                elevation.sin(),
                elevation.cos() * azimuth.cos(),
                e.ambient,
            ],
            sun_color_intensity: [
                e.sun_color[0],
                e.sun_color[1],
                e.sun_color[2],
                e.sun_intensity,
            ],
            controls: [
                u8::from(active && e.enabled) as f32,
                e.exposure,
                e.saturation,
                match e.debug_view {
                    DebugView::Source => 0.,
                    DebugView::Lit => 1.,
                    DebugView::Normals => 2.,
                    DebugView::Uv => 3.,
                },
            ],
            fog_color_start: [e.fog_color[0], e.fog_color[1], e.fog_color[2], e.fog_start],
            fog: [
                e.fog_end,
                u8::from(e.fog_enabled) as f32,
                match e.weather {
                    Weather::Clear => 0.,
                    Weather::Rain => 1.,
                    Weather::Snow => 2.,
                },
                e.precipitation,
            ],
            viewport_wind: [
                width as f32,
                height as f32,
                e.wind,
                u8::from(self.format.is_srgb()) as f32,
            ],
        }
    }

    pub(super) fn lab_clear_color(&self, active: bool) -> wgpu::Color {
        let rgb = if active && self.lab_environment.enabled {
            self.lab_environment.background
        } else {
            LabEnvironment::default().background
        };
        wgpu::Color {
            r: rgb[0] as f64,
            g: rgb[1] as f64,
            b: rgb[2] as f64,
            a: 1.,
        }
    }

    pub(super) fn render_lab_precipitation(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        group: &wgpu::BindGroup,
        viewport: [u32; 4],
    ) {
        let e = self.lab_environment;
        if !e.enabled || e.weather == Weather::Clear || e.precipitation == 0. {
            return;
        }
        let [left, top, right, bottom] = viewport;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Engine Lab screen-space precipitation preview"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_viewport(
            left as f32,
            top as f32,
            (right - left) as f32,
            (bottom - top) as f32,
            0.,
            1.,
        );
        pass.set_scissor_rect(left, top, right - left, bottom - top);
        pass.set_pipeline(&self.lab_weather_pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn validate_transform(transform: Mat4) -> Result<(), String> {
    let determinant = transform.determinant();
    if !transform.is_finite()
        || transform.to_cols_array().iter().any(|v| v.abs() > 1.0e8)
        || transform.x_axis.w != 0.
        || transform.y_axis.w != 0.
        || transform.z_axis.w != 0.
        || transform.w_axis.w != 1.
        || !determinant.is_finite()
        || determinant <= 1.0e-8
    {
        return Err("Engine Lab transform must be finite, affine and preserve orientation".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn environment_ranges_reject_nonfinite_and_invalid_disabled_values() {
        let defaults = LabEnvironment::default();
        defaults.validate().unwrap();
        assert!(!defaults.enabled);
        for value in [f32::NAN, f32::INFINITY, -1., 4.1] {
            assert!(LabEnvironment {
                exposure: value,
                ..defaults
            }
            .validate()
            .is_err());
        }
        assert!(LabEnvironment {
            fog_end: defaults.fog_start,
            ..defaults
        }
        .validate()
        .is_err());
        assert!(LabEnvironment {
            sun_color: [1.1, 0., 0.],
            ..defaults
        }
        .validate()
        .is_err());
        assert!(LabEnvironment {
            wind: -2.,
            ambient: 2.,
            sun_intensity: 4.,
            ..defaults
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn preview_transform_rejects_singular_mirrored_and_projective_matrices() {
        validate_transform(Mat4::IDENTITY).unwrap();
        validate_transform(Mat4::from_scale_rotation_translation(
            Vec3::splat(2.),
            glam::Quat::from_rotation_y(0.5),
            Vec3::X,
        ))
        .unwrap();
        for matrix in [
            Mat4::ZERO,
            Mat4::from_scale(Vec3::new(-1., 1., 1.)),
            Mat4::perspective_rh(1., 1., 0.1, 10.),
        ] {
            assert!(validate_transform(matrix).is_err());
        }
    }
}

#[cfg(test)]
mod gpu_tests {
    use super::*;
    fn quad(color: [u8; 4], size: f32) -> Mesh {
        Mesh {
            vertices: [
                ([-size, -size, 0.], [0., 1.]),
                ([size, -size, 0.], [1., 1.]),
                ([size, size, 0.], [1., 0.]),
                ([-size, size, 0.], [0., 0.]),
            ]
            .map(|(position, uv)| Vertex { position, uv })
            .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(Arc::new(Texture {
                width: 1,
                height: 1,
                rgba: color.to_vec(),
                repeat: [false; 2],
            })),
            material: RenderMaterial::Original(OriginalMaterial {
                color: [1.; 4],
                alpha: AlphaMode::Opaque,
                queue: MaterialQueue::Opaque,
                has_texture: true,
                color_key_reference: 0,
                two_sided: true,
                front_face: FrontFace::CounterClockwise,
                sort_center: [0.; 3],
                visual_z_bias: false,
            }),
        }
    }
    fn camera() -> Camera {
        Camera {
            eye: Vec3::new(0., 0., 3.),
            target: Vec3::ZERO,
            up: Vec3::Y,
            near: 0.1,
            far: 100.,
            vertical_fov_radians: 50f32.to_radians(),
        }
    }
    fn renderer(scene: &Scene, format: wgpu::TextureFormat) -> Renderer {
        let adapter = pollster::block_on(instance().request_adapter(&Default::default()))
            .expect("GPU preview test requires an adapter");
        pollster::block_on(Renderer::with_adapter(&adapter, scene, format, false)).unwrap()
    }
    fn center(bytes: &[u8]) -> &[u8] {
        &bytes[(32 * 64 + 32) * 4..(32 * 64 + 32) * 4 + 4]
    }
    fn close(actual: &[u8], expected: [u8; 4]) {
        for (a, b) in actual.iter().zip(expected) {
            assert!(a.abs_diff(b) <= 2, "{actual:?} != {expected:?}");
        }
    }

    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn opt_in_preview_preserves_source_and_exercises_light_fog_debug_and_weather() {
        let scene = Scene {
            meshes: vec![quad([128, 64, 32, 255], 1.)],
        };
        let view = camera();
        for format in [
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureFormat::Rgba8UnormSrgb,
        ] {
            let mut renderer = renderer(&scene, format);
            let original = renderer.readback(64, 64, &view).unwrap();
            close(center(&original), [128, 64, 32, 255]);
            let defaults = LabEnvironment::default();
            renderer
                .set_lab_environment(defaults, view.eye.to_array(), 0.)
                .unwrap();
            assert_eq!(renderer.readback(64, 64, &view).unwrap(), original);
            let source = LabEnvironment {
                enabled: true,
                ..defaults
            };
            renderer
                .set_lab_environment(source, view.eye.to_array(), 0.)
                .unwrap();
            assert_eq!(renderer.readback(64, 64, &view).unwrap(), original);
            renderer
                .set_lab_environment(
                    LabEnvironment {
                        debug_view: DebugView::Lit,
                        ambient: 0.25,
                        sun_intensity: 0.,
                        ..source
                    },
                    view.eye.to_array(),
                    0.,
                )
                .unwrap();
            close(
                center(&renderer.readback(64, 64, &view).unwrap()),
                [32, 16, 8, 255],
            );
            renderer
                .set_lab_environment(
                    LabEnvironment {
                        fog_enabled: true,
                        fog_start: 0.,
                        fog_end: 1.,
                        fog_color: [0.2, 0.4, 0.6],
                        ..source
                    },
                    view.eye.to_array(),
                    0.,
                )
                .unwrap();
            close(
                center(&renderer.readback(64, 64, &view).unwrap()),
                [51, 102, 153, 255],
            );
            renderer
                .set_lab_environment(
                    LabEnvironment {
                        debug_view: DebugView::Normals,
                        ..source
                    },
                    view.eye.to_array(),
                    0.,
                )
                .unwrap();
            close(
                center(&renderer.readback(64, 64, &view).unwrap()),
                [128, 128, 255, 255],
            );
            renderer
                .set_lab_environment(
                    LabEnvironment {
                        debug_view: DebugView::Uv,
                        ..source
                    },
                    view.eye.to_array(),
                    0.,
                )
                .unwrap();
            let uv = renderer.readback(64, 64, &view).unwrap();
            assert!(center(&uv)[0] > 100 && center(&uv)[0] < 155);
            assert!(center(&uv)[1] > 100 && center(&uv)[1] < 155);
            assert_eq!(&center(&uv)[2..], &[64, 255]);
            for weather in [Weather::Rain, Weather::Snow] {
                let e = LabEnvironment {
                    weather,
                    precipitation: 1.,
                    ..source
                };
                renderer
                    .set_lab_environment(e, view.eye.to_array(), 0.)
                    .unwrap();
                let first = renderer.readback(64, 64, &view).unwrap();
                renderer
                    .set_lab_environment(e, view.eye.to_array(), 0.75)
                    .unwrap();
                let second = renderer.readback(64, 64, &view).unwrap();
                assert_ne!(first, original);
                assert_ne!(first, second);
            }
        }
    }

    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn preview_visibility_transform_and_overlay_have_separate_ownership() {
        let scene = Scene {
            meshes: vec![quad([255, 0, 0, 255], 1.), quad([0, 255, 0, 255], 1.)],
        };
        let mut renderer = renderer(&scene, wgpu::TextureFormat::Rgba8Unorm);
        let view = camera();
        renderer.set_lab_hidden_draws(&[0, 1]).unwrap();
        let hidden = renderer.readback(64, 64, &view).unwrap();
        close(center(&hidden), [6, 9, 14, 255]);
        assert!(renderer.set_lab_hidden_draws(&[2]).is_err());
        assert_eq!(renderer.lab_hidden_draws.len(), 2);
        renderer
            .replace_meshes(0..1, &[quad([255, 0, 0, 255], 1.)])
            .unwrap();
        assert!(renderer.lab_hidden_draws.is_empty());
        renderer.set_lab_hidden_draws(&[1]).unwrap();
        close(
            center(&renderer.readback(64, 64, &view).unwrap()),
            [255, 0, 0, 255],
        );
        renderer
            .set_lab_model_transform(Mat4::from_translation(Vec3::new(10., 0., 0.)))
            .unwrap();
        close(
            center(&renderer.readback(64, 64, &view).unwrap()),
            [6, 9, 14, 255],
        );
        let environment = LabEnvironment {
            enabled: true,
            debug_view: DebugView::Normals,
            weather: Weather::Rain,
            precipitation: 1.,
            ..Default::default()
        };
        renderer
            .set_lab_environment(environment, view.eye.to_array(), 1.)
            .unwrap();
        let screen_with_preview = renderer.readback_screen(64, 64, [2., 2.]).unwrap();
        renderer
            .set_lab_environment(LabEnvironment::default(), view.eye.to_array(), 0.)
            .unwrap();
        let screen_without_preview = renderer.readback_screen(64, 64, [2., 2.]).unwrap();
        assert_eq!(screen_with_preview, screen_without_preview);
        // A final design-canvas mesh is painted after weather and is neither
        // transformed nor hidden by standalone object inspection.
        let overlay_scene = Scene {
            meshes: vec![
                quad([255, 0, 0, 255], 1.),
                Mesh {
                    vertices: [[0., 0., 0.], [2., 0., 0.], [2., 2., 0.], [0., 2., 0.]]
                        .map(|position| Vertex {
                            position,
                            uv: [0.; 2],
                        })
                        .to_vec(),
                    indices: vec![0, 1, 2, 0, 2, 3],
                    material: RenderMaterial::Legacy,
                    texture: Some(Arc::new(Texture {
                        width: 1,
                        height: 1,
                        rgba: vec![0, 255, 0, 255],
                        repeat: [false; 2],
                    })),
                },
            ],
        };
        renderer
            .replace_meshes(0..2, &overlay_scene.meshes)
            .unwrap();
        renderer
            .set_lab_environment(environment, view.eye.to_array(), 1.)
            .unwrap();
        let rgba = renderer
            .readback_matrix(64, 64, view.matrix(1.), Some([2., 2.]))
            .unwrap();
        close(center(&rgba), [0, 255, 0, 255]);
        let before = renderer.lab_environment;
        assert!(renderer
            .set_lab_environment(
                LabEnvironment {
                    exposure: f32::NAN,
                    ..environment
                },
                view.eye.to_array(),
                1.
            )
            .is_err());
        assert_eq!(renderer.lab_environment, before);
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn canonical_backdrop_and_membership_ignore_standalone_inspection_edits() {
        let scene = Scene {
            meshes: vec![quad([0, 0, 255, 255], 1.), quad([255, 0, 0, 255], 1.)],
        };
        let mut renderer = renderer(&scene, wgpu::TextureFormat::Rgba8Unorm);
        let frame = SceneRenderFrame {
            viewport: [8, 8, 56, 56],
            initial_cull: None,
            backdrop: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![0],
            },
            main: SceneRenderPass {
                view_projection: Mat4::from_translation(Vec3::new(0.5, 0., 0.))
                    * Mat4::from_scale(Vec3::new(0.4, 0.6, 1.)),
                draws: vec![1],
            },
            near: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
        };
        let source = renderer.readback_scene_frame(64, 64, &frame).unwrap();
        renderer
            .set_lab_model_transform(Mat4::from_translation(Vec3::new(10., 0., 0.)))
            .unwrap();
        renderer.set_lab_hidden_draws(&[0, 1]).unwrap();
        assert_eq!(
            renderer.readback_scene_frame(64, 64, &frame).unwrap(),
            source
        );
        renderer
            .set_lab_environment(
                LabEnvironment {
                    enabled: true,
                    fog_enabled: true,
                    fog_start: 0.,
                    fog_end: 1.,
                    fog_color: [1., 1., 0.],
                    background: [0.2, 0.3, 0.4],
                    ..Default::default()
                },
                [0., 0., 3.],
                0.,
            )
            .unwrap();
        let preview = renderer.readback_scene_frame(64, 64, &frame).unwrap();
        let pixel = |x: usize, y: usize| &preview[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        close(pixel(16, 32), [0, 0, 255, 255]);
        close(pixel(44, 32), [255, 255, 0, 255]);
        close(pixel(2, 32), [51, 77, 102, 255]);
    }
}
