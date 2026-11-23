use super::*;
use crate::{
    FrontFace, MaterialQueue, Mesh, OriginalMaterial, RenderMaterial, Scene, Texture, Vertex,
};
use glam::Vec4;
use std::sync::Arc;

#[test]
fn viewport_pixels_and_independent_canvas_scale_have_no_aspect_fit_or_offset() {
    for [width, height] in [[13, 7], [1920, 1080], [800, 1600]] {
        let matrix = projection(width, height, [width as f32, height as f32]).unwrap();
        let top_left = matrix * Vec4::new(0., 0., 0., 1.);
        let bottom_right = matrix * Vec4::new(width as f32, height as f32, 0., 1.);
        assert!((top_left.x + 1.).abs() < 1.0e-6);
        assert!((top_left.y - 1.).abs() < 1.0e-6);
        assert!((bottom_right.x - 1.).abs() < 1.0e-6);
        assert!((bottom_right.y + 1.).abs() < 1.0e-6);
        let pixel_center = matrix * Vec4::new(0.5, 0.5, 0., 1.);
        assert!(((pixel_center.x + 1.) * width as f32 * 0.5 - 0.5).abs() < 0.0001);
        assert!(((1. - pixel_center.y) * height as f32 * 0.5 - 0.5).abs() < 0.0001);
    }
    let matrix = projection(160, 60, [80., 40.]).unwrap();
    let point = matrix * Vec4::new(20., 10., 0., 1.);
    assert_eq!((point.x + 1.) * 80., 40.);
    assert_eq!((1. - point.y) * 30., 15.);
}

#[test]
fn invalid_viewports_and_canvases_are_rejected() {
    for canvas in [
        [0., 1.],
        [1., 0.],
        [-1., 1.],
        [1., -1.],
        [f32::NAN, 1.],
        [1., f32::INFINITY],
        [f32::from_bits(1), 1.],
    ] {
        assert!(projection(8, 4, canvas).is_err(), "{canvas:?}");
    }
    assert!(projection(0, 4, [8., 4.]).is_err());
    assert!(projection(8, 0, [8., 4.]).is_err());
}

fn texture(rgba: [u8; 4]) -> Arc<Texture> {
    Arc::new(Texture {
        width: 1,
        height: 1,
        rgba: rgba.to_vec(),
        repeat: [false; 2],
    })
}

fn diffuse(color: [f32; 4]) -> RenderMaterial {
    RenderMaterial::OriginalNull {
        color,
        sort_center: [0.; 3],
        visual_z_bias: true,
    }
}

fn quad(bounds: [f32; 4], image: Arc<Texture>, material: RenderMaterial) -> Mesh {
    let [left, top, right, bottom] = bounds;
    Mesh {
        vertices: vec![
            Vertex {
                position: [left, top, 1000.],
                uv: [0., 0.],
            },
            Vertex {
                position: [right, top, -1000.],
                uv: [1., 0.],
            },
            Vertex {
                position: [left, bottom, 1000.],
                uv: [0., 1.],
            },
            Vertex {
                position: [right, bottom, -1000.],
                uv: [1., 1.],
            },
        ],
        indices: vec![0, 1, 2, 2, 1, 3],
        texture: Some(image),
        material,
    }
}

fn renderer(format: wgpu::TextureFormat) -> Renderer {
    renderer_with_filter(format, false)
}

fn renderer_with_filter(format: wgpu::TextureFormat, linear: bool) -> Renderer {
    let instance = crate::instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("ordered screen test needs a graphics adapter");
    println!(
        "ordered screen adapter: {:?}, target: {format:?}",
        adapter.get_info()
    );
    pollster::block_on(Renderer::with_adapter(
        &adapter,
        &Scene { meshes: Vec::new() },
        format,
        linear,
    ))
    .unwrap()
}

fn replace(renderer: &mut Renderer, meshes: &[Mesh]) {
    renderer
        .replace_meshes(0..renderer.draws.len(), meshes)
        .unwrap();
}

