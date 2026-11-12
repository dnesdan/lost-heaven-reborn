//! Original camera shadow volumes and ProcessOccluders, PC entries
//! 0x3b130/0x3c610/0x3c070 (TestVertListW stays inlined via hides_sphere/hides_vertices).
//! LS3DF.dll SHA-256 e211c85009440d6c032f2074d79184f074a9a046574648745c2594b4cef178a6:
//! RVA 0x3c2e0 CullOcclsEachOther and RVA 0x3c480 POIS_EnumProc are free functions below.
//! Pinned mafia-re 113b5b6 ls3df/I3D/frames/I3d_occluder.cpp:460 and :518; I3D.h:2417 and :2421
//! FUNCTION markers. Both names are PDB procedure names (I3D.h:2414-2416 records no public
//! symbol); evidenced procedure names, not public symbols. Tier this wave: differential
//! (orchestrator REA-verified formed Ghidra bodies FUN_1003c2e0 25BB/411B and FUN_1003c480 25BB/386B).
//! Float policy: f32 exact through Volume::hides_sphere (distance_zyx Z,Y,X order via
//! frame_sector::math add/mul with native_precision single rounding, no fused multiply-add,
//! no f64 substitution); strict f32 comparisons as spelled; non-finite inputs error through the
//! number/math guards and never cull. Accepted deviation: the source spells victim sums as
//! d+X+Y+Z while the shared helper sums Z+Y+X+d (1-ulp edge risk, shared predates this wave).
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

/// I3D_occluder::occl_flags and face-flags words (mafia-re ls3df/I3D/I3D.h:2349
/// at pin 113b5b6; the member names are guessed there). Upstream working names,
/// not evidenced original symbols: the original exposes no named accessors,
/// and UpdateShadow plus the portal walk test the bits in place. The host
/// projection folds OCCLF_ACTIVE onto arena frame-flag bit 2 (cleared with
/// `put_flags` below when a victim is hidden).
/// OCCLF_ACTIVE 0x2: UpdateShadow made its volume this frame, and nothing hides it.
#[cfg(test)] // Test-only bit readers; production tracks the active state as a bool.
pub const OCCLF_ACTIVE: u32 = 0x2;
/// OCCLF_PORTAL 0x4: the camera is on both sides of its sector's portals.
#[cfg(test)] // Test-only bit readers; production tracks the active state as a bool.
pub const OCCLF_PORTAL: u32 = 0x4;
/// OCCLFF_FACING 0x1: the face faces the camera (UpdateShadow).
#[cfg(test)] // Test-only bit readers; production writes the face-flag word directly.
pub const OCCLFF_FACING: u32 = 0x1;

/// Upstream working name, not an evidenced original symbol: direct read of
/// the OCCLF_ACTIVE bit (I3D.h:2350). No named original accessor.
#[cfg(test)] // Test-only bit readers; production tracks the active state as a bool.
pub fn occluder_shadow_active(flags: u32) -> bool {
    flags & OCCLF_ACTIVE != 0
}

