//! Public procedural host previews for the Engine Lab, not original game content.
use lhr_render::{Mesh, RenderMaterial, Scene, Texture, Vertex};
use std::{
    f32::consts::{PI, TAU},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SyntheticPreset {
    #[default]
    Gallery,
    Courtyard,
    Primitives,
    LightStudy,
}

impl SyntheticPreset {
    #[cfg(any(test, feature = "engine-lab"))]
    pub const ALL: [Self; 4] = [
        Self::Gallery,
        Self::Courtyard,
        Self::Primitives,
        Self::LightStudy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gallery => "Material gallery",
            Self::Courtyard => "Outdoor courtyard",
            Self::Primitives => "Geometry playground",
            Self::LightStudy => "Light and fog study",
        }
    }

    pub fn build(self) -> Result<Scene, String> {
        let scene = Scene {
            meshes: self.objects().into_iter().map(|(_, mesh)| mesh).collect(),
        };
        validate(&scene)?;
        Ok(scene)
    }

    /// Labels follow draw order exactly; these are host preview objects.
    pub fn mesh_labels(self) -> Vec<String> {
        self.objects().into_iter().map(|(name, _)| name).collect()
    }

    fn objects(self) -> Vec<(String, Mesh)> {
        let stone = checker([150, 159, 170], [91, 105, 119]);
        let warm = checker([226, 151, 91], [158, 89, 62]);
        let blue = checker([74, 147, 183], [42, 85, 131]);
        let green = checker([107, 155, 103], [58, 99, 74]);
        let pale = checker([224, 225, 218], [186, 193, 195]);
        let mut objects = vec![(
            "Ground / tiled reference".into(),
            cuboid([0., -0.3, 0.], [24., 0.6, 24.], stone.clone(), 12.),
        )];
        match self {
            Self::Gallery => {
                objects.push((
                    "Gallery rear wall".into(),
                    cuboid([0., 2.8, -6.5], [17., 5.6, 0.45], pale.clone(), 5.),
                ));
                objects.push((
                    "Gallery left wall".into(),
                    cuboid([-8.5, 2.8, -1.], [0.45, 5.6, 11.], stone.clone(), 5.),
                ));
                for (i, texture) in [warm.clone(), blue.clone(), green.clone()]
                    .into_iter()
                    .enumerate()
                {
                    let x = (i as f32 - 1.) * 4.8;
                    objects.push((
                        format!("Display plinth {}", i + 1),
                        cuboid([x, 0.4, 0.], [3., 0.8, 3.], pale.clone(), 1.),
                    ));
                    let mesh = match i {
                        0 => cuboid([x, 1.9, 0.], [2., 2.2, 2.], texture, 2.),
                        1 => sphere([x, 1.9, 0.], 1.1, texture),
                        _ => cylinder([x, 1.9, 0.], 1., 2.2, texture),
                    };
                    objects.push((format!("Material sample {}", i + 1), mesh));
                }
                objects.push((
                    "Checker test panel".into(),
                    cuboid([0., 3.6, -6.15], [5., 2., 0.1], blue, 8.),
                ));
            }
            Self::Courtyard => {
                objects.push((
                    "Courtyard rear facade".into(),
                    cuboid([0., 3., -8.], [18., 6., 1.], warm.clone(), 8.),
                ));
                objects.push((
                    "Courtyard west facade".into(),
                    cuboid([-8.5, 2.5, 0.], [1., 5., 16.], pale.clone(), 7.),
                ));
                objects.push((
                    "Courtyard east facade".into(),
                    cuboid([8.5, 2.5, 0.], [1., 5., 16.], pale.clone(), 7.),
                ));
                objects.push((
                    "Fountain base".into(),
                    cylinder([0., 0.25, 0.], 2.4, 0.5, stone.clone()),
                ));
                objects.push((
                    "Fountain column".into(),
                    cylinder([0., 1.45, 0.], 0.4, 2.4, pale.clone()),
                ));
                objects.push((
                    "Fountain crown".into(),
                    sphere([0., 2.7, 0.], 0.65, blue.clone()),
                ));
                for (i, x) in [-5.5, 5.5].into_iter().enumerate() {
                    objects.push((
                        format!("Planter {}", i + 1),
                        cuboid([x, 0.45, 2.], [2.2, 0.9, 2.2], warm.clone(), 2.),
                    ));
                    objects.push((
                        format!("Topiary {}", i + 1),
                        sphere([x, 1.8, 2.], 1.1, green.clone()),
                    ));
                    objects.push((
                        format!("Arch pillar {}", i + 1),
                        cuboid([x * 0.48, 1.5, -4.5], [0.7, 3., 0.9], stone.clone(), 2.),
                    ));
                }
                objects.push((
                    "Arch / curved lintel".into(),
                    arch([0., 3., -4.5], 2.3, 3., 0.9, stone),
                ));
            }
            Self::Primitives => {
                for (i, texture) in [warm.clone(), blue.clone(), green.clone()]
                    .into_iter()
                    .enumerate()
                {
                    let x = (i as f32 - 1.) * 5.;
                    objects.push((
                        format!("Box {}", i + 1),
                        cuboid([x, 1.25, -3.5], [2.5, 2.5, 2.5], texture.clone(), 2.),
                    ));
                    objects.push((
                        format!("Sphere {}", i + 1),
                        sphere([x, 1.35, 1.], 1.35, texture.clone()),
                    ));
                    objects.push((
                        format!("Cylinder {}", i + 1),
                        cylinder([x, 1.6, 5.5], 0.95, 3.2, texture),
                    ));
                }
                for i in 0..5 {
                    objects.push((
                        format!("Stair tread {}", i + 1),
                        cuboid(
                            [-8.8, 0.2 + i as f32 * 0.4, 5. - i as f32 * 1.],
                            [2., 0.4 + i as f32 * 0.8, 1.],
                            pale.clone(),
                            1.,
                        ),
                    ));
                }
            }
            Self::LightStudy => {
                objects.push((
                    "Neutral reference wall".into(),
                    cuboid([0., 3.5, -9.5], [22., 7., 0.5], pale.clone(), 8.),
                ));
                for i in 0..5 {
                    let x = (i as f32 - 2.) * 3.4;
                    let z = 6. - i as f32 * 3.;
                    objects.push((
                        format!("Depth marker {} / sphere", i + 1),
                        sphere([x, 1.4, z], 1.4, pale.clone()),
                    ));
                    objects.push((
                        format!("Depth marker {} / column", i + 1),
                        cylinder(
                            [x, 2.2, z - 1.8],
                            0.3,
                            4.4,
                            if i % 2 == 0 {
                                warm.clone()
                            } else {
                                blue.clone()
                            },
                        ),
                    ));
                }
                objects.push((
                    "Warm reference block".into(),
                    cuboid([-7.5, 0.7, 7.5], [2., 1.4, 2.], warm, 2.),
                ));
                objects.push((
                    "Cool reference block".into(),
                    cuboid([7.5, 0.7, 7.5], [2., 1.4, 2.], blue, 2.),
                ));
            }
        }
        objects
    }
}