#[test]
#[ignore = "requires a graphics device"]
fn gpu_temporary_batch_keeps_scene_draws_and_survives_a_rejected_mesh() {
    let mut renderer = renderer(wgpu::TextureFormat::Rgba8Unorm);
    let shared = texture([0, 255, 0, 255]);
    let bounds = [0., 0., 4., 4.];
    replace(
        &mut renderer,
        &[quad(bounds, shared.clone(), RenderMaterial::Legacy)],
    );
    renderer.lab_hidden_draws.insert(0);
    let original = renderer.readback_ordered_screen(4, 4, [4., 4.]).unwrap();
    assert_eq!(pixel(&original, 4, [2, 2]), [0, 255, 0, 255]);

    let mut batch = renderer.ordered_screen_batch();
    batch
        .push(quad(bounds, shared.clone(), RenderMaterial::Legacy))
        .unwrap();
    assert!(Arc::ptr_eq(
        &batch.draws[0].material,
        &renderer.draws[0].material
    ));
    let mut invalid = quad(bounds, shared, RenderMaterial::Legacy);
    invalid.indices[0] = 100;
    assert_eq!(
        batch.push(invalid),
        Err("Invalid mesh triangle indices".into())
    );
    assert_eq!(batch.draws.len(), 1);
    batch
        .push(quad(
            bounds,
            texture([255, 0, 0, 128]),
            RenderMaterial::Legacy,
        ))
        .unwrap();
    assert_eq!(batch.sources.len(), 2);
    let rgba = renderer
        .readback_texture_using(4, 4, |target| batch.encode(target))
        .unwrap();
    approximately(pixel(&rgba, 4, [2, 2]), [128, 127, 0, 191]);

    drop(batch);
    assert_eq!(renderer.draws.len(), 1);
    assert!(renderer.lab_hidden_draws.contains(&0));
    assert_eq!(
        renderer.readback_ordered_screen(4, 4, [4., 4.]).unwrap(),
        original
    );
}

#[test]
#[ignore = "requires a graphics adapter"]
fn gpu_untextured_menu_quads_preserve_diffuse_color_and_alpha() {
    let renderer = renderer(wgpu::TextureFormat::Bgra8Unorm);
    let mut batch = renderer.ordered_screen_batch();
    let untextured = |bounds, material| Mesh {
        texture: None,
        ..quad(bounds, texture([0; 4]), material)
    };
    batch
        .push(untextured([0., 0., 4., 2.], diffuse([0., 1., 0., 1.])))
        .unwrap();
    batch
        .push(untextured([2., 0., 4., 2.], diffuse([1., 0., 0., 0.5])))
        .unwrap();
    assert!(batch
        .push(untextured([0., 0., 4., 2.], RenderMaterial::Legacy))
        .is_err());
    let rgba = renderer
        .readback_texture_using(4, 2, |target| batch.encode(target))
        .unwrap();
    assert_eq!(pixel(&rgba, 4, [0, 1]), [0, 255, 0, 255]);
    approximately(pixel(&rgba, 4, [3, 1]), [128, 127, 0, 191]);
}

fn pixel(rgba: &[u8], width: usize, position: [usize; 2]) -> [u8; 4] {
    let offset = (position[1] * width + position[0]) * 4;
    rgba[offset..offset + 4].try_into().unwrap()
}

fn approximately(actual: [u8; 4], expected: [u8; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!(
            (i16::from(actual) - i16::from(expected)).abs() <= 1,
            "channel {actual}, expected {expected}"
        );
    }
}

fn black_white_texture(axis: usize, repeat: bool) -> Arc<Texture> {
    let mut dimensions = [1; 2];
    dimensions[axis] = 2;
    let mut repeating = [false; 2];
    repeating[axis] = repeat;
    Arc::new(Texture {
        width: dimensions[0],
        height: dimensions[1],
        rgba: vec![0, 0, 0, 255, 255, 255, 255, 255],
        repeat: repeating,
    })
}

