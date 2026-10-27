//! Camera matrices and draw membership supplied by the canonical engine Scene.
use super::*;

#[derive(Clone, Debug)]
pub struct SceneRenderPass {
    /// Column-major GPU matrix. The engine owns source-camera evaluation.
    pub view_projection: Mat4,
    pub draws: Vec<usize>,
}
#[derive(Clone, Debug)]
pub struct SceneRenderFrame {
    /// Original Scene viewport rectangle boundaries, in target pixels.
    pub viewport: [u32; 4],
    /// Explicit presentation driver prestate for null-material draws. None
    /// means culling disabled. Native driver initialization is a separate owner.
    pub initial_cull: Option<FrontFace>,
    pub backdrop: SceneRenderPass,
    pub main: SceneRenderPass,
    pub near: SceneRenderPass,
}
impl Renderer {
    /// Source order: backdrop without depth, ordinary sectors with depth,
    /// then near sector after clearing depth while retaining scene color.
    pub fn render_scene_frame(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        frame: &SceneRenderFrame,
    ) -> Result<wgpu::CommandEncoder, String> {
        let [left, top, right, bottom] = frame.viewport;
        if left >= right || top >= bottom || right > width || bottom > height {
            return Err("Scene viewport cannot be represented by the GPU target".into());
        }
        for layer in [&frame.backdrop, &frame.main, &frame.near] {
            if !layer.view_projection.is_finite()
                || layer.draws.iter().any(|&i| i >= self.draws.len())
            {
                return Err("Scene pass matrix or draw membership is unavailable".into());
            }
        }
        // Each pass owns an immutable matrix allocation. Queue writes into one
        // shared uniform would make all three passes observe the final matrix.
        let cameras: Vec<_> = [&frame.backdrop, &frame.main, &frame.near]
            .into_iter()
            .enumerate()
            .map(|(number, layer)| {
                let buffer = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Canonical Scene pass camera"),
                        contents: bytemuck::cast_slice(&layer.view_projection.to_cols_array()),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                // Backdrop geometry retains source color and ignores host
                // lighting, fog, debug views and model transforms.
                let environment =
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some("Canonical Scene host preview parameters"),
                            contents: bytemuck::bytes_of(&self.lab_uniform(
                                width,
                                height,
                                number != 0,
                                false,
                            )),
                            usage: wgpu::BufferUsages::UNIFORM,
                        });
                self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Canonical Scene pass camera"),
                    layout: &self.camera_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: environment.as_entire_binding(),
                        },
                    ],
                })
            })
            .collect();
        let depth = self.depth_view(width, height);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut current_cull = frame.initial_cull;
        for (number, layer) in [&frame.backdrop, &frame.main, &frame.near]
            .into_iter()
            .enumerate()
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(["Scene backdrop", "Scene main sectors", "Scene near sector"][number]),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: if number == 0 {
                            wgpu::LoadOp::Clear(self.lab_clear_color(true))
                        } else {
                            wgpu::LoadOp::Load
                        },
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
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
            pass.set_bind_group(0, &cameras[number], &[]);
            // Engine face-group selection supplies source order. Preserve that
            // membership, then apply the existing material queue policy.
            let mut ordered = layer.draws.clone();
            let all = self.draw_order(self.draws.len(), layer.view_projection);
            let rank: HashMap<_, _> = all.into_iter().enumerate().map(|(r, i)| (i, r)).collect();
            if number == 0 {
                let distance = |index: usize| match self.draws[index].policy {
                    RenderMaterial::Original(m) => Vec3::from_array(m.sort_center).length_squared(),
                    RenderMaterial::Legacy => 0.,
                    RenderMaterial::OriginalNull { sort_center, .. } => {
                        Vec3::from_array(sort_center).length_squared()
                    }
                };
                ordered.sort_by(|&a, &b| {
                    distance(b)
                        .total_cmp(&distance(a))
                        .then_with(|| rank[&a].cmp(&rank[&b]))
                });
            } else {
                ordered.sort_by_key(|i| rank[i]);
            }
            for index in ordered {
                let draw = &self.draws[index];
                match draw.policy {
                    RenderMaterial::Legacy => pass.set_pipeline(if number == 0 {
                        &self.backdrop_pipeline
                    } else {
                        &self.pipeline
                    }),
                    RenderMaterial::OriginalNull { visual_z_bias, .. } => {
                        let key = PipelineKey {
                            alpha: AlphaMode::Opaque,
                            two_sided: current_cull.is_none(),
                            front_face: current_cull.unwrap_or(FrontFace::CounterClockwise),
                            visual_z_bias,
                        };
                        pass.set_pipeline(&self.original_pipelines[&(key, number != 0)]);
                        pass.set_bind_group(2, &draw.parameters, &[]);
                        pass.set_bind_group(3, &draw.lighting_material.group, &[]);
                        pass.set_vertex_buffer(1, draw.lighting_vertex.slice(..));
                    }
                    RenderMaterial::Original(m) => {
                        current_cull = (!m.two_sided).then_some(m.front_face);
                        pass.set_pipeline(&self.original_pipelines[&(m.into(), number != 0)]);
                        pass.set_bind_group(2, &draw.parameters, &[]);
                        pass.set_bind_group(3, &draw.lighting_material.group, &[]);
                        pass.set_vertex_buffer(1, draw.lighting_vertex.slice(..));
                    }
                }
                pass.set_bind_group(1, &draw.material.group, &[]);
                pass.set_vertex_buffer(0, draw.vertex.slice(..));
                pass.set_index_buffer(draw.index.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..draw.count, 0, 0..1);
            }
        }
        self.render_lab_precipitation(&mut encoder, view, &cameras[1], frame.viewport);
        Ok(encoder)
    }
    pub fn render_scene_frame_with_overlay(
        &self,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        frame: &SceneRenderFrame,
        design: [f32; 2],
    ) -> Result<wgpu::CommandEncoder, String> {
        let mut encoder = self.render_scene_frame(view, width, height, frame)?;
        self.render_overlay(&mut encoder, view, width, height, design);
        Ok(encoder)
    }
    pub fn capture_scene_frame_with_overlay(
        &self,
        width: u32,
        height: u32,
        frame: &SceneRenderFrame,
        design: [f32; 2],
        path: &Path,
    ) -> Result<(), String> {
        let rgba = self.readback_using(width, height, |view| {
            self.render_scene_frame_with_overlay(view, width, height, frame, design)
        })?;
        write_capture(width, height, &rgba, path)
    }
    pub fn readback_scene_frame(
        &self,
        width: u32,
        height: u32,
        frame: &SceneRenderFrame,
    ) -> Result<Vec<u8>, String> {
        self.readback_using(width, height, |view| {
            self.render_scene_frame(view, width, height, frame)
        })
    }
    pub fn capture_scene_frame(
        &self,
        width: u32,
        height: u32,
        frame: &SceneRenderFrame,
        path: &Path,
    ) -> Result<(), String> {
        let rgba = self.readback_scene_frame(width, height, frame)?;
        write_capture(width, height, &rgba, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn quad(rgb: [u8; 3], z: f32) -> Mesh {
        Mesh {
            material: RenderMaterial::Original(OriginalMaterial {
                color: [1.; 4],
                alpha: AlphaMode::Opaque,
                queue: MaterialQueue::Opaque,
                has_texture: true,
                color_key_reference: 0,
                two_sided: true,
                front_face: FrontFace::CounterClockwise,
                visual_z_bias: false,
                sort_center: [0., 0., z],
            }),
            vertices: [[-1., -1., z], [1., -1., z], [1., 1., z], [-1., 1., z]]
                .map(|position| Vertex {
                    position,
                    uv: [0.; 2],
                })
                .to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            texture: Some(Arc::new(Texture {
                width: 1,
                height: 1,
                rgba: vec![rgb[0], rgb[1], rgb[2], 255],
                repeat: [false; 2],
            })),
        }
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn three_camera_passes_retain_color_disable_backdrop_depth_clear_near_and_apply_viewport() {
        let instance = instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let scene = Scene {
            meshes: vec![
                quad([0, 0, 255], 0.01),
                quad([255, 0, 0], 0.1),
                quad([0, 255, 0], 0.9),
            ],
        };
        let renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let frame = SceneRenderFrame {
            initial_cull: None,
            viewport: [8, 8, 56, 56],
            backdrop: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![0],
            },
            main: SceneRenderPass {
                view_projection: Mat4::from_translation(Vec3::new(-0.5, 0., 0.))
                    * Mat4::from_scale(Vec3::new(0.4, 0.6, 1.)),
                draws: vec![1],
            },
            near: SceneRenderPass {
                view_projection: Mat4::from_translation(Vec3::new(0.5, 0., 0.))
                    * Mat4::from_scale(Vec3::new(0.4, 0.6, 1.)),
                draws: vec![2],
            },
        };
        let rgba = renderer.readback_scene_frame(64, 64, &frame).unwrap();
        let pixel = |x: usize, y: usize| &rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert_eq!(pixel(2, 32), &[6, 9, 14, 255]);
        assert_eq!(pixel(16, 32), &[255, 0, 0, 255]);
        assert_eq!(pixel(32, 32), &[0, 0, 255, 255]);
        assert_eq!(pixel(48, 32), &[0, 255, 0, 255]);
        // Overlap near onto main: z=.9 must win after the source near clear.
        let mut overlap = frame.clone();
        overlap.near.view_projection = overlap.main.view_projection;
        let rgba = renderer.readback_scene_frame(64, 64, &overlap).unwrap();
        assert_eq!(
            &rgba[(32 * 64 + 16) * 4..(32 * 64 + 16) * 4 + 4],
            &[0, 255, 0, 255]
        );
        let mut invalid = frame;
        invalid.main.draws.push(99);
        assert!(renderer.readback_scene_frame(64, 64, &invalid).is_err());
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn saved_lightmap_uses_independent_uv_atlas_add_dynamic_clamp_and_modulate_2x() {
        let instance = instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let scene = Scene {
            meshes: vec![quad([255, 255, 255], 0.5)],
        };
        let mut renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let frame = SceneRenderFrame {
            initial_cull: None,
            viewport: [0, 0, 16, 16],
            backdrop: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
            main: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![0],
            },
            near: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
        };
        let atlas = Arc::new(Texture {
            width: 2,
            height: 1,
            rgba: vec![32, 0, 0, 255, 0, 64, 0, 255],
            repeat: [false; 2],
        });
        let mut lighting = MeshLighting {
            uv2: vec![[0.75, 0.5]; 4],
            colors: vec![[0.1, 0., 0., 1.]; 4],
            atlas: Some(atlas),
            mode: LightingMode::LightMap {
                base_texture: true,
                modulate_2x: true,
            },
        };
        renderer.set_mesh_lighting(0, &lighting).unwrap();
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        let pixel = &rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4];
        assert!(
            (pixel[0] as i32 - 51).abs() <= 1
                && (pixel[1] as i32 - 128).abs() <= 1
                && pixel[2] == 0
                && pixel[3] == 255,
            "{pixel:?}"
        );
        lighting.colors.fill([0.9, 0.9, 0.9, 1.]);
        renderer.set_mesh_lighting(0, &lighting).unwrap();
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        assert_eq!(&rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4], &[255; 4]);
        // D3D8 texture RGB is normalized bytes: gray base128 * greenLM64
        // * MODULATE2X rounds64, without sRGB decoding either sampled texture.
        renderer
            .update_texture(
                0,
                &Texture {
                    width: 1,
                    height: 1,
                    rgba: vec![128, 128, 128, 255],
                    repeat: [false; 2],
                },
            )
            .unwrap();
        lighting.colors.fill([0., 0., 0., 1.]);
        renderer.set_mesh_lighting(0, &lighting).unwrap();
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        let pixel = &rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4];
        assert!(
            pixel[0] == 0 && (pixel[1] as i32 - 64).abs() <= 1 && pixel[2] == 0 && pixel[3] == 255,
            "{pixel:?}"
        );
        let atlas_gpu = Arc::downgrade(&renderer.draws[0].lighting_material);
        lighting.atlas.take();
        renderer.clear_mesh_lighting(0).unwrap();
        assert!(renderer.lighting_materials.borrow().is_empty());
        assert!(atlas_gpu.upgrade().is_none());
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        assert_eq!(
            &rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[128, 128, 128, 255]
        );
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn null_material_draw_inherits_the_explicit_driver_cull_state() {
        let instance = instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let mut setter = quad([255, 0, 0], 0.5);
        for v in &mut setter.vertices {
            v.position[0] += 4.;
        }
        if let RenderMaterial::Original(ref mut m) = setter.material {
            m.two_sided = false;
            m.has_texture = false;
            m.color = [1., 0., 0., 1.];
        }
        setter.texture = Some(Arc::new(Texture {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
            repeat: [false; 2],
        }));
        let mut null = quad([255; 3], 0.5);
        null.material = RenderMaterial::OriginalNull {
            color: [0., 1., 0., 1.],
            sort_center: [0., 0., 0.5],
            visual_z_bias: false,
        };
        for face in null.indices.as_chunks_mut::<3>().0 {
            face.swap(1, 2);
        }
        let scene = Scene {
            meshes: vec![null, setter],
        };
        let renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        let mut frame = SceneRenderFrame {
            initial_cull: None,
            viewport: [0, 0, 16, 16],
            backdrop: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
            main: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![0],
            },
            near: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
        };
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        assert_eq!(
            &rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[0, 255, 0, 255]
        );
        // Both have native key0: Render inserts the later setter first into
        // its stable radix list, so NULL inherits its actual cull decision.
        frame.main.draws = vec![0, 1];
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        assert_eq!(
            &rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[6, 9, 14, 255]
        );
    }
    #[test]
    #[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
    fn opaque_zero_key_null_and_untextured_groups_draw_before_coplanar_texture() {
        let instance = instance();
        let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
        let textured = quad([255, 0, 0], 0.5);
        let mut null = quad([255; 3], 0.5);
        null.material = RenderMaterial::OriginalNull {
            color: [0., 0., 0., 1.],
            sort_center: [0., 0., 0.5],
            visual_z_bias: false,
        };
        // The texture is deliberately uploaded first. The source NULL key0
        // must draw first while keeping both real coplanar triangles present.
        let scene = Scene {
            meshes: vec![textured, null],
        };
        let mut renderer = pollster::block_on(Renderer::with_adapter(
            &adapter,
            &scene,
            wgpu::TextureFormat::Rgba8Unorm,
            false,
        ))
        .unwrap();
        assert_eq!(renderer.draw_order(2, Mat4::IDENTITY), vec![1, 0]);
        let frame = SceneRenderFrame {
            initial_cull: None,
            viewport: [0, 0, 16, 16],
            backdrop: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
            main: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![0, 1],
            },
            near: SceneRenderPass {
                view_projection: Mat4::IDENTITY,
                draws: vec![],
            },
        };
        let rgba = renderer.readback_scene_frame(16, 16, &frame).unwrap();
        assert_eq!(
            &rgba[(8 * 16 + 8) * 4..(8 * 16 + 8) * 4 + 4],
            &[255, 0, 0, 255]
        );
        // An ordinary material without an effective texture has the SAME
        // source key0. The source Render's reverse sort-entry construction
        // makes the later NULL group precede it within this tied zero class.
        let mut no_texture = quad([255; 3], 0.5);
        if let RenderMaterial::Original(ref mut m) = no_texture.material {
            m.has_texture = false;
        }
        renderer.replace_meshes(0..0, &[no_texture]).unwrap();
        assert_eq!(renderer.draw_order(3, Mat4::IDENTITY), vec![2, 0, 1]);
    }
}
