//! Crash-model resources on the existing canonical ModelArena/CPU owner.
use super::*;
use crate::native_precision::{add, div, mul, sqrt, sub};
use crate::native_transform::SMatrix;
impl<B: VehicleComponentTailBackend> VehicleFrameBackend<'_, B> {
    pub(crate) fn crash_indexed_sound_table(&self) -> Result<Vec<u8>, VehicleFrameError<B::Error>> {
        let installation = self
            .installation
            .ok_or(VehicleFrameError::UnavailableFrameMethod(
                "Original indexed sound installation",
            ))?;
        installation
            .read_original_path("tables/sounds.dat")
            .map_err(|error| VehicleFrameError::Audio(error.to_string()))
    }

    pub(crate) fn crash_model_create(&mut self) -> Result<u32, VehicleFrameError<B::Error>> {
        let model = self.arena.create_model()?;
        self.frame_address(model.root())
    }
    pub(crate) fn crash_model_open_original(
        &mut self,
        model: u32,
        path: &[u8],
    ) -> Result<i32, VehicleFrameError<B::Error>> {
        let key = ModelKey(self.frame_key(model)?);
        let name = std::str::from_utf8(path)
            .map_err(|_| VehicleFrameError::UnavailableFrameMethod("Crash model ASCII path"))?;
        let installation = self
            .installation
            .ok_or(VehicleFrameError::UnavailableFrameMethod(
                "Crash model installation",
            ))?;
        match self
            .arena
            .open_original_static_model(key, installation, name)
        {
            Ok(_) => Ok(0),
            Err(error) => match error.error {
                ModelError::NativeModelOpenResult(code) => Ok(code as i32),
                other => Err(other.into()),
            },
        }
    }
    pub(crate) fn crash_model_duplicate_pristine(
        &mut self,
        destination: u32,
        source: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let destination = ModelKey(self.frame_key(destination)?);
        let source = ModelKey(self.frame_key(source)?);
        self.arena
            .duplicate_pristine_crash_model(destination, source)?;
        Ok(())
    }
    pub(crate) fn crash_frame_call(
        &mut self,
        frame: u32,
        slot: u32,
        args: &[u32],
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        if self.arena.state(key)?.kind == FrameKind::Model {
            match (slot, args) {
                (0x5c, [child]) => {
                    let child = self.frame_key(*child)?;
                    self.arena.delete_frame(ModelKey(key), child)?;
                    return Ok(0);
                }
                (0x44, [child]) => {
                    let child = self.frame_key(*child)?;
                    self.arena.add_frame(ModelKey(key), child)?;
                    return Ok(0);
                }
                _ => {}
            }
        }
        if let (0x20, [0]) = (slot, args) {
            self.arena.clear_crash_frame_properties(key)?;
            return Ok(0);
        }
        self.game_virtual(frame, slot, args)
    }
    pub(crate) fn crash_frame_world(
        &mut self,
        frame: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        self.arena.world_matrix(key)?;
        Ok(())
    }
    pub(crate) fn crash_frame_place_sector(
        &mut self,
        frame: u32,
    ) -> Result<(), VehicleFrameError<B::Error>> {
        let key = self.frame_key(frame)?;
        let world = self.arena.world_matrix(key)?;
        let p = [world.elements[12], world.elements[13], world.elements[14]];
        let app = self.read_word(0x63788c)?;
        let scene = self.read_word(app + 0x10)?;
        let scene = crate::scene_ownership::SceneKey(self.frame_key(scene)?);
        self.arena.set_frame_sector_pos(scene, key, p)?;
        Ok(())
    }
    pub(crate) fn crash_frame_find_named(
        &mut self,
        root: u32,
        name: &[u8],
        flags: u32,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let key = self.frame_key(root)?;
        let view = crate::actor_frames::ModelActorFrames::borrow(self.arena, true);
        let reference = view
            .reference(key)
            .map_err(|_| VehicleFrameError::UnavailableFrameMethod("Crash child root"))?;
        let found = view
            .find_child_frame(
                reference,
                name,
                flags,
                crate::frame_lookup::MAX_LOOKUP_VISITS,
            )
            .map_err(|_| VehicleFrameError::UnavailableFrameMethod("Crash child FindFrame"))?;
        found.map_or(Ok(0), |reference| {
            self.frame_address(FrameKey(reference.identity()))
        })
    }
    /// The reached flags0/lod0/no-callback hierarchy ray over the same live
    /// first-LOD plain CPU mesh buffers. No world-frame identities are read as
    /// memory and no synthetic collision result is substituted for a miss.
    pub(crate) fn crash_test_hierarchy(
        &mut self,
        point: [f32; 3],
        delta: [f32; 3],
        model: u32,
    ) -> Result<u32, VehicleFrameError<B::Error>> {
        let root = self.frame_key(model)?;
        self.arena.publish_runtime_buffers()?;
        let square = dot(delta, delta);
        let length = sqrt(square);
        let direction = normalized(delta);
        let mut pending = vec![root];
        let mut best = 1e16f32;
        let mut found = None;
        while let Some(key) = pending.pop() {
            let state = self.arena.state(key)?;
            if state.flags & 1 == 0 {
                continue;
            }
            let kind = state.kind;
            let world_space = state.visual_flags & 0x40 != 0;
            if world_space && matches!(kind, FrameKind::Mesh | FrameKind::Singlemesh) {
                return Err(VehicleFrameError::UnavailableFrameMethod(
                    "Crash hierarchy world-space vertices",
                ));
            }

            let bound = self.arena.hr_bound_volume_w(key)?;
            if bound.bbox[0][0] <= bound.bbox[1][0] {
                if !line_box(point, direction, bound.bbox) {
                    continue;
                }
            } else if kind != FrameKind::Sector {
                continue;
            }
            if kind == FrameKind::Singlemesh {
                return Err(VehicleFrameError::UnavailableFrameMethod(
                    "Crash hierarchy deformed singlemesh",
                ));
            }
            if kind == FrameKind::Mesh {
                let world = self.arena.world_matrix(key)?;
                let object = self.arena.frame_object_resources(key)?.ok_or(
                    VehicleFrameError::UnavailableFrameMethod("Crash visual resource owner"),
                )?;
                let local_bound = self
                    .arena
                    .loaded_visual_resources()
                    .ok_or(VehicleFrameError::UnavailableFrameMethod("Crash resources"))?
                    .object_bounds(object)
                    .map_err(ModelError::LoadedVisual)?;
                let center = world
                    .try_transform_point(local_bound.center)
                    .map_err(|_| VehicleFrameError::NumericDomain("Crash sphere center"))?;
                let scale = self.arena.state(key)?.world_scale;
                let radius = mul(local_bound.radius, scale);
                let projection = dot(minus(center, point), direction);
                let nearest = plus(point, times(direction, projection));
                if projection >= -radius
                    && projection <= add(length, radius)
                    && dot(minus(nearest, center), minus(nearest, center)) < mul(radius, radius)
                {
                    let (base, dir) = if world_space {
                        (point, direction)
                    } else {
                        let (mut inverse, _) = world
                            .inverse()
                            .map_err(|_| VehicleFrameError::NumericDomain("Crash inverse"))?;
                        let base = inverse
                            .try_transform_point(point)
                            .map_err(|_| VehicleFrameError::NumericDomain("Crash point"))?;
                        inverse.elements[12..15].fill(0.);
                        let dir = inverse
                            .try_transform_point(direction)
                            .map_err(|_| VehicleFrameError::NumericDomain("Crash direction"))?;
                        (base, dir)
                    };
                    let resources = self
                        .arena
                        .loaded_visual_resources()
                        .ok_or(VehicleFrameError::UnavailableFrameMethod("Crash resources"))?;
                    if let Some(view) = resources
                        .inspection_mesh(object, 65536)
                        .map_err(ModelError::LoadedVisual)?
                    {
                        let mut visual_best = 1e16f32;
                        let mut back = false;
                        for group in view.groups {
                            let two_sided = group
                                .material
                                .is_some_and(|material| material.flags & 0x10000000 != 0);
                            for face in group.index_bytes.as_chunks::<6>().0 {
                                let mut vertices = [[0.; 3]; 3];
                                for (i, vertex) in vertices.iter_mut().enumerate() {
                                    let index =
                                        u16::from_le_bytes([face[i * 2], face[i * 2 + 1]]) as usize;
                                    let bytes = view
                                        .vertex_bytes
                                        .get(index * 32..index * 32 + 12)
                                        .ok_or(VehicleFrameError::UnavailableFrameMethod(
                                            "Crash face extent",
                                        ))?;
                                    for (axis, value) in vertex.iter_mut().enumerate() {
                                        *value = f32::from_le_bytes(
                                            bytes[axis * 4..axis * 4 + 4].try_into().unwrap(),
                                        );
                                    }
                                }
                                if let Some((t, reverse)) = triangle(base, dir, vertices, two_sided)
                                {
                                    let local = minus(plus(base, times(dir, t)), base);
                                    let hit = rotate(world, local);
                                    let distance = dot(hit, hit);
                                    if distance <= square && distance < visual_best {
                                        visual_best = distance;
                                        if reverse {
                                            back = true;
                                        }
                                        // The original computes a scaled normal for each new best face.
                                        let a = times(minus(vertices[2], vertices[1]), 10000.);
                                        let b = times(minus(vertices[0], vertices[1]), 10000.);
                                        let _normal = cross(a, b);
                                    }
                                }
                            }
                        }
                        if visual_best < best {
                            best = if back {
                                add(visual_best, 0.001)
                            } else {
                                visual_best
                            };
                            found = Some(key);
                        }
                    }
                }
            }
            let mut children = Vec::new();
            let mut child = self.arena.links(key)?.child;
            while let Some(key) = child {
                children.push(key);
                child = self.arena.links(key)?.next;
            }
            pending.extend(children.into_iter().rev());
        }
        found.map_or(Ok(0), |key| self.frame_address(key))
    }
}
fn plus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| add(a[i], b[i]))
}
fn minus(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| sub(a[i], b[i]))
}
fn times(a: [f32; 3], s: f32) -> [f32; 3] {
    a.map(|x| mul(x, s))
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    add(add(mul(a[2], b[2]), mul(a[1], b[1])), mul(a[0], b[0]))
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        sub(mul(a[1], b[2]), mul(a[2], b[1])),
        sub(mul(a[2], b[0]), mul(a[0], b[2])),
        sub(mul(a[0], b[1]), mul(a[1], b[0])),
    ]
}
fn normalized(mut a: [f32; 3]) -> [f32; 3] {
    let n = dot(a, a);
    if f64::from(sub(n, 1.).abs()) < 1e-8 {
        return a;
    }
    if n >= 1e-8 {
        a = times(a, div(1., sqrt(n)))
    } else {
        let i = if a[0] != 0. {
            0
        } else if a[2] != 0. {
            2
        } else {
            1
        };
        a[i] = if a[i] >= 0. { 1. } else { -1. };
    }
    a
}
fn rotate(m: SMatrix, a: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| {
        add(
            add(mul(a[0], m.elements[i]), mul(a[1], m.elements[4 + i])),
            mul(a[2], m.elements[8 + i]),
        )
    })
}
fn triangle(base: [f32; 3], dir: [f32; 3], v: [[f32; 3]; 3], two: bool) -> Option<(f32, bool)> {
    let e1 = minus(v[1], v[0]);
    let e2 = minus(v[2], v[0]);
    let p = cross(dir, e2);
    let det = dot(e1, p);
    if if two {
        f64::from(det) > -1e-6 && f64::from(det) < 1e-6
    } else {
        f64::from(det) < 1e-6
    } {
        return None;
    }
    let inverse = div(1., det);
    let tv = minus(base, v[0]);
    let u = dot(tv, p);
    let q = cross(tv, e1);
    let vv = dot(q, dir);
    let tt = dot(e2, q);
    if two {
        let u = mul(u, inverse);
        let vv = mul(vv, inverse);
        if u < 0. || u > 1. || vv < 0. || add(u, vv) > 1. {
            return None;
        }
    } else if u < 0. || u > det || vv < 0. || add(u, vv) > det {
        return None;
    }
    if tt < 0. && !two {
        return None;
    }
    let t = mul(tt, inverse);
    if t < 0. {
        None
    } else {
        Some((t, inverse < 0.))
    }
}
fn line_box(base: [f32; 3], dir: [f32; 3], bb: [[f32; 3]; 2]) -> bool {
    if (0..3).all(|i| base[i] >= bb[0][i] && base[i] <= bb[1][i]) {
        return true;
    }
    let center = times(plus(bb[0], bb[1]), 0.5);
    let t = dot(dir, minus(center, base));
    let w = minus(center, plus(base, times(dir, t)));
    let e = times(minus(bb[1], bb[0]), 0.5);
    let a = dir.map(f32::abs);
    !(sub(mul(dir[2], w[1]), mul(w[2], dir[1])).abs() > add(mul(a[2], e[1]), mul(a[1], e[2]))
        || sub(mul(w[2], dir[0]), mul(w[0], dir[2])).abs() > add(mul(a[2], e[0]), mul(a[0], e[2]))
        || sub(mul(w[0], dir[1]), mul(dir[0], w[1])).abs() > add(mul(a[1], e[0]), mul(a[0], e[1])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    pub(super) fn mesh() -> Arc<lhr_assets::Model> {
        let mut frame = crate::model_loading::tests::frame(1, None, [0.; 3]);
        frame.properties = b";X8;".to_vec();
        frame.render_flags = 0;
        frame.logic_flags = 0;
        frame.lods = vec![lhr_assets::Lod {
            distance: 0.,
            uv_only: None,
            vertices: [[-1., -1., 0.], [0., 1., 0.], [1., -1., 0.]]
                .map(|position| lhr_assets::Vertex {
                    position,
                    normal: [0., 0., -1.],
                    uv: [0.; 2],
                })
                .to_vec(),
            groups: vec![lhr_assets::FaceGroup {
                material: 0,
                indices: vec![0, 1, 2],
            }],
        }];
        Arc::new(lhr_assets::Model {
            materials: vec![lhr_assets::Material {
                flags: 0,
                ambient: [1.; 3],
                diffuse: [1.; 3],
                texture: String::new(),
                emission: [0.; 3],
                opacity: 1.,
                alpha_texture: None,
                reflection: None,
                environment_texture: None,
                animation: None,
            }],
            frames: vec![frame],
        })
    }
    #[test]
    fn pristine_model_duplicate_shares_real_mesh_and_ray_hits_same_owned_frame() {
        let dir =
            std::env::temp_dir().join(format!("lhr-crash-model-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        let mut arena = ModelArena::default();
        let original = arena.create_model().unwrap();
        let copy = arena.create_model().unwrap();
        let content = arena.load_parsed_into_model(original, mesh()).unwrap();
        arena
            .load_plain_visual_resources(original, &installation)
            .unwrap();
        let source = content.frames()[0];
        let before = arena
            .loaded_visual_resources()
            .unwrap()
            .backend()
            .graphics_bytes();
        arena
            .duplicate_pristine_crash_model(copy, original)
            .unwrap();
        let duplicate = arena.model(copy).unwrap().frames[0];
        let a = arena.frame_object_resources(source).unwrap().unwrap();
        let b = arena.frame_object_resources(duplicate).unwrap().unwrap();
        assert_ne!(a, b);
        let resources = arena.loaded_visual_resources().unwrap();
        assert_eq!(
            resources.object_mesh(a).unwrap(),
            resources.object_mesh(b).unwrap()
        );
        assert_eq!(resources.backend().graphics_bytes(), before);
        let mut memory = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let root = backend.frame_address(copy.root()).unwrap();
        let expected = backend.frame_address(duplicate).unwrap();
        assert_eq!(
            backend
                .crash_test_hierarchy([0., 0., -1.], [0., 0., 2.], root)
                .unwrap(),
            expected
        );
        assert_eq!(
            backend
                .crash_test_hierarchy([3., 0., -1.], [0., 0., 2.], root)
                .unwrap(),
            0
        );
        backend.crash_frame_call(expected, 0x20, &[0]).unwrap();
        assert!(backend
            .arena()
            .frame(duplicate)
            .unwrap()
            .properties
            .is_empty());
        assert_eq!(
            backend.arena().frame(duplicate).unwrap().name_bytes,
            b"fixture"
        );
        backend.arena_mut().release(copy.root()).unwrap();
        backend.arena_mut().release(original.root()).unwrap();
        backend.arena_mut().reclaim_unused_visual_buffers().unwrap();
        let resources = backend.arena().loaded_visual_resources().unwrap();
        assert_eq!(resources.backend().live_graphics(), 0);
        assert_eq!(resources.backend().live_allocations(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn damaged_glass_chain_changes_live_group_material_without_copying_geometry() {
        let dir = std::env::temp_dir().join(format!("lhr-glass-chain-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("maps")).unwrap();
        let bmp = |value: u8| {
            let mut bytes = vec![0u8; 58];
            bytes[..2].copy_from_slice(b"BM");
            bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
            bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
            bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
            bytes[18..22].copy_from_slice(&1u32.to_le_bytes());
            bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
            bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
            bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
            bytes[54..57].fill(value);
            bytes
        };
        std::fs::write(dir.join("maps/glass.bmp"), bmp(21)).unwrap();
        std::fs::write(dir.join("maps/glass+.bmp"), bmp(171)).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let content = arena.load_parsed_into_model(model, mesh()).unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        arena.publish_runtime_buffers().unwrap();
        let frame = content.frames()[0];
        let object = arena.frame_object_resources(frame).unwrap().unwrap();
        let before = {
            let resources = arena.loaded_visual_resources().unwrap();
            let view = resources.inspection_mesh(object, 1).unwrap().unwrap();
            (
                view.vertex_bytes.to_vec(),
                view.groups[0].index_bytes.to_vec(),
                resources.backend().live_graphics(),
                resources.backend().graphics_bytes(),
            )
        };
        let mut memory = VehicleFrameMemory::default();
        let mut backend =
            VehicleFrameBackend::with_installation(&mut arena, &mut memory, &installation);
        let frame_address = backend.frame_address(frame).unwrap();
        // Native row+4 is MeshObject, not frame: GetLOD(0), GetFGroup(0), SetMaterial.
        let mesh = backend.deform_visual_mesh(frame_address).unwrap();
        let level = backend.game_virtual(mesh, 4, &[0]).unwrap();
        let group = backend.game_virtual(level, 0x14, &[0]).unwrap();
        let old = backend.deform_face_group_material(group).unwrap();
        backend.deform_retain_material(old).unwrap(); // original row+0 owner
        let replacement = backend.deform_create_material().unwrap(); // row+8 owner
        backend.deform_copy_material(replacement, old).unwrap();
        let texture = backend
            .arena_mut()
            .visual_resource_owner_mut()
            .unwrap()
            .game_load_glass_texture(&installation, "glass.bmp", "glass+.bmp", 4)
            .unwrap();
        let texture_address = backend.texture_address(texture).unwrap();
        backend
            .game_virtual(replacement, 0x14, &[texture_address])
            .unwrap();
        backend.game_virtual(texture_address, 0, &[]).unwrap();
        backend.game_virtual(group, 0, &[replacement]).unwrap();
        assert_eq!(
            backend.deform_face_group_material(group).unwrap(),
            replacement
        );
        let old_key = backend.material_key(old).unwrap();
        let replacement_key = backend.material_key(replacement).unwrap();
        let resources = backend.resource_owner().unwrap();
        let materials = resources.objects().managed().resources();
        assert_eq!(materials.material(old_key).unwrap().references, 1);
        assert_eq!(materials.material(replacement_key).unwrap().references, 2);
        let view = resources.inspection_mesh(object, 1).unwrap().unwrap();
        assert_eq!(view.vertex_bytes, before.0);
        assert_eq!(view.groups[0].index_bytes, before.1);
        assert_eq!(view.groups[0].texture_identity, Some(texture));
        assert_eq!(view.groups[0].texture.unwrap().rgba, [21, 21, 21, 255]);
        assert_eq!(
            view.groups[0].texture_alpha.unwrap().rgba,
            [171, 171, 171, 255]
        );
        assert_eq!(
            (
                resources.backend().live_graphics(),
                resources.backend().graphics_bytes()
            ),
            (before.2, before.3)
        );
        // Repeating SetMaterial with the same sole/group identity is a no-op.
        backend.game_virtual(group, 0, &[replacement]).unwrap();
        assert_eq!(
            backend
                .resource_owner()
                .unwrap()
                .objects()
                .managed()
                .resources()
                .material(replacement_key)
                .unwrap()
                .references,
            2
        );
        backend.arena_mut().release(model.root()).unwrap();
        assert_eq!(
            backend
                .resource_owner()
                .unwrap()
                .objects()
                .managed()
                .resources()
                .material(replacement_key)
                .unwrap()
                .references,
            1
        );
        assert!(backend.release_deform_resource(old).unwrap());
        assert!(backend.release_deform_resource(replacement).unwrap());
        backend.arena_mut().reclaim_unused_visual_buffers().unwrap();
        let resources = backend.resource_owner().unwrap();
        assert_eq!(resources.backend().live_graphics(), 0);
        assert_eq!(resources.backend().live_allocations(), 0);
        assert_eq!(resources.backend().live_textures(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn cache_duplicate_rejects_a_runtime_moved_source() {
        let mut arena = ModelArena::default();
        let original = arena.create_model().unwrap();
        let copy = arena.create_model().unwrap();
        let content = arena.load_parsed_into_model(original, mesh()).unwrap();
        arena
            .graph
            .assign_position(content.frames()[0], [1., 0., 0.])
            .unwrap();
        assert!(arena
            .duplicate_pristine_crash_model(copy, original)
            .is_err());
        assert!(arena.model(copy).unwrap().frames.is_empty());
    }
    #[test]
    fn nonidentity_hierarchy_chooses_nearest_frame_and_rejects_world_space() {
        let dir =
            std::env::temp_dir().join(format!("lhr-crash-nearest-fixture-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let installation = lhr_assets::Installation::open(&dir).unwrap();
        let mut source = mesh().as_ref().clone();
        let mut second = source.frames[0].clone();
        second.name = "far".into();
        second.name_bytes = b"far".to_vec();
        second.position = [0., 0., 1.];
        source.frames.push(second);
        let mut arena = ModelArena::default();
        let model = arena.create_model().unwrap();
        let content = arena
            .load_parsed_into_model(model, Arc::new(source))
            .unwrap();
        arena
            .load_plain_visual_resources(model, &installation)
            .unwrap();
        arena
            .graph
            .assign_position(model.root(), [7., 0., 1.])
            .unwrap();
        arena
            .graph
            .assign_scale(model.root(), [2., 3., 4.])
            .unwrap();
        let mut memory = VehicleFrameMemory::default();
        let mut backend = VehicleFrameBackend::new(&mut arena, &mut memory);
        let root = backend.frame_address(model.root()).unwrap();
        let near = backend.frame_address(content.frames()[0]).unwrap();
        let far = backend.frame_address(content.frames()[1]).unwrap();
        let nearest = backend
            .crash_test_hierarchy([7., 0., -2.], [0., 0., 10.], root)
            .unwrap();
        assert_eq!(nearest, near);
        let miss = backend
            .crash_test_hierarchy([20., 0., -2.], [0., 0., 10.], root)
            .unwrap();
        assert_eq!(miss, 0);
        let short_segment = backend
            .crash_test_hierarchy([7., 0., -2.], [0., 0., 2.], root)
            .unwrap();
        assert_eq!(short_segment, 0);
        backend
            .arena_mut()
            .set_frame_on(content.frames()[0], 0)
            .unwrap();
        let far_only = backend
            .crash_test_hierarchy([7., 0., -2.], [0., 0., 10.], root)
            .unwrap();
        assert_eq!(far_only, far);
        backend
            .arena_mut()
            .graph
            .projected_state_mut(content.frames()[1])
            .unwrap()
            .visual_flags |= 0x40;
        assert!(matches!(
            backend.crash_test_hierarchy([7., 0., -2.], [0., 0., 10.], root),
            Err(VehicleFrameError::UnavailableFrameMethod(
                "Crash hierarchy world-space vertices"
            ))
        ));
        backend.arena_mut().release(model.root()).unwrap();
        backend.arena_mut().reclaim_unused_visual_buffers().unwrap();
        let resources = backend.arena().loaded_visual_resources().unwrap();
        let graphics = resources.backend().live_graphics();
        let allocations = resources.backend().live_allocations();
        assert_eq!((graphics, allocations), (0, 0));
        // Explicit opt-in fixture output only. These are observed results from
        // the production query and live resource owner, never replay inputs.
        if let Some(path) = std::env::var_os("LHR_HIERARCHY_FIXTURE_OUTPUT") {
            let profile = if cfg!(debug_assertions) {
                "test-opt0"
            } else {
                "release-opt3"
            };
            let output = format!(
                r#"{{
  "schema": "offline-game-geometry-regression-v1",
  "scope": "cfg(test) probe of unchanged production crash_test_hierarchy; synthetic CPU mesh owners",
  "profile": "{profile}",
  "frame_identities": {{"root": {root}, "near": {near}, "far": {far}}},
  "cases": [
    {{"name": "nearest_both_on", "from": [7,0,-2], "delta": [0,0,10], "near_on": true, "result": {nearest}, "label": "near"}},
    {{"name": "lateral_miss", "from": [20,0,-2], "delta": [0,0,10], "near_on": true, "result": {miss}, "label": "null"}},
    {{"name": "segment_too_short", "from": [7,0,-2], "delta": [0,0,2], "near_on": true, "result": {short_segment}, "label": "null"}},
    {{"name": "near_off_far_hit", "from": [7,0,-2], "delta": [0,0,10], "near_on": false, "result": {far_only}, "label": "far"}}
  ],
  "world_space_40": "explicit unavailable; not a native parity case",
  "cleanup": {{"live_graphics": {graphics}, "live_cpu_allocations": {allocations}}}
}}
"#
            );
            std::fs::write(path, output).unwrap();
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod interruption_tests {
    use super::*;
    use crate::crash_actor::{CrashActorBackend, CrashActorEffect};
    use crate::crash_actors::{CrashActorsBackend, CrashPendingResource, CrashSourcePose};
    struct Owner {
        arena: ModelArena,
        memory: VehicleFrameMemory,
        pending: Vec<CrashPendingResource>,
        installation: lhr_assets::Installation,
        directory: std::path::PathBuf,
        admit_then_fail: bool,
        factory: Option<FrameKey>,
        directory_allocations: [u32; 2],
    }
    type Error = VehicleFrameError<VehicleMemoryError>;
    impl Owner {
        fn frames<T>(
            &mut self,
            f: impl FnOnce(&mut VehicleFrameBackend<'_, VehicleFrameMemory>) -> Result<T, Error>,
        ) -> Result<T, Error> {
            f(&mut VehicleFrameBackend::with_installation(
                &mut self.arena,
                &mut self.memory,
                &self.installation,
            ))
        }
        fn new(admit_then_fail: bool) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "lhr-crash-interruption-{}-{admit_then_fail}",
                std::process::id()
            ));
            std::fs::create_dir_all(&directory).unwrap();
            let installation = lhr_assets::Installation::open(&directory).unwrap();
            let mut memory = VehicleFrameMemory::default();
            memory.map_bytes(0x647dd0, &[0; 0x28]).unwrap();
            let string = memory.allocate(16).unwrap();
            memory.write_word(string, 1).unwrap();
            memory.write_word(string + 4, 7).unwrap();
            for (i, c) in b"Models\\\0".iter().copied().enumerate() {
                memory.write_byte(string + 8 + i as u32, c).unwrap();
            }
            let pointer = memory.allocate(4).unwrap();
            memory.write_word(pointer, string).unwrap();
            for (a, x) in [
                (0x647de8, pointer),
                (0x647dec, pointer + 4),
                (0x647df0, pointer + 4),
                (0x647df4, 256),
            ] {
                memory.write_word(a, x).unwrap();
            }
            Self {
                arena: ModelArena::default(),
                memory,
                pending: Vec::new(),
                installation,
                directory,
                admit_then_fail,
                factory: None,
                directory_allocations: [string, pointer],
            }
        }
    }
    impl Drop for Owner {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }
    impl VehicleInitBackend for Owner {
        type Error = Error;
        fn read_byte(&mut self, a: u32) -> Result<u8, Error> {
            self.frames(|b| b.read_byte(a))
        }
        fn read_word(&mut self, a: u32) -> Result<u32, Error> {
            self.frames(|b| b.read_word(a))
        }
        fn write_byte(&mut self, a: u32, x: u8) -> Result<(), Error> {
            self.frames(|b| b.write_byte(a, x))
        }
        fn write_word(&mut self, a: u32, x: u32) -> Result<(), Error> {
            self.frames(|b| b.write_word(a, x))
        }
        fn allocate(&mut self, n: u32) -> Result<u32, Error> {
            self.frames(|b| b.allocate(n))
        }
        fn free(&mut self, a: u32) -> Result<(), Error> {
            self.frames(|b| b.free(a))
        }
        fn collect_frame(&mut self, v: &mut VehicleInitFrameVector, f: u32) -> Result<(), Error> {
            self.frames(|b| b.collect_frame(v, f))
        }
        fn enumerate(
            &mut self,
            _: u32,
            _: VehicleInitQuery,
            _: &mut VehicleInitEnumeration<'_>,
        ) -> Result<(), VehicleInitUnavailable<Error>> {
            Err(VehicleInitUnavailable::Effect(
                VehicleFrameError::UnavailableFrameMethod("test enumeration"),
            ))
        }
    }
    impl CrashActorBackend for Owner {
        fn crash_actor_effect(&mut self, _: CrashActorEffect) -> Result<u32, Error> {
            Err(VehicleFrameError::UnavailableFrameMethod(
                "unexpected actor effect",
            ))
        }
    }
    impl CrashActorsBackend for Owner {
        fn crash_track(&mut self, r: CrashPendingResource) {
            self.pending.push(r)
        }
        fn crash_untrack(&mut self, r: CrashPendingResource) -> Result<(), Error> {
            let index = self.pending.iter().rposition(|p| *p == r).ok_or(
                VehicleFrameError::UnavailableFrameMethod("test pending owner"),
            )?;
            self.pending.remove(index);
            Ok(())
        }
        fn crash_pending_last(&self) -> Option<CrashPendingResource> {
            self.pending.last().copied()
        }
        fn crash_observe_piece(
            &mut self,
            _: u32,
            _: u32,
            _: u32,
            _: [u32; 5],
        ) -> Result<(), Error> {
            Err(VehicleFrameError::UnavailableFrameMethod(
                "unexpected observation",
            ))
        }
        fn crash_source_pose(&mut self, _: u32) -> Result<CrashSourcePose, Error> {
            Err(VehicleFrameError::UnavailableFrameMethod(
                "unexpected source pose",
            ))
        }
        fn crash_new_model(&mut self) -> Result<u32, Error> {
            let address = self.frames(|b| b.crash_model_create())?;
            self.factory = Some(self.frames(|b| b.frame_key(address))?);
            Ok(address)
        }
        fn crash_model_open(&mut self, model: u32, path: &[u8]) -> Result<i32, Error> {
            if self.admit_then_fail {
                let key = ModelKey(self.frames(|b| b.frame_key(model))?);
                let source = super::tests::mesh();
                self.arena
                    .load_parsed_into_model(key, source)
                    .map_err(|e| Error::from(e.error))?;
                self.arena
                    .load_plain_visual_resources(key, &self.installation)?;
                Err(VehicleFrameError::UnavailableFrameMethod(
                    "source unavailable after real mesh admission",
                ))
            } else {
                self.frames(|b| b.crash_model_open_original(model, path))
            }
        }
        fn crash_model_duplicate(&mut self, d: u32, s: u32) -> Result<(), Error> {
            self.frames(|b| b.crash_model_duplicate_pristine(d, s))
        }
        fn crash_frame_virtual(&mut self, f: u32, s: u32, a: &[u32]) -> Result<u32, Error> {
            self.frames(|b| b.crash_frame_call(f, s, a))
        }
        fn crash_update_world(&mut self, f: u32) -> Result<(), Error> {
            self.frames(|b| b.crash_frame_world(f))
        }
        fn crash_place_frame_sector(&mut self, f: u32) -> Result<(), Error> {
            self.frames(|b| b.crash_frame_place_sector(f))
        }
        fn crash_test_col_hierarchy(
            &mut self,
            p: [f32; 3],
            d: [f32; 3],
            m: u32,
        ) -> Result<u32, Error> {
            self.frames(|b| b.crash_test_hierarchy(p, d, m))
        }
        fn crash_allocate_actor(&mut self) -> Result<u32, Error> {
            Err(VehicleFrameError::UnavailableFrameMethod(
                "unexpected actor allocation",
            ))
        }
        fn crash_allocator_marker(&self) -> u8 {
            0
        }
        fn crash_actor_kick(&mut self, _: u32) -> Result<(), Error> {
            Err(VehicleFrameError::UnavailableFrameMethod(
                "unexpected actor kick",
            ))
        }
    }
    #[test]
    fn missing_or_interrupted_model_intake_keeps_owner_until_explicit_close() {
        for admit in [false, true] {
            let mut owner = Owner::new(admit);
            let call = crate::vehicle_car_contacts::VehicleCreateCrashActorsCall {
                source_frame: 0xd0001000,
                point: [0.; 3],
                velocity: [0., 0., 1.],
                strength: 1.,
                filename: b"missing.i3d".to_vec(),
                change_filename: true,
                byte_argument: 0,
                ancestor_depth: 0,
                recursive_flag: false,
                actor_mask: 1,
            };
            assert!(crate::crash_actors::create_crash_actors(&mut owner, &call).is_err());
            assert_eq!(owner.pending.len(), 1);
            let factory = owner.factory.unwrap();
            assert!(owner.arena.frame(factory).is_ok());
            if admit {
                assert!(
                    owner
                        .arena
                        .loaded_visual_resources()
                        .unwrap()
                        .backend()
                        .live_graphics()
                        > 0
                );
            }
            crate::crash_actors::cleanup_crash_pending(&mut owner).unwrap();
            assert!(owner.pending.is_empty());
            assert!(owner.arena.frame(factory).is_err());
            crate::crash_actors::cleanup_crash_model_cache(&mut owner).unwrap();
            owner.arena.reclaim_unused_visual_buffers().unwrap();
            if let Some(resources) = owner.arena.loaded_visual_resources() {
                assert_eq!(resources.backend().live_graphics(), 0);
                assert_eq!(resources.backend().live_allocations(), 0);
            }
            for p in owner.directory_allocations {
                owner.memory.free(p).unwrap();
            }
            assert_eq!(owner.memory.allocations().count(), 0);
        }
    }
}