fn sampled_quad(
    bounds: [f32; 4],
    image: Arc<Texture>,
    material: RenderMaterial,
    uv: [f32; 2],
) -> Mesh {
    let mut mesh = quad(bounds, image, material);
    for vertex in &mut mesh.vertices {
        vertex.uv = uv;
    }
    mesh
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored"]
fn gpu_translucent_overlays_preserve_distinct_unorm_and_srgb_attachment_blending() {
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let mut renderer = renderer(format);
        let mut meshes = vec![
            quad(
                [1., 1., 7., 3.],
                texture([255, 0, 0, 128]),
                RenderMaterial::Legacy,
            ),
            quad(
                [4., 1., 10., 3.],
                texture([0, 0, 255, 128]),
                diffuse([1.; 4]),
            ),
            quad(
                [1., 4., 10., 6.],
                texture([128, 192, 64, 128]),
                diffuse([0.5, 0.25, 1., 0.5]),
            ),
        ];
        replace(&mut renderer, &meshes);
        let (red, blue_over_red, red_over_blue, tinted) = if format.is_srgb() {
            (
                [188, 0, 0, 191],
                [137, 0, 188, 159],
                [188, 0, 137, 159],
                [30, 21, 30, 207],
            )
        } else {
            (
                [128, 0, 0, 191],
                [64, 0, 128, 159],
                [128, 0, 64, 159],
                [16, 12, 16, 207],
            )
        };
        let rgba = renderer.readback_ordered_screen(11, 7, [11., 7.]).unwrap();
        assert_eq!(pixel(&rgba, 11, [0, 1]), [0, 0, 0, 255]);
        approximately(pixel(&rgba, 11, [1, 1]), red);
        approximately(pixel(&rgba, 11, [9, 1]), [0, 0, red[0], red[3]]);
        approximately(pixel(&rgba, 11, [4, 1]), blue_over_red);
        approximately(pixel(&rgba, 11, [1, 4]), tinted);
        assert!(renderer.depth.borrow().is_none());
        meshes.swap(0, 1);
        replace(&mut renderer, &meshes);
        let rgba = renderer.readback_ordered_screen(11, 7, [11., 7.]).unwrap();
        approximately(pixel(&rgba, 11, [4, 1]), red_over_blue);
        approximately(pixel(&rgba, 11, [1, 4]), tinted);
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored"]
fn gpu_linear_legacy_and_original_null_sample_raw_bytes_at_midpoints_edges_and_repeat_seams() {
    let coordinates = [-0.25, 0., 0.25, 0.375, 0.5, 0.625, 0.75, 1., 1.25];
    let clamped = [0, 0, 0, 64, 128, 191, 255, 255, 255];
    let repeated = [255, 128, 0, 64, 128, 191, 255, 128, 0];
    let inverted = [255, 255, 255, 191, 128, 64, 0, 0, 0];
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let mut renderer = renderer_with_filter(format, true);
        let mut meshes = Vec::new();
        for axis in 0..2 {
            for repeat in [false, true] {
                let image = black_white_texture(axis, repeat);
                for material in [RenderMaterial::Legacy, diffuse([1.; 4])] {
                    let row = meshes.len() / coordinates.len();
                    for (column, coordinate) in coordinates.into_iter().enumerate() {
                        let mut uv = [0.5; 2];
                        uv[axis] = coordinate;
                        meshes.push(sampled_quad(
                            [
                                column as f32,
                                row as f32,
                                (column + 1) as f32,
                                (row + 1) as f32,
                            ],
                            Arc::clone(&image),
                            material,
                            uv,
                        ));
                    }
                }
            }
        }
        replace(&mut renderer, &meshes);
        assert_eq!(renderer.materials.borrow().len(), 8);
        let owners: Vec<_> = renderer
            .draws
            .chunks_exact(coordinates.len())
            .map(|draws| Arc::clone(&draws[0].material))
            .collect();
        for (row, draws) in renderer.draws.chunks_exact(coordinates.len()).enumerate() {
            for draw in draws {
                assert!(Arc::ptr_eq(&owners[row], &draw.material));
                assert_eq!(draw.ordered_raw_group.is_some(), row % 2 == 0);
                assert_eq!(draw.material.texture.format().is_srgb(), row % 2 == 0);
                let cache = renderer.materials.borrow();
                let key = (draw.source.as_ptr() as usize, row % 2 == 0);
                assert!(Arc::ptr_eq(
                    &cache.get(&key).unwrap().material,
                    &draw.material
                ));
            }
        }
        let rgba = renderer.readback_ordered_screen(9, 8, [9., 8.]).unwrap();
        for row in 0..8 {
            let expected = if row / 2 % 2 == 0 { clamped } else { repeated };
            for (column, channel) in expected.into_iter().enumerate() {
                approximately(
                    pixel(&rgba, 9, [column, row]),
                    [channel, channel, channel, 255],
                );
            }
        }
        renderer
            .update_texture(
                0,
                &Texture {
                    width: 2,
                    height: 1,
                    rgba: vec![255, 255, 255, 255, 0, 0, 0, 255],
                    repeat: [false; 2],
                },
            )
            .unwrap();
        assert_eq!(renderer.materials.borrow().len(), 7);
        let rgba = renderer.readback_ordered_screen(9, 8, [9., 8.]).unwrap();
        for (row, owner) in owners.iter().enumerate() {
            let expected = if row == 0 {
                inverted
            } else if row / 2 % 2 == 0 {
                clamped
            } else {
                repeated
            };
            for (column, channel) in expected.into_iter().enumerate() {
                approximately(
                    pixel(&rgba, 9, [column, row]),
                    [channel, channel, channel, 255],
                );
            }
            assert!(Arc::ptr_eq(owner, &renderer.draws[row * 9].material));
        }
        renderer
            .update_render_material(4, diffuse([0.5, 1., 1., 1.]))
            .unwrap();
        let rgba = renderer.readback_ordered_screen(9, 8, [9., 8.]).unwrap();
        approximately(pixel(&rgba, 9, [4, 0]), [64, 128, 128, 255]);
        assert!(Arc::ptr_eq(&owners[0], &renderer.draws[4].material));
        replace(&mut renderer, &meshes);
        assert_eq!(renderer.materials.borrow().len(), 8);
        assert!(!Arc::ptr_eq(&owners[0], &renderer.draws[0].material));
        for (row, owner) in owners.iter().enumerate().skip(1) {
            assert!(Arc::ptr_eq(owner, &renderer.draws[row * 9].material));
        }
        let rgba = renderer.readback_ordered_screen(9, 8, [9., 8.]).unwrap();
        for (column, channel) in clamped.into_iter().enumerate() {
            approximately(
                pixel(&rgba, 9, [column, 0]),
                [channel, channel, channel, 255],
            );
        }
        assert!(renderer.depth.borrow().is_none());
    }
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored"]
fn gpu_batches_keep_input_order_rgba_diffuse_and_ignore_scene_depth_and_alpha_tests() {
    let mut renderer = renderer(wgpu::TextureFormat::Rgba8Unorm);
    renderer.set_inspection_alpha_cutout(true);
    let mut meshes = vec![
        quad(
            [0., 0., 13., 7.],
            texture([255, 0, 0, 255]),
            diffuse([1.; 4]),
        ),
        quad(
            [1., 1., 11., 6.],
            texture([0, 255, 0, 128]),
            diffuse([1.; 4]),
        ),
        quad(
            [6., 0., 13., 7.],
            texture([0, 0, 255, 128]),
            RenderMaterial::Original(OriginalMaterial {
                color: [1.; 4],
                alpha: AlphaMode::ColorKey,
                queue: MaterialQueue::Opaque,
                has_texture: true,
                color_key_reference: 255,
                two_sided: false,
                front_face: FrontFace::CounterClockwise,
                visual_z_bias: true,
                sort_center: [0., 0., -1000.],
            }),
        ),
    ];
    meshes[2].indices.reverse();
    replace(&mut renderer, &meshes);
    renderer.lab_hidden_draws.insert(1);
    let rgba = renderer.readback_ordered_screen(13, 7, [13., 7.]).unwrap();
    assert_eq!(rgba.len(), 13 * 7 * 4);
    assert_eq!(pixel(&rgba, 13, [0, 0]), [255, 0, 0, 255]);
    approximately(pixel(&rgba, 13, [1, 1]), [127, 128, 0, 191]);
    approximately(pixel(&rgba, 13, [6, 1]), [63, 64, 128, 159]);
    approximately(pixel(&rgba, 13, [12, 6]), [127, 0, 128, 191]);
    assert!(renderer.depth.borrow().is_none());
    meshes.swap(1, 2);
    replace(&mut renderer, &meshes);
    let rgba = renderer.readback_ordered_screen(13, 7, [13., 7.]).unwrap();
    approximately(pixel(&rgba, 13, [6, 1]), [63, 128, 64, 159]);

    replace(
        &mut renderer,
        &[quad(
            [1., 1., 5., 4.],
            texture([128, 192, 64, 128]),
            diffuse([0.5, 0.25, 1., 0.5]),
        )],
    );
    let rgba = renderer.readback_ordered_screen(13, 7, [13., 7.]).unwrap();
    approximately(pixel(&rgba, 13, [1, 1]), [16, 12, 16, 207]);
    assert_eq!(pixel(&rgba, 13, [0, 1]), [0, 0, 0, 255]);
    assert_eq!(pixel(&rgba, 13, [5, 1]), [0, 0, 0, 255]);
    assert_eq!(pixel(&rgba, 13, [1, 4]), [0, 0, 0, 255]);
    renderer
        .update_render_material(0, diffuse([1.; 4]))
        .unwrap();
    let rgba = renderer.readback_ordered_screen(13, 7, [13., 7.]).unwrap();
    approximately(pixel(&rgba, 13, [1, 1]), [64, 96, 32, 191]);
    replace(&mut renderer, &[]);
    let rgba = renderer.readback_ordered_screen(13, 7, [13., 7.]).unwrap();
    assert!(rgba
        .as_chunks::<4>()
        .0
        .iter()
        .all(|value| *value == [0, 0, 0, 255]));
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored"]
fn gpu_uvs_independent_scaling_and_per_encoder_projection_are_preserved() {
    let mut renderer = renderer(wgpu::TextureFormat::Rgba8Unorm);
    let image = Arc::new(Texture {
        width: 2,
        height: 2,
        rgba: vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
        repeat: [false; 2],
    });
    replace(
        &mut renderer,
        &[quad([0., 0., 4., 4.], image, diffuse([1.; 4]))],
    );
    let rgba = renderer.readback_ordered_screen(12, 4, [4., 4.]).unwrap();
    assert_eq!(pixel(&rgba, 12, [0, 0]), [255, 0, 0, 255]);
    assert_eq!(pixel(&rgba, 12, [11, 0]), [0, 255, 0, 255]);
    assert_eq!(pixel(&rgba, 12, [0, 3]), [0, 0, 255, 255]);
    assert_eq!(pixel(&rgba, 12, [11, 3]), [255, 255, 0, 255]);
    let rgba = renderer.readback_ordered_screen(12, 4, [12., 4.]).unwrap();
    assert_eq!(pixel(&rgba, 12, [3, 3]), [255, 255, 0, 255]);
    assert_eq!(pixel(&rgba, 12, [4, 3]), [0, 0, 0, 255]);

    replace(
        &mut renderer,
        &[quad([0., 0., 4., 4.], texture([255; 4]), diffuse([1.; 4]))],
    );
    let rgba = renderer
        .readback_using(8, 4, |view| {
            let first = renderer.render_ordered_screen(view, 8, 4, [4., 4.])?;
            let target = renderer.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Second ordered screen frame"),
                size: wgpu::Extent3d {
                    width: 8,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: renderer.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            let second = renderer.render_ordered_screen(
                &target.create_view(&Default::default()),
                8,
                4,
                [8., 4.],
            )?;
            renderer.queue.submit(Some(second.finish()));
            Ok(first)
        })
        .unwrap();
    assert!(rgba.iter().all(|value| *value == 255));
    assert!(renderer.readback_ordered_screen(0, 4, [8., 4.]).is_err());
    assert!(renderer
        .readback_ordered_screen(8, 4, [f32::NAN, 4.])
        .is_err());
    assert!(renderer.readback_ordered_screen(8193, 4, [8., 4.]).is_err());
}

#[test]
#[ignore = "requires a graphics adapter; run with --ignored"]
fn gpu_rgba_bgra_unorm_and_srgb_targets_share_uploaded_texture_owners() {
    for format in [
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        wgpu::TextureFormat::Bgra8UnormSrgb,
    ] {
        let mut renderer = renderer(format);
        let image = texture([128, 192, 64, 255]);
        replace(
            &mut renderer,
            &[
                quad(
                    [0., 0., 4., 4.],
                    Arc::clone(&image),
                    diffuse([0.5, 0.25, 1., 1.]),
                ),
                quad([4., 0., 8., 4.], Arc::clone(&image), RenderMaterial::Legacy),
                quad([8., 0., 12., 4.], image, diffuse([0.5, 0.25, 1., 1.])),
            ],
        );
        assert!(Arc::ptr_eq(
            &renderer.draws[0].material,
            &renderer.draws[2].material
        ));
        let rgba = renderer.readback_ordered_screen(12, 4, [12., 4.]).unwrap();
        approximately(pixel(&rgba, 12, [0, 0]), [64, 48, 64, 255]);
        approximately(pixel(&rgba, 12, [4, 0]), [128, 192, 64, 255]);
        approximately(pixel(&rgba, 12, [8, 0]), [64, 48, 64, 255]);
        let material = Arc::clone(&renderer.draws[1].material);
        renderer
            .update_render_material(1, diffuse([0.5, 0.25, 1., 1.]))
            .unwrap();
        let rgba = renderer.readback_ordered_screen(12, 4, [12., 4.]).unwrap();
        approximately(pixel(&rgba, 12, [4, 0]), [64, 48, 64, 255]);
        assert!(Arc::ptr_eq(&material, &renderer.draws[1].material));
    }
}

#[test]
#[ignore = "requires a graphics adapter and LHR_ORDERED_SCREEN_TEST_CAPTURE output path"]
fn gpu_capture_png_round_trip_matches_the_presentable_frame() {
    let path = std::env::var_os("LHR_ORDERED_SCREEN_TEST_CAPTURE")
        .expect("set a task-owned capture output path");
    let path = Path::new(&path);
    let targets = [
        (wgpu::TextureFormat::Bgra8Unorm, path.to_path_buf()),
        (
            wgpu::TextureFormat::Rgba8Unorm,
            path.with_extension("rgba-unorm.png"),
        ),
        (
            wgpu::TextureFormat::Rgba8UnormSrgb,
            path.with_extension("rgba-srgb.png"),
        ),
        (
            wgpu::TextureFormat::Bgra8UnormSrgb,
            path.with_extension("bgra-srgb.png"),
        ),
    ];
    for (_, path) in &targets {
        assert!(
            !path.exists(),
            "capture test must not overwrite an existing file"
        );
    }
    for (format, path) in targets {
        let mut renderer = renderer_with_filter(format, true);
        replace(
            &mut renderer,
            &[
                quad(
                    [1., 1., 6., 3.],
                    texture([255, 128, 64, 255]),
                    diffuse([1.; 4]),
                ),
                sampled_quad(
                    [1., 0., 3., 1.],
                    black_white_texture(0, false),
                    RenderMaterial::Legacy,
                    [0.5; 2],
                ),
                quad(
                    [4., 0., 6., 1.],
                    texture([255, 0, 0, 128]),
                    RenderMaterial::Legacy,
                ),
            ],
        );
        let expected = renderer.readback_ordered_screen(7, 4, [7., 4.]).unwrap();
        assert_eq!(pixel(&expected, 7, [0, 0]), [0, 0, 0, 255]);
        approximately(pixel(&expected, 7, [1, 1]), [255, 128, 64, 255]);
        approximately(pixel(&expected, 7, [1, 0]), [128, 128, 128, 255]);
        approximately(
            pixel(&expected, 7, [4, 0]),
            if format.is_srgb() {
                [188, 0, 0, 191]
            } else {
                [128, 0, 0, 191]
            },
        );
        renderer
            .capture_ordered_screen(7, 4, [7., 4.], &path)
            .unwrap();
        let decoder = png::Decoder::new(std::fs::File::open(&path).unwrap());
        let mut reader = decoder.read_info().unwrap();
        let mut bytes = vec![0; reader.output_buffer_size()];
        let frame = reader.next_frame(&mut bytes).unwrap();
        assert_eq!((frame.width, frame.height), (7, 4));
        assert_eq!(&bytes[..frame.buffer_size()], expected.as_slice());
    }
}