fn checker(a: [u8; 3], b: [u8; 3]) -> Arc<Texture> {
    let mut rgba = Vec::with_capacity(8 * 8 * 4);
    for y in 0..8 {
        for x in 0..8 {
            rgba.extend_from_slice(if (x / 4 + y / 4) % 2 == 0 { &a } else { &b });
            rgba.push(255);
        }
    }
    Arc::new(Texture {
        width: 8,
        height: 8,
        rgba,
        repeat: [true; 2],
    })
}

fn empty(texture: Arc<Texture>) -> Mesh {
    Mesh {
        vertices: Vec::new(),
        indices: Vec::new(),
        texture: Some(texture),
        material: RenderMaterial::Legacy,
    }
}

fn quad(mesh: &mut Mesh, positions: [[f32; 3]; 4], tiles: f32) {
    let start = mesh.vertices.len() as u32;
    for (position, uv) in
        positions
            .into_iter()
            .zip([[0., 0.], [tiles, 0.], [tiles, tiles], [0., tiles]])
    {
        mesh.vertices.push(Vertex { position, uv });
    }
    mesh.indices
        .extend([start, start + 1, start + 2, start, start + 2, start + 3]);
}

fn cuboid(center: [f32; 3], size: [f32; 3], texture: Arc<Texture>, tiles: f32) -> Mesh {
    let mut mesh = empty(texture);
    let [x, y, z] = center;
    let [hx, hy, hz] = size.map(|v| v * 0.5);
    let p = [
        [x - hx, y - hy, z - hz],
        [x + hx, y - hy, z - hz],
        [x + hx, y + hy, z - hz],
        [x - hx, y + hy, z - hz],
        [x - hx, y - hy, z + hz],
        [x + hx, y - hy, z + hz],
        [x + hx, y + hy, z + hz],
        [x - hx, y + hy, z + hz],
    ];
    for face in [
        [0, 3, 2, 1],
        [4, 5, 6, 7],
        [0, 4, 7, 3],
        [1, 2, 6, 5],
        [3, 7, 6, 2],
        [0, 1, 5, 4],
    ] {
        quad(&mut mesh, face.map(|i| p[i]), tiles);
    }
    mesh
}

