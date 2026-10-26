//! Synthetic pixel readbacks through actual software/hardware GPU pipelines.
use super::*;
fn texture(rgba: [u8; 4]) -> Arc<Texture> {
    Arc::new(Texture {
        width: 1,
        height: 1,
        rgba: rgba.to_vec(),
        repeat: [false; 2],
    })
}
fn policy(alpha: AlphaMode, queue: MaterialQueue, depth: f32) -> RenderMaterial {
    RenderMaterial::Original(OriginalMaterial {
        color: [1.; 4],
        alpha,
        queue,
        has_texture: true,
        color_key_reference: 200,
        two_sided: false,
        front_face: FrontFace::CounterClockwise,
        sort_center: [0., 0., depth],
    })
}
fn triangle(image: Arc<Texture>, depth: f32, material: RenderMaterial) -> Mesh {
    Mesh {
        vertices: vec![
            Vertex {
                position: [-0.9, -0.9, depth],
                uv: [0.; 2],
            },
            Vertex {
                position: [0.9, -0.9, depth],
                uv: [0.; 2],
            },
            Vertex {
                position: [0., 0.9, depth],
                uv: [0.; 2],
            },
        ],
        indices: vec![0, 1, 2],
        texture: Some(image),
        material,
    }
}
fn renderer() -> Renderer {
    let instance = instance();
    let adapter = pollster::block_on(instance.request_adapter(&Default::default()))
        .expect("material test needs Vulkan or Metal");
    pollster::block_on(Renderer::with_adapter(
        &adapter,
        &Scene { meshes: Vec::new() },
        wgpu::TextureFormat::Rgba8Unorm,
        false,
    ))
    .unwrap()
}
fn pixel(renderer: &Renderer) -> [u8; 4] {
    let rgba = renderer
        .readback_matrix(32, 32, Mat4::IDENTITY, None)
        .unwrap();
    rgba[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4]
        .try_into()
        .unwrap()
}
fn replace(renderer: &mut Renderer, meshes: &[Mesh]) {
    renderer
        .replace_meshes(0..renderer.draws.len(), meshes)
        .unwrap();
}
fn approximately(got: [u8; 4], expected: [u8; 4]) {
    for (got, expected) in got.into_iter().zip(expected) {
        assert!(
            (i16::from(got) - i16::from(expected)).abs() <= 1,
            "pixel {got}, expected {expected}"
        );
    }
}
#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn alpha_reference_blend_factors_queue_sorting_and_depth_are_pixels() {
    let mut renderer = renderer();
    let opaque = |depth| policy(AlphaMode::Opaque, MaterialQueue::Opaque, depth);
    let blend = |depth| policy(AlphaMode::Blend, MaterialQueue::Transparent, depth);
    let cutout = policy(AlphaMode::ColorKey, MaterialQueue::ColorKey, 0.5);
    replace(
        &mut renderer,
        &[triangle(texture([255, 255, 255, 199]), 0.5, cutout)],
    );
    let clear = pixel(&renderer);
    replace(
        &mut renderer,
        &[triangle(texture([255, 255, 255, 200]), 0.5, cutout)],
    );
    assert_eq!(pixel(&renderer), [255, 255, 255, 200]); // >=200, not legacy 0.5
    assert!(clear[0] < 10);
    replace(
        &mut renderer,
        &[
            triangle(texture([255, 0, 0, 0]), 0.2, blend(0.9)),
            triangle(texture([0, 0, 255, 255]), 0.8, opaque(0.8)),
        ],
    );
    assert_eq!(pixel(&renderer), [0, 0, 255, 255]); // blend alpha test rejects zero
                                                    // Blended draw is intentionally supplied first: opaque queue must run first.
    replace(
        &mut renderer,
        &[
            triangle(texture([255, 0, 0, 128]), 0.2, blend(0.2)),
            triangle(texture([0, 0, 255, 255]), 0.8, opaque(0.8)),
        ],
    );
    approximately(pixel(&renderer), [128, 0, 127, 191]); // BOTH color and alpha use SrcAlpha/InvSrcAlpha
    replace(
        &mut renderer,
        &[
            triangle(
                texture([255, 0, 0, 128]),
                0.2,
                policy(AlphaMode::Additive, MaterialQueue::Transparent, 0.2),
            ),
            triangle(texture([0, 255, 0, 64]), 0.8, opaque(0.8)),
        ],
    );
    approximately(pixel(&renderer), [128, 255, 0, 128]); // SrcAlpha/One, including destination alpha
                                                         // Input near first, far last: draw transparent groups in reverse view depth.
    replace(
        &mut renderer,
        &[
            triangle(texture([255, 0, 0, 128]), 0.2, blend(0.2)),
            triangle(texture([0, 255, 0, 128]), 0.7, blend(0.7)),
            triangle(texture([0, 0, 255, 255]), 0.95, opaque(0.95)),
        ],
    );
    approximately(pixel(&renderer), [128, 64, 63, 159]);
    // Object-bound centers can order overlapping face groups differently from
    // individual fragment depths. Source regular transparent draws STILL write Z.
    replace(
        &mut renderer,
        &[
            triangle(texture([255, 0, 0, 128]), 0.2, blend(0.9)),
            triangle(texture([0, 255, 0, 128]), 0.7, blend(0.1)),
            triangle(texture([0, 0, 255, 255]), 0.95, opaque(0.95)),
        ],
    );
    approximately(pixel(&renderer), [128, 0, 127, 191]);
    // Material parameters must not split the immutable GPU texture cache.
    let shared = texture([255; 4]);
    let first = triangle(Arc::clone(&shared), 0.2, opaque(0.2));
    let mut second = triangle(shared, 0.7, opaque(0.7));
    if let RenderMaterial::Original(ref mut m) = second.material {
        m.color = [0.25, 0.5, 1., 1.];
    }
    replace(&mut renderer, &[first, second]);
    assert!(Arc::ptr_eq(
        &renderer.draws[0].material,
        &renderer.draws[1].material
    ));
    renderer.set_inspection_alpha_cutout(true);
    replace(
        &mut renderer,
        &[triangle(texture([255, 255, 255, 199]), 0.5, cutout)],
    );
    assert!(pixel(&renderer)[0] < 10); // explicit original state ignores the host inspection cutoff
}
#[test]
#[ignore = "requires a Vulkan or Metal adapter; run with --ignored"]
fn source_front_camera_and_back_camera_obey_both_bridge_index_conversions() {
    let mut renderer = renderer();
    // Authored PC triangle: LH front normal -Z, viewer at -Z, D3D screen
    // clockwise front. Z reflection puts normal/viewer at +Z. The car bridge
    // retains indices; assembly/world also swap them. Both must show that front.
    for swap in [false, true] {
        let mut material = policy(AlphaMode::Opaque, MaterialQueue::Opaque, 0.);
        if let RenderMaterial::Original(ref mut m) = material {
            m.front_face = if swap {
                FrontFace::Clockwise
            } else {
                FrontFace::CounterClockwise
            };
        }
        let mut mesh = triangle(texture([255, 0, 0, 255]), 0., material);
        if swap {
            mesh.indices.swap(1, 2);
        }
        replace(&mut renderer, &[mesh]);
        let camera = |z| Camera {
            eye: Vec3::new(0., 0., z),
            target: Vec3::ZERO,
            up: Vec3::Y,
            near: 0.1,
            far: 10.,
        };
        let read = |renderer: &Renderer, camera: Camera| -> [u8; 4] {
            let rgba = renderer.readback(32, 32, &camera).unwrap();
            rgba[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4]
                .try_into()
                .unwrap()
        };
        assert_eq!(read(&renderer, camera(3.)), [255, 0, 0, 255]);
        assert!(read(&renderer, camera(-3.))[0] < 10);
        if let RenderMaterial::Original(ref mut m) = material {
            m.two_sided = true;
        }
        renderer.update_render_material(0, material).unwrap();
        assert_eq!(read(&renderer, camera(-3.)), [255, 0, 0, 255]);
    }
}
