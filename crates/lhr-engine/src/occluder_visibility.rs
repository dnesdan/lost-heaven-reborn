//! Original camera shadow volumes and ProcessOccluders, PC entries
//! 0x3b130/0x3c610/0x3c070 (and inlined TestVertListW/POIS_EnumProc).
//! Volumes are per-render derived data keyed by the live canonical OccluderKey.
//! Vertices, faces, planes and adjacency come from the existing owned buffers.
use crate::frame_occluder::OccluderKey;
use crate::frame_sector::math;
use crate::frame_transform::{FrameKey, FrameKind};
use crate::model_loading::ModelArena;
use crate::portal_visibility::{portal_vector, portal_vertices};
use crate::scene_ownership::SectorKey;
use crate::scene_visibility::{
    append, distance_xyz, distance_yzx, distance_zxy, distance_zyx, number, CameraView, Plane,
    Result, SceneVisibilityError, IN_VIEW, ON,
};

#[derive(Clone)]
struct Volume {
    key: OccluderKey,
    hull: Option<SectorKey>,
    sides: Vec<Plane>,
    faces: Vec<Plane>,
    depth: Plane,
    center: [f32; 3],
    radius: f32,
    dist: f32,
    active: bool,
    portal: bool,
}
impl Volume {
    fn hides_sphere(&self, center: [f32; 3], r: f32) -> Result<bool> {
        if distance_zyx(self.depth, center)? <= r {
            for &p in &self.faces {
                if distance_zyx(p, center)? < r {
                    return Ok(false);
                }
            }
        }
        for &p in &self.sides {
            if distance_zyx(p, center)? < r {
                return Ok(false);
            }
        }
        Ok(true)
    }
    fn hides_vertices(&self, vertices: &[[f32; 3]], incoming: bool) -> Result<bool> {
        for &v in vertices {
            let depth = if incoming {
                distance_zxy(self.depth, v)?
            } else {
                distance_xyz(self.depth, v)?
            };
            if f64::from(depth) < 0.01 {
                return Ok(false);
            }
            for &p in &self.sides {
                let side = if incoming {
                    distance_zyx(p, v)?
                } else {
                    distance_yzx(p, v)?
                };
                if side < 0. {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}
pub(crate) struct SectorOcclusion {
    sector: SectorKey,
    volumes: Vec<Volume>,
}
impl SectorOcclusion {
    pub fn active_count(&self) -> usize {
        self.volumes.iter().filter(|v| v.active).count()
    }
    pub fn cull_visuals(
        &mut self,
        arena: &mut ModelArena,
        visuals: &mut Vec<FrameKey>,
        render_number: u32,
        removed: &mut usize,
    ) -> Result<()> {
        for v in &mut self.volumes {
            arena.occluder(v.key)?;
            if let Some(hull) = v.hull {
                if arena.sector(hull)?.sector_flags & 0x80 != 0 {
                    v.active = false;
                    put_flags(arena, v.key, 2, 0)?;
                }
            }
        }
        // The source's nearest-first mutual culling, preserving strict ties.
        let mut last = 0.;
        loop {
            let mut best = 1e12;
            let mut nearest = None;
            for (i, v) in self.volumes.iter().enumerate() {
                if v.active && v.dist > last && v.dist < best {
                    best = v.dist;
                    nearest = Some(i);
                }
            }
            last = best;
            let Some(i) = nearest else {
                break;
            };
            for j in 0..self.volumes.len() {
                if self.volumes[j].active && self.volumes[j].dist > best {
                    let v = &self.volumes[j];
                    if self.volumes[i].hides_sphere(v.center, v.radius)? {
                        self.volumes[j].active = false;
                        put_flags(arena, self.volumes[j].key, 2, 0)?;
                    }
                }
            }
        }
        let mut at = 0;
        while at < visuals.len() {
            let frame = visuals[at];
            if arena.get_sector(frame)? != Some(self.sector) {
                at += 1;
                continue;
            }
            let bounds = arena.hr_bound_volume_w(frame)?;
            let mut hidden = false;
            for v in &self.volumes {
                if v.active && v.hides_sphere(bounds.center, bounds.radius)? {
                    hidden = true;
                    break;
                }
            }
            if hidden {
                arena.graph.projected_state_mut(frame)?.render_number =
                    render_number.wrapping_sub(1);
                visuals.remove(at);
                *removed += 1;
            } else {
                at += 1;
            }
        }
        Ok(())
    }
}
fn descendants(arena: &ModelArena, root: FrameKey, out: &mut Vec<OccluderKey>) -> Result<()> {
    let mut at = arena.links(root)?.child;
    while let Some(frame) = at {
        at = arena.links(frame)?.next;
        match arena.state(frame)?.kind {
            FrameKind::Camera | FrameKind::Sector => continue,
            FrameKind::Occluder => append(out, arena.occluder_key(frame)?)?,
            _ => (),
        }
        descendants(arena, frame, out)?;
    }
    Ok(())
}
pub(crate) fn update_lists(
    arena: &mut ModelArena,
    scene: crate::scene_ownership::SceneKey,
    sectors: &[SectorKey],
) -> Result<()> {
    let si = arena.scene_index(scene)?;
    if !arena.scenes[si].occluders_changed {
        return Ok(());
    }
    let primary = arena.scenes[si].sectors[0];
    for sector in sectors.iter().copied().chain([primary]) {
        let index = arena.sector_index(sector)?;
        arena.sectors[index].occluder_list = None;
        let mut keys = Vec::new();
        descendants(arena, sector.frame(), &mut keys)?;
        let mut last = None;
        for key in keys {
            let oi = arena.occluder_index(key)?;
            if let Some(previous) = last {
                let pi = arena.occluder_index(previous)?;
                arena.occluders[pi].next_in_sector = Some(key);
            } else {
                arena.sectors[index].occluder_list = Some(key);
            }
            arena.occluders[oi].next_in_sector = None;
            arena.occluders[oi].put(0x5e0, 0);
            last = Some(key);
        }
    }
    arena.scenes[si].occluders_changed = false;
    Ok(())
}
fn put_plane(arena: &mut ModelArena, key: OccluderKey, at: usize, plane: Plane) -> Result<()> {
    let i = arena.occluder_index(key)?;
    for (axis, value) in plane.into_iter().enumerate() {
        arena.occluders[i].put(at + axis * 4, value.to_bits());
    }
    Ok(())
}
fn put_flags(arena: &mut ModelArena, key: OccluderKey, clear: u32, set: u32) -> Result<()> {
    let i = arena.occluder_index(key)?;
    let old = arena.occluders[i]
        .word(0x5c4)
        .ok_or(SceneVisibilityError::Unsupported("occluder flags"))?;
    arena.occluders[i].put(0x5c4, (old & !clear) | set);
    Ok(())
}
fn scalar(raw: &[u8], at: usize) -> Result<f32> {
    let bytes = raw
        .get(at..at + 4)
        .ok_or(SceneVisibilityError::Unsupported("occluder buffer extent"))?;
    number(f32::from_le_bytes(bytes.try_into().unwrap()))
}
fn vec3(raw: &[u8], index: usize) -> Result<[f32; 3]> {
    Ok([
        scalar(raw, index * 12)?,
        scalar(raw, index * 12 + 4)?,
        scalar(raw, index * 12 + 8)?,
    ])
}
fn plane(raw: &[u8], index: usize) -> Result<Plane> {
    Ok([
        scalar(raw, index * 16)?,
        scalar(raw, index * 16 + 4)?,
        scalar(raw, index * 16 + 8)?,
        scalar(raw, index * 16 + 12)?,
    ])
}
fn index(raw: &[u8], at: usize) -> Result<usize> {
    let bytes = raw
        .get(at * 2..at * 2 + 2)
        .ok_or(SceneVisibilityError::Unsupported("occluder index extent"))?;
    Ok(u16::from_le_bytes(bytes.try_into().unwrap()) as usize)
}
fn shadow_direction(mut direction: [f32; 3]) -> Result<[f32; 3]> {
    let magnitude = math::magnitude(direction)?;
    if magnitude == 0. {
        return Err(SceneVisibilityError::Unsupported(
            "zero occluder camera direction",
        ));
    }
    // Raw UpdateShadow 3b3d9..3b403 rounds the reciprocal once, then
    // multiplies each component. Three component divisions differ by an ULP.
    let reciprocal = number(crate::native_precision::div(1., magnitude))?;
    for d in &mut direction {
        *d = math::mul(*d, reciprocal)?;
    }
    Ok(direction)
}
pub(crate) fn prepare_sector(
    arena: &mut ModelArena,
    sector: SectorKey,
    view: CameraView,
) -> Result<SectorOcclusion> {
    let mut keys = Vec::new();
    let mut key = arena.sector(sector)?.occluder_list;
    while let Some(k) = key {
        if keys.contains(&k) {
            return Err(SceneVisibilityError::InvalidRing);
        }
        append(&mut keys, k)?;
        key = arena.occluder(k)?.next_in_sector;
    }
    let mut volumes = Vec::new();
    for key in keys {
        arena.update_frame(key.frame())?;
        put_flags(arena, key, 2 | 4, 0)?;
        let o = arena.occluder(key)?;
        if o.word(0x140) == Some(0) {
            continue;
        }
        let read = |at| {
            o.word(at)
                .map(f32::from_bits)
                .ok_or(SceneVisibilityError::Unsupported("occluder derived scalar"))
        };
        let center = [read(0x5cc)?, read(0x5d0)?, read(0x5d4)?];
        let radius = read(0x5d8)?;
        // The original occluder view test uses its own stored sphere only.
        let depth_distance = distance_zyx(view.planes[0], center)?;
        let oi = arena.occluder_index(key)?;
        arena.occluders[oi].put(0x5e8, depth_distance.to_bits());
        let mut in_view = math::add(depth_distance, radius)? > view.near
            && math::sub(depth_distance, radius)? < view.far;
        for &p in &view.planes[1..] {
            in_view &= distance_zyx(p, center)? > -radius;
        }
        if arena.occluder(key)?.scene.is_none() {
            let mut parent = arena.links(key.frame())?.parent;
            while let Some(p) = parent {
                if arena.state(p)?.kind == FrameKind::Sector {
                    if let Some(scene) = arena.sector(SectorKey(p))?.scene {
                        arena.occluder_set_scene_ring(key, Some(scene))?;
                    }
                    break;
                }
                parent = arena.links(p)?.parent;
            }
            continue;
        }
        if !in_view {
            continue;
        }
        arena.graph.projected_state_mut(key.frame())?.flags |= IN_VIEW;
        if arena.state(key.frame())?.flags & ON == 0 {
            continue;
        }
        if let Some(parent) = arena.links(key.frame())?.parent {
            if arena.state(parent)?.flags & ON == 0 {
                continue;
            }
        }
        let o = arena.occluder(key)?;
        let hull = o.sector;
        let buffer = |at| {
            o.buffer(at).ok_or(SceneVisibilityError::Unsupported(
                "occluder owned geometry buffer",
            ))
        };
        let vertices = buffer(0x14c)?;
        let faces = buffer(0x150)?;
        let adjacency = buffer(0x15c)?;
        let planes = buffer(0x164)?;
        let face_flags = buffer(0x168)?;
        let count = o
            .word(0x144)
            .ok_or(SceneVisibilityError::Unsupported("occluder face count"))?
            as usize;
        let mut facing = Vec::new();
        for i in 0..count {
            let facing_camera = distance_zyx(
                plane(arena.occluder_storage().bytes(planes)?, i)?,
                view.position,
            )? > 0.;
            append(&mut facing, facing_camera)?;
            let old = index(arena.occluder_storage().bytes(face_flags)?, i)? as u16;
            let new = if facing_camera { old | 1 } else { old & !1 };
            arena
                .occluder_storage
                .write(face_flags, i * 2, &new.to_le_bytes())?;
        }
        let mut sides = Vec::new();
        let mut face_planes = Vec::new();
        arena.occluders[oi].put(0x39c, 0);
        arena.occluders[oi].put(0x5b0, 0);
        let direction =
            shadow_direction([view.planes[0][0], view.planes[0][1], view.planes[0][2]])?;
        let mut depth = [direction[0], direction[1], direction[2], 0.];
        depth[3] = -distance_zyx(depth, view.position)?;
        put_plane(arena, key, 0x5b4, depth)?;
        for (face, is_facing) in facing.iter().copied().enumerate() {
            if !is_facing {
                continue;
            }
            let ids = [
                index(arena.occluder_storage().bytes(faces)?, face * 3)?,
                index(arena.occluder_storage().bytes(faces)?, face * 3 + 1)?,
                index(arena.occluder_storage().bytes(faces)?, face * 3 + 2)?,
            ];
            let v = [
                vec3(arena.occluder_storage().bytes(vertices)?, ids[0])?,
                vec3(arena.occluder_storage().bytes(vertices)?, ids[1])?,
                vec3(arena.occluder_storage().bytes(vertices)?, ids[2])?,
            ];
            let p = math::plane(v[1], v[0], v[2])?;
            put_plane(arena, key, 0x3a0 + face_planes.len() * 16, p)?;
            append(&mut face_planes, p)?;
            arena.occluders[oi].put(0x5b0, face_planes.len() as u32);
            for side in 0..3 {
                let neighbor = index(arena.occluder_storage().bytes(adjacency)?, face * 3 + side)?;
                let visible = *facing
                    .get(neighbor)
                    .ok_or(SceneVisibilityError::Unsupported("occluder adjacency face"))?;
                if !visible {
                    let edge = v[(side + 1) % 3];
                    let p = math::plane(view.position, edge, v[side])?;
                    put_plane(arena, key, 0x16c + sides.len() * 16, p)?;
                    append(&mut sides, p)?;
                    arena.occluders[oi].put(0x39c, sides.len() as u32);
                    let d = distance_zyx(depth, edge)?;
                    if d > 0. {
                        depth[3] = math::sub(depth[3], d)?;
                        arena.occluders[oi].put(0x5c0, depth[3].to_bits());
                    }
                }
            }
            // The native arrays admit the final face's extra 3 side / 1 face
            // planes, then leave this occluder inactive past the budget.
            if sides.len() > 32 || face_planes.len() > 32 {
                break;
            }
        }
        let mut portal = false;
        if let Some(hull) = hull {
            let mut front = false;
            let mut back = false;
            for &p in &arena.sector(hull)?.geometry.portals {
                let n = portal_vector(arena, p, 0xe4)?;
                let d = f32::from_bits(
                    arena
                        .portal(p)?
                        .known_word(0xf0)
                        .ok_or(SceneVisibilityError::InvalidRing)?,
                );
                if distance_zyx([n[0], n[1], n[2], d], view.position)? < 0. {
                    back = true;
                } else {
                    front = true;
                }
            }
            portal = front && back;
        }
        let active = !sides.is_empty() && sides.len() <= 32 && face_planes.len() <= 32;
        put_flags(arena, key, 0, if active { 2 } else { 0 })?;
        if portal {
            put_flags(arena, key, 0, 4)?;
        }
        append(
            &mut volumes,
            Volume {
                key,
                hull,
                sides,
                faces: face_planes,
                depth,
                center,
                radius,
                dist: distance_zyx(view.planes[0], center)?,
                active,
                portal,
            },
        )?;
    }
    // PrepareSector2 runs even with RS_USE_OCCLUSION off: the state controls
    // visual occlusion later; portal occlusion belongs to portal traversal.
    let out = arena.sector(sector)?.geometry.portals.clone();
    let incoming = arena.sector(sector)?.geometry.incoming_portals.clone();
    for (portals, incoming) in [(&out, false), (&incoming, true)] {
        for &p in portals {
            let portal = arena.portal(p)?;
            let n = portal_vector(arena, p, 0xe4)?;
            let d = f32::from_bits(
                portal
                    .known_word(0xf0)
                    .ok_or(SceneVisibilityError::InvalidRing)?,
            );
            let side = distance_zyx([n[0], n[1], n[2], d], view.position)?;
            if (!incoming && side <= 0.) || (incoming && side >= 0.) {
                continue;
            }
            let mut hidden = false;
            if !incoming || portal.twin.is_none() {
                let vertices = portal_vertices(arena, p)?;
                for v in &volumes {
                    if v.active && !v.portal && v.hides_vertices(&vertices, incoming)? {
                        hidden = true;
                        break;
                    }
                }
            }
            let i = arena.portal_index(p)?;
            let flags = arena.portal_frames[i].flags() & !2;
            arena.portal_frames[i].put(8, flags | if hidden { 2 } else { 0 });
        }
    }
    Ok(SectorOcclusion { sector, volumes })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_prepare_sector_portal_loops_keep_their_distinct_depth_associations() {
        let mut arena = ModelArena::default();
        let key = arena.create_occluder().unwrap();
        let volume = Volume {
            key,
            hull: None,
            sides: vec![],
            faces: vec![],
            depth: [2f32 / 3., 2f32 / 3., -1f32 / 3., 0.],
            center: [0.; 3],
            radius: 0.,
            dist: 1.,
            active: true,
            portal: false,
        };
        let vertices = [[25_165_824., 1.5, 50_331_648.]];
        // Body46: outgoing XYZ evaluates 0 and fails the .01 depth gate;
        // incoming ZXY evaluates 1 and reaches the (empty) side-plane loop.
        assert!(!volume.hides_vertices(&vertices, false).unwrap());
        assert!(volume.hides_vertices(&vertices, true).unwrap());
    }
    #[test]
    fn shadow_direction_uses_one_rounded_reciprocal_before_component_products() {
        let input = [0x3f42c6f2, 0x3f234fab, 0x3df39d7f].map(f32::from_bits);
        assert_eq!(
            shadow_direction(input).unwrap().map(f32::to_bits),
            [0x3f42c6f4, 0x3f234fac, 0x3df39d81]
        );
        assert_eq!(
            number(crate::native_precision::div(
                input[0],
                f32::from_bits(0x3f7fffff)
            ))
            .unwrap()
            .to_bits(),
            0x3f42c6f3
        );
        assert!(shadow_direction([0.; 3]).is_err());
    }
}