fn sphere(center: [f32; 3], radius: f32, texture: Arc<Texture>) -> Mesh {
    const RINGS: usize = 12;
    const SEGMENTS: usize = 24;
    let mut mesh = empty(texture);
    for ring in 0..=RINGS {
        let latitude = PI * ring as f32 / RINGS as f32;
        for segment in 0..=SEGMENTS {
            let longitude = TAU * segment as f32 / SEGMENTS as f32;
            mesh.vertices.push(Vertex {
                position: [
                    center[0] + radius * latitude.sin() * longitude.cos(),
                    center[1] + radius * latitude.cos(),
                    center[2] + radius * latitude.sin() * longitude.sin(),
                ],
                uv: [
                    segment as f32 / SEGMENTS as f32 * 4.,
                    ring as f32 / RINGS as f32 * 2.,
                ],
            });
        }
    }
    for ring in 0..RINGS {
        for segment in 0..SEGMENTS {
            let a = (ring * (SEGMENTS + 1) + segment) as u32;
            let b = a + (SEGMENTS + 1) as u32;
            if ring != 0 {
                mesh.indices.extend([a, a + 1, b]);
            }
            if ring + 1 != RINGS {
                mesh.indices.extend([a + 1, b + 1, b]);
            }
        }
    }
    mesh
}

fn cylinder(center: [f32; 3], radius: f32, height: f32, texture: Arc<Texture>) -> Mesh {
    const SEGMENTS: usize = 24;
    let mut mesh = empty(texture);
    for i in 0..SEGMENTS {
        let a = TAU * i as f32 / SEGMENTS as f32;
        let b = TAU * (i + 1) as f32 / SEGMENTS as f32;
        let lower = |angle: f32| {
            [
                center[0] + radius * angle.cos(),
                center[1] - height * 0.5,
                center[2] + radius * angle.sin(),
            ]
        };
        let upper = |angle: f32| {
            [
                center[0] + radius * angle.cos(),
                center[1] + height * 0.5,
                center[2] + radius * angle.sin(),
            ]
        };
        quad(&mut mesh, [lower(a), upper(a), upper(b), lower(b)], 1.);
        for positions in [
            [
                upper(a),
                [center[0], center[1] + height * 0.5, center[2]],
                upper(b),
            ],
            [
                lower(b),
                [center[0], center[1] - height * 0.5, center[2]],
                lower(a),
            ],
        ] {
            let start = mesh.vertices.len() as u32;
            for (position, uv) in positions.into_iter().zip([[0., 0.], [0.5, 0.5], [1., 0.]]) {
                mesh.vertices.push(Vertex { position, uv });
            }
            mesh.indices.extend([start, start + 1, start + 2]);
        }
    }
    mesh
}