/// Upstream working name, not an evidenced original symbol: direct read of
/// the OCCLFF_FACING bit (I3D.h:2356). No named original accessor.
#[cfg(test)] // Test-only bit readers; production writes the face-flag word directly.
pub fn occluder_face_facing(face_flags: u32) -> bool {
    face_flags & OCCLFF_FACING != 0
}

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
        cull_occluders_each_other(&mut *self, arena)?;
        let mut at = 0;
        while at < visuals.len() {
            let frame = visuals[at];
            if arena.get_sector(frame)? != Some(self.sector) {
                at += 1;
                continue;
            }
            // Restored evidenced guard: the original skips visuals whose render
            // number is not the current one before touching bound data. The prior
            // Rust walk assumed every listed visual was current.
            let current = arena.state(frame)?.render_number;
            if current != render_number {
                at += 1;
                continue;
            }
            // Bound data is read only on the current path (source reads
            // GetHRBoundVolumeW inside the ACTIVE branch); all-inactive
            // volumes hide nothing, so the read is skipped with them.
            if !self.volumes.iter().any(|v| v.active) {
                at += 1;
                continue;
            }
            let bounds = arena.hr_bound_volume_w(frame)?;
            // The arena adapter walks the visual chain in order and delegates each
            // visual's occluder walk to pois_enum_proc. Per-visual delegation keeps
            // the live bound-volume read timing and the partial-prefix error behavior
            // of the previous loop while leaving the hide/rollback decision in one owner.
            let mut chain = [PoisVisual {
                render_number: current,
                center: bounds.center,
                radius: bounds.radius,
            }];
            if pois_enum_proc(&*self, &mut chain[..], render_number)? == 1 {
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
/// Typed visual-chain entry for pois_enum_proc: the live render number plus the
/// world-space bound sphere the caller read (original m_RenderNumber and
/// GetHRBoundVolumeW). The caller owns sector filtering and live arena reads and
/// writes; this walk owns the hide and rollback decision.
pub(crate) struct PoisVisual {
    pub render_number: u32,
    pub center: [f32; 3],
    pub radius: f32,
}

/// LS3DF RVA 0x3c2e0 CullOcclsEachOther (I3D.h:2417 FUNCTION marker; mafia-re 113b5b6
/// ls3df/I3D/frames/I3d_occluder.cpp:460, nearest-depth mutual culling loop). The name is a
/// PDB procedure name (I3D.h:2414-2416 records no public symbol); evidenced procedure name,
/// not a public symbol. Tier this wave: differential (orchestrator REA-verified
/// formed Ghidra body FUN_1003c2e0 25BB/411B). This function owns 0x3c2e0 only; POIS_EnumProc
/// lives in pois_enum_proc below.
/// Order-exact contract: last starts at 0, best at 1e12, strict dist greater than last and
/// less than best selects the nearest, last becomes best, then every still-active volume with
/// dist greater than best is tested against the nearest through Volume::hides_sphere; hidden
/// volumes clear active and OCCLF_ACTIVE bit 2. Strict ties never hide each other. Float
/// policy: f32 exact, single rounding, no fused multiply-add, no f64 substitution; non-finite
/// inputs error through hides_sphere and abort with the reached prefix retained, never treated
/// as visible.
pub(crate) fn cull_occluders_each_other(
    occlusion: &mut SectorOcclusion,
    arena: &mut ModelArena,
) -> Result<()> {
    let mut last = 0.0f32;
    loop {
        let mut best = 1e12f32;
        let mut nearest = None;
        for (i, v) in occlusion.volumes.iter().enumerate() {
            if v.active && v.dist > last && v.dist < best {
                best = v.dist;
                nearest = Some(i);
            }
        }
        last = best;
        let Some(i) = nearest else {
            break;
        };
        for j in 0..occlusion.volumes.len() {
            if occlusion.volumes[j].active && occlusion.volumes[j].dist > best {
                let center = occlusion.volumes[j].center;
                let radius = occlusion.volumes[j].radius;
                if occlusion.volumes[i].hides_sphere(center, radius)? {
                    occlusion.volumes[j].active = false;
                    let key = occlusion.volumes[j].key;
                    put_flags(arena, key, 2, 0)?;
                }
            }
        }
    }
    Ok(())
}

/// LS3DF RVA 0x3c480 POIS_EnumProc (I3D.h:2421 FUNCTION marker; mafia-re 113b5b6
/// ls3df/I3D/frames/I3d_occluder.cpp:518, visual-chain occlusion walk with RenderNumber
/// rollback). The name is a PDB procedure name (I3D.h:2414-2416 records no public symbol);
/// evidenced procedure name, not a public symbol. Tier this wave: differential
/// (orchestrator REA-verified formed Ghidra body FUN_1003c480 25BB/386B).
/// Order-exact contract: derived-list early exit when the sector holds no
/// volumes (observable-equivalent to the original null-occluder return:
/// an all-inactive list hides nothing and the chain carries no writes on
/// the visible path); per visual in chain order, rollback is read before the
/// equality guard exactly as the original keeps rn in a register, visuals whose render number
/// differs are skipped without touching sphere data, otherwise each ACTIVE volume is tested
/// through Volume::hides_sphere with an early break once hidden, mirroring the original vis
/// flag and its break; PORTAL is ignored because the source tests ACTIVE only. Hidden visuals
/// store rollback and count once. Float policy: f32 exact as above; numeric errors abort with
/// the reached rollback prefix retained.
pub(crate) fn pois_enum_proc(
    occlusion: &SectorOcclusion,
    chain: &mut [PoisVisual],
    render_number: u32,
) -> Result<usize> {
    if occlusion.volumes.is_empty() {
        return Ok(0);
    }
    let mut hidden = 0;
    for entry in chain.iter_mut() {
        let rollback = render_number.wrapping_sub(1);
        if entry.render_number != render_number {
            continue;
        }
        let mut visible = true;
        for volume in &occlusion.volumes {
            if !visible {
                break;
            }
            if volume.active && volume.hides_sphere(entry.center, entry.radius)? {
                visible = false;
            }
        }
        if !visible {
            entry.render_number = rollback;
            hidden += 1;
        }
    }
    Ok(hidden)
}
pub(crate) fn descendants(
    arena: &ModelArena,
    root: FrameKey,
    out: &mut Vec<OccluderKey>,
) -> Result<()> {
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
    fn shadow_active_reader_reports_only_occlf_active_bit() {
        assert_eq!((OCCLF_ACTIVE, OCCLF_PORTAL, OCCLFF_FACING), (0x2, 0x4, 0x1));
        assert!(occluder_shadow_active(OCCLF_ACTIVE));
        assert!(occluder_shadow_active(OCCLF_ACTIVE | OCCLF_PORTAL));
        assert!(occluder_shadow_active(u32::MAX));
        assert!(!occluder_shadow_active(0));
        assert!(!occluder_shadow_active(OCCLF_PORTAL));
    }
    #[test]
    fn face_facing_reader_reports_only_facing_bit() {
        assert!(occluder_face_facing(OCCLFF_FACING));
        assert!(occluder_face_facing(u32::MAX));
        assert!(!occluder_face_facing(0));
        assert!(!occluder_face_facing(0x2));
    }
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
    #[test]
    fn cull_occluders_each_other_takes_nearest_first_and_preserves_strict_ties() {
        // Hand-derived vectors; executed green (lib suite).
        let mut arena = ModelArena::default();
        let near_key = arena.create_occluder().unwrap();
        let mid_key = arena.create_occluder().unwrap();
        let far_key = arena.create_occluder().unwrap();
        // Seed the OCCLF_ACTIVE bit (I3D.h:2350, bit 2) so the victim clear
        // below pins a real 1 -> 0 transition, not a 0 -> 0 no-op.
        for key in [near_key, mid_key, far_key] {
            put_flags(&mut arena, key, 0, 2).unwrap();
        }
        let mut occlusion = SectorOcclusion {
            sector: SectorKey(FrameKey(u32::MAX)),
            volumes: vec![
                Volume {
                    key: near_key,
                    hull: None,
                    sides: vec![[1.0f32, 0.0, 0.0, 10.0]],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, -10.0],
                    center: [0.; 3],
                    radius: 0.5,
                    dist: 1.0,
                    active: true,
                    portal: false,
                },
                Volume {
                    key: mid_key,
                    hull: None,
                    sides: vec![],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.0, 0.0, 5.0],
                    radius: 1.0,
                    dist: 2.0,
                    active: true,
                    portal: false,
                },
                Volume {
                    key: far_key,
                    hull: None,
                    sides: vec![],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.0, 0.0, 6.0],
                    radius: 1.0,
                    dist: 3.0,
                    active: true,
                    portal: false,
                },
            ],
        };
        cull_occluders_each_other(&mut occlusion, &mut arena).unwrap();
        assert!(occlusion.volumes[0].active);
        assert!(!occlusion.volumes[1].active);
        assert!(!occlusion.volumes[2].active);
        // The survivor keeps OCCLF_ACTIVE; victims lose it in the arena word.
        assert_ne!(
            arena.occluder(near_key).unwrap().word(0x5c4).unwrap() & 2,
            0
        );
        assert_eq!(arena.occluder(mid_key).unwrap().word(0x5c4).unwrap() & 2, 0);
        assert_eq!(arena.occluder(far_key).unwrap().word(0x5c4).unwrap() & 2, 0);
        // Strict ties: equal depths never hide each other because both the
        // nearest select (dist less than best) and the victim scan (dist greater
        // than best) are strict. Each volume below would hide the other if compared.
        let tie_a = arena.create_occluder().unwrap();
        let tie_b = arena.create_occluder().unwrap();
        let mut ties = SectorOcclusion {
            sector: SectorKey(FrameKey(u32::MAX)),
            volumes: vec![
                Volume {
                    key: tie_a,
                    hull: None,
                    sides: vec![[1.0f32, 0.0, 0.0, 10.0]],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.; 3],
                    radius: 1.0,
                    dist: 5.0,
                    active: true,
                    portal: false,
                },
                Volume {
                    key: tie_b,
                    hull: None,
                    sides: vec![[1.0f32, 0.0, 0.0, 10.0]],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.; 3],
                    radius: 1.0,
                    dist: 5.0,
                    active: true,
                    portal: false,
                },
            ],
        };
        cull_occluders_each_other(&mut ties, &mut arena).unwrap();
        assert!(ties.volumes[0].active);
        assert!(ties.volumes[1].active);
        // Ties hide nothing, so both keep the seeded bit.
        for key in [tie_a, tie_b] {
            put_flags(&mut arena, key, 0, 2).unwrap();
        }
        cull_occluders_each_other(&mut ties, &mut arena).unwrap();
        assert_ne!(arena.occluder(tie_a).unwrap().word(0x5c4).unwrap() & 2, 0);
        assert_ne!(arena.occluder(tie_b).unwrap().word(0x5c4).unwrap() & 2, 0);
    }
    #[test]
    fn pois_enum_proc_rolls_back_current_hidden_visuals_only() {
        // Hand-derived vectors; executed green (lib suite).
        // Render number 0 exercises the wrapping rollback to u32::MAX.
        let mut arena = ModelArena::default();
        let hiding = arena.create_occluder().unwrap();
        let idle = arena.create_occluder().unwrap();
        let portal_key = arena.create_occluder().unwrap();
        let occlusion = SectorOcclusion {
            sector: SectorKey(FrameKey(u32::MAX)),
            volumes: vec![
                Volume {
                    key: hiding,
                    hull: None,
                    sides: vec![[1.0f32, 0.0, 0.0, 1.0]],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.; 3],
                    radius: 1.0,
                    dist: 1.0,
                    active: true,
                    portal: false,
                },
                Volume {
                    key: idle,
                    hull: None,
                    sides: vec![[1.0f32, 0.0, 0.0, 10.0]],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.; 3],
                    radius: 1.0,
                    dist: 2.0,
                    active: false,
                    portal: false,
                },
                // PORTAL is not tested (source tests ACTIVE only): an active
                // portal volume hides like any active one. Its planes hide
                // the last chain entry alone, so it must roll back too.
                // Side offset 1.4 threads the gap: x=-1 entries escape
                // (0.4 < 0.5) while x=-0.75 is hidden (0.65 >= 0.5); the
                // `hiding` volume (offset 1.0) leaves x=-0.75 alone (0.25).
                Volume {
                    key: portal_key,
                    hull: None,
                    sides: vec![[1.0f32, 0.0, 0.0, 1.4]],
                    faces: vec![],
                    depth: [0f32, 0.0, 1.0, 0.0],
                    center: [0.; 3],
                    radius: 1.0,
                    dist: 3.0,
                    active: true,
                    portal: true,
                },
            ],
        };
        let render_number = 0u32;
        let mut chain = vec![
            PoisVisual {
                render_number,
                center: [0.0, 0.0, 0.0],
                radius: 0.5,
            },
            PoisVisual {
                render_number,
                center: [-1.0, 0.0, 0.0],
                radius: 0.5,
            },
            PoisVisual {
                render_number: 5,
                center: [0.0, 0.0, 0.0],
                radius: 0.5,
            },
            PoisVisual {
                render_number,
                center: [-1.0, 0.0, 0.0],
                radius: 0.5,
            },
            // Hidden by the portal volume alone (see above); the untested
            // PORTAL flag means it hides like any active volume.
            PoisVisual {
                render_number,
                center: [-0.75, 0.0, 0.0],
                radius: 0.5,
            },
        ];
        let hidden = pois_enum_proc(&occlusion, &mut chain, render_number).unwrap();
        assert_eq!(hidden, 2);
        assert_eq!(chain[0].render_number, u32::MAX);
        assert_eq!(chain[1].render_number, render_number);
        assert_eq!(chain[2].render_number, 5);
        // The index-3 entry would be hidden by the idle volume's planes
        // absent the ACTIVE-only guard (offset 10.0 passes at distance 9.0),
        // so the guard leaves it visible; without the guard it would roll back too.
        assert_eq!(chain[3].render_number, render_number);
        // The portal-hidden entry rolls back: the untested PORTAL flag never
        // excludes an active volume.
        assert_eq!(chain[4].render_number, u32::MAX);
        // Empty sectors return early with no rollback, mirroring the null-list guard.
        let empty = SectorOcclusion {
            sector: SectorKey(FrameKey(u32::MAX)),
            volumes: vec![],
        };
        let mut solo = vec![PoisVisual {
            render_number,
            center: [0.0, 0.0, 0.0],
            radius: 0.5,
        }];
        assert_eq!(pois_enum_proc(&empty, &mut solo, render_number).unwrap(), 0);
        assert_eq!(solo[0].render_number, render_number);
    }
}