fn arch(center: [f32; 3], inner: f32, outer: f32, depth: f32, texture: Arc<Texture>) -> Mesh {
    let mut mesh = empty(texture);
    for i in 0..16 {
        let a = PI * i as f32 / 16.;
        let b = PI * (i + 1) as f32 / 16.;
        let point = |angle: f32, radius: f32, z: f32| {
            [
                center[0] + angle.cos() * radius,
                center[1] + angle.sin() * radius,
                center[2] + z,
            ]
        };
        let lo = -depth * 0.5;
        let hi = depth * 0.5;
        quad(
            &mut mesh,
            [
                point(a, inner, lo),
                point(b, inner, lo),
                point(b, outer, lo),
                point(a, outer, lo),
            ],
            1.,
        );
        quad(
            &mut mesh,
            [
                point(a, outer, hi),
                point(b, outer, hi),
                point(b, inner, hi),
                point(a, inner, hi),
            ],
            1.,
        );
        quad(
            &mut mesh,
            [
                point(a, outer, lo),
                point(b, outer, lo),
                point(b, outer, hi),
                point(a, outer, hi),
            ],
            1.,
        );
        quad(
            &mut mesh,
            [
                point(a, inner, hi),
                point(b, inner, hi),
                point(b, inner, lo),
                point(a, inner, lo),
            ],
            1.,
        );
        if i == 0 || i == 15 {
            let angle = if i == 0 { a } else { b };
            quad(
                &mut mesh,
                [
                    point(angle, inner, lo),
                    point(angle, outer, lo),
                    point(angle, outer, hi),
                    point(angle, inner, hi),
                ],
                1.,
            );
        }
    }
    mesh
}

fn validate(scene: &Scene) -> Result<(), String> {
    let mut vertices = 0usize;
    let mut indices = 0usize;
    for mesh in &scene.meshes {
        vertices += mesh.vertices.len();
        indices += mesh.indices.len();
        if vertices > 100_000
            || indices > 300_000
            || mesh.indices.len() % 3 != 0
            || mesh
                .indices
                .iter()
                .any(|&i| i as usize >= mesh.vertices.len())
            || mesh.vertices.iter().any(|v| {
                v.position
                    .iter()
                    .chain(&v.uv)
                    .any(|n| !n.is_finite() || n.abs() > 10_000.)
            })
        {
            return Err("Procedural Lab preview exceeds geometry limits".into());
        }
    }
    lhr_render::Camera::fit(scene)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_are_bounded_distinct_labeled_and_camera_ready() {
        let mut counts = Vec::new();
        for preset in SyntheticPreset::ALL {
            let scene = preset.build().unwrap();
            assert_eq!(scene.meshes.len(), preset.mesh_labels().len());
            assert!(scene.meshes.len() >= 9);
            let camera = lhr_render::Camera::fit(&scene).unwrap();
            assert!(camera.near > 0. && camera.far > camera.near);
            for mesh in &scene.meshes {
                assert!(!mesh.indices.is_empty());
                let texture = mesh.texture.as_ref().unwrap();
                assert_eq!(
                    texture.rgba.len(),
                    (texture.width * texture.height * 4) as usize
                );
                for triangle in mesh.indices.chunks_exact(3) {
                    let p: [glam::Vec3; 3] = std::array::from_fn(|i| {
                        glam::Vec3::from_array(mesh.vertices[triangle[i] as usize].position)
                    });
                    assert!((p[1] - p[0]).cross(p[2] - p[0]).length_squared() > 1.0e-12);
                }
            }
            counts.push((
                scene.meshes.len(),
                scene.meshes.iter().map(|m| m.vertices.len()).sum::<usize>(),
            ));
        }
        counts.sort_unstable();
        counts.dedup();
        assert_eq!(counts.len(), SyntheticPreset::ALL.len());
    }
}
